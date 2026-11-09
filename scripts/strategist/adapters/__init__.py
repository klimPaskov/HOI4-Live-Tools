"""Launcher adapters: one per agent CLI, chosen by `agents.providers.<provider>.launcher`."""

from .base import CapabilityError, clamp_effort  # noqa: F401
from .claude_code import ClaudeCode
from .codex import Codex

LAUNCHERS = {"claude-code": ClaudeCode, "codex": Codex}


def for_settings(settings):
    """The adapter for resolved agent settings (hoi4_live.config.agents_settings)."""
    launcher = settings.get("launcher")
    if launcher not in LAUNCHERS:
        raise CapabilityError("no adapter for launcher %r (known: %s)" % (launcher, ", ".join(sorted(LAUNCHERS))))
    return LAUNCHERS[launcher](settings)
