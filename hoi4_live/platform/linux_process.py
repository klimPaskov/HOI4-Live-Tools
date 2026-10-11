"""Exact, read-only Linux process metadata from procfs.

No ptrace attachment, process suspension or memory access happens here. Linux
start ticks are boot-relative; ``creation_time`` is an opaque boot UUID + tick
token, rather than a rounded wall timestamp. Existing portable leases preserve
that token even when they store only creation_time and executable_path.
"""

from dataclasses import dataclass
import os
import posixpath
import stat
import sys
import uuid

from ..errors import CapabilityUnavailable
from .identity import ProcessIdentity


MAX_METADATA_BYTES = 8192
MAX_PID = 0x7FFFFFFF
DEAD_STATES = frozenset(("Z", "X", "x"))


@dataclass(frozen=True)
class LinuxProcessIdentity(ProcessIdentity):
    boot_id: str
    start_ticks: int
    executable_device: int
    executable_inode: int


@dataclass(frozen=True)
class ProcessStat:
    pid: int
    name: str
    state: str
    start_ticks: int


@dataclass(frozen=True)
class ExecutableWitness:
    path: str
    device: int
    inode: int


def checked_pid(pid):
    if type(pid) is not int or not 0 < pid <= MAX_PID:
        raise OSError("invalid process PID")
    return pid


def creation_token(boot_id, start_ticks):
    """Keep the boot identity without floating point or lossy hashing."""
    try:
        boot = uuid.UUID(boot_id)
    except (TypeError, ValueError, AttributeError) as exc:
        raise OSError("cannot read exact Linux boot identity") from exc
    if str(boot) != boot_id or not boot.int:
        raise OSError("invalid Linux boot identity")
    if type(start_ticks) is not int or not 0 <= start_ticks < (1 << 64):
        raise OSError("invalid Linux process start ticks")
    return (boot.int << 64) | start_ticks


def parse_stat(text, expected_pid):
    """Parse field 22 after the final comm delimiter (comm can contain ')')."""
    checked_pid(expected_pid)
    if not isinstance(text, str) or not text or len(text) > MAX_METADATA_BYTES or "\0" in text:
        raise OSError("invalid Linux process stat record")
    left, right = text.find(" ("), text.rfind(") ")
    if left < 1 or right <= left:
        raise OSError("invalid Linux process stat delimiters")
    try:
        pid = int(text[:left])
        fields = text[right + 2:].split()
        state, ticks = fields[0], int(fields[19])
    except (ValueError, IndexError) as exc:
        raise OSError("incomplete Linux process stat record") from exc
    if (pid != expected_pid or state not in "RSDZTWtXxKPI" or len(state) != 1
            or not 0 <= ticks < (1 << 64)):
        raise OSError("invalid Linux process stat identity")
    return ProcessStat(pid, text[left + 2:right], state, ticks)


class ProcessAPI:
    """Bounded procfs reads; proc_root injection supports portable fixture tests."""

    def __init__(self, proc_root=None):
        if proc_root is None and not sys.platform.startswith("linux"):
            raise CapabilityUnavailable("process identity", "linux", "requires Linux procfs")
        self.proc_root = os.fspath(proc_root) if proc_root is not None else "/proc"
        if not os.path.isabs(self.proc_root):
            raise ValueError("procfs root must be absolute")

    def process_path(self, pid, entry):
        checked_pid(pid)
        if entry not in ("stat", "comm", "exe", "mem"):
            raise ValueError("unsupported procfs entry")
        return os.path.join(self.proc_root, str(pid), entry)

    @staticmethod
    def _read(path, limit=MAX_METADATA_BYTES):
        with open(path, "rb") as stream:
            data = stream.read(limit + 1)
        if len(data) > limit:
            raise OSError("procfs metadata exceeds the allowed bound")
        return data.decode("utf-8", "surrogateescape")

    def boot_id(self):
        path = os.path.join(self.proc_root, "sys", "kernel", "random", "boot_id")
        boot_id = self._read(path, 64).strip()
        creation_token(boot_id, 0)
        return boot_id

    def read_stat(self, pid):
        return parse_stat(self._read(self.process_path(pid, "stat")), pid)

    def image(self, pid):
        link = self.process_path(pid, "exe")
        path = os.readlink(link)
        info = os.stat(link)
        if (not os.path.isabs(path) or "\0" in path or path.endswith(" (deleted)")
                or not stat.S_ISREG(info.st_mode) or info.st_ino <= 0
                or os.readlink(link) != path):
            raise OSError("cannot verify the live process executable")
        return ExecutableWitness(os.path.normpath(path), info.st_dev, info.st_ino)

    def process_name(self, pid):
        # Names are a conservative listing fallback when exe access is denied.
        # They never establish ownership or authorize memory/control.
        return self._read(self.process_path(pid, "comm"), 256).rstrip("\n")

    def list_pids(self):
        with os.scandir(self.proc_root) as entries:
            return sorted(int(entry.name) for entry in entries
                          if entry.name.isascii() and entry.name.isdigit()
                          and 0 < int(entry.name) <= MAX_PID)


def capture_process_identity(pid, expected_executable_path=None, api=None):
    """Reject PID reuse, exec changes and exit while collecting metadata."""
    checked_pid(pid)
    if expected_executable_path is not None:
        if (not isinstance(expected_executable_path, str) or not posixpath.isabs(expected_executable_path)
                or "\0" in expected_executable_path):
            raise OSError("an absolute expected executable path is required")
        expected_executable_path = posixpath.realpath(expected_executable_path)
    api = api if api is not None else ProcessAPI()
    boot = api.boot_id()
    first = api.read_stat(pid)
    if first.state in DEAD_STATES:
        raise OSError("the process has exited")
    image = api.image(pid)
    middle = api.read_stat(pid)
    second_image = api.image(pid)
    last = api.read_stat(pid)
    if (first.pid != pid or middle.pid != pid or last.pid != pid
            or first.start_ticks != middle.start_ticks or first.start_ticks != last.start_ticks
            or middle.state in DEAD_STATES or last.state in DEAD_STATES
            or image != second_image or api.boot_id() != boot):
        raise OSError("process identity changed during query")
    if (not isinstance(image, ExecutableWitness) or not posixpath.isabs(image.path)
            or "\0" in image.path or image.path.endswith(" (deleted)")
            or type(image.device) is not int or image.device < 0
            or type(image.inode) is not int or image.inode <= 0):
        raise OSError("invalid executable identity witness")
    if expected_executable_path is not None and image.path != expected_executable_path:
        raise OSError("launched executable identity does not match")
    return LinuxProcessIdentity(pid, creation_token(boot, first.start_ticks), image.path,
                                boot, first.start_ticks, image.device, image.inode)
