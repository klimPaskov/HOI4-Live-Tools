"""The state recorder: when to take a snapshot, and writing it.

`Recorder(run_dir, call)` where `call(tool, args) -> body` is any client of the
server's read lane (the run's broker socket, an MCP client, a fixture). Every
`poll_seconds` it reads the game date (hoi4_overview), the wars (hoi4_wars) and
the watcher's changes since its cursor (hoi4_observe), plus the executor's
plan_state.json, and takes a snapshot

  - on the first poll (trigger "start"),
  - when `every_days` game days have passed since the last one ("week"),
  - when a war is declared ("war_declared:TAG"), an enemy capitulates
    ("capitulated:TAG") or a war ends ("war_ended"), an own state is lost
    ("own_state_lost"), a peace conference opens, or an executor operation
    changes phase ("operation:<id>:<phase>").

Each snapshot goes to RUN/state/<YYYY.MM.DD.HH>.json and RUN/SITREP.txt is
rewritten from it. Only read-lane tools are called.
"""
from __future__ import annotations

import json
import os
import time

from . import findings as findings_mod
from . import mapfiles, runfiles, sitrep
from .reads import READS, SNAPSHOT_TOOL, STATES_TOOL
from .snapshot import build, days_between, g

MAX_STATE_TAGS = 16
TRIGGER_KINDS = {"war_started", "war_ended", "own_state_lost", "peace_conference_opened"}


def write_atomic(path, text):
    tmp = path + ".tmp"
    with open(tmp, "w") as f:
        f.write(text)
    os.replace(tmp, path)


class Recorder:
    def __init__(self, run_dir, call, game_dir=None, every_days=7.0, keep_in_memory=12, run_name=None,
                 snapshot_tool=None, run_files=True):
        self.run_dir = run_dir
        self.call = call
        self.every_days = every_days
        self.state_dir = os.path.join(run_dir, "state")
        os.makedirs(self.state_dir, exist_ok=True)
        self.prev = []
        self.keep = keep_in_memory
        self.cursor = None
        self.count = 0
        self.cache = {}
        self.last_date = None
        self.enemies = None
        self.won = None
        self.phases = None
        self.fought = set()
        self.run_name = run_name or os.path.basename(os.path.abspath(run_dir))
        self.snapshot_tool = snapshot_tool
        self.game_dir = game_dir
        self.run_files = run_files
        self.regions = mapfiles.state_regions(game_dir) if game_dir else {}
        from .history import load_run
        for s in load_run(run_dir)[-keep_in_memory:]:
            self.prev.append(s)
            self.last_date = s.get("date")
            for t, e in (g(s, "wars", "per_enemy", default={}) or {}).items():
                if e.get("casualties_own") or e.get("at_war"):
                    self.fought.add(t)

    # --- reads ------------------------------------------------------------------

    def _read(self, tool, args):
        t0 = time.monotonic()
        try:
            body = self.call(tool, args)
        except Exception as ex:  # a dead socket must not end the recording
            body = {"error": {"code": "call_failed", "message": str(ex)[:200]}}
        return body if isinstance(body, dict) else {"error": "no reply"}, round((time.monotonic() - t0) * 1000, 1)

    def read_all(self):
        raw, ms = {}, {}
        if self.snapshot_tool:
            body, t = self._read(self.snapshot_tool, {"since": self.cursor or 0})
            if isinstance(body.get("reads"), dict):
                ms["snapshot_tool"] = t
                raw = dict(body["reads"])
                ms.update(body.get("read_ms") or {})
        for key, tool, args, every in READS:
            if key in raw:
                continue
            if key in self.cache and every > 1 and self.count % every:
                raw[key] = self.cache[key]
                continue
            a = dict(args)
            if key == "observe" and self.cursor is not None:
                a["since"] = self.cursor
            raw[key], ms[key] = self._read(tool, a)
            if "error" not in raw[key]:
                self.cache[key] = raw[key]
        seq = g(raw, "observe", "seq")
        if isinstance(seq, int):
            self.cursor = seq
        player = g(raw, "overview", "player")
        enemies = list(g(raw, "wars", "wars", "enemies", default=[]) or [])
        tags = [player] if player else []
        # enemies at war first, then ones fought earlier in the run; casualties order them
        for t in enemies + sorted(self.fought):
            if t and t not in tags:
                tags.append(t)
        states = {}
        for t in tags[:MAX_STATE_TAGS]:
            states[t], ms[f"states:{t}"] = self._read(STATES_TOOL, {"query": t})
        return raw, states, ms

    # --- triggers ---------------------------------------------------------------

    def due(self):
        """Cheap poll: the reasons to take a snapshot now ([] = none)."""
        ov, _ = self._read("hoi4_overview", {})
        date = g(ov, "date", "text")
        if not date:
            return [], None
        why = []
        if self.last_date is None:
            why.append("start")
        else:
            d = days_between(self.last_date, date)
            if d is not None and d >= self.every_days:
                why.append("week")
        wars, _ = self._read("hoi4_wars", {})
        enemies = set(g(wars, "wars", "enemies", default=[]) or [])
        won = set(g(wars, "wars", "wars_won", default=[]) or [])
        if self.enemies is not None:
            why += [f"war_declared:{t}" for t in sorted(enemies - self.enemies)]
            why += [f"war_ended:{t}" for t in sorted(self.enemies - enemies)]
        if self.won is not None:
            why += [f"capitulated:{t}" for t in sorted(won - self.won)]
        if "error" not in wars:
            self.enemies, self.won = enemies, won
            self.fought |= enemies
        if self.cursor is not None:
            obs, _ = self._read("hoi4_observe", {"since": self.cursor, "kinds": sorted(TRIGGER_KINDS)})
            kinds = sorted({c.get("kind") for c in obs.get("changes") or []} & TRIGGER_KINDS)
            why += [k for k in kinds if k not in ("war_started", "war_ended")]
        ex = runfiles.executor_state(self.run_dir) if self.run_files else {}
        phases = {op: o.get("phase") for op, o in (ex.get("operations") or {}).items()}
        if self.phases is not None:
            why += [f"operation:{op}:{p}" for op, p in sorted(phases.items()) if self.phases.get(op) != p]
        self.phases = phases
        return why, date

    # --- one snapshot -------------------------------------------------------------

    def take(self, trigger="manual"):
        raw, states, ms = self.read_all()
        files = self.run_files
        snap = build(raw, states_by_tag=states,
                     executor=runfiles.executor_state(self.run_dir) if files else None,
                     player_calls=runfiles.player_calls(self.run_dir) if files else None,
                     trigger=trigger, read_ms=ms,
                     utc=round(time.time(), 1))
        if not snap.get("date"):
            return None
        for t, e in (g(snap, "wars", "per_enemy", default={}) or {}).items():
            if e.get("at_war") or e.get("casualties_own"):
                self.fought.add(t)
        snap["findings"] = findings_mod.findings(snap, self.prev, self.regions)
        name = snap["date"]
        path = os.path.join(self.state_dir, name + ".json")
        k = 2
        while os.path.exists(path):
            path = os.path.join(self.state_dir, f"{name}-{k}.json")
            k += 1
        write_atomic(path, json.dumps(snap, ensure_ascii=False, separators=(",", ":")))
        write_atomic(os.path.join(self.run_dir, "SITREP.txt"),
                     sitrep.render(snap, self.prev, self.run_name))
        self.prev.append(snap)
        del self.prev[:-self.keep]
        self.last_date = snap["date"]
        self.count += 1
        return path

    def poll(self):
        why, date = self.due()
        if not why:
            return None
        return self.take(",".join(why))
