"""Raw read bodies -> one normalised snapshot.

`build(raw, extras)` takes the bodies of the READS (key -> body, or
{"error": ...}), the per-country state reads and the executor files, and returns
one compact document. Every section says what it could not read (`missing`)
instead of guessing; numbers keep the game's units.
"""
from __future__ import annotations

import datetime as _dt
from collections import Counter, defaultdict

SCHEMA = 1


def g(d, *path, default=None):
    """Nested get that tolerates missing keys, lists and None."""
    cur = d
    for p in path:
        if isinstance(cur, dict):
            cur = cur.get(p)
        elif isinstance(cur, list) and isinstance(p, int) and -len(cur) <= p < len(cur):
            cur = cur[p]
        else:
            return default
        if cur is None:
            return default
    return cur


def deep(d, key, default=None):
    """First value of `key` anywhere in a nested body (breadth first)."""
    todo = [d]
    while todo:
        cur = todo.pop(0)
        if isinstance(cur, dict):
            if key in cur and cur[key] is not None:
                return cur[key]
            todo.extend(v for v in cur.values() if isinstance(v, (dict, list)))
        elif isinstance(cur, list):
            todo.extend(v for v in cur if isinstance(v, (dict, list)))
    return default


def num(v, default=None):
    if isinstance(v, bool):
        return default
    if isinstance(v, (int, float)):
        return v
    if isinstance(v, str):
        try:
            return float(v)
        except ValueError:
            return default
    return default


def ok(body):
    return isinstance(body, dict) and "error" not in body


def parse_date(text):
    """'1940.07.26.08' -> datetime (hour kept); None when unreadable."""
    if not text:
        return None
    parts = str(text).split(".")
    try:
        y, m, d = int(parts[0]), int(parts[1]), int(parts[2])
        h = int(parts[3]) if len(parts) > 3 and parts[3] else 0
        return _dt.datetime(y, m, d, min(h, 23))
    except (ValueError, IndexError):
        return None


def days_between(a, b):
    da, db = parse_date(a), parse_date(b)
    if not da or not db:
        return None
    return (db - da).total_seconds() / 86400.0


# --- sections -----------------------------------------------------------------


def _meta(raw):
    ov = raw.get("overview") or {}
    date = g(ov, "date", "text") or g(raw, "observe", "now", "date")
    return {
        "date": date,
        "player": ov.get("player"),
        "country": ov.get("country"),
        "speed": ov.get("speed"),
        "paused": ov.get("paused"),
        "difficulty": ov.get("difficulty"),
        "mode": ov.get("default_mode"),
    }


def _politics(raw):
    cs = raw.get("country_status") or {}
    f = cs.get("fields") or {}
    ec = raw.get("economy") or {}
    pp = num(g(ec, "political_power", "raw_signed"))
    pp = pp / num(g(ec, "political_power", "raw_units_per_point"), 100000) if pp is not None else None
    if pp is None:
        pp = num(g(f, "political_power", "value"))
    ed = raw.get("economy_detail") or {}
    laws = {}
    found = deep(ed, "laws")
    if isinstance(found, dict):
        for k, v in found.items():
            laws[k] = (v.get("id") or v.get("name")) if isinstance(v, dict) else v
    out = {
        "political_power": round(pp, 2) if pp is not None else None,
        "stability": num(g(f, "stability", "value")),
        "war_support": num(g(f, "war_support", "value")),
        "manpower": num(g(f, "manpower", "value")),
        "command_power": num(g(f, "command_power", "value")),
        "world_tension": num(g(f, "world_tension", "value")),
        "laws": laws,
        "missing": ["advisors and empty seats", "decisions", "political power income"],
    }
    if not ok(cs):
        out["missing"].append("country_status unreadable")
    return out


def _economy(raw):
    ec = raw.get("economy") or {}
    ed = raw.get("economy_detail") or {}
    res = raw.get("resources") or {}
    ftc = ec.get("factory_trigger_count") or {}
    resources = {}
    for name, r in (res.get("resources") or {}).items():
        if not isinstance(r, dict):
            continue
        resources[name] = {
            "produced": num(r.get("produced")),
            "needed": num(r.get("needed_by_production")),
            "balance": num(r.get("balance")),
            "imported": num(r.get("imported")),
            "exported": num(r.get("exported")),
            "short": bool(g(r, "shortage", "in_deficit") or (num(r.get("balance"), 0) < 0)),
        }
    fuel = res.get("fuel") or {}
    return {
        "factories": {"civilian": ftc.get("civilian"), "military": ftc.get("military"),
                      "naval": ftc.get("naval")},
        "civilian_on_construction": ec.get("civilian_factories_on_construction"),
        "civilian_idle": ec.get("civilian_factories_idle"),
        "civilian_for_projects": ec.get("civilian_factories_available_for_projects"),
        "consumer_goods": num(deep(ed, "taken_by_consumer_goods_raw")),
        "trade_civilian": num(deep(ed, "trade_civilian_factories")),
        "resources": resources,
        "fuel": {"stock": num(fuel.get("stock")), "capacity": num(fuel.get("capacity")),
                 "ratio": num(fuel.get("ratio"))},
    }


def _production(raw):
    pl = raw.get("production_lines") or {}
    lines = []
    for ln in pl.get("lines") or []:
        fc = ln.get("factory_counters") or {}
        out = ln.get("output") or {}
        lines.append({
            "equipment": ln.get("equipment_id"),
            "kind": ln.get("line_kind"),
            "factories": num(fc.get("active_factories_raw_signed"), 0),
            "requested": num(fc.get("requested_factories_raw_signed")),
            "efficiency": num(out.get("efficiency")),
            "efficiency_cap": num(out.get("efficiency_cap")),
            "units_per_day": num(out.get("units_per_day")),
        })
    stock = []
    sp = raw.get("stockpile") or {}
    seen = set()
    for e in (sp.get("equipment") or []) + ([sp["trucks"]] if isinstance(sp.get("trucks"), dict) else []):
        if e.get("archetype") in seen:
            continue
        seen.add(e.get("archetype"))
        stock.append({
            "archetype": e.get("archetype"),
            "stockpile": num(e.get("stockpile")),
            "net_per_day": num(e.get("net_per_day")),
            "produced_per_day": num(e.get("produced_per_day")),
            "fielded_short": num(e.get("fielded_short")),
            "days_of_cover": num(e.get("days_of_cover")),
            "depleting": bool(e.get("depleting")),
            "lines": e.get("production_lines"),
        })
    shortages = []
    for row in (raw.get("production_shortages") or {}).get("lines") or []:
        short = [s for s in row.get("short_of") or [] if s]
        if short:
            shortages.append({"equipment": row.get("equipment"), "factories": row.get("factories"),
                              "short": short})
    mil = sum(x["factories"] for x in lines if x["kind"] != "naval")
    nav = sum(x["factories"] for x in lines if x["kind"] == "naval")
    return {"lines": lines, "military_assigned": mil, "naval_assigned": nav,
            "stockpile": stock, "resource_shortages": shortages}


def _construction(raw):
    cq = raw.get("construction_queue") or {}
    rows = []
    for ln in cq.get("lines") or []:
        frac = ln.get("produced_over_cost") or {}
        try:
            progress = round(int(frac["numerator"]) / int(frac["denominator"]), 3)
        except (KeyError, TypeError, ValueError, ZeroDivisionError):
            progress = None
        loc = ln.get("location") or {}
        rows.append({
            "kind": ln.get("line_kind"),
            "building": g(ln, "building", "building_id") or ln.get("line_kind"),
            "state": loc.get("state_id"),
            "province": loc.get("province_id"),
            "factories": num(ln.get("active_factories_raw_signed"), 0),
            "amount": num(ln.get("amount_raw_signed")),
            "progress": progress,
        })
    ec = raw.get("economy") or {}
    queued = ec.get("construction_projects_queued", cq.get("total_count"))
    return {"queue": rows, "queued": queued,
            "factories_working": sum(r["factories"] for r in rows),
            "max_factories_per_project": 15}


def _research(raw):
    ov = raw.get("overview") or {}
    slots = []
    for s in ov.get("research") or []:
        slots.append({"slot": s.get("slot"), "researching": s.get("researching"),
                      "days_remaining": s.get("days_remaining"), "progress": s.get("progress")})
    rx = raw.get("research_extras") or {}
    bonuses = []
    for b in rx.get("tech_bonuses") or []:
        if isinstance(b, dict) and not b.get("exhausted"):
            bonuses.append({"name": b.get("name"), "uses_left": b.get("uses_left"),
                            "bonus": b.get("bonus"), "categories": b.get("categories")})
    idle = g(raw, "observe", "now", "idle_research_slots")
    if idle is None and slots:
        idle = sum(1 for s in slots if not s["researching"])
    return {"slots": slots, "idle_slots": idle, "bonuses": bonuses,
            "focus": ov.get("focus") or g(raw, "observe", "now", "focus")}


def _doctrines(raw):
    d = raw.get("doctrines") or {}
    xp = {k: num(v.get("value")) for k, v in (d.get("experience") or {}).items() if isinstance(v, dict)}
    folders = []
    for f in d.get("folders") or []:
        tracks = []
        for t in f.get("tracks") or []:
            sub = t.get("subdoctrine") if isinstance(t.get("subdoctrine"), dict) else {}
            tracks.append({"track": t.get("track"), "subdoctrine": sub.get("id"),
                           "xp_cost": sub.get("xp_cost"), "mastery": num(t.get("mastery"))})
        gd = f.get("grand_doctrine") if isinstance(f.get("grand_doctrine"), dict) else {}
        folders.append({"folder": f.get("folder"), "xp_type": f.get("xp_type"),
                        "experience": num(f.get("experience")),
                        "grand_doctrine": gd.get("id"), "grand_xp_cost": gd.get("xp_cost"),
                        "open_tracks": [t["track"] for t in tracks if not t["subdoctrine"]],
                        "tracks": tracks})
    return {"experience": xp, "folders": folders}


def _leaders(raw):
    c = raw.get("commanders") or {}
    out = []
    for x in c.get("commanders") or []:
        out.append({"name": x.get("name"), "rank": x.get("rank"),
                    "assigned_to": g(x, "assigned_to", "name"),
                    "assigned_kind": g(x, "assigned_to", "kind"),
                    "skill": x.get("skill"), "planning": x.get("planning"),
                    "logistics": x.get("logistics"), "traits": x.get("traits") or [],
                    "available": x.get("available"),
                    "recruited": x.get("listed_in_panel_or_assigned")})
    unled = []
    for t in c.get("without_commander") or []:
        unled.append(t.get("name") if isinstance(t, dict) else t)
    return {"commanders": out, "without_commander": unled,
            "list_open": c.get("commander_list_open"),
            "missing": ["rank of leaders who lead nothing", "skill levels gained", "open trait slots"]}


def _order(o):
    ln = o.get("line") or {}
    return {
        "kind": o.get("kind"), "id": o.get("id"), "divisions": o.get("divisions"),
        "executing": bool(o.get("executing")), "states": ln.get("states") or [],
        "provinces": ln.get("provinces"), "most_in_one_province": ln.get("most_in_one_province"),
        "offensives": [{"id": f.get("id"), "target_states": g(f, "target", "states", default=[]),
                        "path_states": f.get("path_states") or [], "divisions": f.get("divisions"),
                        "executing": bool(f.get("executing"))} for f in o.get("offensives") or []],
    }


def _land(raw):
    a = raw.get("armies") or {}
    sup = raw.get("supply") or {}
    by_army = {x.get("army"): x for x in sup.get("armies") or []}
    armies = []
    for x in a.get("armies") or []:
        s = by_army.get(x.get("army")) or {}
        orders = [_order(o) for o in x.get("plan") or []]
        armies.append({
            "army": x.get("army"), "group": x.get("army_group"), "commander": x.get("commander"),
            "divisions": x.get("division_count"), "states": x.get("states") or {},
            "planning": x.get("planning") or {}, "orders": orders,
            "executing": bool(x.get("plan_executing")),
            "battle_plan_orders": x.get("battle_plan_orders"),
            "motorization": x.get("motorization"),
            "supply_mean": s.get("supply_percent_mean"), "supply_lowest": s.get("supply_percent_lowest"),
            "organisation_mean": s.get("organisation_mean"), "strength_mean": s.get("hp_percent_mean"),
            "equipment_fill_min": s.get("equipment_fill_min"),
        })
    groups = [{"group": x.get("army_group"), "field_marshal": x.get("field_marshal"),
               "orders": [_order(o) for o in x.get("plan") or []],
               "armies": [r["army"] for r in armies if r["group"] == x.get("army_group")]}
              for x in a.get("army_groups") or []]
    divisions = []
    for d in sup.get("divisions") or []:
        divisions.append({
            "id": d.get("id"), "army": d.get("army"), "template": d.get("template"),
            "province": d.get("province_id"), "state": d.get("state_id"),
            "strength": num(d.get("hp_percent")), "organisation": num(d.get("organisation")),
            "supply": num(d.get("supply_percent")), "equipment_fill": num(d.get("equipment_fill_min")),
            "short": d.get("short_equipment") or [],
        })
    return {"armies": armies, "groups": groups, "divisions": divisions,
            "division_count": len(divisions) or sum(r["divisions"] or 0 for r in armies),
            "unassigned": len(a.get("unassigned") or []),
            "unassigned_by_state": {str(u.get("state_id")): u.get("divisions")
                                    for u in a.get("unassigned_by_state") or []},
            "missing": ["per-division planning, entrenchment, experience", "divisions in combat"]}


def _stacking(raw):
    sup = raw.get("supply") or {}
    provinces = []
    for p in sup.get("provinces") or []:
        provinces.append({
            "province": p.get("province_id"), "state": p.get("state_id"),
            "divisions": p.get("divisions") or len(p.get("division_ids") or []),
            "supply": num(p.get("supply")), "demand": num(p.get("demand")),
            "headroom": num(p.get("headroom")), "railway": p.get("railway_level"),
            "hub": bool(p.get("supply_hub")),
        })
    provinces.sort(key=lambda r: -(r["divisions"] or 0))
    states = []
    for s in sup.get("states") or []:
        states.append({"state": s.get("state_id"), "divisions": s.get("divisions"),
                       "local_supply": num(s.get("local_supply")),
                       "infrastructure": s.get("infrastructure"),
                       "railway_max": s.get("railway_level_max")})
    states.sort(key=lambda r: -(r["divisions"] or 0))
    summ = sup.get("summary") or {}
    return {"provinces": provinces[:40], "states": states[:40],
            "supply_mean": summ.get("supply_percent_mean"),
            "supply_lowest": summ.get("supply_percent_lowest"),
            "bands": summ.get("bands") or {},
            "overdrawn_provinces": summ.get("overdrawn_provinces"),
            "missing": list(g(sup, "unavailable", "fields", default=[]))}


def _wars(raw, states_by_tag, player):
    w = raw.get("wars") or {}
    ws = raw.get("war_stats") or {}
    enemies = list(g(w, "wars", "enemies", default=[]) or [])
    per = {}
    # casualties per enemy: the relation between the player and that enemy
    for war in ws.get("wars") or []:
        for rel in war.get("relations") or []:
            cs = rel.get("countries") or []
            own = [c for c in cs if c.get("tag") == player]
            other = [c for c in cs if c.get("tag") != player]
            if own and other:
                t = other[0].get("tag")
                e = per.setdefault(t, {"tag": t, "name": other[0].get("name")})
                e["casualties_own"] = (e.get("casualties_own") or 0) + (own[0].get("casualties") or 0)
                e["casualties_enemy"] = (e.get("casualties_enemy") or 0) + (other[0].get("casualties") or 0)
                e["since"] = g(rel, "since", "iso")
    results = {}
    for war in g(w, "wars", "wars", default=[]) or []:
        t = g(war, "enemy", "tag")
        if t:
            results[t] = {"result": war.get("result"), "started": g(war, "started", "iso"),
                          "states_controlled": g(war, "enemy_land", "states_controlled"),
                          "states_owned": g(war, "enemy_land", "states_owned")}
    for t in set(enemies) | set(results):
        e = per.setdefault(t, {"tag": t})
        e.update({k: v for k, v in (results.get(t) or {}).items() if v is not None})
        e["at_war"] = t in enemies
    for t, body in (states_by_tag or {}).items():
        if t == player or not ok(body):
            continue
        e = per.setdefault(t, {"tag": t, "at_war": t in enemies})
        vp = body.get("victory_points") or {}
        total = num(vp.get("total"))
        held = 0.0
        held_by_us = 0.0
        for s in body.get("states") or []:
            for v in s.get("victory_points") or []:
                if v.get("controller") == player:
                    held_by_us += num(v.get("victory_points"), 0)
                if v.get("controller") != t:
                    held += num(v.get("victory_points"), 0)
        e["vp_total"] = total
        e["vp_held_by_us"] = round(held_by_us, 1)
        e["vp_held_by_others"] = round(held, 1)
        e["states_we_control"] = sum(1 for s in body.get("states") or [] if s.get("controller") == player)
        e["states_listed"] = len(body.get("states") or [])
    own_lost = []
    own = (states_by_tag or {}).get(player) or {}
    for s in own.get("states") or []:
        if s.get("owner") == player and s.get("controller") not in (None, player):
            own_lost.append({"state": s.get("state_id"), "name": s.get("name"),
                             "controller": s.get("controller")})
    for s in w.get("occupied_own_states") or []:
        sid = s.get("state_id") if isinstance(s, dict) else s
        if sid not in [x["state"] for x in own_lost]:
            own_lost.append({"state": sid, "name": s.get("name") if isinstance(s, dict) else None,
                             "controller": s.get("controller") if isinstance(s, dict) else None})
    threatened = [{"state": s.get("state_id"), "name": s.get("name"), "own_divisions": s.get("own_divisions"),
                   "defended": s.get("defended"), "hostile": s.get("hostile_tags")}
                  for s in w.get("threatened_own_states") or []]
    borders = [{"tag": b.get("neighbour_tag"), "own_states": b.get("own_border_states"),
                "their_states": b.get("neighbour_states"), "divisions": b.get("own_divisions_in_border_states"),
                "uncovered": b.get("uncovered_own_border_states") or [], "hostile": b.get("hostile")}
               for b in w.get("borders") or []]
    controlled = sorted(s.get("state_id") for s in own.get("states") or []
                        if s.get("controller") == player and s.get("state_id") is not None)
    return {"at_war": bool(g(w, "wars", "at_war")), "enemies": enemies,
            "states_controlled": controlled,
            "fighting": list(g(w, "wars", "fighting", default=[]) or []),
            "won": list(g(w, "wars", "wars_won", default=[]) or []),
            "per_enemy": per, "own_states_lost": own_lost, "threatened": threatened, "borders": borders,
            "allies_at_war": list(g(w, "wars", "allies_at_war", default=[]) or []),
            "missing": ["surrender progress", "war score", "war participation"]}


def _air(raw):
    air = raw.get("air") or {}
    ar = raw.get("air_regions") or {}
    wings = []
    for x in air.get("wings") or []:
        wings.append({"wing": x.get("wing"), "type": x.get("unit_type"), "airframe": x.get("aircraft_type"),
                      "aircraft": x.get("aircraft"), "base_state": g(x, "air_base", "state_id"),
                      "region": g(x, "region", "region_id") or g(x, "region", "id"),
                      "region_name": g(x, "region", "name"),
                      "missions": x.get("missions") or [], "standing_by": x.get("standing_by")})
    regions = {}
    for r in ar.get("regions") or []:
        missions = Counter()
        types = Counter()
        for wg in r.get("wings") or []:
            for m in wg.get("missions") or ["none"]:
                missions[m] += wg.get("aircraft") or 0
            types[wg.get("aircraft_type") or "?"] += wg.get("aircraft") or 0
        regions[str(r.get("region_id"))] = {
            "name": r.get("name"), "aircraft": r.get("aircraft_assigned"),
            "wings": r.get("wings_assigned"), "missions": dict(missions), "airframes": dict(types),
            "bases": len(r.get("air_bases_here") or []),
            "enemy_superiority": r.get("enemy_air_superiority")}
    no_mission = [w["wing"] for w in wings if not w["missions"]]
    return {"wings": wings, "regions": regions,
            "wings_without_region": ar.get("wings_without_region") or [],
            "wings_without_mission": no_mission,
            "aircraft_total": sum(w["aircraft"] or 0 for w in wings)
            or sum(r.get("aircraft") or 0 for r in regions.values()),
            "missing": ["air superiority per region", "air losses"]}


def _navy(raw):
    fl = raw.get("fleets") or {}
    fleets = []
    for f in fl.get("fleets") or []:
        tfs = []
        for t in f.get("task_forces") or []:
            tfs.append({"name": t.get("name") or t.get("task_force"),
                        "mission": g(t, "mission", "name") if isinstance(t.get("mission"), dict) else t.get("mission"),
                        "location": g(t, "location", "province") or t.get("location"),
                        "ships": t.get("ship_count") or len(t.get("ships") or [])})
        fleets.append({"fleet": f.get("fleet"), "admiral": f.get("admiral"),
                       "regions": [g(r, "name") or r for r in f.get("assigned_regions") or []],
                       "task_forces": tfs})
    cv = raw.get("convoys") or {}
    return {"fleets": fleets, "ships": g(fl, "totals", "ships"),
            "convoys": {k: g(cv, "convoys", k) for k in ("total", "in_use", "needed", "free")},
            "missing": ["naval dominance per region", "ship fuel and organisation"]}


def _occupation(raw):
    oc = raw.get("occupation") or {}
    rows = []
    for s in oc.get("states") or []:
        rows.append({"state": s.get("state_id"), "name": s.get("name"), "owner": s.get("owner"),
                     "role": s.get("role"), "resistance": s.get("resistance"),
                     "resistance_target": s.get("resistance_target"), "compliance": s.get("compliance")})
    return {"law": oc.get("player_occupation_law"), "states": rows, "missing": ["garrisons", "suppression"]}


def _intel(raw):
    it = raw.get("intelligence") or {}
    ag = it.get("agency") if isinstance(it.get("agency"), dict) else {}
    ops = deep(it, "operatives")
    return {"agency": ag.get("name") if ag else it.get("agency"),
            "created": deep(it, "created"),
            "operatives": len(ops) if isinstance(ops, list) else ops,
            "networks": deep(it, "networks")}


def _misc(raw):
    mi = raw.get("mios") or {}
    sp = raw.get("special_projects") or {}
    al = raw.get("alerts") or {}
    ev = raw.get("events") or {}
    return {
        "mios": {"listed": g(mi, "organisations", "listed"),
                 "state_read": g(mi, "mio_system", "state_read")},
        "special_projects": {"facilities": [{"facility": f.get("facility_id"),
                                             "project": g(f, "project", "key"),
                                             "scientist": g(f, "scientist", "name")}
                                            for f in sp.get("facilities") or []],
                             "scientists_free": sum(1 for s in sp.get("scientists") or []
                                                    if s.get("assigned_facility_id") is None)},
        "alerts": [{"alert": a.get("alert"), "frame": a.get("frame"), "todo": a.get("todo")}
                   for a in al.get("alerts") or []],
        "events_open": [e.get("event") for e in ev.get("events") or []],
        "peace_conference_open": ev.get("peace_conference_open"),
        "templates": [{"name": t.get("name") or t.get("template"), "training": t.get("training_lines")
                       or t.get("queued")} for t in (raw.get("templates") or {}).get("templates") or []],
    }


def _battles(raw):
    """The player's battles from hoi4_battles: per battle the side we fight on, width, reserves, who wins."""
    b = raw.get("battles") or {}
    if not b or "list" not in b:
        return {"readable": False,
                "missing": ["battles: hoi4_battles not answered (older server or no war)"]}
    rows = []
    for x in b.get("list") or []:
        ours = x.get("our_side")
        if ours not in ("attacker", "defender"):
            continue
        theirs = "defender" if ours == "attacker" else "attacker"
        me, them = x.get(ours) or {}, x.get(theirs) or {}
        width = x.get("width") or {}
        rows.append({
            "id": x.get("id"), "province": x.get("province"), "state": x.get("state"),
            "terrain": x.get("terrain"), "hours": x.get("hours"), "since": g(x, "since", "iso"),
            "we": ours, "enemy": sorted(them.get("tags") or []), "winning": x.get("winning"),
            "our_engaged": me.get("engaged"), "our_reserve": me.get("reserve"),
            "their_engaged": them.get("engaged"), "their_reserve": them.get("reserve"),
            "our_org": me.get("org_engaged_mean"), "their_org": them.get("org_engaged_mean"),
            "our_planning": me.get("planning_mean"), "our_supply": me.get("supply_mean"),
            "our_tactic": me.get("tactic"), "their_tactic": them.get("tactic"),
            "width_max": width.get("attacker_max"), "width_engaged": width.get("attacker_engaged"),
            "width_terrain": width.get("terrain"), "directions": width.get("directions"),
            "our_hours_left": me.get("hours_until_broken"), "their_hours_left": them.get("hours_until_broken"),
            "lines": x.get("lines") or [],
            "our_templates": sorted({d.get("template") for d in me.get("engaged_divisions") or []
                                     if d.get("template")}),
        })
    return {"readable": True, "count": b.get("battles"), "summary": b.get("summary") or {},
            "list": rows, "not_read": (b.get("list") or [{}])[0].get("not_read") if b.get("list") else []}


def build(raw, states_by_tag=None, executor=None, player_calls=None, trigger=None, read_ms=None,
          utc=None):
    """One snapshot from the raw read bodies (key -> body)."""
    errors = {k: (v.get("error") if isinstance(v, dict) else "no reply")
              for k, v in raw.items() if not ok(v)}
    raw = {k: (v if ok(v) else {}) for k, v in raw.items()}
    meta = _meta(raw)
    player = meta["player"]
    snap = {
        "schema": SCHEMA,
        "utc": utc,
        **meta,
        "trigger": trigger or "week",
        "politics": _politics(raw),
        "economy": _economy(raw),
        "production": _production(raw),
        "construction": _construction(raw),
        "research": _research(raw),
        "doctrines": _doctrines(raw),
        "leaders": _leaders(raw),
        "land": _land(raw),
        "stacking": _stacking(raw),
        "wars": _wars(raw, states_by_tag or {}, player),
        "battles": _battles(raw),
        "air": _air(raw),
        "navy": _navy(raw),
        "occupation": _occupation(raw),
        "intelligence": _intel(raw),
        **_misc(raw),
        "omissions": g(raw, "observe", "now", "omissions", default=[]) or [],
        "changes": [{"kind": c.get("kind"), "date": c.get("date"), "urgent": c.get("urgent"),
                     "detail": c.get("detail")} for c in g(raw, "observe", "changes", default=[]) or []],
        "executor": executor or {},
        "player_calls": player_calls or [],
        "read_errors": errors,
        "read_ms": read_ms or {},
    }
    return snap


def division_counts_by_province(snap):
    """province -> divisions, from the snapshot's division list (fallback: stacking rows)."""
    c = Counter(d["province"] for d in g(snap, "land", "divisions", default=[]) if d.get("province"))
    if not c:
        for p in g(snap, "stacking", "provinces", default=[]):
            c[p["province"]] = p["divisions"] or 0
    return c


def army_of_division(snap):
    return {d["id"]: d.get("army") for d in g(snap, "land", "divisions", default=[])}


def group_by(rows, key):
    out = defaultdict(list)
    for r in rows:
        out[r.get(key)].append(r)
    return out
