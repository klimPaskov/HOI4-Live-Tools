"""The issue detector (Tier 0): every finding of a run becomes a tracked ISSUE.

Sources, all read without input: the recorder's snapshots (with the sitrep findings and the
executor's omissions), the `now` block of the player's and the monitor's wake reads, the call log,
the executor's order ledger (game-actions.jsonl), the server's [action] lines, the harness's memory
lines and the plan. General for any country: nothing here names one.

An issue has an id, a class (technical, tool_use, duplicate, idling, gameplay), a severity, the
game dates it was first and last seen, its numbers, the executor lane or the tool that owns it, a
deadline (game date) with the reason for it, and a state:

  open       seen, nothing done about it yet
  acted      the owning lane acted since it opened, and it is still there
  resolved   gone (a newer read of its source no longer shows it, or a log issue stayed quiet)
  recurring  back after it was resolved

Files (in the output directory, normally the run directory):
  issues.jsonl       append-only: one line per transition (open, acted, overdue, resolved, recurred,
                     escalated, lesson)
  ISSUES.md          the open issues, overdue first, one line each
  issues_open.json   the open issues for the executor (lanes to act on, rows for the player)
  issues_counts.json counters per game year and class
"""
from __future__ import annotations

import json
import os
import time
from collections import defaultdict, deque

from . import findings as findings_mod
from . import idle_days as idle_mod
from . import sinks as sinks_mod
from . import issue_rules as R
from .runlogs import date_of_days, game_days

SEV_ORDER = {"high": 0, "medium": 1, "low": 2}
QUIET_DAYS = 14          # a log issue not seen again for two game weeks is taken as gone
DUP_WINDOW = 7           # identical verified orders within a game week are duplicates
FAIL_STREAK = 2          # failures of one (tool, code) before it is an issue
LIGHT_ORIGINS = ("omission",)
# "acted" is a remedy in flight: an issue still seen this many game days after the lane's last
# verified order is open again (the order did not fix it), and counts as a remedy that did not
# hold when it passes its deadline (owner, 27 Nov 2026: issues marked acted sat open for weeks)
ACT_EFFECT_DAYS = 3


# log issues whose second sighting is the same refusal of the same order again
REPEAT_CODES = ("refusal_retried_unchanged", "readback_no_change", "tool_failing", "invalid_arguments")


def _key_args(args):
    a = {k: v for k, v in (args or {}).items() if k not in ("request_id", "lease_token", "lease")}
    return json.dumps(a, sort_keys=True, ensure_ascii=False)[:200]


def _action_key(action):
    a = {k: v for k, v in (action or {}).items() if k not in ("request_id", "lease")}
    return json.dumps(a, sort_keys=True, ensure_ascii=False)[:200]


def _err_code(body):
    if not isinstance(body, dict):
        return None
    e = body.get("error")
    if isinstance(e, dict):
        return e.get("code") or "error"
    if e:
        return str(e)[:40]
    return None


def _receipt_status(body):
    if not isinstance(body, dict):
        return None
    r = body.get("receipt")
    if isinstance(r, dict):
        return r.get("status")
    return body.get("status") if isinstance(body.get("status"), str) else None


class Detector:
    def __init__(self, out_dir, plan=None, plan_state=None, history=None, rss_limit_gb=None, write=True):
        self.out = out_dir
        os.makedirs(out_dir, exist_ok=True)
        self.plan = plan or {}
        self.plan_state = plan_state or {}
        self.history = history or {}          # tool -> {"ok": n, "fail": n} from earlier runs
        self.rss_limit = rss_limit_gb or float(os.environ.get("MONITOR_RSS_LIMIT_GB", "9.5"))
        self.write = write
        self.issues = {}
        self.seq = 0
        self.date = None
        self.days = None
        self.at_war = False
        self.listeners = []                   # fn(event, issue)
        self.lessons = None                   # a LessonStore: each issue carries its best lesson
        self.counts = defaultdict(lambda: defaultdict(lambda: defaultdict(float)))
        self.pending = []
        # log state
        self.req_action = {}                  # request id -> action (executor)
        self.req_code = {}                    # request id -> the server's error code
        self.counted = set()                  # request ids whose refusal is counted
        self.refusals = {}                    # (actor, key) -> [code, count, first_days]
        self.verified = defaultdict(deque)    # (actor, key) -> days of verified sends
        self.tool_stats = defaultdict(lambda: [0, 0])
        self.fail_streak = defaultdict(int)   # (actor, tool, code) -> failures in a row
        self.last_refusal = {}                # (actor, tool) -> (code, days)
        self.exec_last_days = None
        self.exec_last_army_days = None
        self.exec_acts = 0
        self.unpaused_since = None            # (date, utc) the clock was last seen moving
        self.ops_executing = False
        self.overhead_ms = []                 # per snapshot
        self.idle = idle_mod.IdleDays()       # idle game days per sink (the gate packet's table)

    # --- issue lifecycle ---------------------------------------------------------------------

    def _emit(self, event, issue, **extra):
        rec = {"t": round(time.time(), 1), "date": self.date, "event": event, "id": issue["id"],
               "key": issue["key"], "class": issue["class"], "severity": issue["severity"]}
        if event in ("open", "recurred", "overdue"):
            rec["text"] = issue["text"][:300]
            rec["lane"] = issue.get("lane")
            rec["numbers"] = issue.get("numbers")
        rec.update(extra)
        self.pending.append(rec)
        year = (self.date or "?")[:4]
        self.counts[year][issue["class"]][event] += 1
        for fn in self.listeners:
            fn(event, issue)

    def raise_issue(self, key, code, text, severity="medium", numbers=None, lane=None, tool=None,
                    origin="snapshot", subject=None, escalate=True):
        cls = R.class_of(code)
        lane = lane or R.lane_of(code, tool)
        it = self.issues.get(key)
        if it is None:
            self.seq += 1
            days, why = R.deadline_days(code, self.at_war)
            it = {"id": f"I{self.seq:04d}", "key": key, "code": code, "class": cls, "severity": severity,
                  "subject": subject, "text": text, "numbers": numbers or {}, "lane": lane, "tool": tool,
                  "origin": origin, "first_seen": self.date, "last_seen": self.date, "seen": 1,
                  "deadline": date_of_days(self.days + days) if self.days is not None else None,
                  "deadline_reason": f"{days} game day(s): {why}", "state": "open", "acts": 0,
                  "tried": [], "recurrences": 0, "overdue": False, "escalate": escalate,
                  "first_days": self.days, "exec_acts_at_open": self.exec_acts}
            self.issues[key] = it
            self._lesson(it)
            self._emit("open", it)
            return it
        it["last_seen"], it["text"], it["numbers"] = self.date, text, numbers or it["numbers"]
        if it["state"] == "acted" and self.days is not None and it.get("acted_days") is not None and \
                self.days - it["acted_days"] > ACT_EFFECT_DAYS:
            # the lane's verified order did not make it go: the remedy did not hold
            it["state"] = "open"
            it["no_effect"] = it.get("no_effect", 0) + 1
            self._emit("acted_no_effect", it, days_since_act=round(self.days - it["acted_days"], 1),
                       tried=it["tried"][-3:])
        it["severity"] = severity if SEV_ORDER.get(severity, 3) < SEV_ORDER.get(it["severity"], 3) else it["severity"]
        it["seen"] += 1
        if it["state"] == "resolved":
            it["state"], it["recurrences"], it["overdue"] = "recurring", it["recurrences"] + 1, False
            days, why = R.deadline_days(code, self.at_war)
            it["first_days"], it["exec_acts_at_open"], it["stale"] = self.days, self.exec_acts, False
            it["deadline"] = date_of_days(self.days + days) if self.days is not None else None
            self._lesson(it)
            self._emit("recurred", it, recurrences=it["recurrences"])
        return it

    def _lesson(self, it):
        """The best lesson for this issue (read from the store's cache: no file read per issue): the
        executor's advice for a refused order, else the lesson for the issue's code."""
        if self.lessons is None:
            return
        try:
            cause = (it.get("numbers") or {}).get("code")
            le = self.lessons.executor_advice(it.get("tool"), cause) if cause and it.get("tool") else None
            if le is None:
                rows = self.lessons.match(code=it["code"], tool=it.get("tool"), cause=cause, at_war=self.at_war,
                                          limit=1)
                le = rows[0] if rows else None
        except Exception:  # a broken store never stops detection
            le = None
        if le:
            ex = le.get("executor") or {}
            it["lesson"] = {"id": le["id"], "confidence": round(le.get("confidence", 0), 2),
                            "treat_as": ex.get("treat_as"), "advice": ex.get("advice") or le.get("statement", "")[:240]}

    def resolve(self, it, why):
        if it["state"] == "resolved":
            return
        it["state"], it["resolved_at"] = "resolved", self.date
        self._emit("resolved", it, why=why, acted=it["acts"] > 0, tried=it["tried"][-3:])

    def _set_date(self, date):
        d = game_days(date)
        if d is None or (self.days is not None and d < self.days - 1):
            return
        if self.days is not None and d > self.days:
            step = d - self.days
            year = (self.date or "?")[:4]
            for it in self.issues.values():
                if it["state"] != "resolved":
                    self.counts[year][it["class"]]["open_days"] += step
            self.date, self.days = date, d
            self.idle.step(self.issues.values(), step)
            self._deadlines()
        elif self.days is None:
            self.date, self.days = date, d

    def _deadlines(self):
        for it in list(self.issues.values()):
            if it["state"] == "resolved":
                continue
            if it["origin"] == "log" and it.get("last_days", it["first_days"]) is not None and \
                    self.days - (it.get("last_days") or it["first_days"] or self.days) > QUIET_DAYS:
                self.resolve(it, f"not seen for {QUIET_DAYS} game days")
                continue
            dl = game_days(it["deadline"]) if it.get("deadline") else None
            if dl is not None and self.days > dl and not it["overdue"]:
                it["overdue"] = it["was_overdue"] = True
                self._emit("overdue", it, acted=it["acts"] > 0,
                           executor_acts_since=self.exec_acts - it["exec_acts_at_open"])
            if dl is not None and it.get("first_days") is not None and not it.get("stale") and \
                    self.days > dl + max(dl - it["first_days"], 3):
                it["stale"] = True
                self._emit("stale", it, acted=it["acts"] > 0)

    def _acted(self, kind, key, ok):
        kind = str(kind or "")
        for it in self.issues.values():
            if it["state"] == "resolved" or not it.get("lane"):
                continue
            kinds = R.LANE_KINDS.get(it["lane"], ())
            if any(kind.startswith(k) for k in kinds if k):
                it["acts"] += 1
                if len(it["tried"]) < 8:
                    it["tried"].append(f"{'ok' if ok else 'refused'} {key[:120]}")
                # "acted" means an order of the lane passed, not that the lane had a turn
                if ok:
                    it["acted_days"] = self.days
                if ok and it["state"] == "open":
                    it["state"] = "acted"
                    self._emit("acted", it, action=key[:160])

    # --- snapshots ------------------------------------------------------------------------

    def snapshot(self, snap, prev=()):
        t0 = time.perf_counter()
        if snap.get("date"):
            self._set_date(snap["date"])
        light = bool(snap.get("light"))
        wars = snap.get("wars") or {}
        self.at_war = bool(wars.get("at_war"))
        ops = ((snap.get("executor") or {}).get("operations") or {})
        if not light:
            self.ops_executing = any((o or {}).get("phase") == "executing" for o in ops.values())
        fs = snap.get("findings")
        if fs is None:
            fs = findings_mod.findings(snap, list(prev))
        if not light:
            # the sinks and slots (sinks.py): computed here too, so a replay of a run recorded
            # before them shows what they would have caught
            seen_codes = {(f.get("code"), f.get("subject")) for f in fs}
            fs = list(fs) + [f for f in sinks_mod.findings(snap, list(prev))
                             if (f["code"], f.get("subject")) not in seen_codes]
        present = set()
        for f in fs:
            code = str(f.get("code") or "finding")
            origin = "omission" if f.get("owner") in ("server omissions",) else "snapshot"
            if light and origin != "omission":
                continue
            key = f"{code}:{f.get('subject')}" if f.get("subject") else code
            sev = f.get("severity") or "medium"
            esc = True
            if code.startswith("frame_") or code.startswith("game_alert"):
                # a game alert a lane owns escalates once its deadline passes acted-but-open (the
                # lane acted and the flag stayed); an alert no lane owns stays a quiet row
                sev, esc = "low", R.lane_of(code) not in (None, "alerts")
            if code in ("executor_repeated_refusal", "plan_item_blocked"):
                esc = False  # the executor's ledger: answered by lessons, reported at the gate
            lane = R.executor_owner_lane(f.get("numbers") or {})
            if lane is None and code in ("lane_wait_outlived", "lane_silent"):
                lane = R.lane_of(f.get("subject") or "")
            it = self.raise_issue(key, code, f.get("text") or code, sev, f.get("numbers"), origin=origin,
                                  subject=f.get("subject"), escalate=esc, lane=lane)
            note = (f.get("numbers") or {}).get("owner_note") or (f.get("numbers") or {}).get("note")
            if note:
                # the owning rule's own stated reason: what the lane answered when it had its turn
                it["lane_reason"] = str(note)[:300]
            present.add(key)
        if light:
            for key in self._plan_rules(snap):  # dated checkpoints need no full snapshot
                present.add(key)
        else:
            for key in self._trends(snap, prev):
                present.add(key)
            for k, v in (snap.get("read_errors") or {}).items():
                key = f"read_errors:{k}"
                self.raise_issue(key, "read_errors", f"the snapshot's {k} read failed: {str(v)[:120]}",
                                 "medium", origin="snapshot", subject=k)
                present.add(key)
        for it in list(self.issues.values()):
            if it["state"] == "resolved" or it["key"] in present:
                continue
            if it["origin"] == "omission" or (not light and it["origin"] == "snapshot"):
                self.resolve(it, "no longer in the " + ("wake read" if light else "snapshot"))
        if not light:
            self.overhead_ms.append(round((time.perf_counter() - t0) * 1000, 2))

    def _trends(self, snap, prev):
        """Gameplay rules beyond the sitrep's findings; returns the keys raised."""
        out = []
        trig = str(snap.get("trigger") or "")
        land = snap.get("land") or {}
        # the planning bonus at a declaration
        if "war_declared" in trig:
            low = []
            for a in land.get("armies") or []:
                pl = (a.get("planning") or {}).get("mean")
                # only an attacking army plans for the war's first blow: a hold (area defence, a
                # front with no offensive) builds no planning to spend (s7n: the Westwall armies
                # were counted as attackers with 0 planning)
                attacking = any(o.get("offensives") or o.get("kind") in ("offensive", "spearhead")
                                for o in a.get("orders") or [])
                if attacking and pl is not None and pl < 0.1 and (a.get("divisions") or 0) > 0:
                    low.append((a.get("army"), pl))
            if low:
                key = "planning_low_at_declaration:" + trig
                names = ", ".join(f"{n} {p:.2f}" for n, p in low[:4])
                self.raise_issue(key, "planning_low_at_declaration",
                                 f"war declared ({trig}) with {len(low)} attacking army(ies) near zero planning "
                                 f"({names}): the front was drawn too late or redrawn", "high",
                                 {"armies": len(low)})
                out.append(key)
        # supply collapsing while an operation executes
        st = snap.get("stacking") or {}
        mean = st.get("supply_mean")
        if self.ops_executing and mean is not None and prev:
            back = findings_mod.lookback(snap, list(prev), 14)
            was = ((back or {}).get("stacking") or {}).get("supply_mean")
            if was is not None and was - mean >= 15 and mean < 60:
                key = "supply_collapsing"
                self.raise_issue(key, "supply_collapsing",
                                 f"mean division supply fell from {was:.0f}% to {mean:.0f}% in 14 days while an "
                                 f"operation executes", "high", {"was": was, "now": mean})
                out.append(key)
        key = self._war_silence()
        if key:
            out.append(key)
        out += self._plan_rules(snap)
        return out

    def _war_silence(self):
        """An executing operation and no army order for three game days (checked at every dated read,
        not only at snapshots: a week between snapshots hides a silent first week of war)."""
        if self.ops_executing and self.at_war and self.exec_last_army_days is not None and \
                self.days is not None and self.days - self.exec_last_army_days >= 3:
            key = "war_orders_silent"
            self.raise_issue(key, "war_orders_silent",
                             f"an operation executes and the executor sent no army order for "
                             f"{self.days - self.exec_last_army_days:.0f} game days", "high",
                             {"days": round(self.days - self.exec_last_army_days, 1)}, origin="clock")
            return key
        return None

    def _plan_rules(self, snap):
        out = []
        hour = (snap.get("executor") or {}).get("game_hour")
        done = ((self.plan_state.get("strategy") or {}).get("checkpoints") or {})
        for cp in self.plan.get("checkpoints") or []:
            if not isinstance(cp, dict) or not cp.get("id") or not cp.get("by"):
                continue
            by = game_days(cp["by"])
            if by is None or self.days is None or self.days <= by:
                continue
            settled = done.get(cp["id"])
            met = isinstance(settled, list) and len(settled) > 1 and settled[1] is True and \
                (hour is None or settled[0] <= hour)
            if not met:
                key = f"checkpoint_missed:{cp['id']}"
                self.raise_issue(key, "checkpoint_missed",
                                 f"checkpoint {cp['id']} (by {cp['by']}: {', '.join(map(str, cp.get('must') or []))[:120]}) "
                                 f"is past and not met; the plan says: {cp.get('else') or 'nothing'}", "high",
                                 subject=cp["id"], lane=R.checkpoint_lane(cp.get("must")))
                out.append(key)
        fielded = (snap.get("land") or {}).get("division_count")
        rows = (self.plan.get("force_design") or {}).get("land") if isinstance(self.plan.get("force_design"), dict) else None
        if fielded and isinstance(rows, list) and self.days is not None:
            due = sum(int(r.get("count") or 0) for r in rows if isinstance(r, dict) and r.get("ready")
                      and (game_days(r["ready"]) or 1e9) <= self.days)
            if due and fielded < 0.9 * due:
                key = "force_behind_plan"
                self.raise_issue(key, "force_behind_plan",
                                 f"{fielded} divisions fielded where the plan's force design has {due} ready by "
                                 f"now ({100 * fielded / due:.0f}%): the operations were sized on the forecast",
                                 "medium", {"fielded": fielded, "planned": due})
                out.append(key)
        return out

    # --- logs ------------------------------------------------------------------------------

    def call(self, c):
        """One calls.jsonl record (a call through the run's broker)."""
        tool, args, body = c.get("tool") or "", c.get("args") or {}, c.get("body")
        actor = c.get("source") or "player"
        code = _err_code(body) if c.get("isError") or _err_code(body) else None
        st = self.tool_stats[tool]
        if code:
            st[1] += 1
        else:
            st[0] += 1
        is_act = isinstance(args, dict) and bool(args.get("request_id"))
        self._tool_result(actor, tool, _key_args(args), code, c.get("ms"), is_act)
        if not code and is_act and _receipt_status(body) in ("NotApplied", "Diverged"):
            self._readback(actor, tool, _key_args(args))
        if not code and is_act and _receipt_status(body) == "Verified":
            self._verified(actor, tool, _key_args(args))
        self._below_history(tool)

    def server_action(self, a):
        """One [action] line of the server (the executor's own calls are marked executor:true)."""
        if not a.get("executor"):
            return
        tool = a.get("tool") or ""
        rid = a.get("request_id")
        action = self.req_action.get(rid)
        key = _action_key(action) if action else ""
        code = None if a.get("ok") else (a.get("error") or "error")
        if not rid and not code:
            return  # the executor's reads: only their failures matter
        st = self.tool_stats[tool]
        st[0 if not code else 1] += 1
        self._tool_result("executor", tool, key or tool, code, a.get("ms"), False)
        if rid and code:
            if rid in self.counted:
                r = self.refusals.get(("executor", key))
                if r and r[0] == "not_applied":
                    r[0] = code
            else:
                self.counted.add(rid)
                self.req_code[rid] = code
                self._refused("executor", tool, key or f"{tool} (refused before input)", code)

    def action(self, a, utc=None):
        """One final game-actions.jsonl record (the executor's order and its read-back)."""
        action = a.get("action") or {}
        if a.get("id"):
            self.req_action[a["id"]] = action
            if len(self.req_action) > 5000:
                for k in list(self.req_action)[:1000]:
                    del self.req_action[k]
        kind = str(action.get("kind") or "")
        key = _action_key(action)
        status = a.get("status")
        if status == "Dispatching":
            return  # the order is known by its request id before the server's line about it
        self.exec_acts += 1
        self.exec_last_days = self.days
        if kind.startswith("army"):
            self.exec_last_army_days = self.days
            ws = self.issues.get("war_orders_silent")
            if ws and ws["state"] != "resolved" and status == "Verified":
                self.resolve(ws, "an army order went out")
        self._acted(kind, key, status == "Verified")
        if status == "Verified":
            self._verified("executor", "hoi4_" + kind, key)
            self._clear_refusal("executor", key)
        elif status == "Diverged":
            self._readback("executor", "hoi4_" + kind, key)
        elif status == "NotApplied" and a.get("id") not in self.counted:
            self.counted.add(a.get("id"))
            self._refused("executor", "hoi4_" + kind, key, self.req_code.get(a.get("id"), "not_applied"))

    def outcome(self, rec):
        """One line of RUN/issue_outcomes.jsonl: what the executor lane did with an issue it was
        offered (acted / refused / waiting / cannot, with the lane's own reason)."""
        for it in self.issues.values():
            if it["id"] == rec.get("id") and it["state"] != "resolved":
                it["lane_outcome"] = {k: rec.get(k) for k in ("outcome", "reason", "lane", "hour")}
                if rec.get("reason"):
                    it["lane_reason"] = str(rec["reason"])[:300]
                self.idle.answer(it, rec.get("outcome"), rec.get("reason"))
                if rec.get("outcome") in ("waiting", "cannot"):
                    self._emit("lane_answer", it, outcome=rec.get("outcome"), reason=str(rec.get("reason"))[:200])
                return it
        return None

    def rss(self, r):
        gb = r.get("gb")
        key = "memory_growth"
        if gb is None:
            return
        if gb >= 0.85 * self.rss_limit:
            self.raise_issue(key, "memory_growth",
                             f"game memory {gb:.1f} GB of the {self.rss_limit:.1f} GB the run allows: save and "
                             f"restart at a segment boundary before it slows or crashes", "high", {"gb": gb},
                             origin="memory")
        elif key in self.issues:
            self.resolve(self.issues[key], f"memory {gb:.1f} GB")

    def clock(self, date, paused, utc):
        """A dated read: the game not advancing while unpaused."""
        if not date:
            return
        self._set_date(date)
        self._war_silence()
        key = "game_not_advancing"
        if paused:
            self.unpaused_since = None
            return
        if self.unpaused_since is None or self.unpaused_since[0] != date:
            self.unpaused_since = (date, utc)
            if key in self.issues and self.issues[key]["state"] != "resolved":
                self.resolve(self.issues[key], "the date moves again")
            return
        if utc is not None and self.unpaused_since[1] is not None and utc - self.unpaused_since[1] >= 180:
            self.raise_issue(key, "game_not_advancing",
                             f"the game reads unpaused at {date} for {utc - self.unpaused_since[1]:.0f} s and its "
                             f"date does not move (hung, a modal window, or the speed lost)", "high",
                             {"seconds": round(utc - self.unpaused_since[1])}, origin="clock")

    # --- log rules ---------------------------------------------------------------------------

    def _log_issue(self, key, code, text, severity, numbers, tool, lane=None, actor=None):
        # the executor's own refusals are answered by its ledger and the lessons (Tier 0), never by
        # a model: they go to the gate packet; the player's are escalated to the player itself
        it = self.raise_issue(key, code, text, severity, numbers, tool=tool, lane=lane, origin="log",
                              subject=tool, escalate=actor != "executor")
        it["actor"] = actor
        it["last_days"] = self.days
        n = (numbers or {}).get("times") or (numbers or {}).get("in_row") or 0
        if n >= 2 and not it.get("repeat_emitted") and code in REPEAT_CODES:
            # the same refusal twice: the lesson writer turns it into a rule for the next try
            it["repeat_emitted"] = True
            self._emit("repeated", it, times=n)
        return it

    def _tool_result(self, actor, tool, key, code, ms, is_act):
        if ms is not None and ms >= 60000 and not tool.startswith("hoi4_wait") and tool != "hoi4_save":
            self._log_issue(f"tool_timeout:{tool}", "tool_timeout",
                            f"{tool} took {ms / 1000:.0f} s ({actor}): a call that long holds the input lane",
                            "medium", {"ms": ms}, tool, actor=actor)
        if tool == "hoi4_save":
            self._save(code)
        if not code:
            for k in [k for k in self.fail_streak if k[0] == actor and k[1] == tool]:
                del self.fail_streak[k]
            fk = f"tool_failing:{tool}"
            if fk in self.issues and self.issues[fk]["state"] != "resolved":
                self.resolve(self.issues[fk], f"{tool} succeeded")
            return
        kind = R.error_kind(code)
        if kind == "noop":
            self._log_issue(f"noop_call:{tool}", "noop_call",
                            f"{tool} ({actor}) answered {code}: the order asked for what already holds "
                            f"(last {key[:100]})", "low", {"code": code}, tool, actor=actor)
            return
        streak_key = (actor, tool, code)
        self.fail_streak[streak_key] += 1
        n = self.fail_streak[streak_key]
        if kind == "tool_use" and (actor != "executor" or n >= FAIL_STREAK):
            self._log_issue(f"invalid_arguments:{tool}:{code}", "invalid_arguments",
                            f"{tool} refused {code} ({actor}, {n} in a row; last args {key[:100]}): the call is "
                            f"wrong, not the game", "medium", {"in_row": n, "code": code}, tool, actor=actor)
        elif kind == "technical" and n >= FAIL_STREAK and tool != "hoi4_save":
            self._log_issue(f"tool_failing:{tool}", "tool_failing",
                            f"{tool} failed {n} times in a row with {code} ({actor})", "high" if n >= 4 else "medium",
                            {"in_row": n, "code": code}, tool, actor=actor)
        last = self.last_refusal.get((actor, tool))
        if kind == "precondition" and last and last[0] == code and self.days is not None and \
                last[1] is not None and self.days - last[1] <= 1:
            self._log_issue(f"precondition_unmet:{tool}:{code}", "precondition_unmet",
                            f"{tool} refused {code} again within a game day ({actor}): it was called while the "
                            f"cause of its last refusal still held", "medium", {"code": code}, tool, actor=actor)
        self.last_refusal[(actor, tool)] = (code, self.days)
        if is_act or actor != "executor":
            self._refused(actor, tool, key, code)

    def _refused(self, actor, tool, key, code):
        r = self.refusals.get((actor, key))
        if r and (r[0] == code or code == "not_applied" or r[0] == "not_applied"):
            r[1] += 1
            if code != "not_applied":
                r[0] = code
            if r[1] >= 2:
                self._log_issue(f"refusal_retried_unchanged:{actor}:{key}", "refusal_retried_unchanged",
                                f"{actor} sent `{tool} {key[:140]}` {r[1]} times and it was refused each time "
                                f"({r[0]})", "high" if r[1] >= 4 else "medium", {"times": r[1], "code": r[0]},
                                tool, actor=actor)
        else:
            self.refusals[(actor, key)] = [code, 1, self.days]

    def _clear_refusal(self, actor, key):
        r = self.refusals.pop((actor, key), None)
        k = f"refusal_retried_unchanged:{actor}:{key}"
        if r and k in self.issues and self.issues[k]["state"] != "resolved":
            self.issues[k]["passed_after"] = r[1]
            self.resolve(self.issues[k], f"the order passed after {r[1]} refusals ({r[0]})")

    def _readback(self, actor, tool, key):
        k = f"readback_no_change:{tool}"
        it = self.issues.get(k)
        n = (it["numbers"].get("times", 0) if it and it["state"] != "resolved" else 0) + 1
        self._log_issue(k, "readback_no_change",
                        f"{tool} ({actor}) read back without the change asked ({n} time(s); last {key[:120]})",
                        "medium", {"times": n}, tool, actor=actor)

    def _verified(self, actor, tool, key):
        q = self.verified[(actor, key)]
        now = self.days if self.days is not None else 0
        while q and now - q[0] > DUP_WINDOW:
            q.popleft()
        q.append(now)
        if len(q) >= 2:
            self._log_issue(f"duplicate_action:{actor}:{key}", "duplicate_action",
                            f"{actor} sent the verified order `{tool} {key[:140]}` {len(q)} times within "
                            f"{DUP_WINDOW} game days", "low" if len(q) == 2 else "medium", {"times": len(q)},
                            tool, actor=actor)

    def _save(self, code):
        key = "save_failing"
        st = self.tool_stats["hoi4_save"]
        if code:
            self.fail_streak[("any", "hoi4_save", "")] += 1
            n = self.fail_streak[("any", "hoi4_save", "")]
            self._log_issue(key, "save_failing",
                            f"hoi4_save failed ({code}); {n} in a row, {st[1]} of {st[0] + st[1]} saves failed in "
                            f"this run", "high" if n >= 2 else "medium", {"in_row": n, "failed": st[1],
                                                                          "calls": st[0] + st[1]},
                            "hoi4_save", lane="saves")
        else:
            self.fail_streak[("any", "hoi4_save", "")] = 0
            if key in self.issues and self.issues[key]["state"] != "resolved":
                self.resolve(self.issues[key], "a save was written")

    def _below_history(self, tool):
        h = self.history.get(tool)
        ok, fail = self.tool_stats[tool]
        if not h or ok + fail < 6 or (h.get("ok", 0) + h.get("fail", 0)) < 10:
            return
        hist = h["ok"] / (h["ok"] + h["fail"])
        now = ok / (ok + fail)
        key = f"tool_below_history:{tool}"
        if now < hist - 0.25:
            self._log_issue(key, "tool_below_history",
                            f"{tool} succeeds {100 * now:.0f}% in this run ({ok}/{ok + fail}) against "
                            f"{100 * hist:.0f}% in earlier runs", "medium", {"now": now, "history": hist}, tool)
        elif key in self.issues and self.issues[key]["state"] != "resolved" and now >= hist - 0.1:
            self.resolve(self.issues[key], "back to its usual rate")

    def executor_silent_check(self):
        if self.days is None or self.exec_last_days is None:
            return
        open_idle = [i for i in self.issues.values() if i["state"] != "resolved" and i["class"] == "idling"]
        key = "executor_silent"
        if self.days - self.exec_last_days >= 30 and open_idle:
            self.raise_issue(key, "executor_silent",
                             f"the executor's last order was {self.days - self.exec_last_days:.0f} game days ago "
                             f"while {len(open_idle)} idling issue(s) stay open", "high", origin="clock")
        elif key in self.issues and self.issues[key]["state"] != "resolved" and self.days - self.exec_last_days < 1:
            self.resolve(self.issues[key], "the executor acts again")

    # --- outputs ---------------------------------------------------------------------------------

    def open_issues(self):
        rows = [i for i in self.issues.values() if i["state"] != "resolved"]
        rows.sort(key=lambda i: (not i["overdue"], SEV_ORDER.get(i["severity"], 3), i["first_seen"] or ""))
        return rows

    def flush(self):
        if not self.write:
            self.pending.clear()
            return
        if self.pending:
            with open(os.path.join(self.out, "issues.jsonl"), "a", encoding="utf-8") as f:
                for rec in self.pending:
                    f.write(json.dumps(rec, ensure_ascii=False, separators=(",", ":")) + "\n")
            self.pending.clear()
        rows = self.open_issues()
        _atomic(os.path.join(self.out, "issues_open.json"), json.dumps({
            "date": self.date, "at_war": self.at_war,
            "issues": [dict({k: i.get(k) for k in ("id", "key", "code", "class", "severity", "lane", "tool", "actor",
                                                    "first_seen", "deadline", "deadline_reason", "overdue", "state",
                                                    "text", "lesson", "packet", "answer", "no_effect")},
                            cause=(i.get("numbers") or {}).get("code"))
                       for i in rows[:60]]}, ensure_ascii=False, indent=1))
        _atomic(os.path.join(self.out, "issues_counts.json"), json.dumps(self.count_table(), indent=1))
        _atomic(os.path.join(self.out, "ISSUES.md"), self.render())

    def count_table(self):
        return {y: {c: {k: round(v, 1) for k, v in d.items()} for c, d in cl.items()}
                for y, cl in sorted(self.counts.items())}

    def render(self, limit=40):
        rows = self.open_issues()
        by = defaultdict(int)
        for i in rows:
            by[i["class"]] += 1
        head = ", ".join(f"{c} {by[c]}" for c in R.CLASSES if by[c]) or "none"
        lines = [f"# Open issues at {self.date} ({len(rows)}: {head})", "",
                 "overdue first; id class severity first-seen deadline lane: text", ""]
        for i in rows[:limit]:
            mark = "OVERDUE " if i["overdue"] else ""
            st = "" if i["state"] == "open" else f" [{i['state']}]"
            if i.get("no_effect") and i["state"] == "open":
                st = f" [acted {i['no_effect']}x, no effect]"
            lines.append(f"- {mark}{i['id']} {i['class']} {i['severity']} {i['first_seen']} -> {i['deadline']} "
                         f"{i.get('lane') or i.get('tool') or '-'}{st}: {i['text'][:180]}")
        if len(rows) > limit:
            lines.append(f"- ... {len(rows) - limit} more in issues_open.json")
        return "\n".join(lines) + "\n"


def _atomic(path, text):
    tmp = path + ".tmp"
    with open(tmp, "w", encoding="utf-8") as f:
        f.write(text)
    os.replace(tmp, path)
