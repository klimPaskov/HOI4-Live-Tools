"""Theatres against each enemy: borders, hubs, railways, ports, infrastructure-0 states and routes.

theatre(world, tag, enemy) uses the mod-layered province graph (mapgraph, via Game.map_graph) with
owners at the start date (World.states):

    region      the strategic (air) regions that hold the enemy's land provinces, and the states
                with land provinces in them (any owner)
    borders     our state x their state pairs with the number of touching provinces and river crossings
    hubs        supply hubs (map/supply_nodes.txt) in the region: province, level, state, owner
    railways    railway lines (map/railways.txt) with a province in the region: level, length, states
    ports       naval bases in the region's states
    infra0      region states with infrastructure 0 at the start
    routes      cheapest land path from our border provinces to each of the enemy's top VP
                provinces (mapgraph.step_cost: terrain, infrastructure, rivers), with river
                crossings and steps, through our and the enemy's territory only (no third countries),
                and the river-free alternative when one exists
    terrain     terrain mix (share of land provinces by terrain) of the border provinces (both sides),
                the route provinces and the enemy's territory, with the width target (width_target)
    overseas    whether the enemy is reachable by land from our capital through our, our subjects' and
                the enemy's territory; if not, the sea route from a home port to a theatre port with the
                adjacency rules it crosses (overseas())
    port_candidates  our coastal provinces near the front without a naval base or with a low one, with
                the next level's cost and finish date (calc.build_order)
    weather     per strategic region of the routes: the months with mud, ground snow, cold and heat
                (weather_months)

route_multi() is a multi-source Dijkstra over mapgraph's step costs. width_target(), ports(),
weather_months() and terrain_width() serve the facts CLI (terrain-width, ports, weather).
"""
import heapq

from . import mapgraph

MONTHS = ("Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec")

# ---------------------------------------------------------------- combat width

WIDTH_DIRECTIONS = (1, 2)
WIDTH_CANDIDATES = tuple(range(10, 46))
WIDTH_METHOD = ("battle width of a terrain = combat_width + combat_support_width x (attack directions - 1) "
                "(common/terrain; the 1.19 extra-direction rule); for a division width w: n = floor(width / w) "
                "whole divisions, fill = n x w / width, times (1 + COMBAT_STACKING_PENALTY x divisions above "
                "COMBAT_STACKING_START + COMBAT_STACKING_EXTRA x (directions - 1)); a terrain's score is the mean "
                "fill over 1 and 2 directions [?: direction weights are judgement]; target = the integer width "
                "10-45 with the best score on the dominant terrain, ties to the best score on the mix, then the "
                "smaller width; no over-width (COMBAT_OVER_WIDTH_PENALTY lets one more division join at a "
                "penalty [?])")


def _stacking(world):
    vals = {}
    for key, dflt in (("COMBAT_STACKING_START", 5), ("COMBAT_STACKING_EXTRA", 3), ("COMBAT_STACKING_PENALTY", -0.02)):
        v, f = world.define(key, dflt)
        vals[key] = {"v": v, "src": (f or {}).get("src")}
    return vals


def battle_widths(row, directions=WIDTH_DIRECTIONS):
    """{directions: battle width} of a terrain row (World.terrains)."""
    base = row.get("combat_width")
    if base is None:
        return {}
    extra = row.get("combat_support_width", base / 2.0)
    return {k: base + extra * (k - 1) for k in directions}


def fill(w, width, k, stack):
    """(fill, divisions) of division width w in a battle of `width` with k attack directions."""
    n = int(width // w) if w > 0 else 0
    if n <= 0:
        return 0.0, 0
    eff = n * w / float(width)
    limit = stack["COMBAT_STACKING_START"]["v"] + stack["COMBAT_STACKING_EXTRA"]["v"] * (k - 1)
    eff *= max(0.0, 1 + stack["COMBAT_STACKING_PENALTY"]["v"] * max(0, n - limit))
    return round(eff, 4), n


def width_target(world, shares, candidates=WIDTH_CANDIDATES):
    """Width target for a terrain mix ({terrain: share}); see WIDTH_METHOD."""
    terr = world.terrains()
    stack = _stacking(world)
    rows = {t: terr[t] for t in shares if t in terr and terr[t].get("combat_width")}
    if not rows:
        return {"target": None, "note": "no terrain with combat_width in the mix", "d": WIDTH_METHOD}
    total = sum(shares[t] for t in rows) or 1.0
    widths = {t: battle_widths(r) for t, r in rows.items()}

    def score(t, w):
        return sum(fill(w, W, k, stack)[0] for k, W in widths[t].items()) / len(widths[t])

    def mix(w):
        return sum(shares[t] * score(t, w) for t in rows) / total

    dom = max(rows, key=lambda t: (shares[t], t))
    ranked = sorted(candidates, key=lambda w: (-round(score(dom, w), 3), -round(mix(w), 3), w))
    target = ranked[0]
    by_mix = sorted(candidates, key=lambda w: (-round(mix(w), 3), w))
    small = sorted((w for w in candidates if w <= 25), key=lambda w: (-round(mix(w), 3), w))

    def detail(w):
        return {"width": w, "dominant_score": round(score(dom, w), 3), "mix_score": round(mix(w), 3),
                "fit": {t: {k: {"divisions": fill(w, W, k, stack)[1], "fill": fill(w, W, k, stack)[0]}
                            for k, W in widths[t].items()} for t in rows}}

    return {"target": target, "dominant": dom, "dominant_share": shares[dom],
            "battle_widths": {t: {str(k): W for k, W in ws.items()} for t, ws in widths.items()},
            "sources": {t: r.get("width_src") or r.get("src") for t, r in rows.items()},
            "stacking": stack, "detail": detail(target),
            "best_for_mix": [detail(w) for w in by_mix[:3]], "best_small": detail(small[0]) if small else None,
            "d": WIDTH_METHOD}


def terrain_width(world, provinces):
    """Terrain mix of the given provinces and its width target."""
    g = world.graph()
    if g is None:
        return {"error": "no map graph"}
    mix = mapgraph.terrain_mix(g, provinces)
    return {"mix": mix, "width": width_target(world, mix["shares"]) if mix["provinces"] else None}


# ---------------------------------------------------------------- weather

WEATHER_METHOD = ("per strategic-region period (map/strategicregions weather): phenomenon weights normalised to "
                  "shares; mud = steady state of rain water (water_gain_on_rain_light/heavy per day) against "
                  "dry-out (water_dryout_multiplier interpolated over water_dryout_temperature at the mean "
                  "temperature, on dry days) capped at water_gain_max, times the period's mud factor; 'mud' from "
                  "water_gain_min, 'heavy' from a quarter of water_gain_max [?]; frozen (no mud) below "
                  "mud min_temperature; ground snow likewise from snow_gain_on_snowing/blizzard against "
                  "snow_melt_multiplier (levels weather_ground_snow_medium/high); cold and heat from the period's "
                  "temperature range against weather_extreme_cold / very_cold / very_hot / extreme_hot "
                  "(common/weather.txt). An approximation of the engine's daily simulation [?]")


def _lerp(pair_x, pair_y, x):
    (x0, x1), (y0, y1) = pair_x, pair_y
    if x <= x0:
        return y0
    if x >= x1:
        return y1
    return y0 + (y1 - y0) * (x - x0) / float(x1 - x0)


def _pair(v, dflt):
    return v if isinstance(v, list) and len(v) == 2 else dflt


def weather_months(world, rid):
    """[{month, temperature, rain, snowfall, sandstorm, mud, wet, ground_snow, cold, heat}] of a strategic
    region (WEATHER_METHOD), or None for an unknown region."""
    reg = world.strategic_regions().get(rid)
    if not reg:
        return None
    st = world.weather_settings()["settings"]
    mud = st.get("mud") or {}
    snow = st.get("snowing") or {}
    temp = st.get("temperature") or {}
    gmax = mud.get("water_gain_max", 200.0)
    gmin = mud.get("water_gain_min", 5.0)
    smax = snow.get("snow_gain_max", 80.0)
    s_med = _pair(snow.get("weather_ground_snow_medium"), [20.0, 40.0])[0]
    s_high = _pair(snow.get("weather_ground_snow_high"), [40.0, 1000.0])[0]
    snow_tmax = _pair(snow.get("temperature"), [1.0, -4.0])[0]
    out = []
    for per in reg["periods"]:
        if not per.get("start"):
            continue
        keys = ("no_phenomenon", "rain_light", "rain_heavy", "snow", "blizzard", "sandstorm")
        total = sum(per.get(k, 0.0) for k in keys) or 1.0
        p = {k: per.get(k, 0.0) / total for k in keys}
        lo, hi = (per["temperature"] + [None, None])[:2] if per.get("temperature") else (None, None)
        mean = (lo + hi) / 2.0 if lo is not None and hi is not None else 10.0
        gain = p["rain_light"] * mud.get("water_gain_on_rain_light", 0.5) + \
            p["rain_heavy"] * mud.get("water_gain_on_rain_heavy", 1.8)
        dry = _lerp(_pair(mud.get("water_dryout_temperature"), [2.0, 38.0]),
                    _pair(mud.get("water_dryout_multiplier"), [0.0005, 0.4]), mean) * \
            max(0.0, 1 - p["rain_light"] - p["rain_heavy"])
        wet = min(gmax, gain / dry) if dry > 0 else (gmax if gain > 0 else 0.0)
        wet *= per.get("mud", 1.0)
        frozen = mean < mud.get("min_temperature", -1.0)
        level = "frozen" if frozen and wet >= gmin else ("heavy" if wet >= gmax / 4.0 else
                                                          ("mud" if wet >= gmin else "none"))
        can_snow = lo is not None and lo <= snow_tmax
        sgain = (p["snow"] * snow.get("snow_gain_on_snowing", 1.0) +
                 p["blizzard"] * snow.get("snow_gain_on_blizzard", 5.0)) if can_snow else 0.0
        melt = _lerp(_pair(snow.get("snow_melt_temperature"), [0.0, 12.0]),
                     _pair(snow.get("snow_melt_multiplier"), [0.001, 0.9]), mean) * \
            max(0.0, 1 - p["snow"] - p["blizzard"])
        ground = min(smax, sgain / melt) if melt > 0 else (smax if sgain > 0 else 0.0)
        cold = None
        if lo is not None:
            if lo <= _pair(temp.get("weather_extreme_cold"), [-100.0, -12.0])[1]:
                cold = "extreme"
            elif lo <= _pair(temp.get("weather_very_cold"), [-12.0, -4.0])[1]:
                cold = "very"
        heat = None
        if hi is not None:
            if hi >= _pair(temp.get("weather_extreme_hot"), [28.0, 100.0])[0]:
                heat = "extreme"
            elif hi >= _pair(temp.get("weather_very_hot"), [20.0, 28.0])[0]:
                heat = "very"
        out.append({"month": per["start"][1], "name": MONTHS[(per["start"][1] - 1) % 12],
                    "temperature": [lo, hi], "rain": round(p["rain_light"] + p["rain_heavy"], 2),
                    "heavy_rain": round(p["rain_heavy"], 2), "snowfall": round(p["snow"] + p["blizzard"], 2),
                    "sandstorm": round(p["sandstorm"], 2), "mud_factor": per.get("mud"),
                    "wet": round(wet, 1), "mud": level,
                    "ground_snow": "high" if ground >= s_high else ("medium" if ground >= s_med else
                                                                     ("light" if ground >= 1 else "none")),
                    "cold": cold, "heat": heat})
    return out


def _months_text(rows, pred):
    ms = [r["name"] for r in rows if pred(r)]
    return ",".join(ms) if ms else "none"


def weather_summary(world, rid):
    """One-line summary of a region's year: mud, heavy mud, ground snow, extreme cold, extreme heat months."""
    rows = weather_months(world, rid) or []
    reg = world.strategic_regions().get(rid) or {}
    return {"region": rid, "name": reg.get("name"), "src": reg.get("src"),
            "mud": _months_text(rows, lambda r: r["mud"] in ("mud", "heavy")),
            "heavy_mud": _months_text(rows, lambda r: r["mud"] == "heavy"),
            "snow": _months_text(rows, lambda r: r["ground_snow"] in ("medium", "high")),
            "cold": _months_text(rows, lambda r: r["cold"] == "extreme"),
            "heat": _months_text(rows, lambda r: r["heat"] == "extreme")}


def province_regions(world):
    """{province: strategic region id} from map/strategicregions (no map graph needed)."""
    return world.cached("provregion", lambda: {p: rid for rid, r in world.strategic_regions().items()
                                               for p in r["provinces"]})


# ---------------------------------------------------------------- ports and overseas theatres

PORT_METHOD = ("port capacity = NSupply.NAVAL_BASE_FLOW + NSupply.NAVAL_FLOW_PER_LEVEL x naval base level "
               "(the max flow of a naval supply node, defines comment); NBuildings.SUPPLY_PORT_LEVEL_THROUGHPUT "
               "is listed as the per-level throughput")
OVERSEAS_METHOD = ("overseas = no land path (passable borders and straits) from the capital's provinces to the "
                   "enemy's provinces through our, our subjects' and the enemy's territory at the start (military "
                   "access not counted [?]); sea route = shortest sea path (centroid distance) from a sea zone of a "
                   "home port (our naval base land-linked to the capital through our territory) to a sea zone of a "
                   "theatre port (our naval base land-linked to the enemy through our and the enemy's territory); "
                   "rules = adjacency rules (map/adjacency_rules.txt) of the links on that path")


def port_capacity(world, level):
    base, bf = world.define("NAVAL_BASE_FLOW", 5.0)
    per, pf = world.define("NAVAL_FLOW_PER_LEVEL", 3.0)
    return round(base + per * (level or 0), 2), [x["src"] for x in (bf, pf) if x]


def _capital_provinces(world, tag):
    cap = world.country(tag)["capital"]
    st = world.states().get(cap["v"]) if cap else None
    if not st:
        return []
    if st["vps"]:
        best = max(st["vps"].items(), key=lambda kv: (kv[1], -kv[0]))[0]
        return [best]
    return st["provinces"][:1]


def _subjects(world, tag):
    return {s["v"] for s in world.country(tag)["subjects"] if s.get("v") and s.get("kind") != "release_puppet"}


def port_rows(world, tag):
    """Our naval bases at the start: [{province, name, state, state_name, level, capacity, sea, region}]."""
    g = world.graph()
    regions = province_regions(world)
    out = []
    for sid in world.owned_states(tag):
        st = world.states()[sid]
        for pid, b in sorted(st["province_buildings"].items()):
            lvl = b.get("naval_base")
            if not lvl:
                continue
            cap, _ = port_capacity(world, lvl)
            out.append({"province": pid, "name": world.province_name(pid), "state": sid, "state_name": st["name"],
                        "level": lvl, "capacity": cap, "region": regions.get(pid),
                        "sea": mapgraph.sea_neighbours(g, pid) if g else [], "src": st["file"]})
    return out


def rule_row(world, name):
    r = world.adjacency_rules().get(name)
    if not r:
        return {"rule": name, "src": None}
    owners = {}
    for pid in r["required_provinces"]:
        owners[pid] = world.owner_of_province(pid)
    closed = [rel for rel in ("enemy", "contested", "neutral", "friend") if r.get(rel) and not r[rel].get("navy", True)]
    no_trade = [rel for rel in ("enemy", "contested", "neutral", "friend") if r.get(rel) and not r[rel].get("trade", True)]
    return {"rule": name, "navy_closed_when": closed, "trade_closed_when": no_trade,
            "required_provinces": owners, "is_disabled": r.get("is_disabled"), "src": r["src"]}


def overseas(world, tag, enemy):
    """{"overseas": bool, "home_ports", "theatre_ports", "route", "rules", "without": [...], "d"} (OVERSEAS_METHOD)."""
    g = world.graph()
    if g is None:
        return {"enemy": enemy, "overseas": None, "error": "no map graph", "note": "no map graph"}
    mine = {tag} | _subjects(world, tag)
    sides = mine | {enemy}
    cap = _capital_provinces(world, tag)
    own = world.owner_of_province
    reach = mapgraph.land_reach(g, cap, allowed=lambda p: own(p) in sides)
    enemy_provs = set(world.provinces_of([enemy]))
    out = {"enemy": enemy, "capital_provinces": cap, "overseas": not (reach & enemy_provs), "d": OVERSEAS_METHOD}
    if not out["overseas"]:
        return out
    home_reach = mapgraph.land_reach(g, cap, allowed=lambda p: own(p) in mine)
    front_reach = mapgraph.land_reach(g, sorted(enemy_provs), allowed=lambda p: own(p) in sides)
    ports = port_rows(world, tag)
    home = [p for p in ports if p["province"] in home_reach]
    theatre_ports = [p for p in ports if p["province"] in front_reach]
    out["home_ports"] = home
    out["theatre_ports"] = theatre_ports
    if not home or not theatre_ports:
        out["route"] = None
        out["note"] = "no home port" if not home else "no own port land-linked to the enemy: a landing is needed"
        return out
    src = {s for p in home for s in p["sea"]}
    dst = {s for p in theatre_ports for s in p["sea"]}
    r = mapgraph.sea_route(g, sorted(src), sorted(dst))
    if r is None:
        out["route"] = None
        out["note"] = "no sea route between the home and theatre ports"
        return out
    r["from_port"] = next((p["province"] for p in home if r["path"][0] in p["sea"]), None)
    r["to_port"] = next((p["province"] for p in theatre_ports if r["path"][-1] in p["sea"]), None)
    out["route"] = r
    names = list(dict.fromkeys(x["rule"] for x in r["rules"]))
    out["rules"] = [rule_row(world, n) for n in names]
    out["without"] = []
    for n in names[:2]:
        alt = mapgraph.sea_route(g, sorted(src), sorted(dst), avoid_rules=(n,))
        out["without"].append({"avoid": n, "steps": alt["steps"] if alt else None,
                               "pixels": alt["pixels"] if alt else None,
                               "rules": list(dict.fromkeys(x["rule"] for x in alt["rules"])) if alt else None})
    return out


SUPPLY_DEFINES = ("NAVAL_BASE_INITIAL_SUPPLY_FLOW", "NAVAL_BASE_STARTING_PENALTY_PER_PROVINCE",
                  "NAVAL_BASE_ADDED_PENALTY_PER_PROVINCE", "NODE_INITIAL_SUPPLY_FLOW", "NODE_STARTING_PENALTY_PER_PROVINCE",
                  "NODE_ADDED_PENALTY_PER_PROVINCE", "CAPITAL_INITIAL_SUPPLY_FLOW", "CAPITAL_STARTING_PENALTY_PER_PROVINCE",
                  "CAPITAL_ADDED_PENALTY_PER_PROVINCE", "SUPPLY_CONVOY_FACTOR", "NAVAL_BASE_FLOW", "NAVAL_FLOW_PER_LEVEL",
                  "RAILWAY_BASE_FLOW", "RAILWAY_FLOW_PER_LEVEL")
SUPPLY_METHOD = ("reach after d provinces = INITIAL_SUPPLY_FLOW - STARTING_PENALTY - ADDED_PENALTY x (d - 1), terrain, "
                 "infrastructure, rivers and the distance falloff ignored (a linear reading of the NSupply comments [?]); "
                 "level cost = base_cost + per_level_extra_cost x levels already built (common/buildings [?]); "
                 "a port supplies only while convoys reach it (SUPPLY_CONVOY_FACTOR convoys per supply)")
CANDIDATE_METHOD = ("coastal land provinces (definition.csv coastal, a sea neighbour) we own or control at the start "
                    "within N land steps of our border provinces with the enemy (BFS through our territory), "
                    "with no naval base or one below the given level; cost of the next level from common/buildings; "
                    "finish = calc.build_order of that one level with our civilian factories at the start, the "
                    "consumer-goods share of our laws and the state's infrastructure")


def supply_compare(world):
    """Supply-flow defines (port vs hub vs capital), the build costs of naval_base, supply_node, rail_way,
    and the linear reach table (SUPPLY_METHOD)."""
    vals = {}
    for key in SUPPLY_DEFINES:
        v, f = world.define(key)
        if f:
            vals[key] = {"v": v, "src": f["src"]}
    bt = world.building_types()
    costs = {b: {"base_cost": bt[b]["cost"], "per_level_extra_cost": bt[b].get("per_level_cost"),
                 "src": bt[b]["src"]} for b in ("naval_base", "supply_node", "rail_way") if b in bt}

    def reach(prefix):
        i = vals.get(prefix + "_INITIAL_SUPPLY_FLOW", {}).get("v")
        s = vals.get(prefix + "_STARTING_PENALTY_PER_PROVINCE", {}).get("v")
        a = vals.get(prefix + "_ADDED_PENALTY_PER_PROVINCE", {}).get("v")
        if None in (i, s, a):
            return None
        return [round(max(0.0, i - s - a * (d - 1)), 2) for d in range(1, 5)]

    return {"defines": vals, "costs": costs,
            "reach_1_to_4": {"naval_base": reach("NAVAL_BASE"), "supply_node": reach("NODE"),
                             "capital": reach("CAPITAL")}, "d": SUPPLY_METHOD}


def front_provinces(world, tag, enemy):
    """Our provinces on the land border with the enemy at the start."""
    theirs = set(world.owned_states(enemy))
    out = set()
    for a in world.owned_states(tag):
        for b, pairs in world.state_borders().get(a, {}).items():
            if b in theirs:
                out |= {x for x, _ in pairs}
    return out


def level_cost(world, building, level):
    """Cost of raising a building from `level` to level + 1 (base_cost + per_level_extra_cost x level [?])."""
    bt = world.building_types().get(building) or {}
    if bt.get("cost") is None:
        return None
    return bt["cost"] + (bt.get("per_level_cost") or 0) * (level or 0)


def port_candidates(world, tag, enemy, front=None, steps=3, below=3, limit=8):
    """Coastal provinces near the front for a new or bigger naval base (CANDIDATE_METHOD)."""
    from . import calc  # local: calc imports the facts CLI, which imports this module

    g = world.graph()
    if g is None:
        return {"error": "no map graph"}
    front = set(front_provinces(world, tag, enemy) if front is None else front)
    out = {"enemy": enemy, "steps": steps, "below_level": below, "rows": [], "d": CANDIDATE_METHOD}
    if not front:
        out["note"] = "no land border with %s: no own coast at the front (a landing or a captured port)" % enemy
        return out
    states = world.states()
    ps = world.province_state()
    mine = lambda p: (states.get(ps.get(p)) or {}).get("controller") == tag  # noqa: E731
    dist = {p: 0 for p in front}
    frontier = list(front)
    for d in range(1, steps + 1):
        nxt = []
        for u in frontier:
            for v, e in g["adj"].get(u, {}).items():
                if v in dist or e.get("impassable") or g["provinces"].get(v, {}).get("type") != "land" or not mine(v):
                    continue
                dist[v] = d
                nxt.append(v)
        frontier = nxt
    civ = sum(states[s]["buildings"].get("industrial_complex", 0) for s in world.owned_states(tag))
    mil = sum(states[s]["buildings"].get("arms_factory", 0) for s in world.owned_states(tag))
    cg = calc.consumer_goods(world, tag)[0]
    for p, d in sorted(dist.items(), key=lambda kv: (kv[1], kv[0])):
        sea = mapgraph.sea_neighbours(g, p)
        if not sea or not g["provinces"][p].get("coastal"):
            continue
        sid = ps.get(p)
        st = states.get(sid) or {}
        lvl = (st.get("province_buildings", {}).get(p) or {}).get("naval_base", 0)
        if lvl >= below:
            continue
        cost = level_cost(world, "naval_base", lvl)
        r = calc.build_order(world, ["naval_base"], civ, mil, cg, world.date, infra=st.get("infrastructure", 0),
                             costs={"naval_base": cost}) if cost else None
        done = r["rows"][0]["date"] if r and r["rows"] else None
        out["rows"].append({"province": p, "name": world.province_name(p), "state": sid, "state_name": st.get("name"),
                            "steps": d, "level": lvl, "next_level_cost": cost, "infrastructure": st.get("infrastructure"),
                            "finish": done, "days": (r["rows"][0].get("days") if r and r["rows"] else None),
                            "sea": sea[:3], "supply_hub": g["provinces"][p].get("supply_hub")})
    out["rows"].sort(key=lambda x: (x["steps"], -x["level"], -(x["infrastructure"] or 0), x["province"]))
    out["count"] = len(out["rows"])
    out["rows"] = out["rows"][:limit]
    return out


def ports(world, tag, enemies=None, steps=3, below=3):
    """Ports of the tag with capacity, per theatre the overseas check, the sea route's adjacency rules and the
    coastal candidates near the front, and the port / hub / railway supply comparison."""
    enemies = list(enemies) if enemies else world.at_war_with(tag)
    rows = port_rows(world, tag)
    _, srcs = port_capacity(world, 0)
    _, tf = world.define("SUPPLY_PORT_LEVEL_THROUGHPUT", None)
    return {"tag": tag, "ports": rows, "capacity_sources": srcs, "throughput_per_level": tf,
            "theatres": [dict(overseas(world, tag, e), candidates=port_candidates(world, tag, e, steps=steps,
                                                                                  below=below)) for e in enemies],
            "supply": supply_compare(world), "d": PORT_METHOD, "fair": "public"}


def route_multi(graph, sources, dst, avoid_rivers=False, weights=None, allowed=None):
    """Cheapest land path from any source to dst; `allowed(province)` limits the provinces entered."""
    w = mapgraph._weights(graph, weights)
    provs = graph["provinces"]
    adj = graph["adj"]
    dist, prev, heap = {}, {}, []
    for s in sources:
        if s in provs:
            dist[s] = 0.0
            heapq.heappush(heap, (0.0, s))
    while heap:
        d, u = heapq.heappop(heap)
        if u == dst:
            break
        if d > dist.get(u, float("inf")):
            continue
        for v, e in adj.get(u, {}).items():
            if e.get("impassable") or v not in provs or provs[v]["type"] != "land":
                continue
            if allowed is not None and v != dst and not allowed(v):
                continue
            nd = d + mapgraph.step_cost(graph, u, v, e, w, avoid_rivers)
            if nd < dist.get(v, float("inf")):
                dist[v] = nd
                prev[v] = u
                heapq.heappush(heap, (nd, v))
    if dst not in dist:
        return None
    path = [dst]
    while path[-1] in prev:
        path.append(prev[path[-1]])
    path.reverse()
    crossings = mapgraph.path_crossings(graph, path)
    cost = sum(mapgraph.step_cost(graph, a, b, adj[a][b], w, False) for a, b in zip(path, path[1:]))
    return {"path": path, "from": path[0], "cost": round(cost, 2), "steps": len(path) - 1,
            "river_crossings": crossings, "rivers": len(crossings)}


ROUTE_METHOD = ("multi-source Dijkstra from our border provinces; step cost = terrain movement_cost x "
                "(1 + 0.5 x (5 - infrastructure) / 5) + 1 per river crossing (mapgraph.step_cost)")


def theatre(world, tag, enemy, top=8):
    g = world.graph()
    states = world.states()
    borders = world.state_borders()
    theirs = set(world.owned_states(enemy))
    ours = set(world.owned_states(tag))
    ps = world.province_state()
    enemy_provs = world.provinces_of([enemy])
    regions = sorted({g["provinces"][p].get("region") for p in enemy_provs
                      if p in g["provinces"] and g["provinces"][p].get("region") is not None}) if g else []
    region_provs = {p for p, row in (g["provinces"].items() if g else ()) if row.get("region") in set(regions)}
    region = {ps[p] for p in region_provs if p in ps}
    out = {"enemy": enemy, "name": world.country_name(enemy), "strategic_regions": regions,
           "region_states": sorted(region),
           "borders": [], "hubs": [], "railways": [], "ports": [], "infra0": [], "routes": [],
           "fair": "public"}
    if g is None:
        out["error"] = "no map graph"
        return out
    our_front, their_front = set(), set()
    for a in sorted(ours):
        for b, pairs in sorted(borders.get(a, {}).items()):
            if b not in theirs:
                continue
            rivers = sum(1 for x, y in pairs if g["adj"][x][y].get("river_crossing"))
            out["borders"].append({"ours": a, "theirs": b, "our_name": states[a]["name"],
                                   "their_name": states[b]["name"], "pairs": len(pairs), "river_pairs": rivers})
            our_front |= {x for x, _ in pairs}
            their_front |= {y for _, y in pairs}
    for pid, level in sorted(g["supply_hubs"].items()):
        sid = g["provinces"].get(pid, {}).get("state")
        if pid in region_provs:
            out["hubs"].append({"province": pid, "name": world.province_name(pid), "level": level, "state": sid,
                                "state_name": states.get(sid, {}).get("name"),
                                "owner": states.get(sid, {}).get("owner"),
                                "src": world.src(world.game.file("map/supply_nodes.txt"))})
    for r in g["railways"]:
        sts = []
        for pid in r["provinces"]:
            s = ps.get(pid)
            if s and (not sts or sts[-1] != s):
                sts.append(s)
        if any(p in region_provs for p in r["provinces"]):
            out["railways"].append({"level": r["level"], "provinces": len(r["provinces"]),
                                    "ends": [r["provinces"][0], r["provinces"][-1]], "states": sts,
                                    "src": world.src(world.game.file("map/railways.txt"), r["line"])})
    for sid in sorted(region):
        st = states.get(sid)
        if not st:
            continue
        for pid, b in sorted(st["province_buildings"].items()):
            if b.get("naval_base"):
                out["ports"].append({"province": pid, "state": sid, "owner": st["owner"], "level": b["naval_base"],
                                     "name": world.province_name(pid)})
        if st["infrastructure"] == 0 and not st["impassable"]:
            out["infra0"].append({"state": sid, "name": st["name"], "owner": st["owner"]})
    vp = world.vp(enemy)
    targets = sorted(vp["provinces"], key=lambda r: (-r["vp"], r["province"]))[:top]
    out["overseas"] = overseas(world, tag, enemy)
    out["port_candidates"] = port_candidates(world, tag, enemy, front=our_front)
    if not our_front:
        out["routes_note"] = "no land border with %s: routes need a landing" % enemy
        _theatre_terrain(world, out, set(), set(), enemy_provs)
        return out
    hub_provs = [p for p in g["supply_hubs"] if world.owner_of_province(p) == tag]
    hub_steps = world.province_steps(hub_provs) if hub_provs else {}
    sides = {tag, enemy}

    def allowed(pid):
        return world.owner_of_province(pid) in sides

    for t in targets:
        r = route_multi(g, sorted(our_front), t["province"], allowed=allowed)
        if not r:
            out["routes"].append({"to": t["province"], "vp": t["vp"], "error": "unreachable over land"})
            continue
        dry = route_multi(g, sorted(our_front), t["province"], avoid_rivers=True, allowed=allowed) \
            if r["rivers"] else None
        owners = []
        for pid in r["path"]:
            o = world.owner_of_province(pid)
            if o and (not owners or owners[-1] != o):
                owners.append(o)
        row = {"to": t["province"], "name": t["name"], "vp": t["vp"], "state": t["state"], "from": r["from"],
               "steps": r["steps"], "rivers": r["rivers"], "cost": r["cost"], "path": r["path"],
               "crossings": r["river_crossings"], "owners": owners,
               "from_hub_steps": hub_steps.get(r["from"]), "d": ROUTE_METHOD}
        if dry and dry["rivers"] < r["rivers"]:
            row["dry"] = {"steps": dry["steps"], "rivers": dry["rivers"], "cost": dry["cost"], "from": dry["from"]}
        out["routes"].append(row)
    route_provs = {p for r in out["routes"] for p in r.get("path", [])}
    _theatre_terrain(world, out, our_front | their_front, route_provs, enemy_provs)
    return out


def _theatre_terrain(world, out, border, route_provs, enemy_provs):
    """Terrain mix of the border, the routes and the enemy's territory; the width target of the fighting
    terrain (border and routes; the territory when there is no land border); the weather of its regions."""
    g = world.graph()
    mixes = {"border": mapgraph.terrain_mix(g, sorted(border)), "routes": mapgraph.terrain_mix(g, sorted(route_provs)),
             "territory": mapgraph.terrain_mix(g, sorted(enemy_provs))}
    fight = sorted(border | route_provs) or sorted(enemy_provs)
    fmix = mapgraph.terrain_mix(g, fight)
    out["terrain"] = {"mixes": mixes, "fighting": fmix,
                      "basis": "border+routes" if (border or route_provs) else "territory",
                      "width": width_target(world, fmix["shares"]) if fmix["provinces"] else None}
    regions = province_regions(world)
    counts = {}
    for p in fight:
        rid = regions.get(p)
        if rid is not None:
            counts[rid] = counts.get(rid, 0) + 1
    out["weather"] = [dict(weather_summary(world, rid), provinces=n)
                      for rid, n in sorted(counts.items(), key=lambda kv: -kv[1])[:4]]
