#!/usr/bin/env python3
"""Generate the Linux translation table that native/src/xlayout.rs reads.

usage:
  gen_linux_layout.py keys [--src native/src] [--out keys.json]
      static scan only: Class tags, image addresses and (class, offset) field keys the sources use
  gen_linux_layout.py generate MACHO ELF [--src native/src] [--hints hints.json] [--include-low] [--no-fill]
                      [--out native/layouts/linux-<build-id>.json] [--report coverage.json] [--cache DIR]
  gen_linux_layout.py --check-table TABLE.json [--keys keys.json]

Keys are the macOS 1.19.4.1 values the tools pass to `img(abi, mac)` and
`fld(abi, &Class::new(name, mac_vtable), mac_offset)`. The table has the format
of `xlayout::Table::parse`:
  {"build_id": "<gnu build id>", "image": {"0xmac": "0xelf"}, "fields": {"Class": {"0xmac": "0xelf"}}}
Only resolved entries are written (high and medium confidence; low with --include-low); everything
else goes to the coverage report with a reason. Values come from scripts/cross_platform_map.py.

hints.json (optional) adds evidence for classes, keyed by Class name:
  {"CInGameIdler":  {"context": ["0x372afc0"]},
   "idler.manager": {"via": {"parent": "CInGameIdler", "offset": "0x670"}},
   "some.record":   {"anchors": ["0x1234560"]},
   "Foo":           {"rtti": "N3Foo3BarE"}}
anchors: Mach-O functions whose first argument (rdi) is the object; context: Mach-O pointer globals
that hold the object; via: the object is the pointer stored in the parent's field (the parent's field
must map first; its functions are then inspected for loads of that field). A vtable-0 tag that is a
real C++ class name (e.g. "CAgencyUpgradeDatabase") is also tried by its RTTI name automatically.
scripts/data/xlayout-hints-1.19.4.1.json is the starter set.
"""
import argparse
import collections
import importlib.util
import json
import os
import pathlib
import re
import sys

HERE = pathlib.Path(__file__).resolve().parent
_spec = importlib.util.spec_from_file_location('cross_platform_map', HERE / 'cross_platform_map.py')
xmap = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(xmap)

# ---------------------------------------------------------------------------
# Rust source scanning (pure; unit tested)
# ---------------------------------------------------------------------------

def strip_comments(text):
    """Remove // and /* */ comments, keeping string literals and line numbers."""
    out, i, n = [], 0, len(text)
    while i < n:
        c = text[i]
        if c == '"':
            j = i + 1
            while j < n and text[j] != '"':
                j += 2 if text[j] == '\\' else 1
            out.append(text[i:j + 1])
            i = j + 1
        elif text.startswith('//', i):
            j = text.find('\n', i)
            i = n if j < 0 else j
        elif text.startswith('/*', i):
            j = text.find('*/', i + 2)
            seg = text[i:(n if j < 0 else j + 2)]
            out.append('\n' * seg.count('\n'))
            i = n if j < 0 else j + 2
        elif c == "'" and i + 2 < n and text[i + 2] == "'":
            out.append(text[i:i + 3])  # char literal like ','
            i += 3
        else:
            out.append(c)
            i += 1
    return ''.join(out)


def production_part(text):
    """Text before a top-level #[cfg(test)] (test modules are not runtime keys)."""
    m = re.search(r'^#\[cfg\(test\)\]', text, re.M)
    return text[:m.start()] if m else text


def balanced(text, i, open_='(', close=')'):
    """Index just past the bracket matching text[i] (which must be open_)."""
    depth = 0
    j = i
    while j < len(text):
        ch = text[j]
        if ch == '"':
            j += 1
            while j < len(text) and text[j] != '"':
                j += 2 if text[j] == '\\' else 1
        elif ch in '([{':
            depth += 1
        elif ch in ')]}':
            depth -= 1
            if depth == 0:
                return j + 1
        j += 1
    return len(text)


def split_args(s):
    out, depth, cur, j = [], 0, [], 0
    while j < len(s):
        ch = s[j]
        if ch == '"':
            k = j + 1
            while k < len(s) and s[k] != '"':
                k += 2 if s[k] == '\\' else 1
            cur.append(s[j:k + 1])
            j = k + 1
            continue
        if ch in '([{':
            depth += 1
        elif ch in ')]}':
            depth -= 1
        elif ch == '|' and depth == 0:
            pass
        if ch == ',' and depth == 0:
            out.append(''.join(cur).strip())
            cur = []
        else:
            cur.append(ch)
        j += 1
    if ''.join(cur).strip():
        out.append(''.join(cur).strip())
    return out


INT = re.compile(r'^(0x[0-9a-fA-F_]+|[0-9][0-9_]*)(?:u64|usize|u32|i32|i64)?$')
PATH_NAME = re.compile(r'^(?:[a-z_][a-z0-9_]*::)*([A-Z_][A-Z0-9_]*)$')
CLASS_NEW = re.compile(r'^(?:(?:crate::)?xlayout::)?Class::new\(\s*("[^"]+"|[A-Za-z_:.]+)\s*,\s*(.+?)\s*,?\s*\)$', re.S)
FIELD_ACCESS = re.compile(r'^([A-Za-z_][A-Za-z0-9_]*(?:::[A-Za-z_][A-Za-z0-9_]*)*)\.([a-z_][a-z0-9_]*)$')


class Source:
    """Constants of one Rust file."""

    def __init__(self, path, rel, text):
        self.path, self.rel = path, rel
        self.text = production_part(strip_comments(text))
        self.stem = pathlib.Path(rel).stem
        self.ints = {}      # NAME -> raw expression (evaluated lazily)
        self.classes = {}   # NAME -> (class name, raw vtable expr)
        self.structs = {}   # NAME -> {field: raw expr}
        self.const_lines = {}
        self.aliases = {}   # `use a::b as c` -> {'c': 'b'}
        for m in re.finditer(r'\buse\s+[^;]*;', self.text):
            for a in re.finditer(r'([A-Za-z_][A-Za-z0-9_]*)\s+as\s+([A-Za-z_][A-Za-z0-9_]*)', m.group(0)):
                self.aliases[a.group(2)] = a.group(1)
        self._scan_consts()

    def line(self, pos):
        return self.text.count('\n', 0, pos) + 1

    def _scan_consts(self):
        for m in re.finditer(r'\bconst\s+([A-Z_][A-Z0-9_]*)\s*:\s*([^=;]+?)\s*=\s*', self.text):
            name, ty = m.group(1), m.group(2).strip()
            start = m.end()
            j, depth = start, 0
            while j < len(self.text):
                ch = self.text[j]
                if ch == '"':
                    j = balanced_string_end(self.text, j)
                    continue
                if ch in '([{':
                    depth += 1
                elif ch in ')]}':
                    depth -= 1
                elif ch == ';' and depth == 0:
                    break
                j += 1
            expr = ' '.join(self.text[start:j].split())
            self.const_lines[name] = self.line(m.start())
            cm = CLASS_NEW.match(expr)
            if cm:
                self.classes[name] = (cm.group(1), cm.group(2))
                continue
            sm = re.match(r'^([A-Z][A-Za-z0-9_]*)\s*\{(.*)\}$', expr, re.S)
            if sm:
                fields = {}
                for part in split_args(sm.group(2)):
                    fm = re.match(r'^([a-z_][a-z0-9_]*)\s*:\s*(.+)$', part, re.S)
                    if fm:
                        fields[fm.group(1)] = fm.group(2).strip()
                self.structs[name] = dict(type=sm.group(1), fields=fields)
                continue
            if ty in ('u64', 'usize', 'u32', 'i32', 'i64', 'u16', 'u8'):
                self.ints[name] = expr


def balanced_string_end(text, j):
    k = j + 1
    while k < len(text) and text[k] != '"':
        k += 2 if text[k] == '\\' else 1
    return k + 1


class Index:
    """All Rust files; resolves constant expressions with Rust-like scoping (same file, module path, unique)."""

    def __init__(self, root):
        self.root = pathlib.Path(root)
        self.files = []
        for p in sorted(self.root.rglob('*.rs')):
            rel = os.path.relpath(p, self.root)
            if xmap.is_test_path(rel):
                continue
            self.files.append(Source(p, rel, p.read_text(errors='replace')))
        self.by_kind = collections.defaultdict(lambda: collections.defaultdict(list))
        for f in self.files:
            for kind in ('ints', 'classes', 'structs'):
                for name in getattr(f, kind):
                    self.by_kind[kind][name].append(f)

    def lookup(self, kind, path, here):
        """Source holding constant `path` (NAME or mod::NAME) as seen from file `here`."""
        parts = path.split('::')
        name = parts[-1]
        cands = self.by_kind[kind].get(name, [])
        if not cands:
            return None
        if len(parts) == 1 and here in cands:
            return here
        if len(parts) > 1 and parts[-2] not in ('self', 'crate', 'super'):
            mod = here.aliases.get(parts[-2], parts[-2])
            hit = [f for f in cands if f.stem == mod or pathlib.PurePath(f.rel).parent.name == mod]
            if len(hit) == 1:
                return hit[0]
        if len(parts) > 1 and parts[-2] == 'super' and here in cands:
            return here
        return cands[0] if len(cands) == 1 else None

    def eval_int(self, expr, here, depth=0):
        """Integer value of a constant expression (literals, consts, + - * and casts) or None."""
        if depth > 8:
            return None
        e = re.sub(r'\bas\s+(u64|usize|u32|i32|i64|u16|u8)\b', '', expr).strip()
        while e.startswith('(') and balanced(e, 0) == len(e):
            e = e[1:-1].strip()
        e = e.lstrip('*&').strip()
        m = INT.match(e)
        if m:
            return int(m.group(1).replace('_', ''), 0)
        for op in ('+', '-', '*'):
            parts = _split_top(e, op)
            if len(parts) > 1:
                vals = [self.eval_int(p, here, depth + 1) for p in parts]
                if None in vals:
                    return None
                v = vals[0]
                for x in vals[1:]:
                    v = v + x if op == '+' else v - x if op == '-' else v * x
                return v
        m = PATH_NAME.match(e)
        if m:
            src = self.lookup('ints', e, here)
            if src is not None:
                return self.eval_int(src.ints[e.split('::')[-1]], src, depth + 1)
        m = FIELD_ACCESS.match(e)
        if m and m.group(1).split('::')[-1][0].isupper():
            src = self.lookup('structs', m.group(1), here)
            if src is not None:
                f = src.structs[m.group(1).split('::')[-1]]['fields'].get(m.group(2))
                if f is not None:
                    return self.eval_int(f, src, depth + 1)
        return None

    def eval_str(self, expr, here):
        e = expr.strip()
        if e.startswith('"') and e.endswith('"'):
            return e[1:-1]
        m = FIELD_ACCESS.match(e)
        if m and m.group(1).split('::')[-1][0].isupper():
            src = self.lookup('structs', m.group(1), here)
            if src is not None:
                f = src.structs[m.group(1).split('::')[-1]]['fields'].get(m.group(2), '')
                if f.startswith('"') and f.endswith('"'):
                    return f[1:-1]
        return None

    def eval_offsets(self, expr, here):
        """All values an offset expression can take: a constant, or `m.field` / `self.field` through the
        macOS struct-literal tables of the file. Returns (values, reason) with values=[] when unresolved."""
        v = self.eval_int(expr, here)
        if v is not None:
            return [v], None
        e = expr.strip().lstrip('*&').strip()
        if e.startswith('"'):
            return [], 'string argument (not an offset)'
        index = None
        im = re.match(r'^(.*)\[(\d+)\]$', e)
        if im:
            e, index = im.group(1), int(im.group(2))
        m = FIELD_ACCESS.match(e)
        if m and not m.group(1).split('::')[-1][0].isupper():
            vals = set()
            for name, st in here.structs.items():
                if re.search(r'LINUX|WINDOWS|ELF', name):
                    continue
                if m.group(2) in st['fields']:
                    fe = st['fields'][m.group(2)]
                    if index is not None:
                        am = re.match(r'^\[(.*)\]$', fe, re.S)
                        items = split_args(am.group(1)) if am else []
                        if index >= len(items):
                            continue
                        fe = items[index]
                    x = self.eval_int(fe, here)
                    if x is not None:
                        vals.add(x)
            if vals:
                return sorted(vals), None
            return [], f'no struct-literal table in this file has field `{m.group(2)}`'
        return [], 'runtime value'

    def eval_class(self, expr, here):
        """(class name, vtable) of a class expression, or (None, reason)."""
        e = expr.strip().lstrip('&').strip()
        cm = CLASS_NEW.match(e)
        if cm:
            name, vt = self.eval_str(cm.group(1), here), self.eval_int(cm.group(2), here)
            if name is None or vt is None:
                return None, f'Class::new({cm.group(1)}, {cm.group(2)}) not constant'
            return (name, vt), None
        if PATH_NAME.match(e):
            src = self.lookup('classes', e, here)
            if src is not None:
                nexpr, vexpr = src.classes[e.split('::')[-1]]
                name, vt = self.eval_str(nexpr, src), self.eval_int(vexpr, src)
                if name is None or vt is None:
                    return None, 'class name or vtable not constant'
                return (name, vt), None
            return None, f'class constant {e} not found'
        return None, 'runtime class'


def _split_top(e, op):
    parts, depth, cur, j = [], 0, [], 0
    while j < len(e):
        ch = e[j]
        if ch in '([{':
            depth += 1
        elif ch in ')]}':
            depth -= 1
        if ch == op and depth == 0 and j > 0 and e[j - 1] == ' ':
            parts.append(''.join(cur).strip())
            cur = []
        else:
            cur.append(ch)
        j += 1
    parts.append(''.join(cur).strip())
    return parts if len(parts) > 1 and all(parts) else [e]


CALL = re.compile(r'(?<![A-Za-z0-9_])(?:(?:crate::)?xlayout::)?(fld|img)\s*\(')


def find_calls(text, names):
    """[(name, pos, [args])] for calls `name(...)` of any name in names (word boundary, not definitions)."""
    if not names:
        return []
    pat = re.compile(r'(?<![A-Za-z0-9_.])(?:(?:crate::)?xlayout::)?(' + '|'.join(map(re.escape, names)) + r')\s*\(')
    out = []
    for m in pat.finditer(text):
        before = text[max(0, m.start() - 4):m.start()]
        if before.endswith('fn ') or text[max(0, m.start() - 1):m.start()] == '|':
            continue
        end = balanced(text, m.end() - 1)
        out.append((m.group(1), m.start(), split_args(text[m.end():end - 1])))
    return out


def forwarders(src):
    """Closures and functions that pass their parameters on to fld/img.
    {name: [(definition position, callee, class arg (param index | ('fixed', expr)), offset param index)]}."""
    out = collections.defaultdict(list)
    spans = []   # body ranges of the forwarders; fld/img calls inside them are not keys
    pats = [re.compile(r'let\s+([a-z_][a-z0-9_]*)\s*=\s*(?:move\s+)?\|([^|]*)\|'),
            re.compile(r'\bfn\s+([a-z_][a-z0-9_]*)\s*(?:<[^>]*>)?\s*\(')]
    for k, pat in enumerate(pats):
        for m in pat.finditer(src.text):
            name = m.group(1)
            if name in ('fld', 'img'):
                continue
            if k == 0:
                params = [p.split(':')[0].strip() for p in split_args(m.group(2))]
                body_start = m.end()
            else:
                pend = balanced(src.text, m.end() - 1)
                params = [p.split(':')[0].strip().lstrip('&').replace('mut ', '') for p in split_args(src.text[m.end():pend - 1])]
                body_start = pend
            body = src.text[body_start:body_start + 400]
            if k == 0:
                body = body[:_stmt_end(body)]
                span = (body_start, body_start + len(body))
            else:
                b = body.find('{')
                if b < 0:
                    continue
                body = body[b:balanced(body, b, '{', '}')]
                span = (body_start + b, body_start + b + len(body))
            calls = find_calls(body, ['fld', 'img'])
            if len(calls) != 1:
                continue
            callee, _, args = calls[0]
            if callee == 'fld' and len(args) == 3:
                c = args[1].lstrip('&').strip()
                o = args[2].lstrip('*').strip()
                o = re.sub(r'\s+as\s+u64$', '', o)
                if o not in params:
                    continue
                cls = params.index(c) if c in params else ('fixed', args[1])
                out[name].append((m.start(), 'fld', cls, params.index(o)))
                spans.append(span)
            elif callee == 'img' and len(args) == 2:
                o = args[1].lstrip('*').strip()
                if o in params:
                    out[name].append((m.start(), 'img', None, params.index(o)))
                    spans.append(span)
    for v in out.values():
        v.sort()
    src.forward_spans = spans
    return dict(out)


def _stmt_end(text):
    """Index of the first ';' outside brackets (end of a let statement), or len(text)."""
    depth = 0
    for j, ch in enumerate(text):
        if ch in '([{':
            depth += 1
        elif ch in ')]}':
            depth -= 1
        elif ch == ';' and depth <= 0:
            return j
    return len(text)


def forwarder_at(defs, pos):
    """The definition in effect at pos: the nearest one before it (closures shadow), else the first."""
    before = [d for d in defs if d[0] < pos]
    return (before[-1] if before else defs[0])[1:]


def scan(root):
    """Static key scan. Returns dict(classes, fields, image, unresolved)."""
    idx = Index(root)
    classes = {}            # name -> vtable
    class_where = collections.defaultdict(set)
    fields = collections.defaultdict(set)   # class name -> {offsets}
    field_where = collections.defaultdict(set)
    image = collections.defaultdict(set)
    unresolved = []
    resolved = []           # (position, class, offsets) of the current file's fld calls
    via = collections.defaultdict(collections.Counter)
    for f in idx.files:
        for name, (nexpr, vexpr) in f.classes.items():
            cname, vt = idx.eval_str(nexpr, f), idx.eval_int(vexpr, f)
            if cname is None:
                continue
            classes.setdefault(cname, vt)
            class_where[cname].add(f'{f.rel}:{f.const_lines[name]}')
        for name, st in f.structs.items():
            # GUI registries such as panelkit::Class {name, vtable, ..} become xlayout classes via owner()
            fl = st['fields']
            if 'name' in fl and 'vtable' in fl and fl['name'].startswith('"'):
                vt = idx.eval_int(fl['vtable'], f)
                if vt is not None:
                    classes.setdefault(fl['name'].strip('"'), vt)
                    class_where[fl['name'].strip('"')].add(f'{f.rel}:{f.const_lines[name]}')
        for m in re.finditer(r'(?:(?:crate::)?xlayout::)?Class::new\(\s*"([^"]+)"\s*,\s*([^)]*?)\s*\)', f.text):
            vt = idx.eval_int(m.group(2), f)
            if vt is not None:
                classes.setdefault(m.group(1), vt)
            class_where[m.group(1)].add(f'{f.rel}:{f.line(m.start())}')
    for f in idx.files:
        if 'fld(' not in f.text and 'img(' not in f.text:
            continue
        fw = forwarders(f)
        for callee, pos, args in find_calls(f.text, ['fld', 'img'] + sorted(fw)):
            where = f'{f.rel}:{f.line(pos)}'
            if callee in ('fld', 'img'):
                kind, cls_arg, off_arg = callee, (1 if callee == 'fld' else None), (2 if callee == 'fld' else 1)
            else:
                kind, cls_arg, off_arg = forwarder_at(fw[callee], pos)
            if off_arg >= len(args):
                continue
            # a forwarder's own body: its parameters are not keys
            if callee in ('fld', 'img') and any(a <= pos < b for a, b in f.forward_spans):
                continue
            offs, why = idx.eval_offsets(args[off_arg], f)
            if why and why.startswith('string argument'):
                continue  # a same-named function that is not the forwarder
            if kind == 'img':
                if offs:
                    for v in offs:
                        image[v].add(where)
                else:
                    unresolved.append(dict(kind='image', where=where, expr=args[off_arg], reason=why))
                continue
            cexpr = cls_arg[1] if isinstance(cls_arg, tuple) else args[cls_arg] if cls_arg < len(args) else ''
            cls, cwhy = idx.eval_class(cexpr, f)
            if cls is None or not offs:
                unresolved.append(dict(kind='field', where=where, cls=cexpr, expr=args[off_arg],
                                       reason=cwhy if cls is None else why))
                continue
            name, vt = cls
            classes.setdefault(name, vt)
            for v in offs:
                fields[name].add(v)
                field_where[(name, v)].add(where)
            if callee == 'fld':
                resolved.append((pos, name, offs))
        for child, cnt in derive_via(f, resolved).items():
            via[child].update(cnt)
        resolved.clear()
    return dict(classes=classes, class_where=class_where, fields=fields, field_where=field_where,
                image=image, unresolved=unresolved, files=[f.rel for f in idx.files],
                via={c: {f'{p}+{o:#x}': n for (p, o), n in cnt.items()} for c, cnt in via.items()})


DEREF = re.compile(r'let\s+(?:mut\s+)?([a-z_][a-z0-9_]*)\s*(?::\s*u64\s*)?=\s*[^;]*?(?:\.u64|pointer)\(\s*[^;]*$', re.S)


def derive_via(f, calls):
    """Parent links read from the code: `let v = r.u64(x + fld(abi, &P, OFF)?)?;` followed by
    `v + fld(abi, &C, ..)` in the same function means C objects are reached through P+OFF.
    Returns {child class: Counter((parent class, offset))}."""
    out = collections.defaultdict(collections.Counter)
    by_pos = sorted(calls)
    for pos, parent, offs in by_pos:
        if len(offs) != 1:
            continue
        stmt_start = max(f.text.rfind(';', 0, pos), f.text.rfind('{', 0, pos), f.text.rfind('\n\n', 0, pos))
        m = DEREF.search(f.text[stmt_start + 1:pos])
        if not m:
            continue
        var = m.group(1)
        end = f.text.find('\nfn ', pos)
        end = len(f.text) if end < 0 else end
        end = min(end, pos + 4000)
        for q, child, _ in by_pos:
            if pos < q < end and child != parent:
                before = f.text[max(0, q - 60):q]
                if re.search(r'\b' + re.escape(var) + r'\s*\+\s*(?:(?:crate::)?xlayout::)?$', before):
                    out[child][(parent, offs[0])] += 1
    return out


def keys_document(sc):
    classes = {}
    for name, vt in sorted(sc['classes'].items()):
        classes[name] = dict(vtable=hex(vt) if vt is not None else None,
                             offsets=[hex(x) for x in sorted(sc['fields'].get(name, ()))],
                             declared=sorted(sc['class_where'][name])[:6])
        if sc.get('via', {}).get(name):
            classes[name]['reached_via'] = sc['via'][name]
    return dict(classes=classes,
                image={hex(a): sorted(w)[:6] for a, w in sorted(sc['image'].items())},
                unresolved=sc['unresolved'])


# ---------------------------------------------------------------------------
# Table checking (pure)
# ---------------------------------------------------------------------------

HEX_KEY = re.compile(r'^0x[0-9a-fA-F]{1,16}$')


def check_table(doc, keys=None):
    """Problems with a table under xlayout::Table::parse rules (empty list = valid). With a keys document,
    also reports coverage of the statically known keys."""
    errs = []
    if not isinstance(doc, dict):
        return ['top level is not an object'], None
    if not isinstance(doc.get('build_id'), str) or not doc.get('build_id'):
        errs.append('build_id missing or not a string')
    elif not re.fullmatch(r'[0-9a-f]{40}', doc['build_id']):
        errs.append('build_id is not a 40-hex-digit GNU build ID (xlayout only compares strings; warning)')
    image = doc.get('image', {})
    if not isinstance(image, dict):
        errs.append('image is not an object')
        image = {}
    for k, v in image.items():
        if not HEX_KEY.match(k) or not (isinstance(v, str) and HEX_KEY.match(v)):
            errs.append(f'image entry {k!r}: {v!r} is not hexadecimal "0x.." on both sides')
    fields = doc.get('fields', {})
    if not isinstance(fields, dict):
        errs.append('fields is not an object')
        fields = {}
    for cls, m in fields.items():
        if not isinstance(m, dict):
            errs.append(f'fields[{cls!r}] is not an object')
            continue
        for k, v in m.items():
            if not HEX_KEY.match(k) or not (isinstance(v, str) and HEX_KEY.match(v)):
                errs.append(f'field {cls}+{k!r}: {v!r} is not hexadecimal "0x.." on both sides')
    cov = None
    if keys is not None:
        have_img = {int(k, 16) for k in image if HEX_KEY.match(k)}
        need_img = {int(k, 16) for k in keys['image']}
        need_f = {(c, int(o, 16)) for c, d in keys['classes'].items() for o in d['offsets']}
        have_f = {(c, int(o, 16)) for c, m in fields.items() if isinstance(m, dict) for o in m if HEX_KEY.match(o)}
        cov = dict(image=f'{len(need_img & have_img)}/{len(need_img)}', fields=f'{len(need_f & have_f)}/{len(need_f)}',
                   missing_image=sorted(hex(a) for a in need_img - have_img),
                   missing_fields=sorted(f'{c}+{o:#x}' for c, o in need_f - have_f))
    return [e for e in errs], cov


# ---------------------------------------------------------------------------
# Generation (needs both binaries)
# ---------------------------------------------------------------------------

ACCEPT = {'high', 'medium'}


def mangle_class_name(name):
    """Itanium RTTI name of a C++ class written as A::B::C (None for descriptive tags like 'idler.manager')."""
    parts = name.split('::')
    if not all(re.fullmatch(r'[A-Za-z_][A-Za-z0-9_]*', p) for p in parts):
        return None
    enc = ''.join(f'{len(p)}{p}' for p in parts)
    return enc if len(parts) == 1 else f'N{enc}E'


def monotone(out):
    """Members keep their order on both ABIs: drop both entries of every adjacent pair whose ELF offsets
    do not increase (one of them is wrong and the votes cannot tell which). Returns the dropped keys."""
    dropped = []
    while True:
        pts = sorted((int(k, 16), int(v, 16)) for k, v in out.items())
        bad = {p for (a, x), (b, y) in zip(pts, pts[1:]) if y <= x for p in (a, b)}
        if not bad:
            return sorted(dropped)
        for p in bad:
            dropped.append(hex(p))
            del out[hex(p)]


def fill_gaps(out, need, res):
    """Fill needed offsets from their mapped neighbours. libc++ and libstdc++ differ only in the size of
    standard-library members, so the ELF-minus-Mach-O delta is constant between two members that bracket
    no such member: an offset between neighbours with the same delta gets that delta ('bracketed'). A
    single-vote ('low') result is accepted when its delta lies between its neighbours' deltas
    ('low-consistent'). Returns {key: how}."""
    known = {int(k, 16): int(v, 16) - int(k, 16) for k, v in out.items()}
    pts = sorted(known)
    filled = {}
    for x in need:
        if hex(x) in out:
            continue
        lo = [p for p in pts if p < x]
        hi = [p for p in pts if p > x]
        da = known[lo[-1]] if lo else None
        db = known[hi[0]] if hi else None
        r = res.get(x)
        d = r['elf'] - x if r and r.get('status') == 'mapped' and r.get('confidence') == 'low' else None
        if d is not None and r['elf'] >= 0 and (
                (da is not None and db is not None and da <= d <= db)
                or (da is None) != (db is None) and d == (da if da is not None else db)):
            out[hex(x)] = hex(r['elf'])
            filled[hex(x)] = 'low-consistent'
        elif da is not None and da == db:
            out[hex(x)] = hex(x + da)
            filled[hex(x)] = 'bracketed'
    return filled


def apply_live(table, live):
    """Entries measured on the running Linux game override the static result:
    {"image": {"0xmac": {"elf": "0x..", "evidence": ".."}},
     "fields": {"Class": {"0xmac": {"elf": "0x..", "evidence": ".."}}}}.
    Every entry must name its evidence. Returns [(key, static value, live value)] for the report."""
    changes = []
    for k, e in live.get('image', {}).items():
        if not e.get('evidence'):
            raise SystemExit(f'live image {k}: evidence required')
        changes.append((f'image {k}', table['image'].get(k), e['elf']))
        table['image'][k] = e['elf']
    for cls, m in live.get('fields', {}).items():
        for k, e in m.items():
            if not e.get('evidence'):
                raise SystemExit(f'live field {cls}+{k}: evidence required')
            changes.append((f'{cls}+{k}', table['fields'].get(cls, {}).get(k), e['elf']))
            table['fields'].setdefault(cls, {})[k] = e['elf']
    return changes


def generate(mac_path, elf_path, src, hints=None, include_low=False, cache=None, log=print, fill=True):
    accept = ACCEPT | ({'low'} if include_low else set())
    hints = hints or {}
    sc = scan(src)
    mac, elf = xmap.load_image(mac_path), xmap.load_image(elf_path)
    m = xmap.Matcher(mac, elf, cache)
    # image keys: img() arguments, every class vtable, and image literals of files that use xlayout
    image_keys = collections.defaultdict(set)
    for a, w in sc['image'].items():
        image_keys[a] |= w
    for name, vt in sc['classes'].items():
        if vt:
            image_keys[vt].add(f'class {name}')
    xfiles = [pathlib.Path(src) / f for f in sc['files']
              if re.search(r'\b(fld|img)\(', (pathlib.Path(src) / f).read_text(errors='replace'))]
    lits = xmap.scan_literals(xfiles, src)
    text_lo = min(s.addr for s in mac.text_sections()) - mac.base
    image_hi = max(s.addr + s.size for s in mac.sections) - mac.base
    starts = set(mac.starts)
    for v, where in lits.items():
        prod = [w for w in where if not w[2]]
        if not prod or not (text_lo <= v < image_hi):
            continue
        c = xmap.classify(mac, v, m.mvt_index, starts)
        if c['kind'] in ('vtable', 'global', 'function', 'typeinfo', 'cstring'):
            image_keys[v] |= {f'{f}:{n}' for f, n, _ in prod}
    table = dict(build_id=elf.identity, image={}, fields={})
    report = dict(macho=dict(file=os.path.basename(mac_path), uuid=mac.identity),
                  elf=dict(file=os.path.basename(elf_path), build_id=elf.identity),
                  accepted_confidence=sorted(accept), image={}, classes={}, unresolved_static_keys=sc['unresolved'])
    log(f'mapping {len(image_keys)} image keys')
    img_stats = collections.Counter()
    for a in sorted(image_keys):
        c = xmap.classify(mac, a, m.mvt_index, starts)
        kind = {'vtable': 'vtable', 'function': 'function', 'code': 'function', 'global': 'global',
                'typeinfo': 'global', 'cstring': 'global', 'vtable-entry': 'global'}.get(c['kind'])
        if kind is None:
            r = dict(status='unresolved', notes=[f'not mappable: {c["kind"]} ({c.get("note", "")})'])
        else:
            r = getattr(m, 'map_' + kind)(a)
        ok = r['status'] == 'mapped' and r.get('confidence') in accept
        if ok:
            table['image'][hex(a)] = hex(elf.rel(r['elf']))
        img_stats['mapped' if ok else 'unresolved'] += 1
        report['image'][hex(a)] = dict(kind=c['kind'], rtti=c.get('rtti'), status='mapped' if ok else 'unresolved',
                                       elf=hex(elf.rel(r['elf'])) if r.get('elf') is not None else None,
                                       confidence=r.get('confidence'), notes=r.get('notes', []),
                                       used=sorted(image_keys[a])[:4])
    log(f'  image: {dict(img_stats)}')
    names = sorted(set(sc['classes']) | set(sc['fields']) | set(hints))
    log(f'mapping fields of {len(names)} classes')
    specs = {}
    for name in names:
        vt = sc['classes'].get(name)
        h = hints.get(name, {})
        rtti, rtti_how = h.get('rtti'), 'hint' if h.get('rtti') else None
        if vt and not rtti:
            hit = m.mvt_index.get(mac.absolute(vt))
            rtti, rtti_how = (hit[0], 'vtable') if hit else (None, None)
        if not rtti:
            guess = mangle_class_name(name)
            if guess and guess in m.mvt:
                rtti, rtti_how = guess, 'name'   # a vtable-0 tag whose name is a real polymorphic class
        via_hint, via_how = h.get('via'), 'hint' if h.get('via') else None
        links = sc['via'].get(name, {})
        if not rtti and not via_hint and not h.get('anchors') and not h.get('context') and len(links) == 1:
            # a parent link read from the sources: `let v = r.u64(x + fld(.., &P, OFF)?)?; v + fld(.., &name, ..)`
            p, o = next(iter(links)).rsplit('+', 1)
            via_hint, via_how = dict(parent=p, offset=o), 'derived'
        specs[name] = dict(cls=rtti, rtti_how=rtti_how, anchors=[xmap.rel_macho(x) for x in h.get('anchors', [])],
                           context=[xmap.rel_macho(x) for x in h.get('context', [])], via=via_hint,
                           via_how=via_how, vt=vt)
    # classes reached through a parent's field go after the parent (its ELF field offset must be known)
    order = [n for n in names if not specs[n]['via']]
    pending = [n for n in names if specs[n]['via']]
    while pending:
        ready = [n for n in pending if specs[n]['via']['parent'] in order]
        if not ready:
            order.extend(pending)
            break
        order.extend(ready)
        pending = [n for n in pending if n not in ready]
    for name in order:
        sp, vt = specs[name], specs[name]['vt']
        rtti, anchors, context = sp['cls'], sp['anchors'], sp['context']
        need = sorted(sc['fields'].get(name, ()))
        entry = dict(vtable=hex(vt) if vt else None, rtti=rtti, rtti_from=sp['rtti_how'], keys_needed=len(need))
        via = None
        if sp['via']:
            parent, poff = sp['via']['parent'], xmap.parse_int(sp['via']['offset'])
            peoff = table['fields'].get(parent, {}).get(hex(poff))
            entry['via'] = f'{parent}+{poff:#x} ({sp["via_how"]})'
            if peoff is None or parent not in specs:
                entry.update(status='needs-owner', mapped=0,
                             reason=f'parent field {parent}+{poff:#x} is not mapped',
                             unresolved={hex(x): 'parent field unmapped' for x in need})
                report['classes'][name] = entry
                continue
            ps = specs[parent]
            via = (dict(cls=ps['cls'], anchors=ps['anchors'], context=ps['context']), poff, int(peoff, 16))
        if not rtti and not anchors and not context and via is None:
            entry.update(status='needs-owner', mapped=0,
                         reason='no vtable/RTTI and no anchor, context or via hint',
                         unresolved={hex(x): 'owner has no evidence source' for x in need})
            report['classes'][name] = entry
            continue
        if via is not None:
            res, nfun = m.map_class(via=via)
        else:
            res, nfun = m.map_class(rtti, anchors, context)
        out = {}
        for x, r in res.items():
            # negative displacements belong to a secondary base or the allocation header, not a field key
            if x >= 0 and r['status'] == 'mapped' and r['confidence'] in accept and r['elf'] >= 0:
                out[hex(x)] = hex(r['elf'])
        dropped = monotone(out)
        filled = fill_gaps(out, need, res) if fill else {}
        if dropped:
            entry['dropped_non_monotonic'] = dropped
        if filled:
            entry['filled'] = filled
        unresolved = {}
        for x in need:
            if hex(x) in out:
                continue
            r = res.get(x)
            if r is None:
                unresolved[hex(x)] = 'no matched class/anchor/context function uses this offset on the object register'
            elif r['status'] == 'mapped':
                unresolved[hex(x)] = f'only {r["confidence"]} confidence ({"; ".join(r["notes"])})'
            else:
                unresolved[hex(x)] = '; '.join(r['notes'])
        if out:
            table['fields'][name] = out
        entry.update(status='mapped', functions=nfun, mapped=len(need) - len(unresolved),
                     extra_offsets=len([k for k in out if int(k, 16) not in need]),
                     unresolved=unresolved)
        report['classes'][name] = entry
    need_f = sum(len(v) for v in sc['fields'].values())
    got_f = sum(e.get('mapped', 0) for e in report['classes'].values())
    report['summary'] = dict(
        image_keys=len(image_keys), image_mapped=len(table['image']),
        classes=len(names), classes_with_vtable=sum(1 for n in names if sc['classes'].get(n)),
        classes_needing_owner=sorted(n for n, e in report['classes'].items() if e['status'] == 'needs-owner'),
        field_keys=need_f, field_keys_mapped=got_f,
        field_entries_written=sum(len(v) for v in table['fields'].values()),
        static_keys_unresolved=len(sc['unresolved']))
    return table, report


def main(argv=None):
    argv = sys.argv[1:] if argv is None else argv
    if argv and argv[0] == '--check-table':
        p = argparse.ArgumentParser(prog='gen_linux_layout.py --check-table')
        p.add_argument('table')
        p.add_argument('--keys', help='keys.json from the `keys` command, to report coverage')
        a = p.parse_args(argv[1:])
        doc = json.loads(pathlib.Path(a.table).read_text())
        keys = json.loads(pathlib.Path(a.keys).read_text()) if a.keys else None
        errs, cov = check_table(doc, keys)
        hard = [e for e in errs if 'warning' not in e]
        for e in errs:
            print(e)
        if cov:
            print(json.dumps(cov, indent=1))
        print('valid' if not hard else f'{len(hard)} problem(s)')
        sys.exit(1 if hard else 0)
    p = argparse.ArgumentParser(description=__doc__.split('\n')[0])
    sub = p.add_subparsers(dest='cmd', required=True)
    k = sub.add_parser('keys')
    k.add_argument('--src', default='native/src')
    k.add_argument('--out')
    g = sub.add_parser('generate')
    g.add_argument('macho')
    g.add_argument('elf')
    g.add_argument('--src', default='native/src')
    g.add_argument('--hints')
    g.add_argument('--include-low', action='store_true')
    g.add_argument('--no-fill', action='store_true', help='skip the bracketed and low-consistent gap fill')
    g.add_argument('--live', help='live-measured entries applied over the generated table (see apply_live)')
    g.add_argument('--out', help='default native/layouts/linux-<build-id>.json')
    g.add_argument('--report', help='default: <out without .json>.coverage.json')
    g.add_argument('--cache', default=os.environ.get('XMAP_CACHE', str(pathlib.Path.home() / '.cache' / 'hoi4-xmap')))
    a = p.parse_args(argv)
    if a.cmd == 'keys':
        sc = scan(a.src)
        doc = keys_document(sc)
        n = sum(len(v['offsets']) for v in doc['classes'].values())
        print(f'{len(doc["classes"])} classes, {n} field keys, {len(doc["image"])} img() keys, '
              f'{len(doc["unresolved"])} unresolved call sites', file=sys.stderr)
        text = json.dumps(doc, indent=1)
        if a.out:
            pathlib.Path(a.out).write_text(text + '\n')
        else:
            print(text)
        return
    hints = json.loads(pathlib.Path(a.hints).read_text()) if a.hints else None
    table, report = generate(a.macho, a.elf, a.src, hints, a.include_low, a.cache,
                             log=lambda s: print(s, file=sys.stderr), fill=not a.no_fill)
    if a.live:
        report['live'] = apply_live(table, json.loads(pathlib.Path(a.live).read_text()))
    out = a.out or f'native/layouts/linux-{table["build_id"]}.json'
    rep = a.report or re.sub(r'\.json$', '', out) + '.coverage.json'
    pathlib.Path(out).parent.mkdir(parents=True, exist_ok=True)
    pathlib.Path(out).write_text(json.dumps(table, indent=1, sort_keys=True) + '\n')
    pathlib.Path(rep).write_text(json.dumps(report, indent=1) + '\n')
    errs, _ = check_table(table)
    print(json.dumps(report['summary'], indent=1), file=sys.stderr)
    print(f'wrote {out} and {rep}; table check: {"valid" if not [e for e in errs if "warning" not in e] else errs}',
          file=sys.stderr)


if __name__ == '__main__':
    main()
