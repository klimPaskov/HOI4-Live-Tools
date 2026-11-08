#!/usr/bin/env python3
"""Map image addresses and struct field offsets of one Linux HOI4 ELF build to another (static, read-only).

usage:
  linux_build_migrate.py OLD_ELF NEW_ELF [--src PATH ...] [--classes C.json] [--out report.json] [--cache DIR]

The same-platform counterpart of macos_build_migrate.py, built on cross_platform_map.Matcher with the old
ELF in place of the Mach-O. Both binaries are read as data and never run.

- Image literals: every hex literal in the Rust sources under --src (production code only, see
  cross_platform_map.scan_literals) that is a vtable address point, a global or a function start of the old
  ELF is mapped to the new ELF: vtables by RTTI name and address-point ordinal, globals and functions by
  matched code.
- Fields: C.json lists classes, {"Name": {"vtable": "0x..", "context": ["0x.."], "anchors": ["0x.."]}}
  with old-build values; each is mapped class-wide (every displacement its matched functions take on the
  object register), as gen_linux_layout.py does across platforms.

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


def main(argv=None):
    p = argparse.ArgumentParser(description=__doc__.split('\n')[0])
    p.add_argument('old')
    p.add_argument('new')
    p.add_argument('--src', nargs='*', default=[])
    p.add_argument('--root', default='.')
    p.add_argument('--all-lines', action='store_true', help='also map literals of test code and fixtures')
    p.add_argument('--classes')
    p.add_argument('--out')
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
