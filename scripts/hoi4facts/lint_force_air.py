"""The air force as a first-class arm: what makes the planned wings work and how they are used, run by lint_force.py
after its own rules (the needs come from force.py air_rows).

Per war whose fighters are required (force.py: fighters the plan can pay, at least MIN_PLANES):

  force.air_none       (error) a war with no air arm the plan can pay (fighters below MIN_PLANES) and no written
                       reason in force_design.air_why: a small fighter and support arm it can afford and fuel, or
                       the reason for none.
  force.air_use        (error) no air_plan entry flies air superiority by the war's first day (offensive wars: the
                       default war missions only reach wings already on the first border state); no close air
                       support entry when the war needs CAS; (warning) an offensive war whose superiority wings
                       never move with the front (follow_region, or then_region with switch_when).
  force.air_lost       (error) the air superiority rule switched off (manage.off) while the plan fights for the air:
                       nothing acts when superiority is lost.
  force.air_deploy     (error) more planes of a kind needed than the start wings hold and no air_plan entry with
                       `deploy` founds wings of that kind: the produced planes stay in reserve.
  force.air_losses     (error) a kind the war needs with no production line of it running on the war's first day:
                       nothing replaces the losses.
  force.air_bases      (error) an air_plan base_state owned by another country on the entry's date (and its owner not
                       capitulated by then); the war's fighters and CAS above the capacity of our air bases on the
                       border with its targets (levels at the start plus the queued air bases x
                       AIRBASE_CAPACITY_MULT).
  force.air_research   (error) an air row or air_plan airframe whose unlocking tech the research plan finishes after
                       the date it must fly (a plane design waits on its airframe).
  force.air_design     (error) a designer airframe flown or produced with no start variant of it and no designs[]
                       entry on it: the line cannot be set.
  force.air_design_blind (warning) a plane designs[] entry on a designer airframe (module slots) with no modules
                       and no upgrades named: the game keeps the airframe's empty or default modules, a blind design.
                       Name every slot from computed stats (facts.py design-best / design-plan) against the role's
                       requirement (range for the theatre: facts.py air-cover), or write `modules_why`.
  force.air_doctrine   (error) no air doctrine entry and no doctrine_defaults.air while the plan fights for the air.
  force.air_command    (warning) no air chief in the military staff.
  force.air_defence    (warning) the front flies bombers and the plan builds no radar station and no state anti-air.
  force.air_fuel       (error) no fuel plan (refineries, oil trade, auto_import with a fuel floor) while the wings
                       fly; facts.force.air_fuel gives the sortie fuel against the fuel the start oil gives.
  force.air_resources  (warning) the plane lines need more aluminium or rubber than the country has, with no trade
                       or auto_import to cover it.
"""
from collections import Counter

from . import derive
from . import force as fm

AA_ENEMY_BOMBERS = 100  # enemy bombers and CAS from which radar and state anti-air are planned
BOMBERS = ("tactical_bomber", "strategic_bomber", "cas")


def _when_day(ctx, pred):
    return ctx.tl.start if pred is None else fm.predicate_day(ctx.lint, ctx.tl, pred, ctx.wars)


def check(ctx):
    lint = ctx.lint
    fought = []
    for w in ctx.wars:
        f = ctx.need_of(w, "fighters")
        if w["start"] is None or f is None:
            continue
        if not f["required"]:
            if f["want"] and not (ctx.fd or {}).get("air_why"):
                lint.add("error", "force.air_none", "force_design.air_why",
                         f"{w['id']} ({w['start_date']}): {f['why']}; the plan pays {f['planes']} fighters by then. "
                         "Plan a small fighter and support arm it can afford and fuel (later rows of force_design.air), "
                         "or write why it has none in force_design.air_why")
            continue
        fought.append((w, f))
    if not fought:
        return
    plans = [(i, a) for i, a in enumerate(lint.list_of("air_plan"))]
    off = (lint.plan.get("manage") or {}).get("off") if isinstance(lint.plan.get("manage"), dict) else None
    war_missions = (lint.plan.get("air") or {}).get("war_missions") is True \
        if isinstance(lint.plan.get("air"), dict) else False
    for w, f in fought:
        day = w["start"]
        sup = [(i, a) for i, a in plans if a.get("mission") == "air_superiority"
               and (_when_day(ctx, a.get("when")) or 10 ** 9) <= day]
        if not sup and (w["kind"] == "offensive" or not war_missions):
            lint.add("error", "force.air_use", "air_plan",
                     f"{w['id']} starts {w['start_date']} and no air_plan entry flies air_superiority by then"
                     + (" (air.war_missions only sends wings already based on the first border state)"
                        if war_missions else "")
                     + f": name the fighter wings, the region over the war's axes, the base, and the date "
                       f"({f['planes']} fighters planned)")
        if w["kind"] == "offensive" and sup and not any(a.get("follow_region") is True or a.get("then_region")
                                                        for _, a in sup):
            lint.add("warning", "force.air_use", f"air_plan[{sup[0][0]}]",
                     f"{w['id']}: the superiority wings never move with the front (follow_region, or then_region "
                     "with switch_when): the air over the advance is left behind")
        cas = ctx.need_of(w, "cas")
        if cas and cas["required"] and not any(a.get("mission") == "cas" and
                                               (_when_day(ctx, a.get("when")) or 10 ** 9) <= day for _, a in plans):
            lint.add("error", "force.air_use", "air_plan",
                     f"{w['id']} needs {cas['planes']} close air support planes over its breakthrough axes and no "
                     "air_plan entry flies cas by its first day")
    if isinstance(off, list) and "air_superiority" in off:
        lint.add("error", "force.air_lost", "manage.off",
                 "the air superiority rule is off while the plan fights for the air: nothing acts when the enemy "
                 "takes the air over a front region (manage.enemy_air_superiority, air_hold_days)")
    _deploy_and_losses(ctx, fought)
    _bases(ctx, fought)
    _research_and_designs(ctx)
    _doctrine_command_defence(ctx, fought)
    _fuel_and_resources(ctx, fought)


def _start_planes(ctx):
    c = Counter()
    for a in ctx.lint.world.oob(ctx.lint.tag)["air"]:
        c[fm.plane_kind(ctx.g, a["type"]) or "other"] += int(a.get("amount") or 0)
    return c


def _deploy_and_losses(ctx, fought):
    lint, g = ctx.lint, ctx.g
    start = _start_planes(ctx)
    deploy = {fm.plane_kind(g, a.get("airframe")) for a in lint.list_of("air_plan")
              if (a.get("deploy") is True or isinstance(a.get("deploy"), dict)) and a.get("airframe")}
    seen = set()
    for w, f in fought:
        rows = [("fighter", f)] + [({"tactical_bombers": "tactical_bomber"}.get(r["role"], r["role"]), r)
                                   for r in (ctx.need_of(w, "cas"), ctx.need_of(w, "transport"),
                                             ctx.need_of(w, "tactical_bombers")) if r and r["required"]]
        for kind, r in rows:
            if r["planes"] > start.get(kind, 0) and kind not in deploy and kind not in seen:
                seen.add(kind)
                lint.add("error", "force.air_deploy", "air_plan",
                         f"{w['id']} needs {r['planes']} {kind} planes and the start wings hold {start.get(kind, 0)}; "
                         f"no air_plan entry with deploy founds {kind} wings, so the planes the lines make stay in "
                         "reserve (deploy {min_planes, max_wings, wing_size} with a base)")
            lines = {eq for eq in lint.pool_lines(w["start"])["military"] if fm.plane_kind(g, eq) == kind}
            if not lines:
                lint.add("error", "force.air_losses", "production",
                         f"{w['id']} ({w['start_date']}) flies {kind} wings and no {kind} line runs then: nothing "
                         "replaces the losses (a line of the role's airframe through the war)")


def _bases(ctx, fought):
    lint, g = ctx.lint, ctx.g
    states = lint.world.states()
    caps = fm.capitulation_days(lint)
    for i, a in enumerate(lint.list_of("air_plan")):
        sid = a.get("base_state")
        if not isinstance(sid, int):
            continue
        owner = (states.get(sid) or {}).get("owner")
        day = _when_day(ctx, a.get("when"))
        if owner and owner != lint.tag and day is not None and not (owner in caps and caps[owner] <= day):
            lint.add("error", "force.air_bases", f"air_plan[{i}] {a.get('id', '')}".strip(),
                     f"base_state {sid} belongs to {owner} on {derive.fmt(derive.date_of(day))} and the plan does "
                     f"not see {owner} capitulated by then: the wing cannot be based there (base it on our side of "
                     "the border, follow_region once the land is taken)")
    levels = derive.air_base_levels(g, lint.ctx, lint.start)
    kinds = derive.air_base_buildings(g)
    queued = Counter(s for _, b, s in lint.queued() if b in kinds)
    mult = float(derive.define(g, "AIRBASE_CAPACITY_MULT", 200))
    try:
        nb = lint.world.neighbours(lint.tag)
    except Exception:  # noqa: BLE001
        nb = {}
    rows = []
    for w, f in fought:
        ours = sorted({s for t in w["targets"] for s in (nb.get(t) or {}).get("ours") or []})
        if not ours:
            continue
        cap = sum((levels.get(s, 0) + queued[s]) * mult for s in ours)
        cas = ctx.need_of(w, "cas")
        planes = f["planes"] + (cas["planes"] if cas and cas["required"] else 0)
        rows.append({"war": w["id"], "border_states": ours, "capacity": cap, "planes": planes})
        if planes > cap:
            lint.add("error", "force.air_bases", "construction",
                     f"{w['id']}: {planes} fighters and CAS to base on the border with {', '.join(w['targets'])} "
                     f"and our air bases there (states {', '.join(map(str, ours[:8]))}) hold {cap:g} "
                     f"({mult:g} a level): queue air bases there before the war")
    lint.facts.setdefault("force", {})["air_bases"] = rows


def _research_and_designs(ctx):
    lint, g = ctx.lint, ctx.g
    unlocks = derive.equipment_unlocks(g)
    end = ctx.tl.forecast_end()
    sched = lint.research_schedule(end)
    uses = []
    for i, r in enumerate(ctx.fd_list("air")):
        if isinstance(r.get("equipment"), str) and r.get("ready") and derive.day(r["ready"]) is not None:
            uses.append((f"force_design.air[{i}]", r["equipment"], derive.day(r["ready"])))
    for i, a in enumerate(lint.list_of("air_plan")):
        if isinstance(a.get("airframe"), str):
            d = _when_day(ctx, a.get("when"))
            if d is not None:
                uses.append((f"air_plan[{i}]", a["airframe"], d))
    for where, eq, day in uses:
        techs = unlocks.get(eq) or set()
        if not techs:
            continue
        got = [ctx.tl.start if t in sched["have"] else sched["finish"].get(t) for t in techs]
        got = [d for d in got if d is not None]
        if not got or min(got) > day:
            lint.add("error", "force.air_research", where,
                     f"{eq} must fly by {derive.fmt(derive.date_of(day))} and the research plan "
                     + (f"finishes {', '.join(sorted(techs))} on {derive.fmt(derive.date_of(min(got)))}" if got
                        else f"never researches {', '.join(sorted(techs))}")
                     + ": research the airframe earlier, or fly the one in hand")
    w = lint.world
    variants = {v.get("type") for v in w.country(lint.tag).get("variants") or []}
    designed = {d.get("base") for d in lint.list_of("designs")}
    keys = [(f"production[{i}]", x.get("equipment")) for i, x in enumerate(lint.list_of("production"))
            if not x.get("remove")]
    keys += [(where, eq) for where, eq, _ in uses]
    seen = set()
    defs = w.equipment_defs()
    for where, eq in keys:
        if not isinstance(eq, str) or eq in seen or not fm.plane_kind(g, eq) or eq not in defs:
            continue
        seen.add(eq)
        try:
            designer = w.eq_kind(eq) == "plane" and bool(defs[eq].get("slots"))
        except Exception:  # noqa: BLE001
            designer = False
        if designer and eq not in variants and eq not in designed:
            lint.add("error", "force.air_design", where,
                     f"{eq} is a designer airframe with no start variant and no designs[] entry on it: give it a "
                     "design (modules for the role, its air experience) before its line or wings")
    for i, d in enumerate(lint.list_of("designs")):
        base = d.get("base")
        if (d.get("kind") or d.get("type")) != "plane" or not isinstance(base, str) or base not in defs:
            continue
        if not defs[base].get("slots") or d.get("modules") or d.get("upgrades") or d.get("modules_why"):
            continue
        lint.add("warning", "force.air_design_blind", f"designs[{i}]",
                 f"{d.get('name') or d.get('id')} on {base} names no module: the designer keeps the base's modules. "
                 "Choose each slot by computed stats for the role and theatre (facts.py design-plan "
                 f"{lint.tag} --roles ... --need fighter:air_range=<facts.py air-cover>), or write modules_why")


def _doctrine_command_defence(ctx, fought):
    lint, g = ctx.lint, ctx.g
    if not any(str(e.get("folder")) == "air" for e in lint.list_of("doctrines")) and \
            not (isinstance(lint.plan.get("doctrine_defaults"), dict) and lint.plan["doctrine_defaults"].get("air")):
        lint.add("error", "force.air_doctrine", "doctrines",
                 "the plan fights for the air with no air doctrine: an air doctrines entry (grand doctrine, tracks, "
                 "experience) or doctrine_defaults.air in preference order")
    slots = derive.politics_slots(g).get("military_staff") or []
    chief = {i for i, k in enumerate(slots) if k == "air_chief"}
    if chief and not any(e.get("category") == "military_staff" and e.get("slot") in chief
                         for e in lint.list_of("politics")):
        lint.add("warning", "force.air_command", "politics",
                 "no air chief in the military staff while the plan fights for the air")
    bombs = 0
    for w, _ in fought:
        n = next((x for x in ctx.needs if x["war"] == w["id"]), {})
        bombs = max(bombs, sum((n.get("front") or {}).get("planes", {}).get(k, 0) for k in BOMBERS))
    if bombs >= AA_ENEMY_BOMBERS:
        built = {b for _, b, _ in lint.queued()}
        if not built & {"radar_station", "anti_air_building"}:
            lint.add("warning", "force.air_defence", "construction",
                     f"the front flies up to {bombs} bombers and CAS and the plan builds no radar station and no "
                     "state anti-air: radar over the regions the fighters defend, anti-air on the factory states")


def _fuel_and_resources(ctx, fought):
    lint, g = ctx.lint, ctx.g
    plan = lint.plan
    auto = plan.get("auto_import")
    auto_ok = auto is True or (isinstance(auto, dict) and auto.get("enabled") is not False) or \
        (isinstance(auto, int) and not isinstance(auto, bool) and auto > 0)
    trade = plan.get("trade")
    deals = trade if isinstance(trade, list) else (trade.get("deals") or [] if isinstance(trade, dict) else [])
    traded = {d.get("resource") for d in deals if isinstance(d, dict)}
    built = {b for _, b, _ in lint.queued()}
    # the trade judgement buys oil for a low fuel ratio (plan_manage.rs trade_judge_rule)
    judged = isinstance(trade, dict) and isinstance(trade.get("judge"), dict) and trade["judge"].get("on") is not False
    fuel_plan = auto_ok or judged or "oil" in traded or bool(built & {"synthetic_refinery", "fuel_silo"})
    # sortie fuel: planes x fuel_consumption x FUEL_COST_MULT, against the start fuel gain (the units of
    # fuel_consumption are not confirmed: reported, not judged)
    mult = derive.define(g, "FUEL_COST_MULT", None)
    gain0, per_oil = derive.define(g, "BASE_FUEL_GAIN", None), derive.define(g, "BASE_FUEL_GAIN_PER_OIL", None)
    res = lint.world.resources(lint.tag)["balance"]
    oil = (res.get("oil") or {}).get("balance", 0.0)
    rows = []
    for w, f in fought:
        use = 0.0
        for role in ("fighters", "cas"):
            r = ctx.need_of(w, role)
            if not r or not r["required"]:
                continue
            kind = "fighter" if role == "fighters" else "cas"
            eq = next((k for k in lint.pool_lines(w["start"])["military"] if fm.plane_kind(g, k) == kind), None)
            fc = fm.fuel_use(g, eq) if eq else None
            if fc and mult:
                use += r["planes"] * fc * mult
        gain = (gain0 or 0) + (per_oil or 0) * max(0.0, oil) if gain0 is not None else None
        rows.append({"war": w["id"], "sortie_fuel_per_hour": round(use, 1),
                     "start_fuel_gain_per_hour": round(gain, 1) if gain is not None else None, "start_oil": oil})
    lint.facts.setdefault("force", {})["air_fuel"] = {
        "rows": rows, "fuel_plan": fuel_plan,
        "note": "fuel_consumption (common/units/equipment) x FUEL_COST_MULT a plane, against BASE_FUEL_GAIN + "
                "BASE_FUEL_GAIN_PER_OIL x the start oil balance; the hour basis is not confirmed"}
    if not fuel_plan:
        lint.add("error", "force.air_fuel", "auto_import",
                 "the wings fly with no fuel plan: no synthetic refinery or fuel silo queued, no oil trade, no "
                 "auto_import: set auto_import with manage.fuel_floor, trade for oil, or queue refineries "
                 "(facts.force.air_fuel)")
    short = []
    for w, _ in fought:
        need = Counter()
        eqres = lint.world.equipment_resources()
        for eq, n in lint.pool_lines(w["start"])["military"].items():
            if fm.plane_kind(g, eq):
                for r, a in ((eqres.get(eq) or {}).get("resources") or {}).items():
                    need[r] += a * n
        for r in ("aluminium", "rubber"):
            have = (res.get(r) or {}).get("available", 0.0)
            if need[r] > have and r not in traded and not auto_ok and not judged:
                short.append(f"{w['id']}: {r} {need[r]:g} for the plane lines against {have:g}")
    if short:
        lint.add("warning", "force.air_resources", "trade",
                 "; ".join(short) + ": trade or auto_import for them, or the lines run short")
