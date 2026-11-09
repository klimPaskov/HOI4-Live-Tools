"""scripts/hoi4facts: mod layering, history by date, tag globbing, tech readings.

Fixture tests/fixtures/mods (no game files needed):
  game/   a tiny install: tags AAA and BBB, states 1 and 2, technologies, ideas, events
  user/   dlc_load.json enables mod/fixture.mod only (mod/disabled.mod is not enabled)
          mod/fixture overrides common/technologies/extra.txt and events/aaa.txt, hides
          history/states and common/ideas with replace_path (not common/ideas/nested), adds the
          tag NEW with its history file, state 3 and a new events file.
The last class checks the installed game and skips when it is absent.
"""

import json
import os
import sys
import unittest

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.join(ROOT, "scripts"))

from hoi4facts import content, script  # noqa: E402
from hoi4facts import tech as techrule  # noqa: E402
from hoi4facts.game import Game  # noqa: E402

FIX = os.path.join(ROOT, "tests", "fixtures", "mods")
GAME = os.path.join(FIX, "game")
USER = os.path.join(FIX, "user")
INSTALL = os.path.expanduser("~/Library/Application Support/Steam/steamapps/common/Hearts of Iron IV")
PLANS = os.path.join(ROOT, "skills", "hoi4-campaign-playbook", "plans")


class DescriptorTests(unittest.TestCase):
    def test_descriptor_fields(self):
        text = ('name="Big Map"\nreplace_path="history/states"\nreplace_path="map/strategicregions/"\n'
                'path="/mods/bigmap" # comment\n')
        m = content.parse_descriptor(text, "/user")
        self.assertEqual(m.path, os.path.normpath("/mods/bigmap"))
        self.assertEqual(m.replace_paths, ["history/states", "map/strategicregions"])
        self.assertEqual(m.name, "Big Map")
        rel = content.parse_descriptor('archive="mod/x.zip"', "/user")
        self.assertEqual(rel.path, os.path.normpath("/user/mod/x.zip"))
        self.assertIsNone(content.parse_descriptor('name="no path"', "/user"))

    def test_enabled_mods(self):
        self.assertEqual(
            content.parse_enabled_mods('{"enabled_mods":["mod/ugc_1.mod","mod/b.mod"],"disabled_dlcs":[]}'),
            ["mod/ugc_1.mod", "mod/b.mod"],
        )
        self.assertEqual(content.parse_enabled_mods("not json"), [])

    def test_user_folder_gives_only_enabled_mods_in_order(self):
        mods = content.mods_from_userdir(USER)
        self.assertEqual([m.name for m in mods], ["Fixture Mod"])
        self.assertEqual(mods[0].path, os.path.join(USER, "mod", "fixture"))
        self.assertEqual(mods[0].replace_paths, ["history/states", "common/ideas"])
        self.assertEqual(content.mods_from_userdir(os.path.join(FIX, "missing")), [])

    def test_mod_folder_argument_reads_its_descriptor(self):
        m = content.mod_from_arg(os.path.join(USER, "mod", "fixture"))
        self.assertEqual(m.name, "Fixture Mod")
        self.assertEqual(m.replace_paths, ["history/states"])


class ContentTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.c = content.Content.from_userdir(GAME, USER)

    def names(self, folder, **kw):
        return [os.path.relpath(p, FIX).replace(os.sep, "/") for p in self.c.files(folder, **kw)]

    def test_a_mod_file_overrides_the_same_relative_path(self):
        self.assertEqual(self.names("common/technologies"),
                         ["game/common/technologies/base.txt", "user/mod/fixture/common/technologies/extra.txt"])
        self.assertEqual(self.c.source(self.c.file("common/technologies/extra.txt")), "Fixture Mod")
        self.assertIsNone(self.c.source(self.c.file("common/technologies/base.txt")))

    def test_replace_path_hides_the_folder_but_not_its_sub_folders(self):
        self.assertEqual(self.names("history/states"), ["user/mod/fixture/history/states/3-Three.txt"])
        self.assertEqual(self.names("common/ideas", recursive=True),
                         ["user/mod/fixture/common/ideas/mod_laws.txt", "game/common/ideas/nested/keep.txt"])

    def test_events_are_layered(self):
        self.assertEqual(self.names("events"),
                         ["user/mod/fixture/events/aaa.txt", "user/mod/fixture/events/new_events.txt"])

    def test_disabled_mod_is_not_read(self):
        self.assertNotIn("disabled", " ".join(self.names("common/technologies")))

    def test_without_mods_the_install_is_read(self):
        plain = content.Content(GAME)
        self.assertEqual(len(plain.files("history/states")), 2)
        self.assertEqual(content.effective_files("history/states", GAME, content.mods_from_userdir(USER)),
                         self.c.files("history/states"))

    def test_glob_and_case(self):
        self.assertEqual(len(self.c.glob("history/countries", "new - *.txt")), 1)
        self.assertIsNotNone(self.c.file("History/Countries/AAA - Alpha.txt"))


class GameTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.g = Game(GAME, content.mods_from_userdir(USER))
        cls.plain = Game(GAME)

    def test_new_tag_and_history_file_by_glob(self):
        self.assertEqual(sorted(self.g.country_tags()), ["AAA", "BBB", "NEW"])
        self.assertEqual(self.g.history_file("NEW"), "history/countries/NEW - Newland.txt")
        self.assertEqual(self.g.history_file("AAA"), "history/countries/AAA - Alpha.txt")
        self.assertIsNone(self.g.history_file("ZZZ"))
        hist = self.g.history_effects(self.g.history_file("NEW"), script.Ctx("NEW", []))
        self.assertEqual(hist["techs"], {"t_a", "t_b"})
        self.assertEqual(hist["capital"], 3)

    def test_mod_technologies_and_states(self):
        techs = self.g.techs()
        self.assertEqual(techs["t_x"]["year"], 1940)
        self.assertIn("t_mod", techs)
        self.assertEqual(sorted(self.g.states()), [3])
        self.assertEqual(sorted(self.plain.states()), [1, 2])
        self.assertEqual(self.g.province_state()[31], 3)

    def test_dated_history_blocks_apply_up_to_the_start_date(self):
        hist = "history/countries/AAA - Alpha.txt"
        ctx = script.Ctx("AAA", [])
        start = self.plain.history_effects(hist, ctx, "1936.1.1")
        self.assertEqual(start["techs"], {"t_a"})
        self.assertEqual(start["oobs"], ["AAA_1936"])
        self.assertEqual((start["capital"], start["research_slots"]), (1, 3))
        summer = self.plain.history_effects(hist, ctx, "1936.6.1")
        self.assertEqual(summer["techs"], {"t_a", "t_b"})
        later = self.plain.history_effects(hist, ctx, "1939.1.1")
        self.assertEqual(later["oobs"], ["AAA_1936", "AAA_1939"])
        self.assertEqual(later["capital"], 2)
        dlc = self.plain.history_effects(hist, script.Ctx("AAA", ["Fixture DLC"]), "1936.1.1")
        self.assertEqual(dlc["oobs"], ["AAA_dlc"])
        # without DLC information every branch is read, still by date
        self.assertEqual(self.plain.start_techs(hist), {"t_a"})
        self.assertEqual(self.plain.start_techs(hist, "1937.1.1"), {"t_a", "t_b"})

    def test_state_owner_and_victory_points_by_date(self):
        ctx = script.Ctx("AAA", [])
        self.assertEqual(self.plain.state_owners(ctx, "1936.1.1")[1], "AAA")
        self.assertEqual(self.plain.state_owners(ctx, "1939.1.1")[1], "BBB")
        self.assertEqual(self.plain.state_facts(ctx)[1]["vps"], {10: 5.0})
        self.assertEqual(self.g.state_facts(None)[3]["vps"], {30: 2.0, 31: 1.0})

    def test_events_from_the_mod(self):
        self.assertEqual(self.g.event("aaa.1")["options"], ["aaa.1.mod"])
        self.assertEqual(self.plain.event("aaa.1")["options"], ["aaa.1.a", "aaa.1.b"])
        self.assertEqual(self.g.event("new.1")["type"], "news_event")
        self.assertIsNone(self.g.event("aaa.2"))


class TechReadingTests(unittest.TestCase):
    def setUp(self):
        self.techs = Game(GAME).techs()

    def test_multi_parent_readings(self):
        t = self.techs["t_ab"]
        self.assertEqual(t["parents"], {"t_a", "t_b"})
        self.assertTrue(techrule.eligible(t, {"t_a"}, rule="any"))
        self.assertFalse(techrule.eligible(t, {"t_a"}, rule="all"))
        self.assertTrue(techrule.eligible("t_ab", {"t_a", "t_b"}, rule="all", techs=self.techs))
        self.assertEqual(techrule.readings(t, {"t_a"}), {"all": ["t_b"], "any": []})
        self.assertEqual(techrule.missing(t, set(), "any"), ["t_a", "t_b"])
        self.assertEqual(techrule.multi_parent(self.techs), ["t_ab"])
        with self.assertRaises(ValueError):
            techrule.eligible(t, set(), rule="some")

    def test_dependencies_are_needed_under_both_readings(self):
        t = {"parents": set(), "deps": {"d1", "d2"}}
        self.assertFalse(techrule.eligible(t, {"d1"}, "any"))
        self.assertTrue(techrule.eligible(t, {"d1", "d2"}, "all"))


@unittest.skipUnless(os.path.isdir(os.path.join(INSTALL, "common")), "HOI4 game files not installed")
class InstalledGameTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.g = Game(INSTALL)

    def test_history_file_by_tag(self):
        self.assertEqual(self.g.history_file("ITA"), "history/countries/ITA - Italy.txt")
        self.assertEqual(self.g.history_file("ETH"), "history/countries/ETH - Ethiopia.txt")

    def test_some_techs_have_several_parents(self):
        multi = techrule.multi_parent(self.g.techs())
        self.assertIn("improved_medium_tank", multi)

    def test_hand_plans_validate(self):
        from hoi4facts.validate import validate
        for name in ("italy-ethiopia-1936", "ethiopia-italy-1936"):
            with open(os.path.join(PLANS, name + ".plan.json"), encoding="utf-8") as fh:
                plan = json.load(fh)
            rep, counts = validate(plan, self.g)
            self.assertEqual(rep.errors, [], name)
            for key in ("research_path", "construction_plan", "vp_targets", "army_layout", "doctrines",
                        "event_rules"):
                self.assertGreater(counts.get(key, 0), 0, f"{name}: {key} not checked")


if __name__ == "__main__":
    unittest.main()
