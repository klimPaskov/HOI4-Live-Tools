"""Deep country state from a plain-text save (the slow, complete source behind hoi4_state and hoi4_briefing).

country_state(sf, tag, sections) reads only the blocks it needs from an indexed SaveFile (see savefile.py) and
returns plain dicts. Names are resolved by the caller through a `names(keys)` callback (localisation), so this
module stays independent of the game install and is unit-testable with a synthetic save.

Sections and their save sources (HOI4 1.19 text saves):
    overview      countries/TAG scalars (stability, war_support, command_power, research_slot, capital, major)
    politics      countries/TAG/politics (ruling party, popularity, ideas, elections, political power)
    focus         countries/TAG/focus (current, progress in days, paused)
    research      countries/TAG/technology (slots in use, points spent, researched count, recent completions)
    production    countries/TAG/production military_lines / naval_lines (variant -> equipment type via equipments)
    construction  countries/TAG/production/general_lines/building (template, state, progress, factories)
    military      countries/TAG/units (divisions, strength, organisation), stockpile, convoys, fuel
    diplomacy     countries/TAG/diplomacy/active_relations (wars with score breakdown, subjects, guarantees,
                  access, market rights) and the top-level faction blocks
    decisions     countries/TAG/decision_status (timed decisions and missions in progress)
    states        states/N whose owner is TAG (buildings, manpower)
"""

import datetime
import re

from .savefile import clean

SECTIONS = ("overview", "politics", "focus", "research", "production", "construction", "military", "diplomacy",
            "decisions", "states")
RELATION_KEYS = ("war_relation", "puppet", "guarantee", "military_access", "non_aggression_pact",
                 "market_access_rights", "lend_lease", "expeditionary_force", "docking_rights", "embargo")


def _one(sf, path):
    got = sf.get(path)
    if not got:
        return None
    return clean(next(iter(got.values())))


def _num(v, default=None):
    if isinstance(v, (int, float)):
        return v
    try:
        return float(v)
    except (TypeError, ValueError):
        return default


def _list(v):
    if v is None:
        return []
    return v if isinstance(v, list) else [v]


def parse_date(text):
    """'1936.1.11.12' -> datetime (hour optional)."""
    try:
        parts = [int(x) for x in str(text).split(".")]
        return datetime.datetime(parts[0], parts[1], parts[2]) + datetime.timedelta(hours=parts[3] if len(parts) > 3 else 0)
    except (ValueError, IndexError):
        return None


def equipment_names(sf, ids):
    """{variant id: equipment type token} from the top-level equipments block (regex over its raw text)."""
    ids = {int(i) for i in ids if i is not None}
    if not ids:
        return {}
    try:
        text = sf.span_text(("equipments",))
    except KeyError:
        return {}
    out = {}
    for m in re.finditer(rb"\n\t([A-Za-z0-9_\-]+)=\{\s*id=\{ id=(\d+) type=\d+ \}", text):
        i = int(m.group(2))
        if i in ids:
            out[i] = m.group(1).decode("utf-8", "replace")
    return out


CHAR_TOKEN = re.compile(rb"id=\{ id=(\d+) type=\d+ \}\s*token=\"([^\"]+)\"")


def appointed_advisors(sf, tag, names=None):
    """[{slot, character, name}] from countries/TAG/characters/appointed_advisors; character ids are resolved to
    their tokens in the top-level character_manager block (HOI4 1.19 saves)."""
    names = names or (lambda keys: {})
    ch = _one(sf, f"countries/{tag}/characters") or {}
    rows = []
    for a in _list(ch.get("appointed_advisors")):
        for item in (a if isinstance(a, list) else [a]):
            if isinstance(item, dict) and isinstance(item.get("character"), dict):
                rows.append({"slot": item.get("slot"), "id": item["character"].get("id")})
    if not rows:
        return []
    ids = {int(r["id"]) for r in rows if r["id"] is not None}
    tokens = {}
    try:
        text = sf.span_text(("character_manager",))
        for m in CHAR_TOKEN.finditer(text):
            i = int(m.group(1))
            if i in ids:
                tokens[i] = m.group(2).decode("utf-8", "replace")
    except KeyError:
        pass
    nm = names(list(tokens.values()))
    out = []
    for r in rows:
        tok = tokens.get(int(r["id"])) if r["id"] is not None else None
        out.append({"slot": r["slot"], "character": tok, "name": (nm.get(tok) or tok) if tok else None})
    return out


def country_state(sf, tag, sections=SECTIONS, names=None, idea_slots=None, tech_costs=None, limit=60):
    """The requested sections for one country. names(keys) -> {key: text}; idea_slots: {idea: category};
    tech_costs: {tech: research_cost}."""
    names = names or (lambda keys: {})
    sections = [s for s in SECTIONS if s in set(sections)]
    header = sf.header()
    root = f"countries/{tag}"
    if (("countries", tag)) not in sf.index():
        return {"error": f"no country {tag} in this save"}
    out = {"tag": tag, "save_date": header.get("date"), "player": header.get("player")}
    scal = sf.keys(root).get("scalars", {})
    pol = _one(sf, root + "/politics") or {}
    ruling = pol.get("ruling_party")
    want = [tag, f"{tag}_{ruling}" if ruling else None, f"{tag}_{ruling}_DEF" if ruling else None]
    if "overview" in sections:
        cap = scal.get("capital")
        nm = names([k for k in want if k] + ([f"STATE_{cap}"] if cap else []))
        out["overview"] = {
            "name": nm.get(f"{tag}_{ruling}") or nm.get(tag) or tag,
            "major": scal.get("major") == "yes",
            "stability_base": _num(scal.get("stability")), "war_support_base": _num(scal.get("war_support")),
            "political_power": _num(pol.get("political_power")), "command_power": _num(scal.get("command_power")),
            "research_slots": _num(scal.get("research_slot")), "capital": cap,
            "capital_name": nm.get(f"STATE_{cap}") if cap else None, "focus_tree": scal.get("focus_tree"),
            "ruling_party": ruling, "note": "stability and war support here are the saved base values; the "
                                            "screen and telemetry show the effective values with modifiers"}
    if "politics" in sections:
        parties = pol.get("parties") or {}
        pops = sorted(((p, _num(v.get("popularity"), 0)) for p, v in parties.items() if isinstance(v, dict)),
                      key=lambda x: -x[1])
        ideas = _list(pol.get("ideas"))
        slots = idea_slots or {}
        nm = names(ideas + [p for p, _ in pops])
        groups = {}
        for i in ideas:
            groups.setdefault(slots.get(i, "other"), []).append({"id": i, "name": nm.get(i, i)})
        laws = {k: v for k, v in groups.items() if k in ("economy", "trade_laws", "mobilization_laws")}
        advisors = appointed_advisors(sf, tag, names)
        out["politics"] = {
            "ruling_party": ruling, "ruling_party_name": nm.get(ruling, ruling),
            "popularity": {p: v for p, v in pops},
            "political_power": _num(pol.get("political_power")),
            "last_election": pol.get("last_election"), "election_frequency_months": _num(pol.get("election_frequency")),
            "elections_allowed": pol.get("elections_allowed") == "yes",
            "laws": {k: v[0] for k, v in laws.items()},
            "advisors": advisors,
            "spirits": groups.get("country", []),
            "other_ideas": {k: v for k, v in groups.items() if k not in laws and k != "country"}}
    if "focus" in sections:
        f = _one(sf, root + "/focus") or {}
        cur = f.get("current")
        done = _list(f.get("completed"))
        nm = names(([cur] if cur else []) + done[-5:])
        out["focus"] = {"current": cur, "current_name": nm.get(cur, cur) if cur else None,
                        "progress_days": _num(f.get("progress")), "paused": f.get("paused") == "yes",
                        "completed": done[-limit:], "tree": scal.get("focus_tree"),
                        "recently_completed": [{"id": x, "name": nm.get(x) or x} for x in reversed(done[-5:])]}
        if not cur:
            out["focus"]["note"] = "no national focus selected"
    if "research" in sections:
        tech = _one(sf, root + "/technology") or {}
        techs = tech.get("technologies") or {}
        slots = tech.get("slots") or {}
        active = [k for k in slots if k != "empty"]
        empty = len(_list(slots.get("empty")))
        costs = tech_costs or {}
        save_dt = parse_date(header.get("date"))
        recent = []
        dated = [(parse_date(v.get("date")), k) for k, v in techs.items() if isinstance(v, dict) and v.get("level")]
        dated = [(d, k) for d, k in dated if d and d.year > 1]
        start = min((d for d, _ in dated), default=None)      # techs a country starts with share the start date
        for d, k in dated:
            if save_dt and d != start and (save_dt - d).days <= 60:
                recent.append((d, k))
        recent.sort(reverse=True)
        nm = names(active + [k for _, k in recent[:10]])
        slots_out = []
        for k in active:
            pts = _num((techs.get(k) or {}).get("research_points"), 0)
            cost = costs.get(k)
            row = {"tech": k, "name": nm.get(k, k), "research_points": pts}
            if cost:
                row["approx_progress"] = round(min(0.99, pts / (cost * 100.0)), 2)
            slots_out.append(row)
        out["research"] = {
            "slots_total": _num(scal.get("research_slot")), "researching": slots_out, "idle_slots": empty,
            "researched": len([1 for v in techs.values() if isinstance(v, dict) and v.get("level")]),
            "recently_completed": [{"tech": k, "name": nm.get(k, k), "date": d.strftime("%Y.%m.%d")}
                                   for d, k in recent[:10]]}
    if "production" in sections or "military" in sections:
        prod = _one(sf, root + "/production") or {}
    if "production" in sections:
        lines = []
        for kind in ("military_lines", "naval_lines"):
            for l in _list(prod.get(kind)):
                if not isinstance(l, dict):
                    continue
                vid = ((l.get("equipment_variant_index") or {}).get("id"))
                lines.append({"kind": kind[:-6], "variant": vid, "factories": _num(l.get("active_factories"), 0),
                              "requested": _num(l.get("requested_factories"), 0),
                              "efficiency": _num(_list(l.get("factory_efficiencies"))[0]) if l.get("factory_efficiencies") else None,
                              "priority": _num(l.get("priority")), "amount": _num(l.get("amount")),
                              "unit_cost": _num(l.get("cost")), "produced_partial": _num(l.get("produced")),
                              "short_of": [r.get("resource") for r in _list(l.get("resources"))
                                           if isinstance(r, dict) and _num(r.get("need"), 0) > 0]})
        eqn = equipment_names(sf, [l["variant"] for l in lines])
        nm = names(list(set(eqn.values())))
        for l in lines:
            t = eqn.get(l["variant"])
            l["equipment"] = t
            l["name"] = nm.get(t, t)
        lines.sort(key=lambda l: (l["priority"] if l["priority"] is not None else 999))
        out["production"] = {"lines": lines[:limit],
                             "military_factories_assigned": sum(l["factories"] for l in lines if l["kind"] == "military"),
                             "dockyards_assigned": sum(l["factories"] for l in lines if l["kind"] == "naval"),
                             "short_of_resources": sorted({r for l in lines for r in l["short_of"] if r})}
    if "construction" in sections:
        prod_c = _one(sf, root + "/production/general_lines") or {}
        rows = []
        for b in _list(prod_c.get("building")):
            if not isinstance(b, dict):
                continue
            bld = b.get("building") or {}
            cost = _num(b.get("cost"), 0)
            done = _num(b.get("produced"), 0)
            rows.append({"building": bld.get("template"), "state": bld.get("location"),
                         "province": bld.get("province"), "amount": _num(b.get("amount"), 1),
                         "factories": _num(b.get("active_factories"), 0), "progress": round(done / cost, 3) if cost else None,
                         "priority": _num(b.get("priority"))})
        nm = names([f"STATE_{r['state']}" for r in rows if r["state"] is not None] +
                   list({r["building"] for r in rows if r["building"]}))
        for r in rows:
            r["state_name"] = nm.get(f"STATE_{r['state']}")
            r["building_name"] = nm.get(r["building"], r["building"])
        rows.sort(key=lambda r: r["priority"] if r["priority"] is not None else 999)
        out["construction"] = {"queue": rows[:limit], "queued": len(rows)}
    if "military" in sections:
        units = _one(sf, root + "/units") or {}
        divs = [d for d in _list(units.get("division")) if isinstance(d, dict)]
        strengths = [_num(d.get("strength"), 0) for d in divs]
        orgs = [_num(d.get("organisation"), 0) for d in divs]
        stock = []
        for e in _list((prod.get("equipments") or {}).get("equipment")):
            if isinstance(e, dict):
                stock.append(((e.get("id") or {}).get("id"), _num(e.get("amount"), 0)))
        eqn = equipment_names(sf, [i for i, _ in stock])
        agg = {}
        for i, amt in stock:
            t = eqn.get(i, f"variant_{i}")
            agg[t] = agg.get(t, 0) + amt
        nm = names(list(agg))
        fuel = _one(sf, root + "/fuel_status") or {}
        conv = _one(sf, root + "/convoys") or {}
        convoys = None
        for e in _list((conv.get("equipment") or {}).get("equipment")):
            if isinstance(e, dict):
                convoys = (convoys or 0) + _num(e.get("amount"), 0)
        from . import military as mil_mod
        arm = mil_mod.armies(sf, tag, names)
        armies = [{"name": a["name"], "divisions": a["divisions"], "leader": a.get("leader"),
                   "orders": [o["type"] for o in a["orders"]], "executing_divisions": a["executing_divisions"]}
                  for a in arm["armies"]]
        out["military"] = {
            "armies": armies, "army_groups": [{"name": g["name"], "armies": g["army_names"], "leader": g.get("leader")}
                                              for g in arm["army_groups"]],
            "unassigned_divisions": arm["unassigned_divisions"],
            "divisions": len(divs),
            "avg_strength": round(sum(strengths) / len(strengths), 1) if strengths else None,
            "avg_organisation": round(sum(orgs) / len(orgs), 1) if orgs else None,
            "stockpile": sorted(({"equipment": k, "name": nm.get(k, k), "amount": round(v)} for k, v in agg.items()),
                                key=lambda r: -r["amount"])[:limit],
            "fuel": _num(fuel.get("fuel")), "max_fuel": _num(fuel.get("max_fuel")),
            "convoys_total": convoys, "ships": _num(scal.get("num_ships"))}
    if "diplomacy" in sections:
        rel = (_one(sf, root + "/diplomacy/active_relations") or {})
        wars, links = [], []
        for other, r in rel.items():
            if not isinstance(r, dict):
                continue
            for key in RELATION_KEYS:
                v = r.get(key)
                if not isinstance(v, dict):
                    continue
                if key == "war_relation":
                    us_first = v.get("first") == tag
                    mine = v.get("war_score_first_vs_second" if us_first else "war_score_second_vs_first") or {}
                    theirs = v.get("war_score_second_vs_first" if us_first else "war_score_first_vs_second") or {}
                    score = lambda d: round(sum(_num(x, 0) for k, x in d.items()
                                                if k not in ("first", "second") and not isinstance(x, list)), 2)
                    cap = lambda d: len(_list(d.get("captured_provinces"))) if isinstance(d, dict) else 0
                    wars.append({"enemy": other, "since": v.get("start_date"),
                                 "provinces_captured_by_us": cap(mine), "provinces_captured_by_them": cap(theirs),
                                 "we_started": (v.get("first_was_instigator") == "yes") == us_first,
                                 "our_casualties": _num(v.get("first_casualties" if us_first else "second_casualties"), 0),
                                 "their_casualties": _num(v.get("second_casualties" if us_first else "first_casualties"), 0),
                                 "score_breakdown_us": score(mine), "score_breakdown_them": score(theirs)})
                else:
                    links.append({"type": key, "with": other,
                                  "direction": "from_us" if v.get("first") == tag else "to_us",
                                  "since": v.get("start_date"),
                                  **({"autonomy": v.get("autonomy_state")} if key == "puppet" else {})})
        factions = []
        idx = sf.index()
        for k in idx:
            if len(k) == 1 and re.match(r"^faction(\[\d+\])?$", k[0]):
                fb = _one(sf, k[0]) or {}
                members = _list(fb.get("members"))
                if tag in members or len(factions) < 12:
                    factions.append({"members": members, "ideology": fb.get("ideology"),
                                     "template": fb.get("template"), "ours": tag in members})
        nm = names([w["enemy"] for w in wars] + [l["with"] for l in links])
        for w in wars:
            w["enemy_name"] = nm.get(w["enemy"], w["enemy"])
        out["diplomacy"] = {"wars": wars, "at_war": bool(wars), "relations": links[:limit],
                            "faction": next((f for f in factions if f["ours"]), None),
                            "factions": [f for f in factions if not f["ours"]]}
    if "decisions" in sections:
        ds = _one(sf, root + "/decision_status") or {}
        timed = []
        for key in ("active_timed_decision", "active_mission", "active_timed_mission"):
            for d in _list(ds.get(key)):
                if isinstance(d, dict) and d.get("decision"):
                    timed.append({"kind": key.replace("active_", ""), "id": d.get("decision"),
                                  "days_left": _num(d.get("days")), "state": d.get("state")})
        taken = [d for d in _list(ds.get("decisions_taken")) if isinstance(d, str)]
        removing = [{"id": d.get("decision"), "days_left": _num(d.get("days"))}
                    for d in _list(ds.get("decision_to_remove")) if isinstance(d, dict) and d.get("decision")]
        nm = names([t["id"] for t in timed] + taken + [r["id"] for r in removing])
        for t in timed:
            t["name"] = nm.get(t["id"], t["id"])
        for r in removing:
            r["name"] = nm.get(r["id"], r["id"])
        avail = [d for d in _list(ds.get("active_targeted_decision")) if isinstance(d, dict)]
        out["decisions"] = {"active": timed[:limit],
                            "taken": [{"id": d, "name": nm.get(d, d)} for d in taken[-limit:]],
                            "in_effect": removing[:limit], "targeted_available": len(avail),
                            "note": "decisions that can be taken now are shown in the decisions panel (shift+q); "
                                    "the save keeps only timed and targeted ones"}
    if "states" in sections:
        hits = sf.find(rf'\n\t\towner="{re.escape(tag)}"', inside="states", limit=2000)
        ids = sorted({h["path"].split("/")[1] for h in hits if h["path"].startswith("states/")}, key=lambda s: int(s) if s.isdigit() else 0)
        rows = []
        for sid in ids[:400]:
            st = _one(sf, f"states/{sid}") or {}
            b = st.get("buildings") or {}
            lvl = lambda k: _num((b.get(k) or {}).get("level"), 0) if isinstance(b.get(k), dict) else 0
            mp = st.get("manpower_pool") or {}
            rows.append({"id": int(sid) if sid.isdigit() else sid, "category": st.get("state_category"),
                         "civ": lvl("industrial_complex"), "mil": lvl("arms_factory"), "dock": lvl("dockyard"),
                         "infra": lvl("infrastructure"), "manpower": _num(mp.get("total")),
                         "demilitarized": st.get("demilitarized") == "yes"})
        nm = names([f"STATE_{r['id']}" for r in rows])
        for r in rows:
            r["name"] = nm.get(f"STATE_{r['id']}")
        out["states"] = {"owned": len(rows), "list": rows if len(rows) <= limit else
                         sorted(rows, key=lambda r: -(r["civ"] + r["mil"] + r["dock"]))[:limit],
                         "totals": {"civ": sum(r["civ"] for r in rows), "mil": sum(r["mil"] for r in rows),
                                    "dock": sum(r["dock"] for r in rows)}}
    return out
