"""Small pure file reads of the game's map data used by the findings.

state -> strategic air region (the region holding most of the state's
provinces), from map/strategicregions and history/states of a game (or mod)
directory. Cached per directory.
"""
from __future__ import annotations

import os
import re
from collections import Counter

_CACHE = {}

_ID = re.compile(r"\bid\s*=\s*(\d+)")
_PROVS = re.compile(r"\bprovinces\s*=\s*\{([^}]*)\}", re.S)
_NAME = re.compile(r"\bname\s*=\s*\"?([^\"\s}]+)")


def _blocks(folder):
    try:
        names = sorted(os.listdir(folder))
    except OSError:
        return
    for n in names:
        if not n.endswith(".txt"):
            continue
        try:
            with open(os.path.join(folder, n), encoding="utf-8-sig", errors="replace") as f:
                text = f.read()
        except OSError:
            continue
        text = re.sub(r"#[^\n]*", "", text)
        m, p = _ID.search(text), _PROVS.search(text)
        if m and p:
            yield int(m.group(1)), [int(x) for x in p.group(1).split() if x.isdigit()], text


def state_regions(game_dir):
    """state id -> strategic region id ({} when the files are not there)."""
    if not game_dir:
        return {}
    key = os.path.abspath(game_dir)
    if key in _CACHE:
        return _CACHE[key]
    region_of = {}
    for rid, provs, _ in _blocks(os.path.join(game_dir, "map", "strategicregions")):
        for p in provs:
            region_of[p] = rid
    out = {}
    if region_of:
        for sid, provs, _ in _blocks(os.path.join(game_dir, "history", "states")):
            c = Counter(region_of[p] for p in provs if p in region_of)
            if c:
                out[sid] = c.most_common(1)[0][0]
    _CACHE[key] = out
    return out


def game_dir_from_exe(exe):
    """The install directory from the game executable path (macOS bundle or plain)."""
    if not exe:
        return None
    d = os.path.dirname(exe)
    for _ in range(4):
        if os.path.isdir(os.path.join(d, "map")) and os.path.isdir(os.path.join(d, "history")):
            return d
        d = os.path.dirname(d)
    return None
