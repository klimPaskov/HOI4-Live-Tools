"""The decisions panel: categories, decision rows (name, cost, available or not, timers) and taking a decision
(HOI4 1.19, verified live on 1.19.2).

Panel (Shift+Q, title "Events and Decisions"), vanilla layout at gui_scale 1, after its slide-in animation:
    category   a header bar with the category name centred (x ~264) and a collapse button, then its description
    decision   a 41 px row: icon, name at x ~71, political power cost right-aligned before x ~440 (red when it
               cannot be paid), a status dot and the take button at x ~491; the row is tinted green when the
               decision can be taken (conditions met), dark otherwise; timed decisions in progress show "N days"
"""

import re
import time

import numpy as np

from ..perception import glyphs
from . import panels


def _cost_font(session):
    return glyphs.font(session.cfg.game_dir, "hoi_18b")

NAME_X = (55, 120)
COST_X = (419, 447)
TAKE_X = 491
HEADER_CX = 264


def _row_tint(img, y):
    """'green' when the row background is the 'can take' green, 'dark' otherwise."""
    band = img[max(0, y - 8):y + 8, 300:400].astype(np.int16)
    if band.size == 0:
        return None
    r, g, b = band[..., 0].mean(), band[..., 1].mean(), band[..., 2].mean()
    return "green" if g > r + 12 and g > b + 12 else "dark"


def read_visible(session, img=None, categories=None):
    """[{kind: 'category'|'decision', name, y, cost, cost_colour, tint, days}] in screen order. categories: the
    localised category names (from the files) that identify header bars."""
    img = session.grab() if img is None else img
    h = img.shape[0]
    res = session.ocr.recognize(img, region=[0, 125, 470, h - 140], scale=1.8)
    out = []
    lines = sorted(res.lines, key=lambda l: (l.box[1], l.box[0]))
    for l in lines:
        t = l.text.strip()
        if not t or re.fullmatch(r"[\W\d]+", t):
            continue
        cx = l.center[0]
        centred = abs(cx - HEADER_CX) < 70 and l.box[0] > 110
        known = categories and max((panels.similar(t, c) for c in categories), default=0) >= 0.85
        if centred and (known or (not categories and l.box[2] < 260)):
            out.append({"kind": "category", "name": t, "y": l.center[1]})
            continue
        if NAME_X[0] <= l.box[0] <= NAME_X[1] and l.box[2] > 40:
            same = [o for o in lines if abs(o.center[1] - l.center[1]) < 9 and o.box[0] > 330]
            days = next((o for o in same if re.search(r"\d+\s*d", o.text)), None)
            cost, colour = None, None
            if not days:
                # the political power cost right-aligned before x ~446: OCR misses short numbers, so read the
                # cell in the game's font first (hoi_18b, digits only), then fall back to OCR
                cy = l.center[1]
                cell = [COST_X[0], cy - 9, COST_X[1] - COST_X[0], 18]
                if _cost_font(session) is not None:
                    txt = glyphs.read(img[cy - 9:cy + 9, COST_X[0]:COST_X[1]], _cost_font(session), threshold=0.6,
                                      charset="0123456789")
                    if txt.isdigit():
                        cost = int(txt)
                if cost is None:
                    o = next((o for o in same if re.fullmatch(r"\W*\d{1,4}\W*", o.text)), None)
                    if o:
                        cost, cell = int(re.search(r"\d+", o.text).group()), o.box
                if cost is not None:
                    colour = panels.text_colour(img, cell)
            out.append({"kind": "decision", "name": t, "y": l.center[1], "cost": cost, "cost_colour": colour,
                        "tint": _row_tint(img, l.center[1]),
                        "days": int(re.search(r"\d+", days.text).group()) if days else None})
    return out


def _key(r):
    return (r["kind"], panels.norm(r["name"]))


def merge_page(doc, rows):
    """Append a scrolled page to the document: the page's first rows overlap the document's last ones."""
    if not doc:
        doc.extend(rows)
        return
    keys = [_key(r) for r in doc]
    first = [_key(r) for r in rows[:3]]
    start = None
    for i in range(len(keys) - 1, -1, -1):
        if keys[i:i + len(first)] == first[:len(keys) - i] and keys[i] == first[0]:
            start = i
            break
    if start is None:
        doc.extend(rows)
        return
    overlap = len(keys) - start
    doc.extend(rows[overlap:])


def scroll(session, notches):
    session.input.scroll(260, 640, notches)
    time.sleep(0.5 + min(1.0, abs(notches) * 0.02))
    session.park_cursor()
    time.sleep(0.2)


def read_all(session, categories=None, max_pages=25):
    """All rows of the panel (scrolling from the top), each decision with its category."""
    scroll(session, 60)
    doc = []
    last = None
    for _ in range(max_pages):
        rows = read_visible(session, categories=categories)
        merge_page(doc, rows)
        sig = tuple(r["name"] for r in rows)
        if sig == last:
            break
        last = sig
        scroll(session, -6)
    scroll(session, 60)
    out, category = [], None
    for r in doc:
        rr = {k: v for k, v in r.items() if k != "y"}
        if r["kind"] == "category":
            category = r["name"]
        else:
            rr["category"] = category
        out.append(rr)
    return out


def find_row(session, name, max_pages=25, threshold=0.88, categories=None):
    """Scroll from the top until the decision row named `name` is visible; returns the row (with its y)."""
    scroll(session, 60)
    last = None
    for _ in range(max_pages):
        rows = read_visible(session, categories=categories)
        best = None
        for r in rows:
            if r["kind"] != "decision":
                continue
            sc = panels.similar(r["name"], name)
            if sc >= threshold and (best is None or sc > best[0]):
                best = (sc, r)
        if best:
            return best[1]
        sig = tuple(r["name"] for r in rows)
        if sig == last:
            return None
        last = sig
        scroll(session, -6)
    return None


def take(session, row):
    session.input.click(TAKE_X, row["y"])
    time.sleep(1.0)
    session.park_cursor()
    return panels.answer_dialog(session, True, attempts=1)
