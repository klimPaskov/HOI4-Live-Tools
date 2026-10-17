"""Standing orders: decisions the agent declares once and the clock engine executes when the situation they cover
comes up, so game time never waits for the next agent turn.

A rule says WHEN (stop-condition codes of hoi4_advance: tech_done, research_idle, construction_done,
production_idle, focus_done, pp_high, ...) and WHAT (tool calls, in the format of hoi4_scenario steps):

    {"name": "research", "when": ["research_idle"], "queue": [
        {"tool": "hoi4_research", "args": {"action": "start", "tech": "basic_machine_tools"}},
        {"tool": "hoi4_research", "args": {"action": "start", "tech": "construction1"}}]}

`do` runs every time the rule fires; `queue` is a list of calls (an entry may itself be a list of calls) of which
each firing runs the next one, so the agent writes its priorities once. When a queue is used up the run stops with
the reason `standing_queue_empty`, so the agent refills it (the engine decides nothing). The macros do what the
agent decided, nothing else: a failed call stops the run with `standing_failed` and the error, and the rule is
kept for the next agent action. `min_gap_days` stops a rule from firing again too soon; `max_fires` ends it.

The hook is called by the fast clock (game/clock.py) with the reasons a day produced and returns the reasons the
engine did not handle, which end the run as usual.
"""

import json
import os
import time

from ..errors import ToolError

# stop conditions a rule may wait for (the codes of advance.STOP_CONDITIONS plus research_idle)
WHEN = ("tech_done", "research_idle", "focus_done", "focus_idle", "construction_done", "production_idle", "war_started",
        "war_ended", "territory_lost", "territory_gained", "losing", "low_stability", "pp_high", "low_fuel",
        "front_changed")


class StandingOrders:
    def __init__(self, path=None):
        self.rules = {}
        self.path = path
        if path:
            self._load()

    def active(self):
        return bool(self.rules)

    # ------------------------------------------------------------------ persistence (a server restart keeps them)
    def _load(self):
        try:
            with open(self.path, encoding="utf-8") as f:
                data = json.load(f)
            if isinstance(data, dict):
                self.rules = {k: v for k, v in data.items() if isinstance(v, dict) and "when" in v}
        except (OSError, ValueError):
            self.rules = {}

    def save(self):
        if not self.path:
            return
        try:
            os.makedirs(os.path.dirname(self.path), exist_ok=True)
            tmp = self.path + ".tmp"
            with open(tmp, "w", encoding="utf-8") as f:
                json.dump(self.rules, f, indent=1)
            os.replace(tmp, self.path)
        except OSError:
            pass

    # ------------------------------------------------------------------ editing
    @staticmethod
    def _calls(item, where):
        """One queue entry or `do` value -> a list of {tool, args}."""
        calls = item if isinstance(item, list) else [item]
        out = []
        for c in calls:
            if not isinstance(c, dict) or not isinstance(c.get("tool"), str):
                raise ToolError(f"{where}: every call needs a tool name", {"field": where})
            out.append({"tool": c["tool"], "args": dict(c.get("args") or {})})
        if not out:
            raise ToolError(f"{where}: empty call list", {"field": where})
        return out

    def set(self, name, when, do=None, queue=None, pause=False, max_fires=None, min_gap_days=0, replace=True,
            on_fail="stop"):
        if not name:
            raise ToolError("a standing order needs a name", {"field": "name"})
        if name in self.rules and not replace:
            raise ToolError(f"standing order '{name}' exists", {"field": "name"})
        bad = [w for w in when or [] if w not in WHEN]
        if bad or not when:
            raise ToolError(f"'when' needs one or more of: {', '.join(WHEN)}", {"field": "when", "unknown": bad})
        if (do is None) == (queue is None):
            raise ToolError("give either do (run every time) or queue (one entry per firing)", {"field": "do"})
        if on_fail not in ("stop", "skip"):
            raise ToolError("on_fail is stop or skip", {"field": "on_fail"})
        rule = {"name": name, "when": list(when), "pause": bool(pause), "max_fires": max_fires, "on_fail": on_fail,
                "failed": [],
                "min_gap_days": int(min_gap_days or 0), "fired": 0, "last_day": None, "last": None}
        if do is not None:
            rule["do"] = self._calls(do, "do")
        else:
            rule["queue"] = [self._calls(q, f"queue[{i}]") for i, q in enumerate(queue)]
        for call in (rule.get("do") or []) + [c for q in rule.get("queue", []) for c in q]:
            from ..tools import registry
            tool = registry.get(call["tool"])
            if tool is None:
                raise ToolError(f"unknown tool '{call['tool']}' in standing order '{name}'", {"field": "do"})
            if tool.test_only:
                raise ToolError(f"{call['tool']} is a test-mode tool and cannot be a standing order",
                                {"field": "do"})
            if call["tool"] in ("hoi4_advance", "hoi4_clock", "hoi4_scenario"):
                raise ToolError(f"{call['tool']} cannot run inside the clock", {"field": "do"})
        self.rules[name] = rule
        self.save()
        return rule

    def extend(self, name, queue):
        """Append queue entries to a rule (it may tell the agent again when it runs dry)."""
        rule = self.rules.get(name)
        if not rule or "queue" not in rule:
            raise ToolError(f"no queue rule '{name}' to extend", {"field": "name"})
        rule["queue"].extend(self._calls(q, f"queue[{i}]") for i, q in enumerate(queue))
        rule["empty_told"] = False
        self.save()
        return rule

    def remove(self, name):
        out = self.rules.pop(name, None)
        self.save()
        return out

    def clear(self):
        n = len(self.rules)
        self.rules.clear()
        self.save()
        return n

    def listing(self):
        out = []
        for r in self.rules.values():
            d = {k: r.get(k) for k in ("name", "when", "pause", "max_fires", "min_gap_days", "fired", "last_day", "last",
                                       "on_fail", "failed")}
            if "queue" in r:
                d["queue_left"] = len(r["queue"])
                d["next"] = r["queue"][0] if r["queue"] else None
            else:
                d["do"] = r["do"]
            out.append(d)
        return out

    # ------------------------------------------------------------------ execution
    def on_reasons(self, run, reasons):
        """Called by the clock with the reasons of a day. Returns the reasons left for the agent."""
        from ..game import clock as clock_mod
        s = run.s
        left = []
        paused = False
        fired_now = set()                       # one firing per rule per batch (focus_done and focus_idle together)
        for reason in reasons:
            handled = False
            for rule in [r for r in self.rules.values() if reason["code"] in r["when"]]:
                if rule["name"] in fired_now:
                    handled = True
                    continue
                fired_now.add(rule["name"])
                if rule["max_fires"] is not None and rule["fired"] >= rule["max_fires"]:
                    continue
                day = (run.progress or {}).get("n")          # the absolute day counter (runs restart "days")
                if (rule["min_gap_days"] and rule["last_day"] is not None and day is not None
                        and day - rule["last_day"] < rule["min_gap_days"]):
                    handled = True              # too soon: the situation is known, do not bother the agent
                    continue
                if rule["pause"] and not paused and not ("queue" in rule and not rule["queue"]):
                    with run.mutex:
                        clock_mod.set_running(s, False)
                    paused = True
                for _ in range(8):              # entries that are moot (skip) are passed over to the next one
                    status, extra = self._fire(run, rule, reason)
                    if status != "skipped":
                        break
                handled = True
                if extra:
                    left.append(extra)
            if not handled:
                left.append(reason)
        self.save()
        if paused and not left:
            with run.mutex:
                clock_mod.set_running(s, True)
        return left

    def _fire(self, run, rule, reason):
        """Run one firing of a rule. Returns (status, reason to hand to the agent or None); status is done,
        deferred, skipped, failed or empty."""
        from . import focustimer
        from ..tools import dispatch
        s = run.s
        date = (run.progress or {}).get("date")
        if "queue" in rule:
            if not rule["queue"]:
                if rule.get("empty_told"):
                    return "empty", None
                rule["empty_told"] = True        # once per refill: an unfilled queue must not stop every day
                return "empty", {"code": "standing_queue_empty",
                                 "text": f"standing order '{rule['name']}' has no entries left "
                                         f"(waiting for {reason['code']}: {reason['text']})"}
            calls = rule["queue"].pop(0)
        else:
            calls = rule["do"]
        outcome = {"rule": rule["name"], "because": reason["code"], "date": date, "calls": []}
        failed = None
        deferred = skipped = False
        for call in calls:
            t0 = time.time()
            mcp = dispatch.call(s, call["tool"], call["args"])
            text = ""
            for part in mcp.get("content", []):
                if part.get("type") == "text":
                    text = part["text"].splitlines()[0] if part["text"] else ""
                    break
            sc = mcp.get("structuredContent") or {}
            outcome["calls"].append({"tool": call["tool"], "args": call["args"], "ok": not mcp.get("isError"),
                                     "summary": text[:200], "seconds": round(time.time() - t0, 1)})
            if mcp.get("isError"):
                if "event window" in text or "no idle research slot" in text:
                    deferred = True              # an event waits for an answer: the entry must not be burned
                    break
                failed = f"{call['tool']}: {text}"
                break
            if sc.get("deferred"):
                deferred = True                  # the call chose to wait (a focus still runs): try again later
                break
            if sc.get("skip"):
                skipped = True                   # nothing to do (already researched): pass to the next entry
                break
        if skipped:
            outcome["skipped"] = True
            run.run["auto"].append(outcome)
            return "skipped", None
        if deferred:
            if "queue" in rule:
                rule["queue"].insert(0, calls)
            focustimer.rearm(s, 3)               # ask again in three days
            outcome["deferred"] = True
            run.run["auto"].append(outcome)
            return "deferred", None
        rule["fired"] += 1
        rule["last_day"] = (run.progress or {}).get("n")
        rule["last"] = {"date": date, "ok": failed is None,
                        "summary": outcome["calls"][-1]["summary"] if outcome["calls"] else ""}
        run.run["auto"].append(outcome)
        if failed:
            if rule.get("on_fail") == "skip" and "queue" in rule:
                # drop the entry (kept in `failed` for the report) and let the loop try the next one
                rule.setdefault("failed", []).append({"call": calls[0], "error": failed[:200], "date": date})
                del rule["failed"][:-20]
                return "skipped", None
            if "queue" in rule:
                rule["queue"].insert(0, calls)   # try the same entry again next time
            return "failed", {"code": "standing_failed",
                              "text": f"standing order '{rule['name']}' failed on {reason['code']}: {failed}"}
        return "done", None


def get(session):
    """The session's standing orders, loaded from artifacts/state/standing.json on first use."""
    if getattr(session, "standing", None) is None:
        session.standing = StandingOrders(os.path.join(session.cfg.artifacts_dir, "state", "standing.json"))
    return session.standing


def hook(session):
    """The clock's on_reasons callback: the session's standing orders, looked up on every call so orders declared
    while a background run goes on take effect."""
    def on_reasons(run, reasons):
        st = get(session)
        return st.on_reasons(run, reasons) if st.active() else reasons
    return on_reasons
