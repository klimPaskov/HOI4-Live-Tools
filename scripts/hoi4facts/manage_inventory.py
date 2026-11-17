#!/usr/bin/env python3
"""manage_inventory.py: everything a country must spend or manage, and who owns each in a plan.

A plan "manages everything" only when every resource, capacity, seat and game alert has an owner: a plan key
the executor reads, a standing executor rule, or, where no tool carries it yet, a written reason in the plan
(`manage.unowned_why`) that says what happens instead. The inventory is built from what the game has, not
from memory or examples:

* the items: the engine's spendable resources and capacities, each present only when the game content (or
  the country) has it: a folder of the game files (`common/military_industrial_organization`,
  `common/special_projects`, `common/intelligence_agency_upgrades`, ...), the country's start OOB (dockyards,
  fleets, air wings), its organisations (`facts.py mio`), so a mod without a mechanic drops the item;
* the alerts: every alert the alert reader names (native `alerts.rs` label table, mirrored in ALERTS; a test
  keeps the two equal), each with the executor's route for it (`plan_manage.rs` route).

Each item names its owner kinds:
  keys   plan keys (dotted paths) the executor reads; the plan owns the item when one is set (not empty);
  rule   a standing executor rule that runs by default unless the plan switches it off (`manage.off`);
  gap    gap register rows of the levers no tool carries yet (planned-only): the plan must still write a reason.
`owners(plan, item)` says which owner the plan has; lint `manage.owner` fails a plan with an item that has
neither an owner nor a reason in `manage.unowned_why` ({item id: reason}).

Usage: manage_inventory.py show [--tag TAG] [--game DIR]   (the inventory for a country, with owner kinds)
Stdlib only, Python 3.9+.
"""
import argparse
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
if os.path.dirname(HERE) not in sys.path:
    sys.path.insert(0, os.path.dirname(HERE))

# id, what, present (see present()), keys, rule (manage.off name or None), gap rows (no tool yet)
ITEMS = [
    ("political_power", "political power: laws, advisors, decisions; never idle above the next buy",
     "always", ["politics", "politics_auto.order", "decisions", "decision_rules"], None, []),
    ("command_power", "command power: field-marshal promotions, commander traits and abilities, advisor upkeep",
     "always", ["army_group", "armies.generals"], None,
     ["formations.commander_traits", "formations.leader_abilities"]),
    ("army_experience", "army experience: doctrines, templates, land designs, exercises",
     "always", ["xp_budget.army", "doctrines", "templates", "designs"], None, []),
    ("navy_experience", "navy experience: naval doctrines and ship designs",
     "fleets", ["xp_budget.navy", "doctrines", "designs"], None, []),
    ("air_experience", "air experience: air doctrines and plane designs",
     "air_wings", ["xp_budget.air", "doctrines", "designs"], None, []),
    ("research_slots", "research slots: every slot busy all run",
     "always", ["research_path", "research_priority"], None, []),
    ("construction", "construction capacity (civilian factories on construction): queue never idle or starved",
     "always", ["construction", "construction_plan", "construction_fallback_any_own_state"], None, []),
    ("consumer_goods", "civilian factories on consumer goods: the economy law and the decisions that lower it",
     "always", ["politics", "decision_rules"], None, []),
    ("trade", "civilian factories on trade and resource imports",
     "always", ["auto_import", "trade"], "resources", []),
    ("military_lines", "military factories: one line per equipment, sized to need, in a priority order",
     "always", ["production", "production_priority"], None, []),
    ("naval_lines", "dockyards: ship and convoy lines",
     "dockyards", ["production", "manage.idle_dockyards"], "convoys", []),
    ("manpower", "manpower and conscription laws",
     "always", ["manage.conscription_laws", "politics"], "manpower", []),
    ("training", "division training sized to the plan's gates and equipment",
     "always", ["training", "training_rules"], None, []),
    ("industrial_organisations", "industrial organisations: one on every line, traits as points come, policies",
     "mio", ["mio", "production.organization"], None, ["mio.policy", "mio.line_manufacturer"]),
    ("special_projects", "special-project facilities, scientists and the project queue",
     "special_projects", ["special_projects"], None, []),
    ("operatives", "intelligence agency, its upgrades and operatives",
     "intelligence", ["intelligence"], None, []),
    ("decisions", "decisions: every useful one taken, the rest named never",
     "always", ["decisions", "decision_rules"], None, []),
    ("government_seats", "political advisors and designer seats",
     "always", ["politics_auto.order", "politics"], None, []),
    ("military_seats", "chiefs of army, navy and air force and the high command",
     "always", ["politics_auto.order", "politics"], None, ["advisors.grow_high_command"]),
    ("doctrines", "doctrine choices paid from experience: the plan's entries, then the fill's defaults per folder",
     "always", ["doctrines", "doctrine_defaults"], "doctrines", []),
    ("officer_corps", "officer-corps spirits", "always", [], None, ["advisors.officer_corps_spirits"]),
    ("laws", "economy, trade and conscription laws", "always", ["politics"], None, []),
    ("faction", "faction membership: join, create, invite members", "always", ["faction", "diplomacy[action=invite_to_faction]"], None,
     ["factions.invite_members", "factions.goals"]),
    ("faction_requests", "incoming requests: joining, access, lend-lease, calls to arms",
     "always", ["incoming_proposals"], None, ["diplomacy.incoming_proposals"]),
    ("call_allies", "allies called into the plan's wars (or kept out)", "always",
     ["faction.call_allies", "diplomacy[action=call_to_arms]", "diplomacy[action=call_allies]"], None,
     ["diplomacy.call_allies"]),
    ("war_goals", "war goals, justifications and declarations", "always",
     ["diplomacy[action=justify]", "diplomacy[action=declare]"], None, []),
    ("focus", "the national focus slot never idle", "always", ["focus_path", "focus_fallback"], None, []),
    ("armies", "every division in an army with a general", "always", ["armies", "army_layout"], None, []),
    ("air_wings", "every air wing on a mission", "air_wings", ["air", "air_plan"], None, []),
    ("fleets", "fleets, admirals, missions and repair", "fleets", ["navy"], None, []),
    ("convoys", "convoys for trade and overseas armies", "dockyards", ["manage.convoys_free"], "convoys", []),
    ("stockpile", "equipment stockpile days of cover", "always", ["stockpile_targets"], "stockpile", []),
    ("fuel", "fuel stock", "always", ["manage.fuel_floor"], "fuel", []),
    ("occupation", "occupation laws and garrisons", "always", ["occupation", "garrison_template"], None, []),
    ("designs", "equipment designs (tanks, planes, ships)", "designer", ["designs"], None, []),
    ("stability_war_support", "stability and war support above the plan's gates", "always",
     ["manage.stability_floor", "politics", "decision_rules"], "stability", []),
]

# The game alerts the alert reader names (native/src/launch/macos/alerts.rs `label`) with the plan keys the
# executor serves each from (plan_manage.rs `route`; [] = it only wakes the player) and the inventory item
# whose owner also answers it. tests/test_manage_inventory.py keeps this table equal to the Rust sources.
ALERTS = {
    "no_national_focus": (["focus_path", "focus_fallback"], "focus"),
    "at_war": (None, None),
    "free_dockyards": (["production"], "naval_lines"),
    "missing_equipment_production": (["production", "production_priority"], "military_lines"),
    "modify_government": (["politics", "politics_auto"], "government_seats"),
    "unfulfilled_import_requests": (["auto_import"], "trade"),
    "air_wings_without_mission": (["air", "air_plan"], "air_wings"),
    "no_divisions_in_training": (["training"], "training"),
    "resistance": (["occupation"], "occupation"),
    "unassigned_divisions": (["armies"], "armies"),
    "decisions_available": (["decisions", "decision_rules"], "decisions"),
    "events_or_decisions_timeout": ([], "decisions"),
    "not_enough_garrison": ([], "occupation"),
    "doctrines_available": (["doctrines"], "doctrines"),
    "modify_officer_corps": (["politics_auto"], "military_seats"),
    "mio_traits": (["mio"], "industrial_organisations"),
    "admiral_assignable": (["navy"], "fleets"),
    "insufficient_energy": (["auto_import"], "trade"),
    "insufficient_resources": (["auto_import"], "trade"),
    # frames the alert reader names from the management pass on (an alert the reader does not name yet is
    # harmless here; one it names that is missing here fails tests/test_manage_inventory.py)
    "research_slot_available": (None, None),
    "free_civilian_factories": (["construction"], "construction"),
    "free_military_factories": (["production", "production_priority"], "military_lines"),
    "outdated_equipment_in_production": ([], "military_lines"),
    "operative_recruitable": (["intelligence"], "operatives"),
    "special_project_available": (["special_projects"], "special_projects"),
    "scientist_unassigned": (["special_projects"], "special_projects"),
}


def _get(plan, path):
    """The value at a dotted path; a list on the way matches when any element has the rest. A part
    `key[field=value]` keeps only the list elements whose field has that value (diplomacy[action=declare])."""
    cur = [plan]
    for part in path.split("."):
        filt = None
        if part.endswith("]") and "[" in part:
            part, cond = part[:-1].split("[", 1)
            filt = cond.split("=", 1)
        nxt = []
        for c in cur:
            if isinstance(c, dict) and part in c:
                nxt.append(c[part])
            elif isinstance(c, list):
                nxt += [x[part] for x in c if isinstance(x, dict) and part in x]
        if filt:
            nxt = [[x for x in v if isinstance(x, dict) and str(x.get(filt[0])) == filt[1]]
                   for v in nxt if isinstance(v, list)]
        cur = nxt
    vals = [v for v in cur if v not in (None, [], {}, False, "")]
    return vals[0] if vals else None


def present(kind, ctx):
    """Is the item there for this country and content? ctx: {"folders": set of non-empty folders,
    "dockyards", "fleets", "air_wings", "mio", "designer"} (bools; None when unknown: counts as present)."""
    if kind == "always":
        return True
    v = ctx.get(kind)
    return True if v is None else bool(v)


def country_context(game, world, tag):
    """What the inventory's presence tests read, from the game files and the country's start."""
    def has(folder):
        try:
            return bool(list(game.files(folder)))
        except Exception:  # noqa: BLE001 - a missing folder is an absent mechanic
            return False
    ctx = {"special_projects": None, "intelligence": has("common/intelligence_agency_upgrades"),
           "designer": has("common/units/equipment/modules")}
    if world is not None:
        try:
            ctx["special_projects"] = bool(world.special_projects())
        except Exception:  # noqa: BLE001
            pass
    if world is not None and tag:
        try:
            oob = world.oob(tag)
            ctx["fleets"] = bool(oob.get("fleets"))
            ctx["air_wings"] = bool(oob.get("air"))
        except Exception:  # noqa: BLE001
            pass
        try:
            ctx["mio"] = any(not r.get("foreign") for r in world.mio(tag)["organisations"])
        except Exception:  # noqa: BLE001
            pass
        try:
            from . import derive
            kinds = derive.building_kinds(game)
            sb = derive.state_buildings(game, world.ctx_for(tag), world.date)
            own = derive.tag_states(game, world.ctx_for(tag), tag, world.date)
            ctx["dockyards"] = any(v for s in own for b, v in sb.get(s, {}).items() if b in kinds["naval"]) or \
                ctx.get("fleets", False)
        except Exception:  # noqa: BLE001
            pass
    return ctx


def inventory_items():
    return [{"id": i, "what": w, "keys": k, "rule": r, "gap": g, "kind": "item"} for i, w, _, k, r, g in ITEMS]


def inventory(ctx):
    """[{"id", "what", "keys", "rule", "gap", "kind": "item"}] present for the country, then the alerts."""
    out = []
    for iid, what, kind, keys, rule, gap in ITEMS:
        if present(kind, ctx):
            out.append({"id": iid, "what": what, "keys": keys, "rule": rule, "gap": gap, "kind": "item"})
    have = {x["id"] for x in out}
    for alert, (keys, item) in ALERTS.items():
        if keys is None:
            continue
        if item and item not in have:
            continue
        out.append({"id": f"alert:{alert}", "what": f"the game alert {alert}", "keys": keys, "rule": None,
                    "gap": [], "kind": "alert", "item": item})
    return out


def owners(plan, entry):
    """(owner text or None) of an inventory entry in a plan: a set key, a running rule, the item's owner (for
    an alert), or a reason in manage.unowned_why / manage.alerts_ignore_why."""
    manage = plan.get("manage") if isinstance(plan.get("manage"), dict) else {}
    for k in entry["keys"]:
        if _get(plan, k) is not None:
            return f"key {k}"
    if entry.get("rule") and entry["rule"] not in (manage.get("off") or []):
        return f"executor rule {entry['rule']}"
    why = manage.get("unowned_why") if isinstance(manage.get("unowned_why"), dict) else {}
    if why.get(entry["id"]):
        return "reason"
    if entry["kind"] == "alert":
        name = entry["id"].split(":", 1)[1]
        item = next((x for x in inventory_items() if x["id"] == entry.get("item")), None)
        if item is not None:
            own = owners(plan, item)
            if own:
                return f"{own} (item {item['id']})"
        ign = manage.get("alerts_ignore_why") if isinstance(manage.get("alerts_ignore_why"), dict) else {}
        if name in (manage.get("alerts_ignore") or []) and ign.get(name):
            return "ignored with a reason"
        item = entry.get("item")
        if item and why.get(item):
            return f"reason for {item}"
    return None


def main(argv=None):
    ap = argparse.ArgumentParser(description="the management inventory of a country")
    ap.add_argument("cmd", choices=["show"])
    ap.add_argument("--tag", default="GER")
    ap.add_argument("--date", default="1936.1.1")
    ap.add_argument("--game", default=os.environ.get("HOI4_DIR"))
    ap.add_argument("--userdir", default=None, help="user folder whose enabled mods are layered over the install")
    ap.add_argument("--mod", action="append", default=[], help="a .mod descriptor or mod folder (repeatable)")
    ap.add_argument("--mode", default="fair")
    ap.add_argument("--plan", help="a plan.json: print each item's owner in it")
    args = ap.parse_args(argv)
    ctx = {}
    if args.game:
        from hoi4facts.lint_plan import load_game
        from hoi4facts.world import World
        game = load_game(args.game, args.userdir, args.mod)
        ctx = country_context(game, World(game, args.tag, args.date), args.tag)
    plan = None
    if args.plan:
        with open(args.plan, encoding="utf-8") as fh:
            plan = json.load(fh)
    for e in inventory(ctx):
        own = owners(plan, e) if plan is not None else None
        gap = f" gap: {', '.join(e['gap'])}" if e["gap"] else ""
        rule = f" rule: {e['rule']}" if e["rule"] else ""
        tail = f"  -> {own or 'NO OWNER'}" if plan is not None else ""
        print(f"{e['id']:<36} keys: {', '.join(e['keys']) or '-'}{rule}{gap}{tail}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
