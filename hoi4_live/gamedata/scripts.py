"""Light indexes of game script files (game + enabled mods, with file overrides): which slot an idea belongs to
(laws, advisors, national spirits), technology research costs, focus costs. Built with a brace-depth scanner (no
full parse), cached on disk keyed by the files' sizes and mtimes.
"""

import hashlib
import json
import os
import re
import tempfile

from .. import platform

from ..telemetry.modgen import effective_files

NAME = re.compile(rb"([A-Za-z0-9_.\-]+)\s*=\s*\{")


def _cache_dir():
    d = os.path.join(platform.current().local_data_dir(), "hoi4_live", "script_index")
    os.makedirs(d, exist_ok=True)
    return d


def _strip_comments(data):
    return re.sub(rb"#[^\n]*", b"", data)


def blocks_at_depth(data, depth):
    """Yield (path_tuple, start, end) for every `name = {` block whose depth is `depth` (0 = top level)."""
    data = _strip_comments(data)
    stack = []
    i = 0
    n = len(data)
    in_quote = False
    pending = []                                   # names waiting for their opening brace
    while i < n:
        c = data[i]
        if c == 34:                                # "
            in_quote = not in_quote
        elif not in_quote:
            if c == 123:                           # {
                m = None
                j = i - 1
                while j >= 0 and data[j] in b" \t\r\n":
                    j -= 1
                if j >= 0 and data[j] == 61:       # '='
                    k = j - 1
                    while k >= 0 and data[k] in b" \t\r\n":
                        k -= 1
                    e = k + 1
                    while k >= 0 and (chr(data[k]).isalnum() or data[k] in b"_.-:@"):
                        k -= 1
                    m = data[k + 1:e].decode("utf-8", "replace")
                stack.append((m or "-", i))
            elif c == 125:                         # }
                if stack:
                    name, start = stack.pop()
                    if len(stack) == depth:
                        yield tuple(s[0] for s in stack) + (name,), start, i + 1
        i += 1


def _signature(paths):
    h = hashlib.sha1()
    for p in paths:
        try:
            st = os.stat(p)
            h.update(f"{p}|{st.st_size}|{st.st_mtime}".encode())
        except OSError:
            pass
    return h.hexdigest()[:20]


def _cached(kind, paths, build):
    cp = os.path.join(_cache_dir(), f"{kind}_{_signature(paths)}.json")
    try:
        with open(cp, encoding="utf-8") as f:
            return json.load(f)
    except (OSError, ValueError):
        pass
    data = build(paths)
    try:
        with open(cp, "w", encoding="utf-8") as f:
            json.dump(data, f)
    except OSError:
        pass
    return data


def idea_slots(game_dir, mods):
    """{idea: category} for common/ideas (category = economy, trade_laws, mobilization_laws, country, ...)."""
    paths = effective_files("common/ideas", game_dir, mods)

    def build(paths):
        out = {}
        for p in paths:
            try:
                with open(p, "rb") as f:
                    data = f.read()
            except OSError:
                continue
            for path, a, b in blocks_at_depth(data, 2):
                if len(path) == 3 and path[0] == "ideas":
                    out[path[2]] = path[1]
        return out
    return _cached("ideas", paths, build)


def tech_costs(game_dir, mods):
    """{technology: research_cost} (the cost multiplier; base research time is about 100 days)."""
    paths = effective_files("common/technologies", game_dir, mods)
    rx = re.compile(rb"research_cost\s*=\s*([0-9.]+)")

    def build(paths):
        out = {}
        for p in paths:
            try:
                with open(p, "rb") as f:
                    data = f.read()
            except OSError:
                continue
            for path, a, b in blocks_at_depth(data, 1):
                if path[0] == "technologies":
                    m = rx.search(_strip_comments(data[a:b]))
                    if m:
                        try:
                            out[path[1]] = float(m.group(1))
                        except ValueError:
                            pass
        return out
    return _cached("techs", paths, build)


_CACHE = {}


def for_session(session, kind):
    from ..telemetry.modgen import mod_paths
    cfg = session.cfg
    mods = mod_paths(cfg.user_dir)
    key = (kind, cfg.game_dir, tuple(m["path"] for m in mods))
    if key not in _CACHE:
        fn = {"ideas": idea_slots, "techs": tech_costs}[kind]
        _CACHE[key] = fn(cfg.game_dir, mods)
    return _CACHE[key]
