"""Table mode of scripts/linux_build_migrate.py with a stand-in matcher; no binary is read."""

import importlib.util
import pathlib
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[1]
_spec = importlib.util.spec_from_file_location('linux_build_migrate', ROOT / 'scripts' / 'linux_build_migrate.py')
lbm = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(lbm)

SHIFT = 0x8000          # every image value of the old build moved by this much in the stand-in
GS_OLD = 0x459e9f0      # the context global of CCurrentGameState in the old build


class Elf:
    identity = 'new-build'


class OldElf:
    starts = [0x100000]


def mapped(v, conf='high'):
    return dict(status='mapped', elf=v, confidence=conf, method='stand-in', notes=[])


class Matcher:
    """Old vtables are known by RTTI name; class functions move field offsets as `moves` says."""

    def __init__(self, moves):
        self.elf = Elf()
        self.mvt_index = {0x4400000: ('8CAirWing', 0), 0x4500000: ('5CArmy', 0)}
        self.mvt = {name for name, _ in self.mvt_index.values()} | {'9CDivision'}
        self.moves = moves
        self.calls = []

    def map_vtable(self, v):
        return mapped(v + SHIFT)

    def map_function(self, v):
        return mapped(v + SHIFT)

    def map_global(self, v):
        return mapped(v + SHIFT) if v != 0x4600000 else dict(status='unresolved', notes=['no evidence'])

    def map_class(self, cls=None, anchors=(), context=(), via=None):
        key = ('via', via[1], via[2]) if via is not None else (cls, tuple(context))
        self.calls.append(key)
        return {old: (mapped(new, conf) if new is not None else dict(status='unresolved', notes=[]))
                for old, (new, conf) in self.moves.get(key, {}).items()}, 3


TABLE = {
    'build_id': 'old-build',
    'image': {'0x34ac5f8': '0x4400000', '0x3500000': '0x4500000', '0x3600000': '0x4600000'},
    'fields': {
        'CAirWing': {'0x10': '0x18', '0x20': '0x28', '0x30': '0x38'},
        'CArmy': {'0x40': '0x48'},
        'CDivision': {'0x8': '0x8'},
        'CCurrentGameState': {'0x428': '0x460'},
        'army.child': {'0x0': '0x10'},
        'nobody': {'0x8': '0x8'},
    },
}
SCAN = {
    'classes': {'CAirWing': 0x34ac5f8, 'CArmy': 0x3500000, 'CDivision': None, 'CCurrentGameState': None,
                'army.child': None, 'nobody': None},
    'via': {'army.child': {'CArmy+0x40': ['army.rs:1']}},
}
HINTS = {'CCurrentGameState': {'context': ['0x372b040']}}
EXTRA = {'0x372b040': hex(GS_OLD)}


class TableMode(unittest.TestCase):
    def run_table(self, moves):
        m = Matcher(moves)
        return m, lbm.migrate_table(m, OldElf(), TABLE, SCAN, HINTS, EXTRA)

    def test_image_values_move_and_unmapped_ones_are_left_out(self):
        _, (new, report) = self.run_table({})
        self.assertEqual(new['build_id'], 'new-build')
        self.assertEqual(new['image'], {'0x34ac5f8': hex(0x4400000 + SHIFT), '0x3500000': hex(0x4500000 + SHIFT)})
        self.assertEqual(report['image']['0x3600000']['status'], 'unresolved')

    def test_fields_move_by_vtable_name_context_and_parent_link(self):
        moves = {
            ('8CAirWing', ()): {0x18: (0x18, 'high'), 0x38: (0x40, 'high'), 0x28: (0x30, 'low')},
            ('5CArmy', ()): {0x48: (0x50, 'medium')},
            ('9CDivision', ()): {0x8: (0x8, 'high')},
            (None, (GS_OLD,)): {0x460: (0x470, 'high')},
            ('via', 0x48, 0x50): {0x10: (0x18, 'high')},
        }
        m, (new, report) = self.run_table(moves)
        f = new['fields']
        # CAirWing 0x28 has only a low vote (+8); its neighbours moved by 0 and +8, so the vote lies
        # between their shifts and is kept.
        self.assertEqual(f['CAirWing'], {'0x10': '0x18', '0x20': '0x30', '0x30': '0x40'})
        self.assertEqual(report['fields']['CAirWing']['filled'], {'0x28': 'low-consistent'})
        self.assertEqual(f['CArmy'], {'0x40': '0x50'})
        self.assertEqual(f['CDivision'], {'0x8': '0x8'})
        self.assertEqual(report['fields']['CDivision']['rtti_from'], 'name')
        self.assertEqual(f['CCurrentGameState'], {'0x428': '0x470'})
        self.assertEqual(f['army.child'], {'0x0': '0x18'})
        # the child is mapped after its parent, through the parent's old and new field offsets
        self.assertLess(m.calls.index(('5CArmy', ())), m.calls.index(('via', 0x48, 0x50)))
        self.assertNotIn('nobody', f)
        self.assertEqual(report['fields']['nobody']['status'], 'no-evidence')

    def test_unmapped_parent_leaves_the_child_out(self):
        _, (new, report) = self.run_table({('8CAirWing', ()): {0x18: (0x18, 'high')}})
        self.assertNotIn('army.child', new['fields'])
        self.assertEqual(report['fields']['army.child']['status'], 'needs-owner')
        self.assertEqual(report['fields']['CArmy']['missing'], ['0x40'])


if __name__ == '__main__':
    unittest.main()
