"""Offline governor acceptance: choices, retry safety, evidence and byte budgets."""

import copy
import json
import unittest

from helpers import FIXTURES  # noqa: F401 (adds the repository to sys.path)
from hoi4_live.errors import ToolError
from hoi4_live.state.governor import StandingPlan, daily_events, digest, fresh_section, wait_matches
from hoi4_live.protocol.schema import validate
from hoi4_live.tools import registry


def observation(day=100, **sections):
    return dict(day=day, date="1936.1.1", tag="GER", source="telemetry", as_of_day=day, **sections)


def research(idle=1, **extra):
    return dict(idle=idle, available=["a", "b"], completed=[], active=[], **extra)


class GovernorTests(unittest.TestCase):
    def test_available_preferences_completed_and_active_are_skipped(self):
        plan = StandingPlan({"research": ["locked", "completed", "active", "b", "a"]})
        snap = observation(research={"idle": 2, "available": ["completed", "active", "a", "b"],
                                     "completed": ["completed"], "active": ["active"]})
        result = plan.reconcile(snap)
        self.assertEqual(result["actions"][0]["args"], {"action": "start", "tech": "b"})
        self.assertEqual(len(result["actions"]), 1)
        self.assertEqual(plan.plan["research"], ["locked", "completed", "active", "b", "a"])

    def test_missing_available_or_idle_is_not_permission_to_act(self):
        for state in ({"idle": 1}, {"available": ["a"]}, {"idle": None, "available": ["a"]}):
            self.assertEqual(StandingPlan({"research": ["a"]}).reconcile(observation(research=state))["actions"], [])

    def test_explicit_freshness_and_provenance_required(self):
        snap = observation(research=research())
        for patch in ({"as_of_day": None}, {"as_of_day": 99}, {"as_of_day": 101}, {"source": ""}, {"day": None}):
            changed = dict(snap, **patch)
            result = StandingPlan({"research": ["a"]}).reconcile(changed)
            self.assertEqual(result["actions"], [])
            self.assertIsNone(result["metrics"]["idle_research_slots"])
        snap["research"].update(source="screen", as_of_day=100)
        snap["as_of_day"] = 99
        self.assertIsNotNone(fresh_section(snap, "research"))

    def test_transient_failures_preserve_preferences_and_back_off_absolute_days(self):
        plan = StandingPlan({"research": ["a", "b"], "retry": {"max_attempts": 3, "backoff_days": 2}})
        action = plan.reconcile(observation(research=research()))["actions"][0]
        self.assertEqual(plan.record(action["id"], {"isError": True}, 100), "failed")
        self.assertEqual(plan.reconcile(observation(101, research=research()))["actions"], [])
        retry = plan.reconcile(observation(102, research=research()))["actions"][0]
        self.assertEqual(retry["id"], action["id"])
        self.assertEqual(retry["args"]["tech"], "a")
        plan.record(retry["id"], {"isError": True}, 102)
        self.assertEqual(plan.reconcile(observation(105, research=research()))["actions"], [])
        retry = plan.reconcile(observation(106, research=research()))["actions"][0]
        plan.record(retry["id"], {"isError": True}, 106)
        out = plan.reconcile(observation(120, research=research()))
        self.assertEqual(out["actions"], [])
        self.assertTrue(out["stop_reasons"][0].startswith("attempt_limit:"))
        self.assertEqual(plan.plan["research"], ["a", "b"])
        self.assertEqual(out["report"]["tool_failures"], 3)

    def test_deferred_result_is_not_success_or_failure_and_does_not_burn_choice(self):
        plan = StandingPlan({"focus": ["GER_a"]})
        snap = observation(focus={"idle": True, "available": ["GER_a"]})
        action = plan.reconcile(snap)["actions"][0]
        self.assertTrue(action["args"]["if_idle"])
        self.assertEqual(plan.record(action["id"], {"structuredContent": {"deferred": True}}, 100), "deferred")
        self.assertEqual(plan.reconcile(snap)["actions"], [])
        self.assertEqual(plan.report()["tool_failures"], 0)

    def test_attempts_are_only_counted_when_executor_records(self):
        plan = StandingPlan({"research": ["a"]})
        first = plan.reconcile(observation(research=research()))
        second = plan.reconcile(observation(research=research()))
        self.assertEqual(first, second)
        self.assertEqual(plan.state["attempts"], {})
        action = first["actions"][0]
        plan.record(action["id"], {}, 100)
        with self.assertRaises(ToolError):
            plan.record(action["id"], {}, 100)
        with self.assertRaises(ToolError):
            plan.record("not-a-proposal", {}, 101)

    def test_success_waits_for_later_day_before_repairing_again(self):
        plan = StandingPlan({"construction": [{"building": "industrial_complex", "state": "64", "target": 4}]})
        snap = observation(construction={"items": [{"building": "industrial_complex", "state": "64", "built": 2,
                                                    "queued": 1, "available": True, "free_slots": 3}]})
        action = plan.reconcile(snap)["actions"][0]
        self.assertEqual(action["args"]["count"], 1)
        plan.record(action["id"], {}, 100)
        self.assertEqual(plan.reconcile(snap)["actions"], [])
        snap = copy.deepcopy(snap)
        snap.update(day=101, as_of_day=101)
        snap["construction"]["items"][0]["queued"] = 2
        self.assertEqual(plan.reconcile(snap)["actions"], [])

    def test_target_does_not_build_on_unknown_counts_or_slots(self):
        plan = StandingPlan({"construction": [{"building": "industrial_complex", "state": "64", "target": 4}]})
        for missing in ("built", "queued", "available", "free_slots"):
            item = {"building": "industrial_complex", "state": "64", "built": 0, "queued": 0,
                    "available": True, "free_slots": 4}
            del item[missing]
            self.assertEqual(plan.reconcile(observation(construction={"items": [item]}))["actions"], [])

    def test_fill_production_requires_known_idle_factory_and_known_line(self):
        plan = StandingPlan({"production": [{"line": "1", "max": 8}, {"line": "2", "max": 3}]})
        snap = observation(production={"lines": [{"line": "1", "factories": 4}]}, metrics={"idle_military_factories": 2})
        actions = plan.reconcile(snap)["actions"]
        self.assertEqual(actions[0]["args"], {"action": "fill", "targets": [{"line": "1", "max": 8}]})
        snap["metrics"]["idle_military_factories"] = None
        self.assertEqual(plan.reconcile(snap)["actions"], [])

    def test_army_assignment_requires_existing_army_and_unassigned_state(self):
        plan = StandingPlan({"army": [{"state": "64", "army": "Home"}]})
        snap = observation(army={"armies": ["Home"], "unassigned_by_state": {"64": 3}, "selection_safe_by_state": {"64": True}})
        action = plan.reconcile(snap)["actions"][0]
        self.assertEqual(action["args"], {"action": "assign", "state": "64", "army": "Home", "zoom": "province"})
        snap["army"]["armies"] = []
        self.assertEqual(plan.reconcile(snap)["actions"], [])

    def test_army_unsafe_or_unknown_selection_only_requests_inspection(self):
        plan = StandingPlan({"army": [{"state": "64", "army": "Home"}]})
        for safe in (None, {"64": False}):
            snap = observation(army={"armies": ["Home"], "unassigned_by_state": {"64": 3}, "selection_safe_by_state": safe})
            out = plan.reconcile(snap)
            self.assertEqual(out["actions"], [])
            self.assertEqual(out["alerts"][0]["code"], "army_selection_unknown")
            self.assertNotIn("fix", out["alerts"][0])

    def test_success_cannot_retry_with_observation_older_than_execution(self):
        plan = StandingPlan({"research": ["a"], "max_age_days": 10})
        action = plan.reconcile(observation(research=research()))["actions"][0]
        plan.record(action["id"], {}, 103)
        snap = observation(104, research=research())
        snap["research"]["as_of_day"] = 101
        self.assertEqual(plan.reconcile(snap)["actions"], [])
        snap["research"]["as_of_day"] = 104
        self.assertEqual(len(plan.reconcile(snap)["actions"]), 1)

    def test_confirmed_production_progress_resets_consecutive_retry_limit(self):
        plan = StandingPlan({"production": [{"line": "1", "max": 8}], "retry": {"max_attempts": 2}})
        for offset in range(6):
            day = 100 + offset
            snap = observation(day, production={"lines": [{"line": "1", "factories": offset}]}, metrics={"idle_military_factories": 1})
            out = plan.reconcile(snap)
            self.assertEqual(len(out["actions"]), 1)
            plan.record(out["actions"][0]["id"], {}, day)
        self.assertEqual(plan.report()["tool_failures"], 0)

    def test_stale_policy_data_never_adds_daily_metric_samples(self):
        plan = StandingPlan({"max_age_days": 5})
        snap = observation(105, research={"idle": 2, "as_of_day": 100})
        out = plan.reconcile(snap)
        self.assertEqual(out["metrics"]["idle_research_slots"], 2)
        self.assertEqual(out["report"]["idle_research_slot_days"], 0)
        self.assertEqual(out["report"]["metric_samples"]["idle_research_slots"], {"known": 0, "unknown": 1})

    def test_unconfirmed_successes_are_still_bounded(self):
        plan = StandingPlan({"production": [{"line": "1", "max": 8}], "retry": {"max_attempts": 2}})
        for day in (100, 101):
            snap = observation(day, production={"lines": [{"line": "1", "factories": 1}]}, metrics={"idle_military_factories": 1})
            action = plan.reconcile(snap)["actions"][0]
            plan.record(action["id"], {}, day)
        out = plan.reconcile(observation(102, production={"lines": [{"line": "1", "factories": 1}]}, metrics={"idle_military_factories": 1}))
        self.assertEqual(out["actions"], [])
        self.assertTrue(out["stop_reasons"][0].startswith("attempt_limit:"))

    def test_events_need_explicit_policy_and_block_other_actions(self):
        snap = observation(events={"open": [{"id": "x", "index": 0, "title": "A choice", "options": ["a", "b"]}]},
                           research=research())
        out = StandingPlan({"research": ["a"]}).reconcile(snap)
        self.assertEqual(out["actions"], [])
        self.assertEqual(out["stop_reasons"], ["event_decision"])
        plan = StandingPlan({"events": {"mode": "stop", "rules": [{"match": "A choice", "option": 1}]}, "research": ["a"]})
        out = plan.reconcile(snap)
        self.assertEqual(out["actions"][0]["args"], {"action": "answer", "popup": 0, "option": 1})
        self.assertEqual(len(out["actions"]), 1)

    def test_only_first_popup_answered_to_keep_indices_valid(self):
        snap = observation(events={"open": [{"id": "x", "index": 0, "options": ["ok"]},
                                             {"id": "y", "index": 1, "options": ["ok"]}]})
        out = StandingPlan({"events": {"mode": "choices"}}).reconcile(snap)
        self.assertEqual(len(out["actions"]), 1)
        self.assertEqual(out["actions"][0]["args"]["popup"], 0)

    def test_invalid_event_option_stops_and_failed_close_counts_failure(self):
        snap = observation(events={"open": [{"id": "x", "index": 0, "options": ["ok"]}]})
        plan = StandingPlan({"events": {"rules": [{"match": ".*", "option": 3}]}})
        self.assertEqual(plan.reconcile(snap)["stop_reasons"], ["event_decision"])
        plan = StandingPlan({"events": {"mode": "choices"}})
        action = plan.reconcile(snap)["actions"][0]
        self.assertEqual(plan.record(action["id"], {"structuredContent": {"closed": False}}, 100), "failed")
        self.assertEqual(plan.report()["tool_failures"], 1)

    def test_daily_event_transitions_do_not_invent_enemy_data(self):
        before = observation(99, research={"idle": 0, "completed": ["a"]})
        after = observation(research={"idle": 2, "completed": ["a", "b"]}, focus={"idle": True},
                            metrics={"idle_military_factories": 3}, events={"open": [{"id": "x"}]},
                            enemy={"divisions": 500, "source": "hidden-memory"})
        self.assertEqual(daily_events(before, after), ["research_idle", "tech_done", "focus_idle", "production_idle", "popup"])
        after["research"]["as_of_day"] = 99
        self.assertNotIn("tech_done", daily_events(before, after))

    def test_wait_predicates_unknown_never_equals_zero(self):
        snap = observation(metrics={"idle_military_factories": 0}, research=research())
        predicates = [{"after_days": 3}, {"event": "research_idle"},
                      {"section": "metrics", "field": "resource_deficit", "op": "eq", "value": 0},
                      {"section": "metrics", "field": "idle_military_factories", "op": "lte", "value": 0}]
        self.assertEqual(wait_matches(snap, predicates, observation(97)), [0, 1, 3])
        with self.assertRaises(ToolError):
            wait_matches(snap, [{"event": "hidden_enemy_ready"}])
        with self.assertRaises(ToolError):
            wait_matches(snap, [{"after_days": True}])

    def test_metrics_coverage_counts_only_observed_days_and_nulls_unknown(self):
        plan = StandingPlan({})
        out = plan.reconcile(observation(research={"idle": 2}, metrics={"idle_military_factories": 3}))
        self.assertIsNone(out["metrics"]["equipment_deficit"])
        plan.reconcile(observation(research={"idle": 2}, metrics={"idle_military_factories": 3}))
        out = plan.reconcile(observation(110, research={"idle": 1}))
        self.assertEqual(out["report"]["game_days_elapsed"], 10)
        self.assertEqual(out["report"]["observed_days"], 2)
        self.assertEqual(out["report"]["missing_game_days"], 9)
        self.assertEqual(out["report"]["idle_research_slot_days"], 3)
        self.assertEqual(out["report"]["idle_military_factory_days"], 3)
        self.assertEqual(out["report"]["metric_samples"]["idle_military_factories"], {"known": 1, "unknown": 1})

    def test_day_regression_does_not_reset_retry_or_campaign_metrics(self):
        plan = StandingPlan({"research": ["a"]})
        plan.reconcile(observation(120, research=research()))
        out = plan.reconcile(observation(100, research=research()))
        self.assertEqual(out["stop_reasons"], ["day_regressed"])
        self.assertEqual(out["actions"], [])
        self.assertEqual(plan.report()["last_day"], 120)

    def test_every_emitted_call_and_suggestion_has_existing_schema(self):
        plan = StandingPlan({"research": ["a"], "focus": ["GER_a"], "max_actions": 1,
                             "construction": [{"building": "industrial_complex", "state": "64", "target": 4}],
                             "production": [{"line": "1", "max": 8}], "army": [{"state": "64", "army": "Home"}]})
        out = plan.reconcile(observation(research=research(), focus={"idle": True, "available": ["GER_a"]},
                    metrics={"idle_military_factories": 2, "equipment_deficit": 400, "resource_deficit": 6},
                    production={"lines": [{"line": "1", "factories": 2}]},
                    construction={"items": [{"building": "industrial_complex", "state": "64", "built": 0,
                                             "queued": 0, "available": True, "free_slots": 4}]},
                    army={"armies": ["Home"], "unassigned_by_state": {"64": 1}}))
        self.assertEqual(len(out["actions"]), 1)
        calls = out["actions"] + [alert.get("fix", alert.get("inspect")) for alert in out["alerts"]]
        for call in calls:
            entry = registry.get(call["tool"])
            self.assertIsNotNone(entry)
            self.assertFalse(entry.test_only)
            validate(entry.schema, call["args"])
        self.assertNotIn("hoi4_trade", [call["tool"] for call in calls])

    def test_digest_valid_json_utf8_limit_preserves_complete_calls_and_nulls(self):
        plan = StandingPlan({"construction": [{"building": "industrial_complex", "state": str(i), "target": 2} for i in range(1, 50)]})
        snap = observation(construction={"items": [{"building": "industrial_complex", "state": str(i), "built": 0,
                       "queued": 0, "available": True, "free_slots": 4} for i in range(1, 50)]})
        snap["tag"] = "\u6771\u4eac"
        out = plan.reconcile(snap)
        for limit in (512, 1024, 2048):
            encoded = digest(out, snap, limit)
            self.assertLessEqual(len(encoded.encode("utf-8")), limit)
            data = json.loads(encoded)
            self.assertGreater(data["omitted_alerts"], 0)
            self.assertIsNone(data["metrics"]["equipment_deficit"])
            for alert in data["alerts"]:
                call = alert["fix"]
                validate(registry.get(call["tool"]).schema, call["args"])

    def test_plan_copies_input_and_survives_exported_state(self):
        declaration = {"research": ["a"]}
        plan = StandingPlan(declaration)
        declaration["research"].append("b")
        self.assertEqual(plan.plan["research"], ["a"])
        action = plan.reconcile(observation(research=research()))["actions"][0]
        plan.record(action["id"], {"isError": True}, 100)
        restored = StandingPlan(plan.plan, json.loads(json.dumps(plan.state)))
        self.assertEqual(restored.reconcile(observation(101, research=research()))["actions"], [])

    def test_strict_bounded_plan_validation(self):
        bad_plans = [{"version": 2}, {"unknown": []}, {"max_actions": 200}, {"research": "a"},
                     {"research": ["a", "a"]}, {"retry": {"max_attempts": True}},
                     {"events": {"mode": "ignore"}}, {"events": {"rules": [{"match": "[", "option": 0}]}},
                     {"production": [{"line": "1", "max": 3}, {"line": "1", "max": 4}]},
                     {"army": [{"state": "64", "army": "A"}, {"state": "64", "army": "B"}]}]
        for plan in bad_plans:
            with self.assertRaises(ToolError, msg=str(plan)):
                StandingPlan(plan)


if __name__ == "__main__":
    unittest.main()
