"""Completed-day metric retention, independent of the live game."""

import unittest

from hoi4_live.telemetry.parser import TelemetryStore


class DailyMetricHistoryTests(unittest.TestCase):
    def day(self, store, n, tag="GER", generation="G1", schema=3):
        row = {"k": "day", "t": tag, "n": n, "g": generation, "v": schema,
               "_date": "1936.1.{}.1".format(n), "_wall": float(n)}
        store.add(row)

    def metric(self, store, kind, n, value, tag="GER", **extra):
        row = {"k": kind, "t": tag, "n": n, "_date": "1936.1.{}.1".format(n), "_wall": float(n)}
        row.update({"steel": value} if kind == "res" else {"infantry_equipment": value})
        row.update(extra)
        store.add(row)

    def test_completed_day_survives_newer_metrics(self):
        store = TelemetryStore()
        for n in (1, 2):
            self.day(store, n)
            self.metric(store, "eq", n, n * 10)
            self.metric(store, "res", n, -n)
        self.assertEqual(store.get("eq", "GER")["infantry_equipment"], 20)
        old = store.get_at("eq", "GER", 1)
        self.assertEqual(old["infantry_equipment"], 10)
        self.assertEqual((old["_generation"], old["_schema"]), ("G1", 3))
        self.assertEqual(store.get_at("res", "GER", 1)["steel"], -1)

    def test_metrics_require_exact_same_tag_day_anchor(self):
        store = TelemetryStore()
        self.metric(store, "eq", 1, 5)
        self.assertIsNone(store.get_at("eq", "GER", 1))
        self.day(store, 1)
        self.metric(store, "eq", 1, 5, tag="POL")
        self.metric(store, "res", 2, 9)
        self.assertIsNone(store.get_at("eq", "POL", 1))
        self.assertIsNone(store.get_at("res", "GER", 2))
        self.metric(store, "eq", 1, 15)
        self.assertEqual(store.get_at("eq", "GER", 1)["infantry_equipment"], 15)
        self.assertIsNone(store.get_at("eq", "GER", 2))

    def test_generation_change_clears_old_history_before_new_day(self):
        store = TelemetryStore()
        self.day(store, 1)
        self.metric(store, "eq", 1, 1)
        store.add({"k": "hello", "t": "GER", "n": 2, "g": "G2", "v": 3, "_wall": 2})
        self.assertIsNone(store.get_at("eq", "GER", 1))
        self.metric(store, "eq", 1, 10)
        self.assertIsNone(store.get_at("eq", "GER", 1))
        self.day(store, 2, generation="G2")
        self.metric(store, "eq", 2, 20)
        self.assertEqual(store.get_at("eq", "GER", 2)["_generation"], "G2")

    def test_schema_change_even_with_same_generation_clears_history(self):
        store = TelemetryStore()
        self.day(store, 1)
        self.metric(store, "res", 1, 1)
        self.day(store, 2, schema=4)
        self.assertIsNone(store.get_at("res", "GER", 1))
        self.metric(store, "res", 2, 10)
        self.assertEqual(store.get_at("res", "GER", 2)["_schema"], 4)

    def test_missing_schema_or_generation_cannot_attest_history(self):
        for key in ("v", "g"):
            store = TelemetryStore()
            row = {"k": "day", "t": "GER", "n": 1, "g": "G1", "v": 3, "_wall": 1}
            del row[key]
            store.add(row)
            self.metric(store, "eq", 1, 1)
            self.assertIsNone(store.get_at("eq", "GER", 1))

    def test_contradictory_explicit_metric_provenance_is_rejected(self):
        for extra in ({"g": "other"}, {"v": 4}):
            store = TelemetryStore()
            self.day(store, 1)
            self.metric(store, "eq", 1, 1, **extra)
            self.assertIsNone(store.get_at("eq", "GER", 1))

    def test_retention_bounded_by_day_history_even_without_new_metrics(self):
        store = TelemetryStore(history_days=2)
        self.day(store, 1)
        self.metric(store, "eq", 1, 1)
        self.day(store, 2)
        self.assertIsNotNone(store.get_at("eq", "GER", 1))
        self.day(store, 3)
        self.assertIsNone(store.get_at("eq", "GER", 1))
        self.assertEqual(store.daily_metrics["eq"]["GER"], {})
        for n in range(4, 20):
            self.day(store, n)
            self.metric(store, "eq", n, n)
        self.assertEqual(set(store.daily_metrics["eq"]["GER"]), {18, 19})

    def test_rewind_and_drop_after_remove_abandoned_metrics(self):
        store = TelemetryStore()
        for n in (1, 2, 3):
            self.day(store, n)
            self.metric(store, "eq", n, n)
        store.drop_after("1936.1.2.12")
        self.assertIsNone(store.get_at("eq", "GER", 3))
        self.assertIsNotNone(store.get_at("eq", "GER", 2))
        self.day(store, 2)
        self.assertIsNone(store.get_at("eq", "GER", 2))
        self.assertIsNotNone(store.get_at("eq", "GER", 1))

    def test_reset_discards_metrics_and_queries_are_copies(self):
        store = TelemetryStore()
        self.day(store, 1)
        self.metric(store, "eq", 1, 1)
        result = store.get_at("eq", "GER", 1)
        result["infantry_equipment"] = 999
        self.assertEqual(store.get_at("eq", "GER", 1)["infantry_equipment"], 1)
        store.reset()
        self.assertIsNone(store.get_at("eq", "GER", 1))

    def test_unsupported_queries_do_not_fall_back_to_latest(self):
        store = TelemetryStore()
        self.day(store, 1)
        self.metric(store, "eq", 1, 1)
        for kind, tag, n in (("day", "GER", 1), ("eq", "POL", 1), ("eq", None, 1),
                             ("eq", "GER", None), ("eq", "GER", True), ("eq", "GER", 999)):
            self.assertIsNone(store.get_at(kind, tag, n))


if __name__ == "__main__":
    unittest.main()
