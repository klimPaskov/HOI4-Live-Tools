"""What the force can do on each offensive land war day (run by lint_quality.check): division counts and factories
are not the bar; the damage a division deals, the experience it fights with and the air over it are.

  force.firepower      the main line and armour templates on the war day have less soft attack or breakthrough
                       than the game's own AI's templates for our country on that date (baselines `templates`,
                       computed on the same technologies and equipment: the composition floor) or than the enemy's
                       main start template (its own start technologies and equipment); stats by divstats.py from
                       the game files.
  force.regular        the divisions of an operation's attack cannot be Regular on the war day: no army exercise
                       entry runs from EXERCISE_DAYS before the operation's prepare_from (the operation's prepare
                       stops exercises; a war without one: 30 days before the war day).
  force.idle_xp        army experience the plan's own spend list leaves idle on the war day above XP_IDLE (the pool
                       from calc.xp: start, the focus path's rewards, exercise days; spends in xp_budget.army order
                       once their date has come and the pool covers them).
  force.idle_pp        political power above PP_WAR_IDLE on the war day by the plan's own PP budget (facts.pp_budget).
  force.staff_domain   a high-command seat (military_staff 3-5) holds a navy or air officer on the first land war
                       day, or the army chief seat a non-army officer (traits from common/characters).
  force.cas_generation the close air support line on the war day builds an older airframe than the newest the
                       start technologies and the research path give by then.
  force.dominance      the steamroll bar (the AI's marks are a floor, never the target): on each land war day the
                       attacking templates' soft attack and breakthrough, the fighters and close air support staged
                       over the operation, the military factories and the divisions must exceed the game's own AI's
                       for our country on that date (baselines) by force_design.dominance_margin (default
                       DOMINANCE_MARGIN): a warning under the margin, an error at or under the AI. Every baseline date
                       up to the last war: total and military factories under the AI's is an error.
  force.air_range      an air_plan wing staged for an operation's war is based in a state that touches neither the
                       operation's front, a formation's own states nor the enemy (the live bed: inland bases did not
                       reach the regions over the Polish border).
"""
import os

from . import derive
from . import divstats as ds
from . import force as fm

EXERCISE_DAYS = 80     # days of exercise from Trained to Regular [bed read, 1.24% of the level a day]
PREPARE_STOP_DAYS = 30  # a war without an operation: exercises stop this long before the war day [H]
XP_IDLE = 100.0        # army experience lying idle on a war day (the price of a doctrine) [H]
PP_WAR_IDLE = 250.0    # political power lying idle on a war day (two advisors) [H]
HIGH_COMMAND = (3, 4, 5)
DOMINANCE_MARGIN = 0.10  # how far above the AI's marks the plan must stand on a war day [owner: steamroll the AI]
RULES = ("force.firepower", "force.regular", "force.idle_xp", "force.idle_pp", "force.staff_domain",
         "force.cas_generation", "force.air_range")


def _fmt(d):
    return derive.fmt(derive.date_of(d)) if d is not None else "undated"


def data_of(lint):
    d = getattr(lint, "_divdata", None)
    if d is None:
        d = lint._divdata = ds.Data(lint.game)
    return d


def techs_on(lint, tag, day):
    """Start technologies of the tag; for our own tag also the research path's blocks of the years before the war
    year (a block's year is when the plan starts it; the year before is a safe done date)."""
    w = lint.world
    out = set(w.country(tag)["techs"]) if w is not None else set()
    if tag == lint.tag:
        y = derive.date_of(day)[0]
        for b in lint.plan.get("research_path") or []:
            if isinstance(b, dict) and isinstance(b.get("year"), int) and b["year"] < y:
                out.update(t for t in b.get("techs") or [] if isinstance(t, str))
            elif isinstance(b, str):
                out.add(b)
    return out


def doctrines_of(lint, tag):
    w = lint.world
    if w is None:
        return []
    return [d.get("v") for d in w.country(tag)["doctrines"] if isinstance(d.get("v"), str)]


def equipment_on(lint, data, tag, techs, day):
    """{archetype: variant key or design stats}: our lines in force on the day (the newest of an archetype), else
    the newest variant the technologies enable; designs from the plan's designs[] modules, else the tag's newest
    start variant of that chassis."""
    out = {}
    lines = lint.active_lines(day) if tag == lint.tag else {}
    for eq in lines:
        arch = data.archetype(eq)
        cur = out.get(arch)
        if cur is None or (isinstance(cur, str) and data.equipment.get(eq, {}).get("year", 0)
                           > data.equipment.get(cur, {}).get("year", 0)):
            out[arch] = eq
    designs = {d.get("id"): d for d in lint.list_of("designs") if isinstance(d, dict)}
    for x in lint.list_of("production"):
        d = designs.get(x.get("design"))
        if d and isinstance(d.get("modules"), dict) and x.get("equipment") in lines:
            out[data.archetype(x["equipment"])] = data.design_stats(x["equipment"], d["modules"], d.get("upgrades"))
    w = lint.world
    variants = [v for v in (w.country(tag)["variants"] if w is not None else []) if not v.get("obsolete")]
    for arch, key in list(out.items()):
        # a chassis or airframe line without a plan design builds the country's start variant of that type
        if isinstance(key, str):
            same = [v for v in variants if v.get("type") == key and v.get("modules")]
            if same:
                v = same[-1]
                out[arch] = data.design_stats(v["type"], v.get("modules"), v.get("upgrades"))
    for arch in {data.archetype(k) for k, e in data.equipment.items() if not e.get("is_archetype")}:
        if arch in out:
            continue
        own = [v for v in variants if v.get("type") and data.archetype(v["type"]) == arch]
        if own:
            v = max(own, key=lambda v: data.equipment.get(v["type"], {}).get("year", 0))
            out[arch] = data.design_stats(v["type"], v.get("modules"), v.get("upgrades"))
            continue
        k = data.newest(arch, techs)
        if k:
            out[arch] = k
    return out


def stats(data, units, equipment, mods):
    bats = [u for u in units if u in data.units and not data.units[u]["support"]]
    sup = [u for u in units if u in data.units and data.units[u]["support"]]
    return ds.division(data, bats, sup, equipment, mods)


def enemy_main(lint, tag):
    """(name, units) of the template most of the enemy's start divisions use."""
    w = lint.world
    if w is None:
        return None, []
    oob = w.oob(tag)
    count = {}
    for d in oob.get("divisions") or []:
        count[d.get("template")] = count.get(d.get("template"), 0) + 1
    if not count:
        return None, []
    name = max(count, key=count.get)
    t = oob["templates"].get(name) or {}
    units = []
    for k, n in (t.get("regiments") or {}).items():
        units += [k] * int(n)
    return name, units + list(t.get("support") or [])


def check(ctx, wars, ops, ai_templates):
    lint, tl = ctx.lint, ctx.tl
    if lint.world is None or not lint.tag:
        return
    try:
        data = data_of(lint)
    except Exception as exc:  # noqa: BLE001 - advisory: no files, no stats
        lint.add("info", "force.firepower", "templates", f"no division stats: {type(exc).__name__}: {exc}")
        return
    facts = []
    land = [w for w in wars if ops.land_targets(w)]
    for w in land:
        day = w["start"]
        where = f"force_design ({w['id']})"
        army = ctx.army(day)
        kinds = {t: ctx.profile(t)["kind"] for t in army}
        techs = techs_on(lint, lint.tag, day)
        docs = doctrines_of(lint, lint.tag)
        mods = ds.factors(data, techs, docs)
        eq = equipment_on(lint, data, lint.tag, techs, day)
        ai = ai_templates(lint.tag, day) or {}
        row = {"war": w["id"], "date": _fmt(day)}
        foes = [t for t in w.get("enemy_tags") or w.get("targets") or [] if isinstance(t, str)]
        foe = foes[0] if foes else None
        enemy = None
        if foe:
            ename, eunits = enemy_main(lint, foe)
            if eunits:
                et = techs_on(lint, foe, day)
                enemy = dict(stats(data, eunits, equipment_on(lint, data, foe, et, day),
                                   ds.factors(data, et, doctrines_of(lint, foe))), template=ename, tag=foe)
        problems = []
        for kind in ("line", "armour"):
            names = [t for t in army if kinds[t] == kind]
            if not names:
                continue
            main = max(names, key=lambda t: army[t])
            ours = stats(data, tl.template_units(main), eq, mods)
            if not ours:
                continue
            r = dict(ours, template=main, divisions=round(army[main]))
            floor = stats(data, ai[kind], eq, mods) if ai.get(kind) else None
            if floor:
                r["ai"] = floor
            if kind == "line" and enemy:
                r["enemy"] = enemy
            row[kind] = r
            for other, label in ((floor, f"the game's own AI's {kind} template for {lint.tag} on {ai.get('date')} "
                                         "(same technologies and equipment)"),
                                 (enemy if kind == "line" else None,
                                  f"{foe}'s main start template {enemy['template'] if enemy else ''}")):
                if not other:
                    continue
                short = [f"{s.replace('_', ' ')} {ours[s]:g} against {other[s]:g}" for s in ("soft_attack", "breakthrough")
                         if ours[s] <= other[s]]
                if short:
                    problems.append(f"{kind} template {main}: " + ", ".join(short) + f" ({label})")
        if problems:
            lint.add("error", "force.firepower", where,
                     f"{w['id']} ({_fmt(day)}): the attack must out-gun the floor, not equal it: " + "; ".join(problems)
                     + ". Add line artillery (a second battalion), the newer gun or rifle on its line, medium tanks "
                       "with the bigger gun; calc: python3 scripts/hoi4facts/divstats.py")
        facts.append(row)
        _regular(ctx, w, ops, where)
        _air(ctx, w, ops, data, techs, day, where)
    if facts:
        lint.facts.setdefault("force", {})["firepower"] = facts
    dominance(ctx, land, facts)
    if land:
        first = min(w["start"] for w in land)
        _staff(ctx, first)
        for w in land:
            _idle(ctx, w)


def baseline_rows(tag):
    """The AI baseline rows for the tag (skills/hoi4-campaign-playbook/baselines), date-sorted."""
    import glob
    import json
    here = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
    rows = []
    for path in sorted(glob.glob(os.path.join(here, "skills", "hoi4-campaign-playbook", "baselines", "*.json"))):
        try:
            with open(path, encoding="utf-8") as fh:
                b = json.load(fh)
        except (OSError, ValueError):
            continue
        if b.get("tag") == tag:
            rows += [r for r in b.get("rows") or [] if isinstance(r.get("date"), str)]
    return sorted(rows, key=lambda r: derive.day(r["date"]))


def ai_on(rows, day, key):
    """The AI's value of `key` on day (linear between baseline rows; None outside them)."""
    pts = [(derive.day(r["date"]), r[key]) for r in rows if isinstance(r.get(key), (int, float))]
    if not pts or day < pts[0][0] or day > pts[-1][0]:
        return None
    for (d0, v0), (d1, v1) in zip(pts, pts[1:] + [pts[-1]]):
        if d0 <= day <= d1:
            return v0 if d1 == d0 else v0 + (v1 - v0) * (day - d0) / (d1 - d0)
    return pts[-1][1]


def dominance(ctx, land, facts):
    lint, tl = ctx.lint, ctx.tl
    fd = lint.plan.get("force_design") if isinstance(lint.plan.get("force_design"), dict) else {}
    margin = fd.get("dominance_margin") if isinstance(fd.get("dominance_margin"), (int, float)) else DOMINANCE_MARGIN
    rows = baseline_rows(lint.tag)
    if not rows or not land:
        return
    table = []
    by_war = {f["war"]: f for f in facts}

    def judge(where, label, ours, ai, rows_out, unsure=None):
        """unsure: why the AI's figure is not a firm read (an upper bound or an estimate): the rule then warns."""
        if ours is None or ai is None or ai <= 0:
            return
        ratio = ours / ai
        row = {"what": label, "ours": round(ours, 1), "ai": round(ai, 1), "ratio": round(ratio, 3)}
        if unsure:
            row["ai_read"] = unsure
        rows_out.append(row)
        if ratio <= 1.0:
            lint.add("warning" if unsure else "error", "force.dominance", where,
                     f"{label}: {ours:.0f} against the game's own AI's {ai:.0f} ({ratio:.0%}"
                     f"{', ' + unsure if unsure else ''}): level or under the AI is a loss; the bar is at least "
                     f"{1 + margin:.0%} of the AI")
        elif ratio < 1 + margin:
            lint.add("warning", "force.dominance", where,
                     f"{label}: {ours:.0f} against the AI's {ai:.0f} ({ratio:.0%}), under the {1 + margin:.0%} margin")

    for w in land:
        day = w["start"]
        where = f"force_design ({w['id']})"
        out = []
        tag = f"{w['id']} ({_fmt(day)})"
        f = by_war.get(w["id"]) or {}
        for kind in ("line", "armour"):
            r = f.get(kind) or {}
            if r.get("ai"):
                for st in ("soft_attack", "breakthrough"):
                    judge(where, f"{tag} {kind} template {r['template']} {st.replace('_', ' ')}", r.get(st),
                          r["ai"].get(st), out)
        judge(where, f"{tag} military factories", lint.military_on(day), ai_on(rows, day, "military_factories"), out)
        army = ctx.army(day)
        judge(where, f"{tag} divisions", sum(army.values()) if army else None, ai_on(rows, day, "divisions"), out)
        op = _op_of(lint, w["id"])
        regions = set((op.get("air") or {}).get("regions") or []) if op else set()
        for mission, key in (("air_superiority", "fighters"), ("cas", "cas")):
            ai = ai_on(rows, day, key)
            if ai is None or not regions:
                continue
            unsure = None
            src = next((r for r in rows if r.get(key) is not None and derive.day(r["date"]) == day), None)
            if src is None:
                src = min((r for r in rows if r.get(key) is not None),
                          key=lambda r: abs((derive.day(r["date"]) or 0) - day), default={})
            if src.get("air_read") in ("contaminated", "upper_bound"):
                est = src.get(f"{key}_estimate")
                if isinstance(est, (int, float)) and src.get(key):
                    ai = ai * est / src[key]
                    unsure = f"estimate from an upper-bound read of {src[key]}"
                else:
                    unsure = "upper-bound read"
            staged = sum(r.get("count") or 0 for r in lint.list_of("air_plan")
                         if r.get("mission") == mission and r.get("region") in regions
                         and (_date_gate(r.get("when")) or tl.start) <= day)
            judge(where, f"{tag} {key} staged over the operation", staged, ai, out, unsure)
        table.append({"war": w["id"], "date": _fmt(day), "rows": out})
    last = max(w["start"] for w in land)
    for r in rows:
        d = derive.day(r["date"])
        if d is None or d > last or d < tl.start:
            continue
        mil = lint.military_on(d)
        fc = getattr(lint, "_econ_fc", None) or {}
        civ = next((x["civ"] for x in fc.get("rows") or [] if x.get("date") == derive.fmt(derive.date_of(d))), None)
        if mil is not None and mil < r["military_factories"]:
            lint.add("error", "force.dominance", "construction",
                     f"{r['date']}: {mil:.0f} military factories against the AI's {r['military_factories']}")
        if mil is not None and civ is not None and civ + mil < r["military_factories"] + r["civilian_factories"]:
            lint.add("error", "force.dominance", "construction",
                     f"{r['date']}: {civ + mil:.0f} factories against the AI's "
                     f"{r['military_factories'] + r['civilian_factories']}")
    lint.facts.setdefault("force", {})["dominance"] = {"margin": margin, "wars": table}


def _ops_of(lint, war_id):
    """The operations that fight a war (several when the war is fought by several army groups)."""
    return [o for o in lint.list_of("operations") if o.get("war") == war_id]


def _op_of(lint, war_id):
    """The war's operations as one: the earliest prepare_from, the fronts, enemies and air regions of all."""
    ops = _ops_of(lint, war_id)
    if not ops:
        return None
    if len(ops) == 1:
        return ops[0]
    regions, enemies, parts = [], [], []
    for o in ops:
        regions += [r for r in (o.get("air") or {}).get("regions") or [] if r not in regions]
        enemies += [t for t in o.get("enemies") or [] if t not in enemies]
        parts += [{"front": o.get("front")}] + list(o.get("formations") or [])
    first = ops[0]
    return {"id": "+".join(str(o.get("id")) for o in ops), "war": war_id, "enemies": enemies,
            "prepare_from": first.get("prepare_from"), "air": {"regions": regions}, "formations": parts,
            "_all": ops}


def _regular(ctx, w, ops, where):
    lint, tl = ctx.lint, ctx.tl
    stop = None
    for op in _ops_of(lint, w["id"]):
        if op.get("prepare_from") is not None:
            d = fm.predicate_day(lint, tl, op.get("prepare_from"), ctx.wars)
            if d is not None and (stop is None or d < stop):
                stop = d
    if stop is None:
        stop = w["start"] - PREPARE_STOP_DAYS
    xb = lint.plan.get("xp_budget") if isinstance(lint.plan.get("xp_budget"), dict) else {}
    ex = [e for e in xb.get("army") or [] if isinstance(e, dict) and e.get("purpose") == "exercise"]
    starts = [fm.predicate_day(lint, tl, e.get("when"), ctx.wars) if e.get("when") is not None else tl.start
              for e in ex]
    starts = [d for d in starts if d is not None]
    if not starts or min(starts) > stop - EXERCISE_DAYS:
        lint.add("error", "force.regular", where,
                 f"{w['id']} ({_fmt(w['start'])}): no army exercise runs {EXERCISE_DAYS} days before "
                 f"{'the operation prepares' if _ops_of(lint, w['id']) else 'the war'} ({_fmt(stop)}): the attackers fight Trained "
                 "(+0%) where Regular is +25%")


def _air(ctx, w, ops, data, techs, day, where):
    lint = ctx.lint
    # the close air support generation
    lines = lint.active_lines(day)
    cas = [eq for eq in lines if data.archetype(eq) == "small_plane_cas_airframe" or
           (data.equipment.get(eq, {}).get("archetype") or "").startswith("small_plane_cas")]
    if cas:
        have = max(cas, key=lambda k: data.equipment.get(k, {}).get("year", 0))
        best = data.newest(data.archetype(have), techs)
        if best and data.equipment.get(best, {}).get("year", 0) > data.equipment.get(have, {}).get("year", 0):
            lint.add("error", "force.cas_generation", where,
                     f"{w['id']} ({_fmt(day)}): the close air support line builds {have} while {best} is researched "
                     "by then: switch the line (production `replace`) so the wings over the attack are of the "
                     "newer airframe")
    # bases in range of the operation
    op = _op_of(lint, w["id"])
    if op is None or lint.world is None:
        return
    near = set()
    for part in [op] + list(op.get("formations") or []):
        fr = part.get("front") if isinstance(part, dict) else None
        if isinstance(fr, dict):
            near.update(s for s in fr.get("own_states") or [] if isinstance(s, int))
    enemy = set(op.get("enemies") or [])
    owner = {}
    try:
        st = lint.world.states()
        owner = {sid: s.get("owner") for sid, s in st.items()}
        borders = lint.world.state_borders()
    except Exception:  # noqa: BLE001
        borders = {}
    def touches(sid):
        if sid in near:
            return True
        for n in borders.get(sid) or ():
            if n in near or owner.get(n) in enemy:
                return True
        return False
    if not borders:
        return
    far = []
    for r in lint.list_of("air_plan"):
        if r.get("mission") not in ("air_superiority", "cas"):
            continue
        reg = r.get("region")
        if reg not in (op.get("air") or {}).get("regions", []):
            continue
        b = r.get("base_state")
        if isinstance(b, int) and not touches(b):
            far.append(f"{r.get('id')} (base {b})")
    if far:
        lint.add("error", "force.air_range", f"air_plan ({w['id']})",
                 f"{w['id']}: {', '.join(far)} fly over the operation's regions from a state that touches neither its "
                 "front nor the enemy: base them on the border fields (the live bed: inland bases could not reach "
                 "the regions over the border)")


def _staff(ctx, first):
    lint, tl = ctx.lint, ctx.tl
    try:
        chars = advisor_traits(lint)
    except Exception:  # noqa: BLE001
        return
    seats = {}
    auto = lint.plan.get("politics_auto") if isinstance(lint.plan.get("politics_auto"), dict) else {}
    for e in lint.list_of("politics") + [x for x in auto.get("order") or [] if isinstance(x, dict)]:
        if e.get("category") != "military_staff":
            continue
        ideas = [e["idea"]] if isinstance(e.get("idea"), str) else [x for x in e.get("ideas") or [] if isinstance(x, str)]
        d = fm.predicate_day(lint, tl, e.get("when"), ctx.wars) if e.get("when") is not None else tl.start
        if d is None or d > first:
            continue
        for i in ideas:
            seats.setdefault(e.get("slot"), []).append(i)
    bad = []
    for slot, ideas in seats.items():
        for i in ideas:
            tr = chars.get(i)
            if not tr:
                continue
            branch = "navy" if any(t.startswith("navy_") for t in tr) else "air" if any(t.startswith("air_") for t in tr) \
                else "army" if any(t.startswith("army_") for t in tr) else None
            if (slot in HIGH_COMMAND or slot == 0) and branch in ("navy", "air"):
                bad.append(f"military_staff:{slot} {i} ({branch}: {', '.join(sorted(tr))})")
    if bad:
        lint.add("error", "force.staff_domain", "politics, politics_auto",
                 f"by the first land war ({_fmt(first)}): " + "; ".join(bad) + " sit(s) in an army seat: name an army "
                 "officer (army_* traits) for the army chief and the three high-command seats")


def advisor_traits(lint):
    """{idea_token: set(traits)} of the advisors in common/characters."""
    got = getattr(lint, "_adv_traits", None)
    if got is not None:
        return got
    out = {}
    for f in lint.game.files("common/characters"):
        for _, chars in ds.parse_file(f):
            if not isinstance(chars, list):
                continue
            for _, body in chars:
                if not isinstance(body, list):
                    continue
                for k, v in body:
                    if k == "advisor" and isinstance(v, list):
                        tok = ds.get1(v, "idea_token")
                        tr = ds.get1(v, "traits")
                        if tok:
                            out[tok] = set(ds.bare(tr)) if isinstance(tr, list) else set()
    lint._adv_traits = out
    return out


def _idle(ctx, w):
    lint, tl = ctx.lint, ctx.tl
    day = w["start"]
    pp = getattr(lint, "_pp_series", None)
    if pp and day in pp and pp[day] > PP_WAR_IDLE:
        lint.add("error", "force.idle_pp", "politics_auto",
                 f"{w['id']} ({_fmt(day)}): about {pp[day]:.0f} political power lies idle on the war day by the plan's "
                 f"own budget (more than {PP_WAR_IDLE:g}): name advisors, laws or decisions that take it before the war")
    pool = xp_pool(lint, tl, ctx.wars, day)
    if pool is not None and pool["idle"] > XP_IDLE:
        lint.add("error", "force.idle_xp", "xp_budget.army",
                 f"{w['id']} ({_fmt(day)}): about {pool['idle']:.0f} army experience lies idle on the war day "
                 f"(pool {pool['pool']:.0f}, next spend {pool['next']}): buy officer-corps spirits, template edits or "
                 "a doctrine before the war")


def xp_pool(lint, tl, wars, until):
    """{pool, idle, next, bought}: the army pool on `until` with the plan's xp_budget.army spends bought in order."""
    w = lint.world
    try:
        info = w.experience(lint.tag)
        gain = info["gain"].get("army") or {}
        train = float(gain["training_max_per_day"]["v"]) if "training_max_per_day" in gain else 0.0
        cap = float(info["max"]["army"]["v"]) if "army" in info["max"] else 1e9
        pool = float(info["start"]["army"]["v"])
    except Exception:  # noqa: BLE001
        return None
    focusmod = __import__((__package__ or "hoi4facts") + ".focus", fromlist=["xp_rewards"])
    ends = {}
    for r in (lint.facts.get("focus") or {}).get("timeline") or []:
        if isinstance(r, dict) and r.get("focus") and r.get("end"):
            ends[r["focus"]] = derive.day(r["end"])
    rewards = sorted((ends[r["focus"]], float(r["army"])) for r in focusmod.xp_rewards(w, lint.tag)
                     if r.get("focus") in ends and (r.get("army") or 0) > 0 and ends[r["focus"]] is not None)
    xb = lint.plan.get("xp_budget") if isinstance(lint.plan.get("xp_budget"), dict) else {}
    entries = [e for e in xb.get("army") or [] if isinstance(e, dict)]
    ex = [fm.predicate_day(lint, tl, e.get("when"), wars) if e.get("when") is not None else tl.start
          for e in entries if e.get("purpose") == "exercise"]
    ex = min([d for d in ex if d is not None], default=None)
    spends = []
    for e in entries:
        if e.get("purpose") == "exercise" or not isinstance(e.get("cost"), (int, float)) or e["cost"] <= 0:
            continue
        d = _date_gate(e.get("when"))
        spends.append([e.get("id"), float(e["cost"]), d if d is not None else tl.start])
    day, bought = tl.start, []
    while day <= until:
        while rewards and rewards[0][0] <= day:
            pool = min(cap, pool + rewards.pop(0)[1])
        if ex is not None and ex <= day:
            pool = min(cap, pool + train)
        for s in list(spends):
            if s[2] <= day and pool >= s[1]:
                pool -= s[1]
                bought.append(s[0])
                spends.remove(s)
            elif s[2] <= day:
                break
        day += 1
    nxt = next((s for s in spends if s[2] <= until), None)
    idle = pool - (nxt[1] if nxt else 0.0)
    return {"pool": pool, "idle": max(0.0, idle), "next": f"{nxt[0]} ({nxt[1]:g})" if nxt else "none dated",
            "bought": bought}


def _date_gate(pred):
    """The day a `date >= Y.M.D` part of a predicate opens (other parts, e.g. xp gates, are the pool itself)."""
    if isinstance(pred, str):
        p = pred.replace(" ", "")
        if p.startswith("date>="):
            return derive.day(p[6:])
        return None
    if isinstance(pred, dict):
        days = [_date_gate(x) for x in pred.get("all") or []]
        days = [d for d in days if d is not None]
        return max(days) if days else None
    return None


__all__ = ["check", "RULES", "xp_pool", "dominance"]

