"""Automatic UI bookkeeping never parks the cursor or scrolls the map."""
import sys
import unittest
from unittest import mock
import helpers
from hoi4_live.session import Session
from hoi4_live.platform.macos_desktop import Input


class CursorStationaryTests(unittest.TestCase):
    def test_session_park_does_not_read_config_or_move_input(self):
        session = mock.Mock()
        Session.park_cursor(session)
        self.assertEqual(session.mock_calls, [])

    def test_macos_legacy_park_never_moves_or_activates(self):
        inputs = mock.Mock()
        Input.park(inputs, .553, .003)
        self.assertEqual(inputs.mock_calls, [])

    @unittest.skipUnless(sys.platform == "win32", 'Windows native input backend')
    def test_windows_legacy_park_never_moves_or_activates(self):
        from hoi4_live.platform.windows_input import Input as WinInput
        inputs = mock.Mock()
        WinInput.park(inputs, .553, .003)
        self.assertEqual(inputs.mock_calls, [])
