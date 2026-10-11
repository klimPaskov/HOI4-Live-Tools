"""Diplomacy with another country and the trade panel (HOI4 1.19, verified live on 1.19.2).

Opening a country's diplomacy view (it has no key): the camera goes to the country's capital state through the
Find view, a click on the map opens that state's view, and its "State Owner" flag opens the owner's diplomacy
view ("DIPLOMACY", the country's name, leader, opinion, and a list of actions).

Actions are rows 29 px apart in the right column: name, political power or command power cost, and a mark: a red
X when the action is not possible now, a green check when it is. A click performs the action, possibly through a
confirmation dialog or a small window (war goal type, lend-lease equipment); the row or the view header changes
afterwards (for example "Cancel improve relations", a justification in progress, a guarantee).

Trade panel (R): civilian factories used/available, resources (surplus, extracted, imported, exported) and the
countries that export them.
"""

import re
import time

import numpy as np

from ..errors import ToolError
from . import mapview, panels

ACTIONS_REGION = (272, 505, 245, 535)          # x, y, w, h of the action column (vanilla, gui_scale 1)


def open_for_state(session, state_name):
    """Camera to the state, click it, click its owner's flag. Returns the diplomacy header."""
    panels.to_map(session)
    cam = mapview.go_to_id(session, int(state_name)) if str(state_name).isdigit() else mapview.go_to(session, state_name)
    mapview.zoom_abs(session, "province") if str(state_name).isdigit() else mapview.zoom(session, "province")
    w, h = session.window.size()
    session.input.click(w // 2, h // 2)
    time.sleep(1.4)
    session.park_cursor()
    img = session.grab()
    res = session.ocr.recognize(img, region=[0, 420, 470, 140], scale=1.8)
    owner = next((l for l in res.lines if re.search(r"state\s*owner", l.text, re.I)), None)
    if not owner:
        raise ToolError(f"the state view of {state_name} did not open (no 'State Owner')",
                        {"seen": [l.text for l in res.lines], "camera": cam})
    session.input.click(owner.box[0] + 12, owner.box[1] + owner.box[3] + 22)   # the flag under the label
    time.sleep(1.4)
    session.park_cursor()
    got = panels.wait_panel(session, "diplomacy", timeout=5.0)
    time.sleep(0.5)                                # the view slides in
    hd = read_header(session)
    if got != "diplomacy":
        raise ToolError("the diplomacy view did not open from the state owner's flag", {"header": hd})
    return hd


def read_header(session, img=None):
    """Country name, relation line (Allies, Neutral ...), leader, and the opinion values near the top."""
    img = session.grab() if img is None else img
    res = session.ocr.recognize(img, region=[230, 125, 300, 70], scale=2.0)
    lines = [l.text for l in sorted(res.lines, key=lambda l: (l.box[1], l.box[0])) if l.box[0] < 370]
    nums = [l.text for l in res.lines if l.box[0] >= 370]
    return {"name": lines[0] if lines else None, "status": lines[1] if len(lines) > 1 else None,
            "leader": lines[2] if len(lines) > 2 else None, "values": nums}


def _mark(img, y):
    """'blocked' (red X), 'ok' (green check) or None for the mark at the right end of an action row."""
    x0 = ACTIONS_REGION[0] + ACTIONS_REGION[2] - 40
    crop = img[y - 9:y + 9, x0:x0 + 36].astype(np.int16)
    if crop.size == 0:
        return None
    r, g, b = crop[..., 0], crop[..., 1], crop[..., 2]
    red = int(((r > 140) & (g < 80) & (b < 60) & (r - g > 80)).sum())
    green = int(((g > 140) & (r < 150) & (g - r > 40) & (b < 150)).sum())
    if red >= 8:
        return "blocked"
    if green >= 8:
        return "ok"
    return None


def read_actions(session, img=None):
    """[{name, cost, mark, y}] of the visible action rows."""
    img = session.grab() if img is None else img
    x, y, w, h = ACTIONS_REGION
    res = session.ocr.recognize(img, region=[x, y, w, h], scale=2.0)
    rows = []
    for l in sorted(res.lines, key=lambda l: l.box[1]):
        if l.box[0] > x + 40 or not re.search(r"[A-Za-z]{3}", l.text):
            continue
        same = [o for o in res.lines if abs(o.center[1] - l.center[1]) < 8 and o.box[0] > x + 150]
        cost = " ".join(o.text for o in same if re.search(r"\d", o.text)) or None
        name = re.sub(r"\s+\S*\d+\S*$", "", l.text).strip()
        rows.append({"name": name, "cost": cost, "mark": _mark(img, l.center[1]), "y": l.center[1]})
    return rows


def scroll_actions(session, notches):
    x, y, w, h = ACTIONS_REGION
    session.input.scroll(x + w // 2, y + h // 2, notches)
    time.sleep(0.6)
    session.park_cursor()
    time.sleep(0.2)


def all_actions(session):
    scroll_actions(session, 15)
    seen = []
    for _ in range(4):
        for r in read_actions(session):
            if not any(panels.similar(r["name"], s["name"]) >= 0.9 for s in seen):
                seen.append({k: v for k, v in r.items() if k != "y"})
        scroll_actions(session, -6)
    scroll_actions(session, 15)
    return seen


def find_action(session, name):
    scroll_actions(session, 15)
    for _ in range(4):
        rows = read_actions(session)
        best = max(((panels.similar(r["name"], name), r) for r in rows), default=(0, None), key=lambda t: t[0])
        if best[1] and best[0] >= 0.8:
            return best[1]
        scroll_actions(session, -6)
    return None


def click_action(session, row):
    x, y, w, h = ACTIONS_REGION
    session.input.click(x + 80, row["y"])
    time.sleep(1.2)
    session.park_cursor()


def read_window(session, img=None):
    """Text of a window opened by an action (war goal choice, lend-lease, a confirmation): OCR of the centre."""
    img = session.grab() if img is None else img
    h, w = img.shape[:2]
    res = session.ocr.recognize(img, region=[540, 80, int(w * 0.55), int(h * 0.8)], scale=1.5)
    return res


# ---------------------------------------------------------------------------------------------- trade
TRADE_LABELS_RX = re.compile(r"factor|used|availab", re.I)


def _number_after(words, label_rx):
    """The first number right of a label word (OCR words sorted by x), not past the next label: OCR drops a lone
    digit now and then ("Civilian Factories 8 Used: 3" read without the 8 must not give 3 for the factories)."""
    for i, w in enumerate(words):
        if re.search(label_rx, w["text"], re.I):
            for v in words[i + 1:i + 4]:
                m = re.search(r"-?\d+", v["text"])
                if m:
                    return int(m.group())
                if TRADE_LABELS_RX.search(v["text"]):
                    return None
    return None


def read_trade(session, img=None):
    """Trade panel: civilian factories (total, used, available for trade) and the market list (country, export
    used/total, delivered, convoys). Resource numbers come from telemetry when available (hoi4_diplomacy)."""
    img = session.grab() if img is None else img
    h = img.shape[0]
    top = session.ocr.recognize(img, region=[110, 130, 560, 30], scale=2.0)
    words = sorted([w for l in top.lines for w in l.words], key=lambda w: w["box"][0])
    out = {"header": " ".join(w["text"] for w in words),
           "civilian_total": _number_after(words, r"factor"), "civilian_used": _number_after(words, r"used"),
           "civilian_available": _number_after(words, r"availab")}
    used, avail, total = out["civilian_used"], out["civilian_available"], out["civilian_total"]
    if used is not None and avail is not None and (total is None or total < used + avail):
        out["civilian_total"] = used + avail         # the panel's total is used + available for trade
        out["civilian_total_from"] = "used + available"
    from ..perception.topbar import _whiteness
    from ..desktop import images
    y0, y1 = 545, h - 15
    names = session.ocr.recognize(_whiteness(images.crop(img, [55, y0, 150, y1 - y0])), scale=2.5, pad=10)
    nums = session.ocr.recognize(img, region=[205, y0, 450, y1 - y0], scale=2.5)
    countries = []
    for l in sorted(names.lines, key=lambda l: l.box[1]):
        if not re.search(r"[A-Za-z]{3}", l.text):
            continue
        cy = l.center[1] + y0
        row = {"country": l.text}
        for o in nums.lines:
            if abs(o.center[1] - cy) >= 12:
                continue
            col = "export" if o.center[0] < 350 else "delivered" if o.center[0] < 520 else "convoys"
            row[col] = o.text
        countries.append(row)
    out["market"] = countries
    return out
