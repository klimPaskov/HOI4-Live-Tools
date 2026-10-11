"""High-level save queries used by hoi4_save_query (and later by hoi4_state in phase 2).

SaveFile objects are kept in a tiny cache (two entries, keyed by path, size and mtime) within one tool call. Call
close_all() when the call ends: a memory-mapped save cannot be overwritten by the game (autosaves, re-saves), so
no mapping may outlive the query. The structural index is cached on disk, so reopening is cheap.
"""

import os
import re

from ..errors import ToolError
from .savefile import SaveFile, clean, flatten

_CACHE = []
_CACHE_SIZE = 2


def open_save(path):
    st = os.stat(path)
    key = (os.path.abspath(path), st.st_size, st.st_mtime)
    for k, sf in _CACHE:
        if k == key:
            return sf
    sf = SaveFile(path)
    _CACHE.append((key, sf))
    while len(_CACHE) > _CACHE_SIZE:
        _, old = _CACHE.pop(0)
        old.close()
    return sf


def close_all():
    while _CACHE:
        _, sf = _CACHE.pop()
        sf.close()


def resolve_save(saves_dir, name):
    """'latest' | name | name.hoi4 | absolute path -> existing file path."""
    if name == "latest":
        files = [os.path.join(saves_dir, f) for f in os.listdir(saves_dir) if f.lower().endswith(".hoi4")]
        if not files:
            raise ToolError("no saves found")
        return max(files, key=os.path.getmtime)
    if os.path.isabs(name) and os.path.exists(name):
        return name
    base = name if name.lower().endswith(".hoi4") else name + ".hoi4"
    p = os.path.join(saves_dir, base)
    if not os.path.exists(p):
        raise ToolError(f"save not found: {p}", {"field": "save"})
    return p


def info(path):
    sf = open_save(path)
    h = sf.header()
    h["top_level_blocks"] = len([k for k in sf.index() if len(k) == 1])
    return {"info": h}


def keys(path, sub):
    sf = open_save(path)
    return {"path": sub, "keys": sf.keys(sub)}


def get(path, sub, limit=200):
    sf = open_save(path)
    got = sf.get(sub)
    out = {}
    total = 0
    for k, v in got.items():
        v = clean(v)
        leaves = flatten(v)
        total += len(leaves)
        out[k] = v if len(leaves) <= limit else {"_truncated": True, "leaves": dict(list(leaves.items())[:limit])}
    return {"path": sub, "matches": len(out), "leaves": total, "values": out}


def find(path, pattern, inside=None, limit=100):
    sf = open_save(path)
    try:
        hits = sf.find(pattern, inside=inside, limit=limit)
    except re.error as e:
        raise ToolError(f"bad regex: {e}", {"field": "pattern"})
    return {"pattern": pattern, "inside": inside, "hits": hits, "count": len(hits)}


def script_state(path, kind, tag=None, pattern=None, limit=200):
    """Variables or flags of a country (or the global ones)."""
    sf = open_save(path)
    sub = f"countries/{tag}/{kind}" if tag else kind
    got = sf.get(sub)
    if not got:
        raise ToolError(f"no '{sub}' block in this save", {"field": "tag" if tag else "query"})
    value = clean(next(iter(got.values())))
    rx = re.compile(pattern, re.I) if pattern else None
    items = {}
    if isinstance(value, dict):
        for k, v in value.items():
            if rx and not rx.search(k):
                continue
            if kind == "flags" and isinstance(v, dict):
                v = {kk: vv for kk, vv in v.items() if kk in ("value", "date")}
            items[k] = v
    shown = dict(list(items.items())[:limit])
    return {"scope": tag or "global", "kind": kind, "count": len(items), "shown": len(shown), "values": shown}


def diff(old_path, new_path, sub, pattern=None, limit=200):
    a, b = open_save(old_path), open_save(new_path)
    rx = re.compile(pattern, re.I) if pattern else None

    def leaves(sf):
        out = {}
        for k, v in sf.get(sub).items():
            for lk, lv in flatten(clean(v), k).items():
                if rx is None or rx.search(lk):
                    out[lk] = lv
        return out
    if not sub:
        raise ToolError("diff needs path (e.g. countries/USA)", {"field": "path"})
    la, lb = leaves(a), leaves(b)
    added = sorted(set(lb) - set(la))
    removed = sorted(set(la) - set(lb))
    changed = sorted(k for k in set(la) & set(lb) if la[k] != lb[k])
    return {"path": sub, "added": {k: lb[k] for k in added[:limit]}, "removed": {k: la[k] for k in removed[:limit]},
            "changed": {k: [la[k], lb[k]] for k in changed[:limit]},
            "counts": {"added": len(added), "removed": len(removed), "changed": len(changed)}}
