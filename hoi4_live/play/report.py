"""The end-of-run report: report.md in the run folder, from the run's own records.

    calls.jsonl         every call the supervisor answered (no tokens)
    game-actions.jsonl  the native runtime's action journal: one terminal receipt per action
    native.stderr       the runtime's [action] lines (self_pause, restored)
    supervisor.log      starts, waits, idle wakes and pauses, pause requests, executor stops, the end
"""

import collections
import json
import os
import re

from . import runs

RECEIPTS = ("Verified", "NotApplied", "Diverged", "Unconfirmed")


def _jsonl(path):
    rows = []
    try:
        with open(path, encoding="utf-8", errors="replace") as f:
            for line in f:
                try:
                    rows.append(json.loads(line))
                except ValueError:
                    continue
    except OSError:
        pass
    return rows


def receipts(run):
    """Count of each terminal receipt in the action journal (the last receipt of each action counts)."""
    last = {}
    for row in _jsonl(os.path.join(run, "game-actions.jsonl")):
        if row.get("status") in RECEIPTS and row.get("id"):
            last[row["id"]] = row["status"]
    return collections.Counter(last.values())


def repeated_failures(run, least=2):
    """Actions whose last receipt is NotApplied or Diverged, grouped by the action itself: (count, kind, action
    text, statuses) for each action that failed at least `least` times, most first. The same order failing again
    and again is a defect to look at, not bad luck."""
    last, action = {}, {}
    for row in _jsonl(os.path.join(run, "game-actions.jsonl")):
        if row.get("id") and row.get("status") in RECEIPTS:
            last[row["id"]] = row["status"]
            action[row["id"]] = row.get("action") or {}
    groups = collections.defaultdict(list)
    for rid, status in last.items():
        if status in ("NotApplied", "Diverged"):
            groups[json.dumps(action[rid], sort_keys=True)].append(status)
    rows = [(len(v), json.loads(k).get("kind", "?"), k, collections.Counter(v)) for k, v in groups.items()
            if len(v) >= least]
    return sorted(rows, key=lambda r: (-r[0], r[1], r[2]))


def self_pauses(run):
    """(self-pauses, restored) from the runtime's [action] lines."""
    paused = restored = 0
    try:
        with open(os.path.join(run, "native.stderr"), encoding="utf-8", errors="replace") as f:
            for line in f:
                if line.startswith("[action]") and '"self_pause":true' in line:
                    paused += 1
                    restored += '"restored":true' in line
    except OSError:
        pass
    return paused, restored


def idle_pauses(events):
    """One line per idle pause: when, the game date, why, and when the next player call lifted it."""
    lines, paused = [], None
    for e in events:
        if e.get("event") == "idle_pause":
            paused = e
        elif e.get("event") == "idle_resume" and paused:
            lines.append(_idle_line(paused, e))
            paused = None
    if paused:
        lines.append(_idle_line(paused, None))
    return lines


def _idle_line(pause, resume):
    if resume is None:
        lifted = "still paused when the run ended"
    else:
        lifted = "lifted %.1f minutes later by the next player call%s" % (
            (resume.get("t", 0) - pause.get("t", 0)) / 60.0,
            " (the game was running again)" if resume.get("game_resumed") else "")
    return "- %s (game date %s): %s; %s; %s" % (
        pause.get("at") or "?", pause.get("date") or "?", pause.get("reason") or "no player call",
        "the game was paused" if pause.get("game_paused") else "the game was already paused", lifted)


def capitulation_aims(plan):
    """[(tag, VP needed or None)] for each country the plan's goal capitulates: goal.capitulation's enemy with
    its vp_needed (the threshold the plan aims at, after the levers that lower it), then every other
    capitulated:TAG predicate with no aim."""
    goal = plan.get("goal") if isinstance(plan.get("goal"), dict) else {}
    cap = goal.get("capitulation") if isinstance(goal.get("capitulation"), dict) else {}
    out = []
    if isinstance(cap.get("enemy"), str):
        out.append((cap["enemy"], cap.get("vp_needed") if isinstance(cap.get("vp_needed"), int) else None))
    for p in goal.get("predicates") or []:
        if isinstance(p, str) and p.startswith("capitulated:"):
            tag = p.split(":", 1)[1].strip()
            if tag and all(tag != t for t, _ in out):
                out.append((tag, None))
    return out


def capitulation_lines(rows):
    """Progress against the VP the plan needs to capitulate each enemy, not against the enemy's total."""
    lines = []
    for r in rows:
        held, total, needed = r.get("held") or 0, r.get("total") or 0, r.get("needed")
        if needed:
            lines.append("- %s: held %g of the %d VP needed to capitulate (%s; %g VP in all)" % (
                r.get("tag"), held, needed, "reached" if held >= needed else "%g to go" % (needed - held), total))
        else:
            lines.append("- %s: held %g VP of %g in all; the plan states no capitulation aim "
                         "(goal.capitulation.vp_needed)" % (r.get("tag"), held, total))
    return lines


def planner_section(run):
    """The rolling planner's updates, proposers, gates and model use (RUN/planner/)."""
    summary = runs.read_json(os.path.join(run, "planner", "summary.json"))
    journal = _jsonl(os.path.join(run, "planner", "journal.jsonl"))
    if not summary and not journal:
        return []
    out = ["", "## Rolling plan", ""]
    applied = [j for j in journal if j.get("version")]
    by = collections.Counter(j.get("applied") for j in applied)
    secs = [j.get("seconds") or 0 for j in journal]
    out.append("- Updates: %d (%d applied: %s), triggers %s" % (
        len(journal), len(applied), ", ".join("%s %d" % kv for kv in by.most_common()) or "none",
        ", ".join("%s %d" % kv for kv in sorted(((summary or {}).get("triggers") or {}).items())) or "none"))
    if secs:
        out.append("- Update wall time: median %.1f s, max %.1f s" % (sorted(secs)[len(secs) // 2], max(secs)))
    b = (summary or {}).get("budget") or {}
    if b:
        out.append("- Model use: %d calls, %.0f s, %d tokens (%d escalations); per hour %s calls, %s s, %s tokens; "
                   "ceiling %s" % (b.get("model_calls", 0), b.get("model_seconds", 0), b.get("tokens", 0),
                                   b.get("escalations", 0), b["per_hour"]["calls"], b["per_hour"]["seconds"],
                                   b["per_hour"]["tokens"], json.dumps(b.get("limits"))))
    skipped = sum(1 for j in journal for p in j.get("proposals") or [] if str(p.get("result", "")).startswith("skipped"))
    rejected = sum(1 for j in journal for p in j.get("proposals") or []
                   if str(p.get("result", "")).startswith(("refused", "gates")))
    out.append("- Proposals refused by the gates: %d; model calls skipped at the ceiling: %d; rollbacks: %d"
               % (rejected, skipped, (summary or {}).get("rollbacks", 0)))
    for j in applied[-12:]:
        out.append("  - v%s %s (%s): %s" % (j["version"], j.get("date"), j.get("applied"), (j.get("summary") or "")[:160]))
    return out


def _day_number(text):
    """Days since 1 Jan 1 of a game date "YYYY.MM.DD[.HH]" or "YYYY-MM-DD" (the game has no leap years); None
    when unreadable."""
    try:
        parts = [int(x) for x in re.split(r"[.-]", str(text))]
        year, month, day = parts[0], parts[1], parts[2]
        hour = parts[3] if len(parts) > 3 else 0
    except (ValueError, IndexError):
        return None
    before = (0, 31, 59, 90, 120, 151, 181, 212, 243, 273, 304, 334)
    if not 1 <= month <= 12:
        return None
    return year * 365 + before[month - 1] + day + hour / 24.0


def pace(start_date, end_date, ready_t, ended):
    """Game days per wall minute from the run's start (the supervisor's ready event) to its end; None when either
    is unknown. A machine short of memory runs the same campaign at half the pace or less, so a result is read
    against it."""
    a, b = _day_number(start_date), _day_number(end_date)
    if a is None or b is None or ready_t is None or ended - ready_t < 60:
        return None
    return (b - a) / ((ended - ready_t) / 60.0)


def machine_line(events):
    """Load and swap when the run started (the supervisor's machine event)."""
    m = next((e for e in events if e.get("event") == "machine"), None)
    if not m:
        return "unknown"
    parts = []
    if m.get("load"):
        parts.append("load %s on %s CPUs" % (" / ".join(map(str, m["load"])), m.get("cpus") or "?"))
    if m.get("swap_total_gb"):
        parts.append("swap %.1f of %.1f GB in use" % (m.get("swap_used_gb") or 0, m["swap_total_gb"]))
    return ", ".join(parts) or "unknown"


def build(run, reason, counts, final, started, ended):
    request = runs.read_json(os.path.join(run, "request.json")) or {}
    calls = _jsonl(os.path.join(run, "calls.jsonl"))
    events = _jsonl(os.path.join(run, "supervisor.log"))
    tools = collections.Counter(c.get("tool") for c in calls if c.get("source") == "player")
    failed = collections.Counter(c.get("code") for c in calls if c.get("isError") and c.get("code"))
    got = receipts(run)
    paused, restored = self_pauses(run)
    ready = next((e for e in events if e.get("event") == "ready"), {})
    start_date = ready.get("date")
    rate = pace(start_date, final.get("date"), ready.get("t"), ended)
    lines = [
        "# Run report: %s (%s)" % (request.get("country") or request.get("tag") or "campaign", request.get("tag", "?")),
        "",
        "- Goal: %s" % request.get("goal", "?"),
        "- Plan: %s, %s mode" % (request.get("plan", "?"), str(request.get("mode", "fair")).upper()),
        "- Ended: %s%s" % (reason, "".join(" (%s)" % e.get("message") for e in events
                                           if e.get("event") == "limit" and e.get("message"))[:400]),
        "- Wall time: %.1f minutes" % ((ended - started) / 60.0),
        "- Machine at the start: %s" % machine_line(events),
        "- Game dates: %s to %s" % (start_date or "?", final.get("date") or "?"),
        "- Pace: %s" % ("%.1f game days per wall minute (paused time included; a machine short of memory "
                        "plays at half its usual pace or less, so judge the result against it)" % rate
                        if rate is not None else "unknown"),
        "",
        "## Calls",
        "",
        "- Player calls: %d (%d distinct tools)" % (sum(tools.values()), len(tools)),
        "- Identical actions refused as already done: %d" % counts.get("already_done", 0),
        "- Game brought to the front before an action: %d" % counts.get("front", 0),
        "- Idle wakes raised (half the idle pause without a player call): %d" % counts.get("idle_wakes", 0),
        "- Idle pauses (the idle pause reached without a player call): %d" % counts.get("idle_pauses", 0),
        "- Pauses the player asked for: %d" % counts.get("pause_requests", 0),
        "- Executor stops reported by the runtime: %d" % counts.get("executor_stops", 0),
        "- Self-pauses recorded by the runtime: %d (%d restored at once)" % (paused, restored),
    ]
    if tools:
        lines.append("- Most used: " + ", ".join("%s %d" % kv for kv in tools.most_common(8)))
    if failed:
        lines.append("- Errors by code: " + ", ".join("%s %d" % kv for kv in failed.most_common(8)))
    paused_lines = idle_pauses(events)
    if paused_lines:
        lines += ["", "## Idle pauses", ""] + paused_lines
    lines += ["", "## Action receipts", ""]
    if got:
        lines += ["- %s: %d" % (name, got.get(name, 0)) for name in RECEIPTS]
    else:
        lines.append("- no action journal in the run folder")
    repeats = repeated_failures(run)
    if repeats:
        lines += ["", "## Actions that failed more than once", ""]
        lines += ["- %d x %s: %s (%s)" % (n, kind, text[:160], ", ".join("%s %d" % kv for kv in sorted(st.items())))
                  for n, kind, text, st in repeats[:12]]
    if final.get("capitulation"):
        lines += ["", "## Capitulation", ""] + capitulation_lines(final["capitulation"])
    lines += planner_section(run)
    stats = final.get("war_stats")
    if stats:
        lines += ["", "## War statistics at the end", "", "```", json.dumps(stats, indent=1)[:3000], "```"]
    changed = runs.compare_user_files(run)
    lines += ["", "## Your files", ""]
    if changed:
        lines += ["- %s: %s" % item for item in changed]
    else:
        lines.append("- the game's settings files and saves are as they were at the start")
    return "\n".join(lines) + "\n"


def write(run, reason, counts, final, started, ended):
    path = os.path.join(run, "report.md")
    runs.write_private(path, build(run, reason, counts, final, started, ended))
    return path
