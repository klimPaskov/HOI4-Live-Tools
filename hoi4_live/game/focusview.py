"""The national focus: the politics panel's focus button (current focus, progress), the focus tree view, its search
box and the focus detail window (HOI4 1.19).

Flow to start a focus (verified live on 1.19.2):
    politics panel (Q) -> focus button ("Select a National Focus" or the current focus) -> "<COUNTRY> FOCUS TREE"
    -> search box: type the focus' localised name; the view jumps to the first match ("1/3 Found"), Tab cycles
    -> Enter opens the detail window of the current match: title, "N days", Start button, requirements (met in
       green, unmet in red), description, effects
    -> Start: the tree closes and the politics panel's focus button shows the focus with a progress bar.
Nothing here depends on where the focus sits in the tree, so it works for any tree, mod or zoom.
"""

import re
import time

from ..desktop import clipboard, images
from ..errors import ToolError
from . import panels

NO_FOCUS_RX = re.compile(r"select\s*a\s*national\s*focus", re.I)


def _button_region(session):
    """The focus button of the politics panel (right of the leader portrait), vanilla layout."""
    return [270, 130, 265, 82]


def read_button(session, img=None):
    """{'current': name or None, 'text': ...} from the politics panel's focus button (panel must be open)."""
    img = session.grab() if img is None else img
    res = session.ocr.recognize(img, region=_button_region(session), scale=2.0)
    text = " ".join(l.text for l in res.lines).strip()
    if not text:
        return {"current": None, "text": "", "unread": True}
    if NO_FOCUS_RX.search(text) or re.search(r"national\s*focus", text, re.I):
        return {"current": None, "text": text}
    return {"current": text, "text": text}


def open_tree(session):
    """Open the focus tree from the politics panel. Returns the view title."""
    name, title = panels.read_title(session)
    if name == "focus_tree":
        return title
    panels.open_panel(session, "politics")
    x, y, w, h = _button_region(session)
    session.input.click(x + w // 2, y + 40)
    time.sleep(1.4)
    session.park_cursor()
    for _ in range(24):                  # the tree slides in: seconds on a busy machine (all cores taken by others)
        name, title = panels.read_title(session)
        if name == "focus_tree":
            return title
        time.sleep(0.5)
    img = session.grab()
    png, _, _ = images.encode(img, 960)
    raise ToolError("the focus tree did not open from the politics panel", {"title": title}, image=png)


def _search_strip(session):
    w, h = session.window.size()
    return [max(0, w - 640), 88, 640, 40]


def search_state(session, img=None):
    """{'found': (i, n) or None, 'box': [x, y, w, h] of the search field or None, 'lines': [...]}."""
    img = session.grab() if img is None else img
    res = session.ocr.recognize(img, region=_search_strip(session), scale=2.0)
    out = {"found": None, "box": None, "lines": [l.text for l in res.lines]}
    for l in res.lines:
        m = re.search(r"(\d+)\s*/\s*(\d+)\s*f\w*nd", l.text, re.I)
        if m:
            out["found"] = (int(m.group(1)), int(m.group(2)))
        elif re.search(r"type\s*to\s*search", l.text, re.I):
            out["box"] = l.box
    if re.search(r"\b0\s*found|no\s*result", " ".join(out["lines"]), re.I):
        out["found"] = (0, 0)
    return out


def search(session, text):
    """Type `text` into the tree's search box. Returns (i, n) found."""
    w, h = session.window.size()
    st = search_state(session)
    if st["box"]:
        x, y = st["box"][0] + 40, st["box"][1] + st["box"][3] // 2
    else:
        x, y = w - 240, 107                      # vanilla: the field sits left of the zoom buttons
    session.input.click(x, y)
    time.sleep(0.3)
    session.input.key("end")
    session.input.key("backspace", times=50, delay=0.01)
    with clipboard.Preserved():
        clipboard.set_text(text)
        time.sleep(0.05)
        session.input.key("ctrl+v")
    time.sleep(1.3)
    found = None
    for _ in range(14):                  # the match count appears late while the machine is busy
        found = search_state(session)["found"]
        if found:
            break
        time.sleep(0.5)
    return found


def _anchor_region(session):
    w, h = session.window.size()
    return [int(w * 0.12), 95, int(w * 0.76), 140]


# the detail window relative to its Start button (vanilla 1.19 layout, gui_scale 1)
DETAIL = {"window": (-429, -61, 540, 545), "title": (-429, -52, 540, 34), "days": (-227, -6, 170, 23),
          "requirements": (-227, 29, 305, 108), "body": (-410, 152, 505, 330)}


def _rel(anchor, key):
    dx, dy, w, h = DETAIL[key]
    return [anchor[0] + dx, anchor[1] + dy, w, h]


def find_detail(session, img=None):
    """Box of the Start button of an open focus detail window, or None."""
    img = session.grab() if img is None else img
    res = session.ocr.recognize(img, region=_anchor_region(session), scale=2.0)
    for l in res.lines:
        if re.search(r"^\W*start\W*$", l.text, re.I):
            return l.box
    return None


EFFECT_LINE_RX = re.compile(r"^\W*effects?\W*$|claim gained|gets event|world tension|gains .* war goal|will continue even|"
                            r"cancel if the|bypass the focus", re.I)
ONE_OF_RX = re.compile(r"(one|any|at least one) of the following|requires? one of", re.I)
INFO_RX = re.compile(r"cannot be cancelled|cannot be canceled|will not be cancelled", re.I)
BULLET_RX = re.compile(r"^\W{0,3}[-•–—]\s*\w")


def _start_enabled(img, box):
    """The Start button is greenish when it can be pressed and plain grey when a requirement is not met."""
    x, y, w, h = [int(v) for v in box]
    crop = img[max(0, y - 6):y + h + 6, max(0, x - 36):x + w + 36].astype("int16")
    if crop.size == 0:
        return True
    return float(crop[..., 1].mean() - crop[..., 0].mean()) > 3.0


def _mark_left_of(img, box):
    """True (green check), False (red cross) or None for the 16 px icon just left of a requirement line."""
    x, y, w, h = [int(v) for v in box]
    crop = img[max(0, y - 2):y + h + 2, max(0, x - 22):max(1, x - 2)].astype("int16")
    if crop.size == 0:
        return None
    r, g, b = crop[..., 0], crop[..., 1], crop[..., 2]
    red = int(((r > 150) & (g < 90) & (b < 90)).sum())
    green = int(((g > 140) & (r < 120) & (b < 120)).sum())
    if red >= 6 and red > green:
        return False
    if green >= 6 and green > red:
        return True
    return None


def unmet_requirements(req):
    """The requirements the game shows in red that really block the focus. A red line inside a 'Requires one of the
    following' group blocks only when no line of the group is green (Anschluss shows 'Heed von Neurath's Concerns'
    red next to 'Reorganize the Wehrmacht' green); a red note such as 'Cannot be cancelled manually' is
    information, not a condition."""
    unmet, group, groups = [], None, []
    for x in req:
        text = x["text"]
        if ONE_OF_RX.search(text):
            group = []
            groups.append(group)
            continue
        if group is not None and BULLET_RX.match(text):
            group.append(x)
            continue
        group = None
        if x["met"] is False and not INFO_RX.search(text):
            unmet.append(text)
    for g in groups:
        if g and not any(i["met"] for i in g):
            unmet.append("one of: " + "; ".join(i["text"].lstrip("-• ").strip() for i in g))
    return unmet


def read_detail(session, img=None):
    """The focus detail window: title, days, the Start button, requirements (met = green, unmet = red),
    description and effects. None when no detail window is open."""
    w, h = session.window.size()
    if img is None:
        session.input.move(int(w * 0.4), 125)             # rest the cursor on the window's title bar
        time.sleep(0.5)
        img = session.grab()
    start = find_detail(session, img)
    if not start:
        return None
    anchor = (start[0], start[1])
    out = {"start_button": start, "window": _rel(anchor, "window")}
    t = session.ocr.recognize(img, region=_rel(anchor, "title"), scale=2.0)
    out["title"] = " ".join(l.text for l in t.lines)
    d = session.ocr.recognize(img, region=_rel(anchor, "days"), scale=2.5)
    m = re.search(r"(\d+)", " ".join(l.text for l in d.lines))
    out["days"] = int(m.group(1)) if m else None
    r = session.ocr.recognize(img, region=_rel(anchor, "requirements"), scale=2.0)
    req = []
    for l in r.lines:
        if EFFECT_LINE_RX.search(l.text):
            break                                   # the region reaches into the effects: nothing below is a condition
        col = panels.text_colour(img, l.box)
        met = {"green": True, "red": False}.get(col)
        if met is None:
            met = _mark_left_of(img, l.box)          # white text with a green check / red cross icon in front
        req.append({"text": l.text, "met": met, "colour": col})
    out["start_enabled"] = _start_enabled(img, start)
    if not out["start_enabled"]:
        # the requirement box shows only four lines: scroll it to read the rest (the blocking one is below)
        box = _rel(anchor, "requirements")
        seen = {x["text"] for x in req}
        for _ in range(3):
            session.input.scroll(box[0] + box[2] // 2, box[1] + box[3] // 2, 2)
            time.sleep(0.4)
            img2 = session.grab()
            r2 = session.ocr.recognize(img2, region=box, scale=2.0)
            added = False
            for l in r2.lines:
                if EFFECT_LINE_RX.search(l.text):
                    continue
                if l.text not in seen:
                    seen.add(l.text)
                    met = {"green": True, "red": False}.get(panels.text_colour(img2, l.box))
                    req.append({"text": l.text, "met": met if met is not None else _mark_left_of(img2, l.box),
                                "colour": panels.text_colour(img2, l.box)})
                    added = True
            if not added:
                break
    out["requirements"] = req
    out["unmet"] = unmet_requirements(req)
    b = session.ocr.recognize(img, region=_rel(anchor, "body"), scale=1.6)
    body = [l.text for l in b.lines]
    eff = next((i for i, l in enumerate(body) if re.search(r"^\W*effects?\W*$", l, re.I)), None)
    out["description"] = " ".join(body[:eff] if eff is not None else body)
    out["effects"] = body[eff + 1:] if eff is not None else []
    return out


def title_score(title, name):
    """1.0 for the same label, else a similarity that penalises extra words ('Air War Plans Division' is
    not 'War Plans Division')."""
    a, b = panels.norm(title), panels.norm(name)
    if not a or not b:
        return 0.0
    if a == b:
        return 1.0
    import difflib
    ratio = difflib.SequenceMatcher(None, a, b).ratio()
    return ratio - 0.04 * abs(len(a) - len(b))


def placeholder_pattern(name):
    """(search text, title regex) for a focus name with script placeholders such as 'Establish the
    [GetReichsWerkeName]': the game shows the placeholder's value, so search for the longest static part and accept
    any title that has the static parts around something. (name, None) for a plain name."""
    if "[" not in name:
        return name, None
    parts = [p.strip() for p in re.split(r"\[[^\]]*\]", name)]
    query = max(parts, key=len) or name
    rx = re.compile("^\s*" + ".+".join(re.escape(p) for p in parts) + "\s*$", re.I)
    return query, rx


def open_detail(session, name, max_cycles=10, search_text=None):
    """Search the tree for `name` and open the detail window of the match whose title is `name` (cycling
    through the matches with Tab). Returns (detail or None, found, tried titles). A name with placeholders is
    searched by its static part (or search_text) and accepted by pattern."""
    query, rx = placeholder_pattern(name)
    found = search(session, search_text or query)
    if not found or found[1] == 0:
        return None, found, []
    n = min(max_cycles, found[1])
    tried = []
    best = (0.0, None)
    for i in range(n):
        session.input.key("enter")
        time.sleep(1.0)
        d = read_detail(session)
        score = (1.0 if rx and d and rx.search(d.get("title") or "") else title_score(d.get("title"), name)) if d else 0.0
        tried.append({"index": i + 1, "title": d.get("title") if d else None, "score": round(score, 2)})
        if d and score >= 0.97:
            return d, found, tried
        if d:
            session.input.key("esc")
            time.sleep(0.6)
        if score > best[0]:
            best = (score, i)
        session.input.key("tab")
        time.sleep(0.7)
    if best[1] is not None and best[0] >= 0.8:
        # back at the first match after n tabs: move to the best one and open it
        session.input.key("tab", times=best[1]) if best[1] else None
        time.sleep(0.5)
        session.input.key("enter")
        time.sleep(1.0)
        return read_detail(session), found, tried
    return None, found, tried


def close_detail(session):
    """Esc closes the detail window (the tree stays open)."""
    session.input.key("esc")
    time.sleep(0.6)


def start(session, detail):
    """Click Start in the open detail window."""
    if not detail.get("start_button"):
        raise ToolError("the focus detail window has no Start button (the focus may be completed, in progress or "
                        "not available)", {"detail": detail})
    x, y, w, h = detail["start_button"]
    session.input.click(x + w // 2, y + h // 2)
    time.sleep(1.2)
    session.park_cursor()
    # starting a focus while another one runs asks to cancel that one
    return panels.answer_dialog(session, accept=True, attempts=1)


def cancel_current(session):
    """Cancel the focus in progress with the small x on the politics panel's focus button."""
    panels.open_panel(session, "politics")
    x, y, w, h = _button_region(session)
    session.input.click(x + w - 23, y + 19)
    time.sleep(1.0)
    session.park_cursor()
    return panels.answer_dialog(session, accept=True)
