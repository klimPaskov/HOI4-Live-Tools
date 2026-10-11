"""Recorder tests never launch ffmpeg, capture a screen, or inspect a live game."""

import argparse
import json
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest
from unittest import mock

from scripts import record_screen as recorder
from hoi4_live.memory.winproc import ProcessIdentity, PROCESS_QUERY_INFORMATION


class FakeStdin:
    def __init__(self):
        self.writes, self.closed = [], False

    def write(self, data):
        self.writes.append(data)

    def flush(self):
        pass

    def close(self):
        self.closed = True


class FakeProcess:
    pid = 777

    def __init__(self, returncode=None, timeouts=0):
        self.returncode, self.timeouts = returncode, timeouts
        self.stdin = FakeStdin()
        self.terminated = self.killed = False

    def poll(self):
        return self.returncode

    def wait(self, timeout):
        if self.timeouts:
            self.timeouts -= 1
            raise subprocess.TimeoutExpired("fake-ffmpeg", timeout)
        self.returncode = 0
        return 0

    def terminate(self):
        self.terminated = True

    def kill(self):
        self.killed = True


class RecorderTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix="hoi4_recorder_test_")
        self.addCleanup(temporary.cleanup)
        self.raw = Path(temporary.name) / "raw"
        self.out = self.raw / "campaign"
        self.out.mkdir(parents=True)
        self.token = "a" * 32
        self.identity = ProcessIdentity(123, 456789012345, r"C:\Game\hoi4.exe")
        self.client = (100, 200, 1600, 900)
        self.good_budget = {"reason": None, "used_bytes": 0, "free_bytes": 10_000_000_000}
        self.a = recorder.parser().parse_args([
            "start", str(self.out), "--raw-root", str(self.raw), "--owned-pid", "123",
            "--owned-created", str(self.identity.creation_time), "--owned-exe", self.identity.executable_path,
            "--hwnd", "0x99"])

    def prepare_run(self):
        info = {"version": 2, "status": "starting", "token": self.token, "out": str(self.out),
                "raw_root": str(self.raw), "owned_game": {"pid": 123,
                "creation_time": self.identity.creation_time, "executable_path": self.identity.executable_path},
                "hwnd": 0x99, "fps": 24, "crf": 28, "encoder": "x264", "size": "auto", "segment": 900,
                "cap_bytes": recorder.DEFAULT_CAP, "headroom_bytes": recorder.DEFAULT_HEADROOM}
        recorder.write_json(self.out / "recording.json", info)
        recorder.write_json(self.raw / ".recording-active.json", {"token": self.token, "out": str(self.out)})
        return argparse.Namespace(out=str(self.out), token=self.token)

    def test_defaults_and_hwnd_client_bounded_command(self):
        self.assertEqual((self.a.encoder, self.a.crf), ("x264", 28))
        cmd = recorder.build_command(self.a, self.client, self.token, executable="fake-ffmpeg")
        self.assertEqual(cmd[cmd.index("-i") + 1], "hwnd=153")
        self.assertNotIn("desktop", cmd)
        self.assertNotIn("h264_amf", cmd)
        self.assertEqual(cmd[cmd.index("-video_size") + 1], "1600x900")
        self.assertEqual(cmd[cmd.index("-offset_x") + 1], "0")
        self.assertIn("matroska", cmd)
        self.assertTrue(cmd[-1].endswith(".mkv"))
        self.a.size = "1920x1080"
        with self.assertRaisesRegex(ValueError, "outside"):
            recorder.build_command(self.a, self.client, self.token)
        self.assertEqual(recorder.capture_size("auto", (0, 0, 1601, 901)), (1600, 900))
        with self.assertRaisesRegex(ValueError, "even"):
            recorder.capture_size("1599x899", self.client)

    def test_owned_identity_query_has_no_vm_reads_and_hwnd_pid_matches(self):
        api = mock.Mock()
        api.open_process.return_value = "query-handle"
        api.get_process_identity.return_value = self.identity
        window = mock.Mock()
        window.user32.IsWindow.return_value = True
        window.user32.IsWindowVisible.return_value = True
        window.user32.IsIconic.return_value = False
        window.window_pid.return_value = self.identity.pid
        window.client_rect.return_value = self.client
        self.assertEqual(recorder.inspect_owned_window(self.identity, 0x99, api, window), self.client)
        api.open_process.assert_called_once_with(123, PROCESS_QUERY_INFORMATION)
        api.read.assert_not_called()
        api.close.assert_called_once_with("query-handle")
        api.get_process_identity.return_value = ProcessIdentity(123, self.identity.creation_time + 1,
                                                              self.identity.executable_path)
        with self.assertRaisesRegex(RuntimeError, "reused"):
            recorder.inspect_owned_window(self.identity, 0x99, api, window)
        api.get_process_identity.return_value = self.identity
        window.window_pid.return_value = 999
        with self.assertRaisesRegex(RuntimeError, "owned game"):
            recorder.inspect_owned_window(self.identity, 0x99, api, window)

    def test_stop_is_token_marker_only_even_with_recycled_metadata_pid(self):
        self.prepare_run()
        info = recorder.read_info(self.out)
        info.update(status="running", pid=1, ffmpeg_pid=1, supervisor_pid=1)
        recorder.write_json(self.out / "recording.json", info)
        with mock.patch.object(subprocess, "run") as run, mock.patch.object(subprocess, "Popen") as popen:
            recorder.request_stop(self.out, self.token)
            run.assert_not_called()
            popen.assert_not_called()
        self.assertTrue(recorder.stop_requested(self.out, self.token))
        with self.assertRaisesRegex(RuntimeError, "does not match"):
            recorder.request_stop(self.out, "b" * 32)

    def test_fake_supervisor_gracefully_stops_and_logs_exit(self):
        a = self.prepare_run()
        process, popen = FakeProcess(), mock.Mock()
        popen.return_value = process

        def sleep(_duration):
            recorder.request_stop(self.out, self.token)

        with mock.patch.object(recorder, "budget_status", return_value=self.good_budget):
            result = recorder.run(a, inspect=mock.Mock(return_value=self.client), popen=popen, sleep=sleep)
        self.assertEqual(result["status"], "stopped")
        self.assertEqual(result["stop_reason"], "stop_requested")
        self.assertEqual(result["ffmpeg_returncode"], 0)
        self.assertEqual(process.stdin.writes, [b"q\n"])
        self.assertFalse(process.terminated)
        self.assertFalse((self.raw / ".recording-active.json").exists())
        self.assertEqual(popen.call_args.kwargs["stdin"], subprocess.PIPE)
        self.assertEqual(popen.call_args.kwargs["creationflags"], recorder.CREATE_NO_WINDOW)
        self.assertEqual(recorder.read_info(self.out)["owned_game"]["creation_time"], self.identity.creation_time)

    def test_geometry_change_stops_capture(self):
        a = self.prepare_run()
        process = FakeProcess()
        inspect = mock.Mock(side_effect=[self.client, (100, 200, 1200, 800)])
        with mock.patch.object(recorder, "budget_status", return_value=self.good_budget):
            result = recorder.run(a, inspect=inspect, popen=mock.Mock(return_value=process), sleep=mock.Mock())
        self.assertEqual(result["stop_reason"], "client_geometry_changed")
        self.assertEqual(process.stdin.writes, [b"q\n"])

    def test_identity_failure_stops_and_records_error(self):
        a = self.prepare_run()
        process = FakeProcess()
        inspect = mock.Mock(side_effect=[self.client, RuntimeError("PID reused")])
        with mock.patch.object(recorder, "budget_status", return_value=self.good_budget):
            result = recorder.run(a, inspect=inspect, popen=mock.Mock(return_value=process), sleep=mock.Mock())
        self.assertEqual(result["status"], "error")
        self.assertIn("PID reused", result["error"])
        self.assertEqual(process.stdin.writes, [b"q\n"])

    def test_shared_budget_sums_campaigns_and_checks_headroom(self):
        other = self.raw / "other"
        other.mkdir()
        (other / "old.mkv").write_bytes(b"x" * 20)
        (self.out / "new.MKV").write_bytes(b"x" * 30)
        (self.out / "notes.txt").write_bytes(b"x" * 100)
        with mock.patch.object(shutil, "disk_usage", return_value=argparse.Namespace(free=10_000)):
            result = recorder.budget_status(self.raw, recorder.CAP_RESERVE + 50, 100)
            self.assertEqual(result["used_bytes"], 50)
            self.assertEqual(result["reason"], "footage_cap")
            self.assertEqual(recorder.budget_status(self.raw, recorder.DEFAULT_CAP, 10_000)["reason"],
                             "disk_headroom")

    def test_budget_monitor_stops_before_further_footage(self):
        a = self.prepare_run()
        process = FakeProcess()
        exhausted = dict(self.good_budget, reason="footage_cap")
        with mock.patch.object(recorder, "budget_status", side_effect=[self.good_budget, exhausted]):
            result = recorder.run(a, inspect=mock.Mock(return_value=self.client),
                                  popen=mock.Mock(return_value=process), sleep=mock.Mock())
        self.assertEqual(result["stop_reason"], "footage_cap")
        self.assertEqual(process.stdin.writes, [b"q\n"])

    def test_ffmpeg_error_exit_is_logged(self):
        a = self.prepare_run()
        with mock.patch.object(recorder, "budget_status", return_value=self.good_budget):
            result = recorder.run(a, inspect=mock.Mock(return_value=self.client),
                                  popen=mock.Mock(return_value=FakeProcess(returncode=19)), sleep=mock.Mock())
        self.assertEqual(result["status"], "error")
        self.assertEqual(result["ffmpeg_returncode"], 19)
        self.assertIn("19", result["error"])

    def test_shutdown_escalation_uses_actual_popen_only(self):
        process = FakeProcess(timeouts=2)
        self.assertEqual(recorder.finish_process(process), (0, "kill"))
        self.assertEqual(process.stdin.writes, [b"q\n"])
        self.assertTrue(process.terminated)
        self.assertTrue(process.killed)

    def test_start_spawns_hidden_supervisor_with_token_and_logs_metadata(self):
        child = FakeProcess()

        def popen(command, **kwargs):
            self.assertEqual(command[2], "run")
            self.assertEqual(kwargs["creationflags"], recorder.CREATE_NO_WINDOW)
            info = recorder.read_info(self.out)
            self.assertEqual(command[-1], info["token"])
            info["status"] = "running"
            recorder.write_json(self.out / "recording.json", info)
            return child

        with mock.patch.object(recorder, "budget_status", return_value=self.good_budget), mock.patch("builtins.print"):
            result = recorder.start(self.a, inspect=mock.Mock(return_value=self.client), popen=popen, sleep=mock.Mock())
        self.assertEqual(result["status"], "running")
        self.assertEqual(result["crf"], 28)
        with self.assertRaisesRegex(RuntimeError, "Another recording"):
            with mock.patch.object(recorder, "budget_status", return_value=self.good_budget):
                recorder.start(self.a, inspect=mock.Mock(return_value=self.client), popen=mock.Mock())

    def test_preexisting_stop_does_not_launch_capture(self):
        a = self.prepare_run()
        recorder.request_stop(self.out, self.token)
        popen = mock.Mock()
        with mock.patch.object(recorder, "budget_status", return_value=self.good_budget):
            result = recorder.run(a, inspect=mock.Mock(return_value=self.client), popen=popen)
        popen.assert_not_called()
        self.assertEqual(result["stop_reason"], "stop_requested_before_capture")

    def test_output_outside_shared_raw_root_rejected(self):
        with self.assertRaisesRegex(ValueError, "within"):
            recorder.paths(self.raw.parent / "outside", self.raw)

    def test_changed_metadata_output_cannot_capture_elsewhere(self):
        a = self.prepare_run()
        info = recorder.read_info(self.out)
        info["out"] = str(self.raw.parent / "elsewhere")
        recorder.write_json(self.out / "recording.json", info)
        popen = mock.Mock()
        with self.assertRaisesRegex(RuntimeError, "output identity"):
            recorder.run(a, inspect=mock.Mock(return_value=self.client), popen=popen)
        popen.assert_not_called()


if __name__ == "__main__":
    unittest.main()
