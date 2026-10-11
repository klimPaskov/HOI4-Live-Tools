"""Procfs fixtures and injected ownership races; no game is queried."""

from dataclasses import replace
import os
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import Mock

from hoi4_live.errors import CapabilityUnavailable
from hoi4_live.platform.linux_process import (
    ExecutableWitness, LinuxProcessIdentity, ProcessAPI, ProcessStat,
    capture_process_identity, creation_token, parse_stat,
)


BOOT_ID = "12345678-1234-4567-89ab-123456789abc"
SECOND_BOOT_ID = "12345678-1234-4567-89ab-123456789abd"


def stat_line(pid=42, name="hoi4", state="R", ticks=1234):
    fields = ["0"] * 50
    fields[0], fields[19] = state, str(ticks)
    return "%s (%s) %s\n" % (pid, name, " ".join(fields))


def fake_api():
    return Mock(boot_id=Mock(return_value=BOOT_ID),
                read_stat=Mock(return_value=ProcessStat(42, "hoi4", "R", 1234)),
                image=Mock(return_value=ExecutableWitness("/fixture/hoi4", 8, 99)))


class LinuxStatTests(unittest.TestCase):
    def test_stat_comm_with_spaces_parentheses_and_newline_is_not_a_field(self):
        name = "odd ) (name\n)"
        result = parse_stat(stat_line(name=name), 42)
        self.assertEqual(result, ProcessStat(42, name, "R", 1234))

    def test_incomplete_wrong_pid_invalid_state_and_ticks_rejected(self):
        for text in ("", "42 (hoi4) R", "42 (hoi4 R " + "0 " * 50,
                     stat_line(pid=43), stat_line(state="?"), stat_line(ticks=-1),
                     stat_line(ticks=1 << 64), stat_line() + "\0", "x" * 8193):
            with self.subTest(text=text[:50]), self.assertRaises(OSError):
                parse_stat(text, 42)
        for pid in (True, 0, -42, "42", 1 << 31):
            with self.subTest(pid=pid), self.assertRaises(OSError):
                parse_stat(stat_line(), pid)

    def test_boot_token_has_no_rounding_or_cross_boot_collision(self):
        token = creation_token(BOOT_ID, 1234)
        self.assertEqual(token & ((1 << 64) - 1), 1234)
        self.assertNotEqual(token, creation_token(BOOT_ID, 1235))
        self.assertNotEqual(token, creation_token(SECOND_BOOT_ID, 1234))
        for boot, ticks in (("", 1), (BOOT_ID.upper(), 1), ("0" * 32, 1),
                            (BOOT_ID, True), (BOOT_ID, -1), (BOOT_ID, 1 << 64)):
            with self.subTest(boot=boot, ticks=ticks), self.assertRaises(OSError):
                creation_token(boot, ticks)


class LinuxIdentityTests(unittest.TestCase):
    def test_exact_boot_start_path_and_inode_witness(self):
        api = fake_api()
        result = capture_process_identity(42, "/fixture/hoi4", api=api)
        self.assertIsInstance(result, LinuxProcessIdentity)
        self.assertEqual(result.creation_time, creation_token(BOOT_ID, 1234))
        self.assertEqual((result.pid, result.start_ticks, result.boot_id,
                          result.executable_path, result.executable_device, result.executable_inode),
                         (42, 1234, BOOT_ID, "/fixture/hoi4", 8, 99))
        self.assertEqual(api.read_stat.call_count, 3)
        self.assertEqual(api.image.call_count, 2)

    def test_pid_reuse_and_exit_at_each_stat_witness_rejected(self):
        for position in range(3):
            for change in ({"start_ticks": 1235}, {"pid": 43}, {"state": "Z"}, {"state": "X"}):
                api = fake_api()
                records = [api.read_stat.return_value] * 3
                records[position] = replace(records[position], **change)
                api.read_stat.side_effect = records
                with self.subTest(position=position, change=change), self.assertRaises(OSError):
                    capture_process_identity(42, api=api)

    def test_exec_image_inode_and_boot_change_rejected(self):
        for change in ({"path": "/other/hoi4"}, {"inode": 100}, {"device": 9}):
            api = fake_api()
            image = api.image.return_value
            api.image.side_effect = [image, replace(image, **change)]
            with self.subTest(change=change), self.assertRaises(OSError):
                capture_process_identity(42, api=api)
        api = fake_api()
        api.boot_id.side_effect = [BOOT_ID, SECOND_BOOT_ID]
        with self.assertRaises(OSError):
            capture_process_identity(42, api=api)

    def test_invalid_executable_witness_or_expected_path_rejected(self):
        for path in ("hoi4", "/other/hoi4", "/fixture/hoi4\0"):
            with self.subTest(path=path), self.assertRaises(OSError):
                capture_process_identity(42, path, api=fake_api())
        for witness in (ExecutableWitness("hoi4", 8, 99), ExecutableWitness("/fixture/hoi4", 8, 0),
                        ExecutableWitness("/fixture/hoi4 (deleted)", 8, 99),
                        ExecutableWitness("/fixture/hoi4", True, 99)):
            api = fake_api()
            api.image.return_value = witness
            with self.subTest(witness=witness), self.assertRaises(OSError):
                capture_process_identity(42, api=api)

    @unittest.skipIf(os.name == "nt", "procfs symlink fixtures require POSIX filesystem semantics")
    def test_procfs_files_are_bounded_and_exe_is_resolved(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            boot = root / "sys/kernel/random/boot_id"
            boot.parent.mkdir(parents=True)
            boot.write_text(BOOT_ID + "\n")
            process = root / "42"
            process.mkdir()
            (process / "stat").write_text(stat_line())
            (process / "comm").write_text("hoi4\n")
            binary = root / "hoi4"
            binary.write_bytes(b"fixture executable")
            (process / "exe").symlink_to(binary)
            (root / "nonnumeric").mkdir()
            api = ProcessAPI(proc_root=str(root))
            result = capture_process_identity(42, str(binary), api=api)
            self.assertEqual(result.executable_inode, binary.stat().st_ino)
            self.assertEqual(result.executable_path, str(binary))
            self.assertEqual(api.process_name(42), "hoi4")
            self.assertEqual(api.list_pids(), [42])
            (process / "stat").write_text("x" * 8193)
            with self.assertRaises(OSError):
                api.read_stat(42)
            boot.write_text("x" * 65)
            with self.assertRaises(OSError):
                api.boot_id()

    def test_native_api_rejects_non_linux_without_loading_native_code(self):
        if sys.platform.startswith("linux"):
            self.skipTest("non-Linux capability rejection")
        with self.assertRaises(CapabilityUnavailable):
            ProcessAPI()

    @unittest.skipUnless(sys.platform.startswith("linux"), "native /proc metadata requires Linux")
    def test_native_self_identity_is_stable_without_ptrace(self):
        first = capture_process_identity(os.getpid())
        self.assertEqual(first, capture_process_identity(os.getpid(), first.executable_path))
        self.assertEqual(first.executable_inode, os.stat("/proc/self/exe").st_ino)


if __name__ == "__main__":
    unittest.main()
