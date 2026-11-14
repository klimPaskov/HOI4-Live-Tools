"""Politics slot indexes: the `slot` of hoi4_politics_options / hoi4_politics_choose and of a plan's politics
entries is the position of the slot kind in its category (the slot lines of common/idea_tags, in order). The
reader, the facts CLI rows and the plan check on a fixture game in a temporary folder; one class on the
installed game when it is there."""
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
from hoi4facts import derive, dossier, facts, mapgraph  # noqa: E402
from hoi4facts.game import Game  # noqa: E402
from hoi4facts.world import World  # noqa: E402

REAL = os.environ.get("HOI4_DIR", mapgraph.DEFAULT_GAME_DIR)

FILES = {
    # slot and character_slot lines interleaved on purpose: the index follows the file order, not the kind
    "common/idea_tags/00_idea.txt": """
idea_categories = {
    hidden_ideas = { hidden = yes cost = 0 }
    country = { type = national_spirit cost = 0 }
    government = {
        slot = mobilization_laws
        slot = trade_laws
        slot = economy
        character_slot = political_advisor
        character_slot = political_advisor
        cost = 150
    }
    research_production = {
        slot = tank_manufacturer
        character_slot = theorist
        slot = industrial_concern
        cost = 150
    }
    military_staff = {
        character_slot = army_chief
        character_slot = navy_chief
        character_slot = air_chief
        character_slot = high_command
        character_slot = high_command
        character_slot = high_command
        cost = 150
    }
    academy_spirit = { type = army_spirit cost = 20 politics_tab = no slot = academy_spirit }
}
""",
    "common/ideas/laws.txt": """
ideas = {
    economy = { law = yes
        civilian_economy = { cost = 150 }
        war_economy = { cost = 150 }
    }
    trade_laws = { law = yes free_trade = { cost = 150 } }
    industrial_concern = { AAA_works = { cost = 150 } }
    country = { AAA_spirit = { } }
    academy_spirit = { AAA_academy = { } }
}
""",
    "common/characters/AAA.txt": """
characters = {
    AAA_chief = { name = AAA_chief advisor = { slot = army_chief idea_token = AAA_chief_token cost = 50 } }
    AAA_logistics = { name = AAA_logistics advisor = { slot = high_command idea_token = AAA_logistics_token cost = 50 } }
    AAA_admiral = { name = AAA_admiral advisor = { slot = navy_chief idea_token = AAA_admiral_token } }
    AAA_minister = { name = AAA_minister advisor = { slot = political_advisor idea_token = AAA_minister_token } }
    AAA_thinker = { name = AAA_thinker advisor = { slot = theorist idea_token = AAA_thinker_token } }
}
""",
    "history/countries/AAA - Testland.txt": "capital = 1\nrecruit_character = AAA_chief\n",
    "common/country_tags/00_countries.txt": 'AAA = "countries/Testland.txt"\n',
    "history/states/1-One.txt": 'state = { id = 1 name = "STATE_1" state_category = city history = { owner = AAA } }\n',
}

PLAN = {
    "_validator": {"tag": "AAA", "stop_date": "1938.1.1", "history_file": "history/countries/AAA - Testland.txt"},
    "politics": [
        {"id": "chief", "category": "military_staff", "slot": 0, "idea": "AAA_chief"},
        {"id": "logistics", "category": "military_staff", "slot": 3, "idea": "AAA_logistics"},
        {"id": "admiral", "category": "military_staff", "slot": 1, "idea": "AAA_admiral"},
        {"id": "thinker", "category": "research_production", "slot": 1, "idea": "AAA_thinker"},
        {"id": "works", "category": "research_production", "slot": 2, "idea": "AAA_works"},
        {"id": "war", "category": "government", "slot": 2, "idea": "war_economy"},
        {"id": "minister", "category": "government", "slot": 4, "idea": "AAA_minister_token"},
    ],
    "stop_on_date": "1938.01.01",
}


class Fixture(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.tmp = tempfile.TemporaryDirectory()
        for rel, text in FILES.items():
            path = os.path.join(cls.tmp.name, *rel.split("/"))
            os.makedirs(os.path.dirname(path), exist_ok=True)
            with open(path, "w", encoding="utf-8") as fh:
                fh.write(text)
        cls.game = Game(cls.tmp.name)

    @classmethod
    def tearDownClass(cls):
        cls.tmp.cleanup()

    def cli(self, argv):
        buf = io.StringIO()
        with contextlib.redirect_stdout(buf):
            rc = facts.main(argv + ["--game", self.tmp.name, "--dlc", "none", "--userdir", self.tmp.name,
                                    "--tag", "AAA"])
        return rc, buf.getvalue()

    def politics_errors(self, plan):
        rep, _ = vp.validate(plan, self.game)
        return [e for e in rep.errors if e.startswith(("politics[", "politics_auto"))]


class Reader(Fixture):
    def test_slot_indexes_follow_the_slot_lines_in_file_order(self):
        slots = derive.politics_slots(self.game)
        self.assertEqual(slots["military_staff"], ["army_chief", "navy_chief", "air_chief", "high_command",
                                                   "high_command", "high_command"])
        self.assertEqual(slots["government"], ["mobilization_laws", "trade_laws", "economy", "political_advisor",
                                               "political_advisor"])
        # a character slot between two idea slots keeps its place
        self.assertEqual(slots["research_production"], ["tank_manufacturer", "theorist", "industrial_concern"])

    def test_only_the_politics_screen_categories(self):
        self.assertEqual(set(derive.politics_slots(self.game)), {"government", "research_production",
                                                                 "military_staff"})

    def test_places_of_a_kind(self):
        self.assertEqual(derive.politics_places(self.game, "high_command"),
                         [{"category": "military_staff", "slots": [3, 4, 5]}])
        self.assertEqual(derive.politics_places(self.game, "economy"), [{"category": "government", "slots": [2]}])
        self.assertEqual(derive.politics_places(self.game, "academy_spirit"), [])

    def test_kind_of_ideas_characters_and_tokens(self):
        kinds = derive.politics_slot_kinds(self.game)
        self.assertEqual(kinds["AAA_logistics"], {"high_command"})
        self.assertEqual(kinds["AAA_logistics_token"], {"high_command"})
        self.assertEqual(kinds["war_economy"], {"economy"})
        self.assertEqual(kinds["AAA_works"], {"industrial_concern"})

    def test_layout_text_and_ranges(self):
        self.assertEqual(derive.politics_layout_text(self.game, "military_staff"),
                         "0 army_chief, 1 navy_chief, 2 air_chief, 3-5 high_command")
        self.assertEqual(derive.index_ranges([5, 0, 3, 4]), "0, 3-5")
        self.assertEqual(derive.politics_layout_text(self.game, "nothing"), "")

    def test_a_game_without_idea_categories_says_nothing(self):
        with tempfile.TemporaryDirectory() as d:
            empty = Game(d)
            self.assertEqual(derive.politics_slots(empty), {})
            self.assertIsNone(derive.politics_slot_problem(empty, None, "military_staff", 7, "AAA_logistics"))


class Cli(Fixture):
    def test_politics_slots_lists_every_slot_with_its_index(self):
        rc, out = self.cli(["politics-slots"])
        self.assertEqual(rc, 0)
        self.assertIn("military_staff: slots 0 army_chief, 1 navy_chief, 2 air_chief, 3-5 high_command", out)
        self.assertIn("  4 high_command (advisor", out)
        self.assertIn("  1 theorist (advisor", out)
        self.assertIn("  2 economy (idea", out)
        rc, out = self.cli(["politics-slots", "--json"])
        rows = {c["category"]: c for c in json.loads(out)}
        self.assertEqual([s["kind"] for s in rows["military_staff"]["slots"]][3:], ["high_command"] * 3)
        self.assertEqual(rows["research_production"]["slots"][1],
                         {"index": 1, "kind": "theorist", "character": True, "cost": 150})

    def test_advisor_rows_say_where_to_hire(self):
        rc, out = self.cli(["characters", "AAA", "--role", "advisor"])
        self.assertEqual(rc, 0)
        line = next(x for x in out.splitlines() if "AAA_logistics" in x)
        self.assertIn("slot high_command (hire: military_staff slot 3-5)", line)
        line = next(x for x in out.splitlines() if "AAA_chief" in x)
        self.assertIn("slot army_chief (hire: military_staff slot 0)", line)
        rc, out = self.cli(["characters", "AAA", "--role", "advisor", "--json"])
        role = next(c for c in json.loads(out) if c["id"] == "AAA_thinker")["roles"][0]
        self.assertEqual(role["politics_slots"], [{"category": "research_production", "slots": [1]}])

    def test_law_categories_and_ideas_say_their_slot(self):
        rc, out = self.cli(["laws", "AAA"])
        self.assertIn("economy (now None) [government slot 2]", out)
        self.assertIn("trade_laws (now None) [government slot 1]", out)
        rc, out = self.cli(["idea", "AAA_works"])
        self.assertIn("politics: research_production slot 2", out)
        rc, out = self.cli(["idea", "AAA_spirit"])  # a national spirit sits in no politics slot
        self.assertNotIn("politics:", out)

    def test_world_rows_carry_the_indexes(self):
        w = World(self.game, "AAA", "1936.1.1", (), "fair")
        self.assertEqual(w.politics_slots()[-1]["layout"], "0 army_chief, 1 navy_chief, 2 air_chief, 3-5 high_command")
        row = next(c for c in w.characters("AAA", "advisor") if c["id"] == "AAA_logistics")
        self.assertEqual(row["roles"][0]["politics_slots"], [{"category": "military_staff", "slots": [3, 4, 5]}])

    def test_dossier_politics_section_states_the_indexes(self):
        game = Game(os.path.join(ROOT, "tests", "fixtures", "dossier", "game"), cache_dir=tempfile.gettempdir())
        d = dossier.build_world(World(game, "AAA", "1936.1.1", (), "fair"))
        self.assertEqual([c["category"] for c in d["politics"]["slots"]], ["government", "military_staff"])
        md = dossier.section_text(dossier.render_md(d), "7")
        self.assertIn("military_staff 0 army_chief, 1 navy_chief, 2 air_chief, 3-5 high_command", md)
        self.assertIn("government 0 mobilization_laws, 1 trade_laws, 2 economy, 3-5 political_advisor", md)


class PlanCheck(Fixture):
    def test_picks_in_slots_of_their_kind_pass(self):
        self.assertEqual(self.politics_errors(copy.deepcopy(PLAN)), [])

    def test_high_command_in_the_army_chief_slot(self):
        plan = copy.deepcopy(PLAN)
        plan["politics"][1]["slot"] = 0
        errs = self.politics_errors(plan)
        self.assertEqual(len(errs), 1, errs)
        self.assertIn("politics[1]: AAA_logistics fits military_staff slot 3-5 (high_command), "
                      "not military_staff slot 0 (army_chief)", errs[0])

    def test_pick_in_another_category(self):
        plan = copy.deepcopy(PLAN)
        plan["politics"][5] = {"id": "war", "category": "military_staff", "slot": 2, "idea": "war_economy"}
        errs = self.politics_errors(plan)
        self.assertEqual(len(errs), 1, errs)
        self.assertIn("war_economy fits government slot 2 (economy), not military_staff slot 2 (air_chief)", errs[0])

    def test_slot_outside_the_category(self):
        plan = copy.deepcopy(PLAN)
        plan["politics"][1]["slot"] = 6
        errs = self.politics_errors(plan)
        self.assertEqual(len(errs), 1, errs)
        self.assertIn("politics[1]: slot 6 is outside military_staff (slots 0 army_chief, 1 navy_chief, 2 air_chief, "
                      "3-5 high_command)", errs[0])

    def test_advisor_token_is_checked_like_its_character(self):
        plan = copy.deepcopy(PLAN)
        plan["politics"][6]["slot"] = 2
        errs = self.politics_errors(plan)
        self.assertEqual(len(errs), 1, errs)
        self.assertIn("AAA_minister_token fits government slot 3-4 (political_advisor), not government slot 2 (economy)", errs[0])

    def test_standing_order_entries(self):
        plan = copy.deepcopy(PLAN)
        plan["politics_auto"] = {"order": [
            {"category": "military_staff", "slot": 4, "ideas": ["AAA_logistics"]},
            {"category": "military_staff", "slot": 1},
            {"category": "government", "slot": 2, "ideas": ["war_economy", "civilian_economy"]},
        ]}
        self.assertEqual(self.politics_errors(plan), [])
        plan["politics_auto"]["order"] = [
            {"category": "military_staff", "slot": 0, "ideas": ["AAA_chief", "AAA_logistics"]},
            {"category": "research_production", "slot": 3},
            {"category": "government", "slot": 2, "ideas": ["no_such_law"]},
        ]
        errs = self.politics_errors(plan)
        self.assertEqual(len(errs), 3, errs)
        self.assertIn("politics_auto.order[0]: AAA_logistics fits military_staff slot 3-5 (high_command)", errs[0])
        self.assertIn("politics_auto.order[1]: slot 3 is outside research_production", errs[1])
        self.assertIn("politics_auto.order[2]: unknown idea/character no_such_law", errs[2])


@unittest.skipUnless(os.path.isfile(os.path.join(REAL, "common", "idea_tags", "00_idea.txt")),
                     "HOI4 game folder not found")
class Installed(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.game = Game(REAL)

    def test_the_three_categories(self):
        slots = derive.politics_slots(self.game)
        self.assertEqual(slots["military_staff"], ["army_chief", "navy_chief", "air_chief"] + ["high_command"] * 3)
        self.assertEqual(slots["government"][:3], ["mobilization_laws", "trade_laws", "economy"])
        self.assertEqual(slots["government"][3:], ["political_advisor"] * 3)
        self.assertEqual(slots["research_production"][-2:], ["industrial_concern", "theorist"])
        self.assertNotIn("army_spirit", slots)

    def test_an_italian_high_command_advisor(self):
        w = World(self.game, "ITA", "1936.1.1", (), "fair")
        hc = [r for c in w.characters("ITA", "advisor") for r in c["roles"] if r.get("slot") == "high_command"]
        self.assertTrue(hc)
        for r in hc:
            self.assertEqual(r["politics_slots"], [{"category": "military_staff", "slots": [3, 4, 5]}])
        who = next(c["id"] for c in w.characters("ITA", "advisor")
                   if any(r.get("slot") == "high_command" for r in c["roles"]))
        msg = derive.politics_slot_problem(self.game, None, "military_staff", 0, who)
        self.assertIn("fits military_staff slot 3-5 (high_command), not military_staff slot 0 (army_chief)", msg)
        self.assertIsNone(derive.politics_slot_problem(self.game, None, "military_staff", 4, who))


if __name__ == "__main__":
    unittest.main()
