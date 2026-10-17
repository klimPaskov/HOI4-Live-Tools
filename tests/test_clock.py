"""The fast clock (game/clock.py): the telemetry-driven loop, its stops, popups, notices, and the background run with
hoi4_clock. The game is replaced by a fake session whose telemetry store a feeder thread fills with one day record
per tick, and by patched screen readers (speed icon, popup detector, notice dismissal)."""

import shutil
import tempfile
import threading
import time
import unittest
from unittest import mock

from helpers import FIXTURES  # noqa: F401

from hoi4_live.game import clock as clock_mod
from hoi4_live.game.popups import PopupPolicy
from hoi4_live.telemetry.parser import TelemetryStore
from hoi4_live.tools import campaign_loop_tools as loop
from test_campaign_loop import day


class ClockStateRetryTests(unittest.TestCase):
    def test_transient_unknown_then_wanted_state_needs_no_input(self):
        session = mock.Mock()
        with mock.patch.object(clock_mod, "icon_state", side_effect=["unknown", "running"]), \
             mock.patch.object(clock_mod.time, "sleep"):
            self.assertTrue(clock_mod.set_running(session, True))
        session.input.key.assert_not_called()

    def test_persistent_unknown_is_bounded_and_never_toggles(self):
        session = mock.Mock()
        with mock.patch.object(clock_mod, "icon_state", return_value="unknown") as probe, \
             mock.patch.object(clock_mod.time, "sleep"):
            self.assertFalse(clock_mod.set_running(session, True))
        self.assertEqual(probe.call_count, 3)
        session.input.key.assert_not_called()

    def test_only_confirmed_opposite_state_authorizes_toggle(self):
        session = mock.Mock()
        states = iter(["unknown", "paused", "running"])
        def observe(_session):
            state = next(states)
            if state in ("unknown", "paused"):
                session.input.key.assert_not_called()
            return state
        with mock.patch.object(clock_mod, "icon_state", side_effect=observe), \
             mock.patch.object(clock_mod.time, "sleep"):
            self.assertTrue(clock_mod.set_running(session, True))
        session.input.key.assert_called_once_with("space")


class FakeInput:
    def __init__(self):
        self.keys = []

    def key(self, combo, times=1, delay=None):
        self.keys.append(combo)


class FakeSession:
    def __init__(self, tmp, store):
        self.cfg = mock.Mock(artifacts_dir=tmp)
        self._store = store
        self.last_tag = None
        self.ui_mutex = threading.RLock()
        self.clock = None
        self.standing = None
        self.date_uncertain = False
        self.input = FakeInput()
        self.lock = mock.MagicMock()

    @property
    def telemetry(self):
        return self._store

    def grab(self, *a, **k):
        return object()

    def check_cancel(self):
        pass


class Feeder(threading.Thread):
    """Adds a telemetry day record every `tick` seconds while the fake game 'runs'."""

    def __init__(self, store, state, tick=0.02, script=None, start_n=100):
        super().__init__(daemon=True)
        self.store, self.state, self.tick, self.script = store, state, tick, script or {}
        self.n = start_n
        self.fields = {}
        self.stop_flag = False

    def run(self):
        while not self.stop_flag:
            time.sleep(self.tick)
            if not self.state["running"]:
                continue
            self.n += 1
            self.fields.update(self.script.get(self.n, {}))
            d = self.n - 100
            self.store.add(day(self.n, f"1936.{1 + d // 28}.{1 + d % 28}.24", **self.fields))


class ClockCase(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.mkdtemp(prefix="hoi4live_clock_")
        self.store = TelemetryStore()
        self.store.add(day(100, "1936.1.1.24"))
        self.session = FakeSession(self.tmp, self.store)
        self.state = {"running": False, "paused_by_game": False}
        self.dismissed = []
        self.detected = []
        self.feeder = None
        p = [mock.patch.object(clock_mod, "POLL", 0.01), mock.patch.object(clock_mod, "LOOK_EVERY", 0.05),
             mock.patch.object(clock_mod, "ensure_speed", lambda s, sp: None),
             mock.patch.object(clock_mod, "set_running", self.set_running),
             mock.patch.object(clock_mod, "icon_state", self.icon_state),
             mock.patch.object(clock_mod.popups_mod, "detect", lambda img: list(self.detected)),
             mock.patch("hoi4_live.game.panels.dismiss_notices", self.dismiss)]
        for x in p:
            x.start()
            self.addCleanup(x.stop)

    def tearDown(self):
        if self.feeder:
            self.feeder.stop_flag = True
            self.feeder.join(timeout=1)
        shutil.rmtree(self.tmp, ignore_errors=True)

    def set_running(self, session, want, **kw):
        self.state["running"] = want
        return True

    def icon_state(self, session, img=None):
        if self.state["paused_by_game"]:
            return "paused"
        return "running" if self.state["running"] else "paused"

    def dismiss(self, session, max_count=4):
        out, self.dismissed = self.dismissed, []
        if out:
            self.state["paused_by_game"] = False
        return out

    def feed(self, **kw):
        self.feeder = Feeder(self.store, self.state, **kw)
        self.feeder.start()

    def args(self, **kw):
        a = {"budget_days": 5, "stop_on": [], "max_seconds": 20, "speed": 5, "stop_on_used": []}
        a.update(kw)
        return a

    def run_loop(self, **kw):
        run = clock_mod.AdvanceRun(self.session, self.args(**kw), PopupPolicy.parse(kw.pop("popups", "choices")))
        self.session.clock = run
        return run, run.execute()


class FastClockTests(ClockCase):
    def test_exception_pauses_and_releases_background_lease(self):
        with mock.patch.object(clock_mod.guard, "drain", side_effect=RuntimeError("broken reader")):
            with self.assertRaisesRegex(RuntimeError, "broken reader"):
                self.run_loop()
        self.assertFalse(self.state["running"])
        self.session.lock.hold.assert_called_with("background campaign clock")
        self.session.lock.hold.return_value.__exit__.assert_called()

    def test_daily_hook_runs_once_for_newest_observation(self):
        self.feed()
        seen = []
        def on_day(run, record):
            seen.append(record["n"])
            return [{"code": "governor_decision", "text": "new plan needed"}]
        run = clock_mod.AdvanceRun(self.session, self.args(budget_days=30),
                                   PopupPolicy.parse("choices"), on_day=on_day)
        res = run.execute()
        self.assertEqual(res["reasons"][0]["code"], "governor_decision")
        self.assertEqual(len(seen), 1)
        self.assertFalse(self.state["running"])

    def test_missing_research_baseline_is_unknown(self):
        run = clock_mod.AdvanceRun(self.session, self.args(), PopupPolicy.parse("choices"))
        self.assertEqual(run._idle_slots(None), 0)

    def test_runs_the_budget_and_keeps_running(self):
        self.feed()
        run, res = self.run_loop(budget_days=5)
        self.assertEqual(res["reasons"][0]["code"], "done")
        self.assertGreaterEqual(res["days"], 5)
        self.assertTrue(self.state["running"], "routine deadlines leave the game running")
        self.assertTrue(res["end_date"].startswith("1936.1."))

    def test_stop_condition_fires_on_the_day_and_keeps_running(self):
        # Publish one day per clock poll rather than racing a 20 ms feeder
        # against the CI scheduler. This proves reaction to the first completed
        # observation without assuming any real-time thread scheduling bound.
        counter = [100]
        def publish(_seconds):
            if self.state["running"]:
                counter[0] += 1
                self.store.add(day(counter[0], "1936.1.4.24",
                                   techs=11 if counter[0] >= 103 else 10))
        with mock.patch.object(clock_mod.time, "sleep", side_effect=publish):
            run, res = self.run_loop(budget_days=30, stop_on=["tech_done"], stop_on_used=["tech_done"])
        self.assertEqual(res["reasons"][0]["code"], "tech_done")
        self.assertEqual(res["days"], 3)
        self.assertEqual(res["changes"]["techs"], [10, 11])
        self.assertTrue(self.state["running"])

    def test_event_with_a_choice_stops_the_run_and_stays_paused(self):
        self.feed()
        popup = {"kind": "event", "title": "Choice", "answered": False, "why": "2 options: the agent decides"}
        calls = []

        def handle_all(session, policy, img=None, limit=25, phase=None):
            calls.append(phase)
            self.detected.clear()
            return [popup]
        self.detected.append(object())
        with mock.patch.object(clock_mod.game_popups, "handle_all", handle_all):
            run, res = self.run_loop(budget_days=30)
        self.assertEqual(res["reasons"][0]["code"], "popup")
        self.assertEqual(res["data"]["events_waiting"][0]["title"], "Choice")
        self.assertEqual(calls, ["running"])
        self.assertFalse(self.state["running"], "an unresolved choice pauses after it is read")

    def test_answered_events_do_not_stop_the_run(self):
        self.feed()
        rec = {"kind": "news", "title": "Aussa", "answered": True, "chosen_text": "Fine", "date": None}

        def handle_all(session, policy, img=None, limit=25, phase=None):
            self.detected.clear()
            return [rec]
        self.detected.append(object())
        with mock.patch.object(clock_mod.game_popups, "handle_all", handle_all):
            run, res = self.run_loop(budget_days=4)
        self.assertEqual(res["reasons"][0]["code"], "done")
        self.assertEqual(res["answered"][0]["title"], "Aussa")

    def test_a_pause_by_a_notice_window_is_dismissed_and_reported(self):
        self.feed()
        self.dismissed = ["NATIONAL FOCUS COMPLETED"]
        self.state["running"] = True
        self.state["paused_by_game"] = True        # the game stopped itself with a notice window open
        run, res = self.run_loop(budget_days=4, stop_on=["focus_done"], stop_on_used=["focus_done"])
        self.assertEqual(res["reasons"][0]["code"], "focus_done")
        self.assertEqual(run.run["notices"][0]["title"], "NATIONAL FOCUS COMPLETED")

    def test_a_game_that_keeps_pausing_is_reported(self):
        self.state["running"] = False

        def set_running(session, want, **kw):
            self.state["running"] = False        # the game pauses again at once
            return True
        with mock.patch.object(clock_mod, "set_running", set_running):
            run, res = self.run_loop(budget_days=30)
        self.assertEqual(res["reasons"][0]["code"], "paused_by_game")

    def test_wall_clock_limit(self):
        self.feed(tick=5)                          # the game is too slow to produce a day
        run, res = self.run_loop(budget_days=30, max_seconds=1)
        self.assertEqual(res["reasons"][0]["code"], "time_limit")

    def test_a_new_game_without_records_starts_from_the_first_day_that_arrives(self):
        empty = TelemetryStore()
        session = FakeSession(self.tmp, empty)
        self.session = session
        self.feeder = Feeder(empty, self.state, start_n=500)
        self.feeder.start()
        run = clock_mod.AdvanceRun(session, self.args(budget_days=4, start_date="1936.1.1.12"),
                                   PopupPolicy.parse("choices"))
        res = run.execute()
        self.assertEqual(res["reasons"][0]["code"], "done")
        self.assertEqual(res["start_date"], "1936.1.1.12")
        self.assertGreaterEqual(res["days"], 4)

    def test_missing_telemetry_is_an_actionable_error(self):
        from hoi4_live.errors import ToolError
        session = FakeSession(self.tmp, TelemetryStore())
        run = clock_mod.AdvanceRun(session, self.args(), PopupPolicy.parse("choices"))
        with self.assertRaises(ToolError) as cm:
            run.execute()
        self.assertIn("telemetry", str(cm.exception))


class ShutdownTests(unittest.TestCase):
    def test_input_closes_before_releasing_desktop_lease(self):
        import helpers
        session = helpers.make_session()
        events = []
        session.input = mock.Mock()
        session.input.close.side_effect = lambda: events.append("input")
        session.lock = mock.Mock()
        session.lock.release_all.side_effect = lambda: events.append("lease")
        session.shutdown()
        self.assertEqual(events, ["input", "lease"])

    def test_running_worker_defers_input_and_lease_teardown(self):
        import helpers
        session = helpers.make_session()
        run = mock.Mock()
        run.done = threading.Event()
        session.clock = run
        session.input = mock.Mock()
        session.lock = mock.Mock()
        self.assertFalse(session.shutdown())
        session.input.close.assert_not_called()
        session.lock.release_all.assert_not_called()
        run.done.set()
        session.shutdown()
        session.input.close.assert_called_once()
        session.lock.release_all.assert_called_once()

    def test_a_background_run_is_stopped_when_the_session_shuts_down(self):
        import helpers
        from hoi4_live.session import Session
        session = helpers.make_session()
        run = mock.Mock()
        run.done = threading.Event()
        session.clock = run
        session.shutdown()
        run.stop.assert_called_once()
        run.join.assert_called_once()


class BackgroundTests(ClockCase):
    def setUp(self):
        super().setUp()
        p = mock.patch.object(loop.timectl, "read_date", lambda session, img=None: ({"hoi4": "1936.1.1.24"}, None))
        p.start()
        self.addCleanup(p.stop)

    def tool_args(self, **kw):
        a = {"popups": "choices", "speed": 5, "chunk_days": 2, "max_seconds": 600, "situation": False,
             "journal": False, "screenshot": False, "engine": "fast", "background": False, "stop_on": []}
        a.update(kw)
        return a

    def test_background_run_returns_at_once_and_join_collects(self):
        self.feed()
        res = loop.hoi4_advance(self.session, self.tool_args(days=6, background=True))
        self.assertTrue(res.data["background"])
        status = loop.hoi4_clock(self.session, {"action": "status", "wait_s": 1})
        self.assertIn("date", status.data)
        done = loop.hoi4_clock(self.session, {"action": "join", "wait_s": 20})
        self.assertEqual(done.data["stopped_by"][0]["code"], "done")
        self.assertGreaterEqual(done.data["days_run"], 6)
        again = loop.hoi4_clock(self.session, {"action": "join", "wait_s": 1})
        self.assertIn("already collected", again.summary)

    def test_a_second_advance_while_one_runs_is_refused(self):
        from hoi4_live.errors import ToolError
        self.feed(tick=0.3)
        loop.hoi4_advance(self.session, self.tool_args(days=30, background=True))
        with self.assertRaises(ToolError):
            loop.hoi4_advance(self.session, self.tool_args(days=3))
        loop.hoi4_clock(self.session, {"action": "stop", "wait_s": 5})

    def test_stop_ends_the_run_and_returns_its_result(self):
        self.feed(tick=0.05)
        loop.hoi4_advance(self.session, self.tool_args(days=300, background=True))
        time.sleep(0.3)
        res = loop.hoi4_clock(self.session, {"action": "stop", "wait_s": 5})
        self.assertEqual(res.data["stopped_by"][0]["code"], "cancelled")
        self.assertFalse(self.state["running"])


if __name__ == "__main__":
    unittest.main()
