#!/usr/bin/env python3
"""Map the macOS vtable keys the native tools use to a Windows (PE, MSVC) game executable by RTTI name.

Static and read-only: both executables are parsed as data, never run.

usage:
  pe_rtti_map.py MACHO PE [--src native/src] [--out TABLE.json] [--report REPORT.json]

MACHO is the macOS 1.19.4.1 executable whose addresses are the keys (the same input as
gen_linux_layout.py); PE is the Windows hoi4.exe. The keys are every vtable the sources name:
the `Class::new(name, vtable)` tags and the `img()` addresses the Mach-O classifies as a
vtable address point, plus `*vtable*: 0x...` values of the hand-written core layouts.

Method: the Mach-O address point gives the Itanium typeinfo name (17CCurrentGameState,
N3Foo3BarE) and its ordinal among the class's vtables. The name is rewritten in MSVC form
(.?AVCCurrentGameState@@, .?AVBar@Foo@@; class and struct both tried) and looked up in the
PE's RTTI: every vtable whose word at -8 points at a complete object locator for that type
descriptor. The primary vtable (offset to top 0) maps to the locator with object offset 0;
when the class has exactly one, the result is "high". Secondary vtables map by order of
object offset when both sides have the same number of vtables ("medium"); otherwise the
key is unresolved. Templates and anonymous namespaces are not translated (unresolved).

Globals (pointer variables, static data) and field offsets are not mapped here: MSVC code
differs too much from clang code for the instruction voting of cross_platform_map.py, so
those come from live reads of the running game (find an object through its vtable, then
read its fields against values shown on screen).

The table written has the format of xlayout::Table::parse with the PE's identity as
`build_id` (sha256 of the executable) and image-relative values (RVA).
"""
import argparse
import hashlib
import importlib.util
import json
import pathlib
import re
import struct
import sys

HERE = pathlib.Path(__file__).resolve().parent


def _load(name):
    spec = importlib.util.spec_from_file_location(name, HERE / (name + '.py'))
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


# ---------------------------------------------------------------------------
# Itanium -> MSVC type names (pure; unit tested)
# ---------------------------------------------------------------------------

def itanium_parts(name):
    """['Foo', 'Bar'] for N3Foo3BarE, ['CArmy'] for 5CArmy; None for templates, anonymous
    namespaces, std:: abbreviations or anything else that is not a plain (nested) class name."""
    def run(s, i):
        out = []
        while i < len(s) and s[i].isdigit():
            j = i
            while j < len(s) and s[j].isdigit():
                j += 1
            n = int(s[i:j])
            part = s[j:j + n]
            if len(part) != n or not re.fullmatch(r'[A-Za-z_][A-Za-z0-9_]*', part):
                return None, i
            if part.startswith('_GLOBAL__N'):
                return None, i
            out.append(part)
            i = j + n
        return out, i
    if not name:
        return None
    if name.startswith('N'):
        parts, i = run(name, 1)
        if not parts or i != len(name) - 1 or name[-1] != 'E':
            return None
        return parts
    parts, i = run(name, 0)
    if not parts or len(parts) != 1 or i != len(name):
        return None
    return parts


def msvc_names(itanium):
    """Candidate MSVC type descriptor names (class first, then struct) or []."""
    parts = itanium_parts(itanium)
    if not parts:
        return []
    body = '@'.join(reversed(parts))
    return ['.?AV%s@@' % body, '.?AU%s@@' % body]


# ---------------------------------------------------------------------------
# PE RTTI (pure parsing of the file bytes)
# ---------------------------------------------------------------------------

class PE:
    def __init__(self, path):
        with open(path, 'rb') as f:
            self.data = f.read()
        d = self.data
        e = struct.unpack_from('<I', d, 0x3C)[0]
        if d[e:e + 4] != b'PE\0\0':
            raise SystemExit('%s is not a PE file' % path)
        machine, nsec, _ts, _s, _n, optsz, _ch = struct.unpack_from('<HHIIIHH', d, e + 4)
        if machine != 0x8664:
            raise SystemExit('x64 PE required')
        opt = e + 24
        self.image_base = struct.unpack_from('<Q', d, opt + 24)[0]
        self.sections = []
        so = opt + optsz
        for i in range(nsec):
            name, vsize, vaddr, rsize, rptr = struct.unpack_from('<8sIIII', d, so + 40 * i)
            self.sections.append((name.rstrip(b'\0').decode(errors='replace'), vaddr, vsize, rptr, rsize))
        end = max(va + max(vs, rs) for _n, va, vs, _rp, rs in self.sections)
        img = bytearray(end)
        for _n, va, vs, rp, rs in self.sections:
            m = min(rs, vs)
            img[va:va + m] = d[rp:rp + m]
        self.image = bytes(img)
        self.sha256 = hashlib.sha256(d).hexdigest()

    def rtti(self):
        """{type name: [(vtable rva, object offset), ...]} sorted by object offset."""
        import numpy as np
        im = self.image
        tds = {}
        for m in re.finditer(rb'\.\?A[VU][A-Za-z0-9_@\?\$]+?@@\x00', im):
            off = m.start() - 16
            if off >= 0:
                tds[off] = m.group()[:-1].decode()
        a32 = np.frombuffer(im[:len(im) // 4 * 4], dtype=np.uint32)
        tdset = set(tds)
        cols = {}
        for i in np.nonzero(a32[:-6] == 1)[0]:
            i = int(i)
            if int(a32[i + 3]) in tdset and int(a32[i + 5]) == i * 4:
                cols[i * 4] = (int(a32[i + 1]), int(a32[i + 3]))
        a64 = np.frombuffer(im[:len(im) // 8 * 8], dtype=np.uint64)
        colva = {self.image_base + c: c for c in cols}
        keys = np.array(sorted(colva), dtype=np.uint64)
        out = {}
        for p in np.nonzero(np.isin(a64, keys))[0]:
            c = colva[int(a64[p])]
            offset, td = cols[c]
            out.setdefault(tds[td], []).append((int(p) * 8 + 8, offset))
        for v in out.values():
            v.sort(key=lambda t: (t[1], t[0]))
        return out


# ---------------------------------------------------------------------------
# Mapping (pure given the two indexes; unit tested)
# ---------------------------------------------------------------------------

def map_vtable(mac_entry, mac_points, pe_rtti):
    """mac_entry = (itanium name, ordinal, offset_to_top); mac_points = all address points of
    that class on macOS; pe_rtti = the PE index. Returns (rva or None, confidence, note)."""
    name, ordinal, top = mac_entry
    names = msvc_names(name)
    if not names:
        return None, 'unresolved', 'type name not translatable (template, anonymous namespace or std)'
    found = [n for n in names if n in pe_rtti]
    if not found:
        return None, 'unresolved', 'no MSVC RTTI for %s' % names[0]
    if len(found) > 1:
        return None, 'unresolved', 'both class and struct descriptors exist'
    pe = pe_rtti[found[0]]
    if top == 0:
        primary = [v for v, off in pe if off == 0]
        if len(primary) == 1:
            return primary[0], 'high', found[0]
        return None, 'unresolved', '%d MSVC vtables at object offset 0' % len(primary)
    if len(pe) == len(mac_points):
        return pe[ordinal][0], 'medium', '%s secondary #%d of %d' % (found[0], ordinal, len(pe))
    return None, 'unresolved', 'secondary vtable, %d on macOS vs %d on Windows' % (len(mac_points), len(pe))


def core_vtables(src):
    """{mac value: [where]} for `*vtable*: 0x...` fields of the hand-written core layouts' MACOS consts."""
    out = {}
    pat = re.compile(r'\b([a-z_]*vtable[a-z_0-9]*)\s*:\s*(0x[0-9a-fA-F]+)')
    for path in sorted(pathlib.Path(src).rglob('*.rs')):
        rel = path.relative_to(src).as_posix()
        if 'test' in rel or 'fixture' in rel:
            continue
        inside = False
        for ln, line in enumerate(path.read_text(errors='replace').splitlines(), 1):
            # only the macOS values are keys; the LINUX consts beside them are ELF values
            if re.match(r'\s*(pub(\(crate\))? )?(const|static) MACOS:', line):
                inside = True
            elif inside and line.startswith('};'):
                inside = False
            if not inside:
                continue
            for m in pat.finditer(line):
                out.setdefault(int(m.group(2), 16), []).append('%s:%d %s' % (rel, ln, m.group(1)))
    return out


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__.split('\n')[0])
    ap.add_argument('macho')
    ap.add_argument('pe')
    ap.add_argument('--src', default=str(HERE.parent / 'native' / 'src'))
    ap.add_argument('--out')
    ap.add_argument('--report')
    a = ap.parse_args(argv)
    xmap = _load('cross_platform_map')
    gen = _load('gen_linux_layout')
    keys = gen.keys_document(gen.scan(a.src))
    mac = xmap.load_image(a.macho)
    points, index = xmap.vtables(mac)
    wanted = {}
    for cls, info in keys['classes'].items():
        v = int(info['vtable'], 16)
        if v:
            wanted.setdefault(v, []).append('Class ' + cls)
    for addr, where in keys['image'].items():
        wanted.setdefault(int(addr, 16), []).extend(where)
    for v, where in core_vtables(a.src).items():
        wanted.setdefault(v, []).extend(where)
    pe = PE(a.pe)
    rtti = pe.rtti()
    image, report = {}, []
    counts = {'high': 0, 'medium': 0, 'unresolved': 0, 'not_vtable': 0}
    for v in sorted(wanted):
        rel = v - 0x100000000 if v >= 0x100000000 else v
        entry = index.get(mac.absolute(rel))
        row = {'mac': '0x%x' % rel, 'used': sorted(set(wanted[v]))[:6]}
        if entry is None:
            row.update(kind='global_or_other', confidence='not_vtable',
                       note='not a vtable address point on macOS; needs code matching or a live read')
            counts['not_vtable'] += 1
        else:
            name, ordinal, top = entry
            rva, conf, note = map_vtable(entry, points[name], rtti)
            row.update(kind='vtable', itanium=name, ordinal=ordinal, offset_to_top=top,
                       confidence=conf, note=note)
            counts[conf] += 1
            if rva is not None:
                row['windows'] = '0x%x' % rva
                image['0x%x' % rel] = '0x%x' % rva
        report.append(row)
    summary = {'pe_sha256': pe.sha256, 'pe_image_base': '0x%x' % pe.image_base,
               'rtti_types': len(rtti), 'keys': len(wanted), 'counts': counts}
    print(json.dumps(summary, indent=1))
    if a.out:
        pathlib.Path(a.out).write_text(json.dumps(
            {'build_id': pe.sha256, 'image': image, 'fields': {}}, indent=1, sort_keys=True) + '\n')
    if a.report:
        pathlib.Path(a.report).write_text(json.dumps({'summary': summary, 'items': report}, indent=1) + '\n')
    return 0


if __name__ == '__main__':
    sys.exit(main())
