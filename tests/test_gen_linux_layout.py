"""Static key scan and table checks of scripts/gen_linux_layout.py; synthetic Rust sources only."""

import importlib.util
import pathlib
import tempfile
import textwrap
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[1]
_spec = importlib.util.spec_from_file_location('gen_linux_layout', ROOT / 'scripts' / 'gen_linux_layout.py')
gen = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(gen)

PANEL = '''
pub(super) const DOCTRINE_VIEW: Class = Class {
    name: "NDoctrines::CCountryDoctrineView",
    vtable: 0x354e610,
    root_name: "country_doctrine_view",
};
pub(super) const VIEW_ROOT: u64 = 0x490;
'''

TOOL = '''
use super::{panelkit as pk, mgmt};
use crate::xlayout::{self, fld, img, Class};
const IDLER: Class = Class::new("CInGameIdler", 0x35089d8);
const MANAGER: xlayout::Class =
    xlayout::Class::new(
        "idler.manager",
        0,
    );
const SLOT: u64 = 0x1_88;
const NEXT: u64 = SLOT + 8;
struct Ui { view: u64, root: u64, kids: [u64; 2] }
const MACOS: Ui = Ui { view: 0x3b8, root: 0x68, kids: [0x10, 0x18] };
const LINUX: Ui = Ui { view: 0x999, root: 0x999, kids: [0x999, 0x999] };
fn at(r: &Reader<'_>, class: &xlayout::Class, mac: u64) -> Result<u64> {
    fld(r.string_abi(), class, mac)
}
fn go(r: &Reader<'_>, m: &Ui, abi: StringAbi, dynamic: u64) -> Result<()> {
    let a = fld(abi, &IDLER, 0x540)?; // fld(abi, &IDLER, 0xdead) in a comment
    let b = fld(abi, &MANAGER, NEXT)?;
    let c = fld(abi, &MANAGER, m.view)?;
    let d = at(r, &IDLER, 0x718)?;
    let f = |offset: u64| fld(abi, &MANAGER, offset);
    let e = f(0x200)?;
    let f = |class: &Class, offset: u64| fld(abi, class, offset);
    let g = f(&IDLER, 0x4c0)?;
    let h = fld(abi, &MANAGER, dynamic)?;
    let i = img(abi, 0x372afc0)?;
    let j = img(abi, pk::DOCTRINE_VIEW.vtable)?;
    let k = fld(abi, &Class::new("CInline", 0x1234560), 0x28)?;
    let l = fld(abi, &MANAGER, m.kids[1])?;
    let v = Class::new(pk::DOCTRINE_VIEW.name, pk::DOCTRINE_VIEW.vtable);
    let n = fld(abi, &v, pk::VIEW_ROOT)?;
    Ok(())
}
const VIEWC: Class = Class::new("manager.view", 0);
fn chain(r: &Reader<'_>, idler: u64, abi: StringAbi) -> Result<u64> {
    let manager = r.u64(idler + fld(abi, &IDLER, 0x670)?)?;
    let view = mgmt::pointer(r, manager + fld(abi, &MANAGER, 0x190)?)?;
    r.u64(view + fld(abi, &VIEWC, 0xda8)?)
}
#[cfg(test)]
mod tests {
    const X: Class = Class::new("TestOnly", 0x1);
    fn t() { fld(abi, &X, 0x77); }
}
'''


class Scan(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.dir = tempfile.TemporaryDirectory()
        root = pathlib.Path(cls.dir.name)
        (root / 'launch').mkdir()
        (root / 'launch' / 'panelkit.rs').write_text(PANEL)
        (root / 'launch' / 'tool.rs').write_text(textwrap.dedent(TOOL))
        cls.sc = gen.scan(root)

    @classmethod
    def tearDownClass(cls):
        cls.dir.cleanup()

    def test_class_registry(self):
        c = self.sc['classes']
        self.assertEqual(c['CInGameIdler'], 0x35089D8)
        self.assertEqual(c['idler.manager'], 0)            # multi-line Class::new
        self.assertEqual(c['CInline'], 0x1234560)          # inline Class::new
        self.assertEqual(c['NDoctrines::CCountryDoctrineView'], 0x354E610)  # registry struct
        self.assertNotIn('TestOnly', c)                     # test module ignored

    def test_field_keys(self):
        f = self.sc['fields']
        self.assertEqual(f['CInGameIdler'], {0x540, 0x718, 0x4C0, 0x670})   # literal, helper fn, shadowed closure
        self.assertEqual(f['idler.manager'], {0x190, 0x3B8, 0x200, 0x18})  # const expr (also chain), MACOS table, closure, array
        self.assertEqual(f['CInline'], {0x28})
        self.assertNotIn(0xDEAD, f['CInGameIdler'])
        self.assertNotIn(0x999, f['idler.manager'])           # LINUX tables are not keys

    def test_parent_links(self):
        self.assertEqual(self.sc['via']['idler.manager'], {'CInGameIdler+0x670': 1})
        self.assertEqual(self.sc['via']['manager.view'], {'idler.manager+0x190': 1})

    def test_image_keys(self):
        self.assertIn(0x372AFC0, self.sc['image'])
        self.assertIn(0x354E610, self.sc['image'])           # through `use panelkit as pk`

    def test_unresolved_reported(self):
        reasons = {(u['kind'], u['expr']): u['reason'] for u in self.sc['unresolved']}
        self.assertEqual(reasons[('field', 'dynamic')], 'runtime value')
        self.assertIn(('field', 'pk::VIEW_ROOT'), reasons)    # runtime class `v`
        self.assertFalse(any(u['expr'] in ('class', 'offset', 'mac') for u in self.sc['unresolved']))

    def test_keys_document(self):
        doc = gen.keys_document(self.sc)
        self.assertEqual(doc['classes']['CInline'], dict(vtable='0x1234560', offsets=['0x28'],
                                                         declared=doc['classes']['CInline']['declared']))
        self.assertIn('0x372afc0', doc['image'])


class Helpers(unittest.TestCase):
    def test_strip_comments_keeps_strings_and_lines(self):
        t = 'a // x 0x1\n"//not" /* 0x2\n */ b'
        out = gen.strip_comments(t)
        self.assertNotIn('0x1', out)
        self.assertNotIn('0x2', out)
        self.assertIn('"//not"', out)
        self.assertEqual(out.count('\n'), 2)

    def test_split_args_and_balanced(self):
        self.assertEqual(gen.split_args('abi, &Class::new("a,b", 0x1), m.f[0]'),
                         ['abi', '&Class::new("a,b", 0x1)', 'm.f[0]'])
        self.assertEqual(gen.balanced('f(a(b), ")")) tail', 1), 12)

    def test_mangle_class_name(self):
        self.assertEqual(gen.mangle_class_name('CAgencyUpgradeDatabase'), '22CAgencyUpgradeDatabase')
        self.assertEqual(gen.mangle_class_name('NProject::CSpecializationDatabase'),
                         'N8NProject23CSpecializationDatabaseE')
        self.assertIsNone(gen.mangle_class_name('idler.manager'))
        self.assertIsNone(gen.mangle_class_name('supply_node.size'))

    def test_forwarder_shadowing(self):
        defs = [(10, 'fld', 0, 1), (50, 'fld', ('fixed', '&X'), 0)]
        self.assertEqual(gen.forwarder_at(defs, 30), ('fld', 0, 1))
        self.assertEqual(gen.forwarder_at(defs, 90), ('fld', ('fixed', '&X'), 0))


class CheckTable(unittest.TestCase):
    GOOD = {'build_id': 'a4971c9e8ac9a6f2487dea109e01f9839ee59db7',
            'image': {'0x36511f0': '0x4211000'},
            'fields': {'CCountryPoliticsView': {'0x3958': '0x3b10'}}}

    def test_valid(self):
        errs, cov = gen.check_table(self.GOOD)
        self.assertEqual(errs, [])
        self.assertIsNone(cov)

    def test_invalid_entries(self):
        bad = dict(self.GOOD, image={'12': '0x1', '0x2': 3}, fields={'C': {'0x8': '8'}, 'D': []})
        errs, _ = gen.check_table(bad)
        self.assertEqual(len(errs), 4)
        errs, _ = gen.check_table({'image': {}})
        self.assertIn('build_id missing or not a string', errs)

    def test_coverage_against_keys(self):
        keys = {'image': {'0x36511f0': [], '0x1000': []},
                'classes': {'CCountryPoliticsView': {'offsets': ['0x3958', '0x40']}}}
        _, cov = gen.check_table(self.GOOD, keys)
        self.assertEqual(cov['image'], '1/2')
        self.assertEqual(cov['fields'], '1/2')
        self.assertEqual(cov['missing_fields'], ['CCountryPoliticsView+0x40'])


if __name__ == '__main__':
    unittest.main()
