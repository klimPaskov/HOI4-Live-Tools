"""The launch failures seen on 1.19.4.1 (2 November 2026): a -debug MAP_ERROR dialog, a missed START click, a mod's own
country screen and a console hidden behind a panel. Everything runs on fakes and synthetic screenshots."""

import os
import tempfile
import unittest
from types import SimpleNamespace
from unittest import mock

import numpy as np

import helpers  # noqa: F401
from hoi4_live.errors import ToolError
from hoi4_live.game import console, launch_errors, menu
from hoi4_live.game.logs import Logs
from hoi4_live.tools import session_tools


def lines(*texts):
    return SimpleNamespace(lines=[SimpleNamespace(text=t, center=(10, 10), box=[0, 0, 10, 10]) for t in texts])


class MapErrorTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory(prefix="hoi4live_launch_")
        self.addCleanup(self.tmp.cleanup)
        self.logs = Logs(self.tmp.name)
        self.session = SimpleNamespace(logs=self.logs, grab=mock.Mock(side_effect=RuntimeError("no screen")))

    def write(self, text):
        with open(os.path.join(self.tmp.name, "error.log"), "a", encoding="utf-8") as f:
            f.write(text)

    def test_lines_are_read_since_the_launch_mark_and_folded(self):
        self.write("[10:00:00][old.cpp:1]: MAP_ERROR: before the launch\n")
        self.logs.mark("launch", ("error",))
        self.write("[10:00:05][map.cpp:9]: MAP_ERROR: province 4711 has no state\n"
                   "[10:00:06][map.cpp:9]: MAP_ERROR: province 4711 has no state\n"
                   "[10:00:07][other.cpp:3]: unrelated\n")
        self.assertEqual(launch_errors.map_error_lines(self.session),
                         ["[10:00:05][map.cpp:9]: MAP_ERROR: province 4711 has no state"])

    def test_debug_launch_fails_at_once_with_the_error_text(self):
        self.logs.mark("launch", ("error",))
        self.write("[10:00:05][map.cpp:9]: MAP_ERROR: province 4711 has no state\n")
        with self.assertRaises(ToolError) as cm:
            launch_errors.check(self.session, debug=True)
        self.assertEqual(cm.exception.details["error"], "map_error_on_launch")
        self.assertIn("province 4711", str(cm.exception))
        self.assertIn("still running", str(cm.exception))

    def test_only_map_errors_match(self):
        self.assertIsNone(launch_errors.map_error_on_screen(["Failed to load bitmap error.dds"]))
        self.assertIsNotNone(launch_errors.map_error_on_screen(["[map.cpp:9]: MAP_ERROR: x"]))
        self.assertIsNotNone(launch_errors.map_error_on_screen(["Map Error: bad adjacency"]))

    def test_a_normal_launch_only_reports_them(self):
        self.logs.mark("launch", ("error",))
        self.write("[10:00:05][map.cpp:9]: MAP_ERROR: province 4711 has no state\n")
        launch_errors.check(self.session, debug=False)

    def test_the_dialog_text_on_screen_counts_even_without_a_log_line(self):
        self.logs.mark("launch", ("error",))
        with self.assertRaises(ToolError) as cm:
            launch_errors.check(self.session, debug=True, texts=["Single Player", "MAP ERROR: bad adjacency", "OK"])
        self.assertEqual(cm.exception.details["dialog_text"], "MAP ERROR: bad adjacency")
        launch_errors.check(self.session, debug=True, texts=["Single Player", "Multiplayer"])

    def run_launch(self, debug, menu_visible):
        cfg = {"window": {"process_name": "hoi4"}, "launch": {"args": [], "debug_args": ["-debug"]}}
        session = mock.Mock()
        session.mode = "test"
        session.cfg = type("Cfg", (dict,), {})(cfg)
        session.cfg.user_dir = self.tmp.name
        session.cfg.game_dir = self.tmp.name
        session.cfg.game_exe = os.path.join(self.tmp.name, "hoi4.exe")
        session.logs = self.logs
        found = iter([False, False])                     # not running before the launch, a window after it
        session.window.find.side_effect = lambda: next(found, True)
        session.platform.capture_process_identity.return_value = SimpleNamespace(
            pid=7, creation_time=1, executable_path="x")
        sleeps = []

        def sleep(seconds):
            sleeps.append(seconds)
            if len(sleeps) == 1:                         # the 20 s settle: the game writes its error now
                self.write("[10:00:05][map.cpp:9]: MAP_ERROR: province 4711 has no state\n")
        session.sleep.side_effect = sleep
        session.logs.idle_seconds = mock.Mock(return_value=60.0)
        pids = iter([[]])                                # no game before the launch, one after it
        args = {"debug": debug, "wait_menu": True, "restart": False, "timeout_s": 120, "mode": None}
        patches = (mock.patch.object(session_tools.process, "running_pids", side_effect=lambda *a: next(pids, [7])),
                   mock.patch.object(session_tools.process, "crash_dirs", return_value=set()),
                   mock.patch.object(session_tools.process, "launch", return_value=7),
                   mock.patch.object(session_tools.menu, "main_menu_visible",
                                     return_value=(menu_visible, ["Single Player"] if menu_visible else [])),
                   mock.patch.object(session_tools, "attach", side_effect=lambda r, *a, **k: r))
        return session, args, patches

    def test_hoi4_launch_stops_on_a_map_error_before_the_menu_wait_ends(self):
        session, args, patches = self.run_launch(debug=True, menu_visible=False)
        with patches[0], patches[1], patches[2], patches[3], patches[4]:
            with self.assertRaises(ToolError) as cm:
                session_tools.hoi4_launch.__wrapped__(session, args)
        self.assertEqual(cm.exception.details["error"], "map_error_on_launch")

    def test_a_normal_launch_reaches_the_menu_and_lists_the_lines(self):
        session, args, patches = self.run_launch(debug=False, menu_visible=True)
        with patches[0], patches[1], patches[2], patches[3], patches[4]:
            res = session_tools.hoi4_launch.__wrapped__(session, args)
        self.assertEqual(len(res.data["map_errors"]), 1)


class StartRetryTests(unittest.TestCase):
    def ready_screen(self):
        return (mock.patch.object(menu.ui, "classify", return_value=({"screen": "unknown"}, None)),
                mock.patch.object(menu.ui, "ocr_screen", return_value=(None, lines("START", "Back to menu"))))

    def test_start_is_clicked_at_most_three_more_times_then_the_call_fails(self):
        session = mock.Mock()
        p1, p2 = self.ready_screen()
        with p1, p2, mock.patch.object(menu.ui, "click_text") as click, \
                mock.patch.object(menu, "START_RETRY_SECONDS", -1), \
                mock.patch("hoi4_live.perception.topbar.read_date", return_value={"error": "no date"}):
            with self.assertRaises(ToolError) as cm:
                menu.new_game(session, timeout=600)
        self.assertEqual(click.call_count, 1 + menu.START_MAX_RETRIES)
        self.assertEqual(cm.exception.details["error"], "start_not_registered")

    def test_a_retry_that_works_reports_how_many_were_needed(self):
        session = mock.Mock()
        p1, p2 = self.ready_screen()
        dates = [{"error": "x"}, {"error": "x"}, {"hoi4": "1936.1.1.12"}]
        with p1, p2, mock.patch.object(menu.ui, "click_text") as click, \
                mock.patch.object(menu, "START_RETRY_SECONDS", -1), \
                mock.patch("hoi4_live.perception.topbar.read_date", side_effect=dates):
            out = menu.new_game(session)
        self.assertEqual(click.call_count, 3)
        self.assertEqual(out["start_retries"], 2)

    def test_a_country_asked_for_on_the_ready_screen_is_reported_as_not_applied(self):
        session = mock.Mock()
        p1, p2 = self.ready_screen()
        with p1, p2, mock.patch.object(menu.ui, "click_text"),                 mock.patch("hoi4_live.perception.topbar.read_date", return_value={"hoi4": "1936.1.1.12"}):
            out = menu.new_game(session, country="Poland")
        self.assertIn("not applied", out["steps"][1]["note"])

    def test_a_click_that_registers_is_not_repeated(self):
        session = mock.Mock()
        p1, p2 = self.ready_screen()
        with p1, p2, mock.patch.object(menu.ui, "click_text") as click, \
                mock.patch("hoi4_live.perception.topbar.read_date", return_value={"hoi4": "1936.1.1.12"}):
            out = menu.new_game(session)
        self.assertEqual(click.call_count, 1)
        self.assertEqual(out["start_retries"], 0)


class CountryScreenTests(unittest.TestCase):
    VANILLA = ["Select Country", "Other countries", "Back to menu"]
    SCENARIO = ["Select Scenario", "Brief history", "Back to menu"]
    READY = ["START", "Back to menu", "Custom game rules"]

    def test_screens_are_told_apart(self):
        self.assertEqual(menu.pregame_stage(self.VANILLA), "select_country")
        self.assertEqual(menu.pregame_stage(self.SCENARIO), "select_scenario")
        self.assertEqual(menu.pregame_stage(self.READY), "ready")
        self.assertEqual(menu.pregame_stage(["Choose your nation", "Back to menu"]), "custom_country")
        self.assertEqual(menu.pregame_stage(["Zinovia", "Confirm", "Back to menu"]), "custom_country")
        # precision over recall: a description that says "pick a nation", or Play/Continue next to Back to menu
        # without a heading, is not taken for a country screen
        self.assertIsNone(menu.pregame_stage(["Pick a nation that suits you and play", "Back to menu"]))
        self.assertIsNone(menu.pregame_stage(["Zinovia", "Play", "Back to menu"]))
        self.assertIsNone(menu.pregame_stage(["Loading", "Please wait"]))
        self.assertIsNone(menu.pregame_stage(["Single Player", "Multiplayer", "Continue"]))

    def test_wait_returns_a_custom_screen_only_once_it_has_settled(self):
        reads = [lines("Back to menu"), lines("Choose your nation", "Back to menu"),
                 lines("Choose your nation", "Back to menu")]
        session = mock.Mock()
        with mock.patch.object(menu.ui, "ocr_screen", side_effect=[(None, r) for r in reads]) as ocr:
            self.assertEqual(menu._wait_country_stage(session, timeout=30), "custom_country")
        self.assertEqual(ocr.call_count, 3)

    def test_wait_times_out_with_the_text_it_saw(self):
        session = mock.Mock()
        with mock.patch.object(menu.ui, "ocr_screen", return_value=(None, lines("Something else"))), \
                mock.patch.object(menu.time, "time", side_effect=[0, 0, 5, 5, 999, 999]):
            with self.assertRaises(ToolError) as cm:
                menu._wait_country_stage(session, timeout=30)
        self.assertEqual(cm.exception.details["error"], "country_screen_missing")
        self.assertEqual(cm.exception.details["seen"], ["Something else"])

    def test_new_game_goes_through_a_custom_country_screen(self):
        session = mock.Mock()
        clicked = []
        with mock.patch.object(menu.ui, "classify", return_value=({"screen": "unknown"}, None)), \
                mock.patch.object(menu.ui, "ocr_screen", return_value=(None, lines(*self.SCENARIO))), \
                mock.patch.object(menu, "_wait_country_stage", return_value="custom_country"), \
                mock.patch.object(menu, "_wait_text"), \
                mock.patch.object(menu.ui, "click_text",
                                  side_effect=lambda s, pat, **kw: clicked.append((pat, kw)) or {"text": pat}):
            out = menu.new_game(session, country="Zinovia", start=False)
        self.assertEqual([c[0] for c in clicked][-2:], ["Zinovia", menu.FORWARD_BUTTON_RX])
        self.assertTrue(clicked[-1][1]["exact"])
        self.assertTrue(out["prepared"])

    def test_a_custom_screen_without_the_country_says_what_to_do(self):
        session = mock.Mock()
        with mock.patch.object(menu.ui, "click_text", side_effect=ToolError("not found", {"seen": ["Tag A", "Tag B"]})):
            with self.assertRaises(ToolError) as cm:
                menu._custom_country(session, "Zinovia")
        self.assertEqual(cm.exception.details["error"], "custom_country_screen")
        self.assertEqual(cm.exception.details["seen"], ["Tag A", "Tag B"])
        self.assertIn("hoi4_new_game again", str(cm.exception))

    def test_a_custom_screen_without_a_confirm_button_says_what_to_do(self):
        session = mock.Mock()
        with mock.patch.object(menu.ui, "click_text", side_effect=ToolError("not found", {"seen": ["x"]})):
            with self.assertRaises(ToolError) as cm:
                menu._custom_country(session, None)
        self.assertEqual(cm.exception.details["error"], "custom_country_screen")

    def test_new_game_resumes_from_a_custom_screen_that_is_already_open(self):
        session = mock.Mock()
        with mock.patch.object(menu.ui, "classify", return_value=({"screen": "unknown"}, None)), \
                mock.patch.object(menu.ui, "ocr_screen",
                                  return_value=(None, lines("Choose your nation", "Back to menu"))), \
                mock.patch.object(menu, "_custom_country", return_value=[{"text": "Play"}]) as custom, \
                mock.patch.object(menu, "_wait_text"):
            out = menu.new_game(session, country="Zinovia", start=False)
        custom.assert_called_once_with(session, "Zinovia")
        self.assertTrue(out["prepared"])


def noise(seed=1):
    return np.random.RandomState(seed).randint(0, 255, (1080, 1920, 3)).astype(np.uint8)


def with_dark_corner(img):
    out = img.copy()
    out[60:700, :700] = 20
    return out


class ConsoleBlindTests(unittest.TestCase):
    """The console frame is not seen (a sea-region panel, a total conversion's interface): a toggle that visibly
    changes the console area counts as opened, and closing confirms the same way. Nothing is kept between calls."""

    def session(self, *screens):
        s = mock.Mock()
        s.cfg = {"console": {"toggle_key": "grave", "slide_seconds": 0}}
        s.grab.side_effect = list(screens)
        return s

    def setUp(self):
        p = mock.patch.object(console.screen_mod, "console_open", return_value=False)
        self.frame = p.start()
        self.addCleanup(p.stop)

    def test_open_by_screen_change_and_close_by_screen_change(self):
        closed, opened = noise(), with_dark_corner(noise())
        s = self.session(closed, opened, opened, closed)
        self.assertEqual(console.open_console(s), "blind")
        s.input.key.assert_called_once_with("grave")
        self.assertTrue(console.close_console(s, blind=True))
        self.assertEqual(s.input.key.call_count, 2)

    def test_the_frame_is_still_the_normal_proof(self):
        self.frame.return_value = True
        s = self.session(noise())
        self.assertEqual(console.open_console(s), "frame")
        s.input.key.assert_not_called()

    def test_nothing_changing_after_three_toggles_is_a_failure_with_a_code(self):
        still = noise()
        s = self.session(*([still] * 4))
        with self.assertRaises(ToolError) as cm:
            console.open_console(s)
        self.assertEqual(cm.exception.details["error"], "console_not_open")
        self.assertEqual(s.input.key.call_count, 3)

    def test_a_blind_close_that_changes_nothing_fails_after_three_tries(self):
        still = with_dark_corner(noise())
        s = self.session(*([still] * 6))
        with self.assertRaises(ToolError) as cm:
            console.close_console(s, blind=True)
        self.assertEqual(cm.exception.details["error"], "console_not_closed")
        self.assertEqual(s.input.key.call_count, 3)

    def test_zone_change_ignores_the_rest_of_the_screen(self):
        a = noise()
        b = a.copy()
        b[800:1000, 1000:1900] = 0                      # bottom right: not where the console draws
        self.assertLess(console.zone_change(a, b), 0.01)
        self.assertGreater(console.zone_change(a, with_dark_corner(a)), console.BLIND_CHANGE)


class ConsoleRunTests(unittest.TestCase):
    def run_console(self, opens, **kw):
        s = mock.Mock()
        s.cfg = {"console": {"toggle_key": "grave", "slide_seconds": 0}}
        with mock.patch.object(console, "open_console", side_effect=opens),                 mock.patch.object(console, "close_console") as close,                 mock.patch.object(console, "read_output", return_value=([], None)),                 mock.patch.object(console, "clipboard"),                 mock.patch("hoi4_live.game.panels.to_map"),                 mock.patch("hoi4_live.game.menu.clear_windows") as clear:
            try:
                out = console.run(s, ["time"], **kw)
            except ToolError as exc:
                out = exc
        return s, out, close, clear

    def test_run_closes_windows_once_when_the_console_stays_hidden(self):
        s, out, close, clear = self.run_console([ToolError("no", {"error": "console_not_open"}), "frame"])
        clear.assert_called_once_with(s)
        close.assert_called_once_with(s)
        self.assertEqual(out[0]["command"], "time")

    def test_other_errors_are_not_retried_that_way(self):
        s, out, close, clear = self.run_console([ToolError("capture failed")])
        self.assertIsInstance(out, ToolError)
        clear.assert_not_called()

    def test_a_blind_console_is_closed_blind_in_the_same_call(self):
        s, out, close, clear = self.run_console(["blind"])
        close.assert_called_once_with(s, blind=True)
        self.assertEqual(out[0]["command"], "time")

    def test_keep_open_cannot_be_tracked_blind_so_it_is_closed_and_refused(self):
        s, out, close, clear = self.run_console(["blind"], keep_open=True)
        close.assert_called_once_with(s, blind=True)
        self.assertEqual(out.details["error"], "console_blind_keep_open")


if __name__ == "__main__":
    unittest.main()
