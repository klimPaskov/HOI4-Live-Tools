"""Division combat stats from the game's files: what a template fields on a date, battalion by battalion.

    python3 scripts/hoi4facts/divstats.py --tag GER --battalions infantry:9,artillery_brigade:1 \
        --support engineer,artillery --equipment infantry_equipment=infantry_equipment_1 [--techs a,b] [--json]

Reads, from the install and its mods (inventory from the files, nothing typed in):
  common/units                 battalions and support companies: combat width, HP (max_strength), organisation,
                               the equipment they need, their own stat factors (an engineer's soft_attack = -0.5 is
                               -50% of its equipment's soft attack), types and categories
  common/units/equipment       equipment stats, archetype then parent then the variant's own fields; tank and
                               plane chassis with designer modules (add_stats, multiply_stats, the chassis'
                               module_add_stats_multiplier) and upgrades (upgrades/*.txt add_stats per level)
  common/technologies          every researched technology's unit and category blocks (soft_attack = 0.05 on
                               category_artillery = +5% to every artillery battalion)
  common/doctrines             the grand doctrine's and the chosen subdoctrines' unit and category blocks (mastery
                               rewards only with --rewards)
  history/countries            the tag's start technologies, doctrines and equipment variants (create_equipment_variant)

Division rules (the game's division view): soft attack, hard attack, air attack, defence and breakthrough are the sum
of the battalions'; HP is the sum of max_strength; organisation the mean of the battalions' max_organisation (support
companies included) plus flat organisation from technologies and doctrines; armour 30% of the highest battalion
armour + 70% of the mean over the combat battalions; piercing 40% of the highest + 60% of the mean over all
battalions; combat width the sum of the combat battalions'. A battalion's stat = the sum over its needed equipment of
that equipment's stat x (1 + the battalion's own factor) x (1 + technology and doctrine factors on the battalion or
its categories). Combat modifiers outside the template (terrain, experience, planning, commander, country, air) are
left out: they are the battle's, not the division's.

Stdlib only, Python 3.9+.
"""
import argparse
import json
import os
import sys

if __package__ in (None, ""):
    sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
    from hoi4facts.script import parse_file, get1, bare  # noqa: E402
else:
    from .script import parse_file, get1, bare

STATS = ("soft_attack", "hard_attack", "air_attack", "defense", "breakthrough", "ap_attack", "armor_value",
         "max_strength", "max_organisation", "hardness")
SUMMED = ("soft_attack", "hard_attack", "air_attack", "defense", "breakthrough")
ROWS = ("width", "hp", "organisation", "soft_attack", "hard_attack", "defense", "breakthrough", "armor", "piercing",
        "hardness", "air_attack")
# logistics: equipment maximum_speed (km/h) and fuel_consumption; a sub-unit's supply_consumption (per battalion),
# supply_consumption_factor (a support company's, on the whole division) and maximum_speed (a factor on its equipment)
LOGISTIC = ("maximum_speed", "fuel_consumption", "supply_consumption", "supply_consumption_factor")
LOGI_ROWS = ("speed", "supply", "fuel")


def _num(v, default=None):
    try:
        return float(v)
    except (TypeError, ValueError):
        return default


def _stats(block, keys=STATS):
    return {k: _num(v) for k, v in block if k in keys and _num(v) is not None} if isinstance(block, list) else {}


class Data:
    """The game's unit, equipment, module, upgrade, technology and doctrine definitions, read once."""

    def __init__(self, game):
        self.game = game
        self.units = {}
        self.equipment = {}
        self.modules = {}
        self.upgrades = {}
        self.techs = {}
        self.doctrines = {}
        self.enables = {}
        self._load()

    def _files(self, folder, recursive=False):
        return self.game.files(folder, recursive=recursive)

    def _load(self):
        for f in self._files("common/units"):
            for k, v in parse_file(f):
                if k != "sub_units" or not isinstance(v, list):
                    continue
                for name, body in v:
                    if not name or not isinstance(body, list):
                        continue
                    need = get1(body, "need")
                    cats = get1(body, "categories")
                    typ = get1(body, "type")
                    self.units[name] = {
                        "width": _num(get1(body, "combat_width"), 0.0),
                        "support": get1(body, "group") == "support",
                        "need": {a: _num(b, 0.0) for a, b in need if a} if isinstance(need, list) else {},
                        "factors": {s: _num(get1(body, s)) for s in SUMMED + ("ap_attack", "armor_value")
                                    if _num(get1(body, s)) is not None},
                        "max_strength": _num(get1(body, "max_strength"), 0.0),
                        "max_organisation": _num(get1(body, "max_organisation"), 0.0),
                        "categories": set(bare(cats)) if isinstance(cats, list) else set(),
                        "types": set(bare(typ)) if isinstance(typ, list) else set(),
                        "logistic": _stats(body, LOGISTIC),
                        "transport": get1(body, "transport"),
                        "affects_speed": get1(body, "affects_speed") != "no",
                        "terrain_move": {t: _num(get1(tb, "movement")) for t, tb in body
                                         if isinstance(tb, list) and _num(get1(tb, "movement")) is not None},
                    }
        for f in self._files("common/units/equipment"):
            for k, v in parse_file(f):
                if k != "equipments" or not isinstance(v, list):
                    continue
                for name, body in v:
                    if not name or not isinstance(body, list):
                        continue
                    mult = {}
                    for kk, vv in body:
                        if kk == "module_add_stats_multiplier" and isinstance(vv, list):
                            for s, x in vv:
                                if _num(x) is not None:
                                    mult[s] = _num(x)
                    self.equipment[name] = {
                        "stats": _stats(body), "logistic": _stats(body, LOGISTIC),
                        "archetype": get1(body, "archetype"),
                        "year": _num(get1(body, "year"), 0.0),
                        "parent": get1(body, "parent"), "type": get1(body, "type"),
                        "is_archetype": get1(body, "is_archetype") == "yes", "module_mult": mult}
        for f in self._files("common/units/equipment/modules"):
            for k, v in parse_file(f):
                if k != "equipment_modules" or not isinstance(v, list):
                    continue
                for name, body in v:
                    if not name or not isinstance(body, list):
                        continue
                    self.modules[name] = {"add": _stats(get1(body, "add_stats")),
                                          "mult": _stats(get1(body, "multiply_stats")),
                                          "category": get1(body, "category")}
        for f in self._files("common/units/equipment/upgrades"):
            for k, v in parse_file(f):
                if k != "upgrades" or not isinstance(v, list):
                    continue
                for name, body in v:
                    if not name or not isinstance(body, list):
                        continue
                    self.upgrades[name] = {"add": _stats(get1(body, "add_stats")), "mult": _stats(body)}
        for f in self._files("common/technologies"):
            for k, v in parse_file(f):
                if k != "technologies" or not isinstance(v, list):
                    continue
                for name, body in v:
                    if name and isinstance(body, list):
                        self.techs[name] = self._unit_blocks(body)
                        en = get1(body, "enable_equipments")
                        if isinstance(en, list):
                            self.enables[name] = list(bare(en))
        for f in self._files("common/doctrines", recursive=True):
            for name, body in parse_file(f):
                if not name or not isinstance(body, list):
                    continue
                rewards = {}
                rw = get1(body, "rewards")
                if isinstance(rw, list):
                    for rk, rb in rw:
                        if rk and isinstance(rb, list):
                            rewards[rk] = self._unit_blocks(rb)
                self.doctrines[name] = {"base": self._unit_blocks(body), "rewards": rewards}

    def _unit_blocks(self, body):
        """[(target, {stat: factor})]: blocks keyed by a battalion or a category (category_*) with stat factors."""
        out = []
        for k, v in body:
            if not k or not isinstance(v, list):
                continue
            if k in self.units or k.startswith("category_"):
                st = _stats(v, STATS + LOGISTIC)
                if st:
                    out.append((k, st))
        return out

    # equipment ------------------------------------------------------------------------------------------------
    def chain(self, key):
        """The variant, its parents, then its archetype: the order in which fields are looked up."""
        out, seen, k = [], set(), key
        while k and k in self.equipment and k not in seen:
            seen.add(k)
            out.append(k)
            k = self.equipment[k]["parent"]
        arch = self.equipment.get(key, {}).get("archetype")
        if arch and arch in self.equipment and arch not in seen:
            out.append(arch)
        return out

    def archetype(self, key):
        e = self.equipment.get(key) or {}
        return key if e.get("is_archetype") else e.get("archetype") or key

    def newest(self, archetype, techs):
        """The newest variant (by year) of an archetype that the researched technologies enable."""
        got = {e for t in techs for e in self.enables.get(t) or ()}
        best = None
        for k, e in self.equipment.items():
            if e.get("is_archetype") or self.archetype(k) != archetype or k not in got:
                continue
            if best is None or e["year"] > self.equipment[best]["year"]:
                best = k
        return best

    def base_stats(self, key):
        out = {}
        for k in reversed(self.chain(key)):
            out.update(self.equipment[k]["stats"])
        return out

    def base_logistic(self, key):
        out = {}
        for k in reversed(self.chain(key)):
            out.update(self.equipment[k].get("logistic") or {})
        return out

    def module_mult(self, key):
        out = {}
        for k in reversed(self.chain(key)):
            out.update(self.equipment[k]["module_mult"])
        return out

    def design_stats(self, chassis, modules=None, upgrades=None):
        """A designed vehicle or plane: chassis + module add_stats (attack scaled by the chassis'
        module_add_stats_multiplier) + upgrade add_stats x level, then x (1 + the summed multiply_stats)."""
        st = dict(self.base_stats(chassis))
        mm = self.module_mult(chassis)
        mult = {}
        for m in (modules or {}).values():
            md = self.modules.get(m)
            if not md:
                continue
            for s, x in md["add"].items():
                st[s] = st.get(s, 0.0) + x * (1 + mm.get(s, 0.0))
            for s, x in md["mult"].items():
                mult[s] = mult.get(s, 0.0) + x
        for u, lvl in (upgrades or {}).items():
            ud = self.upgrades.get(u)
            if not ud:
                continue
            for s, x in ud["add"].items():
                st[s] = st.get(s, 0.0) + x * float(lvl)
            for s, x in ud["mult"].items():
                if s in STATS and "add" in ud and not ud["add"]:
                    mult[s] = mult.get(s, 0.0) + x * float(lvl)
        return {s: v * (1 + mult.get(s, 0.0)) for s, v in st.items()}


def history_start(data, tag):
    """(techs, grand and sub doctrines, {variant name: {type, modules, upgrades}}) from the tag's history file,
    every dated and DLC block included (an upper bound on what the start gives)."""
    techs, docs, variants = set(), [], {}
    for f in data.game.files("history/countries"):
        if not os.path.basename(f).startswith(tag + " "):
            continue

        def walk(items):
            for k, v in items:
                if k == "set_technology" and isinstance(v, list):
                    techs.update(t for t, x in v if t and str(x) == "1")
                elif k in ("set_grand_doctrine", "set_sub_doctrine") and isinstance(v, str):
                    docs.append(v)
                elif k == "create_equipment_variant" and isinstance(v, list):
                    mods = get1(v, "modules")
                    ups = get1(v, "upgrades")
                    variants[str(get1(v, "name", "")).strip('"')] = {
                        "type": get1(v, "type"),
                        "modules": {a: b for a, b in mods if a} if isinstance(mods, list) else {},
                        "upgrades": {a: _num(b, 0) for a, b in ups if a} if isinstance(ups, list) else {}}
                elif isinstance(v, list):
                    walk(v)
        walk(parse_file(f))
    return techs, list(dict.fromkeys(docs)), variants


def factors(data, techs, doctrines, rewards=()):
    """[(target, {stat: factor})] of the researched technologies and the doctrines (base blocks; the named rewards)."""
    out = []
    for t in techs:
        out.extend(data.techs.get(t) or [])
    for d in doctrines:
        dd = data.doctrines.get(d)
        if not dd:
            continue
        out.extend(dd["base"])
        for r, blocks in dd["rewards"].items():
            if rewards == "all" or r in (rewards or ()):
                out.extend(blocks)
    return out


def battalion(data, unit, equipment, mods):
    """{stat: value} of one battalion. equipment: {archetype: variant key or design stats dict}."""
    u = data.units[unit]
    targets = {unit} | u["categories"]
    pct, flat = {}, {}
    for target, st in mods:
        if target in targets:
            for s, x in st.items():
                if s == "max_organisation":
                    flat[s] = flat.get(s, 0.0) + x
                else:
                    pct[s] = pct.get(s, 0.0) + x
    out = {s: 0.0 for s in SUMMED + ("ap_attack", "armor_value", "hardness")}
    used = []
    for need in u["need"]:
        arch = data.archetype(need)
        pick = equipment.get(arch) or equipment.get(need)
        if isinstance(pick, dict):
            st = pick
            used.append(f"{arch}=design")
        else:
            key = pick or need
            st = data.base_stats(key)
            used.append(f"{arch}={key}")
        for s in SUMMED + ("ap_attack",):
            out[s] += st.get(s, 0.0)
        out["armor_value"] = max(out["armor_value"], st.get("armor_value", 0.0))
        out["hardness"] = max(out["hardness"], st.get("hardness", 0.0))
    for s in SUMMED + ("ap_attack", "armor_value"):
        out[s] *= (1 + u["factors"].get(s, 0.0)) * (1 + pct.get(s, 0.0))
    out["max_strength"] = u["max_strength"] * (1 + pct.get("max_strength", 0.0))
    out["max_organisation"] = u["max_organisation"] * (1 + pct.get("max_organisation_factor", 0.0)) \
        + flat.get("max_organisation", 0.0)
    out["width"] = 0.0 if u["support"] else u["width"]
    out["support"] = u["support"]
    out["equipment"] = used
    out.update(_logistic(data, u, equipment, pct))
    out["affects_speed"] = u.get("affects_speed", True)
    return out


def _logistic(data, u, equipment, pct):
    """speed: the transport's maximum_speed when the sub-unit names one, else the slowest needed equipment's, x (1 +
    the sub-unit's own maximum_speed factor + technology and doctrine factors); supply: its supply_consumption plus the
    technology and doctrine supply_consumption on it (a flat amount per battalion [?: the files give small negative
    numbers such as -0.02 against a base of 0.32]); supply_factor: its supply_consumption_factor plus technology
    factors (a support company's, applied to the whole division); fuel: the needed equipment's fuel_consumption x
    the amount needed (the files' figure; the game's time unit for it is not verified [?])."""
    speeds, fuel = {}, 0.0
    for need, amount in u["need"].items():
        arch = data.archetype(need)
        pick = equipment.get(arch) or equipment.get(need)
        lg = dict(pick) if isinstance(pick, dict) else data.base_logistic(pick or need)
        if lg.get("maximum_speed"):
            speeds[arch] = lg["maximum_speed"]
        fuel += float(lg.get("fuel_consumption") or 0.0) * float(amount or 0.0)
    own = u.get("logistic") or {}
    tr = u.get("transport")
    if tr:
        pick = equipment.get(data.archetype(tr)) or equipment.get(tr)
        lg = dict(pick) if isinstance(pick, dict) else data.base_logistic(pick or tr)
        base = lg.get("maximum_speed") or (min(speeds.values()) if speeds else 0.0)
    else:
        base = min(speeds.values()) if speeds else 0.0
    speed = base * (1 + own.get("maximum_speed", 0.0) + pct.get("maximum_speed", 0.0))
    return {"speed": speed, "supply": max(0.0, own.get("supply_consumption", 0.0) + pct.get("supply_consumption", 0.0)),
            "supply_factor": own.get("supply_consumption_factor", 0.0) + pct.get("supply_consumption_factor", 0.0),
            "fuel": fuel}


def division(data, battalions, support, equipment, mods):
    """The division view's rows for a template: battalions [unit, ...] (repeats count), support [unit, ...]."""
    rows = [battalion(data, b, equipment, mods) for b in list(battalions) + list(support)]
    if not rows:
        return {}
    line = [r for r in rows if not r["support"]]
    out = {s: round(sum(r[s] for r in rows), 1) for s in SUMMED}
    out["hp"] = round(sum(r["max_strength"] for r in rows), 1)
    out["organisation"] = round(sum(r["max_organisation"] for r in rows) / len(rows), 1)
    out["width"] = round(sum(r["width"] for r in rows), 1)
    arm = [r["armor_value"] for r in (line or rows)]
    out["armor"] = round(0.3 * max(arm) + 0.7 * sum(arm) / len(arm), 1)
    prc = [r["ap_attack"] for r in rows]
    out["piercing"] = round(0.4 * max(prc) + 0.6 * sum(prc) / len(prc), 1)
    hard = [r["hardness"] for r in (line or rows)]
    out["hardness"] = round(sum(hard) / len(hard), 3)
    # logistics: the division moves at its slowest sub-unit, except those marked affects_speed = no (most support
    # companies); supply = the battalions' and companies' supply consumption x (1 + the companies'
    # supply_consumption_factor); fuel summed
    sp = [r["speed"] for r in rows if r.get("speed") and r.get("affects_speed", True)]
    out["speed"] = round(min(sp), 2) if sp else 0.0
    out["supply"] = round(sum(r["supply"] for r in rows) * max(0.0, 1 + sum(r["supply_factor"] for r in rows)), 3)
    out["fuel"] = round(sum(r["fuel"] for r in rows), 1)
    return out


def parse_counts(text):
    """'infantry:9,artillery_brigade' -> [infantry x9, artillery_brigade]."""
    out = []
    for part in (text or "").split(","):
        part = part.strip()
        if not part:
            continue
        name, _, n = part.partition(":")
        out.extend([name] * int(n or 1))
    return out


def template_units(tpl):
    """(battalions, support) of a plan `templates[]` entry (regiments as columns, or {unit: count})."""
    regs = tpl.get("regiments") or tpl.get("battalions") or []
    bats = []
    if isinstance(regs, dict):
        for k, n in regs.items():
            bats += [k] * int(n)
    else:
        for col in regs:
            bats += list(col) if isinstance(col, list) else [col]
    sup = tpl.get("support") or []
    if isinstance(sup, dict):
        sup = [k for k, n in sup.items() for _ in range(int(n))]
    return bats, list(sup)


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("--game", default=os.environ.get("HOI4_DIR", os.path.expanduser(
        "~/Library/Application Support/Steam/steamapps/common/Hearts of Iron IV")))
    ap.add_argument("--tag", help="start technologies, doctrines and variants from this tag's history")
    ap.add_argument("--battalions", required=True, help="unit:count,... (combat battalions)")
    ap.add_argument("--support", default="", help="support companies, comma separated")
    ap.add_argument("--equipment", default="", help="archetype=variant,... (default: the archetype's newest the "
                                                    "history or --techs give is NOT guessed: name it)")
    ap.add_argument("--design", action="append", default=[],
                    help="archetype=history variant name (a create_equipment_variant of --tag)")
    ap.add_argument("--techs", default="", help="extra researched technologies, comma separated")
    ap.add_argument("--doctrines", default=None, help="doctrines (default: the tag's history)")
    ap.add_argument("--rewards", default="", help="'all' or doctrine reward names, comma separated")
    ap.add_argument("--json", action="store_true")
    a = ap.parse_args(argv)
    lint_plan = __import__("hoi4facts.lint_plan", fromlist=["load_game"]) if __package__ in (None, "") else \
        __import__((__package__ or "hoi4facts") + ".lint_plan", fromlist=["load_game"])
    data = Data(lint_plan.load_game(a.game))
    techs, docs, variants = history_start(data, a.tag) if a.tag else (set(), [], {})
    techs |= {t for t in a.techs.split(",") if t}
    if a.doctrines is not None:
        docs = [d for d in a.doctrines.split(",") if d]
    equipment = dict(p.split("=", 1) for p in a.equipment.split(",") if "=" in p)
    for d in a.design:
        arch, _, name = d.partition("=")
        v = variants.get(name)
        if not v:
            raise SystemExit(f"--design {d}: no variant {name!r} in {a.tag}'s history")
        equipment[arch] = data.design_stats(v["type"], v["modules"], v["upgrades"])
    rewards = "all" if a.rewards == "all" else [r for r in a.rewards.split(",") if r]
    mods = factors(data, techs, docs, rewards)
    out = division(data, parse_counts(a.battalions), [s for s in a.support.split(",") if s], equipment, mods)
    if a.json:
        print(json.dumps(out, indent=1))
    else:
        print("  ".join(f"{k} {out[k]}" for k in ROWS if k in out))
    return 0


if __name__ == "__main__":
    sys.exit(main())
