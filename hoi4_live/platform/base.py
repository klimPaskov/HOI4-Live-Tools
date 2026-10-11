"""The platform interface: everything the server needs from the operating system, behind one object.

`hoi4_live.platform.current()` returns the backend for the running OS (Windows, macOS, or a generic POSIX fallback).
A feature a backend does not provide yet raises `PlatformNotSupported` naming the feature, never an AttributeError.

Status of the groups (see docs/HANDOFF_MAC.md for the migration plan):

    paths      documents folder, HOI4 user dir, Steam game dir, executable path, local data dir, temp dir   IMPLEMENTED
    process    find pids by name, liveness, terminate, image path                                           IMPLEMENTED
    window     find the game window, client rectangle, focus, move, close                                   desktop/window.py + desktop/win32.py (Windows only, not yet behind this interface)
    capture    grab the game window as an image                                                               desktop/capture.py (Windows only, not yet behind this interface)
    input      mouse, keyboard, scroll, text                                                                  desktop/inputs.py (Windows only, not yet behind this interface)
    clipboard  get/set text                                                                                   desktop/clipboard.py (Windows only, not yet behind this interface)
    launcher   start the game detached                                                                        game/process.launch (Windows only, not yet behind this interface)
    recording  ffmpeg input arguments for a window or screen                                                  scripts/record_screen.py (gdigrab, Windows only)
    memory     open a process for reading, module base, build identity                                        memory/pe.py (PE) on Windows; Mach-O on macOS is the first Mac task
"""

import abc
import os
import tempfile


class PlatformNotSupported(NotImplementedError):
    """A platform feature has no backend on this OS yet."""

    def __init__(self, feature, platform_name):
        super().__init__("%s is not implemented for %s yet" % (feature, platform_name))
        self.feature = feature
        self.platform_name = platform_name


class Platform(abc.ABC):
    name = "generic"

    # ---- paths -------------------------------------------------------------------------------------------------
    @abc.abstractmethod
    def documents_dir(self):
        """The user's Documents folder."""

    def user_dir(self):
        """Where HOI4 keeps mods, saves, logs, settings.txt and dlc_load.json."""
        return os.path.join(self.documents_dir(), "Paradox Interactive", "Hearts of Iron IV")

    def game_dir(self):
        """The Steam install folder of HOI4, or None."""
        return None

    @property
    def game_process_name(self):
        """The process name the game runs under (the default of config window.process_name)."""
        return "hoi4"

    def game_executable(self, game_dir):
        """The file to start for `game_dir`, or None when it is not there."""
        return None

    def local_data_dir(self):
        """A per-user folder for caches (script index, save index, RTTI tables)."""
        return os.path.join(os.path.expanduser("~"), ".cache")

    def temp_dir(self):
        return tempfile.gettempdir()

    # ---- process -----------------------------------------------------------------------------------------------
    def find_processes(self, name):
        raise PlatformNotSupported("process listing", self.name)

    def pid_alive(self, pid):
        raise PlatformNotSupported("process liveness", self.name)

    def terminate(self, pid):
        raise PlatformNotSupported("process termination", self.name)

    def process_image(self, pid):
        raise PlatformNotSupported("process image path", self.name)

    # ---- not yet behind the interface (see the module docstring) ------------------------------------------------
    def make_window(self, **kwargs):
        raise PlatformNotSupported("window discovery", self.name)

    def make_capturer(self, window, **kwargs):
        raise PlatformNotSupported("window capture", self.name)

    def make_input(self, window, **kwargs):
        raise PlatformNotSupported("mouse and keyboard input", self.name)

    def clipboard(self):
        raise PlatformNotSupported("clipboard", self.name)

    def launch_game(self, game_dir, args):
        raise PlatformNotSupported("game launch", self.name)

    def recording_input_args(self, window, fps):
        raise PlatformNotSupported("screen recording", self.name)

    def open_process_memory(self, pid):
        raise PlatformNotSupported("process memory access", self.name)
