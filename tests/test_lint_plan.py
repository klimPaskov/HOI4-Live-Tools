"""scripts/hoi4facts/lint_plan.py (design 4.2), hoi4facts/derive.py helpers, the failure ledger
(references/lessons.jsonl) and scripts/strategist/eval_plan.py (design 4.1).

The hand-plan and seeded-plan tests read the installed game and are skipped without it."""

import copy
import io
import json
import os
import re
import sys
import unittest
from contextlib import redirect_stdout

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.join(ROOT, "scripts"))
sys.path.insert(0, os.path.join(ROOT, "scripts", "strategist"))

from hoi4facts import derive, lint_plan  # noqa: E402
import eval_plan  # noqa: E402

PLANS = os.path.join(ROOT, "skills", "hoi4-campaign-playbook", "plans")
LESSONS = os.path.join(ROOT, "skills", "hoi4-campaign-playbook", "references", "lessons.jsonl")
GAME_DIR = os.environ.get("HOI4_DIR", lint_plan.DEFAULT_GAME_DIR)
HAVE_GAME = os.path.isfile(os.path.join(GAME_DIR, "map", "provinces.bmp"))


def load(name):
    with open(os.path.join(PLANS, name), encoding="utf-8") as fh:
        return json.load(fh)


ITA = "italy-ethiopia-1936.plan.json"
ETH = "ethiopia-italy-1936.plan.json"


class CalendarAndHelpers(unittest.TestCase):
    def test_calendar_has_365_days(self):
        self.assertEqual(derive.date_of(derive.day("1936.1.1")), (1936, 1, 1))
        self.assertEqual(derive.add_days("1936.1.1", 355), (1936, 12, 22))
        self.assertEqual(derive.add_days("1936.12.31", 1), (1937, 1, 1))
        self.assertEqual(derive.day("1937.1.1") - derive.day("1936.1.1"), 365)
        self.assertEqual(derive.fmt("1936-3-5"), "1936.03.05")

    def test_assignable_is_a_joint_assignment(self):
        supply = {"A": 2, "B": 1}
        # greedy in order would give the first row A+B and starve the second; a flow finds 3
        rows = [("r1", ["B", "A"], 2), ("r2", ["B"], 1)]
        self.assertEqual(lint_plan.assignable(rows, supply), 3)
        self.assertEqual(lint_plan.assignable([("r", ["A"], 5)], supply), 2)

    def test_control_states_reads_nested_conditions(self):
        block = [("has_full_control_of_state", "550"), ("OR", [("controls_state", "559")]),
                 ("NOT", [("controls_state", "1")])]
        self.assertEqual(derive.control_states(block), {550: "has_full_control_of_state", 559: "controls_state"})

    def test_match_commander(self):
        table = {"ITA_giovanni_messe": {"display": "Giovanni Messe"}, "ETH_ras_kassa": {"display": "Kassa Haile Darge"}}
        self.assertEqual(derive.match_commander("Giovanni Messe", table), "ITA_giovanni_messe")
        self.assertEqual(derive.match_commander("Kassa Haile Darge", table), "ETH_ras_kassa")
        self.assertIsNone(derive.match_commander("Nobody", table))


# Lint rules and executor standing rules that lessons L13-L23 name and that other build items add. A lesson may
# name a rule from these lists before lint_plan.py or the executor has it; drop a name here once it lands.
PENDING_LINT_RULES = set()  # every lesson's lint rule is in lint_plan.py now
RULE_LINE = re.compile(r"^  ([a-z]+\.[a-z0-9_]+)\s", re.M)  # a rule line of the lint_plan docstring
EXECUTOR_RULES = {"planning_wait", "supply_floor", "pocket_threshold", "advance_in_bounds",
                  "factory_rebalance_steps", "domain_turns", "politics_auto", "command_concentration",
                  "move_commitment", "action_receipts", "construction_give_up", "supply_relief"}


class Lessons(unittest.TestCase):
    def test_ledger_lines_name_real_rules(self):
        rules = set(RULE_LINE.findall(lint_plan.__doc__))
        self.assertIn("construction.front", rules)
        with open(LESSONS, encoding="utf-8") as fh:
            rows = [json.loads(line) for line in fh if line.strip()]
        # design table 4.4, L12_early_hub_cost, L13-L23 tactics research, L24-L26 shakedowns (at least: other
        # branches append their own rows)
        self.assertGreaterEqual(len(rows), 25)
        ids = set()
        for r in rows:
            self.assertLessEqual({"id", "failure", "runs", "prevention", "lint_rule"}, set(r))
            self.assertLessEqual(set(r), {"id", "failure", "runs", "prevention", "lint_rule", "executor_rule"})
            self.assertNotIn(r["id"], ids)
            ids.add(r["id"])
            self.assertTrue(r["runs"] and isinstance(r["runs"], list))
            self.assertIsInstance(r["lint_rule"], list)
            self.assertTrue(r["lint_rule"] or r.get("executor_rule"), f"{r['id']}: no lint or executor rule")
            for rule in r["lint_rule"]:
                self.assertIn(rule, rules | PENDING_LINT_RULES, f"{r['id']}: {rule} is not a lint rule")
            for rule in r.get("executor_rule", []):
                self.assertIn(rule, EXECUTOR_RULES, f"{r['id']}: {rule} is not an executor rule")

    def test_pending_rules_are_used(self):
        with open(LESSONS, encoding="utf-8") as fh:
            rows = [json.loads(line) for line in fh if line.strip()]
        named = {rule for r in rows for rule in r["lint_rule"] + r.get("executor_rule", [])}
        self.assertEqual(PENDING_LINT_RULES - named, set())
        self.assertEqual(EXECUTOR_RULES - named, set())


class EvalWithoutGame(unittest.TestCase):
    def test_reference_against_itself_is_perfect(self):
        for name in (ITA, ETH):
            ref = load(name)
            rows = eval_plan.evaluate(copy.deepcopy(ref), ref, game=None)
            scored = [r for r in rows if r["score"] is not None]
            self.assertGreater(len(scored), 10)
            self.assertEqual([r for r in scored if r["score"] != 1.0], [], name)

    def test_perturbed_copy_scores_lower_where_changed(self):
        ref = load(ITA)
        gen = copy.deepcopy(ref)
        gen["focus_path"] = list(reversed(gen["focus_path"]))  # first 5 and order change
        gen["research_path"][0]["techs"] = gen["research_path"][0]["techs"][:8]
        gen["vp_targets"] = list(reversed(gen["vp_targets"]))[:10]
        for i, e in enumerate(gen["vp_targets"]):
            e["order"] = i + 1
        gen["production"][0]["factories"] = 2
        gen["event_rules"] = gen["event_rules"][:3]
        gen["war"]["guard_states"] = [550]
        gen["army_layout"] = [e for e in gen["army_layout"] if e["name_hint"] != "Jubaland guard"]
        gen["surrender_goal"].pop("must_control_state_at_end")
        rows = {(r["area"], r["metric"]): r["score"] for r in eval_plan.evaluate(gen, ref, game=None)}
        for key in [("goal", "predicates"), ("focus", "first5"), ("focus", "kendall_tau"), ("research", "jaccard"),
                    ("vp_targets", "cumulative_vp"), ("vp_targets", "spearman"), ("production", "factories"),
                    ("production", "mix"), ("events", "options"), ("army", "guard_states")]:
            self.assertLess(rows[key], 1.0, key)
        self.assertEqual(rows[("construction", "rail_first")], 1.0)  # untouched
        # event_rules is executor-owned since the strategy layer landed: cutting rules lowers the share
        self.assertLess(rows[("executor", "coverage")], 1.0)

    def test_statistics(self):
        self.assertEqual(eval_plan.kendall_tau(["a", "b", "c"], ["a", "b", "c"]), 1.0)
        self.assertEqual(eval_plan.kendall_tau(["c", "b", "a"], ["a", "b", "c"]), -1.0)
        self.assertEqual(eval_plan.spearman([3, 2, 1], [1, 2, 3]), -1.0)
        self.assertIsNone(eval_plan.spearman([1], [1]))


@unittest.skipUnless(HAVE_GAME, f"game files not found at {GAME_DIR}")
class HandPlans(unittest.TestCase):
    """The two hand plans give only these findings (each is explained in the commit / reply)."""

    @classmethod
    def setUpClass(cls):
        cls.game = lint_plan.load_game(GAME_DIR)

    def findings(self, plan, severity):
        out = lint_plan.lint(plan, self.game)
        return out, sorted((f["rule"], f["where"]) for f in out["findings"] if f["severity"] == severity)

    def test_italy(self):
        out, errors = self.findings(load(ITA), "error")
        self.assertEqual(errors, [
            ("conquest.adjacency", "vp_targets[16] 13244"),  # Gambella is two steps past Yabello
            ("construction.railway", "construction_plan[5] cp08_rail_way_843"),  # 7944 not on Dessie's route
            ("construction.railway", "construction_plan[6] cp09_rail_way_835"),  # whole line queued at Wal Wal
            ("schema.player_call", "retake_rules"),  # army_group is an executor key (plan_military.rs)
            ("win.goal", "goal"),  # only the deprecated surrender_goal
        ])
        _, warnings = self.findings(load(ITA), "warning")
        # the coverage-build rules report as warnings on a hand plan (_meta.source "hand")
        gaps = [("coverage.checklist", f"_coverage.c{n}") for n in (3, 5, 6, 7, 9, 14, 15, 17, 21, 22, 23, 24, 25, 27)]
        self.assertEqual(warnings, sorted(gaps + [
            # one army per guard task: three tiny guard armies and four formations in one theatre
            ("army.formations", "army_layout"),
            ("army.formations", "army_layout[4] 'South guard'"),
            ("army.formations", "army_layout[5] 'Jubaland guard'"),
            ("army.formations", "army_layout[6] 'Afar guard'"),
            # 6-8 wide colonial and irregular divisions fill the highland battles at 0.86-0.92, width 14 at 0.99
            ("army.width_terrain", "army_layout"),
            # the four railways start at plain provinces, not at a hub, port or the capital (sk2 shakedown)
            ("construction.railway_node", "construction_plan[2] cp04_rail_way_842"),
            ("construction.railway_node", "construction_plan[4] cp07_rail_way_271"),
            ("construction.railway_node", "construction_plan[5] cp08_rail_way_843"),
            ("construction.railway_node", "construction_plan[6] cp09_rail_way_835"),
            ("manage.thresholds", "manage"),
            ("navy.overseas", "navy"),  # East Africa is supplied by sea, no convoy or port plan
            ("navy.required", "navy"),
            ("occupation.required", "occupation"),  # Eritrea, Somaliland, Jubaland resist from day one
            ("peace.enemies", "peace"),
            ("politics.law_gates", "politics[0] war_economy"),  # war support > 0.5 gate, no `when`
            ("politics.pp_budget", "politics"),  # PP above 100 with no planned buy from late August
            # the L3/35 line: no army_layout template has light tanks and Divisione Celere is disbanded
            ("production.cull", "start line light_tank_chassis_1"),
            # 10 free factories, 9 go to start lines on day 0, the new artillery line draws from the same pool
            ("production.day0", "production"),
            ("research.throughput", "research_path[0]"),
            ("research.throughput", "research_path[1]"),
            ("supply.port_convoys", "production"),
            # the highland states lie past what the Eritrean hub and the Somali ports reach
            ("supply.axis_reach", "war_by_phase.P2_tigray.axes[0] state 271"),
            ("supply.axis_reach", "war_by_phase.P2_tigray.axes[1] state 835"),
            ("supply.axis_reach", "war_by_phase.P3_gondar_afar.axes[0] state 840"),
            ("supply.axis_reach", "war_by_phase.P3_gondar_afar.axes[2] state 836"),
            ("supply.axis_reach", "war_by_phase.P3_gondar_afar.axes[2] state 837"),
            ("war.planning_wait", "war.direct_moves"),  # direct moves build no planning
            ("war.supply_floor", "war.supply_floor"),  # 30 is under the 35% attrition line
        ]))
        reach = {r["where"]: r for r in out["facts"]["supply_reach"]}
        self.assertEqual(reach["war_by_phase.P2_tigray.axes[0] state 842"]["from"], 13250)  # Asmara hub
        self.assertGreaterEqual(reach["war_by_phase.P2_tigray.axes[0] state 842"]["reach"], 1.0)
        self.assertEqual(out["facts"]["overseas_states"], [550, 559, 844])
        self.assertEqual(out["facts"]["occupation"]["resisting"], [163, 550, 559, 844])
        self.assertNotIn("hub_cost", out["facts"])  # the two hubs that failed construction.hub_cost are gone
        facts = out["facts"]
        self.assertEqual(facts["surrender"]["ETH"]["total_vp"], 32)
        self.assertEqual(facts["surrender"]["ETH"]["vp_needed"], 26)
        self.assertGreaterEqual(facts["surrender"]["ETH"]["planned_vp"], 26)
        self.assertEqual(facts["border_states"], [550, 559, 844])
        self.assertEqual(facts["military_factories"]["start"], 20)
        timers = {t["mission"]: (t["earliest"], t["latest"]) for t in facts["timers"]}
        self.assertEqual(timers["ITA_missiolini_conquer_ethiopia"], ("1936.12.22", "1936.12.27"))
        self.assertIn("271", facts["goal_states"])

    def test_supply_reach_falls_with_distance_and_rises_with_trucks(self):
        from hoi4facts import mapgraph
        graph = self.game.map_graph()
        path = mapgraph.route(graph, 13250, 5010)["path"]  # Asmara hub to Addis Ababa
        near = derive.supply_reach(self.game, path[:3], "hub")
        far = derive.supply_reach(self.game, path, "hub")
        self.assertGreater(near, far)
        self.assertGreater(derive.supply_reach(self.game, path, "hub", 2), far)
        self.assertEqual(derive.supply_reach(self.game, path, "port", 2), derive.supply_reach(self.game, path, "port"))
        self.assertEqual(derive.supply_reach(self.game, path[:1], "capital"), 5.0)
        nodes = derive.railway_nodes(self.game, "ITA", None)
        self.assertEqual(nodes[13250], "hub")
        self.assertEqual(nodes[12991], "port")  # Mogadishu: a naval base, no hub on 1.19.4
        self.assertNotIn(13238, nodes)

    def test_ethiopia(self):
        out, errors = self.findings(load(ETH), "error")
        self.assertEqual(errors, [
            ("schema.player_call", "retake_rules"),
            ("win.goal", "goal"),  # only the deprecated win_condition
        ])
        _, warnings = self.findings(load(ETH), "warning")
        gaps = [("coverage.checklist", f"_coverage.c{n}") for n in (3, 5, 6, 9, 12, 14, 15, 17, 22, 23, 25)]
        self.assertEqual(warnings, sorted(gaps + [
            ("army.width_terrain", "army_layout"),  # Mehal Sefari and Mountain Division are 8 wide (0.92)
            ("manage.thresholds", "manage"),
            ("occupation.required", "occupation"),  # Eritrea, Somaliland, Jubaland are conquered
            ("peace.enemies", "peace"),
            ("politics.law_gates", "politics[0] partial_economic_mobilisation"),
            ("politics.pp_budget", "politics"),  # PP idle after the one planned law
            ("research.throughput", "research_path[0]"),
            ("research.throughput", "research_path[1]"),
            # the Italian coast is far from Addis Ababa over the highlands
            ("supply.axis_reach", "war_by_phase.P3_eritrea.axes[0] state 550"),
            ("supply.axis_reach", "war_by_phase.P4_south.axes[0] state 559"),
            ("supply.axis_reach", "war_by_phase.P4_south.axes[0] state 844"),
            ("war.planning_wait", "war.direct_moves"),
            ("war.supply_floor", "war.supply_floor"),  # 20 is under the 35% attrition line
        ]))
        self.assertEqual(out["facts"]["overseas_states"], [])  # landlocked: no navy rules
        # the decision ETH_defeat_italy gives the states the goal must hold
        self.assertEqual(out["facts"]["goal_states"], {"550": "full", "559": "full", "844": "full"})
        self.assertEqual(out["facts"]["border_states"], [835, 836, 837, 841, 842])

    def test_cli_runs_as_a_script(self):
        # the script path (not the package import) must load every helper module
        import subprocess
        r = subprocess.run([sys.executable, os.path.join(ROOT, "scripts", "hoi4facts", "lint_plan.py"),
                            os.path.join(PLANS, ETH), "--game", GAME_DIR, "--json"], capture_output=True, text=True)
        self.assertEqual(r.returncode, 1, r.stderr)
        self.assertEqual(json.loads(r.stdout)["errors"], 2)

    def test_cli_exit_status_and_json(self):
        buf = io.StringIO()
        with redirect_stdout(buf):
            code = lint_plan.main([os.path.join(PLANS, ETH), "--game", GAME_DIR, "--json"])
        self.assertEqual(code, 1)
        self.assertEqual(json.loads(buf.getvalue())["errors"], 2)


@unittest.skipUnless(HAVE_GAME, f"game files not found at {GAME_DIR}")
class SeededBadPlans(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.game = lint_plan.load_game(GAME_DIR)
        cls.base = load(ITA)
        cls.base_errors = {(f["rule"], f["where"]) for f in lint_plan.lint(cls.base, cls.game)["findings"]
                           if f["severity"] == "error"}

    def new_errors(self, plan):
        out = lint_plan.lint(plan, self.game)
        return [f for f in out["findings"] if f["severity"] == "error" and (f["rule"], f["where"]) not in self.base_errors]

    def assertRule(self, plan, rule, text=""):
        errs = self.new_errors(plan)
        hit = [f for f in errs if f["rule"] == rule and text in f["where"] + " " + f["message"]]
        self.assertTrue(hit, f"no {rule} error containing {text!r}; new errors: {errs}")
        return errs

    def test_phase_override_axes_are_checked_for_supply(self):
        # axes a phase carries in overrides.war are advances like any other and need the same supply check
        plan = copy.deepcopy(self.base)
        plan["phases"] = [{"id": "PX", "overrides": {"war": {"axes": [[271]]}}}] + list(plan.get("phases") or [])
        plan["war"] = dict(plan.get("war") or {}, axes=[])
        plan["war_by_phase"] = {}
        found = [f for f in lint_plan.lint(plan, self.game)["findings"]
                 if f["rule"] == "supply.axis_reach" and f["where"].startswith("phases[0].overrides.war")]
        self.assertTrue(found, "the phase override axis to state 271 was not checked")

    def test_factory_on_the_front(self):
        plan = copy.deepcopy(self.base)
        plan["construction_plan"].append({"id": "cp99_arms_factory_550", "order": 99, "building": "arms_factory",
                                          "state": 550, "when": "1936.01.01"})
        self.assertRule(plan, "construction.front", "cp99")
        self.assertRule(plan, "construction.infrastructure", "cp99")  # Eritrea has infrastructure 2

    def test_factory_after_infrastructure_is_only_a_front_problem(self):
        plan = copy.deepcopy(self.base)
        plan["construction_plan"] += [
            {"id": "cp98_infrastructure_550", "order": 98, "building": "infrastructure", "state": 550, "when": ""},
            {"id": "cp99_arms_factory_550", "order": 99, "building": "arms_factory", "state": 550, "when": ""}]
        rules = {f["rule"] for f in self.new_errors(plan)}
        self.assertIn("construction.front", rules)
        self.assertNotIn("construction.infrastructure", rules)

    def test_tech_from_1940_in_the_1936_block(self):
        techs = self.game.techs()
        late = sorted(k for k, t in techs.items() if t["year"] == 1940)[0]
        plan = copy.deepcopy(self.base)
        plan["research_path"][0]["techs"].append(late)
        self.assertRule(plan, "research.start_year", late)

    def test_vp_targets_below_the_threshold(self):
        plan = copy.deepcopy(self.base)
        plan["vp_targets"] = plan["vp_targets"][:6]
        self.assertRule(plan, "conquest.threshold", "needs 26")

    def test_mandatory_state_without_target(self):
        plan = copy.deepcopy(self.base)
        plan["vp_targets"] = [e for e in plan["vp_targets"] if e["state"] != 271]
        self.assertRule(plan, "conquest.mandatory", "state 271")

    def test_unguarded_border_state(self):
        plan = copy.deepcopy(self.base)
        plan["war"]["guard_states"] = [550, 559]
        plan["army_layout"] = [e for e in plan["army_layout"] if e["name_hint"] != "Jubaland guard"]
        self.assertRule(plan, "army.guards", "state 844")

    def test_exclusive_focus_pair(self):
        tree = self.game.focus_trees()[lint_plan.Lint(self.base, self.game).run().facts["focus"]["tree"]]
        a, b = next((k, sorted(v["exclusive"])[0]) for k, v in sorted(tree.items()) if v["exclusive"])
        plan = copy.deepcopy(self.base)
        plan["focus_fallback"] = plan["focus_fallback"] + [a, b]
        self.assertRule(plan, "focus.exclusive", min(a, b))

    def test_unrestricted_fallback_and_duplicate_general(self):
        plan = copy.deepcopy(self.base)
        plan["research_fallback"] = True
        plan["research_fallback_prefixes"] = []
        plan["army_layout"][1]["general"] = plan["army_layout"][0]["general"]
        self.assertRule(plan, "research.fallback")
        self.assertRule(plan, "army.generals", "more than one army")

    def test_eval_with_game_rows(self):
        ref = self.base
        rows = eval_plan.evaluate(copy.deepcopy(ref), ref, game=self.game)
        self.assertEqual([r for r in rows if r["score"] not in (None, 1.0)], [])
        gen = copy.deepcopy(ref)
        gen["construction_plan"].insert(0, {"id": "cp00_arms_factory_550", "order": 0, "building": "arms_factory",
                                            "state": 550, "when": "1936.01.01"})
        gen["research_path"][0]["techs"].append(sorted(k for k, t in self.game.techs().items() if t["year"] == 1940)[0])
        gen["war"]["guard_states"] = [550, 559]
        gen["army_layout"] = [e for e in gen["army_layout"] if e["name_hint"] != "Jubaland guard"]
        scores = {(r["area"], r["metric"]): r["score"] for r in eval_plan.evaluate(gen, ref, game=self.game)}
        for key in [("construction", "front"), ("construction", "rail_first"), ("construction", "factory_infra"),
                    ("research", "past_start"), ("army", "guard_cover")]:
            self.assertLess(scores[key], 1.0, key)


def strategist(plan):
    """A seeded copy as a strategist plan: the coverage-build rules report errors."""
    plan = copy.deepcopy(plan)
    plan["_meta"] = {"version": 1, "source": "strategist"}
    return plan


@unittest.skipUnless(HAVE_GAME, f"game files not found at {GAME_DIR}")
class CoverageBuildRules(unittest.TestCase):
    """The W1-2 lint rules on seeded copies of the hand plans."""

    @classmethod
    def setUpClass(cls):
        cls.game = lint_plan.load_game(GAME_DIR)
        cls.ita = strategist(load(ITA))
        cls.eth = strategist(load(ETH))

    def run_lint(self, plan, **kw):
        return lint_plan.lint(plan, self.game, **kw)

    def hits(self, plan, rule, severity=None, text=""):
        out = self.run_lint(plan)
        return [f for f in out["findings"] if f["rule"] == rule and (severity is None or f["severity"] == severity)
                and text in f["where"] + " " + f["message"]]

    def assertHit(self, plan, rule, severity, text=""):
        found = self.hits(plan, rule, severity, text)
        self.assertTrue(found, f"no {severity} {rule} containing {text!r}: "
                               f"{[f for f in self.run_lint(plan)['findings'] if f['rule'] == rule]}")
        return found[0]

    def assertNoHit(self, plan, rule, severity=None):
        self.assertEqual(self.hits(plan, rule, severity), [])

    # -------------------------------------------------------------- severity policy
    def test_hand_source_and_flag_downgrade_only_new_rules(self):
        hand = load(ITA)
        self.assertTrue(self.hits(hand, "peace.enemies", "warning"))
        self.assertTrue(self.hits(self.ita, "peace.enemies", "error"))
        out = self.run_lint(self.ita, hand=True)
        self.assertFalse([f for f in out["findings"] if f["rule"] in lint_plan.NEW_RULES and f["severity"] == "error"])
        self.assertTrue([f for f in out["findings"] if f["rule"] == "win.goal" and f["severity"] == "error"])
        rules = set(RULE_LINE.findall(lint_plan.__doc__))
        self.assertLessEqual(lint_plan.NEW_RULES, rules)

    # -------------------------------------------------------------- coverage.checklist
    def test_coverage_checklist(self):
        plan = copy.deepcopy(self.ita)
        del plan["_coverage"]
        self.assertHit(plan, "coverage.checklist", "error", "no _coverage")
        plan = copy.deepcopy(self.ita)
        del plan["_coverage"]["c4"]
        plan["_coverage"]["c11"] = "key:research_path,templates"
        plan["_coverage"]["c26"] = "call:event_rules"
        self.assertHit(plan, "coverage.checklist", "error", "c4 (doctrine path)")
        self.assertHit(plan, "coverage.checklist", "error", "key templates is not in the plan")
        self.assertHit(plan, "coverage.checklist", "error", "plan.event_rules has no item with a call")
        self.assertHit(self.ita, "coverage.checklist", "error", "c3 (force posture")  # na: missing is a gap
        plan["_coverage"] = {c: "na: not applicable here" for c in plan["_coverage"]}
        plan["_coverage"]["c4"] = "md:plan.md section 4"
        self.assertNoHit(plan, "coverage.checklist")

    # -------------------------------------------------------------- posture.shares
    def test_posture_shares(self):
        plan = copy.deepcopy(self.ita)
        plan["force_posture"] = {"reason": "land war; CAS against an enemy without air",
                                 "phases": [{"phase": "base", "land": 60, "air": 40, "navy": 0},
                                            {"phase": "P2_tigray", "land": 50, "air": 30, "navy": 30}]}
        f = self.assertHit(plan, "posture.shares", "warning", "phases[0]")
        self.assertIn("air 0% vs 40%", f["message"])  # every Italian line is land equipment
        self.assertHit(plan, "posture.shares", "error", "land + air + navy is 110")
        plan["force_posture"]["phases"] = [{"phase": "base", "land": 100, "air": 0, "navy": 0}]
        self.assertNoHit(plan, "posture.shares")

    # -------------------------------------------------------------- navy, ports, convoys
    def test_navy_required_and_overseas(self):
        self.assertHit(self.ita, "navy.required", "error", "coastal state")
        f = self.assertHit(self.ita, "navy.overseas", "error", "550, 559, 844")
        self.assertIn("a convoy plan", f["message"])
        plan = copy.deepcopy(self.ita)
        plan["navy"] = {"posture": "coastal", "reason": "no naval enemy; escort the Red Sea convoys"}
        plan["sea_transfer"] = {"convoys": 40, "ports": [550, 559]}
        self.assertNoHit(plan, "navy.required")
        self.assertNoHit(plan, "navy.overseas")
        self.assertNoHit(self.eth, "navy.required")  # landlocked at the start
        self.assertNoHit(self.eth, "navy.overseas")

    def test_port_convoys(self):
        f = self.assertHit(self.ita, "supply.port_convoys", "error", "by sea")
        self.assertIn("0.25 convoys per supply", f["message"])
        plan = copy.deepcopy(self.ita)
        plan["construction_plan"].append({"id": "cp98_naval_base_158", "building": "naval_base", "state": 158,
                                          "province": 11786, "when": "1936.01.01"})
        self.assertHit(plan, "supply.port_convoys", "error", "builds a port in state(s) 158")
        plan["manage"] = {"convoys_free": 0.2}
        self.assertNoHit(plan, "supply.port_convoys")
        plan = copy.deepcopy(self.ita)
        plan["production"].append({"equipment": "convoy_1", "factories": 2})
        self.assertNoHit(plan, "supply.port_convoys")

    # -------------------------------------------------------------- construction.hub_cost
    def test_day_one_hub_in_an_infrastructure_0_state_is_an_error(self):
        plan = copy.deepcopy(self.eth)
        plan["construction_plan"].append({"id": "cp99_supply_node_835", "building": "supply_node", "state": 835,
                                          "when": "1936.01.01"})
        f = self.assertHit(plan, "construction.hub_cost", "error", "cp99")
        self.assertIn("about 2000 days", f["message"])  # 20000 / (2 civs x 5 x 1.0)
        self.assertIn("infrastructure 0", f["message"])
        self.assertIn("plan horizon 1938.01.01", f["message"])

    def test_hub_names_the_railway_and_port_alternatives(self):
        plan = copy.deepcopy(self.ita)
        plan["construction_plan"].insert(0, {"id": "cp01_supply_node_559", "building": "supply_node", "state": 559,
                                             "province": 2020, "when": "1936.01.01"})
        f = self.assertHit(plan, "construction.hub_cost", "warning", "cp01")
        self.assertIn("about 286 days", f["message"])  # 10 free civs, infrastructure 2
        self.assertIn("port alternative: naval_base", f["message"])
        self.assertIn("3.3 vs 2.8", f["message"])
        plan = copy.deepcopy(self.ita)
        plan["construction_plan"].append({"id": "cp05_supply_node_842", "building": "supply_node", "state": 842,
                                          "province": 2072, "when": "state_fully_controlled:842"})
        f = self.assertHit(plan, "construction.hub_cost", "error", "cp05")
        self.assertIn("about 400 days", f["message"])
        self.assertIn("plan horizon 1937.01.01", f["message"])

    # -------------------------------------------------------------- air.base_capacity
    def test_air_base_capacity(self):
        self.assertNoHit(self.ita, "air.base_capacity")  # 120 planes, Eritrea level 3 = 600
        plan = copy.deepcopy(self.ita)
        plan["air_plan"][0]["count"] = 600
        self.assertHit(plan, "air.base_capacity", "warning", "state 550")

    # -------------------------------------------------------------- templates
    def test_templates(self):
        plan = copy.deepcopy(self.ita)
        plan["templates"] = [
            {"id": "tiny", "name": "Tiny", "regiments": [["infantry", "infantry"]], "support": [], "role": "line"},
            {"id": "tanks", "name": "Tanks", "regiments": [["modern_armor"] * 5, ["modern_armor"] * 5],
             "support": [], "role": "armour", "xp": 60},
            {"id": "garr", "name": "Garrison", "regiments": ["cavalry", "cavalry"], "support": ["military_police"],
             "role": "garrison"},
        ]
        f = self.assertHit(plan, "army.width_terrain", "error", "templates[0] tiny")
        self.assertIn("mountain", f["message"])  # the highland terrain mix of the theatre
        self.assertFalse([f for f in self.hits(plan, "army.width_terrain") if "garr" in f["where"]])
        what = "modern_armor equipment modern_tank_chassis"  # armour battalions come with their chassis
        self.assertHit(plan, "templates.equipment", "error", what)
        self.assertHit(plan, "templates.xp", "error", "no xp_budget")
        plan["xp_budget"] = {"army": [{"id": "x1", "cost": 50, "purpose": "template", "ref": "template:tanks"}]}
        self.assertHit(plan, "templates.xp", "warning", "budgets 50 XP")
        arche = derive.equipment_archetypes(self.game)
        tech = sorted(t for t, m in derive.tech_meta(self.game).items()
                      if any(arche.get(e, e) == "modern_tank_chassis" for e in m["enables"]))[0]
        plan["research_path"][2]["techs"].append(tech)  # planned for 1938, after the template's 1936
        self.assertFalse(self.hits(plan, "templates.equipment", "error", what))
        self.assertHit(plan, "templates.equipment", "warning", f"{what} needs {tech}")

    # -------------------------------------------------------------- politics.law_gates
    def test_law_gates(self):
        f = self.assertHit(self.ita, "politics.law_gates", "error", "war_economy")
        self.assertIn("war_support > 0.5", f["message"])
        plan = copy.deepcopy(self.ita)
        plan["politics"][0]["when"] = "war_support > 0.5"
        self.assertNoHit(plan, "politics.law_gates")
        plan["politics"][0]["when"] = {"all": ["1936.03.01", "stability >= 0.5"]}
        self.assertHit(plan, "politics.law_gates", "warning", "does not test war_support > 0.5")

    # -------------------------------------------------------------- occupation and peace
    def test_occupation_and_peace(self):
        f = self.assertHit(self.ita, "occupation.required", "error", "163, 550, 559, 844 resist")
        self.assertIn("conquers 271", f["message"])
        self.assertHit(self.eth, "occupation.required", "error", "conquers 550, 559, 844")
        self.assertHit(self.ita, "peace.enemies", "error", "ETH")
        plan = copy.deepcopy(self.ita)
        plan["occupation"] = {"default_law": "military_governor_occupation", "harsher_at_resistance": 50}
        plan["peace"] = {"take": [271, 842, 843]}
        self.assertNoHit(plan, "occupation.required")
        self.assertNoHit(plan, "peace.enemies")

    # -------------------------------------------------------------- manage.thresholds
    def test_manage_thresholds(self):
        self.assertHit(self.ita, "manage.thresholds", "error", "no manage key")
        plan = copy.deepcopy(self.ita)
        plan["manage"] = {}
        self.assertNoHit(plan, "manage.thresholds")
        out = self.run_lint(plan)
        self.assertEqual(out["facts"]["manage"]["stability_floor"], 0.5)
        plan["manage"] = {"off": ["stability"], "stability_floor": 0.4, "fuel_floor": 0.2}
        self.assertHit(plan, "manage.thresholds", "warning", "rule stability is in manage.off")
        self.assertHit(plan, "manage.thresholds", "warning", "not run by the executor yet (W3-10)")

    # -------------------------------------------------------------- predicates.unreadable
    def test_goal_on_an_always_unknown_atom_warns(self):
        plan = copy.deepcopy(self.ita)
        plan["goal"] = {"predicates": ["mission_days:ITA_missiolini_conquer_ethiopia > 0",
                                       {"any": ["vp_share:ETH < 0.2", "mission_days:ITA_missiolini_conquer_ethiopia > 0"]}],
                        "deadline": "1936.12.22"}
        plan["checkpoints"] = [{"id": "esc", "by": "1936.06.01", "must": ["var:ITA_escalation < 4"]}]
        f = self.assertHit(plan, "predicates.unreadable", "warning", "goal.predicates[0]")
        self.assertIn("mission_days", f["message"])
        self.assertHit(plan, "predicates.unreadable", "warning", "checkpoints[0].must[0]")
        self.assertFalse(self.hits(plan, "predicates.unreadable", None, "goal.predicates[1]"))  # any[] with a readable atom

    # -------------------------------------------------------------- conquest.mandatory, peaceful route
    def test_goal_state_of_a_country_not_fought_is_a_warning(self):
        plan = copy.deepcopy(self.ita)
        plan["goal"] = dict(plan.get("goal") or {}, predicates=["capitulated:ETH", "state_controller:16=ITA"])
        f = self.assertHit(plan, "conquest.mandatory", "warning", "vp_targets")
        self.assertIn("does not fight", f["message"])
        self.assertFalse([x for x in self.hits(plan, "conquest.mandatory", "error", "vp_targets")
                          if "state 16" in x["message"]])

    # -------------------------------------------------------------- research.idle
    def test_idle_research_slots_with_the_fallback_off_fail(self):
        plan = copy.deepcopy(self.ita)
        first = plan["research_path"][0]
        first["techs"] = first["techs"][:2]
        plan["research_fallback"] = False
        f = self.assertHit(plan, "research.idle", "error", "research_path")
        self.assertIn("nothing planned", f["message"])
        plan["research_fallback"] = True
        f = self.assertHit(plan, "research.idle", "error", "research_path")
        self.assertIn("safety net", f["message"])
        self.assertFalse(self.hits(self.ita, "research.idle", None, "research_path"))

    # -------------------------------------------------------------- conquest.skipped_near
    def test_near_vp_province_left_untaken_warns(self):
        plan = copy.deepcopy(self.ita)
        plan["vp_targets"] = [e for e in plan["vp_targets"] if e.get("province") != 1984]
        f = self.assertHit(plan, "conquest.skipped_near", "warning", "province 1984")
        self.assertIn("never targeted", f["message"])
        self.assertFalse(self.hits(self.ita, "conquest.skipped_near", None, "province 1984"))

    # -------------------------------------------------------------- production remove / replace
    def test_production_remove(self):
        plan = copy.deepcopy(self.ita)
        plan["production"].append({"equipment": "infantry_equipment_1", "remove": True, "from": "1936.06.01"})
        plan["production"].append({"equipment": "support_equipment_1", "replace": "infantry_equipment_1",
                                   "from": "1936.06.01"})
        # a land-line removal is run by the executor now: no warning; a replace still warns
        self.assertFalse(self.hits(plan, "production.remove", None, "production[4]"))
        self.assertHit(plan, "production.remove", "warning", "production[5]")
        naval = copy.deepcopy(self.ita)
        naval["production"].append({"equipment": "ship_hull_light_1", "remove": True})
        last = "production[%d]" % (len(naval["production"]) - 1)
        self.assertFalse(self.hits(naval, "production.remove", None, last))
        # the start Maestrale lines hold destroyers 60-70% built: production.in_progress
        self.assertHit(naval, "production.in_progress", "error", last)
        lt = lint_plan.Lint(plan, self.game)
        lines = lt.active_lines(derive.day("1936.07.01"))
        self.assertEqual(lines, {"artillery_equipment_1": 4, "infantry_equipment_1": 2, "motorized_equipment_1": 5})

    # -------------------------------------------------------------- wave-2 executor keys
    def test_army_group_marshals_and_navy_admirals(self):
        plan = copy.deepcopy(self.ita)
        self.assertNoHit(plan, "army.generals")  # Graziani and Badoglio are field marshals of ITA
        plan["army_group"] = [{"id": "ao", "armies": ["North strike (Army 1)", "North guard"],
                               "marshal": ["Nobody Atall", "Ettore Bastico"]},
                              {"id": "south", "armies": ["South (Army 2)", "South guard"], "marshal": "Giovanni Messe"}]
        plan["navy"] = {"posture": "coastal",
                        "fleets": [{"fleet": "Comando Navale Mar Rosso", "admiral": "Rodolfo Graziani"},
                                   {"fleet": "Comando Navale Libia", "admiral": "Inigo Campioni"}],
                        "admirals": {"Comando Navale Sicilia": "Inigo Campioni"}}
        self.assertHit(plan, "army.generals", "error", "army_group[0].marshal[0] 'Nobody Atall' is not a commander")
        f = self.assertHit(plan, "army.generals", "info", "army_group[0].marshal[1]")
        self.assertIn("promotes him", f["message"])  # a corps commander: the tool promotes him, no error
        self.assertFalse(self.hits(plan, "army.generals", "error", "army_group[0].marshal[1]"))
        self.assertHit(plan, "army.generals", "error", "Giovanni Messe commands more than one army")  # layout + group
        self.assertHit(plan, "army.generals", "error", "navy.fleets[0].admiral 'Rodolfo Graziani'")
        self.assertHit(plan, "army.generals", "error", "Inigo Campioni commands more than one fleet")
        self.assertFalse(self.hits(plan, "army.generals", "error", "navy.fleets[1].admiral 'Inigo Campioni' (I"))

    def test_hold_post_outside_the_guard_states_warns(self):
        plan = copy.deepcopy(self.ita)
        plan["hold_targets"] = [{"id": "h", "army": "South guard", "provinces": [11014]}]  # Jubaland, not 559
        f = self.assertHit(plan, "army.guards", "warning", "hold_targets[0] h")
        self.assertIn("border state(s) 559 lose that guard", f["message"])
        plan["hold_targets"] = [{"id": "h", "army": "South guard", "provinces": [2020, 1964]}]  # inside Somaliland
        self.assertNoHit(plan, "army.guards", "warning")


@unittest.skipUnless(HAVE_GAME, f"game files not found at {GAME_DIR}")
class TacticsRules(unittest.TestCase):
    """The tactics-research rules, each on a seeded copy of a hand plan (as a strategist plan, so errors
    stay errors)."""

    @classmethod
    def setUpClass(cls):
        cls.game = lint_plan.load_game(GAME_DIR)
        cls.ita = strategist(load(ITA))
        cls.eth = strategist(load(ETH))

    def hits(self, plan, rule, severity=None, text="", **kw):
        out = lint_plan.lint(plan, self.game, **kw)
        return [f for f in out["findings"] if f["rule"] == rule and (severity is None or f["severity"] == severity)
                and text in f["where"] + " " + f["message"]]

    def assertHit(self, plan, rule, severity, text="", **kw):
        found = self.hits(plan, rule, severity, text, **kw)
        self.assertTrue(found, f"no {severity} {rule} containing {text!r}: {self.hits(plan, rule, **kw)}")
        return found[0]

    def assertNoHit(self, plan, rule, severity=None, text="", **kw):
        self.assertEqual(self.hits(plan, rule, severity, text, **kw), [])

    def plan(self, **changes):
        plan = copy.deepcopy(self.ita)
        plan.update(copy.deepcopy(changes))
        return plan

    # -------------------------------------------------------------- production
    def test_mature_switch(self):
        plan = self.plan()
        plan["production"].append({"equipment": "infantry_equipment_1", "replace": "infantry_equipment_2",
                                   "from": "1937.06.01"})
        f = self.assertHit(plan, "production.mature_switch", "error", "production[4]")
        self.assertIn("keeps 30%", f["message"])  # parent/child switch of the start rifle line
        plan = self.plan()
        plan["production"] += [{"equipment": "infantry_equipment_1", "remove": True, "from": "1936.09.01"},
                               {"equipment": "infantry_equipment_2", "factories": 9, "from": "1936.09.01"}]
        f = self.assertHit(plan, "production.mature_switch", "error", "production[5] adds infantry_equipment_2")
        self.assertIn("restart at 10%", f["message"])
        # the new tier on its own factories next to the old line is the right way: no finding
        plan = self.plan()
        plan["production"].append({"equipment": "infantry_equipment_2", "factories": 3, "from": "1936.09.01"})
        self.assertNoHit(plan, "production.mature_switch")

    def test_cull(self):
        f = self.assertHit(self.ita, "production.cull", "error", "start line light_tank_chassis_1")
        self.assertIn("L3/35", f["message"])
        plan = self.plan()
        plan["production"].append({"equipment": "light_tank_chassis_1", "remove": True})
        self.assertNoHit(plan, "production.cull", text="light_tank_chassis_1")
        # a land-only posture: the start air lines (no air 0% use) and every start naval line are culled
        plan["force_posture"] = {"reason": "land war", "phases": [{"phase": "base", "land": 100, "air": 0, "navy": 0}]}
        plan["navy"] = {"posture": "none", "reason": "no naval enemy"}
        f = self.assertHit(plan, "production.cull", "error", "gives air 0%")
        self.assertIn("small_plane_cas_airframe_1", f["message"])
        f = self.assertHit(plan, "production.cull", "error", "start line ship_hull_heavy_1")
        self.assertIn("when its unit in progress is done", f["message"])  # the Cavour hulls are 70% built
        # a coastal navy keeps the dockyard lines
        plan["navy"] = {"posture": "coastal", "reason": "escort"}
        self.assertNoHit(plan, "production.cull", text="start line ship_hull")

    def test_day0(self):
        f = self.assertHit(self.ita, "production.day0", "error", "production")
        self.assertIn("1 of the 10 free military factories", f["message"])
        self.assertIn("artillery_equipment_1 (4)", f["message"])
        plan = self.plan()
        plan["production"][0]["factories"] = 10  # infantry: 4 at the start + 6
        self.assertNoHit(plan, "production.day0")
        # a start removal frees its factory into the day-0 pool
        plan["production"].append({"equipment": "light_tank_chassis_1", "remove": True})
        f = self.assertHit(plan, "production.day0", "error", "1 freed by start removals")
        self.assertIn("1 of the 11", f["message"])

    def test_in_progress_and_naval_cap(self):
        plan = self.plan(navy={"posture": "coastal", "reason": "escort the Red Sea convoys"})
        plan["production"] += [{"equipment": "ship_hull_heavy_1", "remove": True},
                               {"equipment": "ship_hull_heavy_2", "remove": True, "from": "1936.03.01"},
                               {"equipment": "ship_hull_light_1", "remove": True, "from": "1937.01.01"},
                               {"equipment": "ship_hull_carrier_1", "remove": True},
                               {"equipment": "ship_hull_heavy_2", "factories": 8}]
        f = self.assertHit(plan, "production.in_progress", "error", "production[4]")
        self.assertIn("Conte Di Cavour Class (70% done on 2 dockyards)", f["message"])
        f = self.assertHit(plan, "production.in_progress", "error", "production[5]")
        self.assertIn("removing ship_hull_heavy_2 on 1936.03.01", f["message"])
        self.assertNoHit(plan, "production.in_progress", text="production[6]")  # the destroyers are done by then
        self.assertHit(plan, "production.in_progress", "warning", "production[7]")  # no start line: no progress known
        f = self.assertHit(plan, "production.naval_cap", "error", "production[8]")
        self.assertIn("at most 5", f["message"])

    # -------------------------------------------------------------- army
    def test_width_terrain(self):
        plan = self.plan(templates=[
            {"id": "wide", "name": "Wide", "regiments": ["infantry"] * 20, "role": "line"},
            {"id": "fourteen", "name": "Fourteen", "regiments": [["infantry"] * 3, ["infantry"] * 3, ["infantry"]],
             "role": "line"},
            {"id": "garr", "name": "Garr", "regiments": ["cavalry"], "role": "garrison"}])
        f = self.assertHit(plan, "army.width_terrain", "error", "templates[0] wide")
        self.assertIn("width 40", f["message"])
        self.assertIn("width 14 fills 0.99", f["message"])
        self.assertNoHit(plan, "army.width_terrain", text="templates[1]")
        self.assertNoHit(plan, "army.width_terrain", text="templates[2]")
        f = self.assertHit(self.eth, "army.width_terrain", "warning", "army_layout")
        self.assertIn("Mehal Sefari width 8", f["message"])
        self.assertNotIn("Chitet Sefari", f["message"])  # 12 wide fills 0.98

    # -------------------------------------------------------------- war
    def test_supply_floor(self):
        f = self.assertHit(self.ita, "war.supply_floor", "error", "war.supply_floor")
        self.assertIn("35%", f["message"])
        plan = self.plan()
        plan["war"]["supply_floor"] = 50
        self.assertNoHit(plan, "war.supply_floor")
        plan["war"]["supply_floor"] = 40
        self.assertHit(plan, "war.supply_floor", "warning", "below 50% supply")
        plan["war"]["supply_floor"] = 0.5  # a fraction reads the same
        plan["war_by_phase"]["P2_tigray"]["supply_floor"] = 20
        self.assertHit(plan, "war.supply_floor", "error", "war_by_phase.P2_tigray.supply_floor")

    def schema_with(self, **props):
        schema = copy.deepcopy(lint_plan.load_schema())
        schema["definitions"]["war"]["properties"].update(props)
        return schema

    def test_planning_wait(self):
        self.assertHit(self.ita, "war.planning_wait", "warning", "war.direct_moves")
        plan = self.plan()
        plan["war"]["direct_moves"] = False
        # the executor's planning wait (default on, up to 15 days) is enough
        self.assertNoHit(plan, "war.planning_wait")
        plan["war"]["planning_max_days"] = 6
        self.assertHit(plan, "war.planning_wait", "warning", "12% planning")
        plan["war"]["planning_wait"] = False
        self.assertHit(plan, "war.planning_wait", "error", "war.planning_wait false")
        del plan["war"]["planning_wait"], plan["war"]["planning_max_days"]
        # a schema without the executor keys: an info line, then a named days key
        schema = copy.deepcopy(lint_plan.load_schema())
        for k in [k for k in schema["definitions"]["war"]["properties"] if k.startswith("planning_")]:
            del schema["definitions"]["war"]["properties"][k]
        self.assertHit(plan, "war.planning_wait", "info", "no planning-wait key yet", schema=schema)
        schema["definitions"]["war"]["properties"]["plan_days"] = {"type": "integer", "minimum": 0}
        self.assertHit(plan, "war.planning_wait", "error", "war.plan_days", schema=schema)
        plan["war"]["plan_days"] = 6
        self.assertHit(plan, "war.planning_wait", "warning", "12% planning", schema=schema)
        plan["war"]["plan_days"] = 15
        self.assertNoHit(plan, "war.planning_wait", schema=schema)

    def test_pocket_threshold(self):
        plan = self.plan()
        plan["war_by_phase"]["P2_tigray"]["avoid_pockets"] = False
        self.assertHit(plan, "war.pocket_threshold", "error", "war_by_phase.P2_tigray.avoid_pockets")
        self.assertNoHit(self.ita, "war.pocket_threshold")  # the executor default is 50%
        schema = self.schema_with(pocket_share={"type": "number", "description": "hostile share (default 0.7)"})
        plan = self.plan()
        self.assertHit(plan, "war.pocket_threshold", "warning", "the default 70%", schema=schema)
        plan["war"]["pocket_share"] = 70
        self.assertHit(plan, "war.pocket_threshold", "error", "70% hostile share", schema=schema)
        plan["war"]["pocket_share"] = 0.5
        self.assertNoHit(plan, "war.pocket_threshold", schema=schema)

    # -------------------------------------------------------------- politics
    def test_unneeded_conscription(self):
        plan = self.plan()
        plan["politics"].append({"id": "pol_sbr", "category": "government", "slot": 0,
                                 "idea": "service_by_requirement", "cost": 150})
        f = self.assertHit(plan, "politics.unneeded_conscription", "error", "politics[1] service_by_requirement")
        self.assertIn("-10% factory output", f["message"])
        plan["manage"] = {}
        f = self.assertHit(plan, "politics.unneeded_conscription", "warning", "is unset")
        self.assertIn("scraping_the_barrel", f["message"])
        plan["manage"] = {"conscription_laws": ["limited_conscription", "extensive_conscription"]}
        self.assertNoHit(plan, "politics.unneeded_conscription", "warning")
        self.assertNoHit(self.ita, "politics.unneeded_conscription")

    def test_pp_budget(self):
        self.assertNoHit(self.ita, "politics.pp_budget", "error")  # 150 PP by late May at 1.04 a day
        # after its one law the plan leaves the power idle for months: no domain without a default use
        f = self.assertHit(self.ita, "politics.pp_budget", "warning", "politics")
        self.assertIn("no planned buy for", f["message"])
        self.assertIn("politics_auto.order", f["message"])
        plan = self.plan()
        plan["politics_auto"] = {"order": [{"category": "government", "slot": 3, "ideas": ["ITA_a"]}]}
        self.assertNoHit(plan, "politics.pp_budget", "warning", "no planned buy")
        self.assertHit(plan, "politics.pp_budget", "info", "standing policy")
        plan["politics_auto"]["enabled"] = False
        self.assertHit(plan, "politics.pp_budget", "warning", "politics_auto is off")
        plan = self.plan()
        plan["politics"][0]["when"] = "1936.02.01"
        for k, idea in enumerate(["free_trade", "service_by_requirement", "tot_economic_mobilisation"]):
            plan["politics"].append({"id": f"pol_{k}", "category": "government", "slot": k + 3, "idea": idea,
                                     "cost": 150})
        f = self.assertHit(plan, "politics.pp_budget", "error", "politics[2]")
        self.assertIn("1.04 PP a day with a focus running", f["message"])
        self.assertHit(plan, "politics.pp_budget", "error", "politics[3]")
        self.assertNoHit(plan, "politics.pp_budget", "error", "politics[1]")  # 300 PP by the end of the year
        f = self.assertHit(plan, "politics.pp_budget", "warning", "politics[0]")
        self.assertIn("114 days after its date", f["message"])

    # -------------------------------------------------------------- research
    def test_research_ahead(self):
        plan = self.plan()
        plan["research_path"].insert(0, {"techs": ["infantry_weapons2"]})  # a 1938 tech from day one
        f = self.assertHit(plan, "research.ahead", "error", "research_path[0]")
        self.assertIn("2.0 year(s) ahead, x5.0 the time", f["message"])
        self.assertIn("in-year techs of the same fields stay unplanned", f["message"])
        plan["research_bonuses"] = {"infantry_weapons": ["infantry_weapons2"]}
        self.assertNoHit(plan, "research.ahead")
        self.assertNoHit(self.ita, "research.ahead")

    # -------------------------------------------------------------- navy
    def test_invasion_prep_and_sea_control(self):
        plan = self.plan(invasions=[{"id": "alb", "from_state": 849, "target_state": 44,
                                     "armies": ["North strike (Army 1)"]}])  # Puglia to Central Albania
        f = self.assertHit(plan, "navy.invasion_prep", "error", "prepare_days 14 (the default)")
        self.assertIn("needs 50 days", f["message"])  # 60 less mtg_transport's 10
        f = self.assertHit(plan, "navy.invasion_prep", "error", "8 divisions")
        self.assertIn("at most 6", f["message"])  # 4 plus mtg_transport's 2
        f = self.assertHit(plan, "navy.sea_control", "error", "invasions[0] alb")
        self.assertIn("Adriatic Sea", f["message"])
        plan["invasions"][0]["prepare_days"] = 50
        plan["army_layout"][0]["count"] = 6
        plan["navy"] = {"posture": "coastal", "fleets": [
            {"fleet": "Comando Navale Adriatico", "missions": [{"mission": "patrol", "regions": ["Adriatic Sea"]}]}]}
        self.assertNoHit(plan, "navy.invasion_prep")
        self.assertNoHit(plan, "navy.sea_control")

    def test_strike_without_patrol(self):
        plan = self.plan(navy={"posture": "coastal", "fleets": [
            {"fleet": "Comando Navale Adriatico", "missions": [
                {"mission": "strike_force", "regions": ["Adriatic Sea", "Central Mediterranean"]},
                {"mission": "patrol", "regions": ["Adriatic Sea"]}]}]})
        f = self.assertHit(plan, "navy.strike_without_patrol", "warning", "navy.fleets[0].missions[0]")
        self.assertIn("Central Mediterranean", f["message"])
        self.assertNotIn("Adriatic", f["message"])
        plan["navy"]["fleets"][0]["missions"][1]["regions"].append("central mediterranean")
        self.assertNoHit(plan, "navy.strike_without_patrol")

    # -------------------------------------------------------------- peace
    def test_peace_order(self):
        plan = self.plan(peace={"take": [16, 271]})  # Ile de France is never taken, Addis Abeba is
        f = self.assertHit(plan, "peace.order", "info", "peace.take")
        self.assertIn("state 16 is ranked ahead of state 271", f["message"])
        plan["peace"]["take"] = [271, 16]
        self.assertNoHit(plan, "peace.order")

    # -------------------------------------------------------------- construction
    def test_war_civs(self):
        plan = self.plan()
        plan["construction_plan"].append({"id": "cp99_civ", "building": "industrial_complex", "state": 2,
                                          "when": "1936.06.01"})
        f = self.assertHit(plan, "construction.war_civs", "warning", "cp99_civ")
        self.assertIn("214 days before the plan horizon", f["message"])
        self.assertNoHit(self.ita, "construction.war_civs")

    def test_hand_plans_report_the_new_rules_as_warnings(self):
        plan = self.plan()
        plan["_meta"] = {"version": 1, "source": "hand"}
        out = lint_plan.lint(plan, self.game)
        new = {"production.cull", "production.day0", "war.supply_floor"}
        self.assertTrue([f for f in out["findings"] if f["rule"] in new])
        self.assertFalse([f for f in out["findings"] if f["rule"] in new and f["severity"] == "error"])


@unittest.skipUnless(HAVE_GAME, f"game files not found at {GAME_DIR}")
class DeriveReaders(unittest.TestCase):
    """The small readers derive.py gained for the coverage-build rules, against the install."""

    @classmethod
    def setUpClass(cls):
        cls.game = lint_plan.load_game(GAME_DIR)

    def test_terrain_widths_match_the_audit(self):
        w = derive.terrain_widths(self.game)
        self.assertEqual({k: (v["width"], v["support"]) for k, v in w.items()},
                         {"plains": (70, 35), "forest": (60, 30), "hills": (70, 35), "mountain": (50, 25),
                          "urban": (80, 40), "jungle": (60, 30), "marsh": (50, 25), "desert": (70, 35)})

    def test_units_and_widths(self):
        u = derive.unit_types(self.game)
        self.assertEqual(u["infantry"]["width"], 2)
        self.assertTrue(u["engineer"]["support"])
        self.assertTrue(u["fire_support"]["regimental"])
        self.assertIn("tech_engineers", derive.subunit_unlocks(self.game)["engineer"])
        self.assertEqual(derive.template_width(self.game, [["infantry"] * 3, ["artillery_brigade"]]), 9)
        self.assertEqual(derive.equipment_domain(self.game, "small_plane_cas_airframe_1"), "air")
        self.assertEqual(derive.equipment_domain(self.game, "convoy_1"), "navy")

    def test_width_fill_with_over_width_and_stacking(self):
        g = self.game
        self.assertEqual(derive.width_fill(g, 35, 50, 1), 0.7)  # a second 35 would be 40% over: it cannot join
        self.assertEqual(derive.width_fill(g, 14, 50, 1), 0.9856)  # 4 x 14 = 56, 12% over: 1.12 x 0.88
        self.assertEqual(derive.width_fill(g, 25, 75, 2), 1.0)
        self.assertGreater(derive.terrain_fill(g, 14, "mountain"), 0.98)
        self.assertLess(derive.terrain_fill(g, 40, "forest"), 0.8)
        mix = {"mountain": 0.4, "hills": 0.3, "plains": 0.3}
        scores = derive.width_scores(g, mix)
        self.assertGreater(scores[14], scores[20])
        self.assertGreater(scores[18], scores[34])

    def test_equipment_and_tech_facts(self):
        g = self.game
        self.assertEqual(derive.equipment_tier_link(g, "infantry_equipment_1", "infantry_equipment_2"), "parent")
        self.assertEqual(derive.equipment_tier_link(g, "infantry_equipment_0", "infantry_equipment_2"), "family")
        self.assertIsNone(derive.equipment_tier_link(g, "infantry_equipment_1", "artillery_equipment_1"))
        self.assertEqual(derive.equipment_family(g, "small_plane_cas_airframe_1"), "small_plane_cas_airframe")
        self.assertEqual(derive.equipment_rows(g)["ship_hull_heavy_2"]["parent"], "ship_hull_heavy_1")
        mods = derive.tech_modifiers(g)
        self.assertEqual(mods["mtg_transport"]["naval_invasion_division_cap"], 2)
        self.assertEqual(mods["mtg_transport"]["naval_invasion_prep_days"], -10)
        self.assertIn("dispersed_industry", derive.tech_meta(g)["concentrated_industry"]["xor"])

    def test_start_production_lines(self):
        from hoi4facts.validate import Report, _world, resolve_meta
        plan = load(ITA)
        w = _world(self.game, resolve_meta(plan, self.game, Report()))
        rows = derive.start_production(w, "ITA")
        cavour = [r for r in rows if r["version"] == "Conte Di Cavour Class"]
        self.assertEqual([(r["factories"], r["progress"]) for r in cavour], [(2, 0.7), (2, 0.7)])
        self.assertIn("light_tank_chassis_1", {r["type"] for r in rows})

    def test_construction_estimate(self):
        from hoi4facts.script import Ctx
        ita = Ctx("ITA", load(ITA)["_validator"]["dlcs"] + self.game.integrated_dlcs())
        civ = derive.free_civilian_factories(self.game, ita, "ITA")
        self.assertEqual((civ["civs"], civ["mils"], civ["free"]), (21, 20, 10))
        self.assertEqual(derive.construction_days(self.game, "supply_node", 10, 2), 286)
        self.assertEqual(derive.construction_days(self.game, "supply_node", 30, 0), 267)  # cap of 15 civs
        self.assertEqual(derive.construction_constants(self.game)["project_cap"], 15)


if __name__ == "__main__":
    unittest.main()
