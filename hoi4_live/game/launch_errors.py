"""Errors the game reports while it starts, which hoi4_launch must not wait out.

With -debug the game stops on an OK dialog when the map reports a MAP_ERROR, and it opens error.log in Notepad.
The main menu may or may not be drawn behind that dialog, and waiting for it until the launch timeout (up to 30
minutes) only delays the answer the modder needs: the error text. Without -debug a MAP_ERROR is just a line in
error.log, so a normal launch carries on and reports the lines in its result.
"""

import re

from ..errors import ToolError
from . import process

MAP_ERROR_RX = re.compile(r"(?<![A-Za-z])MAP[_ ]?ERROR", re.I)     # not "bitmap error"


def map_error_lines(session, mark="launch", limit=6):
    """MAP_ERROR lines that error.log gained since the mark (folded: a repeated line shows once)."""
    try:
        rows, _ = session.logs.delta("error", mark, MAP_ERROR_RX.pattern, limit)
    except OSError:
        return []
    return [r["line"] for r in rows]


def map_error_on_screen(texts):
    """The first OCR line that names a MAP_ERROR (the dialog's text), or None."""
    for t in texts or ():
        if MAP_ERROR_RX.search(t):
            return t
    return None


def check(session, debug, texts=None):
    """Raise at once when a -debug launch hit a MAP_ERROR. `texts` are OCR lines of the screen when available."""
    if not debug:
        return
    lines = map_error_lines(session)
    shown = map_error_on_screen(texts)
    if not lines and not shown:
        return
    details = {"error": "map_error_on_launch", "map_error_lines": lines}
    if shown:
        details["dialog_text"] = shown
    image = None
    try:
        from ..desktop import images
        image, _, _ = images.encode(session.grab(focus=False), 960)
    except Exception:
        pass
    raise ToolError("the game reported a MAP_ERROR while starting (with -debug it waits on an OK dialog): "
                    + (lines[0] if lines else shown)
                    + ". Fix the map data, or launch without debug. The game is still running; click OK on the "
                      "dialog, or close it with hoi4_shutdown force=true.", details, image=image)


def _exit_code_text(pid):
    status = process.launch_status(pid)
    code = status.get("exit_code") if status else None
    if code is None:
        return None, None
    return code, "0x%08X" % (code & 0xFFFFFFFF)


def _log_lines(session, limit=8):
    """The error.log lines written since the launch started (the game truncates the log at launch)."""
    mark = "launch_start" if session.logs.has_mark("launch_start") else "launch"
    try:
        rows, _ = session.logs.delta("error", mark, None, limit)
        lines = [r["line"] for r in rows]
    except OSError:
        lines = []
    return lines or session.logs.tail("error", limit)


def check_process(session, pid, creation_time=None, reader=None):
    """Stop a launch that cannot succeed any more instead of waiting out its timeout (issue #30).

    Two signs: the launched process is gone (it crashed, or it quit after a fatal error), or the game started its
    error-dialog helper (another hoi4.exe with -error_title, for example "Failed to load the map"). Without -debug
    that dialog never goes away by itself, so the old wait ended only at the client timeout."""
    name = session.cfg["window"]["process_name"]
    dialogs = process.error_dialogs(pid, name, reader)
    gone = not process.owned_process_present(pid, creation_time, name)
    if not dialogs and not gone:
        return
    code, code_text = _exit_code_text(pid)
    details = {"error": "game_failed_during_launch", "pid": pid, "exit_code": code, "exit_code_hex": code_text,
               "error_log": _log_lines(session)}
    parts = ["the game crashed during startup" if gone else "the game reported a fatal error during startup"]
    if code_text:
        parts.append(f"(exit code {code_text})")
    if dialogs:
        d = dialogs[0]
        details["dialog_title"], details["dialog_message"], details["dialog_pid"] = d["title"], d["message"], d["pid"]
        parts.append(f"- dialog \"{d['title']}\"" + (f": {d['message']}" if d["message"] else ""))
    elif details["error_log"]:
        parts.append("- last error.log line: " + details["error_log"][-1])
    if dialogs and not gone:
        parts.append(". The game is still running behind that dialog; close it with hoi4_shutdown force=true")
    raise ToolError(" ".join(parts), details)
