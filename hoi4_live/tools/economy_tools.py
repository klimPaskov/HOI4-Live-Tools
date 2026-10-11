"""Campaign tools for the economy (phase 3): hoi4_research, hoi4_production, hoi4_construction.

They read structure from the game and mod files (gamedata/), act through the game's interface (no console) and
verify every action on screen; a later save confirms them in hoi4_state.
"""

import re
import time

from ..errors import ToolError
from ..gamedata import tech as tech_data
from ..game import panels, researchview
from . import campaign_common as cc
from .registry import ToolResult, tool

TAG = {"type": "string", "pattern": "^[A-Z][A-Z0-9]{2}$", "description": "default: the player"}


# ============================================================================================== hoi4_research
def _research_context(session, tag, save):
    tag = cc.player_tag(session, tag)
    idx = tech_data.for_session(session)
    if save == "telemetry":
        store = session.telemetry
        known = store.known_techs() if store.player() == tag else None
        if known is not None:
            active = store.researching_techs() or []
            return (tag, idx, sorted(known), list(active), {"save_date": (store.day(tag) or {}).get("_date")},
                    {"source": "telemetry", "as_of": (store.day(tag) or {}).get("_date")})
    sv, src = cc.save_model(session, tag, ["research"], "latest" if save == "telemetry" else save)
    researched = []
    try:
        from ..state import assemble, queries
        from ..state.savefile import clean
        path, _ = assemble.find_save(session, tag, save if save not in ("none", "telemetry") else "latest")
        sf = queries.open_save(path)
        try:
            got = sf.get(f"countries/{tag}/technology/technologies")
            block = clean(next(iter(got.values()))) if got else {}
            researched = [k for k, v in (block or {}).items() if isinstance(v, dict) and v.get("level")]
        finally:
            queries.close_all()
    except ToolError:
        pass
    researching = [r["tech"] for r in ((sv.get("research") or {}).get("researching") or [])]
    return tag, idx, researched, researching, sv, src


def _research_folders(idx):
    vis = tech_data.visible_folders(idx)
    return [f for f in vis if "doctrine" not in f and (idx.get("layout") or {}).get(f, {}).get("gridboxes")]


def _tech_names(session, idx, tag, ids):
    keys = []
    for tid in ids:
        t = idx["techs"].get(tid)
        if t:
            keys += tech_data.display_keys(t, tag)
    nm = cc.names(session, keys)
    out = {}
    for tid in ids:
        t = idx["techs"].get(tid)
        cands = [nm[k] for k in tech_data.display_keys(t, tag) if nm.get(k)] if t else []
        out[tid] = cands
    return out


def _match_tech(session, idx, tag, query, rows):
    q = query.strip()
    if q in idx["techs"]:
        return q
    names = _tech_names(session, idx, tag, [r["id"] for r in rows])
    hits = [r["id"] for r in rows if any(panels.similar(n, q) >= 0.95 for n in names.get(r["id"], []))]
    if len(hits) == 1:
        return hits[0]
    allnames = _tech_names(session, idx, tag, list(idx["techs"]))
    hits = [tid for tid, ns in allnames.items() if any(panels.norm(n) == panels.norm(q) for n in ns)]
    if len(hits) >= 1:
        return hits[0]
    raise ToolError(f"no technology named '{query}' (use an id from action=list)", {"field": "tech"})


def _scroll_into_view(session, idx, folder, offset, origin):
    """Drag the folder with the middle button so the tile is on screen; returns the recalibrated origin."""
    w, h = session.window.size()
    x, y = origin[0] + offset[0], origin[1] + offset[1]
    dx = dy = 0
    if y > h - 90:
        dy = -int(y - h * 0.55)
    elif y < 170:
        dy = int(h * 0.45 - y)
    if x > w - 90:
        dx = -int(x - w * 0.55)
    elif x < 40:
        dx = int(w * 0.45 - x)
    if not dx and not dy:
        return origin, False
    cx, cy = w // 2, h // 2
    session.input.drag(cx, cy, cx + max(-cx + 20, min(cx - 20, dx)), cy + max(-cy + 160, min(cy - 40, dy)),
                       button="middle", seconds=0.5)
    time.sleep(0.6)
    img = session.grab()
    cal = researchview.calibrate(img, idx, folder, base=(origin[0] + dx, origin[1] + dy), dx_range=(-60, 60),
                                 dy_range=(-60, 60))
    return ((cal[0], cal[1]) if cal else (origin[0] + dx, origin[1] + dy)), True


@tool("hoi4_research",
      "Research. slots: the research panel's slots (idle, or the technology and its days left). list: "
      "technologies the country can research now (prerequisites from the files, researched ones from the newest "
      "save), by folder with year, cost and an ahead-of-time flag. start: research a technology (id or localised "
      "name) in a slot: the slot opens the tech tree, the folder tab and the tile are found from the interface "
      "files (calibrated on screen), the detail window's title is checked, Research is pressed and the slot is "
      "read back.",
      {"properties": {
          "action": {"type": "string", "enum": ["slots", "list", "start"], "default": "slots"},
          "tech": {"type": "string", "description": "technology id or localised name (start)"},
          "slot": {"type": "integer", "minimum": 1, "maximum": 10,
                   "description": "slot number (default: the first idle slot)"},
          "replace": {"type": "boolean", "default": False, "description": "start: allow a busy slot"},
          "folder": {"type": "string", "description": "list: only this folder (e.g. industry_folder)"},
          "filter": {"type": "string", "description": "list: regex on id, name or categories"},
          "ahead": {"type": "boolean", "default": True,
                    "description": "list: include technologies ahead of time"},
          "limit": {"type": "integer", "minimum": 1, "maximum": 300, "default": 60},
          "tag": TAG, "save": cc.SAVE_ARG}},
      family="campaign", lock="self", phase=3,
      examples=[{"args": {"action": "slots"}},
                {"args": {"action": "list", "ahead": False}},
                {"args": {"action": "start", "tech": "basic_machine_tools"}},
                {"args": {"action": "start", "tech": "Electronic Engineering", "slot": 2}}])
def hoi4_research(session, args):
    a = args["action"]
    if a == "slots":
        with session.lock.hold("hoi4_research slots"):
            was = panels.current(session)
            panels.open_panel(session, "research")
            slots = researchview.read_slots_settled(session)
            img = session.grab()
            if was != "research":
                panels.close(session)
        idle = [s["index"] for s in slots if s["state"] == "idle"]
        summ = f"{len(slots)} slot(s), {len(idle)} idle" + "".join(
            f"; {s['index']}: {s['name']} ({s['days']} d)" for s in slots if s["state"] == "busy")
        return cc.finish(session, summ, {"slots": slots, "idle": idle}, "research_slots", img=img)
    tag, idx, researched, researching, sv, src = _research_context(session, args.get("tag"), args["save"])
    folders = _research_folders(idx)
    year = cc.game_year(session, sv)
    if a == "list":
        rows = tech_data.plan(idx, researched, researching, year, [args["folder"]] if args.get("folder") else folders)
        if not args["ahead"]:
            rows = [r for r in rows if not r.get("ahead_years")]
        names = _tech_names(session, idx, tag, [r["id"] for r in rows])
        win_w, win_h = session.window.size()
        for r in rows:
            r["name"] = (names.get(r["id"]) or [r["id"]])[0]
            off = tech_data.tile_offset(idx, r["id"], r["folder"], win_w)
            # the tile lies beyond the visible tree: starting it needs a scroll the tool does not do reliably
            r["offscreen"] = bool(off and (off[0] + 12 > win_w - 90 or
                                            off[1] + 138 > win_h - 90))
        if args.get("filter"):
            rx = re.compile(args["filter"], re.I)
            rows = [r for r in rows if rx.search(" ".join([r["id"], r["name"], " ".join(r["categories"])]))]
        total = len(rows)
        rows = rows[:args["limit"]]
        data = {"tag": tag, "year": year, "researched": len(researched), "researching": researching,
                "technologies": rows, "shown": len(rows), "matching": total, "folders": folders, "sources": src,
                "note": "prerequisites and exclusive groups come from the files and the researched list of the "
                        "save; 'allow' conditions are checked by the game when you start"}
        summ = f"{tag} {year}: {total} technolog(ies) can be researched. " + "; ".join(
            f"{r['name']} ({r['folder'].replace('_folder', '')}, {r['year']}" +
            (f", +{r['ahead_years']} y ahead" if r.get("ahead_years") else "") + ")" for r in rows[:10])
        return ToolResult(summ, data)
    # start
    if not args.get("tech"):
        raise ToolError("start needs tech (an id or a localised name)", {"field": "tech"})
    rows = tech_data.plan(idx, researched, researching, year, folders)
    tid = _match_tech(session, idx, tag, args["tech"], rows)
    if tid in researched:
        return ToolResult(f"{tid} is already researched", {"tech": tid, "changed": False, "skip": True,
                                                            "sources": src})
    if tid in researching:
        return ToolResult(f"{tid} is already being researched", {"tech": tid, "changed": False, "skip": True,
                                                                  "sources": src})
    t = idx["techs"][tid]
    st = tech_data.status(t, researched, researching)
    names = _tech_names(session, idx, tag, [tid])[tid] or [tid]
    win_w = session.window.size()[0]
    folder = next((f["name"] for f in t["folders"] if f["name"] in folders and
                   tech_data.tile_offset(idx, tid, f["name"], win_w)), None)
    if not folder:
        raise ToolError(f"{tid} has no tile in a visible research folder", {"folders": [f["name"] for f in t["folders"]]})
    offset = tech_data.tile_offset(idx, tid, folder, win_w)
    data = {"tech": tid, "name": names[0], "folder": folder, "status_from_files": st, "sources": src}
    with session.lock.hold("hoi4_research start"):
        panels.open_panel(session, "research")
        before = researchview.read_slots_settled(session)
        data["before"] = before
        busy_same = [s for s in before if s["state"] == "busy" and any(panels.similar(s.get("name"), n) >= 0.85
                                                                         for n in names)]
        if busy_same:
            return cc.finish(session, f"{names[0]} is already being researched in slot {busy_same[0]['index']}",
                             dict(data, changed=False, after=before), "research_already")
        if args.get("slot"):
            slot = next((s for s in before if s["index"] == args["slot"]), None)
            if not slot:
                raise ToolError(f"there is no research slot {args['slot']}", {"slots": before})
            if slot["state"] == "busy" and not args["replace"]:
                raise ToolError(f"slot {slot['index']} researches {slot['name']}; pass replace=true to change it",
                                {"slots": before})
        else:
            slot = next((s for s in before if s["state"] == "idle"), None)
            if not slot:
                raise ToolError("no idle research slot (pass slot and replace=true to change one)", {"slots": before})
        bx, by, bw, bh = slot["box"]
        session.input.click(bx + bw // 2, by + 40)
        time.sleep(1.5)
        session.park_cursor()
        # the tree takes seconds to open while the machine is busy: wait, and click the slot once more if it did not
        opened = False
        for attempt in range(14):
            if researchview.tree_open(session):
                opened = True
                break
            if attempt == 6:
                session.input.click(bx + bw // 2, by + 40)
                session.park_cursor()
            time.sleep(1.0)
        if not opened:
            cc.fail(session, "the technology tree did not open from the research slot", data, "research_tree")
        tab = researchview.tab_point(idx, folder)
        if tab:
            session.input.click(*tab)
            time.sleep(1.2)
            session.park_cursor()
            time.sleep(0.3)
        img = session.grab()
        cal = researchview.calibrate(img, idx, folder)
        origin = (cal[0], cal[1]) if cal else researchview.ORIGIN
        data["calibration"] = {"origin": origin, "score": round(cal[2], 1) if cal else None,
                               "tiles": cal[3] if cal else 0}
        origin, scrolled = _scroll_into_view(session, idx, folder, offset, origin)
        data["scrolled"] = scrolled
        x, y = int(origin[0] + offset[0]), int(origin[1] + offset[1])
        session.input.click(x, y)
        time.sleep(1.2)
        detail = researchview.read_detail(session)
        data["clicked"] = [x, y]
        data["detail"] = detail
        if not detail or max(panels.similar(detail.get("title"), n) for n in names) < 0.75:
            if detail:
                researchview.close_detail(session)
            researchview.close_tree(session)
            cc.fail(session, f"could not find the tile of {names[0]} in {folder} (the detail window shows "
                             f"{(detail or {}).get('title')})", data, "research_tile")
        bx, by, bw, bh = detail["research_button"]
        session.input.click(bx + bw // 2, by + bh // 2)
        time.sleep(1.5)
        session.park_cursor()
        if slot["state"] == "busy":
            data["replace_dialog"] = bool(panels.answer_dialog(session, accept=True, wait=1.0, attempts=3))
        if panels.current(session) != "research" and researchview.find_detail(session):
            # the game drops a click now and then (seen live: the detail window stayed open, nothing started)
            session.input.click(bx + bw // 2, by + bh // 2)
            time.sleep(1.5)
            session.park_cursor()
            data["research_pressed_twice"] = True
        if panels.current(session) != "research":
            if researchview.find_detail(session):
                researchview.close_detail(session)
            if researchview.tree_open(session):
                researchview.close_tree(session)
        after = researchview.read_slots_settled(session)
        data["after"] = after
        got = [s for s in after if s["state"] == "busy" and any(panels.similar(s.get("name"), n) >= 0.8 for n in names)]
        if not got:
            cc.fail(session, f"{names[0]} does not show in a research slot after pressing Research", data,
                    "research_start")
        data.update({"action": "start", "slot": got[0]["index"], "days": got[0]["days"], "changed": True,
                     "verified_by": "research panel slot"})
        return cc.finish(session, f"researching {names[0]} in slot {got[0]['index']} ({got[0]['days']} days)",
                         data, "research_start")


# ============================================================================================== hoi4_production
def _equipment_names(session, query, tag):
    """Names the production list may show for an equipment id or a typed name."""
    from ..gamedata import economy
    idx = economy.for_session(session)
    q = query.strip()
    if q in idx["equipment"]:
        keys = economy.equipment_name_keys(q, tag)
        arch = idx["equipment"][q].get("archetype")
        if arch:
            keys += economy.equipment_name_keys(arch, tag)
        nm = cc.names(session, keys)
        return [nm[k] for k in keys if nm.get(k)] or [q]
    return [q]


def _line_matcher(spec):
    if spec is None:
        return None
    s = str(spec).strip()
    if s.isdigit():
        n = int(s)
        return lambda l: l["number"] == n
    return lambda l: panels.similar(l["name"], s) >= 0.85 or panels.similar(l.get("type"), s) >= 0.92


@tool("hoi4_production",
      "Military production. lines: every production line (number, equipment, type, factories) and the used/total "
      "military factories and dockyards. add: add a line for an equipment (variant name, equipment id or type "
      "name; category land/armor/air/naval or auto), choose 'produce without' or a named military industrial "
      "organisation, then set its factories. assign: set a line's factories (factories = target count, or delta). "
      "change: switch a line to other equipment (its progress is lost). remove: delete a line. Lines are found by "
      "number or name. fill: put the idle military factories on the lines `targets` names, in order, up to each line's "
      "max (one call instead of reading and assigning line by line). Every action re-reads the line and the factory "
      "header.",
      {"properties": {
          "action": {"type": "string", "enum": ["lines", "add", "assign", "change", "remove", "fill"], "default": "lines"},
          "targets": {"type": "array", "items": {"type": "object", "properties": {
              "line": {"type": "string"}, "max": {"type": "integer", "minimum": 1, "maximum": 200}},
              "required": ["line"]},
              "description": "fill: lines in priority order, each with the most factories it should reach; the free "
                             "military factories go to the first line below its max, then the next"},
          "equipment": {"type": "string", "description": "add/change: variant name, equipment id or type name"},
          "category": {"type": "string", "enum": ["auto", "land", "armor", "air", "naval"], "default": "auto"},
          "line": {"type": "string", "description": "assign/change/remove: line number or equipment name"},
          "factories": {"type": "integer", "minimum": 0, "maximum": 200,
                        "description": "add: factories for the new line (default 1); assign: target count"},
          "delta": {"type": "integer", "minimum": -50, "maximum": 50, "description": "assign: change by this many"},
          "mio": {"type": "string", "description": "add/change: military industrial organisation to use"},
          "tag": TAG}},
      family="campaign", lock="acting", phase=3,
      examples=[{"args": {"action": "lines"}},
                {"args": {"action": "add", "equipment": "Support Equipment", "factories": 3}},
                {"args": {"action": "assign", "line": "M1 Garand", "factories": 4}},
                {"args": {"action": "change", "line": "24", "equipment": "infantry_equipment_1"}}])
def hoi4_production(session, args):
    from ..game import productionview as pv
    a = args["action"]
    tag = cc.player_tag(session, args.get("tag"))
    panels.open_panel(session, "production")
    head = pv.read_header(session)
    if a == "lines":
        lines = pv.all_lines(session)
        data = {"header": head, "lines": lines, "count": len(lines)}
        summ = (f"{len(lines)} production line(s); military factories {head.get('military_used')}/"
                f"{head.get('military_total')}, dockyards {head.get('dockyards_used')}/{head.get('dockyards_total')}. "
                + "; ".join(f"{l['number']} {l['name']} x{l['factories_text']}" for l in lines[:12]))
        return cc.finish(session, summ, data, "production_lines")
    data = {"action": a, "header_before": head}
    if a == "fill":
        if not args.get("targets"):
            raise ToolError("fill needs targets: [{line, max}]", {"field": "targets"})
        free = (head.get("military_total") or 0) - (head.get("military_used") or 0)
        lines = pv.all_lines(session)
        steps = []
        for tg in args["targets"]:
            if free <= 0:
                break
            m = _line_matcher(tg["line"])
            line = next((l for l in lines if m(l)), None)
            if not line:
                steps.append({"line": tg["line"], "error": "no such line"})
                continue
            cur = pv.requested(line) or 0
            give = min(free, (tg["max"] - cur) if tg.get("max") else free)
            if give <= 0:
                continue
            res = hoi4_production(session, {"action": "assign", "line": str(line["number"]), "delta": give,
                                            "category": "auto", "tag": args.get("tag")})
            steps.append({"line": line["number"], "name": line["name"], "from": cur, "to": cur + give})
            free -= give
        data.update({"steps": steps, "free_left": free, "changed": any("to" in s for s in steps)})
        return cc.finish(session, "filled: " + ("; ".join(f"line {s['line']} {s.get('from')}->{s.get('to')}"
                                                           for s in steps if "to" in s) or "nothing to fill")
                         + f" ({free} military factories still free)", data, "production_fill")
    if a == "add":
        if not args.get("equipment"):
            raise ToolError("add needs equipment", {"field": "equipment"})
        names = _equipment_names(session, args["equipment"], tag)
        cats = [args["category"]] if args["category"] != "auto" else ["land", "armor", "air", "naval"]
        before_lines = pv.bottom_lines(session)
        last_number = max([b["number"] for b in before_lines] or [0])
        picked = None
        tried = {}
        for cat in cats:
            session.input.click(*pv.ADD_BUTTONS[cat])
            time.sleep(1.2)
            session.park_cursor()
            if not pv.choices_open(session):
                tried[cat] = "no list opened"
                continue
            picked, entries = pv.pick_choice(session, names)
            tried[cat] = [e["name"] for e in entries][:20]
            if picked:
                data["category"] = cat
                break
            session.input.key("esc")             # close the list (the panel stays)
            time.sleep(0.6)
        if not picked:
            raise ToolError(f"no production choice named {names}", {"lists": tried})
        data["picked"] = {k: picked[k] for k in ("name", "type", "cost")}
        data["mio"] = pv.answer_mio(session, args.get("mio"))
        time.sleep(0.5)
        line, after_lines = None, []
        for _ in range(3):                       # the new line's row may be read while it is still drawn in
            after_lines = pv.bottom_lines(session)
            new = [l for l in after_lines if l["number"] > last_number]
            line = new[-1] if new else None
            if line:
                break
            time.sleep(1.0)
        if not line:
            cc.fail(session, "no new production line appeared", dict(data, lines_after=after_lines), "production_add")
        want = args.get("factories")
        if want is not None and pv.requested(line) is not None and want != pv.requested(line):
            line = pv.set_factories(session, line, want)
        data.update({"line": line, "header_after": pv.read_header(session), "changed": True,
                     "verified_by": "new line in the production list and the factory header"})
        if want is not None and line and pv.requested(line) != want:
            data["warning"] = f"the line requests {pv.requested(line)} factories (asked {want}): the clicks missed?"
        elif line and line.get("factories") is not None and line["factories"] < (pv.requested(line) or 0):
            data["warning"] = (f"only {line['factories']} of the {pv.requested(line)} requested factories are "
                               f"assigned: no more military factories are free")
        return cc.finish(session, f"added line {line['number']} {line['name']} with {line.get('factories')} "
                         f"factor(ies); military factories {data['header_after'].get('military_used')}/"
                         f"{data['header_after'].get('military_total')}", data, "production_add")
    m = _line_matcher(args.get("line"))
    if not m:
        raise ToolError(f"{a} needs line (a number or an equipment name)", {"field": "line"})
    line = pv.find_line(session, m)
    if not line:
        raise ToolError(f"no production line matches '{args['line']}'", {"lines": pv.all_lines(session)})
    data["line_before"] = line
    if a == "assign":
        if args.get("factories") is None and args.get("delta") is None:
            raise ToolError("assign needs factories (target) or delta", {"field": "factories"})
        cur = pv.requested(line) or 0
        diff = (args["factories"] - cur) if args.get("factories") is not None else args["delta"]
        after = pv.set_factories(session, line, cur + diff) if diff else line
        got = pv.requested(after) if after else None
        data.update({"line_after": after, "header_after": pv.read_header(session),
                     "changed": bool(after and got != cur)})
        if diff and not data["changed"]:
            cc.fail(session, f"line {line['number']} still shows {after.get('factories_text')} factories (the "
                             f"clicks missed, or the line cannot go there)", data, "production_assign")
        short = after.get("factories") is not None and got is not None and after["factories"] < got
        return cc.finish(session, f"line {line['number']} {line['name']}: {cur} -> {got} factories requested"
                         + (f" ({after['factories']} assigned: no more military factories are free)" if short else ""),
                         data, "production_assign")
    if a == "remove":
        pv.click_row(session, line, "delete")
        panels.answer_dialog(session, True, attempts=1)
        gone = pv.find_line(session, lambda l, n=line["name"], k=line["number"]:
                            panels.similar(l["name"], n) >= 0.95 and l["number"] == k)
        data.update({"removed": gone is None, "header_after": pv.read_header(session), "changed": gone is None})
        if gone:
            cc.fail(session, "the line is still listed after delete", data, "production_remove")
        return cc.finish(session, f"removed line {line['number']} {line['name']}", data, "production_remove")
    # change
    if not args.get("equipment"):
        raise ToolError("change needs equipment", {"field": "equipment"})
    names = _equipment_names(session, args["equipment"], tag)
    h = session.window.size()[1]
    if line["row_top"] + 60 > h - 25:                      # the picture is cut by the bottom of the list
        pv.scroll_list(session, -3)
        line = pv.find_line(session, lambda l, n=line["number"]: l["number"] == n, from_bottom=True) or line
    session.input.click(150, line["row_top"] + 55)
    time.sleep(1.2)
    session.park_cursor()
    if not pv.choices_open(session):
        cc.fail(session, "the equipment list did not open from the line's picture", data, "production_change")
    picked, entries = pv.pick_choice(session, names)
    if not picked:
        session.input.key("esc")
        raise ToolError(f"no production choice named {names}", {"list": [e["name"] for e in entries]})
    data["picked"] = {k: picked[k] for k in ("name", "type", "cost")}
    data["mio"] = pv.answer_mio(session, args.get("mio"))
    after = pv.find_line(session, lambda l, n=line["number"]: l["number"] == n)
    data.update({"line_after": after,
                 "changed": bool(after and panels.similar(after["name"], line["name"]) < 0.95)})
    if not data["changed"]:
        cc.fail(session, "the line still shows its old equipment", data, "production_change")
    return cc.finish(session, f"line {line['number']}: {line['name']} -> {after['name']}", data, "production_change")


# ============================================================================================== hoi4_construction
def _building_names(session, query):
    from ..gamedata import economy
    idx = economy.for_session(session)
    q = query.strip()
    if q in idx["buildings"]:
        return [cc.name_of(session, q, q), q], q
    nm = cc.names(session, list(idx["buildings"]))
    for bid, name in nm.items():
        if panels.norm(name) == panels.norm(q):
            return [name], bid
    return [q], None


def _state_name(session, state):
    s = str(state).strip()
    if s.isdigit():
        n = cc.name_of(session, f"STATE_{s}", None)
        if not n:
            raise ToolError(f"no localised name for state {s}", {"field": "state"})
        return n, int(s)
    return s, None


def plan_states(rows, levels, per_state, states=None, choose="free_slots", tag=None):
    """[(state row, count)] for `levels` levels over the player's states with free slots (pure: unit tested)."""
    want = None
    if states:
        want = {str(s).strip().lower() for s in states}
    cands = []
    for r in rows:
        if not isinstance(r.get("free"), (int, float)) or r["free"] <= 0:
            continue
        if tag and r.get("ctrl") not in (None, tag):
            continue                                     # occupied by someone else: cannot build
        if want and str(r.get("s")) not in want and str(r.get("name", "")).lower() not in want:
            continue
        cands.append(r)
    key = {"free_slots": lambda r: (-r["free"], -(r.get("civ") or 0) - (r.get("mil") or 0)),
           "industry": lambda r: (-((r.get("civ") or 0) + (r.get("mil") or 0)), -r["free"]),
           "infrastructure": lambda r: (-(r.get("infra") or 0), -r["free"])}[choose]
    cands.sort(key=key)
    out, left = [], levels
    for r in cands:
        n = min(per_state, int(r["free"]), left)
        if n > 0:
            out.append((r, n))
            left -= n
        if left <= 0:
            break
    return out


def _construction_plan(session, args):
    store = session.telemetry
    rows = store.state_rows()
    if not rows:
        raise ToolError("no state scan in the telemetry yet (it is logged weekly: let a week of game time pass, "
                        "or name the states with action=build)", {"field": "states"})
    tag = store.player()
    building = args.get("building") or "industrial_complex"
    skip = getattr(session, "unfindable_states", set())          # names the Find view could not locate before
    plan = plan_states([r for r in rows if r.get("name") not in skip], args["levels"], args["per_state"],
                       args.get("states"), args["choose"], tag)
    summary = [{"state": r["name"], "id": r["s"], "levels": n, "free_slots": r["free"]} for r, n in plan]
    if args["dry_run"] or not plan:
        return ToolResult(f"plan: {sum(x['levels'] for x in summary)} level(s) of {building} in {len(summary)} state(s)"
                          + "".join(f"; {x['state']} x{x['levels']}" for x in summary),
                          {"plan": summary, "building": building, "scan_date": (rows[0] or {}).get("_date")})
    done, failed = [], []
    for r, n in plan:
        session.check_cancel()
        try:
            res = hoi4_construction(session, {"action": "build", "building": building,
                                              "state": str(r["s"]) if str(r.get("s", "")).isdigit() else str(r["name"]),
                                              "count": n, "refresh": False})
            done.append({"state": r["name"], "levels": n, "summary": res.summary})
        except ToolError as e:
            failed.append({"state": r["name"], "levels": n, "error": str(e)[:200]})
            if "Find view has no" in str(e):
                if not hasattr(session, "unfindable_states"):
                    session.unfindable_states = set()
                session.unfindable_states.add(r["name"])
    total = sum(x["levels"] for x in done)
    q = None
    try:
        from ..game import constructionview as cv
        q = cv.read_queue(session)
    except ToolError:
        pass
    text = (f"construction plan: {total} level(s) of {building} queued in {len(done)} state(s)"
            + (f"; {len(failed)} failed ({failed[0]['error'][:80]})" if failed else "")
            + (f"; queue {len(q['rows'])} item(s), civilian factories {q.get('used')}/{q.get('total')}" if q else ""))
    return ToolResult(text, {"queued": done, "failed": failed, "building": building, "changed": bool(done),
                             "queue_after": q, "verified_by": "construction queue per state"},
                      is_error=not done)


@tool("hoi4_construction",
      "Construction. queue: the construction queue (state, factories assigned) and civilian factories used/total. "
      "buttons: the building buttons of the panel, named from their tooltips (cached). build: queue a building "
      "(id such as industrial_complex, arms_factory, infrastructure, or its localised name) in a state (id or "
      "name): the building's button is pressed, the map camera moves to the state through the Find view, the "
      "state is clicked `count` times and the queue is read back. plan: queue `levels` of a building over the states "
      "with the most free building slots (from the telemetry's weekly state scan: no map reading), at most "
      "`per_state` in each; `states` limits the choice, `choose` orders it (free_slots, industry, infrastructure), "
      "dry_run only lists the plan. Several builds in one call.",
      {"properties": {
          "action": {"type": "string", "enum": ["queue", "buttons", "build", "plan"], "default": "queue"},
          "levels": {"type": "integer", "minimum": 1, "maximum": 40, "default": 6, "description": "plan: total levels"},
          "per_state": {"type": "integer", "minimum": 1, "maximum": 8, "default": 3, "description": "plan: most per state"},
          "states": {"type": "array", "items": {"type": "string"}, "description": "plan: only these states (ids or names)"},
          "choose": {"type": "string", "enum": ["free_slots", "industry", "infrastructure"], "default": "free_slots",
                     "description": "plan: free_slots (most free slots first), industry (most factories first), "
                                    "infrastructure (best infrastructure first)"},
          "dry_run": {"type": "boolean", "default": False, "description": "plan: list the plan, build nothing"},
          "building": {"type": "string", "description": "build: building id or localised name"},
          "state": {"type": "string", "description": "build: state id or localised name"},
          "count": {"type": "integer", "minimum": 1, "maximum": 10, "default": 1,
                    "description": "build: how many levels (one click each)"},
          "refresh": {"type": "boolean", "default": False, "description": "buttons/build: re-read the tooltips"}}},
      family="campaign", lock="acting", phase=3,
      examples=[{"args": {"action": "queue"}},
                {"args": {"action": "build", "building": "industrial_complex", "state": "Michigan"}},
                {"args": {"action": "build", "building": "Infrastructure", "state": "376", "count": 2}}])
def hoi4_construction(session, args):
    from ..game import constructionview as cv
    from ..game import mapview
    a = args["action"]
    if a == "plan":
        return _construction_plan(session, args)
    panels.open_panel(session, "construction")
    if a == "queue":
        q = cv.read_queue(session)
        summ = (f"construction: {len(q['rows'])} item(s) queued; civilian factories {q.get('used')}/{q.get('total')}"
                + "".join(f"; {r['state']} ({r['factories']})" for r in q["rows"][:12]))
        return cc.finish(session, summ, q, "construction_queue")
    if a == "buttons":
        btns = cv.buttons(session, refresh=args["refresh"])
        return cc.finish(session, f"{len(btns)} building button(s): " + ", ".join(btns), {"buttons": btns},
                         "construction_buttons")
    if not args.get("building") or not args.get("state"):
        raise ToolError("build needs building and state", {"field": "building" if not args.get("building") else "state"})
    names, bid = _building_names(session, args["building"])
    state, sid = _state_name(session, args["state"])
    data = {"action": "build", "building": bid, "building_name": names[0], "state": state, "state_id": sid}
    before = cv.read_queue(session)
    data["before"] = before
    title, xy = cv.find_button(session, names, refresh=args["refresh"])
    if not xy:
        title, xy = cv.find_button(session, names, refresh=True)
    if not xy:
        raise ToolError(f"no building button named {names}", {"buttons": list(cv.buttons(session))})
    data["button"] = {"title": title, "at": xy}
    session.input.click(*xy)
    time.sleep(0.8)
    session.park_cursor()
    data["camera"] = mapview.go_to_id(session, sid, expect=state) if sid else mapview.go_to(session, state)
    time.sleep(0.6)
    w, h = session.window.size()
    session.park_cursor()
    time.sleep(0.4)
    data["slots_before"] = cv.slots_near_centre(session)     # the state's "used/total" box in construction mode
    for _ in range(args["count"]):
        session.input.click(w // 2, h // 2, settle=0.2)
        time.sleep(0.35)
    session.park_cursor()
    time.sleep(0.6)
    img = session.grab()
    after = cv.read_queue(session, img)
    data["after"] = after
    data["slots"] = cv.slots_near_centre(session, img)
    session.input.click(*xy)                     # leave the building mode
    time.sleep(0.4)
    session.park_cursor()
    hit = [r for r in after["rows"] if panels.similar(r["state"], state) >= 0.85]
    hit_before = [r for r in before["rows"] if panels.similar(r["state"], state) >= 0.85]
    changed = len(after["rows"]) > len(before["rows"]) or (hit and (not hit_before or hit[0] != hit_before[0])) or \
        (after.get("used") != before.get("used"))
    if not hit or not changed:
        s = data.get("slots_before") or data.get("slots")
        for _ in range(3):                       # the box is read again while the camera settles
            if s:
                break
            time.sleep(0.6)
            s = cv.slots_near_centre(session)
        data["slots"] = s
        if s and s["used"] >= s["total"] and bid not in {"infrastructure", "air_base", "anti_air_building",
                                                            "radar_station"}:
            why = f"all {s['total']} building slots of {state} are used (the map shows {s['used']}/{s['total']})"
        else:
            why = "the state may not be ours, have no free slot, or the click missed it"
        cc.fail(session, f"{names[0]} in {state} does not show in the construction queue ({why})",
                dict(data, error="no_slot" if s and s["used"] >= s["total"] else "unchanged"), "construction_build")
    data.update({"changed": True, "verified_by": "construction queue row for the state"})
    return cc.finish(session, f"queued {names[0]} x{args['count']} in {state}; queue {len(after['rows'])} item(s), "
                     f"civilian factories {after.get('used')}/{after.get('total')}", data, "construction_build")
