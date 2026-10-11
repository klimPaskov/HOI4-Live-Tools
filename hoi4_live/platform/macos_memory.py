"""Bounded read-only Mach access to an explicitly owned process.

Import is inert on every OS. No memory layout, command adapter, writes, suspension
or injection is provided. A denied task port leaves telemetry/UI usable. An arm64
helper can read an x86_64 target: Mach addresses are explicit 64-bit integers;
decoding must use the independently verified target image architecture.
"""

import ctypes
import os
import struct
import sys

from ..errors import CapabilityUnavailable


MAX_READ_BYTES = 1 << 20
MAX_USER_ADDRESS = 0x00007FFFFFFFFFFF


class MemoryReadError(RuntimeError):
    """An exact, bounded read could not be completed."""


class OwnershipError(MemoryReadError):
    """The caller's explicitly owned process identity is no longer valid."""


class InvalidMemoryError(MemoryReadError):
    """The address or size is outside the permitted read boundary."""


def checked_range(address, size, max_size=MAX_READ_BYTES):
    if type(address) is not int or not 0x10000 <= address <= MAX_USER_ADDRESS:
        raise InvalidMemoryError("invalid user-space address")
    if type(size) is not int or not 0 < size <= max_size:
        raise InvalidMemoryError("read size exceeds the allowed bound")
    if address > MAX_USER_ADDRESS - size + 1:
        raise InvalidMemoryError("read range overflows user space")


def _identity_key(identity):
    pid = getattr(identity, "pid", None)
    created = getattr(identity, "creation_time", None)
    path = getattr(identity, "executable_path", None)
    if type(pid) is not int or pid <= 0:
        raise OwnershipError("an explicit launched PID is required")
    if type(created) is not int or created <= 0:
        raise OwnershipError("exact process creation time is required")
    if not isinstance(path, str) or not os.path.isabs(path) or "\0" in path:
        raise OwnershipError("absolute launched executable path is required")
    # Do not case-fold: macOS can use a case-sensitive volume.
    path = os.path.normpath(path)
    if os.path.basename(path) != "hoi4":
        raise OwnershipError("the owned executable must be hoi4")
    return pid, created, path


class ReadOnlyProcess:
    def __init__(self, identity, *, owned=False, api=None, max_read_bytes=MAX_READ_BYTES):
        self._expected = _identity_key(identity)
        if owned is not True:
            raise OwnershipError("the session must authorize its own launched process")
        if type(max_read_bytes) is not int or not 0 < max_read_bytes <= MAX_READ_BYTES:
            raise ValueError("invalid maximum read size")
        self.identity = identity
        self.max_read_bytes = max_read_bytes
        self.api = api if api is not None else MachAPI()
        self._task = None
        self._check_identity()
        try:
            self._task = self.api.open_task(self._expected[0])
            if not self._task:
                raise CapabilityUnavailable("process memory", "macos", "no task port was returned",
                                            remedy="Continue with observational telemetry and UI.")
            self.validate_identity()
        except BaseException:
            self.close()
            raise

    @property
    def closed(self):
        return self._task is None

    def _check_identity(self):
        try:
            actual = self.api.capture_process_identity(self._expected[0], self._expected[2])
            if _identity_key(actual) != self._expected:
                raise OwnershipError("owned process identity changed")
        except OSError as exc:
            raise OwnershipError("cannot verify the owned process identity") from exc

    def validate_identity(self):
        if self.closed:
            raise OwnershipError("process reader is closed")
        try:
            self._check_identity()
        except BaseException:
            self.close()
            raise

    def read(self, address, size):
        checked_range(address, size, self.max_read_bytes)
        self.validate_identity()
        try:
            data = self.api.read(self._task, address, size)
        except OSError as exc:
            raise MemoryReadError("process memory read failed") from exc
        # Reject PID reuse/exit during a read before disclosing the result.
        self.validate_identity()
        if not isinstance(data, bytes) or len(data) != size:
            raise MemoryReadError("partial process memory read rejected")
        return data

    def u64(self, address):
        return struct.unpack("<Q", self.read(address, 8))[0]

    def i64(self, address):
        return struct.unpack("<q", self.read(address, 8))[0]

    def i32(self, address):
        return struct.unpack("<i", self.read(address, 4))[0]

    def close(self):
        task, self._task = self._task, None
        if task:
            self.api.close(task)

    def __enter__(self):
        self.validate_identity()
        return self

    def __exit__(self, *_args):
        self.close()


class MachAPI:
    """Minimal lazy Mach binding. Its public surface only queries and reads.

    Mach VM signatures follow Apple's xnu osfmk/mach/mach_vm.defs. The acquired
    port is never exposed as a campaign command adapter.
    """

    def __init__(self):
        if sys.platform != "darwin" or ctypes.sizeof(ctypes.c_void_p) != 8:
            raise CapabilityUnavailable("process memory", "macos", "a 64-bit macOS helper is required",
                                        remedy="Continue with observational telemetry and UI.")
        self._lib = ctypes.CDLL("/usr/lib/libSystem.B.dylib", use_errno=True)
        self._self = ctypes.c_uint32.in_dll(self._lib, "mach_task_self_").value
        signatures = {
            "task_for_pid": ([ctypes.c_uint32, ctypes.c_int, ctypes.POINTER(ctypes.c_uint32)], ctypes.c_int),
            "mach_vm_read_overwrite": ([ctypes.c_uint32, ctypes.c_uint64, ctypes.c_uint64,
                                        ctypes.c_uint64, ctypes.POINTER(ctypes.c_uint64)], ctypes.c_int),
            "mach_port_deallocate": ([ctypes.c_uint32, ctypes.c_uint32], ctypes.c_int),
        }
        for name, (args, result) in signatures.items():
            function = getattr(self._lib, name)
            function.argtypes, function.restype = args, result

    def capture_process_identity(self, pid, expected_executable_path):
        from . import current
        return current().capture_process_identity(pid, expected_executable_path)

    def open_task(self, pid):
        if type(pid) is not int or not 0 < pid <= 0x7FFFFFFF:
            raise OwnershipError("invalid launched PID")
        port = ctypes.c_uint32()
        status = self._lib.task_for_pid(self._self, pid, ctypes.byref(port))
        if status or not port.value:
            raise CapabilityUnavailable(
                "process memory", "macos", "task_for_pid denied access (Mach status %d)" % status,
                remedy="Continue with observational telemetry and UI; task access is a separate capability.")
        return port.value

    def read(self, task, address, size):
        checked_range(address, size)
        buffer = ctypes.create_string_buffer(size)
        actual = ctypes.c_uint64()
        status = self._lib.mach_vm_read_overwrite(task, address, size, ctypes.addressof(buffer),
                                                  ctypes.byref(actual))
        if status:
            raise MemoryReadError("Mach memory read failed (status %d)" % status)
        if actual.value != size:
            raise MemoryReadError("partial process memory read rejected")
        return buffer.raw

    def close(self, task):
        status = self._lib.mach_port_deallocate(self._self, task)
        if status:
            raise MemoryReadError("Mach task port release failed (status %d)" % status)
