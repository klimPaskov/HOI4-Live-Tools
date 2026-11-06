"""Errors the game reports while it starts, which hoi4_launch must not wait out.

With -debug the game stops on an OK dialog when the map reports a MAP_ERROR, and it opens error.log in Notepad.
The main menu may or may not be drawn behind that dialog, and waiting for it until the launch timeout (up to 30
minutes) only delays the answer the modder needs: the error text. Without -debug a MAP_ERROR is just a line in
error.log, so a normal launch carries on and reports the lines in its result.
"""

import re

from ..errors import ToolError

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
