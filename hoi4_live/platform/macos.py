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
