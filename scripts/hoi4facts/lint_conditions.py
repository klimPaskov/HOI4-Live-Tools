"""Planned items against the game's own conditions (rules focus.condition, focus.condition_unchecked,
decision.condition, decision.condition_unchecked, politics.visible, politics.exclusive, politics.dual_role,
diplomacy.borders_unheld, plan.stop_date), run by lint_plan.py (Lint.run) on every plan.

Each planned item is read against the game files (conditions.py) under the plan's own timeline: the land
its checkpoints expect it to hold, the first war it dates, its start government and ideas, and the army its
training lines can field by each day.

  focus.condition            (error) a focus the focus slot walk starts on a date when its `available` gate
                             cannot hold under the plan: an army gate (has_army_manpower, an army-strength
                             ratio over another country) turned into a number of divisions or manpower that the
                             plan's training does not reach by then, or a line the plan's own timeline makes
                             false. The message gives the target and the first day the plan reaches it.
  focus.condition_unchecked  (error on focus_path, warning on focus_fallback) a gate line the plan cannot
                             decide offline (another country's state, a flag, stability, factories, ...) with
                             no dated checkpoint for it: a checkpoint whose `must` names the focus
                             (focus_completed / focus_done) or, for a quantity, an atom on that quantity
                             (stability, war_support, factories, ...) dated no later than the focus.
  decision.condition         (error) a planned decision whose own or category gate is false on its date under
                             the plan.
  decision.condition_unchecked (warning) a planned decision whose gate lines the plan cannot decide: the
                             executor waits on them; the plan should say what makes them true.
  politics.visible           (error) an advisor or law the plan names whose `visible` or `available` gate is
                             closed on every day of the run (a `visible = { always = no }` role is never shown).
  politics.exclusive         (warning) an advisor that needs another planned character not hired
                             (`<character> = { is_hired_as_advisor = no }`): the order of the two decides.
  politics.dual_role         (warning) a character the plan both commands with (general or marshal) and hires
                             as an advisor; the game files give him both roles, so plan which one he holds.
  diplomacy.borders_unheld   (error) a declaration whose own border states with the target (and, without
                             accept_widening, with the joiners it accepts), on its date under the plan,
                             include states no army, task or guard list covers: the executor's war-risk read
                             reports them as borders_not_held and the declaration waits forever, or with
                             accept_widening the war opens on an empty border.
  plan.stop_date             (error) stop_on_date before the run's horizon while stop_on_date_pause is not false:
                             the run pauses itself there. The detail horizon belongs in _validator.stop_date.
"""
import re

from . import conditions as cond
from . import derive

CHECKPOINT_SLACK = 31   # a checkpoint on a quantity may be dated this many days after the focus starts


def _day(s):
    return derive.day(s) if s else None


def _focus_tree(lint):
    trees = lint.game.focus_trees()
    path = [x for x in lint.plan.get("focus_path") or [] if isinstance(x, str)]
    tid = lint.game.loaded_tree(lint.ctx) if lint.ctx else None
    if not tid:
        tid = max(trees, key=lambda t: sum(1 for k in path if k in trees[t]), default=None)
    return trees.get(tid, {})


def _checkpoint_atoms(lint):
    """[(day, atom)] of the dated checkpoints."""
    out = []
    for c in lint.list_of("checkpoints"):
        d = _day(c.get("by"))
        if d is None:
            continue
        for a in cond._atoms(c.get("must")):
            out.append((d, a.strip()))
    for h in lint.list_of("horizons"):
        d = _day(h.get("until"))
        t = h.get("targets") if isinstance(h.get("targets"), dict) else {}
        for k in ("civilian_factories", "military_factories"):
            if d is not None and isinstance(t.get(k), (int, float)):
                out.append((d, f"{k} >= {t[k]}"))
    return out


def _covered(line, focus, start, atoms):
    """A dated checkpoint covers the line: it names the focus, or checks the line's quantity by the focus."""
    for d, a in atoms:
        if re.match(rf"^focus_(?:completed|done):{re.escape(focus)}$", a):
            return True
    names = cond.ATOM_OF_KIND.get(line.get("kind"), ())
    for d, a in atoms:
        head = re.split(r"[\s:<>=!]", a, 1)[0]
        if head in names and d <= start + CHECKPOINT_SLACK:
            if head in ("civilian_factories", "military_factories") and isinstance(line.get("value"), (int, float)):
                m = re.search(r">=?\s*(\d+(?:\.\d+)?)\s*$", a)
                need = line["value"] + (1 if line.get("cmp") == ">" else 0)
                if not m or float(m.group(1)) < need:
                    continue
            return True
    return False


def _flatten(lines):
    for x in lines:
        if x.get("parts"):
            yield from _flatten(x["parts"])
        else:
            yield x


def check(lint):
    if not lint.tag or lint.world is None:
        return lint
    tl = cond.Timeline(lint)
    lint._timeline = tl
    focus(lint, tl)
    decisions(lint, tl)
    politics(lint, tl)
    borders(lint, tl)
    stop_date(lint)
    rows = tl.forecast()["rows"]
    lint.facts["army_forecast"] = {
        "start_divisions": tl.forecast()["start"],
        "by_date": [{"date": derive.fmt(derive.date_of(d)), "divisions": round(n), "manpower": round(m)}
                    for d, n, m in rows[::13]],
        "note": "the executor's training queues and caps, each division started only with its equipment in "
                "stock (start stockpile less the start shortfall, plus the planned lines' output)"}
    return lint


# ---------------------------------------------------------------------------------------------- focuses
def focus(lint, tl):
    sched = (lint.facts.get("focus") or {}).get("schedule") or []
    if not sched:
        return
    fmeta = derive.focus_meta(lint.game)
    atoms = _checkpoint_atoms(lint)
    done = set(lint.meta.get("completed_focuses", []) or [])
    rows = sorted(({"focus": r["focus"], "start": _day(r["start"]), "end": _day(r["end"]), "from": r["from"]}
                   for r in sched), key=lambda r: r["start"])
    out = []
    for r in rows:
        gate = (fmeta.get(r["focus"]) or {}).get("available")
        before = done | {x["focus"] for x in rows if x["end"] <= r["start"]}
        if gate:
            res, lines = cond.evaluate(gate, cond.Env(tl, r["start"], before))
            where = f"focus_{r['from']}"
            when = derive.fmt(derive.date_of(r["start"]))
            row = {"focus": r["focus"], "start": when, "result": res,
                   "lines": [cond.describe(x) for x in _flatten(lines)]}
            out.append(row)
            if res is False:
                army = [x for x in _flatten(lines) if x.get("kind") in ("army_ratio", "army_manpower")]
                text = "; ".join(cond.describe(x) for x in (army or list(_flatten(lines))))
                lint.add("error", "focus.condition", where,
                         f"{r['focus']} is scheduled from {when} but its `available` gate cannot hold then under the "
                         f"plan: {text}. Date it when the plan meets the gate, or plan what meets it by then "
                         f"(training lines and caps for an army gate) and add a dated checkpoint on it")
            elif res is None:
                open_lines = [x for x in _flatten(lines) if not _covered(x, r["focus"], r["start"], atoms)]
                if open_lines:
                    sev = "error" if r["from"] == "path" else "warning"
                    kinds = sorted({x["kind"] for x in open_lines})
                    lint.add(sev, "focus.condition_unchecked", where,
                             f"{r['focus']} (scheduled {when}) waits on gate lines the plan cannot decide offline: "
                             f"{'; '.join(cond.describe(x) for x in open_lines[:4])}"
                             + (" ..." if len(open_lines) > 4 else "")
                             + f" ({', '.join(kinds)}). Add a dated checkpoint for it: `focus_completed:{r['focus']}` "
                             "by the date the plan counts on, or an atom on the same quantity (stability, war_support, "
                             "factories, ...) by the focus's start, so a miss is seen and revised, not waited on")
        done_now = before | {r["focus"]}
        del done_now
    lint.facts["focus_conditions"] = out


# ---------------------------------------------------------------------------------------------- decisions
def decisions(lint, tl):
    w = lint.world
    index = {}
    for d in w.decision_index():
        index.setdefault(d["id"], d)
    cats = w.decision_categories()
    done_rows = [(_day(r["end"]), r["focus"]) for r in (lint.facts.get("focus") or {}).get("schedule") or []]
    done0 = set(lint.meta.get("completed_focuses", []) or [])
    wanted = []
    for i, d in enumerate(lint.plan.get("decisions") or []):
        key = d if isinstance(d, str) else (d.get("decision") or d.get("id") if isinstance(d, dict) else None)
        if key:
            wanted.append((f"decisions[{i}]", key, tl.start))
    for i, r in enumerate(lint.list_of("decision_rules")):
        if r.get("decision") and not r.get("never"):
            ds = tl.lint_dates(r["when"]) if r.get("when") is not None else []
            wanted.append((f"decision_rules[{i}]", r["decision"], max(ds) if ds else tl.start))
    end = max([d for d, _ in lint.horizon()] or [tl.start + 365])
    for where, key, day in wanted:
        d = index.get(key)
        if d is None:
            continue
        cat = cats.get(d["category"]) or {}
        blocks = [(cat.get("visible"), f"category {d['category']} visible"),
                  (cat.get("allowed"), f"category {d['category']} allowed"),
                  (derive.get1(d["block"], "visible"), "visible"),
                  (derive.get1(d["block"], "allowed"), "allowed"),
                  (derive.get1(d["block"], "available"), "available")]
        # the gate is judged on the decision's date and, when closed then, on later weeks up to the horizon
        closed, unknown, opened = [], [], None
        for t in list(range(day, end, 28)) + [end]:
            done = done0 | {f for e, f in done_rows if e is not None and e <= t}
            closed_t, unknown_t = [], []
            for block, whose in blocks:
                if not isinstance(block, list) or not block:
                    continue
                res, lines = cond.evaluate(block, cond.Env(tl, t, done))
                if res is False:
                    closed_t.append((whose, lines))
                elif res is None:
                    unknown_t += [(whose, x) for x in _flatten(lines)]
            if not closed_t:
                opened, unknown = t, unknown_t
                break
            if t == day:
                closed = closed_t
        if opened is None:
            text = "; ".join(f"{whose}: " + "; ".join(cond.describe(x) or x.get("text", "") for x in lines[:2])
                             for whose, lines in closed[:3])
            # decision.unreachable already reports a focus no list takes: only add what it does not cover
            lint.add("error", "decision.condition", where,
                     f"{key} stays closed from {derive.fmt(derive.date_of(day))} to the horizon under the plan "
                     f"({text}): the executor waits on it forever; plan what opens it or drop it")
        elif opened > day + 60:
            lint.add("warning", "decision.condition", where,
                     f"{key} is planned for {derive.fmt(derive.date_of(day))} but its gate opens under the plan only "
                     f"about {derive.fmt(derive.date_of(opened))}: date it then, or plan what opens it earlier")
        if opened is not None and unknown:
            lint.add("warning", "decision.condition_unchecked", where,
                     f"{key} also waits on lines the plan cannot decide offline: "
                     + "; ".join(f"{whose}: {cond.describe(x)}" for whose, x in unknown[:3])
                     + (" ..." if len(unknown) > 3 else "")
                     + ". Say in the entry's why what makes them true, or add a dated checkpoint on them")


# ---------------------------------------------------------------------------------------------- politics
def _planned_ideas(lint):
    """[(where, idea, first day)] of every advisor or law the plan names."""
    out = []
    start = _day(lint.start)
    for i, p in enumerate(lint.list_of("politics")):
        if isinstance(p.get("idea"), str):
            ds = predicate_days(lint, p.get("when"))
            out.append((f"politics[{i}]", p["idea"], max(ds) if ds else start))
    auto = lint.plan.get("politics_auto")
    if isinstance(auto, dict) and auto.get("enabled") is not False:
        for j, e in enumerate(auto.get("order") or []):
            if isinstance(e, dict):
                for x in e.get("ideas") or []:
                    if isinstance(x, str):
                        out.append((f"politics_auto.order[{j}]", x, start))
    return out


def predicate_days(lint, pred):
    if pred is None:
        return []
    from .validate import predicate_dates
    return [derive.day(d) for d in predicate_dates(pred, lint.table) if derive.day(d) is not None]


def _commanders(lint):
    """Display or id names of every general and marshal the plan assigns."""
    out = set()
    for e in lint.list_of("army_layout"):
        if isinstance(e.get("general"), str):
            out.add(e["general"])
    armies = lint.plan.get("armies") if isinstance(lint.plan.get("armies"), dict) else {}
    out.update(g for g in armies.get("generals") or [] if isinstance(g, str))
    for e in lint.list_of("army_group"):
        m = e.get("marshal")
        out.update(x for x in (m if isinstance(m, list) else [m]) if isinstance(x, str))
    for e in lint.list_of("armies"):
        if isinstance(e.get("general"), str):
            out.add(e["general"])
    return out


def _norm(s):
    return re.sub(r"[^a-z]", "", derive._norm(s) if hasattr(derive, "_norm") else s.lower())


# the advisor slots of each politics category (common/idea_tags)
CATEGORY_SLOTS = {"government": {"political_advisor"},
                  "military_staff": {"army_chief", "navy_chief", "air_chief", "high_command"},
                  "research_production": {"theorist"}}


def _entry(lint, where):
    """The politics or politics_auto.order entry a `where` label names."""
    m = re.match(r"^(politics|politics_auto\.order)\[(\d+)\]", where or "")
    if not m:
        return None
    rows = lint.list_of("politics") if m.group(1) == "politics" else \
        (lint.plan.get("politics_auto") or {}).get("order") or []
    i = int(m.group(2))
    return rows[i] if i < len(rows) and isinstance(rows[i], dict) else None


def politics(lint, tl):
    w = lint.world
    planned = _planned_ideas(lint)
    if not planned:
        return
    end = max([d for d, _ in lint.horizon()] or [tl.start + 365])
    done_rows = [(_day(r["end"]), r["focus"]) for r in (lint.facts.get("focus") or {}).get("schedule") or []]
    done0 = set(lint.meta.get("completed_focuses", []) or [])
    chars = {}
    for where, idea, first in planned:
        roles = cond.advisor_conditions(w, lint.tag, idea)
        # a character with several advisor roles is judged by the role of the category the plan picks it for
        cat = (_entry(lint, where) or {}).get("category")
        fit = [r for r in roles if r.get("slot") in CATEGORY_SLOTS.get(cat, ())]
        roles = fit or roles
        for r in roles:
            chars.setdefault(r["character"], []).append((where, idea, r))
        if not roles:
            continue
        # every role the name stands for is closed on every sampled day: never pickable
        never = []
        for r in roles:
            why = None
            for t in list(range(first, end, 28)) + [end]:
                done = done0 | {f for e, f in done_rows if e is not None and e <= t}
                env = cond.Env(tl, t, done)
                bad = []
                for name in ("visible", "available"):
                    block = r.get(name)
                    if isinstance(block, list) and block:
                        res, lines = cond.evaluate(block, env)
                        if res is False:
                            bad.append(f"{name}: " + "; ".join(x.get("text", "") for x in list(_flatten(lines))[:2]))
                if not bad:
                    why = None
                    break
                why = bad
            if why:
                never.append((r, why))
        if never and len(never) == len(roles):
            r, why = never[0]
            alts = []
            if where.startswith("politics_auto.order["):
                j = int(where.split("[")[1].rstrip("]"))
                entry = (lint.plan.get("politics_auto") or {}).get("order")[j]
                alts = [x for x in entry.get("ideas") or [] if x != idea]
            sev = "warning" if alts else "error"
            lint.add(sev, "politics.visible", where,
                     f"{idea} ({r['character']}, {r['slot']}) can never be picked under the plan from "
                     f"{derive.fmt(derive.date_of(first))} to the horizon: {'; '.join(why)}. The game shows it "
                     "disabled or not at all, so the power meant for it stays idle: name another advisor for the seat"
                     + (f" (the entry falls back to {', '.join(alts)})" if alts else ""))
    # exclusions between planned characters
    planned_chars = set(chars)
    for cid, uses in chars.items():
        for where, idea, r in uses:
            for other in cond.hired_exclusions(r.get("available")):
                if other in planned_chars and other != cid:
                    lint.add("warning", "politics.exclusive", where,
                             f"{idea} can be hired only while {other} is not an advisor (its `available`), and the plan "
                             f"hires {other} too ({chars[other][0][0]}): hire {idea} first or drop one")
    # a commander the plan also hires as an advisor
    names = {_norm(n): n for n in _commanders(lint)}
    if names:
        for cid, uses in chars.items():
            display = w.name(cid) if hasattr(w, "name") else cid
            hit = names.get(_norm(display or "")) or names.get(_norm(cid))
            if hit:
                where, idea, r = uses[0]
                lint.add("warning", "politics.dual_role", where,
                         f"{idea} ({cid}) is also the plan's commander '{hit}': the game files give this character "
                         "both roles; plan which one he holds (an advisor seat the game shows disabled leaves its power "
                         "idle, a commander who leaves his army loses its bonuses). Unconfirmed offline: whether the "
                         "game disables the advisor while he commands; the live politics read says")


# ---------------------------------------------------------------------------------------------- borders
def _state_neighbours(lint):
    adj = lint.adjacency(straits=False)
    ps = {}
    for sid, row in lint.states().items():
        for p in row.get("provinces") or []:
            ps[p] = sid
    out = {}
    for p, ns in adj.items():
        a = ps.get(p)
        if a is None:
            continue
        for q in ns:
            b = ps.get(q)
            if b is not None and b != a:
                out.setdefault(a, set()).add(b)
    return out


def _covered_states(lint):
    out = set()
    for e in lint.list_of("army_layout"):
        for k in ("state",):
            if isinstance(e.get(k), int):
                out.add(e[k])
        for k in ("covers", "guard_states_in_order", "states"):
            out.update(x for x in e.get(k) or [] if isinstance(x, int))
    war = lint.plan.get("war") if isinstance(lint.plan.get("war"), dict) else {}
    out.update(x for x in war.get("guard_states") or [] if isinstance(x, int))
    for p in lint.list_of("phases"):
        ov = ((p.get("overrides") or {}).get("war") or {}) if isinstance(p.get("overrides"), dict) else {}
        out.update(x for x in ov.get("guard_states") or [] if isinstance(x, int))
    return out


def borders(lint, tl):
    items = [(i, e) for i, e in enumerate(lint.list_of("diplomacy"))
             if e.get("action") in ("declare", "declare_war") and isinstance(e.get("target"), str)]
    if not items:
        return
    owner0 = {}
    for sid, row in lint.states().items():
        if row.get("owner"):
            owner0[sid] = row["owner"]
    nb = _state_neighbours(lint)
    covered = _covered_states(lint)
    for i, e in items:
        # accept_widening lets the executor declare with weak borders against the joiners it accepts; the
        # target's own border is still the front the war opens on, so it stays checked
        widen = e.get("accept_widening") is True
        ds = predicate_days(lint, e.get("when"))
        day = max(ds) if ds else tl.start
        mine = tl.owned(day)
        theirs_tags = {e["target"]} | (set() if widen else {t for t in e.get("joiners_ok") or [] if isinstance(t, str)})

        def theirs(n):  # their land: the start owners minus what the plan expects to hold by then
            return owner0.get(n) in theirs_tags and n not in mine
        border = {s for s in mine if any(theirs(n) for n in nb.get(s, ()))}
        # land the plan gains is a partition's edge: a neighbour that is not ours may pass to the target by an
        # event (a state the target claims), so a gained state one foreign state away counts as a border too
        border |= {s for s in mine - tl.own_start
                   if any(n not in mine and any(theirs(m) for m in nb.get(n, ())) for n in nb.get(s, ()))}
        empty = sorted(s for s in border if s not in covered)
        if empty:
            gained = sorted(s for s in empty if s not in tl.own_start)
            lint.add("error", "diplomacy.borders_unheld", f"diplomacy[{i}]",
                     f"{e.get('id') or 'declaration'} on {e['target']} ({derive.fmt(derive.date_of(day))}): own border "
                     f"states with {', '.join(sorted(theirs_tags))} that no army, task or guard list covers: "
                     f"{', '.join(map(str, empty))}"
                     + (f" (gained under the plan by then: {', '.join(map(str, gained))})" if gained else "")
                     + (". accept_widening declares anyway, so the target walks into them on the first day; add "
                        "them to war.guard_states or an army's covers" if widen else
                        ". The executor's war-risk read counts them as borders_not_held (at least 2 divisions per "
                        "border state, none empty) and the declaration waits; add them to war.guard_states or an "
                        "army's covers, or set accept_widening with a reason (the target's border stays checked)"))


# ---------------------------------------------------------------------------------------------- stop date
def stop_date(lint):
    stop = lint.plan.get("stop_on_date")
    if not stop or lint.plan.get("stop_on_date_pause") is False:
        return
    ends = [d for d, label in lint.horizon() if label == "goal deadline"]
    hz = lint.plan.get("horizons")
    if isinstance(hz, list):
        ends += [_day(h.get("until")) for h in hz if isinstance(h, dict) and _day(h.get("until"))]
    last = max(ends) if ends else None
    d = _day(stop)
    if d is not None and last is not None and d < last:
        lint.add("error", "plan.stop_date", "stop_on_date",
                 f"stop_on_date {stop} is before the run's end {derive.fmt(derive.date_of(last))}: the executor "
                 "pauses the game there, and a no-pause run must not pause itself. The end of the detail horizon "
                 "belongs in _validator.stop_date (the rolling planner extends the plan); leave stop_on_date out, or "
                 "set stop_on_date_pause false for a run that only stops acting")
