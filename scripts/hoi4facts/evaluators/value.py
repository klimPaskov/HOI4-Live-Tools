"""Country modifiers in factory-years: the pick_value model (native/src/pick_value.rs WEIGHTS) in Python.

WEIGHTS is a copy of the Rust table: factory-years per +1.0 of a modifier held a year, as a share of a base of the
country's own (military, civilian or all factories, dockyards, research slots). The shares are the value model (a
judgement, not game data); tests/test_evaluators_value.py parses the Rust table so the two cannot drift. A plan
overrides a weight the same way as for pick_value (value.weights).

held(mods, base, years)       linear: the modifier held `years` years
compounding(mods, base, ...)  construction-speed modifiers also count the factories the extra construction builds:
                              a civilian factory's construction for a year builds r military factories
                              (r = cp x 365 x (1 + step x infrastructure) / C_mil), each producing for the rest of the
                              horizon, so a share s of the civilian factories held T years gives s x civ x r x T^2 / 2
                              on top of the linear term
"""
WEIGHTS = {
    "industrial_capacity_factory": ("Military", 1.0),
    "industrial_capacity_dockyard": ("Dockyards", 1.0),
    "industry_free_repair_factor": ("Military", 0.05),
    "production_factory_max_efficiency_factor": ("Military", 0.5),
    "production_factory_efficiency_gain_factor": ("Military", 0.2),
    "production_factory_start_efficiency_factor": ("Military", 0.1),
    "line_change_production_efficiency_factor": ("Military", 0.05),
    "consumer_goods_factor": ("All", -1.0),
    "production_speed_buildings_factor": ("Civilian", 0.9),
    "production_speed_industrial_complex_factor": ("Civilian", 0.4),
    "production_speed_arms_factory_factor": ("Civilian", 0.3),
    "production_speed_dockyard_factor": ("Civilian", 0.05),
    "production_speed_infrastructure_factor": ("Civilian", 0.1),
    "production_speed_rail_way_factor": ("Civilian", 0.05),
    "production_speed_synthetic_refinery_factor": ("Civilian", 0.03),
    "production_speed_air_base_factor": ("Civilian", 0.02),
    "production_speed_naval_base_factor": ("Civilian", 0.02),
    "production_speed_bunker_factor": ("Civilian", 0.02),
    "production_speed_coastal_bunker_factor": ("Civilian", 0.01),
    "production_speed_radar_station_factor": ("Civilian", 0.01),
    "production_speed_anti_air_building_factor": ("Civilian", 0.01),
    "local_resources_factor": ("Military", 0.3),
    "research_speed_factor": ("Slots", 3.0),
    "stability_factor": ("All", 0.2),
    "war_support_factor": ("All", 0.05),
}
WEEKLY = {"stability_weekly": "stability_factor", "war_support_weekly": "war_support_factor"}
HORIZON_DAYS = 365.0


def bases(sit):
    f = sit.factories()
    return {"Military": f["military"], "Civilian": f["civilian"], "All": f["military"] + f["civilian"] + f["naval"],
            "Dockyards": f["naval"], "Slots": float(sit.research_slots())}


def weight(key, base, overrides=None):
    if overrides and key in overrides:
        return float(overrides[key])
    if key not in WEIGHTS:
        return None
    b, share = WEIGHTS[key]
    return share * base[b]


def held(mods, base, years=1.0, overrides=None):
    """(factory-years, {key: value}, [unvalued keys])."""
    total, parts, unvalued = 0.0, {}, []
    for k, v in mods.items():
        key, mult = (WEEKLY[k], 52.0) if k in WEEKLY else (k, 1.0)
        w = weight(key, base, overrides)
        if w is None:
            unvalued.append(k)
            continue
        parts[k] = v * mult * w * years
        total += parts[k]
    return total, parts, unvalued


def build_rate(sit):
    """Military factories one civilian factory's construction builds in a year, and how (calc.py constants, the
    military building's base_cost, the best free state's infrastructure)."""
    from .. import calc, derive
    from .context import production_buildings
    mil = [b for b, k in production_buildings(sit.game).items() if k == "military"]
    cost = max((derive.building_costs(sit.game).get(b, {}).get("cost") or 0.0) for b in mil) if mil else 0.0
    infra = 0.0
    states = sit.world.states()
    for sid in sit.world.owned_states(sit.tag):
        if (states[sid].get("free_slots") or 0) > 0:
            infra = max(infra, float(states[sid].get("infrastructure") or 0))
    if not cost:
        return 0.0, "no military-production building"
    r = calc.CP_PER_CIV * 365 * (1 + calc.INFRA_STEP * infra) / cost
    return r, f"{calc.CP_PER_CIV:g} x 365 x (1 + {calc.INFRA_STEP:g} x {infra:g}) / {cost:g}"


def compounding(mods, base, years, rate, overrides=None):
    """held() plus the construction-speed keys' factories built and producing for the rest of `years`."""
    total, parts, unvalued = held(mods, base, years, overrides)
    for k, v in mods.items():
        if k in WEIGHTS and WEIGHTS[k][0] == "Civilian":
            extra = v * weight(k, base, overrides) * rate * years * years / 2.0
            parts[k] += extra
            total += extra
    return total, parts, unvalued
