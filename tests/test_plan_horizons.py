"""Plan horizons: the lint rule plan.horizons, the planner's reading of horizons and targets, the schema and
the plan-key reconcile."""

import json
import os
import sys
import unittest

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.join(ROOT, "scripts"))

from hoi4facts import lint_horizons, plan_keys, schema_check  # noqa: E402
from hoi4_live.planner import targets as tg  # noqa: E402
from hoi4_live.planner import triggers as tr  # noqa: E402
from hoi4_live.planner.state import day_of  # noqa: E402

HORIZONS = [
    {"id": "h1", "from": "1936.01.01", "until": "1937.01.01", "intent": "industry, then the Ethiopian war"},
    {"id": "h2", "from": "1937.01.01", "until": "1938.01.01", "intent": "Spain",
     "targets": {"divisions": 40, "pp_reserve": 100, "states_held": [1, 2]}},
    {"id": "h3", "until": "1939.09.01", "intent": "the Balkans", "targets": {"military_factories": 60}},
]


class Lint:
    def __init__(self, plan, stop=None):
        self.plan, self.findings, self.facts = plan, [], {}
        self.start = "1936.1.1"
        self.meta = {"stop_date": stop} if stop else {}
        self.goal = plan.get("goal") or {}

    def add(self, severity, rule, where, message):
        self.findings.append((severity, rule, where, message))


class LintRuleTests(unittest.TestCase):
    def run_rule(self, horizons, stop="1937.01.01", **plan):
        lint = Lint(dict(plan, horizons=horizons), stop)
        lint_horizons.check(lint)
        return lint

    def test_a_good_cut_has_no_finding_and_facts(self):
        lint = self.run_rule(HORIZONS, goal={"deadline": "1939.09.01"})
        self.assertEqual(lint.findings, [])
        self.assertEqual(lint.facts["horizons"]["first_until"], "1937.01.01")
        self.assertEqual([h["days"] for h in lint.facts["horizons"]["horizons"]], [365, 365, 608])

    def test_order_overlap_gap_start_and_deadline(self):
        bad = [dict(HORIZONS[0], **{"from": "1936.02.01"}), dict(HORIZONS[1], **{"from": "1936.12.01"}),
               dict(HORIZONS[2], **{"from": "1938.03.01"})]
        lint = self.run_rule(bad, stop="1936.06.01", goal={"deadline": "1940.01.01"})
        kinds = sorted((f[0], f[2]) for f in lint.findings)
        self.assertIn(("error", "horizons[0]"), kinds)          # not at the start
        self.assertIn(("error", "horizons[1]"), kinds)          # overlaps the first
        self.assertIn(("warning", "horizons[2]"), kinds)        # a gap, and ends before the deadline
        self.assertIn(("warning", "_validator.stop_date"), kinds)
        until = self.run_rule([dict(HORIZONS[0], until="1935.06.01")])
        self.assertTrue(any("is not after" in f[3] for f in until.findings))

    def test_no_horizons_is_info_and_late_research_is_noted(self):
        self.assertEqual(self.run_rule(None).findings[0][0], "info")
        lint = self.run_rule(HORIZONS, research_path=[{"year": 1936, "techs": ["a"]}, {"year": 1939, "techs": ["b"]}])
        self.assertEqual([f[0] for f in lint.findings], ["info"])
        self.assertIn("1939", lint.findings[0][3])

    def test_the_schema_accepts_horizons_and_refuses_item_lists(self):
        schema = plan_keys.load_schema()
        good = {"_meta": {"version": 1}, "horizons": HORIZONS}
        self.assertEqual(list(schema_check.validate(good, schema)), [])
        bad = {"_meta": {"version": 1},
               "horizons": [dict(HORIZONS[1], targets={"techs": ["radio"]})]}
        self.assertTrue(list(schema_check.validate(bad, schema)))

    def test_the_planner_owns_horizons(self):
        self.assertEqual(plan_keys.readers()["horizons"], "planner")
        self.assertEqual(plan_keys.readers()["horizons[].targets.divisions"], "planner")
        self.assertEqual([p for p in plan_keys.check() if p[0].startswith("horizons")], [])


class PlannerTargetsTests(unittest.TestCase):
    PLAN = {"horizons": HORIZONS}

    def snap(self, date, **kw):
        s = {"day": day_of(date), "date": date, "player": "ITA", "pp": 40.0,
             "armies": {"1. Armata": {"divisions": 20}, "2. Armata": {"divisions": 12, "fill": 0.7}},
             "controllers": {"1": "ITA", "2": "SPR", "3": "ITA"}}
        s.update(kw)
        return s

    def test_current_and_measure(self):
        i, h, left = tg.current(self.PLAN, day_of("1937.06.01"))
        self.assertEqual((i, h["id"]), (1, "h2"))
        self.assertEqual(left, day_of("1938.01.01") - day_of("1937.06.01"))
        self.assertEqual(tg.current(self.PLAN, day_of("1940.01.01")), (None, None, None))
        now = tg.measure(self.snap("1937.06.01"))
        self.assertEqual(now["divisions"], 32)
        self.assertEqual(now["states_held"], [1, 3])
        self.assertEqual(now["equipment_fill"], 0.7)

    def test_extend_before_the_end_then_compare_the_targets(self):
        memory = {}
        self.assertEqual(tg.due(self.PLAN, self.snap("1937.11.01"), memory), [])
        first = tg.due(self.PLAN, self.snap("1937.12.20"), memory)
        self.assertEqual([(t[0], t[1]) for t in first], [("horizon_end:h2", "schedule")])
        self.assertEqual(first[0][3]["next"], "h3")
        self.assertEqual(tg.due(self.PLAN, self.snap("1937.12.27"), memory), [])     # once
        ended = tg.due(self.PLAN, self.snap("1938.01.02"), memory)
        ids = [t[0] for t in ended]
        self.assertIn("deviation:horizon:h2", ids)
        rows = {r["target"]: r for r in ended[ids.index("deviation:horizon:h2")][3]["targets"]}
        self.assertTrue(rows["divisions"]["behind"])                  # 32 of 40
        self.assertTrue(rows["pp_reserve"]["behind"])
        self.assertEqual(rows["states_held"]["missing"], [2])

    def test_triggers_raise_them_and_two_behind_in_a_row_escalate(self):
        t = tr.Triggers()
        plan = {"horizons": [dict(HORIZONS[1], **{"from": "1936.01.01", "until": "1936.06.01"}),
                             dict(HORIZONS[1], id="h2b", **{"from": "1936.06.01", "until": "1936.12.01"})]}
        got = t.evaluate(plan, self.snap("1936.05.20"), {}, [])
        self.assertIn("horizon_end:h2", [x["id"] for x in got])
        got = t.evaluate(plan, self.snap("1936.06.02"), {}, [])
        dev = [x for x in got if x["id"] == "deviation:horizon:h2"][0]
        self.assertEqual((dev["kind"], dev["class"]), ("deviation", "detail"))
        got = t.evaluate(plan, self.snap("1936.12.02"), {}, [])
        dev = [x for x in got if x["id"] == "deviation:horizon:h2b"][0]
        self.assertEqual(dev["class"], "strategy")
        json.dumps(t.m)  # the memory stays serializable


if __name__ == "__main__":
    unittest.main()
