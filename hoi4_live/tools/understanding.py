"""Understanding tools: hoi4_state (merged state model), hoi4_briefing (situation report), hoi4_screen (what is on
screen: screen class, open panel, top bar) and hoi4_map (map modes, camera, region screenshots, front overview)."""

import json
import re
import os
import time

from ..errors import ToolError
from ..perception import popups as popups_mod
from ..state import assemble, briefing as briefing_mod
from .common import attach, result
from .registry import ToolResult, tool

SAVE_ARG = {"type": "string", "default": "latest",
            "description": "deep data source: 'latest' (newest save of this country), 'none', 'fresh' (test mode: "
                           "write hoi4live_state now, ~20 s), or a save name"}


def _state_dir(session):
    d = os.path.join(session.cfg.artifacts_dir, "state")
    os.makedirs(d, exist_ok=True)
    return d


@tool("hoi4_state",
      "One structured model of a country, merged from the telemetry mod (current to the last in-game day), the "
      "newest plain-text save (deep: focus, research, production, construction, laws, wars, states) and optionally "
      "the top bar on screen. Every section lists its sources with their in-game date (freshness per field). "
      "sections: overview, politics, economy, military, diplomacy, focus, research, production, construction, "
      "decisions, states, world, script. Read-only.",
      {"properties": {
          "tag": {"type": "string", "pattern": "^[A-Z][A-Z0-9]{2}$", "description": "default: the player"},
          "sections": {"type": "array", "items": {"type": "string", "enum": list(assemble.SECTIONS)},
                       "default": list(assemble.DEFAULT_SECTIONS)},
          "save": SAVE_ARG,
          "screen": {"type": "boolean", "default": False, "description": "also read the top bar on screen"},
          "limit": {"type": "integer", "minimum": 5, "maximum": 400, "default": 40},
          "raw": {"type": "boolean", "default": False, "description": "include the raw source records"}}},
      family="understanding", lock="self", phase=2,
      examples=[{"args": {}, "note": "the player: overview, politics, economy, military, diplomacy, focus, research"},
                {"args": {"sections": ["economy", "construction", "production"], "screen": True}},
                {"args": {"tag": "GER", "sections": ["overview", "focus", "research"], "save": "autosave"}},
                {"args": {"sections": ["world", "script"], "save": "none"}, "note": "telemetry only, instant"}])
def hoi4_state(session, args):
    m = assemble.collect(session, args.get("tag"), args["sections"], args["save"], args["screen"], args["limit"])
    raw = m.pop("_raw", None)
    if args["raw"]:
        m["raw"] = raw
    ov = m.get("overview") or {}
    src = "; ".join(f"{k} {v.get('as_of')}" + (f" ({v['age_days']:g} d old)" if v.get("age_days") else "")
                    for k, v in m["sources"].items() if v.get("as_of"))
    summ = (f"{m.get('name')} ({m.get('tag')}) on {m.get('date')}: "
            + (f"PP {briefing_mod.fmt_num(ov.get('political_power'))}, stability {briefing_mod.pct(ov.get('stability'))}, "
               f"war support {briefing_mod.pct(ov.get('war_support'))}; " if ov else "")
            + f"sources: {src or 'none'}")
    return ToolResult(summ, m)


def _log_notes(session, since_mark=None, limit=8):
    """Recent mod messages from game.log (script `log` output other than telemetry), deduplicated."""
    try:
        lines = session.logs.tail("game", 400)
    except OSError:
        return []
    out = []
    for l in lines:
        # script `log` output (mods' messages) comes from effectbase.cpp; engine lines are noise here
        if "HOI4LIVE" in l or "effectbase.cpp" not in l:
            continue
        body = l.split("]: ", 1)[-1].strip()
        if body and body not in out:
            out.append(body)
    return out[-limit:]


@tool("hoi4_briefing",
      "One-page situation report for the player: date and game speed, what needs a decision now (popups, idle "
      "focus or research, free factories, unspent political power, equipment and resource shortfalls, losing "
      "wars), government, wars and fronts, economy, military, focus and research, world, recent events (telemetry "
      "events, answered popups, optional game.log notes) and what changed since the previous briefing. Built "
      "from hoi4_state with the screen read; each source is dated. Markdown in `markdown`, facts as data.",
      {"properties": {
          "tag": {"type": "string", "pattern": "^[A-Z][A-Z0-9]{2}$"},
          "save": SAVE_ARG,
          "screen": {"type": "boolean", "default": True, "description": "read the top bar, pause state, popups"},
          "world": {"type": "boolean", "default": True, "description": "include the weekly world summary"},
          "log_notes": {"type": "integer", "minimum": 0, "maximum": 30, "default": 0,
                        "description": "add the last N non-telemetry game.log lines (mod messages)"},
          "remember": {"type": "boolean", "default": True,
                       "description": "store this briefing's facts for the next one's deltas"}}},
      family="understanding", lock="self", phase=2,
      examples=[{"args": {}, "note": "full briefing for the player"},
                {"args": {"save": "fresh"}, "note": "test mode: write a save first so focus and research are current"},
                {"args": {"screen": False, "save": "none"}, "note": "telemetry only (fast, no screen read)"}])
def hoi4_briefing(session, args):
    sections = ["overview", "politics", "economy", "military", "diplomacy", "focus", "research", "decisions"]
    if args["world"]:
        sections.append("world")
    m = assemble.collect(session, args.get("tag"), sections, args["save"], args["screen"])
    m.pop("_raw", None)
    tag = m.get("tag") or "unknown"
    path = os.path.join(_state_dir(session), f"briefing_{tag}.json")
    prev = None
    try:
        with open(path, encoding="utf-8") as f:
            prev = json.load(f).get("facts")
    except (OSError, ValueError):
        pass
    # popups answered since the previous briefing (this server and earlier ones)
    pops = []
    since = (prev or {}).get("_wall", 0)
    try:
        with open(os.path.join(_state_dir(session), "popups.jsonl"), encoding="utf-8") as f:
            for line in f:
                try:
                    p = json.loads(line)
                except ValueError:
                    continue
                if p.get("time", 0) > since:
                    pops.append(p)
    except OSError:
        pass
    notes = _log_notes(session, limit=args["log_notes"]) if args["log_notes"] else None
    b = briefing_mod.build(m, prev, pops[-10:], notes)
    b["facts"]["_wall"] = time.time()
    if args["remember"]:
        with open(path, "w", encoding="utf-8") as f:
            json.dump({"facts": b["facts"], "markdown": b["markdown"]}, f, indent=1)
    stamp = time.strftime("%Y%m%d_%H%M%S")
    md_path = session.artifact_path(f"briefing_{tag}_{stamp}.md")
    with open(md_path, "w", encoding="utf-8") as f:
        f.write(b["markdown"] + "\n")
    data = {"markdown": b["markdown"], "attention": b["attention"], "deltas": b["deltas"], "facts": b["facts"],
            "sources": m["sources"], "file": md_path}
    return ToolResult(b["markdown"], data)


@tool("hoi4_screen",
      "What is on screen, without acting: the screen class (map, esc_menu, main_menu, load_screen, loading, ...), "
      "the open panel or full-screen view on the map (politics, decisions, research, trade, construction, "
      "production, deployment, logistics, officer_corps, faction, army/navy/air overview, focus_tree, diplomacy) "
      "with the key that toggles it, any other window in the centre with its title, the console, popups, the date, "
      "pause and speed, and the top-bar values (political power, stability, war support, manpower, factories, "
      "fuel, supply, convoys, command power, experience, world tension).",
      {"properties": {
          "topbar": {"type": "boolean", "default": True},
          "popups": {"type": "boolean", "default": True, "description": "read popup titles and options (OCR)"},
          "screenshot": {"type": "boolean", "default": False}}},
      family="understanding", lock="none", phase=2,
      examples=[{"args": {}}, {"args": {"topbar": False, "screenshot": True}}])
def hoi4_screen(session, args):
    from ..game import popups as game_popups
    from ..perception import screen as screen_mod
    from ..perception import topbar
    img = session.grab(focus=False)
    st = screen_mod.classify(img, session.ocr, session.logs.idle_seconds("game"))
    data = {"screen": st["screen"], "console_open": st["console_open"]}
    if "date" in st:
        data["date"] = st["date"]["hoi4"]
        data["date_text"] = st["date"]["text"]
        ps, _ = topbar.pause_state(img)
        data["paused"] = {"paused": True, "running": False}.get(ps)
        data["speed"] = topbar.speed_level(img)
    if st.get("panel"):
        data["panel"] = {"name": st["panel"], "title": st.get("panel_title"),
                         "toggle_key": screen_mod.PANEL_KEYS.get(st["panel"])}
    if st.get("window"):
        data["window"] = st["window"]
    found = popups_mod.detect(img)
    if found:
        data["popups"] = (game_popups.describe(session, img, found) if args["popups"]
                          else [p.to_dict(i) for i, p in enumerate(found)])
    if args["topbar"] and st["screen"] == "map":
        bar = topbar.read_bar(img, session.ocr, game_dir=session.cfg.game_dir)
        data["topbar"] = {k: {"value": v["value"], "text": v["text"]} for k, v in bar.items() if k != "_missing"}
        if bar["_missing"]:
            data["topbar_unread"] = bar["_missing"]
    summ = f"screen {data['screen']}"
    if data.get("panel"):
        summ += f", {data['panel']['name']} open"
    if data.get("window"):
        summ += f", window '{data['window'].get('title')}'"
    if data.get("date"):
        summ += f", {data.get('date_text')}" + (" (paused)" if data.get("paused") else "")
    if found:
        summ += f", {len(found)} popup(s)"
    if data.get("topbar"):
        tb = data["topbar"]
        summ += (f"; PP {tb['political_power']['text']}, stability {tb['stability']['text']}, "
                 f"war support {tb['war_support']['text']}, manpower {tb['manpower']['text']}, "
                 f"factories {tb['factories']['text']}, convoys {tb['convoys']['text']}")
    res = ToolResult(summ, data)
    if args["screenshot"]:
        attach(res, session, img, "screen")
    return res


def _state_name(session, sid):
    from ..gamedata import loc as loc_mod
    return loc_mod.for_session(session).get(f"STATE_{sid}")


def _front_overview(session, tag=None):
    """Front summary from the telemetry (weekly front lines, daily wars) and the save's wars."""
    store = session.telemetry
    snap = store.snapshot(tag) if (tag or store.player()) else None
    if not snap:
        return None
    wk = snap.get("week") or {}
    fr = wk.get("front") or {"enemy": [], "own": []}
    by_enemy = {}
    for s in fr.get("enemy", []):
        by_enemy.setdefault(s.get("ctrl"), []).append(s)
    return {"date": snap["date"], "week": wk.get("date"), "at_war": bool((snap.get("day") or {}).get("war")),
            "wars": snap.get("wars"), "front_states_today": (snap.get("day") or {}).get("front"),
            "enemy_front_states": {k: [f"{x['name']} ({x['s']})" for x in v] for k, v in by_enemy.items()},
            "own_front_states": [f"{x['name']} ({x['s']})" for x in fr.get("own", [])],
            "lost_states": [f"{x['name']} ({x['s']}, held by {x['ctrl']})" for x in wk.get("lost", [])],
            "_focus": (fr.get("own") or fr.get("enemy") or [None])[0]}


@tool("hoi4_map",
      "The map. modes: the map modes on the F-key buttons (read from their tooltips once, cached). view: switch "
      "the map mode (name or F-key), move the camera to a state (id or name, 'capital', or 'front'), zoom "
      "(close, province, region, country, continent, world, or notches) and take a screenshot (optionally a "
      "region). front: the front overview from telemetry (enemy-held states touching ours by enemy, our front "
      "states, lost states, wars) plus a view centred on the front. at: the tooltip of the map at (x, y) "
      "(province, state, owner, what the map mode shows).",
      {"properties": {
          "action": {"type": "string", "enum": ["modes", "view", "front", "at", "minimap"], "default": "view"},
          "mode": {"type": "string", "description": "map mode name (default, supply, terrain, resistance, ...) or F1-F10"},
          "target": {"type": "string", "description": "state id, state or province name, 'capital' or 'front'; "
                                                      "'sea:<strategic region id>' or 'point:<x>,<y>' (map pixels) "
                                                      "go through the minimap"},
          "zoom": {"type": "string", "default": "region",
                   "description": "close | province | region | country | continent | world | notches like -5"},
          "x": {"type": "integer", "minimum": 0}, "y": {"type": "integer", "minimum": 0},
          "region": {"type": "array", "items": {"type": "integer", "minimum": 0}, "minItems": 4, "maxItems": 4},
          "max_width": {"type": "integer", "minimum": 0, "maximum": 3840, "default": 1280},
          "recalibrate": {"type": "boolean", "default": False}}},
      family="understanding", lock="acting", phase=2,
      examples=[{"args": {"action": "modes"}},
                {"args": {"action": "view", "target": "capital", "zoom": "country"}},
                {"args": {"action": "view", "mode": "supply", "target": "Texas", "zoom": "region"}},
                {"args": {"action": "front"}}, {"args": {"action": "at", "x": 960, "y": 540}}])
def hoi4_map(session, args):
    from ..game import mapview
    a = args["action"]
    if a == "modes":
        modes = mapview.calibrate_modes(session, force=args["recalibrate"])
        return result(f"{len(modes)} map mode button(s): " + ", ".join(f"{k} {v}" for k, v in modes.items()),
                      modes=modes)
    if a == "minimap":
        from ..gamedata import mapdata
        cal = mapview.calibrate_minimap(session, mapdata.for_session(session), force=args["recalibrate"])
        return result(f"minimap calibrated (residual {cal['residual_px']} px)", **{k: v for k, v in cal.items()
                                                                                    if k != "points"})
    if a == "at":
        if "x" not in args or "y" not in args:
            raise ToolError("at needs x and y", {"field": "x"})
        d, crop = mapview.at_point(session, args["x"], args["y"])
        res = ToolResult(f"map at ({args['x']}, {args['y']}): {d.get('title')}", d)
        return attach(res, session, crop, "map_at", max_width=0)
    data = {}
    target = args.get("target")
    if a == "front":
        fo = _front_overview(session)
        if not fo:
            raise ToolError("no telemetry: the front overview needs the telemetry mod (hoi4_telemetry)")
        focus = fo.pop("_focus")
        data["front"] = fo
        if not fo["at_war"]:
            data["note"] = "not at war: no front"
        if focus and not target:
            target = str(focus["s"])
    if args.get("mode"):
        data["mode_key"] = mapview.set_mode(session, args["mode"])
    if target:
        name = target
        if target == "capital":
            st = assemble.collect(session, None, ["overview"], "latest", False)
            cap = ((st.get("_raw") or {}).get("save") or {}).get("overview", {}).get("capital")
            if not cap:
                raise ToolError("capital unknown (no save of the player)")
            name = _state_name(session, cap) or str(cap)
        if re.match(r"^(sea|region|point):", str(target)):
            from ..gamedata import mapdata
            idx = mapdata.for_session(session)
            kind, _, val = str(target).partition(":")
            if kind == "point":
                px, py = [float(v) for v in val.split(",")]
            else:
                reg = idx["regions"].get(val)
                if not reg:
                    raise ToolError(f"no strategic region {val}")
                px, py = reg["aim"]
            data["camera"] = mapview.go_to_point(session, idx, px, py)
            data["camera"]["name"] = str(target)
        elif str(target).isdigit():
            data["camera"] = mapview.go_to_id(session, int(target), expect=_state_name(session, target))
        else:
            data["camera"] = mapview.go_to(session, name)
        data["zoom_notches"] = mapview.zoom(session, args["zoom"] if not re.match(r"^-?\d+$", args["zoom"])
                                            else int(args["zoom"]))
    elif args["zoom"] and re.match(r"^-?\d+$", args["zoom"]):
        data["zoom_notches"] = mapview.zoom(session, int(args["zoom"]))
    time.sleep(0.6)
    img = session.grab()
    shot = img
    if args.get("region"):
        from ..desktop import images
        shot = images.crop(img, args["region"])
    summ = "map" + (f" ({data['mode_key']})" if data.get("mode_key") else "") + \
           (f" centred on {data['camera']['name']}" if data.get("camera") else "")
    if data.get("front"):
        fo = data["front"]
        summ = (f"front on {fo['date']}: " + ("; ".join(f"{k}: {len(v)} state(s)" for k, v in fo["enemy_front_states"].items())
                                             or "no enemy-held state touches ours") + ". " + summ)
    res = ToolResult(summ, data)
    return attach(res, session, shot, "map", max_width=args["max_width"])
