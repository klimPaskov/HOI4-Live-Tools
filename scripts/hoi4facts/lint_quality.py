"""The quality of the force on each war day (run by lint_force.check): a plan can field the AI's division count
and still deal a fifth of its damage. Composition only: battalions by class and width from common/units; combat
stats need equipment and are read live.

  force.template_floor  on an offensive land war's first day the main line template (most divisions in the forecast)
                        has no line artillery battalion, or fewer than the game's own AI fields in its line template
                        on that date (baselines `templates`); the main armour template has no medium or heavier
                        tank battalion although the chassis can be researched by then, or fewer battalions than the
                        AI's; under half of the armoured divisions carry medium or heavier tanks.
  force.staff           by the first offensive war: no officer_corps entry for the army (and the air force when the
                        plan flies an air plan); a theorist, army chief or high-command seat with no named advisor
                        dated before the war (an unnamed seat takes the first advisor the game offers, of any
                        branch); no political entry at all for a seat.
  force.exercise        no army exercise entry (xp_budget.army purpose exercise) that starts EXERCISE_DAYS before the
                        first offensive war: divisions trained since the start fight at Trained (+0%) where
                        Regular is +25%.
"""
import glob
import json
import os

from . import derive
from . import force as fm

EXERCISE_DAYS = 80   # days of exercise that take a trained division to regular [bed read, 1.24% a day]
MEDIUM_SHARE = 0.5   # share of the armoured divisions that carry medium or heavier tanks on the war day [H]
SEATS = (("research_production", 5, "theorist"), ("military_staff", 0, "army chief"),
         ("military_staff", 3, "high command"), ("military_staff", 4, "high command"),
         ("military_staff", 5, "high command"))
RULES = ("force.template_floor", "force.staff", "force.exercise")
HEAVY = ("medium_armor", "heavy_armor", "modern_armor", "super_heavy_armor")


def _fmt(d):
    return derive.fmt(derive.date_of(d)) if d is not None else "undated"


def shape(game, units):
    """{"battalions", "artillery" (line battalions), "armour", "medium" (medium or heavier), "mobile", "width"}."""
    ut = derive.unit_types(game)
    out = {"battalions": 0, "artillery": 0, "armour": 0, "medium": 0, "mobile": 0, "width": 0.0}
    for u in units:
        c = fm.unit_classes(game, u)
        if "support" in c:
            continue
        out["battalions"] += 1
        out["width"] += (ut.get(u) or {}).get("width") or 0.0
        out["artillery"] += "artillery" in c
        out["armour"] += "armour" in c
        out["mobile"] += "mobile" in c
        out["medium"] += u in HEAVY
    return out


def ai_templates(tag, day):
    """The game's own AI's line and armour templates for the tag on the newest baseline date up to day."""
    here = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
    best = None
    for path in sorted(glob.glob(os.path.join(here, "skills", "hoi4-campaign-playbook", "baselines", "*.json"))):
        try:
            with open(path, encoding="utf-8") as fh:
                b = json.load(fh)
        except (OSError, ValueError):
            continue
        if b.get("tag") != tag:
            continue
        for row in b.get("templates") or []:
            d = derive.day(row.get("date")) if isinstance(row.get("date"), str) else None
            if d is not None and d <= day and (best is None or d > best[0]):
                best = (d, row)
    return best[1] if best else None


def check(ctx):
    lint, tl, g = ctx.lint, ctx.tl, ctx.g
    wars = [w for w in ctx.wars if w["kind"] == "offensive" and w.get("start") is not None
            and "at war at the start" not in w["source"]]
    if not wars:
        return
    from .lint_operations import Ops
    from .lint_war_shape import Shape
    ops = Ops(Shape(lint, tl, ctx.wars))
    facts = []
    for w in wars:
        if not ops.land_targets(w):
            continue
        day = w["start"]
        army = ctx.army(day)
        kinds = {t: ctx.profile(t)["kind"] for t in army}
        where = f"force_design ({w['id']})"
        ai = ai_templates(lint.tag, day) or {}
        row = {"war": w["id"], "date": _fmt(day)}
        line = max((t for t in army if kinds[t] == "line"), key=lambda t: army[t], default=None)
        if line:
            s = shape(g, tl.template_units(line))
            a = shape(g, ai["line"]) if ai.get("line") else None
            row["line"] = dict(s, template=line, divisions=round(army[line]), ai=a)
            floor = max(1, a["artillery"]) if a else 1
            if s["artillery"] < floor:
                lint.add("error", "force.template_floor", where,
                         f"{w['id']} ({_fmt(day)}): the main line template {line} ({round(army[line])} divisions, "
                         f"{s['battalions']} battalions, width {s['width']:g}) has {s['artillery']} line artillery "
                         f"battalion(s)" + (f"; the game's own AI fields {a['artillery']} in a template of "
                                            f"{a['battalions']} battalions, width {a['width']:g}, on {ai.get('date')}"
                                            if a else "")
                         + ". Support artillery alone leaves the division about a quarter short in soft attack: "
                           "add the battalion to the template (templates, xp_budget) and size the artillery line "
                           "to it (production; lint force.readiness)")
        arm = [t for t in army if kinds[t] == "armour"]
        if arm:
            main = max(arm, key=lambda t: army[t])
            s = shape(g, tl.template_units(main))
            a = shape(g, ai["armour"]) if ai.get("armour") else None
            total = sum(army[t] for t in arm)
            med = sum(army[t] for t in arm if shape(g, tl.template_units(t))["medium"])
            row["armour"] = dict(s, template=main, divisions=round(army[main]), medium_divisions=round(med), ai=a)
            can = fm.unit_ready_day(lint, "medium_armor", day)
            problems = []
            if a and s["battalions"] < a["battalions"]:
                problems.append(f"the main armour template {main} has {s['battalions']} battalions "
                                f"({s['armour']} tank, {s['mobile']} motorised or mechanised); the game's own AI "
                                f"fields {a['battalions']} ({a['armour']} tank, {a['medium']} of them medium, "
                                f"{a['mobile']} motorised) on {ai.get('date')}")
            if (can is not None and can <= day or (a and a["medium"])) and med < MEDIUM_SHARE * total:
                problems.append(f"{round(med)} of the {round(total)} armoured divisions carry medium or heavier tanks "
                                f"(at least {MEDIUM_SHARE:.0%} wanted): date the chassis research, the design and "
                                "its line early enough to equip them by the war day")
            if problems:
                lint.add("error", "force.template_floor", where, f"{w['id']} ({_fmt(day)}): " + "; ".join(problems))
        facts.append(row)
    if facts:
        lint.facts.setdefault("force", {})["quality"] = facts
    first = min(w["start"] for w in wars)
    # officer corps and the staff seats
    oc = lint.list_of("officer_corps")
    missing = []
    if not any(str(e.get("slot", "")).startswith("army:") for e in oc):
        missing.append("no officer_corps entry for the army (academy, army and division-command spirits cost army "
                       "experience that otherwise lies idle)")
    if lint.list_of("air_plan") and not any(str(e.get("slot", "")).startswith("air:") for e in oc):
        missing.append("no officer_corps entry for the air force")
    named = {}
    auto = lint.plan.get("politics_auto") if isinstance(lint.plan.get("politics_auto"), dict) else {}
    for e in lint.list_of("politics") + [x for x in auto.get("order") or [] if isinstance(x, dict)]:
        key = (e.get("category"), e.get("slot"))
        has = bool(e.get("idea") or e.get("ideas"))
        d = fm.predicate_day(lint, tl, e.get("when"), ctx.wars) if e.get("when") is not None else tl.start
        if key not in named or (has and not named[key][0]) or (has == named[key][0] and (d or 10 ** 9) < (named[key][1] or 10 ** 9)):
            named[key] = (has, d)
    for cat, slot, what in SEATS:
        has, d = named.get((cat, slot), (None, None))
        if has is None:
            missing.append(f"{what} ({cat}:{slot}): no entry")
        elif not has:
            missing.append(f"{what} ({cat}:{slot}): no advisor named (the first the game offers is taken, of any branch)")
        elif d is None or d > first:
            missing.append(f"{what} ({cat}:{slot}): hired {_fmt(d)}, after the first war day")
    if missing:
        lint.add("error", "force.staff", "officer_corps, politics",
                 f"by the first offensive war ({_fmt(first)}): " + "; ".join(missing))
    xb = lint.plan.get("xp_budget") if isinstance(lint.plan.get("xp_budget"), dict) else {}
    ex = [e for e in xb.get("army") or [] if isinstance(e, dict) and e.get("purpose") == "exercise"]
    days = [fm.predicate_day(lint, tl, e.get("when"), ctx.wars) if e.get("when") is not None else tl.start for e in ex]
    days = [d for d in days if d is not None]
    if not days or min(days) > first - EXERCISE_DAYS:
        lint.add("error", "force.exercise", "xp_budget.army",
                 f"no army exercise entry starts {EXERCISE_DAYS} days before the first offensive war ({_fmt(first)}): "
                 "divisions trained since the start deploy at Trained (+0% in combat) and need about "
                 f"{EXERCISE_DAYS} days of exercise for Regular (+25%)")
