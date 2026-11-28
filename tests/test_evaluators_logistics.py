"""evaluators/logistics.py: the fuel ledger (capacity, gain, use, stock target per war day) and the axis supply graph
(widest railway path, sources, the installed game's Germany against Poland when present); divstats fuel per battalion."""
import os
import sys
import unittest

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import evaluators_fixture as fx  # noqa: E402

from hoi4facts import divstats  # noqa: E402
from hoi4facts.evaluators import logistics  # noqa: E402

BUILDINGS = """
buildings = {
    e121_silo = { base_cost = 5000 country_modifiers = { modifiers = { max_fuel_building = 10.0 } } }
    e121_refinery = { base_cost = 14500 country_modifiers = { modifiers = { fuel_gain_from_states = 2.0 } } }
}
"""
DEFINES = """
NDefines.NCountry.BASE_FUEL_CAPACITY = 1000
NDefines.NCountry.BASE_FUEL_GAIN = 2.0
NDefines.NCountry.BASE_FUEL_GAIN_PER_OIL = 2
NDefines.NCountry.STARTING_FUEL_RATIO = 0.25
NDefines.NMilitary.ARMY_FUEL_COST_MULT = 0.5
NDefines.NMilitary.ARMY_COMBAT_FUEL_MULT = 1.0
NDefines.NMilitary.ARMY_MOVEMENT_FUEL_MULT = 1.0
NDefines.NAir.FUEL_COST_MULT = 0.35
"""
EXTRA = {"common/buildings/zz_e121.txt": BUILDINGS, "common/defines/zz_e121.lua": DEFINES}


class FuelLedgerTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.sit = fx.situation(state={"buildings": {"e121_silo": 2, "e121_refinery": 1}, "resources": {"oil": 3}},
                               extra=EXTRA)

    def test_capacity_and_gain_from_the_buildings(self):
        r = logistics.fuel(self.sit, [])
        self.assertEqual(r["capacity"], 1000 + 1000 * 2 * 10)
        f = 1 + self.sit.modifier("fuel_gain_factor")
        self.assertAlmostEqual(r["gain_hour"], round((2 + 2 * 3 + 2 * 1) * f, 2))
        self.assertEqual(r["levels"], {"e121_silo": 2.0, "e121_refinery": 1.0})
        self.assertIsNone(r["days_carried"])                       # nothing burns: the stock does not fall

    def test_use_target_and_fixes(self):
        divs = [{"name": "armour", "count": 4, "fuel": 25.0}]
        r = logistics.fuel(self.sit, divs, war_days=30)
        use = 4 * 25.0 * 0.5 * (1 + 1)
        self.assertAlmostEqual(r["use_hour"], use)
        self.assertEqual(r["per_war_day"], round(24 * (use - r["gain_hour"])))
        self.assertEqual(r["target"], round(30 * 24 * (use - r["gain_hour"])))
        self.assertGreater(r["shortfall"], 0)
        self.assertEqual(r["refineries_for_shortfall"]["building"], "e121_refinery")
        self.assertEqual(r["silos"]["building"], "e121_silo")      # the target is above capacity
        self.assertAlmostEqual(r["days_carried"], round(r["stock_at_war"] / r["per_war_day"], 1))

    def test_live_stock_and_planes(self):
        sit = fx.situation(state={"fuel": {"stock": 500.0, "capacity": 900.0}, "resources": {"oil": 0}}, extra=EXTRA)
        r = logistics.fuel(sit, [], planes=[{"name": "f", "count": 100, "fuel": 0.2, "mission": "AIR_SUPERIORITY"}])
        self.assertEqual(r["capacity"], 900)
        self.assertEqual(r["stock_now"], 500)
        self.assertEqual(r["stock_from"], "live snapshot")
        self.assertAlmostEqual(r["use_hour"], round(100 * 0.2 * 1.0 * 0.35, 2))


class RailTest(unittest.TestCase):
    def test_widest_path_keeps_the_best_bottleneck(self):
        g = {"railways": [{"level": 1, "provinces": [1, 2, 3]}, {"level": 3, "provinces": [1, 4, 5, 3]}]}
        edges = logistics._rail_edges(g)
        best = logistics.widest_rail(edges, 1, {1, 2, 3, 4, 5})
        self.assertEqual(best[3][0], 3)                           # the longer level-3 line beats the level-1 one
        self.assertEqual(best[3][1], 3)
        self.assertEqual(logistics._rail_segment(best, 3), [(1, 4), (4, 5), (5, 3)])
        self.assertNotIn(6, best)

    def test_no_map_is_said(self):
        sit = fx.situation(extra=EXTRA)
        r = logistics.axis_supply(sit, "BBB")
        self.assertIn("error", r)


class DivisionFuelTest(unittest.TestCase):
    def test_fuel_once_per_battalion_with_the_own_mult(self):
        sit = fx.situation(state={"techs": ["e_inf", "t_a"]})
        data = divstats.Data(sit.game)
        data.units["e121_mot"] = {"width": 2.0, "support": False, "need": {"e121_truck": 36.0}, "factors": {},
                                  "max_strength": 10.0, "max_organisation": 30.0, "categories": set(), "types": set(),
                                  "logistic": {"own_equipment_fuel_consumption_mult": 0.5}, "transport": None,
                                  "affects_speed": True, "terrain_move": {}}
        data.units["e121_log"] = dict(data.units["e121_mot"], support=True, need={},
                                      logistic={"fuel_consumption_factor": -0.1})
        data.equipment["e121_truck"] = {"stats": {}, "logistic": {"fuel_consumption": 1.2, "maximum_speed": 12.0},
                                        "archetype": None, "year": 0.0, "parent": None, "type": None,
                                        "is_archetype": False, "module_mult": {}}
        div = divstats.division(data, ["e121_mot", "e121_mot"], ["e121_log"], {}, [])
        self.assertAlmostEqual(div["fuel"], round(2 * 1.2 * 0.5 * (1 - 0.1), 3))


@unittest.skipUnless(fx.REAL and os.path.isdir(os.path.join(fx.REAL, "common", "units")), "no installed game")
class RealGameTest(unittest.TestCase):
    def test_germany_poland_axes(self):
        sit = fx.real_situation("GER", "1939.9.1", bookmark="1936.1.1")
        r = logistics.axis_supply(sit, "POL", div_supply=2.3, divisions=3)
        axes = {a["enemy_state"]: a for a in r["axes"]}
        self.assertGreaterEqual(len(axes), 5)
        self.assertTrue(all(a["supply"] >= 0 for a in axes.values()))
        fed = [s for a in axes.values() for s in a["sources"] if "port" in (s.get("fed_from") or "")]
        self.assertTrue(fed)                                       # East Prussia: fed through its port
        self.assertTrue(any(a["advance"] and a["advance"][0]["best"] < max(a["flows"].values(), default=9)
                            for a in axes.values() if a["flows"]))
        self.assertGreater(r["capital_supply"], 5)

    def test_germany_fuel(self):
        sit = fx.real_situation("GER", "1939.9.1", bookmark="1936.1.1")
        r = logistics.fuel(sit, [{"name": "armour", "count": 6, "fuel": 24.0}], war_date="1939.9.1", war_days=30)
        self.assertGreater(r["capacity"], 50000)
        self.assertGreater(r["gain_hour"], 2)
        self.assertGreater(r["days_carried"], 0)
        self.assertIn("AIR_SUPERIORITY", logistics.mission_costs(sit))


if __name__ == "__main__":
    unittest.main()
