"""Computed facts over hoi4facts.Game: the numbers the lint (lint_plan.py) and the dossier need that
are not a plain file read. Nothing here is country specific; tags, states and keys come from the plan
or the files, so a mod's content is read the same way.

    calendar      day(date), date_of(n), add_days(date, n), fmt(t)     (the game's 365-day year)
    defines       define(game, "BASE_SURRENDER_LIMIT", 0.8)            (common/defines/*.lua)
    localisation  loc(game, key)                                       (english, $ref$ resolved)
    technologies  tech_meta(game) {tech: folder, enables, cost, year}, equipment_unlocks(game)
    focus         focus_meta(game) {focus: available, cost}, render(block)
    buildings     building_kinds(game) {"factory", "infrastructure", "supply", "railway"}
                  state_buildings(game, ctx, date) {state: {building: level}}
    map           province_controllers, tag_provinces, distances(game, sources, limit),
                  front_distance(game, ctx, date, enemies, k) {state: steps}, border_states
    country       start_ideas, idea_modifiers, research_slots, surrender(game, tag, ...)
    characters    commanders(game, tag, ctx, date), match_commander(name, commanders)
    missions      decision_index(game), control_states(block), scenario_timers(game, tag, ...)
    units         unit_types(game), subunit_unlocks(game), equipment_archetypes, equipment_domains,
                  template_width(game, regiments)
    terrain       terrain_widths(game) (common/terrain combat_width), theatre_width_target(game, states)
    laws, map     law_ideas(game), capital_state, coastal_states, overseas_states, resisting_states,
                  air_base_levels, supply_hubs, coastal_provinces
    construction  building_costs(game), construction_constants (calc.py labels),
                  free_civilian_factories(game, ctx, tag, date), construction_days(game, building, civs, infra)

The lint and the dossier builder share these; each result is memoised on the Game.
"""
import math
import os
import re
from collections import deque

from .script import (DEFAULT_START, Ctx, bare, date_tuple, get, get1, history_walk, op_of, parse_file, read,
                     trigger)

# --------------------------------------------------------------------------- calendar

MONTH_DAYS = (31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31)
_CUM = [sum(MONTH_DAYS[:i]) for i in range(12)]


def as_tuple(date):
    """(y, m, d) from "1936.1.1", "1936-01-01" or a tuple; None when it is not a date."""
    if isinstance(date, tuple):
        return date
    return date_tuple(str(date or "").strip()) if date else None


def day(date):
    """Day number of a date in the game's calendar (365 days a year, no 29 February)."""
    t = as_tuple(date)
    if not t:
        return None
    y, m, d = t
    m = min(max(m, 1), 12)
    return y * 365 + _CUM[m - 1] + min(max(d, 1), MONTH_DAYS[m - 1]) - 1


def date_of(n):
    y, r = divmod(int(n), 365)
    m = 0
    while m < 11 and r >= _CUM[m + 1]:
        m += 1
    return (y, m + 1, r - _CUM[m] + 1)


def add_days(date, n):
    return date_of(day(date) + n)


def fmt(t):
    t = as_tuple(t)
    return "%d.%02d.%02d" % t if t else ""


# --------------------------------------------------------------------------- defines

_DEFINE = re.compile(r"^\s*([A-Z][A-Z0-9_]*)\s*=\s*(-?[0-9.]+|\"[^\"]*\"|true|false)", re.M)


def defines(game):
    """{NAME: value} from common/defines/*.lua (later files and mods override; numbers as float)."""

    def load():
        out = {}
        for f in game.files("common/defines", ext=(".lua",)):
            for name, val in _DEFINE.findall(read(f)):
                if val.startswith('"'):
                    out[name] = val.strip('"')
                elif val in ("true", "false"):
                    out[name] = val == "true"
                else:
                    try:
                        out[name] = float(val)
                    except ValueError:
                        pass
        return out

    return game.cached("derive:defines", load)


def define(game, name, default=None):
    return defines(game).get(name, default)


# --------------------------------------------------------------------------- localisation

_LOC = re.compile(r'^\s*([A-Za-z0-9_.\-]+):\d*\s*"(.*)"')


def localisation(game):
    """{key: text} of the english localisation (localisation/**/*_l_english.yml)."""

    def load():
        out = {}
        for p in game.files("localisation", ext=(".yml",), recursive=True):
            if "english" not in p.lower():
                continue
            for line in read(p).splitlines():
                m = _LOC.match(line)
                if m:
                    out[m.group(1)] = m.group(2)
        return out

    return game.cached("derive:loc", load)


def loc(game, key, default=None):
    """Localised text of key with $other$ references resolved, or default."""
    table = localisation(game)
    text = table.get(key)
    if text is None:
        return default
    for _ in range(4):
        refs = re.findall(r"\$([A-Za-z0-9_.\-]+)\$", text)
        if not refs:
            break
        for r in refs:
            text = text.replace(f"${r}$", table.get(r, r))
    return text


# --------------------------------------------------------------------------- technologies


def tech_meta(game):
    """{tech: {"folder": name or None, "enables": set(equipment), "cost": float, "year": int,
    "categories": set}} from common/technologies."""

    def load():
        out = {}
        for f in game.files("common/technologies"):
            for k, v in parse_file(f):
                if k != "technologies" or not isinstance(v, list):
                    continue
                for name, body in v:
                    if not name or not isinstance(body, list) or name.startswith("@"):
                        continue
                    folder = get1(body, "folder")
                    fname = get1(folder, "name") if isinstance(folder, list) else None
                    en = get1(body, "enable_equipments")
                    try:
                        cost = float(get1(body, "research_cost", "1"))
                    except ValueError:
                        cost = 1.0
                    year = get1(body, "start_year", "0")
                    cats = get1(body, "categories")
                    out[name] = {
                        "folder": fname,
                        "enables": set(bare(en)) if isinstance(en, list) else set(),
                        "cost": cost,
                        "year": int(year) if str(year).isdigit() else 0,
                        "categories": set(bare(cats)) if isinstance(cats, list) else set(),
                    }
        return out

    return game.cached("derive:techmeta", load)


def equipment_types(game):
    """{equipment key: set(unit types)} from common/units/equipment, variants inheriting their
    archetype's `type` block (motorized_equipment_1 -> {"motorized"})."""

    def load():
        raw = {}
        for f in game.files("common/units/equipment"):
            for k, v in parse_file(f):
                if k != "equipments" or not isinstance(v, list):
                    continue
                for name, body in v:
                    if not name or not isinstance(body, list):
                        continue
                    t = get1(body, "type")
                    types = set(bare(t)) if isinstance(t, list) else ({t} if isinstance(t, str) else set())
                    raw[name] = (types, get1(body, "archetype"), get1(body, "parent"))
        out = {}
        for name in raw:
            seen, cur, types = set(), name, set()
            while cur in raw and cur not in seen:
                seen.add(cur)
                t, arch, parent = raw[cur]
                types |= t
                cur = arch or parent
            out[name] = types
        return out

    return game.cached("derive:equiptypes", load)


def equipment_unlocks(game):
    """{equipment key: set(techs that enable it)}."""

    def load():
        out = {}
        for t, m in tech_meta(game).items():
            for e in m["enables"]:
                out.setdefault(e, set()).add(t)
        return out

    return game.cached("derive:unlocks", load)


def tech_days(game, tech):
    """Base research days of a tech: research_cost x BASE_TECH_COST (no research-speed bonus, no
    ahead-of-time penalty)."""
    m = tech_meta(game).get(tech)
    return (m["cost"] if m else 1.0) * float(define(game, "BASE_TECH_COST", 100))


# --------------------------------------------------------------------------- focus


def focus_meta(game):
    """{focus id: {"available": block or None, "cost": float, "tree": id}}."""

    def load():
        out = {}
        for f in game.files("common/national_focus"):
            for k, v in parse_file(f):
                if k not in ("focus_tree", "shared_focus") or not isinstance(v, list):
                    continue
                tid = get1(v, "id", os.path.basename(f))
                bodies = [v] if k == "shared_focus" else [fv for fk, fv in v if fk == "focus" and isinstance(fv, list)]
                for fv in bodies:
                    fid = get1(fv, "id")
                    if not fid:
                        continue
                    av = get1(fv, "available")
                    try:
                        cost = float(get1(fv, "cost", "1"))
                    except ValueError:
                        cost = 1.0
                    out[fid] = {"available": av if isinstance(av, list) and av else None,
                                "cost": cost, "tree": tid}
        return out

    return game.cached("derive:focusmeta", load)


def render(block, depth=0):
    """Compact one-line text of a script block (for messages)."""
    if not isinstance(block, list):
        return str(block)
    parts = []
    for k, v in block:
        if isinstance(v, list):
            inner = render(v, depth + 1) if depth < 3 else "..."
            parts.append(f"{k} = {{ {inner} }}" if k else f"{{ {inner} }}")
        else:
            parts.append(f"{k} = {v}" if k else str(v))
    return " ".join(parts)


# --------------------------------------------------------------------------- buildings


def building_kinds(game):
    """Building keys by role, read from the building attributes (so a mod's buildings count):
    factory (military_production / general_production / naval_production), infrastructure
    (infrastructure = yes), supply (supply_node = yes), railway (rail_way / railway)."""

    def load():
        out = {"factory": set(), "military": set(), "infrastructure": set(), "supply": set(),
               "railway": {"rail_way", "railway"}}
        for f in game.files("common/buildings"):
            for k, v in parse_file(f):
                if k != "buildings" or not isinstance(v, list):
                    continue
                for name, body in v:
                    if not name or not isinstance(body, list):
                        continue
                    for attr in ("military_production", "general_production", "naval_production"):
                        if get1(body, attr) not in (None, "0"):
                            out["factory"].add(name)
                    if get1(body, "military_production") not in (None, "0"):
                        out["military"].add(name)
                    if get1(body, "infrastructure") == "yes":
                        out["infrastructure"].add(name)
                    if get1(body, "supply_node") == "yes":
                        out["supply"].add(name)
        return out

    return game.cached("derive:buildingkinds", load)


def _ctxkey(ctx, date):
    return f"{ctx.tag if ctx else ''}:{sorted(ctx.dlcs) if ctx else None}:{date}"


def state_buildings(game, ctx, date=DEFAULT_START):
    """{state id: {building: level}} of the state-level buildings at date (province blocks such as
    `12345 = { naval_base = 1 }` are left out)."""

    def load():
        out = {}
        for sid, (f, st) in game._state_blocks().items():
            hist = get1(st, "history", [])
            row = {}
            if isinstance(hist, list):

                def visit(k, v, row=row):
                    if k == "buildings" and isinstance(v, list):
                        for a, b in v:
                            if a and not a.isdigit() and isinstance(b, str):
                                try:
                                    row[a] = int(float(b))
                                except ValueError:
                                    pass

                history_walk(hist, ctx, date, visit)
            out[sid] = row
        return out

    return game.cached("derive:statebuildings:" + _ctxkey(ctx, date), load)


# --------------------------------------------------------------------------- map


def province_controllers(game, ctx, date=DEFAULT_START):
    """{province: controller tag} from the state history at date."""

    def load():
        out = {}
        facts = game.state_facts(ctx, date)
        for sid, s in game.states().items():
            ctl = (facts.get(sid) or {}).get("controller")
            if ctl:
                for p in s["provinces"]:
                    out[p] = ctl
        return out

    return game.cached("derive:provctl:" + _ctxkey(ctx, date), load)


def tag_provinces(game, ctx, tags, date=DEFAULT_START):
    tags = set(tags)
    return {p for p, t in province_controllers(game, ctx, date).items() if t in tags}


def tag_states(game, ctx, tag, date=DEFAULT_START):
    """States owned by tag at date."""
    return {sid for sid, o in game.state_owners(ctx, date).items() if o == tag}


def land_adjacency(game):
    """{province: set(land neighbours)} over passable borders and straits, or None without a map."""

    def load():
        graph = game.map_graph()
        if graph is None:
            return None
        provs = graph["provinces"]
        out = {}
        for a, row in graph["adj"].items():
            if provs.get(a, {}).get("type") != "land":
                continue
            out[a] = {b for b, e in row.items() if not e.get("impassable") and provs.get(b, {}).get("type") == "land"}
        return out

    return game.cached("derive:landadj", load)


def distances(game, sources, limit=None):
    """{province: land steps to the nearest source province} (BFS), up to limit steps."""
    adj = land_adjacency(game) or {}
    dist = {p: 0 for p in sources}
    q = deque(sources)
    while q:
        a = q.popleft()
        d = dist[a]
        if limit is not None and d >= limit:
            continue
        for b in adj.get(a, ()):
            if b not in dist:
                dist[b] = d + 1
                q.append(b)
    return dist


def front_distance(game, ctx, date, enemies, k=2):
    """{state id: steps from its nearest province to an enemy-controlled province} for the states
    within k steps (states farther away are absent). Enemy provinces themselves count 0."""
    src = tag_provinces(game, ctx, enemies, date)
    dist = distances(game, src, k)
    out = {}
    pstate = game.province_state()
    for p, d in dist.items():
        sid = pstate.get(p)
        if sid is not None and (sid not in out or d < out[sid]):
            out[sid] = d
    return out


def border_states(game, ctx, date, me, enemies):
    """States owned by `me` with a province touching an enemy-controlled province by land."""
    adj = land_adjacency(game) or {}
    ctl = province_controllers(game, ctx, date)
    enemies = set(enemies)
    out = set()
    for sid in tag_states(game, ctx, me, date):
        for p in game.states()[sid]["provinces"]:
            if ctl.get(p) == me and any(ctl.get(n) in enemies for n in adj.get(p, ())):
                out.add(sid)
                break
    return out


# --------------------------------------------------------------------------- country


def history_effects_of(game, tag, ctx, date=DEFAULT_START, history_file=None):
    """(history file, [(key, value)] effects applied at date) of a country, ctx-aware."""
    hf = history_file or game.history_file(tag)

    def load():
        p = game.file(hf) if hf else None
        if not p:
            return []
        out = []
        history_walk(parse_file(p), ctx, date, lambda k, v: out.append((k, v)))
        return out

    return hf, game.cached(f"derive:histfx:{hf}:" + _ctxkey(ctx, date), load)


def start_ideas(game, tag, ctx, date=DEFAULT_START, history_file=None):
    _, fx = history_effects_of(game, tag, ctx, date, history_file)
    out = set()
    for k, v in fx:
        if k == "add_ideas":
            if isinstance(v, list):
                out.update(bare(v))
                out.update(a for a, b in v if a and b in ("yes", "1"))
            elif isinstance(v, str):
                out.add(v)
    return out


def start_variables(game, tag, ctx, date=DEFAULT_START, history_file=None):
    """{variable: number} from the history's set_variable effects."""
    _, fx = history_effects_of(game, tag, ctx, date, history_file)
    out = {}

    def visit(k, v):
        if k == "set_variable" and isinstance(v, list):
            var = get1(v, "var")
            if var is not None:
                val = get1(v, "value")
                pairs = [(var, val)]
            else:
                pairs = [(a, b) for a, b in v if a]
            for a, b in pairs:
                try:
                    out[a] = float(b)
                except (TypeError, ValueError):
                    pass
        elif isinstance(v, list) and k and k.lower() not in ("limit",):
            for a, b in v:
                visit(a, b)

    for k, v in fx:
        visit(k, v)
    return out


def idea_modifiers(game):
    """{idea: {modifier: value}} from common/ideas (the `modifier` block of each idea)."""

    def load():
        out = {}
        for f in game.files("common/ideas"):
            for k, v in parse_file(f):
                if k != "ideas" or not isinstance(v, list):
                    continue
                for cat, body in v:
                    if not isinstance(body, list):
                        continue
                    for name, idea in body:
                        if not name or not isinstance(idea, list):
                            continue
                        mods = {}
                        m = get1(idea, "modifier")
                        if isinstance(m, list):
                            for a, b in m:
                                try:
                                    mods[a] = float(b)
                                except (TypeError, ValueError):
                                    pass
                        out[name] = mods
        return out

    return game.cached("derive:ideamods", load)


def research_slots(game, tag, ctx, date=DEFAULT_START, history_file=None):
    _, fx = history_effects_of(game, tag, ctx, date, history_file)
    n = None
    for k, v in fx:
        if k == "set_research_slots" and isinstance(v, str) and v.isdigit():
            n = int(v)
    return n if n is not None else int(define(game, "BASE_RESEARCH_SLOTS", 2))


def surrender(game, tag, ctx, date=DEFAULT_START):
    """Capitulation arithmetic of `tag` at date: {"total_vp", "base", "modifiers": [[idea, value]],
    "limit", "vp_needed", "vp_needed_base"}. Total VP = the victory points of the states it owns;
    limit = BASE_SURRENDER_LIMIT plus the surrender_limit of its starting ideas, floored at
    MIN_SURRENDER_LIMIT. Ideas gained later (events, spirits added in war) are not counted."""
    ectx = Ctx(tag, ctx.dlcs) if ctx else None
    facts = game.state_facts(ectx, date)
    total = sum(sum(r["vps"].values()) for r in facts.values() if r["owner"] == tag)
    base = float(define(game, "BASE_SURRENDER_LIMIT", 0.8))
    floor = float(define(game, "MIN_SURRENDER_LIMIT", 0.2))
    mods = []
    table = idea_modifiers(game)
    for idea in sorted(start_ideas(game, tag, ectx, date)):
        v = table.get(idea, {}).get("surrender_limit")
        if v:
            mods.append([idea, v])
    limit = max(floor, base + sum(v for _, v in mods))
    need = lambda lim: int(math.ceil(total * lim - 1e-9))  # noqa: E731
    return {"tag": tag, "total_vp": total, "base": base, "modifiers": mods, "limit": round(limit, 4),
            "vp_needed": need(limit), "vp_needed_base": need(base)}


# --------------------------------------------------------------------------- characters

COMMANDER_ROLES = ("corps_commander", "field_marshal", "navy_leader")


def characters(game, ctx=None):
    """{id: {"names": set(display names), "roles": set, "file"}} with role blocks allowed for ctx."""

    def load():
        out = {}
        for f in game.files("common/characters"):
            for k, v in parse_file(f):
                if k != "characters" or not isinstance(v, list):
                    continue
                for cid, body in v:
                    if not cid or not isinstance(body, list):
                        continue
                    blocks = [body]
                    for inst in get(body, "instance"):
                        if isinstance(inst, list) and not (ctx and trigger(get1(inst, "allowed", []), ctx) is False):
                            blocks.append(inst)
                    roles, names = set(), set()
                    for b in blocks:
                        n = get1(b, "name")
                        if isinstance(n, str):
                            names.add(n)
                        for r in COMMANDER_ROLES + ("advisor", "country_leader"):
                            for rb in get(b, r):
                                if isinstance(rb, list) and not (ctx and trigger(get1(rb, "allowed", []), ctx) is False):
                                    roles.add(r)
                    row = out.setdefault(cid, {"names": set(), "roles": set(), "file": os.path.basename(f)})
                    row["names"] |= names
                    row["roles"] |= roles
        return out

    key = "derive:chars" if ctx is None else "derive:chars:" + _ctxkey(ctx, "")
    return game.cached(key, load)


def _norm(text):
    return re.sub(r"[^a-z0-9]+", " ", (text or "").lower()).strip()


def commanders(game, tag, ctx, date=DEFAULT_START, history_file=None):
    """{character id: {"display": name, "roles": set, "recruited": bool}} of the land/navy
    commanders a country can have: characters recruited by its history at date, plus those
    defined in a characters file named after the tag."""
    chars = characters(game, ctx)
    _, fx = history_effects_of(game, tag, ctx, date, history_file)
    recruited = set()

    def visit(k, v):
        if k == "recruit_character" and isinstance(v, str):
            recruited.add(v)
        elif isinstance(v, list) and k and k.lower() != "limit":
            for a, b in v:
                visit(a, b)

    for k, v in fx:
        visit(k, v)
    out = {}
    for cid, c in chars.items():
        mine = cid in recruited or c["file"].split(".")[0].upper() == tag or cid.startswith(tag + "_")
        if not mine or not (c["roles"] & set(COMMANDER_ROLES)):
            continue
        display = None
        for n in sorted(c["names"]) + [cid]:
            display = loc(game, n)
            if display:
                break
        out[cid] = {"display": display or cid, "roles": c["roles"], "recruited": cid in recruited}
    return out


def match_commander(name, table):
    """Character id for a plan's general name (display name, id, or id without the tag), or None."""
    want = _norm(name)
    for cid, c in table.items():
        if want in (_norm(c["display"]), _norm(cid)):
            return cid
    for cid in table:
        if _norm(cid).endswith(" " + want) or _norm(cid).split(" ", 1)[-1] == want:
            return cid
    return None


# --------------------------------------------------------------------------- missions and timers


def decision_index(game):
    """{decision id: body} of every decision and mission in common/decisions."""

    def load():
        out = {}
        for f in game.files("common/decisions"):
            for k, v in parse_file(f):
                if isinstance(v, list) and k and not k.startswith("@"):
                    for n, b in v:
                        if n and isinstance(b, list):
                            out[n] = b
        return out

    return game.cached("derive:decisions", load)


_CONTROL_KEYS = ("has_full_control_of_state", "controls_state", "owns_state")


def control_states(block):
    """{state id: trigger key} of the plain state-control conditions in a trigger block (nested
    AND/OR included, NOT left out)."""
    out = {}

    def walk(items):
        for k, v in items or []:
            kl = (k or "").lower()
            if kl in _CONTROL_KEYS and isinstance(v, str) and v.isdigit():
                out.setdefault(int(v), kl)
            elif kl in ("and", "or", "hidden_trigger", "custom_trigger_tooltip") and isinstance(v, list):
                walk(v)

    walk(block if isinstance(block, list) else [])
    return out


def _event_body(game, eid):
    ev = game.event(eid)
    if not ev:
        return None
    for k, v in parse_file(ev["file"]):
        if isinstance(v, list) and get1(v, "id") == eid:
            return v
    return None


_EVENT_KEYS = ("country_event", "news_event", "state_event", "unit_leader_event", "character_event")


def _scheduled(items):
    """[(event id, days, random_days)] scheduled in an effect block (scopes entered, limits not)."""
    out = []

    def walk(block):
        for k, v in block:
            kl = (k or "").lower()
            if kl in _EVENT_KEYS:
                if isinstance(v, str):
                    out.append((v, 0, 0))
                elif isinstance(v, list) and get1(v, "id"):
                    def num(x):
                        try:
                            return int(float(get1(v, x, "0")))
                        except ValueError:
                            return 0
                    out.append((get1(v, "id"), num("days"), num("random_days")))
            elif isinstance(v, list) and kl not in ("limit", "trigger", "ai_chance", "ai_will_do"):
                walk(v)

    walk(items)
    return out


def _missions(items):
    out = []

    def visit(k, v):
        if (k or "").lower() == "activate_mission" and isinstance(v, str):
            out.append(v)
        elif isinstance(v, list) and (k or "").lower() not in ("limit", "trigger"):
            for a, b in v:
                visit(a, b)

    for k, v in items:
        visit(k, v)
    return out


def scenario_timers(game, tag, ctx, date=DEFAULT_START, history_file=None, depth=3):
    """Mission deadlines a country's history sets in motion: missions activated in the history, or
    by an event the history schedules (`unit_leader_event = { id days random_days }` in any scope),
    following scheduled events up to `depth` links. Timeout = days_mission_timeout (a number or a
    history variable). [{"mission", "via": [events], "earliest", "latest", "timeout"}] sorted."""
    start = day(date)
    decisions = decision_index(game)
    variables = start_variables(game, tag, ctx, date, history_file)
    _, fx = history_effects_of(game, tag, ctx, date, history_file)
    out = []
    seen = set()

    def timeout_of(mission):
        body = decisions.get(mission)
        if body is None:
            return None
        t = get1(body, "days_mission_timeout")
        if t is None:
            return None
        try:
            return int(float(t))
        except ValueError:
            v = variables.get(t.split(".")[-1])
            return int(v) if v is not None else None

    def add(mission, lo, hi, via):
        t = timeout_of(mission)
        if t is None or (mission, lo) in seen:
            return
        seen.add((mission, lo))
        out.append({"mission": mission, "via": via, "timeout": t,
                    "earliest": fmt(date_of(start + lo + t)), "latest": fmt(date_of(start + hi + t))})

    def follow(eid, lo, hi, via, left):
        body = _event_body(game, eid)
        if body is None or left <= 0:
            return
        for m in _missions(body):
            add(m, lo, hi, via + [eid])
        for nid, d, r in _scheduled([(k, v) for k, v in body if (k or "").lower() in ("immediate", "option")]):
            follow(nid, lo + d, hi + d + r, via + [eid], left - 1)

    for m in _missions(fx):
        add(m, 0, 0, [])
    for eid, d, r in _scheduled(fx):
        follow(eid, d, d + r, [], depth)
    return sorted(out, key=lambda x: (day(x["earliest"]), x["mission"]))


# --------------------------------------------------------------------------- units and terrain
# Small readers for the plan lint's template, posture, air-base and occupation rules. The facts
# commands (`facts.py terrain-width`, `units`) are built elsewhere; these stay minimal.


def _num(v, default=None):
    try:
        return float(v)
    except (TypeError, ValueError):
        return default


def terrain_widths(game):
    """{terrain: {"width": combat_width, "support": combat_support_width}} of the land terrains in
    common/terrain (categories with a combat_width; ocean, lakes and unknown have none)."""

    def load():
        out = {}
        for f in game.files("common/terrain"):
            for k, v in parse_file(f):
                if k != "categories" or not isinstance(v, list):
                    continue
                for name, body in v:
                    if not name or not isinstance(body, list):
                        continue
                    if get1(body, "naval_terrain") == "yes" or get1(body, "is_water") == "yes":
                        continue
                    w = _num(get1(body, "combat_width"))
                    if w is None or w <= 0:
                        continue
                    out[name] = {"width": w, "support": _num(get1(body, "combat_support_width"), w / 2.0)}
        return out

    return game.cached("derive:terrainwidths", load)


def unit_types(game):
    """{sub-unit: {"width", "group", "support", "regimental", "active", "need": {equipment: n},
    "types": set, "categories": set}} from common/units (battalions, support companies, ships and
    planes). support: group = support; regimental: a 1.19 regimental-support battalion
    (category_regimental_support_battalions or `regimental = yes`)."""

    def load():
        out = {}
        for f in game.files("common/units"):
            for k, v in parse_file(f):
                if k != "sub_units" or not isinstance(v, list):
                    continue
                for name, body in v:
                    if not name or not isinstance(body, list):
                        continue
                    cats = get1(body, "categories")
                    cats = set(bare(cats)) if isinstance(cats, list) else set()
                    typ = get1(body, "type")
                    need = get1(body, "need")
                    out[name] = {
                        "width": _num(get1(body, "combat_width"), 0.0),
                        "group": get1(body, "group", ""),
                        "support": get1(body, "group") == "support",
                        "regimental": get1(body, "regimental") == "yes"
                        or "category_regimental_support_battalions" in cats,
                        "active": get1(body, "active") == "yes",
                        "need": {a: _num(b, 0.0) for a, b in need if a} if isinstance(need, list) else {},
                        "types": set(bare(typ)) if isinstance(typ, list) else set(),
                        "categories": cats,
                    }
        return out

    return game.cached("derive:unittypes", load)


def subunit_unlocks(game):
    """{sub-unit: set(technologies whose enable_subunits lists it)}."""

    def load():
        out = {}
        for f in game.files("common/technologies"):
            for k, v in parse_file(f):
                if k != "technologies" or not isinstance(v, list):
                    continue
                for tech, body in v:
                    if not tech or not isinstance(body, list):
                        continue
                    for block in get(body, "enable_subunits"):
                        if isinstance(block, list):
                            for u in bare(block):
                                out.setdefault(u, set()).add(tech)
        return out

    return game.cached("derive:subunitunlocks", load)


def equipment_archetypes(game):
    """{equipment key: archetype key} (an archetype maps to itself) from common/units/equipment."""

    def load():
        raw = {}
        for f in game.files("common/units/equipment"):
            for k, v in parse_file(f):
                if k != "equipments" or not isinstance(v, list):
                    continue
                for name, body in v:
                    if name and isinstance(body, list):
                        raw[name] = (get1(body, "is_archetype") == "yes", get1(body, "archetype"), get1(body, "parent"))
        out = {}
        for name in raw:
            seen, cur = set(), name
            while cur in raw and cur not in seen:
                seen.add(cur)
                arch, a, p = raw[cur]
                if arch or not (a or p):
                    break
                cur = a or p
            out[name] = cur
        return out

    return game.cached("derive:archetypes", load)


_NAVY_TYPES = {"screen_ship", "capital_ship", "submarine", "carrier", "support_ship", "convoy", "floating_harbor"}
_AIR_TYPES = {"fighter", "cas", "tactical_bomber", "strategic_bomber", "naval_bomber", "air_transport", "scout_plane",
              "suicide", "missile", "ballistic_missile", "nuclear_missile", "sam_missile"}


def equipment_domains(game):
    """{equipment key: "land" | "air" | "navy"} for force-posture shares: the equipment's
    interface_category (inherited from its archetype) when it names air or ships, else its unit
    types (ships, convoys, planes, missiles), else land."""

    def load():
        raw, dup = {}, {}
        for f in game.files("common/units/equipment"):
            for k, v in parse_file(f):
                if k not in ("equipments", "duplicate_archetypes") or not isinstance(v, list):
                    continue
                for name, body in v:
                    if name and isinstance(body, list):
                        row = (get1(body, "interface_category"), get1(body, "archetype"), get1(body, "parent"))
                        (raw if k == "equipments" else dup)[name] = row
                        t = get1(body, "type")
                        if k == "duplicate_archetypes":
                            dup[name] += (set(bare(t)) if isinstance(t, list) else ({t} if isinstance(t, str) else set()),)
        types = equipment_types(game)

        def domain(cat, mine):
            cat = (cat or "").lower()
            if "ship" in cat or mine & _NAVY_TYPES:
                return "navy"
            if "air" in cat or mine & _AIR_TYPES:
                return "air"
            return "land"

        out = {}
        for name in raw:
            seen, cur, cat = set(), name, None
            while cur in raw and cur not in seen and not cat:
                seen.add(cur)
                cat, a, p = raw[cur]
                cur = a or p
            out[name] = domain(cat, types.get(name, set()))
        for name, (cat, arch, _, typ) in dup.items():
            out[name] = domain(cat, typ) if (cat or typ) else out.get(arch, "land")
        return out

    return game.cached("derive:equipdomains", load)


def equipment_domain(game, key):
    """Domain of one equipment key (a generated duplicate variant such as small_plane_cas_airframe_1
    falls back to its family), or None when unknown."""
    table = equipment_domains(game)
    if key in table:
        return table[key]
    base = re.sub(r"_\d+$", "", key or "")
    return table.get(base)


def template_width(game, regiments):
    """Combat width of a template from its regiment columns (support companies have width 0);
    None when a unit is unknown. regiments: [[unit, ...], ...] or a flat [unit, ...]."""
    units = unit_types(game)
    flat = [u for col in regiments or [] for u in (col if isinstance(col, list) else [col])]
    if any(u not in units for u in flat):
        return None
    return sum(units[u]["width"] for u in flat)


def _fill(width, frontage):
    """How well divisions of `width` fill `frontage`: the best n x width against frontage ratio
    (1 = exact, over- and under-filling count alike)."""
    if width <= 0 or frontage <= 0:
        return 0.0
    n = max(1, int(round(frontage / width)))
    return max(min(k * width, frontage) / max(k * width, frontage) for k in (n - 1, n, n + 1) if k >= 1)


def theatre_width_target(game, states, lo=10, hi=40, tolerance=0.02):
    """Placeholder width target for a theatre (until facts.py terrain-width lands): the terrain mix
    of the states' land provinces (map/definition.csv terrain) weighted by province count, each
    terrain fought on its base width and with one extra direction (width + support width). Small
    widths always fill well, so the target is the widest template width in [lo, hi] (40 is the
    template tool's limit) whose fill is within `tolerance` of the best fill.
    {"target", "mix": {terrain: share}, "fill": score} or None without terrain data."""
    graph = game.map_graph()
    widths = terrain_widths(game)
    if graph is None or not widths:
        return None
    counts = {}
    all_states = game.states()
    for sid in states or []:
        for p in all_states.get(sid, {}).get("provinces", []):
            t = (graph["provinces"].get(p) or {}).get("terrain")
            if t in widths:
                counts[t] = counts.get(t, 0) + 1
    total = sum(counts.values())
    if not total:
        return None
    mix = {t: n / total for t, n in sorted(counts.items())}
    fronts = [(share, f) for t, share in mix.items()
              for f in (widths[t]["width"], widths[t]["width"] + widths[t]["support"])]
    scores = {w: sum(share * _fill(w, f) for share, f in fronts) / 2.0 for w in range(lo, hi + 1)}
    top = max(scores.values())
    target = max(w for w, s in scores.items() if s >= top - tolerance)
    return {"target": target, "mix": {t: round(s, 3) for t, s in mix.items()}, "fill": round(scores[target], 3)}


# --------------------------------------------------------------------------- laws, capital, coast, resistance


def law_ideas(game):
    """{idea: {"category", "available": block or None, "gates": {"war_support"|"stability": (op, x)}}}
    of the ideas in law categories (`law = yes`) of common/ideas; gates are the plain
    has_war_support / has_stability comparisons at the top level of `available`."""

    def load():
        out = {}
        for f in game.files("common/ideas", recursive=True):
            for k, v in parse_file(f):
                if k != "ideas" or not isinstance(v, list):
                    continue
                for cat, body in v:
                    if not cat or not isinstance(body, list) or get1(body, "law") != "yes":
                        continue
                    for name, idea in body:
                        if not name or not isinstance(idea, list):
                            continue
                        av = get1(idea, "available")
                        av = av if isinstance(av, list) and av else None
                        gates = {}
                        for i, (a, b) in enumerate(av or []):
                            if a in ("has_war_support", "has_stability") and _num(b) is not None:
                                gates[a[4:]] = (op_of(av, i), _num(b))
                        out[name] = {"category": cat, "available": av, "gates": gates}
        return out

    return game.cached("derive:lawideas", load)


def capital_state(game, tag, ctx, date=DEFAULT_START, history_file=None):
    """The `capital = <state>` of a country's history at date, or None."""
    _, fx = history_effects_of(game, tag, ctx, date, history_file)
    cap = None
    for k, v in fx:
        if k == "capital" and isinstance(v, str) and v.isdigit():
            cap = int(v)
    return cap


def coastal_states(game, ctx, tag, date=DEFAULT_START):
    """States owned by tag at date with a coastal province (map/definition.csv), or None without a map."""
    graph = game.map_graph()
    if graph is None:
        return None
    provs = graph["provinces"]
    states = game.states()
    return {sid for sid in tag_states(game, ctx, tag, date)
            if any((provs.get(p) or {}).get("coastal") for p in states.get(sid, {}).get("provinces", []))}


def overseas_states(game, ctx, tag, states, date=DEFAULT_START, history_file=None):
    """The given states owned by tag that its capital cannot reach over land it controls at date
    (straits count, foreign land does not): an army there is supplied and reinforced by sea.
    None without a map or a capital."""
    adj = land_adjacency(game)
    cap = capital_state(game, tag, ctx, date, history_file)
    if adj is None or cap is None:
        return None
    own = tag_provinces(game, ctx, [tag], date)
    all_states = game.states()
    seen = {p for p in all_states.get(cap, {}).get("provinces", []) if p in own}
    q = deque(seen)
    while q:
        a = q.popleft()
        for b in adj.get(a, ()):
            if b in own and b not in seen:
                seen.add(b)
                q.append(b)
    mine = tag_states(game, ctx, tag, date)
    return {sid for sid in states or [] if sid in mine
            and not any(p in seen for p in all_states.get(sid, {}).get("provinces", []))}


def resisting_states(game, tag, ctx, date=DEFAULT_START, history_file=None):
    """{state: {"compliance": x or None, "resistance": x or None}} of the states a country starts
    with resistance on: `<state> = { start_resistance = yes ... }` blocks of the country history,
    and `start_resistance` in the history of a state it owns."""
    _, fx = history_effects_of(game, tag, ctx, date, history_file)
    out = {}
    for k, v in fx:
        if k and k.isdigit() and isinstance(v, list) and get1(v, "start_resistance") not in (None, "no"):
            out[int(k)] = {"compliance": _num(get1(v, "set_compliance")), "resistance": _num(get1(v, "set_resistance"))}
    mine = tag_states(game, ctx, tag, date)
    for sid, (_, st) in game._state_blocks().items():
        hist = get1(st, "history", [])
        if sid not in mine or sid in out or not isinstance(hist, list):
            continue
        found = {}

        def visit(k, v, found=found):
            if k == "start_resistance" and v != "no":
                found["on"] = True
            elif k in ("set_compliance", "set_resistance"):
                found[k[4:]] = _num(v)

        history_walk(hist, ctx, date, visit)
        if found.get("on"):
            out[sid] = {"compliance": found.get("compliance"), "resistance": found.get("resistance")}
    return out


def air_base_buildings(game):
    """Building keys with `air_base = yes` (air_base when the files name none)."""

    def load():
        out = set()
        for f in game.files("common/buildings"):
            for k, v in parse_file(f):
                if k == "buildings" and isinstance(v, list):
                    out.update(n for n, b in v if n and isinstance(b, list) and get1(b, "air_base") == "yes")
        return out or {"air_base"}

    return game.cached("derive:airbases", load)


def building_costs(game):
    """{building: {"cost": base_cost, "per_level": per_level_extra_cost, "infra_effect": bool,
    "port": bool, "coastal": bool, "civilian": bool, "military": bool}} from common/buildings
    (infra_effect: infrastructure_construction_effect = yes, the state infrastructure speeds it up;
    port: is_port; coastal: only_costal)."""

    def load():
        out = {}
        for f in game.files("common/buildings"):
            for k, v in parse_file(f):
                if k != "buildings" or not isinstance(v, list):
                    continue
                for name, b in v:
                    if not name or not isinstance(b, list):
                        continue
                    out[name] = {"cost": _num(get1(b, "base_cost"), 0.0),
                                 "per_level": _num(get1(b, "per_level_extra_cost"), 0.0),
                                 "infra_effect": get1(b, "infrastructure_construction_effect") == "yes",
                                 "port": get1(b, "is_port") == "yes",
                                 "coastal": get1(b, "only_costal") == "yes",
                                 "civilian": get1(b, "general_production") not in (None, "0"),
                                 "military": get1(b, "military_production") not in (None, "0")}
        return out

    return game.cached("derive:buildingcosts", load)


def construction_constants(game):
    """Construction points per civ, infrastructure step and project cap as calc.py labels them
    (the first two are not in the defines; the cap is MAX_CIV_FACTORIES_PER_CONTRACT)."""
    from . import calc  # lazy: calc imports this module

    return {"cp_per_civ": calc.CP_PER_CIV, "infra_step": calc.INFRA_STEP,
            "project_cap": int(define(game, "MAX_CIV_FACTORIES_PER_CONTRACT", calc.PROJECT_CAP_FALLBACK))}


def free_civilian_factories(game, ctx, tag, date=DEFAULT_START, history_file=None):
    """{"civs", "mils", "consumer_goods", "free"} at date: the civilian and military factories of the
    states tag owns, the consumer-goods share of its starting ideas (consumer_goods_expected_value,
    then consumer_goods_factor, as calc.py) and the civs left for construction (no trade deducted)."""
    costs = building_costs(game)
    rows = state_buildings(game, ctx, date)
    civs = mils = 0
    for sid in tag_states(game, ctx, tag, date):
        for b, lvl in rows.get(sid, {}).items():
            if costs.get(b, {}).get("civilian"):
                civs += lvl
            elif costs.get(b, {}).get("military"):
                mils += lvl
    mods = idea_modifiers(game)
    ideas = sorted(start_ideas(game, tag, ctx, date, history_file))
    share = sum(mods.get(i, {}).get("consumer_goods_expected_value", 0.0) for i in ideas)
    share += sum(mods.get(i, {}).get("consumer_goods_factor", 0.0) for i in ideas) * share
    free = max(0, civs - int(math.ceil(share * (civs + mils) - 1e-9)))
    return {"civs": civs, "mils": mils, "consumer_goods": round(share, 4), "free": free}


def construction_days(game, building, civs, infrastructure=0, levels=1):
    """Days one project of `levels` levels of building takes with `civs` free civilian factories:
    cost / (min(project cap, civs) x points per civ x (1 + step x infrastructure when the building
    has infrastructure_construction_effect)). None when the building is unknown or no civ works."""
    b = building_costs(game).get(building)
    k = construction_constants(game)
    n = min(k["project_cap"], civs or 0)
    if b is None or n <= 0:
        return None
    cost = b["cost"] + b["per_level"] * max(0, levels - 1)
    mult = 1 + k["infra_step"] * (infrastructure or 0) if b["infra_effect"] else 1.0
    return int(math.ceil(cost / (n * k["cp_per_civ"] * mult)))


def supply_hubs(game):
    """{province: level} of the supply hubs in map/supply_nodes.txt (the map graph), or {}."""
    graph = game.map_graph()
    if graph is None:
        return {}
    return {p: r["supply_hub"] for p, r in graph["provinces"].items() if r.get("supply_hub")}


def coastal_provinces(game):
    """Coastal land provinces (map/definition.csv), or an empty set without a map."""
    graph = game.map_graph()
    if graph is None:
        return set()
    return {p for p, r in graph["provinces"].items() if r.get("coastal") and r.get("type") == "land"}


def _top_keys(game, folder, wrapper=None):
    out = set()
    for f in game.files(folder):
        for k, v in parse_file(f):
            if wrapper is None:
                if k and isinstance(v, list):
                    out.add(k)
            elif k == wrapper and isinstance(v, list):
                out.update(n for n, b in v if n and isinstance(b, list))
    return out


def occupation_laws(game):
    """Occupation law keys of common/occupation_laws."""
    return game.cached("derive:occlaws", lambda: _top_keys(game, "common/occupation_laws"))


def air_base_levels(game, ctx, date=DEFAULT_START):
    """{state: air base level} at date (states without an air base are absent)."""
    kinds = air_base_buildings(game)
    out = {}
    for sid, row in state_buildings(game, ctx, date).items():
        lvl = sum(v for b, v in row.items() if b in kinds)
        if lvl:
            out[sid] = lvl
    return out
