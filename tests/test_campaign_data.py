"""Phase 3 parsers and planners without the game: script files (constants, overrides), focus trees, technologies
and the tech tree layout, decisions, laws and advisors, buildings and equipment, and the save model's advisors
and decisions."""

import os
import shutil
import tempfile
import unittest

from helpers import FIXTURES

from hoi4_live.gamedata import decisions, economy, focus, politics, scriptdb, tech


def write(root, rel, text):
    p = os.path.join(root, *rel.split("/"))
    os.makedirs(os.path.dirname(p), exist_ok=True)
    with open(p, "w", encoding="utf-8") as f:
        f.write(text)
    return p


FOCUS_FILE = """
focus_tree = {
    id = usa_focus
    country = { factor = 0 modifier = { add = 10 tag = USA } }
    default = no
    shared_focus = SHARED_root
    focus = { id = USA_a  x = 4 y = 0 cost = 10
        completion_reward = { add_political_power = 150 } search_filters = { FOCUS_FILTER_POLITICAL } }
    focus = { id = USA_b  x = 8 y = 0 cost = 5 mutually_exclusive = { focus = USA_a } }
    focus = { id = USA_c  relative_position_id = USA_a x = -1 y = 1 cost = 10
        prerequisite = { focus = USA_a focus = USA_b }
        available = { has_government = democratic } }
    focus = { id = USA_d  relative_position_id = USA_c x = 0 y = 1 cost = 7
        prerequisite = { focus = USA_a } prerequisite = { focus = USA_c }
        offset = { x = 2 y = 0 trigger = { has_dlc = "Man the Guns" } } }
}
focus_tree = {
    id = mex_tree
    country = { factor = 0 modifier = { add = 10 tag = MEX has_dlc = "Trial of Allegiance" } }
    focus = { id = MEX_a x = 0 y = 0 }
}
focus_tree = {
    id = generic_focus
    country = { factor = 1 }
    default = yes
    focus = { id = gen_a x = 0 y = 0 }
}
shared_focus = { id = SHARED_root x = 20 y = 0 cost = 10 }
shared_focus = { id = SHARED_next relative_position_id = SHARED_root x = 0 y = 1
    prerequisite = { focus = SHARED_root } }
shared_focus = { id = SHARED_other x = 30 y = 0 }
"""

TECH_FILE = """
technologies = {
    @1936 = 2
    @1938 = 4
    root_tech = { research_cost = 1.5 start_year = 1918
        folder = { name = test_folder position = { x = 0 y = -1 } }
        path = { leads_to_tech = mid_tech research_cost_coeff = 1 }
        path = { leads_to_tech = side_a research_cost_coeff = 1 }
        enable_equipments = { thing_1 } }
    mid_tech = { research_cost = 2 start_year = 1936
        folder = { name = test_folder position = { x = 0 y = @1936 } } }
    side_a = { research_cost = 1 start_year = 1938 xor = { side_b }
        folder = { name = test_folder position = { x = 1 y = @1938 } } }
    side_b = { research_cost = 1 start_year = 1938 xor = { side_a }
        dependencies = { mid_tech = 1 }
        folder = { name = test_folder position = { x = 2 y = @1938 } } }
    up_root = { research_cost = 1 start_year = 1936
        folder = { name = up_folder position = { x = 1 y = 0 } } }
    pct_root = { research_cost = 1 start_year = 1936
        folder = { name = pct_folder position = { x = 7 y = 2 } } }
}
"""

TAGS_FILE = """
technology_folders = {
    test_folder = { ledger = army }
    up_folder = { ledger = civilian }
    pct_folder = { ledger = army }
    old_folder = { available = { not = { has_dlc = "No Step Back" } } }
    nsb_folder = { available = { has_dlc = "No Step Back" } }
}
"""

GUI_FILE = """
guiTypes = {
    containerWindowType = {
        name = "countrytechtreeview"
        position = { x = 0 y = 0 }
        containerWindowType = {
            name = "folder_tabs"
            position = { x = 10 y = 80 }
            buttonType = { name = "test_folder_tab" position = { x = 22 y = 0 } }
            buttonType = { name = "up_folder_tab" position = { x = 111 y = 0 } }
        }
        containerWindowType = {
            name = "test_folder"
            position = { x = 0 y = 47 }
            containerWindowType = {
                name = "stripes"
                position = { x = 5 y = 10 }
                gridboxtype = { name = "root_tech_tree" position = { x = 140 y = 325 }
                    slotsize = { width = 70 height = 70 } format = "LEFT" }
            }
        }
        containerWindowType = {
            name = "up_folder"
            position = { x = 0 y = 47 }
            gridboxtype = { name = "up_root_tree" position = { x = 0 y = 172 } size = { width = 520 height = 800 }
                slotsize = { width = 70 height = 70 } format = "UP" }
        }
        containerWindowType = {
            name = "pct_folder"
            position = { x = 0 y = 47 }
            size = { width = 100%% height = 100%% }
            margin = { top = 13 left = 13 bottom = 24 right = 25 }
            containerWindowType = {
                name = "pct_stripes"
                position = { x = 0 y = 0 }
                size = { width = 1515 height = 1390 min = { width = 100%% height = 100%% } }
                gridboxtype = { name = "pct_root_tree" position = { x = 300 y = 172 } size = { width = 80% height = 80% }
                    slotsize = { width = 70 height = 70 } format = "UP" }
            }
        }
    }
}
"""


class ScriptDbTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.mkdtemp(prefix="hoi4live_script_")

    def tearDown(self):
        shutil.rmtree(self.tmp, ignore_errors=True)

    def test_constants_and_render(self):
        p = write(self.tmp, "a.txt", "@x = 3\nthing = { pos = @x name = \"A\" list = { a b } }\n")
        block = scriptdb.parse_file(p)
        self.assertEqual(block["thing"]["pos"], 3)
        self.assertNotIn("@x", block)
        self.assertEqual(scriptdb.render(block["thing"]), "pos = 3; name = A; list { a b }")
        self.assertTrue(scriptdb.render({"k": "v" * 50}, limit=20).endswith("..."))
        self.assertEqual(scriptdb.render({"a": {"op": ">", "value": 5}}), "a > 5")

    def test_effective_files_overrides_and_replace_path(self):
        game = os.path.join(self.tmp, "game")
        mod = os.path.join(self.tmp, "mod")
        write(game, "common/x/one.txt", "a = 1")
        write(game, "common/x/two.txt", "a = 2")
        write(mod, "common/x/two.txt", "a = 22")
        write(mod, "common/x/sub/three.txt", "a = 3")
        files = scriptdb.effective_files("common/x", game, [{"path": mod, "replace_path": []}])
        self.assertEqual([os.path.relpath(f, self.tmp).replace("\\", "/") for f in files],
                         ["game/common/x/one.txt", "mod/common/x/two.txt"])
        rec = scriptdb.effective_files("common/x", game, [{"path": mod, "replace_path": []}], recursive=True)
        self.assertEqual(len(rec), 3)
        repl = scriptdb.effective_files("common/x", game, [{"path": mod, "replace_path": ["common/x"]}])
        self.assertEqual([os.path.basename(f) for f in repl], ["two.txt"])


class FocusTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.tmp = tempfile.mkdtemp(prefix="hoi4live_focus_")
        cls.idx = focus.build_index([write(cls.tmp, "common/national_focus/f.txt", FOCUS_FILE)])

    @classmethod
    def tearDownClass(cls):
        shutil.rmtree(cls.tmp, ignore_errors=True)

    def test_trees_and_tag_choice(self):
        self.assertEqual(set(self.idx["trees"]), {"usa_focus", "mex_tree", "generic_focus"})
        self.assertEqual(focus.tree_for_tag(self.idx, "USA"), "usa_focus")
        self.assertEqual(focus.tree_for_tag(self.idx, "MEX"), "mex_tree")      # has_dlc counts as met
        self.assertEqual(focus.tree_for_tag(self.idx, "IRE"), "generic_focus")

    def test_entries(self):
        f = self.idx["trees"]["usa_focus"]["focuses"]
        self.assertEqual(f["USA_c"]["prereq"], [["USA_a", "USA_b"]])
        self.assertEqual(f["USA_d"]["prereq"], [["USA_a"], ["USA_c"]])
        self.assertEqual(f["USA_b"]["mutex"], ["USA_a"])
        self.assertEqual(f["USA_a"]["reward"], "add_political_power = 150")
        self.assertEqual(f["USA_a"]["filters"], ["FOCUS_FILTER_POLITICAL"])
        self.assertEqual(f["USA_c"]["available"], "has_government = democratic")
        self.assertEqual(f["USA_d"]["offsets"][0]["x"], 2)

    def test_shared_focuses_and_positions(self):
        fs = focus.tree_focuses(self.idx, "usa_focus")
        self.assertIn("SHARED_root", fs)
        self.assertIn("SHARED_next", fs)              # follows from SHARED_root through its prerequisite
        self.assertNotIn("SHARED_other", fs)
        pos = focus.positions(fs)
        self.assertEqual(pos["USA_c"], (3, 1))
        self.assertEqual(pos["USA_d"], (3, 2))        # conditional offsets are not applied
        self.assertEqual(pos["SHARED_next"], (20, 1))

    def test_plan_statuses(self):
        fs = focus.tree_focuses(self.idx, "usa_focus")
        st = {r["id"]: r["status"] for r in focus.plan(fs, [], None)}
        self.assertEqual(st["USA_a"], "available")
        self.assertEqual(st["USA_c"], "locked")
        st = {r["id"]: r for r in focus.plan(fs, ["USA_a"], "USA_c", {"USA_a": "A"})}
        self.assertEqual(st["USA_a"]["status"], "completed")
        self.assertEqual(st["USA_c"]["status"], "current")
        self.assertEqual(st["USA_b"]["status"], "excluded")
        self.assertEqual(st["USA_d"]["status"], "locked")       # second AND group (USA_c) not completed
        self.assertEqual(st["USA_a"]["days"], 70)
        self.assertEqual(focus.unmet(fs["USA_d"], ["USA_a"]), ["needs one of: USA_c"])
        rows = focus.plan(fs, [], None, {"USA_a": "Alpha Plan", "USA_b": "Beta Plan"})
        self.assertEqual(focus.match_name(rows, "alpha plan")[0]["id"], "USA_a")
        self.assertEqual(focus.match_name(rows, "USA_b")[0]["id"], "USA_b")
        hit, others = focus.match_name(rows, "plan")
        self.assertIsNone(hit)
        self.assertEqual(len(others), 2)


class TechTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.tmp = tempfile.mkdtemp(prefix="hoi4live_tech_")
        techs = tech.build_index([write(cls.tmp, "common/technologies/t.txt", TECH_FILE)])
        folders = tech.build_folders([write(cls.tmp, "common/technology_tags/t.txt", TAGS_FILE)])
        layout = tech.build_layout([write(cls.tmp, "interface/countrytechtreeview.gui", GUI_FILE)], list(folders))
        cls.idx = {"techs": techs, "folders": folders, "layout": layout}

    @classmethod
    def tearDownClass(cls):
        shutil.rmtree(cls.tmp, ignore_errors=True)

    def test_index(self):
        t = self.idx["techs"]
        self.assertEqual(t["mid_tech"]["folders"][0]["y"], 2)        # @1936 resolved
        self.assertEqual(t["mid_tech"]["parents"], ["root_tech"])
        self.assertEqual(t["side_b"]["deps"], ["mid_tech"])
        self.assertEqual(t["side_a"]["xor"], ["side_b"])
        self.assertEqual(t["root_tech"]["equipment"], ["thing_1"])

    def test_status_and_plan(self):
        t = self.idx["techs"]
        self.assertEqual(tech.status(t["mid_tech"], []), "locked")
        self.assertEqual(tech.status(t["mid_tech"], ["root_tech"]), "available")
        self.assertEqual(tech.status(t["side_b"], ["root_tech"]), "locked")          # dependency mid_tech missing
        self.assertEqual(tech.status(t["side_b"], ["root_tech", "mid_tech"]), "available")
        self.assertEqual(tech.status(t["side_b"], ["root_tech", "mid_tech", "side_a"]), "excluded")
        self.assertEqual(tech.status(t["mid_tech"], ["root_tech"], ["mid_tech"]), "researching")
        rows = tech.plan(self.idx, ["root_tech"], year=1936)
        ids = {r["id"]: r for r in rows}
        self.assertIn("mid_tech", ids)
        self.assertEqual(ids["side_a"]["ahead_years"], 2)
        self.assertNotIn("root_tech", ids)

    def test_folders_and_dlc(self):
        vis = tech.visible_folders(self.idx, has_dlc=lambda d: d == "No Step Back")
        self.assertIn("nsb_folder", vis)
        self.assertNotIn("old_folder", vis)
        vis = tech.visible_folders(self.idx, has_dlc=lambda d: False)
        self.assertIn("old_folder", vis)
        self.assertNotIn("nsb_folder", vis)

    def test_layout(self):
        lay = self.idx["layout"]
        gb = lay["test_folder"]["gridboxes"]["root_tech_tree"]
        self.assertEqual((gb["x"], gb["y"], gb["format"]), (145, 335, "LEFT"))   # nested container (5, 10) added
        self.assertEqual(lay["test_folder"]["tab"], {"x": 32, "y": 80})
        # LEFT: y goes right, x goes down; the tile centre is half a slot in
        self.assertEqual(tech.tile_offset(self.idx, "root_tech", "test_folder"), (145 - 70 + 35, 335 + 35))
        self.assertEqual(tech.tile_offset(self.idx, "mid_tech", "test_folder"), (145 + 140 + 35, 335 + 35))
        self.assertEqual(tech.tile_offset(self.idx, "side_a", "test_folder"), (145 + 280 + 35, 335 + 70 + 35))
        # UP with a width: columns centred in the box
        self.assertEqual(tech.tile_offset(self.idx, "up_root", "up_folder"), (70 + 225 + 35, 172 + 35))
        # UP with a percentage width (the vanilla artillery folder): 80% of the nearest sized ancestor, which
        # grows to the game window less the folder's margins when it has min = 100%
        gb = lay["pct_folder"]["gridboxes"]["pct_root_tree"]
        self.assertEqual((gb["sw"], gb["sw_pct"], gb["parent_w"], gb["parent_fill"]), (None, 0.8, 1515, True))
        self.assertEqual(lay["pct_folder"]["margin_w"], 38)
        self.assertEqual(tech.tile_offset(self.idx, "pct_root", "pct_folder"),
                         (300 + 7 * 70 + (0.8 * 1515 - 70) / 2 + 35, 172 + 2 * 70 + 35))
        self.assertEqual(tech.tile_offset(self.idx, "pct_root", "pct_folder", window_w=1920),
                         (300 + 7 * 70 + (0.8 * (1920 - 38) - 70) / 2 + 35, 172 + 2 * 70 + 35))
        self.assertIn("thing_1", tech.display_keys(self.idx["techs"]["root_tech"], "USA")[1])


class PoliticsDecisionsEconomyTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.tmp = tempfile.mkdtemp(prefix="hoi4live_pol_")

    @classmethod
    def tearDownClass(cls):
        shutil.rmtree(cls.tmp, ignore_errors=True)

    def test_slots_laws_advisors(self):
        slots = politics.build_slots([write(self.tmp, "common/idea_tags/t.txt", """
idea_categories = {
    government = { slot = mobilization_laws slot = trade_laws slot = economy
        character_slot = political_advisor character_slot = political_advisor cost = 150 }
    military_staff = { character_slot = army_chief cost = 150 }
}""")])
        self.assertEqual(slots["political_advisor"]["group"], "government")
        self.assertTrue(slots["political_advisor"]["character"])
        self.assertEqual(slots["_groups"]["government"], ["mobilization_laws", "trade_laws", "economy",
                                                          "political_advisor"])
        laws = politics.build_laws([write(self.tmp, "common/ideas/l.txt", """
ideas = {
    trade_laws = { law = yes
        free_trade = { cost = 150 modifier = { min_export = 0.8 } }
        export_focus = { cost = 100 available = { has_war = no } }
    }
    country = { great_depression = { } }
}""")])
        self.assertEqual(set(laws), {"trade_laws"})
        idx = {"slots": slots, "laws": laws, "advisors": {}}
        self.assertEqual(politics.law_of(idx, "export_focus"), "trade_laws")
        self.assertIsNone(politics.law_of(idx, "great_depression"))
        self.assertEqual(politics.law_cost(idx, "trade_laws", "export_focus"), 100)
        adv = politics.build_advisors([write(self.tmp, "common/characters/USA.txt", """
characters = {
    USA_harold_ickes = { name = "Harold L. Ickes"
        advisor = { slot = political_advisor idea_token = harold_ickes cost = 150
            allowed = { original_tag = USA } traits = { democratic_reformer } } }
    USA_george_patton = {
        advisor = { slot = theorist idea_token = george_patton allowed = { original_tag = USA } } }
}""")])
        self.assertEqual(adv["harold_ickes"]["slot"], "political_advisor")
        self.assertEqual(adv["harold_ickes"]["traits"], ["democratic_reformer"])
        idx["advisors"] = adv
        self.assertEqual([a["token"] for a in politics.advisors_for(idx, "USA", "theorist")], ["george_patton"])
        self.assertEqual(politics.advisors_for(idx, "MEX"), [])

    def test_decisions(self):
        cats = decisions.build_categories([write(self.tmp, "common/decisions/categories/c.txt", """
USA_congress = { icon = generic priority = 100 allowed = { original_tag = USA } }""")])
        decs = decisions.build_decisions([write(self.tmp, "common/decisions/d.txt", """
USA_congress = {
    USA_small_lobby_effort = { cost = 0 days_re_enable = 30 available = { has_war = no }
        complete_effect = { add_political_power = 10 } }
    USA_statehood_for_alaska = { cost = 25 fire_only_once = yes days_remove = 240 }
    USA_custom = { custom_cost_trigger = { command_power > 10 } custom_cost_text = cp_10 }
}""")])
        self.assertEqual(cats["USA_congress"]["priority"], 100)
        self.assertEqual(decs["USA_small_lobby_effort"]["days_re_enable"], 30)
        self.assertEqual(decs["USA_statehood_for_alaska"]["days_remove"], 240)
        self.assertTrue(decs["USA_statehood_for_alaska"]["once"])
        self.assertEqual(decs["USA_custom"]["cost_text"], "cp_10")
        self.assertEqual(set(decisions.in_category({"decisions": decs}, "USA_congress")), set(decs))

    def test_buildings_equipment(self):
        b = economy.build_buildings([write(self.tmp, "common/buildings/b.txt", """
buildings = {
    infrastructure = { base_cost = 3000 provincial = no max_level = 5 }
    bunker = { base_cost = 1500 provincial = yes }
}""")])
        self.assertEqual(b["infrastructure"]["cost"], 3000)
        self.assertTrue(b["bunker"]["provincial"])
        e = economy.build_equipment([write(self.tmp, "common/units/equipment/e.txt", """
equipments = {
    infantry_equipment = { is_archetype = yes build_cost_ic = 0.5 type = infantry }
    infantry_equipment_1 = { archetype = infantry_equipment year = 1939 }
}""")])
        self.assertEqual(e["infantry_equipment_1"]["cost"], 0.5)
        self.assertEqual(e["infantry_equipment_1"]["types"], ["infantry"])
        self.assertEqual(economy.equipment_name_keys("infantry_equipment_1", "USA")[:2],
                         ["USA_infantry_equipment_1", "USA_infantry_equipment_1_short"])


class LocalisationTests(unittest.TestCase):
    def test_references_and_batches(self):
        from hoi4_live.gamedata import loc
        tmp = tempfile.mkdtemp(prefix="hoi4live_loc_")
        try:
            write(tmp, "game/localisation/english/a_l_english.yml",
                  "﻿l_english:\n harold_ickes: \"Harold L. Ickes\"\n USA_harold_ickes: \"$harold_ickes$\"\n"
                  " plain:0 \"§YYellow§! text\"\n")
            write(tmp, "mod/localisation/english/b_l_english.yml", "l_english:\n plain: \"Mod text\"\n")
            L = loc.Localisation(os.path.join(tmp, "game"), [os.path.join(tmp, "mod")],
                                 cache_dir=os.path.join(tmp, "cache"))
            self.assertEqual(L.get_many(["USA_harold_ickes", "plain", "nothing"]),
                             {"USA_harold_ickes": "Harold L. Ickes", "plain": "Mod text"})
            keys = [f"k{i}" for i in range(200)] + ["harold_ickes"]      # the one-pass path for big batches
            L2 = loc.Localisation(os.path.join(tmp, "game"), [], cache_dir=os.path.join(tmp, "cache2"))
            self.assertEqual(L2.get_many(keys), {"harold_ickes": "Harold L. Ickes"})
            self.assertEqual(L2.get("plain"), "Yellow text")
        finally:
            shutil.rmtree(tmp, ignore_errors=True)


class SaveModelPhase3Tests(unittest.TestCase):
    def test_advisors_and_decisions(self):
        from hoi4_live.state import model
        from hoi4_live.state.savefile import SaveFile
        tmp = tempfile.mkdtemp(prefix="hoi4live_model3_")
        try:
            path = os.path.join(tmp, "m.hoi4")
            shutil.copy(os.path.join(FIXTURES, "saves", "model_text.txt"), path)
            names = {"USA_harold_ickes": "Harold L. Ickes", "USA_statehood_for_alaska": "Grant Statehood to Alaska"}
            with SaveFile(path, use_cache=False) as sf:
                st = model.country_state(sf, "USA", ["politics", "decisions"],
                                         names=lambda keys: {k: names[k] for k in keys if k in names})
            adv = st["politics"]["advisors"]
            self.assertEqual([(a["slot"], a["character"]) for a in adv],
                             [("political_advisor", "USA_harold_ickes"), ("theorist", "USA_george_patton")])
            self.assertEqual(adv[0]["name"], "Harold L. Ickes")
            d = st["decisions"]
            self.assertEqual([x["id"] for x in d["taken"]], ["USA_statehood_for_alaska", "USA_small_lobby_effort"])
            self.assertEqual(d["in_effect"], [{"id": "USA_statehood_for_alaska", "days_left": 200,
                                               "name": "Grant Statehood to Alaska"}])
        finally:
            shutil.rmtree(tmp, ignore_errors=True)


if __name__ == "__main__":
    unittest.main()
