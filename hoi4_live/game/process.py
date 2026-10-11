"""The hoi4.exe process: launch (normal or -debug), crash detection, and stopping only a process we identify.

The game is started detached and outside any job object of the MCP client, so closing the agent session never
takes the game down with it. The playset (dlc_load.json) and settings are never modified here.
"""

import json
import os
import subprocess
import sys
import time

from .. import platform
from ..errors import ToolError

DETACHED_PROCESS = 0x00000008
CREATE_NEW_PROCESS_GROUP = 0x00000200
CREATE_BREAKAWAY_FROM_JOB = 0x01000000


def game_version(game_dir):
    try:
        with open(os.path.join(game_dir, "launcher-settings.json"), encoding="utf-8") as f:
            data = json.load(f)
        return data.get("version"), data.get("rawVersion")
    except (OSError, ValueError):
        return None, None


def playset(user_dir):
    """Enabled mods from dlc_load.json (read only)."""
    try:
        with open(os.path.join(user_dir, "dlc_load.json"), encoding="utf-8") as f:
            data = json.load(f)
    except (OSError, ValueError):
        return None
    mods = []
    for m in data.get("enabled_mods", []):
        name = m
        p = os.path.join(user_dir, m)
        try:
            with open(p, encoding="utf-8", errors="replace") as f:
                for line in f:
                    if line.strip().startswith("name"):
                        name = line.split("=", 1)[1].strip().strip('"')
                        break
        except OSError:
            pass
        mods.append({"descriptor": m, "name": name})
    return {"enabled_mods": mods, "disabled_dlcs": data.get("disabled_dlcs", [])}


def crash_dirs(user_dir):
    d = os.path.join(user_dir, "crashes")
    try:
        return set(os.listdir(d))
    except OSError:
        return set()


def launch(game_dir, args):
    exe = platform.current().game_executable(game_dir) or os.path.join(game_dir, "hoi4.exe")
    if not os.path.exists(exe):
        raise ToolError(f"the game executable was not found in {game_dir}; set game_dir in config.json")
    if sys.platform != "win32":
        # Detached from this process on POSIX; a macOS launch through `open` or the Steam URL is a Mac task.
        p = subprocess.Popen([exe] + list(args), cwd=game_dir, start_new_session=True, stdin=subprocess.DEVNULL,
                             stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, close_fds=True)
        return p.pid
    flags = DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP
    for extra in (CREATE_BREAKAWAY_FROM_JOB, 0):
        try:
            p = subprocess.Popen([exe] + list(args), cwd=game_dir, creationflags=flags | extra,
                                 stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
                                 close_fds=True)
            return p.pid
        except OSError:
            if extra == 0:
                raise
    return None


def running_pids(name=None):
    return platform.current().find_processes(name or platform.current().game_process_name)


def wait_exit(pid, timeout=60):
    t0 = time.time()
    while time.time() - t0 < timeout:
        if not platform.current().pid_alive(pid):
            return True
        time.sleep(1)
    return False


def kill(pid):
    """Last resort for a frozen game: terminate exactly this pid (never by image name)."""
    return platform.current().terminate(pid)
