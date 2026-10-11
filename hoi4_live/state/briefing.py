"""The one-page situation report behind hoi4_briefing: a pure function of the merged state model (assemble.py),
the previous briefing's facts, popups and log notes, so it can be unit tested without the game.

build() returns {"markdown", "facts", "attention", "deltas"}:
    facts       a flat dict of the key numbers (kept for the next briefing's deltas)
    attention   what needs a decision now, most urgent first: [{"level": "now" | "soon" | "info", "text"}]
    deltas      [{"field", "from", "to"}] since the previous briefing
"""

import datetime

MONTHS = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"]
FACT_LABELS = {
    "political_power": "political power", "stability": "stability", "war_support": "war support",
    "manpower": "manpower", "civilian_factories": "civilian factories", "military_factories": "military factories",
    "dockyards": "dockyards", "divisions": "divisions", "technologies": "technologies", "enemies": "enemies",
    "front_states": "enemy front states", "world_tension": "world tension", "convoys": "convoys", "fuel": "fuel",
    "focus": "focus", "researching": "research", "construction_queued": "construction items",
    "surrender_progress": "surrender progress", "faction": "faction", "ruling_party": "ruling party",
}
PERCENT = {"stability", "war_support", "world_tension", "surrender_progress"}


def fmt_date(text, hour=True):
    try:
        p = [int(x) for x in str(text).split(".")]
        s = f"{p[2]} {MONTHS[p[1] - 1]} {p[0]}"
        return s + (f" {p[3]:02d}:00" if hour and len(p) > 3 else "")
    except (ValueError, IndexError, TypeError):
        return str(text)


def fmt_num(v, digits=1):
    if v is None:
        return "?"
    if isinstance(v, bool):
        return "yes" if v else "no"
    a = abs(v)
    if a >= 1e6:
        return f"{v / 1e6:.{digits}f}M"
    if a >= 1e4:
        return f"{v / 1e3:.{digits}f}K"
    if isinstance(v, float) and not v.is_integer():
        return f"{v:.{digits}f}"
    return f"{int(v)}"


def pct(v, digits=0):
    return "?" if v is None else f"{v * 100:.{digits}f}%"


def _g(d, *path, default=None):
    for p in path:
        if not isinstance(d, dict):
            return default
        d = d.get(p)
    return default if d is None else d


def facts(m):
    """Key numbers for deltas."""
    ov, eco, mil, dip = m.get("overview", {}), m.get("economy", {}), m.get("military", {}), m.get("diplomacy", {})
    research = m.get("research", {})
    f = {
        "date": m.get("date"), "tag": m.get("tag"),
        "political_power": ov.get("political_power"), "stability": ov.get("stability"),
        "war_support": ov.get("war_support"), "manpower": ov.get("manpower"),
        "civilian_factories": _g(eco, "factories", "civilian") or ov.get("civilian_factories"),
        "military_factories": _g(eco, "factories", "military") or ov.get("military_factories"),
        "dockyards": _g(eco, "factories", "dockyards") or ov.get("dockyards"),
        "divisions": mil.get("divisions") or ov.get("divisions"),
        "technologies": research.get("researched"),
        "enemies": dip.get("enemies"), "front_states": dip.get("front_states"),
        "surrender_progress": dip.get("surrender_progress"),
        "world_tension": ov.get("world_tension"), "convoys": eco.get("convoys"),
        "fuel": _g(eco, "fuel", "fuel"),
        "focus": _g(m, "focus", "current_name") or _g(m, "focus", "current"),
        "researching": ", ".join(r.get("name") or r.get("tech") for r in research.get("researching") or []) or None,
        "construction_queued": eco.get("construction_queued"),
        "faction": dip.get("faction") if isinstance(dip.get("faction"), str) else None,
        "ruling_party": ov.get("ruling_party"),
    }
    return f


def deltas(prev, cur):
    out = []
    if not prev:
        return out
    for k, label in FACT_LABELS.items():
        a, b = prev.get(k), cur.get(k)
        if a is None or b is None or a == b:
            continue
        if isinstance(a, (int, float)) and isinstance(b, (int, float)):
            if k in PERCENT:
                if abs(b - a) < 0.005:
                    continue
                out.append({"field": k, "from": a, "to": b, "text": f"{label} {pct(a, 1)} -> {pct(b, 1)}"})
            else:
                if abs(b - a) < (0.05 if isinstance(a, float) else 1):
                    continue
                sign = "+" if b > a else ""
                out.append({"field": k, "from": a, "to": b,
                            "text": f"{label} {fmt_num(a)} -> {fmt_num(b)} ({sign}{fmt_num(b - a)})"})
        else:
            out.append({"field": k, "from": a, "to": b, "text": f"{label}: {a} -> {b}"})
    return out


def _src_age(m, name):
    s = m.get("sources", {}).get(name) or {}
    if not s.get("as_of"):
        return None
    age = s.get("age_days")
    return f"{name} {fmt_date(s['as_of'])}" + (f", {age:g} d old" if age else "")


def attention(m):
    """What needs a decision now, most urgent first."""
    items = []
    ov, eco, mil, dip = m.get("overview", {}), m.get("economy", {}), m.get("military", {}), m.get("diplomacy", {})
    scr = m.get("sources", {}).get("screen") or {}
    save_age = (m.get("sources", {}).get("save") or {}).get("age_days")
    stale = f" (save, {save_age:g} d old)" if save_age else " (save)"

    def add(level, text, code=None, **data):
        item = {"level": level, "text": text}
        if code:
            item["code"] = code
            item.update(data)
        items.append(item)
    if scr.get("popups"):
        add("now", f"{scr['popups']} popup(s) open: read and answer them (hoi4_popups)", "popups")
    if dip.get("at_war"):
        sp = dip.get("surrender_progress") or 0
        if sp >= 0.2:
            add("now", f"surrender progress {pct(sp)}: the war is going badly", "losing_war", surrender=sp)
        lost = dip.get("lost_states") or []
        if lost:
            add("now", f"{len(lost)} of our states are held by others: "
                       + ", ".join(f"{s.get('name')} ({s.get('ctrl')})" for s in lost[:5]), "lost_states",
                states=[s.get("name") for s in lost[:5]])
        if dip.get("front_states"):
            add("info", f"front: {dip['front_states']} enemy-held states touch ours", "front")
    f = m.get("focus", {})
    if f.get("_sources") and not f.get("current"):
        add("now", "no national focus selected" + stale + ": choose one (politics, Q > National Focus)", "no_focus")
    r = m.get("research", {})
    if r.get("idle_slots"):
        add("now", f"{r['idle_slots']} research slot(s) idle" + stale + ": start research (W)", "idle_research",
            slots=r["idle_slots"])
    fac = eco.get("factories") or {}
    queued = eco.get("construction_queued")
    if fac.get("civilian_free") and queued == 0:
        add("now", f"{fac['civilian_free']} civilian factories free and nothing in construction (T)",
            "idle_construction", free=fac["civilian_free"])
    elif fac.get("civilian_free") and queued is None and fac["civilian_free"] >= 5:
        add("soon", f"{fac['civilian_free']} civilian factories free: check the construction queue (T)",
            "idle_construction", free=fac["civilian_free"])
    if fac.get("military_free"):
        add("now", f"{fac['military_free']} military factories not producing anything (Y)", "idle_production",
            free=fac["military_free"])
    if fac.get("dockyards_free"):
        add("soon", f"{fac['dockyards_free']} dockyards not producing anything (Y)", "idle_dockyards",
            free=fac["dockyards_free"])
    pp = ov.get("political_power")
    if pp is not None and pp >= 100:
        add("soon", f"political power {fmt_num(pp)}: spend it (advisors, laws, decisions)", "spend_pp", pp=pp)
    eq = mil.get("equipment") or {}
    short = sorted(((k, v["deficit"]) for k, v in eq.items() if isinstance(v, dict) and v.get("deficit", 0) > 0),
                   key=lambda x: -x[1])
    if short:
        add("soon", "equipment short in divisions: " + ", ".join(f"{k.replace('_', ' ')} {fmt_num(d)}"
                                                                 for k, d in short[:3]), "equipment_short",
            equipment=[k for k, _ in short[:3]])
    res = eco.get("resources") or {}
    neg = sorted(((k, v["surplus"]) for k, v in res.items() if isinstance(v, dict) and (v.get("surplus") or 0) < 0),
                 key=lambda x: x[1])
    if neg:
        add("soon", "resource deficits: " + ", ".join(f"{k} {fmt_num(s)}" for k, s in neg)
            + " (trade R, or build/produce less)", "resource_deficit", resources=[k for k, _ in neg])
    st = ov.get("stability")
    if st is not None and st < 0.4:
        add("soon", f"stability is low ({pct(st)})", "low_stability", stability=st)
    fuel = eco.get("fuel") or {}
    if fuel.get("ratio") is not None and fuel["ratio"] < 0.1 and dip.get("at_war"):
        add("soon", f"fuel nearly empty ({pct(fuel['ratio'])})", "low_fuel")
    elect = _g(m, "politics", "days_to_election")
    if elect is not None and 0 <= elect <= 30:
        add("info", f"election in {elect} days", "election", days=elect)
    for d in _g(m, "decisions", "active", default=[]) or []:
        if d.get("days_left") is not None and d["days_left"] <= 30 and d.get("kind", "").endswith("mission"):
            add("soon", f"mission '{d.get('name')}' ends in {fmt_num(d['days_left'])} days", "mission_ending",
                mission=d.get("name"))
    tl = m.get("sources", {}).get("telemetry") or {}
    if tl.get("available") is False:
        add("info", "no telemetry: numbers come from the save and the screen only (hoi4_telemetry action=enable)")
    order = {"now": 0, "soon": 1, "info": 2}
    items.sort(key=lambda i: order[i["level"]])
    return items


def build(m, prev=None, popups=None, log_notes=None):
    """The briefing for a merged model m (assemble.collect output)."""
    ov, eco, mil, dip = m.get("overview", {}), m.get("economy", {}), m.get("military", {}), m.get("diplomacy", {})
    pol, f, r = m.get("politics", {}), m.get("focus", {}), m.get("research", {})
    scr = m.get("sources", {}).get("screen") or {}
    F = facts(m)
    now = _date_key(m.get("date"))
    prev_key = _date_key((prev or {}).get("date"))
    # a previous briefing dated later than now belongs to an abandoned timeline (an older save was loaded since)
    later_prev = bool(now and prev_key and prev_key > now)
    D = [] if later_prev else deltas(prev, F)
    A = attention(m)
    L = []
    state = ""
    if scr.get("as_of"):
        state = ", paused" if scr.get("paused") else (", running" if scr.get("paused") is False else "")
        if scr.get("speed"):
            state += f", speed {scr['speed']}"
    L.append(f"# {m.get('name') or m.get('tag')} ({m.get('tag')}), {fmt_date(m.get('date'))}{state}")
    srcs = [s for s in (_src_age(m, "telemetry"), _src_age(m, "save"), _src_age(m, "screen")) if s]
    if (m.get("sources", {}).get("save") or {}).get("file"):
        srcs = [s + (f" ({m['sources']['save']['file']})" if s.startswith("save") else "") for s in srcs]
    L.append("Sources: " + "; ".join(srcs) if srcs else "Sources: none")
    L.append("")
    L.append("## Needs attention")
    if A:
        for i, a in enumerate(A, 1):
            L.append(f"{i}. [{a['level']}] {a['text']}")
    else:
        L.append("- nothing urgent")
    L.append("")
    L.append("## Situation")
    gov = []
    if ov.get("ruling_party"):
        gov.append(f"{ov['ruling_party']}" + (f" ({ov.get('ideology')}, {pct(ov.get('party_popularity'))})"
                                               if ov.get("ideology") else ""))
    if ov.get("leader"):
        gov.append(f"leader {ov['leader']}")
    if pol.get("days_to_election") is not None and pol["days_to_election"] >= 0:
        gov.append(f"election in {pol['days_to_election']} days")
    if gov:
        L.append("- Government: " + "; ".join(gov))
    laws = pol.get("laws") or {}
    if laws:
        L.append("- Laws: " + ", ".join(v.get("name") or v.get("id") for v in laws.values()))
    if pol.get("advisors"):
        L.append("- Advisors: " + ", ".join(f"{a.get('name') or a.get('character')} ({a.get('slot')})"
                                            for a in pol["advisors"][:8]))
    if pol.get("national_spirits"):
        L.append("- Spirits: " + ", ".join(pol["national_spirits"][:6]))
    dec = m.get("decisions") or {}
    if dec.get("in_effect") or dec.get("taken"):
        eff = [f"{d.get('name') or d.get('id')} ({fmt_num(d.get('days_left'))} d left)" for d in dec.get("in_effect") or []]
        done = [d.get("name") or d.get("id") for d in dec.get("taken") or []
                if d.get("id") not in {x.get("id") for x in dec.get("in_effect") or []}]
        L.append("- Decisions: " + "; ".join((["in effect: " + ", ".join(eff)] if eff else []) +
                                            (["taken: " + ", ".join(done[-6:])] if done else [])))
    L.append(f"- Political power {fmt_num(ov.get('political_power'))}"
             + (f" ({'+' if (ov.get('political_power_daily') or 0) >= 0 else ''}{fmt_num(ov.get('political_power_daily'), 2)}/day)"
                if ov.get("political_power_daily") is not None else "")
             + f"; stability {pct(ov.get('stability'))}; war support {pct(ov.get('war_support'))}"
             + (f"; command power {fmt_num(ov.get('command_power'))}" if ov.get("command_power") is not None else ""))
    if dip.get("at_war"):
        wars = dip.get("wars") or []
        names = []
        for w in wars:
            if "e" in w:
                names.append(f"{w.get('name') or w['e']} ({w['e']}: {fmt_num(w.get('div'))} div, surrender "
                             f"{pct(w.get('sp'))})")
            elif "enemy" in w:
                score = ""
                if w.get("score_breakdown_us") is not None:
                    score = (f", war score {fmt_num(w.get('score_breakdown_us'))} vs "
                             f"{fmt_num(w.get('score_breakdown_them'))}, provinces taken "
                             f"{fmt_num(w.get('provinces_captured_by_us'))} vs {fmt_num(w.get('provinces_captured_by_them'))}")
                names.append(f"{w.get('enemy_name') or w['enemy']} (since {fmt_date(w.get('since'), False)}{score})")
        L.append(f"- AT WAR with {dip.get('enemies') or len(wars)}: " + "; ".join(names[:6])
                 + f". Our surrender progress {pct(dip.get('surrender_progress'))}")
        fr = dip.get("front") or {}
        if fr.get("enemy"):
            L.append(f"- Front: {len(fr['enemy'])} enemy-held states border ours: "
                     + ", ".join(f"{s['name']} ({s['ctrl']})" for s in fr["enemy"][:8]))
    else:
        L.append("- At peace" + (f"; world tension {pct(ov.get('world_tension'))}" if ov.get("world_tension") is not None else ""))
    fac_name = _g(pol, "faction", "name") if isinstance(pol.get("faction"), dict) else None
    if fac_name:
        L.append(f"- Faction: {fac_name} (leader {_g(pol, 'faction', 'leader')}, "
                 f"{_g(pol, 'faction', 'members')} members)")
    if pol.get("overlord"):
        L.append(f"- Subject of {pol['overlord']} (autonomy {pct(pol.get('autonomy'))})")
    L.append("")
    L.append("## Economy")
    fac = eco.get("factories") or {}
    if fac:
        L.append(f"- Factories: {fmt_num(fac.get('civilian'))} civilian ({fmt_num(fac.get('civilian_free'))} free), "
                 f"{fmt_num(fac.get('military'))} military ({fmt_num(fac.get('military_free'))} free), "
                 f"{fmt_num(fac.get('dockyards'))} dockyards ({fmt_num(fac.get('dockyards_free'))} free)")
    res = eco.get("resources") or {}
    if res:
        pos = [f"{k} {fmt_num(v['surplus'])}" for k, v in res.items() if (v.get("surplus") or 0) > 0]
        neg = [f"{k} {fmt_num(v['surplus'])}" for k, v in res.items() if (v.get("surplus") or 0) < 0]
        L.append("- Resources (surplus after use and export): " + (", ".join(pos) or "none")
                 + ("; DEFICIT " + ", ".join(neg) if neg else ""))
    fuel = eco.get("fuel") or {}
    if fuel.get("fuel") is not None:
        L.append(f"- Fuel {fmt_num(fuel['fuel'])} of {fmt_num(fuel.get('capacity'))} ({pct(fuel.get('ratio'))}); "
                 f"convoys {fmt_num(eco.get('convoys'))}")
    lines = eco.get("production_lines")
    if lines is not None:
        if lines:
            agg = {}
            for l in lines:
                key = (l.get("kind"), l.get("name") or l.get("equipment"))
                a = agg.setdefault(key, [0, 0])
                a[0] += 1
                a[1] += l.get("factories") or 0
            mil_lines = [f"{n} ({fmt_num(fa)} factories" + (f", {c} lines)" if c > 1 else ")")
                         for (k, n), (c, fa) in agg.items() if k == "military"]
            nav_lines = [f"{n} ({fmt_num(fa)} dockyards" + (f", {c} lines)" if c > 1 else ")")
                         for (k, n), (c, fa) in agg.items() if k != "military"]
            if mil_lines:
                L.append("- Military production: " + "; ".join(mil_lines[:8]))
            if nav_lines:
                L.append("- Naval production: " + "; ".join(nav_lines[:6]) + (f" (+{len(nav_lines) - 6} more)" if len(nav_lines) > 6 else ""))
        else:
            L.append("- Production: no lines")
    q = eco.get("construction_queue")
    if q is not None:
        if q:
            L.append("- Construction: " + "; ".join(
                f"{c.get('building_name') or c.get('building')} in {c.get('state_name') or c.get('state')}"
                + (f" {pct(c['progress'])}" if c.get("progress") is not None else "") for c in q[:5])
                + (f" (+{len(q) - 5} more)" if len(q) > 5 else ""))
        else:
            L.append("- Construction: nothing queued")
    L.append("")
    L.append("## Military")
    if mil:
        L.append(f"- {fmt_num(mil.get('divisions'))} divisions, {fmt_num(mil.get('ships'))} ships, "
                 f"{fmt_num(mil.get('planes'))} planes; manpower {fmt_num(mil.get('manpower'))} available, "
                 f"{fmt_num(mil.get('manpower_deployed'))} deployed")
        if mil.get("average_strength") is not None:
            L.append(f"- Division average: strength {mil['average_strength']}, organisation {mil.get('average_organisation')}"
                     f" ({_src_age(m, 'save') or 'save'})")
        if mil.get("army_list") is not None:
            al = mil["army_list"]
            if al:
                L.append("- Armies: " + "; ".join(
                    f"{a['name']} ({a['divisions']} div, {a.get('leader') or 'no general'}, "
                    + (", ".join(a.get("orders") or []) or "no orders")
                    + (f", {a['executing_divisions']} executing" if a.get("executing_divisions") else "") + ")"
                    for a in al[:6]) + (f" (+{len(al) - 6} more)" if len(al) > 6 else "")
                    + (f"; {mil['unassigned_divisions']} division(s) in no army" if mil.get("unassigned_divisions") else ""))
            else:
                L.append(f"- Armies: none ({fmt_num(mil.get('unassigned_divisions'))} division(s) in no army)")
        eq = mil.get("equipment") or {}
        short = [(k, v) for k, v in eq.items() if isinstance(v, dict) and v.get("deficit", 0) > 0]
        if short:
            L.append("- Equipment short: " + "; ".join(
                f"{k.replace('_', ' ')} {fmt_num(v['deficit'])} ({fmt_num(v['in_armies'])} of {fmt_num(v['needed'])} "
                f"fielded, {fmt_num(v['stockpile'])} in stock)" for k, v in sorted(short, key=lambda x: -x[1]['deficit'])[:4]))
    L.append("")
    L.append("## Focus and research")
    if f.get("_sources"):
        if f.get("current"):
            L.append(f"- Focus: {f.get('current_name') or f['current']} (day {fmt_num(f.get('progress_days'))}"
                     + (", paused" if f.get("paused") else "") + ")")
        else:
            L.append("- Focus: none selected")
        if f.get("recently_completed"):
            L.append("- Completed focuses (latest first): " +
                     ", ".join(x.get("name") or x.get("id") for x in f["recently_completed"][:5]))
    else:
        L.append("- Focus: unknown (no save read)")
    if r.get("researching") is not None:
        rs = [f"{x.get('name') or x['tech']}" + (f" ~{pct(x['approx_progress'])}" if x.get("approx_progress") else "")
              for x in r["researching"]]
        L.append(f"- Research ({fmt_num(r.get('slots'))} slots): " + ("; ".join(rs) if rs else "nothing")
                 + (f"; {r['idle_slots']} idle" if r.get("idle_slots") else ""))
    elif r.get("slots") is not None:
        L.append(f"- Research: {fmt_num(r.get('slots'))} slots, {fmt_num(r.get('researched'))} technologies known")
    if r.get("recently_completed"):
        rc = r["recently_completed"]
        if rc:
            L.append("- Recently researched: " + ", ".join(f"{x['name']} ({x['date']})" for x in rc[:4]))
    world = m.get("world") or {}
    countries = world.get("countries") or []
    if countries:
        L.append("")
        L.append("## World")
        L.append(f"- World tension {pct(world.get('world_tension'))}; {fmt_num(world.get('countries_at_war'))} countries at war"
                 + (": " + ", ".join(f"{a}-{b}" for a, b in (world.get("enemy_pairs") or [])[:6]) if world.get("enemy_pairs") else ""))
        majors = [c for c in countries if c.get("major") and c.get("c") != m.get("tag")]
        if majors:
            L.append("- Majors: " + "; ".join(f"{c['c']} {c.get('ideo')}, {fmt_num(c.get('mil'))} mil, "
                                              f"{fmt_num(c.get('div'))} div" + (", AT WAR" if c.get("enemies") else "")
                                              for c in majors))
    ev = (dip.get("recent_events") or world.get("events") or [])
    # popups from an abandoned timeline (an older save was loaded since) are dropped
    pops = [p for p in (popups or []) if not (now and p.get("date") and _date_key(p["date"]) and _date_key(p["date"]) > now)]
    if ev or pops or log_notes:
        L.append("")
        L.append("## Recent events")
        for e in ev[-8:]:
            L.append(f"- {fmt_date(e.get('_date'), False)}: {describe_event(e)}")
        for p in pops[-6:]:
            L.append(f"- {fmt_date(p.get('date'), False) if p.get('date') else 'popup'}: {p.get('kind')} "
                     f"'{p.get('title')}' -> {p.get('chosen_text')}")
        for n in (log_notes or [])[-5:]:
            L.append(f"- log: {n}")
    if D:
        L.append("")
        L.append(f"## Since the last briefing ({fmt_date(prev.get('date'))})")
        for d in D[:12]:
            L.append(f"- {d['text']}")
    elif later_prev:
        L.append("")
        L.append("## Since the last briefing")
        L.append(f"- the previous briefing ({fmt_date(prev.get('date'))}) is from a later date than now: an older "
                 "save was loaded since, so no changes are compared")
    return {"markdown": "\n".join(L), "facts": F, "attention": A, "deltas": D}


def _date_key(text):
    try:
        p = [int(x) for x in str(text).split(".")]
        return (p[0], p[1], p[2], p[3] if len(p) > 3 else 0)
    except (ValueError, IndexError, TypeError):
        return None


EVENT_TEXT = {
    "declare_war": "{a} declared war on {b}", "peace": "{a} is at peace", "capitulation": "{a} capitulated to {b}",
    "annex": "{a} annexed {b}", "puppet": "{a} became a puppet of {b}", "join_faction": "{a} joined {b}'s faction",
    "leave_faction": "{a} left {b}'s faction", "create_faction": "{a} created a faction ({b} joins)",
    "party": "{a} changed government to {party} ({ideo})", "control": "{a} took control of {name} (owner {owner})",
}


def describe_event(e):
    t = EVENT_TEXT.get(e.get("e"), "{e} {a} {b}")
    try:
        return t.format(**{k: e.get(k, "?") for k in ("a", "b", "e", "party", "ideo", "name", "owner")})
    except (KeyError, IndexError):
        return str(e)
