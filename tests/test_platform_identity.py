"""Ownership is creation time plus executable, never a recycled PID."""
import os
import sys
import unittest
from unittest.mock import Mock

from hoi4_live.platform.macos_process import capture_process_identity


class IdentityTests(unittest.TestCase):
    def api(self):
        return Mock(start_time=Mock(return_value=1000001), image=Mock(return_value='/games/hoi4'))

    def test_exact_microsecond_identity(self):
        result = capture_process_identity(42, '/games/hoi4', api=self.api())
        self.assertEqual((result.pid, result.creation_time, result.executable_path),
                         (42, 1000001, '/games/hoi4'))

    def test_pid_reuse_during_metadata_query_is_rejected(self):
        api = self.api()
        api.start_time.side_effect = [1000001, 1000002]
        with self.assertRaises(OSError):
            capture_process_identity(42, '/games/hoi4', api=api)

    def test_wrong_executable_or_relative_path_is_rejected(self):
        for path in ('/games/another', 'hoi4'):
            with self.subTest(path=path), self.assertRaises(OSError):
                capture_process_identity(42, path, api=self.api())

    @unittest.skipUnless(sys.platform == 'darwin', 'libproc requires macOS')
    def test_native_self_identity_is_stable_without_task_port(self):
        first = capture_process_identity(os.getpid())
        self.assertEqual(first, capture_process_identity(os.getpid(), first.executable_path))
