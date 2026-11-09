#!/usr/bin/env python3
"""check_calls.py: every tool call a plan hands to the player must be valid against the tool catalogue.

Usage:
    check_calls.py RUN_OR_PLAN [--json]

Checks `call` fields anywhere in plan.json (player-owned items name the exact call) and every
`hoi4_tool {json}` span in plan.brief.md and plan.md (a bare `hoi4_tool` in prose is only checked to
exist): the tool exists, required arguments are present,
no unknown arguments, types and enums match (scripts/hoi4facts/tool_schemas.py; catalogue
skills/hoi4-campaign-playbook/tools.schema.json, rebuilt from native/src). Placeholders such as
"<wing>" are allowed as values. Exit 1 when any call is invalid. Stdlib only, Python 3.9+.
"""
import argparse
import json
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
from hoi4facts import tool_schemas as ts  # noqa: E402


def plan_calls(node, path="plan"):
    """[(path, tool, args)] for every {"call": ...} in the plan (string "tool {json}" or {"tool","args"})."""
    out = []
    if isinstance(node, dict):
        call = node.get("call")
        if isinstance(call, str):
            for tool, args, _ in ts.calls_in_text("`%s`" % call.strip("`")):
                out.append(("%s.call" % path, tool, args))
        elif isinstance(call, dict) and call.get("tool"):
            out.append(("%s.call" % path, call["tool"], call.get("args", {})))
        elif isinstance(call, list):
            for i, c in enumerate(call):
                out += plan_calls({"call": c}, "%s.call[%d]" % (path, i))
        for k, v in node.items():
            if k != "call":
                out += plan_calls(v, "%s.%s" % (path, k))
    elif isinstance(node, list):
        for i, v in enumerate(node):
            out += plan_calls(v, "%s[%d]" % (path, i))
    return out


def check(target):
    run_dir = target if os.path.isdir(target) else os.path.dirname(os.path.abspath(target))
    plan_path = target if not os.path.isdir(target) else os.path.join(run_dir, "plan.json")
    catalogue = ts.load()
    findings, checked = [], 0
    with open(plan_path, encoding="utf-8") as f:
        plan = json.load(f)
    items = plan_calls(plan)
    for name in ("plan.brief.md", "plan.md"):
        p = os.path.join(run_dir, name)
        if os.path.exists(p):
            with open(p, encoding="utf-8") as f:
                for n, line in enumerate(f, 1):
                    for tool, args, raw in ts.calls_in_text(line):
                        if "{" not in raw:  # a bare tool name in prose: existence only
                            args = "REF"
                        items.append(("%s:%d" % (name, n), tool, args))
    for where, tool, args in items:
        checked += 1
        if args == "REF":
            if tool not in catalogue:
                findings.append({"where": where, "tool": tool, "problem": "unknown tool %s" % tool})
            continue
        if args is None:
            findings.append({"where": where, "tool": tool, "problem": "arguments are not valid JSON"})
            continue
        for problem in ts.check_call(catalogue, tool, args):
            findings.append({"where": where, "tool": tool, "problem": problem})
    return checked, findings


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("target")
    ap.add_argument("--json", action="store_true")
    a = ap.parse_args(argv)
    checked, findings = check(a.target)
    if a.json:
        print(json.dumps({"checked": checked, "errors": len(findings), "findings": findings}, indent=1))
    else:
        for f in findings:
            print("ERROR %s: %s" % (f["where"], f["problem"]))
        print("checked %d calls; %d error(s)" % (checked, len(findings)))
    return 1 if findings else 0


if __name__ == "__main__":
    sys.exit(main())
