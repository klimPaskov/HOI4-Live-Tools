"""Economy and production by arithmetic, run by lint_plan.py (Lint.run) after lint_conditions.

Production is a set of factory targets per equipment over all lines, the ones the country starts with
(its history's OOB files) and the new ones, in one priority order, sized to what the planned divisions need:

  production.start_lines     (error) a start line whose equipment no production entry names (kept with a
                             factory target, replaced or removed): its factories and its place in the
                             priority order are nobody's decision.
  production.duplicate_line  (error) two entries that add lines of the same equipment family, or a new tier
                             beside a running line without a replace or remove of the old one: one target per
                             equipment, a second line splits the factories and its efficiency starts again.
  production.priority_all    (error) production_priority does not order every military line (start and new):
                             a line left out keeps whatever place the game gave it.
  production.missing_line    (error) equipment the plan's training templates need that no line makes.
  production.need_cap        (warning) a training entry with no cap (its own or training_rules.cap): the
                             executor sizes army equipment lines by the planned force, training caps x each
                             template's equipment less stock and fielded, so an uncapped line orders nothing.
  production.need_balance    (error) from the start to the first war day (else the horizon), what the country
                             has of one family (start stockpile plus planned output) is far above what the
                             plan's army needs while another family sits far below (need: the start
                             divisions' shortfall, every division the training lines add within their caps,
                             5% a month of the army's equipment at war; output: factories x speed x
                             efficiency / unit cost, as production.balance counts it).
  production.parked_request  (warning) a line of 10+ factories whose family the plan makes far above the
                             need: new factories pile onto that line while others starve.
                             Give targets as shares of the factories that exist on each date, re-divided as
                             the factory count grows.
  construction.capacity      (error) the plan's own factory targets (horizons targets) need more factories
                             than the free slots of the states its construction list names, while
                             construction_fallback_any_own_state is not true: civilian factories stand idle
                             once the listed states are full.
  plan.ai_baseline           (error) where a baseline of the game's own AI exists for the country and start
                             (skills/hoi4-campaign-playbook/baselines/<TAG>-<year>.json), a plan checkpoint or horizon
                             target below BASELINE_SLACK of the AI's civilian, military or total factories on that
                             date, or the forecast of the plan's own levers below FORECAST_SLACK of them up to the first war day
                             (the forecast leaves out what conquest adds). A written
                             reason does not lift it: only goal.ai_baseline_constraint entries the lint confirms
                             (no_free_slots: the forecast finds no free slot outside the construction risk by then;
                             law_unavailable: the law is not offered or its gate is closed under the plan's timeline).
                             A civilian shortfall alone is a warning (plan.ai_baseline at "split"): on a date where
                             the forecast meets the AI's military factories and its total (civilian + military), an
                             earlier switch is the plan's choice of what the factories make, not a smaller economy;
                             goal.ai_baseline_why says why the split wins the war.
                             Divisions are reported (info), not held: a stated divisions target, or the army
                             forecast (the executor's training queues, caps and training time, each division
                             started only with its equipment in stock from the planned lines), below BASELINE_SLACK
                             of the AI's divisions. The army's floor is the force table (lint_force.py).
  economy.unbacked           (warning) a civilian or military target more than UNBACKED_SLACK above what the plan's
                             own levers give on that date (facts.economy.forecast).
  economy.below_forecast     (warning) a target below what the plan's own levers give: the plan aims low.
  construction.switch        (warning) the base construction list queues military factories from the first day while
                             the arithmetic switches later: the arms_factory job belongs in a phase from the switch.
  economy.forecast           (info) no forecast could be made (the reason).
facts.economy: factories, consumer goods at the start (ideas, and the dynamic modifiers the history adds,
resolved with the variables it sets), the scripts that change them later, free slots, `forecast` (economy.py: the
plan's own dated levers day by day with the plan's dated switch (else the one the arithmetic picks), by quarter and on the AI's dates) and
`levers` (laws, advisors, techs and focuses the game offers that the plan leaves out or takes late, with the
factories each adds by the first war day); economy.consumer_goods (info) names the scripts. facts.production_need:
need and what the country has per family. facts.ai_baseline: the marks below the AI, the forecast's, and each
constraint with whether the lint confirmed it.
"""
import json
import os
import re
from collections import Counter, defaultdict

from . import calc as calcmod
from . import derive
from . import economy as econmod

HERE = os.path.dirname(os.path.abspath(__file__))
BASELINES = os.path.join(os.path.dirname(os.path.dirname(HERE)), "skills", "hoi4-campaign-playbook", "baselines")
NEAR_ZERO = 0.6         # need_balance: a family with less than this share of its need is starved
FAR_ABOVE = 2.5         # ... while another family has more than this many times its need
BASELINE_SLACK = 1.0    # plan.ai_baseline: the AI is a floor, never a target (rules-and-modes.md): marks below it fail
FORECAST_SLACK = 0.85   # ... and the forecast of its own levers below 85% (the arithmetic leaves some levers out)
UNBACKED_SLACK = 1.1    # economy.unbacked: a target more than 10% above what the levers give
SWITCH_GRACE_DAYS = 180  # construction.switch: a switch within this of the start needs no phase
LOSS_SHARE = 0.05       # share of the fielded equipment lost a month at war (as production.balance)


def _day(s):
    return derive.day(s) if s else None


def check(lint):
    if not lint.tag or lint.world is None:
        return lint
    tl = getattr(lint, "_timeline", None)
    if tl is None:
        from . import conditions as cond
        tl = cond.Timeline(lint)
    structure(lint)
    need_balance(lint, tl)
    economy(lint, tl)
    return lint


# ---------------------------------------------------------------------------------------------- structure
def structure(lint):
    g = lint.game
    fam = lambda k: derive.equipment_family(g, k)  # noqa: E731
    entries = lint.list_of("production")
    named = set()
    for x in entries:
        for k in (x.get("equipment"), x.get("replace")):
            if isinstance(k, str):
                named.add(k)
    named_fams = {fam(k) for k in named}
    start = lint.start_lines()
    unnamed = defaultdict(list)
    for r in start:
        if r["type"] not in named and fam(r["type"]) not in named_fams:
            unnamed[r["type"]].append(r)
    for k, rs in unnamed.items():
        n = sum(int(r.get("factories") or 0) for r in rs)
        lint.add("error", "production.start_lines", "production",
                 f"the {str(len(rs)) + ' start lines' if len(rs) > 1 else 'start line'} of {k} ({n} factories, "
                 f"{rs[0].get('src')}"
                 f"{' ...' if len(rs) > 1 else ''}) {'are' if len(rs) > 1 else 'is'} named by no production entry: "
                 "give the equipment a factory target (an entry for it), a replace or a remove, and a place in "
                 "production_priority")
    # adds by family, with the dates the plan keeps or drops the other keys
    adds = defaultdict(list)
    for i, x in enumerate(entries):
        if isinstance(x.get("equipment"), str) and not x.get("remove") and "replace" not in x:
            adds[fam(x["equipment"])].append((i, x))
    dropped = {x["equipment"] for x in entries if isinstance(x.get("equipment"), str)
               and (x.get("remove") or "replace" in x)}
    start_keys = {r["type"] for r in start}
    for f, rows in adds.items():
        keys = Counter(x["equipment"] for _, x in rows)
        dup = [k for k, n in keys.items() if n > 1]
        for k in dup:
            idx = [i for i, x in rows if x["equipment"] == k]
            lint.add("error", "production.duplicate_line", f"production[{idx[1]}]",
                     f"{k} has {len(idx)} add entries (production{idx}): one factory target per equipment; a "
                     "later size goes in a phase override or a new date of the same target, never a second line")
        beside = [k for k in keys if k not in start_keys and any(fam(s) == f and s != k and s not in dropped
                                                                  for s in start_keys)]
        for k in beside:
            old = sorted(s for s in start_keys if fam(s) == f and s != k)
            i = next(i for i, x in rows if x["equipment"] == k)
            lint.add("error", "production.duplicate_line", f"production[{i}]",
                     f"{k} adds a second line beside the running {', '.join(old)} line: switch the old line with "
                     f"`replace` (the game keeps part of its efficiency) or remove it on a date, so one line "
                     "carries the family")
    # the executor produces army equipment for the force the training caps name
    rules = lint.plan.get("training_rules") if isinstance(lint.plan.get("training_rules"), dict) else {}
    tr = lint.plan.get("training")
    for i, e in enumerate(tr if isinstance(tr, list) else [tr] if isinstance(tr, dict) else []):
        if isinstance(e, dict) and not isinstance(e.get("cap"), int) and not (
                isinstance(rules.get("cap"), int) and rules.get("cap_template") in (None, e.get("template"))):
            lint.add("warning", "production.need_cap", f"training[{i}]" if isinstance(tr, list) else "training",
                     f"training of {e.get('template')} has no cap: the planned force is the caps, and army "
                     "equipment is produced for it (caps x the template's equipment, less stock and fielded); "
                     "name the number of divisions this line builds to")
    # priority over every military line
    prio = [x for x in lint.plan.get("production_priority") or [] if isinstance(x, str)]
    prio_f = {fam(x) for x in prio} | set(prio)
    lines = {r["type"] for r in start if lint.factory_pool(r["type"]) == "military" and r["type"] not in dropped}
    lines |= {x["equipment"] for _, rows in adds.items() for _, x in rows
              if lint.factory_pool(x["equipment"]) == "military"}
    missing = sorted(k for k in lines if k not in prio_f and fam(k) not in prio_f)
    if lines and missing:
        lint.add("error", "production.priority_all", "production_priority",
                 ("production_priority is empty" if not prio else f"production_priority orders {len(prio)} of the "
                  f"{len(lines)} military lines") + f"; not ordered: {', '.join(missing)}. Order every line, the "
                 "start lines and the new ones, by what the plan needs first")


# ---------------------------------------------------------------------------------------------- need
def _variant_cost(lint, family):
    rows = derive.equipment_rows(lint.game)
    keys = [x.get("equipment") for x in lint.list_of("production") if isinstance(x.get("equipment"), str)]
    keys += [r["type"] for r in lint.start_lines()]
    for k in keys:
        if derive.equipment_family(lint.game, k) == family and (rows.get(k) or {}).get("cost"):
            return rows[k]["cost"], k
    cands = sorted(((r.get("year") or 0, k) for k, r in rows.items()
                    if r.get("archetype") == family and r.get("cost") and k != family))
    return (rows[cands[0][1]]["cost"], cands[0][1]) if cands else (None, None)


def need_balance(lint, tl):
    """Cumulative from the start to the judged date, per equipment family: what the country has (start
    stockpile plus what the planned lines make) against what the plan's army needs (the start divisions'
    shortfall, every division the training lines add, the fielded army's losses at war). The divisions are the
    lines' demand (tl.forecast(equipment=False)): the equipment-held forecast would only ask for what is made."""
    fc = tl.forecast(equipment=False)
    if not fc["lines"]:
        return
    g, w = lint.game, lint.world
    start = tl.start
    end = max([d for d, _ in lint.horizon()] or [start + 365])
    # judged on the first war day (what the army goes to war with), else at the end of the horizon
    judge = min(end, tl.war_day) if tl.war_day and tl.war_day > start + 90 else end
    fam = lambda k: derive.equipment_family(g, k)  # noqa: E731
    need = Counter()
    oob = w.oob(lint.tag)
    for dv in oob["divisions"]:
        for eq, a in tl.template_need(dv["template"]).items():
            need[fam(eq)] += a * max(0.0, 1.0 - dv.get("start_equipment_factor", 1.0))
    rows_fc, bt = fc["rows"], fc["by_template"]
    i = max(0, min(len(rows_fc) - 1, (judge - start) // 7))
    for t, n in bt[i].items():
        added = n - (bt[0].get(t, 0) if bt else 0)
        if added > 0:
            for eq, a in tl.template_need(t).items():
                need[fam(eq)] += added * a
    war_days = max(0, judge - tl.war_day) if tl.war_day is not None else 0
    if war_days:
        for t, n in bt[i].items():
            for eq, a in tl.template_need(t).items():
                need[fam(eq)] += n * a * LOSS_SHARE * war_days / 30.0
    have = Counter()
    for st in w.country(lint.tag).get("stockpile") or []:
        have[fam(st.get("v"))] += st.get("amount") or 0
    for d in range(start, judge + 1, 7):
        for k, units in lint.line_output(d).items():
            have[fam(k)] += units * min(7, judge - d + 1)
    rows = []
    for f, n in need.items():
        cost, key = _variant_cost(lint, f)
        if not cost or n < 1:
            continue
        rows.append({"family": f, "need": round(n), "have": round(have.get(f, 0)),
                     "ratio": round(have.get(f, 0) / n, 2), "need_ic": n * cost, "variant": key})
    when = derive.fmt(derive.date_of(judge))
    lint.facts["production_need"] = {"date": when, "rows": [{k: v for k, v in r.items() if k != "need_ic"}
                                                            for r in rows],
                                     "note": "from the start to the date: need = the start divisions' shortfall, "
                                             "the divisions the training lines add (their caps applied), 5% a month "
                                             "of the army's equipment at war; have = the start stockpile plus the "
                                             "planned lines' output"}
    if not rows:
        return
    for r in rows:
        if r["have"] == 0 and r["need"] > 0:
            lint.add("error", "production.missing_line", "production",
                     f"{r['family']} is needed by the planned divisions ({r['need']} by {when}) and no line makes it "
                     f"and none is stocked: add a target for {r['variant']}")
    low = [r for r in rows if r["ratio"] < NEAR_ZERO]
    high = [r for r in rows if r["ratio"] > FAR_ABOVE]
    if low and high:
        lint.add("error", "production.need_balance", "production",
                 f"by {when} the lines are out of balance with the planned army: "
                 + ", ".join(f"{r['family']} {r['have']} for a need of {r['need']} ({r['ratio']:g}x)" for r in high)
                 + " while " + ", ".join(f"{r['family']} {r['have']} for {r['need']} ({r['ratio']:g}x)" for r in low)
                 + ". Divide the military factories by the need of the planned divisions (facts.production_need), "
                   "and re-divide them as the factory count grows")
    # a parked request: a large line whose family the plan makes far above its need holds factories the
    # lines below it lack
    ratio = {r["family"]: r["ratio"] for r in rows}
    pool = lint.active_lines(min(judge, start + 365))
    mil = sum(n for k, n in pool.items() if lint.factory_pool(k) == "military")
    for k, n in pool.items():
        f = derive.equipment_family(g, k)
        if lint.factory_pool(k) == "military" and n >= 10 and ratio.get(f, 0) > FAR_ABOVE:
            lint.add("warning", "production.parked_request", "production",
                     f"{k} holds {n} of {mil} military factories while its family comes to {ratio[f]:g}x the planned "
                     f"army's need by {when}: about {max(1, round(n / ratio[f]))} would cover it, and new factories "
                     "land on a large unfilled request while the lines below starve; give each line its share of "
                     "the factories that exist on each date")


# ---------------------------------------------------------------------------------------------- economy
CG_KEYS = ("consumer_goods_factor", "consumer_goods_expected_value")


def _dynamic_consumer_goods(lint):
    """The consumer-goods terms of the dynamic modifiers the country's history adds, resolved with the
    variables its history sets: ({"factor": x, "expected": y}, [rows], {variable: [files that change it]})."""
    g = lint.game
    try:
        _, effects = derive.history_effects_of(g, lint.tag, lint.ctx, lint.start, lint.meta.get("history_file"))
    except Exception:  # noqa: BLE001 - advisory
        return {"factor": 0.0, "expected": 0.0}, [], {}
    var = {}
    mods = []
    for k, v in effects:
        if k == "set_variable" and isinstance(v, list):
            for a, b in v:
                if a and isinstance(b, str):
                    try:
                        var[a] = float(b)
                    except ValueError:
                        if b in var:
                            var[a] = var[b]
        elif k == "add_dynamic_modifier" and isinstance(v, list) and derive.get1(v, "modifier"):
            mods.append(derive.get1(v, "modifier"))

    def defs():
        out = {}
        for f in g.files("common/dynamic_modifiers"):
            for name, body in derive.parse_file(f):
                if isinstance(body, list):
                    out[name] = {k: derive.get1(body, k) for k in CG_KEYS if derive.get1(body, k) is not None}
        return out

    table = g.cached("economy:dynmods", defs)
    tot = {"factor": 0.0, "expected": 0.0}
    rows, names = [], set()
    for m in mods:
        for k, raw in (table.get(m) or {}).items():
            try:
                val, how = float(raw), "literal"
            except (TypeError, ValueError):
                names.add(raw)
                val, how = var.get(raw), f"variable {raw}"
            if val is None:
                rows.append({"modifier": m, "key": k, "value": None, "how": f"{how} (not set by the history)"})
                continue
            tot["factor" if k == "consumer_goods_factor" else "expected"] += val
            rows.append({"modifier": m, "key": k, "value": val, "how": how})
    changes = {}
    if names:
        try:
            rels = {p: r for r, p, _ in g.content.merged().values()}
        except (AttributeError, TypeError, ValueError):
            rels = {}
        for folder in ("common/on_actions", "common/scripted_effects", "common/national_focus", "common/decisions",
                       "events"):
            for path in g.files(folder):
                try:
                    with open(path, encoding="utf-8-sig", errors="replace") as fh:
                        text = fh.read()
                except OSError:
                    continue
                for n in names:
                    if re.search(r"(add_to|subtract_from|set|multiply)_variable\s*=\s*\{[^}]*\b" + re.escape(n) + r"\b",
                                 text) or re.search(r"\b" + re.escape(n) + r"\s*=\s*-?[\w.]+", text):
                        changes.setdefault(n, []).append(rels.get(path) or os.path.basename(path))
    return tot, rows, {k: sorted(set(v))[:6] for k, v in changes.items()}


def baseline_floors(lint):
    """{day: (civilian, military)}: the AI baseline's marks at BASELINE_SLACK."""
    b = load_baseline(lint.tag, lint.start)
    floors = {}
    for row in (b or {}).get("rows") or []:
        d = _day(row["date"])
        if d:
            floors[d] = (BASELINE_SLACK * row["civilian_factories"], BASELINE_SLACK * row["military_factories"])
    return floors


def forecast_of(lint, tl=None):
    """The plan's economy forecast (economy.forecast on the AI marks), computed once per lint run: the economy
    rules and the production model (Lint.military_on, the idle-factory feed) read the same one."""
    fc = getattr(lint, "_econ_fc", None)
    if fc is not None:
        return fc
    if tl is None:
        tl = getattr(lint, "_timeline", None)
        if tl is None:
            from . import conditions as cond
            tl = cond.Timeline(lint)
    floors = baseline_floors(lint)
    lint._econ_fc = econmod.forecast(lint, tl, marks=list(floors), floors=floors)
    return lint._econ_fc


def economy(lint, tl):
    w = lint.world
    tag = lint.tag
    st = w.states()
    owned = [s for s in w.owned_states(tag)]
    civ = sum(st[s]["buildings"].get("industrial_complex", 0) for s in owned)
    mil = sum(st[s]["buildings"].get("arms_factory", 0) for s in owned)
    cg_ideas, cg_src = calcmod.consumer_goods(w, tag)
    if getattr(lint, "_econ_dyn", None) is None:
        lint._econ_dyn = _dynamic_consumer_goods(lint)
    dyn, dyn_rows, dyn_changes = lint._econ_dyn
    cg = round(max(0.0, (cg_ideas + dyn["expected"]) * (1 + dyn["factor"])), 4)
    start = tl.start
    end = max([d for d, _ in lint.horizon()] or [start + 365])
    fallback = lint.plan.get("construction_fallback_any_own_state") is True
    entries = lint.list_of("construction")
    listed = {"industrial_complex": [], "arms_factory": []}
    for e in entries:
        if e.get("building") in listed and not e.get("fallback_any_own_state"):
            for s_ in e.get("states") or []:
                if isinstance(s_, int) and s_ in st and s_ not in listed[e["building"]]:
                    listed[e["building"]].append(s_)
        elif e.get("building") in listed:
            fallback = True
    slots = {}
    for b_, ss in listed.items():
        slots[b_] = sum(st[s_].get("free_slots") or 0 for s_ in ss)
    shared = sum(st[s_].get("free_slots") or 0 for s_ in set(listed["industrial_complex"]) | set(listed["arms_factory"]))
    # the plan's own factory targets by date
    targets = []
    for h in lint.list_of("horizons"):
        t = h.get("targets") if isinstance(h.get("targets"), dict) else {}
        d = _day(h.get("until"))
        if d:
            targets.append((d, t.get("civilian_factories"), t.get("military_factories")))
    if entries and not fallback and (listed["industrial_complex"] or listed["arms_factory"]):
        for d, c_t, m_t in sorted(targets):
            need_c = max(0, (c_t or civ) - civ) if isinstance(c_t, (int, float)) else 0
            need_m = max(0, (m_t or mil) - mil) if isinstance(m_t, (int, float)) else 0
            short = []
            if need_c > slots["industrial_complex"]:
                short.append(f"{need_c} civilian factories for the target {c_t} with {slots['industrial_complex']} free "
                             f"slots in states {', '.join(map(str, listed['industrial_complex'])) or 'none'}")
            if need_m > slots["arms_factory"]:
                short.append(f"{need_m} military factories for the target {m_t} with {slots['arms_factory']} free "
                             f"slots in states {', '.join(map(str, listed['arms_factory'])) or 'none'}")
            if need_c + need_m > shared and not short:
                short.append(f"{need_c + need_m} factories with {shared} free slots in all listed states")
            if short:
                lint.add("error", "construction.capacity", "construction",
                         f"by {derive.fmt(derive.date_of(d))} the plan builds " + "; ".join(short)
                         + " (free slots at the start; slots focuses add are not counted) and "
                           "construction_fallback_any_own_state is not true: once the listed states are full the "
                           "civilian factories on construction stand idle. Set the fallback, or list states with "
                           "slots for every factory the targets need")
                break
    # the plan's own levers by date (economy.forecast): laws, ideas, advisors, focus gifts, techs, annexations,
    # slots outside the construction risk; the plan's dated switch, else the one the arithmetic gives
    fc, lev = None, []
    try:
        fc = forecast_of(lint, tl)
        lev = econmod.levers(lint, tl, fc)
    except Exception as exc:  # noqa: BLE001 - advisory: the economy rules then run without the forecast
        lint.add("info", "economy.forecast", "construction", f"no forecast: {type(exc).__name__}: {exc}")
    table = fc["rows"] if fc else []
    lint.facts["economy"] = {
        "start": {"civilian": civ, "military": mil, "consumer_goods": cg,
                  "consumer_goods_civs": int(-(-cg * (civ + mil) // 1)), "free_civs": max(0, civ - int(-(-cg * (civ + mil) // 1))),
                  "consumer_goods_src": cg_src + [f"{x['modifier']} {x['key']} {x['value']} ({x['how']})"
                                                  for x in dyn_rows]},
        "consumer_goods_changed_by_script": dyn_changes,
        "free_slots": {"civilian_states": slots.get("industrial_complex"), "military_states": slots.get("arms_factory"),
                       "fallback": fallback},
        "forecast": fc,
        "levers": lev,
        "note": "forecast: what the plan's own dated levers give with the plan's dated switch (else the arithmetic's) "
                "(economy.py); levers: what the game offers that the plan leaves out or takes late, with the "
                "factories it adds by the first war day when taken at the start (law gates and focus time are "
                "the plan's to judge)"}
    if dyn_changes:
        lint.add("info", "economy.consumer_goods", "politics",
                 f"consumer goods start at {cg:g} of all factories ({lint.facts['economy']['start']['consumer_goods_civs']} "
                 f"civilian factories) and scripts change them over time through "
                 + ", ".join(f"{k} ({', '.join(v)})" for k, v in dyn_changes.items())
                 + ": state the share by quarter in plan.md and plan the laws, focuses and decisions that lower it")
    if fc:
        targets_vs_forecast(lint, fc, lev)
        construction_switch(lint, fc)
    baseline(lint, tl, table, fc, lev)


def load_baseline(tag, start):
    year = derive.as_tuple(start)[0] if start else None
    path = os.path.join(BASELINES, f"{tag}-{year}.json")
    if not os.path.isfile(path):
        return None
    with open(path, encoding="utf-8") as fh:
        return json.load(fh)


def _plan_marks(lint):
    """{day: {"factories" | "civilian_factories" | "military_factories" | "divisions": n}} the plan states:
    factories checkpoints and horizon targets."""
    plan_at = defaultdict(dict)
    for c in lint.list_of("checkpoints"):
        d = _day(c.get("by"))
        for a in _atoms(c.get("must")):
            m = re.match(rf"^factories(?::{lint.tag})?\s*>=?\s*(\d+)$", a.strip())
            if m and d:
                plan_at[d]["factories"] = max(plan_at[d].get("factories", 0), int(m.group(1)))
    for h in lint.list_of("horizons"):
        d = _day(h.get("until"))
        t = h.get("targets") if isinstance(h.get("targets"), dict) else {}
        for k_plan in ("divisions", "military_factories", "civilian_factories"):
            if isinstance(t.get(k_plan), (int, float)) and d:
                plan_at[d][k_plan] = t[k_plan]
    return plan_at


def targets_vs_forecast(lint, fc, lev):
    """economy.unbacked / economy.below_forecast: the plan's civilian and military targets against what its own
    levers give on that date."""
    rows = fc.get("rows") or []
    unbacked, low = [], []
    for d, v in sorted(_plan_marks(lint).items()):
        r = econmod.at(rows, d)
        if not r:
            continue
        for key, f_key in (("civilian_factories", "civ"), ("military_factories", "mil")):
            if key not in v:
                continue
            if v[key] > UNBACKED_SLACK * r[f_key] + 1:
                unbacked.append(f"{key.replace('_', ' ')} {v[key]} by {derive.fmt(derive.date_of(d))} against "
                                f"{r[f_key]} the plan's levers give")
            elif v[key] < BASELINE_SLACK * r[f_key] - 1:
                low.append(f"{key.replace('_', ' ')} {v[key]} by {derive.fmt(derive.date_of(d))} against "
                           f"{r[f_key]} its levers give")
    if unbacked:
        lint.add("warning", "economy.unbacked", "horizons",
                 "targets the plan's own levers do not reach (facts.economy.forecast): " + "; ".join(unbacked[:6])
                 + ". Add the levers that close the gap (facts.economy.levers: "
                 + ", ".join(f"{x['id']} +{x['gain']}" for x in lev[:5] if x.get("gain")) + ") or lower the target")
    if low:
        lint.add("warning", "economy.below_forecast", "horizons",
                 "targets below what the plan's own levers give (aim at the forecast, it is the plan's economy): "
                 + "; ".join(low[:6]))


def construction_switch(lint, fc):
    """construction.switch: the base construction list queues military factories from the first day while the
    arithmetic switches from civilian to military factories later (the executor runs every job of the active
    list, so both are built side by side): the military job belongs in a phase override from the switch date."""
    sw = _day(fc.get("switch"))
    start = _day(lint.start)
    if sw is None or start is None or sw - start < SWITCH_GRACE_DAYS:
        return
    jobs = [e for e in lint.list_of("construction") if e.get("building") == "arms_factory"]
    if jobs and any(e.get("building") == "industrial_complex" for e in lint.list_of("construction")):
        lint.add("warning", "construction.switch", "construction",
                 f"the base construction list holds an arms_factory job, so military factories are queued from "
                 f"{lint.start} beside the civilian ones, while the arithmetic switches on {fc['switch']} "
                 f"(facts.economy.forecast): keep only civilian jobs in the base list and put the arms_factory job "
                 f"in a phase whose overrides.construction starts at the switch date")


CONSTRAINT_KINDS = ("no_free_slots", "law_unavailable")


def _constraint_ok(lint, c, fc, day):
    """A goal.ai_baseline_constraint the lint can check: no_free_slots (the forecast found no free slot outside
    the construction risk on or before the date) or law_unavailable (the law is not offered to the country, or
    its gate is closed under the plan's own timeline on the date). (ok, why)."""
    if not isinstance(c, dict) or c.get("kind") not in CONSTRAINT_KINDS:
        return False, "not a checkable constraint (kind must be one of " + ", ".join(CONSTRAINT_KINDS) + ")"
    if c["kind"] == "no_free_slots":
        first = _day((fc or {}).get("first_slot_bound"))
        if first is not None and first <= day:
            return True, f"no free slot outside the construction risk from {fc['first_slot_bound']}"
        return False, "the forecast still finds free slots outside the construction risk on that date"
    law = c.get("law")
    try:
        laws = lint.world.laws(lint.tag)
    except Exception:  # noqa: BLE001
        laws = {}
    offered = {o.get("id"): o for row in laws.values() for o in row.get("options") or []}
    if law not in offered:
        return True, f"{law} is not offered to {lint.tag}"
    facts_at = getattr(lint, "_gate_facts", None)
    raw = (lint.world.ideas().get(law) or {}).get("available")
    gate = raw if isinstance(raw, list) else None
    if facts_at is not None and isinstance(gate, list) and gate:
        from . import plan_gates
        done = {f for end, f in getattr(lint, "_focus_done", []) if end <= day}
        if plan_gates.evaluate(gate, facts_at(day, done)) is False:
            return True, f"{law}'s gate is closed on that date under the plan's timeline"
    return False, f"{law} is offered and its gate is not shown closed on that date"


def baseline(lint, tl, table, fc=None, lev=()):
    """plan.ai_baseline: the plan's factory marks (checkpoints, horizon targets) and the forecast of its own
    levers against the game's own AI on the same dates. Factories below BASELINE_SLACK of the AI's are an error
    unless goal.ai_baseline_constraint names a hard game constraint the lint confirms. Divisions likewise up to
    the first war day (the army forecast counts the equipment the planned lines make); later division marks
    stay a warning goal.ai_baseline_why can answer."""
    b = load_baseline(lint.tag, lint.start)
    if not b:
        return
    goal = lint.plan.get("goal") if isinstance(lint.plan.get("goal"), dict) else {}
    why = goal.get("ai_baseline_why")
    constraints = goal.get("ai_baseline_constraint") or []
    if isinstance(constraints, dict):
        constraints = [constraints]
    marks, div_marks, div_errors, fc_marks, split = [], [], [], [], []
    war_day = getattr(tl, "war_day", None)
    plan_at = _plan_marks(lint)
    horizon_end = max([x for x, _ in lint.horizon()] + list(plan_at) or [None]) if (lint.horizon() or plan_at) else None
    rows = (fc or {}).get("rows") or []
    fc_until = _day((fc or {}).get("war")) if (fc or {}).get("war") else None
    for row in b.get("rows") or []:
        d = _day(row["date"])
        if d is None or (horizon_end is not None and d > horizon_end):
            continue
        ai_c, ai_m = row["civilian_factories"], row["military_factories"]
        ai_f = ai_c + ai_m + row.get("naval_factories", 0)
        near = [(x, v) for x, v in plan_at.items() if d - 31 <= x <= d + 92]
        # the forecast models the economy the plan builds, not what conquest adds: it is held to the AI only up
        # to the first war day (later marks are the plan's targets, checked above)
        r = econmod.at(rows, d) if rows and (fc_until is None or d <= fc_until) else None
        # the floor is what the factories make: military and total at or above the AI's make a civilian
        # shortfall the plan's split (an earlier switch), a warning with its reason, not an error
        meets = bool(r) and r["mil"] >= BASELINE_SLACK * ai_m and r["civ"] + r["mil"] >= BASELINE_SLACK * (ai_c + ai_m)
        for x, v in near:
            when = derive.fmt(derive.date_of(x))
            if "factories" in v and v["factories"] < BASELINE_SLACK * ai_f:
                marks.append((d, f"factories {v['factories']} by {when} against the AI's {ai_f} on {row['date']}"))
            for k_plan, ai_v in (("civilian_factories", ai_c), ("military_factories", ai_m)):
                if k_plan in v and v[k_plan] < BASELINE_SLACK * ai_v:
                    text = (f"{k_plan.replace('_', ' ')} {v[k_plan]} by {when} against the AI's {ai_v} on "
                            f"{row['date']}")
                    (split if k_plan == "civilian_factories" and meets else marks).append((d, text))
            if "divisions" in v and v["divisions"] < BASELINE_SLACK * row["divisions"]:
                (div_errors if war_day is None or d <= war_day else div_marks).append(
                    f"divisions {v['divisions']} by {when} against the AI's {row['divisions']} on {row['date']}")
        if r and (r["civ"] < FORECAST_SLACK * ai_c or r["mil"] < FORECAST_SLACK * ai_m):
            text = f"{r['civ']} civilian / {r['mil']} military on {row['date']} against the AI's {ai_c} / {ai_m}"
            (split if meets else fc_marks).append((d, "forecast " + text if meets else text))
        divs, _ = tl.army_at(d)
        if divs < BASELINE_SLACK * row["divisions"]:
            held = ", ".join(f"{t} by {why}" for t, why in tl.limit_at(d).items()) if hasattr(tl, "limit_at") else ""
            (div_errors if war_day is None or d <= war_day else div_marks).append(
                f"the training lines field {round(divs)} divisions on {row['date']} against the AI's "
                f"{row['divisions']}" + (f" (held: {held})" if held else ""))
    held, refused = [], []
    for c in constraints:
        ok, how = _constraint_ok(lint, c, fc, _day(c.get("date")) if isinstance(c, dict) and c.get("date")
                                 else (horizon_end or _day(lint.start)))
        (held if ok else refused).append((c, how))
    lint.facts["ai_baseline"] = {"file": f"baselines/{lint.tag}-{derive.as_tuple(lint.start)[0]}.json",
                                 "below": [m for _, m in marks] + div_errors + div_marks,
                                 "forecast_below": [m for _, m in fc_marks],
                                 "civilian_split": [m for _, m in split],
                                 "forecast_checked_until": (fc or {}).get("war"), "why": why,
                                 "constraints": [{"constraint": c, "confirmed": True, "how": h} for c, h in held]
                                 + [{"constraint": c, "confirmed": False, "how": h} for c, h in refused]}
    if refused:
        lint.add("error", "plan.ai_baseline", "goal.ai_baseline_constraint",
                 "constraints the lint cannot confirm: " + "; ".join(f"{json.dumps(c)}: {h}" for c, h in refused))
    covered = bool(held)
    lever_text = ", ".join(f"{x['kind']} {x['id']} ({x['gives']}; +{x['gain']} by the first war day)"
                           for x in list(lev)[:6] if x.get("gain")) or "none found"
    if marks and not covered:
        lint.add("error", "plan.ai_baseline", "checkpoints",
                 f"the plan schedules fewer factories than the game's own AI reaches as {lint.tag} ({b.get('ai')} "
                 f"AI, {b.get('difficulty')}): " + "; ".join(m for _, m in marks[:6])
                 + (" ..." if len(marks) > 6 else "")
                 + ". A written reason does not lift this floor; raise the targets with the levers that reach them "
                   "(facts.economy.forecast, facts.economy.levers: " + lever_text + "), or name a hard game "
                   "constraint in goal.ai_baseline_constraint (no_free_slots, law_unavailable) the lint can confirm")
    if fc_marks and not covered:
        lint.add("error", "plan.ai_baseline", "construction",
                 f"the plan's own levers (facts.economy.forecast, switch {fc.get('switch')}) build fewer factories "
                 f"than the game's own AI: " + "; ".join(m for _, m in fc_marks[:6])
                 + ". Use more of what the game offers (laws by date, construction advisors and focuses, "
                   "construction and building-slot techs early, civilian-first to the switch date, build in every "
                   "state outside the construction risk, a smaller import budget): " + lever_text)
    if split:
        lint.add("warning", "plan.ai_baseline", "split",
                 "fewer civilian factories than the game's own AI while the plan's forecast meets its military and "
                 "total factories on the same dates (an earlier switch): " + "; ".join(m for _, m in split[:6])
                 + (" ..." if len(split) > 6 else "") + ". goal.ai_baseline_why: say what the military "
                 "factory-days buy by the first war day and what the civilian factories cost later"
                 + (f" (given: {why})" if why else ""))
    # Divisions: the count against the AI is reported, not a floor. A division count says nothing of what the
    # force is made of (a plan reached the AI's count with one infantry template and no armour for its wars);
    # the floor is the force table's readiness (lint_force.py force.readiness, force.breakthrough).
    if div_errors or div_marks:
        lint.add("info", "plan.ai_baseline", "training",
                 f"division count against the game's own AI as {lint.tag} (reported, not a floor; force.readiness "
                 "holds the force table): " + "; ".join((div_errors + div_marks)[:6])
                 + (" ..." if len(div_errors) + len(div_marks) > 6 else ""))


def _atoms(pred):
    if isinstance(pred, str):
        yield pred
    elif isinstance(pred, list):
        for p in pred:
            yield from _atoms(p)
    elif isinstance(pred, dict):
        for p in pred.values():
            yield from _atoms(p)
