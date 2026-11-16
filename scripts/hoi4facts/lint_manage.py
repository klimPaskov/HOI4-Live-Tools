"""Every resource, capacity, seat and alert has an owner (rule manage.owner), run by lint_plan.py (Lint.run).

The inventory (manage_inventory.py) lists what the country must spend or manage, from the game content and
its start (organisations, special projects, the agency, dockyards, fleets, air wings) and every alert the alert
reader names. An entry is owned when the plan sets one of its keys, a standing executor rule runs it (not
switched off in `manage.off`), or `manage.unowned_why` gives a reason for it ({item id: what happens instead},
required where no tool carries the lever yet: the gap rows are named). An alert is also owned when its item
has a reason, or when it is in manage.alerts_ignore with a reason in manage.alerts_ignore_why.

  manage.owner  (error) an entry with neither owner nor reason; (info) the planned-only levers (gap rows) the
                plan relies on a reason for.
facts.manage_inventory lists every entry with its owner.
"""
from . import manage_inventory as inv

RULE = "manage.owner"


def check(lint):
    if not lint.tag:
        return lint
    world = lint.world
    ctx = inv.country_context(lint.game, world, lint.tag)
    rows = []
    missing = []
    for e in inv.inventory(ctx):
        own = inv.owners(lint.plan, e)
        rows.append({"id": e["id"], "owner": own, "keys": e["keys"], "rule": e["rule"], "gap": e["gap"]})
        if own is None:
            missing.append(e)
    lint.facts["manage_inventory"] = rows
    items = [e for e in missing if e["kind"] == "item"]
    alerts = [e for e in missing if e["kind"] == "alert"]
    for e in items:
        how = (f"set {' or '.join(e['keys'])}" if e["keys"] else "no plan key carries it")
        if e["rule"]:
            how += f" (or keep the executor rule '{e['rule']}' on)"
        gap = f"; no tool yet for part of it (gap {', '.join(e['gap'])}): say what the player does" if e["gap"] else ""
        lint.add("error", RULE, f"manage.unowned_why.{e['id']}",
                 f"{e['what']} has no owner: {how}, or write manage.unowned_why.{e['id']} with what happens to it "
                 f"instead{gap}")
    if alerts:
        names = ", ".join(e["id"].split(":", 1)[1] for e in alerts)
        lint.add("error", RULE, "manage.alerts_ignore",
                 f"game alerts with no owner: {names}. Give each the plan key the executor serves it from "
                 "(see facts.manage_inventory), a reason for its item in manage.unowned_why, or list it in "
                 "manage.alerts_ignore with manage.alerts_ignore_why")
    relied = [r for r in rows if r["owner"] and r["owner"].startswith("reason") and r["gap"]]
    if relied:
        lint.add("info", RULE, "manage.unowned_why",
                 "planned-only levers the plan answers with a reason: "
                 + "; ".join(f"{r['id']} ({', '.join(r['gap'])})" for r in relied))
    return lint
