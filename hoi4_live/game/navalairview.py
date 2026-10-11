"""Fleets and air wings (phase 4, basic): select one, give it a mission, assign a strategic region.

Vanilla 1.19 at gui_scale 1 (checked at 1920x1080):

- The Strategic Navy map mode (F2 here; named from the map mode buttons' tooltips) shows the fleet cards along the
  bottom (an empty "+" card first). Clicking a card selects the fleet: its panel (top left) shows the fleet's name,
  "Active in Regions: n" and one block per task force whose status line reads "Docked in <state>", "No assigned
  regions" or "Moving to assigned region <region>"; the missions bar ("Missions - All Taskforces") sits at the bottom
  centre: Patrol, Strike Force, Convoy Raiding, Naval Exercises, Convoy Escort, Minelaying, Minesweeping, Naval
  Invasion Support, targeting options, Hold, Remove Region. A mission button sets the fleet's mission; a RIGHT click
  on a sea region assigns that region (a left click opens the region's view and drops the selection; its title is
  the region's name).
- The Strategic Air map mode (F3) shows air wings as counters at their air bases; a click on a counter selects the
  base's wings: the panel lists them ("1. Medium Bombardment Group ... Standing by"), the mission buttons are the
  top row of the panel (Pilot Exercises, Air Superiority, Close Air Support, Interception, Strategic Bombing, ...,
  Naval Strike, Kamikaze, Port Strike, Logistics Strike, Air Supply, minelaying, minesweeping, Air Recon, Naval
  Patrol); after a mission button a right click on a region assigns it and each wing's line shows the region.
Each mission button is identified by its tooltip before it is clicked.
"""

import re
import time

import numpy as np

from ..errors import ToolError
from ..perception import tooltip
from . import panels

NAVAL_MISSIONS = {"patrol": r"^patrol", "strike_force": r"strike\s*force", "convoy_raiding": r"convoy\s*raid",
                  "naval_exercises": r"naval\s*exercise", "convoy_escort": r"convoy\s*escort",
                  "minelaying": r"minelaying", "minesweeping": r"minesweeping",
                  "naval_invasion_support": r"invasion\s*support", "hold": r"^hold"}
# vanilla x offsets from the screen centre and y from the bottom (missions bar, bottom centre)
NAVAL_BUTTONS = {"patrol": (-264, -180), "strike_force": (-216, -180), "convoy_raiding": (-166, -180),
                 "naval_exercises": (-90, -180), "convoy_escort": (-41, -180), "minelaying": (10, -180),
                 "minesweeping": (60, -180), "naval_invasion_support": (108, -180), "hold": (180, -180),
                 "remove_region": (210, -180)}
AIR_MISSIONS = {"pilot_exercises": r"pilot\s*exercise", "air_superiority": r"air\s*superiority",
                "cas": r"close\s*air\s*support", "interception": r"interception",
                "strategic_bombing": r"strategic\s*bombing", "naval_strike": r"naval\s*strike",
                "kamikaze": r"kamikaze", "port_strike": r"port\s*strike", "logistics_strike": r"logistics\s*strike",
                "air_supply": r"air\s*supply", "naval_minelaying": r"naval\s*minelaying",
                "naval_minesweeping": r"naval\s*minesweeping", "recon": r"air\s*recon",
                "naval_patrol": r"naval\s*patrol"}
AIR_BUTTONS = {"pilot_exercises": 42, "air_superiority": 84, "cas": 130, "interception": 174,
               "strategic_bombing": 218, "naval_strike": 308, "kamikaze": 356, "port_strike": 404,
               "logistics_strike": 446, "air_supply": 492, "naval_minelaying": 536, "naval_minesweeping": 581,
               "recon": 624, "naval_patrol": 670}
AIR_BUTTON_Y = 106


def _button_ok(session, xy, rx):
    d, _, _ = tooltip.read(session, xy[0], xy[1], wait=0.8)
    lines = d.get("lines") or []
    return any(re.search(rx, l.strip(), re.I) for l in lines[:3]), lines[:3]


def wait_for_text(session, rx, region, seconds=10.0, scale=1.5):
    """Poll the screen until OCR finds rx in region (a bar that slides in after a selection), True or False."""
    t0 = time.time()
    while time.time() - t0 < seconds:
        res = session.ocr.recognize(session.grab(), region=region, scale=scale)
        if any(re.search(rx, l.text, re.I) for l in res.lines):
            return True
        time.sleep(0.8)
    return False


def find_button(session, xy, rx, search=(0,), axis=0):
    """The mission button near xy whose tooltip matches rx (tries small shifts for other layouts)."""
    tried = []
    for off in search:
        p = (xy[0] + off, xy[1]) if axis == 0 else (xy[0], xy[1] + off)
        ok, lines = _button_ok(session, p, rx)
        tried.append(lines)
        if ok:
            return p
    raise ToolError(f"mission button not found near {xy} (tooltips: {tried})")


# ---------------------------------------------------------------------------------------------- fleets
def fleet_panel(session, img=None):
    """{'name', 'active_regions', 'task_forces': [{'name', 'status'}]} of the selected fleet's panel."""
    img = session.grab() if img is None else img
    res = session.ocr.recognize(img, region=[0, 84, 520, 820], scale=1.5)
    lines = sorted(res.lines, key=lambda l: (l.box[1], l.box[0]))
    out = {"name": None, "active_regions": None, "task_forces": [], "lines": [l.text for l in lines][:60]}
    head = [l for l in lines if l.box[1] < 112 and 40 < l.box[0] < 300]
    if head:
        out["name"] = head[0].text.strip()
    act = next((l for l in lines if re.search(r"active\s*in\s*regions", l.text, re.I)), None)
    if act:
        m = re.search(r"regions\W*([0-9OoIl]+)", act.text, re.I)
        if m:
            out["active_regions"] = int(m.group(1).replace("O", "0").replace("o", "0").replace("I", "1")
                                        .replace("l", "1"))
    selects = [l for l in lines if re.match(r"^\W*select\W*$", l.text, re.I)]
    for s in selects:
        nm = [l for l in lines if abs(l.center[1] - s.center[1]) < 9 and l.box[0] < s.box[0] - 60]
        status = [l for l in lines if 20 <= l.box[1] - s.box[1] <= 70 and l.box[0] < 330 and
                  re.search(r"[a-z]{3}", l.text, re.I)]
        out["task_forces"].append({"name": " ".join(l.text for l in nm).strip(),
                                   "status": " ".join(l.text for l in status[:2]).strip()})
    return out


CARD_PITCH = 80          # centre to centre at gui_scale 1
PLUS_TO_FIRST = 16       # gap between the "+" card's dark bottom (75 px) and the first card's (54 px)


def _dark_runs(img, dy):
    h, w = img.shape[:2]
    row = img[h - dy:h - dy + 3, :, :].astype(np.int16).max(2).min(0)
    dark = row < 40
    out, start = [], None
    for x in range(w + 1):
        d = x < w and dark[x]
        if d and start is None:
            start = x
        elif not d and start is not None:
            out.append((start, x - start))
            start = None
    return out


def cards_from_runs(rows, pitch=CARD_PITCH):
    """Card centres from the dark runs of several pixel rows along the bottom. Each card has a dark count box
    (about 55 px wide); the "+" card's is wider (75 px). The cards sit on a regular grid right of the "+" card, so a
    card whose box a highlight or a digit broke up in the sampled rows is still returned (seen live: the first two
    fleet cards vanished while a sea region was selected). Cards off the grid (the theatre card at the end) are kept
    as detected."""
    found, plus = [], None
    for runs in rows:
        for start, width in runs:
            if 40 <= width <= 66:
                found.append(start + width / 2)
            elif 67 <= width <= 90 and plus is None:
                plus = (start, width)
    merged = []
    for c in sorted(found):
        if merged and c - merged[-1] < 14:
            continue
        merged.append(c)
    if not plus or not merged:
        return [int(c) for c in merged]
    first = plus[0] + plus[1] + PLUS_TO_FIRST + 27
    on_grid = [c for c in merged if c >= first - 14 and abs((c - first) / pitch - round((c - first) / pitch)) < 0.15]
    if not on_grid:
        return [int(c) for c in merged]
    n = int(round((max(on_grid) - first) / pitch)) + 1
    slots = [first + pitch * i for i in range(n)]
    off = [c for c in merged if all(abs(c - s) > 14 for s in slots)]
    return [int(c) for c in sorted(slots + off)]


def card_strip(session, img=None):
    """Card centres along the bottom (fleet cards in the navy map mode), found from their frames."""
    img = session.grab() if img is None else img
    return cards_from_runs([_dark_runs(img, dy) for dy in (22, 21, 20, 19)])


def _click_card_and_read(session, cx, name=None, polls=8):
    """Click a fleet card and poll its panel (it slides in: seconds on a busy machine) until the title is `name`
    (or, with no name, until a fleet panel shows). Returns the panel."""
    w, h = session.window.size()
    session.input.click(cx, h - 65)
    session.park_cursor()
    p = {}
    for _ in range(polls):
        time.sleep(0.8)
        p = fleet_panel(session)
        if p.get("name") and (name is None and p.get("active_regions") is not None or
                              name is not None and panels.similar(p["name"], name) >= 0.8):
            break
    return p


def find_fleet(session, name):
    """Select a fleet by name. First hover each card for its tooltip (the last line is the fleet's name) and click the
    match; the tooltip is OCR'd over a busy map and sometimes loses its last line, so when no tooltip names the fleet
    every card is clicked in turn and the panel's title is read instead."""
    w, h = session.window.size()
    seen = []
    # the cursor rests on the sea region just probed and its tooltip (a 340 x 250 box) covers the first fleet cards:
    # move it away and let the tooltip go before looking at the strip
    session.park_cursor()
    time.sleep(1.2)
    first_img = session.grab()
    cards = card_strip(session, first_img)
    strip_info = {"cards": cards, "runs": [[r for r in _dark_runs(first_img, dy) if r[0] > 400 and 20 <= r[1] <= 120]
                                          for dy in (22, 21, 20)]}
    for cx in cards:
        d, _, _ = tooltip.read(session, cx, h - 65, wait=0.8)
        lines = [l for l in (d.get("lines") or []) if len(l) > 2]
        seen.append(lines[:4])
        if any(panels.similar(l, name) >= 0.86 for l in lines):
            p = _click_card_and_read(session, cx, name)
            if p.get("name") and panels.similar(p["name"], name) >= 0.8:
                return cx, p
            raise ToolError(f"the fleet card '{name}' was clicked but the panel's title reads '{p.get('name')}'",
                            {"panel": p, "tooltip": lines})
    titles = []
    for cx in cards[:-1]:                    # the last card is the theatre's reserve
        p = _click_card_and_read(session, cx, None, polls=4)
        titles.append(p.get("name"))
        if p.get("name") and panels.similar(p["name"], name) >= 0.8:
            return cx, p
    try:
        strip_info["screenshot_at_start"] = session.artifact(first_img, "fleet_strip_at_start")
    except Exception:
        pass
    raise ToolError(f"no fleet card named '{name}' (tooltips: {seen}; panel titles: {titles})",
                    {"cards": seen, "titles": titles, "strip": strip_info})


def region_title(session, img=None):
    """Title of an open strategic region view (bottom left), e.g. 'WESTERN SEABOARD', or None."""
    img = session.grab() if img is None else img
    h, w = img.shape[:2]
    res = session.ocr.recognize(img, region=[0, h - 300, 460, 36], scale=1.5)
    t = " ".join(l.text for l in res.lines).strip()
    return t or None


def close_region_view(session):
    w, h = session.window.size()
    session.input.click(413, h - 285)
    time.sleep(0.5)
    session.park_cursor()


def sea_points(img, step=110, margin=(120, 140)):
    """Candidate sea points on screen (dark blue, away from the UI)."""
    h, w = img.shape[:2]
    pts = []
    for y in range(margin[0], h - 260, step):
        for x in range(margin[1], w - 460, step):
            if x < 470:
                continue                       # the unit panel (top) and the region view (bottom left)
            p = img[y - 3:y + 4, x - 3:x + 4].astype(np.int16).reshape(-1, 3).mean(0)
            r, g, b = p
            if b > r + 30 and b >= g:
                pts.append((x, y))
    return pts


def sea_candidates(session, radii=(0, 150, 300, 450), steps=8, limit=8):
    """Points around the screen centre whose map tooltip describes a sea zone ('Naval Dominance', 'seazone'). In
    the navy map mode land and sea are both blue, so colour cannot tell them apart; hovering does not select
    anything (a click on an air base would switch to the air map mode)."""
    import math
    w, h = session.window.size()
    cx, cy = w // 2, h // 2
    found, tried = [], 0
    for r in radii:
        for k in range(steps if r else 1):
            ang = 2 * math.pi * k / steps + (math.pi / steps if r % 300 else 0)
            p = (int(cx + r * math.cos(ang)), int(cy + r * 0.75 * math.sin(ang)))
            if not (470 < p[0] < w - 470 and 120 < p[1] < h - 260):
                continue
            d, _, _ = tooltip.read(session, p[0], p[1], wait=0.5)
            tried += 1
            txt = " ".join(d.get("lines") or [])
            if re.search(r"sea\s*zone|seazone|naval\s*dominance", txt, re.I):
                found.append(p)
                if len(found) >= limit:
                    return found, tried
    return found, tried


def probe_regions(session, points, want=None, limit=14):
    """Left-click points and read the region view's title. {title: point}; stops at a title matching want."""
    found = {}
    for p in points[:limit]:
        session.input.click(*p)
        time.sleep(0.8)
        t = region_title(session)
        if t:
            t = re.sub(r"\s+", " ", t).strip()
            found.setdefault(t, p)
            close_region_view(session)
            if want and panels.similar(t, want) >= 0.85:
                break
    return found


# ---------------------------------------------------------------------------------------------- air wings
def air_panel(session, img=None):
    """{'base', 'selected', 'wings': [{'name', 'status'}]} of the selected air wings' panel."""
    img = session.grab() if img is None else img
    res = session.ocr.recognize(img, region=[0, 140, 520, 760], scale=1.5)
    lines = sorted(res.lines, key=lambda l: (l.box[1], l.box[0]))
    out = {"base": None, "selected": None, "wings": [], "lines": [l.text for l in lines][:60]}
    sel = next((l for l in lines if re.search(r"(\d+)\s*selected", l.text, re.I)), None)
    if sel:
        out["selected"] = int(re.search(r"(\d+)\s*selected", sel.text, re.I).group(1))
    base = next((l for l in lines if 200 < l.box[1] < 240 and 60 < l.box[0] < 200), None)
    if base:
        out["base"] = base.text.strip()
    # one row per wing, 48 px apart from y ~265 (gui_scale 1): the wing's name, then its region or "Standing by";
    # each row is read on its own (a crop of the whole panel loses lines)
    for k in range(10):
        y = 257 + 48 * k
        res2 = session.ocr.recognize(img, region=[236, y, 170, 34], scale=2.0)
        rows = [l.text.strip() for l in sorted(res2.lines, key=lambda l: l.box[1]) if re.search(r"[A-Za-z0-9]", l.text)]
        if not rows:
            break
        out["wings"].append({"name": rows[0], "status": rows[1] if len(rows) > 1 else None})
    return out
