"""playbook.refs (scripts/hoi4facts/lint_refs.py): every force, economy, air and political decision of a plan names
the playbook row, research row or guide section it follows, or why it departs; a playbook reference must exist."""
import os
import sys
import unittest

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.join(ROOT, "scripts"))

from hoi4facts import lint_refs  # noqa: E402


class FakeLint:
    def __init__(self, plan, decisions=None):
        self.plan = plan
        self.decisions = decisions
        self.facts = {}
        self.found = []

    def add(self, severity, rule, where, message):
        self.found.append((severity, rule, where, message))


class PlaybookRefsTests(unittest.TestCase):
    def run_check(self, plan, decisions=None):
        lint = FakeLint(plan, decisions)
        lint_refs.check(lint)
        return lint.found

    def test_a_decision_without_a_source_fails(self):
        found = self.run_check({"templates": [{"id": "t", "why": "nine infantry [V facts.py]"}]})
        self.assertEqual(len(found), 1)
        self.assertEqual(found[0][:3], ("error", "playbook.refs", "templates[0]"))
        self.assertIn("1 decision entries name no source", found[0][3])

    def test_sources_of_every_kind_pass(self):
        plan = {"templates": [{"why": "line artillery [P templates-equipment-production.md#Templates]"}],
                "production": [{"why": "idle factories on support [R economy A16]"}],
                "politics": [{"why": "free trade early [G Italy section 5]"}],
                "politics_auto": {"order": [{"why": "[NONE: the guide's pick is a server mod decision]"}]}}
        self.assertEqual(self.run_check(plan), [])

    def test_a_reference_to_a_missing_file_or_row_fails(self):
        found = self.run_check({"mio": [{"why": "[P no-such-reference.md#X1]"}],
                                "designs": [{"why": "[P germany-fair.md#NO_SUCH_ROW_ZZ9]"}]})
        msgs = [f[3] for f in found]
        self.assertTrue(any("no reference no-such-reference.md" in m for m in msgs), msgs)
        self.assertTrue(any("has no row NO_SUCH_ROW_ZZ9" in m for m in msgs), msgs)

    def test_a_departure_needs_its_reason(self):
        found = self.run_check({"air_plan": [{"why": "[NONE: ]"}]})
        self.assertEqual(found[0][1], "playbook.refs")



TABLE = """# SOV fair: decisions and their sources
| id | area | playbook | private sources | web | game files and arithmetic | conflict | decision | reason |
|---|---|---|---|---|---|---|---|---|
| D-FOCUS-01 | FOCUS | none | none | 1 | 70 d | none | Five Year Plan first | +civs |
| D-TEMPLATE-01 | TEMPLATE | none | none | 2 | width 2 | none | 9 inf + 1 art | width 21 |
| D-WAR-01a | WAR | none | none | none | 92 VP | none | the war on 1939.9 | rules |
"""


class DecisionTableTests(unittest.TestCase):
    """[D row] sources and decisions.coverage (the restore-or-justify pass) against a run's decisions.md."""

    def run_check(self, plan, decisions=TABLE):
        lint = FakeLint(plan, decisions)
        lint_refs.check(lint)
        return lint.found, lint.facts

    def test_rows_are_read_from_the_table(self):
        self.assertEqual(lint_refs.decision_rows(TABLE), ["D-FOCUS-01", "D-TEMPLATE-01", "D-WAR-01a"])

    def test_every_row_cited_or_departed_passes(self):
        plan = {"templates": [{"why": "the attack line [D D-TEMPLATE-01]"}],
                "focus_path": [{"id": "x", "why": "[D D-FOCUS-01]"}],
                "_meta": {"decision_departures": [{"row": "D-WAR-01a", "reason": "the war goal needs the focus "
                                                   "SOV_x which completes 1939.11.2 at the earliest (70 d)"}]}}
        found, facts = self.run_check(plan)
        self.assertEqual(found, [])
        self.assertEqual(facts["decisions"], {"rows": 3, "cited": 2, "departed": 1})

    def test_a_row_neither_cited_nor_departed_is_an_error(self):
        found, _ = self.run_check({"templates": [{"why": "[D D-TEMPLATE-01]"}]})
        self.assertEqual([f[1] for f in found], ["decisions.coverage"])
        self.assertIn("2 of 3 decision table rows", found[0][3])
        self.assertIn("D-FOCUS-01", found[0][3])

    def test_a_lint_is_never_the_reason(self):
        for reason in ("a lint failed on this row", "dropped to pass the gates", "lint force.readiness errors"):
            plan = {"templates": [{"why": "[D D-TEMPLATE-01]"}], "x": "[D D-FOCUS-01]",
                    "_meta": {"decision_departures": [{"row": "D-WAR-01a", "reason": reason}]}}
            found, _ = self.run_check(plan)
            self.assertEqual(len(found), 1, reason)
            self.assertIn("a lint is not a reason", found[0][3])

    def test_unknown_rows_fail_as_source_and_as_departure(self):
        plan = {"templates": [{"why": "[D D-TEMPLATE-09]"}], "x": "[D D-FOCUS-01] [D D-WAR-01a]",
                "_meta": {"decision_departures": [{"row": "D-AIR-04", "reason": "no air base in range of the front"}]}}
        found, _ = self.run_check(plan)
        msgs = " | ".join(f[3] for f in found)
        self.assertIn("the decision table has no row D-TEMPLATE-09", msgs)
        self.assertIn("no row 'D-AIR-04'", msgs)

    def test_a_d_source_without_a_table_fails_and_no_table_means_no_coverage(self):
        found, facts = self.run_check({"templates": [{"why": "[D D-TEMPLATE-01]"}]}, decisions=None)
        self.assertEqual(len(found), 1)
        self.assertIn("no decision table", found[0][3])
        self.assertNotIn("decisions", facts)
        found, _ = self.run_check({"templates": [{"why": "[P germany-fair.md#Contents]"}]}, decisions=None)
        self.assertEqual(found, [])


if __name__ == "__main__":
    unittest.main()
