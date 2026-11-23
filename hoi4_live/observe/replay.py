"""Snapshots of a past run rebuilt from its call log (calls.jsonl).

A run's broker logs every tool call with its reply body. `ReplayClient` answers
a read with the latest recorded body of that tool (and `query`) up to a point in
the log; `replay_run` walks the log and lets a Recorder poll at every recorded
game date, so the snapshots of an old run hold whatever its calls read (reads
nobody made come back as errors and show as missing).
"""
from __future__ import annotations

import json

from .snapshot import g


def _key(tool, args):
    q = (args or {}).get("query")
    return f"{tool}:{q}" if q else tool


class ReplayClient:
    def __init__(self, entries):
        self.entries = entries
        self.pos = 0
        self.latest = {}

    def advance(self, pos):
        while self.pos < pos:
            e = self.entries[self.pos]
            if not e.get("isError") and isinstance(e.get("body"), dict):
                self.latest[_key(e.get("tool"), e.get("args"))] = e["body"]
            self.pos += 1

    def __call__(self, tool, args):
        body = self.latest.get(_key(tool, args))
        if body is None and tool == "hoi4_observe":
            return {"error": "not recorded"}
        return body if body is not None else {"error": {"code": "not_recorded",
                                                        "message": f"{tool} was not read in this run"}}


def load_calls(path):
    out = []
    with open(path, encoding="utf-8", errors="replace") as f:
        for line in f:
            try:
                e = json.loads(line)
            except ValueError:
                continue
            if isinstance(e, dict) and e.get("tool"):
                out.append(e)
    out.sort(key=lambda e: e.get("utc") or 0)
    return out


def replay_run(calls_path, recorder):
    """Feed a Recorder (whose `call` is a ReplayClient over the same log) date by date."""
    entries = recorder.call.entries
    last_date = None
    written = []
    for i, e in enumerate(entries):
        if e.get("tool") != "hoi4_overview" or e.get("isError"):
            continue
        date = g(e, "body", "date", "text")
        if not date or date == last_date:
            continue
        recorder.call.advance(i + 1)
        last_date = date
        p = recorder.poll()
        if p:
            written.append(p)
    recorder.call.advance(len(entries))
    return written
