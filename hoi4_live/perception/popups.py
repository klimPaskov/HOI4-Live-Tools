"""Popup detection: event, news and super-event windows and their option buttons, from one screenshot.

HOI4 popups are paper windows over the map: country events on warm parchment, news events on grey newsprint
("World News"), super-events (and a mod's wide report layout) on grey paper in a
landscape window. Option buttons are dark olive bars with light text.

Detection (resolution independent; all sizes are fractions of the client height):
  1. Paper masks (warm, grey) on a half-size image; morphological closing fills the text strokes.
  2. Components that are large and window-like become candidate windows.
  3. Buttons: inside each candidate (searched with a margin), rows whose longest dark run is wide form bands with a
     stable x-extent; bands are split into single buttons at the light separator rows of their left edge strip.
     A button must sit on paper (paper on its left or right) and contain light text pixels.
  4. kind: warm paper -> event; grey paper in portrait -> news; grey landscape -> super. OCR can refine this.
  5. Stacked popups: candidates are ordered so the most complete window (straight edges, high fill) comes first.
     Answering verifies by re-detecting, so a wrong stacking guess only changes which popup is answered first.

Reading (title, body, option texts) uses OCR on the window and button boxes; see read_popup().
"""

import re

import cv2
import numpy as np


class Button:
    __slots__ = ("box", "text")

    def __init__(self, box, text=None):
        self.box = [int(v) for v in box]      # x, y, w, h in client pixels
        self.text = text

    @property
    def center(self):
        x, y, w, h = self.box
        return x + w // 2, y + h // 2

    def to_dict(self):
        d = {"box": self.box, "center": list(self.center)}
        if self.text is not None:
            d["text"] = self.text
        return d


class Popup:
    def __init__(self, kind, box, buttons, paper, completeness):
        self.kind = kind
        self.box = [int(v) for v in box]
        self.buttons = buttons
        self.paper = paper
        self.completeness = round(float(completeness), 3)
        self.title = None
        self.text = None
        self.stacked = False

    def to_dict(self, index=None):
        d = {"kind": self.kind, "box": self.box, "options": [b.to_dict() for b in self.buttons]}
        if index is not None:
            d["index"] = index
        if self.title is not None:
            d["title"] = self.title
        if self.text is not None:
            d["text"] = self.text
        if self.stacked:
            d["stacked"] = True
        d["completeness"] = self.completeness
        return d


def _paper_masks(a):
    r = a[..., 0].astype(np.int16)
    g = a[..., 1].astype(np.int16)
    b = a[..., 2].astype(np.int16)
    warm = (r > 160) & (g > 130) & (b > 80) & (r - b > 42) & (r - b < 118) & (r >= g) & (g >= b) & (g - b > 18)
    grey = (r > 145) & (g > 135) & (b > 120) & (r - b >= 3) & (r - b < 42) & (r >= g - 3) & (g >= b - 3) & (r < 242)
    return warm, grey


def _luma(a):
    a = a.astype(np.int16)
    return (a[..., 0] * 3 + a[..., 1] * 6 + a[..., 2]) // 10


def _edge_straightness(comp):
    """Fraction of rows/cols whose first/last paper pixel sits on the bounding box edge, per side."""
    h, w = comp.shape
    rows = comp[int(h * 0.1):max(int(h * 0.9), int(h * 0.1) + 1)]
    cols = comp[:, int(w * 0.1):max(int(w * 0.9), int(w * 0.1) + 1)]
    tol = 3
    has_r = rows.any(1)
    has_c = cols.any(0)
    left = np.argmax(rows, 1)
    right = rows.shape[1] - 1 - np.argmax(rows[:, ::-1], 1)
    top = np.argmax(cols, 0)
    bottom = cols.shape[0] - 1 - np.argmax(cols[::-1], 0)
    def frac(v, target, has):
        return float(((np.abs(v - target) <= tol) & has).mean()) if v.size else 0.0
    return (frac(left, 0, has_r), frac(right, rows.shape[1] - 1, has_r),
            frac(top, 0, has_c), frac(bottom, cols.shape[0] - 1, has_c))


def _runs(mask_row, min_len, max_gap=2):
    """(start, end_exclusive) runs of True in a 1-D bool array, bridging gaps up to max_gap."""
    xs = np.flatnonzero(mask_row)
    if xs.size < min_len:
        return []
    breaks = np.flatnonzero(np.diff(xs) > max_gap + 1)
    seg_starts = np.concatenate(([0], breaks + 1))
    seg_ends = np.concatenate((breaks, [xs.size - 1]))
    out = []
    for a, b in zip(seg_starts, seg_ends):
        x0, x1 = int(xs[a]), int(xs[b]) + 1
        if x1 - x0 >= min_len:
            out.append((x0, x1))
    return out


def find_buttons(img, unit, region=None):
    """Option buttons anywhere in the image (or inside region [x0, y0, x1, y1]).

    The HOI4 option button is one sprite (42 px tall at gui_scale 1) whose rims are unmistakable: the top rim is
    light / near-black / light (luma about 120, 25, 107) and the bottom rim near-black / dark grey / near-black
    (about 6, 33, 8). Text never touches the rims, so long horizontal runs of both rim patterns, paired at a
    plausible height, give exact button boxes even when text, flags or the window frame touch the button."""
    H, W = img.shape[:2]
    if region is None:
        region = [0, 0, W, H]
    x0, y0, x1, y1 = [int(v) for v in region]
    lum = _luma(img[y0:y1, x0:x1]).astype(np.int16)
    h = lum.shape[0]
    if h < 8:
        return []
    l0, l1, l2 = lum[:-2], lum[1:-1], lum[2:]
    bottom = (l0 < 24) & (l1 > 20) & (l1 < 52) & (l2 < 24) & (l1 - l0 > 12) & (l1 - l2 > 12)
    top = (l0 > 88) & (l0 < 165) & (l1 < 48) & (l2 > 80) & (l2 < 150) & (l0 - l1 > 45) & (l2 - l1 > 40)
    min_len = max(40, int(unit * 0.1))
    top_rows = np.flatnonzero(top.sum(1) >= min_len // 2)
    buttons = []
    dmin, dmax = int(unit * 0.026), int(unit * 0.075)
    for y in np.flatnonzero(bottom.sum(1) >= min_len):
        for a, b in _runs(bottom[y], min_len, max_gap=6):
            best = None
            for ty in top_rows:
                d = y - ty
                if dmin <= d <= dmax:
                    cover = float(top[ty, a:b].mean())
                    if cover >= 0.55 and (best is None or d < best[0]):
                        best = (d, int(ty), a, b)
            if best is None:
                continue
            d, ty, ta, tb = best
            bx0, bx1 = min(a, ta), max(b, tb)
            by0, by1 = ty, y + 3
            inner = lum[by0 + 3:by1 - 3, bx0:bx1]
            if inner.size == 0 or float((inner < 120).mean()) < 0.5:
                continue
            buttons.append(Button([x0 + bx0, y0 + by0, bx1 - bx0, by1 - by0]))
    uniq = []
    for btn in sorted(buttons, key=lambda b: (b.box[1], b.box[0])):
        if all(abs(btn.box[0] - u.box[0]) > 6 or abs(btn.box[1] - u.box[1]) > 6 for u in uniq):
            uniq.append(btn)
    return uniq


def _paper_around(paper, box, unit):
    """Fraction of paper pixels in a ring around a box (popup buttons sit on paper)."""
    H, W = paper.shape
    x, y, w, h = box
    m = max(6, int(unit * 0.02))
    X0, Y0, X1, Y1 = max(0, x - m), max(0, y - m), min(W, x + w + m), min(H, y + h + m)
    ring = paper[Y0:Y1, X0:X1].copy().astype(np.float32)
    ring[y - Y0:y - Y0 + h, x - X0:x - X0 + w] = np.nan
    vals = ring[~np.isnan(ring)]
    return float(vals.mean()) if vals.size else 0.0


def paper_windows(img, unit, top_bar=0, min_area_frac=0.012):
    """Paper window candidates [{"id", "paper", "box", "fill", "straight"}] and a label map (0 = none).

    The paper masks are closed to swallow text, then cut along long straight non-paper lines (window frames,
    picture borders, button rims), so windows stacked on top of each other separate even when they share a
    paper colour."""
    H, W = img.shape[:2]
    warm, grey = _paper_masks(img)
    nonpaper = (~(warm | grey)).astype(np.uint8)
    lv = max(25, int(unit * 0.12))
    lh = max(25, int(unit * 0.12))
    cut_v = cv2.morphologyEx(nonpaper, cv2.MORPH_OPEN, cv2.getStructuringElement(cv2.MORPH_RECT, (1, lv)))
    cut_h = cv2.morphologyEx(nonpaper, cv2.MORPH_OPEN, cv2.getStructuringElement(cv2.MORPH_RECT, (lh, 1)))
    cut = cv2.dilate(cut_v | cut_h, np.ones((3, 3), np.uint8))
    out = []
    labels = np.zeros((H, W), np.int32)
    kclose = cv2.getStructuringElement(cv2.MORPH_RECT, (7, 7))
    for paper, mask in (("warm", warm), ("grey", grey)):
        m = cv2.morphologyEx(mask.astype(np.uint8), cv2.MORPH_CLOSE, kclose)
        m[cut > 0] = 0
        m = cv2.morphologyEx(m, cv2.MORPH_OPEN, np.ones((5, 5), np.uint8))
        m[:top_bar] = 0
        n, lab, stats, _ = cv2.connectedComponentsWithStats(m, connectivity=4)
        for k in range(1, n):
            x, y, bw, bh, area = [int(v) for v in stats[k]]
            if area < min_area_frac * W * H or bw < unit * 0.18 or bh < unit * 0.12:
                continue
            comp = lab[y:y + bh, x:x + bw] == k
            fill = area / float(bw * bh)
            if fill < 0.25:
                continue
            straight = _edge_straightness(comp)
            if sorted(straight)[-2] < 0.3:            # at least two straight sides: a window, not terrain
                continue
            wid = len(out) + 1
            labels[y:y + bh, x:x + bw][comp] = wid
            out.append({"id": wid, "paper": paper, "box": [x, y, bw, bh], "fill": fill, "straight": straight})
    return out, labels


def _contains(outer, inner, slack=4):
    return (outer[0] - slack <= inner[0] and outer[1] - slack <= inner[1] and
            inner[0] + inner[2] <= outer[0] + outer[2] + slack and inner[1] + inner[3] <= outer[1] + outer[3] + slack)


def _inside(box, pt, mx=0, my=0):
    x, y, w, h = box
    return x - mx <= pt[0] <= x + w + mx and y - my <= pt[1] <= y + h + my


def detect(img, min_area_frac=0.012):
    """Popups visible in an RGB client image (numpy HxWx3). Topmost-guess first."""
    H, W = img.shape[:2]
    unit = H                                   # UI sizes scale with the client height at gui_scale 1
    top_bar = int(H * 0.045)
    buttons = find_buttons(img, unit, [0, top_bar, W, H])
    if not buttons:
        return []
    wins, labels = paper_windows(img, unit, top_bar, min_area_frac)
    warm_f, grey_f = _paper_masks(img)
    paper_full = warm_f | grey_f
    by_id = {w["id"]: w for w in wins}
    for w in wins:
        w["buttons"] = []
    loose = []
    m = max(8, int(unit * 0.05))
    for b in buttons:
        x, y, bw, bh = b.box
        ring = labels[max(0, y - m):min(H, y + bh + m), max(0, x - m):min(W, x + bw + m)]
        ids, counts = np.unique(ring[ring > 0], return_counts=True)
        owner = None
        if ids.size:
            owner = by_id[int(ids[int(np.argmax(counts))])]
        else:
            c = b.center
            near = [w for w in wins if _inside(w["box"], c, mx=int(w["box"][2] * 0.02), my=int(bh * 2.5))]
            if near:
                owner = max(near, key=lambda o: o["fill"])
        if owner is not None:
            owner["buttons"].append(b)
        elif _paper_around(paper_full, b.box, unit) > 0.25:
            loose.append(b)
    popups = []
    for w in wins:
        btns = w["buttons"]
        if not btns:
            continue
        box = w["box"]
        x0 = min([box[0]] + [b.box[0] for b in btns])
        y0 = min([box[1]] + [b.box[1] for b in btns])
        x1 = max([box[0] + box[2]] + [b.box[0] + b.box[2] for b in btns])
        y1 = max([box[1] + box[3]] + [b.box[1] + b.box[3] for b in btns])
        wbox = [x0, y0, x1 - x0, y1 - y0]
        aspect = wbox[2] / float(max(1, wbox[3]))
        kind = "event" if w["paper"] == "warm" else ("news" if aspect < 1.08 else "super")
        btns.sort(key=lambda b: (b.box[1], b.box[0]))
        completeness = w["fill"] * 0.5 + float(np.mean(w["straight"])) * 0.5
        popups.append(Popup(kind, wbox, btns, w["paper"], completeness))
    for b in loose:
        x, y, bw, bh = b.box
        popups.append(Popup("unknown", [x - 10, y - 10, bw + 20, bh + 20], [b], "unknown", 0.0))
    popups = _order(popups)
    if len(popups) > 1:
        for p in popups:
            p.stacked = any(_overlap(p.box, q.box) > 0 for q in popups if q is not p)
    return popups


def _order(popups):
    """Topmost first: a window lying inside another window's box is drawn over it; otherwise completeness."""
    def key(p):
        inside = sum(1 for q in popups if q is not p and _contains(q.box, p.box, slack=8))
        return (-inside, -p.completeness)
    return sorted(popups, key=key)


def _overlap(a, b):
    x0, y0 = max(a[0], b[0]), max(a[1], b[1])
    x1, y1 = min(a[0] + a[2], b[0] + b[2]), min(a[1] + a[3], b[1] + b[3])
    return max(0, x1 - x0) * max(0, y1 - y0)


# ---------------------------------------------------------------------- reading with OCR
_MASTHEAD = re.compile(r"^\W*world\W*news\W*$", re.I)
_QUOTE = re.compile(r"quote\s+of\s+the\s+day", re.I)


def read_popup(img, popup, ocr):
    """Fill popup.title, popup.text and each button's text with OCR. Returns the popup."""
    x, y, w, h = popup.box
    res = ocr.recognize(img, region=[x, y, w, h])
    body = []
    for b in popup.buttons:
        b.text = ""
    for line in res.lines:
        cx, cy = line.center
        owner = None
        for b in popup.buttons:
            bx, by, bw, bh = b.box
            if bx - 4 <= cx <= bx + bw + 4 and by - 2 <= cy <= by + bh + 2:
                owner = b
                break
        if owner is not None:
            owner.text = (owner.text + " " + line.text).strip()
        else:
            body.append(line)
    if any(_MASTHEAD.match(l.text) for l in body):
        popup.kind = "news"
        body = [l for l in body if not _MASTHEAD.match(l.text)]
    if any(_QUOTE.search(l.text) for l in body):
        popup.kind = "super"
    title = None
    if body:
        heights = sorted(l.box[3] for l in body)
        med = heights[len(heights) // 2]
        if popup.kind == "news":
            # the headline sits under the picture: the first line after the biggest vertical gap
            gaps = [(body[i + 1].box[1] - (body[i].box[1] + body[i].box[3]), i + 1) for i in range(len(body) - 1)]
            idx = max(gaps)[1] if gaps else 0
            title = body[idx]
        else:
            tall = [l for l in body if l.box[3] >= med * 1.12]
            title = tall[0] if tall else body[0]
    popup.title = title.text if title else ""
    popup.text = " ".join(l.text for l in body if l is not title).strip()
    return popup
