"""The in-game console: open, run commands (pasted, since typed characters drop while the game is busy), read the
response with OCR, and verify it is closed again.

The console toggles with the grave key (`) and slides in over about a second; a toggle pressed mid-slide is
ignored, so every toggle waits out the animation and the state is verified from the screen afterwards.
"""

import re
import time

from ..desktop import clipboard
from ..errors import ToolError
from ..perception import screen as screen_mod

# commands that are not cheats: allowed even in campaign mode
SAFE_COMMANDS = re.compile(r"^\s*(save|savegame|time|help|helphelp|version|fps)\b", re.I)


def is_open(session, img=None):
    img = session.grab() if img is None else img
    return screen_mod.console_open(img)


def _toggle(session):
    session.input.key(session.cfg["console"]["toggle_key"])
    session.sleep(session.cfg["console"]["slide_seconds"])


def open_console(session):
    for _ in range(3):
        if is_open(session):
            return True
        _toggle(session)
    if not is_open(session):
        raise ToolError("the console did not open (is the console key bound to ` on this keyboard layout?)")
    return True


def close_console(session):
    for _ in range(3):
        if not is_open(session):
            return True
        _toggle(session)
    if is_open(session):
        raise ToolError("the console did not close")
    return True


def console_region(img):
    """The console's output area: inside its frame (found on screen), or the usual top-left area."""
    r = screen_mod.console_rect(img)
    h, w = img.shape[:2]
    if r is None:
        return [0, int(h * 0.15), int(w * 0.27), int(h * 0.33)]
    x0, y0, x1, y1 = r
    return [x0 + 9, y0 + 7, x1 - x0 - 18, y1 - y0 - 12]


def read_output(session, img=None):
    """OCR the console log. Its text is white; the map shows through the translucent panel in dimmer
    colours, so only near-white pixels are kept (black text on white) before OCR."""
    import numpy as np
    from ..desktop import images
    from ..perception.topbar import _whiteness
    img = session.grab() if img is None else img
    region = console_region(img)
    crop = images.crop(img, region)
    res = session.ocr.recognize(_whiteness(crop, low=130, gain=2.6), scale=2.5, pad=16)
    return [l.text for l in res.lines], img


def run(session, commands, keep_open=False, read=True):
    """Run console commands in order. Returns [{"command", "response": [new console lines]}]."""
    if isinstance(commands, str):
        commands = [commands]
    results = []
    session.park_cursor()            # a tooltip left open by a hover covers the console frame
    time.sleep(0.3)
    # a side panel's dark frame (Recruit & Deploy) reads as the console's: close panels first (seen live in
    # phase 4: the console "did not close" and the effect never ran)
    from . import panels
    try:
        panels.to_map(session)
    except ToolError:
        pass
    open_console(session)
    before, _ = read_output(session) if read else ([], None)
    with clipboard.Preserved():
        for cmd in commands:
            session.check_cancel()
            session.input.key("end")
            session.input.key("backspace", times=40, delay=0.008)   # clear any leftover input
            clipboard.set_text(cmd)
            time.sleep(0.05)
            session.input.key("ctrl+v")
            time.sleep(0.25)
            session.input.key("enter")
            time.sleep(0.7)
            entry = {"command": cmd}
            if read:
                after, _ = read_output(session)
                entry["response"] = _new_lines(before, after, cmd)
                before = after
            results.append(entry)
    if not keep_open:
        close_console(session)
    return results


def _new_lines(before, after, cmd):
    """Console lines that appeared after the command (best effort: OCR text is compared loosely)."""
    norm = lambda s: re.sub(r"\W+", "", s.lower())
    b = [norm(x) for x in before]
    out = []
    # find the longest suffix of `before` that is a prefix-aligned part of `after`
    start = 0
    for k in range(min(len(b), len(after)), 0, -1):
        if [norm(x) for x in after[:k]] == b[-k:]:
            start = k
            break
    for line in after[start:]:
        if norm(line) and norm(line) != norm(cmd):
            out.append(line)
    return out[-12:]
