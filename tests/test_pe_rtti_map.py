import importlib.util
import pathlib
import tempfile
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[1]
_spec = importlib.util.spec_from_file_location('pe_rtti_map', ROOT / 'scripts' / 'pe_rtti_map.py')
m = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(m)


class NameTranslation(unittest.TestCase):
    def test_plain_and_nested_names(self):
        self.assertEqual(m.itanium_parts('17CCurrentGameState'), ['CCurrentGameState'])
        self.assertEqual(m.itanium_parts('N3Foo3BarE'), ['Foo', 'Bar'])
        self.assertEqual(m.msvc_names('12CEventWindow'), ['.?AVCEventWindow@@', '.?AUCEventWindow@@'])
        self.assertEqual(m.msvc_names('N23NIndustrialOrganisation35COpenRosterButtonForPoliticalScreenE'),
                         ['.?AVCOpenRosterButtonForPoliticalScreen@NIndustrialOrganisation@@',
                          '.?AUCOpenRosterButtonForPoliticalScreen@NIndustrialOrganisation@@'])

    def test_untranslatable_names_are_refused(self):
        for name in ('11TNullObjectI17CBuildingTemplateS0_E', 'N12_GLOBAL__N_13FooE', 'St9exception',
                     '', '5CArm', 'N3FooE7'):
            self.assertIsNone(m.itanium_parts(name), name)
            self.assertEqual(m.msvc_names(name), [])


class VtableMapping(unittest.TestCase):
    RTTI = {
        '.?AVCArmy@@': [(0x2000, 0)],
        '.?AVCWin@@': [(0x3000, 0), (0x3100, 0x40), (0x3200, 0x58)],
        '.?AVCTwo@@': [(0x4000, 0), (0x4100, 0)],
    }

    def test_primary_is_high(self):
        self.assertEqual(m.map_vtable(('5CArmy', 0, 0), [1], self.RTTI), (0x2000, 'high', '.?AVCArmy@@'))

    def test_secondary_by_order_when_counts_agree(self):
        rva, conf, _ = m.map_vtable(('4CWin', 2, -0x60), [1, 2, 3], self.RTTI)
        self.assertEqual((rva, conf), (0x3200, 'medium'))

    def test_secondary_with_other_count_is_unresolved(self):
        rva, conf, _ = m.map_vtable(('4CWin', 1, -0x40), [1, 2], self.RTTI)
        self.assertEqual((rva, conf), (None, 'unresolved'))

    def test_missing_or_ambiguous_primary_is_unresolved(self):
        self.assertEqual(m.map_vtable(('5CNone', 0, 0), [1], self.RTTI)[:2], (None, 'unresolved'))
        self.assertEqual(m.map_vtable(('4CTwo', 0, 0), [1], self.RTTI)[:2], (None, 'unresolved'))


class CoreVtables(unittest.TestCase):
    def test_only_macos_consts_are_keys(self):
        with tempfile.TemporaryDirectory() as d:
            pathlib.Path(d, 'game.rs').write_text(
                'pub(crate) const LINUX: Layout = Layout {\n    gs_vtable: 0x445e068,\n};\n'
                'pub(crate) const MACOS: Layout = Layout {\n    gs_vtable: 0x3504e48,\n    date: 0x428,\n};\n')
            self.assertEqual(m.core_vtables(d), {0x3504e48: ['game.rs:5 gs_vtable']})


if __name__ == '__main__':
    unittest.main()
