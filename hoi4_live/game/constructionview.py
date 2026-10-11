"""Construction: the construction panel's queue, its building buttons, and queuing a building in a state through
the map camera (HOI4 1.19, verified live on 1.19.2).

Panel (T), vanilla layout at gui_scale 1:
    header   "used/total" civilian factories (with "From trade" and "Owned")
    queue    "Location  Task  #  Priority"; the first row is Consumer Goods, then one row per construction:
             state name, the building's icon, factories "assigned/max"
    buttons  a column right of the queue grouped under "State Buildings", "Shared Buildings", "Province
             Buildings" (and mod groups such as "Experimental Facilities"), icons 46 px apart in rows of four,
             45 px apart; each icon's tooltip title is the building's localised name
Queuing: click the building's button (the map switches to the construction map mode), move the camera to the
state with the Find view (game.mapview.go_to), click the map at the state, read the queue back.
"""

import json
import os
import re
import time

import numpy as np

from ..perception import tooltip
from . import panels

HEADER_RX = re.compile(r"(state|shared|province|provincial)\s*buildings|experimental|facilities|special", re.I)
COLS = [475, 520, 566, 612]
ROW_PITCH = 45


def read_queue(session, img=None):
    """{'used', 'total', 'rows': [{'state', 'factories', 'y'}]} from the open construction panel."""
    img = session.grab() if img is None else img
    h = img.shape[0]
    head = session.ocr.recognize(img, region=[0, 125, 260, 45], scale=2.2)
    out = {}
    m = re.search(r"(\d+)\s*/\s*(\d+)", " ".join(l.text for l in head.lines))
    if m:
        out["used"], out["total"] = int(m.group(1)), int(m.group(2))
    res = session.ocr.recognize(img, region=[0, 225, 425, h - 240], scale=2.0)
    rows = []
    for l in res.lines:
        if l.box[0] > 120 or re.search(r"consumer\s*goods", l.text, re.I) or re.fullmatch(r"[\W\d/]+", l.text):
            continue
        fac = next((x.text for x in res.lines if abs(x.center[1] - l.center[1]) < 14 and 230 < x.box[0] < 330 and
                    re.search(r"\d", x.text)), None)
        rows.append({"state": l.text, "factories": fac, "y": l.center[1]})
    out["rows"] = rows
    return out


def _cache_path(session):
    d = os.path.join(session.cfg.artifacts_dir, "state")
    os.makedirs(d, exist_ok=True)
    return os.path.join(d, "construction_buttons.json")


def _signature(session):
    from ..gamedata import scriptdb
    mods = scriptdb.mods_for(session)
    return f"{session.window.size()}|{[m['path'] for m in mods]}"


def button_cells(session, img=None):
    """[(x, y)] of the building buttons: rows of icons under each group header in the panel's right column."""
    img = session.grab() if img is None else img
    h = img.shape[0]
    res = session.ocr.recognize(img, region=[430, 125, 240, h - 140], scale=1.8)
    heads = sorted([l for l in res.lines if HEADER_RX.search(l.text)], key=lambda l: l.box[1])
    cells = []
    for i, hd in enumerate(heads):
        y0 = hd.center[1] + 50
        y_end = heads[i + 1].center[1] - 20 if i + 1 < len(heads) else h - 40
        y = y0
        while y < y_end:
            row = []
            for x in COLS:
                patch = img[y - 15:y + 15, x - 15:x + 15].astype(np.int16)
                if patch.size and float(patch.max(2).mean()) > 60:       # an icon, not the dark panel
                    row.append((x, y))
            if not row:
                break
            cells += row
            y += ROW_PITCH
    return cells, [h.text for h in heads]


def buttons(session, refresh=False):
    """{building name: [x, y]} from the buttons' tooltips, cached per window size and playset."""
    p = _cache_path(session)
    sig = _signature(session)
    if not refresh:
        try:
            with open(p, encoding="utf-8") as f:
                data = json.load(f)
            if data.get("signature") == sig and data.get("buttons"):
                return data["buttons"]
        except (OSError, ValueError):
            pass
    cells, heads = button_cells(session)
    out = {}
    for x, y in cells:
        d, _, _ = tooltip.read(session, x, y, wait=0.7)
        title = (d.get("title") or "").strip()
        if title:
            out[title] = [x, y]
    with open(p, "w", encoding="utf-8") as f:
        json.dump({"signature": sig, "buttons": out, "groups": heads, "time": time.time()}, f, indent=1)
    return out


def find_button(session, names, refresh=False):
    """(name, [x, y]) of the button whose tooltip title matches one of names."""
    btns = buttons(session, refresh)
    best = None
    for title, xy in btns.items():
        score = max(panels.similar(title, n) for n in names)
        if best is None or score > best[0]:
            best = (score, title, xy)
    if best and best[0] >= 0.85:
        return best[1], best[2]
    return None, None


def selected(img, xy):
    """True when the button at xy shows the selected (highlighted) frame."""
    x, y = xy
    ring = img[y - 24:y + 24, x - 24:x + 24].astype(np.int16)
    if ring.size == 0:
        return False
    edge = np.concatenate([ring[:3].reshape(-1, 3), ring[-3:].reshape(-1, 3), ring[:, :3].reshape(-1, 3),
                           ring[:, -3:].reshape(-1, 3)])
    return float(edge.max(1).mean()) > 120


SLOTS_RX = re.compile(r"^\W*(\d{1,2})\s*/\s*(\d{1,2})\W*$")


def slots_near_centre(session, img=None, radius=(380, 300)):
    """The building slots box of the state under the camera in the construction map mode: the map draws
    "used/total" (and an infrastructure percentage) at each state's centre. Returns {'used', 'total', 'at',
    'distance'} for the box nearest the screen centre, or None."""
    img = session.grab() if img is None else img
    h, w = img.shape[:2]
    cx, cy = w // 2, h // 2
    rx, ry = radius
    region = [max(0, cx - rx), max(0, cy - ry), min(w, cx + rx) - max(0, cx - rx), min(h, cy + ry) - max(0, cy - ry)]
    best = None
    for scale in (2.0, 3.0, 2.5):                 # the box is 19x12 px: Windows OCR misses it at one scale now and then
        res = session.ocr.recognize(img, region=region, scale=scale)
        for l in res.lines:
            m = SLOTS_RX.match(l.text)
            if not m:
                continue
            used, total = int(m.group(1)), int(m.group(2))
            if used > total or total == 0:
                continue
            d = ((l.center[0] - cx) ** 2 + (l.center[1] - cy) ** 2) ** 0.5
            if best is None or d < best["distance"]:
                best = {"used": used, "total": total, "at": [int(l.center[0]), int(l.center[1])], "distance": round(d)}
        if best:
            break
    return best
