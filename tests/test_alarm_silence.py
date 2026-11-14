"""An alarm switched off in a plan carries its reason, and the schema and the validator take that reason.

A plan once set `alarms.research_slot_empty` to false because slots stayed empty on purpose and the alarm
would have repeated every run. The executor now holds that alarm back while it owns the slot
(native/src/plan_run.rs hold_owned_slot_alarms), so nothing is left to silence; a plan that still
switches an alarm off has to say why in the same object.
"""
import json
import os
import sys
import unittest

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.join(ROOT, "scripts"))

from hoi4facts import lint_plan, schema_check, validate  # noqa: E402

SCHEMA = os.path.join(ROOT, "skills", "hoi4-campaign-playbook", "plan.schema.json")


def silenced(plan, hand=False):
    lt = lint_plan.Lint.__new__(lint_plan.Lint)
    lt.plan, lt.findings, lt.hand = plan, [], hand
    lt.alarms_silenced()
    return lt.findings


class SilencedAlarms(unittest.TestCase):
    def test_an_alarm_switched_off_without_a_reason_is_an_error(self):
        found = silenced({"alarms": {"idle_civilian_factories": 4, "research_slot_empty": False}})
        self.assertEqual([(f["severity"], f["rule"], f["where"]) for f in found],
                         [("error", "alarms.silenced", "alarms.research_slot_empty")])
        self.assertIn("alarms.why is missing", found[0]["message"])
        self.assertIn("already waits while the plan's lists own the slot", found[0]["message"])
        # a numeric alarm set to null or false is switched off as well
        found = silenced({"alarms": {"political_power": None, "unassigned_divisions": False}})
        self.assertEqual(sorted(f["where"] for f in found), ["alarms.political_power", "alarms.unassigned_divisions"])
        self.assertNotIn("already waits", found[0]["message"])

    def test_the_reason_has_to_name_the_alarm_and_say_something(self):
        plan = {"alarms": {"open_events": False, "why": "open_events"}}
        self.assertEqual(len(silenced(plan)), 1)  # the bare code is no reason
        plan["alarms"]["why"] = "the brief answers every event itself"
        found = silenced(plan)
        self.assertEqual(len(found), 1)
        self.assertIn("does not name it", found[0]["message"])
        plan["alarms"]["why"] = "open_events: the player answers every event from its own event loop"
        self.assertEqual(silenced(plan), [])
        # one reason does not cover a second alarm it does not name
        plan["alarms"]["armies_over_max"] = False
        self.assertEqual([f["where"] for f in silenced(plan)], ["alarms.armies_over_max"])

    def test_thresholds_and_enabled_alarms_need_no_reason(self):
        plan = {"alarms": {"idle_civilian_factories": 4, "political_power": 160, "research_slot_empty": True,
                           "slot_empty_grace_days": 0}}
        self.assertEqual(silenced(plan), [])
        self.assertEqual(silenced({}), [])
        self.assertEqual(silenced({"alarms": "x"}), [])

    def test_phase_and_contingency_overrides_are_checked(self):
        plan = {"phases": [{"id": "p1", "overrides": {"alarms": {"border_uncovered": False}}}],
                "contingencies": [{"id": "c1", "then": {"overrides": {"alarms": {
                    "army_out_of_supply": False,
                    "why": "army_out_of_supply: the fallback line is held out of supply on purpose for a week"}}}},
                    {"id": "c2", "overrides": {"alarms": {"goal_at_risk": None}}}]}
        self.assertEqual(sorted(f["where"] for f in silenced(plan)),
                         ["contingencies[1].overrides.alarms.goal_at_risk", "phases[0].overrides.alarms.border_uncovered"])

    def test_a_hand_plan_gets_a_warning(self):
        found = silenced({"alarms": {"research_slot_empty": False}}, hand=True)
        self.assertEqual([f["severity"] for f in found], ["warning"])

    def test_the_rule_runs_with_the_others_and_is_documented(self):
        self.assertIn("alarms.silenced", lint_plan.__doc__)
        self.assertIn("alarms.silenced", lint_plan.NEW_RULES)
        import inspect
        self.assertIn("self.alarms_silenced", inspect.getsource(lint_plan.Lint.run))


class SchemaAndValidator(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        with open(SCHEMA, encoding="utf-8") as fh:
            cls.schema = json.load(fh)

    def test_schema_takes_the_reason_and_the_grace(self):
        props = self.schema["definitions"]["alarms"]["properties"]
        self.assertEqual(props["why"]["type"], "string")
        self.assertEqual(props["note"]["type"], "string")
        self.assertEqual(props["slot_empty_grace_days"]["default"], 2)
        plan = {"alarms": {"open_events": False, "slot_empty_grace_days": 3,
                           "why": "open_events: answered by the player's own loop"}}
        self.assertEqual(schema_check.validate(plan, self.schema), [])
        plan["alarms"]["why"] = 3
        self.assertTrue(schema_check.validate(plan, self.schema))

    def test_validator_takes_text_only_for_the_reason(self):
        rep = validate.Report()
        validate.check_misc({"alarms": {"open_events": False, "why": "open_events: answered elsewhere",
                                        "note": "x"}}, rep)
        self.assertEqual([e for e in rep.errors if "alarms" in e], [])
        rep = validate.Report()
        validate.check_misc({"alarms": {"why": 3, "open_events": "no"}}, rep)
        self.assertEqual(sorted(e for e in rep.errors if "alarms" in e),
                         ["alarms.open_events: must be a number, boolean or null", "alarms.why: must be text"])


if __name__ == "__main__":
    unittest.main()
