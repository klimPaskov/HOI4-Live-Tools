"""Lessons: what a run learned from its own mistakes, kept as data beside the learned skills.

A lesson is one JSON file in `<learned store>/lessons/<id>.json` (the store of hoi4_skill_save:
`HOI4_LIVE_LEARNED`, else `learned/` beside the runs folder). It says, in general terms:

  situation   when it applies: an issue code, or a tool and its refusal cause, and when (war/peace)
  statement   what is true, in one sentence
  tried       what was done, and the outcome (worked, failed, rule)
  evidence    the lines that support it (run, date, numbers)
  scope       run:<name>, plan:<name>, country:<TAG> or any
  learned_on  game version and mod set (a patch retires it: it is not applied on another version
              until confirmed there)
  confidence  0..1, up with each confirmation, down with each contradiction
  executor    optional: how the executor treats a refusal that matches (transient: retry soon and
              never give up; permanent: do not resend unchanged and tell the player the advice; wait:
              rest the given game days)
  plan_lint   optional: a plan path and a value that the lesson warns against

Lessons never override a safety rule or the FAIR rules: they change only how soon an order is
retried, what a packet shows and what a lint warns about.
"""
from __future__ import annotations

import json
import os
import re
import time

MIN_EXECUTOR_CONFIDENCE = 0.5
TREATS = ("transient", "permanent", "wait")


def store_dir(run_dir):
    """The learned store (the same rule as the server's skills store)."""
    env = os.environ.get("HOI4_LIVE_LEARNED")
    if env:
        return env
    parent = os.path.dirname(os.path.abspath(run_dir))
    if os.path.basename(parent) == "runs":
        return os.path.join(os.path.dirname(parent), "learned")
    return os.path.join(parent, "learned")


def slug(text):
    s = re.sub(r"[^a-z0-9]+", "_", str(text).lower()).strip("_")
    return s[:64] or "lesson"


def _confirm(c):
    return round(1 - (1 - c) * 0.7, 3)


def _contradict(c):
    return round(c * 0.6, 3)


class LessonStore:
    def __init__(self, store, game_version="", mod_set=""):
        self.dir = os.path.join(store, "lessons")
        self.game_version = game_version
        self.mod_set = mod_set
        self._cache = None

    # --- files ---------------------------------------------------------------------------------

    def path(self, lid):
        return os.path.join(self.dir, f"{lid}.json")

    def all(self):
        if self._cache is not None:
            return self._cache
        out = []
        try:
            names = sorted(os.listdir(self.dir))
        except OSError:
            names = []
        for n in names:
            if not n.endswith(".json"):
                continue
            try:
                with open(os.path.join(self.dir, n), encoding="utf-8") as f:
                    v = json.load(f)
            except (OSError, ValueError):
                continue
            if isinstance(v, dict) and v.get("id"):
                out.append(v)
        self._cache = out
        return out

    def save(self, lesson):
        os.makedirs(self.dir, exist_ok=True)
        lesson["updated_unix"] = int(time.time())
        tmp = self.path(lesson["id"]) + ".tmp"
        with open(tmp, "w", encoding="utf-8") as f:
            f.write(json.dumps(lesson, ensure_ascii=False, indent=2) + "\n")
        os.replace(tmp, self.path(lesson["id"]))
        self._cache = None
        return lesson

    def get(self, lid):
        return next((x for x in self.all() if x["id"] == lid), None)

    # --- matching ----------------------------------------------------------------------------------

    def usable(self, lesson, country=None, plan=None, run=None):
        if lesson.get("status", "active") != "active":
            return False
        scope = str(lesson.get("scope") or "any")
        kind, _, val = scope.partition(":")
        if kind == "country" and country and val != country:
            return False
        if kind == "plan" and plan and val != plan:
            return False
        if kind == "run" and run and val != run:
            return False
        lo = lesson.get("learned_on") or {}
        if self.game_version and lo.get("game_version") and lo["game_version"] != self.game_version \
                and not lesson.get("version_independent"):
            return False
        if self.mod_set and lo.get("mod_set") and lo["mod_set"] != self.mod_set \
                and not lesson.get("version_independent"):
            return False
        return True

    def match(self, code=None, tool=None, cause=None, at_war=None, country=None, plan=None, run=None,
              limit=5):
        rows = []
        for le in self.all():
            if not self.usable(le, country, plan, run):
                continue
            s = le.get("situation") or {}
            score = 0
            if code and s.get("code"):
                if s["code"] != code and not code.startswith(s["code"] + ":"):
                    continue
                score += 2
            if cause and s.get("cause"):
                if not str(cause).startswith(s["cause"]):
                    continue
                score += 2
            if tool and s.get("tool"):
                if s["tool"] not in ("*", tool):
                    continue
                score += 1
            if not score:
                continue
            when = s.get("when") or "any"
            if at_war is not None and when != "any" and (when == "war") != bool(at_war):
                continue
            rows.append((score, le.get("confidence", 0), le))
        rows.sort(key=lambda r: (-r[0], -r[1]))
        return [r[2] for r in rows[:limit]]

    def executor_advice(self, tool, cause, threshold=MIN_EXECUTOR_CONFIDENCE):
        """What the executor does with a refusal (tool, cause): the Python mirror of the server's
        lookup (native plan_issues.rs), for tests and for the packet."""
        for le in self.match(tool=tool, cause=cause, limit=10):
            ex = le.get("executor") or {}
            if ex.get("treat_as") in TREATS and le.get("confidence", 0) >= threshold:
                return le
        return None

    # --- learning ----------------------------------------------------------------------------------

    def learn(self, situation, statement, outcome, evidence, scope, tried=None, executor=None, source="detector",
              confidence=0.4):
        """Write a lesson, or confirm the one with the same situation and statement; a lesson whose
        situation matches with the opposite outcome is contradicted."""
        lid = slug(f"{situation.get('code') or ''}_{situation.get('tool') or ''}_{situation.get('cause') or ''}_"
                   f"{outcome}")
        same = self.get(lid)
        run = str(evidence).split(" ", 1)[0]
        opposite = "failed" if outcome == "worked" else "worked" if outcome == "failed" else None
        if opposite:
            oid = slug(f"{situation.get('code') or ''}_{situation.get('tool') or ''}_"
                       f"{situation.get('cause') or ''}_{opposite}")
            other = self.get(oid)
            if other and not any(str(e).startswith(f"contradicted: {run} ") for e in other.get("evidence") or []):
                other["contradictions"] = other.get("contradictions", 0) + 1
                other["confidence"] = _contradict(other.get("confidence", 0.4))
                other.setdefault("evidence", []).append(f"contradicted: {evidence}"[:300])
                if other["confidence"] < 0.15:
                    other["status"], other["status_reason"] = "retired", "contradicted by later runs"
                self.save(other)
        if same and any(str(e).split(" ", 1)[0] == run for e in same.get("evidence") or []):
            return same  # one run confirms a lesson once: a flickering issue is not more evidence
        if same:
            same["confirmations"] = same.get("confirmations", 0) + 1
            same["confidence"] = _confirm(same.get("confidence", confidence))
            ev = same.setdefault("evidence", [])
            if evidence not in ev:
                ev.append(str(evidence)[:300])
                del ev[:-12]
            if tried:
                same["tried"] = tried
            return self.save(same)
        lesson = {
            "id": lid, "statement": statement, "situation": situation, "tried": tried or "",
            "outcome": outcome, "evidence": [str(evidence)[:300]], "scope": scope,
            "learned_on": {"game_version": self.game_version, "mod_set": self.mod_set},
            "confidence": confidence, "confirmations": 0, "contradictions": 0, "status": "active",
            "source": source,
        }
        if executor:
            lesson["executor"] = executor
        return self.save(lesson)

    def seed(self, seed_path, overwrite=False):
        """Install the seeded lessons (a JSON list) that the store does not have yet."""
        with open(seed_path, encoding="utf-8") as f:
            rows = json.load(f)
        n = 0
        for le in rows:
            if not overwrite and self.get(le["id"]):
                continue
            le = dict(le)
            le.setdefault("learned_on", {"game_version": self.game_version, "mod_set": self.mod_set})
            le.setdefault("confirmations", 0)
            le.setdefault("contradictions", 0)
            le.setdefault("status", "active")
            le.setdefault("source", "seed")
            self.save(le)
            n += 1
        return n

    # --- the plan lint -----------------------------------------------------------------------------

    def lint_plan(self, plan, country=None, plan_name=None):
        """Warnings for plan rows a lesson of scope plan/country/any warns against."""
        out = []
        for le in self.all():
            pl = le.get("plan_lint")
            if not pl or not self.usable(le, country, plan_name):
                continue
            if str(le.get("scope", "any")).startswith("run:"):
                continue
            for where, value in walk(plan, pl.get("path", "")):
                if _matches(value, pl):
                    out.append({"lesson": le["id"], "where": where, "confidence": le.get("confidence"),
                                "text": pl.get("text") or le.get("statement")})
        return out


def walk(obj, path):
    """(dotted location, value) for a path with `[*]` wildcards ("operations[*].assault_guard")."""
    parts = [p for p in re.split(r"\.", path) if p]
    found = [("", obj)]
    for p in parts:
        nxt = []
        m = re.fullmatch(r"([^\[]*)(\[\*\])?", p)
        name, star = m.group(1), m.group(2)
        for where, v in found:
            if name:
                if not isinstance(v, dict) or name not in v:
                    continue
                v, where = v[name], f"{where}.{name}" if where else name
            if star:
                items = v.items() if isinstance(v, dict) else enumerate(v) if isinstance(v, list) else []
                for k, x in items:
                    nxt.append((f"{where}[{k}]", x))
            else:
                nxt.append((where, v))
        found = nxt
    return found


def _matches(value, pl):
    if "equals" in pl:
        return value == pl["equals"]
    if "longer_than" in pl:
        return isinstance(value, list) and len(value) > pl["longer_than"]
    if "shorter_than" in pl:
        return isinstance(value, list) and len(value) < pl["shorter_than"]
    if "contains" in pl:
        return isinstance(value, (list, str)) and pl["contains"] in value
    return value is not None


def lessons_from_issue(store, issue, event, run_name, plan_name=None, country=None):
    """The detector's lesson writer.

    - A state issue (idling, gameplay) that went past its deadline and then cleared after its lane
      acted: what was done worked (scope run; confirmed by later runs it rises).
    - A state issue back a second time after it was cleared: the remedy tried does not hold.
    - An order refused with a typed cause (not a GUI or read-back failure) that later passed
      unchanged: that cause is transient for that tool (executor: retry soon, never give up).
    - An order refused four or more times with the same typed cause and never passed: it does not
      pass unchanged (executor: do not resend it unchanged; tell the player).
    Generic tool-use and duplicate issues write no lesson: the seeded ones cover the known forms.
    """
    from .issue_rules import error_kind
    code = issue.get("code") or ""
    tried = "; ".join(issue.get("tried") or [])[:300]
    if issue["class"] in ("idling", "gameplay") and not code.startswith(("frame_", "game_alert")) \
            and issue.get("origin") != "log":
        if event == "resolved" and issue.get("acts") and (issue.get("overdue") or issue.get("recurrences")):
            ev = (f"{run_name} {issue.get('first_seen')}..{issue.get('resolved_at')}: {issue['text'][:160]}; "
                  f"cleared after {issue['acts']} action(s) of lane {issue.get('lane')}")
            return store.learn({"code": code, "when": "any"},
                               f"{code}: cleared after the {issue.get('lane')} lane acted (see tried)",
                               "worked", ev, f"run:{run_name}", tried=tried)
        if event == "recurred" and issue.get("recurrences", 0) >= 2:
            ev = f"{run_name} {issue.get('last_seen')}: {code} back {issue['recurrences']} times: {issue['text'][:160]}"
            return store.learn({"code": code, "when": "any"},
                               f"{code} keeps coming back after it is cleared: the remedy tried does not hold "
                               f"({tried[:120] or 'nothing logged'})", "failed", ev, f"run:{run_name}", tried=tried)
        return None
    if code != "refusal_retried_unchanged":
        return None
    cause = (issue.get("numbers") or {}).get("code")
    tool = issue.get("tool")
    if not cause or not tool or cause == "not_applied" or error_kind(cause) == "technical":
        return None
    if event == "resolved" and issue.get("passed_after"):
        ev = f"{run_name} {issue.get('resolved_at')}: {tool} passed after {issue['passed_after']} refusals ({cause})"
        return store.learn({"tool": tool, "cause": cause},
                           f"{tool} refused with {cause} passes later unchanged: the cause is transient",
                           "worked", ev, "any", confidence=0.5,
                           executor={"treat_as": "transient",
                                     "advice": f"retry {tool} soon after {cause}; do not give it up"})
    if event == "overdue" and (issue.get("numbers") or {}).get("times", 0) >= 4:
        times = issue["numbers"]["times"]
        ev = f"{run_name} {issue.get('last_seen')}: {tool} refused {times} times with {cause}, never passed"
        return store.learn({"tool": tool, "cause": cause},
                           f"{tool} refused with {cause} does not pass when sent again unchanged",
                           "failed", ev, "any", confidence=0.5,
                           executor={"treat_as": "permanent",
                                     "advice": f"change the order: {tool} with {cause} does not pass unchanged"})
    return None
