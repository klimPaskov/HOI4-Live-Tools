"""lint_operations.py and the `operations` plan key: a large land war as one army-group front, one group offensive
whose target line spans the enemy country, the armour in its own army, a hold on every other hostile border and the
war-day placement by template. The schema part runs anywhere; the lint part reads the installed game and is skipped
without it."""

import copy
import json
import os
import sys
import unittest

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.join(ROOT, "scripts"))
sys.path.insert(0, os.path.join(ROOT, "tests"))

from hoi4facts import lint_plan, schema_check  # noqa: E402
from hoi4facts.plan_keys import SCHEMA_PATH  # noqa: E402
from test_force_design import GAME_DIR, HAVE_GAME, germany_at_war  # noqa: E402

INF, MOT, PZ = "Infanterie-Division", "Infanterie-Division (mot.)", "Panzer-Division"
VISTULA = [85, 807, 98, 10, 92, 91, 88, 89]   # Danzig to the Romanian border through Warsaw (history/states)


def operation():
    return {
        "id": "op_poland", "war": "war_pol", "enemies": ["POL"],
        "prepare_from": "date >= 1939.7.15", "execute_on": "at_war_with:POL", "execution": "balanced",
        "group": "ag_ost",
        "front": {"enemy": "POL", "own_states": [63, 68, 66, 67]},
        "offensive": {"axis": [67, 86, 87, 10], "target_line": list(VISTULA)},
        "formations": [
            {"army": "Armee Ost", "role": "line", "divisions": {INF: 24}},
            {"army": "Armee Sued", "role": "line", "divisions": {INF: 24}},
            {"army": "Armee Panzer", "role": "mobile", "divisions": {PZ: 3, MOT: 1},
             "front": {"enemy": "POL", "own_states": [67]},
             "offensive": {"axis": [67, 87, 10], "target_line": [10]}},
            {"army": "Armee Nord", "role": "wing", "divisions": {INF: 24},
             "front": {"enemy": "POL", "own_states": [5, 763]},
             "offensive": {"axis": [5, 98], "target_line": [98, 97, 95]}}],
        "holds": [{"id": "hold_west", "facing": ["FRA"], "group": "ag_west",
                   "armies": [{"army": "Armee West", "divisions": {INF: 24}},
                              {"army": "Armee Rhein", "divisions": {INF: 5}}],
                   "states": [978, 42], "min_per_province": 3}],
        "air": {"regions": [38, 39], "superiority": True, "cas": True},
        "supply": {"motorization": 1, "floor": 35},
        "phase_lines": [{"id": "pl2", "when": "controls_state:10", "target_line": [96, 95, 94, 93, 91, 89]}],
        "on_capitulation": {"next": None},
        "success": {"capitulated_by": "1939.10.20", "casualty_ratio": 3.0}}


def poland(with_operation=True):
    plan = germany_at_war()
    plan["diplomacy"][0]["joiners_ok"] = ["FRA", "ENG"]
    names = [("Armee Ost", 66), ("Armee Sued", 67), ("Armee Panzer", 67), ("Armee Nord", 5), ("Armee West", 978),
             ("Armee Rhein", 42)]
    plan["army_layout"] = [{"id": f"L{i}", "name_hint": n, "state": s, "role": "strike", "count": 3,
                            "templates": [PZ, MOT] if n == "Armee Panzer" else [INF], "general": "Wilhelm List"}
                           for i, (n, s) in enumerate(names)]
    plan["army_group"] = [{"id": "ag_ost", "armies": ["Armee Ost", "Armee Sued", "Armee Panzer", "Armee Nord"],
                           "marshal": "Gerd von Rundstedt"},
                          {"id": "ag_west", "armies": ["Armee West", "Armee Rhein"]}]
    plan["force_design"] = {"wars": [{"id": "war_pol", "enemies": ["POL"], "start": "1939.09.01", "kind": "offensive",
                                      "attack_ratio": 0.9}]}
    plan["air_plan"] = [
        {"id": "f", "airframe": "small_plane_airframe_1", "count": 100, "base_state": 66, "region": 38,
         "mission": "air_superiority", "when": "date >= 1939.8.25"},
        {"id": "c", "airframe": "small_plane_cas_airframe_1", "count": 50, "base_state": 66, "region": 38,
         "mission": "cas", "when": "date >= 1939.8.25"}]
    if with_operation:
        plan["operations"] = [operation()]
    return plan


class Schema(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        with open(SCHEMA_PATH, encoding="utf-8") as fh:
            cls.schema = json.load(fh)

    def test_the_worked_operation_is_valid(self):
        self.assertEqual(schema_check.validate({"operations": [operation()]}, self.schema), [])

    def test_the_parts_an_operation_cannot_lack(self):
        for key in ("front", "offensive", "formations", "holds", "air", "supply", "on_capitulation", "success",
                    "prepare_from", "execute_on", "execution", "group", "enemies", "war"):
            op = operation()
            del op[key]
            self.assertNotEqual(schema_check.validate({"operations": [op]}, self.schema), [], key)

    def test_unknown_keys_and_roles_are_refused(self):
        op = operation()
        op["spearhead"] = [1, 2]
        self.assertNotEqual(schema_check.validate({"operations": [op]}, self.schema), [])
        op = operation()
        op["formations"][0]["role"] = "reserve"
        self.assertNotEqual(schema_check.validate({"operations": [op]}, self.schema), [])
        op = operation()
        op["formations"][0]["divisions"] = {INF: 0}
        self.assertNotEqual(schema_check.validate({"operations": [op]}, self.schema), [])


@unittest.skipUnless(HAVE_GAME, f"game files not found at {GAME_DIR}")
class Poland(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.game = lint_plan.load_game(GAME_DIR)
        cls.good = lint_plan.lint(poland(), cls.game)

    def errors(self, plan, rule):
        out = plan if "findings" in plan else lint_plan.lint(plan, self.game)
        return [f["message"] for f in out["findings"] if f["rule"] == rule and f["severity"] == "error"]

    def changed(self, edit):
        plan = poland()
        edit(plan["operations"][0])
        return plan

    def test_the_worked_operation_passes_every_war_rule(self):
        bad = [f for f in self.good["findings"] if f["rule"].startswith("war.") and f["severity"] == "error"]
        self.assertEqual(bad, [])
        fact = self.good["facts"]["force"]["operations"][0]
        self.assertEqual(fact["attack"], 76)
        self.assertGreaterEqual(fact["surrender"]["POL"]["first_line"], fact["surrender"]["POL"]["vp_needed"])
        self.assertEqual(fact["holds"][0]["border_provinces"], 8)

    def test_a_large_land_war_needs_an_operation(self):
        found = self.errors(poland(False), "war.operation_required")
        self.assertTrue(found and "war_pol" in found[0], found)

    def test_the_target_line_spans_the_country(self):
        def short(op):
            op["offensive"]["target_line"] = [10]
            op["formations"][3]["offensive"]["target_line"] = [98]
            del op["phase_lines"]
        found = self.errors(self.changed(short), "war.operation_line")
        self.assertTrue(any("does not span" in m for m in found), found)

    def test_the_lines_reach_the_surrender_threshold(self):
        def shallow(op):  # the western border states only: a real line, far short of Warsaw and Krakow
            op["offensive"]["target_line"] = [807, 86, 87, 762]
            op["offensive"]["axis"] = [67, 86]
            op["formations"][2]["offensive"] = {"axis": [67, 86], "target_line": [86]}
            del op["phase_lines"]
        found = self.errors(self.changed(shallow), "war.operation_line")
        self.assertTrue(any("capitulates at" in m for m in found), found)

    def test_lines_stay_in_the_enemies_land_and_form_a_chain(self):
        found = self.errors(self.changed(lambda op: op["offensive"]["target_line"].append(34)), "war.operation_line")
        self.assertTrue(any("34 (BEL)" in m for m in found), found)
        found = self.errors(self.changed(lambda op: op["offensive"].update(target_line=[85, 10, 89])),
                            "war.operation_line")
        self.assertTrue(any("not a chain" in m for m in found), found)

    def test_an_enemy_of_the_operation_is_at_war_that_day(self):
        found = self.errors(self.changed(lambda op: op["enemies"].append("LIT")), "war.objective_not_at_war")
        self.assertTrue(any("LIT" in m for m in found), found)

    def test_every_other_hostile_border_has_a_hold_with_enough_divisions(self):
        found = self.errors(self.changed(lambda op: op.update(holds=[])), "war.hold_sector")
        self.assertTrue(any("in no hold" in m and "978" in m for m in found), found)

        def thin(op):
            op["holds"][0]["armies"] = [{"army": "Armee West", "divisions": {INF: 8}}]
        found = self.errors(self.changed(thin), "war.hold_sector")
        self.assertTrue(any("1.0 a province" in m for m in found), found)

    def test_the_placement_is_the_forecast_and_the_armour_is_mobile(self):
        def tanks_in_the_line(op):
            op["formations"][0]["divisions"][PZ] = 1
            op["formations"][2]["divisions"][PZ] = 2
        found = self.errors(self.changed(tanks_in_the_line), "war.attack_force")
        self.assertTrue(any("outside the mobile formations (Armee Ost)" in m for m in found), found)
        found = self.errors(self.changed(lambda op: op["formations"][2]["divisions"].update({PZ: 9})),
                            "war.attack_force")
        self.assertTrue(any("more divisions than the forecast" in m for m in found), found)
        found = self.errors(self.changed(lambda op: op["formations"][3]["divisions"].update({INF: 6})),
                            "war.attack_force")
        self.assertTrue(any("attacks with 58 divisions" in m for m in found), found)

    def test_plans_are_drawn_a_month_before_and_executed_on_the_war_day(self):
        found = self.errors(self.changed(lambda op: op.update(prepare_from="date >= 1939.8.20")),
                            "war.operation_timing")
        self.assertTrue(any("prepare_from" in m for m in found), found)
        found = self.errors(self.changed(lambda op: op.update(execute_on="date >= 1939.10.1")), "war.operation_timing")
        self.assertTrue(any("execute_on" in m for m in found), found)

    def test_roles_groups_and_armies_are_consistent(self):
        def line_with_orders(op):
            op["formations"][0]["front"] = {"enemy": "POL", "own_states": [66]}
        found = self.errors(self.changed(line_with_orders), "war.operation_refs")
        self.assertTrue(any("ignores the group's" in m for m in found), found)
        found = self.errors(self.changed(lambda op: op["holds"][0].update(group="ag_ost")), "war.operation_refs")
        self.assertTrue(any("own group" in m for m in found), found)
        found = self.errors(self.changed(lambda op: op["formations"][0].update(army="Armee Niemand")),
                            "war.operation_refs")
        self.assertTrue(any("Armee Niemand" in m for m in found), found)
        found = self.errors(self.changed(lambda op: op["formations"][0]["divisions"].update({INF: 30})),
                            "war.operation_refs")
        self.assertTrue(any("over the 24" in m for m in found), found)

    def test_the_air_flies_over_the_operation(self):
        found = self.errors(self.changed(lambda op: op["air"].update(regions=[38])), "war.operation_support")
        self.assertTrue(any("[39]" in m for m in found), found)
        plan = poland()
        plan["air_plan"] = plan["air_plan"][:1]
        found = self.errors(plan, "war.operation_support")
        self.assertTrue(any("close_air_support" in m for m in found), found)

    def test_the_layout_rules_leave_an_operations_armies_alone(self):
        plan = copy.deepcopy(poland())
        plan["vp_targets"] = [{"province": 11519, "army": "Armee Panzer", "by": "1940.5.20"}]  # Belgian, a later war
        self.assertFalse(self.errors(plan, "war.mobile_axis"))
        self.assertFalse([m for m in self.errors(plan, "war.objective_not_at_war") if "Armee Panzer" in m])


if __name__ == "__main__":
    unittest.main()
