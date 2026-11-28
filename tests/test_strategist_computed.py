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
        self.assertEqual(rows[1]["difference"], "differs")

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

    def test_summary_flattens_answers_and_skips_missing(self):
        data = {"construction": {"k": 1.786, "k_single_bonus": 1.5, "decision": "military"},
                "subdoctrine": {"tracks": [{"track": "infantry", "chosen": "assault_infantry"}]},
                "fuel": {"days_carried": 19.6, "target": 5000, "capacity": 9000, "stock_at_war": 8000, "per_war_day": 400},
                "training": {"in_training": 3, "binding": ["equipment"]},
                "focus": {"order": [{"focus": "A"}, {"focus": "B"}], "xp_price_idea": "x", "xp_price": 0.01, "pp_price": 0.06},
                "air": {"fighters": 10, "cas": 4}, "force": None, "errors": {}}
        s = computed.summary(data)
        self.assertEqual(s["construction_k"], 1.786)
        self.assertEqual(s["subdoctrines"], {"infantry": "assault_infantry"})
        self.assertEqual(s["focus_first"], ["A", "B"])
        self.assertEqual((s["air_fighters"], s["fuel_days_carried"], s["training_at_once"]), (10, 19.6, 3))
        self.assertNotIn("force_total", s)                                  # no force answer: left out
        rows = computed.compare([{"what": "summary.subdoctrines.infantry", "says": "commandos"}], {"summary": s})
        self.assertEqual(rows[0]["computed"], "assault_infantry")
        text = "\n".join(computed.lines(dict(data, tag="GER", date="1936.1.1", enemies=["POL"], enemies_how="given",
                                             war_date="1939.9.1", goals=[], references=[],
                                             subdoctrine={"grand": ["grand_battleplan"], "tracks": [
                                                 {"track": "infantry", "chosen": "assault_infantry"}]}), ""))
        self.assertIn("computed subdoctrines", text)
        self.assertIn("computed training: 3", text.replace(" line divisions", ""))


if __name__ == "__main__":
    unittest.main()
