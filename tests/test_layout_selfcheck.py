"""scripts/layout_selfcheck.py: memory reads compared with the values of a save, module by module."""

import importlib.util
import pathlib
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[1]
_spec = importlib.util.spec_from_file_location('layout_selfcheck', ROOT / 'scripts' / 'layout_selfcheck.py')
sc = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(sc)

SAVE = {'save_date': '1936.01.01.12', 'politics': {'political_power': 12.314},
        'production': {'military_factories_assigned': 14}, 'military': {'divisions': 46},
        'research': {'researching': [{'tech': 'infantry_weapons1'}, {'tech': 'gw_artillery'}]},
        'focus': {'current': 'GER_rhineland'}}


class Compare(unittest.TestCase):
    def test_agreeing_reads_prove_every_module(self):
        mem = {'build': 'X', 'date': '1936.1.1', 'political_power': 12.31, 'military_factories_assigned': 14,
               'divisions': 46, 'researching': ['gw_artillery', 'infantry_weapons1'], 'focus': 'GER_rhineland'}
        got = sc.compare(mem, SAVE)
        self.assertTrue(got['agrees'])
        self.assertEqual(sorted(got['modules']),
                         ['army', 'game', 'own_focus', 'politics', 'production_lines', 'research'])

    def test_one_wrong_offset_disables_only_its_module(self):
        mem = {'date': '1936.1.1', 'divisions': 45, 'political_power': 12.31}
        got = sc.compare(mem, SAVE)
        self.assertFalse(got['agrees'])
        self.assertFalse(got['modules']['army']['agrees'])
        self.assertTrue(got['modules']['game']['agrees'])
        self.assertTrue(got['modules']['politics']['agrees'])
        self.assertNotIn('research', got['modules'])

    def test_a_value_missing_from_the_save_is_not_a_failure(self):
        got = sc.compare({'focus': 'GER_rhineland'}, {'save_date': '1936.1.1'})
        self.assertTrue(got['agrees'])
        self.assertIsNone(got['modules']['own_focus']['checks'][0]['agrees'])


if __name__ == '__main__':
    unittest.main()
