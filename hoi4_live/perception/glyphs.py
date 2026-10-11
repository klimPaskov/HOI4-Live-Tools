"""Read short numbers drawn in a known game font by template matching (no OCR).

Windows OCR misses lone digits ("0" experience, "9" political power) and reads "%" as "0/0" in the small top-bar
boxes. The top bar always draws its values with the bitmap font hoi_18mbs (interface/topbar.gui), whose glyphs ship
with the game as a BMFont table (gfx/fonts/<font>.fnt) and a texture (.dds): the alpha channel of each glyph cell
is an exact template of what the game draws at gui_scale 1. Matching every template against the text's brightness
(any colour: white, green, red, yellow) and keeping the best non-overlapping hits gives the string.
"""

import os
import re

import cv2
import numpy as np

CHARSET = "0123456789%.,KMBdh-+/:"
SMALL = ".,-:"


class Font:
    def __init__(self, fnt_path):
        from PIL import Image
        with open(fnt_path, "rb") as f:
            text = f.read().decode("latin-1")
        page = re.search(r'page id=0 file="([^"]+)"', text)
        tex = os.path.join(os.path.dirname(fnt_path), page.group(1)) if page else fnt_path[:-4] + ".dds"
        if not os.path.exists(tex):
            tex = fnt_path[:-4] + ".dds"
        img = Image.open(tex).convert("RGBA")
        alpha = np.asarray(img)[..., 3].astype(np.float32) / 255.0
        self.templates = {}
        for m in re.finditer(r"char id=(\d+)\s+x=(\d+)\s+y=(\d+)\s+width=(\d+)\s+height=(\d+)\s+xoffset=(-?\d+)\s+"
                             r"yoffset=(-?\d+)\s+xadvance=(\d+)", text):
            cid, x, y, w, h, xo, yo, adv = [int(v) for v in m.groups()]
            ch = chr(cid)
            if ch not in CHARSET or w == 0 or h == 0:
                continue
            t = alpha[y:y + h, x:x + w]
            rows = np.flatnonzero(t.max(1) > 0.15)
            cols = np.flatnonzero(t.max(0) > 0.15)
            if rows.size == 0 or cols.size == 0:
                continue
            t = t[rows[0]:rows[-1] + 1, cols[0]:cols[-1] + 1]
            self.templates[ch] = {"t": t, "top": yo + rows[0], "adv": adv}


_FONTS = {}


def font(game_dir, name="hoi_18mbs"):
    key = (game_dir, name)
    if key not in _FONTS:
        p = os.path.join(game_dir or "", "gfx", "fonts", name + ".fnt")
        _FONTS[key] = Font(p) if os.path.exists(p) else None
    return _FONTS[key]


def brightness(crop):
    """Text strength 0..1: the brightest channel, stretched between the dark background and the text."""
    v = crop.astype(np.float32).max(2)
    lo, hi = np.percentile(v, 35), np.percentile(v, 99.5)
    if hi - lo < 40:
        return np.zeros_like(v)
    return np.clip((v - lo) / (hi - lo), 0, 1)


def read(crop, fnt, threshold=0.62, charset=CHARSET):
    """String drawn in `fnt` inside crop (RGB array), '' when nothing matches."""
    if fnt is None or crop.size == 0:
        return ""
    v = brightness(crop)
    if not v.any():
        return ""
    v = np.pad(v, 3)
    hits = []
    for ch in charset:
        tpl = fnt.templates.get(ch)
        if tpl is None:
            continue
        t = tpl["t"]
        if t.shape[0] > v.shape[0] or t.shape[1] > v.shape[1]:
            continue
        res = cv2.matchTemplate(v, t, cv2.TM_CCOEFF_NORMED)
        # flat patches (background, solid frame) correlate as 1.0: keep only patches with real contrast
        mean = cv2.blur(v, (t.shape[1], t.shape[0]))
        sq = cv2.blur(v * v, (t.shape[1], t.shape[0]))
        sd = np.sqrt(np.maximum(sq - mean * mean, 0))
        oy, ox = t.shape[0] // 2, t.shape[1] // 2
        sd = sd[oy:oy + res.shape[0], ox:ox + res.shape[1]]
        res = np.where(sd[:res.shape[0], :res.shape[1]] > 0.15, res, 0)
        thr = threshold + (0.18 if ch in SMALL else 0.0)
        ys, xs = np.where(res >= thr)
        for y, x in zip(ys, xs):
            hits.append((float(res[y, x]), int(x), int(y), ch, t.shape[1], t.shape[0]))
    if not hits:
        return ""
    big = sorted((h for h in hits if h[3] not in SMALL), key=lambda h: -h[0])
    small = sorted((h for h in hits if h[3] in SMALL), key=lambda h: -h[0])
    taken = []

    def free(x, w):
        for _, tx, _, _, tw, _ in taken:
            if min(x + w, tx + tw) - max(x, tx) > 0.35 * min(w, tw):
                return False
        return True
    for h in big:
        if free(h[1], h[4]):
            taken.append(h)
    # glyph rows: keep hits on the main text line (big glyphs define it)
    if taken:
        base = np.median([h[2] + h[5] for h in taken])
        taken = [h for h in taken if abs(h[2] + h[5] - base) <= 4]
    for h in small:
        if free(h[1], h[4]) and (not taken or abs(h[2] + h[5] - base) <= 5 or h[3] == "-"):
            taken.append(h)
    taken.sort(key=lambda h: h[1])
    clusters, cur = [], []
    for h in taken:
        if cur and h[1] - (cur[-1][1] + cur[-1][4]) > 7:
            clusters.append(cur)
            cur = []
        cur.append(h)
    if cur:
        clusters.append(cur)
    if clusters:
        taken = max(clusters, key=lambda c: (sum(1 for h in c if h[3] not in SMALL), sum(h[0] for h in c)))
    out = ""
    prev_end = None
    for _, x, _, ch, w, _ in taken:
        if prev_end is not None and x - prev_end > 4:
            out += " "
        out += ch
        prev_end = x + w
    return out
