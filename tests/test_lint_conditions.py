"""scripts/hoi4facts/conditions.py and lint_conditions.py: planned items against the game's own conditions.

The evaluator tests run without the game (a stub timeline); the lint tests read the installed game and are
skipped without it."""

import copy
import json
import os
import sys
import unittest

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.join(ROOT, "scripts"))

from hoi4facts import conditions as cond, derive, lint_plan, plan_gates  # noqa: E402
from hoi4facts.script import parse  # noqa: E402

PLANS = os.path.join(ROOT, "skills", "hoi4-campaign-playbook", "plans")
GAME_DIR = os.environ.get("HOI4_DIR", lint_plan.DEFAULT_GAME_DIR)
HAVE_GAME = os.path.isfile(os.path.join(GAME_DIR, "map", "provinces.bmp"))


class StubTimeline:
    """What a plan says, without the game: peace until day 1000, 30 divisions growing 1 a week."""

    def __init__(self):
        self.tag, self.dlcs, self.game = "GER", ("Gotterdammerung",), None
        self.government, self.subject, self.war_why = "fascism", False, "diplomacy[0]"
        self.start = derive.day("1936.1.1")

    def at_war(self, day):
        return day >= self.start + 1000

    def ideas(self, day):
        return {"free_trade"}

    def owned(self, day):
        return {64, 65} | ({74} if day >= self.start + 500 else set())

    def army_at(self, day):
        n = 30 + max(0, day - self.start) / 7.0
        return n, n * 10000

    def first_day(self, divisions=None, manpower=None):
        if manpower is not None:
            return self.start + int((manpower / 10000 - 30) * 7)
        return self.start + int((divisions - 30) * 7)

    def start_army(self, tag):
        return (6, 30000)

    def manpower_per_division(self):
        return 10000

    def facts(self, day, done):
        return plan_gates.Facts(self.tag, self.dlcs, derive.fmt(derive.date_of(day)), self.at_war(day), (), done,
                                set(), self.owned(day), self.government)


def ev(text, day_offset=0, done=()):
    tl = StubTimeline()
    return cond.evaluate(parse(text), cond.Env(tl, tl.start + day_offset, done))


class Evaluate(unittest.TestCase):
    def test_plan_timeline_decides_war_land_ideas_government(self):
        self.assertEqual(ev("has_war = no is_subject = no has_government = fascism has_idea = free_trade")[0], True)
        self.assertEqual(ev("has_war = no", 1200)[0], False)
        self.assertEqual(ev("owns_state = 74", 100)[0], None)
        self.assertEqual(ev("owns_state = 74", 600)[0], True)
        self.assertEqual(ev("74 = { is_fully_controlled_by = ROOT }", 600)[0], True)

    def test_army_manpower_becomes_a_number_by_date(self):
        r, lines = ev("has_army_manpower = { size = 724999 }", 70)
        self.assertIs(r, False)
        self.assertEqual(lines[0]["kind"], "army_manpower")
        self.assertEqual(lines[0]["need"], 725000)
        self.assertIn("first on", cond.describe(lines[0]))
        self.assertIs(ev("has_army_manpower = { size = 724999 }", 7 * 50)[0], True)

    def test_army_ratio_by_manpower(self):
        # 24 x 30000 men = 720000: 72 divisions of 10000
        r, lines = ev("fighting_army_strength_ratio = { tag = AUS ratio > 24 }", 70)
        self.assertIs(r, False)
        self.assertEqual(lines[0]["need_divisions"], 73)
        self.assertIs(ev("fighting_army_strength_ratio = { tag = AUS ratio > 24 }", 7 * 50)[0], True)

    def test_if_else_chain_takes_the_branch_whose_limit_holds(self):
        text = ("IF = { limit = { has_dlc = Gotterdammerung } has_army_manpower = { size = 2000000 } } "
                "ELSE = { always = yes }")
        self.assertIs(ev(text)[0], False)
        self.assertIs(ev(text.replace("Gotterdammerung", "Other"))[0], True)

    def test_unknown_lines_carry_their_kind(self):
        r, lines = ev("has_stability > 0.6 HUN = { has_government = fascism } has_global_flag = x")
        self.assertIsNone(r)
        self.assertEqual(sorted(x["kind"] for x in lines), ["flag", "other_country", "stability"])

    def test_or_of_unknowns_is_one_line(self):
        r, lines = ev("OR = { has_stability > 0.6 has_war_support > 0.5 }")
        self.assertIsNone(r)
        self.assertEqual(len(lines), 1)
        self.assertEqual(lines[0]["kind"], "any_of")

    def test_hired_exclusions(self):
        block = parse("has_dynamic_modifier = { modifier = m } GER_walther_funk = { is_hired_as_advisor = no }")
        self.assertEqual(cond.hired_exclusions(block), ["GER_walther_funk"])


class Coverage(unittest.TestCase):
    def test_a_factory_count_gate_needs_its_own_number(self):
        from hoi4facts import lint_conditions as lc
        line = {"kind": "civilian_factories", "value": 49, "cmp": ">"}  # industrial_complex > 49
        self.assertFalse(lc._covered(line, "F", 100, [(90, "factories:GER >= 95")]))  # the total says nothing
        self.assertFalse(lc._covered(line, "F", 100, [(120, "civilian_factories >= 45")]))
        self.assertTrue(lc._covered(line, "F", 100, [(120, "civilian_factories >= 50")]))
        self.assertTrue(lc._covered(line, "F", 100, [(500, "focus_completed:F")]))
        self.assertTrue(lc._covered({"kind": "stability"}, "F", 100, [(110, "stability >= 0.5")]))


class StopDate(unittest.TestCase):
    class L:
        def __init__(self, plan):
            self.plan, self.findings = plan, []

        def horizon(self):
            return [(derive.day("1946.1.1"), "goal deadline")]

        def add(self, *f):
            self.findings.append(f)

    def test_stop_before_the_end_pauses_a_no_pause_run(self):
        from hoi4facts import lint_conditions as lc
        lt = self.L({"stop_on_date": "1939.08.31"})
        lc.stop_date(lt)
        self.assertEqual(lt.findings[0][1], "plan.stop_date")
        lt = self.L({"stop_on_date": "1939.08.31", "stop_on_date_pause": False})
        lc.stop_date(lt)
        self.assertEqual(lt.findings, [])
        lt = self.L({})
        lc.stop_date(lt)
        self.assertEqual(lt.findings, [])


@unittest.skipUnless(HAVE_GAME, f"game files not found at {GAME_DIR}")
class GermanyConditions(unittest.TestCase):
    """The holes of the Germany 1936 plan of 16 Nov on a seeded copy of the Germany hand plan."""

    @classmethod
    def setUpClass(cls):
        cls.game = lint_plan.load_game(GAME_DIR)
        with open(os.path.join(PLANS, "germany-fair-1936-1938.plan.json"), encoding="utf-8") as fh:
            plan = json.load(fh)
        plan = copy.deepcopy(plan)
        plan["_meta"] = {"version": 1, "source": "strategist"}
        plan["goal"] = {"summary": "test", "deadline": "1940.1.1"}
        plan["_validator"]["stop_date"] = "1940.1.1"
        plan["stop_on_date"] = "1938.06.01"
        plan["politics_auto"] = {"enabled": True, "order": [
            {"category": "government", "ideas": ["GER_hermann_goring"], "cost": 150}]}
        plan["checkpoints"] = [{"id": "cp_cze", "by": "1939.04.15", "must": ["owner:74=GER", "owner:75=GER"],
                                "else": "revise"}]
        plan["diplomacy"] = [{"id": "dp_pol", "action": "declare", "target": "POL", "when": "date >= 1939.09.01"}]
        plan["training"] = [{"template": "Infanterie-Division", "queue": 2, "batch": 1, "cap": 40}]
        cls.out = lint_plan.lint(plan, cls.game)

    def hits(self, rule):
        return [f for f in self.out["findings"] if f["rule"] == rule]

    def test_anschluss_army_gate_becomes_a_division_target(self):
        f = [x for x in self.hits("focus.condition") if "GER_anschluss" in x["message"]]
        self.assertTrue(f, self.hits("focus.condition"))
        self.assertEqual(f[0]["severity"], "error")
        self.assertIn("x AUS", f[0]["message"])
        self.assertNotIn(GAME_DIR, f[0]["message"])

    def test_an_advisor_the_game_never_shows(self):
        f = self.hits("politics.visible")
        self.assertTrue(any("GER_hermann_goring" in x["message"] and x["severity"] == "error" for x in f), f)

    def test_declaration_waits_on_gained_border_states(self):
        f = self.hits("diplomacy.borders_unheld")
        self.assertTrue(f)
        self.assertIn("74", f[0]["message"])

    def test_stop_date(self):
        self.assertTrue(self.hits("plan.stop_date"))

    def test_army_forecast_fact(self):
        self.assertIn("army_forecast", self.out["facts"])


if __name__ == "__main__":
    unittest.main()
