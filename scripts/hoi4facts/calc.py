#!/usr/bin/env python3
"""Calculators for the strategist: construction compounding, template equipment, political power.

    calc.py build-order [--tag T --date D] [--civ N --mil N] [--cg 0.25] --order civ:10,mil:5,infrastructure:2
                        [--until D] [--infra L | --states 161,162] [--bonus 0.1]
                        (any building: naval_base:2@1 = two levels on a level-1 port, rail_way:1x12 = one
                        level on 12 provinces, supply_node:1; #STATE puts an item in a state and takes that
                        state's infrastructure, e.g. supply_node:1#550; each row prints its cost, days and
                        the infrastructure it was built at, and a project whose infrastructure had to be
                        guessed is listed under WARNING)
    calc.py equipment   (--tag T --template NAME | --regiments infantry=6,artillery=1 --support engineer)
                        [--regimental field_guns] [--divisions N] [--year Y]
                        (equipment, manpower, IC, width, divisions per terrain width, designer problems)
    calc.py pp          [--tag T --date D] [--start PP] [--gain PP_PER_DAY] [--factor F] [--stability S]
                        [--difficulty normal] [--cost PP_PER_DAY] [--no-focus | --focus-until D]
                        --until D [--buy 1936.2.1:150:free_trade ...]
                        (net income: a running focus costs its political_power_cost, 1 a day in vanilla)
    calc.py xp          [--tag T --date D] [--branch army|air|navy] --until D [--start-xp N]
                        [--reward 1936.3.12:25:army_primacy ...] [--war 1936.1.1:1936.12.22 ...]
                        [--training-days N] [--combat-per-day XP] [--cost 100 ...]
                        (the country experience pool month by month: floor = start + dated rewards + daily
                        spirits + training, ceiling = floor + the most combat XP a day x days at war, and
                        the estimate with --combat-per-day; the day each --cost is first affordable)

Every constant is read from the effective game files where it exists (common/defines, common/buildings,
common/units, common/ideas) and printed with its source. Constants the files do not hold are labelled
"not in defines" with where the number comes from:

    construction points per civilian factory per day   5      not in defines (wiki; economy-build-orders.md 1.1)
    state infrastructure speed multiplier              1 + 0.2 x level   not in defines (wiki; same section)
    factories per construction project                 15     the defines have no construction key; the same
                                                              number as NMarket.MAX_CIV_FACTORIES_PER_CONTRACT
                                                              (economy-build-orders.md section 0, user-verified)
    political power per day                            NPolitics.BASE_POLITICAL_POWER_INCREASE (the comment
                                                              says weekly; the game shows it per day), less
                                                              the static modifier gain_focus political_power_cost
                                                              while a focus runs (common/modifiers)

Global options as facts.py: --game, --userdir, --mod, --dlc, --json. Stdlib only, Python 3.9+.
"""
import argparse
import json
import math
import os
import re
import sys

if __package__ in (None, ""):
    sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
    __package__ = "hoi4facts"  # noqa: A001

from hoi4facts import derive  # noqa: E402
from hoi4facts import facts as factsmod  # noqa: E402
from hoi4facts.script import get1, parse_file  # noqa: E402
from hoi4facts.world import day_number, num  # noqa: E402

CP_PER_CIV = 5.0
INFRA_STEP = 0.2
PROJECT_CAP_FALLBACK = 15
KIND = {"civ": "industrial_complex", "mil": "arms_factory", "dock": "dockyard", "infra": "infrastructure",
        "infrastructure": "infrastructure", "refinery": "synthetic_refinery"}


def constants(w):
    """The construction constants with their sources."""
    cap, cap_f = w.define("MAX_CIV_FACTORIES_PER_CONTRACT", PROJECT_CAP_FALLBACK)
    return {
        "cp_per_civ": {"v": CP_PER_CIV, "src": "not in defines (wiki; economy-build-orders.md 1.1)"},
        "infra_step": {"v": INFRA_STEP, "src": "not in defines (wiki; economy-build-orders.md 1.1): x(1 + 0.2 x level)"},
        "project_cap": {"v": int(cap), "src": (cap_f or {}).get("src"),
                        "note": "no construction key in defines; NMarket.MAX_CIV_FACTORIES_PER_CONTRACT has the same "
                                "15 (economy-build-orders.md section 0)"},
    }


# ---------------------------------------------------------------- build order

def consumer_goods(w, tag):
    """Consumer-goods share of factories from the tag's starting ideas (consumer_goods_expected_value,
    else consumer_goods_factor added to it)."""
    mods = derive.idea_modifiers(w.game)
    share, srcs = 0.0, []
    for f in w.country(tag)["ideas"]:
        m = mods.get(f["v"], {})
        if "consumer_goods_expected_value" in m:
            share += m["consumer_goods_expected_value"]
            srcs.append(f"{f['v']} {m['consumer_goods_expected_value']}")
    for f in w.country(tag)["ideas"]:
        m = mods.get(f["v"], {})
        if "consumer_goods_factor" in m:
            share += m["consumer_goods_factor"] * share
            srcs.append(f"{f['v']} factor {m['consumer_goods_factor']}")
    return round(share, 4), srcs


def parse_order(text, w=None):
    """"civ:10,mil:5,naval_base:2@1,rail_way:1x12,supply_node:1#550" -> build items. KIND:N is N buildings or
    N levels; @L starts a levelled building (one with per_level_extra_cost, e.g. naval_base, rail_way) at
    level L (default 0); xP multiplies the cost by P provinces (a railway line); #S builds in state S, at
    that state's infrastructure. Plain kinds stay strings; other items are (kind, level already built or
    None, provinces) with the state as a fourth member when one is named."""
    out = []
    btypes = w.building_types() if w is not None else {}
    for part in (text or "").split(","):
        part = part.strip()
        if not part:
            continue
        kind, _, rest = part.partition(":")
        kind, _, tail = kind.partition("#")  # "supply_node#550" (no count) reads as one building
        kind = kind.strip()
        m = re.match(r"^(\d*)(?:@(\d+))?(?:x(\d+))?(?:#(\d+))?$", rest.strip() + ("#" + tail if tail else ""))
        if not m:
            raise ValueError(f"bad order item {part!r} (KIND:N[@LEVEL][xPROVINCES][#STATE])")
        n = int(m.group(1) or 1)
        start = m.group(2)
        provs = int(m.group(3) or 1)
        state = int(m.group(4)) if m.group(4) else None
        b = KIND.get(kind, kind)
        levelled = (btypes.get(b) or {}).get("per_level_cost") is not None
        if start is None and not levelled and provs == 1 and state is None:
            out += [kind] * n
            continue
        lvl = int(start or 0)
        for i in range(n):
            item = (kind, lvl + i if levelled or start is not None else None, provs)
            out.append(item + (state,) if state is not None else item)
    return out


def item_cost(w, item, costs=None):
    """(building, cost, level after the build or None, provinces) of a build item (see parse_order):
    base_cost + per_level_extra_cost x levels already built, times provinces (common/buildings)."""
    kind, level, provs = (item, None, 1) if isinstance(item, str) else item[:3]
    b = KIND.get(kind, kind)
    bt = w.building_types().get(b) or {}
    base = (costs or {}).get(b) if (costs or {}).get(b) is not None else bt.get("cost")
    if base is None:
        raise ValueError(f"unknown building {kind!r}")
    if (costs or {}).get(b) is None and level:
        base += (bt.get("per_level_cost") or 0) * level
    return b, base * provs, (level + 1 if level is not None else None), provs


def build_order(w, order, civ, mil, cg, start, until=None, slots=None, infra=None, bonus=0.0, costs=None,
                slots_named=True):
    """Simulate construction day by day. `order` is a list of build items (parse_order: plain kinds, or
    tuples that may name a state), `slots` a list of [state, free slots, infrastructure] taken in order
    (None: one state with unlimited slots and `infra`); `slots_named` False says the caller ranked the
    states itself (the user named none). Each project gets at most the project cap of the free civs.

    Infrastructure of a project, in this order: the state its item names; the state whose slot it takes; the
    first state of `slots` for a building that takes no slot (a guess when the states were not named);
    `infra`; else 0, a guess. An infrastructure level finished in a state speeds up the projects after it
    there. Every guess is listed in "warnings" and marks its rows `infra_src` "assumed".
    Returns {"rows": [...], "end": {...}, "warnings": [...], "constants": {...}}."""
    k = constants(w)
    cap = k["project_cap"]["v"]
    btypes = w.building_types()
    kinds = derive.building_kinds(w.game)
    costs = costs or {}
    queue = []
    slots = [list(s) for s in slots] if slots else None
    levels = {s[0]: s[2] for s in slots or []}  # state -> infrastructure now (raised by finished levels)
    free = {s[0]: s[1] for s in slots or []}
    guessed = []
    for i, item in enumerate(order):
        b, cost, level, provs = item_cost(w, item, costs)
        kind = item if isinstance(item, str) else item[0]
        named = item[3] if not isinstance(item, str) and len(item) > 3 else None
        state, lvl, src = None, infra if infra is not None else 0, "--infra" if infra is not None else "assumed"
        extra = {"level": level} if level is not None else {}
        if provs > 1:
            extra["provinces"] = provs
        shares = bool((btypes.get(b) or {}).get("shares_slots"))
        blocked = None
        if named is not None:
            row = w.states().get(named)
            if row is None:
                raise ValueError(f"unknown state {named} in order item {kind}#{named}")
            if named not in levels:
                levels[named] = row["infrastructure"]
                free[named] = row["free_slots"] or 0
            state, src = named, "state"
            if shares:
                if free[named] > 0:
                    free[named] -= 1
                else:
                    blocked = "no free slot"
        elif slots and shares:
            pick = next((s for s in slots if free[s[0]] > 0), None)
            if pick is None:
                blocked = "no free slot"
            else:
                free[pick[0]] -= 1
                state, src = pick[0], "state"
        elif slots:
            state, src = slots[0][0], "state" if slots_named else "assumed"
        q = dict(extra, kind=kind, building=b, cost=cost, state=state, infra=lvl, infra_src=src, progress=0.0)
        if blocked:
            q["blocked"] = blocked
        elif src == "assumed":
            guessed.append(q)
        queue.append(q)
    warnings = []
    if guessed:
        nowhere = [q["kind"] for q in guessed if q["state"] is None]
        ranked = [q for q in guessed if q["state"] is not None]
        step = f"each infrastructure level adds {INFRA_STEP * 100:g}% construction speed"
        if nowhere:
            warnings.append(f"infrastructure 0 assumed for {len(nowhere)} project(s) with no state "
                            f"({', '.join(sorted(set(nowhere)))}): {step}, so the dates are the latest possible; "
                            f"name the state (KIND:N#STATE or --states) or pass --infra")
        if ranked:
            warnings.append(f"{', '.join(sorted({q['kind'] for q in ranked}))} take no building slot and no state was "
                            f"named: the infrastructure of state {ranked[0]['state']} (the first of the ranking, "
                            f"level {levels[ranked[0]['state']]}) was used, which is not where they are built; name "
                            f"the state (KIND:N#STATE)")
    infra_max = (btypes.get("infrastructure") or {}).get("state_max")
    today = day_number(start)
    stop = day_number(until) if until else today + 365 * 10
    rows = []
    civs, mils = civ, mil
    while queue and today < stop:
        total = civs + mils
        free_civs = max(0, civs - int(math.ceil(cg * total)))
        active = [q for q in queue if not q.get("blocked")][: max(1, int(math.ceil(free_civs / cap))) if free_civs else 0]
        left = free_civs
        done = []
        for q in active:
            n = min(cap, left)
            left -= n
            if n <= 0:
                break
            lvl = levels[q["state"]] if q["state"] in levels else q["infra"]
            speed = n * CP_PER_CIV * (1 + INFRA_STEP * lvl) * (1 + bonus)
            q.setdefault("begin", today)
            q.setdefault("infra_begin", lvl)
            q["progress"] += speed
            if q["progress"] >= q["cost"]:
                done.append(q)
        today += 1
        for q in done:
            queue.remove(q)
            if q["building"] == "industrial_complex":
                civs += 1
            elif q["building"] == "arms_factory":
                mils += 1
            elif q["building"] in kinds["infrastructure"] and q["state"] in levels:
                levels[q["state"]] = min(levels[q["state"]] + 1, infra_max or levels[q["state"]] + 1)
            row = {"date": _fmt(today), "kind": q["kind"], "state": q["state"], "civ": civs, "mil": mils,
                   "free_civs": max(0, civs - int(math.ceil(cg * (civs + mils)))), "cost": q["cost"],
                   "days": today - q["begin"], "infra": q["infra_begin"], "infra_src": q["infra_src"]}
            for key in ("level", "provinces"):
                if key in q:
                    row[key] = q[key]
            rows.append(row)
        if not active and not done:
            break
    return {"rows": rows, "end": {"date": _fmt(today), "civ": civs, "mil": mils},
            "unfinished": [{"kind": q["kind"], "progress": round(q["progress"]), "cost": q["cost"],
                            **({"blocked": q["blocked"]} if q.get("blocked") else {})} for q in queue],
            "warnings": warnings, "constants": k, "consumer_goods": cg,
            "d": "daily: free civs = civs - ceil(cg x (civs + mils)); each project <= cap civs; progress = civs x "
                 "cp_per_civ x (1 + infra_step x infrastructure) x (1 + bonus); a finished civ joins the next day; "
                 "cost = base_cost + per_level_extra_cost x levels already built, x provinces (common/buildings [?]); "
                 "days = first day of progress to the finish; infrastructure = the named or slot state's level "
                 "(a level finished there counts for later projects), else --infra"}


def _fmt(n):
    from hoi4facts.world import from_day
    return from_day(n)


# ---------------------------------------------------------------- equipment

def sub_units(w):
    def load():
        out = {}
        for p in w.game.files("common/units"):
            for k, v in parse_file(p):
                if k != "sub_units" or not isinstance(v, list):
                    continue
                for name, body in v:
                    if not name or not isinstance(body, list):
                        continue
                    need = get1(body, "need")
                    out[name] = {"need": {a: num(b, 0) for a, b in need} if isinstance(need, list) else {},
                                 "manpower": num(get1(body, "manpower"), 0), "src": w.src(p)}
        return out

    return w.cached("calc:subunits", load)


def equipment_costs(w, year):
    """{archetype: (build_cost_ic of the newest variant with year <= year, variant)}."""

    def load():
        rows = {}
        for p in w.game.files("common/units/equipment"):
            for k, v in parse_file(p):
                if k != "equipments" or not isinstance(v, list):
                    continue
                for name, body in v:
                    if not name or not isinstance(body, list):
                        continue
                    rows[name] = {"archetype": get1(body, "archetype"), "parent": get1(body, "parent"),
                                  "year": num(get1(body, "year"), 0), "cost": num(get1(body, "build_cost_ic")),
                                  "is_archetype": get1(body, "is_archetype") == "yes"}
        return rows

    rows = w.cached("calc:equipment", load)

    def cost(name, seen=()):
        r = rows.get(name)
        if not r or name in seen:
            return None
        if r["cost"] is not None:
            return r["cost"]
        return cost(r["parent"] or r["archetype"], seen + (name,)) if (r["parent"] or r["archetype"]) else None

    out = {}
    for name, r in rows.items():
        arch = name if r["is_archetype"] else r["archetype"]
        if not arch or r["is_archetype"] or (r["year"] or 0) > year:
            continue
        c = cost(name)
        if c is None:
            continue
        if arch not in out or (r["year"] or 0) >= out[arch][2]:
            out[arch] = (c, name, r["year"] or 0)
    return {a: (c, n) for a, (c, n, _) in out.items()}


TEMPLATE_DEFINES = ("MAX_DIVISION_BRIGADE_WIDTH", "MAX_DIVISION_BRIGADE_HEIGHT", "MAX_DIVISION_SUPPORT_WIDTH",
                    "MAX_DIVISION_SUPPORT_HEIGHT", "MAX_REGIMENTAL_SUPPORT_WIDTH", "MAX_REGIMENTAL_SUPPORT_HEIGHT",
                    "REGIMENTAL_SUPPORT_REQUIRED_BATTALIONS")
WIDTH_FIT_METHOD = ("division width = sum of combat_width x count over battalions and supports (common/units); per "
                    "terrain and attack directions: divisions = floor(battle width / division width), fill and "
                    "stacking as theatre.WIDTH_METHOD")


def template_checks(w, regiments, support, regimental):
    """Problems of a template against the designer defines and the sub-unit rows (1.19 regimental supports:
    a sub-unit of row `regimental support`, its allowed_battalion_groups meeting a battalion group of the
    template, at most MAX_REGIMENTAL_SUPPORT_WIDTH x HEIGHT, each needing a column of
    REGIMENTAL_SUPPORT_REQUIRED_BATTALIONS battalions; columns are not known from counts [?])."""
    units = w.sub_units()
    lim = {}
    for key in TEMPLATE_DEFINES:
        v, f = w.define(key)
        if f:
            lim[key] = {"v": v[0] if isinstance(v, list) and v else v, "src": f["src"]}
    val = lambda key, d: (lim.get(key) or {}).get("v", d)  # noqa: E731
    out = []
    bats = sum(regiments.values())
    if bats > val("MAX_DIVISION_BRIGADE_WIDTH", 5) * val("MAX_DIVISION_BRIGADE_HEIGHT", 5):
        out.append(f"{bats} battalions > the designer grid")
    if len(support) > val("MAX_DIVISION_SUPPORT_WIDTH", 1) * val("MAX_DIVISION_SUPPORT_HEIGHT", 5):
        out.append(f"{len(support)} support companies > the support column")
    groups = {(units.get(r) or {}).get("group") for r in regiments}
    for s in support:
        row = (units.get(s) or {}).get("row")
        if row == "regimental support":
            out.append(f"{s} is a regimental support (divisional = no): put it in the regimental row")
        elif row not in ("support", "divisional support", None):
            out.append(f"{s} is a {row}, not a support company")
    for s in regimental:
        u = units.get(s) or {}
        if u.get("row") != "regimental support":
            out.append(f"{s} is not a regimental support (row {u.get('row')})")
        elif u.get("allowed_battalion_groups") and not (set(u["allowed_battalion_groups"]) & groups):
            out.append(f"{s} needs a battalion of group {'/'.join(u['allowed_battalion_groups'])}")
    cap = val("MAX_REGIMENTAL_SUPPORT_WIDTH", 5) * val("MAX_REGIMENTAL_SUPPORT_HEIGHT", 1)
    if len(regimental) > cap:
        out.append(f"{len(regimental)} regimental supports > {cap}")
    need = val("REGIMENTAL_SUPPORT_REQUIRED_BATTALIONS", 3)
    if regimental and bats < need * len(regimental):
        out.append(f"{len(regimental)} regimental supports need {need} battalions in each one's column")
    return out, lim


def template_width(w, regiments, support=(), regimental=()):
    units = w.sub_units()
    return sum((units.get(n) or {}).get("combat_width", 0) * c for n, c in regiments.items()) + \
        sum((units.get(s) or {}).get("combat_width", 0) for s in list(support) + list(regimental))


def width_fit(w, width):
    """{terrain: {directions: {battle_width, divisions, fill}}} of a division width (WIDTH_FIT_METHOD)."""
    from hoi4facts import theatre
    stack = theatre._stacking(w)
    out = {}
    for t, row in sorted(w.terrains().items()):
        bw = theatre.battle_widths(row)
        if not bw or not width:
            continue
        out[t] = {k: dict(zip(("fill", "divisions"), theatre.fill(width, W, k, stack)), battle_width=W)
                  for k, W in bw.items()}
    return out


def equipment(w, regiments, support, divisions=1, year=1936, regimental=()):
    units = w.sub_units() or sub_units(w)
    costs = equipment_costs(w, year)
    need, manpower, missing = {}, 0, []
    for name, n in list(regiments.items()) + [(s, 1) for s in list(support) + list(regimental)]:
        u = units.get(name)
        if not u:
            missing.append(name)
            continue
        manpower += u["manpower"] * n
        for eq, amount in u["need"].items():
            need[eq] = need.get(eq, 0) + amount * n
    rows = []
    for eq, amount in sorted(need.items()):
        c = costs.get(eq)
        rows.append({"equipment": eq, "per_division": amount, "total": amount * divisions,
                     "variant": c[1] if c else None, "ic_each": c[0] if c else None,
                     "ic_total": round(c[0] * amount * divisions, 1) if c else None})
    width = template_width(w, regiments, support, regimental)
    problems, lim = template_checks(w, regiments, list(support), list(regimental))
    return {"rows": rows, "manpower_per_division": manpower, "manpower_total": manpower * divisions,
            "divisions": divisions, "missing_sub_units": missing,
            "ic_total": round(sum(r["ic_total"] or 0 for r in rows), 1),
            "width": width, "width_fit": width_fit(w, width), "template_problems": problems, "designer": lim,
            "regimental_support": list(regimental),
            "d": "need of each sub-unit (common/units) x count x divisions; IC = build_cost_ic of the newest variant "
                 f"with year <= {year} (common/units/equipment), no production bonuses; " + WIDTH_FIT_METHOD}


# ---------------------------------------------------------------- political power

PP_METHOD = ("per day = (BASE_POLITICAL_POWER_INCREASE + gain) x (1 + factor + stability factor + difficulty factor) "
             "- daily costs - the running focus's political_power_cost (static modifier gain_focus) while a focus "
             "runs; stability above 50% scales stability_good_modifier's political_power_factor up to its full value "
             "at 100%, below 50% stability_bad_modifier's down to its full value at 0%; a purchase happens on its "
             "date or the first day the balance covers it")


def pp_rates(w, gain=0.0, factor=0.0, stability=None, difficulty=None, cost=0.0, focus_cost=None):
    """The PP income terms with their sources: {"base", "gain", "factor", "stability_factor",
    "difficulty_factor", "cost", "focus_cost", "per_day_focus", "per_day_idle"}. focus_cost None reads
    gain_focus political_power_cost from common/modifiers (0 when the files have none)."""
    from hoi4facts import scenario
    base, base_f = w.define("BASE_POLITICAL_POWER_INCREASE", 2)
    mods = scenario.static_modifiers(w)
    if focus_cost is None:
        focus_cost = float((mods.get("gain_focus") or {}).get("political_power_cost") or 0.0)
    stab = 0.0
    if stability is not None:
        s = stability / 100.0 if stability > 1 else float(stability)
        good = float((mods.get("stability_good_modifier") or {}).get("political_power_factor") or 0.0)
        bad = float((mods.get("stability_bad_modifier") or {}).get("political_power_factor") or 0.0)
        stab = good * max(0.0, (s - 0.5) / 0.5) + bad * max(0.0, (0.5 - s) / 0.5)
    diff = 0.0
    if difficulty:
        diff = float((mods.get(f"diff_{difficulty}_player") or {}).get("political_power_factor") or 0.0)
    gross = (base + gain) * (1 + factor + stab + diff)
    return {"base": dict(base_f or {}, v=base, note="the define comment says weekly; the game adds it daily"),
            "gain": gain, "factor": factor, "stability": stability, "stability_factor": round(stab, 4),
            "difficulty": difficulty or "normal", "difficulty_factor": diff, "cost": cost, "focus_cost": focus_cost,
            "per_day_focus": round(gross - cost - focus_cost, 4), "per_day_idle": round(gross - cost, 4)}


def pp(w, start_pp, gain, factor, start, until, buys, stability=None, difficulty=None, cost=0.0, focus_cost=None,
       focus_until=None):
    """Daily PP balance from start to until; purchases [(date, cost, label)] wait until affordable.
    A focus runs the whole time unless focus_until (a date) ends it earlier; focus_cost 0 leaves the
    focus cost out (no focus running)."""
    rates = pp_rates(w, gain, factor, stability, difficulty, cost, focus_cost)
    balance = float(start_pp)
    today, stop = day_number(start), day_number(until)
    focus_end = day_number(focus_until) if focus_until else stop
    pending = sorted(((day_number(d), c, label) for d, c, label in buys), key=lambda x: x[0])
    rows = []
    while today <= stop:
        while pending and pending[0][0] <= today and balance >= pending[0][1]:
            when, price, label = pending.pop(0)
            balance -= price
            rows.append({"date": _fmt(today), "buy": label, "cost": price, "planned": _fmt(when),
                         "late_days": today - when, "balance": round(balance, 1)})
        today += 1
        if today <= stop:
            balance += rates["per_day_focus"] if today <= focus_end else rates["per_day_idle"]
    return {"per_day": rates["per_day_focus"], "per_day_without_focus": rates["per_day_idle"],
            "base": rates["base"], "gain": gain, "factor": factor, "stability_factor": rates["stability_factor"],
            "difficulty": rates["difficulty"], "difficulty_factor": rates["difficulty_factor"], "cost": cost,
            "focus_cost": rates["focus_cost"], "focus_until": _fmt(focus_end), "rows": rows,
            "end": {"date": _fmt(stop), "balance": round(balance, 1)},
            "not_bought": [{"planned": _fmt(d), "cost": c, "buy": l} for d, c, l in pending], "d": PP_METHOD}


XP_METHOD = ("country experience pool (not a unit's veterancy: the UNIT_EXPERIENCE_* defines never feed it); "
             "floor = start + dated rewards + daily experience_gain spirits + training days x training_max_per_day; "
             "ceiling = floor + combat_max_per_day x days at war; estimate = floor + min(combat_max_per_day, "
             "the given combat XP a day) x days at war (the daily combat gain is the planner's assumption: the "
             "game scales the losses of the country's battles by combat_scale, up to combat_max_per_day); every "
             "line stops at the pool cap; defines read by full key (NMilitary / NAir / NNavy), mods included")


def xp(w, branch, start, until, tag=None, start_xp=None, rewards=(), war=(), training_days=0, combat_per_day=None,
       costs=()):
    """Forecast of one country experience pool (army, air or navy) from start to until.

    rewards: [(date, xp, label)] (focus, decision or event rewards); war: [(from date, to date)] the days the
    country fights; training_days: days of exercises from the start; combat_per_day: the assumed combat XP a
    day at war (capped by the define), None for no estimate line; costs: XP prices whose first affordable day
    is reported for the floor, the estimate and the ceiling. Raises ValueError for an unknown branch or when
    the game files hold no combat define for it."""
    tag = tag or w.tag
    info = w.experience(tag)
    if branch not in info["start"]:
        raise ValueError(f"branch {branch!r}: give army, air or navy")
    gain = info["gain"].get(branch) or {}
    if "combat_max_per_day" not in gain:
        raise ValueError(f"the defines hold no FIELD_EXPERIENCE_MAX_PER_DAY for the {branch} pool")
    cap_day = float(gain["combat_max_per_day"]["v"])
    train_day = float(gain["training_max_per_day"]["v"]) if "training_max_per_day" in gain else 0.0
    pool_cap = float(info["max"][branch]["v"]) if branch in info["max"] else math.inf
    spirits = sum(m["value"] for m in info["spirits"] if m["branch"] == branch and m["daily"])
    base = float(info["start"][branch]["v"] if start_xp is None else start_xp)
    today, stop = day_number(start), day_number(until)
    first = today
    wars = [(day_number(a), day_number(b)) for a, b in war]
    pending = sorted(((day_number(d), float(n), label) for d, n, label in rewards), key=lambda r: r[0])
    est_day = None if combat_per_day is None else min(cap_day, max(0.0, float(combat_per_day)))
    lines = {"floor": base, "ceiling": base}
    if est_day is not None:
        lines["estimate"] = base
    reach = {float(c): {k: None for k in lines} for c in costs}
    rows, war_days = [], 0
    while today <= stop:
        got = 0.0
        while pending and pending[0][0] <= today:
            got += pending.pop(0)[1]
        if today > first:
            got += spirits + (train_day if today - first <= training_days else 0.0)
        fighting = today > first and any(a < today <= b for a, b in wars)
        war_days += fighting
        for k in lines:
            extra = (cap_day if k == "ceiling" else est_day if k == "estimate" else 0.0) if fighting else 0.0
            lines[k] = min(pool_cap, lines[k] + got + extra)
            for c, hit in reach.items():
                if hit[k] is None and lines[k] >= c:
                    hit[k] = _fmt(today)
        y, m, d = (int(x) for x in _fmt(today + 1).split(".")[:3])
        if d == 1 or today == stop:
            rows.append(dict({"date": _fmt(today)}, **{k: round(v, 1) for k, v in lines.items()}))
        today += 1
    return {"tag": tag, "branch": branch, "start": {"date": start, "xp": base, "src": info["start"][branch].get("src")},
            "defines": gain, "cap": info["max"].get(branch), "daily_spirits": spirits, "training_days": training_days,
            "war_days": war_days, "combat_per_day": est_day, "rows": rows,
            "end": dict({"date": _fmt(stop)}, **{k: round(v, 1) for k, v in lines.items()}),
            "reach": [dict({"cost": c}, **hit) for c, hit in sorted(reach.items())],
            "not_counted": [{"date": _fmt(d), "xp": n, "reward": label} for d, n, label in pending], "d": XP_METHOD}


def pp_modifiers(w, tag):
    """(flat gain, factor) from the tag's starting ideas (political_power_gain, political_power_factor)."""
    mods = derive.idea_modifiers(w.game)
    gain = factor = 0.0
    for f in w.country(tag)["ideas"]:
        m = mods.get(f["v"], {})
        gain += m.get("political_power_gain", 0.0)
        factor += m.get("political_power_factor", 0.0)
    return gain, factor


def pp_costs(w, tag):
    """Daily political_power_cost of the tag's starting ideas."""
    mods = derive.idea_modifiers(w.game)
    return sum(mods.get(f["v"], {}).get("political_power_cost", 0.0) for f in w.country(tag)["ideas"])


def start_stability(w, tag):
    """The tag's starting stability (history set_stability), else 0.5 (the game's default)."""
    f = w.country(tag).get("stability") if tag else None
    return f["v"] if f and isinstance(f.get("v"), (int, float)) else 0.5


# ---------------------------------------------------------------- CLI

def _common(p):
    for flag in ("--game", "--userdir", "--dlc", "--tag", "--date", "--out"):
        p.add_argument(flag, default=None)
    p.add_argument("--mod", action="append")
    p.add_argument("--mode", default=None)
    p.add_argument("--json", action="store_true")


def main(argv=None):
    ap = argparse.ArgumentParser(prog="calc.py", description=__doc__.split("\n")[0])
    sub = ap.add_subparsers(dest="cmd", required=True)
    b = sub.add_parser("build-order")
    _common(b)
    b.add_argument("--civ", type=int)
    b.add_argument("--mil", type=int)
    b.add_argument("--cg", type=float, help="consumer goods share (default: the tag's laws)")
    b.add_argument("--order", required=True,
                   help="civ:10,mil:5,infra:2,naval_base:2@1,supply_node:1#550,rail_way:1x12 (kinds: civ mil dock "
                        "infra or any building of common/buildings; @L = levels already built, xP = provinces, "
                        "#S = built in state S at its infrastructure)")
    b.add_argument("--until")
    b.add_argument("--infra", type=int, help="infrastructure of every project without a state (no state slots)")
    b.add_argument("--states", help="state ids in build order (default with --tag: the dossier ranking); a "
                                    "building that takes no slot goes to the first one unless its item names a state")
    b.add_argument("--bonus", type=float, default=0.0, help="construction speed bonus, 0.1 = +10%%")
    e = sub.add_parser("equipment")
    _common(e)
    e.add_argument("--template", help="division template name in the tag's start OOB")
    e.add_argument("--regiments", help="infantry=6,artillery_brigade=1")
    e.add_argument("--support", help="engineer,artillery")
    e.add_argument("--regimental", help="1.19 regimental supports: field_guns,anti_tank_battery")
    e.add_argument("--divisions", type=int, default=1)
    e.add_argument("--year", type=int)
    q = sub.add_parser("pp")
    _common(q)
    q.add_argument("--start", type=float, help="PP at the start (default: the history's add_political_power, else 0)")
    q.add_argument("--gain", type=float, help="extra PP per day (default: starting ideas)")
    q.add_argument("--factor", type=float, help="political_power_factor (default: starting ideas)")
    q.add_argument("--until", required=True)
    q.add_argument("--buy", action="append", default=[], help="DATE:COST:LABEL (repeat)")
    q.add_argument("--stability", type=float, help="stability (0.6 or 60; default: the tag's start, else 0.5)")
    q.add_argument("--difficulty", default=None, help="very_easy, easy, normal, hard, very_hard (default normal)")
    q.add_argument("--cost", type=float, help="daily PP costs (default: starting ideas' political_power_cost)")
    q.add_argument("--no-focus", action="store_true", help="no national focus runs (no focus cost)")
    q.add_argument("--focus-until", help="date the last planned focus ends (focus cost until then)")
    x = sub.add_parser("xp")
    _common(x)
    x.add_argument("--branch", default="army", choices=("army", "air", "navy"))
    x.add_argument("--until", required=True)
    x.add_argument("--start-xp", type=float, help="XP at the start (default: the history's, else 0)")
    x.add_argument("--reward", action="append", default=[], help="DATE:XP[:LABEL] (repeat): focus, decision, event")
    x.add_argument("--war", action="append", default=[], help="FROM:TO dates at war (repeat)")
    x.add_argument("--training-days", type=int, default=0, help="days of exercises from the start")
    x.add_argument("--combat-per-day", type=float, help="assumed combat XP a day at war (capped by the define)")
    x.add_argument("--cost", action="append", type=float, default=[], help="XP price to date (repeat)")
    args = ap.parse_args(argv)
    w = factsmod.world(args, args.tag)
    tag = w.tag
    start = w.date

    if args.cmd == "xp":
        try:
            rewards = []
            for item in args.reward:
                d, n, label = (item.split(":", 2) + [""])[:3]
                rewards.append((d, float(n), label or f"+{n}"))
            wars = [tuple(item.split(":", 1)) for item in args.war]
            if any(len(pair) != 2 for pair in wars):
                raise ValueError("--war wants FROM:TO")
            r = xp(w, args.branch, start, args.until, tag, args.start_xp, rewards, wars, args.training_days,
                   args.combat_per_day, args.cost)
        except ValueError as e:
            print(json.dumps({"error": str(e)}))
            return 1
        if args.json:
            print(json.dumps(r, indent=1))
            return 0
        print(f"{r['tag'] or '-'} {r['branch']} XP from {r['start']['xp']:g} at {start}; "
              + "; ".join(f"{name} {f['v']:g} [{f['key']} {f['src']}]" for name, f in r["defines"].items())
              + (f"; cap {r['cap']['v']:g} [{r['cap']['src']}]" if r["cap"] else ""))
        names = [k for k in ("floor", "estimate", "ceiling") if k in r["end"]]
        print(f"{'date':>10} " + " ".join(f"{k:>8}" for k in names))
        for row in r["rows"]:
            print(f"{row['date']:>10} " + " ".join(f"{row[k]:>8g}" for k in names))
        for hit in r["reach"]:
            print(f"{hit['cost']:g} XP reached: " + ", ".join(f"{k} {hit[k] or 'not by ' + r['end']['date']}"
                                                              for k in names))
        for nc in r["not_counted"]:
            print(f"NOT COUNTED (after {r['end']['date']}): {nc}")
        print(f"{r['war_days']} days at war. {r['d']}")
        return 0

    if args.cmd == "build-order":
        civ, mil = args.civ, args.mil
        if tag and (civ is None or mil is None):
            rows = [w.states()[s] for s in w.owned_states(tag)]
            civ = sum(r["buildings"].get("industrial_complex", 0) for r in rows) if civ is None else civ
            mil = sum(r["buildings"].get("arms_factory", 0) for r in rows) if mil is None else mil
        if civ is None or mil is None:
            print(json.dumps({"error": "give --civ and --mil, or --tag"}))
            return 1
        cg, cg_src = (args.cg, ["--cg"]) if args.cg is not None else (consumer_goods(w, tag) if tag else (0.0, []))
        slots, named = None, True
        if args.states:
            st = w.states()
            slots = [[int(s), st[int(s)]["free_slots"] or 0, st[int(s)]["infrastructure"]] for s in args.states.split(",")]
        elif tag and args.infra is None:
            from hoi4facts import dossier as dossiermod
            slots = [[r["id"], r["free_slots"] or 0, r["infrastructure"]] for r in dossiermod.state_rows(w, tag)
                     if r["core"]]
            named = False
        try:
            r = build_order(w, parse_order(args.order, w), civ, mil, cg, start, args.until, slots, args.infra,
                            args.bonus, slots_named=named)
        except ValueError as e:
            print(json.dumps({"error": str(e)}))
            return 1
        r["consumer_goods_src"] = cg_src
        r["start"] = {"date": start, "civ": civ, "mil": mil}
        if args.json:
            print(json.dumps(r, indent=1))
            return 0
        for msg in r["warnings"]:
            print(f"WARNING: {msg}")
        print(f"start {start}: {civ} civ, {mil} mil, consumer goods {cg} ({', '.join(cg_src) or 'none'})")
        for k, v in r["constants"].items():
            print(f"  {k} = {v['v']} [{v['src']}]")
        for row in r["rows"]:
            what = row["kind"] + (f" L{row['level']}" if row.get("level") else "") + \
                (f" x{row['provinces']} prov" if row.get("provinces") else "")
            infra_txt = f"infra {row['infra']}" + (" ASSUMED" if row["infra_src"] == "assumed" else "")
            print(f"{row['date']:>10} {what:<14} cost {row['cost']:g} in {row['days']}d state {row['state']} "
                  f"{infra_txt} -> civ {row['civ']} mil {row['mil']} (free civs {row['free_civs']})")
        for u in r["unfinished"]:
            print(f"unfinished {u}")
        print(f"end {r['end']['date']}: civ {r['end']['civ']} mil {r['end']['mil']}. {r['d']}")
        return 0

    if args.cmd == "equipment":
        regs, sup, rsup = {}, [], []
        if args.template:
            o = w.oob(tag)
            t = o["templates"].get(args.template)
            if not t:
                print(json.dumps({"error": f"no template {args.template!r} in {tag}'s start OOB",
                                  "templates": sorted(x for x in o["templates"] if x)}))
                return 1
            regs, sup, rsup = dict(t["regiments"]), list(t["support"]), list(t.get("regimental_support") or [])
        if args.regiments:
            for part in args.regiments.split(","):
                n, _, c = part.partition("=")
                regs[n.strip()] = int(c or 1)
        if args.support:
            sup += [s.strip() for s in args.support.split(",") if s.strip()]
        if args.regimental:
            rsup += [s.strip() for s in args.regimental.split(",") if s.strip()]
        year = args.year or int(start.split(".")[0])
        r = equipment(w, regs, sup, args.divisions, year, rsup)
        if args.json:
            print(json.dumps(r, indent=1))
            return 0
        for row in r["rows"]:
            print(f"{row['equipment']}: {row['per_division']:g}/div x{r['divisions']} = {row['total']:g}"
                  + (f" ({row['variant']} {row['ic_each']} IC -> {row['ic_total']} IC)" if row["ic_each"] else ""))
        print(f"manpower {r['manpower_per_division']}/div, {r['manpower_total']} total; IC {r['ic_total']}"
              + (f"; unknown sub-units {r['missing_sub_units']}" if r["missing_sub_units"] else ""))
        print(f"width {r['width']:g}" + (f"; regimental supports {','.join(rsup)}" if rsup else "") + "; per terrain "
              "(1 / 2 directions: divisions x fill): " + "; ".join(
                  f"{t} " + " / ".join(f"{x['divisions']}x{x['fill']:.2f}" for x in f.values())
                  for t, f in r["width_fit"].items()))
        for prob in r["template_problems"]:
            print(f"PROBLEM {prob}")
        print(r["d"])
        return 0

    gain, factor = (pp_modifiers(w, tag) if tag else (0.0, 0.0))
    if args.gain is not None:
        gain = args.gain
    if args.factor is not None:
        factor = args.factor
    start_pp = args.start
    if start_pp is None:
        f = w.country(tag)["political_power"] if tag else None
        start_pp = f["v"] if f else 0
    buys = []
    for b in args.buy:
        d, c, label = (b.split(":", 2) + [""])[:3]
        buys.append((d, float(c), label or f"buy {c}"))
    stability = args.stability if args.stability is not None else (start_stability(w, tag) if tag else None)
    cost = args.cost if args.cost is not None else (pp_costs(w, tag) if tag else 0.0)
    r = pp(w, start_pp, gain, factor, start, args.until, buys, stability=stability, difficulty=args.difficulty,
           cost=cost, focus_cost=0.0 if args.no_focus else None, focus_until=args.focus_until)
    if args.json:
        print(json.dumps(r, indent=1))
        return 0
    print(f"{r['per_day']} PP/day with a focus running, {r['per_day_without_focus']} without (base {r['base']['v']} "
          f"[{r['base'].get('src')}] + gain {gain}, factor {factor} + stability {r['stability_factor']:g} + "
          f"difficulty {r['difficulty_factor']:g}; costs {cost:g} + focus {r['focus_cost']:g}); start {start_pp}")
    for row in r["rows"]:
        print(f"{row['date']:>10} {row['buy']} -{row['cost']:g} -> {row['balance']}"
              + (f" ({row['late_days']} days after {row['planned']})" if row["late_days"] else ""))
    for nb in r["not_bought"]:
        print(f"NOT BOUGHT by {r['end']['date']}: {nb}")
    print(f"end {r['end']['date']}: {r['end']['balance']} PP. {r['d']}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
