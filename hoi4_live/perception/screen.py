"""Screen classification and text lookup.

Phase 1 classes, from cheapest evidence first:
    map                 the top-bar date reads (a game is loaded and the interface is shown)
    esc_menu            the pause menu (Resume / Save Game / Load Game ...) is open over the map
    main_menu           the front end (Single Player / Multiplayer / Quit)
    single_player_menu  Continue / New Game / Load / Tutorial
    load_screen         the save list with Load / Back to Menu
    loading             none of the above while game.log is busy
    unknown
plus flags: console_open, popups (from perception.popups).

Phase 2 adds, on the map:
    panel               the open side panel or full-screen view, from its title (top-left strip): politics,
                        decisions, intelligence, research, trade, construction, production, deployment, logistics,
                        officer_corps, faction, army_overview, navy_overview, air_overview, focus_tree, diplomacy
    window              any other window in the centre (a large neutral-dark framed box), with its title
                        (mod windows such as a welcome screen, confirmation dialogs)
"""

import re

import numpy as np

from . import topbar

MAIN_MENU_WORDS = re.compile(r"single\s*player|multi\s*player|tutorial|credits|exit|mods|settings", re.I)
ESC_MENU_WORDS = re.compile(r"resume|save\s*game|load\s*game|settings|quit|exit to|surrender|options", re.I)


CONSOLE_FRAME = (39, 36, 33)        # the console's flat brown-grey frame (8 px wide at gui_scale 1)


def console_rect(img):
    """[x0, y0, x1, y1] of the open console's frame, or None. The console is a translucent panel in the
    top-left quarter whose border is a uniform brown-grey band; long rows and columns of exactly that colour
    only exist while it is open."""
    h, w = img.shape[:2]
    X1, Y0, Y1 = int(w * 0.4), int(h * 0.05), int(h * 0.7)
    a = img[Y0:Y1, :X1].astype(np.int16)
    m = ((np.abs(a[..., 0] - CONSOLE_FRAME[0]) <= 9) & (np.abs(a[..., 1] - CONSOLE_FRAME[1]) <= 9) &
         (np.abs(a[..., 2] - CONSOLE_FRAME[2]) <= 9) & (a[..., 0] >= a[..., 2]))
    rows = np.flatnonzero(m.sum(1) > w * 0.15)
    cols = np.flatnonzero(m.sum(0) > h * 0.2)

    def groups(idx):
        if idx.size == 0:
            return []
        cut = np.flatnonzero(np.diff(idx) > 1)
        starts = np.concatenate(([0], cut + 1))
        ends = np.concatenate((cut, [idx.size - 1]))
        return [(int(idx[a]), int(idx[b])) for a, b in zip(starts, ends)]

    rg, cg = groups(rows), groups(cols)
    band = lambda g: 3 <= g[1] - g[0] + 1 <= 14          # a frame side is a band a few pixels thick
    rg = [g for g in rg if band(g)]
    cg = [g for g in cg if band(g)]
    if len(rg) < 2 or len(cg) < 2:
        return None
    (t0, t1), (b0, b1) = rg[0], rg[-1]
    (l0, l1), (r0, r1) = cg[0], cg[-1]
    x0, x1, y0, y1 = l0, r1, t0 + Y0, b1 + Y0
    if x1 - x0 < w * 0.12 or y1 - y0 < h * 0.15 or y0 < h * 0.08:
        return None
    inner = m[t1 + 4:b0 - 3, l1 + 4:r0 - 3]
    if inner.size == 0 or float(inner.mean()) > 0.2:
        return None                                       # a solid panel, not a frame around a see-through log
    return [x0, y0, x1, y1]


def console_open(img):
    return console_rect(img) is not None


_LETTERS = str.maketrans({"0": "O", "1": "I", "5": "S", "8": "B", "|": "I"})


def text_variants(text):
    """The OCR text and a letter-normalised copy (OCR reads OK as 0K, Il as 11 ...)."""
    alt = text.translate(_LETTERS)
    return (text, alt) if alt != text else (text,)


def matches(rx, text):
    return any(rx.search(t) for t in text_variants(text))


def find_text(ocr_result, pattern):
    try:
        rx = re.compile(pattern, re.I)
    except re.error:
        rx = re.compile(re.escape(pattern), re.I)
    return [l for l in ocr_result.lines if matches(rx, l.text)]


# panel titles as the game draws them (English UI); checked in order, first match wins
PANELS = [
    ("focus_tree", re.compile(r"focus\s*tree", re.I)),
    ("politics", re.compile(r"^\W*political\W*$", re.I)),
    ("decisions", re.compile(r"^\W*decisions\W*$|events.{0,6}decisions", re.I)),
    ("intelligence", re.compile(r"intelligence\s*agency", re.I)),
    ("research", re.compile(r"^\W*research\W*$|research\s*slots", re.I)),
    ("trade", re.compile(r"^\W*trade\W*$", re.I)),
    ("construction", re.compile(r"^\W*construction\W*$", re.I)),
    ("production", re.compile(r"^\W*production\W*$", re.I)),
    ("deployment", re.compile(r"recruit\s*&?\s*deploy", re.I)),
    ("logistics", re.compile(r"^\W*logistics\W*$", re.I)),
    ("officer_corps", re.compile(r"officer\s*corps", re.I)),
    ("faction", re.compile(r"^\W*faction\W*$", re.I)),
    ("army_overview", re.compile(r"division\s*overview", re.I)),
    ("navy_overview", re.compile(r"^\W*navy\W*$|\bfleets\b", re.I)),
    ("air_overview", re.compile(r"forces\s*overview|air\s*wings", re.I)),
    ("diplomacy", re.compile(r"^\W*diplomacy\W*$", re.I)),
]
PANEL_KEYS = {"politics": "q", "decisions": "shift+q", "intelligence": "shift+w", "research": "w", "trade": "r",
              "construction": "t", "production": "y", "deployment": "u", "logistics": "i",
              "officer_corps": "shift+e", "faction": "ctrl+q", "army_overview": "o", "navy_overview": "p",
              "air_overview": "l", "focus_tree": None, "diplomacy": None}


def detect_panel(lines, w, h):
    """(panel name, title text) from OCR lines of the whole screen: titles sit in the strip under the top bar."""
    strip = [l for l in lines if 60 <= l.center[1] <= 200 and l.center[0] <= min(w, 1100)]
    for name, rx in PANELS:
        for l in strip:
            if matches(rx, l.text):
                return name, l.text
    # a title the OCR split in words ("RECRUIT" "DEPLOY": the ampersand is dropped) matches on the strip's joined text
    left = sorted((l for l in strip if l.center[0] <= 560), key=lambda l: (l.center[1] // 12, l.box[0]))
    joined = " ".join(l.text for l in left)
    if joined:
        for name, rx in PANELS:
            if matches(rx, joined) and name not in ("politics", "research", "trade", "production", "construction",
                                                     "logistics", "faction", "decisions", "diplomacy"):
                return name, joined
    return None, None


def dark_window_box(img, min_frac=0.06):
    """[x, y, w, h] of a large neutral-dark window in the centre of the screen (HOI4 window paper), or None. The
    map (blue sea, coloured land) and the side panels are excluded by colour and position."""
    import cv2
    h, w = img.shape[:2]
    a = img[::2, ::2].astype(np.int16)
    mx, mn = a.max(2), a.min(2)
    m = ((mx - mn < 16) & (mx > 12) & (mx < 80)).astype(np.uint8)
    m = cv2.morphologyEx(m, cv2.MORPH_CLOSE, np.ones((9, 9), np.uint8))
    n, labels, stats, _ = cv2.connectedComponentsWithStats(m, connectivity=8)
    best = None
    for i in range(1, n):
        x, y, bw, bh, area = [int(v) * (2 if k < 4 else 4) for k, v in enumerate(stats[i])]
        if area < min_frac * w * h or bw < w * 0.15 or bh < h * 0.15:
            continue
        cx, cy = x + bw / 2, y + bh / 2
        if not (w * 0.25 < cx < w * 0.75 and h * 0.15 < cy < h * 0.85):
            continue
        fill = area / float(bw * bh)
        if fill < 0.55:
            continue
        if best is None or area > best[0]:
            best = (area, [x, y, bw, bh])
    return best[1] if best else None


SAVE_ROW_DATE = re.compile(r"\d{1,2}:\d{2},?\s*\d{1,2}\s+[A-Za-z]+,?\s*\d{4}")


def classify(img, ocr, game_log_idle=None):
    """Return {"screen": ..., "date": ..., "console_open": bool, "evidence": [...]}.

    Screens: map, esc_menu, load_screen (save list + Load button, before or in game), main_menu,
    single_player_menu (Continue / New Game / Load / Tutorial), loading, unknown."""
    out = {"console_open": console_open(img)}
    d = topbar.read_date(img, ocr)
    if "error" not in d:
        out["date"] = d
    h, w = img.shape[:2]
    res = ocr.recognize(img, scale=1.0)
    texts = [l.text for l in res.lines]
    low = [t.lower() for t in texts]
    has = lambda rx: any(re.search(rx, t, re.I) for t in texts)
    center = [l.text for l in res.lines if w * 0.3 < l.center[0] < w * 0.7 and h * 0.15 < l.center[1] < h * 0.85]
    esc_hits = [t for t in center if ESC_MENU_WORDS.search(t)]
    save_rows = [t for t in texts if SAVE_ROW_DATE.search(t)]
    if "date" in out and len(esc_hits) >= 3:
        out["screen"] = "esc_menu"
    elif has(r"back\s*to\s*menu") or (len(save_rows) >= 2 and has(r"^\W*load\W*$")) or \
            (has(r"^\W*local\W*$") and has(r"^\W*cloud") and has(r"^\W*load\W*$") and has(r"^\W*back\W*$")):
        out["screen"] = "load_screen"
    elif "date" in out:
        out["screen"] = "map"
    elif has(r"single\s*player") and has(r"multi\s*player"):
        out["screen"] = "main_menu"
    elif has(r"^\W*continue\W*$") or (has(r"new\s*game") and has(r"tutorial")):
        out["screen"] = "single_player_menu"
    elif has(r"^\W*loading") or (game_log_idle is not None and game_log_idle < 3):
        out["screen"] = "loading"
    else:
        out["screen"] = "unknown"
    if out["screen"] in ("map", "unknown"):
        name, title = detect_panel(res.lines, w, h) if out["screen"] == "map" else (None, None)
        if name:
            out["panel"] = name
            out["panel_title"] = title
        box = dark_window_box(img)
        if box and name != "focus_tree":
            x, y, bw, bh = box
            inside = [l.text for l in res.lines if x <= l.center[0] <= x + bw and y <= l.center[1] <= y + bh]
            out["window"] = {"box": box, "title": inside[0] if inside else None, "text": inside[:12]}
    out["evidence"] = texts[:15]
    return out
