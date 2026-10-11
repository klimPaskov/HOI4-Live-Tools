"""macOS backend (paths and processes only so far; window, capture, input, launch and memory are the first Mac tasks).

The Steam layout is taken from the plan in docs/HANDOFF_MAC.md and is UNVERIFIED on a real Mac: confirm the folder
names listed there before relying on game_dir()."""

import os
import re

from .posix import PosixPlatform

STEAM_ROOT = os.path.join("~", "Library", "Application Support", "Steam")
APP_BINARY = os.path.join("hoi4.app", "Contents", "MacOS", "hoi4")


class MacOSPlatform(PosixPlatform):
    name = "macos"

    def game_dir(self):
        root = os.path.expanduser(STEAM_ROOT)
        libs = [root]
        vdf = os.path.join(root, "steamapps", "libraryfolders.vdf")
        if os.path.exists(vdf):
            with open(vdf, encoding="utf-8", errors="replace") as f:
                libs += re.findall(r'"path"\s+"([^"]+)"', f.read())
        for lib in libs:
            p = os.path.join(lib, "steamapps", "common", "Hearts of Iron IV")
            if os.path.isdir(os.path.join(p, "hoi4.app")) or os.path.exists(os.path.join(p, "hoi4")):
                return os.path.normpath(p)
        return None

    def game_executable(self, game_dir):
        if not game_dir:
            return None
        for rel in (APP_BINARY, "hoi4"):
            exe = os.path.join(game_dir, rel)
            if os.path.exists(exe):
                return exe
        return None

    def local_data_dir(self):
        return os.path.join(os.path.expanduser("~"), "Library", "Caches")

    def capture_process_identity(self, pid, expected_executable_path=None):
        from .macos_process import capture_process_identity
        return capture_process_identity(pid, expected_executable_path)

    def process_image(self, pid):
        try:
            return self.capture_process_identity(pid).executable_path
        except OSError:
            return None

    def make_window(self, *args, **kwargs):
        from .macos_desktop import GameWindow
        kwargs.setdefault("process_backend", self)
        kwargs.setdefault("coordinate_space", "logical")
        return GameWindow(*args, **kwargs)

    def make_capturer(self, *args, **kwargs):
        from .macos_desktop import Capturer
        return Capturer(*args, **kwargs)

    def make_input(self, *args, **kwargs):
        from .macos_desktop import Input
        return Input(*args, **kwargs)

    def clipboard(self):
        from .macos_desktop import Clipboard
        return Clipboard()

    def preflight_game_launch(self):
        from .macos_desktop import Native
        native = Native()
        native.permission("input")
        native.permission("capture")
        native.displays()
