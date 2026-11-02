"""Windows window discovery after the bound game exits (seen live on 1.19.4.1: a relaunch in the same server
failed with "owned process has exited")."""
import sys
import unittest
from collections import namedtuple
from unittest.mock import Mock, patch

from hoi4_live.errors import ToolError

Identity = namedtuple("Identity", "pid creation_time executable_path")
OLD = Identity(41, 1000, r"C:\games\hoi4.exe")
NEW = Identity(52, 2000, r"C:\games\hoi4.exe")


class Exited(RuntimeError):
    pass


@unittest.skipUnless(sys.platform == "win32", "Windows window backend")
class ExitedGameTests(unittest.TestCase):
    def setUp(self):
        from hoi4_live.platform.windows_window import GameWindow
        self.backend = Mock()
        self.window = GameWindow(process_backend=self.backend, expected_identity=OLD)
        self.win32 = patch("hoi4_live.platform.windows_window.win32").start()
        self.addCleanup(patch.stopall)
        self.win32.find_processes.return_value = []
        self.win32.enum_windows.return_value = []

    def test_exited_bound_process_has_no_window_and_is_forgotten(self):
        self.backend.capture_process_identity.side_effect = Exited("owned process has exited")
        self.win32.pid_alive.return_value = False
        self.assertIsNone(self.window.find())
        self.assertIsNone(self.window.pid)
        self.window.bind_identity(NEW)        # a relaunch can bind its new process
        self.assertEqual(self.window.pid, NEW.pid)

    def test_live_process_that_cannot_be_verified_still_fails(self):
        self.backend.capture_process_identity.side_effect = Exited("access denied")
        self.win32.pid_alive.return_value = True
        with self.assertRaises(Exited):
            self.window.find()

    def test_reused_pid_is_still_refused(self):
        self.backend.capture_process_identity.return_value = Identity(41, 9999, OLD.executable_path)
        with self.assertRaisesRegex(ToolError, "identity changed"):
            self.window.find()


if __name__ == "__main__":
    unittest.main()
