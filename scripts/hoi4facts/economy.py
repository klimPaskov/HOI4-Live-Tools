"""Economy by arithmetic for the strategist and the lint: where a country may build (construction risk) and
what the plan's own levers give by date (the forecast).

Construction risk. A state is barred from new factories only when the plan has a reason to expect it lost
within its horizon: it lies within K province steps of the land of an enemy the plan fights (a war at the start,
a declaration the plan dates, a goal predicate on that enemy) AND that enemy's side can take it. The side is the
enemy, its faction partners at the start that the plan also fights, and its subjects or overlord; its strength is
the civilian plus military factories it owns at the start against ours. A plan that attacks the enemy (a goal
capitulation, a dated declaration or a vp target in its land) is at risk only from a side at least
ATTACK_RISK_RATIO times as strong; a plan that defends from DEFEND_RISK_RATIO. Adjacency alone is no risk: a
country that plans to attack builds in its border states. Growth after the start, coalitions that form later and
strategic bombing are not modelled; a plan that expects them names the states itself.

Forecast. A day-by-day construction simulation from the start to the horizon that counts every lever the plan
dates: the start laws and ideas, its politics picks (laws replace the law of their group), the ideas, factories,
slots and annexations its focus path gives on the day each focus completes, the techs of its research path on the
day each finishes (construction speed, building-slot factors), the states its checkpoints expect to own (with the
factories and slots in them), consumer goods from the active laws and ideas, the civilian factories its import
budget sends abroad, at most 15 civilian factories a project, the state's infrastructure, and only states
outside the construction risk. Civilian factories are queued until the switch date, military ones after it and
at war; the switch date is the plan's own (the entry date of its first phase whose construction builds military
factories only) or, when the plan dates none, chosen by the arithmetic (the month that meets the most AI marks,
then gives the most military factory-days over the plan's horizon, the civilian count at its end breaking ties). Undated picks (politics_auto), decisions' variables and the
game's own random targets of a focus are not modelled; each forecast lists what it left out.
Stdlib only, Python 3.9+.
"""
import math
import re
from collections import defaultdict

from . import derive
from .script import bare, get1, parse_file

ATTACK_RISK_RATIO = 1.0   # an enemy side at least as strong as we are can still take border land while we attack
DEFEND_RISK_RATIO = 0.5   # a defender loses border land to a side half its size or more
CP_PER_CIV = 5.0          # construction points per civilian factory a day (calc.py, economy-build-orders.md 1.1)
INFRA_STEP = 0.2          # x (1 + 0.2 x infrastructure level)
PROJECT_CAP = 15          # civilian factories per project
FACTORY_COST = {"industrial_complex": 10800.0, "arms_factory": 7200.0}  # fallback when common/buildings lacks it
TRADE_PER_CIV = 8.0       # resource units one civilian factory imports (economy-build-orders.md EC14)
CG_VAR_CAP = 1.0          # a consumer-goods variable that grows monthly is clamped here unless its script says

SPEED_ALL = "production_speed_buildings_factor"
SPEED = {"industrial_complex": "production_speed_industrial_complex_factor",
         "arms_factory": "production_speed_arms_factory_factor"}
CG_EV = "consumer_goods_expected_value"
CG_F = "consumer_goods_factor"
SLOTS_F = "global_building_slots_factor"
LAW_GROUPS = ("economy", "trade_laws", "mobilization_laws")
ECON_KEYS = (SPEED_ALL, SPEED["industrial_complex"], SPEED["arms_factory"], CG_EV, CG_F, SLOTS_F)


def _day(s):
    return derive.day(s) if s else None


def _fmt(d):
    return derive.fmt(derive.date_of(int(d)))


def _numbers(block):
    out = {}
    for a, b in block or []:
        if isinstance(a, str) and not isinstance(b, list):
            try:
                out[a] = out.get(a, 0.0) + float(b)
            except (TypeError, ValueError):
                pass
    return out


# ------------------------------------------------------------------------------------------------ tables
def idea_table(game):
    """{idea: {"group": idea category, "mods": modifier numbers, "traits": [trait]}} from common/ideas."""

    def load():
        out = {}
        for f in game.files("common/ideas"):
            for k, v in parse_file(f):
                if k != "ideas" or not isinstance(v, list):
                    continue
                for group, body in v:
                    if not isinstance(body, list):
                        continue
                    for name, idea in body:
                        if not name or not isinstance(idea, list):
                            continue
                        m = get1(idea, "modifier")
                        tr = get1(idea, "traits")
                        shown = get1(idea, "name")
                        out[name] = {"group": group, "mods": _numbers(m) if isinstance(m, list) else {},
                                     "traits": bare(tr) if isinstance(tr, list) else [],
                                     "shown_as": shown if isinstance(shown, str) else None}
        return out

    return game.cached("economy:ideas", load)


def trait_table(game):
    """{trait: modifier numbers} of common/country_leader (advisor and leader traits)."""

    def load():
        out = {}
        for f in game.files("common/country_leader"):
            for k, v in parse_file(f):
                if k != "leader_traits" or not isinstance(v, list):
                    continue
                for name, body in v:
                    if name and isinstance(body, list):
                        mods = _numbers(body)
                        for skip in ("random", "sprite"):
                            mods.pop(skip, None)
                        m = get1(body, "modifier")
                        if isinstance(m, list):
                            mods.update(_numbers(m))
                        out[name] = mods
        return out

    return game.cached("economy:traits", load)


def advisor_table(world, tag):
    """{idea_token or character id: {"traits": [...], "character": id, "visible": text, "cost": n}} (cached on
    the world per tag)."""
    cache = world.__dict__.setdefault("_econ_advisors", {})
    if tag not in cache:
        cache[tag] = _advisor_table(world, tag)
    return cache[tag]


def _advisor_table(world, tag):
    out = {}
    for ch in world.characters(tag, "advisor"):
        for r in ch["roles"]:
            if r.get("role") != "advisor":
                continue
            row = {"traits": list(r.get("traits") or []), "character": ch["id"], "visible": r.get("visible"),
                   "available": r.get("available"), "cost": r.get("cost"), "slot": r.get("slot"),
                   "blocks": [b for b in (r.get("visible_block"), r.get("available_block")) if isinstance(b, list)]}
            out.setdefault(ch["id"], row)
            if r.get("idea_token"):
                out[r["idea_token"]] = row
    return out


def modifiers_of(game, world, tag, name):
    """Modifier numbers of an idea, law or advisor (its own modifier block plus its traits)."""
    ideas = idea_table(game)
    traits = trait_table(game)
    mods = defaultdict(float)
    row = ideas.get(name)
    trait_names = []
    if row:
        for k, v in row["mods"].items():
            mods[k] += v
        trait_names = row["traits"]
    else:
        adv = advisor_table(world, tag).get(name) if world is not None else None
        if adv:
            trait_names = adv["traits"]
    for t in trait_names:
        for k, v in (traits.get(t) or {}).items():
            mods[k] += v
    return dict(mods)


def industry(world, tag):
    st = world.states()
    own = world.owned_states(tag)
    return sum(st[s]["buildings"].get("industrial_complex", 0) + st[s]["buildings"].get("arms_factory", 0)
               for s in own)


# ------------------------------------------------------------------------------------------- construction risk
def enemy_wars(lint):
    """{enemy: {"from": day or None, "attack": bool, "how": [..]}} for every enemy of the plan."""
    start = _day(lint.start)
    at_start = set(lint.world.at_war_with(lint.tag)) if lint.tag else set()
    enemies = set(lint.enemies()) | at_start
    out = {}
    capit = set()
    for p in (lint.goal.get("predicates") or []):
        for a in _atoms(p):
            a = a.strip()
            if a.startswith("capitulated:"):
                capit.add(a.split(":", 1)[1])
    declare = defaultdict(list)
    for e in lint.list_of("diplomacy"):
        if e.get("action") in ("declare", "declare_war") and isinstance(e.get("target"), str):
            ds = []
            if e.get("when") is not None:
                from .validate import predicate_dates
                ds = [_day(d) for d in predicate_dates(e["when"], getattr(lint, "table", None) or {}) if _day(d)]
            declare[e["target"]].append(max(ds) if ds else None)
    checkpoint_war = defaultdict(list)
    for c in lint.list_of("checkpoints"):
        for a in _atoms(c.get("must")):
            a = a.strip()
            if a.startswith("at_war_with:") and c.get("by"):
                checkpoint_war[a.split(":", 1)[1]].append(_day(c["by"]))
    vp_owners = set()
    try:
        ctl = derive.province_controllers(lint.game, lint.ctx, lint.start)
        for e in lint.targets():
            o = ctl.get(e["province"])
            if o:
                vp_owners.add(o)
    except Exception:  # noqa: BLE001 - advisory: posture falls back to the predicates and declarations
        pass
    for e in sorted(enemies):
        how = []
        if e in at_start:
            frm = start
            how.append("at war at the start")
        else:
            ds = [d for d in declare.get(e, []) if d] + [d for d in checkpoint_war.get(e, []) if d]
            frm = min(ds) if ds else None
            how.append(f"war from {_fmt(frm)}" if frm else "war date not in the plan (within the horizon)")
        attack = e in capit or e in declare or e in vp_owners
        how.append("the plan attacks" if attack else "the plan defends")
        out[e] = {"from": frm, "attack": attack, "how": how}
    return out


def enemy_side(world, tag, enemy, enemies):
    """The enemy, its start faction partners the plan also fights, and its subjects and overlord."""
    rel = world.relations()
    side = {enemy}
    for f in rel["factions"]:
        if enemy in f:
            side |= {t for t in f if t in enemies}
    side |= set(rel["subjects"].get(enemy, ()))
    for over, subs in rel["subjects"].items():
        if enemy in subs:
            side.add(over)
    side.discard(tag)
    return sorted(side)


def construction_risk(lint, k=None):
    """{"enemies": {tag: {side, industry, ratio, threshold, attack, from, risk}}, "own_industry",
    "states": {state: {"enemy", "steps", "risk", "why"}}} for every own state within K steps of an enemy."""
    k = lint.k if k is None else k
    w = lint.world
    wars = enemy_wars(lint)
    own = max(1, industry(w, lint.tag))
    mine = set(w.owned_states(lint.tag))
    tl_gains = getattr(lint, "_timeline", None)
    if tl_gains is not None:
        mine |= {s for _, s in tl_gains.gains}
    for c in lint.list_of("checkpoints"):
        for a in _atoms(c.get("must")):
            a = a.strip()
            if a.startswith("owner:") and a.endswith("=" + lint.tag):
                try:
                    mine.add(int(a[6:].split("=")[0]))
                except ValueError:
                    pass
    table = {}
    states = {}
    for e, info in wars.items():
        side = enemy_side(w, lint.tag, e, set(wars))
        ind = sum(industry(w, t) for t in side)
        ratio = round(ind / own, 2)
        thr = ATTACK_RISK_RATIO if info["attack"] else DEFEND_RISK_RATIO
        risk = ratio >= thr
        table[e] = {"side": side, "industry": ind, "ratio": ratio, "threshold": thr, "attack": info["attack"],
                    "from": _fmt(info["from"]) if info["from"] else None, "risk": risk, "how": info["how"]}
        near = derive.front_distance(lint.game, lint.ctx, lint.start, [e], k)
        why = (f"{e} side {'+'.join(side)} {ind} factories against our {own} (ratio {ratio}, "
               f"{'attacker' if info['attack'] else 'defender'} threshold {thr}); {'; '.join(info['how'])}")
        for sid, d in near.items():
            if sid not in mine:
                continue
            cur = states.get(sid)
            if cur is None or (risk and not cur["risk"]) or (risk == cur["risk"] and d < cur["steps"]):
                states[sid] = {"enemy": e, "steps": d, "risk": risk, "why": why}
    return {"enemies": table, "own_industry": own, "states": states, "k": k}


def risk_states(lint):
    if getattr(lint, "_construction_risk", None) is None:
        lint._construction_risk = construction_risk(lint) if lint.tag and lint.world is not None else \
            {"enemies": {}, "own_industry": 0, "states": {}, "k": lint.k}
    return lint._construction_risk


# ------------------------------------------------------------------------------------------------ events
def _reward_events(block, tag, static=None):
    """[(kind, payload)] of an effect block: ideas added and removed, factories, slots, annexations. An if /
    else_if / else chain takes the first branch whose limit `static` (limit -> True, False or None) does not
    rule out and that gives something (a guard or debug branch that gives nothing is passed over); without
    `static` the first branch that gives something. A random state scope builds in the best state."""
    out = []

    def chain_pick(branches, scope, mult):
        for limit, body in branches:
            if static is not None and limit is not None:
                try:
                    if static(limit) is False:
                        continue
                except Exception:  # noqa: BLE001 - unknown: the branch may run
                    pass
            n = len(out)
            walk(body, scope, mult)
            if len(out) > n:
                return

    def walk(items, scope, mult):
        branches = None
        for k, v in list(items) + [(None, None)]:
            kl = (k or "").lower()
            if kl in ("else_if", "else") and branches is not None and isinstance(v, list):
                branches.append((get1(v, "limit") if kl == "else_if" else None, v))
                continue
            if branches is not None:
                chain_pick(branches, scope, mult)
                branches = None
            if k is None:
                break
            if kl == "if" and isinstance(v, list):
                branches = [(get1(v, "limit"), v)]
                continue
            if kl in ("limit", "else", "else_if", "ai_will_do", "modifier", "tooltip"):
                continue
            if kl == "add_ideas":
                for x in (bare(v) if isinstance(v, list) else [v]):
                    out.append(("idea+", x))
            elif kl == "remove_ideas":
                for x in (bare(v) if isinstance(v, list) else [v]):
                    out.append(("idea-", x))
            elif kl == "swap_ideas" and isinstance(v, list):
                out.append(("idea-", get1(v, "remove_idea")))
                out.append(("idea+", get1(v, "add_idea")))
            elif kl == "add_timed_idea" and isinstance(v, list):
                try:
                    days = int(float(get1(v, "days") or 0))
                except (TypeError, ValueError):
                    days = 0
                out.append(("idea_timed", (get1(v, "idea"), days)))
            elif kl == "add_building_construction" and isinstance(v, list):
                b = get1(v, "type")
                try:
                    lvl = int(float(get1(v, "level") or 1))
                except (TypeError, ValueError):
                    lvl = 1
                out.append(("build", (b, lvl * mult, scope)))
            elif kl == "add_extra_state_shared_building_slots":
                try:
                    out.append(("slots", (int(float(v)) * mult, scope)))
                except (TypeError, ValueError):
                    pass
            elif kl in ("add_to_variable", "subtract_from_variable") and isinstance(v, list):
                sign = -1.0 if kl == "subtract_from_variable" else 1.0
                for a, b in v:
                    if a and a not in ("tooltip", "var", "value"):
                        try:
                            out.append(("var", (a, sign * float(b))))
                        except (TypeError, ValueError):
                            pass
            elif kl == "annex_country":
                t = get1(v, "target") if isinstance(v, list) else v
                if isinstance(t, str):
                    out.append(("annex", t))
            elif isinstance(v, list):
                if k and k.isdigit():
                    walk(v, int(k), mult)
                elif kl in ("hidden_effect", "root", "capital_scope") or kl.startswith("effect_"):
                    walk(v, scope if kl != "capital_scope" else "capital", mult)
                elif kl.startswith(("random_owned", "random_core", "random_state", "every_owned", "every_core",
                                    "every_state", "random_controlled", "every_controlled")):
                    n = get1(v, "random_select_amount")
                    try:
                        n = int(float(n)) if n is not None else 1
                    except (TypeError, ValueError):
                        n = 1
                    walk(v, "random", mult * n)
                elif k == tag:
                    walk(v, scope, mult)

    if isinstance(block, list):
        walk(block, None, 1)
    return out


def _rewards(lint, kind_wanted="focus"):
    """{id: [(kind, payload)]} of the tag's focus rewards (or, with kind_wanted "decision", the decisions'
    complete effects)."""
    from . import scenario
    cache = getattr(lint, "_econ_rewards_by", None)
    if cache is None:
        cache = {}
        try:
            for kind, fid, block, _ in scenario.tag_sources(lint.world, lint.tag):
                if kind in ("focus", "decision") and block and fid not in cache.get(kind, {}):  # complete first
                    cache.setdefault(kind, {})[fid] = _reward_events(
                        block, lint.tag, static=lambda lim: lint.world.static(lim, lint.tag))
        except Exception:  # noqa: BLE001 - advisory: no rewards then
            pass
        lint._econ_rewards_by = cache
    return cache.get(kind_wanted, {})


def plan_events(lint, tl, extra=()):
    """[(day, kind, payload, source)] the plan dates, sorted. `extra` adds lever events (the levers check)."""
    start = tl.start
    ev = []
    ideas = idea_table(lint.game)
    for p in lint.list_of("politics"):
        name = p.get("idea")
        if not isinstance(name, str):
            continue
        ds = tl.lint_dates(p["when"]) if p.get("when") is not None else []
        d = max(ds) if ds else start
        grp = (ideas.get(name) or {}).get("group")
        ev.append((d, "law" if grp in LAW_GROUPS else "idea+", (grp, name) if grp in LAW_GROUPS else name,
                   f"politics {p.get('id') or name}"))
    for r in lint.list_of("decision_rules"):
        dec, when = r.get("decision"), r.get("when")
        if not isinstance(dec, str) or when in (None, "never", False) or r.get("max") == 0:
            continue
        ds = tl.lint_dates(when)
        if not ds:
            continue
        for kind, payload in _rewards(lint, "decision").get(dec, []):
            if kind == "var":  # only the variables a decision moves (consumer goods): ideas and factories vary
                ev.append((max(ds), kind, payload, f"decision {dec}"))
    for end, fid in getattr(lint, "_focus_done", []) or []:
        for kind, payload in _rewards(lint).get(fid, []):
            if kind == "idea+" and (ideas.get(payload) or {}).get("group") in LAW_GROUPS:
                ev.append((end, "law", (ideas[payload]["group"], payload), f"focus {fid}"))
            else:
                ev.append((end, kind, payload, f"focus {fid}"))
    try:
        from . import research as researchmod
        for r in researchmod.timeline(lint.world, lint.plan)["rows"]:
            d = _day(r["end"])
            if d is not None:
                ev.append((d, "tech", r["id"], f"research {r['id']}"))
    except Exception:  # noqa: BLE001 - advisory: no techs then
        pass
    for d, s in tl.gains:
        ev.append((d, "gain", s, "checkpoint"))
    ev += list(extra)
    return sorted(ev, key=lambda x: (x[0], str(x[1])))


# ------------------------------------------------------------------------------------------------ simulation
class _Econ:
    def __init__(self, lint, tl):
        self.lint, self.tl = lint, tl
        w = lint.world
        self.w = w
        st = w.states()
        self.st = st
        self.risk = {s for s, r in risk_states(lint)["states"].items() if r["risk"]}
        self.own = set(w.owned_states(lint.tag))
        self.civ = sum(st[s]["buildings"].get("industrial_complex", 0) for s in self.own)
        self.mil = sum(st[s]["buildings"].get("arms_factory", 0) for s in self.own)
        self.slots = {}
        for s in self.own:
            self._add_state(s)
        self.ideas = defaultdict(int)
        self.law = {}
        ideas = idea_table(lint.game)
        dms = history_dynamic_modifiers(lint)
        for f in w.country(lint.tag).get("ideas") or []:
            name = f.get("v")
            if not name:
                continue
            grp = (ideas.get(name) or {}).get("group")
            if grp in LAW_GROUPS:
                self.law[grp] = name
            elif (ideas.get(name) or {}).get("shown_as") in dms:
                continue  # a start-screen proxy of a dynamic modifier the country has (the forecast counts that)
            else:
                self.ideas[name] += 1
        self.techs = set()
        from . import research as researchmod
        try:
            self.techs = set(researchmod.start_techs(w, lint.tag))
        except Exception:  # noqa: BLE001
            pass
        self.mod_cache = {}
        self.dyn = {"factor": 0.0, "expected": 0.0}
        self.cg_vars = {}      # variable -> "factor" | "expected" (the dynamic modifiers' consumer-goods terms)
        self.drift = []        # [(variable, monthly amount, cap)] from the country's monthly on_action
        self.resource_trade = 0
        self.trade = 0
        ai = lint.plan.get("auto_import")
        if isinstance(ai, dict) and ai.get("enabled", True) is not False:
            try:
                self.trade = int(ai.get("budget") or 0)
            except (TypeError, ValueError):
                self.trade = 0
        self.costs = dict(FACTORY_COST)
        try:
            bt = w.building_types()
            for b in self.costs:
                c = (bt.get(b) or {}).get("cost")
                if c:
                    self.costs[b] = float(c)
        except Exception:  # noqa: BLE001
            pass
        self.timed = []

    def _add_state(self, s):
        row = self.st.get(s)
        if row is None or s in self.slots:
            return
        self.slots[s] = {"base": int(row.get("slots") or 0), "used": int(row.get("used_slots") or 0),
                         "extra": 0, "infra": int(row.get("infrastructure") or 0)}

    def mods(self, name):
        if name not in self.mod_cache:
            self.mod_cache[name] = modifiers_of(self.lint.game, self.w, self.lint.tag, name)
        return self.mod_cache[name]

    def total(self, key):
        t = 0.0
        for name, n in self.ideas.items():
            if n > 0:
                t += self.mods(name).get(key, 0.0)
        for name in self.law.values():
            t += self.mods(name).get(key, 0.0)
        tm = derive.tech_modifiers(self.lint.game)
        for tech in self.techs:
            t += (tm.get(tech) or {}).get(key, 0.0)
        return t

    def snapshot(self):
        cg = max(0.0, (self.total(CG_EV) + self.dyn["expected"]) * (1 + self.total(CG_F) + self.dyn["factor"]))
        self.cg = cg
        self.speed_all = self.total(SPEED_ALL)
        self.speed = {b: self.total(k) for b, k in SPEED.items()}
        self.slot_factor = self.total(SLOTS_F)

    def free_slots(self, s):
        r = self.slots[s]
        return int(math.floor(r["base"] * (1 + self.slot_factor) + 1e-9)) + r["extra"] - r["used"]

    def best_state(self, scope=None):
        if isinstance(scope, int) and scope in self.slots:
            return scope
        if scope == "capital":
            cap = (self.w.country(self.lint.tag).get("capital") or {}).get("v")
            try:
                cap = int(cap)
            except (TypeError, ValueError):
                cap = None
            if cap in self.slots:
                return cap
        cands = [s for s in self.slots if s not in self.risk and self.free_slots(s) > 0]
        if not cands:
            return None
        return max(cands, key=lambda s: (self.slots[s]["infra"], self.free_slots(s), -s))

    def apply(self, kind, payload):
        if kind == "law":
            grp, name = payload
            self.law[grp] = name
        elif kind == "idea+" and payload:
            self.ideas[payload] += 1
        elif kind == "idea-" and payload:
            self.ideas[payload] = max(0, self.ideas[payload] - 1)
        elif kind == "idea_timed":
            name, days = payload
            if name:
                self.ideas[name] += 1
                self.timed.append((self.today + days, name))
        elif kind == "tech":
            self.techs.add(payload)
        elif kind == "slots":
            n, scope = payload
            s = self.best_state(scope) if scope is not None else None
            if s is None and scope == "random":
                cands = [x for x in self.slots if x not in self.risk]
                s = max(cands, key=lambda x: (self.slots[x]["infra"], -x)) if cands else None
            if s is not None:
                self.slots[s]["extra"] += n
        elif kind == "build":
            b, n, scope = payload
            if b in ("industrial_complex", "arms_factory"):
                s = self.best_state(scope)
                if s is not None:
                    self.slots[s]["used"] += n
                if b == "industrial_complex":
                    self.civ += n
                else:
                    self.mil += n
            elif b == "infrastructure" and isinstance(scope, int) and scope in self.slots:
                self.slots[scope]["infra"] = min(5, max(self.slots[scope]["infra"], n if n >= 5 else
                                                        self.slots[scope]["infra"] + n))
        elif kind == "var":
            name, delta = payload
            key = self.cg_vars.get(name)
            if key:
                self.dyn[key] = min(self.cap_of(name), self.dyn[key] + delta)
        elif kind == "annex":
            for s in self.w.owned_states(payload):
                self._gain(s)
        elif kind == "gain":
            self._gain(payload)

    def cap_of(self, name):
        caps = [c for v, _, c in self.drift if v == name]
        return min(caps) if caps else float("inf")

    def month(self):
        """The monthly script drift of the consumer-goods variables (on_monthly_<TAG>)."""
        changed = False
        for name, amount, cap in self.drift:
            key = self.cg_vars.get(name)
            if key and self.dyn[key] < cap:
                self.dyn[key] = min(cap, self.dyn[key] + amount)
                changed = True
        return changed

    def import_civs(self, lines):
        """Civilian factories the market takes on day `today` for the resources the lines lack: per resource,
        the lines' need (scaled to the military factories that exist) less what the owned states give after
        the trade law's export share, in blocks of TRADE_PER_CIV."""
        eqres = self.w.equipment_resources()
        need = defaultdict(float)
        pool = getattr(self.lint, "factory_pool", lambda eq: "military")
        mil_lines = {eq: n for eq, n in lines.items() if pool(eq) == "military"}
        total = sum(mil_lines.values()) or 0
        scale = min(1.0, self.mil / total) if total else 0.0
        for eq, n in lines.items():
            k = scale if eq in mil_lines else 1.0  # dockyard lines: the dockyards are not modelled, taken as full
            for r, a in ((eqres.get(eq) or {}).get("resources") or {}).items():
                need[r] += a * n * k
        if not need:
            return 0
        got = defaultdict(float)
        for st in self.own:
            for r, a in ((self.st.get(st) or {}).get("resources") or {}).items():
                got[r] += a
        trade = self.law.get("trade_laws")
        share = self.mods(trade).get("min_export", 0.0) if trade else 0.0
        civs = 0
        for r, n in need.items():
            short = n - got.get(r, 0.0) * (1 - share)
            if short > 0:
                civs += int(math.ceil(short / TRADE_PER_CIV - 1e-9))
        return civs

    def _gain(self, s):
        if s in self.own or s not in self.st:
            return
        self.own.add(s)
        b = self.st[s]["buildings"]
        self.civ += b.get("industrial_complex", 0)
        self.mil += b.get("arms_factory", 0)
        self._add_state(s)


def history_dynamic_modifiers(lint):
    """The dynamic modifiers the country's history adds. An idea displayed under one of these names
    (its `name =` field) is the start-screen proxy of that modifier, not a second modifier."""
    try:
        _, effects = derive.history_effects_of(lint.game, lint.tag, lint.ctx, lint.start, lint.meta.get("history_file"))
    except Exception:  # noqa: BLE001 - advisory
        return set()
    return {derive.get1(v, "modifier") for k, v in effects
            if k == "add_dynamic_modifier" and isinstance(v, list) and derive.get1(v, "modifier")}


def cg_variables(lint):
    """({variable: "factor" | "expected"}, [(variable, monthly amount, cap)]): the variables behind the
    consumer-goods terms of the country's dynamic modifiers, and the amounts its monthly on_action
    (on_monthly_<TAG>) adds to them, resolved with the variables its history sets; the cap is the bound of the
    `check_variable = { V < c }` that guards the addition (CG_VAR_CAP without one)."""
    cached = getattr(lint, "_econ_cg_vars", None)
    if cached is not None:
        return cached
    out, drift = {}, []
    try:
        from .lint_economy import _dynamic_consumer_goods
        if getattr(lint, "_econ_dyn", None) is None:
            lint._econ_dyn = _dynamic_consumer_goods(lint)
        for r in lint._econ_dyn[1]:
            m = re.match(r"variable (\S+)", r.get("how") or "")
            if m:
                out[m.group(1)] = "factor" if r["key"] == "consumer_goods_factor" else "expected"
        if out:
            var = {}
            _, effects = derive.history_effects_of(lint.game, lint.tag, lint.ctx, lint.start,
                                                   lint.meta.get("history_file"))
            for k, v in effects:
                if k == "set_variable" and isinstance(v, list):
                    for x, y in v:
                        try:
                            var[x] = float(y)
                        except (TypeError, ValueError):
                            if y in var:
                                var[x] = var[y]

            def value(raw):
                try:
                    return float(raw)
                except (TypeError, ValueError):
                    return var.get(raw)

            def walk(items, cap):
                for k, v in items or []:
                    if not isinstance(v, list):
                        continue
                    if k == "if":
                        c = cap
                        lim = get1(v, "limit")
                        for ck, cv in (lim if isinstance(lim, list) else []):
                            if ck == "check_variable" and isinstance(cv, list):
                                for x, y in cv:
                                    if x in out:
                                        c = value(y) if value(y) is not None else c
                        walk(v, c)
                    elif k == "add_to_variable":
                        for x, y in v:
                            if x in out and value(y):
                                drift.append((x, value(y), cap if cap is not None else CG_VAR_CAP))
                    elif k not in ("limit", "subtract_from_variable", "set_variable"):
                        walk(v, cap)

            for p in lint.game.files("common/on_actions"):
                for k, v in parse_file(p):
                    if k == "on_actions" and isinstance(v, list):
                        for name, body in v:
                            if name == f"on_monthly_{lint.tag}" and isinstance(body, list):
                                walk(get1(body, "effect") or [], None)
    except Exception:  # noqa: BLE001 - advisory: the start value is kept
        pass
    lint._econ_cg_vars = (out, drift)
    return lint._econ_cg_vars


def observed_economy(lint):
    """(day, civilian, military) of force_design.observed.economy: the factories a save of this plan holds on
    that day, which replace the forecast's own count from then on; None when the plan sets none."""
    fd = lint.plan.get("force_design") if isinstance(lint.plan.get("force_design"), dict) else {}
    ob = fd.get("observed") if isinstance(fd.get("observed"), dict) else {}
    eco = ob.get("economy") if isinstance(ob.get("economy"), dict) else {}
    day = _day(ob.get("date")) if isinstance(ob.get("date"), str) else None
    civ, mil = eco.get("civilian"), eco.get("military")
    if day is None or not isinstance(civ, int) or not isinstance(mil, int):
        return None
    return day, civ, mil


def simulate(lint, tl, switch_day, until_day, events=None, dyn=None, sample=()):
    """Run the construction day by day: {"rows": {day: {...}}, "end": {...}, "slot_bound_days": n}. A plan
    continued from a save (force_design.observed.economy) takes the save's factories on its date: the projects
    under way then are not read and start anew."""
    e = _Econ(lint, tl)
    seen = observed_economy(lint)
    if dyn:
        e.dyn = dict(dyn)
    e.cg_vars, e.drift = cg_variables(lint)
    lines_on = getattr(lint, "active_lines", None)
    events = list(events if events is not None else plan_events(lint, tl))
    start = tl.start
    war = tl.war_day
    projects = []
    mil_days = 0
    rows = {}
    bound = 0
    first_bound = None
    sample = set(sample)
    i = 0
    e.today = start
    e.snapshot()
    for today in range(start, until_day + 1):
        e.today = today
        changed = False
        while i < len(events) and events[i][0] <= today:
            _, kind, payload, _src = events[i]
            e.apply(kind, payload)
            i += 1
            changed = True
        if e.timed:
            for end, name in [x for x in e.timed if x[0] <= today]:
                e.ideas[name] = max(0, e.ideas[name] - 1)
                e.timed.remove((end, name))
                changed = True
        if derive.date_of(today)[2] == 1 and today > start:
            changed = e.month() or changed
            if lines_on is not None:
                try:
                    e.resource_trade = e.import_civs(lines_on(today))
                except Exception:  # noqa: BLE001 - no resource data: no import estimate
                    e.resource_trade = 0
        elif today == start and lines_on is not None:
            try:
                e.resource_trade = e.import_civs(lines_on(today))
            except Exception:  # noqa: BLE001
                e.resource_trade = 0
        if seen is not None and today == max(seen[0], start):
            e.civ, e.mil = seen[1], seen[2]
            for pr in projects:
                e.slots[pr["s"]]["used"] -= 1
            projects.clear()
        if changed:
            e.snapshot()
        total = e.civ + e.mil
        trade = max(e.trade, e.resource_trade)
        free = max(0, e.civ - int(math.ceil(e.cg * total - 1e-9)) - trade)
        want = int(math.ceil(free / PROJECT_CAP)) if free else 0
        at_war = war is not None and today >= war
        kind = "industrial_complex" if (today < switch_day and not at_war) else "arms_factory"
        while len(projects) < want:
            s = e.best_state()
            if s is None:
                break
            e.slots[s]["used"] += 1
            projects.append({"b": kind, "s": s, "p": 0.0})
        if len(projects) < want:
            bound += 1
            if first_bound is None:
                first_bound = today
        left = free
        done = []
        for pr in projects:
            n = min(PROJECT_CAP, left)
            if n <= 0:
                break
            left -= n
            infra = e.slots[pr["s"]]["infra"]
            pr["p"] += n * CP_PER_CIV * (1 + INFRA_STEP * infra) * max(0.0, 1 + e.speed_all + e.speed[pr["b"]])
            if pr["p"] >= e.costs[pr["b"]]:
                done.append(pr)
        mil_days += e.mil
        for pr in done:
            projects.remove(pr)
            if pr["b"] == "industrial_complex":
                e.civ += 1
            else:
                e.mil += 1
        if today in sample:
            rows[today] = {"date": _fmt(today), "civ": e.civ, "mil": e.mil, "free_civs": free,
                           "consumer_goods": round(e.cg, 3),
                           "consumer_goods_civs": int(math.ceil(e.cg * total - 1e-9)), "trade_civs": trade,
                           "speed_civ": round(1 + e.speed_all + e.speed["industrial_complex"], 3),
                           "speed_mil": round(1 + e.speed_all + e.speed["arms_factory"], 3),
                           "slot_factor": round(e.slot_factor, 2),
                           "free_slots_safe": sum(max(0, e.free_slots(s)) for s in e.slots if s not in e.risk),
                           "building": "civilian" if kind == "industrial_complex" else "military"}
    return {"rows": rows, "end": {"date": _fmt(until_day), "civ": e.civ, "mil": e.mil}, "mil_days": mil_days,
            "slot_bound_days": bound, "first_slot_bound": _fmt(first_bound) if first_bound else None,
            "laws": dict(e.law), "risk_states": sorted(e.risk)}


def plan_end(lint, start=None):
    """The last day the plan covers: its horizons, goal deadline or stop date (at most ten years)."""
    start = start if start is not None else _day(lint.start)
    ends = [d for d, _ in lint.horizon()]
    for h in lint.list_of("horizons"):
        d = _day(h.get("until"))
        if d:
            ends.append(d)
    end = max(ends) if ends else start + 3 * 365
    return min(end, start + 10 * 365)


def _dyn(lint):
    try:
        from .lint_economy import _dynamic_consumer_goods
        if getattr(lint, "_econ_dyn", None) is None:
            lint._econ_dyn = _dynamic_consumer_goods(lint)
        return lint._econ_dyn[0]
    except Exception:  # noqa: BLE001
        return {"factor": 0.0, "expected": 0.0}


def plan_switch(lint):
    """The day the plan itself switches from civilian to military factories: the entry date of the first phase
    entered by a date whose construction override builds arms factories and no civilian factory while the base
    construction list builds civilian factories (the executor runs the active list, so that phase's date IS the
    switch). None when the plan dates none (the arithmetic then picks one)."""
    base = [e.get("building") for e in lint.plan.get("construction") or [] if isinstance(e, dict)]
    if "industrial_complex" not in base:
        return None
    best = None
    for ph in lint.plan.get("phases") or []:
        if not isinstance(ph, dict):
            continue
        cons = (ph.get("overrides") or {}).get("construction")
        if not isinstance(cons, list):
            continue
        kinds = {e.get("building") for e in cons if isinstance(e, dict)}
        if "arms_factory" not in kinds or "industrial_complex" in kinds:
            continue
        m = re.fullmatch(r"\s*date\s*>=\s*(\d{3,4}\.\d{1,2}\.\d{1,2})\s*", str(ph.get("enter") or ""))
        if not m:
            continue
        d = _day(m.group(1))
        if d is not None and (best is None or d < best):
            best = d
    return best


def forecast(lint, tl, until=None, extra=(), marks=(), floors=None):
    """The plan's economy by arithmetic with the best switch date: {"switch", "war", "rows": [...], ...}.
    A plan that dates its own switch (plan_switch) is forecast on that date. Otherwise
    floors: {day: (civilian, military)} marks a switch should keep (the AI baseline): the switch that meets
    the most of them up to the first war day wins (the marks lint_economy holds as errors), then the most of
    them in all, then the most military factory-days, then the most civilian factories at the end."""
    start = tl.start
    until = until or plan_end(lint, start)
    war = tl.war_day
    last = min(war, until) if war and war > start else until
    events = plan_events(lint, tl, extra)
    dyn = _dyn(lint)
    sample = set(range(start, until + 1, 91)) | {d for d in marks if start <= d <= until}
    if war and start < war <= until:
        sample.add(war)
    best = None
    cands = list(range(start, last + 1, 30)) + [last]
    floors = {d: v for d, v in (floors or {}).items() if start <= d <= until}
    dated = plan_switch(lint)
    if dated is not None and start <= dated <= until:
        cands = [dated]
    for sw in cands:
        r = simulate(lint, tl, sw, until, events, dyn, sample=floors)
        hit = [d for d, (c, m) in floors.items() if r["rows"][d]["civ"] >= c and r["rows"][d]["mil"] >= m]
        pre = sum(1 for d in hit if war is None or d <= war)
        key = (pre, len(hit), r["mil_days"], r["end"]["civ"])
        if best is None or key > best[0]:
            best = (key, sw)
    switch = best[1]
    r = simulate(lint, tl, switch, until, events, dyn, sample=sample)
    left_out = ["political power: every dated pick is taken on its date",
                "undated decisions that move consumer-goods variables (the monthly on_action, the focus rewards "
                "and the dated decision_rules are counted)", "civilian factories the market pays for exports",
                "the module resources of designed equipment (chassis, airframes, hulls) in the import estimate"]
    auto = lint.plan.get("politics_auto")
    if isinstance(auto, dict) and auto.get("order"):
        left_out.append("politics_auto picks (undated)")
    return {"switch": _fmt(switch), "switch_from": "plan" if cands == [dated] else "arithmetic",
            "war": _fmt(war) if war else None, "until": _fmt(until),
            "rows": [r["rows"][d] for d in sorted(r["rows"])], "slot_bound_days": r["slot_bound_days"],
            "first_slot_bound": r["first_slot_bound"], "risk_states": r["risk_states"],
            "trade_civs": lint.plan.get("auto_import", {}).get("budget") if isinstance(lint.plan.get("auto_import"),
                                                                                       dict) else 0,
            "start_consumer_goods_dynamic": dyn, "left_out": left_out,
            "how": "day by day from the start: free civilian factories = civilian - ceil(consumer goods x all "
                   "factories) - civilian factories on trade (the import budget, or the resources the lines lack "
                   "after the trade law's export share in blocks of 8, whichever is larger); consumer goods from "
                   "the laws, ideas and the dynamic modifiers' variables, with their monthly script drift and the "
                   "focus rewards that move them; projects of at most 15; points = factories x 5 x (1 + 0.2 x "
                   "infrastructure) x (1 + construction speed); speed, consumer goods and slot factors from the "
                   "start laws and ideas, the dated politics picks, the focus path's ideas on completion and the "
                   "research path's techs on completion; focus factories and slots instantly; states owned by "
                   "the checkpoints and annexations with their factories; only states outside the construction "
                   "risk; civilian factories until the switch date, military after it and at war"}


def _gain_text(mods):
    parts = []
    for k, label in ((SPEED_ALL, "construction"), (SPEED["industrial_complex"], "civilian factory construction"),
                     (SPEED["arms_factory"], "military factory construction"), (SLOTS_F, "building slots"),
                     (CG_EV, "consumer goods"), (CG_F, "consumer goods factor")):
        if mods.get(k):
            parts.append(f"{mods[k]:+.0%} {label}")
    return ", ".join(parts)


def _law_open_day(lint, tl, law):
    """The first day the plan's own timeline does not show the law's gate closed: the start, else the first war
    day; None when its gate stays closed then too (or needs a focus outside the country's tree)."""
    return _gate_open_day(lint, tl, (lint.world.ideas().get(law) or {}).get("available"))


def _gate_open_day(lint, tl, raw):
    """_law_open_day for any trigger block (an idea's `available`, an advisor's `visible` and `available`)."""
    if not isinstance(raw, list) or not raw:
        return tl.start
    facts_at = getattr(lint, "_gate_facts", None)
    tree = set()
    try:
        from . import focus as focusmod
        tree = set(focusmod.tree(lint.world, lint.tag)["focuses"])
    except Exception:  # noqa: BLE001
        pass
    text = derive.render(raw)
    import re as _re
    for f in _re.findall(r"has_completed_focus\s*=\s*(\w+)", text):
        if f not in tree and "OR" not in text:
            return None
    if facts_at is None:
        return tl.start
    from . import plan_gates
    needs_war = bool(_re.search(r"(?<![\w])has_war\s*=\s*yes", text))
    days = ([] if needs_war else [tl.start]) + ([tl.war_day] if tl.war_day else [])
    for d in days:
        done = {f for end, f in getattr(lint, "_focus_done", []) if end <= d}
        if plan_gates.evaluate(raw, facts_at(d, done)) is not False:
            return d
    return None


def _block_open_day(lint, tl, blk):
    """The first of the start and the first war day on which the plan's timeline (conditions.evaluate: the
    government, focuses done, wars, ideas) does not show the trigger block closed; None when it is closed on
    both. A block the timeline cannot read counts as open."""
    from . import conditions
    days = [tl.start] + ([tl.war_day] if tl.war_day and tl.war_day > tl.start else [])
    for d in days:
        done = {f for end, f in getattr(lint, "_focus_done", []) if end <= d}
        try:
            ok, _ = conditions.evaluate(blk, conditions.Env(tl, d, done))
        except Exception:  # noqa: BLE001
            ok = None
        if ok is not False:
            return d
    return None


def levers(lint, tl, base, limit=12):
    """Construction levers the game offers the country that the plan leaves out or takes late: laws, advisors,
    techs and focuses, each with what it gives and, where a date can be set, the forecast's factories on the
    first war day (else the horizon end) when it is taken at the start: [{"kind", "id", "gives", "gate",
    "gain"}] best first."""
    w = lint.world
    tag = lint.tag
    start = tl.start
    switch = _day(base["switch"])
    end = _day(base["until"])
    mark = tl.war_day if tl.war_day and start < tl.war_day <= end else end
    events = plan_events(lint, tl)
    dyn = _dyn(lint)
    ref = simulate(lint, tl, switch, mark, events, dyn)
    ref_n = ref["end"]["civ"] + ref["end"]["mil"]
    have = {payload if isinstance(payload, str) else payload[1] for _, kind, payload, _ in events
            if kind in ("idea+", "law")}
    auto = lint.plan.get("politics_auto") if isinstance(lint.plan.get("politics_auto"), dict) else {}
    for e in auto.get("order") or []:
        if isinstance(e, dict):
            have.update(x for x in e.get("ideas") or [] if isinstance(x, str))
    out = []

    def gain(extra):
        r = simulate(lint, tl, switch, mark, sorted(events + extra, key=lambda x: (x[0], str(x[1]))), dyn)
        return r["end"]["civ"] + r["end"]["mil"] - ref_n

    useful = (SPEED_ALL, SPEED["industrial_complex"], SPEED["arms_factory"], SLOTS_F)
    # laws
    try:
        laws = w.laws(tag)
    except Exception:  # noqa: BLE001
        laws = {}
    for grp in ("economy", "trade_laws"):
        row = laws.get(grp) or {}
        for o in row.get("options") or []:
            lid = o.get("id")
            if not lid or lid in have or lid == row.get("current"):
                continue
            mods = modifiers_of(lint.game, w, tag, lid)
            if not any(mods.get(k, 0) > 0 for k in useful) and not mods.get(CG_EV, 1) < \
                    modifiers_of(lint.game, w, tag, row.get("current") or "").get(CG_EV, 0):
                continue
            when = _law_open_day(lint, tl, lid)
            if when is None:
                continue
            g = gain([(when, "law", (grp, lid), "lever")])
            if g > 0:
                out.append({"kind": "law", "id": lid, "gives": _gain_text(mods),
                            "gate": (o.get("available") or "")[:160], "gain": g})
    # laws the plan takes later than the plan's own timeline opens them
    for d, kind, payload, src in events:
        if kind != "law" or not src.startswith("politics"):
            continue
        open_day = _law_open_day(lint, tl, payload[1])
        if open_day is not None and d - open_day > 90:
            g = gain([(open_day, "law", payload, "lever")])
            if g > 0:
                out.append({"kind": "law", "id": payload[1], "gives": "take it earlier: "
                            + _gain_text(modifiers_of(lint.game, w, tag, payload[1])),
                            "gate": f"open {_fmt(open_day)} under the plan's timeline, planned {_fmt(d)}", "gain": g})
    # advisors
    for aid, a in advisor_table(w, tag).items():
        if aid != a["character"] or aid in have or any(k in have for k, v in advisor_table(w, tag).items()
                                                       if v is a):
            continue
        mods = modifiers_of(lint.game, w, tag, aid)
        if not any(mods.get(k, 0) > 0 for k in useful):
            continue
        # hireable only while both his visible and his available blocks hold under the plan's timeline (a
        # government or flag the plan never reaches keeps him out); the static start value decides a block the
        # timeline cannot read
        opens = [_block_open_day(lint, tl, blk) for blk in a.get("blocks") or []]
        if any(d is None for d in opens):
            continue
        when = max([start] + opens)
        g = gain([(when, "idea+", aid, "lever")])
        out.append({"kind": "advisor", "id": aid, "gives": _gain_text(mods),
                    "gate": (a.get("visible") or a.get("available") or "")[:160], "gain": g})
    # techs
    try:
        from . import research as researchmod
        ts = researchmod.techs(w)
        planned = {r["id"]: _day(r["end"]) for r in researchmod.timeline(w, lint.plan)["rows"]}
        done = set(researchmod.start_techs(w, tag))
    except Exception:  # noqa: BLE001
        ts, planned, done = {}, {}, set()
    tm = derive.tech_modifiers(lint.game)
    last_year = derive.as_tuple(_fmt(mark))[0]
    for tid, row in ts.items():
        mods = tm.get(tid) or {}
        if tid in done or not any(mods.get(k, 0) > 0 for k in (SPEED_ALL, SLOTS_F)):
            continue
        year = row.get("year") or 0
        if year > last_year:
            continue
        due = derive.day(f"{max(year, derive.as_tuple(lint.start)[0])}.1.1")
        when = planned.get(tid)
        if when is not None and when <= due + 365:
            continue
        g = gain([(due, "tech", tid, "lever")]) if when is None else 0
        out.append({"kind": "tech", "id": tid, "gives": _gain_text(mods),
                    "gate": f"year {year}" + (f"; the plan finishes it {_fmt(when)}" if when else "; not planned"),
                    "gain": g})
    # focuses with factories or construction ideas off the path
    path = set(lint.plan.get("focus_path") or []) | set(lint.plan.get("focus_fallback") or [])
    ideas = idea_table(lint.game)
    for fid, evs in _rewards(lint).items():
        if fid in path:
            continue
        civ = sum(p[1] for k, p in evs if k == "build" and p[0] == "industrial_complex")
        mil = sum(p[1] for k, p in evs if k == "build" and p[0] == "arms_factory")
        speed = {}
        for k, p in evs:
            if k == "idea+" and (ideas.get(p) or {}).get("group") not in LAW_GROUPS:
                for key, v in modifiers_of(lint.game, w, tag, p).items():
                    if key in useful and v > 0:
                        speed[key] = speed.get(key, 0) + v
        if civ + mil >= 2 or speed:
            gives = ", ".join(x for x in (f"+{civ} civilian" if civ else "", f"+{mil} military" if mil else "",
                                          _gain_text(speed)) if x)
            out.append({"kind": "focus", "id": fid, "gives": gives, "gate": "focus tree", "gain": civ + mil})
    out.sort(key=lambda x: (-x["gain"], x["kind"], x["id"]))
    return out[:limit]


def at(rows, day):
    """The forecast row on or before day (rows sorted by date)."""
    best = None
    for r in rows:
        if _day(r["date"]) <= day:
            best = r
    return best


def _atoms(pred):
    if isinstance(pred, str):
        yield pred
    elif isinstance(pred, list):
        for p in pred:
            yield from _atoms(p)
    elif isinstance(pred, dict):
        for p in pred.values():
            yield from _atoms(p)
