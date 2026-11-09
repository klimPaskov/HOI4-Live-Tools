"""check_calls.py: player calls valid against the catalogue and never duplicating executor work."""
import json
import os
import sys
import tempfile
import unittest

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.join(ROOT, "scripts"))

from hoi4facts import check_calls  # noqa: E402


class Overlap(unittest.TestCase):
    PLAN = {"army_layout": [{"name_hint": "North_strike", "state": 550, "role": "strike"}],
            "war": {"enemies": ["ETH"]}, "production": [{"equipment": "infantry_equipment_1", "factories": 4}],
            "peace": {"take": [271]}}

    def run_on(self, brief):
        d = tempfile.mkdtemp()
        with open(os.path.join(d, "plan.json"), "w", encoding="utf-8") as f:
            json.dump(self.PLAN, f)
        with open(os.path.join(d, "plan.brief.md"), "w", encoding="utf-8") as f:
            f.write(brief)
        return check_calls.check(d)

    def test_executor_work_in_the_brief_fails(self):
        _, findings = self.run_on('* `hoi4_army_front {"army":"North_strike","state_id":842}`\n'
                                  '* `hoi4_production_set {"line":4,"equipment":"x","factories":0}`\n'
                                  '* `hoi4_peace_submit {}`\n')
        problems = [f["problem"] for f in findings if "duplicates the executor" in f["problem"]]
        self.assertEqual(len(problems), 3, findings)

    def test_reads_and_unplanned_work_pass(self):
        _, findings = self.run_on('* `hoi4_armies {"threat":false}`\n'
                                  '* `hoi4_army_move {"army":"Army 9","state_id":550}`\n')
        self.assertFalse([f for f in findings if "duplicates the executor" in f["problem"]], findings)


if __name__ == "__main__":
    unittest.main()
