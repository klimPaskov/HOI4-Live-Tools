"""Replay of the recorded Italy-Ethiopia runs sk9 and sk10 through the rolling planner (offline).

The records hold the reads the runs made (overview, states, armies, wars, technology list, the change feed) in
order. The replay steps the planner at every overview read and checks that updates are produced, pass the gates
and keep the queues filled. With the game files and the planning tools installed, a second pass runs the full
gates (strict validation and the lint ratchet) and the game files' costs.
"""

import copy
import json
import os
import tempfile
import unittest

from hoi4_live.planner import facts as pf
from hoi4_live.planner import gates, replay
from hoi4_live.planner import horizon as hz

HERE = os.path.dirname(os.path.abspath(__file__))
FIX = os.path.join(HERE, "fixtures", "rolling")
PLAN = os.path.join(FIX, "plan-ita-1936.json")
RECORDS = [os.path.join(FIX, name) for name in ("sk9.jsonl.gz", "sk10.jsonl.gz")]


def strategy(plan):
    return {p: gates._at(plan, p) for p in gates.STRATEGY_PATHS}


class ReplayTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        with open(PLAN, encoding="utf-8") as f:
            self.plan = json.load(f)

    def replay(self, record, plan=None, **kw):
        path = PLAN
        if plan is not None:
            path = os.path.join(self.tmp.name, "plan-in.json")
            with open(path, "w", encoding="utf-8") as f:
                json.dump(plan, f)
        work = os.path.join(self.tmp.name, "work-" + os.path.basename(record))
        out = replay.run(record, path, work, **kw)
        with open(os.path.join(work, "plan.json"), encoding="utf-8") as f:
            out["final"] = json.load(f)
        return out

    def test_updates_are_produced_pass_the_gates_and_keep_the_strategy(self):
        for record in RECORDS:
            with self.subTest(record=os.path.basename(record)):
                out = self.replay(record)
                applied = [u for u in out["updates"] if u.get("version")]
                self.assertGreaterEqual(len(applied), 3, [u.get("triggers") for u in out["updates"]])
                for u in applied:
                    gate = [p for p in u["proposals"] if p.get("result", "").startswith("applied")][0]["gates"]
                    self.assertTrue(gate["ok"])
                    self.assertEqual([s["gate"] for s in gate["steps"]], ["shape", "invariants", "anchored"])
                # default updates never touch the strategy
                self.assertEqual(strategy(out["final"]), strategy(self.plan))
                # far fewer updates than steps: cooldowns, hysteresis and the schedule hold
                self.assertLess(len(out["updates"]), len(out["checks"]))
                versions = [c["version"] for c in out["checks"]]
                self.assertEqual(versions, sorted(versions))

    def test_shortages_get_a_line_and_political_power_a_use(self):
        for record in RECORDS:
            with self.subTest(record=os.path.basename(record)):
                out = self.replay(record)
                short = [c for c in out["checks"] if c["unlined_shortages"]]
                self.assertEqual(short, [])
                self.assertTrue(any(e.get("equipment", "").startswith("artillery_equipment")
                                    for e in out["final"].get("production") or []))
                self.assertGreater(len(out["final"]["politics_auto"]["order"]),
                                   len(self.plan["politics_auto"]["order"]))

    def test_a_short_research_list_is_extended_before_it_runs_dry(self):
        plan = copy.deepcopy(self.plan)
        plan["research_path"] = [dict(plan["research_path"][0], techs=plan["research_path"][0]["techs"][:4])]
        out = self.replay(RECORDS[1], plan)
        research = [u for u in out["updates"] if "horizon_low:research" in u["triggers"]]
        self.assertTrue(research)
        self.assertTrue(research[0].get("version"), research[0])
        first = research[0]["date"]
        after = [c for c in out["checks"] if c["date"] >= first]
        self.assertTrue(all(c["research_days"] > 0 for c in after), after[:3])
        added = hz.planned_techs(out["final"])[4:]
        self.assertTrue(added)
        self.assertFalse(any(t.startswith(("early_destroyer", "early_submarine")) for t in added))


class FakeModel:
    name = "model"

    def __init__(self, good):
        self.good, self.calls = good, 0

    def propose(self, folder, escalated):
        self.calls += 1
        with open(os.path.join(folder, "packet.json"), encoding="utf-8") as f:
            packet = json.load(f)
        if not self.good:
            return {"patch": {"summary": "bad", "ops": [{"op": "set", "key": "war.axes", "value": []}]},
                    "seconds": 1.0, "tokens": 100}
        default = [m["default"] for m in packet["menus"].values() if m.get("default")]
        ops = [op for d in default for op in d["ops"]]
        return {"patch": {"summary": "model picks", "ops": ops}, "seconds": 1.0, "tokens": 100}


class ReplayModelTests(unittest.TestCase):
    def test_model_patches_apply_and_bad_ones_fall_back(self):
        with tempfile.TemporaryDirectory() as d:
            good = FakeModel(True)
            out = replay.run(RECORDS[1], PLAN, os.path.join(d, "g"), proposer=good)
            self.assertTrue(any(u.get("applied") == "model" for u in out["updates"]))
            self.assertLessEqual(out["worker"]["budget"]["model_calls"], good.calls)
            bad = FakeModel(False)
            out = replay.run(RECORDS[1], PLAN, os.path.join(d, "b"), proposer=bad)
            self.assertTrue(bad.calls)
            self.assertFalse(any(u.get("applied") == "model" for u in out["updates"]))
            self.assertTrue(any(u.get("applied") == "default" for u in out["updates"]))


FACTS = pf.load()


@unittest.skipUnless(FACTS is not None, "game files or planning tools not found")
class ReplayWithGameFilesTests(unittest.TestCase):
    def test_full_gates_pass_on_the_recorded_runs(self):
        tools = gates.ToolGates(pf.tools_dir(), pf.game_dir())
        with tempfile.TemporaryDirectory() as d:
            out = replay.run(RECORDS[1], PLAN, os.path.join(d, "w"), facts=FACTS, tools=tools)
        applied = [u for u in out["updates"] if u.get("version")]
        self.assertTrue(applied)
        for u in applied:
            gate = [p for p in u["proposals"] if p.get("result", "").startswith("applied")][0]["gates"]
            self.assertFalse(gate["partial"])
            self.assertIn("lint", [s["gate"] for s in gate["steps"]])
        self.assertTrue(all(c["research_days"] > 0 and c["focus_days"] > 0 for c in out["checks"][5:]))


if __name__ == "__main__":
    unittest.main()
