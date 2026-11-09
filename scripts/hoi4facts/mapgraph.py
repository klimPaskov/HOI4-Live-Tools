"""Province graph of a HOI4 install (plus mods): adjacency, river borders, terrain, states, supply, regions.

Sources (all resolved through ``content_path(relpath)`` so a mod layer can override any of them):

    map/definition.csv        id;r;g;b;type;coastal;terrain;continent
    map/provinces.bmp         24-bit BMP, one flat colour per province
    map/rivers.bmp            8-bit indexed BMP; palette index < 254 is river (0 source, 1 merge, 2 split,
                              3-11 river widths), 254 is land, 255 is sea
    map/adjacencies.csv       From;To;Type;Through;...;adjacency_rule_name;Comment  (sea = strait, impassable)
    map/supply_nodes.txt      "<level> <province>" per line
    map/railways.txt          "<level> <count> <province> ..." per line
    map/strategicregions/     strategic_region = { id name provinces = { ... } }
    history/states/           state = { id provinces history = { owner buildings = { infrastructure } } }
    common/terrain/           categories = { <terrain> = { movement_cost = x } }   (route weights)

Directories (strategicregions, history/states, common/terrain) are listed with ``content_files(reldir)`` when
the caller passes one (a mod layer returns the merged file list); otherwise the files in ``content_path(reldir)``.

API: ``load(game_dir=None, content_path=None, content_files=None, cache_dir=None)`` -> graph;
``province(graph, id)``, ``neighbours(graph, id)``, ``route(graph, src, dst, avoid_rivers=False, weights=None)``,
``path_crossings(graph, path)``, ``state_neighbours(graph, state_id)``. In the graph, ``adj[a][b]`` is the edge row
{kind, border, river, river_crossing, impassable?, through?, rule?}.

Adjacency: two provinces are neighbours when their pixels touch horizontally or vertically (4-neighbourhood,
like the Rust ``wars.rs::land_edges``). Each edge records ``border`` = the number of touching pixel pairs.

River rule (method R1): a pixel pair on the A|B border is a river pair when either of its two pixels is a river
pixel in rivers.bmp. ``river`` = number of river pairs on the border. An edge is a river crossing when
``river >= RIVER_MIN_PAIRS`` (1: any touch). This reproduces the italo-ethiopian-war.md section 7.1 table, where
several marked crossings (5097-13239, 2072-7944, 1984-13241) touch the river at a single pair. The raw counts
(``river`` and ``border``) are kept so a caller can apply a stricter threshold, e.g. a river that only ends at
or cuts a corner of a border gives one or two pairs, a river drawn along the border gives many. The engine's
own rule is not documented; this is the file-level approximation.

Adjacencies.csv: ``sea`` rows add a strait edge (``kind = "strait"``, ``through`` = sea province); ``impassable``
rows mark an existing edge impassable (route never uses it); rows with an empty type (rule-controlled sea
links such as GIBRALTAR_STRAIT) add a ``link`` edge. ``rule`` keeps the adjacency_rule_name.

The built graph is cached as gzip JSON keyed by a content signature (each source's relpath, resolved path, size
and mtime), so a mod or patch that changes any file rebuilds it.

CLI:
    python3 scripts/hoi4facts/mapgraph.py province <id>
    python3 scripts/hoi4facts/mapgraph.py route <from> <to> [--avoid-rivers]
"""

import argparse
import gzip
import hashlib
import heapq
import json
import os
import re
import sys

VERSION = 1
RIVER_MIN_PAIRS = 1
DEFAULT_GAME_DIR = os.path.expanduser("~/Library/Application Support/Steam/steamapps/common/Hearts of Iron IV")

# Route weights. Terrain costs come from common/terrain movement_cost when the files have them; these are
# only the fallback. Cost of entering a province = terrain * (1 + infra * (5 - infrastructure) / 5)
# plus `river` when the step crosses a river border, plus `strait` for a strait.
DEFAULT_WEIGHTS = {
    "terrain": {"plains": 1.0, "urban": 1.2, "forest": 1.5, "hills": 1.5, "jungle": 1.5, "mountain": 2.0,
                "marsh": 2.0, "desert": 1.05},
    "terrain_default": 1.0,
    "river": 1.0,
    "avoid_river": 1000.0,
    "infra": 0.5,
    "max_infra": 5,
    "strait": 3.0,
}

_MEM = {}


# ---------------------------------------------------------------- content resolution

def default_content(game_dir=None):
    """(content_path, content_files) for a plain install with no mods."""
    base = game_dir or os.environ.get("HOI4_DIR") or DEFAULT_GAME_DIR

    def content_path(relpath):
        return os.path.join(base, *relpath.split("/"))

    return content_path, None


def _files_in(content_path, content_files, reldir, ext=".txt"):
    if content_files is not None:
        files = list(content_files(reldir))
    else:
        d = content_path(reldir)
        try:
            files = [os.path.join(d, n) for n in sorted(os.listdir(d))]
        except OSError:
            files = []
    return [f for f in files if f.lower().endswith(ext) and os.path.isfile(f)]


def _sources(content_path, content_files):
    """[(label, path)] of every file the graph depends on."""
    out = []
    for rel in ("map/definition.csv", "map/provinces.bmp", "map/rivers.bmp", "map/adjacencies.csv",
                "map/supply_nodes.txt", "map/railways.txt"):
        out.append((rel, content_path(rel)))
    for reldir in ("map/strategicregions", "history/states", "common/terrain"):
        for p in _files_in(content_path, content_files, reldir):
            out.append((reldir + "/" + os.path.basename(p), p))
    return out


def signature(sources):
    h = hashlib.sha1(("mapgraph%d;%d" % (VERSION, RIVER_MIN_PAIRS)).encode())
    for label, path in sources:
        try:
            st = os.stat(path)
            h.update(("%s|%s|%d|%d\n" % (label, path, st.st_size, st.st_mtime_ns)).encode())
        except OSError:
            h.update(("%s|%s|missing\n" % (label, path)).encode())
    return h.hexdigest()[:20]


def default_cache_dir():
    if os.environ.get("HOI4FACTS_CACHE"):
        return os.environ["HOI4FACTS_CACHE"]
    if sys.platform == "darwin":
        return os.path.expanduser("~/Library/Caches/hoi4facts")
    if os.name == "nt":
        return os.path.join(os.environ.get("LOCALAPPDATA", os.path.expanduser("~")), "hoi4facts", "cache")
    return os.path.join(os.environ.get("XDG_CACHE_HOME", os.path.expanduser("~/.cache")), "hoi4facts")


# ---------------------------------------------------------------- tiny Clausewitz reader

_TOKEN = re.compile(r'"[^"]*"|[{}=<>]|[^\s{}=<>"]+')


def parse_script(text):
    """Nested [(key, value)] lists; bare values get key None. Comments (#) are stripped. Enough for map files."""
    text = re.sub(r"#[^\n]*", "", text)
    toks = _TOKEN.findall(text)
    pos = 0

    def block():
        nonlocal pos
        out = []
        while pos < len(toks):
            t = toks[pos]
            if t == "}":
                pos += 1
                return out
            if t == "{":
                pos += 1
                out.append((None, block()))
                continue
            if pos + 1 < len(toks) and toks[pos + 1] in ("=", "<", ">"):
                pos += 2
                if pos < len(toks) and toks[pos] == "{":
                    pos += 1
                    out.append((t, block()))
                elif pos < len(toks):
                    out.append((t, toks[pos].strip('"')))
                    pos += 1
                continue
            out.append((None, t.strip('"')))
            pos += 1
        return out

    return block()


def _get(items, key):
    for k, v in items:
        if k == key:
            return v
    return None


def _num(v, default=None):
    try:
        return float(v)
    except (TypeError, ValueError):
        return default


def _ints(items):
    return [int(v) for k, v in (items or []) if k is None and isinstance(v, str) and v.isdigit()]


# ---------------------------------------------------------------- BMP reading (stdlib only)

def _bmp(path):
    """(data, width, height, bpp, offset, stride, top_down, palette_bytes)."""
    with open(path, "rb") as f:
        data = f.read()
    if data[:2] != b"BM":
        raise ValueError("%s is not a BMP" % path)
    offset = int.from_bytes(data[10:14], "little")
    hsize = int.from_bytes(data[14:18], "little")
    w = int.from_bytes(data[18:22], "little", signed=True)
    h = int.from_bytes(data[22:26], "little", signed=True)
    bpp = int.from_bytes(data[28:30], "little")
    comp = int.from_bytes(data[30:34], "little")
    if comp != 0:
        raise ValueError("%s: compressed BMP is not supported" % path)
    top_down = h < 0
    h = abs(h)
    stride = ((w * bpp + 31) // 32) * 4
    if offset + stride * h > len(data):
        raise ValueError("%s is truncated" % path)
    palette = data[14 + hsize:offset]
    return data, w, h, bpp, offset, stride, top_down, palette


def _row(bmp, y):
    """Row y counted from the top, without padding."""
    data, w, h, bpp, offset, stride, top_down, _ = bmp
    r = y if top_down else h - 1 - y
    start = offset + r * stride
    return data[start:start + (w * bpp) // 8]


_RUN = re.compile(rb"(...)\1*", re.S)


# ---------------------------------------------------------------- build

def _read_definition(path):
    colours, provinces = {}, {}
    with open(path, encoding="utf-8-sig", errors="replace") as f:
        for line in f:
            parts = line.strip().split(";")
            if len(parts) < 5 or not parts[0].isdigit():
                continue
            pid = int(parts[0])
            if pid == 0:
                continue
            r, g, b = int(parts[1]), int(parts[2]), int(parts[3])
            colours[bytes((b, g, r))] = pid
            provinces[pid] = {"type": parts[4], "coastal": len(parts) > 5 and parts[5].strip().lower() == "true",
                              "terrain": parts[6] if len(parts) > 6 else "unknown",
                              "continent": int(parts[7]) if len(parts) > 7 and parts[7].strip().isdigit() else 0}
    return colours, provinces


def _scan_bitmaps(prov_path, river_path, colours):
    """Edges {(a, b): [border pairs, river pairs]} (a < b) and per-province [pixels, sum x, sum y]."""
    pb = _bmp(prov_path)
    if pb[3] != 24:
        raise ValueError("provinces.bmp must be 24 bpp")
    w, h = pb[1], pb[2]
    rb = None
    if river_path and os.path.exists(river_path):
        rb = _bmp(river_path)
        if rb[3] != 8 or rb[1] != w or rb[2] != h:
            rb = None  # unusable rivers map: no river flags rather than wrong ones
    is_river = bytes(1 if i < 254 else 0 for i in range(256))
    edges = {}
    stats = {}
    get = colours.get
    prev_runs = prev_mask = None
    zero_mask = bytes(w)
    for y in range(h):
        row = _row(pb, y)
        runs = []
        for m in _RUN.finditer(row):
            x0, x1 = m.start() // 3, m.end() // 3
            pid = get(m.group(1), 0)
            if runs and runs[-1][2] == pid:
                runs[-1][1] = x1
            else:
                runs.append([x0, x1, pid])
        mask = _row(rb, y).translate(is_river) if rb else zero_mask
        # province pixel stats (for centroids)
        for x0, x1, pid in runs:
            if pid:
                s = stats.get(pid)
                n = x1 - x0
                if s is None:
                    stats[pid] = [n, (x0 + x1 - 1) * n / 2.0, float(y * n)]
                else:
                    s[0] += n
                    s[1] += (x0 + x1 - 1) * n / 2.0
                    s[2] += y * n
        # horizontal contacts inside this row
        for i in range(1, len(runs)):
            a, b = runs[i - 1][2], runs[i][2]
            if a and b:
                x = runs[i][0]
                key = (a, b) if a < b else (b, a)
                e = edges.get(key)
                if e is None:
                    e = edges[key] = [0, 0]
                e[0] += 1
                if mask[x - 1] or mask[x]:
                    e[1] += 1
        # vertical contacts with the row above
        if prev_runs is not None:
            both = (int.from_bytes(mask, "big") | int.from_bytes(prev_mask, "big")).to_bytes(w, "big") \
                if rb else zero_mask
            i = j = 0
            while i < len(prev_runs) and j < len(runs):
                ax0, ax1, a = prev_runs[i]
                bx0, bx1, b = runs[j]
                s = ax0 if ax0 > bx0 else bx0
                t = ax1 if ax1 < bx1 else bx1
                if a != b and a and b and t > s:
                    key = (a, b) if a < b else (b, a)
                    e = edges.get(key)
                    if e is None:
                        e = edges[key] = [0, 0]
                    e[0] += t - s
                    if rb:
                        e[1] += both.count(1, s, t)
                if ax1 <= bx1:
                    i += 1
                if bx1 <= ax1:
                    j += 1
        prev_runs, prev_mask = runs, mask
    return edges, stats, (w, h), rb is not None


def _read_adjacencies(path):
    rows = []
    if not os.path.exists(path):
        return rows
    with open(path, encoding="utf-8-sig", errors="replace") as f:
        for line in f:
            parts = line.strip().split(";")
            if len(parts) < 3 or not parts[0].strip().lstrip("-").isdigit() or not parts[1].strip().isdigit():
                continue
            a, b = int(parts[0]), int(parts[1])
            if a < 0:
                continue
            through = int(parts[3]) if len(parts) > 3 and parts[3].strip().lstrip("-").isdigit() else -1
            rule = parts[8].strip() if len(parts) > 8 else ""
            comment = parts[9].strip() if len(parts) > 9 else ""
            rows.append({"from": a, "to": b, "type": parts[2].strip().lower(), "through": through,
                         "rule": rule, "comment": comment})
    return rows


def _read_supply_nodes(path):
    hubs = {}
    if os.path.exists(path):
        with open(path, encoding="utf-8-sig", errors="replace") as f:
            for line in f:
                parts = re.sub(r"#.*", "", line).split()
                if len(parts) >= 2 and parts[0].isdigit() and parts[1].isdigit():
                    hubs[int(parts[1])] = max(hubs.get(int(parts[1]), 0), int(parts[0]))
    return hubs


def _read_railways(path):
    rails = []
    if os.path.exists(path):
        with open(path, encoding="utf-8-sig", errors="replace") as f:
            for n, line in enumerate(f, 1):
                parts = re.sub(r"#.*", "", line).split()
                if len(parts) < 3 or not all(p.isdigit() for p in parts):
                    continue
                level, count = int(parts[0]), int(parts[1])
                provs = [int(p) for p in parts[2:2 + count]]
                rails.append({"level": level, "provinces": provs, "line": n})
    return rails


def _read_regions(files):
    regions = {}
    for p in files:
        with open(p, encoding="utf-8-sig", errors="replace") as f:
            items = parse_script(f.read())
        for k, v in items:
            if k != "strategic_region" or not isinstance(v, list):
                continue
            rid = _num(_get(v, "id"))
            if rid is None:
                continue
            regions[int(rid)] = {"id": int(rid), "name": _get(v, "name"), "provinces": _ints(_get(v, "provinces")),
                                 "file": os.path.basename(p)}
    return regions


def _read_states(files):
    """{state id: {...}}; the undated history block only (start-of-game owner and buildings)."""
    states = {}
    for p in files:
        with open(p, encoding="utf-8-sig", errors="replace") as f:
            items = parse_script(f.read())
        for k, v in items:
            if k != "state" or not isinstance(v, list):
                continue
            sid = _num(_get(v, "id"))
            if sid is None:
                continue
            hist = _get(v, "history") or []
            if not isinstance(hist, list):
                hist = []
            owner = _get(hist, "owner")
            infra, naval, vps = 0, {}, {}
            for hk, hv in hist:
                if hk == "buildings" and isinstance(hv, list):
                    for bk, bv in hv:
                        if bk == "infrastructure":
                            infra = int(_num(bv, 0))
                        elif bk and bk.isdigit() and isinstance(bv, list):
                            nb = _num(_get(bv, "naval_base"))
                            if nb:
                                naval[int(bk)] = int(nb)
                elif hk == "victory_points" and isinstance(hv, list):
                    vals = [x for kk, x in hv if kk is None]
                    if len(vals) >= 2 and vals[0].isdigit():
                        vps[int(vals[0])] = _num(vals[1], 0)
            states[int(sid)] = {"id": int(sid), "name": _get(v, "name"), "owner": owner,
                                "category": _get(v, "state_category"), "infrastructure": infra,
                                "provinces": _ints(_get(v, "provinces")), "naval_bases": naval,
                                "victory_points": vps, "file": os.path.basename(p)}
    return states


def _read_terrain_costs(files):
    costs, water = {}, set()
    for p in files:
        with open(p, encoding="utf-8-sig", errors="replace") as f:
            items = parse_script(f.read())
        cats = _get(items, "categories")
        if not isinstance(cats, list):
            continue
        for name, body in cats:
            if not name or not isinstance(body, list):
                continue
            mc = _num(_get(body, "movement_cost"))
            if mc is not None:
                costs[name] = mc
            if _get(body, "is_water") == "yes":
                water.add(name)
    return costs, sorted(water)


def build(content_path, content_files=None):
    """Build the graph dict (JSON-ready) from the game files."""
    colours, provinces = _read_definition(content_path("map/definition.csv"))
    edges, stats, size, have_rivers = _scan_bitmaps(content_path("map/provinces.bmp"),
                                                    content_path("map/rivers.bmp"), colours)
    for pid, (n, sx, sy) in stats.items():
        if pid in provinces:
            provinces[pid]["pixels"] = n
            provinces[pid]["xy"] = [round(sx / n, 1), round(sy / n, 1)]
    edge_rows = []
    index = {}
    for (a, b), (border, river) in sorted(edges.items()):
        row = {"a": a, "b": b, "kind": "border", "border": border, "river": river}
        index[(a, b)] = row
        edge_rows.append(row)
    adjacencies = _read_adjacencies(content_path("map/adjacencies.csv"))
    for adj in adjacencies:
        a, b = sorted((adj["from"], adj["to"]))
        row = index.get((a, b))
        if adj["type"] == "impassable":
            if row is not None:
                row["impassable"] = True
            continue
        kind = "strait" if adj["type"] == "sea" else (adj["type"] or "link")
        if row is None:
            row = {"a": a, "b": b, "kind": kind, "border": 0, "river": 0}
            index[(a, b)] = row
            edge_rows.append(row)
        else:
            row["adjacency"] = kind  # a pixel border stays a border; note the csv row on it
        if adj["through"] >= 0:
            row["through"] = adj["through"]
        if adj["rule"]:
            row["rule"] = adj["rule"]
    states = _read_states(_files_in(content_path, content_files, "history/states"))
    for sid, st in states.items():
        for pid in st["provinces"]:
            if pid in provinces:
                provinces[pid]["state"] = sid
    regions = _read_regions(_files_in(content_path, content_files, "map/strategicregions"))
    for rid, reg in regions.items():
        for pid in reg["provinces"]:
            if pid in provinces:
                provinces[pid]["region"] = rid
    hubs = _read_supply_nodes(content_path("map/supply_nodes.txt"))
    for pid, level in hubs.items():
        if pid in provinces:
            provinces[pid]["supply_hub"] = level
    railways = _read_railways(content_path("map/railways.txt"))
    for r in railways:
        for pid in r["provinces"]:
            if pid in provinces:
                provinces[pid]["railway"] = max(provinces[pid].get("railway", 0), r["level"])
    for st in states.values():
        for pid, lvl in st["naval_bases"].items():
            if pid in provinces:
                provinces[pid]["naval_base"] = lvl
    terrain_costs, water_terrain = _read_terrain_costs(_files_in(content_path, content_files, "common/terrain"))
    return {
        "version": VERSION,
        "method": {"adjacency": "4-neighbourhood pixel contact in provinces.bmp",
                   "river": "border pixel pairs where either pixel is river (rivers.bmp index < 254); "
                            "river crossing when river pairs >= %d" % RIVER_MIN_PAIRS,
                   "rivers_map": have_rivers},
        "river_min_pairs": RIVER_MIN_PAIRS,
        "size": list(size),
        "provinces": {str(k): v for k, v in provinces.items()},
        "edges": edge_rows,
        "states": {str(k): {kk: (({str(p): x for p, x in vv.items()}) if isinstance(vv, dict) else vv)
                            for kk, vv in v.items()} for k, v in states.items()},
        "regions": {str(k): v for k, v in regions.items()},
        "supply_hubs": {str(k): v for k, v in hubs.items()},
        "railways": railways,
        "terrain_costs": terrain_costs,
        "water_terrain": water_terrain,
    }


def _inflate(data):
    """Int-keyed in-memory view with an adjacency index ``adj[pid][nb] = edge row``."""
    g = dict(data)
    g["provinces"] = {int(k): v for k, v in data["provinces"].items()}
    g["states"] = {int(k): dict(v, naval_bases={int(p): x for p, x in v["naval_bases"].items()},
                                victory_points={int(p): x for p, x in v["victory_points"].items()})
                   for k, v in data["states"].items()}
    g["regions"] = {int(k): v for k, v in data["regions"].items()}
    g["supply_hubs"] = {int(k): v for k, v in data["supply_hubs"].items()}
    adj = {}
    mn = data.get("river_min_pairs", RIVER_MIN_PAIRS)
    for e in data["edges"]:
        e["river_crossing"] = e["river"] >= mn
        adj.setdefault(e["a"], {})[e["b"]] = e
        adj.setdefault(e["b"], {})[e["a"]] = e
    g["adj"] = adj
    return g


def load(game_dir=None, content_path=None, content_files=None, cache_dir=None, rebuild=False):
    """The graph for this content set: memory cache, then gzip JSON cache, then a fresh build."""
    if content_path is None:
        content_path, dflt_files = default_content(game_dir)
        content_files = content_files or dflt_files
    sources = _sources(content_path, content_files)
    sig = signature(sources)
    if not rebuild and _MEM.get("sig") == sig:
        return _MEM["graph"]
    cache_dir = cache_dir or default_cache_dir()
    cache = os.path.join(cache_dir, "mapgraph_%s.json.gz" % sig)
    data = None
    if not rebuild:
        try:
            with gzip.open(cache, "rt", encoding="utf-8") as f:
                data = json.load(f)
            if data.get("version") != VERSION:
                data = None
        except (OSError, ValueError, EOFError):
            data = None
    if data is None:
        data = build(content_path, content_files)
        data["signature"] = sig
        try:
            os.makedirs(cache_dir, exist_ok=True)
            tmp = cache + ".tmp%d" % os.getpid()
            with gzip.open(tmp, "wt", encoding="utf-8", compresslevel=6) as f:
                json.dump(data, f, separators=(",", ":"))
            os.replace(tmp, cache)
        except OSError:
            pass
    graph = _inflate(data)
    _MEM["sig"], _MEM["graph"] = sig, graph
    return graph


# ---------------------------------------------------------------- queries

def is_land(graph, pid):
    p = graph["provinces"].get(pid)
    return bool(p) and p["type"] == "land"


def neighbours(graph, pid, land_only=True, passable_only=True):
    """[(neighbour, edge)] sorted by id."""
    out = []
    for nb, e in sorted(graph["adj"].get(pid, {}).items()):
        if land_only and not is_land(graph, nb):
            continue
        if passable_only and e.get("impassable"):
            continue
        out.append((nb, e))
    return out


def province(graph, pid):
    """Everything known about one province, with its neighbours and their river flags."""
    p = graph["provinces"].get(pid)
    if p is None:
        return None
    st = graph["states"].get(p.get("state"))
    out = dict(p, id=pid)
    if st:
        out["owner"] = st["owner"]
        out["infrastructure"] = st["infrastructure"]
        if pid in st["victory_points"]:
            out["victory_points"] = st["victory_points"][pid]
    out["neighbours"] = [
        {"id": nb, "type": graph["provinces"][nb]["type"], "state": graph["provinces"][nb].get("state"),
         "terrain": graph["provinces"][nb]["terrain"], "kind": e["kind"], "river": e["river_crossing"],
         "river_pairs": e["river"], "border_pairs": e["border"],
         **({"impassable": True} if e.get("impassable") else {}),
         **({"through": e["through"]} if "through" in e else {})}
        for nb, e in sorted(graph["adj"].get(pid, {}).items()) if nb in graph["provinces"]]
    return out


def _weights(graph, weights):
    """DEFAULT_WEIGHTS, then terrain movement costs from the game files, then the caller's overrides."""
    w = {k: (dict(v) if isinstance(v, dict) else v) for k, v in DEFAULT_WEIGHTS.items()}
    w["terrain"].update(graph.get("terrain_costs") or {})
    for k, v in (weights or {}).items():
        if k == "terrain" and isinstance(v, dict):
            w["terrain"].update(v)
        else:
            w[k] = v
    return w


def step_cost(graph, a, b, edge, w, avoid_rivers=False):
    """Cost of moving a -> b over edge with resolved weights w (see route)."""
    p = graph["provinces"][b]
    terrain = w["terrain"].get(p["terrain"], w["terrain_default"])
    st = graph["states"].get(p.get("state"))
    infra = st["infrastructure"] if st else 0
    mx = float(w["max_infra"]) or 1.0
    cost = terrain * (1.0 + w["infra"] * max(0.0, mx - infra) / mx)
    if edge.get("river_crossing"):
        cost += w["avoid_river"] if avoid_rivers else w["river"]
    if edge["kind"] == "strait":
        cost += w["strait"]
    return cost


def route(graph, src, dst, avoid_rivers=False, weights=None, land_only=True):
    """Cheapest province path src -> dst (Dijkstra).

    Cost of each step = cost of entering the next province: terrain movement cost (common/terrain, else
    DEFAULT_WEIGHTS) x (1 + infra * (max_infra - state infrastructure) / max_infra), plus `river` for a river
    crossing (`avoid_river` instead when avoid_rivers, so a dry path wins whenever one exists), plus `strait`.
    Returns {path, cost, steps, river_crossings: [[a, b], ...], rivers} or None when unreachable.
    """
    w = _weights(graph, weights)
    provs = graph["provinces"]
    if src not in provs or dst not in provs:
        return None
    dist = {src: 0.0}
    prev = {}
    heap = [(0.0, src)]
    adj = graph["adj"]
    while heap:
        d, u = heapq.heappop(heap)
        if u == dst:
            break
        if d > dist.get(u, float("inf")):
            continue
        for v, e in adj.get(u, {}).items():
            if e.get("impassable") or v not in provs:
                continue
            if land_only and provs[v]["type"] != "land":
                continue
            nd = d + step_cost(graph, u, v, e, w, avoid_rivers)
            if nd < dist.get(v, float("inf")):
                dist[v] = nd
                prev[v] = u
                heapq.heappush(heap, (nd, v))
    if dst not in dist:
        return None
    path = [dst]
    while path[-1] != src:
        path.append(prev[path[-1]])
    path.reverse()
    crossings = path_crossings(graph, path)
    # report the real cost with the plain river weight, even when avoid_rivers steered the search
    cost = sum(step_cost(graph, a, b, adj[a][b], w, False) for a, b in zip(path, path[1:]))
    return {"path": path, "cost": round(cost, 3), "steps": len(path) - 1, "river_crossings": crossings,
            "rivers": len(crossings), "avoid_rivers": avoid_rivers}


def path_crossings(graph, path):
    """[[a, b], ...] river crossings along a given province path; KeyError if two steps are not adjacent."""
    return [[a, b] for a, b in zip(path, path[1:]) if graph["adj"][a][b].get("river_crossing")]


def terrain_mix(graph, provinces):
    """{"provinces": n, "counts": {terrain: n}, "shares": {terrain: share}} of the land provinces given
    (definition.csv terrain column; unknown ids and sea provinces are skipped)."""
    counts = {}
    n = 0
    for pid in dict.fromkeys(provinces):
        p = graph["provinces"].get(pid)
        if not p or p.get("type") != "land":
            continue
        counts[p["terrain"]] = counts.get(p["terrain"], 0) + 1
        n += 1
    shares = {t: round(c / n, 3) for t, c in sorted(counts.items(), key=lambda kv: (-kv[1], kv[0]))} if n else {}
    return {"provinces": n, "counts": dict(sorted(counts.items(), key=lambda kv: (-kv[1], kv[0]))), "shares": shares}


def land_reach(graph, sources, allowed=None):
    """Set of land provinces reachable from `sources` over passable land borders and straits
    (adjacencies.csv `sea` rows between land provinces), entering only provinces where `allowed(pid)`."""
    provs = graph["provinces"]
    seen = {s for s in sources if s in provs and provs[s].get("type") == "land"}
    stack = list(seen)
    while stack:
        u = stack.pop()
        for v, e in graph["adj"].get(u, {}).items():
            if v in seen or e.get("impassable") or provs.get(v, {}).get("type") != "land":
                continue
            if allowed is not None and not allowed(v):
                continue
            seen.add(v)
            stack.append(v)
    return seen


def sea_neighbours(graph, pid):
    """Sea provinces touching a (coastal land) province."""
    return sorted(nb for nb in graph["adj"].get(pid, {}) if graph["provinces"].get(nb, {}).get("type") == "sea")


def _xy_dist(graph, a, b):
    pa, pb = graph["provinces"][a].get("xy"), graph["provinces"][b].get("xy")
    if not pa or not pb:
        return 1.0
    return ((pa[0] - pb[0]) ** 2 + (pa[1] - pb[1]) ** 2) ** 0.5


def sea_route(graph, sources, targets, avoid_rules=()):
    """Shortest sea path between two sets of sea provinces (Dijkstra over sea-to-sea edges: pixel borders,
    straits and canal links of adjacencies.csv; cost = centroid distance in map pixels). Edges whose
    adjacency rule is in `avoid_rules` are not used. Returns {path, steps, pixels, rules: [{rule, a, b}]}
    or None."""
    provs = graph["provinces"]
    targets = set(targets)
    avoid = set(avoid_rules)
    dist, prev, heap = {}, {}, []
    for s in sources:
        if provs.get(s, {}).get("type") == "sea":
            dist[s] = 0.0
            heapq.heappush(heap, (0.0, s))
    end = None
    while heap:
        d, u = heapq.heappop(heap)
        if d > dist.get(u, float("inf")):
            continue
        if u in targets:
            end = u
            break
        for v, e in graph["adj"].get(u, {}).items():
            if provs.get(v, {}).get("type") != "sea" or e.get("impassable") or e.get("rule") in avoid:
                continue
            nd = d + _xy_dist(graph, u, v)
            if nd < dist.get(v, float("inf")):
                dist[v] = nd
                prev[v] = u
                heapq.heappush(heap, (nd, v))
    if end is None:
        return None
    path = [end]
    while path[-1] in prev:
        path.append(prev[path[-1]])
    path.reverse()
    rules = [{"rule": graph["adj"][a][b]["rule"], "a": a, "b": b} for a, b in zip(path, path[1:])
             if graph["adj"][a][b].get("rule")]
    return {"path": path, "steps": len(path) - 1, "pixels": round(dist[end]), "rules": rules}


def state_neighbours(graph, sid, land_only=True):
    """Sorted ids of states that share a passable border with state sid."""
    st = graph["states"].get(sid)
    out = set()
    for pid in (st or {}).get("provinces", []):
        for nb, _ in neighbours(graph, pid, land_only=land_only):
            other = graph["provinces"][nb].get("state")
            if other and other != sid:
                out.add(other)
    return sorted(out)


# ---------------------------------------------------------------- CLI

def _main(argv=None):
    ap = argparse.ArgumentParser(description="HOI4 province graph: neighbours, rivers and routes")
    ap.add_argument("--game-dir", default=os.environ.get("HOI4_DIR", DEFAULT_GAME_DIR))
    ap.add_argument("--rebuild", action="store_true")
    sub = ap.add_subparsers(dest="cmd", required=True)
    p1 = sub.add_parser("province")
    p1.add_argument("id", type=int)
    p2 = sub.add_parser("route")
    p2.add_argument("src", type=int)
    p2.add_argument("dst", type=int)
    p2.add_argument("--avoid-rivers", action="store_true")
    args = ap.parse_args(argv)
    g = load(game_dir=args.game_dir, rebuild=args.rebuild)
    if args.cmd == "province":
        out = province(g, args.id)
        if out is None:
            print(json.dumps({"error": "unknown province %d" % args.id}))
            return 1
    else:
        out = route(g, args.src, args.dst, avoid_rivers=args.avoid_rivers)
        if out is None:
            print(json.dumps({"error": "no route %d -> %d" % (args.src, args.dst)}))
            return 1
        out["provinces"] = [{"id": p, "terrain": g["provinces"][p]["terrain"],
                             "state": g["provinces"][p].get("state")} for p in out["path"]]
    print(json.dumps(out, indent=1))
    return 0


if __name__ == "__main__":
    sys.exit(_main())
