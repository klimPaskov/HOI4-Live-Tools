"""The bar audit of a run, from its own files (reads nothing from the game): one line per item of the bar a
published campaign is measured against, with counts.

    game-actions.jsonl   the runtime's action journal (the last receipt of each action counts)
    feed.jsonl           the runtime's feed as the supervisor kept it (alerts, wakes, lost land, executor orders)
    native.stderr        the runtime's [action] lines (self-pauses, per-state interface selections)
    watch.jsonl          the supervisor's samples: date and pause each minute (pace), army supply, production
    calls.jsonl          the player's reads (per-state reads)
    plan.json            the alerts the plan ignores

A run folder of the developer harness (calls.jsonl with bodies, mcp.stderr, watch.log, monitor.log, prod.log)
reads the same way, so a run gives the same lines whichever way it was played. Standard library only.
"""

import collections
import datetime
import json
import os
import re


def _open(run, *names):
    for name in names:
        try:
            return open(os.path.join(run, name), encoding="utf-8", errors="replace")
        except OSError:
            continue
    return None


def _lines(run, *names):
    f = _open(run, *names)
    if f is None:
        return []
    with f:
        return f.read().splitlines()


def _jsonl(run, *names):
    rows = []
    for line in _lines(run, *names):
        try:
            rows.append(json.loads(line))
        except ValueError:
            continue
    return rows


def days(a, b):
    """Game days from a to b on the audit's 30-day month (the same count the bar was set with)."""
    ya, ma, da = map(int, a[:10].split("."))
    yb, mb, db = map(int, b[:10].split("."))
    return (yb - ya) * 360 + (mb - ma) * 30 + (db - da)


def actions(run):
    """{id: [action, last status, game date]} in journal order."""
    acts = collections.OrderedDict()
    for d in _jsonl(run, "game-actions.jsonl"):
        if not d.get("id"):
            continue
        a = acts.setdefault(d["id"], [d.get("action") or {}, None, d.get("game_date") or d.get("date")])
        a[1] = d.get("status")
    return acts


def feed(run):
    """The feed's changes in order, each once: feed.jsonl, else the bodies of the observe calls in calls.jsonl."""
    seen, out = set(), []
    rows = _jsonl(run, "feed.jsonl")
    if not rows:
        for d in _jsonl(run, "calls.jsonl"):
            if d.get("tool") in ("hoi4_observe", "hoi4_wait_change") and isinstance(d.get("body"), dict):
                rows += d["body"].get("changes") or []
    for ch in rows:
        if not isinstance(ch, dict) or ch.get("seq") in seen or not isinstance(ch.get("date"), str):
            continue
        seen.add(ch.get("seq"))
        out.append(ch)
    return out


def action_lines(run):
    """The runtime's [action] records (native.stderr in a product run, mcp.stderr in a harness run)."""
    out = []
    for line in _lines(run, "native.stderr", "mcp.stderr"):
        if line.startswith("[action]"):
            try:
                out.append(json.loads(line[9:]))
            except ValueError:
                continue
    return out


def watch(run):
    """The samples: [{"date", "paused", "t", "armies": {name: {"sup", "fill"}}, "idle_slots", "pp"}]."""
    out = []
    for d in _jsonl(run, "watch.jsonl"):
        armies = d.get("armies")
        if isinstance(armies, list):          # the supervisor's form
            armies = {a.get("army"): {"sup": a.get("supply"), "fill": a.get("fill")}
                      for a in armies if isinstance(a, dict)}
        out.append(dict(d, armies=armies or {}))
    if out:
        return out
    return [dict(d, armies=d.get("armies") or {}) for d in _jsonl(run, "watch.log")]


def production(run):
    """Production samples (lines, stock, factories, political power): watch.jsonl "prod", else prod.log."""
    out = [d["prod"] for d in _jsonl(run, "watch.jsonl") if isinstance(d.get("prod"), dict)]
    return out or _jsonl(run, "prod.log")


def pace_points(run):
    """(wall time, game day) pairs while the game ran: watch.jsonl (t), else monitor.log (wall clock HH:MM:SS)."""
    pts = []
    for d in _jsonl(run, "watch.jsonl"):
        if d.get("paused") is False and d.get("date") and d.get("t") is not None:
            try:
                game = datetime.datetime.strptime(str(d["date"])[:10], "%Y.%m.%d")
            except ValueError:
                continue
            pts.append((float(d["t"]), game))
    if pts:
        return pts, False
    for line in _lines(run, "monitor.log"):
        p = line.split()
        if len(p) > 2 and "paused=False" in line:
            try:
                wall = datetime.datetime.strptime(p[0], "%H:%M:%S")
                game = datetime.datetime.strptime(p[1][:10], "%Y.%m.%d")
            except ValueError:
                continue
            pts.append((wall, game))
    return pts, True


def pace_line(run):
    pts, clock = pace_points(run)
    if len(pts) < 2:
        return None
    if clock:
        pairs = [(((b[0] - a[0]).total_seconds() % 86400), (b[1] - a[1]).days) for a, b in zip(pts, pts[1:])]
    else:
        pairs = [(b[0] - a[0], (b[1] - a[1]).days) for a, b in zip(pts, pts[1:])]
    pairs = [(w, g) for w, g in pairs if 0 < w < 300]
    wall = sum(w for w, _ in pairs)
    game = sum(g for _, g in pairs)
    return ("pace: %.1f game days per wall minute over %.0f running minutes" % (game / (wall / 60), wall / 60)
            if wall else "pace: ?")


def fam(e):
    return re.sub(r"_\d+$", "", e or "")


# Actions that select ONE state through the interface (Find view + click); a slot read over 1.5 s opened views.
STATE_UI = {"hoi4_state_slots", "hoi4_army_create", "hoi4_army_assign", "hoi4_army_move", "hoi4_construction_add",
            "hoi4_railway_build", "hoi4_construction_find_state", "hoi4_occupation_law"}
MANAGEMENT = ["decisions_available", "modify_government", "modify_officer_corps", "doctrines_available", "mio_traits",
              "free_dockyards", "free_military_factories", "free_civilian_factories", "unassigned_divisions",
              "outdated_equipment_in_production", "special_project_available", "scientist_unassigned",
              "operative_recruitable", "research_slot_available", "admiral_assignable",
              "missing_equipment_production", "not_enough_garrison"]


def audit_lines(run, name=None):
    """The bar audit as lines of text (the sections the game is not needed for)."""
    name = name or os.path.basename(os.path.normpath(run))
    acts = actions(run)
    by_kind = collections.defaultdict(collections.Counter)
    fail_groups = collections.Counter()
    for a, s, _ in acts.values():
        by_kind[a.get("kind")][s] += 1
        if s != "Verified":
            fail_groups[json.dumps(a, sort_keys=True)] += 1
    repeat_fail = {g: n for g, n in fail_groups.items() if n >= 2}

    changes = feed(run)
    last_date = changes[-1]["date"][:10] if changes else "?"
    first_unpause = next((c["date"][:10] for c in changes if c.get("kind") == "paused_changed"), None)
    spans, open_at = collections.defaultdict(int), {}
    wakes, wake_ev = collections.Counter(), collections.defaultdict(list)
    for c in changes:
        k, det, dt = c.get("kind"), c.get("detail") or {}, c["date"][:10]
        if k == "wake_player":
            r = str(det.get("reason"))
            wakes[r.split(" ")[0][:40]] += 1
            wake_ev[r.split(" ")[0][:40]].append((dt, det.get("evidence")))
            if r.startswith("alert:"):
                open_at.setdefault(r[6:], dt)
        elif k == "alert_raised":
            open_at.setdefault(det.get("alert"), dt)
        elif k == "alert_cleared":
            a = det.get("alert")
            if a in open_at:
                spans[a] += max(0, days(open_at.pop(a), dt))
    for a, t in open_at.items():
        if last_date != "?":
            spans[a] += max(0, days(t, last_date))
    lost = [(c["date"][:10], c["kind"], (c.get("detail") or {}).get("state_id")
             or (c.get("detail") or {}).get("province_id"))
            for c in changes if c.get("kind") in ("own_state_lost", "own_province_lost")]

    records = action_lines(run)
    sp = sum(1 for d in records if d.get("self_pause"))
    samples = watch(run)
    idle_samples = sum(1 for d in samples if d.get("idle_slots")) - (1 if samples and samples[0].get("idle_slots")
                                                                       else 0)
    low_fill, low_sup = collections.Counter(), collections.Counter()
    for d in samples:
        for n, x in (d.get("armies") or {}).items():
            if not isinstance(x, dict):
                continue
            if (x.get("fill") or 1) < 0.8:
                low_fill[n] += 1
            if x.get("sup") is not None and x["sup"] < 50:
                low_sup[n] += 1

    def kinds(*ks):
        out = []
        for k in ks:
            c = by_kind.get(k)
            if c:
                out.append("%s %d/%d" % (k, c.get("Verified", 0), sum(c.values())))
        return ", ".join(out) or "none"

    stab = [e.get("stability") for _, e in wake_ev.get("stability_low", []) if isinstance(e, dict)]
    pps = [d.get("pp") for d in samples if d.get("pp") is not None]
    out = [
        "BAR AUDIT %s: feed to %s, %d actions, %d watch samples" % (name, last_date, len(acts), len(samples)),
        "1 self-pauses: %d" % sp,
        "2 failed actions: %d; repeated failures: %d in %d groups: %s" % (
            sum(1 for _, s, _ in acts.values() if s != "Verified"), sum(repeat_fail.values()), len(repeat_fail),
            "; ".join("%sx %s" % (n, json.loads(g).get("kind"))
                      for g, n in sorted(repeat_fail.items(), key=lambda x: -x[1]))),
        "3 research: slot_idle events %d, idle watch samples %d" % (
            sum(1 for c in changes if c.get("kind") == "research_slot_idle"), idle_samples),
        "4 political power: idle wakes %d; max PP %s; actions %s; decisions_available open %d d, "
        "modify_officer_corps open %d d" % (
            wakes.get("political_power_idle", 0), max(pps) if pps else "?",
            kinds("politics_choose", "decision_take", "law_change", "advisor_hire"),
            spans.get("decisions_available", 0), spans.get("modify_officer_corps", 0)),
        "5 stability: low wakes %d (values %s-%s); war support: not readable" % (
            wakes.get("stability_low", 0), min(stab) if stab else "?", max(stab) if stab else "?"),
        "6 armies: fill<0.8 samples %s; supply<50%% samples %s; equipment_fill_low wakes %d; "
        "no_divisions_in_training open %d d; unassigned_divisions open %d d; missing_equipment_production open %d d; "
        "stockpile_low wakes %d" % (
            dict(low_fill), dict(low_sup), wakes.get("equipment_fill_low", 0),
            spans.get("no_divisions_in_training", 0), spans.get("unassigned_divisions", 0),
            spans.get("missing_equipment_production", 0), wakes.get("stockpile_low", 0)),
        "7 battle: %s" % kinds("army_move", "army_plan", "army_offensive", "army_front"),
    ]
    strike, posts = collections.Counter(), collections.Counter()
    for c in changes:
        if c.get("kind") == "autopilot_action":
            a = (c.get("detail") or {}).get("action") or ""
            if a.startswith("strike "):
                strike[c["date"][:7]] += 1
                for w in ("hold covered state", "retake the land", "leave land that does not feed"):
                    if w in a:
                        posts[w] += 1
    out += [
        "7b executor strike orders per month: %s; post orders %s" % (dict(sorted(strike.items())), dict(posts)),
        "8 air: %s; air_wings_without_mission open %d d" % (kinds("air_mission", "air_rebase"),
                                                            spans.get("air_wings_without_mission", 0)),
        "9 navy: %s; free_dockyards open %d d; admiral_assignable open %d d" % (
            kinds("fleet_mission", "fleet_region", "fleet_admiral", "naval_production"),
            spans.get("free_dockyards", 0), spans.get("admiral_assignable", 0)),
        "10 espionage: %s" % kinds("agency_create", "agency_upgrade", "operative_recruit", "operative_assign"),
        "11 doctrine: %s; doctrines_available open %d d" % (kinds("doctrine_adopt"),
                                                            spans.get("doctrines_available", 0)),
        "12 designer: %s" % kinds("template_create", "template_edit", "template_design", "designer_save"),
        "13 construction: %s" % kinds("construction_add", "railway_build"),
        "13b MIO and special projects: %s; mio_traits open %d d, special_project_available open %d d, "
        "scientist_unassigned open %d d" % (
            kinds("mio_design_team", "mio_policy", "mio_trait", "special_project_assign", "special_project_start",
                  "scientist_assign"),
            spans.get("mio_traits", 0), spans.get("special_project_available", 0),
            spans.get("scientist_unassigned", 0)),
        "13c decisions and factions: %s; decisions_available open %d d" % (
            kinds("decision_take", "faction_invite", "faction_create", "faction_leave", "diplomacy_act"),
            spans.get("decisions_available", 0)),
        "14 trade: %s" % kinds("auto_import", "trade_import", "trade"),
        "15 territory lost: %d %s" % (len(lost), lost[:8]),
    ]
    out += alert_lines(run, changes, spans)
    pace = pace_line(run)
    if pace:
        out.append(pace)
    out += production_lines(run, acts)
    out += state_walk_lines(run, records, changes, first_unpause, last_date)
    out.append("M alerts open (days) by management item: " + ", ".join("%s %d" % (k, spans.get(k, 0))
                                                                    for k in MANAGEMENT))
    return out


def alert_lines(run, changes, spans):
    """Alerts open by days, each marked with its written reason: the plan's alerts_ignore, or the owning step's
    status line in the last wake for it."""
    try:
        with open(os.path.join(run, "plan.json"), encoding="utf-8") as f:
            ignored = set((json.load(f).get("manage") or {}).get("alerts_ignore") or [])
    except (OSError, ValueError, AttributeError):
        ignored = set()
    why = {}
    for c in changes:
        d = c.get("detail") or {}
        if c.get("kind") == "wake_player" and str(d.get("reason", "")).startswith("alert:"):
            m = str(d.get("message") or "")
            hit = re.search(r"; (\w+): (.*)$", m) or re.search(r"the plan ignores it \(([^)]*)", m)
            if hit:
                why[str(d["reason"])[6:]] = hit.group(0)[:90]
    marked = dict(sorted(((k + (" [plan ignores]" if k in ignored else " [reason]" if k in why else " [NO REASON]"),
                           v) for k, v in spans.items()), key=lambda x: -x[1])[:14])
    long = [k for k, v in spans.items() if v >= 60 and k not in ignored and k not in why]
    out = ["alerts open (days): %s" % marked,
           "alerts open 60+ days without a written reason: %d %s" % (len(long), long)]
    out += ["  reason %s: %s" % (k, why[k]) for k in sorted(why) if spans.get(k, 0) >= 60]
    return out


def production_lines(run, acts):
    prod = production(run)
    if not prod:
        return ["P1-P6 production: no production samples (the supervisor takes one every 5 minutes)"]
    dup_max, dup_ex, over, spread = 0, None, collections.defaultdict(list), []
    for s in prod:
        mil = [x for x in s.get("lines") or [] if x[1] == "military"]
        c = collections.Counter(x[2] for x in mil)
        d = [e for e, n in c.items() if n > 1]
        if len(d) > dup_max:
            dup_max, dup_ex = len(d), (s["date"], d[:4])
        for x in mil:
            if (x[3] or 0) - (x[4] or 0) > 2:
                over[x[2]].append(s["date"])
        cov = [(a, (st or 0) / ne) for a, st, ne, *_ in s.get("stock") or [] if ne]
        if cov:
            spread.append((s["date"], max(cov, key=lambda x: x[1]), min(cov, key=lambda x: x[1])))
    long_over = {e: (v[0], v[-1]) for e, v in over.items() if len(v) >= 2 and days(v[0], v[-1]) > 21}
    adds = [(a.get("equipment"), dt) for a, _s, dt in acts.values() if a.get("kind") == "production_add"]
    out = ["P1 duplicate military lines: max %d equipment with two lines %s" % (dup_max, dup_ex or ""),
           "P2 lines appended (production_add): %d adds: %s" % (
               len(adds), collections.Counter(fam(e) for e, _ in adds).most_common(6)),
           "P3 requested above active by >2 for >3 weeks: %s" % (long_over or "none")]
    if spread:
        last = spread[-1]
        out.append("P4 stock/fielded-need spread (last sample %s): best %s %.2f, worst %s %.2f"
                   % (last[0], last[1][0], last[1][1], last[2][0], last[2][1]))
    s = prod[-1]
    mil = sorted((x for x in s.get("lines") or [] if x[1] == "military"), key=lambda x: x[0])
    out.append("P5 last lines %s (mil %s, requested %s): %s" % (
        s["date"], s.get("mil"), sum(x[3] or 0 for x in mil),
        ", ".join("%s %s/%s" % (fam(x[2])[:18], x[3], x[4]) for x in mil)))
    pps = [x.get("pp") or 0 for x in prod]
    out.append("P6 political power samples: max %.0f, samples above 150: %d/%d"
               % (max(pps), sum(1 for p in pps if p > 150), len(pps)))
    return out


def state_walk_lines(run, records, changes, first_unpause, last_date):
    walk, walk_ms = collections.Counter(), collections.Counter()
    for d in records:
        t = d.get("tool")
        if t not in STATE_UI or (t == "hoi4_state_slots" and (d.get("ms") or 0) < 1500):
            continue
        who = "executor" if d.get("executor") else "player"
        walk[(who, t[5:])] += 1
        walk_ms[(who, t[5:])] += d.get("ms") or 0
    reads = collections.Counter()
    for d in _jsonl(run, "calls.jsonl"):
        q = str((d.get("args") or {}).get("query", ""))
        if d.get("tool") == "hoi4_states" and not (len(q) == 3 and q.isupper()):
            reads["hoi4_states by name/id"] += 1
        elif d.get("tool") == "hoi4_state_slots":
            reads["hoi4_state_slots"] += 1
    try:
        y0, m0 = map(int, (first_unpause or changes[0]["date"])[:7].split("."))
        y1, m1 = map(int, last_date[:7].split("."))
        months = max(1, (y1 - y0) * 12 + m1 - m0)
    except (ValueError, IndexError, KeyError):
        months = 1
    return ["S1 per-state interface selections per game month (%d months): %s; per-state reads by the player: %s" % (
        months, ", ".join("%s:%s %.1f (%.0f s)" % (w, t, n / months, walk_ms[(w, t)] / 1000 / months)
                          for (w, t), n in walk.most_common()) or "none", dict(reads) or "none")]
