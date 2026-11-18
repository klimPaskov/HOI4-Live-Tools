"""playbook.refs (scripts/hoi4facts/lint_refs.py): every force, economy, air and political decision of a plan names
the playbook row, research row or guide section it follows, or why it departs; a playbook reference must exist."""
import os
import sys
import unittest

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.join(ROOT, "scripts"))

from hoi4facts import lint_refs  # noqa: E402


class FakeLint:
    def __init__(self, plan):
        self.plan = plan
        self.found = []

    def add(self, severity, rule, where, message):
        self.found.append((severity, rule, where, message))


class PlaybookRefsTests(unittest.TestCase):
    def run_check(self, plan):
        lint = FakeLint(plan)
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


if __name__ == "__main__":
    unittest.main()
