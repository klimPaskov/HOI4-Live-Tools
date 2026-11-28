"""Equipment designs by computed stats: one evaluator for the plane, tank and ship designers.

Everything comes from the effective game files (install + mods): the base equipment's stats (its own block over its
archetype's, a duplicate_archetypes family taking its source's), the slots and the categories they allow, the
modules' add_stats / multiply_stats / add_average_stats and their mission_type_stats, the technologies that unlock
bases and modules. Nothing about a country, a mod or a year is written here.

Stat rule (the designer's): total = (base + sum of the modules' add_stats) x (1 + sum of their multiply_stats); an
add_average_stats entry adds the mean over the modules that carry it. A plane needs thrust >= weight. A slot takes a
module whose category it lists. Techs, doctrines, MIO traits and ideas are applied by the game AFTER this; designs of
one country compared with each other share them, a comparison with another country's design leaves them out (the
evaluator says so in its result).

Air combat (the exchange estimate of `duel`): from the NAir defines the game ships: a better speed raises our damage by
up to (BIGGEST_SPEED_FACTOR_DIFF - 1) x COMBAT_BETTER_SPEED_DAMAGE_INCREASE, a better agility cuts the damage we take
by up to (BIGGEST_AGILITY_FACTOR_DIFF - 1) x COMBAT_BETTER_AGILITY_DAMAGE_REDUCTION (linear in the ratio, capped),
damage scales with attack over the target's defence. It ranks designs; it is not the engine's combat.

Roles: ROLES gives each role the stats it values, its weapon category, the equipment type the design must end up as
(planes: the weapon's add_equipment_type), and hard requirements (`min`: e.g. air_range for a theatre). `best`
searches the module space slot by slot (a beam over the slots) under the requirements and returns the top designs.
"""
import itertools

from .script import bare, get, get1, parse_file

STAT_KEYS_PLANE = ("air_attack", "air_defence", "air_agility", "maximum_speed", "air_range", "air_ground_attack",
                   "naval_strike_attack", "naval_strike_targetting", "air_bombing", "air_superiority", "reliability",
                   "build_cost_ic", "weight", "thrust", "fuel_consumption", "surface_detection", "sub_detection")

# role -> (designer kind, archetype families allowed, weapon categories for the main slot, the type the design must
# carry, stat weights (value per point), mission for mission_type_stats)
ROLES = {
    "fighter": {"kind": "plane", "families": ("small_plane_airframe",), "type": "fighter",
                "mission": "air_superiority",
                "weights": {"air_attack": 1.0, "air_agility": 0.6, "maximum_speed": 0.06, "air_defence": 1.0}},
    "heavy_fighter": {"kind": "plane", "families": ("medium_plane_fighter_airframe",), "type": "heavy_fighter",
                      "mission": "air_superiority",
                      "weights": {"air_attack": 1.0, "air_agility": 0.4, "maximum_speed": 0.05, "air_defence": 1.0}},
    "cas": {"kind": "plane", "families": ("small_plane_cas_airframe",), "type": "cas", "mission": "cas",
            "weights": {"air_ground_attack": 1.0, "air_defence": 0.35, "air_agility": 0.05}},
    "naval_bomber": {"kind": "plane", "families": ("small_plane_naval_bomber_airframe",), "type": "naval_bomber",
                     "mission": "naval_bomber",
                     "weights": {"naval_strike_attack": 1.0, "naval_strike_targetting": 0.8, "air_defence": 0.3}},
    "tactical_bomber": {"kind": "plane", "families": ("medium_plane_airframe",), "type": "tactical_bomber",
                        "mission": "strategic_bomber",
                        "weights": {"air_bombing": 1.0, "air_ground_attack": 0.5, "air_defence": 0.4}},
    "strategic_bomber": {"kind": "plane", "families": ("large_plane_airframe",), "type": "strategic_bomber",
                         "mission": "strategic_bomber",
                         "weights": {"air_bombing": 1.0, "air_defence": 0.4}},
    # engineer 125 (G250): the scout plane (the recon camera module adds the scout_plane type and the recon mission):
    # it must live over the enemy's air (speed, agility, defence) and reach the front (range)
    # (slot_type None: the weapon slots keep their own weapons, the camera adds the type; checked on the whole design)
    "recon": {"kind": "plane", "families": ("small_plane_airframe",), "type": "scout_plane", "slot_type": None,
              "mission": "recon",
              "weights": {"maximum_speed": 0.06, "air_agility": 0.6, "air_defence": 1.0, "air_range": 0.002}},
    "tank_medium": {"kind": "tank", "families": ("medium_tank_chassis",), "type": None, "mission": None,
                    "weights": {"soft_attack": 0.6, "hard_attack": 0.8, "breakthrough": 0.5, "armor_value": 0.7,
                                "ap_attack": 0.4, "reliability": 10.0, "maximum_speed": 0.5}},
}


def num(v, default=None):
    try:
        return float(v)
    except (TypeError, ValueError):
        return default


def numbers(block):
    """{key: float} of a flat stat block (non-numbers dropped)."""
    out = {}
    for k, v in block or []:
        f = num(v)
        if k and f is not None:
            out[k] = out.get(k, 0.0) + f
    return out


# ---------------------------------------------------------------- data from the game files

class Catalog:
    """Bases and modules of the effective files. `game` is a hoi4facts.game.Game (files(), cached())."""

    def __init__(self, game):
        self.game = game
        self.equip = self._equipment()
        self.modules = self._modules()

    def _equipment(self):
        raw, dups = {}, []
        for p in self.game.files("common/units/equipment"):
            for k, v in parse_file(p):
                if k not in ("equipments", "duplicate_archetypes") or not isinstance(v, list):
                    continue
                for name, b in v:
                    if not name or not isinstance(b, list):
                        continue
                    row = {"id": name, "archetype": get1(b, "archetype"), "parent": get1(b, "parent"),
                           "is_archetype": get1(b, "is_archetype") == "yes", "type": get1(b, "type"),
                           "year": num(get1(b, "year")), "stats": numbers(b), "slots": _slots(get1(b, "module_slots")),
                           "own_slots": isinstance(get1(b, "module_slots"), list),
                           "derived": get1(b, "derived_variant_name"),
                           "can_be_produced": get1(b, "is_buildable") != "no"}
                    if k == "duplicate_archetypes":
                        vn = get1(b, "variant_name")
                        row["variant_names"] = [x for _, x in vn] if isinstance(vn, list) else []
                        dups.append(row)
                    else:
                        raw[name] = row
        # a duplicated archetype copies its source family (small_plane_cas_airframe_N from small_plane_airframe_N)
        for d in dups:
            src = d["archetype"]
            if src not in raw:
                continue
            raw[d["id"]] = dict(raw[src], id=d["id"], type=d["type"] or raw[src]["type"], is_archetype=True,
                                archetype=None, duplicate_of=src)
            for name, r in list(raw.items()):
                if r.get("archetype") == src and name.startswith(src) and not r.get("duplicate_of"):
                    nid = d["id"] + name[len(src):]
                    # alias, type, variant_name and derived_variant_name are not duplicated (the game's rule)
                    raw.setdefault(nid, dict(r, id=nid, archetype=d["id"], duplicate_of=name, derived=None))
            self.variant_names = getattr(self, "variant_names", set()) | set(d["variant_names"])
        return raw

    def _modules(self):
        out = {}
        for p in self.game.files("common/units/equipment/modules"):
            for k, v in parse_file(p):
                if k != "equipment_modules" or not isinstance(v, list):
                    continue
                for name, b in v:
                    if not name or name == "limit" or not isinstance(b, list):
                        continue
                    mts = []
                    for blk in get(b, "mission_type_stats"):
                        if isinstance(blk, list):
                            lim = get1(blk, "limit")
                            mts.append({"missions": set(bare(lim)) if isinstance(lim, list) else set(),
                                        "add": numbers(get1(blk, "add_stats")),
                                        "mul": numbers(get1(blk, "multiply_stats"))})
                    aet = get1(b, "add_equipment_type")
                    out[name] = {"id": name, "category": get1(b, "category"),
                                 "add": numbers(get1(b, "add_stats")), "mul": numbers(get1(b, "multiply_stats")),
                                 "avg": numbers(get1(b, "add_average_stats")), "mission": mts,
                                 "types": set(bare(aet)) if isinstance(aet, list) else ({aet} if aet else set()),
                                 "allow_types": set(x for blk in get(b, "allow_equipment_type")
                                                    for x in (bare(blk) if isinstance(blk, list) else [blk])),
                                 "resources": numbers(get1(b, "build_cost_resources")),
                                 # a turret opens slots of its own (allowed_module_categories = { main_armament_slot
                                 # = { tank_medium_main_armament } }): slot -> categories it adds
                                 "opens": {n: bare(c) for n, c in (get1(b, "allowed_module_categories") or [])
                                           if n and isinstance(c, list)}
                                 if isinstance(get1(b, "allowed_module_categories"), list) else {},
                                 "xp_cost": num(get1(b, "xp_cost"))}
        return out

    def chain(self, e):
        out, cur = [], e
        while cur in self.equip and cur not in out:
            out.append(cur)
            cur = self.equip[cur].get("archetype")
        return out

    def base_stats(self, e):
        """The base's stats: the archetype's under its own (a duplicate family's from its source)."""
        stats = {}
        for x in reversed(self.chain(e)):
            r = self.equip[x]
            src = self.equip.get(r["duplicate_of"]) if r.get("duplicate_of") else None
            stats.update((src or r)["stats"])
        return stats

    def slots(self, e):
        found = []
        for x in self.chain(e):
            r = self.equip[x]
            s = r["slots"] or (self.equip.get(r.get("duplicate_of"), {}).get("slots") if r.get("duplicate_of") else None)
            if s:
                found.append(s)
        return resolve_slots(found)

    def family(self, e):
        c = self.chain(e)
        return c[-1] if c else e


def _slots(block):
    if not isinstance(block, list):
        return None
    out = []
    for n, b in block:
        if n and isinstance(b, list):
            out.append({"slot": n, "required": get1(b, "required") == "yes",
                        "categories": bare(get1(b, "allowed_module_categories", []))})
        elif n and b == "inherit":
            # `fixed_ship_engine_slot = inherit` (ship_hull_light_2, the Panzerschiff): the slot as the next
            # equipment up the chain defines it
            out.append({"slot": n, "inherit": True})
    return out or None


def resolve_slots(found):
    """The slot list of an equipment from the slot lists along its chain, the equipment's own first: the first
    list wins, and each of its `inherit` slots takes the definition from the next list that defines that slot (an
    inherit slot no list defines is dropped)."""
    if not found:
        return []
    out = []
    for s in found[0]:
        if not s.get("inherit"):
            out.append(s)
            continue
        for lst in found[1:]:
            d = next((x for x in lst if x["slot"] == s["slot"] and not x.get("inherit")), None)
            if d:
                out.append(d)
                break
    return out


# ---------------------------------------------------------------- the evaluator

def evaluate(cat, base, modules, mission=None, bonus=None):
    """Stats of `base` with {slot: module}; problems listed (unknown module, category not allowed, a required slot
    empty, thrust below weight). `mission` applies the modules' mission_type_stats for that mission. `bonus`
    {stat: fraction} is the country's own equipment bonus (MIO traits, techs, doctrine: e.g. a range trait 0.1)
    applied on top as total x (1 + bonus); how the engine stacks it with the module multipliers is a live check."""
    problems = []
    stats = dict(cat.base_stats(base))
    add, mul, avg = {}, {}, {}
    types = set()
    slots = {s["slot"]: dict(s, categories=list(s["categories"])) for s in cat.slots(base)}
    for m in modules.values():  # a chosen turret adds the categories its own slots take
        for slot, cats in (cat.modules.get(m) or {}).get("opens", {}).items():
            if slot in slots:
                slots[slot]["categories"] += [c for c in cats if c not in slots[slot]["categories"]]
    for s in slots.values():
        if s["required"] and not modules.get(s["slot"]) or (s["required"] and modules.get(s["slot"]) == "empty"):
            problems.append(f"required slot {s['slot']} empty")
    for slot, m in modules.items():
        if not m or m == "empty":
            continue
        md = cat.modules.get(m)
        if md is None:
            problems.append(f"unknown module {m}")
            continue
        if slot not in slots:
            problems.append(f"{base} has no slot {slot}")
        elif md["category"] not in slots[slot]["categories"]:
            problems.append(f"{m} ({md['category']}) does not fit {slot}")
        types |= md["types"]
        for k, v in md["add"].items():
            add[k] = add.get(k, 0.0) + v
        for k, v in md["mul"].items():
            mul[k] = mul.get(k, 0.0) + v
        for k, v in md["avg"].items():
            avg.setdefault(k, []).append(v)
        for mt in md["mission"]:
            if mission and mission in mt["missions"]:
                for k, v in mt["add"].items():
                    add[k] = add.get(k, 0.0) + v
                for k, v in mt["mul"].items():
                    mul[k] = mul.get(k, 0.0) + v
    for k, vs in avg.items():
        add[k] = add.get(k, 0.0) + sum(vs) / len(vs)
    out = {}
    for k in set(stats) | set(add) | set(mul):
        out[k] = round((stats.get(k, 0.0) + add.get(k, 0.0)) * (1.0 + mul.get(k, 0.0))
                       * (1.0 + (bonus or {}).get(k, 0.0)), 3)
    if "thrust" in out or "weight" in out:
        if out.get("thrust", 0.0) < out.get("weight", 0.0):
            problems.append(f"thrust {out.get('thrust', 0):g} below weight {out.get('weight', 0):g}")
    btype = cat.equip.get(cat.family(base), {}).get("type")
    btypes = set(bare(btype)) if isinstance(btype, list) else ({btype} if btype else set())  # type = { a b } on duplicates
    return {"base": base, "modules": dict(modules), "stats": out, "types": sorted(types | btypes),
            "problems": problems}


def value(ev, role):
    w = ROLES[role]["weights"]
    return sum(ev["stats"].get(k, 0.0) * c for k, c in w.items())


def duel(a, b, defines=None):
    """Exchange estimate of fighter stats `a` against `b` (dicts with air_attack, air_defence, air_agility,
    maximum_speed): our damage dealt over damage taken, from the NAir defines (defaults are vanilla 1.20)."""
    d = {"BIGGEST_AGILITY_FACTOR_DIFF": 4.0, "BIGGEST_SPEED_FACTOR_DIFF": 3.5,
         "COMBAT_BETTER_AGILITY_DAMAGE_REDUCTION": 0.45, "COMBAT_BETTER_SPEED_DAMAGE_INCREASE": 0.65}
    d.update(defines or {})

    def frac(x, y, big):
        if y <= 0 or x <= y:
            return 0.0
        return min(x / y - 1.0, big - 1.0) / (big - 1.0)

    def dealt(x, y):
        dmg = max(x.get("air_attack", 0.0), 0.0) / max(y.get("air_defence", 1.0), 1.0)
        dmg *= 1.0 + frac(x.get("maximum_speed", 0.0), y.get("maximum_speed", 0.0), d["BIGGEST_SPEED_FACTOR_DIFF"]) * \
            (d["BIGGEST_SPEED_FACTOR_DIFF"] - 1.0) * d["COMBAT_BETTER_SPEED_DAMAGE_INCREASE"]
        cut = frac(y.get("air_agility", 0.0), x.get("air_agility", 0.0), d["BIGGEST_AGILITY_FACTOR_DIFF"]) * \
            d["COMBAT_BETTER_AGILITY_DAMAGE_REDUCTION"]
        return dmg * max(0.0, 1.0 - cut)

    taken = dealt(b, a)
    return round(dealt(a, b) / taken, 3) if taken > 0 else float("inf")


def available(cat, techs, kind_bases=None, granted=()):
    """(bases, modules) the technologies `techs` unlock. A duplicate family's base unlocks with its source; a base or
    module no technology unlocks is available unless a special project grants it (then only when listed in
    `granted`: completed projects' rewards), which keeps prototype engines and airframes out of a plan."""
    import re
    from . import derive  # local: derive pulls the whole tech table
    unlocks = derive.equipment_unlocks(cat.game)
    munlock = {}
    for name, body in _tech_blocks(cat.game):
        for en in get(body, "enable_equipment_modules"):
            for m in bare(en):
                munlock.setdefault(m, set()).add(name)

    def projects():
        toks = set()
        for p in cat.game.files("common/special_projects", recursive=True):
            with open(p, encoding="utf-8-sig", errors="replace") as f:
                toks.update(re.findall(r"[A-Za-z0-9_]+", f.read()))
        return toks

    by_project = cat.game.cached("designer:project_tokens", projects)
    techs, granted = set(techs), set(granted)

    def base_ok(e):
        r = cat.equip[e]
        src = r.get("duplicate_of") or e
        u = unlocks.get(e) or unlocks.get(src)
        if u:
            return bool(u & techs)
        return e in granted or not ({e, src} & by_project)

    # a designer base is a non-archetype with slots; the derived variants a base names (derived_variant_name:
    # fighter_equipment_N, the designer-less game's equipment) inherit slots but are not designer bases
    derived = {r["derived"] for r in cat.equip.values() if r.get("derived")} | getattr(cat, "variant_names", set())
    bases = [e for e, r in cat.equip.items() if not r["is_archetype"] and cat.slots(e) and e not in derived
             and (r.get("duplicate_of") or e) not in derived
             and base_ok(e) and (kind_bases is None or cat.family(e) in kind_bases)]
    mods = {m for m in cat.modules if (munlock[m] & techs if munlock.get(m) else (m in granted or m not in by_project))}
    return bases, mods


def _tech_blocks(game):
    def load():
        out = []
        for p in game.files("common/technologies"):
            for k, v in parse_file(p):
                if k == "technologies" and isinstance(v, list):
                    out.extend((n, b) for n, b in v if n and isinstance(b, list) and not n.startswith("@"))
        return out
    return game.cached("designer:techs", load)


def score(ev, role, objective="per_ic"):
    """The rank of a design: its role value, or value per sqrt(IC) (`per_ic`, default: a better plane per factory
    without always taking the cheapest)."""
    v = value(ev, role)
    if objective == "value":
        return v
    return v / max(ev["stats"].get("build_cost_ic", 1.0), 1.0) ** 0.5


def best(cat, role, bases, mods, need=None, top=3, beam=60, max_cost=None, objective="per_ic", rival=None,
         min_exchange=1.0, bonus=None):
    """Top designs for `role` over `bases` with modules from `mods`. `need`: {stat: minimum} (e.g. air_range for a
    theatre); `max_cost`: build_cost_ic cap; `rival`: the enemy design's stats a fighter must beat by `min_exchange`
    (duel); `objective`: see score; `bonus`: the country's equipment bonus (evaluate). Returns [{base, modules, stats, value, problems}] best first; a design
    that misses a requirement is left out (the result says why none fit when empty)."""
    spec = ROLES[role]
    need = dict(need or {})
    out, misses = [], {}
    fams = set(spec["families"])
    for base in bases:
        if cat.family(base) not in fams:
            continue
        # engines first: thrust is then known and weight only grows, so a partial design below its thrust is dead
        slots = sorted(cat.slots(base), key=lambda s: (not any("engine" in c for c in s["categories"]),))
        options = []
        for s in slots:
            stype = spec.get("slot_type", spec["type"])
            opts = [m for m in mods if cat.modules[m]["category"] in s["categories"]
                    and (not cat.modules[m]["allow_types"] or not stype
                         or stype in cat.modules[m]["allow_types"])]
            options.append([None] if not opts else ([] if s["required"] else [None]) + sorted(opts))
        partial = [{}]
        for s, opts in zip(slots, options):
            nxt = []
            for p in partial:
                for m in opts:
                    q = dict(p)
                    if m:
                        q[s["slot"]] = m
                    nxt.append(q)
            # keep the beam by an optimistic score (requirements are checked at the end)
            scored = []
            for q in nxt:
                ev = evaluate(cat, base, q, spec["mission"], bonus)
                ok_type = not spec["type"] or "slot_type" in spec or spec["type"] in ev["types"] or not any(
                    cat.modules[m]["types"] for m in q.values())
                if not ok_type:
                    continue
                st = ev["stats"]
                if st.get("thrust", 0.0) and st.get("thrust", 0.0) < st.get("weight", 0.0):
                    continue
                short = sum(max(0.0, lo - st.get(k, 0.0)) / max(lo, 1.0) for k, lo in need.items())
                scored.append((score(ev, role, objective) * (1.0 - min(short, 0.99)) + 1e-6 * len(q), q))
            scored.sort(key=lambda x: -x[0])
            partial = [q for _, q in scored[:beam]] or partial
        for q in partial:
            ev = evaluate(cat, base, q, spec["mission"], bonus)
            why = list(ev["problems"])
            if spec["type"] and spec["type"] not in ev["types"]:
                why.append(f"not a {spec['type']}")
            for k, lo in need.items():
                if ev["stats"].get(k, 0.0) < lo:
                    why.append(f"{k} {ev['stats'].get(k, 0.0):g} < {lo:g}")
            if max_cost is not None and ev["stats"].get("build_cost_ic", 0.0) > max_cost:
                why.append(f"cost {ev['stats'].get('build_cost_ic', 0.0):g} > {max_cost:g}")
            if rival and duel(ev["stats"], rival) < min_exchange:
                why.append(f"exchange {duel(ev['stats'], rival):g} < {min_exchange:g} against the rival")
            if why:
                for w in why:
                    misses[w.split(" ")[0]] = misses.get(w.split(" ")[0], 0) + 1
                continue
            out.append(dict(ev, value=round(value(ev, role), 2), score=round(score(ev, role, objective), 3), role=role,
                            exchange=duel(ev["stats"], rival) if rival else None))
    out.sort(key=lambda d: (-d["score"], d["stats"].get("build_cost_ic", 0.0)))
    seen, uniq = set(), []
    for d in out:
        key = (d["base"], tuple(sorted(d["modules"].items())))
        if key not in seen:
            seen.add(key)
            uniq.append(d)
    return {"designs": uniq[:top], "rejected_by": misses}


def plan_entry(design, did, name, production=None, when=None, why=""):
    """A plan `designs[]` row (the executor's affairs designs) with every slot written: never a blind design."""
    row = {"id": did, "kind": "plane" if design.get("role") in ROLES and ROLES[design["role"]]["kind"] == "plane"
           else ROLES.get(design.get("role"), {}).get("kind", "plane"),
           "base": design["base"], "name": name, "modules": dict(sorted(design["modules"].items()))}
    if production:
        row["production"] = production
    if when:
        row["when"] = when
    s = design["stats"]
    keys = [k for k in STAT_KEYS_PLANE if k in s] if row["kind"] == "plane" else sorted(s)
    row["why"] = (why + "; " if why else "") + "computed: " + ", ".join(f"{k} {s[k]:g}" for k in keys[:9]) + \
        " (designer rule from the game files; techs, MIO and doctrine applied by the game on top)"
    return row


def combos(cat, base, slot_options):
    """Every design of `base` from explicit per-slot option lists (small spaces, tests)."""
    names = list(slot_options)
    for pick in itertools.product(*(slot_options[n] for n in names)):
        yield {n: m for n, m in zip(names, pick) if m}


# ---------------------------------------------------------------- plan fragment

def xp_estimate(cat, design, add_cost=5.0):
    """Upper estimate of the air/army/navy XP a design costs from an empty base: each module's xp_cost, else the
    EQUIPMENT_MODULE_ADD_XP_COST define (`add_cost`). The designer's own price (hoi4_design_options) is the truth."""
    return round(sum(cat.modules[m]["xp_cost"] if cat.modules.get(m, {}).get("xp_cost") is not None else add_cost
                     for m in design["modules"].values()), 1)


def unlocking_techs(cat, design, start_techs):
    """Techs (not in `start_techs`) that unlock the design's base and modules: its `when` gate and research needs."""
    from . import derive
    unlocks = derive.equipment_unlocks(cat.game)
    munlock = {}
    for name, body in _tech_blocks(cat.game):
        for en in get(body, "enable_equipment_modules"):
            for m in bare(en):
                munlock.setdefault(m, set()).add(name)
    need = set()
    r = cat.equip.get(design["base"], {})
    for opts in (unlocks.get(design["base"]) or unlocks.get(r.get("duplicate_of") or "") or set(),
                 *[munlock.get(m, set()) for m in design["modules"].values()]):
        if opts and not (opts & set(start_techs)):
            need.add(sorted(opts)[0])
    return sorted(need)


def _module_unlocks(cat):
    munlock = {}
    for name, body in _tech_blocks(cat.game):
        for en in get(body, "enable_equipment_modules"):
            for m in bare(en):
                munlock.setdefault(m, set()).add(name)
    return munlock


def refresh_block(cat, designs, start_techs):
    """Fold a role's designs by year (earliest first, as plan_fragment computes them) into one `designs[].refresh`
    block: per slot the modules the later years choose, the latest first, each with the technology that unlocks it
    (none when the country starts with it), and the bases likewise. The executor makes a new version whenever the
    best available candidate differs from the newest version, so the line follows the research as it lands, not as a
    year was assumed. Nothing is ranked by hand: the order is the evaluator's choice by year."""
    from . import derive
    unlocks = derive.equipment_unlocks(cat.game)
    munlock = _module_unlocks(cat)
    start = set(start_techs)

    def gate(opts):
        return None if not opts or (opts & start) else sorted(opts)[0]

    def cand(field, key, opts):
        t = gate(opts)
        return {field: key, "tech": t} if t else key

    slots, bases = {}, []
    for d in reversed(designs):
        r = cat.equip.get(d["base"], {})
        b = cand("base", d["base"], unlocks.get(d["base"]) or unlocks.get(r.get("duplicate_of") or "") or set())
        if b not in bases:
            bases.append(b)
        for slot, m in sorted(d["modules"].items()):
            c = cand("module", m, munlock.get(m, set()))
            if c not in slots.setdefault(slot, []):
                slots[slot].append(c)
    out = {"slots": slots}
    if len(bases) > 1:
        out["bases"] = bases
    return out


def plan_fragment(cat, tag, start_techs, years, roles, tech_year, needs=None, rivals=None, objective="per_ic",
                  bonus=None, refresh=False):
    """Plan `designs[]` and the research they need, per role by year: at each year the techs of that year or before
    are assumed researched (on-time research; the strategist moves the dates), the best design is computed, and a
    new entry is written only when it beats the previous one by 5% or more. The later design of a role names the
    earlier one's base as `production` (the line switches once the design exists; the executor's mature-line guard
    applies). needs: {role: {stat: min}} (range by theatre from aircover); rivals: {role: enemy stats}; bonus:
    {role: {stat: fraction}} the country's MIO/tech bonus on that role's line (a range trait lets fewer modules
    reach the range: armour instead of a second tank). refresh: one row per role (its first year's design) whose
    `refresh` block carries the later years' choices with their techs (refresh_block): the executor makes each
    version when its technology lands and moves the line, instead of one dated row per year."""
    needs, rivals = needs or {}, rivals or {}
    rows, research, last = [], [], {}
    by_role = {}
    for y in sorted(years):
        techs = set(start_techs) | {t for t, yr in tech_year.items() if yr and yr <= y}
        bases, mods = available(cat, techs)
        for role in roles:
            r = best(cat, role, bases, mods, need=needs.get(role), rival=rivals.get(role), objective=objective, top=1,
                     bonus=(bonus or {}).get(role))
            if not r["designs"]:
                continue
            d = r["designs"][0]
            prev = last.get(role)
            if prev and (d["base"], d["modules"]) == (prev["base"], prev["modules"]):
                continue
            if prev and d["score"] < prev["score"] * 1.05:
                continue
            gate = unlocking_techs(cat, d, start_techs)
            did = f"d_{tag.lower()}_{role}_{y}"
            row = plan_entry(d, did, f"{role.replace('_', ' ').title()} {y}",
                             production=prev["base"] if prev else None,
                             when=({"all": [f"tech_researched:{t}" for t in gate]} if len(gate) > 1 else
                                   (f"tech_researched:{gate[0]}" if gate else None)),
                             why=f"{role} for {y}" + (f" (need {needs[role]})" if needs.get(role) else ""))
            row["xp"] = xp_estimate(cat, d)
            rows.append(row)
            by_role.setdefault(role, []).append((row, d))
            research.extend(t for t in gate if t not in research)
            last[role] = d
    if refresh:
        folded = []
        for role, pairs in by_role.items():
            row = dict(pairs[0][0])
            if len(pairs) > 1:
                row["refresh"] = refresh_block(cat, [d for _, d in pairs], start_techs)
                row["why"] += "; refresh: later years' designs as their techs land (" + \
                    ", ".join(r["id"] for r, _ in pairs[1:]) + ")"
            folded.append(row)
        rows = folded
    return {"designs": rows, "research_needed": research,
            "method": "designer.plan_fragment: best per role and year by computed stats; techs of a year assumed on time"
            + ("; refresh blocks follow the research as it lands" if refresh else "")}


# ---------------------------------------------------------------- MIO by value for a design's line

def equipment_groups(game):
    """{group: set(equipment archetypes)} of common/equipment_groups (the MIO categories such as
    mio_cat_eq_all_small_plane)."""
    def load():
        out = {}
        for p in game.files("common/equipment_groups"):
            for k, v in parse_file(p):
                if k and isinstance(v, list):
                    et = get1(v, "equipment_type")
                    out[k] = set(bare(et)) if isinstance(et, list) else set()
        return out
    return game.cached("designer:equipment_groups", load)


def covers(cat, tokens, base):
    """True when MIO equipment tokens (archetypes or equipment groups) include `base`'s family (a duplicate family
    also through its source)."""
    if not tokens:
        return True
    groups = equipment_groups(cat.game)
    fams = {cat.family(base)}
    src = cat.equip.get(base, {}).get("duplicate_of")
    if src:
        fams.add(cat.family(src))
    for t in tokens:
        if t in fams or t == base or fams & groups.get(t, set()):
            return True
    return False


def mio_bonus(cat, org, base, traits=None):
    """{stat: fraction} an organisation gives `base`: its initial trait plus the traits named (tokens; None: every
    trait, the organisation's ceiling). Only traits whose limit_to_equipment_type covers the base count."""
    out = {}
    rows = [org.get("initial_trait") or {}] + [t for t in org.get("traits") or []
                                               if traits is None or t.get("token") in traits]
    for t in rows:
        if not covers(cat, t.get("limit_to_equipment_type") or [], base):
            continue
        for k, v in (t.get("equipment_bonus") or {}).items():
            if isinstance(v, (int, float)):
                out[k] = out.get(k, 0.0) + v
    return out


def mio_rank(cat, orgs, role, bases, mods, need=None, objective="per_ic"):
    """Organisations for `role`'s line, best first by the best design each makes reachable: the design search is
    run again with the organisation's bonus (initial trait; then all traits, its ceiling), so a range trait counts
    by what it frees under a range requirement (armour in place of a fuel tank) and not by a weight on range.
    orgs: World.mio(tag)["organisations"]. Rows: {id, name, base, initial_score, ceiling_score, design}."""
    plain = best(cat, role, bases, mods, need=need, objective=objective, top=1)["designs"]
    rows = []
    fams = set(ROLES[role]["families"])
    for o in orgs:
        if not o.get("available", True):
            continue
        pick = [b for b in bases if cat.family(b) in fams and covers(cat, o.get("equipment_type") or [], b)]
        if not pick:
            continue
        b0 = pick[-1]
        ini, ceil = mio_bonus(cat, o, b0, traits=()), mio_bonus(cat, o, b0)
        r1 = best(cat, role, pick, mods, need=need, objective=objective, top=1, bonus=ini)["designs"]
        r2 = best(cat, role, pick, mods, need=need, objective=objective, top=1, bonus=ceil)["designs"]
        rows.append({"id": o["id"], "name": o.get("name"), "initial_bonus": ini,
                     "initial_score": r1[0]["score"] if r1 else None, "ceiling_score": r2[0]["score"] if r2 else None,
                     "design": r1[0] if r1 else None})
    rows.sort(key=lambda r: (-(r["ceiling_score"] or 0), -(r["initial_score"] or 0)))  # traits grow over a war
    return {"without": plain[0]["score"] if plain else None, "organisations": rows}
