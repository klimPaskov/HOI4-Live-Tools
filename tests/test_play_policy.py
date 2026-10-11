"""Offline policy enforcement tests; none are live campaign evidence."""

from dataclasses import replace
import json
from pathlib import Path
import tempfile
import unittest

from hoi4_live.errors import ModeRefused
from hoi4_live.game.play_policy import POLICY_PATH, PlayPolicy, ReviewEvidence, request_digest


class PlayPolicyTests(unittest.TestCase):
    def make(self, mode="fair", **kwargs):
        return PlayPolicy(mode, session_id="campaign-a", monotonic=lambda: 100.0, **kwargs)

    def proof(self, policy, tool, args, kind="fair_review_requirements"):
        rules = json.loads(POLICY_PATH.read_text(encoding="utf-8"))
        return ReviewEvidence("campaign-a", policy.sha256, request_digest(tool, args), 99.0,
                              "own_country_telemetry", frozenset(rules[kind]), "offline-test-review")

    def refused(self, policy, name, args=None, reason=None, **kwargs):
        with self.assertRaises(ModeRefused) as raised:
            policy.authorize(name, args or {}, **kwargs)
        if reason:
            self.assertEqual(raised.exception.details["reason"], reason)
        self.assertFalse(policy.audit_events()[-1]["allowed"])

    def test_default_is_fair_and_selection_is_logged(self):
        captured = []
        policy = self.make(audit=captured.append)
        self.assertEqual(policy.mode, "fair")
        self.assertEqual(captured[0]["reason"], "session_play_mode")
        self.assertEqual(len(policy.summary()["policy_sha256"]), 64)
        self.assertIn("no live semantic review provider", policy.summary()["enforcement_limits"])
        with self.assertRaises(AttributeError):
            policy.mode = "meta"

    def test_showcase_requires_fair(self):
        self.assertTrue(self.make(showcase=True).summary()["showcase"])
        with self.assertRaises(ModeRefused):
            self.make("meta", showcase=True)

    def test_country_specific_showcase_modes(self):
        self.assertEqual(self.make("meta", showcase="POL").summary()["showcase"], "POL")
        for country in ("GER", "CHI"):
            with self.subTest(country=country):
                self.assertEqual(self.make("fair", showcase=country).mode, "fair")
                with self.assertRaises(ModeRefused):
                    self.make("meta", showcase=country)
        with self.assertRaises(ModeRefused):
            self.make("fair", showcase="POL")
        for unknown in ("USA", "pol", "", 1, None):
            with self.subTest(showcase=unknown), self.assertRaises(ModeRefused):
                self.make(showcase=unknown)

    def test_unknown_modes_refused(self):
        for mode in ("test", "campaign", "FAIR", "", None):
            with self.subTest(mode=mode), self.assertRaises(ModeRefused):
                self.make(mode)

    def test_missing_policy_fails_closed(self):
        with tempfile.TemporaryDirectory() as tmp:
            with self.assertRaises(ModeRefused):
                self.make(policy_path=Path(tmp) / "missing.json")

    def test_invalid_policy_fails_closed(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "policy.json"
            for value in ("not json", "{}", "null"):
                path.write_text(value)
                with self.subTest(value=value), self.assertRaises(ModeRefused):
                    self.make(policy_path=path)

    def test_both_modes_reject_cheat_tools_and_raw_input(self):
        for mode in ("fair", "meta"):
            policy = self.make(mode)
            for name in ("hoi4_console", "hoi4_eval_effect", "hoi4_fire_event", "hoi4_scenario",
                         "hoi4_click", "hoi4_key", "hoi4_type", "hoi4_drag", "hoi4_scroll"):
                with self.subTest(mode=mode, tool=name):
                    self.refused(policy, name)

    def test_unknown_tool_and_action_fail_closed(self):
        policy = self.make("meta")
        self.refused(policy, "hoi4_write_memory", reason="unreviewed_tool")
        self.refused(policy, "hoi4_orders", {"action": "naval_invasion"}, reason="unreviewed_action")

    def test_registered_test_only_tools_cannot_bypass_policy(self):
        self.refused(self.make("meta"), "hoi4_status", test_only=True, reason="test_only_tool")

    def test_campaign_cannot_switch_to_test(self):
        for mode in ("fair", "meta"):
            self.refused(self.make(mode), "hoi4_lock", {"action": "set_mode", "mode": "test"},
                         reason="campaign_test_mode_escape")
            self.refused(self.make(mode), "hoi4_launch", {"mode": "test"})

    def test_debug_and_console_country_switch_rejected(self):
        for mode in ("fair", "meta"):
            self.refused(self.make(mode), "hoi4_launch", {"debug": True}, reason="debug_launch")
            self.refused(self.make(mode), "hoi4_new_game", {"tag": "GER"}, reason="console_tag_switch")

    def test_configured_launch_flags_are_reviewed(self):
        policy = self.make("meta")
        policy.validate_launch_arguments([])
        policy.validate_launch_arguments(["-skiplauncher", "-windowed"])
        for flags in (["-debug"], ["--debug"], ["-debug=yes"], ["-foo"], "-windowed"):
            with self.subTest(flags=flags), self.assertRaises(ModeRefused):
                policy.validate_launch_arguments(flags)

    def test_both_modes_reject_unfiltered_information(self):
        cases = [("hoi4_logs", {"log": "game"}), ("hoi4_logs", {"log": "text"}),
                 ("hoi4_telemetry", {"action": "read", "raw": True}),
                 ("hoi4_telemetry", {"action": "watch"}),
                 ("hoi4_state", {"raw": True}), ("hoi4_state", {"sections": ["script"]}),
                 ("hoi4_briefing", {"log_notes": 1}),
                 ("hoi4_save_query", {"query": "country"}),
                 ("hoi4_state", {"save": "fresh"})]
        for mode in ("fair", "meta"):
            for name, args in cases:
                with self.subTest(mode=mode, tool=name, args=args):
                    self.refused(self.make(mode), name, args)

    def test_normal_economy_and_observations_permitted(self):
        for mode in ("fair", "meta"):
            policy = self.make(mode)
            for name, args in [("hoi4_status", {}), ("hoi4_state", {}), ("hoi4_memory", {}),
                               ("hoi4_save_query", {"query": "info"}),
                               ("hoi4_research", {"action": "start", "tech": "basic_machine_tools"}),
                               ("hoi4_production", {"action": "fill"}),
                               ("hoi4_construction", {"action": "build"})]:
                with self.subTest(mode=mode, tool=name):
                    self.assertTrue(policy.authorize(name, args)["allowed"])

    def test_sensitive_fair_actions_require_server_review(self):
        for tool, action in [("hoi4_army", "design"), ("hoi4_army", "recruit"),
                             ("hoi4_army", "deploy"), ("hoi4_orders", "attack"),
                             ("hoi4_orders", "batch"), ("hoi4_orders", "paint"),
                             ("hoi4_front", "order"), ("hoi4_peace", "submit")]:
            with self.subTest(tool=tool, action=action):
                args = {"action": action}
                self.refused(self.make(), tool, args, reason="verified_review_unavailable")
                self.assertTrue(self.make("meta").authorize(tool, args)["allowed"])

    def test_intent_or_dictionary_is_not_evidence(self):
        policy = self.make()
        args = {"action": "attack", "intent": "fair", "no_exploits": True}
        self.refused(policy, "hoi4_orders", args, evidence={"all_checks": True},
                     reason="verified_review_unavailable")

    def test_bound_trusted_evidence_permits_fair_action(self):
        policy = self.make()
        args = {"action": "attack", "target": "123"}
        evidence = self.proof(policy, "hoi4_orders", args)
        self.assertTrue(policy.authorize("hoi4_orders", args, evidence=evidence)["allowed"])

    def test_evidence_is_session_policy_request_source_and_time_bound(self):
        policy = self.make()
        args = {"action": "attack", "target": "123"}
        evidence = self.proof(policy, "hoi4_orders", args)
        invalid = [replace(evidence, session_id="campaign-b"),
                   replace(evidence, policy_sha256="different"),
                   replace(evidence, request_sha256=request_digest("hoi4_orders", {"action": "hold"})),
                   replace(evidence, observed_monotonic=94.9),
                   replace(evidence, observed_monotonic=101),
                   replace(evidence, observed_monotonic=float("nan")),
                   replace(evidence, observed_monotonic="yesterday"),
                   replace(evidence, source="model_intention"),
                   replace(evidence, evidence_id=""), replace(evidence, checks=frozenset()),
                   replace(evidence, checks=set(evidence.checks))]
        for bad in invalid:
            with self.subTest(evidence=bad):
                self.refused(policy, "hoi4_orders", args, evidence=bad)

    def test_evidence_cannot_be_replayed_for_spam(self):
        policy = self.make()
        args = {"action": "attack", "target": "123"}
        evidence = self.proof(policy, "hoi4_orders", args)
        self.assertTrue(policy.authorize("hoi4_orders", args, evidence=evidence)["allowed"])
        self.refused(policy, "hoi4_orders", args, evidence=evidence,
                     reason="verified_review_unavailable")

    def test_review_cannot_waive_a_universal_prohibition(self):
        policy = self.make("meta")
        args = {"debug": True}
        self.refused(policy, "hoi4_launch", args, evidence=self.proof(policy, "hoi4_launch", args))

    def test_reload_requires_verified_interruption_resume_in_both_modes(self):
        args = {"save": "hoi4live_resume"}
        for mode in ("fair", "meta"):
            policy = self.make(mode)
            self.refused(policy, "hoi4_load", args, reason="verified_review_unavailable")
            evidence = self.proof(policy, "hoi4_load", args, "resume_review_requirements")
            self.assertTrue(policy.authorize("hoi4_load", args, evidence=evidence)["allowed"])

    def test_audit_failure_prevents_authorization(self):
        def fail(event):
            if event["tool"] != "session_selected":
                raise OSError("audit unavailable")
        policy = self.make(audit=fail)
        with self.assertRaises(OSError):
            policy.authorize("hoi4_research", {"action": "start"})

    def test_audit_is_bounded_and_does_not_include_arguments(self):
        policy = self.make()
        for i in range(300):
            policy.authorize("hoi4_journal", {"action": "add", "text": "private content"})
        events = policy.audit_events()
        self.assertEqual(len(events), 256)
        self.assertNotIn("private content", json.dumps(events))
        events[0]["allowed"] = False
        self.assertTrue(policy.audit_events()[0]["allowed"])

    def test_request_digest_deterministic_and_sensitive_to_changes(self):
        self.assertEqual(request_digest("x", {"b": 2, "a": 1}), request_digest("x", {"a": 1, "b": 2}))
        self.assertNotEqual(request_digest("x", {"a": 1}), request_digest("x", {"a": 2}))


if __name__ == "__main__":
    unittest.main()
