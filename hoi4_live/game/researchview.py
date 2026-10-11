"""Research: the research panel's slots and the technology tree view (HOI4 1.19, verified live on 1.19.2).

Flow to research a technology:
    research panel (W): one row per slot, "Select a technology to research" when idle, else "<name> N day(s)"
    -> click a slot: the tech tree opens on a folder (tabs along the top, positions from interface .gui files)
    -> click the folder's tab; the folder's tiles are laid out from gamedata.tech.tile_offset (gridbox origin +
       grid position * 70 px); the folder's scroll offset is measured by correlating the predicted tile centres
       with the edges of the tiles on screen, so a gridbox quirk or a mod's layout only needs to be roughly right
    -> click the tile: a detail window opens (title, "N day(s)", Research button, description, stats); the title
       is compared with the technology's localised name before anything is committed
    -> Research: the tree closes and the slot shows the technology and its days.
"""

import re
import time

import cv2
import numpy as np

from ..errors import ToolError
from ..gamedata import tech as tech_data
from . import panels

ORIGIN = (10, 102)              # folder window origin on screen at scroll 0 (vanilla, gui_scale 1)
SLOT_TOP, SLOT_PITCH = 300, 100


def read_slots(session, img=None):
    """[{index, state: idle|busy, name, days, box}] from the open research panel."""
    img = session.grab() if img is None else img
    h = img.shape[0]
    res = session.ocr.recognize(img, region=[0, SLOT_TOP - 10, 540, min(h - SLOT_TOP, 760)], scale=1.6)
    rows = {}
    for l in res.lines:
        i = int((l.box[1] - SLOT_TOP + 10) // SLOT_PITCH)
        if i < 0:
            continue
        rows.setdefault(i, []).append(l)
    slots = []
    for i in sorted(rows):
        ls = sorted(rows[i], key=lambda l: (l.box[1], l.box[0]))
        text = " ".join(l.text for l in ls)
        box = [12, SLOT_TOP + i * SLOT_PITCH, 500, 88]
        # "Select a technology to research" (OCR garbles it: "Selec a techno Ogy Eo research")
        if re.search(r"sele\w*\s*a\s*\w|research\W*$", text, re.I) and not re.search(r"\d+\s*day", text, re.I):
            slots.append({"index": i + 1, "state": "idle", "box": box})
            continue
        days = next((l for l in ls if re.search(r"\d+\s*day", l.text, re.I)), None)
        if not days:
            continue
        name = " ".join(l.text for l in ls if l is not days and l.box[1] <= days.box[1] + 6 and
                        not re.fullmatch(r"\W*\d{4}\W*", l.text))
        slots.append({"index": i + 1, "state": "busy", "name": name.strip(),
                      "days": int(re.search(r"\d+", days.text).group()), "box": box})
    return slots


def read_slots_settled(session, attempts=4):
    """read_slots until two reads agree and at least one slot shows (the panel slides in after opening)."""
    last = None
    for i in range(attempts):
        slots = read_slots(session)
        sig = [(s["index"], s["state"], s.get("name")) for s in slots]
        if slots and sig == last:
            return slots
        last = sig
        time.sleep(0.5)
    return slots


def tree_open(session, img=None):
    """True when the tech tree view is open: the folder tabs strip and its close button, no panel title."""
    img = session.grab() if img is None else img
    res = session.ocr.recognize(img, region=[0, 84, 1100, 40], scale=2.0)
    if any(re.search(r"research|political|production|construction", l.text, re.I) for l in res.lines):
        return False
    # the tabs strip: a row of ~82 px dark buttons with coloured icons right under the top bar
    strip = img[92:128, 20:1000].astype(np.int16)
    return float(strip.std()) > 18 and _years_or_tiles(img)


def _years_or_tiles(img):
    """Bright tiles and year labels, or the tree's near-black fill (a modded folder can have so few
    bright pixels that only the fill tells it from the map: 95 % dark pixels against 40 % on the darkest map)."""
    g = cv2.cvtColor(img[140:img.shape[0] - 30, :], cv2.COLOR_RGB2GRAY)
    return float((g > 140).mean()) > 0.01 or float((g < 45).mean()) > 0.6


def close_tree(session):
    w, h = session.window.size()
    session.input.click(w - 35, 105)
    time.sleep(0.8)
    session.park_cursor()


def tab_point(idx, folder):
    t = ((idx.get("layout") or {}).get(folder) or {}).get("tab")
    if not t:
        return None
    return (int(t["x"] + 44), int(t["y"] + 25))


# ---------------------------------------------------------------------------------------------- calibration
def _edge_response(img):
    """High where a 66 px tile's border edges surround the point."""
    g = cv2.cvtColor(img, cv2.COLOR_RGB2GRAY).astype(np.float32)
    ax = np.abs(cv2.Sobel(g, cv2.CV_32F, 1, 0, ksize=3))
    ay = np.abs(cv2.Sobel(g, cv2.CV_32F, 0, 1, ksize=3))
    k, s = 25, 33
    vx = cv2.blur(ax, (3, 2 * k + 1))
    hy = cv2.blur(ay, (2 * k + 1, 3))
    H, W = g.shape
    R = np.zeros_like(g)
    R[:, s:W - s] += vx[:, :W - 2 * s] + vx[:, 2 * s:]
    R[s:H - s, :] += hy[:H - 2 * s, :] + hy[2 * s:, :]
    return R


def folder_points(idx, folder, window_w=None):
    pts = {}
    for tid, t in idx["techs"].items():
        if any(f["name"] == folder for f in t["folders"]):
            o = tech_data.tile_offset(idx, tid, folder, window_w)
            if o:
                pts[tid] = o
    return pts


def calibrate(img, idx, folder, base=ORIGIN, dx_range=(-80, 80), dy_range=(-160, 160)):
    """(ox, oy, score, n): the screen position of the folder's grid origin, found by sliding the predicted tile
    centres over the tile-edge response of the screenshot."""
    pts = folder_points(idx, folder, img.shape[1])
    if not pts:
        return None
    arr = np.array(list(pts.values()), dtype=np.float32)
    R = _edge_response(img)
    H, W = R.shape
    best = None
    for dy in range(dy_range[0], dy_range[1] + 1, 1):
        for dx in range(dx_range[0], dx_range[1] + 1, 2):
            p = arr + [base[0] + dx, base[1] + dy]
            ok = (p[:, 0] > 25) & (p[:, 0] < W - 30) & (p[:, 1] > 150) & (p[:, 1] < H - 25)
            n = int(ok.sum())
            if n < 4:
                continue
            sc = float(R[p[ok, 1].astype(int), p[ok, 0].astype(int)].mean())
            if best is None or sc > best[2]:
                best = (base[0] + dx, base[1] + dy, sc, n)
    return best


# ---------------------------------------------------------------------------------------------- detail window
def find_detail(session, img=None):
    """Box of the Research button of an open technology detail window, or None."""
    img = session.grab() if img is None else img
    w = img.shape[1]
    res = session.ocr.recognize(img, region=[int(w * 0.12), 95, int(w * 0.76), 140], scale=2.0)
    for l in res.lines:
        if re.search(r"^\W*research\W*$", l.text, re.I):
            return l.box
    return None


def read_detail(session, img=None):
    """{title, days, research_button, description} of the open detail window, or None."""
    if img is None:
        w, h = session.window.size()
        session.input.move(int(w * 0.4), 125)
        time.sleep(0.4)
        img = session.grab()
    btn = find_detail(session, img)
    if not btn:
        return None
    ax, ay = btn[0], btn[1]
    t = session.ocr.recognize(img, region=[ax - 414, ay - 52, 540, 30], scale=2.0)
    d = session.ocr.recognize(img, region=[ax - 230, ay - 4, 172, 24], scale=2.5)
    body = session.ocr.recognize(img, region=[ax - 400, ay + 60, 520, 60], scale=1.6)
    m = re.search(r"(\d+)", " ".join(l.text for l in d.lines))
    return {"title": " ".join(l.text for l in t.lines), "days": int(m.group(1)) if m else None,
            "research_button": btn, "button_colour": panels.text_colour(img, btn),
            "description": " ".join(l.text for l in body.lines)}


def close_detail(session):
    session.input.key("esc")
    time.sleep(0.6)
