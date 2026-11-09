#!/usr/bin/env python3
"""check_calls.py: every tool call a plan hands to the player must be valid against the tool catalogue.

Usage:
    check_calls.py RUN_OR_PLAN [--json]

Checks `call` fields anywhere in plan.json (player-owned items name the exact call) and every
`hoi4_tool {json}` span in plan.brief.md and plan.md (a bare `hoi4_tool` in prose is only checked to
exist): the tool exists, required arguments are present,
no unknown arguments, types and enums match (scripts/hoi4facts/tool_schemas.py; catalogue
skills/hoi4-campaign-playbook/tools.schema.json, rebuilt from native/src). Placeholders such as
"<wing>" are allowed as values. A brief call that duplicates work the executor owns in this plan
(an army order to a role army, production, research, focus, construction, laws, templates, events in
the plan, ...) is an error too: the executor does it, the player only does what it cannot. Exit 1 when any call is invalid. Stdlib only, Python 3.9+.
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


# A player call that does what the executor already does for this plan (the user: "lots of duplicate and
# failed actions"): tool prefix -> the plan keys whose presence makes the executor own that work.
EXECUTOR_OWNS = [
    ("hoi4_production_set", ("production",)), ("hoi4_production_add", ("production",)),
    ("hoi4_research_start", ("research_path", "research_priority")),
    ("hoi4_focus_start", ("focus_path",)),
    ("hoi4_construction_", ("construction", "construction_plan")), ("hoi4_railway_build", ("construction_plan",)),
    ("hoi4_politics_choose", ("politics",)), ("hoi4_decision_take", ("decisions", "decision_rules")),
    ("hoi4_template_", ("templates",)), ("hoi4_train", ("training",)),
    ("hoi4_air_mission", ("air_plan", "air")), ("hoi4_air_rebase", ("air_plan",)),
    ("hoi4_occupation_law", ("occupation",)), ("hoi4_peace_", ("peace",)),
    ("hoi4_diplomacy_act", ("diplomacy",)), ("hoi4_doctrine_adopt", ("doctrines",)),
    ("hoi4_army_group_create", ("army_group",)), ("hoi4_mio_trait", ("mio",)), ("hoi4_design_", ("designs",)),
    ("hoi4_trade_set", ("trade",)), ("hoi4_fleet_", ("navy",)), ("hoi4_naval_invasion", ("invasions",)),
    ("hoi4_faction_", ("faction",)), ("hoi4_agency_", ("intelligence",)),
]
ARMY_ORDERS = ("hoi4_army_front", "hoi4_army_move", "hoi4_army_attack", "hoi4_army_offensive", "hoi4_army_hold",
               "hoi4_army_commander", "hoi4_army_detach", "hoi4_army_create", "hoi4_army_merge", "hoi4_army_split")


def role_armies(plan):
    """Army names and layout roles the executor drives in this plan."""
    out = set()
    for e in plan.get("army_layout") or []:
        if isinstance(e, dict) and e.get("name_hint"):
            out.add(e["name_hint"])
    for e in plan.get("army_group") or []:
        if isinstance(e, dict):
            out.update(a for a in e.get("armies") or [] if isinstance(a, str))
    for e in plan.get("hold_targets") or []:
        if isinstance(e, dict) and isinstance(e.get("army"), str):
            out.add(e["army"])
    return out


def overlap(plan, tool, args):
    """Why this player call duplicates executor work, or None."""
    for prefix, keys in EXECUTOR_OWNS:
        if tool.startswith(prefix) and any(plan.get(k) for k in keys):
            return "the executor owns this through %s; put it in the plan, not the brief" % "/".join(
                k for k in keys if plan.get(k))
    if tool in ARMY_ORDERS and isinstance(args, dict):
        army = args.get("army") or args.get("source")
        if army in role_armies(plan) and (plan.get("war") or plan.get("vp_targets") or plan.get("hold_targets")):
            return "army %s has a role in the plan, so the executor gives its orders" % army
    return None


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
        if where.startswith("plan.brief.md") and isinstance(args, dict):
            why = overlap(plan, tool, args)
            if why:
                findings.append({"where": where, "tool": tool, "problem": "duplicates the executor: " + why})
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
