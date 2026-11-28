#!/usr/bin/env python3
"""computed.py: the evaluators' answers for a run, before the strategist plans and before the game is unpaused.

    computed.py --run RUN [--game DIR] [--mod PATH ...] [--userdir DIR]

Reads RUN/run_spec.json (tag, start_date, horizon, optional war_date, enemies, goals such as ideology:fascism or
no_war:FRA, grand_doctrine, fighter_ratio, war_days) and writes RUN/computed.json and RUN/computed.md: design,
templates, research, production, construction, force, doctrines and focus at the start date; subdoctrines, axis supply,
air need, the fuel ledger at the war date; training size; every line tagged "computed" with the command that
reproduces it. computed.json["summary"] holds one flat value per decision (claims name them summary.<key>). A country with no playbook reference (an unresearched country) plans from these; where a reference or
RUN/guide-claims.json exists, each claim is printed beside the computed answer:
"the guide says X; computed says Y; difference Z".

RUN/guide-claims.json (optional, run-local, may come from private sources): [{"what": "construction.k", "says": 1.92,
"where": "the source"}]; `what` is a dotted path into computed.json. Stdlib only, Python 3.9+.
"""
import argparse
import json
import os
import sys
import traceback

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(os.path.dirname(HERE))
sys.path.insert(0, os.path.join(ROOT, "scripts"))
REFERENCES = os.path.join(ROOT, "skills", "hoi4-campaign-playbook", "references")


def references_for(name, tag):
    """Playbook references named after the country (its English name or tag): country knowledge."""
    out = []
    keys = {k for k in (name or "").lower().replace(" ", "-").split("-") if len(k) > 3} | {tag.lower()}
    for f in sorted(os.listdir(REFERENCES)) if os.path.isdir(REFERENCES) else []:
        stem = f.rsplit(".", 1)[0].lower()
        if any(stem.startswith(k) or stem.split("-")[0] == k for k in keys):
            out.append(f)
    return out


def dig(obj, path):
    for part in path.split("."):
        if isinstance(obj, list):
            try:
                obj = obj[int(part)]
            except (ValueError, IndexError):
                return None
        elif isinstance(obj, dict):
            obj = obj.get(part)
        else:
            return None
    return obj


def compare(claims, data):
    rows = []
    for c in claims:
        got = dig(data, c.get("what", ""))
        says = c.get("says")
        diff = None
        if isinstance(got, (int, float)) and isinstance(says, (int, float)):
            diff = round(got - says, 4)
        elif got is None:
            diff = "not computed"
        else:
            diff = "same" if got == says else "differs"
        rows.append({"what": c.get("what"), "guide": says, "computed": got, "difference": diff,
                     "where": c.get("where")})
    return rows


def run(spec, game_dir, mods=(), dlcs=None):
    from hoi4facts.evaluators import construction, design, focus, force, production, research, template
    from hoi4facts.evaluators.context import Situation
    from hoi4facts.game import Game

    tag, date = spec["tag"], spec.get("start_date", "1936.1.1")
    sit = Situation.load(Game(game_dir, list(mods)), tag, date, dlcs=dlcs)
    enemies, how = sit.enemies(spec.get("enemies") or None)
    war = spec.get("war_date") or spec.get("horizon")
    goals = list(spec.get("goals") or [])
    out = {"tag": tag, "date": date, "enemies": enemies, "enemies_how": how, "war_date": war, "goals": goals,
           "errors": {}}

    def step(name, fn):
        try:
            out[name] = fn()
        except Exception as e:  # noqa: BLE001 - one evaluator failing never blocks the others
            out["errors"][name] = f"{type(e).__name__}: {e}"
            out["errors"][name + "_trace"] = traceback.format_exc()[-600:]

    step("construction", lambda: construction.evaluate(sit, war))
    step("research", lambda: research.evaluate(sit, picks=3, enemies=enemies))
    step("template", lambda: {r: template.best(sit, r, enemies=enemies, top=1) for r in ("line", "attacker", "armour")})
    step("design", lambda: design.best(sit, "tank_main", enemies=enemies, top=1))
    step("force", lambda: force.needed(sit, enemies[0], war) if enemies else None)
    step("doctrine", lambda: force.doctrines(sit, enemies))
    step("production", lambda: production.evaluate(sit, war, add={"line": (out.get("force") or {}).get("missing", 0)},
                                                   enemies=enemies) if war else None)
    step("focus", lambda: focus.evaluate(sit, goals=goals, count=10))
    # the war-date evaluators judge the situation on the war date (history to the start date, techs on time)
    wsit = Situation.load(Game(game_dir, list(mods)), tag, war, dlcs=dlcs, bookmark=date) if war else sit
    grand = spec.get("grand_doctrine")
    step("subdoctrine", lambda: _ev("force").subdoctrines(wsit, enemies, grand=grand))
    step("supply", lambda: _ev("logistics").axis_supply(wsit, enemies[0]) if enemies else None)
    step("air", lambda: _ev("air").need(wsit, enemies, axes=(out.get("force") or {}).get("axes") or 1,
                                        fighter_ratio=spec.get("fighter_ratio"), war_date=war))
    step("fuel", lambda: _fuel(wsit, enemies, out, war, spec.get("war_days") or 30))
    step("training", lambda: _ev("production").training(sit, "line", count=(out.get("force") or {}).get("missing"),
                                                         need_date=war, enemies=enemies) if war else None)
    out["summary"] = summary(out)
    out["references"] = references_for(sit.world.name(tag) if hasattr(sit.world, "name") else "", tag)
    return out


def _ev(name):
    import importlib
    return importlib.import_module("hoi4facts.evaluators." + name)


def _fuel(sit, enemies, out, war, war_days):
    """The fuel ledger for the force's attack divisions (evaluator 2's attack role) plus the fielded divisions and
    the air need's planes."""
    import types
    from hoi4facts import evaluate as ev
    f = out.get("force") or {}
    arm = ((out.get("template") or {}).get("armour") or {}).get("templates") or [{}]
    role = "armour" if f.get("attack_template") and f.get("attack_template") == arm[0].get("battalions") else "attacker"
    divs = ev._divisions(types.SimpleNamespace(enemy=enemies, division=[f"{role}:{f.get('attack') or 0}"]), sit)
    a = out.get("air") or {}
    planes = []
    for role, mission, n in (("fighter", "AIR_SUPERIORITY", a.get("fighters")), ("cas", "CAS", a.get("cas"))):
        if n:
            dz = _ev("design").best(sit, role, enemies=enemies)
            f = float(((dz.get("designs") or [{}])[0].get("stats") or {}).get("fuel_consumption", 0.0))
            planes.append({"name": f"{role} (air need, evaluator 1's design)", "count": n, "fuel": f,
                           "mission": mission})
    r = _ev("logistics").fuel(sit, divs, war_date=war, war_days=war_days, planes=planes,
                              enemy=enemies[0] if enemies else None)
    return r


def summary(d):
    """Flat answers for guide claims (`what` = summary.<key>): one value per decision."""
    s = {}

    def put(k, fn):
        try:
            v = fn()
        except Exception:  # noqa: BLE001 - an evaluator that failed or gave no answer: left out
            return
        if v is not None:
            s[k] = v
    put("construction_k", lambda: d["construction"]["k"])
    put("construction_decision", lambda: d["construction"]["decision"])
    put("research_first", lambda: [sl[0]["tech"] for sl in d["research"]["picks"] if sl])
    for role in ("line", "attacker", "armour"):
        put(f"template_{role}", lambda role=role: d["template"][role]["templates"][0]["battalions"])
        put(f"template_{role}_width", lambda role=role: d["template"][role]["templates"][0]["width"])
        put(f"template_{role}_support", lambda role=role: d["template"][role]["templates"][0]["support"])
    put("design_tank_main", lambda: d["design"]["designs"][0]["base"])
    put("force_total", lambda: d["force"]["total"])
    put("force_hold", lambda: d["force"]["hold"])
    put("force_attack", lambda: d["force"]["attack"])
    put("doctrine_line_best", lambda: max(d["doctrine"]["rows"],
                                          key=lambda x: (x.get("line") or {}).get("all milestones", 0))["doctrine"])
    put("doctrine_attack_best", lambda: max(
        d["doctrine"]["rows"], key=lambda x: max(v["R"] for by in (x.get("attack") or {}).values()
                                                 for t in by.values() for v in t.values()))["doctrine"])
    put("subdoctrines", lambda: {t["track"]: t["chosen"] for t in d["subdoctrine"]["tracks"]})
    put("production_lines", lambda: {x["variant"]: x["factories"] for x in d["production"]["lines"] if x.get("factories")})
    put("trains_factories", lambda: d["production"]["logistics"]["trains"])
    put("focus_first", lambda: [o["focus"] for o in d["focus"]["order"]])
    put("army_xp_source", lambda: d["focus"]["xp_price_idea"])
    put("pp_price", lambda: d["focus"]["pp_price"])
    put("air_fighters", lambda: d["air"]["fighters"])
    put("air_cas", lambda: d["air"]["cas"])
    put("supply_per_province", lambda: d["supply"]["per_province_mean"])
    put("fuel_days_carried", lambda: d["fuel"]["days_carried"])
    put("fuel_target", lambda: d["fuel"]["target"])
    put("training_at_once", lambda: d["training"]["in_training"])
    return s


def lines(data, args_text):
    cmd = "python scripts/hoi4facts/evaluate.py"
    base = f"--country {data['tag']} --date {data['date']}" + (f" {args_text}" if args_text else "")
    en = " ".join(f"--enemy {t}" for t in data["enemies"])
    out = [f"# Computed decisions for {data['tag']} at {data['date']}",
           "",
           f"Every line is computed from the game files by the evaluators (tag: computed); rerun with the command "
           f"shown. Enemies: {data['enemies']} ({data['enemies_how']}); war date {data['war_date']}; goals "
           f"{data['goals'] or '-'}.",
           ""]
    if data.get("references"):
        out.append(f"Country knowledge exists ({', '.join(data['references'])}): follow it where it is sourced, and "
                   f"where it differs from a computed line below, say which you take and why.")
    else:
        out.append("No playbook reference for this country: these computed answers are the plan's defaults.")
    out.append("")
    c = data.get("construction")
    if c:
        out.append(f"- [C] computed construction: k = {c['k']} military factories per civilian factory (one shared "
                   f"bonus: {c['k_single_bonus']}); {c.get('decision', 'no war date')} ({cmd} construction {base} "
                   f"--war-date {data['war_date']})")
    r = data.get("research")
    if r:
        firsts = [sl[0]["tech"] for sl in r["picks"] if sl]
        out.append(f"- [C] computed research: first picks per slot {firsts}; then "
                   f"{[p['tech'] for sl in r['picks'] for p in sl[1:]]} ({cmd} research {base} {en})")
    for role, t in (data.get("template") or {}).items():
        if t and t.get("templates"):
            x = t["templates"][0]
            out.append(f"- [C] computed template {role}: width {x['width']:g}, {x['battalions']}, regimental "
                       f"{x['regimental']}, support {x['support']}, {x['ic']:g} IC ({cmd} template {role} {base} {en})")
    d = data.get("design")
    if d and d.get("designs"):
        x = d["designs"][0]
        out.append(f"- [C] computed design tank_main: {x['base']} {sorted(x['modules'].values())} upgrades "
                   f"{x['upgrades']} ({cmd} design tank_main {base} {en})")
    f = data.get("force")
    if f and not f.get("error"):
        out.append(f"- [C] computed force against {f['enemy']['tag']}: hold {f['hold']}, attack {f['attack']}, total "
                   f"{f['total']}, missing {f['missing']} ({cmd} force {base} {en} --war-date {data['war_date']})")
    dc = data.get("doctrine")
    if dc:
        best = max(dc["rows"], key=lambda x: (x.get("line") or {}).get("all milestones", 0), default=None)
        if best:
            out.append(f"- [C] computed doctrine: {best['doctrine']} lifts the line most ({best.get('line')}); held "
                       f"{dc['held']} ({cmd} doctrine {base} {en})")
    p = data.get("production")
    if p:
        ls = [(x["variant"], x["factories"]) for x in p["lines"] if x.get("factories")]
        out.append(f"- [C] computed production to {p['need_date']}: lines {ls}; trains {p['logistics']['trains']} "
                   f"({cmd} production {base} {en} --need-date {data['war_date']})")
    fo = data.get("focus")
    if fo:
        out.append(f"- [C] computed focus order: {[o['focus'] for o in fo['order']]} ({cmd} focus {base} "
                   + " ".join(f"--goal {g}" for g in data["goals"]) + ")")
    w = f"--country {data['tag']} --date {data['war_date']} --bookmark {data['date']}"
    sd = data.get("subdoctrine")
    if sd:
        out.append(f"- [C] computed subdoctrines (grand {sd['grand']}): "
                   + ", ".join(f"{t['track']} {t['chosen']}" for t in sd["tracks"])
                   + f" ({cmd} subdoctrine {w} {en})")
    sp = data.get("supply")
    if sp and not sp.get("error"):
        out.append(f"- [C] computed supply against {data['enemies'][0]}: {sp['per_province_mean']} a border province "
                   f"(mean over {len(sp['axes'])} axes) ({cmd} supply {w} {en})")
    a = data.get("air")
    if a:
        out.append(f"- [C] computed air need: {a['fighters']} fighters, {a['cas']} close air support "
                   f"({cmd} air {w} {en} --war-date {data['war_date']})")
    fu = data.get("fuel")
    if fu:
        out.append(f"- [C] computed fuel: capacity {fu['capacity']:,}, stock at war {fu['stock_at_war']:,}, "
                   f"{fu['per_war_day']:,} a war day, carries {fu['days_carried']} days ({cmd} fuel {w} {en})")
    tr = data.get("training")
    if tr:
        out.append(f"- [C] computed training: {tr['in_training']} line divisions in training at once ({tr['binding']}) "
                   f"({cmd} training line {base} {en} --need-date {data['war_date']})")
    fo = data.get("focus") or {}
    if fo.get("xp_price_idea"):
        out.append(f"- [C] computed army experience source: {fo['xp_price_idea']} ({fo['xp_price']} factory-years a "
                   f"year per point); PP price {fo.get('pp_price')}")
    if data.get("claims"):
        out += ["", "## The guide against the computed answer", ""]
        for row in data["claims"]:
            out.append(f"- the guide says {row['guide']}; computed says {row['computed']}; difference "
                       f"{row['difference']} ({row['what']}; {row['where']})")
    for k, v in data["errors"].items():
        if not k.endswith("_trace"):
            out.append(f"- evaluator {k} failed: {v} (plan this area from the playbook and the facts)")
    return out


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("--run", required=True)
    ap.add_argument("--game", default=None)
    ap.add_argument("--mod", action="append", default=[])
    ap.add_argument("--userdir", default=None)
    ap.add_argument("--dlc", default=None)
    a, _ = ap.parse_known_args(argv)
    spec = json.load(open(os.path.join(a.run, "run_spec.json"), encoding="utf-8"))
    from hoi4facts import mapgraph
    from hoi4facts.content import mod_from_arg
    game_dir = a.game or os.environ.get("HOI4_DIR") or mapgraph.DEFAULT_GAME_DIR
    mods = [m for m in (mod_from_arg(x, a.userdir) for x in a.mod) if m]
    dlcs = [x for x in a.dlc.split(";") if x] if a.dlc else None
    data = run(spec, game_dir, mods, dlcs)
    claims_path = os.path.join(a.run, "guide-claims.json")
    if os.path.exists(claims_path):
        data["claims"] = compare(json.load(open(claims_path, encoding="utf-8")), data)
    with open(os.path.join(a.run, "computed.json"), "w", encoding="utf-8") as fh:
        json.dump(data, fh, indent=1, default=lambda o: sorted(o) if isinstance(o, set) else str(o))
    with open(os.path.join(a.run, "computed.md"), "w", encoding="utf-8") as fh:
        fh.write("\n".join(lines(data, "")) + "\n")
    print(os.path.join(a.run, "computed.md"))
    return 0


if __name__ == "__main__":
    sys.exit(main())
