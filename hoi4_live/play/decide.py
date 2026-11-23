"""Decide only: the executor's lanes judged on a supplied snapshot; the actions they would take, as data.

Opt-in and separate from play: nothing here attaches to a game, takes the input lease or sends input. Another
program (a planner, a test, a tool that carries decisions out some other way) hands in the plan and a snapshot
of the country it plays, and gets back what the executor's lanes would do next, with the plan's own reasons:

    python -m hoi4_live.play.decide --plan plan.json --snapshot snap.json [--memory memory.json] [--out out.json]

or `decide(plan, snap, memory)` from Python. The answer:

    {"schema": 1, "mode": "decide_only", "date", "country",
     "goal": text or null, "phase": {"id", "goal"} or null,
     "actions": [{"lane", "domain", "tool", "args", "why", "weight": "minor"|"heavy", "basis": "state"|"plan",
                  "key", "directive"?: {"kind", "target", "states", "priority"}}],
     "waiting": [{"lane", "key", "unknown": [predicate atoms]}],
     "blind":   [{"lane", "domain", "needs": [snapshot blocks], "why"}],
     "workload": {"level": "fighting"|"heavy"|"minor"|"idle", "why", "lanes"},
     "findings": [{"code", "severity", "text", "subject"}],
     "memory": {...}}           # pass back on the next call: what was already decided and when

Lanes, their order and their weight are the executor's (native/src/plan_run.rs `lanes()` and LANE_WEIGHT; a
test keeps the two in step). Each lane here is the plan-level half of the executor's lane: what to do and why,
from the plan's keys and the snapshot's blocks. The input-level half (which button, which province, retries,
read-backs) stays in the executor and is not modelled.

A snapshot is the shape `hoi4_live.observe.snapshot.build` returns. A snapshot that was not read from the game
may say which of its blocks it really knows: `"blocks": {"production": "unknown", "politics": "partial", ...}`
(no `blocks` key: everything known). A lane whose block is unknown decides from the plan alone (`basis: "plan"`,
once per item, paced by game time) when the plan names the work, and is listed under `blind` when the work comes
from the state alone (a shortage, an idle wing).

`workload` is the speed rule's measure (plan_speed.rs): fighting when a war is managed (an attack ordered on a
country we are at war with, or own states lost or threatened), heavy when a heavy lane would act, minor when only
minor lanes would, idle when nothing is due.
"""
from __future__ import annotations

import argparse
import json
import sys

from ..observe.snapshot import days_between, g, num

SCHEMA = 1

#: (lane, minor, domain): the executor's lanes in its pass order (plan_run.rs `lanes()`, the fast lanes first);
#: `minor` as in LANE_WEIGHT. Lanes with no plan-level half here (reads only, or a learned skill) are listed so
#: the order and weights stay one table.
LANES = [
    ("events", True, "events"),          # tick step 1, ahead of every lane (not in LANE_WEIGHT: an event is minor)
    ("research", True, "research"),
    ("focus", True, "focus"),
    ("hold", False, "war"),
    ("manage", True, "signals"),
    ("stockpile", False, "production"),
    ("balance", False, "production"),
    ("production", False, "production"),
    ("naval_queue", False, "navy"),
    ("dockyards", False, "navy"),
    ("convoys", False, "navy"),
    ("construction", False, "construction"),
    ("politics", True, "politics"),
    ("officer_corps", True, "army_command"),
    ("mio_lines", False, "production"),
    ("mio", True, "production"),
    ("mio_traits", True, "production"),
    ("extras", True, "doctrine"),
    ("decisions", True, "decisions"),
    ("learned", False, "other"),
    ("training", False, "forces"),
    ("templates", False, "forces"),
    ("template_cleanup", False, "forces"),
    ("air_plan", False, "air"),
    ("air_reserve", False, "air"),
    ("air_idle", False, "air"),
    ("rests", False, "war"),
    ("invasion", False, "war"),
    ("casualties", False, "war"),
    ("air_superiority", False, "air"),
    ("army_groups", False, "army_command"),
    ("navy", False, "navy"),
    ("intelligence", True, "intelligence"),
    ("occupation", True, "occupation"),
    ("resources", False, "trade"),
    ("import_budget", True, "trade"),
    ("fuel", True, "trade"),
    ("country", True, "politics"),
    ("overdrawn", False, "production"),
    ("equipment", False, "production"),
    ("diplomacy", True, "diplomacy"),
    ("faction", True, "diplomacy"),
    ("call_allies", True, "diplomacy"),
    ("incoming", True, "diplomacy"),
    ("trade", False, "trade"),
    ("designs", False, "forces"),
    ("line_designs", False, "production"),
    ("decommission", True, "navy"),
    ("seams", True, "other"),
    ("command", False, "army_command"),
    ("armies", False, "army_command"),
    ("war", False, "war"),
    ("ship", False, "war"),
    ("disband", False, "forces"),
]
MINOR = {n: m for n, m, _ in LANES}
DOMAIN = {n: d for n, _, d in LANES}

#: game days between two plan-basis decisions of a lane that works through a list (one focus, one doctrine ...)
PLAN_PACE = {"focus": 70, "extras": 60, "research": 30, "training": 30, "politics": 30, "decisions": 30}
#: game days before an item decided from the plan alone is decided again (it may not have been carried out)
REPEAT_DAYS = 120
#: game days before a need the state shows is decided again
STATE_GAP_DAYS = 7


# --- small helpers -------------------------------------------------------------


def _date(snap):
    return str(snap.get("date") or "")


def _year(date):
    try:
        return int(str(date).split(".")[0])
    except (ValueError, IndexError):
        return None


def _on_or_after(date, when):
    """Whether game date `date` is on or after `when` ('1936.01.01' or '1936-01-01'); None when unreadable."""
    d = days_between(str(when).replace("-", "."), date)
    return None if d is None else d >= 0


def known(snap, block):
    """'known', 'partial' or 'unknown' for a snapshot block (no `blocks` map: a live snapshot, all known)."""
    blocks = snap.get("blocks")
    if not isinstance(blocks, dict):
        return "known"
    return blocks.get(block, "unknown")


def _id(item, *keys):
    if isinstance(item, dict):
        for k in ("id",) + keys:
            if item.get(k) not in (None, ""):
                return str(item[k])
    return str(item)


def _why(item, default=""):
    if isinstance(item, dict):
        for k in ("why", "reason", "note", "goal"):
            if item.get(k):
                return str(item[k])
    return default


def judge(when, snap):
    """A plan predicate on the snapshot: (True|False|None, unknown atoms). Empty holds. A dict {"all": [...]} or
    {"any": [...]} combines; a string joins atoms with " and ". Judged: dates; `focus_completed:X`;
    `state_controlled:N`, `state_fully_controlled:N`, `state_controller:N=TAG`, `owner:N=TAG` (TAG our own:
    the states we control); `at_war:TAG`; `capitulated:TAG` / `won:TAG` (wars.won); `tech:X`. Any other atom is
    unknown and the item waits, as in the executor."""
    if when in (None, "", True):
        return True, []
    if when is False:
        return False, []
    if isinstance(when, dict):
        parts = [judge(w, snap) for w in when.get("all") or when.get("any") or []]
        unknown = [u for _, us in parts for u in us]
        if "any" in when:
            if any(ok for ok, _ in parts):
                return True, []
            return (None, unknown) if unknown else (False, [])
        if any(ok is False for ok, _ in parts):
            return False, []
        return (None, unknown) if unknown else (True, [])
    if isinstance(when, list):
        return judge({"all": when}, snap)
    date = _date(snap)
    me = snap.get("country") or snap.get("player")
    atoms = [a.strip() for a in str(when).split(" and ") if a.strip()]
    unknown = []
    for atom in atoms:
        if atom[:1].isdigit():
            ok = _on_or_after(date, atom)
            if ok is None:
                unknown.append(atom)
            elif not ok:
                return False, []
            continue
        kind, _, arg = atom.partition(":")
        res = None
        if kind in ("state_controller", "owner") and "=" in arg:
            arg, _, who = arg.partition("=")
            kind = "state_controlled" if who == me else "other"
        if kind == "focus_completed":
            done = g(snap, "research", "focuses_done") or snap.get("focuses_done")
            res = None if done is None else arg in done
        elif kind in ("state_controlled", "state_fully_controlled") and arg.isdigit():
            ctl = g(snap, "wars", "states_controlled")
            if ctl is None:
                res = None
            elif known(snap, "wars") == "known":
                res = int(arg) in ctl
            else:   # a partial list proves control; a state missing from it is not known
                res = True if int(arg) in ctl else None
        elif kind == "at_war":
            en = g(snap, "wars", "enemies")
            res = None if en is None else arg in en
        elif kind in ("capitulated", "won"):
            won = g(snap, "wars", "won")
            res = True if won and arg in won else None
        elif kind == "tech":
            done = g(snap, "research", "done")
            res = None if done is None else arg in done
        if res is None:
            unknown.append(atom)
        elif not res:
            return False, []
    return (None, unknown) if unknown else (True, [])


def phase_in_force(plan, snap, memory=None):
    """The plan's phase for the snapshot, in order: the first phase, then the next once the current one's `exit`
    holds and the next one's `enter` (a date or a predicate; none: at once) holds. The index is kept in `memory`
    (`phase`) so a phase is never left backwards when a predicate becomes unreadable."""
    phases = [ph for ph in plan.get("phases") or [] if isinstance(ph, dict)]
    if not phases:
        return None
    memory = memory if memory is not None else {}
    i = min(int(memory.get("phase") or 0), len(phases) - 1)
    while i + 1 < len(phases):
        cur, nxt = phases[i], phases[i + 1]
        exit_ = cur.get("exit") or cur.get("until")
        left = judge(exit_, snap)[0] is True if exit_ else False
        enter = nxt.get("enter") or nxt.get("from")
        entered = judge(enter, snap)[0] is True if enter else left
        if not (left or (enter and entered)) or (enter and not entered):
            break
        i += 1
    # a later phase whose exit already holds (the enemy capitulated) ends every phase up to it
    for j in range(len(phases) - 2, i, -1):
        exit_ = phases[j].get("exit") or phases[j].get("until")
        if exit_ and not str(exit_)[:1].isdigit() and judge(exit_, snap)[0] is True:
            i = j + 1
            break
    memory["phase"] = i
    return phases[i]


def merged(plan, phase):
    """The plan with the phase's overrides (`overrides` on the phase, `war_by_phase[id]` over `war`)."""
    out = dict(plan)
    if not phase:
        return out
    for k, v in (phase.get("overrides") or {}).items():
        out[k] = v
    wbp = (plan.get("war_by_phase") or {}).get(phase.get("id"))
    if isinstance(wbp, dict):
        out["war"] = {**(plan.get("war") or {}), **wbp}
    return out


class _Ctx:
    def __init__(self, plan, snap, memory):
        self.plan = plan
        self.snap = snap
        self.date = _date(snap)
        self.memory = memory
        self.sent = memory.setdefault("sent", {})
        self.lane_at = memory.setdefault("lane_at", {})
        self.actions = []
        self.waiting = []
        self.blind = []

    def since(self, key):
        at = self.sent.get(key)
        return None if at is None else days_between(at, self.date)

    def fresh(self, key, basis):
        """Whether `key` may be decided now: never decided, or long enough ago for its basis."""
        d = self.since(key)
        return d is None or d >= (STATE_GAP_DAYS if basis == "state" else REPEAT_DAYS)

    def paced(self, lane):
        """A plan-basis lane that works through a list decides one item per PLAN_PACE days."""
        at = self.lane_at.get(lane)
        d = None if at is None else days_between(at, self.date)
        return d is None or d >= PLAN_PACE.get(lane, 0)

    def act(self, lane, tool, args, why, basis, key, directive=None):
        if not self.fresh(key, basis):
            return False
        row = {"lane": lane, "domain": DOMAIN.get(lane, "other"), "tool": tool, "args": args, "why": why,
               "weight": "minor" if MINOR.get(lane, False) else "heavy", "basis": basis, "key": key}
        if directive:
            row["directive"] = directive
        self.actions.append(row)
        self.sent[key] = self.date
        self.lane_at[lane] = self.date
        return True

    def wait(self, lane, key, unknown):
        self.waiting.append({"lane": lane, "key": key, "unknown": unknown})

    def no_eyes(self, lane, needs, why):
        self.blind.append({"lane": lane, "domain": DOMAIN.get(lane, "other"), "needs": needs, "why": why})


# --- lanes ---------------------------------------------------------------------


def _events(c):
    plan, snap = c.plan, c.snap
    opts = plan.get("event_options") or {}
    rules = {r.get("event"): r for r in plan.get("event_rules") or [] if isinstance(r, dict)}
    for ev in snap.get("events_open") or []:
        key = ev if isinstance(ev, str) else (ev or {}).get("event")
        if not key:
            continue
        if key in rules:
            r = rules[key]
            args = {"event": key, "option_name": r.get("option_name")} if r.get("option_name") else \
                {"event": key, "option": r.get("option", 0)}
            c.act("events", "hoi4_event_choose", args, _why(r, "the plan's rule for this event"), "state",
                  "event:" + key)
        elif key in opts:
            c.act("events", "hoi4_event_choose", {"event": key, "option": opts[key]},
                  "the plan names this option", "state", "event:" + key)


def _research_queue(plan, date):
    year = _year(date)
    out = []
    for row in plan.get("research_path") or []:
        if not isinstance(row, dict):
            continue
        if year is not None and row.get("year") and int(row["year"]) > year:
            continue
        nb = row.get("not_before")
        if nb and _on_or_after(date, nb) is False:
            continue
        for t in row.get("techs") or []:
            out.append((t, row.get("why") or f"the plan's {row.get('year')} research"))
    for t in plan.get("research_priority") or []:
        out.append((t, "the plan's research priority"))
    seen, uniq = set(), []
    for t, why in out:
        if t not in seen:
            seen.add(t)
            uniq.append((t, why))
    return uniq


def _research(c):
    queue = _research_queue(c.plan, c.date)
    if not queue:
        return
    k = known(c.snap, "research")
    if k == "known":
        slots = g(c.snap, "research", "slots", default=[]) or []
        running = {s.get("researching") for s in slots if s.get("researching")}
        done = set(g(c.snap, "research", "done", default=[]) or [])
        idle = [s.get("slot") for s in slots if not s.get("researching")]
        for slot in idle:
            for tech, why in queue:
                if tech in running or tech in done or not c.fresh("research:" + tech, "state"):
                    continue
                if c.act("research", "hoi4_research_start", {"tech": tech, "slot": slot},
                         f"{why}; slot {slot} is idle", "state", "research:" + tech):
                    running.add(tech)
                    break
        return
    if not c.paced("research"):
        return
    n = 0
    for tech, why in queue:
        if n >= 2:
            break
        if c.act("research", "hoi4_research_start", {"tech": tech}, why, "plan", "research:" + tech):
            n += 1


def _focus(c):
    path = [f for f in (c.plan.get("focus_path") or []) + (c.plan.get("focus_fallback") or []) if f]
    if not path:
        return
    focus = g(c.snap, "research", "focus")
    done = set(g(c.snap, "research", "focuses_done", default=[]) or c.snap.get("focuses_done") or [])
    if known(c.snap, "focus") == "known" and focus not in (None, "", "none"):
        return
    basis = "state" if known(c.snap, "focus") == "known" else "plan"
    if basis == "plan" and not c.paced("focus"):
        return
    for f in path:
        fid = f if isinstance(f, str) else _id(f, "focus")
        if fid in done or c.since("focus:" + fid) is not None:
            continue
        why = _why(f, "next on the plan's focus path") if not isinstance(f, str) else "next on the plan's focus path"
        c.act("focus", "hoi4_focus_start", {"focus": fid}, why, basis, "focus:" + fid)
        return


def _politics(c):
    picks = [p for p in c.plan.get("politics") or [] if isinstance(p, dict)]
    if not picks:
        return
    pp = num(g(c.snap, "politics", "political_power"))
    laws = g(c.snap, "politics", "laws", default={}) or {}
    have = set(str(v) for v in laws.values()) | set(g(c.snap, "politics", "advisors", default=[]) or [])
    for i, p in enumerate(picks):
        key = "politics:" + _id(p, "idea")
        if p.get("idea") in have or c.since(key) is not None:
            continue
        ok, unknown = judge(p.get("when"), c.snap)
        if ok is None:
            c.wait("politics", key, unknown)
            return
        if not ok:
            continue
        cost = num(p.get("cost"), 0)
        if pp is not None and pp < cost:
            return   # the picks go in order: the next waits for its power
        basis = "state" if pp is not None else "plan"
        if basis == "plan" and not c.paced("politics"):
            return
        c.act("politics", "hoi4_politics_choose",
              {"category": p.get("category"), "slot": p.get("slot"), "idea": p.get("idea"), "cost": cost},
              _why(p, f"the plan's {p.get('category') or 'political'} pick"), basis, key)
        return


def _family(equipment):
    """'infantry_equipment_1' -> 'infantry_equipment' (the archetype a line's equipment belongs to)."""
    e = str(equipment or "")
    parts = e.rsplit("_", 1)
    return parts[0] if len(parts) == 2 and parts[1].isdigit() else e


def _production(c):
    rows = [p for p in c.plan.get("production") or [] if isinstance(p, dict) and p.get("equipment")]
    k = known(c.snap, "production")
    if k == "known":
        lines = g(c.snap, "production", "lines", default=[]) or []
        by_family = {}
        for ln in lines:
            by_family[_family(ln.get("equipment"))] = by_family.get(_family(ln.get("equipment")), 0) + (ln.get("factories") or 0)
        for p in rows:
            fam = _family(p["equipment"])
            want = num(p.get("factories"), 1)
            have = by_family.get(fam)
            key = f"production:{fam}"
            if have is None:
                c.act("production", "hoi4_production_add", {"equipment": p["equipment"], "factories": want},
                      _why(p, f"the plan keeps a {fam} line"), "state", key)
            elif have < want:
                c.act("production", "hoi4_production_set", {"equipment": p["equipment"], "factories": want},
                      _why(p, f"the plan's {fam} line has {have:g} of {want:g} factories"), "state", key)
        for s in g(c.snap, "production", "stockpile", default=[]) or []:
            if s.get("depleting") or (s.get("fielded_short") or 0) > 0:
                fam = s.get("archetype")
                c.act("stockpile", "hoi4_production_priority", {"equipment": fam},
                      f"{fam} is short in the field or falling", "state", "stockpile:" + str(fam))
        return
    for p in rows:
        fam = _family(p["equipment"])
        c.act("production", "hoi4_production_add", {"equipment": p["equipment"], "factories": num(p.get("factories"), 1)},
              _why(p, f"the plan keeps a {fam} line"), "plan", f"production:{fam}")
    if c.plan.get("production_priority"):
        c.act("production", "hoi4_production_priority", {"order": list(c.plan["production_priority"])},
              "the plan's equipment priority", "plan", "production:priority")
    c.no_eyes("stockpile", ["production"], "stockpile and field shortages are read from the production block")


def _construction(c):
    k = known(c.snap, "construction")
    queued = {(r.get("building"), r.get("state")) for r in g(c.snap, "construction", "queue", default=[]) or []}
    for item in sorted((i for i in c.plan.get("construction_plan") or [] if isinstance(i, dict)),
                       key=lambda i: i.get("order") or 0):
        key = "construction:" + _id(item, "building")
        if c.since(key) is not None:
            continue
        ok, unknown = judge(item.get("when"), c.snap)
        if ok is None:
            c.wait("construction", key, unknown)
            continue
        if not ok:
            continue
        building = item.get("building")
        state = item.get("state") or item.get("to_state")
        if (building, state) in queued:
            continue
        tool = "hoi4_railway_build" if building in ("rail_way", "railway") else "hoi4_construction_order"
        args = {"building": building, "state": state}
        if tool == "hoi4_railway_build":
            args = {"from_state": item.get("from_state"), "to_state": item.get("to_state"), "path": item.get("path")}
        c.act("construction", tool, args, _why(item, f"the plan's {building}"),
              "state" if k == "known" else "plan", key)
    idle = num(g(c.snap, "economy", "civilian_idle"))
    min_idle = num(c.plan.get("construction_min_idle"), 1)
    fillers = [i for i in c.plan.get("construction") or [] if isinstance(i, dict)]
    if not fillers:
        return
    if known(c.snap, "economy") == "known" and idle is not None:
        if idle >= min_idle:
            f = fillers[0]
            c.act("construction", "hoi4_construction_add",
                  {"building": f.get("building"), "count": f.get("count", 1), "states": f.get("states") or []},
                  _why(f, f"{idle:g} civilian factories idle"), "state", "construction:fill:" + str(f.get("building")))
        return
    for f in fillers:
        c.act("construction", "hoi4_construction_add",
              {"building": f.get("building"), "count": f.get("count", 1), "states": f.get("states") or []},
              _why(f, f"the plan's standing {f.get('building')} work"), "plan",
              "construction:fill:" + str(f.get("building")))


def _decisions(c):
    for d in c.plan.get("decisions") or []:
        name = d if isinstance(d, str) else _id(d, "decision")
        why = "the plan takes this decision" if isinstance(d, str) else _why(d, "the plan takes this decision")
        when = None if isinstance(d, str) else d.get("when")
        ok, unknown = judge(when, c.snap)
        if ok is None:
            c.wait("decisions", "decision:" + name, unknown)
            continue
        if ok:
            c.act("decisions", "hoi4_decision_take", {"decision": name}, why, "plan", "decision:" + name)


def _extras(c):
    docs = [d for d in c.plan.get("doctrines") or [] if isinstance(d, dict)]
    if docs and c.paced("extras"):
        xp = g(c.snap, "doctrines", "experience", default={}) or {}
        for d in docs:
            key = "doctrine:" + _id(d, "subdoctrine")
            if c.since(key) is not None:
                continue
            have = num(xp.get(d.get("folder") or "")) if xp else None
            if have is not None and have < num(d.get("xp"), 0):
                break
            c.act("extras", "hoi4_doctrine_adopt",
                  {"folder": d.get("folder"), "track": d.get("track"), "subdoctrine": d.get("subdoctrine")},
                  _why(d, f"the plan's {d.get('folder')} doctrine"), "state" if have is not None else "plan", key)
            break
    sp = c.plan.get("special_projects") or {}
    for proj in (sp.get("priority") or []) if isinstance(sp, dict) else []:
        c.act("extras", "hoi4_special_project_assign", {"project": proj}, "the plan's special project",
              "plan", "special_project:" + str(proj))
        break


def _training(c):
    tr = c.plan.get("training")
    rows = tr if isinstance(tr, list) else ([tr] if isinstance(tr, dict) else [])
    if not rows or not c.paced("training"):
        return
    mp = num(g(c.snap, "politics", "manpower"))
    for t in rows:
        if mp is not None and mp < 20000:
            c.wait("training", "training:" + str(t.get("template")), ["manpower"])
            return
        c.act("training", "hoi4_train", {"template": t.get("template"), "count": t.get("batch") or t.get("queue") or 1,
                                          "deploy_state": t.get("deploy_state")},
              _why(t, f"the plan trains {t.get('template')}"), "state" if mp is not None else "plan",
              "training:" + str(t.get("template")) + ":" + c.date)


def _templates(c):
    for t in c.plan.get("templates") or []:
        if isinstance(t, dict):
            c.act("templates", "hoi4_template_create", {"name": t.get("name"), "regiments": t.get("regiments"),
                                                         "support": t.get("support")},
                  _why(t, f"the plan's {t.get('name')} template"), "plan", "template:" + _id(t, "name"))


def _air(c):
    for a in c.plan.get("air_plan") or []:
        if not isinstance(a, dict):
            continue
        key = f"air:{a.get('base_state')}:{a.get('mission')}"
        region = a.get("region")
        if a.get("switch_when") and a.get("then_region"):
            ok, _ = judge(a["switch_when"], c.snap)
            if ok:
                region, key = a["then_region"], key + ":then"
        c.act("air_plan", "hoi4_air_mission", {"base_state": a.get("base_state"), "mission": a.get("mission"),
                                                "region": region, "count": a.get("count")},
              _why(a, f"{a.get('mission')} over region {region}"), "plan", key)
    if known(c.snap, "air") != "known":
        c.no_eyes("air_idle", ["air"], "idle wings are found in the air block")


def _navy(c):
    nv = c.plan.get("navy")
    for f in (nv.get("fleets") if isinstance(nv, dict) else nv) or []:
        if isinstance(f, dict):
            c.act("navy", "hoi4_fleet_mission", {"fleet": f.get("fleet") or f.get("name"), "mission": f.get("mission"),
                                                  "regions": f.get("regions")},
                  _why(f, f"the plan's {f.get('mission')} fleet"), "plan", "fleet:" + _id(f, "fleet", "name"))


def _armies(c):
    for a in c.plan.get("army_layout") or []:
        if isinstance(a, dict):
            c.act("command", "hoi4_army_create", {"name": a.get("name_hint"), "state": a.get("state"),
                                                   "count": a.get("count"), "general": a.get("general"),
                                                   "role": a.get("role")},
                  _why(a, f"{a.get('role') or 'an'} army of {a.get('count')} at state {a.get('state')}"), "plan",
                  "army:" + _id(a, "name_hint"))
    for grp in c.plan.get("army_group") or []:
        if isinstance(grp, dict):
            c.act("army_groups", "hoi4_army_group_create", {"name": grp.get("name"), "armies": grp.get("armies")},
                  _why(grp, f"army group {grp.get('name')}"), "plan", "group:" + _id(grp, "name"))
    if known(c.snap, "land") != "known":
        c.no_eyes("armies", ["land"], "unassigned divisions and army sizes are read from the land block")


def _war(c, phase):
    war = c.plan.get("war")
    if not isinstance(war, dict) or war.get("enabled") is False:
        return False
    won = set(g(c.snap, "wars", "won", default=[]) or [])
    enemies = [e for e in war.get("enemies") or [] if e and e not in won]   # a beaten enemy is no longer planned
    if not enemies:
        return False
    at_war = set(g(c.snap, "wars", "enemies", default=[]) or [])
    fighting = [e for e in enemies if e in at_war]
    reason = (phase or {}).get("goal") or war.get("why") or "the plan's war"
    target = fighting[0] if fighting else (enemies[0] if enemies else None)
    tag = "war:" + str((phase or {}).get("id") or "plan")
    attacked = False
    if fighting:
        for i, axis in enumerate(war.get("axes") or []):
            states = axis if isinstance(axis, list) else (axis or {}).get("states") or []
            c.act("war", "hoi4_army_offensive", {"target_states": states},
                  reason, "plan", f"{tag}:axis:{i}",
                  directive={"kind": "attack", "target": target, "states": states, "priority": 3 if i == 0 else 2})
            attacked = True
        nxt = None
        held = set(g(c.snap, "wars", "states_controlled", default=[]) or [])
        for v in sorted((x for x in c.plan.get("vp_targets") or [] if isinstance(x, dict)),
                        key=lambda x: x.get("order") or 0):
            if v.get("state") not in held:
                nxt = v
                break
        if nxt is not None:
            c.act("war", "hoi4_army_attack", {"province": nxt.get("province"), "army": nxt.get("army")},
                  f"next victory point: {nxt.get('name')} (by {nxt.get('by')}); {reason}", "plan",
                  f"vp:{nxt.get('province')}",
                  directive={"kind": "attack", "target": target, "states": [nxt.get("state")], "priority": 3})
            attacked = True
    guards = war.get("guard_states") or []
    if guards:
        c.act("hold", "hoi4_army_front", {"states": guards, "kind": "hold"},
              f"hold {', '.join(str(s) for s in guards)}; {reason}", "plan", f"{tag}:hold",
              directive={"kind": "hold", "target": target, "states": guards, "priority": 2})
    for h in c.plan.get("hold_targets") or []:
        if isinstance(h, dict) and (not h.get("until") or _on_or_after(c.date, h["until"]) is False):
            c.act("hold", "hoi4_army_front", {"army": h.get("army"), "provinces": h.get("provinces"), "kind": "hold"},
                  _why(h, "the plan holds here"), "plan", "hold:" + _id(h),
                  directive={"kind": "hold", "target": target, "states": [], "priority": 3})
    if enemies and not fighting and war.get("prepare", True):
        c.act("war", "hoi4_army_plan", {"target": target, "theatre": war.get("theatre") or []},
              reason, "plan", f"{tag}:prepare:{target}",
              directive={"kind": "prepare", "target": target, "states": war.get("theatre") or [], "priority": 2})
    return attacked


def _diplomacy(c):
    dip = c.plan.get("diplomacy")
    rows = dip if isinstance(dip, list) else (dip.get("actions") if isinstance(dip, dict) else []) or []
    for d in rows:
        if not isinstance(d, dict):
            continue
        key = f"diplomacy:{d.get('action')}:{d.get('target')}"
        ok, unknown = judge(d.get("when"), c.snap)
        if ok is None:
            c.wait("diplomacy", key, unknown)
            continue
        if ok:
            kind = "declare_war" if d.get("action") in ("declare_war", "declare") else None
            c.act("diplomacy", "hoi4_diplomacy_act", {"action": d.get("action"), "target": d.get("target")},
                  _why(d, f"{d.get('action')} toward {d.get('target')}"), "plan", key,
                  directive={"kind": kind, "target": d.get("target"), "states": [], "priority": 3} if kind else None)
    fac = c.plan.get("faction")
    if isinstance(fac, dict) and (fac.get("create") or fac.get("join") or fac.get("invite")):
        if fac.get("create"):
            c.act("faction", "hoi4_faction_create", {"name": fac.get("name")}, _why(fac, "the plan founds a faction"),
                  "plan", "faction:create")
        for t in fac.get("invite") or []:
            c.act("faction", "hoi4_diplomacy_act", {"action": "invite_to_faction", "target": t},
                  _why(fac, f"invite {t} into the faction"), "plan", "faction:invite:" + t)
        if fac.get("join"):
            c.act("faction", "hoi4_diplomacy_act", {"action": "join_faction", "target": fac["join"]},
                  _why(fac, f"join {fac['join']}'s faction"), "plan", "faction:join")


def _trade(c):
    if known(c.snap, "economy") != "known":
        c.no_eyes("resources", ["economy"], "resource deficits are read from the economy block")
        return
    for name, r in (g(c.snap, "economy", "resources", default={}) or {}).items():
        if isinstance(r, dict) and r.get("short"):
            c.act("resources", "hoi4_trade_set", {"resource": name, "amount": abs(num(r.get("balance"), 0) or 0)},
                  f"{name} is short ({r.get('balance')} a day)", "state", "trade:" + name)


def _occupation(c):
    oc = c.plan.get("occupation")
    law = oc.get("law") if isinstance(oc, dict) else (oc if isinstance(oc, str) else None)
    if law and g(c.snap, "occupation", "states"):
        if g(c.snap, "occupation", "law") != law:
            c.act("occupation", "hoi4_occupation_law", {"law": law}, _why(oc, "the plan's occupation law"),
                  "state", "occupation:" + law)


def _intelligence(c):
    it = c.plan.get("intelligence")
    if isinstance(it, dict) and it.get("agency") and not g(c.snap, "intelligence", "agency"):
        c.act("intelligence", "hoi4_agency_create", {}, _why(it, "the plan founds an agency"), "plan",
              "intelligence:agency")


def _being_attacked(snap):
    lost = g(snap, "wars", "own_states_lost", default=[]) or []
    threatened = [t for t in g(snap, "wars", "threatened", default=[]) or [] if not t.get("defended")]
    return bool(lost), lost, threatened


# --- the call ------------------------------------------------------------------


def decide(plan, snap, memory=None, prev=None):
    """What the executor's lanes would do now for `snap`'s country under `plan`; nothing is sent anywhere."""
    memory = json.loads(json.dumps(memory or {}))
    phase = phase_in_force(plan, snap, memory)
    c = _Ctx(merged(plan, phase), snap, memory)
    if plan.get("active") is False:
        return _answer(c, plan, phase, False, snap, prev)
    _events(c)
    _research(c)
    _focus(c)
    attacked = _war(c, phase)
    _production(c)
    _navy(c)
    _construction(c)
    _politics(c)
    _extras(c)
    _decisions(c)
    _training(c)
    _templates(c)
    _air(c)
    _intelligence(c)
    _occupation(c)
    _trade(c)
    _diplomacy(c)
    _armies(c)
    order = {n: i for i, (n, _, _) in enumerate(LANES)}
    c.actions.sort(key=lambda a: order.get(a["lane"], -1))
    return _answer(c, plan, phase, attacked, snap, prev)


def _goal(plan):
    for k in ("goal", "win_condition", "surrender_goal"):
        v = plan.get(k)
        if isinstance(v, str) and v:
            return v
        if isinstance(v, dict):
            return v.get("text") or v.get("why") or v.get("needs") or json.dumps(v, sort_keys=True)[:200]
    return None


def workload(actions, snap, attacked):
    hit, lost, threatened = _being_attacked(snap)
    if attacked:
        return {"level": "fighting", "why": "an attack is ordered on a country we are at war with",
                "lanes": sorted({a["lane"] for a in actions})}
    if hit or threatened:
        what = (f"{len(lost)} own state(s) lost" if hit else "") + \
               ((" and " if hit and threatened else "") + f"{len(threatened)} undefended state(s) threatened"
                if threatened else "")
        return {"level": "fighting", "why": "being attacked: " + what, "lanes": sorted({a["lane"] for a in actions})}
    heavy = sorted({a["lane"] for a in actions if a["weight"] == "heavy"})
    if heavy:
        return {"level": "heavy", "why": "heavy work is due: " + ", ".join(heavy), "lanes": heavy}
    minor = sorted({a["lane"] for a in actions})
    if minor:
        return {"level": "minor", "why": "only one-click work is due: " + ", ".join(minor), "lanes": minor}
    return {"level": "idle", "why": "nothing is due", "lanes": []}


def _answer(c, plan, phase, attacked, snap, prev):
    try:
        from ..observe.findings import findings as _findings
        found = [{k: f.get(k) for k in ("code", "severity", "text", "subject")} for f in _findings(snap, prev or [])]
    except Exception as exc:  # a partial snapshot must not cost the decisions
        found = [{"code": "findings_unavailable", "severity": "low", "text": f"{type(exc).__name__}: {exc}"[:200],
                  "subject": None}]
    return {
        "schema": SCHEMA,
        "mode": "decide_only",
        "date": snap.get("date"),
        "country": snap.get("country") or snap.get("player"),
        "goal": _goal(plan),
        "phase": {"id": phase.get("id"), "goal": phase.get("goal")} if phase else None,
        "actions": c.actions,
        "waiting": c.waiting,
        "blind": c.blind,
        "workload": workload(c.actions, snap, attacked),
        "findings": found,
        "memory": c.memory,
    }


def main(argv=None):
    ap = argparse.ArgumentParser(prog="python -m hoi4_live.play.decide",
                                 description="The executor's lanes on a supplied snapshot: the actions as data.")
    ap.add_argument("--plan", required=True)
    ap.add_argument("--snapshot", required=True)
    ap.add_argument("--memory", help="the memory of the last call (read, then replaced with the new one)")
    ap.add_argument("--out", help="write the answer here instead of stdout")
    a = ap.parse_args(argv)
    with open(a.plan, encoding="utf-8") as f:
        plan = json.load(f)
    with open(a.snapshot, encoding="utf-8") as f:
        snap = json.load(f)
    memory = {}
    if a.memory:
        try:
            with open(a.memory, encoding="utf-8") as f:
                memory = json.load(f)
        except (OSError, ValueError):
            memory = {}
    out = decide(plan, snap, memory)
    if a.memory:
        with open(a.memory, "w", encoding="utf-8") as f:
            json.dump(out["memory"], f)
    text = json.dumps(out, indent=1, sort_keys=False)
    if a.out:
        with open(a.out, "w", encoding="utf-8") as f:
            f.write(text)
    else:
        sys.stdout.write(text + "\n")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
