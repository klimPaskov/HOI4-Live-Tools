"""Campaign tools for politics (phase 3): hoi4_focus, hoi4_politics, hoi4_decisions, hoi4_diplomacy.

All of them act through the game's own interface (no console), read structure from the game and mod files
(gamedata/), and verify every action on screen; the save model (hoi4_state) confirms it after the next save.
"""

import re
import time

from ..errors import ToolError
from ..gamedata import focus as focus_data
from ..game import focusview, panels
from . import campaign_common as cc
from .registry import ToolResult, tool

TAG = {"type": "string", "pattern": "^[A-Z][A-Z0-9]{2}$", "description": "default: the player"}


# ============================================================================================== hoi4_focus
def _focus_context(session, tag, save):
    tag = cc.player_tag(session, tag)
    sv, f = {}, {}
    if save == "telemetry":
        src = {"focus": "unknown", "current_known": False}
        snap = session.telemetry.snapshot(tag) if tag else None
        obs = (snap or {}).get("focus")
        if isinstance(obs, dict):
            from ..game import timectl
            from ..state.assemble import days_between
            screen_date, _ = timectl.read_date(session)
            age = days_between(obs.get("as_of_date"), screen_date.get("hoi4"))
            if (age is not None and 0 <= age <= 1 and obs.get("tag") == tag
                    and obs.get("day") == snap.get("n")):
                f = dict(obs)
                src = {"focus": "telemetry", "as_of": obs.get("as_of_date"),
                       "generation": obs.get("generation"), "current_known": False}
            else:
                src["note"] = "focus telemetry is stale or screen date is unverified"
    else:
        sv, src = cc.save_model(session, tag, ["focus"], save)
        f = (sv or {}).get("focus") or {}
    idx = focus_data.for_session(session)
    tree = f.get("tree") or ((sv or {}).get("overview") or {}).get("focus_tree")
    if not tree and tag:
        tree = focus_data.tree_for_tag(idx, tag)
        src["tree_from"] = "country rules of the focus files (no save)"
    focuses = focus_data.tree_focuses(idx, tree) if tree else {}
    return tag, tree, focuses, f, src


@tool("hoi4_focus",
      "National focus. list: the focuses of the country's tree with status (available / current / locked / "
      "excluded / completed, from the files and the newest save), days, prerequisites, mutually exclusive "
      "focuses, script requirements and rewards; screen=true also reads the current focus on screen. details: "
      "the game's detail window for one focus (requirements met/unmet in colour, days, description, effects). "
      "select: start a focus through the tree view (search box, detail window, Start), verified on the politics "
      "panel. progress: the current focus and its progress. cancel: cancel the focus in progress. The focus is an "
      "id or its localised name; campaign selection requires the exact installed ID and unique screen verification.",
      {"properties": {
          "action": {"type": "string", "enum": ["list", "details", "select", "progress", "cancel"], "default": "list"},
          "focus": {"type": "string", "description": "focus id or localised name (details, select)"},
          "status": {"type": "string", "enum": ["available", "all", "locked", "completed", "current"],
                     "default": "available"},
          "filter": {"type": "string", "description": "regex on id, name, rewards or search filters (list)"},
          "search": {"type": "string", "description": "select/details: text to type in the tree's search box when "
                                                       "the focus name has placeholders the game fills in"},
          "if_idle": {"type": "boolean", "default": False,
                      "description": "select: do nothing (deferred=true, no error) when a focus is in progress; "
                                     "for standing orders, which retry on the next firing"},
          "replace": {"type": "boolean", "default": False,
                      "description": "select: cancel the focus in progress first"},
          "force": {"type": "boolean", "default": False,
                    "description": "test mode only: select even when the detail window shows unmet requirements"},
          "screen": {"type": "boolean", "default": False, "description": "list: also read the current focus on screen"},
          "keep_open": {"type": "boolean", "default": False, "description": "leave the focus tree open"},
          "limit": {"type": "integer", "minimum": 1, "maximum": 400, "default": 40},
          "tag": TAG, "save": cc.SAVE_ARG}},
      family="campaign", lock="self", phase=3,
      examples=[{"args": {}, "note": "available focuses of the player's tree"},
                {"args": {"action": "details", "focus": "War Plans Division"}},
                {"args": {"action": "select", "focus": "USA_continue_the_new_deal"}},
                {"args": {"action": "progress"}},
                {"args": {"action": "list", "status": "all", "filter": "industr", "limit": 20}}])
def hoi4_focus(session, args):
    a = args["action"]
    campaign = session.mode == "campaign"
    if campaign and a == "select" and args.get("force"):
        raise ToolError("campaign focus selection requires verified requirements; force is unavailable",
                        {"error": "focus_requirements_unverified"})
    tag, tree, focuses, saved, src = _focus_context(session, args.get("tag"), args["save"])
    if a == "list":
        return _focus_list(session, args, tag, tree, focuses, saved, src)
    if a == "progress":
        return _focus_progress(session, tag, tree, focuses, saved, src)
    if a == "cancel":
        with session.lock.hold("hoi4_focus cancel"):
            panels.open_panel(session, "politics")
            before = focusview.read_button(session)
            if campaign and (before.get("unread") or not before.get("text")):
                raise ToolError("current focus button is unreadable; not cancelling", {"error": "focus_state_unknown"})
            if not before.get("current"):
                raise ToolError("no focus in progress", {"button": before})
            focusview.cancel_current(session)
            after = focusview.read_button(session)
            if after.get("current") or (campaign and (after.get("unread") or not after.get("text"))):
                cc.fail(session, "the focus is still in progress after cancelling", {"after": after})
            from ..state import focustimer
            focustimer.clear(session)
            return cc.finish(session, f"cancelled {before['current']}",
                             {"action": "cancel", "before": before["current"], "after": None, "changed": True,
                              "verified_by": "politics panel focus button"}, "focus_cancel")
    if not args.get("focus"):
        raise ToolError(f"{a} needs focus (an id or a localised name)", {"field": "focus"})
    focus_names = cc.names(session, list(focuses))
    rows = focus_data.plan(focuses, saved.get("completed"), saved.get("current"), focus_names)
    row, others = focus_data.match_name(rows, args["focus"])
    if row is None and others:
        raise ToolError(f"'{args['focus']}' matches several focuses", {"candidates": [
            {"id": r["id"], "name": r["name"]} for r in others[:15]]})
    name = row["name"] if row else args["focus"]
    if campaign and a == "select" and (row is None or args["focus"] != row["id"]):
        raise ToolError("campaign focus selection requires an installed focus ID", {"error": "focus_id_required"})
    with session.lock.hold(f"hoi4_focus {a}"):
        if a == "select":
            panels.open_panel(session, "politics")
            before = focusview.read_button(session)
            if campaign and (before.get("unread") or not before.get("text")):
                raise ToolError("current focus button is unreadable; not treating it as idle",
                                {"error": "focus_state_unknown"})
            if before.get("current"):
                _q0, _rx0 = focusview.placeholder_pattern(name)
                current_id = focusview.title_focus_id(before["current"], focus_names) if campaign else None
                if campaign and current_id is None:
                    raise ToolError("current focus title does not resolve to a unique installed ID",
                                    {"error": "focus_state_unknown"})
                same = (current_id == row["id"] if campaign else
                        ((_rx0 and _rx0.search(before["current"])) or panels.similar(before["current"], name) >= 0.9))
                if same:
                    return cc.finish(session, f"{name} is already in progress",
                                     {"action": "select", "focus": row and row["id"], "before": before["current"],
                                      "after": before["current"], "changed": False,
                                      "verified_focus_id": current_id}, "focus_already")
                if args.get("if_idle"):
                    panels.close(session)
                    return ToolResult(f"deferred: {before['current']} is still in progress",
                                      {"action": "select", "focus": row and row["id"], "before": before["current"],
                                       "changed": False, "deferred": True,
                                       "verified_current_focus_id": current_id})
                if not args["replace"]:
                    raise ToolError(f"a focus is in progress ({before['current']}); pass replace=true to cancel it "
                                    "first", {"current": before["current"]})
                focusview.cancel_current(session)
                if campaign:
                    cancelled = focusview.read_button(session)
                    if cancelled.get("current") or cancelled.get("unread") or not cancelled.get("text"):
                        raise ToolError("focus cancellation is unverified; not selecting another focus",
                                        {"error": "focus_state_unknown"})
        focusview.open_tree(session)
        detail, found, tried = focusview.open_detail(session, name, search_text=args.get("search"))
        if not detail:
            panels.close(session)
            raise ToolError(f"the focus tree search found no focus titled '{name}'",
                            {"found": found, "tried": tried, "focus": row and row["id"]})
        data = {"focus": row["id"] if row else None, "name": name, "detail": detail,
                "files": {k: row[k] for k in ("status", "days", "prerequisites", "mutually_exclusive", "requires",
                                              "effects", "war_with")} if row else None,
                "tree": tree, "sources": src}
        if a == "details":
            focusview.close_detail(session)
            if not args["keep_open"]:
                panels.close(session)
            return cc.finish(session, f"{name}: {detail.get('days')} days; unmet: "
                             f"{'; '.join(detail['unmet']) or 'none on screen'}", data, "focus_details",
                             img=None)
        # select
        if campaign:
            if focusview.title_focus_id(detail.get("title"), focus_names) != row["id"]:
                raise ToolError("focus detail title does not uniquely verify the requested ID",
                                {"error": "focus_identity_unverified", "focus": row["id"]})
            if detail.get("start_enabled") is not True or detail.get("unmet") != []:
                raise ToolError("focus requirements and Start button are not verified",
                                {"error": "focus_requirements_unverified", "focus": row["id"]})
        if detail.get("start_enabled") is False and not detail["unmet"] and not args["force"]:
            raise ToolError(f"{name} cannot be started: the Start button is disabled (a requirement is not met; "
                            "hoi4_focus action=details shows the lines)", dict(data, error="requirements"))
        if detail["unmet"] and not args["force"]:
            focusview.close_detail(session)
            panels.close(session)
            raise ToolError(f"{name} cannot be started: " + "; ".join(detail["unmet"]),
                            dict(data, error="requirements"))
        focusview.start(session, detail)
        # the tree closes on Start and the politics panel shows the focus
        after = None
        for _ in range(4):
            if panels.current(session) == "politics":
                after = focusview.read_button(session)
                if after.get("current"):
                    break
            time.sleep(0.6)
        _q, _rx = focusview.placeholder_pattern(name)
        shows_it = bool(after and after.get("current") and
                        ((_rx and _rx.search(after["current"])) or panels.similar(after["current"], name) >= 0.75))
        if campaign:
            shows_it = bool(after and not after.get("unread")
                            and focusview.title_focus_id(after.get("current"), focus_names) == row["id"])
        if not shows_it:
            if panels.current(session) == "focus_tree":
                focusview.close_detail(session)
                panels.close(session)
            cc.fail(session, f"{name} was not started (the politics panel shows "
                             f"{(after or {}).get('current') or 'no focus'})", dict(data, after=after), "focus_select")
        data.update({"action": "select", "before": None, "after": after["current"], "changed": True,
                     "verified_by": "politics panel focus button"})
        if campaign:
            data["verified_focus_id"] = row["id"]
        from ..state import focustimer
        focustimer.start(session, args["focus"], name, detail.get("days"))
        return cc.finish(session, f"started {name} ({detail.get('days')} days)", data, "focus_select")


def _focus_list(session, args, tag, tree, focuses, saved, src):
    if not tree:
        raise ToolError("the country's focus tree is unknown (no save and no matching tree)", {"tag": tag})
    nm = cc.names(session, list(focuses))
    current = saved.get("current")
    screen_current = None
    if args["screen"]:
        with session.lock.hold("hoi4_focus list"):
            panels.open_panel(session, "politics")
            screen_current = focusview.read_button(session).get("current")
    if screen_current:
        current = focusview.title_focus_id(screen_current, nm)
    rows = focus_data.plan(focuses, saved.get("completed"), current, nm)
    if args["status"] != "all":
        rows = [r for r in rows if r["status"] == args["status"]]
    if args.get("filter"):
        rx = re.compile(args["filter"], re.I)
        rows = [r for r in rows if rx.search(" ".join([r["id"], r["name"] or "", r.get("effects") or "",
                                                         " ".join(r["filters"])]))]
    total = len(rows)
    rows = rows[:args["limit"]]
    data = {"tag": tag, "tree": tree, "current": current, "current_name": nm.get(current) if current else None,
            "screen_current": screen_current, "completed": len(saved.get("completed") or []),
            "focuses": rows, "shown": len(rows), "matching": total, "sources": src,
            "availability_verified": False,
            "completed_known": "completed" in saved,
            "note": "status is a prerequisite/exclusion estimate using the stated observation source; "
                    "missing completion history is unknown. Script conditions and actual availability "
                    "require the game's detail view"}
    summ = (f"{tag} {tree}: {total} {args['status']} focus(es)" +
            (f"; in progress: {data['current_name'] or screen_current}" if (current or screen_current) else
             "; no focus in progress") + ". " + "; ".join(f"{r['name']} ({r['days']} d)" for r in rows[:8]))
    return ToolResult(summ, data)


def _focus_progress(session, tag, tree, focuses, saved, src):
    with session.lock.hold("hoi4_focus progress"):
        was = panels.current(session)
        panels.open_panel(session, "politics")
        btn = focusview.read_button(session)
        img = session.grab()
        if was != "politics":
            panels.close(session)
    nm = cc.names(session, list(focuses))
    cur = None
    if btn.get("current"):
        cur = focusview.title_focus_id(btn["current"], nm)
    known = bool(btn.get("text") and not btn.get("unread") and (not btn.get("current") or cur))
    data = {"tag": tag, "screen_current": btn.get("current"), "focus": cur,
            "current_known": known, "sources": src}
    if cur:
        total = int(focuses[cur]["cost"] * focus_data.DAYS_PER_COST)
        data["total_days"] = total
        if saved.get("current") == cur and saved.get("progress_days") is not None:
            done = saved["progress_days"]
            data.update({"progress_days_at_save": done, "remaining_days_at_save": max(0, total - done),
                         "save_date": src.get("as_of")})
    summ = (f"in progress: {btn['current']}" + (f" ({data.get('total_days')} days in all)" if cur else "")
            if btn.get("current") else ("no focus in progress" if known else "current focus unknown"))
    return cc.finish(session, summ, data, "focus_progress", img=img)


# ============================================================================================== helpers
def _pp(session, img=None):
    """Political power on the top bar (None when unreadable)."""
    from ..perception import topbar
    img = session.grab() if img is None else img
    try:
        bar = topbar.read_bar(img, session.ocr, game_dir=session.cfg.game_dir)
        return (bar.get("political_power") or {}).get("value")
    except Exception:
        return None


# ============================================================================================== hoi4_politics
def _slot_pos(session, slot, index=None, empty_first=False):
    from ..game import politicsview as pv
    want = pv.resolve_slot(slot)
    pos = [p for p in pv.slot_positions(session) if p["slot"] == want]
    if not pos:
        raise ToolError(f"no politics slot '{slot}' (laws: economy, trade_laws, mobilization_laws; advisors: "
                        "political_advisor, theorist, industrial_concern, army_chief, navy_chief, air_chief, "
                        "high_command)", {"field": "slot"})
    if index is not None:
        return pos[min(index, len(pos) - 1)], None
    if empty_first and len(pos) > 1:
        for p in pos:
            st = pv.read_slot(session, p)
            if st["empty"]:
                return p, st
    return pos[0], None


@tool("hoi4_politics",
      "Laws, advisors and ideas. view: the government (laws, national spirits, ideas and advisors from the newest "
      "save; the slots of the politics panel read from their tooltips: laws, political advisors, and with "
      "rows=all the designers, theorist and military staff). options: the selection list of a slot (laws with "
      "their political power cost, advisors with traits and cost; 'blocked' when the game shows a red X or a red "
      "cost). change_law: adopt a law (id or name) through its slot. hire: hire an advisor (name or idea token) "
      "into a slot. Every change is verified on the slot's tooltip and the political power on the top bar.",
      {"properties": {
          "action": {"type": "string", "enum": ["view", "options", "change_law", "hire"], "default": "view"},
          "slot": {"type": "string", "description": "options/hire: economy, trade_laws, mobilization_laws, "
                                                    "political_advisor, theorist, industrial_concern, army_chief, ..."},
          "law": {"type": "string", "description": "change_law: law idea id or localised name"},
          "advisor": {"type": "string", "description": "hire: advisor name or idea token"},
          "rows": {"type": "string", "enum": ["government", "all", "none"], "default": "government",
                   "description": "view: which rows of slots to read on screen"},
          "force": {"type": "boolean", "default": False, "description": "click even when the choice is blocked"},
          "tag": TAG, "save": cc.SAVE_ARG}},
      family="campaign", lock="self", phase=3,
      examples=[{"args": {}, "note": "laws, spirits and advisors"},
                {"args": {"action": "options", "slot": "economy"}},
                {"args": {"action": "change_law", "law": "partial_economic_mobilisation"}},
                {"args": {"action": "hire", "advisor": "Harold L. Ickes"}}])
def hoi4_politics(session, args):
    from ..game import politicsview as pv
    from ..gamedata import politics as pol_data
    a = args["action"]
    tag = cc.player_tag(session, args.get("tag"))
    idx = pol_data.for_session(session)
    if a == "view":
        sv, src = cc.save_model(session, tag, ["politics"], args["save"])
        data = {"tag": tag, "save": (sv or {}).get("politics"), "sources": src}
        if args["rows"] != "none":
            with session.lock.hold("hoi4_politics view"):
                panels.open_panel(session, "politics")
                slots = []
                for p in pv.slot_positions(session):
                    if args["rows"] == "government" and p["row"] != "Laws & Government":
                        continue
                    slots.append(pv.read_slot(session, p))
                data["slots"] = slots
                img = session.grab()
                data["political_power"] = _pp(session, img)
            laws = {s["slot"]: s["title"] for s in slots if s["slot"] in ("economy", "trade_laws", "mobilization_laws")}
            adv = [s["title"] for s in slots if not s["empty"] and s["slot"] not in laws]
            summ = (f"{tag}: laws {', '.join(f'{k} = {v}' for k, v in laws.items())}; advisors: "
                    f"{', '.join(adv) or 'none'}; empty slots: "
                    f"{', '.join(s['slot'] for s in slots if s['empty']) or 'none'}; PP {data['political_power']}")
            return cc.finish(session, summ, data, "politics_view", img=img)
        p = (sv or {}).get("politics") or {}
        return ToolResult(f"{tag}: laws {p.get('laws')}", data)
    with session.lock.hold(f"hoi4_politics {a}"):
        panels.open_panel(session, "politics")
        if a == "options":
            if not args.get("slot"):
                raise ToolError("options needs slot", {"field": "slot"})
            pos, _ = _slot_pos(session, args["slot"])
            current = pv.read_slot(session, pos)
            pv.open_list(session, pos)
            title = pv.list_title(session)
            cards = pv.read_cards(session)
            img = session.grab()
            pv.close_list(session)
            data = {"slot": pos["slot"], "current": current["title"], "list_title": title, "choices": cards,
                    "political_power": _pp(session, img)}
            summ = f"{pos['slot']} ({current['title']}): " + "; ".join(
                f"{c['name']}" + (f" {c['cost']}" if c["cost"] else "") + (" [blocked]" if c["blocked"] else "")
                for c in cards)
            return cc.finish(session, summ, data, "politics_options", img=img)
        if a == "change_law":
            if not args.get("law"):
                raise ToolError("change_law needs law", {"field": "law"})
            q = args["law"].strip()
            cat = pol_data.law_of(idx, q)
            names = [cc.name_of(session, q, q)] if cat else [q]
            if not cat:
                allnames = cc.names(session, [i for c in idx["laws"].values() for i in c["ideas"]])
                hit = next((i for i, n in allnames.items() if panels.similar(n, q) >= 0.9), None)
                if hit:
                    cat, names = pol_data.law_of(idx, hit), [allnames[hit]]
            if not cat:
                raise ToolError(f"'{q}' is not a law (economy, trade or conscription)", {"field": "law"})
            pos, _ = _slot_pos(session, args.get("slot") or cat)
            return _choose(session, pos, names, args["force"], "change_law")
        # hire
        if not args.get("advisor"):
            raise ToolError("hire needs advisor", {"field": "advisor"})
        q = args["advisor"].strip()
        adv = idx["advisors"].get(q)
        slot = args.get("slot") or (adv or {}).get("slot") or "political_advisor"
        names = [q]
        if adv:
            n = cc.names(session, [adv["name"], adv["character"], q])
            names = [v for v in n.values() if v] or [q]
        pos, _ = _slot_pos(session, slot, empty_first=True)
        return _choose(session, pos, names, args["force"], "hire")


def _choose(session, pos, names, force, action):
    from ..game import politicsview as pv
    before = pv.read_slot(session, pos)
    session.park_cursor()
    time.sleep(0.4)
    img0 = session.grab()
    look_before = pv.slot_image(session, pos, img0)
    pp_before = _pp(session, img0)
    pv.open_list(session, pos)
    cards = pv.read_cards(session)
    best = max(((max(panels.similar(c["name"], n) for n in names), c) for c in cards), default=(0, None),
               key=lambda t: t[0])
    data = {"action": action, "slot": pos["slot"], "before": before["title"], "choices": cards,
            "political_power_before": pp_before}
    if not best[1] or best[0] < 0.8:
        pv.close_list(session)
        raise ToolError(f"{names[0]} is not in the {pos['slot']} list", data)
    card = best[1]
    data["choice"] = card
    if panels.similar(before["title"], card["name"]) >= 0.9:
        pv.close_list(session)
        return cc.finish(session, f"{card['name']} is already in place", dict(data, changed=False), action)
    if card["blocked"] and not force:
        pv.close_list(session)
        why = "a condition is not met (red X)" if card["condition_unmet"] else "not enough political power"
        raise ToolError(f"{card['name']} cannot be chosen now: {why}", data)
    session.input.click(*card["center"])
    time.sleep(1.0)
    session.park_cursor()
    data["dialog"] = panels.answer_dialog(session, True, attempts=5)
    time.sleep(0.4)
    if pv.list_title(session):
        pv.close_list(session)
    session.park_cursor()
    time.sleep(0.4)
    img1 = session.grab()
    from .common import change_ratio
    look = change_ratio(look_before, pv.slot_image(session, pos, img1))
    pp_after = _pp(session, img1)
    after = pv.read_slot(session, pos)
    data.update({"after": after["title"], "political_power_after": pp_after, "slot_image_changed": round(look, 3)})
    # "Early Mobilization" and "Partial Mobilization" read alike: the slot counts as changed only when it is
    # clearly closer to the new choice than to what it showed before
    by_title = (panels.similar(after["title"], card["name"]) >= 0.8 and
                panels.similar(after["title"], card["name"]) > panels.similar(after["title"], before["title"]))
    paid = (pp_before is not None and pp_after is not None and card.get("cost") and
            pp_before - pp_after >= 0.8 * card["cost"])
    if not by_title and not (look > 0.15 and (paid or not card.get("cost"))):
        cc.fail(session, f"the {pos['slot']} slot still shows {after['title']!r} after choosing {card['name']}", data,
                action)
    data.update({"changed": True, "verified_by": "slot tooltip" if by_title else
                 "slot picture changed and political power was paid"})
    return cc.finish(session, f"{pos['slot']}: {before['title']} -> {after['title']} (PP {pp_before} -> {pp_after})",
                     data, action)


# ============================================================================================== hoi4_decisions
@tool("hoi4_decisions",
      "Decisions. list: the decision panel's categories and decisions as the game shows them (cost, whether it can "
      "be taken: green row = conditions met, red cost = not enough political power, timers of decisions in "
      "progress), matched to their ids and file data (conditions, effects, cooldowns). details: the tooltip of one "
      "decision. take: take a decision (id or localised name), verified by the row and the political power.",
      {"properties": {
          "action": {"type": "string", "enum": ["list", "details", "take"], "default": "list"},
          "decision": {"type": "string", "description": "details/take: decision id or localised name"},
          "category": {"type": "string", "description": "list: only this category (name or regex)"},
          "available": {"type": "boolean", "default": False, "description": "list: only decisions that can be taken"},
          "force": {"type": "boolean", "default": False,
                    "description": "take: click even when the row looks unavailable"}}},
      family="campaign", lock="acting", phase=3,
      examples=[{"args": {}}, {"args": {"available": True}},
                {"args": {"action": "details", "decision": "Small Lobbying Effort"}},
                {"args": {"action": "take", "decision": "USA_small_lobby_effort"}}])
def hoi4_decisions(session, args):
    from ..game import decisionsview as dv
    from ..gamedata import decisions as dec_data
    from ..perception import tooltip
    a = args["action"]
    idx = dec_data.for_session(session)
    catnames = list(cc.names(session, list(idx["categories"])).values())
    panels.open_panel(session, "decisions", settle=1.8)
    if a == "list":
        pp = _pp(session)
        rows = dv.read_all(session, categories=catnames)
        dnames = cc.names(session, list(idx["decisions"]))
        by_name = {}
        for did, n in dnames.items():
            by_name.setdefault(panels.norm(n), []).append(did)
        out = []
        for r in rows:
            if r["kind"] != "decision":
                continue
            ids = by_name.get(panels.norm(r["name"]))
            if not ids:
                cand = [(panels.similar(r["name"], n), d) for d, n in dnames.items()
                        if abs(len(n) - len(r["name"])) < 6]
                best = max(cand, default=(0, None))
                ids = [best[1]] if best[0] >= 0.88 else []
            d = idx["decisions"].get(ids[0]) if ids else None
            cost = r.get("cost")
            if cost is None and d and d.get("cost") is not None and r.get("days") is None:
                cost = d["cost"]                       # the screen showed no number: the files' cost
            affordable = r.get("cost_colour") != "red" and (pp is None or cost is None or cost <= pp + 0.5)
            # the localised name of a matched id (the row's OCR may read "Smalt Lobbying Effort")
            row = {"name": (dnames.get(ids[0]) if ids else None) or r["name"], "screen_name": r["name"],
                   "id": ids[0] if ids else None, "category": r.get("category"),
                   "cost": cost, "can_take": r.get("tint") == "green" and affordable and r.get("days") is None,
                   "conditions_met": r.get("tint") == "green", "affordable": affordable,
                   "days_left": r.get("days")}
            if d:
                row.update({"once": d["once"], "cooldown_days": d["days_re_enable"], "timed_days": d["days_remove"],
                            "effect": d["effect"], "requires": d["available"]})
            out.append(row)
        if args.get("category"):
            rx = re.compile(args["category"], re.I)
            out = [r for r in out if r["category"] and rx.search(r["category"])]
        if args["available"]:
            out = [r for r in out if r["can_take"]]
        data = {"decisions": out, "categories": [r["name"] for r in rows if r["kind"] == "category"],
                "count": len(out), "political_power": pp}
        summ = f"{len(out)} decision(s) shown" + "; ".join(
            [""] + [f"{r['name']}" + (f" ({r['cost']} PP)" if r["cost"] else "") +
                    (" [can take]" if r["can_take"] else "") for r in out[:15]])
        return cc.finish(session, summ, data, "decisions_list")
    if not args.get("decision"):
        raise ToolError(f"{a} needs decision", {"field": "decision"})
    q = args["decision"].strip()
    name = cc.name_of(session, q, q) if q in idx["decisions"] else q
    row = dv.find_row(session, name, categories=catnames)
    if not row:
        raise ToolError(f"no decision row named '{name}' in the panel (not visible for this country now?)",
                        {"decision": q})
    if a == "details":
        d, crop, img = tooltip.read(session, 200, row["y"], wait=0.9)
        data = {"row": row, "tooltip": d, "files": idx["decisions"].get(q)}
        res = ToolResult(f"{row['name']}: " + " | ".join(d.get("lines", [])[:12]), data)
        from .common import attach
        return attach(res, session, crop, "decision_details", max_width=0)
    pp_before = _pp(session)
    data = {"action": "take", "decision": q, "name": name, "row_before": row, "political_power_before": pp_before}
    did = q if q in idx["decisions"] else None
    cost = row.get("cost") if row.get("cost") is not None else (idx["decisions"].get(did) or {}).get("cost")
    short = pp_before is not None and cost is not None and cost > pp_before + 0.5
    if short:
        row["cost_colour"] = "red"
    if (row.get("tint") != "green" or row.get("cost_colour") == "red" or row.get("days") is not None) \
            and not args["force"]:
        d, _, _ = tooltip.read(session, 200, row["y"], wait=0.9)
        data["tooltip"] = d.get("lines")
        why = ("not enough political power" if row.get("cost_colour") == "red" else
               f"in progress, {row['days']} days left" if row.get("days") is not None else "conditions not met")
        raise ToolError(f"{row['name']} cannot be taken now ({why})", data)
    data["dialog"] = dv.take(session, row)
    time.sleep(0.8)
    after = dv.find_row(session, name, categories=catnames)
    pp_after = _pp(session)
    data.update({"row_after": after, "political_power_after": pp_after})
    changed = (after is None or after.get("tint") != row.get("tint") or after.get("days") != row.get("days") or
               (pp_before is not None and pp_after is not None and pp_after < pp_before - 0.5))
    if not changed:
        cc.fail(session, f"{row['name']} looks unchanged after clicking it", data, "decision_take")
    data.update({"changed": True, "verified_by": "decision row and political power"})
    return cc.finish(session, f"took {name}" + (f" (PP {pp_before} -> {pp_after})" if pp_before is not None else ""),
                     data, "decision_take")


# ============================================================================================== hoi4_diplomacy
def _country_capital(session, tag):
    """Capital state id of a country: the newest save, else its history file."""
    import os
    from ..gamedata import scriptdb
    from ..state import assemble, queries
    try:
        path, _ = assemble.find_save(session, None, "latest")
        sf = queries.open_save(path)
        try:
            cap = sf.keys(f"countries/{tag}").get("scalars", {}).get("capital")
        finally:
            queries.close_all()
        if cap:
            return int(cap)
    except (ToolError, KeyError, ValueError):
        pass
    for root in reversed(scriptdb.roots(session.cfg.game_dir, scriptdb.mods_for(session))):
        d = os.path.join(root["path"], "history", "countries")
        if not os.path.isdir(d):
            continue
        for n in os.listdir(d):
            if n.upper().startswith(tag + " ") or n.upper().startswith(tag + "-"):
                m = re.search(rb"^\s*capital\s*=\s*(\d+)", scriptdb.read_bytes(os.path.join(d, n)), re.M)
                if m:
                    return int(m.group(1))
    return None


ACTION_ALIASES = {"improve_relations": "Improve relations", "guarantee": "Guarantee independence",
                  "justify": "Justify War goal", "justify_war_goal": "Justify War goal",
                  "invite_to_faction": "Invite to Faction", "faction_invite": "Invite to Faction",
                  "volunteers": "Send Volunteers", "send_volunteers": "Send Volunteers",
                  "lend_lease": "Start Lend lease", "military_access": "Ask for military access",
                  "non_aggression_pact": "Non-aggression pact", "attache": "Send attache",
                  "embargo": "Issue Trade Embargo", "license": "Negotiate Licenses",
                  "declare_war": "Declare war"}


def _select_war_goal_states(session, dip, wanted):
    """Click states in the right-hand list of the war goal window (names as shown, or 'all' = the first five)."""
    res = dip.read_window(session)
    w, _ = session.window.size()
    cands = [l for l in res.lines if l.center[0] > w * 0.5 and not re.search(r"^\W*(send|cancel|justify)\W*$", l.text, re.I)
             and 500 <= l.center[1] <= 730 and len(l.text) > 2 and not re.search(r"%|political|war goal|select", l.text, re.I)]
    picks = cands[:5] if [x.lower() for x in wanted] == ["all"] else         [l for l in cands if any(panels.similar(l.text, name) >= 0.8 for name in wanted)]
    if not picks:
        raise ToolError("none of the states is listed in the war goal window", {"seen": [l.text for l in cands],
                                                                                 "wanted": wanted})
    for l in picks:
        session.input.click(*l.center)
        time.sleep(0.4)
    return [l.text for l in picks]


@tool("hoi4_diplomacy",
      "Diplomacy with a country (tag), and trade. view: open the country's diplomacy view (camera to its capital, "
      "state view, owner flag) and list its actions with cost and a mark (ok / blocked). act: perform an action by "
      "name or alias (improve_relations, guarantee, justify, invite_to_faction, volunteers, lend_lease, "
      "military_access, non_aggression_pact, attache, embargo, license; declare_war only with confirm_war=true): "
      "blocked actions return their tooltip; a follow-up window is answered with `option` (a war goal type ...) "
      "or its confirm button; the action list, header and political power are compared before and after. trade: "
      "the trade panel (civilian factories for trade, resources, exporting countries).",
      {"properties": {
          "action": {"type": "string", "enum": ["view", "act", "trade"], "default": "view"},
          "country": {"type": "string", "pattern": "^[A-Z][A-Z0-9]{2}$", "description": "target country tag"},
          "do": {"type": "string", "description": "act: action name or alias"},
          "option": {"type": "string", "description": "act: text of the choice in a follow-up window"},
          "states": {"type": "array", "items": {"type": "string"},
                     "description": "justify: the states of the war goal window to select (names as shown, or "
                                    "['all'] for the first five)"},
          "via": {"type": "string", "description": "a state of that country as the game displays it (its name or "
                                                    "a sibling's), used when the capital's name is not found in the "
                                                    "Find view; a name that worked is remembered per country"},
          "confirm_war": {"type": "boolean", "default": False}}},
      family="campaign", lock="acting", phase=3,
      examples=[{"args": {"action": "view", "country": "CAN"}},
                {"args": {"action": "act", "country": "MEX", "do": "improve_relations"}},
                {"args": {"action": "act", "country": "CUB", "do": "justify", "option": "Puppet"}},
                {"args": {"action": "trade"}}])
def hoi4_diplomacy(session, args):
    from ..game import diplomacyview as dip
    from ..perception import tooltip
    a = args["action"]
    if a == "trade":
        panels.open_panel(session, "trade", settle=1.5)
        t = dip.read_trade(session)
        snap = session.telemetry.snapshot() if session.telemetry.player() else None
        if snap:
            from ..perception import topbar
            from ..state import assemble
            d = topbar.read_date(session.grab(), session.ocr)
            gap = assemble.days_between(d.get("hoi4"), snap.get("date")) if d.get("hoi4") else None
            if gap is not None and gap > 1.05:
                t["resources_note"] = "telemetry is ahead of the screen (a save was loaded): no resource numbers yet"
                snap = None
        res = ((snap or {}).get("res") or {}).get("resources") or (snap or {}).get("res")
        if isinstance(res, dict) and res:
            res = {k: v for k, v in res.items() if isinstance(v, dict)}
            t["resources"] = res
            t["resources_source"] = f"telemetry ({snap.get('date')})"
            t["deficits"] = [k for k, v in res.items() if (v.get("surplus") or v.get("s") or 0) < 0]
        summ = (f"trade: {t.get('civilian_total')} civilian factories, {t.get('civilian_used')} used, "
                f"{t.get('civilian_available')} available; {len(t['market'])} market row(s)" +
                (f"; deficits: {', '.join(t['deficits'])}" if t.get("deficits") else ""))
        return cc.finish(session, summ, t, "trade")
    tag = args.get("country")
    if not tag:
        raise ToolError(f"{a} needs country (a tag)", {"field": "country"})
    cap = _country_capital(session, tag)
    remembered = getattr(session, "country_via", {})
    candidates = []
    if args.get("via"):
        candidates.append(args["via"])
    if tag in remembered:
        candidates.append(remembered[tag])
    if cap:
        candidates.append(str(cap))                      # by id: the Find view lists the state with that id first
        loc = cc.name_of(session, f"STATE_{cap}", None)
        if loc:
            candidates.append(loc)
    candidates = list(dict.fromkeys(candidates))
    if not candidates:
        raise ToolError(f"capital of {tag} unknown (pass via: a state of that country)", {"country": tag})
    header, state, last_err = None, None, None
    for state in candidates:
        try:
            header = dip.open_for_state(session, state)
            break
        except ToolError as e:
            last_err = e
    if header is None:
        raise ToolError(f"{last_err} (tried {', '.join(candidates)}; pass via=<a state of {tag} as displayed in "
                        "the game>)", getattr(last_err, "details", None))
    if not hasattr(session, "country_via"):
        session.country_via = {}
    session.country_via[tag] = state
    names = [n for n in cc.names(session, [tag, f"{tag}_DEF"]).values() if n]
    data = {"country": tag, "capital": cap, "capital_name": state, "header": header}
    if names and header.get("name") and max(panels.similar(header["name"], n) for n in names) < 0.5 and \
            not any(panels.norm(n) in panels.norm(header["name"]) for n in names):
        data["warning"] = f"the view shows {header['name']}, expected {names[0]} (its capital may be occupied)"
    if a == "view":
        acts = dip.all_actions(session)
        data["actions"] = acts
        summ = f"{header.get('name')} ({header.get('status')}): " + "; ".join(
            f"{x['name']}" + (f" {x['cost']}" if x["cost"] else "") + (f" [{x['mark']}]" if x["mark"] else "")
            for x in acts)
        return cc.finish(session, summ, data, "diplomacy_view")
    want = args.get("do")
    if not want:
        raise ToolError("act needs do (an action)", {"field": "do"})
    label = ACTION_ALIASES.get(want.strip().lower().replace(" ", "_"), want)
    if re.search(r"declare\s*war", label, re.I) and not args["confirm_war"]:
        raise ToolError("declaring war needs confirm_war=true", {"field": "confirm_war"})
    row = dip.find_action(session, label)
    if not row:
        raise ToolError(f"no action '{label}' in {tag}'s diplomacy view", {"actions": dip.all_actions(session)})
    data["row_before"] = row
    if row["mark"] == "blocked":
        d, _, _ = tooltip.read(session, dip.ACTIONS_REGION[0] + 80, row["y"], wait=0.9)
        data["tooltip"] = d.get("lines")
        raise ToolError(f"{row['name']} is not possible now: " + " | ".join(d.get("lines", [])[1:6]), data)
    before = dip.read_actions(session)
    pp_before = _pp(session)
    dip.click_action(session, row)
    follow = None
    if args.get("option"):
        res = dip.read_window(session)
        hit = next((l for l in res.lines if panels.similar(l.text, args["option"]) >= 0.8), None)
        if not hit:
            raise ToolError(f"no choice '{args['option']}' in the window", {"window": [l.text for l in res.lines]})
        session.input.click(*hit.center)
        time.sleep(1.0)
        follow = hit.text
    if args.get("states"):
        follow = (follow or "") + " " + ", ".join(_select_war_goal_states(session, dip, args["states"]))
    dialog = panels.answer_dialog(session, True, attempts=2)
    if not dialog:
        res = dip.read_window(session)
        btn = next((l for l in res.lines if re.search(r"^\W*(send|confirm|ok|justify|accept|start)\W*$", l.text, re.I)),
                   None)
        if btn:
            session.input.click(*btn.center)
            time.sleep(1.0)
            dialog = {"button": btn.text}
    session.park_cursor()
    time.sleep(0.6)
    after_header = dip.read_header(session)
    after = dip.read_actions(session)
    pp_after = _pp(session)
    changed = ([(r["name"], r["mark"]) for r in after] != [(r["name"], r["mark"]) for r in before] or
               after_header.get("values") != header.get("values") or
               (pp_before is not None and pp_after is not None and abs(pp_after - pp_before) >= 1))
    data.update({"option": follow, "dialog": dialog, "header_after": after_header, "actions_after": after,
                 "political_power_before": pp_before, "political_power_after": pp_after, "changed": changed,
                 "verified_by": "action list, header and political power"})
    if not changed:
        cc.fail(session, f"nothing changed after {row['name']}", data, "diplomacy_act")
    return cc.finish(session, f"{row['name']} for {tag}" + (f" ({follow})" if follow else "") +
                     (f"; PP {pp_before} -> {pp_after}" if pp_before is not None else ""), data, "diplomacy_act")


# ============================================================================================== hoi4_peace
@tool("hoi4_peace",
      "The peace conference that opens when a war is won (the clock stops with peace_conference). status: score, "
      "turn, the loser and its states with their costs. take: select demands: all=true (Select All) or states=[...] "
      "(names as listed); submit=true then submits them (final for the turn). submit: submit what is selected. "
      "exit: Confirm and Exit (OK). pass: pass the turn. Use it as soon as the clock reports peace_conference; "
      "taking every state of a defeated country annexes it.",
      {"properties": {
          "action": {"type": "string", "enum": ["status", "take", "submit", "exit", "pass"], "default": "status"},
          "all": {"type": "boolean", "default": False, "description": "take: Select All"},
          "states": {"type": "array", "items": {"type": "string"}, "description": "take: states by name"},
          "submit": {"type": "boolean", "default": False, "description": "take: submit right away"},
          "exit": {"type": "boolean", "default": False, "description": "take/submit: leave the conference after"}}},
      family="campaign", lock="acting", phase=7,
      examples=[{"args": {"action": "status"}},
                {"args": {"action": "take", "all": True, "submit": True, "exit": True},
                 "note": "annex the defeated country completely and leave"},
                {"args": {"action": "take", "states": ["Tirol", "Vorarlberg"], "submit": True}}])
def hoi4_peace(session, args):
    from ..game import peaceview as pv
    a = args["action"]
    st = pv.read(session)
    if not st.get("open"):
        raise ToolError("the peace conference is not open (it opens when a war is won; hoi4_clock / hoi4_advance "
                        "stop with peace_conference)", {"screen": panels.current(session)})
    if a == "status":
        return cc.finish(session, f"peace conference: score {st.get('score')}, turn {st.get('turn')}, "
                         f"{len(st.get('rows') or [])} state(s) listed" + (f" ({st.get('loser')})" if st.get("loser") else ""),
                         st, "peace_status")
    data = {"action": a, "before": {k: st.get(k) for k in ("score", "turn", "loser")}}
    if a == "take":
        if not args["all"] and not args.get("states"):
            raise ToolError("take needs all=true or states=[...]", {"field": "states"})
        clicked, st2 = pv.take(session, args.get("states") or [], args["all"])
        data.update({"selected": clicked, "after": {k: st2.get(k) for k in ("score", "turn")},
                     "current_demands": st2.get("current_demands"), "submit_label": st2.get("submit_label"),
                     "changed": True, "verified_by": "current demands list and the submit button"})
        if not re.search(r"submit|\d{2,}", st2.get("submit_label") or "", re.I):
            cc.fail(session, "nothing was selected (the submit button shows no cost)", data, "peace_take")
        if args["submit"]:
            st3 = pv.submit(session)
            data["submitted"] = {"turn": st3.get("turn"), "score": st3.get("score")}
    elif a == "submit":
        st3 = pv.submit(session)
        data.update({"submitted": {"turn": st3.get("turn"), "score": st3.get("score")}, "changed": True})
    elif a == "pass":
        session.input.click(*pv._scaled(session, 1756, 1038))
        time.sleep(1.5)
        data["changed"] = True
    if a == "exit" or args.get("exit"):
        closed, dialog = pv.leave(session)
        data.update({"closed": closed, "changed": True})
        if not closed:
            cc.fail(session, "the conference is still open after Confirm and Exit", data, "peace_exit")
    return cc.finish(session, f"peace conference {a}: " + ", ".join(f"{k}={v}" for k, v in data.items()
                                                                   if k in ("selected", "submitted", "closed")),
                     data, "peace")
