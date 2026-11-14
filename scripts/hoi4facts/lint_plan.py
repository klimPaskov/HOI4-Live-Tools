#!/usr/bin/env python3
"""Deterministic plan lint (strategist design 4.2): strategy rules a schema-valid plan can still break.

Usage:
    lint_plan.py PLAN.json [--game DIR] [--userdir DIR] [--mod PATH ...] [--front-distance K] [--json] [--hand]

validate_plan.py checks that every key is something the game and the executor know. The lint checks
that the plan is a sound strategy against the game files: nothing researched ahead of time, no
factory on a front, every border guarded, enough victory points for a capitulation, routes that
connect, a goal with predicates. Every number comes from the effective game content (install plus
mods) through hoi4facts.Game and hoi4facts.derive; nothing is country specific.

Rules (error unless marked; rule ids match skills/hoi4-campaign-playbook/references/lessons.jsonl):
  research.start_year      research_path tech with start_year after its block year
  research.idle            more than 25% of the slots' time to the horizon has nothing planned (an error,
                           with or without a fallback: the fallback is a safety net, not the plan); or any
                           slot time is left unplanned (above 2%) while in-time techs the country can
                           research are not listed (land, industry, electronics and support folders; naval
                           and air ones when the plan has such a goal): an idle slot is only acceptable
                           when nothing in-time remains
  research.throughput      (warning) a block's base research days do not fit the slots before the
                           next block year (research_cost x BASE_TECH_COST, no speed bonus)
  research.prerequisites   prerequisite not at the start or earlier in the path under either parent
                           reading; warning when only the any-parent reading allows it
  research.folders         (warning) naval or air folder techs without a naval or air goal
  research.fallback        research_fallback on without research_fallback_prefixes/categories
  focus.exclusive          two mutually exclusive focuses in focus_path / focus_fallback
  focus.order              a focus planned before its prerequisites (timeline from the focus costs)
  focus.gates              (info) focuses with a runtime `available` gate, with the planned start
  focus.available          a listed focus (focus_path, focus_fallback) that can never start before the plan
                           horizon under the plan's own conditions: its `available` trigger, prerequisites
                           or exclusions read from the game files against the plan's date, war state (the
                           start wars until the planned capitulation), completed focuses and start flags
  focus.idle               the focus slot has nothing it can start for more than 7 days (every listed
                           focus is blocked then, e.g. the fallbacks all need peace while the plan is at
                           war), or the lists run out more than 30 days before the horizon
  focus.unconfirmed        (warning) the slot rests on a focus whose gate cannot be decided from the plan
                           (ownership, control, variables) while no focus that is certainly available is
                           listed for that time; or a listed focus waits for a prerequisite only the game
                           completes by script
  construction.front       factory in a state within K province steps of an enemy-held province
  construction.infrastructure  factory in a state with infrastructure <= 2 and no infrastructure
                           queued before it there
  construction.railway     railway path provinces not touching, or not controlled when the
                           trigger fires (start control plus the vp_targets taken by then)
  construction.railway_node  a railway whose start province is no supply hub, naval base or capital and
                           no hub or port the plan builds there (the game runs a railway from a node; the
                           executor would move the start to the nearest node)
  construction.trigger_route  a province_controlled: trigger off every vp target route
  construction.hub         supply hub in a province or state an enemy still holds when it fires
  production.factories     production lines need more factories than the plan has, each pool on its own:
                           land and air lines against the military factories, naval lines (ships, convoys)
                           against the dockyards
  production.equipment     unknown equipment, or equipment no planned tech unlocks
  production.trucks        war.motorization set but no motorized (truck) line
  army.guards              own state bordering an enemy with no guard (layout guard role,
                           war.guard_states or war.guards); warning: a hold_targets post outside the guard
                           states of the layout guard it moves (that border loses the guard while the hold runs)
  army.templates           layout template not in the start order of battle
  army.counts              layout count above the divisions of its templates in its state (warning
                           when the entries of one state together need more than it has)
  army.generals            general, army_group marshal or navy admiral unknown or not a commander of the tag; a
                           marshal with no land role, an admiral with no navy_leader role; one person in two
                           posts (info: a marshal who is a general is promoted by hoi4_army_commander)
  army.strike_size         strike entry above 24 divisions
  army.formations          (warning) command not concentrated: an army_layout entry below MIN_FORMATION divisions
                           without a general that is not a task (task_of), or more than MAX_FORMATIONS formations
                           in one theatre (the whole layout, or each army group when there are several); the
                           executor folds them into tasks of the nearest formation either way
  army.marshal             (warning) two or more formations and no army_group over them (one field marshal per
                           theatre); (info) a group with no marshal: the executor picks the best free field marshal
  conquest.threshold       vp_targets below the enemy's surrender threshold (base limit plus its
                           starting surrender_limit modifiers), when the goal is a capitulation; the
                           threshold is the one at war support 50% or more (low war support lowers it, and
                           facts.surrender.war_support gives the lower numbers for earlier checkpoints)
  conquest.mandatory       a state the goal must control has no vp target (warning: provinces of a
                           fully-controlled state left out)
  conquest.adjacency       target (or the first province of its path) not next to own land or an
                           earlier target; path provinces not touching
  conquest.neutral_land    a vp target or path province in the land of a country the plan neither fights nor
                           is allied to at the start (no unit crosses a neutral border); warning when a plan
                           predicate names that country gone, beaten or at war (the leg waits for it)
  conquest.rivers          (warning) more than 2 river crossings on one leg
  conquest.by_order        (warning) `by` dates going backwards for one army
  conquest.skipped_near    (warning) an enemy VP province next to own land or a planned target is never
                           taken while the plan takes targets farther away
  win.goal                 no goal.predicates
  win.deadline             goal.deadline differs from a scenario mission timer (info when the goal
                           has no deadline and timers exist)
  events.options           event_rules option names / event_options indexes that the event lacks
  schema.keys              schema errors (unknown keys, except _-prefixed)
  schema.player_call       player-owned items without a `call` (the tool call they stand for)
  schema.lint_keys         keys the plan sets inside executor-owned keys that the executor does not act on
                           (info: lint keys shape these estimates only, planned keys have no executor step
                           yet; plan_keys.py); a warning when vp_targets `order` differs from the list order
                           the executor follows

Coverage-build rules (coverage audit section 3 "Check" column; W1-2):
  coverage.checklist       no top-level _coverage, a checklist id c1..c29 missing, a value that is not
                           key:<plan key>[,...] | call[:<key>] | md:<plan.md section> | na:<reason>, a key
                           the plan lacks, or na: "missing ..." (a gap, not a reason)
  posture.shares           (warning) production factory shares of a force_posture phase more than 15
                           points from the posture (start lines that only complete a unit in progress are
                           left out); error when a posture phase does not sum to 100
  navy.required            a coastal country (an owned coastal province) without a navy key
  navy.overseas            an army state the capital cannot reach over own land, without a navy key,
                           a convoy plan (convoy production line or sea_transfer.convoys) and a port plan
                           (sea_transfer.ports or a port building queued there)
  supply.port_convoys      the plan builds a port or relies on one (sea_transfer.ports, an overseas army)
                           but has no convoy item (convoy production line or manage.convoys_free);
                           sea supply needs SUPPLY_CONVOY_FACTOR convoys per supply
  supply.convoy_size       the convoys the plan's sea supply and sea trade need, against the convoys the country
                           starts with: divisions standing in states the capital cannot reach over own land
                           (start OOB and army_layout) x the supply use of their templates (sub-unit
                           supply_consumption) x SUPPLY_CONVOY_FACTOR, plus the planned imports (auto_import
                           budget x 8 units a factory, trade deal amounts) x 0.1 convoy a unit; error when the
                           need is above the start convoys or above sea_transfer.convoys; the sizing is always
                           in facts.convoys (before range: CONVOY_RANGE_FACTOR x the route length is not modelled)
  xp.unspent               (warning) a plan at war from the start that spends no army experience although the
                           country pool can pay the cheapest land doctrine before the war ends: start + the
                           focus_path rewards + FIELD_EXPERIENCE_MAX_PER_DAY x days at war (the NMilitary
                           country-pool defines; UNIT_EXPERIENCE_* is a division's veterancy); facts.xp
                           holds the floor and the ceiling
  alarms.silenced          an alarm switched off (false or null in alarms, or in the alarms of a phase or
                           contingency override) whose code the same object's `why` does not name with a
                           reason: an alarm is a safety net, and one that seems noisy is a defect to report,
                           not to silence. research_slot_empty and no_active_focus already stay quiet while
                           the executor owns the slot (alarms.slot_empty_grace_days)
  supply.axis_reach        an axis state (war, war_by_phase, phase overrides) whose main province gets an estimated supply
                           flow below 1.0 from the best node held by then (start nodes, planned hubs and
                           ports, nodes of earlier axis states), from the NSupply defines, terrain, state
                           infrastructure, rivers and war.motorization (warning: an estimate; error when no
                           node is held or planned within 14 steps); each state once, at its first axis
  construction.hub_cost    a supply hub (or any building of base_cost >= 15000) finishes after the goal
                           deadline or the plan horizon: base_cost / (min(15, free civs) x points per civ
                           x infrastructure speed); warning above 120 days, or when an existing or
                           capturable hub within 3 steps could serve the same provinces by railway; the
                           finding names the port alternative when an own coastal province is 3 steps away
  air.base_capacity        (warning) air_plan planes on a base above its capacity (level at the start plus
                           queued levels, x AIRBASE_CAPACITY_MULT)
  templates.equipment     a template unit or its equipment unlocked by no tech at the start or in the
                           research plan (warning when the tech comes after the template's date)
  templates.xp             a template with xp and no xp_budget.army spend ref template:<id>
  templates.locked         a template whose base is obsolete (decommissioned) or is_locked in the history
                           OOB: the game offers no Edit for it
  politics.law_gates       an economy or conscription law with an `available` gate bought without a
                           `when` predicate (other gated laws: warning; warning when `when` skips the
                           gate's war support / stability)
  occupation.required      states resisting at the start, or states the plan conquers, without an
                           occupation key
  peace.enemies            an enemy without a peace item (one of its states, or its tag, in peace.take,
                           puppet or liberate)
  manage.thresholds        no manage key (plan_manage.rs runs the manpower, stability, equipment and
                           stockpile rules only with it; {} takes every default); warning for a threshold
                           whose rule is in manage.off or that the executor does not run yet
  production.remove        (warning) a `replace` entry (not run by the executor yet; removals are
                           production.in_progress)
  predicates.unreadable    (warning) a goal, fail predicate, checkpoint, phase or contingency that can never
                           hold because it rests on atoms the executor cannot read yet (x-atoms readable:
                           false: var, mission_days)

Tactics rules (the strong-player habits the plan can state; numbers from the defines and files):
  production.mature_switch an entry that moves a mature line (a start line, or one 30+ days old) to another
                           tier of its family (replace, a design on another tier, or a remove plus an add
                           within 14 days) instead of starting the new tier on new factories: a switch keeps
                           30% (parent/child) or 20% (same archetype) of the line's efficiency, new factories
                           start at 10%
  production.cull          a start line whose equipment no template (plan or kept start template), air_plan
                           wing, design, stockpile target, training guard, plan line or production_priority
                           prefix uses, and no remove entry (naval lines: when navy.posture is none; trucks
                           with war.motorization; convoys for overseas supply); and any line, start or plan,
                           of a domain a force_posture phase gives 0% on that phase's date, except a start
                           line that only completes its unit in progress (a remove entry waits for the
                           finish date production.in_progress asks for): it is neither culled early nor
                           counted in the posture shares
  production.day0          free military factories at the start (military factories minus the start lines,
                           plus those freed by start removals) not put on kept start lines by the day-0
                           production entries (they join a start line at its efficiency while paused)
  production.in_progress   a removal that throws away a unit in progress: a start line's ship or expensive
                           item past the game's own finish threshold (10%; cheap items 75%) removed without a
                           `from` or before it finishes (base factory speed, hull cost); (warning) any naval
                           line removed without a `from`
  production.naval_cap     a naval line asking for more dockyards than one line takes (capital ships 5,
                           convoys 15, others 10: *_MAX_NAV_FACTORIES_PER_LINE)
  army.width_terrain       a plan template (not garrison), or the start templates army_layout fields, whose
                           width fills the theatre's battles (terrain mix of war.theatre, axes and target
                           states; over-width and stacking penalties) 0.05 below the best width (error at 0.10)
  war.supply_floor         war.supply_floor (base, war_by_phase, overrides) below the supply where attrition
                           starts (SUPPLY_THRESHOLD_FOR_ARMY_ATTRITION, 35%); (warning) the attack threshold
                           (supply_floor, or an attack-supply key) below 50%
  war.planning_wait        offensives (vp_targets or axes) with no planning wait: a planning-days key missing
                           or 0 (warning below two thirds of PLANNING_MAX / PLANNING_GAIN days); (warning)
                           war.direct_moves; (info) no such key in the schema yet
  war.pocket_threshold     pocket fall-back switched off (avoid_pockets false) or a pocket-share key above
                           50%; (info) the executor's 70% when no key exists
  politics.pp_budget       politics, decisions and decision_rules costs that the net PP income cannot pay by the
                           horizon (calc.pp_rates: (base + gain) x (1 + factor + stability) minus the running
                           focus's cost); (warning) a dated buy affordable more than 30 days late; (warning)
                           projected PP above 100 with no planned buy left for more than 60 days in a row
                           (info when politics_auto.order sets a standing buy order)
  politics.unneeded_conscription  a politics conscription law above the lowest law whose recruitable
                           manpower covers the plan's divisions plus manage.manpower_months of losses (15% a
                           month assumed); (warning) manage.conscription_laws unset or listing laws with an
                           output penalty above that need
  research.ahead           (warning; error from a year) a tech of a year-less research_path block started more
                           than half a year before its start year on the plan's slot timeline (each year ahead
                           adds BASE_YEAR_AHEAD_PENALTY_FACTOR times the time) while startable in-year techs of
                           the plan's folders stay unplanned; techs listed in research_bonuses are left out
  navy.invasion_prep       an invasion with prepare_days below the preparation the start techs allow
                           (NAVAL_INVASION_PREPARE_DAYS plus naval_invasion_prep_days), or armies above the
                           division cap (BASE_NAVAL_INVASION_DIVISION_CAP plus naval_invasion_division_cap)
  navy.sea_control         an invasion whose shortest sea route crosses a region no navy patrol or
                           strike_force mission names (control needs DOMINANCE_CONTROLLED_THRESHOLD_RATIO)
  construction.war_civs    (warning) a civilian factory queued at war with less than a year to the goal
                           deadline or plan horizon
  navy.strike_without_patrol  (warning) a strike_force mission in a sea region no own patrol mission covers
                           (a strike force engages only what patrols spot)
  peace.order              (info) peace.take ranks states the plan does not hold at the conference (no vp target
                           or axis there, not owned) ahead of a state it holds

Hand plans: with _meta.source "hand" (or --hand) every coverage-build and tactics error above is reported as
a warning, so the two hand plans keep 0 errors while their gaps stay visible; the older rules are
unchanged.

Stdlib only, Python 3.9+. Exit status 1 when there are errors, 2 on usage problems.
"""
import argparse
import json
import math
import os
import re
import sys
from collections import Counter, defaultdict, deque

if __package__ in (None, ""):
    sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
    from hoi4facts import calc as calcmod  # noqa: E402
    from hoi4facts import content, derive, mapgraph, plan_gates, plan_keys, schema_check  # noqa: E402
    from hoi4facts import tech as techrule  # noqa: E402
    from hoi4facts.game import Game  # noqa: E402
    from hoi4facts import research as researchmod  # noqa: E402
    from hoi4facts.validate import (Report, ctx_of, load_schema, parse_predicate, parse_when,  # noqa: E402
                                    predicate_dates, predicate_table, resolve_meta, start_of, start_oobs,
                                    start_tech_set, template_columns,
                                    predicate_can_hold)
    from hoi4facts.validate import _world as world_of  # noqa: E402
else:
    from . import calc as calcmod
    from . import content, derive, mapgraph, plan_gates, plan_keys, schema_check
    from . import tech as techrule
    from .game import Game
    from . import research as researchmod
    from .validate import (Report, ctx_of, load_schema, parse_predicate, parse_when, predicate_dates,
                           predicate_table, resolve_meta, start_of, start_oobs, start_tech_set, template_columns,
                           predicate_can_hold)
    from .validate import _world as world_of

DEFAULT_GAME_DIR = os.path.expanduser("~/Library/Application Support/Steam/steamapps/common/Hearts of Iron IV")
FRONT_K = 2
MAX_STRIKE = 24
LOW_INFRA = 2
MAX_RIVERS = 2
POSTURE_POINTS = 15        # posture.shares: percentage points a production share may differ
FOCUS_IDLE_DAYS = 7        # focus.idle: days the focus slot may stand empty between two focuses
FOCUS_END_SLACK = 30       # focus.idle: days before the horizon the focus lists may run out
HUB_COST = 15000           # construction.hub_cost: base_cost from which a building counts as a hub-sized job
HUB_DAYS = 120             # construction.hub_cost: warning above this many days
HUB_STEPS = 3              # construction.hub_cost: steps to a hub or coastal province that could serve instead
NODE_STEPS = 6             # construction.railway_node: steps searched for the nearest node to name
REACH_STEPS = 14           # supply.axis_reach: steps searched for supply nodes around an axis state
REACH_SOURCES = 4          # supply.axis_reach: nearest nodes routed per axis state
REACH_OK = 1.0             # supply.axis_reach: estimated flow below which the axis state is short of supply
MATURE_DAYS = 30          # production.mature_switch: a line this old (or a start line) is mature
SWITCH_MATCH_DAYS = 14    # production.mature_switch: a remove and an add this close are one switch
MAX_FORMATIONS = 3        # army.formations: formations per theatre (one field marshal)
MIN_FORMATION = 3         # army.formations: smallest formation without a named general
WIDTH_WARN = 0.05         # army.width_terrain: theatre fill this far below the best width warns
WIDTH_ERROR = 0.10        # ... and this far is an error
ATTACK_SUPPLY = 0.5       # war.supply_floor: no new offensive below this supply (the game's low-supply alert)
POCKET_SHARE = 0.5        # war.pocket_threshold: fall back at this hostile share around an army
EXECUTOR_POCKET_SHARE = 0.5  # the encirclement_risk read of hoi4_armies the executor falls back on
AHEAD_YEARS = 0.5         # research.ahead: warning from half a year ahead, error from one year
PP_LATE_DAYS = 30         # politics.pp_budget: a purchase this late against its date warns
PP_IDLE = 100             # politics.pp_budget: political power above this with no planned buy is idle
PP_IDLE_DAYS = 60         # ... and idle this many days in a row warns
MONTHLY_LOSS_SHARE = 0.15  # politics.unneeded_conscription: assumed share of the fielded manpower lost a month at war
WAR_CIV_DAYS = 365        # construction.war_civs: a civilian factory queued at war needs this long to the goal
NEW_RULES = {"coverage.checklist", "posture.shares", "navy.required", "navy.overseas", "supply.port_convoys",
             "supply.convoy_size",
             "construction.hub_cost", "air.base_capacity", "templates.equipment", "templates.xp", "templates.locked",
             "politics.law_gates", "occupation.required", "peace.enemies", "manage.thresholds", "production.remove",
             "predicates.unreadable", "conquest.neutral_land", "alarms.silenced",
             # tactics research rules
             "production.mature_switch", "production.cull", "production.day0", "production.in_progress",
             "production.naval_cap", "army.width_terrain", "war.supply_floor", "war.planning_wait",
             "war.pocket_threshold", "politics.unneeded_conscription", "politics.pp_budget", "research.ahead",
             "navy.invasion_prep", "navy.sea_control", "construction.war_civs", "navy.strike_without_patrol",
             "peace.order", "construction.railway_node", "supply.axis_reach"}
NEW_RULES |= {"focus.available", "focus.idle", "focus.unconfirmed"}  # the focus slot walk (plan_gates)
SEVERITIES = ("error", "warning", "info")
_PRED_TAG = re.compile(r"^(capitulated|at_war|at_war_with|exists|not_exists|not_at_war):([A-Z][A-Z0-9]{2})$")
_PRED_STATE = re.compile(r"^(state_controller|owner|controller):(\d+)\s*==?\s*([A-Z][A-Z0-9]{2})$"
                         r"|^(state_fully_controlled|controls_state):(\d+)$")


# Sea trade: neither number is in the defines (the trade window shows both).
TRADE_UNITS_PER_FACTORY = 8
TRADE_CONVOYS_PER_UNIT = 0.1
RESEARCH_IDLE_SLACK = 0.02  # research.idle: unplanned share of the slot time allowed while in-time techs remain


def folder_family(folder):
    """"naval" or "air" for a technology folder of that arm (naval_folder, mtgnavalfolder,
    mtgnavalsupportfolder; air_techs_folder, bba_air_techs_folder), else None."""
    f = (folder or "").lower()
    if "naval" in f:
        return "naval"
    if re.search(r"(^|_)air(_|$)", f):
        return "air"
    return None


def _atoms(pred):
    """Atom strings of a predicate (composites flattened; NOT branches left out; a list is all)."""
    if isinstance(pred, list):
        pred = {"all": pred}
    if isinstance(pred, str):
        return [pred]
    out = []
    if isinstance(pred, dict):
        for k in ("all", "any"):
            for p in pred.get(k) or []:
                out += _atoms(p)
    return out


def _ints(v):
    return [x for x in v if isinstance(x, int) and not isinstance(x, bool)] if isinstance(v, list) else []


class Lint:
    def __init__(self, plan, game, k=FRONT_K, schema=None, hand=None):
        self.plan = plan
        self.game = game
        self.k = k
        self.schema = schema if schema is not None else load_schema()
        meta = plan.get("_meta") if isinstance(plan.get("_meta"), dict) else {}
        self.hand = bool(hand) if hand is not None else meta.get("source") == "hand"
        self.findings = []
        self.facts = {}
        self.meta = resolve_meta(plan, game, Report())
        self.tag = self.meta.get("tag", "")
        self.ctx = ctx_of(self.meta, game)
        self.start = start_of(self.meta)
        self.stop = self.meta.get("stop_date") or plan.get("stop_on_date")
        self.war = plan.get("war") if isinstance(plan.get("war"), dict) else {}
        self.goal = plan.get("goal") if isinstance(plan.get("goal"), dict) else {}

    # ------------------------------------------------------------------ helpers
    def add(self, severity, rule, where, message):
        if self.hand and severity == "error" and rule in NEW_RULES:
            severity, message = "warning", message + " (hand plan: reported as a warning)"
        self.findings.append({"severity": severity, "rule": rule, "where": where, "message": message})

    def list_of(self, key, plan=None):
        v = (plan or self.plan).get(key)
        return [x for x in v if isinstance(x, dict)] if isinstance(v, list) else []

    def states(self):
        return self.game.states()

    def enemies(self):
        out = set()
        for t in self.war.get("enemies") or []:
            if isinstance(t, str):
                out.add(t)
        phases = self.plan.get("war_by_phase")
        if isinstance(phases, dict):
            for p in phases.values():
                if isinstance(p, dict):
                    out.update(t for t in p.get("enemies") or [] if isinstance(t, str))
        for p in self.goal.get("predicates") or []:
            for a in _atoms(p):
                m = _PRED_TAG.match(a)
                if m and m.group(1) in ("capitulated", "at_war", "at_war_with"):
                    out.add(m.group(2))
        out.discard(self.tag)
        return sorted(out)

    def passable_tags(self):
        """Tags whose land the plan's units may enter: own, friends, access givers and enemies (the start
        wars plus the plan's); None without a tag."""
        if not self.tag:
            return None
        if getattr(self, "_passable", None) is None:
            self._passable = set(self.world.land_access(self.tag, self.enemies())["tags"])
        return self._passable

    def predicate_tags(self):
        """Tags a predicate anywhere in the plan names as gone, beaten or at war (not_exists, capitulated,
        at_war_with): land that opens later in the campaign."""
        out = set()

        def walk(node):
            if isinstance(node, dict):
                for v in node.values():
                    walk(v)
            elif isinstance(node, list):
                for v in node:
                    walk(v)
            elif isinstance(node, str):
                m = _PRED_TAG.match(node)
                if m and m.group(1) in ("capitulated", "not_exists", "at_war_with"):
                    out.add(m.group(2))

        walk(self.plan)
        out.discard(self.tag)
        return out

    def start_control(self):
        return self.game.cached(f"lint:startctl:{self.tag}:{self.start}",
                                lambda: derive.tag_provinces(self.game, self.ctx, [self.tag], self.start))

    def targets(self):
        """vp_targets in plan order ("order" when every entry has one)."""
        t = self.list_of("vp_targets")
        if t and all(isinstance(e.get("order"), int) for e in t):
            t = sorted(t, key=lambda e: e["order"])
        return [e for e in t if isinstance(e.get("province"), int)]

    def captures(self, e):
        return _ints(e.get("path")) + [e["province"]]

    def control_at(self, when):
        """Provinces the plan expects to control when a trigger fires: the start control plus the
        vp targets (and their paths) taken by then."""
        w = parse_when(when) if isinstance(when, str) or when is None else ("text", str(when))
        ctl = set(self.start_control())
        targets = self.targets()
        kind = w[0]
        if kind in ("date", "date_prefix"):
            d = derive.day(w[1])
            for e in targets:
                if e.get("by") and derive.day(e["by"]) is not None and derive.day(e["by"]) <= d:
                    ctl.update(self.captures(e))
        elif kind == "province":
            ctl.add(w[1])
            for i, e in enumerate(targets):
                if w[1] in self.captures(e):
                    for x in targets[:i + 1]:
                        ctl.update(self.captures(x))
                    break
        elif kind == "state":
            ctl.update(self.states().get(w[1], {}).get("provinces", []))
            last = max((i for i, e in enumerate(targets) if e.get("state") == w[1]), default=-1)
            for x in targets[:last + 1]:
                ctl.update(self.captures(x))
        elif kind == "owner" and w[2] == self.tag:
            ctl.update(self.states().get(w[1], {}).get("provinces", []))
        return ctl

    def adjacency(self, straits=True):
        if straits:
            return derive.land_adjacency(self.game) or {}
        return self.game.neighbours(straits=False) or {}

    # ------------------------------------------------------------------ research
    def research(self):
        game, plan = self.game, self.plan
        techs = game.techs()
        meta = derive.tech_meta(game)
        have = start_tech_set(game, self.meta)
        done = set(have)
        path = [(i, e) for i, e in enumerate(plan.get("research_path") or []) if isinstance(e, dict)]
        blocks = []
        for i, e in path:
            year = e.get("year")
            if not isinstance(year, int):
                year = derive.as_tuple(e.get("not_before"))[0] if derive.as_tuple(e.get("not_before")) else None
            ts = [t for t in e.get("techs") or [] if isinstance(t, str)]
            blocks.append((i, year, e.get("not_before"), ts))
            for t in ts:
                info = techs.get(t)
                if info is None:
                    continue  # validate_plan reports unknown keys
                where = f"research_path[{i}]"
                if year and info["year"] > year:
                    self.add("error", "research.start_year", where,
                             f"{t}: start_year {info['year']} is after the block year {year} (ahead-of-time penalty)")
                any_miss = techrule.missing(info, done, "any")
                all_miss = techrule.missing(info, done, "all")
                if any_miss:
                    self.add("error", "research.prerequisites", where,
                             f"{t}: needs {' / '.join(any_miss)} at the start or earlier in research_path")
                elif all_miss:
                    self.add("warning", "research.prerequisites", where,
                             f"{t}: startable only under the any-parent reading; all-parent reading still "
                             f"needs {', '.join(all_miss)}")
                done.add(t)
        # research_priority has no year: flag what lies past the plan window (one finding)
        last_year = derive.as_tuple(self.stop)[0] if self.stop else derive.as_tuple(self.start)[0] + 1
        late = [f"{t} ({techs[t]['year']})" for t in plan.get("research_priority") or []
                if isinstance(t, str) and t in techs and techs[t]["year"] > last_year
                and not any(t in b[3] for b in blocks)]
        if late:
            self.add("warning", "research.start_year", "research_priority",
                     f"{len(late)} tech(s) start after the plan window ({last_year}): {', '.join(late[:12])}"
                     + (" ..." if len(late) > 12 else ""))
        # throughput: cumulative base days against the slot-days before the next block
        slots = derive.research_slots(game, self.tag, self.ctx, self.start, self.meta.get("history_file"))
        begin = derive.day(self.start)
        total = 0.0
        cap_rows = []
        for n, (i, year, nb, ts) in enumerate(blocks):
            total += sum(derive.tech_days(game, t) for t in ts if t in techs and t not in have)
            if n + 1 < len(blocks):
                nxt = blocks[n + 1]
                end = derive.day(nxt[2]) if nxt[2] else (derive.day((nxt[1], 1, 1)) if nxt[1] else None)
                end_label = derive.fmt(derive.date_of(end)) if end else None
            elif self.stop:
                end, end_label = derive.day(self.stop), derive.fmt(self.stop)
            else:
                end = end_label = None
            own = derive.day(nb) if nb else (derive.day((year, 1, 1)) if year else begin)
            if end is None or end <= max(begin, own or begin):
                continue  # the plan stops before this block starts: nothing to fit
            capacity = slots * (end - begin)
            cap_rows.append({"block": i, "cumulative_days": round(total), "slot_days": capacity, "until": end_label})
            if total > capacity:
                self.add("warning", "research.throughput", f"research_path[{i}]",
                         f"blocks up to year {year} need {total:.0f} base research days, {slots} slots give "
                         f"{capacity} by {end_label} (before research-speed bonuses)")
        self.facts["research"] = {"slots": slots, "base_tech_cost": derive.define(game, "BASE_TECH_COST"),
                                  "capacity": cap_rows}
        # idle slots: planned base days against the slot-days to the horizon. A slot with nothing to
        # research is wasted time (user rule: research proper techs, never idle); with the fallback off
        # nothing fills it.
        horizon = self.stop or (self.goal.get("deadline") if isinstance(self.goal, dict) else None)
        end_day = derive.day(horizon) if horizon else begin + 365
        if end_day > begin and slots:
            # the capacity row of the last block window that ends by the horizon (its cumulative days
            # are the techs planned to start before then); else everything against the horizon
            rows_in = [r for r in cap_rows if r["until"] and derive.day(r["until"]) <= end_day]
            if rows_in:
                last = max(rows_in, key=lambda r: derive.day(r["until"]))
                total, capacity = last["cumulative_days"], last["slot_days"]
                end_day = derive.day(last["until"])
            else:
                capacity = slots * (end_day - begin)
            idle = max(0.0, 1.0 - total / capacity)
            fallback = plan.get("research_fallback", True)
            self.facts["research"]["idle_share"] = round(idle, 3)
            spare = self.unlisted_in_time_techs(derive.date_of(end_day - 1)[0]) if idle > RESEARCH_IDLE_SLACK else []
            self.facts["research"]["unlisted_in_time"] = len(spare)
            if RESEARCH_IDLE_SLACK < idle <= 0.25 and spare:
                self.add("error", "research.idle", "research_path",
                         f"about {idle:.0%} of the {slots} slots' time to {derive.fmt(derive.date_of(end_day))} has "
                         f"nothing planned ({total:.0f} of {capacity} slot-days) while {len(spare)} in-time techs are "
                         f"not listed (e.g. {', '.join(spare[:8])}): every slot stays busy while an in-time tech "
                         f"remains (industry and production first, then the army's equipment and support, then "
                         f"what later wars need); the campaign goes on after this war")
            if idle > 0.25:
                # The fallback is a safety net, not the plan (user rule: every slot busy with proper,
                # in-time techs, industry first): an unplanned quarter of the slots fails either way.
                how = ("research_fallback is off" if fallback in (False, "priority_only") else
                       "the fallback (%s) is a safety net, not a plan"
                       % ", ".join(plan.get("research_fallback_prefixes") or ["the whole catalogue"]))
                self.add("error", "research.idle", "research_path",
                         f"about {idle:.0%} of the {slots} slots' time to {derive.fmt(derive.date_of(end_day))} has "
                         f"nothing planned ({total:.0f} of {capacity} slot-days); {how}: list in-time techs "
                         f"(industry first, then the templates' equipment) or pull the next tier forward")
        # naval / air folders
        planned = [t for b in blocks for t in b[3]] + [x for x in plan.get("research_priority") or [] if isinstance(x, str)]
        fams = defaultdict(list)
        for t in planned:
            f = (meta.get(t) or {}).get("folder") or ""
            for fam in ("naval", "air"):
                if folder_family(f) == fam:
                    fams[fam].append(t)
        goals = self.domain_goals()
        for fam, ts in sorted(fams.items()):
            if not goals[fam]:
                self.add("warning", "research.folders", "research_path",
                         f"{fam} folder techs {', '.join(sorted(set(ts)))} but the plan has no {fam} goal")
        # fallback
        fb = plan.get("research_fallback")
        if fb not in (None, False):
            restricted = any(plan.get(k) for k in ("research_fallback_prefixes", "research_fallback_categories"))
            if not restricted:
                self.add("error", "research.fallback", "research_fallback",
                         "fallback research is on and unrestricted (no research_fallback_prefixes or "
                         "research_fallback_categories): it picks far-future techs")

    def domain_goals(self):
        plan = self.plan
        text = " ".join(str(self.goal.get(k, "")) for k in ("summary", "reading")).lower()
        folders = {str(d.get("folder", "")) for d in self.list_of("doctrines")}
        prod = " ".join(str(x.get("equipment", "")) for x in self.list_of("production"))
        prod += " " + " ".join(x for x in plan.get("production_priority") or [] if isinstance(x, str))
        air = bool(self.list_of("air_plan")) or "air" in folders or bool(re.search(r"plane|airframe", prod)) \
            or bool(re.search(r"\bair\b", text))
        naval = bool(folders & {"navy", "naval"}) or bool(re.search(r"ship|hull|convoy|submarine", prod)) \
            or bool(re.search(r"naval|navy|\bsea\b", text))
        return {"air": air, "naval": naval}

    # ------------------------------------------------------------------ focus
    def focus(self):
        game, plan = self.game, self.plan
        path = [x for x in plan.get("focus_path") or [] if isinstance(x, str)]
        fallback = [x for x in plan.get("focus_fallback") or [] if isinstance(x, str)]
        if not path and not fallback:
            return
        trees = game.focus_trees()
        tid = game.loaded_tree(self.ctx) if self.ctx else None
        if not tid:
            tid = max(trees, key=lambda t: sum(1 for k in path + fallback if k in trees[t]), default=None)
        tree = trees.get(tid, {})
        fmeta = derive.focus_meta(game)
        days_per = float(derive.define(game, "FOCUS_POINT_DAYS", 7))
        listed = path + fallback
        reported = set()
        for k in listed:
            info = tree.get(k)
            if not info:
                continue
            for other in sorted(info["exclusive"] & set(listed)):
                pair = tuple(sorted((k, other)))
                if pair not in reported:
                    reported.add(pair)
                    self.add("error", "focus.exclusive", "focus_path",
                             f"{pair[0]} and {pair[1]} are mutually exclusive; the second can never start")
        done = set(self.meta.get("completed_focuses", []))
        t = derive.day(self.start)
        timeline = []
        for i, k in enumerate(path):
            info = tree.get(k)
            if info is None:
                continue
            for block in info["prereq"]:
                if not block & done:
                    self.add("error", "focus.order", f"focus_path[{i}]",
                             f"{k} starts {derive.fmt(derive.date_of(t))} before its prerequisite "
                             f"{' or '.join(sorted(block))} is completed")
            dur = fmeta.get(k, {}).get("cost", 1.0) * days_per
            timeline.append({"focus": k, "start": derive.fmt(derive.date_of(t)), "end": derive.fmt(derive.date_of(t + dur))})
            gate = fmeta.get(k, {}).get("available")
            if gate:
                self.add("info", "focus.gates", f"focus_path[{i}]",
                         f"{k} (planned {derive.fmt(derive.date_of(t))}) is available only when: {derive.render(gate)[:240]}")
            t += dur
            done.add(k)
        self.facts["focus"] = {"tree": tid, "timeline": timeline}
        self.focus_slot(path, fallback, tree, fmeta, days_per)

    def war_window(self):
        """(first day at war or None, day the plan expects the war over or None, enemy tags) from the plan:
        at war from the start when the history starts a war, until the last vp target date (else the goal
        deadline); a country at peace at the start gives no dates (when its wars begin is not in the plan,
        so war-dependent gates stay undecided)."""
        start = derive.day(self.start)
        at_start = set(self.world.at_war_with(self.tag)) if self.tag else set()
        enemies = at_start | set(self.enemies())
        if not at_start:
            return None, None, enemies
        ends = [derive.day(e["by"]) for e in self.targets() if e.get("by") and derive.day(e["by"]) is not None]
        end = max(ends) if ends else (derive.day(self.goal["deadline"]) if self.goal.get("deadline") else None)
        return start, end, enemies

    def focus_slot(self, path, fallback, tree, fmeta, days_per):
        """focus.available, focus.idle, focus.unconfirmed: the focus slot walked through the plan's own
        conditions (plan_gates.focus_schedule)."""
        if not self.tag or not tree:
            return
        start = derive.day(self.start)
        ends = [d for d, _ in self.horizon()]
        horizon = max(ends) if ends else start + 365
        war_start, war_end, enemies = self.war_window()
        flags = self.world.start_global_flags() | {f["v"] for f in self.world.country(self.tag)["flags"] if f.get("v")}
        dlcs = self.ctx.dlcs if self.ctx else ()
        own = set(derive.tag_states(self.game, self.ctx, self.tag, self.start))

        def facts_at(day, done):
            if war_start is not None:
                war = war_end is None or day < war_end
                # what the history sets at the start is taken to last while the starting war lasts
                held = flags if war else set()
            else:
                # a country at peace at the start can be drawn into a war the plan does not date
                war, held = None, (flags if day == start else set())
            return plan_gates.Facts(self.tag, dlcs, derive.fmt(derive.date_of(day)), war,
                                    enemies if war else (), done, held, own)

        changes = [d for d in (war_start, war_end) if d is not None]
        sched = plan_gates.focus_schedule(path, fallback, tree, fmeta, days_per, start, horizon, facts_at,
                                          self.meta.get("completed_focuses", []), changes)
        day = lambda d: derive.fmt(derive.date_of(int(d)))  # noqa: E731
        self.facts["focus"]["schedule"] = [dict(r, start=day(r["start"]), end=day(r["end"])) for r in sched["rows"]]
        self.facts["focus"]["idle"] = [dict(g, **{"from": day(g["from"]), "to": day(g["to"])}) for g in sched["idle"]]
        for n in sched["never"]:
            where = f"focus_{n['list']}"
            self.add("error", "focus.available", where,
                     f"{n['focus']} cannot start before the plan horizon {day(horizon)} under the plan's own "
                     f"conditions ({n['why']}): take it out or list it where its gate holds")
        for n in sched["waiting"]:
            self.add("warning", "focus.unconfirmed", f"focus_{n['list']}",
                     f"{n['focus']} waits for {' or '.join(n['on'])}, which only the game completes (by script, "
                     f"e.g. when a war is won): the plan cannot tell when, so it is no cover for the focus slot "
                     f"until then")
        for g in sched["idle"]:
            days = int(g["to"] - g["from"])
            if g["exhausted"]:
                if days > FOCUS_END_SLACK:
                    self.add("error", "focus.idle", "focus_path",
                             f"the focus lists run out on {day(g['from'])}, {days} days before the plan horizon "
                             f"{day(horizon)}: list focuses to keep the slot busy until then")
                continue
            if days <= FOCUS_IDLE_DAYS:
                continue
            at_war = war_start is not None and (war_end is None or g["from"] < war_end)
            why = "; ".join(f"{b['focus']}: {b['why']}" for b in g["blocked"][:6])
            self.add("error", "focus.idle", "focus_fallback" if fallback else "focus_path",
                     f"the focus slot is idle from {day(g['from'])} to {day(g['to'])} ({days} days"
                     f"{', while at war' if at_war else ''}): no listed focus can start then ({why}); list focuses "
                     f"whose prerequisites and `available` gate hold in that time")
        certain = {r["focus"] for r in sched["rows"] if r["gate"] is True}
        for r in sched["rows"]:
            if r["gate"] is None and not self.certain_alternative(r, path, fallback, tree, fmeta, certain):
                gate = derive.render(fmeta[r["focus"]]["available"])[:200]
                self.add("warning", "focus.unconfirmed", f"focus_{r['from']}",
                         f"from {day(r['start'])} the focus slot rests on {r['focus']}, whose gate the plan cannot "
                         f"confirm ({gate}), and no focus without a gate is listed after it: when the gate is "
                         f"closed in the game the slot idles; list one that is certainly available as a fallback")

    @staticmethod
    def certain_alternative(row, path, fallback, tree, fmeta, certain):
        """A listed focus with no `available` gate that is scheduled after `row` starts (so the slot has
        something to fall to when the uncertain gate is closed)."""
        listed = path + fallback
        later = listed[listed.index(row["focus"]) + 1:] if row["focus"] in listed else []
        return any(k in tree and not (fmeta.get(k) or {}).get("available") for k in later)

    # ------------------------------------------------------------------ construction
    def construction_lists(self):
        """(label prefix, construction_plan list) for the base plan and each override."""
        out = [("construction_plan", self.list_of("construction_plan"))]
        for key in ("phases", "contingencies"):
            for j, p in enumerate(self.list_of(key)):
                ov = p.get("overrides")
                if isinstance(ov, dict) and isinstance(ov.get("construction_plan"), list):
                    out.append((f"{key}[{j}].overrides.construction_plan",
                                [x for x in ov["construction_plan"] if isinstance(x, dict)]))
        return out

    def construction(self):
        game = self.game
        kinds = derive.building_kinds(game)
        enemies = self.enemies()
        front = derive.front_distance(game, self.ctx, self.start, enemies, self.k) if enemies else {}
        infra = derive.state_buildings(game, self.ctx, self.start)
        states = self.states()
        self.facts["front_states"] = {str(s): d for s, d in sorted(front.items())}

        def infra_of(sid):
            return sum(v for b, v in infra.get(sid, {}).items() if b in kinds["infrastructure"])

        def factory_checks(where, b, sid, infra_first):
            if sid in front:
                self.add("error", "construction.front", where,
                         f"{b} in state {sid}: {front[sid]} province step(s) from {'/'.join(enemies)}-held land "
                         f"(K={self.k})")
            lvl = infra_of(sid)
            if lvl <= LOW_INFRA and not infra_first:
                self.add("error", "construction.infrastructure", where,
                         f"{b} in state {sid} with infrastructure {lvl} and no infrastructure queued before it there")

        route = self.route_provinces()
        pstate = game.province_state()
        for prefix, items in self.construction_lists():
            infra_done = set()
            for i, e in enumerate(items):
                b = e.get("building")
                where = f"{prefix}[{i}] {e.get('id', e.get('order', ''))}".strip()
                sid = e.get("state", e.get("to_state"))
                when = e.get("when", "")
                w = parse_when(when) if isinstance(when, str) else ("text", when)
                if b in kinds["infrastructure"] and isinstance(sid, int):
                    infra_done.add(sid)
                if b in kinds["factory"] and isinstance(sid, int) and sid in states:
                    factory_checks(where, b, sid, sid in infra_done)
                if w[0] == "province" and route is not None and w[1] not in route:
                    self.add("error", "construction.trigger_route", where,
                             f"trigger province {w[1]} is on no vp target route")
                if b in kinds["railway"] and isinstance(e.get("path"), list):
                    path = _ints(e["path"])
                    nodes = self.rail_nodes()
                    if path and nodes and path[0] not in nodes:
                        near = derive.distances(game, [path[0]], NODE_STEPS)
                        alt = sorted((d, q) for q, d in near.items() if q in nodes and q != path[0])
                        self.add("error", "construction.railway_node", where,
                                 f"railway start {path[0]} (state {pstate.get(path[0])}) is no supply hub, port or "
                                 f"capital, nor a hub or port the plan builds: a railway runs from a node, so the "
                                 f"executor would start it elsewhere"
                                 + (f"; the nearest node is {alt[0][1]} (state {pstate.get(alt[0][1])}, "
                                    f"{alt[0][0]} step(s))" if alt else f"; no node within {NODE_STEPS} steps"))
                    adj = self.adjacency(straits=False)
                    for a, c in zip(path, path[1:]):
                        if adj and c not in adj.get(a, ()):
                            self.add("error", "construction.railway", where, f"path provinces {a} and {c} do not touch")
                    if w[0] != "text":
                        ctl = self.control_at(when)
                        miss = [p for p in path if p not in ctl]
                        if miss:
                            self.add("error", "construction.railway", where,
                                     f"path provinces {', '.join(f'{p} (state {pstate.get(p)})' for p in miss)} are not "
                                     f"controlled when '{when}' fires (start control plus the vp targets taken by then)")
                if b in kinds["supply"] and isinstance(sid, int) and w[0] != "text":
                    ctl = self.control_at(when)
                    prov = e.get("province")
                    held = [p for p in states.get(sid, {}).get("provinces", []) if p not in ctl]
                    if isinstance(prov, int) and prov not in ctl:
                        self.add("error", "construction.hub", where,
                                 f"hub province {prov} is not controlled when '{when}' fires")
                    elif held:
                        self.add("error", "construction.hub", where,
                                 f"state {sid} still has provinces held by others when '{when}' fires "
                                 f"({', '.join(map(str, held[:6]))}); the game refuses the hub until the state is clear")
        # legacy `construction` lists (filler by state)
        planned_infra = {e.get("state") for _, items in self.construction_lists() for e in items
                         if e.get("building") in kinds["infrastructure"]}
        legacy_infra = set()
        by_name = {}
        for sid, s in states.items():
            for n in s["names"]:
                by_name.setdefault(n, set()).add(sid)
        for i, e in enumerate(self.list_of("construction")):
            b = e.get("building")
            ids = []
            for s in e.get("states") or []:
                if isinstance(s, int) or (isinstance(s, str) and s.isdigit()):
                    ids.append(int(s))
                elif isinstance(s, str) and len(by_name.get(s, ())) == 1:
                    ids.append(next(iter(by_name[s])))
            for sid in ids:
                if b in kinds["infrastructure"]:
                    legacy_infra.add(sid)
                elif b in kinds["factory"] and sid in states:
                    factory_checks(f"construction[{i}]", b, sid, sid in legacy_infra or sid in planned_infra)

    def legs(self):
        """[(target index, entry, leg province list or None, problems)] for vp_targets in order."""
        if getattr(self, "_legs", None) is not None:
            return self._legs
        adj = self.adjacency()
        ctl = set(self.start_control())
        out = []
        for i, e in enumerate(self.targets()):
            problems = []
            path = _ints(e.get("path"))
            head = path[0] if path else e["province"]
            for a, c in zip(path + [e["province"]], (path + [e["province"]])[1:]):
                if c not in adj.get(a, ()) and a != c:
                    problems.append(f"path provinces {a} and {c} do not touch")
            anchors = [n for n in adj.get(head, ()) if n in ctl]
            leg = None
            if head in ctl:
                leg = path + [e["province"]]
            elif anchors:
                graph = self.game.map_graph()
                best = None
                for a in sorted(anchors):
                    cand = [a] + path + [e["province"]]
                    try:
                        n = len(derive_crossings(graph, cand))
                    except KeyError:
                        n = 0
                    if best is None or n < best[0]:
                        best = (n, cand)
                leg = best[1]
            else:
                bfs = self.bfs_route(ctl, head, adj)
                problems.append(f"{'path start ' if path else ''}{head} is not next to own land or an earlier target"
                                + (f" ({len(bfs) - 1} steps from it: {' '.join(map(str, bfs))})" if bfs else " (unreachable by land)"))
            out.append((i, e, leg, problems))
            ctl.update(self.captures(e))
        self._legs = out
        return out

    @staticmethod
    def bfs_route(sources, dst, adj):
        prev = {s: None for s in sources}
        q = deque(sources)
        while q:
            a = q.popleft()
            if a == dst:
                path = [a]
                while prev[path[-1]] is not None:
                    path.append(prev[path[-1]])
                return list(reversed(path))
            for b in adj.get(a, ()):
                if b not in prev:
                    prev[b] = a
                    q.append(b)
        return None

    def route_provinces(self):
        """Provinces on the vp target routes (targets, paths, from, legs), or None without targets."""
        targets = self.targets()
        if not targets:
            return None
        out = set()
        for e in targets:
            out.update(self.captures(e))
            out.update(_ints(e.get("from")))
        for _, _, leg, problems in self.legs():
            if leg:
                out.update(leg)
        # a non-adjacent target's BFS route counts as its route too
        ctl = set(self.start_control())
        adj = self.adjacency()
        for e in targets:
            r = self.bfs_route(ctl, e["province"], adj)
            if r:
                out.update(r)
            ctl.update(self.captures(e))
        return out

    # ------------------------------------------------------------------ production
    def production(self):
        game, plan = self.game, self.plan
        kinds = derive.building_kinds(game)
        sb = derive.state_buildings(game, self.ctx, self.start)
        own = derive.tag_states(game, self.ctx, self.tag, self.start)
        # two pools the game never mixes: military factories build land and air equipment, dockyards build
        # naval equipment (ships and convoys)
        pools = {"military": ("military factories", kinds["military"]), "dockyards": ("dockyards", kinds["naval"])}
        have, planned = {}, {}
        for pool, (_, buildings) in pools.items():
            have[pool] = sum(v for s in own for b, v in sb.get(s, {}).items() if b in buildings)
            planned[pool] = 0
            for _, items in self.construction_lists()[:1]:
                planned[pool] += sum(1 for e in items if e.get("building") in buildings)
            for e in self.list_of("construction"):
                if e.get("building") in buildings:
                    planned[pool] += int(e.get("count", 1) or 1) * max(1, len(e.get("states") or []))
        mils = have["military"]
        lines = self.list_of("production")
        start = derive.day(self.start)
        dates = sorted({start} | {derive.day(x["from"]) for x in lines if derive.day(x.get("from")) is not None})
        for d in dates:
            used = self.pool_lines(d)
            for pool, (label, _) in pools.items():
                n = sum(used[pool].values())
                cap = have[pool] if d == start else have[pool] + planned[pool]
                if n > cap:
                    extra = "" if d == start else f" plus {planned[pool]} planned"
                    what = ", ".join(f"{k} {v}" for k, v in sorted(used[pool].items()))
                    self.add("error", "production.factories", "production",
                             f"{n} {label} allocated from {derive.fmt(derive.date_of(d))} ({what}), but {self.tag} "
                             f"has {have[pool]} {label} at the start{extra}")
        self.facts["military_factories"] = {"start": mils, "planned": planned["military"]}
        self.facts["dockyards"] = {"start": have["dockyards"], "planned": planned["dockyards"]}
        equip = game.equipment()
        unlocks = derive.equipment_unlocks(game)
        have = start_tech_set(game, self.meta)
        by_year = {}
        for e in plan.get("research_path") or []:
            if isinstance(e, dict):
                y = e.get("year") if isinstance(e.get("year"), int) else 0
                for t in e.get("techs") or []:
                    by_year.setdefault(t, y)
        prio = {t for t in plan.get("research_priority") or [] if isinstance(t, str)}
        for i, x in enumerate(lines):
            k = x.get("equipment")
            if k not in equip:
                self.add("error", "production.equipment", f"production[{i}]", f"unknown equipment {k}")
                continue
            need = unlocks.get(k)
            if not need or need & have:
                continue
            planned_t = [(by_year.get(t, 0), t) for t in need if t in by_year or t in prio]
            if not planned_t:
                self.add("error", "production.equipment", f"production[{i}]",
                         f"{k} is unlocked by {', '.join(sorted(need))}, none at the start or in the research plan")
                continue
            year = derive.as_tuple(x.get("from") or self.start)[0]
            first = min(planned_t)
            if first[0] and first[0] > year:
                self.add("warning", "production.equipment", f"production[{i}]",
                         f"{k} from {year} but its tech {first[1]} is planned for {first[0]}")
        mot = self.war.get("motorization")
        if isinstance(mot, (int, float)) and mot > 0:
            types = derive.equipment_types(game)
            if not any("motorized" in types.get(x.get("equipment"), set()) for x in lines):
                self.add("error", "production.trucks", "production",
                         f"war.motorization is {mot} but no production line builds motorized equipment (trucks)")

    # ------------------------------------------------------------------ army
    def army(self):
        game = self.game
        layout = self.list_of("army_layout")
        enemies = self.enemies()
        borders = derive.border_states(game, self.ctx, self.start, self.tag, enemies) if enemies else set()
        guarded = set(_ints(self.war.get("guard_states")))
        g = self.war.get("guards")
        if isinstance(g, dict):
            guarded.update(v for v in g.values() if isinstance(v, int))
        for e in layout:
            guarded.update(_ints(e.get("covers")))
            if e.get("role") == "guard" or e.get("task_of"):
                guarded.update(_ints(e.get("guard_states_in_order")))
                if isinstance(e.get("state"), int) and (e.get("task_of") or not e.get("guard_states_in_order")):
                    guarded.add(e["state"])
        for sid in sorted(borders - guarded):
            self.add("error", "army.guards", "army_layout",
                     f"state {sid} borders {'/'.join(enemies)} and has no guard (a guard role in army_layout, "
                     f"war.guard_states or war.guards)")
        self.facts["border_states"] = sorted(borders)
        # a hold post replaces the layout guard post of its army
        posts = {}
        for e in layout:
            if e.get("role") == "guard" and isinstance(e.get("name_hint"), str):
                home = {e["state"]} if isinstance(e.get("state"), int) else set()
                posts[e["name_hint"]] = set(_ints(e.get("guard_states_in_order"))) or home
        pstate = game.province_state()
        for i, h in enumerate(self.list_of("hold_targets")):
            post = posts.get(h.get("army"))
            held = {pstate.get(p) for p in _ints(h.get("provinces"))} - {None}
            if post and held and not held & post:
                left = sorted(post & borders)
                if left:
                    self.add("warning", "army.guards", f"hold_targets[{i}] {h.get('id')}",
                             f"{h.get('army')!r} guards state(s) {', '.join(map(str, sorted(post)))} but its hold post "
                             f"is in state(s) {', '.join(map(str, sorted(held)))}; border state(s) "
                             f"{', '.join(map(str, left))} lose that guard while the hold runs")
        # templates and counts against the start OOB
        oobs = start_oobs(game, self.meta)
        pstate = game.province_state()
        templates = set()
        per_state = defaultdict(Counter)
        for o in oobs:
            ob = game.oob(o, self.ctx)
            templates |= ob["templates"]
            for d in ob["divisions"]:
                sid = pstate.get(d["location"])
                if sid is not None and d["template"]:
                    per_state[sid][d["template"]] += 1
        shared = defaultdict(list)  # state -> [(where, templates, count)] of the start entries
        for i, e in enumerate(layout):
            where = f"army_layout[{i}] {e.get('name_hint', '')!r}"
            ts = [t for t in e.get("templates") or [] if isinstance(t, str)]
            count = e.get("count", 1) if isinstance(e.get("count"), int) else 1
            if e.get("role") == "strike" and count > MAX_STRIKE:
                self.add("error", "army.strike_size", where, f"strike entry of {count} divisions; keep it at {MAX_STRIKE} or fewer")
            if "when" in e or not oobs:
                continue
            for t in ts:
                if t not in templates:
                    self.add("error", "army.templates", where, f"template {t!r} is not in the start OOB {', '.join(oobs)}")
            here = per_state.get(e.get("state"), Counter())
            n = sum(here.get(t, 0) for t in ts)
            if count > n:
                self.add("error", "army.counts", where,
                         f"count {count} but only {n} division(s) of {', '.join(ts)} start in state {e.get('state')}")
            shared[e.get("state")].append((where, ts, min(count, n)))
        for sid, rows in sorted(shared.items(), key=lambda kv: str(kv[0])):
            need = sum(c for _, _, c in rows)
            got = assignable(rows, per_state.get(sid, Counter()))
            if len(rows) > 1 and got < need:
                self.add("warning", "army.counts", ", ".join(w for w, _, _ in rows),
                         f"the entries of state {sid} need {need} divisions together, but only {got} can be "
                         f"assigned from the start divisions of their templates")
        armies = self.plan.get("armies") if isinstance(self.plan.get("armies"), dict) else {}
        if isinstance(armies.get("max"), int) and armies["max"] > MAX_STRIKE:
            self.add("error", "army.strike_size", "armies.max", f"armies.max {armies['max']} is above {MAX_STRIKE}")
        self.formations(layout, armies)
        self.generals(layout, armies)

    def formations(self, layout, armies):
        """army.formations and army.marshal: command concentrated in a few formations per theatre, each under
        a general, all under one field marshal; small roles are tasks inside a formation."""
        if armies.get("concentrate") is False:
            return
        cap = armies.get("max_per_group", MAX_FORMATIONS)
        least = armies.get("min_divisions", MIN_FORMATION)
        groups = self.list_of("army_group")
        def theatre(hint):
            if len(groups) < 2:
                return "the theatre"
            for g in groups:
                if hint in (g.get("armies") or []):
                    return f"army group {g.get('id')}"
            return None
        per = defaultdict(list)
        for i, e in enumerate(layout):
            if e.get("task_of") or not isinstance(e.get("name_hint"), str):
                continue
            where = f"army_layout[{i}] {e['name_hint']!r}"
            count = e.get("count", 1) if isinstance(e.get("count"), int) else 1
            if count < least and not e.get("general"):
                self.add("warning", "army.formations", where,
                         f"{count} division(s) without a general is a task, not an army: give it task_of (the formation "
                         f"whose front covers its state) so its divisions fight under a general who gains experience")
                continue
            t = theatre(e["name_hint"])
            if t is not None:
                per[t].append(e["name_hint"])
        for t, hints in per.items():
            if isinstance(cap, int) and cap > 0 and len(hints) > cap:
                self.add("warning", "army.formations", "army_layout",
                         f"{len(hints)} formations in {t} ({', '.join(hints)}); concentrate command in at most {cap} "
                         f"(armies.max_per_group) so each general leads enough divisions to learn and none sits idle")
        formations = [h for hints in per.values() for h in hints]
        if len(formations) >= 2 and not groups:
            self.add("warning", "army.marshal", "army_group",
                     f"{len(formations)} formations ({', '.join(formations)}) and no army group: one field marshal "
                     f"over the theatre gains experience from all of them and lends them his traits")
        for i, g in enumerate(groups):
            if not g.get("marshal"):
                self.add("info", "army.marshal", f"army_group[{i}]",
                         "no marshal named: the executor takes the best free field marshal")

    def commander_uses(self, layout, armies):
        """[(where, name, post, kind)]: post general | marshal | admiral; kind command (one post per person),
        fallback (a later marshal candidate) or pool (armies.generals)."""
        uses = []
        for i, n in enumerate(armies.get("generals") or []):
            uses.append((f"armies.generals[{i}]", n, "general", "pool"))
        for i, e in enumerate(layout):
            if isinstance(e.get("general"), str):
                uses.append((f"army_layout[{i}]", e["general"], "general", "command"))
        for i, e in enumerate(self.list_of("army_group")):
            m = e.get("marshal")
            names = [m] if isinstance(m, str) else [x for x in m if isinstance(x, str)] if isinstance(m, list) else []
            for j, name in enumerate(names):
                where = f"army_group[{i}].marshal" + (f"[{j}]" if isinstance(m, list) else "")
                uses.append((where, name, "marshal", "command" if j == 0 else "fallback"))
        nv = self.plan.get("navy") if isinstance(self.plan.get("navy"), dict) else {}
        for j, f in enumerate(nv.get("fleets") or []):
            if isinstance(f, dict) and isinstance(f.get("admiral"), str):
                uses.append((f"navy.fleets[{j}].admiral", f["admiral"], "admiral", "command"))
        ad = nv.get("admirals")
        rows = list(ad.items()) if isinstance(ad, dict) else [(r.get("fleet"), r.get("admiral")) for r in ad or []
                                                              if isinstance(r, dict)] if isinstance(ad, list) else []
        for fleet, name in rows:
            uses.append((f"navy.admirals[{fleet}]", name, "admiral", "command"))
        return uses

    def generals(self, layout, armies):
        table = derive.commanders(self.game, self.tag, self.ctx, self.start, self.meta.get("history_file"))
        commands = defaultdict(list)
        pool = Counter()
        for where, name, post, kind in self.commander_uses(layout, armies):
            if not isinstance(name, str):
                continue
            cid = match_or_none(name, table)
            if cid is None:
                self.add("error", "army.generals", where, f"{name!r} is not a commander of {self.tag} in common/characters")
                continue
            c = table[cid]
            if not c["recruited"]:
                self.add("warning", "army.generals", where, f"{name!r} ({cid}) is not recruited by the history at the start")
            if post == "marshal" and "field_marshal" not in c["roles"]:
                if "corps_commander" in c["roles"]:
                    self.add("info", "army.generals", where,
                             f"{name!r} ({cid}) is a general, not a field marshal: hoi4_army_commander promotes him "
                             f"(command power; the executor waits 7 game days while it is short)")
                else:
                    self.add("error", "army.generals", where, f"{name!r} ({cid}) has no field_marshal or "
                                                              f"corps_commander role")
            if post == "admiral" and "navy_leader" not in c["roles"]:
                self.add("error", "army.generals", where, f"{name!r} ({cid}) has no navy_leader role")
            if kind == "command":
                commands[(cid, post == "admiral")].append(where)
            elif kind == "pool":
                pool[cid] += 1
        for (cid, navy), wheres in commands.items():
            if len(wheres) > 1:
                what = "more than one fleet" if navy else "more than one army"
                self.add("error", "army.generals", ", ".join(wheres), f"{table[cid]['display']} commands {what}")
        for cid, n in pool.items():
            if n > 1:
                self.add("error", "army.generals", "armies.generals", f"{table[cid]['display']} is listed {n} times")

    # ------------------------------------------------------------------ conquest and goal
    def goal_states(self):
        """{state: kind} the goal must control (goal predicates, then the deprecated
        surrender_goal.must_control_state_at_end and win_condition.decision)."""
        out = {}
        for p in self.goal.get("predicates") or []:
            for a in _atoms(p):
                m = _PRED_STATE.match(a)
                if not m:
                    continue
                if m.group(4):
                    out[int(m.group(5))] = "full" if m.group(4) == "state_fully_controlled" else "control"
                elif m.group(3) == self.tag:
                    out[int(m.group(2))] = {"state_controller": "full", "owner": "owner"}.get(m.group(1), "control")
        sg = self.plan.get("surrender_goal")
        if isinstance(sg, dict) and isinstance(sg.get("must_control_state_at_end"), int):
            out.setdefault(sg["must_control_state_at_end"], "control")
        wc = self.plan.get("win_condition")
        if isinstance(wc, dict) and isinstance(wc.get("decision"), str):
            body = derive.decision_index(self.game).get(wc["decision"])
            if body is not None:
                for sid, key in derive.control_states(dict((k, v) for k, v in body if k).get("available")).items():
                    out.setdefault(sid, "full" if key == "has_full_control_of_state" else "control")
        return out

    def capitulation_targets(self):
        out = set()
        for p in self.goal.get("predicates") or []:
            for a in _atoms(p):
                m = _PRED_TAG.match(a)
                if m and m.group(1) == "capitulated":
                    out.add(m.group(2))
        if not out and isinstance(self.plan.get("surrender_goal"), dict):
            out.update(self.war.get("enemies") or [])
        return sorted(out)

    def conquest(self):
        game = self.game
        targets = self.targets()
        facts = game.state_facts(self.ctx, self.start)
        vps = {p: v for r in facts.values() for p, v in r["vps"].items()}
        owners = {sid: r["owner"] for sid, r in facts.items()}
        pstate = game.province_state()
        surr = {}
        for enemy in self.capitulation_targets():
            s = derive.surrender(game, enemy, self.ctx, self.start)
            surr[enemy] = s
            taken = set()
            for e in targets:
                taken.update(self.captures(e))
            got = sum(vps.get(p, 0) for p in taken if owners.get(pstate.get(p)) == enemy)
            s["planned_vp"] = got
            if got < s["vp_needed"]:
                mods = ", ".join(f"{n} {v:+g}" for n, v in s["modifiers"]) or "no starting modifier"
                ws, low = s["war_support"], ""
                if ws["at_zero"]["vp_needed"] < s["vp_needed"]:
                    low = (f"; low war support lowers it (to {ws['at_start']['vp_needed']} at the start value "
                           f"{ws['start'] * 100:g}%, {ws['at_zero']['vp_needed']} at 0%), but war support can rise "
                           f"to {ws['split'] * 100:g}% during the war, so the plan must reach {s['vp_needed']}")
                self.add("error", "conquest.threshold", "vp_targets",
                         f"targets hold {got:g} of {enemy}'s {s['total_vp']:g} VP; capitulation needs {s['vp_needed']} "
                         f"(limit {s['limit']:g}: base {s['base']:g}, {mods}){low}")
        self.facts["surrender"] = surr
        if surr:
            # rows on another country's state count nothing towards the surrender (owner at the start)
            for i, e in enumerate(targets):
                owner = owners.get(pstate.get(e.get("province")))
                if owner and owner not in surr and vps.get(e.get("province"), 0) > 0:
                    self.add("info", "conquest.threshold", f"vp_targets[{i}] {e['province']}",
                             f"state {pstate.get(e['province'])} is {owner}'s at the start: its "
                             f"{vps[e['province']]:g} VP do not count towards {', '.join(sorted(surr))}")
        self.conquest_skipped_near(targets, vps, owners, pstate)
        must = self.goal_states()
        self.facts["goal_states"] = {str(k): v for k, v in sorted(must.items())}
        start_ctl = self.start_control()
        for sid, kind in sorted(must.items()):
            provs = game.states().get(sid, {}).get("provinces", [])
            mine = [p for p in provs if p in start_ctl]
            tp = [e for e in targets if e.get("state") == sid or e["province"] in provs]
            if not tp and len(mine) < len(provs):
                owner = owners.get(sid)
                declared = {str(d.get("target")) for d in self.list_of("diplomacy")
                            if isinstance(d, dict) and d.get("action") == "declare"}
                at_war = set(self.war.get("enemies") or []) | set(self.capitulation_targets()) | declared
                if owner in at_war:
                    self.add("error", "conquest.mandatory", "vp_targets",
                             f"the goal needs state {sid} ({kind} control) but no vp target is in it")
                else:
                    # A state of a country the plan does not fight (annexed by decision, demand or
                    # puppet): no vp target is needed, but the route must be named somewhere.
                    self.add("warning", "conquest.mandatory", "vp_targets",
                             f"the goal needs state {sid} ({kind} control) of {owner}, which the plan does not "
                             f"fight: name the decision or diplomacy item that acquires it")
            elif kind == "full":
                covered = set(mine)
                for e in targets:
                    covered.update(self.captures(e))
                left = [p for p in provs if p not in covered]
                if left:
                    self.add("warning", "conquest.mandatory", "vp_targets",
                             f"state {sid} must be fully controlled; provinces {', '.join(map(str, left))} are on no target or path")
        graph = game.map_graph()
        last_by = {}
        open_tags = self.passable_tags()
        later = self.predicate_tags()
        for i, e, leg, problems in self.legs():
            where = f"vp_targets[{i}] {e['province']}"
            for msg in problems:
                self.add("error", "conquest.adjacency", where, msg)
            neutral = defaultdict(list)
            for p in dict.fromkeys((leg or []) + self.captures(e)):
                o = owners.get(pstate.get(p))
                if open_tags is not None and o and o not in open_tags:
                    neutral[o].append(p)
            for o, ps in sorted(neutral.items()):
                self.add("warning" if o in later else "error", "conquest.neutral_land", where,
                         f"the leg crosses {o}'s land ({' '.join(map(str, ps))}), which {self.tag} neither fights "
                         f"nor is allied to at the start: no unit crosses a neutral border"
                         + (f"; a plan predicate names {o}, so the leg must wait for it" if o in later else
                            f"; route around it (facts route keeps to open land), or name {o} in war.enemies or in "
                            f"the predicate (not_exists:{o}, capitulated:{o}, at_war_with:{o}) that opens its land"))
            if leg and graph is not None:
                try:
                    n = len(derive_crossings(graph, leg))
                except KeyError:
                    n = 0
                if n > MAX_RIVERS:
                    self.add("warning", "conquest.rivers", where, f"{n} river crossings on the leg {' '.join(map(str, leg))}")
            army = e.get("army", "")
            d = derive.day(e.get("by")) if e.get("by") else None
            if d is not None:
                if army in last_by and d < last_by[army][0]:
                    self.add("warning", "conquest.by_order", where,
                             f"by {e['by']} is before the previous target of {army or 'any army'} ({last_by[army][1]})")
                last_by[army] = (d, e["by"])

    def conquest_skipped_near(self, targets, vps, owners, pstate):
        """(warning) an enemy VP province next to own land or to a planned target is never taken,
        while the plan takes targets farther from its start front (the w-run critic finding: Gondar,
        2 VP one step from Eritrea, skipped while the threshold rested on far-western targets)."""
        enemies = set(self.capitulation_targets())
        if not enemies or not targets:
            return
        adj = self.adjacency()
        start = set(self.start_control())
        taken = set()
        for e in targets:
            taken.update(self.captures(e))
        near_of = {}
        for p in start | taken:
            for n in adj.get(p, ()):
                near_of.setdefault(n, p)
        dist = {}
        frontier, seen, step = list(start), set(start), 0
        while frontier and step < 40:
            step += 1
            nxt = []
            for a in frontier:
                for b in adj.get(a, ()):
                    if b not in seen:
                        seen.add(b)
                        dist[b] = step
                        nxt.append(b)
            frontier = nxt
        far = max((dist.get(e["province"], 0) for e in targets), default=0)
        for prov, vp in sorted(vps.items()):
            if vp < 1 or prov in taken or prov in start or owners.get(pstate.get(prov)) not in enemies:
                continue
            if prov in near_of and far > dist.get(prov, 0) + 1:
                self.add("warning", "conquest.skipped_near", f"province {prov}",
                         f"{vp:g} VP province {prov} (state {pstate.get(prov)}) touches {near_of[prov]} (own or a target) "
                         f"but is never targeted, while targets reach {far} steps from the start front; take it on the "
                         f"way or say why not (cost, river, mission)")

    def win(self):
        preds = self.goal.get("predicates")
        if not preds:
            old = [k for k in ("surrender_goal", "win_condition") if k in self.plan]
            self.add("error", "win.goal", "goal",
                     "no goal.predicates" + (f" (only the deprecated {', '.join(old)}, which nothing evaluates)" if old else ""))
        horizon = derive.day(self.stop) if self.stop else derive.day(self.start) + 730
        timers = [t for t in derive.scenario_timers(self.game, self.tag, self.ctx, self.start, self.meta.get("history_file"))
                  if derive.day(t["earliest"]) <= horizon]
        self.facts["timers"] = timers
        if not timers:
            return
        dl = self.goal.get("deadline")
        names = "; ".join(f"{t['mission']} {t['earliest']}..{t['latest']}" for t in timers)
        if dl:
            d = derive.day(dl)
            if not any(derive.day(t["earliest"]) - 3 <= d <= derive.day(t["latest"]) + 3 for t in timers):
                self.add("error", "win.deadline", "goal.deadline", f"deadline {dl} matches no scenario timer ({names})")
        else:
            self.add("info", "win.deadline", "goal", f"scenario timers in the plan window: {names}")

    # ------------------------------------------------------------------ events and schema
    def events(self):
        for i, e in enumerate(self.list_of("event_rules")):
            ev = self.game.event(e.get("event", ""))
            if ev is None:
                self.add("error", "events.options", f"event_rules[{i}]", f"no event {e.get('event')} in events/")
                continue
            names = [n for n in ev["options"] if n]
            for key in ("option_name", "fallback_option_name", "else_option_name"):
                if key in e and e[key] not in names:
                    self.add("error", "events.options", f"event_rules[{i}]",
                             f"{key} {e[key]} is not an option of {e['event']} ({', '.join(names)})")
        eo = self.plan.get("event_options")
        if isinstance(eo, dict):
            for eid, idx in eo.items():
                ev = self.game.event(eid)
                if ev is None:
                    self.add("error", "events.options", f"event_options.{eid}", "no such event")
                elif isinstance(idx, int) and idx >= len(ev["options"]):
                    self.add("error", "events.options", f"event_options.{eid}",
                             f"index {idx} but the event has {len(ev['options'])} options")

    def schema_rules(self):
        if self.schema is None:
            self.add("warning", "schema.keys", "", "plan.schema.json not found; schema rules skipped")
            return
        for path, msg in schema_check.validate(self.plan, self.schema):
            self.add("error", "schema.keys", path or "(plan)", msg)
        for key, spec in (self.schema.get("properties") or {}).items():
            if spec.get("x-owner") != "player" or not isinstance(self.plan.get(key), list):
                continue
            props = ((spec.get("items") or {}).get("properties") or {})
            if "call" not in props:
                continue
            items = [x for x in self.plan[key] if isinstance(x, dict)]
            missing = [j for j, x in enumerate(items) if "call" not in x]
            if missing:
                self.add("error", "schema.player_call", key,
                         f"{len(missing)} of {len(items)} player items have no call (the tool call they stand for): "
                         f"indexes {', '.join(map(str, missing))}")

        # Keys the plan sets that the executor does not act on: the plan must not read as if it did.
        unread = plan_keys.unread_in(self.plan, self.schema)
        for kind, paths in sorted(unread.items()):
            what = ("only the planning tools read them (they shape the estimates above; the executor does not "
                    "act on them)" if kind == "lint" else
                    "no executor step reads them yet: the player acts on them from the brief, or they do nothing")
            self.add("info", "schema.lint_keys", kind, f"{len(paths)} key(s) the executor does not act on: "
                     f"{', '.join(paths)}: {what}")
        t = self.list_of("vp_targets")
        if t and all(isinstance(e.get("order"), int) for e in t):
            listed = [e["order"] for e in t]
            if listed != sorted(listed):
                self.add("warning", "schema.lint_keys", "vp_targets",
                         "`order` differs from the list order: the executor takes the targets in list order, the "
                         "estimates here follow `order`; write the list in the planned order")

    # ------------------------------------------------------------------ coverage-build helpers
    @property
    def table(self):
        return predicate_table(self.schema)

    def active_lines(self, d):
        """{equipment: factories} of the production lines in force on day d (remove and replace
        entries applied in date order)."""
        start = derive.day(self.start)
        rows = sorted(enumerate(self.list_of("production")),
                      key=lambda ix: ((derive.day(ix[1].get("from")) or start), ix[0]))
        out = {}
        for _, x in rows:
            if (derive.day(x.get("from")) or start) > d:
                continue
            k = x.get("equipment")
            if x.get("remove"):
                out.pop(k, None)
            elif "replace" in x:
                if k in out:
                    out[x["replace"]] = out.get(x["replace"], 0) + out.pop(k)
            elif isinstance(x.get("factories"), int):
                out[k] = out.get(k, 0) + x["factories"]
        return out

    def factory_pool(self, eq):
        """Which factories build an equipment: "dockyards" for naval equipment (ships, convoys), else
        "military"."""
        return "dockyards" if derive.equipment_domain(self.game, eq) == "navy" else "military"

    def pool_lines(self, d):
        """{"military": {equipment: factories}, "dockyards": {...}} of the plan lines in force on day d."""
        out = {"military": {}, "dockyards": {}}
        for eq, n in self.active_lines(d).items():
            out[self.factory_pool(eq)][eq] = n
        return out

    def unlisted_in_time_techs(self, last_year):
        """Techs the country could research by `last_year` without an ahead-of-time penalty that the plan does
        not list: not owned at the start, start_year up to last_year, not excluded by a listed or owned
        either-or tech, outside the naval and air folders unless the plan lists a tech of that folder.
        Industry, production and construction folders first."""
        if not self.tag:
            return []
        first = derive.as_tuple(self.start)[0]
        planned = {t for e in self.plan.get("research_path") or [] if isinstance(e, dict) for t in e.get("techs") or []}
        planned |= {t for t in self.plan.get("research_priority") or [] if isinstance(t, str)}
        have = start_tech_set(self.game, self.meta)
        rows = [r for block in researchmod.calendar(self.world, self.tag, self.start, (min(first, last_year), last_year))
                for r in block["techs"]]
        # techs of earlier years the country does not have yet count too
        early = researchmod.calendar(self.world, self.tag, self.start, (1900, first - 1)) if first > 1900 else []
        rows = [r for block in early for r in block["techs"]] + rows
        known = researchmod.techs(self.world)
        fams = {folder_family(((known.get(t) or {}).get("folders") or [""])[0]) for t in planned} - {None}
        # an either-or tech whose rival is listed or owned is closed, and so is everything behind it
        closed = {r["id"] for r in rows if any(x in planned or x in have for x in r.get("xor") or [])}
        closed |= {x for t in planned | have for x in (known.get(t) or {}).get("xor") or []} - planned - have
        out = []
        for r in rows:
            tid, folder = r["id"], r.get("folder") or ""
            if tid in planned or tid in have or tid in closed:
                continue
            if closed & set(r.get("chain_any") or r.get("chain_all") or []):
                continue
            fam = folder_family(folder)
            if fam and fam not in fams:
                continue
            out.append((0 if re.search(r"industry|production|construction", folder) else 1, folder, tid))
        return [tid for _, _, tid in sorted(out)]

    def has_path(self, dotted):
        node = self.plan
        for part in dotted.split("."):
            if not isinstance(node, dict) or part not in node:
                return False
            node = node[part]
        return True

    def horizon(self):
        """[(day, label)] the plan must finish by: goal.deadline and the plan horizon."""
        out = []
        if self.goal.get("deadline") and derive.day(self.goal["deadline"]) is not None:
            out.append((derive.day(self.goal["deadline"]), "goal deadline"))
        if self.stop and derive.day(self.stop) is not None:
            out.append((derive.day(self.stop), "plan horizon"))
        return out

    def trigger_day(self, when):
        """Day a construction trigger fires: its date, the `by` of the vp target that takes the
        province (or the last one in the state), else the start."""
        w = parse_when(when) if isinstance(when, str) or when is None else ("text", str(when))
        start = derive.day(self.start)
        if w[0] in ("date", "date_prefix"):
            return max(start, derive.day(w[1]))
        by = [(e, derive.day(e["by"])) for e in self.targets() if e.get("by") and derive.day(e["by"]) is not None]
        if w[0] == "province":
            hit = [d for e, d in by if w[1] in self.captures(e)]
            if hit:
                return max(start, hit[0])
        elif w[0] == "state":
            hit = [d for e, d in by if e.get("state") == w[1]]
            if hit:
                return max(start, max(hit))
        return start

    def rail_nodes(self):
        """{province: kind} a railway can start from: the map's hubs and naval bases, the capital, and the
        hubs and ports the construction plans build at a province."""
        if getattr(self, "_rail_nodes", None) is not None:
            return self._rail_nodes
        kinds = derive.building_kinds(self.game)
        costs = derive.building_costs(self.game)
        out = dict(derive.railway_nodes(self.game, self.tag, self.ctx, self.start, self.meta.get("history_file")))
        for _, items in self.construction_lists():
            for e in items:
                b, prov = e.get("building"), e.get("province")
                if isinstance(prov, int) and (b in kinds["supply"] or (costs.get(b) or {}).get("port")):
                    out.setdefault(prov, "planned")
        self._rail_nodes = out
        return out

    def supply_axis_reach(self):
        """supply.axis_reach: estimated supply at each axis state's main province from the best node held by
        then (start nodes under start control, planned hubs and ports, nodes of earlier axis states)."""
        game = self.game
        graph = game.map_graph()
        nodes = self.rail_nodes()
        if graph is None or not nodes:
            return
        gstates = graph.get("states", {})
        start_ctl = set(self.start_control())
        # supply runs over land the country may enter, never across a neutral border
        open_tags, own = self.passable_tags(), self.world.owner_of_province
        can_enter = (lambda p: own(p) is None or own(p) in open_tags) if open_tags is not None else None
        wars = [("war", self.war)] + [(f"war_by_phase.{k}", p) for k, p in
                                      sorted((self.plan.get("war_by_phase") or {}).items()) if isinstance(p, dict)]
        # a phase can carry its own axes in overrides.war; those advances need supply as much as the base ones
        for i, phase in enumerate(self.plan.get("phases") or []):
            war = ((phase or {}).get("overrides") or {}).get("war") if isinstance(phase, dict) else None
            if isinstance(war, dict) and war.get("axes"):
                wars.append((f"phases[{i}].overrides.war", war))
        rows, seen = [], set()
        for label, war in wars:
            mot = war.get("motorization", self.war.get("motorization", 0))
            mot = mot if isinstance(mot, int) else 0
            for j, axis in enumerate(war.get("axes") or []):
                held = {p: k for p, k in nodes.items() if p in start_ctl or k == "planned"}
                for sid in _ints(axis):
                    st = gstates.get(sid) or {}
                    vps = st.get("victory_points") or {}
                    provs = st.get("provinces") or []
                    target = max(vps, key=lambda p: (vps[p], -p)) if vps else (provs[0] if provs else None)
                    if target is None or sid in seen:
                        continue
                    seen.add(sid)
                    best = None
                    if target in held:
                        best = (9.9, target, held[target], 0)
                    else:
                        near = derive.distances(game, [target], REACH_STEPS)
                        for d, q in sorted((d, q) for q, d in near.items() if q in held)[:REACH_SOURCES]:
                            r = mapgraph.route(graph, q, target, allowed=can_enter)
                            if not r:
                                continue
                            kind = "hub" if held[q] == "planned" else held[q]
                            v = derive.supply_reach(game, r["path"], kind, mot)
                            if best is None or v > best[0]:
                                best = (v, q, held[q], r["steps"])
                    where = f"{label}.axes[{j}] state {sid}"
                    if best is None:
                        self.add("error", "supply.axis_reach", where,
                                 f"no supply node held or planned within {REACH_STEPS} steps of province {target}: "
                                 f"armies there starve; build a port, a railway to a node or a hub first, or "
                                 f"stop the axis short")
                        rows.append({"where": where, "province": target, "reach": None})
                    else:
                        v, q, kind, steps = best
                        rows.append({"where": where, "province": target, "reach": v, "from": q, "node": kind,
                                     "steps": steps, "motorization": mot})
                        if v < REACH_OK:
                            self.add("warning", "supply.axis_reach", where,
                                     f"estimated supply flow {v:g} at province {target} from the {kind} at {q} "
                                     f"({steps} step(s), motorization {mot}); below {REACH_OK:g} the armies there "
                                     f"run short: pace the axis to the supply, add a port or railway to a node "
                                     f"nearer, or raise motorization")
                    for p in list(st.get("naval_bases") or {}) + [q for q in provs if nodes.get(q) == "hub"]:
                        held.setdefault(p, nodes.get(p, "port"))
        if rows:
            self.facts["supply_reach"] = rows

    def axis_states(self):
        out = set()
        for war in [self.war] + [p for p in (self.plan.get("war_by_phase") or {}).values() if isinstance(p, dict)]:
            for axis in war.get("axes") or []:
                out.update(_ints(axis))
        return out

    def capturable(self):
        """Provinces of the states the plan attacks (vp target states and war axes)."""
        states = self.states()
        out = set()
        for sid in {e.get("state") for e in self.targets()} | self.axis_states():
            out.update(states.get(sid, {}).get("provinces", []))
        return out

    def army_states(self):
        """States the plan keeps armies in: army_layout, armies.states, war.theatre, training
        deploy states, sea_transfer destinations."""
        out = {e["state"] for e in self.list_of("army_layout") if isinstance(e.get("state"), int)}
        armies = self.plan.get("armies") if isinstance(self.plan.get("armies"), dict) else {}
        out.update(_ints(armies.get("states")))
        out.update(_ints(self.war.get("theatre")))
        tr = self.plan.get("training")
        for line in tr if isinstance(tr, list) else [tr]:
            if isinstance(line, dict) and isinstance(line.get("deploy_state"), int):
                out.add(line["deploy_state"])
        st = self.plan.get("sea_transfer") if isinstance(self.plan.get("sea_transfer"), dict) else {}
        out.update(r["to_state"] for r in st.get("routes") or [] if isinstance(r, dict) and isinstance(r.get("to_state"), int))
        return out

    def overseas(self):
        if getattr(self, "_overseas", None) is None:
            self._overseas = derive.overseas_states(self.game, self.ctx, self.tag, self.army_states(), self.start,
                                                    self.meta.get("history_file")) or set()
        return self._overseas

    def queued(self):
        """[(where, building, state)] of construction_plan (base and overrides) and construction."""
        out = []
        for prefix, items in self.construction_lists():
            for i, e in enumerate(items):
                if isinstance(e.get("state"), int):
                    out.append((f"{prefix}[{i}]", e.get("building"), e["state"]))
        for i, e in enumerate(self.list_of("construction")):
            for s in self.state_ids(e.get("states")):
                out.extend([(f"construction[{i}]", e.get("building"), s)] * max(1, int(e.get("count", 1) or 1)))
        return out

    def state_ids(self, items):
        by_name = {}
        for sid, s in self.states().items():
            for n in s["names"]:
                by_name.setdefault(n, set()).add(sid)
        out = []
        for s in items or []:
            if isinstance(s, int) or (isinstance(s, str) and s.isdigit()):
                out.append(int(s))
            elif isinstance(s, str) and len(by_name.get(s, ())) == 1:
                out.append(next(iter(by_name[s])))
        return out

    def convoy_item(self, manage_counts=False):
        """True when a production line builds convoys (or, with manage_counts, manage.convoys_free is set)."""
        types = derive.equipment_types(self.game)
        if any("convoy" in types.get(x.get("equipment"), set()) and not x.get("remove")
               for x in self.list_of("production")):
            return True
        if manage_counts:
            m = self.plan.get("manage") if isinstance(self.plan.get("manage"), dict) else {}
            v = m.get("convoys_free")
            return v is not None and v is not False and not (isinstance(v, dict) and v.get("on") is False)
        return False

    def planned_techs(self):
        """{tech: year} planned in research_path (its year) and research_priority (0: at once)."""
        out = {}
        for e in self.list_of("research_path"):
            y = e.get("year") if isinstance(e.get("year"), int) else 0
            for t in e.get("techs") or []:
                out.setdefault(t, y)
        for t in self.plan.get("research_priority") or []:
            if isinstance(t, str):
                out.setdefault(t, 0)
        return out

    # ------------------------------------------------------------------ coverage-build rules
    def coverage(self):
        spec = ((self.schema or {}).get("properties") or {}).get("_coverage") or {}
        names = {c: s.get("description", c) for c, s in (spec.get("properties") or {}).items()}
        if not names:
            return
        cov = self.plan.get("_coverage")
        ids = list(names)
        if not isinstance(cov, dict):
            self.add("error", "coverage.checklist", "_coverage",
                     f"no _coverage object: map each checklist id {ids[0]}..{ids[-1]} to key:<plan key>, call, "
                     f"md:<plan.md section> or na:<reason>")
            return
        missing = [c for c in ids if c not in cov]
        if missing:
            self.add("error", "coverage.checklist", "_coverage",
                     f"checklist items without an entry: {', '.join(f'{c} ({names[c]})' for c in missing)}")
        for cid in ids:
            v = cov.get(cid)
            if not isinstance(v, str):
                continue
            where, label = f"_coverage.{cid}", f"{cid} ({names[cid]})"
            if v.startswith("key:"):
                for k in (x.strip() for x in v[4:].split(",")):
                    if not self.has_path(k):
                        self.add("error", "coverage.checklist", where, f"{label}: key {k} is not in the plan")
            elif v == "call" or v.startswith("call:"):
                key = v[5:] or None
                pool = self.plan.get(key) if key else self.plan
                if not _has_call(pool):
                    self.add("error", "coverage.checklist", where,
                             f"{label}: {v} but {'plan.' + key if key else 'the plan'} has no item with a call")
            elif v.startswith("na:"):
                reason = v[3:].strip()
                if not reason:
                    self.add("error", "coverage.checklist", where, f"{label}: na needs a reason")
                elif reason.lower().startswith("missing"):
                    self.add("error", "coverage.checklist", where, f"{label} is a gap, not a reason: {v}")
            elif not v.startswith("md:"):
                self.add("error", "coverage.checklist", where, f"{label}: {v!r} is not key:, call, md: or na:")

    def posture(self):
        fp = self.plan.get("force_posture")
        if not isinstance(fp, dict):
            return
        start = derive.day(self.start)
        dates = {}
        for p in self.list_of("phases"):
            ds = predicate_dates(p["enter"], self.table) if p.get("enter") is not None else []
            if ds:
                dates[str(p.get("id"))] = derive.day(max(ds))
        domain = lambda k: derive.equipment_domain(self.game, k) or "land"  # noqa: E731
        rows = []
        for j, ph in enumerate(fp.get("phases") or []):
            if not isinstance(ph, dict):
                continue
            where, name = f"force_posture.phases[{j}]", ph.get("phase")
            want = {k: ph.get(k) for k in ("land", "air", "navy") if isinstance(ph.get(k), (int, float))}
            if len(want) < 3:
                continue
            if abs(sum(want.values()) - 100) > 1:
                self.add("error", "posture.shares", where, f"land + air + navy is {sum(want.values()):g}, not 100")
                continue
            d = start if name == "base" else dates.get(name)
            if d is None:
                self.add("info", "posture.shares", where, f"phase {name} has no enter date; shares not compared")
                continue
            finishing = self.finishing_lines(d)
            lines = {eq: n for eq, n in self.active_lines(d).items() if eq not in finishing}
            total = sum(lines.values())
            if not total:
                continue
            got = {k: 0.0 for k in want}
            for eq, n in lines.items():
                got[domain(eq)] += 100.0 * n / total
            rows.append({"phase": name, "date": derive.fmt(derive.date_of(d)), "plan": want,
                         "production": {k: round(v, 1) for k, v in got.items()},
                         "finishing": {eq: derive.fmt(derive.date_of(f)) for eq, f in finishing.items()}})
            off = [f"{k} {got[k]:.0f}% vs {want[k]:g}%" for k in want if abs(got[k] - want[k]) > POSTURE_POINTS]
            if off:
                self.add("warning", "posture.shares", where,
                         f"production on {derive.fmt(derive.date_of(d))} ({total} factories) differs from the posture "
                         f"by more than {POSTURE_POINTS} points: {', '.join(off)}")
        self.facts["posture"] = rows

    def navy(self):
        coastal = derive.coastal_states(self.game, self.ctx, self.tag, self.start) or set()
        has_navy = isinstance(self.plan.get("navy"), dict)
        if coastal and not has_navy:
            self.add("error", "navy.required", "navy",
                     f"{self.tag} owns {len(coastal)} coastal state(s) ({', '.join(map(str, sorted(coastal)[:8]))}"
                     f"{' ...' if len(coastal) > 8 else ''}) and the plan has no navy key; give a naval plan or "
                     f"{{\"posture\": \"none\", \"reason\": ...}}")
        over = self.overseas()
        self.facts["overseas_states"] = sorted(over)
        if not over:
            return
        st = self.plan.get("sea_transfer") if isinstance(self.plan.get("sea_transfer"), dict) else {}
        ports = {b for b, r in derive.building_costs(self.game).items() if r["port"]}
        port_plan = (set(_ints(st.get("ports"))) & over) or {s for _, b, s in self.queued() if b in ports and s in over}
        convoys = self.convoy_item() or (isinstance(st.get("convoys"), int) and st["convoys"] > 0)
        missing = [m for m, ok in (("a navy key", has_navy),
                                   ("a convoy plan (a convoy production line or sea_transfer.convoys)", convoys),
                                   ("a port plan (sea_transfer.ports or a port building queued there)", port_plan))
                   if not ok]
        if missing:
            self.add("error", "navy.overseas", "navy",
                     f"armies stand in states {', '.join(map(str, sorted(over)))}, which {self.tag}'s capital cannot "
                     f"reach over its own land (supply and reinforcements go by sea); missing: {'; '.join(missing)}")

    def port_convoys(self):
        ports = {b for b, r in derive.building_costs(self.game).items() if r["port"]}
        built = sorted({s for _, b, s in self.queued() if b in ports})
        st = self.plan.get("sea_transfer") if isinstance(self.plan.get("sea_transfer"), dict) else {}
        relied = sorted(set(_ints(st.get("ports"))) | self.overseas())
        if not built and not relied:
            return
        if self.convoy_item(manage_counts=True):
            return
        factor = derive.define(self.game, "SUPPLY_CONVOY_FACTOR", 0.25)
        why = []
        if built:
            why.append(f"builds a port in state(s) {', '.join(map(str, built))}")
        if relied:
            why.append(f"supplies state(s) {', '.join(map(str, relied))} by sea")
        self.add("error", "supply.port_convoys", "production",
                 f"the plan {' and '.join(why)} but has no convoy item (a convoy production line or a "
                 f"manage.convoys_free threshold); sea supply needs {factor:g} convoys per supply "
                 f"(SUPPLY_CONVOY_FACTOR)")

    def full_define(self, key, default):
        """A define by its full key ("NCountry.SUPPLY_CONVOY_FACTOR"), mods included; default when the
        files do not hold a number for it."""
        v, _ = self.world.define(key)
        return float(v) if isinstance(v, (int, float)) and not isinstance(v, bool) else default

    def division_supply(self, template):
        """Daily supply use of one division of a start template (the sum of its sub-units'
        supply_consumption, at least MIN_SUPPLY_CONSUMPTION), None when the template is unknown."""
        t = self.start_templates().get(template)
        if not t:
            return None
        units = self.world.sub_units()

        def use(u):
            return (units.get(u, {}).get("stats") or {}).get("supply_consumption") or 0.0

        total = sum(n * use(u) for u, n in t["regiments"].items())
        total += sum(use(u) for u in list(t["support"]) + list(t.get("regimental_support") or []))
        return max(total, self.full_define("NMilitary.MIN_SUPPLY_CONSUMPTION", 0.0))

    def sea_supply(self):
        """[{"state", "divisions", "supply"}] of the states supplied by sea: the start divisions standing
        there, raised to the army_layout count of the state (extra divisions at the mean use of the
        entry's templates), and the divisions sea_transfer.routes bring."""
        if not self.tag:
            return []
        start = defaultdict(list)
        for d in self.world.oob(self.tag)["divisions"]:
            if d.get("state") is not None:
                start[d["state"]].append(self.division_supply(d["template"]) or 0.0)
        over = derive.overseas_states(self.game, self.ctx, self.tag, set(start) | self.army_states(), self.start,
                                      self.meta.get("history_file")) or set()
        every = [x for uses in start.values() for x in uses if x]
        mean_all = sum(every) / len(every) if every else 0.0

        def mean(templates):
            uses = [u for u in (self.division_supply(t) for t in templates or [] if isinstance(t, str)) if u]
            return sum(uses) / len(uses) if uses else mean_all

        extra = defaultdict(list)  # state: [(count, mean use)]
        for e in self.list_of("army_layout"):
            if isinstance(e.get("state"), int) and isinstance(e.get("count"), int):
                extra[e["state"]].append((e["count"], mean(e.get("templates"))))
        st = self.plan.get("sea_transfer") if isinstance(self.plan.get("sea_transfer"), dict) else {}
        moved = defaultdict(float)
        for r in st.get("routes") or []:
            if isinstance(r, dict) and isinstance(r.get("to_state"), int) and isinstance(r.get("count"), int):
                moved[r["to_state"]] += r["count"] * mean(r.get("templates"))
        out = []
        for sid in sorted(over | (set(moved) & over)):
            uses = list(start.get(sid, []))
            planned = sum(n for n, _ in extra.get(sid, []))
            supply = sum(uses)
            divisions = len(uses)
            if planned > divisions:
                layout_mean = sum(n * u for n, u in extra[sid]) / planned
                supply += (planned - divisions) * layout_mean
                divisions = planned
            if moved.get(sid):
                supply += moved[sid]
            if divisions or moved.get(sid):
                out.append({"state": sid, "divisions": divisions, "supply": round(supply, 2)})
        return out

    def sea_trade_units(self):
        """(units the plan imports, [what was counted], [what could not be counted])."""
        units, counted, unknown = 0.0, [], []
        ai = self.plan.get("auto_import")
        budget = ai.get("budget") if isinstance(ai, dict) else ai
        on = ai.get("enabled", True) if isinstance(ai, dict) else ai not in (None, False)
        if on and isinstance(budget, int) and not isinstance(budget, bool):
            units += budget * TRADE_UNITS_PER_FACTORY
            counted.append(f"auto_import budget {budget} factories x {TRADE_UNITS_PER_FACTORY} units")
        elif on and ai is not None:
            unknown.append("auto_import without a budget")
        tr = self.plan.get("trade")
        deals = tr.get("deals") if isinstance(tr, dict) else tr
        for i, deal in enumerate(deals if isinstance(deals, list) else []):
            if not isinstance(deal, dict):
                continue
            amount = deal.get("amount")
            if isinstance(amount, (int, float)) and not isinstance(amount, bool):
                units += amount
                counted.append(f"trade deal {deal.get('id', i)} {amount:g} {deal.get('resource', 'units')}")
            else:
                unknown.append(f"trade deal {deal.get('id', i)} amount {amount or 'needed'}")
        return units, counted, unknown

    def convoy_size(self):
        if not self.tag:
            return
        rows = self.sea_supply()
        units, counted, unknown = self.sea_trade_units()
        if not rows and not units:
            return
        factor = self.full_define("NCountry.SUPPLY_CONVOY_FACTOR", None)
        if factor is None:
            return  # no such define in these game files: nothing to size with
        range_factor = self.full_define("NCountry.CONVOY_RANGE_FACTOR", 1.0)
        supply = sum(r["supply"] for r in rows)
        for_supply, for_trade = supply * factor, units * TRADE_CONVOYS_PER_UNIT
        need = math.ceil(for_supply + for_trade - 1e-9)
        f = self.world.country(self.tag)["convoys"]
        have = int(f["v"]) if f and isinstance(f["v"], (int, float)) else 0
        st = self.plan.get("sea_transfer") if isinstance(self.plan.get("sea_transfer"), dict) else {}
        stated = st.get("convoys") if isinstance(st.get("convoys"), int) and not isinstance(st.get("convoys"), bool) else None
        self.facts["convoys"] = {
            "start": have, "start_src": f["src"] if f else None, "need": need,
            "sea_supply": {"states": rows, "supply": round(supply, 2), "convoys_per_supply": factor,
                           "convoys": round(for_supply, 2)},
            "sea_trade": {"units": round(units, 2), "convoys_per_unit": TRADE_CONVOYS_PER_UNIT,
                          "convoys": round(for_trade, 2), "counted": counted, "not_counted": unknown},
            "range_factor": range_factor, "free": have - need,
            "d": "need = sea supply (divisions in states the capital cannot reach over own land x the supply use "
                 "of their templates x SUPPLY_CONVOY_FACTOR) + sea trade (imported units x "
                 f"{TRADE_CONVOYS_PER_UNIT:g}, not in the defines; every import counted as sea trade: an upper "
                 "bound); before range: the game raises both with the route length (CONVOY_RANGE_FACTOR), which "
                 "is not modelled, so keep a wide margin on long lanes"}
        parts = []
        if rows:
            parts.append(f"sea supply {for_supply:.1f} ({supply:.1f} supply for "
                         f"{sum(r['divisions'] for r in rows)} divisions in state(s) "
                         f"{', '.join(str(r['state']) for r in rows)} x {factor:g} SUPPLY_CONVOY_FACTOR)")
        if units:
            parts.append(f"sea trade {for_trade:.1f} ({units:g} imported units x {TRADE_CONVOYS_PER_UNIT:g})")
        text = " + ".join(parts)
        if need > have:
            self.add("error", "supply.convoy_size", "production",
                     f"the plan needs about {need} convoys before range ({text}) and {self.tag} starts with {have}"
                     f"{' [' + f['src'] + ']' if f else ' (no set_convoys in its history)'}: build convoys first "
                     f"(a convoy production line sized to the gap), cut the imports, or move fewer divisions overseas")
        if stated is not None and stated < need:
            self.add("error", "supply.convoy_size", "sea_transfer.convoys",
                     f"sea_transfer.convoys is {stated}, the plan's sea supply and trade need about {need} before "
                     f"range ({text})")

    def xp_unspent(self):
        """A war banks army experience in the country pool; a plan that lists no army XP spend leaves it idle."""
        if not self.tag:
            return
        first, last, _ = self.war_window()
        ends = [d for d, _ in self.horizon()]
        if first is None:
            return
        last = last if last is not None else (min(ends) if ends else None)
        if last is None or last <= first:
            return
        w = self.world
        xp = w.experience(self.tag)
        gain = xp["gain"].get("army") or {}
        if "combat_max_per_day" not in gain:
            return
        per_day = float(gain["combat_max_per_day"]["v"])
        pool = float(xp["max"]["army"]["v"]) if "army" in xp["max"] else math.inf
        path = {k for k in self.plan.get("focus_path") or [] if isinstance(k, str)}
        focusmod = __import__((__package__ or "hoi4facts") + ".focus", fromlist=["xp_rewards"])
        rewards = [(r["focus"], r["army"]) for r in focusmod.xp_rewards(w, self.tag)
                   if r["focus"] in path and (r.get("army") or 0) > 0]
        floor = min(pool, float(xp["start"]["army"]["v"]) + sum(n for _, n in rewards))
        ceiling = min(pool, floor + per_day * (last - first))
        d = self.game.doctrines(self.ctx) if self.ctx else {"grand": {}, "sub": {}}
        costs = [g["xp_cost"] for g in d["grand"].values() if g.get("folder") == "land" and g.get("xp_cost")]
        costs += [x["xp_cost"] for x in d["sub"].values() if x.get("xp_type") == "army" and x.get("xp_cost")]
        cheapest = min(costs) if costs else None
        self.facts["xp"] = {"army": {"floor": round(floor, 1), "ceiling": round(ceiling, 1), "war_days": last - first,
                                     "combat_max_per_day": per_day, "combat_scale": gain.get("combat_scale", {}).get("v"),
                                     "define": gain["combat_max_per_day"]["key"], "src": gain["combat_max_per_day"]["src"],
                                     "focus_rewards": rewards, "cheapest_land_doctrine": cheapest}}
        xb = self.plan.get("xp_budget") if isinstance(self.plan.get("xp_budget"), dict) else {}
        spends = bool(xb.get("army"))
        spends |= any(e.get("folder") == "land" for e in self.list_of("doctrines"))
        spends |= any(isinstance(t.get("xp"), (int, float)) and t["xp"] > 0 for t in self.list_of("templates"))
        spends |= any(e.get("kind") == "tank" for e in self.list_of("designs"))
        if spends or cheapest is None or ceiling < cheapest:
            return
        self.add("warning", "xp.unspent", "doctrines",
                 f"the plan spends no army experience, and the war can bank up to {ceiling:.0f} by "
                 f"{'.'.join(map(str, derive.date_of(last)))} (floor {floor:.0f} from the start and the focus_path "
                 f"rewards, plus at most {per_day:g} a day in combat over {last - first} days: "
                 f"{gain['combat_max_per_day']['key']} [{gain['combat_max_per_day']['src']}], the country pool; "
                 f"UNIT_EXPERIENCE_* is a division's veterancy); the cheapest land doctrine costs {cheapest:g}: add a "
                 f"doctrines entry with its xp (the executor adopts it when the pool has it) or another army XP "
                 f"spend; `calc xp` dates the pool")

    def hub_cost(self):
        game = self.game
        costs = derive.building_costs(game)
        kinds = derive.building_kinds(game)
        big = {b for b, r in costs.items() if b in kinds["supply"] or r["cost"] >= HUB_COST}
        jobs = []
        for prefix, items in self.construction_lists():
            for i, e in enumerate(items):
                if e.get("building") in big and isinstance(e.get("state"), int):
                    jobs.append((f"{prefix}[{i}] {e.get('id', e.get('order', ''))}".strip(), e["building"], e["state"],
                                 e.get("province"), e.get("when", "")))
        for i, e in enumerate(self.list_of("construction")):
            if e.get("building") in big:
                jobs += [(f"construction[{i}]", e["building"], s, None, "") for s in self.state_ids(e.get("states"))]
        if not jobs:
            return
        civ = derive.free_civilian_factories(game, self.ctx, self.tag, self.start, self.meta.get("history_file"))
        k = derive.construction_constants(game)
        sb = derive.state_buildings(game, self.ctx, self.start)
        states = self.states()
        hubs = derive.supply_hubs(game)
        coast = derive.coastal_provinces(game)
        capt = self.capturable()
        rail = next((b for b in sorted(kinds["railway"]) if b in costs), None)
        port = next((b for b, r in sorted(costs.items()) if r["port"]), None)
        horizon = self.horizon()
        rows = []
        for where, b, sid, prov, when in jobs:
            if not isinstance(when, str) or parse_when(when)[0] in ("text", "bad"):
                continue
            t0 = self.trigger_day(when)
            lvl = sum(v for x, v in sb.get(sid, {}).items() if x in kinds["infrastructure"])
            days = derive.construction_days(game, b, civ["free"], lvl)
            if days is None:
                self.add("error", "construction.hub_cost", where,
                         f"{b} in state {sid}: no free civilian factory builds it ({civ['civs']} civs, consumer goods "
                         f"{civ['consumer_goods']:g})")
                continue
            n = min(k["project_cap"], civ["free"])
            mult = 1 + k["infra_step"] * lvl if costs[b]["infra_effect"] else 1.0
            finish = t0 + days
            msg = (f"{b} in state {sid}: about {days} days ({costs[b]['cost']:g} points / ({n} civs x "
                   f"{k['cp_per_civ']:g} x {mult:g} at infrastructure {lvl})), {derive.fmt(derive.date_of(t0))} to "
                   f"{derive.fmt(derive.date_of(finish))}")
            severity = None
            late = [(d, label) for d, label in horizon if finish > d]
            if late:
                severity = "error"
                msg += "; finishes after the " + " and the ".join(f"{label} {derive.fmt(derive.date_of(d))}"
                                                               for d, label in late)
            elif days > HUB_DAYS:
                severity = "warning"
                msg += f"; more than {HUB_DAYS} days"
            src = [prov] if isinstance(prov, int) else list(states.get(sid, {}).get("provinces", []))
            near = derive.distances(game, src, HUB_STEPS)
            ctl = self.control_at(when)
            if b in kinds["supply"]:
                alt = sorted((d, p) for p, d in near.items() if p in hubs and p not in src and (p in ctl or p in capt))
                if alt:
                    d, p = alt[0]
                    severity = severity or "warning"
                    extra = (f"rail_way {costs[rail]['cost']:g} + {costs[rail]['per_level']:g} per extra level a "
                             f"province" if rail else "a railway")
                    msg += (f"; a railway level from the {'existing' if p in ctl else 'capturable'} hub at {p} "
                            f"(state {game.province_state().get(p)}, {d} step(s)) could serve the same provinces "
                            f"({extra})")
                shore = sorted((d, p) for p, d in near.items() if p in coast and p in ctl)
                if shore and port:
                    d, p = shore[0]
                    severity = severity or "warning"
                    df = lambda n: derive.define(game, n)  # noqa: E731
                    msg += (f"; port alternative: {port} at coastal province {p} ({d} step(s)) costs "
                            f"{costs[port]['cost']:g} + {costs[port]['per_level']:g} per extra level vs "
                            f"{costs[b]['cost']:g}, supply flow {df('NAVAL_BASE_INITIAL_SUPPLY_FLOW')} vs "
                            f"{df('NODE_INITIAL_SUPPLY_FLOW')} but penalties {df('NAVAL_BASE_STARTING_PENALTY_PER_PROVINCE')}"
                            f"/{df('NAVAL_BASE_ADDED_PENALTY_PER_PROVINCE')} vs {df('NODE_STARTING_PENALTY_PER_PROVINCE')}"
                            f"/{df('NODE_ADDED_PENALTY_PER_PROVINCE')} per province, and sea supply needs convoys")
            rows.append({"where": where, "building": b, "state": sid, "days": days,
                         "finish": derive.fmt(derive.date_of(finish)), "free_civs": civ["free"], "infrastructure": lvl})
            if severity:
                self.add(severity, "construction.hub_cost", where, msg)
        self.facts["hub_cost"] = {"free_civs": civ, "jobs": rows}

    def air_bases(self):
        per = Counter()
        for e in self.list_of("air_plan"):
            if isinstance(e.get("base_state"), int) and isinstance(e.get("count"), int):
                per[e["base_state"]] += e["count"]
        if not per:
            return
        levels = derive.air_base_levels(self.game, self.ctx, self.start)
        kinds = derive.air_base_buildings(self.game)
        queued = Counter(s for _, b, s in self.queued() if b in kinds)
        mult = float(derive.define(self.game, "AIRBASE_CAPACITY_MULT", 200))
        for sid, n in sorted(per.items()):
            lvl = levels.get(sid, 0) + queued[sid]
            if n > lvl * mult:
                self.add("warning", "air.base_capacity", f"air_plan (base_state {sid})",
                         f"{n} planes based in state {sid}, whose air bases (level {levels.get(sid, 0)} at the start plus "
                         f"{queued[sid]} queued) hold {lvl * mult:g} (AIRBASE_CAPACITY_MULT {mult:g} per level)")

    def templates(self):
        temps = self.list_of("templates")
        if not temps:
            return
        game = self.game
        units = derive.unit_types(game)
        have = start_tech_set(game, self.meta)
        planned = self.planned_techs()
        subs = derive.subunit_unlocks(game)
        arche = derive.equipment_archetypes(game)
        by_arch = defaultdict(set)
        for eq, ts in derive.equipment_unlocks(game).items():
            by_arch[arche.get(eq, eq)] |= ts
        xb = self.plan.get("xp_budget") if isinstance(self.plan.get("xp_budget"), dict) else None
        spends = {s.get("ref"): s for s in (xb or {}).get("army") or [] if isinstance(s, dict)}
        start_year = derive.as_tuple(self.start)[0]
        locked = game.locked_templates() if hasattr(game, "locked_templates") else {}
        for i, t in enumerate(temps):
            where = f"templates[{i}] {t.get('id', t.get('name'))}"
            base = t.get("base")
            if isinstance(base, str) and base in locked:
                self.add("error", "templates.locked", where,
                         f"base {base!r} is {locked[base]}: the game offers no Edit for it; create a new template")
            cols = template_columns(t.get("regiments")) or []
            dates = predicate_dates(t["when"], self.table) if t.get("when") is not None else []
            year = max(dates)[0] if dates else start_year
            flat = [u for c in cols for u in c] + list(t.get("support") or []) + list(t.get("regimental_support") or [])
            for u in sorted(set(flat)):
                info = units.get(u)
                if info is None:
                    continue  # validate_plan reports unknown units
                self.unlocked(where, f"unit {u}", info["active"], subs.get(u), have, planned, year)
                for eq in sorted(info["need"]):
                    self.unlocked(where, f"{u} equipment {eq}", False, by_arch.get(eq), have, planned, year)
            xp = t.get("xp")
            if isinstance(xp, (int, float)) and not isinstance(xp, bool) and xp > 0:
                ref = f"template:{t.get('id')}"
                if xb is None:
                    self.add("error", "templates.xp", where, f"costs {xp:g} army XP but the plan has no xp_budget")
                elif ref not in spends:
                    self.add("error", "templates.xp", where, f"costs {xp:g} army XP; xp_budget.army has no spend ref {ref}")
                elif isinstance(spends[ref].get("cost"), (int, float)) and spends[ref]["cost"] < xp:
                    self.add("warning", "templates.xp", where,
                             f"xp_budget.army spend {spends[ref].get('id')} budgets {spends[ref]['cost']:g} XP, the "
                             f"template costs {xp:g}")

    def unlocked(self, where, what, active, techs, have, planned, year):
        if active or not techs or techs & have:
            return
        hits = sorted((planned[t], t) for t in techs if t in planned)
        if not hits:
            self.add("error", "templates.equipment", where,
                     f"{what} is unlocked by {', '.join(sorted(techs)[:4])}{' ...' if len(techs) > 4 else ''}, none at "
                     f"the start or in the research plan")
        elif hits[0][0] and hits[0][0] > year:
            self.add("warning", "templates.equipment", where,
                     f"{what} needs {hits[0][1]}, planned for {hits[0][0]}, after the template's year {year}")

    def laws(self):
        laws = derive.law_ideas(self.game)
        for i, p in enumerate(self.list_of("politics")):
            info = laws.get(p.get("idea"))
            if not info or not info["available"]:
                continue
            where = f"politics[{i}] {p.get('idea')}"
            core = re.search(r"econom|mobili[sz]|conscript|manpower", info["category"] or "")
            gate = " and ".join(f"{k} {op} {x:g}" for k, (op, x) in sorted(info["gates"].items()))
            if "when" not in p:
                text = (f"{gate}{' plus other conditions' if len(info['available']) > len(info['gates']) else ''}"
                        if gate else derive.render(info["available"])[:160])
                self.add("error" if core else "warning", "politics.law_gates", where,
                         f"{p['idea']} ({info['category']} law) has an `available` gate ({text}); give `when` as a "
                         f"predicate" + (f", e.g. \"{gate}\"" if gate else ""))
                continue
            atoms, _ = parse_predicate(p["when"], self.table)
            for key, (op, x) in sorted(info["gates"].items()):
                if not any(a["name"] == key and a["op"] in (">", ">=") and a["value"] >= x - 1e-9 for _, a in atoms):
                    self.add("warning", "politics.law_gates", where,
                             f"`when` does not test {key} {op} {x:g}, the gate of {p['idea']}")

    def occupation(self):
        game = self.game
        res = derive.resisting_states(game, self.tag, self.ctx, self.start, self.meta.get("history_file"))
        owners = game.state_owners(self.ctx, self.start)
        enemies = set(self.enemies())
        conq = {e["state"] for e in self.targets() if isinstance(e.get("state"), int)
                and owners.get(e["state"]) not in (None, self.tag)}
        conq |= {s for s in self.axis_states() if owners.get(s) in enemies}
        self.facts["occupation"] = {"resisting": sorted(res), "conquered": sorted(conq)}
        if (res or conq) and not isinstance(self.plan.get("occupation"), dict):
            parts = []
            if res:
                parts.append(f"states {', '.join(map(str, sorted(res)))} resist from the start")
            if conq:
                parts.append(f"the plan conquers {', '.join(map(str, sorted(conq)))}")
            self.add("error", "occupation.required", "occupation",
                     f"{' and '.join(parts)}, but the plan has no occupation key (default_law, ladder, "
                     f"harsher_at_resistance, milder_at_compliance)")

    def peace(self):
        enemies = self.enemies()
        if not enemies:
            return
        p = self.plan.get("peace") if isinstance(self.plan.get("peace"), dict) else {}
        owners = self.game.state_owners(self.ctx, self.start)
        covered = set()
        for key in ("take", "puppet", "liberate"):
            covered |= {owners.get(s) for s in _ints(p.get(key))}
            covered |= {t for t in p.get(key) or [] if isinstance(t, str)}
        for e in enemies:
            if e not in covered:
                self.add("error", "peace.enemies", "peace",
                         f"no peace item for {e}: list its states by rank in peace.take (or puppet / liberate); "
                         f"without peace the runner stops at the conference and wakes the player")

    def manage(self):
        """manage: the standing rules run only with the key; a threshold of a rule switched off in
        `off`, or a threshold the executor does not run yet (x-executor-planned), is flagged."""
        m = self.plan.get("manage")
        if not isinstance(m, dict):
            self.add("error", "manage.thresholds", "manage",
                     "no manage key: the manpower, stability, equipment-fill and stockpile rules run only when the "
                     "plan has it (write {} for every default, or the thresholds)")
            return
        spec = ((((self.schema or {}).get("properties") or {}).get("manage") or {}).get("properties") or {})
        off = set(m.get("off") or []) if isinstance(m.get("off"), list) else set()
        for name, v in m.items():
            node = spec.get(name) or {}
            if node.get("x-rule") in off:
                self.add("warning", "manage.thresholds", f"manage.{name}",
                         f"{name} is set but its rule {node['x-rule']} is in manage.off")
            if node.get("x-executor-planned"):
                self.add("warning", "manage.thresholds", f"manage.{name}",
                         f"{name} is not run by the executor yet ({node['x-executor-planned']}); the player watches it")
        self.facts["manage"] = {k: (m[k] if k in m else v.get("default")) for k, v in spec.items()
                                if "default" in v and k not in ("why", "note")}

    def alarm_blocks(self):
        """[(label, alarms object)]: the base alarms and those of phase and contingency overrides."""
        out = [("alarms", self.plan.get("alarms"))]
        for key in ("phases", "contingencies"):
            for j, p in enumerate(self.list_of(key)):
                then = p.get("then") if isinstance(p.get("then"), dict) else {}
                for label, ov in ((f"{key}[{j}].overrides", p.get("overrides")),
                                  (f"{key}[{j}].then.overrides", then.get("overrides"))):
                    if isinstance(ov, dict):
                        out.append((f"{label}.alarms", ov.get("alarms")))
        return [(label, a) for label, a in out if isinstance(a, dict)]

    def alarms_silenced(self):
        """An alarm switched off needs its reason next to it: `why` of the same alarms object names the code."""
        for label, block in self.alarm_blocks():
            why = block.get("why") if isinstance(block.get("why"), str) else ""
            words = set(re.findall(r"[A-Za-z0-9_]+", why))
            # a reason is more than the bare code: something has to be said about it
            said = len(words - set(block)) >= 3
            for code, v in block.items():
                if code in ("why", "note") or not (v is False or v is None):
                    continue
                if code in words and said:
                    continue
                extra = ""
                if code in ("research_slot_empty", "no_active_focus"):
                    extra = ("; this alarm already waits while the plan's lists own the slot (a slot the executor "
                             "fills next is not reported, a list that ran out raises the plan-exhausted alarm), so "
                             "an empty slot it reports is one nobody is filling")
                self.add("error", "alarms.silenced", f"{label}.{code}",
                         f"{code} is switched off and {label}.why "
                         f"{'does not name it' if why else 'is missing'}: keep the alarm (delete the key), or name "
                         f"{code} in {label}.why with the reason it must stay quiet{extra}")

    def unreadable(self):
        """Strategy predicates that can never hold while their atoms are unreadable (always unknown)."""
        table = self.table
        if not table:
            return
        spots = [(f"goal.predicates[{i}]", p) for i, p in enumerate(self.goal.get("predicates") or [])]
        spots += [(f"goal.fail_predicates[{i}]", p) for i, p in enumerate(self.goal.get("fail_predicates") or [])]
        for j, c in enumerate(self.list_of("checkpoints")):
            spots += [(f"checkpoints[{j}].must[{i}]", p) for i, p in enumerate(c.get("must") or [])]
        for j, p in enumerate(self.list_of("phases")):
            spots += [(f"phases[{j}].{k}", p[k]) for k in ("enter", "exit") if k in p]
        for j, c in enumerate(self.list_of("contingencies")):
            for k in ("when", "trigger"):
                if k in c:
                    spots.append((f"contingencies[{j}].{k}", c[k]))
        for where, pred in spots:
            if predicate_can_hold(pred, table)[0]:
                continue
            atoms, problems = parse_predicate(pred, table)
            if problems:
                continue  # validate_plan reports malformed predicates
            names = sorted({a["text"] for _, a in atoms if not a["readable"]})
            if names:
                self.add("warning", "predicates.unreadable", where,
                         f"can never hold: {', '.join(names)} cannot be read by the executor yet (always unknown); "
                         f"use a readable atom (x-atoms readable) or an any[] with one")

    def production_changes(self):
        """production.remove: a `replace` entry the executor does not run yet (removals: production.in_progress)."""
        for i, x in enumerate(self.list_of("production")):
            eq = str(x.get("equipment") or "")
            if "replace" in x:
                self.add("warning", "production.remove", f"production[{i}]",
                         f"replace of {eq} is not read by the executor yet: remove the line and add the new "
                         f"equipment as two entries, or start the new line small next to it (efficiency)")

    # ------------------------------------------------------------------ tactics research rules: helpers
    @property
    def world(self):
        if getattr(self, "_world_obj", None) is None:
            self._world_obj = world_of(self.game, self.meta)
        return self._world_obj

    def start_lines(self):
        """Production lines at the start (derive.start_production), [] without a tag."""
        return derive.start_production(self.world, self.tag) if self.tag else []

    def define(self, name, default):
        v = derive.define(self.game, name, default)
        return float(v) if isinstance(v, (int, float)) else default

    def phase_days(self):
        """{phase id: day its `enter` date fires} for the phases with a date in `enter`."""
        out = {}
        for p in self.list_of("phases"):
            ds = predicate_dates(p["enter"], self.table) if p.get("enter") is not None else []
            if ds:
                out[str(p.get("id"))] = derive.day(max(ds))
        return out

    def removal_days(self):
        """{equipment: [(production index, from day or None)]} of the remove entries."""
        out = defaultdict(list)
        for i, x in enumerate(self.list_of("production")):
            if x.get("remove") and isinstance(x.get("equipment"), str):
                out[x["equipment"]].append((i, derive.day(x.get("from")) if x.get("from") else None))
        return out

    def start_templates(self):
        """{name: {"regiments": {unit: n}, "support": [...], "regimental_support": [...]}} of the start OOBs."""
        return self.world.oob(self.tag)["templates"] if self.tag else {}

    def kept_start_templates(self):
        """Start templates the plan keeps fielding: army_layout and training templates, and the
        templates of start divisions the disband list does not name."""
        names = {t for e in self.list_of("army_layout") for t in e.get("templates") or [] if isinstance(t, str)}
        tr = self.plan.get("training")
        for line in tr if isinstance(tr, list) else [tr]:
            if isinstance(line, dict) and isinstance(line.get("template"), str):
                names.add(line["template"])
        db = self.plan.get("disband") if isinstance(self.plan.get("disband"), dict) else {}
        gone = {t for t in db.get("templates") or [] if isinstance(t, str)}
        if self.tag:
            names |= {d["template"] for d in self.world.oob(self.tag)["divisions"] if d["template"] not in gone}
        return names - {None}

    def used_families(self):
        """(equipment families the plan uses, reasons): template and kept start-template battalions,
        air_plan airframes, designs, stockpile targets, training guards and the plan's own lines."""
        g = self.game
        units = derive.unit_types(g)
        used = set()

        def need(names):
            for u in names:
                for eq in (units.get(u) or {}).get("need", {}):
                    used.add(derive.equipment_family(g, eq))

        for t in self.list_of("templates"):
            cols = template_columns(t.get("regiments")) or []
            need([u for c in cols for u in c] + list(t.get("support") or []) + list(t.get("regimental_support") or []))
        start = self.start_templates()
        for name in self.kept_start_templates():
            t = start.get(name)
            if t:
                need(list(t["regiments"]) + list(t["support"]) + list(t.get("regimental_support") or []))
        for e in self.list_of("air_plan"):
            if isinstance(e.get("airframe"), str):
                used.add(derive.equipment_family(g, e["airframe"]))
        for d in self.list_of("designs"):
            if isinstance(d.get("base"), str):
                used.add(derive.equipment_family(g, d["base"]))
        st = self.plan.get("stockpile_targets")
        for k in st if isinstance(st, dict) else {}:
            if k not in ("why", "note"):
                used.add(derive.equipment_family(g, k))
        tr = self.plan.get("training")
        for line in tr if isinstance(tr, list) else [tr]:
            guard = (line or {}).get("only_if_stockpile") if isinstance(line, dict) else None
            used.update(derive.equipment_family(g, k) for k in guard or {})
        rules = self.plan.get("training_rules")
        if isinstance(rules, dict) and isinstance(rules.get("only_if_stockpile"), dict):
            used.update(derive.equipment_family(g, k) for k in rules["only_if_stockpile"])
        for x in self.list_of("production"):
            if not x.get("remove"):
                for k in (x.get("equipment"), x.get("replace")):
                    if isinstance(k, str):
                        used.add(derive.equipment_family(g, k))
        return used

    # ------------------------------------------------------------------ production (tactics)
    def production_mature_switch(self):
        """A plan entry that moves an existing, mature line to another tier of its family (replace, a
        design on another tier, or a remove plus an add) instead of starting the new tier on new factories."""
        g = self.game
        start = derive.day(self.start)
        lines = self.list_of("production")
        since = {r["type"]: start - MATURE_DAYS for r in self.start_lines()}
        for x in lines:
            eq = x.get("equipment")
            if isinstance(eq, str) and not x.get("remove") and "replace" not in x and isinstance(x.get("factories"), int):
                d = derive.day(x.get("from")) or start
                since[eq] = min(since.get(eq, d), d)
        designs = {}
        for d in self.list_of("designs"):
            for k in ("id", "name"):
                if d.get(k) is not None:
                    designs[str(d[k])] = d
        keep = {"parent": ("PARENT", 30), "family": ("ARCHETYPE", 20)}
        base_eff = self.define("BASE_FACTORY_START_EFFICIENCY_FACTOR", 10)

        def kept(link):
            name, dflt = keep[link]
            key = f"BASE_FACTORY_EFFICIENCY_{name}_CHANGE_FACTOR"
            return self.define(key, dflt), key

        def mature(eq, d):
            return eq in since and d - since[eq] >= MATURE_DAYS

        for i, x in enumerate(lines):
            eq = x.get("equipment")
            if not isinstance(eq, str):
                continue
            d = derive.day(x.get("from")) or start
            when = derive.fmt(derive.date_of(d))
            new, how = None, None
            if isinstance(x.get("replace"), str):
                new, how = x["replace"], "replace"
            elif x.get("design") is not None and isinstance((designs.get(str(x["design"])) or {}).get("base"), str):
                new, how = designs[str(x["design"])]["base"], f"design {x['design']}"
            if new:
                link = derive.equipment_tier_link(g, eq, new)
                if link in keep and mature(eq, d):
                    pct, key = kept(link)
                    self.add("error", "production.mature_switch", f"production[{i}]",
                             f"{how} switches the mature {eq} line to {new} on {when}, another tier of "
                             f"{derive.equipment_family(g, eq)}: the line keeps {pct:g}% of its efficiency ({key}) "
                             f"and loses the unit in progress; start {new} on new or idle factories (3-5) and keep "
                             f"the {eq} line while its gear is still needed, then switch it whole")
                continue
            if not x.get("remove") or not mature(eq, d):
                continue
            for j, y in enumerate(lines):
                ny = y.get("equipment")
                if j == i or y.get("remove") or not isinstance(ny, str) or not isinstance(y.get("factories"), int):
                    continue
                dy = derive.day(y.get("from")) or start
                link = derive.equipment_tier_link(g, eq, ny)
                if link in keep and abs(dy - d) <= SWITCH_MATCH_DAYS and not mature(ny, dy):
                    pct, key = kept(link)
                    self.add("error", "production.mature_switch", f"production[{i}]",
                             f"production[{i}] removes the mature {eq} line on {when} and production[{j}] adds {ny} "
                             f"({y['factories']} factories) on {derive.fmt(derive.date_of(dy))}: the factories restart "
                             f"at {base_eff:g}% (BASE_FACTORY_START_EFFICIENCY_FACTOR), worse than the {pct:g}% a "
                             f"switched line keeps ({key}); start {ny} on new factories and keep {eq} until its gear "
                             f"covers the need")

    def finishing_lines(self, d):
        """{equipment: finish day} of the start lines that on day d only complete a unit in progress: the
        game would finish the unit first (unit_finish) and the plan removes the line afterwards, as
        production.in_progress asks. Such a line is no choice of the posture: the cull rule leaves it
        alone until the unit is done and posture.shares does not count it."""
        removed = self.removal_days()
        out = {}
        for eq in sorted({r["type"] for r in self.start_lines()}):
            if eq not in removed or any(f is None or f <= d for _, f in removed[eq]):
                continue  # kept for good, or gone by d
            fin = self.unit_finish(eq)
            if fin and d < fin[0]:
                out[eq] = fin[0]
        return out

    def production_cull(self):
        """Start lines the plan never uses and does not remove; lines of a domain the force posture gives 0%."""
        g = self.game
        lines = self.start_lines()
        removed = self.removal_days()
        nv = self.plan.get("navy")
        naval_used = isinstance(nv, dict) and nv.get("posture") != "none"
        used = self.used_families()
        prefixes = [p for p in self.plan.get("production_priority") or [] if isinstance(p, str) and p]
        types = derive.equipment_types(g)
        mot = self.war.get("motorization")
        sea = bool(self.overseas()) or isinstance(self.plan.get("sea_transfer"), dict)
        air = self.plan.get("air") if isinstance(self.plan.get("air"), dict) else {}
        air_any = bool(air.get("war_missions")) and not self.list_of("air_plan")
        by_eq = defaultdict(list)
        for r in lines:
            by_eq[r["type"]].append(r)
        rows = []
        for eq, rs in sorted(by_eq.items()):
            dom = derive.equipment_domain(g, eq) or "land"
            if eq in removed:
                continue
            if dom == "navy" and nv is None:
                continue  # navy.required asks for the naval plan first
            t = types.get(eq, set())
            if derive.equipment_family(g, eq) in used or any(eq.startswith(p) for p in prefixes) \
                    or (dom == "navy" and naval_used and "convoy" not in t) or (dom == "air" and air_any) \
                    or ("motorized" in t and isinstance(mot, (int, float)) and mot > 0) or ("convoy" in t and sea):
                continue
            n = sum(int(r.get("factories") or 0) for r in rs)
            names = sorted({r["version"] for r in rs if r.get("version")})
            fin = self.unit_finish(eq)
            rows.append(eq)
            what = ("dockyards" if n != 1 else "dockyard") if dom == "navy" else \
                ("military factories" if n != 1 else "military factory")
            self.add("error", "production.cull", f"start line {eq}",
                     f"start line {eq}{' (' + ', '.join(names) + ')' if names else ''} keeps {n} {what} on {dom} "
                     f"equipment that no template, air wing, fleet or stockpile target of the plan uses; remove it at "
                     f"the start or a phase change (production[] {{equipment, remove: true}}"
                     + (f", from {derive.fmt(derive.date_of(fin[0]))} when its unit in progress is done" if fin else "")
                     + ") and give the factories to a kept start line")
        self.facts["unused_start_lines"] = rows
        fp = self.plan.get("force_posture")
        if not isinstance(fp, dict):
            return
        days = self.phase_days()
        start = derive.day(self.start)
        for j, ph in enumerate(fp.get("phases") or []):
            if not isinstance(ph, dict):
                continue
            zero = [k for k in ("land", "air", "navy") if ph.get(k) == 0]
            d = start if ph.get("phase") == "base" else days.get(str(ph.get("phase")))
            if not zero or d is None:
                continue
            finishing = self.finishing_lines(d)
            live = {eq: n for eq, n in self.active_lines(d).items() if eq not in finishing}
            for eq, rs in by_eq.items():
                if eq not in finishing and not any(f is None or f <= d for _, f in removed.get(eq, [])):
                    live[eq] = live.get(eq, 0) + sum(int(r.get("factories") or 0) for r in rs)
            for dom in zero:
                hit = sorted(eq for eq, n in live.items() if n and (derive.equipment_domain(g, eq) or "land") == dom)
                if hit:
                    self.add("error", "production.cull", f"force_posture.phases[{j}]",
                             f"phase {ph.get('phase')} gives {dom} 0% but lines {', '.join(hit)} build {dom} "
                             f"equipment on {derive.fmt(derive.date_of(d))}: remove them by then")

    def production_day0(self):
        """Free (and freed) military factories at the start not put on kept start lines before the
        first unpause (factories added to a start line on day 0 join at its efficiency)."""
        g = self.game
        mils = (self.facts.get("military_factories") or {}).get("start")
        lines = [r for r in self.start_lines() if (derive.equipment_domain(g, r["type"]) or "land") != "navy"]
        if mils is None or not self.tag:
            return
        start = derive.day(self.start)
        on_lines = defaultdict(int)
        for r in lines:
            on_lines[r["type"]] += int(r.get("factories") or 0)
        free = max(0, mils - sum(on_lines.values()))
        gone = {eq for eq, rows in self.removal_days().items() if any(f is None or f <= start for _, f in rows)}
        freed = sum(n for eq, n in on_lines.items() if eq in gone)
        kept = {eq: n for eq, n in on_lines.items() if eq not in gone}
        want = defaultdict(int)
        new = {}
        for x in self.list_of("production"):
            eq = x.get("equipment")
            d = derive.day(x.get("from")) if x.get("from") else start
            if not isinstance(eq, str) or x.get("remove") or "replace" in x or d is None or d > start \
                    or not isinstance(x.get("factories"), int):
                continue
            if eq in kept:
                want[eq] += x["factories"]
            else:
                new[eq] = new.get(eq, 0) + x["factories"]
        pool = free + freed
        added = sum(max(0, n - kept[eq]) for eq, n in want.items())
        left = pool - min(pool, added)
        self.facts["day0"] = {"military_factories": mils, "on_start_lines": sum(on_lines.values()), "free": free,
                              "freed": freed, "to_start_lines": min(pool, added), "new_lines": new}
        if left <= 0:
            return
        msg = (f"{left} of the {pool} free military factories at the start ({mils} military factories, "
               f"{sum(on_lines.values())} on start lines" + (f", {freed} freed by start removals" if freed else "")
               + f") {'is' if left == 1 else 'are'} not put on kept start lines before the first unpause; a factory "
               f"added to a start line on day 0 joins at the line's efficiency")
        if new:
            msg += (f", while the new line(s) {', '.join(f'{k} ({n})' for k, n in sorted(new.items()))} take day-0 "
                    f"factories at the base {self.define('BASE_FACTORY_START_EFFICIENCY_FACTOR', 10):g}%")
        useful = sorted(set(kept) - set(self.facts.get("unused_start_lines") or []))
        msg += f"; size up kept start lines ({', '.join(useful) or 'none kept'}) in production"
        self.add("error", "production.day0", "production", msg)

    def unit_finish(self, eq):
        """(finish day, progress, start line, threshold) of the latest unit in progress on the start
        lines of eq that the game itself would finish first (ships and expensive items past 10%, cheap
        ones past 75%: PRODUCTION_MAX_PROGRESS_TO_SWITCH_NAVAL, PRODUCTION_WAIT_TO_FINISH_IF_*), at the
        base factory speed; None when no such unit is known."""
        g = self.game
        cost = (derive.equipment_rows(g).get(eq) or {}).get("cost")
        naval = (derive.equipment_domain(g, eq) or "land") == "navy"
        start = derive.day(self.start)
        out = []
        for r in self.start_lines():
            p, n = r.get("progress"), r.get("factories") or 0
            if r["type"] != eq or p is None or not cost or not n:
                continue
            if naval:
                per_day = n * self.define("BASE_FACTORY_SPEED_NAV", 2.0)
                cut = self.define("PRODUCTION_MAX_PROGRESS_TO_SWITCH_NAVAL", 0.1)
            else:
                eff = r.get("efficiency") or self.define("BASE_FACTORY_START_EFFICIENCY_FACTOR", 10)
                per_day = n * self.define("BASE_FACTORY_SPEED_MIL", 3.5) * eff / 100.0
                # the game's line between expensive and cheap items: fewer than one a week
                cut = self.define("PRODUCTION_WAIT_TO_FINISH_IF_EXPENSIVE", 0.1) if cost / per_day > 7 else \
                    self.define("PRODUCTION_WAIT_TO_FINISH_IF_CHEAP", 0.75)
            if p > cut:
                out.append((start + int(math.ceil((1 - p) * cost / per_day)), p, r, cut))
        return max(out, key=lambda f: f[0]) if out else None

    def production_in_progress(self):
        """A removal that throws away a unit in progress: a naval or expensive start line whose unit is
        past the game's finish threshold, removed before it is done or without a `from`; a naval line
        removed without a `from`."""
        g = self.game
        for i, x in enumerate(self.list_of("production")):
            eq = x.get("equipment")
            if not x.get("remove") or not isinstance(eq, str):
                continue
            naval = (derive.equipment_domain(g, eq) or "land") == "navy"
            frm = derive.day(x.get("from")) if x.get("from") else None
            fin = self.unit_finish(eq)
            if fin:
                end, p, r, cut = fin
                if frm is None or frm < end:
                    what = f"{r.get('version') or eq} ({p:.0%} done on {int(r.get('factories') or 0)} " \
                           f"{'dockyards' if naval else 'factories'})"
                    self.add("error", "production.in_progress", f"production[{i}]",
                             f"removing {eq} {'at once' if frm is None else 'on ' + derive.fmt(derive.date_of(frm))} "
                             f"throws away {what}, which finishes about {derive.fmt(derive.date_of(end))}"
                             + (" (hull cost only: the full design takes longer)" if naval else "")
                             + f"; give `from` on or after that date (the game itself keeps an item past "
                               f"{cut * 100:g}% done)")
            elif naval and frm is None:
                self.add("warning", "production.in_progress", f"production[{i}]",
                         f"removing naval line {eq} needs a `from` date after its ship in progress finishes; the "
                         f"executor waits without one")

    def production_naval_cap(self):
        """A naval line asking for more dockyards than one line takes (capital ships 5, convoys 15, others 10)."""
        g = self.game
        types = derive.equipment_types(g)
        for i, x in enumerate(self.list_of("production")):
            eq = x.get("equipment")
            n = x.get("factories")
            if not isinstance(eq, str) or not isinstance(n, int) or derive.equipment_domain(g, eq) != "navy":
                continue
            t = types.get(eq, set())
            key, dflt = (("CAPITAL_SHIP_MAX_NAV_FACTORIES_PER_LINE", 5) if t & {"capital_ship", "carrier"} else
                         ("CONVOY_MAX_NAV_FACTORIES_PER_LINE", 15) if "convoy" in t else
                         ("FLOATING_HARBOR_MAX_NAV_FACTORIES_PER_LINE", 5) if "floating_harbor" in t else
                         ("DEFAULT_MAX_NAV_FACTORIES_PER_LINE", 10))
            cap = self.define(key, dflt)
            if n > cap:
                self.add("error", "production.naval_cap", f"production[{i}]",
                         f"{eq} asks for {n} dockyards but one line takes at most {cap:g} ({key}); split it into "
                         f"{int(math.ceil(n / cap))} lines")

    # ------------------------------------------------------------------ army (tactics)
    def theatre_states(self):
        out = set(_ints(self.war.get("theatre"))) | self.axis_states() | {e["state"] for e in self.targets()
                                                                           if isinstance(e.get("state"), int)}
        return sorted(out or {e["state"] for e in self.list_of("army_layout") if isinstance(e.get("state"), int)})

    def army_width_terrain(self):
        """Template widths far from the best fill of the theatre's terrain mix, or of its main terrain
        (over-width and stacking penalties included): the plan's templates, and the start templates
        its armies field."""
        g = self.game
        mix = derive.terrain_mix(g, self.theatre_states())
        if not mix:
            return
        scores = derive.width_scores(g, mix)
        best = max(scores.values())
        top = sorted(scores, key=lambda w: (-scores[w], w))[:3]
        main = max(mix, key=lambda t: (mix[t], t))
        main_best = max(derive.terrain_fill(g, w, main) for w in scores)
        units = derive.unit_types(g)
        mixtext = ", ".join(f"{t} {s:.0%}" for t, s in sorted(mix.items(), key=lambda kv: -kv[1]))
        tail = (f"width {top[0]} fills {scores[top[0]]:.2f} ({', '.join(f'{w} {scores[w]:.2f}' for w in top[1:])}), "
                f"the best on {main} {main_best:.2f}; terrain {mixtext}")
        self.facts["width_fill"] = {"mix": mix, "best": {str(w): scores[w] for w in top}, "main": main}

        def judge(width):
            """(severity or None, theatre fill, main-terrain fill)."""
            if not width:
                return None, None, None
            s, m = derive.width_score(g, width, mix), derive.terrain_fill(g, width, main)
            gap = max(best - s, main_best - m)
            if gap < WIDTH_WARN - 1e-9:
                return None, s, m
            return ("error" if gap >= WIDTH_ERROR - 1e-9 else "warning"), s, m

        edited = set()
        for i, t in enumerate(self.list_of("templates")):
            if isinstance(t.get("base"), str):
                edited.add(t["base"])
            if t.get("role") == "garrison":
                continue
            width = derive.template_width(g, template_columns(t.get("regiments")) or [])
            sev, s, m = judge(width)
            if sev:
                self.add(sev, "army.width_terrain", f"templates[{i}] {t.get('id', t.get('name'))}",
                         f"width {width:g} fills the theatre's battles at {s:.2f} ({main} {m:.2f}) after the over-width "
                         f"and stacking penalties; {tail}")
        start = self.start_templates()
        names = {n for e in self.list_of("army_layout") if e.get("role") != "garrison"
                 for n in e.get("templates") or [] if isinstance(n, str)} - edited
        bad, worst = [], None
        for name in sorted(names):
            t = start.get(name)
            if not t or any(u not in units for u in t["regiments"]):
                continue
            width = sum(units[u]["width"] * n for u, n in t["regiments"].items()) + \
                sum((units.get(x) or {}).get("width", 0) for x in list(t["support"]) + list(t.get("regimental_support") or []))
            sev, s, m = judge(width)
            if sev:
                bad.append(f"{name} width {width:g} (fill {s:.2f}, {main} {m:.2f})")
                worst = "error" if "error" in (sev, worst) else "warning"
        if bad:
            self.add(worst, "army.width_terrain", "army_layout",
                     f"start templates the armies field fill the theatre's battles poorly: {'; '.join(bad)}; {tail}: "
                     f"plan templates near the best width (templates[] with base) or merge the small divisions")

    # ------------------------------------------------------------------ war (tactics)
    def war_blocks(self):
        """[(label, war object)]: war, war_by_phase entries and phases / contingencies overrides.war."""
        out = [("war", self.war)] if self.war else []
        wbp = self.plan.get("war_by_phase")
        if isinstance(wbp, dict):
            out += [(f"war_by_phase.{k}", v) for k, v in wbp.items() if isinstance(v, dict)]
        for key in ("phases", "contingencies"):
            for j, p in enumerate(self.list_of(key)):
                ov = p.get("overrides")
                if isinstance(ov, dict) and isinstance(ov.get("war"), dict):
                    out.append((f"{key}[{j}].overrides.war", ov["war"]))
        return out

    def war_schema(self):
        return ((((self.schema or {}).get("definitions") or {}).get("war") or {}).get("properties") or {})

    def war_key(self, pattern, numeric=True):
        """The war key of the schema whose name matches pattern (numeric ones only), else None."""
        for k, v in self.war_schema().items():
            if re.search(pattern, k) and (not numeric or v.get("type") in ("number", "integer")):
                return k
        return None

    def schema_default(self, key, fallback):
        prop = self.war_schema().get(key) or {}
        if isinstance(prop.get("default"), (int, float)) and not isinstance(prop.get("default"), bool):
            return float(prop["default"])
        desc = str(prop.get("description") or "")
        m = re.search(r"default\s+(-?\d+(?:\.\d+)?)", desc)
        return float(m.group(1)) if m else fallback

    @staticmethod
    def share(v):
        """A fraction from a fraction or a percent (values above 1 are percent)."""
        return v / 100.0 if v > 1 else float(v)

    def offensives(self):
        return bool(self.targets()) or bool(self.axis_states())

    def war_supply_floor(self):
        """war.supply_floor below the supply where attrition starts, or the attack threshold below 50%."""
        if not self.war:
            return
        attrition = self.define("SUPPLY_THRESHOLD_FOR_ARMY_ATTRITION", 0.35)
        attack_key = self.war_key(r"attack.*supply|supply.*attack")
        for label, w in self.war_blocks():
            base = label == "war"
            if "supply_floor" not in w and not base:
                continue
            raw = w.get("supply_floor", self.schema_default("supply_floor", 25))
            if not isinstance(raw, (int, float)):
                continue
            floor = self.share(raw)
            attack = floor
            if attack_key and isinstance(w.get(attack_key), (int, float)):
                attack = self.share(w[attack_key])
            src = f"war.supply_floor {raw:g}" + ("" if "supply_floor" in w else " (the executor default)")
            if floor < attrition:
                self.add("error", "war.supply_floor", f"{label}.supply_floor",
                         f"{src}: armies rest only below {floor:.0%} supply, so they keep fighting under the "
                         f"{attrition:.0%} where attrition starts (SUPPLY_THRESHOLD_FOR_ARMY_ATTRITION) and an "
                         f"attacker out of supply loses attack and defence; set it to 50 (no new offensive below "
                         f"50%, stop below 35%)")
            elif attack < ATTACK_SUPPLY:
                self.add("warning", "war.supply_floor", f"{label}.{attack_key or 'supply_floor'}",
                         f"{(attack_key or 'supply_floor')} {attack:.0%}: new offensives start below "
                         f"{ATTACK_SUPPLY:.0%} supply; set the attack threshold to 50")

    def war_planning_wait(self):
        """Offensives executed with no planning wait: direct moves, or no wait key / a wait of 0."""
        if not self.war or not self.offensives():
            return
        gain = self.define("PLANNING_GAIN", 0.02)
        cap = self.define("PLANNING_MAX", 0.3)
        full = int(math.ceil(cap / gain)) if gain else 15
        key = self.war_key(r"^plan(ning)?_(days|wait)")
        why = (f"a battle plan gains {gain:.0%} planning a day up to {cap:.0%} after {full} days (PLANNING_GAIN, "
               f"PLANNING_MAX); manual orders lose {self.define('PLAYER_ORDER_PLANNING_DECAY', 0.03):.0%} a day")
        if self.war.get("direct_moves") is True:
            self.add("warning", "war.planning_wait", "war.direct_moves",
                     f"offensives are direct moves, so no planning builds before an attack: {why}; use battle "
                     f"plans and wait 12-{full} days after drawing them")
            return
        if "planning_wait" in self.war_schema():
            if self.war.get("planning_wait") is False:
                self.add("error", "war.planning_wait", "war.planning_wait",
                         f"war.planning_wait false: offensives execute the tick after drawing them; {why}")
                return
            days = self.war.get("planning_max_days", self.schema_default("planning_max_days", full))
            if isinstance(days, (int, float)) and not isinstance(days, bool) and days < full * 2 / 3:
                self.add("warning", "war.planning_wait", "war.planning_max_days",
                         f"war.planning_max_days {days:g} builds {min(cap, days * gain):.0%} planning of "
                         f"{cap:.0%}: {why}")
            return
        if key:
            v = self.war.get(key)
            if not isinstance(v, (int, float)) or v <= 0:
                self.add("error", "war.planning_wait", f"war.{key}",
                         f"offensives execute with no planning wait (war.{key} {'missing' if v is None else v}): "
                         f"{why}; wait 12-{full} days")
            elif v < full * 2 / 3:
                self.add("warning", "war.planning_wait", f"war.{key}",
                         f"war.{key} {v:g} days builds {min(cap, v * gain):.0%} planning of {cap:.0%}: {why}")
            return
        self.add("info", "war.planning_wait", "war",
                 f"the executor executes an offensive the tick after drawing it (no planning-wait key yet): {why}; "
                 f"name the 12-{full} day wait in a brief line")

    def war_pocket_threshold(self):
        """Pocket fall-back above 50% hostile share (or switched off)."""
        if not self.war:
            return
        key = self.war_key(r"pocket")
        for label, w in self.war_blocks():
            if w.get("avoid_pockets") is False:
                self.add("error", "war.pocket_threshold", f"{label}.avoid_pockets",
                         "the pocket fall-back is off: an encircled army gets no manpower, equipment or fuel and "
                         f"loses its HQ general; keep it on and fall back at {POCKET_SHARE:.0%} hostile share")
            if key and isinstance(w.get(key), (int, float)) and self.share(w[key]) > POCKET_SHARE:
                self.add("error", "war.pocket_threshold", f"{label}.{key}",
                         f"pocket fall-back at {self.share(w[key]):.0%} hostile share fires after the neck closes; "
                         f"fall back at {POCKET_SHARE:.0%} or when the neck to the rear is 2 own provinces or fewer")
        if key and not isinstance(self.war.get(key), (int, float)):
            dflt = self.schema_default(key, EXECUTOR_POCKET_SHARE)
            if self.share(dflt) > POCKET_SHARE:
                self.add("warning", "war.pocket_threshold", f"war.{key}",
                         f"war.{key} unset: the default {self.share(dflt):.0%} hostile share is late; set "
                         f"{POCKET_SHARE:.0%}")
        elif not key and self.war.get("avoid_pockets") is not False and self.offensives():
            self.add("info", "war.pocket_threshold", "war",
                     f"the executor falls back at the {EXECUTOR_POCKET_SHARE:.0%} hostile share of the armies read "
                     f"(no threshold key yet); {POCKET_SHARE:.0%} or a neck of 2 own provinces belongs in a brief line")

    # ------------------------------------------------------------------ politics (tactics)
    def pp_cost(self, p):
        if isinstance(p.get("cost"), (int, float)) and not isinstance(p.get("cost"), bool):
            return float(p["cost"])
        idea = p.get("idea")
        c = self.world.idea_cost(idea) if isinstance(idea, str) else None
        if c is not None:
            return float(c)
        for ch in self.world.characters(self.tag, "advisor") if isinstance(idea, str) else []:
            for r in ch["roles"]:
                if r["role"] == "advisor" and idea in (ch["id"], r.get("idea_token")) and r.get("cost") is not None:
                    return float(r["cost"])
        return None

    def politics_pp_budget(self):
        """Planned PP spending (politics, decisions) against the net income: base x factors minus the
        running focus's cost (calc.pp_rates); unaffordable by the horizon or bought late."""
        if not self.tag:
            return
        buys = []
        for i, p in enumerate(self.list_of("politics")):
            c = self.pp_cost(p)
            if c:
                ds = predicate_dates(p["when"], self.table) if p.get("when") is not None else []
                buys.append((f"politics[{i}] {p.get('idea')}", c, derive.day(max(ds)) if ds else None))
        w = self.world
        for i, d in enumerate(self.plan.get("decisions") or []):
            row = w.find_decision(d) if isinstance(d, str) else None
            c = w.decision_row(row).get("cost") if row else None
            if isinstance(c, (int, float)) and c > 0:
                buys.append((f"decisions[{i}] {d}", float(c), None))
        for i, r in enumerate(self.list_of("decision_rules")):
            row = w.find_decision(r.get("decision")) if isinstance(r.get("decision"), str) and not r.get("never") else None
            c = w.decision_row(row).get("cost") if row else None
            if isinstance(c, (int, float)) and c > 0:
                ds = predicate_dates(r["when"], self.table) if r.get("when") is not None else []
                for _ in range(max(1, r["max"]) if isinstance(r.get("max"), int) else 1):
                    buys.append((f"decision_rules[{i}] {r.get('decision')}", float(c), derive.day(max(ds)) if ds else None))
        gain, factor = calcmod.pp_modifiers(w, self.tag)
        rates = calcmod.pp_rates(w, gain, factor, calcmod.start_stability(w, self.tag), None,
                                 calcmod.pp_costs(w, self.tag))
        start = derive.day(self.start)
        end = derive.day(self.stop) if self.stop else (derive.day(self.goal["deadline"]) if self.goal.get("deadline")
                                                       else start + 365)
        timeline = (self.facts.get("focus") or {}).get("timeline") or []
        focus_end = end if self.plan.get("focus_fallback") else (derive.day(timeline[-1]["end"]) if timeline else start)
        f = w.country(self.tag).get("political_power")
        balance = float(f["v"]) if f and isinstance(f.get("v"), (int, float)) else 0.0
        pending = list(buys)
        bought = {}
        day = start
        idle_run, idle_from, idle_worst = 0, start, (0, start)
        while day <= end:
            for b in list(pending):
                label, c, due = b
                if (due is None or due <= day) and balance >= c:
                    balance -= c
                    bought[label] = (day, due)
                    pending.remove(b)
            # power above PP_IDLE that no planned buy is waiting for (one not yet due or not yet
            # affordable keeps it banked on purpose)
            if balance > PP_IDLE and not pending:
                if not idle_run:
                    idle_from = day
                idle_run += 1
                if idle_run > idle_worst[0]:
                    idle_worst = (idle_run, idle_from)
            else:
                idle_run = 0
            day += 1
            balance += rates["per_day_focus"] if day <= focus_end else rates["per_day_idle"]
        how = (f"{rates['per_day_focus']:g} PP a day with a focus running ((base {rates['base']['v']:g} + gain "
               f"{gain:g}) x (1 + {factor + rates['stability_factor']:g}) - {rates['focus_cost']:g} for the focus"
               + (f" - {rates['cost']:g} daily costs" if rates["cost"] else "") + ")")
        self.facts["pp_budget"] = {"per_day_focus": rates["per_day_focus"], "per_day_idle": rates["per_day_idle"],
                                   "bought": {k: derive.fmt(derive.date_of(v[0])) for k, v in bought.items()},
                                   "not_bought": [b[0] for b in pending]}
        if idle_worst[0] > PP_IDLE_DAYS:
            auto = self.plan.get("politics_auto")
            policy = isinstance(auto, dict) and isinstance(auto.get("order"), list)
            off = auto is False or (isinstance(auto, dict) and auto.get("enabled") is False)
            self.add("info" if policy and not off else "warning", "politics.pp_budget", "politics",
                     f"political power stays above {PP_IDLE:g} with no planned buy for {idle_worst[0]} days from "
                     f"{derive.fmt(derive.date_of(idle_worst[1]))} ({how}): every spendable resource needs an "
                     "owner and a default use; budget laws, advisors and decisions over the whole horizon"
                     + (" (politics_auto.order is the standing policy that spends it)" if policy and not off
                        else "; politics_auto is off, so it piles up to the cap" if off
                        else ", or set politics_auto.order (the executor's default order only buys the chief of "
                             "army and the economy and trade laws)"))
        for label, c, due in pending:
            self.add("error", "politics.pp_budget", label.split(" ")[0],
                     f"{label} ({c:g} PP) is not affordable by {derive.fmt(derive.date_of(end))}: {how}; "
                     f"{sum(x[1] for x in buys if x[0] in bought):g} PP go to earlier buys")
        for label, (d, due) in bought.items():
            if due is not None and d - due > PP_LATE_DAYS:
                self.add("warning", "politics.pp_budget", label.split(" ")[0],
                         f"{label} is affordable only on {derive.fmt(derive.date_of(d))}, {d - due} days after its "
                         f"date {derive.fmt(derive.date_of(due))}: {how}")

    def politics_conscription(self):
        """A conscription law above what the plan's divisions need, or a manpower rule free to climb the
        whole ladder (manage.conscription_laws unset) into laws that cost factory output."""
        if not self.tag:
            return
        w = self.world
        laws = w.laws(self.tag)
        cat, ladder = None, []
        for c, row in laws.items():
            opts = [(w.modifier_value(o["id"], "conscription"), o["id"]) for o in row["options"]]
            opts = sorted((v, k) for v, k in opts if isinstance(v, (int, float)))
            if len(opts) > len(ladder):
                cat, ladder = c, opts
        if not ladder:
            return
        share = {k: v for v, k in ladder}
        current = laws[cat]["current"]
        mods = derive.idea_modifiers(self.game)
        ideas = [f["v"] for f in w.country(self.tag)["ideas"]]
        other = sum(mods.get(i, {}).get("conscription", 0.0) for i in ideas if i != current)
        factor = sum(mods.get(i, {}).get("conscription_factor", 0.0) for i in ideas)
        noncore = self.define("LOCAL_MANPOWER_ACCESSIBLE_NON_CORE_FACTOR", 0.02)
        pop = 0.0
        for r in w.states().values():
            if r["owner"] == self.tag:
                pop += r["manpower"] * (1.0 if self.tag in r["cores"] else noncore)
        recruit = {k: pop * (other + v) * (1 + factor) for k, v in share.items()}
        # manpower the plan fields: start divisions not disbanded, training up to its caps, two months of losses
        subs = w.sub_units()
        start_t = self.start_templates()

        def mp(name):
            t = start_t.get(name)
            if t:
                return sum((subs.get(u) or {}).get("manpower", 0) * n for u, n in t["regiments"].items()) + \
                    sum((subs.get(s) or {}).get("manpower", 0) for s in t["support"])
            for x in self.list_of("templates"):
                if name in (x.get("name"), x.get("id")):
                    flat = [u for c in template_columns(x.get("regiments")) or [] for u in c] + list(x.get("support") or [])
                    return sum((subs.get(u) or {}).get("manpower", 0) for u in flat)
            return 0

        divs = Counter(d["template"] for d in w.oob(self.tag)["divisions"])
        db = self.plan.get("disband") if isinstance(self.plan.get("disband"), dict) else {}
        left = db.get("max") if isinstance(db.get("max"), int) else sum(divs[t] for t in db.get("templates") or [])
        for t in db.get("templates") or []:
            k = min(divs.get(t, 0), left)
            divs[t] -= k
            left -= k
        tr = self.plan.get("training")
        rules = self.plan.get("training_rules") if isinstance(self.plan.get("training_rules"), dict) else {}
        for line in tr if isinstance(tr, list) else [tr]:
            if isinstance(line, dict) and isinstance(line.get("template"), str):
                cap = line.get("cap") if isinstance(line.get("cap"), int) else (
                    rules.get("cap") if rules.get("cap_template") in (None, line["template"]) else None)
                if isinstance(cap, int):
                    divs[line["template"]] = max(divs[line["template"]], cap)
        fielded = sum(mp(t) * n for t, n in divs.items())
        m = self.plan.get("manage") if isinstance(self.plan.get("manage"), dict) else None
        months = m["manpower_months"] if m and isinstance(m.get("manpower_months"), (int, float)) else 2
        need = fielded * (1 + months * MONTHLY_LOSS_SHARE)
        enough = next((k for v, k in ladder if recruit[k] >= need), ladder[-1][1])
        self.facts["manpower"] = {"population": round(pop), "fielded": fielded, "need": round(need),
                                  "law_needed": enough, "current": current}
        basis = (f"the plan fields {sum(divs.values())} divisions, {fielded:,.0f} manpower, plus {months:g} months of "
                 f"losses at {MONTHLY_LOSS_SHARE:.0%} a month = {need:,.0f}; {enough} recruits "
                 f"{recruit[enough]:,.0f} ({pop:,.0f} population x {other + share[enough]:.3g})")

        def cost(law):
            v = w.modifier_value(law, "industrial_capacity_factory")
            return f" ({v:+.0%} factory output)" if isinstance(v, (int, float)) and v < 0 else ""

        for i, p in enumerate(self.list_of("politics")):
            law = p.get("idea")
            if law in share and share[law] > share[enough] and share[law] > share.get(current, -1):
                self.add("error", "politics.unneeded_conscription", f"politics[{i}] {law}",
                         f"{law}{cost(law)} is above what the plan needs: {basis}")
        if m is None or "manpower" in (m.get("off") or []):
            return
        allowed = [x for x in m.get("conscription_laws") or [] if isinstance(x, str)]
        ladder_used = allowed or [k for _, k in ladder]
        dear = [k for k in ladder_used if k in share and share[k] > share[enough] and cost(k)]
        if dear:
            src = "manage.conscription_laws" if allowed else "manage.conscription_laws is unset: the default ladder"
            self.add("warning", "politics.unneeded_conscription", "manage.conscription_laws",
                     f"{src} lets the manpower rule take {', '.join(k + cost(k) for k in dear)}, above what the plan "
                     f"needs: {basis}; list only the laws the plan allows")

    # ------------------------------------------------------------------ research (tactics)
    def research_ahead(self):
        """Techs researched more than half a year before their start year (each year ahead adds
        BASE_YEAR_AHEAD_PENALTY_FACTOR times the cost) while in-year techs of the plan's fields stay
        unplanned. Start days come from a slot timeline of the plan (base research days)."""
        g = self.game
        techs = g.techs()
        meta = derive.tech_meta(g)
        have = start_tech_set(g, self.meta)
        start = derive.day(self.start)
        end = derive.day(self.stop) if self.stop else start + 730
        slots = derive.research_slots(g, self.tag, self.ctx, self.start, self.meta.get("history_file")) or 1
        penalty = self.define("BASE_YEAR_AHEAD_PENALTY_FACTOR", 2)
        blocks = []
        for i, e in enumerate(self.plan.get("research_path") or []):
            if not isinstance(e, dict):
                continue
            dated = bool(e.get("not_before")) or isinstance(e.get("year"), int)
            due = derive.day(e["not_before"]) if e.get("not_before") and derive.day(e["not_before"]) else (
                derive.day((e["year"], 1, 1)) if isinstance(e.get("year"), int) else start)
            blocks.append((max(start, due), f"research_path[{i}]", dated,
                           [t for t in e.get("techs") or [] if t in techs]))
        prio = [t for t in self.plan.get("research_priority") or [] if t in techs]
        planned = {t for b in blocks for t in b[3]} | set(prio)
        rb = self.plan.get("research_bonuses")
        bonus = {t for k, v in rb.items() if k not in ("why", "note") and isinstance(v, list) for t in v} \
            if isinstance(rb, dict) else set()
        year_day = lambda t: derive.day((techs[t]["year"], 1, 1))  # noqa: E731
        finish, started = {}, {}
        free = [start] * slots
        while True:
            k = min(range(slots), key=lambda s: free[s])
            now = free[k]
            if now > end:
                break
            done = have | {t for t, f in finish.items() if f <= now}
            pick = None
            for _, where, dated, ts in [b for b in blocks if b[0] <= now] + [(start, "research_priority", False, prio)]:
                for t in ts:
                    if t in have or t in started or techrule.missing(techs[t], done, "any"):
                        continue
                    pick = (t, where, dated)
                    break
                if pick:
                    break
            if pick is None:
                later = [x for x in list(finish.values()) + [b[0] for b in blocks] + free if x > now]
                if not later:
                    break
                free[k] = min(later)
                continue
            t, where, dated = pick
            ahead = max(0.0, (year_day(t) - now) / 365.0) if techs[t]["year"] else 0.0
            started[t] = (now, where, dated, ahead)
            finish[t] = free[k] = now + derive.tech_days(g, t) * (1 + penalty * ahead)
        goals = self.domain_goals()
        folders = {(meta.get(t) or {}).get("folder") for t in planned} - {None}
        last_year = derive.as_tuple(self.stop)[0] if self.stop else derive.as_tuple(self.start)[0] + 1
        out = defaultdict(list)
        for t, (now, where, dated, ahead) in started.items():
            if ahead <= AHEAD_YEARS or t in bonus or dated or (where == "research_priority" and techs[t]["year"] > last_year):
                continue  # dated blocks: research.start_year; far-future priority techs: its window warning
            year = derive.date_of(now)[0]
            known = have | planned
            idle = []
            for c, info in techs.items():
                m = meta.get(c) or {}
                f = m.get("folder") or ""
                if c in known or not info["year"] or info["year"] > year or m.get("gated") or (m.get("xor") or set()) & known \
                        or f not in folders or techrule.missing(info, known, "any"):
                    continue
                if any(folder_family(f) == d and not goals[d] for d in ("naval", "air")):
                    continue
                idle.append(c)
            if idle:
                out[where].append((ahead, t, now, sorted(idle)))
        for where, rows in sorted(out.items()):
            rows.sort(key=lambda r: -r[0])
            idle = sorted({c for r in rows for c in r[3]})
            sev = "error" if rows[0][0] >= 2 * AHEAD_YEARS else "warning"
            self.add(sev, "research.ahead", where,
                     "researched ahead of time: " + "; ".join(
                         f"{t} ({techs[t]['year']}) from about {derive.fmt(derive.date_of(now))}, {a:.1f} year(s) "
                         f"ahead, x{1 + penalty * a:.1f} the time" for a, t, now, _ in rows[:6])
                     + f" (BASE_YEAR_AHEAD_PENALTY_FACTOR {penalty:g} a year), while in-year techs of the same fields "
                       f"stay unplanned: {', '.join(idle[:8])}{' ...' if len(idle) > 8 else ''}; research those first "
                       f"or pair the pull-forward with an ahead-of-time bonus (research_bonuses)")

    # ------------------------------------------------------------------ navy (tactics)
    def navy_invasions(self):
        """navy.invasion_prep (preparation and division cap from the start techs) and navy.sea_control
        (patrol or strike force on every sea region the invasion crosses)."""
        invs = self.list_of("invasions")
        if not invs:
            return
        g = self.game
        have = start_tech_set(g, self.meta)
        tmods = derive.tech_modifiers(g)
        imods = derive.idea_modifiers(g)
        ideas = derive.start_ideas(g, self.tag, self.ctx, self.start, self.meta.get("history_file"))

        def total(key):
            parts = [(t, tmods.get(t, {}).get(key, 0.0)) for t in sorted(have)] + \
                [(i, imods.get(i, {}).get(key, 0.0)) for i in sorted(ideas)]
            return sum(v for _, v in parts), [n for n, v in parts if v]

        prep_mod, prep_by = total("naval_invasion_prep_days")
        cap_mod, cap_by = total("naval_invasion_division_cap")
        prep = self.define("NAVAL_INVASION_PREPARE_DAYS", 60) + prep_mod
        cap = self.define("BASE_NAVAL_INVASION_DIVISION_CAP", 4) + cap_mod
        later = sorted(t for t in self.planned_techs() if tmods.get(t, {}).get("naval_invasion_division_cap"))
        spec = ((((self.schema or {}).get("properties") or {}).get("invasions") or {}).get("items") or {}).get(
            "properties") or {}
        dflt = (spec.get("prepare_days") or {}).get("default", 14)
        layout = Counter()
        for e in self.list_of("army_layout"):
            if isinstance(e.get("name_hint"), str):
                layout[e["name_hint"]] += e.get("count", 1) if isinstance(e.get("count"), int) else 1
        self.facts["invasion"] = {"prepare_days": prep, "division_cap": cap}
        for i, inv in enumerate(invs):
            where = f"invasions[{i}] {inv.get('id', '')}".strip()
            days = inv.get("prepare_days", dflt)
            if isinstance(days, (int, float)) and days < prep:
                self.add("error", "navy.invasion_prep", where,
                         f"prepare_days {days:g}{'' if 'prepare_days' in inv else ' (the default)'} but an invasion "
                         f"needs {prep:g} days of preparation (NAVAL_INVASION_PREPARE_DAYS"
                         + (f" with {', '.join(prep_by)}" if prep_by else "") + "); the plan cannot execute before then")
            armies = [a for a in inv.get("armies") or [] if isinstance(a, str)]
            n = sum(layout.get(a, 0) for a in armies)
            if n > cap:
                self.add("error", "navy.invasion_prep", where,
                         f"{n} divisions in {', '.join(armies)} but an invasion takes at most {cap:g} with the start "
                         f"techs (BASE_NAVAL_INVASION_DIVISION_CAP" + (f" with {', '.join(cap_by)}" if cap_by else "")
                         + ")" + (f"; the planned {', '.join(later)} raise it" if later else "")
                         + f": split it into invasions of {cap:g} or research the transport techs first")
            self.sea_control(where, inv)

    def sea_regions(self, inv):
        """[(region id, name)] of the sea regions on the shortest sea route from from_state to the target."""
        graph = self.game.map_graph()
        if graph is None:
            return []
        states = self.states()

        def seas(provs):
            return {s for p in provs for s in mapgraph.sea_neighbours(graph, p)}

        src = seas(states.get(inv.get("from_state"), {}).get("provinces", []))
        tp = inv.get("target_province")
        dst = seas([tp] if isinstance(tp, int) else states.get(inv.get("target_state"), {}).get("provinces", []))
        if not src or not dst:
            return []
        route = mapgraph.sea_route(graph, sorted(src), sorted(dst))
        if not route:
            return []
        regions = self.world.strategic_regions()
        out = []
        for p in route["path"]:
            rid = (graph["provinces"].get(p) or {}).get("region")
            if rid is not None and rid not in [r for r, _ in out]:
                out.append((rid, (regions.get(rid) or {}).get("name") or str(rid)))
        return out

    def sea_control(self, where, inv):
        route = self.sea_regions(inv)
        if not route:
            return
        nv = self.plan.get("navy") if isinstance(self.plan.get("navy"), dict) else {}
        covered = set()
        for fl in nv.get("fleets") or []:
            for msn in (fl or {}).get("missions") or [] if isinstance(fl, dict) else []:
                if isinstance(msn, dict) and msn.get("mission") in ("patrol", "strike_force"):
                    for r in msn.get("regions") or []:
                        name = r.get("region") if isinstance(r, dict) else r
                        if isinstance(name, (str, int)):
                            covered.add(str(name).strip().lower())
        miss = [(rid, name) for rid, name in route if name.lower() not in covered and str(rid) not in covered]
        if miss:
            ratio = self.define("DOMINANCE_CONTROLLED_THRESHOLD_RATIO", 0.6)
            self.add("error", "navy.sea_control", where,
                     f"the invasion crosses {', '.join(n for _, n in route)}; a region counts as controlled at "
                     f"{ratio:.0%} of the needed naval dominance (DOMINANCE_CONTROLLED_THRESHOLD_RATIO), which only "
                     f"patrol and strike force build, but no navy patrol or strike_force mission covers "
                     f"{', '.join(f'{n} ({r})' for r, n in miss)}")

    # ------------------------------------------------------------------ construction (tactics)
    def construction_war_civs(self):
        """A civilian factory queued after the war starts with less than a year left to the goal."""
        costs = derive.building_costs(self.game)
        civ = {b for b, r in costs.items() if r["civilian"]}
        start = derive.day(self.start)
        war_days = [start] if self.tag and self.world.country(self.tag)["wars"] else []
        for d in self.list_of("diplomacy"):
            if d.get("action") == "declare":
                ds = predicate_dates(d["when"], self.table) if d.get("when") is not None else []
                if ds:
                    war_days.append(derive.day(max(ds)))
        horizon = self.horizon()
        if not war_days or not horizon:
            return
        war = min(war_days)
        end, label = min(horizon)
        jobs = []
        for prefix, items in self.construction_lists():
            for i, e in enumerate(items):
                if e.get("building") in civ:
                    jobs.append((f"{prefix}[{i}] {e.get('id', '')}".strip(), e["building"],
                                 max(war, self.trigger_day(e.get("when", "")))))
        for i, e in enumerate(self.list_of("construction")):
            if e.get("building") in civ:
                jobs.append((f"construction[{i}]", e["building"], war))  # the filler runs whenever civs are idle
        for where, b, d in jobs:
            if end - d < WAR_CIV_DAYS:
                self.add("warning", "construction.war_civs", where,
                         f"{b} ({costs[b]['cost']:g} points) queued at war on {derive.fmt(derive.date_of(d))}, "
                         f"{end - d} days before the {label} {derive.fmt(derive.date_of(end))}: a civilian factory "
                         f"cannot pay back by then; queue military factories, railways or supply instead")

    # ------------------------------------------------------------------ navy and peace (tactics)
    def navy_strike_without_patrol(self):
        """navy.strike_without_patrol: a strike force reacts only to what the patrols spot, so each of its
        regions needs an own patrol mission."""
        nv = self.plan.get("navy") if isinstance(self.plan.get("navy"), dict) else {}

        def regions(msn):
            out = []
            for r in msn.get("regions") or []:
                name = r.get("region") if isinstance(r, dict) else r
                if isinstance(name, (str, int)) and not isinstance(name, bool):
                    out.append(str(name).strip())
            return out

        patrolled, strikes = set(), []
        for fi, fl in enumerate(nv.get("fleets") or []):
            if not isinstance(fl, dict):
                continue
            for mi, msn in enumerate(fl.get("missions") or []):
                if not isinstance(msn, dict):
                    continue
                if msn.get("mission") == "patrol":
                    patrolled |= {r.lower() for r in regions(msn)}
                elif msn.get("mission") == "strike_force":
                    strikes.append((f"navy.fleets[{fi}].missions[{mi}]", regions(msn)))
        for where, regs in strikes:
            miss = [r for r in regs if r.lower() not in patrolled]
            if miss:
                self.add("warning", "navy.strike_without_patrol", where,
                         f"strike force in {', '.join(miss)} with no own patrol mission there: a strike force only "
                         f"engages what patrols spot; add a patrol entry (submarines or light cruisers) for "
                         f"{'that region' if len(miss) == 1 else 'those regions'}")

    def peace_order(self):
        """peace.order (info): peace.take ranks a state the plan does not hold at the conference ahead of one
        it holds (an occupied state is cheaper and the first picks decide what is left)."""
        p = self.plan.get("peace") if isinstance(self.plan.get("peace"), dict) else {}
        take = _ints(p.get("take"))
        if len(take) < 2 or not self.tag:
            return
        owners = self.game.state_owners(self.ctx, self.start)
        held = {s for s, t in owners.items() if t == self.tag}
        held |= {e["state"] for e in self.targets() if isinstance(e.get("state"), int)}
        held |= set(self.axis_states())
        first_held = next((i for i, s in enumerate(take) if s in held), None)
        early = [s for s in take[:first_held] if s not in held] if first_held is not None else []
        if early:
            self.add("info", "peace.order", "peace.take",
                     (f"state {early[0]} is" if len(early) == 1 else f"states {', '.join(map(str, early))} are")
                     + f" ranked ahead of state {take[first_held]}, but the plan "
                     f"does not take them before the conference (no vp target or axis there); rank the states the "
                     f"armies hold first, they cost fewer points")

    # ------------------------------------------------------------------
    def run(self):
        steps = [self.schema_rules, self.research, self.focus, self.construction, self.production, self.army,
                 self.conquest, self.win, self.events, self.coverage, self.posture, self.navy, self.port_convoys,
                 self.convoy_size, self.xp_unspent, self.hub_cost, self.supply_axis_reach, self.air_bases, self.templates, self.laws, self.occupation, self.peace, self.manage,
                 self.alarms_silenced,
                 self.production_changes, self.unreadable,
                 # tactics research rules (production.day0 reads production's factory count, pp_budget the focus
                 # timeline)
                 self.production_mature_switch, self.production_cull, self.production_day0,
                 self.production_in_progress, self.production_naval_cap, self.army_width_terrain,
                 self.war_supply_floor, self.war_planning_wait, self.war_pocket_threshold, self.politics_pp_budget,
                 self.politics_conscription, self.research_ahead, self.navy_invasions, self.construction_war_civs,
                 self.navy_strike_without_patrol, self.peace_order]
        for step in steps:
            step()
        __import__((__package__ or "hoi4facts") + ".lint_fair", fromlist=["check"]).check(self)  # fair.* (FAIR mode)
        order = {s: i for i, s in enumerate(SEVERITIES)}
        self.findings.sort(key=lambda f: (order[f["severity"]], f["rule"]))
        return self


def _has_call(node, depth=0):
    """True when a dict or list holds an item with a {"tool": ...} call."""
    if depth > 6:
        return False
    if isinstance(node, dict):
        if isinstance(node.get("call"), dict) and node["call"].get("tool"):
            return True
        return any(_has_call(v, depth + 1) for v in node.values())
    if isinstance(node, list):
        return any(_has_call(v, depth + 1) for v in node)
    return False


def derive_crossings(graph, path):
    return [[a, b] for a, b in zip(path, path[1:]) if graph["adj"][a][b].get("river_crossing")]


def assignable(rows, supply):
    """Most divisions that can be given to rows [(label, templates, count)] from supply
    {template: divisions} at once (max flow, entries x templates)."""
    temps = sorted(supply)
    n = len(rows)
    src, sink = 0, n + len(temps) + 1
    cap = defaultdict(int)
    nbr = defaultdict(set)

    def edge(a, b, c):
        cap[(a, b)] += c
        nbr[a].add(b)
        nbr[b].add(a)

    for i, (_, ts, c) in enumerate(rows):
        edge(src, 1 + i, c)
        for t in ts:
            if t in supply:
                edge(1 + i, 1 + n + temps.index(t), c)
    for j, t in enumerate(temps):
        edge(1 + n + j, sink, supply[t])
    flow = 0
    while True:
        prev = {src: None}
        q = deque([src])
        while q and sink not in prev:
            a = q.popleft()
            for b in nbr[a]:
                if b not in prev and cap[(a, b)] > 0:
                    prev[b] = a
                    q.append(b)
        if sink not in prev:
            return flow
        path, b = [], sink
        while prev[b] is not None:
            path.append((prev[b], b))
            b = prev[b]
        push = min(cap[e] for e in path)
        for a, b in path:
            cap[(a, b)] -= push
            cap[(b, a)] += push
        flow += push


def match_or_none(name, table):
    return derive.match_commander(name, table)


def lint(plan, game, k=FRONT_K, schema=None, hand=None):
    """{"errors": n, "warnings": n, "infos": n, "findings": [...], "facts": {...}}. hand: True reports
    the coverage-build rules as warnings (default: when _meta.source is "hand")."""
    lt = Lint(plan, game, k, schema, hand).run()
    count = Counter(f["severity"] for f in lt.findings)
    return {"errors": count["error"], "warnings": count["warning"], "infos": count["info"],
            "findings": lt.findings, "facts": lt.facts}


def load_game(game_dir, userdir=None, mod_args=()):
    mods = content.mods_from_userdir(userdir) if userdir else []
    for arg in mod_args:
        m = content.mod_from_arg(arg, userdir)
        if m is None:
            raise SystemExit(f"--mod {arg}: not a .mod descriptor or a mod folder")
        mods.append(m)
    return Game(game_dir, mods)


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("plan")
    ap.add_argument("--game", default=os.environ.get("HOI4_DIR", DEFAULT_GAME_DIR))
    ap.add_argument("--userdir", default=None, help="user folder whose enabled mods are layered over the install")
    ap.add_argument("--mod", action="append", default=[], help="a .mod descriptor or mod folder (repeatable)")
    ap.add_argument("--front-distance", type=int, default=FRONT_K, help="K: province steps that count as a front")
    ap.add_argument("--json", action="store_true", help="print the findings and facts as JSON")
    ap.add_argument("--hand", action="store_true",
                    help="hand-written plan: report the coverage-build rules as warnings (as _meta.source hand)")
    args = ap.parse_args(argv)
    try:
        with open(args.plan, encoding="utf-8") as fh:
            plan = json.load(fh)
    except (OSError, ValueError) as exc:
        print(f"cannot read plan: {exc}", file=sys.stderr)
        return 2
    if not isinstance(plan, dict):
        print("plan must be a JSON object", file=sys.stderr)
        return 2
    if not os.path.isdir(args.game):
        print(f"game directory not found: {args.game}", file=sys.stderr)
        return 2
    out = lint(plan, load_game(args.game, args.userdir, args.mod), args.front_distance, hand=args.hand or None)
    if args.json:
        print(json.dumps(out, indent=2, default=sorted))
    else:
        for f in out["findings"]:
            print(f"{f['severity']}: [{f['rule']}] {f['where']}: {f['message']}")
        print(f"lint: {out['errors']} error(s), {out['warnings']} warning(s), {out['infos']} info")
    return 1 if out["errors"] else 0


if __name__ == "__main__":
    sys.exit(main())
