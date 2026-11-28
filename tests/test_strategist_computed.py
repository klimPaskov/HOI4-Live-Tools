"""scripts/strategist/computed.py: guide claims beside computed answers, country knowledge by reference name."""
import os
import sys
import unittest

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.join(ROOT, "scripts", "strategist"))
import computed  # noqa: E402


class ComputedTest(unittest.TestCase):
    def test_claims_against_computed(self):
        data = {"construction": {"k": 1.786}, "research": {"picks": [[{"tech": "construction1"}]]}}
        rows = computed.compare([{"what": "construction.k", "says": 1.92, "where": "a guide"},
                                 {"what": "research.picks.0.0.tech", "says": "electronic_mechanical_engineering"}],
                                data)
        self.assertEqual(rows[0]["difference"], round(1.786 - 1.92, 4))
        self.assertEqual(rows[1]["computed"], "construction1")
        self.assertIsNone(rows[1]["difference"])

    def test_country_knowledge(self):
        self.assertIn("germany-fair.md", computed.references_for("Germany", "GER"))
        self.assertEqual(computed.references_for("Hungary", "HUN"), [])

    def test_lines_tag_every_decision_computed(self):
        data = {"tag": "HUN", "date": "1936.1.1", "enemies": ["CZE"], "enemies_how": "given", "war_date": "1939.1.1",
                "goals": [], "errors": {}, "references": [],
                "construction": {"k": 0.786, "k_single_bonus": 1.5, "decision": "military factories now"},
                "claims": [{"what": "construction.k", "guide": 1.5, "computed": 0.786, "difference": -0.714,
                            "where": "x"}]}
        text = "\n".join(computed.lines(data, ""))
        self.assertIn("[C] computed construction", text)
        self.assertIn("No playbook reference", text)
        self.assertIn("the guide says 1.5; computed says 0.786; difference -0.714", text)


if __name__ == "__main__":
    unittest.main()
