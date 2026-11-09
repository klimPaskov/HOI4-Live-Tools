#!/usr/bin/env python3
"""Deterministic plan lint (strategist design 4.2): strategy rules a schema-valid plan can still break.

Usage:
    lint_plan.py PLAN.json [--game DIR] [--userdir DIR] [--mod PATH ...] [--front-distance K] [--json]

validate_plan.py checks that every key is something the game and the executor know. The lint checks
that the plan is a sound strategy against the game files: nothing researched ahead of time, no
factory on a front, every border guarded, enough victory points for a capitulation, routes that
connect, a goal with predicates. Every number comes from the effective game content (install plus
mods) through hoi4facts.Game and hoi4facts.derive; nothing is country specific.

Rules (error unless marked; rule ids match skills/hoi4-campaign-playbook/references/lessons.jsonl):
  research.start_year      research_path tech with start_year after its block year
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
                           war.guard_states or war.guards)
  army.templates           layout template not in the start order of battle
  army.counts              layout count above the divisions of its templates in its state (warning
                           when the entries of one state together need more than it has)
  army.generals            general unknown or not a commander of the tag; one general in two places
  army.strike_size         strike entry above 24 divisions
  conquest.threshold       vp_targets below the enemy's surrender threshold (base limit plus its
                           starting surrender_limit modifiers), when the goal is a capitulation
  conquest.mandatory       a state the goal must control has no vp target (warning: provinces of a
                           fully-controlled state left out)
  conquest.adjacency       target (or the first province of its path) not next to own land or an
                           earlier target; path provinces not touching
  conquest.rivers          (warning) more than 2 river crossings on one leg
  conquest.by_order        (warning) `by` dates going backwards for one army
  win.goal                 no goal.predicates
  win.deadline             goal.deadline differs from a scenario mission timer (info when the goal
                           has no deadline and timers exist)
  events.options           event_rules option names / event_options indexes that the event lacks
  schema.keys              schema errors (unknown keys, except _-prefixed)
  schema.player_call       player-owned items without a `call` (the tool call they stand for)

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
    from hoi4facts.validate import (Report, ctx_of, load_schema, parse_when, resolve_meta,  # noqa: E402
                                    start_of, start_oobs, start_tech_set)
else:
    from . import content, derive, schema_check
    from . import tech as techrule
    from .game import Game
    from .validate import Report, ctx_of, load_schema, parse_when, resolve_meta, start_of, start_oobs, start_tech_set

DEFAULT_GAME_DIR = os.path.expanduser("~/Library/Application Support/Steam/steamapps/common/Hearts of Iron IV")
FRONT_K = 2
MAX_STRIKE = 24
LOW_INFRA = 2
MAX_RIVERS = 2
SEVERITIES = ("error", "warning", "info")
_PRED_TAG = re.compile(r"^(capitulated|at_war|exists|not_exists|not_at_war):([A-Z][A-Z0-9]{2})$")
_PRED_STATE = re.compile(r"^(state_controller|owner):(\d+)=([A-Z][A-Z0-9]{2})$|^state_fully_controlled:(\d+)$")


def _atoms(pred):
    """Atom strings of a predicate (composites flattened; NOT branches left out)."""
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
    def __init__(self, plan, game, k=FRONT_K, schema=None):
        self.plan = plan
        self.game = game
        self.k = k
        self.schema = schema if schema is not None else load_schema()
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
                if m and m.group(1) in ("capitulated", "at_war"):
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
            active = [x for x in lines if (derive.day(x.get("from")) or start) <= d]
            n = sum(x.get("factories", 0) for x in active if isinstance(x.get("factories"), int))
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

    def generals(self, layout, armies):
        table = derive.commanders(self.game, self.tag, self.ctx, self.start, self.meta.get("history_file"))
        uses = []  # (where, name, needs field marshal)
        for i, n in enumerate(armies.get("generals") or []):
            uses.append((f"armies.generals[{i}]", n, False, "pool"))
        for i, e in enumerate(layout):
            if isinstance(e.get("general"), str):
                uses.append((f"army_layout[{i}]", e["general"], False, "command"))
        for i, e in enumerate(self.list_of("army_group")):
            for key in ("field_marshal", "fallback_field_marshal"):
                if isinstance(e.get(key), str):
                    uses.append((f"army_group[{i}].{key}", e[key], True, "command" if key == "field_marshal" else "fallback"))
        commands = defaultdict(list)
        pool = Counter()
        for where, name, fm, kind in uses:
            if not isinstance(name, str):
                continue
            cid = match_or_none(name, table)
            if cid is None:
                self.add("error", "army.generals", where, f"{name!r} is not a commander of {self.tag} in common/characters")
                continue
            c = table[cid]
            if not c["recruited"]:
                self.add("warning", "army.generals", where, f"{name!r} ({cid}) is not recruited by the history at the start")
            if fm and "field_marshal" not in c["roles"]:
                self.add("error", "army.generals", where, f"{name!r} ({cid}) has no field_marshal role")
            if kind == "command":
                commands[cid].append(where)
            elif kind == "pool":
                pool[cid] += 1
        for cid, wheres in commands.items():
            if len(wheres) > 1:
                self.add("error", "army.generals", ", ".join(wheres), f"{table[cid]['display']} commands more than one army")
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
                    out[int(m.group(4))] = "full"
                elif m.group(3) == self.tag:
                    out[int(m.group(2))] = "full" if m.group(1) == "state_controller" else "owner"
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

    # ------------------------------------------------------------------
    def run(self):
        steps = [self.schema_rules, self.research, self.focus, self.construction, self.production, self.army,
                 self.conquest, self.win, self.events]
        for step in steps:
            step()
        order = {s: i for i, s in enumerate(SEVERITIES)}
        self.findings.sort(key=lambda f: (order[f["severity"]], f["rule"]))
        return self


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


def lint(plan, game, k=FRONT_K, schema=None):
    """{"errors": n, "warnings": n, "infos": n, "findings": [...], "facts": {...}}."""
    lt = Lint(plan, game, k, schema).run()
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
    out = lint(plan, load_game(args.game, args.userdir, args.mod), args.front_distance)
    if args.json:
        print(json.dumps(out, indent=2, default=sorted))
    else:
        for f in out["findings"]:
            print(f"{f['severity']}: [{f['rule']}] {f['where']}: {f['message']}")
        print(f"lint: {out['errors']} error(s), {out['warnings']} warning(s), {out['infos']} info")
    return 1 if out["errors"] else 0


if __name__ == "__main__":
    sys.exit(main())
