"""Closed focus scans cannot turn missing/partial telemetry into completion."""
import tempfile
import unittest

import helpers  # noqa: F401
from hoi4_live.errors import ToolError
from hoi4_live.telemetry import focus, modgen
from hoi4_live.telemetry.parser import TelemetryStore
from hoi4_live.state.savefile import parse_block


class FocusGenerationTests(unittest.TestCase):
    def test_catalog_rejects_injection_duplicates_and_excess(self):
        for value in ([1], {1: []}, {"tree": ["A } log = bad"]},
                      {"tree": ["A", "A"]}, {"tree": ["F%d" % i for i in range(2049)]}):
            with self.subTest(value_type=type(value).__name__), self.assertRaises(ToolError):
                focus.catalog(value)
        self.assertEqual(focus.catalog({"tree": ["ETH_anti-tank_guns"]}),
                         {"tree": ["ETH_anti-tank_guns"]})

    def test_own_tree_readonly_script_and_provenance(self):
        spec = {"generation": "g", "campaign": True, "watch": modgen.empty_watch(),
                "techs": [], "resources": [], "equipment": [], "focuses": {"tree": ["A", "B"]}}
        text = focus.effects(spec["focuses"])
        parsed = parse_block(text.encode())
        self.assertIn("hoi4live_telemetry_focus", parsed)
        self.assertIn("is_ai = no", text)
        self.assertIn("has_focus_tree = tree", text)
        self.assertIn("has_completed_focus = A", text)
        self.assertIn("progress > 0.5", text)
        for forbidden in ("every_country", "every_enemy", "complete_national_focus", "set_variable ="):
            self.assertNotIn(forbidden, text)
        full = modgen.effects_text(spec)
        self.assertEqual(full.count("hoi4live_telemetry_focus = yes"), 1)
        lines = modgen.templates(spec)
        for kind in focus.KINDS:
            self.assertIn('"g":"g"', lines["hoi4live_tlm_" + kind])
            self.assertIn('"t":"[ROOT.GetTag]"', lines["hoi4live_tlm_" + kind])

    def test_manifest_binds_catalog_into_generation(self):
        with tempfile.TemporaryDirectory() as root:
            a = modgen.generate(root, None, campaign=True, techs=[], focuses={"tree": ["A"]})
            b = modgen.generate(root, None, campaign=True, techs=[], focuses={"tree": ["B"]})
        self.assertEqual(a["focuses"], {"tree": ["A"]})
        self.assertEqual(a["focus_observation_version"], 1)
        self.assertNotEqual(a["generation"], b["generation"])


class FocusBatchTests(unittest.TestCase):
    def setUp(self):
        self.store = TelemetryStore(history_days=2)
        self.store.set_focus_catalog({"tree": ["A", "B", "C"]}, "g")
        self.day = 1
        self.anchor()

    def row(self, kind, **extra):
        row = dict(k=kind, t="GER", n=self.day, g="g", v=modgen.SCHEMA,
                   tree=0, _date="1936.1.%d.1" % self.day, _wall=self.day)
        row.update(extra)
        self.store.add(row)

    def anchor(self):
        self.row("day")

    def batch(self):
        self.row("fb", total=3)
        self.row("fc", fid=0)
        self.row("fp", fid=1, low=50)
        self.row("fe", completed=1, progressed=1)

    def get(self):
        return self.store.focus_snapshot("GER", self.day)

    def test_closed_batch_has_ids_progress_bounds_but_no_active_claim(self):
        self.batch()
        result = self.get()
        self.assertEqual(result["completed"], ["A"])
        self.assertEqual(result["progress"]["B"], {"lower_exclusive": .5, "upper_inclusive": .6})
        self.assertIsNone(result["current"])
        self.assertFalse(result["current_known"])
        result["completed"].append("C")
        self.assertEqual(self.get()["completed"], ["A"])
        self.assertIsNone(self.store.focus_snapshot("POL", self.day))

    def test_empty_closed_batch_is_distinct_from_missing(self):
        self.assertIsNone(self.get())
        self.row("fb", total=3)
        self.assertIsNone(self.get())
        self.row("fe", completed=0, progressed=0)
        self.assertEqual(self.get()["completed"], [])

    def test_public_snapshot_never_substitutes_previous_day_focus(self):
        self.batch()
        self.assertEqual(self.store.snapshot("GER")["focus"]["completed"], ["A"])
        self.day = 2
        self.anchor()
        result = self.store.snapshot("GER")
        self.assertNotIn("focus", result)
        self.assertIn("focus_unknown", result)

    def test_dropped_row_or_end_never_confirms(self):
        self.row("fb", total=3)
        self.row("fc", fid=0)
        self.assertIsNone(self.get())
        self.row("fe", completed=2, progressed=0)
        self.assertIsNone(self.get())

    def test_duplicate_or_conflicting_focus_invalidates(self):
        for kind in ("fc", "fp"):
            self.setUp()
            self.row("fb", total=3)
            self.row("fc", fid=0)
            self.row(kind, fid=0, low=20)
            self.row("fe", completed=1, progressed=0)
            self.assertIsNone(self.get())

    def test_wrong_generation_schema_tag_date_and_index_refuse(self):
        for extra in ({"g": "other"}, {"v": 999}, {"v": True}, {"t": "POL"},
                      {"_date": "wrong"}, {"tree": True}, {"fid": -1}, {"fid": True},
                      {"fid": 3}, {"fid": 1.5}):
            self.setUp()
            self.row("fb", total=3)
            self.row("fc", **dict({"fid": 0}, **extra))
            self.row("fe", completed=1, progressed=0)
            self.assertIsNone(self.get())

    def test_bad_progress_and_counts_refuse(self):
        for low in (-10, 100, True, 21, 20.5, float("nan")):
            self.setUp()
            self.row("fb", total=3)
            self.row("fp", fid=1, low=low)
            self.row("fe", completed=0, progressed=1)
            self.assertIsNone(self.get())

    def test_replay_or_late_records_revoke_closed_batch(self):
        self.batch()
        self.row("fc", fid=2)
        self.assertIsNone(self.get())
        self.setUp()
        self.batch()
        self.batch()
        self.assertIsNone(self.get())

    def test_catalog_generation_and_timeline_changes_invalidate(self):
        self.batch()
        self.store.set_focus_catalog({"tree": ["B", "A", "C"]}, "g")
        self.assertIsNone(self.get())
        self.batch()
        self.row("hello", g="other")
        self.assertIsNone(self.get())
        self.setUp()
        self.batch()
        self.anchor()
        self.assertIsNone(self.get())
        self.batch()
        self.store.reset()
        self.assertIsNone(self.get())

    def test_bounded_history_and_wrong_manifest(self):
        for day in (1, 2, 3):
            self.day = day
            if day > 1:
                self.anchor()
            self.batch()
        self.assertIsNone(self.store.focus_snapshot("GER", 1))
        self.assertIsNotNone(self.store.focus_snapshot("GER", 2))
        self.store.set_focus_catalog({"tree": ["A", "B", "C"]}, "wrong")
        self.assertIsNone(self.get())


if __name__ == "__main__":
    unittest.main()
