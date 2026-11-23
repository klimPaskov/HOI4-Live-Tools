"""Resources as strategy: facts resource-states (states, owners, reach, discounts, the synthetic refinery) and the
economy.resource_source lint. Reads the installed game; skipped without it."""

import copy
import json
import os
import sys
import unittest

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.join(ROOT, "scripts"))

from hoi4facts import lint_plan, lint_resources, resources  # noqa: E402

PLANS = os.path.join(ROOT, "skills", "hoi4-campaign-playbook", "plans")
GAME_DIR = os.environ.get("HOI4_DIR", lint_plan.DEFAULT_GAME_DIR)
HAVE_GAME = os.path.isfile(os.path.join(GAME_DIR, "map", "provinces.bmp"))


def base_plan():
    with open(os.path.join(PLANS, "germany-fair-1936-1938.plan.json"), encoding="utf-8") as fh:
        plan = copy.deepcopy(json.load(fh))
    plan["_meta"] = {"version": 1, "source": "strategist"}
    plan["goal"] = {"summary": "test", "deadline": "1939.9.1"}
    plan["production"] = [{"equipment": "motorized_equipment_1", "factories": 20, "from": "1936.1.1"}]
    plan.pop("resource_sources", None)
    plan.pop("trade", None)
    return plan


@unittest.skipUnless(HAVE_GAME, "needs the installed game")
class ResourceFacts(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.game = lint_plan.load_game(GAME_DIR)
        cls.lint = lint_plan.Lint(base_plan(), cls.game, lint_plan.FRONT_K, None, None)

    def test_rubber_states_are_ranked_with_owner_and_reach(self):
        rows = resources.resource_states(self.lint.world, "GER", "rubber", 10)
        self.assertTrue(rows)
        units = [r["resources"]["rubber"] for r in rows]
        self.assertEqual(units, sorted(units, reverse=True))
        for r in rows:
            self.assertIn(r["reach"], ("own", "land", "sea"))
            self.assertTrue(r["owner"])
        # the richest rubber lies overseas for Germany
        self.assertEqual(rows[0]["reach"], "sea")

    def test_discounts_and_the_refinery_come_from_the_files(self):
        disc = {d["idea"]: d["factor"] for d in resources.trade_discounts(self.lint.world, "GER")}
        self.assertTrue(any(v < 0 for v in disc.values()))
        ref = resources.synthetic_refinery(self.lint.world)
        self.assertGreater(ref["base_cost"] or 0, 0)
        self.assertGreater(ref["rubber_max"], ref["rubber_base"])
        some = next(iter(ref["rubber_per_tech"]))
        self.assertEqual(resources.refinery_rubber(ref, {some}),
                         ref["rubber_base"] + ref["rubber_per_tech"][some])


@unittest.skipUnless(HAVE_GAME, "needs the installed game")
class ResourceSourceLint(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.game = lint_plan.load_game(GAME_DIR)

    def findings(self, plan):
        lt = lint_plan.Lint(plan, self.game, lint_plan.FRONT_K, None, None)
        lint_resources.check(lt)
        return [f for f in lt.findings if f["rule"] == "economy.resource_source" and "rubber" in f["where"]]

    def test_an_unnamed_rubber_need_is_a_warning(self):
        got = self.findings(base_plan())
        self.assertEqual([f["severity"] for f in got], ["warning"])

    def test_named_sources_short_of_the_need_are_an_error(self):
        plan = base_plan()
        plan["resource_sources"] = {"rubber": [{"source": "trade", "partner": "HOL", "route": "sea", "units": 1}]}
        got = self.findings(plan)
        self.assertEqual([f["severity"] for f in got], ["error"])
        self.assertIn("navy.convoy_escort", got[0]["message"])

    def test_a_stated_deficit_or_a_trade_covers_it_until_its_end(self):
        plan = base_plan()
        plan["resource_sources"] = {"rubber": [
            {"source": "trade", "partner": "HOL", "route": "sea", "units": 99, "until": "1937.1.1"},
            {"source": "deficit", "units": 99, "until": "1939.9.1", "why": "refineries from 1939"}]}
        self.assertEqual(self.findings(plan), [])
        # the trade alone stops counting after its end; a later line date finds the gap
        plan["resource_sources"]["rubber"].pop()
        plan["production"].append({"equipment": "motorized_equipment_1", "factories": 25, "from": "1938.1.1"})
        got = self.findings(plan)
        self.assertEqual(len(got), 1)
        self.assertIn("1938.01.01", got[0]["message"])


if __name__ == "__main__":
    unittest.main()
