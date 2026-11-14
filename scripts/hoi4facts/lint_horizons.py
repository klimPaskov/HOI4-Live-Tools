"""Horizon checks (rule plan.horizons), run by lint_plan.py (Lint.run) on every plan.

A plan details its first horizon (about one game year, or up to the first major checkpoint) and gives every
later horizon an intent and numeric targets only; the rolling planner (hoi4_live/planner) fills the detail
from the live state as each horizon comes up. `horizons` is
    [{"id", "from", "until", "intent", "targets": {"divisions", "military_factories", ...}}]

Rules:
  plan.horizons   (error) a horizon whose until is not after its from, horizons out of order or overlapping,
                  the first one not starting at the scenario start; (warning) a gap between two horizons, the
                  last one ending before goal.deadline, _validator.stop_date not the first horizon's until;
                  (info) a plan without horizons (all of it is detail), research_path years past the first
                  horizon (the updates replace them)
facts.horizons lists each horizon with its days and the first horizon's end.
"""

from . import derive

RULE = "plan.horizons"


def _day(date):
    return derive.day(date) if date else None


def check(lint):
    plan = lint.plan
    horizons = plan.get("horizons")
    if not horizons:
        lint.add("info", RULE, "horizons", "no horizons: the whole plan is detail (a rolling plan details the "
                 "first horizon and gives later ones intent and targets)")
        return lint
    if not isinstance(horizons, list):
        return lint
    start = _day(lint.start)
    rows, prev = [], None
    for i, h in enumerate(horizons):
        if not isinstance(h, dict):
            continue
        where = "horizons[%d]" % i
        a = _day(h.get("from")) if h.get("from") else (prev if prev is not None else start)
        b = _day(h.get("until"))
        if b is None:
            continue
        if a is not None and b <= a:
            lint.add("error", RULE, where, "until %s is not after from %s" % (h.get("until"), derive.fmt(
                derive.date_of(a))))
        if i == 0 and start is not None and a is not None and abs(a - start) > 1:
            lint.add("error", RULE, where, "the first horizon starts %s, the scenario %s"
                     % (derive.fmt(derive.date_of(a)), derive.fmt(derive.date_of(start))))
        if prev is not None and a is not None:
            if a < prev:
                lint.add("error", RULE, where, "starts %s, before the horizon above ends (%s)"
                         % (derive.fmt(derive.date_of(a)), derive.fmt(derive.date_of(prev))))
            elif a > prev + 1:
                lint.add("warning", RULE, where, "a gap of %d days after the horizon above" % (a - prev))
        rows.append({"id": h.get("id"), "from": derive.fmt(derive.date_of(a)) if a is not None else None,
                     "until": derive.fmt(derive.date_of(b)), "days": (b - a) if a is not None else None,
                     "targets": sorted((h.get("targets") or {}).keys()) if isinstance(h.get("targets"), dict)
                     else []})
        prev = b
    if not rows:
        return lint
    first_end = _day(rows[0]["until"])
    stop = _day(lint.meta.get("stop_date")) if lint.meta.get("stop_date") else None
    if stop is not None and abs(stop - first_end) > 1:
        lint.add("warning", RULE, "_validator.stop_date", "the detail horizon (stop_date %s) is not the first "
                 "horizon's until (%s)" % (lint.meta.get("stop_date"), rows[0]["until"]))
    deadline = _day(lint.goal.get("deadline")) if lint.goal.get("deadline") else None
    if deadline is not None and prev is not None and prev < deadline:
        lint.add("warning", RULE, "horizons[%d]" % (len(horizons) - 1), "the last horizon ends %s, before the "
                 "goal deadline %s" % (rows[-1]["until"], lint.goal.get("deadline")))
    end_year = derive.date_of(first_end)[0]
    late = sorted({e.get("year") for e in plan.get("research_path") or [] if isinstance(e, dict)
                   and isinstance(e.get("year"), int) and e["year"] > end_year + 1})
    if late:
        lint.add("info", RULE, "research_path", "research for %s is past the first horizon (%s): the rolling "
                 "updates replace it from the live state" % (", ".join(map(str, late)), rows[0]["until"]))
    lint.facts["horizons"] = {"first_until": rows[0]["until"], "horizons": rows}
    return lint
