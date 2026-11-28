"""evaluators/research.py: days with the years-ahead penalty, values from the files, XOR and parents in the slot
simulation, bonus advice; the installed game when present (Germany, a minor, a major)."""
import os
import sys
import unittest

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import evaluators_fixture as fx  # noqa: E402

from hoi4facts.evaluators import research  # noqa: E402
from test_evaluators_construction import BUILDINGS  # noqa: E402

TECHS = {"common/technologies/zz_research.txt": """technologies = {
    r_industry = { start_year = 1936 research_cost = 1 industrial_capacity_factory = 0.1 categories = { industry }
        path = { leads_to_tech = r_industry2 } }
    r_industry2 = { start_year = 1938 research_cost = 1 industrial_capacity_factory = 0.1 categories = { industry } }
    r_build = { start_year = 1936 research_cost = 1 production_speed_buildings_factor = 0.1 categories = { industry }
        XOR = { r_build_alt } }
    r_build_alt = { start_year = 1936 research_cost = 1 production_speed_buildings_factor = 0.05 categories = { industry }
        XOR = { r_build } }
    r_comp = { start_year = 1936 research_cost = 2 research_speed_factor = 0.05 categories = { electronics } }
}
"""}


class ResearchFixtureTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.sit = fx.situation(extra=dict(TECHS, **BUILDINGS), state={"techs": ["e_inf"], "research_slots": 2,
                                                   "factories": {"civilian": 10, "military": 10, "naval": 0}})
        cls.m = research.Model(cls.sit, horizon_years=4)

    def test_days_with_the_years_ahead_penalty(self):
        base = self.m.btc / (1 + self.m.speed)
        self.assertAlmostEqual(self.m.days("r_industry", 0), base)
        self.assertAlmostEqual(self.m.days("r_industry2", 0), base * (1 + self.m.pen * 2))
        self.assertAlmostEqual(self.m.days("r_industry2", 0, ahead_reduction=1), base * (1 + self.m.pen))
        self.assertAlmostEqual(self.m.days("r_industry2", 730), base)

    def test_values_from_the_weights(self):
        c = self.m.coef["r_industry"]
        self.assertAlmostEqual(c["lin"], 0.1 * 10)                       # +10% output of 10 military factories a year
        self.assertGreater(self.m.coef["r_build"]["quad"], 0)            # construction compounds
        self.assertGreater(self.m.coef["r_comp"]["lin"], 0)              # research speed: the research it speeds up

    def test_plan_respects_xor_and_parents(self):
        plan = self.m.plan(slots=2, picks=3)
        order = [p["tech"] for sl in plan for p in sl]
        self.assertFalse({"r_build", "r_build_alt"} <= set(order))
        self.assertIn("r_build", order)                                  # the larger bonus of the pair
        if "r_industry2" in order:
            first = {p["tech"]: p["start_day"] for sl in plan for p in sl}
            self.assertGreaterEqual(first["r_industry2"], first["r_industry"] + 1)

    def test_bonus_advice(self):
        adv = research.bonus_advice(self.m, [{"name": "b", "categories": ["industry"], "bonus": 1.0,
                                              "ahead_reduction": 2, "uses": 1}])[0]
        self.assertEqual(adv["best_any"], "r_industry2")                 # ahead reduction saves the most there
        self.assertIn("hold for r_industry2", adv["advice"])


@unittest.skipUnless(fx.real_situation("GER") is not None, "installed game not found (HOI4_EVAL_GAME / HOI4_DIR)")
class ResearchRealGameTest(unittest.TestCase):
    def test_germany_industry_first(self):
        r = research.evaluate(fx.real_situation("GER"), enemies=["POL"], picks=2)
        first = {sl[0]["tech"] for sl in r["picks"]}
        self.assertEqual(len(r["picks"]), 4)
        self.assertTrue(first & {"construction1", "basic_machine_tools", "electronic_mechanical_engineering"})
        order = [p["tech"] for sl in r["picks"] for p in sl]
        self.assertFalse({"concentrated_industry", "dispersed_industry"} <= set(order))

    def test_minor_and_major(self):
        for tag, enemy in (("ETH", "ITA"), ("SOV", "GER")):
            r = research.evaluate(fx.real_situation(tag), enemies=[enemy], picks=1)
            self.assertTrue(all(sl for sl in r["picks"]), tag)


if __name__ == "__main__":
    unittest.main()
