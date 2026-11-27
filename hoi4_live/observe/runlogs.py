"""Incremental readers of a run's own logs, and the game clock that dates them.

Every reader keeps a byte offset and returns only what was appended since the last read, so a
monitor that lives as long as the run reads each line once. Nothing here talks to the game.

  calls.jsonl         every tool call through the run's broker (source player, monitor, recorder...)
  game-actions.jsonl  the executor's orders, one line per status (Dispatching, then the final one)
  mcp.stderr          the server's `[action]` lines: every tool the server ran, the executor's too,
                      with its error code and milliseconds
  health.log          the harness's memory lines (`rss=4.10G`)
  state/*.json        the recorder's snapshots

The clock maps wall time to game dates from every dated thing seen (snapshots, overview reads,
the `now` block of hoi4_observe / hoi4_wait_change answers).
"""
from __future__ import annotations

import bisect
import glob
import json
import os
import re

FINAL = ("Verified", "NotApplied", "Diverged", "Failed", "Refused", "Rejected", "Error")
CUM = [0, 31, 59, 90, 120, 151, 181, 212, 243, 273, 304, 334]


def game_days(date_text):
    """Game date text Y.M.D[.H] -> days on the game's 365-day calendar (None if unreadable)."""
    try:
        parts = [int(p) for p in str(date_text).split(".")[:4]]
    except ValueError:
        return None
    if len(parts) < 3:
        return None
    y, m, d = parts[:3]
    h = parts[3] if len(parts) > 3 else 0
    return y * 365 + CUM[max(1, min(12, m)) - 1] + (d - 1) + h / 24.0


def date_of_days(days):
    """Inverse of game_days, to the day: 'Y.M.D'."""
    y = int(days // 365)
    rest = days - y * 365
    m = max(i for i in range(12) if CUM[i] <= rest)
    return f"{y}.{m + 1:02d}.{int(rest - CUM[m]) + 1:02d}"


class Tail:
    """New complete lines of one append-only file since the last call."""

    def __init__(self, path):
        self.path = path
        self.offset = 0
        self.partial = b""

    def lines(self):
        try:
            size = os.path.getsize(self.path)
        except OSError:
            return []
        if size < self.offset:  # rotated or rewritten: start over
            self.offset, self.partial = 0, b""
        if size == self.offset:
            return []
        with open(self.path, "rb") as f:
            f.seek(self.offset)
            data = f.read(size - self.offset)
        self.offset = size
        data = self.partial + data
        parts = data.split(b"\n")
        self.partial = parts.pop()
        return [p.decode("utf-8", "replace") for p in parts if p.strip()]


def _json(line):
    try:
        v = json.loads(line)
    except ValueError:
        return None
    return v if isinstance(v, dict) else None


def body_date(body):
    """The game date a read's answer carries, if any."""
    if not isinstance(body, dict):
        return None
    d = body.get("date")
    if isinstance(d, dict) and d.get("text"):
        return str(d["text"])
    now = body.get("now")
    if isinstance(now, dict):
        d = now.get("date")
        if isinstance(d, str) and d:
            return d
        if isinstance(d, dict) and d.get("text"):
            return str(d["text"])
    return None


class Clock:
    """Wall seconds -> game date, from every dated observation (kept sorted)."""

    def __init__(self):
        self.utc = []
        self.dates = []

    def add(self, utc, date):
        if utc is None or not date or game_days(date) is None:
            return
        i = bisect.bisect_right(self.utc, utc)
        self.utc.insert(i, float(utc))
        self.dates.insert(i, str(date))

    def at(self, utc):
        if not self.utc:
            return None
        i = bisect.bisect_right(self.utc, utc) - 1
        return self.dates[max(i, 0)]

    def latest(self):
        return self.dates[-1] if self.dates else None


class RunLogs:
    """All the run's logs, read incrementally. `poll()` returns the new records by kind."""

    ACTION_RE = re.compile(r"^\[action\] ")
    RSS_RE = re.compile(r"rss=([0-9.]+)([GMK])")

    def __init__(self, run_dir):
        self.run_dir = run_dir
        self.calls = Tail(os.path.join(run_dir, "calls.jsonl"))
        self.actions = Tail(os.path.join(run_dir, "game-actions.jsonl"))
        self.server = Tail(os.path.join(run_dir, "mcp.stderr"))
        self.health = Tail(os.path.join(run_dir, "health.log"))
        self.clock = Clock()
        self.snap_seen = set()

    def poll(self):
        out = {"calls": [], "actions": [], "server": [], "rss": [], "snapshots": []}
        for line in self.calls.lines():
            c = _json(line)
            if c is None:
                continue
            self.clock.add(c.get("utc"), body_date(c.get("body")))
            out["calls"].append(c)
        for line in self.actions.lines():
            a = _json(line)
            if a is not None and a.get("status") in FINAL + ("Dispatching",):
                out["actions"].append(a)
        for line in self.server.lines():
            if line.startswith("[action] "):
                a = _json(line[9:])
                if a is not None:
                    out["server"].append(a)
        for line in self.health.lines():
            m = self.RSS_RE.search(line)
            if m:
                gb = float(m.group(1)) * {"G": 1.0, "M": 1 / 1024, "K": 1 / 1048576}[m.group(2)]
                out["rss"].append({"line": line[:8], "gb": gb, "date": self.clock.latest()})
        for path in sorted(glob.glob(os.path.join(self.run_dir, "state", "*.json"))):
            if path in self.snap_seen:
                continue
            try:
                with open(path, encoding="utf-8") as f:
                    snap = json.load(f)
            except (OSError, ValueError):
                continue  # being written: next poll
            self.snap_seen.add(path)
            self.clock.add(snap.get("utc"), snap.get("date"))
            out["snapshots"].append(snap)
        return out


def now_snapshot(call):
    """A light snapshot from the `now` block of a hoi4_observe / hoi4_wait_change answer, for runs
    recorded before the state recorder existed (or between its snapshots)."""
    body = call.get("body")
    now = body.get("now") if isinstance(body, dict) else None
    if not isinstance(now, dict) or not now.get("date"):
        return None
    occupied = now.get("own_states_occupied") or []
    return {
        "date": now["date"], "utc": call.get("utc"), "paused": now.get("paused"), "light": True,
        "omissions": now.get("omissions") or [],
        "lane_turns": now.get("lane_turns") or [],
        "research": {"idle_slots": now.get("idle_research_slots")},
        "economy": {"civilian_idle": now.get("civilian_factories_idle")},
        "wars": {"at_war": bool(now.get("at_war_with")), "enemies": now.get("at_war_with") or [],
                 "own_states_lost": [{"state": s} if not isinstance(s, dict) else s for s in occupied]},
    }
