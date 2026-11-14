"""The plan lint for template_cleanup (templates.cleanup) and special_projects keys (projects.keys): the
rule functions on plain plans, the schema accepting the keys, and one class on the installed game when it is
there."""
import copy
import json
import os
import sys
import unittest

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.join(ROOT, "scripts"))

from hoi4facts import lint_plan, schema_check  # noqa: E402

GAME_DIR = os.environ.get("HOI4_DIR", lint_plan.DEFAULT_GAME_DIR)
HAVE_GAME = os.path.isfile(os.path.join(GAME_DIR, "map", "provinces.bmp"))
SCHEMA = os.path.join(ROOT, "skills", "hoi4-campaign-playbook", "plan.schema.json")

PLAN = {
    "training": [{"template": "Infantry", "queue": 2, "deploy_state": 1}],
    "templates": [{"id": "line", "name": "Line", "base": "Old Line", "regiments": ["infantry"] * 7}],
    "army_layout": [{"role": "guard", "templates": ["Militia"], "state": 1}],
    "template_cleanup": [{"template": "Cavalry"}, {"template": "Garrison", "rename_to": "Police"}],
    "special_projects": {"priority": ["sp_land_flamethrower"], "recruit": {"max_cost": 50},
                         "stop": [{"project": "sp_air_radar", "when": "1937.1.1", "unassign": True}]},
}
START = {"Infantry", "Cavalry", "Garrison", "Old Line", "Militia", "Mountaineers"}


class Cleanup(unittest.TestCase):
    def test_unused_templates_pass(self):
        self.assertEqual(lint_plan.cleanup_findings(PLAN, START), [])

    def test_a_template_the_plan_trains_edits_or_lays_out_is_refused(self):
        for name, use in (("Infantry", "training[0]"), ("Old Line", "templates[0] base"), ("Line", "templates[0]"),
                          ("Militia", "army_layout[0]")):
            plan = copy.deepcopy(PLAN)
            plan["template_cleanup"] = [{"template": name}]
            found = lint_plan.cleanup_findings(plan, START)
            self.assertEqual([f[0] for f in found], ["error"], found)
            self.assertIn(use, found[0][2])

    def test_a_taken_name_and_an_unknown_template(self):
        plan = copy.deepcopy(PLAN)
        plan["template_cleanup"] = [{"template": "Garrison", "rename_to": "Mountaineers"}, {"template": "Nope"}]
        found = lint_plan.cleanup_findings(plan, START)
        self.assertEqual([(f[0], f[1]) for f in found],
                         [("error", "template_cleanup[0] 'Garrison'"), ("warning", "template_cleanup[1] 'Nope'")])
        # without a start OOB neither the taken name nor the unknown template is known
        self.assertEqual(lint_plan.cleanup_findings(plan, None), [])

    def test_a_single_training_object_counts(self):
        plan = {"training": {"template": "Infantry"}, "template_cleanup": [{"template": "Infantry"}]}
        found = lint_plan.cleanup_findings(plan, None)
        self.assertEqual(len(found), 1)
        self.assertIn("(training)", found[0][2])


class Projects(unittest.TestCase):
    KEYS = {"sp_land_flamethrower", "sp_air_radar"}

    def test_known_keys_pass(self):
        self.assertEqual(lint_plan.special_project_findings(PLAN, self.KEYS), [])

    def test_unknown_keys_and_a_stopped_priority(self):
        plan = copy.deepcopy(PLAN)
        plan["special_projects"]["priority"] = ["sp_nope", "sp_air_radar"]
        plan["special_projects"]["stop"].append({"project": "sp_gone"})
        found = lint_plan.special_project_findings(plan, self.KEYS)
        self.assertEqual([(f[0], f[1]) for f in found], [
            ("error", "special_projects.priority[0]"), ("error", "special_projects.stop[1]"),
            ("warning", "special_projects.stop[0]")])
        # unknown game files: only the overlap
        self.assertEqual(len(lint_plan.special_project_findings(plan, None)), 1)


class Schema(unittest.TestCase):
    def test_the_schema_takes_the_keys(self):
        with open(SCHEMA, encoding="utf-8") as fh:
            schema = json.load(fh)
        errors = schema_check.validate(PLAN, schema)
        self.assertFalse([e for e in errors if "template_cleanup" in str(e) or "special_projects" in str(e)], errors)
        bad = copy.deepcopy(PLAN)
        bad["special_projects"]["stop"] = [{"when": "1937.1.1"}]
        self.assertTrue([e for e in schema_check.validate(bad, schema) if "special_projects" in str(e)])
        bad = copy.deepcopy(PLAN)
        bad["template_cleanup"] = [{"rename_to": "X"}]
        self.assertTrue([e for e in schema_check.validate(bad, schema) if "template_cleanup" in str(e)])


@unittest.skipUnless(HAVE_GAME, f"game files not found at {GAME_DIR}")
class Installed(unittest.TestCase):
    def test_vanilla_project_keys(self):
        game = lint_plan.load_game(GAME_DIR)
        plan = {"_validator": {"tag": "GER", "stop_date": "1937.1.1"},
                "special_projects": {"priority": ["sp_no_such_project"]}}
        out = lint_plan.lint(plan, game)
        hits = [f for f in out["findings"] if f["rule"] == "projects.keys"]
        self.assertEqual(len(hits), 1, hits)
        from hoi4facts.validate import _world
        keys = set(_world(game, {"tag": "GER"}).special_projects())
        self.assertTrue(keys)
        self.assertNotIn("sp_no_such_project", keys)


if __name__ == "__main__":
    unittest.main()
