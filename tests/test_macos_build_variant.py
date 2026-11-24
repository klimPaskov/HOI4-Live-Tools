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


if __name__ == '__main__':
    unittest.main()
