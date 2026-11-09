"""Tool gaps a strategist run reported (Italy-Ethiopia): duplicated-archetype equipment ids, the _validator
defaults from the run folder, script-completed focus prerequisites, vp rows on another country's state, FAIR
grep without the AI-only folders, event callers, scripted effect bodies, untruncated hook conditions and the
per-resource balance. Fixture game in a temporary folder; no installed game needed."""
import contextlib
import copy
import io
import json
import os
import sys
import tempfile
import unittest

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.join(ROOT, "scripts"))

import validate_plan as vp  # noqa: E402
from hoi4facts import facts, scenario  # noqa: E402
from hoi4facts.game import Game  # noqa: E402
from hoi4facts.world import World  # noqa: E402

LONG = " ".join(str(1000 + i) for i in range(60))

FILES = {
    "common/units/equipment/plane_airframes.txt": """
equipments = {
    small_plane_airframe = { is_archetype = yes resources = { aluminium = 2 rubber = 1 } module_slots = { } }
    small_plane_airframe_0 = { archetype = small_plane_airframe }
    small_plane_airframe_1 = { archetype = small_plane_airframe resources = { aluminium = 3 rubber = 1 } }
}
""",
    "common/units/equipment/x_plane_airframes.txt": """
duplicate_archetypes = {
    small_plane_cas_airframe = {
        archetype = small_plane_airframe
        variant_name = { small_plane_cas_airframe_1 = CAS_equipment_1 }
    }
}
""",
    "common/units/equipment/infantry.txt": """
equipments = {
    infantry_equipment = { is_archetype = yes resources = { steel = 2 } }
    infantry_equipment_1 = { archetype = infantry_equipment }
}
""",
    "common/national_focus/aaa.txt": """
focus_tree = {
    id = aaa_focus
    country = { factor = 0 modifier = { add = 10 tag = AAA } }
    focus = { id = AAA_start cost = 5 }
    focus = { id = AAA_success cost = 5
        available = { custom_trigger_tooltip = { tooltip = AAA_success_tt always = no } } }
    focus = { id = AAA_after cost = 10 prerequisite = { focus = AAA_success } }
    focus = { id = AAA_unlockable cost = 5 available = { always = no } }
    focus = { id = AAA_mediation cost = 5
        completion_reward = { BBB = { country_event = { id = test.2 hours = 1 random_hours = 4 } } } }
}
""",
    "common/scripted_effects/aaa_effects.txt": """
AAA_mission_success_effect = {
    add_stability = 0.03
    add_war_support = 0.05
    complete_national_focus = AAA_success
}
""",
    "common/decisions/AAA.txt": "AAA_cat = { AAA_mission = { complete_effect = { AAA_mission_success_effect = yes } } }\n",
    "events/test.txt": """
add_namespace = test
country_event = { id = test.2 title = test.2.t is_triggered_only = yes
    option = { name = test.2.a }
    option = { name = test.2.b FROM = { country_event = { id = test.3 hours = 2 } } }
}
country_event = { id = test.3 title = test.3.t is_triggered_only = yes option = { name = test.3.a } }
""",
    "common/ai_strategy_plans/AAA_plan.txt": "AAA_plan = { enable = { has_completed_focus = AAA_start } }\n",
    "common/on_actions/00_on_actions.txt": """
on_actions = {
    on_daily_AAA = {
        effect = {
            if = {
                limit = { BBB = { num_divisions_in_states = { count > 24 states = { %s } exclude = { irregular_infantry } } } }
                set_country_flag = AAA_escalated_flag
            }
        }
    }
}
""" % LONG,
    "history/countries/AAA - Testland.txt": "capital = 1\nadd_ideas = { test_exports }\nset_oob = AAA_1936\nunlock_national_focus = AAA_unlockable\n",
    "history/countries/BBB - Otherland.txt": "capital = 3\n",
    "history/countries/CCC - Thirdland.txt": "capital = 2\n",
    "common/country_tags/00_countries.txt": 'AAA = "countries/A.txt"\nBBB = "countries/B.txt"\nCCC = "countries/C.txt"\n',
    "common/ideas/laws.txt": """
ideas = {
    trade_laws = {
        law = yes
        test_exports = { modifier = { min_export = 0.25 } }
    }
}
""",
    "history/units/AAA_1936.txt": """
division_template = { name = "Line Division" }
instant_effect = {
    add_equipment_production = { equipment = { type = infantry_equipment_1 creator = "AAA" } requested_factories = 2 }
    add_equipment_production = { equipment = { type = small_plane_cas_airframe_0 creator = "AAA" version_name = "Old CAS" } requested_factories = 1 }
}
""",
    "history/states/1-One.txt": """
state = { id = 1 name = "STATE_1" state_category = city provinces = { 10 } resources = { steel = 10 aluminium = 1 }
    history = { owner = AAA victory_points = { 10 5 } } }
""",
    "history/states/2-Two.txt": """
state = { id = 2 name = "STATE_2" state_category = town provinces = { 20 } history = { owner = CCC victory_points = { 20 2 } } }
""",
    "history/states/3-Three.txt": """
state = { id = 3 name = "STATE_3" state_category = town provinces = { 30 } history = { owner = BBB victory_points = { 30 3 } } }
""",
    "common/buildings/00_buildings.txt": "buildings = { industrial_complex = { } }\n",
    "localisation/english/test_l_english.yml": 'l_english:\n test.2.t: "Mediation"\n',
}


def build(tmp):
    for rel, text in FILES.items():
        path = os.path.join(tmp, *rel.split("/"))
        os.makedirs(os.path.dirname(path), exist_ok=True)
        with open(path, "w", encoding="utf-8") as fh:
            fh.write(text)


class Fixture(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.tmp = tempfile.TemporaryDirectory()
        build(cls.tmp.name)
        cls.game = Game(cls.tmp.name, cache_dir=tempfile.gettempdir())

    @classmethod
    def tearDownClass(cls):
        cls.tmp.cleanup()

    def world(self, mode="fair"):
        return World(Game(self.tmp.name, cache_dir=tempfile.gettempdir()), "AAA", "1936.1.1", [], mode)

    def cli(self, argv):
        buf = io.StringIO()
        with contextlib.redirect_stdout(buf):
            rc = facts.main(argv + ["--game", self.tmp.name, "--dlc", "none", "--userdir", self.tmp.name,
                                    "--tag", "AAA"])
        return rc, buf.getvalue()


class Equipment(Fixture):
    def test_duplicated_archetype_copies_every_equipment(self):
        eq = self.game.equipment()
        self.assertIn("small_plane_cas_airframe_0", eq)  # copy of small_plane_airframe_0
        self.assertIn("small_plane_cas_airframe_1", eq)

    def test_validate_accepts_the_start_line(self):
        plan = {"_validator": {"tag": "AAA"},
                "production": [{"equipment": "small_plane_cas_airframe_0", "factories": 1},
                               {"equipment": "small_plane_cas_airframe_9", "factories": 1}]}
        rep, _ = vp.validate(plan, self.game)
        text = "\n".join(rep.errors)
        self.assertNotIn("unknown equipment small_plane_cas_airframe_0", text)
        self.assertIn("unknown equipment small_plane_cas_airframe_9", text)


class ValidatorDefaults(unittest.TestCase):
    def test_tag_from_run_spec_and_manifest(self):
        with tempfile.TemporaryDirectory() as run:
            os.makedirs(os.path.join(run, "dossier"))
            with open(os.path.join(run, "run_spec.json"), "w") as fh:
                json.dump({"tag": "ITA", "start_date": "1936.1.1", "mode": "fair", "dlcs": "auto"}, fh)
            with open(os.path.join(run, "dossier", "manifest.json"), "w") as fh:
                json.dump({"tag": "ITA", "date": "1936.1.1", "dlcs": ["By Blood Alone"]}, fh)
            plan = {}
            note, warn = vp.derive_validator(plan, os.path.join(run, "plan.json"))
            self.assertIsNone(warn)
            self.assertEqual(plan["_validator"]["tag"], "ITA")
            self.assertEqual(plan["_validator"]["dlcs"], ["By Blood Alone"])  # "auto" in the spec is skipped
            self.assertIn("run_spec.json", note)
            plan = {"_validator": {"tag": "GER"}}
            vp.derive_validator(plan, os.path.join(run, "plan.json"))
            self.assertEqual(plan["_validator"]["tag"], "GER")  # an explicit tag wins

    def test_missing_tag_warns(self):
        with tempfile.TemporaryDirectory() as run:
            plan = {}
            note, warn = vp.derive_validator(plan, os.path.join(run, "plan.json"))
            self.assertIsNone(note)
            self.assertIn("GER", warn)


class ScriptCompletedFocus(Fixture):
    def test_tree_marks_never_available(self):
        tree = self.game.focus_trees()["aaa_focus"]
        self.assertTrue(tree["AAA_success"]["script_only"])
        self.assertFalse(tree["AAA_after"]["script_only"])
        self.assertIn("AAA_success", self.game.script_completed_focuses())

    def test_prerequisite_satisfied_by_script(self):
        rep, _ = vp.validate({"_validator": {"tag": "AAA"}, "focus_path": ["AAA_start"],
                              "focus_fallback": ["AAA_after"]}, self.game)
        self.assertEqual([w for w in rep.warnings if "AAA_after" in w], [])
        rep, _ = vp.validate({"_validator": {"tag": "AAA"}, "focus_path": ["AAA_after"]}, self.game)
        self.assertEqual([e for e in rep.errors if "AAA_after" in e], [])
        self.assertTrue(any("completed only by script" in w for w in rep.warnings))
        rep, _ = vp.validate({"_validator": {"tag": "AAA"}, "focus_path": ["AAA_start"],
                              "focus_fallback": ["AAA_success"]}, self.game)
        self.assertTrue(any("AAA_success" in w and "always = no" in w for w in rep.warnings))
        rep, _ = vp.validate({"_validator": {"tag": "AAA"}, "focus_path": ["AAA_unlockable"]}, self.game)
        self.assertEqual([w for w in rep.warnings if "AAA_unlockable" in w], [])  # unlock_national_focus lifts it


class VpOwner(Fixture):
    def test_vp_zero_on_another_countrys_state(self):
        base = {"_validator": {"tag": "AAA"}, "goal": {"predicates": ["capitulated:BBB"]}}
        row = {"id": "t", "province": 20, "state": 2}
        rep, _ = vp.validate(dict(copy.deepcopy(base), vp_targets=[dict(row, vp=0)]), self.game)
        self.assertEqual([e for e in rep.errors if "vp_targets" in e], [])
        rep, _ = vp.validate(dict(copy.deepcopy(base), vp_targets=[dict(row, vp=1)]), self.game)
        self.assertTrue(any("CCC's, not the enemy's" in e for e in rep.errors))
        rep, _ = vp.validate(dict(copy.deepcopy(base), vp_targets=[{"id": "b", "province": 30, "state": 3, "vp": 0}]),
                             self.game)
        self.assertTrue(any("vp 0 but the state history gives 3" in e for e in rep.errors))


class FairGrep(Fixture):
    def test_fair_skips_ai_only_folders(self):
        rows = scenario.grep(self.world("fair"), "AAA_start")
        self.assertFalse(any("ai_strategy" in r.get("src", "") for r in rows))
        self.assertTrue(any(r.get("hidden_ai_only") for r in rows))
        rows = scenario.grep(self.world("meta"), "AAA_start")
        self.assertTrue(any("ai_strategy_plans" in r.get("src", "") for r in rows))
        rc, out = self.cli(["grep", "AAA_start", "--mode", "fair"])
        self.assertNotIn("ai_strategy_plans", out.split("hidden")[0])

    def test_substring_fallback_and_context(self):
        rows = scenario.grep(self.world(), "mission_success_effect", context=1)
        self.assertTrue(rows and rows[0].get("substring"))
        self.assertEqual(len(rows[0]["context"]), 2)


class Callers(Fixture):
    def test_event_callers_with_scope_and_delay(self):
        rows = scenario.event_callers(self.world(), "test.2")
        self.assertEqual(len(rows), 1)
        r = rows[0]
        self.assertIn("focus(AAA_mediation)", r["via"])
        self.assertEqual(r["scope"], "BBB")
        self.assertEqual(r["delay"], {"hours": "1", "random_hours": "4"})
        rows = scenario.event_callers(self.world(), "test.3")
        self.assertEqual(rows[0]["scope"], "FROM")
        self.assertIn("option(test.2.b)", rows[0]["via"])
        rc, out = self.cli(["event", "test.2"])
        self.assertEqual(rc, 0, out)
        self.assertIn("fired by common/national_focus/aaa.txt", out)

    def test_effect_body_and_callers(self):
        e = scenario.effect_body(self.world(), "AAA_mission_success_effect")
        self.assertIn("add_war_support = 0.05", e["text"])
        self.assertTrue(e["text"].rstrip().endswith("}"))
        rc, out = self.cli(["effect", "AAA_mission_success_effect"])
        self.assertEqual(rc, 0, out)
        self.assertIn("called from common/decisions/AAA.txt", out)


class Hooks(Fixture):
    def test_condition_is_not_cut(self):
        rows = scenario._on_action_hooks(self.world(), "AAA")
        self.assertTrue(any(LONG in (r.get("condition") or "") and "irregular_infantry" in r["condition"]
                            for r in rows))


class Resources(Fixture):
    def test_balance(self):
        r = self.world().resources("AAA")
        self.assertEqual(r["trade_law"], "test_exports")
        self.assertEqual(r["balance"]["steel"], {"produced": 10.0, "exported": 2.5, "available": 7.5,
                                                 "needed": 4.0, "balance": 3.5})
        self.assertEqual(r["balance"]["rubber"]["needed"], 1.0)  # the CAS copy takes its source's resources
        self.assertIn("small_plane_cas_airframe_0", r["partial_lines"])


if __name__ == "__main__":
    unittest.main()
