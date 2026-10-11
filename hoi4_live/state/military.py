"""The player's military from a plain-text save (phase 4): division templates, divisions, armies and their battle
plans, army groups, fleets and task forces, air wings.

Where things live in a 1.19 save:
    division_templates/division_template[]   name, country, regiments {sub_unit: [{x, y}]}, support {...}
    countries/TAG/units/division[]            id, location (province), division_template_id, division_name
                                              (override or name_order), strength, organisation, move_priority,
                                              execute_order (1 or 2 while the division executes its plan's orders)
    countries/TAG/units/fleet[]               name, task_force[] {name, location, ship[], mission {mission,
                                              regions ...}}
    countries/TAG/theatres/theatre[]          orders_group[] = armies: name, member {unit}, leader (character),
                                              order_instance[] {type 1 offensive / 2 front line / 5 area defence,
                                              path (provinces)}, execution_type; theater_group[] = army groups
    strategic_air/TAG/air_wing_pool[]         air_base, definition, air_wings[] {name, count, mission {type}}
    character_manager                         character ids -> tokens (names through localisation)

Every function takes an open SaveFile and never raises for missing blocks (empty results instead).
"""

import re

from .model import _list, _num, _one

ORDER_TYPES = {1: "offensive", 2: "front_line", 3: "fallback", 4: "naval_invasion", 5: "area_defense",
               6: "paradrop", 7: "spearhead"}
# a commander's role block inside character_manager: corps_commander = { id = { id=957 type=4713 } name="USA_..." }
ROLE_NAME = re.compile(rb"(?:corps_commander|field_marshal|navy_leader)=\{\s*id=\{ id=(\d+) type=\d+ \}\s*"
                       rb"name=\"([^\"]+)\"")


def _id(v):
    if isinstance(v, dict):
        return v.get("id")
    return v


def character_tokens(sf, ids):
    """{commander role id: name key} (armies and army groups point at the commander role, not the character)."""
    ids = {int(i) for i in ids if i is not None}
    if not ids:
        return {}
    out = {}
    try:
        text = sf.span_text(("character_manager",))
    except KeyError:
        return {}
    for m in ROLE_NAME.finditer(text):
        i = int(m.group(1))
        if i in ids:
            out[i] = m.group(2).decode("utf-8", "replace")
    return out


def templates(sf, tag):
    """[{id, name, regiments: [{type, x, y}], support: [...], battalions, locked}] owned by tag."""
    block = _one(sf, "division_templates") or {}
    out = []
    for t in _list(block.get("division_template")):
        if not isinstance(t, dict) or t.get("country") != tag:
            continue
        rows = {}
        for kind in ("regiments", "support"):
            items = []
            for sub, slots in (t.get(kind) or {}).items():
                for s in _list(slots):
                    if isinstance(s, dict):
                        items.append({"type": sub, "x": _num(s.get("x"), 0), "y": _num(s.get("y"), 0)})
            items.sort(key=lambda r: (r["x"], r["y"]))
            rows[kind] = items
        out.append({"id": _id(t.get("id")), "name": t.get("name"), "regiments": rows["regiments"],
                    "support": rows["support"], "battalions": len(rows["regiments"]),
                    "locked": t.get("is_locked") == "yes", "obsolete": t.get("obsolete") == "yes"})
    return out


def divisions(sf, tag):
    units = _one(sf, f"countries/{tag}/units") or {}
    out = []
    for d in _list(units.get("division")):
        if not isinstance(d, dict):
            continue
        ident = d.get("id")
        if isinstance(ident, list):
            ident = ident[0]
        nm = d.get("division_name") or {}
        out.append({"id": _id(ident), "name": nm.get("override"), "name_order": nm.get("name_order"),
                    "template_id": _id(d.get("division_template_id")), "location": d.get("location"),
                    "strength": _num(d.get("strength")), "organisation": _num(d.get("organisation")),
                    "executing": _num(d.get("execute_order"), 0) > 0, "move": d.get("move_priority")})
    return out


def armies(sf, tag, names=None):
    """Armies (orders groups) with members, leader, orders; army groups (theater groups)."""
    names = names or (lambda keys: {})
    th = _one(sf, f"countries/{tag}/theatres") or {}
    divs = {d["id"]: d for d in divisions(sf, tag)}
    rows, groups, leaders = [], [], []
    for theatre in _list(th.get("theatre")):
        if not isinstance(theatre, dict):
            continue
        for og in _list(theatre.get("orders_group")):
            if not isinstance(og, dict):
                continue
            members = [_id(m.get("unit")) for m in _list(og.get("member")) if isinstance(m, dict)]
            orders = []
            for o in _list(og.get("order_instance")):
                if not isinstance(o, dict) or o.get("virtual_order") == "yes" or "virtual_creator" in o:
                    continue                 # the game's own helper orders (the front segment of an offensive)
                t = int(_num(o.get("type"), 0))
                path = _list(o.get("path"))
                orders.append({"type": ORDER_TYPES.get(t, f"type_{t}"), "type_id": t,
                               "provinces": len(path), "from": path[0] if path else None,
                               "to": path[-1] if path else None,
                               "operation": [x for x in (o.get("prefix"), o.get("first"), o.get("second")) if x],
                               "can_execute": o.get("can_execute") == 1 or o.get("can_execute") == "1"})
            leader = _id(og.get("leader"))
            if leader is not None:
                leaders.append(leader)
            executing = sum(1 for m in members if divs.get(m, {}).get("executing"))
            rows.append({"id": _id(og.get("id")), "name": og.get("name"), "divisions": len(members),
                         "division_ids": members, "leader_id": leader, "orders": orders,
                         "executing_divisions": executing, "execution_type": og.get("execution_type"),
                         "avg_strength": _avg([divs.get(m, {}).get("strength") for m in members]),
                         "avg_organisation": _avg([divs.get(m, {}).get("organisation") for m in members])})
        for tg in _list(theatre.get("theater_group")):
            if isinstance(tg, dict):
                groups.append({"id": _id(tg.get("id")), "name": tg.get("name"),
                               "armies": [_id(x) for x in _list(tg.get("orders_group"))],
                               "leader_id": _id(tg.get("leader"))})
                if _id(tg.get("leader")) is not None:
                    leaders.append(_id(tg.get("leader")))
    toks = character_tokens(sf, leaders)
    op_keys = sorted({k for r in rows for o in r["orders"] for k in o["operation"]})
    nm = names(list(toks.values()) + op_keys)
    for r in rows:
        for o in r["orders"]:
            o["operation"] = " ".join(nm.get(k, k) for k in o["operation"]) or None
    for r in rows + groups:
        tok = toks.get(int(r["leader_id"])) if r.get("leader_id") is not None else None
        r["leader"] = (nm.get(tok) or tok) if tok else None
    by_id = {r["id"]: r["name"] for r in rows}
    for g in groups:
        g["army_names"] = [by_id.get(i, i) for i in g["armies"]]
    assigned = {m for r in rows for m in r["division_ids"]}
    return {"armies": rows, "army_groups": groups, "divisions": len(divs),
            "unassigned_divisions": len([i for i in divs if i not in assigned])}


def _avg(vals):
    v = [x for x in vals if x is not None]
    return round(sum(v) / len(v), 1) if v else None


def fleets(sf, tag):
    units = _one(sf, f"countries/{tag}/units") or {}
    out = []
    for f in _list(units.get("fleet")):
        if not isinstance(f, dict):
            continue
        tfs = []
        for tf in _list(f.get("task_force")):
            if not isinstance(tf, dict):
                continue
            mission = tf.get("mission") or {}
            tfs.append({"name": tf.get("name"), "location": tf.get("location"),
                        "ships": len([s for s in _list(tf.get("ship")) if isinstance(s, dict)]),
                        "mission": _num(mission.get("mission")),
                        # patrol is mission 0, so only the region list tells an assigned fleet from an idle one:
                        # a task force sent to a sea region lists it under accessible_regions
                        "regions": _list(mission.get("regions") or mission.get("region")
                                         or mission.get("accessible_regions")),
                        "hours_in_mission": _num(mission.get("hours_in_mission"))})
        out.append({"id": _id(f.get("id")), "name": f.get("name"), "task_forces": tfs,
                    "ships": sum(t["ships"] for t in tfs)})
    return out


def air_base_states(sf):
    """{air base id: state id}: a wing pool names its base by an id of its own, the base's block has the state."""
    block = _one(sf, "strategic_air") or {}
    out = {}
    for b in _list(block.get("air_base")):
        if isinstance(b, dict) and b.get("state") is not None:
            out[_id(b.get("id"))] = int(_num(b.get("state"), 0))
    return out


def air_wings(sf, tag):
    block = _one(sf, f"strategic_air/{tag}") or {}
    base_state = air_base_states(sf)
    out = []
    for pool in _list(block.get("air_wing_pool")):
        if not isinstance(pool, dict):
            continue
        for w in _list(pool.get("air_wings")):
            if not isinstance(w, dict):
                continue
            m = w.get("mission") or {}
            out.append({"name": w.get("name"), "type": pool.get("definition"), "base": _id(pool.get("air_base")),
                        "base_state": base_state.get(_id(pool.get("air_base"))),
                        "planes": _num(w.get("count")), "mission": _num(m.get("type")),
                        "mission_active": m.get("active") == "yes", "region": _id(m.get("region"))})
    return out
