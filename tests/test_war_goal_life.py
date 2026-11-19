"""A declaration gated on a focus whose war goal expires (war.declaration_gate), template rows priced at what the game
charges (templates.xp_cost), and dated production floors read as one line each (production_floors: true).

Germany's Danzig or War creates a take_state goal on Poland (germany.86 -> germany.88) that common/wargoals lets live
60 days: a focus finished months before the war day leaves no war goal on it. A template create costs 5 army XP a
battalion, 10 a company and 25 a column after the first on this build, so a create priced at 40 waits for ever.
Reads the installed game; skipped without it."""

import os
import sys
import unittest

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.join(ROOT, "scripts"))
sys.path.insert(0, os.path.join(ROOT, "tests"))

from hoi4facts import lint_plan  # noqa: E402
from test_force_design import GAME_DIR, HAVE_GAME, germany_at_war  # noqa: E402

DANZIG = "GER_danzig_or_war"


class Prices(unittest.TestCase):
    def test_a_create_costs_battalions_companies_and_columns(self):
        cols = [["infantry"] * 3] * 3 + [["artillery_brigade"] * 2]
        self.assertEqual(lint_plan.template_create_cost(cols, 3), 11 * 5 + 3 * 10 + 3 * 25)

    def test_an_edit_floor_counts_changed_slots(self):
        mot = [["motorized"] * 3] * 3
        self.assertEqual(lint_plan.template_edit_floor(mot, mot[:2] + [["medium_armor"] * 3]), 15)
        self.assertEqual(lint_plan.template_edit_floor(mot, mot + [["medium_armor"] * 3]), 15)
        self.assertEqual(lint_plan.template_edit_floor(mot, mot), 0)


def _at(day):
    return lint_plan.derive.fmt(lint_plan.derive.date_of(day))


@unittest.skipUnless(HAVE_GAME, f"game files not found at {GAME_DIR}")
class WarGoalLife(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.game = lint_plan.load_game(GAME_DIR)
        out = lint_plan.lint(germany_at_war(), cls.game)
        ends = {r["focus"]: r["end"] for r in out["facts"]["focus"]["timeline"]}
        cls.done = lint_plan.derive.day(ends[DANZIG])

    def gate(self, war_day):
        plan = germany_at_war()
        plan["diplomacy"][0]["when"] = {"all": [f"date >= {_at(war_day)}", f"focus_completed:{DANZIG}"]}
        out = lint_plan.lint(plan, self.game)
        return [f["message"] for f in out["findings"] if f["rule"] == "war.declaration_gate"
                and f["severity"] == "error"]

    def test_inside_the_goals_life_is_clean(self):
        self.assertEqual(self.gate(self.done + 30), [])

    def test_a_goal_that_lapses_before_the_war_day(self):
        found = self.gate(self.done + 120)
        self.assertTrue(found)
        self.assertIn("take_state", found[0])
        self.assertIn("lapses", found[0])

    def test_a_focus_too_close_to_the_war_day(self):
        found = self.gate(self.done + 5)
        self.assertTrue(found)
        self.assertIn("expires 60 days", found[0])


@unittest.skipUnless(HAVE_GAME, f"game files not found at {GAME_DIR}")
class TemplatePriceAndFloors(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        game = lint_plan.load_game(GAME_DIR)
        plan = germany_at_war()
        plan["templates"] = [
            {"id": "t_new", "name": "Angriff", "regiments": [["infantry"] * 3] * 3 + [["artillery_brigade"] * 2],
             "support": ["engineer", "artillery", "recon"], "xp": 40},
            {"id": "t_art", "name": "Infanterie-Division", "base": "Infanterie-Division",
             "regiments": [["infantry"] * 3] * 3 + [["artillery_brigade"] * 2], "support": ["engineer", "artillery"],
             "xp": 30, "upgrade_fielded": True},
            {"id": "t_mot", "name": "Infanterie-Division (mot.)", "base": "Infanterie-Division (mot.)",
             "regiments": [["motorized"] * 3] * 2 + [["medium_armor"] * 3],
             "support": ["mot_recon", "engineer", "artillery"], "xp": 5, "upgrade_fielded": True}]
        plan["xp_budget"] = {"army": [{"id": f"x_{t['id']}", "cost": t["xp"], "purpose": "template",
                                       "ref": f"template:{t['id']}", "when": "date >= 1937.1.1"}
                                      for t in plan["templates"]]}
        plan["production_floors"] = True
        plan["production"] = [r for r in plan.get("production") or [] if r.get("equipment") != "infantry_equipment_1"]
        plan["production"] += [{"equipment": "infantry_equipment_1", "factories": 10, "from": "1936.1.1"},
                               {"equipment": "infantry_equipment_1", "factories": 6, "from": "1938.1.1"}]
        cls.out = lint_plan.lint(plan, game)

    def found(self, rule, where=""):
        return [f for f in self.out["findings"] if f["rule"] == rule and f["severity"] == "error"
                and where in f["where"]]

    def test_a_create_priced_under_the_charge(self):
        f = self.found("templates.xp_cost", "t_new")
        self.assertTrue(f)
        self.assertIn("charges 160", f[0]["message"])

    def test_an_edit_at_its_price_is_clean(self):
        self.assertFalse([f for f in self.found("templates.xp_cost", "t_art") if "edits" in f["message"]])

    def test_an_edit_under_the_tools_floor(self):
        f = self.found("templates.xp_cost", "t_mot")
        self.assertTrue(f)
        self.assertIn("refuses below 15", f[0]["message"])

    def test_dated_floors_are_one_line(self):
        self.assertFalse([f for f in self.found("production.duplicate_line")
                          if "infantry_equipment_1" in f["message"]])


if __name__ == "__main__":
    unittest.main()
