"""One rolling-plan update: menus and default picks, the gates, the proposers and the atomic apply."""

import json
import os
import sys
import tempfile
import unittest

from hoi4_live.planner import budget, gates, ledger, menus, store, update
from hoi4_live.planner import horizon as hz
from hoi4_live.planner.state import day_of

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
DAY = day_of("1936.11.01")

PLAN = {
    "_meta": {"version": 1},
    "research_path": [{"year": 1936, "techs": ["basic_machine_tools", "radio"]}],
    "focus_path": ["F_a"], "focus_fallback": [],
    "politics_auto": {"threshold": 0, "order": [{"category": "government", "slot": 3, "ideas": ["X"], "cost": 150}]},
    "production": [{"equipment": "infantry_equipment_1", "factories": 10}],
    "production_priority": ["infantry_equipment"],
    "construction": [{"building": "arms_factory", "states": [161, 157], "count": 2}],
    "war": {"enemies": ["ETH"], "axes": [[842]]},
    "goal": {"summary": "ETH capitulates"},
    "vp_targets": [{"state": 842, "province": 1}],
    "checkpoints": [],
}


def snap(**kw):
    s = {"day": DAY, "date": "1936.11.01", "player": "ITA", "slots": 4,
         "researching": ["radio", None, None, None], "research_done": ["basic_machine_tools"],
         "available_techs": {"concentrated_industry": 1937, "construction2": 1937, "improved_machine_tools": 1937,
                             "early_destroyer": 1922, "cas1": 1936, "tech_engineers": 1936, "gwtank": 1918,
                             "infantry_weapons1": 1936, "far_future": 1940},
         "focuses_done": ["F_a"], "focus": None, "focus_states": {"F_b": "available", "F_c": "unavailable"},
         "pp": 145.0, "pp_gain": 1.5, "civ_idle": 11, "wars": ["ETH"], "controllers": {"842": "ITA"},
         "signals": {"political_power_idle": DAY - 2, "civilian_factories_idle": DAY},
         "shortages": {"artillery_equipment": {"day": DAY - 1, "days_of_cover": 0.0}}, "fill_low": {},
         "lines": {"infantry_equipment_1": 10},
         "vp": {"842": {"owner": "ETH", "provinces": [{"province": 1, "vp": 4, "controller": "ITA"}]},
                "271": {"owner": "ETH", "name": "Shewa", "provinces": [{"province": 2, "vp": 5, "controller": "ETH"}]},
                "835": {"owner": "ETH", "name": "Hararghe", "provinces": [{"province": 3, "vp": 6, "controller": "ETH"}]}}}
    s.update(kw)
    return s


def low_triggers(*domains):
    return [{"id": "horizon_low:%s" % d, "kind": "horizon_low", "domain": d, "class": "detail", "why": "x",
             "evidence": {}} for d in domains]


class MenuTests(unittest.TestCase):
    def test_research_menu_in_time_industry_first_no_naval_no_far_future(self):
        m = menus.research(PLAN, snap(), None, None, 120)
        keys = [o["key"] for o in m["options"]]
        self.assertTrue(keys)
        self.assertNotIn("early_destroyer", keys)            # no naval work in the plan
        self.assertNotIn("cas1", keys)                       # no air work in the plan
        self.assertNotIn("far_future", keys)                 # four years ahead
        in_time = [o["key"] for o in m["options"] if o["in_time"]]
        self.assertEqual(in_time[0], "infantry_weapons1")    # 1936 before 1937; land support before the rest
        self.assertEqual(m["default"]["ops"][0]["key"], "research_path")

    def test_focus_menu_from_live_availability(self):
        m = menus.focus(PLAN, snap(), None, None, 90)
        self.assertEqual([o["key"] for o in m["options"]], ["F_b"])
        self.assertEqual(m["default"]["ops"][0]["items"], ["F_b"])

    def test_politics_production_construction_army(self):
        h = hz.estimate(PLAN, snap())
        p = menus.politics(PLAN, snap(), None, None)
        order = p["default"]["ops"][0]["value"]
        self.assertEqual(order[0]["ideas"], ["X"])           # the plan's own entries stay first
        self.assertNotIn(("government", 3), [(e["category"], e["slot"]) for e in order[1:]])
        pr = menus.production(PLAN, snap(), None, None, h["production"])
        ops = pr["default"]["ops"]
        self.assertEqual(ops[0]["item"]["equipment"], "artillery_equipment_1")
        self.assertEqual(ops[1]["value"][0], "artillery_equipment")
        c = menus.construction(PLAN, snap(), None, None)
        self.assertEqual(c["default"]["ops"][0]["items"][0]["building"], "arms_factory")
        a = menus.army(PLAN, snap(), None, None)
        self.assertIn("held 4 of the 12 needed", a["progress"])
        self.assertEqual([i["state"] for i in a["default"]["ops"][0]["items"]], [835, 271])


class GateTests(unittest.TestCase):
    def test_invariants(self):
        cand = json.loads(json.dumps(PLAN))
        cand["war"]["axes"] = [[841]]
        self.assertTrue(gates.invariants(PLAN, cand, {"ops": []}, snap()))
        cand = json.loads(json.dumps(PLAN))
        cand["research_path"].append({"techs": ["gwtank"]})
        self.assertIn("researched techs listed again",
                      " ".join(gates.invariants(PLAN, cand, {"ops": []}, snap(research_done=["gwtank"]))))
        plan = dict(PLAN, construction_plan=[{"id": "c1"}])
        cand = dict(PLAN, construction_plan=[])
        self.assertIn("lost item ids", " ".join(gates.invariants(plan, cand, {"ops": []}, snap())))

    def test_tool_gates_ratchet(self):
        outputs = {}

        def runner(argv):
            path = argv[3]
            with open(path) as f:
                plan = json.load(f)
            bad = len(plan.get("production") or []) < 2
            if "validate_plan.py" in argv[2]:
                return 0, "checked; 1 error(s), 0 warning(s)"
            outputs.setdefault("n", 0)
            outputs["n"] += 1
            findings = [{"severity": "error", "rule": "production.balance"}] if bad else []
            return 0, json.dumps({"findings": findings})
        tools = gates.ToolGates("/nowhere", runner=runner)
        cand = dict(PLAN, production=PLAN["production"] + [{"equipment": "artillery_equipment_1", "factories": 3}])
        ok = gates.run(PLAN, cand, {"ops": [{"op": "add", "key": "production", "item": {}}]}, snap(), [], {},
                       hz.Costs(), tools=tools, incumbent_key="k")
        self.assertTrue(ok["ok"], ok)
        worse = gates.run(cand, PLAN, {"ops": []}, snap(), [], {}, hz.Costs(), tools=tools)
        self.assertFalse(worse["ok"])
        self.assertEqual(worse["steps"][-1]["gate"], "lint")


class FakeModel:
    name = "model"

    def __init__(self, patch_obj):
        self.patch = patch_obj
        self.calls = 0

    def propose(self, folder, escalated):
        self.calls += 1
        self.assertions = os.path.exists(os.path.join(folder, "packet.json"))
        return {"patch": self.patch, "seconds": 12.0, "tokens": 5000, "role": "planner", "model": "m"}


class UpdaterTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.run_dir = self.tmp.name
        with open(os.path.join(self.run_dir, "plan.json"), "w") as f:
            json.dump(PLAN, f)

    def tearDown(self):
        self.tmp.cleanup()

    def updater(self, proposer=None):
        return update.Updater(self.run_dir, store.PlanStore(self.run_dir),
                              ledger.Ledger(os.path.join(self.run_dir, "planner", "ledger.jsonl")),
                              budget.Budget(), proposer=proposer)

    def test_default_pick_fills_every_low_queue_and_is_versioned(self):
        s = snap()
        h = hz.estimate(PLAN, s)
        trig = low_triggers("research", "focus", "politics", "production", "construction")
        entry = self.updater().run(trig, s, h)
        self.assertEqual(entry["applied"], "default", entry)
        self.assertEqual(entry["version"], 2)
        plan, _ = store.PlanStore(self.run_dir).read()
        after = hz.estimate(plan, s)
        self.assertFalse(after["research"]["low"], after["research"])
        self.assertIn("F_b", plan["focus_path"])
        self.assertTrue(any(e.get("equipment") == "artillery_equipment_1" for e in plan["production"]))
        self.assertTrue(os.path.exists(os.path.join(self.run_dir, "plans", "plan.v1.json")))
        with open(os.path.join(self.run_dir, "planner", "journal.jsonl")) as f:
            self.assertEqual(json.loads(f.readline())["version"], 2)

    def test_model_patch_wins_when_it_passes_and_default_when_it_does_not(self):
        s = snap()
        h = hz.estimate(PLAN, s)
        good = {"summary": "focus F_b", "ops": [{"op": "extend", "key": "focus_path", "items": ["F_b"]}]}
        model = FakeModel(good)
        entry = self.updater(model).run(low_triggers("focus"), s, h)
        self.assertEqual((entry["applied"], model.calls), ("model", 1))
        bad = {"summary": "axes", "ops": [{"op": "set", "key": "war.axes", "value": [[1]]}]}
        with open(os.path.join(self.run_dir, "plan.json"), "w") as f:
            json.dump(PLAN, f)
        entry = self.updater(FakeModel(bad)).run(low_triggers("focus"), s, h)
        self.assertEqual(entry["applied"], "default")
        self.assertTrue(entry["proposals"][0]["result"].startswith("refused"))

    def test_budget_ceiling_skips_the_model(self):
        s = snap()
        u = self.updater(FakeModel({"ops": []}))
        u.budget = budget.Budget(max_calls=0)
        entry = u.run(low_triggers("focus"), s, hz.estimate(PLAN, s))
        self.assertTrue(entry["proposals"][0]["result"].startswith("skipped"))
        self.assertEqual(entry["applied"], "default")

    def test_agent_answer(self):
        s = snap()
        answer = {"summary": "agent", "ops": [{"op": "extend", "key": "focus_path", "items": ["F_b"]}]}

        def notify(folder):
            with open(os.path.join(folder, "patch.json"), "w") as f:
                json.dump(answer, f)
        entry = self.updater(update.AgentProposer(notify, timeout=5, sleep=lambda s: None)).run(
            low_triggers("focus"), s, hz.estimate(PLAN, s))
        self.assertEqual(entry["applied"], "agent")


class ModelProposerTests(unittest.TestCase):
    def test_runs_the_launcher_with_the_planner_role_and_reads_tokens(self):
        seen = {}
        with tempfile.TemporaryDirectory() as d:
            def runner(argv, timeout):
                seen["argv"] = argv
                with open(os.path.join(d, "agents-manifest.json"), "w") as f:
                    json.dump({"roles": {"planner": {"resolved_model": "m", "usage": {"inputTokens": 10,
                                                                                    "outputTokens": 5}}}}, f)
                with open(os.path.join(d, "patch.json"), "w") as f:
                    json.dump({"ops": []}, f)
                return 0
            got = update.ModelProposer(os.path.join(ROOT, "scripts"), runner=runner).propose(d, False)
        self.assertIn("planner", seen["argv"])
        self.assertEqual((got["tokens"], got["model"]), (15, "m"))

    def test_claude_adapter_reports_usage(self):
        sys.path.insert(0, os.path.join(ROOT, "scripts"))
        from strategist.adapters.claude_code import ClaudeCode
        out = ClaudeCode({}).parse_output(json.dumps({"modelUsage": {"m": {"inputTokens": 3, "outputTokens": 4}},
                                                      "duration_ms": 900}), None)
        self.assertEqual(out["usage"]["outputTokens"], 4)
        self.assertEqual(out["duration_ms"], 900)


if __name__ == "__main__":
    unittest.main()
