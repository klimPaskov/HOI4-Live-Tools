"""1. Equipment design by role, against what the enemy fields: tanks (and other land designer vehicles), ships; planes
through designer.py (engineer 106's evaluator) with the enemy's AI fighter as the rival.

Data (all from the effective files; nothing about a country or year is written here):
  common/units/equipment           bases: own stats over the parent chain over the archetype (a variant names its
                                   `parent`; stats, slots, upgrades and module_add_stats_multiplier are inherited
                                   that way), module_slots (required, allowed_module_categories), module_count_limit
                                   (module or category, count < N), type
  common/units/equipment/modules   category, add_stats, multiply_stats, allowed_module_categories (a turret opens the
                                   main armament categories), allow_equipment_type / forbid_equipment_type /
                                   forbid_equipment_type_exact_match(_for_category) (which design role a module may
                                   serve), a DLC `limit`
  common/units/equipment/upgrades  per-level add_stats and top-level per-level stats; level_requirements (has_tech)
  common/technologies              enable_equipments / enable_equipment_modules (designer.available) and upgrade gates
  common/ai_equipment              the AI's design templates (available_for / blocked_for, target_variant type,
                                   modules incl. any_of, upgrades incl. base): what an enemy fields by its techs
  common/defines                   PIERCING_THRESHOLDS / PIERCING_THRESHOLD_DAMAGE_VALUES, LAND_COMBAT_ORG_DICE_SIZE,
                                   LAND_COMBAT_ORG_ARMOR_ON_SOFT_DICE_SIZE, RELIABILITY_ORG_REGAIN

Stat rule (land designer, the game's): stat = (base + sum of the modules' add_stats, attack stats scaled by the
chassis' module_add_stats_multiplier, + upgrade add_stats x level) x (1 + sum of multiply_stats); a top-level stat of
an upgrade adds per level (reliability -0.015 a level). Techs, doctrines and MIO traits apply after this and are
shared by every design of one country, so a ranking between our designs holds; the comparison with an enemy design
leaves them out on both sides.

Role score (land), from the combat formulas in the defines against the enemy profile:
  effective attack  = (soft x (1 - h) + hard x h) x pierce(our piercing / their armour)
                      x (armour dice / plain dice when our armour exceeds their piercing)
  effective defence = (breakthrough or defence) / pierce(their piercing / our armour)   (their hits on us are cut
                      the same way their piercing falls short of our armour)
  reliability       x (1 + RELIABILITY_ORG_REGAIN x (1 - reliability))   (missing reliability slows org regain)
  speed             a floor, not a weight: the speed of the battalions the vehicle fights beside (the infantry
                    equipment's maximum_speed); points above it buy nothing in a division that moves at its slowest
  objective         value per sqrt(IC) as designer.score (a better vehicle per factory, not always the cheapest)
h is the enemy division's hardness, their armour the best armour among their designs by the date, their piercing
the division piercing of their best infantry template (0.4 x max + 0.6 x mean over battalions, divstats).
Upgrades (engine and armour levels) are searched under the tech caps: for each armour level, the least engine level
that meets the speed floor.
"""
import itertools

from .. import designer, divstats
from ..script import bare, get, get1, parse_file

LAND_ROLES = {
    # role: design type (the equipment type the role fields), what it is valued for
    "tank_main": {"type": "armor", "value": "attack+breakthrough"},
    "tank_destroyer": {"type": "anti_tank", "value": "hard_attack+breakthrough"},
    "tank_artillery": {"type": "artillery", "value": "soft_attack"},
    "tank_aa": {"type": "anti_air", "value": "air_attack"},
    "tank_garrison": {"type": "armor", "value": "hardness"},
}
SHIP_ROLES = {
    # value: provisional stat weights (not a naval combat model); types: the hull archetype's `type` in
    # common/units/equipment that may serve the role (cruiser hulls are screen_ship in the files, so the weights tell
    # a screen from a cruiser)
    "ship_screen": {"value": {"sub_attack": 1.0, "lg_attack": 0.7, "torpedo_attack": 0.3, "anti_air_attack": 0.3},
                    "types": {"screen_ship"}},
    "ship_cruiser": {"value": {"lg_attack": 1.0, "armor_value": 0.3, "anti_air_attack": 0.3}, "types": {"screen_ship"}},
    "ship_capital": {"value": {"hg_attack": 1.0, "armor_value": 0.5, "anti_air_attack": 0.2}, "types": {"capital_ship"}},
    "ship_submarine": {"value": {"torpedo_attack": 1.0, "naval_range": 0.002}, "types": {"submarine"}},
}
ATTACK_STATS = ("soft_attack", "hard_attack", "air_attack")


def _num(v, default=None):
    try:
        return float(v)
    except (TypeError, ValueError):
        return default


def _numbers(block):
    out = {}
    for k, v in block or []:
        f = _num(v)
        if k and f is not None:
            out[k] = out.get(k, 0.0) + f
    return out


def _tokens(v):
    if isinstance(v, list):
        return set(bare(v)) | {a for a, b in v if a and b in ("yes",)}
    return {v} if v else set()


# ---------------------------------------------------------------- catalog

class Vehicles:
    """Designer bases, modules and upgrades of the effective files (read once per game)."""

    def __init__(self, game):
        self.game = game
        self.cat = designer.Catalog(game)
        self.equip = {}
        self.modules = {}
        self.upgrades = {}
        self._load()

    @classmethod
    def of(cls, game):
        return game.cached("eval:vehicles", lambda: cls(game))

    def _load(self):
        for p in self.game.files("common/units/equipment"):
            for k, v in parse_file(p):
                if k != "equipments" or not isinstance(v, list):
                    continue
                for name, b in v:
                    if not name or not isinstance(b, list):
                        continue
                    mult = {}
                    for kk, vv in b:
                        if kk == "module_add_stats_multiplier" and isinstance(vv, list):
                            mult.update(_numbers(vv))
                    limits = []
                    for blk in get(b, "module_count_limit"):
                        if isinstance(blk, list):
                            i = next((j for j, (a, _) in enumerate(blk) if a == "count"), None)
                            n = _num(blk[i][1]) if i is not None else None
                            op = getattr(blk, "ops", ["<"] * len(blk))[i] if i is not None else "<"
                            limits.append({"module": get1(blk, "module"), "category": get1(blk, "category"),
                                           "max": (n - 1 if op == "<" else n) if n is not None else None})
                    ups = get1(b, "upgrades")
                    typ = get1(b, "type")
                    slots = get1(b, "module_slots")
                    self.equip[name] = {
                        "id": name, "archetype": get1(b, "archetype"), "parent": get1(b, "parent"),
                        "is_archetype": get1(b, "is_archetype") == "yes", "year": _num(get1(b, "year"), 0),
                        "type": set(bare(typ)) if isinstance(typ, list) else ({typ} if typ else set()),
                        "stats": {kk: vv for kk, vv in _numbers(b).items()
                                  if kk not in ("year", "priority", "visual_level", "lend_lease_cost")},
                        "module_mult": mult, "limits": limits,
                        "upgrades": list(bare(ups)) if isinstance(ups, list) else None,
                        "slots": designer._slots(slots) if isinstance(slots, list) else None}
        for p in self.game.files("common/units/equipment/modules"):
            for k, v in parse_file(p):
                if k != "equipment_modules" or not isinstance(v, list):
                    continue
                file_dlcs = set()  # a `limit = { has_dlc = .. }` at the top of the block gates every module in it
                top = get1(v, "limit")
                for a, c in (top if isinstance(top, list) else []):
                    if a == "has_dlc" and isinstance(c, str):
                        file_dlcs.add(c.strip('"'))
                for name, b in v:
                    if not name or name == "limit" or not isinstance(b, list):
                        continue
                    opens = get1(b, "allowed_module_categories")
                    exact_cat = get1(b, "forbid_equipment_type_exact_match_for_category")
                    lim = get1(b, "limit")
                    dlcs = set(file_dlcs)
                    if isinstance(lim, list):
                        for a, c in lim:
                            if a == "has_dlc" and isinstance(c, str):
                                dlcs.add(c.strip('"'))
                    self.modules[name] = {
                        "id": name, "category": get1(b, "category"),
                        "add": _numbers(get1(b, "add_stats")), "mul": _numbers(get1(b, "multiply_stats")),
                        "opens": {n: list(bare(c)) for n, c in opens if n and isinstance(c, list)}
                        if isinstance(opens, list) else {},
                        "allow": set().union(*[_tokens(x) for x in get(b, "allow_equipment_type")])
                        if get(b, "allow_equipment_type") else set(),
                        "forbid": set().union(*[_tokens(x) for x in get(b, "forbid_equipment_type")])
                        if get(b, "forbid_equipment_type") else set(),
                        "forbid_exact": set().union(*[_tokens(x) for x in get(b, "forbid_equipment_type_exact_match")])
                        if get(b, "forbid_equipment_type_exact_match") else set(),
                        "forbid_exact_cat": {a: c for a, c in exact_cat if a and isinstance(c, str)}
                        if isinstance(exact_cat, list) else {},
                        "dlcs": dlcs, "xp": _num(get1(b, "xp_cost"))}
        for p in self.game.files("common/units/equipment/upgrades"):
            for k, v in parse_file(p):
                if k != "upgrades" or not isinstance(v, list):
                    continue
                for name, b in v:
                    if not name or not isinstance(b, list):
                        continue
                    req = {}
                    lr = get1(b, "level_requirements")
                    for lvl, blk in (lr if isinstance(lr, list) else []):
                        if _num(lvl) is not None and isinstance(blk, list):
                            req[int(_num(lvl))] = set(x for a, x in blk if a == "has_tech")
                    top = {kk: vv for kk, vv in _numbers(b).items() if kk not in ("max_level",)}
                    self.upgrades[name] = {"max": int(_num(get1(b, "max_level"), 0) or 0), "req": req,
                                           "add": _numbers(get1(b, "add_stats")), "per_level": top}

    # chain: the variant, its parents, then the archetype
    def chain(self, e):
        out, cur = [], e
        while cur in self.equip and cur not in out:
            out.append(cur)
            cur = self.equip[cur]["parent"]
        arch = self.equip.get(e, {}).get("archetype")
        if arch in self.equip and arch not in out:
            out.append(arch)
        return out

    def inherited(self, e, field):
        for x in self.chain(e):
            v = self.equip[x].get(field)
            if v:
                return v
        return None

    def base_stats(self, e):
        st = {}
        for x in reversed(self.chain(e)):
            st.update(self.equip[x]["stats"])
        return st

    def family(self, e):
        c = self.chain(e)
        return c[-1] if c else e

    def slots(self, e):
        found = [self.equip[x]["slots"] for x in self.chain(e) if self.equip[x].get("slots")]
        cat = self.cat.slots(e)
        return designer.resolve_slots(found + ([cat] if cat else [])) or cat

    def limits(self, e):
        out = []
        for x in self.chain(e):
            out.extend(self.equip[x]["limits"])
        return out

    def upgrade_caps(self, e, techs):
        """{upgrade: highest level the techs allow}: a level needs the tech of the highest requirement at or below it."""
        out = {}
        for u in self.inherited(e, "upgrades") or []:
            ud = self.upgrades.get(u)
            if not ud:
                continue
            cap = ud["max"]
            for lvl in sorted(ud["req"]):
                if not (ud["req"][lvl] & techs):
                    cap = min(cap, lvl - 1)
                    break
            out[u] = max(0, cap)
        return out


def stats(v, base, modules, upgrades=None):
    """The design's stats by the land/naval designer rule (see the module doc)."""
    st = dict(v.base_stats(base))
    mm = v.inherited(base, "module_mult") or {}
    mult = {}
    for m in modules.values():
        md = v.modules.get(m)
        if not md:
            continue
        for s, x in md["add"].items():
            st[s] = st.get(s, 0.0) + x * (1 + (mm.get(s, 0.0) if s in ATTACK_STATS else 0.0))
        for s, x in md["mul"].items():
            mult[s] = mult.get(s, 0.0) + x
    for u, lvl in (upgrades or {}).items():
        ud = v.upgrades.get(u)
        if not ud or not lvl:
            continue
        for s, x in list(ud["add"].items()) + list(ud["per_level"].items()):
            st[s] = st.get(s, 0.0) + x * lvl
    return {s: round(x * (1 + mult.get(s, 0.0)), 4) for s, x in st.items()}


def legal(v, base, modules, role_type, dlcs=None):
    """Problems of a design for a role type (empty: legal): slots and their categories (a chosen turret adds its own),
    required slots, count limits, the modules' allow/forbid rules for the type, DLC limits."""
    probs = []
    slots = {s["slot"]: dict(s, categories=list(s["categories"])) for s in v.slots(base)}
    for m in modules.values():
        for slot, cats in (v.modules.get(m) or {}).get("opens", {}).items():
            if slot in slots:
                slots[slot]["categories"] = list(cats)
    for s in slots.values():
        if s["required"] and not modules.get(s["slot"]):
            probs.append(f"required slot {s['slot']} empty")
    counts_m, counts_c = {}, {}
    for slot, m in modules.items():
        md = v.modules.get(m)
        if not md:
            probs.append(f"unknown module {m}")
            continue
        if slot not in slots or md["category"] not in slots[slot]["categories"]:
            probs.append(f"{m} does not fit {slot}")
        if role_type in md["forbid"] or role_type in md["forbid_exact"]:
            probs.append(f"{m} forbidden for {role_type}")
        if md["allow"] and role_type != "armor" and role_type not in md["allow"] and slot == "main_armament_slot":
            probs.append(f"{m} not allowed for {role_type}")
        if dlcs is not None and md["dlcs"] and not (md["dlcs"] & set(dlcs)):
            probs.append(f"{m} needs {sorted(md['dlcs'])}")
        counts_m[m] = counts_m.get(m, 0) + 1
        counts_c[md["category"]] = counts_c.get(md["category"], 0) + 1
    if role_type != "armor":
        gun = v.modules.get(modules.get("main_armament_slot") or "")
        if gun and gun["allow"] and role_type not in gun["allow"]:
            pass
        elif gun is not None and not gun["allow"] and role_type in ("anti_tank", "artillery", "anti_air"):
            probs.append(f"main armament {gun['id']} serves no {role_type} role")
    for m in modules.values():
        for cat, typ in (v.modules.get(m) or {}).get("forbid_exact_cat", {}).items():
            if typ == role_type and any((v.modules.get(x) or {}).get("category") == cat for x in modules.values()):
                probs.append(f"{m} with a {cat} cannot be {role_type}")
    for lim in v.limits(base):
        if lim["max"] is None:
            continue
        n = counts_m.get(lim["module"], 0) if lim["module"] else counts_c.get(lim["category"], 0)
        if n > lim["max"]:
            probs.append(f"more than {lim['max']:g} of {lim['module'] or lim['category']}")
    return sorted(set(probs))


# ---------------------------------------------------------------- combat model from the defines

class Combat:
    def __init__(self, sit):
        th = sit.define("PIERCING_THRESHOLDS") or [1.0, 0.0]
        dv = sit.define("PIERCING_THRESHOLD_DAMAGE_VALUES") or [1.0, 0.5]
        self.thresholds = list(zip(th, dv))
        plain = float(sit.define("LAND_COMBAT_ORG_DICE_SIZE", 0) or 0)
        armour = float(sit.define("LAND_COMBAT_ORG_ARMOR_ON_SOFT_DICE_SIZE", 0) or 0)
        # mean of a dN is (N + 1) / 2: the armour dice replace the plain dice when our armour wins
        self.dice = (armour + 1) / (plain + 1) if plain and armour else 1.0
        self.rel = float(sit.define("RELIABILITY_ORG_REGAIN", 0) or 0)
        self.src = [sit.define_src(k) for k in ("PIERCING_THRESHOLDS", "PIERCING_THRESHOLD_DAMAGE_VALUES",
                                                "LAND_COMBAT_ORG_DICE_SIZE", "LAND_COMBAT_ORG_ARMOR_ON_SOFT_DICE_SIZE",
                                                "RELIABILITY_ORG_REGAIN")]

    def pierce(self, piercing, armour):
        if not armour or armour <= 0:
            return 1.0
        r = (piercing or 0.0) / armour
        for t, d in self.thresholds:
            if r >= t:
                return d
        return self.thresholds[-1][1]


# ---------------------------------------------------------------- the enemy as our country sees it

def ai_designs(game):
    """[(group, {available_for, blocked_for, category, roles}, design name, target_variant)] of common/ai_equipment."""
    def load():
        out = []
        for p in game.files("common/ai_equipment"):
            for gname, g in parse_file(p):
                if not gname or not isinstance(g, list):
                    continue
                av = get1(g, "available_for")
                bl = get1(g, "blocked_for")
                rl = get1(g, "roles")
                meta = {"available_for": set(bare(av)) if isinstance(av, list) else set(),
                        "blocked_for": set(bare(bl)) if isinstance(bl, list) else set(),
                        "category": get1(g, "category"), "roles": list(bare(rl)) if isinstance(rl, list) else []}
                for dname, d in g:
                    if not isinstance(d, list):
                        continue
                    tv = get1(d, "target_variant")
                    if isinstance(tv, list) and get1(tv, "type"):
                        out.append((gname, meta, dname, tv))
        return out
    return game.cached("eval:aidesigns", load)


def _resolve_ai(v, tv, bases, mods):
    """(base, modules, upgrades) of an AI target variant with what the country has, or None."""
    base = get1(tv, "type")
    if base not in bases:
        return None
    modules = {}
    for slot, m in get1(tv, "modules") or []:
        if not slot:
            continue
        if isinstance(m, str):
            if m != "empty":
                if m not in mods:
                    return None
                modules[slot] = m
        elif isinstance(m, list):
            opts = []
            for k, x in m:
                if k in ("any_of", "all_of") and isinstance(x, list):
                    opts += bare(x)
            pick = next((o for o in opts if o in mods), None)
            if pick:
                modules[slot] = pick
            elif opts:
                return None
    ups = {}
    for u, lv in get1(tv, "upgrades") or []:
        n = _num(lv) if isinstance(lv, str) else _num(get1(lv, "base"))
        if u and n:
            ups[u] = n
    return base, modules, ups


def enemy_profile(sit, enemies=None):
    """What the enemies field by the date (files; FAIR: the AI's own design templates and start variants, their
    templates and techs): best armour and piercing among their vehicles, their infantry division's piercing, hardness
    and armour, their best fighter. Live sightings (state.enemy_designs) are added when given."""
    tags, how = sit.enemies(enemies)
    v = Vehicles.of(sit.game)
    data = sit.cached("divdata", lambda: divstats.Data(sit.game))
    rows, divs = [], []
    fighters = []
    for tag in tags:
        en = sit.other(tag)
        techs = en.techs()
        bases, mods = designer.available(v.cat, techs)
        bases = set(bases)
        for gname, meta, dname, tv in ai_designs(sit.game):
            if meta["available_for"] and tag not in meta["available_for"]:
                continue
            if tag in meta["blocked_for"]:
                continue
            r = _resolve_ai(v, tv, bases, mods)
            if not r:
                continue
            base, modules, ups = r
            if meta["category"] == "air":
                ev = designer.evaluate(v.cat, base, modules, "air_superiority")
                if "fighter" in ev["types"] or any("fighter" in x for x in meta["roles"]):
                    fighters.append({"tag": tag, "design": f"{gname}.{dname}", "stats": ev["stats"]})
                continue
            if meta["category"] != "land":
                continue
            st = stats(v, base, modules, ups)
            rows.append({"tag": tag, "design": f"{gname}.{dname}", "base": base,
                         "armor": st.get("armor_value", 0.0), "piercing": st.get("ap_attack", 0.0),
                         "hardness": st.get("hardness", 0.0)})
        for d in (sit.state.get("enemy_designs") or {}).get(tag, []):
            st = stats(v, d.get("type"), d.get("modules") or {}, d.get("upgrades") or {})
            rows.append({"tag": tag, "design": "seen: " + str(d.get("name") or d.get("type")), "base": d.get("type"),
                         "armor": st.get("armor_value", 0.0), "piercing": st.get("ap_attack", 0.0),
                         "hardness": st.get("hardness", 0.0)})
        # their infantry: the start templates with the newest equipment their techs give
        oob = en.world.oob(tag)
        mods_f = divstats.factors(data, techs, en.doctrines())
        equip = {}
        for arch in {data.archetype(n) for u in data.units.values() for n in u["need"]}:
            new = data.newest(arch, techs)
            if new:
                equip[arch] = new
        for name, tpl in (oob.get("templates") or {}).items():
            bats, sup = divstats.template_units(tpl)
            bats = [b for b in bats if b in data.units]
            sup = [s for s in sup if s in data.units]
            if not bats:
                continue
            try:
                d = divstats.division(data, bats, sup, equip, mods_f)
            except Exception:  # noqa: BLE001 - an odd template is skipped, not guessed
                continue
            n = sum(1 for x in oob.get("divisions") or [] if x.get("template") == name)
            divs.append({"tag": tag, "template": name, "piercing": d.get("piercing", 0.0),
                         "armor": d.get("armor", 0.0), "hardness": d.get("hardness", 0.0),
                         "width": d.get("width", 0.0), "fielded": n, "stats": d})
    line = [d for d in divs if d["hardness"] < 0.5] or divs
    prof = {
        "enemies": tags, "chosen": how,
        "best_armor": max([r["armor"] for r in rows] + [d["armor"] for d in divs] + [0.0]),
        "best_vehicle_piercing": max([r["piercing"] for r in rows] + [0.0]),
        "division_piercing": max([d["piercing"] for d in line] + [0.0]),
        "hardness": round(sum(d["hardness"] for d in line) / len(line), 3) if line else 0.0,
        "tank_hardness": max([r["hardness"] for r in rows] + [0.0]),
        "vehicles": sorted(rows, key=lambda r: -r["armor"])[:6],
        "divisions": sorted(line, key=lambda d: -d["piercing"])[:4],
        "fighters": sorted(fighters, key=lambda f: -f["stats"].get("air_attack", 0.0))[:3],
    }
    prof["piercing"] = max(prof["division_piercing"], prof["best_vehicle_piercing"])
    return prof


# ---------------------------------------------------------------- scoring and search

def floor_speed(sit):
    """The speed of the battalions a vehicle fights beside: the infantry equipment's maximum_speed, found as the
    equipment the most common line sub-unit needs (no name typed in)."""
    data = sit.cached("divdata", lambda: divstats.Data(sit.game))
    v = Vehicles.of(sit.game)
    counts = {}
    oob = sit.world.oob(sit.tag)
    used = {}
    for d in oob.get("divisions") or []:
        used[d.get("template")] = used.get(d.get("template"), 0) + 1
    for name, tpl in (oob.get("templates") or {}).items():
        bats, _ = divstats.template_units(tpl)
        for b in bats:
            counts[b] = counts.get(b, 0) + used.get(name, 0)
    for unit, _ in sorted(counts.items(), key=lambda x: -x[1]):
        u = data.units.get(unit) or {}
        for need in u.get("need") or {}:
            sp = v.base_stats(need).get("maximum_speed")
            if sp and v.equip.get(need, {}).get("is_archetype", True):
                return sp, f"{need} maximum_speed (the equipment of {unit}, the most common battalion)"
            arch = data.archetype(need)
            sp = v.base_stats(arch).get("maximum_speed")
            if sp:
                return sp, f"{arch} maximum_speed (the equipment of {unit}, the most common battalion)"
    return 0.0, "no line battalion found"


def value(st, role, prof, combat):
    spec = LAND_ROLES[role]
    h = prof["tank_hardness"] if role == "tank_destroyer" else prof["hardness"]
    armour = st.get("armor_value", 0.0)
    # a tank destroyer is judged against the enemy's best armour, any other role against the line it attacks
    pf_out = combat.pierce(st.get("ap_attack", 0.0),
                           prof["best_armor"] if role == "tank_destroyer" else _enemy_line_armor(prof))
    dice = combat.dice if armour > prof["piercing"] else 1.0
    pf_in = combat.pierce(prof["piercing"], armour)
    att = (st.get("soft_attack", 0.0) * (1 - h) + st.get("hard_attack", 0.0) * h) * pf_out * dice
    surv = st.get("breakthrough", 0.0) / max(pf_in, 0.01)
    if spec["value"] == "attack+breakthrough":
        val = att + surv
    elif spec["value"] == "hard_attack+breakthrough":
        val = st.get("hard_attack", 0.0) * combat.pierce(st.get("ap_attack", 0.0), prof["best_armor"]) + surv
    elif spec["value"] == "soft_attack":
        val = st.get("soft_attack", 0.0) * (1 - h) * dice + 0.25 * surv
    elif spec["value"] == "air_attack":
        val = st.get("air_attack", 0.0) + 0.25 * surv
    else:  # hardness: what a garrison keeps per attack
        val = st.get("hardness", 0.0) * 10
    rel = st.get("reliability", 1.0)
    val *= max(0.0, 1.0 + combat.rel * max(0.0, 1.0 - rel))
    return val, {"effective_attack": round(att, 2), "effective_breakthrough": round(surv, 2),
                 "pierce_out": pf_out, "pierce_in": pf_in, "armour_dice": dice}


def _enemy_line_armor(prof):
    return max([d["armor"] for d in prof["divisions"]] + [0.0])


def _role_bases(v, bases, role_type, families=None):
    out = []
    for b in bases:
        fam = v.family(b)
        types = v.equip.get(fam, {}).get("type") or set()
        if "armor" not in types:
            continue
        if families and fam not in families:
            continue
        out.append(b)
    return out


def _upgrade_grid(v, base, techs, mods, floor, role, prof, combat, objective):
    return _upgrade_grids(v, base, techs, mods, [floor], role, prof, combat, objective)[floor]


def _upgrade_grids(v, base, techs, mods, floors, role, prof, combat, objective):
    """{floor: (score, value, upgrades, stats, parts) or None}: for every level combination of the non-engine
    upgrades, the least engine level that meets each speed floor, scored; the best per floor."""
    caps = v.upgrade_caps(base, techs)
    names = sorted(caps)
    eng = next((u for u in names if v.upgrades[u]["add"].get("maximum_speed", 0) > 0), None)
    others = [u for u in names if u != eng]
    best = {fl: None for fl in floors}
    ranges = [range(0, caps[u] + 1) for u in others]
    for combo in itertools.product(*ranges) if others else [()]:
        ups = dict(zip(others, combo))
        todo = sorted(floors)
        for e in range(0, (caps[eng] if eng else 0) + 1):
            if not todo:
                break
            if eng:
                ups[eng] = e
            st = stats(v, base, mods, ups)
            met = [fl for fl in todo if st.get("maximum_speed", 0.0) + 1e-9 >= fl or role == "tank_garrison"]
            if not met:
                continue
            val, parts = value(st, role, prof, combat)
            sc = val / max(st.get("build_cost_ic", 1.0), 1.0) ** (0.5 if objective == "per_ic" else 1.0) \
                if objective != "value" else val
            for fl in met:
                if best[fl] is None or sc > best[fl][0]:
                    best[fl] = (sc, val, dict(ups), st, parts)
            todo = [fl for fl in todo if fl not in met]
    return best


def best_land(sit, role, enemies=None, families=None, top=3, beam=40, objective=None, techs=None, prof=None,
              floor=None):
    """Top designs for a land role (LAND_ROLES) with what the country has at the date. floor: a speed floor in km/h
    instead of the infantry's, or a list of floors (the armour template weighs speed itself): the module search runs
    once and the upgrade grid once per floor, and `by_floor` lists [(floor, designs)]."""
    v = Vehicles.of(sit.game)
    combat = Combat(sit)
    techs = set(techs if techs is not None else sit.techs())
    bases, mods = designer.available(v.cat, techs)
    prof = prof or enemy_profile(sit, enemies)
    role_type = LAND_ROLES[role]["type"]
    objective = objective or ("per_ic_linear" if role == "tank_garrison" else "per_ic")
    if floor is None:
        f0, floor_src = floor_speed(sit)
        floors = [f0]
    else:
        floors = [float(x) for x in (floor if isinstance(floor, (list, tuple)) else [floor])]
        floor_src = "asked by the caller"
    dlcs = sit.world.dlcs
    outs = {fl: [] for fl in floors}
    rejected = {}
    for base in _role_bases(v, bases, role_type, families):
        slots = sorted(v.slots(base), key=lambda s: (s["slot"] != "turret_type_slot",
                                                     s["slot"] != "main_armament_slot", s["slot"]))
        partial = [{}]
        for s in slots:
            nxt = []
            for p in partial:
                cats = list(s["categories"])
                for m in p.values():
                    cats = (v.modules.get(m) or {}).get("opens", {}).get(s["slot"], cats)
                opts = [m for m in mods if m in v.modules and v.modules[m]["category"] in cats]
                choices = ([] if s["required"] and opts else [None]) + sorted(opts)
                for m in choices:
                    q = dict(p)
                    if m:
                        q[s["slot"]] = m
                    probs = [x for x in legal(v, base, q, role_type, dlcs) if "required slot" not in x]
                    if not probs:
                        nxt.append(q)
            scored, seen_q = [], set()
            for q in nxt:
                key = tuple(sorted(q.values()))
                if key in seen_q:
                    continue
                seen_q.add(key)
                st = stats(v, base, q)
                val, _ = value(st, role, prof, combat)
                scored.append((val / max(st.get("build_cost_ic", 1.0), 1.0) ** 0.5 + 1e-6 * len(q), q))
            scored.sort(key=lambda x: -x[0])
            partial = [q for _, q in scored[:beam]] or partial
        for q in partial:
            probs = legal(v, base, q, role_type, dlcs)
            if probs:
                for p in probs:
                    rejected[p.split(" ")[0]] = rejected.get(p.split(" ")[0], 0) + 1
                continue
            grids = _upgrade_grids(v, base, techs, q, floors, role, prof, combat, objective)
            for fl in floors:
                g = grids[fl]
                if not g:
                    if fl == floors[0]:
                        rejected["speed"] = rejected.get("speed", 0) + 1
                    continue
                sc, val, ups, st, parts = g
                outs[fl].append({"base": base, "modules": q, "upgrades": ups, "stats": _show(st),
                                 "value": round(val, 2), "score": round(sc, 3), "parts": parts, "role": role,
                                 "type": role_type})

    def uniq(out):
        out.sort(key=lambda d: -d["score"])
        seen, res = set(), []
        for d in out:
            key = (d["base"], tuple(sorted(d["modules"].values())))  # the same modules in other free slots: one design
            if key not in seen:
                seen.add(key)
                res.append(d)
        return res[:top]

    by_floor = [(fl, uniq(outs[fl])) for fl in floors]
    res = {"role": role, "designs": by_floor[0][1], "rejected_by": rejected, "enemy": prof,
           "speed_floor": floors[0], "speed_floor_from": floor_src, "combat_defines": combat.src,
           "techs": sit.techs_source(), "objective": objective}
    if isinstance(floor, (list, tuple)):
        res["by_floor"] = by_floor
    return res


def _show(st):
    keys = ("soft_attack", "hard_attack", "ap_attack", "breakthrough", "defense", "armor_value", "air_attack",
            "maximum_speed", "reliability", "hardness", "build_cost_ic", "fuel_consumption")
    return {k: round(st[k], 2) for k in keys if k in st}


def score_design(sit, role, base, modules, upgrades=None, enemies=None, prof=None):
    """Score a given design (the owner's, a plan's) by the same rule, for comparison."""
    v = Vehicles.of(sit.game)
    combat = Combat(sit)
    prof = prof or enemy_profile(sit, enemies)
    st = stats(v, base, modules, upgrades)
    val, parts = value(st, role, prof, combat)
    return {"base": base, "modules": modules, "upgrades": upgrades or {}, "stats": _show(st), "value": round(val, 2),
            "score": round(val / max(st.get("build_cost_ic", 1.0), 1.0) ** 0.5, 3), "parts": parts,
            "legal": legal(v, base, modules, LAND_ROLES[role]["type"], sit.world.dlcs)}


# ---------------------------------------------------------------- planes: designer.py with the enemy's AI fighter

def best_plane(sit, role, enemies=None, need=None, top=3, prof=None):
    v = Vehicles.of(sit.game)
    bases, mods = designer.available(v.cat, sit.techs())
    prof = prof or enemy_profile(sit, enemies)
    rival = prof["fighters"][0]["stats"] if prof["fighters"] and role in ("fighter", "heavy_fighter") else None
    r = designer.best(v.cat, role, bases, mods, need=need, top=top, rival=rival, min_exchange=0.0)
    return {"role": role, "designs": r["designs"], "rejected_by": r["rejected_by"],
            "rival": prof["fighters"][0] if rival else None, "enemy": prof, "techs": sit.techs_source(),
            "method": "designer.best (engineer 106) with the enemy's best AI fighter template as the rival"}


# ---------------------------------------------------------------- ships

def best_ship(sit, role, top=3, beam=30):
    """Top hull designs for a ship role from the hulls and modules the country has; a hull whose modules all need a
    DLC that is not active is reported, not designed (the country then builds the base equipment)."""
    v = Vehicles.of(sit.game)
    bases, mods = designer.available(v.cat, sit.techs())
    weights = SHIP_ROLES[role]["value"]
    dlcs = sit.world.dlcs
    usable = {m for m in mods if m in v.modules and (not v.modules[m]["dlcs"] or v.modules[m]["dlcs"] & set(dlcs))}
    out, blocked = [], []
    want = SHIP_ROLES[role].get("types") or set()
    hulls = [b for b in bases if set(v.equip.get(v.family(b), {}).get("type") or ()) & want]
    for base in hulls:
        slots = v.slots(base)
        if not slots:
            continue
        if not any(v.modules[m]["category"] in s["categories"] for s in slots for m in usable):
            blocked.append(base)
            continue
        partial = [{}]
        for s in slots:
            opts = sorted(m for m in usable if v.modules[m]["category"] in s["categories"])
            nxt = []
            for p in partial:
                for m in ([] if s["required"] and opts else [None]) + opts:
                    q = dict(p)
                    if m:
                        q[s["slot"]] = m
                    nxt.append(q)
            sc = []
            for q in nxt:
                st = stats(v, base, q)
                val = sum(st.get(k, 0.0) * w for k, w in weights.items())
                sc.append((val / max(st.get("build_cost_ic", 1.0), 1.0) ** 0.5, q))
            sc.sort(key=lambda x: -x[0])
            partial = [q for _, q in sc[:beam]]
        for q in partial:
            st = stats(v, base, q)
            if [x for x in legal(v, base, q, None, dlcs) if "forbidden" not in x and "allowed" not in x]:
                continue
            val = sum(st.get(k, 0.0) * w for k, w in weights.items())
            out.append({"base": base, "modules": q, "stats": {k: round(x, 2) for k, x in st.items()
                                                               if k in weights or k in ("build_cost_ic", "naval_speed",
                                                                                        "reliability")},
                        "value": round(val, 2), "score": round(val / max(st.get("build_cost_ic", 1.0), 1.0) ** 0.5, 3)})
    out.sort(key=lambda d: -d["score"])
    seen, uniq = set(), []
    for d in out:  # the same modules in other free slots: one design
        key = (d["base"], tuple(sorted(d["modules"].values())))
        if key not in seen:
            seen.add(key)
            uniq.append(d)
    return {"role": role, "designs": uniq[:top], "blocked_hulls": sorted(blocked),
            "dlcs": list(dlcs or ()), "hull_types": sorted(want),
            "note": "ship weights are provisional (stat shares, not a naval combat model)" if out else
            "no designable hull: the modules need a DLC that is not active; build the base equipment"}


# ---------------------------------------------------------------- refresh trigger

def refresh_roles(sit, tech):
    """The roles whose best design may change when `tech` completes: it enables a base or module the role's bases can
    take, or raises an upgrade cap of such a base. The executor re-runs best() for these."""
    v = Vehicles.of(sit.game)
    blocks = dict(designer._tech_blocks(sit.game))
    body = blocks.get(tech) or []
    en_e, en_m = set(), set()
    for x in get(body, "enable_equipments"):
        en_e |= set(bare(x)) if isinstance(x, list) else set()
    for x in get(body, "enable_equipment_modules"):
        en_m |= set(bare(x)) if isinstance(x, list) else set()
    roles = set()
    tank_fams = {f for f, e in v.equip.items() if e["is_archetype"] and "armor" in e["type"] and e["slots"]}
    if any(v.family(e) in tank_fams for e in en_e):
        roles |= set(LAND_ROLES)
    cats = {v.modules[m]["category"] for m in en_m if m in v.modules}
    tank_cats = set()
    for f in tank_fams:
        for s in v.slots(f) or []:
            tank_cats |= set(s["categories"])
    for m in v.modules.values():
        for cs in m["opens"].values():
            if m["category"] in tank_cats:
                tank_cats |= set(cs)
    if cats & tank_cats:
        roles |= set(LAND_ROLES)
    for u in v.upgrades.values():
        if any(tech in t for t in u["req"].values()):
            roles |= set(LAND_ROLES)
    for role, spec in designer.ROLES.items():
        if spec["kind"] != "plane":
            continue
        fams = set(spec["families"])
        if any(v.cat.family(e) in fams for e in en_e if e in v.cat.equip):
            roles.add(role)
        if any(m in v.cat.modules for m in en_m) and any(c in (v.cat.modules.get(m, {}).get("category") or "")
                                                         for m in en_m for c in ("gun", "engine", "bomb", "fuel",
                                                                                 "torpedo", "armor", "weapon")):
            roles.add(role)
    return sorted(roles)


def best(sit, role, **kw):
    if role in LAND_ROLES:
        return best_land(sit, role, **kw)
    if role in SHIP_ROLES:
        return best_ship(sit, role, **{k: x for k, x in kw.items() if k in ("top",)})
    if role in designer.ROLES and designer.ROLES[role]["kind"] == "plane":
        return best_plane(sit, role, **{k: x for k, x in kw.items() if k in ("enemies", "need", "top")})
    raise KeyError(f"unknown role {role}: land {sorted(LAND_ROLES)}, ships {sorted(SHIP_ROLES)}, planes "
                   f"{sorted(r for r, s in designer.ROLES.items() if s['kind'] == 'plane')}")
