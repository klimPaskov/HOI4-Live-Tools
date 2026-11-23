"""CLEANLINESS per game year of a run: what the executor did twice, what it retried unchanged
after a refusal, how long it stood still while something was assignable, how the player spent
its calls, how often the game speed changed, and what each action kind costs per element
(calls, elements changed, wall seconds, seconds per element: the batching measure).

Sources (all in the run directory, read only):
  game-actions.jsonl  the executor's actions (one record per status: Dispatching, then the final
                      Verified / NotApplied / ...), wall-clock unix_ms;
  calls.jsonl         every tool call with its source (player, monitor, recorder) and wall-clock
                      utc; overview bodies carry the game date;
  state/*.json        the recorder's snapshots (game date, utc, speed, idle factories and
                      research slots, omissions).
Game dates for wall-clock times come from the snapshots and the overview reads (the latest date
read at or before the time).
"""
from __future__ import annotations

import bisect
import json
import os
from collections import Counter, defaultdict

FINAL = ("Verified", "NotApplied", "Failed", "Refused", "Rejected", "Error")
# tools that only read or wait (everything else with a request is an act)
WAIT_TOOLS = ("hoi4_wait", "hoi4_wait_change", "hoi4_wait_until", "hoi4_sleep")


def _jsonl(path):
    out = []
    try:
        with open(path, encoding="utf-8") as fh:
            for line in fh:
                line = line.strip()
                if line:
                    try:
                        out.append(json.loads(line))
                    except ValueError:
                        continue
    except OSError:
        pass
    return out


def _clock(calls, snaps):
    """Sorted (utc, game date text) pairs."""
    pairs = []
    for s in snaps:
        if s.get("utc") is not None and s.get("date"):
            pairs.append((float(s["utc"]), str(s["date"])))
    for c in calls:
        body = c.get("body")
        if c.get("tool") == "hoi4_overview" and isinstance(body, dict) and c.get("utc"):
            text = (body.get("date") or {}).get("text")
            if text:
                pairs.append((float(c["utc"]), str(text)))
    pairs.sort()
    return pairs


def _date_at(clock, utc):
    i = bisect.bisect_right([p[0] for p in clock], utc) - 1
    return clock[i][1] if i >= 0 else (clock[0][1] if clock else "")


def _days(date_text):
    """Game date text Y.M.D[.H] -> days (365-day years, the game's calendar)."""
    parts = [int(p) for p in date_text.split(".")[:4]] + [0, 0, 0, 0]
    y, m, d, h = parts[:4]
    cum = [0, 31, 59, 90, 120, 151, 181, 212, 243, 273, 304, 334]
    return y * 365 + cum[max(1, min(12, m)) - 1] + (d - 1) + h / 24.0


def _action_key(action):
    a = {k: v for k, v in (action or {}).items() if k not in ("request_id", "lease")}
    return json.dumps(a, sort_keys=True)[:160]


def _assignable(snap):
    """What a snapshot shows assignable now: {domain: short text} (domain = the action kind
    prefix that would answer it; "" = any action)."""
    out = {}
    fac = ((snap.get("economy") or {}).get("factories") or {}).get("military")
    used = (snap.get("production") or {}).get("military_assigned")
    if isinstance(fac, (int, float)) and isinstance(used, (int, float)) and fac - used >= 1:
        out["production"] = f"{int(fac - used)} mil factories idle"
    idle = (snap.get("research") or {}).get("idle_slots")
    if isinstance(idle, (int, float)) and idle > 0:
        out["research"] = f"{int(idle)} research slots idle"
    rows = [
        o for o in (snap.get("omissions") or [])
        if isinstance(o, dict) and not str(o.get("item", "")).startswith("frame_")
        and o.get("owner_note") is None
    ]
    if rows:
        out[""] = f"{len(rows)} omissions ({', '.join(str(o.get('item')) for o in rows[:4])})"
    return out


# the list arguments that name the elements one action acts on (wings, divisions, states, ...)
ELEMENT_KEYS = ("wings", "divisions", "states", "lines", "armies", "provinces", "traits",
                "trait_indices", "items", "orders")


def _elements(action):
    """How many elements an action names (1 when it names no list)."""
    for key in ELEMENT_KEYS:
        v = (action or {}).get(key)
        if isinstance(v, list):
            return len(v)
    return 1


def _changed(final, asked):
    """Elements a final receipt shows changed: the receipt's own count (`changed`, or the
    length of an `unchanged` list taken from the asked count) when it gives one; else all asked
    when Verified, none otherwise."""
    detail = final.get("detail")
    if isinstance(detail, dict):
        if isinstance(detail.get("changed"), int):
            return max(0, detail["changed"])
        if isinstance(detail.get("unchanged"), list):
            return max(0, asked - len(detail["unchanged"]))
    return asked if final.get("status") == "Verified" else 0


def per_kind(actions):
    """Per action kind: calls, elements asked, elements changed, wall seconds (Dispatching to
    the final receipt) and seconds per changed element. Returns {kind: dict}."""
    started = {}
    out = defaultdict(lambda: {"calls": 0, "elements": 0, "changed": 0, "seconds": 0.0})
    for a in sorted(actions, key=lambda a: a.get("unix_ms") or 0):
        rid = a.get("id")
        status = a.get("status")
        if status == "Dispatching":
            started.setdefault(rid, a)
            continue
        if status not in FINAL and status != "Diverged":
            continue
        first = started.pop(rid, None)
        action = (first or a).get("action") or {}
        kind = str(action.get("kind", "")) or "?"
        k = out[kind]
        asked = _elements(action)
        k["calls"] += 1
        k["elements"] += asked
        k["changed"] += _changed(a, asked)
        if first is not None and a.get("unix_ms") and first.get("unix_ms"):
            k["seconds"] += max(0.0, (a["unix_ms"] - first["unix_ms"]) / 1000.0)
    for k in out.values():
        k["per_element"] = k["seconds"] / k["changed"] if k["changed"] else None
    return dict(out)


def render_per_kind(kinds, top=16):
    """The per-action-kind table, costliest kinds first."""
    if not kinds:
        return []
    lines = ["  per action kind (calls, elements asked, elements changed, wall s, s per changed element):"]
    for kind, k in sorted(kinds.items(), key=lambda kv: -kv[1]["seconds"])[:top]:
        per = f"{k['per_element']:.1f}" if k["per_element"] is not None else "-"
        lines.append(
            f"    {kind}: {k['calls']} calls, {k['elements']} asked, {k['changed']} changed, "
            f"{k['seconds']:.0f} s, {per} s/element"
        )
    return lines


def measure(run_dir, snaps):
    """Per game year: a dict of the measures (see the module text)."""
    actions = _jsonl(os.path.join(run_dir, "game-actions.jsonl"))
    calls = _jsonl(os.path.join(run_dir, "calls.jsonl"))
    clock = _clock(calls, snaps)
    years = defaultdict(lambda: {
        "acts": 0, "refused": 0, "repeats": Counter(), "retried": Counter(),
        "idle_days": 0.0, "idle_longest": (0.0, ""), "player": Counter(), "speed_changes": 0,
    })
    finals = [a for a in actions if a.get("status") in FINAL]
    finals.sort(key=lambda a: a.get("unix_ms") or 0)
    seen = Counter()
    refused_before = set()
    act_utc = []
    for a in finals:
        utc = (a.get("unix_ms") or 0) / 1000.0
        act_utc.append(utc)
        year = _date_at(clock, utc)[:4] or "?"
        y = years[year]
        key = _action_key(a.get("action"))
        y["acts"] += 1
        seen[key] += 1
        if seen[key] > 1:
            y["repeats"][key] += 1
        if key in refused_before:
            y["retried"][key] += 1
        if a.get("status") != "Verified":
            y["refused"] += 1
            refused_before.add(key)
        else:
            refused_before.discard(key)
        kind = str((a.get("action") or {}).get("kind", ""))
        if "speed" in kind or kind in ("time", "time_set"):
            y["speed_changes"] += 1
    # speed changes: the clock ledger (the executor's ids start with "plan-"; others are the
    # harness's or a human's)
    for c in _jsonl(os.path.join(run_dir, "clock-actions.jsonl")):
        if c.get("status") != "Verified" or (c.get("action") or {}).get("kind") != "SetSpeed":
            continue
        utc = (c.get("unix_ms") or 0) / 1000.0
        year = (_date_at(clock, utc) if utc else (clock[-1][1] if clock else ""))[:4] or "?"
        if str(c.get("id", "")).startswith("plan-"):
            years[year]["speed_changes"] += 1
        else:
            years[year]["speed_others"] = years[year].get("speed_others", 0) + 1
    act_utc.sort()
    by_kind = defaultdict(list)
    for a in finals:
        kind = str((a.get("action") or {}).get("kind", ""))
        by_kind[kind.split("_")[0]].append((a.get("unix_ms") or 0) / 1000.0)
    # stretches where a domain had something assignable in two snapshots running and no action
    # of that domain came between them
    ordered = sorted((s for s in snaps if s.get("utc") is not None and s.get("date")), key=lambda s: s["utc"])
    for prev, cur in zip(ordered, ordered[1:]):
        now_open = _assignable(cur)
        for domain, what in _assignable(prev).items():
            if domain not in now_open:
                continue
            times = sorted(act_utc if not domain else by_kind.get(domain, []))
            lo = bisect.bisect_right(times, float(prev["utc"]))
            hi = bisect.bisect_right(times, float(cur["utc"]))
            if hi > lo:
                continue
            days = _days(cur["date"]) - _days(prev["date"])
            y = years[str(prev["date"])[:4]]
            y["idle_days"] += days
            if days > y["idle_longest"][0]:
                y["idle_longest"] = (days, f"{prev['date']}..{cur['date']}: {what}")
    # observed speed changes (the recorder's reads), when the executor's ledger has none
    observed = Counter()
    for prev, cur in zip(ordered, ordered[1:]):
        if prev.get("speed") != cur.get("speed") and not cur.get("paused") and not prev.get("paused"):
            observed[str(cur["date"])[:4]] += 1
    for year, n in observed.items():
        years[year]["speed_seen"] = n
    for c in calls:
        if c.get("source") != "player":
            continue
        year = _date_at(clock, float(c.get("utc") or 0))[:4] or "?"
        tool = str(c.get("tool", ""))
        args = c.get("args") or {}
        if tool in WAIT_TOOLS or tool.startswith("hoi4_wait"):
            kind = "waits"
        elif isinstance(args, dict) and args.get("request_id"):
            kind = "acts"
        else:
            kind = "reads"
        years[year]["player"][kind] += 1
        if tool == "hoi4_time" and isinstance(args, dict) and "speed" in args:
            years[year]["player"]["speed sets"] += 1
    return dict(years)


def render(run_dir, snaps):
    years = measure(run_dir, snaps)
    if not years:
        return ""
    lines = ["", "CLEANLINESS (per game year; executor ledger game-actions.jsonl, calls.jsonl, snapshots)"]
    lines += render_per_kind(per_kind(_jsonl(os.path.join(run_dir, "game-actions.jsonl"))))
    for year in sorted(years):
        y = years[year]
        reps = sum(y["repeats"].values())
        retr = sum(y["retried"].values())
        lines.append(
            f"  {year}: {y['acts']} actions, {y['refused']} refused; identical repeated {reps}; "
            f"refused then retried unchanged {retr}; domain-days still with something assignable "
            f"{y['idle_days']:.0f}; speed changes {y['speed_changes']} by the executor"
            + (f", {y['speed_others']} by others (harness or human)" if y.get("speed_others") else "")
            + (f", {y['speed_seen']} seen" if y.get("speed_seen") else "")
            + (" OVER THE BUDGET (6 a year outside war starts and ends: check)" if y["speed_changes"] > 8 else "")
        )
        for key, n in y["retried"].most_common(3):
            lines.append(f"    retried after a refusal x{n}: {key}")
        for key, n in y["repeats"].most_common(3):
            lines.append(f"    repeated x{n + 1}: {key}")
        if y["idle_longest"][0] > 0:
            lines.append(f"    longest still stretch {y['idle_longest'][0]:.0f} d: {y['idle_longest'][1]}")
        p = y["player"]
        if p:
            total = sum(v for k, v in p.items() if k in ("waits", "reads", "acts")) or 1
            lines.append(
                f"    player calls: waits {p['waits']} ({100 * p['waits'] // total}%), reads "
                f"{p['reads']} ({100 * p['reads'] // total}%), acts {p['acts']} "
                f"({100 * p['acts'] // total}%)"
                + (f", speed sets {p['speed sets']}" if p.get("speed sets") else "")
            )
    return "\n".join(lines) + "\n"
