#!/usr/bin/env python3
"""Generate the native runtime's source for another macOS game build from a migration map (no game needed).

usage: macos_build_variant.py MAP.json --out DIR [--src native] [--report FILE] [--check]

The runtime's sources keep the macOS 1.19.4.1 (Mach-O DFA875A5) image addresses as their keys. For another
macOS build this writes DIR/native: a copy of the crate whose macOS image literals are the new build's, taken
from MAP's "image" table (old -> {new, confidence, how}), and whose build identity (Mach-O UUID, SHA-256) is
the new build's. Build it like the normal crate (cargo build --release in DIR/native). Nothing else changes:
struct fields are not rewritten (MAP's "fields_default": "identity" says the migration found them unchanged).

Which literals: production code only (not native/tests, not after a top-level #[cfg(test)]), in a macOS
context (a macOS path such as launch/macos or gui/macos, or the nearest enclosing const/static named like
MACOS/Mac) or in one of MAP["variant"]["shared_key_files"] (canonical keys that pass through xlayout::img,
which is the identity on macOS); values inside MAP["variant"]["image_range"] that are not listed in
MAP["variant"]["not_image"] (pointer floors, CPU type, masks, field offsets that fall in the range).

Every such literal must map with confidence high or medium. A literal the map leaves unresolved or low is an
error, unless its file is listed in MAP["variant"]["disabled"] {file: reason}: then the literal becomes an
address in the poison range (crate::memory::DISABLED_BASE + old value), which the Reader refuses with
layout_disabled, and the reason is written into builds::DISABLED, so that module stops with a clear message
instead of reading garbage. --check generates nothing and exits 1 when the map does not cover the sources.
"""
import argparse, json, pathlib, re, shutil, sys

MAC_PATH = re.compile(r'(launch/macos|/mac_|gui/macos|platform/macos|mac_ui|mac_find|mac_panel|mac_map_point|mac_projection)')
ITEM = re.compile(r'^\s*(pub(\([a-z]+\))?\s+)?(const|static)\s+([A-Za-z0-9_]+)')
LIT = re.compile(r'(?<![0-9A-Za-z_])0x([0-9a-fA-F_]{3,12})(?![0-9A-Za-z_])')
POISON = 0x00de_ad00_0000_0000   # memory::DISABLED_BASE
OK_CONFIDENCE = ('high', 'medium')


def mac_block(lines, i):
    for j in range(i, max(-1, i - 400), -1):
        m = ITEM.match(lines[j])
        if m:
            return bool(re.search(r'MAC|Mac', m.group(4)))
    return False


def is_test_path(rel):
    return any(p == 'tests' or p == 'tests.rs' or p.endswith('_tests.rs') for p in pathlib.PurePath(rel).parts)


def plan(src_root, mapping):
    """(edits {file: {line index: new text}}, sites, problems). src_root is the crate folder (native)."""
    v = mapping.get('variant', {})
    lo, hi = (int(x, 16) for x in v.get('image_range', ['0x0', '0x0']))
    not_image = {int(k, 16) for k in v.get('not_image', {})}
    shared = set(v.get('shared_key_files', []))
    disabled = v.get('disabled', {})
    image = {int(k, 16): e for k, e in mapping.get('image', {}).items()}
    old_id, new_id = mapping['from'], mapping['to']
    idents = [(old_id['build_id'], new_id['build_id'])]
    if old_id.get('sha256') and new_id.get('sha256'):
        idents.append((old_id['sha256'], new_id['sha256']))
    edits, sites, problems = {}, [], []
    src = pathlib.Path(src_root) / 'src'
    for f in sorted(src.rglob('*.rs')):
        rel = 'native/' + f.relative_to(src_root).as_posix()
        if is_test_path(f.relative_to(src_root)) or rel == 'native/src/builds.rs':
            continue
        lines = read(f).split('\n')
        test = False
        for i, line in enumerate(lines):
            if line.startswith('#[cfg(test)]'):
                test = True
            if test:
                continue
            mac = bool(MAC_PATH.search(rel)) or rel in shared or mac_block(lines, i)
            if not mac:
                continue
            new_line = line
            for a, b in idents:
                if a in new_line:
                    new_line = new_line.replace(a, b)
                    sites.append(dict(site=f'{rel}:{i + 1}', kind='identity', old=a, new=b))
            code_end = len(line.split('//')[0])

            def sub(m):
                if m.start() >= code_end:
                    return m.group(0)
                val = int(m.group(1).replace('_', ''), 16)
                if not (lo <= val < hi) or val in not_image:
                    return m.group(0)
                e = image.get(val) or {}
                site = f'{rel}:{i + 1}'
                if e.get('new') and e.get('confidence') in OK_CONFIDENCE:
                    sites.append(dict(site=site, kind='image', old=hex(val), new=e['new'],
                                      confidence=e['confidence'], how=e.get('how')))
                    return e['new']
                if rel in disabled:
                    sites.append(dict(site=site, kind='disabled', old=hex(val), reason=disabled[rel],
                                      confidence=e.get('confidence', 'unresolved')))
                    return hex(POISON + val)
                problems.append(dict(site=site, old=hex(val), confidence=e.get('confidence', 'unresolved'),
                                     text=line.strip()[:120]))
                return m.group(0)
            new_line = LIT.sub(sub, new_line)
            if new_line != line:
                edits.setdefault(f, {})[i] = new_line
    return edits, sites, problems


def read(path):
    with open(path, encoding='utf-8', newline='') as f:
        return f.read()


def write(path, text):
    with open(path, 'w', encoding='utf-8', newline='') as f:
        f.write(text)


def builds_disabled(text, disabled, build_id):
    entries = ''.join(f'\n    ("{f}", "{r}"),' for f, r in sorted(disabled.items()))
    return re.sub(r'pub const DISABLED: &\[\(&str, &str\)\] = &\[[^\]]*\];',
                  f'pub const DISABLED: &[(&str, &str)] = &[{entries}\n];' if entries else
                  'pub const DISABLED: &[(&str, &str)] = &[];', text)


def main(argv=None):
    p = argparse.ArgumentParser(description=__doc__.split('\n')[0])
    p.add_argument('map')
    p.add_argument('--src', default='native')
    p.add_argument('--out')
    p.add_argument('--report')
    p.add_argument('--check', action='store_true')
    a = p.parse_args(argv)
    mapping = json.loads(read(a.map))
    edits, sites, problems = plan(a.src, mapping)
    kinds = {}
    for s in sites:
        key = s['kind'] + ('' if s['kind'] != 'image' else ':' + s['confidence'])
        kinds[key] = kinds.get(key, 0) + 1
    summary = dict(build=mapping['to'], sites=kinds, problems=len(problems),
                   disabled=mapping.get('variant', {}).get('disabled', {}))
    if a.report:
        write(a.report, json.dumps(dict(summary=summary, sites=sites, problems=problems), indent=1) + '\n')
    print(json.dumps(summary, indent=1))
    for x in problems[:40]:
        print(f"UNMAPPED {x['old']} ({x['confidence']}) {x['site']}: {x['text']}", file=sys.stderr)
    if problems:
        return 1
    if a.check or not a.out:
        return 0
    out = pathlib.Path(a.out) / 'native'
    if out.exists():
        shutil.rmtree(out)
    shutil.copytree(a.src, out, ignore=shutil.ignore_patterns('target'))
    for f, changed in edits.items():
        g = out / f.relative_to(a.src)
        lines = read(g).split('\n')
        for i, text in changed.items():
            lines[i] = text
        write(g, '\n'.join(lines))
    b = out / 'src' / 'builds.rs'
    write(b, builds_disabled(read(b), mapping.get('variant', {}).get('disabled', {}), mapping['to']['build_id'])
                 .replace('pub const READER_BUILD: &str = "' + mapping['from']['build_id'],
                          'pub const READER_BUILD: &str = "' + mapping['to']['build_id']))
    write(pathlib.Path(a.out) / 'VARIANT.json', json.dumps(summary, indent=1) + '\n')
    print(f'wrote {out} ({sum(len(c) for c in edits.values())} lines in {len(edits)} files)')
    return 0


if __name__ == '__main__':
    sys.exit(main())
