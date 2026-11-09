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


class Block(list):
    """A parsed block: a list of (key, value) pairs that also knows where it came from.

    `line` is the 1-based line of the block's key (1 for the top level), `lines[i]` the line of
    item i, and `ops[i]` its operator ("=", "<", ">", "<=", ">=", or None for a bare value). It is
    a plain list for every other purpose."""

    __slots__ = ("line", "lines", "ops")

    def __init__(self, line=1):
        super().__init__()
        self.line = line
        self.lines = []
        self.ops = []

    def add(self, item, line, op):
        self.append(item)
        self.lines.append(line)
        self.ops.append(op)


def line_of(items, i, default=None):
    """Line of item i of a parsed block (default for a plain list)."""
    lines = getattr(items, "lines", None)
    return lines[i] if lines is not None and i < len(lines) else default


def op_of(items, i, default="="):
    ops = getattr(items, "ops", None)
    return (ops[i] or default) if ops is not None and i < len(ops) else default


def parse(text):
    """Return a block: list of (key, value) where value is str or block (list).
    Bare values inside a block are stored as (None, token). Blocks are Block lists that carry
    the line and operator of each item (see Block)."""
    text = text.lstrip("\ufeff")
    toks, tlines = [], []
    line, last = 1, 0
    for m in _TOKEN.finditer(text):
        t = m.group(0)
        start = m.start()
        line += text.count("\n", last, start)
        last = start
        if t.startswith("#"):
            continue
        toks.append(t)
        tlines.append(line)
    pos = 0

    def block(top, at):
        nonlocal pos
        items = Block(at)
        while pos < len(toks):
            t = toks[pos]
            ln = tlines[pos]
            if t == "}":
                pos += 1
                if top:
                    continue  # tolerate a stray brace at top level
                return items
            pos += 1
            if t in _OPS or t == "{":
                if t == "{":  # anonymous nested block
                    items.add((None, block(False, ln)), ln, None)
                continue
            if pos < len(toks) and toks[pos] in _OPS:
                op = toks[pos]
                pos += 1
                if pos < len(toks) and toks[pos] == "{":
                    pos += 1
                    items.add((t.strip('"'), block(False, ln)), ln, op)
                elif pos < len(toks):
                    items.add((t.strip('"'), toks[pos].strip('"')), ln, op)
                    pos += 1
            else:
                items.add((None, t.strip('"')), ln, None)
        return items

    return block(True, 1)


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
    """Evaluation context: the plan's tag, its ACTIVE DLC display names, and optionally the start
    date (for `has_start_date` and `date` comparisons, which are static at the start of a game)."""

    def __init__(self, tag, dlcs, date=None):
        self.tag = tag
        self.dlcs = set(dlcs)
        self.date = date

    def with_date(self, date):
        return Ctx(self.tag, self.dlcs, date)


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


def _pairs(items):
    """(key, value, op) of a block's items (op "=" for plain lists)."""
    ops = getattr(items, "ops", None)
    for i, (k, v) in enumerate(items):
        yield k, v, ((ops[i] or "=") if ops is not None and i < len(ops) else "=")


def trigger(items, ctx):
    """Evaluate a trigger block (implicit AND)."""
    if not isinstance(items, list):
        return None
    return _and(trigger_one(k, v, ctx, op) for k, v, op in _pairs(items))


def _compare(a, op, b):
    if op == "<":
        return a < b
    if op == ">":
        return a > b
    if op == "<=":
        return a <= b
    if op == ">=":
        return a >= b
    return a == b


def trigger_one(key, value, ctx, op="="):
    k = (key or "").lower()
    if ctx is None:
        return None
    if k == "has_dlc":
        return value in ctx.dlcs
    if k in ("tag", "original_tag"):
        return value == ctx.tag
    if k == "always":
        return value == "yes"
    if k in ("has_start_date", "date") and isinstance(value, str):
        start = date_tuple(getattr(ctx, "date", None) or "")
        when = date_tuple(value)
        if start is None or when is None or op == "=":
            return None  # "=" on a date is not a plain equality in the game; leave it runtime
        return _compare(start, op, when)
    if k == "and":
        return trigger(value, ctx)
    if k == "or":
        if not isinstance(value, list):
            return None
        return _or(trigger_one(a, b, ctx, o) for a, b, o in _pairs(value))
    if k == "not":  # HOI4 NOT is NOR
        if not isinstance(value, list):
            return None
        r = _or(trigger_one(a, b, ctx, o) for a, b, o in _pairs(value))
        return None if r is None else not r
    return None


def render(items, limit=None):
    """One-line script text of a parsed block ("has_war_with = ITA OR = { tag = A tag = B }"),
    cut at `limit` characters with "..."."""
    parts = []

    def walk(block):
        for k, v, op in _pairs(block):
            if isinstance(v, list):
                parts.append(f"{k} {op} {{" if k is not None else "{")
                walk(v)
                parts.append("}")
            elif k is None:
                parts.append(_quote(v))
            else:
                parts.append(f"{k} {op} {_quote(v)}")

    if isinstance(items, list):
        walk(items)
    else:
        parts.append(str(items))
    text = " ".join(parts)
    if limit and len(text) > limit:
        text = text[: max(0, limit - 3)].rstrip() + "..."
    return text


def _quote(v):
    return f'"{v}"' if isinstance(v, str) and (not v or any(c.isspace() for c in v)) else str(v)


def _sub(items, drop):
    """The items of a block without the keys in `drop`, keeping lines and operators."""
    out = Block(getattr(items, "line", None))
    lines = getattr(items, "lines", None)
    ops = getattr(items, "ops", None)
    for i, (k, v) in enumerate(items):
        if (k or "").lower() in drop:
            continue
        out.add((k, v), lines[i] if lines is not None and i < len(lines) else None,
                ops[i] if ops is not None and i < len(ops) else "=")
    return out


def run_effects(items, ctx, visit, with_line=False):
    """Walk an effect block, taking only the if/else_if/else branches that apply
    under ctx (an unknown limit counts as true). Calls visit(key, value) for
    every other entry (visit(key, value, line) with with_line); nested plain blocks
    are not entered."""
    taken = None  # state of the sibling if-chain: None (no chain), True, False

    def branch(body):
        cond = trigger(get1(body, "limit", []), ctx)
        if cond is False:
            return False
        run_effects(_sub(body, ("limit", "else", "else_if")), ctx, visit, with_line)
        return True

    for i, (key, value) in enumerate(items):
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
                        run_effects(b, ctx, visit, with_line)
                        taken = True
                        break
        elif k == "else_if" and isinstance(value, list):
            if taken is False:
                taken = branch(value)
        elif k == "else" and isinstance(value, list):
            if taken is False:
                run_effects(value, ctx, visit, with_line)
            taken = None
        elif with_line:
            visit(key, value, line_of(items, i))
        else:
            visit(key, value)


def is_date(key):
    return bool(key and re.match(r"^\d{1,4}\.\d+\.\d+$", key))


def date_tuple(text):
    m = re.match(r"^(\d{1,4})[.-](\d+)[.-](\d+)", str(text or ""))
    return tuple(int(x) for x in m.groups()) if m else None


DEFAULT_START = "1936.1.1"


def history_walk(items, ctx, start_date, visit, with_line=False):
    """Apply a history block as the game does at start_date: the undated entries first, then
    every dated block whose date is on or before start_date, oldest first (blocks nested in
    if-branches included). visit(key, value) gets the effects (visit(key, value, line) with
    with_line); dated blocks after the start are skipped. With ctx None every if-branch body is
    walked (no DLC information)."""
    start = date_tuple(start_date or DEFAULT_START) or (1936, 1, 1)
    if ctx is not None and getattr(ctx, "date", None) is None:
        ctx = ctx.with_date(start_date or DEFAULT_START)  # has_start_date is static at a bookmark
    dated = []

    def collect(k, v, line=None):
        if is_date(k):
            if isinstance(v, list):
                dated.append((date_tuple(k), len(dated), v))
            return
        if with_line:
            visit(k, v, line)
        else:
            visit(k, v)

    if ctx is None:
        walk_all(items, collect, True)
    else:
        run_effects(items, ctx, collect, True)
    for when, _, body in sorted(dated, key=lambda x: (x[0], x[1])):
        if when <= start:
            if ctx is None:
                walk_all(body, collect, True)
            else:
                run_effects(body, ctx, collect, True)


def walk_all(items, visit, with_line=False):
    """Every branch of every if/else (legacy reading without DLC information)."""
    for i, (k, v) in enumerate(items):
        kl = (k or "").lower()
        if kl in ("if", "else_if", "else") and isinstance(v, list):
            walk_all(_sub(v, ("limit",)), visit, with_line)
        elif with_line:
            visit(k, v, line_of(items, i))
        else:
            visit(k, v)
