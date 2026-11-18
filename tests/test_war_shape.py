"""lint_war_shape.py: each war's shape on its first day (axes, objectives at war, hold sectors, the mobile arm's
axis). The faults of a Polish war that failed: a phase with posture attack and no axes, the armour army's first
objective in neutral Belgium, the western border held only by armies marked reserve. Reads the installed game;
skipped without it."""

import os
import sys
import unittest

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.join(ROOT, "scripts"))
sys.path.insert(0, os.path.join(ROOT, "tests"))

from hoi4facts import lint_plan  # noqa: E402
from test_force_design import GAME_DIR, HAVE_GAME, germany_at_war  # noqa: E402

POLISH_AXIS = [86, 87, 10]   # Poznan, Lodz, Warszawa (history/states)
BELGIAN_VP = 11519           # a victory point in Belgian state 34
WESTWALL = [978, 51, 42]     # own states on the French border


def poland(fixed):
    plan = germany_at_war()
    plan["diplomacy"][0]["joiners_ok"] = ["FRA", "ENG"]
    plan["army_layout"] = [
        {"id": "L_ost", "name_hint": "Armee Ost", "state": 66, "role": "strike", "count": 12,
         "templates": ["Infanterie-Division"], "axis": POLISH_AXIS},
        {"id": "L_pz", "name_hint": "Armee Panzer", "state": 64, "role": "strike", "count": 6,
         "templates": ["Panzer-Division", "Infanterie-Division (mot.)"],
         "axis": [86, 88] if fixed else POLISH_AXIS},
        {"id": "L_west", "name_hint": "Armee West", "state": 51, "role": "strike", "count": 8,
         "templates": ["Infanterie-Division"], "covers": WESTWALL}]
    war = {"posture": "attack", "axes": [POLISH_AXIS, [86, 88]] if fixed else []}
    if not fixed:
        war["reserve"] = ["Armee West"]
    plan["phases"] = [{"id": "P_pol", "enter": "at_war_with:POL", "overrides": {"war": war}}]
    first = {"province": BELGIAN_VP, "army": "Armee Panzer", "by": "1940.5.20"}
    plan["vp_targets"] = [first] if not fixed else []
    return plan


@unittest.skipUnless(HAVE_GAME, f"game files not found at {GAME_DIR}")
class Poland(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        game = lint_plan.load_game(GAME_DIR)
        cls.bad = lint_plan.lint(poland(False), game)
        cls.good = lint_plan.lint(poland(True), game)

    def rules(self, out, rule):
        return [f for f in out["findings"] if f["rule"] == rule and f["severity"] == "error"]

    def test_an_attack_phase_names_its_axes(self):
        f = self.rules(self.bad, "war.axes_missing")
        self.assertTrue(f)
        self.assertIn("posture attack with war.axes []", f[0]["message"])
        self.assertFalse(self.rules(self.good, "war.axes_missing"))

    def test_an_armys_first_objective_is_in_the_war(self):
        f = self.rules(self.bad, "war.objective_not_at_war")
        self.assertTrue(any(str(BELGIAN_VP) in x["where"] and "Armee Panzer" in x["message"] for x in f), f)
        self.assertFalse(self.rules(self.good, "war.objective_not_at_war"))

    def test_a_border_held_only_by_the_reserve_is_open(self):
        f = self.rules(self.bad, "war.hold_sector")
        self.assertTrue(any("FRA" in x["message"] and "Armee West" in x["message"] for x in f), f)
        self.assertFalse(self.rules(self.good, "war.hold_sector"))

    def test_the_mobile_arm_has_its_own_axis(self):
        f = self.rules(self.bad, "war.mobile_axis")
        self.assertTrue(any("Armee Panzer" in x["message"] and "Armee Ost" in x["message"] for x in f), f)
        self.assertFalse(self.rules(self.good, "war.mobile_axis"))


if __name__ == "__main__":
    unittest.main()
