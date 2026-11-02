"""Standing orders (state/standing.py, hoi4_standing): validation, queues, failures and their effect on the clock."""

import unittest
from unittest import mock

from helpers import FIXTURES  # noqa: F401

from hoi4_live.errors import ToolError
from hoi4_live.game import clock as clock_mod
from hoi4_live.state import standing
from hoi4_live.tools import campaign_loop_tools as loop
from test_campaign_loop import day
from test_clock import ClockCase


def call(tool="hoi4_research", **args):
    return {"tool": tool, "args": args}


class RuleTests(unittest.TestCase):
    def test_a_rule_needs_a_known_trigger_and_one_of_do_or_queue(self):
        st = standing.StandingOrders()
        with self.assertRaises(ToolError):
            st.set("r", ["no_such_code"], do=[call()])
        with self.assertRaises(ToolError):
            st.set("r", [], do=[call()])
        with self.assertRaises(ToolError):
            st.set("r", ["tech_done"])
        with self.assertRaises(ToolError):
            st.set("r", ["tech_done"], do=[call()], queue=[call()])

    def test_tools_that_cannot_run_inside_the_clock_are_refused(self):
        st = standing.StandingOrders()
        for tool in ("hoi4_advance", "hoi4_scenario", "hoi4_console", "hoi4_eval_effect", "hoi4_fire_event",
                     "hoi4_nonexistent"):
            with self.assertRaises(ToolError, msg=tool):
                st.set("r", ["tech_done"], do=[call(tool)])

    def test_queue_entries_may_be_lists_of_calls(self):
        st = standing.StandingOrders()
        st.set("r", ["research_idle"], queue=[call(tech="a"), [call(tech="b"), call("hoi4_production", action="lines")]])
        self.assertEqual([len(q) for q in st.rules["r"]["queue"]], [1, 2])
        row = st.listing()[0]
        self.assertEqual(row["queue_left"], 2)

    def test_orders_survive_a_restart(self):
        import os
        import tempfile
        d = tempfile.mkdtemp()
        path = os.path.join(d, "state", "standing.json")
        st = standing.StandingOrders(path)
        st.set("r", ["research_idle"], queue=[call(tech="a"), call(tech="b")])
        st.rules["r"]["queue"].pop(0)
        st.extend("r", [call(tech="c")])
        again = standing.StandingOrders(path)
        self.assertEqual([q[0]["args"]["tech"] for q in again.rules["r"]["queue"]], ["b", "c"])
        again.remove("r")
        self.assertFalse(standing.StandingOrders(path).active())

    def test_remove_and_clear(self):
        st = standing.StandingOrders()
        st.set("a", ["tech_done"], do=[call()])
        st.set("b", ["tech_done"], do=[call()])
        self.assertIsNotNone(st.remove("a"))
        self.assertIsNone(st.remove("a"))
        self.assertEqual(st.clear(), 1)
        self.assertFalse(st.active())


class ToolTests(ClockCase):
    def test_tool_set_append_list_remove(self):
        out = loop.hoi4_standing(self.session, {"action": "set", "name": "r", "when": ["research_idle"], "queue": [call(tech="a")],
                                                "append": False, "pause": True, "min_gap_days": 0})
        self.assertEqual(out.data["orders"][0]["queue_left"], 1)
        out = loop.hoi4_standing(self.session, {"action": "set", "name": "r", "queue": [call(tech="b"), call(tech="c")],
                                                "append": True, "pause": True, "min_gap_days": 0})
        self.assertEqual(out.data["orders"][0]["queue_left"], 3)
        self.assertIn("r on research_idle", loop.hoi4_standing(self.session, {"action": "list", "append": False,
                                                                              "pause": True, "min_gap_days": 0}).summary)
        loop.hoi4_standing(self.session, {"action": "remove", "name": "r", "append": False, "pause": True,
                                          "min_gap_days": 0})
        self.assertFalse(self.session.standing.active())
        with self.assertRaises(ToolError):
            loop.hoi4_standing(self.session, {"action": "remove", "name": "r", "append": False, "pause": True,
                                              "min_gap_days": 0})


class ClockWithOrdersTests(ClockCase):
    def setUp(self):
        super().setUp()
        self.calls = []
        self.fail_on = None

        def fake_call(session, name, args, progress=None, cancel=None):
            self.calls.append((name, args))
            bad = self.fail_on and args.get("tech") == self.fail_on
            return {"content": [{"type": "text", "text": ("boom" if bad else f"started {args.get('tech')}")}],
                    "isError": bool(bad)}
        p = mock.patch("hoi4_live.tools.dispatch.call", fake_call)
        p.start()
        self.addCleanup(p.stop)
        self.session.standing = standing.StandingOrders()

    def run_with_orders(self, **kw):
        run = clock_mod.AdvanceRun(self.session, self.args(**kw), clock_mod.game_popups.PopupPolicy.parse("choices"),
                                   on_reasons=standing.hook(self.session))
        self.session.clock = run
        return run, run.execute()

    def test_a_queue_is_used_one_entry_per_firing_and_the_run_goes_on(self):
        self.session.standing.set("r", ["tech_done"], queue=[call(tech="a"), call(tech="b")])
        self.feed(script={102: {"techs": 11}, 105: {"techs": 12}})
        run, res = self.run_with_orders(budget_days=8)
        self.assertEqual([c[1]["tech"] for c in self.calls], ["a", "b"])
        self.assertEqual(res["reasons"][0]["code"], "done")
        self.assertEqual([o["rule"] for o in run.run["auto"]], ["r", "r"])
        self.assertEqual(self.session.standing.rules["r"]["fired"], 2)
        self.assertEqual(res["data"]["standing_orders_done"][0]["calls"][0]["tool"], "hoi4_research")

    def test_the_clock_is_paused_around_a_rule_and_resumed(self):
        events = []
        orig = self.set_running

        def spy(session, want, **kw):
            events.append(want)
            return orig(session, want, **kw)
        self.session.standing.set("r", ["tech_done"], do=[call(tech="a")], pause=True)
        self.feed(script={102: {"techs": 11}})
        with mock.patch.object(clock_mod, "set_running", spy):
            self.run_with_orders(budget_days=4)
        self.assertEqual(events[:3], [True, False, True])

    def test_pause_false_keeps_the_game_running(self):
        events = []
        orig = self.set_running
        self.session.standing.set("r", ["tech_done"], do=[call(tech="a")], pause=False)
        self.feed(script={102: {"techs": 11}})
        with mock.patch.object(clock_mod, "set_running",
                               lambda s, want, **kw: (events.append(want), orig(s, want, **kw))[1]):
            self.run_with_orders(budget_days=4)
        self.assertEqual(events[0], True)
        self.assertNotIn(False, events[1:-1])

    def test_an_empty_queue_stops_the_run(self):
        self.session.standing.set("r", ["tech_done"], queue=[call(tech="a")])
        # Deliver both completion observations separately. A real-time feeder
        # can publish both before a busy CI runner polls, merging their change
        # into one firing and never exercising the empty queue.
        counter = [100]
        def publish(_seconds):
            if self.state["running"]:
                counter[0] += 1
                techs = 12 if counter[0] >= 104 else 11 if counter[0] >= 102 else 10
                self.store.add(day(counter[0], "1936.1.5.24", techs=techs))
        with mock.patch.object(clock_mod.time, "sleep", side_effect=publish):
            run, res = self.run_with_orders(budget_days=30)
        self.assertEqual(res["reasons"][0]["code"], "standing_queue_empty")
        self.assertEqual(res["days"], 4)
        self.assertEqual(len(self.calls), 1)

    def test_a_failed_call_stops_the_run_and_keeps_the_entry(self):
        self.session.standing.set("r", ["tech_done"], queue=[call(tech="bad"), call(tech="b")])
        self.fail_on = "bad"
        self.feed(script={102: {"techs": 11}})
        run, res = self.run_with_orders(budget_days=30)
        self.assertEqual(res["reasons"][0]["code"], "standing_failed")
        self.assertIn("boom", res["reasons"][0]["text"])
        self.assertEqual(self.session.standing.rules["r"]["queue"][0][0]["args"]["tech"], "bad")

    def test_a_rule_waits_for_codes_that_are_not_in_stop_on(self):
        # stop_on is empty, yet the rule on construction_done fires
        self.session.standing.set("r", ["construction_done"], do=[call("hoi4_construction", action="queue")])
        self.feed(script={102: {"civ_free": 9}})
        run, res = self.run_with_orders(budget_days=4)
        self.assertEqual(res["reasons"][0]["code"], "done")
        self.assertEqual(self.calls[0][0], "hoi4_construction")

    def test_unhandled_codes_not_in_stop_on_do_not_stop_the_run(self):
        self.session.standing.set("r", ["tech_done"], do=[call(tech="a")])
        self.feed(script={102: {"civ_free": 9}})          # construction_done is not wanted by anyone
        run, res = self.run_with_orders(budget_days=4)
        self.assertEqual(res["reasons"][0]["code"], "done")
        self.assertEqual(self.calls, [])

    def test_focus_done_comes_from_the_timer_of_the_started_focus(self):
        from hoi4_live.state import focustimer
        p = mock.patch.object(clock_mod, "FOCUS_TIMER_TRIGGER", True)
        p.start()
        self.addCleanup(p.stop)
        focustimer.start(self.session, "GER_x", "Remilitarize", 3)
        self.session.standing.set("f", ["focus_done"], queue=[call("hoi4_focus", action="select", focus="GER_next")])
        self.feed()
        run, res = self.run_with_orders(budget_days=8)
        self.assertEqual([c[1]["focus"] for c in self.calls], ["GER_next"])
        self.assertEqual(res["reasons"][0]["code"], "done")
        self.assertIsNone(focustimer.due(self.session, 10 ** 9))      # fires once

    def test_focus_done_stops_a_plain_run_when_asked(self):
        from hoi4_live.state import focustimer
        p = mock.patch.object(clock_mod, "FOCUS_TIMER_TRIGGER", True)
        p.start()
        self.addCleanup(p.stop)
        focustimer.start(self.session, "GER_x", "Remilitarize", 2)
        self.feed()
        run, res = self.run_with_orders(budget_days=8, stop_on=["focus_done"], stop_on_used=["focus_done"])
        self.assertEqual(res["reasons"][0]["code"], "focus_done")
        self.assertIn("Remilitarize", res["reasons"][0]["text"])

    def test_a_notice_closed_by_the_guard_reaches_the_standing_orders(self):
        from hoi4_live.game import guard
        guard.queue(self.session).append({"title": "NATIONAL FOCUS COMPLETED"})
        self.session.standing.set("f", ["focus_done"], queue=[call("hoi4_focus", action="select", focus="GER_next")])
        self.feed()
        run, res = self.run_with_orders(budget_days=3)
        self.assertEqual([c[1]["focus"] for c in self.calls], ["GER_next"])
        self.assertEqual(run.run["notices"][0]["title"], "NATIONAL FOCUS COMPLETED")

    def test_a_deferred_call_keeps_its_queue_entry_and_asks_again(self):
        from hoi4_live.state import focustimer
        p = mock.patch.object(clock_mod, "FOCUS_TIMER_TRIGGER", True)
        p.start()
        self.addCleanup(p.stop)
        results = iter([{"content": [{"type": "text", "text": "deferred"}], "isError": False,
                         "structuredContent": {"deferred": True}},
                        {"content": [{"type": "text", "text": "started"}], "isError": False}])

        def fake(session, name, args, progress=None, cancel=None):
            self.calls.append((name, args))
            return next(results)
        focustimer.start(self.session, "GER_x", "Remilitarize", 2)
        self.session.standing.set("f", ["focus_done"], queue=[call("hoi4_focus", action="select", focus="GER_next",
                                                                    if_idle=True)])
        self.feed()
        with mock.patch("hoi4_live.tools.dispatch.call", fake):
            run, res = self.run_with_orders(budget_days=8)
        self.assertEqual(len(self.calls), 2)                       # deferred once, started on the retry
        self.assertEqual(self.session.standing.rules["f"]["fired"], 1)
        self.assertEqual(self.session.standing.rules["f"]["queue"], [])
        self.assertTrue(run.run["auto"][0].get("deferred"))

    def test_a_moot_entry_is_passed_over_to_the_next_one_in_the_same_firing(self):
        results = {"gone": {"content": [{"type": "text", "text": "already researched"}], "isError": False,
                            "structuredContent": {"skip": True}}}

        def fake(session, name, args, progress=None, cancel=None):
            self.calls.append((name, args))
            return results.get(args.get("tech"), {"content": [{"type": "text", "text": "started"}], "isError": False})
        self.session.standing.set("r", ["tech_done"], queue=[call(tech="gone"), call(tech="gone"), call(tech="b"),
                                                              call(tech="c")])
        self.feed(script={102: {"techs": 11}})
        with mock.patch("hoi4_live.tools.dispatch.call", fake):
            run, res = self.run_with_orders(budget_days=4)
        self.assertEqual([c[1]["tech"] for c in self.calls], ["gone", "gone", "b"])
        self.assertEqual(self.session.standing.rules["r"]["fired"], 1)
        self.assertEqual(len(self.session.standing.rules["r"]["queue"]), 1)       # "c" is still waiting

    def test_focus_idle_fires_every_five_days_while_no_focus_runs(self):
        self.session.standing.set("f", ["focus_idle"], queue=[call("hoi4_focus", action="select", focus="GER_a", if_idle=True),
                                                              call("hoi4_focus", action="select", focus="GER_b", if_idle=True)])
        self.feed()
        run, res = self.run_with_orders(budget_days=8)
        self.assertEqual([c[1]["focus"] for c in self.calls], ["GER_a", "GER_b"])

    def test_on_fail_skip_drops_the_entry_and_tries_the_next_one(self):
        self.fail_on = "bad"
        self.session.standing.set("r", ["tech_done"], queue=[call(tech="bad"), call(tech="good"), call(tech="later")],
                                  on_fail="skip")
        self.feed(script={102: {"techs": 11}})
        run, res = self.run_with_orders(budget_days=4)
        self.assertEqual([c[1]["tech"] for c in self.calls], ["bad", "good"])
        self.assertEqual(res["reasons"][0]["code"], "done")
        rule = self.session.standing.rules["r"]
        self.assertEqual(rule["failed"][0]["call"]["args"]["tech"], "bad")
        self.assertEqual(len(rule["queue"]), 1)

    def test_min_gap_days(self):
        self.session.standing.set("r", ["tech_done"], do=[call(tech="a")], min_gap_days=5)
        self.feed(script={102: {"techs": 11}, 103: {"techs": 12}})
        run, res = self.run_with_orders(budget_days=5, stop_on=["tech_done"], stop_on_used=["tech_done"])
        self.assertEqual(len(self.calls), 1)
        self.assertEqual(res["reasons"][0]["code"], "done")

    def test_research_idle_from_the_telemetry(self):
        # four slots, one rs line on every day record -> three idle; fires once when it first shows
        for n in range(100, 140):
            self.store.add({"k": "rs", "t": "USA", "n": n, "tid": 0, "_date": "1936.1.1.24", "_wall": 1.0})
        self.store.add({"k": "day", "t": "USA", "n": 100, "_date": "1936.1.1.24", "_wall": 1.0, "rslots": 4})
        self.session.standing.set("r", ["research_idle"], queue=[call(tech="a"), call(tech="b")])
        # Several feeder ticks may arrive in one batch on a busy CI runner.
        # A rule intentionally fires once per batch, so publish one day per
        # poll to verify the every-other-day policy without a scheduler race.
        counter = [100]
        def publish(_seconds):
            if self.state["running"]:
                counter[0] += 1
                elapsed = counter[0] - 100
                self.store.add({"k": "day", "t": "USA", "n": counter[0],
                                "_date": f"1936.1.{1 + elapsed}.24", "_wall": 1.0, "rslots": 4})
        with mock.patch.object(clock_mod.time, "sleep", side_effect=publish):
            run, res = self.run_with_orders(budget_days=4)
        # three idle slots: it fires on the first day and again every other day while they stay idle
        self.assertEqual([c[1]["tech"] for c in self.calls], ["a", "b"])
        self.assertEqual(self.session.standing.rules["r"]["fired"], 2)
        self.assertEqual(res["reasons"][0]["code"], "done")
        self.assertEqual(res["days"], 4)

    def test_an_empty_queue_tells_the_agent_once_per_refill(self):
        for n in range(100, 140):
            self.store.add({"k": "rs", "t": "USA", "n": n, "tid": 0, "_date": "1936.1.1.24", "_wall": 1.0})
        self.store.add({"k": "day", "t": "USA", "n": 100, "_date": "1936.1.1.24", "_wall": 1.0, "rslots": 4})
        self.session.standing.set("r", ["research_idle"], queue=[call(tech="a")])
        self.feed(script={}, tick=0.02)
        self.feeder.fields["rslots"] = 4
        run, res = self.run_with_orders(budget_days=10)
        self.assertEqual(res["reasons"][0]["code"], "standing_queue_empty")
        self.assertTrue(self.session.standing.rules["r"]["empty_told"])
        self.session.standing.extend("r", [call(tech="b")])
        self.assertFalse(self.session.standing.rules["r"]["empty_told"])


if __name__ == "__main__":
    unittest.main()
