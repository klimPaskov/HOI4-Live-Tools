"""The plan's horizons against the live state: which horizon is in force, when it ends, and how far its
numeric targets are from what the game shows.

A plan's `horizons` are [{"id", "from", "until", "intent", "targets": {...}}]: the first is the opening's
detailed horizon, later ones carry intent and targets only. Near a horizon's end the planner extends the
detail into the next one (trigger `horizon_end:<id>`, a schedule-class update over every domain); at the end
the targets are compared with the state, and a horizon that ends behind raises `deviation:horizon:<id>`.

Targets the planner can measure from its reads: divisions (the armies' division counts), pp_reserve (political
power), vp_held (enemy victory points the player controls), states_held (state ids the player controls),
equipment_fill (the armies' mean fill when the feed reported it). military_factories, civilian_factories,
dockyards and manpower are carried for the model and the report; they are compared only when a snapshot holds
them (`factories`, `manpower`).
"""

from .state import day_of

LEAD_DAYS = 21          # extend the detail this many game days before the horizon in force ends
BEHIND = 0.9            # a target counts as missed below this share of it
MEASURED = ("divisions", "pp_reserve", "vp_held", "states_held", "equipment_fill", "military_factories",
            "civilian_factories", "dockyards", "manpower")


def horizons(plan):
    """[(from day or None, until day, horizon)] in plan order, skipping malformed entries."""
    out = []
    for h in plan.get("horizons") or []:
        if not isinstance(h, dict) or not h.get("id"):
            continue
        until = day_of(h.get("until"))
        if until is None:
            continue
        out.append((day_of(h.get("from")), until, h))
    return out


def current(plan, day):
    """(index, horizon, days left) of the horizon in force on `day`, else (None, None, None)."""
    prev = None
    for i, (start, until, h) in enumerate(horizons(plan)):
        start = start if start is not None else prev
        if (start is None or day >= start) and day < until:
            return i, h, until - day
        prev = until
    return None, None, None


def measure(snap):
    """{target: value now} for the targets the snapshot can answer."""
    tag = snap.get("player")
    out = {}
    if isinstance(snap.get("pp"), (int, float)):
        out["pp_reserve"] = snap["pp"]
    armies = snap.get("armies") or {}
    counts = [a.get("divisions") for a in armies.values() if isinstance(a.get("divisions"), (int, float))]
    if counts:
        out["divisions"] = sum(counts)
    fills = [a.get("fill") for a in armies.values() if isinstance(a.get("fill"), (int, float))]
    if fills:
        out["equipment_fill"] = round(sum(fills) / len(fills), 3)
    controllers = snap.get("controllers") or {}
    if controllers and tag:
        out["states_held"] = sorted(int(s) for s, c in controllers.items() if c == tag)
    vp = snap.get("vp") or {}
    if vp and tag:
        out["vp_held"] = sum(float(p.get("vp") or 0) for row in vp.values() if row.get("owner") != tag
                             for p in row.get("provinces") or [] if p.get("controller") == tag)
    for k in ("military_factories", "civilian_factories", "dockyards"):
        if isinstance((snap.get("factories") or {}).get(k), (int, float)):
            out[k] = snap["factories"][k]
    if isinstance(snap.get("manpower"), (int, float)):
        out["manpower"] = snap["manpower"]
    return out


def compare(targets, now):
    """[{"target", "want", "now", "behind"}] for each target the state answers."""
    rows = []
    for k in MEASURED:
        if k not in (targets or {}) or k not in now:
            continue
        want, have = targets[k], now[k]
        if k == "states_held":
            missing = sorted(set(want or []) - set(have))
            rows.append({"target": k, "want": len(want or []), "now": len(want or []) - len(missing),
                         "missing": missing[:12], "behind": bool(missing)})
            continue
        if not isinstance(want, (int, float)):
            continue
        rows.append({"target": k, "want": want, "now": have, "behind": have < want * BEHIND})
    return rows


def due(plan, snap, memory):
    """Trigger requests for the horizons: [(id, kind, why, evidence)]. `memory` (a dict kept by the triggers)
    records which horizons were extended and checked, so each fires once."""
    day = snap.get("day")
    if day is None:
        return []
    seen = memory.setdefault("horizons", {})
    out = []
    now = measure(snap)
    items = horizons(plan)
    i, h, left = current(plan, day)
    if h is not None and left <= LEAD_DAYS and i + 1 < len(items) and not seen.get("extend:" + str(h["id"])):
        nxt = items[i + 1][2]
        seen["extend:" + str(h["id"])] = day
        out.append(("horizon_end:%s" % h["id"], "schedule", "horizon %s ends in %d days: detail %s (%s)"
                    % (h["id"], left, nxt.get("id"), nxt.get("intent") or "no intent"),
                    {"horizon": h["id"], "next": nxt.get("id"), "intent": nxt.get("intent"),
                     "targets": nxt.get("targets") or {}}))
    for start, until, past in items:
        key = "check:" + str(past["id"])
        if until > day or seen.get(key):
            continue
        seen[key] = day
        rows = compare(past.get("targets") or {}, now)
        behind = [r for r in rows if r["behind"]]
        seen["results"] = (seen.get("results") or []) + [bool(behind)]
        if behind:
            out.append(("deviation:horizon:%s" % past["id"], "deviation", "horizon %s ended behind its targets: %s"
                        % (past["id"], ", ".join("%s %s of %s" % (r["target"], r["now"], r["want"]) for r in behind)),
                        {"horizon": past["id"], "targets": rows,
                         "behind_in_a_row": _run_length(seen["results"])}))
    return out


def _run_length(results):
    n = 0
    for r in reversed(results):
        if not r:
            break
        n += 1
    return n
