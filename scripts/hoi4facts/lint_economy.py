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
  plan.ai_baseline           (warning) where a baseline of the game's own AI exists for the country and start
                             (skills/hoi4-campaign-playbook/baselines/<TAG>-<year>.json), a plan checkpoint,
                             horizon target or forecast below the AI's mark on that date, with no reason in
                             `goal.ai_baseline_why`.
facts.economy: factories, consumer goods at the start (ideas, and the dynamic modifiers the history adds,
resolved with the variables it sets), the scripts that change them later, free slots, free civilian factories
by quarter on the listed builds; economy.consumer_goods (info) names the scripts. facts.production_need: need
and what the country has per family.
"""
import json
import os
import re
from collections import Counter, defaultdict

from . import calc as calcmod
from . import derive

HERE = os.path.dirname(os.path.abspath(__file__))
BASELINES = os.path.join(os.path.dirname(os.path.dirname(HERE)), "skills", "hoi4-campaign-playbook", "baselines")
NEAR_ZERO = 0.6         # need_balance: a family with less than this share of its need is starved
FAR_ABOVE = 2.5         # ... while another family has more than this many times its need
BASELINE_SLACK = 0.9    # plan.ai_baseline: below 90% of the AI's mark warns
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
    shortfall, every division the training lines add, the fielded army's losses at war)."""
    fc = tl.forecast()
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


def economy(lint, tl):
    w = lint.world
    tag = lint.tag
    st = w.states()
    owned = [s for s in w.owned_states(tag)]
    civ = sum(st[s]["buildings"].get("industrial_complex", 0) for s in owned)
    mil = sum(st[s]["buildings"].get("arms_factory", 0) for s in owned)
    cg_ideas, cg_src = calcmod.consumer_goods(w, tag)
    dyn, dyn_rows, dyn_changes = _dynamic_consumer_goods(lint)
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
    # free civilian factories by quarter on the calc's build of the listed factories (a floor: no construction
    # bonuses from techs, focuses or laws)
    table = []
    order = []
    for b_, k in (("industrial_complex", "civ"), ("arms_factory", "mil")):
        order += [k] * (slots[b_] if not fallback else 40)
    if order:
        try:
            r = calcmod.build_order(w, calcmod.parse_order(",".join(f"{k}:1" for k in order), w), civ, mil, cg,
                                    lint.start, derive.fmt(derive.date_of(end)))
        except Exception:  # noqa: BLE001 - the calc refuses odd input; the table is left out then
            r = None
        if r:
            for q in range(start, end, 91):
                done = [x for x in r["rows"] if _day(x["date"]) <= q]
                c_, m_ = (done[-1]["civ"], done[-1]["mil"]) if done else (civ, mil)
                table.append({"date": derive.fmt(derive.date_of(q)), "civ": c_, "mil": m_,
                              "free_civs": max(0, c_ - int(-(-cg * (c_ + m_) // 1)))})
    lint.facts["economy"] = {
        "start": {"civilian": civ, "military": mil, "consumer_goods": cg,
                  "consumer_goods_civs": int(-(-cg * (civ + mil) // 1)), "free_civs": max(0, civ - int(-(-cg * (civ + mil) // 1))),
                  "consumer_goods_src": cg_src + [f"{x['modifier']} {x['key']} {x['value']} ({x['how']})"
                                                  for x in dyn_rows]},
        "consumer_goods_changed_by_script": dyn_changes,
        "free_slots": {"civilian_states": slots.get("industrial_complex"), "military_states": slots.get("arms_factory"),
                       "fallback": fallback},
        "by_quarter": table,
        "note": "by_quarter builds the listed factories with the start consumer goods and no construction bonuses "
                "(a floor); law, focus and decision changes to consumer goods are the plan's to count in plan.md"}
    if dyn_changes:
        lint.add("info", "economy.consumer_goods", "politics",
                 f"consumer goods start at {cg:g} of all factories ({lint.facts['economy']['start']['consumer_goods_civs']} "
                 f"civilian factories) and scripts change them over time through "
                 + ", ".join(f"{k} ({', '.join(v)})" for k, v in dyn_changes.items())
                 + ": state the share by quarter in plan.md and plan the laws, focuses and decisions that lower it")
    baseline(lint, tl, table)


def load_baseline(tag, start):
    year = derive.as_tuple(start)[0] if start else None
    path = os.path.join(BASELINES, f"{tag}-{year}.json")
    if not os.path.isfile(path):
        return None
    with open(path, encoding="utf-8") as fh:
        return json.load(fh)


def baseline(lint, tl, table):
    b = load_baseline(lint.tag, lint.start)
    if not b:
        return
    why = (lint.plan.get("goal") or {}).get("ai_baseline_why") if isinstance(lint.plan.get("goal"), dict) else None
    marks = []
    # the plan's own numbers by date: factories checkpoints, horizon targets, the army forecast
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
    for row in b.get("rows") or []:
        d = _day(row["date"])
        if d is None or d > max([x for x, _ in lint.horizon()] or [d]):
            continue
        ai_f = row["civilian_factories"] + row["military_factories"] + row.get("naval_factories", 0)
        # the nearest plan number at or after the AI's date (within a quarter)
        near = [(x, v) for x, v in plan_at.items() if d - 31 <= x <= d + 92]
        for x, v in near:
            if "factories" in v and v["factories"] < BASELINE_SLACK * ai_f:
                marks.append(f"factories {v['factories']} by {derive.fmt(derive.date_of(x))} against the AI's {ai_f} "
                             f"on {row['date']}")
            for k_plan, ai_k in (("civilian_factories", "civilian_factories"), ("military_factories", "military_factories")):
                if k_plan in v and v[k_plan] < BASELINE_SLACK * row[ai_k]:
                    marks.append(f"{k_plan.replace('_', ' ')} {v[k_plan]} by {derive.fmt(derive.date_of(x))} against "
                                 f"the AI's {row[ai_k]} on {row['date']}")
            if "divisions" in v and v["divisions"] < BASELINE_SLACK * row["divisions"]:
                marks.append(f"divisions {v['divisions']} by {derive.fmt(derive.date_of(x))} against the AI's "
                             f"{row['divisions']} on {row['date']}")
        divs, _ = tl.army_at(d)
        if divs < BASELINE_SLACK * row["divisions"]:
            marks.append(f"the training lines field at most {round(divs)} divisions on {row['date']} against the "
                         f"AI's {row['divisions']}")
    lint.facts["ai_baseline"] = {"file": f"baselines/{lint.tag}-{derive.as_tuple(lint.start)[0]}.json",
                                 "below": marks}
    if marks and not why:
        lint.add("warning", "plan.ai_baseline", "checkpoints",
                 f"the plan schedules less than the game's own AI reaches as {lint.tag} ({b.get('ai')} AI, "
                 f"{b.get('difficulty')}): " + "; ".join(marks[:6]) + (" ..." if len(marks) > 6 else "")
                 + ". Raise the plan, or say why in goal.ai_baseline_why")


def _atoms(pred):
    if isinstance(pred, str):
        yield pred
    elif isinstance(pred, list):
        for p in pred:
            yield from _atoms(p)
    elif isinstance(pred, dict):
        for p in pred.values():
            yield from _atoms(p)
