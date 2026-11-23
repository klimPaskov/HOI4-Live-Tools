"""The supply, trains, radar, special-project and formations lint (lint_support.py) on plain plans, the schema
accepting the keys it reads, and the support facts on the installed game when it is there."""
import json
import os
import sys
import unittest

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.join(ROOT, "scripts"))

from hoi4facts import lint_plan, lint_support, schema_check  # noqa: E402

GAME_DIR = os.environ.get("HOI4_DIR", lint_plan.DEFAULT_GAME_DIR)
HAVE_GAME = os.path.isfile(os.path.join(GAME_DIR, "map", "provinces.bmp"))
SCHEMA = os.path.join(ROOT, "skills", "hoi4-campaign-playbook", "plan.schema.json")

GATES = {"radar_station": [{"tech": "radio_detection", "level": 1}, {"tech": "cavity_magnatron", "level": 2}],
         "anti_air_building": [{"tech": "interwar_antiair", "level": 5}]}


class RequiresTech(unittest.TestCase):
    def plan(self, **extra):
        item = {"id": "cp_radar_56", "building": "radar_station", "state": 56, "when": "1940.3.1"}
        item.update(extra)
        return {"construction_plan": [item, {"id": "aa", "building": "anti_air_building", "state": 64}]}

    def test_a_dated_radar_item_without_its_technology_is_named(self):
        out = lint_support.requires_findings(self.plan(), GATES, {"radio"})
        self.assertEqual(len(out), 1)
        self.assertIn("radio_detection", out[0][2])
        self.assertIn("cp_radar_56", out[0][1])

    def test_requires_or_a_tech_when_or_a_held_unlock_clears_it(self):
        self.assertEqual(lint_support.requires_findings(self.plan(requires="radio_detection"), GATES, set()), [])
        self.assertEqual(lint_support.requires_findings(
            self.plan(when="tech_researched:radio_detection"), GATES, set()), [])
        # GER 1939: cavity_magnatron held (level 2) permits the building
        self.assertEqual(lint_support.requires_findings(self.plan(), GATES, {"cavity_magnatron"}), [])


class Facilities(unittest.TestCase):
    PROJECTS = [{"id": "sp_air_radar", "specialization": "specialization_air"},
                {"id": "sp_land_flamethrower_tank", "specialization": "specialization_land"}]

    def test_a_priority_project_with_no_facility_of_its_kind_is_named(self):
        plan = {"special_projects": {"priority": ["sp_air_radar", "sp_land_flamethrower_tank"]}}
        out = lint_support.facility_findings(plan, self.PROJECTS, [{"building": "land_facility", "state": 64}])
        self.assertEqual([w for _, w, _ in out], ["special_projects.priority[0]"])
        plan["construction_plan"] = [{"id": "f", "building": "air_facility", "state": 60}]
        self.assertEqual(lint_support.facility_findings(plan, self.PROJECTS, [{"building": "land_facility"}]), [])


class WarSupply(unittest.TestCase):
    WAR = {"war": {"enemies": ["POL"]},
           "air_plan": [{"id": "cas", "airframe": "small_plane_cas_airframe_1", "mission": "cas"}]}

    def test_cas_wings_without_a_logistics_strike_entry_are_noted(self):
        self.assertEqual(len(lint_support.logistics_findings(self.WAR)), 1)
        plan = json.loads(json.dumps(self.WAR))
        plan["air_plan"].append({"id": "ls", "airframe": "small_plane_cas_airframe_1", "mission": "logistics_strike"})
        self.assertEqual(lint_support.logistics_findings(plan), [])
        self.assertEqual(lint_support.logistics_findings({"air_plan": self.WAR["air_plan"]}), [])

    def test_a_train_line_is_wanted_from_the_start_and_a_target_to_grow_it(self):
        self.assertEqual([f[0] for f in lint_support.train_findings(self.WAR)], ["warning", "info"])
        self.assertEqual([f[0] for f in lint_support.train_findings({})], ["warning"])
        line = dict(self.WAR, logistics={"trains": {"factories": 1}})
        self.assertEqual([f[1] for f in lint_support.train_findings(line)], ["stockpile_targets"])
        self.assertEqual(lint_support.train_findings(dict(line, stockpile_targets={"train_equipment": 60})), [])
        self.assertEqual([f[0] for f in lint_support.train_findings(
            dict(self.WAR, production=[{"equipment": "train_equipment_1", "factories": 2}]))], ["info"])

    def test_operation_motorization_needs_the_hub_rule(self):
        plan = {"operations": [{"id": "op_e", "supply": {"motorization": 2, "floor": 35}}]}
        self.assertEqual([f[1] for f in lint_support.hub_findings(plan)], ["operations[0].supply.motorization"])
        plan["logistics"] = {"hubs": True}
        self.assertEqual([f[1] for f in lint_support.hub_findings(plan)], ["logistics.hubs"])
        plan["production"] = [{"equipment": "motorized_equipment_1", "factories": 4}]
        self.assertEqual(lint_support.hub_findings(plan), [])


class Formations(unittest.TestCase):
    def test_a_keep_name_that_is_no_template_is_named(self):
        plan = {"formations": {"act": True, "keep": ["Gebirgs-Brigade", "Gebirgsjaeger"]},
                "training": [{"template": "Infanterie-Division"}]}
        out = lint_support.keep_findings(plan, {"Gebirgs-Brigade", "Infanterie-Division"})
        self.assertEqual([w for _, w, _ in out], ["formations.keep[1]"])
        self.assertEqual(lint_support.keep_findings(plan, None), [])


class Schema(unittest.TestCase):
    def test_the_schema_takes_requires_a_predicate_when_and_formations(self):
        plan = {"construction_plan": [
                    {"id": "r", "building": "radar_station", "state": 56, "requires": "radio_detection",
                     "when": "tech_researched:radio_detection"},
                    {"id": "r2", "building": "radar_station", "state": 51, "requires": ["radio_detection"],
                     "when": "1940.3.1"}],
                "formations": {"act": True, "keep": ["Gebirgs-Brigade"], "why": "Norway"}}
        errors = schema_check.validate(plan, json.load(open(SCHEMA)))
        self.assertEqual([e for e in errors if "construction_plan" in str(e) or "formations" in str(e)], [])


@unittest.skipUnless(HAVE_GAME, "game files not installed")
class OnTheGame(unittest.TestCase):
    def test_radar_needs_a_technology_for_germany_in_1936(self):
        from hoi4facts import facts
        w = facts.World(facts.Game(GAME_DIR, []), "GER", "1936.1.1", [], "fair")
        data = facts.support_facts(w, "GER")
        self.assertTrue(data["buildings"]["radar_station"]["needs_tech"])
        self.assertFalse(data["buildings"]["anti_air_building"]["needs_tech"])
        self.assertIn("train_equipment_3", data["equipment_unlocks"])


if __name__ == "__main__":
    unittest.main()
