"""The plan-key reconcile (scripts/hoi4facts/plan_keys.py): every key plan.schema.json accepts has a reader.

(a) the repository passes: the gate CI runs;
(b) the executor reads a victory-point target's path, and the schema gives the key to the executor;
(c) each kind of drift is reported: a key the executor reads under another owner, a player key inside an
    executor-owned key, an unmarked key nothing reads, a lint key no planning tool reads;
(d) the lint tells a plan which of its keys the executor does not act on.
"""

import copy
import os
import sys
import unittest

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.join(ROOT, "scripts"))

from hoi4facts import plan_keys  # noqa: E402


def _problems(schema):
    out = {}
    for path, msg in plan_keys.check(schema):
        out[path] = (out[path] + "; " if path in out else "") + msg
    return out


class Reconcile(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.schema = plan_keys.load_schema()

    def test_every_key_of_the_schema_has_a_reader(self):
        self.assertEqual(plan_keys.check(self.schema), [])
        self.assertEqual(plan_keys.main(["check"]), 0)

    def test_the_executor_reads_the_path_of_a_victory_point_target(self):
        self.assertIn("vp_targets[].path", plan_keys.consumed_keys())
        self.assertEqual(plan_keys.readers(self.schema)["vp_targets[].path"], "executor")

    def test_a_key_the_executor_reads_cannot_belong_to_someone_else(self):
        schema = copy.deepcopy(self.schema)
        plan_keys.lookup(schema, "vp_targets[].path")["x-owner"] = "player"
        self.assertIn("the executor reads it", _problems(schema)["vp_targets[].path"])

    def test_a_player_key_inside_an_executor_key_needs_a_reader(self):
        schema = copy.deepcopy(self.schema)
        items = plan_keys._deref(schema["properties"]["vp_targets"], schema)["items"]["properties"]
        items["detour"] = {"type": "array", "x-owner": "player", "description": "waypoints (executor support later)"}
        self.assertIn("read by nobody", _problems(schema)["vp_targets[].detour"])
        items["detour"]["x-executor-planned"] = "W1-99"
        self.assertNotIn("vp_targets[].detour", _problems(schema))

    def test_an_unmarked_key_nothing_reads_is_reported(self):
        schema = copy.deepcopy(self.schema)
        war = plan_keys._deref(schema["properties"]["war"], schema)["properties"]
        war["flank_by_moonlight"] = {"type": "boolean"}
        self.assertIn("nothing in the executor reads it", _problems(schema)["war.flank_by_moonlight"])
        war["flank_by_moonlight"]["x-owner"] = "executor"
        self.assertIn("marked executor", _problems(schema)["war.flank_by_moonlight"])
        war["flank_by_moonlight"]["x-owner"] = "text"
        self.assertNotIn("war.flank_by_moonlight", _problems(schema))

    def test_a_lint_key_is_read_by_a_planning_tool_and_says_what_it_shapes(self):
        schema = copy.deepcopy(self.schema)
        war = plan_keys._deref(schema["properties"]["war"], schema)["properties"]
        war["flank_by_moonlight"] = {"type": "boolean", "x-owner": "lint", "description": "an estimate"}
        self.assertIn("no planning tool reads it", _problems(schema)["war.flank_by_moonlight"])
        by = plan_keys.lookup(schema, "vp_targets[].by")
        self.assertEqual(by["x-owner"], "lint")
        del by["description"]
        self.assertIn("description", _problems(schema)["vp_targets[].by"])

    def test_owner_names_are_checked(self):
        schema = copy.deepcopy(self.schema)
        plan_keys.lookup(schema, "vp_targets[].name")["x-owner"] = "nobody"
        self.assertIn("is not one of", _problems(schema)["vp_targets[].name"])

    def test_keys_a_plan_sets_that_the_executor_does_not_act_on(self):
        plan = {"vp_targets": [{"province": 1, "state": 2, "order": 1, "by": "1936.03.01", "path": [3, 1],
                                "name": "A", "why": "w"}],
                "production": [{"equipment": "x", "factories": 1, "replace": "y"}]}
        unread = plan_keys.unread_in(plan, self.schema)
        self.assertEqual(unread["lint"], ["vp_targets[].by", "vp_targets[].order"])
        self.assertEqual(unread["planned"], ["production[].replace"])
        self.assertNotIn("vp_targets[].path", sum(unread.values(), []))


if __name__ == "__main__":
    unittest.main()
