#!/usr/bin/env python3
"""Compare a generated plan with a reference plan (strategist design 4.1).

Usage:
    eval_plan.py GEN.json REF.json [--game DIR | --no-game] [--mod PATH ...] [--json]

Each row is one metric: the value for GEN, the value for REF and a score in 0..1 where 1 means GEN is
as good as REF or matches it (so REF against itself scores 1 everywhere). Divergence is not failure in
itself; the table feeds the human diff review. Nothing here knows a country: tags, states, techs and
predicates all come from the two plans and the game files.

Rows (game = needs the game folder, through hoi4facts.lint_plan):
    goal        predicates     goal predicate set equality (Jaccard); deprecated surrender_goal /
                               win_condition are read as capitulated:/state_controller:/decision:
    goal        deadline       days between the deadlines
    focus       first5         first five focus_path keys as a set, overlap with REF's
    focus       kendall_tau    order agreement over the shared focus keys ((tau + 1) / 2)
    research    jaccard        mean per-year Jaccard of the research_path blocks
    research    past_start     techs whose start_year is after their block year (game)
    vp_targets  cumulative_vp  VP of the targets (file values with the game, else the plan's vp)
    vp_targets  spearman       order agreement over the shared target provinces ((rho + 1) / 2)
    vp_targets  reachable      share of targets next to own land or an earlier target (game)
    construction rail_first    share of railway/infrastructure items queued before the first factory
    construction hubs          supply_node items (20000 points each; fewer is better, each must pass hub_cost)
    construction factory_infra mean infrastructure percentile of the factory states (game)
    construction front         factories on fronts (lint construction.front, game)
    production  factories      total factories on lines
    production  mix            overlap of the factory shares by equipment family
    events      options        share of REF's event choices (event -> option name) that GEN makes
    army        guard_states   Jaccard of the guarded state sets
    army        guard_cover    share of the own border states with a guard (game)
    executor    coverage       share of plan items under a key with x-owner executor (plan.schema.json)

Stdlib only, Python 3.9+.
"""
import argparse
import json
import os
import re
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
SCRIPTS = os.path.dirname(HERE)
ROOT = os.path.dirname(SCRIPTS)
sys.path.insert(0, SCRIPTS)

from hoi4facts import derive, lint_plan  # noqa: E402

SCHEMA_PATH = os.path.join(ROOT, "skills", "hoi4-campaign-playbook", "plan.schema.json")
DEFAULT_GAME_DIR = lint_plan.DEFAULT_GAME_DIR


# --------------------------------------------------------------------------- plan readers


def _dicts(plan, key):
    v = plan.get(key)
    return [x for x in v if isinstance(x, dict)] if isinstance(v, list) else []


def _strs(plan, key):
    v = plan.get(key)
    return [x for x in v if isinstance(x, str)] if isinstance(v, list) else []


def _norm_pred(p):
    return p if isinstance(p, str) else json.dumps(p, sort_keys=True)


def tag_of(plan):
    meta = plan.get("_validator") if isinstance(plan.get("_validator"), dict) else {}
    return meta.get("tag", "")


def goal_predicates(plan):
    """(set of predicate strings, deadline or None). Deprecated keys are read when goal is absent."""
    goal = plan.get("goal") if isinstance(plan.get("goal"), dict) else {}
    preds = {_norm_pred(p) for p in goal.get("predicates") or []}
    if not preds:
        war = plan.get("war") if isinstance(plan.get("war"), dict) else {}
        sg = plan.get("surrender_goal")
        if isinstance(sg, dict):
            preds.update(f"capitulated:{t}" for t in war.get("enemies") or [] if isinstance(t, str))
            if isinstance(sg.get("must_control_state_at_end"), int):
                preds.add(f"state_controller:{sg['must_control_state_at_end']}={tag_of(plan)}")
        wc = plan.get("win_condition")
        if isinstance(wc, dict) and isinstance(wc.get("decision"), str):
            preds.add(f"decision:{wc['decision']}")
    return preds, goal.get("deadline")


def research_blocks(plan):
    out = {}
    for e in _dicts(plan, "research_path"):
        y = e.get("year")
        if not isinstance(y, int):
            t = derive.as_tuple(e.get("not_before"))
            y = t[0] if t else 0
        out.setdefault(y, set()).update(t for t in e.get("techs") or [] if isinstance(t, str))
    return out


def targets(plan):
    t = [e for e in _dicts(plan, "vp_targets") if isinstance(e.get("province"), int)]
    if t and all(isinstance(e.get("order"), int) for e in t):
        t = sorted(t, key=lambda e: e["order"])
    return t


def construction_items(plan):
    return _dicts(plan, "construction_plan")


def factory_states(plan, factory_keys):
    out = []
    for e in construction_items(plan):
        if e.get("building") in factory_keys and isinstance(e.get("state"), int):
            out.append(e["state"])
    for e in _dicts(plan, "construction"):
        if e.get("building") in factory_keys:
            out += [s for s in e.get("states") or [] if isinstance(s, int)]
    return out


def supply_first(plan, supply_keys, factory_keys):
    """Share of supply items placed before the first factory item of construction_plan (1.0 when
    there are no supply items or no factory items)."""
    items = construction_items(plan)
    supply = [i for i, e in enumerate(items) if e.get("building") in supply_keys]
    factories = [i for i, e in enumerate(items) if e.get("building") in factory_keys]
    if not supply or not factories:
        return 1.0
    return sum(1 for i in supply if i < factories[0]) / len(supply)


def production_mix(plan):
    out = {}
    for e in _dicts(plan, "production"):
        k = e.get("equipment")
        n = e.get("factories")
        if isinstance(k, str) and isinstance(n, int):
            fam = re.sub(r"_\d+$", "", k)
            out[fam] = out.get(fam, 0) + n
    return out


def guard_states(plan):
    war = plan.get("war") if isinstance(plan.get("war"), dict) else {}
    out = {s for s in war.get("guard_states") or [] if isinstance(s, int)}
    g = war.get("guards")
    if isinstance(g, dict):
        out.update(v for v in g.values() if isinstance(v, int))
    for e in _dicts(plan, "army_layout"):
        if e.get("role") == "guard":
            out.update(s for s in e.get("guard_states_in_order") or [] if isinstance(s, int))
    return out


def event_choices(plan):
    return {e.get("event"): e.get("option_name") for e in _dicts(plan, "event_rules") if e.get("event")}


def executor_share(plan, schema):
    props = (schema or {}).get("properties") or {}
    total = mine = 0
    for k, v in plan.items():
        if k.startswith("_"):
            continue
        w = len(v) if isinstance(v, list) else 1
        total += w
        if (props.get(k) or {}).get("x-owner") == "executor":
            mine += w
    return mine / total if total else 0.0


# --------------------------------------------------------------------------- statistics


def jaccard(a, b):
    a, b = set(a), set(b)
    return 1.0 if not a and not b else len(a & b) / len(a | b)


def kendall_tau(gen, ref):
    shared = [k for k in ref if k in set(gen)]
    n = len(shared)
    if n < 2:
        return None
    pos = {k: i for i, k in enumerate(gen)}
    conc = disc = 0
    for i in range(n):
        for j in range(i + 1, n):
            s = pos[shared[i]] - pos[shared[j]]
            if s < 0:
                conc += 1
            elif s > 0:
                disc += 1
    return (conc - disc) / (n * (n - 1) / 2)


def spearman(gen, ref):
    shared = [k for k in ref if k in set(gen)]
    n = len(shared)
    if n < 2:
        return None
    gen_rank = {k: i for i, k in enumerate([k for k in gen if k in set(shared)])}
    d2 = sum((i - gen_rank[k]) ** 2 for i, k in enumerate(shared))
    return 1 - 6 * d2 / (n * (n * n - 1))


def higher_better(g, r):
    if g is None or r is None:
        return None
    if g >= r:
        return 1.0
    return g / r if r else 1.0


def lower_better(g, r):
    if g is None or r is None:
        return None
    return 1.0 if g <= r else 1.0 / (1 + g - r)


def ratio(g, r):
    if g is None or r is None:
        return None
    if g == r:
        return 1.0
    return min(g, r) / max(g, r) if max(g, r) else 1.0


# --------------------------------------------------------------------------- game rows


class GameView:
    """Lint results and facts for one plan (None without a game)."""

    def __init__(self, plan, game):
        self.plan = plan
        self.game = game
        self.result = lint_plan.lint(plan, game) if game is not None else None
        self.lint = lint_plan.Lint(plan, game) if game is not None else None

    def count(self, rule, severity="error"):
        if not self.result:
            return None
        return sum(1 for f in self.result["findings"] if f["rule"] == rule and f["severity"] == severity)

    def cumulative_vp(self):
        if self.game is None:
            return float(sum(e.get("vp", 0) for e in targets(self.plan) if isinstance(e.get("vp"), (int, float))))
        facts = self.game.state_facts(self.lint.ctx, self.lint.start)
        vps = {p: v for r in facts.values() for p, v in r["vps"].items()}
        taken = set()
        for e in targets(self.plan):
            taken.update(self.lint.captures(e))
        return float(sum(vps.get(p, 0) for p in taken))

    def reachable(self):
        if not self.result:
            return None
        n = len(targets(self.plan))
        if not n:
            return 1.0
        bad = {f["where"] for f in self.result["findings"] if f["rule"] == "conquest.adjacency"}
        return 1 - len(bad) / n

    def factory_infra(self):
        if self.game is None:
            return None
        kinds = derive.building_kinds(self.game)
        sb = derive.state_buildings(self.game, self.lint.ctx, self.lint.start)
        own = sorted(derive.tag_states(self.game, self.lint.ctx, self.lint.tag, self.lint.start))
        infra = {s: sum(v for b, v in sb.get(s, {}).items() if b in kinds["infrastructure"]) for s in own}
        chosen = [s for s in factory_states(self.plan, kinds["factory"]) if s in infra]
        if not chosen or len(own) < 2:
            return 1.0
        # percentile of each chosen state: share of own states with lower infrastructure (ties half)
        vals = list(infra.values())

        def pct(s):
            v = infra[s]
            return (sum(1 for x in vals if x < v) + 0.5 * (sum(1 for x in vals if x == v) - 1)) / (len(vals) - 1)

        return sum(pct(s) for s in chosen) / len(chosen)

    def guard_cover(self):
        if not self.result:
            return None
        borders = self.result["facts"].get("border_states") or []
        if not borders:
            return 1.0
        return 1 - self.count("army.guards") / len(borders)

    def past_start(self):
        if not self.result:
            return None
        return sum(1 for f in self.result["findings"] if f["rule"] == "research.start_year" and f["severity"] == "error")


# --------------------------------------------------------------------------- evaluation


def evaluate(gen, ref, game=None, schema=None):
    """[{"area", "metric", "gen", "ref", "score"}] for GEN against REF."""
    if schema is None:
        try:
            with open(SCHEMA_PATH, encoding="utf-8") as fh:
                schema = json.load(fh)
        except (OSError, ValueError):
            schema = {}
    gv, rv = GameView(gen, game), GameView(ref, game)
    kinds = derive.building_kinds(game) if game is not None else {
        "factory": {"arms_factory", "industrial_complex", "dockyard"}, "supply": {"supply_node"},
        "railway": {"rail_way", "railway"}}
    rows = []

    def row(area, metric, g, r, score):
        rows.append({"area": area, "metric": metric, "gen": g, "ref": r,
                     "score": None if score is None else round(score, 3)})

    gp, gd = goal_predicates(gen)
    rp, rd = goal_predicates(ref)
    row("goal", "predicates", sorted(gp), sorted(rp), jaccard(gp, rp))
    if gd or rd:
        if gd and rd:
            score = max(0.0, 1 - abs(derive.day(gd) - derive.day(rd)) / 30)
        else:
            score = 0.0  # one plan has a deadline, the other none
        row("goal", "deadline", gd, rd, score)
    gf, rf = _strs(gen, "focus_path"), _strs(ref, "focus_path")
    g5, r5 = gf[:5], rf[:5]
    row("focus", "first5", g5, r5, len(set(g5) & set(r5)) / len(r5) if r5 else 1.0)
    tau = kendall_tau(gf, rf)
    row("focus", "kendall_tau", None if tau is None else round(tau, 3), 1.0, None if tau is None else (tau + 1) / 2)
    gb, rb = research_blocks(gen), research_blocks(ref)
    years = sorted(set(gb) | set(rb))
    per_year = {y: round(jaccard(gb.get(y, ()), rb.get(y, ())), 3) for y in years}
    row("research", "jaccard", per_year, None,
        sum(per_year.values()) / len(per_year) if per_year else 1.0)
    row("research", "past_start", gv.past_start(), rv.past_start(), lower_better(gv.past_start(), rv.past_start()))
    gvp, rvp = gv.cumulative_vp(), rv.cumulative_vp()
    row("vp_targets", "cumulative_vp", gvp, rvp, higher_better(gvp, rvp))
    go, ro = [e["province"] for e in targets(gen)], [e["province"] for e in targets(ref)]
    rho = spearman(go, ro)
    row("vp_targets", "spearman", None if rho is None else round(rho, 3), 1.0, None if rho is None else (rho + 1) / 2)
    row("vp_targets", "reachable", gv.reachable(), rv.reachable(), higher_better(gv.reachable(), rv.reachable()))
    # Cheap supply (railway levels, infrastructure) ahead of factories is good; supply hubs cost
    # 20000 points (common/buildings), so they are counted separately and every one must pass
    # the lint rule construction.hub_cost (finishes before it is needed).
    cheap = set(kinds["railway"]) | {"infrastructure"}
    gs, rs = supply_first(gen, cheap, kinds["factory"]), supply_first(ref, cheap, kinds["factory"])
    row("construction", "rail_first", round(gs, 3), round(rs, 3), higher_better(gs, rs))
    gh = sum(1 for e in construction_items(gen) if e.get("building") in kinds["supply"])
    rh = sum(1 for e in construction_items(ref) if e.get("building") in kinds["supply"])
    row("construction", "hubs", gh, rh, lower_better(gh, rh))
    gi, ri = gv.factory_infra(), rv.factory_infra()
    row("construction", "factory_infra", gi if gi is None else round(gi, 3), ri if ri is None else round(ri, 3),
        higher_better(gi, ri))
    row("construction", "front", gv.count("construction.front"), rv.count("construction.front"),
        lower_better(gv.count("construction.front"), rv.count("construction.front")))
    gm, rm = production_mix(gen), production_mix(ref)
    gt, rt = sum(gm.values()), sum(rm.values())
    row("production", "factories", gt, rt, ratio(gt, rt))
    if gt and rt:
        mix = sum(min(gm.get(k, 0) / gt, rm.get(k, 0) / rt) for k in set(gm) | set(rm))
    else:
        mix = 1.0 if gt == rt else 0.0
    row("production", "mix", gm, rm, mix)
    ge, re_ = event_choices(gen), event_choices(ref)
    row("events", "options", len([k for k, v in re_.items() if ge.get(k) == v]), len(re_),
        (len([k for k, v in re_.items() if ge.get(k) == v]) / len(re_)) if re_ else 1.0)
    gg, rg = guard_states(gen), guard_states(ref)
    row("army", "guard_states", sorted(gg), sorted(rg), jaccard(gg, rg))
    row("army", "guard_cover", gv.guard_cover(), rv.guard_cover(), higher_better(gv.guard_cover(), rv.guard_cover()))
    gx, rx = executor_share(gen, schema), executor_share(ref, schema)
    row("executor", "coverage", round(gx, 3), round(rx, 3), higher_better(gx, rx))
    return rows


def summary(rows):
    scores = [r["score"] for r in rows if r["score"] is not None]
    return round(sum(scores) / len(scores), 3) if scores else None


def _short(v, width=38):
    text = json.dumps(v, default=sorted) if not isinstance(v, str) else v
    return text if len(text) <= width else text[:width - 3] + "..."


def table(rows):
    lines = [f"{'area':<13} {'metric':<14} {'gen':<38} {'ref':<38} score"]
    for r in rows:
        s = "n/a" if r["score"] is None else f"{r['score']:.3f}"
        lines.append(f"{r['area']:<13} {r['metric']:<14} {_short(r['gen']):<38} {_short(r['ref']):<38} {s}")
    lines.append(f"mean score {summary(rows)}")
    return "\n".join(lines)


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("gen")
    ap.add_argument("ref")
    ap.add_argument("--game", default=os.environ.get("HOI4_DIR", DEFAULT_GAME_DIR))
    ap.add_argument("--no-game", action="store_true", help="plan-only rows (game rows read n/a)")
    ap.add_argument("--userdir", default=None)
    ap.add_argument("--mod", action="append", default=[])
    ap.add_argument("--json", action="store_true")
    args = ap.parse_args(argv)
    plans = []
    for p in (args.gen, args.ref):
        try:
            with open(p, encoding="utf-8") as fh:
                plans.append(json.load(fh))
        except (OSError, ValueError) as exc:
            print(f"cannot read {p}: {exc}", file=sys.stderr)
            return 2
    game = None
    if not args.no_game:
        if os.path.isdir(args.game):
            game = lint_plan.load_game(args.game, args.userdir, args.mod)
        else:
            print(f"game directory not found ({args.game}); game rows are n/a", file=sys.stderr)
    rows = evaluate(plans[0], plans[1], game)
    if args.json:
        print(json.dumps({"rows": rows, "mean_score": summary(rows)}, indent=2, default=sorted))
    else:
        print(table(rows))
    return 0


if __name__ == "__main__":
    sys.exit(main())
