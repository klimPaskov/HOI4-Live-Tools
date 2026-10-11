"""Armies on the map (phase 4): army cards, the army and selection panels, commanders, battle plans (front lines,
offensive lines, execute and hold), selecting divisions on the map.

Vanilla 1.19 layout at gui_scale 1 (checked at 1920x1080):

- Army cards sit along the bottom centre (interface/leadergroups.gui). An army card shows its general's portrait and
  a "divisions/capacity" box under it ("4/24"); the execute (green arrow) and stop (red square) buttons sit above it;
  an animated overlay plays on the execute button while the plan is active (GFX_active_battleplan_anim), which is how
  "executing" is read from the screen. The empty "+" card to the right of the last army creates an army from the
  selected divisions ("New Army"); the "+" card to the left of the first creates an army group from selected armies.
  Right-clicking an army card assigns the selected divisions to it.
- With an army selected, its panel (top left) shows the name (an edit box), the commander ("No Commander / Click to
  assign" opens "Select Commander for <army>" with a Search field), "n/m" divisions and the division list; the
  battle plan toolbar (interface/battleplantools.gui, window battleplantools_window) slides up at the bottom centre.
  Its buttons: naval invasion, ... front line (Z), offensive line (X), spearhead (Shift+X), fallback (C), area
  defence (V), assign, unassign, edit, delete (Del). The toolbar title reads "Battle Plans" until the army has an
  order, then the operation's name ("Operation Dark Star").
- Front line: the button, then a click on another country's territory sets the front along the whole border with it.
  Offensive line: the button, then a right-button drag paints the line in enemy territory; the game adds the arrow
  from the front to it. Both are drawn in a red that nothing else on the map uses in long thin strokes.
- Divisions are selected by clicking a counter or dragging a box with the left button on the map; the selection
  panel lists them ("4 Divisions" and the names; "These Divisions are unassigned ..." when they belong to no army).
"""

import math
import os
import re
import time

import cv2
import numpy as np

from ..desktop import clipboard
from ..errors import ToolError
from ..perception import tooltip
from . import panels

COUNT_RX = re.compile(r"^\W*(\d{1,3})\s*[/|Il1\s]\s*(\d{1,3})\W*$")
SEL_RX = re.compile(r"(\d+)\s*(divisions?|selected)", re.I)

# battle plan toolbar buttons (name in battleplantools.gui -> our name), with vanilla fallbacks
BPT_BUTTONS = {"btn_order_draw_naval_inv": "naval_invasion", "btn_order_draw_naval_inv_with_harbor": "naval_invasion_harbor",
               "btn_order_draw_paradrop": "paratroop", "btn_order_draw_defence": "front_line",
               "btn_order_draw_offence": "offensive", "btn_order_draw_blitz": "spearhead",
               "btn_order_draw_fallback": "fallback", "btn_area_defense": "area_defense",
               "btn_assign_units": "assign", "btn_unassign_units": "unassign", "btn_order_reshape": "edit",
               "btn_order_delete": "delete"}
BPT_TOOLTIPS = {"naval_invasion": r"naval\s*invasion", "paratroop": r"paratroop", "front_line": r"front\s*line",
                "offensive": r"offensive\s*line", "spearhead": r"spearhead", "fallback": r"fallback",
                "area_defense": r"area\s*defen", "assign": r"assignment", "unassign": r"unassign",
                "edit": r"edit\s*mode", "delete": r"delete\s*order"}
_VANILLA_BPT = {"window": (-270, -245), "buttons": {
    "naval_invasion": (124, 33), "naval_invasion_harbor": (158, 33), "paratroop": (194, 33), "front_line": (239, 33),
    "offensive": (276, 33), "spearhead": (311, 33), "fallback": (355, 33), "area_defense": (391, 33),
    "assign": (437, 33), "unassign": (472, 33), "edit": (517, 32), "delete": (563, 33)}}
BUTTON_HALF = 15
_BPT_CACHE = {}


def _bpt_layout(session):
    """Toolbar window offset and button positions from the effective battleplantools.gui (a mod may move them)."""
    key = session.cfg.game_dir
    if key in _BPT_CACHE:
        return _BPT_CACHE[key]
    layout = {"window": _VANILLA_BPT["window"], "buttons": dict(_VANILLA_BPT["buttons"]), "source": "vanilla constants"}
    try:
        from ..gamedata import scriptdb
        files = [p for p in scriptdb.effective_files("interface", session.cfg.game_dir, scriptdb.mods_for(session),
                                                      ext=(".gui",)) if os.path.basename(p).lower() == "battleplantools.gui"]
        if files:
            with open(files[-1], encoding="utf-8-sig", errors="replace") as f:
                text = f.read()
            m = re.search(r'name\s*=\s*"battleplantools_window".*?show_position\s*=\s*\{\s*x\s*=\s*(-?\d+)\s*y\s*=\s*(-?\d+)',
                          text, re.S)
            m0 = re.search(r'name\s*=\s*"battleplantools_window"\s*position\s*=\s*\{\s*x\s*=\s*(-?\d+)', text, re.S)
            if m and m0:
                layout["window"] = (int(m0.group(1)), int(m.group(2)))
            for gui_name, ours in BPT_BUTTONS.items():
                b = re.search(r'name\s*=\s*"%s"\s*position\s*=\s*\{\s*x\s*=\s*(-?\d+)\s*y\s*=\s*(-?\d+)' % gui_name, text)
                if b:
                    layout["buttons"][ours] = (int(b.group(1)), int(b.group(2)))
            layout["source"] = files[-1]
    except (OSError, ImportError):
        pass
    _BPT_CACHE[key] = layout
    return layout


def toolbar_button(session, name):
    """Client (x, y) of a battle plan toolbar button."""
    lay = _bpt_layout(session)
    w, h = session.window.size()
    wx, wy = lay["window"]
    bx, by = lay["buttons"][name]
    return (w // 2 + wx + bx + BUTTON_HALF, h + wy + by + BUTTON_HALF)


def toolbar_title(session, img=None):
    """The toolbar's title ('Battle Plans' or 'Operation <name>'), or None when the toolbar is not shown."""
    img = session.grab() if img is None else img
    h, w = img.shape[:2]
    lay = _bpt_layout(session)
    x0 = w // 2 + lay["window"][0] - 40
    y0 = h + lay["window"][1] - 2
    res = session.ocr.recognize(img, region=[max(0, x0), max(0, y0), 520, 30], scale=2.0)
    texts = [l.text for l in sorted(res.lines, key=lambda l: l.box[0])]
    t = " ".join(texts).strip()
    return t or None


def press_toolbar(session, name, verify=True):
    """Click a toolbar button after checking its tooltip names it (a moved toolbar fails loudly, not silently)."""
    x, y = toolbar_button(session, name)
    if verify and name in BPT_TOOLTIPS:
        d, _, _ = tooltip.read(session, x, y, wait=0.8)
        title = " ".join(d.get("lines") or [])[:200]
        if not re.search(BPT_TOOLTIPS[name], title, re.I):
            raise ToolError(f"the battle plan toolbar's {name} button was not found at ({x}, {y}) (tooltip: "
                            f"{title[:80]!r}); is an army selected?", {"tooltip": d.get("lines")})
    session.input.click(x, y)
    time.sleep(0.5)
    return (x, y)


# ---------------------------------------------------------------------------------------------- army cards
def cards(session, img=None):
    """Army cards along the bottom: [{x, divisions, capacity}] from their "n/m" boxes, left to right."""
    return card_row(session, img)["armies"]


def card_boxes(img):
    """Centres of the cards' dark count boxes along the bottom edge (pure image analysis)."""
    h, w = img.shape[:2]
    row = img[h - 25:h - 22, :, :].astype(np.int16).max(2).min(0)
    dark = row < 40
    boxes, start = [], None
    for x in range(w + 1):
        d = x < w and dark[x]
        if d and start is None:
            start = x
        elif not d and start is not None:
            if 55 <= x - start <= 90 and w * 0.12 < x < w * 0.88:
                boxes.append(int(start + (x - start) / 2))
            start = None
    return boxes


def card_row(session, img=None):
    """{'armies': [{x, divisions, capacity}], 'others': [x ...]}: every card along the bottom has a dark box
    (about 70 px wide) 24 px above the bottom edge; an army card's box reads "n/m" (OCR of that box alone at 3-4x,
    which is reliable where a wide strip is not), the "+" cards' do not."""
    img = session.grab() if img is None else img
    h, w = img.shape[:2]
    boxes = card_boxes(img)
    # the row is [army group "+", armies ..., new army "+"]: the outer two are the "+" cards
    if len(boxes) < 2:
        return {"armies": [], "others": boxes}
    others = [boxes[0], boxes[-1]]
    armies = []
    from ..perception import glyphs
    fnt = glyphs.font(session.cfg.game_dir, "hoi_18mbs")      # the count box's font (leadergroups.gui)
    for cx in boxes[1:-1]:
        got = None
        for scale in (3.0, 4.0):
            res = session.ocr.recognize(img, region=[cx - 36, h - 28, 72, 24], scale=scale)
            for text, _ in _word_groups([wd for l in res.lines for wd in (l.words or [])]):
                m = re.search(r"(\d{1,3})\s*[/|Il1 ]\s*(\d{1,3})", text)
                if m:
                    got = {"x": cx, "divisions": int(m.group(1)), "capacity": int(m.group(2))}
                    break
            if got:
                break
        if not got and fnt is not None:
            # an army without a general shows a lone number ("2"), which OCR misses: the game's own font
            g = glyphs.read(img[h - 30:h - 6, cx - 34:cx + 34], fnt, charset="0123456789")
            if re.match(r"^\d{1,3}$", g or ""):
                got = {"x": cx, "divisions": int(g), "capacity": None}
        if got:
            armies.append(got)
        else:
            others.append(cx)                 # dark map terrain can look like a count box: no number, no army
    return {"armies": armies, "others": sorted(others)}


def find_card_by_tooltip(session, pattern, exclude=None, xs=None):
    """x of the bottom card whose tooltip matches pattern (and not exclude), scanning the card row."""
    from ..perception import tooltip as tip_mod
    w, h = session.window.size()
    xs = xs or list(range(w // 2 - 360, w // 2 + 480, 40))
    seen = []
    for x in xs:
        d, _, _ = tip_mod.read(session, x, h - 70, wait=0.7)
        t = " ".join(d.get("lines") or [])
        seen.append((x, t[:40]))
        if re.search(pattern, t, re.I) and not (exclude and re.search(exclude, t, re.I)):
            return x, seen
    return None, seen


def _word_groups(words, gap=24):
    """[(text, box)] of OCR words joined while they are closer than gap pixels."""
    out = []
    for wd in sorted(words, key=lambda w: w["box"][0]):
        x, y, bw, bh = wd["box"]
        if out and x - (out[-1][1][0] + out[-1][1][2]) < gap:
            t, (ox, oy, ow, oh) = out[-1]
            x1 = max(ox + ow, x + bw)
            out[-1] = (t + " " + wd["text"], [ox, min(oy, y), x1 - ox, max(oh, bh)])
        else:
            out.append((wd["text"], [x, y, bw, bh]))
    return out


def card_points(session, cx):
    w, h = session.window.size()
    return {"portrait": (cx, h - 70), "execute": (cx + 18, h - 122), "stop": (cx - 21, h - 122)}


def plus_cards(session, armies, others=None):
    """(army group '+' card x, new army '+' card x): the cards without a count left of the first army and right
    of the last (when no army exists, the two '+' cards in order)."""
    w, h = session.window.size()
    if others is None:
        others = card_row(session)["others"]
    if not armies:
        if len(others) >= 2:
            return others[0], others[1]
        return w // 2 - 52, w // 2 + 28
    left = [x for x in others if x < armies[0]["x"]]
    right = [x for x in others if x > armies[-1]["x"]]
    return (left[-1] if left else armies[0]["x"] - 83), (right[0] if right else armies[-1]["x"] + 83)


def plan_active(session, cx, frames=7):
    """True while the army's plan executes: the execute button's overlay animates (even while paused)."""
    x, y = card_points(session, cx)["execute"]
    region = (slice(max(0, y - 16), y + 16), slice(max(0, x - 40), x + 40))
    session.park_cursor()
    time.sleep(0.3)
    shots = []
    for _ in range(frames):
        shots.append(session.grab()[region].astype(np.int16))
        time.sleep(0.15)
    diffs = [float(np.abs(s - shots[0]).max(2).mean()) for s in shots[1:]]
    return float(np.mean(diffs)) > 1.2, round(float(np.mean(diffs)), 2)


# ---------------------------------------------------------------------------------------------- panels
def panel(session, img=None):
    """The top-left unit panel: {'kind': 'army'|'selection'|None, 'name', 'commander', 'divisions', 'capacity',
    'selected', 'rows': [division names], 'unassigned': bool}."""
    img = session.grab() if img is None else img
    res = session.ocr.recognize(img, region=[0, 84, 440, 440], scale=1.5)
    lines = sorted(res.lines, key=lambda l: (l.box[1], l.box[0]))
    out = {"kind": None, "name": None, "commander": None, "divisions": None, "capacity": None, "selected": None,
           "rows": [], "unassigned": False, "lines": [l.text for l in lines][:40]}
    sel = next((l for l in lines if SEL_RX.search(l.text) and l.box[0] < 200), None)
    if sel:
        m = SEL_RX.search(sel.text)
        out["selected"] = int(m.group(1))
    else:
        # "14 Divisions": the number in front of the word is often dropped by OCR at panel scale
        word = next((l for l in lines if re.match(r"^\W*(divisions?|selected)\b", l.text, re.I) and l.box[0] < 200
                     and l.box[1] < 260), None)
        if word:
            sel = word
            x, y, bw, bh = word.box
            for scale in (3.0, 4.0):
                r = session.ocr.recognize(img, region=[max(0, x - 44), y - 5, 48, bh + 10], scale=scale)
                m = re.search(r"(\d{1,3})", " ".join(l.text for l in r.lines))
                if m:
                    out["selected"] = int(m.group(1))
                    break
            if out["selected"] is None:
                from ..perception import glyphs
                g = glyphs.read(img[y - 4:y + bh + 4, max(0, x - 44):x - 2],
                                glyphs.font(session.cfg.game_dir, "hoi_18mbs"), charset="0123456789")
                out["selected"] = int(g) if re.match(r"^\d{1,3}$", g or "") else -1   # -1: some, count unread
    if sel and sel.box[1] > 150:
        # an army: the header holds its name (after the division-count badge), the commander and "n/m"
        head = session.ocr.recognize(img, region=[0, 84, 440, 72], scale=2.0).lines
        names = [l for l in head if l.box[1] < 112 and l.box[0] < 250 and not re.match(r"^\W*\d+\W*$", l.text)]
        if names:
            out["kind"] = "army"
            out["name"] = re.sub(r"^\s*\d+\s+", "", names[0].text).strip() if names[0].box[0] < 50 else names[0].text.strip()
        cmd = [l.text for l in sorted(head, key=lambda l: l.box[1]) if 112 < l.box[1] < 152 and 60 < l.box[0] < 220]
        txt = " ".join(cmd)
        if not re.search(r"no\s*commander", txt, re.I):
            out["commander"] = re.sub(r"^\s*(genera\w*|field\s*marshal|admiral)\s*", "", txt, flags=re.I).strip() or None
        for scale in (3.0, 2.0):
            cnt = session.ocr.recognize(img, region=[290, 88, 130, 26], scale=scale).lines
            m = next((COUNT_RX.match(l.text) for l in cnt if COUNT_RX.match(l.text)), None)
            if m:
                out["divisions"], out["capacity"] = int(m.group(1)), int(m.group(2))
                break
    elif sel:
        out["kind"] = "selection"
    if out["kind"] is None and lines:
        # an army with a commander: "<n> <army name>" then "General|Field Marshal" and the commander's name
        top = [l for l in lines if l.box[1] < 160]
        m = re.match(r"^\W*(\d{1,3})\s+(\S.*)$", top[0].text) if top else None
        if m and any(re.search(r"gener|marshal|admiral|commander", l.text, re.I) for l in top[1:6]):
            out["kind"] = "army"
            out["name"] = m.group(2).strip()
            cm = next((l.text for l in top[1:6] if not re.search(r"gener|marshal|admiral|commander|assign", l.text, re.I)
                       and len(l.text) > 3), None)
            out["commander"] = cm
    if out["kind"] in (None, "selection") and any(re.search(r"no\s*commander|click to assign", l.text, re.I)
                                                  for l in lines[:12]):
        # a new army has no commander and a shorter header: the name is the first text line of the panel
        heads = [l for l in lines if l.box[1] < 135 and re.search(r"[A-Za-z]{3}", l.text)
                 and not re.search(r"commander|assign|selected|click", l.text, re.I)]
        if heads:
            out["kind"] = "army"
            out["name"] = heads[0].text.strip()
    if sel:
        out["rows"] = [l.text.strip() for l in lines if l.box[1] > sel.box[1] + 20 and 140 < l.box[0] < 330
                       and len(l.text.strip()) > 3 and not re.search(r"unassigned|right-click|create a new|"
                                                                      r"expeditionar", l.text, re.I)]
        out["unassigned"] = any(re.search(r"unassigned", l.text, re.I) for l in lines)
    return out


def select_card(session, cx):
    x, y = card_points(session, cx)["portrait"]
    session.input.click(x, y)
    time.sleep(0.9)
    session.park_cursor()
    time.sleep(0.3)
    return panel(session)


def find_army(session, name=None, index=None):
    """Select an army by name (or 1-based card index). Returns (card, panel)."""
    armies = cards(session)
    if not armies:
        raise ToolError("no army cards at the bottom of the screen (no armies, or another view hides them)")
    if index:
        if index > len(armies):
            raise ToolError(f"there are {len(armies)} army card(s), not {index}")
        c = armies[index - 1]
        return c, select_card(session, c["x"])
    seen = []
    for c in armies:
        p = select_card(session, c["x"])
        seen.append(p.get("name"))
        if name is None or (p.get("name") and panels.similar(p["name"], name) >= 0.85):
            return c, p
    raise ToolError(f"no army named '{name}' (cards show: {seen})", {"armies": seen})


def deselect(session):
    """Clear the unit selection with a left click on empty water/land far from units is unreliable; the panel's
    close button (top right of the panel) is used instead."""
    img = session.grab()
    res = session.ocr.recognize(img, region=[0, 84, 440, 40], scale=1.5)
    if res.lines:
        session.input.click(417, 99)
        time.sleep(0.5)
        session.park_cursor()


def rename_army(session, new_name):
    """Click the army name in its panel, replace the text, Enter."""
    session.input.click(110, 100)
    time.sleep(0.4)
    with clipboard.Preserved():
        session.input.key("end")
        session.input.key("backspace", times=40, delay=0.01)
        clipboard.set_text(new_name)
        time.sleep(0.05)
        session.input.key("ctrl+v")
    time.sleep(0.3)
    session.input.key("enter")
    time.sleep(0.6)
    session.park_cursor()
    return panel(session)


# ---------------------------------------------------------------------------------------------- commanders
def open_commander_window(session):
    session.input.click(34, 135)
    time.sleep(1.2)
    session.park_cursor()
    img = session.grab()
    res = session.ocr.recognize(img, region=[500, 150, 920, 800], scale=1.5)
    title = next((l for l in res.lines if re.search(r"select\s*commander", l.text, re.I)), None)
    if not title:
        raise ToolError("the Select Commander window did not open", {"seen": [l.text for l in res.lines][:15]})
    return title, res


def commander_rows(session, img=None):
    """[{name, box}] of commanders listed in the Select Commander window (name lines under a 'General' rank)."""
    img = session.grab() if img is None else img
    res = session.ocr.recognize(img, region=[500, 150, 920, 800], scale=1.5)
    ranks = [l for l in res.lines if re.match(r"^\W*(general|field\s*marshal|admiral)\W*$", l.text, re.I)]
    rows = []
    for r in ranks:
        below = [l for l in res.lines if 8 <= l.box[1] - r.box[1] <= 30 and abs(l.box[0] - r.box[0]) < 12]
        if below:
            rows.append({"name": below[0].text.strip(), "box": below[0].box, "rank": r.text.strip()})
    return rows, res


def assign_commander(session, name=None):
    """Open the commander window from the army panel, search by name (or take the first listed), click the row."""
    title, res = open_commander_window(session)
    search = next((l for l in res.lines if re.match(r"^\W*search\W*$", l.text, re.I)), None)
    if name and search:
        session.input.click(*search.center)
        time.sleep(0.3)
        with clipboard.Preserved():
            session.input.key("end")
            session.input.key("backspace", times=30, delay=0.01)
            clipboard.set_text(name.split()[-1])          # the surname filters the list reliably
            time.sleep(0.05)
            session.input.key("ctrl+v")
        time.sleep(1.0)
    rows, _ = commander_rows(session)
    if not rows:
        close_window(session)
        raise ToolError(f"no commander listed{' for ' + name if name else ''}")
    pick = rows[0]
    if name:
        good = [r for r in rows if panels.similar(r["name"], name) >= 0.8 or
                panels.norm(name.split()[-1]) in panels.norm(r["name"])]
        if not good:
            close_window(session)
            raise ToolError(f"no commander named '{name}' in the list", {"listed": [r["name"] for r in rows]})
        pick = good[0]
    x, y, bw, bh = pick["box"]
    session.input.click(x + bw // 2, y + bh // 2)
    time.sleep(1.2)
    session.park_cursor()
    return pick, [r["name"] for r in rows]


def close_window(session):
    """Close a centred window (commander list) with Esc: a window is open, so Esc does not reach the map."""
    from . import menu
    session.input.key("esc")
    time.sleep(0.6)
    if menu.esc_menu_open(session)[0]:
        session.input.key("esc")
        time.sleep(0.5)


# ---------------------------------------------------------------------------------------------- the map
def plan_mask(img):
    """Pixels of battle plan lines (front, offensive): saturated red to magenta, inside the map area. Under the
    night tint of a save loaded at 12:00 the offensive arrow is (133, 34, 79) to (158, 66, 127), not the daylight red
    (200, 40, 40) of the first live runs, so blue and a lower red are allowed."""
    a = img.astype(np.int16)
    r, g, b = a[..., 0], a[..., 1], a[..., 2]
    m = ((r > 120) & (g < 105) & (b < 150) & (r - g > 70) & (r - b > 10)).astype(np.uint8)
    h, w = m.shape
    m[:95, :] = 0                       # top bar
    m[h - 230:, int(w * 0.28):int(w * 0.72)] = 0   # cards and toolbar
    m[h - 270:, w - 330:] = 0           # minimap and map mode buttons
    return m


def plan_lines(img, min_len=50):
    """Long thin red components (plan lines) as [{box, pixels, points}] (victory point markers are small blobs)."""
    m = plan_mask(img)
    m = cv2.morphologyEx(m, cv2.MORPH_CLOSE, np.ones((5, 5), np.uint8))
    n, labels, stats, _ = cv2.connectedComponentsWithStats(m, connectivity=8)
    out = []
    for i in range(1, n):
        x, y, bw, bh, area = [int(v) for v in stats[i]]
        if max(bw, bh) < min_len or area > 0.5 * bw * bh:
            continue
        ys, xs = np.nonzero(labels == i)
        out.append({"box": [x, y, bw, bh], "pixels": int(area), "xs": xs, "ys": ys})
    return out


def nearest_line_point(img, target, exclude_box=None):
    lines = plan_lines(img)
    best = None
    for ln in lines:
        d = (ln["xs"] - target[0]) ** 2 + (ln["ys"] - target[1]) ** 2
        i = int(d.argmin())
        if best is None or d[i] < best[0]:
            best = (int(d[i]), (int(ln["xs"][i]), int(ln["ys"][i])))
    return best[1] if best else None


def red_pixels_near(img, centre, radius=170):
    m = plan_mask(img)
    x, y = centre
    h, w = m.shape
    return int(m[max(0, y - radius):min(h, y + radius), max(0, x - radius):min(w, x + radius)].sum())


def _name_match(a, b):
    import difflib
    na = re.sub(r"[^a-z0-9]", "", (a or "").lower())
    nb = re.sub(r"[^a-z0-9]", "", (b or "").lower())
    return bool(na) and bool(nb) and (na in nb or nb in na or difflib.SequenceMatcher(None, na, nb).ratio() >= 0.8)


def find_front_point(session, centre, radii=(0, 70, 140, 210, 280), steps=8, owner=None):
    """With the front line tool active: a point near centre where the game offers 'Assign Front Line' (the cursor
    must be on the other country's territory close to the shared border). Spirals out from centre."""
    w, h = session.window.size()
    tried = []
    for r in radii:
        for k in range(steps if r else 1):
            ang = 2 * math.pi * k / steps
            p = (int(centre[0] + r * math.cos(ang)), int(centre[1] + r * math.sin(ang)))
            if not (40 < p[0] < w - 340 and 110 < p[1] < h - 240) or (p[0] < 450 and p[1] < 420):
                continue
            d, _, _ = tooltip.read(session, p[0], p[1], wait=0.55, park=False)
            txt = " ".join(d.get("lines") or [])
            tried.append((p, txt[:40]))
            if re.search(r"defensive\s*line|entire\s*border", txt, re.I) and not re.search(r"cannot\s*draw", txt, re.I):
                if owner:
                    # the line is drawn against the owner of the clicked province: it must be the intended enemy (a
                    # live run drew a line against a neighbour that was not at war, 150 px from the target)
                    m = re.search(r"owner\W+(.+?)(?:\s+weather|$)", " ".join(d.get("lines") or []), re.I)
                    got = m.group(1) if m else (d.get("values") or {}).get("Owner")
                    tried.append((p, "owner " + str(got)))
                    if got and not any(_name_match(got, o) for o in owner):
                        continue
                return p, tried
    return None, tried


def box_select(session, centre, half=(150, 110)):
    """Drag a selection box with the left button around centre (divisions inside get selected)."""
    x, y = centre
    session.input.drag(x - half[0], y - half[1], x + half[0], y + half[1], button="left", seconds=0.5)
    time.sleep(0.8)
    session.park_cursor()
    time.sleep(0.3)
    return panel(session)


def empty_map_points(img, count=4):
    """Points on the default map away from counters, panels and cards: dark-ish, no green or red counter colours in a
    40 px square, tried right of the unit panel and left of the theatre box."""
    h, w = img.shape[:2]
    out = []
    for fy in (0.30, 0.40, 0.50, 0.22, 0.60):
        for fx in (0.40, 0.50, 0.60, 0.32):
            x, y = int(w * fx), int(h * fy)
            patch = img[y - 20:y + 20, x - 20:x + 20].astype(np.int16).reshape(-1, 3)
            r, g, b = patch[:, 0], patch[:, 1], patch[:, 2]
            counter = ((g > r + 25) & (g > b + 15)).mean() + ((r > g + 60) & (r > b + 60)).mean()
            if counter < 0.01 and patch.max() < 235:
                out.append((x, y))
            if len(out) >= count:
                return out
    return out or [(w // 2, int(h * 0.35))]


def deselect(session):
    """Clear the unit selection with a left click on empty map (the fleet and air wing cards only show when nothing
    is selected: with an army selected the bottom strip stays the army cards, as seen live). Returns whether a
    selection existed."""
    p = panel(session)
    if not p.get("kind"):
        return False
    for pt in empty_map_points(session.grab()):
        session.input.click(*pt)
        time.sleep(0.8)
        session.park_cursor()
        time.sleep(0.4)
        if not panel(session).get("kind"):
            break
    return True


def right_drag(session, p0, p1, steps=30, seconds=0.6):
    """Hold the right button from p0 to p1 (offensive lines are painted this way)."""
    session.input.move(*p0)
    time.sleep(0.15)
    session.input.mouse_down("right")
    try:
        for k in range(1, steps + 1):
            t = k / steps
            session.input.move(int(p0[0] + (p1[0] - p0[0]) * t), int(p0[1] + (p1[1] - p0[1]) * t))
            time.sleep(seconds / steps)
        time.sleep(0.2)
    finally:
        session.input.mouse_up("right")
    time.sleep(1.0)
    session.park_cursor()


def right_path(session, points, steps_per_leg=24, seconds_per_leg=0.5):
    """Hold the right button along a polyline of client points (an offensive line that bends through several
    places, for example across the pockets an enemy still holds)."""
    session.input.move(*points[0])
    time.sleep(0.15)
    session.input.mouse_down("right")
    try:
        for a, b in zip(points, points[1:]):
            for k in range(1, steps_per_leg + 1):
                t = k / steps_per_leg
                session.input.move(int(a[0] + (b[0] - a[0]) * t), int(a[1] + (b[1] - a[1]) * t))
                time.sleep(seconds_per_leg / steps_per_leg)
        time.sleep(0.2)
    finally:
        session.input.mouse_up("right")
    time.sleep(1.0)
    session.park_cursor()
