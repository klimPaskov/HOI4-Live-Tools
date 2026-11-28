"""evaluators/air.py: fighters and close air support a front needs; the installed game when present (Germany against
Poland in 1939)."""
import os
import sys
import unittest

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import evaluators_fixture as fx  # noqa: E402

from hoi4facts.evaluators import air  # noqa: E402


class AirFixtureTest(unittest.TestCase):
    def test_no_planes_no_need_and_the_answer_says_why(self):
        r = air.need(fx.situation(), ["BBB"], axes=2)
        self.assertEqual(r["fighters"], 0)
        self.assertEqual(r["cas"], 0)
        self.assertIn("parity", r["terms"]["fighter_ratio_from"])
        self.assertIn("no close-air-support airframe", r["why"]["cas"])

    def test_live_fighters_override_the_start_wings(self):
        sit = fx.situation(state={"enemy_fighters": {"BBB": 120}})
        got, src = air.enemy_fighters(sit, ["BBB"])
        self.assertEqual(got, {"BBB": 120})
        self.assertIn("live", src)
        r = air.need(sit, ["BBB"], fighter_ratio=1.5)
        self.assertEqual(r["fighters"], 180)                            # 120 x 1.5 (no designs to compare: quality 1)


BBB_OOB = """
division_template = { name = "Levy" regiments = { infantry = { x = 0 y = 0 } } }
units = { division = { location = 20 division_template = "Levy" } }
air_wings = { 2 = { fighter_equipment_1 = { owner = "BBB" amount = 10 } } }
instant_effect = {
    add_equipment_production = { equipment = { type = fighter_equipment_1 creator = "BBB" } requested_factories = 2 }
}
"""


class AirGrowthTest(unittest.TestCase):
    def test_enemy_fighters_grow_by_their_start_lines(self):
        sit = fx.situation(extra={"history/units/BBB_1936.txt": BBB_OOB})
        start, src0 = air.enemy_fighters(sit, ["BBB"])
        self.assertEqual(start, {"BBB": 10})
        grown, src = air.enemy_fighters(sit, ["BBB"], war_date="1937.1.1")
        from hoi4facts.evaluators import production
        made = int(2 * production.Efficiency(sit.other("BBB")).ic_per_factory(365) / 20)
        self.assertEqual(grown["BBB"], 10 + made)
        self.assertGreater(made, 0)
        self.assertIn("start fighter lines", src)


@unittest.skipUnless(fx.real_situation("GER") is not None, "installed game not found (HOI4_EVAL_GAME / HOI4_DIR)")
class AirRealGameTest(unittest.TestCase):
    def test_germany_against_poland(self):
        sit = fx.real_situation("GER", "1939.9.1", bookmark="1936.1.1")
        r = air.need(sit, ["POL"], axes=3)
        t = r["terms"]
        self.assertGreater(t["enemy_fighters"]["POL"], 0)
        self.assertEqual(r["fighters"], -(-int(sum(t["enemy_fighters"].values()) * t["quality_ratio"] * 10) // 10))
        self.assertGreater(t["exchange"], 1)                             # 1939 German fighter against the start PZL
        self.assertEqual(t["planes_per_enemy_width"], 3)                 # LAND_AIR_COMBAT_MAX_PLANES_PER_ENEMY_WIDTH
        self.assertEqual(r["cas_cap"], int(3 * t["battle_width"] * 3))
        self.assertGreater(r["cas"], 0)


if __name__ == "__main__":
    unittest.main()
