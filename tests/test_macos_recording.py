"""Mac recording injection tests; never inspect or capture the real desktop."""
import argparse
import json
import os
import signal
import subprocess
import sys
import textwrap
import time
from pathlib import Path
import tempfile
import unittest
from unittest import mock

import numpy as np

from hoi4_live.errors import CapabilityUnavailable, ToolError
from hoi4_live.platform.identity import ProcessIdentity
from hoi4_live.platform.macos_recording import FrameSource
from scripts import record_screen as recorder


class Stream:
    def __init__(self):
        self.closed, self.writes = False, []

    def close(self):
        self.closed = True

    def write(self, value):
        self.writes.append(value)

    def flush(self):
        pass


class Process:
    pid = 100
    returncode = None

    def __init__(self):
        self.stdin = Stream()

    def poll(self):
        return self.returncode

    def wait(self, timeout):
        self.returncode = 0
        return 0


class MacRecordingTests(unittest.TestCase):
    def setUp(self):
        temp = tempfile.TemporaryDirectory()
        self.addCleanup(temp.cleanup)
        self.raw = Path(temp.name).resolve()
        self.out = self.raw / "campaign"
        self.out.mkdir()
        self.token = "c" * 32
        self.identity = ProcessIdentity(123, 7654321, "/Games/hoi4.app/Contents/MacOS/hoi4")
        self.options = recorder.parser().parse_args([
            "start", str(self.out), "--raw-root", str(self.raw), "--owned-pid", "123",
            "--owned-created", "7654321", "--owned-exe", self.identity.executable_path,
            "--window-id", "42"])
        self.options.backend = "macos"
        self.budget = {"reason": None, "free_bytes": 10**12, "used_bytes": 0}

    def prepare(self):
        info = dict(version=3, backend="macos", token=self.token, status="starting",
                    raw_root=str(self.raw), out=str(self.out), hwnd=42, fps=24, crf=28,
                    encoder="x264", size="auto", segment=900, cap_bytes=recorder.DEFAULT_CAP,
                    headroom_bytes=recorder.DEFAULT_HEADROOM, owned_game=dict(
                        pid=123, creation_time=7654321, executable_path=self.identity.executable_path))
        recorder.write_json(self.out / "recording.json", info)
        recorder.write_json(self.raw / ".recording-active.json", {"token": self.token})
        return argparse.Namespace(out=str(self.out), token=self.token)

    @unittest.skipUnless(os.name == "posix", "POSIX session and SIGHUP lifecycle")
    def test_supervisor_survives_wrapper_process_group_shutdown(self):
        # A synthetic supervisor only writes metadata. No ffmpeg, native APIs,
        # capture or game process is involved. The wrapper owns a fresh session,
        # so its SIGHUP cannot reach this test runner or an unrelated process.
        child = self.raw / "synthetic_supervisor.py"
        child.write_text(textwrap.dedent("""\
            import json, os, sys, time
            from pathlib import Path
            out = Path(sys.argv[1])
            info_path = out / "recording.json"
            info = json.loads(info_path.read_text())
            info.update(status="running", supervisor_session=os.getsid(0))
            temporary = out / "ready.json"
            temporary.write_text(json.dumps(info))
            temporary.replace(info_path)
            deadline = time.monotonic() + 4
            while not (out / "wrapper_shutdown").exists():
                if time.monotonic() >= deadline:
                    sys.exit(2)
                time.sleep(.01)
            time.sleep(.15)
            (out / "survived.json").write_text(json.dumps({"session": os.getsid(0)}))
            """), encoding="utf-8")
        wrapper = self.raw / "synthetic_wrapper.py"
        wrapper.write_text(textwrap.dedent("""\
            import os, signal, sys
            from pathlib import Path
            sys.path.insert(0, sys.argv[1])
            from scripts import record_screen as recorder
            out, raw, child = sys.argv[2:]
            options = recorder.parser().parse_args([
                "start", out, "--raw-root", raw, "--owned-pid", "123",
                "--owned-created", "7654321", "--owned-exe", "/Games/hoi4",
                "--window-id", "42"])
            options.backend = "macos"
            recorder.budget_status = lambda *args: {"reason": None}
            recorder.supervisor_command = lambda out, token: [sys.executable, child, str(out)]
            recorder.start(options, inspect=lambda *args: (0, 0, 4, 4))
            (Path(out) / "wrapper_shutdown").write_text(str(os.getsid(0)))
            os.killpg(os.getpgrp(), signal.SIGHUP)
            """), encoding="utf-8")
        repo = Path(recorder.__file__).resolve().parent.parent
        process = subprocess.Popen([sys.executable, str(wrapper), str(repo), str(self.out),
                                    str(self.raw), str(child)], stdout=subprocess.PIPE,
                                   stderr=subprocess.PIPE, start_new_session=True)
        try:
            _stdout, stderr = process.communicate(timeout=8)
        finally:
            if process.poll() is None:
                process.kill()
                process.wait(timeout=2)
        self.assertEqual(process.returncode, -signal.SIGHUP, stderr.decode())
        deadline = time.monotonic() + 3
        proof = self.out / "survived.json"
        while not proof.exists() and time.monotonic() < deadline:
            time.sleep(.02)
        self.assertTrue(proof.exists(), "supervisor died with its start wrapper")
        recorded = recorder.read_info(self.out)
        self.assertNotEqual(recorded["supervisor_session"], process.pid)
        self.assertEqual(json.loads(proof.read_text())["session"], recorded["supervisor_session"])

    def test_mac_identity_requires_exact_creation_and_game_executable(self):
        self.assertEqual(recorder.identity_from_args(self.options), self.identity)
        self.options.owned_created = 1.5
        with self.assertRaisesRegex(ValueError, "exact"):
            recorder.identity_from_args(self.options)
        self.options.owned_created = 7654321
        self.options.owned_exe = "/Applications/Other.app/other"
        with self.assertRaisesRegex(ValueError, "hoi4"):
            recorder.identity_from_args(self.options)

    def test_raw_command_has_no_display_input_and_preserves_wall_timestamps(self):
        command = recorder.build_command(self.options, (0, 0, 1600, 900), self.token)
        self.assertEqual(command[command.index("-i") + 1], "pipe:0")
        self.assertIn("rawvideo", command)
        self.assertIn("rgb24", command)
        self.assertIn("-use_wallclock_as_timestamps", command)
        self.assertIn("matroska", command)
        self.assertNotIn("avfoundation", command)
        self.assertNotIn("desktop", command)
        self.options.encoder = "amf"
        with self.assertRaisesRegex(ValueError, "x264"):
            recorder.build_command(self.options, (0, 0, 1600, 900), self.token)

    def test_frame_source_validates_rgb_geometry_and_crops_only_inside(self):
        window, capture = mock.Mock(), mock.Mock()
        window.geometry.return_value = argparse.Namespace(x=-1200, y=0, pixels=(4, 4))
        capture.grab.return_value = np.arange(48, dtype=np.uint8).reshape(4, 4, 3)
        source = FrameSource(self.identity, 42, window=window, capturer=capture)
        self.assertEqual(source.inspect(), (-1200, 0, 4, 4))
        self.assertEqual(source.frame((2, 2)), capture.grab.return_value[:2, :2].tobytes())
        with self.assertRaisesRegex(ValueError, "outside"):
            source.frame((6, 4))
        capture.grab.return_value = np.zeros((2, 4, 3), dtype=np.uint8)
        with self.assertRaisesRegex(RuntimeError, "geometry"):
            source.frame((2, 2))
        window.geometry.side_effect = CapabilityUnavailable("capture", "macos", "window disappeared")
        with self.assertRaises(CapabilityUnavailable):
            source.inspect()

    def test_desktop_integration_binds_exact_window_and_rejects_reused_pid(self):
        from hoi4_live.platform.macos_desktop import GameWindow, Capturer, Geometry
        from hoi4_live.errors import ToolError
        native, backend = mock.Mock(), mock.Mock()
        backend.capture_process_identity.return_value = self.identity
        backend.find_processes.return_value = [self.identity.pid]
        target = dict(id=42, pid=123, title="Hearts of Iron IV", bounds=(0, 0, 4, 4))
        native.windows.return_value = [dict(target, id=99), target]
        native.geometry.return_value = Geometry(0, 0, 4, 4, 1)
        native.capture.return_value = (np.full((4, 4, 3), 90, dtype=np.uint8), "quartz")
        window = GameWindow(expected_identity=self.identity, window_id=42,
                            native=native, process_backend=backend)
        source = FrameSource(self.identity, 42, window=window)
        source.inspect()
        self.assertEqual(len(source.frame((4, 4))), 48)
        self.assertEqual(native.capture.call_args.args[0]["id"], 42)
        native.focus.assert_not_called()
        backend.capture_process_identity.return_value = ProcessIdentity(123, 7654322, self.identity.executable_path)
        with self.assertRaisesRegex(ToolError, "identity changed"):
            source.frame((4, 4))
        self.assertEqual(native.capture.call_count, 1)
        backend.capture_process_identity.return_value = self.identity
        native.capture.return_value = (np.zeros((4, 4, 3), dtype=np.uint8), "quartz")
        self.assertEqual(source.frame((4, 4)), bytes(48))

    def test_frame_pipe_is_bounded_and_handles_partial_writes(self):
        stream = mock.Mock()
        stream.fileno.return_value = 99
        with mock.patch("os.set_blocking", create=True) as mode, mock.patch("select.select", return_value=([], [99], [])), \
                mock.patch("os.write", side_effect=[2, 2]) as write:
            FrameSource.write(stream, b"abcd")
        mode.assert_called_once_with(99, False)
        self.assertEqual(write.call_count, 2)
        with mock.patch("os.set_blocking", create=True), mock.patch("select.select", return_value=([], [], [])):
            with self.assertRaisesRegex(CapabilityUnavailable, "stalled"):
                FrameSource.write(stream, b"abcd")

    def test_supervisor_feeds_only_source_frames_and_closes_pipe_for_stop(self):
        a, process, source = self.prepare(), Process(), mock.Mock()
        source.inspect.return_value = (0, 0, 4, 4)
        source.frame.return_value = bytes(48)
        source.write.side_effect = lambda *_: recorder.request_stop(self.out, self.token)
        def stop(_):
            recorder.request_stop(self.out, self.token)
        with mock.patch.object(recorder, "budget_status", return_value=self.budget):
            info = recorder.run(a, frame_source_factory=mock.Mock(return_value=source),
                                popen=mock.Mock(return_value=process), sleep=stop)
        self.assertEqual(info["status"], "stopped")
        self.assertEqual(info["frames_written"], 1)
        source.write.assert_called_once_with(process.stdin, bytes(48))
        self.assertEqual(process.stdin.writes, [])
        self.assertTrue(process.stdin.closed)
        self.assertFalse((self.raw / ".recording-active.json").exists())

    def test_source_ownership_failure_stops_before_any_frame(self):
        a, process, source = self.prepare(), Process(), mock.Mock()
        source.inspect.return_value = (0, 0, 4, 4)
        source.frame.side_effect = RuntimeError("PID reused")
        with mock.patch.object(recorder, "budget_status", return_value=self.budget):
            info = recorder.run(a, frame_source_factory=mock.Mock(return_value=source),
                                popen=mock.Mock(return_value=process), sleep=mock.Mock())
        self.assertEqual(info["status"], "error")
        self.assertIn("PID reused", info["error"])
        source.write.assert_not_called()
        self.assertFalse(process.stdin.closed)

    def test_running_metadata_reports_new_frames_before_shutdown(self):
        a, process, source = self.prepare(), Process(), mock.Mock()
        source.inspect.return_value = (0, 0, 4, 4)
        source.frame.return_value = bytes(48)
        now = [0.0]
        def sleep(_seconds):
            now[0] += 6
        observed = []
        original = recorder.write_json
        def write(path, value):
            if path.name == "recording.json" and value.get("status") == "running":
                observed.append(dict(value))
                if value.get("frames_written", 0) >= 2:
                    recorder.request_stop(self.out, self.token)
            return original(path, value)
        with mock.patch.object(recorder, "budget_status", return_value=self.budget), \
             mock.patch.object(recorder, "write_json", side_effect=write):
            info = recorder.run(a, frame_source_factory=mock.Mock(return_value=source),
                                popen=mock.Mock(return_value=process), sleep=sleep, monotonic=lambda: now[0])
        self.assertEqual(info["status"], "stopped")
        self.assertEqual([r["frames_written"] for r in observed], [1, 2])
        self.assertIn("last_frame_epoch", observed[-1])
        self.assertIn("supervisor_checked_epoch", observed[-1])

    def test_changed_frame_retries_with_gap_and_fresh_inspection(self):
        a, process, source = self.prepare(), Process(), mock.Mock()
        source.inspect.return_value = (0, 0, 4, 4)
        source.frame.side_effect = [bytes(48), ToolError("changed", {"error": "capture_frame_changed"}), bytes(48)]
        def written(*_):
            if source.write.call_count == 2:
                recorder.request_stop(self.out, self.token)
        source.write.side_effect = written
        with mock.patch.object(recorder, "budget_status", return_value=self.budget):
            info = recorder.run(a, frame_source_factory=mock.Mock(return_value=source),
                                popen=mock.Mock(return_value=process), sleep=mock.Mock())
        self.assertEqual(info["status"], "stopped")
        self.assertEqual(info["frames_written"], 2)
        self.assertFalse(info["continuous_capture"])
        self.assertTrue(info["capture_gaps"][0]["recovered"])
        self.assertEqual(info["capture_gaps"][0]["discarded_frames"], 1)
        self.assertEqual(source.inspect.call_count, 3)

    def test_changed_frame_retry_is_bounded(self):
        a, process, source = self.prepare(), Process(), mock.Mock()
        source.inspect.return_value = (0, 0, 4, 4)
        changed = ToolError("changed", {"error": "capture_frame_changed"})
        source.frame.side_effect = [bytes(48), changed, changed, changed]
        with mock.patch.object(recorder, "budget_status", return_value=self.budget):
            info = recorder.run(a, frame_source_factory=mock.Mock(return_value=source),
                                popen=mock.Mock(return_value=process), sleep=mock.Mock())
        self.assertEqual(info["status"], "error")
        self.assertIn("retry budget", info["error"])
        self.assertEqual(info["frames_written"], 1)
        self.assertFalse(info["capture_gaps"][0]["recovered"])
        self.assertEqual(source.frame.call_count, 4)

    def test_permission_failure_after_first_frame_is_not_retried(self):
        a, process, source = self.prepare(), Process(), mock.Mock()
        source.inspect.return_value = (0, 0, 4, 4)
        source.frame.side_effect = [bytes(48), CapabilityUnavailable("capture", "macos", "permission denied")]
        with mock.patch.object(recorder, "budget_status", return_value=self.budget):
            info = recorder.run(a, frame_source_factory=mock.Mock(return_value=source),
                                popen=mock.Mock(return_value=process), sleep=mock.Mock())
        self.assertEqual(info["status"], "error")
        self.assertEqual(source.frame.call_count, 2)
        self.assertNotIn("capture_gaps", info)

    def test_mac_budget_stop_sends_no_further_frame(self):
        a, source, process = self.prepare(), mock.Mock(), Process()
        source.inspect.return_value = (0, 0, 4, 4)
        with mock.patch.object(recorder, "budget_status", side_effect=[self.budget, dict(self.budget, reason="footage_cap")]):
            info = recorder.run(a, frame_source_factory=mock.Mock(return_value=source),
                                popen=mock.Mock(return_value=process), sleep=mock.Mock())
        self.assertEqual(info["stop_reason"], "footage_cap")
        self.assertEqual(source.write.call_count, 1)
        self.assertEqual(info["frames_written"], 1)
        self.assertTrue(process.stdin.closed)


if __name__ == "__main__":
    unittest.main()
