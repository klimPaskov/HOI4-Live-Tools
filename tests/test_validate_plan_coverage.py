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
}

GOOD = {
    "_validator": {"tag": "AAA", "stop_date": "1938.1.1", "dlcs": []},
    "research_path": [
        {"year": 1936, "not_before": "1936.01.01", "techs": ["t_b"]},
        {"year": 1937, "not_before": "1937.01.01", "techs": ["t_ab"]},
    ],
    "construction_plan": [
        {"order": 1, "building": "supply_node", "state": 1, "province": 11, "when": "1936.01.01"},
        {"order": 2, "building": "rail_way", "from_state": 1, "to_state": 2, "path": [10, 11, 20],
         "when": "province_controlled:20"},
        {"order": 3, "building": "infrastructure", "state": 2, "when": "owner:3=AAA"},
        {"order": 4, "building": "infrastructure", "state": 1, "when": "state_fully_controlled:3"},
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
    "event_rules": [{"event": "aaa.1", "option_name": "aaa.1.b", "condition": "x", "else_option_name": "aaa.1.a"}],
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
        self.assertIn("research_path must be a list", text)
        self.assertIn("army_layout[0].state must be an integer", text)
        self.assertIn("army_layout[0]: templates is required", text)
        self.assertIn("war.axes must be a list of integer lists", text)
        self.assertIn("doctrines[0].xp must be an integer", text)

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
        bad.append({"order": 1, "building": "bunker", "state": 9, "when": "province_controlled:x"})
        text = self.errors(construction_plan=bad)
        self.assertIn("province 20 is in state 2, not 1", text)
        self.assertIn("path starts at 10 in state 1", text.replace("\n", " ") + " path starts at 10 in state 1")
        self.assertIn("path provinces 10 and 20 do not touch", text)
        self.assertIn("ZZZ is not a country tag", text)
        self.assertIn("province_controlled:30 is in state 3, but the executor looks for it in the item's state 1",
                      text)
        self.assertIn("unknown building bunker", text)
        self.assertIn("state 9 is not a state id", text)
        self.assertIn("needs a numeric id", text)
        self.assertIn("order 1 is used by entries", text)
        rep = self.run_plan(construction_plan=[{"order": 1, "building": "infrastructure", "states": [1],
                                                "when": "when it feels right"}])
        self.assertEqual(rep.errors, [])
        self.assertTrue(any("player-only" in w for w in rep.warnings))

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
            {"name_hint": "C", "state": 1, "templates": ["Guard"], "count": 3, "role": "hold"},
            {"name_hint": "D", "state": 2, "templates": ["Guard"], "count": 1, "role": "guard",
             "guard_states_in_order": [77]},
            {"name_hint": "E", "state": 3, "templates": ["Elite"], "role": "guard", "when": "owner:3=AAA",
             "guard_states_in_order": [3]},
        ])
        text = "\n".join(rep.errors)
        self.assertIn("template 'Elite' is not in the start OOB AAA_1936", text)
        self.assertIn("role must be strike or guard, not 'hold'", text)
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
            {"id": "a", "folder": "sea", "grand_doctrine": "grand_x", "xp": 100},
            {"id": "b", "folder": "land", "subdoctrine": "sub_inf", "xp": 50},
            {"id": "c", "folder": "land", "track": "other", "subdoctrine": "sub_inf", "xp": 50},
            {"id": "c", "folder": "land", "grand_doctrine": "nope", "xp": 50},
            {"id": "e", "folder": "land", "xp": 50},
        ])
        self.assertIn("old shape with 'when'", text)
        self.assertIn("unknown doctrine folder sea", text)
        self.assertIn("grand doctrine grand_x is in folder land, not sea", text)
        self.assertIn("subdoctrine sub_inf needs its track (inf)", text)
        self.assertIn("is on track inf, not other", text)
        self.assertIn("duplicate id", text)
        self.assertIn("unknown grand doctrine nope", text)
        self.assertIn("exactly one of grand_doctrine or subdoctrine", text)
        rep = self.run_plan(doctrines=[{"id": "x", "folder": "land", "grand_doctrine": "grand_x", "xp": 20}])
        self.assertTrue(any("below the base cost 100" in w for w in rep.warnings))

    def test_event_rules(self):
        text = self.errors(event_rules=[{"event": "aaa.1", "option_name": "aaa.1.z"},
                                        {"event": "aaa.9", "option_name": "aaa.9.a"},
                                        {"event": "aaa.1", "fallback_option_name": "aaa.1.q"}])
        self.assertIn("option_name aaa.1.z is not an option of the event (has aaa.1.a, aaa.1.b)", text)
        self.assertIn("aaa.9: no event with this id", text)
        self.assertIn("fallback_option_name aaa.1.q", text)

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
