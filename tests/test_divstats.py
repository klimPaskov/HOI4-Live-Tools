"""divstats.py: division combat stats from the game files; lint_firepower.py: firepower, dominance and the rest."""

import os
import sys
import unittest

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.join(ROOT, "scripts"))
sys.path.insert(0, os.path.join(ROOT, "tests"))

from hoi4facts import derive, divstats as ds, economy, lint_firepower as lf, lint_plan  # noqa: E402
from test_force_design import GAME_DIR, HAVE_GAME, germany_at_war  # noqa: E402

EQ = {"infantry_equipment": "infantry_equipment_1", "artillery_equipment": "artillery_equipment_1",
      "support_equipment": "support_equipment_1"}


def synthetic():
    d = ds.Data.__new__(ds.Data)
    d.units = {
        "inf": {"width": 2.0, "support": False, "need": {"rifle": 100}, "factors": {}, "max_strength": 25.0,
                "max_organisation": 60.0, "categories": {"category_light_infantry"}, "types": set()},
        "gun": {"width": 3.0, "support": False, "need": {"cannon": 24}, "factors": {}, "max_strength": 1.6,
                "max_organisation": 0.0, "categories": {"category_artillery"}, "types": set()},
        "sup_gun": {"width": 0.0, "support": True, "need": {"cannon": 12}, "factors": {"soft_attack": -0.5},
                    "max_strength": 0.2, "max_organisation": 0.0, "categories": {"category_artillery"},
                    "types": set()},
        "tank": {"width": 2.0, "support": False, "need": {"chassis": 60}, "factors": {}, "max_strength": 2.0,
                 "max_organisation": 10.0, "categories": set(), "types": set()}}
    d.equipment = {
        "rifle": {"stats": {"soft_attack": 6.0, "defense": 22.0, "breakthrough": 3.0, "ap_attack": 4.0},
                  "archetype": None, "parent": None, "is_archetype": True, "module_mult": {}, "year": 1936},
        "cannon": {"stats": {"soft_attack": 26.0, "breakthrough": 10.0, "defense": 11.0, "ap_attack": 5.0},
                   "archetype": None, "parent": None, "is_archetype": True, "module_mult": {}, "year": 1934},
        "chassis": {"stats": {"armor_value": 20.0, "hardness": 0.8}, "archetype": None, "parent": None,
                    "is_archetype": True, "module_mult": {"soft_attack": 0.05}, "year": 1936}}
    d.modules = {"gun_mod": {"add": {"soft_attack": 10.0, "ap_attack": 20.0}, "mult": {}, "category": None},
                 "plate": {"add": {"defense": 2.0}, "mult": {"armor_value": 0.5}, "category": None}}
    d.upgrades = {"arm": {"add": {"armor_value": 2.5}, "mult": {}}}
    d.techs, d.doctrines, d.enables = {"t": [("category_artillery", {"soft_attack": 0.1})]}, {}, {}
    return d


class Arithmetic(unittest.TestCase):
    def test_battalions_sum_and_factors_apply(self):
        d = synthetic()
        out = ds.division(d, ["inf"] * 9 + ["gun"], ["sup_gun"], {}, ds.factors(d, {"t"}, []))
        # 9 x 6 + 26 x 1.1 + 26 x 0.5 x 1.1
        self.assertAlmostEqual(out["soft_attack"], round(54 + 28.6 + 14.3, 1))
        self.assertEqual(out["width"], 21.0)
        self.assertAlmostEqual(out["hp"], 226.8)
        self.assertAlmostEqual(out["breakthrough"], 27 + 10 + 10)  # the support gun has no breakthrough factor here

    def test_a_design_adds_modules_scales_attack_and_multiplies(self):
        d = synthetic()
        st = d.design_stats("chassis", {"a": "gun_mod", "b": "plate"}, {"arm": 2})
        self.assertAlmostEqual(st["soft_attack"], 10.5)        # module attack x 1.05
        self.assertAlmostEqual(st["armor_value"], (20 + 5) * 1.5)
        self.assertAlmostEqual(st["ap_attack"], 20.0)

    def test_template_units_reads_columns_and_counts(self):
        self.assertEqual(ds.template_units({"regiments": [["inf", "inf"], ["gun"]], "support": ["eng"]}),
                         (["inf", "inf", "gun"], ["eng"]))
        self.assertEqual(ds.parse_counts("inf:3,gun"), ["inf", "inf", "inf", "gun"])


class Helpers(unittest.TestCase):
    def test_date_gate_and_baseline_interpolation(self):
        self.assertEqual(lf._date_gate({"all": ["date >= 1938.3.1", "xp:army >= 20"]}), derive.day("1938.3.1"))
        self.assertIsNone(lf._date_gate("xp:army >= 20"))
        rows = [{"date": "1939.01.01", "military_factories": 51}, {"date": "1939.09.01", "military_factories": 67}]
        self.assertEqual(lf.ai_on(rows, derive.day("1939.9.1"), "military_factories"), 67)
        self.assertIsNone(lf.ai_on(rows, derive.day("1940.1.1"), "military_factories"))
        self.assertTrue(51 < lf.ai_on(rows, derive.day("1939.5.1"), "military_factories") < 67)

    def test_the_plan_dates_its_own_switch(self):
        class L:
            plan = {"construction": [{"building": "industrial_complex"}],
                    "phases": [{"id": "a", "enter": "date >= 1936.1.1", "overrides": {}},
                               {"id": "b", "enter": "date >= 1938.7.1",
                                "overrides": {"construction": [{"building": "arms_factory"}]}}]}
        self.assertEqual(economy.plan_switch(L), derive.day("1938.7.1"))
        L.plan["construction"] = [{"building": "arms_factory"}]
        self.assertIsNone(economy.plan_switch(L))

    def test_the_ai_air_read_is_in_the_baseline(self):
        rows = lf.baseline_rows("GER")
        row = next(r for r in rows if r["date"] == "1939.09.01")
        self.assertEqual((row["fighters"], row["cas"]), (2400, 950))
        self.assertEqual(row["air_read"], "contaminated")
        self.assertEqual((row["fighters_estimate"], row["cas_estimate"]), (1340, 530))


@unittest.skipUnless(HAVE_GAME, f"game files not found at {GAME_DIR}")
class InGame(unittest.TestCase):
    """The four rows read in game (A/B table, 18 Nov): the division view against the calculator."""

    @classmethod
    def setUpClass(cls):
        cls.game = lint_plan.load_game(GAME_DIR)
        cls.data = ds.Data(cls.game)
        techs, docs, cls.variants = ds.history_start(cls.data, "GER")
        cls.mods = ds.factors(cls.data, techs, docs)

    def row(self, bats, sup, eq=EQ):
        return ds.division(self.data, bats, sup, eq, self.mods)

    def test_the_line_templates_against_the_reads(self):
        ours = self.row(["infantry"] * 9, ["engineer", "artillery"])
        ai = self.row(["infantry"] * 9 + ["artillery_brigade"], ["engineer", "artillery"])
        for got, read in ((ours, {"width": 18, "hp": 227, "defense": 231, "breakthrough": 40, "soft_attack": 86}),
                          (ai, {"width": 21, "hp": 229, "defense": 242, "breakthrough": 50, "soft_attack": 112})):
            self.assertEqual(got["width"], read["width"])
            for k, tol in (("hp", 0.01), ("defense", 0.03), ("breakthrough", 0.03), ("soft_attack", 0.16)):
                self.assertLess(abs(got[k] - read[k]) / read[k], tol, (k, got[k], read[k]))
        # the line artillery battalion is the 26 soft attack and 10 breakthrough the reads differ by
        self.assertAlmostEqual(ai["soft_attack"] - ours["soft_attack"], 26 * 1.1, delta=1)

    def test_the_panzer_division_against_the_read(self):
        v = self.variants["Panzer II"]
        eq = dict(EQ, motorized_equipment="motorized_equipment_1",
                  light_tank_chassis=self.data.design_stats(v["type"], v["modules"], v["upgrades"]))
        got = self.row(["light_armor"] * 4 + ["motorized"] * 2, ["mot_recon", "engineer", "artillery"], eq)
        self.assertEqual(got["width"], 12)
        self.assertLess(abs(got["hp"] - 67) / 67, 0.02)
        self.assertLess(abs(got["soft_attack"] - 92) / 92, 0.03)

    def test_a_war_plan_gets_firepower_and_dominance_facts(self):
        out = lint_plan.lint(germany_at_war(), self.game)
        force = out["facts"].get("force") or {}
        self.assertIn("dominance", force)
        self.assertTrue(force.get("firepower"))
        rules = {f["rule"] for f in out["findings"]}
        self.assertTrue(rules & {"force.firepower", "force.dominance"}, sorted(rules)[:40])


if __name__ == "__main__":
    unittest.main()
