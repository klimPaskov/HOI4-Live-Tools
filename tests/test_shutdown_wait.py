"""hoi4_shutdown waits until the owned game process is gone before it restores anything (issue #29), and the DLC
restore gives a late-clearing process list a short grace. Everything runs on a fake process table with a fake clock."""

import unittest
from types import SimpleNamespace
from unittest import mock

import helpers  # noqa: F401
from hoi4_live.game import process
from hoi4_live.tools import session_tools


class FakeClock:
    def __init__(self):
        self.now = 0.0

    def __call__(self):
        return self.now

    def sleep(self, seconds):
        self.now += seconds


class FakePlatform:
    """pid 7 exits at `dead_at` (exit code seen) but stays in the process list until `listed_until`."""

    def __init__(self, clock, dead_at, listed_until, created=111, reused_at=None, reused_created=222):
        self.clock, self.dead_at, self.listed_until = clock, dead_at, listed_until
        self.created, self.reused_at, self.reused_created = created, reused_at, reused_created

    def pid_alive(self, pid):
        return pid == 7 and self.clock.now < self.dead_at

    def find_processes(self, name):
        return [7] if self.clock.now < self.listed_until else []

    def capture_process_identity(self, pid, expected=None):
        created = self.reused_created if self.reused_at is not None and self.clock.now >= self.reused_at else self.created
        return SimpleNamespace(pid=pid, creation_time=created, executable_path="x")


class WaitGoneTests(unittest.TestCase):
    def wait(self, plat, clock, timeout=60, created=111):
        return process.wait_gone(7, created, "hoi4.exe", timeout, plat=plat, clock=clock, sleep=clock.sleep)

    def test_a_process_that_exits_late_is_waited_for(self):
        clock = FakeClock()
        plat = FakePlatform(clock, dead_at=2, listed_until=6)
        self.assertTrue(self.wait(plat, clock))
        self.assertGreaterEqual(clock.now, 6)              # not at 2 s, when only the exit code said "exited"

    def test_an_exit_code_alone_is_not_enough_while_the_list_still_shows_it(self):
        clock = FakeClock()
        plat = FakePlatform(clock, dead_at=0, listed_until=100)
        self.assertFalse(self.wait(plat, clock, timeout=10))
        self.assertGreaterEqual(clock.now, 10)

    def test_a_process_that_never_exits_times_out(self):
        clock = FakeClock()
        plat = FakePlatform(clock, dead_at=1000, listed_until=1000)
        self.assertFalse(self.wait(plat, clock, timeout=5))

    def test_a_reused_pid_counts_as_gone(self):
        clock = FakeClock()
        plat = FakePlatform(clock, dead_at=1000, listed_until=1000, reused_at=3)
        self.assertTrue(self.wait(plat, clock))
        self.assertLess(clock.now, 10)                     # new creation time: another process owns the pid now

    def test_an_unreadable_identity_keeps_waiting(self):
        clock = FakeClock()
        plat = FakePlatform(clock, dead_at=4, listed_until=4)
        plat.capture_process_identity = mock.Mock(side_effect=OSError("denied"))
        self.assertTrue(self.wait(plat, clock))
        self.assertGreaterEqual(clock.now, 4)

    def test_wait_no_game_polls_the_process_list(self):
        clock = FakeClock()
        seen = []

        def pids(name):
            seen.append(clock.now)
            return [9] if clock.now < 3 else []
        with mock.patch.object(process, "running_pids", side_effect=pids):
            self.assertTrue(process.wait_no_game("hoi4.exe", 15, clock=clock, sleep=clock.sleep))
            clock.now = 0
            with mock.patch.object(process, "running_pids", return_value=[9]):
                self.assertFalse(process.wait_no_game("hoi4.exe", 2, clock=clock, sleep=clock.sleep))


class ShutdownTests(unittest.TestCase):
    def make_session(self):
        session = mock.Mock()
        session.cfg = {"window": {"process_name": "hoi4.exe"}}
        session.window.pid = 7
        session.launched_pid = 7
        session.launched_identity = SimpleNamespace(pid=7, creation_time=111, executable_path="x")
        session.restore_dlc_after_exit.return_value = {"restored": ["a"], "removed": [], "conflicts": {}}
        session.config_session = None
        return session

    def run_shutdown(self, wait_gone_result):
        session = self.make_session()
        with mock.patch.object(session_tools.menu, "quit_to_desktop"), \
                mock.patch.object(session_tools.process, "wait_gone", return_value=wait_gone_result) as gone, \
                mock.patch.object(session_tools.process, "wait_exit", side_effect=AssertionError("exit code only")):
            res = session_tools.hoi4_shutdown.__wrapped__(session, {"confirm": True, "force": False})
        return session, res, gone

    def test_the_restore_runs_only_after_the_process_is_gone(self):
        session, res, gone = self.run_shutdown(True)
        self.assertTrue(res.data["exited"])
        session.restore_dlc_after_exit.assert_called_once_with()
        args = gone.call_args[0]
        self.assertEqual(args[:3], (7, 111, "hoi4.exe"))        # the owned pid with its creation time

    def test_a_process_that_is_still_there_is_reported_and_nothing_is_restored(self):
        session, res, _ = self.run_shutdown(False)
        self.assertFalse(res.data["exited"])
        self.assertIsNone(res.data["dlc_restore"])
        session.restore_dlc_after_exit.assert_not_called()


if __name__ == "__main__":
    unittest.main()
