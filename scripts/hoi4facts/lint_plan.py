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
  research.idle            more than 25% of the slots' time to the horizon has nothing planned: an error
                           with research_fallback off, else a warning naming what the fallback uses
  research.throughput      (warning) a block's base research days do not fit the slots before the
                           next block year (research_cost x BASE_TECH_COST, no speed bonus)
  research.prerequisites   prerequisite not at the start or earlier in the path under either parent
                           reading; warning when only the any-parent reading allows it
  research.folders         (warning) naval or air folder techs without a naval or air goal
  research.fallback        research_fallback on without research_fallback_prefixes/categories
  focus.exclusive          two mutually exclusive focuses in focus_path / focus_fallback
  focus.order              a focus planned before its prerequisites (timeline from the focus costs)
  focus.gates              (info) focuses with a runtime `available` gate, with the planned start
  construction.front       factory in a state within K province steps of an enemy-held province
  construction.infrastructure  factory in a state with infrastructure <= 2 and no infrastructure
                           queued before it there
  construction.railway     railway path provinces not touching, or not controlled when the
                           trigger fires (start control plus the vp_targets taken by then)
  construction.trigger_route  a province_controlled: trigger off every vp target route
  construction.hub         supply hub in a province or state an enemy still holds when it fires
  production.factories     production lines need more military factories than the plan has
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
  conquest.threshold       vp_targets below the enemy's surrender threshold (base limit plus its
                           starting surrender_limit modifiers), when the goal is a capitulation
  conquest.mandatory       a state the goal must control has no vp target (warning: provinces of a
                           fully-controlled state left out)
  conquest.adjacency       target (or the first province of its path) not next to own land or an
                           earlier target; path provinces not touching
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

Coverage-build rules (coverage audit section 3 "Check" column; W1-2):
  coverage.checklist       no top-level _coverage, a checklist id c1..c29 missing, a value that is not
                           key:<plan key>[,...] | call[:<key>] | md:<plan.md section> | na:<reason>, a key
                           the plan lacks, or na: "missing ..." (a gap, not a reason)
  posture.shares           (warning) production factory shares of a force_posture phase more than 15
                           points from the posture; error when a posture phase does not sum to 100
  navy.required            a coastal country (an owned coastal province) without a navy key
  navy.overseas            an army state the capital cannot reach over own land, without a navy key,
                           a convoy plan (convoy production line or sea_transfer.convoys) and a port plan
                           (sea_transfer.ports or a port building queued there)
  supply.port_convoys      the plan builds a port or relies on one (sea_transfer.ports, an overseas army)
                           but has no convoy item (convoy production line or manage.convoys_free);
                           sea supply needs SUPPLY_CONVOY_FACTOR convoys per supply
  construction.hub_cost    a supply hub (or any building of base_cost >= 15000) finishes after the goal
                           deadline or the plan horizon: base_cost / (min(15, free civs) x points per civ
                           x infrastructure speed); warning above 120 days, or when an existing or
                           capturable hub within 3 steps could serve the same provinces by railway; the
                           finding names the port alternative when an own coastal province is 3 steps away
  air.base_capacity        (warning) air_plan planes on a base above its capacity (level at the start plus
                           queued levels, x AIRBASE_CAPACITY_MULT)
  templates.width          (warning) template width more than 10% from the theatre target (placeholder:
                           derive.theatre_width_target over common/terrain combat widths)
  templates.equipment      a template unit or its equipment unlocked by no tech at the start or in the
                           research plan (warning when the tech comes after the template's date)
  templates.xp             a template with xp and no xp_budget.army spend ref template:<id>
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
  production.remove        (warning) production remove/replace entries: not read by the executor yet
  predicates.unreadable    (warning) a goal, fail predicate, checkpoint, phase or contingency that can never
                           hold because it rests on atoms the executor cannot read yet (x-atoms readable:
                           false: convoys_free, stockpile_days, var, bop, mission_days)

Hand plans: with _meta.source "hand" (or --hand) every coverage-build error above is reported as a
warning, so the two hand plans keep 0 errors while their gaps stay visible; the older rules are
unchanged.

Stdlib only, Python 3.9+. Exit status 1 when there are errors, 2 on usage problems.
"""
import argparse
import json
import os
import re
import sys
from collections import Counter, defaultdict, deque

if __package__ in (None, ""):
    sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
    from hoi4facts import content, derive, schema_check  # noqa: E402
    from hoi4facts import tech as techrule  # noqa: E402
    from hoi4facts.game import Game  # noqa: E402
    from hoi4facts.validate import (Report, ctx_of, load_schema, parse_predicate, parse_when,  # noqa: E402
                                    predicate_dates, predicate_table, resolve_meta, start_of, start_oobs,
                                    start_tech_set, template_columns,
                                    predicate_can_hold)
else:
    from . import content, derive, schema_check
    from . import tech as techrule
    from .game import Game
    from .validate import (Report, ctx_of, load_schema, parse_predicate, parse_when, predicate_dates,
                           predicate_table, resolve_meta, start_of, start_oobs, start_tech_set, template_columns,
                           predicate_can_hold)

DEFAULT_GAME_DIR = os.path.expanduser("~/Library/Application Support/Steam/steamapps/common/Hearts of Iron IV")
FRONT_K = 2
MAX_STRIKE = 24
LOW_INFRA = 2
MAX_RIVERS = 2
POSTURE_POINTS = 15        # posture.shares: percentage points a production share may differ
WIDTH_TOLERANCE = 0.10     # templates.width: share of the theatre target
HUB_COST = 15000           # construction.hub_cost: base_cost from which a building counts as a hub-sized job
HUB_DAYS = 120             # construction.hub_cost: warning above this many days
HUB_STEPS = 3              # construction.hub_cost: steps to a hub or coastal province that could serve instead
NEW_RULES = {"coverage.checklist", "posture.shares", "navy.required", "navy.overseas", "supply.port_convoys",
             "construction.hub_cost", "air.base_capacity", "templates.width", "templates.equipment", "templates.xp",
             "politics.law_gates", "occupation.required", "peace.enemies", "manage.thresholds", "production.remove",
             "predicates.unreadable"}
SEVERITIES = ("error", "warning", "info")
_PRED_TAG = re.compile(r"^(capitulated|at_war|at_war_with|exists|not_exists|not_at_war):([A-Z][A-Z0-9]{2})$")
_PRED_STATE = re.compile(r"^(state_controller|owner|controller):(\d+)\s*==?\s*([A-Z][A-Z0-9]{2})$"
                         r"|^(state_fully_controlled|controls_state):(\d+)$")


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
            if idle > 0.25 and fallback in (False, "priority_only"):
                self.add("error", "research.idle", "research_path",
                         f"about {idle:.0%} of the {slots} slots' time to {derive.fmt(derive.date_of(end_day))} has "
                         f"nothing planned ({total:.0f} of {capacity} slot-days) and research_fallback is off: "
                         f"extend research_path with in-time techs or enable a restricted fallback")
            elif idle > 0.25:
                self.add("warning", "research.idle", "research_path",
                         f"about {idle:.0%} of the slots' time has nothing planned; the fallback fills it from "
                         f"{', '.join(plan.get('research_fallback_prefixes') or ['the whole catalogue'])}")
        # naval / air folders
        planned = [t for b in blocks for t in b[3]] + [x for x in plan.get("research_priority") or [] if isinstance(x, str)]
        fams = defaultdict(list)
        for t in planned:
            f = (meta.get(t) or {}).get("folder") or ""
            for fam in ("naval", "air"):
                if re.search(r"(^|_)" + fam + r"(_|$)", f):
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
        mils = sum(v for s in own for b, v in sb.get(s, {}).items() if b in kinds["military"])
        planned = 0
        for _, items in self.construction_lists()[:1]:
            planned += sum(1 for e in items if e.get("building") in kinds["military"])
        for e in self.list_of("construction"):
            if e.get("building") in kinds["military"]:
                planned += int(e.get("count", 1) or 1) * max(1, len(e.get("states") or []))
        lines = self.list_of("production")
        start = derive.day(self.start)
        dates = sorted({start} | {derive.day(x["from"]) for x in lines if derive.day(x.get("from")) is not None})
        for d in dates:
            n = sum(self.active_lines(d).values())
            cap = mils if d == start else mils + planned
            if n > cap:
                extra = "" if d == start else f" plus {planned} planned"
                self.add("error", "production.factories", "production",
                         f"{n} factories allocated from {derive.fmt(derive.date_of(d))}, but {self.tag} has {mils} "
                         f"military factories at the start{extra}")
        self.facts["military_factories"] = {"start": mils, "planned": planned}
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
            if e.get("role") == "guard":
                guarded.update(_ints(e.get("guard_states_in_order")))
                if isinstance(e.get("state"), int) and not e.get("guard_states_in_order"):
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
        self.generals(layout, armies)

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
                self.add("error", "conquest.threshold", "vp_targets",
                         f"targets hold {got:g} of {enemy}'s {s['total_vp']:g} VP; capitulation needs {s['vp_needed']} "
                         f"(limit {s['limit']:g}: base {s['base']:g}, {mods})")
        self.facts["surrender"] = surr
        self.conquest_skipped_near(targets, vps, owners, pstate)
        must = self.goal_states()
        self.facts["goal_states"] = {str(k): v for k, v in sorted(must.items())}
        start_ctl = self.start_control()
        for sid, kind in sorted(must.items()):
            provs = game.states().get(sid, {}).get("provinces", [])
            mine = [p for p in provs if p in start_ctl]
            tp = [e for e in targets if e.get("state") == sid or e["province"] in provs]
            if not tp and len(mine) < len(provs):
                self.add("error", "conquest.mandatory", "vp_targets",
                         f"the goal needs state {sid} ({kind} control) but no vp target is in it")
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
        for i, e, leg, problems in self.legs():
            where = f"vp_targets[{i}] {e['province']}"
            for msg in problems:
                self.add("error", "conquest.adjacency", where, msg)
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
            lines = self.active_lines(d)
            total = sum(lines.values())
            if not total:
                continue
            got = {k: 0.0 for k in want}
            for eq, n in lines.items():
                got[domain(eq)] += 100.0 * n / total
            rows.append({"phase": name, "date": derive.fmt(derive.date_of(d)), "plan": want,
                         "production": {k: round(v, 1) for k, v in got.items()}})
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
        theatre = _ints(self.war.get("theatre")) or sorted({e.get("state") for e in self.targets()}
                                                           | {e.get("state") for e in self.list_of("army_layout")} - {None})
        target = derive.theatre_width_target(game, theatre) if theatre else None
        self.facts["width_target"] = target
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
        for i, t in enumerate(temps):
            where = f"templates[{i}] {t.get('id', t.get('name'))}"
            cols = template_columns(t.get("regiments")) or []
            width = derive.template_width(game, cols)
            if target and width and t.get("role") != "garrison" \
                    and abs(width - target["target"]) > WIDTH_TOLERANCE * target["target"]:
                mix = ", ".join(f"{k} {v:.0%}" for k, v in target["mix"].items())
                self.add("warning", "templates.width", where,
                         f"width {width:g} is more than {WIDTH_TOLERANCE:.0%} from the theatre target "
                         f"{target['target']} (terrain {mix}; placeholder until facts.py terrain-width)")
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
        for i, x in enumerate(self.list_of("production")):
            if x.get("remove") or "replace" in x:
                self.add("warning", "production.remove", f"production[{i}]",
                         f"{'remove' if x.get('remove') else 'replace'} of {x.get('equipment')} is not read by the "
                         f"executor yet (W1-9): it would add this line; do it by hand until then")

    # ------------------------------------------------------------------
    def run(self):
        steps = [self.schema_rules, self.research, self.focus, self.construction, self.production, self.army,
                 self.conquest, self.win, self.events, self.coverage, self.posture, self.navy, self.port_convoys,
                 self.hub_cost, self.air_bases, self.templates, self.laws, self.occupation, self.peace, self.manage,
                 self.production_changes, self.unreadable]
        for step in steps:
            step()
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
