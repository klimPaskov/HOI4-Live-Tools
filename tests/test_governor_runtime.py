"""Owned runtime safety and condition waits; no process/window/input access."""

import copy
import json
import os
import tempfile
import threading
import time
import unittest
from types import SimpleNamespace
from unittest import mock

from helpers import FIXTURES  # noqa: F401
from hoi4_live.errors import ToolError
from hoi4_live.game.governor import Governor, telemetry_snapshot, STRATEGIC_STOP
from hoi4_live.state.standing import StandingOrders
from hoi4_live.telemetry.parser import TelemetryStore
from hoi4_live.tools import governor_tools
from hoi4_live.protocol.schema import validate
from hoi4_live.tools import registry


def day(n, **extra):
    return dict(k="day", t="GER", n=n, g="test-generation", v=3, _date="1936.1.1.24", _wall=n,
                rslots=2, techs=1, mil_free=3, civ_free=4, **extra)


class FakeLock:
    sticky = True
    held = True

    def held_by_us(self):
        return self.held


class FakeRun:
    def __init__(self):
        self.cancel = threading.Event()
        self.done = threading.Event()
        self.run = {"auto": [], "left_open": []}

    def stop(self):
        self.cancel.set()


class FakeSession:
    def __init__(self, folder):
        self.mode = "campaign"
        self.owned = True
        self.owned_checks = 0
        self.reads = 0
        self.lock = FakeLock()
        self.ui_mutex = threading.RLock()
        self.window = SimpleNamespace(size=lambda: (1920, 1080))
        self.cfg = SimpleNamespace(artifacts_dir=folder)
        self.standing = StandingOrders()
        self.clock = None
        self.governor = None
        self._call = threading.local()
        self.store = TelemetryStore()
        self.store.set_catalog(["known", "safe", "ahead", "scripted", "branch", "doctrine", "offscreen"])
        self.store.add(dict(k="hello", t="GER", g="test-generation", v=3, _wall=1))
        self.store.add(day(100))
        self.store.add(dict(k="kt", t="GER", n=100, tid=0, _wall=100))
        self.store.add(dict(k="rs", t="GER", n=100, tid=0, _wall=100))
        self.store.add(day(101))

    def require_owned_game(self):
        self.owned_checks += 1
        if not self.owned:
            raise ToolError("unowned_process")

    @property
    def telemetry(self):
        self.reads += 1
        return self.store

    def grab(self, *args, **kwargs):
        return object()


def index():
    technologies = {}
    for order, name in enumerate(("known", "safe", "ahead", "scripted", "branch", "doctrine", "offscreen")):
        technologies[name] = dict(id=name, order=order, folders=[{"name": "industry_folder", "x": 0, "y": 0}],
            year=1938 if name == "ahead" else 1936, cost=1, parents=[], deps=[], xor=[], categories=[], equipment=[],
            doctrine=name == "doctrine", allow="unknown trigger" if name == "scripted" else None,
            allow_branch="unknown branch" if name == "branch" else None)
    return {"techs": technologies, "folders": {"industry_folder": {"available": None}},
            "layout": {"industry_folder": {"gridboxes": {"x": {}}}}}


class RuntimeTests(unittest.TestCase):
    def setUp(self):
        self.folder = tempfile.TemporaryDirectory()
        self.addCleanup(self.folder.cleanup)
        self.session = FakeSession(self.folder.name)
        self.runtime = Governor(self.session)
        self.session.governor = self.runtime
        p = mock.patch("hoi4_live.gamedata.tech.for_session", return_value=index())
        p.start()
        self.addCleanup(p.stop)
        p = mock.patch("hoi4_live.gamedata.tech.tile_offset", side_effect=lambda idx, tid, folder, width: (3000, 10) if tid == "offscreen" else (100, 100))
        p.start()
        self.addCleanup(p.stop)
        p = mock.patch("hoi4_live.perception.popups.detect", return_value=[])
        self.popup_detector = p.start()
        self.addCleanup(p.stop)
        self.calls = []

        def dispatch(session, name, args, **kwargs):
            self.calls.append((name, copy.deepcopy(args)))
            if name == "hoi4_advance":
                session.clock = FakeRun()
                return {"isError": False, "structuredContent": {"background": True}}
            return {"isError": False, "structuredContent": {"changed": True}}

        p = mock.patch("hoi4_live.tools.dispatch.call", side_effect=dispatch)
        self.dispatch = p.start()
        self.addCleanup(p.stop)

    def configure(self, **extra):
        return self.runtime.set_plan(dict(research=["safe"], **extra))

    def test_completed_batch_and_conservative_research_candidates(self):
        snap = telemetry_snapshot(self.session)
        self.assertEqual(snap["day"], 100)
        self.assertEqual(snap["latest_day"], 101)
        self.assertEqual(snap["research"]["idle"], 1)
        self.assertEqual(snap["research"]["available"], ["safe"])
        self.assertNotIn("equipment_deficit", snap["metrics"])
        self.assertNotIn("resource_deficit", snap["metrics"])

    def test_latest_research_batch_is_not_assumed_complete(self):
        self.session.store.add(dict(k="rs", t="GER", n=101, tid=1, _wall=101))
        snap = telemetry_snapshot(self.session)
        self.assertEqual(snap["research"]["active"], ["known"])
        self.assertEqual(snap["day"], 100)

    def test_mixed_country_untagged_counts_cannot_authorize_action(self):
        self.session.store.add(dict(day(101), t="USA", _wall=102))
        snap = telemetry_snapshot(self.session, "GER")
        self.assertNotIn("research", snap)
        self.assertIn("unverified", snap["research_unknown"])

    def test_kt_scan_requires_following_day_and_matches_explicit_count(self):
        self.session.store.history["GER"][-1]["techs"] = 2
        self.session.store.add(dict(k="kt", t="GER", n=101, tid=1, _wall=110))
        snap = telemetry_snapshot(self.session)
        self.assertNotIn("available", snap["research"])
        self.session.store.add(dict(day(102), techs=2))
        snap = telemetry_snapshot(self.session)
        self.assertNotIn("available", snap["research"])

    def test_missing_catalog_or_generation_leaves_research_unknown(self):
        self.session.store.techs = []
        snap = telemetry_snapshot(self.session)
        self.assertNotIn("research", snap)

    def test_telemetry_gap_never_exposes_old_idle_as_current(self):
        self.session.store.add(day(105))
        snap = telemetry_snapshot(self.session)
        self.assertEqual(snap["day"], 104)
        self.assertEqual(snap["as_of_day"], 101)
        self.assertNotIn("research", snap)
        self.assertIn("stale", snap["research_unknown"])

    def test_start_requires_ownership_sticky_lock_campaign_before_telemetry(self):
        self.configure()
        for attribute, value in (("owned", False), ("mode", "test")):
            old = getattr(self.session, attribute)
            setattr(self.session, attribute, value)
            before = self.session.reads
            with self.assertRaises(ToolError):
                self.runtime.start()
            self.assertEqual(self.session.reads, before)
            setattr(self.session, attribute, old)
        self.session.lock.sticky = False
        with self.assertRaises(ToolError):
            self.runtime.start()
        self.assertEqual(self.calls, [])

    def test_start_rejects_unsupported_components_without_partial_activation(self):
        self.configure(army=[{"state": "64", "army": "1"}])
        with self.assertRaises(ToolError) as error:
            self.runtime.start()
        self.assertEqual(error.exception.details["unsupported_state"], ["army"])
        self.assertFalse(self.runtime.active())
        self.assertEqual(self.calls, [])

    def test_start_rejects_legacy_standing_conflict(self):
        self.configure()
        self.session.standing.set("legacy", ["research_idle"], do={"tool": "hoi4_research", "args": {"action": "start", "tech": "safe"}})
        with self.assertRaises(ToolError):
            self.runtime.start()
        self.assertEqual(self.calls, [])

    def test_start_uses_existing_fast_background_clock_and_declared_policy(self):
        events = {"mode": "stop", "rules": [{"match": "News", "option": 0}]}
        self.configure(events=events)
        self.runtime.start(days=30, speed=4, max_seconds=600)
        name, args = self.calls[0]
        self.assertEqual(name, "hoi4_advance")
        self.assertEqual(args["stop_on"], STRATEGIC_STOP)
        self.assertEqual(args["popup_rules"], events["rules"])
        self.assertEqual(args["popups"], "stop")
        self.assertTrue(args["background"])
        self.assertTrue(self.runtime.active())

    def test_daily_hook_uses_telemetry_instead_of_injected_observation(self):
        self.configure()
        self.runtime.start()
        self.calls.clear()
        self.runtime.on_day(self.session.clock, {"research": {"available": ["evil"], "idle": 999}, "n": 999999})
        self.assertEqual(self.calls[0][0], "hoi4_research")
        self.assertEqual(self.calls[0][1]["tech"], "safe")
        self.assertEqual(self.calls[0][1]["save"], "telemetry")
        self.assertEqual(self.runtime.snapshot["day"], 100)
        self.assertEqual(self.runtime.policy.state["attempts"][next(iter(self.runtime.policy.state["attempts"]))]["last_day"], 101)

    def test_slow_action_uses_actual_execution_day_and_cannot_replay_old_state(self):
        self.configure()
        self.runtime.start()
        self.calls.clear()

        def slow(session, name, args, **kwargs):
            self.calls.append((name, args))
            session.store.add(day(104))
            return {"isError": False}

        self.dispatch.side_effect = slow
        self.runtime.on_day(self.session.clock, {})
        attempt = next(iter(self.runtime.policy.state["attempts"].values()))
        self.assertEqual(attempt["last_day"], 104)
        self.assertEqual(self.runtime.snapshot["day"], 103)
        self.assertNotIn("research", self.runtime.snapshot)
        self.runtime.on_day(self.session.clock, {})
        self.assertEqual(len(self.calls), 1)

    def test_lost_ownership_or_lease_stops_without_new_read_or_action(self):
        self.configure()
        self.runtime.start()
        self.calls.clear()
        before = self.session.reads
        self.session.owned = False
        reasons = self.runtime.on_day(self.session.clock, {})
        self.assertEqual(reasons[0]["code"], "governor_stopped")
        self.assertFalse(self.runtime.active())
        self.assertEqual(self.session.reads, before)
        self.assertEqual(self.calls, [])

    def test_generation_change_during_action_does_not_consume_plan(self):
        self.configure()
        self.runtime.start()
        self.calls.clear()
        def changed_generation(session, name, args, **kwargs):
            session.store.add(dict(day(102), g="different-generation"))
            session.store.add(dict(day(103), g="different-generation"))
            return {"isError": False}
        self.dispatch.side_effect = changed_generation
        reasons = self.runtime.on_day(self.session.clock, {})
        self.assertEqual(reasons[0]["code"], "governor_stopped")
        self.assertIn("during governor action", reasons[0]["text"])
        self.assertEqual(self.runtime.policy.state["attempts"], {})
        self.assertFalse(self.runtime.active())

    def test_refreshed_blocking_reason_is_returned_without_waiting_for_next_day(self):
        self.configure()
        self.runtime.start()
        original = self.runtime.policy.reconcile
        calls = []
        def reconcile(snapshot):
            result = original(snapshot)
            calls.append(snapshot["day"])
            if len(calls) == 2:
                result["stop_reasons"] = ["decision:observed after action"]
            return result
        with mock.patch.object(self.runtime.policy, "reconcile", side_effect=reconcile):
            reasons = self.runtime.on_day(self.session.clock, {})
        self.assertEqual(reasons, [{"code": "governor_decision", "text": "decision:observed after action"}])
        self.assertFalse(self.runtime.active())

    def test_telemetry_rewind_stops_and_requires_explicit_restart(self):
        self.configure()
        self.runtime.start()
        self.session.store.add(day(90))
        self.session.store.add(day(91))
        reasons = self.runtime.on_day(self.session.clock, {})
        self.assertEqual(reasons[0]["code"], "governor_stopped")
        self.assertFalse(self.runtime.active())

    def test_cached_status_digest_do_not_read_game_and_json_is_bounded(self):
        self.configure()
        self.runtime.start()
        before = self.session.reads
        self.runtime.status()
        encoded = self.runtime.compact_digest()
        json.loads(encoded)
        self.assertLessEqual(len(encoded.encode("utf-8")), 2048)
        self.assertEqual(self.session.reads, before)

    def test_popup_preflight_defers_repair_to_declared_clock_policy(self):
        self.configure(events={"mode": "stop"})
        self.runtime.start()
        self.calls.clear()
        self.popup_detector.return_value = [object()]
        self.runtime.on_day(self.session.clock, {})
        self.assertEqual(self.calls, [])
        self.assertEqual(self.runtime.policy.state["attempts"], {})

    def test_adapter_also_refuses_unowned_reads_directly(self):
        self.session.owned = False
        with self.assertRaises(ToolError):
            telemetry_snapshot(self.session)
        self.assertEqual(self.session.reads, 0)

    def test_durable_plan_reloads_inactive_and_preserves_retry_state(self):
        path = os.path.join(self.folder.name, "state", "governor.json")
        self.runtime = Governor(self.session, path)
        self.session.governor = self.runtime
        self.configure()
        self.runtime.start()
        self.runtime.on_day(self.session.clock, {})
        loaded = Governor(self.session, path)
        self.assertFalse(loaded.active())
        self.assertEqual(loaded.policy.plan["research"], ["safe"])
        self.assertEqual(loaded.policy.state["attempts"], self.runtime.policy.state["attempts"])

    def test_wait_condition_wakes_on_daily_event(self):
        self.configure()
        self.runtime.start()
        answers = []
        waiter = threading.Thread(target=lambda: answers.append(self.runtime.wait_for([{"after_days": 1}], timeout_s=2)))
        waiter.start()
        time.sleep(0.02)
        self.session.store.add(day(102))
        self.runtime.on_day(self.session.clock, {})
        waiter.join(1)
        self.assertFalse(waiter.is_alive())
        self.assertEqual(answers[0]["state"], "matched")

    def test_wait_wakes_on_stop_and_respects_hard_timeout(self):
        self.configure()
        self.runtime.start()
        answers = []
        waiter = threading.Thread(target=lambda: answers.append(self.runtime.wait_for([{"after_days": 20}], timeout_s=2)))
        waiter.start()
        time.sleep(0.02)
        self.runtime.on_stop(self.session.clock, [{"code": "war_started", "text": "war"}])
        waiter.join(1)
        self.assertFalse(waiter.is_alive())
        self.assertEqual(answers[0]["state"], "stopped")
        with self.assertRaises(ToolError):
            self.runtime.wait_for([{"after_days": 20}], timeout_s=61)

    def test_wait_cancel_and_unknown_data_cannot_satisfy_predicate(self):
        self.configure()
        self.runtime.start()
        predicate = [{"section": "metrics", "field": "equipment_deficit", "op": "eq", "value": 0}]
        result = self.runtime.wait_for(predicate, timeout_s=0)
        self.assertEqual(result["state"], "timeout")
        cancel = threading.Event()
        cancel.set()
        self.assertEqual(self.runtime.wait_for(predicate, cancel=cancel)["state"], "cancelled")

    def test_stop_does_not_read_or_act_and_cancels_our_clock(self):
        self.configure()
        self.runtime.start()
        before = self.session.reads
        self.session.owned = False
        self.runtime.stop()
        self.assertEqual(self.session.reads, before)
        self.assertTrue(self.session.clock.cancel.is_set())

    def test_mcp_tools_disallow_injected_state_and_validate_wait_budget(self):
        self.assertIsNotNone(registry.get("hoi4_governor"))
        for name, args in (("hoi4_governor", {"action": "start", "observations": {}}),
                           ("hoi4_wait_for", {"predicates": [{"after_days": 1}], "timeout_s": 61})):
            with self.assertRaises(ToolError):
                validate(registry.get(name).schema, args)
        with self.assertRaises(ToolError):
            governor_tools.hoi4_governor(self.session, {"action": "plan"})


if __name__ == "__main__":
    unittest.main()
