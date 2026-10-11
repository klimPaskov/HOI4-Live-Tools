"""Exception types shared by every layer.

ToolError is the only exception a tool should raise on purpose: the server turns it into an MCP tool result with
isError=true, so the calling agent reads the message (and the optional screenshot) instead of a protocol failure.
"""


class Hoi4LiveError(Exception):
    """Base class for all expected failures."""


class ToolError(Hoi4LiveError):
    """A tool could not do what was asked. `details` is merged into the structured result; `image` (PNG bytes)
    is attached so the agent sees the screen at the moment of failure."""

    def __init__(self, message, details=None, image=None):
        super().__init__(message)
        self.details = details or {}
        self.image = image


class ValidationError(ToolError):
    """Arguments do not match the tool's input schema. `field` names the offending argument path."""

    def __init__(self, field, message):
        super().__init__(f"invalid argument '{field}': {message}", {"field": field})
        self.field = field


class LockBusy(ToolError):
    """Another process holds the desktop lock."""


class GameNotRunning(ToolError):
    """The action needs a running Hearts of Iron IV window."""


class ModeRefused(ToolError):
    """The session mode (campaign) forbids this test-mode action."""
