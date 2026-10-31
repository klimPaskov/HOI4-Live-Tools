"""The map: map modes, moving the camera to a state, zoom, and reading what is under a point.

Map modes are the buttons beside the minimap (interface/mapmodes_interface.gui, window MapmodesInterface_Ingame at
lower-right offset (-631, -244)), bound to F1..F10. Which mode sits on which button is the player's choice, so the
names are read from the buttons' tooltips once and cached (artifacts/state/map_modes.json).

The camera goes to a state through the game's own search (the Find view, key F): its name is pasted, the first
matching result is clicked, and the game centres and zooms the camera there. Mouse-wheel notches at the screen
centre then zoom out to the wanted scale.
"""

import json
import os
import re
import time

from ..desktop import clipboard
from ..errors import ToolError
from ..perception import tooltip
from . import ui

# (key, x, y, size) relative to the map mode window origin (w - 631, h - 244), gui_scale 1
BUTTONS = [("F1", 564, 48, 36), ("F2", 564, 95, 36), ("F3", 564, 143, 36), ("F4", 608, 42, 18),
           ("F5", 608, 63, 18), ("F6", 608, 86, 18), ("F7", 608, 107, 18), ("F8", 608, 129, 18),
           ("F9", 608, 151, 18), ("F10", 608, 173, 18)]
ZOOM = {"close": 0, "province": -3, "region": -7, "country": -11, "continent": -16, "world": -24}


def _cache_path(session):
    d = os.path.join(session.cfg.artifacts_dir, "state")
    os.makedirs(d, exist_ok=True)
    return os.path.join(d, "map_modes.json")


def _mode_title(session, x, y):
    d, _, _ = tooltip.read(session, x, y, wait=0.7)
    title = d.get("title") if d.get("found") else None
    if title and re.search(r"map\s*mode|mapmode", title, re.I):
        return re.sub(r"\s*map\s*mode\.?$|\s*mapmode\.?$", "", title, flags=re.I).strip()
    return None


def calibrate_modes(session, force=False):
    """{F-key: mode name} from the map mode buttons' tooltips (cached).

    The big buttons (F1-F3) move: vanilla shows three (y 48, 95, 143), a game with the strategic air mode shows four
    (y about 40, 76, 111, 149; the fourth, Operatives, has no key). The big column is scanned top to bottom and the
    first three distinct modes get F1-F3; keyless big buttons are kept with their click position ('extra')."""
    p = _cache_path(session)
    if not force:
        try:
            with open(p, encoding="utf-8") as f:
                data = json.load(f)
            if data.get("size") == list(session.window.size()) and data.get("version") == 3:
                return data["modes"]
        except (OSError, ValueError):
            pass
    w, h = session.window.size()
    ox, oy = w - 631, h - 244
    modes, extra, big, xy = {}, {}, [], {}
    for dy in range(44, 200, 12):                      # the big-button column, top to bottom
        x, y = ox + 564 + 18, oy + dy
        name = _mode_title(session, x, y)
        if name and (not big or big[-1][0] != name) and name not in [b[0] for b in big]:
            big.append((name, (x, y)))
    for i, (name, pos) in enumerate(big):
        if i < 3:
            modes[f"F{i + 1}"] = name
            xy[f"F{i + 1}"] = list(pos)
        else:
            extra[name] = list(pos)
    for key, x, y, size in BUTTONS[3:]:
        name = _mode_title(session, ox + x + size // 2, oy + y + size // 2)
        if name:
            modes[key] = name
            xy[key] = [ox + x + size // 2, oy + y + size // 2]
    with open(p, "w", encoding="utf-8") as f:
        json.dump({"size": [w, h], "modes": modes, "extra": extra, "xy": xy, "time": time.time(), "version": 3},
                  f, indent=1)
    return modes


def _extra_modes(session):
    try:
        with open(_cache_path(session), encoding="utf-8") as f:
            return json.load(f).get("extra") or {}
    except (OSError, ValueError):
        return {}


def resolve_mode(session, mode):
    """'F5', 'terrain', 'supply' ... -> F-key."""
    if re.match(r"^F\d{1,2}$", mode, re.I):
        return mode.upper()
    modes = calibrate_modes(session)
    for k, name in modes.items():
        if mode.lower() in name.lower():
            return k
    for name, xy in _extra_modes(session).items():
        if mode.lower() in name.lower():
            return "click:%d,%d" % tuple(xy)
    raise ToolError(f"no map mode matching '{mode}' (available: {modes})", {"modes": modes})


def _mode_xy(session):
    try:
        with open(_cache_path(session), encoding="utf-8") as f:
            return json.load(f).get("xy") or {}
    except (OSError, ValueError):
        return {}


def set_mode(session, mode, click=False):
    """Switch the map mode. The F-key goes to whatever holds the keyboard: with an army selected (its name box
    keeps the focus) the key did nothing in a live run, so click=True presses the mode's button instead."""
    key = resolve_mode(session, mode)
    pos = None
    if click and not key.startswith("click:"):
        pos = _mode_xy(session).get(key)
    if key.startswith("click:"):
        pos = [int(v) for v in key[6:].split(",")]
    if pos:
        session.input.click(pos[0], pos[1])
        session.park_cursor()
    else:
        session.input.key(key.lower())
    time.sleep(0.8)
    return key


def _find_region(session):
    """The Find view sits above the minimap at the right edge (about 265 x 290 at gui_scale 1)."""
    w, h = session.window.size()
    return [max(0, w - 300), max(0, h - 560), 300, 340]


def _ocr_find(session):
    """OCR of the Find view; Windows OCR sometimes returns nothing at one scale, so try a few."""
    res = None
    for scale in (1.0, 1.5, 2.0):
        img, res = ui.ocr_screen(session, region=_find_region(session), scale=scale)
        if len(res.lines) >= 2:
            return res
    return res


def _find_open(session):
    res = _ocr_find(session)
    # its two buttons, "Find" and "Go Home" (OCR also reads "GO Horne", "Fin")
    return any(re.match(r"^\W*(go\s*ho\w*|fin\w?)\W*$", l.text, re.I) for l in res.lines), res


def _close_find(session):
    """Esc closes the Find view; if it opened the pause menu instead (nothing was open), close that again."""
    from . import menu
    session.input.key("esc")
    time.sleep(0.6)
    if menu.esc_menu_open(session)[0]:
        session.input.key("esc")
        time.sleep(0.6)


def _fold(text):
    """Lower case, accents and punctuation dropped ('Nuevo León' == 'nuevoleon')."""
    import unicodedata
    t = unicodedata.normalize("NFKD", text or "")
    return re.sub(r"\W+", "", "".join(c for c in t if not unicodedata.combining(c)).lower())


def _press_find(session):
    """Open the Find view: Ctrl+F (HOI4 1.19.4+), then F (1.19.3 and older) if it did not open.

    Untested on Windows 1.19.4; each chord is pressed once and verified by OCR before the next.
    """
    opened, res = False, None
    for combo in ("ctrl+f", "f"):
        session.input.key(combo)
        for _ in range(4):
            time.sleep(0.6)
            opened, res = _find_open(session)
            if opened:
                return opened, res
    return opened, res


def _find_hits(session, name):
    """Open the Find view, paste name, return (exact or near hits top to bottom, OCR result)."""
    want = _fold(name)
    session.park_cursor()
    opened, res = _find_open(session)
    if not opened:
        opened, res = _press_find(session)
    if not opened:
        return None, res
    with clipboard.Preserved():
        session.input.key("end")
        session.input.key("backspace", times=40, delay=0.01)
        clipboard.set_text(name)
        time.sleep(0.05)
        session.input.key("ctrl+v")
    time.sleep(1.2)
    res = _ocr_find(session)
    # the top line is the search field itself; results are listed below it
    field_y = min((l.box[1] for l in res.lines), default=0)
    hits = sorted((l for l in res.lines if _fold(l.text) == want and l.box[1] > field_y + 8), key=lambda l: l.box[1])
    if not hits:                          # OCR noise in a result ("Nuevo Le6n" for Nuevo León)
        import difflib
        hits = sorted((l for l in res.lines if l.box[1] > field_y + 8 and len(_fold(l.text)) >= 4 and
                       difflib.SequenceMatcher(None, _fold(l.text), want).ratio() >= 0.8), key=lambda l: l.box[1])
    return hits, res


def go_to(session, name, attempts=2, check=None):
    """Centre the camera on a state or province by name through the Find view. Returns the clicked result.

    Several places can share a name (Georgia the US state and Georgia the country's state): with check, a callable
    that looks at the camera's new position and returns True when it is the wanted place, the results are tried in
    turn until check accepts one."""
    last = None
    tried = 0
    for _ in range(attempts + 3):
        hits, res = _find_hits(session, name)
        if hits is None:
            last = {"seen": [l.text for l in res.lines][:20], "note": "the Find view did not open"}
            continue
        if not hits:
            last = {"seen": [l.text for l in res.lines][:20]}
            if tried >= attempts:
                break
            tried += 1
            continue
        index = min(tried, len(hits) - 1) if check else 0
        target = hits[index]
        session.input.click(*target.center)
        time.sleep(1.5)
        session.park_cursor()
        if _find_open(session)[0]:            # the Find view closes on a result click; close it if it stayed
            _close_find(session)
        out = {"name": name, "clicked": target.text, "result": index + 1, "results": len(hits)}
        if check is None:
            return out
        verdict = check()
        out["check"] = verdict
        if verdict:
            return out
        tried += 1
        last = {"note": f"result {index + 1} of {len(hits)} was not the wanted place", "check": verdict}
        if tried >= len(hits):
            break
    if _find_open(session)[0]:
        _close_find(session)
    raise ToolError(f"the Find view has no (matching) result named '{name}'", last)


class _Point:
    def __init__(self, center, text):
        self.center, self.text = center, text


def go_to_id(session, sid, expect=None):
    """Centre the camera on a state by its id through the Find view. The view matches numbers as well as names and
    lists the state with exactly that id first (checked live: 1 Corsica, 9 Bohemia, 11 Kaunas, 64 Brandenburg, 100
    Iceland), so nothing is read as text except to find where the first result sits. expect: the state's localised
    name; when the first result's text is readable it must resemble it, else the next results are tried."""
    import difflib
    sid = int(sid)
    last = None
    for attempt in range(4):
        session.park_cursor()
        opened, res = _find_open(session)
        if not opened:
            opened, res = _press_find(session)
        if not opened:
            last = {"seen": [l.text for l in res.lines][:20], "note": "the Find view did not open"}
            continue
        with clipboard.Preserved():
            session.input.key("end")
            session.input.key("backspace", times=40, delay=0.01)
            clipboard.set_text(str(sid))
            time.sleep(0.05)
            session.input.key("ctrl+v")
        time.sleep(1.0)
        # the first result row sits at a fixed place below the search field (OCR of that row is unreliable: the
        # field's own text and the row merge); it is the state with exactly this id (the list is ordered by id)
        w, h = session.window.size()
        pick = _Point((w - 150, h - 465), "first result")
        session.input.click(*pick.center)
        time.sleep(1.2)
        session.park_cursor()
        if _find_open(session)[0]:
            _close_find(session)
        return {"id": sid, "name": expect or str(sid), "clicked": "first result", "by": "id"}
    if _find_open(session)[0]:
        _close_find(session)
    raise ToolError(f"the Find view has no result for state id {sid}", last)


def zoom(session, level):
    """Mouse wheel at the screen centre: a named level (relative to the close view the Find view leaves) or
    notches (negative = out)."""
    notches = ZOOM.get(level, level) if isinstance(level, str) else level
    if not notches:
        return 0
    w, h = session.window.size()
    session.input.scroll(w // 2, h // 2, int(notches))
    time.sleep(0.8)
    session.park_cursor()
    return int(notches)


def zoom_abs(session, level):
    """Zoom to a named level regardless of where the camera was: scroll all the way in first (the notches past the
    closest view do nothing), then out by the level's notches."""
    w, h = session.window.size()
    session.input.scroll(w // 2, h // 2, 30)
    time.sleep(0.5)
    return zoom(session, level)


# ---------------------------------------------------------------------------------------------- the minimap
MINIMAP_BOX = (1572, 976, 1850, 1078)     # search box for the camera rectangle at 1080p (client pixels)


def camera_on_minimap(session, img=None):
    """Centre (x, y) in client pixels of the red camera rectangle on the minimap, or None."""
    import numpy as np
    img = session.grab() if img is None else img
    h, w = img.shape[:2]
    x0, y0, x1, y1 = MINIMAP_BOX
    sx, sy = w / 1920.0, h / 1080.0
    crop = img[int(y0 * sy):int(y1 * sy), int(x0 * sx):int(x1 * sx)].astype(np.int16)
    m = (crop[..., 0] > 190) & (crop[..., 1] < 70) & (crop[..., 2] < 70)
    ys, xs = np.nonzero(m)
    if len(xs) < 12:
        return None
    return (x0 * sx + float(xs.mean()), y0 * sy + float(ys.mean()))


def _minimap_cache(session):
    d = os.path.join(session.cfg.artifacts_dir, "state")
    os.makedirs(d, exist_ok=True)
    return os.path.join(d, "minimap.json")


def calibrate_minimap(session, idx, force=False, probes=(100, 11, 1, 454, 64)):
    """Fit minimap pixel = a * map pixel + b for both axes from the camera rectangle seen after jumping to a few
    states by id (Iceland, Kaunas, Corsica, Palestine, Brandenburg by default: spread over the map). Cached."""
    path = _minimap_cache(session)
    if not force:
        try:
            with open(path, encoding="utf-8") as f:
                data = json.load(f)
            if data.get("size") == list(session.window.size()):
                return data
        except (OSError, ValueError):
            pass
    pts = []
    for sid in probes:
        st = idx["states"].get(str(sid))
        if not st:
            continue
        go_to_id(session, sid)
        time.sleep(0.8)
        c = camera_on_minimap(session)
        if c:
            pts.append((st["anchor"][0], st["anchor"][1], c[0], c[1]))
    if len(pts) < 3:
        raise ToolError("the minimap's camera rectangle was not found (is the default map mode showing?)",
                        {"points": pts})
    import numpy as np
    A = np.array([[p[0], 1.0] for p in pts])
    ax, bx = np.linalg.lstsq(A, np.array([p[2] for p in pts]), rcond=None)[0]
    B = np.array([[p[1], 1.0] for p in pts])
    ay, by = np.linalg.lstsq(B, np.array([p[3] for p in pts]), rcond=None)[0]
    resid = max(abs(ax * p[0] + bx - p[2]) for p in pts), max(abs(ay * p[1] + by - p[3]) for p in pts)
    data = {"size": list(session.window.size()), "ax": float(ax), "bx": float(bx), "ay": float(ay), "by": float(by),
            "points": pts, "residual_px": [round(float(r), 2) for r in resid], "time": time.time()}
    with open(path, "w", encoding="utf-8") as f:
        json.dump(data, f, indent=1)
    return data


def go_to_point(session, idx, px, py, calibration=None):
    """Centre the camera on a map pixel (x east, y south, as in provinces.bmp) by clicking the minimap: used for
    places the Find view does not know (sea regions). The click lands within about 20 map pixels."""
    cal = calibration or calibrate_minimap(session, idx)
    x = cal["ax"] * px + cal["bx"]
    y = cal["ay"] * py + cal["by"]
    session.input.click(int(round(x)), int(round(y)))
    time.sleep(0.9)
    session.park_cursor()
    return {"x": px, "y": py, "minimap": [round(x), round(y)]}


def at_point(session, x, y):
    """The tooltip of the map at (x, y): province / state name and what the map mode shows there."""
    d, crop, img = tooltip.read(session, x, y, wait=0.8)
    return d, crop
