"""Top bar reading: the in-game date (OCR) and whether time is running.

The date widget is anchored to the top-right corner: "12:00, 1 Jan, 1936" in white over a dark box. While the game
is paused the box shows yellow/black hazard stripes and the speed icon is a pause sign; while it runs the stripes
disappear. Positions are offsets from the right edge in UI pixels (gui_scale 1); they do not depend on the
resolution because HOI4 anchors the top bar to the corner without scaling it.
"""

import datetime
import re

import numpy as np

MONTHS = {m: i + 1 for i, m in enumerate(
    ["jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec"])}
DATE_RX = re.compile(r"(\d{1,2})\s*[:.;]\s*(\d{2})\s*[,.]?\s*(\d{1,2})\s+([A-Za-z]{3})[A-Za-z]*\s*[,.]?\s*(\d{4})")
# OCR confusions inside digit fields
_DIGIT_FIX = str.maketrans({"O": "0", "o": "0", "D": "0", "l": "1", "I": "1", "|": "1", "i": "1", "S": "5",
                            "s": "5", "B": "8", "a": "4", "Z": "2", "z": "2", "g": "9", "q": "9"})


def date_region(w, h, scale=1.0):
    """[x, y, w, h] of the date text inside the top-right widget."""
    return [int(w - 300 * scale), int(6 * scale), int(170 * scale), int(28 * scale)]


def wide_region(w, h, scale=1.0):
    return [int(w - 460 * scale), 0, int(400 * scale), int(48 * scale)]


def _whiteness(crop, low=110, gain=2.2):
    """Black text on white from white text on a striped background (keeps anti-aliasing soft)."""
    mn = crop.astype(np.int16).min(2)
    g = np.clip(255 - (mn - low) * gain, 0, 255).astype(np.uint8)
    return np.stack([g, g, g], 2)


def parse_date(text, lenient=False):
    """'12:00, 1 Jan, 1936' -> dict, tolerant to common OCR confusions. None when it does not parse.

    HOI4 shows midnight as 24:00 of the day that ends, so hours run 0-24. lenient=True also accepts an
    impossible hour (an OCR misread): the date is kept and the hour is marked uncertain (hour 0)."""
    if not text:
        return None
    t = text.replace("\n", " ")
    m = DATE_RX.search(t)
    if not m:
        fixed = re.sub(r"(?<=\s)[a-zA-Z](?=\s+[A-Za-z]{3})", lambda x: x.group(0).translate(_DIGIT_FIX), t)
        fixed = re.sub(r"(?<![A-Za-z])([0-9OoIlZzaA]{1,2})\s*[:.;]\s*([0-9Oo]{2})",
                       lambda x: x.group(1).translate(_DIGIT_FIX) + ":" + x.group(2).translate(_DIGIT_FIX), fixed)
        m = DATE_RX.search(fixed)
        if not m:
            return None
    hour, minute, day, mon, year = m.groups()
    mon = MONTHS.get(mon[:3].lower())
    try:
        hour, day, year = int(hour), int(day), int(year)
    except ValueError:
        return None
    uncertain = False
    if not 0 <= hour <= 24:
        if not lenient:
            return None
        hour, uncertain = 0, True
    if mon is None or not (1 <= day <= 31 and 1800 <= year <= 2200):
        return None
    try:
        datetime.date(year, mon, day)
    except ValueError:
        return None
    out = {"year": year, "month": mon, "day": day, "hour": hour, "text": m.group(0),
           "iso": f"{year:04d}-{mon:02d}-{day:02d}T{hour:02d}:00", "hoi4": f"{year}.{mon}.{day}.{hour}"}
    if uncertain:
        out["hour_uncertain"] = True
    return out


def _preps(crop):
    """Preprocessing variants, best first: soft whiteness, a harder threshold, a stronger soft curve."""
    yield _whiteness(crop), 3
    g = np.where(crop.astype(np.int16).min(2) > 150, 0, 255).astype(np.uint8)
    yield np.stack([g, g, g], 2), 3
    yield _whiteness(crop, low=140, gain=3.0), 4


def read_date(img, ocr, scale=1.0):
    """OCR the date widget: several preprocessing variants on the exact date box, then a wider box. A reading
    with a valid hour wins; otherwise the date with hour_uncertain is returned; otherwise an error dict."""
    h, w = img.shape[:2]
    attempts = []
    fallback = None
    for region in (date_region(w, h, scale), wide_region(w, h, scale)):
        x, y, rw, rh = region
        crop = img[y:y + rh, x:x + rw]
        for im, sc in _preps(crop):
            res = ocr.recognize(im, scale=sc, pad=8)
            text = " ".join(l.text for l in res.lines)
            attempts.append(text)
            d = parse_date(text)
            if d:
                d["region"] = region
                return d
            if fallback is None:
                fb = parse_date(text, lenient=True)
                if fb:
                    fb["region"] = region
                    fallback = fb
    if fallback is not None:
        return fallback
    return {"error": "date not readable", "ocr": attempts}


def date_key(d):
    """Comparable tuple (year, month, day, hour), with 24:00 normalised to the next day's 00:00."""
    t = to_datetime(d)
    return (t.year, t.month, t.day, t.hour)


def to_datetime(d):
    """24:00 becomes 00:00 of the next day."""
    return datetime.datetime(d["year"], d["month"], d["day"]) + datetime.timedelta(hours=d["hour"])


_EPOCH = datetime.datetime(1, 1, 1)


def hours(d):
    """In-game hours since year 1 (naive arithmetic: datetime.timestamp() fails before 1970 on Windows)."""
    return int((to_datetime(d) - _EPOCH).total_seconds() // 3600)


def stripe_score(img, scale=1.0):
    """Share of hazard-stripe pixels (olive/amber, neither black nor white) in the date box background. The
    stripes are drawn only while paused; running time shows a plain black box."""
    h, w = img.shape[:2]
    x0, x1 = int(w - 270 * scale), int(w - 140 * scale)
    y0, y1 = int(9 * scale), int(31 * scale)
    a = img[y0:y1, x0:x1].astype(np.int16)
    r, g, b = a[..., 0], a[..., 1], a[..., 2]
    mx = a.max(2)
    stripe = (r - b > 14) & (g - b > 8) & (mx > 35) & (mx < 200)
    return float(stripe.mean())


def speed_icon(img, scale=1.0):
    """The icon left of the date: a green play arrow while running, a pause sign (pulsing between amber and
    dull tan) while paused. Returns ('play' | 'pause' | 'unknown', counts)."""
    h, w = img.shape[:2]
    x0, x1 = int(w - 295 * scale), int(w - 277 * scale)
    y0, y1 = int(11 * scale), int(30 * scale)
    a = img[y0:y1, x0:x1].astype(np.int16)
    r, g, b = a[..., 0], a[..., 1], a[..., 2]
    green = int(((g > 120) & (g > r + 30) & (g > b + 30)).sum())
    warm = int(((r > 70) & (r - b > 18) & (r >= g) & (g - b > 5)).sum())
    if green > 12 and green > warm:
        return "play", {"green": green, "warm": warm}
    if warm > 10:
        return "pause", {"green": green, "warm": warm}
    return "unknown", {"green": green, "warm": warm}


def pause_state(img, scale=1.0):
    """'paused' | 'running' | 'unknown' from the speed icon (orange pause sign vs green play arrow), with the
    hazard stripes of the paused date box as a tie-breaker. No OCR needed."""
    icon, stats = speed_icon(img, scale)
    stripes = stripe_score(img, scale)
    if icon == "pause":
        state = "paused"
    elif icon == "play":
        state = "running"
    elif stripes > 0.12:
        state = "paused"
    else:
        state = "unknown"
    return state, {"icon": icon, "stripes": round(stripes, 3), **stats}


def speed_level(img, scale=1.0):
    """Lit segments of the five-step speed bar under the date (1-5), or None. Segments light green; the fifth
    lights red at maximum speed; unlit segments are grey."""
    h, w = img.shape[:2]
    y0, y1 = int(35 * scale), int(40 * scale)
    x0, x1 = int(w - 282 * scale), int(w - 157 * scale)
    a = img[y0:y1, x0:x1].astype(np.int16)
    if a.size == 0:
        return None
    seg = (x1 - x0) / 5.0
    lit = 0
    for i in range(5):
        s = a[:, int(i * seg) + 3:int((i + 1) * seg) - 3].reshape(-1, 3).mean(0)
        if s.max() > 90 and s.max() - s.min() > 25:
            lit += 1
    return lit or None


# ---------------------------------------------------------------------------------------------- resource bar
# Text boxes of the top bar at gui_scale 1, from the vanilla interface/topbar.gui text widgets (pol_power,
# stability, war_support, manpower, industrial_capacity, fuel_value, supply_value, convoys_count,
# command_power_value, experience_value ...), widened a little for the glyphs. The bar is anchored to the top-left
# corner; world tension is anchored to the top-right corner (threat_value at x=-98, y=74).
# name: (x, y, w, h, kind)
LEFT_CELLS = {
    "political_power": (114, 2, 62, 26, "number"),
    "stability": (196, 2, 48, 26, "percent"),
    "war_support": (268, 2, 48, 26, "percent"),
    "manpower": (343, 2, 76, 26, "amount"),
    "factories": (432, 0, 60, 22, "number"),
    "fuel": (506, 0, 62, 22, "fuel"),
    "supply": (580, 0, 58, 22, "percent"),
    "convoys": (656, 0, 56, 22, "number"),
    "command_power": (757, 2, 40, 26, "number"),
    "army_experience": (814, 2, 40, 26, "number"),
    "navy_experience": (872, 2, 40, 26, "number"),
    "air_experience": (930, 2, 40, 26, "number"),
}
RIGHT_CELLS = {"world_tension": (100, 72, 60, 20, "percent")}       # x measured from the right edge
# where to hover for each item's tooltip (centre of its icon + value)
HOVER = {"political_power": (140, 16), "stability": (205, 16), "war_support": (280, 16), "manpower": (375, 16),
         "factories": (460, 14), "fuel": (535, 14), "supply": (610, 14), "convoys": (684, 14),
         "command_power": (765, 16), "army_experience": (822, 16), "navy_experience": (880, 16),
         "air_experience": (938, 16)}
NUM = re.compile(r"[-+]?\d+(?:[.,]\d+)?")
_FIX = str.maketrans({"O": "0", "o": "0", "D": "0", "l": "1", "I": "1", "|": "1", "i": "1", "S": "5", "s": "5",
                      "B": "8", "Z": "2", "z": "2", "g": "9", "q": "9", "—": "-", "–": "-"})


def cell_boxes(w, h, scale=1.0):
    out = {k: [int(x * scale), int(y * scale), int(cw * scale), int(ch * scale)]
           for k, (x, y, cw, ch, _) in LEFT_CELLS.items()}
    for k, (dx, y, cw, ch, _) in RIGHT_CELLS.items():
        out[k] = [int(w - dx * scale), int(y * scale), int(cw * scale), int(ch * scale)]
    return out


def _bright(crop, low=95, gain=2.4):
    """Dark text on white from bright text of any colour (white, green, yellow, red) on the dark bar."""
    mx = crop.astype(np.int16).max(2)
    g = np.clip(255 - (mx - low) * gain, 0, 255).astype(np.uint8)
    return np.stack([g, g, g], 2)


def parse_value(text, kind):
    """Top-bar text -> number. 'percent' -> fraction, 'amount' handles K/M suffixes, 'fuel' keeps the unit."""
    if text is None:
        return None
    t = text.strip().replace(" ", "")
    m = re.search(r"([-+]?[0-9OoIlSB.,]+)\s*([KkMm%d]?)", t)
    if not m:
        return None
    digits = m.group(1).translate(_FIX).replace(",", ".")
    if digits.count(".") > 1:
        digits = digits.replace(".", "", digits.count(".") - 1)
    try:
        v = float(digits)
    except ValueError:
        return None
    suffix = m.group(2).lower()
    if suffix == "k":
        v *= 1000
    elif suffix == "m":
        v *= 1000000
    if kind == "percent":
        return round(v / 100.0, 4)
    if kind in ("number", "amount"):
        return int(round(v)) if abs(v - round(v)) < 1e-6 else v
    return v


PATTERNS = {"percent": re.compile(r"^-?\d{1,3}(?:[.,]\d+)?%$"),
            "amount": re.compile(r"^-?\d+(?:[.,]\d+)?[KMB]?$"),
            "number": re.compile(r"^[-+]?\d+(?:[.,]\d+)?[KM]?$"),
            "fuel": re.compile(r"^\d+(?:[.,]\d+)?\s?[dhKM]?$")}
GLYPH_SETS = {"percent": "0123456789%.-", "amount": "0123456789.KMB-", "number": "0123456789.-+KM",
              "fuel": "0123456789.dhKM"}


def normalise(text, kind):
    """Undo the usual OCR confusions in a top-bar value: '0/0' or 'o/o' for %, letters for digits."""
    if not text:
        return None
    t = text.strip().replace(" ", "") if kind != "fuel" else re.sub(r"\s+", " ", text.strip())
    t = re.sub(r"[0oO]/[0oO]$", "%", t)
    t = re.sub(r"(?<=\d)[oO°]$", "%", t) if kind == "percent" and not t.endswith("%") else t
    if kind == "fuel" and re.match(r"^\d{2,}8$", t):
        t = t[:-1] + " d"                                   # 'd' read as '8' when the space is lost
    head = re.match(r"^([^0-9]*)(.*)$", t)
    body = head.group(2).translate(_FIX) if head else t
    return body if body else None


def _valid(text, kind):
    return bool(text) and bool(PATTERNS[kind].match(text.replace(" ", "") if kind != "fuel" else text))


def _ocr_cell(crop, ocr, mode):
    """OCR one cell: 'prefix' writes a word in front (lone digits read better on a line), 'repeat' lays the
    cell three times side by side and keeps a value read at least twice."""
    from PIL import Image, ImageDraw, ImageFont
    g = Image.fromarray(_bright(crop)).convert("L")
    g = g.resize((g.width * 3, g.height * 3), Image.LANCZOS)
    if mode == "prefix":
        try:
            fnt = ImageFont.truetype("arial.ttf", 44)
        except OSError:
            fnt = ImageFont.load_default()
        can = Image.new("L", (g.width + 260, g.height + 40), 255)
        ImageDraw.Draw(can).text((10, 20 + g.height // 2 - 26), "Value", fill=0, font=fnt)
        can.paste(g, (200, 20))
        res = ocr.recognize(np.asarray(can.convert("RGB")), scale=1.0, pad=10)
        texts = [re.sub(r"^\W*v\W*a\W*l\W*u\W*e\W*", "", l.text, flags=re.I) for l in res.lines]
        texts = [t for t in texts if t.strip() and not re.match(r"^\W*v\s*a", t, re.I)]
        return " ".join(texts).strip() or None
    can = Image.new("L", (3 * (g.width + 60) + 80, g.height + 60), 255)
    for i in range(3):
        can.paste(g, (40 + i * (g.width + 60), 30))
    res = ocr.recognize(np.asarray(can.convert("RGB")), scale=1.0, pad=0)
    words = [w["text"] for l in res.lines for w in (l.words or [{"text": l.text}])]
    best = max(set(words), key=words.count) if words else None
    return best if best and words.count(best) >= 2 else None


def read_bar(img, ocr, scale=1.0, game_dir=None, cells=None):
    """Read the top-bar values. Each cell is read with OCR (a word in front of the value; then the value
    repeated three times on a line), and with the game's own font glyphs when OCR gives nothing valid.
    Returns {name: {"text", "value", "box", "via"}} plus "_missing" for cells nothing valid was read from."""
    from . import glyphs
    h, w = img.shape[:2]
    boxes = cell_boxes(w, h, scale)
    kinds = {k: v[4] for k, v in list(LEFT_CELLS.items()) + list(RIGHT_CELLS.items())}
    fnt = glyphs.font(game_dir) if game_dir else None
    out = {}
    for name, (x, y, cw, ch) in boxes.items():
        if cells and name not in cells:
            continue
        crop = img[max(0, y):y + ch, max(0, x):x + cw]
        kind = kinds[name]
        text, via = None, None
        for mode in ("prefix", "repeat", "glyphs"):
            if mode == "glyphs":
                raw = glyphs.read(crop, fnt, charset=GLYPH_SETS[kind]) if fnt else None
            else:
                raw = _ocr_cell(crop, ocr, mode)
            t = normalise(raw, kind)
            if _valid(t, kind):
                text, via = t, mode
                break
        out[name] = {"text": text, "value": parse_value(text, kind), "box": boxes[name], "via": via}
    out["_missing"] = [n for n in out if n != "_missing" and out[n]["text"] is None]
    return out
