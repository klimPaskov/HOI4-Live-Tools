"""Session and safety tools: hoi4_status, hoi4_launch, hoi4_new_game, hoi4_load, hoi4_save, hoi4_shutdown, hoi4_lock."""

import os
import time

from .. import __version__
from ..desktop import win32
from ..errors import ToolError
from ..game import console, menu, process, saves, timectl, ui
from ..perception import popups as popups_mod
from ..perception import topbar
from .common import MODE, apply_mode, attach, result
from .registry import PLANNED, ToolResult, all_tools, tool


@tool("hoi4_status",
      "Situation check without acting: is the game running, its window and version, the playset, the session mode, "
      "who holds the desktop lock, recent saves, log freshness and (look=true) what the screen shows: screen class, "
      "in-game date, running/paused, open popups and console. Call it first.",
      {"properties": {
          "look": {"type": "boolean", "default": True, "description": "read the screen (date, popups, screen class)"},
          "screenshot": {"type": "boolean", "default": False, "description": "attach a screenshot"}}},
      family="session", lock="none",
      examples=[{"args": {}, "note": "full status"}, {"args": {"look": False}, "note": "process/lock only, instant"}])
def hoi4_status(session, args):
    cfg = session.cfg
    hwnd = session.window.find()
    version, raw = process.game_version(cfg.game_dir) if cfg.game_dir else (None, None)
    data = {
        "server_version": __version__,
        "mode": session.mode,
        "game": {"running": bool(hwnd), "pids": process.running_pids(cfg["window"]["process_name"]),
                 "version": version, "game_dir": cfg.game_dir, "user_dir": cfg.user_dir},
        "playset": process.playset(cfg.user_dir),
        "lock": session.lock.holder(),
        "recent_saves": saves.list_saves(session, limit=5) if os.path.isdir(cfg.saves_dir) else [],
        "logs": {n: (round(session.logs.idle_seconds(n), 1) if session.logs.idle_seconds(n) is not None else None)
                 for n in ("error", "game")},
        "tools": len(all_tools()),
        "planned_tools": PLANNED,
    }
    data["logs_note"] = "seconds since last write"
    try:
        from ..game import gamemode
        gm = gamemode.current(session)
        if gm.get("known"):
            data["game_mode"] = {k: gm.get(k) for k in ("ironman", "multiplayer", "difficulty", "players", "mods",
                                                        "player", "file")}
    except Exception:
        pass
    res = ToolResult("", data)
    if hwnd:
        x, y, w, h = session.window.client()
        data["window"] = {"title": session.window.title(), "client": [x, y, w, h],
                          "foreground": session.window.is_foreground()}
        if args["look"]:
            img = session.grab(focus=False)
            data["capture_method"] = session.capturer.last_method
            if not data["window"]["foreground"] and session.capturer.last_method == "ddagrab":
                data["warning"] = "the game is not the foreground window; the screen read may show other windows"
            state = ui.screen_mod.classify(img, session.ocr, session.logs.idle_seconds("game"))
            data["screen"] = state["screen"]
            data["console_open"] = state["console_open"]
            for k in ("panel", "window"):
                if state.get(k):
                    data[k] = state[k]
            if "date" in state:
                data["date"] = state["date"]["hoi4"]
                data["date_text"] = state["date"]["text"]
                ps, ev = topbar.pause_state(img)
                data["pause_hint"] = ps
            found = popups_mod.detect(img)
            data["popups"] = [{"kind": p.kind, "options": len(p.buttons), "box": p.box} for p in found]
            if args["screenshot"]:
                attach(res, session, img, "status")
    res.summary = (f"HOI4 {'running' if hwnd else 'not running'}; mode {session.mode}; "
                   f"screen {data.get('screen', '-')}" + (f" ({data['panel']} open)" if data.get("panel") else "")
                   + f"; date {data.get('date', '-')}; "
                   f"popups {len(data.get('popups', []))}")
    return res


@tool("hoi4_launch",
      "Start Hearts of Iron IV (normal, or -debug) with the current playset and wait for the main menu. If the game "
      "already runs it is left alone (restart=true quits it through its own menu first). The playset and settings "
      "are never changed.",
      {"properties": {
          "debug": {"type": "boolean", "default": False, "description": "launch with -debug"},
          "wait_menu": {"type": "boolean", "default": True},
          "restart": {"type": "boolean", "default": False, "description": "quit a running game first"},
          "timeout_s": {"type": "integer", "minimum": 30, "maximum": 1800, "default": 900},
          "mode": MODE}},
      family="session", lock="acting",
      examples=[{"args": {}, "note": "normal launch"}, {"args": {"debug": True}, "note": "debug build"}])
def hoi4_launch(session, args):
    apply_mode(session, args)
    cfg = session.cfg
    if session.window.find() and not args["restart"]:
        return result("HOI4 is already running; nothing launched", already_running=True,
                      pids=process.running_pids())
    if session.window.find() and args["restart"]:
        menu.quit_to_desktop(session)
        pid = session.window.pid
        if not process.wait_exit(pid, 90):
            raise ToolError("the running game did not exit through its menu; not relaunching")
    crashes_before = process.crash_dirs(cfg.user_dir)
    t0 = time.time()
    launch_args = list(cfg["launch"]["debug_args" if args["debug"] else "args"])
    pid = process.launch(cfg.game_dir, launch_args)
    session.launched_pid = pid
    data = {"pid": pid, "args": launch_args, "debug": args["debug"]}
    # window
    deadline = t0 + args["timeout_s"]
    while time.time() < deadline and not session.window.find():
        session.sleep(2)
        session.report("waiting for the game window")
        if process.crash_dirs(cfg.user_dir) - crashes_before:
            raise ToolError("the game crashed during startup", {"crash": sorted(process.crash_dirs(cfg.user_dir) - crashes_before)})
    if not session.window.find():
        raise ToolError("no game window appeared", data)
    session.logs.mark("launch", ("error", "game", "system", "setup"))
    data["window_after_s"] = round(time.time() - t0, 1)
    if args["wait_menu"]:
        # loading: logs move; the menu is up when they settle and OCR finds the main-menu buttons
        session.sleep(20)
        while time.time() < deadline:
            session.check_cancel()
            new = process.crash_dirs(cfg.user_dir) - crashes_before
            if new:
                raise ToolError("the game crashed during startup", {"crash": sorted(new)})
            if not process.running_pids():
                raise ToolError("the game exited during startup")
            session.window.close_notepad_logs()
            idle = session.logs.idle_seconds("setup") or 0
            if idle > 10:
                try:
                    ok, texts = menu.main_menu_visible(session)
                except ToolError:
                    ok = False
                if ok:
                    break
            session.report(f"loading ({int(time.time() - t0)} s)")
            session.sleep(4)
        else:
            raise ToolError("timed out waiting for the main menu", data)
        session.window.pin_client()
        data["menu_after_s"] = round(time.time() - t0, 1)
    img = session.grab()
    res = ToolResult(f"HOI4 launched (pid {pid}) in {round(time.time() - t0)} s", data)
    return attach(res, session, img, "launch")


@tool("hoi4_new_game",
      "Start a new game from the main menu (or from a running game with exit_to_menu=true): Single Player, New "
      "Game, the scenario (bookmark label, e.g. '1939'), a major country by its name on the Select Country screen, "
      "START. In test mode `tag` switches to any country afterwards with the console. Waits until the map is up.",
      {"properties": {
          "bookmark": {"type": "string", "description": "scenario label regex, e.g. '1936' or 'August, 1939'"},
          "country": {"type": "string", "description": "a major's name as shown, e.g. 'United States', 'German Reich'"},
          "exit_to_menu": {"type": "boolean", "default": False,
                           "description": "resign the running game first (unsaved progress is lost)"},
          "tag": {"type": "string", "pattern": "^[A-Z][A-Z0-9]{2}$", "description": "switch to this tag (test mode)"},
          "mode": MODE}},
      family="session", lock="acting",
      examples=[{"args": {"country": "United States"}, "note": "1936 USA"},
                {"args": {"country": "German Reich", "tag": "SWE"}, "note": "start, then tag-switch to Sweden (test mode)"},
                {"args": {"bookmark": "1939", "country": "Soviet Union", "exit_to_menu": True}}])
def hoi4_new_game(session, args):
    apply_mode(session, args)
    pre = None
    if session.window.find():
        state, _ = ui.classify(session)
        if state["screen"] in ("map", "esc_menu"):
            if not args["exit_to_menu"]:
                raise ToolError("a game is running; pass exit_to_menu=true to resign it (save first if needed)",
                                {"field": "exit_to_menu"})
            pre = menu.quit_to_main_menu(session)
    rep = menu.new_game(session, country=args.get("country"), bookmark=args.get("bookmark"))
    if pre:
        rep["exited_previous_game"] = pre
    if args.get("tag"):
        if session.mode != "test":
            raise ToolError("tag switching is a console command; use test mode")
        rep["tag"] = console.run(session, [f"tag {args['tag']}"])
    d, img = timectl.read_date(session)
    rep["date"] = d.get("hoi4")
    if d.get("hoi4"):
        # game.log is not rewritten for a new game: forget the days of the timeline that was left
        rep["telemetry_days_dropped"] = session.telemetry.drop_after(d["hoi4"])
    res = ToolResult(f"new game started; date {d.get('hoi4')}", rep)
    return attach(res, session, img, "new_game")


@tool("hoi4_load",
      "Load a save (by name, without .hoi4) through the game's Load Game dialog, from the map or the main menu. "
      "Verifies that the map date afterwards matches the save. Any save may be loaded; none is modified.",
      {"properties": {
          "save": {"type": "string", "minLength": 1, "description": "save name without .hoi4"},
          "mode": MODE,
          "timeout_s": {"type": "integer", "minimum": 30, "maximum": 1800}},
       "required": ["save"]},
      family="session", lock="acting",
      examples=[{"args": {"save": "qa_001_fresh_head"}, "note": "fresh 1936 USA test save"}])
def hoi4_load(session, args):
    apply_mode(session, args)
    rep = saves.load(session, args["save"], args.get("timeout_s"))
    if rep.get("date_on_screen"):
        # the telemetry log still holds the days of the timeline that was left: drop those after the loaded date
        rep["telemetry_days_dropped"] = session.telemetry.drop_after(rep["date_on_screen"])
    img = session.grab()
    res = ToolResult(f"loaded {rep['loaded']}: map date {rep['date_on_screen']} (save {rep.get('save_date')})", rep)
    if rep.get("date_matches") is False:
        res.summary += " - WARNING: date differs from the save header"
    return attach(res, session, img, "after_load")


@tool("hoi4_save",
      "Save the running game. The name always gets the hoi4live_ prefix (other saves are never written). Test mode "
      "uses the console `save`; campaign mode uses the Save Game dialog. Waits until the file is complete and "
      "reports its player and date.",
      {"properties": {
          "name": {"type": "string", "minLength": 1, "maxLength": 80, "description": "letters, digits, _ and -"},
          "overwrite": {"type": "boolean", "default": True}},
       "required": ["name"]},
      family="session", lock="acting",
      examples=[{"args": {"name": "smoke"}, "note": "writes save games/hoi4live_smoke.hoi4"}])
def hoi4_save(session, args):
    rep = saves.save(session, args["name"], args["overwrite"])
    return result(f"saved {rep['name']}.hoi4 ({rep['size_mb']} MB, {rep.get('player')}, {rep.get('date')})", **rep)


@tool("hoi4_shutdown",
      "Quit Hearts of Iron IV through its own menu. Needs confirm=true. force=true terminates a frozen game, but only "
      "the process this server launched. The game is shared: prefer leaving it running and paused.",
      {"properties": {
          "confirm": {"type": "boolean", "description": "must be true"},
          "force": {"type": "boolean", "default": False}},
       "required": ["confirm"]},
      family="session", lock="acting",
      examples=[{"args": {"confirm": True}, "note": "graceful quit"}])
def hoi4_shutdown(session, args):
    if not args["confirm"]:
        raise ToolError("confirm must be true to quit the game", {"field": "confirm"})
    if not session.window.find():
        return result("HOI4 is not running")
    pid = session.window.pid
    if args["force"]:
        if pid != session.launched_pid:
            raise ToolError("force only terminates a game this server launched; quit it through the menu instead")
        process.kill(pid)
        return result(f"terminated pid {pid}", pid=pid)
    menu.quit_to_desktop(session)
    exited = process.wait_exit(pid, 120)
    return result("game exited" if exited else "quit requested; the game is still closing", pid=pid, exited=exited)


@tool("hoi4_lock",
      "Desktop lock and session safety. status: who holds %TEMP%/hoi4_live_desktop.lock. acquire: hold it across "
      "several tool calls (a multi-step test run); release: give it back (always do this at the end). set_mode: "
      "test (console, events, eval allowed) or campaign (human-equivalent play only). record: append one JSON line "
      "per tool call (time, tool, argument summary, outcome) to `path` for overlaying a screen recording; an "
      "empty path stops it.",
      {"properties": {
          "action": {"type": "string", "enum": ["status", "acquire", "release", "set_mode", "record", "set_images", "set_pace"],
                     "default": "status"},
          "path": {"type": "string", "description": "record: the .jsonl file to append to; empty stops recording"},
          "pace": {"description": "set_pace: careful (1.0), normal (0.6), fast (0.35), faster (0.2) or a number "
                                  "0.05-2: the factor on every fixed wait of the UI tools",
                   "anyOf": [{"type": "string", "enum": ["careful", "normal", "fast", "faster"]},
                             {"type": "number", "minimum": 0.05, "maximum": 2}]},
          "images": {"type": "string", "enum": ["all", "errors", "none"],
                     "description": "set_images: which results carry a screenshot (errors: only failures; faster)"},
          "purpose": {"type": "string", "default": "hoi4-live-tools run"},
          "wait_s": {"type": "integer", "minimum": 0, "maximum": 600, "default": 20},
          "mode": {"type": "string", "enum": ["test", "campaign"]}}},
      family="session", lock="none",
      examples=[{"args": {"action": "acquire", "purpose": "event 006 test"}},
                {"args": {"action": "release"}}, {"args": {"action": "set_mode", "mode": "campaign"}},
                {"args": {"action": "record", "path": "C:/recordings/campaign/calls.jsonl"}}])
def hoi4_lock(session, args):
    a = args["action"]
    if a == "acquire":
        session.lock.acquire(args["purpose"], args["wait_s"], sticky=True)
    elif a == "release":
        session.lock.release(force_sticky=True)
    elif a == "set_mode":
        if not args.get("mode"):
            raise ToolError("set_mode needs mode", {"field": "mode"})
        session.mode = args["mode"]
    elif a == "set_pace":
        from .. import perf
        p = args.get("pace")
        if p is None:
            raise ToolError("set_pace needs pace", {"field": "pace"})
        perf.set_pace(perf.PACES[p] if isinstance(p, str) else p, session)
    elif a == "set_images":
        if not args.get("images"):
            raise ToolError("set_images needs images (all, errors or none)", {"field": "images"})
        session.image_mode = args["images"]
    elif a == "record":
        if "path" not in args:
            raise ToolError("record needs path (empty to stop)", {"field": "path"})
        session.call_log_path = os.path.abspath(args["path"]) if args["path"] else ""
    return result(f"lock {a}: holder {session.lock.holder()}; mode {session.mode}"
                  + (f"; call log {session.call_log_path or 'off'}" if a == "record" else ""),
                  lock=session.lock.holder(), mode=session.mode, path=session.lock.path, pid=os.getpid(),
                  call_log=session.call_log_path or None, images=session.image_mode,
                  pace=__import__("hoi4_live.perf", fromlist=["x"]).pace_factor())
