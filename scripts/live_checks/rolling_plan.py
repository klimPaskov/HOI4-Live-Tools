#!/usr/bin/env python3
"""Live checks of the rolling planner (docs/ROLLING_PLAN.md). Never run in CI; needs a running campaign.

    python3 scripts/live_checks/rolling_plan.py --prefix INSTALL [--minutes 20] [--out FOLDER]

Start a campaign the normal way (`hoi4-live play TAG "<goal>"`, the game unpaused at speed 3, the player
playing) with an install of the branch under test, then run this script beside it. It never sends input to the
game and never pauses it: it talks to the run's supervisor (hoi4_run planner, hoi4_overview, hoi4_observe) and
reads the run folder. What it checks, each PASS, FAIL or SKIP with its evidence (results.json in --out):

  ticks             the planner reads every ~15 s of wall time while the game runs (summary ticks grow)
  state_matches     the planner's state date is within 10 game days of hoi4_overview
  facts_bound       the game files and planning tools were bound (supervisor.log planner_ready)
  update_applied    at least one update applied within --minutes (journal), with its gates, version chain
                    v<N> -> v<N+1> in RUN/plans, and the executor's plan_state still for the same campaign
  executor_follows  after an applied update the executor acts on an item the update added (a research or focus
                    start, a production line, a politics pick) within 30 game days (hoi4_observe autopilot_action)
  no_pause          the game stayed unpaused for the whole check (hoi4_overview samples)
  pace              game days per wall minute while an update ran are within 15% of the run's median
  model_cost        model calls, seconds and tokens per hour stay under the ceiling (summary budget)
  agent_answer      with HOI4_LIVE_PLANNER=agent in the supervisor's environment: the player receives the
                    "plan update is waiting" note and its hoi4_run plan_patch answer is applied or refused
                    with a reason (SKIP otherwise)
  rollback          only when one happened: the plan went back to the parent's content as a new version
Exit 0 when nothing failed.
"""

import argparse
import json
import os
import statistics
import sys
import time

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.dirname(os.path.dirname(HERE)))

from hoi4_live.play import proxy, runs  # noqa: E402

RESULTS = []


def check(name, ok, evidence, skip=False):
    state = "SKIP" if skip else ("PASS" if ok else "FAIL")
    RESULTS.append({"check": name, "state": state, "evidence": evidence})
    print("%-5s %s: %s" % (state, name, json.dumps(evidence, ensure_ascii=False, default=str)[:400]), flush=True)
    return ok


def call(prefix, tool, args):
    conn = proxy.connect(prefix)
    if conn is None:
        raise SystemExit("no active run: start one with hoi4-live play first")
    with conn:
        conn.send({"op": "call", "name": tool, "args": args})
        out = conn.recv()
    return out.get("isError"), out.get("body")


def jsonl(path):
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


def day(text):
    sys.path.insert(0, os.path.dirname(os.path.dirname(HERE)))
    from hoi4_live.planner.state import day_of
    return day_of(text)


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("--prefix", required=True)
    ap.add_argument("--minutes", type=float, default=20.0)
    ap.add_argument("--out", default=None)
    args = ap.parse_args(argv)
    active = runs.active_run(args.prefix) or {}
    run = active.get("run")
    if not run:
        raise SystemExit("no active run under %s" % args.prefix)
    out_dir = args.out or os.path.join(run, "live-check-rolling")
    os.makedirs(out_dir, exist_ok=True)
    samples, ticks, seq = [], [], 0
    actions = []
    deadline = time.time() + args.minutes * 60
    while time.time() < deadline:
        err, ov = call(args.prefix, "hoi4_overview", {})
        _, planner = call(args.prefix, "hoi4_run", {"op": "planner"})
        _, feed = call(args.prefix, "hoi4_observe", {"since": seq})
        if isinstance(feed, dict):
            seq = feed.get("seq", seq)
            actions += [c for c in feed.get("changes") or [] if c.get("kind") == "autopilot_action"]
        if not err:
            samples.append({"t": time.time(), "date": (ov.get("date") or {}).get("text"), "paused": ov.get("paused"),
                            "planner_date": (planner or {}).get("state_date"), "ticks": (planner or {}).get("ticks")})
        ticks.append(planner)
        time.sleep(15)
    planner_dir = os.path.join(run, "planner")
    journal = jsonl(os.path.join(planner_dir, "journal.jsonl"))
    events = jsonl(os.path.join(run, "supervisor.log"))
    summary = ticks[-1] if ticks else {}

    grow = [s["ticks"] for s in samples if s.get("ticks") is not None]
    check("ticks", len(grow) > 2 and grow[-1] > grow[0], {"first": grow[:1], "last": grow[-1:]})
    gaps = [abs(day(s["date"]) - day(s["planner_date"])) for s in samples if s.get("planner_date") and s.get("date")]
    check("state_matches", bool(gaps) and max(gaps) <= 10, {"max_gap_days": max(gaps) if gaps else None})
    ready = [e for e in events if e.get("event") == "planner_ready"]
    check("facts_bound", bool(ready) and ready[-1].get("facts"), ready[-1:] or "no planner_ready event")
    applied = [j for j in journal if j.get("version")]
    chain = sorted(int(n[6:-5]) for n in os.listdir(os.path.join(run, "plans"))) if os.path.isdir(
        os.path.join(run, "plans")) else []
    check("update_applied", bool(applied) and bool(chain),
          {"applied": [(j["version"], j.get("date"), j.get("applied"), j.get("summary")) for j in applied[-3:]],
           "history": chain})
    if applied:
        added = []
        for j in applied:
            folder = os.path.join(planner_dir, "update-%s" % j["update"])
            patch = (runs.read_json(os.path.join(folder, "applied.json")) or {}).get("ops") or []
            for op in patch:
                for item in op.get("items") or []:
                    added += item.get("techs") or [] if isinstance(item, dict) else [item]
                if isinstance(op.get("item"), dict):
                    added.append(op["item"].get("equipment"))
        hits = [a for a in actions if any(x and x in json.dumps(a) for x in added)]
        check("executor_follows", bool(hits), {"added": added[:10], "acted": [h.get("detail") for h in hits[:3]]})
    else:
        check("executor_follows", False, "no update applied", skip=True)
    paused = [s for s in samples if s.get("paused")]
    check("no_pause", not paused, {"paused_samples": len(paused), "samples": len(samples)})
    pace = []
    for a, b in zip(samples, samples[1:]):
        if a.get("date") and b.get("date") and b["t"] > a["t"]:
            pace.append((day(b["date"]) - day(a["date"])) / ((b["t"] - a["t"]) / 60.0))
    if pace:
        med = statistics.median(pace)
        check("pace", min(pace) >= 0.85 * med or len(pace) < 4, {"median_days_per_min": round(med, 1),
                                                                "min": round(min(pace), 1)})
    b = (summary or {}).get("budget") or {}
    limits = b.get("limits") or {}
    per = b.get("per_hour") or {}
    check("model_cost", not b or (per.get("calls", 0) <= limits.get("max_calls", 1e9) and
                                  per.get("tokens", 0) <= limits.get("max_tokens", 1e12)), b)
    waiting = [e for e in events if e.get("event") == "note" and "plan update is waiting" in str(e.get("text"))]
    agent = [j for j in journal for p in j.get("proposals") or [] if p.get("proposer") == "agent"]
    check("agent_answer", bool(agent), {"asked": len(waiting), "answered": len(agent)}, skip=not waiting)
    rolled = [j for j in jsonl(os.path.join(planner_dir, "ledger.jsonl")) if j.get("outcome") == "rolled_back"]
    check("rollback", True, rolled[-1:] or "none happened", skip=not rolled)
    with open(os.path.join(out_dir, "results.json"), "w", encoding="utf-8") as f:
        json.dump(RESULTS, f, indent=1, default=str)
    return 1 if any(r["state"] == "FAIL" for r in RESULTS) else 0


if __name__ == "__main__":
    sys.exit(main())
