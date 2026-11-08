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
_LAUNCHES = {}  # retain handles so an unexpected exit has an observable exit code


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


# No Steam variables (SteamAppId, SteamGameId, STEAM_COMPAT_*, the overlay preload ...) reach the game. Refusing to
# launch while a Steam client or steamcmd runs is opt-in: set HOI4_LIVE_REQUIRE_STEAM_CLOSED=1.
REQUIRE_STEAM_CLOSED_ENV = "HOI4_LIVE_REQUIRE_STEAM_CLOSED"
WINDOWS_STEAM = ("steam.exe", "steamcmd.exe", "steamwebhelper.exe")
POSIX_STEAM = ("steam", "steamcmd", "steamwebhelper", "steam_osx", "steam.sh", "steamcmd.sh")


def steam_processes(plat=None, windows=None):
    """(name, pid) of every running Steam client, web helper or steamcmd process."""
    plat = plat or platform.current()
    windows = sys.platform == "win32" if windows is None else windows
    return [(n, pid) for n in (WINDOWS_STEAM if windows else POSIX_STEAM) for pid in plat.find_processes(n)]


def steam_check_required(env=None):
    """True when the environment asks for launches to be refused while Steam runs (off by default)."""
    value = (os.environ if env is None else env).get(REQUIRE_STEAM_CLOSED_ENV, "")
    return value.strip().lower() in ("1", "true", "yes", "on")


def refuse_running_steam(plat=None, windows=None, env=None):
    """Raise when Steam runs and HOI4_LIVE_REQUIRE_STEAM_CLOSED asks for that check; otherwise do nothing."""
    if not steam_check_required(env):
        return
    found = steam_processes(plat, windows)
    if found:
        names = ", ".join(sorted({n for n, _ in found}))
        raise ToolError(f"Steam is running ({names}) and {REQUIRE_STEAM_CLOSED_ENV} is set; "
                        "close Steam and steamcmd, or unset it, before launching the game.")


def launch_env(base=None):
    """The environment for the game: the caller's, without any Steam variable or Steam overlay preload."""
    env = dict(os.environ if base is None else base)
    for key in list(env):
        if key.lower().startswith("steam") or key.upper().startswith("ENABLE_VK_LAYER_VALVE"):
            del env[key]
    for key in ("LD_PRELOAD", "DYLD_INSERT_LIBRARIES"):
        if key in env:
            kept = [p for p in env[key].replace(";", ":").split(":") if p and "gameoverlayrenderer" not in p]
            if kept:
                env[key] = ":".join(kept)
            else:
                del env[key]
    return env


def launch(game_dir, args):
    exe = platform.current().game_executable(game_dir) or os.path.join(game_dir, "hoi4.exe")
    if not os.path.exists(exe):
        raise ToolError(f"the game executable was not found in {game_dir}; set game_dir in config.json")
    refuse_running_steam()
    env = launch_env()
    if sys.platform != "win32":
        # Detached from this process on POSIX; a macOS launch through `open` or the Steam URL is a Mac task.
        p = subprocess.Popen([exe] + list(args), cwd=game_dir, start_new_session=True, stdin=subprocess.DEVNULL, env=env,
                             stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, close_fds=True)
        _LAUNCHES[p.pid] = p
        return p.pid
    flags = DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP
    for extra in (CREATE_BREAKAWAY_FROM_JOB, 0):
        try:
            p = subprocess.Popen([exe] + list(args), cwd=game_dir, creationflags=flags | extra, env=env,
                                 stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
                                 close_fds=True)
            _LAUNCHES[p.pid] = p
            return p.pid
        except OSError:
            if extra == 0:
                raise
    return None


def launch_status(pid):
    """Status only for our retained launch handle, never an arbitrary/recycled PID."""
    child = _LAUNCHES.get(pid)
    if child is None:
        return None
    code = child.poll()
    return {"pid": pid, "running": code is None, "exit_code": code}


def running_pids(name=None):
    return platform.current().find_processes(name or platform.current().game_process_name)


def wait_exit(pid, timeout=60):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        child = _LAUNCHES.get(pid)
        # poll reaps an exited owned POSIX child. kill(pid, 0) alone still sees
        # its zombie and otherwise waits until the timeout despite actual exit.
        if child is not None and child.poll() is not None:
            return True
        if not platform.current().pid_alive(pid):
            return True
        time.sleep(1)
    return False


def kill(pid):
    """Last resort for a frozen game: terminate exactly this pid (never by image name)."""
    return platform.current().terminate(pid)
