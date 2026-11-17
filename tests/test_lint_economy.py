"""scripts/hoi4facts/lint_economy.py and economy.py: production over all lines sized to the need, construction
capacity, the AI baseline as a floor, the forecast of the plan's own levers, consumer goods with the history's
dynamic modifiers. Reads the installed game; skipped without it (the reward reader runs without it)."""

import copy
import json
import os
import sys
import unittest

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.join(ROOT, "scripts"))

from hoi4facts import economy, lint_economy, lint_plan  # noqa: E402

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


class Rewards(unittest.TestCase):
    """economy._reward_events without a game: what a focus reward gives."""

    def test_if_takes_its_first_branch_and_random_states_multiply(self):
        block = [("every_core_state", [("limit", [("is_controlled_by", "ROOT")]), ("random_select_amount", "2"),
                                       ("add_extra_state_shared_building_slots", "2"),
                                       ("add_building_construction", [("type", "arms_factory"), ("level", "2")])]),
                 ("if", [("limit", [("has_dlc", "x")]), ("59", [("add_building_construction",
                                                                  [("type", "industrial_complex"), ("level", "3")])]),
                         ("else", [("add_building_construction", [("type", "industrial_complex"), ("level", "9")])])]),
                 ("add_ideas", "autarkic_economy"), ("annex_country", [("target", "AUS")])]
        ev = economy._reward_events(block, "GER")
        self.assertIn(("build", ("arms_factory", 4, "random")), ev)
        self.assertIn(("slots", (4, "random")), ev)
        self.assertIn(("build", ("industrial_complex", 3, 59)), ev)
        self.assertNotIn(("build", ("industrial_complex", 9, None)), ev)
        self.assertIn(("idea+", "autarkic_economy"), ev)
        self.assertIn(("annex", "AUS"), ev)


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
        cls.lopsided_plan = plan
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

    def test_below_the_ai_is_an_error(self):
        f = self.rules(self.lopsided, "plan.ai_baseline")
        errors = [x for x in f if x["severity"] == "error"]
        self.assertTrue(errors, f)
        self.assertTrue(any("civilian factories 80 by 1939.09.01 against the AI's 104" in x["message"]
                            for x in errors), errors)
        self.assertTrue(any("129" in x["message"] and x["severity"] == "warning" for x in f), f)

    def test_a_written_reason_does_not_lift_the_factory_floor(self):
        plan = copy.deepcopy(self.lopsided_plan)
        plan["goal"]["ai_baseline_why"] = "the safe states allow no more"
        out = lint_plan.lint(plan, self.game)
        errors = [x for x in self.rules(out, "plan.ai_baseline") if x["severity"] == "error"]
        self.assertTrue(errors)
        # the divisions warning is answered by the reason
        self.assertFalse([x for x in self.rules(out, "plan.ai_baseline") if x["severity"] == "warning"])

    def test_only_a_confirmed_constraint_lifts_it(self):
        plan = copy.deepcopy(self.lopsided_plan)
        plan["goal"]["ai_baseline_constraint"] = [{"kind": "no_free_slots", "date": "1939.9.1"},
                                                  {"kind": "law_unavailable", "law": "war_economy",
                                                   "date": "1936.1.1"}]
        out = lint_plan.lint(plan, self.game)
        cons = out["facts"]["ai_baseline"]["constraints"]
        self.assertEqual(len(cons), 2)
        self.assertFalse(cons[0]["confirmed"], cons)  # Germany has free slots in 1939
        errors = [x for x in self.rules(out, "plan.ai_baseline") if x["severity"] == "error"]
        self.assertTrue(any("cannot confirm" in x["message"] for x in errors), errors)

    def test_the_forecast_counts_the_plan_levers(self):
        fc = self.lopsided["facts"]["economy"]["forecast"]
        self.assertTrue(fc["rows"])
        first, last = fc["rows"][0], fc["rows"][-1]
        self.assertEqual((first["civ"], first["mil"]), (35, 28))
        self.assertGreater(last["civ"] + last["mil"], first["civ"] + first["mil"])
        self.assertEqual(fc["risk_states"], [])  # Germany attacks weaker neighbours: no border state barred
        self.assertIsInstance(self.lopsided["facts"]["economy"]["levers"], list)
        # the forecast is held to the AI only up to the first war day (it leaves out conquest)
        until = self.lopsided["facts"]["ai_baseline"]["forecast_checked_until"]
        for m in self.lopsided["facts"]["ai_baseline"]["forecast_below"] if until else []:
            self.assertLessEqual(lint_plan.derive.day(m.rsplit(" on ", 1)[1].split()[0]), lint_plan.derive.day(until))

    def test_levers_leave_out_advisors_the_government_cannot_hire(self):
        # Hans Luther's available block is NOT = { has_government = fascism }, and Germany starts fascist
        ids = {x["id"] for x in self.lopsided["facts"]["economy"]["levers"]}
        self.assertNotIn("GER_hans_luther", ids)

    def test_military_jobs_from_the_first_day(self):
        f = self.rules(self.lopsided, "construction.switch")
        sw = self.lopsided["facts"]["economy"]["forecast"]["switch"]
        if lint_plan.derive.day(sw) - lint_plan.derive.day("1936.1.1") >= lint_economy.SWITCH_GRACE_DAYS:
            self.assertTrue(f)
        else:
            self.assertFalse(f)

    def test_consumer_goods_count_the_history_modifiers(self):
        e = self.appended["facts"]["economy"]
        self.assertGreater(e["start"]["consumer_goods"], 0.25)  # the law's 0.25 times the MEFO factor
        self.assertIn("GER_industrial_consumer_goods_factor", e["consumer_goods_changed_by_script"])


if __name__ == "__main__":
    unittest.main()
