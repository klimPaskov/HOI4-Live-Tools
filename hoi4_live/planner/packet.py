"""The update packet: the compact state, the menus with their numbers, the ledger tail and the plan's strategy,
and the prompt a planning model (or the user's agent) answers with a patch.
"""

import json

from . import horizon as hz
from .patch import OPS
from .state import date_text, day_of

PROMPT = """You update a running Hearts of Iron IV campaign plan. The game keeps running; the executor follows
the plan in force until your patch is applied. Answer quickly.

Read `{packet}` (JSON). It holds the triggers, the game state now, the queues' horizons, a menu per domain with
the calculators' numbers and a default pick, the ledger of what was tried and failed, and the plan's strategy.

Write `{answer}`: one JSON object, the patch:
  {{"summary": "<one line>", "reason": "<why>", "escalated": {escalated}, "ops": [...]}}
Operations: {ops}. Keys you may change: the detail keys of the menus (research_path, focus_path,
politics_auto.order, production, production_priority, construction, construction_fallback_any_own_state,
vp_targets, training_rules, stockpile_targets, war.<number keys>).{strategy}

Rules:
* choose from the menus; every number you write comes from a menu or the state, never from memory;
* fix what the triggers name first; keep the default pick unless you can say why another option is better;
* never list again what the state shows done; keep the ids of items you do not change;
* never re-propose what the ledger shows failed;
* no other output file. If nothing should change, write {{"summary": "no change", "ops": []}}.
"""
STRATEGY_ALLOWED = """
This update is ESCALATED: you may also change goal-level keys (phases, checkpoints, war.axes, war.enemies,
horizons, contingencies) when the triggers show the strategy no longer fits; say why in `reason`."""


def strategy_view(plan, snap):
    day = snap.get("day")
    checkpoints = [c for c in plan.get("checkpoints") or [] if isinstance(c, dict)]
    upcoming = [{"id": c.get("id"), "by": c.get("by"), "must": c.get("must")} for c in checkpoints
                if day is None or (day_of(c.get("by")) or 0) >= day][:3]
    return {"goal": (plan.get("goal") or {}).get("summary"), "deadline": (plan.get("goal") or {}).get("deadline"),
            "phase": snap.get("phase"), "phases": [p.get("id") for p in plan.get("phases") or [] if isinstance(p, dict)],
            "next_checkpoints": upcoming, "horizons": plan.get("horizons"),
            "war": {k: (plan.get("war") or {}).get(k) for k in ("enemies", "axes", "supply_floor")}}


def tails(plan):
    techs = hz.planned_techs(plan)
    auto = plan.get("politics_auto") if isinstance(plan.get("politics_auto"), dict) else {}
    return {"research_planned_tail": techs[-12:], "focus_path": plan.get("focus_path"),
            "focus_fallback": plan.get("focus_fallback"), "politics_auto.order": auto.get("order"),
            "production": [{k: e.get(k) for k in ("id", "equipment", "factories", "remove")}
                           for e in plan.get("production") or [] if isinstance(e, dict)],
            "production_priority": plan.get("production_priority"),
            "construction": [{k: e.get(k) for k in ("building", "states", "count")}
                             for e in plan.get("construction") or [] if isinstance(e, dict)]}


def state_view(snap):
    keep = ("date", "player", "wars", "pp", "pp_gain", "civ_idle", "slots", "researching", "focus", "phase",
            "shortages", "fill_low", "armies", "lines")
    out = {k: snap.get(k) for k in keep}
    out["focuses_done"] = len(snap.get("focuses_done") or [])
    out["research_done_recent"] = (snap.get("research_done") or [])[-10:]
    return out


def build(plan, snap, h, triggers, menus, ledger, escalated):
    return {
        "date": snap.get("date") or (date_text(snap["day"]) if snap.get("day") is not None else None),
        "escalated": escalated,
        "triggers": [{k: t.get(k) for k in ("id", "kind", "class", "why")} for t in triggers],
        "state": state_view(snap),
        "horizons": {d: {"days": v.get("days"), "low": v.get("low"), "why": v.get("why")} for d, v in h.items()},
        "menus": {d: {k: m.get(k) for k in ("need", "options", "default", "progress") if m.get(k) is not None}
                  for d, m in menus.items()},
        "ledger": ledger.tail(15) if ledger else [],
        "strategy": strategy_view(plan, snap),
        "plan_now": tails(plan),
    }


def prompt(packet_path, answer_path, escalated):
    return PROMPT.format(packet=packet_path, answer=answer_path, escalated=json.dumps(bool(escalated)),
                         ops="|".join(OPS), strategy=STRATEGY_ALLOWED if escalated else "")
