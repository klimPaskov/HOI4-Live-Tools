"""The state recorder, its findings, the sitrep, the diff and the history."""
from __future__ import annotations

import copy
import json
import os
import tempfile
import unittest

from hoi4_live.observe import findings as F
from hoi4_live.observe import history, sitrep
from hoi4_live.observe.recorder import Recorder
from hoi4_live.observe.replay import ReplayClient, load_calls, replay_run
from hoi4_live.observe.snapshot import build

HERE = os.path.dirname(os.path.abspath(__file__))
RECORDED = os.path.join(HERE, "fixtures", "observe", "recorded_bodies.json")


def snap(date="1940.06.01.12", **over):
    s = {
        "schema": 1, "date": date, "player": "GER", "country": "German Reich", "speed": 3, "paused": False,
        "trigger": "week",
        "politics": {"political_power": 100.0, "stability": 0.7, "war_support": 0.6},
        "economy": {"factories": {"civilian": 50, "military": 40, "naval": 0}, "civilian_idle": 0,
                    "civilian_on_construction": 45, "resources": {}},
        "production": {"lines": [{"equipment": "infantry_equipment_1", "kind": "military", "factories": 40}],
                       "military_assigned": 40, "naval_assigned": 0, "stockpile": [], "resource_shortages": []},
        "construction": {"queue": [], "queued": 4},
        "research": {"slots": [{"slot": 0, "researching": "a"}], "idle_slots": 0},
        "doctrines": {"experience": {}, "folders": []},
        "leaders": {"commanders": [], "without_commander": []},
        "land": {"armies": [], "groups": [], "divisions": [], "unassigned": 0},
        "stacking": {"provinces": []},
        "wars": {"at_war": True, "enemies": ["FRA"], "per_enemy": {}, "own_states_lost": [], "threatened": [],
                 "borders": [], "states_controlled": [64, 42, 6]},
        "air": {"wings": [], "regions": {}, "wings_without_region": [], "wings_without_mission": [],
                "aircraft_total": 0},
        "navy": {"fleets": [], "convoys": {}},
        "special_projects": {}, "alerts": [], "events_open": [], "omissions": [], "executor": {},
    }
    for k, v in over.items():
        if isinstance(v, dict) and isinstance(s.get(k), dict):
            s[k] = {**s[k], **v}
        else:
            s[k] = v
    return s


def army(name, divisions, orders=None, group=None, commander="A General", planning=None, states=None):
    return {"army": name, "group": group, "commander": commander, "divisions": divisions,
            "states": states or {"6": divisions}, "planning": planning or {"mean": 0.3, "max": 0.35},
            "orders": orders or [], "executing": bool(orders)}


def front(states, divisions, offensives=None, executing=True):
    return {"kind": "front", "states": states, "divisions": divisions, "executing": executing,
            "offensives": offensives or []}


def codes(fs):
    return [f["code"] for f in fs]


class FindingFixtures(unittest.TestCase):
    def test_air_force_left_over_poland(self):
        regions = {6: 5, 980: 5, 34: 5, 81: 38, 85: 38}
        air = {"regions": {"38": {"name": "Western Poland", "aircraft": 1300,
                                  "missions": {"cas": 800, "air_superiority": 500}},
                           "5": {"name": "Benelux Region", "aircraft": 1400,
                                 "missions": {"air_superiority": 1400}}},
               "aircraft_total": 2700, "wings": [], "wings_without_mission": []}
        land = {"armies": [army("Armee Nord", 20, [front([6, 980], 20)])], "groups": [], "divisions": [],
                "unassigned": 0}
        prev = [snap(d, air=air, land=land) for d in ("1940.05.11.00", "1940.05.18.00", "1940.05.25.00")]
        now = snap("1940.06.01.00", air=air, land=land)
        fs = F.findings(now, prev, regions)
        hit = [f for f in fs if f["code"] == "air_away_from_front"]
        self.assertEqual(len(hit), 1)
        self.assertEqual(hit[0]["subject"], "38")
        self.assertEqual(hit[0]["numbers"]["aircraft"], 1300)
        self.assertEqual(hit[0]["numbers"]["snapshots"], 3)
        self.assertIn("Western Poland", hit[0]["text"])
        self.assertIn("48% of the air force", hit[0]["text"])
        # in peace the planes may sit anywhere
        self.assertNotIn("air_away_from_front", codes(F.findings(snap(air=air, land=land, wars={"at_war": False}),
                                                                 [], regions)))

    def test_28_divisions_in_one_province_at_30_percent(self):
        divs = [{"id": f"d{i}", "army": "Armee Mitte", "province": 516, "state": 6, "supply": 30.0}
                for i in range(28)]
        now = snap(land={"armies": [army("Armee Mitte", 28, [front([6], 28)])], "groups": [],
                         "divisions": divs, "unassigned": 0},
                   stacking={"provinces": [{"province": 516, "state": 6, "divisions": 28, "supply": 11.0,
                                            "demand": 36.0, "headroom": -25.0},
                                           {"province": 600, "state": 6, "divisions": 3, "supply": 9.0,
                                            "demand": 4.0, "headroom": 5.0}]})
        fs = F.findings(now, [])
        hit = [f for f in fs if f["code"] == "province_overstacked"]
        self.assertEqual(len(hit), 1)
        self.assertEqual(hit[0]["severity"], "high")
        self.assertEqual(hit[0]["numbers"]["divisions"], 28)
        self.assertIn("28 divisions draw 36.0 supply where the province gives 11.0", hit[0]["text"])
        self.assertIn("their supply is 30%", hit[0]["text"])
        self.assertIn("carries about 8", hit[0]["text"])

    def test_army_with_no_order(self):
        land = {"armies": [army("Panzergruppe Nord", 16), army("Armee Süd", 20, group="Army Group 1"),
                           army("Armee Nord", 12, [front([6], 12)])],
                "groups": [{"group": "Army Group 1", "field_marshal": "Rundstedt",
                            "orders": [front([6, 977], 20)]}],
                "divisions": [], "unassigned": 0}
        fs = F.findings(snap(land=land), [])
        hit = [f for f in fs if f["code"] == "army_without_order"]
        self.assertEqual([f["subject"] for f in hit], ["Panzergruppe Nord"])  # the group's front covers Süd
        self.assertEqual(hit[0]["severity"], "high")
        peace = F.findings(snap(land=land, wars={"at_war": False}), [])
        self.assertEqual([f["severity"] for f in peace if f["code"] == "army_without_order"], ["low"])

    def test_free_field_marshal_and_a_group_without_one(self):
        land = {"armies": [army("Armee Pommern", 23, [front([980], 23)], group="Army Group 2")],
                "groups": [{"group": "Army Group 2", "field_marshal": None, "orders": []}],
                "divisions": [], "unassigned": 0}
        leaders = {"commanders": [{"name": "Fedor von Bock", "rank": "field_marshal", "assigned_to": None},
                                  {"name": "Hermann Hoth", "rank": "general", "assigned_to": "Armee Pommern"}]}
        fs = F.findings(snap(land=land, leaders=leaders), [])
        hit = [f for f in fs if f["code"] == "army_group_without_field_marshal"]
        self.assertEqual(len(hit), 1)
        self.assertIn("Fedor von Bock", hit[0]["text"])
        self.assertEqual(hit[0]["numbers"]["divisions"], 23)
        # the server's omission row owns the check when it is there: reported once, from the server
        om = [{"item": "army_group_without_field_marshal", "days_open": 20,
               "todo": "hoi4_army_commander", "owner": "group_marshals"}]
        fs = F.findings(snap(land=land, leaders=leaders, omissions=om), [])
        hit = [f for f in fs if f["code"] == "army_group_without_field_marshal"]
        self.assertEqual(len(hit), 1)
        self.assertEqual(hit[0]["owner"], "server omissions")

    def test_political_power_banked_with_an_affordable_law(self):
        law = [{"name": "partial_economic_mobilisation", "cost": 150}]
        prev = [snap("1940.05.15.00", politics={"political_power": 480.0})]
        now = snap("1940.06.01.00", politics={"political_power": 540.0, "affordable": law})
        fs = F.findings(now, prev)
        hit = [f for f in fs if f["code"] == "political_power_unspent"]
        self.assertEqual(len(hit), 1)
        self.assertIn("political power 540 banked, +60 in 14 days", hit[0]["text"])
        self.assertIn("partial_economic_mobilisation (150)", hit[0]["text"])
        # saving for a costlier pick: not a finding
        saving = snap("1940.06.01.00", politics={"political_power": 540.0, "affordable": law,
                                                 "saving_for": {"name": "war_economy", "cost": 600}})
        self.assertNotIn("political_power_unspent", codes(F.findings(saving, prev)))
        # being spent (falling): not a finding
        spent = snap("1940.06.01.00", politics={"political_power": 400.0, "affordable": law})
        self.assertNotIn("political_power_unspent", codes(F.findings(spent, prev)))
        # nothing affordable and no game mark: not a finding
        none = snap("1940.06.01.00", politics={"political_power": 540.0})
        self.assertNotIn("political_power_unspent", codes(F.findings(none, prev)))
        # the game's own mark is enough
        marked = snap("1940.06.01.00", politics={"political_power": 540.0},
                      alerts=[{"alert": "modify_government"}])
        self.assertIn("political_power_unspent", codes(F.findings(marked, prev)))


class MoreFindings(unittest.TestCase):
    def test_alert_icons_no_tool_names_are_something_to_learn(self):
        now = snap("1940.06.01.00", alerts=[{"alert": "decisions_available", "frame": 3},
                                            {"alert": None, "frame": 41}, {"alert": None, "frame": 41}])
        hit = [f for f in F.findings(now, []) if f["code"] == "game_alert_unknown"]
        self.assertEqual(len(hit), 1)
        self.assertIn("frames 41", hit[0]["text"])
        self.assertIn("hoi4_ui_inspect", hit[0]["text"])
        named = snap("1940.06.01.00", alerts=[{"alert": "decisions_available", "frame": 3}])
        self.assertNotIn("game_alert_unknown", codes(F.findings(named, [])))

    def test_losing_exchange_and_flat_victory_points(self):
        prev = [snap("1940.05.18.00", wars={"per_enemy": {"BEL": {"at_war": True, "casualties_own": 10000,
                                                                  "casualties_enemy": 9000,
                                                                  "vp_held_by_us": 5, "vp_total": 86}}})]
        now = snap("1940.06.01.00", wars={"per_enemy": {"BEL": {"at_war": True, "casualties_own": 31000,
                                                                "casualties_enemy": 13000,
                                                                "vp_held_by_us": 5, "vp_total": 86}}})
        fs = F.findings(now, prev)
        lose = [f for f in fs if f["code"] == "losing_exchange"][0]
        self.assertEqual((lose["numbers"]["lost"], lose["numbers"]["dealt"]), (21000, 4000))
        self.assertIn("victory_points_flat", codes(fs))

    def test_skirmish_noise_is_not_an_exchange(self):
        prev = [snap("1940.05.18.00", wars={"per_enemy": {"NZL": {"at_war": True, "casualties_own": 60,
                                                                  "casualties_enemy": 12}}})]
        now = snap("1940.06.01.00", wars={"per_enemy": {"NZL": {"at_war": True, "casualties_own": 70,
                                                                "casualties_enemy": 12}}})
        self.assertNotIn("losing_exchange", codes(F.findings(now, prev)))

    def test_offensive_at_own_state_and_planning_near_zero(self):
        off = {"target_states": [42], "path_states": [6], "executing": False}
        land = {"armies": [army("PG Ost", 9, [front([6], 9, [off], executing=False)],
                                planning={"mean": 0.0, "max": 0.002})], "groups": [], "divisions": [],
                "unassigned": 0}
        fs = codes(F.findings(snap(land=land), []))
        self.assertIn("offensive_at_own_state", fs)
        self.assertIn("planning_near_zero", fs)

    def test_agency_without_a_network_abroad_and_resistance_over_its_target(self):
        home = {"mission": "counter_intelligence", "mission_target": "GER"}
        now = snap(intelligence={"created": True, "networks": [],
                                 "operatives": {"idle": 0, "list": [home, dict(home)]}},
                   occupation={"law": {"key": "martial_law_occupation"}, "states": [
                       {"state": 7, "name": "Holland", "resistance": 11.1, "resistance_target": 10.0},
                       {"state": 6, "name": "Flanders", "resistance": 6.4, "resistance_target": 10.0}]})
        fs = F.findings(now, [])
        self.assertIn("intel_no_network_abroad", codes(fs))
        res = [f for f in fs if f["code"] == "resistance_above_target"]
        self.assertEqual(len(res), 1)
        self.assertIn("Holland 11/10", res[0]["text"])
        # a network abroad, or an operative abroad: nothing to say
        ok = snap(intelligence={"created": True, "networks": [{"target": "FRA", "strength": 20}],
                                "operatives": {"list": [home]}})
        self.assertNotIn("intel_no_network_abroad", codes(F.findings(ok, [])))

    def test_executor_repeated_refusal_and_idle_assignables(self):
        ex = {"game_hour": 1000, "failures": [{"what": "hoi4_air_mission {...}", "count": 4, "last_hour": 990}]}
        now = snap(executor=ex, economy={"civilian_idle": 6, "factories": {"civilian": 50, "military": 45}},
                   research={"slots": [], "idle_slots": 2})
        fs = codes(F.findings(now, []))
        for c in ("executor_repeated_refusal", "civilian_factories_idle", "military_factories_unassigned",
                  "research_slot_idle"):
            self.assertIn(c, fs)

    def test_findings_remember_how_long_they_are_open(self):
        land = {"armies": [army("Lost Army", 5)], "groups": [], "divisions": [], "unassigned": 0}
        p1 = snap("1940.05.18.00", land=land)
        p1["findings"] = F.findings(p1, [])
        now = snap("1940.05.25.00", land=land)
        f = [x for x in F.findings(now, [p1]) if x["code"] == "army_without_order"][0]
        self.assertEqual(f["open_snapshots"], 1)


class RecordedBodies(unittest.TestCase):
    def setUp(self):
        with open(RECORDED) as f:
            self.raw = json.load(f)

    def test_build_from_recorded_reads(self):
        raw = dict(self.raw)
        states = {"LUX": raw.pop("states_LUX")}
        s = build(raw, states_by_tag=states, trigger="start")
        self.assertEqual(s["date"], "1940.07.26.08")
        self.assertEqual(s["player"], "GER")
        self.assertTrue(s["land"]["armies"])
        a = s["land"]["armies"][0]
        self.assertEqual(a["army"], "Panzergruppe Ost")
        self.assertEqual(a["orders"][0]["kind"], "front")
        self.assertEqual(a["orders"][0]["offensives"][0]["target_states"], [980])
        self.assertEqual(s["land"]["groups"][0]["field_marshal"], None)
        self.assertEqual(s["stacking"]["provinces"][0]["divisions"], 24)
        self.assertGreater(s["air"]["regions"]["5"]["aircraft"], 1000)
        hol = s["wars"]["per_enemy"]["HOL"]
        self.assertGreater(hol["casualties_own"], 0)
        self.assertGreater(hol["casualties_enemy"], 0)
        self.assertEqual(s["wars"]["per_enemy"]["LUX"]["vp_total"], 5.0)
        self.assertEqual(s["economy"]["factories"]["military"], 109)
        fs = F.findings(s, [], {6: 5, 980: 5, 34: 5, 977: 5})
        self.assertIn("province_overstacked", codes(fs))
        self.assertIn("air_away_from_front", codes(fs))  # the wings left in the Polish regions
        text = sitrep.render({**s, "findings": fs}, [], "fixture")
        self.assertTrue(text.startswith("SITREP fixture 1940.07.26.08"))
        self.assertIn("WHAT IS WRONG", text)
        self.assertIn("NOT READABLE", text)


class RecorderLoop(unittest.TestCase):
    def setUp(self):
        self.dir = tempfile.mkdtemp()
        with open(RECORDED) as f:
            self.raw = json.load(f)
        self.date = "1940.07.26.08"
        self.enemies = ["HOL"]
        self.calls = []

    def client(self, tool, args):
        self.calls.append(tool)
        if tool == "hoi4_overview":
            b = copy.deepcopy(self.raw["overview"])
            b["date"]["text"] = self.date
            return b
        if tool == "hoi4_wars":
            b = copy.deepcopy(self.raw["wars"])
            b["wars"]["enemies"] = list(self.enemies)
            return b
        key = {"hoi4_armies": "armies", "hoi4_supply": "supply", "hoi4_air_regions": "air_regions",
               "hoi4_war_stats": "war_stats", "hoi4_economy": "economy",
               "hoi4_production_lines": "production_lines", "hoi4_stockpile": "stockpile"}.get(tool)
        if key:
            return self.raw[key]
        if tool == "hoi4_states" and args.get("query") == "LUX":
            return self.raw["states_LUX"]
        return {"error": {"code": "not_in_fixture"}}

    def test_snapshots_on_start_week_war_and_operation_phase(self):
        rec = Recorder(self.dir, self.client, every_days=7)
        p = rec.poll()
        self.assertTrue(p.endswith("1940.07.26.08.json"))
        self.assertEqual(rec.prev[-1]["trigger"], "start")
        self.assertTrue(os.path.exists(os.path.join(self.dir, "SITREP.txt")))
        self.date = "1940.07.30.08"
        self.assertIsNone(rec.poll())  # four days: not due
        self.enemies = ["HOL", "BEL"]
        p = rec.poll()
        self.assertIn("war_declared:BEL", rec.prev[-1]["trigger"])
        with open(os.path.join(self.dir, "plan_state.json"), "w") as f:
            json.dump({"done": {"op:op_west:exec": 1, "op:op_west:draw:A:6": 1}}, f)
        p = rec.poll()
        self.assertIn("operation:op_west:executing", rec.prev[-1]["trigger"])
        self.date = "1940.08.07.08"
        p = rec.poll()
        self.assertEqual(rec.prev[-1]["trigger"], "week")
        self.assertEqual(rec.prev[-1]["executor"]["operations"]["op_west"]["phase"], "executing")
        # only read-lane tools were called
        leased = {"hoi4_politics_options", "hoi4_decisions_list", "hoi4_trade", "hoi4_plan"}
        self.assertFalse(leased & set(self.calls))
        snaps = history.load_run(self.dir)
        self.assertEqual(len(snaps), 4)
        out = history.render(snaps, "planes")
        self.assertIn("Benelux", out.splitlines()[0])
        d = sitrep.diff(snaps[0], snaps[-1])
        self.assertTrue(d.startswith("CHANGES 1940.07.26.08 -> 1940.08.07.08"))
        self.assertIn("now at war with BEL", d)

    def test_replay_of_a_call_log(self):
        log = os.path.join(self.dir, "calls.jsonl")
        with open(log, "w") as f:
            for i, date in enumerate(["1940.07.01.00", "1940.07.03.00", "1940.07.09.00"]):
                ov = copy.deepcopy(self.raw["overview"])
                ov["date"]["text"] = date
                f.write(json.dumps({"utc": i * 10 + 1, "tool": "hoi4_armies", "args": {},
                                    "isError": False, "body": self.raw["armies"]}) + "\n")
                f.write(json.dumps({"utc": i * 10 + 2, "tool": "hoi4_overview", "args": {},
                                    "isError": False, "body": ov}) + "\n")
        out = os.path.join(self.dir, "replay")
        rec = Recorder(out, ReplayClient(load_calls(log)), run_files=False)
        written = replay_run(log, rec)
        self.assertEqual(len(written), 2)  # start, then a week later
        self.assertEqual(history.load_run(out)[0]["land"]["armies"][0]["army"], "Panzergruppe Ost")


class HistoryPaths(unittest.TestCase):
    def test_paths(self):
        s = snap(stacking={"provinces": [{"province": 516, "divisions": 24}]},
                 air={"regions": {"5": {"aircraft": 10, "name": "B"}, "38": {"aircraft": 3, "name": "P"}}})
        self.assertEqual(history.extract(s, "stacking.provinces[province=516].divisions"), {"value": 24})
        self.assertEqual(history.extract(s, "planes"), {"5": 10, "38": 3})
        self.assertEqual(history.extract(s, "pp"), {"value": 100.0})
        self.assertEqual(history.extract(s, "air.wings_without_mission.#"), {"value": 0})


def battles_body():
    """hoi4_battles as the server answers it: the Belgian forest attack of the 23 Nov study session
    (province 3488, 23 divisions, 4 in the line and 19 in reserve) and a Dutch defence we are losing."""
    def side(role, tags, engaged, reserve, org, hours_left, planning=0.33, tactic="tactic_shock"):
        return {"role": role, "tags": tags, "tactic": tactic, "divisions": engaged + reserve,
                "engaged": engaged, "reserve": reserve, "org_engaged_mean": org, "planning_mean": planning,
                "supply_mean": 1.0, "hours_until_broken": hours_left,
                "engaged_divisions": [{"template": "Infanterie-Division"}]}
    return {"country": "GER", "battles": 2, "summary": {"attacking": 1, "defending": 1,
            "divisions_engaged": 5, "divisions_in_reserve": 19},
            "list": [
                {"id": 5346, "province": 3488, "state": 980, "terrain": "forest", "hours": 72,
                 "since": {"iso": "1940-05-11 06:00"}, "our_side": "attacker",
                 "width": {"terrain": 60.0, "per_extra_direction": 30.0, "directions": 2,
                           "attacker_max": 90.0, "attacker_engaged": 72.0, "full": True},
                 "winning": "defender",
                 "lines": [{"line": "terrain", "side": "attacker", "value": -0.15},
                           {"line": "river", "side": "attacker", "divisions_crossing": 13,
                            "crossings": [{"from": 3448, "river": "small", "value": -0.3}]}],
                 "attacker": side("attacker", ["GER"], 4, 19, 34.9, 122.0),
                 "defender": side("defender", ["BEL"], 5, 0, 33.0, 205.0, 0.0, "tactic_basic_defend"),
                 "not_read": ["night"]},
                {"id": 5343, "province": 6500, "state": 35, "terrain": "plains", "hours": 86,
                 "since": {"iso": "1940-05-11 00:00"}, "our_side": "attacker",
                 "width": {"terrain": 70.0, "per_extra_direction": 35.0, "directions": 1,
                           "attacker_max": 70.0, "attacker_engaged": 12.0, "full": False},
                 "winning": "defender", "lines": [],
                 "attacker": side("attacker", ["GER"], 1, 0, 13.6, 20.0, tactic="tactic_basic_attack"),
                 "defender": side("defender", ["HOL"], 2, 0, 51.6, 900.0, 0.0, "tactic_ambush")},
            ]}


class Battles(unittest.TestCase):
    def test_battle_read_becomes_findings_and_a_sitrep_block(self):
        with open(RECORDED) as f:
            raw = json.load(f)
        states = {"LUX": raw.pop("states_LUX")}
        raw["battles"] = battles_body()
        s = build(raw, states_by_tag=states, trigger="start")
        self.assertTrue(s["battles"]["readable"])
        self.assertEqual(s["battles"]["list"][0]["our_reserve"], 19)
        fs = F.findings(s, [], {})
        got = codes(fs)
        self.assertIn("battle_reserve_waiting", got)
        self.assertIn("battle_across_river", got)
        self.assertIn("battle_being_lost", got)
        self.assertIn("battle_low_organisation", got)
        text = " ".join(f["text"] for f in fs if f["code"] == "battle_reserve_waiting")
        self.assertIn("19 of our divisions in reserve", text)
        out = sitrep.render(s, [], "t", fs)
        self.assertIn("BATTLES 2", out)
        self.assertIn("3488 (forest, st 980) we attacker vs BEL 72h: line 4+19 reserve", out)

    def test_no_battle_read_is_marked_missing(self):
        s = build({}, trigger="start")
        self.assertFalse(s["battles"]["readable"])


if __name__ == "__main__":
    unittest.main()
