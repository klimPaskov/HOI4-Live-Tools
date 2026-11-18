"""The navy as a first-class arm: the plan's sea operations held to what each needs (navy.py), run by lint_force.py
after the air rules. The plan states its sea operations in force_design.sea and its ships in force_design.navy; the
lint holds them to the plan's own arithmetic (start fleet plus dockyard output, plane lines, research) and to the
goal: every landing, overseas front and sea region the goal needs is carried out with the means it takes.

  force.sea_required   (error) the checkpoints take land across the sea (a state our capital cannot reach over land
                       or straits) and no sea operation or invasion lands on that landmass; a country the goal or
                       checkpoints capitulate, or an offensive force_design war fights, has its capital across the
                       sea and no landing reaches it; or the plan schedules sea operations (invasions, sea
                       transfers, raids) and force_design has no `sea` entries.
  force.sea_control    (error) an invasion, sea-control or deny operation whose chosen means do not hold the sea
                       regions on its date against the enemy side's fleet: the message gives what each means takes
                       (surface hull IC and the years at the plan's dockyards, naval strike planes, carrier air,
                       submarines) so the plan can provide the strongest one by the date.
  force.sea_screens    (error) screens short of a patrol group per region plus the screening ratio of the battle
                       fleet, or fewer capital ships than carriers, on an operation's date.
  force.sea_lift       (error) convoys short for a landing or an overseas front on its date, a landing larger than
                       the landing techs in hand allow, escorts short of one per CONVOYS_PER_ESCORT convoys.
  force.sea_port       (error) a landing on a state without a naval base and without with_harbor.
  force.sea_patrol     (error) a declared control or deny operation whose regions no navy patrol or strike_force
                       mission covers (invasions[] have navy.sea_control).
  force.sea_air_cover  (error) an operation won from the air whose route regions no air_plan naval_strike or
                       naval_patrol entry flies over by its date (carrier means cover their own route).
  force.sea_raid       (error) a raid operation (or a deny operation by submarine) with fewer submarines than its
                       regions need.
  force.carrier_air    (error) carriers in the plan's operations or lines and no carrier-capable plane line, air row
                       or air_plan entry to fly from them.
  force.navy_unused    (error) a ship line whose role no sea operation uses, or whose first hull is done after the
                       last operation that uses it.
  force.navy_readiness (error) a force_design.navy row the start fleet plus the dockyard lines do not reach by its date.
  force.navy_design    (error) a designer hull line with no start variant and no designs[] entry.
  force.navy_tools     (error) an operation that rests on a lever the gap register marks not executable and gives
                       no `fallback`; (warning) the unvalidated tools it rests on, listed in facts.force.navy.tools in
                       the order the campaign rests on them.
  force.navy_doctrine  (warning) sea operations and no naval doctrine entry or doctrine_defaults.navy.
  force.navy_admirals  (warning) a fleet of navy.fleets with no admiral while the plan has sea operations.
facts.force.navy: the start fleets (ours, each enemy side), the operations with their needs and the date each is met,
the building programme by role and date, the dockyards, the fuel of the patrols and the tools by reliance.
"""
from collections import Counter, OrderedDict

from . import derive
from . import force as fm
from . import navy as nv

_PRED_STATE = ("owner:", "controls_state:", "state_fully_controlled:")


def _fmt(d):
    return derive.fmt(derive.date_of(d)) if d is not None else "undated"


def check(ctx):
    lint = ctx.lint
    ops = nv.operations(ctx)
    prog = nv.Programme(ctx) if (ops or lint.pool_lines(ctx.tl.start)["dockyards"]) else None
    facts = {"ours": nv.fleet(lint, lint.tag), "operations": [], "programme": [], "tools": []}
    lint.facts.setdefault("force", {})["navy"] = facts
    _required(ctx, ops)
    if prog is None:
        return
    status = nv.gap_status()
    reliance = OrderedDict()
    for op in ops:
        n = nv.needs(ctx, prog, op)
        met = nv.met_date(ctx, prog, op) if not n["met"] and op["day"] is not None else op["day"]
        row = {k: v for k, v in op.items() if k not in ("enemy",)}
        row.update({"enemy_fleet": op["enemy"], "needs": n["rows"], "met": n["met"], "met_date": _fmt(met)
                    if met is not None else None, "fuel": nv.fuel_per_day(lint, prog, op) if op["day"] else None})
        tools = []
        for key in nv.gap_rows(op):
            r = status.get(key)
            st = r["status"] if r else "unknown"
            tools.append({"row": key, "status": st})
            if st != "ok":
                reliance.setdefault(key, {"row": key, "status": st, "operations": [],
                                          "owner": (r or {}).get("owner"), "first": op["date"]})
                reliance[key]["operations"].append(op["id"])
        row["tools"] = tools
        facts["operations"].append(row)
        _judge(ctx, op, n, met, tools)
    order = sorted(reliance.values(), key=lambda r: (r["status"] != "not executable", -len(r["operations"]),
                                                      r["first"] or "9999"))
    facts["tools"] = order
    _programme(ctx, prog, ops, facts)
    _carriers(ctx, prog, ops)
    _designs(ctx)
    _command(ctx, ops)


# ---------------------------------------------------------------------------------------------- rules
def _required(ctx, ops):
    lint = ctx.lint
    fd = ctx.fd
    if fd is not None and ops and not [x for x in fd.get("sea") or [] if isinstance(x, dict)]:
        lint.add("error", "force.sea_required", "force_design.sea",
                 f"the plan schedules {len(ops)} sea operation(s) ({', '.join(o['where'] for o in ops[:4])}) and "
                 "force_design has no `sea` entries: name each operation (kind, date, regions, enemies, means) and "
                 "give it the fleet, planes, convoys and landing techs it needs by its date (facts.force.navy)")
    # land across the sea the checkpoints take
    targets = {}
    goal = lint.plan.get("goal") if isinstance(lint.plan.get("goal"), dict) else {}
    for c in [{"id": "goal", "must": goal.get("predicates")}] + list(lint.list_of("checkpoints")):
        for a in fm._atoms(c.get("must")):
            sid = None
            for p in _PRED_STATE:
                if a.startswith(p):
                    v = a[len(p):].split("=")[0].strip()
                    sid = int(v) if v.isdigit() else None
            if a.startswith("province_controlled:"):
                v = a[len("province_controlled:"):].strip()
                sid = lint.world.province_state().get(int(v)) if v.isdigit() else None
            if isinstance(sid, int) and sid not in targets:
                targets[sid] = c.get("id")
    states = lint.world.states()
    landings = [o["target_state"] for o in ops if isinstance(o.get("target_state"), int)]
    own_lands = set()
    for s, r in states.items():
        if r.get("owner") == lint.tag:
            own_lands |= nv.state_land(lint, s)
    seen_land = []
    for sid, cp in targets.items():
        if (states.get(sid) or {}).get("owner") == lint.tag or not nv.across_sea(lint, sid):
            continue
        if any(nv.same_land(lint, sid, t) for t in landings):
            continue
        if any(nv.same_land(lint, sid, s) for s in seen_land):
            continue
        if nv.state_land(lint, sid) & own_lands:
            continue  # we hold land there at the start: an overseas front (navy.overseas)
        seen_land.append(sid)
        lint.add("error", "force.sea_required", "force_design.sea",
                 f"checkpoint {cp} takes state {sid} ({(states.get(sid) or {}).get('owner')}), across the sea from "
                 "our capital, and no sea operation or invasion lands on that land: add a force_design.sea entry "
                 "(kind invasion, from_state, target_state, divisions, the date, the means that win the route) and "
                 "provide its sea control, lift, landing techs and cover by then")
    # a country the plan capitulates (goal, checkpoints, an offensive force_design war) whose capital lies across
    # the sea: its home is taken by landing there (the USA, Britain, Australia, Japan for most of the world)
    for tag, where in _overseas_enemies(ctx):
        cap = _capital(lint, tag)
        if cap is None or (states.get(cap) or {}).get("owner") == lint.tag or not nv.across_sea(lint, cap):
            continue
        if any(nv.same_land(lint, cap, t) for t in landings) or nv.state_land(lint, cap) & own_lands:
            continue
        lint.add("error", "force.sea_required", where,
                 f"the plan defeats {tag}, whose capital (state {cap}) lies across the sea, and no sea operation or "
                 "invasion lands on its home: add the landing (force_design.sea kind invasion with its date, "
                 "divisions and means, an invasions entry) with the sea control, lift, landing techs and cover it "
                 "takes, and the land campaign after it in force_design.wars")


def _capital(lint, tag):
    cache = lint.__dict__.setdefault("_navy_capitals", {})
    if tag not in cache:
        try:
            cache[tag] = derive.capital_state(lint.game, tag, lint.ctx, lint.start)
        except Exception:  # noqa: BLE001
            cache[tag] = None
    return cache[tag]


def _overseas_enemies(ctx):
    """(tag, where) for every country the plan capitulates or fights offensively: capitulated:TAG in the goal and
    the checkpoints, the enemies of offensive force_design.wars entries."""
    lint = ctx.lint
    out = dict(fm.goal_enemies(lint))
    for w in (ctx.fd or {}).get("wars") or []:
        if isinstance(w, dict) and w.get("kind", "offensive") == "offensive":
            for tag in w.get("enemies") or []:
                if isinstance(tag, str) and tag != lint.tag:
                    out.setdefault(tag, f"force_design.wars {w.get('id')}")
    return sorted(out.items())


def _judge(ctx, op, n, met, tools):
    lint = ctx.lint
    where = op["where"]
    if op["day"] is None:
        if op["declared"]:
            lint.add("error", "force.sea_control", where,
                     f"{op['kind']} {op['id']} has no date: give `by` (or a `when` the plan dates) so its needs can be "
                     "sized and met in time")
        return
    if op["unknown_regions"]:
        lint.add("error", "force.sea_patrol", where,
                 f"unknown sea regions {', '.join(op['unknown_regions'])} (strategic sea region names or ids)")
    when = f"{op['kind']} {op['id']} on {op['date']}"
    met_txt = (f"; at the plan's arithmetic its needs hold from {_fmt(met)}" if met is not None and met != op["day"]
               else "")
    for r in n["rows"]:
        if r["ok"]:
            continue
        need = r["need"]
        if need in ("control", "deny"):
            parts = []
            for o in r["options"]:
                if o["means"] == "surface":
                    yrs = o.get("years_at_plan_dockyards")
                    parts.append(f"surface: {o['have']} of {o['want']} hull IC"
                                 + (f" ({yrs:g} more years at the plan's dockyards)" if yrs else
                                    " (no surface line runs then)" if yrs is None else ""))
                elif o["means"] == "air":
                    parts.append(f"air: {o['have']} of {o['want']} naval strike planes")
                elif o["means"] == "carrier":
                    parts.append(f"carrier: {o['have']} of {o['want']} carrier strike planes, decks {o['deck']}")
                elif o["means"] == "submarine":
                    parts.append(f"submarines: {o['have']} of {o['want']}")
            lint.add("error", "force.sea_control", where,
                     f"{when}: the chosen means ({', '.join(r['chosen']) or 'none'}) do not {need} "
                     f"{', '.join(x['name'] for x in op['regions']) or 'the route'} against "
                     f"{', '.join(op['enemy']['tags']) or 'the enemy'} ({op['enemy']['heavy']} capital ships and "
                     f"carriers, {op['enemy']['surface_ic']} surface hull IC at the start) by then. What each means "
                     f"takes: {'; '.join(parts)}{met_txt}. Provide the strongest by the date (lines, factories, "
                     "research and designs dated back from it) and name it in force_design.sea[].means")
        elif need in ("screens", "capitals_per_carrier"):
            lint.add("error", "force.sea_screens", where,
                     f"{when}: {r['have']} against {r['want']}: {r['why']}{met_txt}; plan the destroyer and cruiser "
                     "lines that close it by the date")
        elif need == "carrier_air":
            lint.add("error", "force.carrier_air", where,
                     f"{when}: {r['have']} carrier planes for decks of {r['want']}: {r['why']}")
        elif need == "submarines":
            lint.add("error", "force.sea_raid", where,
                     f"{when}: {r['have']} submarines against {r['want']} ({r['why']}){met_txt}")
        elif need == "port":
            lint.add("error", "force.sea_port", where,
                     f"{when}: state {op['target_state']} has no naval base: {r['why']}; land on or beside a port "
                     "or set with_harbor (a floating harbour, its tech researched)")
        else:
            lint.add("error", "force.sea_lift", where,
                     f"{when}: {need} {r['have']} against {r['want']} ({r['why']}){met_txt}; "
                     + ("research the landing techs before the date or split the landing" if need == "landing" else
                        "a convoy line on the dockyards (or more of them) dated back from it" if need == "lift" else
                        "escort destroyers by the date"))
    if op["declared"] and (op["kind"] in nv.CONTROL_KINDS or op["kind"] == "deny") and op["regions"]:
        covered = _patrolled(lint)
        miss = [x for x in op["regions"] if x["name"].lower() not in covered and str(x["id"]) not in covered]
        if miss and not (op["kind"] == "deny" and "submarine" in op["means"] and _raided(lint, op)):
            lint.add("error", "force.sea_patrol", where,
                     f"{when}: no navy patrol or strike_force mission covers {', '.join(x['name'] for x in miss)}; "
                     f"only patrols build dominance (one task force per region, {nv.PATROL_DAYS}+ days before)")
    if ("air" in op["means"] or "carrier" in op["means"]) and op["regions"] and \
            (op["kind"] in nv.CONTROL_KINDS or op["kind"] == "deny"):
        flown = _air_covered(ctx, op["day"])
        carrier = "carrier" in op["means"]
        miss = [x for x in op["regions"] if x["name"].lower() not in flown and str(x["id"]) not in flown]
        if miss and not carrier:
            lint.add("error", "force.sea_air_cover", where,
                     f"{when}: the sea is won from the air, and no air_plan entry flies naval_strike or naval_patrol "
                     f"over {', '.join(x['name'] for x in miss)} by then; every region of the route needs its "
                     "naval strike wings (based in range) and fighters over them, or a carrier or surface means "
                     "there")
    blocked = [t["row"] for t in tools if t["status"] == "not executable"]
    if blocked and not op.get("fallback"):
        lint.add("error", "force.navy_tools", where,
                 f"{when} rests on {', '.join(blocked)}, which no tool carries yet (gap register): state the "
                 "fallback the tools do carry in force_design.sea[].fallback, and list the row in tool-gaps.md")
    open_rows = [t["row"] for t in tools if t["status"] in ("unvalidated", "not plannable")]
    if open_rows:
        lint.add("warning", "force.navy_tools", where,
                 f"{when} rests on tools with no live receipt yet: {', '.join(open_rows)} (facts.force.navy.tools: "
                 "the live engineers' order before the run)")


def _air_covered(ctx, day):
    """Sea region names and ids an air_plan naval_strike, naval_patrol or port_strike entry flies over by day
    (an entry whose `when` the plan dates after day does not count)."""
    lint = ctx.lint
    out = set()
    regions, by_name = nv._region_index(lint)
    for a in lint.list_of("air_plan"):
        if not isinstance(a, dict) or a.get("mission") not in ("naval_strike", "naval_patrol", "port_strike"):
            continue
        d = fm.predicate_day(lint, ctx.tl, a.get("when"), ctx.wars) if a.get("when") else None
        if d is not None and day is not None and d > day:
            continue
        for key in ("region", "regions", "then_region", "follow_region"):
            v = a.get(key)
            for r in v if isinstance(v, list) else [v]:
                if isinstance(r, (str, int)) and not isinstance(r, bool):
                    out.add(str(r).strip().lower())
                    rid = by_name.get(str(r).strip().lower())
                    if rid is not None:
                        out.add(str(rid))
                        out.add(str((regions.get(rid) or {}).get("name", "")).lower())
    return out


def _patrolled(lint):
    out = set()
    navy = lint.plan.get("navy") if isinstance(lint.plan.get("navy"), dict) else {}
    for fl in navy.get("fleets") or []:
        for m in (fl or {}).get("missions") or [] if isinstance(fl, dict) else []:
            if isinstance(m, dict) and m.get("mission") in ("patrol", "strike_force"):
                for r in m.get("regions") or []:
                    name = r.get("region") if isinstance(r, dict) else r
                    if isinstance(name, (str, int)) and not isinstance(name, bool):
                        out.add(str(name).strip().lower())
    return out


def _raided(lint, op):
    navy = lint.plan.get("navy") if isinstance(lint.plan.get("navy"), dict) else {}
    names = {x["name"].lower() for x in op["regions"]}
    for fl in navy.get("fleets") or []:
        for m in (fl or {}).get("missions") or [] if isinstance(fl, dict) else []:
            if isinstance(m, dict) and m.get("mission") == "convoy_raiding":
                got = {str(r.get("region") if isinstance(r, dict) else r).strip().lower() for r in m.get("regions") or []}
                if names & got:
                    return True
    return False


def _programme(ctx, prog, ops, facts):
    lint, g = ctx.lint, ctx.g
    dated = sorted({o["day"] for o in ops if o["day"] is not None})
    rows = ctx.fd_list("navy")
    days = sorted(set(dated) | {derive.day(r["ready"]) for r in rows if isinstance(r.get("ready"), str)
                                and derive.day(r["ready"]) is not None})
    for d in days:
        s = prog.ships(d)
        facts["programme"].append({"date": _fmt(d), "dockyards": prog.dockyards(d),
                                   "ships": {k: round(v) for k, v in s.items() if v},
                                   "surface_hull_ic": round(prog.surface_ic(d))})
    # force_design.navy rows against the forecast
    for i, r in enumerate(rows):
        role = nv.ROW_KIND.get(r.get("kind"))
        d = derive.day(r["ready"]) if isinstance(r.get("ready"), str) else None
        if not role or d is None or not isinstance(r.get("count"), int):
            continue
        have = prog.ships(d).get(role, 0)
        if have < nv.READY_SLACK * r["count"]:
            lint.add("error", "force.navy_readiness", f"force_design.navy[{i}]",
                     f"{r['count']} {r.get('kind')} by {r['ready']}, but the start fleet and the dockyard lines give "
                     f"about {round(have)}: give the line the dockyards that make them by then (one line per hull "
                     "type), or date the row when they are done")
    # lines no operation uses, or done too late
    uses = Counter()
    last = {}
    for o in ops:
        roles = {"screen"} if o["kind"] in nv.CONTROL_KINDS + ("deny", "escort", "overseas_supply") else set()
        if "surface" in o["means"]:
            roles |= {"capital", "screen"}
        if "carrier" in o["means"]:
            roles |= {"carrier", "capital", "screen"}
        if o["kind"] == "raid" or "submarine" in o["means"]:
            roles.add("submarine")
        if o["kind"] in ("invasion", "overseas_supply", "escort"):
            roles.add("convoy")
        for r in roles:
            uses[r] += 1
            if o["day"] is not None:
                last[r] = max(last.get(r, 0), o["day"])
    rows_roles = {nv.ROW_KIND.get(r.get("kind")) for r in rows}
    seen = set()
    for i, x in enumerate(lint.list_of("production")):
        eq = x.get("equipment")
        if not isinstance(eq, str) or x.get("remove") or lint.factory_pool(eq) != "dockyards" or eq in seen:
            continue
        seen.add(eq)
        role = nv.role_of(g, eq) or ("convoy" if "convoy" in (derive.equipment_types(g).get(eq) or ()) else None)
        if not role or role == "convoy":
            continue
        if not uses[role] and role not in rows_roles:
            lint.add("error", "force.navy_unused", f"production[{i}]",
                     f"{eq} ({role}) has a dockyard line but no sea operation's means and no force_design.navy row "
                     "uses that role: tie it to the operation it serves (force_design.sea[].means), or give the "
                     "dockyards to what the operations need")
            continue
        first = prog.first_hull.get(eq)
        if role in last and (first is None or first > last[role]):
            lint.add("error", "force.navy_unused", f"production[{i}]",
                     f"{eq} ({role}): the line's first hull is done "
                     f"{_fmt(first) if first else 'after the forecast'}, after the last operation that uses "
                     f"{role}s ({_fmt(last[role])}): start it earlier with enough dockyards, or build what is done "
                     "in time")


def _carriers(ctx, prog, ops):
    lint, g = ctx.lint, ctx.g
    lines = [eq for eq in lint.pool_lines(ctx.tl.forecast_end())["dockyards"] if nv.role_of(g, eq) == "carrier"]
    used = any("carrier" in o["means"] for o in ops)
    if not lines and not used:
        return
    planes = any(nv.carrier_plane(g, x.get("equipment")) for x in lint.list_of("production")
                 if isinstance(x.get("equipment"), str) and not x.get("remove"))
    planes = planes or any(nv.carrier_plane(g, r.get("equipment")) for r in ctx.fd_list("air")
                           if isinstance(r.get("equipment"), str))
    planes = planes or any(nv.carrier_plane(g, a.get("airframe")) for a in lint.list_of("air_plan")
                           if isinstance(a.get("airframe"), str))
    if not planes:
        lint.add("error", "force.carrier_air", "production",
                 ("the plan builds carriers (" + ", ".join(lines) + ")" if lines else
                  "the plan's sea operations use carriers")
                 + " and no carrier-capable plane line, air row or air_plan entry gives them air groups: a carrier "
                   "fighter and a carrier naval bomber line sized to the decks "
                   f"({nv.deck_size(lint):g} deck points x CARRIER_SIZE_STAT_INCREMENT a carrier)")


def _designs(ctx):
    lint = ctx.lint
    w = lint.world
    variants = {v.get("type") for v in w.country(lint.tag).get("variants") or []}
    designed = {d.get("base") for d in lint.list_of("designs")} | {d.get("production") for d in lint.list_of("designs")}
    defs = w.equipment_defs()
    seen = set()
    for i, x in enumerate(lint.list_of("production")):
        eq = x.get("equipment")
        if not isinstance(eq, str) or x.get("remove") or eq in seen or eq not in defs:
            continue
        seen.add(eq)
        try:
            designer = w.eq_kind(eq) == "ship" and bool(defs[eq].get("slots"))
        except Exception:  # noqa: BLE001
            designer = False
        if designer and eq not in variants and eq not in designed and not x.get("design"):
            lint.add("error", "force.navy_design", f"production[{i}]",
                     f"{eq} is a designer hull with no start variant and no designs[] entry on it: design it for its "
                     "role (torpedo destroyer, light cruiser that strips screens, carrier with most hangars and AA, "
                     "raider submarine) with its navy experience before the line")


def _command(ctx, ops):
    lint = ctx.lint
    if not ops:
        return
    if not any(str(e.get("folder")) in ("navy", "naval") for e in lint.list_of("doctrines")) and \
            not (isinstance(lint.plan.get("doctrine_defaults"), dict) and
                 (lint.plan["doctrine_defaults"].get("navy") or lint.plan["doctrine_defaults"].get("naval"))):
        lint.add("warning", "force.navy_doctrine", "doctrines",
                 "the plan has sea operations and no naval doctrine: a navy doctrines entry (grand doctrine and the "
                 "tracks that fit the means: fleet in being for capital and carrier fleets, trade interdiction for "
                 "submarines, base strike for carriers) or doctrine_defaults.navy")
    navy = lint.plan.get("navy") if isinstance(lint.plan.get("navy"), dict) else {}
    admirals = navy.get("admirals")
    named = set(admirals) if isinstance(admirals, dict) else {a.get("fleet") for a in admirals or []
                                                              if isinstance(a, dict)}
    for i, fl in enumerate(navy.get("fleets") or []):
        if isinstance(fl, dict) and not fl.get("admiral") and fl.get("fleet") not in named:
            lint.add("warning", "force.navy_admirals", f"navy.fleets[{i}]",
                     f"fleet {fl.get('fleet')} has no admiral while the plan has sea operations (NA30: one per fleet)")
