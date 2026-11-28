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
  add_political_power, political_power_gain   political power at its shadow price: the best value per year per
                political power among what the country may buy now (common/ideas with a `cost`; the advisors of
                common/characters, their traits' modifiers from common/country_leader; not another country's, not
                shown only after a flag or a focus), at the mean value per power of the sinks the horizon's
                power buys best first (pp_price); power at completion buys an idea held for the years left (per
                year); a daily gain buys ideas through the years (per year squared / 2)
  scopes (if, every_owned_state, random_owned_controlled_state, a tag, a state) are walked; everything else is named
  in `unvalued` (army experience, events, wargoals, claims): the player's or the plan's judgement
Goals and constraints: ideology:<ideology> drops focuses that set another ruling party or add another ideology's
popularity; no_war:<TAG> drops focuses that lead to war with the tag or declare war on it or create a wargoal on it.
Order: one focus at a time, the best chain rate (value / days over chains of up to three focuses) among those whose
prerequisites are done and that no taken focus excludes.
"""
import os

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
        self.pp_price, self.pp_idea = pp_price(sit, self.base, horizon_years)

    def mods_coef(self, mods):
        """(per year held, per year squared from construction compounding, unvalued keys)."""
        lin, _, unvalued = value.held(mods, self.base, 1.0)
        quad = sum(v * value.weight(k, self.base) * self.rate / 2.0 for k, v in mods.items()
                   if value.WEIGHTS.get(k, ("",))[0] == "Civilian")
        if "political_power_gain" in unvalued and self.pp_price:
            unvalued = [u for u in unvalued if u != "political_power_gain"]
            quad += mods["political_power_gain"] * 365 * self.pp_price / 2.0
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
            elif k == "add_political_power" and _num(v) is not None and self.pp_price:
                self.add(out, f"political power {_num(v):g} (at {self.pp_idea})", lin=_num(v) * self.pp_price)
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


def _gate_open(block):
    """False when a visible/available block needs a flag or a focus not had now (top-level, not under NOT)."""
    if not isinstance(block, list):
        return True
    for a, b in block:
        if (a or "").lower() in ("has_country_flag", "has_completed_focus", "has_global_flag"):
            return False
        if (a or "").lower() == "always" and b == "no":
            return False
    return True


def _tags_of(block):
    tags, stack = set(), [block] if isinstance(block, list) else []
    while stack:
        for a, b in stack.pop():
            if (a or "").lower() in ("original_tag", "tag") and isinstance(b, str):
                tags.add(b)
            elif isinstance(b, list) and (a or "").upper() != "NOT":
                stack.append(b)
    return tags


def pp_sinks(sit):
    """[(name, kind, cost, {modifier: value})]: the priced ideas (common/ideas `cost`) and the advisors of our
    characters (common/characters advisor {cost, traits}; trait modifiers from common/country_leader; a character
    with no tag in `allowed` belongs to the country its file is named after) open now."""
    from ..script import parse_file

    def traits():
        out = {}
        for f in sit.game.files("common/country_leader"):
            for k, v in parse_file(f):
                if k == "leader_traits" and isinstance(v, list):
                    for name, body in v:
                        if name and isinstance(body, list):
                            out[name] = {a: _num(b) for a, b in body if _num(b) is not None}
        return out

    def ideas():
        rows = []
        for f in sit.game.files("common/ideas"):
            for k, v in parse_file(f):
                if k != "ideas" or not isinstance(v, list):
                    continue
                for cat, body in v:
                    for name, idea in (body if isinstance(body, list) else []):
                        if not name or not isinstance(idea, list) or _num(get1(idea, "cost")) is None:
                            continue
                        m = get1(idea, "modifier")
                        rows.append((name, cat, _num(get1(idea, "cost")), _tags_of(get1(idea, "allowed")),
                                     _gate_open(get1(idea, "allowed")) and _gate_open(get1(idea, "visible")),
                                     {a: _num(b) for a, b in m if _num(b) is not None} if isinstance(m, list) else {}))
        return rows

    def advisors():
        tr = sit.game.cached("eval:leader_traits", traits)
        rows = []
        for f in sit.game.files("common/characters"):
            owner = os.path.splitext(os.path.basename(f))[0]
            for k, v in parse_file(f):
                if k != "characters" or not isinstance(v, list):
                    continue
                for name, ch in v:
                    for a, adv in (ch if isinstance(ch, list) else []):
                        if a != "advisor" or not isinstance(adv, list) or _num(get1(adv, "cost")) is None:
                            continue
                        mods = {}
                        t = get1(adv, "traits")
                        for _, tn in (t if isinstance(t, list) else []):
                            for mk, mv in (tr.get(tn) or {}).items():
                                mods[mk] = mods.get(mk, 0.0) + mv
                        rows.append((get1(adv, "idea_token") or name, get1(adv, "slot"), _num(get1(adv, "cost")),
                                     _tags_of(get1(adv, "allowed")) or {owner},
                                     _gate_open(get1(adv, "allowed")) and _gate_open(get1(adv, "visible")), mods))
        return rows
    rows = sit.game.cached("eval:priced_ideas", ideas) + sit.game.cached("eval:advisors", advisors)
    have = set(sit.state.get("ideas") or ())
    return [(n, k, c, m) for n, k, c, tags, open_, m in rows
            if open_ and c and c > 0 and n not in have and (not tags or sit.tag in tags)]


def pp_price(sit, base, years=4.0):
    """(factory-years per year held per political power, how): the sinks (pp_sinks), best value per power first, one
    per law category and per advisor slot (political_advisor: [?: three slots]), are bought with the power the horizon
    brings (BASE_POLITICAL_POWER_INCREASE a day [?: the define's comment says weekly] + political_power_gain, x 365 x
    years); the price is their mean value per power [?: between the best sink (upper bound) and the last valued one;
    sinks value.py does not value, army chiefs and decisions among them, are not counted, so it is low].
    (0, None) when none is valued."""
    rows = []
    for name, kind, cost, mods in pp_sinks(sit):
        tot, _, _ = value.held(mods, base, 1.0)
        if tot > 0:
            rows.append((tot / cost, name, kind, cost))
    rows.sort(reverse=True)
    per_kind, picked = {}, []
    for r in rows:
        cap = 3 if r[2] == "political_advisor" else 1
        if per_kind.get(r[2], 0) < cap:
            per_kind[r[2]] = per_kind.get(r[2], 0) + 1
            picked.append(r)
    daily = float(_first_num(sit.define("BASE_POLITICAL_POWER_INCREASE"), 0.0)) + sit.modifier("political_power_gain")
    budget = max(0.0, daily) * 365 * years
    spent = got = 0.0
    names = []
    for ratio, name, kind, cost in picked:
        if spent >= budget:
            break
        spent += cost
        got += ratio * cost
        names.append(name)
    if not spent:
        return 0.0, None
    return got / spent, (f"mean of {len(names)} sinks the {budget:.0f} PP of {years:g} years buys, best first "
                         f"({', '.join(names[:3])}{', ...' if len(names) > 3 else ''}; {spent:.0f} PP valued)")


def _first_num(v, default):
    if isinstance(v, list):
        v = v[0] if v else default
    x = _num(v)
    return default if x is None else x


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
            "horizon_years": horizon_years, "base": walker.base, "build_rate": round(walker.rate, 3),
            "pp_price": round(walker.pp_price, 4), "pp_price_idea": walker.pp_idea}
