"""Clausewitz script as text: split a trigger block into statements, render them back on one line and cut a focus's
blocks out of a national focus file.

The parsed dictionaries of `scriptdb` lose quotes and the order of repeated keys, so a trigger that is evaluated in
the game is taken from the text itself. A statement is `key op value`, `key op { statements }`, a bare word or a
bare `{ ... }`.
"""

import re

OPS = ("=", "<", ">", "<=", ">=", "!=", "?=")
TOKEN = re.compile(r'"[^"\n]*"|#[^\n]*|@\[[^\]\n]*\]|[<>!?]=|[=<>{}]|(?:[^\s=<>{}#"!?]|[!?](?!=))+')
LOGIC_KEYS = ("AND", "OR", "NOT")
# blocks a caller may paste around the real conditions: `limit = { ... }`, `available = { ... }`
WRAPPER_KEYS = ("limit", "available", "trigger", "allow_branch", "bypass", "visible", "potential", "allowed", "allow")


class ScriptError(ValueError):
    pass


class Stmt:
    __slots__ = ("key", "op", "value", "children")

    def __init__(self, key=None, op=None, value=None, children=None):
        self.key, self.op, self.value, self.children = key, op, value, children

    @property
    def is_block(self):
        return self.children is not None

    def render(self):
        if self.is_block:
            inner = " ".join(c.render() for c in self.children)
            body = "{ " + inner + " }" if inner else "{ }"
            return f"{self.key} {self.op} {body}" if self.key is not None else body
        if self.key is None:
            return self.value
        return f"{self.key} {self.op} {self.value}"

    def __repr__(self):
        return f"Stmt({self.render()!r})"


def tokens(text):
    out = [t for t in TOKEN.findall(text) if not t.startswith("#")]
    depth = 0
    for t in out:
        if t == "{":
            depth += 1
        elif t == "}":
            depth -= 1
            if depth < 0:
                raise ScriptError("unbalanced braces: a } closes nothing")
    if depth:
        raise ScriptError("unbalanced braces: a { is never closed")
    return out


def parse(text):
    """The statements of a block's inner text, as a list of Stmt."""
    toks = tokens(text)
    stmts, _ = _block(toks, 0)
    return stmts


def _block(toks, i):
    stmts = []
    n = len(toks)
    while i < n:
        t = toks[i]
        if t == "}":
            return stmts, i + 1
        if t == "{":
            children, i = _block(toks, i + 1)
            stmts.append(Stmt(children=children))
            continue
        if i + 1 < n and toks[i + 1] in OPS and i + 2 < n:
            op = toks[i + 1]
            if toks[i + 2] == "{":
                children, i = _block(toks, i + 3)
                stmts.append(Stmt(t, op, children=children))
            else:
                stmts.append(Stmt(t, op, toks[i + 2]))
                i += 3
            continue
        stmts.append(Stmt(value=t))
        i += 1
    return stmts, i


def unwrap(stmts):
    """The conditions inside a pasted wrapper: a lone `limit = { ... }`, `available = { ... }` or bare `{ ... }`."""
    while len(stmts) == 1 and stmts[0].is_block and (stmts[0].key is None or stmts[0].key in WRAPPER_KEYS):
        stmts = stmts[0].children
    return stmts


def conditions(text):
    """The top-level conditions of a trigger as pasted by a person."""
    return unwrap(parse(text))


def render_all(stmts):
    return " ".join(s.render() for s in stmts)


def find_focus(text, focus_id):
    """The Stmt of `focus = { id = <focus_id> ... }` in a national focus file's text, or None."""
    for top in parse(text):
        for found in _walk_focuses(top, focus_id):
            return found
    return None


def _walk_focuses(stmt, focus_id):
    if not stmt.is_block:
        return
    if stmt.key in ("focus", "shared_focus", "joint_focus"):
        for c in stmt.children:
            if c.key == "id" and not c.is_block and c.value.strip('"') == focus_id:
                yield stmt
                return
        return
    for c in stmt.children:
        yield from _walk_focuses(c, focus_id)


def child_blocks(stmt, key):
    """Every direct child block called `key` (a focus has one `available`, but several `prerequisite`)."""
    return [c for c in stmt.children if c.is_block and c.key == key]


CONST_DEF = re.compile(r'^[ \t]*(@[A-Za-z0-9_]+)[ \t]*=[ \t]*(-?[0-9.]+|"[^"\n]*"|[A-Za-z0-9_]+)[ \t\r]*(?:#[^\n]*)?$', re.M)     # \r: the files are CRLF on Windows


def resolve_constants(text):
    """Replace `@name` by the value of its `@name = value` line in the same file (the line itself is dropped). The
    game does this at load, but the console would see an undefined @name. Inline math `@[ ... ]` stays as it is."""
    consts = {m.group(1): m.group(2) for m in CONST_DEF.finditer(text)}
    if not consts:
        return text
    text = CONST_DEF.sub("", text)
    for name in sorted(consts, key=len, reverse=True):
        text = re.sub(re.escape(name) + r"(?![A-Za-z0-9_])", lambda m, v=consts[name]: v, text)
    return text
