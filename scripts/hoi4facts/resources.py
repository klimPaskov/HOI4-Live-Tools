"""resources.py: resources as strategy. Where the strategic resources lie, who owns them, how they reach the plan's
country and what a source costs, from the game files; lint_resources.py holds a plan's production lines to it.

Facts (common/resources, common/buildings, common/technologies, common/ideas, history/states, the map):
  states            every state with a strategic resource: id, name, owner, units by resource; ranked for one
                    resource by units.
  reach             the owner's route to the plan's country: "own", "land" (a chain of land borders between
                    countries, BFS over the start borders, and the capitals within NTrade BASE_LAND_TRADE_RANGE,
                    taken as map pixels), else "sea" (convoys: 0.1 per unit, raiding and blocked routes apply).
  discounts         ideas whose targeted modifier changes our trade cost (trade_cost_for_target_factor with
                    tag = us: Sweden's -20% for Germany) with the file that holds them; overlord_trade_cost_factor
                    (a subject's price for its overlord) is named in the method.
  synthetic         the synthetic refinery: base cost, rubber per refinery (local_resources_rubber plus every
                    tech whose on_research_complete raises the building's rubber), the per-state cap.
The method is in RESOURCES_STRATEGY_METHOD. Stdlib only, Python 3.9+.
"""
from collections import defaultdict, deque

from .script import get1, parse_file

RESOURCE_KEYS = ("oil", "aluminium", "rubber", "tungsten", "steel", "chromium", "coal")
UNITS_PER_CIV = 8.0          # common/resources cic 0.125: one civilian factory buys 8 units
CONVOYS_PER_UNIT = 0.1       # common/resources convoys 0.1 per traded unit by sea
RESOURCES_STRATEGY_METHOD = (
    "resource states from history/states (owner at the start); reach by a BFS over the start land borders "
    "between countries (land) else sea; a trade costs one civilian factory per 8 units (cic 0.125) times "
    "1 + the discounts (targeted trade_cost_for_target_factor ideas, overlord_trade_cost_factor for a "
    "subject's overlord), 0.1 convoy per unit by sea; the synthetic refinery from common/buildings "
    "(local_resources_rubber, state_max) and the techs that raise its rubber (modify_building_resources)")


def _num(v, default=0.0):
    try:
        return float(v)
    except (TypeError, ValueError):
        return default


def country_borders(world):
    """{tag: set(tags it shares a land border with)} at the start."""
    states = world.states()
    out = defaultdict(set)
    for sid, row in world.state_borders().items():
        a = (states.get(sid) or {}).get("owner")
        for nb in row:
            b = (states.get(nb) or {}).get("owner")
            if a and b and a != b:
                out[a].add(b)
                out[b].add(a)
    return out


def _capital_xy(world, tag):
    """Mean map position (pixels) of the provinces of `tag`'s capital state, or None."""
    try:
        cap = (world.country(tag).get("capital") or {}).get("v")
    except Exception:  # noqa: BLE001 - a country without history has no capital
        return None
    g = world.graph()
    if not cap or g is None:
        return None
    pts = [p["xy"] for p in g["provinces"].values() if p.get("state") == cap and p.get("xy")]
    if not pts:
        return None
    return (sum(x for x, _ in pts) / len(pts), sum(y for _, y in pts) / len(pts))


def land_range(world):
    """NTrade BASE_LAND_TRADE_RANGE (1000): beyond it a trade goes by sea. The unit is taken as map pixels
    between the capitals (assumption: the define names no unit)."""
    from . import derive
    return derive.define(world.game, "BASE_LAND_TRADE_RANGE", 1000.0) or 1000.0


def reach(world, tag, owner, borders=None):
    """"own", "land" or "sea": how the owner's goods reach `tag`. Land: a chain of land borders between the
    countries (BFS over the start borders) and the capitals within the land trade range; else sea."""
    if owner == tag:
        return "own"
    borders = borders if borders is not None else country_borders(world)
    a, b = _capital_xy(world, tag), _capital_xy(world, owner)
    if a and b and ((a[0] - b[0]) ** 2 + (a[1] - b[1]) ** 2) ** 0.5 > land_range(world):
        return "sea"
    seen, queue = {tag}, deque([tag])
    while queue:
        c = queue.popleft()
        for n in borders.get(c, ()):
            if n == owner:
                return "land"
            if n not in seen:
                seen.add(n)
                queue.append(n)
    return "sea"


def resource_states(world, tag, resource=None, limit=None):
    """States holding strategic resources: [{id, name, owner, resources, reach}], the richest in `resource`
    first (all resources summed when none is named)."""
    borders = country_borders(world)
    rows = []
    for sid, st in world.states().items():
        res = {k: v for k, v in (st.get("resources") or {}).items() if v}
        if not res or (resource and not res.get(resource)):
            continue
        owner = st.get("owner")
        rows.append({"id": sid, "name": st.get("name"), "owner": owner, "resources": res,
                     "reach": reach(world, tag, owner, borders) if owner else None})
    key = (lambda r: -r["resources"].get(resource, 0)) if resource else (lambda r: -sum(r["resources"].values()))
    rows.sort(key=lambda r: (key(r), r["id"]))
    return rows[:limit] if limit else rows


def _walk_targeted(items, path=()):
    """Yield (idea name, targeted_modifier block) from a parsed ideas file."""
    if not isinstance(items, list):
        return
    for k, v in items:
        if k == "targeted_modifier" and isinstance(v, list):
            yield (path[-1] if path else None), v
        elif isinstance(v, list):
            yield from _walk_targeted(v, path + (k,))


def trade_discounts(world, tag):
    """[{idea, file, factor}] ideas whose targeted modifier changes `tag`'s trade cost (negative = cheaper)."""
    out = []
    for p in world.game.files("common/ideas"):
        try:
            items = parse_file(p)
        except Exception:  # noqa: BLE001 - one bad file never stops the facts
            continue
        for idea, block in _walk_targeted(items):
            if get1(block, "tag") != tag:
                continue
            f = get1(block, "trade_cost_for_target_factor")
            if f is not None:
                out.append({"idea": idea, "file": str(p).split("common/")[-1], "factor": _num(f)})
    out.sort(key=lambda r: (r["factor"], r["idea"] or ""))
    return out


def synthetic_refinery(world):
    """{base_cost, rubber_base, rubber_per_tech: {tech: n}, rubber_max, state_max} of the synthetic refinery."""
    row = {"base_cost": None, "rubber_base": 0.0, "rubber_per_tech": {}, "state_max": None}
    for p in world.game.files("common/buildings"):
        for k, v in parse_file(p):
            if k != "buildings" or not isinstance(v, list):
                continue
            for name, body in v:
                if name != "synthetic_refinery" or not isinstance(body, list):
                    continue
                row["base_cost"] = _num(get1(body, "base_cost"), None)
                row["rubber_base"] = _num(get1(body, "local_resources_rubber"))
                cap = get1(body, "level_cap")
                if isinstance(cap, list):
                    row["state_max"] = _num(get1(cap, "state_max"), None)
    for p in world.game.files("common/technologies"):
        for k, v in parse_file(p):
            if k != "technologies" or not isinstance(v, list):
                continue
            for tech, body in v:
                done = get1(body, "on_research_complete") if isinstance(body, list) else None
                if not isinstance(done, list):
                    continue
                for ek, ev in done:
                    if ek == "modify_building_resources" and isinstance(ev, list) and \
                            get1(ev, "building") == "synthetic_refinery" and get1(ev, "resource") == "rubber":
                        row["rubber_per_tech"][tech] = _num(get1(ev, "amount"))
    row["rubber_max"] = row["rubber_base"] + sum(row["rubber_per_tech"].values())
    return row


def refinery_rubber(row, techs_done):
    """Rubber of one synthetic refinery with the techs in `techs_done` researched."""
    return row["rubber_base"] + sum(n for t, n in row["rubber_per_tech"].items() if t in techs_done)


def facts(world, tag, resource=None, limit=30):
    return {"tag": tag, "resource": resource,
            "states": resource_states(world, tag, resource, limit),
            "discounts": trade_discounts(world, tag),
            "synthetic_refinery": synthetic_refinery(world),
            "units_per_civ": UNITS_PER_CIV, "convoys_per_unit": CONVOYS_PER_UNIT,
            "d": RESOURCES_STRATEGY_METHOD}
