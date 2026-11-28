"""designer.py (module-by-module designs by computed stats) and aircover.py (range against regions).

A mini game folder is written per test run: one fighter airframe archetype with slots, a duplicated CAS family, a
handful of modules (weapons that set the equipment type, engines with thrust, a fuel tank with a multiply, an
average-stat module, a mission-only bonus), techs that unlock them and a special project that grants a prototype
engine. The stat rule under test is the designer's: (base + sum add) x (1 + sum multiply).
"""
import os
import sys
import tempfile
import unittest

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.join(ROOT, "scripts"))

from hoi4facts import aircover, designer, mapgraph  # noqa: E402
from hoi4facts.game import Game  # noqa: E402

EQUIP = """
equipments = {
    small_plane_airframe = {
        year = 1933
        is_archetype = yes
        type = fighter
        module_slots = {
            fixed_main_weapon_slot = { required = yes allowed_module_categories = { fixed_gun } }
            engine_type_slot = { required = yes allowed_module_categories = { engine } }
            special_type_slot_1 = { required = no allowed_module_categories = { fuel special } }
            special_type_slot_2 = { required = no allowed_module_categories = { fuel special } }
        }
        air_range = 650
        maximum_speed = 450
        air_agility = 45
        air_defence = 11
        weight = 5
        build_cost_ic = 6
    }
    small_plane_airframe_1 = { archetype = small_plane_airframe year = 1936 derived_variant_name = fighter_equipment_1 }
    small_plane_airframe_2 = { archetype = small_plane_airframe year = 1940 air_defence = 12 }
    fighter_equipment_1 = { archetype = small_plane_airframe_1 }
}
duplicate_archetypes = {
    small_plane_cas_airframe = { archetype = small_plane_airframe type = cas
        variant_name = { small_plane_cas_airframe_1 = CAS_equipment_1 } }
}
equipments = { CAS_equipment_1 = { archetype = small_plane_cas_airframe year = 1936 } }
"""
MODULES = """
equipment_modules = {
    light_mg = { category = fixed_gun add_equipment_type = fighter
        add_stats = { air_attack = 10 build_cost_ic = 2 weight = 1 } }
    cannon = { category = fixed_gun add_equipment_type = fighter
        add_stats = { air_attack = 20 build_cost_ic = 5 weight = 3 } multiply_stats = { air_agility = -0.1 } }
    bomb_locks = { category = fixed_gun add_equipment_type = cas
        add_stats = { air_ground_attack = 8 weight = 2 } }
    engine_1 = { category = engine add_stats = { thrust = 6 build_cost_ic = 4 } multiply_stats = { maximum_speed = 0.1 } }
    engine_2 = { category = engine add_stats = { thrust = 12 build_cost_ic = 8 } multiply_stats = { maximum_speed = 0.2 } }
    proto_engine = { category = engine add_stats = { thrust = 30 } multiply_stats = { maximum_speed = 1.0 } }
    fuel_tank = { category = fuel add_stats = { weight = 1 build_cost_ic = 1 } multiply_stats = { air_range = 0.5 } }
    armor = { category = special add_stats = { weight = 4 } add_average_stats = { air_defence = 4 }
        mission_type_stats = { limit = { air_superiority } add_stats = { air_attack = 3 } } }
}
"""
TECHS = """
technologies = {
    early_fighter = { enable_equipments = { small_plane_airframe_1 } enable_equipment_modules = { light_mg engine_1 } }
    better_fighter = { enable_equipments = { small_plane_airframe_2 }
        enable_equipment_modules = { cannon engine_2 fuel_tank armor bomb_locks } }
}
"""
GROUPS = """
mio_cat_small = { equipment_type = { small_plane_airframe } }
"""
PROJECT = """
sp_proto = { complete_effect = { add_equipment_module = proto_engine } }
"""


def mini_game():
    root = tempfile.mkdtemp(prefix="designer_")
    files = {"common/units/equipment/planes.txt": EQUIP, "common/units/equipment/modules/m.txt": MODULES,
             "common/technologies/air.txt": TECHS, "common/special_projects/projects/air.txt": PROJECT,
             "common/equipment_groups/g.txt": GROUPS}
    for rel, text in files.items():
        p = os.path.join(root, rel)
        os.makedirs(os.path.dirname(p), exist_ok=True)
        with open(p, "w") as f:
            f.write(text)
    return Game(root, cache_dir=root)


class DesignerTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.cat = designer.Catalog(mini_game())

    def test_base_stats_inherit_archetype_and_duplicate(self):
        s = self.cat.base_stats("small_plane_airframe_2")
        self.assertEqual(s["air_range"], 650)
        self.assertEqual(s["air_defence"], 12)  # own value over the archetype's
        self.assertIn("small_plane_cas_airframe_2", self.cat.equip)  # duplicated family
        self.assertEqual(self.cat.base_stats("small_plane_cas_airframe_1")["air_range"], 650)
        self.assertEqual(len(self.cat.slots("small_plane_cas_airframe_2")), 4)

    def test_stat_rule_add_then_multiply(self):
        ev = designer.evaluate(self.cat, "small_plane_airframe_2",
                               {"fixed_main_weapon_slot": "light_mg", "engine_type_slot": "engine_2",
                                "special_type_slot_1": "fuel_tank", "special_type_slot_2": "fuel_tank"})
        self.assertEqual(ev["problems"], [])
        self.assertEqual(ev["stats"]["air_range"], 1300)  # 650 x (1 + 0.5 + 0.5)
        self.assertAlmostEqual(ev["stats"]["maximum_speed"], 540)  # 450 x 1.2
        self.assertEqual(ev["stats"]["build_cost_ic"], 6 + 2 + 8 + 2)
        self.assertIn("fighter", ev["types"])

    def test_average_and_mission_stats(self):
        mods = {"fixed_main_weapon_slot": "light_mg", "engine_type_slot": "engine_2",
                "special_type_slot_1": "armor", "special_type_slot_2": "armor"}
        plain = designer.evaluate(self.cat, "small_plane_airframe_2", mods)
        self.assertEqual(plain["stats"]["air_defence"], 16)  # 12 + mean(4, 4)
        self.assertEqual(plain["stats"]["air_attack"], 10)
        sup = designer.evaluate(self.cat, "small_plane_airframe_2", mods, "air_superiority")
        self.assertEqual(sup["stats"]["air_attack"], 16)  # two armor modules' mission bonus

    def test_problems(self):
        ev = designer.evaluate(self.cat, "small_plane_airframe_2",
                               {"fixed_main_weapon_slot": "engine_1", "special_type_slot_1": "armor",
                                "special_type_slot_2": "armor"})
        text = " ".join(ev["problems"])
        self.assertIn("required slot engine_type_slot empty", text)
        self.assertIn("does not fit fixed_main_weapon_slot", text)
        heavy = designer.evaluate(self.cat, "small_plane_airframe_1",
                                  {"fixed_main_weapon_slot": "cannon", "engine_type_slot": "engine_1",
                                   "special_type_slot_1": "armor", "special_type_slot_2": "armor"})
        self.assertTrue(any("thrust" in p for p in heavy["problems"]))  # 6 thrust < 5+3+8 weight

    def test_available_follows_techs_and_projects(self):
        bases, mods = designer.available(self.cat, {"early_fighter"})
        self.assertIn("small_plane_airframe_1", bases)
        self.assertNotIn("small_plane_airframe_2", bases)
        self.assertNotIn("fighter_equipment_1", bases)  # inherits slots, not a designer base
        self.assertNotIn("CAS_equipment_1", bases)  # a duplicate family's variant name
        self.assertIn("small_plane_cas_airframe_1", bases)  # unlocks with its source
        self.assertEqual(mods & {"light_mg", "engine_1", "cannon", "proto_engine"}, {"light_mg", "engine_1"})
        _, granted = designer.available(self.cat, {"early_fighter"}, granted={"proto_engine"})
        self.assertIn("proto_engine", granted)

    def test_best_meets_range_and_type(self):
        bases, mods = designer.available(self.cat, {"early_fighter", "better_fighter"})
        r = designer.best(self.cat, "fighter", bases, mods, need={"air_range": 900}, top=1)
        d = r["designs"][0]
        self.assertGreaterEqual(d["stats"]["air_range"], 900)
        self.assertIn("fuel_tank", d["modules"].values())
        self.assertIn("fighter", d["types"])
        cas = designer.best(self.cat, "cas", bases, mods, top=1)["designs"][0]
        self.assertEqual(cas["base"].startswith("small_plane_cas_airframe"), True)
        self.assertEqual(cas["modules"]["fixed_main_weapon_slot"], "bomb_locks")
        none = designer.best(self.cat, "fighter", bases, mods, need={"air_range": 5000})
        self.assertEqual(none["designs"], [])
        self.assertIn("air_range", none["rejected_by"])

    def test_rival_exchange(self):
        bases, mods = designer.available(self.cat, {"early_fighter", "better_fighter"})
        rival = designer.evaluate(self.cat, "small_plane_airframe_2",
                                  {"fixed_main_weapon_slot": "cannon", "engine_type_slot": "engine_2"})["stats"]
        r = designer.best(self.cat, "fighter", bases, mods, rival=rival, min_exchange=1.0, top=1)
        self.assertTrue(r["designs"])
        self.assertGreaterEqual(r["designs"][0]["exchange"], 1.0)

    def test_duel_symmetry(self):
        a = {"air_attack": 20, "air_defence": 10, "air_agility": 40, "maximum_speed": 500}
        self.assertEqual(designer.duel(a, a), 1.0)
        faster = dict(a, maximum_speed=600)
        self.assertGreater(designer.duel(faster, a), 1.0)
        self.assertLess(designer.duel(a, faster), 1.0)

    def test_plan_entry_writes_every_module(self):
        bases, mods = designer.available(self.cat, {"early_fighter", "better_fighter"})
        d = designer.best(self.cat, "fighter", bases, mods, top=1)["designs"][0]
        row = designer.plan_entry(d, "d_f", "Fighter 40", when="tech:better_fighter")
        self.assertEqual(row["kind"], "plane")
        self.assertTrue(row["modules"])
        self.assertIn("computed:", row["why"])


    def test_mio_bonus_and_rank(self):
        orgs = [{"id": "range_co", "equipment_type": ["mio_cat_small"],
                 "initial_trait": {"equipment_bonus": {"air_range": 0.5}},
                 "traits": [{"token": "t1", "equipment_bonus": {"air_attack": 0.2}},
                            {"token": "t_tank", "equipment_bonus": {"air_attack": 9},
                             "limit_to_equipment_type": ["medium_tank_chassis"]}]},
                {"id": "attack_co", "equipment_type": ["small_plane_airframe"],
                 "initial_trait": {"equipment_bonus": {"air_attack": 0.05}}, "traits": []},
                {"id": "tank_co", "equipment_type": ["medium_tank_chassis"], "initial_trait": {}, "traits": []}]
        self.assertTrue(designer.covers(self.cat, ["mio_cat_small"], "small_plane_cas_airframe_2"))
        self.assertFalse(designer.covers(self.cat, ["medium_tank_chassis"], "small_plane_airframe_2"))
        self.assertEqual(designer.mio_bonus(self.cat, orgs[0], "small_plane_airframe_2"),
                         {"air_range": 0.5, "air_attack": 0.2})  # the tank-only trait is left out
        bases, mods = designer.available(self.cat, {"early_fighter", "better_fighter"})
        r = designer.mio_rank(self.cat, orgs, "fighter", bases, mods, need={"air_range": 900})
        self.assertEqual([o["id"] for o in r["organisations"]], ["range_co", "attack_co"])
        # the range trait reaches 900 km (650 x 1.5) with no fuel tank, freeing the slot the plain design spends
        plain = designer.best(self.cat, "fighter", bases, mods, need={"air_range": 900}, top=1)["designs"][0]
        self.assertIn("fuel_tank", plain["modules"].values())
        self.assertNotIn("fuel_tank", r["organisations"][0]["design"]["modules"].values())


class AirCoverTest(unittest.TestCase):
    def graph(self):
        # region 1: provinces 10 px apart on a line; region 2 far away
        prov = {i: {"xy": [i * 10.0, 0.0], "region": 1, "type": "land", "state": 1} for i in range(1, 11)}
        prov[99] = {"xy": [5000.0, 0.0], "region": 2, "type": "land", "state": 2}
        prov[50] = {"xy": [0.0, 0.0], "region": 1, "type": "sea"}
        return {"provinces": prov, "states": {1: {"provinces": list(range(1, 11))}, 2: {"provinces": [99]}}}

    def test_coverage_and_range_for(self):
        g = self.graph()
        base = [(0.0, 0.0)]
        self.assertEqual(aircover.coverage(g, 1, base, 50, km=1.0), 0.5)  # x = 10..50 of 10..100
        self.assertEqual(aircover.coverage(g, 1, base, 1000, km=1.0), 1.0)
        self.assertEqual(aircover.range_for(g, 1, base, 0.9, km=1.0), 90.0)
        self.assertEqual(aircover.coverage(g, 2, base, 1000, km=1.0), 0.0)
        self.assertIsNone(aircover.range_for(g, 3, base))

    def test_radius_and_state_point(self):
        g = self.graph()
        self.assertEqual(aircover.radius(g, 1, km=1.0), 50.0)
        self.assertEqual(aircover.state_point(g, 1), (50.0, 0.0))
        self.assertEqual(len(aircover.base_points(g, states=[1], provinces=[99])), 2)


REAL = os.environ.get("HOI4_DIR", mapgraph.DEFAULT_GAME_DIR)


@unittest.skipUnless(os.path.isdir(os.path.join(REAL, "common", "units", "equipment", "modules")), "no game files")
class Golden(unittest.TestCase):
    """The owner's worked designs (Germany and the UK, 1940 techs) through the evaluator on the installed game:
    the module rule must give the range the in-game designer shows before country bonuses."""

    @classmethod
    def setUpClass(cls):
        cls.cat = designer.Catalog(Game(REAL))

    def test_spitfire_range_and_thrust(self):
        mods = {"fixed_main_weapon_slot": "heavy_mg_4x", "fixed_auxiliary_weapon_slot_1": "heavy_mg_4x",
                "fixed_auxiliary_weapon_slot_2": "heavy_mg_4x", "engine_type_slot": "engine_3_1x",
                "special_type_slot_1": "fuel_tanks_small", "special_type_slot_2": "fuel_tanks_small"}
        ev = designer.evaluate(self.cat, "small_plane_airframe_2", mods, "air_superiority")
        self.assertEqual(ev["problems"], [])
        self.assertEqual(ev["stats"]["air_range"], 1300)  # the owner's Spitfire: 1,300 km
        self.assertEqual(ev["stats"]["air_attack"], 54)

    def test_cas_family_is_a_designer_base(self):
        bases, _ = designer.available(self.cat, {"basic_small_airframe", "improved_small_airframe"})
        self.assertIn("small_plane_cas_airframe_2", bases)
        self.assertNotIn("CAS_equipment_2", bases)
        self.assertNotIn("fighter_equipment_2", bases)


if __name__ == "__main__":
    unittest.main()
