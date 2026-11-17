"""Force design: what each war the plan plans needs, derived from the plan's own wars and the game files.

lint_force.py checks the plan's force table (`force_design`), training, production and air plan against it.
Nothing here is a taste: every need comes from a rule of the playbook with the game files' numbers, and every
enemy figure is an aggregate of its start order of battle (history/units), which the FAIR boundary allows.

A war is a group of declarations the plan dates on one day (offensive), or the war the country is in at the
start, or the enemies `war.enemies` names that no declaration covers (the enemy declares). Its kind is
offensive when the plan declares it, or when it runs from the start with a posture other than defend.

Per war the derivation gives:
  theatre       the states the war is fought over: the plan's vp_targets and strike axes in the targets' land,
                else the targets' states on our border, else all their states; its terrain mix.
  enemy side    the targets, the joiners the declaration accepts that border a target, and the enemies of
                earlier wars the plan does not see capitulated by then (a capitulated:<TAG> checkpoint dated
                before, or a capitulated:<TAG> atom in the declaration's own `when`). Aggregates of their start
                OOBs: divisions, front-line combat width, tank battalions, planes by kind, surface ships.
  front         the targets and the bordering joiners only: an earlier war's enemy that is not a target keeps
                its divisions and planes on its own land. The line, the breakthrough and the air are sized on it.
  breakthrough  (offensive wars) a defended line stands when the front's line width fills the combat width of
                every strike axis LINE_DEPTH times over (common/terrain plains width): then the war needs
                MOBILE_PER_AXIS mobile (armoured or motorised/mechanised) divisions per strike axis and at least
                MOBILE_PER_ENEMY of the front's divisions, half of them (and ARMOUR_PER_AXIS per axis) armoured,
                by the war's first day (germany-fair.md §7: 6-8 tank or motorised divisions for the three Polish
                axes against 40 divisions; §10: 25+ against the Soviet Union; war-air-naval-peace.md §12: armour
                spearheads in each axis). Affordable when the units' techs are owned or researched by then and
                their equipment costs at most BREAK_SHARE of the military output from the start to the war.
  air           a first-class arm, inside AIR_SHARE of the military output to the war's first day (fighters
                first): fighters FIGHTER_RATIO x the front's start fighters (procedure item 20: about 1.5x the
                enemy's fighters over the decisive region; war-air-naval-peace.md §5.1: superiority first); once
                they are paid, close air support over the breakthrough axes, one wing an axis; transports when the
                plan fields paratroopers; anti-air where the plan cannot win the air. Below MIN_PLANES a row is no
                air arm and the plan says why (force_design.air_why). Enemy air is its start air force from the
                history files: the start lines are one-factory placeholders, so no growth is estimated.
  anti-tank     line divisions carry anti-tank once the front fields AT_ENEMY_ARMOUR tank battalions.
  mountains     mountains and hills a third of the theatre: mountaineers or narrow line divisions.
  invasions     per planned landing: fighters over the target owner's start fighters x FIGHTER_RATIO, and
                naval strike planes when the owner's surface fleet outweighs ours SEA_RATIO times
                (war-air-naval-peace.md §7: air superiority, the enemy fleet neutralised by naval bombers).

Unit classes come from common/units categories (category_tanks, category_vehicle_infantry, ...), equipment
kinds from common/units/equipment types (and the duplicate-archetype names of the plane designer), so mods
that keep the vanilla categories work unchanged.
"""
import re
from collections import Counter

from . import derive

MOBILE_PER_AXIS = 2      # armoured or motorised divisions per strike axis (germany-fair.md §7)
ARMOUR_PER_AXIS = 1      # of them armoured (war-air-naval-peace.md §12.1-12.4: armour spearheads)
MOBILE_PER_ENEMY = 0.15  # and per enemy division at the start: germany-fair.md §7 asks 6-8 against Poland's 40,
                         # §10 25+ against the Soviet Union's 138 (15-18%)
LINE_DEPTH = 2           # a defended line: enemy front-line width >= axes x plains width x this
FIGHTER_RATIO = 1.5      # procedure item 20, war-air-naval-peace.md §5.1
BREAK_SHARE = 0.5        # a breakthrough force the plan cannot pay with this share of it
AT_ENEMY_ARMOUR = 10     # enemy tank battalions from which line divisions need anti-tank
SEA_RATIO = 1.5          # the enemy's surface fleet this many times ours: the sea is won from the air
WING_PLANES = 100        # one air wing (war-air-naval-peace.md §5; the wing size the air tools use)
AIR_SHARE = 0.35         # of the military output to a war's first day the air force may take (fighters first)
MIN_PLANES = 20          # an air row below this is no air arm: the plan says why (force_design.air_why)
SINGLE_SHARE = 0.85      # one template carrying more than this share of the army
LOGISTICS_DAYS = 120     # a war planned longer than this needs tactical bombers on logistics strike (§5.1, AI11)
STRATEGIC_DAYS = 365     # strategic bombing only in a war longer than a year (AI14)
STRATEGIC_PLANES = 1000  # about 1000 bombers beat the repairs (AI14)
DECISIVE_DAYS = 120      # a war planned longer than this needs its reasons (force.decisive)

PLANE_KINDS = ("fighter", "heavy_fighter", "cas", "tactical_bomber", "naval_bomber", "strategic_bomber",
               "transport", "scout")
SURFACE = {"capital_ship", "carrier", "screen_ship"}


# ---------------------------------------------------------------------------------------------- units
def unit_classes(game, unit):
    """{"armour", "mobile", "line", "cavalry", "mountain", "marine", "para", "artillery", "anti_tank",
    "anti_air", "engineer", "support", "front"} that apply to a sub-unit (common/units categories)."""
    ut = derive.unit_types(game).get(unit) or {}
    cats = ut.get("categories") or set()
    out = set()
    if ut.get("support") or ut.get("regimental"):
        out.add("support")
    if "category_tanks" in cats:
        out.add("armour")
    elif "category_vehicle_infantry" in cats or ("category_mobile_and_mobile_combat_sup" in cats
                                                 and "category_cavalry" not in cats and not ut.get("support")):
        out.add("mobile")
    if "category_cavalry" in cats:
        out.add("cavalry")
    if "category_mountaineers" in cats:
        out.add("mountain")
    if "category_marines" in cats or "category_marines_and_amphibious" in cats:
        out.add("marine")
    if "category_paratroopers" in cats:
        out.add("para")
    if cats & {"category_artillery", "category_line_artillery", "category_self_propelled_artillery"} and \
            "category_anti_tank" not in cats and "category_anti_air" not in cats:
        out.add("artillery")
    if cats & {"category_anti_tank", "category_tank_destroyers"} or "anti_tank" in unit:
        out.add("anti_tank")
    if "category_anti_air" in cats or "anti_air" in unit:
        out.add("anti_air")
    if "engineer" in unit:
        out.add("engineer")
    if "category_front_line" in cats and not ut.get("support"):
        out.add("front")
    if "category_all_infantry" in cats and not out & {"mobile", "armour", "marine", "para", "mountain", "support"}:
        out.add("line")
    return out


def template_profile(game, units):
    """{"kind": armour | mobile | marine | mountain | para | cavalry | line | none, "classes": Counter over
    the units, "width": front width} of a template given as a list of units (one entry per battalion or
    company)."""
    ut = derive.unit_types(game)
    cls = Counter()
    width = 0.0
    battalions = 0
    for u in units:
        c = unit_classes(game, u)
        for x in c:
            cls[x] += 1
        if "support" not in c:
            battalions += 1
            width += (ut.get(u) or {}).get("width") or 0.0
    if cls["armour"]:
        kind = "armour"
    elif battalions and cls["mobile"] * 2 >= battalions:
        kind = "mobile"
    elif cls["marine"]:
        kind = "marine"
    elif cls["para"]:
        kind = "para"
    elif cls["mountain"] * 2 >= max(1, battalions):
        kind = "mountain"
    elif cls["cavalry"] * 2 >= max(1, battalions):
        kind = "cavalry"
    elif battalions:
        kind = "line"
    else:
        kind = "none"
    return {"kind": kind, "classes": cls, "width": width}


def is_mobile(kind):
    return kind in ("armour", "mobile")


# ---------------------------------------------------------------------------------------------- equipment
def plane_kind(game, key):
    """fighter, heavy_fighter, cas, tactical_bomber, naval_bomber, strategic_bomber, transport, scout, or None:
    from the equipment's types (common/units/equipment), else the generated plane-designer family name."""
    types = set()
    for k in (key, derive.equipment_family(game, key)):
        types |= set(derive.equipment_types(game).get(k) or ())
    name = (key or "").lower()
    if "naval_bomber" in types or "naval_bomber" in name or "nav_bomber" in name:
        return "naval_bomber"
    if "cas" in types or "_cas_" in name or name.startswith("cas_"):
        return "cas"
    if "scout" in name or "scout_plane" in types:
        return "scout"
    if "air_transport" in types or "transport_plane" in name:
        return "transport"
    if "medium_plane_fighter" in name or "heavy_fighter" in name:
        return "heavy_fighter"
    if "fighter" in types or name.startswith("fighter_equipment") or name.startswith("small_plane_airframe"):
        return "fighter"
    if "tactical_bomber" in types:
        return "tactical_bomber"
    if "strategic_bomber" in types:
        return "strategic_bomber"
    if "cv_" in name and "fighter" in name:
        return "fighter"
    return None


def ship_class(game, key):
    """capital_ship, carrier, screen_ship, submarine, convoy or None (common/units/equipment types)."""
    types = set()
    for k in (key, derive.equipment_family(game, key)):
        types |= set(derive.equipment_types(game).get(k) or ())
    for t in ("carrier", "capital_ship", "submarine", "convoy", "screen_ship"):
        if t in types:
            return t
    return None


def _variant_cost(world, v, base_cost=None):
    rows = derive.equipment_rows(world.game)
    mods = world.equipment_modules()
    base = base_cost if base_cost is not None else (rows.get(v.get("type")) or {}).get("cost")
    if not base:
        return None
    add, mult = 0.0, 1.0
    for m in (v.get("modules") or {}).values() if isinstance(v.get("modules"), dict) else []:
        r = mods.get(m) or {}
        a, mm = r.get("add_stats") or {}, r.get("multiply_stats") or {}
        add += float(a.get("build_cost_ic") or 0) if isinstance(a, dict) else 0.0
        mult *= 1.0 + (float(mm.get("build_cost_ic") or 0) if isinstance(mm, dict) else 0.0)
    return (base + add) * mult


def unit_cost(lint, key):
    """IC of one unit of an equipment the plan produces: a plan design with modules, else the start variant of
    that equipment, else its chassis or airframe cost plus the mean module cost of the start variants of the
    same family (then of the same designer), else the build_cost_ic of the equipment row. (cost, how)."""
    cache = lint.__dict__.setdefault("_unit_costs", {})
    if key in cache:
        return cache[key]
    g = lint.game
    rows = derive.equipment_rows(g)
    row_cost = (rows.get(key) or {}).get("cost")
    out = (row_cost, "build_cost_ic")
    w = lint.world if lint.tag else None
    if w is None or not row_cost:
        cache[key] = out
        return out
    try:
        variants = [v for v in w.country(lint.tag).get("variants") or [] if v.get("type")]
        kind = w.eq_kind(key) if key in w.equipment_defs() else None
    except Exception:  # noqa: BLE001 - a game without designer data: the row cost stands
        cache[key] = out
        return out
    if kind not in ("tank", "plane", "ship"):
        cache[key] = out
        return out
    designs = {d.get("id"): d for d in lint.list_of("designs")}
    used = [designs.get(x.get("design")) for x in lint.list_of("production") if x.get("equipment") == key]
    used += [d for d in designs.values() if d.get("production") == key or d.get("base") == key]
    for d in used:
        if isinstance(d, dict) and isinstance(d.get("modules"), dict) and d["modules"]:
            c = _variant_cost(w, {"type": d.get("base") or key, "modules": d["modules"]},
                              (rows.get(d.get("base")) or {}).get("cost") or row_cost)
            if c:
                cache[key] = (round(c, 2), f"design {d.get('id')}")
                return cache[key]
    same = [v for v in variants if v["type"] == key]
    if same:
        c = _variant_cost(w, same[-1])
        if c:
            cache[key] = (round(c, 2), f"start variant {same[-1].get('v')}")
            return cache[key]
    fam = derive.equipment_family(g, key)
    for label, pool in (("family", [v for v in variants if derive.equipment_family(g, v["type"]) == fam]),
                        ("designer", [v for v in variants if v["type"] in w.equipment_defs()
                                      and w.eq_kind(v["type"]) == kind])):
        extra = []
        for v in pool:
            c, b = _variant_cost(w, v), (rows.get(v["type"]) or {}).get("cost")
            if c and b:
                extra.append(c - b)
        if extra:
            c = row_cost + sum(extra) / len(extra)
            cache[key] = (round(c, 2), f"{label} start variants' mean module cost")
            return cache[key]
    cache[key] = out
    return out


# ---------------------------------------------------------------------------------------------- enemies
def enemy_aggregate(lint, tag):
    """Start-OOB aggregate of one country: divisions, front-line width, tank battalions, planes by kind,
    surface ships and their hull cost. FAIR: aggregates only, as dossier §3 shows them."""
    cache = lint.__dict__.setdefault("_enemy_agg", {})
    if tag in cache:
        return cache[tag]
    g = lint.game
    out = {"tag": tag, "divisions": 0, "width": 0.0, "tank_battalions": 0, "planes": Counter(), "surface": 0,
           "surface_ic": 0, "submarines": 0}
    try:
        o = lint.world.oob(tag)
    except Exception:  # noqa: BLE001 - a tag without history: nothing known
        cache[tag] = out
        return out
    ut = derive.unit_types(g)
    rows = derive.equipment_rows(g)
    for d in o["divisions"]:
        t = o["templates"].get(d["template"]) or {}
        out["divisions"] += 1
        for u, n in (t.get("regiments") or {}).items():
            c = unit_classes(g, u)
            if "support" in c:
                continue
            out["width"] += ((ut.get(u) or {}).get("width") or 0.0) * n
            if "armour" in c:
                out["tank_battalions"] += n
    for a in o["air"]:
        k = plane_kind(g, a["type"]) or "other"
        out["planes"][k] += int(a.get("amount") or 0)
    for f in o["fleets"]:
        for hull, n in (f.get("ships") or {}).items():
            cls = ship_class(g, hull)
            if cls in SURFACE:
                out["surface"] += n
                out["surface_ic"] += round(((rows.get(hull) or {}).get("cost") or 0.0) * n)
            elif cls == "submarine":
                out["submarines"] += n
    out["planes"] = dict(out["planes"])
    cache[tag] = out
    return out


def side_aggregate(lint, tags):
    rows = [enemy_aggregate(lint, t) for t in tags]
    planes = Counter()
    for r in rows:
        planes.update(r["planes"])
    return {"tags": list(tags), "divisions": sum(r["divisions"] for r in rows),
            "width": round(sum(r["width"] for r in rows), 1),
            "tank_battalions": sum(r["tank_battalions"] for r in rows), "planes": dict(planes),
            "surface": sum(r["surface"] for r in rows), "surface_ic": round(sum(r["surface_ic"] for r in rows)),
            "submarines": sum(r["submarines"] for r in rows)}


def ai_growth_rate():
    """Divisions a year per start military factory the game's own AI added in peace, measured in this install's
    baselines (skills/hoi4-campaign-playbook/baselines/*.json, rows up to `peace_until`): the rate the start
    order of battle of an enemy is grown by to the war's first day. None without a baseline."""
    import glob
    import json
    import os
    here = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
    rates = []
    for path in sorted(glob.glob(os.path.join(here, "skills", "hoi4-campaign-playbook", "baselines", "*.json"))):
        try:
            with open(path, encoding="utf-8") as fh:
                b = json.load(fh)
        except (OSError, ValueError):
            continue
        until = b.get("peace_until")
        rows = [r for r in b.get("rows") or [] if until is None or r.get("date", "") <= until]
        if len(rows) < 2 or not rows[0].get("military_factories"):
            continue
        d0, d1 = derive.day(rows[0]["date"]), derive.day(rows[-1]["date"])
        if d0 is None or d1 is None or d1 <= d0:
            continue
        rates.append((rows[-1]["divisions"] - rows[0]["divisions"]) / ((d1 - d0) / 365.0)
                     / rows[0]["military_factories"])
    return sum(rates) / len(rates) if rates else None


def start_military_factories(lint, tag):
    cache = lint.__dict__.setdefault("_start_mil", {})
    if tag not in cache:
        try:
            sb = derive.state_buildings(lint.game, lint.ctx, lint.start)
            cache[tag] = sum((sb.get(sid) or {}).get("arms_factory", 0) for sid, r in lint.world.states().items()
                             if r.get("owner") == tag)
        except Exception:  # noqa: BLE001
            cache[tag] = 0
    return cache[tag]


def grown(lint, side, day, grow=None):
    """The side's start aggregate with its land force grown to day at the AI's peace rate (ai_growth_rate): an
    estimate of the enemy the war meets, the floor of the start OOB kept. Adds divisions_est, width_est,
    tank_battalions_est and growth_why."""
    out = dict(side)
    rate = ai_growth_rate()
    start = derive.day(lint.start)
    if rate is None or day is None or start is None or day <= start or not side["divisions"]:
        out.update(divisions_est=side["divisions"], width_est=side["width"],
                   tank_battalions_est=side["tank_battalions"], growth_why="no growth estimate (no baseline)")
        return out
    years = (day - start) / 365.0
    tags = [t for t in side["tags"] if grow is None or t in grow]
    mil = sum(start_military_factories(lint, t) for t in tags)
    add = rate * mil * years
    f = (side["divisions"] + add) / side["divisions"]
    out.update(divisions_est=int(round(side["divisions"] + add)), width_est=round(side["width"] * f, 1),
               tank_battalions_est=int(round(side["tank_battalions"] * f)),
               growth_why=f"{side['divisions']} start divisions + {rate:.2f} a year per start military factory "
                          f"(the AI's peace rate in the baselines) x {mil} factories of {', '.join(tags)} x "
                          f"{years:.1f} years")
    return out


# ---------------------------------------------------------------------------------------------- dates
_CAP = re.compile(r"^capitulated:([A-Z][A-Z0-9]{2})$")
_WAR = re.compile(r"^at_war_with:([A-Z][A-Z0-9]{2})$")


def _atoms(pred):
    if isinstance(pred, str):
        yield pred.strip()
    elif isinstance(pred, list):
        for p in pred:
            yield from _atoms(p)
    elif isinstance(pred, dict):
        for k, p in pred.items():
            if k == "not":
                continue
            yield from _atoms(p)


def capitulation_days(lint):
    """{tag: earliest checkpoint day whose `must` names capitulated:<tag>}."""
    out = {}
    for c in lint.list_of("checkpoints"):
        d = derive.day(c.get("by")) if c.get("by") else None
        if d is None:
            continue
        for a in _atoms(c.get("must")):
            m = _CAP.match(a)
            if m:
                out[m.group(1)] = min(out.get(m.group(1), d), d)
    return out


def predicate_day(lint, tl, pred, wars=()):
    """The day a predicate first holds under the plan, as far as it can be dated: its date atoms, the start day
    of the war an at_war_with atom names, the checkpoint that dates a capitulated atom. None when nothing in it
    can be dated."""
    days = list(tl.lint_dates(pred)) if pred is not None else []
    caps = capitulation_days(lint)
    for a in _atoms(pred):
        m = _WAR.match(a)
        if m:
            for w in wars:
                if m.group(1) in w["enemy_tags"] and w.get("start") is not None:
                    days.append(w["start"])
                    break
        m = _CAP.match(a)
        if m and m.group(1) in caps:
            days.append(caps[m.group(1)])
    return max(days) if days else None


# ---------------------------------------------------------------------------------------------- wars
def plan_wars(lint, tl):
    """[{id, kind, start (day or None), targets, joiners, enemy_tags, axes, theatre, source}] in start order."""
    w = lint.world
    tag = lint.tag
    caps = capitulation_days(lint)
    start = tl.start
    groups = {}
    for i, e in enumerate(lint.list_of("diplomacy")):
        if e.get("action") not in ("declare", "declare_war") or not isinstance(e.get("target"), str):
            continue
        ds = tl.lint_dates(e["when"]) if e.get("when") is not None else []
        day = max(ds) if ds else None
        g = groups.setdefault(day if day is not None else f"undated-{i}", {
            "start": day, "targets": [], "joiners": set(), "source": [], "gone": set()})
        g["targets"].append(e["target"])
        g["joiners"] |= {j for j in e.get("joiners_ok") or [] if isinstance(j, str)}
        g["source"].append(f"diplomacy[{i}] {e.get('id') or e['target']}")
        g["gone"] |= {m.group(1) for a in _atoms(e.get("when")) for m in [_CAP.match(a)] if m}
    wars = []
    posture = (lint.war or {}).get("posture")
    at_start = w.at_war_with(tag) if tag else []
    if at_start:
        wars.append({"start": start, "targets": list(at_start), "joiners": set(), "source": ["at war at the start"],
                     "gone": set(), "kind": "defensive" if posture == "defend" else "offensive"})
    declared = {t for gr in groups.values() for t in gr["targets"]} | set(at_start)
    expected = [t for t in (lint.war or {}).get("enemies") or [] if isinstance(t, str) and t not in declared]
    if expected:
        wars.append({"start": tl.war_day, "targets": expected, "joiners": set(), "source": ["war.enemies (the enemy "
                     "declares)"], "gone": set(), "kind": "defensive"})
    for gr in groups.values():
        wars.append(dict(gr, kind="offensive"))
    wars.sort(key=lambda x: (x["start"] is None, x["start"] or 0))
    neighbours = {}

    def borders(a, b):
        if a not in neighbours:
            try:
                neighbours[a] = set(w.neighbours(a))
            except Exception:  # noqa: BLE001
                neighbours[a] = set()
        return b in neighbours[a]

    earlier = []
    out = []
    for n, x in enumerate(wars):
        targets = list(dict.fromkeys(x["targets"]))
        day = x["start"]
        joiners = sorted(j for j in x["joiners"] if j not in targets and any(borders(t, j) for t in targets))
        still = []
        for t in earlier:
            if t in targets or t in joiners or t in x["gone"]:
                continue
            cd = caps.get(t)
            if cd is not None and day is not None and cd <= day:
                continue
            still.append(t)
        enemy = list(dict.fromkeys(targets + joiners + still))
        earlier += [t for t in targets + sorted(x["joiners"]) if t not in earlier]
        wid = "war_" + "_".join(t.lower() for t in targets[:3])
        out.append({"id": wid, "kind": x["kind"], "start": day,
                    "start_date": derive.fmt(derive.date_of(day)) if day is not None else None,
                    "targets": targets, "joiners": joiners, "carried": still, "enemy_tags": enemy,
                    "source": x["source"]})
    for war in out:
        war["axes"], war["theatre"] = _axes_and_theatre(lint, war)
    return out


def _axes_and_theatre(lint, war):
    states = lint.world.states()
    targets = set(war["targets"])
    owner = lambda s: (states.get(s) or {}).get("owner")  # noqa: E731
    axes = 0
    theatre = set()
    for e in lint.list_of("army_layout"):
        ax = [s for s in e.get("axis") or [] if isinstance(s, int)]
        if e.get("role") == "strike" and ax and any(owner(s) in targets for s in ax):
            axes += 1
            theatre |= {s for s in ax if owner(s) in targets}
    for v in lint.list_of("vp_targets"):
        if isinstance(v.get("state"), int) and owner(v["state"]) in targets:
            theatre.add(v["state"])
    if not theatre:
        try:
            nb = lint.world.neighbours(lint.tag)
        except Exception:  # noqa: BLE001
            nb = {}
        for t in targets:
            theatre |= set((nb.get(t) or {}).get("states") or [])
    if not theatre:
        for t in targets:
            theatre |= set(lint.world.owned_states(t))
    return axes, sorted(theatre)


# ---------------------------------------------------------------------------------------------- needs
def plains_width(game):
    widths = derive.terrain_widths(game)
    return (widths.get("plains") or {}).get("width") or max([r["width"] for r in widths.values()] or [70])


def military_output(lint, tl, day):
    """IC the plan's military factories make from the start to day (the economy forecast's military factories
    x the factory speed x the mean line efficiency, rough: half the cap of each day)."""
    try:
        from . import economy as econmod
        rows = econmod.forecast(lint, tl)["rows"]
    except Exception:  # noqa: BLE001
        rows = []
    if not rows:
        return None
    total = 0.0
    d = tl.start
    while d < day:
        r = econmod.at(rows, d)
        if r:
            _, cap, _, _ = lint.production_mods(d)
            total += r["mil"] * lint.factory_speed(d) * cap / 200.0 * 7
        d += 7
    return total


def tech_day(lint, tech, end):
    """Day a tech is in hand under the plan: the start when owned, the research timeline's finish, else None."""
    sched = lint.research_schedule(end)
    if tech in sched["have"]:
        return tl_start(lint)
    return sched["finish"].get(tech)


def tl_start(lint):
    return derive.day(lint.start)


def unit_ready_day(lint, unit, end):
    """First day a sub-unit can be used: active from the start, else the earliest unlock tech in hand."""
    ut = derive.unit_types(lint.game).get(unit) or {}
    if ut.get("active"):
        return tl_start(lint)
    techs = derive.subunit_unlocks(lint.game).get(unit) or set()
    days = [d for d in (tech_day(lint, t, end) for t in techs) if d is not None]
    return min(days) if days else None


def needs(lint, tl, wars):
    """Per war: what the force must hold on its first day (see the module doc), with the numbers."""
    g = lint.game
    pw = plains_width(g)
    out = []
    for war in wars:
        day = war["start"]
        side = side_aggregate(lint, war["enemy_tags"])
        # the front: the targets and the joiners that border them (an earlier war's enemy that is not a target
        # keeps its divisions and fighters on its own land); the line, the breakthrough and the air are sized on it
        # the targets grow to the war's day; a bordering joiner (often by a colony) counts with its start force
        air_side = grown(lint, side_aggregate(lint, war["targets"] + war["joiners"]), war["start"], war["targets"])
        mix = derive.terrain_mix(g, war["theatre"]) if war["theatre"] else {}
        need = {"war": war["id"], "kind": war["kind"], "start": war["start_date"], "enemy": side,
                "theatre_terrain": {k: round(v, 2) for k, v in sorted(mix.items(), key=lambda x: -x[1])[:5]},
                "front": air_side, "rows": []}
        out_mil = military_output(lint, tl, day) if day is not None else None
        need["military_output_ic"] = round(out_mil) if out_mil else None
        axes = max(1, war["axes"]) if war["kind"] == "offensive" else war["axes"]
        need["axes"] = axes
        # breakthrough
        if war["kind"] == "offensive":
            line_width = axes * pw * LINE_DEPTH
            front = air_side
            defended = front["width_est"] >= line_width
            mobile = max(MOBILE_PER_AXIS * axes, int(-(-MOBILE_PER_ENEMY * front["divisions_est"] // 1)))
            armour = max(ARMOUR_PER_AXIS * axes, -(-mobile // 2))
            row = {"role": "breakthrough", "armour": armour, "mobile": mobile, "required": defended,
                   "why": (f"{', '.join(front['tags'])} field about {front['divisions_est']} divisions on the day "
                           f"({front['growth_why']}), {front['width_est']:g} front-line width "
                           f"against {line_width:g} ({axes} strike axes x plains width {pw:g} x {LINE_DEPTH}): "
                           + (f"a defended line; armour on every axis to break it and mobile divisions to exploit "
                              f"({MOBILE_PER_AXIS} per axis, {MOBILE_PER_ENEMY:.0%} of the enemy's divisions, half "
                              "of them armoured)"
                              if defended else "no defended line on every axis; mobile divisions only speed the "
                                               "advance"))}
            afford = breakthrough_cost(lint, day, row["mobile"])
            row.update(afford)
            if defended and not afford["affordable"] and afford.get("division_ic") and out_mil:
                # what BREAK_SHARE of the output pays, never below the start-strength share: the plan that wants
                # more raises its military output (more factories earlier) and the lint follows
                floor = max(MOBILE_PER_AXIS * axes, int(-(-MOBILE_PER_ENEMY * front["divisions"] // 1)))
                paid = int(BREAK_SHARE * out_mil // afford["division_ic"])
                if paid < row["mobile"]:
                    row["mobile"] = max(floor, paid)
                    row["armour"] = max(ARMOUR_PER_AXIS * axes, -(-row["mobile"] // 2))
                    row["why"] += (f"; {BREAK_SHARE:.0%} of the {round(out_mil)} IC the military factories make by "
                                   f"then pays {paid} at {afford['division_ic']} IC ({afford['unit']}): sized to "
                                   f"{row['mobile']} (raise the output to field more)")
            need["rows"].append(row)
        # air: a first-class arm (air_rows)
        bt = next((r for r in need["rows"] if r["role"] == "breakthrough"), None)
        need["rows"] += air_rows(lint, war, air_side, out_mil, axes, bool(bt and bt["required"]),
                                 planned_days(lint, war))
        need["planned_days"] = planned_days(lint, war)
        if air_side["tank_battalions_est"] >= AT_ENEMY_ARMOUR:
            need["rows"].append({"role": "anti_tank", "required": False, "enemy_tank_battalions":
                                 air_side["tank_battalions_est"],
                                 "why": f"the front fields {air_side['tank_battalions']} tank battalions at the "
                                        "start: anti-tank in the line divisions that face them"})
        hills = sum(mix.get(t, 0) for t in ("mountain", "hills"))
        if hills >= 0.3:
            need["rows"].append({"role": "mountain", "required": False, "share": round(hills, 2),
                                 "why": f"mountains and hills are {hills:.0%} of the theatre: mountaineers or narrow "
                                        "line divisions where the axes cross them"})
        out.append(need)
    return out


def planned_days(lint, war):
    """Days the plan gives a war: force_design.wars[].duration_days of the entry naming its targets, else from its
    first day to the latest capitulation checkpoint of its targets; None when the plan dates no end."""
    fd = lint.plan.get("force_design") if isinstance(lint.plan.get("force_design"), dict) else {}
    for x in fd.get("wars") or []:
        if isinstance(x, dict) and set(x.get("enemies") or []) & set(war["targets"]) and \
                isinstance(x.get("duration_days"), int):
            return x["duration_days"]
    caps = capitulation_days(lint)
    ends = [caps[t] for t in war["targets"] if t in caps]
    if war.get("start") is None or not ends:
        return None
    return max(ends) - war["start"]


def air_rows(lint, war, front, out_mil, axes, breakthrough, days=None):
    """The air force a war needs on its first day, inside what the plan can pay (AIR_SHARE of its military output
    to that day): fighters FIGHTER_RATIO x the front's start fighters; once those are paid, close air support over
    the breakthrough axes (one wing an axis); naval strike planes for a landing against a stronger fleet;
    transports for paratroopers; anti-air where the air cannot be contested. A row below MIN_PLANES is no air arm:
    the plan says why in force_design.air_why."""
    g = lint.game
    rows = []
    budget = AIR_SHARE * out_mil if out_mil else 0.0
    fighters = front["planes"].get("fighter", 0) + front["planes"].get("heavy_fighter", 0)
    bombers = sum(front["planes"].get(k, 0) for k in ("cas", "tactical_bomber", "strategic_bomber"))
    want = -(-int(FIGHTER_RATIO * fighters * 10) // 10)
    f_cost, how = kind_cost(lint, "fighter")
    cap = int(budget // f_cost) if f_cost else 0
    planes = min(want, cap) if want else 0
    superiority = bool(want) and planes >= want
    spent = planes * (f_cost or 0.0)
    why = (f"{FIGHTER_RATIO:g} x the {fighters} start fighters of {', '.join(front['tags'])}" if fighters else
           f"{', '.join(front['tags'])} fly no fighters at the start")
    if want and not f_cost:
        why += "; the country has no fighter equipment, no variant and no line: no air contest"
    elif want and not superiority:
        why += (f"; {AIR_SHARE:.0%} of the {round(out_mil or 0)} IC its military factories make by then pays {cap} "
                f"at {f_cost} IC" + (f": {want} win it, so the plan raises the air share or the output "
                                     "(force.decisive)" if cap >= MIN_PLANES else
                                     f": {want} win it; raise the air share or the output (force.decisive)"))
    rows.append({"role": "fighters", "planes": planes, "want": want, "required": planes >= MIN_PLANES,
                 "superiority": superiority, "enemy_fighters": fighters, "ic": round(spent),
                 "unit_cost": f_cost, "unit_cost_from": how, "budget_ic": round(budget), "why": why})
    if bombers and not superiority:
        rows.append({"role": "anti_air", "required": False, "enemy_bombers": bombers,
                     "why": f"{', '.join(front['tags'])} fly {bombers} bombers and CAS and the fighters are short of "
                            "superiority: anti-air in the line divisions, radar and state anti-air where they bomb, "
                            "until the fighters win it"})
    if war["kind"] == "offensive" and breakthrough:
        c_cost, c_how = kind_cost(lint, "cas")
        cas_want = WING_PLANES * max(1, axes)
        cas = min(cas_want, int((budget - spent) // c_cost)) if c_cost and superiority else 0
        rows.append({"role": "cas", "planes": cas, "want": cas_want, "required": cas >= MIN_PLANES,
                     "unit_cost": c_cost, "unit_cost_from": c_how,
                     "why": f"one wing of close air support over each of the {max(1, axes)} breakthrough axes, once "
                            "the fighters win the air there (at most 3 per enemy combat width)"
                            + ("" if superiority else "; the fighters do not win the air first yet: none required")})
    # bombers where the war uses them: tactical bombers on logistics strike behind the front in a war planned past
    # LOGISTICS_DAYS (§5.1, AI11, AI12: rail, hubs, air bases), one wing an axis once fighters and CAS are paid;
    # strategic bombers in a war past a year (AI14), about STRATEGIC_PLANES to beat the repairs
    if war["kind"] == "offensive":
        long_war = days is None or days >= LOGISTICS_DAYS
        t_cost, t_how = kind_cost(lint, "tactical_bomber")
        left = budget - spent - sum(r.get("planes", 0) * (r.get("unit_cost") or 0) for r in rows
                                    if r["role"] == "cas")
        t_want = WING_PLANES * max(1, axes)
        t_planes = min(t_want, int(left // t_cost)) if t_cost and superiority and long_war else 0
        rows.append({"role": "tactical_bombers", "planes": t_planes, "want": t_want if long_war else 0,
                     "required": long_war and t_planes >= MIN_PLANES, "unit_cost": t_cost, "unit_cost_from": t_how,
                     "why": (f"the war is planned for {days if days is not None else 'an undated number of'} days: "
                             f"logistics strike on rail, hubs and air bases behind the front, one wing for each of "
                             f"{max(1, axes)} axes once the fighters and CAS are paid" if long_war else
                             f"the war is planned for {days} days, under {LOGISTICS_DAYS}: no logistics campaign")})
        if days is None or days >= STRATEGIC_DAYS:
            rows.append({"role": "strategic_bombers", "planes": 0, "want": STRATEGIC_PLANES, "required": False,
                         "why": f"the war runs a year or more: about {STRATEGIC_PLANES} strategic bombers on "
                                "industry, ports and air bases (AI14), or the reason for none in "
                                "force_design.bomber_why"})
    if any(force_kind(g, u) == "para" for u in template_units_all(lint)):
        rows.append({"role": "transport", "planes": WING_PLANES, "required": True,
                     "why": "the plan fields paratroopers: transport planes to drop them"})
    return rows


def kind_cost(lint, kind):
    """(IC of one plane of a kind, how): the plan's or the start lines' equipment of that kind, else the
    country's first start variant of it."""
    g = lint.game
    keys = [x.get("equipment") for x in lint.list_of("production") if isinstance(x.get("equipment"), str)]
    keys += [r["type"] for r in lint.start_lines()]
    keys += [v["type"] for v in ((lint.world.country(lint.tag).get("variants") or []) if lint.tag else [])
             if v.get("type")]
    for k in keys:
        if plane_kind(g, k) == kind:
            return unit_cost(lint, k)
    return None, None


def fighter_cost(lint):
    return kind_cost(lint, "fighter")


def force_kind(game, unit):
    return "para" if "para" in unit_classes(game, unit) else None


def template_units_all(lint):
    tl = getattr(lint, "_timeline", None)
    if tl is None:
        return []
    names = set(lint.start_templates()) | {x.get("name") for x in lint.list_of("templates")}
    return [u for n in names if isinstance(n, str) for u in tl.template_units(n)
            if n not in lint.start_templates()]


MISSION_KINDS = {  # the air mission set of the engine (hoi4_air_mission), by the plane kinds that fly it
    "air_superiority": {"fighter", "heavy_fighter"}, "interception": {"fighter", "heavy_fighter"},
    "cas": {"cas", "tactical_bomber", "heavy_fighter"},
    "logistics_strike": {"cas", "tactical_bomber", "heavy_fighter"},
    "strategic_bombing": {"strategic_bomber", "tactical_bomber"},
    "naval_strike": {"naval_bomber", "tactical_bomber", "cas"}, "port_strike": {"naval_bomber", "tactical_bomber",
                                                                                "strategic_bomber"},
    "naval_patrol": {"naval_bomber", "scout", "tactical_bomber"}, "naval_mines": {"naval_bomber", "tactical_bomber"},
    "naval_minesweeping": {"naval_bomber", "tactical_bomber"}, "air_supply": {"transport"},
    "recon": {"scout"}, "training": set(PLANE_KINDS), "kamikaze": {"fighter", "cas", "naval_bomber"},
}


def fuel_use(game, key):
    """fuel_consumption of an equipment (common/units/equipment, through its archetype or parent), or None."""
    def load():
        from .script import get1, parse_file
        raw = {}
        for f in game.files("common/units/equipment"):
            for k, v in parse_file(f):
                if k == "equipments" and isinstance(v, list):
                    for name, body in v:
                        if name and isinstance(body, list):
                            fc = get1(body, "fuel_consumption")
                            try:
                                fc = float(fc) if fc is not None else None
                            except (TypeError, ValueError):
                                fc = None
                            raw[name] = (fc, get1(body, "archetype") or get1(body, "parent"))
        return raw
    raw = game.cached("force:fuel", load)
    seen = set()
    k = key
    while k in raw and k not in seen:
        seen.add(k)
        if raw[k][0] is not None:
            return raw[k][0]
        k = raw[k][1]
    fam = derive.equipment_family(game, key)
    return raw.get(fam, (None, None))[0] if fam != key else None


def breakthrough_cost(lint, day, divisions):
    """{"affordable", "afford_why", "unit"}: whether `divisions` armoured or motorised divisions of the
    cheapest start mobile template are within reach: the units' techs by `day`, the equipment within
    BREAK_SHARE of the military output."""
    tl = getattr(lint, "_timeline", None)
    if tl is None or day is None:
        return {"affordable": True, "afford_why": "undated war"}
    g = lint.game
    best = None
    for name, t in lint.start_templates().items():
        units = [u for u, n in (t.get("regiments") or {}).items() for _ in range(n)] + list(t.get("support") or [])
        p = template_profile(g, units)
        if is_mobile(p["kind"]):
            best = best or (name, units)
    for x in lint.list_of("templates"):
        if best:
            break
        from .validate import template_columns
        units = [u for c in template_columns(x.get("regiments")) or [] for u in c] + list(x.get("support") or [])
        if is_mobile(template_profile(g, units)["kind"]):
            best = (x.get("name") or x.get("id"), units)
    if best is None:
        ut = derive.unit_types(g)
        tech_ok = any(unit_ready_day(lint, u, day) is not None and unit_ready_day(lint, u, day) <= day
                      for u, r in ut.items() if r.get("width") and ("category_tanks" in (r.get("categories") or ())
                                                                    or "category_vehicle_infantry" in
                                                                    (r.get("categories") or ())))
        return {"affordable": tech_ok, "afford_why": "no tank or motorised unit is unlocked by then"
                if not tech_ok else "no mobile template yet: one is designed from the unlocked units"}
    name, units = best
    late = [u for u in set(units) if (unit_ready_day(lint, u, day) or 10 ** 9) > day]
    subs = lint.world.sub_units()
    ic = 0.0
    for u in units:
        for eq, a in ((subs.get(u) or {}).get("need") or {}).items():
            c = equipment_cost(lint, eq)
            ic += a * (c or 0.0)
    out_mil = military_output(lint, tl, day) or 0.0
    total = ic * divisions
    ok = not late and total <= BREAK_SHARE * out_mil
    why = []
    if late:
        why.append("units not unlocked by then: " + ", ".join(sorted(late)))
    if total > BREAK_SHARE * out_mil:
        why.append(f"{divisions} x {round(ic)} IC ({name}) is more than {BREAK_SHARE:.0%} of {round(out_mil)} IC")
    return {"affordable": ok, "afford_why": "; ".join(why) or "within reach", "unit": name,
            "division_ic": round(ic)}


def equipment_cost(lint, archetype):
    """IC of one unit of an equipment archetype: the plan's or start line of that family, else its first
    variant."""
    g = lint.game
    keys = [x.get("equipment") for x in lint.list_of("production") if isinstance(x.get("equipment"), str)]
    keys += [r["type"] for r in lint.start_lines()]
    for k in keys:
        if derive.equipment_family(g, k) == archetype:
            return unit_cost(lint, k)[0]
    rows = derive.equipment_rows(g)
    cands = sorted(((r.get("year") or 0, k) for k, r in rows.items()
                    if r.get("archetype") == archetype and r.get("cost") and k != archetype))
    return unit_cost(lint, cands[0][1])[0] if cands else None
