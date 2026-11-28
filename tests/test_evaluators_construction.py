"""evaluators/construction.py: the split-bonus break-even, its two horizons, the modifier list it prints; the
installed game when present (Germany 1936: the reports' 1.5 against 1.92 settled, a minor with an arms penalty)."""
import os
import sys
import unittest

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import evaluators_fixture as fx  # noqa: E402

from hoi4facts.evaluators import construction  # noqa: E402

BUILDINGS = {"common/buildings/00_buildings.txt": """buildings = {
    infrastructure = { base_cost = 6000 infrastructure = yes level_cap = { state_max = 5 } }
    arms_factory = { base_cost = 7200 base_cost_conversion = 4000 military_production = 1
        level_cap = { shares_slots = yes state_max = 20 } }
    industrial_complex = { base_cost = 10800 general_production = 1 level_cap = { shares_slots = yes state_max = 20 } }
    dockyard = { base_cost = 6400 naval_production = 1 level_cap = { shares_slots = yes state_max = 20 } }
}
"""}


class ConstructionFixtureTest(unittest.TestCase):
    def test_split_bonus_and_horizons(self):
        sit = fx.situation(extra=BUILDINGS, state={"modifiers": {"production_speed_arms_factory_factor": 0.25,
                                                                 "production_speed_buildings_factor": 0.05}})
        r = construction.evaluate(sit, war_date="1939.1.1", horizon_years_after=2)
        self.assertEqual((r["civilian_building"], r["military_building"]), ("industrial_complex", "arms_factory"))
        self.assertAlmostEqual(r["b_civ"], 0.05)
        self.assertAlmostEqual(r["b_mil"], 0.30)
        self.assertAlmostEqual(r["k"], round((10800 / 1.05) / (7200 / 1.30), 3))
        self.assertAlmostEqual(r["k_single_bonus"], 1.5)
        self.assertAlmostEqual(r["consumer_goods"], 0.25)                   # partial_mob in the history
        p0 = 7200 / (5 * 1.30 * (1 + 0.2 * r["infrastructure"]))
        self.assertAlmostEqual(r["P0_days"], round(p0, 1), places=1)
        g = (1 - 0.25 + r["k"] * 0.25) / p0
        self.assertAlmostEqual(r["standing_days"], round(r["k"] / g, 1), places=0)
        self.assertAlmostEqual(r["output_days"], 2 * r["standing_days"], places=0)
        self.assertTrue(any(src == "live read" for src, _, _ in r["terms_mil"]))
        self.assertEqual(r["conversion"]["conversion_cost"], 4000)
        self.assertIn("decision", r)

    def test_no_war_date_no_decision(self):
        r = construction.evaluate(fx.situation(extra=BUILDINGS))
        self.assertNotIn("decision", r)
        self.assertAlmostEqual(r["k"], 1.5)


@unittest.skipUnless(fx.real_situation("GER") is not None, "installed game not found (HOI4_EVAL_GAME / HOI4_DIR)")
class ConstructionRealGameTest(unittest.TestCase):
    def test_germany_1936(self):
        r = construction.evaluate(fx.real_situation("GER"), war_date="1939.9.1")
        self.assertAlmostEqual(r["b_mil"], 0.25)                            # limited exports, partial mob, MEFO 0.1
        self.assertAlmostEqual(r["k"], 1.786, places=3)
        srcs = " ".join(src for src, _, _ in r["terms_mil"])
        self.assertIn("GER_mefo_bills_modifier", srcs)
        self.assertTrue(any(g["needs"] == "industrial_complex > 49" for g in r["gates"]))
        self.assertTrue(r["last_civilian_standing"] and r["last_civilian_standing"] < "1939")

    def test_minor_with_an_arms_penalty(self):
        r = construction.evaluate(fx.real_situation("HUN"), war_date="1940.1.1")
        self.assertLess(r["b_mil"], 0)                                      # the Trianon idea
        self.assertLess(r["k"], 1.0)


if __name__ == "__main__":
    unittest.main()
