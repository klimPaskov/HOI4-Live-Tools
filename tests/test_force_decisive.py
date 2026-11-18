"""force.decisive, force.bombers, force.design_cost and the enemy grown to the war's day (force.py grown): the
dominance standard (rules-and-modes.md "What FAIR and META mean"). Reads the installed game; skipped without it."""

import os
import sys
import unittest

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.join(ROOT, "scripts"))
sys.path.insert(0, os.path.join(ROOT, "tests"))

from hoi4facts import force, lint_plan  # noqa: E402
from test_force_design import GAME_DIR, HAVE_GAME, germany_at_war  # noqa: E402


class Growth(unittest.TestCase):
    def test_the_ai_peace_rate_comes_from_the_baselines(self):
        rate = force.ai_growth_rate()
        self.assertIsNotNone(rate)
        self.assertGreater(rate, 0.5)  # Germany's AI: 37 to 129 divisions from 28 factories in 3.7 peace years
        self.assertLess(rate, 2.0)


@unittest.skipUnless(HAVE_GAME, f"game files not found at {GAME_DIR}")
class Decisive(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.game = lint_plan.load_game(GAME_DIR)
        plan = germany_at_war()
        plan["force_design"] = {"wars": [{"id": "war_pol", "enemies": ["POL"], "kind": "offensive", "axes": 3}]}
        plan["designs"] = [{"id": "d_tank", "kind": "tank", "base": "medium_tank_chassis_1"}]
        plan["production"] = list(plan.get("production") or []) + [
            {"equipment": "medium_tank_chassis_1", "design": "d_tank", "factories": 2, "from": "1937.1.1"}]
        cls.out = lint_plan.lint(plan, cls.game)

    def rules(self, rule):
        return [f for f in self.out["findings"] if f["rule"] == rule and f["severity"] == "error"]

    def test_a_war_without_duration_or_reasons_is_not_decisive(self):
        f = self.rules("force.decisive")
        self.assertTrue(f)
        self.assertIn("force_design.wars[].decisive", f[0]["message"])
        row = self.out["facts"]["force"]["decisive"][0]
        self.assertEqual(row["war"], "war_pol")
        self.assertTrue(row["problems"])

    def test_the_enemy_is_grown_to_the_war(self):
        need = next(n for n in self.out["facts"]["force"]["needs"] if n["war"] == "war_pol")
        self.assertGreater(need["front"]["divisions_est"], need["front"]["divisions"])
        self.assertIn("peace rate", need["front"]["growth_why"])

    def test_a_design_without_modules_is_costed_on_the_chassis(self):
        f = self.rules("force.design_cost")
        self.assertTrue(any("d_tank" in x["where"] for x in f), f)


@unittest.skipUnless(HAVE_GAME, f"game files not found at {GAME_DIR}")
class GoalWars(unittest.TestCase):
    def test_every_country_the_goal_defeats_has_its_war(self):
        game = lint_plan.load_game(GAME_DIR)
        plan = germany_at_war()
        plan["force_design"] = {"wars": [{"id": "war_pol", "enemies": ["POL"], "kind": "offensive", "axes": 3}]}
        plan["goal"]["predicates"] = ["capitulated:POL", "vp_share:SOV <= 0.2", "vp_share:FRA <= 0.9",
                                      "capitulated:ISR"]
        out = lint_plan.lint(plan, game)
        msgs = [f["message"] for f in out["findings"] if f["rule"] == "force.required"]
        self.assertTrue(any("defeats SOV" in m for m in msgs), msgs)
        self.assertFalse(any("defeats POL" in m or "defeats FRA" in m for m in msgs), msgs)
        self.assertFalse(any("defeats ISR" in m for m in msgs), msgs)  # holds no state at the start


@unittest.skipUnless(HAVE_GAME, f"game files not found at {GAME_DIR}")
class Duration(unittest.TestCase):
    def test_the_label_follows_the_plans_own_timetable(self):
        game = lint_plan.load_game(GAME_DIR)
        plan = germany_at_war()
        plan["force_design"] = {"wars": [{"id": "war_pol", "enemies": ["POL"], "kind": "offensive", "axes": 3,
                                          "duration_days": 30, "decisive": "three axes, air superiority"}]}
        plan["checkpoints"] = list(plan.get("checkpoints") or []) + [
            {"id": "cp_pol", "by": "1940.6.1", "must": ["capitulated:POL"]}]
        out = lint_plan.lint(plan, game)
        row = next(r for r in out["facts"]["force"]["decisive"] if r["war"] == "war_pol")
        self.assertGreater(row["planned_days"], 200)
        self.assertTrue(any("duration_days 30" in p for p in row["problems"]), row["problems"])


if __name__ == "__main__":
    unittest.main()
