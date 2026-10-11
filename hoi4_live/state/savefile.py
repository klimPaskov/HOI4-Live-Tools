"""Plain-text HOI4 save reader: a fast structural index plus an on-demand subtree parser.

A text save (header `HOI4txt`, written with save_as_binary=no) is Clausewitz script of 100-250 MB. Parsing all of
it in Python takes minutes, so this reader:

  1. memory-maps the file (no private copy),
  2. scans it in chunks with numpy for braces outside quoted strings and records only the shallow structure:
     every block whose path has at most INDEX_DEPTH segments (top-level blocks, countries/TAG, countries/TAG/xxx,
     states/N/xxx ...) with its byte span,
  3. caches that index on disk keyed by path, size and mtime,
  4. parses only the requested subtree with a small tokenizer.

Paths use "/" between keys and accept fnmatch globs per segment: countries/USA/variables, countries/*/flags.
A key repeated inside one block is addressed as key[1], key[2] (the first occurrence is plain key); an unnamed
block (list element) is "-".

Parsed values: a block becomes a dict (repeated keys collect into a list under one key, bare items go to
"_items"); a block holding only bare items becomes a list; numbers become int/float, everything else stays a
string (yes/no stay strings, quotes are removed).
"""

import fnmatch
import hashlib
import json
import mmap
import os
import re
import tempfile
import zipfile

import numpy as np

from .. import platform

from ..errors import ToolError

TEXT_HEADER = b"HOI4txt"
BINARY_HEADER = b"HOI4bin"
INDEX_DEPTH = 3
INDEX_VERSION = 2
CHUNK = 8 << 20

TOKEN = re.compile(rb'"(?:[^"\\]|\\.)*"|[{}]|[<>!]?=|[<>]|#[^\n]*|[^\s{}=<>!"#]+')
INT_RX = re.compile(r"^-?\d+$")
FLOAT_RX = re.compile(r"^-?\d+\.\d+$")
KEY_BACK = re.compile(rb'(?:"((?:[^"\\]|\\.)*)"|([^\s{}=<>!"#]+))\s*[<>!]?=\s*$')


class SaveError(ToolError):
    pass


def _cache_dir():
    d = os.path.join(platform.current().local_data_dir(), "hoi4_live", "save_index")
    os.makedirs(d, exist_ok=True)
    return d


def convert_scalar(tok):
    if tok.startswith('"') and tok.endswith('"') and len(tok) >= 2:
        return tok[1:-1]
    if INT_RX.match(tok):
        try:
            return int(tok)
        except ValueError:
            return tok
    if FLOAT_RX.match(tok):
        try:
            return float(tok)
        except ValueError:
            return tok
    return tok


class SaveFile:
    def __init__(self, path, use_cache=True):
        self.path = os.path.abspath(path)
        if not os.path.exists(self.path):
            raise SaveError(f"save not found: {self.path}")
        st = os.stat(self.path)
        self.size, self.mtime = st.st_size, st.st_mtime
        self._file = None
        self._mm = None
        self._data = None
        self.use_cache = use_cache
        self._index = None
        self._children = None
        self._spans = None
        self._open()

    # ------------------------------------------------------------------ opening
    def _open(self):
        with open(self.path, "rb") as f:
            head = f.read(8)
        if head[:2] == b"PK":
            with zipfile.ZipFile(self.path) as z:
                names = [n for n in z.namelist() if not n.endswith("/")]
                data = z.read(names[0]) if names else b""
            if data.startswith(BINARY_HEADER):
                raise self._binary_error()
            self._data = data
        else:
            if head.startswith(BINARY_HEADER):
                raise self._binary_error()
            if not head.startswith(TEXT_HEADER):
                raise SaveError(f"{self.path} is not a HOI4 save (header {head!r})")
            self._file = open(self.path, "rb")
            self._mm = mmap.mmap(self._file.fileno(), 0, access=mmap.ACCESS_READ)
            self._data = self._mm

    def _binary_error(self):
        return SaveError(f"{os.path.basename(self.path)} is a binary save (HOI4bin). Set save_as_binary=no in "
                         "Documents/Paradox Interactive/Hearts of Iron IV/settings.txt and save again.",
                         {"binary": True})

    def close(self):
        if self._mm is not None:
            self._mm.close()
            self._mm = None
        if self._file is not None:
            self._file.close()
            self._file = None
        self._data = None

    def __enter__(self):
        return self

    def __exit__(self, *exc):
        self.close()

    @property
    def data(self):
        return self._data

    # ------------------------------------------------------------------ header
    def header(self):
        """Top-level scalars and the small blocks at the start (player, date, version, mods ...)."""
        head = bytes(self._data[:65536])
        out = {}
        stop = len(head)
        m = re.search(rb"\n(mod_achievement|railway_gun_index|flags)=", head)
        if m:
            stop = m.start()
        parsed = parse_block(head[len(TEXT_HEADER):stop] if head.startswith(TEXT_HEADER) else head[:stop])
        if isinstance(parsed, dict):
            for k, v in parsed.items():
                if k in ("player", "ideology", "date", "difficulty", "version", "save_version", "minor_save_version",
                         "dlcs", "tutorial", "mods", "player_countries", "game_rules"):
                    out[k] = v
        out["file"] = os.path.basename(self.path)
        out["size_mb"] = round(self.size / 1048576, 1)
        return out

    # ------------------------------------------------------------------ index
    def _cache_path(self):
        key = hashlib.sha1(f"{self.path}|{self.size}|{self.mtime}|{INDEX_VERSION}|{INDEX_DEPTH}".encode()).hexdigest()
        return os.path.join(_cache_dir(), key[:24] + ".json")

    def index(self):
        if self._index is not None:
            return self._index
        cp = self._cache_path()
        if self.use_cache and os.path.exists(cp):
            try:
                with open(cp, encoding="utf-8") as f:
                    raw = json.load(f)
                self._index = {tuple(k.split("\x1f")): v for k, v in raw.items()}
                return self._index
            except (ValueError, OSError):
                pass
        self._index = self._build_index()
        if self.use_cache:
            try:
                tmp = cp + ".tmp"
                with open(tmp, "w", encoding="utf-8") as f:
                    json.dump({"\x1f".join(k): v for k, v in self._index.items()}, f)
                os.replace(tmp, cp)
                self._prune_cache()
            except OSError:
                pass
        return self._index

    @staticmethod
    def _prune_cache(keep=40):
        d = _cache_dir()
        files = sorted((os.path.join(d, f) for f in os.listdir(d) if f.endswith(".json")), key=os.path.getmtime)
        for f in files[:-keep]:
            try:
                os.remove(f)
            except OSError:
                pass

    def _structure(self):
        """Positions of braces outside strings with their depth, for shallow levels only.
        Returns two int64 arrays: opens (pos, depth_before) and closes (pos, depth_after), depth <= INDEX_DEPTH-1."""
        buf = np.frombuffer(self._data, dtype=np.uint8)
        n = buf.size
        in_quote = False
        depth = 0
        opens, closes = [], []
        for start in range(0, n, CHUNK):
            chunk = buf[start:start + CHUNK]
            q = np.flatnonzero(chunk == 34)
            if q.size:
                prev = chunk[np.maximum(q - 1, 0)]
                q = q[(prev != 92) | (q == 0)]          # ignore escaped quotes
            ob = np.flatnonzero(chunk == 123)
            cb = np.flatnonzero(chunk == 125)
            if q.size:
                # parity of quotes before each brace tells whether it is inside a string
                qpos = q
                def outside(pos):
                    k = np.searchsorted(qpos, pos)
                    parity = (k + (1 if in_quote else 0)) % 2
                    return pos[parity == 0]
                ob = outside(ob)
                cb = outside(cb)
            elif in_quote:
                ob = ob[:0]
                cb = cb[:0]
            if q.size % 2 == 1:
                in_quote = not in_quote
            if ob.size or cb.size:
                pos = np.concatenate((ob, cb))
                delta = np.concatenate((np.ones(ob.size, np.int32), -np.ones(cb.size, np.int32)))
                order = np.argsort(pos, kind="stable")
                pos, delta = pos[order], delta[order]
                after = depth + np.cumsum(delta)
                before = after - delta
                is_open = delta > 0
                sel_o = is_open & (before < INDEX_DEPTH)
                sel_c = (~is_open) & (after < INDEX_DEPTH)
                if sel_o.any():
                    opens.append(np.stack((pos[sel_o].astype(np.int64) + start, before[sel_o].astype(np.int64)), 1))
                if sel_c.any():
                    closes.append(np.stack((pos[sel_c].astype(np.int64) + start, after[sel_c].astype(np.int64)), 1))
                depth = int(after[-1])
        o = np.concatenate(opens) if opens else np.zeros((0, 2), np.int64)
        c = np.concatenate(closes) if closes else np.zeros((0, 2), np.int64)
        return o, c

    def _key_before(self, pos):
        lo = max(0, pos - 200)
        back = bytes(self._data[lo:pos])
        m = KEY_BACK.search(back)
        if not m:
            return "-"
        raw = m.group(1) if m.group(1) is not None else m.group(2)
        return raw.decode("utf-8", "replace")

    def _build_index(self):
        opens, closes = self._structure()
        events = [(int(p), 1, int(d)) for p, d in opens] + [(int(p), -1, int(d)) for p, d in closes]
        events.sort()
        index = {}
        stack = []                 # (path, start)
        seen = [dict()]            # occurrence counters per open block
        for pos, kind, depth in events:
            if kind == 1:
                key = self._key_before(pos)
                counter = seen[-1]
                n = counter.get(key, 0)
                counter[key] = n + 1
                name = key if n == 0 else f"{key}[{n}]"
                parent = stack[-1][0] if stack else ()
                stack.append((parent + (name,), pos))
                seen.append(dict())
            else:
                if not stack:
                    continue
                path, start = stack.pop()
                seen.pop()
                index[path] = [start, pos + 1]
        return index

    # ------------------------------------------------------------------ navigation
    def children(self, path_parts):
        """Indexed child names of an indexed path (for globbing)."""
        if self._children is None:
            ch = {}
            for k in self.index():
                ch.setdefault(k[:-1], []).append(k[-1])
            self._children = ch
        return list(self._children.get(tuple(path_parts), []))

    def resolve(self, path):
        """Expand a glob path into concrete indexed paths plus the unindexed remainder:
        [(indexed_path_tuple, remainder_list)]."""
        parts = [p for p in (path or "").strip("/").split("/") if p]
        if not parts:
            return [((), [])]
        idx = self.index()
        results = [()]
        depth = 0
        while depth < len(parts) and depth < INDEX_DEPTH:
            seg = parts[depth]
            nxt = []
            for base in results:
                if any(ch in seg for ch in "*?["):
                    nxt += [base + (n,) for n in self.children(base) if fnmatch.fnmatchcase(n, seg)]
                elif base + (seg,) in idx:
                    nxt.append(base + (seg,))
            if not nxt:
                # the path may continue into a scalar or an unindexed child of the last indexed block
                return [(b, parts[depth:]) for b in results if b]
            results = nxt
            depth += 1
        return [(r, parts[depth:]) for r in results]

    def span_text(self, path_tuple):
        start, end = self.index()[path_tuple]
        return bytes(self._data[start:end])

    def get(self, path, max_bytes=8 << 20):
        """Parsed value(s) at path. Returns {concrete_path: value}."""
        out = {}
        for base, rest in self.resolve(path):
            if not base:
                raise SaveError("get needs a path (use info/keys for the top level)")
            start, end = self.index()[base]
            if end - start > max_bytes:
                raise SaveError(f"{'/'.join(base)} is {round((end - start) / 1048576, 1)} MB; narrow the path",
                                {"bytes": end - start})
            value = parse_block(bytes(self._data[start + 1:end - 1]))
            for key, val in _descend(value, rest, "/".join(base)):
                out[key] = val
        return out

    def keys(self, path=""):
        """Child keys with occurrence counts under an indexed path ('' = top level)."""
        parts = tuple(p for p in path.strip("/").split("/") if p)
        if len(parts) < INDEX_DEPTH and (not parts or parts in self.index()):
            names = self.children(parts)
            counts = {}
            for n in names:
                base = re.sub(r"\[\d+\]$", "", n)
                counts[base] = counts.get(base, 0) + 1
            scalars = {}
            if parts:
                val = parse_block(self.span_text(parts)[1:-1], max_depth=1)
                if isinstance(val, dict):
                    for k, v in val.items():
                        if not isinstance(v, (dict, list)) and k not in counts:
                            scalars[k] = v
            return {"blocks": counts, "scalars": scalars}
        got = self.get(path)
        out = {}
        for k, v in got.items():
            if isinstance(v, dict):
                out[k] = {kk: type(vv).__name__ for kk, vv in v.items()}
            else:
                out[k] = type(v).__name__
        return out

    def find(self, pattern, inside=None, limit=100):
        """Regex search over the raw text (fast), each hit mapped to its deepest indexed path and line."""
        rx = re.compile(pattern.encode("utf-8") if isinstance(pattern, str) else pattern)
        spans = []
        if inside:
            for base, rest in self.resolve(inside):
                if base:
                    spans.append(tuple(self.index()[base]))
        else:
            spans = [(0, len(self._data))]
        hits = []
        for a, b in spans:
            for m in rx.finditer(self._data, a, b):
                ls = self._data.rfind(b"\n", 0, m.start()) + 1
                le = self._data.find(b"\n", m.end())
                line = bytes(self._data[ls:le if le != -1 else m.end()]).decode("utf-8", "replace").strip()
                hits.append({"offset": m.start(), "path": "/".join(self.path_at(m.start())), "line": line[:300]})
                if len(hits) >= limit:
                    return hits
        return hits

    def path_at(self, offset):
        """Deepest indexed path whose span contains the byte offset (binary search per depth)."""
        import bisect
        if self._spans is None:
            levels = {}
            for k, (a, b) in self.index().items():
                levels.setdefault(len(k), []).append((a, b, k))
            self._spans = {d: (sorted(v), [x[0] for x in sorted(v)]) for d, v in levels.items()}
        best = ()
        for d in sorted(self._spans):
            rows, starts = self._spans[d]
            i = bisect.bisect_right(starts, offset) - 1
            if i >= 0 and rows[i][0] <= offset < rows[i][1]:
                best = rows[i][2]
        return best


def _descend(value, rest, prefix):
    if not rest:
        yield prefix, value
        return
    seg, more = rest[0], rest[1:]
    if isinstance(value, dict):
        for k, v in value.items():
            vals = v if isinstance(v, list) and k in value.get("_repeated", ()) else [v]
            for i, item in enumerate(vals):
                name = k if i == 0 else f"{k}[{i}]"
                if fnmatch.fnmatchcase(name, seg) or fnmatch.fnmatchcase(k, seg):
                    yield from _descend(item, more, f"{prefix}/{name}")


def parse_block(text, max_depth=None):
    """Parse Clausewitz script (bytes) into Python values. See the module docstring for the mapping."""
    toks = [t for t in TOKEN.findall(text) if not t.startswith(b"#")]
    pos = [0]
    n = len(toks)

    def block(level):
        d = {}
        items = []
        repeated = set()
        while pos[0] < n:
            t = toks[pos[0]]
            if t == b"}":
                pos[0] += 1
                break
            if t == b"{":
                pos[0] += 1
                items.append(block(level + 1))
                continue
            if pos[0] + 1 < n and toks[pos[0] + 1] in (b"=", b"<", b">", b"<=", b">=", b"!=", b"?="):
                key = convert_scalar(t.decode("utf-8", "replace"))
                key = key if isinstance(key, str) else str(key)
                op = toks[pos[0] + 1]
                pos[0] += 2
                if pos[0] >= n:
                    break
                v = toks[pos[0]]
                if v == b"{":
                    pos[0] += 1
                    if max_depth is not None and level >= max_depth:
                        _skip()
                        val = {}
                    else:
                        val = block(level + 1)
                else:
                    pos[0] += 1
                    val = convert_scalar(v.decode("utf-8", "replace"))
                    if op not in (b"=",):
                        val = {"op": op.decode(), "value": val}
                if key in d:
                    if key not in repeated:
                        d[key] = [d[key]]
                        repeated.add(key)
                    d[key].append(val)
                else:
                    d[key] = val
                continue
            items.append(convert_scalar(t.decode("utf-8", "replace")))
            pos[0] += 1
        if items and not d:
            return items
        if items:
            d["_items"] = items
        if repeated:
            d["_repeated"] = sorted(repeated)
        return d

    def _skip():
        depth = 1
        while pos[0] < n and depth:
            t = toks[pos[0]]
            if t == b"{":
                depth += 1
            elif t == b"}":
                depth -= 1
            pos[0] += 1

    return block(0)


def clean(value):
    """Drop internal markers (_repeated) for presentation."""
    if isinstance(value, dict):
        return {k: clean(v) for k, v in value.items() if k != "_repeated"}
    if isinstance(value, list):
        return [clean(v) for v in value]
    return value


def flatten(value, prefix="", out=None, limit=None):
    """{path: scalar} leaves of a parsed value."""
    if out is None:
        out = {}
    if limit is not None and len(out) >= limit:
        return out
    if isinstance(value, dict):
        rep = set(value.get("_repeated", ()))
        for k, v in value.items():
            if k == "_repeated":
                continue
            if k in rep and isinstance(v, list):
                for i, item in enumerate(v):
                    flatten(item, f"{prefix}/{k}" + (f"[{i}]" if i else ""), out, limit)
            else:
                flatten(v, f"{prefix}/{k}" if prefix else k, out, limit)
    elif isinstance(value, list):
        if all(not isinstance(v, (dict, list)) for v in value):
            out[prefix] = value
        else:
            for i, v in enumerate(value):
                flatten(v, f"{prefix}/-" + (f"[{i}]" if i else ""), out, limit)
    else:
        out[prefix] = value
    return out
