#!/usr/bin/env python3
"""Calculators for the strategist: construction compounding, template equipment, political power.

    calc.py build-order [--tag T --date D] [--civ N --mil N] [--cg 0.25] --order civ:10,mil:5,infrastructure:2
                        [--until D] [--infra L | --states 161,162] [--bonus 0.1]
    calc.py equipment   (--tag T --template NAME | --regiments infantry=6,artillery=1 --support engineer)
                        [--divisions N] [--year Y]
    calc.py pp          [--tag T --date D] [--start PP] [--gain PP_PER_DAY] [--factor F]
                        --until D [--buy 1936.2.1:150:free_trade ...]

Every constant is read from the effective game files where it exists (common/defines, common/buildings,
common/units, common/ideas) and printed with its source. Constants the files do not hold are labelled
"not in defines" with where the number comes from:

    construction points per civilian factory per day   5      not in defines (wiki; economy-build-orders.md 1.1)
    state infrastructure speed multiplier              1 + 0.2 x level   not in defines (wiki; same section)
    factories per construction project                 15     the defines have no construction key; the same
                                                              number as NMarket.MAX_CIV_FACTORIES_PER_CONTRACT
                                                              (economy-build-orders.md section 0, user-verified)
    political power per day                            NPolitics.BASE_POLITICAL_POWER_INCREASE (the comment
                                                              says weekly; the game shows it per day)

Global options as facts.py: --game, --userdir, --mod, --dlc, --json. Stdlib only, Python 3.9+.
"""
import argparse
import json
import math
import os
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


def parse_order(text):
    out = []
    for part in (text or "").split(","):
        part = part.strip()
        if not part:
            continue
        kind, _, n = part.partition(":")
        out += [kind.strip()] * int(n or 1)
    return out


def build_order(w, order, civ, mil, cg, start, until=None, slots=None, infra=None, bonus=0.0, costs=None):
    """Simulate construction day by day. `order` is a list of kinds ("civ", "mil", "infra", ...),
    `slots` a list of [state, free slots, infrastructure] taken in order (None: one state with
    unlimited slots and `infra`). Each project gets at most the project cap of the free civs.
    Returns {"rows": [...], "end": {...}, "constants": {...}}."""
    k = constants(w)
    cap = k["project_cap"]["v"]
    btypes = w.building_types()
    costs = costs or {}
    queue = []
    slots = [list(s) for s in slots] if slots else None
    for i, kind in enumerate(order):
        b = KIND.get(kind, kind)
        cost = costs.get(b) or (btypes.get(b) or {}).get("cost")
        if cost is None:
            raise ValueError(f"unknown building {kind!r}")
        state, lvl = None, infra if infra is not None else 0
        if slots and (btypes.get(b) or {}).get("shares_slots"):
            pick = next((s for s in slots if s[1] > 0), None)
            if pick is None:
                queue.append({"kind": kind, "building": b, "cost": cost, "state": None, "infra": lvl,
                              "progress": 0.0, "blocked": "no free slot"})
                continue
            pick[1] -= 1
            state, lvl = pick[0], pick[2]
        elif slots:
            state, lvl = slots[0][0], slots[0][2]
        queue.append({"kind": kind, "building": b, "cost": cost, "state": state, "infra": lvl, "progress": 0.0})
    today = day_number(start)
    stop = day_number(until) if until else today + 365 * 10
    rows = []
    civs, mils = civ, mil
    while queue and today < stop:
        total = civs + mils
        free = max(0, civs - int(math.ceil(cg * total)))
        active = [q for q in queue if not q.get("blocked")][: max(1, int(math.ceil(free / cap))) if free else 0]
        left = free
        done = []
        for q in active:
            n = min(cap, left)
            left -= n
            if n <= 0:
                break
            speed = n * CP_PER_CIV * (1 + INFRA_STEP * q["infra"]) * (1 + bonus)
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
            rows.append({"date": _fmt(today), "kind": q["kind"], "state": q["state"], "civ": civs, "mil": mils,
                         "free_civs": max(0, civs - int(math.ceil(cg * (civs + mils))))})
        if not active and not done:
            break
    return {"rows": rows, "end": {"date": _fmt(today), "civ": civs, "mil": mils},
            "unfinished": [{"kind": q["kind"], "progress": round(q["progress"]), "cost": q["cost"],
                            **({"blocked": q["blocked"]} if q.get("blocked") else {})} for q in queue],
            "constants": k, "consumer_goods": cg,
            "d": "daily: free civs = civs - ceil(cg x (civs + mils)); each project <= cap civs; progress = civs x "
                 "cp_per_civ x (1 + infra_step x infrastructure) x (1 + bonus); a finished civ joins the next day"}


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


def equipment(w, regiments, support, divisions=1, year=1936):
    units = sub_units(w)
    costs = equipment_costs(w, year)
    need, manpower, missing = {}, 0, []
    for name, n in list(regiments.items()) + [(s, 1) for s in support]:
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
    return {"rows": rows, "manpower_per_division": manpower, "manpower_total": manpower * divisions,
            "divisions": divisions, "missing_sub_units": missing,
            "ic_total": round(sum(r["ic_total"] or 0 for r in rows), 1),
            "d": "need of each sub-unit (common/units) x count x divisions; IC = build_cost_ic of the newest variant "
                 f"with year <= {year} (common/units/equipment), no production bonuses"}


# ---------------------------------------------------------------- political power

def pp(w, start_pp, gain, factor, start, until, buys):
    """Daily PP balance from start to until; purchases [(date, cost, label)] wait until affordable."""
    base, base_f = w.define("BASE_POLITICAL_POWER_INCREASE", 2)
    per_day = (base + gain) * (1 + factor)
    balance = float(start_pp)
    today, stop = day_number(start), day_number(until)
    pending = sorted(((day_number(d), c, label) for d, c, label in buys), key=lambda x: x[0])
    rows = []
    while today <= stop:
        while pending and pending[0][0] <= today and balance >= pending[0][1]:
            when, cost, label = pending.pop(0)
            balance -= cost
            rows.append({"date": _fmt(today), "buy": label, "cost": cost, "planned": _fmt(when),
                         "late_days": today - when, "balance": round(balance, 1)})
        today += 1
        if today <= stop:
            balance += per_day
    return {"per_day": round(per_day, 3), "base": dict(base_f or {}, v=base,
                                                       note="the define comment says weekly; the game adds it daily"),
            "gain": gain, "factor": factor, "rows": rows, "end": {"date": _fmt(stop), "balance": round(balance, 1)},
            "not_bought": [{"planned": _fmt(d), "cost": c, "buy": l} for d, c, l in pending],
            "d": "per day = (BASE_POLITICAL_POWER_INCREASE + gain) x (1 + factor); a purchase happens on its date or "
                 "the first day the balance covers it"}


def pp_modifiers(w, tag):
    """(flat gain, factor) from the tag's starting ideas (political_power_gain, political_power_factor)."""
    mods = derive.idea_modifiers(w.game)
    gain = factor = 0.0
    for f in w.country(tag)["ideas"]:
        m = mods.get(f["v"], {})
        gain += m.get("political_power_gain", 0.0)
        factor += m.get("political_power_factor", 0.0)
    return gain, factor


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
    b.add_argument("--order", required=True, help="civ:10,mil:5,infra:2 (kinds: civ mil dock infra or a building)")
    b.add_argument("--until")
    b.add_argument("--infra", type=int, help="infrastructure of every project (no state slots)")
    b.add_argument("--states", help="state ids in build order (default with --tag: the dossier ranking)")
    b.add_argument("--bonus", type=float, default=0.0, help="construction speed bonus, 0.1 = +10%%")
    e = sub.add_parser("equipment")
    _common(e)
    e.add_argument("--template", help="division template name in the tag's start OOB")
    e.add_argument("--regiments", help="infantry=6,artillery_brigade=1")
    e.add_argument("--support", help="engineer,artillery")
    e.add_argument("--divisions", type=int, default=1)
    e.add_argument("--year", type=int)
    q = sub.add_parser("pp")
    _common(q)
    q.add_argument("--start", type=float, help="PP at the start (default: the history's add_political_power, else 0)")
    q.add_argument("--gain", type=float, help="extra PP per day (default: starting ideas)")
    q.add_argument("--factor", type=float, help="political_power_factor (default: starting ideas)")
    q.add_argument("--until", required=True)
    q.add_argument("--buy", action="append", default=[], help="DATE:COST:LABEL (repeat)")
    args = ap.parse_args(argv)
    w = factsmod.world(args, args.tag)
    tag = w.tag
    start = w.date

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
        slots = None
        if args.states:
            st = w.states()
            slots = [[int(s), st[int(s)]["free_slots"] or 0, st[int(s)]["infrastructure"]] for s in args.states.split(",")]
        elif tag and args.infra is None:
            from hoi4facts import dossier as dossiermod
            slots = [[r["id"], r["free_slots"] or 0, r["infrastructure"]] for r in dossiermod.state_rows(w, tag)
                     if r["core"]]
        r = build_order(w, parse_order(args.order), civ, mil, cg, start, args.until, slots,
                        args.infra if args.infra is not None else 0, args.bonus)
        r["consumer_goods_src"] = cg_src
        r["start"] = {"date": start, "civ": civ, "mil": mil}
        if args.json:
            print(json.dumps(r, indent=1))
            return 0
        print(f"start {start}: {civ} civ, {mil} mil, consumer goods {cg} ({', '.join(cg_src) or 'none'})")
        for k, v in r["constants"].items():
            print(f"  {k} = {v['v']} [{v['src']}]")
        for row in r["rows"]:
            print(f"{row['date']:>10} {row['kind']:<8} state {row['state']} -> civ {row['civ']} mil {row['mil']} "
                  f"(free civs {row['free_civs']})")
        for u in r["unfinished"]:
            print(f"unfinished {u}")
        print(f"end {r['end']['date']}: civ {r['end']['civ']} mil {r['end']['mil']}. {r['d']}")
        return 0

    if args.cmd == "equipment":
        regs, sup = {}, []
        if args.template:
            o = w.oob(tag)
            t = o["templates"].get(args.template)
            if not t:
                print(json.dumps({"error": f"no template {args.template!r} in {tag}'s start OOB",
                                  "templates": sorted(x for x in o["templates"] if x)}))
                return 1
            regs, sup = dict(t["regiments"]), list(t["support"])
        if args.regiments:
            for part in args.regiments.split(","):
                n, _, c = part.partition("=")
                regs[n.strip()] = int(c or 1)
        if args.support:
            sup += [s.strip() for s in args.support.split(",") if s.strip()]
        year = args.year or int(start.split(".")[0])
        r = equipment(w, regs, sup, args.divisions, year)
        if args.json:
            print(json.dumps(r, indent=1))
            return 0
        for row in r["rows"]:
            print(f"{row['equipment']}: {row['per_division']:g}/div x{r['divisions']} = {row['total']:g}"
                  + (f" ({row['variant']} {row['ic_each']} IC -> {row['ic_total']} IC)" if row["ic_each"] else ""))
        print(f"manpower {r['manpower_per_division']}/div, {r['manpower_total']} total; IC {r['ic_total']}"
              + (f"; unknown sub-units {r['missing_sub_units']}" if r["missing_sub_units"] else ""))
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
    r = pp(w, start_pp, gain, factor, start, args.until, buys)
    if args.json:
        print(json.dumps(r, indent=1))
        return 0
    print(f"{r['per_day']} PP/day (base {r['base']['v']} [{r['base'].get('src')}] + gain {gain}, factor {factor}); "
          f"start {start_pp}")
    for row in r["rows"]:
        print(f"{row['date']:>10} {row['buy']} -{row['cost']:g} -> {row['balance']}"
              + (f" ({row['late_days']} days after {row['planned']})" if row["late_days"] else ""))
    for nb in r["not_bought"]:
        print(f"NOT BOUGHT by {r['end']['date']}: {nb}")
    print(f"end {r['end']['date']}: {r['end']['balance']} PP. {r['d']}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
