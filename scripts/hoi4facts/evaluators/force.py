"""6. Force needed against an enemy as our country sees it, and the land doctrine paths compared.

The enemy is what the files show (FAIR: history/units, the enemy's factories and techs at the start): its divisions
and its most fielded template (evaluator 2 builds it with the enemy's equipment), grown to the war date by its own
military factories at the start (the efficiency curve of evaluator 4 over the days, at the template's IC), said so.

Front: our provinces on the land border with the enemy (theatre.front_provinces); the enemy spreads its divisions over
them. Per province the attacker brings at most the battle width (common/terrain combat_width + the extra width of
the directions, theatre.width_target) of divisions.

  hold      a province holds when our defending width there outlasts the attackers' width:
            our width >= their width per province / R_def (R from evaluator 2's battle model per width), at least one
            division; divisions = provinces x ceil(that width / our line template width)
  attack    each axis fills its battle width with our best attacking template (armour when its R_att is higher):
            ceil(battle width / template width) divisions, and one more such wave for each time the defender's
            reserve per axis (their divisions not on the line, spread over the axes) refills the width
  axes      the enemy's states on the border (each a direction), unless given
Doctrines (common/doctrines/grand_doctrines, folder land): each grand doctrine's own country modifiers (planning_speed,
max_planning, army speed ...) and unit blocks, with its milestones, applied to our line and armour templates: the
change in R and in the planning modifiers, side by side.
"""
import math

from .. import divstats, theatre
from . import design, production, template


def _first(v, d):
    if isinstance(v, list):
        return v[0] if v else d
    return d if v is None else v


def enemy_army(sit, enemy, war_date=None):
    from ..world import day_number
    other = sit.other(enemy)
    oob = other.world.oob(enemy)
    n = len(oob.get("divisions") or [])
    out = {"tag": enemy, "divisions_start": n, "grown": 0, "how": "start OOB (history/units)"}
    if war_date:
        days = max(0, day_number(war_date) - day_number(sit.world.date))  # from the history's date
        mil = other.factories()["military"]
        b = template.Builder(other, None, [sit.tag], designs=False)
        prof = design.enemy_profile(sit, [enemy])
        st, _ = template.enemy_division(template.Builder(sit, prof, [enemy], designs=False))
        ic = 0.0
        rows = [d for d in prof.get("divisions") or [] if d.get("tag") == enemy and d.get("stats")]
        if rows:
            best = max(rows, key=lambda d: d.get("fielded", 0))
            tpl = (oob.get("templates") or {}).get(best["template"]) or {}
            bats, sup = divstats.template_units(tpl)
            ic, _, _ = b.cost(bats, sup)
        eff = production.Efficiency(other)
        made = mil * eff.ic_per_factory(days) if days else 0.0
        out["grown"] = int(made // ic) if ic else 0
        out["how"] = (f"start OOB + {mil:g} military factories x {days} days (efficiency curve) / {ic:g} IC per "
                      f"division = {out['grown']} more")
    out["divisions"] = n + out["grown"]
    return out


def needed(sit, enemy, war_date=None, axes=None, line=None, attackers=None):
    tags = [enemy]
    line = line or template.best(sit, "line", enemies=tags)
    att_r = attackers or [template.best(sit, "attacker", enemies=tags), template.best(sit, "armour", enemies=tags)]
    b = template.Builder(sit, None, tags, designs=False)
    est, ewhy = template.enemy_division(b)
    army = enemy_army(sit, enemy, war_date)
    try:
        front = sorted(theatre.front_provinces(sit.world, sit.tag, enemy))
        tw = theatre.terrain_width(sit.world, front) if front else None
    except Exception:  # noqa: BLE001 - no map: no front
        front, tw = [], None
    if not front or not line["templates"] or est is None:
        return {"enemy": army, "error": "no land front or no template", "front_provinces": len(front)}
    shares = (tw or {}).get("mix", {}).get("shares") or {}
    bw = theatre.width_target(sit.world, shares or {"plains": 1.0}, candidates=[10])["battle_widths"]
    # one direction: the battle width of each terrain, weighted by the front's terrain mix
    battle_w = round(sum(sh * float((bw.get(t) or {}).get("1", 0)) for t, sh in shares.items())) if shares else 0
    lt = line["templates"][0]
    r_def = lt["parts"].get("R") or 1.0
    per_prov_div = army["divisions"] / max(1, len(front))
    their_w = min(battle_w, per_prov_div * est.get("width", 1.0)) if battle_w else per_prov_div * est.get("width", 1.0)
    need_w = their_w / max(r_def, 1e-6)
    hold_each = max(1, math.ceil(need_w / max(lt["width"], 1.0)))
    hold = hold_each * len(front)
    if axes is None:
        theirs = set(sit.world.owned_states(enemy))
        prov_state = {p: s for s in theirs for p in (sit.world.states()[s].get("provinces") or [])}
        border = set()
        for a in sit.world.owned_states(sit.tag):
            for bst, pairs in sit.world.state_borders().get(a, {}).items():
                if bst in theirs:
                    border.add(bst)
        axes = max(1, len(border))
    best_att = max((r["templates"][0] for r in att_r if r["templates"]), key=lambda t: t["parts"].get("R", 0))
    wave = math.ceil((battle_w or best_att["width"]) / max(best_att["width"], 1.0))
    on_line = min(army["divisions"], len(front) * max(1, math.ceil(per_prov_div)))
    reserve = max(0, army["divisions"] - on_line)
    refills = math.ceil(reserve * est.get("width", 1.0) / max(1, axes) / max(battle_w or 1, 1))
    attack = axes * wave * (1 + refills)
    ours = len(sit.world.oob(sit.tag).get("divisions") or [])
    return {"enemy": army, "enemy_template": ewhy, "front_provinces": len(front), "terrain": shares,
            "battle_width": battle_w, "line_template": lt["battalions"], "line_width": lt["width"], "R_def": r_def,
            "their_width_per_province": round(their_w, 1), "hold_per_province": hold_each, "hold": hold,
            "axes": axes, "attack_template": best_att["battalions"], "attack_width": best_att["width"],
            "R_att": best_att["parts"].get("R"), "wave": wave, "refills": refills, "attack": attack,
            "total": hold + attack, "ours": ours, "missing": max(0, hold + attack - ours)}


def grand_doctrines(sit, folder="land"):
    def load():
        from ..script import get1, parse_file
        data = sit.cached("divdata", lambda: divstats.Data(sit.game))
        out = {}
        for p in sit.game.files("common/doctrines/grand_doctrines"):
            for name, body in parse_file(p):
                if not name or not isinstance(body, list) or get1(body, "folder") != folder:
                    continue
                mods, ms_mods, ms_units = {}, {}, []
                for k, v in body:
                    if isinstance(v, str) and k != "xp_cost":
                        try:
                            mods[k] = mods.get(k, 0.0) + float(v)
                        except ValueError:
                            pass
                for k, v in body:
                    if k == "milestones" and isinstance(v, list):
                        for _, m in v:
                            if not isinstance(m, list):
                                continue
                            for mk, mv in m:
                                try:
                                    ms_mods[mk] = ms_mods.get(mk, 0.0) + float(mv)
                                except (TypeError, ValueError):
                                    pass
                            ms_units += data._unit_blocks(m)
                out[name] = {"modifiers": mods, "units": data._unit_blocks(body), "milestone_modifiers": ms_mods,
                             "milestone_units": ms_units, "src": sit.world.rel(p)}
        return out
    return sit.game.cached("eval:grand:" + folder, load)


def doctrines(sit, enemies=None, folder="land"):
    """Each grand doctrine: R of our line and armour templates with it (base, and with every milestone)."""
    tags, _ = sit.enemies(enemies)
    b = template.Builder(sit, None, tags, designs=True)
    est, ewhy = template.enemy_division(b)
    roles = {}
    for role in ("line", "armour"):
        r = template.best(sit, role, enemies=tags)
        if r["templates"]:
            t = r["templates"][0]
            roles[role] = ([k for k, n in t["battalions"].items() for _ in range(n)], t["support"] + t["regimental"])
    held = [d for d in sit.doctrines() if d in grand_doctrines(sit, folder)]
    others = [d for d in sit.doctrines() if d not in held]
    base_mods = divstats.factors(b.data, sit.techs(), others)
    rows = []
    for name, g in sorted(grand_doctrines(sit, folder).items()):
        row = {"doctrine": name, "held": name in held, "planning": {k: v for k, v in
               g["modifiers"].items() if "planning" in k}, "planning_milestones": {k: v for k, v in
               g["milestone_modifiers"].items() if "planning" in k}, "other": {k: v for k, v in g["modifiers"].items()
               if "planning" not in k}, "src": g["src"]}
        for role, (bats, sup) in roles.items():
            for label, extra in (("base", g["units"]), ("all milestones", g["units"] + g["milestone_units"])):
                div = divstats.division(b.data, bats, sup, b.equipment, base_mods + extra)
                if est is None or not div:
                    continue
                attacking = role != "line"
                r, _ = template.battle(b.rules, div, est, attacking, b.prof)
                row.setdefault(role, {})[label] = round(r, 3)
        rows.append(row)
    return {"rows": rows, "enemy_template": ewhy, "roles": {k: v[0][:1] + [len(v[0])] for k, v in roles.items()},
            "held": held}
