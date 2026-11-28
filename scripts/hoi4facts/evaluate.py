#!/usr/bin/env python3
"""Evaluators on the command line: the answer and its reasoning, computed from the game's files (and a live snapshot).

    evaluate.py design ROLE --country TAG [--date D] [--enemy TAG ...] [--family medium_tank_chassis]
    evaluate.py template ROLE --country TAG [--date D] [--enemy TAG] [--width N]
    evaluate.py research --country TAG [--date D] [--slots N]
    evaluate.py production --country TAG [--date D] [--need-date D]
    evaluate.py training ROLE|TEMPLATE --country TAG [--count N] [--need-date D]   (divisions in training at once;
                     live snapshot keys: stockpile, production_output {equipment: units a day}, manpower,
                     field_manpower, training_manpower)
    evaluate.py construction --country TAG [--date D] [--war-date D]
    evaluate.py force --country TAG --enemy TAG [--date D]
    evaluate.py doctrine --country TAG [--date D]
    evaluate.py subdoctrine --country TAG --enemy TAG [--reach 1]  (one subdoctrine per track by value)
    evaluate.py focus --country TAG [--date D] [--goal ideology:fascism] [--goal no_war:FRA]
    evaluate.py refresh TECH --country TAG          (the design roles a finished technology re-opens)
    evaluate.py variant --country TAG [--date D] [--equipment KEY]   (production-cost clicks on mass lines)
    evaluate.py convert FAMILY --country TAG --date D --to-date D     (base-and-convert: a base now, the design later)

Output: text (default), --json (the whole answer), --lines (one JSON object per decision: evaluators/lines.py, the
plan generator's and the companion's shape), --jsonl PATH or --run RUN (append those lines to PATH or
RUN/evaluations.jsonl as well as printing).

Common options: --game DIR (default HOI4_DIR), --mod PATH (repeatable; a .mod descriptor or a mod folder, layered over
the install like the launcher), --mods none|auto (auto: also the mods enabled in the user folder), --state JSON (a live
snapshot, see evaluators/context.py), --bookmark D (history applied up to this date; default the date), --json.
Stdlib only, Python 3.9+.
"""
import argparse
import json
import os
import sys

if __package__ in (None, ""):
    sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
    __package__ = "hoi4facts"  # noqa: A001

from hoi4facts import mapgraph  # noqa: E402
from hoi4facts.content import default_userdir, mod_from_arg, mods_from_userdir  # noqa: E402
from hoi4facts.evaluators import context  # noqa: E402
from hoi4facts.game import Game  # noqa: E402


def situation(a):
    game_dir = a.game or os.environ.get("HOI4_DIR") or mapgraph.DEFAULT_GAME_DIR
    userdir = getattr(a, "userdir", None) or (default_userdir() if a.mods == "auto" else None)
    mods = mods_from_userdir(userdir) if (userdir and a.mods == "auto") else []
    for m in a.mod or []:
        mod = mod_from_arg(m, userdir)
        if mod is None:
            raise SystemExit(f"--mod {m}: not a .mod descriptor or a mod folder")
        mods.append(mod)
    state = None
    if a.state:
        with open(a.state, encoding="utf-8") as fh:
            state = json.load(fh)
    dlcs = [x.strip() for x in a.dlc.split(";") if x.strip()] if a.dlc else None
    return context.Situation.load(Game(game_dir, mods), a.country, a.date, dlcs=dlcs, state=state,
                                  bookmark=a.bookmark, assume=a.assume)


def _fmt_stats(st, keys=None):
    keys = keys or list(st)
    return ", ".join(f"{k} {st[k]:g}" for k in keys if k in st)


# ---------------------------------------------------------------- design

def text_design(r):
    out = []
    if "speed_floor" in r:
        e = r["enemy"]
        out.append(f"DESIGN {r['role']} (design type {r['designs'][0]['type'] if r['designs'] else '-'}) "
                   f"for {r.get('country', '')} at {r.get('date', '')}; techs: {r['techs']}")
        out.append(f"  enemy ({', '.join(e['enemies'])}; {e['chosen']}): best armour {e['best_armor']:g}, piercing "
                   f"{e['piercing']:g} (division {e['division_piercing']:g}, vehicles {e['best_vehicle_piercing']:g}), "
                   f"line hardness {e['hardness']:g}, tank hardness {e['tank_hardness']:g}")
        for x in e["vehicles"][:3]:
            out.append(f"    their {x['design']} ({x['tag']}, {x['base']}): armour {x['armor']:g}, piercing "
                       f"{x['piercing']:g}")
        out.append(f"  speed floor {r['speed_floor']:g} km/h: {r['speed_floor_from']}")
        out.append(f"  combat rule from: {'; '.join(r['combat_defines'])}")
        for i, d in enumerate(r["designs"], 1):
            out.append(f"  #{i} {d['base']} score {d['score']:g} (value {d['value']:g} / sqrt IC)")
            out.append("     modules: " + ", ".join(f"{k.replace('_slot', '')}={m}" for k, m in sorted(d["modules"].items())))
            out.append("     upgrades: " + ", ".join(f"{k} {n}" for k, n in sorted(d["upgrades"].items())))
            out.append("     stats: " + _fmt_stats(d["stats"]))
            p = d["parts"]
            out.append(f"     why: effective attack {p['effective_attack']:g} (piercing factor {p['pierce_out']:g}, armour "
                       f"dice x{p['armour_dice']:g}), effective breakthrough {p['effective_breakthrough']:g} (their "
                       f"piercing factor on us {p['pierce_in']:g})")
        if len(r["designs"]) > 1:
            a, b = r["designs"][0], r["designs"][1]
            diff = {k: round(a["stats"].get(k, 0) - b["stats"].get(k, 0), 2) for k in a["stats"]
                    if abs(a["stats"].get(k, 0) - b["stats"].get(k, 0)) > 1e-6}
            out.append(f"  trade-off #1 vs #2: {_fmt_stats(diff)}")
        if r["rejected_by"]:
            out.append(f"  rejected: {r['rejected_by']}")
    elif "rival" in r:
        out.append(f"DESIGN {r['role']} (planes, {r['method']}); techs: {r['techs']}")
        if r["rival"]:
            out.append(f"  rival: {r['rival']['design']} ({r['rival']['tag']}): "
                       + _fmt_stats(r["rival"]["stats"], ("air_attack", "air_defence", "air_agility", "maximum_speed")))
        for i, d in enumerate(r["designs"], 1):
            out.append(f"  #{i} {d['base']} score {d['score']:g} exchange {d.get('exchange')}: "
                       + ", ".join(f"{k}={m}" for k, m in sorted(d["modules"].items())))
            out.append("     stats: " + _fmt_stats(d["stats"], ("air_attack", "air_defence", "air_agility",
                                                                "maximum_speed", "air_range", "air_ground_attack",
                                                                "reliability", "build_cost_ic")))
    else:
        out.append(f"DESIGN {r['role']} (ships): {r['note'] if not r['designs'] else ''}")
        for i, d in enumerate(r["designs"], 1):
            out.append(f"  #{i} {d['base']} score {d['score']:g}: " + ", ".join(f"{k}={m}" for k, m in
                                                                             sorted(d["modules"].items())))
            out.append("     stats: " + _fmt_stats(d["stats"]))
        if r.get("blocked_hulls"):
            out.append(f"  hulls without a usable module (DLC): {', '.join(r['blocked_hulls'][:8])}")
    return out


def cmd_design(a, sit):
    from hoi4facts.evaluators import design
    kw = {"enemies": a.enemy or None}
    if a.family:
        kw["families"] = set(a.family)
    if a.role in design.SHIP_ROLES:
        kw = {}
    r = design.best(sit, a.role, **kw)
    r["country"], r["date"] = sit.tag, sit.date
    return r, text_design(r)


def cmd_refresh(a, sit):
    from hoi4facts.evaluators import design
    roles = design.refresh_roles(sit, a.tech)
    return {"tech": a.tech, "roles": roles}, [f"REFRESH after {a.tech}: re-run design for {', '.join(roles) or 'none'}"]


def text_template(r):
    out = [f"TEMPLATE {r['role']} for {r.get('country', '')} at {r.get('date', '')}; techs: {r['techs']}",
           f"  widths: {r['width_why']}", f"  chosen width {r.get('chosen_width')}: the widest of the equal fills "
           f"({r.get('command_rule')}: frontage per commander grows with width)",
           f"  against: {r['enemy_template']}: " + _fmt_stats(r["enemy_template_stats"])]
    g = r.get("axis")
    if g:
        out.append(f"  axis: {g['why']}; terrain {g['shares']}; battle width {g['battle_width']}, movement cost "
                   f"{g['movement_cost']}, {g['km']} km between provinces; supply delivered {g['supply']:g} "
                   f"({g['supply_from']})")
    for arch, d in (r.get("designs_used") or {}).items():
        sp = f" ({d['speed']:g} km/h, {d['ic']:g} IC, floor {d['floor']:g})" if d.get("speed") and d.get("floor") else ""
        out.append(f"  design for {arch}{sp}: {d['base']} " + ", ".join(sorted(d["modules"].values())))
    for i, t in enumerate(r["templates"], 1):
        bats = ", ".join(f"{n} {k}" for k, n in t["battalions"].items())
        out.append(f"  #{i} width {t['width']:g}: {bats}; regimental {t['regimental'] or '-'} "
                   f"({t.get('regimental_slots', 0)} slots); support {t['support'] or '-'}")
        out.append("     stats: " + _fmt_stats(t["stats"]))
        out.append(f"     cost: {t['ic']:g} IC, {t['manpower']:g} manpower; equipment " +
                   ", ".join(f"{k} {v:g}" for k, v in t["equipment"].items()))
        p = t["parts"]
        lg = t.get("logistics") or {}
        if lg:
            out.append(f"     logistics: speed {lg.get('speed', 0):g} km/h, supply {lg.get('supply', 0):g} a day, fuel "
                       f"{lg.get('fuel', 0):g} (equipment fuel_consumption x amount; the game's time unit for it is not verified)")
        if "provinces_per_day" in p:
            out.append(f"     why: {p['provinces_per_day']:g} provinces a day = 24 / (battle {p['battle_hours']:g} h + march "
                       f"{p['march_hours']:g} h at {p['speed']:g} km/h); R {p['R']:g}; supply ratio {p['supply_ratio']:g} "
                       f"({p['divisions_per_battle']:g} divisions x {p['supply_use']:g} against {p['axis_supply']:g}); "
                       f"exploitation {p['exploit_km_per_day']:g} km a day; score {t['score']:g} = provinces a day x "
                       f"sqrt(width / IC)" + (f"; {p['note']}" if p.get("note") else ""))
        elif "R" in p:
            out.append(f"     why: R {p['R']:g} = our endurance {p['our_hours']:g} h / theirs {p['their_hours']:g} h "
                       f"(attacks per width ours {p['our_attacks_per_width']:g}, theirs {p['their_attacks_per_width']:g});"
                       f" score {t['score']:g} = R x sqrt(width / IC)")
        else:
            out.append(f"     why: {p}; score {t['score']:g}")
    for t in r.get("other_widths") or []:
        out.append(f"  best at width {t['target_width']}: " + ", ".join(f"{n} {k}" for k, n in t["battalions"].items())
                   + f"; support {t['support']}; score {t['score']:g}")
    if r.get("special_forces_cap"):
        c = r["special_forces_cap"]
        out.append(f"  special forces cap {c['cap']:g} battalions = max({c['min']:g}, {c['factor']:g} x "
                   f"{c['non_sf_battalions']} other battalions)")
    out.append("  rules: " + "; ".join(r["rules"] + list(r.get("logistic_rules") or [])))
    return out


def cmd_template(a, sit):
    from hoi4facts.evaluators import template
    r = template.best(sit, a.role, width=a.width, enemies=a.enemy or None)
    r["country"], r["date"] = sit.tag, sit.date
    return r, text_template(r)


def text_construction(r):
    out = [f"CONSTRUCTION for {r.get('country', '')} at {r.get('date', '')}: {r['factories']['civilian']:g} civilian, "
           f"{r['factories']['military']:g} military, {r['factories']['naval']:g} naval ({r['factories']['source']})"]
    out.append(f"  buildings: civilian = {r['civilian_building']} ({r['cost_civ']:g}), military = "
               f"{r['military_building']} ({r['cost_mil']:g}) from common/buildings")
    for name, total, terms in (("civilian", r["b_civ"], r["terms_civ"]), ("military", r["b_mil"], r["terms_mil"]),
                               ("consumer goods", r["consumer_goods"], r["terms_cg"])):
        out.append(f"  {name} {total:+g}: " + ("; ".join(f"{k} {v:+g} ({src})" for src, k, v in terms) or "none"))
    out.append(f"  k = ({r['cost_civ']:g}/(1{r['b_civ']:+g})) / ({r['cost_mil']:g}/(1{r['b_mil']:+g})) = {r['k']:g} "
               f"military factories per civilian factory (one shared bonus would give {r['k_single_bonus']:g})")
    out.append(f"  P0 = {r['cost_mil']:g} / ({r['cp_per_civ']:g} x (1{r['b_mil']:+g}) x (1 + 0.2 x {r['infrastructure']:g}"
               f" infrastructure)) = {r['P0_days']:g} days per military factory from one civilian factory")
    out.append(f"  a civilian factory pays when more than {r['standing_days']:g} days remain (factories standing) or "
               f"{r['output_days']:g} days (output to the horizon)")
    if r.get("decision"):
        out.append(f"  decision for war on {r['war_date']}: {r['decision']}")
    for g in r["gates"][:6]:
        out.append(f"  gate: {g['focus']} needs {g['needs']} (a separate purchase)")
    out.append("  build in: " + "; ".join(f"{w['name']} ({w['free_slots']} free, infrastructure {w['infrastructure']})"
                                          for w in r["where"]))
    if r.get("conversion"):
        out.append(f"  conversion to {r['conversion']['building']}: {r['conversion']['conversion_cost']:g}")
    out.append("  constants: " + "; ".join(f"{k} {v['v']:g} ({v['src']})" for k, v in r["constants"].items()
                                          if isinstance(v, dict) and isinstance(v.get("v"), (int, float))))
    return out


def cmd_construction(a, sit):
    from hoi4facts.evaluators import construction
    r = construction.evaluate(sit, a.war_date, a.war_years)
    r["country"], r["date"] = sit.tag, sit.date
    return r, text_construction(r)


def text_research(r):
    m = r["model"]
    out = [f"RESEARCH for {r.get('country', '')} at {r.get('date', '')}: {r['slots']} slots, research speed "
           f"{m['research_speed']:+g}; techs: {r['techs']}",
           f"  value base: {m['base']}; IC per factory-year {m['ic_per_factory_year']:g}; a civilian factory builds "
           f"{m['build_rate']:g} military factories a year ({m['build_rate_why']}); horizon {m['horizon_years']:g} years",
           f"  army: {m['army_templates']} fielded templates, {m['army_ic']:g} IC, against {m['enemy_template']}; "
           f"built later: {m.get('future_template') or 'the fielded templates'}; military factories grow "
           f"{m.get('military_growth_per_year', 0):g} a year (consumer goods {m.get('consumer_goods', 0):g})",
           f"  days: {r['method']}; " + "; ".join(m["defines"])]
    for i, sl in enumerate(r["picks"], 1):
        out.append(f"  slot {i}:")
        for p in sl:
            extra = f"; bonus {p['bonus']}" if p.get("bonus") else ""
            out.append(f"    day {p['start_day']:>4} {p['tech']} ({p['days']} days): value {p['value']:g} factory-years,"
                       f" chain rate {p['rate']:g}/day via {' > '.join(p['chain'])}; parts {p['parts']}{extra}")
            if p.get("equipment") or p.get("unvalued"):
                out.append(f"         equipment {p['equipment'] or '-'}; unvalued {p['unvalued'] or '-'}")
            out.append(f"         next best then: {p['next_best']}")
    for b in r["bonuses"]:
        out.append(f"  bonus {b['bonus']}: {b['advice']}")
    return out


def cmd_research(a, sit):
    from hoi4facts.evaluators import research
    r = research.evaluate(sit, slots=a.slots, picks=a.picks, horizon_years=a.horizon_years, enemies=a.enemy or None)
    r["country"], r["date"] = sit.tag, sit.date
    return r, text_research(r)


def text_air(r):
    t = r["terms"]
    return [f"AIR need for {r.get('country', '')} at {r.get('date', '')} against {', '.join(r['enemies'])}: "
            f"{r['fighters']} fighters, {r['cas']} close air support (at most {r['cas_cap']} join; "
            f"{r['cas_requires']})",
            f"  fighters: {r['why']['fighters']}",
            f"  cas: {r['why']['cas']}",
            f"  our fighter: {t['our_design']}; their best: {t['their_airframe']}; {t['src']}"]


def cmd_air(a, sit):
    from hoi4facts.evaluators import air
    r = air.need(sit, a.enemy or None, axes=a.axes, fighter_ratio=a.fighter_ratio)
    r["country"], r["date"] = sit.tag, sit.date
    return r, text_air(r)


def text_variant(r):
    out = [f"VARIANT cost clicks for {r.get('country', '')} at {r.get('date', '')}"]
    if not r["clicks"]:
        out.append("  no buildable equipment with a production-cost upgrade")
    for x in r["clicks"]:
        out.append(f"  {x['equipment']} ({x['upgrade']}): take {x['chosen']}: {x['why']}")
        for row in x["levels"]:
            out.append(f"    {row['level']} clicks: {row['ic']:g} IC, reliability {row['reliability']:g}, {row['xp']:g} XP,"
                       f" {row['units_per_100_ic']:g} per 100 IC, value per IC {row['value_per_ic']:g}")
        out.append("    rules: " + "; ".join(x["rules"]))
    return out


def cmd_variant(a, sit):
    from hoi4facts.evaluators import variant
    r = {"clicks": variant.cost_clicks(sit, a.equipment), "country": sit.tag, "date": sit.date}
    return r, text_variant(r)


def text_convert(r):
    if r.get("note") and not r.get("base"):
        return [f"CONVERT {r.get('family')}: {r['note']}"]
    b, f = r["base"], r["final"]
    out = [f"CONVERT {r['family']} for {r.get('country', '')}: base at {r['date']}, design at {r['to_date']} "
           f"({r['techs_then']})",
           f"  final ({f['stats'].get('build_cost_ic', 0):g} IC): {f['base']} " + ", ".join(sorted(f["modules"].values()))
           + f"; upgrades {f['upgrades']}",
           f"  base  ({b['stats'].get('build_cost_ic', 0):g} IC): {b['base']} " + ", ".join(sorted(b["modules"].values()))
           + f"; upgrades {b['upgrades']}" + (f"; NOT LEGAL: {b['legal']}" if b["legal"] else "")]
    for row in r["conversion_rows"]:
        out.append(f"    {row['part']}: {row['ic']:g} IC" + (f", {row['xp']:g} XP" if row.get("xp") else "")
                   + (f" ({row.get('how') or row.get('rule')})" if row.get("how") or row.get("rule") else ""))
    out.append(f"  conversion {r['conversion_ic']:g} IC and {r['conversion_xp']:g} XP per vehicle: {r['why']}")
    return out


def cmd_convert(a, sit):
    from hoi4facts.evaluators import variant
    r = variant.base_and_convert(sit, a.family, a.to_date, role=a.role, enemies=a.enemy or None)
    r["country"] = sit.tag
    return r, text_convert(r)


def text_production(r):
    e = r["efficiency"]
    out = [f"PRODUCTION for {r.get('country', '')} at {r.get('date', '')} to {r['need_date']} ({r['days']} days): "
           f"{r['military_factories']:g} military factories; stockpile: {r['stockpile']}",
           f"  efficiency {e['start']:g}% -> {e['max']:g}% in {e['days_to_max']} days, output x{e['output']:g}; a line "
           f"change keeps {e['change']} (%); " + "; ".join(e["defines"])]
    for d in r["divisions"]:
        out.append(f"  divisions: {d['divisions']} x {d['template']} ({d['source']})")
    for ln in r["lines"]:
        if not ln["shortfall"]:
            continue
        out.append(f"  line {ln['variant']}: need {ln['need']:g}, have {ln['have']:g}, short {ln['shortfall']:g} x "
                   f"{ln['unit_ic']:g} IC = {ln['ic']:g} IC; one factory makes {ln['ic_per_factory']:g} IC by then -> "
                   f"{ln['factories']} factories ({ln['start']})" + (f"; {ln['late']}" if ln.get("late") else ""))
    out.append(f"  unassigned military factories: {r['unassigned']:g}")
    lg = r["logistics"]
    out.append(f"  trains {lg['trains']}: {lg['trains_why']}")
    out.append(f"  trucks: {lg['trucks_divisional']:g} in the templates; {lg['trucks_per_motorised_hub']:g} per hub "
               f"motorised; convoys {lg['convoys']:g} (overseas supply and trade, live)")
    out.append("  logistics defines: " + "; ".join(lg["defines"]))
    return out


def text_training(r):
    if r.get("error"):
        return [f"TRAINING {r['role']}: {r['error']}"]
    out = [f"TRAINING {r['template']} for {r.get('country', '')} at {r.get('date', '')}: {r['training_days']:g} days "
           f"({r['training_days_why']})",
           f"  IN TRAINING AT ONCE: {r['in_training']} (one deployment line of {r['in_training']}); binding: "
           f"{', '.join(r['binding'])}"]
    for k, v in r["terms"].items():
        out.append(f"  {k}: {v} = {r['why'][k]}")
    if r.get("fielded_by_need_date") is not None:
        out.append(f"  fielded by {r['need_date']}: {r['fielded_by_need_date']}" +
                   (f" of {r['count']}" if r.get("count") else ""))
    out.append(f"  output: {r['output_from']}")
    if r.get("note"):
        out.append(f"  {r['note']}")
    out.append("  method: " + r["method"])
    return out


def cmd_training(a, sit):
    from hoi4facts.evaluators import production
    r = production.training(sit, a.role, count=a.count, need_date=a.need_date, enemies=a.enemy or None)
    r["country"], r["date"] = sit.tag, sit.date
    return r, text_training(r)


def cmd_production(a, sit):
    from hoi4facts.evaluators import production
    add = {}
    for x in a.add or []:
        role, _, n = x.partition("=")
        add[role] = int(n or 1)
    r = production.evaluate(sit, a.need_date, add=add, enemies=a.enemy or None)
    r["country"], r["date"] = sit.tag, sit.date
    return r, text_production(r)


def text_force(r):
    e = r["enemy"]
    out = [f"FORCE against {e['tag']} for {r.get('country', '')} at {r.get('date', '')}: {e['divisions']} enemy divisions "
           f"({e['how']})"]
    if r.get("error"):
        return out + [f"  {r['error']}"]
    out += [f"  front: {r['front_provinces']} provinces, terrain {r['terrain']}, battle width {r['battle_width']}; "
            f"enemy template {r['enemy_template']}",
            f"  hold: their width per province {r['their_width_per_province']:g} / R_def {r['R_def']:g} -> "
            f"{r['hold_per_province']} x {r['line_template']} (width {r['line_width']:g}) per province = {r['hold']}",
            f"  attack: {r['axes']} axes x {r['wave']} x {r['attack_template']} (width {r['attack_width']:g}, R_att "
            f"{r['R_att']:g}) x (1 + {r['refills']} refills of their reserve) = {r['attack']}",
            f"  total {r['total']} divisions; we field {r['ours']}; missing {r['missing']}"]
    return out


def cmd_force(a, sit):
    from hoi4facts.evaluators import force
    if not a.enemy:
        raise SystemExit("force: --enemy TAG is required")
    r = force.needed(sit, a.enemy[0], a.war_date, axes=a.axes)
    r["country"], r["date"] = sit.tag, sit.date
    return r, text_force(r)


def text_doctrine(r):
    out = [f"DOCTRINE (land grand doctrines) for {r.get('country', '')} at {r.get('date', '')}: held {r['held']}; "
           f"R against {r['enemy_template']} for our line and armour templates (base / with every milestone)"]
    for x in r["rows"]:
        out.append(f"  {x['doctrine']}{' (held)' if x['held'] else ''}: line {x.get('line')}, attacker "
                   f"{x.get('attacker')}, armour {x.get('armour')}; planning {x['planning']} + milestones "
                   f"{x['planning_milestones']}; other {x['other']} ({x['src']})")
        for role, by in (x.get("attack") or {}).items():
            out.append(f"      {role} attacking with planning (mean over the first T days): " + "; ".join(
                f"{label}: " + ", ".join(f"{t} R {v['R']:g} (planning {v['planning']:g})" for t, v in d.items())
                for label, d in by.items()))
        out.append(f"      not valued: {x.get('unvalued', {}).get('all milestones')}")
    out.append(f"  planning: {'; '.join(r.get('planning_defines') or [])}; subdoctrine tracks are the same under "
               "every grand doctrine (not compared)")
    return out


def text_subdoctrine(r):
    out = [f"SUBDOCTRINES ({len(r['tracks'])} land tracks) for {r.get('country', '')} at {r.get('date', '')}: grand "
           f"{r['grand'] or '-'}; R against {r['enemy_template']} without a subdoctrine {r['none']}; chosen by the "
           f"activation + the first {r['reach']} reward(s) (each needs {r['reward_mastery']:g} mastery), attacks with "
           f"planning over {r['planning_days']} days"]
    for t in r["tracks"]:
        out.append(f"  track {t['track']}: CHOSEN {t['chosen']}" + (f" ({t['xor_note']})" if t.get("xor_note") else ""))
        for c in t["candidates"][:4]:
            res = c["results"]
            out.append(f"     {c['subdoctrine']}: value {c['value']:+g} (base {res['base']['value']:+g}, every reward "
                       f"{res['all']['value']:+g}); R {res['reach']['R']}" + (f"; gate {c['gate']}" if c["gate"] else "")
                       + (f"; tactic {c['tactic']}" if c.get("tactic") else "")
                       + (f"; not valued {c['not_valued'][:4]}" if c["not_valued"] else ""))
    out.append("  defines: " + "; ".join(r["defines"]))
    return out


def cmd_subdoctrine(a, sit):
    from hoi4facts.evaluators import force
    r = force.subdoctrines(sit, a.enemy or None, planning_days=a.planning_days, reach=a.reach, grand=a.grand)
    r["country"], r["date"] = sit.tag, sit.date
    return r, text_subdoctrine(r)


def cmd_doctrine(a, sit):
    from hoi4facts.evaluators import force
    r = force.doctrines(sit, a.enemy or None, planning_days=tuple(a.planning_days))
    r["country"], r["date"] = sit.tag, sit.date
    return r, text_doctrine(r)


def text_focus(r):
    out = [f"FOCUS for {r.get('country', '')} at {r.get('date', '')} (tree {r['tree']}); goals {r['goals'] or '-'}; "
           f"horizon {r['horizon_years']:g} years; value base {r['base']}",
           f"  prices: political power {r.get('pp_price', 0):g} factory-years a year held per point ({r.get('pp_price_idea')}); "
           f"army experience {r.get('xp_price', 0):g} per point ({r.get('xp_price_idea')})"]
    for o in r["order"]:
        out.append(f"  day {o['start_day']:>4} {o['focus']} ({o['days']:g} days): value {o['value']:g} factory-years, "
                   f"chain rate {o['rate']:g}/day via {' > '.join(o['chain'])}")
        out.append(f"         parts {o['parts']}; unvalued {o['unvalued'] or '-'}"
                   + (f"; gated: {o['gated'][:120]}" if o.get("gated") else ""))
    for fid, why in list(r["dropped"].items())[:8]:
        out.append(f"  dropped {fid}: {why}")
    return out


def cmd_focus(a, sit):
    from hoi4facts.evaluators import focus
    r = focus.evaluate(sit, goals=a.goal, horizon_years=a.horizon_years, count=a.count)
    r["country"], r["date"] = sit.tag, sit.date
    return r, text_focus(r)


COMMANDS = {"force": cmd_force, "doctrine": cmd_doctrine, "subdoctrine": cmd_subdoctrine, "production": cmd_production, "research": cmd_research, "design": cmd_design, "refresh": cmd_refresh, "template": cmd_template, "construction": cmd_construction, "variant": cmd_variant, "convert": cmd_convert, "focus": cmd_focus, "training": cmd_training, "air": cmd_air}


def parser():
    ap = argparse.ArgumentParser(prog="evaluate.py", description=__doc__.split("\n")[0])
    sub = ap.add_subparsers(dest="cmd", required=True)

    def add(name, *pos):
        p = sub.add_parser(name)
        for x in pos:
            p.add_argument(x)
        p.add_argument("--country", required=True, help="our country tag")
        p.add_argument("--date", default="1936.1.1")
        p.add_argument("--bookmark", default=None, help="history applied up to this date (default: --date)")
        p.add_argument("--assume", choices=("auto", "start", "on-time"), default="auto",
                       help="techs after the bookmark: on-time research (auto: when --date is past the bookmark year)")
        p.add_argument("--game", default=None)
        p.add_argument("--mod", action="append", default=[])
        p.add_argument("--mods", choices=("auto", "none"), default="none",
                       help="auto: also the mods enabled in the user folder")
        p.add_argument("--userdir", default=None)
        p.add_argument("--dlc", default=None, help="active DLC names separated by ';' (default: the installed)")
        p.add_argument("--state", default=None, help="live snapshot JSON")
        p.add_argument("--enemy", action="append", default=[])
        p.add_argument("--json", action="store_true")
        p.add_argument("--lines", action="store_true", help="one JSON object per decision (evaluators/lines.py)")
        p.add_argument("--jsonl", default=None, help="also append the decision lines to this file")
        p.add_argument("--run", default=None, help="also append the decision lines to RUN/evaluations.jsonl")
        return p

    p = add("design", "role")
    p.add_argument("--family", action="append", default=[], help="only these chassis families")
    add("refresh", "tech")
    p = add("template", "role")
    p.add_argument("--width", type=int, default=None, help="a width target instead of the theatre's terrain")
    p = add("force")
    p.add_argument("--war-date", default=None, help="the enemy grows by its factories to this date")
    p.add_argument("--axes", type=int, default=None, help="attack axes (default: the enemy's border states)")
    p = add("subdoctrine")
    p.add_argument("--planning-days", type=int, default=14, help="offensive length the planning bonus is averaged over")
    p.add_argument("--reach", type=int, default=1, help="rewards counted in the choice (each needs the mastery)")
    p.add_argument("--grand", default=None, help="the grand doctrine assumed (default: the one held)")
    p = add("doctrine")
    p.add_argument("--planning-days", type=int, nargs="+", default=[14, 30],
                   help="offensive lengths the planning bonus is averaged over")
    p = add("focus")
    p.add_argument("--goal", action="append", default=[], help="ideology:<name> or no_war:<TAG> (repeatable)")
    p.add_argument("--count", type=int, default=12)
    p.add_argument("--horizon-years", type=float, default=4.0)
    p = add("production")
    p.add_argument("--need-date", required=True, help="the date the equipment must be in the stockpile")
    p = add("training", "role")
    p.add_argument("--count", type=int, default=None, help="divisions of it still wanted by --need-date")
    p.add_argument("--need-date", default=None)
    p.add_argument("--add", action="append", default=[], help="ROLE=N divisions to add (templates from evaluator 2)")
    p = add("research")
    p.add_argument("--slots", type=int, default=None)
    p.add_argument("--picks", type=int, default=3, help="picks per slot")
    p.add_argument("--horizon-years", type=float, default=4.0, help="years the value is counted for")
    p = add("air")
    p.add_argument("--axes", type=int, default=1, help="attack axes on the front (force sizing gives them)")
    p.add_argument("--fighter-ratio", type=float, default=None,
                   help="the plan's air_situation.fighter_ratio (default: parity)")
    p = add("construction")
    p.add_argument("--war-date", default=None, help="the planned war date (the 'standing' horizon)")
    p.add_argument("--war-years", type=float, default=2.0, help="the war's planned length (the 'output' horizon)")
    p = add("variant")
    p.add_argument("--equipment", default=None, help="one equipment (default: the newest of every archetype held)")
    p = add("convert", "family")
    p.add_argument("--to-date", required=True, help="the date of the final design (techs by on-time research)")
    p.add_argument("--role", default="tank_main")
    return ap


def main(argv=None):
    argv = list(sys.argv[1:] if argv is None else argv)
    a = parser().parse_args(argv)
    sit = situation(a)
    data, text = COMMANDS[a.cmd](a, sit)
    if a.lines or a.jsonl or a.run:
        from hoi4facts.evaluators import lines
        rows = lines.lines(a.cmd, data, argv, sit)
        for path in [x for x in (a.jsonl, os.path.join(a.run, "evaluations.jsonl") if a.run else None) if x]:
            lines.write(path, rows)
    if a.lines:
        for row in rows:
            print(json.dumps(row, ensure_ascii=False, default=lambda o: sorted(o) if isinstance(o, set) else str(o)))
    elif a.json:
        print(json.dumps(data, indent=1, ensure_ascii=False, default=lambda o: sorted(o) if isinstance(o, set) else str(o)))
    else:
        print("\n".join(text))
    return 0


if __name__ == "__main__":
    sys.exit(main())
