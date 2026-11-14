"""The end-of-run report: report.md in the run folder, from the run's own records.

    calls.jsonl         every call the supervisor answered (no tokens)
    game-actions.jsonl  the native runtime's action journal: one terminal receipt per action
    native.stderr       the runtime's [action] lines (self_pause, restored)
    supervisor.log      starts, waits, idle wakes and pauses, pause requests, executor stops, the end
"""

import collections
import json
import os

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


def build(run, reason, counts, final, started, ended):
    request = runs.read_json(os.path.join(run, "request.json")) or {}
    calls = _jsonl(os.path.join(run, "calls.jsonl"))
    events = _jsonl(os.path.join(run, "supervisor.log"))
    tools = collections.Counter(c.get("tool") for c in calls if c.get("source") == "player")
    failed = collections.Counter(c.get("code") for c in calls if c.get("isError") and c.get("code"))
    got = receipts(run)
    paused, restored = self_pauses(run)
    start_date = next((e.get("date") for e in events if e.get("event") == "ready"), None)
    lines = [
        "# Run report: %s (%s)" % (request.get("country") or request.get("tag") or "campaign", request.get("tag", "?")),
        "",
        "- Goal: %s" % request.get("goal", "?"),
        "- Plan: %s, %s mode" % (request.get("plan", "?"), str(request.get("mode", "fair")).upper()),
        "- Ended: %s" % reason,
        "- Wall time: %.1f minutes" % ((ended - started) / 60.0),
        "- Game dates: %s to %s" % (start_date or "?", final.get("date") or "?"),
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
