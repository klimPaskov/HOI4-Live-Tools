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


class Lessons(unittest.TestCase):
    def test_ledger_lines_name_real_rules(self):
        rules = set(re.findall(r"^  ([a-z]+\.[a-z_]+)\s", lint_plan.__doc__, re.M))
        self.assertIn("construction.front", rules)
        with open(LESSONS, encoding="utf-8") as fh:
            rows = [json.loads(line) for line in fh if line.strip()]
        self.assertEqual(len(rows), 12)  # design table 4.4 plus L12_early_hub_cost
        ids = set()
        for r in rows:
            self.assertEqual(set(r), {"id", "failure", "runs", "prevention", "lint_rule"})
            self.assertNotIn(r["id"], ids)
            ids.add(r["id"])
            self.assertTrue(r["runs"] and isinstance(r["runs"], list))
            for rule in r["lint_rule"]:
                self.assertIn(rule, rules, f"{r['id']}: {rule} is not a lint rule")


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
            ("schema.player_call", "army_group"),
            ("schema.player_call", "retake_rules"),
            ("win.goal", "goal"),  # only the deprecated surrender_goal
        ])
        _, warnings = self.findings(load(ITA), "warning")
        # the coverage-build rules report as warnings on a hand plan (_meta.source "hand")
        gaps = [("coverage.checklist", f"_coverage.c{n}") for n in (3, 5, 6, 7, 9, 14, 15, 17, 21, 22, 23, 24, 25, 27)]
        self.assertEqual(warnings, sorted(gaps + [
            ("manage.thresholds", "manage"),
            ("navy.overseas", "navy"),  # East Africa is supplied by sea, no convoy or port plan
            ("navy.required", "navy"),
            ("occupation.required", "occupation"),  # Eritrea, Somaliland, Jubaland resist from day one
            ("peace.enemies", "peace"),
            ("politics.law_gates", "politics[0] war_economy"),  # war support > 0.5 gate, no `when`
            ("predicates.unreadable", "phases[5].exit"),  # capitulated:ETH has no read yet
            ("research.throughput", "research_path[0]"),
            ("research.throughput", "research_path[1]"),
            ("supply.port_convoys", "production"),
        ]))
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

    def test_ethiopia(self):
        out, errors = self.findings(load(ETH), "error")
        self.assertEqual(errors, [
            ("schema.player_call", "army_group"),
            ("schema.player_call", "hold_targets"),
            ("schema.player_call", "retake_rules"),
            ("win.goal", "goal"),  # only the deprecated win_condition
        ])
        _, warnings = self.findings(load(ETH), "warning")
        gaps = [("coverage.checklist", f"_coverage.c{n}") for n in (3, 5, 6, 9, 12, 14, 15, 17, 22, 23, 25)]
        self.assertEqual(warnings, sorted(gaps + [
            ("manage.thresholds", "manage"),
            ("occupation.required", "occupation"),  # Eritrea, Somaliland, Jubaland are conquered
            ("peace.enemies", "peace"),
            ("politics.law_gates", "politics[0] partial_economic_mobilisation"),
            ("research.throughput", "research_path[0]"),
            ("research.throughput", "research_path[1]"),
        ]))
        self.assertEqual(out["facts"]["overseas_states"], [])  # landlocked: no navy rules
        # the decision ETH_defeat_italy gives the states the goal must hold
        self.assertEqual(out["facts"]["goal_states"], {"550": "full", "559": "full", "844": "full"})
        self.assertEqual(out["facts"]["border_states"], [835, 836, 837, 841, 842])

    def test_cli_exit_status_and_json(self):
        buf = io.StringIO()
        with redirect_stdout(buf):
            code = lint_plan.main([os.path.join(PLANS, ETH), "--game", GAME_DIR, "--json"])
        self.assertEqual(code, 1)
        self.assertEqual(json.loads(buf.getvalue())["errors"], 4)


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
        rules = set(re.findall(r"^  ([a-z]+\.[a-z_]+)\s", lint_plan.__doc__, re.M))
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
        f = self.assertHit(plan, "templates.width", "warning", "templates[0] tiny")
        self.assertIn("mountain", f["message"])  # the highland terrain mix of the theatre
        self.assertFalse([f for f in self.hits(plan, "templates.width") if "garr" in f["where"]])
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
        plan["goal"] = {"predicates": ["capitulated:ETH", {"any": ["vp_share:ETH < 0.2", "capitulated:ETH"]}],
                        "deadline": "1936.12.22"}
        plan["checkpoints"] = [{"id": "esc", "by": "1936.06.01", "must": ["var:ITA_escalation < 4"]}]
        f = self.assertHit(plan, "predicates.unreadable", "warning", "goal.predicates[0]")
        self.assertIn("capitulated:ETH", f["message"])
        self.assertHit(plan, "predicates.unreadable", "warning", "checkpoints[0].must[0]")
        self.assertFalse(self.hits(plan, "predicates.unreadable", None, "goal.predicates[1]"))  # any[] with a readable atom

    # -------------------------------------------------------------- production remove / replace
    def test_production_remove(self):
        plan = copy.deepcopy(self.ita)
        plan["production"].append({"equipment": "infantry_equipment_1", "remove": True, "from": "1936.06.01"})
        plan["production"].append({"equipment": "support_equipment_1", "replace": "infantry_equipment_1",
                                   "from": "1936.06.01"})
        self.assertHit(plan, "production.remove", "warning", "production[4]")
        self.assertHit(plan, "production.remove", "warning", "production[5]")
        lt = lint_plan.Lint(plan, self.game)
        lines = lt.active_lines(derive.day("1936.07.01"))
        self.assertEqual(lines, {"artillery_equipment_1": 4, "infantry_equipment_1": 2, "motorized_equipment_1": 5})


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
