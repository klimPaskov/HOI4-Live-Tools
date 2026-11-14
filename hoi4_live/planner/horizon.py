"""Game days of planned work left in each queue the plan owns, and whether that is below the low-water mark.

    research      planned techs not yet researched or running, in base days, over the research slots
    focus         planned focuses (focus_path, focus_fallback) not done and not known unavailable, in days
    politics      political power with nothing planned to buy: the feed's political_power_idle, or power above
                  the reserve that the remaining planned picks cannot use
    production    an equipment family the executor reported short (stockpile_low, equipment_fill_low)
    construction  idle civilian factories with no construction left in the plan
    army          victory-point targets the plan still has to take

`Costs` gives the days of a tech or a focus. The defaults are flat estimates; `hoi4_live.planner.facts` binds
the game files' costs when the planning tools are installed. Every estimate names its inputs in `why`.
"""

from .state import year_of

LOW = {"research_days": 60, "focus_days": 35, "signal_days": 30, "construction_signal_days": 15}
TECH_DAYS = 120.0
FOCUS_DAYS = 70.0


class Costs:
    def __init__(self, tech_days=None, focus_days=None, tech_year=None):
        self._tech, self._focus, self._year = tech_days, focus_days, tech_year
        self.source = "game files" if tech_days or focus_days else "flat estimate"

    def tech(self, key):
        v = self._tech(key) if self._tech else None
        return float(v) if isinstance(v, (int, float)) and v > 0 else TECH_DAYS

    def focus(self, key):
        v = self._focus(key) if self._focus else None
        return float(v) if isinstance(v, (int, float)) and v > 0 else FOCUS_DAYS

    def year(self, key):
        return self._year(key) if self._year else None


def planned_techs(plan):
    out, seen = [], set()
    for block in plan.get("research_path") or []:
        if isinstance(block, dict):
            for t in block.get("techs") or []:
                if isinstance(t, str) and t not in seen:
                    seen.add(t)
                    out.append(t)
    for t in plan.get("research_priority") or []:
        if isinstance(t, str) and t not in seen:
            seen.add(t)
            out.append(t)
    return out


def planned_focuses(plan):
    out = []
    for key in ("focus_path", "focus_fallback"):
        out += [f for f in plan.get(key) or [] if isinstance(f, str) and f not in out]
    return out


def _recent(day, now, window):
    return day is not None and now is not None and now - day <= window


def research(plan, snap, costs, low):
    slots = snap.get("slots") or 0
    done = set(snap.get("research_done") or [])
    running = {t for t in snap.get("researching") or [] if t}
    left = [t for t in planned_techs(plan) if t not in done and t not in running]
    if not slots:
        return {"days": None, "low": False, "why": "research slots not read yet", "left": left}
    days = sum(costs.tech(t) for t in left) / slots
    idle = sum(1 for t in snap.get("researching") or [] if not t)
    low_now = days < low["research_days"] or (idle > 0 and not left)
    return {"days": round(days, 1), "low": low_now, "left": left, "idle_slots": idle,
            "why": "%d planned techs left (%s) over %d slots = %.0f days; low-water %d"
                   % (len(left), costs.source, slots, days, low["research_days"])}


def focus(plan, snap, costs, low):
    done = set(snap.get("focuses_done") or [])
    states = snap.get("focus_states") or {}
    left = [f for f in planned_focuses(plan) if f not in done and f != snap.get("focus")
            and states.get(f) != "unavailable"]
    days = sum(costs.focus(f) for f in left)
    low_now = days < low["focus_days"]
    return {"days": round(days, 1), "low": low_now, "left": left,
            "why": "%d planned focuses left (%s) = %.0f days; low-water %d"
                   % (len(left), costs.source, days, low["focus_days"])}


def _politics_reserve(plan):
    auto = plan.get("politics_auto") if isinstance(plan.get("politics_auto"), dict) else {}
    return float(auto.get("threshold") or 0)


def politics(plan, snap, executor, low):
    now = snap.get("day")
    signal = (snap.get("signals") or {}).get("political_power_idle")
    done = set((executor or {}).get("done") or {})
    pending = [p for p in plan.get("politics") or [] if isinstance(p, dict)
               and "pol:id:%s" % p.get("id") not in done]
    pending_cost = sum(float(p.get("cost") or 0) for p in pending)
    pp, gain = snap.get("pp"), snap.get("pp_gain")
    out = {"days": None, "low": False, "pending": [p.get("id") for p in pending]}
    if _recent(signal, now, low["signal_days"]):
        out.update(low=True, why="the executor reports political power with nothing to buy (day %s)" % signal)
        return out
    if isinstance(pp, (int, float)) and isinstance(gain, (int, float)) and gain > 0:
        spare = pp - _politics_reserve(plan) - pending_cost
        out["days"] = round(max(0.0, -spare) / gain, 1) if spare < 0 else 0.0
        if spare > 0 and not (plan.get("politics_auto") or {}).get("order"):
            out.update(low=True, why="%.0f PP above the reserve and the planned picks, and no politics_auto order"
                       % spare)
            return out
    out["why"] = "political power has planned uses"
    return out


def production(plan, snap, low):
    now = snap.get("day")
    short = {eq: v for eq, v in (snap.get("shortages") or {}).items() if _recent(v.get("day"), now, low["signal_days"])}
    fill = {a: v for a, v in (snap.get("fill_low") or {}).items() if _recent(v.get("day"), now, low["signal_days"])}
    if not short and not fill:
        return {"days": None, "low": False, "why": "no equipment family reported short", "short": {}, "fill_low": {}}
    parts = ["%s %s days of cover" % (eq, v.get("days_of_cover")) for eq, v in sorted(short.items())]
    parts += ["%s fill %s" % (a, v.get("fill")) for a, v in sorted(fill.items())]
    return {"days": min([v.get("days_of_cover") or 0 for v in short.values()] or [None]) if short else None,
            "low": True, "short": short, "fill_low": fill, "why": "short: " + "; ".join(parts)}


def construction(plan, snap, low):
    now = snap.get("day")
    signal = (snap.get("signals") or {}).get("civilian_factories_idle")
    if _recent(signal, now, low["construction_signal_days"]):
        return {"days": 0.0, "low": True, "why": "%s civilian factories idle (day %s)" % (snap.get("civ_idle"), signal)}
    return {"days": None, "low": False, "why": "no idle civilian factories reported"}


def army(plan, snap):
    tag = snap.get("player") or (plan.get("_meta") or {}).get("tag")
    ctl = snap.get("controllers") or {}
    targets = [t for t in plan.get("vp_targets") or [] if isinstance(t, dict)]
    left = [t for t in targets if ctl.get(str(t.get("state"))) != tag]
    return {"days": None, "low": bool(targets) and not left, "targets_left": len(left),
            "why": "%d of %d victory-point targets not held" % (len(left), len(targets))}


def estimate(plan, snap, costs=None, executor=None, low=None):
    """{domain: {"days", "low", "why", ...}} for the plan in force against the state."""
    costs = costs or Costs()
    low = dict(LOW, **(low or {}))
    return {"research": research(plan, snap, costs, low), "focus": focus(plan, snap, costs, low),
            "politics": politics(plan, snap, executor, low), "production": production(plan, snap, low),
            "construction": construction(plan, snap, low), "army": army(plan, snap)}


def in_time(year, day):
    """True when a tech of start `year` costs no ahead-of-time penalty on game day `day`."""
    return year is None or day is None or year <= year_of(day)
