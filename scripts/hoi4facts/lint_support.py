"""Supply, trains, radar, special projects and formations checks, run by lint_plan.py (Lint.run).

  construction.requires_tech  (warning) a construction_plan item for a building no start technology unlocks
                     (enable_building level 1: radar_station needs radio_detection, which the sp_air_radar
                     project grants) carries neither `requires` with that technology nor a `when` that waits for
                     it: a dated item is refused until the technology lands and is given up after three tries
                     (g17n, g17o, s53a: radar_station six refusals a run, never built).
  projects.facility  (warning) a special_projects.priority project of a specialisation the country has no
                     facility of at the start and no construction_plan facility item for: it can never run.
  air.logistics_strike (info) a plan at war with wings that can fly logistics strikes (CAS, tactical bombers,
                     heavy fighters) and no air_plan entry on logistics_strike: trains and trucks are the
                     enemy's supply (500 CAS killed 48 trains and 200 trucks in a month, supply 26% by month 2:
                     airnaval T4).
  supply.trains      (warning) no train line from the start (logistics.trains or a train_equipment production
                     row): the start stock equals the start need (DEFAULT_STARTING_TRAIN_RATIO 1), the need
                     grows with every railway province between the capital and a hub (NUM_RAILWAYS_TRAIN_FACTOR)
                     and logistics strikes kill trains; with too few, supply falls toward MIN_TRAIN_SUPPLY_FACTOR.
                     (info) a train line without stockpile_targets.train_equipment to grow it.
  supply.hubs        (warning) an operation's supply.motorization without logistics.hubs (nothing else reads
                     it); (info) logistics.hubs without a truck line.
  formations.keep    (warning) a formations.keep name that is neither a start template nor a plan template.
"""

CAS_KINDS = ("cas", "tac", "tactical", "heavy_fighter", "medium_plane", "small_plane_cas")


def _items(plan, key):
    v = plan.get(key)
    return [x for x in v if isinstance(x, dict)] if isinstance(v, list) else []


def _mentions(value, word):
    if isinstance(value, str):
        return word in value
    if isinstance(value, list):
        return any(_mentions(v, word) for v in value)
    if isinstance(value, dict):
        return any(_mentions(v, word) for v in value.values())
    return False


def requires_findings(plan, gates, start):
    """construction.requires_tech: [(severity, where, message)]; gates {building: [{"tech", "level"}]},
    start the start technologies."""
    out = []
    for i, item in enumerate(_items(plan, "construction_plan")):
        b = item.get("building")
        rows = gates.get(b) or []
        if not any(r["level"] <= 1 for r in rows) or any(r["tech"] in start for r in rows):
            continue
        first = sorted({r["tech"] for r in rows if r["level"] <= 1})
        req = item.get("requires")
        req = [req] if isinstance(req, str) else [r for r in req or [] if isinstance(r, str)]
        if any(t in req for t in first) or any(_mentions(item.get("when"), f"tech_researched:{t}") for t in first):
            continue
        out.append(("warning", f"construction_plan[{i}] {item.get('id') or b}",
                    f"{b} cannot be selected before {' or '.join(first)} (enable_building level 1, not a start "
                    f"technology): add \"requires\": {first[0]!r} so the item waits without spending its three "
                    "tries (a dated item is refused and given up)"))
    return out


def facility_findings(plan, projects, facilities_at_start):
    """projects.facility: priority projects whose specialisation has no facility at start or in the plan."""
    sp = plan.get("special_projects")
    if not isinstance(sp, dict):
        return []
    spec_of = {p["id"]: (p.get("specialization") or "").replace("specialization_", "") for p in projects}
    have = {(f.get("building") or "").replace("_facility", "") for f in facilities_at_start}
    have |= {(c.get("building") or "").replace("_facility", "") for c in _items(plan, "construction_plan")
             if str(c.get("building") or "").endswith("_facility")}
    out = []
    for i, key in enumerate(p for p in sp.get("priority") or [] if isinstance(p, str)):
        spec = spec_of.get(key)
        if spec and spec not in have:
            out.append(("warning", f"special_projects.priority[{i}]",
                        f"{key} is a {spec} project and the country has no {spec}_facility at the start and none "
                        f"in construction_plan: it never runs (a facility costs 10000 + 6000 per facility owned; "
                        "build one only when the project pays that back)"))
    return out


def at_war(plan):
    war = plan.get("war")
    return isinstance(war, dict) and bool(war.get("enemies") or war.get("declare") or war.get("theatre"))


def logistics_findings(plan):
    if not at_war(plan):
        return []
    entries = _items(plan, "air_plan")
    if any(e.get("mission") == "logistics_strike" for e in entries):
        return []
    strikers = [e for e in entries if any(k in str(e.get("airframe") or e.get("mission") or "") for k in CAS_KINDS)]
    if not strikers:
        return []
    return [("info", "air_plan", f"{len(strikers)} air_plan entries fly CAS-capable airframes and none flies "
             "logistics_strike: when the front has no battle to support (a pause, a held sector, before an "
             "offensive) the same wings kill trains and trucks (airnaval T4: 500 CAS, 48 trains and 200 trucks in "
             "a month, enemy supply 26% by month 2); add an entry with mission logistics_strike over the enemy's "
             "supply region and its `when`")]


def train_findings(plan):
    """Trains from the start: the start stock equals the start need, so a line must exist before the fronts grow."""
    out = []
    logi = plan.get("logistics") if isinstance(plan.get("logistics"), dict) else {}
    targets = plan.get("stockpile_targets") if isinstance(plan.get("stockpile_targets"), dict) else {}
    line = isinstance(logi.get("trains"), dict) or any(
        str(p.get("equipment") or "").startswith("train_equipment") for p in _items(plan, "production"))
    target = any(str(k).startswith("train_equipment") for k in targets)
    if not line:
        out.append(("warning", "logistics.trains", "no train line from the start: a country starts with exactly the "
                    "trains its network needs (DEFAULT_STARTING_TRAIN_RATIO 1); each railway province between the "
                    "capital and a hub adds need (NUM_RAILWAYS_TRAIN_FACTOR 0.03), logistics strikes kill trains, "
                    "and short trains cut supply up to half (MIN_TRAIN_SUPPLY_FACTOR 0.5). Set logistics.trains "
                    "{factories: 1} or a production row for train_equipment"))
    if not target and (line or at_war(plan)):
        out.append(("info", "stockpile_targets", "no train_equipment in stockpile_targets: name it with its days "
                    "of cover so the stockpile rule raises the train line when the stock falls"))
    return out


def hub_findings(plan):
    """Operation motorization is acted on only by logistics.hubs; trucks for hubs need a truck line."""
    out = []
    logi = plan.get("logistics") if isinstance(plan.get("logistics"), dict) else {}
    for i, op in enumerate(_items(plan, "operations")):
        level = (op.get("supply") or {}).get("motorization") if isinstance(op.get("supply"), dict) else None
        if level is not None and logi.get("hubs") is not True:
            out.append(("warning", f"operations[{i}].supply.motorization",
                        f"{op.get('id')}: motorization {level} is read only by logistics.hubs (set it true): "
                        "without it the operation's hubs keep the standing war.motorization"))
    if logi.get("hubs") is True and not any(
            str(p.get("equipment") or "").startswith("motorized_equipment") for p in _items(plan, "production")):
        out.append(("info", "logistics.hubs", "hub motorization spends trucks from the stockpile "
                    "(SUPPLY_HUB_FULL_MOTORIZATION_TRUCK_COST 60 a hub): plan a truck line"))
    return out


def keep_findings(plan, start_templates):
    f = plan.get("formations")
    if not isinstance(f, dict):
        return []
    known = set(start_templates or ()) | {t.get("name") for t in _items(plan, "templates")}
    known |= {t.get("template") for t in _items(plan, "training")}
    out = []
    for i, name in enumerate(f.get("keep") or []):
        if start_templates is not None and name not in known:
            out.append(("warning", f"formations.keep[{i}]", f"{name!r} is no start template and no plan template"))
    return out


def check(lint):
    """Adds the findings above to `lint` (a lint_plan.Lint)."""
    plan = lint.plan
    w = lint.world
    if w is None:
        return
    try:
        from . import research as researchmod
    except ImportError:  # run as a script from scripts/hoi4facts
        from hoi4facts import research as researchmod
    start = set(researchmod.start_techs(w, lint.tag)) if getattr(lint, "tag", None) else set()
    for sev, where, msg in requires_findings(plan, w.building_unlocks(), start):
        lint.add(sev, "construction.requires_tech", where, msg)
    if isinstance(plan.get("special_projects"), dict) and getattr(lint, "tag", None):
        pr = w.projects(lint.tag)
        for sev, where, msg in facility_findings(plan, pr.get("projects", []), pr.get("facilities_at_start", [])):
            lint.add(sev, "projects.facility", where, msg)
    for sev, where, msg in logistics_findings(plan):
        lint.add(sev, "air.logistics_strike", where, msg)
    for sev, where, msg in train_findings(plan):
        lint.add(sev, "supply.trains", where, msg)
    for sev, where, msg in hub_findings(plan):
        lint.add(sev, "supply.hubs", where, msg)
    if isinstance(plan.get("formations"), dict):
        start_t = None
        try:
            start_t = set()
            for o in lint_start_oobs(lint):
                start_t |= set(lint.game.oob(o, lint.ctx)["templates"])
        except Exception:  # noqa: BLE001 - the OOB read is best effort; without it the check is skipped
            start_t = None
        for sev, where, msg in keep_findings(plan, start_t):
            lint.add(sev, "formations.keep", where, msg)


def lint_start_oobs(lint):
    try:
        from .lint_plan import start_oobs
    except ImportError:
        from hoi4facts.lint_plan import start_oobs
    return start_oobs(lint.game, lint.meta)
