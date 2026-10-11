"""The window guard (game/guard.py) and its place in the dispatcher."""

import unittest
from unittest import mock

from helpers import FIXTURES  # noqa: F401

from hoi4_live.errors import ToolError
from hoi4_live.game import guard
from hoi4_live.tools import dispatch


class Sess:
    def __init__(self):
        self.notice_queue = []
        self.lock = mock.MagicMock()

    def grab(self, focus=None):
        return object()


class ClearWindowsTests(unittest.TestCase):
    def patches(self, detect=(), state="paused", notices=(), handle=None):
        det = list(detect)
        return [mock.patch.object(guard.popups_mod, "detect", lambda img: det.pop(0) if det else []),
                mock.patch.object(guard.topbar, "pause_state", lambda img: (state, {})),
                mock.patch.object(guard.panels, "dismiss_notices", mock.Mock(side_effect=[list(notices), []])),
                mock.patch.object(guard.game_popups, "handle_all", handle or (lambda *a, **k: []))]

    def run_guard(self, **kw):
        ps = self.patches(**kw)
        for p in ps:
            p.start()
        self.addCleanup(lambda: [p.stop() for p in ps])
        s = Sess()
        return s, guard.clear_windows(s)

    def test_nothing_open_costs_no_clicks(self):
        s, done = self.run_guard(state="running")
        self.assertEqual(done, {"answered": [], "notices": []})
        guard.panels.dismiss_notices.assert_not_called()      # a running clock has no modal notice open

    def test_a_notice_is_dismissed_and_queued_for_the_standing_orders(self):
        s, done = self.run_guard(notices=["NATIONAL FOCUS COMPLETED"])
        self.assertEqual(done["notices"], ["NATIONAL FOCUS COMPLETED"])
        self.assertEqual(guard.drain(s), [{"title": "NATIONAL FOCUS COMPLETED"}])
        self.assertEqual(guard.drain(s), [])

    def test_news_is_answered(self):
        def handle(session, policy, img=None, phase=None):
            return [{"answered": True, "kind": "news", "title": "Aussa", "chosen_text": "Fine"}]
        s, done = self.run_guard(detect=[[object()]], handle=handle)
        self.assertEqual(done["answered"][0]["title"], "Aussa")

    def test_an_event_with_a_choice_stops_the_action(self):
        def handle(session, policy, img=None, phase=None):
            return [{"answered": False, "kind": "event", "title": "Choose wisely"}]
        ps = self.patches(detect=[[object()]], handle=handle)
        for p in ps:
            p.start()
            self.addCleanup(p.stop)
        with self.assertRaises(ToolError) as cm:
            guard.clear_windows(Sess())
        self.assertIn("Choose wisely", str(cm.exception))


class DispatcherTests(unittest.TestCase):
    def tool(self, family, name="hoi4_research"):
        return mock.Mock(family=family, lock="acting", name=name)

    def test_only_campaign_tools_are_guarded(self):
        t = mock.Mock(family="campaign")
        t.name = "hoi4_research"
        with mock.patch.object(dispatch.guard, "clear_windows") as cw:
            dispatch._guard(Sess(), t)
            cw.assert_called_once()
        for family, name in (("lowlevel", "hoi4_click"), ("campaign", "hoi4_popups"), ("session", "hoi4_load"),
                             ("campaign_loop", "hoi4_standing")):
            t = mock.Mock(family=family)
            t.name = name
            with mock.patch.object(dispatch.guard, "clear_windows") as cw:
                dispatch._guard(Sess(), t)
                cw.assert_not_called()

    def test_it_can_be_switched_off(self):
        s = Sess()
        s.guard_windows = False
        t = mock.Mock(family="campaign")
        t.name = "hoi4_research"
        with mock.patch.object(dispatch.guard, "clear_windows") as cw:
            dispatch._guard(s, t)
            cw.assert_not_called()


if __name__ == "__main__":
    unittest.main()


class RestorePauseTests(unittest.TestCase):
    def tool(self):
        t = mock.Mock(family="campaign")
        t.name = "hoi4_focus"
        return t

    def test_a_game_left_running_is_paused_again(self):
        from hoi4_live.tools.registry import ToolResult
        s = Sess()
        s.ui_mutex = mock.MagicMock()
        res = ToolResult("ok", {})
        with mock.patch.object(dispatch, "_clock_state", lambda session: "running"), \
                mock.patch("hoi4_live.game.clock.set_running") as sr:
            dispatch._restore_pause(s, self.tool(), "paused", res)
        sr.assert_called_once()
        self.assertTrue(res.data["clock_restored"])

    def test_nothing_happens_when_it_was_running_or_is_still_paused(self):
        from hoi4_live.tools.registry import ToolResult
        s = Sess()
        for before, now in (("running", "running"), ("paused", "paused"), (None, "running")):
            res = ToolResult("ok", {})
            with mock.patch.object(dispatch, "_clock_state", lambda session, n=now: n), \
                    mock.patch("hoi4_live.game.clock.set_running") as sr:
                dispatch._restore_pause(s, self.tool(), before, res)
            sr.assert_not_called()
            self.assertNotIn("clock_restored", res.data)

    def test_only_campaign_and_military_tools_watch_the_clock(self):
        for fam, name, want in (("campaign", "hoi4_focus", True), ("military", "hoi4_orders", True),
                                ("campaign", "hoi4_popups", False), ("lowlevel", "hoi4_key", False),
                                ("campaign_loop", "hoi4_standing", False)):
            t = mock.Mock(family=fam)
            t.name = name
            self.assertEqual(dispatch._watches_clock(t), want, name)
