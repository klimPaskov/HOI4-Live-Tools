"""The save model (state/model.py), the merge helpers and the briefing builder (state/briefing.py), without the game."""

import os
import shutil
import tempfile
import unittest

from helpers import FIXTURES

from hoi4_live.state import assemble, briefing, model
from hoi4_live.state.savefile import SaveFile

MODEL_SAVE = os.path.join(FIXTURES, "saves", "model_text.txt")
NAMES = {"USA": "America", "USA_democratic": "United States", "STATE_361": "Maryland", "STATE_375": "Texas",
         "USA_great_depression": "Great Depression", "free_trade": "Free Trade", "gwtank": "Great War Tank",
         "basic_machine_tools": "Basic Machine Tools", "infantry_equipment_1": "Infantry Equipment I",
         "MEX": "Mexico", "industrial_complex": "Civilian Factory"}
SLOTS = {"free_trade": "trade_laws", "disarmed_nation": "mobilization_laws", "great_depression": "country",
         "undisturbed_isolation": "economy"}


def names(keys):
    return {k: NAMES[k] for k in keys if k in NAMES}


class SaveModelTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.tmp = tempfile.mkdtemp(prefix="hoi4live_model_")
        cls.path = os.path.join(cls.tmp, "model_text.txt")
        shutil.copy(MODEL_SAVE, cls.path)
        cls.sf = SaveFile(cls.path, use_cache=False)
        cls.st = model.country_state(cls.sf, "USA", model.SECTIONS, names=names, idea_slots=SLOTS,
                                     tech_costs={"basic_machine_tools": 1.5})

    @classmethod
    def tearDownClass(cls):
        cls.sf.close()
        shutil.rmtree(cls.tmp, ignore_errors=True)

    def test_overview_and_politics(self):
        ov = self.st["overview"]
        self.assertEqual(ov["name"], "United States")
        self.assertEqual(ov["capital_name"], "Maryland")
        self.assertEqual(ov["political_power"], 42.5)
        pol = self.st["politics"]
        self.assertEqual(pol["popularity"], {"democratic": 90, "communism": 10})
        self.assertEqual(pol["laws"]["trade_laws"]["name"], "Free Trade")
        self.assertEqual(pol["laws"]["economy"]["id"], "undisturbed_isolation")
        self.assertEqual([s["id"] for s in pol["spirits"]], ["great_depression"])

    def test_focus_and_research(self):
        f = self.st["focus"]
        self.assertEqual((f["current"], f["current_name"], f["progress_days"]), ("USA_great_depression", "Great Depression", 12))
        r = self.st["research"]
        self.assertEqual(r["idle_slots"], 2)
        self.assertEqual(r["researching"][0]["tech"], "basic_machine_tools")
        self.assertEqual(r["researching"][0]["approx_progress"], 0.2)
        # starting techs (dated on the start day) are not "recent"; gwtank was researched in game
        self.assertEqual([t["tech"] for t in r["recently_completed"]], ["gwtank"])

    def test_production_and_construction(self):
        p = self.st["production"]
        self.assertEqual([l["equipment"] for l in p["lines"]], ["infantry_equipment_1", "ship_hull_cruiser_1"])
        self.assertEqual(p["military_factories_assigned"], 3)
        self.assertEqual(p["dockyards_assigned"], 5)
        self.assertEqual(p["short_of_resources"], ["chromium"])
        c = self.st["construction"]["queue"][0]
        self.assertEqual((c["building"], c["state_name"], c["factories"]), ("industrial_complex", "Texas", 15))
        self.assertAlmostEqual(c["progress"], 0.144, places=3)

    def test_military_diplomacy_decisions_states(self):
        m = self.st["military"]
        self.assertEqual((m["divisions"], m["avg_strength"], m["convoys_total"]), (2, 225.0, 700))
        self.assertEqual(m["stockpile"][0], {"equipment": "infantry_equipment_1", "name": "Infantry Equipment I",
                                             "amount": 500})
        d = self.st["diplomacy"]
        self.assertTrue(d["at_war"])
        w = d["wars"][0]
        self.assertEqual((w["enemy"], w["enemy_name"], w["we_started"]), ("MEX", "Mexico", True))
        self.assertEqual((w["our_casualties"], w["score_breakdown_us"]), (120, 12.5))
        self.assertEqual(d["relations"][0]["type"], "puppet")
        self.assertEqual(d["faction"]["members"], ["USA", "PHI"])
        dec = {x["id"]: x for x in self.st["decisions"]["active"]}
        self.assertEqual(dec["USA_some_mission"]["days_left"], 12)
        st = self.st["states"]
        self.assertEqual(st["owned"], 2)
        self.assertEqual(st["totals"], {"civ": 8, "mil": 2, "dock": 1})

    def test_unknown_country(self):
        self.assertIn("error", model.country_state(self.sf, "XXX", ["overview"]))


def sample_model(at_war=False):
    m = {
        "tag": "USA", "name": "United States", "date": "1936.1.8.8",
        "sources": {"telemetry": {"as_of": "1936.1.7.23", "age_days": 0.38},
                    "save": {"as_of": "1936.1.1.12", "age_days": 6.83, "file": "autosave.hoi4"},
                    "screen": {"as_of": "1936.1.8.8", "paused": True, "speed": 3, "popups": 1}},
        "overview": {"ruling_party": "Democratic Party", "ideology": "democratic", "party_popularity": 0.99,
                     "leader": "FDR", "political_power": 120.0, "political_power_daily": 1.21, "stability": 0.35,
                     "war_support": 0.05, "manpower": 259760, "world_tension": 0.0},
        "politics": {"days_to_election": 20, "laws": {"economy": {"name": "Civilian Economy"}},
                     "faction": {"name": "American Commonwealth", "leader": "USA", "members": 2}},
        "economy": {"factories": {"civilian": 156, "military": 10, "dockyards": 22, "civilian_free": 48,
                                  "military_free": 5, "dockyards_free": 0},
                    "resources": {"steel": {"surplus": 76.0}, "chromium": {"surplus": -4.0}},
                    "fuel": {"fuel": 71100, "capacity": 195500, "ratio": 0.36}, "convoys": 640,
                    "production_lines": [{"kind": "military", "name": "Infantry Equipment I", "factories": 2}],
                    "construction_queue": [], "construction_queued": 0},
        "military": {"divisions": 36, "ships": 230, "planes": 882, "manpower": 259760,
                     "equipment": {"infantry_equipment": {"stockpile": 0, "in_armies": 11061, "needed": 33120,
                                                          "deficit": 22059}}},
        "diplomacy": {"at_war": at_war, "enemies": 1 if at_war else 0, "_sources": {}},
        "focus": {"_sources": {"save": {}}, "current": None},
        "research": {"slots": 4, "researching": [], "idle_slots": 4, "researched": 56,
                     "_sources": {"save": {}}},
        "world": {"world_tension": 0.0, "countries_at_war": 2, "enemy_pairs": [["ETH", "ITA"]],
                  "countries": [{"c": "GER", "ideo": "fascist", "mil": 28, "div": 30, "major": 1, "enemies": 0}]},
    }
    if at_war:
        m["diplomacy"].update({"wars": [{"e": "MEX", "name": "Mexico", "div": 21, "sp": 0.0}],
                               "surrender_progress": 0.25, "front_states": 4,
                               "front": {"enemy": [{"s": 482, "name": "Chihuahua", "ctrl": "MEX"}], "own": []},
                               "lost_states": [{"s": 375, "name": "Texas", "ctrl": "MEX"}],
                               "recent_events": [{"k": "ev", "e": "declare_war", "a": "USA", "b": "MEX",
                                                  "_date": "1936.1.5.5"}]})
    return m


class BriefingTests(unittest.TestCase):
    def test_attention_items_in_priority_order(self):
        items = briefing.attention(sample_model())
        texts = [i["text"] for i in items]
        levels = [i["level"] for i in items]
        self.assertEqual(levels, sorted(levels, key={"now": 0, "soon": 1, "info": 2}.get))
        self.assertTrue(texts[0].startswith("1 popup(s) open"))
        joined = "\n".join(texts)
        for want in ("no national focus selected (save, 6.83 d old)", "4 research slot(s) idle",
                     "48 civilian factories free and nothing in construction", "5 military factories",
                     "political power 120", "infantry equipment 22.1K", "chromium -4", "stability is low (35%)",
                     "election in 20 days"):
            self.assertIn(want, joined)

    def test_markdown_sections(self):
        b = briefing.build(sample_model())
        md = b["markdown"]
        self.assertTrue(md.startswith("# United States (USA), 8 Jan 1936 08:00, paused, speed 3"))
        self.assertIn("Sources: telemetry 7 Jan 1936 23:00, 0.38 d old; save 1 Jan 1936 12:00, 6.83 d old "
                      "(autosave.hoi4); screen 8 Jan 1936 08:00", md)
        for header in ("## Needs attention", "## Situation", "## Economy", "## Military", "## Focus and research",
                       "## World"):
            self.assertIn(header, md)
        self.assertIn("- At peace; world tension 0%", md)
        self.assertIn("Faction: American Commonwealth (leader USA, 2 members)", md)
        self.assertIn("DEFICIT chromium -4", md)
        self.assertIn("Construction: nothing queued", md)
        self.assertLess(len(md.splitlines()), 60)                     # one page

    def test_war_briefing(self):
        b = briefing.build(sample_model(at_war=True))
        md = b["markdown"]
        self.assertIn("AT WAR with 1: Mexico (MEX: 21 div, surrender 0%)", md)
        self.assertIn("Front: 1 enemy-held states border ours: Chihuahua (MEX)", md)
        self.assertIn("5 Jan 1936: USA declared war on MEX", md)
        texts = [a["text"] for a in b["attention"]]
        self.assertTrue(any("surrender progress 25%" in t for t in texts))
        self.assertTrue(any("Texas (MEX)" in t for t in texts))

    def test_deltas_since_previous_briefing(self):
        prev = briefing.facts(sample_model())
        cur = sample_model(at_war=True)
        cur["date"] = "1936.2.1.12"
        cur["overview"]["political_power"] = 30.0
        cur["overview"]["stability"] = 0.6
        cur["economy"]["factories"]["civilian"] = 160
        b = briefing.build(cur, prev)
        texts = [d["text"] for d in b["deltas"]]
        self.assertIn("political power 120 -> 30 (-90)", texts)
        self.assertIn("stability 35.0% -> 60.0%", texts)
        self.assertIn("civilian factories 156 -> 160 (+4)", texts)
        self.assertIn("enemies 0 -> 1 (+1)", texts)
        self.assertIn("## Since the last briefing (8 Jan 1936 08:00)", b["markdown"])
        self.assertEqual(briefing.deltas(None, b["facts"]), [])

    def test_deltas_against_a_later_previous_briefing_are_skipped(self):
        # the previous briefing came from a timeline left by loading an older save: no deltas, a note instead
        later = sample_model(at_war=True)
        later["date"] = "1936.3.21.23"
        later["overview"]["political_power"] = 177.6
        prev = briefing.facts(later)
        cur = sample_model()
        cur["date"] = "1936.2.1.12"
        b = briefing.build(cur, prev)
        self.assertEqual(b["deltas"], [])
        self.assertIn("is from a later date than now", b["markdown"])
        self.assertNotIn("political power 177.6", b["markdown"].split("## Since the last briefing")[-1])

    def test_popups_from_an_abandoned_timeline_are_dropped(self):
        pops = [{"kind": "news", "title": "Old", "chosen_text": "OK", "date": "1936.1.4.12"},
                {"kind": "news", "title": "Future", "chosen_text": "OK", "date": "1936.5.1.2"}]
        md = briefing.build(sample_model(), popups=pops)["markdown"]
        self.assertIn("'Old'", md)
        self.assertNotIn("'Future'", md)

    def test_formatting(self):
        self.assertEqual(briefing.fmt_num(261620), "261.6K")
        self.assertEqual(briefing.fmt_num(1.5e6), "1.5M")
        self.assertEqual(briefing.fmt_num(13.38), "13.4")
        self.assertEqual(briefing.pct(0.948), "95%")
        self.assertEqual(briefing.fmt_date("1936.1.8.8"), "8 Jan 1936 08:00")
        self.assertEqual(briefing.describe_event({"e": "capitulation", "a": "FRA", "b": "GER"}), "FRA capitulated to GER")


class MergeTests(unittest.TestCase):
    def test_section_records_sources(self):
        s = assemble.Section()
        s.put("telemetry", "1936.1.7.23", political_power=8.5, stability=None)
        s.put("save", "1936.1.1.12", political_power=1.2, stability=0.75)
        out = s.finish()
        self.assertEqual(out["political_power"], 8.5)                 # the fresher source wins
        self.assertEqual(out["stability"], 0.75)                       # filled from the save
        self.assertEqual(out["_sources"]["telemetry"]["fields"], ["political_power"])
        self.assertEqual(out["_sources"]["save"], {"as_of": "1936.1.1.12", "fields": ["political_power", "stability"]})

    def test_days_between(self):
        self.assertEqual(assemble.days_between("1936.1.1.12", "1936.1.8.8"), 6.83)


if __name__ == "__main__":
    unittest.main()
