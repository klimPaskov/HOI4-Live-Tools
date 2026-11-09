"""validate_plan checks for the executor keys: shapes, research_path (both tech readings),
construction_plan, vp_targets, army_layout, war, doctrines, event_rules. Fixture game written to a
temp folder (no game files needed), with a 4 x 2 provinces.bmp:

    row 0:  10 10 11 20        state 1 = 10, 11 (VP 10 = 5)   state 2 = 20, 21   state 3 = 30 (BBB)
    row 1:  10 11 21 30
"""

import copy
import importlib.util
import os
import struct
import tempfile
import unittest

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
_spec = importlib.util.spec_from_file_location("validate_plan", os.path.join(ROOT, "scripts", "validate_plan.py"))
vp = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(vp)

COLOURS = {10: (10, 0, 0), 11: (11, 0, 0), 20: (20, 0, 0), 21: (21, 0, 0), 30: (30, 0, 0)}
ROWS = [[10, 10, 11, 20], [10, 11, 21, 30]]  # top row first

FILES = {
    "common/country_tags/00_countries.txt": 'AAA = "countries/A.txt"\nBBB = "countries/B.txt"\n',
    "common/technologies/t.txt": """technologies = {
    t_a = { start_year = 1936 path = { leads_to_tech = t_ab } }
    t_b = { start_year = 1936 path = { leads_to_tech = t_ab } }
    t_ab = { start_year = 1936 }
    t_late = { start_year = 1939 }
}
""",
    "history/countries/AAA - Alpha.txt": 'capital = 1\nset_oob = "AAA_1936"\nset_technology = { t_a = 1 }\n',
    "history/states/1-One.txt": """state = { id = 1 name = "STATE_1"
    history = { owner = AAA victory_points = { 10 5 } }
    provinces = { 10 11 } }
""",
    "history/states/2-Two.txt": 'state = { id = 2 name = "STATE_2" history = { owner = AAA } provinces = { 20 21 } }\n',
    "history/states/3-Three.txt": 'state = { id = 3 name = "STATE_3" history = { owner = BBB } provinces = { 30 } }\n',
    "history/units/AAA_1936.txt": """division_template = { name = "Line" }
division_template = { name = "Guard" }
units = {
    division = { location = 10 division_template = "Line" }
    division = { location = 10 division_template = "Line" }
    division = { location = 11 division_template = "Guard" }
    division = { location = 20 division_template = "Line" }
}
""",
    "common/buildings/00_buildings.txt": "buildings = { supply_node = { } rail_way = { } infrastructure = { } }\n",
    "common/doctrines/folders/f.txt": "land = { allowed = { always = yes } ledger = army }\n",
    "common/doctrines/grand_doctrines/g.txt": "grand_x = { folder = land xp_cost = 100 tracks = { inf } }\n",
    "common/doctrines/tracks/t.txt": "inf = { name = X }\n",
    "common/doctrines/subdoctrines/land/s.txt": "sub_inf = { track = inf xp_cost = 50 xp_type = army }\n",
    "events/aaa.txt": "add_namespace = aaa\ncountry_event = { id = aaa.1 option = { name = aaa.1.a } "
                      "option = { name = aaa.1.b } }\n",
    "map/definition.csv": "0;0;0;0;land;false;unknown;0\n"
    + "".join(f"{p};{r};{g};{b};land;false;plains;1\n" for p, (r, g, b) in COLOURS.items()),
    "common/units/land.txt": """sub_units = {
    infantry = { group = infantry combat_width = 2 need = { gun = 100 } }
    cavalry = { group = mobile combat_width = 2 active = yes need = { gun = 120 } }
    artillery_brigade = { group = combat_support combat_width = 3 active = yes }
    engineer = { group = support combat_width = 0 }
    fire_support = { group = support combat_width = 0 categories = { category_regimental_support_battalions } }
}
""",
    "common/units/equipment/gun.txt": "equipments = { gun = { is_archetype = yes } gun_1 = { archetype = gun } }\n",
    "common/occupation_laws/laws.txt": "no_garrison = { }\nmilitary_governor_occupation = { }\n",
}

NEW_KEYS = {
    "_meta": {"version": 1, "source": "strategist"},
    "phases": [{"id": "P1", "enter": "1936.02.01", "exit": {"all": ["state_fully_controlled:3", "stability >= 0.5"]}}],
    "goal": {"predicates": [{"any": ["capitulated:BBB", "owner:3 == AAA"]}], "deadline": "1937.12.01"},
    "contingencies": [{"id": "lost", "when": ["province_controller:10=BBB", "war_support < 25%"],
                       "then": {"phase": "P1"}}],
    "event_rules": [{"event": "aaa.1", "option_name": "aaa.1.b", "when": "at_war_with:BBB",
                     "else_option_name": "aaa.1.a", "fallback_index": 1}],
    "checkpoints": [{"id": "cp", "by": "1936.06.01", "must": ["controls_state:3"], "else": "P1"}],
    "politics": [],
    "templates": [
        {"id": "line", "name": "Line", "regiments": [["infantry"] * 3, ["artillery_brigade"]], "support": ["engineer"],
         "regimental_support": ["fire_support"], "role": "line", "xp": 30, "when": "phase:P1"},
        {"id": "flat", "name": "Flat", "regiments": ["cavalry", "cavalry"], "role": "garrison"},
    ],
    "training": [{"template": "Line", "deploy_state": 1, "queue": 2, "when": "date >= 1936.03.01"},
                 {"template": "Guard", "deploy_state": 2}],
    "xp_budget": {"army": [{"id": "x1", "cost": 30, "purpose": "template", "ref": "template:line"}]},
    "stockpile_targets": {"gun": 30, "gun_1": 10},
    "force_posture": {"reason": "land war", "phases": [{"phase": "base", "land": 100, "air": 0, "navy": 0},
                                                       {"phase": "P1", "land": 80, "air": 20, "navy": 0}]},
    "occupation": {"default_law": "military_governor_occupation", "ladder": ["no_garrison", "military_governor_occupation"],
                   "harshest": "military_governor_occupation", "per_state": {"3": "no_garrison"}},
    "peace": {"take": [3], "puppet": [], "max_rounds": 3},
    "manage": {"stability_floor": 0.4},
    "sea_transfer": {"convoys": 4, "ports": [2], "routes": [{"id": "r1", "from_state": 1, "to_state": 2}]},
}

GOOD = {
    "_validator": {"tag": "AAA", "stop_date": "1938.1.1", "dlcs": []},
    "research_path": [
        {"year": 1936, "not_before": "1936.01.01", "techs": ["t_b"]},
        {"year": 1937, "not_before": "1937.01.01", "techs": ["t_ab"]},
    ],
    "construction_plan": [
        {"id": "c1", "order": 1, "building": "supply_node", "state": 1, "province": 11, "when": "1936.01.01"},
        {"id": "c2", "order": 2, "building": "rail_way", "from_state": 1, "to_state": 2, "path": [10, 11, 20],
         "when": "province_controlled:20"},
        {"id": "c3", "order": 3, "building": "infrastructure", "state": 2, "when": "owner:3=AAA"},
        {"id": "c4", "order": 4, "building": "infrastructure", "state": 1, "when": "state_fully_controlled:3"},
    ],
    "vp_targets": [{"province": 10, "state": 1, "vp": 5, "army": "North"}, {"province": 30, "state": 3}],
    "army_layout": [
        {"name_hint": "North", "state": 1, "templates": ["Line"], "count": 2, "role": "strike"},
        {"name_hint": "North guard", "state": 1, "templates": ["Guard"], "count": 1, "role": "guard",
         "guard_states_in_order": [1]},
    ],
    "war": {"enemies": ["BBB"], "axes": [[3]], "theatre": [1, 2], "guard_states": [2], "guards": {"Army 2": 2}},
    "doctrines": [
        {"id": "grand", "folder": "land", "grand_doctrine": "grand_x", "xp": 100},
        {"id": "inf", "folder": "land", "track": "inf", "subdoctrine": "sub_inf", "xp": 50},
    ],
    "event_rules": [{"event": "aaa.1", "option_name": "aaa.1.b", "condition": "province_controlled:10", "else_option_name": "aaa.1.a"}],
}


def write_bmp(path):
    width, height = len(ROWS[0]), len(ROWS)
    stride = (width * 3 + 3) & ~3
    pixels = b""
    for row in reversed(ROWS):  # bottom-up
        line = b"".join(bytes((COLOURS[p][2], COLOURS[p][1], COLOURS[p][0])) for p in row)
        pixels += line + b"\0" * (stride - len(line))
    header = struct.pack("<2sIHHI", b"BM", 54 + len(pixels), 0, 0, 54)
    info = struct.pack("<IiiHHIIiiII", 40, width, height, 1, 24, 0, len(pixels), 2835, 2835, 0, 0)
    with open(path, "wb") as fh:
        fh.write(header + info + pixels)


class CoverageTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.tmp = tempfile.TemporaryDirectory()
        for rel, text in FILES.items():
            p = os.path.join(cls.tmp.name, *rel.split("/"))
            os.makedirs(os.path.dirname(p), exist_ok=True)
            with open(p, "w", encoding="utf-8") as fh:
                fh.write(text)
        write_bmp(os.path.join(cls.tmp.name, "map", "provinces.bmp"))
        cls.game = vp.Game(cls.tmp.name)

    @classmethod
    def tearDownClass(cls):
        cls.tmp.cleanup()

    def run_plan(self, **changes):
        plan = copy.deepcopy(GOOD)
        plan.update(changes)
        rep, counts = vp.validate(plan, self.game)
        return rep

    def errors(self, **changes):
        return "\n".join(self.run_plan(**changes).errors)

    def test_clean_plan(self):
        rep = self.run_plan()
        self.assertEqual(rep.errors, [])
        self.assertEqual(rep.warnings, [])

    def test_adjacency_from_the_bitmap(self):
        n = self.game.neighbours(straits=False)
        self.assertEqual(n[10], {11})
        self.assertEqual(n[11], {10, 20, 21})
        self.assertEqual(n[30], {20, 21})

    def test_shapes(self):
        text = self.errors(research_path={"year": 1936}, army_layout=[{"name_hint": "x", "state": "1"}],
                           war={"axes": [3]}, doctrines=[{"id": "d", "folder": "land", "xp": "100"}])
        self.assertIn("schema: research_path: expected array, got object", text)
        self.assertIn("schema: army_layout[0].state: expected integer, got string", text)
        self.assertIn("schema: army_layout[0]: missing required key 'templates'", text)
        self.assertIn("schema: war.axes[0]: expected array, got integer", text)
        self.assertIn("schema: doctrines[0].xp: expected number, got string", text)

    def test_research_path_readings_and_years(self):
        # t_ab after t_a (start) only: the ANY reading allows it, ALL does not -> warning naming it
        rep = self.run_plan(research_path=[{"year": 1936, "techs": ["t_ab"]}])
        self.assertEqual(rep.errors, [])
        self.assertTrue(any("t_ab" in w and "ALL" in w and "t_b" in w for w in rep.warnings))
        self.assertTrue(vp.trigger_one("always", "yes", vp.Ctx("AAA", [])))
        rep = self.run_plan(research_path=[{"year": 1936, "not_before": "1936-13-01",
                                            "techs": ["t_late", "nope", "t_b", "t_b"]}])
        text = "\n".join(rep.errors)
        self.assertIn("unknown technology nope", text)
        self.assertIn("duplicate tech t_b", text)
        self.assertIn("not_before", text)
        self.assertTrue(any("t_late" in w and "ahead-of-time" in w for w in rep.warnings))

    def test_construction_plan(self):
        bad = copy.deepcopy(GOOD["construction_plan"])
        bad[0]["province"] = 20
        bad[1]["path"] = [10, 20]
        bad[2]["when"] = "owner:3=ZZZ"
        bad[3]["when"] = "province_controlled:30"
        bad.append({"id": "c1", "building": "bunker", "state": 9, "when": "province_controlled:10"})
        bad.append({"id": "c9", "building": "infrastructure", "state": 1, "when": "province_controlled:x"})
        text = self.errors(construction_plan=bad)
        self.assertIn("province 20 is in state 2, not 1", text)
        self.assertIn("path starts at 10 in state 1", text.replace("\n", " ") + " path starts at 10 in state 1")
        self.assertIn("path provinces 10 and 20 do not touch", text)
        self.assertIn("ZZZ is not a country tag", text)
        self.assertIn("province_controlled:30 is in state 3, but the executor looks for it in the item's state 1",
                      text)
        self.assertIn("unknown building bunker", text)
        self.assertIn("state 9 is not a state id", text)
        self.assertIn('schema: construction_plan[5].when: "province_controlled:x" does not match', text)
        self.assertIn("id c1 is used by entries", text)

    def test_railway_ends_must_be_in_their_states(self):
        bad = copy.deepcopy(GOOD["construction_plan"])
        bad[1]["path"] = [11, 21]
        bad[1]["from_state"] = 2
        text = self.errors(construction_plan=bad)
        self.assertIn("path starts at 11 in state 1, not from_state 2", text)

    def test_vp_targets(self):
        text = self.errors(vp_targets=[{"province": 10, "state": 1, "vp": 3}, {"province": 11, "state": 2},
                                       {"province": 99, "state": 1}])
        self.assertIn("vp 3 but the state history gives 5", text)
        self.assertIn("province 11 is in state 1, not 2", text)
        self.assertIn("province 99 is not in any state", text)

    def test_army_layout(self):
        rep = self.run_plan(army_layout=[
            {"name_hint": "A", "state": 2, "templates": ["Line"], "role": "strike"},
            {"name_hint": "B", "state": 1, "templates": ["Elite"], "count": 1, "role": "guard",
             "guard_states_in_order": [1]},
            {"name_hint": "C", "state": 1, "templates": ["Guard"], "count": 3, "role": "guard",
             "guard_states_in_order": [1]},
            {"name_hint": "F", "state": 1, "templates": ["Guard"], "count": 1, "role": "hold"},
            {"name_hint": "D", "state": 2, "templates": ["Guard"], "count": 1, "role": "guard",
             "guard_states_in_order": [77]},
            {"name_hint": "E", "state": 3, "templates": ["Elite"], "role": "guard", "when": "owner:3=AAA",
             "guard_states_in_order": [3]},
        ])
        text = "\n".join(rep.errors)
        self.assertIn("template 'Elite' is not in the start OOB AAA_1936", text)
        self.assertIn('schema: army_layout[3].role: "hold" is not one of ["strike", "guard"]', text)
        self.assertIn("no division of Guard starts in state 2", text)
        self.assertIn("guard_states_in_order 77 is not a state id", text)
        self.assertNotIn("'E'", text)  # a when-entry only warns about its template
        self.assertTrue(any("'C'" in w and "count 3 but only 1" in w for w in rep.warnings))
        self.assertTrue(any("'E'" in w and "Elite" in w for w in rep.warnings))

    def test_war_ids(self):
        text = self.errors(war={"enemies": ["QQQ"], "axes": [[3, 404]], "theatre": [505], "guard_states": [606],
                                "guards": {"Army 1": 707}},
                           war_by_phase={"P1": {"axes": [[808]]}})
        for s in ("QQQ is not a country tag", "axes[0]: 404", "theatre: 505", "guard_states: 606",
                  "guards[Army 1]: 707", "war_by_phase.P1.axes[0]: 808"):
            self.assertIn(s, text)

    def test_doctrines(self):
        text = self.errors(doctrines=[
            {"folder": "land", "track": "inf", "subdoctrine": "sub_inf", "when": "army_xp >= 100"},
            {"id": "a", "folder": "air", "grand_doctrine": "grand_x", "xp": 100},
            {"id": "b", "folder": "land", "subdoctrine": "sub_inf", "xp": 50},
            {"id": "c", "folder": "land", "track": "other", "subdoctrine": "sub_inf", "xp": 50},
            {"id": "c", "folder": "land", "grand_doctrine": "nope", "xp": 50},
            {"id": "e", "folder": "land", "xp": 50},
        ])
        self.assertIn("old shape with 'when'", text)
        self.assertIn("unknown doctrine folder air", text)
        self.assertIn("grand doctrine grand_x is in folder land, not air", text)
        self.assertIn("subdoctrine sub_inf needs its track (inf)", text)
        self.assertIn("is on track inf, not other", text)
        self.assertIn("duplicate id", text)
        self.assertIn("unknown grand doctrine nope", text)
        self.assertIn("give grand_doctrine or subdoctrine", text)
        rep = self.run_plan(doctrines=[{"id": "both", "folder": "land", "grand_doctrine": "grand_x",
                                        "track": "inf", "subdoctrine": "sub_inf", "xp": 100}])
        self.assertEqual(rep.errors, [])
        rep = self.run_plan(doctrines=[{"id": "x", "folder": "land", "grand_doctrine": "grand_x", "xp": 20}])
        self.assertTrue(any("below the base cost 100" in w for w in rep.warnings))

    def test_event_rules(self):
        text = self.errors(event_rules=[{"event": "aaa.1", "option_name": "aaa.1.z"},
                                        {"event": "aaa.9", "option_name": "aaa.9.a"},
                                        {"event": "aaa.1", "option_name": "aaa.1.a",
                                         "fallback_option_name": "aaa.1.q"}])
        self.assertIn("option_name aaa.1.z is not an option of the event (has aaa.1.a, aaa.1.b)", text)
        self.assertIn("aaa.9: no event with this id", text)
        self.assertIn("fallback_option_name aaa.1.q", text)

    # ------------------------------------------------------------------ coverage-build keys (W1-2)
    def new(self, **changes):
        plan = copy.deepcopy(NEW_KEYS)
        plan.update(changes)
        return self.run_plan(**plan)

    def test_new_keys_clean(self):
        rep = self.new()
        self.assertEqual(rep.errors, [])
        self.assertEqual(rep.warnings, [])

    def test_predicates_parse_and_name_known_things(self):
        rep = self.new(phases=[{"id": "P1", "enter": "stability >= 150",
                                "exit": {"all": ["state_fully_controlled:9", "phase:P9", "at_war:QQQ"]}}],
                       contingencies=[{"id": "c", "trigger": "province_controlled:99", "phase": "P1"}],
                       goal={"predicates": ["owner:3=ZZZ", "tech_researched:t_none", "stockpile_days:nothing < 3",
                                            "mission_days:no_mission < 5", "xp:sea > 3", "focus_done:no_focus"]})
        text = "\n".join(rep.errors)
        self.assertIn("predicate phases[0].enter: 'stability >= 150': stability as a percent runs from 0 to 100", text)
        self.assertIn("phases[0].exit.all[0]: 'state_fully_controlled:9': 9 is not a state id", text)
        self.assertIn("no phases[] entry with id P9", text)
        self.assertIn("QQQ is not a country tag", text)
        self.assertIn("contingencies[0].trigger: 'province_controlled:99': province 99 is in no state", text)
        self.assertIn("ZZZ is not a country tag", text)
        self.assertIn("no technology t_none", text)
        self.assertIn("no equipment nothing", text)
        self.assertIn("no decision or mission no_mission", text)
        self.assertIn("'xp:sea > 3': 'sea' is not a branch", text)

    def test_templates(self):
        bad = [
            {"id": "a", "name": "A", "regiments": [["infantry", "cavalry"], ["engineer"]], "support": ["infantry", "nope"],
             "regimental_support": ["engineer", "fire_support"], "role": "line"},
            {"id": "a", "name": "Wide", "regiments": [["infantry"] * 5] * 5, "support": [], "role": "line"},
            {"id": "c", "name": "Mixed", "regiments": [["infantry"], "cavalry"], "role": "line"},
        ]
        text = "\n".join(self.new(templates=bad, xp_budget={}, training=[]).errors)
        self.assertIn("regiments[0] mixes groups infantry, mobile", text)
        self.assertIn("regiments[1] engineer is a support company", text)
        self.assertIn("support infantry is a battalion", text)
        self.assertIn("support nope is not a sub-unit", text)
        self.assertIn("regimental_support[0] engineer is not a regimental support battalion", text)
        self.assertIn("regimental_support[0] needs 3 battalions in regiments[0]", text)
        self.assertIn("regimental_support[1] needs 3 battalions in regiments[1]", text)
        self.assertIn("templates[1] a: duplicate id a", text)
        self.assertIn("combat width 50 is above 40", text)
        self.assertIn("not a mix", text)

    def test_production_remove_replace_and_training_list(self):
        text = self.errors(production=[{"equipment": "gun_1", "factories": 3},
                                       {"equipment": "gun_1", "remove": True, "from": "1936.6.1"},
                                       {"equipment": "gun_1", "replace": "gun_9"},
                                       {"equipment": "gun", "remove": True, "replace": "gun_1"}],
                           training=[{"template": "Line"}, {"template": "Guard", "deploy_state": 99}])
        self.assertNotIn("duplicate key gun_1", text)
        self.assertNotIn("factories must be", text)
        self.assertIn("replace gun_9 is not an equipment key", text)
        self.assertIn("give remove or replace, not both", text)
        self.assertIn("training[0].deploy_state is required", text)
        self.assertIn("training[1].deploy_state: unknown state id 99", text)

    def test_new_key_references(self):
        rep = self.new(stockpile_targets={"rifles": 30},
                       xp_budget={"army": [{"id": "x", "cost": 5, "purpose": "template", "ref": "template:nope"},
                                           {"id": "y", "cost": 5, "purpose": "doctrine", "ref": "doctrine:inf"}]},
                       force_posture={"reason": "r", "phases": [{"phase": "P7", "land": 50, "air": 20, "navy": 0}]},
                       occupation={"default_law": "gentle", "ladder": ["no_garrison"], "mildest": "military_governor_occupation"},
                       peace={"take": [3, 77, "BBB"], "puppet": ["BBB"]},
                       sea_transfer={"ports": [88], "routes": [{"id": "r", "from_state": 1, "to_state": 66}]},
                       garrison_template={"template": "Nobody"},
                       event_rules=[{"event": "aaa.1", "option_name": "aaa.1.b", "fallback_index": 5}])
        text = "\n".join(rep.errors)
        self.assertIn("fallback_index 5 but the event has 2 options", text)
        self.assertIn("stockpile_targets.rifles: no equipment rifles", text)
        self.assertIn("ref template:nope names no templates[] id", text)
        self.assertNotIn("doctrine:inf", text)  # GOOD has a doctrines entry with id inf
        self.assertIn("phase P7 is neither base nor a phases[] id", text)
        self.assertIn("land + air + navy is 70, not 100", text)
        self.assertIn("occupation.default_law: no occupation law gentle", text)
        self.assertIn("occupation.mildest: military_governor_occupation is not on occupation.ladder", text)
        self.assertIn("peace.take[1]: 77 is not a state id", text)
        self.assertIn("peace.take[2]: BBB is a tag; the executor takes state ids", text)
        self.assertIn("sea_transfer.routes[0].to_state: 66", text)
        self.assertIn("sea_transfer.ports: 88", text)
        self.assertTrue(any("peace.puppet[0]: tag BBB is only noted" in w for w in rep.warnings))
        self.assertTrue(any("garrison_template.template 'Nobody'" in w for w in rep.warnings))

    def test_when_parser(self):
        self.assertEqual(vp.parse_when(""), ("now",))
        self.assertEqual(vp.parse_when("1936.01.01"), ("date", (1936, 1, 1)))
        self.assertEqual(vp.parse_when("1936-1-1"), ("date", (1936, 1, 1)))
        self.assertEqual(vp.parse_when("date:1936.2.1")[0], "date_prefix")
        self.assertEqual(vp.parse_when("owner:908=ITA"), ("owner", 908, "ITA"))
        self.assertEqual(vp.parse_when("province_controlled: 13238"), ("province", 13238))
        self.assertEqual(vp.parse_when("owner:908"), ("bad", "'owner:908': owner: needs <state id>=<TAG>"))
        self.assertEqual(vp.parse_when("soon")[0], "text")


if __name__ == "__main__":
    unittest.main()
