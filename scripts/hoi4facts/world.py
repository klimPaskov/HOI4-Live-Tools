"""Country-level facts at a start date, with sources: the shared core of the dossier and the facts CLI.

World(game, tag, date, dlcs, mode) evaluates the effective content (install plus mods, through
game.Game and content.Content) for one country at one bookmark:

    country(tag)        history at the start: capital, slots, stability, war support, politics,
                        ideas, OOB names, variables, flags, wars, guarantees, characters, doctrines
    states()            every state at the start: owner, controller, cores, claims, category and
                        static slots, buildings (state and province level), VPs, manpower, resources
    ideas(), laws(tag)  common/ideas and common/idea_tags (slots, costs, modifiers, conditions)
    characters(tag)     common/characters roles (DLC `instance` blocks evaluated)
    oob(tag)            history/units: templates, divisions with location and state, air wings,
                        fleets, production lines from instant_effect
    decisions(tag)      common/decisions filtered by static `allowed`, with timers and the state /
                        province control lists of `available`
    vp(tag)             victory points, total and the surrender threshold with its modifiers
    terrains()          common/terrain: combat_width, combat_support_width, movement, attrition, attack
    sub_units()         common/units: group, template row (battalion, divisional / regimental support, hq),
                        width, manpower, equipment need, special forces, enabling techs
    experience(tag)     army/air/navy experience at the start, the cap, experience_gain spirits
    tech_level(tag)     anti-tank / armour / anti-air / air classes reached at the start (private: meta mode only)
    occupation_start(tag)  resistance and compliance settings of the tag's states at the start
    occupation_laws(), resistance_modifiers(), resistance_activities()
    adjacency_rules(), strategic_regions() (with weather periods), weather_settings(), power_balances()
    laws(tag) rows carry `gates` (gates(): numeric and named conditions of `available`)
  wave 2 (F-12 .. F-21; public rules or own-country facts):
    designs(tag, kind)  chassis / hulls / airframes and modules the start techs unlock, XP costs, start variants
    mio(tag)            military-industrial organisations (include resolved), trait trees, policies
    projects(tag)       special projects, facilities at start, scientists, scientist traits
    diplomacy(tag)      world tension at start, ideology rules and tension gates, war goal types and costs
    faction_content()   faction templates, rules, goals, manifests, member upgrades (scenario.factions per tag)
    peace(tag, enemies) peace-conference cost modifiers naming the tag / enemies / nobody, peace defines
    command(tag)        military spirits (XP cost), leader abilities, assignable traits, command-power defines
    agency(tag)         intelligence agency upgrades, operations, phases, agency defines
    energy(tag)         energy resource per own state, factories, energy buildings, the estimate from the defines
    static(block, tag)  static trigger value with scripted triggers expanded; gate_text() its text otherwise

A fact is {"v": value, "src": "common/decisions/ETH.txt:3", "mod": None}; derived numbers carry
"d": "<method>". Enemy facts carry "fair": public | aggregate | private (design 2.3): the FAIR
profile drops private ones. Nothing here is country specific.

Stdlib only, Python 3.9+.
"""
import math
import os
import re
from collections import defaultdict

from . import derive
from .defines import Defines
from .script import (Block, Ctx, _sub, bare, date_tuple, get, get1, history_walk, line_of, op_of, parse_file, render,
                     run_effects, trigger)

FAIR_LEVELS = ("public", "aggregate", "private")
XP_EFFECTS = {"army_experience": "army", "air_experience": "air", "navy_experience": "navy"}
XP_MODIFIERS = {"experience_gain_army": "army", "experience_gain_air": "air", "experience_gain_navy": "navy"}
OCCUPATION_KEYS = {"start_resistance": "resistance_active", "set_resistance": "resistance",
                   "set_compliance": "compliance", "set_garrison_strength": "garrison"}
# sub-unit fields reported by World.sub_units (the rest of a sub-unit block is presentation or AI)
UNIT_STATS = ("max_strength", "max_organisation", "default_morale", "manpower", "training_time", "suppression",
              "weight", "supply_consumption", "maximum_speed", "recon", "entrenchment", "reliability_factor",
              "soft_attack", "hard_attack", "defense", "breakthrough", "air_attack", "ap_attack", "armor_value",
              "hardness", "initiative", "casualty_trickleback", "experience_loss_factor", "equipment_capture_factor")
GATE_KEYS_TEXT = {"has_war_support": "war support", "has_stability": "stability",
                  "has_political_power": "political power", "has_manpower": "manpower",
                  "num_of_factories": "factories", "num_of_military_factories": "military factories",
                  "num_of_civilian_factories": "civilian factories", "surrender_progress": "surrender progress",
                  "enemies_strength_ratio": "enemy strength ratio", "threat": "world tension"}

# designers (F-12): unit type -> designer; plane types; the XP branch each designer spends
DESIGN_KINDS = {"armor": "tank", "capital_ship": "ship", "screen_ship": "ship", "submarine": "ship", "carrier": "ship"}
PLANE_TYPES = {"fighter", "interceptor", "cas", "tactical_bomber", "strategic_bomber", "naval_bomber", "heavy_fighter",
               "maritime_patrol_plane", "suicide", "scout_plane", "transport_plane"}
XP_BRANCH = {"tank": "army", "plane": "air", "ship": "navy"}
DESIGN_DEFINES = ("EQUIPMENT_MODULE_ADD_XP_COST", "EQUIPMENT_MODULE_REPLACE_XP_COST", "EQUIPMENT_MODULE_CONVERT_XP_COST",
                  "EQUIPMENT_MODULE_REMOVE_XP_COST", "DESIGN_TEAM_CHANGE_XP_COST", "LICENSE_EQUIPMENT_UPGRADE_XP_FACTOR",
                  "BASE_LAND_EQUIPMENT_CONVERSION_IC_COST_FACTOR", "BASE_NAVAL_EQUIPMENT_CONVERSION_IC_COST_FACTOR")
DESIGNS_METHOD = ("bases = equipment with module_slots (own or inherited; duplicate_archetypes families generated) enabled "
                  "by a start technology (history set_technology at the date); modules = enable_equipment_modules of "
                  "those techs, kind from the slots that allow the module's category; next = enabled by a tech with "
                  "start_year <= the year + 2; loaded = the module block's `limit` (DLC) is not false; XP per change: "
                  "the module's xp_cost when set, else EQUIPMENT_MODULE_ADD_XP_COST (empty slot) / REPLACE (unrelated "
                  "module) / CONVERT (can_convert_from) / REMOVE [?] (rule from the defines' comments, not live-verified)")
MIO_BONUS_KEYS = ("equipment_bonus", "production_bonus", "organization_modifier")
MIO_DEFINES = ("FUNDS_FOR_SIZE_UP", "FUNDS_FOR_SIZE_UP_LEVEL_FACTOR", "FUNDS_FOR_SIZE_UP_LEVEL_POW",
               "UNLOCKED_TRAITS_PER_SIZE_UP", "ASSIGN_DESIGN_TEAM_PP_COST_PER_DAY",
               "ASSIGN_INDUSTRIAL_MANUFACTURER_PP_COST_PER_DAY", "DESIGN_TEAM_RESEARCH_BONUS",
               "FUNDS_FOR_RESEARCH_COMPLETION_PER_RESEARCH_COST", "FUNDS_FROM_MANUFACTURER_PER_IC_PER_DAY",
               "MAX_FUNDS_FROM_MANUFACTURER_PER_DAY", "DEFAULT_INITIAL_POLICY_ATTACH_COST",
               "DEFAULT_INITIAL_ATTACH_POLICY_COOLDOWN", "DESIGN_TEAM_CHANGE_XP_COST")
MIO_METHOD = ("common/military_industrial_organization/organizations with `include` resolved; listed when the static "
              "`allowed` (tag, original_tag, has_dlc, scripted triggers) is not false for the tag; visible/available "
              "true when static, else their text; traits unlock one per size-up (UNLOCKED_TRAITS_PER_SIZE_UP)")
PROJECT_DEFINES = ("DEFAULT_COMPLEXITY", "PROTOTYPE_PHASE_MAX_PROGRESS", "NEEDED_SUPPLY_FOR_FULL_SPEED_PROJECT",
                   "MINIMUM_PROJECT_SPEED_FACTOR_FROM_SUPPLY", "RECRUIT_SCIENTIST_COST", "SCIENTIST_SKILL_LEVEL_THRESHOLDS",
                   "SCIENTIST_SKILL_LEVEL_SPEED_MODIFIER", "DISMANTLE_FACILITY_DAYS", "BREAKTHROUGH_DAILY_TECHNOLOGY_GAIN",
                   "BREAKTHROUGH_DAILY_SCIENTIST_SKILL_GAIN", "AMOUNT_OF_SUPPORTIVE_SCIENTISTS")
PROJECT_METHOD = ("common/special_projects/projects; listed when the static `allowed` is not false; prototype_days from "
                  "sp_time constants, complexity {min, max} from sp_complexity (common/script_constants); outputs = "
                  "enable_equipment_modules / enable_equipments / set_technology found in project_output; facilities = "
                  "buildings with tags facility in own states at the start; scientists = characters with a scientist role")
TENSION_KEYS = ("generate_wargoal_tension", "join_faction_tension", "guarantee_tension", "lend_lease_tension",
                "send_volunteers_tension", "request_lease_tension")
GATE_TEXT = {"generate_wargoal_tension": "justify a war goal", "join_faction_tension": "join a faction",
             "guarantee_tension": "guarantee", "lend_lease_tension": "send lend-lease",
             "send_volunteers_tension": "send volunteers", "request_lease_tension": "request lend-lease"}
DIPLOMACY_DEFINES = ("BASE_GENERATE_WARGOAL_DAILY_PP", "WARGOAL_VERSUS_MAJOR_AT_WAR_REDUCTION",
                     "WARGOAL_WORLD_TENSION_REDUCTION", "WARGOAL_JUSTIFY_TENSION_FROM_PRODUCTION",
                     "MIN_WARGOAL_JUSTIFY_COST", "WARGOAL_PER_JUSTIFY_AND_WAR_COST_FACTOR", "WARGOAL_COST_LEND_LEASE",
                     "WARGOAL_COST_VOLUNTEERS", "GUARANTEE_COST", "VOLUNTEERS_DIVISIONS_REQUIRED",
                     "VOLUNTEERS_PER_TARGET_PROVINCE", "VOLUNTEERS_PER_COUNTRY_ARMY", "VOLUNTEERS_TRANSFER_SPEED",
                     "TENSION_CB_WAR", "TENSION_NO_CB_WAR", "TENSION_FACTION_JOIN", "TENSION_GUARANTEE",
                     "TENSION_VOLUNTEER_FORCE_DIVISION", "NAP_EXPIRY_MONTHS", "BASE_IMPROVE_RELATION_COST",
                     "BASE_SEND_ATTACHE_COST", "BASE_STAGE_COUP_DAILY_PP")
DIPLOMACY_METHOD = ("gate need = the ruling ideology's modifier (common/ideologies, hidden_modifier included) plus the "
                    "start spirits' modifiers of the same key, as a world-tension fraction (0.8 = 80%); the action needs "
                    "world tension at or above it (additive combination assumed [?]); rules = the ideology's `rules`; "
                    "war goals = common/wargoals with the static `allowed` decided (costs as in the file: "
                    "generate_base_cost + generate_per_state_cost per state; justification days not computed [?])")
LEADER_MODIFIER_KEYS = ("modifier", "non_shared_modifier", "corps_commander_modifier", "field_marshal_modifier")
COMMAND_DEFINES = ("BASE_MAX_COMMAND_POWER", "BASE_COMMAND_POWER_GAIN", "STARTING_COMMAND_POWER", "PROMOTE_LEADER_CP_COST",
                   "UNIT_LEADER_ASSIGN_TRAIT_COST", "ARMY_LEADER_COST", "NAVY_LEADER_COST",
                   "PREFERRED_TACTIC_COMMAND_POWER_COST", "BASE_SEND_ATTACHE_CP_COST", "CORPS_COMMANDER_DIVISIONS_CAP",
                   "FIELD_MARSHAL_DIVISIONS_CAP", "FIELD_MARSHAL_ARMIES_CAP", "FIELD_MARSHAL_XP_RATIO")
COMMAND_METHOD = ("spirits = common/ideas in idea categories of type army/navy/air_spirit (common/idea_tags; cost in that "
                  "branch's experience), static `allowed` decided; abilities = common/abilities (cost as in the file, "
                  "command power; duration and cooldown in hours [?]); traits = common/unit_leader assignable traits "
                  "with gui row/column, parents and `cost` as in the file [?]; skills = leader_skills level costs")
AGENCY_DEFINES = ("AGENCY_CREATION_DAYS", "AGENCY_UPGRADE_DAYS", "AGENCY_CREATION_FACTORIES", "AGENCY_UPGRADE_PER_OPERATIVE_SLOT",
                  "MAX_OPERATIVE_SLOT_FROM_AGENCY_UPGRADES", "AGENCY_OPERATIVE_RECRUITMENT_TIME")
AGENCY_METHOD = ("common/intelligence_agency_upgrades (one block per branch; `level` blocks in order), common/operations "
                 "(blocks with days), common/operation_phases; the agency needs La Resistance; operations with a "
                 "static `allowed` false for the tag are left out")
ENERGY_DEFINES = ("ENERGY_RESOURCE", "RESOURCE_TO_ENERGY_COEFFICIENT", "BASE_COUNTRY_ENERGY_PRODUCTION",
                  "ENERGY_SCALING_COST_BY_FACTORY_COUNT", "BASE_ENERGY_COST", "ENERGY_COST_CAP",
                  "ENERGY_SCALE_PER_TRADE_FACTORY_EXPORT")
RESOURCES_METHOD = ("produced = state history resources of the owned states at the start (extraction techs, "
                    "local_resources_factor and occupation left out [?]); exported = produced x the trade law's "
                    "min_export (the share the market takes at most); needed = start production lines x factories x the "
                    "equipment's resources (archetype resources for designs; modules add more); balance = produced - "
                    "exported - needed; a negative balance is imported or stalls the lines")
ENERGY_METHOD = ("produced = BASE_COUNTRY_ENERGY_PRODUCTION + energy resource in own states x RESOURCE_TO_ENERGY_COEFFICIENT; "
                 "per factory = min(ENERGY_COST_CAP, BASE_ENERGY_COST + ENERGY_SCALING_COST_BY_FACTORY_COUNT x factories); "
                 "demand = sum over states of factories x per factory x (1 + local consumption modifiers of the energy "
                 "buildings there; _per_infrastructure keys x infrastructure); factories = civilian + military + "
                 "dockyards; formula from the defines' comments, trade and country modifiers left out [?]")
FACTORY_BUILDINGS = ("industrial_complex", "arms_factory", "dockyard")
PEACE_DEFINES = ("BASE_PEACE_PUPPET_FACTOR", "BASE_PEACE_LIBERATE_FACTOR", "BASE_PEACE_TAKE_UNCONTROLLED_STATE_FACTOR",
                 "BASE_PEACE_TAKE_FACTION_CONTROLLED_STATE_FACTOR", "BASE_PEACE_FORCE_GOVERNMENT_COST",
                 "PEACE_COST_FACTOR_CONTESTED_BID", "PEACE_COST_FACTOR_UNCONTESTED_BID_MIN",
                 "PEACE_COST_FACTOR_UNCONTESTED_BID_MAX", "PEACE_COST_FACTOR_UNCONTESTED_BID_STEP",
                 "PEACE_INCREASE_COST_FACTOR_PER_MISSING_PERCENT_FOR_CAPITULATION", "PEACE_COST_FACTOR_COMPLIANCE_STEPS",
                 "PEACE_SCORE_SCALE_FACTOR", "PEACE_SCORE_DISTRIBUTION", "PEACE_CONTEST_REFUND_FACTOR",
                 "PEACE_ACTION_MAX_COST", "PEACE_COST_FACTOR_CAPITAL_SHIP_IC", "TENSION_ANNEX_NO_CLAIM", "TENSION_ANNEX_CLAIM",
                 "TENSION_ANNEX_CORE", "TENSION_PUPPET", "TENSION_LIBERATE")
PEACE_METHOD = ("common/peace_conference/cost_modifiers: a modifier is listed when its `enable` names the tag or one of "
                "the given enemies (tag / original_tag), or names no tag at all (generic); all active modifiers of an "
                "action multiply (cost_multiplier); base costs and bid factors from the defines")


def _numbers(block, deep=False):
    """{key: number} of a block's numeric items ({} for a non-block); deep flattens nested blocks (key of the leaf)."""
    out = {}
    if not isinstance(block, list):
        return out
    for k, v in block:
        if not k:
            continue
        if isinstance(v, list):
            if deep:
                out.update(_numbers(v, True))
        elif num(v) is not None:
            out[k] = num(v)
    return out


def expand_scripted(items, table, depth=3, aliases=()):
    """A trigger block with `name = yes` scripted triggers replaced by AND = { body } and `name = no` by
    NOT = { AND = { body } } (`depth` levels), `is_debug` false, and the scope blocks named in `aliases`
    (FROM, OWNER ...: scopes that are the country itself in that context) read as AND blocks."""
    if not isinstance(items, list):
        return items
    out = Block(getattr(items, "line", 1))
    ops = getattr(items, "ops", None)
    for i, (k, v) in enumerate(items):
        op = (ops[i] if ops is not None and i < len(ops) else "=") or "="
        if k == "is_debug" and v in ("yes", "no"):  # a campaign never runs in debug mode
            out.add(("always", "no" if v == "yes" else "yes"), line_of(items, i), "=")
        elif k in aliases and isinstance(v, list):
            out.add(("AND", expand_scripted(v, table, depth, aliases)), line_of(items, i), "=")
        elif depth > 0 and isinstance(v, str) and v in ("yes", "no") and k in table:
            body = expand_scripted(table[k], table, depth - 1, aliases)
            if v == "yes":
                out.add(("AND", body), line_of(items, i), "=")
            else:
                inner = Block(line_of(items, i) or 1)
                inner.add(("AND", body), line_of(items, i), "=")
                out.add(("NOT", inner), line_of(items, i), "=")
        elif isinstance(v, list):
            out.add((k, expand_scripted(v, table, depth, aliases)), line_of(items, i), op)
        else:
            out.add((k, v), line_of(items, i), op)
    return out


def occupation_settings(block):
    """{"resistance_active", "resistance", "compliance", "garrison"} set directly in a state-scope
    effect block (start_resistance, set_resistance, set_compliance, set_garrison_strength), or None."""
    out = {}
    for k, v in block:
        if k in OCCUPATION_KEYS and isinstance(v, str):
            out[OCCUPATION_KEYS[k]] = num(v, v)
    return out or None


def state_limit(items, row):
    """Static value of a state-scope `limit` (is_core_of, is_claimed_by, state, is_owned_by,
    is_controlled_by, AND/OR/NOT) against a World.states() row; None when it needs the running game."""
    if not isinstance(items, list):
        return True
    results = []
    for k, v in items:
        kl = (k or "").lower()
        if kl == "is_core_of" and isinstance(v, str):
            results.append(v in row["cores"])
        elif kl == "is_claimed_by" and isinstance(v, str):
            results.append(v in row["claims"])
        elif kl == "state" and isinstance(v, str) and v.isdigit():
            results.append(int(v) == row["id"])
        elif kl in ("is_owned_by", "is_owned_and_controlled_by") and isinstance(v, str):
            results.append(row["owner"] == v)
        elif kl == "is_controlled_by" and isinstance(v, str):
            results.append(row["controller"] == v)
        elif kl == "and" and isinstance(v, list):
            results.append(state_limit(v, row))
        elif kl == "or" and isinstance(v, list):
            sub = [state_limit([item], row) for item in v]
            results.append(True if True in sub else (None if None in sub else False))
        elif kl == "not" and isinstance(v, list):
            sub = [state_limit([item], row) for item in v]
            results.append(False if True in sub else (None if None in sub else True))
        else:
            results.append(None)
    if False in results:
        return False
    return None if None in results else True


def gates(items, group="all", out=None):
    """Numeric and named conditions of an `available` block (static parts already reduced):
    [{"key", "op", "value", "group": all | any | not | if, "text"}]. Numbers are compared values
    (has_war_support > 0.15); other atoms (has_war = yes, has_government = fascism) keep op "=". Every
    block below an OR is `any`, below a NOT `not`, inside an if `if` (a condition that applies only
    when its limit holds)."""
    out = [] if out is None else out
    if not isinstance(items, list):
        return out
    ops = getattr(items, "ops", None)
    for i, (k, v) in enumerate(items):
        kl = (k or "").lower()
        op = (ops[i] if ops is not None and i < len(ops) else "=") or "="
        if isinstance(v, list):
            if kl == "or":
                gates(v, "any" if group == "all" else group, out)
            elif kl == "not":
                gates(v, "not", out)
            elif kl in ("if", "else_if", "else"):
                gates(_sub(v, ("limit",)), "if", out)
            elif kl in ("and", "custom_trigger_tooltip", "hidden_trigger"):
                gates(v, group, out)
            continue
        if not k or kl in ("tooltip", "limit"):
            continue
        value = num(v)
        if op in ("<", ">", "<=", ">=") and value is not None:
            out.append({"key": k, "op": op, "value": value, "group": group,
                        "text": f"{GATE_KEYS_TEXT.get(kl, k)} {op} {v}"})
        elif kl in ("has_war", "has_government", "is_major", "is_puppet", "is_subject", "has_idea",
                    "has_completed_focus", "is_in_faction", "has_defensive_war", "has_offensive_war") or \
                kl in GATE_KEYS_TEXT:
            out.append({"key": k, "op": "=", "value": v, "group": group, "text": f"{k} = {v}"})
    return out


# tech_level classes: (class, equipment types, how kinds are named: "type" or the equipment archetype)
TECH_CLASSES = (
    ("anti_tank", {"anti_tank"}, "archetype"),
    ("armour", {"armor"}, "archetype"),
    ("anti_air", {"anti_air"}, "archetype"),
    ("air", {"fighter", "interceptor", "cas", "tactical_bomber", "strategic_bomber", "naval_bomber",
             "heavy_fighter", "maritime_patrol_plane", "suicide", "scout_plane"}, "type"),
)


def unit_row(name, block, group, path):
    """Where a sub-unit sits in a template: battalion, support (both rows), divisional support (regimental =
    no), regimental support (divisional = no, the 1.19 row), hq (Army HQ template), air or naval."""
    icon = get1(block, "map_icon_category")
    if icon in ("ship", "transport", "uboat"):
        return "naval"
    if group is None:
        return "air"
    if name.startswith("hq_") or os.path.basename(path).lower().startswith("hq_"):
        return "hq"
    if group == "support":
        if get1(block, "divisional") == "no":
            return "regimental support"
        if get1(block, "regimental") == "no":
            return "divisional support"
        return "support"
    return "battalion"


def _equipment_archetypes(game):
    """{equipment: archetype} (an archetype maps to itself)."""
    out = {}
    for p in game.files("common/units/equipment"):
        for k, v in parse_file(p):
            if k != "equipments" or not isinstance(v, list):
                continue
            for name, body in v:
                if name and isinstance(body, list):
                    out[name] = name if get1(body, "is_archetype") == "yes" else (get1(body, "archetype") or
                                                                                 get1(body, "parent") or name)
    return out


def _day_month(text):
    """Weather `between` token "27.1" (day.month, both 0-based) -> (28, 2)."""
    m = re.match(r"^(\d+)\.(\d+)$", str(text))
    return (int(m.group(1)) + 1, int(m.group(2)) + 1) if m else None


def xp_in(block):
    """{"army": n, "air": n, "navy": n} of the army/air/navy_experience effects in an effect block
    (every branch counted: an upper bound when the block has if/else)."""
    out = {}
    if not isinstance(block, list):
        return out
    for k, v in block:
        kl = (k or "").lower()
        if kl in XP_EFFECTS and num(v) is not None:
            out[XP_EFFECTS[kl]] = out.get(XP_EFFECTS[kl], 0) + num(v)
        elif isinstance(v, list) and kl not in ("limit", "ai_will_do", "available", "visible", "allowed"):
            for b, n in xp_in(v).items():
                out[b] = out.get(b, 0) + n
    return out


def gate_text(gs, limit=4):
    """Short text of a gates() list: "war support > 0.15; any of: has_war = yes | ..."."""
    alls = [g["text"] for g in gs if g["group"] == "all"]
    anys = [g["text"] for g in gs if g["group"] == "any"]
    nots = [g["text"] for g in gs if g["group"] == "not"]
    parts = alls[:limit]
    if anys:
        parts.append("any of: " + " | ".join(anys[:limit]))
    if nots:
        parts.append("not: " + " | ".join(nots[:2]))
    return "; ".join(parts)


# ---------------------------------------------------------------- 365-day calendar (derive.py)

def day_number(date):
    """Days since 1.1.1 of a date in the game's 365-day calendar (derive.day)."""
    n = derive.day(date)
    if n is None:
        raise ValueError(f"not a date: {date!r}")
    return n


def from_day(n):
    y, m, d = derive.date_of(n)
    return f"{y}.{m}.{d}"


def add_days(date, days):
    return from_day(day_number(date) + int(math.ceil(days)))


class Loc:
    """English localisation through derive.loc (one table per Game), cleaned of colour codes and icons."""

    FORMAT = re.compile(r"§[A-Za-z0-9!]|£[A-Za-z0-9_|]+£?|\$[A-Za-z0-9_|.]+\$")

    def __init__(self, game):
        self.game = game

    def get(self, key, default=None):
        if not key:
            return default
        v = derive.loc(self.game, key)
        if v is None:
            return default
        v = self.FORMAT.sub("", v.replace("\\n", " ").replace('\\"', '"'))
        return re.sub(r"\s+", " ", v).strip() or default

    def name(self, key):
        return self.get(key, key)


def norm_date(date):
    t = date_tuple(date)
    return f"{t[0]}.{t[1]}.{t[2]}" if t else date


def num(v, default=None):
    try:
        f = float(v)
        return int(f) if f.is_integer() and re.match(r"^-?\d+$", str(v).strip()) else f
    except (TypeError, ValueError):
        return default


# ---------------------------------------------------------------- the world at a start date

class World:
    def __init__(self, game, tag, date="1936.1.1", dlcs=(), mode="fair", language="english"):
        self.game = game
        self.content = game.content
        self.tag = tag
        self.date = norm_date(date or "1936.1.1")
        names = list(dict.fromkeys(list(dlcs) + list(game.integrated_dlcs())))
        self.dlcs = names
        self.active_dlcs = [d for d in dlcs]
        self.mode = mode
        self.ctx = Ctx(tag, names, self.date)
        self.loc = Loc(game)
        self.defines = Defines(self.content)
        self._cache = {}
        self._rel = None

    # ------------------------------------------------------------ sources
    def cached(self, name, fn):
        if name not in self._cache:
            self._cache[name] = fn()
        return self._cache[name]

    def ctx_for(self, tag):
        return Ctx(tag, self.dlcs, self.date)

    def rel(self, path):
        if self._rel is None:
            self._rel = {p: r for r, p, _ in self.content.merged().values()}
        return self._rel.get(path) or os.path.basename(path or "")

    def src(self, path, line=None):
        if not path:
            return None
        r = self.rel(path)
        return f"{r}:{line}" if line else r

    def fact(self, v, path=None, line=None, d=None, fair=None):
        out = {"v": v, "src": self.src(path, line), "mod": self.content.source(path) if path else None}
        if d:
            out["d"] = d
        if fair:
            out["fair"] = fair
        return out

    def define(self, key, default=None):
        """(value, fact) of a define (fact None when missing)."""
        full, row = self.defines.row(key)
        if row is None:
            return default, None
        return row["v"], dict(self.fact(row["v"], row["file"], row["line"]), key=full)

    def name(self, key):
        return self.loc.name(key)

    def state_name(self, sid):
        return self.loc.get(f"STATE_{sid}") or f"state {sid}"

    def province_name(self, pid):
        return self.loc.get(f"VICTORY_POINTS_{pid}") or None

    def country_name(self, tag):
        return self.loc.get(tag) or self.loc.get(f"{tag}_DEF") or self.country(tag)["name"]

    def visible(self, fair):
        """True when a fact of this FAIR level is shown in the current mode."""
        return self.mode != "fair" or fair != "private"

    def text(self, items, limit=300, tag=None):
        """One-line trigger text with the static parts (has_dlc, tag, start date) already decided."""
        if not isinstance(items, list):
            return items
        return render(reduce_trigger(items, self.ctx_for(tag or self.tag)), limit)

    def variable(self, name, tags=None):
        """The start value fact of a scenario variable set in a country history ("ETH.x" or "x")."""
        short = name.split(".")[-1]
        scope = name.split(".")[0] if "." in name else None
        order = [scope] if scope and re.match(r"^[A-Z][A-Z0-9]{2}$", scope) else []
        order += list(tags or []) + [self.tag] + self.at_war_with(self.tag)
        for t in dict.fromkeys(order):
            for v in self.country(t)["variables"]:
                if v["name"].split(".")[-1] == short:
                    return dict(v, tag=t)
        return None

    # ------------------------------------------------------------ country history
    def country(self, tag=None):
        tag = tag or self.tag
        return self.cached("country:" + tag, lambda: self._country(tag))

    def _country(self, tag):
        rel = self.game.history_file(tag)
        out = {"tag": tag, "name": self.loc.get(tag) or self.loc.get(f"{tag}_DEF") or tag,
               "history_file": rel, "capital": None,
               "research_slots": None, "stability": None, "war_support": None, "convoys": None,
               "ideas": [], "removed_ideas": [], "oobs": [], "politics": {}, "popularities": {},
               "variables": [], "flags": [], "global_flags": [], "wars": [], "guarantees": [],
               "faction": [], "access": [], "subjects": [], "characters": [], "doctrines": [],
               "techs": [], "events": [], "completed_focuses": [], "dynamic_modifiers": [],
               "political_power": None, "command_power": None, "power_balance": [], "experience": {},
               "state_scopes": [], "variants": [], "threats": [], "agency": None}
        path = self.game.file(rel) if rel else None
        if not path:
            return out
        F = lambda v, line, **kw: self.fact(v, path, line, **kw)  # noqa: E731
        ideas = {}

        def visit(k, v, line):
            kl = (k or "").lower()
            if kl == "capital" and isinstance(v, str):
                out["capital"] = F(num(v), line)
            elif kl == "set_research_slots":
                out["research_slots"] = F(num(v), line)
            elif kl == "set_stability":
                out["stability"] = F(num(v), line)
            elif kl == "set_war_support":
                out["war_support"] = F(num(v), line)
            elif kl == "set_convoys":
                out["convoys"] = F(num(v), line)
            elif kl == "add_political_power":
                out["political_power"] = F(num(v), line)
            elif kl == "add_command_power":
                out["command_power"] = F(num(v), line)
            elif kl == "add_ideas":
                for idea in (bare(v) if isinstance(v, list) else [v]):
                    ideas[idea] = F(idea, line)
            elif kl == "remove_ideas":
                for idea in (bare(v) if isinstance(v, list) else [v]):
                    ideas.pop(idea, None)
                    out["removed_ideas"].append(F(idea, line))
            elif kl in ("oob", "set_oob", "set_naval_oob", "set_air_oob", "load_oob") and isinstance(v, str):
                out["oobs"].append(dict(F(v, line), kind=kl))
            elif kl == "set_politics" and isinstance(v, list):
                for a, b in v:
                    if a and isinstance(b, str):
                        out["politics"][a] = F(b, line)
            elif kl == "set_popularities" and isinstance(v, list):
                for a, b in v:
                    if a:
                        out["popularities"][a] = F(num(b), line)
            elif kl == "set_variable" and isinstance(v, list):
                var, val = None, None
                if get1(v, "var") is not None:
                    var, val = get1(v, "var"), get1(v, "value")
                else:
                    for a, b in v:
                        if a and isinstance(b, str):
                            var, val = a, b
                            break
                if var is not None and num(val) is not None:
                    out["variables"].append(dict(F(num(val), line), name=var))
            elif kl in ("set_country_flag", "set_global_flag"):
                flag = get1(v, "flag") if isinstance(v, list) else v
                if flag:
                    out["flags" if kl == "set_country_flag" else "global_flags"].append(F(flag, line))
            elif kl == "declare_war_on" and isinstance(v, list):
                out["wars"].append(dict(F(get1(v, "target"), line), type=get1(v, "type"), attacker=tag))
            elif kl == "give_guarantee":
                out["guarantees"].append(F(v, line))
            elif kl in ("create_faction", "add_to_faction"):
                out["faction"].append(dict(F(v, line), kind=kl))
            elif kl == "give_military_access":
                out["access"].append(F(v, line))
            elif kl in ("set_autonomy", "puppet", "release_puppet"):
                target = get1(v, "target") if isinstance(v, list) else v
                out["subjects"].append(dict(F(target, line), kind=kl,
                                            level=get1(v, "autonomy_state") if isinstance(v, list) else None))
            elif kl == "recruit_character" and isinstance(v, str):
                out["characters"].append(F(v, line))
            elif kl in ("set_grand_doctrine", "add_doctrine", "set_sub_doctrine", "add_subdoctrine"):
                out["doctrines"].append(dict(F(v if isinstance(v, str) else render(v, 120), line), kind=kl))
            elif kl == "set_technology" and isinstance(v, list):
                out["techs"].extend(a for a, b in v if a and b == "1")
            elif kl in ("country_event", "news_event"):
                eid = get1(v, "id") if isinstance(v, list) else v
                days = num(get1(v, "days")) if isinstance(v, list) else None
                out["events"].append(dict(F(eid, line), days=days,
                                          random_days=num(get1(v, "random_days")) if isinstance(v, list) else None))
            elif kl in ("complete_national_focus",):
                out["completed_focuses"].append(F(v, line))
            elif kl == "add_dynamic_modifier" and isinstance(v, list):
                out["dynamic_modifiers"].append(F(get1(v, "modifier"), line))
            elif kl == "set_power_balance" and isinstance(v, list):
                out["power_balance"].append(dict(F(get1(v, "id"), line), kind="set",
                                                 set_default=get1(v, "set_default") == "yes",
                                                 value=num(get1(v, "set_value")),
                                                 left_side=get1(v, "left_side"), right_side=get1(v, "right_side")))
            elif kl == "add_power_balance_value" and isinstance(v, list):
                out["power_balance"].append(dict(F(get1(v, "id"), line), kind="add", value=num(get1(v, "value"))))
            elif kl == "create_equipment_variant" and isinstance(v, list):
                mods, ups = get1(v, "modules"), get1(v, "upgrades")
                out["variants"].append(dict(F(get1(v, "name"), line), type=get1(v, "type"),
                                            modules={a: b for a, b in mods if a and isinstance(b, str)}
                                            if isinstance(mods, list) else {},
                                            upgrades={a: num(b, b) for a, b in ups if a} if isinstance(ups, list) else {},
                                            obsolete=get1(v, "obsolete") == "yes"))
            elif kl == "add_named_threat" and isinstance(v, list) and num(get1(v, "threat")) is not None:
                out["threats"].append(dict(F(num(get1(v, "threat")), line), name=get1(v, "name")))
            elif kl == "add_threat" and num(v) is not None:
                out["threats"].append(dict(F(num(v), line), name=None))
            elif kl == "create_intelligence_agency":
                out["agency"] = F(get1(v, "name") if isinstance(v, list) else (v or "yes"), line)
            elif kl in XP_EFFECTS and num(v) is not None:
                branch = XP_EFFECTS[kl]
                prev = out["experience"].get(branch, {}).get("v", 0)
                out["experience"][branch] = F(prev + num(v), line)
            elif (k or "").isdigit() and isinstance(v, list):
                occ = occupation_settings(v)
                if occ:
                    out["state_scopes"].append(dict(F(int(k), line), occupation=occ))
            elif kl == "every_owned_state" and isinstance(v, list):
                occ = occupation_settings(v)
                if occ:
                    out["state_scopes"].append(dict(F("every_owned_state", line), occupation=occ,
                                                    limit=get1(v, "limit")))

        history_walk(parse_file(path), self.ctx_for(tag), self.date, visit, with_line=True)
        out["ideas"] = list(ideas.values())
        party = out["politics"].get("ruling_party")
        if out["name"] == tag and party:
            out["name"] = self.loc.get(f"{tag}_{party['v']}") or self.loc.get(f"{tag}_{party['v']}_DEF") or tag
        out["techs"] = sorted(set(out["techs"]))
        return out

    def ruling_party(self, tag=None):
        p = self.country(tag)["politics"].get("ruling_party")
        return p["v"] if p else None

    # ------------------------------------------------------------ wars at start
    def wars(self):
        """Every declare_war_on in the country histories at the start date: [{attacker, target, type, src}]."""

        def load():
            out = []
            for p in self.game.files("history/countries"):
                try:
                    with open(p, encoding="utf-8-sig", errors="replace") as fh:
                        if "declare_war_on" not in fh.read():
                            continue
                except OSError:
                    continue
                m = re.match(r"^([A-Z][A-Z0-9]{2})", os.path.basename(p))
                if not m:
                    continue
                for w in self.country(m.group(1))["wars"]:
                    out.append({"attacker": m.group(1), "target": w["v"], "type": w.get("type"),
                                "src": w["src"], "mod": w["mod"]})
            return out

        return self.cached("wars", load)

    def start_global_flags(self):
        """Global flags the country histories set at the start date (any country's history can set one)."""

        def load():
            out = set()
            for p in self.game.files("history/countries"):
                try:
                    with open(p, encoding="utf-8-sig", errors="replace") as fh:
                        if "set_global_flag" not in fh.read():
                            continue
                except OSError:
                    continue
                m = re.match(r"^([A-Z][A-Z0-9]{2})", os.path.basename(p))
                if m:
                    out.update(f["v"] for f in self.country(m.group(1))["global_flags"] if f.get("v"))
            return out

        return self.cached("start_global_flags", load)

    def at_war_with(self, tag=None):
        tag = tag or self.tag
        enemies = set()
        for w in self.wars():
            if w["attacker"] == tag:
                enemies.add(w["target"])
            elif w["target"] == tag:
                enemies.add(w["attacker"])
        return sorted(e for e in enemies if e)

    # ------------------------------------------------------------ states
    def state_categories(self):
        def load():
            out = {}
            for p in self.game.files("common/state_category"):
                items = parse_file(p)
                for i, (k, v) in enumerate(items):
                    if k != "state_categories" or not isinstance(v, list):
                        continue
                    for j, (name, body) in enumerate(v):
                        if name and isinstance(body, list):
                            out[name] = {"slots": num(get1(body, "local_building_slots"), 0),
                                         "src": self.src(p, line_of(v, j))}
            return out

        return self.cached("statecats", load)

    def building_types(self):
        """{building: {"cost", "shares_slots", "state_max", "province_max", "src"}} from common/buildings."""

        def load():
            out = {}
            for p in self.game.files("common/buildings"):
                items = parse_file(p)
                for k, v in items:
                    if k != "buildings" or not isinstance(v, list):
                        continue
                    for j, (name, body) in enumerate(v):
                        if not name or not isinstance(body, list):
                            continue
                        cap = get1(body, "level_cap", [])
                        cap = cap if isinstance(cap, list) else []
                        out[name] = {"cost": num(get1(body, "base_cost")),
                                     "per_level_cost": num(get1(body, "per_level_extra_cost")),
                                     "coastal_only": get1(body, "only_costal") == "yes",
                                     "shares_slots": get1(cap, "shares_slots") == "yes",
                                     "state_max": num(get1(cap, "state_max")),
                                     "province_max": num(get1(cap, "province_max")),
                                     "infra_effect": get1(body, "infrastructure_construction_effect") == "yes",
                                     "src": self.src(p, line_of(v, j))}
            return out

        return self.cached("buildingtypes", load)

    def states(self):
        """{state id: row} at the start date (see the module doc)."""
        return self.cached("states", self._states)

    def _states(self):
        cats = self.state_categories()
        btypes = self.building_types()
        out = {}
        for sid, (path, st) in self.game._state_blocks().items():
            cat = get1(st, "state_category", "")
            res = get1(st, "resources", [])
            row = {"id": sid, "name": self.state_name(sid), "file": self.src(path, getattr(st, "line", None)),
                   "mod": self.content.source(path),
                   "category": cat, "slots": cats.get(cat, {}).get("slots"),
                   "manpower": num(get1(st, "manpower"), 0),
                   "resources": {a: num(b, 0) for a, b in (res if isinstance(res, list) else []) if a},
                   "provinces": [int(x) for x in bare(get1(st, "provinces", [])) if x.isdigit()],
                   "impassable": get1(st, "impassable") == "yes",
                   "owner": None, "controller": None, "cores": [], "claims": [],
                   "buildings": {}, "province_buildings": {}, "vps": {}, "vp_src": {}, "occupation": {}}
            hist = get1(st, "history", [])
            if isinstance(hist, list):

                def visit(k, v, line, row=row, path=path):
                    if k in OCCUPATION_KEYS and isinstance(v, str):
                        occ = row["occupation"]
                        occ[OCCUPATION_KEYS[k]] = num(v, v)
                        occ.setdefault("src", self.src(path, line))
                    elif k in ("owner", "transfer_state_to") and isinstance(v, str):
                        row["owner"] = row["controller"] = v
                    elif k == "controller" and isinstance(v, str):
                        row["controller"] = v
                    elif k == "add_core_of" and isinstance(v, str):
                        if v not in row["cores"]:
                            row["cores"].append(v)
                    elif k == "remove_core_of" and isinstance(v, str):
                        row["cores"] = [c for c in row["cores"] if c != v]
                    elif k == "add_claim_by" and isinstance(v, str):
                        row["claims"].append(v)
                    elif k == "victory_points" and isinstance(v, list):
                        vals = bare(v)
                        for i in range(0, len(vals) - 1, 2):
                            if vals[i].isdigit() and num(vals[i + 1]) is not None:
                                row["vps"][int(vals[i])] = num(vals[i + 1])
                                row["vp_src"][int(vals[i])] = self.src(path, line)
                    elif k == "buildings" and isinstance(v, list):
                        for bk, bv in v:
                            if bk and bk.isdigit() and isinstance(bv, list):
                                pb = row["province_buildings"].setdefault(int(bk), {})
                                for a, b in bv:
                                    if a and num(b) is not None:
                                        pb[a] = num(b)
                            elif bk and num(bv) is not None:
                                row["buildings"][bk] = num(bv)

                history_walk(hist, self.ctx, self.date, visit, with_line=True)
            used = sum(n for b, n in row["buildings"].items() if btypes.get(b, {}).get("shares_slots"))
            row["used_slots"] = used
            row["free_slots"] = (row["slots"] - used) if row["slots"] is not None else None
            row["infrastructure"] = row["buildings"].get("infrastructure", 0)
            out[sid] = row
        return out

    def owned_states(self, tag=None):
        tag = tag or self.tag
        return sorted(sid for sid, r in self.states().items() if r["owner"] == tag)

    def province_state(self):
        return self.game.province_state()

    # ------------------------------------------------------------ map
    def graph(self):
        return self.game.map_graph()

    def owner_of_province(self, pid):
        sid = self.province_state().get(pid)
        st = self.states().get(sid) if sid else None
        return st["owner"] if st else None

    def state_borders(self):
        """{state: {neighbour state: [(own province, other province)]}} over passable land edges."""

        def load():
            g = self.graph()
            if g is None:
                return {}
            ps = self.province_state()
            out = defaultdict(lambda: defaultdict(list))
            for a, row in g["adj"].items():
                if g["provinces"].get(a, {}).get("type") != "land":
                    continue
                for b, e in row.items():
                    if e.get("impassable") or g["provinces"].get(b, {}).get("type") != "land":
                        continue
                    sa, sb = ps.get(a), ps.get(b)
                    if sa and sb and sa != sb:
                        out[sa][sb].append((a, b))
            return {k: dict(v) for k, v in out.items()}

        return self.cached("stateborders", load)

    def neighbours(self, tag=None):
        """{neighbour tag: {"states": [their states on the border], "ours": [our states], "pairs": n}}."""
        tag = tag or self.tag
        states = self.states()
        out = {}
        for sid in self.owned_states(tag):
            for nb, pairs in self.state_borders().get(sid, {}).items():
                owner = states.get(nb, {}).get("owner")
                if not owner or owner == tag:
                    continue
                row = out.setdefault(owner, {"states": set(), "ours": set(), "pairs": 0})
                row["states"].add(nb)
                row["ours"].add(sid)
                row["pairs"] += len(pairs)
        return {t: {"states": sorted(r["states"]), "ours": sorted(r["ours"]), "pairs": r["pairs"]}
                for t, r in sorted(out.items())}

    def province_steps(self, sources):
        """{province: land steps to the nearest source province} (derive.distances: BFS over passable
        land borders and straits)."""
        return derive.distances(self.game, list(sources))

    def provinces_of(self, tags):
        tags = set(tags)
        out = []
        for sid, r in self.states().items():
            if r["owner"] in tags:
                out.extend(r["provinces"])
        return out

    # ------------------------------------------------------------ ideas and laws
    def ideas(self):
        """{idea: {"category", "law", "cost", "removal_cost", "modifier", "allowed", "available",
        "block", "path", "line"}} from common/ideas."""

        def load():
            out = {}
            for p in self.game.files("common/ideas", recursive=True):
                for k, v in parse_file(p):
                    if k != "ideas" or not isinstance(v, list):
                        continue
                    for cat, body in v:
                        if not cat or not isinstance(body, list):
                            continue
                        law = get1(body, "law") == "yes"
                        for j, (name, idea) in enumerate(body):
                            if not name or not isinstance(idea, list):
                                continue
                            mod = get1(idea, "modifier")
                            out[name] = {"id": name, "category": cat, "law": law,
                                         "cost": num(get1(idea, "cost")),
                                         "removal_cost": num(get1(idea, "removal_cost")),
                                         "modifier": mod if isinstance(mod, list) else [],
                                         "allowed": get1(idea, "allowed"), "available": get1(idea, "available"),
                                         "default": get1(idea, "default") == "yes",
                                         "block": idea, "path": p, "line": line_of(body, j)}
            return out

        return self.cached("ideas", load)

    def idea_slots(self):
        """{slot or category: {"group", "cost", "removal_cost"}} from common/idea_tags."""

        def load():
            out = {}
            for p in self.game.files("common/idea_tags"):
                for k, v in parse_file(p):
                    if k != "idea_categories" or not isinstance(v, list):
                        continue
                    for group, body in v:
                        if not group or not isinstance(body, list):
                            continue
                        row = {"group": group, "cost": num(get1(body, "cost")),
                               "removal_cost": num(get1(body, "removal_cost"))}
                        out.setdefault(group, row)
                        for kind in ("slot", "character_slot"):
                            for s in get(body, kind):
                                if isinstance(s, str):
                                    out.setdefault(s, dict(row, character=kind == "character_slot"))
            return out

        return self.cached("ideaslots", load)

    def idea_cost(self, idea):
        i = self.ideas().get(idea)
        if not i:
            return None
        if i["cost"] is not None:
            return i["cost"]
        return self.idea_slots().get(i["category"], {}).get("cost")

    def modifier_value(self, idea, key):
        i = self.ideas().get(idea)
        if not i:
            return None
        for a, b in i["modifier"]:
            if a == key:
                return num(b)
        return None

    def idea_row(self, idea, tag=None):
        i = self.ideas().get(idea)
        if not i:
            return None
        ctx = self.ctx_for(tag or self.tag)
        allowed = trigger(i["allowed"], ctx) if isinstance(i["allowed"], list) else True
        return {"id": idea, "name": self.name(idea), "category": i["category"], "law": i["law"],
                "cost": self.idea_cost(idea), "removal_cost": i["removal_cost"],
                "allowed": allowed,
                "gates": gates(reduce_trigger(i["available"], ctx)) if isinstance(i["available"], list) else [],
                "available": self.text(i["available"], 240, tag) if isinstance(i["available"], list) else None,
                "modifier": render(i["modifier"], 300) if i["modifier"] else None,
                "src": self.src(i["path"], i["line"]), "mod": self.content.source(i["path"])}

    def laws(self, tag=None):
        """{category: {"current", "options": [idea rows]}} for the law categories."""
        tag = tag or self.tag
        current = {f["v"] for f in self.country(tag)["ideas"]}
        out = {}
        for iid, i in self.ideas().items():
            if not i["law"]:
                continue
            row = self.idea_row(iid, tag)
            if row["allowed"] is False:
                continue
            cat = out.setdefault(i["category"], {"current": None, "options": []})
            cat["options"].append(row)
            if iid in current:
                cat["current"] = iid
        return out

    # ------------------------------------------------------------ characters
    def characters(self, tag=None, role=None):
        """Characters of the tag's character files plus those it recruits at the start, with their
        roles (DLC `instance`/`allowed` blocks evaluated): [{id, name, roles: [...], recruited, src}]."""
        tag = tag or self.tag
        recruited = {f["v"] for f in self.country(tag)["characters"]}
        ctx = self.ctx_for(tag)
        roles_wanted = {"general": ("corps_commander", "field_marshal"), "marshal": ("field_marshal",),
                        "admiral": ("navy_leader",), "advisor": ("advisor",), "leader": ("country_leader",),
                        "scientist": ("scientist",)}
        want = roles_wanted.get(role, (role,) if role else None)
        out = []
        for p in self.game.files("common/characters"):
            base = os.path.basename(p)
            file_tag = base[:3] if re.match(r"^[A-Z][A-Z0-9]{2}", base) else None
            for k, v in parse_file(p):
                if k != "characters" or not isinstance(v, list):
                    continue
                for j, (cid, body) in enumerate(v):
                    if not cid or not isinstance(body, list):
                        continue
                    if cid not in recruited and file_tag != tag and not cid.startswith(tag + "_"):
                        continue
                    blocks = [body]
                    for inst in get(body, "instance"):
                        if isinstance(inst, list) and trigger(get1(inst, "allowed", []), ctx) is not False:
                            blocks.append(inst)
                    roles = []
                    for b in blocks:
                        for rk, rv in b:
                            if rk not in ("country_leader", "corps_commander", "field_marshal", "navy_leader",
                                          "advisor", "operative", "scientist") or not isinstance(rv, list):
                                continue
                            if trigger(get1(rv, "allowed", []), ctx) is False:
                                continue
                            r = {"role": rk}
                            traits = get1(rv, "traits")
                            if isinstance(traits, list):
                                r["traits"] = bare(traits)
                            if rk == "scientist":
                                r["skills"] = {a: num(c) for a, c in (get1(rv, "skills") or []) if a and num(c) is not None}
                            for sk in ("skill", "attack_skill", "defense_skill", "planning_skill",
                                       "logistics_skill", "maneuvering_skill", "coordination_skill"):
                                if get1(rv, sk) is not None:
                                    r[sk.replace("_skill", "")] = num(get1(rv, sk))
                            if rk == "advisor":
                                slot = get1(rv, "slot")
                                cost = num(get1(rv, "cost"))
                                r.update({"slot": slot, "idea_token": get1(rv, "idea_token"),
                                          "cost": cost if cost is not None else
                                          self.idea_slots().get(slot, {}).get("cost")})
                                av = get1(rv, "available")
                                if isinstance(av, list):
                                    r["available"] = self.text(av, 160, tag)
                            if rk == "country_leader":
                                r["ideology"] = get1(rv, "ideology")
                            vis = get1(rv, "visible")
                            if isinstance(vis, list):
                                r["visible"] = render(vis, 120)
                            roles.append(r)
                    if want and not any(r["role"] in want for r in roles):
                        continue
                    out.append({"id": cid, "name": self.name(cid), "recruited": cid in recruited,
                                "roles": roles, "src": self.src(p, line_of(v, j)), "mod": self.content.source(p)})
        return out

    # ------------------------------------------------------------ orders of battle
    def oob(self, tag=None):
        """Templates, divisions (with state), air wings, fleets and production lines of the OOBs the
        tag's history loads at the start."""
        tag = tag or self.tag
        return self.cached("oob:" + tag, lambda: self._oob(tag))

    def _oob(self, tag):
        ctx = self.ctx_for(tag)
        ps = self.province_state()
        out = {"files": [], "templates": {}, "divisions": [], "air": [], "fleets": [], "production": []}
        for o in self.country(tag)["oobs"]:
            path = self.game.file(f"history/units/{o['v']}.txt")
            if not path:
                out["files"].append({"name": o["v"], "kind": o["kind"], "src": None, "missing": True})
                continue
            out["files"].append({"name": o["v"], "kind": o["kind"], "src": self.src(path), "set_by": o["src"],
                                 "mod": self.content.source(path)})

            def visit(k, v, line, path=path):
                if not isinstance(v, list):
                    return
                if k == "division_template":
                    name = get1(v, "name")
                    regs = defaultdict(int)
                    sup, rsup = [], []
                    for rk, rv in v:
                        if rk == "regiments" and isinstance(rv, list):
                            for a, _ in rv:
                                if a:
                                    regs[a] += 1
                        elif rk == "support" and isinstance(rv, list):
                            sup.extend(a for a, _ in rv if a)
                        elif rk == "regimental_support" and isinstance(rv, list):
                            rsup.extend(a for a, _ in rv if a)
                    out["templates"][name] = {"name": name, "regiments": dict(regs), "support": sup,
                                              "regimental_support": rsup,
                                              "division_cap": num(get1(v, "division_cap")),
                                              "src": self.src(path, line)}
                elif k == "division":
                    loc = num(get1(v, "location"))
                    out["divisions"].append({"template": get1(v, "division_template"), "location": loc,
                                             "state": ps.get(loc), "src": self.src(path, line)})
                elif k == "units":
                    run_effects(v, ctx, visit, with_line=True)
                elif k == "air_wings":
                    for sk, sv in v:
                        if not (sk and sk.isdigit() and isinstance(sv, list)):
                            continue
                        for ek, ev in sv:
                            if isinstance(ev, list) and get1(ev, "amount") is not None:
                                out["air"].append({"state": int(sk), "type": ek, "amount": num(get1(ev, "amount")),
                                                   "version": get1(ev, "version_name"), "src": self.src(path, line)})
                elif k == "navy" or k == "fleet":
                    self._fleet(v, out, path, line)
                elif k == "instant_effect":
                    def prod(a, b, ln):
                        if a == "add_equipment_production" and isinstance(b, list):
                            eq = get1(b, "equipment", [])
                            out["production"].append({"type": get1(eq, "type") if isinstance(eq, list) else None,
                                                      "version": get1(eq, "version_name") if isinstance(eq, list) else None,
                                                      "factories": num(get1(b, "requested_factories")),
                                                      "src": self.src(path, ln)})
                    run_effects(v, ctx, prod, with_line=True)

            run_effects(parse_file(path), ctx, visit, with_line=True)
        return out

    def _fleet(self, v, out, path, line):
        if get1(v, "fleet") is not None or get1(v, "task_force") is not None or get1(v, "ship") is not None:
            pass
        ships = defaultdict(int)

        def count(block):
            for a, b in block:
                if a == "ship" and isinstance(b, list):
                    eq = get1(b, "equipment", [])
                    kind = eq[0][0] if isinstance(eq, list) and eq else get1(b, "definition")
                    ships[kind or "ship"] += 1
                elif isinstance(b, list):
                    count(b)

        count(v)
        out["fleets"].append({"name": get1(v, "name"), "base": num(get1(v, "naval_base")),
                              "ships": dict(ships), "src": self.src(path, line)})

    # ------------------------------------------------------------ decisions and missions
    def decision_index(self):
        """[{id, category, block, path, line, cat_allowed}] of every decision."""

        def load():
            out = []
            for p in self.game.files("common/decisions"):
                for i, (cat, body) in enumerate(parse_file(p)):
                    if not cat or not isinstance(body, list) or cat.startswith("@"):
                        continue
                    for j, (did, d) in enumerate(body):
                        if not did or not isinstance(d, list):
                            continue
                        if did in ("allowed", "visible", "available", "icon", "priority", "picture",
                                   "on_map_area", "scripted_gui", "visibility_type", "target_root_trigger"):
                            continue
                        out.append({"id": did, "category": cat, "block": d, "path": p, "line": line_of(body, j)})
            return out

        return self.cached("decisions", load)

    def decision_categories(self):
        def load():
            out = {}
            for p in self.game.files("common/decisions/categories"):
                for i, (cat, body) in enumerate(parse_file(p)):
                    if cat and isinstance(body, list):
                        out[cat] = {"allowed": get1(body, "allowed"), "visible": get1(body, "visible"),
                                    "src": self.src(p, line_of(parse_file(p), i))}
            return out

        return self.cached("decisioncats", load)

    def decisions(self, tag=None, missions=None):
        """Decisions the tag may take: static `allowed` (decision and category) not false. Each row has
        cost, timers, conditions (one-line text) and the control lists of `available`.
        missions=True keeps only missions (days_mission_timeout), False drops them."""
        tag = tag or self.tag
        ctx = self.ctx_for(tag)
        cats = self.decision_categories()
        out = []
        for d in self.decision_index():
            b = d["block"]
            cat = cats.get(d["category"], {})
            if isinstance(cat.get("allowed"), list) and trigger(cat["allowed"], ctx) is False:
                continue
            allowed = get1(b, "allowed")
            if isinstance(allowed, list):
                if trigger(allowed, ctx) is False:
                    continue
            elif not _names_tag(b, tag) and not _names_tag(cat.get("visible") or [], tag) and \
                    not _names_tag(cat.get("allowed") or [], tag):
                continue  # no allowed block and nothing names the tag: a generic decision, left out
            raw = get1(b, "days_mission_timeout")
            timeout = num(raw) if num(raw) is not None else raw  # a number or a variable name
            if missions is True and timeout is None:
                continue
            if missions is False and timeout is not None:
                continue
            row = self.decision_row(d, timeout)
            row["specific"] = _names_tag(allowed if isinstance(allowed, list) else [], tag) or \
                _names_tag(cat.get("allowed") or [], tag) or _names_tag(cat.get("visible") or [], tag)
            out.append(row)
        return out

    def decision_row(self, d, timeout=None):
        b = d["block"]
        row = {"id": d["id"], "name": self.name(d["id"]), "category": d["category"],
               "cost": num(get1(b, "cost")) if get1(b, "cost") is not None else get1(b, "cost"),
               "days_remove": num(get1(b, "days_remove")), "days_re_enable": num(get1(b, "days_re_enable")),
               "mission_days": timeout if timeout is not None else num(get1(b, "days_mission_timeout")),
               "fire_only_once": get1(b, "fire_only_once") == "yes",
               "src": self.src(d["path"], d["line"]), "mod": self.content.source(d["path"])}
        if row["mission_days"] is None and get1(b, "days_mission_timeout") is not None:
            row["mission_days"] = get1(b, "days_mission_timeout")  # a variable name
        if isinstance(row["mission_days"], str):
            var = self.variable(row["mission_days"])
            if var:
                row["mission_days_value"] = var
        for key in ("available", "visible", "activation", "cancel_trigger"):
            v = get1(b, key)
            if isinstance(v, list) and v:
                row[key] = self.text(v, 300)
        for key in ("complete_effect", "remove_effect", "timeout_effect", "cancel_effect"):
            v = get1(b, key)
            if isinstance(v, list) and v:
                row[key] = effect_summary(v)
        av = get1(b, "available")
        if isinstance(av, list):
            ctl = control_lists(av)
            if ctl:
                row["controls"] = ctl
        targets = get1(b, "target_trigger") or get1(b, "targets")
        if targets is not None:
            row["targets"] = render(targets, 160) if isinstance(targets, list) else targets
        return row

    def find_decision(self, did):
        for d in self.decision_index():
            if d["id"] == did:
                return d
        return None

    # ------------------------------------------------------------ victory points and surrender
    def vp(self, tag=None, war_support=None):
        """VP list, total, capital, and the surrender threshold: NCountry.BASE_SURRENDER_LIMIT plus
        the surrender_limit modifiers of the tag's ideas (applied when held at the start, possible
        otherwise), needed VP = ceil(limit x total). `war_support` rows give the limit under the
        low-war-support modifier at the start war support, at zero and at the war_support argument,
        and say which row holds (derive.surrender)."""
        tag = tag or self.tag
        states = self.states()
        rows = []
        for sid in self.owned_states(tag):
            st = states[sid]
            for pid, val in sorted(st["vps"].items(), key=lambda kv: -kv[1]):
                rows.append({"province": pid, "name": self.province_name(pid), "state": sid,
                             "state_name": st["name"], "vp": val, "src": st["vp_src"].get(pid)})
        total = sum(r["vp"] for r in rows)
        base, base_fact = self.define("BASE_SURRENDER_LIMIT", 0.8)
        mn, mn_fact = self.define("MIN_SURRENDER_LIMIT", 0.2)
        held = {f["v"] for f in self.country(tag)["ideas"]}
        mods = []
        for iid in self._tag_ideas(tag) | held:
            val = self.modifier_value(iid, "surrender_limit")
            if val is None:
                continue
            i = self.ideas()[iid]
            mods.append({"idea": iid, "name": self.name(iid), "value": val, "held_at_start": iid in held,
                         "src": self.src(i["path"], i["line"]), "mod": self.content.source(i["path"])})
        mods.sort(key=lambda m: (not m["held_at_start"], m["idea"]))
        # the start threshold is derive.surrender's (shared with the lint); the possible modifiers and
        # the sources are added here
        sur = derive.surrender(self.game, tag, self.ctx_for(tag), self.date, war_support)
        start_limit = sur["limit"]
        ws = dict(sur["war_support"])
        ws["src"] = self._static_modifier_src(ws["modifier"])
        hist = self.country(tag)["war_support"]
        ws["start_src"] = hist["src"] if hist else "DEFAULT_WAR_SUPPORT"

        def need(limit):
            return int(math.ceil(round(limit * total, 6)))

        thresholds = [{"limit": start_limit, "vp_needed": sur["vp_needed"], "with": "start ideas",
                       "d": "derive.surrender"}]
        for m in mods:
            if not m["held_at_start"]:
                lim = max(mn, start_limit + m["value"])
                thresholds.append({"limit": round(lim, 4), "vp_needed": need(lim), "with": m["idea"]})
        cap = self.country(tag)["capital"]
        return {"tag": tag, "provinces": rows, "total": total, "capital": cap["v"] if cap else None,
                "base_limit": base_fact, "min_limit": mn_fact, "modifiers": mods, "thresholds": thresholds,
                "war_support": ws,
                "d": "vp_needed = ceil(limit x total VP of owned states at start); limit = BASE_SURRENDER_LIMIT "
                     "+ surrender_limit of held ideas, floored at MIN_SURRENDER_LIMIT; war support below "
                     f"{derive.WAR_SUPPORT_SPLIT:g} adds the {ws['modifier']} surrender_limit in proportion to the "
                     "gap (all of it at zero), so the limit moves with the live war support",
                "fair": "public"}

    def _static_modifier_src(self, name):
        """file:line of a static modifier block in common/modifiers, or None."""
        for p in reversed(list(self.game.files("common/modifiers"))):
            items = parse_file(p)
            for i, (k, v) in enumerate(items):
                if k == name and isinstance(v, list):
                    return self.src(p, line_of(items, i))
        return None

    @staticmethod
    def surrender_lines(v):
        """Text rows of the war-support part of vp(): the limit with and without the low-war-support
        modifier, and which one holds."""
        ws = v.get("war_support") or {}
        if not ws.get("full_malus"):
            return []
        pct = lambda x: f"{x * 100:g}%"  # noqa: E731
        mark = lambda key: "  <- applies" if ws["applies"] == key else ""  # noqa: E731
        top = v["thresholds"][0]
        out = [f"  war support: {pct(ws['start'])} at the start [{ws['start_src']}]; below {pct(ws['split'])} "
               f"{ws['modifier']} adds surrender_limit {ws['full_malus']:g} x (gap below {pct(ws['split'])}) "
               f"[{ws['src']}]",
               f"    war support {pct(ws['split'])} or more: limit {top['limit']} -> {top['vp_needed']} VP "
               f"(the most it can need: plan for this){mark('limit')}"]
        rows = [("at_start", "the start value")] + ([("at", "asked")] if "at" in ws else []) + [("at_zero", "the floor")]
        for key, label in rows:
            r = ws[key]
            if key == "at_start" and not r["malus"]:
                continue
            out.append(f"    war support {pct(r['war_support'])} ({label}): limit {r['limit']} -> {r['vp_needed']} VP "
                       f"(modifier {r['malus']:+g}){mark(key)}")
        out.append("    the live war support moves with world tension, the war, casualties, bombing and convoy "
                   "losses: read it in the game and pass --war-support")
        return out

    def _tag_ideas(self, tag):
        """Ideas that belong to a tag: key prefix TAG_, or defined in a file named after the country."""
        name = self.country_name(tag).lower()
        out = set()
        for iid, i in self.ideas().items():
            base = os.path.basename(i["path"]).lower()
            if iid.startswith(tag + "_") or (name and base.startswith(name.split()[0][:5]) and len(name) > 3):
                out.add(iid)
        return out

    # ------------------------------------------------------------ terrain and units
    def terrains(self):
        """{terrain: {combat_width, combat_support_width, movement_cost, attrition, attack, ...,
        is_water, src, width_src}} from common/terrain `categories`."""

        def load():
            out = {}
            keys = ("combat_width", "combat_support_width", "movement_cost", "attrition", "supply_flow_penalty_factor",
                    "truck_attrition_factor", "sickness_chance", "enemy_army_bonus_air_superiority_factor")
            for p in self.game.files("common/terrain"):
                for k, v in parse_file(p):
                    if k != "categories" or not isinstance(v, list):
                        continue
                    for j, (name, body) in enumerate(v):
                        if not name or not isinstance(body, list):
                            continue
                        row = {"id": name, "is_water": get1(body, "is_water") == "yes" or
                               get1(body, "naval_terrain") == "yes",
                               "src": self.src(p, line_of(v, j)), "mod": self.content.source(p)}
                        for idx, (a, b) in enumerate(body):
                            if a in keys and num(b) is not None:
                                row[a] = num(b)
                                if a == "combat_width":
                                    row["width_src"] = self.src(p, line_of(body, idx))
                        units = get1(body, "units")
                        if isinstance(units, list) and num(get1(units, "attack")) is not None:
                            row["attack"] = num(get1(units, "attack"))
                        out[name] = row
            return out

        return self.cached("terrains", load)

    def unit_unlocks(self):
        """{sub-unit: [{"tech", "year", "src"}]} from the technologies' enable_subunits."""

        def load():
            out = {}
            for p in self.game.files("common/technologies"):
                for k, v in parse_file(p):
                    if k != "technologies" or not isinstance(v, list):
                        continue
                    for j, (name, body) in enumerate(v):
                        if not name or not isinstance(body, list) or name.startswith("@"):
                            continue
                        for en in get(body, "enable_subunits"):
                            for u in bare(en):
                                out.setdefault(u, []).append({"tech": name, "year": num(get1(body, "start_year"), 0),
                                                              "src": self.src(p, line_of(v, j))})
            return out

        return self.cached("unitunlocks", load)

    def sub_units(self):
        """{sub-unit: row} from common/units: group, type, categories, combat_width, manpower,
        training_time, need (equipment per battalion), essential, the template row it may sit in
        (battalion | support | regimental support | divisional support | hq), allowed_battalion_groups,
        special-forces flag, own stats and terrain modifiers, the techs that enable it."""

        def load():
            unlock = self.unit_unlocks()
            terr = self.terrains()
            out = {}
            for p in self.game.files("common/units"):
                for k, v in parse_file(p):
                    if k != "sub_units" or not isinstance(v, list):
                        continue
                    for j, (name, b) in enumerate(v):
                        if not name or not isinstance(b, list):
                            continue
                        need = get1(b, "need")
                        cats = bare(get1(b, "categories", []))
                        group = get1(b, "group")
                        typ = get1(b, "type")
                        out[name] = {
                            "id": name, "name": self.loc.get(name) or name, "abbreviation": get1(b, "abbreviation"),
                            "group": group, "type": bare(typ) if isinstance(typ, list) else ([typ] if typ else []),
                            "categories": cats, "combat_width": num(get1(b, "combat_width"), 0),
                            "manpower": num(get1(b, "manpower"), 0), "training_time": num(get1(b, "training_time")),
                            "need": {a: num(c, 0) for a, c in need if a} if isinstance(need, list) else {},
                            "essential": bare(get1(b, "essential", [])),
                            "active": get1(b, "active") == "yes",
                            "special_forces": get1(b, "special_forces") == "yes" or "category_special_forces" in cats,
                            "can_be_parachuted": get1(b, "can_be_parachuted") == "yes",
                            "allowed_battalion_groups": bare(get1(b, "allowed_battalion_groups", [])),
                            "row": unit_row(name, b, group, p),
                            "stats": {s: num(get1(b, s)) for s in UNIT_STATS if num(get1(b, s)) is not None},
                            "terrain": {t: {a: num(c) for a, c in tb if a and num(c) is not None}
                                        for t, tb in b if t in terr and isinstance(tb, list)},
                            "unlocked_by": unlock.get(name, []),
                            "src": self.src(p, line_of(v, j)), "mod": self.content.source(p)}
            return out

        return self.cached("subunits", load)

    # ------------------------------------------------------------ experience and technology level
    def experience(self, tag=None):
        """Experience at the start per branch: history army/air/navy_experience (none set means 0 [?]),
        the stored maximum (defines), daily gain and factors from the starting spirits."""
        tag = tag or self.tag
        c = self.country(tag)
        start = {}
        for b in ("army", "air", "navy"):
            f = c["experience"].get(b)
            start[b] = f if f else {"v": 0, "src": c["history_file"], "mod": None,
                                    "d": "no army/air/navy_experience effect in the history up to the start; "
                                         "0 assumed [?]"}
        caps = {}
        for b in ("army", "air", "navy"):
            v, f = self.define(f"MAX_{b.upper()}_EXPERIENCE")
            if f:
                caps[b] = f
        mods = []
        for f in c["ideas"]:
            i = self.ideas().get(f["v"])
            if not i:
                continue
            for a, b in i["modifier"]:
                if a and a.startswith("experience_gain_") and num(b) is not None:
                    branch = next((x for x in ("army", "air", "navy") if x in a), "?")
                    mods.append({"idea": f["v"], "key": a, "branch": branch, "value": num(b),
                                 "daily": a in XP_MODIFIERS, "src": self.src(i["path"], i["line"])})
        return {"tag": tag, "start": start, "max": caps, "spirits": mods}

    def tech_level(self, tag):
        """Aggregate technology level at the start (FAIR aggregate, as the diplomacy view shows it):
        per class (anti_tank, armour, anti_air, air) whether any equipment of the class is enabled by
        the tag's start technologies, the newest start_year among them and the kinds reached
        (equipment types or archetypes); no tech list."""
        done = set(self.country(tag)["techs"])
        meta = derive.tech_meta(self.game)
        types = derive.equipment_types(self.game)
        arche = self.cached("eqarchetypes", lambda: _equipment_archetypes(self.game))
        out = {}
        for cls, want, kinds_of in TECH_CLASSES:
            hit, year, kinds = False, None, set()
            for t in done:
                m = meta.get(t)
                if not m:
                    continue
                for e in m["enables"]:
                    ts = types.get(e, set())
                    if not (ts & want) or (cls == "anti_air" and "screen_ship" in ts):
                        continue
                    hit = True
                    year = max(year or 0, m["year"] or 0)
                    kinds |= (ts & want) if kinds_of == "type" else {arche.get(e) or e}
            out[cls] = {"reached": hit, "year": year, "kinds": sorted(kinds)}
        return {"tag": tag, "classes": out, "fair": "private",
                "d": "start techs (history set_technology at the start) -> enable_equipments -> equipment type "
                     "(archetype inherited); year = newest start_year of those techs"}

    # ------------------------------------------------------------ occupation
    def occupation_start(self, tag=None):
        """{state: {"resistance_active", "resistance", "compliance", "garrison", "srcs"}} for the tag's
        states at the start that the files give occupation settings: state history first, then the
        owner's country history (`<state> = { ... }` scopes and `every_owned_state` with a static limit)."""
        tag = tag or self.tag
        states = self.states()
        own = set(self.owned_states(tag))
        out = {}
        for sid in sorted(own):
            occ = dict(states[sid].get("occupation") or {})
            if occ:
                src = occ.pop("src", None)
                out[sid] = dict(occ, srcs=[src] if src else [])
        for sc in self.country(tag)["state_scopes"]:
            if sc["v"] == "every_owned_state":
                targets = [s for s in sorted(own) if state_limit(sc.get("limit"), states[s]) is not False]
            else:
                targets = [sc["v"]] if sc["v"] in own else []
            for s in targets:
                row = out.setdefault(s, {"srcs": []})
                row.update(sc["occupation"])
                row["srcs"].append(sc["src"])
        for s, row in out.items():
            row["core"] = tag in states[s]["cores"]
            row["name"] = states[s]["name"]
        return out

    def occupation_laws(self):
        """[{id, name, default, starting, main_fallback, fallback_law, visible, available, state_modifier,
        suppressed_state_modifier, gui_order, src}] from common/occupation_laws in load order."""

        def load():
            out = []
            for p in self.game.files("common/occupation_laws"):
                items = parse_file(p)
                for i, (k, v) in enumerate(items):
                    if not k or not isinstance(v, list):
                        continue
                    row = {"id": k, "name": self.loc.get(k) or k, "src": self.src(p, line_of(items, i)),
                           "mod": self.content.source(p), "gui_order": num(get1(v, "gui_order"), 0)}
                    for flag in ("default_law", "starting_law", "main_fallback_law", "missing_garrison_law"):
                        row[flag.replace("_law", "")] = get1(v, flag) == "yes"
                    row["fallback_law"] = get1(v, "fallback_law")
                    for key in ("visible", "available"):
                        b = get1(v, key)
                        if isinstance(b, list) and b:
                            row[key] = render(b, 200)
                    for key in ("state_modifier", "suppressed_state_modifier"):
                        b = get1(v, key)
                        row[key] = {a: num(c, c) for a, c in b if a} if isinstance(b, list) else {}
                    out.append(row)
            return out

        return self.cached("occupationlaws", load)

    def resistance_modifiers(self):
        """[{id, type, threshold, margin, state_modifier, visible, enabled, src}] from
        common/resistance_compliance_modifiers, sorted by type and threshold."""

        def load():
            out = []
            for p in self.game.files("common/resistance_compliance_modifiers"):
                items = parse_file(p)
                for i, (k, v) in enumerate(items):
                    if not k or not isinstance(v, list):
                        continue
                    sm = get1(v, "state_modifier")
                    row = {"id": k, "type": get1(v, "type"), "threshold": num(get1(v, "threshold")),
                           "margin": num(get1(v, "margin")),
                           "state_modifier": {a: num(c, c) for a, c in sm if a} if isinstance(sm, list) else {},
                           "src": self.src(p, line_of(items, i)), "mod": self.content.source(p)}
                    for key in ("visible", "enabled", "on_enable"):
                        b = get1(v, key)
                        if isinstance(b, list) and b:
                            row[key] = render(b, 160) if key != "on_enable" else effect_summary(b, 6)
                    out.append(row)
            out.sort(key=lambda r: (r["type"] or "", r["threshold"] or 0, r["id"]))
            return out

        return self.cached("resistancemods", load)

    def resistance_activities(self):
        """[{id, max_amount, duration, state_modifier, effect, src}] from common/resistance_activity."""

        def load():
            out = []
            for p in self.game.files("common/resistance_activity"):
                items = parse_file(p)
                for i, (k, v) in enumerate(items):
                    if not k or not isinstance(v, list):
                        continue
                    sm = get1(v, "state_modifier")
                    eff = get1(v, "effect")
                    out.append({"id": k, "max_amount": num(get1(v, "max_amount")), "duration": num(get1(v, "duration")),
                                "state_modifier": {a: num(c, c) for a, c in sm if a} if isinstance(sm, list) else {},
                                "effect": effect_summary(eff, 4) if isinstance(eff, list) else [],
                                "src": self.src(p, line_of(items, i)), "mod": self.content.source(p)})
            return out

        return self.cached("resistanceacts", load)

    # ------------------------------------------------------------ map rules, regions, weather, power balance
    def adjacency_rules(self):
        """{rule name: {contested, enemy, friend, neutral: {army, navy, submarine, trade}, required_provinces,
        is_disabled, src}} from map/adjacency_rules.txt."""

        def load():
            out = {}
            p = self.game.file("map/adjacency_rules.txt")
            if not p:
                return out
            items = parse_file(p)
            for i, (k, v) in enumerate(items):
                if k != "adjacency_rule" or not isinstance(v, list) or not get1(v, "name"):
                    continue
                row = {"name": get1(v, "name"), "src": self.src(p, line_of(items, i)), "mod": self.content.source(p),
                       "required_provinces": [int(x) for x in bare(get1(v, "required_provinces", [])) if x.isdigit()]}
                for rel in ("contested", "enemy", "friend", "neutral"):
                    b = get1(v, rel)
                    row[rel] = {a: c == "yes" for a, c in b if a} if isinstance(b, list) else {}
                dis = get1(v, "is_disabled")
                if isinstance(dis, list):
                    row["is_disabled"] = render([(a, b) for a, b in dis if a != "tooltip"], 220)
                for rel in ("is_enemy", "is_friend", "is_neutral"):
                    b = get1(v, rel)
                    if isinstance(b, list):
                        row[rel] = render(b, 160)
                out[row["name"]] = row
            return out

        return self.cached("adjrules", load)

    def strategic_regions(self):
        """{region id: {id, name, provinces, periods: [{start: (day, month), end, temperature: [lo, hi],
        weights...}], src}} from map/strategicregions (months and days 1-based)."""

        def load():
            out = {}
            for p in self.game.files("map/strategicregions"):
                items = parse_file(p)
                for i, (k, v) in enumerate(items):
                    if k != "strategic_region" or not isinstance(v, list):
                        continue
                    rid = num(get1(v, "id"))
                    if not isinstance(rid, int):
                        continue
                    periods = []
                    w = get1(v, "weather")
                    for per in get(w, "period") if isinstance(w, list) else []:
                        if not isinstance(per, list):
                            continue
                        between = [_day_month(x) for x in bare(get1(per, "between", []))]
                        temp = [num(x) for x in bare(get1(per, "temperature", []))]
                        row = {"start": between[0] if between else None, "end": between[1] if len(between) > 1 else None,
                               "temperature": temp}
                        for a, b in per:
                            if a and not isinstance(b, list) and num(b) is not None:
                                row[a] = float(num(b))
                        periods.append(row)
                    out[rid] = {"id": rid, "name": self.loc.get(get1(v, "name") or "") or get1(v, "name"),
                                "provinces": [int(x) for x in bare(get1(v, "provinces", [])) if x.isdigit()],
                                "periods": periods, "src": self.src(p, line_of(items, i)),
                                "mod": self.content.source(p)}
            return out

        return self.cached("regions", load)

    def weather_settings(self):
        """common/weather.txt: {"settings": {...}, "terrain_modifiers": {phenomenon: {terrain: value}}, "src"}."""

        def load():
            p = self.game.file("common/weather.txt")
            if not p:
                return {"settings": {}, "terrain_modifiers": {}, "src": None}
            items = parse_file(p)

            def flat(block):
                out = {}
                for a, b in block:
                    if not a:
                        continue
                    if isinstance(b, list):
                        vals = [num(x) for x in bare(b)]
                        if vals and all(x is not None for x in vals):
                            out[a] = vals if len(vals) > 1 else vals[0]
                        else:
                            out[a] = flat(b)
                    elif num(b) is not None:
                        out[a] = num(b)
                return out

            st = get1(items, "settings")
            tm = get1(items, "terrain_modifiers")
            mods = {}
            for ph, body in (tm if isinstance(tm, list) else []):
                if isinstance(body, list):
                    mods[ph] = {get1(t, "type"): num(get1(t, "value")) for a, t in body if a == "terrain" and isinstance(t, list)}
            return {"settings": flat(st) if isinstance(st, list) else {}, "terrain_modifiers": mods,
                    "src": self.src(p)}

        return self.cached("weathersettings", load)

    def power_balances(self):
        """{bop id: {initial_value, left_side, right_side, decision_category, ranges: [{id, side, min, max,
        modifier, on_activate}], src}} from common/bop."""

        def load():
            out = {}
            for p in self.game.files("common/bop"):
                items = parse_file(p)
                for i, (k, v) in enumerate(items):
                    if not k or not isinstance(v, list) or get1(v, "initial_value") is None:
                        continue
                    row = {"id": k, "initial_value": num(get1(v, "initial_value")), "left_side": get1(v, "left_side"),
                           "right_side": get1(v, "right_side"), "decision_category": get1(v, "decision_category"),
                           "ranges": [], "src": self.src(p, line_of(items, i)), "mod": self.content.source(p)}

                    def ranges(block, side):
                        for j, (a, b) in enumerate(block):
                            if a == "range" and isinstance(b, list):
                                mod = get1(b, "modifier")
                                act = get1(b, "on_activate")
                                row["ranges"].append({
                                    "id": get1(b, "id"), "side": side, "min": num(get1(b, "min")), "max": num(get1(b, "max")),
                                    "modifier": {x: num(y, y) for x, y in mod if x and not isinstance(y, list)
                                                 and x != "custom_modifier_tooltip"} if isinstance(mod, list) else {},
                                    "on_activate": effect_summary(act, 6) if isinstance(act, list) else [],
                                    "src": self.src(p, line_of(block, j))})
                            elif a == "side" and isinstance(b, list):
                                ranges(b, get1(b, "id"))

                    ranges(v, None)
                    row["ranges"].sort(key=lambda r: (r["min"] if r["min"] is not None else 0))
                    out[k] = row
            return out

        return self.cached("bops", load)

    # ------------------------------------------------------------ static triggers, constants, top-level blocks
    def scripted_triggers(self):
        """{name: block} of common/scripted_triggers."""
        return self.cached("scriptedtriggers", lambda: {k: v for p in self.game.files("common/scripted_triggers")
                                                       for k, v in parse_file(p) if k and isinstance(v, list)})

    def static(self, items, tag=None, aliases=()):
        """Static value of a trigger block for the tag (scripted triggers `name = yes/no` expanded three levels;
        `aliases` = scope blocks that are the country itself here): True, False, or None when it needs the
        running game. An absent block (None) counts as True."""
        if items is None:
            return True
        if not isinstance(items, list):
            return None
        return trigger(expand_scripted(items, self.scripted_triggers(), 3, aliases), self.ctx_for(tag or self.tag))

    def script_constants(self):
        """{"sp_time.prototype.short": 45, ...} of common/script_constants (schema blocks skipped)."""

        def load():
            out = {}

            def walk(block, prefix):
                for k, v in block:
                    if not k or k == "schema":
                        continue
                    if isinstance(v, list):
                        walk(v, f"{prefix}{k}.")
                    elif num(v) is not None:
                        out[prefix + k] = num(v)

            for p in self.game.files("common/script_constants"):
                walk(parse_file(p), "")
            return out

        return self.cached("scriptconstants", load)

    def blocks(self, folder, recursive=False, inside=None):
        """[(name, block, path, line)] of the top-level blocks of a content folder (or of the blocks inside
        each top-level `inside` wrapper), in load order; a later definition of a name replaces an earlier one."""
        out = {}
        for p in self.game.files(folder, recursive=recursive):
            items = parse_file(p)
            for i, (k, v) in enumerate(items):
                if not k or not isinstance(v, list) or k.startswith("@"):
                    continue
                if inside is None:
                    out[k] = (k, v, p, line_of(items, i))
                elif k == inside:
                    for j, (n, b) in enumerate(v):
                        if n and isinstance(b, list) and n != "limit":
                            out[n] = (n, b, p, line_of(v, j))
        return list(out.values())

    def foreign(self, ident, tag=None):
        """True when an id starts with another country's tag ("CHI_force_attack" for ITA): content of that country."""
        m = re.match(r"^([A-Z][A-Z0-9]{2})_", ident or "")
        return bool(m and m.group(1) != (tag or self.tag) and m.group(1) in self.game.country_tags())

    def gate_text(self, items, tag=None, limit=200, aliases=()):
        """True / False when a trigger block is static for the tag, else its text with the static parts decided
        (`aliases` scope blocks read as the country itself; a lone FROM / ROOT / owner wrapper is unwrapped)."""
        st = self.static(items, tag, aliases) if isinstance(items, list) else True
        if st is not None:
            return st
        while len(items) == 1 and isinstance(items[0][1], list) and (items[0][0] or "").upper() in ("FROM", "ROOT", "OWNER"):
            items = items[0][1]
        flat = Block(getattr(items, "line", 1))

        def inline(block):  # top-level AND blocks are the implicit AND of the block itself
            for i, (k, v) in enumerate(block):
                if (k or "").upper() == "AND" and isinstance(v, list):
                    inline(v)
                else:
                    flat.add((k, v), line_of(block, i), op_of(block, i))

        inline(reduce_trigger(expand_scripted(items, {}, 0, aliases), self.ctx_for(tag or self.tag)))
        return render(flat, limit) if flat else True

    def defines_rows(self, keys):
        """{key: {"v", "src", "mod", "key"}} of the defines that exist among `keys` (short names)."""
        out = {}
        for k in keys:
            _, f = self.define(k)
            if f:
                out[k] = f
        return out

    # ------------------------------------------------------------ designers (F-12)
    def equipment_defs(self):
        """{equipment: {id, archetype, parent, is_archetype, types, year, slots (block | "inherit" | None),
        upgrades, variant_name, path, line}} of common/units/equipment, plus the families duplicate_archetypes
        generates (archetype X duplicated as Y: Y and Y<suffix> for every X<suffix> of X)."""

        def load():
            raw, dups = {}, []
            for p in self.game.files("common/units/equipment"):
                for k, v in parse_file(p):
                    if k not in ("equipments", "duplicate_archetypes") or not isinstance(v, list):
                        continue
                    for j, (name, b) in enumerate(v):
                        if not name or not isinstance(b, list):
                            continue
                        t, ms, ups = get1(b, "type"), get1(b, "module_slots"), get1(b, "upgrades")
                        row = {"id": name, "archetype": get1(b, "archetype"), "parent": get1(b, "parent"),
                               "is_archetype": get1(b, "is_archetype") == "yes",
                               "types": bare(t) if isinstance(t, list) else ([t] if t else []),
                               "year": num(get1(b, "year")),
                               "slots": ms if isinstance(ms, list) else ("inherit" if ms == "inherit" else None),
                               "upgrades": bare(ups) if isinstance(ups, list) else [],
                               "variant_name": get1(b, "derived_variant_name"), "path": p, "line": line_of(v, j)}
                        if k == "duplicate_archetypes":
                            dups.append(row)
                        else:
                            raw[name] = row
            for d in dups:
                base = d["archetype"]
                if base not in raw:
                    continue
                raw[d["id"]] = dict(raw[base], id=d["id"], is_archetype=True, archetype=None, parent=None,
                                    types=d["types"] or raw[base]["types"], path=d["path"], line=d["line"],
                                    duplicate_of=base)
                for name, r in list(raw.items()):
                    if r.get("archetype") == base and name.startswith(base) and not r.get("duplicate_of"):
                        par = r["parent"]
                        nid = d["id"] + name[len(base):]
                        raw.setdefault(nid, dict(r, id=nid, archetype=d["id"], types=[], duplicate_of=name,
                                                 parent=d["id"] + par[len(base):] if par and par.startswith(base)
                                                 else par))
            return raw

        return self.cached("equipdefs", load)

    def _eq_chain(self, e):
        defs, out, cur = self.equipment_defs(), [], e
        while cur in defs and cur not in out:
            out.append(cur)
            cur = defs[cur]["parent"] or defs[cur]["archetype"]
        return out

    def eq_slots(self, e):
        """[{"slot", "required", "categories"}] of an equipment (own module_slots, else inherited), or None."""
        return self.cached("eqslots:" + e, lambda: self._eq_slots(e))

    def _eq_slots(self, e):
        defs = self.equipment_defs()
        chain = self._eq_chain(e)
        for i, x in enumerate(chain):
            s = defs[x]["slots"]
            if not isinstance(s, list):
                continue
            up = None
            out = []
            for n, b in s:
                if not n:
                    continue
                if isinstance(b, list):
                    out.append({"slot": n, "required": get1(b, "required") == "yes",
                                "categories": bare(get1(b, "allowed_module_categories", []))})
                elif b == "inherit":  # the same slot of the parent
                    if up is None:
                        up = {r["slot"]: r for r in (self.eq_slots(chain[i + 1]) or [])} if i + 1 < len(chain) else {}
                    if n in up:
                        out.append(dict(up[n]))
                elif isinstance(b, str):  # a copy of another slot of this equipment
                    src = next((r for r in out if r["slot"] == b), None)
                    if src:
                        out.append(dict(src, slot=n))
            return out
        return None

    def eq_kind(self, e):
        """Designer of an equipment from its type: tank (armor), ship, plane, else the type."""
        defs = self.equipment_defs()
        types = next((defs[x]["types"] for x in self._eq_chain(e) if defs[x]["types"]), [])
        for t in types:
            if t in DESIGN_KINDS:
                return DESIGN_KINDS[t]
        return "plane" if set(types) & PLANE_TYPES else (types[0] if types else None)

    def equipment_modules(self):
        """{module: {id, name, category, loaded (static `limit` of its equipment_modules block), xp_cost,
        dismantle_cost_ic, add_stats, multiply_stats, build_cost_resources, can_convert_from,
        allow_equipment_type, forbid_equipment_type, src, mod}} of common/units/equipment/modules."""

        def load():
            out = {}
            for p in self.game.files("common/units/equipment/modules"):
                for k, v in parse_file(p):
                    if k != "equipment_modules" or not isinstance(v, list):
                        continue
                    lim = get1(v, "limit")
                    loaded = trigger(lim, self.ctx) if isinstance(lim, list) else True
                    for j, (name, b) in enumerate(v):
                        if not name or name == "limit" or not isinstance(b, list):
                            continue
                        out[name] = {"id": name, "name": self.loc.get(name) or name, "category": get1(b, "category"),
                                     "loaded": loaded, "xp_cost": num(get1(b, "xp_cost")),
                                     "dismantle_cost_ic": num(get1(b, "dismantle_cost_ic")),
                                     "add_stats": _numbers(get1(b, "add_stats")),
                                     "multiply_stats": _numbers(get1(b, "multiply_stats")),
                                     "build_cost_resources": _numbers(get1(b, "build_cost_resources")),
                                     "can_convert_from": [x for c in get(b, "can_convert_from") if isinstance(c, list)
                                                          for x in (get1(c, "module"), get1(c, "module_category"))
                                                          if x],
                                     "allow_equipment_type": [x for c in get(b, "allow_equipment_type")
                                                              for x in (bare(c) if isinstance(c, list) else [c])],
                                     "forbid_equipment_type": [x for key in ("forbid_equipment_type",
                                                                             "forbid_equipment_type_exact_match")
                                                               for c in get(b, key)
                                                               for x in (bare(c) if isinstance(c, list) else [c])],
                                     "src": self.src(p, line_of(v, j)), "mod": self.content.source(p)}
            return out

        return self.cached("eqmodules", load)

    def module_unlocks(self):
        """{module: [{"tech", "year", "src"}]} from the technologies' enable_equipment_modules."""

        def load():
            out = {}
            for name, body, p, ln in self.blocks("common/technologies", inside="technologies"):
                if name.startswith("@"):
                    continue
                for en in get(body, "enable_equipment_modules"):
                    for m in bare(en):
                        out.setdefault(m, []).append({"tech": name, "year": num(get1(body, "start_year"), 0),
                                                      "src": self.src(p, ln)})
            return out

        return self.cached("moduleunlocks", load)

    def equipment_upgrades(self):
        """{upgrade: {max_level, cost (land | air | naval), cost_by_level, src}} of common/units/equipment/upgrades."""

        def load():
            out = {}
            for name, b, p, ln in self.blocks("common/units/equipment/upgrades", inside="upgrades"):
                lc = get1(b, "linear_cost")
                out[name] = {"max_level": num(get1(b, "max_level")), "cost": get1(b, "cost"),
                             "cost_by_level": num(get1(lc, "cost_by_level")) if isinstance(lc, list) else None,
                             "src": self.src(p, ln)}
            return out

        return self.cached("equpgrades", load)

    def designs(self, tag=None, kind=None):
        """Designer facts of a tag at the date (DESIGNS_METHOD): per designer (tank, plane, ship) whether its
        modules load with the active DLCs, the chassis/hulls/airframes and modules enabled by the start
        technologies (with XP cost, dismantle cost, stats, resources), the next ones by research year, the
        start variants (history create_equipment_variant), the branch XP at start and the XP cost defines."""
        tag = tag or self.tag
        done = set(self.country(tag)["techs"])
        year = int(self.date.split(".")[0])
        defs, mods, munlock = self.equipment_defs(), self.equipment_modules(), self.module_unlocks()
        eunlock, meta, ups = derive.equipment_unlocks(self.game), derive.tech_meta(self.game), self.equipment_upgrades()
        catkinds = {}
        for e in defs:
            for s in self.eq_slots(e) or []:
                for c in s["categories"]:
                    catkinds.setdefault(c, set()).add(self.eq_kind(e))
        xp = self.experience(tag)["start"]
        out = {"tag": tag, "date": self.date, "kinds": {}, "fair": "public", "d": DESIGNS_METHOD,
               "costs": self.defines_rows(DESIGN_DEFINES)}
        for e, r in sorted(defs.items()):
            if r["is_archetype"] or self.eq_slots(e) is None:
                continue
            k = self.eq_kind(e)
            if kind and k != kind:
                continue
            kd = out["kinds"].setdefault(k, {"bases": [], "next_bases": [], "modules": [], "next_modules": [],
                                             "variants": [], "xp": xp.get(XP_BRANCH.get(k, "army"))})
            techs = sorted(eunlock.get(e, set()))
            have = [t for t in techs if t in done]
            arch = self._eq_chain(e)[-1]
            row = {"id": e, "name": self.loc.get(r["variant_name"] or "") or self.loc.get(e) or e, "archetype": arch,
                   "family": defs[arch].get("duplicate_of") or arch,
                   "year": r["year"], "techs": have or techs, "src": self.src(r["path"], r["line"]),
                   "upgrades": [dict(ups[u], id=u) for u in r["upgrades"] if u in ups]}
            if have:
                row["slots"] = self.eq_slots(e)
                kd["bases"].append(row)
            elif techs and min((meta.get(t) or {}).get("year") or 9999 for t in techs) <= year + 2:
                row["year"] = min((meta.get(t) or {}).get("year") or 9999 for t in techs)
                kd["next_bases"].append(row)
        for m, r in sorted(mods.items()):
            ks = sorted(x for x in catkinds.get(r["category"], set()) if x)
            if not ks or (kind and kind not in ks) or r["loaded"] is False:  # a module whose DLC is off does not exist
                continue
            techs = munlock.get(m, [])
            have = [t["tech"] for t in techs if t["tech"] in done]
            row = dict(r, techs=have or [t["tech"] for t in techs])
            for k in ks:
                kd = out["kinds"].get(k)
                if kd is None:
                    continue
                if have:
                    kd["modules"].append(row)
                elif techs and min(t["year"] or 9999 for t in techs) <= year + 2:
                    kd["next_modules"].append(dict(row, year=min(t["year"] or 9999 for t in techs)))
        for k, kd in out["kinds"].items():
            kd["loaded"] = any(m["loaded"] is not False for m in mods.values()
                               if k in catkinds.get(m["category"], set()))
            kd["next_bases"].sort(key=lambda x: (x["year"], x["id"]))
            kd["next_modules"].sort(key=lambda x: (x["year"], x["id"]))
        for v in self.country(tag)["variants"]:
            k = self.eq_kind(v["type"]) if v.get("type") in defs else None
            if k in out["kinds"]:
                out["kinds"][k]["variants"].append(v)
        return out

    # ------------------------------------------------------------ military-industrial organisations (F-13)
    def mio_organisations(self):
        """{mio: resolved row} of common/military_industrial_organization/organizations with `include`
        applied (the included MIO's fields and traits, then delete_included_values, own fields, remove_trait,
        override_trait, add_trait; trait rows: token, name, parents, exclusive, position, bonuses)."""

        def load():
            raw = {n: (b, p, ln) for n, b, p, ln in self.blocks("common/military_industrial_organization/organizations")}
            done = {}

            def trait_row(b):
                row = {"token": get1(b, "token"), "name": None, "any_parent": bare(get1(b, "any_parent", [])),
                       "all_parents": bare(get1(b, "all_parents", [])),
                       "mutually_exclusive": bare(get1(b, "mutually_exclusive", [])),
                       "special": get1(b, "special_trait_background") == "yes"}
                pos = get1(b, "position")
                row["position"] = [num(get1(pos, "x")), num(get1(pos, "y"))] if isinstance(pos, list) else None
                for key in MIO_BONUS_KEYS:
                    if isinstance(get1(b, key), list):
                        row[key] = _numbers(get1(b, key), deep=True)
                for key in ("available", "visible"):
                    if isinstance(get1(b, key), list) and get1(b, key):
                        row[key] = render(get1(b, key), 160)
                row["limit_to_equipment_type"] = bare(get1(b, "limit_to_equipment_type", []))
                n = get1(b, "name") or row["token"]
                row["name"] = self.loc.get(n) or n
                return row

            def resolve(mid, depth=0):
                if mid in done:
                    return done[mid]
                b, p, ln = raw[mid]
                inc = get1(b, "include")
                base = resolve(inc, depth + 1) if inc in raw and inc != mid and depth < 5 else None
                row = {"id": mid, "include": inc, "src": self.src(p, ln), "mod": self.content.source(p)}
                for key in ("allowed", "visible", "available", "equipment_type", "research_categories",
                            "research_bonus", "task_capacity", "name", "initial_trait"):
                    row[key] = base.get(key) if base else None
                traits = [dict(t) for t in base["traits"]] if base else []
                for key in bare(get1(b, "delete_included_values", [])):
                    row[key] = None
                for key in ("allowed", "visible", "available"):
                    if isinstance(get1(b, key), list):
                        row[key] = get1(b, key)
                for key in ("equipment_type", "research_categories"):
                    if isinstance(get1(b, key), list):
                        row[key] = bare(get1(b, key))
                for key in ("research_bonus", "task_capacity", "name"):
                    if get1(b, key) is not None:
                        row[key] = num(get1(b, key), get1(b, key))
                if isinstance(get1(b, "initial_trait"), list):
                    it = trait_row(get1(b, "initial_trait"))
                    row["initial_trait"] = dict(row["initial_trait"] or {}, **{a: c for a, c in it.items() if c})
                gone = {x for r in get(b, "remove_trait") if isinstance(r, list) for x in bare(r)}
                traits = [t for t in traits if t["token"] not in gone]
                for key, val in b:
                    if key in ("trait", "add_trait") and isinstance(val, list):
                        traits.append(trait_row(val))
                    elif key == "override_trait" and isinstance(val, list):
                        tr = trait_row(val)
                        for t in traits:
                            if t["token"] == tr["token"]:
                                t.update({a: c for a, c in tr.items() if c not in (None, [], {}, False)})
                row["traits"] = traits
                own_name = get1(b, "name")  # the name is not inherited: the loc key defaults to the MIO token
                row["name"] = self.loc.get(own_name or mid) or own_name or mid
                done[mid] = row
                return row

            return {m: resolve(m) for m in raw}

        return self.cached("mios", load)

    def mio_policies(self):
        """[{id, name, allowed, available, cost, cooldown, bonuses, src}] of military_industrial_organization/policies
        (cost and cooldown default to DEFAULT_INITIAL_POLICY_ATTACH_COST / DEFAULT_INITIAL_ATTACH_POLICY_COOLDOWN)."""
        cost, _ = self.define("DEFAULT_INITIAL_POLICY_ATTACH_COST")
        cool, _ = self.define("DEFAULT_INITIAL_ATTACH_POLICY_COOLDOWN")
        out = []
        for n, b, p, ln in self.blocks("common/military_industrial_organization/policies"):
            out.append({"id": n, "name": self.loc.get(n) or n, "allowed": render(get1(b, "allowed", []), 160),
                        "available": render(get1(b, "available", []), 160),
                        "cost": num(get1(b, "cost"), cost), "cooldown": num(get1(b, "cooldown"), cool),
                        "bonuses": {key: _numbers(get1(b, key), deep=True) for key in MIO_BONUS_KEYS
                                    if isinstance(get1(b, key), list)},
                        "src": self.src(p, ln), "mod": self.content.source(p)})
        return out

    def mio(self, tag=None):
        """MIOs a tag may get (static `allowed` not false): equipment and research lines, initial trait, trait
        tree, visible/available text with the static parts decided; policies; the MIO defines."""
        tag = tag or self.tag
        rows = []
        for mid, r in sorted(self.mio_organisations().items()):
            allowed = self.static(r["allowed"], tag) if r["allowed"] is not None else False
            if allowed is False:
                continue
            row = {k: r[k] for k in ("id", "name", "include", "equipment_type", "research_categories", "research_bonus",
                                     "task_capacity", "initial_trait", "traits", "src", "mod")}
            row["allowed"] = allowed
            row["foreign"] = self.foreign(mid, tag)  # another country's MIO (invited / licensed)
            for key in ("visible", "available"):
                row[key] = self.gate_text(r[key], tag, 220, ("FROM", "owner", "OWNER"))
            if False in (row["visible"], row["available"]) and row["foreign"]:
                continue
            rows.append(row)
        rows.sort(key=lambda r: (r["foreign"], r["id"]))
        return {"tag": tag, "organisations": rows, "policies": self.mio_policies(), "fair": "public",
                "defines": self.defines_rows(MIO_DEFINES), "d": MIO_METHOD}

    # ------------------------------------------------------------ special projects (F-14)
    def special_projects(self):
        """{project: {id, name, specialization, tags, parent, allowed, available, breakthrough_cost, prototype_days,
        complexity {min, max}, resources, outputs {modules, equipments, techs}, src}} of common/special_projects/projects
        (sp_time / sp_complexity constants resolved from common/script_constants)."""

        def load():
            const = self.script_constants()
            out = {}
            for n, b, p, ln in self.blocks("common/special_projects/projects"):
                if get1(b, "specialization") is None and get1(b, "project_output") is None:
                    continue
                cx = get1(b, "complexity")
                pt = get1(b, "prototype_time")
                rc = get1(b, "resource_cost")
                outp = get1(b, "project_output") or []
                found = {"modules": set(), "equipments": set(), "techs": set()}

                def walk(block):
                    for k, v in block:
                        if k == "enable_equipment_modules" and isinstance(v, list):
                            found["modules"].update(bare(v))
                        elif k == "enable_equipments" and isinstance(v, list):
                            found["equipments"].update(bare(v))
                        elif k == "set_technology" and isinstance(v, list):
                            found["techs"].update(a for a, c in v if a and a != "popup")
                        elif k in ("TECH", "tech") and isinstance(v, str):
                            found["techs"].add(v)
                        elif isinstance(v, list):
                            walk(v)

                walk(outp if isinstance(outp, list) else [])
                cxv = {"min": const.get(f"{cx}.min"), "max": const.get(f"{cx}.max")} if isinstance(cx, str) \
                    and f"{cx}.min" in const else ({"min": num(cx), "max": num(cx)} if num(cx) is not None else None)
                out[n] = {"id": n, "name": self.loc.get(n) or n, "specialization": get1(b, "specialization"),
                          "tags": bare(get1(b, "project_tags")) if isinstance(get1(b, "project_tags"), list)
                          else [x for x in [get1(b, "project_tags")] if x],
                          "parent": [x for pb in get(b, "special_project_parent")
                                     for x in (bare(pb) if isinstance(pb, list) else [pb])],
                          "allowed": get1(b, "allowed"),
                          "available": get1(b, "available"), "breakthrough_cost": _numbers(get1(b, "breakthrough_cost")),
                          "prototype_days": const.get(pt, num(pt)) if isinstance(pt, str) else None,
                          "prototype_time": pt, "complexity": cxv, "complexity_key": cx,
                          "resources": _numbers(get1(rc, "resources")) if isinstance(rc, list) else {},
                          "outputs": {k: sorted(v) for k, v in found.items()},
                          "src": self.src(p, ln), "mod": self.content.source(p)}
            return out

        return self.cached("specialprojects", load)

    def facility_buildings(self):
        """{building: {specialization, coastal, cost, per_controlled_cost, src}} of common/buildings with tags facility."""

        def load():
            out = {}
            for n, b, p, ln in self.blocks("common/buildings", inside="buildings"):
                if "facility" in bare(get1(b, "tags", [])):
                    out[n] = {"specialization": bare(get1(b, "specialization", [])), "coastal": get1(b, "only_costal") == "yes",
                              "cost": num(get1(b, "base_cost")), "per_controlled_cost":
                              num(get1(b, "per_controlled_building_extra_cost")), "src": self.src(p, ln)}
            return out

        return self.cached("facilities", load)

    def projects(self, tag=None):
        """Special projects for a tag: projects whose static `allowed` is not false (available text with the static
        parts decided), facilities at the start in own states, scientists (characters with a scientist role),
        scientist traits, facility buildings and the NProject defines."""
        tag = tag or self.tag
        rows = []
        for pid, r in sorted(self.special_projects().items()):
            allowed = self.static(r["allowed"], tag)
            if allowed is False:
                continue
            row = {k: v for k, v in r.items() if k not in ("allowed", "available")}
            row["allowed"] = allowed
            row["available"] = self.gate_text(r["available"], tag, 200, ("FROM",))
            if row["available"] is False:
                continue
            rows.append(row)
        fac = self.facility_buildings()
        states = self.states()
        have = []
        for sid in self.owned_states(tag):
            st = states[sid]
            for b, n in st["buildings"].items():
                if b in fac and n:
                    have.append({"state": sid, "name": st["name"], "building": b, "level": n, "src": st["file"]})
            for prov, bs in st["province_buildings"].items():
                for b, n in bs.items():
                    if b in fac and n:
                        have.append({"state": sid, "name": st["name"], "province": prov, "building": b, "level": n,
                                     "src": st["file"]})
        sci = []
        for ch in self.characters(tag, "scientist"):
            for r in ch["roles"]:
                if r["role"] == "scientist":
                    sci.append({"id": ch["id"], "name": ch["name"], "skills": r.get("skills", {}),
                                "traits": r.get("traits", []), "visible": r.get("visible"), "src": ch["src"]})
        traits = {n: {"modifier": _numbers(get1(b, "modifier")), "src": self.src(p, ln)}
                  for n, b, p, ln in self.blocks("common/scientist_traits")}
        return {"tag": tag, "projects": rows, "facilities_at_start": have, "facility_buildings": fac,
                "scientists": sci, "scientist_traits": traits, "defines": self.defines_rows(PROJECT_DEFINES),
                "fair": "public", "d": PROJECT_METHOD}

    # ------------------------------------------------------------ diplomacy: world tension, ideology gates, war goals (F-15, F-16)
    def ideologies(self):
        """{ideology group: {types, rules, modifiers (hidden_modifier flattened), faction_modifiers,
        war_impact_on_world_tension, faction_impact_on_world_tension, can_collaborate, src}} of common/ideologies."""

        def load():
            out = {}
            for n, b, p, ln in self.blocks("common/ideologies", inside="ideologies"):
                types = get1(b, "types")
                out[n] = {"id": n, "name": self.loc.get(n) or n,
                          "types": [a for a, c in types if a] if isinstance(types, list) else [],
                          "rules": {a: c for a, c in (get1(b, "rules") or []) if a and isinstance(c, str)},
                          "modifiers": _numbers(get1(b, "modifiers"), deep=True),
                          "faction_modifiers": _numbers(get1(b, "faction_modifiers"), deep=True),
                          "src": self.src(p, ln), "mod": self.content.source(p)}
                for key in ("war_impact_on_world_tension", "faction_impact_on_world_tension"):
                    out[n][key] = num(get1(b, key))
                for key in ("can_collaborate", "can_host_government_in_exile"):
                    out[n][key] = get1(b, key) == "yes"
            return out

        return self.cached("ideologies", load)

    def wargoals(self):
        """{war goal: {generate_base_cost, generate_per_state_cost, take_states_limit, take_states_cost, expire,
        threat, allowed, available, src}} of common/wargoals."""

        def load():
            out = {}
            for n, b, p, ln in self.blocks("common/wargoals", inside="wargoal_types"):
                row = {"id": n, "name": self.loc.get(n) or n, "src": self.src(p, ln), "mod": self.content.source(p)}
                for key in ("generate_base_cost", "generate_per_state_cost", "take_states_limit", "take_states_cost",
                            "expire", "threat"):
                    row[key] = num(get1(b, key))
                for key in ("allowed", "available"):
                    row[key] = get1(b, key) if isinstance(get1(b, key), list) else None
                out[n] = row
            return out

        return self.cached("wargoals", load)

    def world_tension(self):
        """World tension the histories set up to the start date: add_named_threat / add_threat of every country
        history ({"named": [...], "total", "wars": wars at start with their war goal's threat, "d"})."""

        def load():
            named = []
            for p in self.game.files("history/countries"):
                try:
                    with open(p, encoding="utf-8-sig", errors="replace") as fh:
                        if "_threat" not in fh.read():
                            continue
                except OSError:
                    continue
                m = re.match(r"^([A-Z][A-Z0-9]{2})", os.path.basename(p))
                if m:
                    named += [dict(t, tag=m.group(1)) for t in self.country(m.group(1))["threats"]]
            goals = self.wargoals()
            wars = [dict(w, threat=(goals.get(w["type"]) or {}).get("threat")) for w in self.wars()]
            return {"named": named, "total": round(sum(t["v"] for t in named), 3), "wars": wars, "fair": "public",
                    "d": "sum of add_named_threat / add_threat in the country histories up to the start (percent "
                         "points); wars declared in the histories add tension when the game starts (war goal threat x "
                         "the TENSION_* defines, scaled by the TENSION_TIME_SCALE defines), not computed here [?]"}

        return self.cached("worldtension", load)

    def diplomacy(self, tag=None):
        """Diplomatic gates of a tag at the start: world tension, the ruling ideology's rules and tension thresholds
        (ideology modifier plus the start spirits' modifiers of the same key), the war goal types with costs and
        the justification, guarantee, volunteer and lend-lease defines."""
        tag = tag or self.tag
        party = self.ruling_party(tag)
        ideo = self.ideologies().get(party) or {}
        spirits = []
        for f in self.country(tag)["ideas"]:
            i = self.ideas().get(f["v"])
            if not i:
                continue
            for a, c in _numbers(i["modifier"], deep=True).items():
                if a in TENSION_KEYS or a.startswith("justify_war_goal"):
                    spirits.append({"idea": f["v"], "key": a, "value": c, "src": self.src(i["path"], i["line"])})
        gates_ = {}
        for key in TENSION_KEYS:
            base = (ideo.get("modifiers") or {}).get(key)
            extra = [s for s in spirits if s["key"] == key]
            total = (base or 0) + sum(s["value"] for s in extra)
            gates_[key] = {"ideology": base, "spirits": extra, "need": round(total, 4),
                           "text": GATE_TEXT.get(key, key)}
        goals = []
        for gid, g in sorted(self.wargoals().items()):
            st = self.static(g["allowed"], tag)
            if st is False:
                continue
            goals.append(dict({k: v for k, v in g.items() if k not in ("allowed", "available")},
                              allowed=self.gate_text(g["allowed"], tag, 160),
                              available=self.gate_text(g["available"], tag, 160) if g["available"] else True))
        return {"tag": tag, "ideology": party, "ideology_src": ideo.get("src"), "rules": ideo.get("rules") or {},
                "war_impact_on_world_tension": ideo.get("war_impact_on_world_tension"),
                "gates": gates_, "spirits": spirits, "world_tension": self.world_tension(), "wargoals": goals,
                "defines": self.defines_rows(DIPLOMACY_DEFINES), "fair": "public", "d": DIPLOMACY_METHOD}

    # ------------------------------------------------------------ factions (F-17)
    def faction_content(self):
        """{"templates", "rules", "rule_groups", "goals", "member_upgrades", "member_groups", "upgrades",
        "upgrade_groups"}: {id: row} of common/factions (goals and manifests from goals/, rules by `type`)."""

        def load():
            out = {k: {} for k in ("templates", "rules", "rule_groups", "goals", "member_upgrades", "member_groups",
                                   "upgrades", "upgrade_groups")}
            for n, b, p, ln in self.blocks("common/factions/templates"):
                out["templates"][n] = {"id": n, "name": self.loc.get(get1(b, "name") or "") or get1(b, "name"),
                                       "manifest": get1(b, "manifest"), "goals": bare(get1(b, "goals", [])),
                                       "default_rules": bare(get1(b, "default_rules", [])),
                                       "can_leader_join_other_factions": get1(b, "can_leader_join_other_factions") == "yes",
                                       "visible": get1(b, "visible"), "available": get1(b, "available"),
                                       "src": self.src(p, ln), "mod": self.content.source(p)}
            for n, b, p, ln in self.blocks("common/factions/rules"):
                out["rules"][n] = {"id": n, "name": self.loc.get(n) or n, "type": get1(b, "type"),
                                   "trigger": render(get1(b, "trigger", []), 200),
                                   "visible": render(get1(b, "visible", []), 120),
                                   "modifier": _numbers(get1(b, "modifier"), deep=True),
                                   "peace_action_modifiers": bare(get1(b, "peace_action_modifiers", [])),
                                   "src": self.src(p, ln)}
            for n, b, p, ln in self.blocks("common/factions/rules/groups"):
                out["rule_groups"][n] = {"id": n, "default": get1(b, "default_rule"), "rules": bare(get1(b, "rules", [])),
                                         "src": self.src(p, ln)}
            for n, b, p, ln in self.blocks("common/factions/goals"):
                rp = get1(b, "ratio_progress")
                secs = get1(rp, "progress_sections") if isinstance(rp, list) else None
                at = {k: v for k, v in parse_file(p) if k and k.startswith("@") and isinstance(v, str)}
                out["goals"][n] = {"id": n, "name": self.loc.get(get1(b, "name") or n) or n,
                                   "category": get1(b, "category"), "manifest": get1(b, "is_manifest") == "yes",
                                   "completed": render(get1(b, "completed", []), 200),
                                   "ratio": {k: v for k, v in (rp or []) if k and isinstance(v, str)} if isinstance(rp, list) else None,
                                   "scale": _numbers(get1(rp, "scale")) if isinstance(rp, list) else {},
                                   "sections": {k: {"min": num(at.get(get1(v, "min"), get1(v, "min"))),
                                                    "max": num(at.get(get1(v, "max"), get1(v, "max"))),
                                                    "modifier": _numbers(get1(v, "modifier"))}
                                                for k, v in (secs or []) if k and isinstance(v, list)},
                                   "complete_effect": effect_summary(get1(b, "complete_effect"), 6)
                                   if isinstance(get1(b, "complete_effect"), list) else [],
                                   "src": self.src(p, ln)}
            for folder, key in (("common/factions/member_upgrades", "member_upgrades"),
                                ("common/factions/member_upgrades/member_groups", "member_groups"),
                                ("common/factions/upgrades", "upgrades"), ("common/factions/upgrades/groups", "upgrade_groups")):
                for n, b, p, ln in self.blocks(folder):
                    out[key][n] = {"id": n, "name": self.loc.get(get1(b, "name") or n) or n,
                                   "bonus": num(get1(b, "bonus")), "upgrade_cost": num(get1(b, "upgrade_cost")),
                                   "upgrades": bare(get1(b, "upgrades", [])), "default": get1(b, "default_upgrade"),
                                   "type": get1(b, "upgrade_type"), "src": self.src(p, ln)}
            return out

        return self.cached("factions", load)

    # ------------------------------------------------------------ peace conference (F-18)
    def peace_modifiers(self):
        """[{id, category, types, enable (text), tags (tags the enable names), cost_multiplier, src}] of
        common/peace_conference/cost_modifiers, and {category: name} of the categories."""

        def load():
            mods = []
            for n, b, p, ln in self.blocks("common/peace_conference/cost_modifiers", inside="peace_action_modifiers"):
                pt = get1(b, "peace_action_type")
                en = get1(b, "enable", [])
                text = render(en, 260)
                mods.append({"id": n, "category": get1(b, "category") or "other",
                             "types": bare(pt) if isinstance(pt, list) else [pt],
                             "enable": text, "tags": sorted(set(re.findall(r"\b(?:original_)?tag = ([A-Z][A-Z0-9]{2})\b",
                                                                         render(en)))
                                                            | set(re.findall(r"(?<![\w.])([A-Z][A-Z0-9]{2}) = \{", render(en)))
                                                            - {"NOT", "AND"}),
                             "cost_multiplier": num(get1(b, "cost_multiplier")), "src": self.src(p, ln),
                             "mod": self.content.source(p)})
            cats = {n: self.loc.get(get1(b, "name") or n) or n
                    for n, b, p, ln in self.blocks("common/peace_conference/categories", inside="peace_action_categories")}
            return mods, cats

        return self.cached("peacemods", load)

    def peace(self, tag=None, enemies=None):
        """Peace-conference costs for a tag (PEACE_METHOD): the cost modifiers that name the tag or its enemies
        (`own` / `enemy`) and the generic ones, grouped by action type, with the categories and the defines."""
        tag = tag or self.tag
        enemies = list(enemies) if enemies is not None else self.at_war_with(tag)
        mods, cats = self.peace_modifiers()
        rows = []
        for m in mods:
            if not m["tags"]:
                rel = "generic"
            elif tag in m["tags"]:
                rel = "own"
            elif set(m["tags"]) & set(enemies):
                rel = "enemy"
            else:
                continue
            rows.append(dict(m, relation=rel, category_name=cats.get(m["category"], m["category"])))
        rows.sort(key=lambda r: ({"own": 0, "enemy": 1, "generic": 2}[r["relation"]], r["types"], r["id"]))
        return {"tag": tag, "enemies": enemies, "modifiers": rows, "categories": cats,
                "defines": self.defines_rows(PEACE_DEFINES), "fair": "public", "d": PEACE_METHOD}

    # ------------------------------------------------------------ command: spirits, abilities, leader traits (F-19)
    def spirit_slots(self):
        """{idea category: {"type": army_spirit | navy_spirit | air_spirit, "cost", "removal_cost", "src"}} of
        common/idea_tags (military spirits are bought with army / navy / air experience)."""

        def load():
            out = {}
            for n, b, p, ln in self.blocks("common/idea_tags", inside="idea_categories"):
                t = get1(b, "type")
                if isinstance(t, str) and t.endswith("_spirit") and t != "national_spirit":
                    for s in [n] + [x for x in get(b, "slot") if isinstance(x, str)]:
                        out[s] = {"type": t, "cost": num(get1(b, "cost")), "removal_cost": num(get1(b, "removal_cost")),
                                  "src": self.src(p, ln)}
            return out

        return self.cached("spiritslots", load)

    def leader_abilities(self):
        """{ability: {name, type, cost, duration, cooldown, allowed, modifiers, src}} of common/abilities."""

        def load():
            out = {}
            for n, b, p, ln in self.blocks("common/abilities", inside="ability"):
                out[n] = {"id": n, "name": self.loc.get(get1(b, "name") or n) or n, "type": get1(b, "type"),
                          "cost": num(get1(b, "cost")), "duration": num(get1(b, "duration")),
                          "cooldown": num(get1(b, "cooldown")), "allowed": get1(b, "allowed"),
                          "modifiers": {k: _numbers(get1(b, k), deep=True) for k in ("unit_modifiers", "modifier")
                                        if isinstance(get1(b, k), list)},
                          "src": self.src(p, ln), "mod": self.content.source(p)}
            return out

        return self.cached("abilities", load)

    def leader_traits(self):
        """{trait: {type, trait_type, cost, gui_row, gui_column, parents, mutually_exclusive, slot, modifiers,
        enable_ability, src}} of common/unit_leader `leader_traits`."""

        def load():
            out = {}
            for n, b, p, ln in self.blocks("common/unit_leader", inside="leader_traits"):
                par = [x for pb in get(b, "parent") if isinstance(pb, list) for x in bare(get1(pb, "traits", []))]
                t = get1(b, "type")
                out[n] = {"id": n, "name": self.loc.get(n) or n, "type": bare(t) if isinstance(t, list) else t,
                          "trait_type": get1(b, "trait_type"), "cost": num(get1(b, "cost")),
                          "gui_row": num(get1(b, "gui_row")), "gui_column": num(get1(b, "gui_column")),
                          "any_parent": bare(get1(b, "any_parent", [])), "all_parents": bare(get1(b, "all_parents", [])),
                          "parent": par, "mutually_exclusive": get1(b, "mutually_exclusive"),
                          "slot": get1(b, "slot"), "enable_ability": get1(b, "enable_ability"),
                          "attack_skill": num(get1(b, "attack_skill")), "defense_skill": num(get1(b, "defense_skill")),
                          "modifiers": {k: _numbers(get1(b, k), deep=True) for k in LEADER_MODIFIER_KEYS
                                        if isinstance(get1(b, k), list) and get1(b, k)},
                          "src": self.src(p, ln), "mod": self.content.source(p)}
            return out

        return self.cached("leadertraits", load)

    def leader_skills(self):
        """[{level, type, cost, src}] of common/unit_leader `leader_skills` (skill level costs in leader XP)."""
        out = []
        for p in self.game.files("common/unit_leader"):
            for k, v in parse_file(p):
                if k == "leader_skills" and isinstance(v, list):
                    for j, (lvl, b) in enumerate(v):
                        if isinstance(b, list):
                            out.append({"level": num(lvl), "type": get1(b, "type"), "cost": num(get1(b, "cost")),
                                        "src": self.src(p, line_of(v, j))})
        return out

    def command(self, tag=None):
        """Officer-corps facts for a tag: military spirits (idea categories of type army/navy/air_spirit, cost in
        that branch's experience) allowed for the tag, leader abilities, assignable leader traits (trait tree rows),
        skill level costs, the command-power defines and command power at the start."""
        tag = tag or self.tag
        slots = self.spirit_slots()
        spirits = []
        for iid, i in sorted(self.ideas().items()):
            s = slots.get(i["category"])
            if not s:
                continue
            vis = get1(i["block"], "visible")
            gate = {k: self.gate_text(b, tag, 120) for k, b in (("allowed", i["allowed"]), ("visible", vis),
                                                                 ("available", i["available"]))}
            if False in gate.values() or (self.foreign(iid, tag) and gate["visible"] is not True):
                continue
            spirits.append({"id": iid, "name": self.name(iid), "slot": i["category"], "branch": s["type"].split("_")[0],
                            "cost": i["cost"] if i["cost"] is not None else s["cost"],
                            "available": gate["available"], "visible": gate["visible"],
                            "modifier": render(i["modifier"], 200) if i["modifier"] else None,
                            "src": self.src(i["path"], i["line"])})
        abilities = []
        for aid, a in sorted(self.leader_abilities().items()):
            al = self.gate_text(a["allowed"], tag, 160, ("OWNER", "owner"))
            if al is False or (self.foreign(aid, tag) and al is not True):
                continue
            abilities.append(dict({k: v for k, v in a.items() if k != "allowed"}, allowed=al))
        traits = [t for t in self.leader_traits().values() if t["trait_type"] in ("assignable_trait",
                                                                                  "assignable_terrain_trait")
                  and not self.foreign(t["id"], tag)]
        traits.sort(key=lambda t: (str(t["type"]), t["gui_row"] if t["gui_row"] is not None else 99,
                                   t["gui_column"] if t["gui_column"] is not None else 99, t["id"]))
        return {"tag": tag, "command_power_at_start": self.country(tag)["command_power"], "spirits": spirits,
                "abilities": abilities, "traits": traits, "skills": self.leader_skills(),
                "defines": self.defines_rows(COMMAND_DEFINES), "fair": "public", "d": COMMAND_METHOD}

    # ------------------------------------------------------------ intelligence agency (F-20)
    def agency(self, tag=None):
        """Intelligence agency rules: upgrades per branch (levels with modifiers, modifiers while upgrading),
        operations (days, network strength, operatives, risk, equipment, phases) with the static `allowed`
        decided for the tag, operation phases, the agency defines and whether the history creates the agency."""
        tag = tag or self.tag
        ups = []
        for p in self.game.files("common/intelligence_agency_upgrades"):
            for k, v in parse_file(p):
                if not k or not isinstance(v, list):
                    continue
                for j, (n, b) in enumerate(v):
                    if not n or not isinstance(b, list):
                        continue
                    ups.append({"branch": k, "id": n, "name": self.loc.get(n) or n,
                                "levels": [_numbers(get1(lv, "modifier"), deep=True) for lv in get(b, "level")
                                           if isinstance(lv, list)],
                                "during_progress": _numbers(get1(b, "modifiers_during_progress")),
                                "available": render(get1(b, "available", []), 160),
                                "src": self.src(p, line_of(v, j))})
        phases = {n: {"name": get1(b, "name"), "equipment": _numbers(get1(b, "equipment")), "src": self.src(p, ln)}
                  for n, b, p, ln in self.blocks("common/operation_phases")}
        ops = []
        for n, b, p, ln in self.blocks("common/operations"):
            if get1(b, "days") is None:
                continue
            al = self.static(get1(b, "allowed"), tag) if tag else None
            if al is False:
                continue
            groups = []
            for pb in get(b, "phases"):
                if isinstance(pb, list):
                    groups.append([a for a, c in pb if a and a in phases] or bare(pb))
            ops.append({"id": n, "name": self.loc.get(get1(b, "name") or n) or n, "days": num(get1(b, "days")),
                        "network_strength": num(get1(b, "network_strength")), "operatives": num(get1(b, "operatives")),
                        "risk_chance": num(get1(b, "risk_chance")), "outcome_extra_chance": num(get1(b, "outcome_extra_chance")),
                        "cost_multiplier": num(get1(b, "cost_multiplier")),
                        "equipment": _numbers(get1(b, "equipment")), "phases": groups,
                        "requirements": render(get1(b, "requirements", []), 160),
                        "visible": render(get1(b, "visible", []), 160), "allowed": al,
                        "awarded_tokens": bare(get1(b, "awarded_tokens", [])),
                        "src": self.src(p, ln), "mod": self.content.source(p)})
        return {"tag": tag or None, "agency_at_start": self.country(tag)["agency"] if tag else None,
                "upgrades": ups, "operations": ops, "phases": phases, "defines": self.defines_rows(AGENCY_DEFINES),
                "dlc_la_resistance": "La Resistance" in self.dlcs, "fair": "public", "d": AGENCY_METHOD}

    # ------------------------------------------------------------ resources
    def equipment_resources(self):
        """{equipment: {resources: {res: n}, base: equipment the numbers come from, design: bool}} from
        common/units/equipment; an equipment without its own resources takes its archetype's (or parent's);
        a duplicated archetype's copy (small_plane_cas_airframe_0) takes its source's (small_plane_airframe_0).
        design is true for chassis, airframes and hulls, whose modules add resources not counted here."""

        def load():
            raw, dups = {}, []
            for p in self.game.files("common/units/equipment"):
                for k, v in parse_file(p):
                    if k == "equipments" and isinstance(v, list):
                        for name, body in v:
                            if name and isinstance(body, list):
                                raw[name] = {"res": _numbers(get1(body, "resources")),
                                             "up": get1(body, "archetype") or get1(body, "parent"),
                                             "modules": isinstance(get1(body, "module_slots"), (list, str))}
                    elif k == "duplicate_archetypes" and isinstance(v, list):
                        for name, body in v:
                            if name and isinstance(body, list) and isinstance(get1(body, "archetype"), str):
                                dups.append((name, get1(body, "archetype")))
            for dup, arch in dups:
                for eq, r in list(raw.items()):
                    if r["up"] == arch and eq.startswith(arch) and eq != arch:
                        raw.setdefault(dup + eq[len(arch):], {"res": {}, "up": eq, "modules": r["modules"]})

            def resolve(name, seen=()):
                r = raw.get(name)
                if r is None or name in seen:
                    return {}, None, False
                if r["res"]:
                    return r["res"], name, r["modules"]
                res, base, mod = resolve(r["up"], seen + (name,)) if isinstance(r["up"], str) else ({}, None, False)
                return res, base, mod or r["modules"]

            out = {}
            for name in raw:
                res, base, mod = resolve(name)
                out[name] = {"resources": res, "base": base, "design": bool(mod)}
            return out

        return self.cached("equipment_resources", load)

    def resources(self, tag=None):
        """Per-resource balance at the start (RESOURCES_METHOD): produced in owned states, the trade law's
        min_export share, the start production lines' needs, and the balance."""
        tag = tag or self.tag
        states = self.states()
        produced, by_state = defaultdict(float), []
        for sid in self.owned_states(tag):
            st = states[sid]
            if st["resources"]:
                by_state.append({"id": sid, "name": st["name"], "resources": st["resources"], "src": st["file"]})
                for r, n in st["resources"].items():
                    produced[r] += n
        laws = self.laws(tag)
        trade = (laws.get("trade_laws") or {}).get("current")
        min_export = None
        if trade:
            min_export = _numbers(self.ideas().get(trade, {}).get("modifier"), deep=True).get("min_export")
        eq = self.equipment_resources()
        lines, need, partial = [], defaultdict(float), []
        for line in self.oob(tag)["production"]:
            info = eq.get(line["type"]) or {"resources": {}, "base": None, "design": False}
            f = line.get("factories") or 0
            row = {"equipment": line["type"], "version": line.get("version"), "factories": f,
                   "per_factory": info["resources"], "src": line["src"]}
            if info["design"]:
                row["note"] = "design: module resources not counted"
                partial.append(line["type"])
            lines.append(row)
            for r, n in info["resources"].items():
                need[r] += n * f
        names = sorted(set(produced) | set(need))
        balance = {}
        for r in names:
            p = produced.get(r, 0.0)
            exported = p * (min_export or 0)
            balance[r] = {"produced": round(p, 2), "exported": round(exported, 2),
                          "available": round(p - exported, 2), "needed": round(need.get(r, 0.0), 2),
                          "balance": round(p - exported - need.get(r, 0.0), 2)}
        return {"tag": tag, "trade_law": trade, "min_export": min_export, "balance": balance, "states": by_state,
                "lines": lines, "partial_lines": partial, "fair": "public", "d": RESOURCES_METHOD}

    # ------------------------------------------------------------ energy (F-21)
    def energy_buildings(self):
        """{building: {affects_energy, keystone group, energy modifiers (state_modifiers keys with "energy"),
        shares_slots, src}} of common/buildings that change factory energy use."""

        def load():
            out = {}
            for n, b, p, ln in self.blocks("common/buildings", inside="buildings"):
                sm = _numbers(get1(b, "state_modifiers"), deep=True)
                em = {k: v for k, v in sm.items() if "energy" in k}
                cap = get1(b, "level_cap") or []
                if em or get1(b, "affects_energy") == "yes":
                    out[n] = {"affects_energy": get1(b, "affects_energy") == "yes", "energy_modifiers": em,
                              "group": get1(cap, "group_by") if isinstance(cap, list) else None,
                              "cost": num(get1(b, "base_cost")), "buildable": get1(b, "is_buildable") != "no",
                              "shares_slots": get1(cap, "shares_slots") == "yes" if isinstance(cap, list) else False,
                              "src": self.src(p, ln)}
            return out

        return self.cached("energybuildings", load)

    def energy(self, tag=None):
        """Energy of a tag at the start (ENERGY_METHOD): energy resource per own state, factories, the energy
        buildings present and their local consumption modifiers, and the country estimate from the defines."""
        tag = tag or self.tag
        d = self.defines_rows(ENERGY_DEFINES)
        res = (d.get("ENERGY_RESOURCE") or {}).get("v") or "coal"
        eb = self.energy_buildings()
        states = self.states()
        rows, coal, facts_ = [], 0, 0
        for sid in self.owned_states(tag):
            st = states[sid]
            fac = sum(st["buildings"].get(b, 0) for b in FACTORY_BUILDINGS)
            present = {b: n for b, n in st["buildings"].items() if n and eb.get(b, {}).get("energy_modifiers")}
            for bs in st["province_buildings"].values():
                for b, n in bs.items():
                    if n and eb.get(b, {}).get("energy_modifiers"):
                        present[b] = present.get(b, 0) + n
            local = 0.0
            for b in present:
                for k, v in eb[b]["energy_modifiers"].items():
                    local += v * (st["infrastructure"] if k.endswith("_per_infrastructure") else 1)
            c = st["resources"].get(res, 0)
            coal += c
            facts_ += fac
            rows.append({"id": sid, "name": st["name"], "coal": c, "factories": fac, "infrastructure": st["infrastructure"],
                         "buildings": present, "local_consumption": round(local, 4), "src": st["file"]})
        v = lambda k, dflt: (d.get(k) or {}).get("v", dflt)  # noqa: E731
        produced = v("BASE_COUNTRY_ENERGY_PRODUCTION", 0) + coal * v("RESOURCE_TO_ENERGY_COEFFICIENT", 0)
        per = min(v("ENERGY_COST_CAP", 99), v("BASE_ENERGY_COST", 0) + v("ENERGY_SCALING_COST_BY_FACTORY_COUNT", 0) * facts_)
        demand = sum(r["factories"] * per * max(0.0, 1 + r["local_consumption"]) for r in rows)
        return {"tag": tag, "resource": res, "states": rows, "coal": coal, "factories": facts_,
                "estimate": {"produced": round(produced, 2), "per_factory": round(per, 4), "demand": round(demand, 2),
                             "ratio": round(produced / demand, 3) if demand else None},
                "keystones": {b: r for b, r in eb.items() if r["buildable"] and r["energy_modifiers"]}, "defines": d,
                "fair": "public",
                "d": ENERGY_METHOD}


# ---------------------------------------------------------------- script summaries

def _extend(out, block):
    for i, item in enumerate(block):
        out.add(item, line_of(block, i), getattr(block, "ops", [None] * len(block))[i] or "=")


def reduce_trigger(items, ctx):
    """A trigger block with the statically known parts removed: true has_dlc/tag/date atoms are
    dropped, if/else chains whose limit is static are replaced by the branch that applies, and
    NOT/OR/AND wrappers that become empty are dropped. A false atom stays (the block is false)."""
    from .script import trigger_one  # local: keep the module import list short

    out = Block(getattr(items, "line", 1))
    ops = getattr(items, "ops", None)
    taken = None
    for i, (k, v) in enumerate(items):
        kl = (k or "").lower()
        op = (ops[i] if ops is not None and i < len(ops) else "=") or "="
        if kl == "if" and isinstance(v, list):
            cond = trigger(get1(v, "limit", []), ctx)
            if cond is True:
                body = _sub(v, ("limit", "else", "else_if"))
                _extend(out, reduce_trigger(body, ctx))
                taken = True
                continue
            if cond is False:
                taken = False  # the else/else_if siblings that follow decide
                continue
            taken = None
            out.add((k, v), line_of(items, i), op)
            continue
        if kl in ("else", "else_if") and isinstance(v, list):
            if taken is False:
                cond = trigger(get1(v, "limit", []), ctx) if kl == "else_if" else True
                if cond is True:
                    _extend(out, reduce_trigger(_sub(v, ("limit",)), ctx))
                    taken = True
                elif cond is None:
                    out.add(("if", v), line_of(items, i), op)
                    taken = None
            elif taken is None:
                out.add((k, v), line_of(items, i), op)
            continue
        taken = None
        if kl in ("has_dlc", "tag", "original_tag", "has_start_date", "always") and isinstance(v, str):
            if trigger_one(k, v, ctx, op) is True:
                continue
        if kl in ("and", "not", "or", "hidden_trigger") and isinstance(v, list):
            r = trigger_one(k, v, ctx, op) if kl != "hidden_trigger" else None
            if r is True:
                continue
            inner = reduce_trigger(v, ctx) if kl in ("and", "hidden_trigger") else v
            if not inner:
                continue
            out.add((k, inner), line_of(items, i), op)
            continue
        out.add((k, v), line_of(items, i), op)
    return out


_STATE_KEYS = ("has_full_control_of_state", "controls_state", "owns_state", "is_owned_and_controlled_by")
_PROV_KEYS = ("controls_province",)


def control_lists(items):
    """{"states": [...], "provinces": [...], "how": [...]} of the state and province control conditions
    in a trigger (has_full_control_of_state, controls_state, owns_state, controls_province,
    `<state> = { is_controlled_by = ... }`, count_triggers amount)."""
    states, provs, how = [], [], set()

    def walk(block, inside_not=False):
        for k, v in block:
            kl = (k or "").lower()
            if kl == "not" and isinstance(v, list):
                walk(v, True)
                continue
            if kl in _STATE_KEYS and isinstance(v, str) and v.isdigit():
                if not inside_not:
                    states.append(int(v))
                    how.add(kl)
            elif kl in _PROV_KEYS and isinstance(v, str) and v.isdigit():
                if not inside_not:
                    provs.append(int(v))
                    how.add(kl)
            elif k and k.isdigit() and isinstance(v, list):
                if any((a or "").lower() in ("is_controlled_by", "is_fully_controlled_by", "is_owned_by",
                                             "is_owned_and_controlled_by") for a, _ in v) and not inside_not:
                    states.append(int(k))
                    how.add("state scope")
                walk(v, inside_not)
            elif kl == "count_triggers" and isinstance(v, list):
                amount = get1(v, "amount")
                if amount:
                    how.add(f"count_triggers amount {amount}")
                walk(v, inside_not)
            elif isinstance(v, list):
                walk(v, inside_not)

    walk(items)
    if not states and not provs:
        return None
    return {"states": sorted(set(states)), "provinces": sorted(set(provs)), "how": sorted(how)}


_EFFECT_SKIP = {"custom_effect_tooltip", "effect_tooltip", "hidden_effect", "log", "tooltip", "limit"}


def effect_summary(items, limit=12, depth=0):
    """Short list of what an effect block does: ["idea:X", "build:industrial_complex x2@161", "event:Y",
    "pp:+120", "wargoal:ETH", "flag:Z", "annex:ETH", ...] (scopes walked; at most `limit` entries)."""
    out = []

    def add(s):
        if s not in out:
            out.append(s)

    def walk(block, scope=None, d=0):
        for k, v in block:
            if len(out) >= limit:
                return
            kl = (k or "").lower()
            if kl in ("add_ideas", "swap_ideas", "remove_ideas", "add_timed_idea"):
                if isinstance(v, list):
                    if kl == "swap_ideas":
                        add(f"swap:{get1(v, 'remove_idea')}->{get1(v, 'add_idea')}")
                    elif kl == "add_timed_idea":
                        add(f"idea:{get1(v, 'idea')}({get1(v, 'days')}d)")
                    else:
                        for x in bare(v):
                            add(("idea:" if kl == "add_ideas" else "remove:") + x)
                else:
                    add(("idea:" if kl == "add_ideas" else "remove:") + str(v))
            elif kl == "add_building_construction" and isinstance(v, list):
                lvl = get1(v, "level")
                add(f"build:{get1(v, 'type')} x{lvl}" + (f"@{scope}" if scope else ""))
            elif kl in ("add_extra_state_shared_building_slots",):
                add(f"slots:+{v}" + (f"@{scope}" if scope else ""))
            elif kl == "add_research_slot":
                add(f"research_slot:+{v}")
            elif kl == "add_tech_bonus" and isinstance(v, list):
                what = get1(v, "category") or get1(v, "technology") or get1(v, "name")
                add(f"tech_bonus:{what} {get1(v, 'bonus')}x{get1(v, 'uses', '1')}")
            elif kl in ("add_political_power", "add_stability", "add_war_support", "add_manpower",
                        "army_experience", "navy_experience", "air_experience", "add_command_power"):
                add(f"{kl.replace('add_', '')}:{v}" if not isinstance(v, list) else kl)
            elif kl == "create_wargoal" and isinstance(v, list):
                add(f"wargoal:{get1(v, 'target')}({get1(v, 'type')})")
            elif kl in ("declare_war_on",) and isinstance(v, list):
                add(f"war:{get1(v, 'target')}")
            elif kl in ("annex_country", "puppet", "white_peace", "transfer_state", "set_state_owner",
                        "add_state_core", "add_core_of", "give_military_access", "release", "add_to_faction",
                        "create_faction", "set_capital", "add_state_claim", "transfer_state_to"):
                tv = get1(v, "target") if isinstance(v, list) else v
                add(f"{kl}:{tv}" + (f"@{scope}" if scope and kl in ('add_core_of', 'add_state_core') else ""))
            elif kl in ("country_event", "news_event", "state_event", "unit_leader_event"):
                add("event:" + str(get1(v, "id") if isinstance(v, list) else v))
            elif kl in ("set_country_flag", "set_global_flag", "clr_country_flag", "clr_global_flag"):
                f = get1(v, "flag") if isinstance(v, list) else v
                add(("flag:" if kl.startswith("set") else "clear_flag:") + str(f))
            elif kl in ("set_variable", "add_to_variable", "subtract_from_variable"):
                if isinstance(v, list) and v:
                    a, b = v[0]
                    if a == "var":
                        a, b = get1(v, "var"), get1(v, "value")
                    add(f"var:{a}{'=' if kl == 'set_variable' else ('+' if kl.startswith('add') else '-')}{b}")
            elif kl in ("unlock_decision_tooltip", "unlock_decision_category_tooltip", "activate_mission",
                        "activate_decision", "activate_targeted_decision"):
                add(f"{kl.replace('_tooltip', '')}:{get1(v, 'decision') if isinstance(v, list) else v}")
            elif kl in ("complete_national_focus", "unlock_national_focus", "load_focus_tree"):
                add(f"{kl}:{v if not isinstance(v, list) else get1(v, 'tree')}")
            elif kl in ("add_dynamic_modifier", "remove_dynamic_modifier") and isinstance(v, list):
                add(f"{kl.replace('_dynamic_modifier', '')}_dynmod:{get1(v, 'modifier')}")
            elif kl == "set_rule":
                add("set_rule")
            elif kl in ("add_equipment_to_stockpile",) and isinstance(v, list):
                add(f"equipment:{get1(v, 'type')} {get1(v, 'amount')}")
            elif kl in ("add_offsite_building", "add_resource"):
                add(kl)
            elif kl == "if" and isinstance(v, list) and d < 4:
                walk([(a, b) for a, b in v if (a or "").lower() not in ("limit",)], scope, d + 1)
            elif isinstance(v, list) and d < 4 and kl not in ("limit", "tooltip"):
                if k and k.isdigit():
                    walk(v, k, d + 1)
                elif kl.startswith(("random_", "every_", "any_")) or kl in _EFFECT_SKIP or \
                        re.match(r"^[A-Z][A-Z0-9]{2}$", k or "") or kl in ("else", "else_if", "capital_scope",
                                                                          "owner", "controller", "root", "from"):
                    walk(v, scope if not re.match(r"^[A-Z][A-Z0-9]{2}$", k or "") else k, d + 1)
                elif kl not in ("ai_will_do", "modifier") and re.match(r"^[a-z]", k or "") and \
                        kl.endswith("_effect"):
                    add(f"effect:{k}")
            elif isinstance(v, str) and v == "yes" and kl.endswith("_effect"):
                add(f"effect:{k}")

    walk(items)
    return out


def _names_tag(items, tag):
    """True when a parsed block names the tag (tag = X, original_tag = X, X = {...})."""
    if not isinstance(items, list):
        return False
    for k, v in items:
        if k in ("tag", "original_tag") and v == tag:
            return True
        if k == tag:
            return True
        if isinstance(v, list) and _names_tag(v, tag):
            return True
    return False
