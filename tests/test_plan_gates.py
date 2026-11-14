"""Focus gates under a plan's own conditions (scripts/hoi4facts/plan_gates.py).

A focus the plan lists must be able to start when the plan gets to it: prerequisites done, no exclusive
focus done, and its `available` trigger not closed by what the plan itself says about that day (the date,
the war, the focuses done, the start flags). The slot walk finds the stretches where nothing listed can
start. Synthetic trees run everywhere; the Italy test reads the game files.
"""
import copy
import json
import os
import sys
import unittest

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.join(ROOT, "scripts"))

from hoi4facts import lint_plan, plan_gates  # noqa: E402
from hoi4facts.script import parse  # noqa: E402


def facts(war=None, date="1936.1.1", **kw):
    return plan_gates.Facts("AAA", kw.pop("dlcs", ()), date, war, **kw)


class Evaluate(unittest.TestCase):
    def test_war_state(self):
        self.assertIs(plan_gates.evaluate(parse("has_war = no"), facts(war=True)), False)
        self.assertIs(plan_gates.evaluate(parse("has_war = no"), facts(war=False)), True)
        self.assertIsNone(plan_gates.evaluate(parse("has_war = no"), facts(war=None)))
        self.assertIs(plan_gates.evaluate(parse("has_war_with = BBB"), facts(war=True, enemies=["BBB"])), True)
        self.assertIs(plan_gates.evaluate(parse("has_war_with = BBB"), facts(war=False)), False)
        self.assertIsNone(plan_gates.evaluate(parse("has_war_with = CCC"), facts(war=True, enemies=["BBB"])))
        self.assertIs(plan_gates.evaluate(parse("NOT = { has_war_with = BBB }"), facts(war=True, enemies=["BBB"])), False)

    def test_wrappers_flags_and_focuses(self):
        never = parse("custom_trigger_tooltip = { tooltip = some_tt always = no }")
        self.assertIs(plan_gates.evaluate(never, facts()), False)
        flag = parse("NOT = { has_global_flag = the_war_flag }")
        self.assertIs(plan_gates.evaluate(flag, facts(flags=["the_war_flag"])), False)
        self.assertIsNone(plan_gates.evaluate(flag, facts()))  # not set at the start: may be set later
        done = parse("has_completed_focus = f_one")
        self.assertIs(plan_gates.evaluate(done, facts(focuses=["f_one"])), True)
        self.assertIs(plan_gates.evaluate(done, facts()), False)
        # the country's own land at the start is its own; land of others may or may not be taken
        own = parse("OR = { controls_state = 2 controls_state = 9 }")
        self.assertIs(plan_gates.evaluate(own, facts(states=[2])), True)
        self.assertIsNone(plan_gates.evaluate(parse("controls_state = 9"), facts(states=[2])))
        # what the plan cannot know stays undecided, and an OR with one open branch is open
        self.assertIsNone(plan_gates.evaluate(parse("any_owned_state = { is_core_of = BBB }"), facts()))
        self.assertIs(plan_gates.evaluate(parse("OR = { has_war = no has_war_with = BBB }"),
                                          facts(war=True, enemies=["BBB"])), True)
        self.assertIs(plan_gates.evaluate(parse("date > 1936.6.1"), facts(date="1936.3.1")), False)

    def test_blocking_names_the_closed_line(self):
        gate = parse("custom_trigger_tooltip = { tooltip = tt has_war = no } has_completed_focus = f_one")
        why = plan_gates.blocking(gate, facts(war=True, focuses=["f_one"]))
        self.assertEqual(why, ["has_war = no"])


TREE = {
    "open_a": {"prereq": [], "exclusive": set()},
    "open_b": {"prereq": [{"open_a"}], "exclusive": set()},
    "peace_1": {"prereq": [], "exclusive": set()},
    "peace_2": {"prereq": [], "exclusive": set()},
    "scripted": {"prereq": [], "exclusive": set()},
    "holds_land": {"prereq": [], "exclusive": set()},
    "war_time": {"prereq": [], "exclusive": set()},
}
META = {
    "open_a": {"available": None, "cost": 5.0}, "open_b": {"available": None, "cost": 10.0},
    "peace_1": {"available": parse("has_war = no"), "cost": 10.0},
    "peace_2": {"available": parse("NOT = { has_global_flag = the_war_flag }"), "cost": 10.0},
    "scripted": {"available": parse("custom_trigger_tooltip = { tooltip = tt always = no }"), "cost": 10.0},
    "holds_land": {"available": parse("any_controlled_state = { is_core_of = BBB }"), "cost": 10.0},
    "war_time": {"available": parse("has_war = yes"), "cost": 10.0},
}
WAR_END = 300


def at_war_until(day, done):
    war = day < WAR_END
    return plan_gates.Facts("AAA", (), "1936.1.1", war, ["BBB"] if war else (), done,
                            ["the_war_flag"] if war else ())


def walk(path, fallback, horizon=400):
    return plan_gates.focus_schedule(path, fallback, TREE, META, 7.0, 0, horizon, at_war_until, changes=[WAR_END])


class FocusSlot(unittest.TestCase):
    def test_fallbacks_that_all_need_peace_leave_the_slot_idle_in_the_war(self):
        # the path ends on day 105; every fallback needs the war over
        s = walk(["open_a", "open_b"], ["peace_1", "peace_2", "scripted"])
        self.assertEqual([(r["focus"], r["start"], r["end"]) for r in s["rows"][:2]],
                         [("open_a", 0, 35.0), ("open_b", 35.0, 105.0)])
        gap = s["idle"][0]
        self.assertEqual((gap["from"], gap["to"], gap["exhausted"]), (105.0, WAR_END, False))
        why = {b["focus"]: b["why"] for b in gap["blocked"]}
        self.assertEqual(why["peace_1"], "not available: has_war = no")
        self.assertIn("the_war_flag", why["peace_2"])
        self.assertIn("always = no", why["scripted"])
        # once the war is over the slot fills again, and the scripted focus never starts
        self.assertEqual([r["focus"] for r in s["rows"][2:]], ["peace_1", "peace_2"])
        self.assertEqual(s["rows"][2]["start"], WAR_END)
        self.assertEqual([n["focus"] for n in s["never"]], ["scripted"])

    def test_a_focus_that_holds_at_war_keeps_the_slot_busy(self):
        s = walk(["open_a", "open_b"], ["peace_1", "war_time"])
        self.assertEqual([r["focus"] for r in s["rows"][:3]], ["open_a", "open_b", "war_time"])
        self.assertEqual([g for g in s["idle"] if g["to"] <= WAR_END and not g["exhausted"]][0]["from"], 175.0)

    def test_blocked_path_focus_waits_and_later_ones_go_first(self):
        s = walk(["peace_1", "open_a", "open_b"], [], horizon=WAR_END + 100)
        self.assertEqual([r["focus"] for r in s["rows"]], ["open_a", "open_b", "peace_1"])
        self.assertEqual(s["rows"][2]["start"], WAR_END)
        self.assertEqual(s["never"], [])

    def test_undecided_gate_is_an_assumption(self):
        s = walk(["holds_land"], [], horizon=100)
        self.assertIsNone(s["rows"][0]["gate"])
        self.assertTrue(s["idle"][0]["exhausted"])  # the list ran out on day 70

    def test_focus_behind_a_scripted_one_waits(self):
        # after_win needs `scripted`, which only the game completes; later needs after_win
        tree = copy.deepcopy(TREE)
        tree["after_win"] = {"prereq": [{"scripted", "peace_1"}], "exclusive": set()}
        tree["later"] = {"prereq": [{"after_win"}], "exclusive": set()}
        meta = dict(META, after_win={"available": None, "cost": 5.0}, later={"available": None, "cost": 5.0})
        self.assertTrue(plan_gates.scripted("scripted", meta))
        self.assertFalse(plan_gates.scripted("peace_1", meta))
        s = plan_gates.focus_schedule(["open_a"], ["after_win", "later"], tree, meta, 7.0, 0, 100, at_war_until)
        self.assertEqual(s["never"], [])  # not an error: the game may complete the prerequisite
        self.assertEqual([(w["focus"], w["on"]) for w in s["waiting"]],
                         [("after_win", ["scripted"]), ("later", ["after_win"])])
        self.assertEqual(s["idle"][0]["from"], 35.0)  # and they are no cover for the slot meanwhile

    def test_prerequisites_and_exclusions(self):
        tree = copy.deepcopy(TREE)
        tree["open_b"]["exclusive"] = {"open_a"}
        s = plan_gates.focus_schedule(["open_a", "open_b"], [], tree, META, 7.0, 0, 200, at_war_until)
        self.assertEqual(s["never"][0]["why"], "mutually exclusive with open_a")
        s = walk(["open_b"], [], horizon=100)
        self.assertEqual(s["never"][0]["why"], "needs open_a first")


GAME_DIR = os.environ.get("HOI4_DIR", lint_plan.DEFAULT_GAME_DIR)
HAVE_GAME = os.path.isfile(os.path.join(GAME_DIR, "map", "provinces.bmp"))


@unittest.skipUnless(HAVE_GAME, f"game files not found at {GAME_DIR}")
class ItalyAtWar(unittest.TestCase):
    """Italy starts 1936 at war with Ethiopia; three of its focuses wait for that war to end."""

    @classmethod
    def setUpClass(cls):
        cls.game = lint_plan.load_game(GAME_DIR)
        with open(os.path.join(ROOT, "skills", "hoi4-campaign-playbook", "plans",
                               "italy-ethiopia-1936.plan.json"), encoding="utf-8") as fh:
            cls.plan = json.load(fh)
        cls.plan["_meta"] = {"version": 1, "source": "strategist"}

    def rule(self, plan, rule):
        return [f for f in lint_plan.lint(plan, self.game)["findings"] if f["rule"] == rule]

    def test_war_blocked_fallbacks_are_found(self):
        plan = copy.deepcopy(self.plan)
        plan["focus_path"] = plan["focus_path"][:3]  # done in mid April, the war runs to the autumn
        plan["focus_fallback"] = ["ITA_solid_progress", "ITA_ministry_of_italian_africa"]
        never = self.rule(plan, "focus.available")
        self.assertEqual(len(never), 1)
        self.assertIn("ITA_solid_progress cannot start", never[0]["message"])
        self.assertIn("always = no", never[0]["message"])
        idle = self.rule(plan, "focus.idle")
        self.assertEqual({f["severity"] for f in idle}, {"error"})
        self.assertIn("while at war", idle[0]["message"])
        self.assertIn("second_italo_ethiopian_war_flag", idle[0]["message"])
        # with focuses that can start at war listed, the slot stays busy through it
        plan["focus_path"] = list(self.plan["focus_path"])
        self.assertFalse([f for f in self.rule(plan, "focus.idle") if "while at war" in f["message"]])

    def test_gate_on_land_not_yet_held_is_unconfirmed(self):
        plan = copy.deepcopy(self.plan)
        plan["focus_path"] = plan["focus_path"][:3] + ["ITA_ministry_of_italian_africa", "ITA_develop_ethiopia"]
        plan["focus_fallback"] = []
        lt = lint_plan.Lint(plan, self.game)
        lt.run()
        sched = {r["focus"]: r for r in lt.facts["focus"]["schedule"]}
        # the highways focus asks for control of Italian home states: decided from the start ownership
        self.assertIs(sched["ITA_italian_highways_bba"]["gate"], True)
        # the ministry waits for the war to end; Develop Ethiopia then asks for a fully controlled Ethiopian
        # core state, which the plan cannot decide, and nothing certain stands behind it
        self.assertGreater(sched["ITA_ministry_of_italian_africa"]["start"], "1936.06.01")
        self.assertIsNone(sched["ITA_develop_ethiopia"]["gate"])
        hit = [f for f in lt.findings if f["rule"] == "focus.unconfirmed" and "ITA_develop_ethiopia" in f["message"]]
        self.assertEqual([f["severity"] for f in hit], ["warning"])

if __name__ == "__main__":
    unittest.main()
