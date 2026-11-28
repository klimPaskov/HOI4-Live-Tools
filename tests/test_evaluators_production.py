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
