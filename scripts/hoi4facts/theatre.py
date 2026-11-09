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

route_multi() is a multi-source Dijkstra over mapgraph's step costs.
"""
import heapq

from . import mapgraph


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
    our_front = set()
    for a in sorted(ours):
        for b, pairs in sorted(borders.get(a, {}).items()):
            if b not in theirs:
                continue
            rivers = sum(1 for x, y in pairs if g["adj"][x][y].get("river_crossing"))
            out["borders"].append({"ours": a, "theirs": b, "our_name": states[a]["name"],
                                   "their_name": states[b]["name"], "pairs": len(pairs), "river_pairs": rivers})
            our_front |= {x for x, _ in pairs}
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
    if not our_front:
        out["routes_note"] = "no land border with %s: routes need a landing" % enemy
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
    return out
