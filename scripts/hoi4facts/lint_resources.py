"""Resources as strategy, run by lint_plan.py after lint_economy: every strategic resource the planned production
lines need on a date is covered by a named source.

  economy.resource_source  a resource the lines in force on a date need beyond the country's own output (its states
                           at the start less the trade law's export share) with no named source for the rest:
                           `resource_sources` {"rubber": [{"source": "conquest", "states": [..], "by": date},
                           {"source": "synthetic", "refineries": n, "by": date}, {"source": "trade",
                           "partner": TAG, "route": "land"|"sea", "units": n, "by"?}, {"source": "deficit",
                           "units": n, "until": date, "why": ...}]} (a trade[] deal of the resource counts as
                           a trade source). Error when the plan keeps `resource_sources` (it claims the cover);
                           a warning otherwise (a hand plan, or one that leaves trade to the game's automatic
                           importing, which neither ranks partners nor sees raided routes).
                           A sea trade with no escort (navy.convoy_escort) while the plan is at war with a
                           navy is reported in the message (raiding). A source past its `until` stops counting.

Need: the production lines in force on each line date (Lint.active_lines) times each equipment's resources per
factory (common/units/equipment, a design's base hull or chassis: its modules are not counted). Own output:
facts.resources (the start states, the trade law's min_export). Conquest: the named states' resources from the
history files (facts resource-states lists them with owner and reach). Synthetic: refineries x rubber per
refinery (the building's base plus the techs the plan's research path lists). facts.resource_sources holds the
arithmetic per resource.
"""
from . import derive
from . import resources as resmod

SOURCE_KINDS = ("own", "conquest", "synthetic", "trade", "deficit")


def _day(s):
    try:
        return derive.day(s) if s else None
    except Exception:  # noqa: BLE001 - a bad date is the schema's finding
        return None


def _research_techs(plan):
    out = set()
    for e in plan.get("research_path") or []:
        if isinstance(e, dict):
            out.update(t for t in e.get("techs") or [] if isinstance(t, str))
    out.update(t for t in plan.get("research_priority") or [] if isinstance(t, str))
    return out


def _line_dates(lint):
    start = _day(lint.start)
    days = {start} if start is not None else set()
    for e in lint.plan.get("production") or []:
        if isinstance(e, dict) and _day(e.get("from")) is not None:
            days.add(_day(e.get("from")))
    deadline = _day((getattr(lint, "goal", None) or {}).get("deadline"))
    return sorted(d for d in days if deadline is None or d <= deadline)


def source_units(lint, resource, src, day, refinery):
    """Units one named source gives on `day` (0 before its date), and its text."""
    kind = src.get("source")
    by, until = _day(src.get("by")), _day(src.get("until"))
    if (by is not None and day < by) or (until is not None and day > until):
        return 0.0, None
    if kind == "conquest":
        states = lint.world.states()
        n = sum(((states.get(s) or {}).get("resources") or {}).get(resource, 0) for s in src.get("states") or []
                if isinstance(s, int))
        return float(n), f"conquest of {src.get('states')}"
    if kind == "synthetic" and resource == "rubber":
        per = resmod.refinery_rubber(refinery, _research_techs(lint.plan))
        n = (src.get("refineries") or 0) * per
        return float(n), f"{src.get('refineries')} synthetic refineries x {per:g}"
    if kind == "synthetic":
        return float(src.get("units") or 0), "synthetic"
    if kind == "trade":
        return float(src.get("units") or 0), f"trade {src.get('partner')} by {src.get('route')}"
    if kind == "deficit":
        return float(src.get("units") or 0), f"temporary deficit until {src.get('until')}: {src.get('why')}"
    if kind == "own":
        return float(src.get("units") or 0), "own (named)"
    return 0.0, None


def check(lint):
    if not lint.tag or lint.world is None:
        return lint
    plan = lint.plan
    named = plan.get("resource_sources")
    named = named if isinstance(named, dict) else {}
    severity = "error" if named else "warning"
    try:
        bal = lint.world.resources(lint.tag)["balance"]
        eqres = lint.world.equipment_resources()
        refinery = resmod.synthetic_refinery(lint.world)
    except Exception as exc:  # noqa: BLE001 - facts unavailable: said so, not judged
        lint.add("info", "economy.resource_source", "resource_sources", f"no resource arithmetic: {exc}")
        return lint
    trade = plan.get("trade")
    deals = trade if isinstance(trade, list) else ((trade or {}).get("deals") or [] if isinstance(trade, dict) else [])
    escort = bool(((plan.get("navy") or {}).get("convoy_escort")))
    rows, reported = [], set()
    for day in _line_dates(lint):
        need = {}
        for eq, f in lint.active_lines(day).items():
            for r, n in ((eqres.get(eq) or {}).get("resources") or {}).items():
                need[r] = need.get(r, 0.0) + n * (f or 0)
        for r, n in sorted(need.items()):
            own = (bal.get(r) or {}).get("available", 0.0)
            if n <= own + 1e-9:
                continue
            cover, parts = own, [f"own {own:g}"]
            for src in named.get(r) or []:
                if isinstance(src, dict):
                    u, text = source_units(lint, r, src, day, refinery)
                    if text and u:
                        cover += u
                        parts.append(f"{text} {u:g}")
            for d in deals:
                if isinstance(d, dict) and d.get("resource") == r and (_day(d.get("from")) or 0) <= day:
                    u = float(d.get("amount") or d.get("units") or 0)
                    cover += u
                    parts.append(f"trade deal {d.get('id', d.get('country'))} {u:g}")
            date = derive.fmt(derive.date_of(day))
            rows.append({"date": date, "resource": r, "need": round(n, 1), "cover": round(cover, 1),
                         "sources": parts})
            if cover + 1e-9 < n and r not in reported:
                reported.add(r)
                sea = [s for s in named.get(r) or [] if isinstance(s, dict) and s.get("route") == "sea"]
                raid = " (a sea trade with no navy.convoy_escort: raided convoys stop it)" if sea and not escort else ""
                lint.add(severity, "economy.resource_source", f"resource_sources.{r}",
                         f"on {date} the lines need {n:g} {r}, the named sources give {cover:g} "
                         f"({', '.join(parts)}){raid}: name a source (states taken by a date for their {r}, "
                         f"synthetic refineries, a trade partner with its route) or a temporary deficit with its "
                         f"reason (facts resource-states {lint.tag} --resource {r})")
    lint.facts["resource_sources"] = {"rows": rows[:60], "kinds": SOURCE_KINDS,
                                      "refinery": refinery, "d": resmod.RESOURCES_STRATEGY_METHOD}
    return lint
