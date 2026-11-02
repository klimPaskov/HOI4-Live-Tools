#!/usr/bin/env python3
"""Apply a macos_build_migrate.py mapping to the Rust sources, only in macOS contexts.

usage: apply_build_migration.py MAPPING.json [--dry-run]
A literal is rewritten when its file is macOS-specific (path contains macos/ mac_ or gui/macos) or when the
nearest enclosing `const`/`static` item above it is named like MACOS/MAC/Mac. Everything else is reported."""
import json, re, sys, pathlib
MAC_PATH = re.compile(r'(launch/macos|/mac_|gui/macos|platform/macos|mac_ui|mac_find|mac_panel|mac_map_point|mac_projection)')
ITEM = re.compile(r'^\s*(pub(\([a-z]+\))?\s+)?(const|static)\s+([A-Za-z0-9_]+)')
def mac_block(lines, i):
    for j in range(i, max(-1, i - 400), -1):
        m = ITEM.match(lines[j])
        if m:
            return bool(re.search(r'MAC|Mac', m.group(4)))
    return False
FORCE = []
def main():
    if '--force' in sys.argv:
        FORCE.extend(sys.argv[sys.argv.index('--force') + 1].split(','))
    mapping = json.load(open(sys.argv[1]))
    table = {int(k, 16): int(e['new'], 16) for k, e in mapping.items() if e.get('new')}
    dry = '--dry-run' in sys.argv
    changed, skipped = 0, []
    for f in sorted(list(pathlib.Path('native/src').rglob('*.rs')) + list(pathlib.Path('native/tests').rglob('*.rs'))):
        text = f.read_text()
        lines = text.split('\n')
        out = []
        for i, line in enumerate(lines):
            def sub(m):
                nonlocal changed
                v = int(m.group(1).replace('_', ''), 16)
                if v not in table:
                    return m.group(0)
                if MAC_PATH.search(str(f)) or mac_block(lines, i) or any(str(f).endswith(x) for x in FORCE):
                    changed += 1
                    return hex(table[v])
                skipped.append(f'{f}:{i+1}: {m.group(0)}')
                return m.group(0)
            out.append(re.sub(r'0x([0-9a-fA-F_]{6,9})\b', sub, line))
        new = '\n'.join(out)
        if new != text and not dry:
            f.write_text(new)
    print(f'rewritten {changed} literal(s)')
    print('not rewritten (review):')
    for s in skipped:
        print('  ' + s)
if __name__ == '__main__':
    main()
