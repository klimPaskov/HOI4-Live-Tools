"""Recruit & Deploy (U) and the division designer (phase 4).

Vanilla 1.19 at gui_scale 1 (checked at 1920x1080):

- The deployment key opens two panels side by side: "Recruit & Deploy" (left: reinforcement priorities, then one
  block per training line) and "Division Templates" (right: one row per template with Train and Edit under its name,
  rows 72 px apart; a "Division Designer" button on top).
- Train adds a training line for the template at the bottom of the left panel: the template's name with -, the
  serial count and + on its right, below it the location button ("No location set": click, then click a state on
  the map), "Add Unit" (one more division in parallel; Shift+Click adds five) and the line's Deploy button, then
  "<n> Division(s)" and one row per division in training with its own deploy and cancel buttons. A division can be
  deployed once it has equipment ("Cannot Deploy a unit that does not have any equipment" before that).
- Edit opens the division designer: the template name is an edit box at the top; the battalion grid has five
  columns of five slots and a support column; a filled slot's tooltip says "Click to change / Shift + Click to
  remove", an empty one "Click to add" (it opens the list of battalion groups, then the battalions of a group with
  their experience cost); "Duplicate" makes the save create a new template and keep the original; "Save" shows the
  army experience it costs ("Design is too expensive" when there is not enough). Close with the window's X.

The grid is located from the "Divisional Composition" label, the buttons from their own labels.
"""

import re
import time

from ..desktop import clipboard
from ..errors import ToolError
from ..perception import tooltip
from . import panels

# offsets from the top-left of the OCR box of "Divisional Composition" (vanilla, gui_scale 1)
GRID_COLS = [25, 127, 229, 331, 433]
GRID_ROWS = [146, 191, 238, 284, 330]
SUPPORT_X = 522
SUPPORT_ROWS = [172, 207, 242, 276]
NAME_FIELD = (209, -93)
CLOSE_X = (1110, -142)


def open_panel(session):
    panels.open_panel(session, "deployment")
    time.sleep(0.6)


# ---------------------------------------------------------------------------------------------- templates panel
def template_rows(session, img=None):
    """[{name, train: (x, y), edit: (x, y)}] of the Division Templates panel as shown (top to bottom)."""
    img = session.grab() if img is None else img
    h, w = img.shape[:2]
    res = session.ocr.recognize(img, region=[540, 180, 380, h - 260], scale=1.5)
    trains = [l for l in res.lines if re.match(r"^\W*train\W*$", l.text, re.I)]
    edits = [l for l in res.lines if re.match(r"^\W*edit\W*$", l.text, re.I)]
    rows = []
    for t in trains:
        name = [l for l in res.lines if 18 <= t.box[1] - l.box[1] <= 45 and not re.match(r"^\W*(train|edit)\W*$",
                                                                                         l.text, re.I)]
        e = min(edits, key=lambda l: abs(l.box[1] - t.box[1]) + abs(l.box[0] - t.box[0] - 73), default=None)
        if not name:
            continue
        nm = max(name, key=lambda l: l.box[1])
        rows.append({"name": nm.text.strip(), "box": nm.box, "train": t.center,
                     "edit": e.center if e and abs(e.box[1] - t.box[1]) < 8 else (t.center[0] + 73, t.center[1])})
    rows.sort(key=lambda r: r["box"][1])
    return rows


def find_template(session, name, scrolls=6):
    """The row of a template by name, scrolling the list down when needed."""
    seen = []
    for i in range(scrolls + 1):
        rows = template_rows(session)
        for r in rows:
            if r["name"] not in seen:
                seen.append(r["name"])
        good = [r for r in rows if panels.similar(r["name"], name) >= 0.86]
        if good:
            return good[0], seen
        if i < scrolls:
            session.input.scroll(730, 600, -3)
            time.sleep(0.5)
    raise ToolError(f"no division template named '{name}' in the Division Templates list", {"templates": seen})


# ---------------------------------------------------------------------------------------------- training lines
LINE_COUNT_RX = re.compile(r"^\W*(\d+)\s*divisions?\W*$", re.I)


def training_lines(session, img=None):
    """[{template, y, location, divisions, add_unit, deploy, location_button}] of the Recruit & Deploy panel."""
    img = session.grab() if img is None else img
    h, w = img.shape[:2]
    res = session.ocr.recognize(img, region=[0, 400, 530, h - 420], scale=1.5)
    # a block: the template name (x ~120), 33 px below it the button row (location, Add Unit, Deploy: their
    # labels sit on coloured buttons that OCR misses), 72 px below it "<n> Division(s)"
    counts = [l for l in res.lines if LINE_COUNT_RX.match(l.text) and l.box[0] > 250]
    lines = []
    for c in sorted(counts, key=lambda l: l.box[1]):
        name = [l for l in res.lines if 58 <= c.center[1] - l.center[1] <= 86 and 90 < l.box[0] < 260]
        if not name:
            continue
        nm = name[0]
        y = nm.center[1] + 33
        loc_res = session.ocr.recognize(img, region=[128, y - 10, 205, 20], scale=3.0)
        loc = " ".join(l.text for l in loc_res.lines).strip() or None
        # "No location set" is drawn red; a set location (a state's name) is not
        colour = panels.text_colour(img, [130, y - 8, 200, 16])
        if colour == "red" or (loc and panels.similar(loc, "No location set") >= 0.6):
            loc = None
        lines.append({"template": nm.text.strip(), "y": y, "location": loc,
                      "divisions": int(LINE_COUNT_RX.match(c.text).group(1)),
                      "add_unit": (385, y), "deploy": (455, y), "location_button": (221, y)})
    return lines


def division_rows(session, line, img=None):
    """[{name, y, deploy}] of the divisions in training under a line (rows 40 px apart below its count line)."""
    img = session.grab() if img is None else img
    h = img.shape[0]
    res = session.ocr.recognize(img, region=[0, line["y"] + 40, 300, min(h - line["y"] - 60, 520)], scale=1.5)
    rows = []
    for l in sorted(res.lines, key=lambda l: l.box[1]):
        if l.box[0] > 40 or len(l.text.strip()) < 4:
            if l.box[0] > 90 and rows:            # the next line block's template name: stop
                break
            continue
        if rows and l.center[1] - rows[-1]["y"] < 25:
            continue
        rows.append({"name": l.text.strip(), "y": l.center[1], "deploy": (475, l.center[1])})
    return rows


def row_tooltip(session, row):
    """The tooltip of a division row's deploy button ('' when no tooltip box was found)."""
    d, _, _ = tooltip.read(session, *row["deploy"], wait=0.9)
    lines = d.get("lines") or []
    # near the bottom of the panel the box is not always isolated: the tooltip's own lines are the last ones read
    return " ".join(lines if d.get("found") else lines[-3:])


def deploy_tooltip(session, line):
    d, _, _ = tooltip.read(session, *line["deploy"], wait=0.8)
    return " ".join(d.get("lines") or [])


# ---------------------------------------------------------------------------------------------- the designer
def designer_anchor(session, img=None):
    img = session.grab() if img is None else img
    res = session.ocr.recognize(img, region=[300, 150, 1300, 760], scale=1.5)
    comp = next((l for l in res.lines if re.search(r"divisional\s*composition", l.text, re.I)), None)
    title = next((l for l in res.lines if re.search(r"division\s*designer|designer", l.text, re.I)
                  and l.box[1] < 260), None)
    return comp, title, res


def wait_designer(session, timeout=4.0):
    t0 = time.time()
    while time.time() - t0 < timeout:
        comp, title, res = designer_anchor(session)
        if comp:
            return comp, res
        time.sleep(0.5)
    raise ToolError("the division designer did not open")


def designer_point(comp, dx, dy):
    return (comp.box[0] + dx, comp.box[1] + dy)


def slot_point(comp, col, row):
    """Battalion slot (1-based column 1..5, row 1..5) centre."""
    return designer_point(comp, GRID_COLS[col - 1], GRID_ROWS[row - 1])


def support_point(comp, row):
    return designer_point(comp, SUPPORT_X, SUPPORT_ROWS[row - 1])


def slot_info(session, point):
    """('filled', name) | ('empty', None) | ('locked', None) from the slot's tooltip."""
    d, _, _ = tooltip.read(session, *point, wait=0.7)
    lines = [l.strip() for l in d.get("lines") or []]
    session.last_slot_tooltip = lines
    txt = " ".join(lines)
    if re.search(r"to\s*change", txt, re.I):
        # the battalion's name is the line before "Click" / "to change" (the box may include other text above)
        for i, l in enumerate(lines):
            if re.match(r"^\W*click\W*$", l, re.I) or re.search(r"click\s*to\s*change", l, re.I):
                return "filled", lines[i - 1] if i else None
        return "filled", lines[0] if lines else None
    if re.search(r"to\s*add", txt, re.I):
        return "empty", None
    return "locked", txt[:80] or None


def designer_name(session, comp, tries=3):
    """The designer's name box, read at several OCR scales and again after a pause (it comes back empty now and
    then while the machine is busy)."""
    x, y = designer_point(comp, *NAME_FIELD)
    for attempt in range(tries):
        img = session.grab()
        for scale in (2.0, 3.0, 1.5):
            res = session.ocr.recognize(img, region=[x - 110, y - 14, 260, 28], scale=scale)
            text = " ".join(l.text for l in res.lines).strip()
            if text:
                return text
        time.sleep(1.0)
    return None


def set_designer_name(session, comp, name):
    x, y = designer_point(comp, *NAME_FIELD)
    session.input.click(x, y)
    time.sleep(0.4)
    with clipboard.Preserved():
        session.input.key("end")
        session.input.key("backspace", times=50, delay=0.01)
        clipboard.set_text(name)
        time.sleep(0.05)
        session.input.key("ctrl+v")
    time.sleep(0.3)
    session.input.key("enter")
    time.sleep(0.5)
    session.park_cursor()
    return designer_name(session, comp)


# button centres relative to the "Divisional Composition" label (vanilla, gui_scale 1), used when OCR misses a label
BUTTON_OFFSETS = {"Save": (530, 519), "Duplicate": (149, 469), "Reset": (21, 469)}


def designer_button(session, label, res=None, comp=None):
    """Centre of a designer button by its label (Duplicate, Save, Reset, Back); OCR misses the white-on-green Save
    label now and then, so a second read of the bottom strip and then the vanilla offset from the composition
    label are tried."""
    if res is None:
        comp, _, res = designer_anchor(session)
    rx = re.compile(r"^\W*" + label + r"", re.I)
    hits = [l for l in res.lines if rx.search(l.text)]
    if not hits and comp is not None:
        img = session.grab()
        strip = session.ocr.recognize(img, region=[comp.box[0] - 40, comp.box[1] + 440, 900, 110], scale=2.0)
        hits = [l for l in strip.lines if rx.search(l.text)]
    if hits:
        return hits[-1].center
    if comp is not None and label in BUTTON_OFFSETS:
        dx, dy = BUTTON_OFFSETS[label]
        return (comp.box[0] + dx, comp.box[1] + dy)
    raise ToolError(f"the designer's {label} button was not found", {"seen": [l.text for l in res.lines][:30]})


def choose_battalion(session, wanted_names, max_groups=7):
    """After a '+' slot click: pick the battalion whose label matches one of wanted_names, opening groups as
    needed. Returns (label clicked, cost text) or raises."""
    def listing():
        session.park_cursor()                # a tooltip over the list merges its lines ("Artillery Anti-Air")
        time.sleep(0.5)
        img = session.grab()
        return session.ocr.recognize(img, region=[360, 230, 640, 470], scale=1.5)

    def same(a, b):
        import difflib
        na, nb = panels.norm(a), panels.norm(b)
        return bool(na) and (na == nb or difflib.SequenceMatcher(None, na, nb).ratio() >= 0.86)

    def match(res):
        for l in res.lines:
            words = [w["text"] for w in (l.words or [])]
            if any(same(l.text, n) for n in wanted_names) or                     (words and any(same(words[0], n) for n in wanted_names)):
                cost = next((c.text for c in res.lines if re.search(r"cost", c.text, re.I) and
                             0 < c.box[1] - l.box[1] < 50 and abs(c.box[0] - l.box[0]) < 30), None)
                return l, cost
        return None, None

    res = listing()
    hit, cost = match(res)
    if hit:
        session.input.click(*hit.center)
        time.sleep(0.8)
        return hit.text, cost
    groups = [l for l in res.lines if re.search(r"battalions|support\s*companies|companies", l.text, re.I)
              and not re.search(r"make a selection", l.text, re.I)]
    tried = []
    for g in groups[:max_groups]:
        session.input.click(*g.center)
        time.sleep(0.8)
        sub = listing()
        hit, cost = match(sub)
        tried.append(g.text)
        if hit:
            session.input.click(*hit.center)
            time.sleep(0.8)
            return hit.text, cost
        back = next((l for l in sub.lines if re.match(r"^\W*back\W*$", l.text, re.I)), None)
        if back:
            session.input.click(*back.center)
            time.sleep(0.7)
    back = next((l for l in listing().lines if re.match(r"^\W*back\W*$", l.text, re.I)), None)
    if back:
        session.input.click(*back.center)
        time.sleep(0.6)
    img = session.grab()
    from ..desktop import images
    png, _, _ = images.encode(img, 960)
    seen = [l.text for l in listing().lines][:40]
    raise ToolError(f"no battalion named {wanted_names} in the designer's lists (groups tried: {tried})",
                    {"groups": [g.text for g in groups], "listing": seen}, image=png)


def save_tooltip(session, res=None, comp=None):
    x, y = designer_button(session, "Save", res, comp)
    d, _, _ = tooltip.read(session, x, y, wait=0.8)
    return (x, y), " ".join(d.get("lines") or [])


def save_tooltip_at(session, xy):
    d, _, _ = tooltip.read(session, xy[0], xy[1], wait=0.8)
    return " ".join(d.get("lines") or [])


def close_designer(session, comp):
    session.input.click(*designer_point(comp, *CLOSE_X))
    time.sleep(0.8)
    session.park_cursor()
    # a "discard changes?" confirmation may follow
    panels.answer_dialog(session, accept=True, wait=0.6, attempts=1)
