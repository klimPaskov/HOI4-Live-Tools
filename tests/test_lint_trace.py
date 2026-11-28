"""The traceability lint (lint_trace.py) on plain plans, and the schema accepting the keys it reads."""
import json
import os
import sys
import unittest

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.join(ROOT, "scripts"))

from hoi4facts import lint_trace  # noqa: E402

SCHEMA = os.path.join(ROOT, "skills", "hoi4-campaign-playbook", "plan.schema.json")


class FakeLint:
    def __init__(self, plan, mode="fair"):
        self.plan = plan
        self.findings = []
        self.facts = {} if mode == "fair" else {"fair": {"mode": mode}}

    def add(self, severity, rule, where, message):
        self.findings.append((severity, rule, where, message))

    def rules(self, severity=None):
        return [f for f in self.findings if severity is None or f[0] == severity]


def run(plan, mode="fair"):
    return lint_trace.check(FakeLint(plan, mode))


class Why(unittest.TestCase):
    def test_sources_are_recognised(self):
        for text in ("[G GER s8] the guide", "computed from the files", "[V common/ideas/france.txt]",
                     "BASE_TECH_COST 110", "strategy review gap 4", "engineer 104 kept it", "[NONE: a choice]"):
            self.assertTrue(lint_trace.has_source(text), text)
        for text in ("", None, "the plain reason", "Germany keeps it"):
            self.assertFalse(lint_trace.has_source(text), text)

    def test_one_finding_per_section_and_the_counts(self):
        plan = {"politics": [{"id": "a", "why": "plain"}, {"id": "b", "why": "[G x]"}, {"id": "c"}],
                "navy": {"fleets": [{"fleet": "F", "why": "computed"}]}}
        lint = run(plan)
        hits = [f for f in lint.findings if f[1] == "trace.why"]
        self.assertEqual(len(hits), 1)
        self.assertEqual(hits[0][0], "info")
        self.assertIn("a, c", hits[0][3])
        self.assertEqual(lint.facts["trace"]["entries"], 4)
        self.assertEqual(lint.facts["trace"]["with_source"], 2)

    def test_a_plan_family_gets_warnings(self):
        plan = {"goals": [{"id": "g", "source": "owner", "why": "x"}], "politics": [{"id": "a"}]}
        self.assertEqual([f[0] for f in run(plan).findings if f[1] == "trace.why"], ["warning"])


class Inert(unittest.TestCase):
    def test_inert_rows_name_their_needs(self):
        plan = {"_inert": [{"section": "intelligence", "entry": "decrypt", "needs": "engineer 110: decryption tool",
                            "why": "review"}, {"section": "x", "entry": "y"}]}
        lint = run(plan)
        rows = [f for f in lint.findings if f[1] == "trace.inert"]
        self.assertEqual(sorted(f[0] for f in rows), ["info", "warning"])


class Switches(unittest.TestCase):
    def test_goals_and_constraints(self):
        plan = {"goals": [{"id": "g", "why": "x"}], "constraints": {"airborne": {"value": False}}}
        self.assertEqual(len([f for f in run(plan).findings if f[1] == "trace.goals"]), 2)

    def test_airborne_off_with_paratroopers_left(self):
        plan = {"airborne": {"enabled": False},
                "templates": [{"id": "fj", "regiments": [["paratrooper", "paratrooper"]]}]}
        self.assertTrue([f for f in run(plan).findings if f[1] == "trace.airborne"])
        plan["_inert"] = [{"section": "templates", "entry": "fj", "needs": "airborne switch", "why": "owner"}]
        self.assertFalse([f for f in run(plan).findings if f[1] == "trace.airborne"])

    def test_a_raid_keeps_its_paratroopers_while_drops_are_off(self):
        plan = {"airborne": {"enabled": False, "raids": [{"id": "raid_eben_emael", "switch": "eben_emael_raid",
                                                           "why": "x"}]},
                "templates": [{"id": "fj", "regiments": [["paratrooper", "paratrooper"]]}]}
        self.assertFalse([f for f in run(plan).findings if f[1] == "trace.airborne"])
        plan["airborne"]["drops"] = [{"id": "ab_holland", "why": "x"}]
        self.assertTrue([f for f in run(plan).findings if f[1] == "trace.airborne"])
        del plan["airborne"]["drops"]
        plan["constraints"] = {"eben_emael_raid": {"value": False, "why": "a multiplayer rule"}}
        self.assertTrue([f for f in run(plan).findings if f[1] == "trace.airborne"])

    def test_airborne_on_in_fair_needs_roleplay_or_owner(self):
        plan = {"airborne": {"enabled": True, "switch": "airborne"}}
        self.assertTrue([f for f in run(plan).findings if f[1] == "trace.airborne"])
        self.assertFalse([f for f in run(plan, "meta").findings if f[1] == "trace.airborne"])
        plan["constraints"] = {"airborne": {"value": True, "source": "roleplay", "why": "the human asked"}}
        self.assertFalse([f for f in run(plan).findings if f[1] == "trace.airborne"])

    def test_volunteers(self):
        plan = {"air_plan": [{"id": "w", "count": 100}],
                "volunteers": [{"id": "a", "target": "SPR", "kind": "air", "planes": 50, "why": "x"},
                               {"id": "b", "target": "SPR", "kind": "land", "why": "x"}]}
        rows = [f for f in run(plan).findings if f[1] == "trace.volunteers"]
        self.assertEqual(len(rows), 2)
        self.assertIn("20%", rows[0][3])


class Schema(unittest.TestCase):
    def test_the_schema_lists_the_keys(self):
        with open(SCHEMA, encoding="utf-8") as f:
            props = json.load(f)["properties"]
        for k in ("goals", "constraints", "volunteers", "airborne"):
            self.assertIn(k, props)


if __name__ == "__main__":
    unittest.main()
