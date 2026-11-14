"""The rolling planner's core: state, horizons, triggers, ledger, patches, the plan store and the budget."""

import json
import os
import tempfile
import unittest

from hoi4_live.planner import budget, horizon, ledger, patch, state, store, triggers


def overview(date, research, focus=None, paused=False, player="ITA"):
    return {"date": {"text": date}, "paused": paused, "player": player,
            "research": [{"slot": i, "researching": t} for i, t in enumerate(research)],
            "focus": {"key": focus} if focus else None}


def feed(*changes):
    return {"changes": [dict(c, seq=i + 1) for i, c in enumerate(changes)]}


PLAN = {
    "_meta": {"version": 3},
    "research_path": [{"year": 1936, "techs": ["a", "b", "c"]}],
    "research_priority": ["d"],
    "focus_path": ["f1", "f2"], "focus_fallback": ["f3"],
    "politics": [{"id": "p1", "idea": "x", "cost": 50}],
    "politics_auto": {"threshold": 0, "order": [{"category": "government", "slot": 3}]},
    "war": {"enemies": ["ETH"], "axes": [[842]]},
    "vp_targets": [{"state": 842, "province": 1}, {"state": 271, "province": 2}],
    "checkpoints": [{"id": "ck1", "by": "1936.02.01", "must": ["x"]}],
}
NOCK = dict(PLAN, checkpoints=[])


class StateTests(unittest.TestCase):
    def test_overview_feed_and_inference(self):
        b = state.StateBuilder()
        b.feed("hoi4_overview", overview("1936.01.01.12", ["a", "b", None], "f1", paused=True))
        b.feed("hoi4_overview", overview("1936.03.01.01", ["c", "b", None], "f2"))
        b.feed("hoi4_observe", feed(
            {"kind": "focus_finished", "date": "1936.03.01.01", "detail": {"focus": "f1"}},
            {"kind": "wake_player", "date": "1936.03.02.01", "detail": {
                "reason": "stockpile_low", "evidence": {"equipment": "artillery_equipment", "days_of_cover": 0.0}}},
            {"kind": "war_started", "date": "1936.03.03.01", "detail": {"with": "ETH"}}))
        s = b.snapshot()
        self.assertEqual(s["date"], "1936.03.03")
        self.assertEqual(s["slots"], 3)
        self.assertIn("f1", s["focuses_done"])
        # "a" left its slot while the game ran: researched
        self.assertIn("a", s["research_done"])
        self.assertEqual(s["shortages"]["artillery_equipment"]["days_of_cover"], 0.0)
        self.assertEqual(s["wars"], ["ETH"])
        self.assertEqual(len(b.take_events()), 3)
        self.assertEqual(b.take_events(), [])

    def test_a_feed_change_is_taken_once(self):
        b = state.StateBuilder()
        body = feed({"kind": "revision_requested", "date": "1936.01.02.01", "detail": {"reason": "x"}})
        b.feed("hoi4_wait_change", body)
        b.feed("hoi4_observe", body)
        self.assertEqual(len(b.take_events()), 1)

    def test_economy_gain_from_two_reads(self):
        b = state.StateBuilder()
        b.feed("hoi4_overview", overview("1936.01.01.01", []))
        b.feed("hoi4_economy", {"political_power": {"raw_signed": 10000000}, "civilian_factories_idle": 3})
        b.feed("hoi4_overview", overview("1936.01.11.01", []))
        b.feed("hoi4_economy", {"political_power": {"raw_signed": 12000000}})
        s = b.snapshot()
        self.assertEqual(s["pp"], 120.0)
        self.assertEqual(s["pp_gain"], 2.0)
        self.assertEqual(s["civ_idle"], 3)

    def test_dates(self):
        self.assertEqual(state.date_text(state.day_of("1936.1.1")), "1936.01.01")
        self.assertEqual(state.day_of("1936.03.01.12") - state.day_of("1936.02.28"), 2)
        self.assertIsNone(state.day_of("soon"))


class HorizonTests(unittest.TestCase):
    def snap(self, **kw):
        s = {"day": state.day_of("1936.06.01"), "slots": 2, "researching": ["a", None], "research_done": [],
             "focuses_done": [], "focus": "f1", "signals": {}, "shortages": {}, "fill_low": {},
             "controllers": {}, "player": "ITA"}
        s.update(kw)
        return s

    def test_research_days_and_idle_slot(self):
        h = horizon.estimate(PLAN, self.snap(), horizon.Costs(tech_days=lambda t: 100))
        self.assertEqual(h["research"]["left"], ["b", "c", "d"])
        self.assertEqual(h["research"]["days"], 150.0)
        self.assertFalse(h["research"]["low"])
        h = horizon.estimate(PLAN, self.snap(research_done=["b", "c", "d"]), horizon.Costs())
        self.assertTrue(h["research"]["low"])

    def test_focus_counts_only_startable_and_not_done(self):
        h = horizon.estimate(PLAN, self.snap(focus_states={"f3": "unavailable"}), horizon.Costs(focus_days=lambda f: 70))
        self.assertEqual(h["focus"]["left"], ["f2"])
        self.assertFalse(h["focus"]["low"])
        h = horizon.estimate(PLAN, self.snap(focuses_done=["f2"], focus_states={"f3": "unavailable"}))
        self.assertTrue(h["focus"]["low"])

    def test_signals_make_politics_production_construction_low(self):
        day = state.day_of("1936.06.01")
        h = horizon.estimate(PLAN, self.snap(signals={"political_power_idle": day - 3, "civilian_factories_idle": day},
                                             shortages={"artillery_equipment": {"day": day - 1, "days_of_cover": 0}}))
        self.assertTrue(h["politics"]["low"])
        self.assertTrue(h["production"]["low"])
        self.assertTrue(h["construction"]["low"])
        old = horizon.estimate(PLAN, self.snap(shortages={"artillery_equipment": {"day": day - 90}}))
        self.assertFalse(old["production"]["low"])

    def test_army_targets(self):
        h = horizon.estimate(PLAN, self.snap(controllers={"842": "ITA", "271": "ETH"}))
        self.assertEqual(h["army"]["targets_left"], 1)
        self.assertFalse(h["army"]["low"])


class TriggerTests(unittest.TestCase):
    def test_horizon_low_fires_once_then_waits_for_recovery(self):
        t = triggers.Triggers()
        day = state.day_of("1936.03.01")
        snap = {"day": day, "wars": ["ETH"]}
        low = {"research": {"low": True, "days": 10, "why": "x"}}
        fired = t.evaluate(NOCK, snap, low, [])
        self.assertEqual([f["id"] for f in fired], ["horizon_low:research"])
        t.fired(fired, day)
        snap["day"] = day + 40
        def ids(fired):
            return [f["id"] for f in fired if f["kind"] == "horizon_low"]
        self.assertEqual(ids(t.evaluate(NOCK, snap, low, [])), [])          # disarmed: the queue has not recovered
        t.evaluate(NOCK, snap, {"research": {"low": False, "days": 200}}, [])
        self.assertEqual(ids(t.evaluate(NOCK, snap, low, [])), ["horizon_low:research"])

    def test_cooldown_for_signal_domains(self):
        t = triggers.Triggers()
        day = state.day_of("1936.03.01")
        low = {"politics": {"low": True, "why": "idle"}}
        fired = t.evaluate(NOCK, {"day": day}, low, [])
        t.fired(fired, day)
        self.assertEqual(t.evaluate(NOCK, {"day": day + 5}, low, []), [])
        self.assertEqual(len(t.evaluate(NOCK, {"day": day + 25}, low, [])), 1)

    def test_checkpoint_due_and_missed(self):
        t = triggers.Triggers()
        day = state.day_of("1936.02.02")
        fired = t.evaluate(PLAN, {"day": day}, {}, [])
        self.assertEqual([f["kind"] for f in fired], ["checkpoint_due"])
        t.fired(fired, day)
        self.assertEqual([f for f in t.evaluate(PLAN, {"day": day + 1}, {}, []) if f["kind"] == "checkpoint_due"], [])
        missed = [{"kind": "checkpoint_missed", "detail": {"checkpoint": "ck1", "failed": ["x"]}}]
        self.assertEqual(t.evaluate(PLAN, {"day": day + 1}, {}, missed)[0]["class"], "detail")
        missed2 = [{"kind": "checkpoint_missed", "detail": {"checkpoint": "ck2", "failed": ["y"]}}]
        t.evaluate(PLAN, {"day": day + 2}, {}, missed2)
        self.assertEqual(t.evaluate(PLAN, {"day": day + 20}, {}, missed2)[0]["class"], "strategy")

    def test_unexpected_war_is_strategy_expected_is_not(self):
        t = triggers.Triggers()
        day = state.day_of("1936.05.01")
        ev = [{"kind": "war_started", "detail": {"with": "ETH"}}, {"kind": "war_started", "detail": {"with": "FRA"}}]
        fired = t.evaluate(NOCK, {"day": day}, {}, ev)
        self.assertEqual([(f["id"], f["class"]) for f in fired], [("war_changed:war_started:FRA", "strategy")])

    def test_goal_at_risk_needs_confirmation(self):
        t = triggers.Triggers()
        day = state.day_of("1936.10.01")
        ev = [{"kind": "goal_at_risk", "detail": {"why": "late"}}]
        first = t.evaluate(NOCK, {"day": day}, {}, ev)
        self.assertEqual(first[0]["class"], "detail")
        t.fired(first, day)
        again = t.evaluate(NOCK, {"day": day + 31}, {}, ev)
        self.assertEqual(again[0]["class"], "strategy")

    def test_schedule(self):
        t = triggers.Triggers()
        day = state.day_of("1936.01.01")
        self.assertEqual(t.evaluate(NOCK, {"day": day, "wars": ["ETH"]}, {}, []), [])
        fired = t.evaluate(NOCK, {"day": day + 31, "wars": ["ETH"]}, {}, [])
        self.assertIn("schedule", [f["id"] for f in fired])
        t.updated(day + 31)
        self.assertNotIn("schedule", [f["id"] for f in t.evaluate(NOCK, {"day": day + 32, "wars": ["ETH"]}, {}, [])])

    def test_feed_reasons(self):
        t = triggers.Triggers()
        day = state.day_of("1936.05.01")
        ev = [{"kind": "revision_requested", "detail": {"reason": "pp idle"}},
              {"kind": "construction_item_blocked", "detail": {"item": "cp01", "why": "no node"}},
              {"kind": "wake_player", "detail": {"reason": "casualty_ratio:ETH", "message": "2.3"}},
              {"kind": "army_out_of_supply", "detail": {"army": "North"}}]
        ids = [f["id"] for f in t.evaluate(NOCK, {"day": day}, {}, ev)]
        self.assertEqual(ids, ["player_request", "blocked:cp01", "deviation:casualties", "deviation:supply"])


class LedgerTests(unittest.TestCase):
    def test_executor_memory_and_failed_lookup(self):
        with tempfile.TemporaryDirectory() as d:
            path = os.path.join(d, "planner", "ledger.jsonl")
            led = ledger.Ledger(path)
            conv = ledger.hour_to_day_from(1000, 50)
            led.from_executor({"failures": {"hoi4_politics_choose {\"idea\":\"ITA_x\"}": [1000, 1024, 1048]},
                               "plan_blocked": {"id:cp01": {"at": 1000, "why": "no node"}}}, conv)
            led.from_executor({"failures": {"hoi4_politics_choose {\"idea\":\"ITA_x\"}": [1000, 1024, 1048]}}, conv)
            self.assertEqual(len(led.entries), 2)
            self.assertTrue(led.failed("ITA_x", 60))
            self.assertFalse(led.failed("ITA_x", 200))
            self.assertEqual(len(ledger.Ledger(path).entries), 2)


class PatchTests(unittest.TestCase):
    def test_operations(self):
        p = {"ops": [
            {"op": "extend", "key": "research_path", "items": [{"year": 1936, "techs": ["e"]}]},
            {"op": "replace_tail", "key": "focus_path", "items": ["f9", "f1"]},
            {"op": "set", "key": "politics_auto.order", "value": []},
            {"op": "add", "key": "production", "item": {"equipment": "artillery_equipment_1", "factories": 3}},
        ]}
        new = patch.apply(PLAN, p, done={"f1"})
        self.assertEqual(new["research_path"][-1]["techs"], ["e"])
        self.assertEqual(new["focus_path"], ["f1", "f9"])
        self.assertEqual(new["politics_auto"]["order"], [])
        self.assertNotIn("id", new["production"][0])               # production items carry no id
        added = patch.apply(PLAN, {"ops": [{"op": "add", "key": "construction_plan", "item": {"building": "x"}}]})
        self.assertEqual(added["construction_plan"][0]["id"], "construction_plan_u1")
        self.assertEqual(PLAN["focus_path"], ["f1", "f2"])          # the incumbent is untouched
        self.assertEqual(patch.touched(p), ["focus_path", "politics_auto", "production", "research_path"])

    def test_strategy_keys_need_escalation_and_reason(self):
        p = {"ops": [{"op": "set", "key": "war.axes", "value": [[841]]}]}
        self.assertTrue(patch.shape_errors(p))
        self.assertTrue(patch.shape_errors(dict(p, escalated=True)))
        self.assertEqual(patch.shape_errors(dict(p, escalated=True, reason="Tigray held, Gondar next")), [])
        self.assertEqual(patch.shape_errors({"ops": [{"op": "set", "key": "war.supply_floor", "value": 40}]}), [])
        self.assertTrue(patch.shape_errors({"ops": [{"op": "set", "key": "war", "value": {}}]}))
        self.assertTrue(patch.shape_errors({"ops": [{"op": "set", "key": "active", "value": False}]}))

    def test_done_items_stay(self):
        plan = {"construction_plan": [{"id": "c1"}, {"id": "c2"}]}
        with self.assertRaises(patch.PatchError):
            patch.apply(plan, {"ops": [{"op": "remove", "key": "construction_plan", "id": "c1"}]}, done={"c1"})
        new = patch.apply(plan, {"ops": [{"op": "remove", "key": "construction_plan", "id": "c2"}]}, done={"c1"})
        self.assertEqual(new["construction_plan"], [{"id": "c1"}])


class StoreTests(unittest.TestCase):
    def test_apply_conflict_history_rollback(self):
        with tempfile.TemporaryDirectory() as d:
            with open(os.path.join(d, "plan.json"), "w") as f:
                json.dump({"_meta": {"version": 1}, "x": 1}, f)
            s = store.PlanStore(d, clock=lambda: "T")
            plan, sha = s.read()
            v = s.apply(dict(plan, x=2), sha, "x to 2", trigger="test")
            self.assertEqual(v, 2)
            now, _ = s.read()
            self.assertEqual((now["x"], now["_meta"]["parent"], now["_meta"]["trigger"]), (2, 1, "test"))
            self.assertTrue(os.path.exists(os.path.join(d, "plans", "plan.v1.json")))
            with self.assertRaises(store.Conflict):
                s.apply(dict(plan, x=3), sha, "stale")
            self.assertEqual(s.rollback("bad"), (3, 1))
            back, _ = s.read()
            self.assertEqual((back["x"], back["_meta"]["version"], back["_meta"]["rolled_back_from"]), (1, 3, 2))


class BudgetTests(unittest.TestCase):
    def test_ceiling_per_hour(self):
        now = [0.0]
        b = budget.Budget(clock=lambda: now[0], max_calls=2, max_escalations=1)
        self.assertTrue(b.allow()[0])
        b.record(30, 1000)
        b.record(30, 1000, escalated=True)
        self.assertFalse(b.allow()[0])
        now[0] = 3700
        self.assertTrue(b.allow()[0])
        b.record(10, 10, escalated=True)
        self.assertFalse(b.allow(escalated=True)[0])
        self.assertEqual(b.summary()["model_calls"], 3)
        self.assertEqual(budget.tokens_of({"inputTokens": 5, "outputTokens": 7, "cacheReadInputTokens": 30,
                                           "cache_creation_input_tokens": 10}), 25)


if __name__ == "__main__":
    unittest.main()
