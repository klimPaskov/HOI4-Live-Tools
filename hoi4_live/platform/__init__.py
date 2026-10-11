"""Operating-system abstraction. `current()` is the backend for this machine; tests may swap it with `set_backend`."""

import sys

from .base import Platform, PlatformNotSupported

_BACKEND = None


def _detect():
    if sys.platform.startswith("win"):
        from .windows import WindowsPlatform
        return WindowsPlatform()
    if sys.platform == "darwin":
        from .macos import MacOSPlatform
        return MacOSPlatform()
    from .posix import PosixPlatform
    return PosixPlatform()


def current():
    global _BACKEND
    if _BACKEND is None:
        _BACKEND = _detect()
    return _BACKEND


def set_backend(backend):
    """Install `backend` (None re-detects). For tests."""
    global _BACKEND
    _BACKEND = backend


__all__ = ["Platform", "PlatformNotSupported", "current", "set_backend"]
