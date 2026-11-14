"""Replay a recorded run through the rolling planner, offline.

    python -m hoi4_live.planner.replay compact CALLS.jsonl OUT.jsonl
        keep only the reads the planner uses from a run's calls.jsonl, without paths, process ids or texts
    python -m hoi4_live.planner.replay run RECORD.jsonl PLAN.json WORKDIR [--model] [--gates]
        feed the record through the planner in game-date order; print the updates and the queue check

A record is one JSON object per line: {"tool", "args", "body"} in the order the run read them. The replay feeds
each read to the worker's state, steps the planner at every overview read (about every 7 game days in the
recorded runs), runs each update synchronously and checks, at every later step, that the plan in force still
has work queued for research and focus and that every reported shortage has a line.
"""

import argparse
import gzip
import json
import os
import shutil
import sys

from . import horizon as hz
from .worker import PlannerWorker

KEEP = {"hoi4_overview", "hoi4_technology_list", "hoi4_states", "hoi4_armies", "hoi4_war_stats", "hoi4_observe",
        "hoi4_wait_change", "hoi4_production_lines", "hoi4_focus_available", "hoi4_economy", "hoi4_research"}
CHANGE_DETAIL = {"focus", "with", "to", "from", "revise_on_enter", "checkpoint", "failed", "by", "else", "reason",
                 "evidence", "idle", "item", "why", "contingency", "revise", "status", "army", "paused", "alert"}
EVIDENCE = {"political_power", "reserve", "equipment", "days_of_cover", "critical", "armies", "floor", "ratio",
            "enemy", "army", "equipment_fill_mean", "short_equipment", "divisions"}


def _slim_change(c):
    detail = {k: v for k, v in (c.get("detail") or {}).items() if k in CHANGE_DETAIL}
    if isinstance(detail.get("evidence"), dict):
        ev = {k: v for k, v in detail["evidence"].items() if k in EVIDENCE}
        if isinstance(ev.get("armies"), list):
            ev["armies"] = [{k: a.get(k) for k in ("army", "equipment_fill_mean", "short_equipment")}
                            if isinstance(a, dict) else a for a in ev["armies"]]
        detail["evidence"] = ev
    if isinstance(detail.get("why"), str):
        detail["why"] = detail["why"][:120]
    return {"seq": c.get("seq"), "kind": c.get("kind"), "date": c.get("date"), "detail": detail}


def _slim(tool, args, body):
    if tool == "hoi4_overview":
        return {k: body.get(k) for k in ("date", "paused", "player", "research", "focus")}
    if tool in ("hoi4_observe", "hoi4_wait_change"):
        return {"seq": body.get("seq"), "changes": [_slim_change(c) for c in body.get("changes") or []]}
    if tool == "hoi4_states":
        return {"states": [{k: s.get(k) for k in ("state_id", "name", "owner", "controller", "victory_points")}
                           for s in body.get("states") or []]}
    if tool == "hoi4_armies":
        return {"armies": [{k: a.get(k) for k in ("army", "division_count", "planning", "plan_executing", "states")}
                           for a in body.get("armies") or []]}
    if tool == "hoi4_war_stats":
        return {"player": body.get("player"), "wars": [{"sides": [
            {"includes_player": s.get("includes_player"),
             "countries": [{"tag": c.get("tag")} for c in s.get("countries") or []]}
            for s in w.get("sides") or []]} for w in body.get("wars") or []]}
    if tool == "hoi4_production_lines":
        return {"lines": [{"equipment_id": l.get("equipment_id"), "factory_counters": {
            "active_factories_raw_signed": (l.get("factory_counters") or {}).get("active_factories_raw_signed")}}
            for l in body.get("lines") or []]}
    if tool == "hoi4_technology_list":
        return {k: body.get(k) for k in ("available_only", "start_years", "technologies") if k in body}
    if tool == "hoi4_focus_available":
        return {"focuses": body.get("focuses"), "wars": body.get("wars")}
    if tool == "hoi4_economy":
        return {k: body.get(k) for k in ("political_power", "civilian_factories_idle")}
    return {k: body.get(k) for k in ("researched",)}


def compact(calls_path, out_path):
    n = 0
    with open(calls_path, encoding="utf-8") as src, open(out_path, "w", encoding="utf-8") as out:
        for line in src:
            try:
                row = json.loads(line)
            except ValueError:
                continue
            tool, body = row.get("tool"), row.get("body")
            if tool not in KEEP or not isinstance(body, dict) or row.get("isError"):
                continue
            args = {k: v for k, v in (row.get("args") or {}).items() if k not in ("request_id", "lease_token")}
            out.write(json.dumps({"tool": tool, "args": args, "body": _slim(tool, args, body)},
                                 ensure_ascii=False, separators=(",", ":")) + "\n")
            n += 1
    return n


def records(path):
    opener = gzip.open if path.endswith(".gz") else open
    with opener(path, "rt", encoding="utf-8") as f:
        for line in f:
            if line.strip():
                yield json.loads(line)


def run(record_path, plan_path, work, proposer=None, facts=None, tools=None, low=None):
    """{"updates": [...], "checks": [...], "worker": summary} for a record replayed against a plan."""
    if os.path.isdir(work):
        shutil.rmtree(work)
    os.makedirs(work)
    shutil.copyfile(plan_path, os.path.join(work, "plan.json"))
    worker = PlannerWorker(work, call=lambda name, args: (True, {}), facts=facts, tools=tools, proposer=proposer,
                           background=False)
    if facts is not None:
        worker.attach(facts, tools)
    updates, checks = [], []
    for row in records(record_path):
        worker.state.feed(row["tool"], row["body"], row.get("args"))
        if row["tool"] != "hoi4_overview":
            continue
        entry = worker.step(wait=True)
        if isinstance(entry, dict):
            updates.append(entry)
        snap = worker.state.snapshot()
        plan, _ = worker.store.read()
        h = hz.estimate(plan, snap, worker.costs)
        lines = set(snap.get("lines") or {})
        entries = {e.get("equipment") or "" for e in plan.get("production") or [] if isinstance(e, dict)
                   and not e.get("remove")}
        unlined = [f for f, v in (h["production"].get("short") or {}).items()
                   if not any(x.startswith(f) for x in lines | entries)]
        checks.append({"date": snap.get("date"), "version": (plan.get("_meta") or {}).get("version"),
                       "research_days": h["research"]["days"], "focus_days": h["focus"]["days"],
                       "politics_order": len(((plan.get("politics_auto") or {}).get("order")) or []),
                       "unlined_shortages": unlined})
    return {"updates": updates, "checks": checks, "worker": worker.summary()}


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    sub = ap.add_subparsers(dest="cmd", required=True)
    c = sub.add_parser("compact")
    c.add_argument("calls")
    c.add_argument("out")
    r = sub.add_parser("run")
    r.add_argument("record")
    r.add_argument("plan")
    r.add_argument("work")
    r.add_argument("--model", action="store_true", help="propose with the configured planner model")
    r.add_argument("--gates", action="store_true", help="bind the game files and the planning tools")
    args = ap.parse_args(argv)
    if args.cmd == "compact":
        print(compact(args.calls, args.out))
        return 0
    from . import facts as pf, gates as pg, update as up
    facts = pf.load() if args.gates else None
    tools = pg.ToolGates(pf.tools_dir(), pf.game_dir()) if args.gates and pf.tools_dir() else None
    proposer = up.ModelProposer(pf.tools_dir()) if args.model else None
    out = run(args.record, args.plan, args.work, proposer, facts, tools)
    for u in out["updates"]:
        print(json.dumps({k: u.get(k) for k in ("update", "date", "triggers", "applied", "version", "summary",
                                                 "seconds")}, ensure_ascii=False))
        for p in u.get("proposals") or []:
            print("   ", json.dumps({k: p.get(k) for k in ("proposer", "result", "seconds", "tokens", "model")}))
    last = out["checks"][-1] if out["checks"] else {}
    print(json.dumps({"steps": len(out["checks"]), "last": last, "worker": out["worker"]}, default=str))
    return 0


if __name__ == "__main__":
    sys.exit(main())
