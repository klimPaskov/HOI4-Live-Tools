"""Game script files as data: the effective files of a folder (game + enabled mods, with file overrides and
replace_path), parsed with the save parser, `@constants` resolved, and compact per-kind indexes cached on disk.

The campaign tools read structure from these files (focus trees, technologies, decisions, laws, advisors,
buildings, equipment) and use the screen only to locate and confirm. Parsing every national focus file of the game
plus a large mod (~20 MB) takes several seconds, so each kind builds a small index once per file set (keyed by the
files' paths, sizes and mtimes) and keeps it in `%LOCALAPPDATA%/hoi4_live/script_index/`.
"""

import hashlib
import json
import os
import re
import tempfile

from .. import platform

from ..state.savefile import parse_block

INDEX_VERSION = 6


def cache_dir():
    d = os.path.join(platform.current().local_data_dir(), "hoi4_live", "script_index")
    os.makedirs(d, exist_ok=True)
    return d


def roots(game_dir, mods):
    """[{path, replace_path}] in load order: the game first, then the mods."""
    return [{"path": game_dir, "replace_path": []}] + [m for m in mods if m.get("path")]


def effective_files(folder, game_dir, mods, ext=(".txt",), recursive=False):
    """Files of `folder` after overrides: a mod file with the same relative path replaces the earlier one, a mod's
    replace_path drops everything earlier in that folder. Sorted by relative path (the game's load order)."""
    folder = folder.strip("/").replace("\\", "/")
    files = {}
    for root in roots(game_dir, mods):
        if folder in root.get("replace_path", []):
            files = {}
        base = os.path.join(root["path"], *folder.split("/"))
        if not os.path.isdir(base):
            continue
        if recursive:
            for dirpath, dirs, names in os.walk(base):
                dirs.sort()
                for n in sorted(names):
                    if n.lower().endswith(tuple(ext)):
                        rel = os.path.relpath(os.path.join(dirpath, n), base).replace("\\", "/")
                        files[rel.lower()] = os.path.join(dirpath, n)
        else:
            for n in sorted(os.listdir(base)):
                p = os.path.join(base, n)
                if n.lower().endswith(tuple(ext)) and os.path.isfile(p):
                    files[n.lower()] = p
    return [files[k] for k in sorted(files)]


def signature(paths, extra=""):
    h = hashlib.sha1(f"v{INDEX_VERSION}|{extra}".encode())
    for p in paths:
        try:
            st = os.stat(p)
            h.update(f"{p}|{st.st_size}|{st.st_mtime}".encode())
        except OSError:
            pass
    return h.hexdigest()[:20]


def read_bytes(path):
    with open(path, "rb") as f:
        data = f.read()
    if data.startswith(b"\xef\xbb\xbf"):
        data = data[3:]
    return data


CONST_DEF = re.compile(rb"^\s*(@[A-Za-z0-9_]+)\s*=\s*(-?[0-9.]+|[A-Za-z0-9_\"]+)", re.M)


def parse_file(path):
    """Parse one script file; `@name = value` constants of the file are substituted in its values."""
    data = read_bytes(path)
    consts = {m.group(1).decode(): _scalar(m.group(2).decode().strip('"')) for m in CONST_DEF.finditer(data)}
    block = parse_block(data)
    if consts:
        block = substitute(block, consts)
    return block


def _scalar(text):
    try:
        return int(text)
    except ValueError:
        try:
            return float(text)
        except ValueError:
            return text


def substitute(value, consts):
    if isinstance(value, dict):
        return {k: substitute(v, consts) for k, v in value.items() if not k.startswith("@")}
    if isinstance(value, list):
        return [substitute(v, consts) for v in value]
    if isinstance(value, str) and value.startswith("@"):
        return consts.get(value, value)
    return value


def as_list(v):
    if v is None:
        return []
    return v if isinstance(v, list) else [v]


def num(v, default=None):
    if isinstance(v, (int, float)):
        return v
    try:
        return float(v)
    except (TypeError, ValueError):
        return default


def render(value, limit=400, depth=4):
    """Compact one-line rendering of a parsed script block: 'a = 1; OR { b = yes; c = no }'."""
    out = _render(value, depth)
    out = re.sub(r"\s+", " ", out).strip()
    return out if len(out) <= limit else out[:limit - 3].rstrip() + "..."


def _render(value, depth):
    if isinstance(value, dict):
        rep = set(value.get("_repeated", ()))
        parts = []
        for k, v in value.items():
            if k in ("_repeated",):
                continue
            if k == "_items":
                parts.extend(_render(x, depth - 1) for x in as_list(v))
                continue
            vals = v if (k in rep and isinstance(v, list)) else [v]
            for x in vals:
                if isinstance(x, dict) and "op" in x and "value" in x and len(x) == 2:
                    parts.append(f"{k} {x['op']} {x['value']}")
                elif isinstance(x, (dict, list)):
                    parts.append(f"{k} {{ {_render(x, depth - 1)} }}" if depth > 0 else f"{k} {{ ... }}")
                else:
                    parts.append(f"{k} = {x}")
        return "; ".join(parts)
    if isinstance(value, list):
        return " ".join(_render(x, depth - 1) if isinstance(x, (dict, list)) else str(x) for x in value)
    return str(value)


def cached_index(kind, paths, build, extra=""):
    """Load the index of `kind` for this file set from disk, or build(paths) and store it."""
    sig = signature(paths, extra)
    cp = os.path.join(cache_dir(), f"{kind}_{sig}.json")
    try:
        with open(cp, encoding="utf-8") as f:
            return json.load(f)
    except (OSError, ValueError):
        pass
    data = build(paths)
    try:
        with open(cp + ".tmp", "w", encoding="utf-8") as f:
            json.dump(data, f, ensure_ascii=False)
        os.replace(cp + ".tmp", cp)
    except OSError:
        pass
    _prune(kind)
    return data


def _prune(kind, keep=3):
    d = cache_dir()
    try:
        files = sorted((os.path.join(d, f) for f in os.listdir(d) if f.startswith(kind + "_")),
                       key=os.path.getmtime, reverse=True)
    except OSError:
        return
    for p in files[keep:]:
        try:
            os.remove(p)
        except OSError:
            pass


def mods_for(session):
    from ..telemetry.modgen import mod_paths
    return mod_paths(session.cfg.user_dir)


_MEMO = {}


def memo(session, kind, fn):
    """One in-memory copy of an index per (kind, game, playset) for the server process."""
    mods = mods_for(session)
    key = (kind, session.cfg.game_dir, tuple(m["path"] for m in mods))
    if key not in _MEMO:
        for k in [k for k in _MEMO if k[0] == kind]:
            del _MEMO[k]
        _MEMO[key] = fn(session.cfg.game_dir, mods)
    return _MEMO[key]
