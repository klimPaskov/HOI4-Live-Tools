"""Low-level tools: hoi4_screenshot, hoi4_ocr, hoi4_click, hoi4_key, hoi4_type, hoi4_scroll, hoi4_drag, hoi4_hover.

Coordinates are game client pixels (0,0 = top-left of the game picture, see hoi4_status window.client for the
size). Every acting tool re-reads the screen afterwards and reports how much of it changed, with a screenshot.
"""

import time

from ..desktop import clipboard, images
from ..errors import ToolError
from ..game import ui
from .common import REGION, attach, change_ratio, result
from .registry import ToolResult, tool

POINT = {"x": {"type": "integer", "minimum": 0}, "y": {"type": "integer", "minimum": 0}}
VERIFY = {"type": "boolean", "default": True, "description": "re-read the screen and attach a screenshot"}


def _after(session, res, before, name, region=None, verify=True, wait=0.5):
    if not verify:
        return res
    time.sleep(wait)
    after = session.grab()
    res.data["changed_ratio"] = round(change_ratio(before, after), 4)
    if region:
        res.data["changed_ratio_near"] = round(change_ratio(before, after, region), 4)
    # map clouds and animated widgets move a few thousand pixels on their own
    res.data["screen_changed"] = (res.data["changed_ratio"] > 0.01 or
                                  res.data.get("changed_ratio_near", 0) > 0.05)
    res.after_image = after
    return attach(res, session, after, name)


def _near(session, x, y, r=120):
    w, h = session.window.size()
    x0, y0 = max(0, x - r), max(0, y - r)
    return [x0, y0, min(w, x + r) - x0, min(h, y + r) - y0]


@tool("hoi4_screenshot",
      "Screenshot of the game client (optionally a region). Returns a downscaled PNG (max_width, 0 = full size) and "
      "saves the full image under artifacts/. focus=true brings the game to the front first (takes the lock).",
      {"properties": {
          "region": REGION,
          "max_width": {"type": "integer", "minimum": 0, "maximum": 3840, "default": 960},
          "format": {"type": "string", "enum": ["png", "jpeg"], "default": "png"},
          "name": {"type": "string", "default": "screenshot"},
          "focus": {"type": "boolean", "default": False}}},
      family="lowlevel", lock="self",
      examples=[{"args": {}}, {"args": {"region": [1500, 0, 420, 60], "max_width": 0}, "note": "top-right, full size"}])
def hoi4_screenshot(session, args):
    if args["focus"]:
        with session.lock.hold("screenshot"):
            img = session.grab(focus=True)
    else:
        img = session.grab(focus=False)
    full = img
    if args.get("region"):
        img = images.crop(img, args["region"])
    h, w = full.shape[:2]
    res = ToolResult(f"screenshot {img.shape[1]}x{img.shape[0]} of client {w}x{h} via {session.capturer.last_method}",
                     {"client_size": [w, h], "region": args.get("region"), "method": session.capturer.last_method,
                      "foreground": session.window.is_foreground()})
    return attach(res, session, img, args["name"], args["max_width"], args["format"], force=True)


@tool("hoi4_ocr",
      "Read text on the game screen with Windows OCR. Returns lines with boxes in client pixels; pattern keeps only "
      "matching lines; words=true adds word boxes. Small regions are upscaled automatically for small game fonts. "
      "preprocess=white_text keeps only white glyphs (top bar, console, tooltips over busy backgrounds).",
      {"properties": {
          "region": REGION,
          "scale": {"type": "number", "minimum": 0.5, "maximum": 4},
          "pattern": {"type": "string", "description": "regex filter (case-insensitive)"},
          "words": {"type": "boolean", "default": False},
          "preprocess": {"type": "string", "enum": ["none", "white_text", "invert"], "default": "none"}}},
      family="lowlevel", lock="none",
      examples=[{"args": {"pattern": "Load Game"}}, {"args": {"region": [600, 250, 700, 450]}},
                {"args": {"region": [1620, 6, 170, 28], "preprocess": "white_text"}, "note": "the date at 1080p"}])
def hoi4_ocr(session, args):
    img = session.grab(focus=False)
    if args["preprocess"] == "white_text":
        from ..perception.topbar import _whiteness
        region = args.get("region") or [0, 0, img.shape[1], img.shape[0]]
        sub = _whiteness(images.crop(img, region))
        res = session.ocr.recognize(sub, scale=args.get("scale") or 3, pad=12)
        for l in res.lines:                      # back to client coordinates
            l.box = [l.box[0] + region[0], l.box[1] + region[1], l.box[2], l.box[3]]
            for wd in l.words:
                wd["box"] = [wd["box"][0] + region[0], wd["box"][1] + region[1], wd["box"][2], wd["box"][3]]
    else:
        res = session.ocr.recognize(img, region=args.get("region"), scale=args.get("scale"),
                                    invert=args["preprocess"] == "invert")
    lines = res.lines
    if args.get("pattern"):
        lines = ui.screen_mod.find_text(res, args["pattern"])
    out = [l.to_dict(args["words"]) for l in lines]
    return result(f"{len(out)} line(s)", lines=out, text="\n".join(l["text"] for l in out), scale=res.scale)


@tool("hoi4_click",
      "Click inside the game window at client (x, y), or on the text matching `text` (OCR). Verifies by re-reading "
      "the screen: reports how much changed overall and near the click, with a screenshot.",
      {"properties": dict(POINT, **{
          "text": {"type": "string", "description": "regex of a visible label to click instead of x/y"},
          "button": {"type": "string", "enum": ["left", "right", "middle"], "default": "left"},
          "clicks": {"type": "integer", "minimum": 1, "maximum": 3, "default": 1},
          "modifiers": {"type": "array", "items": {"type": "string", "enum": ["shift", "ctrl", "alt"]},
                        "description": "keys held during the click (shift or ctrl to add to a selection)"},
          "verify": VERIFY})},
      family="lowlevel", lock="acting",
      examples=[{"args": {"x": 960, "y": 540}}, {"args": {"text": "Load Game"}}, {"args": {"x": 100, "y": 200, "button": "right"}}])
def hoi4_click(session, args):
    before = session.grab()
    if args.get("text"):
        hit = ui.click_text(session, args["text"])
        x, y = hit["box"][0] + hit["box"][2] // 2, hit["box"][1] + hit["box"][3] // 2
        res = ToolResult(f"clicked '{hit['text']}' at ({x}, {y})", {"clicked": hit})
        if args["verify"]:
            # a busy game (just started or loading) can drop a click: retry a label click once if nothing moved
            time.sleep(0.6)
            mid = session.grab()
            if change_ratio(before, mid) <= 0.01 and change_ratio(before, mid, _near(session, x, y)) <= 0.05:
                session.input.click(x, y)
                session.park_cursor()
                res.data["retried"] = True
    else:
        if "x" not in args or "y" not in args:
            raise ToolError("give x and y, or text", {"field": "x"})
        x, y = args["x"], args["y"]
        mods = args.get("modifiers") or []
        if mods:
            session.input.move(x, y)
        for m in mods:
            session.input.key_down(m)
        if mods:
            time.sleep(0.35)                     # the game samples the modifier keys once per frame
        try:
            session.input.click(x, y, args["button"], args["clicks"], hold=0.12 if mods else None)
        finally:
            for m in reversed(mods):
                session.input.key_up(m)
        res = ToolResult(f"{args['button']} click at ({x}, {y})" + (f" with {'+'.join(mods)}" if mods else ""),
                         {"x": x, "y": y})
    session.park_cursor()
    return _after(session, res, before, "click", _near(session, x, y), args["verify"])


@tool("hoi4_key",
      "Press a key or chord in the game: 'esc', 'space', 'enter', 'f1', 'q', 'ctrl+s', 'shift+f5', 'add' (numpad +), "
      "'grave' (console key). times repeats it.",
      {"properties": {
          "keys": {"type": "string", "minLength": 1},
          "times": {"type": "integer", "minimum": 1, "maximum": 50, "default": 1},
          "verify": VERIFY},
       "required": ["keys"]},
      family="lowlevel", lock="acting",
      examples=[{"args": {"keys": "esc"}}, {"args": {"keys": "space"}, "note": "pause/unpause"}])
def hoi4_key(session, args):
    before = session.grab() if args["verify"] else None
    session.input.key(args["keys"], args["times"])
    return _after(session, ToolResult(f"pressed {args['keys']} x{args['times']}", {"keys": args["keys"]}),
                  before, "key", verify=args["verify"])


@tool("hoi4_type",
      "Type text into the focused game field. paste=true uses the clipboard (restored afterwards), which is reliable "
      "while the game is busy and supports any character; enter=true presses Enter after.",
      {"properties": {
          "text": {"type": "string"},
          "paste": {"type": "boolean", "default": False},
          "enter": {"type": "boolean", "default": False},
          "verify": VERIFY},
       "required": ["text"]},
      family="lowlevel", lock="acting",
      examples=[{"args": {"text": "hoi4live_campaign_1", "enter": True}}])
def hoi4_type(session, args):
    before = session.grab() if args["verify"] else None
    if args["paste"]:
        with clipboard.Preserved():
            clipboard.set_text(args["text"])
            session.input.key("ctrl+v")
            time.sleep(0.2)
    else:
        session.input.type_text(args["text"])
    if args["enter"]:
        session.input.key("enter")
    return _after(session, ToolResult(f"typed {len(args['text'])} characters", {"length": len(args["text"])}),
                  before, "type", verify=args["verify"])


@tool("hoi4_scroll",
      "Mouse wheel at client (x, y): positive notches scroll up / zoom the map in, negative down / out.",
      {"properties": dict(POINT, **{"notches": {"type": "integer", "minimum": -30, "maximum": 30}, "verify": VERIFY}),
       "required": ["x", "y", "notches"]},
      family="lowlevel", lock="acting",
      examples=[{"args": {"x": 960, "y": 540, "notches": -5}, "note": "zoom the map out"}])
def hoi4_scroll(session, args):
    before = session.grab() if args["verify"] else None
    session.input.scroll(args["x"], args["y"], args["notches"])
    session.park_cursor()
    return _after(session, ToolResult(f"scrolled {args['notches']} at ({args['x']}, {args['y']})", {}),
                  before, "scroll", verify=args["verify"])


@tool("hoi4_drag",
      "Drag with a mouse button from (x0, y0) to (x1, y1) over `seconds` (map panning with right/middle, box "
      "selection with left, drawing front lines later).",
      {"properties": {
          "x0": {"type": "integer", "minimum": 0}, "y0": {"type": "integer", "minimum": 0},
          "x1": {"type": "integer", "minimum": 0}, "y1": {"type": "integer", "minimum": 0},
          "button": {"type": "string", "enum": ["left", "right", "middle"], "default": "left"},
          "seconds": {"type": "number", "minimum": 0.05, "maximum": 10, "default": 0.6},
          "verify": VERIFY},
       "required": ["x0", "y0", "x1", "y1"]},
      family="lowlevel", lock="acting",
      examples=[{"args": {"x0": 900, "y0": 500, "x1": 700, "y1": 500, "button": "middle"}, "note": "pan the map"}])
def hoi4_drag(session, args):
    before = session.grab() if args["verify"] else None
    session.input.drag(args["x0"], args["y0"], args["x1"], args["y1"], args["button"], args["seconds"])
    session.park_cursor()
    return _after(session, ToolResult("dragged", {}), before, "drag", verify=args["verify"])


@tool("hoi4_hover",
      "Read a tooltip: move the cursor to (x, y) or onto a named top-bar item (target), wait, find the tooltip box "
      "that appeared (screen difference) and OCR it. Returns the title, the lines and 'label: value' pairs, with a "
      "crop of the tooltip. The cursor is parked afterwards (stay=true leaves it, so the tooltip stays open). "
      "Targets: political_power, stability, war_support, manpower, factories, fuel, supply, convoys, command_power, "
      "army_experience, navy_experience, air_experience, world_tension, date.",
      {"properties": dict(POINT, **{
          "target": {"type": "string", "enum": sorted(list(__import__("hoi4_live.perception.topbar",
                                                                       fromlist=["HOVER"]).HOVER) +
                                                       ["world_tension", "date"])},
          "wait_s": {"type": "number", "minimum": 0, "maximum": 5, "default": 0.9},
          "ocr": {"type": "boolean", "default": True},
          "stay": {"type": "boolean", "default": False},
          "radius": {"type": "integer", "minimum": 100, "maximum": 1200, "default": 420}})},
      family="lowlevel", lock="acting",
      examples=[{"args": {"target": "factories"}, "note": "factories in use by type, energy"},
                {"args": {"target": "political_power"}}, {"args": {"x": 960, "y": 540}, "note": "a map province"}])
def hoi4_hover(session, args):
    from ..perception import tooltip, topbar
    t = args.get("target")
    if t:
        w, h = session.window.size()
        if t == "world_tension":
            x, y = w - 70, 82
        elif t == "date":
            x, y = w - 215, 20
        else:
            x, y = topbar.HOVER[t]
    elif "x" in args and "y" in args:
        x, y = args["x"], args["y"]
    else:
        raise ToolError("give x and y, or target", {"field": "target"})
    if not args["ocr"]:
        session.input.move(x, y, glide=0.25)
        time.sleep(args["wait_s"])
        img = session.grab()
        res = ToolResult(f"hovering at ({x}, {y})", {"x": x, "y": y})
        return attach(res, session, images.crop(img, _near(session, x, y, args["radius"])), "hover", max_width=0)
    data, crop, full = tooltip.read(session, x, y, args["wait_s"], args["radius"], park=not args["stay"])
    if t:
        data["target"] = t
    summ = (f"tooltip at ({x}, {y}): {data.get('title')}" if data["found"]
            else f"no tooltip box at ({x}, {y}); read {len(data['lines'])} line(s) around the cursor")
    res = ToolResult(summ, data)
    return attach(res, session, crop, "hover", max_width=0)
