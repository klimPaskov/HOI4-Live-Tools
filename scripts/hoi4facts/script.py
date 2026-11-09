"""Clausewitz script reader for definitions, and the static part of the scripting language.

Moved from scripts/validate_plan.py (it was the stdlib reader there). A parsed block is a list of
(key, value) pairs; value is a string or a nested block (list). Bare values inside a block are
stored as (None, token). Comments are dropped. It is enough for definition files (technologies,
focus trees, history, events, doctrines, OOBs); it does not run scripts.

Triggers are evaluated tri-state (True, False, None = unknown at load time) for the conditions
that are static at the start of a game: has_dlc, tag, original_tag, always, AND/OR/NOT.
`run_effects` walks an effect block taking only the if/else_if/else branches that apply.
`history_walk` additionally applies dated blocks (`1936.6.1 = { ... }`) up to a start date, in
date order, after the undated part, as the game does when a bookmark starts.

Stdlib only, Python 3.9+.
"""
import re

_TOKEN = re.compile(r'"[^"]*"|[{}]|[<>=]=?|[^\s{}=<>"#]+|#[^\n]*')
_OPS = ("=", "<", ">", "<=", ">=")


def tokenize(text):
    out = []
    for m in _TOKEN.finditer(text):
        t = m.group(0)
        if t.startswith("#"):
            continue
        out.append(t)
    return out


def parse(text):
    """Return a block: list of (key, value) where value is str or block (list).
    Bare values inside a block are stored as (None, token)."""
    toks = tokenize(text.lstrip("﻿"))
    pos = 0

    def block(top):
        nonlocal pos
        items = []
        while pos < len(toks):
            t = toks[pos]
            if t == "}":
                pos += 1
                if top:
                    continue  # tolerate a stray brace at top level
                return items
            pos += 1
            if t in _OPS or t == "{":
                if t == "{":  # anonymous nested block
                    items.append((None, block(False)))
                continue
            if pos < len(toks) and toks[pos] in _OPS:
                pos += 1
                if pos < len(toks) and toks[pos] == "{":
                    pos += 1
                    items.append((t.strip('"'), block(False)))
                elif pos < len(toks):
                    items.append((t.strip('"'), toks[pos].strip('"')))
                    pos += 1
            else:
                items.append((None, t.strip('"')))
        return items

    return block(True)


def get(items, key):
    return [v for k, v in items if k == key]


def get1(items, key, default=None):
    for k, v in items:
        if k == key:
            return v
    return default


def bare(items):
    """The bare values of a block ({ 2072 13237 13238 } -> ["2072", "13237", "13238"])."""
    return [v for k, v in items if k is None and isinstance(v, str)] if isinstance(items, list) else []


def read(path):
    with open(path, encoding="utf-8-sig", errors="replace") as fh:
        return fh.read()


_PARSED = {}


def parse_file(path):
    """Parsed file, memoised per (path, size, mtime) for the process."""
    import os

    try:
        st = os.stat(path)
        key = (path, st.st_size, st.st_mtime)
    except OSError:
        return []
    hit = _PARSED.get(path)
    if hit and hit[0] == key:
        return hit[1]
    items = parse(read(path))
    _PARSED[path] = (key, items)
    return items


# --------------------------------------------------------------------------
# Static trigger evaluation (has_dlc, tag, original_tag, always, AND/OR/NOT).


class Ctx:
    """Evaluation context: the plan's tag and its ACTIVE DLC display names."""

    def __init__(self, tag, dlcs):
        self.tag = tag
        self.dlcs = set(dlcs)


def _and(results):
    results = list(results)
    if any(r is False for r in results):
        return False
    return None if any(r is None for r in results) else True


def _or(results):
    results = list(results)
    if any(r is True for r in results):
        return True
    return None if any(r is None for r in results) else False


def trigger(items, ctx):
    """Evaluate a trigger block (implicit AND)."""
    if not isinstance(items, list):
        return None
    return _and(trigger_one(k, v, ctx) for k, v in items)


def trigger_one(key, value, ctx):
    k = (key or "").lower()
    if ctx is None:
        return None
    if k == "has_dlc":
        return value in ctx.dlcs
    if k in ("tag", "original_tag"):
        return value == ctx.tag
    if k == "always":
        return value == "yes"
    if k == "and":
        return trigger(value, ctx)
    if k == "or":
        if not isinstance(value, list):
            return None
        return _or(trigger_one(a, b, ctx) for a, b in value)
    if k == "not":  # HOI4 NOT is NOR
        if not isinstance(value, list):
            return None
        r = _or(trigger_one(a, b, ctx) for a, b in value)
        return None if r is None else not r
    return None


def run_effects(items, ctx, visit):
    """Walk an effect block, taking only the if/else_if/else branches that apply
    under ctx (an unknown limit counts as true). Calls visit(key, value) for
    every other entry; nested plain blocks are not entered."""
    taken = None  # state of the sibling if-chain: None (no chain), True, False

    def branch(body):
        cond = trigger(get1(body, "limit", []), ctx)
        if cond is False:
            return False
        inner = [(a, b) for a, b in body if (a or "").lower() not in ("limit", "else", "else_if")]
        run_effects(inner, ctx, visit)
        return True

    for key, value in items:
        k = (key or "").lower()
        if k == "if" and isinstance(value, list):
            taken = branch(value)
            if not taken:  # else / else_if written inside the if block
                for a, b in value:
                    a = (a or "").lower()
                    if a == "else_if" and isinstance(b, list) and branch(b):
                        taken = True
                        break
                    if a == "else" and isinstance(b, list):
                        run_effects(b, ctx, visit)
                        taken = True
                        break
        elif k == "else_if" and isinstance(value, list):
            if taken is False:
                taken = branch(value)
        elif k == "else" and isinstance(value, list):
            if taken is False:
                run_effects(value, ctx, visit)
            taken = None
        else:
            visit(key, value)


def is_date(key):
    return bool(key and re.match(r"^\d{1,4}\.\d+\.\d+$", key))


def date_tuple(text):
    m = re.match(r"^(\d{1,4})[.-](\d+)[.-](\d+)", str(text or ""))
    return tuple(int(x) for x in m.groups()) if m else None


DEFAULT_START = "1936.1.1"


def history_walk(items, ctx, start_date, visit):
    """Apply a history block as the game does at start_date: the undated entries first, then
    every dated block whose date is on or before start_date, oldest first (blocks nested in
    if-branches included). visit(key, value) gets the effects; dated blocks after the start are
    skipped. With ctx None every if-branch body is walked (no DLC information)."""
    start = date_tuple(start_date or DEFAULT_START) or (1936, 1, 1)
    dated = []

    def collect(k, v):
        if is_date(k):
            if isinstance(v, list):
                dated.append((date_tuple(k), len(dated), v))
            return
        visit(k, v)

    if ctx is None:
        walk_all(items, collect)
    else:
        run_effects(items, ctx, collect)
    for when, _, body in sorted(dated, key=lambda x: (x[0], x[1])):
        if when <= start:
            if ctx is None:
                walk_all(body, collect)
            else:
                run_effects(body, ctx, collect)


def walk_all(items, visit):
    """Every branch of every if/else (legacy reading without DLC information)."""
    for k, v in items:
        kl = (k or "").lower()
        if kl in ("if", "else_if", "else") and isinstance(v, list):
            walk_all([(a, b) for a, b in v if (a or "").lower() != "limit"], visit)
        else:
            visit(k, v)
