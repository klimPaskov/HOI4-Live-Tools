"""Linux paths/processes and honest capability-aware desktop factories.

Native Steam installations are discovered independently of Windows/macOS.
Desktop placeholders let the stdio server start on a headless machine, but every
actual window/capture/input operation fails with a typed capability error until
an implemented backend is injected. No desktop availability or live acceptance
is inferred from a DISPLAY variable, Steam files or offline tests.
"""

import errno
import os
import re
import signal

from ..errors import CapabilityUnavailable
from .posix import PosixPlatform


GAME_FOLDER = "Hearts of Iron IV"
STEAM_APP_ID = "394360"
MAX_PATH_FILE_BYTES = 1 << 20
_QUOTED_PAIR = re.compile(r'"((?:\\.|[^"\\])*)"\s*"((?:\\.|[^"\\])*)"')


def _vdf_unquote(value):
    return re.sub(r'\\([\\"])', r'\1', value)


def _read_pairs(path):
    try:
        with open(path, "r", encoding="utf-8", errors="replace") as stream:
            text = stream.read(MAX_PATH_FILE_BYTES + 1)
        if len(text) > MAX_PATH_FILE_BYTES:
            return []
    except OSError:
        return []
    # Steam uses quoted key/value pairs. Never evaluate or source these files.
    return [(_vdf_unquote(key), _vdf_unquote(value)) for key, value in _QUOTED_PAIR.findall(text)]


def _unavailable(feature):
    raise CapabilityUnavailable(feature, "linux", "no verified Linux desktop backend is implemented",
                                "Use telemetry and offline data tools; Linux desktop actions need an implemented backend.")


class _UnavailableWindow:
    def __init__(self, process_name="hoi4", title_prefix="Hearts of Iron IV", pin=True,
                 process_backend=None, expected_identity=None, **_kwargs):
        self.process_name, self.title_prefix, self.pin = process_name, title_prefix, pin
        self.process_backend = process_backend
        self._identity = expected_identity
        self.hwnd = None
        self.pid = expected_identity.pid if expected_identity else None

    def bind_identity(self, identity):
        if self._identity is not None and self._identity != identity:
            raise CapabilityUnavailable("window ownership", "linux", "window is bound to a different process")
        self._identity, self.pid = identity, identity.pid

    def process_ids(self):
        return self.process_backend.find_processes(self.process_name)

    def _refuse(self, *_args, **_kwargs):
        _unavailable("window discovery")

    find = require = client = size = to_screen = from_screen = title = monitor = _refuse
    is_foreground = ensure_foreground = pin_client = close_notepad_logs = _refuse


class _UnavailableCapturer:
    last_method = None

    def __init__(self, window, *_args, **_kwargs):
        self.window = window

    def grab(self, *_args, **_kwargs):
        _unavailable("window capture")

    def close(self):
        pass  # no native resources were acquired


class _UnavailableInput:
    def __init__(self, window, key_delay=0.04, type_delay=0.035, click_hold=0.06, settle=0.25, **_kwargs):
        self.window = window
        self.key_delay, self.type_delay = key_delay, type_delay
        self.click_hold, self.settle = click_hold, settle

    def _refuse(self, *_args, **_kwargs):
        _unavailable("mouse and keyboard input")

    click = key = type_text = drag = scroll = move = park = _refuse
    key_down = key_up = mouse_down = mouse_up = hotkey = _refuse


class LinuxPlatform(PosixPlatform):
    name = "linux"

    def __init__(self, home=None, environ=None, process_api=None, desktop_backend=None):
        self.home = os.path.abspath(home if home is not None else os.path.expanduser("~"))
        self.environ = dict(os.environ if environ is None else environ)
        self._process_api = process_api
        self._desktop_backend = desktop_backend

    def _xdg(self, name, default):
        path = self.environ.get(name)
        return os.path.normpath(path) if path and os.path.isabs(path) else os.path.normpath(os.path.join(self.home, default))

    def documents_dir(self):
        cfg = self._xdg("XDG_CONFIG_HOME", ".config")
        try:
            with open(os.path.join(cfg, "user-dirs.dirs"), encoding="utf-8", errors="replace") as stream:
                text = stream.read(MAX_PATH_FILE_BYTES + 1)
        except OSError:
            text = ""
        if len(text) <= MAX_PATH_FILE_BYTES:
            match = re.search(r'^\s*XDG_DOCUMENTS_DIR\s*=\s*"((?:\\.|[^"\\])*)"\s*(?:#.*)?$', text, re.M)
            if match:
                path = _vdf_unquote(match.group(1)).replace("${HOME}", self.home).replace("$HOME", self.home)
                if os.path.isabs(path) and "$" not in path and "\0" not in path:
                    return os.path.normpath(path)
        return os.path.join(self.home, "Documents")

    def user_dir(self):
        return os.path.join(self._xdg("XDG_DATA_HOME", ".local/share"), "Paradox Interactive", GAME_FOLDER)

    def local_data_dir(self):
        return self._xdg("XDG_CACHE_HOME", ".cache")

    def _steam_roots(self):
        candidates = [os.path.join(self._xdg("XDG_DATA_HOME", ".local/share"), "Steam")]
        candidates += [os.path.join(self.home, suffix) for suffix in (
            ".steam/steam", ".steam/root", ".steam/debian-installation", ".local/share/Steam",
            ".var/app/com.valvesoftware.Steam/.local/share/Steam", "snap/steam/common/.local/share/Steam")]
        return list(dict.fromkeys(os.path.realpath(path) for path in candidates))

    def game_dir(self):
        libraries = []
        for root in self._steam_roots():
            libraries.append(root)
            for key, path in _read_pairs(os.path.join(root, "steamapps", "libraryfolders.vdf")):
                if (key.lower() == "path" or key.isdigit()) and os.path.isabs(path) and "\0" not in path:
                    libraries.append(os.path.realpath(path))
        for library in dict.fromkeys(libraries):
            folders = [GAME_FOLDER]
            manifest = os.path.join(library, "steamapps", "appmanifest_%s.acf" % STEAM_APP_ID)
            for key, folder in _read_pairs(manifest):
                if (key.lower() == "installdir" and folder not in ("", ".", "..")
                        and os.path.basename(folder) == folder and "\0" not in folder):
                    folders.insert(0, folder)
            for folder in dict.fromkeys(folders):
                path = os.path.join(library, "steamapps", "common", folder)
                if self.game_executable(path):
                    return os.path.normpath(path)
        return None

    def game_executable(self, game_dir):
        if not game_dir:
            return None
        path = os.path.join(game_dir, "hoi4")
        try:
            if not os.path.isfile(path) or not os.access(path, os.X_OK):
                return None
            with open(path, "rb") as stream:
                if stream.read(4) != b"\x7fELF":
                    return None
        except OSError:
            return None
        return os.path.realpath(path)

    def _proc(self):
        if self._process_api is None:
            from .linux_process import ProcessAPI
            self._process_api = ProcessAPI()
        return self._process_api

    def capture_process_identity(self, pid, expected_executable_path=None):
        from .linux_process import capture_process_identity
        return capture_process_identity(pid, expected_executable_path, api=self._proc())

    def process_image(self, pid):
        try:
            return self.capture_process_identity(pid).executable_path
        except OSError:
            return None

    def find_processes(self, name):
        if not isinstance(name, str) or not name or os.path.basename(name) != name:
            raise ValueError("a process executable basename is required")
        api, found = self._proc(), []
        try:
            pids = api.list_pids()
        except OSError as exc:
            raise CapabilityUnavailable("process listing", "linux", "procfs process listing is unavailable") from exc
        for pid in pids:
            try:
                info = api.read_stat(pid)
                if info.state in ("Z", "X", "x"):
                    continue
                try:
                    basename = os.path.basename(api.image(pid).path)
                except PermissionError:
                    basename = api.process_name(pid)
                if basename.casefold() == name.casefold():
                    found.append(pid)
            except OSError:
                continue  # process exit during a listing is ordinary
        return found

    def pid_alive(self, pid):
        from .linux_process import checked_pid
        try:
            checked_pid(pid)
        except OSError:
            return False
        try:
            return self._proc().read_stat(pid).state not in ("Z", "X", "x")
        except (FileNotFoundError, ProcessLookupError):
            return False
        except PermissionError:
            return True  # denial must never authorize stale-lease takeover
        except OSError as exc:
            raise CapabilityUnavailable("process liveness", "linux", "procfs liveness cannot be verified") from exc

    def terminate(self, pid, expected_identity=None):
        """Use a pidfd bound to a caller-proved identity; never fall back to kill(pid)."""
        from .linux_process import LinuxProcessIdentity
        if (not isinstance(expected_identity, LinuxProcessIdentity) or type(pid) is not int
                or pid != expected_identity.pid):
            raise CapabilityUnavailable("owned process termination", "linux", "an explicit expected process identity is required")
        if not hasattr(os, "pidfd_open") or not hasattr(signal, "pidfd_send_signal"):
            raise CapabilityUnavailable("owned process termination", "linux", "kernel/Python pidfd support is required")
        fd = None
        try:
            if self.capture_process_identity(pid, expected_identity.executable_path) != expected_identity:
                raise OSError("owned process identity changed")
            fd = os.pidfd_open(pid, 0)
            if self.capture_process_identity(pid, expected_identity.executable_path) != expected_identity:
                raise OSError("owned process identity changed during pidfd acquisition")
            signal.pidfd_send_signal(fd, signal.SIGTERM, None, 0)
            return True
        except (ProcessLookupError, FileNotFoundError):
            return False
        except OSError as exc:
            if exc.errno in (errno.ENOSYS, errno.EINVAL, errno.EPERM, errno.EACCES):
                raise CapabilityUnavailable("owned process termination", "linux", "pidfd termination is unavailable or denied") from exc
            raise
        finally:
            if fd is not None:
                os.close(fd)

    def capabilities(self):
        """Implementation status only; this does not grant desktop/kernel permissions."""
        desktop = self._desktop_backend is not None
        return {"paths": True, "process_identity": True, "read_only_memory": True,
                "live_memory_layout": False, "window": desktop, "capture": desktop,
                "input": desktop, "clipboard": desktop, "ocr": False, "recording": False}

    def make_window(self, *args, **kwargs):
        if self._desktop_backend is not None:
            return self._desktop_backend.make_window(*args, **kwargs)
        kwargs.setdefault("process_backend", self)
        return _UnavailableWindow(*args, **kwargs)

    def make_capturer(self, *args, **kwargs):
        if self._desktop_backend is not None:
            return self._desktop_backend.make_capturer(*args, **kwargs)
        return _UnavailableCapturer(*args, **kwargs)

    def make_input(self, *args, **kwargs):
        if self._desktop_backend is not None:
            return self._desktop_backend.make_input(*args, **kwargs)
        return _UnavailableInput(*args, **kwargs)

    def clipboard(self):
        if self._desktop_backend is not None:
            return self._desktop_backend.clipboard()
        _unavailable("clipboard")

    def preflight_game_launch(self):
        if self._desktop_backend is not None:
            return self._desktop_backend.preflight_game_launch()
        _unavailable("game launch prerequisites")

    def recording_input_args(self, window, fps):
        _unavailable("screen recording")

    def open_process_memory(self, identity, *, owned=False, **kwargs):
        from .linux_memory import ReadOnlyProcess
        return ReadOnlyProcess(identity, owned=owned, **kwargs)
