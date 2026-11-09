"""FAIR ruleset checks (rule ids fair.*), run by lint_plan.py (Lint.run) on every plan whose mode is fair.

FAIR mode is the multiplayer ruleset in skills/hoi4-campaign-playbook/references/rules-and-modes.md plus the
house rules. This module checks what a plan can break; the executor guards below cover what only the live
game shows. The mode is the first of _validator.mode, _meta.mode, run_spec.mode and _run_spec.mode in the plan
(the strategist copies RUN/run_spec.json's mode into _validator.mode); a plan without one is fair, a META plan
gets no fair.* finding. Nation rules bind the plan's own tag (single player: the agent's nation follows the
rules of its nation and side). A host-approved exception is `_fair.exceptions` {"fair.<rule>": "<reason>"}:
that rule's findings become info carrying the reason. Unit, equipment, module, technology and project keys
come from the effective game content (install plus mods); tags and key states are the historical setting's.

Rules (error unless marked; every message ends with the rule in plain words):
  fair.width            a template the plan makes or trains below 10 combat width (before doctrines) while the
                        plan has a war (at the start, war.enemies, a declaration or an invasion); start
                        divisions are exempt
  fair.space_marines    infantry or cavalry battalions with a line tank, SPG, tank destroyer or SPAA battalion
  fair.paratroopers     a paratrooper template below 20 width; more than 18 paratrooper divisions (start ones
                        plus training caps); (warning) a paratrooper training line without a cap
  fair.special_forces   special forces battalions added to a template whose fielded divisions are reorganised
                        (upgrade_fielded: a cap bypass); (warning) planned special forces battalions above the
                        cap estimate (SPECIAL_FORCES_CAP_MIN, SPECIAL_FORCES_CAP_BASE x other battalions)
  fair.submarines       research (path, priority, fallback), a special project, production, a design or a call
                        for a submarine above Submarine II or an AIP or coating module; a start fleet of only
                        submarines below 5 given a mission
  fair.naval_mines      mine_laying fleet missions, naval_mines air missions; (warning) mine-laying modules
  fair.access           military access, docking rights or airbase access asked or offered; (warning) the
                        European Axis with Japan, or the Allies with the Soviet Union (allowed once the Iran
                        corridor or all of mainland Asia is secured)
  fair.factions         leaving a faction the plan leads (warning when it may not lead one); Japan's side
                        joining or inviting the European Axis, or the reverse
  fair.justify          a war-goal justification that can run before the Danzig crisis (Germany at war with
                        Poland: date 1939.09.01, at_war_with:POL for Germany, at_war_with:GER, capitulated:POL);
                        exceptions: the Italian and Balkan Axis on the Balkans, the Soviet Union on Iran after
                        the Molotov-Ribbentrop Pact
  fair.bombing          strategic bombing aimed at refineries
  fair.blockade         a convoy_raiding mission that can run while the plan is at peace
  fair.deletion         an action that deletes or demolishes factories, stockpiles or research facilities
                        (warning for dockyards: allowed only when the state is out of slots)
  fair.ghost_divisions  (warning) a training line without a stockpile guard (only_if_stockpile)
  fair.licence          a plane design built on equipment the plan only has by licence
  fair.volunteers       volunteers outside Spain; (warning) USA to China, the Soviet Union to China (limits)
  fair.timing           Germany against the Soviet Union before 1941 (warning after 1942); Japan, Manchukuo or
                        Siam against the Allies before 1941 or the Soviet Union (warning); the Soviet Union
                        against the Axis before 1942; Italy joining the war after 1941.01.01 (warning when it
                        never joins within the horizon); the USA joining before its conditions; Spain joining
                        the Axis before France falls; Finland joining the Axis in the Winter War (warning); the
                        Netherlands or Belgium joining the Allies unprovoked; a manual justification where the
                        rules require the focus; the USA trading with the UK or France before Danzig
  fair.invasions        an invasion before its prerequisites: Sea Lion (2 of Gibraltar, Malta, Suez; 1 once at
                        war with the Soviet Union), Japan's side into Australia or New Zealand (Papua New
                        Guinea) or mainland North America (Hawaii and the Philippines; Alaska excepted), the
                        Allies into mainland Japan (Iwo Jima, Taiwan, Okinawa) or the core Italian mainland
                        (North Africa, or Italy capitulated), Allies from outside mainland Europe into it
                        before France capitulates (invasions, sea transfers, expeditionary forces), Soviet
                        naval invasions of Finland
  fair.france           France before capitulation: ports or air bases built in France, troops sent outside
                        mainland Europe (training, sea transfers, invasions; warning for army_layout), lend-
                        lease except to Spain

Executor guards: rules the executor must refuse at action time in FAIR mode (the plan lint cannot see them).
native/src/fair_guard.rs refuses (error fair_rule_refused, message starting with the rule id) the ones the
action's arguments, the plan and the game files decide, for the plan runner and direct tool calls alike; its
module doc lists what it guards and what is not guarded yet (needs live state):
  hoi4_fleet_mission        mine_laying; convoy_raiding while at peace; any mission for a submarine task force
                            of fewer than 5 submarines; an Italian surface fleet out of core Italian ports when
                            Italy joins the war
  (engagement setting)      always engage for submarines (no tool sets it today; refuse when one is added)
  hoi4_air_mission          naval_mines; a bomber wing's region or mission changed back and forth (flicker
                            bombing: no second switch within 7 days, never on enemy fighters appearing);
                            strategic bombing aimed at refineries (refuse any target option that names them)
  hoi4_diplomacy_act        ask/offer military access, docking rights or airbase access (side exceptions only
                            with the corridor verified); justify_war_goal / add_war_goal while Germany and Poland
                            are not at war (hoi4_wars), outside the exceptions; send_volunteers outside Spain
                            and the USA / Soviet China limits; join_faction, declare_war and invitations against
                            the nation timing rules; start_lend_lease by France before capitulation;
                            expeditionary_force into mainland Europe by outside Allies before France falls
  hoi4_faction_leave        as faction leader (dismantles the faction)
  hoi4_division_disband     divisions encircled or about to be (hoi4_encirclement_candidates, pocket flags)
  hoi4_template_edit        allow_in_use while a division of the template is encircled or about to be; adding
                            special forces battalions to a fielded template; space marines; para below 20 width
  hoi4_template_create      space marines; paratrooper templates below 20 width; templates below 10 width at war
  hoi4_train                templates below 10 width at war; space marines; paratroopers beyond 18 divisions;
                            training into an empty stockpile (ghost divisions)
  hoi4_army_move / _plan / _front / _attack  tank divisions parked in a port province with no battle on or next
                            to it; recalling divisions encircled or about to be
  hoi4_research_start       technologies above Submarine II; hoi4_special_project_assign: AIP, coating and
                            advanced submarine projects
  hoi4_design_create        AIP or coating modules, submarine hulls above II, mine-laying modules; plane designs
                            on licensed equipment (a role change)
  hoi4_production_add / hoi4_design_set_production  submarine hulls above II
  hoi4_construction_add / _place  France: ports or air bases in France before capitulation; any demolition of
                            factories or research facilities (dockyards only when out of slots)
  hoi4_naval_invasion       the invasion prerequisites of fair.invasions, checked against hoi4_states
  hoi4_operative_assign (operations)  Axis: no collaboration government in the Soviet Union
  (equipment toggles)       no mastery grinding with the main equipment type switched off (no tool today)

Stdlib only, Python 3.9+.
"""
import re
from collections import Counter, defaultdict, deque

from . import derive
from .script import bare, get1, parse_file
from .validate import _world, parse_atom, start_tech_set, template_columns

DANZIG = (1939, 9, 1)
MOLOTOV = (1939, 8, 23)
MIN_WIDTH = 10
PARA_WIDTH = 20
PARA_MAX = 18
SUB_FLEET_MIN = 5

EURO_AXIS = {"GER", "ITA", "HUN", "ROM", "BUL", "FIN", "SLO", "CRO"}
JAPAN_SIDE = {"JAP", "MAN", "SIA", "MEN"}
AXIS = EURO_AXIS | JAPAN_SIDE
ALLIES = {"ENG", "FRA", "USA", "CAN", "AST", "NZL", "SAF", "RAJ", "HOL", "BEL", "LUX", "PHI"}
OUTSIDE_ALLIES = {"ENG", "USA", "CAN", "AST", "NZL", "SAF", "RAJ"}
SPAIN = {"SPR", "SPA", "SPB", "SPC", "SPD"}
BALKAN_AXIS = {"ITA", "HUN", "ROM", "BUL"}
BALKANS = {"ALB", "YUG", "GRE", "BUL", "ROM"}
LOW_COUNTRIES = {"HOL", "BEL"}

RULES = {
    "fair.width": "deployed divisions at war are at least 10 combat width, counted before doctrines; starting "
                  "divisions are exempt",
    "fair.space_marines": "no space marines: no infantry or cavalry battalions with tank battalions in one "
                          "division (tank support companies are fine)",
    "fair.paratroopers": "paratrooper divisions are at least 20 width and a nation has at most 18 of them",
    "fair.special_forces": "no intentional bypass of the special forces division limit",
    "fair.submarines": "submarines: nothing above Submarine II, no special submarine modules (AIP, coating), at "
                       "least 5 per submarine fleet, no always engage",
    "fair.naval_mines": "naval mining is banned",
    "fair.access": "no military access, docking rights or airbase access (only for civil-war supplies)",
    "fair.factions": "no disbanding factions; Japan's side never joins the Axis and no European Axis nation "
                     "joins Japan's faction",
    "fair.justify": "no manual war-goal justification before the Danzig crisis, except the listed exceptions",
    "fair.bombing": "strategic bombing may not target refineries",
    "fair.blockade": "no naval blockade in peacetime",
    "fair.deletion": "no deleting stockpiles, factories (dockyards only when out of slots) or research facilities",
    "fair.ghost_divisions": "no ghost divisions without equipment",
    "fair.licence": "no changing the type or role of licensed aircraft, nor any way around licence penalties",
    "fair.volunteers": "no volunteers outside Spain (the USA may send them to China, the Allies and the Soviet "
                       "Union; the Soviet Union to China)",
    "fair.timing": "the nation rules on when a war, a faction join or a justification may happen",
    "fair.invasions": "the nation rules on what must be taken before an invasion",
    "fair.france": "France before capitulation: no building or upgrading ports or air bases in France, all "
                   "troops in mainland Europe, no lend-lease (Republican Spain excepted)",
}

_SPECIAL_SUB = re.compile(r"aip|anechoic|coating", re.I)
_DELETE_VERB = re.compile(r"delete|demolish|scrap|destroy|remove_building", re.I)
_DELETE_WHAT = re.compile(r"factor|industrial|stockpile|facilit|research|dockyard", re.I)


def mode_of(plan):
    """"fair" or "meta" (anything but meta is fair; no mode is fair)."""
    for key in ("_validator", "_meta", "run_spec", "_run_spec"):
        v = plan.get(key)
        if isinstance(v, dict) and isinstance(v.get("mode"), str) and v["mode"].strip():
            return "meta" if v["mode"].strip().lower() == "meta" else "fair"
    return "fair"


def kind_of(action):
    """Rule class of a diplomacy action key or row name."""
    a = re.sub(r"[^a-z]+", "_", str(action or "").lower()).strip("_")
    if a.startswith(("revoke", "cancel", "end_", "return", "decline", "reject", "refuse", "kick")):
        return "other"
    if "justify" in a or a in ("add_war_goal", "war_goal"):
        return "justify"
    if "declare" in a:
        return "declare"
    if "faction" in a and ("leave" in a or "dismantle" in a or "disband" in a):
        return "leave_faction"
    if a == "join" or ("join" in a and "faction" in a):
        return "join"
    if "invite" in a:
        return "invite"
    if "access" in a or "docking" in a or "dock" in a.split("_") or "airbase" in a or "air_base" in a:
        return "access"
    if "volunteer" in a:
        return "volunteers"
    if "expedition" in a:
        return "expedition"
    if "lend_lease" in a and not a.startswith("request"):
        return "lend_lease"
    if "licen" in a:
        return "licence"
    return "other"


def calls(node, path="", depth=0):
    """(where, tool, args, parent) of every {"call": {"tool", "args"}} in the plan."""
    if depth > 8:
        return
    if isinstance(node, dict):
        c = node.get("call")
        if isinstance(c, dict) and isinstance(c.get("tool"), str):
            yield path or "plan", c["tool"], c.get("args") if isinstance(c.get("args"), dict) else {}, node
        for k, v in node.items():
            if k != "call":
                yield from calls(v, f"{path}.{k}" if path else k, depth + 1)
    elif isinstance(node, list):
        for i, v in enumerate(node):
            yield from calls(v, f"{path}[{i}]", depth + 1)


def _strings(node, depth=0):
    if depth > 6:
        return
    if isinstance(node, str):
        yield node
    elif isinstance(node, dict):
        for v in node.values():
            yield from _strings(v, depth + 1)
    elif isinstance(node, list):
        for v in node:
            yield from _strings(v, depth + 1)


# --------------------------------------------------------------------------- game facts (memoised)

def sub_facts(game):
    """{"equipment": {key: why}, "modules": special sub modules, "mine_modules", "techs": {tech: why},
    "projects": {project: why}}: submarines above Submarine II (a third tier of a hull line, or a hull
    without a parent newer than the second tier), AIP and coating modules, and what unlocks them."""

    def load():
        types = derive.equipment_types(game)
        raw = {}
        for f in game.files("common/units/equipment"):
            for k, v in parse_file(f):
                if k != "equipments" or not isinstance(v, list):
                    continue
                for name, body in v:
                    if name and isinstance(body, list):
                        year = get1(body, "year")
                        raw[name] = (int(year) if isinstance(year, str) and year.isdigit() else None,
                                     get1(body, "parent"), get1(body, "is_archetype") == "yes")
        subs = {k for k, (_, _, arche) in raw.items() if "submarine" in types.get(k, set()) and not arche}

        def tier(k, seen=()):
            parent = raw[k][1]
            return 1 if parent not in subs or parent in seen else 1 + tier(parent, seen + (k,))

        tiers = {k: tier(k) for k in subs}
        second = [raw[k][0] for k, t in tiers.items() if t == 2 and raw[k][0]]
        cut = min(second) if second else None
        banned = {}
        for k, t in sorted(tiers.items()):
            if t >= 3:
                banned[k] = f"{k} is a tier-{t} submarine"
            elif t == 1 and cut and raw[k][0] and raw[k][0] > cut:
                banned[k] = f"{k} ({raw[k][0]}) is newer than Submarine II ({cut})"
        special, mines = {}, set()
        for f in game.files("common/units/equipment/modules"):
            for k, v in parse_file(f):
                if k != "equipment_modules" or not isinstance(v, list):
                    continue
                for name, body in v:
                    if not name or not isinstance(body, list):
                        continue
                    cat = get1(body, "category") or ""
                    if _SPECIAL_SUB.search(name) or _SPECIAL_SUB.search(cat):
                        special[name] = f"{name} is a special submarine module"
                    stats = get1(body, "add_stats")
                    if "mine_layer" in name or "mine_layer" in cat or (
                            isinstance(stats, list) and get1(stats, "mines_planting") not in (None, "0")):
                        mines.add(name)
        unlocks = dict(banned, **special)

        def found(body, depth=0):
            hits = []
            if depth > 8 or not isinstance(body, list):
                return hits
            for k, v in body:
                if k in ("enable_equipments", "enable_equipment_modules") and isinstance(v, list):
                    hits += [unlocks[x] for x in bare(v) if x in unlocks]
                elif isinstance(v, list):
                    hits += found(v, depth + 1)
            return hits

        techs, projects, gated = {}, {}, set()
        for f in game.files("common/technologies"):
            for k, v in parse_file(f):
                if k == "technologies" and isinstance(v, list):
                    for name, body in v:
                        hits = found(body) if name and isinstance(body, list) else []
                        if hits:
                            techs[name] = "; ".join(sorted(set(hits)))
                            if get1(body, "is_special_project_tech") == "yes":
                                gated.add(name)
        for f in game.files("common/special_projects/projects"):
            for name, body in parse_file(f):
                hits = found(body) if name and isinstance(body, list) else []
                if hits:
                    projects[name] = "; ".join(sorted(set(hits)))
        return {"equipment": banned, "modules": special, "mine_modules": mines, "techs": techs,
                "projects": projects, "project_techs": gated}

    return game.cached("fair:subs", load)


def continents(game):
    def load():
        p = game.file("map/continent.txt")
        for k, v in parse_file(p) if p else []:
            if k == "continents" and isinstance(v, list):
                return bare(v)
        return []

    return game.cached("fair:continents", load)


def state_geo(game):
    """{state: {"continent": name or None, "xy": (x, y) or None}} from the land provinces (map graph)."""

    def load():
        graph = game.map_graph()
        if graph is None:
            return {}
        names = continents(game)
        provs = graph["provinces"]
        out = {}
        for sid, s in game.states().items():
            land = [provs[p] for p in s["provinces"] if p in provs and provs[p].get("type") == "land"]
            cont = Counter(r.get("continent", 0) for r in land).most_common(1)
            idx = cont[0][0] if cont else 0
            px = sum(r.get("pixels", 1) for r in land if r.get("xy")) or 0
            xy = (sum(r["xy"][0] * r.get("pixels", 1) for r in land if r.get("xy")) / px,
                  sum(r["xy"][1] * r.get("pixels", 1) for r in land if r.get("xy")) / px) if px else None
            out[sid] = {"continent": names[idx - 1] if 0 < idx <= len(names) else None, "xy": xy}
        return out

    return game.cached("fair:stategeo", load)


def state_adjacency(game, straits=False):
    """{state: set(states)} sharing a land border (straits=True: also straits)."""

    def load():
        nb = game.neighbours(straits) or {}
        graph = game.map_graph() or {"provinces": {}}
        provs, ps = graph["provinces"], game.province_state()
        out = defaultdict(set)
        for a, bs in nb.items():
            if provs.get(a, {}).get("type") != "land":
                continue
            for b in bs:
                sa, sb = ps.get(a), ps.get(b)
                if sa and sb and sa != sb and provs.get(b, {}).get("type") == "land":
                    out[sa].add(sb)
        return dict(out)

    return game.cached(f"fair:stateadj:{straits}", load)


def component(adj, seed, allowed):
    if seed not in allowed:
        return set()
    seen, q = {seed}, deque([seed])
    while q:
        a = q.popleft()
        for b in adj.get(a, ()):
            if b in allowed and b not in seen:
                seen.add(b)
                q.append(b)
    return seen


def mainland(game, continent):
    """The largest land-border component of the states of one continent."""

    def load():
        geo = state_geo(game)
        adj = state_adjacency(game, False)
        left = {s for s, g in geo.items() if g["continent"] == continent}
        best = set()
        while left:
            part = component(adj, next(iter(left)), left)
            left -= part
            if len(part) > len(best):
                best = part
        return best

    return game.cached(f"fair:mainland:{continent}", load)


# --------------------------------------------------------------------------- the checks

class Fair:
    def __init__(self, lint):
        self.lt = lint
        self.plan = lint.plan
        self.game = lint.game
        self.me = lint.tag
        self.table = lint.table
        self.start = derive.as_tuple(lint.start)
        self.stop = derive.as_tuple(lint.stop) if lint.stop else None
        fair = self.plan.get("_fair") if isinstance(self.plan.get("_fair"), dict) else {}
        exc = fair.get("exceptions") if isinstance(fair.get("exceptions"), dict) else {}
        self.exceptions = {k: str(v) for k, v in exc.items() if isinstance(k, str)}
        self.phases = {str(p.get("id")): p for p in lint.list_of("phases")}
        self.calls = list(calls(self.plan))
        self._cache = {}
        try:
            self.world = _world(self.game, lint.meta)
        except Exception:  # noqa: BLE001 - the lint must not fail on a missing history file
            self.world = None

    # ------------------------------------------------------------------ helpers
    def add(self, severity, rule, where, detail):
        msg = f"{detail}. FAIR rule: {RULES[rule]}"
        if rule in self.exceptions:
            severity, msg = "info", f"{msg} (host exception: {self.exceptions[rule]})"
        self.lt.add(severity, rule, where, msg)

    def memo(self, key, fn):
        if key not in self._cache:
            self._cache[key] = fn()
        return self._cache[key]

    def atom(self, text):
        if not isinstance(text, (str, bool)):
            return None
        a, _ = parse_atom(text, self.table)
        return a

    def requires(self, pred, test, neg=False):
        """True when every way pred can hold includes an atom with test(atom, negated)."""
        if pred is None:
            return False
        if isinstance(pred, list):
            pred = {"all": pred}
        if isinstance(pred, dict):
            if len(pred) != 1:
                return False
            k, v = next(iter(pred.items()))
            if k == "not":
                return self.requires(v, test, not neg)
            parts = v if isinstance(v, list) else []
            if k not in ("all", "any"):
                return False
            if (k == "all") != neg:  # all, or not(any)
                return any(self.requires(p, test, neg) for p in parts)
            return bool(parts) and all(self.requires(p, test, neg) for p in parts)
        a = self.atom(pred)
        return bool(a) and bool(test(a, neg))

    def earliest(self, pred, seen=frozenset()):
        """Earliest date tuple pred can hold by its date atoms (phases followed), None without a bound."""
        if isinstance(pred, list):
            pred = {"all": pred}
        if isinstance(pred, dict):
            if len(pred) != 1:
                return None
            k, v = next(iter(pred.items()))
            parts = [self.earliest(p, seen) for p in (v if isinstance(v, list) else [])]
            if k == "all":
                dated = [d for d in parts if d]
                return max(dated) if dated else None
            if k == "any":
                return min(parts) if parts and all(parts) else None
            return None
        a = self.atom(pred)
        if not a:
            return None
        if a["name"] == "date" and a["op"] in (">=", ">", "=="):
            return tuple(a["value"])
        if a["name"] == "phase" and a["arg"] not in seen and a["arg"] in self.phases:
            return self.earliest(self.phases[a["arg"]].get("enter"), seen | {a["arg"]})
        return None

    def day(self, pred):
        e = self.earliest(pred)
        return max(self.start, e) if e else self.start

    def at_war_req(self, pred, tags=None):
        return self.requires(pred, lambda a, neg: not neg and a["name"] in ("at_war_with", "at_war")
                             and (tags is None or a["arg"] in tags))

    def capitulated_req(self, pred, tag):
        return self.requires(pred, lambda a, neg: not neg and a["name"] in ("capitulated", "not_exists")
                             and a["arg"] == tag)

    def after_danzig(self, pred):
        if self.day(pred) >= DANZIG:
            return True

        def test(a, neg):
            if neg:
                return False
            if a["name"] in ("capitulated", "not_exists"):
                return a["arg"] == "POL"
            return a["name"] in ("at_war_with", "at_war") and a["arg"] == ("POL" if self.me == "GER" else "GER")

        return self.requires(pred, test)

    def owners(self):
        return self.memo("owners", lambda: self.game.state_owners(self.lt.ctx, self.lt.start))

    def controls_req(self, pred, sid):
        """pred needs the plan's tag to hold state sid (control atoms, or its start owner capitulated)."""
        provs = set(self.game.states().get(sid, {}).get("provinces", []))
        owner = self.owners().get(sid)

        def test(a, neg):
            if neg:
                return False
            n, arg = a["name"], a["arg"]
            if n in ("controls_state", "state_fully_controlled"):
                return arg is not None and int(arg) == sid
            if n in ("owner", "controller", "state_controller"):
                return int(arg) == sid and a["op"] == "==" and a["value"] == self.me
            if n == "province_controlled":
                return int(arg) in provs
            if n == "province_controller":
                return int(arg) in provs and a["op"] == "==" and a["value"] == self.me
            if n in ("capitulated", "not_exists"):
                return owner is not None and arg == owner
            return False

        return self.requires(pred, test)

    def invaded_req(self, pred, states):
        """States of `states` that pred needs held by someone other than their start owner."""
        ps = self.game.province_state()
        out = set()
        for sid in states:
            owner = self.owners().get(sid)

            def test(a, neg, sid=sid, owner=owner):
                n = a["name"]
                if n in ("owner", "controller", "state_controller"):
                    hit = int(a["arg"]) == sid
                elif n == "province_controller":
                    hit = ps.get(int(a["arg"])) == sid
                else:
                    return False
                differs = (a["op"] == "!=" and a["value"] == owner) or (a["op"] == "==" and a["value"] != owner)
                return hit and (differs != neg)

            if self.requires(pred, test):
                out.add(sid)
        return out

    def named(self, names, owner=None, pattern=None):
        out = set()
        for sid, s in self.game.states().items():
            ns = s.get("names") or set()
            if (ns & set(names) or (pattern and any(re.search(pattern, n, re.I) for n in ns))) and (
                    owner is None or self.owners().get(sid) == owner):
                out.add(sid)
        return out

    def capital(self, tag):
        return self.memo(f"cap:{tag}", lambda: derive.capital_state(self.game, tag, self.lt.ctx, self.lt.start))

    def home(self, tag, straits=False):
        """States the tag owns at the start, connected to its capital over own land (or straits)."""

        def load():
            own = {s for s, o in self.owners().items() if o == tag}
            return component(state_adjacency(self.game, straits), self.capital(tag), own)

        return self.memo(f"home:{tag}:{straits}", load)

    def start_wars(self):
        return self.memo("wars", lambda: set(self.world.at_war_with(self.me)) if self.world else set())

    def oob(self):
        def load():
            try:
                return self.world.oob(self.me) if self.world else None
            except (OSError, ValueError, KeyError):
                return None
        return self.memo("oob", load) or {"templates": {}, "divisions": [], "fleets": []}

    def diplomacy(self):
        """[(where, kind, target, when, action)] of diplomacy[] and hoi4_diplomacy_act / faction calls."""

        def load():
            out = []
            for i, d in enumerate(self.lt.list_of("diplomacy")):
                a = str(d.get("action") or "")
                out.append((f"diplomacy[{i}] {d.get('id', a)}", kind_of(a), d.get("target"), d.get("when"), a))
            for where, tool, args, parent in self.calls:
                if tool == "hoi4_diplomacy_act":
                    a = str(args.get("action") or "")
                    out.append((where, kind_of(a), args.get("country"), parent.get("when"), a))
                elif tool == "hoi4_faction_leave":
                    out.append((where, "leave_faction", None, parent.get("when"), "leave_faction"))
            return out

        return self.memo("dipl", load)

    def joins(self):
        """[(where, faction member or ANY, when)] of the faction joins the plan makes."""
        out = []
        f = self.plan.get("faction") if isinstance(self.plan.get("faction"), dict) else {}
        if isinstance(f.get("join"), str):
            out.append(("faction.join", f["join"], f.get("when")))
        out += [(w, t, when) for w, kind, t, when, _ in self.diplomacy() if kind == "join"]
        return out

    def war_planned(self):
        return bool(self.start_wars() or self.lt.enemies() or self.lt.list_of("invasions")
                    or any(k == "declare" for _, k, _, _, _ in self.diplomacy()))

    def training_lines(self):
        tr = self.plan.get("training")
        lines = tr if isinstance(tr, list) else [tr]
        rules = self.plan.get("training_rules") if isinstance(self.plan.get("training_rules"), dict) else {}
        out = []
        for i, line in enumerate(lines):
            if not isinstance(line, dict) or not isinstance(line.get("template"), str):
                continue
            cap = line.get("cap")
            if cap is None and rules.get("cap_template") == line["template"]:
                cap = rules.get("cap")
            guard = bool(line.get("only_if_stockpile") or rules.get("only_if_stockpile"))
            where = f"training[{i}]" if isinstance(tr, list) else "training"
            out.append((where, line["template"], cap if isinstance(cap, int) else None, guard, line))
        return out

    # ------------------------------------------------------------------ templates
    def unit_class(self):
        units = derive.unit_types(self.game)

        def cls(key):
            u = units.get(key)
            if u is None or u["support"]:
                return set()
            out = set()
            if u["types"] == {"infantry"} or "category_cavalry" in u["categories"]:
                out.add("leg")
            if "armor" in u["types"]:
                out.add("armour")
            if "category_paratroopers" in u["categories"]:
                out.add("para")
            if "category_special_forces" in u["categories"]:
                out.add("sf")
            return out

        return units, cls

    def layouts(self):
        """({name: (where, battalions)} of plan templates, {name: battalions} of start templates)."""
        plan = {}
        for i, t in enumerate(self.lt.list_of("templates")):
            keys = [u for c in template_columns(t.get("regiments")) or [] for u in c]
            where = f"templates[{i}] {t.get('id', t.get('name'))}"
            for n in (t.get("name"), t.get("id")):
                if isinstance(n, str):
                    plan.setdefault(n, (where, keys, t))
        start = {}
        for name, t in (self.oob().get("templates") or {}).items():
            if isinstance(name, str):
                start[name] = [u for u, n in (t.get("regiments") or {}).items() for _ in range(int(n or 0))]
        return plan, start

    def template_rules(self, where, name, keys, at_war, trained=False):
        units, cls = self.unit_class()
        if not keys or any(k not in units for k in keys):
            return
        width = sum(units[k]["width"] for k in keys)
        kinds = Counter(c for k in keys for c in cls(k))
        verb = "trains new divisions of" if trained else "makes"
        if at_war and width < MIN_WIDTH:
            self.add("error", "fair.width", where,
                     f"{verb} template {name!r} at {width:g} combat width while the plan has a war")
        if kinds["leg"] and kinds["armour"]:
            self.add("error", "fair.space_marines", where,
                     f"template {name!r} mixes {kinds['leg']} infantry or cavalry battalions with {kinds['armour']} "
                     f"tank battalions")
        if kinds["para"] and width < PARA_WIDTH:
            self.add("error", "fair.paratroopers", where,
                     f"paratrooper template {name!r} is {width:g} wide (at least {PARA_WIDTH})")
        return kinds

    def templates(self):
        plan, start = self.layouts()
        at_war = self.war_planned()
        units, cls = self.unit_class()
        divisions = Counter(d.get("template") for d in self.oob().get("divisions") or [])
        kinds_of = {}
        for name, (where, keys, t) in plan.items():
            if name != t.get("name") and t.get("name") in plan:
                continue  # one entry per template (by name; the id only when it has no name)
            kinds_of[name] = self.template_rules(where, name, keys, at_war) or Counter()
            base = t.get("base")
            sf_new = kinds_of[name]["sf"]
            sf_old = sum(1 for k in start.get(base, []) if "sf" in cls(k)) if isinstance(base, str) else 0
            if base and t.get("upgrade_fielded") and sf_new > sf_old:
                self.add("error", "fair.special_forces", where,
                         f"edits {base!r} to {sf_new} special forces battalions and reorganises its fielded "
                         f"divisions (upgrade_fielded): fielded divisions converted into special forces skip the cap")
        para_lines = sf_trained = 0
        para_uncapped = []
        sf_planned = sum(divisions[n] * sum(1 for k in ks if "sf" in cls(k)) for n, ks in start.items())
        other = sum(divisions[n] * sum(1 for k in ks if k in units and "sf" not in cls(k)) for n, ks in start.items())
        for where, name, cap, guard, line in self.training_lines():
            if name in plan:
                keys = plan[name][1]
                kinds = kinds_of.get(name) or Counter(c for k in keys for c in cls(k))
            elif name in start:
                keys = start[name]
                kinds = self.template_rules(where, name, keys, at_war, trained=True) or Counter()
            else:
                keys, kinds = [], Counter()
            if not guard:
                self.add("warning", "fair.ghost_divisions", where,
                         f"trains {name!r} without a stockpile guard (only_if_stockpile here or in training_rules)")
            if kinds["para"]:
                if cap is None:
                    para_uncapped.append(name)
                    self.add("warning", "fair.paratroopers", where,
                             f"trains paratrooper template {name!r} without a cap; set cap so the total stays at "
                             f"{PARA_MAX} or fewer")
                else:
                    para_lines += cap
            if kinds["sf"] and cap is not None:
                sf_trained += cap * kinds["sf"]
        para_start = sum(n for t, n in divisions.items() if t in start and any("para" in cls(k) for k in start[t]))
        if para_start + para_lines > PARA_MAX:
            self.add("error", "fair.paratroopers", "training",
                     f"{para_start} paratrooper divisions at the start plus training caps {para_lines} make "
                     f"{para_start + para_lines} (at most {PARA_MAX})")
        cap_min = float(derive.define(self.game, "SPECIAL_FORCES_CAP_MIN", 24) or 24)
        cap_base = float(derive.define(self.game, "SPECIAL_FORCES_CAP_BASE", 0.05) or 0.05)
        cap = max(cap_min, cap_base * other)
        sf_planned += sf_trained
        if sf_trained and sf_planned > cap:
            self.add("warning", "fair.special_forces", "training",
                     f"{sf_planned} special forces battalions planned (start plus training caps) against a cap of "
                     f"about {cap:g} ({cap_min:g} or {cap_base:g} x {other} other battalions, before modifiers)")

    # ------------------------------------------------------------------ navy, air
    def fleet_missions(self):
        """[(where, fleet, mission, when)] of navy.fleets[].missions and hoi4_fleet_mission calls."""
        nv = self.plan.get("navy") if isinstance(self.plan.get("navy"), dict) else {}
        out = []
        for j, f in enumerate(nv.get("fleets") or []):
            if not isinstance(f, dict):
                continue
            for k, m in enumerate(f.get("missions") or []):
                if isinstance(m, dict):
                    out.append((f"navy.fleets[{j}].missions[{k}]", f.get("fleet"), str(m.get("mission") or ""),
                                m.get("when"), m.get("task_force")))
        for where, tool, args, parent in self.calls:
            if tool == "hoi4_fleet_mission":
                out.append((where, None, str(args.get("mission") or ""), parent.get("when"), args.get("task_force")))
        return out

    def air_entries(self):
        """[(where, entry, [missions])] of air_plan and hoi4_air_mission calls."""
        out = []
        for i, e in enumerate(self.lt.list_of("air_plan")):
            ms = [str(e[k]) for k in ("mission", "then_mission", "fallback_mission") if e.get(k)]
            out.append((f"air_plan[{i}]", e, ms))
        for where, tool, args, parent in self.calls:
            if tool == "hoi4_air_mission":
                out.append((where, args, [str(args.get("mission") or "")]))
        return out

    def navy(self):
        types = derive.equipment_types(self.game)
        fleets = {str(f.get("name") or "").strip().lower(): f for f in self.oob().get("fleets") or []}
        peace_ok = bool(self.start_wars())
        reported = set()
        for where, fleet, mission, when, tf in self.fleet_missions():
            m = mission.lower()
            if m == "mine_laying":
                self.add("error", "fair.naval_mines", where, "mine_laying mission")
            if m == "convoy_raiding" and not peace_ok and not self.at_war_req(when):
                self.add("error", "fair.blockade", where,
                         "convoy_raiding can run in peacetime: its `when` needs an at_war_with atom")
            f = fleets.get(str(fleet or "").strip().lower())
            if f and m != "hold" and not tf and fleet not in reported:
                ships = f.get("ships") or {}
                n = sum(ships.values())
                if ships and all("submarine" in types.get(k, set()) for k in ships) and n < SUB_FLEET_MIN:
                    reported.add(fleet)
                    self.add("error", "fair.submarines", where,
                             f"fleet {fleet!r} has {n} submarines (at least {SUB_FLEET_MIN} per submarine fleet)")
        for where, entry, missions in self.air_entries():
            ms = {m.lower() for m in missions}
            if ms & {"naval_mines", "naval_mines_planting", "mines_planting"}:
                self.add("error", "fair.naval_mines", where, "naval_mines air mission")
            if any("strategic" in m or "bomb" in m for m in ms) and any(
                    re.search(r"refiner", s, re.I) for s in _strings(entry)):
                self.add("error", "fair.bombing", where, "strategic bombing aimed at refineries")

    def submarines(self):
        sf = sub_facts(self.game)
        plan = self.plan
        for i, e in enumerate(self.lt.list_of("research_path")):
            for t in e.get("techs") or []:
                if t in sf["techs"]:
                    self.add("error", "fair.submarines", f"research_path[{i}]", f"researches {t} ({sf['techs'][t]})")
        for i, t in enumerate(plan.get("research_priority") or []):
            if t in sf["techs"]:
                self.add("error", "fair.submarines", f"research_priority[{i}]", f"researches {t} ({sf['techs'][t]})")
        if plan.get("research_fallback") not in (False, "priority_only"):
            prefixes = [p for p in plan.get("research_fallback_prefixes") or [] if isinstance(p, str)]
            cats = [c for c in plan.get("research_fallback_categories") or [] if isinstance(c, str)]
            ex = plan.get("research_fallback_exclude")
            exclude = [x for x in ex if isinstance(x, str)] if isinstance(ex, list) else [
                "ship", "hull", "naval", "carrier", "submarine", "destroyer", "cruiser", "battleship", "torpedo",
                "convoy"]
            open_ = sorted(t for t in set(sf["techs"]) - sf["project_techs"] if not any(x in t for x in exclude) and (
                (not prefixes and not cats) or any(t.startswith(p) for p in prefixes) or any(c in t for c in cats)))
            if open_:
                self.add("error", "fair.submarines", "research_fallback",
                         f"the research fallback can take {', '.join(open_)}: add them (or `submarine`) to "
                         f"research_fallback_exclude")
        sp = plan.get("special_projects") if isinstance(plan.get("special_projects"), dict) else {}
        for i, p in enumerate(sp.get("priority") or []):
            if p in sf["projects"]:
                self.add("error", "fair.submarines", f"special_projects.priority[{i}]",
                         f"special project {p} ({sf['projects'][p]})")
        for i, x in enumerate(self.lt.list_of("production")):
            eq = x.get("equipment")
            if eq in sf["equipment"] and not x.get("remove"):
                self.add("error", "fair.submarines", f"production[{i}]", f"produces {sf['equipment'][eq]}")
        for i, d in enumerate(self.lt.list_of("designs")):
            where = f"designs[{i}] {d.get('id')}"
            if d.get("base") in sf["equipment"]:
                self.add("error", "fair.submarines", where, f"designs on {sf['equipment'][d['base']]}")
            mods = d.get("modules") if isinstance(d.get("modules"), dict) else {}
            for slot, mod in sorted(mods.items()):
                if mod in sf["modules"]:
                    self.add("error", "fair.submarines", where, f"slot {slot}: {sf['modules'][mod]}")
                elif mod in sf["mine_modules"]:
                    self.add("warning", "fair.naval_mines", where, f"slot {slot}: mine-laying module {mod}")
        banned = set(sf["equipment"]) | set(sf["techs"]) | set(sf["projects"]) | set(sf["modules"])
        for where, tool, args, _ in self.calls:
            hits = sorted({s for s in _strings(args) if s in banned})
            if hits:
                self.add("error", "fair.submarines", where, f"{tool} names {', '.join(hits)}")

    # ------------------------------------------------------------------ diplomacy and nation rules
    def diplomacy_rules(self):
        me = self.me
        leads = isinstance((self.plan.get("faction") or {}).get("create"), dict) if isinstance(
            self.plan.get("faction"), dict) else False
        for where, kind, target, when, action in self.diplomacy():
            if kind == "access":
                if (me in EURO_AXIS and target in JAPAN_SIDE) or (me in JAPAN_SIDE and target in EURO_AXIS):
                    self.add("warning", "fair.access", where, f"{action} with {target}: allowed between the "
                             f"European Axis and Japan only once the Iran corridor is secured")
                elif (me in ALLIES and target == "SOV") or (me == "SOV" and target in ALLIES):
                    self.add("warning", "fair.access", where, f"{action} with {target}: allowed between the Allies "
                             f"and the Soviet Union only once all of mainland Asia is secured")
                else:
                    self.add("error", "fair.access", where, f"{action} with {target}")
            elif kind == "leave_faction":
                self.add("error" if leads else "warning", "fair.factions", where,
                         "leaves a faction" + (" the plan creates (dismantles it)" if leads else
                                               "; as its leader this dismantles it"))
            elif kind == "invite" and ((me in JAPAN_SIDE and target in EURO_AXIS)
                                       or (me in EURO_AXIS and target in JAPAN_SIDE)):
                self.add("error", "fair.factions", where, f"invites {target} into {me}'s faction")
            elif kind == "justify":
                self.justify(where, target, when, action)
            elif kind == "declare":
                self.declare(where, target, when)
            elif kind == "volunteers":
                if target in SPAIN or (me == "USA" and target in ALLIES | {"SOV"}):
                    continue
                if (me == "USA" and target == "CHI") or (me == "SOV" and target == "CHI"):
                    limit = "6 divisions and 1 air wing" if me == "USA" else "6 divisions, no tanks or air"
                    self.add("warning", "fair.volunteers", where, f"volunteers to China: at most {limit}")
                else:
                    self.add("error", "fair.volunteers", where, f"volunteers to {target}")
            elif kind == "expedition" and me in OUTSIDE_ALLIES:
                cap = self.capital(target) if isinstance(target, str) else None
                if cap in mainland(self.game, "europe") and not self.capitulated_req(when, "FRA"):
                    self.add("error", "fair.invasions", where,
                             f"expeditionary force to {target} in mainland Europe before France capitulates "
                             f"(`when` needs capitulated:FRA)")
            elif kind == "lend_lease" and me == "FRA" and target not in SPAIN:
                self.add("error", "fair.france", where, f"lend-lease to {target} before capitulation")
        for where, target, when in self.joins():
            self.join(where, target, when)
        self.italy_joins()
        self.usa_trade()

    def justify(self, where, target, when, action):
        me = self.me
        if target == "SOV" and me in {"GER"} | JAPAN_SIDE:
            self.add("error", "fair.timing", where,
                     f"{action} on the Soviet Union by hand: {me} must use the focus war goal")
            return
        if me in BALKAN_AXIS and target in BALKANS:
            return
        if me == "SOV" and target == "PER":
            molotov = self.requires(when, lambda a, neg: not neg and a["name"] in (
                "focus_done", "focus_completed") and "molotov" in str(a["arg"]).lower())
            if self.day(when) < MOLOTOV and not molotov:
                self.add("error", "fair.justify", where, "justification on Iran before the Molotov-Ribbentrop "
                         "Pact (`when` needs its focus or date >= 1939.08.23)")
            return
        if not self.after_danzig(when):
            self.add("error", "fair.justify", where,
                     f"{action} on {target} can run before the Danzig crisis (`when` needs date >= 1939.09.01, "
                     f"{'at_war_with:POL' if me == 'GER' else 'at_war_with:GER'} or capitulated:POL)")

    def declare(self, where, target, when):
        me, d = self.me, self.day(when)
        if me == "GER" and target == "SOV":
            if d < (1941, 1, 1):
                self.add("error", "fair.timing", where, f"war on the Soviet Union can start {derive.fmt(d)}: "
                         f"Germany may attack it from 1941 (`when` needs date >= 1941.01.01)")
            elif d > (1942, 12, 31):
                self.add("warning", "fair.timing", where, "Barbarossa is due in 1942 at the latest")
        elif me in JAPAN_SIDE and target in ALLIES and d < (1941, 1, 1):
            self.add("error", "fair.timing", where, f"war on {target} can start {derive.fmt(d)}: Japan, Manchukuo "
                     f"and Siam may start a war with the Allies from 1941")
        elif me in JAPAN_SIDE and target == "SOV":
            self.add("warning", "fair.timing", where, "war on the Soviet Union only after all of mainland Asia is "
                     "secured, with the focus war goal")
        elif me == "SOV" and target in AXIS and d < (1942, 1, 1):
            homes = self.home("ENG") | self.home("USA")
            if not self.invaded_req(when, homes):
                self.add("error", "fair.timing", where, f"war on {target} can start {derive.fmt(d)}: the Soviet Union "
                         f"may attack the Axis from 1942, or once the Axis has invaded the mainland USA or UK")
        elif me == "USA" and target in AXIS and not self.usa_may_join(when):
            self.usa_error(where, f"war on {target}")

    def usa_may_join(self, when):
        if self.day(when) >= (1942, 1, 1) or self.at_war_req(when, {"JAP"}):
            return True
        geo = state_geo(self.game)
        americas = {s for s, g in geo.items() if g["continent"] in ("north_america", "south_america")}
        cities = self.named({"Moscow", "Leningrad", "Stalingrad"}, owner="SOV")
        return bool(self.invaded_req(when, self.home("ENG") | americas)) or len(self.invaded_req(when, cities)) >= 2

    def usa_error(self, where, what):
        self.add("error", "fair.timing", where,
                 f"{what}: the USA joins the Allies only when Japan attacks, from 1942, when the UK or land in the "
                 f"Americas is invaded, or when the Soviet Union loses 2 of Moscow, Leningrad and Stalingrad "
                 f"(`when` needs one of these)")

    def join(self, where, target, when):
        me = self.me
        axis, allies = target in EURO_AXIS or target == "ANY", target in ALLIES or target == "ANY"
        if (me in JAPAN_SIDE and target in EURO_AXIS) or (me in EURO_AXIS and target in JAPAN_SIDE):
            self.add("error", "fair.factions", where, f"{me} joins {target}'s faction")
        elif me in SPAIN and axis and not self.capitulated_req(when, "FRA"):
            self.add("error" if target != "ANY" else "warning", "fair.timing", where,
                     "Spain joins the Axis only after France falls (`when` needs capitulated:FRA)")
        elif me == "FIN" and axis and not self.requires(when, lambda a, neg: (
                (neg and a["name"] in ("at_war_with", "at_war")) or (not neg and a["name"] == "not_at_war"))
                and a["arg"] == "SOV"):
            self.add("warning", "fair.timing", where,
                     "Finland may not join the Axis during the Winter War (`when` needs not_at_war:SOV)")
        elif me in LOW_COUNTRIES and allies and not self.at_war_req(when, {"GER"}):
            self.add("error", "fair.timing", where, f"{me} joins the Allies only when Germany declares war on it "
                     f"(`when` needs at_war_with:GER)")
        elif me == "USA" and allies and not self.usa_may_join(when):
            self.usa_error(where, "joining the Allies")

    def italy_joins(self):
        if self.me != "ITA":
            return
        entries = [(w, self.day(when)) for w, k, t, when, _ in self.diplomacy() if k == "declare" and t in ALLIES]
        entries += [(w, self.day(when)) for w, t, when in self.joins() if t in EURO_AXIS or t == "ANY"]
        due = (1941, 1, 1)
        if entries and min(d for _, d in entries) > due:
            self.add("error", "fair.timing", entries[0][0],
                     f"Italy's first entry into the war is {derive.fmt(min(d for _, d in entries))}: Italy joins "
                     f"the war by 1941.01.01")
        elif not entries and self.stop and self.stop > due and not (self.start_wars() & ALLIES):
            self.add("warning", "fair.timing", "diplomacy",
                     "the plan runs past 1941.01.01 and Italy never joins the war: it must join by then")

    def usa_trade(self):
        if self.me != "USA":
            return
        tr = self.plan.get("trade")
        deals = tr.get("deals") if isinstance(tr, dict) else tr
        names = {"ENG": {"ENG", "United Kingdom"}, "FRA": {"FRA", "France"}}
        for i, d in enumerate(deals if isinstance(deals, list) else []):
            if not isinstance(d, dict):
                continue
            partner = d.get("from") or d.get("partner")
            if any(partner in n for n in names.values()) and not self.after_danzig(d.get("when")):
                self.add("error", "fair.timing", f"trade[{i}]", f"buys from {partner} before Danzig: the USA may not "
                         f"boost the UK or France with civilian factories before the Danzig crisis")

    # ------------------------------------------------------------------ invasions and France
    def landings(self):
        """[(where, state, when, kind)] of invasions, sea transfer routes and hoi4_naval_invasion calls."""
        out = [(f"invasions[{i}] {e.get('id')}", e.get("target_state"), e.get("when"), "invasion")
               for i, e in enumerate(self.lt.list_of("invasions")) if isinstance(e.get("target_state"), int)]
        st = self.plan.get("sea_transfer") if isinstance(self.plan.get("sea_transfer"), dict) else {}
        out += [(f"sea_transfer.routes[{i}] {r.get('id')}", r.get("to_state"), r.get("when"), "transfer")
                for i, r in enumerate(st.get("routes") or [])
                if isinstance(r, dict) and isinstance(r.get("to_state"), int)]
        for where, tool, args, parent in self.calls:
            if tool == "hoi4_naval_invasion":
                for key in ("target_state", "state_id", "to_state"):
                    if isinstance(args.get(key), int):
                        out.append((where, args[key], parent.get("when"), "invasion"))
                        break
        return out

    def need(self, where, sid, when, keys, what, enough=None):
        held = [s for s in sorted(keys) if self.controls_req(when, s)]
        if len(held) < (enough or len(keys)):
            names = ", ".join(sorted(next(iter(self.game.states()[s]["names"]), str(s)) for s in keys))
            self.add("error", "fair.invasions", where,
                     f"landing in state {sid} {what}: `when` must hold {names}"
                     + (f" ({enough} of them)" if enough and enough < len(keys) else ""))

    def invasions(self):
        me = self.me
        landings = self.landings()
        if not landings or self.game.map_graph() is None:
            return
        own = self.owners()
        europe = mainland(self.game, "europe")
        for where, sid, when, kind in landings:
            if kind == "invasion" and me in EURO_AXIS and sid in self.home("ENG"):
                keys = self.named({"Gibraltar", "Malta", "Suez"}, owner="ENG")
                enough = 1 if self.at_war_req(when, {"SOV"}) else 2
                if keys:
                    self.need(where, sid, when, keys, "is Sea Lion" + (" (Barbarossa running)" if enough == 1 else ""),
                              enough)
            if kind == "invasion" and me in JAPAN_SIDE:
                png = self.named(set(), owner="AST", pattern=r"papua|new guinea")
                if own.get(sid) in ("AST", "NZL") and sid not in png and png:
                    self.need(where, sid, when, png, "invades Australia or New Zealand")
                america = mainland(self.game, "north_america")
                alaska = self.named(set(), pattern=r"alaska")
                if sid in america and sid not in alaska:
                    keys = self.named({"Hawaii"}) | {s for s, o in own.items() if o == "PHI"}
                    if keys:
                        self.need(where, sid, when, keys, "invades mainland North America")
            if kind == "invasion" and me in ALLIES:
                if sid in self.home("JAP", straits=True):
                    keys = (self.named({"Iwo Jima", "Okinawa"}, owner="JAP")
                            | self.named({"Taiwan", "Formosa"}, owner="JAP"))
                    if keys:
                        self.need(where, sid, when, keys, "attacks mainland Japan")
                if sid in self.home("ITA") and not self.capitulated_req(when, "ITA"):
                    keys = self.north_africa()
                    if keys:
                        self.need(where, sid, when, keys, "invades the core Italian mainland before North Africa is "
                                  "secured (or capitulated:ITA)")
            if me in OUTSIDE_ALLIES and sid in europe and own.get(sid) != me and not self.capitulated_req(when, "FRA"):
                self.add("error", "fair.invasions", where, f"troops into mainland Europe (state {sid}) before France "
                         f"capitulates (`when` needs capitulated:FRA)")
            if kind == "invasion" and me == "SOV" and own.get(sid) == "FIN":
                self.add("error", "fair.invasions", where, "naval invasion of Finland: the Soviet Union may not "
                         "naval invade in the Winter War")
            if me == "FRA" and sid not in europe:
                self.add("error", "fair.france", where, f"French troops sent to state {sid} outside mainland Europe")

    def north_africa(self):
        """Italy's start states in Africa nearest to Rome (one land component)."""
        geo = state_geo(self.game)
        own = {s for s, o in self.owners().items() if o == "ITA" and geo.get(s, {}).get("continent") == "africa"}
        cap = geo.get(self.capital("ITA"), {}).get("xy")
        if not own or not cap:
            return set()
        adj = state_adjacency(self.game, False)

        def dist(s):
            xy = geo[s]["xy"] or (1e9, 1e9)
            return (xy[0] - cap[0]) ** 2 + (xy[1] - cap[1]) ** 2

        return component(adj, min(own, key=dist), own)

    def france(self):
        if self.me != "FRA" or self.game.map_graph() is None:
            return
        geo, own = state_geo(self.game), self.owners()
        europe = mainland(self.game, "europe")
        bases = derive.air_base_buildings(self.game) or set()
        for where, building, sid in self.lt.queued():
            b = str(building or "")
            if (b in bases or "air_base" in b or "naval_base" in b) and own.get(sid) == "FRA" \
                    and geo.get(sid, {}).get("continent") == "europe":
                self.add("error", "fair.france", where, f"builds {b} in state {sid} in France")
        for where, name, cap, guard, line in self.training_lines():
            if isinstance(line.get("deploy_state"), int) and line["deploy_state"] not in europe:
                self.add("error", "fair.france", where,
                         f"deploys new divisions in state {line['deploy_state']} outside mainland Europe")
        for i, e in enumerate(self.lt.list_of("army_layout")):
            if isinstance(e.get("state"), int) and e["state"] not in europe:
                self.add("warning", "fair.france", f"army_layout[{i}] {e.get('name_hint')}",
                         f"keeps an army in state {e['state']} outside mainland Europe")

    # ------------------------------------------------------------------ deletion, licences
    def deletion(self):
        seen = set()
        for where, tool, args, _ in self.calls:
            text = " ".join([tool] + list(_strings(args)))
            if _DELETE_VERB.search(text) and _DELETE_WHAT.search(text) and tool not in (
                    "hoi4_template_delete", "hoi4_fleet_delete", "hoi4_division_disband", "hoi4_air_wing_disband"):
                seen.add(where)
                dock = "dockyard" in text.lower() and not re.search(r"factor|industrial|stockpile|facilit|research",
                                                                    text.replace("dockyard", ""), re.I)
                self.add("warning" if dock else "error", "fair.deletion", where, f"{tool} deletes {text[:80]}")

        def walk(node, path, depth=0):
            if depth > 6:
                return
            if isinstance(node, dict):
                if path not in seen:
                    for key in ("action", "building", "mission", "decision"):
                        v = node.get(key)
                        if isinstance(v, str) and _DELETE_VERB.search(v) and _DELETE_WHAT.search(v):
                            dock = "dockyard" in v.lower()
                            self.add("warning" if dock else "error", "fair.deletion", path, f"{key} {v}")
                for k, v in node.items():
                    if k not in ("why", "note", "rule", "_comment", "call"):
                        walk(v, f"{path}.{k}" if path else k, depth + 1)
            elif isinstance(node, list):
                for i, v in enumerate(node):
                    walk(v, f"{path}[{i}]", depth + 1)

        walk(self.plan, "")

    def licence(self):
        licensed = any(k == "licence" for _, k, _, _, _ in self.diplomacy())
        if not licensed:
            return
        have = start_tech_set(self.game, self.lt.meta) | set(self.lt.planned_techs())
        unlocks = derive.equipment_unlocks(self.game)
        known = derive.equipment_types(self.game)
        for i, d in enumerate(self.lt.list_of("designs")):
            base = d.get("base")
            if (d.get("kind") or d.get("type")) != "plane" or not isinstance(base, str) or base not in known:
                continue
            techs = unlocks.get(base, set())
            if techs and not techs & have:
                self.add("error", "fair.licence", f"designs[{i}] {d.get('id')}",
                         f"plane design on {base}, which the plan only has by licence")

    # ------------------------------------------------------------------
    def run(self):
        for step in (self.templates, self.navy, self.submarines, self.diplomacy_rules, self.invasions, self.france,
                     self.deletion, self.licence):
            step()
        self.lt.facts["fair"] = {"mode": "fair", "at_war_at_start": sorted(self.start_wars()),
                                 "war_planned": self.war_planned(), "exceptions": sorted(self.exceptions)}


def check(lint):
    """Add the fair.* findings to a lint_plan.Lint (nothing for a META plan)."""
    mode = mode_of(lint.plan)
    if mode != "fair":
        lint.facts["fair"] = {"mode": mode}
        return lint
    Fair(lint).run()
    return lint
