#!/usr/bin/env python3
"""reconcile.py: check a frozen plan against the live game on day one (strategist procedure step 9).

Usage:
    reconcile.py --run RUN --call-cmd "<command that runs one read tool>" [--dry-run]

The call command is split like a shell command, then TOOL and its JSON arguments are appended, e.g.
`--call-cmd "python3 -I /path/to/broker.py RUN call"` runs `... call hoi4_armies '{"threat": false}'`.
Its stdout is the tool result, either the body itself or {"isError": bool, "body": {...}}.

Only read tools are called (no lease, no input). The game should be paused on day one, which is allowed
pre-start setup. What it checks, against the names the plan uses:
  * division templates  (army_layout[].templates, training template(s), templates[].base)
                        vs hoi4_division_templates
  * generals            (army_layout[].general, armies.generals, army_group[].marshal) vs
                        hoi4_commanders
  * air wings           (air_plan[].wing: a live name, or "<variant> at <state id>", which expands
                        to one entry per matching wing from hoi4_air) vs hoi4_air_wings
  * production targets  (production[].remove / replace) vs hoi4_production_lines
  * first research block (research_path, earliest year) vs hoi4_technology_list {available: true}
A plan name that differs from exactly one live name only in case, accents, punctuation or the degree
sign (e.g. "12 Stormo" vs "12° Stormo") is patched in RUN/plan.json (the original is kept as
plan.pre-reconcile.json). Everything else goes to RUN/reconcile.json as unresolved, for the
strategist to fix. Exit status: 0 nothing unresolved, 2 unresolved items, 1 a read failed.
Stdlib only, Python 3.9+.
"""
import argparse
import json
import os
import shlex
import subprocess
import sys

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__)))))
from hoi4_live.play.reconcile import (  # noqa: E402,F401 - the logic lives in the product
    SELECTOR, expand_wing_selectors, first_block, live_names, norm, reconcile, references, run)


class Caller:
    def __init__(self, call_cmd):
        # POSIX splitting would eat Windows backslashes; there, split without escapes and drop quotes.
        if os.name == "nt":
            self.argv = [a.strip('"') for a in shlex.split(call_cmd, posix=False)]
        else:
            self.argv = shlex.split(call_cmd)

    def __call__(self, tool, args):
        p = subprocess.run(self.argv + [tool, json.dumps(args)], capture_output=True, text=True, timeout=120,
                           encoding="utf-8", errors="replace")
        if p.returncode != 0:
            raise RuntimeError("%s failed: %s" % (tool, (p.stderr or p.stdout)[-400:]))
        out = json.loads(p.stdout[p.stdout.index("{"):])
        if isinstance(out, dict) and "body" in out and "isError" in out:
            if out["isError"]:
                raise RuntimeError("%s returned an error: %s" % (tool, json.dumps(out["body"])[:400]))
            return out["body"]
        return out


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("--run", required=True)
    ap.add_argument("--call-cmd", required=True)
    ap.add_argument("--dry-run", action="store_true", help="report only; do not patch plan.json")
    args = ap.parse_args(argv)
    try:
        report = run(os.path.abspath(args.run), Caller(args.call_cmd), dry_run=args.dry_run)
    except (RuntimeError, ValueError, OSError, subprocess.TimeoutExpired) as e:
        print(json.dumps({"ok": False, "error": str(e)}))
        return 1
    print(json.dumps({"ok": report["ok"], "patched": len(report["patched"]), "unresolved": len(report["unresolved"]),
                      "research_not_yet_available": len(report["research"]["not_yet_available"])}))
    return 0 if report["ok"] else 2


if __name__ == "__main__":
    sys.exit(main())
