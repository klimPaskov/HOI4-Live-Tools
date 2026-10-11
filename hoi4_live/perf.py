"""Per-call time accounting: where the wall time of a tool call went (OCR, screen capture, input, sleeps).

`start()` begins a measurement on the calling thread, `snapshot()` returns the totals. Session.__init__ wraps the
OCR, capture, input and sleep entry points with `timed()`, and the dispatcher adds the totals to the call log, so a
recorded run shows which calls were slow and why.
"""

import functools
import threading
import time

_L = threading.local()


def start():
    """Begin a measurement on this thread; returns the one it interrupts (nested calls) for end()."""
    prev = getattr(_L, "d", None)
    _L.d = {}
    return prev


def end(prev):
    _L.d = prev


def add(key, dt):
    d = getattr(_L, "d", None)
    if d is not None:
        d[key + "_n"] = d.get(key + "_n", 0) + 1
        d[key + "_s"] = d.get(key + "_s", 0.0) + dt


def snapshot():
    d = getattr(_L, "d", None) or {}
    return {k: (round(v, 2) if isinstance(v, float) else v) for k, v in d.items()}


def timed(key, fn):
    @functools.wraps(fn)
    def wrapper(*a, **kw):
        t = time.time()
        try:
            return fn(*a, **kw)
        finally:
            add(key, time.time() - t)
    return wrapper


def instrument(obj, names, key):
    for n in names:
        fn = getattr(obj, n, None)
        if fn is not None and not getattr(fn, "_perf", False):
            w = timed(key, fn)
            w._perf = True
            setattr(obj, n, w)


# ---------------------------------------------------------------------------------------------- pace
# The view and tool modules wait for animations and the game's reaction with fixed sleeps tuned for a slow, busy
# machine (about 20 s of a 30 s action were sleeping). `set_pace` scales those sleeps (and the settle time after
# a click) so a fast machine does not pay for a slow one; every action still verifies its result on screen, so a
# sleep that is too short shows as a retry or a clear failure, never as a silent success.
import sys
import time as _time

PACE_MODULES = (
    "hoi4_live.game.focusview", "hoi4_live.game.researchview", "hoi4_live.game.productionview",
    "hoi4_live.game.constructionview", "hoi4_live.game.politicsview", "hoi4_live.game.decisionsview",
    "hoi4_live.game.diplomacyview", "hoi4_live.game.armyview", "hoi4_live.game.deployview",
    "hoi4_live.game.navalairview", "hoi4_live.game.mapview", "hoi4_live.game.panels", "hoi4_live.game.popups",
    "hoi4_live.game.ui", "hoi4_live.tools.economy_tools", "hoi4_live.tools.politics_tools",
    "hoi4_live.tools.military_tools", "hoi4_live.tools.lowlevel", "hoi4_live.tools.campaign_common",
    "hoi4_live.actions.verify", "hoi4_live.perception.tooltip")

PACES = {"careful": 1.0, "normal": 0.6, "fast": 0.35, "faster": 0.2}
_state = {"factor": 1.0, "base_settle": None}
_real_sleep = _time.sleep


class _TimeShim:
    """Stands in for the `time` module of a UI module: sleep() is scaled, everything else is the real thing."""

    def sleep(self, seconds):
        _real_sleep(max(0.0, seconds * _state["factor"]))

    def __getattr__(self, name):
        return getattr(_time, name)


_SHIM = _TimeShim()


def set_pace(factor, session=None):
    """Scale the UI sleeps by `factor` (0.05 to 2). Returns the factor. With a session the click settle time of
    its Input is scaled too."""
    factor = max(0.05, min(2.0, float(factor)))
    _state["factor"] = factor
    for name in PACE_MODULES:
        mod = sys.modules.get(name)
        if mod is not None and getattr(mod, "time", None) in (_time, _SHIM):
            mod.time = _SHIM
    if session is not None:
        inp = session.input
        if _state["base_settle"] is None:
            _state["base_settle"] = inp.settle
        inp.settle = _state["base_settle"] * factor
    return factor


def pace_factor():
    return _state["factor"]
