"""The run monitor: the issue detector, lessons, escalations and gate packets over one run.

`Monitor(run_dir).poll()` reads what the run's logs gained since the last poll (live: `observe.py
watch RUN` beside the state recorder); `Monitor(run_dir, out_dir).replay()` runs the same code over a
finished run in time order and says what it would have caught and when. No model is called by the
monitor itself unless an answer command is configured for escalations (MONITOR_ANSWER_CMD).
"""
from __future__ import annotations

import json
import os
import time
from collections import deque

from . import answer, budget, cleanliness, costs
from .escalation import Escalator, gate_kind, gate_packet
from .issues import Detector, _atomic
from .lessons import LessonStore, lessons_from_issue, store_dir
from .runlogs import RunLogs, body_date, now_snapshot


def _load(path, default=None):
    try:
        with open(path, encoding="utf-8") as f:
            return json.load(f)
    except (OSError, ValueError):
        return default


class Monitor:
    def __init__(self, run_dir, out_dir=None, replay=False, store=None, game_version=None, per_year=None,
                 history=None):
        self.run = run_dir
        self.out = out_dir or run_dir
        os.makedirs(self.out, exist_ok=True)
        self.name = os.path.basename(os.path.abspath(run_dir))
        self.replaying = replay
        self.plan = _load(os.path.join(run_dir, "plan.json"), {}) or {}
        self.plan_name = ((self.plan.get("_meta") or {}).get("name") if isinstance(self.plan.get("_meta"), dict)
                          else None) or self.name
        self.store_path = store or store_dir(run_dir)
        hist = history if history is not None else _load(os.path.join(self.store_path, "tool_stats.json"), {})
        self.lessons = LessonStore(self.store_path, game_version or os.environ.get("HOI4_GAME_VERSION", ""))
        self.det = Detector(self.out, plan=self.plan, plan_state=self._plan_state(), history=hist)
        self.esc = Escalator(self.out, per_year=per_year or 24, lessons=self.lessons, run_name=self.name,
                             live=not replay)
        self.budget = budget.Budget()
        self.reviews = {}            # game year -> gate reviews asked
        self.last_sync = 0.0
        self.stop_written = False
        self.det.listeners.append(self._on_event)
        self.det.lessons = self.lessons
        self.logs = RunLogs(run_dir)
        self.prev = deque(maxlen=12)
        self.country = None
        self.last_gate_seq = 0
        self.last_snap_date = None
        self.poll_ms = []
        self.gates = []
        self.lesson_events = 0
        self.full_seen = False

    def _plan_state(self):
        return _load(os.path.join(self.run, "plan_state.json"), {}) or {}

    # --- events -------------------------------------------------------------------------------

    def _on_event(self, event, issue):
        if event in ("overdue", "recurred", "stale"):
            rec = self.esc.consider(event, issue, self.det.date, self.det.at_war)
            if rec:
                self.det._emit("escalated", issue, tier=1, why=rec.get("why"))
        if event in ("resolved", "recurred", "overdue"):
            le = lessons_from_issue(self.lessons, issue, event, self.name, self.plan_name, self.country)
            if le:
                self.lesson_events += 1
                self.det.pending.append({"t": round(time.time(), 1), "date": self.det.date, "event": "lesson",
                                         "id": issue["id"], "key": issue["key"], "lesson": le["id"],
                                         "confidence": le.get("confidence"), "outcome": le.get("outcome")})

    # --- feeding ------------------------------------------------------------------------------

    def _stream(self, polled):
        rows = []
        for c in polled["calls"]:
            rows.append((float(c.get("utc") or 0), 1, "call", c))
        for a in polled["actions"]:
            rows.append(((a.get("unix_ms") or 0) / 1000.0, -1 if a.get("status") == "Dispatching" else 2, "action", a))
        for a in polled["server"]:
            rows.append(((a.get("unix_ms") or 0) / 1000.0, 0, "server", a))
        for s in polled["snapshots"]:
            rows.append((float(s.get("utc") or 0), 3, "snapshot", s))
        rows.sort(key=lambda r: (r[0], r[1]))
        return rows

    def feed(self, rows):
        for utc, _, kind, rec in rows:
            if kind == "call":
                d = body_date(rec.get("body"))
                b = rec.get("body") if isinstance(rec.get("body"), dict) else {}
                now = b.get("now") if isinstance(b.get("now"), dict) else b
                if d:
                    self.det.clock(d, now.get("paused"), utc)
                if isinstance(b, dict) and b.get("player") and not self.country:
                    self.country = b.get("player")
                self.det.call(rec)
                self.esc.note(f"{rec.get('source')} {rec.get('tool')} "
                              f"{'ERR ' + str((b.get('error') or {}).get('code')) if b.get('error') else 'ok'}")
                light = now_snapshot(rec)
                if light and not self.full_seen:
                    self.det.snapshot(light)
            elif kind == "action":
                self.det.action(rec)
                if rec.get("status") == "Dispatching":
                    continue
                self.esc.note(f"executor {(rec.get('action') or {}).get('kind')} {rec.get('status')}")
            elif kind == "server":
                self.det.server_action(rec)
            elif kind == "snapshot":
                self._snapshot(rec)

    def _snapshot(self, snap):
        self.full_seen = True
        self.country = self.country or snap.get("player")
        if not self.replaying:
            self.det.plan_state = self._plan_state()
        self.det.clock(snap.get("date"), snap.get("paused"), snap.get("utc"))
        self.det.snapshot(snap, list(self.prev))
        self.det.executor_silent_check()
        kind = gate_kind(self.last_snap_date, snap)
        self.last_snap_date = snap.get("date") or self.last_snap_date
        self.prev.append(snap)
        if kind:
            self.gate(kind, snap)

    def poll(self):
        t0 = time.perf_counter()
        polled = self.logs.poll()
        for r in polled["rss"]:
            self.det.rss(r)
        self.feed(self._stream(polled))
        self.det.flush()
        self.poll_ms.append(round((time.perf_counter() - t0) * 1000, 2))
        del self.poll_ms[:-500]
        try:
            self.budget_poll()
        except Exception as ex:  # the ledger is a report and a guard; it never stops the watch
            print(time.strftime("%H:%M:%S"), "budget poll failed:", repr(ex)[:200], flush=True)
        self._stats()
        return polled

    def replay(self):
        """The whole finished run, in time order; returns the issue events."""
        t0 = time.perf_counter()
        polled = self.logs.poll()
        rows = self._stream(polled)
        for i in range(0, len(rows), 200):
            self.feed(rows[i:i + 200])
            self.det.flush()
        for r in polled["rss"]:
            self.det.rss(r)
        self.det.flush()
        self.poll_ms.append(round((time.perf_counter() - t0) * 1000, 2))
        self.close(write_history=False)
        return _read_jsonl(os.path.join(self.out, "issues.jsonl"))

    # --- gates and the end ---------------------------------------------------------------------

    def gate(self, kind, snap):
        snaps_line = ""
        try:
            from .history import load_run
            snaps = load_run(self.run)
            snaps_line = cleanliness.render(self.run, snaps)
        except Exception:  # the gate is written whatever the cleanliness block does
            snaps_line = ""
        text = gate_packet(kind, snap, self.det, self.last_gate_seq, self._costs_line(), snaps_line,
                           budget_line=self.budget.line(self.name))
        os.makedirs(os.path.join(self.out, "gates"), exist_ok=True)
        name = f"{str(snap.get('date') or 'end').replace('.', '')}-{kind}.md"
        _atomic(os.path.join(self.out, "gates", name), text)
        _atomic(os.path.join(self.out, "GATE.md"), text)
        self.last_gate_seq = self.det.seq
        self.gates.append({"kind": kind, "date": snap.get("date"), "chars": len(text)})
        if not self.replaying:
            self._review(kind, snap, text, name)
        if not self.replaying:
            try:
                _atomic(os.path.join(self.out, "COSTS.md"), costs.render_costs(self.run, self.out))
            except Exception as ex:  # costs are a report, never a reason to stop watching
                _atomic(os.path.join(self.out, "COSTS.md"), f"costs failed: {ex!r}\n")

    def _review(self, kind, snap, text, name):
        """Tier 2: a strong model reviews the gate packet (API key, budget checked, at most
        REVIEWS_PER_YEAR a game year; never in a replay). The review goes beside the packet."""
        if kind not in REVIEW_KINDS or answer.available() is not None:
            return
        year = str(snap.get("date") or "?")[:4]
        if self.reviews.get(year, 0) >= REVIEWS_PER_YEAR:
            return
        self.reviews[year] = self.reviews.get(year, 0) + 1
        out = answer.ask(text, tier=2, run=self.name, role="review")
        row = {"date": snap.get("date"), "kind": kind, "role": "review"}
        if "error" in out:
            row["error"] = out["error"]
        else:
            u = out["usage"]
            row.update({"model": out["model"], "input": u["input_tokens"], "output": u["output_tokens"],
                        "cache_read": u["cache_read_input_tokens"], "cache_write": u["cache_creation_input_tokens"],
                        "seconds": out["seconds"], "usd": out["usd"]})
            review = f"# Review of {name}\n\n{out['result']}\n"
            _atomic(os.path.join(self.out, "gates", name.replace(".md", "-review.md")), review)
            _atomic(os.path.join(self.out, "GATE.md"), text + "\n\n" + review)
        with open(os.path.join(self.out, "reviews.jsonl"), "a", encoding="utf-8") as f:
            f.write(json.dumps(row) + "\n")

    def budget_poll(self, force=False):
        """The player's spend from its own stream into the ledger (every 5 minutes), and the stop
        flag when the run, the day or the whole key is out of budget: RUN/budget_stop.json, which the
        launchers read (the run goes on executor-only; under the floor the player falls back to the
        subscription)."""
        if not self.budget.active or self.replaying:
            return None
        if not force and time.time() - self.last_sync < 300:
            return None
        self.last_sync = time.time()
        if budget.player_billed_to_key(self.run):
            self.budget.set_role(self.name, "player", budget.player_rows(self.run))
        code, why = self.budget.check(self.name, "player")
        path = os.path.join(self.run, "budget_stop.json")
        if code != budget.OK and not self.stop_written:
            _atomic(path, json.dumps({"code": code, "why": why, "floor": code == budget.FLOOR,
                                      "at": time.strftime("%Y-%m-%d %H:%M:%S"), "date": self.det.date}))
            self.stop_written = True
            return why
        return None

    def _costs_line(self):
        try:
            usage = costs.player_usage(self.run)
            usd = sum(r["usd_micro"] for r in usage.values()) / 1e6
            calls = sum(r["calls"] for r in usage.values())
        except Exception:
            usd, calls = 0.0, 0
        esc = len([e for e in self.esc.log if not e.get("skipped")])
        return (f"Costs so far: player {calls} model calls ${usd:.2f}; escalations {esc} (Tier 1 budget "
                f"{self.esc.per_year}/game year); monitor overhead mean "
                f"{_mean(self.det.overhead_ms):.1f} ms a snapshot, no model calls (Tier 0)")

    def _stats(self):
        st = {"snapshots": len(self.det.overhead_ms), "ms_per_snapshot_mean": _mean(self.det.overhead_ms),
              "ms_per_snapshot_max": max(self.det.overhead_ms or [0]), "polls": len(self.poll_ms),
              "ms_per_poll_mean": _mean(self.poll_ms), "issues": len(self.det.issues),
              "escalations": len(self.esc.log), "lesson_events": self.lesson_events, "gates": self.gates[-20:]}
        _atomic(os.path.join(self.out, "monitor_stats.json"), json.dumps(st, indent=1))
        return st

    def close(self, write_history=True):
        if not self.replaying:
            try:
                self.budget_poll(force=True)
            except Exception:
                pass
        snap = self.prev[-1] if self.prev else {"date": self.det.date, "trigger": "end"}
        self.gate("run_end", dict(snap, trigger="end"))
        self.det.flush()
        self._stats()
        m = costs.measure(self.run, self.out)
        _atomic(os.path.join(self.out, "COSTS.md"), costs.render_costs(self.run, self.out, m))
        _atomic(os.path.join(self.out, "COVERAGE.md"), costs.coverage(self.run, m))
        if write_history:
            path = os.path.join(self.store_path, "tool_stats.json")
            hist = _load(path, {}) or {}
            for tool, (ok, fail) in self.det.tool_stats.items():
                h = hist.setdefault(tool, {"ok": 0, "fail": 0})
                h["ok"] += ok
                h["fail"] += fail
            os.makedirs(self.store_path, exist_ok=True)
            _atomic(path, json.dumps(hist, indent=1, sort_keys=True))


REVIEW_KINDS = ("war_declared", "capitulated", "new_year", "failure", "run_end")
REVIEWS_PER_YEAR = 8


def _mean(v):
    return sum(v) / len(v) if v else 0.0


def _read_jsonl(path):
    out = []
    try:
        with open(path, encoding="utf-8") as f:
            for line in f:
                try:
                    out.append(json.loads(line))
                except ValueError:
                    pass
    except OSError:
        pass
    return out


def summarize(events, limit=60):
    """What the replay caught and when: first sighting per issue key, by class."""
    first = {}
    for e in events:
        if e.get("event") == "open" and e["key"] not in first:
            first[e["key"]] = e
    by = {}
    for e in first.values():
        by.setdefault(e["class"], []).append(e)
    lines = []
    for cls in ("technical", "tool_use", "duplicate", "idling", "gameplay"):
        rows = sorted(by.get(cls, []), key=lambda e: e.get("date") or "")
        lines.append(f"{cls}: {len(rows)} issue(s)")
        for e in rows[:limit]:
            lines.append(f"  {e.get('date')} {e['id']} {e['severity']} {e['key'][:70]}: {(e.get('text') or '')[:140]}")
    over = sum(1 for e in events if e.get("event") == "overdue")
    esc = sum(1 for e in events if e.get("event") == "escalated")
    les = sum(1 for e in events if e.get("event") == "lesson")
    lines.append(f"overdue {over}, escalated {esc}, lesson events {les}")
    return "\n".join(lines) + "\n"


def watch(run_dir, poll=10.0, gone=300.0):
    """Run the monitor for the life of a run (stops at closed.json or when the run's socket is gone)."""
    mon = Monitor(run_dir)
    sock = os.path.join(run_dir, "play.sock")
    gone_since = None
    while True:
        try:
            mon.poll()
        except Exception as ex:  # keep watching through a bad line
            print(time.strftime("%H:%M:%S"), "poll failed:", repr(ex)[:300], flush=True)
        if os.path.exists(os.path.join(run_dir, "closed.json")):
            break
        if not os.path.exists(sock):
            gone_since = gone_since or time.time()
            if time.time() - gone_since > gone:
                break
        else:
            gone_since = None
        time.sleep(poll)
    mon.poll()
    mon.close()
    return mon
