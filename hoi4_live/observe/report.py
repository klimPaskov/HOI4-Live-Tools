"""The programme report's measured sections, filled from runs.

A report is Markdown written by people, with generated blocks between markers:

    <!-- auto:coverage -->  ...  <!-- /auto:coverage -->

`fill(report_path, runs, store)` rewrites only the text between each pair of markers and leaves
everything else as written. Blocks: runs, coverage, issues, monitoring, lessons, costs.
"""
from __future__ import annotations

import json
import os
import re
from collections import Counter, defaultdict

from . import costs
from .lessons import LessonStore

BLOCKS = ("runs", "coverage", "issues", "monitoring", "lessons", "costs")


def _jsonl(path):
    try:
        with open(path, encoding="utf-8") as f:
            for line in f:
                try:
                    yield json.loads(line)
                except ValueError:
                    pass
    except OSError:
        return


def _load(path, default=None):
    try:
        with open(path, encoding="utf-8") as f:
            return json.load(f)
    except (OSError, ValueError):
        return default


def _name(run):
    return os.path.basename(os.path.abspath(run))


def runs_block(runs, measured):
    lines = ["| run | game dates | real minutes | wars declared | capitulations | executor orders verified/refused |",
             "|---|---|---|---|---|---|"]
    for run in runs:
        m = measured[run]
        span = m["span"]
        gates = Counter(e.get("kind") for e in (_load(os.path.join(run, "monitor_stats.json"), {}) or {})
                        .get("gates", []))
        lines.append(f"| {_name(run)} | {span[2][:10] + '-' + span[3][:10] if span else '-'} | "
                     f"{span[4]:.0f} | {gates.get('war_declared', 0)} | {gates.get('capitulated', 0)} | "
                     f"{m['acts'].get('Verified', 0)}/{m['acts'].get('NotApplied', 0)} |"
                     if span else f"| {_name(run)} | - | - | - | - | - |")
    return "\n".join(lines)


def coverage_block(runs, measured):
    tools = costs.registered_tools()
    seen = defaultdict(Counter)
    for run in runs:
        for t, c in measured[run]["success"].items():
            seen[t].update(c)
    called = [t for t in tools if seen.get(t)]
    never = [t for t in tools if not seen.get(t)]
    failing = sorted(((t, c["fail"], c["ok"] + c["fail"]) for t, c in seen.items() if c["fail"]),
                     key=lambda r: -r[1] / max(1, r[2]))
    lines = [f"Registered tools: {len(tools)}; called in these runs: {len(called)}; never called: {len(never)}.", "",
             "Highest failure shares:", "", "| tool | failed | calls |", "|---|---|---|"]
    lines += [f"| {t} | {f} | {n} |" for t, f, n in failing[:15]]
    if never:
        lines += ["", "Never called: " + ", ".join(never)]
    return "\n".join(lines)


def issues_block(runs):
    lines = ["| run | technical | tool use | duplicates | idling | gameplay | overdue | escalated |",
             "|---|---|---|---|---|---|---|---|"]
    for run in runs:
        first, ev = {}, Counter()
        for e in _jsonl(os.path.join(run, "issues.jsonl")):
            ev[e.get("event")] += 1
            if e.get("event") == "open":
                first.setdefault(e.get("key"), e.get("class"))
        cls = Counter(first.values())
        lines.append(f"| {_name(run)} | {cls['technical']} | {cls['tool_use']} | {cls['duplicate']} | {cls['idling']} "
                     f"| {cls['gameplay']} | {ev['overdue']} | {ev['escalated']} |")
    return "\n".join(lines)


def monitoring_block(runs):
    lines = ["| run | snapshots | ms a snapshot | escalations (skipped) | packet chars mean | gate packets |",
             "|---|---|---|---|---|---|"]
    for run in runs:
        st = _load(os.path.join(run, "monitor_stats.json"), {}) or {}
        esc = list(_jsonl(os.path.join(run, "escalations.jsonl")))
        sent = [e for e in esc if not e.get("skipped")]
        chars = [e.get("packet_chars") or 0 for e in sent]
        lines.append(f"| {_name(run)} | {st.get('snapshots', 0)} | {st.get('ms_per_snapshot_mean', 0):.1f} | "
                     f"{len(sent)} ({len(esc) - len(sent)}) | {sum(chars) / len(chars) if chars else 0:.0f} | "
                     f"{len(st.get('gates') or [])} |")
    return "\n".join(lines)


def lessons_block(store):
    rows = LessonStore(store).all() if store else []
    by = Counter((r.get("source", "?"), r.get("status", "?")) for r in rows)
    used = sum(1 for r in rows if r.get("confirmations"))
    lines = [f"Lessons: {len(rows)} ({', '.join(f'{s} {st}: {n}' for (s, st), n in sorted(by.items()))}); "
             f"confirmed by a later occurrence: {used}; executor advice: "
             f"{sum(1 for r in rows if r.get('executor'))}; plan lints: {sum(1 for r in rows if r.get('plan_lint'))}."]
    top = sorted(rows, key=lambda r: -r.get("confidence", 0))[:8]
    lines += [f"- {r['id']} ({r.get('confidence', 0):.2f}): {r.get('statement', '')[:160]}" for r in top]
    return "\n".join(lines)


def costs_block(runs, measured):
    lines = ["| run | player model calls | player USD | game days per real minute (speed: value) | executor orders a game day |",
             "|---|---|---|---|---|"]
    for run in runs:
        m = measured[run]
        try:
            usage = costs.player_usage(run)
        except Exception:
            usage = {}
        usd = sum(r.get("usd_micro", 0) for r in usage.values()) / 1e6
        n = sum(r.get("calls", 0) for r in usage.values())
        speeds = ", ".join(f"{s}: {d / t:.2f}" for s, (d, t) in sorted(m["by_speed"].items()) if t > 0)
        span = m["span"]
        days = (span[1] - span[0]) if span else 0
        per_day = sum(m["acts"].values()) / days if days else 0
        lines.append(f"| {_name(run)} | {n} | {usd:.2f} | {speeds or '-'} | {per_day:.1f} |")
    return "\n".join(lines)


def fill(report_path, runs, store=None):
    with open(report_path, encoding="utf-8") as f:
        text = f.read()
    measured = {run: costs.measure(run) for run in runs}
    made = {
        "runs": lambda: runs_block(runs, measured),
        "coverage": lambda: coverage_block(runs, measured),
        "issues": lambda: issues_block(runs),
        "monitoring": lambda: monitoring_block(runs),
        "lessons": lambda: lessons_block(store),
        "costs": lambda: costs_block(runs, measured),
    }
    for name in BLOCKS:
        pat = re.compile(rf"(<!-- auto:{name} -->).*?(<!-- /auto:{name} -->)", re.S)
        if pat.search(text):
            body = made[name]()
            text = pat.sub(lambda m: m.group(1) + "\n" + body + "\n" + m.group(2), text)
    tmp = report_path + ".tmp"
    with open(tmp, "w", encoding="utf-8") as f:
        f.write(text)
    os.replace(tmp, report_path)
    return text
