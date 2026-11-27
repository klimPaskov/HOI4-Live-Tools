#!/usr/bin/env python3
"""Map image addresses and struct field offsets of one Linux HOI4 ELF build to another (static, read-only).

usage:
  linux_build_migrate.py OLD_ELF NEW_ELF [--src PATH ...] [--classes C.json] [--out report.json] [--cache DIR]
  linux_build_migrate.py OLD_ELF NEW_ELF --table native/layouts/linux-OLD.json --out-table linux-NEW.json
                         [--keys-src native/src]

The same-platform counterpart of macos_build_migrate.py, built on cross_platform_map.Matcher with the old
ELF in place of the Mach-O. Both binaries are read as data and never run.

- Image literals: every hex literal in the Rust sources under --src (production code only, see
  cross_platform_map.scan_literals) that is a vtable address point, a global or a function start of the old
  ELF is mapped to the new ELF: vtables by RTTI name and address-point ordinal, globals and functions by
  matched code.
- Fields: C.json lists classes, {"Name": {"vtable": "0x..", "context": ["0x.."], "anchors": ["0x.."]}}
  with old-build values; each is mapped class-wide (every displacement its matched functions take on the
  object register), as gen_linux_layout.py does across platforms.

- Table (--table): every value of a generated xlayout table for the old build (macOS key -> old ELF) is
  moved to the new build: image values as vtables, functions or globals as above; the fields of each class
  class-wide through the class's old ELF vtable (the class's macOS vtable from the sources, looked up in the
  table's image section). Keys stay the macOS values the sources use, so the result is the new build's
  table. A value that does not map is left out (the tools then refuse that read with layout_unmapped);
  --out-table writes only mapped entries, the report lists the rest.

Only high and medium confidence counts as mapped; anything else keeps its tallies and is never guessed.
"""
import argparse
import collections
import importlib.util
import json
import os
import pathlib
import sys

HERE = pathlib.Path(__file__).resolve().parent
_spec = importlib.util.spec_from_file_location('cross_platform_map', HERE / 'cross_platform_map.py')
xmap = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(xmap)

ACCEPT = {'high', 'medium'}


def literal_items(old, paths, root, mvt_index, all_lines=False):
    """Image literals of the old ELF used by production code (with all_lines also test fixtures):
    [(value, classification, sources)]."""
    starts = set(old.starts)
    text_lo = min(s.addr for s in old.text_sections())
    image_hi = max(s.addr + s.size for s in old.sections)
    out = []
    for v, where in sorted(xmap.scan_literals(paths, root).items()):
        prod = list(where) if all_lines else [w for w in where if not w[2]]
        if not prod or not (text_lo <= v < image_hi):
            continue
        c = xmap.classify(old, v, mvt_index, starts)
        if c['kind'] in ('vtable', 'global', 'function', 'vtable-entry'):
            out.append((v, c, sorted({f'{f}:{n}' for f, n, _ in prod})[:8]))
    return out


def map_literal(m, v, c):
    kind = {'vtable': 'vtable', 'function': 'function', 'global': 'global', 'vtable-entry': 'global'}[c['kind']]
    r = getattr(m, 'map_' + kind)(v)
    if kind == 'function' and r['status'] != 'mapped':
        # small lambda and std::function helpers rarely pair as functions, but the matched code that
        # installs them loads their address RIP-relative: vote on those loads as for a global
        g = m.map_global(v)
        if g['status'] == 'mapped' and m.elf.is_code(g['elf']):
            r = dict(g, notes=['function address voted from RIP loads in matched functions'] + g['notes'])
    ok = r['status'] == 'mapped' and r.get('confidence') in ACCEPT
    return dict(kind=c['kind'], rtti=c.get('rtti'), status='mapped' if ok else 'unresolved',
                new=hex(r['elf']) if r.get('elf') is not None else None, confidence=r.get('confidence'),
                method=r.get('method'), notes=r.get('notes', []), candidates=r.get('candidates'))


def map_classes(m, classes):
    out = {}
    for name, spec in classes.items():
        vt = xmap.parse_int(spec.get('vtable'))
        rtti = spec.get('rtti')
        if vt and not rtti:
            hit = m.mvt_index.get(vt)
            rtti = hit[0] if hit else None
        anchors = [xmap.parse_int(x) for x in spec.get('anchors', [])]
        context = [xmap.parse_int(x) for x in spec.get('context', [])]
        if not rtti and not anchors and not context:
            out[name] = dict(status='needs-owner', reason='no vtable/RTTI, anchor or context')
            continue
        res, nfun = m.map_class(rtti, anchors, context)
        fields = {}
        for x, r in sorted(res.items()):
            if x < 0:
                continue
            ok = r['status'] == 'mapped' and r['confidence'] in ACCEPT and r['elf'] >= 0
            fields[hex(x)] = dict(new=hex(r['elf']) if ok else None,
                                  confidence=r.get('confidence'), notes=r['notes'],
                                  candidates=r.get('candidates'))
        moved = {k: f['new'] for k, f in fields.items() if f['new'] is not None and f['new'] != k}
        out[name] = dict(rtti=rtti, functions=nfun, fields=fields, moved=moved)
    return out


def classify_elf(old, v, mvt_index, starts):
    """vtable, function or global for a value of the old ELF table (absolute = image-relative in a PIE)."""
    if v in mvt_index:
        return dict(kind='vtable', rtti=mvt_index[v][0])
    if v in starts:
        return dict(kind='function')
    return dict(kind='global')


def _gen():
    spec = importlib.util.spec_from_file_location('gen_linux_layout', HERE / 'gen_linux_layout.py')
    gen = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(gen)
    return gen


def class_specs(m, table, scan, hints, extra):
    """Evidence per class of the old table, in the old ELF's terms: RTTI (from the class's vtable key, a hint
    or the class name), context globals (hint keys looked up in the table's image section or `extra`) and
    a parent link (a hint or the sources). Same rules as gen_linux_layout.generate."""
    gen = _gen()
    def old_value(key):
        v = table.get('image', {}).get(hex(xmap.parse_int(key)))
        v = v if v is not None else extra.get(hex(xmap.parse_int(key)))
        return xmap.parse_int(v) if v is not None else None
    specs = {}
    for name in sorted(table.get('fields', {})):
        h, how = hints.get(name, {}), None
        rtti = h.get('rtti')
        how = 'hint' if rtti else None
        vt = scan['classes'].get(name)
        if not rtti and vt:
            hit = m.mvt_index.get(old_value(vt)) if old_value(vt) is not None else None
            rtti, how = (hit[0], 'vtable') if hit else (None, None)
        if not rtti:
            guess = gen.mangle_class_name(name)
            if guess and guess in m.mvt:
                rtti, how = guess, 'name'
        context = [c for c in (old_value(x) for x in h.get('context', ())) if c is not None]
        anchors = [a for a in (old_value(x) for x in h.get('anchors', ())) if a is not None]
        via = h.get('via')
        links = scan['via'].get(name, {})
        if not rtti and not via and not context and not anchors and len(links) == 1:
            p, o = next(iter(links)).rsplit('+', 1)
            via = dict(parent=p, offset=o)
        specs[name] = dict(cls=rtti, how=how, context=context, anchors=anchors, via=via)
    return specs


def migrate_table(m, old, table, scan, hints=None, extra=None):
    """(new table, report) for a generated xlayout table of the old build. scan: gen_linux_layout.scan() of
    the sources; hints: the generator's hints file; extra: {macOS key: old ELF value} for context globals
    kept outside the table (the hand-written core layouts)."""
    gen = _gen()
    hints, extra = hints or {}, extra or {}
    starts = set(old.starts)
    image, img_report = {}, {}
    for key, val in sorted(table.get('image', {}).items()):
        v = xmap.parse_int(val)
        r = map_literal(m, v, classify_elf(old, v, m.mvt_index, starts))
        img_report[key] = dict(old=val, **r)
        if r['status'] == 'mapped':
            image[key] = r['new']
    specs = class_specs(m, table, scan, hints, extra)
    order = [n for n in specs if not specs[n]['via']]
    pending = [n for n in specs if specs[n]['via']]
    while pending:
        ready = [n for n in pending if specs[n]['via']['parent'] in order]
        if not ready:
            order.extend(pending)
            break
        order.extend(ready)
        pending = [n for n in pending if n not in ready]
    old_fields = table.get('fields', {})
    fields, fld_report = {}, {}
    for cls in order:
        offs, sp = old_fields[cls], specs[cls]
        via = None
        if sp['via']:
            parent, poff = sp['via']['parent'], hex(xmap.parse_int(sp['via']['offset']))
            po, pn = old_fields.get(parent, {}).get(poff), fields.get(parent, {}).get(poff)
            if po is None or pn is None or parent not in specs:
                fld_report[cls] = dict(status='needs-owner', reason=f'parent field {parent}+{poff} not mapped',
                                       count=len(offs))
                continue
            ps = specs[parent]
            via = (dict(cls=ps['cls'], anchors=ps['anchors'], context=ps['context']),
                   xmap.parse_int(po), xmap.parse_int(pn))
        if not sp['cls'] and not sp['context'] and not sp['anchors'] and via is None:
            fld_report[cls] = dict(status='no-evidence', reason='no vtable/RTTI, context, anchor or parent link',
                                   count=len(offs))
            continue
        res, nfun = m.map_class(via=via) if via is not None else m.map_class(sp['cls'], sp['anchors'],
                                                                             sp['context'])
        out = {}
        for x, r in res.items():
            if x >= 0 and r['status'] == 'mapped' and r.get('confidence') in ACCEPT and r['elf'] >= 0:
                out[hex(x)] = hex(r['elf'])
        need = sorted({xmap.parse_int(v) for v in offs.values()})
        dropped = gen.monotone(out)
        filled = gen.fill_gaps(out, need, res)
        got, missing = {}, []
        for key, val in sorted(offs.items()):
            new = out.get(hex(xmap.parse_int(val)))
            if new is None:
                missing.append(key)
            else:
                got[key] = new
        if got:
            fields[cls] = got
        fld_report[cls] = dict(rtti=sp['cls'], rtti_from=sp['how'], via=sp['via'], functions=nfun,
                               mapped=len(got), missing=missing, filled=filled, dropped_non_monotonic=dropped)
    new = dict(build_id=m.elf.identity, image=image, fields=fields)
    return new, dict(image=img_report, fields=fld_report)


def main(argv=None):
    p = argparse.ArgumentParser(description=__doc__.split('\n')[0])
    p.add_argument('old')
    p.add_argument('new')
    p.add_argument('--src', nargs='*', default=[])
    p.add_argument('--root', default='.')
    p.add_argument('--all-lines', action='store_true', help='also map literals of test code and fixtures')
    p.add_argument('--classes')
    p.add_argument('--out')
    p.add_argument('--table', help='generated xlayout table of the old build to move to the new one')
    p.add_argument('--out-table', help='where to write the table of the new build (with --table)')
    p.add_argument('--hints', help='gen_linux_layout hints (rtti, context, via) for classes without a vtable')
    p.add_argument('--image-extra', help='{macOS key: old ELF value} for hint globals outside the table')
    p.add_argument('--keys-src', default='native/src', help='sources scanned for the Class tags (with --table)')
    p.add_argument('--cache', default=os.environ.get('XMAP_CACHE', str(pathlib.Path.home() / '.cache' / 'hoi4-xmap')))
    a = p.parse_args(argv)
    old, new = xmap.load_image(a.old), xmap.load_image(a.new)
    if old.kind != 'elf' or new.kind != 'elf':
        raise SystemExit('expected two ELF files')
    old.names = getattr(old, 'names', {})  # classify/global_users look up Mach-O symbol names
    m = xmap.Matcher(old, new, a.cache)
    report = dict(old=dict(file=os.path.basename(a.old), build_id=old.identity, sha256=old.sha256()),
                  new=dict(file=os.path.basename(a.new), build_id=new.identity, sha256=new.sha256()),
                  function_pairs=len(m.fmap), image={}, classes={})
    if a.src:
        for v, c, srcs in literal_items(old, a.src, a.root, m.mvt_index, a.all_lines):
            report['image'][hex(v)] = dict(map_literal(m, v, c), sources=srcs)
    if a.classes:
        report['classes'] = map_classes(m, json.loads(pathlib.Path(a.classes).read_text()))
    if a.table:
        old_t = json.loads(pathlib.Path(a.table).read_text())
        hints = json.loads(pathlib.Path(a.hints).read_text()) if a.hints else {}
        extra = json.loads(pathlib.Path(a.image_extra).read_text()) if a.image_extra else {}
        new_table, report['table'] = migrate_table(m, old, old_t, _gen().scan(a.keys_src), hints, extra)
        report['table_summary'] = dict(
            image=f"{len(new_table['image'])} of {len(old_t.get('image', {}))}",
            fields=f"{sum(map(len, new_table['fields'].values()))} of {sum(map(len, old_t.get('fields', {}).values()))}")
        print(json.dumps(report['table_summary']), file=sys.stderr)
        if a.out_table:
            pathlib.Path(a.out_table).write_text(json.dumps(new_table, indent=1, sort_keys=True) + '\n')
    s = collections.Counter(r['status'] for r in report['image'].values())
    report['summary'] = dict(image=dict(s), classes=len(report['classes']),
                             fields_moved=sum(len(c.get('moved', {})) for c in report['classes'].values()))
    print(json.dumps(report['summary'], indent=1), file=sys.stderr)
    text = json.dumps(report, indent=1)
    if a.out:
        pathlib.Path(a.out).write_text(text + '\n')
    else:
        print(text)


if __name__ == '__main__':
    main()
