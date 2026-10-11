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


def _campaign_visibility(session, name, args):
    if session.mode != "campaign":
        return
    if name == "hoi4_save_query" and args.get("query", "info") != "info":
        raise ModeRefused("campaign save queries expose only info; use hoi4_state for player-visible state")
    if name == "hoi4_logs" and args.get("log", "error") in ("game", "text"):
        raise ModeRefused("raw gameplay logs can expose hidden state; use hoi4_telemetry read")
    if name == "hoi4_telemetry" and (args.get("action") == "watch" or args.get("raw")):
        raise ModeRefused("campaign telemetry uses filtered read snapshots; watch/raw are test-mode diagnostics")
    if name == "hoi4_state" and (args.get("raw") or "script" in args.get("sections", [])):
        raise ModeRefused("raw/script state is unavailable in campaign mode")
    if name == "hoi4_briefing" and args.get("log_notes"):
        raise ModeRefused("campaign briefings cannot include unfiltered gameplay logs")
    if args.get("tag") and name in ("hoi4_state", "hoi4_briefing", "hoi4_front", "hoi4_army",
                                    "hoi4_orders", "hoi4_navy_air", "hoi4_research", "hoi4_focus",
                                    "hoi4_production", "hoi4_construction", "hoi4_politics", "hoi4_decisions"):
        player = session.telemetry.player()
        if not player or args["tag"] != player:
            raise ModeRefused("campaign country state must be the resolved player; omit tag or wait for telemetry")


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
        if hasattr(session, "authorize_tool"):
            session.authorize_tool(name, args, t.test_only)
        _campaign_visibility(session, name, args)
        if (getattr(session, "owned_process_only", False) and t.lock != "none"
                and name not in ("hoi4_launch", "hoi4_telemetry")):
            session.require_owned_game()
        if t.test_only and session.mode == "campaign":
            raise ModeRefused(f"{name} is a test-mode tool and the session is in campaign mode; use a separate QA session")
        if t.test_only or name == "hoi4_save":
            from ..game import gamemode
            gamemode.check(session, name)
        prev = session.begin_call(name, progress, cancel)
        # The registered wrapper guards direct/background calls. Dispatch has
        # already authorized this exact request before popup/ownership handling.
        handler = getattr(t.handler, "__wrapped__", t.handler)
        try:
            if t.lock == "acting":
                with session.ui_mutex, session.lock.hold(name):
                    _guard(session, t)
                    before = _clock_state(session) if _watches_clock(t) else None
                    value = handler(session, args)
            elif t.lock == "self":
                with session.ui_mutex:
                    _guard(session, t)
                    before = _clock_state(session) if _watches_clock(t) else None
                    value = handler(session, args)
            else:
                value = handler(session, args)
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
        if session.mode == "campaign" and hasattr(session, "play_policy"):
            result.data["play_mode"] = session.play_policy.mode
            result.data["policy_sha256"] = session.play_policy.sha256
    log.info("tool %s -> %s in %.2fs", name, "error" if result.is_error else "ok", elapsed)
    mcp = result.to_mcp()
    path = getattr(session, "call_log_path", "")
    if path:
        from . import calllog
        calllog.record(path, name, arguments, mcp, started, time.time(), perf.snapshot())
    perf.end(prev_perf)
    return mcp
