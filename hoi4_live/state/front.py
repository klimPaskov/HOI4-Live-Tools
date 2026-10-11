"""The front as the save shows it: who stands where, who holds which state, which enemy states touch which of ours.

A human reads this off the map by eye; the tools read it from a save so an agent can order a whole theatre without
screenshots. Everything is by state id (the ids of history/states): the save has provinces (a division's location,
a controller override), `gamedata.mapdata` maps provinces to states and gives each state's neighbours, its anchor on
the map and its victory points.

    controllers(sf)            {province: controller tag} for provinces whose controller differs from their owner
    owners(sf)                 {state: owner tag}
    state_control(...)         {state: {"owner", "controller", "share"}}: the tag that controls most of the state
    divisions_by_state(...)    {state: [division, ...]} for one country
    situation(...)             armies, borders, enemy forces, objectives (see the function)
    suggest_orders(...)        {army: {front_target, target}} from a situation, one objective per army

Pure functions over an open SaveFile and the map index: unit-testable with a synthetic save and no game.
"""

import math
import re

from . import military

STATE_START = re.compile(rb"\n\t(\d+)=\{")
OWNER = re.compile(rb'\n\t\towner="([A-Z0-9]{3})"')
CONTROLLER = re.compile(rb'\n\t\tcontroller="([A-Z0-9]{3})"')


def _blocks(text):
    """[(id, chunk)] of the depth-1 numbered blocks of a block's text."""
    marks = [(m.start(), int(m.group(1))) for m in STATE_START.finditer(text)]
    out = []
    for i, (pos, ident) in enumerate(marks):
        end = marks[i + 1][0] if i + 1 < len(marks) else len(text)
        out.append((ident, text[pos:end]))
    return out


def owners(sf):
    try:
        text = sf.span_text(("states",))
    except KeyError:
        return {}
    out = {}
    for ident, chunk in _blocks(text):
        m = OWNER.search(chunk)
        if m:
            out[ident] = m.group(1).decode()
    return out


def controllers(sf):
    try:
        text = sf.span_text(("provinces",))
    except KeyError:
        return {}
    out = {}
    for ident, chunk in _blocks(text):
        m = CONTROLLER.search(chunk)
        if m:
            out[ident] = m.group(1).decode()
    return out


def state_control(idx, own, ctrl):
    """{state: {"owner", "controller", "share"}}: controller = the tag controlling most provinces (pixels) of it."""
    out = {}
    for k, st in idx["states"].items():
        sid = int(k)
        owner = own.get(sid) or st.get("owner")
        weight = {}
        total = 0
        for p in st["provinces"]:
            info = idx["provinces"].get(str(p))
            if not info:
                continue
            px = info["pixels"]
            tag = ctrl.get(p) or owner
            weight[tag] = weight.get(tag, 0) + px
            total += px
        if not total:
            out[sid] = {"owner": owner, "controller": owner, "share": 1.0}
            continue
        best = max(weight.items(), key=lambda kv: kv[1])
        out[sid] = {"owner": owner, "controller": best[0], "share": round(best[1] / total, 2)}
    return out


def divisions_by_state(sf, tag, idx):
    """{state: [{id, strength, organisation, province}]} for the divisions of one country, plus the unplaced ones."""
    by_state = {}
    for d in military.divisions(sf, tag):
        prov = d.get("location")
        sid = (idx["provinces"].get(str(prov)) or {}).get("state") if prov is not None else None
        by_state.setdefault(sid, []).append({"id": d["id"], "strength": d.get("strength"),
                                             "organisation": d.get("organisation"), "province": prov,
                                             "executing": d.get("executing")})
    return by_state


def _avg(vals):
    v = [x for x in vals if x is not None]
    return round(sum(v) / len(v), 2) if v else None


def _dist(idx, a, b):
    pa, pb = idx["states"][str(a)]["anchor"], idx["states"][str(b)]["anchor"]
    return math.hypot(pa[0] - pb[0], pa[1] - pb[1])


def vp_value(idx, sid):
    st = idx["states"].get(str(sid))
    return sum(st["vp"].values()) if st and st.get("vp") else 0


def situation(sf, tag, idx, enemies, names=None, focus=None):
    """What the war looks like for `tag` against `enemies` (tags).

    armies      each army: name, divisions, orders (type/operation), executing, average strength and organisation,
                `stand` = the states its divisions stand in ({state, name, divisions}) and `touch` = the enemy-held
                states bordering them ({state, name, owner, enemy_divisions, enemy_org}), nearest first
    fronts      per holder (the tag that controls the state): the pairs (our state, their state) along the border, with the enemy divisions in
                their state and ours in ours
    enemy       per enemy tag: divisions, total strength, the states holding the most of them
    objectives  enemy-held states with victory points, biggest first, with the distance (map pixels) from the
                nearest of our armies
    unplaced    our divisions that stand in no state (at sea, in a transport) or in no army
    """
    names = names or {}
    own = owners(sf)
    ctrl = controllers(sf)
    control = state_control(idx, own, ctrl)
    ours = divisions_by_state(sf, tag, idx)
    enemy_divs = {e: divisions_by_state(sf, e, idx) for e in enemies}
    enemy_in_state = {}
    for e, by in enemy_divs.items():
        for sid, ds in by.items():
            if sid is None:
                continue
            slot = enemy_in_state.setdefault(sid, {"divisions": 0, "org": [], "strength": [], "tags": set()})
            slot["divisions"] += len(ds)
            slot["org"] += [d["organisation"] for d in ds]
            slot["strength"] += [d["strength"] for d in ds]
            slot["tags"].add(e)

    def sname(sid):
        return names.get(f"STATE_{sid}") or f"state {sid}"

    def hostile(sid):
        c = control.get(sid) or {}
        return c.get("controller") in enemies or (c.get("owner") in enemies and c.get("controller") != tag)

    mil = military.armies(sf, tag, lambda keys: {})
    division_state = {}
    for sid, ds in ours.items():
        for d in ds:
            division_state[d["id"]] = sid
    armies = []
    for a in mil["armies"]:
        stand = {}
        for did in a["division_ids"]:
            sid = division_state.get(did)
            if sid is not None:
                stand[sid] = stand.get(sid, 0) + 1
        touch = {}
        for sid in stand:
            for nb in idx["states"].get(str(sid), {}).get("neighbours", []):
                if hostile(nb):
                    e = enemy_in_state.get(nb, {"divisions": 0, "org": [], "tags": set()})
                    touch[nb] = {"state": nb, "name": sname(nb), "owner": control[nb]["owner"],
                                 "held_by": control[nb]["controller"],
                                 "enemy_divisions": e["divisions"], "enemy_org": _avg(e["org"]), "from": sid}
        main = max(stand.items(), key=lambda kv: kv[1])[0] if stand else None
        ordered = sorted(touch.values(), key=lambda t: (_dist(idx, main, t["state"]) if main else 0))
        armies.append({"name": a["name"], "divisions": a["divisions"], "orders": a["orders"],
                       "executing": a["executing_divisions"], "avg_strength": a["avg_strength"],
                       "avg_organisation": a["avg_organisation"],
                       "main_state": main, "main_state_name": sname(main) if main else None,
                       "stand": [{"state": s, "name": sname(s), "divisions": n}
                                 for s, n in sorted(stand.items(), key=lambda kv: -kv[1])],
                       "touch": ordered})
    fronts = {}
    seen_pairs = set()
    for sid, ds in ours.items():
        if sid is None:
            continue
        for nb in idx["states"].get(str(sid), {}).get("neighbours", []):
            if hostile(nb) and (sid, nb) not in seen_pairs:
                seen_pairs.add((sid, nb))
                e = enemy_in_state.get(nb, {"divisions": 0, "org": []})
                owner = control[nb]["controller"]
                fronts.setdefault(owner, []).append({"ours": sid, "ours_name": sname(sid), "our_divisions": len(ds),
                                                     "theirs": nb, "theirs_name": sname(nb),
                                                     "enemy_divisions": e["divisions"], "enemy_org": _avg(e["org"])})
    enemy = {}
    for e in enemies:
        by = enemy_divs.get(e) or {}
        allds = [d for ds in by.values() for d in ds]
        top = sorted(((s, len(ds)) for s, ds in by.items() if s is not None), key=lambda kv: -kv[1])[:8]
        enemy[e] = {"divisions": len(allds), "avg_strength": _avg([d["strength"] for d in allds]),
                    "avg_organisation": _avg([d["organisation"] for d in allds]),
                    "top_states": [{"state": s, "name": sname(s), "divisions": n} for s, n in top],
                    "at_sea_or_unplaced": len(by.get(None, []))}
    placed = [s for s in ours if s is not None]
    objectives = []
    for k, st in idx["states"].items():
        sid = int(k)
        if not st.get("vp") or not hostile(sid):
            continue
        near = min((_dist(idx, sid, p) for p in placed), default=None)
        e = enemy_in_state.get(sid, {"divisions": 0})
        objectives.append({"state": sid, "name": sname(sid), "owner": control[sid]["owner"],
                           "vp": sum(st["vp"].values()), "distance": round(near) if near is not None else None,
                           "enemy_divisions": e["divisions"]})
    objectives.sort(key=lambda o: (-o["vp"], o["distance"] if o["distance"] is not None else 1e9))
    in_army = {d for a in mil["armies"] for d in a["division_ids"]}
    return {"tag": tag, "enemies": list(enemies), "armies": armies, "army_groups": mil["army_groups"],
            "fronts": fronts, "enemy": enemy, "objectives": objectives[:40],
            "unplaced": {"no_state": len(ours.get(None, [])), "no_army": sum(len(v) for v in ours.values()) - len(
                [d for d in in_army if d in division_state])}}


def suggest_orders(sit, objectives=None, max_armies=None):
    """One offensive per army: the nearest still unclaimed objective (an explicit list, else the situation's victory
    point states of the enemy) and, as the front target, the enemy-held state beside the army that is nearest to it.
    -> [{army, front_target, target, from, why}], armies that touch no enemy state are listed in `idle`."""
    idx = sit.get("_idx")
    cand = [o["state"] for o in sit["objectives"]] if not objectives else [int(o) for o in objectives]
    taken = set()
    plan, idle = [], []
    for a in sit["armies"]:
        if not a["touch"] or a["main_state"] is None:
            idle.append(a["name"])
            continue
        best, bd = None, None
        for obj in cand:
            if obj in taken:
                continue
            d = _dist(idx, a["main_state"], obj) if idx else 0
            if bd is None or d < bd:
                best, bd = obj, d
        if best is None:
            best = a["touch"][-1]["state"]
        front = min(a["touch"], key=lambda t: _dist(idx, t["state"], best) if idx else 0)
        taken.add(best)
        plan.append({"army": a["name"], "front_target": front["state"], "front_name": front["name"],
                     "target": best, "from": a["main_state"],
                     "why": f"{a['divisions']} divisions at {a['main_state_name']}, front state {front['name']}, "
                            f"objective {best}"})
        if max_armies and len(plan) >= max_armies:
            break
    return {"orders": plan, "idle": idle}
