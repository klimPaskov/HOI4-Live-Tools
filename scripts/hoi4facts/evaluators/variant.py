"""Variants: how many production-cost clicks a mass line takes, and the base-and-convert pattern for designer vehicles.

1. Cost clicks (legacy upgrades of non-designer equipment, e.g. a mechanised carrier's cost upgrade):
   an upgrade whose top-level build_cost_ic is negative is a cost click. The legacy rule multiplies: stat x (1 + level
   x per-level factor) (checked against the owner's screenshot: mechanized_equipment_1 8 IC and 0.8 reliability give
   4.00 IC and 60% after five clicks). XP per level = LAND_EQUIPMENT_BASE_COST + levels already taken x
   LAND_EQUIPMENT_RAMP_COST (the define's text), x (1 + an equipment XP cost modifier the country holds). A level is
   worth taking while the unit's value per IC rises; value = 1 + RELIABILITY_ORG_REGAIN x (1 - reliability) (missing
   reliability slows organisation regain, the same factor evaluator 1 uses). Every level is printed with its IC,
   reliability, XP and units per 100 IC so the plan can quote the trade.

2. Base and convert (designer vehicles; guide rows "base" and "BASE PLEASE CONVERT"):
   the final design F is the best design of a chassis family with the techs of a later date (on-time research); the
   base B is what can be built now that converts to F most cheaply: F's modules where they are available now, else in
   a required slot the available module whose own IC plus its conversion into F's module is least, else the slot
   left empty; upgrades at F's levels capped by today's techs. Conversion IC of one vehicle B -> F (common/defines,
   modules' can_convert_from):
       BASE_LAND_EQUIPMENT_CONVERSION_IC_COST_FACTOR x F's chassis IC
     + for each slot whose module changes: F's module convert_cost_ic when F's module can_convert_from B's (module or
       module category), else F's module build_cost_ic
     + the IC of upgrade levels F has above B [?: assumed paid at their per-level IC]
   Design XP of F made from B: EQUIPMENT_MODULE_ADD / REPLACE / CONVERT / REMOVE_XP_COST per slot, plus the upgrades'
   linear_cost cost_by_level per level added. The verdict compares, per vehicle, building B now and converting
   (IC_B + conversion) with building F new when it becomes available (IC_F): the conversion recovers IC_F -
   conversion of each base vehicle; a base line pays when vehicles are needed before F's techs land.
"""
from .. import designer
from ..script import get, get1, parse_file
from . import design
from .context import Situation


def _first(v, default):
    if isinstance(v, list):
        return v[0] if v else default
    return default if v is None else v


def _d(sit, key, default):
    return float(_first(sit.define(key), default))


def _num(v):
    try:
        return float(v)
    except (TypeError, ValueError):
        return None


# ---------------------------------------------------------------- catalogue extras (read once per game)

def extras(game):
    """{"convert": {module: [{"module" | "category", "ic"}]}, "linear": {upgrade: xp per level}, "legacy": {upgrade:
    {"max", "cost", "per_level": {stat: factor}}}} from the modules and upgrades files."""
    def load():
        conv, linear, legacy = {}, {}, {}
        for p in game.files("common/units/equipment/modules"):
            for k, v in parse_file(p):
                if k != "equipment_modules" or not isinstance(v, list):
                    continue
                for name, b in v:
                    if not name or not isinstance(b, list):
                        continue
                    for blk in get(b, "can_convert_from"):
                        if isinstance(blk, list):
                            conv.setdefault(name, []).append({"module": get1(blk, "module"),
                                                              "category": get1(blk, "module_category"),
                                                              "ic": _num(get1(blk, "convert_cost_ic")) or 0.0})
        for p in game.files("common/units/equipment/upgrades"):
            for k, v in parse_file(p):
                if k != "upgrades" or not isinstance(v, list):
                    continue
                for name, b in v:
                    if not name or not isinstance(b, list):
                        continue
                    lc = get1(b, "linear_cost")
                    if isinstance(lc, list) and _num(get1(lc, "cost_by_level")) is not None:
                        linear[name] = _num(get1(lc, "cost_by_level"))
                    top = {kk: _num(vv) for kk, vv in b if _num(vv) is not None and kk not in ("max_level",)}
                    legacy[name] = {"max": int(_num(get1(b, "max_level")) or 0), "cost": get1(b, "cost"),
                                    "per_level": top, "add": isinstance(get1(b, "add_stats"), list)}
        return {"convert": conv, "linear": linear, "legacy": legacy}
    return game.cached("eval:variant-extras", load)


# ---------------------------------------------------------------- 1. cost clicks

def cost_clicks(sit, equipment=None):
    """[{equipment, upgrade, levels: [...], chosen, why}] for every buildable equipment the country has whose legacy
    upgrades include a cost click (or the one named)."""
    v = design.Vehicles.of(sit.game)
    ex = extras(sit.game)
    from .. import divstats
    data = sit.cached("divdata", lambda: divstats.Data(sit.game))
    techs = sit.techs()
    base_xp = {b: _d(sit, f"{b.upper()}_EQUIPMENT_BASE_COST", 0.0) for b in ("land", "naval", "air")}
    ramp_xp = {b: _d(sit, f"{b.upper()}_EQUIPMENT_RAMP_COST", 0.0) for b in ("land", "naval", "air")}
    org = _d(sit, "RELIABILITY_ORG_REGAIN", 0.0)
    if equipment:
        items = [equipment]
    else:
        archs = sorted({data.archetype(e) for e in data.equipment})
        items = [data.newest(a, techs) for a in archs]
    out = []
    for e in items:
        if not e or e not in v.equip:
            continue
        ups = v.inherited(e, "upgrades") or []
        st = v.base_stats(e)
        for u in ups:
            lg = ex["legacy"].get(u) or {}
            pl = lg.get("per_level") or {}
            if lg.get("add") or not pl.get("build_cost_ic", 0) < 0:
                continue
            kind = lg.get("cost") or "land"
            xmod = sit.modifier(f"{kind}_equipment_upgrade_xp_cost")
            rows, xp = [], 0.0
            for lvl in range(0, lg["max"] + 1):
                if lvl:
                    xp += (base_xp.get(kind, 0.0) + (lvl - 1) * ramp_xp.get(kind, 0.0)) * (1 + xmod)
                ic = st.get("build_cost_ic", 0.0) * (1 + lvl * pl["build_cost_ic"])
                rel = st.get("reliability", 1.0) * (1 + lvl * pl.get("reliability", 0.0))
                val = max(0.0, 1 + org * (1 - rel))
                rows.append({"level": lvl, "ic": round(ic, 3), "reliability": round(rel, 3), "xp": round(xp, 1),
                             "value": round(val, 4), "value_per_ic": round(val / max(ic, 1e-6), 4),
                             "units_per_100_ic": round(100.0 / max(ic, 1e-6), 1)})
            best = max(rows, key=lambda r: (r["value_per_ic"], -r["level"]))
            out.append({"equipment": e, "upgrade": u, "xp_kind": kind, "levels": rows, "chosen": best["level"],
                        "why": (f"value per IC peaks at level {best['level']}: {best['ic']:g} IC at reliability "
                                f"{best['reliability']:g} (value {best['value']:g} = 1 + {org:g} x (1 - reliability)) "
                                f"against {rows[0]['ic']:g} IC at {rows[0]['reliability']:g}; {best['xp']:g} {kind} XP"),
                        "rules": [sit.define_src(k) for k in (f"{kind.upper()}_EQUIPMENT_BASE_COST",
                                                              f"{kind.upper()}_EQUIPMENT_RAMP_COST",
                                                              "RELIABILITY_ORG_REGAIN")]})
    return out


# ---------------------------------------------------------------- 2. base and convert

def _converts(ex, new, old, v):
    """convert_cost_ic when module `new` can be converted from `old`, else None."""
    oc = (v.modules.get(old) or {}).get("category")
    for r in ex["convert"].get(new) or []:
        if (r["module"] and r["module"] == old) or (r["category"] and r["category"] == oc):
            return r["ic"]
    return None


def _module_ic(v, m):
    return float(((v.modules.get(m) or {}).get("add") or {}).get("build_cost_ic", 0.0))


def conversion(sit, b_base, b_mods, b_ups, f_base, f_mods, f_ups):
    """(IC, XP, rows) of converting one vehicle B into design F (see the module doc)."""
    v = design.Vehicles.of(sit.game)
    ex = extras(sit.game)
    factor = _d(sit, "BASE_LAND_EQUIPMENT_CONVERSION_IC_COST_FACTOR", 1.0)
    chassis = float(v.base_stats(f_base).get("build_cost_ic", 0.0))
    ic = factor * chassis
    rows = [{"part": f"chassis {f_base}", "ic": round(factor * chassis, 2),
             "rule": sit.define_src("BASE_LAND_EQUIPMENT_CONVERSION_IC_COST_FACTOR")}]
    xp = 0.0
    xpd = {k: _d(sit, f"EQUIPMENT_MODULE_{k}_XP_COST", 0.0) for k in ("ADD", "REPLACE", "CONVERT", "REMOVE")}
    for slot in sorted(set(b_mods) | set(f_mods)):
        old, new = b_mods.get(slot), f_mods.get(slot)
        if old == new:
            continue
        if new is None:
            xp += xpd["REMOVE"]
            rows.append({"part": f"{slot}: remove {old}", "ic": 0.0, "xp": xpd["REMOVE"]})
            continue
        c = _converts(ex, new, old, v) if old else None
        add = c if c is not None else _module_ic(v, new)
        kind = "ADD" if old is None else ("CONVERT" if c is not None else "REPLACE")
        ic += add
        xp += xpd[kind]
        rows.append({"part": f"{slot}: {old or '-'} -> {new}", "ic": round(add, 2), "xp": xpd[kind],
                     "how": "can_convert_from" if c is not None else "new module"})
    for u in sorted(set(b_ups) | set(f_ups)):
        d = int(f_ups.get(u, 0)) - int(b_ups.get(u, 0))
        if d <= 0:
            continue
        ud = v.upgrades.get(u) or {}
        per = float((ud.get("add") or {}).get("build_cost_ic", 0.0)) + float((ud.get("per_level") or {}).get(
            "build_cost_ic", 0.0))
        lin = ex["linear"].get(u, 0.0)
        ic += per * d
        xp += lin * d
        rows.append({"part": f"{u} +{d}", "ic": round(per * d, 2), "xp": lin * d})
    return round(ic, 2), round(xp, 1), rows


def base_for(sit, f, techs_now, role="tank_main"):
    """The base design buildable now that converts into F most cheaply (see the module doc), or None."""
    v = design.Vehicles.of(sit.game)
    ex = extras(sit.game)
    bases, mods = designer.available(v.cat, techs_now)
    fam = v.family(f["base"])
    chassis = f["base"] if f["base"] in bases else None
    if chassis is None:  # the newest chassis of the family available now [?: conversion across chassis versions]
        same = sorted((b for b in bases if v.family(b) == fam), key=lambda b: v.equip.get(b, {}).get("year", 0))
        chassis = same[-1] if same else None
    if chassis is None:
        return None
    role_type = design.LAND_ROLES[role]["type"]
    q = {}
    for s in v.slots(chassis):
        want = f["modules"].get(s["slot"])
        if want and want in mods:
            q[s["slot"]] = want
            continue
        if not s["required"]:
            continue
        cats = list(s["categories"])
        for m in q.values():
            cats = (v.modules.get(m) or {}).get("opens", {}).get(s["slot"], cats)
        opts = [m for m in mods if m in v.modules and v.modules[m]["category"] in cats]

        def total(m):
            c = _converts(ex, want, m, v) if want else None
            return _module_ic(v, m) + (c if c is not None else (_module_ic(v, want) if want else 0.0))
        opts = [m for m in opts if not [p for p in design.legal(v, chassis, dict(q, **{s["slot"]: m}), role_type,
                                                                 sit.world.dlcs) if "required slot" not in p]]
        if opts:
            q[s["slot"]] = min(opts, key=lambda m: (total(m), m))
    caps = v.upgrade_caps(chassis, set(techs_now))
    ups = {u: min(int(n), caps.get(u, 0)) for u, n in (f.get("upgrades") or {}).items()}
    probs = design.legal(v, chassis, q, role_type, sit.world.dlcs)
    st = design.stats(v, chassis, q, ups)
    return {"base": chassis, "modules": q, "upgrades": ups, "stats": design._show(st), "legal": probs,
            "same_chassis": chassis == f["base"]}


def base_and_convert(sit, family, to_date, role="tank_main", enemies=None):
    """Final design F at `to_date` (on-time techs), the base B buildable now, the conversion IC and XP per vehicle and
    the comparison with building F new."""
    later = Situation.load(sit.game, sit.tag, to_date, bookmark=sit.world.date, state=None)
    techs_now = set(sit.techs())
    techs_then = set(later.techs())
    prof = design.enemy_profile(later, enemies)
    rf = design.best_land(later, role, families={family}, top=1, prof=prof, techs=techs_then)
    if not rf["designs"]:
        return {"family": family, "to_date": to_date, "note": f"no {role} design of {family} at {to_date}"}
    f = rf["designs"][0]
    b = base_for(sit, f, techs_now, role)
    out = {"family": family, "role": role, "date": sit.date, "to_date": to_date, "final": f, "base": b,
           "techs_then": later.techs_source()}
    if not b:
        out["note"] = f"no {family} chassis available at {sit.date}"
        return out
    ic, xp, rows = conversion(sit, b["base"], b["modules"], b["upgrades"], f["base"], f["modules"], f["upgrades"])
    ic_b, ic_f = b["stats"].get("build_cost_ic", 0.0), f["stats"].get("build_cost_ic", 0.0)
    out.update({"conversion_ic": ic, "conversion_xp": xp, "conversion_rows": rows, "ic_base": ic_b, "ic_final": ic_f,
                "recovered_per_base": round(ic_f - ic, 2),
                "share_of_new": round(ic / ic_f, 3) if ic_f else None,
                "why": (f"a base {b['base']} costs {ic_b:g} IC now; converting it to the {to_date} design costs "
                        f"{ic:g} IC ({round(100 * ic / ic_f) if ic_f else '-'}% of building that design new at "
                        f"{ic_f:g}), so each base vehicle recovers {round(ic_f - ic, 2):g} IC of its {ic_b:g} when "
                        f"converted; worth it for vehicles needed before {to_date}, not as a saving on its own"
                        + ("" if b["same_chassis"] else f"; the final chassis {f['base']} is not available now "
                           "[?: conversion across chassis versions]"))})
    return out
