"""Offline own-country observation coverage; no game input or launch."""

import json
import tempfile
import unittest
from unittest import mock

from helpers import FIXTURES  # noqa: F401
import test_governor_runtime as runtime
from hoi4_live.game.governor import telemetry_snapshot
from hoi4_live.state.governor import StandingPlan, digest, wait_matches


def observation(day=100, **sections):
    return dict(day=day, as_of_day=day, source="telemetry", tag="GER", **sections)


class FreshSectionsTests(unittest.TestCase):
    def setUp(self):
        folder = tempfile.TemporaryDirectory()
        self.addCleanup(folder.cleanup)
        self.session = runtime.FakeSession(folder.name)
        patch = mock.patch("hoi4_live.gamedata.tech.for_session", return_value={"techs": {}})
        patch.start()
        self.addCleanup(patch.stop)

    def add_economy(self, **extra):
        # Produce a complete record before its following-day completion marker.
        self.session.store.add(runtime.day(102))
        self.session.store.add(dict(k="eq", t="GER", n=102, _wall=102, _date="1936.1.1.24",
                                    infantry_equipment=10, infantry_equipment_army=80,
                                    infantry_equipment_need=100, **extra))
        self.session.store.add(dict(k="res", t="GER", n=102, _wall=102, _date="1936.1.1.24",
                                    steel=-4, steel_prod=12, aluminium=3, aluminium_prod=5))
        self.session.store.add(runtime.day(103))

    def test_exact_completed_own_country_shortages_keep_ids_and_narrow_scope(self):
        self.add_economy()
        snap = telemetry_snapshot(self.session)
        shortages = snap["shortages"]
        self.assertEqual(shortages["as_of_day"], 102)
        self.assertEqual(shortages["scope"], "observed_categories_only")
        self.assertEqual(shortages["equipment"][0]["army_deficit"], 10)
        self.assertEqual(shortages["equipment"][0]["id"], "infantry_equipment")
        self.assertEqual(shortages["resources"][1]["deficit"], 4)
        self.assertEqual(wait_matches(snap, [{"section": "shortages", "field": "army_deficit",
                                             "op": "gte", "value": 1}]), [0])
        result = StandingPlan({}).reconcile(snap)
        self.assertIsNone(result["metrics"]["equipment_deficit"])
        self.assertEqual({alert["id"] for alert in result["alerts"]}, {"steel", "infantry_equipment"})
        self.assertEqual(result["actions"], [])
        self.assertEqual(result["stop_reasons"], [])

    def test_missing_need_and_invalid_numbers_do_not_become_zero(self):
        self.add_economy(support_equipment=0, support_equipment_army=0,
                         artillery_equipment=float("nan"), artillery_equipment_army=1,
                         artillery_equipment_need=10, giant_equipment=10 ** 400,
                         giant_equipment_army=1, giant_equipment_need=2)
        snap = telemetry_snapshot(self.session)
        self.assertEqual([row["id"] for row in snap["shortages"]["equipment"]], ["infantry_equipment"])

    def test_partial_new_day_cannot_replace_completed_shortage_batch(self):
        self.add_economy()
        self.session.store.add(dict(k="eq", t="GER", n=103, _wall=103, _date="1936.1.1.24",
                                    infantry_equipment=100, infantry_equipment_army=80,
                                    infantry_equipment_need=100))
        snap = telemetry_snapshot(self.session)
        self.assertEqual(snap["shortages"]["army_deficit"], 10)
        self.assertEqual(snap["shortages"]["as_of_day"], 102)

    def test_old_and_foreign_records_do_not_create_current_shortages(self):
        self.add_economy()
        self.session.store.add(runtime.day(104))
        self.session.store.add(dict(k="eq", t="USA", n=103, _wall=104,
                                    infantry_equipment=0, infantry_equipment_army=0,
                                    infantry_equipment_need=900))
        snap = telemetry_snapshot(self.session, "GER")
        self.assertNotIn("shortages", snap)

    def test_unverified_generation_keeps_economy_unknown(self):
        self.add_economy()
        self.session.store.history["GER"][-1]["g"] = "other-generation"
        snap = telemetry_snapshot(self.session)
        self.assertNotIn("shortages", snap)
        self.assertEqual(snap["metrics"], {})

    def test_economy_observation_does_not_require_technology_catalog(self):
        self.add_economy()
        self.session.store.techs = []
        snap = telemetry_snapshot(self.session)
        self.assertIn("shortages", snap)
        self.assertNotIn("research", snap)
        self.assertEqual(snap["metrics"]["idle_military_factories"], 3)

    def test_unsupported_action_sections_remain_explicit(self):
        snap = telemetry_snapshot(self.session)
        self.assertEqual(set(snap["unavailable_observations"]), {"focus", "construction", "production", "army"})
        for section in snap["unavailable_observations"]:
            self.assertNotIn(section, snap)

    def test_daily_section_coverage_never_inflates_on_reconciliation_or_gaps(self):
        plan = StandingPlan({"max_age_days": 2})
        snap = observation(research={"idle": 1}, focus={"as_of_day": 99, "idle": True})
        plan.reconcile(snap)
        plan.reconcile(snap)
        plan.reconcile(observation(102))
        report = plan.report()
        self.assertEqual(report["observed_days"], 2)
        self.assertEqual(report["missing_game_days"], 1)
        self.assertEqual(report["section_samples"]["research"], {"fresh": 1, "unknown_or_stale": 1})
        self.assertEqual(report["section_samples"]["focus"], {"fresh": 0, "unknown_or_stale": 2})

    def test_digest_exposes_unknown_sections_and_keeps_byte_limit(self):
        snap = observation(research={"idle": 1})
        result = StandingPlan({}).reconcile(snap)
        encoded = digest(result, snap)
        self.assertEqual(json.loads(encoded)["observations"]["research"], "fresh")
        self.assertEqual(json.loads(encoded)["observations"]["production"], "unknown")
        self.assertLessEqual(len(digest(result, snap, 512).encode("utf-8")), 512)

    def test_stale_shortages_do_not_alert_and_valid_observations_survive_restart(self):
        plan = StandingPlan({})
        snap = observation(shortages={"as_of_day": 99, "equipment": [
            {"id": "infantry_equipment", "army_deficit": 10}]})
        self.assertEqual(plan.reconcile(snap)["alerts"], [])
        restored = StandingPlan(plan.plan, json.loads(json.dumps(plan.state)))
        restored.reconcile(observation(101))
        self.assertEqual(restored.report()["section_samples"]["shortages"]["unknown_or_stale"], 2)


if __name__ == "__main__":
    unittest.main()
