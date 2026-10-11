"""Bounded, read-only access to an explicitly owned Windows process.

Importing this module never opens a process or loads a Windows DLL. The small API
interface is injectable so ownership, handle lifetime and failures can be tested
without accessing a running game.
"""

import ctypes
import ntpath
import os
import struct
from dataclasses import dataclass


PROCESS_VM_READ = 0x0010
PROCESS_QUERY_INFORMATION = 0x0400
READ_ONLY_RIGHTS = PROCESS_VM_READ | PROCESS_QUERY_INFORMATION
MAX_USER_ADDRESS = 0x00007FFFFFFFFFFF
MAX_READ_BYTES = 1 << 20


class MemoryReadError(RuntimeError):
    """An exact read could not be completed safely."""


class OwnershipError(MemoryReadError):
    """The process is not the exact instance authorized by the caller."""


class InvalidMemoryError(MemoryReadError):
    """An address, object or bound is invalid."""


@dataclass(frozen=True)
class ProcessIdentity:
    pid: int
    creation_time: int  # Windows FILETIME, never a rounded timestamp
    executable_path: str


@dataclass(frozen=True)
class ModuleInfo:
    base: int
    size: int
    path: str


def _path_key(path):
    return ntpath.normcase(ntpath.normpath(path))


def _validate_identity(identity):
    if not isinstance(identity, ProcessIdentity):
        raise OwnershipError("an explicit launched process identity is required")
    if type(identity.pid) is not int or identity.pid <= 0:
        raise OwnershipError("invalid launched PID")
    if type(identity.creation_time) is not int or identity.creation_time <= 0:
        raise OwnershipError("exact process creation FILETIME is required")
    if not isinstance(identity.executable_path, str) or not ntpath.isabs(identity.executable_path):
        raise OwnershipError("absolute launched executable path is required")
    if ntpath.basename(identity.executable_path).lower() != "hoi4.exe":
        raise OwnershipError("the owned executable must be hoi4.exe")


def checked_range(address, size, max_size=MAX_READ_BYTES):
    if type(address) is not int or not 0x10000 <= address <= MAX_USER_ADDRESS:
        raise InvalidMemoryError("invalid user-space address")
    if type(size) is not int or not 0 < size <= max_size:
        raise InvalidMemoryError("read size exceeds the allowed bound")
    if address > MAX_USER_ADDRESS - size + 1:
        raise InvalidMemoryError("read range overflows user space")


def capture_launched_identity(launched_pid, expected_executable_path, api=None):
    """Capture metadata for a PID returned by the caller's own successful launch.

    The session must establish ownership before calling this function; this is not
    an attach/discovery API. No VM_READ permission or memory read is used here.
    """
    if type(launched_pid) is not int or launched_pid <= 0:
        raise OwnershipError("invalid launched PID")
    if not isinstance(expected_executable_path, str) or not ntpath.isabs(expected_executable_path):
        raise OwnershipError("absolute launched executable path is required")
    api = api if api is not None else WindowsAPI()
    try:
        handle = api.open_process(launched_pid, PROCESS_QUERY_INFORMATION)
    except OSError as exc:
        raise OwnershipError("cannot query launched process identity") from exc
    if not handle:
        raise OwnershipError("cannot query launched process identity")
    try:
        identity = api.get_process_identity(handle, launched_pid)
        _validate_identity(identity)
        if identity.pid != launched_pid or _path_key(identity.executable_path) != _path_key(expected_executable_path):
            raise OwnershipError("launched executable identity does not match")
        return identity
    except OSError as exc:
        raise OwnershipError("cannot verify launched process identity") from exc
    finally:
        api.close(handle)


class ReadOnlyProcess:
    def __init__(self, identity, *, owned=False, api=None, max_read_bytes=MAX_READ_BYTES):
        _validate_identity(identity)
        if owned is not True:
            raise OwnershipError("the session must authorize its own launched process")
        if type(max_read_bytes) is not int or not 0 < max_read_bytes <= MAX_READ_BYTES:
            raise ValueError("invalid maximum read size")
        self.identity = identity
        self.max_read_bytes = max_read_bytes
        self.api = api if api is not None else WindowsAPI()
        self._handle = None
        try:
            handle = self.api.open_process(identity.pid, READ_ONLY_RIGHTS)
        except OSError as exc:
            raise OwnershipError("cannot open the owned process for reading") from exc
        if not handle:
            raise OwnershipError("cannot open the owned process for reading")
        self._handle = handle
        try:
            self.validate_identity()
        except BaseException:
            self.close()
            raise

    @property
    def closed(self):
        return self._handle is None

    def validate_identity(self):
        if self.closed:
            raise OwnershipError("process reader is closed")
        try:
            actual = self.api.get_process_identity(self._handle, self.identity.pid)
            _validate_identity(actual)
            if (actual.pid != self.identity.pid or actual.creation_time != self.identity.creation_time
                    or _path_key(actual.executable_path) != _path_key(self.identity.executable_path)):
                raise OwnershipError("owned process identity changed")
        except OSError as exc:
            self.close()
            raise OwnershipError("cannot verify the owned process identity") from exc
        except BaseException:
            self.close()
            raise

    def module_info(self):
        self.validate_identity()
        try:
            info = self.api.module_info(self._handle, self.identity.pid)
        except OSError as exc:
            raise MemoryReadError("cannot query owned executable module") from exc
        self.validate_identity()
        if not isinstance(info, ModuleInfo) or _path_key(info.path) != _path_key(self.identity.executable_path):
            self.close()
            raise OwnershipError("module does not belong to the owned executable")
        checked_range(info.base, info.size, MAX_USER_ADDRESS)
        return info

    def read(self, address, size):
        checked_range(address, size, self.max_read_bytes)
        self.validate_identity()
        try:
            data = self.api.read(self._handle, address, size)
        except OSError as exc:
            raise MemoryReadError("process memory read failed") from exc
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
        handle, self._handle = self._handle, None
        if handle is not None:
            self.api.close(handle)

    def __enter__(self):
        self.validate_identity()
        return self

    def __exit__(self, *_args):
        self.close()


class WindowsAPI:
    """Only query/read Win32 calls; no writing, injection or process control."""

    def __init__(self):
        if os.name != "nt":
            raise OSError("Windows process memory access requires Windows")
        if ctypes.sizeof(ctypes.c_void_p) != 8:
            raise OSError("HOI4 memory access requires a 64-bit Python process")
        from ctypes import wintypes as wt
        self.wt = wt
        self.k32 = ctypes.WinDLL("kernel32", use_last_error=True)
        signatures = {
            "OpenProcess": ([wt.DWORD, wt.BOOL, wt.DWORD], wt.HANDLE),
            "CloseHandle": ([wt.HANDLE], wt.BOOL),
            "ReadProcessMemory": ([wt.HANDLE, ctypes.c_void_p, ctypes.c_void_p, ctypes.c_size_t,
                                   ctypes.POINTER(ctypes.c_size_t)], wt.BOOL),
            "GetProcessTimes": ([wt.HANDLE] + [ctypes.POINTER(wt.FILETIME)] * 4, wt.BOOL),
            "GetExitCodeProcess": ([wt.HANDLE, ctypes.POINTER(wt.DWORD)], wt.BOOL),
            "QueryFullProcessImageNameW": ([wt.HANDLE, wt.DWORD, wt.LPWSTR, ctypes.POINTER(wt.DWORD)], wt.BOOL),
            "CreateToolhelp32Snapshot": ([wt.DWORD, wt.DWORD], wt.HANDLE),
        }
        for name, (argtypes, restype) in signatures.items():
            function = getattr(self.k32, name)
            function.argtypes, function.restype = argtypes, restype
        class ModuleEntry(ctypes.Structure):
            _fields_ = [("dwSize", wt.DWORD), ("th32ModuleID", wt.DWORD), ("th32ProcessID", wt.DWORD),
                        ("GlblcntUsage", wt.DWORD), ("ProccntUsage", wt.DWORD),
                        ("modBaseAddr", ctypes.c_void_p), ("modBaseSize", wt.DWORD),
                        ("hModule", wt.HMODULE), ("szModule", wt.WCHAR * 256), ("szExePath", wt.WCHAR * 260)]
        self.ModuleEntry = ModuleEntry
        for name in ("Module32FirstW", "Module32NextW"):
            function = getattr(self.k32, name)
            function.argtypes, function.restype = [wt.HANDLE, ctypes.POINTER(ModuleEntry)], wt.BOOL

    def _error(self):
        return ctypes.WinError(ctypes.get_last_error())

    def open_process(self, pid, rights):
        if rights not in (PROCESS_QUERY_INFORMATION, READ_ONLY_RIGHTS):
            raise ValueError("only read/query process rights are permitted")
        handle = self.k32.OpenProcess(rights, False, pid)
        if not handle:
            raise self._error()
        return handle

    def close(self, handle):
        if not self.k32.CloseHandle(handle):
            raise self._error()

    def get_process_identity(self, handle, pid):
        code = self.wt.DWORD()
        if not self.k32.GetExitCodeProcess(handle, ctypes.byref(code)):
            raise self._error()
        if code.value != 259:  # STILL_ACTIVE
            raise OwnershipError("owned process has exited")
        times = [self.wt.FILETIME() for _ in range(4)]
        if not self.k32.GetProcessTimes(handle, *[ctypes.byref(value) for value in times]):
            raise self._error()
        size = self.wt.DWORD(32768)
        path = ctypes.create_unicode_buffer(size.value)
        if not self.k32.QueryFullProcessImageNameW(handle, 0, path, ctypes.byref(size)):
            raise self._error()
        created = (times[0].dwHighDateTime << 32) | times[0].dwLowDateTime
        return ProcessIdentity(pid, created, path.value)

    def read(self, handle, address, size):
        buffer = ctypes.create_string_buffer(size)
        got = ctypes.c_size_t()
        ok = self.k32.ReadProcessMemory(handle, ctypes.c_void_p(address), buffer, size, ctypes.byref(got))
        if not ok:
            raise self._error()
        if got.value != size:
            raise MemoryReadError("partial process memory read rejected")
        return buffer.raw

    def module_info(self, handle, pid):
        # PID reuse is checked through the held process handle by ReadOnlyProcess.
        identity = self.get_process_identity(handle, pid)
        snapshot = self.k32.CreateToolhelp32Snapshot(0x08 | 0x10, pid)
        if snapshot in (None, 0, ctypes.c_void_p(-1).value):
            raise self._error()
        try:
            entry = self.ModuleEntry()
            entry.dwSize = ctypes.sizeof(entry)
            ok = self.k32.Module32FirstW(snapshot, ctypes.byref(entry))
            while ok:
                if _path_key(entry.szExePath) == _path_key(identity.executable_path):
                    return ModuleInfo(entry.modBaseAddr, entry.modBaseSize, entry.szExePath)
                ok = self.k32.Module32NextW(snapshot, ctypes.byref(entry))
            raise OwnershipError("owned executable module was not found")
        finally:
            self.close(snapshot)
