#!/usr/bin/env python3
"""Run observation from the command line: record, report, diff, history.

  observe.py record RUN [--sock PATH] [--every-days 7] [--poll 5] [--once] [--game-dir DIR]
      record snapshots for the whole life of a run: RUN/state/<game date>.json and
      RUN/SITREP.txt; stops when RUN/closed.json appears or the run's socket has been
      gone for --gone seconds. Reads only (the server's read lane); never input.
  observe.py replay RUN [--out DIR]
      rebuild snapshots of a past run from RUN/calls.jsonl (whatever it read).
  observe.py sitrep RUN [--since GAME_DATE]
      print the newest SITREP; with --since, what changed since that game date.
  observe.py history RUN PATH [--changes]
      one quantity over time (see hoi4_live/observe/history.py for paths).
  observe.py watch RUN [--poll 10]
      the issue monitor for the whole life of a run: RUN/issues.jsonl, ISSUES.md, issues_open.json,
      GATE.md and gates/, COSTS.md, COVERAGE.md (docs/MONITORING.md). Reads files only.
  observe.py monitor RUN [--out DIR]
      replay the monitor over a finished run and print what it would have caught and when.
  observe.py costs RUN / observe.py coverage RUN
      print the cost meter / the tool coverage table of a run.
  observe.py lessons STORE {seed [FILE] | list | lint PLAN [--country TAG]}
      the lessons store (learned/lessons): install the seeded lessons, list them, lint a plan.
  observe.py report REPORT.md RUN [RUN ...] [--store STORE]
      rewrite the generated blocks (<!-- auto:NAME --> ... <!-- /auto:NAME -->) of a report from runs.

RUN is a run directory. The socket speaks one JSON line each way:
{"tool", "args", "source": "recorder"} -> {"isError", "body"}.
"""
from __future__ import annotations

import argparse
import json
import os
import socket
import sys
import time

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__)))))

from hoi4_live.observe import cleanliness  # noqa: E402
from hoi4_live.observe import findings as findings_mod  # noqa: E402
from hoi4_live.observe import history, mapfiles, sitrep  # noqa: E402
from hoi4_live.observe.reads import SNAPSHOT_TOOL  # noqa: E402
from hoi4_live.observe.recorder import Recorder  # noqa: E402
from hoi4_live.observe.replay import ReplayClient, load_calls, replay_run  # noqa: E402


def socket_client(path, timeout=60):
    def call(tool, args):
        s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        s.settimeout(timeout)
        try:
            s.connect(path)
            s.sendall((json.dumps({"tool": tool, "args": args or {}, "source": "recorder",
                                   "t_send": time.time()}) + "\n").encode())
            line = s.makefile().readline()
        finally:
            s.close()
        reply = json.loads(line)
        body = reply.get("body")
        if reply.get("isError") and isinstance(body, dict) and "error" not in body:
            body = {"error": body}
        return body
    return call


def listed_tools(path):
    try:
        s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        s.settimeout(30)
        s.connect(path)
        s.sendall((json.dumps({"tool": "__tools__", "args": {}, "source": "recorder"}) + "\n").encode())
        reply = json.loads(s.makefile().readline())
        s.close()
        return {t.get("name") for t in reply.get("tools") or []}
    except (OSError, ValueError):
        return set()


def game_dir_of(run, given):
    if given:
        return given
    try:
        with open(os.path.join(run, "identity.json")) as f:
            exe = json.load(f)["game"]["exe"]
    except (OSError, ValueError, KeyError, TypeError):
        return None
    return mapfiles.game_dir_from_exe(exe)


def game_pid(run):
    try:
        with open(os.path.join(run, "identity.json")) as f:
            return int(json.load(f)["game"]["pid"])
    except (OSError, ValueError, KeyError, TypeError):
        return None


def alive(pid):
    try:
        os.kill(pid, 0)  # signal 0: existence check only
    except ProcessLookupError:
        return False
    except PermissionError:
        return True
    return True


def cmd_record(a):
    sock = a.sock or os.path.join(a.run, "play.sock")
    gone_since = None
    while not os.path.exists(sock):
        gone_since = gone_since or time.time()
        if time.time() - gone_since > a.gone or os.path.exists(os.path.join(a.run, "closed.json")):
            print("no socket: nothing to record", flush=True)
            return 1
        time.sleep(a.poll)
    tools = listed_tools(sock)
    rec = Recorder(a.run, socket_client(sock), game_dir=game_dir_of(a.run, a.game_dir),
                   every_days=a.every_days, snapshot_tool=SNAPSHOT_TOOL if SNAPSHOT_TOOL in tools else None)
    print(f"recording {a.run} every {a.every_days} game days (one-call snapshot tool: "
          f"{bool(rec.snapshot_tool)}, regions known for {len(rec.regions)} states)", flush=True)
    if a.once:
        print(rec.take(a.trigger))
        return 0
    gone_since = None
    pid = game_pid(a.run)
    while True:
        if os.path.exists(os.path.join(a.run, "closed.json")):
            print("run closed", flush=True)
            return 0
        if not os.path.exists(sock) or (pid and not alive(pid)):
            gone_since = gone_since or time.time()
            if time.time() - gone_since > a.gone:
                print("socket or game gone", flush=True)
                return 0
            time.sleep(a.poll)
            continue
        gone_since = None
        try:
            path = rec.poll()
            if path:
                print(time.strftime("%H:%M:%S"), os.path.basename(path), rec.prev[-1].get("trigger"),
                      f"{len(rec.prev[-1].get('findings') or [])} findings", flush=True)
        except Exception as ex:  # keep recording through a bad read
            print(time.strftime("%H:%M:%S"), "poll failed:", repr(ex)[:300], flush=True)
        time.sleep(a.poll)


def cmd_replay(a):
    out = a.out or os.path.join(a.run, "replay")
    os.makedirs(out, exist_ok=True)
    calls = os.path.join(a.run, "calls.jsonl")
    client = ReplayClient(load_calls(calls))
    rec = Recorder(out, client, game_dir=game_dir_of(a.run, a.game_dir), every_days=a.every_days,
                   run_files=False, run_name=os.path.basename(os.path.abspath(a.run)))
    written = replay_run(calls, rec)
    print(f"{len(written)} snapshots in {out}/state")
    return 0


def cmd_sitrep(a):
    snaps = history.load_run(a.run)
    if not snaps:
        print(f"no snapshots in {a.run}/state")
        return 1
    if a.since:
        older = [s for s in snaps if (s.get("date") or "") <= a.since] or snaps[:1]
        sys.stdout.write(sitrep.diff(older[-1], snaps[-1]))
        return 0
    newest, prev = snaps[-1], snaps[:-1]
    fs = newest.get("findings")
    if fs is None:
        fs = findings_mod.findings(newest, prev)
    sys.stdout.write(sitrep.render(newest, prev, os.path.basename(os.path.abspath(a.run)), fs))
    sys.stdout.write(cleanliness.render(a.run, snaps))
    return 0


def cmd_history(a):
    sys.stdout.write(history.render(history.load_run(a.run), a.path, a.changes))
    return 0


def cmd_watch(a):
    from hoi4_live.observe.monitor import watch
    mon = watch(a.run, poll=a.poll)
    print(f"monitor stopped: {len(mon.det.issues)} issues, {len(mon.esc.log)} escalations", flush=True)
    return 0


def cmd_monitor(a):
    from hoi4_live.observe.monitor import Monitor, summarize
    out = a.out or os.path.join(a.run, "monitor-replay")
    if os.path.exists(os.path.join(out, "issues.jsonl")):
        os.remove(os.path.join(out, "issues.jsonl"))
    for name in ("escalations.jsonl",):
        if os.path.exists(os.path.join(out, name)):
            os.remove(os.path.join(out, name))
    mon = Monitor(a.run, out, replay=True, store=a.store)
    events = mon.replay()
    sys.stdout.write(summarize(events, a.limit))
    st = mon._stats()
    print(f"overhead: {st['snapshots']} snapshots, mean {st['ms_per_snapshot_mean']:.1f} ms; whole replay "
          f"{mon.poll_ms[-1]:.0f} ms; files in {out}")
    return 0


def cmd_costs(a):
    from hoi4_live.observe import costs
    sys.stdout.write(costs.render_costs(a.run))
    return 0


def cmd_coverage(a):
    from hoi4_live.observe import costs
    sys.stdout.write(costs.coverage(a.run))
    return 0


def cmd_lessons(a):
    from hoi4_live.observe.lessons import LessonStore
    st = LessonStore(a.store, os.environ.get("HOI4_GAME_VERSION", ""))
    if a.action == "seed":
        seed = a.arg or os.path.join(os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__)))),
                                     "hoi4_live", "observe", "lessons_seed.json")
        print(f"{st.seed(seed)} lessons installed in {st.dir}")
    elif a.action == "list":
        for le in st.all():
            print(f"{le['id']} [{le.get('status')}, {le.get('confidence', 0):.2f}, {le.get('scope')}]: "
                  f"{le.get('statement', '')[:150]}")
    elif a.action == "lint":
        with open(a.arg) as f:
            plan = json.load(f)
        warns = st.lint_plan(plan, a.country)
        for w in warns:
            print(f"WARN lesson {w['lesson']} at {w['where']}: {w['text'][:200]}")
        print(f"{len(warns)} warning(s)")
    return 0


def cmd_report(a):
    from hoi4_live.observe.report import fill
    fill(a.report, a.runs, a.store)
    print(f"filled {a.report} from {len(a.runs)} run(s)")
    return 0


def main(argv=None):
    p = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    sub = p.add_subparsers(dest="cmd", required=True)
    r = sub.add_parser("record")
    r.add_argument("run")
    r.add_argument("--sock")
    r.add_argument("--every-days", type=float, default=7.0)
    r.add_argument("--poll", type=float, default=5.0)
    r.add_argument("--gone", type=float, default=300.0)
    r.add_argument("--once", action="store_true")
    r.add_argument("--trigger", default="manual")
    r.add_argument("--game-dir")
    r.set_defaults(fn=cmd_record)
    rp = sub.add_parser("replay")
    rp.add_argument("run")
    rp.add_argument("--out")
    rp.add_argument("--every-days", type=float, default=7.0)
    rp.add_argument("--game-dir")
    rp.set_defaults(fn=cmd_replay)
    s = sub.add_parser("sitrep")
    s.add_argument("run")
    s.add_argument("--since")
    s.set_defaults(fn=cmd_sitrep)
    h = sub.add_parser("history")
    h.add_argument("run")
    h.add_argument("path")
    h.add_argument("--changes", action="store_true")
    h.set_defaults(fn=cmd_history)
    w = sub.add_parser("watch")
    w.add_argument("run")
    w.add_argument("--poll", type=float, default=10.0)
    w.set_defaults(fn=cmd_watch)
    m = sub.add_parser("monitor")
    m.add_argument("run")
    m.add_argument("--out")
    m.add_argument("--store", help="lessons store (default: learned/ beside the run, as the server's)")
    m.add_argument("--limit", type=int, default=40)
    m.set_defaults(fn=cmd_monitor)
    c = sub.add_parser("costs")
    c.add_argument("run")
    c.set_defaults(fn=cmd_costs)
    cv = sub.add_parser("coverage")
    cv.add_argument("run")
    cv.set_defaults(fn=cmd_coverage)
    ls = sub.add_parser("lessons")
    ls.add_argument("store")
    ls.add_argument("action", choices=["seed", "list", "lint"])
    ls.add_argument("arg", nargs="?")
    ls.add_argument("--country")
    ls.set_defaults(fn=cmd_lessons)
    rpt = sub.add_parser("report")
    rpt.add_argument("report")
    rpt.add_argument("runs", nargs="+")
    rpt.add_argument("--store")
    rpt.set_defaults(fn=cmd_report)
    a = p.parse_args(argv)
    return a.fn(a)


if __name__ == "__main__":
    sys.exit(main())
