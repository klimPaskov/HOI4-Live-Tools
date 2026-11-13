"""hoi4_launch stops early when the game dies on a map load error (issue #30, seen on 1.19.4.2 with a total
conversion): the owned process exited with 0xC0000409 and the game left a child hoi4.exe started with
-error_title "Failed to load the map". Fake process table, fake clock, no game."""

import os
import tempfile
import unittest
from types import SimpleNamespace
from unittest import mock

import helpers  # noqa: F401
from hoi4_live.errors import ToolError
from hoi4_live.game import launch_errors, process
from hoi4_live.game.logs import Logs
from hoi4_live.tools import session_tools

DIALOG = '"C:\\game\\hoi4.exe" -error_title "Failed to load the map" -error_message "Map load failed, see error.log"'


class FakePlatform:
    def __init__(self):
        self.listed = {}                       # pid -> command line, what the process list shows
        self.alive = set()

    def find_processes(self, name):
        return sorted(self.listed)

    def pid_alive(self, pid):
        return pid in self.alive

    def capture_process_identity(self, pid, expected=None):
        return SimpleNamespace(pid=pid, creation_time=1, executable_path="x")


class EarlyFailureTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory(prefix="hoi4live_early_")
        self.addCleanup(self.tmp.cleanup)
        self.plat = FakePlatform()
        for patcher in (mock.patch.object(process.platform, "current", return_value=self.plat),
                        mock.patch.object(process, "command_line", side_effect=lambda pid: self.plat.listed.get(pid)),
                        mock.patch.object(process, "crash_dirs", return_value=set()),
                        mock.patch.object(process, "launch", side_effect=self.start_game),
                        mock.patch.object(session_tools, "attach", side_effect=lambda r, *a, **k: r)):
            patcher.start()
            self.addCleanup(patcher.stop)
        self.logs = Logs(self.tmp.name)
        self.after = {"listed": {}, "alive": set()}      # the process table once the game has been started

    def start_game(self, game_dir, args):
        self.plat.listed, self.plat.alive = dict(self.after["listed"]), set(self.after["alive"])
        return 7

    def write_error_log(self, text):
        with open(os.path.join(self.tmp.name, "error.log"), "w", encoding="utf-8") as f:
            f.write(text)

    def session(self, windows=False):
        cfg = {"window": {"process_name": "hoi4.exe"}, "launch": {"args": [], "debug_args": ["-debug"]}}
        session = mock.Mock()
        session.mode = "test"
        session.cfg = type("Cfg", (dict,), {})(cfg)
        session.cfg.user_dir = session.cfg.game_dir = self.tmp.name
        session.cfg.game_exe = os.path.join(self.tmp.name, "hoi4.exe")
        session.logs = self.logs
        session.logs.idle_seconds = mock.Mock(return_value=60.0)
        session.launched_identity = None
        session.platform.capture_process_identity.side_effect = lambda pid, exe: SimpleNamespace(
            pid=pid, creation_time=1, executable_path=exe)
        self.found = iter([False, False])             # not running before the launch
        session.window.find.side_effect = lambda: next(self.found, windows)
        self.sleeps = []
        session.sleep.side_effect = self.sleeps.append
        return session

    def launch(self, session):
        args = {"debug": False, "wait_menu": True, "restart": False, "timeout_s": 900, "mode": None}
        with mock.patch.object(session_tools.menu, "main_menu_visible", return_value=(False, [])), \
                mock.patch.object(process, "launch_status",
                                  return_value={"pid": 7, "running": False, "exit_code": 0xC0000409}):
            return session_tools.hoi4_launch.__wrapped__(session, args)

    def test_a_crash_with_the_error_dialog_helper_fails_at_once_with_title_and_log(self):
        self.write_error_log("[10:00:01][map.cpp:9]: Province 4711 is not valid\n[10:00:02][map.cpp:9]: bad map\n")
        self.after = {"listed": {9: DIALOG}, "alive": set()}    # the owned pid 7 is gone, only the dialog helper remains
        session = self.session()
        with self.assertRaises(ToolError) as cm:
            self.launch(session)
        d = cm.exception.details
        self.assertEqual(d["error"], "game_failed_during_launch")
        self.assertEqual(d["dialog_title"], "Failed to load the map")
        self.assertEqual(d["dialog_message"], "Map load failed, see error.log")
        self.assertEqual(d["exit_code_hex"], "0xC0000409")
        self.assertIn("Province 4711 is not valid", d["error_log"][0])
        self.assertIn("Failed to load the map", str(cm.exception))
        self.assertLessEqual(len(self.sleeps), 1)        # nowhere near the 900 s timeout

    def test_the_dialog_with_the_owned_game_still_listed_also_fails(self):
        self.after = {"listed": {7: '"C:\\game\\hoi4.exe"', 9: DIALOG}, "alive": {7}}
        session = self.session(windows=True)
        with self.assertRaises(ToolError) as cm:
            self.launch(session)
        self.assertEqual(cm.exception.details["dialog_title"], "Failed to load the map")
        self.assertIn("still running", str(cm.exception))

    def test_an_exit_without_any_dialog_is_reported_with_its_exit_code(self):
        self.write_error_log("[10:00:01][engine.cpp:1]: fatal\n")
        session = self.session(windows=True)
        with self.assertRaises(ToolError) as cm:
            self.launch(session)
        self.assertIn("crashed during startup", str(cm.exception))
        self.assertEqual(cm.exception.details["exit_code_hex"], "0xC0000409")
        self.assertIsNone(cm.exception.details.get("dialog_title"))

    def test_a_healthy_launch_is_not_interrupted(self):
        self.plat.listed = {7: '"C:\\game\\hoi4.exe"'}
        self.plat.alive = {7}
        session = self.session(windows=True)
        launch_errors.check_process(session, 7)           # returns without raising
        self.assertEqual(process.error_dialogs(7, "hoi4.exe"), [])

    def test_the_dialog_arguments_are_parsed(self):
        parsed = process.parse_error_dialog(DIALOG)
        self.assertEqual((parsed["title"], parsed["message"]), ("Failed to load the map", "Map load failed, see error.log"))
        self.assertIsNone(process.parse_error_dialog('"C:\\game\\hoi4.exe" -debug'))
        self.assertIsNone(process.parse_error_dialog(None))
        bare = process.parse_error_dialog("hoi4.exe -error_title Oops")
        self.assertEqual((bare["title"], bare["message"]), ("Oops", None))


if __name__ == "__main__":
    unittest.main()
