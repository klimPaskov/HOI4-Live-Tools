"""The planner worker: reads through the runtime's read tools, updates in the background, rolls back, and the
supervisor's hoi4_run planner / plan_patch operations."""

import json
import os
import tempfile
import unittest

from hoi4_live.planner import worker as pw
from hoi4_live.planner.state import day_of

PLAN = {"_meta": {"version": 1}, "research_path": [{"year": 1936, "techs": ["a"]}], "focus_path": ["F1"],
        "politics_auto": {"order": [{"category": "government", "slot": 3}]}, "war": {"enemies": ["ETH"]},
        "production": [], "checkpoints": []}


class Game:
    """Answers the planner's reads; `changes` is the feed."""

    def __init__(self):
        self.date = "1936.03.01.12"
        self.changes = []
        self.calls = []

    def call(self, name, args):
        self.calls.append(name)
        if name == "hoi4_overview":
            return False, {"date": {"text": self.date}, "paused": False, "player": "ITA",
                           "research": [{"slot": 0, "researching": "a"}, {"slot": 1, "researching": None}],
                           "focus": {"key": "F1"}}
        if name == "hoi4_technology_list":
            return False, {"available_only": True, "start_years": {"industry_x": 1936, "radio_y": 1936}}
        if name == "hoi4_observe":
            return False, {"seq": len(self.changes), "changes": [dict(c, seq=i + 1) for i, c in enumerate(self.changes)
                                                               if i + 1 > args.get("since", 0)]}
        return True, {"error": {"code": "unsupported"}}


class WorkerTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.run_dir = self.tmp.name
        with open(os.path.join(self.run_dir, "plan.json"), "w") as f:
            json.dump(PLAN, f)
        self.game = Game()
        self.now = [0.0]
        self.notes = []
        self.w = pw.PlannerWorker(self.run_dir, self.game.call, note=self.notes.append, clock=lambda: self.now[0],
                                  background=False)

    def plan(self):
        with open(os.path.join(self.run_dir, "plan.json")) as f:
            return json.load(f)

    def test_tick_reads_only_and_updates_a_low_queue(self):
        self.w.tick()
        self.assertNotIn("hoi4_input_lease", self.game.calls)
        self.assertTrue(all(not c.startswith(("hoi4_time", "hoi4_autopilot")) for c in self.game.calls))
        plan = self.plan()
        self.assertEqual(plan["_meta"]["version"], 2)
        self.assertIn("industry_x", plan["research_path"][-1]["techs"])
        self.assertTrue(self.notes and self.notes[0].startswith("plan updated to v2"))
        # the next tick within read_every reads nothing
        n = len(self.game.calls)
        self.now[0] += 5
        self.w.tick()
        self.assertEqual(len(self.game.calls), n)

    def test_rollback_when_failures_rise_after_an_update(self):
        self.w.tick()
        self.assertEqual(self.plan()["_meta"]["version"], 2)
        day = day_of("1936.03.01")
        self.game.changes = [{"kind": "autopilot_step_failed", "date": "1936.03.%02d.01" % (2 + i), "detail": {}}
                             for i in range(5)]
        self.game.date = "1936.03.12.01"
        self.now[0] += 60
        self.w.tick()
        plan = self.plan()
        self.assertEqual(plan["_meta"]["rolled_back_from"], 2, plan["_meta"])
        self.assertEqual(self.w.counts["rollbacks"], 1)
        self.assertTrue(self.w.ledger.failed("v2", day + 11))

    def test_submit_from_the_users_agent(self):
        ok, _ = self.w.submit(7, {"ops": []})
        self.assertFalse(ok)
        self.w.tick()
        ok, _ = self.w.submit(1, {"ops": []})
        self.assertTrue(ok)
        self.assertTrue(os.path.exists(os.path.join(self.run_dir, "planner", "update-1", "patch.json")))

    def test_the_proposer_follows_the_configuration(self):
        settings = {"roles": {"planner": {"model": "m"}}}
        self.assertEqual(pw.proposer_for(lambda: settings, "/tools").name, "model")
        none = {"roles": {"planner": {"model": None}}}
        self.assertEqual(pw.proposer_for(lambda: none, "/tools", notify=print).name, "agent")
        self.assertIsNone(pw.proposer_for(lambda: none, None))


if __name__ == "__main__":
    unittest.main()
