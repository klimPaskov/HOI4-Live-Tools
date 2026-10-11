"""Injected exact owned reads plus Linux-only reads of a helper's own buffer."""

import ctypes
from dataclasses import replace
import errno
import os
import sys
import unittest
from unittest.mock import Mock, patch

from hoi4_live.errors import CapabilityUnavailable
from hoi4_live.platform.identity import ProcessIdentity
from hoi4_live.platform.linux_memory import (
    InvalidMemoryError, MAX_BATCH_RANGES, MAX_READ_BYTES, MAX_USER_ADDRESS, MemoryReadError,
    OwnershipError, ProcMemoryAPI, ReadOnlyProcess,
)
from hoi4_live.platform.linux_process import LinuxProcessIdentity, creation_token


BOOT_ID = "12345678-1234-4567-89ab-123456789abc"
SECOND_BOOT_ID = "12345678-1234-4567-89ab-123456789abd"
IDENTITY = LinuxProcessIdentity(42, creation_token(BOOT_ID, 1234), "/fixture/hoi4", BOOT_ID, 1234, 8, 99)


class FakeAPI:
    def __init__(self):
        self.identity = IDENTITY
        self.opened, self.closed, self.reads = [], [], []
        self.fd, self.result, self.after_read, self.after_open = 17, None, None, None

    def capture_process_identity(self, pid, path):
        return self.identity

    def open_memory(self, pid):
        self.opened.append(pid)
        if self.after_open is not None:
            self.identity = self.after_open
        return self.fd

    def read(self, fd, address, size):
        self.reads.append((fd, address, size))
        if self.after_read is not None:
            self.identity = self.after_read
        return self.result if self.result is not None else b"\x01" * size

    def close(self, fd):
        self.closed.append(fd)


class LinuxMemoryTests(unittest.TestCase):
    def test_exact_read_and_scalar_decoders_and_idempotent_close(self):
        api = FakeAPI()
        with ReadOnlyProcess(IDENTITY, owned=True, api=api) as reader:
            self.assertEqual(reader.read(0x100000000, 8), b"\x01" * 8)
            self.assertEqual(reader.u64(0x100000000), 0x0101010101010101)
            self.assertEqual(reader.i64(0x100000000), 0x0101010101010101)
            self.assertEqual(reader.i32(0x100000000), 0x01010101)
        reader.close()
        self.assertEqual(api.opened, [42])
        self.assertEqual(api.closed, [17])

    def test_fd_zero_is_valid_and_gets_closed(self):
        api = FakeAPI()
        api.fd = 0
        with ReadOnlyProcess(IDENTITY, owned=True, api=api) as reader:
            self.assertFalse(reader.closed)
            self.assertEqual(reader.read(0x10000, 1), b"\x01")
        self.assertEqual(api.closed, [0])

    def test_ownership_and_all_native_identity_fields_required_before_open(self):
        invalid = [None, ProcessIdentity(42, 1, "/fixture/hoi4"), replace(IDENTITY, pid=True),
                   replace(IDENTITY, creation_time=1234), replace(IDENTITY, executable_path="hoi4"),
                   replace(IDENTITY, executable_path="/fixture/other"), replace(IDENTITY, boot_id=""),
                   replace(IDENTITY, start_ticks=-1), replace(IDENTITY, executable_inode=0)]
        api = FakeAPI()
        for identity in invalid:
            with self.subTest(identity=identity), self.assertRaises(OwnershipError):
                ReadOnlyProcess(identity, owned=True, api=api)
        for owned in (False, 1, None):
            with self.subTest(owned=owned), self.assertRaises(OwnershipError):
                ReadOnlyProcess(IDENTITY, owned=owned, api=api)
        self.assertEqual(api.opened, [])

    def test_reuse_and_exec_or_reboot_changes_never_disclose_bytes(self):
        changes = [replace(IDENTITY, pid=43), replace(IDENTITY, executable_path="/other/hoi4"),
                   replace(IDENTITY, executable_inode=100), replace(IDENTITY, executable_device=9),
                   replace(IDENTITY, start_ticks=1235, creation_time=creation_token(BOOT_ID, 1235)),
                   replace(IDENTITY, boot_id=SECOND_BOOT_ID, creation_time=creation_token(SECOND_BOOT_ID, 1234))]
        for after_read in (False, True):
            for changed in changes:
                api = FakeAPI()
                reader = ReadOnlyProcess(IDENTITY, owned=True, api=api)
                if after_read:
                    api.after_read = changed
                else:
                    api.identity = changed
                with self.subTest(after_read=after_read, changed=changed), self.assertRaises(OwnershipError):
                    reader.read(0x10000, 8)
                self.assertTrue(reader.closed)
                self.assertEqual(api.closed, [17])
                self.assertEqual(len(api.reads), int(after_read))

    def test_race_before_open_does_not_open_and_race_after_open_closes_fd(self):
        api = FakeAPI()
        api.identity = replace(IDENTITY, executable_inode=100)
        with self.assertRaises(OwnershipError):
            ReadOnlyProcess(IDENTITY, owned=True, api=api)
        self.assertEqual(api.opened, [])
        api = FakeAPI()
        api.after_open = replace(IDENTITY, executable_inode=100)
        with self.assertRaises(OwnershipError):
            ReadOnlyProcess(IDENTITY, owned=True, api=api)
        self.assertEqual(api.closed, [17])

    def test_bounds_and_custom_limits_are_checked_before_native_reads(self):
        api = FakeAPI()
        with ReadOnlyProcess(IDENTITY, owned=True, api=api) as reader:
            for address, size in ((0, 8), (True, 8), (0x10000, True), (0x10000, 0),
                                  (0x10000, MAX_READ_BYTES + 1), (MAX_USER_ADDRESS, 2),
                                  (MAX_USER_ADDRESS + 1, 1)):
                with self.subTest(address=address, size=size), self.assertRaises(InvalidMemoryError):
                    reader.read(address, size)
            self.assertEqual(api.reads, [])
            self.assertEqual(len(reader.read(0x10000, MAX_READ_BYTES)), MAX_READ_BYTES)
        for limit in (True, 0, MAX_READ_BYTES + 1):
            with self.assertRaises(ValueError):
                ReadOnlyProcess(IDENTITY, owned=True, api=api, max_read_bytes=limit)
        with ReadOnlyProcess(IDENTITY, owned=True, api=api, max_read_bytes=4) as reader:
            with self.assertRaises(InvalidMemoryError):
                reader.read(0x10000, 8)

    def test_partial_invalid_and_closed_reads_rejected(self):
        for result in (b"", b"short", bytearray(b"12345678")):
            api = FakeAPI()
            api.result = result
            with ReadOnlyProcess(IDENTITY, owned=True, api=api) as reader:
                with self.assertRaises(MemoryReadError):
                    reader.read(0x10000, 8)
            with self.assertRaises(OwnershipError):
                reader.read(0x10000, 8)

    def test_batch_exact_reads_amortize_identity_checks(self):
        api = FakeAPI()
        with ReadOnlyProcess(IDENTITY, owned=True, api=api) as reader:
            api.capture_process_identity = Mock(return_value=IDENTITY)
            self.assertEqual(reader.read_many([(0x10000, 8), (0x20000, 4)]),
                             [b"\x01" * 8, b"\x01" * 4])
            self.assertEqual(api.reads, [(17, 0x10000, 8), (17, 0x20000, 4)])
            self.assertEqual(api.capture_process_identity.call_count, 2)

    def test_batch_all_bounds_checked_before_any_identity_query_or_read(self):
        api = FakeAPI()
        with ReadOnlyProcess(IDENTITY, owned=True, api=api) as reader:
            api.capture_process_identity = Mock(return_value=IDENTITY)
            invalid = [[], iter([(0x10000, 8)]), [(0x10000, 8)] * (MAX_BATCH_RANGES + 1),
                       [(0x10000, 8), (0, 8)], [(0x10000, 8), (0x10000, True)],
                       [(0x10000, 8), (0x10000, 8, 1)],
                       [(0x10000, MAX_READ_BYTES), (0x10000, 1)]]
            for requests in invalid:
                with self.subTest(requests=type(requests)), self.assertRaises(InvalidMemoryError):
                    reader.read_many(requests)
            api.capture_process_identity.assert_not_called()
            self.assertEqual(api.reads, [])
        with ReadOnlyProcess(IDENTITY, owned=True, api=api, max_read_bytes=4) as reader:
            with self.assertRaises(InvalidMemoryError):
                reader.read_many([(0x10000, 3), (0x20000, 2)])

    def test_batch_reuse_or_exec_mid_read_discards_all_results_and_closes(self):
        for changed in (replace(IDENTITY, executable_inode=100),
                        replace(IDENTITY, start_ticks=1235, creation_time=creation_token(BOOT_ID, 1235))):
            api = FakeAPI()
            with self.subTest(changed=changed):
                reader = ReadOnlyProcess(IDENTITY, owned=True, api=api)
                api.after_read = changed
                with self.assertRaises(OwnershipError):
                    reader.read_many([(0x10000, 8), (0x20000, 4)])
                self.assertEqual(len(api.reads), 2)
                self.assertTrue(reader.closed)
                self.assertEqual(api.closed, [17])

    def test_batch_short_or_failed_later_read_does_not_return_earlier_bytes(self):
        for failure in (b"", b"short", bytearray(b"12345678"), OSError("unmapped")):
            api = FakeAPI()
            api.read = Mock(side_effect=[b"12345678", failure])
            with self.subTest(failure=failure), ReadOnlyProcess(IDENTITY, owned=True, api=api) as reader:
                with self.assertRaises(MemoryReadError):
                    reader.read_many([(0x10000, 8), (0x20000, 8)])

    def test_batch_rejects_closed_reader(self):
        api = FakeAPI()
        reader = ReadOnlyProcess(IDENTITY, owned=True, api=api)
        reader.close()
        with self.assertRaises(OwnershipError):
            reader.read_many([(0x10000, 8)])

    def test_open_denial_is_typed_capability_and_uses_read_only_flags(self):
        process_api = Mock(process_path=Mock(return_value="/proc/42/mem"))
        api = ProcMemoryAPI(process_api=process_api)
        for error in (errno.EPERM, errno.EACCES):
            with patch("hoi4_live.platform.linux_memory.os.open", side_effect=OSError(error, "denied")) as opened:
                with self.assertRaises(CapabilityUnavailable) as caught:
                    api.open_memory(42)
                self.assertEqual(caught.exception.details["capability"], "process memory")
                self.assertIn("telemetry", caught.exception.details["remedy"])
                self.assertEqual(opened.call_args.args[1] & (os.O_WRONLY | os.O_RDWR), 0)

    def test_native_pread_has_explicit_offset_and_rejects_partial(self):
        api = ProcMemoryAPI(process_api=Mock())
        with patch("hoi4_live.platform.linux_memory.os.pread", return_value=b"12345678", create=True) as read:
            self.assertEqual(api.read(17, 0x123456789, 8), b"12345678")
            read.assert_called_once_with(17, 8, 0x123456789)
            read.return_value = b"short"
            with self.assertRaises(MemoryReadError):
                api.read(17, 0x123456789, 8)

    def test_native_api_has_no_mutation_attachment_or_command_methods(self):
        for name in ("write", "suspend", "resume", "inject", "command", "ptrace", "terminate"):
            self.assertFalse(hasattr(ProcMemoryAPI, name))

    @unittest.skipUnless(sys.platform.startswith("linux"), "native read-only procfs requires Linux")
    def test_native_pread_of_own_generated_buffer_without_ptrace(self):
        # A generated buffer in this test helper, never another process or game.
        # A nonfixed hint keeps ARM helpers with high malloc addresses inside
        # the reader's conservative range without replacing an existing mapping.
        data = b"hoi4-live-linux-read-only"
        libc = ctypes.CDLL(None, use_errno=True)
        libc.mmap.argtypes = [ctypes.c_void_p, ctypes.c_size_t, ctypes.c_int,
                              ctypes.c_int, ctypes.c_int, ctypes.c_long]
        libc.mmap.restype = ctypes.c_void_p
        libc.munmap.argtypes = [ctypes.c_void_p, ctypes.c_size_t]
        libc.munmap.restype = ctypes.c_int
        address = libc.mmap(0x100000000, len(data), 3, 0x22, -1, 0)
        if address is None or address == ctypes.c_void_p(-1).value:
            raise OSError(ctypes.get_errno(), "generated-buffer mmap failed")
        try:
            ctypes.memmove(address, data, len(data))
            api = ProcMemoryAPI()
            fd = api.open_memory(os.getpid())
            try:
                self.assertEqual(api.read(fd, address, len(data)), data)
            finally:
                api.close(fd)
        finally:
            self.assertEqual(libc.munmap(address, len(data)), 0)


if __name__ == "__main__":
    unittest.main()
