"""Side panels and full-screen views on the map: which one is open, open one by its key, close it, and get back to
the bare map without ever pressing Esc there (Esc on the bare map opens the pause menu).

Panels are named from their title, read by OCR in the strip under the top bar (perception.screen.PANELS). The
vanilla keys toggle them (screen.PANEL_KEYS). The focus tree and the tech tree have no key: they close with their
close button or Esc (Esc is safe while a view is open: it closes the topmost window).

Also: colour helpers that tell red (unmet, unaffordable) from green (met) text in tooltips and lists.
"""

import re
import time

import numpy as np

from ..errors import ToolError
from ..perception import screen as screen_mod
from . import menu


def title_region(session):
    w, h = session.window.size()
    return [0, 84, min(w, 1100), 40]


def read_title(session, img=None, attempts=2):
    """(panel name or None, title text) from the title strip."""
    img = session.grab() if img is None else img
    w, h = img.shape[1], img.shape[0]
    region = title_region(session)
    for i in range(attempts):
        res = session.ocr.recognize(img, region=region, scale=2.0 if i == 0 else 2.6)
        name, title = screen_mod.detect_panel(res.lines, w, h)
        if name:
            return name, title
        if i + 1 < attempts:
            continue
    texts = " ".join(l.text for l in res.lines)
    return None, texts or None


def current(session, img=None):
    return read_title(session, img)[0]


def wait_panel(session, name, timeout=4.0, present=True):
    t0 = time.time()
    last = None
    while time.time() - t0 < timeout:
        session.check_cancel()
        last = current(session)
        if (last == name) == present:
            return last
        time.sleep(0.35)
    return last


def open_panel(session, name, settle=1.2):
    """Open a panel by its key (closing another first) and wait until its title shows. Returns the title."""
    key = screen_mod.PANEL_KEYS.get(name)
    if not key:
        raise ToolError(f"the {name} view has no key; open it through its button")
    cur = current(session)
    if cur is None:
        dismiss_notices(session)             # a "National Focus Completed" window takes the keys
    for _ in range(3):                       # a full-screen view may sit over another panel
        if cur == name:
            return name
        if not cur:
            break
        close(session, cur)
        cur = current(session)
    session.input.key(key)
    time.sleep(settle)                       # panels slide in
    got = wait_panel(session, name, timeout=4.0)
    if got != name:
        session.input.key(key)               # a busy game can drop the first key
        time.sleep(settle)
        got = wait_panel(session, name, timeout=4.0)
    if got != name:
        img = session.grab()
        from ..desktop import images
        png, _, _ = images.encode(img, 960)
        raise ToolError(f"the {name} panel did not open (title strip shows {got})", {"panel": got}, image=png)
    session.park_cursor()
    return name


# close buttons of views without a key, relative to the client's top-right corner (vanilla, gui_scale 1)
CLOSE_BUTTONS = {"focus_tree": (-30, 107), "diplomacy": None}


def close(session, name=None):
    """Close the open panel or view: its key when it has one, its close button for full-screen views (a search
    field may hold the keyboard there), else Esc (only while a view is open)."""
    cur = current(session)
    if cur is None:
        return None
    key = screen_mod.PANEL_KEYS.get(cur)
    w, h = session.window.size()
    if key:
        session.input.key(key)
    elif CLOSE_BUTTONS.get(cur):
        dx, dy = CLOSE_BUTTONS[cur]
        session.input.click(w + dx, dy)
        session.park_cursor()
    elif cur == "diplomacy":
        session.input.click(517, 103)        # the panel's close button (left panel, vanilla)
        session.park_cursor()
    else:
        session.input.key("esc")
    time.sleep(0.8)
    if menu.esc_menu_open(session)[0]:      # Esc reached the bare map: close the pause menu again
        session.input.key("esc")
        time.sleep(0.6)
    return cur


def to_map(session, max_steps=4):
    """Close views until no panel title shows (never presses Esc on the bare map)."""
    closed = []
    for _ in range(max_steps):
        c = close(session)
        if not c:
            break
        closed.append(c)
    return closed


# ---------------------------------------------------------------------------------------------- colours
def text_colour(img, box):
    """Dominant colour class of the glyphs inside an OCR box: 'red', 'green', 'yellow', 'white' or 'grey'.
    HOI4 colours unmet requirements and unaffordable costs red and met ones green."""
    x, y, w, h = [int(v) for v in box]
    H, W = img.shape[:2]
    crop = img[max(0, y):min(H, y + h), max(0, x):min(W, x + w)].astype(np.int16)
    if crop.size == 0:
        return None
    r, g, b = crop[..., 0], crop[..., 1], crop[..., 2]
    bright = crop.max(2) > 110
    if bright.sum() < 4:
        return "grey"
    red = ((r > 150) & (g < 110) & (b < 110) & bright).sum()
    green = ((g > 130) & (r < 140) & (b < 140) & (g > r + 25) & bright).sum()
    yellow = ((r > 170) & (g > 140) & (b < 110) & bright).sum()
    white = ((r > 170) & (g > 170) & (b > 170)).sum()
    counts = {"red": red, "green": green, "yellow": yellow, "white": white}
    best = max(counts, key=counts.get)
    return best if counts[best] >= 3 else "grey"


def mark_colours(img, lines):
    """[{text, box, colour}] for OCR lines."""
    return [{"text": l.text, "box": l.box, "colour": text_colour(img, l.box)} for l in lines]


_NORM = str.maketrans({"0": "o", "1": "l", "|": "l", "i": "l", "5": "s", "8": "b"})


def norm(text):
    return re.sub(r"[^a-z0-9]+", "", (text or "").lower().translate(_NORM))


def similar(a, b):
    """Similarity 0..1 of two labels after normalisation (OCR noise tolerant)."""
    import difflib
    na, nb = norm(a), norm(b)
    if not na or not nb:
        return 0.0
    if na == nb:
        return 1.0
    if len(na) >= 6 and (na in nb or nb in na):
        return 0.9
    return difflib.SequenceMatcher(None, na, nb).ratio()


# ---------------------------------------------------------------------------------------------- dialogs
OK_RX = re.compile(r"^\W*([o0]k|yes|confirm|accept|send|agree)\W*$", re.I)
CANCEL_RX = re.compile(r"^\W*(cancel|no|decline|back)\W*$", re.I)


def find_dialog(session, img=None):
    """A centred confirmation dialog: {'title', 'text', 'ok': box, 'cancel': box} or None."""
    img = session.grab() if img is None else img
    h, w = img.shape[:2]
    region = [int(w * 0.25), int(h * 0.2), int(w * 0.5), int(h * 0.6)]
    res = session.ocr.recognize(img, region=region, scale=1.5)
    ok = [l for l in res.lines if OK_RX.search(l.text)]
    cancel = [l for l in res.lines if CANCEL_RX.search(l.text)]
    if not ok or not cancel:
        return None
    okl = ok[-1]
    near = [c for c in cancel if abs(c.center[1] - okl.center[1]) < 25]
    if not near:
        return None
    x0, x1 = min(near[0].box[0], okl.box[0]) - 150, max(near[0].box[0], okl.box[0]) + okl.box[2] + 150
    texts = [l.text for l in res.lines if okl.box[1] - 360 < l.box[1] < okl.box[1] - 10 and x0 <= l.center[0] <= x1]
    return {"title": texts[0] if texts else None, "text": " ".join(texts), "ok": okl.box, "cancel": near[0].box}


def answer_dialog(session, accept=True, wait=1.0, attempts=3):
    """Click OK (accept) or Cancel on a confirmation dialog if one is open. Returns the dialog or None."""
    for _ in range(attempts):
        d = find_dialog(session)
        if d:
            x, y, bw, bh = d["ok" if accept else "cancel"]
            session.input.click(x + bw // 2, y + bh // 2)
            time.sleep(wait)
            session.park_cursor()
            d["answered"] = "ok" if accept else "cancel"
            return d
        time.sleep(0.4)
    return None


NOTICE_OK_RX = re.compile(r"^\W*([o0]k|close|continue)\W*$", re.I)


def find_notice(session, img=None):
    """A centred notification window with a single OK button ("National Focus Completed", research done ...),
    which the parchment popup detector does not see. {'title', 'text', 'ok'} or None."""
    img = session.grab() if img is None else img
    h, w = img.shape[:2]
    region = [int(w * 0.28), int(h * 0.25), int(w * 0.44), int(h * 0.5)]
    res, oks = None, []
    for scale in (1.0, 2.0, 1.5):                  # Windows OCR sometimes returns nothing at one scale
        res = session.ocr.recognize(img, region=region, scale=scale)
        oks = [l for l in res.lines if NOTICE_OK_RX.search(l.text)]
        # confirm the button where it was read (a large crop can map a word box off by a line)
        oks = [l for l in oks if any(NOTICE_OK_RX.search(c.text) for c in session.ocr.recognize(
            img, region=[max(0, l.box[0] - 30), max(0, l.box[1] - 12), l.box[2] + 60, l.box[3] + 24], scale=3.0).lines)]
        if oks:
            break
    if not oks:
        return None
    ok = oks[-1]
    if any(CANCEL_RX.search(l.text) and abs(l.center[1] - ok.center[1]) < 25 for l in res.lines):
        return None                                   # a confirmation: answer_dialog decides
    above = [l for l in res.lines if ok.box[1] - 260 < l.box[1] < ok.box[1] - 8]
    if not above:
        return None
    first = min(l.box[1] for l in above)
    title = " ".join(l.text for l in sorted(above, key=lambda l: l.box[0]) if l.box[1] - first < 8)
    return {"title": title, "text": " ".join(l.text for l in above), "ok": ok.box}


def dismiss_notices(session, max_count=4):
    """Click OK on notification windows until none is left. Returns their titles."""
    out = []
    for _ in range(max_count):
        n = find_notice(session)
        if not n:
            break
        x, y, bw, bh = n["ok"]
        session.input.click(x + bw // 2, y + bh // 2)
        time.sleep(0.8)
        session.park_cursor()
        out.append(n["title"])
    return out
