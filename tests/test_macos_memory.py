"""Injected Mach tests: never attach to or read a real process."""

import ctypes
import sys
import unittest
from dataclasses import dataclass, replace
from unittest import mock

from hoi4_live.errors import CapabilityUnavailable
from hoi4_live.platform.macos_memory import (
    InvalidMemoryError, MachAPI, MAX_READ_BYTES, MAX_USER_ADDRESS,
    MemoryReadError, OwnershipError, ReadOnlyProcess,
)


@dataclass(frozen=True)
class Identity:
    pid: int = 1234
    creation_time: int = 123456789000001
    executable_path: str = "/fixture/hoi4.app/Contents/MacOS/hoi4"


class FakeAPI:
    def __init__(self):
        self.identity = Identity()
        self.closed = []
        self.opened = []
        self.reads = []
        self.after_read = None
        self.result = None

    def capture_process_identity(self, pid, path):
        return self.identity

    def open_task(self, pid):
        self.opened.append(pid)
        return 17

    def close(self, task):
        self.closed.append(task)

    def read(self, task, address, size):
        self.reads.append((task, address, size))
        if self.after_read:
            self.identity = self.after_read
        return self.result if self.result is not None else b"\x01" * size


class MacMemoryTests(unittest.TestCase):
    def test_exact_read_and_cleanup(self):
        api = FakeAPI()
        with ReadOnlyProcess(api.identity, owned=True, api=api) as reader:
            self.assertEqual(reader.read(0x100000000, 8), b"\x01" * 8)
            self.assertEqual(reader.u64(0x100000000), 0x0101010101010101)
            self.assertEqual(reader.i32(0x100000000), 0x01010101)
            self.assertEqual(reader.i64(0x100000000), 0x0101010101010101)
        reader.close()
        self.assertEqual(api.closed, [17])
        self.assertEqual(api.opened, [1234])

    def test_session_ownership_must_be_explicit(self):
        api = FakeAPI()
        for owned in (False, 1, None):
            with self.subTest(owned=owned), self.assertRaises(OwnershipError):
                ReadOnlyProcess(api.identity, owned=owned, api=api)
        self.assertEqual(api.opened, [])

    def test_invalid_identity_rejected_before_task_request(self):
        for identity in (None, replace(Identity(), pid=True), replace(Identity(), creation_time=0),
                         replace(Identity(), executable_path="hoi4"),
                         replace(Identity(), executable_path="/fixture/other")):
            api = FakeAPI()
            with self.subTest(identity=identity), self.assertRaises(OwnershipError):
                ReadOnlyProcess(identity, owned=True, api=api)
            self.assertEqual(api.opened, [])

    def test_identity_mismatch_before_open(self):
        api = FakeAPI()
        api.identity = replace(api.identity, creation_time=api.identity.creation_time + 1)
        with self.assertRaises(OwnershipError):
            ReadOnlyProcess(Identity(), owned=True, api=api)
        self.assertEqual(api.opened, [])

    def test_identity_checked_after_open(self):
        api = FakeAPI()
        original_open = api.open_task
        def opened(pid):
            api.identity = replace(api.identity, pid=999)
            return original_open(pid)
        api.open_task = opened
        with self.assertRaises(OwnershipError):
            ReadOnlyProcess(Identity(), owned=True, api=api)
        self.assertEqual(api.closed, [17])

    def test_pid_reuse_and_exec_change_rejected_each_read(self):
        for change in ({"creation_time": 123456789000002}, {"pid": 1235},
                       {"executable_path": "/other/hoi4"},
                       {"executable_path": "/FIXTURE/hoi4.app/Contents/MacOS/hoi4"}):
            api = FakeAPI()
            reader = ReadOnlyProcess(api.identity, owned=True, api=api)
            api.identity = replace(api.identity, **change)
            with self.subTest(change=change), self.assertRaises(OwnershipError):
                reader.read(0x100000000, 8)
            self.assertEqual(api.closed, [17])
            self.assertEqual(api.reads, [])

    def test_identity_change_during_read_discards_bytes(self):
        api = FakeAPI()
        reader = ReadOnlyProcess(api.identity, owned=True, api=api)
        api.after_read = replace(api.identity, creation_time=api.identity.creation_time + 1)
        with self.assertRaises(OwnershipError):
            reader.read(0x100000000, 8)
        self.assertTrue(reader.closed)

    def test_bounds_checked_before_read(self):
        api = FakeAPI()
        with ReadOnlyProcess(api.identity, owned=True, api=api) as reader:
            for address, size in ((0, 8), (True, 8), (0x10000, True), (0x10000, 0),
                                  (0x10000, MAX_READ_BYTES + 1), (MAX_USER_ADDRESS, 2),
                                  (MAX_USER_ADDRESS + 1, 1)):
                with self.subTest(address=address, size=size), self.assertRaises(InvalidMemoryError):
                    reader.read(address, size)
            self.assertEqual(len(reader.read(0x10000, MAX_READ_BYTES)), MAX_READ_BYTES)
        self.assertEqual(len(api.reads), 1)

    def test_custom_limit_cannot_expand_bound(self):
        api = FakeAPI()
        for limit in (MAX_READ_BYTES + 1, 0, True):
            with self.assertRaises(ValueError):
                ReadOnlyProcess(api.identity, owned=True, api=api, max_read_bytes=limit)
        with ReadOnlyProcess(api.identity, owned=True, api=api, max_read_bytes=4) as reader:
            with self.assertRaises(InvalidMemoryError):
                reader.read(0x10000, 8)

    def test_short_and_non_bytes_reads_rejected(self):
        for data in (b"", b"a", bytearray(b"12345678")):
            api = FakeAPI()
            api.result = data
            with ReadOnlyProcess(api.identity, owned=True, api=api) as reader:
                with self.assertRaises(MemoryReadError):
                    reader.read(0x10000, 8)

    def test_closed_reader_cannot_read(self):
        api = FakeAPI()
        reader = ReadOnlyProcess(api.identity, owned=True, api=api)
        reader.close()
        with self.assertRaises(OwnershipError):
            reader.read(0x10000, 8)

    def test_native_permission_denial_is_actionable_capability(self):
        api = MachAPI.__new__(MachAPI)
        api._self = 1
        api._lib = mock.Mock()
        api._lib.task_for_pid.return_value = 5
        with self.assertRaises(CapabilityUnavailable) as error:
            api.open_task(123)
        self.assertIn("task_for_pid", str(error.exception))
        self.assertIn("telemetry", str(error.exception.details))
        api._lib.mach_vm_read_overwrite.assert_not_called()

    def test_native_read_uses_explicit_64_bit_address_and_rejects_short_read(self):
        api = MachAPI.__new__(MachAPI)
        api._lib = mock.Mock()
        def read(task, address, size, destination, actual):
            self.assertEqual(address, 0x123456789)
            ctypes.memmove(destination, b"12345678", size)
            ctypes.cast(actual, ctypes.POINTER(ctypes.c_uint64)).contents.value = size
            return 0
        api._lib.mach_vm_read_overwrite.side_effect = read
        self.assertEqual(api.read(7, 0x123456789, 8), b"12345678")
        api._lib.mach_vm_read_overwrite.side_effect = None
        api._lib.mach_vm_read_overwrite.return_value = 0
        with self.assertRaises(MemoryReadError):
            api.read(7, 0x123456789, 8)

    def test_native_api_has_no_mutation_or_control_methods(self):
        for name in ("write", "suspend", "resume", "inject", "command", "terminate"):
            self.assertFalse(hasattr(MachAPI, name))

    @unittest.skipIf(sys.platform == "darwin", "non-Mac capability rejection")
    def test_native_api_rejects_other_platforms(self):
        with self.assertRaises(CapabilityUnavailable):
            MachAPI()


if __name__ == "__main__":
    unittest.main()
