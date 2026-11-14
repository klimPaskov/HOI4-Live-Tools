#!/usr/bin/env python3
"""plan_keys.py: the plan-key reconcile. Every key plan.schema.json accepts has exactly one reader.

Usage:
    plan_keys.py check [--schema PATH]     # exit 1 with one line per key that has no reader
    plan_keys.py show [--schema PATH]      # every key path with its reader

A plan key that steers nothing is worse than a missing one: the plan reads as if the executor followed
it. So each key path of the schema (x-owner on the key, else inherited from the key above it) is one of
  executor  the standing-plan executor reads it (native/src/plan_run.rs and its child modules);
  lint      only the planning tools read it (lint, the validator, the evaluator): it shapes estimates,
            the executor does not act on it, and the lint lists it for every plan that sets it;
  text      for people: a label or commentary nothing reads (`why` and `note` are text without a mark);
  player    a top-level key the player acts on from the brief. Inside a key the executor owns, a
            player key is read by nobody, so it is allowed there only with x-executor-planned (the
            build item whose executor step takes it over).
`check` fails when
  * the executor reads a key the schema does not list or does not give to the executor;
  * a key marked executor is read nowhere in the executor's code;
  * a key inside an executor-owned key has no mark and its name is nowhere in the executor's code;
  * a player key sits inside an executor-owned key without a planned build item;
  * a lint key is read by the executor, or its name is nowhere in the planning tools;
  * a text key is read by the executor.
Reads are found in the sources (comments and tests stripped): `plan["key"]` and the helper reads the
tables below name, and for unmarked nested keys the key's name as a string literal. Stdlib only,
Python 3.9+.
"""
import argparse
import json
import os
import re
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(os.path.dirname(HERE))
if os.path.dirname(HERE) not in sys.path:
    sys.path.insert(0, os.path.dirname(HERE))

from hoi4facts import schema_check  # noqa: E402

SCHEMA_PATH = os.path.join(ROOT, "skills", "hoi4-campaign-playbook", "plan.schema.json")
PLAN_RUN = os.path.join(ROOT, "native", "src", "plan_run.rs")
# The runner's child modules read plan keys too (plan_manage.rs: continuous management;
# plan_military.rs: army groups, navy, invasions, casualty and air superiority rules;
# plan_predicate.rs: the predicate evaluator; plan_affairs.rs: designs, diplomacy, faction, trade,
# MIOs, intelligence, research bonuses; plan_war.rs: the war-step rules). Missing files are skipped.
EXECUTOR_SOURCES = [PLAN_RUN] + [os.path.join(ROOT, "native", "src", name)
                                 for name in ("plan_manage.rs", "plan_military.rs", "plan_predicate.rs",
                                              "plan_affairs.rs", "plan_war.rs", "plan_route.rs")]
# Sources searched for a key's name when no read pattern finds it (helpers with their own variables).
LITERAL_SOURCES = EXECUTOR_SOURCES + [os.path.join(ROOT, "native", "src", name)
                                      for name in ("plan_formations.rs", "alarms.rs", "watch.rs",
                                                   # alarms.<code>: the hoi4_wars alert codes are the keys
                                                   os.path.join("launch", "macos", "wars.rs"))]
# The planning tools: what reads a lint key.
TOOL_SOURCES = [os.path.join(ROOT, "scripts", *parts) for parts in (
    ("hoi4facts", "lint_plan.py"), ("hoi4facts", "lint_fair.py"), ("hoi4facts", "validate.py"),
    ("hoi4facts", "plan_gates.py"), ("hoi4facts", "check_calls.py"), ("strategist", "eval_plan.py"),
    ("strategist", "reconcile.py"), ("validate_plan.py",))]
OWNERS = ("executor", "player", "lint", "text")
TEXT_KEYS = ("why", "note")
# Definitions whose inner keys are grammar, not plan keys of their own (the predicate tree, a date).
GRAMMAR_REFS = ("#/definitions/predicate", "#/definitions/date_object", "#/definitions/when")
ALARMS = os.path.join(ROOT, "native", "src", "alarms.rs")
HAND_PLANS = [
    os.path.join(ROOT, "skills", "hoi4-campaign-playbook", "plans", name)
    for name in ("italy-ethiopia-1936.plan.json", "ethiopia-italy-1936.plan.json")
]

# Per-item fields: list key -> (functions that read its entries, the first one walks the list;
# loop variable names there).
ENTRY_VARS = {
    "politics": (["politics_step", "politics_auto_step"], ["pick"]),
    "production": (["production_step", "wanted_switches"], ["line"]),
    "research_path": (["tick"], ["step"]),
    "construction": (["construction_step"], ["job"]),
    "construction_plan": (["construction_plan_step"], ["item"]),
    "army_layout": (["layout_step"], ["e", "x"]),
    "doctrines": (["extras_step"], ["entry"]),
    "vp_targets": (["vp_choice", "war_step", "waypoints", "choose"], ["t", "target"]),
    "training": (["training_step"], ["tr"]),
    "templates": (["templates_step"], ["entry"]),
    "air_plan": (["air_plan_step", "wing_matches", "rebase_target"], ["entry"]),
    "decision_rules": (["decisions_step", "decision_forbidden"], ["rule", "r"]),
    "event_rules": (["event_rule_choice"], ["rule"]),
    "designs": (["designs_step", "wanted_switches"], ["entry", "e"]),
    "diplomacy": (["diplomacy_step", "justify", "declare", "relation", "widening", "expiry_watch",
                   "justify_items", "dip_target"], ["item", "j", "o"]),
    "mio": (["mio_step"], ["entry"]),
    "trade": (["trade_step"], ["item"]),
    "army_group": (["army_group_step", "marshals"], ["entry", "e"]),
    "invasions": (["invasion_step", "invasion_fleet", "invasion_claims"], ["entry", "e"]),
}
# Plan objects the child modules read through a local variable in a helper: function -> {variable: key path}.
NESTED_VARS = {
    "fleet_plans": {"cfg": "navy", "f": "navy.fleets[]", "escort": "navy.convoy_escort"},
    "admiral_plans": {"cfg": "navy", "f": "navy.fleets[]", "r": "navy.admirals[]"},
    "region_spec": {"v": "navy.fleets[].missions[].regions[]"},
    "navy_step": {"cfg": "navy", "m": "navy.fleets[].missions[]"},
    "faction_step": {"cfg": "faction", "create": "faction.create"},
    "intelligence_step": {"cfg": "intelligence", "op": "intelligence.operatives[]"},
    "upgrade_ref": {"entry": "intelligence.upgrades[]"},
    "casualty_rule": {"m": "manage"},
    "air_superiority_rule": {"m": "manage"},
    "politics_auto_step": {"entry": "politics_auto.order[]"},
    "politics_auto_choice": {"entry": "politics_auto.order[]"},
}
# Executor-owned keys whose step is being built in parallel to an agreed shape: tagged executor before the
# code reads them (the unread check skips them; their fields are checked once the code lands).
LANDING = {}
# Fields the executor still reads that the schema deliberately leaves out.
KNOWN_GAPS = {
    "doctrines[].doctrine": "old {doctrine, track} entry, still read for one release; plans use the new "
                            "shape {id, folder, grand_doctrine?, track?, subdoctrine?, xp}",
    "templates[].regimental": "alias of regimental_support the templates step also skips; plans write "
                              "regimental_support",
}


def _code(path):
    with open(path, encoding="utf-8") as f:
        text = f.read()
    text = text.split("#[cfg(test)]", 1)[0]
    return "\n".join(line.split("//", 1)[0] for line in text.splitlines())


def _functions(code):
    """{function name: body} (a body runs to the next top-level fn)."""
    parts = re.split(r"^(?:pub(?:\(\w+\))? )?fn (\w+)", code, flags=re.M)
    return {parts[i]: parts[i + 1] for i in range(1, len(parts), 2)}


def executor_functions():
    """{function name: body} over plan_run.rs and its child modules (code only, comments and
    tests stripped; a name defined twice keeps both bodies)."""
    out = {}
    for path in EXECUTOR_SOURCES:
        if os.path.exists(path):
            for name, body in _functions(_code(path)).items():
                out[name] = out.get(name, "") + body
    return out


def consumed_keys():
    """Plan key paths the executor reads: "war", "war.enemies", "politics[].idea", ..."""
    keys = set()
    functions = executor_functions()
    for body in functions.values():
        for k in re.findall(r'\bplan\["(\w+)"\]', body):
            keys.add(k)
        for k in re.findall(r'\bplan\.get\("(\w+)"\)', body):
            keys.add(k)
        for k in re.findall(r'\b(?:strings|list)\(&?plan, "(\w+)"\)', body):
            keys.add(k)
        for k in re.findall(r'\bskip_hours\(plan, "(\w+)"', body):
            keys.update(("skip_hours", "skip_hours." + k))
        for a, b in re.findall(r'\bplan\["(\w+)"\]\["(\w+)"\]', body):
            keys.add(a + "." + b)
        for a, b in re.findall(r'\b(?:strings|list)\(&plan\["(\w+)"\], "(\w+)"\)', body):
            keys.add(a + "." + b)
        for var, top in re.findall(r'let (\w+) = &plan\["(\w+)"\];', body):
            for k in re.findall(r"\b%s\[\"(\w+)\"\]" % var, body):
                keys.add(top + "." + k)
            for k in re.findall(r"\b%s\.get\(\"(\w+)\"\)" % var, body):
                keys.add(top + "." + k)
            for k in re.findall(r"\b(?:strings|list)\(%s, \"(\w+)\"\)" % var, body):
                keys.add(top + "." + k)
    for top, (fns, names) in ENTRY_VARS.items():
        if not re.search(r'plan, "%s"\)|plan\["%s"\]' % (top, top), functions.get(fns[0], "")):
            raise AssertionError("ENTRY_VARS is stale: %s no longer walks %s" % (fns[0], top))
        body = "".join(functions.get(fn, "") for fn in fns)
        for var in names:
            for k in re.findall(r"\b%s\[\"(\w+)\"\]" % var, body):
                keys.add("%s[].%s" % (top, k))
            for k in re.findall(r"\b(?:strings|list)\(&?%s, \"(\w+)\"\)" % var, body):
                keys.add("%s[].%s" % (top, k))
            if re.search(r"\b(?:item_id|invasion_key)\(&?%s\b" % var, body):
                keys.add("%s[].id" % top)
            if re.search(r"\breached\(%s\b" % var, body):
                keys.add("%s[].from" % top)
    for fn, paths in NESTED_VARS.items():
        if fn not in functions:
            raise AssertionError("NESTED_VARS is stale: no function %s" % fn)
        for var, path in paths.items():
            for k in re.findall(r"\b%s\[\"(\w+)\"\]" % var, functions[fn]):
                keys.add("%s.%s" % (path, k))
            for k in re.findall(r"\b(?:strings|list)\(&?%s, \"(\w+)\"\)" % var, functions[fn]):
                keys.add("%s.%s" % (path, k))
    alarms = _code(ALARMS)
    for k in re.findall(r'\bplan(?:\.get\(|\[)"(\w+)"', alarms):
        keys.add(k)
    for a, b in re.findall(r'\bplan\["(\w+)"\]\["(\w+)"\]', alarms):
        keys.add(a + "." + b)
    for k in re.findall(r'\b(?:enabled|threshold)\(config, "(\w+)"', alarms):
        keys.add("alarms." + k)
    return keys


def load_schema(path=SCHEMA_PATH):
    with open(path, encoding="utf-8") as f:
        return json.load(f)


def _deref(node, schema):
    while "$ref" in node:
        node = schema_check._resolve(node["$ref"], schema)
    return node


def lookup(schema, path):
    """The schema node for a key path ("war.enemies", "politics[].idea"), or None."""
    node = schema
    for part in path.split("."):
        node = _deref(node, schema)
        if part.endswith("[]"):
            part = part[:-2]
            item = _deref(node, schema).get("properties", {}).get(part)
            if item is None:
                return None
            node = _deref(item, schema).get("items")
            if node is None:
                return None
            continue
        props = node.get("properties", {})
        if part not in props:
            return None
        node = props[part]
    return node


def _literals(paths):
    out = set()
    for path in paths:
        if not os.path.exists(path):
            continue
        if path.endswith(".rs"):
            text = _code(path)
        else:
            with open(path, encoding="utf-8") as f:
                text = f.read()
        out.update(re.findall(r"[\"']([A-Za-z_][A-Za-z0-9_]*)[\"']", text))
    return out


def schema_keys(schema):
    """[{path, name, owner, marked, planned, above}] for every key path below the top level's
    properties: `owner` is the key's reader (its own x-owner, else text for why/note, else the owner of
    the key above), `marked` whether the key carries its own x-owner, `above` the owner of the key
    above. Grammar definitions (predicates, dates) and the override blocks, which repeat the top-level
    keys, are not descended into."""
    out = []

    def walk(node, path, above, trail):
        own = node.get("x-owner")
        planned = node.get("x-executor-planned")
        grammar = node.get("$ref") in GRAMMAR_REFS
        node = _deref(node, schema)
        own = own or node.get("x-owner")
        planned = planned or node.get("x-executor-planned")
        name = path.rsplit(".", 1)[-1]
        owner = own or ("text" if name in TEXT_KEYS else above)
        if path:
            out.append({"path": path, "name": name, "owner": owner, "marked": own is not None,
                        "planned": planned, "above": above})
        if grammar or name == "overrides" or id(node) in trail:
            return
        trail = trail | {id(node)}
        for k, v in (node.get("properties") or {}).items():
            if isinstance(v, dict):
                walk(v, "%s.%s" % (path, k) if path else k, owner, trail)
        items = node.get("items")
        if isinstance(items, dict):
            inner = _deref(items, schema)
            if any(items.get("$ref") == g for g in GRAMMAR_REFS):
                return
            for k, v in (inner.get("properties") or {}).items():
                if isinstance(v, dict):
                    walk(v, "%s[].%s" % (path, k), owner, trail | {id(inner)})

    walk(schema, "", None, frozenset())
    return out


def check(schema=None):
    """Problems as [(key path, message)]; [] when every key has its reader."""
    schema = load_schema() if schema is None else schema
    reads = consumed_keys()
    code, tools = _literals(LITERAL_SOURCES), _literals(TOOL_SOURCES)
    problems = []
    for k in sorted(reads):
        if k in KNOWN_GAPS:
            continue
        node = lookup(schema, k)
        if node is None:
            problems.append((k, "the executor reads it but plan.schema.json does not list it"))
    keys = schema_keys(schema)
    by_path = {e["path"]: e for e in keys}
    for k in sorted(reads):
        e = by_path.get(k)
        if e is not None and e["owner"] != "executor":
            problems.append((k, "the executor reads it, so its x-owner is executor, not %s" % e["owner"]))
    for e in keys:
        path, owner, top = e["path"], e["owner"], "." not in e["path"] and "[" not in e["path"]
        read = path in reads or (top and path in {k.split(".")[0].split("[")[0] for k in reads})
        if owner not in OWNERS:
            problems.append((path, "x-owner %r is not one of %s" % (owner, ", ".join(OWNERS))))
        elif top:
            if owner not in ("executor", "player"):
                problems.append((path, "a top-level key is the executor's or the player's, not %s" % owner))
            elif owner == "executor" and not read and path not in LANDING:
                problems.append((path, "marked executor but the executor never reads it"))
        elif owner == "executor":
            if not read and e["name"] not in code:
                problems.append((path, (
                    "marked executor but nothing in the executor reads it" if e["marked"] else
                    "inside an executor-owned key, but nothing in the executor reads it") +
                    ": make the executor read it, or mark it x-owner lint (only the planning tools read it) "
                    "or text (for people)"))
        elif owner == "player":
            if e["above"] == "executor" and e["marked"] and not e["planned"]:
                problems.append((path, "a player key inside an executor-owned key is read by nobody: make the "
                                       "executor read it, mark it lint or text, or name the build item that takes it "
                                       "over (x-executor-planned)"))
        elif owner == "lint":
            if e["name"] not in tools:
                problems.append((path, "marked lint but no planning tool reads it"))
            if not (lookup(schema, path) or {}).get("description"):
                problems.append((path, "a lint key says in its own description what it shapes"))
    return problems


def readers(schema=None):
    """{key path: reader} for every key path of the schema."""
    schema = load_schema() if schema is None else schema
    return {e["path"]: ("planned" if e["planned"] and e["owner"] != "executor" else e["owner"])
            for e in schema_keys(schema)}


def used_paths(plan, schema):
    """Key paths of the schema that `plan` sets (list items as key[])."""
    out = set()

    def walk(value, node, path):
        grammar = node.get("$ref") in GRAMMAR_REFS
        node = _deref(node, schema)
        if grammar:
            return
        if isinstance(value, dict):
            for k, v in value.items():
                sub = (node.get("properties") or {}).get(k)
                if isinstance(sub, dict):
                    p = "%s.%s" % (path, k) if path else k
                    out.add(p)
                    if k != "overrides":
                        walk(v, sub, p)
        elif isinstance(value, list) and isinstance(node.get("items"), dict):
            for v in value:
                walk(v, node["items"], path + "[]")

    walk(plan, schema, "")
    return out


def unread_in(plan, schema):
    """{reader: [key paths]} of the keys `plan` sets that the executor does not act on although they sit
    inside a key it owns: lint keys (planning estimates only) and planned keys (no executor step yet)."""
    info = {e["path"]: e for e in schema_keys(schema)}
    out = {}
    for p in sorted(used_paths(plan, schema)):
        e = info.get(p)
        if e is None or p.startswith("_") or e["above"] != "executor" and not e["planned"]:
            continue
        kind = "planned" if e["planned"] and e["owner"] != "executor" else e["owner"]
        if kind in ("lint", "planned") and e["marked"]:
            out.setdefault(kind, []).append(p)
    return out


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("cmd", choices=("check", "show"))
    ap.add_argument("--schema", default=SCHEMA_PATH)
    a = ap.parse_args(argv)
    schema = load_schema(a.schema)
    if a.cmd == "show":
        for path, reader in sorted(readers(schema).items()):
            print("%-8s %s" % (reader, path))
        return 0
    problems = check(schema)
    for path, msg in problems:
        print("%s: %s" % (path, msg))
    print("plan keys: %d key paths, %d without a reader" % (len(schema_keys(schema)), len(problems)))
    return 1 if problems else 0


if __name__ == "__main__":
    sys.exit(main())
