"""What every launcher adapter shares: the effort clamp, the binary probe and the invocation record."""

import os
import shutil
import subprocess

SCALE = ("low", "medium", "high", "xhigh", "max")


class CapabilityError(RuntimeError):
    """The launcher cannot do what was asked (CLI missing or broken, model unset, unsupported option)."""


def clamp_effort(effort, supported, scale=SCALE):
    """The provider level for a normalised `effort`: the level itself when supported, else the highest
    supported level below it, else the lowest supported level (e.g. xhigh -> high without xhigh)."""
    if effort not in scale:
        raise ValueError("effort %r is not one of %s" % (effort, "|".join(scale)))
    supported = [s for s in scale if s in supported]
    if not supported:
        raise CapabilityError("the launcher supports no effort level of %s" % "|".join(scale))
    if effort in supported:
        return effort
    below = [s for s in supported if scale.index(s) < scale.index(effort)]
    return below[-1] if below else supported[0]


class Invocation:
    """One planned launcher run: argv, the file fed on stdin, extra environment, and notes."""

    def __init__(self, argv, stdin_path=None, env=None, cwd=None, notes=None, output_last=None):
        self.argv = list(argv)
        self.stdin_path = stdin_path
        self.env = dict(env or {})
        self.cwd = cwd
        self.notes = list(notes or [])
        self.output_last = output_last


class Adapter:
    """Base adapter. Subclasses set `name`, `default_binary`, `levels`, `fallback_binaries`, and implement
    `build` and `parse_output`."""

    name = "base"
    default_binary = None
    levels = SCALE
    fallback_binaries = ()

    def __init__(self, settings):
        self.settings = settings
        self.pconf = settings.get("provider_config") or {}

    def supported_levels(self):
        return tuple(self.pconf.get("effort_levels") or self.levels)

    def effort(self, normalised):
        """(clamped normalised level, the value passed to the provider)."""
        level = clamp_effort(normalised, self.supported_levels())
        native = (self.pconf.get("effort_map") or {}).get(level, level)
        return level, native

    def role(self, role):
        conf = (self.settings.get("roles") or {}).get(role)
        if conf is None:
            raise CapabilityError("role %r is not configured for provider %r" % (role, self.settings.get("provider")))
        if not conf.get("model"):
            raise CapabilityError("agents.providers.%s.%s.model is not set (set it in config.json or HOI4_LIVE_AGENT_%s_MODEL)"
                                  % (self.settings.get("provider"), role, role.upper()))
        return conf

    def binary_candidates(self):
        configured = self.settings.get("binary") or self.default_binary
        out = []
        if configured and ("/" in configured or os.sep in configured):
            out.append(configured)  # an explicit path is tried as given (the probe says if it runs)
        elif configured and shutil.which(configured):
            out.append(shutil.which(configured))
        out += [b for b in self.fallback_binaries if os.path.isfile(b) and b not in out]
        return out

    def resolve_binary(self, probe=None):
        """The first candidate binary whose `--version` runs, with that version text."""
        probe = probe or _probe
        tried = []
        for candidate in self.binary_candidates():
            ok, text = probe(candidate)
            if ok:
                return candidate, text
            tried.append("%s (%s)" % (candidate, text))
        raise CapabilityError("%s CLI is not runnable; tried: %s. Set agents.providers.%s.binary or HOI4_LIVE_AGENT_BINARY"
                              % (self.name, ", ".join(tried) or "nothing found on PATH", self.settings.get("provider")))

    def build(self, binary, role, effort, prompt_file, run_dir, cwd, add_dirs=()):
        raise NotImplementedError

    def parse_output(self, stdout_text, invocation):
        return {}


def _probe(binary):
    try:
        done = subprocess.run([binary, "--version"], stdin=subprocess.DEVNULL, capture_output=True,
                              text=True, timeout=20)
    except (OSError, subprocess.TimeoutExpired) as e:
        return False, str(e)
    text = (done.stdout or done.stderr).strip().splitlines()
    return done.returncode == 0, text[0] if text else "exit %d" % done.returncode
