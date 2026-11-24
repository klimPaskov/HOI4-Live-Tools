import json
import pathlib
import sys
import tempfile
import unittest

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1] / 'scripts'))
import macos_build_variant as V  # noqa: E402

OLD, NEW = 'DFA875A5-25BA-35E0-9736-627F8A5C4396', 'ED135FE5-CDA8-38E8-8835-3F7381B79974'
OLD_SHA, NEW_SHA = 'a' * 64, 'b' * 64


def mapping(**variant):
    v = dict(image_range=['0x8dd0', '0x39d4db0'], not_image={'0x10000': 'pointer floor'},
             shared_key_files=['native/src/shared.rs'])
    v.update(variant)
    return dict(system='macos', **{'from': dict(build_id=OLD, sha256=OLD_SHA)},
                to=dict(build_id=NEW, sha256=NEW_SHA, game_version='1.20.0.0'),
                image={'0x35089d8': dict(new='0x350ce78', confidence='high', how='vtable'),
                       '0x372afc0': dict(new='0x3730150', confidence='medium', how='global'),
                       '0x2460fb0': dict(new='0x2464000', confidence='low', how='guess')},
                fields_default='identity', variant=v)


CRATE = {
    'src/builds.rs': f'pub const READER_BUILD: &str = "{OLD}";\npub const DISABLED: &[(&str, &str)] = &[];\n',
    'src/launch/macos/kit.rs': (
        'const IDLER: Class = Class::new("CInGameIdler", 0x35089d8);\n'
        'fn f(base: u64) -> bool { base + 0x372afc0 > 0x10000 } // 0x35089d8 in a comment stays\n'
        '#[cfg(test)]\nmod tests { const T: u64 = 0x35089d8; }\n'),
    'src/game.rs': (
        'pub(crate) const LINUX: Layout = Layout { gs: 0x35089d8 };\n'
        f'pub(crate) const MACOS: Layout = Layout {{ gs: 0x372afc0, build_id: "{OLD}" }};\n'),
    'src/shared.rs': 'const LINE: u64 = 0x35089d8;\n',
    'src/platform/macos.rs': f'pub const BUILD_UUID: &str = "{OLD}";\npub const SHA256: &str = "{OLD_SHA}";\n',
    'tests/fixture.rs': 'const X: u64 = 0x35089d8;\n',
}


class VariantTest(unittest.TestCase):
    def crate(self, extra=None):
        d = tempfile.TemporaryDirectory()
        self.addCleanup(d.cleanup)
        root = pathlib.Path(d.name) / 'native'
        for rel, text in {**CRATE, **(extra or {})}.items():
            p = root / rel
            p.parent.mkdir(parents=True, exist_ok=True)
            p.write_text(text, encoding='utf-8')
        return root, pathlib.Path(d.name)

    def generate(self, m, extra=None):
        root, tmp = self.crate(extra)
        mp = tmp / 'map.json'
        mp.write_text(json.dumps(m), encoding='utf-8')
        rc = V.main([str(mp), '--src', str(root), '--out', str(tmp / 'out'), '--report', str(tmp / 'r.json')])
        return rc, tmp / 'out' / 'native', json.loads((tmp / 'r.json').read_text(encoding='utf-8'))

    def test_rewrites_macos_literals_and_identity_only(self):
        rc, out, report = self.generate(mapping())
        self.assertEqual(rc, 0)
        kit = (out / 'src/launch/macos/kit.rs').read_text(encoding='utf-8')
        self.assertIn('Class::new("CInGameIdler", 0x350ce78)', kit)
        self.assertIn('base + 0x3730150 > 0x10000', kit)            # not_image keeps the pointer floor
        self.assertIn('// 0x35089d8 in a comment stays', kit)
        self.assertIn('const T: u64 = 0x35089d8', kit)               # test code is not rewritten
        game = (out / 'src/game.rs').read_text(encoding='utf-8')
        self.assertIn('LINUX: Layout = Layout { gs: 0x35089d8 }', game)  # Linux layout untouched
        self.assertIn(f'gs: 0x3730150, build_id: "{NEW}"', game)
        self.assertIn('LINE: u64 = 0x350ce78', (out / 'src/shared.rs').read_text(encoding='utf-8'))
        mac = (out / 'src/platform/macos.rs').read_text(encoding='utf-8')
        self.assertIn(NEW, mac)
        self.assertIn(NEW_SHA, mac)
        self.assertIn(f'READER_BUILD: &str = "{NEW}"', (out / 'src/builds.rs').read_text(encoding='utf-8'))
        self.assertIn('0x35089d8', (out / 'tests/fixture.rs').read_text(encoding='utf-8'))
        self.assertEqual(report['summary']['sites']['image:high'], 2)
        self.assertEqual(report['summary']['sites']['image:medium'], 2)
        self.assertEqual(report['problems'], [])

    def test_unresolved_or_low_literal_is_an_error(self):
        extra = {'src/gui/macos.rs': 'fn g(base: u64) -> u64 { base + 0x2460fb0 + 0x37ed910 }\n'}
        rc, out, report = self.generate(mapping(), extra)
        self.assertEqual(rc, 1)
        self.assertFalse(out.exists())
        self.assertEqual({p['old'] for p in report['problems']}, {'0x2460fb0', '0x37ed910'})
        self.assertEqual({p['confidence'] for p in report['problems']}, {'low', 'unresolved'})

    def test_disabled_module_gets_poison_addresses_and_its_reason(self):
        extra = {'src/gui/macos.rs': 'fn g(base: u64) -> u64 { base + 0x2460fb0 }\n'}
        rc, out, report = self.generate(mapping(disabled={'native/src/gui/macos.rs': 'Cocoa callbacks unresolved'}), extra)
        self.assertEqual(rc, 0)
        self.assertIn(hex(V.POISON + 0x2460fb0), (out / 'src/gui/macos.rs').read_text(encoding='utf-8'))
        builds = (out / 'src/builds.rs').read_text(encoding='utf-8')
        self.assertIn('("native/src/gui/macos.rs", "Cocoa callbacks unresolved")', builds)
        self.assertEqual(report['summary']['sites']['disabled'], 1)


    def test_single_test_item_does_not_end_the_scan(self):
        extra = {'src/launch/macos/late.rs': (
            '#[cfg(test)]\nuse std::fmt;\n'
            'const A: u64 = 0x35089d8;\n'
            '#[cfg(test)]\nfn helper() -> u64 {\n    0x35089d8\n}\n'
            'const B: u64 = 0x372afc0;\n')}
        rc, out, report = self.generate(mapping(), extra)
        self.assertEqual(rc, 0)
        late = (out / 'src/launch/macos/late.rs').read_text(encoding='utf-8')
        self.assertIn('const A: u64 = 0x350ce78', late)
        self.assertIn('    0x35089d8\n', late)                      # the test function stays
        self.assertIn('const B: u64 = 0x3730150', late)

    def test_map_key_outside_a_macos_context_is_rewritten(self):
        extra = {'src/tree.rs': (
            'fn linux() { const BUTTON: Class = Class::new("CButtonStandard", 0x35089d8); }\n'
            'fn f(base: u64) -> u64 { base + 0x1234567 }\n'),
            'src/elf.rs': 'const LINUX_TABLE: [u64; 1] = [0x35089d8];\n',
            'src/skip.rs': 'const K: u64 = 0x35089d8;\n'}
        rc, out, report = self.generate(mapping(not_macos={'native/src/skip.rs': 'Linux literal'}), extra)
        self.assertEqual(rc, 0)
        tree = (out / 'src/tree.rs').read_text(encoding='utf-8')
        self.assertIn('Class::new("CButtonStandard", 0x350ce78)', tree)
        self.assertIn('base + 0x1234567', tree)                      # not a key: left alone
        self.assertIn('[0x35089d8]', (out / 'src/elf.rs').read_text(encoding='utf-8'))
        self.assertIn('0x35089d8', (out / 'src/skip.rs').read_text(encoding='utf-8'))
        self.assertTrue(any(s.get('context') == 'key' for s in report['sites']))

    def test_files_of_a_test_module_are_skipped(self):
        extra = {'src/own.rs': '#[cfg(test)]\npub(crate) mod fixture;\n',
                 'src/own/fixture.rs': 'const F: u64 = 0x35089d8;\n'}
        rc, out, _ = self.generate(mapping(), extra)
        self.assertEqual(rc, 0)
        self.assertIn('0x35089d8', (out / 'src/own/fixture.rs').read_text(encoding='utf-8'))

    def test_verify_finds_a_leftover_old_address(self):
        rc, out, _ = self.generate(mapping())
        self.assertEqual(rc, 0)
        m = mapping()
        self.assertEqual(V.verify(out, m), [])
        (out / 'src/launch/macos/hand.rs').write_text('const H: u64 = 0x35089d8;\n', encoding='utf-8')
        left = V.verify(out, m)
        self.assertEqual([x['old'] for x in left], ['0x35089d8'])
        mp = out.parent / 'm.json'
        mp.write_text(json.dumps(m), encoding='utf-8')
        self.assertEqual(V.main([str(mp), '--verify', str(out.parent)]), 1)


class CommittedMapTest(unittest.TestCase):
    def test_every_macos_literal_of_the_sources_maps_to_1_20(self):
        root = pathlib.Path(__file__).resolve().parents[1]
        m = json.loads((root / 'native/layouts/migrations/macos-DFA875A5-to-ED135FE5.json').read_text(encoding='utf-8'))
        _, sites, problems = V.plan(root / 'native', m)
        self.assertEqual(problems, [], 'a macOS game address in native/src has no 1.20 entry in '
                         'native/layouts/migrations/macos-DFA875A5-to-ED135FE5.json: find it with scripts/macos_resolve.py '
                         'and add it (docs/GAME_VERSIONS.md, "Adding a macOS game address")')
        self.assertEqual(m['to']['build_id'], 'ED135FE5-CDA8-38E8-8835-3F7381B79974')
        self.assertGreater(sum(s['kind'] == 'image' for s in sites), 600)
        self.assertEqual(sum(s['kind'] == 'identity' for s in sites) >= 3, True)

    def test_the_focus_and_widget_addresses_e69_patched_by_hand_are_generated(self):
        root = pathlib.Path(__file__).resolve().parents[1]
        m = json.loads((root / 'native/layouts/migrations/macos-DFA875A5-to-ED135FE5.json').read_text(encoding='utf-8'))
        _, sites, _ = V.plan(root / 'native', m)
        got = {s['site']: s['new'] for s in sites if s['kind'] == 'image'}
        for site, new in {'own_focus.rs:56': '0x358e128', 'construction/mac_find.rs:31': '0x366a438',
                          'construction/mac_find.rs:33': '0x3585618', 'launch/macos/panelkit.rs:627': '0x36d1be0',
                          'construction/mac_ui/selection_state.rs:384': '0x36aa940'}.items():
            self.assertEqual(got.get('native/src/' + site), new, site)
        self.assertIn('0x36be898', {got.get(k) for k in got if k.startswith('native/src/launch/macos/focus/binding.rs')})


if __name__ == '__main__':
    unittest.main()
