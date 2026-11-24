#!/usr/bin/env python3
"""Check a layout against the game's own save: memory reads vs the same values in a plain-text save.

A save is version-proof truth: it needs no memory offsets. After a migration (and at any paused moment of
a run), pause the game, save it as text, read the same values through the native tools, and compare:
a module whose read disagrees with the save is not proven on this build and stays disabled.

usage: layout_selfcheck.py SAVE.hoi4 TAG MEMORY.json [--out RESULT.json]

MEMORY.json holds the native reads taken while the game stood paused on the save's date, flat:
  {"build": "<build id>", "date": "1936.1.1", "political_power": 12.31, "military_factories_assigned": 14,
   "divisions": 46, "researching": ["infantry_weapons1"], "focus": "GER_rhineland"}
Keys that are absent are not compared. The save must be plain text (settings.txt save_as_binary=no).
Exit status 0 when every compared module agrees, 1 otherwise.
"""
import json
import pathlib
import sys

ROOT = pathlib.Path(__file__).resolve().parents[1]

# value key -> (runtime module it proves, how the save section and field give it, tolerance)
CHECKS = {
    'date': ('game', lambda s: s.get('save_date'), None),
    'political_power': ('politics', lambda s: (s.get('politics') or {}).get('political_power'), 0.01),
    'military_factories_assigned': ('production_lines',
                                    lambda s: (s.get('production') or {}).get('military_factories_assigned'), 0),
    'divisions': ('army', lambda s: (s.get('military') or {}).get('divisions'), 0),
    'researching': ('research', lambda s: sorted(r.get('tech') for r in (s.get('research') or {}).get('researching', [])
                                                  if isinstance(r, dict) and r.get('tech')), None),
    'focus': ('own_focus', lambda s: (s.get('focus') or {}).get('current'), None),
}


def normalise_date(v):
    if not isinstance(v, str):
        return v
    parts = v.split('.')[:3]
    try:
        return '.'.join(str(int(p)) for p in parts)
    except ValueError:
        return v


def compare(memory, save):
    """{"modules": {module: {"agrees": bool, "checks": [...]}}, "agrees": bool} for every key in `memory`."""
    mods = {}
    for key, (module, get, tol) in CHECKS.items():
        if key not in memory:
            continue
        want, got = get(save), memory[key]
        if key == 'date':
            want, got = normalise_date(want), normalise_date(got)
        if key == 'researching':
            got = sorted(got or [])
        if want is None:
            ok, why = None, 'the save has no value'
        elif tol is not None and isinstance(want, (int, float)) and isinstance(got, (int, float)):
            ok, why = abs(want - got) <= tol, f'save {want}, memory {got}'
        else:
            ok, why = want == got, f'save {want!r}, memory {got!r}'
        entry = mods.setdefault(module, dict(agrees=True, checks=[]))
        entry['checks'].append(dict(value=key, agrees=ok, detail=why))
        if ok is False:
            entry['agrees'] = False
    return dict(build=memory.get('build'), modules=mods, agrees=all(m['agrees'] for m in mods.values()))


def main(argv=None):
    argv = sys.argv[1:] if argv is None else argv
    if len(argv) < 3:
        print(__doc__, file=sys.stderr)
        return 2
    sys.path.insert(0, str(ROOT))
    from hoi4_live.state.model import country_state
    from hoi4_live.state.savefile import SaveFile
    save_path, tag, mem_path = argv[:3]
    save = country_state(SaveFile(save_path), tag, sections=('overview', 'politics', 'focus', 'research',
                                                              'production', 'military'))
    result = compare(json.loads(pathlib.Path(mem_path).read_text()), save)
    text = json.dumps(result, indent=1)
    if '--out' in argv:
        pathlib.Path(argv[argv.index('--out') + 1]).write_text(text + '\n')
    print(text)
    return 0 if result['agrees'] else 1


if __name__ == '__main__':
    sys.exit(main())
