#!/usr/bin/env python3
"""brief.py: render the player's brief for a run from the generic template and the plan's brief.

Usage:
    brief.py --run RUN --call "<cmd that runs one tool>" --batch "<cmd that runs a batch>"
             [--sitrep "<cmd>"] [--log PATH] [--stop-date D] [--minutes N] [--speed 3] [--out FILE]

Fills skills/hoi4-campaign-playbook/briefs/player.txt with RUN/plan.brief.md (the strategist's player
section) and the run's commands. The title comes from RUN/run_spec.json (tag and goal); the stop date
defaults to the goal deadline in RUN/plan.json, else the run spec's horizon. Writes RUN/BRIEF.txt unless
--out is given. Stdlib only, Python 3.9+; no provider names in the output.
"""
import argparse
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(os.path.dirname(HERE))
TEMPLATE = os.path.join(ROOT, "skills", "hoi4-campaign-playbook", "briefs", "player.txt")
PLACEHOLDERS = ("TITLE", "RUN", "SPEED", "CALL", "BATCH", "PLAN_BRIEF", "SITREP", "LOG", "STOP_DATE", "MINUTES")


def read_json(path):
    try:
        with open(path, encoding="utf-8") as f:
            return json.load(f)
    except (OSError, ValueError):
        return {}


def render(run_dir, call, batch, sitrep=None, log=None, stop_date=None, minutes=150, speed=3, template=TEMPLATE):
    spec = read_json(os.path.join(run_dir, "run_spec.json"))
    plan = read_json(os.path.join(run_dir, "plan.json"))
    with open(os.path.join(run_dir, "plan.brief.md"), encoding="utf-8") as f:
        plan_brief = f.read().strip()
    with open(template, encoding="utf-8") as f:
        text = f.read()
    run = os.path.basename(os.path.normpath(run_dir))
    goal = plan.get("goal") if isinstance(plan.get("goal"), dict) else {}
    values = {
        "TITLE": "%s: %s" % (spec.get("tag", "?"), spec.get("goal", "campaign")),
        "RUN": run,
        "SPEED": str(speed),
        "CALL": call,
        "BATCH": batch,
        "PLAN_BRIEF": plan_brief,
        "SITREP": sitrep or "%s hoi4_overview '{}'" % call,
        "LOG": log or os.path.join(run_dir, "%s-log.md" % run),
        "STOP_DATE": stop_date or goal.get("deadline") or spec.get("horizon") or "the plan horizon",
        "MINUTES": str(minutes),
    }
    # Replace only the known placeholders; JSON braces in the template stay as they are.
    for key in PLACEHOLDERS:
        text = text.replace("{%s}" % key, values[key])
    return text


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("--run", required=True)
    ap.add_argument("--call", required=True)
    ap.add_argument("--batch", required=True)
    ap.add_argument("--sitrep")
    ap.add_argument("--log")
    ap.add_argument("--stop-date")
    ap.add_argument("--minutes", type=int, default=150)
    ap.add_argument("--speed", type=int, default=3)
    ap.add_argument("--out")
    args = ap.parse_args(argv)
    run_dir = os.path.abspath(args.run)
    text = render(run_dir, args.call, args.batch, args.sitrep, args.log, args.stop_date, args.minutes, args.speed)
    out = args.out or os.path.join(run_dir, "BRIEF.txt")
    with open(out, "w", encoding="utf-8") as f:
        f.write(text)
    print(out)
    return 0


if __name__ == "__main__":
    sys.exit(main())
