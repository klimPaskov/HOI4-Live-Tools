"""5. Construction: the civilian-against-military break-even for this country now, where to build, when to stop.

Inputs, all read (the modifier list is printed so the reader sees every term):
  common/buildings          base_cost of the general-production and the military-production building (the factory
                            classes are found by what a building produces, not by name); base_cost_conversion
  ideas, dynamic modifiers, researched techs, a live read   production_speed_buildings_factor and the per-building
                            production_speed_<building>_factor terms; consumer_goods_expected_value / _factor
  states                    free slots and infrastructure of the owned states (World.states, or the live snapshot)
  calc.py constants         construction points per civilian factory and the infrastructure step (not in the defines;
                            calc.py labels their source)

Break-even (the split-bonus form, derived again here): a civilian factory costs
k = (C_civ / (1 + b_civ)) / (C_mil / (1 + b_mil)) military factories of construction. Built, it adds a factory of
construction, one military factory every P0 = C_mil / (cp x (1 + b_mil) x (1 + step x infrastructure)) days, less the
consumer-goods share cg of itself; and the k military factories it replaces would each have taken cg of a civilian
factory from the queue. So per day the civilian choice is ahead by g = (1 - cg + k x cg) / P0 military factories.
Two objectives, both printed:
  standing   the military factories standing at the war date (the force's production when it starts):
             a civilian factory pays when the days left exceed k / g; the last one is queued k / g days before
  output     the military-factory-days up to a horizon (the war date + the war's planned length): pays when the days
             left exceed 2k / g (t^2 g / 2 against k t)
Civilians building civilians before the switch only make the switch rule stronger (bang-bang: all civilian, then all
military); the military factories' efficiency ramp is left out (it favours civilians a little); construction
technologies and laws researched later shorten P0, so re-run with the live snapshot. Focus gates that count factories
are separate purchases, listed.
"""
import re

from .. import calc, derive
from ..world import add_days, day_number
from .context import production_buildings


def _building(sit, kind):
    kinds = production_buildings(sit.game)
    names = [b for b, k in kinds.items() if k == kind]
    if not names:
        return None
    if len(names) == 1:
        return names[0]
    held = {}
    states = sit.world.states()
    for sid in sit.world.owned_states(sit.tag):
        for b, lvl in (states[sid].get("buildings") or {}).items():
            held[b] = held.get(b, 0) + lvl
    return max(names, key=lambda b: (held.get(b, 0), b))


def bonuses(sit, building):
    """(total, [(source, modifier, value)]) of the construction speed bonus that applies to `building`."""
    _, rows = sit.modifiers()
    keys = {"production_speed_buildings_factor", f"production_speed_{building}_factor"}
    terms = [(src, k, v) for src, k, v in rows if k in keys]
    return round(sum(v for _, _, v in terms), 4), terms


def consumer_goods(sit):
    if sit.state.get("consumer_goods") is not None:
        return float(sit.state["consumer_goods"]), [("live read", "consumer_goods", float(sit.state["consumer_goods"]))]
    _, rows = sit.modifiers()
    exp = [(s, k, v) for s, k, v in rows if k == "consumer_goods_expected_value"]
    fac = [(s, k, v) for s, k, v in rows if k == "consumer_goods_factor"]
    share = sum(v for _, _, v in exp)
    share += share * sum(v for _, _, v in fac)
    return round(share, 4), exp + fac


def states_to_build(sit, limit=6):
    """Owned states by construction speed (infrastructure) then free slots: where the next factories go."""
    states = sit.world.states()
    step = calc.INFRA_STEP
    rows = []
    for sid in sit.world.owned_states(sit.tag):
        s = states[sid]
        free = s.get("free_slots") or 0
        if free <= 0:
            continue
        infra = s.get("infrastructure") or 0
        rows.append({"state": sid, "name": s.get("name"), "free_slots": free, "infrastructure": infra,
                     "speed": round(1 + step * infra, 2)})
    rows.sort(key=lambda r: (-r["speed"], -r["free_slots"], r["state"]))
    return rows[:limit]


def gates(sit):
    """Focus availability gates that count factories (industrial_complex > N, num_of_civilian_factories > N): the
    civilian factories bought for a gate are a separate purchase from the break-even."""
    out = []
    try:
        from .. import focus as focusmod
        tree = focusmod.tree(sit.world, sit.tag)
    except Exception:  # noqa: BLE001 - no tree: no gates
        return out
    pat = re.compile(r"(industrial_complex|num_of_civilian_factories|num_of_factories|num_of_military_factories)"
                     r"\s*(>|>=|<)\s*(\d+)")
    for fid, f in (tree.get("focuses") or {}).items():
        text = " ".join(str(f.get(k) or "") for k in ("available", "bypass"))
        for m in pat.finditer(text):
            out.append({"focus": fid, "needs": f"{m.group(1)} {m.group(2)} {m.group(3)}"})
    return out


def evaluate(sit, war_date=None, horizon_years_after=2.0):
    f = sit.factories()
    civ_b = _building(sit, "general")
    mil_b = _building(sit, "military")
    costs = derive.building_costs(sit.game)
    c_civ = costs.get(civ_b, {}).get("cost") or 0.0
    c_mil = costs.get(mil_b, {}).get("cost") or 0.0
    b_civ, t_civ = bonuses(sit, civ_b)
    b_mil, t_mil = bonuses(sit, mil_b)
    cg, t_cg = consumer_goods(sit)
    where = states_to_build(sit)
    infra = (sum(r["infrastructure"] for r in where[:3]) / len(where[:3])) if where else 0.0
    cp = calc.CP_PER_CIV
    k = (c_civ / (1 + b_civ)) / (c_mil / (1 + b_mil)) if c_civ and c_mil else None
    P0 = c_mil / (cp * (1 + b_mil) * (1 + calc.INFRA_STEP * infra)) if c_mil else None
    g = (1 - cg + (k or 0) * cg) / P0 if P0 else None
    today = day_number(sit.date)
    single = (c_civ / (1 + b_civ)) / (c_mil / (1 + b_civ)) if c_civ and c_mil else None
    out = {"civilian_building": civ_b, "military_building": mil_b, "cost_civ": c_civ, "cost_mil": c_mil,
           "b_civ": b_civ, "b_mil": b_mil, "terms_civ": t_civ, "terms_mil": t_mil, "consumer_goods": cg,
           "terms_cg": t_cg, "infrastructure": round(infra, 2), "cp_per_civ": cp,
           "k": round(k, 3) if k else None, "k_single_bonus": round(single, 3) if single else None,
           "P0_days": round(P0, 1) if P0 else None, "gain_per_day": round(g, 6) if g else None,
           "standing_days": round(k / g, 1) if k and g else None,
           "output_days": round(2 * k / g, 1) if k and g else None,
           "factories": f, "where": where, "gates": gates(sit), "constants": calc.constants(sit.world)}
    if war_date and k and g:
        h = day_number(war_date)
        h2 = h + int(365 * horizon_years_after)
        out["war_date"] = war_date
        out["horizon"] = add_days(war_date, int(365 * horizon_years_after))

        def last(end, days):
            return add_days(sit.date, int(end - days - today)) if end - days > today else None
        out["last_civilian_standing"] = last(h, k / g)
        out["last_civilian_output"] = last(h2, 2 * k / g)
        stop = out["last_civilian_output"] or out["last_civilian_standing"]
        out["decision"] = (f"civilian factories until {out['last_civilian_standing'] or 'now'} for the most military "
                           f"factories at {war_date}; until {out['last_civilian_output'] or 'now'} for the most output "
                           f"to {out['horizon']}; then military" if stop else
                           "military factories now: a civilian factory queued today repays after both horizons")
    out["conversion"] = _conversion(sit, mil_b)
    out["method"] = __doc__.split("\n\n")[2].strip() if __doc__ else ""
    return out


def _conversion(sit, building):
    """Converting a civilian factory to a military one (base_cost_conversion) against building one new."""
    from ..script import get1, parse_file
    for p in sit.game.files("common/buildings"):
        for k, v in parse_file(p):
            if k != "buildings" or not isinstance(v, list):
                continue
            for name, b in v:
                if name == building and isinstance(b, list):
                    try:
                        c = float(get1(b, "base_cost_conversion"))
                    except (TypeError, ValueError):
                        return None
                    return {"building": building, "conversion_cost": c,
                            "note": "converting gives a military factory for conversion_cost but removes a civilian "
                                    "one; worth it late in a war when the queue no longer needs the civilian factory"}
    return None
