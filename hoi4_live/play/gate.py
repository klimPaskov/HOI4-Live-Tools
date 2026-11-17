"""The campaign gate: where the country stands against the marks it must reach, from one snapshot of the game.

The snapshot is what the supervisor read (Supervisor.snapshot: hoi4_observe, hoi4_economy, hoi4_armies,
hoi4_alerts, hoi4_country_status, hoi4_supply, hoi4_stockpile, hoi4_template_get, hoi4_air, hoi4_fleets,
hoi4_air_regions, hoi4_resources), kept in the run folder at the end of a run (gate/<date>.json) or taken from the
running campaign on demand. Nothing here reads the game.

    marks      civilian and military factories and divisions against marks given by the user, or against the
               game's own AI at that date (a stored baseline, skills/hoi4-campaign-playbook/baselines/TAG-YEAR.json;
               optional: without one the counts are shown without marks)
    equipped   the equipment fill of the fielded divisions (a division count passes only with a mean fill of 0.9
               or better and no army below 0.8), the binding equipment of the main template, stock and cover
    force      divisions by template and by role (from each template's battalions and the game's unit files)
               against the plan's force table (force_design), when it has one
    air        wings by type, planes, bases, missions, superiority over the plan's regions, planes in stock, fuel

Standard library only.
"""

import collections
import datetime
import json
import os
import re

FILL_MEAN, FILL_ARMY = 0.9, 0.8
ROLES = ("armour", "motorised/mechanised", "special forces", "line", "garrison")


def _key(d):
    return tuple(int(x) for x in re.split(r"[.-]", str(d))[:3])


def _days(a, b):
    return (datetime.date(*b) - datetime.date(*a)).days


def baseline_row(path, date):
    """The AI's row at or before the game date from a baseline file; None without one."""
    try:
        with open(path, encoding="utf-8") as f:
            rows = json.load(f).get("rows") or []
    except (OSError, ValueError, AttributeError):
        return None
    rows = [r for r in rows if r.get("date") and _key(r["date"]) <= _key(date)]
    return rows[-1] if rows else None


def find_baseline(app, tag, start=None):
    """The stored AI baseline for the country and start year in the install, or None."""
    folder = os.path.join(app, "skills", "hoi4-campaign-playbook", "baselines")
    year = str(start or "1936")[:4]
    path = os.path.join(folder, "%s-%s.json" % (tag, year))
    return path if tag and os.path.isfile(path) else None


def division_fill(d):
    eq = [e for e in (d.get("equipment") or []) if e.get("need")]
    if not eq:
        return None
    return sum(min(e["have"] / e["need"], 1.0) for e in eq) / len(eq)


def equipped(reads, date=None, target=None, target_row=None):
    """Equipped-strength lines and whether the fill passes. `target`: the date the path is measured to (with the
    AI's division count there, `target_row`, when a baseline has it)."""
    sup = reads.get("hoi4_supply") or {}
    divs = sup.get("divisions") or []
    fills = [(d.get("army") or "(none)", f, d) for d in divs if (f := division_fill(d)) is not None]
    out, ok = [], False
    if fills:
        mean = sum(f for _, f, _ in fills) / len(fills)
        by = {}
        for a, f, _ in fills:
            by.setdefault(a, []).append(f)
        arm = sorted(((sum(v) / len(v), a, len(v)) for a, v in by.items()))
        low = sum(1 for _, f, _ in fills if f < 0.9)
        ok = mean >= FILL_MEAN and arm[0][0] >= FILL_ARMY
        out.append(f"equipment fill of {len(fills)} fielded divisions: mean {mean:.2f}, below 0.9: {low}; "
                   f"worst army {arm[0][1]} {arm[0][0]:.2f} ({arm[0][2]} div): {'ok' if ok else 'BELOW'} "
                   f"(needs mean >= {FILL_MEAN}, every army >= {FILL_ARMY})")
        out.append("  armies: " + ", ".join(f"{a} {m:.2f}" for m, a, _ in arm))
        field = sum(d.get("manpower") or 0 for _, _, d in fills)
        out.append(f"manpower in the field {field / 1e6:.2f}M")
    else:
        out.append("equipment fill: unreadable (hoi4_supply gave no division equipment)")
    need = {}
    for d in divs:
        t = d.get("template")
        if t and t not in need and d.get("equipment"):
            need[t] = {e["archetype"]: e["need"] for e in d["equipment"] if e.get("need")}
    stock = {e["archetype"]: e for e in (reads.get("hoi4_stockpile") or {}).get("equipment") or []
             if isinstance(e, dict) and e.get("archetype")}
    count = collections.Counter(d.get("template") for d in divs)
    main = max(count, key=count.get) if count else None
    if main in need and need[main]:
        rates = []
        for a, n in need[main].items():
            s = stock.get(a, {})
            rates.append(((s.get("produced_per_day") or 0) / n, a, s))
        rates.sort(key=lambda r: r[0])
        out.append(f"main template {main} ({count[main]} fielded): divisions a day the output equips, by archetype: "
                   + ", ".join(f"{a} {p:.3f}" for p, a, _ in rates) + f"; binding {rates[0][1]}")
        if date and target:
            span = _days(_key(date), _key(target))
            if span > 0:
                by = sorted(((s.get("stockpile") or 0) / need[main][a] + p * span, a) for p, a, s in rates)
                more, bind = by[0]
                against = (f"against the AI's {target_row.get('divisions')}" if target_row
                           else "(no AI baseline for this country)")
                out.append(f"path to {target}: stock plus today's output equips {more:.0f} more {main} in {span} days "
                           f"(binding {bind}; " + ", ".join(f"{a} {m:.0f}" for m, a in by) + f"): "
                           f"{len(fills) + more:.0f} divisions {against} "
                           f"(output grows with factories; training lines and their own equipment not counted)")
    for a in sorted({a for n in need.values() for a in n}):
        s = stock.get(a, {})
        out.append(f"  {a}: stock {s.get('stockpile')}, made {s.get('produced_per_day')}/d, used "
                   f"{s.get('consumption_per_day')}/d, cover {s.get('days_of_cover')} d, fielded "
                   f"{s.get('fielded_have')}/{s.get('fielded_need')}, factories {s.get('factories')}")
    return out, ok


def unit_categories(game_dir):
    """Sub-unit -> (group, categories) from the game's common/units (a mod's files read the same way)."""
    out = {}
    folder = os.path.join(game_dir or "", "common", "units")
    for f in sorted(os.listdir(folder)) if game_dir and os.path.isdir(folder) else []:
        if not f.endswith(".txt"):
            continue
        with open(os.path.join(folder, f), encoding="utf-8", errors="replace") as fh:
            s = fh.read()
        for m in re.finditer(r"^\t([A-Za-z_0-9]+) = \{(.*?)^\t\}", s, re.S | re.M):
            g = re.search(r"\bgroup = (\w+)", m.group(2))
            out[m.group(1)] = (g.group(1) if g else "", set(re.findall(r"category_\w+", m.group(2))))
    return out


def role_of(regiments, units):
    """A template's role from its battalions: any tank -> armour; vehicle infantry -> motorised/mechanised;
    special forces or marines -> special forces; only cavalry or militia -> garrison; else line."""
    cats = [units.get(b, ("", set()))[1] for col in regiments or [] for b in col if b]
    every = set().union(*cats) if cats else set()
    if "category_tanks" in every:
        return "armour"
    if "category_vehicle_infantry" in every:
        return "motorised/mechanised"
    if every & {"category_special_forces", "category_marines", "category_paratroopers", "category_mountaineers"}:
        return "special forces"
    if cats and all(c & {"category_cavalry", "category_militia"} for c in cats):
        return "garrison"
    return "line"


def plan_force_table(plan):
    """The plan's force table rows (force_design {land, air, navy} or older force_table / forces), or None."""
    fd = plan.get("force_design")
    if isinstance(fd, dict) and any(isinstance(fd.get(k), list) for k in ("land", "air", "navy")):
        rows = []
        for r in fd.get("land") or []:
            rows.append(dict(r, divisions=r.get("count"), by=r.get("ready"), domain="land"))
        for r in fd.get("air") or []:
            rows.append(dict(r, plane_type=r.get("equipment") or r.get("kind"), planes=r.get("count"),
                             by=r.get("ready"), domain="air", template=None, role=None))
        for r in fd.get("navy") or []:
            rows.append(dict(r, ship_type=r.get("ship_type") or r.get("kind"), ships=r.get("count"),
                             by=r.get("ready"), domain="navy", template=None, role=None))
        return rows or None
    for k in ("force_design", "force_table", "forces"):
        v = plan.get(k)
        if isinstance(v, dict):
            v = v.get("rows") or v.get("table") or [dict(r, key=n) for n, r in v.items() if isinstance(r, dict)]
        if isinstance(v, list) and v:
            return v
    return None


def force(reads, date, plan, units, plan_start="1936.1.1"):
    """Force composition lines and whether it is on the plan's path (None: the plan has no force table)."""
    out = []
    by_t = collections.Counter(d.get("template") or "?" for d in (reads.get("hoi4_supply") or {}).get("divisions")
                               or [])
    temps = {(t.get("name") or t.get("template")): t for t in (reads.get("hoi4_template_get") or {}).get("templates")
             or []}
    role = {n: role_of(t.get("columns") or t.get("regiments"), units) for n, t in temps.items()}
    by_r = collections.Counter()
    for t, n in by_t.items():
        by_r[role.get(t, "line")] += n
    out.append("divisions by template: " + ", ".join(f"{t} {n} ({role.get(t, '?')})" for t, n in by_t.most_common()))
    out.append("divisions by role: " + ", ".join(f"{r} {by_r.get(r, 0)}" for r in ROLES))
    planes, wings = collections.Counter(), collections.Counter()
    for w in (reads.get("hoi4_air") or {}).get("wings") or []:
        k = w.get("aircraft_type") or w.get("unit_type") or "?"
        planes[k] += w.get("aircraft") or 0
        wings[k] += 1
    out.append("air wings by type: " + (", ".join(f"{k} {wings[k]} wings {planes[k]} planes" for k in sorted(planes))
                                        or "none"))
    ships = ((reads.get("hoi4_fleets") or {}).get("totals") or {}).get("ships_by_type") or {}
    out.append("ships by class: " + (", ".join(f"{k} {v}" for k, v in sorted(ships.items())) or "none"))
    table = plan_force_table(plan) if plan else None
    if table is None:
        out.append("force against the plan: the plan has no force table (force_design); composition not gated")
        return out, None
    built = collections.Counter(planes)
    for e in (reads.get("hoi4_stockpile") or {}).get("equipment") or []:
        for v in e.get("variants") or []:
            if "plane" in (v.get("type") or ""):
                built[v["type"]] += int(v.get("amount") or 0)
    have = {"template": by_t, "role": by_r, "plane": built, "ship": collections.Counter(ships)}
    ok, rows, ahead, seen = True, [], [], set()
    start = _key(plan_start)
    for r in sorted(table, key=lambda r: _key(r.get("by") or r.get("date") or "1936.1.1")):
        when = r.get("by") or r.get("date")
        for kind, field, count in (("template", "template", "divisions"), ("role", "role", "divisions"),
                                   ("plane", "plane_type", "planes"), ("ship", "ship_type", "ships")):
            name, want = r.get(field), r.get(count) or r.get("count")
            if not (name and isinstance(want, (int, float))):
                continue
            got = have[kind].get(name, 0)
            if not when or _key(when) <= _key(date):
                good = got >= want
                ok &= good
                rows.append(f"{name} {got}/{want} by {when or 'now'} {'ok' if good else 'BELOW'}")
            elif (kind, name) not in seen:
                seen.add((kind, name))
                span = _days(start, _key(when))
                part = max(0.0, min(1.0, _days(start, _key(date)) / span)) if span > 0 else 1.0
                line = want * part
                good = got >= 0.5 * line
                ok &= good
                ahead.append(f"{name} {got} (straight line {line:.0f} of {want} by {when}) "
                             f"{'on line' if got >= line else 'behind' if good else 'FAR BEHIND'}")
            break
    out.append("force against the plan (rows due by today): " + ("; ".join(rows) or "none due yet"))
    out.append("force ahead (next row per name; fails below half the straight line): " + ("; ".join(ahead) or "none"))
    out.append(f"force: {'on path' if ok else 'OFF PATH'}")
    return out, ok


def air(reads, plan):
    """Wings by type with planes and bases, missions flying, superiority in the plan's regions, planes in stock,
    fuel."""
    out = []
    a = reads.get("hoi4_air") or {}
    wings = a.get("wings") or []
    bases = {b.get("base_id"): b for b in a.get("air_bases") or []}
    by = collections.defaultdict(lambda: [0, 0, collections.Counter(), collections.Counter()])
    for w in wings:
        k = w.get("aircraft_type") or w.get("unit_type") or "?"
        row = by[k]
        row[0] += 1
        row[1] += w.get("aircraft") or 0
        b = bases.get((w.get("air_base") or {}).get("base_id")) or {}
        row[2][(b.get("region") or {}).get("name") or f"state {(w.get('air_base') or {}).get('state_id')}"] += 1
        region = w.get("region")
        for m in w.get("missions") or []:
            row[3][f"{m} over {region.get('name') if isinstance(region, dict) else region}"] += 1
    out.append(f"wings {len(wings)}, planes {sum(w.get('aircraft') or 0 for w in wings)}")
    for k, (n, planes, where, miss) in sorted(by.items()):
        out.append(f"  {k}: {n} wings, {planes} planes; based " + ", ".join(f"{r} {c}" for r, c in where.most_common())
                   + "; flying " + (", ".join(f"{m} ({c})" for m, c in miss.most_common()) or "nothing"))
    idle = [w.get("wing") for w in wings if not w.get("missions")]
    out.append(f"wings with no mission: {len(idle)}" + (
        f" ({', '.join(map(str, idle[:6]))}{'...' if len(idle) > 6 else ''})" if idle else ""))
    names = set()
    plan = plan or {}
    for e in (plan.get("manage") or {}).get("air_plan") or plan.get("air_plan") or []:
        for k in ("region", "then_region"):
            if e.get(k):
                names.add(str(e[k]))
    for r in (plan.get("war") or {}).get("air_front_regions") or (plan.get("manage") or {}).get("air_front_regions") \
            or []:
        names.add(str(r))
    regions = (reads.get("hoi4_air_regions") or {}).get("regions") or []
    sup = []
    for r in regions:
        if names and str(r.get("name")) not in names and str(r.get("id")) not in names:
            continue
        v = r.get("enemy_air_superiority")
        sup.append(f"{r.get('name')} {'unread' if v is None else v}")
    reason = next((r.get("enemy_air_superiority_reason") for r in regions if r.get("enemy_air_superiority_reason")),
                  None)
    out.append("enemy air superiority in the plan's regions: " + (", ".join(sup[:12]) or "no region named")
               + (f" (reason: {reason})" if reason and any("unread" in x for x in sup) else ""))
    stocked = [e for e in (reads.get("hoi4_stockpile") or {}).get("equipment") or []
               if any(x in (e.get("archetype") or "") for x in ("airframe", "plane"))]
    out.append("planes in stock: " + (", ".join(f"{e['archetype']} {e.get('stockpile')}" for e in stocked)
                                      or "none listed (hoi4_stockpile names no plane archetype)"))
    f = (reads.get("hoi4_resources") or {}).get("fuel") or {}
    left = f.get("days_left")
    out.append(f"fuel {f.get('stock')} of {f.get('capacity')} (ratio {f.get('ratio') and round(f['ratio'], 2)}); "
               f"days left {'unknown (the tool has no burn rate)' if left is None else left}")
    return out


WAR_DAY = "1939.09.01"


def gate_lines(snap, plan=None, units=None, baseline=None, marks=None, target=None, plan_start=None):
    """The gate as lines of text, and (fill_ok, force_ok). `marks` (civ, mil, divs) from the user win over the
    baseline file; `target` is the date the equipment path is measured to (the baseline's WAR_DAY by default)."""
    reads = snap.get("reads") or {}
    now = (reads.get("hoi4_observe") or {}).get("now") or {}
    date = str(now.get("date") or snap.get("date") or "").strip()
    tag = now.get("player") or snap.get("tag")
    pool = None
    if marks:
        civ, mil, divs = marks
        src = "given"
    elif baseline and date:
        row = baseline_row(baseline, date) or {}
        civ, mil, divs, pool = (row.get("civilian_factories"), row.get("military_factories"), row.get("divisions"),
                                row.get("manpower"))
        src = f"AI baseline {row.get('date')}"
    else:
        civ = mil = divs = None
        src = "none: no stored AI baseline for %s and no marks given" % (tag or "this country")
    target = target or (WAR_DAY if baseline else None)
    target_row = baseline_row(baseline, target) if baseline and target else None
    ec = reads.get("hoi4_economy") or {}
    ar = reads.get("hoi4_armies") or {}
    al = reads.get("hoi4_alerts") or {}
    st = (reads.get("hoi4_country_status") or {}).get("fields") or {}
    fc = ec.get("factory_trigger_count") or {}
    d = sum(a.get("division_count") or 0 for a in ar.get("armies") or [])
    un = len(ar.get("unassigned") or [])
    lines, fill_ok = equipped(reads, date or None, target, target_row)

    def mark(have, want):
        return "" if want is None else (" / AI %s: %s" % (want, "ok" if (have or 0) >= want else "BELOW"))
    out = [f"date {date} (marks: {src})",
           f"civilian {fc.get('civilian')}{mark(fc.get('civilian'), civ)}",
           f"military {fc.get('military')}{mark(fc.get('military'), mil)}"]
    if divs is None:
        out.append(f"divisions in armies {d} + unassigned {un}: EQUIPPED {'ok' if fill_ok else 'BELOW'} "
                   f"(the fill alone; no mark for the count)")
    else:
        count_ok = d + un >= divs
        out.append(f"divisions in armies {d} + unassigned {un} / AI {divs}: count {'ok' if count_ok else 'BELOW'}; "
                   f"EQUIPPED {'ok' if count_ok and fill_ok else 'BELOW'} (a count passes only with the fill)")
    out += lines
    mp = (st.get("manpower") or {}).get("value")
    out.append(f"manpower pool {mp} / AI pool {pool}")
    out.append(f"armies {len(ar.get('armies') or [])}: " + ", ".join(
        f"{a.get('army')} {a.get('division_count')}" for a in ar.get("armies") or []))
    out.append(f"idle: research slots {now.get('idle_research_slots')}, civilian factories "
               f"{ec.get('civilian_factories_idle')}")
    out.append("alerts: " + ", ".join(a.get("alert") or str(a.get("frame")) for a in al.get("alerts") or []))
    flines, fok = force(reads, date, plan or {}, units or {}, plan_start or "1936.1.1")
    out.append("FORCE " + ("not gated (no force table)" if fok is None else "on the plan path" if fok
                           else "OFF the plan path (gate fails)"))
    out += ["  " + x for x in flines]
    out.append("AIR")
    out += ["  " + x for x in air(reads, plan or {})]
    return out, (fill_ok, fok)
