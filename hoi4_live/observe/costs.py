"""The cost meter and the tool coverage table of a run (file reads only).

COSTS.md
  model tokens and money by role: the player (its stream-json log), escalations
  (escalations.jsonl), reviews and engineers when they are recorded (reviews.jsonl,
  engineer_costs.jsonl: one JSON line per model call with model, input, output, cache_read,
  cache_write, seconds); game days per real minute by speed; tool latency percentiles and success
  rates by tool; actions per game day; the monitor's own overhead.
COVERAGE.md
  every registered tool: called or not, by whom, succeeded or not, verified by read-back or not,
  first and last use (game date). "Verified" means the game read back the change the order asked
  for (an executor order with status Verified, a player act whose receipt says Verified), which is
  the owner's "used correctly, not merely called".

Prices are list prices per million tokens (Nov 2026) and can be overridden with a JSON file in
MONITOR_PRICES ({"model": {"in":..,"out":..,"cache_read":..,"cache_write_5m":..,"cache_write_1h":..}}).
"""
from __future__ import annotations

import json
import os
from collections import Counter, defaultdict

from .runlogs import FINAL, body_date, game_days

PRICES = {
    # model: in, out, cache read, cache write 5m, cache write 1h ($ per million tokens)
    "claude-haiku-5-5": {"in": 0.10, "out": 0.50, "cache_read": 0.01, "cache_write_5m": 0.125,
                         "cache_write_1h": 0.20, "long_over": 100_000,
                         "long": {"in": 0.50, "out": 2.50, "cache_read": 0.05, "cache_write_5m": 0.625,
                                  "cache_write_1h": 1.00}},
    "claude-sonnet-5-5": {"in": 2.0, "out": 10.0, "cache_read": 0.10, "cache_write_5m": 2.5, "cache_write_1h": 4.0},
    "claude-opus-5-5": {"in": 4.0, "out": 20.0, "cache_read": 0.20, "cache_write_5m": 5.0, "cache_write_1h": 8.0},
}


def prices():
    p = dict(PRICES)
    path = os.environ.get("MONITOR_PRICES")
    if path:
        try:
            with open(path, encoding="utf-8") as f:
                p.update(json.load(f))
        except (OSError, ValueError):
            pass
    return p


def price_of(model, usage, table=None):
    """Dollars for one model call's usage (None when the model has no price)."""
    table = table or prices()
    pr = None
    for name, row in table.items():
        if model and str(model).startswith(name):
            pr = row
            break
    if pr is None:
        return None
    cin = usage.get("input_tokens") or 0
    cread = usage.get("cache_read_input_tokens") or 0
    cwrite = usage.get("cache_creation_input_tokens") or 0
    split = usage.get("cache_creation") or {}
    w1h = split.get("ephemeral_1h_input_tokens") or 0
    w5m = split.get("ephemeral_5m_input_tokens") or (cwrite - w1h)
    if pr.get("long_over") and cin + cread + cwrite > pr["long_over"]:
        pr = dict(pr, **pr["long"])
    out = usage.get("output_tokens") or 0
    return (cin * pr["in"] + out * pr["out"] + cread * pr["cache_read"] + w5m * pr["cache_write_5m"]
            + w1h * pr["cache_write_1h"]) / 1e6


def _jsonl(path):
    try:
        with open(path, encoding="utf-8", errors="replace") as f:
            for line in f:
                line = line.strip()
                if line.startswith("{"):
                    try:
                        yield json.loads(line)
                    except ValueError:
                        continue
    except OSError:
        return


def player_usage(run_dir, name="player.out"):
    """Unique model messages of a stream-json log: {model: {tokens..., calls, usd}}."""
    msgs = {}
    for d in _jsonl(os.path.join(run_dir, name)):
        if d.get("type") != "assistant":
            continue
        m = d.get("message") or {}
        if m.get("id"):
            msgs[m["id"]] = (m.get("model"), m.get("usage") or {})
    out = defaultdict(lambda: Counter())
    table = prices()
    for model, u in msgs.values():
        row = out[model or "?"]
        row["calls"] += 1
        for k in ("input_tokens", "output_tokens", "cache_read_input_tokens", "cache_creation_input_tokens"):
            row[k] += u.get(k) or 0
        usd = price_of(model, u, table)
        if usd is not None:
            row["usd_micro"] += int(usd * 1e6)
    return out


def recorded_usage(path):
    out = defaultdict(lambda: Counter())
    table = prices()
    for r in _jsonl(path):
        row = out[(r.get("role") or "?", r.get("model") or "?")]
        row["calls"] += 1
        u = {"input_tokens": r.get("input"), "output_tokens": r.get("output"),
             "cache_read_input_tokens": r.get("cache_read"), "cache_creation_input_tokens": r.get("cache_write")}
        for k, v in u.items():
            row[k] += v or 0
        row["seconds"] += r.get("seconds") or 0
        usd = r.get("usd")
        if usd is None:
            usd = price_of(r.get("model"), u, table)
        row["usd_micro"] += int((usd or 0) * 1e6)
    return out


def _pct(values, p):
    if not values:
        return None
    v = sorted(values)
    return v[min(len(v) - 1, int(round(p / 100 * (len(v) - 1))))]


def _snapshots(run_dir):
    import glob
    out = []
    for path in sorted(glob.glob(os.path.join(run_dir, "state", "*.json"))):
        try:
            with open(path, encoding="utf-8") as f:
                s = json.load(f)
        except (OSError, ValueError):
            continue
        out.append({k: s.get(k) for k in ("utc", "date", "speed", "paused")})
    return out


def measure(run_dir, monitor_dir=None):
    monitor_dir = monitor_dir or run_dir
    calls = list(_jsonl(os.path.join(run_dir, "calls.jsonl")))
    actions = [a for a in _jsonl(os.path.join(run_dir, "game-actions.jsonl")) if a.get("status") in FINAL]
    server = []
    try:
        with open(os.path.join(run_dir, "mcp.stderr"), encoding="utf-8", errors="replace") as f:
            for line in f:
                if line.startswith("[action] {"):
                    try:
                        server.append(json.loads(line[9:]))
                    except ValueError:
                        pass
    except OSError:
        pass
    # time: game days per real minute by speed
    points = [s for s in _snapshots(run_dir) if s.get("utc") and s.get("date")]
    for c in calls:
        b = c.get("body")
        d = body_date(b)
        if d and c.get("utc") and isinstance(b, dict):
            now = b.get("now") if isinstance(b.get("now"), dict) else b
            points.append({"utc": c["utc"], "date": d, "speed": now.get("speed"), "paused": now.get("paused")})
    points.sort(key=lambda s: s["utc"])
    by_speed = defaultdict(lambda: [0.0, 0.0])
    for a, b in zip(points, points[1:]):
        if a.get("paused") or b.get("paused"):
            continue
        dd = (game_days(b["date"]) or 0) - (game_days(a["date"]) or 0)
        dt = (b["utc"] - a["utc"]) / 60
        if dd < 0 or dt <= 0 or dt > 30:
            continue
        sp = a.get("speed")
        by_speed[str(sp if sp is not None else "?")][0] += dd
        by_speed[str(sp if sp is not None else "?")][1] += dt
    span = None
    dated = [p for p in points if game_days(p["date"]) is not None]
    if dated:
        span = (game_days(dated[0]["date"]), game_days(dated[-1]["date"]), dated[0]["date"], dated[-1]["date"],
                (dated[-1]["utc"] - dated[0]["utc"]) / 60)
    # tools
    lat = defaultdict(list)
    ok = defaultdict(Counter)
    for c in calls:
        t = c.get("tool") or "?"
        if c.get("ms") is not None:
            lat[t].append(float(c["ms"]))
        bad = c.get("isError") or (isinstance(c.get("body"), dict) and c["body"].get("error"))
        ok[t]["fail" if bad else "ok"] += 1
    for a in server:
        if not a.get("executor"):
            continue
        t = a.get("tool") or "?"
        if a.get("ms") is not None:
            lat[t].append(float(a["ms"]))
        ok[t]["ok" if a.get("ok") else "fail"] += 1
    acts = Counter(a.get("status") for a in actions)
    player_acts = sum(1 for c in calls if c.get("source") == "player" and isinstance(c.get("args"), dict)
                      and c["args"].get("request_id"))
    stats = {}
    try:
        with open(os.path.join(monitor_dir, "monitor_stats.json"), encoding="utf-8") as f:
            stats = json.load(f)
    except (OSError, ValueError):
        pass
    return {"calls": calls, "actions": actions, "server": server, "by_speed": by_speed, "span": span,
            "latency": lat, "success": ok, "acts": acts, "player_acts": player_acts, "monitor": stats}


def render_costs(run_dir, monitor_dir=None, m=None):
    monitor_dir = monitor_dir or run_dir
    m = m or measure(run_dir, monitor_dir)
    span = m["span"]
    years = ((span[1] - span[0]) / 365) if span and span[1] > span[0] else None
    lines = [f"# Costs of {os.path.basename(os.path.abspath(run_dir))}", ""]
    if span:
        lines.append(f"Game {span[2]} .. {span[3]} ({span[1] - span[0]:.0f} game days) in {span[4]:.0f} real minutes")
    lines += ["", "## Model tokens and money by role", "",
              "| role | model | calls | input | output | cache read | cache write | $ | $ per game year |",
              "|---|---|---|---|---|---|---|---|---|"]
    total = 0.0

    def row(role, model, r):
        nonlocal total
        usd = r["usd_micro"] / 1e6
        total += usd
        per = f"{usd / years:.2f}" if years else "-"
        lines.append(f"| {role} | {model} | {r['calls']} | {r['input_tokens']:,} | {r['output_tokens']:,} | "
                     f"{r['cache_read_input_tokens']:,} | {r['cache_creation_input_tokens']:,} | {usd:.3f} | {per} |")

    for model, r in player_usage(run_dir).items():
        row("player", model, r)
    for name in ("escalation_costs.jsonl", "reviews.jsonl", "engineer_costs.jsonl"):
        for (role, model), r in recorded_usage(os.path.join(monitor_dir, name)).items():
            row(role, model, r)
    esc = list(_jsonl(os.path.join(monitor_dir, "escalations.jsonl")))
    n_esc = sum(1 for e in esc if not e.get("skipped"))
    lines.append(f"\nTotal recorded: ${total:.2f}" + (f" (${total / years:.2f} a game year)" if years else ""))
    lines.append(f"Escalations: {n_esc} built ({sum(1 for e in esc if e.get('skipped'))} over budget), "
                 f"{sum(1 for e in esc if e.get('answered'))} answered by a command; packets "
                 f"~{sum(e.get('packet_tokens_est') or 0 for e in esc):,} tokens in all")
    lines += ["", "## Game time per real minute", "", "| speed | game days | real minutes | game days a minute |",
              "|---|---|---|---|"]
    for sp, (dd, dt) in sorted(m["by_speed"].items()):
        lines.append(f"| {sp} | {dd:.1f} | {dt:.1f} | {dd / dt:.2f} |" if dt else f"| {sp} | {dd:.1f} | 0 | - |")
    gd = (span[1] - span[0]) if span else 0
    acts = m["acts"]
    lines += ["", "## Actions", "",
              f"Executor orders: {sum(acts.values())} ({', '.join(f'{k} {v}' for k, v in acts.most_common())})"
              + (f"; {sum(acts.values()) / gd:.2f} a game day" if gd else ""),
              f"Player acts: {m['player_acts']}" + (f"; {m['player_acts'] / gd:.2f} a game day" if gd else "")]
    lines += ["", "## Tools (latency ms p50 / p90 / p99, success)", "",
              "| tool | calls | p50 | p90 | p99 | success |", "|---|---|---|---|---|---|"]
    for t in sorted(m["success"], key=lambda t: -sum(m["success"][t].values()))[:40]:
        s = m["success"][t]
        n = s["ok"] + s["fail"]
        la = m["latency"].get(t) or []
        lines.append(f"| {t} | {n} | {_fmt(_pct(la, 50))} | {_fmt(_pct(la, 90))} | {_fmt(_pct(la, 99))} | "
                     f"{100 * s['ok'] / n:.0f}% |")
    mon = m["monitor"]
    if mon:
        lines += ["", "## Monitor overhead", "",
                  f"Detector: {mon.get('snapshots', 0)} snapshots, mean {mon.get('ms_per_snapshot_mean', 0):.1f} ms, "
                  f"max {mon.get('ms_per_snapshot_max', 0):.1f} ms; {mon.get('polls', 0)} log polls, mean "
                  f"{mon.get('ms_per_poll_mean', 0):.1f} ms; model calls by the monitor itself: 0 (Tier 0)"]
    return "\n".join(lines) + "\n"


def _fmt(v):
    return "-" if v is None else f"{v:.0f}"


def registered_tools(schema_path=None):
    here = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
    path = schema_path or os.path.join(here, "skills", "hoi4-campaign-playbook", "tools.schema.json")
    try:
        with open(path, encoding="utf-8") as f:
            d = json.load(f)
        t = d.get("tools") if isinstance(d, dict) else d
        return sorted(t) if isinstance(t, (dict, list)) else []
    except (OSError, ValueError):
        return []


def coverage(run_dir, m=None, schema_path=None):
    m = m or measure(run_dir)
    clock = sorted((c["utc"], body_date(c.get("body"))) for c in m["calls"] if c.get("utc") and body_date(c.get("body")))

    def date_at(utc):
        best = None
        for u, d in clock:
            if u > utc:
                break
            best = d
        return best or (clock[0][1] if clock else None)

    rows = defaultdict(lambda: {"by": Counter(), "ok": 0, "fail": 0, "verified": 0, "first": None, "last": None})

    def seen(tool, who, good, utc, verified=False):
        r = rows[tool]
        r["by"][who] += 1
        r["ok" if good else "fail"] += 1
        r["verified"] += 1 if verified else 0
        d = date_at(utc) if utc else None
        if d:
            r["first"] = r["first"] or d
            r["last"] = d

    for c in m["calls"]:
        body = c.get("body") if isinstance(c.get("body"), dict) else {}
        bad = bool(c.get("isError") or body.get("error"))
        rec = body.get("receipt") if isinstance(body.get("receipt"), dict) else {}
        seen(c.get("tool") or "?", c.get("source") or "?", not bad, c.get("utc"), rec.get("status") == "Verified")
    for a in m["server"]:
        if a.get("executor"):
            seen(a.get("tool") or "?", "executor", bool(a.get("ok")), (a.get("unix_ms") or 0) / 1000)
    for a in m["actions"]:
        kind = (a.get("action") or {}).get("kind")
        if kind and a.get("status") == "Verified":
            rows["hoi4_" + kind]["verified"] += 1
    tools = registered_tools(schema_path)
    names = sorted(set(tools) | set(rows))
    called = [t for t in names if rows[t]["ok"] + rows[t]["fail"]]
    lines = [f"# Tool coverage of {os.path.basename(os.path.abspath(run_dir))}", "",
             f"{len(called)} of {len(tools) or len(names)} registered tools called; "
             f"{sum(1 for t in called if rows[t]['ok'])} succeeded at least once; "
             f"{sum(1 for t in called if rows[t]['verified'])} verified by read-back at least once", "",
             "| tool | called by | ok | failed | verified | first | last |", "|---|---|---|---|---|---|---|"]
    for t in names:
        r = rows[t]
        if not (r["ok"] + r["fail"]):
            lines.append(f"| {t} | NOT CALLED | | | | | |")
            continue
        by = ", ".join(f"{k} {v}" for k, v in r["by"].most_common())
        lines.append(f"| {t} | {by} | {r['ok']} | {r['fail']} | {r['verified']} | {r['first'] or '-'} | "
                     f"{r['last'] or '-'} |")
    return "\n".join(lines) + "\n"


def tool_history(run_dirs):
    """Per tool ok/fail over earlier runs (the detector's 'below its history' rule)."""
    out = defaultdict(lambda: {"ok": 0, "fail": 0})
    for rd in run_dirs:
        for c in _jsonl(os.path.join(rd, "calls.jsonl")):
            bad = c.get("isError") or (isinstance(c.get("body"), dict) and c["body"].get("error"))
            out[c.get("tool") or "?"]["fail" if bad else "ok"] += 1
    return dict(out)
