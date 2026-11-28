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
