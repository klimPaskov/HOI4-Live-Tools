"""4. Production mix: factories per line by shortfall and time-to-need, the efficiency rules, trains, trucks and convoys.

Inputs, all read:
  common/units, equipment   what each template needs (sub-unit `need`), the newest variant by techs and its
                            build_cost_ic, the parent chain and archetype (for the line-change rule)
  common/defines            BASE_FACTORY_SPEED_MIL, BASE_FACTORY_START_EFFICIENCY_FACTOR, BASE_FACTORY_MAX_EFFICIENCY_FACTOR,
                            BASE_FACTORY_EFFICIENCY_GAIN, BASE_FACTORY_EFFICIENCY_BALANCE_FACTOR, the four
                            BASE_FACTORY_EFFICIENCY_*_CHANGE_FACTOR, MAX_MIL_FACTORIES_PER_LINE, SUPPLY_POINTS_PER_TRAIN,
                            NUM_RAILWAYS_TRAIN_FACTOR, SUPPLY_HUB_FULL_MOTORIZATION_TRUCK_COST, SUPPLY_CONVOY_FACTOR
  modifiers                 industrial_capacity_factory, production_factory_*_efficiency_factor (ideas, techs, live)
  the situation             military factories, the stockpile (snapshot), the fielded army (OOB), the divisions to add
                            (role -> count: templates from evaluator 2), the railway graph from the capital to the front

Output of a line = factories x BASE_FACTORY_SPEED_MIL x (1 + industrial_capacity_factory) x efficiency / 100 IC a day.
Efficiency starts at the start factor (or what a line change keeps) and grows each day by
BALANCE_FACTOR x EFFICIENCY_GAIN x (1 + gain factor) x max^2 / (100 x current) points (the form the wiki gives; the
factors are the defines'), up to the max. Factories per line = the fewest whose output by the need date covers the
shortfall's IC; lines are filled in order of need date, and when the military factories run out the line says by how
much and the date its shortfall would be covered with what is left.

Line change: switching a line keeps the share of its efficiency the defines give for the relation of the old and the
new equipment: the same equipment (a design variant) VARIANT_CHANGE, parent and child PARENT_CHANGE, the same family
(sharing a parent) FAMILY_CHANGE, the same archetype ARCHETYPE_CHANGE, anything else the start efficiency.

Trains: the army's daily supply / SUPPLY_POINTS_PER_TRAIN x NUM_RAILWAYS_TRAIN_FACTOR x the railway distance (in
provinces) from the capital to the front, read from the railway graph. Trucks: the templates' own truck need, plus
SUPPLY_HUB_FULL_MOTORIZATION_TRUCK_COST per hub to motorise; a hub's running truck use is the supply screen's (a live
read: state["logistics"]). Convoys: SUPPLY_CONVOY_FACTOR per supply point sent overseas plus the trade convoys (live).
"""
import math
from collections import deque

from .. import divstats, theatre
from . import design, template


def _first(v, d):
    if isinstance(v, list):
        return v[0] if v else d
    return d if v is None else v


class Efficiency:
    def __init__(self, sit):
        d = lambda k, x: float(_first(sit.define(k), x))  # noqa: E731
        self.speed = d("BASE_FACTORY_SPEED_MIL", 4)
        self.start = d("BASE_FACTORY_START_EFFICIENCY_FACTOR", 10) * (1 + sit.modifier(
            "production_factory_start_efficiency_factor"))
        self.cap = d("BASE_FACTORY_MAX_EFFICIENCY_FACTOR", 50) * (1 + sit.modifier(
            "production_factory_max_efficiency_factor"))
        self.k = d("BASE_FACTORY_EFFICIENCY_BALANCE_FACTOR", 0.1) * d("BASE_FACTORY_EFFICIENCY_GAIN", 1) * (
            1 + sit.modifier("production_factory_efficiency_gain_factor"))
        self.output = 1 + sit.modifier("industrial_capacity_factory")
        self.change = {r: d(f"BASE_FACTORY_EFFICIENCY_{r.upper()}_CHANGE_FACTOR", x) for r, x in
                       (("variant", 90), ("parent", 30), ("family", 70), ("archetype", 20))}
        self.per_line = int(d("MAX_MIL_FACTORIES_PER_LINE", 150))
        self.src = [sit.define_src(k) for k in ("BASE_FACTORY_SPEED_MIL", "BASE_FACTORY_START_EFFICIENCY_FACTOR",
                                                "BASE_FACTORY_MAX_EFFICIENCY_FACTOR",
                                                "BASE_FACTORY_EFFICIENCY_BALANCE_FACTOR")]

    def ic_per_factory(self, days, start=None):
        """IC one factory makes in `days`, from efficiency `start` (default the start factor)."""
        e = self.start if start is None else start
        total = 0.0
        for _ in range(int(days)):
            total += self.speed * self.output * e / 100.0
            e = min(self.cap, e + self.k * self.cap * self.cap / (100.0 * max(e, 1e-6)))
        return total

    def days_to_cap(self, start=None):
        e, n = (self.start if start is None else start), 0
        while e < self.cap - 1e-6 and n < 5000:
            e = min(self.cap, e + self.k * self.cap * self.cap / (100.0 * max(e, 1e-6)))
            n += 1
        return n


def relation(data, old, new):
    """variant | parent | family | archetype | None: how the line-change rule sees old -> new."""
    if old == new:
        return "variant"
    co, cn = data.chain(old), data.chain(new)
    if new in co or old in cn:
        return "parent"
    po = (data.equipment.get(old) or {}).get("parent")
    pn = (data.equipment.get(new) or {}).get("parent")
    if po and po == pn:
        return "family"
    if data.archetype(old) == data.archetype(new):
        return "archetype"
    return None


def kept_efficiency(sit, old, new, current=None, eff=None):
    eff = eff or Efficiency(sit)
    data = sit.cached("divdata", lambda: divstats.Data(sit.game))
    r = relation(data, old, new)
    cur = eff.cap if current is None else current
    if r is None:
        return eff.start, None
    return max(eff.start, cur * eff.change[r] / 100.0), r


def demand(sit, add=None, enemies=None, b=None):
    """{archetype: amount} for the fielded army topped up and the divisions to add ({role: count})."""
    b = b or template.Builder(sit, None, enemies, designs=False)
    oob = sit.world.oob(sit.tag)
    need, supply, rows = {}, 0.0, []
    counts = {}
    for d in oob.get("divisions") or []:
        counts[d.get("template")] = counts.get(d.get("template"), 0) + 1
    for name, n in counts.items():
        bats, sup = divstats.template_units((oob.get("templates") or {}).get(name) or {})
        _, _, eq = b.cost(bats, sup)
        for k, v in eq.items():
            need[k] = need.get(k, 0) + v * n
        supply += n * _supply_use(b, bats + sup)
        rows.append({"template": name, "divisions": n, "source": "fielded"})
    for role, n in (add or {}).items():
        r = template.best(sit, role, enemies=enemies)
        if not r["templates"]:
            continue
        t = r["templates"][0]
        for k, v in t["equipment"].items():
            need[k] = need.get(k, 0) + v * n
        bats = [k for k, c in t["battalions"].items() for _ in range(c)]
        supply += n * _supply_use(b, bats + t["support"] + t["regimental"])
        rows.append({"template": f"{role} (evaluator 2): " + ", ".join(f"{c} {k}" for k, c in t["battalions"].items()),
                     "divisions": n, "source": "added"})
    return need, supply, rows, b


def _supply_use(b, units):
    return sum(float(((b.units.get(x) or {}).get("stats") or {}).get("supply_consumption", 0.0)) for x in units)


def rail_distance(sit, enemy):
    """Railway provinces from the capital to the nearest railway province within two land steps of the front."""
    try:
        g = sit.world.graph() or {}
    except Exception:  # noqa: BLE001 - no map (a fixture): no distance
        return None
    rail = {}
    for line in g.get("railways") or []:
        p = line["provinces"]
        for a, c in zip(p, p[1:]):
            rail.setdefault(a, set()).add(c)
            rail.setdefault(c, set()).add(a)
    caps = theatre._capital_provinces(sit.world, sit.tag)
    if not caps or caps[0] not in rail:
        return None
    dist = {caps[0]: 0}
    q = deque([caps[0]])
    while q:
        x = q.popleft()
        for y in rail.get(x, ()):
            if y not in dist:
                dist[y] = dist[x] + 1
                q.append(y)
    front = theatre.front_provinces(sit.world, sit.tag, enemy) if enemy else set()
    near = set(front)
    for _ in range(2):
        near |= {y for x in near for y in (g.get("adj") or {}).get(x, {})}
    cands = [dist[p] for p in near if p in dist]
    return min(cands) if cands else None


def evaluate(sit, need_date, add=None, enemies=None, lines=None, overseas_supply=0.0):
    from ..world import day_number
    eff = Efficiency(sit)
    need, supply, rows, b = demand(sit, add, enemies)
    stock = {k: float(v) for k, v in (sit.state.get("stockpile") or {}).items()}
    assume_full = not stock
    days = max(0, day_number(need_date) - day_number(sit.date))
    mil = sit.factories()["military"]
    fielded = {}
    if assume_full:  # no stockpile read: the fielded divisions are taken as equipped, the additions are the shortfall
        oob_need, _, _, _ = demand(sit, None, enemies, b)
        fielded = oob_need
    out_lines = []
    left = mil
    for arch in sorted(need, key=lambda a: -need[a]):
        have = fielded.get(arch, 0.0) if assume_full else stock.get(arch, 0.0)
        short = max(0.0, need[arch] - have)
        variant = b.equipment.get(arch)
        cost = (variant.get("build_cost_ic", 0.0) if isinstance(variant, dict) else
                design.Vehicles.of(sit.game).base_stats(variant or arch).get("build_cost_ic", 0.0))
        start = None
        why = "new line"
        if lines and arch in lines:
            old = lines[arch]
            start, rel = kept_efficiency(sit, old.get("equipment"), variant, old.get("efficiency"), eff)
            why = f"switch {old.get('equipment')} -> {variant}: {rel or 'unrelated'} keeps {start:.1f}%"
        per = eff.ic_per_factory(days, start) if days else 0.0
        ic = short * cost
        f = 0 if ic <= 0 else (min(eff.per_line, int(-(-ic // per))) if per > 0 else None)
        row = {"equipment": arch, "variant": variant if isinstance(variant, str) else
               (b.designs.get(arch) or {}).get("base", arch), "need": need[arch],
               "have": have, "shortfall": short, "unit_ic": round(cost, 2), "ic": round(ic, 1), "factories": f,
               "ic_per_factory": round(per, 1), "start": why}
        if f:
            row["assigned"] = min(f, left)
            left -= row["assigned"]
            if row["assigned"] < f:
                row["late"] = f"short by {f - row['assigned']} factories"
        out_lines.append(row)
    rd = rail_distance(sit, (sit.enemies(enemies)[0] or [None])[0])
    spt = float(_first(sit.define("SUPPLY_POINTS_PER_TRAIN"), 1))
    ntf = float(_first(sit.define("NUM_RAILWAYS_TRAIN_FACTOR"), 0.03))
    hub = float(_first(sit.define("SUPPLY_HUB_FULL_MOTORIZATION_TRUCK_COST"), 60))
    scf = float(_first(sit.define("SUPPLY_CONVOY_FACTOR"), 0.25))
    live = sit.state.get("logistics") or {}
    trucks = sum(v for k, v in need.items() if "motorized" in k or "truck" in k)
    logistics = {
        "supply": round(supply, 2), "rail_distance": rd,
        "trains": round(supply / spt * ntf * rd, 1) if rd else None,
        "trains_why": f"{supply:.1f} supply / {spt:g} per train x {ntf:g} x {rd} railway provinces" if rd else
                      "no railway path from the capital to the front",
        "trucks_divisional": trucks, "trucks_per_motorised_hub": hub,
        "convoys": round(scf * overseas_supply + float(live.get("trade_convoys", 0)), 1),
        "live": live,
        "defines": [sit.define_src(k) for k in ("SUPPLY_POINTS_PER_TRAIN", "NUM_RAILWAYS_TRAIN_FACTOR",
                                                "SUPPLY_HUB_FULL_MOTORIZATION_TRUCK_COST", "SUPPLY_CONVOY_FACTOR")]}
    return {"need_date": need_date, "days": days, "military_factories": mil, "unassigned": left,
            "lines": out_lines, "divisions": rows, "logistics": logistics, "stockpile": "snapshot" if not assume_full
            else "none read: the fielded divisions are taken as equipped",
            "efficiency": {"start": round(eff.start, 1), "max": round(eff.cap, 1), "days_to_max": eff.days_to_cap(),
                           "output": round(eff.output, 3), "change": eff.change, "defines": eff.src}}


TRAINING_METHOD = (
    "in training at once N = min(rate, equipment, manpower, training cap): T = the template's training days (the "
    "longest training_time of its sub-units x (1 + training_time_factor + training_time_army_factor)); rate = "
    "ceil(divisions still wanted / (days to the need date // T)) (all of them when one training period is left); "
    "equipment = min over the equipment it needs of floor((stock + daily output x T) / need per division), the daily "
    "output from the live lines or the history's lines on hand (factories x the efficiency curve's mean over T / unit "
    "IC; the fielded divisions' own shortfall is not taken first [?]); "
    "manpower = floor(free manpower / manpower per division), free = live, else owned states' manpower x "
    "conscription x (1 + conscription_factor) less the fielded divisions' manpower [?: the game adds recruits "
    "monthly]; cap = floor((max(MIN_POSSIBLE_TRAINING_MANPOWER, MIN_FIELD_TO_TRAINING_MANPOWER_RATIO x field "
    "manpower) - manpower already in training) / manpower per division). One deployment line carries the N; more "
    "lines only wait for the same equipment.")


def _template_row(sit, role, enemies, b):
    """(name, battalions [unit...], support [unit...], equipment need {arch: n}, manpower) of a role or a template."""
    oob = sit.world.oob(sit.tag)
    tpls = oob.get("templates") or {}
    if role in tpls:
        bats, sup = divstats.template_units(tpls[role])
        _, mp, need = b.cost(bats, sup)
        return role, bats, sup, need, mp
    r = template.best(sit, role, enemies=enemies)
    if not r["templates"]:
        return None
    t = r["templates"][0]
    bats = [k for k, c in t["battalions"].items() for _ in range(c)]
    sup = list(t["support"]) + list(t.get("regimental") or [])
    name = f"{role} (evaluator 2): " + ", ".join(f"{c} {k}" for k, c in t["battalions"].items())
    return name, bats, sup, dict(t["equipment"]), float(t["manpower"])


def training(sit, role, count=None, need_date=None, enemies=None):
    """How many divisions of one template to keep in training at once, from stock, production and manpower on hand
    (TRAINING_METHOD). role: an evaluator-2 role (line, attacker, armour, ...) or a fielded template's name; count:
    the divisions still wanted by need_date (None: as many as the means allow)."""
    from ..world import day_number
    b = template.Builder(sit, None, enemies, designs=False)
    row = _template_row(sit, role, enemies, b)
    if row is None:
        return {"role": role, "error": "no template of that role or name"}
    name, bats, sup, need, mp = row
    units = sit.world.sub_units()
    days_base = max([float((units.get(u) or {}).get("training_time") or 0) for u in bats + sup] or [0.0])
    tmod = sit.modifier("training_time_factor") + sit.modifier("training_time_army_factor")
    T = max(1.0, days_base * (1 + tmod))
    days_left = max(0, day_number(need_date) - day_number(sit.date)) if need_date else None
    terms, why = {}, {}
    # rate: the divisions still wanted, spread over the training periods left
    if count:
        periods = max(1, int(days_left // T)) if days_left else 1
        terms["rate"] = min(count, math.ceil(count / periods))
        why["rate"] = f"ceil({count} / {periods} training periods of {T:g} days to {need_date})"
    # equipment: stock + output over one training period, per division
    stock = {k: float(v) for k, v in (sit.state.get("stockpile") or {}).items()}
    live_out = sit.state.get("production_output") or {}
    out_day, out_from = {}, ""
    if live_out:
        out_day = {k: float(v) for k, v in live_out.items()}
        out_from = "live lines (units a day)"
    else:
        # production on hand: the country's lines now (history start lines), at the efficiency curve's mean over T
        eff = Efficiency(sit)
        veh = design.Vehicles.of(sit.game)
        for ln in sit.world.oob(sit.tag).get("production") or []:
            ic = float(veh.base_stats(ln["type"]).get("build_cost_ic") or 0.0)
            if ic > 0 and ln.get("factories"):
                arch = b.data.archetype(ln["type"])
                out_day[arch] = out_day.get(arch, 0.0) + ln["factories"] * eff.ic_per_factory(T) / T / ic
        out_from = (f"the production lines on hand ({sit.world.date} history lines; efficiency from "
                    f"{eff.start:g}% over {T:g} days)")
    eq = {}
    for arch, n in need.items():
        if n <= 0:
            continue
        have = stock.get(arch, 0.0) + out_day.get(arch, 0.0) * T
        eq[arch] = {"need": n, "stock": stock.get(arch, 0.0), "per_day": round(out_day.get(arch, 0.0), 2),
                    "divisions": round(have / n, 2)}
    flow = None
    if eq:
        bind = min(eq, key=lambda k: eq[k]["divisions"])
        flow = eq[bind]["divisions"]
        terms["equipment"] = int(flow)
        why["equipment"] = (f"{bind}: ({eq[bind]['stock']:g} stock + {eq[bind]['per_day']:g} a day x {T:g} days) / "
                            f"{eq[bind]['need']:g} per division")
    # manpower on hand and the training cap
    fielded = 0.0
    oob = sit.world.oob(sit.tag)
    for d in oob.get("divisions") or []:
        bb, ss = divstats.template_units((oob.get("templates") or {}).get(d.get("template")) or {})
        fielded += b.cost(bb, ss)[1]
    field_mp = float(sit.state.get("field_manpower") or fielded)
    if isinstance(sit.state.get("manpower"), (int, float)):
        free, free_from = float(sit.state["manpower"]), "live snapshot"
    else:
        pop = sum(float(sit.world.states()[s].get("manpower") or 0) for s in sit.world.owned_states(sit.tag))
        rec = pop * sit.modifier("conscription") * (1 + sit.modifier("conscription_factor"))
        free = max(0.0, rec - fielded)
        free_from = (f"{pop:,.0f} population x conscription {sit.modifier('conscription'):g} x (1 + "
                     f"{sit.modifier('conscription_factor'):g}) - {fielded:,.0f} fielded (estimate)")
    if mp > 0:
        terms["manpower"] = int(free // mp)
        why["manpower"] = f"{free:,.0f} free ({free_from}) / {mp:g} per division"
        lo, ratio = sit.define("MIN_POSSIBLE_TRAINING_MANPOWER"), sit.define("MIN_FIELD_TO_TRAINING_MANPOWER_RATIO")
        if lo is not None or ratio is not None:  # a mod without the training cap has none
            lo, ratio = float(_first(lo, 0)), float(_first(ratio, 0))
            busy = float(sit.state.get("training_manpower") or 0)
            cap = max(lo, ratio * field_mp) - busy
            terms["cap"] = max(0, int(cap // mp))
            why["cap"] = (f"(max({lo:,.0f}, {ratio:g} x {field_mp:,.0f} in the field) - {busy:,.0f} in training) / "
                          f"{mp:g}")
    n = min(terms.values()) if terms else 0
    binding = [k for k, v in terms.items() if v == n]
    out = {"template": name, "role": role, "training_days": round(T, 1), "training_days_why":
           f"longest sub-unit training_time {days_base:g} x (1 + {tmod:g})", "count": count, "need_date": need_date,
           "in_training": n, "binding": binding, "terms": terms, "why": why, "equipment": eq, "output_from": out_from,
           "lines": 1 if n else 0, "method": TRAINING_METHOD,
           "defines": [sit.define_src(k) for k in ("MIN_POSSIBLE_TRAINING_MANPOWER",
                                                   "MIN_FIELD_TO_TRAINING_MANPOWER_RATIO")]}
    if days_left is not None:
        # the binding equipment keeps flowing: its fractional divisions per period add up over the periods left
        per = n if flow is None or n < terms.get("equipment", n) else flow
        out["fielded_by_need_date"] = min(count or 10 ** 9, int(per * max(1, int(days_left // T))))
    if n == 0 and binding:
        out["note"] = ("nothing to train yet: " + ", ".join(binding) + " binds (" + "; ".join(why[k] for k in binding)
                       + ")")
    return out
