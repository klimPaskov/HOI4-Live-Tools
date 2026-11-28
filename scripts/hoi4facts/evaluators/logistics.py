"""16 and 10. Logistics: the fuel ledger (stock target per war day) and the supply an attack axis gets (hubs,
railways, trucks, trains).

FUEL LEDGER. Inputs, all read:
  common/defines     BASE_FUEL_CAPACITY, BASE_FUEL_GAIN, BASE_FUEL_GAIN_PER_OIL, STARTING_FUEL_RATIO,
                     ARMY_FUEL_COST_MULT, ARMY_COMBAT_FUEL_MULT, ARMY_MOVEMENT_FUEL_MULT, ARMY_IDLE_FUEL_MULT,
                     NAir.MISSION_FUEL_COSTS (by mission, names from the define's own comment lines), NAir.FUEL_COST_MULT,
                     CAPITULATE_FUEL_RATIO
  common/buildings   every building whose country_modifiers carry max_fuel_building (thousands of fuel per level)
                     or fuel_gain_from_states (fuel an hour per level): the levels in our owned states
  modifiers          fuel_gain_factor, max_fuel_factor, max_fuel_building, fuel_gain_from_states (ideas, techs, live)
  resources          the oil left after the trade law's export share and the production lines (world.resources), or
                     the live excess (state["resources"]["oil"]) or the caller's figure
  divisions          per division the fuel an hour of its template (divstats: each needed equipment's
                     fuel_consumption once per battalion x own_equipment_fuel_consumption_mult, x (1 + the companies'
                     and technologies' fuel_consumption_factor))
  live               state["fuel"] = {"stock", "capacity"} (hoi4_economy_detail fuel)

  capacity  = (BASE_FUEL_CAPACITY + 1000 x sum(levels x max_fuel_building)) x (1 + max_fuel_factor)
  gain/hour = (BASE_FUEL_GAIN + BASE_FUEL_GAIN_PER_OIL x excess oil + sum(levels x fuel_gain_from_states))
              x (1 + fuel_gain_factor) [?: whether the factor multiplies the base and the oil part too]
  use/hour  = sum over attacking divisions of fuel x ARMY_FUEL_COST_MULT x (ARMY_COMBAT_FUEL_MULT +
              ARMY_MOVEMENT_FUEL_MULT) (an attack moves and fights; idle divisions burn ARMY_IDLE_FUEL_MULT)
              + planes x fuel_consumption x MISSION_FUEL_COSTS[mission] x FUEL_COST_MULT
  stock at the war date = the live stock, else STARTING_FUEL_RATIO x capacity at the history's date, plus the gain
              over the peace days, capped at capacity (peace use such as exercises is not counted)
  per war day = 24 x (use - gain); the stock target for W war days = W x that; days the stock carries the offensive;
              refineries that close the hourly gap; fuel silos when the target is above capacity.
  [?] THE DIVISION FUEL UNIT is not verified in game: the division view's tables give one figure per battalion (a
  motorised anti-tank battalion of 36 trucks at 1.2 shows 1.2), so the ledger takes fuel_consumption once per
  battalion, per hour while moving or fighting. One live read of one division's use settles it.

AXIS SUPPLY. Inputs, all read:
  map/supply_nodes.txt, map/railways.txt (mapgraph), the owned states, the capital
  common/defines     RAILWAY_BASE_FLOW, RAILWAY_FLOW_PER_LEVEL, MAX_RAILWAY_LEVEL, NODE_FLOW_BONUS_PER_RAIL_LEVEL,
                     CAPITAL_SUPPLY_BASE, CAPITAL_SUPPLY_CIVILIAN_FACTORIES / _MILITARY_FACTORIES / _DOCKYARDS,
                     SUPPLY_HUB_FULL_MOTORIZATION_TRUCK_COST, SUPPLY_POINTS_PER_TRAIN, NUM_RAILWAYS_TRAIN_FACTOR,
                     RAILWAY_CONVERSION_COOLDOWN, and the reach defines that derive.supply_reach reads (the node's
                     initial flow, the per-province penalties, terrain supply_flow_penalty_factor, infrastructure,
                     rivers, the distance curve)
  common/buildings   rail_way and supply_node costs (the build list)
  live               state["logistics"]["axis_supply"] (one number, or {enemy state id: number}) replaces the estimate

  a hub's throughput = RAILWAY_BASE_FLOW + RAILWAY_FLOW_PER_LEVEL x the lowest railway level on the widest railway
             path from the capital or an own port [?: the level counting], at most what feeds it: the capital's
             supply (CAPITAL_SUPPLY_BASE + the factory terms) or the port's NAVAL_BASE_FLOW + NAVAL_FLOW_PER_LEVEL x
             level (while convoys reach it); a hub with neither delivers nothing [?]; a port supplies its own
             surroundings with the NAVAL_BASE reach defines
  a province's flow from a source = derive.supply_reach along the land path through our provinces (motorised hubs
             gain SUPPLY_HUB_FULL_MOTORIZATION_BONUS), + NODE_FLOW_BONUS_PER_RAIL_LEVEL x the hub's railway level;
             each attack province takes its best source; a hub's provinces share its throughput
  an axis    = the enemy state across the border; its supply = the summed flows of our provinces on that border;
             beside it the flow after 1..N provinces of advance into it (the hub stays behind), the enemy hubs in it
             (usable after RAILWAY_CONVERSION_COOLDOWN days once taken), trucks to motorise the hubs used, trains
             (supply / SUPPLY_POINTS_PER_TRAIN x NUM_RAILWAYS_TRAIN_FACTOR x railway provinces from the capital),
             and, when the divisions planned on the axis need more than the hub passes, the railway level that
             would carry them with the segments below it and their cost; new hubs come last (the dearest fix).
"""
import math
import re
from collections import deque

from .. import derive, theatre
from ..script import get1, parse_file


def _first(v, d):
    if isinstance(v, list):
        return v[0] if v else d
    return d if v is None else v


def _d(sit, key, default):
    return float(_first(sit.define(key), default))


def _num(v):
    try:
        return float(v)
    except (TypeError, ValueError):
        return None


# ---------------------------------------------------------------- fuel ledger

FUEL_KEYS = ("max_fuel_building", "fuel_gain_from_states")


def fuel_buildings(sit):
    """{building: {"max_fuel_building": k per level, "fuel_gain_from_states": per hour per level}} of common/buildings."""

    def load():
        out = {}
        for p in sit.game.files("common/buildings"):
            for k, v in parse_file(p):
                if k != "buildings" or not isinstance(v, list):
                    continue
                for name, body in v:
                    if not name or not isinstance(body, list):
                        continue
                    cm = get1(body, "country_modifiers")
                    mods = get1(cm, "modifiers") if isinstance(cm, list) else None
                    row = {}
                    for kk, vv in (mods if isinstance(mods, list) else []):
                        if kk in FUEL_KEYS and _num(vv) is not None:
                            row[kk] = _num(vv)
                    if row:
                        out[name] = row
                    elif name in out:
                        del out[name]
        return out

    return sit.cached("fuel_buildings", load)


def building_levels(sit, names):
    """{building: levels in our owned states} (state["buildings"] when the snapshot has it)."""
    live = sit.state.get("buildings")
    if isinstance(live, dict):
        return {b: float(live.get(b, 0)) for b in names}, "live snapshot"
    states = sit.world.states()
    out = {b: 0.0 for b in names}
    for sid in sit.world.owned_states(sit.tag):
        for b, lvl in (states[sid].get("buildings") or {}).items():
            if b in out:
                out[b] += float(lvl or 0)
    return out, f"owned states at {sit.world.date}"


def mission_costs(sit, key="NAir.MISSION_FUEL_COSTS"):
    """{mission: cost} from the define list, the names from its comment lines (the order the file gives)."""
    full, row = sit._defines.row(key)
    vals = list((row or {}).get("v") or [])
    names = []
    try:
        with open(row["file"], encoding="utf-8-sig", errors="replace") as fh:
            lines = fh.read().split("\n")[row["line"] - 1:]
        for ln in lines[1:]:
            if "}" in ln.split("--")[0]:
                break
            m = re.search(r"--\s*([A-Z_]+)", ln)
            if m and re.search(r"\d", ln.split("--")[0]):
                names.append(m.group(1))
    except (OSError, TypeError, KeyError):
        pass
    return {n: float(vals[i]) for i, n in enumerate(names) if i < len(vals)}


def fuel(sit, divisions, war_date=None, war_days=None, planes=None, oil=None, enemy=None):
    """The fuel ledger. divisions: [{"name", "count", "fuel"}] (fuel an hour of one division, divstats);
    planes: [{"name", "count", "fuel", "mission"}]; oil: the excess oil (else live, else world.resources)."""
    from ..world import day_number
    capb = _d(sit, "BASE_FUEL_CAPACITY", 50000)
    base_gain = _d(sit, "BASE_FUEL_GAIN", 2.0)
    per_oil = _d(sit, "BASE_FUEL_GAIN_PER_OIL", 2.0)
    start_ratio = _d(sit, "STARTING_FUEL_RATIO", 0.25)
    army_mult = _d(sit, "ARMY_FUEL_COST_MULT", 0.5)
    combat = _d(sit, "ARMY_COMBAT_FUEL_MULT", 1.0)
    move = _d(sit, "ARMY_MOVEMENT_FUEL_MULT", 1.0)
    idle = _d(sit, "ARMY_IDLE_FUEL_MULT", 0.0)
    air_mult = float(_first(sit.define("NAir.FUEL_COST_MULT"), 0.35))
    cap_ratio = _d(sit, "CAPITULATE_FUEL_RATIO", 0.5)
    blds = fuel_buildings(sit)
    levels, lsrc = building_levels(sit, list(blds))
    cap_k = sum(levels[b] * r.get("max_fuel_building", 0.0) for b, r in blds.items()) + sit.modifier("max_fuel_building")
    capacity = (capb + 1000.0 * cap_k) * (1 + sit.modifier("max_fuel_factor"))
    gain_states = sum(levels[b] * r.get("fuel_gain_from_states", 0.0) for b, r in blds.items()) \
        + sit.modifier("fuel_gain_from_states")
    oil_src = "given"
    if oil is None:
        live = (sit.state.get("resources") or {}).get("oil")
        if isinstance(live, (int, float)):
            oil, oil_src = float(live), "live snapshot (excess oil)"
        else:
            bal = (sit.world.resources(sit.tag).get("balance") or {}).get("oil") or {}
            oil = float(bal.get("balance", 0.0))
            oil_src = (f"history: {bal.get('produced', 0):g} produced - {bal.get('exported', 0):g} exported - "
                       f"{bal.get('needed', 0):g} for the start production lines (technology resource bonuses "
                       f"and later lines not counted)")
    oil = max(0.0, oil)
    factor = sit.modifier("fuel_gain_factor")
    gain = (base_gain + per_oil * oil + gain_states) * (1 + factor)
    live_fuel = sit.state.get("fuel") or {}
    if isinstance(live_fuel.get("capacity"), (int, float)):
        capacity = float(live_fuel["capacity"])
    days_peace = max(0, day_number(war_date) - day_number(sit.date)) if war_date else 0
    if isinstance(live_fuel.get("stock"), (int, float)):
        stock0, stock_src = float(live_fuel["stock"]), "live snapshot"
        hours = days_peace * 24
    else:
        stock0 = start_ratio * capacity
        stock_src = f"STARTING_FUEL_RATIO {start_ratio:g} x capacity at {sit.world.date}"
        hours = (max(0, day_number(war_date or sit.date) - day_number(sit.world.date))) * 24
    stock_war = min(capacity, stock0 + gain * hours)
    rows, use = [], 0.0
    per_div_mult = army_mult * (combat + move)
    for d in divisions or []:
        h = float(d.get("fuel", 0.0)) * per_div_mult * float(d.get("count", 0))
        use += h
        rows.append(dict(d, hour=round(h, 2), why=f"{d.get('count', 0):g} x {d.get('fuel', 0):g} x {army_mult:g} x "
                                                 f"({combat:g} + {move:g})"))
    costs = mission_costs(sit)
    for p in planes or []:
        m = costs.get(p.get("mission") or "AIR_SUPERIORITY", 1.0)
        h = float(p.get("fuel", 0.0)) * float(p.get("count", 0)) * m * air_mult
        use += h
        rows.append(dict(p, hour=round(h, 2), why=f"{p.get('count', 0):g} x {p.get('fuel', 0):g} x mission "
                                                 f"{m:g} x {air_mult:g}"))
    net = use - gain
    per_day = 24.0 * net
    out = {"capacity": round(capacity), "capacity_why": f"({capb:g} + 1000 x {cap_k:g}) x (1 + "
                                                       f"{sit.modifier('max_fuel_factor'):g})",
           "levels": {b: levels[b] for b in blds if levels[b]}, "levels_from": lsrc,
           "gain_hour": round(gain, 2),
           "gain_why": f"({base_gain:g} + {per_oil:g} x {oil:g} oil + {gain_states:g} from refineries) x (1 + {factor:g})",
           "oil": oil, "oil_from": oil_src, "stock_now": round(stock0), "stock_from": stock_src,
           "stock_at_war": round(stock_war), "war_date": war_date,
           "use_hour": round(use, 2), "rows": rows, "net_hour": round(net, 2), "per_war_day": round(per_day),
           "idle_mult": idle, "unverified": "division fuel: fuel_consumption once per battalion, an hour while moving "
                                            "or fighting [?: one live division read]"}
    if net > 0:
        out["days_carried"] = round(stock_war / per_day, 1)
    else:
        out["days_carried"] = None
        out["days_why"] = "the gain covers the use: the stock does not fall"
    if war_days:
        target = max(0.0, per_day * war_days)
        out["war_days"] = war_days
        out["target"] = round(target)
        out["shortfall"] = round(max(0.0, target - stock_war))
        per_ref = [(b, r["fuel_gain_from_states"] * (1 + factor)) for b, r in blds.items()
                   if r.get("fuel_gain_from_states")]
        if out["shortfall"] and per_ref:
            b, g = max(per_ref, key=lambda x: x[1])
            # refineries that make up the shortfall over the war days, and those that close the hourly gap
            out["refineries_for_shortfall"] = {"building": b, "count": math.ceil(out["shortfall"] / (g * 24 * war_days)),
                                               "per_level_hour": round(g, 2)}
            out["refineries_for_gap"] = {"building": b, "count": math.ceil(net / g) if g else None}
        silos = [(b, r["max_fuel_building"]) for b, r in blds.items() if r.get("max_fuel_building")]
        if target > capacity and silos:
            b, k = max(silos, key=lambda x: x[1])
            out["silos"] = {"building": b, "count": math.ceil((target - capacity) / (1000.0 * k * (
                1 + sit.modifier("max_fuel_factor")))), "per_level": 1000.0 * k}
    if enemy:
        try:
            other = sit.other(enemy)
            e = fuel(other, [], None)
            out["capitulation_gain"] = {"enemy": enemy, "fuel": round(cap_ratio * e["stock_now"]),
                                        "why": f"CAPITULATE_FUEL_RATIO {cap_ratio:g} x its stock {e['stock_now']:g} "
                                               f"(history prior)"}
        except Exception:  # noqa: BLE001 - an enemy the files do not hold
            pass
    out["defines"] = [sit.define_src(k) for k in ("BASE_FUEL_CAPACITY", "BASE_FUEL_GAIN", "BASE_FUEL_GAIN_PER_OIL",
                                                   "STARTING_FUEL_RATIO", "ARMY_FUEL_COST_MULT",
                                                   "ARMY_COMBAT_FUEL_MULT", "ARMY_MOVEMENT_FUEL_MULT",
                                                   "NAir.FUEL_COST_MULT")]
    return out


# ---------------------------------------------------------------- axis supply

def _own_provinces(sit, g):
    out = set()
    for sid in sit.world.owned_states(sit.tag):
        out |= set((g["states"].get(sid) or {}).get("provinces") or [])
    return out


def _rail_edges(g):
    edges = {}
    for line in g.get("railways") or []:
        p, lvl = line["provinces"], line["level"]
        for a, c in zip(p, p[1:]):
            edges.setdefault(a, {})
            edges.setdefault(c, {})
            edges[a][c] = max(edges[a].get(c, 0), lvl)
            edges[c][a] = max(edges[c].get(a, 0), lvl)
    return edges


def widest_rail(edges, src, allowed):
    """{province: (bottleneck level, railway steps, parent)} of the widest railway path from src through allowed."""
    import heapq
    best = {src: (99, 0, None)}
    heap = [(-99, 0, src)]
    while heap:
        nl, steps, x = heapq.heappop(heap)
        lvl = -nl
        if best.get(x, (0,))[0] > lvl or (best[x][0] == lvl and best[x][1] < steps):
            continue
        for y, el in edges.get(x, {}).items():
            if y not in allowed:
                continue
            b = min(lvl, el)
            cur = best.get(y)
            if cur is None or b > cur[0] or (b == cur[0] and steps + 1 < cur[1]):
                best[y] = (b, steps + 1, x)
                heapq.heappush(heap, (-b, steps + 1, y))
    return best


def _land_paths(g, src, allowed, depth):
    """{province: path from src} by land steps through allowed, up to depth."""
    from .. import mapgraph
    par = {src: None}
    q = deque([(src, 0)])
    while q:
        x, d = q.popleft()
        if d >= depth:
            continue
        for y, _ in mapgraph.neighbours(g, x):
            if y in allowed and y not in par:
                par[y] = x
                q.append((y, d + 1))

    def path(p):
        out = []
        while p is not None:
            out.append(p)
            p = par[p]
        return out[::-1]

    return {p: path(p) for p in par}


def _rail_segment(best, p):
    """[(a, b)] railway edges from the capital to p along the widest path."""
    out = []
    while p in best and best[p][2] is not None:
        out.append((best[p][2], p))
        p = best[p][2]
    return out[::-1]


def axis_supply(sit, enemy, motorization=2, advance=2, div_supply=None, divisions=None, depth=8):
    """Supply per attack axis against `enemy` (see the module doc). motorization 0..2 (2 = fully motorised hubs);
    div_supply: one division's supply use (for 'divisions carried'); divisions: planned divisions per axis (for the
    railway level needed)."""
    from .. import mapgraph
    w = sit.world
    try:
        g = w.graph()
    except Exception:  # noqa: BLE001 - no map
        g = None
    if not g:
        return {"error": "no map graph", "axes": []}
    own = _own_provinces(sit, g)
    base = _d(sit, "RAILWAY_BASE_FLOW", 10.0)
    per_level = _d(sit, "RAILWAY_FLOW_PER_LEVEL", 5.0)
    max_level = int(_d(sit, "MAX_RAILWAY_LEVEL", 5))
    rail_bonus = _d(sit, "NODE_FLOW_BONUS_PER_RAIL_LEVEL", 0.0)
    trucks_full = _d(sit, "SUPPLY_HUB_FULL_MOTORIZATION_TRUCK_COST", 60.0)
    spt = _d(sit, "SUPPLY_POINTS_PER_TRAIN", 1.0)
    ntf = _d(sit, "NUM_RAILWAYS_TRAIN_FACTOR", 0.03)
    cooldown = _d(sit, "RAILWAY_CONVERSION_COOLDOWN", 0.0)
    f = sit.factories()
    cap_supply = (_d(sit, "CAPITAL_SUPPLY_BASE", 5.0) + f["civilian"] * _d(sit, "CAPITAL_SUPPLY_CIVILIAN_FACTORIES", 0.3)
                  + f["military"] * _d(sit, "CAPITAL_SUPPLY_MILITARY_FACTORIES", 0.6)
                  + f["naval"] * _d(sit, "CAPITAL_SUPPLY_DOCKYARDS", 0.4))
    caps = theatre._capital_provinces(w, sit.tag)
    capital = caps[0] if caps else None
    edges = _rail_edges(g)
    hubs_all = dict(g.get("supply_hubs") or {})
    live_hubs = sit.state.get("supply_hubs")
    if isinstance(live_hubs, dict):
        hubs_all = {int(k): v for k, v in live_hubs.items()}
    nb, npl = _d(sit, "NAVAL_BASE_FLOW", 5.0), _d(sit, "NAVAL_FLOW_PER_LEVEL", 3.0)
    ports = {}
    for sid in w.owned_states(sit.tag):
        for p, lvl in ((g["states"].get(sid) or {}).get("naval_bases") or {}).items():
            if p in own and lvl:
                ports[int(p)] = min(cap_supply, nb + npl * float(lvl))
    # railway roots: the capital and every own naval base (a port is fed by convoys from the capital)
    roots = {}
    if capital is not None:
        roots[capital] = cap_supply
    roots.update({p: t for p, t in ports.items() if p not in roots})
    widest, root_of = {}, {}
    for r, thr in sorted(roots.items(), key=lambda kv: (kv[0] != capital, -kv[1])):
        if r not in edges:
            continue
        for p, row in widest_rail(edges, r, own).items():
            val = min(thr, base + per_level * row[0]) if p != r else thr
            if p not in widest or val > widest[p][3]:
                widest[p] = (row[0], row[1], row[2], val)
                root_of[p] = r
    rail_tree = {}
    for p, row in widest.items():
        rail_tree[p] = row
    sources = {}
    for p, lvl in hubs_all.items():
        if p not in own or p == capital:
            continue
        if p in widest:
            bl, steps, _, thr = widest[p]
            sources[p] = {"kind": "hub", "rail_level": bl if bl < 99 else max_level, "rail_steps": steps,
                          "throughput": thr, "fed_from": "capital" if root_of[p] == capital else
                          f"port {root_of[p]} (convoys)"}
        else:
            sources[p] = {"kind": "hub", "rail_level": 0, "rail_steps": None, "throughput": 0.0,
                          "note": "no railway to the capital or a port"}
    for p, thr in ports.items():
        if p not in sources and p != capital:
            sources[p] = {"kind": "port", "rail_level": 0, "rail_steps": 0, "throughput": thr,
                          "note": "naval base: supplies while convoys reach it"}
    if capital is not None:
        sources[capital] = {"kind": "capital", "rail_level": max_level, "rail_steps": 0, "throughput": cap_supply}
    live = (sit.state.get("logistics") or {}).get("axis_supply")
    theirs_states = set(w.owned_states(enemy))
    borders = w.state_borders()
    axes = {}
    for a in w.owned_states(sit.tag):
        for bst, pairs in borders.get(a, {}).items():
            if bst in theirs_states:
                ax = axes.setdefault(bst, {"ours": set(), "theirs": set()})
                for x, y in pairs:
                    ax["ours"].add(x)
                    ax["theirs"].add(y)
    if not axes:
        return {"error": f"no land border with {enemy}", "axes": [], "capital_supply": round(cap_supply, 1)}
    front = set().union(*(ax["ours"] for ax in axes.values()))
    # land paths from every source near the front (sources farther than depth steps never reach a front province)
    paths = {}
    for s in sources:
        if sources[s]["throughput"] <= 0:
            continue
        pp = _land_paths(g, s, own, depth)
        if front & set(pp):
            paths[s] = pp
    reach_cache = {}

    def reach(s, path):
        key = (s, tuple(path))
        if key not in reach_cache:
            src = sources[s]
            r = derive.supply_reach(sit.game, path, src["kind"], motorization if src["kind"] == "hub" else 0)
            if src["kind"] == "hub":
                r += rail_bonus * src["rail_level"]
            reach_cache[key] = r
        return reach_cache[key]

    best_src = {}
    for p in front:
        cands = [(reach(s, pp[p]), s) for s, pp in paths.items() if p in pp]
        if cands:
            r, s = max(cands)
            best_src[p] = (max(0.0, r), s)
    # a hub's provinces share its throughput
    by_src = {}
    for p, (r, s) in best_src.items():
        by_src.setdefault(s, []).append(p)
    share = {}
    for s, ps in by_src.items():
        tot = sum(best_src[p][0] for p in ps)
        share[s] = min(1.0, sources[s]["throughput"] / tot) if tot > 0 else 0.0
    eprov = set()
    for sid in theirs_states:
        eprov |= set((g["states"].get(sid) or {}).get("provinces") or [])
    rows = []
    rcost = theatre.level_cost(w, "rail_way", 0)
    hcost = theatre.level_cost(w, "supply_node", 0)
    for bst, ax in sorted(axes.items()):
        provs = sorted(ax["ours"])
        flows = {p: round(best_src[p][0] * share[best_src[p][1]], 2) for p in provs if p in best_src}
        used = sorted({best_src[p][1] for p in provs if p in best_src})
        total = round(sum(flows.values()), 2)
        row = {"enemy_state": bst, "name": w.state_name(bst), "provinces": provs, "flows": flows,
               "supply": total, "sources": [dict(sources[s], province=s) for s in used]}
        if isinstance(live, dict) and (bst in live or str(bst) in live):
            row["supply"], row["supply_from"] = float(live.get(bst, live.get(str(bst)))), "live read"
        elif isinstance(live, (int, float)):
            row["supply"], row["supply_from"] = float(live), "live read (one value for every axis)"
        else:
            row["supply_from"] = "estimate (supply graph)"
        # the flow after k provinces of advance into the enemy state, from the best source of the axis
        ahead = []
        start = max(flows, key=flows.get) if flows else None
        if start is not None:
            s = best_src[start][1]
            base_path = paths[s][start]
            seen, frontier = {start}, [base_path]
            for k in range(1, advance + 1):
                nxt = []
                for path in frontier:
                    for y, _ in mapgraph.neighbours(g, path[-1]):
                        if y in eprov and y not in seen:
                            seen.add(y)
                            nxt.append(path + [y])
                if not nxt:
                    break
                vals = [reach(s, p) for p in nxt]
                ahead.append({"provinces_in": k, "best": round(max(vals), 2), "mean": round(sum(vals) / len(vals), 2)})
                frontier = nxt
        row["advance"] = ahead
        row["enemy_hubs"] = [{"province": p, "level": lvl, "usable_after_days": cooldown}
                             for p, lvl in sorted(hubs_all.items()) if p in set((g["states"].get(bst) or {})
                                                                             .get("provinces") or [])]
        hubs_used = [s for s in used if sources[s]["kind"] == "hub"]
        row["trucks"] = round(trucks_full * min(1.0, motorization / 2.0) * len(hubs_used))
        steps = max((sources[s]["rail_steps"] or 0) for s in used) if used else 0
        row["trains"] = round(row["supply"] / spt * ntf * steps, 1) if steps else 0.0
        row["trains_why"] = f"{row['supply']:g} / {spt:g} x {ntf:g} x {steps} railway provinces from the capital"
        if div_supply:
            row["divisions_carried"] = int(row["supply"] // div_supply) if div_supply > 0 else None
        want = (divisions or {}).get(bst) if isinstance(divisions, dict) else divisions
        if want and div_supply:
            need = want * div_supply
            row["need"] = round(need, 2)
            thr = sum(sources[s]["throughput"] for s in used)
            if need > thr and hubs_used:
                lvl = min(max_level, math.ceil((need / max(1, len(hubs_used)) - base) / per_level))
                segs = []
                for s in hubs_used:
                    for a, c in _rail_segment(rail_tree, s):
                        if edges[a][c] < lvl:
                            segs.append({"from": a, "to": c, "level": edges[a][c]})
                levels = sum(lvl - x["level"] for x in segs)
                row["railway"] = {"level_needed": lvl, "segments": segs, "levels_to_build": levels,
                                  "cost": round(levels * rcost) if rcost else None,
                                  "why": f"need {need:g} over {len(hubs_used)} hub(s) > throughput {thr:g}: "
                                         f"{base:g} + {per_level:g} x level"}
            if row["supply"] < need:
                # last resort: a new hub on a railway province nearer the front
                cands = []
                for p in own:
                    if p in sources or p not in widest or p not in edges:
                        continue
                    pp = _land_paths(g, p, own, 3)
                    near = [q for q in provs if q in pp]
                    if not near:
                        continue
                    sources[p] = {"kind": "hub", "rail_level": widest[p][0], "rail_steps": widest[p][1],
                                  "throughput": widest[p][3]}
                    gain = sum(max(0.0, reach(p, pp[q])) for q in near)
                    del sources[p]
                    cands.append((gain, p, widest[p][0]))
                cands.sort(reverse=True)
                row["hub_candidates"] = [{"province": p, "rail_level": lvl, "flow_to_border": round(gain, 2),
                                          "cost": hcost} for gain, p, lvl in cands[:3]]
        rows.append(row)
    per = [r for row in rows for r in row["flows"].values()]
    return {"enemy": enemy, "capital": capital, "capital_supply": round(cap_supply, 1), "motorization": motorization,
            "axes": rows, "per_province_mean": round(sum(per) / len(per), 2) if per else None,
            "defines": [sit.define_src(k) for k in ("RAILWAY_BASE_FLOW", "RAILWAY_FLOW_PER_LEVEL",
                                                     "NODE_FLOW_BONUS_PER_RAIL_LEVEL", "NODE_INITIAL_SUPPLY_FLOW",
                                                     "SUPPLY_HUB_FULL_MOTORIZATION_BONUS", "CAPITAL_SUPPLY_BASE")]}
