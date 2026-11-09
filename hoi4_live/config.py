"""Configuration: defaults, config.json, environment overrides and path discovery.

Lookup order for the file: $HOI4_LIVE_CONFIG, then config.json next to the package (the repository root).
Every key is optional; missing keys fall back to DEFAULTS. Environment overrides:
    HOI4_LIVE_GAME_DIR, HOI4_LIVE_USER_DIR, HOI4_LIVE_DLC_SOURCE_DIR, HOI4_LIVE_ARTIFACTS, HOI4_LIVE_MODE (test|campaign),
    HOI4_LIVE_CALL_LOG (a .jsonl file that records every tool call)
Agent overrides (the `agents` section, see `agents_settings`):
    HOI4_LIVE_AGENT_PROVIDER, HOI4_LIVE_AGENT_BINARY, HOI4_LIVE_AGENT_<ROLE>_MODEL, HOI4_LIVE_AGENT_<ROLE>_EFFORT
    (ROLE = STRATEGIST | CRITIC | PLAYER), HOI4_LIVE_AGENT_REVISION_QUICK_EFFORT, HOI4_LIVE_AGENT_REVISION_FULL_EFFORT
"""

import copy
import json
import os

from . import platform

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

DEFAULTS = {
    "game_dir": "auto",
    "user_dir": "auto",
    "dlc_source_dir": "auto",
    "artifacts_dir": "artifacts",
    "mode": "campaign",
    "play_mode": "fair",
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
    # Provider-neutral model settings for the strategist workflow (scripts/strategist/run.py). Effort is the
    # normalised scale below; each launcher adapter maps it to its provider and clamps a missing level.
    # Defaults decided by the user: Claude strategist sonnet/xhigh, player haiku/medium, revisions xhigh.
    # The critic (sonnet/high, fresh context) is a proposal. OpenAI models are left for the user to set.
    "agents": {
        "provider": "claude",
        "roles": ["strategist", "critic", "player"],
        "effort_scale": ["low", "medium", "high", "xhigh", "max"],
        "providers": {
            "claude": {
                "launcher": "claude-code",
                "binary": "claude",
                "strategist": {"model": "sonnet", "effort": "xhigh",
                               "tools": ["Read", "Write", "Edit", "Glob", "Grep", "Bash"]},
                "critic": {"model": "sonnet", "effort": "high", "tools": ["Read", "Write", "Glob", "Grep", "Bash"]},
                "player": {"model": "haiku", "effort": "medium", "tools": ["Read", "Bash", "mcp__hoi4-live"]},
                "revision": {"quick_effort": "xhigh", "full_effort": "xhigh"},
            },
            "openai": {
                "launcher": "codex",
                "binary": "codex",
                "effort_levels": ["low", "medium", "high"],
                "strategist": {"model": None, "effort": "high"},
                "critic": {"model": None, "effort": "high"},
                "player": {"model": None, "effort": "medium"},
            },
        },
    },
}

AGENT_ROLES = ("strategist", "critic", "player")
EFFORT_SCALE = ("low", "medium", "high", "xhigh", "max")


def _merge(base, extra):
    for k, v in (extra or {}).items():
        if isinstance(v, dict) and isinstance(base.get(k), dict):
            _merge(base[k], v)
        else:
            base[k] = v
    return base


def normalize_effort(value, scale=EFFORT_SCALE):
    """`value` on the normalised effort scale (case-insensitive); ValueError when it is not on it."""
    effort = str(value or "").strip().lower()
    if effort not in scale:
        raise ValueError("effort %r is not one of %s" % (value, "|".join(scale)))
    return effort


def agents_settings(agents, env=None):
    """The resolved agent settings of an `agents` section (DEFAULTS already merged) plus env overrides:

    {"provider", "launcher", "binary", "effort_scale", "provider_config",
     "roles": {role: {"model", "effort", ...role keys}}, "revision": {"quick_effort", "full_effort"}}
    A role whose model is unset keeps model None; the launcher refuses to run it.
    """
    env = os.environ if env is None else env
    scale = tuple(agents.get("effort_scale") or EFFORT_SCALE)
    provider = env.get("HOI4_LIVE_AGENT_PROVIDER") or agents.get("provider") or "claude"
    providers = agents.get("providers") or {}
    if provider not in providers:
        raise ValueError("agents.provider %r has no entry in agents.providers (%s)" % (provider, ", ".join(sorted(providers))))
    pconf = copy.deepcopy(providers[provider])
    roles = {}
    for role in agents.get("roles") or AGENT_ROLES:
        rconf = dict(pconf.get(role) or {})
        up = role.upper()
        model = env.get("HOI4_LIVE_AGENT_%s_MODEL" % up) or rconf.get("model")
        rconf["model"] = model or None
        effort = env.get("HOI4_LIVE_AGENT_%s_EFFORT" % up) or rconf.get("effort") or "medium"
        rconf["effort"] = normalize_effort(effort, scale)
        roles[role] = rconf
    revision = dict(pconf.get("revision") or {})
    strategist_effort = roles.get("strategist", {}).get("effort", "high")
    for kind in ("quick", "full"):
        value = env.get("HOI4_LIVE_AGENT_REVISION_%s_EFFORT" % kind.upper()) or revision.get("%s_effort" % kind)
        revision["%s_effort" % kind] = normalize_effort(value or strategist_effort, scale)
    return {
        "provider": provider,
        "launcher": pconf.get("launcher") or provider,
        "binary": env.get("HOI4_LIVE_AGENT_BINARY") or pconf.get("binary"),
        "effort_scale": list(scale),
        "provider_config": pconf,
        "roles": roles,
        "revision": revision,
    }


def load_agents(path=None, env=None):
    """Agent settings from DEFAULTS, the config file's `agents` section and the environment. Unlike
    `load` it touches no platform backend, so launcher scripts can call it anywhere."""
    env = os.environ if env is None else env
    data = copy.deepcopy(DEFAULTS["agents"])
    path = path or env.get("HOI4_LIVE_CONFIG") or os.path.join(ROOT, "config.json")
    if os.path.exists(path):
        with open(path, encoding="utf-8") as f:
            _merge(data, json.load(f).get("agents") or {})
    return agents_settings(data, env)


def documents_dir():
    """The user's Documents folder (platform backend; on Windows it follows OneDrive redirection)."""
    return platform.current().documents_dir()


def steam_game_dir():
    """Find the HOI4 install (platform backend: Windows registry or the macOS Steam folder)."""
    return platform.current().game_dir()


class Config:
    def __init__(self, data, source):
        self.data = data
        if data["window"].get("process_name") == "auto":
            data["window"]["process_name"] = platform.current().game_process_name
        self.source = source
        env = os.environ
        gd = env.get("HOI4_LIVE_GAME_DIR") or data["game_dir"]
        self.game_dir = steam_game_dir() if gd == "auto" else gd
        ud = env.get("HOI4_LIVE_USER_DIR") or data["user_dir"]
        self.user_dir = platform.current().user_dir() if ud == "auto" else ud
        # where hoi4_prepare copies launcher-v2.sqlite and dlc_signature from: the normal user folder
        ds = env.get("HOI4_LIVE_DLC_SOURCE_DIR") or data.get("dlc_source_dir", "auto")
        self.dlc_source_dir = platform.current().user_dir() if ds == "auto" else ds
        art = env.get("HOI4_LIVE_ARTIFACTS") or data["artifacts_dir"]
        self.artifacts_dir = art if os.path.isabs(art) else os.path.join(ROOT, art)
        self.mode = env.get("HOI4_LIVE_MODE") or data["mode"]
        self.play_mode = env.get("HOI4_LIVE_PLAY_MODE") or data.get("play_mode", "fair")
        self.save_prefix = data["save_prefix"]
        self.lock_path = os.path.expandvars(data["lock"]["path"].replace("%TEMP%", os.environ.get("TEMP") or platform.current().temp_dir()))

    def __getitem__(self, key):
        return self.data[key]

    @property
    def agents(self):
        """Resolved agent settings (`agents_settings`); computed on use so a bad value never stops the server."""
        return agents_settings(self.data["agents"])

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
                "dlc_source_dir": self.dlc_source_dir, "artifacts_dir": self.artifacts_dir, "mode": self.mode, "play_mode": self.play_mode,
                "lock_path": self.lock_path}


_CONFIG = None


def load(path=None, overrides=None):
    data = copy.deepcopy(DEFAULTS)
    data["window"]["process_name"] = platform.current().game_process_name
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
