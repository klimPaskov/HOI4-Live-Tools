"""Where states and provinces are on the world map, read from the game files (no screen, no OCR).

The tools used to find a state by typing its name into the Find view and OCR'ing the result list, which fails for
names with diacritics (Lodz, Hessen, Bohemia). The files know everything needed to go there by id:

    map/definition.csv        province id;r;g;b;type;coastal;terrain;continent
    map/provinces.bmp         one flat colour per province (5632 x 2048)
    history/states/*.txt      state id, its provinces, victory points, owner
    map/buildings.txt         state id;building;x;y;z;...   (world coordinates, z north-up)

World coordinates are bitmap pixels with z measured from the bottom edge, so bitmap row = height - z. The minimap
shows the whole bitmap, so a state's fractional position (px / width, py / height) is also its place on the minimap.

`index()` builds {states: {id: {...}}, provinces: {...}} once per file set and caches it under
%LOCALAPPDATA%/hoi4_live/script_index/. It is pure data: functions here never touch the game.
"""

import json
import os
import re

from . import scriptdb

CACHE_VERSION = 9
_MEM = {}


def _province_colours(game_dir):
    """{(r, g, b): province id} and the sea/lake flags from definition.csv."""
    colours, kinds = {}, {}
    path = os.path.join(game_dir, "map", "definition.csv")
    with open(path, encoding="utf-8-sig", errors="replace") as f:
        for line in f:
            parts = line.strip().split(";")
            if len(parts) < 5 or not parts[0].isdigit():
                continue
            pid = int(parts[0])
            if pid == 0:
                continue
            colours[(int(parts[1]), int(parts[2]), int(parts[3]))] = pid
            kinds[pid] = parts[4]
    return colours, kinds


def _centroids(game_dir, colours):
    """{province id: (x, y, pixels, bbox)} in bitmap pixels (y down) from provinces.bmp."""
    import numpy as np
    from PIL import Image
    img = Image.open(os.path.join(game_dir, "map", "provinces.bmp")).convert("RGB")
    a = np.asarray(img, dtype=np.int64)
    h, w = a.shape[:2]
    key = (a[..., 0] << 16) | (a[..., 1] << 8) | a[..., 2]
    flat = key.ravel()
    uniq, inverse = np.unique(flat, return_inverse=True)
    ys, xs = np.divmod(np.arange(flat.size), w)
    count = np.bincount(inverse)
    sx = np.bincount(inverse, weights=xs)
    sy = np.bincount(inverse, weights=ys)
    out = {}
    pid_of = np.zeros(len(uniq), dtype=np.int32)
    for i, u in enumerate(uniq):
        rgb = (int(u >> 16) & 255, int(u >> 8) & 255, int(u) & 255)
        pid = colours.get(rgb)
        if pid is None:
            continue
        pid_of[i] = pid
        out[pid] = (float(sx[i] / count[i]), float(sy[i] / count[i]), int(count[i]))
    idmap = pid_of[inverse].reshape(h, w)
    return out, (w, h), idmap


def _province_edges(idmap):
    """{(a, b)}: provinces whose pixels touch (4-neighbourhood), a < b."""
    import numpy as np
    edges = set()
    for a, b in ((idmap[:, :-1], idmap[:, 1:]), (idmap[:-1, :], idmap[1:, :])):
        m = (a != b) & (a > 0) & (b > 0)
        lo = np.minimum(a[m], b[m]).astype(np.int64)
        hi = np.maximum(a[m], b[m]).astype(np.int64)
        packed = np.unique(lo * 100000 + hi)
        edges.update((int(v // 100000), int(v % 100000)) for v in packed)
    return edges


def _state_files(game_dir, mods):
    return scriptdb.effective_files("history/states", game_dir, mods)


def _parse_state(text):
    """(id, [provinces], {province: vp value}, owner) from a state history file (a light, regex-level read)."""
    sid = re.search(r"\bid\s*=\s*(\d+)", text)
    prov = re.search(r"\bprovinces\s*=\s*\{([^}]*)\}", text)
    if not sid or not prov:
        return None
    provinces = [int(x) for x in prov.group(1).split()]
    vps = {}
    for m in re.finditer(r"victory_points\s*=\s*\{\s*(\d+)\s+(\d+)\s*\}", text):
        vps[int(m.group(1))] = max(vps.get(int(m.group(1)), 0), int(m.group(2)))
    owner = re.search(r"\bowner\s*=\s*([A-Z0-9]{3})", text)
    return int(sid.group(1)), provinces, vps, owner.group(1) if owner else None


def _build(game_dir, mods):
    colours, kinds = _province_colours(game_dir)
    cents, (w, h), idmap = _centroids(game_dir, colours)
    states = {}
    prov_state = {}
    for p in _state_files(game_dir, mods):
        with open(p, encoding="utf-8-sig", errors="replace") as f:
            parsed = _parse_state(f.read())
        if not parsed:
            continue
        sid, provinces, vps, owner = parsed
        have = [q for q in provinces if q in cents]
        if not have:
            continue
        tot = sum(cents[q][2] for q in have)
        cx = sum(cents[q][0] * cents[q][2] for q in have) / tot
        cy = sum(cents[q][1] * cents[q][2] for q in have) / tot
        # the point to aim at: the victory point province (its centroid is inside the state and is where the
        # game puts the city), else the largest province
        anchor = max(have, key=lambda q: (vps.get(q, 0), cents[q][2]))
        states[sid] = {"id": sid, "provinces": provinces, "centroid": [round(cx, 1), round(cy, 1)],
                       "anchor_province": anchor, "anchor": [round(cents[anchor][0], 1), round(cents[anchor][1], 1)],
                       "pixels": tot, "vp": vps, "owner": owner}
        for q in provinces:
            prov_state[q] = sid
    adjacency = {}
    for a, b in _province_edges(idmap):
        sa, sb = prov_state.get(a), prov_state.get(b)
        if sa and sb and sa != sb:
            adjacency.setdefault(sa, set()).add(sb)
            adjacency.setdefault(sb, set()).add(sa)
    for k, v in adjacency.items():
        states[k]["neighbours"] = sorted(v)
    buildings = {}
    bpath = os.path.join(game_dir, "map", "buildings.txt")
    if os.path.exists(bpath):
        with open(bpath, encoding="utf-8-sig", errors="replace") as f:
            for line in f:
                parts = line.strip().split(";")
                if len(parts) < 5 or parts[1] not in ("air_base", "naval_base", "supply_node"):
                    continue
                try:
                    sid = int(parts[0])
                    x, z = float(parts[2]), float(parts[4])
                except ValueError:
                    continue
                buildings.setdefault(str(sid), {}).setdefault(parts[1], [round(x, 1), round(h - z, 1)])
    regions = {}
    for rp in scriptdb.effective_files("map/strategicregions", game_dir, mods):
        with open(rp, encoding="utf-8-sig", errors="replace") as f:
            text = f.read()
        rid = re.search(r"id\s*=\s*(\d+)", text)
        prov = re.search(r"provinces\s*=\s*\{([^}]*)\}", text)
        nm = re.search(r'name\s*=\s*"([^"]+)"', text)
        if not rid or not prov:
            continue
        plist = [int(x) for x in re.sub(r"#[^\n]*", "", prov.group(1)).split() if x.isdigit()]
        have = [q for q in plist if q in cents]
        if not have:
            continue
        sea = [q for q in have if kinds.get(q) in ("sea", "lake")]
        pool = sea or have
        tot = sum(cents[q][2] for q in pool)
        cx = sum(cents[q][0] * cents[q][2] for q in pool) / tot
        cy = sum(cents[q][1] * cents[q][2] for q in pool) / tot
        # an aim point inside the region: the sea province nearest to the centroid
        aim = min(pool, key=lambda q: (cents[q][0] - cx) ** 2 + (cents[q][1] - cy) ** 2)
        regions[str(int(rid.group(1)))] = {"id": int(rid.group(1)), "name_key": nm.group(1) if nm else None,
                                           "provinces": plist, "naval": bool(sea), "all_sea": bool(sea) and len(sea) == len(have),
                                           "centroid": [round(cx, 1), round(cy, 1)],
                                           "aim": [round(cents[aim][0], 1), round(cents[aim][1], 1)]}
    provinces = {str(pid): {"xy": [round(c[0], 1), round(c[1], 1)], "pixels": c[2], "kind": kinds.get(pid),
                            "state": prov_state.get(pid)} for pid, c in cents.items()}
    return {"version": CACHE_VERSION, "size": [w, h], "states": {str(k): v for k, v in states.items()},
            "provinces": provinces, "buildings": buildings, "regions": regions}


def index(game_dir, mods=()):
    """The cached index for this game directory and set of state files."""
    mods = list(mods)
    paths = _state_files(game_dir, mods) + scriptdb.effective_files("map/strategicregions", game_dir, mods) + [os.path.join(game_dir, "map", "provinces.bmp"),
                                            os.path.join(game_dir, "map", "definition.csv")]
    sig = scriptdb.signature(paths, f"mapdata{CACHE_VERSION}")
    if _MEM.get("sig") == sig:
        return _MEM["data"]
    cache = os.path.join(scriptdb.cache_dir(), f"mapdata_{sig}.json")
    data = None
    try:
        with open(cache, encoding="utf-8") as f:
            data = json.load(f)
    except (OSError, ValueError):
        pass
    if not data or data.get("version") != CACHE_VERSION:
        data = _build(game_dir, mods)
        tmp = cache + ".tmp"
        with open(tmp, "w", encoding="utf-8") as f:
            json.dump(data, f)
        os.replace(tmp, cache)
    _MEM["sig"], _MEM["data"] = sig, data
    return data


def for_session(session):
    return index(session.cfg.game_dir, scriptdb.mods_for(session))


def state(idx, sid):
    return idx["states"].get(str(sid))


def province_state(idx, pid):
    p = idx["provinces"].get(str(pid))
    return p.get("state") if p else None


def point_of(idx, sid, what="anchor"):
    """Bitmap pixel (x, y) of a state: 'anchor' (the victory-point or largest province), 'centroid', or
    'air_base' / 'naval_base' when buildings.txt places one there (falls back to the anchor)."""
    st = state(idx, sid)
    if not st:
        return None
    if what in ("air_base", "naval_base", "supply_node"):
        b = (idx["buildings"].get(str(sid)) or {}).get(what)
        if b:
            return tuple(b)
        return tuple(st["anchor"])
    return tuple(st["centroid"] if what == "centroid" else st["anchor"])


def to_minimap_fraction(idx, xy):
    w, h = idx["size"]
    return xy[0] / w, xy[1] / h


def state_at(idx, xy):
    """The state whose anchor/centroid is closest to a bitmap pixel (used to judge where the camera is)."""
    best, bd = None, 1e18
    for k, st in idx["states"].items():
        for pt in (st["anchor"], st["centroid"]):
            d = (pt[0] - xy[0]) ** 2 + (pt[1] - xy[1]) ** 2
            if d < bd:
                best, bd = int(k), d
    return best, bd ** 0.5


def neighbours_by_distance(idx, sid, limit=8):
    st = state(idx, sid)
    if not st:
        return []
    ax, ay = st["anchor"]
    rows = sorted(((((v["anchor"][0] - ax) ** 2 + (v["anchor"][1] - ay) ** 2) ** 0.5, int(k))
                   for k, v in idx["states"].items() if k != str(sid)))
    return rows[:limit]
