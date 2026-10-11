"""Production: the production panel's lines and factory counts, adding a line, changing a line's equipment and
assigning factories (HOI4 1.19 with Arms Against Tyranny, verified live on 1.19.2).

Panel (Y), vanilla layout at gui_scale 1:
    header      "Military Factories used/total", "Dockyards used/total"
    add buttons land (60, 262), armour (140, 262), air (220, 262), naval (370, 262)
    lines       a scrolling list from y~365, one line per 111 px: "<n> <variant name>" with "-  <factories>  +" and
                a delete button on its first row, the equipment type under the name, the picture below (clicking
                the picture changes the produced equipment: the line's progress is lost)
    choose list a window right of the panel listing the variants that can be produced (name, type, cost)
    MIO dialog  after picking equipment (Arms Against Tyranny): "Produce without a Military Industrial
                Organization" or one of the organisations
"""

import re
import time

from ..errors import ToolError
from ..perception import glyphs
from . import panels

ADD_BUTTONS = {"land": (60, 262), "armor": (140, 262), "air": (220, 262), "naval": (370, 262)}
LIST_TOP = 362
LINE_PITCH = 111
ROW = {"minus": 400, "count": 436, "plus": 473, "delete": 502}


def read_header(session, img=None):
    img = session.grab() if img is None else img
    res = session.ocr.recognize(img, region=[0, 208, 540, 30], scale=2.2)
    text = " ".join(l.text for l in res.lines)
    out = {"text": text}
    fr = re.findall(r"(\d+)\s*/\s*(\d+)", text)
    if fr:
        out["military_used"], out["military_total"] = int(fr[0][0]), int(fr[0][1])
    if len(fr) > 1:
        out["dockyards_used"], out["dockyards_total"] = int(fr[1][0]), int(fr[1][1])
    return out


def read_lines(session, img=None):
    """Visible production lines: [{number, name, type, factories, row_top}] (factories as shown: 'n' or
    'assigned/requested')."""
    img = session.grab() if img is None else img
    h = img.shape[0]
    res = session.ocr.recognize(img, region=[0, LIST_TOP, 540, h - LIST_TOP - 8], scale=2.0)
    heads = []
    for l in res.lines:
        # "4 Mondragón M94"; a line short of equipment shows a warning triangle after its number, which OCR reads
        # as "a"/"A", glued to the number or not: "4a Mondragón M94", "2 a Towed Artillery"
        m = re.match(r"^\W{0,2}(\d{1,3})[\s.,]*(?:[aA]\s+)?(.{2,})$", l.text)
        if m and l.box[0] < 40:
            heads.append((l, int(m.group(1)), m.group(2).strip()))
    lines = []
    fonts = [(glyphs.font(session.cfg.game_dir, n), t) for n, t in (("hoi_18b", 0.6), ("hoi_16mbs", 0.55))]
    for l, num, name in heads:
        top = l.box[1] - 7
        if top > h - 60:                         # a line cut by the bottom of the list
            continue
        row = [x for x in res.lines if top - 2 <= x.box[1] <= top + 22 and x is not l]
        count = next((x for x in row if 410 <= x.center[0] <= 460), None)
        sub = next((x for x in res.lines if top + 18 <= x.box[1] <= top + 40 and x.box[0] < 60), None)
        # lone digits escape Windows OCR: read the count with the game's own font first. The crop is taller than
        # the header row because `top` follows the OCR'd head line by up to +-6 px: a crop that cuts the bottom
        # bar of a "2" reads "7"
        text = ""
        for fnt, thr in fonts:
            if fnt is not None and not re.search(r"\d", text):
                text = glyphs.read(img[max(0, top - 4):top + 30, 415:463], fnt, threshold=thr)
        if "/" in (text or "") or not re.search(r"\d", text or ""):
            # "1/3" (1 assigned of 3 requested: none free) draws the 3 dimmer, which the font matcher can drop
            # ("1/"); Windows OCR reads the two-number form well, only lone digits escape it
            ocr = session.ocr.recognize(img, region=[412, max(0, top - 2), 56, 30], scale=3.0)
            got = next((l2.text for l2 in ocr.lines if re.fullmatch(r"\W*\d{1,3}\s*/\s*\d{1,3}\W*", l2.text)), None)
            if got:
                text = got.strip()
        if not re.search(r"\d", text or ""):
            text = count.text if count else None
        fac = req = None
        mm = re.search(r"(\d+)(?:\s*/\s*(\d+)?)?", text or "")
        if mm:
            # "3" when every requested factory is assigned; "1/3" = 1 assigned of 3 requested (none free);
            # a dangling "1/" means more than 1 requested but the number was not readable
            fac = int(mm.group(1))
            req = int(mm.group(2)) if mm.group(2) else (None if "/" in (text or "") else fac)
        lines.append({"number": num, "name": re.sub(r"\s+", " ", name), "type": sub.text if sub else None,
                      "factories": fac, "requested": req, "factories_text": text, "row_top": top})
    return lines


def scroll_list(session, notches):
    session.input.scroll(260, 700, notches)
    time.sleep(0.5 + min(1.0, abs(notches) * 0.02))
    session.park_cursor()
    time.sleep(0.5)                              # a line's tooltip under the parked cursor fades out


def all_lines(session, max_pages=12):
    """Every line: scroll to the top, then page down reading the visible lines."""
    scroll_list(session, 40)
    seen = {}
    last = None
    for _ in range(max_pages):
        ls = read_lines(session)
        for l in ls:
            seen.setdefault(l["number"], l)
        key = tuple(l["number"] for l in ls)
        if key == last:
            break
        last = key
        scroll_list(session, -5)
    scroll_list(session, 40)
    return [seen[k] for k in sorted(seen)]


def bottom_lines(session):
    """The lines visible at the bottom of the list (the newest line is the last one)."""
    scroll_list(session, -60)
    return read_lines(session)


def find_line(session, match, max_pages=12, from_bottom=False):
    """Scroll until a line satisfying match(line) is visible; returns the line (with its current row_top)."""
    ls = read_lines(session)                      # already visible?
    hit = next((l for l in ls if match(l)), None)
    if hit:
        return hit
    scroll_list(session, -60 if from_bottom else 40)
    last = None
    for _ in range(max_pages):
        ls = read_lines(session)
        hit = next((l for l in ls if match(l)), None)
        if hit:
            return hit
        key = tuple(l["number"] for l in ls)
        if key == last:
            return None
        last = key
        scroll_list(session, 4 if from_bottom else -4)
    return None


def click_row(session, line, what, times=1):
    x = ROW[what]
    y = line["row_top"] + 12
    for _ in range(times):
        session.input.click(x, y, settle=0.15)
        time.sleep(0.25)
    session.park_cursor()
    time.sleep(0.5)


def requested(line):
    """The factories a line asks for (what + and - set); the assigned count can be lower when none is free.
    None for a dangling "1/" (more than the assigned, number unreadable)."""
    r = line.get("requested")
    if r is None and "/" in (line.get("factories_text") or ""):
        return None
    return line.get("factories") if r is None else r


def set_factories(session, line, target, max_clicks=60):
    """Click + or - on the line until it requests `target` factories (re-reading the count after each burst; the
    game drops clicks that come too fast). The line shows "assigned/requested" while factories are short, so the
    requested number is what moves. Returns the line as read last; stops when the number no longer moves."""
    cur = line
    stuck = 0
    clicks = 0
    while clicks < max_clicks:
        have = requested(cur)
        if have is None or have == target:
            break
        diff = target - have
        n = min(abs(diff), 5)
        click_row(session, cur, "plus" if diff > 0 else "minus", n)
        clicks += n
        time.sleep(0.3)
        again = next((l for l in read_lines(session) if l["number"] == line["number"]), None)
        if again is None:
            again = find_line(session, lambda l, k=line["number"]: l["number"] == k, from_bottom=True)
        if again is None:
            break
        stuck = stuck + 1 if requested(again) == have else 0
        cur = again
        if stuck >= 2:
            break
    return cur


# ---------------------------------------------------------------------------------------------- choose list
def read_choices(session, img=None):
    """Entries of the open equipment list: [{name, type, cost, center}]."""
    img = session.grab() if img is None else img
    h = img.shape[0]
    res = session.ocr.recognize(img, region=[548, 170, 470, h - 180], scale=1.8)
    lines = sorted(res.lines, key=lambda l: (l.box[1], l.box[0]))
    entries = []
    for l in lines:
        if l.box[0] > 600 or l.box[0] < 555:
            continue
        if re.match(r"^[\W\d.,]+$", l.text):
            continue
        # an entry title starts a new entry when it is >= 40 px below the previous title
        if entries and l.box[1] - entries[-1]["title_y"] < 22:
            if entries[-1]["type"] is None:
                entries[-1]["type"] = l.text
            continue
        entries.append({"name": l.text, "type": None, "title_y": l.box[1],
                        "center": (760, l.box[1] + 30)})
    for e in entries:
        cost = [l.text for l in lines if l.box[1] > e["title_y"] + 40 and l.box[1] < e["title_y"] + 68 and
                l.box[0] < 640 and re.search(r"\d", l.text)]
        e["cost"] = cost[0] if cost else None
    return entries


def choices_open(session, img=None):
    img = session.grab() if img is None else img
    res = session.ocr.recognize(img, region=[548, 88, 470, 40], scale=2.0)
    return any(re.search(r"outdated", l.text, re.I) for l in res.lines)


def pick_choice(session, names, max_scrolls=8):
    """Click the list entry whose name or type matches one of names. Returns the entry."""
    last = None
    for _ in range(max_scrolls):
        entries = read_choices(session)
        best = None
        for e in entries:
            score = max([panels.similar(e["name"], n) for n in names] +
                        [panels.similar(e["type"], n) - 0.05 for n in names if e["type"]])
            if best is None or score > best[0]:
                best = (score, e)
        if best and best[0] >= 0.8:
            session.input.click(*best[1]["center"])
            time.sleep(1.2)
            session.park_cursor()
            return best[1], entries
        key = tuple(e["name"] for e in entries)
        if key == last:
            break
        last = key
        session.input.scroll(780, 600, -4)
        time.sleep(0.5)
    return None, entries if 'entries' in locals() else []


def answer_mio(session, organisation=None):
    """After picking equipment the MIO dialog may ask which organisation produces it. Picks 'without' unless an
    organisation name is given. Returns what was chosen or None when no dialog showed."""
    time.sleep(0.4)
    img = session.grab()
    res = session.ocr.recognize(img, region=[540, 80, 560, 700], scale=1.6)
    texts = [l.text for l in res.lines]
    if not any(re.search(r"industrial\s*organi[sz]ation", t, re.I) for t in texts):
        return None
    if organisation:
        hit = next((l for l in res.lines if panels.similar(l.text, organisation) >= 0.8), None)
        if hit:
            session.input.click(hit.center[0], hit.center[1])
            time.sleep(1.0)
            return hit.text
    hit = next((l for l in res.lines if re.search(r"produce\s*without", l.text, re.I)), None)
    if not hit:
        raise ToolError("the organisation dialog has no 'Produce without' entry", {"seen": texts})
    session.input.click(hit.center[0], hit.center[1])
    time.sleep(1.0)
    session.park_cursor()
    return "without organisation"
