"""Bounded reads of an explicitly owned Linux process through /proc/PID/mem.

The descriptor is opened O_RDONLY and stays bound to the original kernel memory
object; PID reuse cannot redirect an existing descriptor to a new process. Exact
boot/start/executable witnesses are checked before and after every read. Kernel
access denial is a capability error, with observational telemetry/UI as fallback.
No ptrace attachment, writes, injected calls or game layouts are implemented.
"""

import errno
import os
import posixpath
import struct
import sys
import threading

from ..errors import CapabilityUnavailable
from .linux_process import LinuxProcessIdentity, ProcessAPI, capture_process_identity, creation_token


MAX_READ_BYTES = 1 << 20
MAX_BATCH_RANGES = 1024
# Conservative 47-bit user address range. Wider mappings need an independently
# verified architecture reader; never silently extend a game's address boundary.
MAX_USER_ADDRESS = 0x00007FFFFFFFFFFF


class MemoryReadError(RuntimeError):
    """An exact, bounded read could not be completed."""


class OwnershipError(MemoryReadError):
    """The explicitly owned Linux process identity is no longer valid."""


class InvalidMemoryError(MemoryReadError):
    """The requested address or size is outside the permitted boundary."""


def checked_range(address, size, max_size=MAX_READ_BYTES):
    if type(address) is not int or not 0x10000 <= address <= MAX_USER_ADDRESS:
        raise InvalidMemoryError("invalid user-space address")
    if type(size) is not int or not 0 < size <= max_size:
        raise InvalidMemoryError("read size exceeds the allowed bound")
    if address > MAX_USER_ADDRESS - size + 1:
        raise InvalidMemoryError("read range overflows user space")


def _identity_key(identity):
    if not isinstance(identity, LinuxProcessIdentity):
        raise OwnershipError("an exact Linux boot/start/executable identity is required")
    if type(identity.pid) is not int or not 0 < identity.pid <= 0x7FFFFFFF:
        raise OwnershipError("an explicitly launched PID is required")
    path = identity.executable_path
    if (not isinstance(path, str) or not posixpath.isabs(path) or "\0" in path
            or path.endswith(" (deleted)") or posixpath.basename(path) != "hoi4"):
        raise OwnershipError("the owned executable must be an absolute native hoi4 binary")
    try:
        token = creation_token(identity.boot_id, identity.start_ticks)
    except OSError as exc:
        raise OwnershipError("exact Linux boot and start witnesses are required") from exc
    if (type(identity.creation_time) is not int or identity.creation_time != token
            or type(identity.executable_device) is not int or identity.executable_device < 0
            or type(identity.executable_inode) is not int or identity.executable_inode <= 0):
        raise OwnershipError("invalid Linux executable or creation witness")
    return (identity.pid, token, posixpath.normpath(path), identity.boot_id,
            identity.start_ticks, identity.executable_device, identity.executable_inode)


class ReadOnlyProcess:
    def __init__(self, identity, *, owned=False, api=None, max_read_bytes=MAX_READ_BYTES):
        self._expected = _identity_key(identity)
        if owned is not True:
            raise OwnershipError("the session must authorize its own launched process")
        if type(max_read_bytes) is not int or not 0 < max_read_bytes <= MAX_READ_BYTES:
            raise ValueError("invalid maximum read size")
        self.identity, self.max_read_bytes = identity, max_read_bytes
        self.api = api if api is not None else ProcMemoryAPI()
        self._fd = None
        self._mutex = threading.RLock()
        self._check_identity()
        try:
            self._fd = self.api.open_memory(identity.pid)
            if type(self._fd) is not int or self._fd < 0:
                self._fd = None
                raise MemoryReadError("no read-only process descriptor was returned")
            self.validate_identity()
        except BaseException:
            self.close()
            raise

    @property
    def closed(self):
        return self._fd is None

    def _check_identity(self):
        try:
            actual = self.api.capture_process_identity(self._expected[0], self._expected[2])
            if _identity_key(actual) != self._expected:
                raise OwnershipError("owned process identity changed")
        except OSError as exc:
            raise OwnershipError("cannot verify the owned process identity") from exc

    def validate_identity(self):
        with self._mutex:
            if self.closed:
                raise OwnershipError("process reader is closed")
            try:
                self._check_identity()
            except BaseException:
                self.close()
                raise

    def read(self, address, size):
        checked_range(address, size, self.max_read_bytes)
        with self._mutex:
            self.validate_identity()
            try:
                data = self.api.read(self._fd, address, size)
            except OSError as exc:
                raise MemoryReadError("process memory read failed") from exc
            # Do not return bytes from an exited, replaced or recycled target.
            self.validate_identity()
            if not isinstance(data, bytes) or len(data) != size:
                raise MemoryReadError("partial process memory read rejected")
            return data

    def read_many(self, ranges):
        """Amortize ownership queries across an exact, bounded observation batch.

        Every range and the aggregate byte count are validated before any
        process query or read. Results remain private until all exact reads and
        the final identity check succeed. This is not an atomic game snapshot:
        layout consumers must still check their own freshness/torn-state witnesses.
        """
        if not isinstance(ranges, (list, tuple)) or not 0 < len(ranges) <= MAX_BATCH_RANGES:
            raise InvalidMemoryError("a bounded, nonempty list of read ranges is required")
        requests, total = [], 0
        for entry in ranges:
            if not isinstance(entry, (list, tuple)) or len(entry) != 2:
                raise InvalidMemoryError("each read range must contain an address and size")
            address, size = entry
            checked_range(address, size, self.max_read_bytes)
            total += size
            if total > self.max_read_bytes:
                raise InvalidMemoryError("batch read size exceeds the allowed aggregate bound")
            requests.append((address, size))
        with self._mutex:
            self.validate_identity()
            results = []
            for address, size in requests:
                try:
                    data = self.api.read(self._fd, address, size)
                except OSError as exc:
                    raise MemoryReadError("process memory batch read failed") from exc
                if not isinstance(data, bytes) or len(data) != size:
                    raise MemoryReadError("partial process memory batch read rejected")
                results.append(data)
            self.validate_identity()
            return results

    def u64(self, address):
        return struct.unpack("<Q", self.read(address, 8))[0]

    def i64(self, address):
        return struct.unpack("<q", self.read(address, 8))[0]

    def i32(self, address):
        return struct.unpack("<i", self.read(address, 4))[0]

    def close(self):
        with self._mutex:
            fd, self._fd = self._fd, None
            if fd is not None:
                self.api.close(fd)

    def __enter__(self):
        self.validate_identity()
        return self

    def __exit__(self, *_args):
        self.close()


class ProcMemoryAPI:
    """Native reads use pread, so concurrent callers cannot move an fd cursor."""

    def __init__(self, process_api=None):
        if process_api is None and not sys.platform.startswith("linux"):
            raise CapabilityUnavailable("process memory", "linux", "requires Linux procfs",
                                        "Use fresh own-country telemetry and UI observations.")
        self.process_api = process_api if process_api is not None else ProcessAPI()

    def capture_process_identity(self, pid, expected_executable_path):
        return capture_process_identity(pid, expected_executable_path, api=self.process_api)

    def open_memory(self, pid):
        flags = os.O_RDONLY | getattr(os, "O_CLOEXEC", 0)
        try:
            return os.open(self.process_api.process_path(pid, "mem"), flags)
        except OSError as exc:
            if exc.errno in (errno.EACCES, errno.EPERM):
                raise CapabilityUnavailable(
                    "process memory", "linux", "kernel ptrace policy or procfs permissions denied read-only access",
                    "Continue with observational telemetry and UI; memory access is a separate capability.") from exc
            raise

    def read(self, fd, address, size):
        checked_range(address, size)
        data = os.pread(fd, size, address)
        if len(data) != size:
            raise MemoryReadError("partial process memory read rejected")
        return data

    def close(self, fd):
        os.close(fd)
