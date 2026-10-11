"""Run one tool call: validate arguments, enforce the session mode, hold the desktop lock, convert the result.

Used by the MCP server (tools/call) and by the command line (`python -m hoi4_live call <tool> <json>`), so both
paths behave identically.
"""

import logging
import time
import traceback

from ..errors import ModeRefused, ToolError, ValidationError
from ..protocol.schema import validate
from .. import perf
from ..game import guard
from . import registry

log = logging.getLogger("hoi4_live.dispatch")


def _guard(session, t):
    """Close notices and answer trivial events before a campaign action (see game/guard.py)."""
    if t.family not in ("campaign", "military") or t.name in guard.SKIP or not getattr(session, "guard_windows", True):
        return
    with session.lock.hold(t.name):
        guard.clear_windows(session)


def _clock_state(session):
    try:
        from ..perception import topbar
        return topbar.pause_state(session.grab(focus=False))[0]
    except Exception:
        return "unknown"


def _restore_pause(session, t, before, result):
    """A campaign action must not change the clock: a stray Space (typed into the search box of a window that did
    not take focus) once ran 50 days. If the game was paused and runs after the tool, pause it again."""
    if before != "paused":
        return
    if _clock_state(session) == "running":
        from ..game import clock as clock_mod
        with session.lock.hold(t.name):
            clock_mod.set_running(session, False)
        if isinstance(result.data, dict):
            result.data["clock_restored"] = True
        log.warning("tool %s left the game running: paused again", t.name)


def _watches_clock(t):
    return t.family in ("campaign", "military") and t.name not in guard.SKIP


def call(session, name, arguments, progress=None, cancel=None):
    """Returns an MCP CallToolResult dict. Never raises for tool failures."""
    t = registry.get(name)
    if t is None:
        raise KeyError(name)
    started = time.time()
    prev_perf = perf.start()
    before = None
    try:
        args = validate(t.schema, arguments if arguments is not None else {}, "arguments")
        if t.test_only and session.mode == "campaign":
            raise ModeRefused(f"{name} is a test-mode tool and the session is in campaign mode "
                              "(switch with hoi4_lock action=set_mode mode=test)")
        if t.test_only or name == "hoi4_save":
            from ..game import gamemode
            gamemode.check(session, name)
        prev = session.begin_call(name, progress, cancel)
        try:
            if t.lock == "acting":
                with session.ui_mutex, session.lock.hold(name):
                    _guard(session, t)
                    before = _clock_state(session) if _watches_clock(t) else None
                    value = t.handler(session, args)
            elif t.lock == "self":
                with session.ui_mutex:
                    _guard(session, t)
                    before = _clock_state(session) if _watches_clock(t) else None
                    value = t.handler(session, args)
            else:
                value = t.handler(session, args)
        finally:
            session.end_call(prev)
        result = registry.coerce_result(value)
        if before:
            with session.ui_mutex:
                _restore_pause(session, t, before, result)
        if t.family in ("campaign", "military"):
            from .campaign_loop_tools import journal_action
            journal_action(session, name, t.family, args, result)
    except ValidationError as e:
        result = registry.ToolResult(str(e), {"error": "validation", "field": e.field}, is_error=True)
    except ToolError as e:
        data = {"error": type(e).__name__}
        data.update(e.details or {})
        result = registry.ToolResult(str(e), data, is_error=True)
        if before:
            with session.ui_mutex:
                _restore_pause(session, t, before, result)
        if e.image:
            result.add_image(e.image)
    except Exception as e:  # unexpected: report with the location, keep the server alive
        tb = traceback.format_exc()
        log.error("tool %s crashed: %s", name, tb)
        last = tb.strip().splitlines()[-3:]
        result = registry.ToolResult(f"{name} failed unexpectedly: {type(e).__name__}: {e}",
                                     {"error": "internal", "trace": last}, is_error=True)
    elapsed = round(time.time() - started, 2)
    if isinstance(result.data, dict):
        result.data.setdefault("elapsed_s", elapsed)
    log.info("tool %s -> %s in %.2fs", name, "error" if result.is_error else "ok", elapsed)
    mcp = result.to_mcp()
    path = getattr(session, "call_log_path", "")
    if path:
        from . import calllog
        calllog.record(path, name, arguments, mcp, started, time.time(), perf.snapshot())
    perf.end(prev_perf)
    return mcp
