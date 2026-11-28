"""7. Focus value: each focus's parsed rewards in factory-years against its days, under the plan's goals and constraints.

Inputs, all read: the tree the game loads for the country (hoi4facts.focus.tree: cost x FOCUS_POINT_DAYS, prerequisite
OR groups, mutually exclusive focuses, will_lead_to_war_with), each focus's completion_reward block, common/ideas
(modifiers), common/dynamic_modifiers (a variable the country's dynamic modifiers read), common/buildings.

Rewards valued (factory-years earned from the focus's completion to the horizon, T years):
  add_ideas, add_timed_idea (days), swap_ideas, remove_ideas, modifying a dynamic modifier's variable (set_variable,
  add_to_variable on a variable a held dynamic modifier reads)   modifiers through evaluators/value.py (the pick_value
                weights), construction speed compounding; a timed idea for its days only
  add_building_construction   factories built at once: each factory produces for T (a civilian factory as the
                construction it adds: the military factories it builds producing for the rest of T)
  add_research_slot           the weight of a research slot (research_speed_factor's base: Slots) for T
  add_tech_bonus              the days it saves on the best technology of its categories (evaluator 3) at the mean value
                per research day
  scopes (if, every_owned_state, random_owned_controlled_state, a tag, a state) are walked; everything else is named
  in `unvalued` (political power, army experience, events, wargoals, claims): the player's or the plan's judgement
Goals and constraints: ideology:<ideology> drops focuses that set another ruling party or add another ideology's
popularity; no_war:<TAG> drops focuses that lead to war with the tag or declare war on it or create a wargoal on it.
Order: one focus at a time, the best chain rate (value / days over chains of up to three focuses) among those whose
prerequisites are done and that no taken focus excludes.
"""
from .. import derive
from .. import focus as focusmod
from ..script import get1
from . import value

CHAIN_DEPTH = 3
SCOPE_KEYS = {"if", "else", "else_if", "hidden_effect", "every_owned_state", "random_owned_state",
              "random_owned_controlled_state", "every_controlled_state", "capital_scope", "ROOT", "effect"}
SKIP = {"limit", "tooltip", "custom_effect_tooltip", "ai_will_do", "allowed", "available", "visible", "trigger"}


def _num(v):
    try:
        return float(v)
    except (TypeError, ValueError):
        return None


class Walker:
    def __init__(self, sit, horizon_years=4.0, research=None):
        self.sit = sit
        self.base = value.bases(sit)
        self.rate, _ = value.build_rate(sit)
        self.ideas = derive.idea_modifiers(sit.game)
        self.horizon = horizon_years
        self.research = research
        dyn = sit.dynamic_modifier_table()
        held = set(sit.dynamic_modifier_names())
        self.var_mods = {}
        for dm in held:
            for k, raw in dyn.get(dm) or []:
                if _num(raw) is None and isinstance(raw, str):
                    self.var_mods.setdefault(raw, []).append(k)
        self.vars = sit.variables()

    def mods_coef(self, mods):
        """(per year held, per year squared from construction compounding, unvalued keys)."""
        lin, _, unvalued = value.held(mods, self.base, 1.0)
        quad = sum(v * value.weight(k, self.base) * self.rate / 2.0 for k, v in mods.items()
                   if value.WEIGHTS.get(k, ("",))[0] == "Civilian")
        return lin, quad, unvalued

    @staticmethod
    def add(out, label, lin=0.0, quad=0.0, stock=0.0):
        out["lin"] += lin
        out["quad"] += quad
        out["stock"] += stock
        out["parts"][label] = {k: round(v, 3) for k, v in (("per_year", lin), ("per_year2", quad), ("once", stock))
                               if v}

    def walk(self, block, years, out, scope_tag=None):
        for k, v in block:
            if not k or k in SKIP:
                continue
            if k in ("add_ideas", "add_timed_idea", "swap_ideas", "remove_ideas"):
                self._ideas(k, v, years, out)
            elif k in ("set_variable", "add_to_variable") and isinstance(v, list):
                self._variable(k, v, years, out)
            elif k == "add_building_construction" and isinstance(v, list):
                self._building(v, years, out)
            elif k == "add_research_slot":
                n = _num(v) or 0.0
                w = value.weight("research_speed_factor", self.base)
                self.add(out, "research slot", lin=n * w / max(self.base["Slots"], 1.0))
            elif k == "add_tech_bonus" and isinstance(v, list):
                self._tech_bonus(v, out)
            elif k in ("set_politics", "add_popularity") and isinstance(v, list):
                ideo = get1(v, "ruling_party") or get1(v, "ideology")
                if ideo and ideo.islower():
                    out["ideologies"].add(ideo)
                out["unvalued"].append(k)
            elif k in ("declare_war_on", "create_wargoal") and isinstance(v, list):
                t = get1(v, "target")
                if t:
                    out["war_with"].add(t)
                out["unvalued"].append(k)
            elif isinstance(v, list) and (k in SCOPE_KEYS or k.isupper() or k.isdigit() or k.startswith("every_")
                                          or k.startswith("random_")):
                self.walk(v, years, out, scope_tag)
            else:
                out["unvalued"].append(k)

    def _ideas(self, k, v, years, out):
        names, days = [], None
        if k == "add_timed_idea" and isinstance(v, list):
            names, days = [get1(v, "idea")], _num(get1(v, "days"))
        elif k == "swap_ideas" and isinstance(v, list):
            names = [get1(v, "add_idea")]
            rem = get1(v, "remove_idea")
            if rem:
                self._apply(self.ideas.get(rem) or {}, years, out, f"-{rem}", -1)
        elif isinstance(v, list):
            names = [x for a, x in v if a is None and isinstance(x, str)] or [a for a, _ in v if a]
        elif isinstance(v, str):
            names = [v]
        sign = -1 if k == "remove_ideas" else 1
        for n in names:
            if not n:
                continue
            self._apply(self.ideas.get(n) or {}, years, out, n, sign, days)

    def _apply(self, mods, years, out, label, sign=1, days=None):
        if not mods:
            out["unvalued"].append(f"idea {label} (no modifiers)")
            return
        lin, quad, unvalued = self.mods_coef({k: sign * x for k, x in mods.items()})
        if days:  # a timed idea: held its days, once
            y = days / 365.0
            self.add(out, label, stock=lin * y + quad * y * y)
        else:
            self.add(out, label, lin=lin, quad=quad)
        out["unvalued"] += [f"{label}:{u}" for u in unvalued]

    def _variable(self, k, v, years, out):
        for name, raw in v:
            if name in ("var", "value"):
                name, raw = get1(v, "var"), get1(v, "value")
            x = _num(raw)
            if name in self.var_mods and x is not None:
                delta = x - self.vars.get(name, 0.0) if k == "set_variable" else x
                self.vars[name] = self.vars.get(name, 0.0) + delta
                lin, quad, _ = self.mods_coef({m: delta for m in self.var_mods[name]})
                self.add(out, f"variable {name}", lin=lin, quad=quad)
                return
        out["unvalued"].append(k)

    def _building(self, v, years, out):
        from .context import production_buildings
        kind = production_buildings(self.sit.game).get(get1(v, "type"))
        lvl = _num(get1(v, "level")) or 0.0
        label = f"build {get1(v, 'type')} x{lvl:g}"
        if kind in ("military", "naval"):
            self.add(out, label, lin=lvl)
        elif kind == "general":
            self.add(out, label, quad=lvl * self.rate / 2.0)
        else:
            out["unvalued"].append(f"build {get1(v, 'type')}")

    def _tech_bonus(self, v, out):
        if not self.research:
            out["unvalued"].append("add_tech_bonus")
            return
        from . import research as rs
        b = {"categories": [x for a, x in v if a == "category"], "bonus": _num(get1(v, "bonus")) or 0.0,
             "ahead_reduction": _num(get1(v, "ahead_reduction")) or 0.0, "uses": _num(get1(v, "uses")) or 1}
        techs = [x for a, x in v if a == "technology"]
        m = self.research
        # the technologies it can serve inside the research window: start year up to now + its ahead reduction
        window = self.sit.year + max(1.0, b["ahead_reduction"])
        cands = [t for t in m.coef if (m.table[t]["year"] or 0) <= window and (
            (techs and t in techs) or set(m.table[t].get("categories") or ()) & set(b["categories"]))]
        save = max((m.days(t, 0) - m.days(t, 0, b["bonus"], b["ahead_reduction"]) for t in cands), default=0.0)
        self.add(out, "tech bonus " + ",".join(b["categories"] or techs), stock=save * m.mean_rate * b["uses"])
        del rs


def blocks(sit):
    trees, shared = focusmod._focus_blocks(sit.world)
    out = {}
    for t in trees.values():
        for b, _, _ in t["focuses"]:
            out[get1(b, "id")] = b
    for fid, (b, _, _) in shared.items():
        out.setdefault(fid, b)
    return out


def evaluate(sit, goals=(), horizon_years=4.0, count=12, done=(), research_model=None):
    tree = focusmod.tree(sit.world, sit.tag)
    focuses = tree.get("focuses") or {}
    raw = blocks(sit)
    if research_model is False:
        research_model = None
    elif research_model is None:
        try:
            from . import research as rs
            research_model = rs.Model(sit, horizon_years)
        except Exception:  # noqa: BLE001 - no research model: tech bonuses unvalued
            research_model = None
    walker = Walker(sit, horizon_years, research_model)
    ideology = [g.split(":", 1)[1] for g in goals if g.startswith("ideology:")]
    no_war = {g.split(":", 1)[1] for g in goals if g.startswith("no_war:")}
    info, dropped = {}, {}
    for fid, row in focuses.items():
        out = {"lin": 0.0, "quad": 0.0, "stock": 0.0, "parts": {}, "unvalued": [], "ideologies": set(),
               "war_with": set()}
        reward = get1(raw.get(fid) or [], "completion_reward")
        days = float(row.get("days") or 0)
        if isinstance(reward, list):
            walker.walk(reward, 1.0, out)  # per year held; scaled by the years left at completion below
        if row.get("war_with"):
            out["war_with"].add(row["war_with"])
        if ideology and out["ideologies"] - set(ideology):
            dropped[fid] = f"sets {sorted(out['ideologies'])} against the goal {ideology}"
            continue
        if out["war_with"] & no_war:
            dropped[fid] = f"war with {sorted(out['war_with'] & no_war)}"
            continue
        info[fid] = dict(out, days=days, ideologies=sorted(out["ideologies"]), war_with=sorted(out["war_with"]))
    # value at completion: per-year parts x years left (construction already counts T^2/2 at T = 1 -> rescale)
    H = horizon_years * 365.0

    def val(fid, day_done):
        T = max(0.0, (H - day_done) / 365.0)
        i = info[fid]
        return i["lin"] * T + i["quad"] * T * T + (i["stock"] if T > 0 else 0.0)

    taken, order, day = set(done), [], 0.0
    excl = {fid: set(focuses[fid].get("exclusive") or ()) for fid in focuses}

    def ok(fid, have):
        if fid in have or fid not in info:
            return False
        if excl.get(fid, set()) & have:
            return False
        return all(any(p in have for p in grp) for grp in focuses[fid].get("prereq") or [])

    children = {}
    for fid, row in focuses.items():
        for grp in row.get("prereq") or []:
            for p in grp:
                children.setdefault(p, []).append(fid)

    def chain(fid, have, d0):
        best = (-1.0, [fid])

        def walk(ch, hv, d, v, depth):
            nonlocal best
            r = v / max(d, 1.0)
            if r > best[0]:
                best = (r, list(ch))
            if depth >= CHAIN_DEPTH:
                return
            for c in children.get(ch[-1], ()):
                if ok(c, hv):
                    dd = info[c]["days"]
                    walk(ch + [c], hv | {c}, d + dd, v + val(c, d0 + d + dd), depth + 1)
        dd = info[fid]["days"]
        walk([fid], have | {fid}, dd, val(fid, d0 + dd), 1)
        return best

    while len(order) < count:
        cands = [f for f in info if ok(f, taken)]
        if not cands:
            break
        scored = sorted(((chain(f, taken, day), f) for f in cands), key=lambda x: -x[0][0])
        (rate, ch), fid = scored[0]
        d = info[fid]["days"]
        row = focuses[fid]
        order.append({"focus": fid, "name": row.get("name"), "start_day": round(day), "days": d,
                      "value": round(val(fid, day + d), 1), "rate": round(rate, 4), "chain": ch,
                      "parts": info[fid]["parts"], "unvalued": sorted(set(info[fid]["unvalued"]))[:8],
                      "gated": row.get("available"), "next_best": [(f, round(r[0], 4)) for r, f in scored[1:3]]})
        taken.add(fid)
        day += d
    return {"order": order, "dropped": dropped, "goals": list(goals), "tree": tree.get("id"),
            "horizon_years": horizon_years, "base": walker.base, "build_rate": round(walker.rate, 3)}
