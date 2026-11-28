"""raids.py: the raid system from the game files, for any country and mod (common/raids, its categories, the NRaids
defines).

A raid type names a category (its DLC and tech gate), the command power it holds, the days it prepares, its gates
(`allowed` for the tag and DLC, `visible` before any target, `show_target` and `available` per target to prepare,
`launchable` to start, `launchable_from` for the starting point's owner), the units it takes (`unit_requirements`:
battalion types with a minimum, or air equipment of a type and amount), the equipment it collects (`essential_equipment`
needed to create it, `additional_equipment` collected after; the larger of the two counts), its targets (a province, a
building, a state trigger), its starting points (air base, naval base, rocket site, carrier, submarine, a building),
whether it fires once and its target cooldown (`days_re_enable`, else RAID_DEFAULT_TARGET_COOLDOWN_DAYS), the success
formulas (`success`, `critical` conditional on success, `disaster`) and the effects of each outcome level.

Success chance (the game's documented formula): base plus, for each modifier, a linear term from `start_weight` at
`start_reference` (default 0) to `weight` at `reference` (default 1), clamped to that range; the total is clamped to
0..1. `chance(formula, inputs)` evaluates it for the inputs a plan knows (prep_time 1.0 at launch, experience,
air_superiority as a fraction, anti_air levels, enemy_units in the province, intel, organisation, strength, ...); an
input not given counts at its neutral point (no term), and the result lists the terms it could not evaluate.

Value (an EVALUATOR, not a table): a raid is worth preparing when its target lies on an operation's axis or landing and
P(success) x what the outcome removes from the defence beats what it holds (command power, the battalions or wings for
the preparation days, the transports). `rows(...)` lists every raid allowed for a tag with its gates still to meet
(focus, tech), so the strategist plans the prerequisites (the tech, the battalions, the transports' production).
"""
from .script import bare, get1, parse_file, render

SUCCESS_TERMS = ("success", "critical", "disaster")
DEFAULT_COOLDOWN_DEFINE = "RAID_DEFAULT_TARGET_COOLDOWN_DAYS"


def _num(v, default=None):
    try:
        return float(v)
    except (TypeError, ValueError):
        return default


def _block(items, key):
    v = get1(items, key)
    return v if isinstance(v, list) else []


def _numbers(items):
    return {k: _num(v) for k, v in items if _num(v) is not None}


def formula(items):
    """A success formula block -> {"base": b, "terms": {name: {weight, reference, start_weight, start_reference}}}."""
    out = {"base": _num(get1(items, "base"), 0.0), "terms": {}}
    for k, v in items:
        if k == "base" or not isinstance(v, list):
            continue
        out["terms"][k] = {"weight": _num(get1(v, "weight"), 0.0), "reference": _num(get1(v, "reference"), 1.0),
                           "start_weight": _num(get1(v, "start_weight"), 0.0),
                           "start_reference": _num(get1(v, "start_reference"), 0.0)}
    return out


def term(t, x):
    """One modifier's contribution at input x."""
    lo, hi = t["start_reference"], t["reference"]
    if hi == lo:
        f = 1.0 if x >= hi else 0.0
    else:
        f = min(1.0, max(0.0, (x - lo) / (hi - lo)))
    return t["start_weight"] + (t["weight"] - t["start_weight"]) * f


def chance(f, inputs):
    """(probability, {term: contribution}, [terms without an input]) of a formula for the given inputs."""
    if not f:
        return 0.0, {}, []
    p = f["base"]
    parts, missing = {}, []
    for name, t in f["terms"].items():
        if name not in inputs:
            missing.append(name)
            continue
        c = term(t, float(inputs[name]))
        parts[name] = round(c, 4)
        p += c
    return max(0.0, min(1.0, p)), parts, missing


def outcome_probabilities(success_factors, inputs):
    """{failure_or_disaster, limited_success, success, critical_success} from the three formulas: the game draws
    disaster first (not conditional), then success, then critical within success. Limited success is a success roll
    that missed: the game's own split between failure and limited success is not documented, so `not_success` is
    given as one number with the disaster share inside it."""
    ps, _, _ = chance(success_factors.get("success"), inputs)
    pc, _, _ = chance(success_factors.get("critical"), inputs)
    pd, _, _ = chance(success_factors.get("disaster"), inputs)
    return {"disaster": round(pd, 4), "success_or_better": round((1 - pd) * ps, 4),
            "critical": round((1 - pd) * ps * pc, 4), "not_success": round(1 - (1 - pd) * ps, 4)}


EFFECT_KEYS = ("damage_building", "add_dynamic_modifier", "add_timed_idea", "add_ideas", "raid_damage_units",
               "raid_add_unit_experience", "add_war_support", "add_stability", "air_experience", "army_experience",
               "navy_experience", "add_divisional_commander_xp", "damage_units", "add_resistance", "set_country_flag")


def _effects(items, limit=12):
    """Short list of what an outcome does: the raid's own effects (building damage, dynamic modifiers and ideas with
    their days, unit damage and experience, war support) found anywhere in the block, scopes walked."""
    out = []

    def walk(block):
        for k, v in block:
            if len(out) >= limit:
                return
            if k in EFFECT_KEYS:
                if isinstance(v, list):
                    keep = [(a, b) for a, b in v if a in ("type", "damage", "modifier", "idea", "days", "org_damage",
                                                          "str_damage", "province")]
                    out.append(k + " " + render(keep, 90))
                else:
                    out.append(f"{k} {v}")
            elif isinstance(v, list):
                walk(v)

    if isinstance(items, list):
        walk(items)
    return out


def _requirements(raid):
    out = []
    for k, v in raid:
        if k != "unit_requirements" or not isinstance(v, list):
            continue
        row = {}
        bt = _block(v, "battalion_types")
        if bt:
            row["battalions"] = {name: _num(get1(b, "min"), 1.0) if isinstance(b, list) else 1.0 for name, b in bt}
        eq = _block(v, "equipment")
        if eq:
            types = get1(eq, "type")
            amount = _block(eq, "amount")
            row["equipment"] = {"types": bare(types) if isinstance(types, list) else [types],
                                "min": _num(get1(amount, "min")), "max": _num(get1(amount, "max")),
                                "modules": bare(_block(eq, "modules"))}
        out.append(row)
    return out


def _targets(raid):
    out = []
    for key in ("target_type", "target_types"):
        for k, v in raid:
            if k != key or not isinstance(v, list):
                continue
            for tk, tv in v:
                if tk == "province":
                    ids = bare(tv) if isinstance(tv, list) else [tv]
                    out.append({"province": ids})
                elif tk == "building":
                    out.append({"building": get1(tv, "type") or get1(tv, "tags"), "level": _numbers(_block(tv, "level")),
                                "coastal": get1(tv, "is_coastal") == "yes"})
                elif tk == "state":
                    out.append({"state": render(tv, 160) if isinstance(tv, list) else tv})
    return out


def _gate_terms(items):
    """The plain prerequisites a gate names: focuses, techs, DLCs, tags (other triggers stay as text)."""
    out = {"focus": [], "tech": [], "dlc": [], "tag": [], "other": []}
    for k, v in items:
        if k == "has_completed_focus":
            out["focus"].append(v)
        elif k == "has_tech":
            out["tech"].append(v)
        elif k == "has_dlc":
            out["dlc"].append(v)
        elif k in ("tag", "original_tag"):
            out["tag"].append(v)
        else:
            out["other"].append(render([(k, v)], 120))
    return {k: v for k, v in out.items() if v}


class Raids:
    """Raid categories and types of the effective files. `game` is a hoi4facts.game.Game; `cooldown` the define."""

    def __init__(self, game, cooldown=365.0):
        self.game = game
        self.cooldown = cooldown
        self.categories = {}
        self.types = {}
        for p in game.files("common/raids/categories"):
            for k, v in parse_file(p):
                if k == "categories" and isinstance(v, list):
                    self.categories.update((c, b) for c, b in v if isinstance(b, list))
        for p in game.files("common/raids"):
            for k, v in parse_file(p):
                if k == "types" and isinstance(v, list):
                    self.types.update((t, b) for t, b in v if isinstance(b, list) and get1(b, "category"))

    def row(self, rid):
        r = self.types[rid]
        cat = get1(r, "category")
        cblock = self.categories.get(cat, [])
        add = _numbers(_block(r, "additional_equipment"))
        ess = _numbers(_block(r, "essential_equipment"))
        sf = {k: formula(_block(_block(r, "success_factors"), k)) for k in SUCCESS_TERMS
              if _block(_block(r, "success_factors"), k)}
        levels = {}
        for lv, b in _block(r, "success_levels"):
            if isinstance(b, list):
                levels[lv] = {"actor": _effects(_block(b, "actor_effects")),
                              "victim": _effects(_block(b, "victim_effects")),
                              "destroy_additional_equipment": _num(get1(b, "destroy_additional_equipment"), 1.0)}
        days_re = _num(get1(r, "days_re_enable"))
        return {
            "id": rid, "category": cat,
            "category_gates": {"visible": _gate_terms(_block(cblock, "visible")),
                               "available": _gate_terms(_block(cblock, "available"))},
            "command_power": _num(get1(r, "command_power"), _num(get1(r, "cost"), 0.0)),
            "days_to_prepare": _num(get1(r, "days_to_prepare"), _num(get1(r, "preparation_time"))),
            "fire_only_once": get1(r, "fire_only_once") == "yes",
            "cooldown_days": days_re if days_re is not None else self.cooldown,
            "max_distance": _num(get1(r, "max_distance")),
            "allowed": _gate_terms(_block(r, "allowed")), "visible": _gate_terms(_block(r, "visible")),
            "show_target": render(_block(r, "show_target"), 200), "available": render(_block(r, "available"), 200),
            "launchable": render(_block(r, "launchable"), 200),
            "units": _requirements(r),
            "equipment": {k: max(add.get(k, 0), ess.get(k, 0)) for k in set(add) | set(ess)},
            "essential_equipment": ess,
            "targets": _targets(r),
            "starting_points": bare(_block(_block(r, "starting_point"), "types")),
            "success_factors": sf, "outcomes": levels,
        }

    def allowed_for(self, rid, ctx):
        """True / False / None (unknown) for the type's `allowed` and its category's `visible` (the static gates: tag,
        DLC) for a country; the category's `available` (a tech) is a prerequisite, listed in `needs`."""
        from .script import trigger
        r = self.types[rid]
        a = trigger(_block(r, "allowed"), ctx) if _block(r, "allowed") else True
        cat = self.categories.get(get1(r, "category"), [])
        c = trigger(_block(cat, "visible"), ctx) if _block(cat, "visible") else True
        if a is False or c is False:
            return False
        return True if (a and c) else None

    def rows(self, ctx, techs=(), focuses=(), inputs=None):
        """Every raid type the country may have (allowed not False), with the prerequisites still missing
        (`needs`: techs and focuses from visible/category gates not held) and the outcome probabilities at
        `inputs` (default: full preparation only)."""
        inputs = dict(inputs or {"prep_time": 1.0})
        out = []
        for rid in sorted(self.types):
            ok = self.allowed_for(rid, ctx)
            if ok is False:
                continue
            row = self.row(rid)
            gates = [row["visible"], row["category_gates"]["visible"], row["category_gates"]["available"]]
            row["needs"] = {"tech": sorted({t for g in gates for t in g.get("tech", []) if t not in techs}),
                            "focus": sorted({f for g in gates for f in g.get("focus", []) if f not in focuses})}
            row["allowed_known"] = ok is True
            row["probabilities"] = outcome_probabilities(row["success_factors"], inputs)
            p, parts, missing = chance(row["success_factors"].get("success"), inputs)
            row["success_terms"] = {"given": parts, "not_given": missing}
            out.append(row)
        return out


METHOD = ("raids.py: raid types from common/raids (any country and mod); success = base + linear terms from "
          "start_weight at start_reference to weight at reference, clamped; disaster drawn first, critical within "
          "success. Inputs not given add no term (listed as not_given). Value: prepare a raid whose target lies on an "
          "operation's axis when P(success) x the defence its outcome removes beats the command power, units and "
          "transports it holds for its preparation days.")
