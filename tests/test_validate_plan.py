"""Fixture-based tests for scripts/validate_plan.py (no game files needed)."""

import copy
import importlib.util
import os
import tempfile
import unittest

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
_spec = importlib.util.spec_from_file_location(
    "validate_plan", os.path.join(ROOT, "scripts", "validate_plan.py")
)
vp = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(vp)

FILES = {
    "common/technologies/industry.txt": """
technologies = {
    basic_machine_tools = { research_cost = 1.5 start_year = 1936 path = { leads_to_tech = improved_machine_tools } }
    improved_machine_tools = { research_cost = 1.5 start_year = 1937 }
    construction1 = { research_cost = 2 start_year = 1936 }
    radio = { research_cost = 2 dependencies = { construction1 = 1 } }
    start_tech = { research_cost = 1 }
}
""",
    "history/countries/AAA - Testland.txt": """
capital = 1
set_technology = { start_tech = 1 }
1939.1.1 = { set_technology = { basic_machine_tools = 1 } }
""",
    "common/national_focus/test.txt": """
focus_tree = {
    id = test_focus
    focus = { id = T_root cost = 5 }
    focus = { id = T_left cost = 5 prerequisite = { focus = T_root } mutually_exclusive = { focus = T_right } }
    focus = { id = T_right cost = 5 prerequisite = { focus = T_root } mutually_exclusive = { focus = T_left } }
    focus = { id = T_late cost = 5 prerequisite = { focus = T_left } prerequisite = { focus = T_extra } }
    focus = { id = T_extra cost = 5 }
}
""",
    "common/units/equipment/infantry.txt": """
equipments = {
    infantry_equipment = { is_archetype = yes }
    infantry_equipment_1 = { archetype = infantry_equipment }
}
""",
    "common/buildings/00_buildings.txt": """
buildings = { industrial_complex = { } arms_factory = { } infrastructure = { } }
""",
    "history/states/1-One.txt": """
state = { id = 1 name = "STATE_1" state_category = city history = { owner = AAA } }
""",
    "history/states/2-Two.txt": """
state = { id = 2 name = "STATE_2" state_category = town history = { owner = AAA } }
""",
    "localisation/english/state_names_l_english.yml": (
        'l_english:\n STATE_1: "Alpha"\n STATE_2: "Beta"\n AAA_STATE_2: "Betaland"\n'
    ),
    "common/ideas/laws.txt": "ideas = { country = { free_trade = { } } }\n",
    "common/characters/AAA.txt": """
characters = {
    AAA_adviser = { advisor = { slot = political_advisor idea_token = adviser } }
}
""",
    "common/decisions/AAA.txt": "AAA_cat = { AAA_decision = { } }\n",
    "history/units/AAA_1936.txt": 'division_template = { name = "Line Division" }\n',
}

GOOD = {
    "_validator": {
        "tag": "AAA",
        "stop_date": "1938.1.1",
        "history_file": "history/countries/AAA - Testland.txt",
    },
    "research_priority": ["basic_machine_tools", "improved_machine_tools", "construction1", "radio"],
    "focus_path": ["T_root", "T_left", "T_extra", "T_late"],
    "focus_fallback": [],
    "production": [{"equipment": "infantry_equipment_1", "factories": 3}],
    "production_priority": ["infantry_equipment"],
    "construction": [{"building": "industrial_complex", "states": [1, "Beta"], "count": 2}],
    "politics": [{"category": "government", "slot": 1, "idea": "free_trade"}],
    "decisions": ["AAA_decision"],
    "training": {"template": "Line Division", "queue": 2, "batch": 1, "deploy_state": 1},
    "stop_on_date": "1938.01.01",
}


class ValidatePlanTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.tmp = tempfile.TemporaryDirectory()
        for rel, text in FILES.items():
            path = os.path.join(cls.tmp.name, *rel.split("/"))
            os.makedirs(os.path.dirname(path), exist_ok=True)
            with open(path, "w", encoding="utf-8") as fh:
                fh.write(text)
        cls.game = vp.Game(cls.tmp.name)

    @classmethod
    def tearDownClass(cls):
        cls.tmp.cleanup()

    def run_plan(self, **changes):
        plan = copy.deepcopy(GOOD)
        plan.update(changes)
        rep, _ = vp.validate(plan, self.game)
        return rep

    def test_clean_plan_has_no_findings(self):
        rep = self.run_plan()
        self.assertEqual(rep.errors, [])
        self.assertEqual(rep.warnings, [])

    def test_parser_handles_comments_and_nesting(self):
        items = vp.parse('a = { b = 1 # c = 2\n c = { d } }\n')
        self.assertEqual(vp.get1(vp.get1(items, "a"), "b"), "1")
        self.assertEqual(vp.get1(vp.get1(vp.get1(items, "a"), "c"), None), "d")

    def test_unknown_and_duplicate_techs(self):
        rep = self.run_plan(research_priority=["basic_machine_tools", "nope", "basic_machine_tools"])
        text = "\n".join(rep.errors)
        self.assertIn("unknown technology nope", text)
        self.assertIn("duplicate key basic_machine_tools", text)

    def test_tech_prerequisite_order(self):
        rep = self.run_plan(research_priority=["improved_machine_tools", "basic_machine_tools"])
        self.assertTrue(any("improved_machine_tools" in e and "prerequisite" in e for e in rep.errors))
        # radio depends on construction1 (dependencies block), which comes later
        rep = self.run_plan(research_priority=["radio", "construction1"])
        self.assertTrue(any("radio" in e and "construction1" in e for e in rep.errors))

    def test_start_techs_do_not_count_later_bookmark_blocks(self):
        have = self.game.start_techs("history/countries/AAA - Testland.txt")
        self.assertEqual(have, {"start_tech"})

    def test_focus_order_and_exclusion(self):
        rep = self.run_plan(focus_path=["T_late", "T_root"])
        self.assertTrue(any("T_late" in e and "prerequisite" in e for e in rep.errors))
        rep = self.run_plan(focus_path=["T_root", "T_left", "T_right"])
        self.assertTrue(any("mutually exclusive" in e for e in rep.errors))
        rep = self.run_plan(focus_path=["T_root", "T_missing"])
        self.assertTrue(any("unknown focus T_missing" in e for e in rep.errors))

    def test_focus_fallback_conflicts(self):
        rep = self.run_plan(focus_fallback=["T_right"])
        self.assertTrue(any("T_right" in e and "exclusive" in e for e in rep.errors))

    def test_equipment_buildings_states(self):
        rep = self.run_plan(
            production=[{"equipment": "nonsense_1", "factories": 2}],
            production_priority=["tank"],
            construction=[
                {"building": "bogus", "states": [99, "Gamma", "Beta", 2]},
            ],
        )
        text = "\n".join(rep.errors)
        self.assertIn("unknown equipment nonsense_1", text)
        self.assertIn("no equipment starts with tank", text)
        self.assertIn("unknown building bogus", text)
        self.assertIn("unknown state id 99", text)
        self.assertIn("unknown state name 'Gamma'", text)
        self.assertIn("duplicate key 2", text)  # "Beta" resolves to state 2

    def test_localised_variant_names_resolve(self):
        rep = self.run_plan(construction=[{"building": "arms_factory", "states": ["Betaland"]}])
        self.assertEqual(rep.errors, [])

    def test_politics_and_decisions(self):
        rep = self.run_plan(
            politics=[
                {"category": "wrong", "slot": 1, "idea": "free_trade"},
                {"category": "government", "slot": 9, "idea": "free_trade"},
                {"category": "government", "slot": 1, "idea": "no_such_idea"},
                {"category": "government", "slot": 3, "idea": "AAA_adviser"},
            ],
            decisions=["AAA_decision", "AAA_missing"],
        )
        text = "\n".join(rep.errors)
        self.assertIn("unknown category wrong", text)
        self.assertIn("slot must be 0..7", text)
        self.assertIn("unknown idea/character no_such_idea", text)
        self.assertIn("unknown decision AAA_missing", text)
        self.assertNotIn("AAA_adviser", text)

    def test_advisor_token_is_a_warning(self):
        rep = self.run_plan(politics=[{"category": "government", "slot": 3, "idea": "adviser"}])
        self.assertEqual(rep.errors, [])
        self.assertTrue(any("idea_token" in w for w in rep.warnings))

    def test_misc_shapes(self):
        rep = self.run_plan(stop_on_date="1938-01-01", event_options={"x.1": -1}, active="yes")
        text = "\n".join(rep.errors)
        self.assertIn("stop_on_date", text)
        self.assertIn("event_options[x.1]", text)
        self.assertIn("active must be boolean", text)

    def test_command_line_exit_status(self):
        import io
        import json
        from contextlib import redirect_stdout

        path = os.path.join(self.tmp.name, "plan.json")
        with open(path, "w", encoding="utf-8") as fh:
            json.dump(GOOD, fh)
        out = io.StringIO()
        with redirect_stdout(out):
            code = vp.main([path, "--game-dir", self.tmp.name])
        self.assertEqual(code, 0)
        self.assertIn("0 error(s)", out.getvalue())
        bad = dict(GOOD, decisions=["AAA_missing"])
        with open(path, "w", encoding="utf-8") as fh:
            json.dump(bad, fh)
        with redirect_stdout(io.StringIO()):
            self.assertEqual(vp.main([path, "--game-dir", self.tmp.name]), 1)


if __name__ == "__main__":
    unittest.main()
