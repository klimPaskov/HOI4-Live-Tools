"""Escalation (Tier 1) and gate packets (Tier 2): compact pages built by code for a model.

Tier 1. An issue is escalated when
  - it passes its deadline while still open (nothing acted, or the lane acted and it stayed),
  - it comes back a second time after it was resolved, or
  - its kind is a judgement between named alternatives that no rule ranks (issue_rules.ALTERNATIVES)
    and it passed its deadline.
The PACKET is one self-contained page: what is wrong, the numbers and the trend, the last relevant
actions and refusals, the lessons that match, and the ALLOWED RESPONSES: one of the listed server
actions, a plan edit in the live-edit log format, or "leave it" with a reason. A packet's size is
bounded (MAX_PACKET_CHARS). Every escalation is logged in RUN/escalations.jsonl with its tokens and
seconds once answered, and counted against a budget per game year (ESCALATIONS_PER_YEAR).

Delivery: the packet goes into issues_open.json (`packet`), which the executor turns into a row of
the player's wake (the omissions list), so in a campaign the player model (already a Haiku) answers
it at its next wake and no second model runs. Without a player, `answer_cmd` (a command reading the
packet on stdin and printing the JSON answer) can be configured; the answer is validated and logged.

Tier 2. At a declaration, a capitulation, a new game year and a run-ending failure, a GATE PACKET
(RUN/GATE.md and RUN/gates/<date>-<kind>.md) summarises what happened, the issues opened and closed
since the last gate, the overdue ones, the counters, and the costs: one small file per gate for a
reviewing model or a person, never the raw logs.
"""
from __future__ import annotations

import json
import os
import subprocess
import time

from . import issue_rules as R
from . import answer
from .runlogs import game_days

# A model is asked for every acted-but-open issue past its deadline (Tier 1) and every overdue high-severity
# issue (Tier 2); the money caps (budget.py: per run, per day, the floor) bound the spend, this count only
# stops a runaway (owner, 27 Nov 2026: use the API where it buys reliability; 24 a game year left most
# acted-but-open issues unanswered in t1939l/t1939m).
ESCALATIONS_PER_YEAR = 400
MAX_PACKET_CHARS = 5000          # about 1,300 tokens
ANSWER_KINDS = ("action", "plan_edit", "leave")
TIER1_CLASSES = ("idling", "gameplay", "tool_use")
COOLDOWN_DAYS = 14       # one escalation of a kind per two game weeks: the answer covers its siblings
ACTED = "the owning lane's verified orders did not make it go: the remedy did not hold"
HIGH = "high severity and past its deadline"
EVERY = (ACTED, HIGH)    # asked per issue: no sibling cooldown, no lane bundle


def allowed_responses(issue):
    tools = R.LANE_TOOLS.get(issue.get("lane") or "", [])
    if issue.get("tool") and issue["tool"] not in tools:
        tools = [issue["tool"]] + tools
    return tools


def build_packet(issue, recent=(), lessons=(), trend=None, date=None):
    """One page for a small model: built by code, self-contained."""
    alts = R.ALTERNATIVES.get(issue.get("code"), [])
    lines = [
        f"ESCALATION {issue['id']} at {date or issue.get('last_seen')}: {issue['class']} / {issue['code']} "
        f"({issue['severity']})",
        f"WHAT: {issue['text'][:400]}",
        f"SINCE: {issue.get('first_seen')} (deadline {issue.get('deadline')}: {issue.get('deadline_reason', '')[:160]})",
        f"STATE: {issue.get('state')}; owner lane {issue.get('lane') or '-'}; verified orders of that lane since: "
        f"{issue.get('acts', 0)}; recurrences {issue.get('recurrences', 0)}",
    ]
    if issue.get("acts"):
        lines.append(f"NOTE: the issue is still seen after those orders ({issue.get('no_effect', 0)} time(s) more than "
                     f"a few game days after one): they did not fix it. 'The lane already acted' is no reason to leave it.")
    if issue.get("lane_reason") or issue.get("lane_outcome"):
        out = issue.get("lane_outcome") or {}
        lines.append(f"LANE SAYS ({out.get('outcome') or 'its rule note'}): "
                     f"{(issue.get('lane_reason') or out.get('reason') or '')[:400]}")
    if issue.get("numbers"):
        lines.append("NUMBERS: " + json.dumps({k: v for k, v in issue["numbers"].items() if k != "owner_note"},
                                              ensure_ascii=False)[:300])
    if trend:
        lines.append("TREND: " + str(trend)[:300])
    if issue.get("tried"):
        lines.append("TRIED: " + " | ".join(issue["tried"][-5:])[:500])
    for r in list(recent)[-5:]:
        lines.append("RECENT: " + str(r)[:200])
    for le in list(lessons)[:3]:
        lines.append(f"LESSON {le['id']} (confidence {le.get('confidence', 0):.2f}): {le.get('statement', '')[:220]}")
    lines.append("ALLOWED RESPONSES (answer with ONE JSON object, nothing else):")
    tools = allowed_responses(issue)
    if tools:
        lines.append('  {"kind":"action","tool":<one of ' + ", ".join(tools) + '>,"args":{...},"why":"..."}')
    lines.append('  {"kind":"plan_edit","path":"<plan key path>","value":<new value>,"why":"..."}')
    lines.append('  {"kind":"leave","why":"<why it is right to leave it: the finding is wrong, or the wait has a named '
                 'end (what closes it and by when)>"}')
    if alts:
        lines.append("NAMED ALTERNATIVES (rank them; pick one):")
        lines += [f"  {i + 1}. {a}" for i, a in enumerate(alts)]
    text = "\n".join(lines)
    return text[:MAX_PACKET_CHARS]


def deliver(issue, rec):
    """A model's checked answer goes with the issue (issues_open.json `answer`, and first in the
    packet the executor puts in the player's wake): an action is what to do now, a leave is the
    issue's written reason. Before, the answer stayed in escalations.jsonl and reached no one."""
    ans = rec.get("answer") if rec.get("answered") else None
    if not isinstance(ans, dict):
        return
    issue["answer"] = ans
    if ans.get("kind") == "leave":
        issue["lane_reason"] = f"model: {str(ans.get('why'))[:280]}"
    head = f"MODEL ANSWER: {json.dumps(ans, ensure_ascii=False)[:500]}"
    issue["packet"] = (head + "\n" + (issue.get("packet") or ""))[:1200]


def validate_answer(text, issue):
    """Parse and check a model's answer against the allowed responses: (answer or None, why)."""
    try:
        start, end = text.index("{"), text.rindex("}") + 1
        ans = json.loads(text[start:end])
    except ValueError:
        return None, "no JSON object in the answer"
    kind = ans.get("kind")
    if kind not in ANSWER_KINDS:
        return None, f"kind {kind!r} is not one of {ANSWER_KINDS}"
    if kind == "action":
        if ans.get("tool") not in allowed_responses(issue):
            return None, f"tool {ans.get('tool')!r} is not allowed for this issue"
        if not isinstance(ans.get("args"), dict):
            return None, "an action needs args"
    if kind == "plan_edit" and not ans.get("path"):
        return None, "a plan edit needs a path"
    if not ans.get("why"):
        return None, "every answer gives its reason"
    return ans, "ok"


class Escalator:
    def __init__(self, out_dir, per_year=ESCALATIONS_PER_YEAR, answer_cmd=None, lessons=None, run_name="",
                 live=True):
        self.out = out_dir
        self.per_year = per_year
        # live: a model may be asked (a replay never asks one: it would spend money on the past)
        self.live = live
        self.answer_cmd = answer_cmd or os.environ.get("MONITOR_ANSWER_CMD")
        self.lessons = lessons
        self.run_name = run_name
        self.used = {}           # year -> count
        self.last_by_code = {}   # issue code -> game days of its last escalation
        self.log = []
        self.recent = []         # recent refusals and actions for packets
        self.lane_packets = {}   # lane -> (game days, first issue id, packet path) of its open review
        self.asked = set()       # issue ids asked per issue (EVERY): one answer each

    def note(self, line):
        self.recent.append(line)
        del self.recent[:-30]

    def trigger(self, event, issue):
        """Why this event needs a model, or None. A model is asked only where a rule cannot answer:
        a judgement between named alternatives, a remedy that the owning lane tried and that did not
        hold, an issue no executor lane owns, or one the owning lane left past twice its deadline."""
        if not issue.get("escalate", True) or issue["class"] not in TIER1_CLASSES or issue.get("lane") == "gate":
            return None
        if event == "recurred" and issue.get("recurrences", 0) >= 3 and issue.get("was_overdue"):
            # a flicker (a slot idle for an hour at a read) recurs too; a recurrence that also
            # outlived its deadline is a remedy that does not hold
            return "recurring after resolution"
        if event == "overdue" and issue.get("code") in R.ALTERNATIVES and issue.get("severity") == "high":
            return "a judgement between named alternatives"
        if event == "overdue" and issue.get("acts"):
            # the lane acted and the issue outlived its deadline: the remedy did not hold
            return ACTED
        if event == "overdue" and issue["class"] in ("idling", "gameplay") and (
                issue.get("lane_outcome") or issue.get("lane_reason") or issue.get("acts")
                or any(R.LANE_KINDS.get(issue.get("lane") or "", ()))):
            # offered to its lane (a turn, an order, or a written reason to wait) and still open past
            # its deadline: the loop notice -> act did not close; whatever the severity, a model
            # weighs the lane's own reason against the allowed responses
            return "offered to its lane and still open past its deadline"
        if issue.get("severity") != "high" and event != "stale":
            return None
        if event == "overdue":
            if issue.get("code") in R.ALTERNATIVES:
                return "a judgement between named alternatives"
            if not any(R.LANE_KINDS.get(issue.get("lane") or "", ())):
                return "no executor lane owns it"
            return HIGH
        if event == "stale" and issue.get("severity") in ("high", "medium") and not issue.get("acts"):
            return "the owning lane did not act by twice its deadline"
        return None

    def consider(self, event, issue, date, at_war=None):
        why = self.trigger(event, issue)
        if not why:
            return None
        year = (date or "?")[:4]
        code = R.family(issue.get("code"))
        last = self.last_by_code.get(code)
        days = game_days(date) if date else None
        lane = issue.get("lane") or ""
        every = why in EVERY
        if every and issue["id"] in self.asked:
            return None   # this issue was asked once already: its answer stands
        if not every and why.startswith("offered to its lane") and lane in self.lane_packets and days is not None and \
                days - self.lane_packets[lane][0] < COOLDOWN_DAYS:
            # the lane's open packet takes this issue too: one model answer per lane review, so a
            # backlog of a lane's overdue issues is weighed together inside the yearly budget
            first, path = self.lane_packets[lane][1], self.lane_packets[lane][2]
            with open(path, "a", encoding="utf-8") as f:
                f.write(f"ALSO {issue['id']} {issue.get('code')}: {issue['text'][:240]}\n"
                        f"  LANE SAYS: {(issue.get('lane_reason') or '-')[:240]}\n")
            issue["packet"] = f"bundled into {first}"
            rec = {"date": date, "id": issue["id"], "key": issue["key"], "tier": 1, "why": why,
                   "bundled_into": first}
            self._write(rec)
            return rec
        if not every and last is not None and days is not None and days - last < COOLDOWN_DAYS:
            return None  # the same kind was escalated lately: its answer covers this one too
        if self.used.get(year, 0) >= self.per_year:
            self._write({"date": date, "id": issue["id"], "key": issue["key"], "tier": 1, "why": why,
                         "skipped": "budget for the game year used"})
            return None
        self.used[year] = self.used.get(year, 0) + 1
        if every:
            self.asked.add(issue["id"])
        # Tier 2 (the strong model) for a high-severity issue past its deadline, Tier 1 for the rest
        tier = 2 if issue.get("severity") == "high" and event == "overdue" else 1
        if days is not None:
            self.last_by_code[code] = days
        lessons = []
        if self.lessons is not None:
            lessons = self.lessons.match(code=issue.get("code"), tool=issue.get("tool"),
                                         cause=(issue.get("numbers") or {}).get("code"), at_war=at_war)
        rel = [r for r in self.recent if issue.get("tool") and issue["tool"] in r or
               (issue.get("lane") and issue["lane"] in r)]
        packet = build_packet(issue, rel, lessons, date=date)
        issue["packet"] = packet[:1200]
        os.makedirs(os.path.join(self.out, "escalations"), exist_ok=True)
        path = os.path.join(self.out, "escalations", f"{issue['id']}-{(date or '').replace('.', '')}.md")
        with open(path, "w", encoding="utf-8") as f:
            f.write(packet + "\n")
        if why.startswith("offered to its lane") and days is not None:
            self.lane_packets[issue.get("lane") or ""] = (days, issue["id"], path)
        rec = {"date": date, "id": issue["id"], "key": issue["key"], "tier": tier, "why": why,
               "packet_chars": len(packet), "packet_tokens_est": len(packet) // 4, "lessons": [x["id"] for x in lessons],
               "delivered": "player wake (issues_open.json packet)"}
        if self.live and self.answer_cmd:
            rec.update(self._ask(packet, issue))
        elif self.live and answer.available() is None:
            rec.update(self._ask_api(packet, issue, tier))
        deliver(issue, rec)
        self._write(rec)
        if rec.get("tokens") and rec.get("delivered") != "api":
            self._cost_row(rec)  # the run's own cost file (the API path wrote its row already)
        return rec

    def _ask_api(self, packet, issue, tier=1):
        """An escalation on the API key (answer.py: budget checked first, usage written to the ledger):
        Tier 1 the small model, Tier 2 the strong one, both answering in the escalation format."""
        out = answer.ask(packet, tier=tier, run=self.run_name, role="escalation", system=answer.SYSTEM[1])
        if "error" in out:
            return {"answered": False, "error": out["error"], "seconds": out.get("seconds")}
        ans, why = validate_answer(out["result"], issue)
        u = out["usage"]
        self._cost_row({"tokens": {"input": u["input_tokens"], "output": u["output_tokens"],
                                   "cache_read": u["cache_read_input_tokens"],
                                   "cache_write": u["cache_creation_input_tokens"], "cost_usd": out["usd"]},
                        "model": out["model"], "seconds": out["seconds"]})
        return {"answered": ans is not None, "answer": ans, "check": why, "model": out["model"],
                "tokens": {"input": u["input_tokens"], "output": u["output_tokens"], "cost_usd": out["usd"]},
                "seconds": out["seconds"], "delivered": "api", "ledger": True}

    def _cost_row(self, rec):
        t = rec.get("tokens") or {}
        row = {"role": "escalation", "model": rec.get("model") or "answer_cmd", "input": t.get("input"),
               "output": t.get("output"), "cache_read": t.get("cache_read"), "cache_write": t.get("cache_write"),
               "seconds": rec.get("seconds"), "usd": t.get("cost_usd")}
        with open(os.path.join(self.out, "escalation_costs.jsonl"), "a", encoding="utf-8") as f:
            f.write(json.dumps(row) + "\n")

    def _ask(self, packet, issue):
        t0 = time.time()
        try:
            p = subprocess.run(self.answer_cmd, shell=True, input=packet, capture_output=True, text=True, encoding="utf-8",
                               timeout=300)
            out = p.stdout
        except (OSError, subprocess.SubprocessError) as ex:
            return {"answered": False, "error": str(ex)[:200], "seconds": round(time.time() - t0, 1)}
        tokens = None
        ledger = False
        try:  # a command printing a JSON envelope with usage (claude -p --output-format json)
            env = json.loads(out)
            if isinstance(env, dict) and "result" in env:
                u = env.get("usage") or {}
                tokens = {"input": u.get("input_tokens"), "output": u.get("output_tokens"),
                          "cache_read": u.get("cache_read_input_tokens"), "cost_usd": env.get("total_cost_usd")}
                out = env.get("result") or ""
                ledger = bool(env.get("ledger"))
        except ValueError:
            pass
        ans, why = validate_answer(out, issue)
        return {"answered": ans is not None, "answer": ans, "check": why, "tokens": tokens,
                "seconds": round(time.time() - t0, 1), "delivered": "answer_cmd", "ledger": ledger}

    def _write(self, rec):
        self.log.append(rec)
        with open(os.path.join(self.out, "escalations.jsonl"), "a", encoding="utf-8") as f:
            f.write(json.dumps(rec, ensure_ascii=False) + "\n")


# --- gate packets (Tier 2) ------------------------------------------------------------------------

GATE_TRIGGERS = ("war_declared", "capitulated", "war_ended", "new_year", "run_end", "failure", "days_60")
GATE_EVERY_DAYS = 60     # a gate every 60 game days between the event gates


def gate_kind(prev_date, snap, failure=None, last_gate=None):
    """The gate this snapshot opens, if any (`last_gate`: the game date of the last gate written)."""
    if failure:
        return "failure"
    trig = str(snap.get("trigger") or "")
    for k in ("war_declared", "capitulated", "war_ended"):
        if k in trig:
            return k
    if prev_date and snap.get("date") and str(snap["date"])[:4] != str(prev_date)[:4]:
        return "new_year"
    if trig == "end":
        return "run_end"
    now, last = game_days(snap.get("date")), game_days(last_gate) if last_gate else None
    if now is not None and last is not None and now - last >= GATE_EVERY_DAYS:
        return "days_60"
    return None


def silent_lanes(detector):
    """Open issues whose owning lane is past the deadline and has said nothing: the lane_silent
    findings, and rows recorded before them that quote "no reason given"."""
    out = []
    for i in detector.open_issues():
        text = str(i.get("text") or "")
        if i.get("code") == "lane_silent" or (i.get("overdue") and ("no reason given" in text or "SILENT:" in text)):
            out.append(i)
    return out


def lane_turns_line(snap):
    """Turns per lane as the executor reports them (its `lane_turns` in the watcher's brief): the
    executor's pace first (ticks and lane turns per game day), then the lanes that never had a
    turn or waited longest, each with its turns per game day, so a starved lane shows at the gate
    (t1939o, 27 Nov 2026: decisions, resources and intelligence had no turn in 56 days; w92b: one
    lane per tick, about two ticks a game day)."""
    all_rows = [r for r in snap.get("lane_turns") or [] if isinstance(r, dict) and r.get("lane")]
    rows = [r for r in all_rows if r.get("kind") != "ticks"]
    pace = next((r for r in all_rows if r.get("kind") == "ticks"), None)
    if not rows and not pace:
        return ""

    def wait(r):
        return (r.get("turns") or 0) > 0, -(r.get("days_since") if r.get("days_since") is not None else 10 ** 6)

    parts = []
    for r in sorted(rows, key=wait):
        last = r.get("days_since")
        when = "never" if not r.get("turns") else f"{last}d ago" if last is not None else "?"
        rate = f", {r['per_day']}/day" if r.get("per_day") is not None else ""
        cost = ", input" if r.get("input") else ""
        parts.append(f"{r['lane']} {r.get('turns') or 0} ({when}{rate}{cost}{', silent' if r.get('silent') else ''})")
    head = ""
    if pace:
        head = (f"Executor pace: {pace.get('ticks_per_day')} ticks and {pace.get('lane_turns_per_day')} lane turns "
                f"per game day ({pace.get('ticks')} ticks, {pace.get('lane_turns')} turns). ")
    return head + "Turns per lane (turns, last, per game day): " + "; ".join(parts)


def starved_lanes(snap):
    """Lanes the executor reports past the longest wait any of its rules gives a lane (`starved`):
    a lane with work that had no turn for that long is a failure of the scheduler."""
    return [r for r in snap.get("lane_turns") or []
            if isinstance(r, dict) and r.get("lane") and r.get("kind") != "ticks" and r.get("starved")]


def equipment_fill_lines(land):
    """Equipment fill per army at a declaration: the mean and lowest fill of its divisions and how
    many stand below three quarters (the share below which the game's own division attacks no
    more; w92b, 28 Nov 2026: the infantry went into the West at 0.27 after a template edit at war)."""
    by_army = {}
    for d in land.get("divisions") or []:
        f = d.get("equipment_fill")
        if isinstance(f, (int, float)) and d.get("army"):
            by_army.setdefault(d["army"], []).append(float(f))
    rows = []
    for a in land.get("armies") or []:
        fills = by_army.get(a.get("army")) or []
        if fills:
            mean = sum(fills) / len(fills)
            low = sum(1 for f in fills if f < 0.75)
            rows.append((mean, f"- {a.get('army')}: equipment fill mean {mean:.2f}, lowest {min(fills):.2f}, "
                               f"{low} of {len(fills)} division(s) below 0.75"))
        elif isinstance(a.get("equipment_fill_min"), (int, float)):
            rows.append((a["equipment_fill_min"], f"- {a.get('army')}: equipment fill lowest "
                                                  f"{a['equipment_fill_min']:.2f} (no division read)"))
    if not rows:
        return ["Equipment fill per army: not read"]
    rows.sort(key=lambda r: r[0])
    return ["Equipment fill per army (lowest first):"] + [r[1] for r in rows]


def gate_packet(kind, snap, detector, since_seq, costs_line="", cleanliness="", budget_line=""):
    """One page: what happened, issues opened and closed since the last gate, overdue, counters, costs."""
    w = snap.get("wars") or {}
    per = w.get("per_enemy") or {}
    lines = [f"# GATE {kind} at {snap.get('date')} ({snap.get('trigger') or ''})", ""]
    if budget_line:
        lines += [budget_line, ""]
    lines.append(f"At war: {bool(w.get('at_war'))}; enemies {', '.join(sorted(w.get('enemies') or [])[:10]) or '-'}; "
                 f"own states lost: {len(w.get('own_states_lost') or [])}")
    for tag, e in sorted(per.items(), key=lambda kv: -(kv[1].get('casualties_own') or 0))[:6]:
        lines.append(f"- {tag}: VP held {e.get('vp_held_by_us')}/{e.get('vp_total')}, casualties ours "
                     f"{e.get('casualties_own')} theirs {e.get('casualties_enemy')}")
    land = snap.get("land") or {}
    if land.get("division_count") is not None:
        lines.append(f"Divisions {land.get('division_count')}; armies {len(land.get('armies') or [])}")
    if kind == "war_declared":
        lines += equipment_fill_lines(land)
    opened = [i for i in detector.issues.values() if int(i["id"][1:]) > since_seq]
    closed = [i for i in detector.issues.values() if i["state"] == "resolved" and i.get("resolved_at")
              and int(i["id"][1:]) <= since_seq]
    over = [i for i in detector.open_issues() if i["overdue"]]
    lines += ["", f"Issues opened since the last gate: {len(opened)}; earlier ones resolved: {len(closed)}; "
                  f"open now: {len(detector.open_issues())}; overdue: {len(over)}"]
    for i in over[:10]:
        lines.append(f"- OVERDUE {i['id']} {i['class']} {i['code']} since {i['first_seen']}: {i['text'][:150]}")
    for i in [i for i in opened if i["severity"] == "high" and not i["overdue"]][:8]:
        lines.append(f"- new {i['id']} {i['class']} {i['code']}: {i['text'][:150]}")
    # a lane past an issue's deadline that states no reason is a failure of the rule, counted
    # apart from the waits (t1939n, 27 Nov 2026: 20 overdue rows read "no reason given")
    silent = silent_lanes(detector)
    lines.append(f"Silent lanes past their deadline (FAIL while any): {len(silent)}")
    for i in silent[:10]:
        lines.append(f"- SILENT {i['id']} lane {i.get('lane')} {i.get('subject') or i['code']}: {i['text'][:150]}")
    starved = starved_lanes(snap)
    if snap.get("lane_turns"):
        lines.append(f"Starved lanes, no turn past the longest wait (FAIL while any): {len(starved)}"
                     + (": " + ", ".join(f"{r['lane']} ({r.get('days_since')}d)" for r in starved[:12]) if starved else ""))
    turns = lane_turns_line(snap)
    if turns:
        lines.append(turns)
    counts = detector.count_table()
    year = (snap.get("date") or "?")[:4]
    if year in counts:
        cl = counts[year]
        lines.append("Counters this year: " + "; ".join(
            f"{c} opened {int(cl[c].get('open', 0))}, resolved {int(cl[c].get('resolved', 0))}, overdue "
            f"{int(cl[c].get('overdue', 0))}" for c in R.CLASSES if c in cl))
    idle = getattr(detector, "idle", None)
    if idle is not None:
        lines += ["", idle.table(snap.get("lane_turns"))]
    if cleanliness:
        lines += ["", cleanliness.strip()[:1200]]
    if costs_line:
        lines += ["", costs_line.strip()[:800]]
    return "\n".join(lines) + "\n"
