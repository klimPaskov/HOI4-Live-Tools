"""The plan-side reader of focus gates (scripts/hoi4facts/plan_gates.py) and the runtime reader
(native/src/focus_rules.rs) must never contradict each other.

tests/fixtures/focus_availability_cases.json holds the runtime's answers on a mod's focus tree (the native
tests hold the runtime to them). Here the plan-side reader judges the same focuses in the same states:
it may leave a gate undecided that the runtime decides, or the other way round, but where both decide
they must agree. Skipped while the plan-side reader is not in the tree."""

import json
import os
import sys
import unittest

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.join(ROOT, "scripts"))

from hoi4facts.script import get, get1, parse  # noqa: E402

try:
    from hoi4facts import plan_gates  # noqa: E402
except ImportError:  # the plan-side reader arrives with the focus slot lint
    plan_gates = None

FIXTURES = os.path.join(ROOT, "tests", "fixtures")


def load():
    with open(os.path.join(FIXTURES, "focus_availability_cases.json"), encoding="utf-8") as fh:
        data = json.load(fh)
    focuses = {}
    for rel in data["focus_files"]:
        with open(os.path.join(FIXTURES, rel), encoding="utf-8") as fh:
            for key, value in parse(fh.read()):
                if key == "focus_tree":
                    blocks = [b for k, b in value if k == "focus" and isinstance(b, list)]
                elif key in ("shared_focus", "joint_focus"):
                    blocks = [value]
                else:
                    blocks = []
                for block in blocks:
                    focuses.setdefault(get1(block, "id"), []).append(block)
    return data["cases"], focuses


def focuses_of(block, key):
    return [{x for a, x in group if a == "focus"} for group in get(block, key) if isinstance(group, list)]


@unittest.skipIf(plan_gates is None, "hoi4facts/plan_gates.py is not in this tree")
class PlanSideAgreesWithRuntime(unittest.TestCase):
    def plan_side(self, block, state):
        """available / unavailable / unknown as the plan-side reader sees the focus in `state`."""
        done = set(state["completed"])
        wars = state.get("wars")
        war = None if wars is None or (not wars and state.get("maybe_wars")) else bool(wars)
        own = [int(sid) for sid, row in (state.get("states") or {}).items()
               if row.get("owner") == state["tag"] and row.get("controller") == state["tag"]]
        facts = plan_gates.Facts(state["tag"], state.get("dlcs") or (), state.get("date"), war, wars or (),
                                 done, state.get("flags") or (), own)
        if get1(block, "id") in done or any(not group & done for group in focuses_of(block, "prerequisite")) \
                or any(group & done for group in focuses_of(block, "mutually_exclusive")):
            return "unavailable"
        gate = get1(block, "available")
        verdict = plan_gates.evaluate(gate, facts) if isinstance(gate, list) and gate else True
        return {True: "available", False: "unavailable", None: "unknown"}[verdict]

    def test_where_both_decide_they_agree(self):
        cases, focuses = load()
        compared = decided = 0
        for case in cases:
            state, blocks = case["state"], focuses.get(case["focus"]) or []
            # the plan-side reader takes the completed focuses and the tag as given, and reads
            # neither bypass, allow_branch nor the loaded tree
            if len(blocks) != 1 or "completed" not in state or "tag" not in state or "tree" in state:
                continue
            if get(blocks[0], "bypass") or get(blocks[0], "allow_branch"):
                continue
            compared += 1
            plan_side, runtime = self.plan_side(blocks[0], state), case["expect"]["state"]
            if "unknown" in (plan_side, runtime):
                continue
            decided += 1
            with self.subTest(case["name"]):
                self.assertEqual(plan_side, runtime)
        self.assertGreaterEqual(compared, 50)
        self.assertGreaterEqual(decided, 15)


if __name__ == "__main__":
    unittest.main()
