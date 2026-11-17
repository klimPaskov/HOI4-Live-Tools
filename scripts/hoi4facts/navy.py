"""Navy force design: what each sea operation the plan schedules needs, derived from the plan's own operations and
the game files, like the land and air needs in force.py. lint_force_navy.py holds the plan to it.

A sea operation is
  invasion          a landing (an invasions[] entry, or a force_design.sea entry of kind invasion that a later
                    horizon carries out): sea control on every region of the route, lift, landing capacity, a
                    port, cover;
  overseas_supply   an army supplied over the sea (a sea_transfer route, or a force_design.sea entry): convoys,
                    escorts, and the enemy kept from controlling the route (a controlled region blocks convoys,
                    CONVOY_BLOCKED_BY_ENEMY_CONTROLLED_REGION);
  sea_control       regions the plan must hold (dominance of an ocean, a blockade): control of each region;
  deny              regions the enemy must not control: any control means, or submarines raiding there;
  raid              enemy convoys cut in named regions (a convoy_raiding mission, or a force_design.sea entry);
  escort            own convoys protected in named regions.
The plan states its operations in force_design.sea; invasions[], sea_transfer routes and convoy_raiding missions no
entry names are operations too. Every enemy figure is the start fleet of the enemy side from history/units (FAIR:
aggregates), the floor of what the enemy will have.

Per operation, by its date (`by`, else the date its `when` can first hold):
  enemy fleet      the enemy side (the operation's enemies, else its war's, less the ones the plan sees capitulated
                   by then): capital ships, carriers, screens, submarines by their unit role, and the surface hull IC.
  control means    the plan chooses one or more (force_design.sea[].means), each sized from the game files:
                   surface   our surface hull IC (start fleet plus the dockyard lines' output to the date) at least
                             SEA_CONTROL_RATIO x the enemy's (NA06: fight with 1-2x the enemy's ships);
                   air       naval strike planes at least NB_PER_HEAVY per enemy capital ship or carrier
                             (war-air-naval-peace.md AI20, germany-fair.md §9: 300-600 naval bombers by mid-1940
                             against Britain's 38 start capital ships, heavy cruisers included, and carriers: 8-16
                             each), with superiority over the route;
                   carrier   the same strike need met by carrier air: carrier-capable planes count
                             NAVAL_STRIKE_CARRIER_MULTIPLIER x BASE_CARRIER_SORTIE_EFFICIENCY times (the damage
                             bonus with their carrier present, half of them sortie), within the decks' capacity
                             (carrier_size x CARRIER_SIZE_STAT_INCREMENT);
                   submarine (deny and raid only) SUBS_PER_REGION submarines per region (NA11).
                   Dominance itself comes only from patrols (NA01): every control or deny operation needs a patrol
                   group of PATROL_GROUP screens per region (NA19) on top of the battle fleet's screens, started
                   PATROL_DAYS before.
  screens          3 per capital ship or carrier and one capital per carrier (SCREEN_RATIO_FOR_FULL_SCREENING_FOR_
                   CAPITALS, CAPITAL_RATIO_FOR_FULL_SCREENING_FOR_CARRIERS) for a surface or carrier fleet.
  lift             invasion: ESTIMATED_CONVOYS_PER_DIVISION convoys per division (the game's own AI estimate; the
                   exact transport need is not in the files); overseas supply: the convoy need the lint computes
                   (facts.convoys); escorts one per CONVOYS_PER_ESCORT convoys (NA16).
  landing          divisions per landing within BASE_NAVAL_INVASION_DIVISION_CAP plus the naval_invasion_division_cap
                   of the techs the research plan finishes by the date.
  port             a landing state with a naval base, or with_harbor.
  tools            the gap register rows each means rests on, with their status.
The date each operation's needs are first met comes from the same arithmetic, week by week.
"""
import math
from collections import Counter, deque

from . import derive
from . import force as fm

SEA_CONTROL_RATIO = 1.0   # surface means: our surface hull IC against the enemy side's (NA06: 1-2x per task force)
NB_PER_HEAVY = 12         # naval strike planes per enemy capital ship or carrier (AI20: 300-600 against Britain's 38)
PATROL_GROUP = 8          # screens of one patrol group per region (NA19: 8 destroyers or 4-6 cruisers)
PATROL_DAYS = 50          # patrols start this long before an invasion or blockade (NA01)
SUBS_PER_REGION = 8       # submarines per raid or deny region (NA11: task forces of 8-12)
CONVOYS_PER_ESCORT = 12   # one escort ship covers 12 convoys (NA16)
READY_SLACK = 0.9

ROLES = ("capital", "carrier", "screen", "submarine", "convoy")
_CLASS = {"capital_ship": "capital", "carrier": "carrier", "screen_ship": "screen", "submarine": "submarine",
          "convoy": "convoy"}
ROW_KIND = {"screen": "screen", "cruiser": "screen", "capital": "capital", "carrier": "carrier",
            "submarine": "submarine", "convoy": "convoy"}
CONTROL_KINDS = ("invasion", "sea_control")
CONTROL_MEANS = ("surface", "air", "carrier")
# the gap register rows each part of an operation rests on
GAP_ROWS = {
    "invasion": ["navy.naval_invasion"],
    "overseas_supply": ["navy.sea_transfer", "navy.convoy_escort", "navy.convoy_stock"],
    "escort": ["navy.convoy_escort", "navy.convoy_stock"],
    "raid": ["navy.convoy_raiding"],
    "control": ["navy.patrol_dominance", "navy.dominance_intel_read"],
    "surface": ["navy.strike_force", "navy.fleet_organisation"],
    "air": ["air.naval_and_port_strike", "air.recon_and_naval_patrol", "navy.enemy_fleet_read"],
    "carrier": ["air.carrier_air", "navy.strike_force"],
    "submarine": ["navy.convoy_raiding"],
    "bombardment": ["navy.invasion_support_bombardment"],
    "build": ["navy.ship_production"],
    "design": ["navy.ship_design"],
}


# ---------------------------------------------------------------------------------------------- game facts
def role_of(game, hull, definition=None):
    """capital, carrier, screen, submarine, convoy or None: the ship's unit role (common/units `type` of its
    definition: a heavy cruiser is a capital ship on a cruiser hull), else its hull's equipment type."""
    if definition:
        types = (derive.unit_types(game).get(definition) or {}).get("types") or set()
        for t in ("carrier", "capital_ship", "submarine", "screen_ship"):
            if t in types:
                return _CLASS[t]
    return _CLASS.get(fm.ship_class(game, hull))


def carrier_plane(game, key):
    """True when a plane equipment flies from carriers (carrier_capable = yes on it or its archetype/parent)."""
    def load():
        from .script import get1, parse_file
        raw = {}
        for f in game.files("common/units/equipment"):
            for k, v in parse_file(f):
                # duplicate_archetypes: the plane designer's generated families (x_plane_airframes.txt)
                if k in ("equipments", "duplicate_archetypes") and isinstance(v, list):
                    for name, body in v:
                        if name and isinstance(body, list):
                            raw[name] = (get1(body, "carrier_capable") == "yes",
                                         get1(body, "archetype") or get1(body, "parent"))
        return raw
    raw = game.cached("navy:carrier_capable", load)
    if not isinstance(key, str):
        return False
    for k in (key, derive.equipment_family(game, key)):
        seen = set()
        while k in raw and k not in seen:
            seen.add(k)
            if raw[k][0]:
                return True
            k = raw[k][1]
    return False


def seazone_minimum(game):
    """{naval terrain: minimum_seazone_dominance} from common/terrain."""
    def load():
        from .script import get1, parse_file
        out = {}
        for f in game.files("common/terrain"):
            for k, v in parse_file(f):
                if k == "categories" and isinstance(v, list):
                    for name, body in v:
                        if name and isinstance(body, list) and get1(body, "minimum_seazone_dominance") is not None:
                            try:
                                out[name] = float(get1(body, "minimum_seazone_dominance"))
                            except (TypeError, ValueError):
                                pass
        return out
    return game.cached("navy:seazone_min", load)


def fleet(lint, tag):
    """Start fleet of a country (history/units): ships and surface hull IC by role, the fleets and their bases,
    the start convoys."""
    cache = lint.__dict__.setdefault("_navy_fleets", {})
    if tag in cache:
        return cache[tag]
    g = lint.game
    rows = derive.equipment_rows(g)
    out = {"tag": tag, "ships": Counter(), "hull_ic": Counter(), "fleets": [], "convoys": 0}
    try:
        o = lint.world.oob(tag)
        c = lint.world.country(tag).get("convoys")
        out["convoys"] = int(c["v"]) if c and isinstance(c.get("v"), (int, float)) else 0
    except Exception:  # noqa: BLE001 - a tag without history: nothing known
        cache[tag] = out
        return out
    for f in o["fleets"]:
        units = f.get("units") or [{"hull": h, "definition": None, "n": n} for h, n in (f.get("ships") or {}).items()]
        ships = Counter()
        for u in units:
            role = role_of(g, u["hull"], u.get("definition"))
            if not role:
                continue
            ships[role] += u["n"]
            out["hull_ic"][role] += ((rows.get(u["hull"]) or {}).get("cost") or 0.0) * u["n"]
        out["ships"].update(ships)
        out["fleets"].append({"name": f.get("name"), "base": f.get("base"), "ships": dict(ships)})
    out["ships"], out["hull_ic"] = dict(out["ships"]), {k: round(v) for k, v in out["hull_ic"].items()}
    cache[tag] = out
    return out


def side_fleet(lint, tags):
    ships, ic, convoys = Counter(), Counter(), 0
    for t in tags:
        f = fleet(lint, t)
        ships.update(f["ships"])
        ic.update(f["hull_ic"])
        convoys += f["convoys"]
    surface = sum(ic.get(r, 0) for r in ("capital", "carrier", "screen"))
    return {"tags": list(tags), "ships": dict(ships), "hull_ic": dict(ic), "surface_ic": round(surface),
            "heavy": ships.get("capital", 0) + ships.get("carrier", 0), "convoys": convoys}


def components(lint):
    """{land province: landmass id}: provinces joined by land and straits (any owner)."""
    cache = lint.__dict__.get("_navy_components")
    if cache is not None:
        return cache
    adj = derive.land_adjacency(lint.game) or {}
    comp = {}
    for p0 in adj:
        if p0 in comp:
            continue
        comp[p0] = p0
        q = deque([p0])
        while q:
            a = q.popleft()
            for b in adj.get(a, ()):
                if b not in comp:
                    comp[b] = p0
                    q.append(b)
    lint._navy_components = comp
    return comp


def state_land(lint, sid):
    """The landmass ids of a state's provinces."""
    comp = components(lint)
    return {comp[p] for p in lint.game.states().get(sid, {}).get("provinces", []) if p in comp}


def home_land(lint):
    try:
        cap = derive.capital_state(lint.game, lint.tag, lint.ctx, lint.start, lint.meta.get("history_file"))
    except Exception:  # noqa: BLE001
        cap = None
    return state_land(lint, cap) if cap is not None else set()


def across_sea(lint, sid):
    """True when our capital cannot reach the state over land or straits (any owner): it is taken by sea."""
    home, there = home_land(lint), state_land(lint, sid)
    return bool(home) and bool(there) and not (home & there)


def same_land(lint, a, b):
    return bool(state_land(lint, a) & state_land(lint, b))


# ---------------------------------------------------------------------------------------------- our navy by date
class Programme:
    """Our ships, convoys and naval strike planes by date: the start fleet plus the dockyard lines' output (dockyards x
    POWERED_FACTORY_SPEED_NAV / the ship's cost with its design's modules; naval output modifiers left out, so the
    count is a floor) and the plane lines' output (lint_force.Ctx.air_have)."""

    def __init__(self, ctx):
        self.ctx = ctx
        self.lint = ctx.lint
        self.g = ctx.g
        lint = self.lint
        self.speed = lint.define("POWERED_FACTORY_SPEED_NAV", lint.define("BASE_FACTORY_SPEED_NAV", 2.5))
        self.start = fleet(lint, lint.tag)
        self.rows = derive.equipment_rows(self.g)
        self.weeks = []  # (day, Counter ships by role, Counter hull IC by role, Counter by equipment)
        total, ic, by_eq = Counter(), Counter(), Counter()
        for r in lint.start_lines():  # a start ship line carries the progress of its first hull
            if lint.factory_pool(r["type"]) == "dockyards" and isinstance(r.get("progress"), (int, float)):
                by_eq[r["type"]] = max(by_eq[r["type"]], min(0.99, float(r["progress"])))
        d = ctx.tl.start
        end = ctx.tl.forecast_end()
        self.first_hull = {}  # equipment: first day a whole hull is done
        while d <= end:
            for eq, n in lint.pool_lines(d)["dockyards"].items():
                cost = self.cost(eq)
                if not cost or not n:
                    continue
                units = n * self.speed * 7 / cost
                role = role_of(self.g, eq) if derive.equipment_family(self.g, eq) != "convoy" else "convoy"
                role = role or ("convoy" if "convoy" in (derive.equipment_types(self.g).get(eq) or ()) else None)
                if not role:
                    continue
                total[role] += units
                by_eq[eq] += units
                ic[role] += units * ((self.rows.get(eq) or {}).get("cost") or 0.0)
                if by_eq[eq] >= 1 and eq not in self.first_hull:
                    self.first_hull[eq] = d
            self.weeks.append((d, Counter(total), Counter(ic), Counter(by_eq)))
            d += 7

    def cost(self, eq):
        return fm.unit_cost(self.lint, eq)[0] or (self.rows.get(eq) or {}).get("cost")

    def made(self, day):
        last = (None, Counter(), Counter(), Counter())
        for w in self.weeks:
            if w[0] > day:
                break
            last = w
        return last

    def ships(self, day):
        _, n, _, _ = self.made(day)
        out = Counter(self.start["ships"])
        out["convoy"] = self.start["convoys"]
        for k, v in n.items():
            out[k] += v
        return out

    def surface_ic(self, day):
        _, _, ic, _ = self.made(day)
        return sum(self.start["hull_ic"].get(r, 0) for r in ("capital", "carrier", "screen")) + \
            sum(ic.get(r, 0) for r in ("capital", "carrier", "screen"))

    def surface_rate(self, day):
        """Surface hull IC a year the dockyard lines in force on day add."""
        out = 0.0
        for eq, n in self.lint.pool_lines(day)["dockyards"].items():
            if role_of(self.g, eq) in ("capital", "carrier", "screen"):
                cost = self.cost(eq)
                if cost:
                    out += n * self.speed * 365 / cost * ((self.rows.get(eq) or {}).get("cost") or 0.0)
        return out

    def dockyards(self, day):
        return sum(self.lint.pool_lines(day)["dockyards"].values())

    def strike_planes(self, day):
        """(land-based naval strike planes, carrier-capable naval strike planes) on day."""
        nb = self.ctx.air_have("naval_bomber", day)[0]
        cv = self.ctx.carrier_have(day)
        return max(0.0, nb - cv.get("naval_bomber", 0.0)), cv

    def deck(self, day):
        """Planes our carriers hold on day: carrier_size of the start and planned carriers x
        CARRIER_SIZE_STAT_INCREMENT (a start or design variant's deck modules, else the hull's own carrier_size)."""
        inc = self.lint.define("CARRIER_SIZE_STAT_INCREMENT", 10)
        n = self.ships(day).get("carrier", 0)
        return n * deck_size(self.lint) * inc


def deck_size(lint):
    """carrier_size of one carrier: the deck modules of the plan's carrier designs or start variants (mean), else 2
    decks of the commonest deck module (common/units/equipment/modules)."""
    mods = lint.world.equipment_modules()
    sizes = []
    variants = list(lint.world.country(lint.tag).get("variants") or []) if lint.tag else []
    variants += [{"type": d.get("base"), "modules": d.get("modules")} for d in lint.list_of("designs")]
    for v in variants:
        if role_of(lint.game, v.get("type")) != "carrier" or not isinstance(v.get("modules"), dict):
            continue
        s = 0.0
        for m in v["modules"].values():
            a = (mods.get(m) or {}).get("add_stats") or {}
            s += float(a.get("carrier_size") or 0) if isinstance(a, dict) else 0.0
        if s:
            sizes.append(s)
    if sizes:
        return sum(sizes) / len(sizes)
    deck = [float(((r.get("add_stats") or {}).get("carrier_size") or 0)) for r in mods.values()
            if isinstance(r.get("add_stats"), dict) and (r.get("add_stats") or {}).get("carrier_size")]
    return 2 * max(deck) if deck else 4.0


# ---------------------------------------------------------------------------------------------- operations
def _region_index(lint):
    regions = lint.world.strategic_regions()
    by_name = {}
    for rid, r in regions.items():
        if r.get("naval_terrain"):
            by_name[str(r.get("name") or rid).strip().lower()] = rid
            by_name[str(rid)] = rid
    return regions, by_name


def resolve_regions(lint, items):
    regions, by_name = _region_index(lint)
    out, unknown = [], []
    for r in items or []:
        name = r.get("region") if isinstance(r, dict) else r
        if isinstance(name, bool) or not isinstance(name, (str, int)):
            continue
        rid = by_name.get(str(name).strip().lower())
        if rid is None:
            unknown.append(str(name))
        elif rid not in [x["id"] for x in out]:
            out.append(region_row(lint, rid, regions))
    return out, unknown


def region_row(lint, rid, regions=None):
    regions = regions or lint.world.strategic_regions()
    r = regions.get(rid) or {}
    terrain = r.get("naval_terrain")
    return {"id": rid, "name": r.get("name") or str(rid), "terrain": terrain,
            "min_dominance": seazone_minimum(lint.game).get(terrain)}


def route_regions(lint, src_state, dst_state, dst_province=None):
    if not isinstance(src_state, int) or not isinstance(dst_state, int):
        return []
    rows = lint.sea_regions({"from_state": src_state, "target_state": dst_state,
                             "target_province": dst_province})
    regions = lint.world.strategic_regions()
    return [region_row(lint, rid, regions) for rid, _ in rows]


def _war_for(lint, wars, op, owner):
    fd = lint.plan.get("force_design") if isinstance(lint.plan.get("force_design"), dict) else {}
    named = {x.get("id"): x for x in fd.get("wars") or [] if isinstance(x, dict)}
    ref = op.get("war")
    if ref in named:
        en = set(named[ref].get("enemies") or [])
        for w in wars:
            if en & set(w["targets"]):
                return w
    for w in wars:
        if w.get("id") == ref:
            return w
    if owner:
        for w in wars:
            if owner in w["enemy_tags"]:
                return w
    return None


def operations(ctx):
    """[op] in date order: the plan's force_design.sea entries, then the invasions[], sea_transfer routes and
    convoy_raiding missions no entry names."""
    lint, tl, wars = ctx.lint, ctx.tl, ctx.wars
    states = lint.world.states()
    caps = fm.capitulation_days(lint)
    fd = ctx.fd or {}
    ops = []
    for i, e in enumerate(fd.get("sea") or []):
        if not isinstance(e, dict):
            continue
        ops.append(dict(e, _where=f"force_design.sea[{i}] {e.get('id', '')}".strip(), _declared=True))
    named_inv = {e.get("invasion") for e in ops if e.get("invasion")}
    named_routes = {e.get("route") for e in ops if e.get("route")}
    for i, inv in enumerate(lint.list_of("invasions")):
        if inv.get("id") in named_inv:
            continue
        ops.append({"id": inv.get("id"), "kind": "invasion", "invasion": inv.get("id"),
                    "_where": f"invasions[{i}] {inv.get('id', '')}".strip(), "_declared": False})
    st = lint.plan.get("sea_transfer") if isinstance(lint.plan.get("sea_transfer"), dict) else {}
    for i, r in enumerate(st.get("routes") or []):
        if isinstance(r, dict) and r.get("id") not in named_routes:
            ops.append({"id": r.get("id"), "kind": "overseas_supply", "route": r.get("id"),
                        "_where": f"sea_transfer.routes[{i}] {r.get('id', '')}".strip(), "_declared": False})
    if not any(e.get("kind") == "raid" for e in ops):
        nv = lint.plan.get("navy") if isinstance(lint.plan.get("navy"), dict) else {}
        for fi, fl in enumerate(nv.get("fleets") or []):
            for mi, m in enumerate((fl or {}).get("missions") or [] if isinstance(fl, dict) else []):
                if isinstance(m, dict) and m.get("mission") == "convoy_raiding":
                    ops.append({"id": f"raid_{fi}_{mi}", "kind": "raid", "regions": m.get("regions") or [],
                                "when": m.get("when"), "means": ["submarine"],
                                "_where": f"navy.fleets[{fi}].missions[{mi}]", "_declared": False})
    invs = {x.get("id"): x for x in lint.list_of("invasions")}
    routes = {x.get("id"): x for x in st.get("routes") or [] if isinstance(x, dict)}
    out = []
    for e in ops:
        op = {"id": e.get("id"), "kind": e.get("kind"), "where": e["_where"], "declared": e["_declared"],
              "means": [m for m in e.get("means") or [] if isinstance(m, str)], "fallback": e.get("fallback"),
              "war": e.get("war"), "unknown_regions": []}
        inv = invs.get(e.get("invasion")) if e.get("invasion") else None
        route = routes.get(e.get("route")) if e.get("route") else None
        src = e.get("from_state") or (inv or {}).get("from_state") or (route or {}).get("from_state")
        dst = e.get("target_state") or (inv or {}).get("target_state") or (route or {}).get("to_state")
        prov = e.get("target_province") or (inv or {}).get("target_province")
        op.update({"from_state": src, "target_state": dst, "target_province": prov,
                   "with_harbor": bool(e.get("with_harbor") or (inv or {}).get("with_harbor"))})
        day = derive.day(e["by"]) if isinstance(e.get("by"), str) and derive.day(e["by"]) is not None else None
        if day is None:
            pred = e.get("when", (inv or {}).get("when", (route or {}).get("when")))
            day = fm.predicate_day(lint, tl, pred, wars) if pred is not None else None
        owner = (states.get(dst) or {}).get("owner") if isinstance(dst, int) else None
        war = _war_for(lint, wars, e, owner)
        if day is None and war is not None and op["kind"] in ("sea_control", "deny", "raid", "escort"):
            day = war["start"]
        op["day"], op["date"] = day, derive.fmt(derive.date_of(day)) if day is not None else None
        enemies = [t for t in e.get("enemies") or [] if isinstance(t, str)]
        if not enemies and not owner and war is None:
            pred = e.get("when", (inv or {}).get("when", (route or {}).get("when")))
            enemies = [a.split(":", 1)[1] for a in fm._atoms(pred) if a.startswith("at_war_with:")]
            if enemies:
                war = next((w for w in wars if enemies[0] in w["enemy_tags"]), None)
        if not enemies:
            enemies = list(war["enemy_tags"]) if war else []
            if owner and owner != lint.tag and owner not in enemies:
                enemies.insert(0, owner)
        if day is not None:
            enemies = [t for t in enemies if not (t in caps and caps[t] <= day)]
        op["enemies"] = enemies
        op["enemy"] = side_fleet(lint, enemies)
        regs, unknown = resolve_regions(lint, e.get("regions"))
        if not regs and src and dst:
            regs = route_regions(lint, src, dst, prov)
        op["regions"], op["unknown_regions"] = regs, unknown
        if op["kind"] == "invasion":
            op["divisions"] = e.get("divisions") if isinstance(e.get("divisions"), int) else \
                _invasion_divisions(lint, inv)
            if not op["means"]:
                op["means"] = ["surface"]
        if op["kind"] == "overseas_supply" and not op["means"]:
            op["means"] = ["surface"]
        if isinstance(e.get("convoys"), int):
            op["convoys_stated"] = e["convoys"]
        out.append(op)
    out.sort(key=lambda o: (o["day"] is None, o["day"] or 0))
    return out


def _invasion_divisions(lint, inv):
    if not inv:
        return None
    layout = Counter()
    for e in lint.list_of("army_layout"):
        if isinstance(e.get("name_hint"), str):
            layout[e["name_hint"]] += e.get("count", 1) if isinstance(e.get("count"), int) else 1
    n = sum(layout.get(a, 0) for a in inv.get("armies") or [] if isinstance(a, str))
    return n or None


def landing_cap(lint, day):
    """Divisions one landing takes on day: BASE_NAVAL_INVASION_DIVISION_CAP + the techs in hand then."""
    tmods = derive.tech_modifiers(lint.game)
    sched = lint.research_schedule(max(day, derive.day(lint.start)))
    have = set(sched["have"]) | {t for t, d in sched["finish"].items() if d is not None and d <= day}
    return lint.define("BASE_NAVAL_INVASION_DIVISION_CAP", 4) + \
        sum(tmods.get(t, {}).get("naval_invasion_division_cap", 0.0) for t in have)


def needs(ctx, prog, op):
    """The needs of one operation on its date and whether the plan meets them, with the arithmetic."""
    lint = ctx.lint
    day = op["day"]
    E = op["enemy"]
    n_regions = max(1, len(op["regions"]))
    kind = op["kind"]
    means = set(op["means"])
    rows = []
    if day is None:
        return {"rows": [], "met": None}
    ships = prog.ships(day)
    control = kind in CONTROL_KINDS or (kind == "overseas_supply" and E["surface_ic"] > 0)
    deny = kind == "deny"
    carrier_w = lint.define("NAVAL_STRIKE_CARRIER_MULTIPLIER", 12.0) * lint.define("BASE_CARRIER_SORTIE_EFFICIENCY", 0.5)
    if control or deny:
        options = []
        ours_ic = prog.surface_ic(day)
        want_ic = SEA_CONTROL_RATIO * E["surface_ic"]
        rate = prog.surface_rate(day)
        surf_ok = ours_ic >= want_ic
        options.append({"means": "surface", "ok": surf_ok, "have": round(ours_ic), "want": round(want_ic),
                        "years_at_plan_dockyards": (round((want_ic - ours_ic) / rate, 1) if rate else None)
                        if not surf_ok else 0,
                        "why": f"our surface hull IC {round(ours_ic)} against {SEA_CONTROL_RATIO:g} x the enemy's "
                               f"{E['surface_ic']} ({', '.join(E['tags']) or 'none'})"})
        land_nb, cv = prog.strike_planes(day)
        deck = prog.deck(day)
        cv_strike = min(cv.get("naval_bomber", 0.0) + cv.get("cas", 0.0), deck)
        want_nb = NB_PER_HEAVY * E["heavy"]
        air_have = land_nb + ("carrier" in means and cv_strike * carrier_w or 0.0)
        options.append({"means": "air", "ok": air_have >= READY_SLACK * want_nb, "have": round(land_nb),
                        "want": want_nb,
                        "why": f"{NB_PER_HEAVY} naval strike planes per enemy capital ship or carrier "
                               f"({E['heavy']}), with air superiority over the regions"})
        options.append({"means": "carrier", "ok": cv_strike * carrier_w >= READY_SLACK * want_nb and deck > 0,
                        "have": round(cv_strike), "deck": round(deck), "want": math.ceil(want_nb / carrier_w),
                        "why": f"carrier strike planes x {carrier_w:g} (NAVAL_STRIKE_CARRIER_MULTIPLIER x "
                               f"BASE_CARRIER_SORTIE_EFFICIENCY) against the same need, within the decks"})
        if deny or kind == "overseas_supply":
            subs = ships.get("submarine", 0)
            options.append({"means": "submarine", "ok": subs >= SUBS_PER_REGION * n_regions, "have": round(subs),
                            "want": SUBS_PER_REGION * n_regions,
                            "why": f"{SUBS_PER_REGION} submarines raiding in each of {n_regions} regions lower the "
                                   "enemy's dominance there"})
        chosen = [o for o in options if o["means"] in means] or [options[0]]
        if E["surface_ic"] == 0 and E["heavy"] == 0:
            chosen_ok = True
        elif "air" in means and "carrier" in means:
            chosen_ok = any(o["ok"] for o in chosen) or air_have >= READY_SLACK * want_nb
        else:
            chosen_ok = any(o["ok"] for o in chosen)
        rows.append({"need": "control" if control else "deny", "ok": chosen_ok, "chosen": sorted(means),
                     "options": options})
        # patrols build the dominance (NA01): one group per region on top of the battle fleet's screens
        heavy_ours = ships.get("capital", 0) + ships.get("carrier", 0) if means & {"surface", "carrier"} else 0
        screen_ratio = lint.define("SCREEN_RATIO_FOR_FULL_SCREENING_FOR_CAPITALS", 3.0)
        want_screens = PATROL_GROUP * len(op["regions"]) + screen_ratio * heavy_ours
        rows.append({"need": "screens", "ok": ships.get("screen", 0) >= READY_SLACK * want_screens,
                     "have": round(ships.get("screen", 0)), "want": round(want_screens),
                     "why": f"a patrol group of {PATROL_GROUP} screens in each of {len(op['regions'])} regions "
                            f"(only patrols build dominance) plus {screen_ratio:g} per capital ship or carrier "
                            f"of the battle fleet ({round(heavy_ours)})"})
        if means & {"surface", "carrier"} and ships.get("carrier", 0):
            cap_ratio = lint.define("CAPITAL_RATIO_FOR_FULL_SCREENING_FOR_CARRIERS", 1.0)
            rows.append({"need": "capitals_per_carrier", "ok": ships.get("capital", 0) >= cap_ratio * ships["carrier"],
                         "have": round(ships.get("capital", 0)), "want": round(cap_ratio * ships["carrier"]),
                         "why": "one capital ship per carrier for full screening"})
        if "carrier" in means:
            rows.append({"need": "carrier_air", "ok": deck > 0 and cv_strike + cv.get("fighter", 0) >= 0.5 * deck,
                         "have": round(cv_strike + cv.get("fighter", 0)), "want": round(deck),
                         "why": "carrier-capable planes to fill the decks (carrier_size x CARRIER_SIZE_STAT_INCREMENT)"})
    if kind == "raid" or (deny and "submarine" in means):
        subs = ships.get("submarine", 0)
        rows.append({"need": "submarines", "ok": subs >= SUBS_PER_REGION * n_regions, "have": round(subs),
                     "want": SUBS_PER_REGION * n_regions,
                     "why": f"task forces of {SUBS_PER_REGION}-12, one per raid region (NA11)"})
    convoys_need = 0
    if kind == "invasion" and isinstance(op.get("divisions"), int):
        per = lint.define("ESTIMATED_CONVOYS_PER_DIVISION", 6)
        convoys_need = op.get("convoys_stated") or math.ceil(op["divisions"] * per)
        cap = landing_cap(lint, day)
        rows.append({"need": "landing", "ok": op["divisions"] <= cap, "have": cap, "want": op["divisions"],
                     "why": "divisions one landing takes (BASE_NAVAL_INVASION_DIVISION_CAP + the landing techs the "
                            "research plan finishes by the date)"})
    elif kind == "overseas_supply":
        convoys_need = op.get("convoys_stated") or (lint.facts.get("convoys") or {}).get("need") or 0
    if convoys_need:
        rows.append({"need": "lift", "ok": ships.get("convoy", 0) >= convoys_need, "have": round(ships.get("convoy", 0)),
                     "want": convoys_need,
                     "why": ("divisions x ESTIMATED_CONVOYS_PER_DIVISION (the game's AI estimate)" if kind == "invasion"
                             else "the sea supply and trade convoy need (facts.convoys)")})
    if kind in ("overseas_supply", "escort") or (kind == "invasion" and convoys_need):
        want = math.ceil((convoys_need or (lint.facts.get("convoys") or {}).get("need") or 0) / CONVOYS_PER_ESCORT)
        if want:
            rows.append({"need": "escorts", "ok": ships.get("screen", 0) >= want, "have": round(ships.get("screen", 0)),
                         "want": want, "why": f"one escort per {CONVOYS_PER_ESCORT} convoys on the route (NA16)"})
    if kind == "invasion" and isinstance(op.get("target_state"), int) and not op["with_harbor"]:
        graph = lint.game.map_graph() or {}
        bases = ((graph.get("states") or {}).get(op["target_state"]) or {}).get("naval_bases") or {}
        rows.append({"need": "port", "ok": bool(bases), "have": sorted(bases)[:4], "want": "a naval base",
                     "why": "without a port the landed troops run out of supply after 72 hours"})
    return {"rows": rows, "met": all(r["ok"] for r in rows)}


def met_date(ctx, prog, op):
    """First week from the start the operation's needs all hold (None when not by its date + 3 years)."""
    if op["day"] is None:
        return None
    end = min(op["day"] + 3 * 365, ctx.tl.forecast_end())
    d = ctx.tl.start
    probe = dict(op)
    while d <= end:
        probe["day"] = d
        if needs(ctx, prog, probe)["met"]:
            return d
        d += 28
    return None


def gap_rows(op):
    keys = list(GAP_ROWS.get(op["kind"], []))
    if op["kind"] in CONTROL_KINDS or op["kind"] == "deny":
        keys += GAP_ROWS["control"]
    for m in op["means"]:
        keys += GAP_ROWS.get(m, [])
    if op["kind"] == "invasion" and "surface" in op["means"]:
        keys += GAP_ROWS["bombardment"]
    return list(dict.fromkeys(keys))


def gap_status():
    try:
        from . import gap_register
        return {r["id"]: r for r in gap_register.annotate(gap_register.load())}
    except Exception:  # noqa: BLE001 - without the register no tool status is known
        return {}


def fuel_per_day(lint, prog, op):
    """Fuel a day the patrol groups of an operation burn on mission: screens x the destroyer hull's
    fuel_consumption x NNavy FUEL_COST_MULT x 24 (the hour basis is not confirmed: reported, not judged)."""
    mult = lint.full_define("NNavy.FUEL_COST_MULT", None)
    hull, fc = None, None
    lines = list(lint.pool_lines(op["day"] or derive.day(lint.start))["dockyards"]) + ["ship_hull_light_1"]
    lines.sort(key=lambda eq: derive.equipment_family(lint.game, eq) != "ship_hull_light")
    for eq in lines:
        if role_of(lint.game, eq) == "screen":
            fc = fm.fuel_use(lint.game, eq)
            hull = eq
            if fc:
                break
    if not fc or not mult:
        return None
    ships = PATROL_GROUP * max(1, len(op["regions"]))
    return {"ships": ships, "hull": hull, "fuel_consumption": fc, "mult": mult,
            "per_day": round(ships * fc * mult * 24, 1)}
