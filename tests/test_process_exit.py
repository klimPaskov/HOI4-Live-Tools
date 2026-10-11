"""Retained child handles distinguish actual exit from unreaped PID existence."""
import unittest
from unittest.mock import Mock, patch
from hoi4_live.game import process

class ProcessExitTests(unittest.TestCase):
    def test_owned_exited_child_is_reaped_without_pid_poll(self):
        child = Mock()
        child.poll.return_value = -15
        with patch.dict(process._LAUNCHES, {321: child}, clear=True), patch.object(process.platform, "current") as backend:
            self.assertTrue(process.wait_exit(321, 1))
        child.poll.assert_called_once_with()
        backend.assert_not_called()

    def test_untracked_dead_pid_returns_without_mutation(self):
        with patch.dict(process._LAUNCHES, {}, clear=True), patch.object(process.platform, "current") as backend:
            backend.return_value.pid_alive.return_value = False
            self.assertTrue(process.wait_exit(123, 1))
            backend.return_value.pid_alive.assert_called_once_with(123)

if __name__ == "__main__":
    unittest.main()
