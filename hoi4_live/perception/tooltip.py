"""Tooltips: find the tooltip window that appeared after the cursor moved, and read it.

HOI4 tooltips are dark translucent boxes with a thin frame, drawn next to the cursor after a short delay. The box is
found from two screenshots (cursor parked, then hovering): the changed pixels, closed into solid blobs, give
rectangles; the tooltip is the largest well-filled rectangle near the cursor. Map animation (water, clouds) changes
scattered pixels that never form a filled rectangle.
"""

import re

import cv2
import numpy as np

KV = re.compile(r"^\s*(.+?)\s*:\s*(.+?)\s*$")


def find_box(before, after, cursor, max_dist=700, min_area=1200):
    """[x, y, w, h] of the tooltip, or None."""
    if before is None or after is None or before.shape != after.shape:
        return None
    d = (np.abs(after.astype(np.int16) - before.astype(np.int16)).max(2) > 22).astype(np.uint8)
    H, W = d.shape
    cx, cy = cursor
    if not (cx > W - 470 and cy < 50):
        d[0:48, W - 470:W - 60] = 0          # the date box flashes while paused; ignore it unless hovered
    d = cv2.morphologyEx(d, cv2.MORPH_CLOSE, np.ones((11, 11), np.uint8))
    n, labels, stats, _ = cv2.connectedComponentsWithStats(d, connectivity=8)
    best = None
    for i in range(1, n):
        x, y, w, h, area = stats[i]
        if area < min_area or w < 50 or h < 16:
            continue
        fill = area / float(w * h)
        if fill < 0.55:
            continue
        # distance from the cursor to the box
        dx = max(x - cx, 0, cx - (x + w))
        dy = max(y - cy, 0, cy - (y + h))
        if (dx * dx + dy * dy) ** 0.5 > max_dist:
            continue
        score = area * fill
        if best is None or score > best[0]:
            best = (score, [int(x), int(y), int(w), int(h)])
    return best[1] if best else None


def parse_lines(lines):
    """Split OCR lines into a title, plain lines and 'label: value' pairs."""
    texts = [l.strip() for l in lines if l and l.strip()]
    out = {"title": texts[0] if texts else None, "lines": texts, "values": {}}
    for t in texts[1:]:
        m = KV.match(t)
        if m and len(m.group(1)) <= 60:
            out["values"].setdefault(m.group(1), m.group(2))
    return out


def read(session, x, y, wait=0.9, radius=420, park=True):
    """Hover (x, y), find the tooltip and OCR it. Returns (result dict, crop image, full image)."""
    import time
    from ..desktop import images
    session.park_cursor()
    time.sleep(0.25)
    before = session.grab()
    session.input.move(x, y, glide=0.2)
    time.sleep(wait)
    after = session.grab()
    box = find_box(before, after, (x, y))
    tries = 1
    while box is None and tries < 4:          # tooltips appear after a variable delay (and fade in)
        time.sleep(0.45)
        after = session.grab()
        box = find_box(before, after, (x, y))
        tries += 1
    out = {"x": x, "y": y, "box": box, "found": box is not None, "tries": tries}
    if box:
        bx, by, bw, bh = box
        region = [max(0, bx - 4), max(0, by - 4), bw + 8, bh + 8]
    else:
        w, h = after.shape[1], after.shape[0]
        x0, y0 = max(0, x - radius), max(0, y - radius)
        region = [x0, y0, min(w, x + radius) - x0, min(h, y + radius) - y0]
        out["note"] = "no tooltip box found; read the area around the cursor"
    res = session.ocr.recognize(after, region=region, scale=2.0 if box and box[3] < 200 else None)
    out.update(parse_lines([l.text for l in res.lines]))
    out["region"] = region
    if park:
        session.park_cursor()
    return out, images.crop(after, region), after
