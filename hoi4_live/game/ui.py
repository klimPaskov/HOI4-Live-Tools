"""Generic UI helpers built on capture + OCR + input: find text on screen, click it, wait for a condition.

These are the resolution-independent primitives the menu, console and later campaign actions are built from:
nothing here uses fixed pixel positions.
"""

import re
import time

from ..errors import ToolError
from ..perception import screen as screen_mod


def ocr_screen(session, img=None, region=None, scale=None):
    img = session.grab() if img is None else img
    return img, session.ocr.recognize(img, region=region, scale=scale)


def find_text(session, pattern, img=None, region=None, scale=None, exact=False):
    img, res = ocr_screen(session, img, region, scale)
    if exact:
        rx = re.compile(r"^\W*" + pattern + r"\W*$", re.I)
        lines = [l for l in res.lines if screen_mod.matches(rx, l.text)]
    else:
        lines = screen_mod.find_text(res, pattern)
    return img, lines, res


def click_text(session, pattern, region=None, exact=False, index=0, settle=0.6, retries=2, scale=None,
               prefer="top", park=True):
    """Click the centre of the OCR line matching pattern. Returns the clicked line's text and box. The cursor
    is parked afterwards so its tooltip does not cover the next screen."""
    session.park_cursor()
    last = None
    for attempt in range(retries + 1):
        # small button labels can be missed at 1x on a full screen: later attempts read at 2x
        s = scale if attempt == 0 else 2.0
        img, lines, res = find_text(session, pattern, region=region, exact=exact, scale=s)
        if lines:
            if prefer == "bottom":
                lines = sorted(lines, key=lambda l: -l.box[1])
            line = lines[min(index, len(lines) - 1)]
            x, y = line.center
            session.input.click(x, y)
            if park:
                session.park_cursor()
            time.sleep(settle)
            return {"text": line.text, "box": line.box}
        last = [l.text for l in res.lines][:25]
        time.sleep(0.8)
    raise ToolError(f"text matching '{pattern}' not found on screen", {"seen": last})


def wait_for(session, predicate, timeout=30.0, interval=0.8, what="condition"):
    """Poll predicate(img) on fresh screenshots until it returns a truthy value."""
    t0 = time.time()
    while time.time() - t0 < timeout:
        session.check_cancel()
        img = session.grab()
        v = predicate(img)
        if v:
            return v, img
        time.sleep(interval)
    raise ToolError(f"timed out after {int(timeout)} s waiting for {what}")


def classify(session, img=None):
    img = session.grab() if img is None else img
    idle = session.logs.idle_seconds("game")
    return screen_mod.classify(img, session.ocr, idle), img
