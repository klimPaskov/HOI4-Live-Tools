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

    def test_political_power_at_the_price_of_what_it_buys(self):
        ideas = """
ideas = {
    economy = {
        e_cheap = { cost = 50 modifier = { industrial_capacity_factory = 0.1 } }
        e_dear = { cost = 150 modifier = { industrial_capacity_factory = 0.1 } }
        e_locked = { cost = 10 allowed = { always = no } modifier = { industrial_capacity_factory = 0.5 } }
        e_other = { cost = 10 allowed = { original_tag = BBB } modifier = { industrial_capacity_factory = 0.5 } }
    }
    trade_laws = {
        e_law_a = { cost = 100 modifier = { industrial_capacity_factory = 0.2 } }
        e_law_b = { cost = 100 modifier = { industrial_capacity_factory = 0.1 } }
    }
}
"""
        sit = fx.situation(extra=dict(BUILDINGS, **{"common/ideas/e119.txt": ideas,
                                                    "common/defines/zz_pp.lua":
                                                    "NDefines.NCountry.BASE_POLITICAL_POWER_INCREASE = 2\n"}),
                           state={"factories": {"civilian": 10, "military": 10, "naval": 0}})
        names = {n for n, _, _, _ in focus.pp_sinks(sit)}
        self.assertFalse({"e_locked", "e_other"} & names)               # never on offer / another country's
        price, how = focus.pp_price(sit, focus.value.bases(sit), years=4.0)
        # one per category, best first: e_cheap (1/50 a power), e_law_a (2/100); e_dear and e_law_b not bought
        self.assertAlmostEqual(price, (1.0 + 2.0) / (50 + 100))
        self.assertIn("e_cheap", how)
        w = focus.Walker(sit)
        out = {"lin": 0.0, "quad": 0.0, "stock": 0.0, "parts": {}, "unvalued": [], "ideologies": set(),
               "war_with": set()}
        w.walk([("add_political_power", "100")], 1.0, out)
        self.assertAlmostEqual(out["lin"], 100 * price)
        self.assertNotIn("add_political_power", out["unvalued"])


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
