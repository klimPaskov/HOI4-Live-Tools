"""Decide only (hoi4_live/play/decide.py): the executor's lanes on a supplied snapshot, the actions as data."""
import io
import json
import re
import tempfile
import unittest
from contextlib import redirect_stdout
from pathlib import Path

from hoi4_live.play import decide as D

ROOT = Path(__file__).resolve().parents[1]
PLANS = ROOT / "skills" / "hoi4-campaign-playbook" / "plans"


def snap(**over):
    base = {
        "date": "1936.01.02.12", "player": "ITA", "country": "ITA",
        "politics": {"political_power": 40, "laws": {}, "manpower": 300000},
        "economy": {"factories": {"civilian": 15, "military": 12, "naval": 8}, "civilian_idle": 0, "resources": {}},
        "production": {"lines": [], "stockpile": []},
        "construction": {"queue": []},
        "research": {"slots": [{"slot": 0, "researching": None}, {"slot": 1, "researching": "construction1"}],
                     "focus": None},
        "doctrines": {"experience": {}},
        "land": {"armies": [], "divisions": []},
        "wars": {"at_war": False, "enemies": [], "states_controlled": [], "own_states_lost": [], "threatened": []},
        "air": {"wings": []}, "navy": {}, "occupation": {}, "intelligence": {},
        "events_open": [],
    }
    base.update(over)
    return base


def plan(**over):
    base = {
        "goal": "take the enemy's capital by summer",
        "research_path": [{"year": 1936, "techs": ["basic_machine_tools", "construction1"], "why": "industry first"}],
        "research_priority": ["support_weapons"],
        "focus_path": ["F_one", "F_two"],
        "politics": [{"id": "pol_a", "category": "government", "slot": 1, "idea": "war_economy", "cost": 150,
                      "why": "more factories for the army"}],
        "production": [{"equipment": "infantry_equipment_1", "factories": 6}],
        "construction": [{"building": "arms_factory", "count": 2, "states": [1, 2]}],
        "war": {"enemies": ["ENE"], "axes": [[10, 11]], "guard_states": [5]},
        "phases": [{"id": "P1", "goal": "break through in the north", "enter": "1936.01.01"}],
    }
    base.update(over)
    return base


class LaneTable(unittest.TestCase):
    def test_lanes_and_weights_are_the_executors(self):
        src = (ROOT / "native" / "src" / "plan_run.rs").read_text(encoding="utf-8")
        start = src.index("const LANE_WEIGHT")
        block = src[start:src.index("];", start)]
        weights = dict((n, m == "true") for n, m in re.findall(r'\(\s*"([a-z_]+)",\s*(true|false),', block))
        self.assertGreater(len(weights), 40)
        names = {n for n, _, _ in D.LANES}
        self.assertFalse(set(weights) - names, "a lane of the executor is missing here")
        for name, minor, _ in D.LANES:
            if name in weights:
                self.assertEqual(minor, weights[name], name)
            elif name != "events":
                self.assertFalse(minor, f"{name} is not in LANE_WEIGHT, so it is heavy")


class Decide(unittest.TestCase):
    def test_nothing_is_sent_and_the_answer_is_data(self):
        out = D.decide(plan(), snap())
        self.assertEqual(out["mode"], "decide_only")
        json.dumps(out)  # plain data
        tools = {a["tool"] for a in out["actions"]}
        self.assertIn("hoi4_research_start", tools)
        self.assertIn("hoi4_focus_start", tools)
        self.assertIn("hoi4_production_add", tools)
        for a in out["actions"]:
            self.assertIn(a["weight"], ("minor", "heavy"))
            self.assertTrue(a["why"])

    def test_research_fills_only_the_idle_slot_skipping_running(self):
        out = D.decide(plan(), snap())
        res = [a for a in out["actions"] if a["lane"] == "research"]
        self.assertEqual([(a["args"]["tech"], a["args"]["slot"]) for a in res], [("basic_machine_tools", 0)])
        self.assertIn("industry first", res[0]["why"])
        self.assertEqual(res[0]["basis"], "state")

    def test_politics_waits_for_power_then_picks_with_the_plans_reason(self):
        self.assertFalse([a for a in D.decide(plan(), snap())["actions"] if a["lane"] == "politics"])
        s = snap(politics={"political_power": 160, "laws": {}, "manpower": 300000})
        pol = [a for a in D.decide(plan(), s)["actions"] if a["lane"] == "politics"]
        self.assertEqual(pol[0]["args"]["idea"], "war_economy")
        self.assertEqual(pol[0]["why"], "more factories for the army")

    def test_war_orders_are_directives_with_the_phase_goal(self):
        s = snap(wars={"at_war": True, "enemies": ["ENE"], "states_controlled": [], "own_states_lost": [],
                       "threatened": []})
        out = D.decide(plan(), s)
        war = [a for a in out["actions"] if a.get("directive")]
        kinds = {(a["directive"]["kind"], a["directive"]["target"]) for a in war}
        self.assertIn(("attack", "ENE"), kinds)
        self.assertIn(("hold", "ENE"), kinds)
        self.assertTrue(all("break through in the north" in a["why"] for a in war))
        self.assertEqual(out["workload"]["level"], "fighting")
        self.assertEqual(out["phase"]["id"], "P1")

    def test_phases_advance_in_order_on_their_exits(self):
        p = plan(phases=[{"id": "A", "enter": "1936.01.01", "exit": "1936.01.16"},
                         {"id": "B", "enter": "1936.01.16", "exit": "state_controlled:842"},
                         {"id": "C", "exit": "province_controlled:1"},
                         {"id": "D", "exit": "capitulated:ENE"},
                         {"id": "E"}])
        m = {}
        at = lambda d, ctl=(), won=(): D.phase_in_force(p, {"date": d, "country": "ITA", "blocks": {"wars": "partial"},
                                                            "wars": {"states_controlled": list(ctl),
                                                                     "won": list(won)}}, m)["id"]
        self.assertEqual(at("1936.01.07"), "A")
        self.assertEqual(at("1936.01.20"), "B")
        self.assertEqual(at("1936.03.01", [842]), "C")
        self.assertEqual(at("1936.04.01"), "C")          # unreadable exit: it stays, never goes back
        self.assertEqual(at("1936.05.10", [842], ["ENE"]), "E")

    def test_a_beaten_enemy_is_no_longer_planned(self):
        s = snap(wars={"at_war": False, "enemies": [], "won": ["ENE"], "states_controlled": [],
                       "own_states_lost": [], "threatened": []})
        out = D.decide(plan(), s)
        self.assertFalse([a for a in out["actions"] if a.get("directive")])

    def test_before_the_war_it_prepares(self):
        out = D.decide(plan(), snap())
        prep = [a for a in out["actions"] if (a.get("directive") or {}).get("kind") == "prepare"]
        self.assertEqual(prep[0]["directive"]["target"], "ENE")
        self.assertEqual(out["workload"]["level"], "heavy")

    def test_memory_keeps_a_decision_from_repeating(self):
        first = D.decide(plan(), snap())
        again = D.decide(plan(), snap(), first["memory"])
        keys = {a["key"] for a in again["actions"]}
        self.assertNotIn("production:infantry_equipment", keys)
        self.assertNotIn("war:P1:prepare:ENE", keys)

    def test_unknown_blocks_decide_from_the_plan_and_say_what_is_blind(self):
        s = {"date": "1936.03.01", "country": "ITA",
             "politics": {"political_power": None},
             "blocks": {"politics": "partial", "wars": "known", "production": "unknown", "research": "unknown",
                        "focus": "unknown", "economy": "unknown", "air": "unknown", "land": "unknown"},
             "wars": {"at_war": False, "enemies": []}}
        out = D.decide(plan(), s)
        bases = {a["lane"]: a["basis"] for a in out["actions"]}
        self.assertEqual(bases.get("research"), "plan")
        self.assertEqual(bases.get("focus"), "plan")
        self.assertEqual(bases.get("production"), "plan")
        blind = {b["lane"] for b in out["blind"]}
        self.assertTrue({"stockpile", "resources"} <= blind)

    def test_plan_basis_focus_is_paced_by_game_time(self):
        s = {"date": "1936.03.01", "country": "ITA", "blocks": {"focus": "unknown"}}
        p = plan(war={}, phases=[])
        one = D.decide(p, s)
        two = D.decide(p, dict(s, date="1936.03.20"), one["memory"])
        three = D.decide(p, dict(s, date="1936.05.15"), two["memory"])
        focus = lambda o: [a["args"]["focus"] for a in o["actions"] if a["lane"] == "focus"]
        self.assertEqual(focus(one), ["F_one"])
        self.assertEqual(focus(two), [])
        self.assertEqual(focus(three), ["F_two"])

    def test_unreadable_predicates_wait(self):
        p = plan(construction_plan=[{"id": "cp1", "building": "rail_way", "from_state": 1, "to_state": 2,
                                     "when": "province_controlled:123", "order": 1}])
        out = D.decide(p, snap())
        self.assertIn("cp1", [w["key"].split(":")[-1] for w in out["waiting"]])

    def test_idle_when_nothing_is_due(self):
        out = D.decide({"active": True}, snap())
        self.assertEqual(out["workload"]["level"], "idle")

    def test_reviewed_plans_decide_without_errors(self):
        for f in sorted(PLANS.glob("*.plan.json")):
            p = json.loads(f.read_text(encoding="utf-8"))
            out = D.decide(p, snap(date="1936.02.01"))
            self.assertTrue(out["actions"], f.name)

    def test_cli_writes_the_answer_and_the_memory(self):
        with tempfile.TemporaryDirectory() as d:
            pp, sp, mp = Path(d, "plan.json"), Path(d, "snap.json"), Path(d, "memory.json")
            pp.write_text(json.dumps(plan()))
            sp.write_text(json.dumps(snap()))
            buf = io.StringIO()
            with redirect_stdout(buf):
                self.assertEqual(D.main(["--plan", str(pp), "--snapshot", str(sp), "--memory", str(mp)]), 0)
            out = json.loads(buf.getvalue())
            self.assertEqual(out["mode"], "decide_only")
            self.assertIn("sent", json.loads(mp.read_text()))


if __name__ == "__main__":
    unittest.main()
