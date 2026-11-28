"""3. Research order: the value per technology against its days with the years-ahead penalty, the research bonuses
held and where to spend them, the next picks per slot.

Inputs, all read:
  common/technologies     research_cost, start_year, path (parents), XOR, categories, the country-wide modifiers at a
                          technology's top level, the unit and category blocks (divstats), enable_equipments
  common/defines          BASE_TECH_COST, BASE_YEAR_AHEAD_PENALTY_FACTOR, BASE_FACTORY_SPEED_MIL,
                          BASE_FACTORY_MAX_EFFICIENCY_FACTOR
  the situation           techs held, research slots, research_speed_factor, factories, the army (start OOB or the
                          snapshot), the enemy's most fielded template (evaluator 2's battle model), research bonuses

Days of a technology = research_cost x BASE_TECH_COST x (1 + BASE_YEAR_AHEAD_PENALTY_FACTOR x max(0, start_year - the
year it starts - ahead reduction)) / (1 + research speed + bonus).

Value in factory-years earned between the technology's completion and the horizon (T years left):
  country modifiers     evaluators/value.py (the pick_value weights): held T years; construction speed also counts the
                        factories it builds (compounding, T^2); research_speed_factor counts the research it speeds up:
                        x x slots x 365 x the mean value per research day of the best candidates, per year
  the army              the technology is added to our techs and the templates are rebuilt (divstats): the endurance
                        ratio R against the enemy template moves by x. The army standing (start OOB or snapshot,
                        IC-weighted) gains x_mod x (army IC / IC per factory-year) once from a unit or category
                        modifier (new equipment does not reach divisions already equipped); what is built later is
                        the line template evaluator 2 computes now (the army the plan fields, not the start's), and
                        gains (x_mod + x_eq) per military factory-year
  military factories    grow over the horizon: today's plus what the civilian factories not on consumer goods build
                        (build_rate per year each), integrated from the technology's completion to the horizon; the
                        country modifiers weighted by military factories use the same integral
  unlocks               sub-units, buildings, modules and designer chassis are listed; their worth arrives through the
                        technologies after them (the chain rate) and through evaluators 1 and 2 when fielded
Rank: the chain rate, the best (value / days) over chains of up to three technologies starting here, so a gateway is
worth what it opens. Slots are simulated in time: the slot that frees first takes the best rate available then (the
years-ahead penalty falls as the date moves).
"""
from .. import divstats, research as base_research
from . import design, template, value

CHAIN_DEPTH = 3


def _first(v, d):
    if isinstance(v, list):
        return v[0] if v else d
    return d if v is None else v


class Army:
    """Our fielded templates and their battle ratio against the enemy template, rebuilt for one more technology."""

    def __init__(self, sit, enemies=None):
        self.sit = sit
        self.b = template.Builder(sit, None, enemies, designs=False)
        self.enemy, self.enemy_why = template.enemy_division(self.b)
        self.have = set(sit.techs())
        self.doctrines = sit.doctrines()
        oob = sit.world.oob(sit.tag)
        counts = {}
        for d in oob.get("divisions") or []:
            counts[d.get("template")] = counts.get(d.get("template"), 0) + 1
        self.rows = []
        for name, n in counts.items():
            tpl = (oob.get("templates") or {}).get(name)
            if not tpl:
                continue
            bats, sup = divstats.template_units(tpl)
            if not bats or not all(self.b.fieldable(x) for x in bats + sup):
                continue
            ic, _, _ = self.b.cost(bats, sup)
            self.rows.append({"template": name, "n": n, "bats": bats, "sup": sup, "ic": ic})
        self.archs = sorted({self.b.data.archetype(e) for r in self.rows for x in r["bats"] + r["sup"]
                             for e in (self.b.data.units.get(x) or {}).get("need", {})})
        self.ic = sum(r["n"] * r["ic"] for r in self.rows)
        # what is built from now on: the line template computed for today's techs and enemy
        self.future = []
        try:
            t = (template.best(sit, "line", enemies=enemies, top=1).get("templates") or [None])[0]
        except Exception:  # noqa: BLE001 - no fieldable template: the standing army stands in
            t = None
        if t:
            bats = [k for k, n in t["battalions"].items() for _ in range(n)]
            sup = list(t["support"]) + list(t["regimental"])
            if all(self.b.fieldable(x) for x in bats + sup):
                ic, _, _ = self.b.cost(bats, sup)
                self.future = [{"template": "computed line", "n": 1, "bats": bats, "sup": sup, "ic": ic}]
        self.archs = sorted(set(self.archs) | {self.b.data.archetype(e) for r in self.future
                                               for x in r["bats"] + r["sup"]
                                               for e in (self.b.data.units.get(x) or {}).get("need", {})})
        self.base_mods = divstats.factors(self.b.data, self.have, self.doctrines)
        self.base_eq = dict(self.b.equipment)
        self.r0 = [self._r(r, self.base_mods, self.base_eq) for r in self.rows]
        self.f0 = [self._r(r, self.base_mods, self.base_eq) for r in self.future]

    def _r(self, row, mods, eq):
        if self.enemy is None:
            return 1.0
        div = divstats.division(self.b.data, row["bats"], row["sup"], eq, mods)
        if not div:
            return 1.0
        r_def, _ = template.battle(self.b.rules, div, self.enemy, False, self.b.prof)
        r_att, _ = template.battle(self.b.rules, div, self.enemy, True, self.b.prof)
        return (r_def * r_att) ** 0.5

    def lift(self, tech):
        """(x from unit and category modifiers on the standing army, x from new equipment on it, [equipment it brings],
        x of what is built later: the computed line template with both, or the standing army's when there is none)."""
        if not self.rows or self.ic <= 0:
            return 0.0, 0.0, [], 0.0
        techs = self.have | {tech}
        mods_new = self.b.data.techs.get(tech)
        eq = dict(self.base_eq)
        changed = []
        for a in self.archs:
            n = self.b.data.newest(a, techs)
            if n and n != self.base_eq.get(a):
                eq[a] = n
                changed.append(n)
        if not mods_new and not changed:
            return 0.0, 0.0, [], 0.0
        mods = divstats.factors(self.b.data, techs, self.doctrines) if mods_new else self.base_mods
        x_mod = x_eq = 0.0
        for row, r0 in zip(self.rows, self.r0):
            w = row["n"] * row["ic"] / self.ic
            r_mod = self._r(row, mods, self.base_eq) if mods_new else r0
            r_all = self._r(row, mods, eq) if changed else r_mod
            x_mod += w * (r_mod / r0 - 1)
            x_eq += w * (r_all / r_mod - 1)
        x_new = x_mod + x_eq
        if self.future:
            row, f0 = self.future[0], self.f0[0]
            x_new = self._r(row, mods, eq) / f0 - 1 if f0 else 0.0
        return x_mod, x_eq, changed, x_new


def unlocks(sit):
    def load():
        from ..script import bare, get, parse_file
        out = {}
        for p in sit.game.files("common/technologies"):
            for k, v in parse_file(p):
                if k != "technologies" or not isinstance(v, list):
                    continue
                for name, body in v:
                    if not name or not isinstance(body, list):
                        continue
                    u = []
                    for key in ("enable_subunits", "enable_equipments", "enable_equipment_modules", "enable_building"):
                        for b in get(body, key):
                            if isinstance(b, list):
                                u += [x for x in bare(b)] or [a for a, _ in b if a]
                    if u:
                        out[name] = u
        return out
    return sit.game.cached("eval:techunlocks", load)


class Model:
    def __init__(self, sit, horizon_years=4.0, enemies=None, overrides=None):
        self.sit = sit
        self.table = sit.tech_table()
        self.have = set(sit.techs())
        self.base = value.bases(sit)
        self.rate, self.rate_why = value.build_rate(sit)
        self.speed = sit.modifier("research_speed_factor")
        self.btc = float(_first(sit.define("BASE_TECH_COST"), 100))
        self.pen = float(_first(sit.define("BASE_YEAR_AHEAD_PENALTY_FACTOR"), 2))
        self.icfy = float(_first(sit.define("BASE_FACTORY_SPEED_MIL"), 4)) * 365 * \
            float(_first(sit.define("BASE_FACTORY_MAX_EFFICIENCY_FACTOR"), 50)) / 100.0
        self.horizon = horizon_years * 365.0
        self.overrides = overrides
        self.army = Army(sit, enemies)
        from .construction import consumer_goods
        cg, _ = consumer_goods(sit)
        # military factories added a year: every civilian factory not on consumer goods building them [?: the
        # construction evaluator's civilian years first are not modelled here; they add more later]
        self.cg = cg
        self.mil_growth = self.rate * max(0.0, self.base["Civilian"] * (1 - cg))
        self.mods = {t: m for t, m in _tech_mods(sit).items()}
        self.unl = unlocks(sit)
        self.coef = {}
        for tid, t in self.table.items():
            if tid in self.have or t.get("doctrine"):
                continue
            self.coef[tid] = self._coef(tid)
        own = sorted((self._value_at(t, self.days(t, 0)) / max(self.days(t, 0), 1.0) for t in self.coef
                      if self.startable(t, self.have)), reverse=True)
        n = max(1, int(self.base["Slots"]))
        self.mean_rate = sum(own[:n]) / n if own else 0.0
        for tid, c in self.coef.items():  # research speed is worth the research it speeds up
            x = self.mods.get(tid, {}).get("research_speed_factor", 0.0)
            if x:
                c["lin"] += x * self.base["Slots"] * 365 * self.mean_rate
                c["parts"]["research_speed_factor"] = round(x * self.base["Slots"] * 365 * self.mean_rate, 2)

    def _coef(self, tid):
        mods = self.mods.get(tid, {})
        lin = quad = stock = 0.0
        parts = {}
        unvalued = []
        per_mil = 0.0  # factory-years per military factory-year (integrated over the growing count)
        for k, v in mods.items():
            if k == "research_speed_factor":
                continue
            w = value.weight(k, self.base, self.overrides)
            if w is None:
                unvalued.append(k)
                continue
            if value.WEIGHTS.get(k, ("",))[0] == "Military" and not (self.overrides and k in self.overrides):
                per_mil += v * value.WEIGHTS[k][1]
                lin += v * w
            else:
                lin += v * w
            parts[k] = v * w
            if value.WEIGHTS.get(k, ("",))[0] == "Civilian":
                quad += v * w * self.rate / 2.0
        x_mod, x_eq, eq, x_new = self.army.lift(tid)
        if x_mod or x_eq or x_new:
            stock += x_mod * self.army.ic / self.icfy
            lin += x_new * self.base["Military"]
            per_mil += x_new
            parts["army"] = round(x_new, 4)
            if x_mod:
                parts["army_standing"] = round(x_mod, 4)
        return {"lin": lin, "quad": quad, "stock": stock, "parts": parts, "unvalued": unvalued, "per_mil": per_mil,
                "x_mod": x_mod, "x_eq": x_eq, "x_new": x_new, "equipment": eq, "unlocks": self.unl.get(tid, [])}

    def years_left(self, day_done):
        return max(0.0, (self.horizon - day_done) / 365.0)

    def _value_at(self, tid, day_done):
        c = self.coef[tid]
        T = self.years_left(day_done)
        if T <= 0:
            return 0.0
        h, t0 = self.horizon / 365.0, day_done / 365.0
        grown = c.get("per_mil", 0.0) * self.mil_growth * (h * h - t0 * t0) / 2.0  # the factories added meanwhile
        return c["lin"] * T + c["quad"] * T * T + c["stock"] + grown

    def days(self, tid, day, bonus=0.0, ahead_reduction=0.0):
        t = self.table[tid]
        year = self.sit.year + int(day // 365)
        ahead = max(0.0, (t["year"] or 0) - year - ahead_reduction)
        return t["cost"] * self.btc * (1 + self.pen * ahead) / max(0.1, 1 + self.speed + bonus)

    def startable(self, tid, done):
        t = self.table[tid]
        if tid in done or t.get("doctrine"):
            return False
        if set(t.get("xor") or ()) & done:
            return False
        if t.get("parents") and not all(p in done for p in t["parents"]):
            return False
        return all(d in done for d in t.get("deps") or [])

    def chain_rate(self, tid, done, day):
        """(best value per research day over chains from tid, the chain)."""
        best = (-1.0, [tid])
        children = {}
        for c, t in self.table.items():
            for p in t.get("parents") or ():
                children.setdefault(p, []).append(c)

        def walk(chain, done_now, d, v, depth):
            nonlocal best
            r = v / max(d, 1.0)
            if r > best[0]:
                best = (r, list(chain))
            if depth >= CHAIN_DEPTH:
                return
            for c in children.get(chain[-1], ()):
                if c in self.coef and self.startable(c, done_now):
                    dd = self.days(c, day + d)
                    walk(chain + [c], done_now | {c}, d + dd, v + self._value_at(c, day + d + dd), depth + 1)
        d0 = self.days(tid, day)
        walk([tid], done | {tid}, d0, self._value_at(tid, day + d0), 1)
        return best

    def plan(self, slots=None, picks=3, bonuses=()):
        slots = slots or int(self.base["Slots"])
        free = [0.0] * slots
        done = set(self.have)
        running = {}
        out = [[] for _ in range(slots)]
        bonuses = [dict(b) for b in bonuses]
        while min(len(x) for x in out) < picks:
            i = min(range(slots), key=lambda s: (free[s], s))
            day = free[i]
            finished = {t for t, d in running.items() if d <= day + 1e-9}
            done |= finished
            for t in finished:
                running.pop(t)
            cands = [t for t in self.coef if t not in running and self.startable(t, done)
                     and not set(self.table[t].get("xor") or ()) & set(running)]
            if not cands:
                break
            scored = sorted(((self.chain_rate(t, done, day), t) for t in cands), key=lambda x: -x[0][0])
            (rate, chain), tid = scored[0]
            use = _bonus_for(self, tid, day, bonuses)
            d = self.days(tid, day, use["bonus"] if use else 0.0, use.get("ahead_reduction", 0.0) if use else 0.0)
            running[tid] = day + d
            free[i] = day + d
            c = self.coef[tid]
            out[i].append({"tech": tid, "start_day": round(day), "days": round(d), "value": round(
                self._value_at(tid, day + d), 1), "rate": round(rate, 4), "chain": chain,
                "parts": {k: round(v, 2) for k, v in c["parts"].items()}, "unvalued": c["unvalued"],
                "equipment": c["equipment"], "unlocks": c["unlocks"][:6], "bonus": use,
                "next_best": [(t, round(r[0], 4)) for r, t in scored[1:3]]})
        return out


def _tech_mods(sit):
    from .context import tech_country_modifiers
    return tech_country_modifiers(sit.game)


def _bonus_for(m, tid, day, bonuses):
    """Spend a held bonus on this technology when it is the best use in sight: the days it saves here are at least
    those it would save on any other startable candidate of its categories within the next picks."""
    cats = set(m.table[tid].get("categories") or ())
    for b in bonuses:
        if b.get("uses", 1) <= 0 or not (cats & set(b.get("categories") or ())):
            continue
        save = m.days(tid, day) - m.days(tid, day, b.get("bonus", 0.0), b.get("ahead_reduction", 0.0))
        rivals = [t for t in m.coef if t != tid and set(m.table[t].get("categories") or ()) &
                  set(b.get("categories") or ()) and m.startable(t, m.have)]
        best_other = max((m.days(t, day) - m.days(t, day, b.get("bonus", 0.0), b.get("ahead_reduction", 0.0))
                          for t in rivals), default=0.0)
        if save >= best_other * 0.999:
            b["uses"] = b.get("uses", 1) - 1
            return {"name": b.get("name"), "bonus": b.get("bonus", 0.0),
                    "ahead_reduction": b.get("ahead_reduction", 0.0), "days_saved": round(save)}
    return None


def bonus_advice(m, bonuses):
    """For each held bonus: the technology it saves the most days on now, and whether to spend it now."""
    out = []
    for b in bonuses:
        cats = set(b.get("categories") or ())
        rows = []
        for t in m.coef:
            if not (cats & set(m.table[t].get("categories") or ())):
                continue
            save = m.days(t, 0) - m.days(t, 0, b.get("bonus", 0.0), b.get("ahead_reduction", 0.0))
            rows.append((save, m.startable(t, m.have), t))
        rows.sort(reverse=True)
        now = [r for r in rows if r[1]]
        out.append({"bonus": b, "best_now": now[0][2] if now else None,
                    "days_saved_now": round(now[0][0]) if now else 0,
                    "best_any": rows[0][2] if rows else None, "days_saved_best": round(rows[0][0]) if rows else 0,
                    "advice": ("spend now on " + now[0][2]) if now and rows and now[0][0] >= rows[0][0] * 0.8 else
                              (f"hold for {rows[0][2]} (saves {round(rows[0][0])} days, now best "
                               f"{round(now[0][0]) if now else 0})" if rows else "no technology of its categories")})
    return out


def evaluate(sit, slots=None, picks=3, horizon_years=4.0, enemies=None, overrides=None):
    m = Model(sit, horizon_years, enemies, overrides)
    bonuses = list(sit.state.get("research_bonuses") or [])
    picks_out = m.plan(slots, picks, bonuses)
    ranked = []
    for t in m.coef:
        if m.startable(t, m.have):
            d = m.days(t, 0)
            ranked.append((m._value_at(t, d) / max(d, 1.0), t, d))
    ranked.sort(reverse=True)
    return {"slots": slots or int(m.base["Slots"]), "picks": picks_out, "bonuses": bonus_advice(m, bonuses),
            "top_now": [{"tech": t, "rate": round(r, 4), "days": round(d), "parts": m.coef[t]["parts"]}
                        for r, t, d in ranked[:8]],
            "model": {"research_speed": m.speed, "base": m.base, "ic_per_factory_year": round(m.icfy, 1),
                      "build_rate": round(m.rate, 3), "build_rate_why": m.rate_why, "horizon_years": horizon_years,
                      "military_growth_per_year": round(m.mil_growth, 2), "consumer_goods": m.cg,
                      "future_template": (m.army.future[0]["bats"][:1] + [len(m.army.future[0]["bats"])] +
                                          m.army.future[0]["sup"]) if m.army.future else None,
                      "mean_rate": round(m.mean_rate, 4), "army_ic": round(m.army.ic), "army_templates":
                      len(m.army.rows), "enemy_template": m.army.enemy_why,
                      "defines": [sit.define_src(k) for k in ("BASE_TECH_COST", "BASE_YEAR_AHEAD_PENALTY_FACTOR",
                                                              "BASE_FACTORY_SPEED_MIL",
                                                              "BASE_FACTORY_MAX_EFFICIENCY_FACTOR")]},
            "techs": sit.techs_source(), "method": base_research.DAYS_METHOD}
