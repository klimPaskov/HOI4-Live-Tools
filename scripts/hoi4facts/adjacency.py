"""Province adjacency from the map files, stdlib only (a small reader for plan checks).

    map/definition.csv    province id;r;g;b;type;coastal;terrain;continent
    map/provinces.bmp     one flat colour per province, 24-bit BMP
    map/adjacencies.csv   From;To;Type;...  extra links (straits); type "impassable" removes one

Two provinces are adjacent when their pixels touch (4-neighbourhood), as in
hoi4_live/gamedata/mapdata.py `_province_edges`, plus the non-impassable rows of
adjacencies.csv. Rows of the bitmap are run-length encoded with a regular expression, so a full
5632 x 2048 map takes about two seconds; the result is cached in the temp folder, keyed by the
three files' paths, sizes and mtimes. (The full map graph with rivers and terrain is a separate
module; this one only answers "do these provinces touch".)
"""
import hashlib
import json
import os
import re
import struct
import tempfile

CACHE_VERSION = 1
_RUN = re.compile(b"(?s)(...)\\1*")
_MEM = {}


def _colours(path):
    out = {}
    with open(path, encoding="utf-8-sig", errors="replace") as fh:
        for line in fh:
            parts = line.strip().split(";")
            if len(parts) < 4 or not parts[0].isdigit() or parts[0] == "0":
                continue
            try:
                # BMP stores blue, green, red
                out[bytes((int(parts[3]), int(parts[2]), int(parts[1])))] = int(parts[0])
            except ValueError:
                continue
    return out


def _runs(row, colours):
    """[(start, end, province)] of one pixel row (byte offsets / 3)."""
    out = []
    for m in _RUN.finditer(row):
        out.append((m.start() // 3, m.end() // 3, colours.get(m.group(1), 0)))
    return out


def bitmap_edges(bmp_path, colours):
    """{(a, b)} with a < b for provinces whose pixels touch."""
    with open(bmp_path, "rb") as fh:
        data = fh.read()
    if data[:2] != b"BM":
        raise ValueError("not a BMP file")
    offset = struct.unpack_from("<I", data, 10)[0]
    width, height = struct.unpack_from("<ii", data, 18)
    bpp = struct.unpack_from("<H", data, 28)[0]
    if bpp != 24:
        raise ValueError(f"provinces.bmp is {bpp}-bit; only 24-bit is read")
    stride = (width * 3 + 3) & ~3
    rows = abs(height)
    edges = set()
    prev = None
    for y in range(rows):
        start = offset + y * stride
        runs = _runs(data[start:start + width * 3], colours)
        for (_, _, a), (_, _, b) in zip(runs, runs[1:]):
            if a and b and a != b:
                edges.add((a, b) if a < b else (b, a))
        if prev is not None:
            i = j = 0
            while i < len(prev) and j < len(runs):
                s1, e1, a = prev[i]
                s2, e2, b = runs[j]
                if a and b and a != b and s1 < e2 and s2 < e1:
                    edges.add((a, b) if a < b else (b, a))
                if e1 <= e2:
                    i += 1
                else:
                    j += 1
        prev = runs
    return edges


def _csv_links(path):
    add, remove = set(), set()
    if not path or not os.path.exists(path):
        return add, remove
    with open(path, encoding="utf-8-sig", errors="replace") as fh:
        for line in fh:
            parts = line.strip().split(";")
            if len(parts) < 3 or not parts[0].isdigit() or not parts[1].isdigit():
                continue
            a, b = int(parts[0]), int(parts[1])
            if a <= 0 or b <= 0 or a == b:
                continue
            pair = (a, b) if a < b else (b, a)
            (remove if parts[2].strip().lower() == "impassable" else add).add(pair)
    return add, remove


def _signature(paths):
    h = hashlib.sha1(f"v{CACHE_VERSION}".encode())
    for p in paths:
        try:
            st = os.stat(p)
            h.update(f"{p}|{st.st_size}|{st.st_mtime}".encode())
        except (OSError, TypeError):
            h.update(b"-")
    return h.hexdigest()[:20]


def neighbours(content, straits=True, cache_dir=None):
    """{province: set(provinces)} for the effective map of a Content, or None when the map files
    are missing or unreadable. straits=False leaves out the adjacencies.csv links (a railway
    needs provinces that touch); impassable rows are always removed."""
    definition = content.file("map/definition.csv")
    bmp = content.file("map/provinces.bmp")
    adj = content.file("map/adjacencies.csv")
    if not definition or not bmp:
        return None
    sig = _signature([definition, bmp, adj])
    key = (sig, straits)
    if key in _MEM:
        return _MEM[key]
    cache_dir = cache_dir or os.path.join(tempfile.gettempdir(), "hoi4facts")
    cp = os.path.join(cache_dir, f"adjacency_{sig}.json")
    edges = None
    try:
        with open(cp, encoding="utf-8") as fh:
            edges = {tuple(e) for e in json.load(fh)}
    except (OSError, ValueError):
        pass
    if edges is None:
        try:
            edges = bitmap_edges(bmp, _colours(definition))
        except (OSError, ValueError, struct.error):
            return None
        try:
            os.makedirs(cache_dir, exist_ok=True)
            with open(cp + ".tmp", "w", encoding="utf-8") as fh:
                json.dump(sorted(edges), fh)
            os.replace(cp + ".tmp", cp)
        except OSError:
            pass
    add, remove = _csv_links(adj)
    edges = ((edges | add) if straits else set(edges)) - remove
    out = {}
    for a, b in edges:
        out.setdefault(a, set()).add(b)
        out.setdefault(b, set()).add(a)
    _MEM[key] = out
    return out
