"""Air side of force sizing: fighters and close air support a front needs.

Plane stats and the air plan belong to the air stream (designer.py, plan_air.rs's `air_situation`); this module only
sizes the counts for force planning, with the same inputs the executor uses.

Fighters per front = enemy fighters x (their air_superiority per plane / ours) x fighter_ratio.
  enemy fighters        the live read when the situation has one (state `enemy_fighters` {tag: planes}, what we
                        see over the front's regions); otherwise the enemy's start OOB fighters [prior: the start
                        wings, not what the enemy builds by the war date]
  per-plane quality     the region's superiority is the sum of the planes' air_superiority (the airframe's stat:
                        1 a fighter in vanilla, so numbers decide it); theirs from their best start fighter variant,
                        ours from the best fighter the designer builds with our techs (designer.best, role
                        fighter); quality acts through losses: the duel exchange (kills per loss) is reported
  fighter_ratio         the plan's `air_situation.fighter_ratio` (the executor flies wings until ours are that many
                        times theirs); when the plan gives none: parity (1.0), said in the answer
CAS per front (the most that can still join the battles) = LAND_AIR_COMBAT_MAX_PLANES_PER_ENEMY_WIDTH x the enemy width in one battle x attack axes: the planes
  that can still join (more do not enter the combat). Enemy width in a battle = the front's terrain battle width
  (common/terrain combat_width, one attack direction, weighted by the front's terrain mix); no map: plains.
  CAS flies where the fighters win the air: none required while fighters fall short (the answer says so).
"""
from .. import designer, force as fm, theatre


def _first(v, default):
    if isinstance(v, list):
        return v[0] if v else default
    return default if v is None else v


def enemy_fighters(sit, tags):
    """({tag: fighters}, source)."""
    live = sit.state.get("enemy_fighters")
    if isinstance(live, dict) and any(t in live for t in tags):
        return {t: int(live.get(t) or 0) for t in tags}, "live read (state enemy_fighters)"
    out = {}
    for t in tags:
        out[t] = sum(int(a.get("amount") or 0) for a in sit.world.oob(t).get("air") or []
                     if fm.plane_kind(sit.game, a.get("type")) in ("fighter", "heavy_fighter"))
    return out, "start OOB wings [prior: not what they build by the war date]"


def _their_fighter(sit, cat, tags):
    """(stats, name) of the enemy's best start fighter: its start variants (modules) on the airframes its fighter
    wings fly, the best air superiority of them [upper bound: every wing flown as the best variant]."""
    frames = set()
    for t in tags:
        for a in sit.world.oob(t).get("air") or []:
            if fm.plane_kind(sit.game, a.get("type")) == "fighter" and a.get("type") in cat.equip:
                frames.add(a["type"])
    best = (None, None)
    for t in tags:
        for v in sit.world.country(t).get("variants") or []:
            if v.get("type") not in frames:
                continue
            mods = {k: m for k, m in (v.get("modules") or {}).items() if m and m != "empty"}
            st = designer.evaluate(cat, v["type"], mods)["stats"]
            if best[0] is None or (st.get("air_superiority") or 0) > (best[0].get("air_superiority") or 0):
                best = (st, f"{t} {v.get('name') or v['type']} ({', '.join(sorted(mods.values()))})")
    if best[0] is None and frames:
        e = sorted(frames)[0]
        best = (designer.evaluate(cat, e, {})["stats"], f"{e} (no start variant: empty airframe)")
    return best


def battle_width(sit, tags):
    """(enemy width one battle holds, why)."""
    terr = sit.world.terrains()
    shares = None
    try:
        fp = theatre.front_provinces(sit.world, sit.tag, tags[0]) if tags else None
        tw = theatre.terrain_width(sit.world, sorted(fp)) if fp else None
        shares = (tw or {}).get("mix", {}).get("shares")
    except Exception:  # noqa: BLE001 - no map graph: plains below
        shares = None
    rows = {t: theatre.battle_widths(terr[t]).get(1) for t in (shares or {}) if t in terr}
    rows = {t: w for t, w in rows.items() if w}
    if rows:
        tot = sum(shares[t] for t in rows)
        w = sum(shares[t] * rows[t] for t in rows) / tot
        return w, f"terrain mix against {tags[0]}: " + ", ".join(f"{t} {shares[t]:.0%} x {rows[t]:g}" for t in rows)
    w = theatre.battle_widths(terr.get("plains") or {}).get(1)
    return (w or 0.0), "no map graph: one plains battle"


def need(sit, enemies=None, axes=1, fighter_ratio=None):
    """{fighters, cas, terms, why} for one front against `enemies` with `axes` attack axes."""
    tags, _ = sit.enemies(enemies)
    cat = designer.Catalog(sit.game)
    fighters, src = enemy_fighters(sit, tags)
    total = sum(fighters.values())
    theirs, their_e = _their_fighter(sit, cat, tags)
    bases, mods = designer.available(cat, set(sit.techs()))
    ours = (designer.best(cat, "fighter", bases, mods, top=1).get("designs") or [None])[0]
    q = None
    exch = None
    if ours and theirs:
        a_us = ours["stats"].get("air_superiority") or 0.0
        a_them = theirs.get("air_superiority") or 0.0
        q = (a_them / a_us) if a_us > 0 else None
        exch = designer.duel(ours["stats"], theirs)
    ratio = fighter_ratio if fighter_ratio is not None else 1.0
    ratio_src = "plan air_situation.fighter_ratio" if fighter_ratio is not None else \
        "parity (the plan gives no air_situation.fighter_ratio)"
    want_f = -(-int(total * (q if q is not None else 1.0) * ratio * 10) // 10) if total else 0
    per_width = float(_first(sit.define("LAND_AIR_COMBAT_MAX_PLANES_PER_ENEMY_WIDTH"), 0) or 0)
    width, wwhy = battle_width(sit, tags)
    cas = int(per_width * width * max(1, int(axes)))
    have_cas = any(fm.plane_kind(sit.game, b) == "cas" for b in bases)
    terms = {"enemy_fighters": fighters, "enemy_fighters_from": src, "their_airframe": their_e,
             "their_air_superiority": theirs.get("air_superiority") if theirs else None,
             "our_design": ({"base": ours["base"], "modules": ours["modules"]} if ours else None),
             "our_air_superiority": ours["stats"].get("air_superiority") if ours else None,
             "quality_ratio": round(q, 3) if q is not None else None, "exchange": exch,
             "fighter_ratio": ratio, "fighter_ratio_from": ratio_src,
             "planes_per_enemy_width": per_width, "battle_width": round(width, 1), "battle_width_from": wwhy,
             "axes": max(1, int(axes)), "src": sit.define_src("LAND_AIR_COMBAT_MAX_PLANES_PER_ENEMY_WIDTH")}
    why_f = (f"{total} enemy fighters ({src}) x quality {q:.2f} (their {their_e} air superiority "
             f"{terms['their_air_superiority']:g} / our best {terms['our_air_superiority']:g}) x {ratio:g} "
             f"({ratio_src}); exchange {exch} kills per loss" if q is not None else
             f"{total} enemy fighters ({src}) x {ratio:g} ({ratio_src}); no fighter of ours or theirs to compare")
    why_c = (f"{per_width:g} planes per enemy width x {width:.0f} width a battle ({wwhy}) x {max(1, int(axes))} axes"
             + ("" if have_cas else "; we have no close-air-support airframe yet"))
    return {"enemies": tags, "fighters": want_f, "cas": cas if have_cas else 0, "cas_cap": cas,
            "cas_requires": "the fighters win the air over the axes first", "terms": terms,
            "why": {"fighters": why_f, "cas": why_c}}
