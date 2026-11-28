"""Air range against strategic regions, from the map files.

A wing's coverage of a strategic region is the share of the region's land provinces whose centre lies within the
wing's air range of its air base (the game shows it as "Coverage in this region"; the engine measures from the base,
the map scale is NDefines MAP_SCALE_PIXEL_TO_KM). This module computes it from provinces.bmp centres
(mapgraph) and the defines, so a plan can ask "which range keeps full mission efficiency over that region from those
bases" before it designs and builds the planes. Nothing here names a country or a map: any mod's map works.

    radius(graph, region)                 km from the region's best inner province to its farthest one
    coverage(graph, region, bases, km)    share of the region's land provinces within km of the nearest base point
    range_for(graph, region, bases, share) the least range (km) that gives `share` coverage from those bases
    base_points(graph, states=, provinces=) province centres of air bases (a state's best-connected province:
                                            its centroid's nearest province)
"""
import math

DEFAULT_KM_PER_PIXEL = 7.114  # vanilla NDefines.NGame.MAP_SCALE_PIXEL_TO_KM; pass the define from the files


def _land(graph, region):
    """Centres of the region's land provinces."""
    region = int(region)
    out = []
    for pid, p in graph["provinces"].items():
        if p.get("region") == region and p.get("xy") and p.get("type") == "land":
            out.append(tuple(p["xy"]))
    return out


def _d(a, b, km):
    return math.hypot(a[0] - b[0], a[1] - b[1]) * km


def radius(graph, region, km=DEFAULT_KM_PER_PIXEL):
    pts = _land(graph, region)
    if not pts:
        return None
    best = min(pts, key=lambda a: max(_d(a, b, km) for b in pts))
    return round(max(_d(best, b, km) for b in pts), 1)


def coverage(graph, region, bases, air_range, km=DEFAULT_KM_PER_PIXEL):
    pts = _land(graph, region)
    if not pts or not bases:
        return 0.0
    hit = sum(1 for b in pts if min(_d(a, b, km) for a in bases) <= air_range)
    return round(hit / len(pts), 3)


def range_for(graph, region, bases, share=0.9, km=DEFAULT_KM_PER_PIXEL):
    """The least range reaching `share` of the region's provinces from the nearest base (None: no base or region)."""
    pts = _land(graph, region)
    if not pts or not bases:
        return None
    ds = sorted(min(_d(a, b, km) for a in bases) for b in pts)
    k = max(0, min(len(ds) - 1, math.ceil(share * len(ds)) - 1))
    return round(ds[k], 1)


def state_point(graph, state):
    """The province of a state whose centre is nearest the state's centroid (an air base stands in one province)."""
    st = graph["states"].get(int(state))
    if not st:
        return None
    pts = [tuple(graph["provinces"][p]["xy"]) for p in st["provinces"]
           if p in graph["provinces"] and graph["provinces"][p].get("xy")]
    if not pts:
        return None
    cx, cy = sum(x for x, _ in pts) / len(pts), sum(y for _, y in pts) / len(pts)
    return min(pts, key=lambda a: (a[0] - cx) ** 2 + (a[1] - cy) ** 2)


def base_points(graph, states=(), provinces=()):
    out = [state_point(graph, s) for s in states]
    out += [tuple(graph["provinces"][int(p)]["xy"]) for p in provinces
            if int(p) in graph["provinces"] and graph["provinces"][int(p)].get("xy")]
    return [p for p in out if p]


def table(graph, regions, bases, ranges, km=DEFAULT_KM_PER_PIXEL):
    """Rows {region, radius_km, coverage {range: share}, range_for_90} for a theatre's regions."""
    rows = []
    for r in regions:
        rows.append({"region": int(r), "radius_km": radius(graph, r, km),
                     "coverage": {int(x): coverage(graph, r, bases, x, km) for x in ranges},
                     "range_for_90": range_for(graph, r, bases, 0.9, km)})
    return rows
