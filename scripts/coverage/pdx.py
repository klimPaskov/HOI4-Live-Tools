"""Minimal Clausewitz script reader: enough to list entity keys (no evaluation)."""
import re

TOK = re.compile(r'"(?:[^"\\]|\\.)*"|[{}]|[<>!]?=|[<>]|[^\s{}=<>"#]+|#[^\n]*')


def tokens(text):
    for m in TOK.finditer(text):
        t = m.group(0)
        if t.startswith('#'):
            continue
        yield t


def parse(text):
    """Return a list of (key, value) pairs; value is a str or a nested list. Bare list items have key None."""
    toks = list(tokens(text))
    pos = 0

    def block():
        nonlocal pos
        out = []
        while pos < len(toks):
            t = toks[pos]
            if t == '}':
                pos += 1
                return out
            if t == '{':
                pos += 1
                out.append((None, block()))
                continue
            if pos + 1 < len(toks) and toks[pos + 1] in ('=', '<', '>', '<=', '>=', '!=', '?='):
                pos += 2
                if pos < len(toks) and toks[pos] == '{':
                    pos += 1
                    out.append((t.strip('"'), block()))
                elif pos < len(toks):
                    out.append((t.strip('"'), toks[pos].strip('"')))
                    pos += 1
                continue
            out.append((None, t.strip('"')))
            pos += 1
        return out

    return block()


def read(path):
    try:
        with open(path, encoding='utf-8-sig', errors='replace') as f:
            return parse(f.read())
    except OSError:
        return []


def get(block, key):
    return [v for k, v in block if k == key]


def first(block, key, default=None):
    for k, v in block:
        if k == key:
            return v
    return default
