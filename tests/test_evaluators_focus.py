"""evaluators/focus.py: the reward walker, goals and constraints, the order under prerequisites and exclusions;
the installed game when present (Germany with goals, Hungary)."""
import os
import sys
import unittest

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import evaluators_fixture as fx  # noqa: E402

from hoi4facts.evaluators import focus  # noqa: E402
from test_evaluators_construction import BUILDINGS  # noqa: E402


class FocusFixtureTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.sit = fx.situation(extra=BUILDINGS, state={"factories": {"civilian": 10, "military": 10, "naval": 0}})

    def test_order_values_buildings_and_respects_exclusions(self):
        r = focus.evaluate(self.sit, count=5, research_model=False)
        order = [o["focus"] for o in r["order"]]
        self.assertEqual(order[0], "AAA_root")
        self.assertFalse({"AAA_left", "AAA_right"} <= set(order))
        left = [o for o in r["order"] if o["focus"] == "AAA_left"]
        self.assertTrue(left and left[0]["parts"]["build arms_factory x2"]["per_year"] == 2.0)
        self.assertIn("add_political_power", r["order"][0]["unvalued"])

    def test_no_war_constraint_drops_the_wargoal_focus(self):
        r = focus.evaluate(self.sit, goals=["no_war:BBB"], count=5, research_model=False)
        self.assertIn("AAA_right", r["dropped"])
        self.assertNotIn("AAA_right", [o["focus"] for o in r["order"]])

    def test_walker_ideas_timed_and_removed(self):
        w = focus.Walker(self.sit)
        out = {"lin": 0.0, "quad": 0.0, "stock": 0.0, "parts": {}, "unvalued": [], "ideologies": set(),
               "war_with": set()}
        w.ideas = {"x": {"industrial_capacity_factory": 0.1}}
        w.walk([("add_timed_idea", [("idea", "x"), ("days", "365")]), ("remove_ideas", "x")], 1.0, out)
        self.assertAlmostEqual(out["stock"], 0.1 * 10)
        self.assertAlmostEqual(out["lin"], -0.1 * 10)


@unittest.skipUnless(fx.real_situation("GER") is not None, "installed game not found (HOI4_EVAL_GAME / HOI4_DIR)")
class FocusRealGameTest(unittest.TestCase):
    def test_focus_germany_goals_and_hungary(self):
        r = focus.evaluate(fx.real_situation("GER"), goals=["ideology:fascism", "no_war:SOV"], count=4)
        self.assertEqual(r["order"][0]["focus"], "GER_the_four_year_plan")
        self.assertIn("GER_war_with_the_ussr", r["dropped"])
        h = focus.evaluate(fx.real_situation("HUN"), count=3)
        self.assertEqual(len(h["order"]), 3)

if __name__ == "__main__":
    unittest.main()
