"""The Linux address table covers every key the tool sources use, or the key is listed as unmapped.

xlayout fails a read whose key is missing from the table with `layout_unmapped` (never a guess), so a
tool added on macOS silently refuses on Linux until its keys are mapped. This test makes every new key
visible: it must be mapped (scripts/gen_linux_layout.py `extend`, `generate` with the ELF, or a live
entry) or listed in scripts/data/xlayout-unmapped-1.19.4.1.json, which docs/PORTING_STATUS.md explains.
Static scan only; no game binary is read.
"""

import importlib.util
import json
import pathlib
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[1]
_spec = importlib.util.spec_from_file_location('gen_linux_layout', ROOT / 'scripts' / 'gen_linux_layout.py')
gen = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(gen)

TABLE = ROOT / 'native' / 'layouts' / 'linux-2265e6e7c1f527bf4b1f2c4b4ca3efd4371dc082.json'
BASELINE = ROOT / 'scripts' / 'data' / 'xlayout-unmapped-1.19.4.1.json'
REFRESH = ('python3 scripts/gen_linux_layout.py extend ' + str(TABLE.relative_to(ROOT)) +
           ' --unmapped ' + str(BASELINE.relative_to(ROOT)))


class LinuxTableCoverage(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        keys = gen.keys_document(gen.scan(str(ROOT / 'native' / 'src')))
        table = json.loads(TABLE.read_text())
        cls.missing = gen.unmapped_by_file(table, keys)
        cls.baseline = json.loads(BASELINE.read_text())

    def test_new_keys_are_mapped_or_listed(self):
        for kind in ('image', 'fields'):
            new = sorted(set(self.missing[kind]) - set(self.baseline[kind]))
            self.assertEqual(new, [], f'{kind} keys used by the sources but absent from the Linux table: '
                             f'{dict((k, self.missing[kind][k]) for k in new)}. Map them, or list them with: {REFRESH}')

    def test_listed_keys_are_still_unmapped(self):
        for kind in ('image', 'fields'):
            stale = sorted(set(self.baseline[kind]) - set(self.missing[kind]))
            self.assertEqual(stale, [], f'{kind} keys listed as unmapped are now mapped or no longer used; '
                             f'refresh the list: {REFRESH}')


if __name__ == '__main__':
    unittest.main()
