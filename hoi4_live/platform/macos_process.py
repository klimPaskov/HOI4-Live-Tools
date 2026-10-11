"""Read-only libproc metadata, independent of task ports and VM access."""
import ctypes
import os
import sys

from ..errors import CapabilityUnavailable
from .identity import ProcessIdentity


class _BsdInfo(ctypes.Structure):
    _fields_ = [(name, ctypes.c_uint32) for name in (
        "flags", "status", "xstatus", "pid", "ppid", "uid", "gid", "ruid", "rgid",
        "svuid", "svgid", "reserved")]
    _fields_ += [("comm", ctypes.c_char * 16), ("name", ctypes.c_char * 32)]
    _fields_ += [(name, ctypes.c_uint32) for name in (
        "nfiles", "pgid", "pjobc", "tdev", "tpgid")]
    _fields_ += [("nice", ctypes.c_int32), ("start_sec", ctypes.c_uint64),
                ("start_usec", ctypes.c_uint64)]


class ProcessAPI:
    def __init__(self):
        if sys.platform != "darwin":
            raise CapabilityUnavailable("process identity", "macos", "requires macOS")
        self.lib = ctypes.CDLL("/usr/lib/libproc.dylib", use_errno=True)
        self.lib.proc_pidinfo.argtypes = [ctypes.c_int, ctypes.c_int, ctypes.c_uint64,
                                         ctypes.c_void_p, ctypes.c_int]
        self.lib.proc_pidinfo.restype = ctypes.c_int
        self.lib.proc_pidpath.argtypes = [ctypes.c_int, ctypes.c_void_p, ctypes.c_uint32]
        self.lib.proc_pidpath.restype = ctypes.c_int

    def start_time(self, pid):
        info = _BsdInfo()
        got = self.lib.proc_pidinfo(pid, 3, 0, ctypes.byref(info), ctypes.sizeof(info))
        if got != ctypes.sizeof(info) or info.pid != pid or info.status == 5:
            raise OSError("cannot read exact live process creation time")
        if not info.start_sec or info.start_usec >= 1_000_000:
            raise OSError("invalid process creation time")
        return info.start_sec * 1_000_000 + info.start_usec

    def image(self, pid):
        buf = ctypes.create_string_buffer(4096)
        if self.lib.proc_pidpath(pid, buf, len(buf)) <= 0:
            raise OSError("cannot read process executable")
        return os.fsdecode(buf.value)


def capture_process_identity(pid, expected_executable_path=None, api=None):
    """Validate start/image/start witnesses; never infer ownership from a PID."""
    if type(pid) is not int or pid <= 0:
        raise OSError("invalid process PID")
    api = api or ProcessAPI()
    start = api.start_time(pid)
    image = api.image(pid)
    if not os.path.isabs(image) or api.start_time(pid) != start:
        raise OSError("process identity changed during query")
    image = os.path.realpath(image)
    if expected_executable_path is not None:
        if not os.path.isabs(expected_executable_path) or image != os.path.realpath(expected_executable_path):
            raise OSError("launched executable identity does not match")
    return ProcessIdentity(pid, start, image)
