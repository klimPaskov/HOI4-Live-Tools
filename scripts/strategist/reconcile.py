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
import re
import shlex
import shutil
import subprocess
import sys
import unicodedata


def norm(name):
    """Case, accents, punctuation and the degree sign do not distinguish names."""
    s = unicodedata.normalize("NFKD", str(name)).replace("°", " ").replace("º", " ")
    s = "".join(c for c in s if not unicodedata.combining(c))
    return re.sub(r"[^a-z0-9]+", " ", s.casefold()).strip()


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


def live_names(call):
    """{kind: [names]} and info, from read tools only."""
    names, info = {}, {}
    t = call("hoi4_division_templates", {})
    names["template"] = [r.get("template") for r in t.get("templates") or [] if r.get("template")]
    c = call("hoi4_commanders", {})
    names["general"] = [r.get("name") for r in c.get("commanders") or [] if r.get("name")]
    w = call("hoi4_air_wings", {})
    names["wing"] = [r.get("wing") for r in w.get("wings") or [] if r.get("wing")]
    # Wing names are made at runtime, so plans may select wings as "<variant> at <state id>"
    # (the dossier knows the start OOB's variants and bases); resolve that from hoi4_air.
    air = call("hoi4_air", {})
    base_state = {}
    for b in air.get("air_bases") or []:
        for name in b.get("wings") or []:
            base_state[name] = b.get("state_id")
    # (wing, variant, state, airframe type): a selector may name the variant or the airframe id
    names["wing_by_selector"] = [(r.get("wing"), r.get("variant"), base_state.get(r.get("wing")),
                                  r.get("aircraft_type"))
                                 for r in air.get("wings") or [] if r.get("wing")]
    lines = call("hoi4_production_lines", {})
    names["equipment"] = sorted({r.get("equipment_id") for r in lines.get("lines") or [] if r.get("equipment_id")})
    info["production_lines_at_start"] = [
        {"equipment": r.get("equipment_id"),
         "factories": (r.get("factory_counters") or {}).get("active_factories_raw_signed")}
        for r in lines.get("lines") or []]
    techs = call("hoi4_technology_list", {"available": True})
    names["tech_available"] = list(techs.get("technologies") or [])
    return names, info


def references(plan):
    """[(kind, path, getter, setter)] for every live-checked name in the plan."""
    refs = []

    def add(kind, container, key, path):
        value = container.get(key)
        if isinstance(value, str) and value:
            refs.append((kind, path, value, lambda new, c=container, k=key: c.__setitem__(k, new)))
        elif isinstance(value, list):
            for i, v in enumerate(value):
                if isinstance(v, str) and v:
                    refs.append((kind, "%s[%d]" % (path, i), v, lambda new, c=value, i=i: c.__setitem__(i, new)))

    for i, e in enumerate(plan.get("army_layout") or []):
        if isinstance(e, dict):
            add("template", e, "templates", "army_layout[%d].templates" % i)
            add("general", e, "general", "army_layout[%d].general" % i)
    training = plan.get("training")
    for i, e in enumerate(training if isinstance(training, list) else [training] if training else []):
        if isinstance(e, dict):
            add("template", e, "template", "training[%d].template" % i)
    for i, e in enumerate(plan.get("templates") or []):
        if isinstance(e, dict):
            add("template", e, "base", "templates[%d].base" % i)
    for i, e in enumerate(plan.get("army_group") or []):
        if isinstance(e, dict):
            add("general", e, "marshal", "army_group[%d].marshal" % i)
    armies = plan.get("armies") or {}
    if isinstance(armies, dict):
        gens = armies.get("generals")
        if isinstance(gens, list):
            add("general", armies, "generals", "armies.generals")
        elif isinstance(gens, dict):
            for k in list(gens):
                add("general", gens, k, "armies.generals.%s" % k)
    for i, e in enumerate(plan.get("air_plan") or []):
        if isinstance(e, dict):
            add("wing", e, "wing", "air_plan[%d].wing" % i)
    for i, e in enumerate(plan.get("production") or []):
        if isinstance(e, dict):
            for key in ("remove", "replace"):
                if isinstance(e.get(key), str):
                    add("equipment", e, key, "production[%d].%s" % (i, key))
    return refs


def first_block(plan):
    blocks = [b for b in plan.get("research_path") or [] if isinstance(b, dict) and b.get("techs")]
    if not blocks:
        return []
    year = min(int(b.get("year", 9999)) for b in blocks)
    return [t for b in blocks if int(b.get("year", 9999)) == year for t in b["techs"]]


SELECTOR = re.compile(r"^\s*(.+?)\s+at\s+(\d+)\s*$")


def expand_wing_selectors(plan, names, patched, unresolved):
    """Replace air_plan entries whose wing is "<variant or airframe> at <state>" by one entry per live wing."""
    entries = plan.get("air_plan")
    if not isinstance(entries, list):
        return
    out = []
    for i, e in enumerate(entries):
        m = SELECTOR.match(e.get("wing", "")) if isinstance(e, dict) and isinstance(e.get("wing"), str) else None
        if not m or e["wing"] in (names.get("wing") or []):
            out.append(e)
            continue
        variant, state = norm(m.group(1)), int(m.group(2))
        sel = [tuple(r) + (None,) * (4 - len(r)) for r in names.get("wing_by_selector") or []]
        hits = [w for w, v, st, t in sel
                if st == state and ((v and norm(v) == variant) or (t and norm(t) == variant))]
        if not hits:
            unresolved.append({"kind": "wing", "path": "air_plan[%d].wing" % i, "name": e["wing"],
                               "candidates": ["%s at %s" % (v, st) for _, v, st, _ in sel][:8]})
            out.append(e)
            continue
        for w in hits:
            out.append(dict(e, wing=w, selector=e["wing"]))
        patched.append({"kind": "wing", "path": "air_plan[%d].wing" % i, "from": e["wing"], "to": hits})
    plan["air_plan"] = out


def reconcile(plan, names):
    """(patched, unresolved, research) and patches `plan` in place."""
    patched, unresolved = [], []
    expand_wing_selectors(plan, names, patched, unresolved)
    for kind, path, value, setter in references(plan):
        live = names.get(kind) or []
        if value in live or (kind == "wing" and SELECTOR.match(value)):
            continue  # selectors were resolved (or listed) by expand_wing_selectors
        matches = [n for n in live if norm(n) == norm(value)]
        if len(matches) == 1:
            setter(matches[0])
            patched.append({"kind": kind, "path": path, "from": value, "to": matches[0]})
        else:
            unresolved.append({"kind": kind, "path": path, "name": value,
                               "candidates": matches or sorted(live, key=lambda n: _distance(n, value))[:5]})
    available = set(names.get("tech_available") or [])
    block = first_block(plan)
    research = {"first_block": block, "available_now": [t for t in block if t in available],
                "not_yet_available": [t for t in block if t not in available]}
    return patched, unresolved, research


def _distance(a, b):
    a, b = norm(a), norm(b)
    return 0 if a == b else len(set(a.split()) ^ set(b.split())) + abs(len(a) - len(b)) / 100.0


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("--run", required=True)
    ap.add_argument("--call-cmd", required=True)
    ap.add_argument("--dry-run", action="store_true", help="report only; do not patch plan.json")
    args = ap.parse_args(argv)
    run_dir = os.path.abspath(args.run)
    plan_path = os.path.join(run_dir, "plan.json")
    with open(plan_path, encoding="utf-8") as f:
        plan = json.load(f)
    try:
        names, info = live_names(Caller(args.call_cmd))
    except (RuntimeError, ValueError, OSError, subprocess.TimeoutExpired) as e:
        print(json.dumps({"ok": False, "error": str(e)}))
        return 1
    patched, unresolved, research = reconcile(plan, names)
    if patched and not args.dry_run:
        backup = os.path.join(run_dir, "plan.pre-reconcile.json")
        if not os.path.exists(backup):
            shutil.copy2(plan_path, backup)
        tmp = plan_path + ".tmp"
        with open(tmp, "w", encoding="utf-8") as f:
            json.dump(plan, f, indent=1, ensure_ascii=False)
            f.write("\n")
        os.replace(tmp, plan_path)
    report = {"ok": not unresolved, "patched": patched, "unresolved": unresolved, "research": research,
              "info": info, "dry_run": args.dry_run}
    with open(os.path.join(run_dir, "reconcile.json"), "w", encoding="utf-8") as f:
        json.dump(report, f, indent=1, ensure_ascii=False)
    print(json.dumps({"ok": report["ok"], "patched": len(patched), "unresolved": len(unresolved),
                      "research_not_yet_available": len(research["not_yet_available"])}))
    return 0 if report["ok"] else 2


if __name__ == "__main__":
    sys.exit(main())
