"""evaluators/design.py: land designer stats, legality by role, the combat model from the defines, the enemy's AI
designs, the search with upgrades under a speed floor, the refresh trigger; the installed game when present (Germany
against the owner's MC2, a poor-tech minor, a major with other techs)."""
import os
import sys
import unittest

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import evaluators_fixture as fx  # noqa: E402

from hoi4facts.evaluators import design  # noqa: E402


class DesignFixtureTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.sit = fx.situation(state={"techs": ["e_tank_1", "e_tank_2", "e_armor_tech"]})
        cls.v = design.Vehicles.of(cls.sit.game)

    def test_stats_follow_parent_chain_module_multiplier_and_upgrades(self):
        mods = {"turret_type_slot": "e_turret_small", "main_armament_slot": "e_mg",
                "engine_type_slot": "e_engine_diesel", "special_type_slot_1": "e_radio"}
        st = design.stats(self.v, "mt_chassis_2", mods, {"e_armor_up": 2, "e_engine_up": 1})
        self.assertAlmostEqual(st["soft_attack"], 9.0)          # 10 x (1 - 0.1): mt_chassis_1's multiplier, inherited
        self.assertAlmostEqual(st["armor_value"], 46.0)         # 40 + 2 x 3
        self.assertAlmostEqual(st["breakthrough"], 14.4)        # (10 + 2 x 1) x 1.2
        self.assertAlmostEqual(st["reliability"], 1 + 0.2 - 0.02 - 0.02)
        self.assertAlmostEqual(st["maximum_speed"], 6 - 0.4 + 0.2)

    def test_legal_by_role(self):
        armour_how = {"turret_type_slot": "e_turret_big", "main_armament_slot": "e_howitzer",
                      "engine_type_slot": "e_engine_petrol"}
        self.assertTrue(any("forbidden for armor" in p for p in design.legal(self.v, "mt_chassis_2", armour_how, "armor")))
        self.assertEqual(design.legal(self.v, "mt_chassis_2", armour_how, "artillery"), [])
        small_how = dict(armour_how, turret_type_slot="e_turret_small")  # the small turret does not open big guns
        self.assertTrue(any("does not fit" in p for p in design.legal(self.v, "mt_chassis_2", small_how, "artillery")))
        mg_td = {"turret_type_slot": "e_turret_small", "main_armament_slot": "e_mg", "engine_type_slot": "e_engine_petrol"}
        self.assertTrue(design.legal(self.v, "mt_chassis_2", mg_td, "anti_tank"))   # an MG serves no TD role
        two_radios = dict(mg_td, special_type_slot_1="e_radio", special_type_slot_2="e_radio")
        self.assertTrue(any("more than 1" in p for p in design.legal(self.v, "mt_chassis_2", two_radios, "armor")))
        self.assertTrue(any("required" in p for p in design.legal(self.v, "mt_chassis_2", {}, "armor")))

    def test_upgrade_caps_follow_techs(self):
        self.assertEqual(design.Vehicles.upgrade_caps(self.v, "mt_chassis_2", {"e_armor_tech"}),
                         {"e_armor_up": 10, "e_engine_up": 4})

    def test_combat_from_defines(self):
        c = design.Combat(self.sit)
        self.assertEqual(c.pierce(50, 40), 1.0)
        self.assertEqual(c.pierce(30, 40), 0.8)
        self.assertEqual(c.pierce(10, 40), 0.5)
        self.assertAlmostEqual(c.dice, 7 / 5)
        self.assertEqual(c.rel, -0.3)

    def test_ai_design_resolves_with_the_enemy_techs(self):
        bases = {"mt_chassis_1"}
        tv = [d for d in design.ai_designs(self.sit.game) if d[2] == "bbb_basic"][0][3]
        r = design._resolve_ai(self.v, tv, bases, {"e_turret_small", "e_mg", "e_engine_petrol"})
        self.assertEqual(r[1]["main_armament_slot"], "e_mg")          # any_of: the first the enemy has
        self.assertEqual(r[2], {"e_armor_up": 2.0})
        self.assertIsNone(design._resolve_ai(self.v, tv, set(), set()))

    def test_best_meets_the_speed_floor_and_beats_the_enemy_armour(self):
        prof = {"enemies": ["BBB"], "chosen": "test", "best_armor": 36.0, "piercing": 45.0, "division_piercing": 4.0,
                "best_vehicle_piercing": 45.0, "hardness": 0.1, "tank_hardness": 0.85, "vehicles": [],
                "divisions": [{"armor": 36.0, "piercing": 4.0, "hardness": 0.1}], "fighters": []}
        r = design.best_land(self.sit, "tank_main", prof=prof, top=3)
        self.assertTrue(r["designs"])
        top = r["designs"][0]
        self.assertEqual(design.legal(self.v, top["base"], top["modules"], "armor"), [])
        self.assertGreaterEqual(top["stats"]["maximum_speed"], r["speed_floor"])
        self.assertGreaterEqual(top["stats"]["ap_attack"], 36.0)        # the cannon pierces their line, the MG does not
        self.assertGreater(top["stats"]["armor_value"], 45.0)           # armour clicks past their piercing
        art = design.best_land(self.sit, "tank_artillery", prof=prof, top=1)["designs"][0]
        self.assertEqual(art["modules"]["main_armament_slot"], "e_howitzer")
        poor = fx.situation(state={"techs": ["e_tank_1"]})
        p = design.best_land(poor, "tank_main", prof=prof, top=1)["designs"][0]
        self.assertEqual(p["base"], "mt_chassis_1")                     # what a poor country has, still designed
        self.assertLessEqual(p["upgrades"]["e_armor_up"], 4)

    def test_floors_in_one_search(self):
        prof = {"enemies": ["BBB"], "chosen": "test", "best_armor": 36.0, "piercing": 45.0, "division_piercing": 4.0,
                "best_vehicle_piercing": 45.0, "hardness": 0.1, "tank_hardness": 0.85, "vehicles": [],
                "divisions": [{"armor": 36.0, "piercing": 4.0, "hardness": 0.1}], "fighters": []}
        one = design.best_land(self.sit, "tank_main", prof=prof, top=1)
        many = design.best_land(self.sit, "tank_main", prof=prof, top=1, floor=[one["speed_floor"], 99.0])
        self.assertEqual(many["by_floor"][0][1][0]["stats"], one["designs"][0]["stats"])   # same answer per floor
        self.assertEqual(many["by_floor"][1][1], [])                                       # an unreachable floor
        for fl, ds in many["by_floor"]:
            for d in ds:
                self.assertGreaterEqual(d["stats"]["maximum_speed"] + 1e-9, fl)

    def test_enemy_profile_counts_live_sightings(self):
        sit = fx.situation(state={"techs": ["e_tank_1"], "enemies": ["BBB"], "enemy_designs": {"BBB": [
            {"type": "mt_chassis_2", "modules": {"turret_type_slot": "e_turret_big", "main_armament_slot": "e_cannon",
                                                 "engine_type_slot": "e_engine_petrol"}, "upgrades": {"e_armor_up": 3}}]}})
        prof = design.enemy_profile(sit)
        self.assertEqual(prof["enemies"], ["BBB"])
        self.assertEqual(prof["best_armor"], 49.0)
        self.assertEqual(prof["best_vehicle_piercing"], 40.0)

    def test_refresh_trigger(self):
        self.assertIn("tank_main", design.refresh_roles(self.sit, "e_tank_2"))
        self.assertIn("tank_main", design.refresh_roles(self.sit, "e_armor_tech"))
        self.assertEqual(design.refresh_roles(self.sit, "no_such_tech"), [])


@unittest.skipUnless(fx.real_situation("GER") is not None, "installed game not found (HOI4_EVAL_GAME / HOI4_DIR)")
class DesignRealGameTest(unittest.TestCase):
    def test_germany_1941_reaches_the_owner_rule_and_beats_his_mc2(self):
        sit = fx.real_situation("GER", "1941.6.1", bookmark="1936.1.1")
        prof = design.enemy_profile(sit, ["SOV"])
        r = design.best_land(sit, "tank_main", prof=prof, top=1)
        top = r["designs"][0]
        self.assertTrue(top["base"].startswith("medium_tank_chassis"))
        self.assertEqual(top["modules"]["main_armament_slot"], "tank_medium_cannon_2")
        self.assertIn("tank_radio_3", top["modules"].values())
        self.assertLess(abs(top["stats"]["maximum_speed"] - r["speed_floor"]), 0.5)   # his 4.0 km/h rule
        self.assertGreaterEqual(top["upgrades"]["tank_nsb_armor_upgrade"], 10)
        mc2 = {"turret_type_slot": "tank_medium_three_man_tank_turret", "main_armament_slot": "tank_medium_cannon_2",
               "special_type_slot_1": "tank_radio_3", "special_type_slot_2": "secondary_turret_small_cannon",
               "special_type_slot_3": "secondary_turret_small_cannon", "suspension_type_slot":
               "tank_torsion_bar_suspension", "engine_type_slot": "tank_diesel_engine",
               "armor_type_slot": "tank_riveted_armor"}
        his = design.score_design(sit, "tank_main", "medium_tank_chassis_2", mc2,
                                  {"tank_nsb_engine_upgrade": 0, "tank_nsb_armor_upgrade": 17}, prof=prof)
        self.assertEqual(his["legal"], [])
        self.assertGreaterEqual(top["score"], his["score"])

    def test_ship_roles_use_their_hull_types_with_integrated_dlc(self):
        # Man the Guns ships inside the install (integrated_dlc/): the ship designer counts as owned with no DLC given
        sit = fx.real_situation("GER", "1939.1.1", bookmark="1936.1.1")
        self.assertIn("Man the Guns", sit.world.dlcs)
        v = design.Vehicles.of(sit.game)
        for role, spec in design.SHIP_ROLES.items():
            r = design.best(sit, role)
            self.assertTrue(r["designs"], role)
            for d in r["designs"]:
                self.assertTrue(set(v.equip[v.family(d["base"])]["type"]) & spec["types"], (role, d["base"]))
            keys = [(d["base"], tuple(sorted(d["modules"].values()))) for d in r["designs"]]
            self.assertEqual(len(keys), len(set(keys)))

    def test_poor_minor_and_other_major(self):
        for tag, date in (("ETH", "1936.1.1"), ("SOV", "1938.1.1"), ("JAP", "1938.1.1")):
            sit = fx.real_situation(tag, date, bookmark="1936.1.1")
            r = design.best_land(sit, "tank_main", top=1)
            if tag == "ETH":
                self.assertTrue(r["designs"] or r["rejected_by"] is not None)
            else:
                self.assertTrue(r["designs"], tag)
                self.assertGreaterEqual(r["designs"][0]["stats"]["maximum_speed"], r["speed_floor"])


if __name__ == "__main__":
    unittest.main()


class DesignCliTest(unittest.TestCase):
    def test_cli_prints_the_answer_and_its_reasoning(self):
        import contextlib
        import io
        from hoi4facts import evaluate
        mod = fx.write_mod()
        state = os.path.join(mod, "state.json")
        with open(state, "w") as fh:
            fh.write('{"techs": ["e_tank_1", "e_tank_2"], "enemies": ["BBB"]}')
        out = io.StringIO()
        with contextlib.redirect_stdout(out):
            evaluate.main(["design", "tank_main", "--country", "AAA", "--game", fx.GAME, "--mod", mod,
                           "--state", state, "--dlc", "none"])
        text = out.getvalue()
        self.assertIn("DESIGN tank_main", text)
        self.assertIn("speed floor", text)
        self.assertIn("PIERCING_THRESHOLDS", text)
        self.assertIn("#1 mt_chassis_", text)
        self.assertIn("why: effective attack", text)
