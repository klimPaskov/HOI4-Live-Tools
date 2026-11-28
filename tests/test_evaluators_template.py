"""evaluators/template.py: the regimental-row rule, the battle model, templates by role and width, the special-forces
cap; the installed game when present (Germany 1939 against Poland, a minor, a major)."""
import os
import sys
import unittest

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import evaluators_fixture as fx  # noqa: E402

from hoi4facts.evaluators import template  # noqa: E402

PROF = {"enemies": ["BBB"], "chosen": "test", "best_armor": 0.0, "piercing": 1.0, "division_piercing": 1.0,
        "best_vehicle_piercing": 0.0, "hardness": 0.0, "tank_hardness": 0.0, "vehicles": [], "fighters": [],
        "divisions": [{"tag": "BBB", "template": "Levy", "fielded": 1, "armor": 0.0, "piercing": 1.0, "hardness": 0.0,
                       "stats": {"width": 4.0, "organisation": 40.0, "soft_attack": 12.0, "hard_attack": 1.0,
                                 "defense": 30.0, "breakthrough": 6.0, "armor": 0.0, "piercing": 1.0,
                                 "hardness": 0.0, "hp": 50.0}}]}


class TemplateFixtureTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.sit = fx.situation(state={"techs": ["e_inf", "t_a"]})
        cls.rules = template.Rules(cls.sit)

    def test_regimental_row_rule(self):
        self.assertEqual(self.rules.slots({"infantry": 9}), 3)
        self.assertEqual(self.rules.slots({"infantry": 15}), 5)
        self.assertEqual(self.rules.slots({"infantry": 12, "combat_support": 3}), 5)
        self.assertEqual(self.rules.slots({"infantry": 8, "combat_support": 2}), 2)
        self.assertIsNone(self.rules.slots({"infantry": 26}))           # more columns than the designer has

    def test_battle_is_symmetric_and_rewards_defence(self):
        d = PROF["divisions"][0]["stats"]
        r_att, _ = template.battle(self.rules, d, d, True, PROF)
        r_def, _ = template.battle(self.rules, d, d, False, PROF)
        self.assertAlmostEqual(r_att * r_def, 1.0)                      # attacking a twin is its defence reversed
        self.assertLess(r_att, 1.0)                                      # breakthrough 6 against defence 30
        stronger = dict(d, defense=60.0)
        r2, _ = template.battle(self.rules, stronger, d, False, PROF)
        self.assertGreater(r2, 1.0)

    def test_available_units_follow_techs(self):
        units = template.available_units(self.sit)
        self.assertIn("infantry", units)
        self.assertIn("field_guns", units)
        self.assertIn("light_armor", units)                              # active = yes
        poor = template.available_units(fx.situation(state={"techs": []}))
        self.assertNotIn("infantry", poor)

    def test_line_template_at_a_given_width(self):
        r = template.best(self.sit, "line", width=6, prof=PROF)
        t = r["templates"][0]
        self.assertEqual(t["battalions"], {"infantry": 3})
        self.assertLessEqual(t["width"], 6)
        self.assertLessEqual(len(t["regimental"]), t["regimental_slots"])
        self.assertEqual(t["regimental_slots"], 1)
        self.assertIn("R", t["parts"])
        self.assertGreater(t["ic"], 0)

    def test_armour_advance_weighs_speed_supply_and_the_break(self):
        r = template.Rules(self.sit)
        r.lack_attack, r.lack_defend, r.lack_speed = -0.25, -0.65, -0.8   # the game's defines, set for the fixture
        b = type("B", (), {"rules": r, "prof": PROF})()
        enemy = PROF["divisions"][0]["stats"]
        tank = {"width": 10.0, "organisation": 30.0, "soft_attack": 60.0, "hard_attack": 10.0, "defense": 40.0,
                "breakthrough": 120.0, "armor": 20.0, "piercing": 10.0, "hardness": 0.7, "hp": 20.0,
                "speed": 8.0, "supply": 1.0}
        geo = {"battle_width": 40.0, "km": 60.0, "movement_cost": 1.0, "supply": 10.0}
        fast, pf = template.advance(b, tank, enemy, 1000.0, geo)
        slow, ps = template.advance(b, dict(tank, speed=4.0), enemy, 1000.0, geo)
        self.assertGreater(fast, slow)                                   # the same fight, half the march speed
        self.assertAlmostEqual(ps["march_hours"], 2 * pf["march_hours"], places=1)
        self.assertEqual(pf["supply_ratio"], 1.0)                        # 4 divisions x 1.0 against 10 delivered
        hungry, ph = template.advance(b, dict(tank, supply=5.0), enemy, 1000.0, geo)
        self.assertAlmostEqual(ph["supply_ratio"], 0.5)                  # 4 x 5.0 against 10
        self.assertLess(hungry, fast)
        self.assertLess(ph["speed"], pf["speed"])                        # OUT_OF_SUPPLY_SPEED on the shortfall
        weak, pw = template.advance(b, dict(tank, soft_attack=1.0, hard_attack=0.0, breakthrough=1.0), enemy,
                                    1000.0, geo)
        self.assertLess(weak, 0.0)                                       # the line holds: below every break
        self.assertEqual(pw["provinces_per_day"], 0.0)

    def test_special_forces_cap(self):
        c = template.special_forces_cap(self.sit)
        self.assertEqual(c["min"], self.rules.sf_min)
        self.assertGreaterEqual(c["cap"], c["min"])


@unittest.skipUnless(fx.real_situation("GER") is not None, "installed game not found (HOI4_EVAL_GAME / HOI4_DIR)")
class TemplateRealGameTest(unittest.TestCase):
    def test_germany_1939_against_poland(self):
        sit = fx.real_situation("GER", "1939.1.1", bookmark="1936.1.1")
        line = template.best(sit, "line", enemies=["POL"])
        self.assertEqual(line["chosen_width"], 30)                       # land.md: 30-38 wide, columns of three
        t = line["templates"][0]
        self.assertEqual(sum(t["battalions"].values()), 15)
        self.assertEqual(t["regimental_slots"], 5)
        att = template.best(sit, "attacker", enemies=["POL"])["templates"][0]
        self.assertTrue(any("artillery" in k for k in att["battalions"]))
        g = template.best(sit, "garrison", enemies=["POL"])["templates"][0]
        self.assertLessEqual(g["width"], 4)

    def test_armour_is_judged_by_advance_per_factory(self):
        sit = fx.real_situation("GER", "1939.9.1", bookmark="1936.1.1")
        r = template.best(sit, "armour", enemies=["POL"])
        geo = r["axis"]
        self.assertGreater(geo["km"], 0)                                  # province spacing from the map
        self.assertGreater(geo["supply"], 0)
        t = r["templates"][0]
        p = t["parts"]
        self.assertGreater(p["provinces_per_day"], 0)
        self.assertGreater(p["march_hours"], 0)
        self.assertGreater(t["logistics"]["supply"], 0)
        floor = template.design.floor_speed(sit)[0]
        self.assertTrue(any(len(v) > 1 for v in r["design_variants"].values()))   # designs at several floors
        tanks = [d for d in t["designs"].values()]
        self.assertTrue(tanks)
        self.assertGreater(max(d["speed"] for d in tanks), floor)         # a faster tank than the infantry floor
        self.assertNotIn("heavy_tank_chassis", t["designs"])              # the old R-only score's pick
        b = template.Builder(sit, None, ["POL"], designs=False)
        towed = b.division(["motorized"] * 6, ["artillery", "engineer"])    # affects_speed = no on both companies
        self.assertEqual(towed["speed"], b.division(["motorized"] * 6, [])["speed"])

    def test_minor_and_major(self):
        for tag, enemy in (("ETH", "ITA"), ("SOV", "GER")):
            sit = fx.real_situation(tag, "1936.1.1")
            r = template.best(sit, "line", enemies=[enemy])
            self.assertTrue(r["templates"], tag)


if __name__ == "__main__":
    unittest.main()
