"""Compatibility import for Windows callers; loading this facade is portable."""
import sys


def __getattr__(name):
    if name.startswith("__"):
        raise AttributeError(name)
    if sys.platform != "win32":
        from ..errors import CapabilityUnavailable
        raise CapabilityUnavailable("win32." + name, sys.platform, "Windows API requested on another platform")
    from ..platform import win32
    return getattr(win32, name)
