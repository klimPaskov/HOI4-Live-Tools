"""evaluators/force.py: force needed and the land doctrines compared; the installed game when present
(Germany against Poland, Hungary against Czechoslovakia)."""
import os
import sys
import unittest

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import evaluators_fixture as fx  # noqa: E402

from hoi4facts.evaluators import force  # noqa: E402


@unittest.skipUnless(fx.real_situation("GER") is not None, "installed game not found (HOI4_EVAL_GAME / HOI4_DIR)")
class ForceRealGameTest(unittest.TestCase):
    def test_force_and_doctrines_germany_poland(self):
        sit = fx.real_situation("GER")
        r = force.needed(sit, "POL", "1939.9.1")
        self.assertGreater(r["enemy"]["divisions"], r["enemy"]["divisions_start"])
        self.assertEqual(r["front_provinces"], 25)
        self.assertGreaterEqual(r["total"], r["front_provinces"])
        d = force.doctrines(sit, ["POL"])
        names = {x["doctrine"] for x in d["rows"]}
        self.assertTrue({"new_mobile_warfare", "superior_firepower", "grand_battleplan", "mass_assault"} <= names)
        self.assertEqual(d["held"], ["new_mobile_warfare"])
        rows = {x["doctrine"]: x for x in d["rows"]}
        gbp = rows["grand_battleplan"]["attack"]["attacker"]["base"]["T14"]
        nmw = rows["new_mobile_warfare"]["attack"]["attacker"]["base"]["T14"]
        self.assertAlmostEqual(gbp["planning"], 0.3 + 0.1 - 0.01 * 14 / 2)      # PLANNING_MAX + max_planning - decay
        self.assertGreater(gbp["R"], nmw["R"])                                   # the higher cap, before milestones
        self.assertIn("coordination_bonus", rows["grand_battleplan"]["unvalued"]["all milestones"])

    def test_force_minor(self):
        r = force.needed(fx.real_situation("HUN"), "CZE", "1938.1.1")
        self.assertGreater(r["total"], 0)

if __name__ == "__main__":
    unittest.main()
