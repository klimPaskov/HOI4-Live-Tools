"""Sinks and slots (hoi4_live/observe/sinks.py, gamerules.py): every spendable or assignable thing
judged for "available and unused", the conscription answer from the files, the lane's own reason in
an escalation, lane reviews bundled inside the budget, and a refusal repeated with the same reason
turned into a lesson of the run."""
import json
import os
import tempfile
import unittest

from hoi4_live.observe import gamerules, sinks
from hoi4_live.observe import issue_rules as R
from hoi4_live.observe.escalation import Escalator
from hoi4_live.observe.issues import Detector
from hoi4_live.observe.lessons import LessonStore, lessons_from_issue

LAWS = """ideas = {
    mobilization_laws = {
        law = yes
        volunteer_only = { cost = 150 modifier = { conscription = 0.015 } }
        limited_conscription = { cost = 150 available = { has_war_support > 0.1 } modifier = { conscription = 0.025 } }
        extensive_conscription = { cost = 150 available = { has_war_support > 0.2 }
            modifier = { conscription = 0.05 training_time_factor = 0.1 } }
        service_by_requirement = { cost = 150 available = { has_war = yes }
            modifier = { conscription = 0.1 industrial_capacity_factory = -0.1 training_time_factor = 0.2 } }
        XXX_own_law = { cost = 150 allowed = { original_tag = XXX } modifier = { conscription = 0.04 } }
    }
}
"""
SUB = """mobile_infantry = { xp_cost = 100 rewards = { a = { } b = { } c = { mastery = 150 } } }
carpet = { xp_cost = 100 rewards = { a = { } } }
"""
DEFINES = "MAX_ARMY_EXPERIENCE = 500,\nMAX_NAVY_EXPERIENCE = 500,\nMAX_AIR_EXPERIENCE = 400,\nDEFAULT_REWARD_MASTERY = 100.0,"


def game_dir():
    root = tempfile.mkdtemp()
    common = os.path.join(root, "common")
    for sub in ("ideas", "defines", "idea_tags", "doctrines/subdoctrines/land"):
        os.makedirs(os.path.join(common, sub), exist_ok=True)
    with open(os.path.join(common, "ideas", "_manpower.txt"), "w") as f:
        f.write(LAWS)
    with open(os.path.join(common, "defines", "00_defines.lua"), "w") as f:
        f.write(DEFINES)
    with open(os.path.join(common, "doctrines", "subdoctrines", "land", "x.txt"), "w") as f:
        f.write(SUB)
    with open(os.path.join(common, "idea_tags", "00_idea.txt"), "w") as f:
        f.write("idea_categories = { a = { type = air_spirit cost = 50 } b = { type = air_spirit cost = 50 } }")
    return root


def snapshot(date="1940.06.03.19", xp=None, **over):
    s = {"date": date, "wars": {"at_war": True},
         "politics": {"political_power": 296, "war_support": 0.77, "manpower": 2540000,
                      "laws": {"conscription": "extensive_conscription"}},
         "doctrines": {"experience": xp or {"army": 101, "navy": 279, "air": 316},
                       "folders": [{"folder": "land", "xp_type": "army", "grand_doctrine": "g",
                                    "open_tracks": [], "tracks": [
                                        {"track": "infantry", "subdoctrine": "mobile_infantry", "mastery": 400},
                                        {"track": "armor", "subdoctrine": "carpet", "mastery": 0}]}]},
         "templates": [{"name": "Infanterie-Division", "training": None}],
         "land": {"division_count": 146},
         "production": {"stockpile": [{"archetype": "infantry_equipment", "stockpile": 56463,
                                       "fielded_short": 807}]},
         "navy": {"fleets": [{"fleet": "U", "task_forces": [{"name": "I. U", "mission": "training", "ships": 8}]},
                             {"fleet": "R", "task_forces": [{"name": "Res", "mission": "reserve", "ships": 25}]}]},
         "intelligence": {"agency": "A", "operatives": {"list": [
             {"mission": "counter_intelligence", "mission_target": "GER", "nationalities": ["GER"]}]}},
         "mios": {"listed": 27, "state_read": False},
         "alerts": [{"alert": "modify_officer_corps"}], "omissions": []}
    s.update(over)
    return s


class GameRules(unittest.TestCase):
    def test_the_files_give_caps_laws_and_reward_mastery(self):
        f = gamerules.facts(game_dir())
        self.assertEqual(f["xp_cap"]["air"], 400.0)
        self.assertEqual(f["spirit_costs"]["air"], [50.0, 50.0])
        self.assertEqual(f["subdoctrines"]["mobile_infantry"]["mastery_to_complete"], 350.0)
        laws = {x["law"]: x for x in f["laws"]["mobilization_laws"]}
        self.assertEqual(laws["extensive_conscription"]["modifiers"]["training_time_factor"], 0.1)
        self.assertTrue(laws["XXX_own_law"]["country_specific"])

    def test_laws_affordable_is_derived_from_power_and_conditions(self):
        rows = gamerules.laws_affordable({"political_power": 100, "war_support": 0.15,
                                          "laws": {"conscription": "limited_conscription"}}, False, game_dir())
        by = {r["law"]: r for r in rows}
        self.assertNotIn("XXX_own_law", by)
        # the files' base cost only: whether the power pays is the game's enabled state
        self.assertNotIn("affordable", by["extensive_conscription"])
        self.assertEqual(by["extensive_conscription"]["base_cost"], 150.0)
        self.assertFalse(by["extensive_conscription"]["available"])  # war support 15% not above 20%
        self.assertFalse(by["service_by_requirement"]["available"])  # needs a war


class Sinks(unittest.TestCase):
    def setUp(self):
        self.g = game_dir()

    def codes(self, snap, prev=()):
        return {f["code"]: f for f in sinks.findings(snap, prev, self.g)}

    def test_the_owner_examples_are_all_named(self):
        prev = [snapshot("1940.05.20.10", xp={"army": 90, "navy": 250, "air": 280})]
        got = self.codes(snapshot(), prev)
        for code in ("experience_unspent", "doctrine_track_complete", "doctrine_track_no_mastery",
                     "manpower_idle", "conscription_above_need", "fleet_training_at_war", "reserve_fleet_ships",
                     "operatives_home_only", "decryption_unread", "mio_state_unread"):
            self.assertIn(code, got)
        air = [f for f in sinks.findings(snapshot(), prev, self.g)
               if f["code"] == "experience_unspent" and f["subject"] == "air"][0]
        self.assertEqual(air["numbers"]["cap"], 400.0)
        self.assertIsNotNone(air["numbers"]["days_to_cap"])
        self.assertIn("limited_conscription", got["conscription_above_need"]["text"])
        self.assertIn("training_time_factor", got["conscription_above_need"]["text"])

    def test_experience_being_spent_and_an_omission_row_are_not_named_twice(self):
        prev = [snapshot("1940.05.20.10", xp={"army": 300, "navy": 400, "air": 450})]
        got = [f for f in sinks.findings(snapshot(), prev, self.g) if f["code"] == "experience_unspent"]
        self.assertEqual(got, [])  # every pool fell in 14 days: being spent
        s = snapshot(omissions=[{"item": "manpower_idle"}])
        self.assertNotIn("manpower_idle", self.codes(s))

    def test_no_cost_or_cap_is_kept_in_code_and_a_mod_changes_what_is_read(self):
        # owner, 27 Nov 2026: "not always the case, don't hardcode anything"
        none = gamerules.facts(tempfile.mkdtemp(), ())
        self.assertEqual((none["xp_cap"], none["spirit_costs"], none["bank_max"]), ({}, {}, None))
        # a mod that reprices a law and raises a cap is what the monitor reads
        mod = tempfile.mkdtemp()
        os.makedirs(os.path.join(mod, "common", "ideas"))
        os.makedirs(os.path.join(mod, "common", "defines"))
        with open(os.path.join(mod, "common", "ideas", "_manpower.txt"), "w") as f:
            f.write(LAWS.replace("limited_conscription = { cost = 150", "limited_conscription = { cost = 75"))
        with open(os.path.join(mod, "common", "defines", "zz_mod.lua"), "w") as f:
            f.write("MAX_AIR_EXPERIENCE = 900,")
        modded = gamerules.facts(self.g, ({"path": mod, "replace_path": []},))
        laws = {x["law"]: x for x in modded["laws"]["mobilization_laws"]}
        self.assertEqual(laws["limited_conscription"]["cost"], 75.0)
        self.assertEqual(modded["xp_cap"]["air"], 900.0)
        # the cap the experience finding uses moves with the files: 316 air experience growing is
        # high against a 400 cap and not against 900 (a changed cap changes the decision)
        prev = [snapshot("1940.05.20.10", xp={"army": 90, "navy": 250, "air": 280})]
        def air(facts_dir_mods):
            gamerules.DEFAULT_MODS = facts_dir_mods
            try:
                return [f for f in sinks.findings(snapshot(), prev, self.g)
                        if f["code"] == "experience_unspent" and f["subject"] == "air"][0]
            finally:
                gamerules.DEFAULT_MODS = ()
        self.assertEqual(air(())["severity"], "high")
        self.assertEqual(air(({"path": mod, "replace_path": []},))["numbers"]["cap"], 900.0)
        # no finding prices political power from the files: a pile of power alone is no finding
        s = snapshot()
        s["politics"]["political_power"] = 480
        s["alerts"] = []
        self.assertNotIn("political_power_unspent", self.codes(s))

    def test_the_monitor_finds_the_runs_game_folder(self):
        run = tempfile.mkdtemp()
        exe = os.path.join(self.g, "hoi4.app", "Contents", "MacOS", "hoi4")
        with open(os.path.join(run, "launch.json"), "w") as f:
            json.dump({"arguments": [exe, "-userdir=x"]}, f)
        self.assertEqual(gamerules.game_dir_of_run(run), self.g)
        self.assertEqual(gamerules.game_dir_of_run(tempfile.mkdtemp()), "")

    def test_a_lower_law_is_named_only_for_a_penalty_it_drops(self):
        s = snapshot()
        s["politics"]["laws"]["conscription"] = "limited_conscription"
        self.assertNotIn("conscription_above_need", self.codes(s))

    def test_a_wait_past_the_rows_deadline_is_named_with_the_lanes_own_words(self):
        s = snapshot(omissions=[{"item": "decisions_available", "days_open": 28, "owner": "decisions",
                                 "owner_note": "decisions: wanted, waiting for the game or its rule: X"},
                                {"item": "frame_4", "days_open": 28, "owner": "player"}])
        got = [f for f in sinks.findings(s, (), self.g) if f["code"] == "lane_wait_outlived"]
        self.assertEqual(len(got), 1)
        self.assertIn("waiting for the game or its rule", got[0]["text"])
        self.assertEqual(R.lane_of(got[0]["subject"]), "decisions")

    def test_a_lane_past_the_deadline_without_a_reason_is_a_fault_of_its_own_and_owned_by_that_lane(self):
        # t1939n (27 Nov 2026): "lane training says: no reason given" for 151 days, and the issue
        # went to the lane "alerts", which no executor lane answers
        s = snapshot(omissions=[{"item": "no_divisions_in_training", "days_open": 40, "owner": "training",
                                 "owner_note": "SILENT: lane training had 3 turn(s), the last 1 day(s) ago, "
                                               "and never stated a reason", "owner_silent": True},
                                {"item": "operative_recruitable", "days_open": 40, "owner": "intelligence"}])
        got = {f["subject"]: f for f in sinks.findings(s, (), self.g) if f["code"].startswith("lane_")}
        self.assertEqual(got["no_divisions_in_training"]["code"], "lane_silent")
        self.assertEqual(got["operative_recruitable"]["code"], "lane_silent")
        self.assertEqual(R.class_of("lane_silent"), "technical")
        self.assertEqual(R.executor_owner_lane(got["no_divisions_in_training"]["numbers"]), "training")
        self.assertIsNone(R.executor_owner_lane({"owner_rule": "player (manage.alerts_ignore)"}))


class LoopCloses(unittest.TestCase):
    def test_an_offered_issue_past_its_deadline_escalates_with_the_lane_reason_and_bundles(self):
        out = tempfile.mkdtemp()
        esc = Escalator(out, live=False)
        a = {"id": "I1", "key": "doctrines_available", "code": "doctrines_available", "class": "idling",
             "severity": "medium", "lane": "extras", "text": "a doctrine can be unlocked", "state": "open",
             "lane_reason": "special_forces grand doctrine: slot None"}
        b = dict(a, id="I2", key="special_project_available", code="special_project_available",
                 lane_reason="no facility of their kind")
        r1 = esc.consider("overdue", a, "1940.06.01.00")
        self.assertIn("offered to its lane", r1["why"])
        with open(os.path.join(out, "escalations", "I1-1940060100.md")) as f:
            self.assertIn("LANE SAYS", f.read())
        r2 = esc.consider("overdue", b, "1940.06.02.00")
        self.assertEqual(r2["bundled_into"], "I1")
        self.assertEqual(esc.used.get("1940"), 1)

    def test_a_lane_outcome_is_recorded_and_a_turn_alone_is_not_acted(self):
        det = Detector(tempfile.mkdtemp(), write=False)
        it = det.raise_issue("mio_traits", "mio_traits", "traits", lane="mio_traits")
        det._acted("mio_policy", "{}", False)
        self.assertEqual(it["state"], "open")
        det.outcome({"id": it["id"], "outcome": "waiting", "reason": "no trait readable", "lane": "mio_traits"})
        self.assertEqual(it["lane_outcome"]["outcome"], "waiting")
        self.assertEqual(it["lane_reason"], "no trait readable")

    def test_the_second_identical_refusal_writes_a_run_lesson_the_executor_reads(self):
        store = LessonStore(tempfile.mkdtemp())
        key = 'refusal_retried_unchanged:executor:{"kind": "mio_policy", "organization": "O", "policy": "P"}'
        issue = {"id": "I9", "key": key, "code": "refusal_retried_unchanged", "class": "tool_use",
                 "tool": "hoi4_mio_policy", "numbers": {"times": 2, "code": "input_readback_failed"},
                 "tried": [], "text": "x"}
        le = lessons_from_issue(store, issue, "repeated", "p65a")
        self.assertEqual(le["scope"], "run:p65a")
        self.assertEqual(le["situation"]["args"], {"organization": "O", "policy": "P"})
        self.assertEqual(le["executor"]["treat_as"], "wait")
        self.assertGreaterEqual(le["confidence"], 0.5)
        bad = dict(issue, key="invalid_arguments:hoi4_politics_options:use_mio_tools", code="invalid_arguments",
                   tool="hoi4_politics_options", numbers={"in_row": 2, "code": "use_mio_tools"})
        self.assertEqual(lessons_from_issue(store, bad, "repeated", "p65a")["executor"]["treat_as"], "permanent")
        broad = dict(issue, key="tool_failing:hoi4_army_move", code="tool_failing", tool="hoi4_army_move",
                     numbers={"in_row": 2, "code": "input_readback_failed"})
        self.assertNotIn("executor", lessons_from_issue(store, broad, "repeated", "p65a"))


if __name__ == "__main__":
    unittest.main()
