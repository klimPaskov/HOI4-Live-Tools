"""2. Division templates by role for a width target from the theatre's terrain, from battalion stats as of the date.

Roles: line (holds a front), attacker (infantry offensive), armour (breakthrough and exploitation), special (special
forces to the cap), garrison (occupation). Inputs, all read:
  common/units                 sub-units: row (battalion / divisional support / regimental support), group, width,
                               manpower, need, special_forces, allowed_battalion_groups, unlocked_by (techs) or active
  common/units/equipment       the newest variant each researched technology enables (divstats.Data.newest); a
                               designer chassis takes design.best (evaluator 1) for its family
  common/technologies, doctrines  unit and category factors (divstats.factors)
  common/terrain + the map     the theatre's terrain mix (theatre.front_provinces against the enemy) and
                               theatre.width_target (battle widths, the stacking defines): the candidate widths
  common/defines               REGIMENTAL_SUPPORT_REQUIRED_BATTALIONS, MAX_REGIMENTAL_SUPPORT_WIDTH,
                               MAX_DIVISION_BRIGADE_WIDTH / HEIGHT, MAX_DIVISION_SUPPORT_HEIGHT, BASE_CHANCE_TO_AVOID_HIT,
                               CHANCE_TO_AVOID_HIT_AT_NO_DEF, LAND_COMBAT_ORG_DICE_SIZE (+ the armour dice),
                               LAND_COMBAT_ORG_DAMAGE_MODIFIER, the piercing thresholds, SPECIAL_FORCES_CAP_*,
                               MAXIMUM_GARRISON_HARDNESS_WHEN_ATTACKED

Regimental row (land.md): a regimental support company needs a column of REGIMENTAL_SUPPORT_REQUIRED_BATTALIONS
battalions; columns hold one battalion group, at most MAX_DIVISION_BRIGADE_HEIGHT each and MAX_DIVISION_BRIGADE_WIDTH
in all, so the slots are min(MAX_REGIMENTAL_SUPPORT_WIDTH, sum over groups of floor(n / required)).

Battle model (per width of frontage, so different division widths compare fairly): a side's hits on the other =
miss(attacks, the other's defence or breakthrough): attacks up to that stat hit with 1 - BASE_CHANCE_TO_AVOID_HIT,
the rest with 1 - CHANCE_TO_AVOID_HIT_AT_NO_DEF; attacks = soft x (1 - their hardness) + hard x their hardness, cut by
the piercing threshold against their armour; each hit removes the mean organisation die (the armour die when our
armour beats their piercing) x LAND_COMBAT_ORG_DAMAGE_MODIFIER. Endurance = organisation / organisation lost per
hour. R = our endurance / theirs against the enemy's most fielded infantry template scaled to the same frontage: the
line role defends (our defence against their attack, their breakthrough against ours), the attacker and armour roles
attack (our breakthrough against their attack). Score = R x sqrt(width / IC): a better line per factory. Terrain,
planning, generals, experience and air are the battle's, not the template's, and are left out on both sides.

Armour (a thrust is judged by how fast it takes ground, not by how long it outlasts the line): the score is provinces
taken per day x sqrt(width / IC), from the axis it attacks along (axis()):
  supply     the division's supply use (divstats: battalions' supply_consumption x (1 + companies'
             supply_consumption_factor), at least MIN_SUPPLY_CONSUMPTION) x the divisions that fill one battle (the
             axis' battle width / the template's width) against what the axis delivers: a live read
             (state["logistics"]["axis_supply"]) or, from the defines, the flow one province from a fully motorised hub
             (NODE_INITIAL_SUPPLY_FLOW - NODE_STARTING_PENALTY_PER_PROVINCE) x SUPPLY_HUB_FULL_MOTORIZATION_BONUS for each
             province the attack is mounted from [?: the attack directions of the width rule; the live read replaces
             it]. Supply ratio s = min(1, delivered / used); the shortfall (1 - s) scales COMBAT_SUPPLY_LACK_ATTACKER_ATTACK
             on our attacks, COMBAT_SUPPLY_LACK_ATTACKER_DEFEND on our breakthrough and OUT_OF_SUPPLY_SPEED on our speed
             [?: linear in the shortfall]
  battle     their endurance in hours under our attack (the battle model above, with the supply cuts); a thrust whose R is
             below 1 does not break the line and scores below every one that does
  march      province spacing on the axis (centroid distance x MAP_SCALE_PIXEL_TO_KM) x the terrain's movement_cost /
             (division speed x (1 + the battalions' mean terrain movement modifier)); the division moves at its slowest
             sub-unit, so every tank design is tried at each speed floor from the infantry's up to the motorised
             partner's (design.best_land(floor=...))
  advance    24 / (battle hours + march hours) provinces a day at the line of contact; exploitation behind it is the
             march alone (printed beside it)
Fuel (equipment fuel_consumption x amount, the files' figure) is printed for the fuel ledger, not scored.
"""
import math

from .. import divstats, mapgraph, theatre
from . import design

ROLES = ("line", "attacker", "armour", "special", "garrison")
TIE = 0.03  # widths whose terrain fill is within this of the best are equal candidates
LINE_GROUPS = ("infantry",)


def _first(v, default):
    if isinstance(v, list):
        return v[0] if v else default
    return default if v is None else v


class Rules:
    def __init__(self, sit):
        d = lambda k, x: float(_first(sit.define(k), x))  # noqa: E731
        self.required = int(d("REGIMENTAL_SUPPORT_REQUIRED_BATTALIONS", 3))
        self.reg_width = int(d("MAX_REGIMENTAL_SUPPORT_WIDTH", 5))
        self.cols = int(d("MAX_DIVISION_BRIGADE_WIDTH", 5))
        self.height = int(d("MAX_DIVISION_BRIGADE_HEIGHT", 5))
        self.support = int(d("MAX_DIVISION_SUPPORT_HEIGHT", 5)) * int(d("MAX_DIVISION_SUPPORT_WIDTH", 1))
        self.hit_def = 1 - d("BASE_CHANCE_TO_AVOID_HIT", 90) / 100.0
        self.hit_nodef = 1 - d("CHANCE_TO_AVOID_HIT_AT_NO_DEF", 60) / 100.0
        self.org_die = d("LAND_COMBAT_ORG_DICE_SIZE", 4)
        self.org_die_armour = d("LAND_COMBAT_ORG_ARMOR_ON_SOFT_DICE_SIZE", self.org_die)
        self.org_mod = d("LAND_COMBAT_ORG_DAMAGE_MODIFIER", 0.053)
        self.sf_base = d("SPECIAL_FORCES_CAP_BASE", 0.05)
        self.sf_min = d("SPECIAL_FORCES_CAP_MIN", 24)
        self.garrison_hard = d("MAXIMUM_GARRISON_HARDNESS_WHEN_ATTACKED", 0.9)
        self.min_supply = d("MIN_SUPPLY_CONSUMPTION", 0.0)
        self.lack_attack = d("COMBAT_SUPPLY_LACK_ATTACKER_ATTACK", 0.0)
        self.lack_defend = d("COMBAT_SUPPLY_LACK_ATTACKER_DEFEND", 0.0)
        self.lack_speed = d("OUT_OF_SUPPLY_SPEED", 0.0)
        self.node_flow = d("NODE_INITIAL_SUPPLY_FLOW", 0.0)
        self.node_start = d("NODE_STARTING_PENALTY_PER_PROVINCE", 0.0)
        self.motor_bonus = d("SUPPLY_HUB_FULL_MOTORIZATION_BONUS", 1.0)
        self.km_per_pixel = d("MAP_SCALE_PIXEL_TO_KM", 0.0)
        self.combat = design.Combat(sit)
        self.src = [sit.define_src(k) for k in (
            "REGIMENTAL_SUPPORT_REQUIRED_BATTALIONS", "MAX_REGIMENTAL_SUPPORT_WIDTH", "BASE_CHANCE_TO_AVOID_HIT",
            "CHANCE_TO_AVOID_HIT_AT_NO_DEF", "LAND_COMBAT_ORG_DAMAGE_MODIFIER")]
        self.logistic_src = [sit.define_src(k) for k in (
            "MIN_SUPPLY_CONSUMPTION", "COMBAT_SUPPLY_LACK_ATTACKER_ATTACK", "COMBAT_SUPPLY_LACK_ATTACKER_DEFEND",
            "OUT_OF_SUPPLY_SPEED", "NODE_INITIAL_SUPPLY_FLOW", "NODE_STARTING_PENALTY_PER_PROVINCE",
            "SUPPLY_HUB_FULL_MOTORIZATION_BONUS", "MAP_SCALE_PIXEL_TO_KM")]

    def slots(self, counts_by_group):
        if sum(-(-n // self.height) for n in counts_by_group.values() if n) > self.cols:
            return None
        return min(self.reg_width, self.cols, sum(n // self.required for n in counts_by_group.values()))


def available_units(sit):
    """{sub-unit: row} the country can field at the date (active, or unlocked by a researched technology)."""
    techs = sit.techs()
    out = {}
    for k, r in sit.world.sub_units().items():
        if r.get("row") not in ("battalion", "divisional support", "regimental support"):
            continue
        if r.get("active") or any(u.get("tech") in techs for u in r.get("unlocked_by") or []):
            out[k] = r
    return out


class Builder:
    """Battalion data, equipment and modifiers of one situation, shared by every template evaluated."""

    def __init__(self, sit, prof=None, enemies=None, designs=True):
        self.sit = sit
        self.data = sit.cached("divdata", lambda: divstats.Data(sit.game))
        self.units = available_units(sit)
        self.techs = sit.techs()
        self.mods = divstats.factors(self.data, self.techs, sit.doctrines())
        self.rules = Rules(sit)
        self.prof = prof or design.enemy_profile(sit, enemies)
        self.equipment, self.designs, self.variants = {}, {}, {}
        archs = {self.data.archetype(n) for k in self.units for n in (self.data.units.get(k) or {}).get("need", {})}
        v = design.Vehicles.of(sit.game)
        tanks = []
        for arch in sorted(archs):
            if v.equip.get(arch, {}).get("slots") and "armor" in v.equip[arch]["type"]:
                tanks.append(arch)  # a designer chassis fields a design, not its bare chassis variant (evaluator 1)
                continue
            new = self.data.newest(arch, self.techs)
            if new:
                self.equipment[arch] = new
        if designs:
            for arch in tanks:
                r = design.best_land(sit, "tank_main", families={arch}, top=1, prof=self.prof,
                                     floor=self.speed_floors())
                for floor, ds in r["by_floor"]:
                    if not ds:
                        break  # a higher floor cannot be met either
                    d = dict(ds[0], floor=floor)
                    if all(d["stats"] != x["stats"] for x in self.variants.get(arch, [])):
                        self.variants.setdefault(arch, []).append(d)
                if self.variants.get(arch):
                    self.use(arch, 0)

    def speed_floors(self):
        """km/h floors for tank designs: the infantry's (design.floor_speed), then each whole km/h up to the fastest
        mobile battalion the country fields beside its tanks (its transport's maximum_speed)."""
        if hasattr(self, "_floors"):
            return self._floors
        lo, _ = design.floor_speed(self.sit)
        hi = lo
        for k, r in self.units.items():
            if r.get("row") == "battalion" and r.get("group") == "mobile" and not r.get("special_forces"):
                div = self.division([k], [])
                hi = max(hi, (div or {}).get("speed", 0.0))
        self._floors = [lo] + [float(x) for x in range(int(lo) + 1, int(hi) + 1)]
        return self._floors

    def use(self, arch, i):
        """Field design variant i of a tank chassis family (the speed floors give one variant each)."""
        d = self.variants[arch][i]
        self.equipment[arch] = dict(d["stats"])
        self.designs[arch] = d

    def fieldable(self, unit):
        u = self.data.units.get(unit)
        return bool(u) and all(self.data.archetype(n) in self.equipment for n in u["need"])

    def division(self, bats, support):
        return divstats.division(self.data, bats, support, self.equipment, self.mods)

    def cost(self, bats, support):
        ic, mp, need = 0.0, 0.0, {}
        v = design.Vehicles.of(self.sit.game)
        for b in list(bats) + list(support):
            u = self.data.units.get(b) or {}
            mp += float((self.units.get(b) or {}).get("manpower") or 0)
            for n, amount in u.get("need", {}).items():
                arch = self.data.archetype(n)
                pick = self.equipment.get(arch)
                each = pick.get("build_cost_ic", 0.0) if isinstance(pick, dict) else \
                    v.base_stats(pick or n).get("build_cost_ic", 0.0)
                ic += each * amount
                need[arch] = need.get(arch, 0) + amount
        return round(ic, 1), mp, need


def battle(rules, ours, theirs, attacking, prof):
    """R = our endurance / theirs per width of frontage (see the module doc), with the parts."""
    c = rules.combat

    def attacks(a, b, h):
        raw = a.get("soft_attack", 0.0) * (1 - h) + a.get("hard_attack", 0.0) * h
        return raw * c.pierce(a.get("piercing", 0.0), b.get("armor", 0.0))

    def hits(att, stat):
        return rules.hit_def * min(att, stat) + rules.hit_nodef * max(0.0, att - stat)

    def die(a, b):
        n = rules.org_die_armour if a.get("armor", 0.0) > b.get("piercing", 0.0) else rules.org_die
        return (n + 1) / 2.0

    wo, wt = max(ours.get("width", 1.0), 1.0), max(theirs.get("width", 1.0), 1.0)
    per = lambda d, k, w: d.get(k, 0.0) / w  # noqa: E731
    our_att = attacks(ours, theirs, theirs.get("hardness", 0.0)) / wo
    their_att = attacks(theirs, ours, ours.get("hardness", 0.0)) / wt
    our_block = per(ours, "breakthrough" if attacking else "defense", wo)
    their_block = per(theirs, "defense" if attacking else "breakthrough", wt)
    on_us = hits(their_att, our_block) * die(theirs, ours) * rules.org_mod
    on_them = hits(our_att, their_block) * die(ours, theirs) * rules.org_mod
    ours_end = per(ours, "organisation", wo) / max(on_us, 1e-6)
    theirs_end = per(theirs, "organisation", wt) / max(on_them, 1e-6)
    return ours_end / max(theirs_end, 1e-6), {"our_hours": round(ours_end, 1), "their_hours": round(theirs_end, 1),
                                              "our_attacks_per_width": round(our_att, 2),
                                              "their_attacks_per_width": round(their_att, 2)}


def enemy_division(b):
    rows = [d for d in b.prof.get("divisions") or [] if d.get("stats")]
    if not rows:
        return None, None
    best = max(rows, key=lambda d: (d.get("fielded", 0), d["stats"].get("defense", 0)))
    return best["stats"], f"{best['tag']} {best['template']} (fielded {best.get('fielded', 0)})"


def width_candidates(sit, enemies, width=None):
    """([widths], why): the given width, or the theatre's width_target ties (equal fill within 2%) against the
    first enemy, from the map; no map: the widths of the plains battle."""
    if width:
        return [int(width)], "given"
    tags, _ = sit.enemies(enemies)
    try:
        fp = theatre.front_provinces(sit.world, sit.tag, tags[0]) if tags else None
        tw = theatre.terrain_width(sit.world, sorted(fp)) if fp else None
    except Exception:  # noqa: BLE001 - no map graph (a fixture, a broken mod map): fall back below
        tw = None
    if tw and tw.get("width") and tw["width"].get("target"):
        w = tw["width"]
        shares = tw["mix"]["shares"]
        fills = {c: theatre.width_target(sit.world, shares, candidates=[c])["detail"]["mix_score"]
                 for c in theatre.WIDTH_CANDIDATES}
        best = max(fills.values())
        cands = sorted(c for c, f in fills.items() if f >= best - TIE)
        return cands, (f"terrain against {tags[0]}: {shares}; best fill {best} at {w['target']}; widths within "
                       f"{TIE:g} of it: {cands}")
    return list(theatre.WIDTH_CANDIDATES), "no map graph: every candidate width"


def _groups(b, bats):
    out = {}
    for x in bats:
        g = (b.units.get(x) or {}).get("group") or "?"
        out[g] = out.get(g, 0) + 1
    return out


def _mains(b, role):
    rows = {k: r for k, r in b.units.items() if r["row"] == "battalion" and b.fieldable(k)}
    if role in ("line", "attacker"):
        main = [k for k, r in rows.items() if r.get("group") in LINE_GROUPS and not r.get("special_forces")]
        sec = [k for k, r in rows.items() if r.get("group") == "combat_support"] if role == "attacker" else []
    elif role == "armour":
        main = [k for k, r in rows.items() if r.get("group") == "armor" and "armor" in (r.get("type") or [])
                and not set(r.get("type") or []) & {"anti_tank", "artillery", "anti_air"}]
        sec = [k for k, r in rows.items() if r.get("group") == "mobile" and not r.get("special_forces")]
    elif role == "special":
        main = [k for k, r in rows.items() if r.get("special_forces") and r.get("group") in LINE_GROUPS]
        sec = []
    else:
        main = [k for k, r in rows.items() if r.get("combat_width", 0) > 0]
        sec = []
    return sorted(main), sorted(sec)


def _score(b, role, div, ic, enemy):
    if role == "garrison":
        sup = div.get("suppression", 0.0)
        loss = 1 - min(div.get("hardness", 0.0), b.rules.garrison_hard)
        return sup / max(ic * (1 + loss), 1.0), {"suppression": sup, "loss_factor": round(loss, 3)}
    if enemy is None:
        val = div.get("defense" if role == "line" else "soft_attack", 0.0) + div.get("breakthrough", 0.0)
        return val / math.sqrt(max(ic, 1.0)), {"note": "no enemy template: stats only"}
    r, parts = battle(b.rules, div, enemy, role != "line", b.prof)
    return r * math.sqrt(max(div.get("width", 1.0), 1.0) / max(ic, 1.0)), dict(parts, R=round(r, 3))


def _suppression(b, bats, support):
    return sum(float(((b.units.get(x) or {}).get("stats") or {}).get("suppression", 0.0)) for x in list(bats) + list(support))


def evaluate_template(b, role, bats, support, regimental, enemy, geo=None):
    div = b.division(bats, list(support) + list(regimental))
    if not div:
        return None
    ic, mp, need = b.cost(bats, list(support) + list(regimental))
    div["suppression"] = _suppression(b, bats, list(support) + list(regimental))
    div["supply"] = round(max(div.get("supply", 0.0), b.rules.min_supply), 3)
    if role == "armour" and enemy is not None and geo:
        score, parts = advance(b, div, enemy, ic, geo, _terrain_move(b, bats, geo))
    else:
        score, parts = _score(b, role, div, ic, enemy)
    return {"battalions": _count(bats), "support": list(support), "regimental": list(regimental),
            "width": div.get("width"), "stats": {k: div[k] for k in divstats.ROWS if k in div},
            "logistics": {k: div[k] for k in divstats.LOGI_ROWS if k in div},
            "ic": ic, "manpower": mp, "equipment": need, "score": round(score, 6), "parts": parts}


def _terrain_move(b, bats, geo):
    """The battalions' mean terrain movement modifier over the axis' terrain mix [?: the division takes the mean]."""
    rows = [(b.data.units.get(x) or {}).get("terrain_move") or {} for x in bats]
    if not rows:
        return 0.0
    shares = geo.get("shares") or {}
    return sum(sh * sum(r.get(t, 0.0) for r in rows) / len(rows) for t, sh in shares.items())


def axis(sit, enemies, rules):
    """The ground a thrust crosses against the first enemy: the enemy's first two province rows behind our land
    border (map graph). Gives the terrain shares, the battle width (common/terrain combat_width + support width per
    extra direction, the mean over one and two directions as the width rule), the movement cost (terrain
    movement_cost by share), the province spacing (mean centroid distance between adjacent provinces of the band x
    MAP_SCALE_PIXEL_TO_KM) and the supply the attack draws (see the module doc). No map: plains-free fallback with
    the march term left out."""
    tags, _ = sit.enemies(enemies)
    w = sit.world
    out = {"enemy": tags[0] if tags else None, "directions": list(theatre.WIDTH_DIRECTIONS)}
    band, g = [], None
    try:
        g = w.graph()
        if tags and g is not None:
            ours = theatre.front_provinces(w, sit.tag, tags[0])
            theirs = set()
            for sid in w.owned_states(tags[0]):
                theirs |= set((g["states"].get(sid) or {}).get("provinces") or [])
            row1 = {nb for p in ours for nb, _ in mapgraph.neighbours(g, p) if nb in theirs}
            how = f"the first two rows behind the border with {tags[0]}"
            if not row1 and theirs:
                # no common land border at the bookmark (a later war, a partition): the enemy's provinces nearest
                # our own land stand in for the first row, as many as the border would have
                row1, how = _nearest(g, w, sit.tag, theirs), f"{tags[0]}'s provinces nearest our land (no common border)"
            row2 = {nb for p in row1 for nb, _ in mapgraph.neighbours(g, p) if nb in theirs} - row1
            band = sorted(row1 | row2)
    except Exception:  # noqa: BLE001 - no map graph (a fixture, a broken mod map): the fallback below
        band = []
    terr = w.terrains()
    if band:
        shares = mapgraph.terrain_mix(g, band)["shares"]
        dists = [mapgraph._xy_dist(g, a, nb) for a in band for nb, _ in mapgraph.neighbours(g, a) if nb in set(band)
                 and a < nb]
        out["km"] = round(sum(dists) / len(dists) * rules.km_per_pixel, 1) if dists and rules.km_per_pixel else None
        out["provinces"] = len(band)
        out["why"] = f"{len(band)} provinces: {how}"
    else:
        flat = [t for t, r in terr.items() if r.get("combat_width") and not r.get("is_water")]
        best = min(flat, key=lambda t: (terr[t].get("movement_cost", 9), -terr[t]["combat_width"]), default=None)
        shares = {best: 1.0} if best else {}
        out["km"] = None
        out["provinces"] = 0
        out["why"] = f"no map graph: {best or 'no'} terrain assumed, march term left out"
    rows = {t: terr[t] for t in shares if t in terr and terr[t].get("combat_width")}
    tot = sum(shares[t] for t in rows) or 1.0
    bw = {t: theatre.battle_widths(r) for t, r in rows.items()}
    out["shares"] = {t: shares[t] for t in rows}
    out["battle_width"] = round(sum(shares[t] * sum(ws.values()) / len(ws) for t, ws in bw.items()) / tot, 1) \
        if rows else None
    out["movement_cost"] = round(sum(shares[t] * float(rows[t].get("movement_cost", 1.0)) for t in rows) / tot, 3) \
        if rows else 1.0
    live = ((sit.state or {}).get("logistics") or {}).get("axis_supply")
    if isinstance(live, (int, float)):
        out["supply"], out["supply_from"] = float(live), "live read (logistics.axis_supply)"
    else:
        per = max(0.0, rules.node_flow - rules.node_start) * rules.motor_bonus
        n = sum(out["directions"]) / len(out["directions"])
        out["supply"] = round(per * n, 2)
        out["supply_from"] = (f"({rules.node_flow:g} - {rules.node_start:g}) x {rules.motor_bonus:g} per province, "
                              f"from {n:g} provinces (the mean attack directions)")
    return out


def _nearest(g, w, tag, theirs, n=None):
    """The enemy land provinces closest (centroid distance) to our provinces; n defaults to a twentieth of theirs."""
    mine = set()
    for sid in w.owned_states(tag):
        mine |= set((g["states"].get(sid) or {}).get("provinces") or [])
    pts = [g["provinces"][p]["xy"] for p in sorted(mine) if (g["provinces"].get(p) or {}).get("xy")]
    pts = pts[::max(1, len(pts) // 300)]
    if not pts:
        return set()
    near = []
    for p in theirs:
        xy = (g["provinces"].get(p) or {}).get("xy")
        if xy and mapgraph.is_land(g, p):
            near.append((min((xy[0] - a) ** 2 + (xy[1] - b) ** 2 for a, b in pts), p))
    near.sort()
    return {p for _, p in near[:n or max(1, len(near) // 20)]}


def advance(b, div, enemy, ic, geo, move_mod=0.0):
    """(score, parts) of an armoured thrust: provinces a day x sqrt(width / IC); see the module doc."""
    r = b.rules
    width = max(div.get("width", 1.0), 1.0)
    n = (geo.get("battle_width") or width) / width
    used = n * div.get("supply", 0.0)
    s = min(1.0, geo["supply"] / used) if used > 0 else 1.0
    short = 1.0 - s
    ours = dict(div)
    for k in ("soft_attack", "hard_attack"):
        ours[k] = div.get(k, 0.0) * max(0.0, 1 + r.lack_attack * short)
    for k in ("breakthrough", "defense"):
        ours[k] = div.get(k, 0.0) * max(0.0, 1 + r.lack_defend * short)
    speed = div.get("speed", 0.0) * max(0.0, 1 + r.lack_speed * short) * max(0.0, 1 + move_mod)
    R, parts = battle(r, ours, enemy, True, b.prof)
    battle_h = parts["their_hours"]
    march_h = geo["km"] * geo["movement_cost"] / speed if geo.get("km") and speed > 0 else 0.0
    per_day = 24.0 / max(battle_h + march_h, 1e-6)
    out = dict(parts, R=round(R, 3), supply_use=round(div.get("supply", 0.0), 3), divisions_per_battle=round(n, 2),
               axis_supply_used=round(used, 2), axis_supply=geo["supply"], supply_ratio=round(s, 3),
               speed=round(speed, 2), battle_hours=round(battle_h, 1), march_hours=round(march_h, 1),
               provinces_per_day=round(per_day, 3),
               exploit_km_per_day=round(24.0 * speed / geo["movement_cost"], 1) if speed > 0 else 0.0)
    if R < 1.0:  # the line holds: no advance; ranked below every thrust that breaks it, by how near it came
        return -(1.0 - R) * 1e-3, dict(out, provinces_per_day=0.0, note="R below 1: the line holds")
    return per_day * math.sqrt(width / max(ic, 1.0)), out


def _count(bats):
    out = {}
    for x in bats:
        out[x] = out.get(x, 0) + 1
    return out


def best(sit, role, width=None, enemies=None, top=3, prof=None):
    if role not in ROLES:
        raise KeyError(f"unknown role {role}: {ROLES}")
    b = Builder(sit, prof, enemies, designs=(role == "armour"))
    widths, wwhy = width_candidates(sit, enemies, width) if role != "garrison" else ([2, 4, 6, 10], "garrison")
    enemy, ewhy = enemy_division(b)
    geo = axis(sit, enemies, b.rules) if role == "armour" else None
    mains, secs = _mains(b, role)
    reg_opts = [k for k, r in b.units.items() if r["row"] == "regimental support" and b.fieldable(k)]
    sup_opts = [k for k, r in b.units.items() if r["row"] == "divisional support" and b.fieldable(k)]
    base = []
    for W in widths:
        for m in mains:
            wm = b.units[m].get("combat_width") or 0
            if not wm:
                continue
            for s in [None] + secs:
                ws = (b.units[s].get("combat_width") or 0) if s else 0
                for ns in (range(0, 6) if s else [0]):
                    left = W - ns * ws
                    nm = int(left // wm)
                    if nm <= 0 or (s and ns == 0):
                        continue
                    if nm + ns > b.rules.cols * b.rules.height:
                        continue
                    if role == "armour" and s and ns > nm * 2:
                        continue
                    bats = [m] * nm + ([s] * ns if s else [])
                    if b.rules.slots(_groups(b, bats)) is None:
                        continue
                    for var in _variants(b, m):
                        t = evaluate_template(b, role, bats, [], [], enemy, geo)
                        if t:
                            t["target_width"] = W
                            t["variant"] = var
                            base.append(t)
    base.sort(key=lambda t: -t["score"])
    out = []
    seen = set()
    tops = []
    for W in widths:  # the best few at every candidate width go on to the support search
        tops += [t for t in base if t["target_width"] == W][:4]
    for t in tops:
        bats = [k for k, n in t["battalions"].items() for _ in range(n)]
        if t.get("variant"):
            b.use(*t["variant"])
        slots = b.rules.slots(_groups(b, bats)) or 0
        groups = set(_groups(b, bats))
        regs = [k for k in reg_opts if not b.units[k].get("allowed_battalion_groups")
                or set(b.units[k]["allowed_battalion_groups"]) & groups]
        cur = t
        regimental, support = [], []
        for _ in range(slots):  # each regimental slot: the company that adds the most
            cand = [evaluate_template(b, role, bats, support, regimental + [k], enemy, geo) for k in regs]
            cand = [c for c in cand if c and c["score"] > cur["score"]]
            if not cand:
                break
            cur = max(cand, key=lambda c: c["score"])
            regimental = cur["regimental"]
        for _ in range(b.rules.support):  # support companies by value, while they add
            cand = [evaluate_template(b, role, bats, support + [k], regimental, enemy, geo) for k in sup_opts
                    if k not in support]
            cand = [c for c in cand if c and c["score"] > cur["score"] * 1.001]
            if not cand:
                break
            cur = max(cand, key=lambda c: c["score"])
            support = cur["support"]
        cur["target_width"] = t["target_width"]
        cur["regimental_slots"] = slots
        cur["designs"] = {a: _design_row(d) for a, d in b.designs.items() if a in cur["equipment"]}
        if t.get("variant"):
            b.use(t["variant"][0], 0)
        key = (tuple(sorted(cur["battalions"].items())), tuple(sorted(cur["support"])), tuple(sorted(cur["regimental"])),
               tuple(sorted((a, d["base"], tuple(sorted(d["modules"].values()))) for a, d in cur["designs"].items())))
        if key not in seen:
            seen.add(key)
            out.append(cur)
    # The width: the terrain gives equal candidates (fill within TIE); among them the widest, because a commander
    # leads at most CORPS_COMMANDER_DIVISIONS_CAP divisions (a wider division holds more frontage per commander)
    # and five regimental slots need five full columns. Templates are ranked at that width, where the per-width
    # battle model compares like with like; the best at the other widths are listed beside it.
    cap = _first(sit.define("CORPS_COMMANDER_DIVISIONS_CAP"), 0)
    for t in out:
        t["frontage_per_commander"] = (cap * t["width"]) if cap else None
    out.sort(key=lambda t: -t["score"])
    chosen = max((t["target_width"] for t in out), default=None) if role != "garrison" else None
    picked = [t for t in out if chosen is None or t["target_width"] == chosen]
    others = {}
    for t in out:
        if t["target_width"] != chosen and t["target_width"] not in others:
            others[t["target_width"]] = t
    res = {"role": role, "templates": picked[:top], "chosen_width": chosen,
           "other_widths": [others[w] for w in sorted(others)],
           "command_rule": sit.define_src("CORPS_COMMANDER_DIVISIONS_CAP"), "widths": widths, "width_why": wwhy, "enemy_template": ewhy,
           "enemy_template_stats": {k: enemy[k] for k in divstats.ROWS if enemy and k in enemy},
           "designs_used": dict(picked[0]["designs"]) if picked and picked[0].get("designs") else
           {a: _design_row(d) for a, d in b.designs.items()},
           "design_variants": {a: [_design_row(d) for d in vs] for a, vs in b.variants.items()},
           "axis": geo, "logistic_rules": b.rules.logistic_src if role == "armour" else [],
           "rules": b.rules.src, "techs": sit.techs_source()}
    if role == "special":
        res["special_forces_cap"] = special_forces_cap(sit, b)
    return res


def _variants(b, unit):
    """[(chassis family, variant index)] to try for a battalion: each speed variant of the tank it needs, else one
    pass with the current equipment ([None])."""
    archs = [b.data.archetype(n) for n in (b.data.units.get(unit) or {}).get("need", {})]
    for arch in archs:
        if b.variants.get(arch):
            return _cycle(b, [(arch, i) for i in range(len(b.variants[arch]))])
    return [None]


def _cycle(b, pairs):
    """Yield each variant with b.equipment set to it (the caller evaluates in between); the first back at the end."""
    for p in pairs:
        b.use(*p)
        yield p
    b.use(pairs[0][0], 0)


def _design_row(d):
    return {"base": d["base"], "modules": d["modules"], "upgrades": d.get("upgrades") or {},
            "speed": d["stats"].get("maximum_speed"), "ic": d["stats"].get("build_cost_ic"), "floor": d.get("floor")}


def special_forces_cap(sit, b=None):
    """max(SPECIAL_FORCES_CAP_MIN, (SPECIAL_FORCES_CAP_BASE + special_forces_cap modifiers) x non-SF battalions) +
    the flat special_forces_min modifiers, over the start army (land.md row 20)."""
    b = b or Builder(sit)
    oob = sit.world.oob(sit.tag)
    tpl = oob.get("templates") or {}
    n = 0
    for d in oob.get("divisions") or []:
        bats, _ = divstats.template_units(tpl.get(d.get("template")) or {})
        n += sum(1 for x in bats if not (sit.world.sub_units().get(x) or {}).get("special_forces"))
    factor = b.rules.sf_base + sit.modifier("special_forces_cap")
    flat = sit.modifier("special_forces_min")
    cap = max(b.rules.sf_min + flat, factor * n)
    return {"cap": round(cap, 1), "non_sf_battalions": n, "factor": factor, "min": b.rules.sf_min + flat}
