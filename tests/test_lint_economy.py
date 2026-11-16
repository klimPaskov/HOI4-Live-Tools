"""scripts/hoi4facts/lint_economy.py: production over all lines sized to the need, construction capacity, the
AI baseline, consumer goods with the history's dynamic modifiers. Reads the installed game; skipped without it."""

import copy
import json
import os
import sys
import unittest

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.join(ROOT, "scripts"))

from hoi4facts import lint_economy, lint_plan  # noqa: E402

PLANS = os.path.join(ROOT, "skills", "hoi4-campaign-playbook", "plans")
GAME_DIR = os.environ.get("HOI4_DIR", lint_plan.DEFAULT_GAME_DIR)
HAVE_GAME = os.path.isfile(os.path.join(GAME_DIR, "map", "provinces.bmp"))


def base_plan():
    with open(os.path.join(PLANS, "germany-fair-1936-1938.plan.json"), encoding="utf-8") as fh:
        plan = copy.deepcopy(json.load(fh))
    plan["_meta"] = {"version": 1, "source": "strategist"}
    plan["goal"] = {"summary": "test", "deadline": "1939.9.1"}
    plan["_validator"]["stop_date"] = "1939.9.1"
    plan.pop("stop_on_date", None)
    return plan


class Baseline(unittest.TestCase):
    def test_the_germany_baseline_is_in_the_repo(self):
        b = lint_economy.load_baseline("GER", "1936.1.1")
        self.assertIsNotNone(b)
        row = next(r for r in b["rows"] if r["date"] == "1939.09.01")
        self.assertEqual((row["civilian_factories"], row["military_factories"], row["divisions"]), (104, 67, 129))
        self.assertIsNone(lint_economy.load_baseline("ZZZ", "1936.1.1"))


@unittest.skipUnless(HAVE_GAME, f"game files not found at {GAME_DIR}")
class Germany(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.game = lint_plan.load_game(GAME_DIR)
        # the hand plan appends lines: no entry for the start lines, new air tiers beside the running ones
        cls.appended = lint_plan.lint(base_plan(), cls.game)
        plan = base_plan()
        plan["production"] = [
            {"equipment": "infantry_equipment_1", "factories": 60, "from": "1936.01.01"},
            {"equipment": "infantry_equipment_1", "factories": 5, "from": "1937.01.01"},
            {"equipment": "artillery_equipment_1", "factories": 1, "from": "1936.01.01"},
        ]
        plan["training"] = [{"template": "Infanterie-Division", "queue": 8, "batch": 2, "cap": 120}]
        plan["construction"] = [{"building": "industrial_complex", "count": 2, "states": [64]},
                                {"building": "arms_factory", "count": 2, "states": [65]}]
        plan["construction_fallback_any_own_state"] = False
        plan["horizons"] = [{"id": "h1", "until": "1939.9.1", "detail": True,
                             "targets": {"civilian_factories": 80, "military_factories": 60, "divisions": 60}}]
        cls.lopsided = lint_plan.lint(plan, cls.game)

    def rules(self, out, rule):
        return [f for f in out["findings"] if f["rule"] == rule]

    def test_start_lines_need_a_target(self):
        f = self.rules(self.appended, "production.start_lines")
        self.assertTrue(any("infantry_equipment_1" in x["message"] and x["severity"] == "error" for x in f), f)
        self.assertFalse(any("infantry_equipment_1" in x["message"] for x in self.rules(self.lopsided,
                                                                                         "production.start_lines")))

    def test_a_new_tier_beside_a_running_line(self):
        f = self.rules(self.appended, "production.duplicate_line")
        self.assertTrue(any("small_plane_airframe_2" in x["message"] for x in f), f)

    def test_two_adds_of_one_equipment(self):
        f = self.rules(self.lopsided, "production.duplicate_line")
        self.assertTrue(any("infantry_equipment_1 has 2 add entries" in x["message"] for x in f), f)

    def test_priority_over_every_line(self):
        f = self.rules(self.appended, "production.priority_all")
        self.assertTrue(f)
        self.assertIn("light_tank_chassis_2", f[0]["message"])

    def test_one_family_far_above_its_need(self):
        f = self.rules(self.lopsided, "production.need_balance") + self.rules(self.lopsided, "production.missing_line")
        self.assertTrue(f, self.lopsided["facts"].get("production_need"))
        self.assertTrue(self.rules(self.lopsided, "production.parked_request"))

    def test_construction_slots_for_the_targets(self):
        f = self.rules(self.lopsided, "construction.capacity")
        self.assertTrue(f)
        self.assertEqual(f[0]["severity"], "error")

    def test_below_the_ai(self):
        f = self.rules(self.lopsided, "plan.ai_baseline")
        self.assertTrue(f)
        self.assertIn("129", f[0]["message"])

    def test_consumer_goods_count_the_history_modifiers(self):
        e = self.appended["facts"]["economy"]
        self.assertGreater(e["start"]["consumer_goods"], 0.25)  # the law's 0.25 times the MEFO factor
        self.assertIn("GER_industrial_consumer_goods_factor", e["consumer_goods_changed_by_script"])


if __name__ == "__main__":
    unittest.main()
