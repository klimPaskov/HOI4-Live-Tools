"""Configuration: defaults, config.json, environment overrides and path discovery.

Lookup order for the file: $HOI4_LIVE_CONFIG, then config.json next to the package (the repository root).
Every key is optional; missing keys fall back to DEFAULTS. Environment overrides:
    HOI4_LIVE_GAME_DIR, HOI4_LIVE_USER_DIR, HOI4_LIVE_ARTIFACTS, HOI4_LIVE_MODE (test|campaign),
    HOI4_LIVE_CALL_LOG (a .jsonl file that records every tool call)
"""

import copy
import json
import os

from . import platform

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

DEFAULTS = {
    "game_dir": "auto",
    "user_dir": "auto",
    "artifacts_dir": "artifacts",
    "mode": "test",
    "save_prefix": "hoi4live_",
    "lock": {"path": "%TEMP%/hoi4_live_desktop.lock", "stale_seconds": 600, "wait_seconds": 20},
    "window": {"process_name": "hoi4.exe", "title_prefix": "Hearts of Iron IV", "pin_client_to_origin": True},
    "capture": {"method": "auto", "ffmpeg": "ffmpeg", "output_idx": "auto", "timeout_seconds": 20},
    "input": {"key_delay": 0.04, "type_delay": 0.035, "click_hold": 0.06, "settle": 0.25, "park": [0.553, 0.003]},
    "console": {"toggle_key": "grave", "slide_seconds": 1.2},
    "ocr": {"language": "en-US", "idle_shutdown_seconds": 600, "request_timeout_seconds": 30},
    "screenshot": {"max_width": 960, "format": "png"},
    "launch": {"args": [], "debug_args": ["-debug"], "timeout_seconds": 900},
    "timeouts": {"load_seconds": 420, "save_seconds": 180, "log_idle_seconds": 8},
    "time": {"poll_seconds": 0.8, "popup_policy": "first"},
    "call_log": "",
}


def _merge(base, extra):
    for k, v in (extra or {}).items():
        if isinstance(v, dict) and isinstance(base.get(k), dict):
            _merge(base[k], v)
        else:
            base[k] = v
    return base


def documents_dir():
    """The user's Documents folder (platform backend; on Windows it follows OneDrive redirection)."""
    return platform.current().documents_dir()


def steam_game_dir():
    """Find the HOI4 install (platform backend: Windows registry or the macOS Steam folder)."""
    return platform.current().game_dir()


class Config:
    def __init__(self, data, source):
        self.data = data
        self.source = source
        env = os.environ
        gd = env.get("HOI4_LIVE_GAME_DIR") or data["game_dir"]
        self.game_dir = steam_game_dir() if gd == "auto" else gd
        ud = env.get("HOI4_LIVE_USER_DIR") or data["user_dir"]
        self.user_dir = platform.current().user_dir() if ud == "auto" else ud
        art = env.get("HOI4_LIVE_ARTIFACTS") or data["artifacts_dir"]
        self.artifacts_dir = art if os.path.isabs(art) else os.path.join(ROOT, art)
        self.mode = env.get("HOI4_LIVE_MODE") or data["mode"]
        self.save_prefix = data["save_prefix"]
        self.lock_path = os.path.expandvars(data["lock"]["path"].replace("%TEMP%", os.environ.get("TEMP") or platform.current().temp_dir()))

    def __getitem__(self, key):
        return self.data[key]

    @property
    def logs_dir(self):
        return os.path.join(self.user_dir, "logs")

    @property
    def saves_dir(self):
        return os.path.join(self.user_dir, "save games")

    @property
    def settings_path(self):
        return os.path.join(self.user_dir, "settings.txt")

    @property
    def game_exe(self):
        if not self.game_dir:
            return None
        return platform.current().game_executable(self.game_dir) or os.path.join(self.game_dir, "hoi4.exe")

    def summary(self):
        return {"config_file": self.source, "game_dir": self.game_dir, "user_dir": self.user_dir,
                "artifacts_dir": self.artifacts_dir, "mode": self.mode, "lock_path": self.lock_path}


_CONFIG = None


def load(path=None, overrides=None):
    data = copy.deepcopy(DEFAULTS)
    path = path or os.environ.get("HOI4_LIVE_CONFIG") or os.path.join(ROOT, "config.json")
    source = None
    if os.path.exists(path):
        with open(path, encoding="utf-8") as f:
            _merge(data, json.load(f))
        source = os.path.abspath(path)
    _merge(data, overrides or {})
    return Config(data, source)


def get():
    global _CONFIG
    if _CONFIG is None:
        _CONFIG = load()
    return _CONFIG


def set_config(cfg):
    global _CONFIG
    _CONFIG = cfg
