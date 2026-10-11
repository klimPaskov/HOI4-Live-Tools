"""Session integration with fake process metadata/memory; no desktop or native API."""

import dataclasses
from types import SimpleNamespace
import unittest
from unittest.mock import Mock, patch

import tests.helpers  # noqa: F401
from hoi4_live.errors import ToolError
from hoi4_live.memory.layout import VERIFIED_BUILD_KEY
from hoi4_live.memory.winproc import ReadOnlyProcess
from hoi4_live.session import Session
from tests.test_memory_reader import IDENTITY, RTTI, fixture


def session_stub(identity=IDENTITY, window_pid=None):
    session = Session.__new__(Session)
    from hoi4_live.platform.windows import WindowsPlatform
    session.platform = WindowsPlatform()
    session.launched_identity = identity
    session.launched_pid = identity.pid if identity else None
    session.window = SimpleNamespace(pid=IDENTITY.pid if window_pid is None else window_pid,
                                     find=Mock(return_value=True))
    session._memory_process = None
    session._memory_reader = None
    return session


class SessionMemoryIntegration(unittest.TestCase):
    def test_unowned_window_is_rejected_before_process_open_or_read(self):
        for case in ("no_launch", "pid_mismatch", "other_window", "no_window"):
            with self.subTest(case=case):
                session = session_stub()
                if case == "no_launch":
                    session.launched_identity = None
                elif case == "pid_mismatch":
                    session.launched_pid = 999
                elif case == "other_window":
                    session.window.pid = 22000
                else:
                    session.window.find.return_value = False
                with patch("hoi4_live.memory.winproc.WindowsAPI", side_effect=AssertionError("native API forbidden")), \
                     patch("hoi4_live.platform.windows.WindowsPlatform.capture_process_identity") as capture, \
                     patch("hoi4_live.memory.winproc.ReadOnlyProcess") as process, \
                     patch("hoi4_live.memory.pe.load_rtti") as pe:
                    with self.assertRaises(ToolError) as caught:
                        session.memory_snapshot()
                    self.assertEqual(caught.exception.details["error"], "unowned_process")
                    capture.assert_not_called()
                    process.assert_not_called()
                    pe.assert_not_called()

    def test_pid_reuse_is_rejected_before_opening_a_memory_reader(self):
        session = session_stub()
        reused = dataclasses.replace(IDENTITY, creation_time=IDENTITY.creation_time + 1)
        with patch("hoi4_live.memory.winproc.WindowsAPI", side_effect=AssertionError("native API forbidden")), \
             patch("hoi4_live.platform.windows.WindowsPlatform.capture_process_identity", return_value=reused), \
             patch("hoi4_live.memory.winproc.ReadOnlyProcess") as process, \
             patch("hoi4_live.memory.pe.load_rtti") as pe:
            with self.assertRaises(ToolError) as caught:
                session.memory_snapshot()
            self.assertEqual(caught.exception.details["error"], "unowned_process")
            process.assert_not_called()
            pe.assert_not_called()

    def test_owned_snapshot_uses_exact_identity_and_reuses_reader(self):
        session = session_stub()
        api, _layout = fixture()
        def open_fake(identity, *, owned):
            self.assertEqual(identity, IDENTITY)
            self.assertIs(owned, True)
            return ReadOnlyProcess(identity, owned=owned, api=api)
        with patch("hoi4_live.memory.winproc.WindowsAPI", side_effect=AssertionError("native API forbidden")), \
             patch("hoi4_live.platform.windows.WindowsPlatform.capture_process_identity", return_value=IDENTITY) as capture, \
             patch("hoi4_live.memory.winproc.ReadOnlyProcess", side_effect=open_fake) as process, \
             patch("hoi4_live.memory.pe.load_rtti", return_value=(SimpleNamespace(build_key=VERIFIED_BUILD_KEY), RTTI)):
            try:
                first = session.memory_snapshot()
                second = session.memory_snapshot()
                self.assertEqual(first, second)
                self.assertEqual(first["country"]["name"], "German Reich")
                process.assert_called_once_with(IDENTITY, owned=True)
                self.assertEqual(capture.call_count, 2)
            finally:
                if session._memory_process is not None:
                    session._memory_process.close()
        self.assertEqual(api.closed, [99])

    def test_owned_unknown_build_closes_reader_and_returns_fallback_error(self):
        session = session_stub()
        api, _layout = fixture()
        with patch("hoi4_live.memory.winproc.WindowsAPI", side_effect=AssertionError("native API forbidden")), \
             patch("hoi4_live.platform.windows.WindowsPlatform.capture_process_identity", return_value=IDENTITY), \
             patch("hoi4_live.memory.winproc.ReadOnlyProcess", side_effect=lambda identity, owned: ReadOnlyProcess(identity, owned=owned, api=api)), \
             patch("hoi4_live.memory.pe.load_rtti", return_value=(SimpleNamespace(build_key="unknown"), RTTI)):
            with self.assertRaises(ToolError) as caught:
                session.memory_snapshot()
        self.assertEqual(caught.exception.details["error"], "memory_unavailable")
        self.assertEqual(caught.exception.details["fallback"], "hoi4_briefing")
        self.assertEqual(api.closed, [99])
        self.assertEqual(api.reads, [])
        self.assertIsNone(session._memory_reader)

    def test_cached_reader_cannot_bypass_new_ownership_validation(self):
        session = session_stub()
        api, _layout = fixture()
        process = ReadOnlyProcess(IDENTITY, owned=True, api=api)
        session._memory_process = process
        session._memory_reader = Mock()
        with process, patch("hoi4_live.memory.winproc.WindowsAPI", side_effect=AssertionError("native API forbidden")), \
             patch("hoi4_live.platform.windows.WindowsPlatform.capture_process_identity", return_value=dataclasses.replace(IDENTITY, creation_time=1)):
            with self.assertRaises(ToolError):
                session.memory_snapshot()
        session._memory_reader.snapshot.assert_not_called()
        self.assertEqual(api.reads, [])


if __name__ == "__main__":
    unittest.main()
