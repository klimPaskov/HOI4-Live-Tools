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
        self.assertEqual(len(rows), 11)  # design table 4.4
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
        self.assertEqual(rows[("executor", "coverage")], 1.0)

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
            ("construction.railway", "construction_plan[7] cp08_rail_way_843"),  # 7944 not on Dessie's route
            ("construction.railway", "construction_plan[8] cp09_rail_way_835"),  # whole line queued at Wal Wal
            ("schema.player_call", "air_plan"),
            ("schema.player_call", "army_group"),
            ("schema.player_call", "retake_rules"),
            ("win.goal", "goal"),  # only the deprecated surrender_goal
        ])
        _, warnings = self.findings(load(ITA), "warning")
        self.assertEqual(warnings, [("research.throughput", "research_path[0]"),
                                    ("research.throughput", "research_path[1]")])
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
        self.assertEqual(warnings, [("research.throughput", "research_path[0]"),
                                    ("research.throughput", "research_path[1]")])
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


if __name__ == "__main__":
    unittest.main()
