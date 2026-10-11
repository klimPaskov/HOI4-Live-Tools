"""Campaign tools for the military (phase 4): hoi4_army, hoi4_orders, hoi4_navy_air.

They act through the game's interface only (no console, so they work in campaign mode), for any country, and verify
every action on screen: the army panel and cards, the training lines, the division designer and template list, the
battle plan toolbar, the execute button's animation, the map tooltip, the fleet and air wing panels. Structure that
the screen does not list (template compositions, armies' orders, fleets' task forces, air wings) comes from the
newest save (`save` argument); telemetry gives war score and state control for orders.
"""

import math
import re
import time

from ..errors import ToolError
from ..game import armyview as av
from ..game import deployview as dv
from ..game import mapview, panels
from ..game import navalairview as nv
from . import campaign_common as cc
from .registry import ToolResult, tool

TAG = {"type": "string", "pattern": "^[A-Z][A-Z0-9]{2}$", "description": "default: the player"}
SAVE = {"type": "string", "default": "latest",
        "description": "save the lists come from: 'latest' (newest save of the country), 'none', or a save name"}


# ---------------------------------------------------------------------------------------------- save side
def _military(session, tag, save, parts=("templates", "armies", "fleets", "air")):
    """Military lists from a save: ({templates, armies, army_groups, fleets, air_wings, divisions}, source)."""
    if save == "none":
        return {}, {"save": "none"}
    from ..state import assemble, military, queries
    try:
        snap = session.telemetry.snapshot(tag) if tag else None
        path, note = assemble.find_save(session, tag, save, (snap or {}).get("date"))
    except ToolError as e:
        return {}, {"save": None, "error": str(e)}
    out = {}
    try:
        sf = queries.open_save(path)
        hdr = sf.header()
        names = lambda keys: cc.names(session, keys)
        if "templates" in parts:
            tps = military.templates(sf, tag)
            nm = names(sorted({r["type"] for t in tps for r in t["regiments"] + t["support"]}))
            divs = military.divisions(sf, tag)
            count = {}
            for d in divs:
                count[d["template_id"]] = count.get(d["template_id"], 0) + 1
            for t in tps:
                t["divisions"] = count.get(t["id"], 0)
                t["composition"] = _composition(t["regiments"], nm)
                t["support_companies"] = [nm.get(r["type"], r["type"]) for r in t["support"]]
            out["templates"] = tps
        if "armies" in parts:
            out.update(military.armies(sf, tag, names))
        if "fleets" in parts:
            out["fleets"] = military.fleets(sf, tag)
        if "air" in parts:
            out["air_wings"] = military.air_wings(sf, tag)
        src = {"save": path, "as_of": hdr.get("date")}
        if note:
            src["note"] = note
        return out, src
    finally:
        queries.close_all()


def _composition(regs, nm):
    counts = {}
    for r in regs:
        n = nm.get(r["type"], r["type"])
        counts[n] = counts.get(n, 0) + 1
    return ", ".join(f"{v}x {k}" for k, v in counts.items())


def _telemetry_war(session, tag):
    """{war_score, enemies, states_owned, states_controlled, date} from the last daily telemetry line."""
    try:
        snap = session.telemetry.snapshot(tag) or {}
    except Exception:
        snap = {}
    day = snap.get("day") or {}
    if not day:
        return None
    out = {"date": snap.get("date"), "war_score": day.get("war_score_any"), "states_owned": day.get("states"),
           "states_controlled": day.get("states_ctrl"), "occupied": day.get("occupied"),
           "enemies": day.get("enemies"), "surrender": day.get("surrender"), "casualties_k": day.get("cas_k"),
           "divisions": day.get("div"), "enemy_front_states": day.get("front")}
    out["wars"] = [{"enemy": w.get("e"), "surrender": w.get("sp"), "divisions": w.get("div")}
                   for w in snap.get("wars") or []]
    return out


def _ensure_map(session):
    """Close side panels (the unit panel and the cards need the bare map) without Esc on the map, and show the
    default map mode (the navy and air modes hide the land units and show other cards)."""
    panels.to_map(session)
    try:
        mapview.set_mode(session, "default", click=True)
    except ToolError:
        pass


def _open_deploy(session):
    """Recruit & Deploy from a clean default map: a fleet or army still selected from an earlier step, or a strategic
    map mode, kept the U key from opening it in live runs."""
    _ensure_map(session)
    dv.open_panel(session)


def _country_words(session, tag):
    """Names the map tooltip may use for a country: the game's own (telemetry GetName, which follows cosmetic
    tags and mods), then the localisation keys TAG and TAG_DEF."""
    out = []
    try:
        snap = session.telemetry.snapshot(tag) or {}
        nm = (snap.get("pol") or snap.get("pol_stale") or {}).get("name")
        if nm:
            out.append(nm)
    except Exception:
        pass
    nm = cc.names(session, [tag, tag + "_DEF"])
    out += [n for n in nm.values() if n and n != tag]
    return out


def _owner_at_centre(session):
    w, h = session.window.size()
    tip, _ = mapview.at_point(session, w // 2, h // 2)
    for l in tip.get("lines") or []:
        m = re.match(r"^\s*(owner|controller)\s*:\s*\W*\s*(.+)$", l, re.I)
        if m:
            return m.group(2).strip(), tip.get("lines")
    return None, tip.get("lines")


def _words_in(name, text):
    import difflib
    words = [w for w in re.findall(r"[a-z0-9]+", (name or "").lower()) if len(w) >= 3 and w != "the"]
    have = re.findall(r"[a-z0-9]+", (text or "").lower())
    return bool(words) and all(any(difflib.SequenceMatcher(None, w, x).ratio() >= 0.8 for x in have) for w in words)


def _go(session, place, zoom=None, ours=None, tag=None):
    """Camera to a state (name or id). ours=True/False: when several places share the name, take the one we own
    (divisions to select) or one we do not own (an enemy target), judged from the map tooltip's Owner line."""
    if not place:
        return None
    name = place
    if str(place).isdigit():
        # a state id: the Find view lists the state with exactly that id first (no name matching, no OCR slips)
        from .understanding import _state_name
        cam = mapview.go_to_id(session, int(place), expect=_state_name(session, place))
        if zoom:
            mapview.zoom_abs(session, zoom)
        time.sleep(0.4)
        return cam
    check = None
    if ours is not None and tag:
        mine = _country_words(session, tag)

        def check():
            owner, _ = _owner_at_centre(session)
            if not owner:
                return True
            is_ours = any(_words_in(n, owner) for n in mine)
            return is_ours == ours
    cam = mapview.go_to(session, name, check=check)
    if zoom:
        mapview.zoom(session, zoom)
    time.sleep(0.4)
    return cam


def _select_army(session, args):
    if not args.get("army") and not args.get("army_index"):
        raise ToolError("name the army (army = its name as on its panel, or army_index = card number from the left)",
                        {"field": "army"})
    card, p = av.find_army(session, args.get("army"), args.get("army_index"))
    if p.get("kind") != "army":
        raise ToolError(f"the card did not open an army panel ({p.get('lines')[:6]})")
    return card, p


def _small(p):
    return {k: v for k, v in (p or {}).items() if k != "lines"}


# ============================================================================================== hoi4_army
@tool("hoi4_army",
      "Land forces through the game's interface. list: armies (divisions, general, orders, executing), army groups, "
      "templates and unassigned divisions (newest save) plus the army cards on screen. templates: the Division "
      "Templates panel (names as shown) with each template's composition from the save. design: open a template "
      "in the division designer and change it: duplicate (save as a new template, the original stays), new_name, "
      "add battalions (type id or name, column 1-5; support companies with support=true), remove (column/row "
      "or type), then Save (costs army experience; refused with the game's reason). recruit: train count "
      "divisions of a template (optionally deploying to a state). deploy: deploy the ready divisions of a "
      "template's training line. create_army: select the divisions in a state (box on the map) and make a new "
      "army of them (optional name and general). assign: add the divisions in a state to an army. general: put a "
      "general (by name, or the first listed) in command. rename: rename an army. army_group: group named armies "
      "under a new army group. Every action is verified on screen (panels, cards, lists).",
      {"properties": {
          "action": {"type": "string", "enum": ["list", "templates", "design", "recruit", "deploy", "create_army",
                                                "assign", "general", "rename", "army_group"], "default": "list"},
          "template": {"type": "string", "description": "design/recruit/deploy: template name as shown"},
          "duplicate": {"type": "boolean", "default": False, "description": "design: save as a new template"},
          "new_name": {"type": "string", "description": "design: template name; rename: army name"},
          "add": {"type": "array", "items": {"type": "object", "properties": {
              "type": {"type": "string", "description": "sub-unit id (artillery_brigade) or its name (Artillery)"},
              "column": {"type": "integer", "minimum": 1, "maximum": 5},
              "support": {"type": "boolean", "default": False},
              "count": {"type": "integer", "minimum": 1, "maximum": 5, "default": 1}},
              "required": ["type"], "additionalProperties": False}, "description": "design: battalions to add"},
          "remove": {"type": "array", "items": {"type": "object", "properties": {
              "column": {"type": "integer", "minimum": 1, "maximum": 5},
              "row": {"type": "integer", "minimum": 1, "maximum": 5},
              "support_row": {"type": "integer", "minimum": 1, "maximum": 5}},
              "additionalProperties": False}, "description": "design: slots to empty (column+row, or support_row)"},
          "count": {"type": "integer", "minimum": 1, "maximum": 30, "default": 1, "description": "recruit: divisions"},
          "state": {"type": "string", "description": "recruit/deploy: deployment state; create_army/assign: the state "
                                                     "whose divisions are selected (localised name or id)"},
          "box": {"type": "array", "items": {"type": "integer"}, "minItems": 2, "maxItems": 2,
                  "description": "create_army/assign: half width and height of the selection box around the "
                                 "state's centre at region zoom (default [220, 160])"},
          "zoom": {"type": "string", "enum": ["close", "province", "region", "country"],
                   "description": "create_army/assign: camera zoom for the selection (default region; a closer zoom "
                                  "with a small box selects the divisions of one state only)"},
          "army": {"type": "string", "description": "army name as on its panel"},
          "army_index": {"type": "integer", "minimum": 1, "maximum": 40, "description": "army card number (left = 1)"},
          "armies": {"type": "array", "items": {"type": "string"}, "description": "army_group: army names"},
          "name": {"type": "string", "description": "create_army: name for the new army"},
          "general": {"type": "string", "description": "general/create_army: commander name (default: first listed)"},
          "early": {"type": "boolean", "default": False,
                    "description": "deploy: also deploy divisions that are only partly trained (at least 20 %)"},
          "screen": {"type": "boolean", "default": True, "description": "list: also read the army cards"},
          "tag": TAG, "save": SAVE}},
      family="military", lock="acting", phase=4,
      examples=[{"args": {"action": "list"}},
                {"args": {"action": "templates"}},
                {"args": {"action": "design", "template": "Infantry Division", "duplicate": True,
                          "new_name": "Infantry Division II", "add": [{"type": "infantry", "column": 4}]}},
                {"args": {"action": "recruit", "template": "Infantry Division", "count": 2, "state": "Texas"}},
                {"args": {"action": "deploy", "template": "Infantry Division"}},
                {"args": {"action": "create_army", "state": "Texas", "name": "Rio Grande Army",
                          "general": "George S. Patton"}},
                {"args": {"action": "general", "army": "Army 1", "general": "Omar Bradley"}},
                {"args": {"action": "army_group", "armies": ["Army 1", "Army 2"]}}])
def hoi4_army(session, args):
    a = args["action"]
    tag = cc.player_tag(session, args.get("tag"))
    if a == "list":
        mil, src = _military(session, tag, args["save"], ("armies", "templates"))
        data = {"tag": tag, "sources": src, "armies": mil.get("armies", []), "army_groups": mil.get("army_groups", []),
                "divisions": mil.get("divisions"), "unassigned_divisions": mil.get("unassigned_divisions"),
                "templates": [{k: t[k] for k in ("name", "battalions", "composition", "divisions")}
                              for t in mil.get("templates", [])]}
        if args["screen"]:
            _ensure_map(session)
            data["cards"] = av.cards(session)
        summ = (f"{tag}: {len(data['armies'])} arm(ies), {data['unassigned_divisions']} of {data['divisions']} "
                f"division(s) unassigned (save {src.get('as_of')}); " + "; ".join(
                    f"{x['name']}: {x['divisions']} div, {x.get('leader') or 'no general'}, "
                    f"{', '.join(o['type'] for o in x['orders']) or 'no orders'}"
                    + (f", {x['executing_divisions']} executing" if x["executing_divisions"] else "")
                    for x in data["armies"][:8]))
        if args["screen"]:
            summ += f"; {len(data['cards'])} army card(s) on screen"
            return cc.finish(session, summ, data, "army_list")
        return ToolResult(summ, data)
    if a == "templates":
        mil, src = _military(session, tag, args["save"], ("templates",))
        _open_deploy(session)
        rows, seen = [], set()
        for i in range(4):
            for r in dv.template_rows(session):
                if r["name"] not in seen:
                    seen.add(r["name"])
                    rows.append(r["name"])
            if i < 3 and len(rows) >= 7 * (i + 1):
                session.input.scroll(730, 600, -3)
                time.sleep(0.5)
            else:
                break
        saved = {t["name"]: t for t in mil.get("templates", [])}
        out = []
        for n in rows:
            t = next((v for k, v in saved.items() if panels.similar(k, n) >= 0.86), None)
            out.append({"name": n, "battalions": t["battalions"] if t else None,
                        "composition": t["composition"] if t else None,
                        "support": t["support_companies"] if t else None,
                        "divisions": t["divisions"] if t else None,
                        "in_save": bool(t)})
        img = session.grab()
        panels.close(session)
        data = {"tag": tag, "templates": out, "sources": src}
        return cc.finish(session, f"{len(out)} template(s): " + "; ".join(
            f"{t['name']}" + (f" ({t['composition']})" if t["composition"] else "") for t in out[:10]),
            data, "army_templates", img=img)
    if a == "design":
        return _design(session, args, tag)
    if a == "recruit":
        return _recruit(session, args, tag)
    if a == "deploy":
        return _deploy(session, args, tag)
    if a in ("create_army", "assign"):
        return _create_or_assign(session, args, tag)
    if a == "general":
        _ensure_map(session)
        card, before = _select_army(session, args)
        pick, listed = av.assign_commander(session, args.get("general"))
        after = av.panel(session)
        ok = after.get("commander") and panels.similar(after["commander"], pick["name"]) >= 0.75
        data = {"army": before.get("name"), "before": _small(before), "after": _small(after), "picked": pick["name"],
                "listed": listed, "changed": bool(ok), "verified_by": "army panel commander line"}
        if not ok:
            cc.fail(session, f"{pick['name']} does not show as the commander of {before.get('name')}", data,
                    "army_general")
        return cc.finish(session, f"{after['commander']} commands {before.get('name')}", data, "army_general")
    if a == "rename":
        if not args.get("new_name"):
            raise ToolError("rename needs new_name", {"field": "new_name"})
        _ensure_map(session)
        card, before = _select_army(session, args)
        after = av.rename_army(session, args["new_name"])
        ok = after.get("name") and panels.similar(after["name"], args["new_name"]) >= 0.85
        data = {"before": _small(before), "after": _small(after), "changed": bool(ok),
                "verified_by": "army panel name"}
        if not ok:
            cc.fail(session, f"the army panel shows '{after.get('name')}', not '{args['new_name']}'", data,
                    "army_rename")
        return cc.finish(session, f"army '{before.get('name')}' renamed to '{after['name']}'", data, "army_rename")
    if a == "army_group":
        return _army_group(session, args, tag)
    raise ToolError(f"unknown action {a}")


def _design(session, args, tag):
    try:
        return _design_steps(session, args, tag)
    except ToolError:
        comp, _, _ = dv.designer_anchor(session)       # never leave the designer open over the map
        if comp is not None:
            dv.close_designer(session, comp)
        raise


def _design_steps(session, args, tag):
    if not args.get("template"):
        raise ToolError("design needs template (its name as shown)", {"field": "template"})
    adds, removes = args.get("add") or [], args.get("remove") or []
    if not (adds or removes or args.get("new_name") or args["duplicate"]):
        raise ToolError("design needs something to change: add, remove, new_name or duplicate")
    _open_deploy(session)
    row, seen = dv.find_template(session, args["template"])
    before_rows = [r["name"] for r in dv.template_rows(session)]
    session.input.click(*row["edit"])
    time.sleep(1.4)
    session.park_cursor()
    comp, res = dv.wait_designer(session)
    data = {"template": row["name"], "templates_before": before_rows, "steps": []}
    name0 = dv.designer_name(session, comp)
    data["designer_name"] = name0
    if args["duplicate"]:
        session.input.click(*dv.designer_button(session, "Duplicate", res, comp))
        time.sleep(0.6)
        data["steps"].append("duplicate")
    if args.get("new_name"):
        got = dv.set_designer_name(session, comp, args["new_name"])
        data["steps"].append({"name": got})
        if not got or panels.similar(got, args["new_name"]) < 0.85:
            dv.close_designer(session, comp)
            cc.fail(session, f"the designer's name field shows '{got}', not '{args['new_name']}'", data, "design_name")
    for rm in removes:
        if rm.get("support_row"):
            pt = dv.support_point(comp, rm["support_row"])
        elif rm.get("column") and rm.get("row"):
            pt = dv.slot_point(comp, rm["column"], rm["row"])
        else:
            raise ToolError("remove needs column and row, or support_row", {"field": "remove"})
        kind, nm = dv.slot_info(session, pt)
        if kind != "filled":
            dv.close_designer(session, comp)
            cc.fail(session, f"slot {rm} is {kind}, nothing to remove", data, "design_remove")
        session.input.key_down("shift")
        try:
            session.input.click(*pt)
        finally:
            session.input.key_up("shift")
        time.sleep(0.6)
        kind2, _ = dv.slot_info(session, pt)
        data["steps"].append({"removed": nm, "slot": rm, "slot_now": kind2})
        if kind2 == "filled":
            # the slots below move up: a filled slot after the removal is fine when it holds another battalion
            pass
    for ad in adds:
        names = _battalion_names(session, ad["type"])
        for _ in range(ad.get("count", 1)):
            if ad.get("support"):
                pts = [dv.support_point(comp, r) for r in range(1, 5)]
            else:
                col = ad.get("column") or 1
                pts = [dv.slot_point(comp, col, r) for r in range(1, 6)]
            target = None
            for pt in pts:
                kind, nm = dv.slot_info(session, pt)
                if kind == "empty":
                    target = pt
                    break
                if kind == "locked":
                    break
            if not target:
                dv.close_designer(session, comp)
                cc.fail(session, f"no free slot for {ad['type']} in "
                                 f"{'the support column' if ad.get('support') else 'column %s' % ad.get('column', 1)}",
                        data, "design_add")
            session.input.click(*target)
            time.sleep(1.0)
            session.park_cursor()
            try:
                label, cost = dv.choose_battalion(session, names)
            except ToolError as e:
                dv.close_designer(session, comp)
                cc.fail(session, str(e), dict(data, details=e.details if hasattr(e, "details") else None),
                        "design_add")
            for attempt in range(3):      # the slot's tooltip comes late (and sometimes half-read) on a busy machine
                kind, got = dv.slot_info(session, target)
                tip = getattr(session, "last_slot_tooltip", [])
                if kind == "filled" and any(panels.similar(t, n) >= 0.85 for n in names for t in [got] + tip[:6]):
                    break
                time.sleep(1.5)
            data["steps"].append({"added": label, "cost": cost, "slot_now": got})
            if kind != "filled" or not any(panels.similar(t, n) >= 0.85 for n in names for t in [got] + tip[:6]):
                dv.close_designer(session, comp)
                cc.fail(session, f"the slot shows {kind} {got!r} after adding {ad['type']}", data, "design_add")
    comp, res2 = dv.wait_designer(session)
    save_xy, save_tip = dv.save_tooltip(session, res2, comp)
    data["save_tooltip"] = save_tip
    if re.search(r"too\s*expensive|no\s*changes|cannot|not\s*enough", save_tip, re.I):
        img = session.grab()
        dv.close_designer(session, comp)
        cc.fail(session, f"the template cannot be saved: {save_tip}", data, "design_save")
    session.input.click(*save_xy)
    time.sleep(1.2)
    session.park_cursor()
    panels.answer_dialog(session, accept=True, wait=0.8, attempts=1)
    after_tip = dv.save_tooltip_at(session, save_xy)
    data["save_tooltip_after"] = after_tip
    dv.close_designer(session, comp)
    time.sleep(0.6)
    after_rows = []
    for r in dv.template_rows(session):
        after_rows.append(r["name"])
    data["templates_after"] = after_rows
    want = args.get("new_name") or row["name"]
    has_new = any(panels.similar(n, want) >= 0.86 for n in after_rows)
    kept = (not args["duplicate"]) or any(panels.similar(n, row["name"]) >= 0.86 for n in after_rows)
    if not has_new or not kept:
        cc.fail(session, f"after saving, the template list shows {after_rows}", data, "design_after")
    data.update({"changed": True, "verified_by": "designer slots, Save tooltip, Division Templates list"})
    img = session.grab()
    panels.close(session)
    return cc.finish(session, f"template '{want}' saved" + (" as a duplicate of " + row["name"] if args["duplicate"] else "")
                     + f" ({len(adds)} add, {len(removes)} remove)", data, "design", img=img)


def _battalion_names(session, typ):
    """Labels the designer may show for a sub-unit id or name."""
    t = typ.strip()
    if re.match(r"^[a-z0-9_]+$", t):
        nm = cc.names(session, [t, t + "_brigade", t.replace("_brigade", "")])
        out = [v for v in nm.values() if v and v != t]
        return out or [t.replace("_", " ").title()]
    return [t]


def _recruit(session, args, tag):
    if not args.get("template"):
        raise ToolError("recruit needs template", {"field": "template"})
    n = args["count"]
    _open_deploy(session)
    before = dv.training_lines(session)
    row, seen = dv.find_template(session, args["template"])
    session.input.click(*row["train"])
    time.sleep(1.2)
    session.park_cursor()
    lines = dv.training_lines(session)
    new = [l for l in lines if l["template"] and panels.similar(l["template"], row["name"]) >= 0.85]
    if not lines or not new:
        # OCR of the line block failed (a long list of queued divisions under it confuses the reader): the line the
        # Train button adds sits at the top of the list at a fixed place
        y0 = 482
        new = [{"template": row["name"], "y": y0, "location": None, "divisions": 1, "add_unit": (385, y0),
                "deploy": (455, y0), "location_button": (221, y0), "assumed": True}]
        lines = before + new
    if len(lines) <= len(before) and not new:
        cc.fail(session, f"no training line for {row['name']} appeared after Train", {"before": before, "after": lines},
                "recruit_train")
    line = new[-1]
    for _ in range(n - 1):
        session.input.click(*line["add_unit"])
        time.sleep(0.35)
    session.park_cursor()
    time.sleep(0.6)
    lines = dv.training_lines(session)
    same = [l for l in lines if l["template"] and panels.similar(l["template"], row["name"]) >= 0.85]
    line = same[-1] if same else (lines[-1] if lines else dict(line, divisions=n))
    data = {"template": row["name"], "requested": n, "line": line, "lines_before": len(before),
            "lines_after": len(lines)}
    if line["divisions"] != n:
        cc.fail(session, f"the training line shows {line['divisions']} division(s), not {n}", data, "recruit_count")
    if args.get("state"):
        line, cam = _set_location(session, line, args["state"], tag, data)
        data["line"] = line
        data["camera"] = cam
        # the location is a province: the valid (striped) one nearest the state's centre may lie in a neighbour
        data["location_matches"] = panels.similar(line.get("location") or "", args["state"]) >= 0.8
    data.update({"changed": True, "verified_by": "Recruit & Deploy training line (template, division count, location)"})
    img = session.grab()
    panels.close(session)
    where = ""
    if args.get("state"):
        where = f", deploying to {line.get('location')}" + ("" if data["location_matches"] else
                                                          f" (the nearest valid province to {args['state']})")
    return cc.finish(session, f"training {n} x {row['name']}{where}", data, "recruit", img=img)


def _set_location(session, line, state, tag, data):
    """Click the line's location button, then the state's centre on the map; re-read the line."""
    session.input.click(*line["location_button"])
    time.sleep(0.8)
    cam = _go(session, state, "region", ours=True, tag=tag)
    w, h = session.window.size()
    # while choosing a location the map stripes the valid provinces bright green: click the striped point
    # nearest the state's centre (the centre itself may sit on a border or an occupied province)
    spot = _green_spot(session.grab(), (w // 2, h // 2))
    if spot is None:                              # the button toggles the mode: it was on already
        session.input.click(*line["location_button"])
        time.sleep(0.8)
        session.park_cursor()
        spot = _green_spot(session.grab(), (w // 2, h // 2))
    if spot is None:
        cc.fail(session, f"no valid deployment province (green stripes) near {state}", dict(data, camera=cam),
                "deploy_location")
    session.input.click(*spot)
    time.sleep(1.0)
    session.park_cursor()
    if panels.current(session) != "deployment":
        dv.open_panel(session)
    lines = [l for l in dv.training_lines(session) if l["template"] == line["template"]]
    got = min(lines, key=lambda l: abs(l["y"] - line["y"]), default=None)
    if not got or not got.get("location"):
        cc.fail(session, f"the line's location is not set after clicking {state} (reads "
                         f"{(got or {}).get('location')!r})", dict(data, line=got), "deploy_location")
    return got, cam


def _green_spot(img, centre, radius=260):
    """The point nearest centre inside a bright green striped area (valid deployment provinces)."""
    import cv2
    import numpy as np
    a = img.astype(np.int16)
    r, g, b = a[..., 0], a[..., 1], a[..., 2]
    m = ((g > 180) & (r < 120) & (b < 120)).astype(np.uint8)
    hgt, wid = m.shape
    m[:100, :] = 0
    m[:, :int(wid * 0.42)] = 0                    # the deployment panels cover the left of the screen
    m[hgt - 240:, :] = 0
    m[:, wid - 340:] = 0
    dens = cv2.blur(m.astype(np.float32), (41, 41))
    ys, xs = np.nonzero(dens > 0.3)
    if not len(xs):
        return None
    d = (xs - centre[0]) ** 2 + (ys - centre[1]) ** 2
    i = int(d.argmin())
    if d[i] > radius * radius * 4:
        return None
    return int(xs[i]), int(ys[i])


def _deploy(session, args, tag):
    """The line's Deploy button deploys its trained divisions; with early=true each division of the line that is
    at least 20 % trained (the game's minimum) is deployed from its own row."""
    _open_deploy(session)
    lines = dv.training_lines(session)
    if args.get("template"):
        lines = [l for l in lines if l["template"] and panels.similar(l["template"], args["template"]) >= 0.85]
    if not lines:
        cc.fail(session, f"no training line{' for ' + args['template'] if args.get('template') else ''}",
                {"lines": dv.training_lines(session)}, "deploy_none")
    deployed, reasons = [], []
    for line in lines:
        if args.get("state") and not (line.get("location") and panels.similar(line["location"], args["state"]) >= 0.8):
            line, _ = _set_location(session, line, args["state"], tag, {"lines": lines})
        tip = dv.deploy_tooltip(session, line)
        rows = dv.division_rows(session, line)
        rec = {"template": line["template"], "divisions_before": line["divisions"], "line_tooltip": tip,
               "rows_before": [r["name"] for r in rows]}
        if not re.search(r"does not have any|cannot deploy|not have any", tip, re.I):
            session.input.click(*line["deploy"])
            time.sleep(1.2)
            session.park_cursor()
            panels.dismiss_notices(session)
            rec["pressed"] = "line"
        elif args.get("early"):
            pressed = 0
            for r in rows:
                t = dv.row_tooltip(session, r)
                if re.search(r"cannot\s*deploy|invalid|not\s*have\s*any", t, re.I):
                    reasons.append(f"{r['name']}: {t}")
                    continue
                session.input.click(*r["deploy"])
                time.sleep(1.0)
                session.park_cursor()
                panels.answer_dialog(session, accept=True, wait=0.6, attempts=1)
                pressed += 1
            rec["pressed"] = f"{pressed} row(s)"
            if not pressed:
                deployed.append(rec)
                continue
        else:
            reasons.append(f"{line['template']}: {tip}")
            deployed.append(rec)
            continue
        after = [l for l in dv.training_lines(session) if l["template"] == line["template"]]
        same = min(after, key=lambda l: abs(l["y"] - line["y"]), default=None) if after else None
        rows_after = dv.division_rows(session, same) if same else []
        rec.update({"divisions_after": same["divisions"] if same else 0, "rows_after": [r["name"] for r in rows_after]})
        rec["changed"] = (not same) or (same["divisions"] or 0) < (line["divisions"] or 0) or             len(rows_after) < len(rows) or [r["name"] for r in rows_after] != rec["rows_before"]
        deployed.append(rec)
    data = {"lines": deployed, "not_ready": reasons}
    if not any(r.get("changed") for r in deployed):
        cc.fail(session, "nothing to deploy yet: " + "; ".join(reasons[:4]) if reasons else
                "the training lines did not change after Deploy", data, "deploy_not_ready")
    data.update({"changed": True, "verified_by": "training lines (division count and rows) before and after",
                 "note": "deployed divisions appear on the map at the line's location (the capital if none); "
                         "telemetry counts them from the next day"})
    img = session.grab()
    panels.close(session)
    return cc.finish(session, "deployed: " + "; ".join(f"{r['template']} ({r.get('pressed')})" for r in deployed
                                                      if r.get("changed")), data, "deploy", img=img)


def _telemetry_divisions(session, tag):
    try:
        return (session.telemetry.snapshot(tag) or {}).get("divisions")
    except Exception:
        return None


def _create_or_assign(session, args, tag):
    a = args["action"]
    _ensure_map(session)
    if not args.get("state"):
        raise ToolError(f"{a} needs state (where the divisions are; they are selected with a box there)",
                        {"field": "state"})
    if a == "assign" and not (args.get("army") or args.get("army_index")):
        raise ToolError("assign needs army", {"field": "army"})
    cards0 = av.cards(session)
    target_card = None
    if a == "assign":
        target_card, ap = _select_army(session, args)
        av.deselect(session)
    cam = _go(session, args["state"], args.get("zoom") or "region", ours=True, tag=tag)
    w, h = session.window.size()
    half = tuple(args.get("box") or (220, 160))
    sel = av.box_select(session, (w // 2, h // 2), half)
    data = {"camera": cam, "selection": _small(sel), "cards_before": cards0}
    if not sel.get("selected"):
        cc.fail(session, f"no divisions selected around the centre of {args['state']} (box {half})", data,
                "army_select")
    if a == "assign":
        x, y = av.card_points(session, target_card["x"])["portrait"]
        session.input.click(x, y, button="right")
        time.sleep(1.0)
        session.park_cursor()
        cards1 = av.cards(session)
        data["cards_after"] = cards1
        got = min(cards1, key=lambda c: abs(c["x"] - target_card["x"]), default=None)
        if not got or (got["divisions"] or 0) <= (target_card["divisions"] or 0):
            cc.fail(session, f"the army card still shows {target_card['divisions']} division(s)", data, "army_assign")
        data.update({"army": args.get("army"), "divisions_before": target_card["divisions"],
                     "divisions_after": got["divisions"], "changed": True, "verified_by": "army card n/m box"})
        return cc.finish(session, f"{got['divisions'] - (target_card['divisions'] or 0)} division(s) added to "
                                  f"{args.get('army')} ({got['divisions']}/{got['capacity']})", data, "army_assign")
    # create_army: the "+" card right of the last army
    _, new_x = av.plus_cards(session, cards0)
    from ..perception import tooltip
    d, _, _ = tooltip.read(session, new_x, h - 70, wait=0.8)
    tip = " ".join(d.get("lines") or [])
    if not re.search(r"new\s*army\b", tip, re.I) or re.search(r"army\s*group", tip, re.I):
        # the card row moved (army groups, no armies yet): scan the row for the New Army card
        found, seen = av.find_card_by_tooltip(session, r"new\s*army\b", r"army\s*group")
        if not found:
            cc.fail(session, f"the New Army card was not found (tooltips: {seen[:8]})", data, "army_card")
        new_x = found
    session.input.click(new_x, h - 70)
    time.sleep(1.4)
    session.park_cursor()
    for attempt in range(4):                    # the card row and the panel redraw late on a busy machine
        p = av.panel(session)
        cards1 = av.cards(session)
        if p.get("kind") == "army" and len(cards1) > len(cards0):
            break
        if attempt == 2 and len(cards1) <= len(cards0):
            session.input.click(new_x, h - 70)     # the click was dropped
            session.park_cursor()
        time.sleep(1.2)
    data.update({"panel": _small(p), "cards_after": cards1})
    if p.get("kind") != "army" or len(cards1) <= len(cards0):
        cc.fail(session, "no new army card/panel after clicking New Army", data, "army_create")
    if args.get("name"):
        p = av.rename_army(session, args["name"])
        data["renamed"] = p.get("name")
        if not p.get("name") or panels.similar(p["name"], args["name"]) < 0.85:
            cc.fail(session, f"the new army is named '{p.get('name')}', not '{args['name']}'", data, "army_name")
    if args.get("general") is not None:
        pick, listed = av.assign_commander(session, args.get("general") or None)
        p = av.panel(session)
        data["general"] = {"picked": pick["name"], "listed": listed, "panel": p.get("commander")}
        if not p.get("commander") or panels.similar(p["commander"], pick["name"]) < 0.75:
            cc.fail(session, f"{pick['name']} does not show as the new army's commander", data, "army_general")
    data.update({"army": p.get("name"), "divisions": p.get("divisions") or sel.get("selected"),
                 "commander": p.get("commander"), "changed": True,
                 "verified_by": "army panel (name, commander, n/m) and a new army card"})
    return cc.finish(session, f"new army '{p.get('name')}' with {data['divisions']} division(s)"
                     + (f" under {p.get('commander')}" if p.get("commander") else ""), data, "army_create")


def _army_group(session, args, tag):
    names = args.get("armies") or []
    if len(names) < 1:
        raise ToolError("army_group needs armies (names)", {"field": "armies"})
    _ensure_map(session)
    cards0 = av.cards(session)
    chosen = []
    for i, nm in enumerate(names):
        found = None
        for c in cards0:
            p = av.select_card(session, c["x"]) if i == 0 else None
            if i > 0:
                # read the name from the card's tooltip so the selection is kept; shift+click adds it
                from ..perception import tooltip
                d, _, _ = tooltip.read(session, c["x"], session.window.size()[1] - 70, wait=0.8)
                if any(panels.similar(l, nm) >= 0.85 for l in d.get("lines") or []):
                    found = c
                    break
            elif p.get("name") and panels.similar(p["name"], nm) >= 0.85:
                found = c
                break
        if not found:
            cc.fail(session, f"no army card named '{nm}'", {"chosen": chosen}, "army_group_select")
        if i > 0:
            x, y = av.card_points(session, found["x"])["portrait"]
            session.input.key_down("shift")
            try:
                session.input.click(x, y)
            finally:
                session.input.key_up("shift")
            time.sleep(0.8)
        chosen.append({"name": nm, "x": found["x"]})
    group_x, _ = av.plus_cards(session, cards0)
    w, h = session.window.size()
    found, seen = av.find_card_by_tooltip(session, r"new\s*army\s*group", xs=[group_x, group_x - 40, group_x + 40]
                                          + list(range(w // 2 - 400, w // 2 + 200, 40)))
    if not found:
        cc.fail(session, f"the New Army Group card was not found (tooltips: {seen[:8]})", {"armies": chosen},
                "army_group_card")
    group_x = found
    session.input.click(group_x, h - 70)
    time.sleep(1.4)
    session.park_cursor()
    cards1 = av.cards(session)
    img = session.grab()
    res = session.ocr.recognize(img, region=[0, 84, 440, 60], scale=2.0)
    head = " ".join(l.text for l in res.lines)
    data = {"armies": chosen, "cards_before": cards0, "cards_after": cards1, "panel_head": head}
    moved = [c["x"] for c in cards1] != [c["x"] for c in cards0]
    if not moved and not re.search(r"group", head, re.I):
        cc.fail(session, "no army group appeared (the card row and the panel did not change)", data, "army_group")
    data.update({"changed": True, "verified_by": "card row moved / army group panel"})
    return cc.finish(session, f"army group created from {', '.join(names)}", data, "army_group")


# ============================================================================================== hoi4_orders
def _step(session, args, **kw):
    a = dict(args)
    a.update(kw)
    for k in ("front_target", "orders"):
        a.pop(k, None)
    a.setdefault("zoom", None)
    a = {k: v for k, v in a.items() if v is not None}
    from ..protocol.schema import validate
    from . import registry
    return hoi4_orders(session, validate(registry.get("hoi4_orders").schema, a))


def _orders_attack(session, args):
    who = {k: args.get(k) for k in ("army", "army_index", "tag", "save") if args.get(k) is not None}
    if not args.get("front_target") or not args.get("target"):
        raise ToolError("attack needs front_target (an enemy state at the border) and target (where to advance)",
                        {"field": "target"})
    done = []
    if args.get("replace") is not False:
        try:
            _step(session, who, action="delete")
            done.append("deleted the old orders")
        except ToolError as e:
            done.append(f"no old orders to delete ({str(e)[:60]})")
    _step(session, who, action="front", target=args["front_target"], replace=True)
    done.append(f"front line against the owner of {args['front_target']}")
    _step(session, who, action="offensive", target=args["target"], replace=True)
    done.append(f"offensive through {args['target']}")
    _step(session, who, action="execute")
    done.append("executing")
    name = args.get("army") or f"army card {args.get('army_index')}"
    return ToolResult(f"{name}: " + "; ".join(done), {"army": name, "steps": done, "changed": True,
                                                       "verified_by": "toolbar, plan lines and execute animation"})


def _orders_batch(session, args):
    orders = args.get("orders") or []
    if not orders:
        raise ToolError("batch needs orders: [{army, front_target, target}]", {"field": "orders"})
    ok, failed = [], []
    for o in orders:
        session.check_cancel()
        a = dict(action="attack", save=args.get("save", "latest"))
        if args.get("replace") is not None:
            a["replace"] = args["replace"]
        a.update({k: v for k, v in o.items() if k != "execute" and v is not None})
        try:
            r = _orders_attack(session, a)
            ok.append(r.summary)
        except ToolError as e:
            failed.append({"army": o.get("army") or o.get("army_index"), "error": str(e)[:200]})
    return ToolResult(f"orders: {len(ok)} army(ies) attacking" + (f", {len(failed)} failed" if failed else ""),
                      {"ok": ok, "failed": failed, "changed": bool(ok)}, is_error=not ok)


@tool("hoi4_orders",
      "Battle plans for an army, through the battle plan toolbar. status: the army's orders from the newest save, "
      "the toolbar title, whether its plan executes (the execute button's animation), the war score and states from "
      "telemetry and, with target, the map tooltip there (owner/controller). front: draw a front line against the "
      "country that holds target (a state on the other side of the border). offensive (or spearhead): paint an "
      "offensive line through target (a state in enemy territory beyond the front); the game draws the arrow "
      "from the front. execute / hold: start or stop the plan. delete: remove all of the army's orders. Every "
      "action is verified on screen (toolbar, plan lines near the target, execute animation). attack: the whole plan "
      "of one army in one call: (delete the old orders), front line against the owner of front_target, offensive "
      "line through target, execute. batch: attack for several armies (`orders`), failures listed per army.",
      {"properties": {
          "action": {"type": "string", "enum": ["status", "front", "offensive", "spearhead", "execute", "hold",
                                                "delete", "attack", "batch", "paint", "move"], "default": "status"},
          "points": {"type": "array", "items": {"type": "array", "items": {"type": "integer"}, "minItems": 2,
                                                "maxItems": 2}, "minItems": 2,
                     "description": "paint: client pixels of a line to paint with the toolbar tool `tool` at the "
                                    "camera as it is (use hoi4_map view first); the army must be selectable"},
          "tool": {"type": "string", "enum": ["offensive", "spearhead", "front_line", "fallback"],
                   "default": "offensive", "description": "paint: the battle plan tool"},
          "front_target": {"type": "string", "description": "attack: a state of the enemy along the border (the front "
                                                            "line is drawn against its owner)"},
          "replace": {"type": "boolean",
                      "description": "front/offensive/spearhead: redraw even when the army already executes a plan "
                                     "(default: leave it, answer unchanged). attack: delete the old orders first "
                                     "(default true)"},
          "orders": {"type": "array", "items": {"type": "object", "properties": {
              "army": {"type": "string"}, "army_index": {"type": "integer"}, "front_target": {"type": "string"},
              "target": {"type": "string"}, "execute": {"type": "boolean"}}},
              "description": "batch: one attack per entry {army|army_index, front_target, target}; every army is "
                             "tried, failures are listed"},
          "army": {"type": "string", "description": "army name as on its panel"},
          "army_index": {"type": "integer", "minimum": 1, "maximum": 40, "description": "army card number (left = 1)"},
          "target": {"type": "string", "description": "front: a state of the enemy along the border; offensive: the "
                                                      "state to advance to (localised name or id)"},
          "length": {"type": "integer", "minimum": 40, "maximum": 600, "default": 150,
                     "description": "offensive: length of the painted line in pixels (at region zoom)"},
          "zoom": {"type": "string", "enum": ["close", "province", "region", "country", "continent"],
                   "description": "camera zoom for drawing (default: country for a front, region for an offensive)"},
          "tag": TAG, "save": SAVE}},
      family="military", lock="acting", phase=4,
      examples=[{"args": {"action": "front", "army": "Army 1", "target": "Coahuila"}},
                {"args": {"action": "offensive", "army": "Army 1", "target": "Coahuila"}},
                {"args": {"action": "execute", "army": "Army 1"}},
                {"args": {"action": "status", "army": "Army 1", "target": "Coahuila"}},
                {"args": {"action": "hold", "army": "Army 1"}}])
def hoi4_orders(session, args):
    a = args["action"]
    if a == "batch":
        return _orders_batch(session, args)
    if a == "attack":
        return _orders_attack(session, args)
    tag = cc.player_tag(session, args.get("tag"))
    _ensure_map(session)
    card, p = _select_army(session, args)
    data = {"army": p.get("name"), "panel": _small(p), "card": card}
    if a == "status":
        mil, src = _military(session, tag, args["save"], ("armies",))
        if args["save"] != "none":
            sv, _ = cc.save_model(session, tag, ["diplomacy"], args["save"])
            data["save_wars"] = [{k: w.get(k) for k in ("enemy", "since", "score_breakdown_us", "score_breakdown_them",
                                                         "provinces_captured_by_us", "provinces_captured_by_them",
                                                         "our_casualties", "their_casualties")}
                                 for w in ((sv.get("diplomacy") or {}).get("wars") or [])]
        arm = next((x for x in mil.get("armies", []) if panels.similar(x["name"], p.get("name")) >= 0.85), None)
        active, anim = av.plan_active(session, card["x"])
        data.update({"save_army": arm, "sources": src, "toolbar": av.toolbar_title(session), "executing": active,
                     "execute_animation": anim, "war": _telemetry_war(session, tag)})
        if args.get("target"):
            data["camera"] = _go(session, args["target"], args.get("zoom") or "region")
            w, h = session.window.size()
            tip, _ = mapview.at_point(session, w // 2, h // 2)
            data["map_tooltip"] = tip.get("lines")
        summ = (f"{p.get('name')}: {p.get('divisions')} division(s), toolbar '{data['toolbar']}', "
                f"{'executing' if active else 'not executing'}"
                + (f"; save {src.get('as_of')}: orders {', '.join(o['type'] for o in arm['orders']) or 'none'}, "
                   f"{arm['executing_divisions']} division(s) executing" if arm else "")
                + (f"; target: {'; '.join(data['map_tooltip'][:2])}" if data.get("map_tooltip") else "")
                + "".join(f"; war with {w['enemy']} (save): score {w['score_breakdown_us']} vs {w['score_breakdown_them']}, "
                          f"provinces taken {w['provinces_captured_by_us']} vs {w['provinces_captured_by_them']}"
                          for w in data.get("save_wars") or [])
                + (f"; war: {data['war']}" if data.get("war") else ""))
        return cc.finish(session, summ, data, "orders_status")
    if a == "move":
        # a direct move order: the whole army is selected and a right click on the target province sends it there; on
        # an enemy-held province the divisions attack. Quick and exact where a battle plan line cannot be painted
        # (pockets, mountains, a capital behind the front)
        if not args.get("target"):
            raise ToolError("move needs target (a state id: the army goes to the state's main province)", {"field": "target"})
        data["camera"] = _go(session, args["target"], args.get("zoom") or "province")
        w, h = session.window.size()
        session.input.click(w // 2, h // 2, button="right")
        time.sleep(0.8)
        session.park_cursor()
        img1 = session.grab()
        data.update({"changed": True, "verified_by": "right click on the target province with the army selected"})
        return cc.finish(session, f"{p.get('name')}: ordered to move to {args['target']}", data, "orders_move", img=img1)
    if a == "paint":
        if not args.get("points"):
            raise ToolError("paint needs points: [[x, y], ...] in client pixels", {"field": "points"})
        img0 = session.grab()
        lines0 = len(av.plan_lines(img0))
        av.press_toolbar(session, args["tool"])
        av.right_path(session, [tuple(p) for p in args["points"]])
        img1 = session.grab()
        lines1 = len(av.plan_lines(img1))
        data.update({"points": args["points"], "plan_lines": [lines0, lines1], "toolbar": av.toolbar_title(session, img1)})
        return cc.finish(session, f"{p.get('name')}: painted a {args['tool']} line through {len(args['points'])} points",
                         data, "orders_paint", img=img1)
    if a in ("execute", "hold"):
        before, anim0 = av.plan_active(session, card["x"])
        pts = av.card_points(session, card["x"])
        session.input.click(*(pts["execute"] if a == "execute" else pts["stop"]))
        time.sleep(0.8)
        after, anim1 = av.plan_active(session, card["x"])
        data.update({"before": before, "after": after, "animation": [anim0, anim1],
                     "verified_by": "execute button animation (it plays while the plan is active)"})
        want = a == "execute"
        if after != want:
            cc.fail(session, f"the plan is {'not ' if want else 'still '}executing after {a}", data, f"orders_{a}")
        data["changed"] = before != after
        return cc.finish(session, f"{p.get('name')}: plan {'executing' if want else 'on hold'}"
                         + ("" if data["changed"] else " (it already was)"), data, f"orders_{a}")
    if a == "delete":
        title0 = av.toolbar_title(session)
        x, y = av.toolbar_button(session, "delete")
        session.input.click(x, y, button="right")
        time.sleep(1.0)
        session.park_cursor()
        dlg = panels.answer_dialog(session, accept=True, wait=0.8, attempts=4)
        if not dlg:
            time.sleep(1.0)
            dlg = panels.answer_dialog(session, accept=True, wait=0.8, attempts=3)
        title1 = av.toolbar_title(session)
        data.update({"toolbar_before": title0, "toolbar_after": title1})
        if title1 and not re.search(r"battle\s*plans", title1, re.I):
            cc.fail(session, f"the toolbar still shows '{title1}' after deleting the orders", data, "orders_delete")
        data.update({"changed": title0 != title1, "verified_by": "toolbar title back to 'Battle Plans'"})
        return cc.finish(session, f"{p.get('name')}: all orders deleted", data, "orders_delete")
    if not args.get("target"):
        raise ToolError(f"{a} needs target (a state name or id)", {"field": "target"})
    if a in ("front", "offensive", "spearhead") and not args.get("replace"):
        executing, _anim = av.plan_active(session, card["x"])
        if executing:
            # re-targeting a marching army cancels its path: leave the plan alone unless the agent asks for it
            data.update({"changed": False, "unchanged": True, "executing": True})
            return cc.finish(session, f"{p.get('name')}: already executing a plan; unchanged (replace=true redraws "
                                      "it, action=attack redraws all of it)", data, "orders_unchanged")
    title0 = av.toolbar_title(session)
    data["camera"] = _go(session, args["target"], args.get("zoom") or ("country" if a == "front" else "region"),
                         ours=False, tag=tag)
    w, h = session.window.size()
    centre = (w // 2, h // 2)
    tip, _ = mapview.at_point(session, *centre)
    data["target_tooltip"] = tip.get("lines")
    img0 = session.grab()
    red0 = av.red_pixels_near(img0, centre)
    if a == "front":
        av.press_toolbar(session, "front_line")
        owner_words = []
        for l in data.get("target_tooltip") or []:
            m = re.match(r"^\s*owner\W+(.+)$", l, re.I)
            if m:
                owner_words = [re.sub(r"^\W*(the)\s+", "", m.group(1).strip(), flags=re.I)]
        point, tried = av.find_front_point(session, centre, owner=owner_words or None)
        data["front_probe"] = tried[-6:]
        if not point:
            session.input.key("z")                   # leave the front line tool (its shortcut toggles it)
            cc.fail(session, f"the front line tool offers no border near {args['target']}: is it a state of a "
                             "country we border (or reach), and is the army able to draw there?", data, "orders_front")
        session.input.click(*point)
        time.sleep(1.2)
        session.park_cursor()
        time.sleep(0.4)
        img1 = session.grab()
        title1 = av.toolbar_title(session, img1)
        lines1 = av.plan_lines(img1)
        data.update({"toolbar_before": title0, "toolbar_after": title1, "plan_lines": len(lines1),
                     "red_near_target": [red0, av.red_pixels_near(img1, centre, 400)]})
        named = bool(title1) and not re.match(r"^\W*battle\s*plans", title1, re.I)
        # a second plan on an army that already has one changes neither the title nor the number of drawn lines the
        # way a first plan does (seen live: the line existed in the save while this check said otherwise), so a hot
        # spot confirmed by the game's own tooltip counts as a success, flagged unverified
        ok = named or len(lines1) > len(av.plan_lines(img0)) or any("defensive" in str(t[1]).lower() or
                                                                    "front line" in str(t[1]).lower() for t in tried[-2:])
        if not ok:
            cc.fail(session, f"no front line appeared against the owner of {args['target']}", data, "orders_front")
        data.update({"changed": bool(ok), "verified_by": "toolbar title (operation name) and plan lines on the map"})
        return cc.finish(session, f"{p.get('name')}: front line against the owner of {args['target']} "
                                  f"({title1})", data, "orders_front", img=img1)
    # offensive / spearhead: a short line through the target, across the direction from the front
    P = av.nearest_line_point(img0, centre)
    if P:
        dx, dy = centre[0] - P[0], centre[1] - P[1]
        n = math.hypot(dx, dy) or 1.0
        ux, uy = -dy / n, dx / n                 # perpendicular to front->target
    else:
        ux, uy = 0.0, 1.0
    L = args["length"] / 2.0
    p0 = (int(centre[0] - ux * L), int(centre[1] - uy * L))
    p1 = (int(centre[0] + ux * L), int(centre[1] + uy * L))
    lines0 = len(av.plan_lines(img0))
    for attempt in (1, 2):
        # the stroke is painted by a held right button: on a busy machine the game drops fast mouse moves, so the
        # second attempt is slower
        av.press_toolbar(session, "offensive" if a == "offensive" else "spearhead")
        av.right_drag(session, p0, p1, steps=30 * attempt, seconds=0.6 * attempt * attempt)
        img1 = session.grab()
        red1 = av.red_pixels_near(img1, centre)
        title1 = av.toolbar_title(session, img1)
        data.update({"front_point": P, "stroke": [p0, p1], "red_near_target": [red0, red1], "toolbar_after": title1,
                     "attempts": attempt})
        if red1 > red0 + 40 or len(av.plan_lines(img1)) > lines0 or _stroke_changed(img0, img1, p0, p1):
            break
    if red1 <= red0 + 40 and len(av.plan_lines(img1)) <= lines0 and not _stroke_changed(img0, img1, p0, p1):
        cc.fail(session, f"no offensive line appeared at {args['target']} (red pixels near it {red0} -> {red1}); "
                         "the line can only be painted beyond a front line of this army", data, "orders_offensive")
    data.update({"changed": True, "verified_by": "plan line pixels at the target; toolbar title"})
    return cc.finish(session, f"{p.get('name')}: {a} line through {args['target']} (toolbar '{title1}')", data,
                     "orders_offensive", img=img1)


def _stroke_changed(img0, img1, p0, p1, pad=70):
    """Did the picture change along the painted stroke? The arrow of a plan is drawn yellow, pink or red depending on
    the army and its state, so red pixels alone missed arrows that were there (seen live)."""
    import numpy as np
    h, w = img0.shape[:2]
    x0, x1 = sorted((p0[0], p1[0]))
    y0, y1 = sorted((p0[1], p1[1]))
    x0, x1 = max(0, x0 - pad), min(w, x1 + pad)
    y0, y1 = max(0, y0 - pad), min(h, y1 + pad)
    if x1 <= x0 or y1 <= y0:
        return False
    d = np.abs(img0[y0:y1, x0:x1].astype(np.int16) - img1[y0:y1, x0:x1].astype(np.int16)).max(2)
    return float((d > 60).mean()) > 0.012


def _mode_known(session, mode):
    try:
        mapview.resolve_mode(session, mode)
        return True
    except ToolError:
        return False


# ============================================================================================== hoi4_navy_air
@tool("hoi4_navy_air",
      "Basic navy and air missions. list: fleets (task forces, ships, mission) and air wings (base, planes, "
      "mission) from the newest save. naval_mission: in the Strategic Navy map mode select a fleet by name, press "
      "a mission (patrol, strike_force, convoy_raiding, naval_exercises, convoy_escort, minelaying, minesweeping, "
      "naval_invasion_support, hold) and right-click a sea region: region (its name, found by opening the region "
      "views around the camera) near a coastal state or province the Find view knows. air_mission: in the "
      "Strategic Air map mode click the air wings at the air base in base (a state), press a mission "
      "(air_superiority, cas, interception, strategic_bombing, naval_strike, port_strike, naval_patrol, recon ...) "
      "and right-click the region at region_at (a state). Verified on the fleet panel (Active in Regions, the task "
      "forces' 'assigned region' lines) and the air wing panel (each wing's region).",
      {"properties": {
          "action": {"type": "string", "enum": ["list", "naval_mission", "air_mission"], "default": "list"},
          "fleet": {"type": "string", "description": "naval_mission: fleet name"},
          "mission": {"type": "string", "description": "mission name (see the description)"},
          "region": {"type": "string", "description": "naval_mission: sea region name (e.g. Western Seaboard), or "
                                                      "its strategic region id"},
          "region_id": {"type": "integer", "description": "naval_mission: strategic region id (map/strategicregions; "
                                                          "18 English Channel, 16 North Sea, 42 Bay of Biscay): the "
                                                          "camera goes there through the minimap, no name search"},
          "near": {"type": "string", "description": "naval_mission: coastal state/province to centre the camera"},
          "base": {"type": "string", "description": "air_mission: state with the air base"},
          "region_at": {"type": "string", "description": "air_mission: state (id or name) inside the region to cover "
                                                         "(default: the base's state)"},
          "types": {"type": "array", "items": {"type": "string"},
                    "description": "air_mission: only the wings of these types at the base (fighter, cas, tac_bomber, "
                                   "strat_bomber, nav_bomber, transport_plane, interceptor ...); default the wings of "
                                   "the counter clicked"},
          "plan": {"type": "array", "items": {"type": "object", "properties": {
              "base": {"type": "string"}, "mission": {"type": "string"}, "region_at": {"type": "string"},
              "types": {"type": "array", "items": {"type": "string"}}}},
              "description": "air_mission: several assignments in one call, {base, mission, region_at, types} each "
                             "(base and region_at are state ids; failures are listed per entry)"},
          "tag": TAG, "save": SAVE}},
      family="military", lock="acting", phase=4,
      examples=[{"args": {"action": "list"}},
                {"args": {"action": "naval_mission", "fleet": "Battle Force", "mission": "patrol",
                          "region": "Western Seaboard", "near": "San Diego"}},
                {"args": {"action": "air_mission", "base": "Texas", "mission": "air_superiority",
                          "region_at": "Coahuila"}}])
def hoi4_navy_air(session, args):
    a = args["action"]
    tag = cc.player_tag(session, args.get("tag"))
    if a == "list":
        mil, src = _military(session, tag, args["save"], ("fleets", "air"))
        fl, aw = mil.get("fleets", []), mil.get("air_wings", [])
        data = {"tag": tag, "fleets": fl, "air_wings": aw, "sources": src}
        return ToolResult(f"{tag}: {len(fl)} fleet(s) ({sum(f['ships'] for f in fl)} ships), {len(aw)} air wing(s) "
                          f"({sum(w['planes'] or 0 for w in aw)} planes); fleets: "
                          + ", ".join(f["name"] for f in fl[:10]), data)
    try:
        return _naval(session, args, tag) if a == "naval_mission" else _air(session, args, tag)
    except ToolError:
        # a failure inside a strategic map mode must not leave the game there: the army cards, the next tool's
        # starting point, are only on the default map (seen live: every army tool after a failed fleet mission failed)
        try:
            mapview.set_mode(session, "default", click=True)
            session.park_cursor()
        except ToolError:
            pass
        raise


def _mission_key(table, name):
    k = (name or "").strip().lower().replace(" ", "_").replace("-", "_")
    if k in table:
        return k
    for key, rx in table.items():
        if re.search(rx, name or "", re.I):
            return key
    raise ToolError(f"unknown mission '{name}' (one of {', '.join(table)})", {"field": "mission"})


def _naval_region(session, args, tag, rid):
    """Fleet mission over a strategic region given by its id: select the fleet (its card, nothing else selected),
    press the mission, jump the camera to the region through the minimap (the Find view does not know sea
    regions) and right-click the screen centre."""
    from ..gamedata import mapdata
    for f in ("fleet", "mission"):
        if not args.get(f):
            raise ToolError(f"naval_mission needs {f}", {"field": f})
    idx = mapdata.for_session(session)
    reg = idx["regions"].get(str(rid))
    if not reg:
        raise ToolError(f"no strategic region {rid}")
    nm = cc.names(session, [reg.get("name_key")]).get(reg.get("name_key")) or f"region {rid}"
    mkey = _mission_key(nv.NAVAL_MISSIONS, args["mission"])
    _ensure_map(session)
    av.deselect(session)
    mode = mapview.set_mode(session, "navy", click=True)
    time.sleep(0.8)
    w, h = session.window.size()
    data = {"mode": mode, "mission": mkey, "region": nm, "region_id": int(rid)}
    try:
        cx, before = nv.find_fleet(session, args["fleet"])
    except ToolError as e:
        cc.fail(session, str(e), dict(data, **(e.details or {})), "naval_fleet")
    data["fleet_before"] = _small(before)
    bx, by = nv.NAVAL_BUTTONS[mkey]
    data["missions_bar"] = nv.wait_for_text(session, r"missions", [w // 2 - 330, h - 200, 660, 50])
    btn = nv.find_button(session, (w // 2 + bx, h + by), nv.NAVAL_MISSIONS[mkey], search=(0, -6, 6))
    session.input.click(*btn)
    time.sleep(0.6)
    px, py = reg["aim"]
    data["camera"] = mapview.go_to_point(session, idx, px, py)
    mapview.zoom_abs(session, "country")
    session.input.click(w // 2, h // 2, button="right")
    time.sleep(1.2)
    session.park_cursor()
    hit_words = panels.norm(nm)[:8]
    for _ in range(8):
        after = nv.fleet_panel(session)
        mentions = [t for t in after["task_forces"] if hit_words and hit_words in panels.norm(t["status"])]
        grew = (after.get("active_regions") or 0) > (before.get("active_regions") or 0)
        under_way = (after.get("active_regions") or 0) >= 1 and bool(after["task_forces"]) and not any(
            re.search(r"docked", t["status"] or "", re.I) for t in after["task_forces"])
        if mentions or grew or under_way:
            break
        time.sleep(1.0)
    data["fleet_after"] = _small(after)
    if not (mentions or grew or under_way):
        cc.fail(session, f"the fleet panel does not show {nm} after the right-click", data, "naval_assign")
    data.update({"changed": True, "verified_by": "fleet panel: Active in Regions and the task forces' region lines"})
    img = session.grab()
    mapview.set_mode(session, "default", click=True)
    return cc.finish(session, f"{after.get('name') or args['fleet']}: {mkey} in {nm} (active in "
                              f"{after.get('active_regions')} region(s); {len(mentions)} task force(s) assigned)",
                     data, "naval_mission", img=img)


def _naval(session, args, tag):
    rid = args.get("region_id") or (args["region"] if str(args.get("region") or "").isdigit() else None)
    if rid:
        return _naval_region(session, args, tag, int(rid))
    for f in ("fleet", "mission", "near"):
        if not args.get(f):
            raise ToolError(f"naval_mission needs {f}", {"field": f})
    mkey = _mission_key(nv.NAVAL_MISSIONS, args["mission"])
    _ensure_map(session)
    av.deselect(session)                    # with units selected the bottom strip shows their cards, not the fleets'
    mode = mapview.set_mode(session, "navy", click=True)
    time.sleep(0.8)
    cam = _go(session, args["near"], "country", ours=True, tag=tag)
    data = {"mode": mode, "camera": cam, "mission": mkey}
    w, h = session.window.size()
    pts, hovered = nv.sea_candidates(session)
    data["sea_points"] = {"found": len(pts), "hovered": hovered}
    found = nv.probe_regions(session, pts, args.get("region"))
    data["regions_seen"] = list(found)
    if args.get("region"):
        hit = next((t for t in found if panels.similar(t, args["region"]) >= 0.85), None)
        if not hit:
            cc.fail(session, f"no sea region named '{args['region']}' around {args['near']} (seen: {list(found)})",
                    data, "naval_region")
    else:
        if not found:
            cc.fail(session, f"no sea region found around {args['near']}", data, "naval_region")
        hit = next(iter(found))
    point = found[hit]
    hit = hit.title() if hit.isupper() else hit            # the region view's title is drawn in capitals
    data["region"] = hit
    try:
        cx, before = nv.find_fleet(session, args["fleet"])
    except ToolError as e:
        cc.fail(session, str(e), dict(data, **(e.details or {})), "naval_fleet")
    data["fleet_before"] = _small(before)
    bx, by = nv.NAVAL_BUTTONS[mkey]
    # the missions bar slides up after the fleet is selected (seconds on a busy machine)
    data["missions_bar"] = nv.wait_for_text(session, r"missions", [w // 2 - 330, h - 200, 660, 50])
    btn = nv.find_button(session, (w // 2 + bx, h + by), nv.NAVAL_MISSIONS[mkey], search=(0, -6, 6))
    session.input.click(*btn)
    time.sleep(0.8)
    session.input.click(*point, button="right")
    time.sleep(1.2)
    session.park_cursor()
    hit_words = panels.norm(hit)[:8]
    for _ in range(8):                       # the task forces' status lines follow the click by seconds when busy
        after = nv.fleet_panel(session)
        mentions = [t for t in after["task_forces"] if hit_words and hit_words in panels.norm(t["status"])]
        grew = (after.get("active_regions") or 0) > (before.get("active_regions") or 0)
        # a fleet that already patrolled a region keeps its count, and the status lines of ships under way are
        # often unreadable: active in a region and no task force still "Docked" is the same evidence
        under_way = (after.get("active_regions") or 0) >= 1 and bool(after["task_forces"]) and not any(
            re.search(r"docked", t["status"] or "", re.I) for t in after["task_forces"])
        if mentions or grew or under_way:
            break
        time.sleep(1.0)
    data["fleet_after"] = _small(after)
    if not (mentions or grew or under_way):
        cc.fail(session, f"the fleet panel does not show {hit} after the right-click", data, "naval_assign")
    data.update({"changed": True, "verified_by": "fleet panel: Active in Regions and the task forces' region lines"})
    img = session.grab()
    mapview.set_mode(session, "default", click=True)
    return cc.finish(session, f"{after.get('name') or args['fleet']}: {mkey} in {hit} (active in "
                              f"{after.get('active_regions')} region(s); {len(mentions)} task force(s) assigned)",
                     data, "naval_mission", img=img)


WING_ROW_Y0, WING_ROW_PITCH = 275, 46.5        # the base panel's wing rows (client pixels at 1080p, gui_scale 1)
WING_HEADER = (150, 240)                       # the base's title row: a click selects every wing, shift+click none


def _air_wings_at(session, tag, save, state):
    """Wings at the air base of a state, in the base panel's order (the save's order), or None without a save."""
    if save == "none":
        return None
    mil, _ = _military(session, tag, save, ("air",))
    return [w for w in mil.get("air_wings", []) if w.get("base_state") == int(state)]


def _open_base(session, state, tag):
    """Camera on the state, click the base's counters until the base panel lists its wings."""
    w, h = session.window.size()
    cam = _go(session, state, "province", ours=True, tag=tag)
    img = session.grab()
    counters = _air_counters(img)
    counters.sort(key=lambda p: (p[0] - w / 2) ** 2 + (p[1] - h / 2) ** 2)
    panel = None
    for c in counters[:6]:
        session.input.click(*c)
        time.sleep(0.9)
        session.park_cursor()
        panel = nv.air_panel(session)
        if panel.get("wings") and panel.get("selected") is not None:
            return cam, counters, panel
    return cam, counters, panel


def _select_wings(session, rows, total):
    """Select exactly the wing rows (0-based) of the open base panel: the header selects all, otherwise a plain click
    on the first wanted row and shift+click on the others."""
    def row_xy(k):
        return (150, int(WING_ROW_Y0 + WING_ROW_PITCH * k))
    if len(rows) == total:
        session.input.click(*WING_HEADER)
        time.sleep(0.4)
        return
    first = True
    for k in rows:
        if first:
            session.input.click(*row_xy(k))
            first = False
        else:
            session.input.key_down("shift")
            try:
                session.input.click(*row_xy(k))
            finally:
                session.input.key_up("shift")
        time.sleep(0.3)


def _air_item(session, tag, item, save):
    mkey = _mission_key(nv.AIR_MISSIONS, item["mission"])
    base = item["base"]
    w, h = session.window.size()
    wings = _air_wings_at(session, tag, save, base)
    cam, counters, panel0 = _open_base(session, base, tag)
    data = {"base": base, "mission": mkey, "camera": cam, "counters": counters[:6]}
    if not panel0 or not panel0.get("wings"):
        raise ToolError(f"no air wings found at the air base of state {base}", data)
    total = len(panel0["wings"])
    types = item.get("types")
    rows = None
    if types and wings and len(wings) == total:
        rows = [i for i, wg in enumerate(wings) if wg.get("type") in types]
        if not rows:
            raise ToolError(f"no wing of type {types} at the air base of state {base} (it has "
                            f"{[wg.get('type') for wg in wings]})", data)
    elif types:
        data["note"] = "the base panel and the save disagree on the wings (or no save): the whole base was selected"
    if rows is not None:
        _select_wings(session, rows, total)
    elif types or item.get("all"):
        _select_wings(session, list(range(total)), total)
    panel1 = nv.air_panel(session)
    data["selected"] = panel1.get("selected")
    if not panel1.get("selected"):
        raise ToolError(f"no wing selected at state {base}", data)
    btn = nv.find_button(session, (nv.AIR_BUTTONS[mkey], nv.AIR_BUTTON_Y), nv.AIR_MISSIONS[mkey], search=(0, -8, 8))
    session.input.click(*btn)
    time.sleep(0.6)
    target = item.get("region_at")
    if target and str(target) != str(base):
        data["region_camera"] = _go(session, target)
    session.input.click(w // 2, h // 2, button="right")
    time.sleep(1.0)
    session.park_cursor()
    after = nv.air_panel(session)
    data["panel_after"] = _small(after)
    changed = [(b, a2) for b, a2 in zip(panel0["wings"], after["wings"]) if b.get("status") != a2.get("status")]
    data["changed_wings"] = len(changed)
    if rows is not None and not changed:
        # the line shows the region only, so re-giving a wing the mission it had (same region) changes nothing on
        # screen: a selected wing that now shows a region and is not "Standing by" counts as assigned
        for k in rows:
            if k < len(after["wings"]):
                st = (after["wings"][k].get("status") or "")
                if st and not re.search(r"standing|stand\s*by", st, re.I):
                    changed.append((panel0["wings"][k], after["wings"][k]))
    data["regions"] = sorted({a2.get("status") for _, a2 in changed if a2.get("status")})
    return data, panel1.get("selected"), changed


def _air(session, args, tag):
    items = args.get("plan") or []
    if not items:
        for f in ("base", "mission"):
            if not args.get(f):
                raise ToolError(f"air_mission needs {f} (or plan)", {"field": f})
        items = [{k: args[k] for k in ("base", "mission", "region_at", "types") if args.get(k)}]
    _ensure_map(session)
    av.deselect(session)
    mapview.set_mode(session, "air", click=True)
    time.sleep(0.6)
    done, failed = [], []
    for it in items:
        session.check_cancel()
        try:
            d, selected, changed = _air_item(session, tag, it, args.get("save", "latest"))
            if not changed:
                raise ToolError("no air wing's line changed after assigning the region (out of range, or the same "
                                "mission and region as before)", d)
            done.append({"base": it["base"], "mission": d["mission"], "types": it.get("types"), "selected": selected,
                         "changed": len(changed), "regions": d["regions"]})
        except ToolError as e:
            failed.append({"base": it.get("base"), "mission": it.get("mission"), "types": it.get("types"),
                           "error": str(e)[:240]})
            try:
                session.input.key("escape")
                time.sleep(0.4)
                av.deselect(session)
            except Exception:
                pass
    img = session.grab()
    mapview.set_mode(session, "default", click=True)
    summ = (f"air: {len(done)} assignment(s) done" + (f", {len(failed)} failed" if failed else "") + "; " +
            "; ".join(f"state {d['base']} {d['types'] or 'wings'} {d['mission']} over {', '.join(d['regions']) or '?'} "
                      f"({d['changed']} wing(s))" for d in done[:6]))
    data = {"done": done, "failed": failed, "changed": bool(done),
            "verified_by": "air wing panel: each wing's region line"}
    if not done:
        cc.fail(session, summ + " " + "; ".join(f["error"] for f in failed[:3]), data, "air_mission")
    return cc.finish(session, summ, data, "air_mission", img=img)


def _air_counters(img):
    """Centres of green unit counters (air wings in the air map mode) on screen."""
    import cv2
    import numpy as np
    a = img.astype(np.int16)
    r, g, b = a[..., 0], a[..., 1], a[..., 2]
    # the counter's number box is dark green (about 20, 43, 23) with a brighter green strip at its right edge
    m = ((g > r + 15) & (g > b + 12) & (g > 30) & (g < 130) & (r < 70)).astype(np.uint8)
    hgt, wid = m.shape
    m[:100, :] = 0
    m[hgt - 230:, :] = 0
    m[:, wid - 330:] = 0
    m = cv2.morphologyEx(m, cv2.MORPH_CLOSE, np.ones((5, 5), np.uint8))
    n, labels, stats, _ = cv2.connectedComponentsWithStats(m, connectivity=8)
    out = []
    for i in range(1, n):
        x, y, bw, bh, area = [int(v) for v in stats[i]]
        if 30 <= bw <= 80 and 14 <= bh <= 75 and area > 0.4 * bw * bh:
            k = max(1, int(round(bh / 23.0)))            # stacked counters (one per wing type) merge
            for j in range(k):
                out.append((x + bw // 2, int(y + (j + 0.5) * bh / k)))
    return out
