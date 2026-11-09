"""Light reader for common/defines/*.lua (install plus mods), with the source of every value.

Two shapes are read:

    NDefines = { NCountry = { BASE_SURRENDER_LIMIT = 0.8, -- comment ... } }   (the install)
    NDefines.NCountry.BASE_SURRENDER_LIMIT = 0.7                                (typical mod file)

Files are read in load order (content.Content.files, sorted by name, mods replacing files of the
same name), and a later assignment of the same key wins, as the game's Lua does. Values are
numbers, strings, booleans or lists of those (`{ 2.0, 0.5 }`). Keys are stored as
"NCountry.BASE_SURRENDER_LIMIT"; lookups accept the short name too when it is unique.

Stdlib only, Python 3.9+.
"""
import re

_TOK = re.compile(r'--\[\[.*?\]\]|--[^\n]*|"[^"\n]*"|[{}=,]|[A-Za-z_][A-Za-z0-9_.]*|-?\d+(?:\.\d+)?(?:[eE][-+]?\d+)?|\n|\S',
                  re.S)


def _value(tok):
    if tok.startswith('"'):
        return tok.strip('"')
    if tok in ("true", "false"):
        return tok == "true"
    try:
        f = float(tok)
        return int(f) if re.match(r"^-?\d+$", tok) else f
    except ValueError:
        return tok


def parse_lua(text):
    """[(dotted key, value, line)] of every scalar or list assignment in a defines file."""
    out = []
    toks = []
    line = 1
    for m in _TOK.finditer(text):
        t = m.group(0)
        if t == "\n":
            line += 1
            continue
        if t.startswith("--"):
            line += t.count("\n")
            continue
        toks.append((t, line))
    stack = []  # names of the open tables (None for an anonymous table)
    i = 0
    n = len(toks)
    while i < n:
        t, ln = toks[i]
        if t == "}":
            if stack:
                stack.pop()
            i += 1
            continue
        if i + 2 < n and toks[i + 1][0] == "=" and re.match(r"^[A-Za-z_]", t):
            key = t
            v, vln = toks[i + 2]
            prefix = ".".join(s for s in stack if s)
            full = f"{prefix}.{key}" if prefix else key
            if v == "{":
                # a list of scalars stays one value; a table of names is a namespace
                j = i + 3
                items = []
                flat = True
                while j < n and toks[j][0] != "}":
                    tj = toks[j][0]
                    if tj == "{" or (j + 1 < n and toks[j + 1][0] == "="):
                        flat = False
                        break
                    if tj != ",":
                        items.append(_value(tj))
                    j += 1
                if flat and j < n:
                    out.append((_strip_root(full), items, ln))
                    i = j + 1
                    continue
                stack.append(key)
                i += 3
                continue
            out.append((_strip_root(full), _value(v), ln))
            i += 3
            continue
        if t == "{":
            stack.append(None)
        i += 1
    return out


def _strip_root(key):
    """"NDefines.NCountry.X" and "NCountry.X" are the same key."""
    parts = key.split(".")
    if len(parts) > 2 and parts[0].startswith("NDefines"):
        parts = parts[1:]
    return ".".join(parts)


class Defines:
    """{key: {"v": value, "file": path, "line": n}} over the effective defines files."""

    def __init__(self, content):
        self.content = content
        self._table = None

    def table(self):
        if self._table is None:
            table = {}
            for p in self.content.files("common/defines", ext=(".lua",), recursive=True):
                try:
                    with open(p, encoding="utf-8-sig", errors="replace") as fh:
                        text = fh.read()
                except OSError:
                    continue
                for key, value, line in parse_lua(text):
                    table[key] = {"v": value, "file": p, "line": line}
            self._table = table
        return self._table

    def find(self, key):
        """[(full key, row)] for a full key ("NCountry.BASE_SURRENDER_LIMIT"), a short one
        ("BASE_SURRENDER_LIMIT"), or a substring when nothing matches exactly."""
        t = self.table()
        if key in t:
            return [(key, t[key])]
        short = [(k, r) for k, r in t.items() if k.split(".")[-1] == key]
        if short:
            return sorted(short)
        up = key.upper()
        return sorted((k, r) for k, r in t.items() if up in k.upper())

    def get(self, key, default=None):
        hits = self.find(key)
        exact = [h for h in hits if h[0] == key or h[0].split(".")[-1] == key]
        if len(exact) >= 1:
            return exact[0][1]["v"]
        return default

    def row(self, key):
        hits = [h for h in self.find(key) if h[0] == key or h[0].split(".")[-1] == key]
        return hits[0] if hits else (None, None)
