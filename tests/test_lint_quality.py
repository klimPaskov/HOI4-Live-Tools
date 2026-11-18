"""lint_quality.py: the quality of the force on a war day (templates against the AI's, staff, exercises)."""

import os
import sys
import unittest

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.join(ROOT, "scripts"))
sys.path.insert(0, os.path.join(ROOT, "tests"))

from hoi4facts import derive, lint_plan, lint_quality  # noqa: E402
from test_force_design import GAME_DIR, HAVE_GAME, germany_at_war  # noqa: E402


class Baseline(unittest.TestCase):
    def test_the_ai_templates_of_a_date(self):
        row = lint_quality.ai_templates("GER", derive.day("1940.5.10"))
        self.assertEqual(row["date"], "1939.09.01")
        self.assertEqual(row["line"].count("artillery_brigade"), 1)
        self.assertEqual(row["armour"].count("medium_armor"), 2)
        self.assertIsNone(lint_quality.ai_templates("GER", derive.day("1938.1.1")))
        self.assertIsNone(lint_quality.ai_templates("XXX", derive.day("1940.5.10")))


@unittest.skipUnless(HAVE_GAME, f"game files not found at {GAME_DIR}")
class Germany(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.game = lint_plan.load_game(GAME_DIR)
        cls.out = lint_plan.lint(germany_at_war(), cls.game)

    def errors(self, rule):
        return [f["message"] for f in self.out["findings"] if f["rule"] == rule and f["severity"] == "error"]

    def test_templates_are_held_to_the_ai_floor(self):
        self.assertEqual(lint_quality.shape(self.game, ["infantry"] * 9 + ["artillery_brigade", "engineer"]),
                         {"battalions": 10, "artillery": 1, "armour": 0, "medium": 0, "mobile": 0, "width": 21.0})
        found = self.errors("force.template_floor")
        self.assertTrue(any("0 line artillery battalion(s)" in m and "the game's own AI fields 1" in m for m in found),
                        found)
        self.assertTrue(any("medium or heavier tanks" in m for m in found), found)
        row = self.out["facts"]["force"]["quality"][0]
        self.assertEqual(row["line"]["width"], 18.0)

    def test_the_staff_and_the_officer_corps_are_planned(self):
        found = self.errors("force.staff")
        self.assertTrue(found and "officer_corps" in found[0] and "theorist" in found[0], found)


if __name__ == "__main__":
    unittest.main()
