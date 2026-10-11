"""hoi4_front: read the front from a save and order a whole theatre in one or two calls.

The Polish war of the first German run stalled for months because each army's lines were drawn by hand, state by
state, after reading the map by eye. This tool does the reading (state ids, who stands where, which enemy states
touch which army, where the enemy divisions are, which victory points are left) and the ordering (a front line and an
offensive line for every army, through `hoi4_orders attack`, all by state id).

    situation  armies, their borders with the enemy, the enemy's forces, the objectives (from the newest save, or from
               a fresh one with fresh=true)
    plan       the same plus the suggested order for each army: nearest unclaimed objective, front state beside it
    order      execute: the suggested plan, or `orders` given by the agent ({army, front_target, target} by state id);
               every army is tried, failures are listed
"""

import time

from ..errors import ToolError
from ..game import saves
from ..gamedata import mapdata
from ..state import assemble, front, queries
from . import campaign_common as cc
from .registry import ToolResult, tool

TAG = {"type": "string", "pattern": "^[A-Z][A-Z0-9]{2}$", "description": "default: the player"}


def _enemies(sf, tag, only=None):
    from ..state.model import _one
    rel = _one(sf, f"countries/{tag}/diplomacy/active_relations") or {}
    out = [other for other, r in rel.items() if isinstance(r, dict) and isinstance(r.get("war_relation"), dict)]
    if only:
        out = [e for e in out if e in only]
    return out


def read_situation(session, tag, save="latest", fresh=False, enemies=None):
    """(situation dict with the map index under `_idx`, source note)."""
    if fresh:
        saves.save(session, "hoi4live_front", True)
        save = "hoi4live_front"
    snap = session.telemetry.snapshot(tag) if tag else None
    path, note = assemble.find_save(session, tag, save, (snap or {}).get("date"))
    idx = mapdata.for_session(session)
    try:
        sf = queries.open_save(path)
        hdr = sf.header()
        who = _enemies(sf, tag, enemies)
        loc = cc.names(session, [f"STATE_{k}" for k in idx["states"]])
        sit = front.situation(sf, tag, idx, who, loc,
                              include_enemy_details=getattr(session, "mode", "campaign") == "test")
    finally:
        queries.close_all()
    sit["_idx"] = idx
    sit["as_of"] = hdr.get("date")
    sit["save"] = path
    if note:
        sit["note"] = note
    return sit


def _brief(sit, armies_touch=4):
    out = {"as_of": sit["as_of"], "enemies": sit["enemies"], "unplaced": sit["unplaced"],
           "enemy": sit["enemy"], "objectives": sit["objectives"][:12], "army_groups": [
               {"name": g["name"], "armies": g.get("army_names")} for g in sit["army_groups"]]}
    out["armies"] = [{"name": a["name"], "divisions": a["divisions"], "executing": a["executing"],
                      "strength": a["avg_strength"], "organisation": a["avg_organisation"],
                      "orders": [o["type"] + (f" ({o['operation']})" if o.get("operation") else "")
                                 for o in a["orders"]],
                      "stand": a["stand"][:4], "touch": a["touch"][:armies_touch]} for a in sit["armies"]]
    out["fronts"] = {e: [f"{p['ours_name']} ({p['our_divisions']}) vs {p['theirs_name']} "
                         f"({p['enemy_divisions'] if p['enemy_divisions'] is not None else 'unknown'})"
                         for p in sorted(v, key=lambda p: (p["enemy_divisions"] is None,
                                                          -(p["enemy_divisions"] or 0), p["theirs"]))[:8]]
                     for e, v in sit["fronts"].items()}
    return out


@tool("hoi4_front",
      "The front from a save, by state id, and the orders for a whole theatre. situation: every army (divisions, "
      "strength, organisation, orders), the states it stands in, the enemy-held states beside it with the enemy "
      "divisions there in test mode only; campaign mode reads our units and public map control, with enemy "
      "forces unknown. The objectives are victory-point states still in enemy hands, nearest "
      "first). plan: the same plus one suggested order per army (nearest unclaimed objective, the enemy state "
      "beside the army nearest to it). order: execute the plan or your own `orders` "
      "[{army, front_target, target}] (state ids; front line against the owner of front_target, offensive line "
      "through target, execute); every army is tried and failures listed. fresh=true saves the game first so the "
      "read is current (about 30 s); otherwise the newest save is used and its date is reported.",
      {"properties": {
          "action": {"type": "string", "enum": ["situation", "plan", "order"], "default": "situation"},
          "enemy": {"type": "array", "items": {"type": "string"},
                    "description": "only these enemy tags (default: every country at war with us)"},
          "objectives": {"type": "array", "items": {"type": "string"},
                         "description": "plan/order: state ids to take, in priority order (default: victory-point "
                                        "states of the enemy, nearest first)"},
          "orders": {"type": "array", "items": {"type": "object", "properties": {
              "army": {"type": "string"}, "front_target": {"type": "string"}, "target": {"type": "string"}}},
              "description": "order: explicit orders instead of the plan"},
          "armies": {"type": "array", "items": {"type": "string"},
                     "description": "plan/order: only these armies (default all that touch the enemy)"},
          "fresh": {"type": "boolean", "default": False, "description": "save first (the read is then current)"},
          "save": {"type": "string", "default": "latest"},
          "tag": TAG}},
      family="military", lock="self", phase=7,
      examples=[{"args": {"action": "situation", "fresh": True}},
                {"args": {"action": "plan", "enemy": ["POL"]}},
                {"args": {"action": "order", "orders": [{"army": "Heer Nord", "front_target": "63", "target": "98"}]}}])
def hoi4_front(session, args):
    a = args["action"]
    tag = cc.player_tag(session, args.get("tag"))
    if not tag:
        raise ToolError("no player tag known (no telemetry and no save)")
    if args.get("fresh"):
        with session.ui_mutex, session.lock.hold("hoi4_front"):
            sit = read_situation(session, tag, args["save"], True, args.get("enemy"))
    else:
        sit = read_situation(session, tag, args["save"], False, args.get("enemy"))
    if not sit["enemies"]:
        return ToolResult(f"{tag} is at war with nobody in the save of {sit['as_of']}", {"as_of": sit["as_of"]})
    if a == "situation":
        brief = _brief(sit)
        top = ", ".join(f"{e}: {v['divisions']} div" if v["divisions"] is not None else f"{e}: forces unknown"
                        for e, v in sit["enemy"].items())
        return ToolResult(f"front as of {sit['as_of']}: {len(sit['armies'])} army(ies) vs {top}", brief)
    wanted = args.get("armies")
    if wanted:
        sit["armies"] = [x for x in sit["armies"] if x["name"] in wanted]
    plan = front.suggest_orders(sit, args.get("objectives"))
    if a == "plan":
        return ToolResult(f"plan for {len(plan['orders'])} army(ies) as of {sit['as_of']}"
                          + (f"; no enemy beside: {', '.join(plan['idle'])}" if plan["idle"] else ""),
                          {"as_of": sit["as_of"], **plan})
    orders = args.get("orders")
    if not orders:
        orders = [{"army": o["army"], "front_target": str(o["front_target"]), "target": str(o["target"])}
                  for o in plan["orders"]]
    if not orders:
        raise ToolError("nothing to order: no army touches an enemy state in this save", {"idle": plan["idle"]})
    from . import military_tools as mt
    with session.ui_mutex, session.lock.hold("hoi4_front"):
        res = mt._orders_batch(session, {"orders": orders, "save": args["save"], "replace": True})
    res.data["plan"] = plan if not args.get("orders") else None
    return res
