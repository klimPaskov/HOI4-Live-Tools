"""One quantity over time from the snapshots of a run.

A path walks the snapshot: dots between keys, `*` for every key of a mapping
(one series each), `[key=value]` to pick list rows, `#` for a list's length:

  politics.political_power
  air.regions.*.aircraft                  planes per air region
  wars.per_enemy.HOL.vp_held_by_us        victory points of one enemy we hold
  stacking.provinces[province=516].divisions
  land.armies[army=Armee Nord].supply_mean
  air.wings_without_mission.#

Short names: planes, pp, vp, supply, stacking (see ALIASES).
"""
from __future__ import annotations

import json
import os

ALIASES = {
    "planes": "air.regions.*.aircraft",
    "pp": "politics.political_power",
    "vp": "wars.per_enemy.*.vp_held_by_us",
    "casualties": "wars.per_enemy.*.casualties_own",
    "supply": "land.armies.*.supply_mean",
    "planning": "land.armies.*.planning.mean",
    "stacking": "stacking.provinces.0.divisions",
    "factories": "economy.factories.*",
    "findings": "findings.#",
}


def _split(path):
    out, buf, depth = [], "", 0
    for ch in path:
        if ch == "[":
            depth += 1
        elif ch == "]":
            depth -= 1
        if ch == "." and depth == 0:
            out.append(buf)
            buf = ""
        else:
            buf += ch
    if buf:
        out.append(buf)
    return out


def _step(value, part):
    """-> list of (label, value)."""
    filt = None
    if "[" in part and part.endswith("]"):
        part, filt = part[:part.index("[")], part[part.index("[") + 1:-1]
    if part == "#":
        return [("", len(value) if isinstance(value, (list, dict)) else None)]
    if part == "*":
        if isinstance(value, dict):
            return [(str(k), v) for k, v in value.items()]
        if isinstance(value, list):
            return [(str(i), v) for i, v in enumerate(value)]
        return []
    if part:
        if isinstance(value, dict):
            value = value.get(part)
        elif isinstance(value, list) and part.lstrip("-").isdigit():
            i = int(part)
            value = value[i] if -len(value) <= i < len(value) else None
        else:
            value = None
    if filt is not None:
        k, _, want = filt.partition("=")
        rows = value if isinstance(value, list) else []
        hits = [r for r in rows if isinstance(r, dict) and str(r.get(k)) == want]
        return [("", hits[0] if hits else None)]
    return [("", value)]


def extract(snap, path):
    """{series label: value} for one snapshot."""
    cur = [("", snap)]
    for part in _split(ALIASES.get(path, path)):
        nxt = []
        for label, v in cur:
            for sub, w in _step(v, part):
                nxt.append(((label + "/" + sub).strip("/") if sub else label, w))
        cur = nxt
    return {label or "value": v for label, v in cur}


def load_run(run_dir):
    """Every snapshot of a run, oldest first (state/<game date>.json)."""
    folder = os.path.join(run_dir, "state")
    try:
        names = sorted(n for n in os.listdir(folder) if n.endswith(".json"))
    except OSError:
        return []
    out = []
    for n in names:
        try:
            with open(os.path.join(folder, n)) as f:
                s = json.load(f)
        except (OSError, ValueError):
            continue
        s["_file"] = n
        out.append(s)
    return out


def series(snaps, path):
    rows = []
    labels = []
    for s in snaps:
        vals = extract(s, path)
        for k in vals:
            if k not in labels:
                labels.append(k)
        rows.append((s.get("date"), s.get("trigger"), vals))
    return labels, rows


def render(snaps, path, changes_only=False):
    labels, rows = series(snaps, path)
    if not rows:
        return "no snapshots\n"

    def fmt(v):
        if isinstance(v, float):
            return f"{v:.2f}".rstrip("0").rstrip(".")
        if isinstance(v, (dict, list)):
            return json.dumps(v, ensure_ascii=False)[:40]
        return "-" if v is None else str(v)

    names = {}
    for s in snaps:
        for rid, r in ((s.get("air") or {}).get("regions") or {}).items():
            names[rid] = r.get("name")
    head = ["date"] + [f"{lab}:{names[lab]}" if lab in names and path in ("planes", "air.regions.*.aircraft")
                       else lab for lab in labels]
    out = ["\t".join(head)]
    last = None
    for date, trig, vals in rows:
        cells = [fmt(vals.get(lab)) for lab in labels]
        if changes_only and cells == last:
            continue
        last = cells
        out.append("\t".join([str(date) + ("" if trig in (None, "week") else f" ({trig})")] + cells))
    return "\n".join(out) + "\n"
