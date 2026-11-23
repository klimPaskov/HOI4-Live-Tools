#!/usr/bin/env python3
"""Bring up a new HOI4 build: identify the executable, map every layout key to it, check the result.

The native runtime's tools address game memory through layout keys: the image addresses (vtables,
globals, functions) and struct field offsets of the canonical macOS build (see native/src/xlayout.rs).
A table maps those keys to one other executable. This script makes that table for any build, macOS
(Mach-O), Linux (ELF) or Windows (PE: identification only so far), from the game files alone. The
executables are read as data and never run.

usage:
  game_migrate.py identify EXE
      system, format, build id, SHA-256 and game version (from launcher-settings.json beside the game)
  game_migrate.py migrate NEW_EXE --canonical MACHO [--src native/src] [--hints H.json] [--live L.json]
                  [--out-dir native/layouts] [--cache DIR]
      map every key from the canonical Mach-O to NEW_EXE (the matcher in cross_platform_map.py: RTTI
      vtables, string-anchored and callee-propagated function pairs, then field displacements voted on
      the object register), then run the static check. Writes <system>-<build id>.json (the table, with a
      confidence per entry) and <system>-<build id>.coverage.json (evidence, unresolved keys with reasons,
      modules). Same-system targets (a new macOS build) match far better than cross-system ones.
      --previous TABLE (the same system's table for the build before, with its .coverage.json) carries its
      field values forward into classes whose mapped fields did not move between the two builds
      (confidence "carried"), and its live-measured entries over static votes in such classes.
  game_migrate.py check TABLE NEW_EXE [--canonical MACHO] [--src native/src]
      static self-check of a table against its executable (sections, vtables, code, field order)
  game_migrate.py modules TABLE [--src native/src]
      per source file: keys needed, keys mapped, enabled (every key mapped) or the keys it lacks
  game_migrate.py refcheck [--tables native/layouts] [--refs DIR] [--canonical MACHO]
      the static check of every committed table whose executable is in the private reference folder
      (DIR/<system>-<build id>/ holding the game executable); tables without a reference are listed as
      skipped, so the command also runs where no game file may be kept (CI)
  game_migrate.py rekey TABLE MIGRATION.json [--write]
      when the canonical build moves (the macOS sources are rewritten to a new build), give another
      system's table the new keys: every image key and field key goes through the migration map
      (native/layouts/migrations/<from>-to-<to>.json); a key the map lacks is dropped and listed
  game_migrate.py score TABLE TRUTH.json
      compare a table with known answers {"0xkey": {"0xvalue": n}} (e.g. a hand-made migration)

Exit status: 0 when the table passed the static check, 1 when it has problems, 2 on usage errors.
Reference executables are never committed: keep them in a private folder (HOI4_REFERENCE_DIR,
default ~/.cache/hoi4-live/reference/<system>-<build id>/) and pass their paths.
"""
import argparse
import collections
import hashlib
import importlib.util
import json
import os
import pathlib
import re
import struct
import sys
from array import array

HERE = pathlib.Path(__file__).resolve().parent


def _load(name, file):
    spec = importlib.util.spec_from_file_location(name, HERE / file)
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


xmap = _load('cross_platform_map', 'cross_platform_map.py')
gen = _load('gen_linux_layout', 'gen_linux_layout.py')

# The build whose values are the layout keys (native/src/platform/macos.rs BUILD_UUID).
CANONICAL = 'DFA875A5-25BA-35E0-9736-627F8A5C4396'
CONFIDENCE_RANK = {'high': 3, 'medium': 2, 'carried': 2, 'filled': 1, 'low': 0, 'live': 4}


# ---------------------------------------------------------------------------
# identify
# ---------------------------------------------------------------------------

def pe_identity(data):
    """(machine, CodeView GUID+age) of a PE image, or None. The GUID+age is what symbol servers key
    PDBs by, so it is the PE counterpart of the Mach-O UUID and the GNU build ID."""
    if data[:2] != b'MZ' or len(data) < 0x40:
        return None
    pe = struct.unpack_from('<I', data, 0x3c)[0]
    if data[pe:pe + 4] != b'PE\0\0':
        return None
    machine, nsect, _, _, _, optsize, _ = struct.unpack_from('<HHIIIHH', data, pe + 4)
    opt = pe + 24
    magic = struct.unpack_from('<H', data, opt)[0]
    if magic != 0x20b:          # PE32+ only: the game is 64-bit
        return machine, None
    ndirs = struct.unpack_from('<I', data, opt + 108)[0]
    if ndirs <= 6:
        return machine, None
    dbg_rva, dbg_size = struct.unpack_from('<II', data, opt + 112 + 6 * 8)
    sections = []
    so = opt + optsize
    for k in range(nsect):
        vsize, va, rawsize, rawptr = struct.unpack_from('<IIII', data, so + 40 * k + 8)
        sections.append((va, max(vsize, rawsize), rawptr))

    def off(rva):
        for va, size, raw in sections:
            if va <= rva < va + size:
                return raw + rva - va
        return None
    o = off(dbg_rva) if dbg_rva else None
    for k in range(dbg_size // 28 if o is not None else 0):
        typ, size, _, ptr = struct.unpack_from('<IIII', data, o + 28 * k + 12)
        if typ == 2 and data[ptr:ptr + 4] == b'RSDS':
            g = data[ptr + 4:ptr + 20]
            age = struct.unpack_from('<I', data, ptr + 20)[0]
            d1, d2, d3 = struct.unpack_from('<IHH', g, 0)
            guid = f'{d1:08X}{d2:04X}{d3:04X}{g[8:].hex().upper()}'
            return machine, f'{guid}{age:X}'
    return machine, None


def pe_sections(data):
    """(image base, [(name, rva, size, file offset)]) of a PE32+ image."""
    pe = struct.unpack_from('<I', data, 0x3c)[0]
    nsect, optsize = struct.unpack_from('<H', data, pe + 6)[0], struct.unpack_from('<H', data, pe + 20)[0]
    base = struct.unpack_from('<Q', data, pe + 24 + 24)[0]
    so = pe + 24 + optsize
    out = []
    for k in range(nsect):
        name = data[so + 40 * k:so + 40 * k + 8].rstrip(b'\0').decode(errors='replace')
        vsize, va, rawsize, rawptr = struct.unpack_from('<IIII', data, so + 40 * k + 8)
        out.append((name, va, min(vsize, rawsize) if rawsize else 0, rawptr))
    return base, out


def msvc_name(itanium):
    """'17CCurrentGameState' -> '.?AVCCurrentGameState@@'; 'N3Foo3BarE' -> '.?AVBar@Foo@@' (None if not simple)."""
    s = itanium[1:-1] if itanium.startswith('N') and itanium.endswith('E') else itanium
    parts = []
    while s:
        m = re.match(r'(\d+)', s)
        if not m:
            return None
        k, s = int(m.group(1)), s[len(m.group(1)):]
        parts.append(s[:k])
        s = s[k:]
    return '.?AV' + '@'.join(reversed(parts)) + '@@' if parts else None


def pe_vtables(data):
    """MSVC RTTI of a PE32+ image: {type name: [(this offset, vtable rva)]}. A complete object locator
    (signature 1, type descriptor rva, self rva) is found for each type descriptor; the vtable starts one
    pointer after the slot holding the locator's address."""
    base, sects = pe_sections(data)
    rd = [s for s in sects if s[0] in ('.rdata', '.data') and s[2]]
    tds = {}
    for name, va, size, off in sects:
        if not size:
            continue
        blob = data[off:off + size]
        for m in re.finditer(rb'\.\?A[VU][^\0]{1,400}?@@\0', blob):
            tds[va + m.start() - 0x10] = m.group(0)[:-1].decode(errors='replace').replace('.?AU', '.?AV', 1)
    cols = {}
    for name, va, size, off in rd:
        words = array('I', data[off:off + size - size % 4])
        for i in range(len(words) - 5):
            if words[i] == 1 and words[i + 5] == va + 4 * i and words[i + 3] in tds:
                cols[base + va + 4 * i] = (tds[words[i + 3]], words[i + 1])
    out = collections.defaultdict(list)
    for name, va, size, off in rd:
        q = array('Q', data[off:off + size - size % 8])
        for i, v in enumerate(q):
            hit = cols.get(v)
            if hit:
                out[hit[0]].append((hit[1], va + 8 * i + 8))
    return out


def pe_table(new_exe, canonical, src):
    """The part of a Windows table that needs no disassembler: every vtable key whose class has an MSVC
    RTTI vtable at this offset 0 (primary address points only). Globals, functions and fields stay
    unresolved until the matcher reads PE code (x64 MSVC: the object is in rcx)."""
    data = pathlib.Path(new_exe).read_bytes()
    vt = pe_vtables(data)
    mac = xmap.load_image(canonical)
    _, mindex = xmap.vtables(mac)
    sc = gen.scan(src)
    keys = set(sc['image']) | {v for v in sc['classes'].values() if v}
    table, report = dict(image={}, fields={}), dict(image={}, classes={})
    for k in sorted(keys):
        hit = mindex.get(mac.absolute(k))
        if hit is None:
            report['image'][hex(k)] = dict(kind='other', status='unresolved', notes=['not a vtable: needs PE code matching'])
            continue
        rtti, ordinal, top = hit
        ms = msvc_name(rtti)
        prim = [v for off, v in vt.get(ms, []) if off == 0]
        if ordinal == 0 and top == 0 and len(prim) == 1:
            table['image'][hex(k)] = hex(prim[0])
            report['image'][hex(k)] = dict(kind='vtable', rtti=rtti, status='mapped', confidence='high',
                                          notes=[f'MSVC RTTI {ms}'])
        else:
            why = 'secondary vtable' if ordinal or top else f'{len(prim)} primary MSVC vtables for {ms}'
            report['image'][hex(k)] = dict(kind='vtable', rtti=rtti, status='unresolved', notes=[why])
    report['summary'] = dict(image_keys=len(keys), image_mapped=len(table['image']),
                             field_keys=sum(len(v) for v in sc['fields'].values()), field_keys_mapped=0)
    return table, report


def game_version(exe):
    """rawVersion and version from launcher-settings.json in the game folder, when it is there."""
    p = pathlib.Path(exe).resolve()
    for d in [p.parent] + list(p.parents)[:4]:
        f = d / 'launcher-settings.json'
        if f.is_file():
            try:
                doc = json.loads(f.read_text(encoding='utf-8-sig'))
                return doc.get('rawVersion'), doc.get('version')
            except (OSError, ValueError):
                return None, None
    return None, None


def identify(exe):
    data = pathlib.Path(exe).read_bytes()
    out = dict(file=str(exe), sha256=hashlib.sha256(data).hexdigest(), size=len(data))
    if data[:4] == b'\x7fELF':
        out.update(system='linux', format='elf', build_id=xmap.ELF(exe).identity)
    elif data[:4] == b'\xcf\xfa\xed\xfe':
        out.update(system='macos', format='macho', build_id=xmap.MachO(exe).identity)
    elif data[:2] == b'MZ':
        pe = pe_identity(data)
        out.update(system='windows', format='pe', build_id=pe[1] if pe else None,
                   machine=hex(pe[0]) if pe else None)
    else:
        raise SystemExit(f'{exe}: not a Mach-O, ELF or PE executable')
    raw, label = game_version(exe)
    out.update(game_version=raw, version_label=label, canonical=out['build_id'] == CANONICAL)
    return out


def table_name(ident):
    return f'{ident["system"]}-{ident["build_id"]}'


# ---------------------------------------------------------------------------
# static self-check
# ---------------------------------------------------------------------------

def static_check(table, report, new, canonical=None):
    """Problems that show a table entry cannot be right for executable `new` (an Image). Image entries
    must land in the section kind their key had on the canonical build: a vtable key on an RTTI vtable
    address point of the same class, a function key on a function start, a global inside a data
    section. With the canonical image of the same format, every entry must also land in the section of
    the same name as its key (a global must not become the GOT slot its users load it through). Field
    entries must keep their members' order inside each class and stay below 1 MiB.
    Returns (problems, counts)."""
    problems, counts = [], collections.Counter()
    _, vindex = xmap.vtables(new)
    starts = set(new.starts)
    kinds = report.get('image', {})
    for k, v in sorted(table.get('image', {}).items()):
        a = new.absolute(int(v, 16))
        sect = new.section_of(a)
        kind = (kinds.get(k) or {}).get('kind')
        counts['image'] += 1
        same = canonical is not None and canonical.kind == new.kind
        ksect = canonical.section_of(canonical.absolute(int(k, 16))) if same else None
        if sect is None:
            problems.append(f'image {k} -> {v}: outside every section')
        elif ksect is not None and (ksect.seg, ksect.name) != (sect.seg, sect.name):
            problems.append(f'image {k} -> {v}: in {sect}, the key is in {ksect}')
        elif kind == 'vtable':
            want = (kinds.get(k) or {}).get('rtti')
            hit = vindex.get(a)
            if hit is None:
                problems.append(f'image {k} -> {v}: not a vtable address point')
            elif want and hit[0] != want:
                problems.append(f'image {k} -> {v}: vtable of {hit[0]}, the key is {want}')
            elif not new.is_code(new.ptr(a) or 0) and not new.sym_at(a):
                problems.append(f'image {k} -> {v}: first vtable slot is not code')
        elif kind in ('function', 'code'):
            if sect.kind != 'text':
                problems.append(f'image {k} -> {v}: code key outside the text section ({sect})')
            elif kind == 'function' and starts and a not in starts:
                problems.append(f'image {k} -> {v}: not a function start')
        elif kind in ('global', 'typeinfo', 'vtable-entry'):
            if sect.kind == 'text':
                problems.append(f'image {k} -> {v}: data key inside the text section')
    for cls, m in sorted(table.get('fields', {}).items()):
        pairs = sorted((int(k, 16), int(v, 16)) for k, v in m.items())
        counts['fields'] += len(pairs)
        for (k1, v1), (k2, v2) in zip(pairs, pairs[1:]):
            if v2 <= v1:
                problems.append(f'field {cls}: {k1:#x}->{v1:#x} and {k2:#x}->{v2:#x} change the member order')
        for k, v in pairs:
            if v >= 0x100000:
                problems.append(f'field {cls}+{k:#x} -> {v:#x}: beyond any object this game allocates')
            if k % 8 == 0 and v % 4:
                problems.append(f'field {cls}+{k:#x} -> {v:#x}: an aligned member became misaligned')
    return problems, counts


# ---------------------------------------------------------------------------
# confidence, modules, score
# ---------------------------------------------------------------------------

def confidence(table, report):
    """{"image": {key: c}, "fields": {class: {key: c}}} for every written entry. c is the matcher's
    vote (high, medium), "filled" (bracketed by neighbours with one shift) or "live" (measured)."""
    out = dict(image={}, fields={})
    changed = [c[0] for c in report.get('live', []) or []]
    live_img = {c.split(' ', 1)[1] for c in changed if c.startswith('image ')}
    live = {tuple(c.split('+', 1)) for c in changed if not c.startswith('image ')}
    for k in table.get('image', {}):
        got = (report.get('image', {}).get(k) or {}).get('confidence')
        out['image'][k] = 'live' if k in live_img else (got or 'live')
    for cls, m in table.get('fields', {}).items():
        got = (report.get('classes', {}).get(cls) or {}).get('confidence', {})
        out['fields'][cls] = {k: ('live' if (cls, k) in live else got.get(k, 'live')) for k in m}
    return out


def carry_forward(table, conf, prev, prev_live=(), keys=None):
    """Carry the previous build's field values into the new table, class by class, where the evidence says
    the class did not change: every field both tables map (live-measured previous entries aside) has the same
    value, and at least two do. Then a previous live entry replaces the new static vote, and a field the new
    run left unmapped takes the previous value (confidence "carried"). With `keys`, only needed fields are
    carried. Returns {class: {"agree": n, "carried": [...], "live": [...]}} for the classes carried."""
    live = set(prev_live)
    need = None
    if keys is not None:
        need = {(c, o) for c, d in keys['classes'].items() for o in d['offsets']}
    out = {}
    for cls, old in sorted(prev.get('fields', {}).items()):
        new = table.get('fields', {}).get(cls, {})
        shared = [k for k in old if k in new and f'{cls}+{k}' not in live]
        if len(shared) < 2 or any(old[k] != new[k] for k in shared):
            continue
        row = dict(agree=len(shared), carried=[], live=[])
        for k, v in sorted(old.items()):
            if need is not None and (cls, k) not in need:
                continue
            if f'{cls}+{k}' in live:
                if new.get(k) != v:
                    row['live'].append(k)
                new[k] = v
                conf['fields'].setdefault(cls, {})[k] = 'live'
            elif k not in new:
                new[k] = v
                conf['fields'].setdefault(cls, {})[k] = 'carried'
                row['carried'].append(k)
        if row['carried'] or row['live']:
            table.setdefault('fields', {})[cls] = new
            out[cls] = row
    return out


def prune_extras(table, conf, keys):
    """Drop field entries no tool reads (extra displacements the matcher saw) where they break member order
    with an entry a tool does read: a live-corrected key and a wrong static extra cannot both stand, and the
    key is the one with evidence. Returns ["Class+0x..", ...] removed."""
    need = {(c, o) for c, d in keys['classes'].items() for o in d['offsets']}
    removed = []
    for cls, m in table.get('fields', {}).items():
        kept = [(int(k, 16), int(v, 16)) for k, v in m.items() if (cls, k) in need]
        for k, v in list(m.items()):
            if (cls, k) in need:
                continue
            x, y = int(k, 16), int(v, 16)
            if any((a < x and b >= y) or (a > x and b <= y) or a == x for a, b in kept):
                del m[k]
                conf.get('fields', {}).get(cls, {}).pop(k, None)
                removed.append(f'{cls}+{k}')
    return sorted(removed)


def modules(table, keys):
    """Per source file: how many of its keys the table maps; a module is enabled when all are."""
    need = collections.defaultdict(lambda: [set(), set()])
    for a, where in keys['image'].items():
        for w in where:
            need[w.split(':')[0]][0].add(a)
    for cls, d in keys['classes'].items():
        for w in d.get('declared', []):
            need[w.split(':')[0]][1].update(f'{cls}+{o}' for o in d['offsets'])
    have_img = set(table.get('image', {}))
    have_f = {f'{c}+{o}' for c, m in table.get('fields', {}).items() for o in m}
    out = {}
    for f, (imgs, flds) in sorted(need.items()):
        miss = sorted(imgs - have_img) + sorted(flds - have_f)
        out[f] = dict(keys=len(imgs) + len(flds), mapped=len(imgs) + len(flds) - len(miss),
                      enabled=not miss, missing=miss[:12])
    return out


def score(table, truth):
    """Compare table values with known answers. Field keys are scored per value: a key seen with several
    answers (one offset in several classes) is right when the table's value is any of them."""
    res = collections.Counter()
    wrong = []
    entries = [('image', k, v) for k, v in table.get('image', {}).items()]
    entries += [(cls, k, v) for cls, m in table.get('fields', {}).items() for k, v in m.items()]
    for where, k, v in entries:
        answers = truth.get(k)
        if not answers:
            res['no answer'] += 1
        elif v in answers:
            res['right'] += 1
        else:
            res['wrong'] += 1
            wrong.append(f'{where} {k}: table {v}, known {sorted(answers)}')
    return dict(res), wrong


# ---------------------------------------------------------------------------
# migrate
# ---------------------------------------------------------------------------

def migrate(new_exe, canonical, src, hints=None, live=None, out_dir='native/layouts', cache=None, log=print,
            previous=None):
    ident = identify(new_exe)
    if identify(canonical)['build_id'] != CANONICAL:
        raise SystemExit(f'{canonical} is not the canonical macOS build {CANONICAL}')
    if ident['format'] == 'pe':
        # vtables by MSVC RTTI only; the static check below needs the matcher's image model, so it is skipped
        table, report = pe_table(new_exe, canonical, src)
        table.update(build_id=ident['build_id'], system='windows', sha256=ident['sha256'],
                     game_version=ident['game_version'])
        table['confidence'] = confidence(table, report)
        keys = gen.keys_document(gen.scan(src))
        mods = modules(table, keys)
        report.update(target=dict(ident, file=os.path.basename(ident['file'])), modules=mods,
                      static_check=dict(passed=True, problems=[], checked={}, note='PE: not implemented'))
        report['summary'].update(modules=len(mods), modules_enabled=sum(1 for m in mods.values() if m['enabled']))
        out = pathlib.Path(out_dir) / f'{table_name(ident)}.json'
        out.parent.mkdir(parents=True, exist_ok=True)
        out.write_text(json.dumps(table, indent=1, sort_keys=True) + '\n')
        rep = out.with_name(f'{table_name(ident)}.coverage.json')
        rep.write_text(json.dumps(report, indent=1) + '\n')
        return out, rep, report
    table, report = gen.generate(canonical, new_exe, src, hints, False, cache, log=log)
    if live:
        report['live'] = gen.apply_live(table, live)
    table.update(system=ident['system'], sha256=ident['sha256'], game_version=ident['game_version'])
    table['confidence'] = confidence(table, report)
    keys = gen.keys_document(gen.scan(src))
    if previous:
        prev = json.loads(pathlib.Path(previous).read_text())
        if prev.get('system', ident['system']) != ident['system']:
            raise SystemExit(f'{previous} is a {prev.get("system")} table, the executable is {ident["system"]}')
        rp = pathlib.Path(re.sub(r'\.json$', '', str(previous)) + '.coverage.json')
        prev_live = [c[0] for c in json.loads(rp.read_text()).get('live', [])] if rp.exists() else []
        report['carried'] = dict(previous=pathlib.Path(previous).name,
                                 classes=carry_forward(table, table['confidence'], prev, prev_live, keys))
        need = {(c, o) for c, d in keys['classes'].items() for o in d['offsets']}
        report['summary']['field_keys_mapped_with_previous'] = sum(
            1 for c, o in need if o in table.get('fields', {}).get(c, {}))
    report['pruned_extras'] = prune_extras(table, table['confidence'], keys)
    new = xmap.load_image(new_exe)
    problems, counts = static_check(table, report, new, xmap.load_image(canonical))
    mods = modules(table, keys)
    report.update(target=dict(ident, file=os.path.basename(ident['file'])), static_check=dict(problems=problems, checked=dict(counts),
                                                 passed=not problems), modules=mods)
    report['summary']['modules_enabled'] = sum(1 for m in mods.values() if m['enabled'])
    report['summary']['modules'] = len(mods)
    name = table_name(ident)
    out = pathlib.Path(out_dir) / f'{name}.json'
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(json.dumps(table, indent=1, sort_keys=True) + '\n')
    rep = out.with_name(f'{name}.coverage.json')
    rep.write_text(json.dumps(report, indent=1) + '\n')
    return out, rep, report


def rekey(table, migration):
    """(new table, dropped keys): `table`'s keys translated through a migration map
    {"image": {"0xold": {"new": "0xnew", ...}}, "fields": {"Class": {"0xold": {"new": ...}}}}. Values and
    per-entry confidence stay; the confidence block is re-keyed with them. With "fields_default": "identity"
    a field key the map does not list keeps its value as key (a build whose structures did not move)."""
    out = {k: v for k, v in table.items() if k not in ('image', 'fields', 'confidence')}
    out['image'], out['fields'], dropped = {}, {}, []
    conf = table.get('confidence') or {}
    nconf = dict(image={}, fields={})
    img = migration.get('image', {})
    for k, v in table.get('image', {}).items():
        e = img.get(k)
        if not e or not e.get('new'):
            dropped.append(f'image {k}')
            continue
        out['image'][e['new']] = v
        if k in conf.get('image', {}):
            nconf['image'][e['new']] = conf['image'][k]
    for cls, m in table.get('fields', {}).items():
        fm = migration.get('fields', {}).get(cls, {})
        for k, v in m.items():
            e = fm.get(k)
            if (not e or not e.get('new')) and migration.get('fields_default') == 'identity':
                e = dict(new=k)     # the migration found the structures unchanged
            if not e or not e.get('new'):
                dropped.append(f'{cls}+{k}')
                continue
            out['fields'].setdefault(cls, {})[e['new']] = v
            c = conf.get('fields', {}).get(cls, {}).get(k)
            if c:
                nconf['fields'].setdefault(cls, {})[e['new']] = c
    if conf:
        out['confidence'] = nconf
    return out, dropped


def reference_dir():
    return pathlib.Path(os.environ.get('HOI4_REFERENCE_DIR') or pathlib.Path.home() / '.cache' / 'hoi4-live' / 'reference')


def find_reference(refs, name):
    """The executable inside refs/<name>/ (the only regular file larger than 1 MiB), or None."""
    d = pathlib.Path(refs) / name
    if not d.is_dir():
        return None
    files = [f for f in sorted(d.iterdir()) if f.is_file() and f.stat().st_size > 1 << 20]
    return files[0] if len(files) == 1 else None


def refcheck(tables, refs, canonical=None):
    """[{table, status, problems}] for every table in `tables` (status checked/failed/skipped)."""
    out = []
    canon = xmap.load_image(canonical) if canonical else None
    for path in sorted(pathlib.Path(tables).glob('*.json')):
        if path.name.endswith('.coverage.json'):
            continue
        table = json.loads(path.read_text())
        name = path.stem if '-' in path.stem else None
        exe = find_reference(refs, name) if name else None
        if exe is None:
            out.append(dict(table=path.name, status='skipped', reason=f'no reference executable in {refs}/{name}/'))
            continue
        ident = identify(exe)
        if ident['build_id'] != table.get('build_id') or table.get('sha256') not in (None, ident['sha256']):
            out.append(dict(table=path.name, status='failed', problems=[f'{exe} is build {ident["build_id"]}']))
            continue
        if ident['format'] == 'pe':
            out.append(dict(table=path.name, status='skipped', reason='the static check reads Mach-O and ELF only'))
            continue
        rp = path.with_name(path.stem + '.coverage.json')
        report = json.loads(rp.read_text()) if rp.exists() else {}
        problems, counts = static_check(table, report, xmap.load_image(str(exe)), canon)
        out.append(dict(table=path.name, status='failed' if problems else 'checked', checked=dict(counts),
                        problems=problems[:20]))
    return out


def main(argv=None):
    p = argparse.ArgumentParser(description=__doc__.split('\n')[0])
    sub = p.add_subparsers(dest='cmd', required=True)
    s = sub.add_parser('identify')
    s.add_argument('exe')
    s = sub.add_parser('migrate')
    s.add_argument('new')
    s.add_argument('--canonical', required=True, help='the canonical macOS executable (the key space)')
    s.add_argument('--src', default='native/src')
    s.add_argument('--hints', default=str(HERE / 'data' / 'xlayout-hints-1.19.4.1.json'))
    s.add_argument('--live')
    s.add_argument('--previous', help='the same system\'s table for the previous build (carry unchanged classes)')
    s.add_argument('--out-dir', default='native/layouts')
    s.add_argument('--cache', default=os.environ.get('XMAP_CACHE', str(pathlib.Path.home() / '.cache' / 'hoi4-xmap')))
    s = sub.add_parser('check')
    s.add_argument('table')
    s.add_argument('new')
    s.add_argument('--report', help='default: the .coverage.json beside the table')
    s.add_argument('--canonical', help='the canonical macOS executable: adds the same-section check')
    s = sub.add_parser('modules')
    s.add_argument('table')
    s.add_argument('--src', default='native/src')
    s = sub.add_parser('rekey')
    s.add_argument('table')
    s.add_argument('migration')
    s.add_argument('--write', action='store_true')
    s = sub.add_parser('refcheck')
    s.add_argument('--tables', default='native/layouts')
    s.add_argument('--refs', default=str(reference_dir()))
    s.add_argument('--canonical')
    s = sub.add_parser('score')
    s.add_argument('table')
    s.add_argument('truth')
    a = p.parse_args(argv)
    if a.cmd == 'identify':
        print(json.dumps(identify(a.exe), indent=1))
        return 0
    if a.cmd == 'migrate':
        hints = json.loads(pathlib.Path(a.hints).read_text()) if a.hints else None
        live = json.loads(pathlib.Path(a.live).read_text()) if a.live else None
        out, rep, report = migrate(a.new, a.canonical, a.src, hints, live, a.out_dir, a.cache,
                                   log=lambda m: print(m, file=sys.stderr), previous=a.previous)
        print(json.dumps(dict(table=str(out), report=str(rep), summary=report['summary'],
                              static_check=dict(passed=report['static_check']['passed'],
                                                problems=report['static_check']['problems'][:20])), indent=1))
        return 0 if report['static_check']['passed'] else 1
    if a.cmd == 'rekey':
        new, dropped = rekey(json.loads(pathlib.Path(a.table).read_text()),
                             json.loads(pathlib.Path(a.migration).read_text()))
        if a.write:
            pathlib.Path(a.table).write_text(json.dumps(new, indent=1, sort_keys=True) + '\n')
        print(json.dumps(dict(image=len(new['image']), fields=sum(len(m) for m in new['fields'].values()),
                              dropped=dropped), indent=1))
        return 0 if not dropped else 1
    if a.cmd == 'refcheck':
        res = refcheck(a.tables, a.refs, a.canonical)
        print(json.dumps(res, indent=1))
        return 1 if any(r['status'] == 'failed' for r in res) else 0
    table = json.loads(pathlib.Path(a.table).read_text())
    if a.cmd == 'check':
        rp = pathlib.Path(a.report) if a.report else pathlib.Path(re.sub(r'\.json$', '', a.table) + '.coverage.json')
        report = json.loads(rp.read_text()) if rp.exists() else {}
        canonical = xmap.load_image(a.canonical) if a.canonical else None
        problems, counts = static_check(table, report, xmap.load_image(a.new), canonical)
        print(json.dumps(dict(checked=dict(counts), problems=problems), indent=1))
        return 0 if not problems else 1
    if a.cmd == 'modules':
        mods = modules(table, gen.keys_document(gen.scan(a.src)))
        print(json.dumps(dict(enabled=sum(m['enabled'] for m in mods.values()), modules=len(mods),
                              files=mods), indent=1))
        return 0
    if a.cmd == 'score':
        res, wrong = score(table, json.loads(pathlib.Path(a.truth).read_text()))
        print(json.dumps(dict(result=res, wrong=wrong[:40]), indent=1))
        return 0
    return 2


if __name__ == '__main__':
    sys.exit(main())
