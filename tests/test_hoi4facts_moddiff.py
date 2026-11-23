"""scripts/hoi4facts/moddiff.py and tool_widgets.py: what a mod set adds against the plain game.

A tiny install and a tiny total conversion are written to a temporary folder: the mod replaces the
resources and the map, adds a building, a decision category with a mission, a scripted interface with a
button that spends political power and one that declares war, overrides a define with the
`NDefines.NX.KEY = v` form mods use, and replaces a panel's .gui file without the element a typed tool
looks up.
"""

import os
import sys
import tempfile
import unittest

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.join(ROOT, "scripts"))

from hoi4facts import moddiff, tool_widgets  # noqa: E402
from hoi4facts.content import Mod  # noqa: E402
from hoi4facts.game import Game  # noqa: E402

GAME = {
    "common/resources/00_resources.txt": "resources = { oil = { icon_frame = 1 } steel = { icon_frame = 2 } }",
    "common/buildings/00_buildings.txt": "buildings = { arms_factory = { cost = 7200 } }",
    "common/decisions/categories/cats.txt": "economy_decisions = { icon = generic }",
    "common/decisions/eco.txt": "economy_decisions = { build_x = { cost = 50 } }",
    "common/defines/00_defines.lua": "NDefines = {\nNGame = {\n\tSTART_DATE = \"1936.1.1.12\",\n"
                                     "\tMAX_X = 10,\n},\n}\n",
    "interface/decisions.gui": "guiTypes = { containerWindowType = { name = \"decisions_view\" "
                               "buttonType = { name = \"decisionview_button\" } "
                               "instantTextBoxType = { name = \"name_text\" } } }",
    "map/definition.csv": "0;0;0;0;land;false;unknown;0\n1;1;1;1;land;false;plains;1\n2;2;2;2;land;false;plains;1\n",
    "history/states/1-A.txt": "state = { id = 1 }",
    "localisation/english/x_l_english.yml": "l_english:\n oil:0 \"Oil\"\n",
}
MOD = {
    "common/resources/00_resources.txt": "resources = { water = { icon_frame = 1 } }",
    "common/buildings/mod_buildings.txt": "buildings = { water_pump = { cost = 3000 } }",
    "common/decisions/categories/mod_cats.txt": "wasteland_decisions = { icon = generic }",
    "common/decisions/mod.txt": "wasteland_decisions = { scavenge = { cost = 10 } "
                                "hold_the_line = { days_mission_timeout = 30 } }",
    "common/scripted_guis/caps.txt": (
        "scripted_gui = { caps_panel = { context_type = player_context window_name = \"caps_window\" "
        "parent_window_token = politics_tab visible = { always = yes } "
        "effects = { buy_button_click = { add_political_power = -25 } "
        "raid_button_click = { if = { limit = { always = yes } declare_war_on = { target = XXX type = annex } } } "
        "info_button_click = { set_country_flag = seen } } "
        "triggers = { buy_button_click_enabled = { has_political_power > 25 } } } }"),
    "common/defines/01_mod.lua": "NDefines.NGame.START_DATE = \"2275.1.1.12\"\n",
    "interface/decisions.gui": "guiTypes = { containerWindowType = { name = \"decisions_view\" "
                               "instantTextBoxType = { name = \"name_text\" } } }",
    "interface/caps.gui": "guiTypes = { containerWindowType = { name = \"caps_window\" "
                          "buttonType = { name = \"buy_button\" } instantTextBoxType = { name = \"caps_value\" "
                          "text = \"[?caps_amount|0]\" } } }",
    "map/definition.csv": "0;0;0;0;land;false;unknown;0\n1;1;1;1;land;false;plains;1\n",
    "localisation/english/mod_l_english.yml": "l_english:\n water:0 \"£water_icon Water\"\n"
                                              " caps_window:0 \"§YCaps§!\"\n",
}


def write(base, files):
    for rel, text in files.items():
        path = os.path.join(base, rel)
        os.makedirs(os.path.dirname(path), exist_ok=True)
        with open(path, "w", encoding="utf-8") as fh:
            fh.write(text)


class ModDiffTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.tmp = tempfile.TemporaryDirectory()
        game, mod = os.path.join(cls.tmp.name, "game"), os.path.join(cls.tmp.name, "mod")
        write(game, GAME)
        write(mod, MOD)
        widgets = {"modules": {"decisions": {"tools": ["hoi4_decision_take"],
                                             "widgets": ["decisionview_button", "name_text"]}}}
        cls.rep = moddiff.diff(Game(game, []), Game(game, [Mod(mod, ["common/resources", "map"], "Wasteland")]),
                               tool_widgets=widgets)

    @classmethod
    def tearDownClass(cls):
        cls.tmp.cleanup()

    def test_categories_report_new_and_removed(self):
        c = self.rep["categories"]
        self.assertEqual(c["resources"]["new"], ["water"])
        self.assertEqual(sorted(c["resources"]["removed"]), ["oil", "steel"])
        self.assertEqual(c["buildings"]["new"], ["water_pump"])
        self.assertEqual(c["decision_categories"]["new"], ["wasteland_decisions"])
        self.assertEqual(c["decisions"]["new"], ["hold_the_line", "scavenge"])
        self.assertEqual(c["decisions"]["missions_modded"], 1)

    def test_scripted_gui_buttons_carry_cost_and_irreversible_effects(self):
        sg = self.rep["scripted_guis"]
        self.assertEqual(sg["new_count"], 1)
        g = sg["new"][0]
        self.assertEqual((g["name"], g["window_name"], g["parent_window"]), ("caps_panel", "caps_window",
                                                                              "politics_tab"))
        b = {x["widget"]: x for x in g["buttons"]}
        self.assertEqual(b["buy_button"]["spends"], ["add_political_power"])
        self.assertTrue(b["buy_button"]["enabled_trigger"])
        self.assertEqual(b["raid_button"]["irreversible"], ["declare_war_on"])
        self.assertEqual(b["info_button"]["spends"] + b["info_button"]["irreversible"], [])

    def test_map_defines_interface_and_variables(self):
        self.assertTrue(self.rep["map"]["replaced"])
        self.assertEqual(self.rep["map"]["modded"]["provinces"], 1)
        self.assertEqual(self.rep["map"]["vanilla"]["provinces"], 2)
        ch = {d["name"]: d for d in self.rep["defines"]["changed"]}
        self.assertEqual(ch["START_DATE"]["modded"], "2275.1.1.12")
        self.assertEqual(self.rep["interface"]["replaced_files"], ["interface/decisions.gui"])
        self.assertEqual(self.rep["interface"]["new_files"], ["interface/caps.gui"])
        self.assertIn("caps_amount", self.rep["shown_variables"]["names"])

    def test_typed_tools_whose_elements_vanish_are_named(self):
        rows = self.rep["affected_tools"]
        self.assertEqual(len(rows), 1)
        self.assertEqual(rows[0]["tools"], ["hoi4_decision_take"])
        self.assertEqual(rows[0]["missing_widgets"], ["decisionview_button"])

    def test_learning_targets_put_broken_panels_then_resources_then_costly_guis_first(self):
        kinds = [t["kind"] for t in self.rep["learning_targets"]]
        self.assertEqual(kinds[:4], ["replaced_panel", "resource", "building", "scripted_gui"])
        gui = self.rep["learning_targets"][3]
        self.assertEqual(gui["name"], "Caps")
        self.assertEqual(gui["spends"], ["add_political_power"])
        self.assertEqual(self.rep["learning_targets"][1]["name"], "Water")
        self.assertIn("Learn first:", moddiff.text(self.rep))


class ToolWidgetsTest(unittest.TestCase):
    def test_lookups_count_and_json_keys_do_not(self):
        with tempfile.TemporaryDirectory() as tmp:
            src = os.path.join(tmp, "macos")
            os.makedirs(src)
            with open(os.path.join(src, "decisions.rs"), "w") as fh:
                fh.write('fn f() { find(r, i, root, "decisionview_button", 6); json!({"name_text": 1}); '
                         'v["cost"]; }\n#[cfg(test)]\nmod tests { const X: &str = "caps_window"; }\n')
            with open(os.path.join(tmp, "macos.rs"), "w") as fh:
                fh.write('if name == "hoi4_decision_take" { decisions::take(p) }\n')
            out = tool_widgets.scan({"decisionview_button", "name_text", "cost", "caps_window"}, src,
                                    os.path.join(tmp, "macos.rs"))
        self.assertEqual(out["modules"]["decisions"], {"tools": ["hoi4_decision_take"],
                                                       "widgets": ["decisionview_button"]})


if __name__ == "__main__":
    unittest.main()
