"""evaluators/production.py: the efficiency curve and the line-change rule from the defines, factories per line by
shortfall and time-to-need; the installed game when present."""
import os
import sys
import unittest

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import evaluators_fixture as fx  # noqa: E402

from hoi4facts import divstats  # noqa: E402
from hoi4facts.evaluators import production  # noqa: E402


class ProductionFixtureTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.sit = fx.situation(state={"techs": ["e_inf", "e_tank_1", "e_tank_2"]})
        cls.eff = production.Efficiency(cls.sit)

    def test_efficiency_curve(self):
        e = self.eff
        self.assertEqual((e.start, e.cap), (10.0, 50.0))
        one = e.ic_per_factory(1)
        self.assertAlmostEqual(one, e.speed * e.output * 0.10)
        self.assertGreater(e.ic_per_factory(400) / 400, e.ic_per_factory(100) / 100)   # the ramp
        n = e.days_to_cap()
        self.assertAlmostEqual(n, (50 ** 2 - 10 ** 2) / (2 * e.k * 50 ** 2 / 100), delta=3)
        self.assertGreater(e.ic_per_factory(100, start=45), e.ic_per_factory(100))

    def test_line_change_rule(self):
        data = divstats.Data(self.sit.game)
        self.assertEqual(production.relation(data, "mt_chassis_1", "mt_chassis_1"), "variant")
        self.assertEqual(production.relation(data, "mt_chassis_1", "mt_chassis_2"), "parent")
        self.assertIsNone(production.relation(data, "mt_chassis_1", "infantry_equipment_1"))
        kept, rel = production.kept_efficiency(self.sit, "mt_chassis_1", "mt_chassis_2", 50.0, self.eff)
        self.assertEqual((kept, rel), (50.0 * self.eff.change["parent"] / 100, "parent"))

    def test_factories_cover_the_shortfall(self):
        sit = fx.situation(state={"techs": ["e_inf"], "stockpile": {"infantry_equipment": 0},
                                  "factories": {"civilian": 5, "military": 50, "naval": 0}})
        r = production.evaluate(sit, "1937.1.1")
        rows = {ln["equipment"]: ln for ln in r["lines"]}
        if "infantry_equipment" in rows and rows["infantry_equipment"]["ic"]:
            ln = rows["infantry_equipment"]
            self.assertGreaterEqual(ln["factories"] * ln["ic_per_factory"], ln["ic"])
            self.assertLess((ln["factories"] - 1) * ln["ic_per_factory"], ln["ic"])


@unittest.skipUnless(fx.real_situation("GER") is not None, "installed game not found (HOI4_EVAL_GAME / HOI4_DIR)")
class ProductionRealGameTest(unittest.TestCase):
    def test_germany_40_line_divisions_by_1939(self):
        r = production.evaluate(fx.real_situation("GER"), "1939.9.1", add={"line": 40}, enemies=["POL"])
        inf = [ln for ln in r["lines"] if ln["equipment"] == "infantry_equipment"][0]
        self.assertGreater(inf["factories"], 0)
        self.assertLessEqual(sum(ln.get("assigned", 0) for ln in r["lines"]), 28)
        self.assertIsNotNone(r["logistics"]["rail_distance"])

    def test_minor(self):
        r = production.evaluate(fx.real_situation("HUN"), "1939.1.1", add={"line": 6}, enemies=["CZE"])
        self.assertTrue(r["lines"])


if __name__ == "__main__":
    unittest.main()


UNITS = """
sub_units = {
	infantry = { group = infantry combat_width = 2 manpower = 1000 training_time = 90 need = { infantry_equipment = 100 } }
	artillery = { group = support regimental = no combat_width = 0 manpower = 500 training_time = 120
		need = { artillery_equipment = 12 support_equipment = 5 } }
}
"""
TRAINING_DEFINES = """
NDefines.NCountry.MIN_POSSIBLE_TRAINING_MANPOWER = 10000
NDefines.NCountry.MIN_FIELD_TO_TRAINING_MANPOWER_RATIO = 0.75
"""


class TrainingSizeTest(unittest.TestCase):
    """How many divisions of a template to keep in training at once (the owner's question of 28 Nov)."""

    def sit(self, **state):
        base = {"techs": ["e_inf"], "stockpile": {"infantry_equipment": 1000, "artillery_equipment": 100,
                                                  "support_equipment": 100},
                "production_output": {"infantry_equipment": 2.0}, "manpower": 20000, "field_manpower": 5000}
        base.update(state)
        return fx.situation(state=base, extra={"common/units/infantry.txt": UNITS,
                                               "common/defines/zz_e121.lua": TRAINING_DEFINES})

    def test_terms_and_binding(self):
        r = production.training(self.sit(), "Line", count=10, need_date="1937.1.1")
        self.assertEqual(r["training_days"], 120)                     # the longest sub-unit training_time
        self.assertEqual(r["terms"]["equipment"], 6)                  # (1000 + 2 x 120) / 200 infantry equipment
        self.assertEqual(r["terms"]["manpower"], 8)                   # 20000 / 2500
        self.assertEqual(r["terms"]["cap"], 4)                        # max(10000, 0.75 x 5000) / 2500
        self.assertEqual(r["terms"]["rate"], 4)                       # ceil(10 / (365 // 120))
        self.assertEqual(r["in_training"], 4)
        self.assertEqual(sorted(r["binding"]), ["cap", "rate"])
        self.assertEqual(r["lines"], 1)

    def test_equipment_binds_and_says_so(self):
        r = production.training(self.sit(stockpile={}, production_output={}), "Line", count=10, need_date="1937.1.1")
        self.assertEqual(r["in_training"], 0)
        self.assertIn("equipment", r["binding"])
        self.assertIn("nothing to train yet", r["note"])

    def test_training_already_running_lowers_the_cap(self):
        r = production.training(self.sit(training_manpower=5000), "Line")
        self.assertEqual(r["terms"]["cap"], 2)                        # (10000 - 5000) / 2500
        self.assertNotIn("rate", r["terms"])                          # no count: as many as the means allow
        self.assertEqual(r["in_training"], 2)


@unittest.skipUnless(fx.real_situation("GER") is not None, "installed game not found (HOI4_EVAL_GAME / HOI4_DIR)")
class TrainingRealGameTest(unittest.TestCase):
    def test_germany_line_divisions(self):
        r = production.training(fx.real_situation("GER"), "Infanterie-Division", count=20, need_date="1939.9.1",
                                enemies=["POL"])
        self.assertGreater(r["training_days"], 0)
        self.assertIn("manpower", r["terms"])
        self.assertIn("cap", r["terms"])
        self.assertLessEqual(r["in_training"], 20)
