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

Outside a macOS context, a literal that IS a key of MAP's image table (an exact 1.19.4.1 macOS address whose
new value differs) is rewritten too (a canonical key passed through xlayout::img, a match arm comparing a
rewritten layout value), unless it stands in an item named for another platform (LINUX, ELF, WINDOWS, ...)
or its site is listed in MAP["variant"]["not_macos"] {"file" or "file:line": reason}. Test code is skipped
per item: a #[cfg(test)] attribute covers the item that follows it (a use line, a function, a test module),
not the rest of the file.

--verify DIR checks a generated tree (DIR/native) and exits 1 when any production literal there is still an
old key of the map (a 1.19.4.1 address left behind, e.g. by a hand edit or a context the plan missed).
Build scripts run it before cargo so a variant with a leftover address never compiles.
"""
import argparse, json, pathlib, re, shutil, sys

MAC_PATH = re.compile(r'(launch/macos|/mac_|gui/macos|platform/macos|mac_ui|mac_find|mac_panel|mac_map_point|mac_projection)')
ITEM = re.compile(r'^\s*(pub(\([a-z]+\))?\s+)?(const|static)\s+([A-Za-z0-9_]+)')
LIT = re.compile(r'(?<![0-9A-Za-z_])0x([0-9a-fA-F_]{3,12})(?![0-9A-Za-z_])')
POISON = 0x00de_ad00_0000_0000   # memory::DISABLED_BASE
OK_CONFIDENCE = ('high', 'medium')


OTHER = re.compile(r'LINUX|Linux|ELF|Elf|WINDOWS|Windows|WIN_|_WIN|PE_')


def item_name(lines, i):
    for j in range(i, max(-1, i - 400), -1):
        m = ITEM.match(lines[j])
        if m:
            return m.group(4)
    return ''


def mac_block(lines, i):
    return bool(re.search(r'MAC|Mac', item_name(lines, i)))


def other_platform_block(lines, i):
    name = item_name(lines, i)
    return bool(name) and not re.search(r'MAC|Mac', name) and bool(OTHER.search(name))


def code_only(line):
    """The line without string/char literals and // comments (for brace counting)."""
    out, i, n = [], 0, len(line)
    while i < n:
        c = line[i]
        if line.startswith('//', i):
            break
        if c == '"':
            i += 1
            while i < n and line[i] != '"':
                i += 2 if line[i] == '\\' else 1
            i += 1
            continue
        if c == "'" and re.match(r"'(\\.|[^\\'])'", line[i:i + 4]):
            i += len(re.match(r"'(\\.|[^\\'])'", line[i:i + 4]).group(0))
            continue
        out.append(c)
        i += 1
    return ''.join(out)


def test_lines(lines):
    """Indices of lines that belong to an item under #[cfg(test)] (attribute, the item, its body)."""
    skip, i, n = set(), 0, len(lines)
    while i < n:
        if not re.match(r'\s*#\[cfg\((all\()?test\b', lines[i]):
            i += 1
            continue
        j = i
        while j < n and (lines[j].strip().startswith('#[') or not lines[j].strip()):
            skip.add(j)
            j += 1
        depth, opened = 0, False
        while j < n:
            skip.add(j)
            code = code_only(lines[j])
            depth += code.count('{') - code.count('}')
            opened = opened or '{' in code
            if (opened and depth <= 0) or (not opened and code.rstrip().endswith(';')):
                break
            j += 1
        i = j + 1
    return skip


def is_test_path(rel):
    return any(p == 'tests' or p == 'tests.rs' or p.endswith('_tests.rs') for p in pathlib.PurePath(rel).parts)


TEST_MOD = re.compile(r'^\s*(pub(\([a-z]+\))?\s+)?mod\s+([A-Za-z0-9_]+)\s*;')


def test_module_files(src):
    """Files (and folders) of modules declared under #[cfg(test)] (`#[cfg(test)] mod fixture;`)."""
    out = set()
    for f in src.rglob('*.rs'):
        lines = read(f).split('\n')
        for i in test_lines(lines):
            m = TEST_MOD.match(lines[i])
            if m:
                here = f.parent if f.name in ('mod.rs', 'lib.rs', 'main.rs') else f.parent / f.stem
                out.add(here / (m.group(3) + '.rs'))
                out.add(here / m.group(3))
    return out


def production_lines(src_root):
    """(file, rel, lines, index) for every production line of the crate (tests and builds.rs left out)."""
    src = pathlib.Path(src_root) / 'src'
    test_mods = test_module_files(src)
    for f in sorted(src.rglob('*.rs')):
        rel = 'native/' + f.relative_to(src_root).as_posix()
        if is_test_path(f.relative_to(src_root)) or rel == 'native/src/builds.rs':
            continue
        if f in test_mods or any(t in f.parents for t in test_mods):
            continue
        lines = read(f).split('\n')
        skip = test_lines(lines)
        for i in range(len(lines)):
            if i not in skip:
                yield f, rel, lines, i


def load_map(path):
    """A migration map; one that names `same_code_of` (a build with the base build's code in place,
    scripts/game_migrate.py same-code) takes the base map's tables and keeps its own identity."""
    path = pathlib.Path(path)
    m = json.loads(read(path))
    base = m.get('same_code_of')
    if base:
        b = load_map(path.parent / base)
        if b.get('from') != m.get('from'):
            raise SystemExit(f'{path}: same_code_of {base} starts from another build')
        b.update({k: v for k, v in m.items() if k != 'same_code_of'})
        m = b
    return m


def plan(src_root, mapping):
    """(edits {file: {line index: new text}}, sites, problems). src_root is the crate folder (native)."""
    v = mapping.get('variant', {})
    lo, hi = (int(x, 16) for x in v.get('image_range', ['0x0', '0x0']))
    not_image = {int(k, 16) for k in v.get('not_image', {})}
    shared = set(v.get('shared_key_files', []))
    disabled = v.get('disabled', {})
    not_macos = v.get('not_macos', {})
    image = {int(k, 16): e for k, e in mapping.get('image', {}).items()}
    old_id, new_id = mapping['from'], mapping['to']
    idents = [(old_id['build_id'], new_id['build_id'])]
    if old_id.get('sha256') and new_id.get('sha256'):
        idents.append((old_id['sha256'], new_id['sha256']))
    edits, sites, problems = {}, [], []
    for f, rel, lines, i in production_lines(src_root):
        line = lines[i]
        site = f'{rel}:{i + 1}'
        mac = bool(MAC_PATH.search(rel)) or rel in shared or mac_block(lines, i)
        other = not mac and other_platform_block(lines, i)
        exempt = rel in not_macos or site in not_macos
        new_line = line
        if mac:
            for a, b in idents:
                if a in new_line:
                    new_line = new_line.replace(a, b)
                    sites.append(dict(site=site, kind='identity', old=a, new=b))
        code_end = len(line.split('//')[0])

        def sub(m):
            if m.start() >= code_end:
                return m.group(0)
            val = int(m.group(1).replace('_', ''), 16)
            if not (lo <= val < hi) or val in not_image:
                return m.group(0)
            e = image.get(val) or {}
            if not mac:
                # outside a macOS context only an exact key of the map is a macOS address
                if other or exempt or not e.get('new') or int(e['new'], 16) == val:
                    return m.group(0)
                if e.get('confidence') in OK_CONFIDENCE:
                    sites.append(dict(site=site, kind='image', old=hex(val), new=e['new'], context='key',
                                      confidence=e['confidence'], how=e.get('how')))
                    return e['new']
                problems.append(dict(site=site, old=hex(val), confidence=e.get('confidence', 'unresolved'),
                                     text=line.strip()[:120]))
                return m.group(0)
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


def verify(out_root, mapping):
    """Leftover 1.19.4.1 keys in a generated crate (out_root = DIR/native): [{site, old, text}]."""
    v = mapping.get('variant', {})
    not_image = {int(k, 16) for k in v.get('not_image', {})}
    not_macos = v.get('not_macos', {})
    image = {int(k, 16): e for k, e in mapping.get('image', {}).items()}
    new_values = {int(e['new'], 16) for e in image.values() if e.get('new')}
    old_keys = {k for k, e in image.items() if e.get('new') and int(e['new'], 16) != k
                and k not in not_image and k not in new_values}
    left = []
    for f, rel, lines, i in production_lines(out_root):
        line = lines[i]
        site = f'{rel}:{i + 1}'
        if rel in not_macos or site in not_macos or other_platform_block(lines, i):
            continue
        code = line.split('//')[0]
        for m in LIT.finditer(code):
            val = int(m.group(1).replace('_', ''), 16)
            if val in old_keys:
                left.append(dict(site=site, old=hex(val), text=line.strip()[:120]))
    return left


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
    p.add_argument('--verify', metavar='DIR', help='check a generated tree for leftover old addresses')
    a = p.parse_args(argv)
    mapping = load_map(a.map)
    if a.verify:
        left = verify(pathlib.Path(a.verify) / 'native', mapping)
        for x in left[:60]:
            print(f"LEFTOVER {x['old']} {x['site']}: {x['text']}", file=sys.stderr)
        print(json.dumps(dict(verified=str(a.verify), leftovers=len(left))))
        return 1 if left else 0
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
    left = verify(out, mapping)
    if left:
        for x in left[:60]:
            print(f"LEFTOVER {x['old']} {x['site']}: {x['text']}", file=sys.stderr)
        shutil.rmtree(out)
        return 1
    write(pathlib.Path(a.out) / 'VARIANT.json', json.dumps(summary, indent=1) + '\n')
    print(f'wrote {out} ({sum(len(c) for c in edits.values())} lines in {len(edits)} files)')
    return 0


if __name__ == '__main__':
    sys.exit(main())
