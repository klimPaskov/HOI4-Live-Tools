"""The politics panel: law and advisor slots, their selection lists, changing a law and hiring an advisor (HOI4
1.19, verified live on 1.19.2).

Panel (Q), vanilla layout at gui_scale 1: three rows of six slots, 80 px apart from x = 60, 51 px under their
headers:
    "Laws & Government"      conscription (mobilization_laws), trade_laws, economy, political advisor x3
    "Research & Production"  tank, naval, aircraft and materiel designers (military industrial organisations),
                             industrial concern, theorist
    "Military Staff"         army chief, navy chief, air chief, high command x3
A slot's tooltip title is the current law or advisor ("Click to add Political Advisor" when empty). Clicking a
slot opens its selection list right of the panel: laws as one column of 74 px rows (name, political power cost,
effect; a red X when it cannot be taken), advisors as two columns of 74 px cards (name, traits, cost, red X).
"""

import re
import time

import numpy as np

from ..perception import tooltip
from . import panels

ROWS = {"Laws & Government": ["mobilization_laws", "trade_laws", "economy", "political_advisor",
                              "political_advisor", "political_advisor"],
        "Research & Production": ["tank_manufacturer", "naval_manufacturer", "aircraft_manufacturer",
                                  "materiel_manufacturer", "industrial_concern", "theorist"],
        "Military Staff": ["army_chief", "navy_chief", "air_chief", "high_command", "high_command",
                           "high_command"]}
HEADER_RX = {"Laws & Government": re.compile(r"laws\s*&?\s*gov", re.I),
             "Research & Production": re.compile(r"research\s*&?\s*prod", re.I),
             "Military Staff": re.compile(r"military\s*staff", re.I)}
SLOT_X0, SLOT_DX, SLOT_DY = 60, 80, 51
SLOT_ALIASES = {"conscription": "mobilization_laws", "mobilization": "mobilization_laws",
                "mobilisation": "mobilization_laws", "trade": "trade_laws", "economic": "economy",
                "economy_law": "economy", "political": "political_advisor", "advisor": "political_advisor"}


def slot_positions(session, img=None):
    """[{row, index, slot, x, y}] for the 18 slots, anchored on the row headers (OCR)."""
    img = session.grab() if img is None else img
    h = img.shape[0]
    res = session.ocr.recognize(img, region=[0, 500, 540, min(420, h - 500)], scale=1.6)
    out = []
    for row, rx in HEADER_RX.items():
        hd = next((l for l in res.lines if rx.search(l.text)), None)
        if not hd:
            continue
        y = hd.center[1] + SLOT_DY
        for i, slot in enumerate(ROWS[row]):
            out.append({"row": row, "index": i, "slot": slot, "x": SLOT_X0 + SLOT_DX * i, "y": y})
    return out


def slot_image(session, pos, img=None):
    img = session.grab() if img is None else img
    x, y = pos["x"], pos["y"]
    return img[y - 32:y + 32, x - 32:x + 32].copy()


def resolve_slot(name):
    n = (name or "").strip().lower().replace(" ", "_")
    return SLOT_ALIASES.get(n, n)


def read_slot(session, pos, attempts=3):
    d = {}
    for i in range(attempts):
        d, _, _ = tooltip.read(session, pos["x"], pos["y"], wait=0.7 + 0.4 * i)
        if (d.get("title") or "").strip() and d.get("found"):
            break
    title = (d.get("title") or "").strip()
    empty = bool(re.search(r"click\s*to\s*(add|select)|military\s*industrial", title, re.I))
    return {"slot": pos["slot"], "row": pos["row"], "index": pos["index"], "title": title, "empty": empty,
            "lines": d.get("lines", [])[:6]}


def open_list(session, pos):
    session.input.click(pos["x"], pos["y"])
    time.sleep(1.1)
    session.park_cursor()
    time.sleep(0.3)


def list_title(session, img=None):
    img = session.grab() if img is None else img
    res = session.ocr.recognize(img, region=[548, 84, 490, 42], scale=2.0)
    return " ".join(l.text for l in res.lines)


def red_marks(img, box):
    """(red X pixels, red text pixels) inside box: the X marks a choice that cannot be taken (a condition), red
    digits a cost that cannot be paid now."""
    x, y, w, h = box
    crop = img[max(0, y):y + h, max(0, x):x + w].astype(np.int16)
    if crop.size == 0:
        return 0, 0
    r, g, b = crop[..., 0], crop[..., 1], crop[..., 2]
    xmark = int(((r > 140) & (g < 80) & (b < 50) & (r - g > 90)).sum())
    text = int(((r > 170) & (g > 55) & (g < 130) & (b > 35) & (b < 115) & (r - g > 70)).sum())
    return xmark, text


def read_cards(session, img=None, two_columns=None):
    """Entries of the open selection list: [{name, lines, cost, blocked, center}]. Laws are one column of rows,
    advisors two columns of cards (decided from the layout when two_columns is None)."""
    img = session.grab() if img is None else img
    h = img.shape[0]
    top, bottom = 130, min(h - 20, 680)
    res = session.ocr.recognize(img, region=[548, top, 490, bottom - top], scale=1.7)
    if two_columns is None:
        two_columns = not re.search(r"\blaw\b", list_title(session, img), re.I)
    cols = [(555, 780), (785, 1010)] if two_columns else [(555, 1012)]
    lines = _column_lines(res, cols)
    cards = []
    for x0, x1 in cols:
        col = sorted([l for l in lines if x0 <= l.box[0] < x1], key=lambda l: l.box[1])
        cur = None
        for l in col:
            if re.fullmatch(r"[\W\d%+.,]+", l.text) or re.fullmatch(r"\W*[il|]{2,4}\W*", l.text):
                if cur is not None and re.search(r"\d", l.text):
                    cur["numbers"].append(l.text)
                continue
            if cur is None or l.box[1] - cur["y"] > 60:
                cur = {"name": l.text, "lines": [], "numbers": [], "y": l.box[1], "x0": x0, "x1": x1}
                cards.append(cur)
            elif l.box[1] - cur["y"] > 5:
                cur["lines"].append(l.text)
    out = []
    for c in cards:
        box = [c["x0"], c["y"] - 12, c["x1"] - c["x0"], 70]
        cost = None
        for n in c["numbers"] + [d for d in c["lines"] if re.search(r"\d{2,4}\W*$", d)]:
            m = re.search(r"(\d{2,4})\W*$", n) or re.search(r"(\d{2,4})", n)
            if m:
                cost = int(m.group(1))
                break
        details = [d for d in c["lines"] if not re.fullmatch(r"\W*\w{0,3}\W*\d{2,4}\W*", d)]
        xmark, red = red_marks(img, [box[0] + 60, box[1] + 2, box[2] - 60, 68])
        out.append({"name": c["name"], "details": details, "cost": cost,
                    "condition_unmet": xmark >= 10, "cannot_afford": red >= 10,
                    "blocked": xmark >= 10 or red >= 10,
                    "center": (c["x0"] + (c["x1"] - c["x0"]) // 2, c["y"] + 20)})
    return out


class _Line:
    def __init__(self, text, box):
        self.text, self.box = text, box


def _column_lines(res, cols):
    """OCR lines re-cut at column borders (OCR joins the two columns of a card grid into one line)."""
    out = []
    for l in res.lines:
        groups = {}
        for wd in l.words:
            cx = wd["box"][0] + wd["box"][2] / 2.0
            ci = next((i for i, (x0, x1) in enumerate(cols) if x0 <= cx < x1), None)
            if ci is not None:
                groups.setdefault(ci, []).append(wd)
        for ci, wds in groups.items():
            x0 = min(w["box"][0] for w in wds)
            y0 = min(w["box"][1] for w in wds)
            x1 = max(w["box"][0] + w["box"][2] for w in wds)
            y1 = max(w["box"][1] + w["box"][3] for w in wds)
            if x0 > 560:
                out.append(_Line(" ".join(w["text"] for w in wds), [x0, y0, x1 - x0, y1 - y0]))
    return out


def close_list(session):
    """The list's close button sits at the top right of the list window."""
    session.input.click(1005, 100)
    time.sleep(0.6)
    session.park_cursor()
