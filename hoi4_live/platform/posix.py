"""POSIX basics shared by macOS and the generic fallback: processes through `ps` and signals (no psutil needed)."""

import os
import signal
import subprocess

from .base import Platform


class PosixPlatform(Platform):
    name = "posix"

    def documents_dir(self):
        return os.path.join(os.path.expanduser("~"), "Documents")

    def find_processes(self, name):
        """Pids whose executable basename equals `name` (case-insensitive; `.app` bundles match their binary)."""
        try:
            out = subprocess.run(["ps", "-axo", "pid=,comm="], capture_output=True, text=True, timeout=10).stdout
        except (OSError, subprocess.SubprocessError):
            return []
        wanted = name.lower()
        pids = []
        for line in out.splitlines():
            parts = line.strip().split(None, 1)
            if len(parts) != 2 or not parts[0].isdigit():
                continue
            if os.path.basename(parts[1]).lower() == wanted:
                pids.append(int(parts[0]))
        return pids

    def pid_alive(self, pid):
        try:
            os.kill(int(pid), 0)
        except ProcessLookupError:
            return False
        except PermissionError:
            return True
        except (OSError, ValueError):
            return False
        return True

    def terminate(self, pid):
        """Exactly this pid, SIGTERM."""
        try:
            os.kill(int(pid), signal.SIGTERM)
        except OSError:
            return False
        return True

    def process_image(self, pid):
        try:
            out = subprocess.run(["ps", "-p", str(int(pid)), "-o", "comm="], capture_output=True, text=True,
                                 timeout=10).stdout.strip()
        except (OSError, ValueError, subprocess.SubprocessError):
            return None
        return out or None
