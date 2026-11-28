"""evaluators/variant.py (cost clicks, base and convert) and evaluators/lines.py (one JSON object per decision); the
installed game when present (Germany's mechanised cost clicks and its medium tank base-and-convert)."""
import contextlib
import io
import json
import os
import sys
import tempfile
import unittest

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import evaluators_fixture as fx  # noqa: E402

from hoi4facts.evaluators import lines, variant  # noqa: E402

CARRIER = """
equipments = {
    e_carrier = { is_archetype = yes is_buildable = no type = mechanized year = 1936
        upgrades = { e_cost_up e_rel_up }
        maximum_speed = 8 reliability = 0.8 build_cost_ic = 8 defense = 20 }
    e_carrier_1 = { archetype = e_carrier year = 1936 }
}
"""
CARRIER_UPGRADES = """
upgrades = {
    e_cost_up = { max_level = 5 cost = land build_cost_ic = -0.1 reliability = -0.05 }
    e_rel_up = { max_level = 5 cost = land reliability = 0.1 build_cost_ic = 0.05 }
}
"""
CARRIER_TECH = """
technologies = {
    e_carrier_tech = { start_year = 1936 research_cost = 1 enable_equipments = { e_carrier_1 } categories = { armor } }
}
"""
CONVERT_MODULES = fx.MODULES.replace(
    "e_cannon = { category = e_small_gun allow_equipment_type = anti_tank",
    "e_cannon = { category = e_small_gun allow_equipment_type = anti_tank "
    "can_convert_from = { module = e_mg convert_cost_ic = 0.5 }")
DEFINES = fx.DEFINES + """
NDefines.NMilitary.LAND_EQUIPMENT_BASE_COST = 10
NDefines.NMilitary.LAND_EQUIPMENT_RAMP_COST = 5
NDefines.NProduction.BASE_LAND_EQUIPMENT_CONVERSION_IC_COST_FACTOR = 0.9
NDefines.NProduction.EQUIPMENT_MODULE_ADD_XP_COST = 5.0
NDefines.NProduction.EQUIPMENT_MODULE_REPLACE_XP_COST = 6.0
NDefines.NProduction.EQUIPMENT_MODULE_CONVERT_XP_COST = 3.0
NDefines.NProduction.EQUIPMENT_MODULE_REMOVE_XP_COST = 1.0
"""
EXTRA = {"common/units/equipment/e119_carrier.txt": CARRIER,
         "common/units/equipment/upgrades/e119_carrier.txt": CARRIER_UPGRADES,
         "common/technologies/e119_carrier.txt": CARRIER_TECH,
         "common/units/equipment/modules/e112_modules.txt": CONVERT_MODULES,
         "common/defines/zz_e112.lua": DEFINES}


class VariantFixtureTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.sit = fx.situation(state={"techs": ["e_carrier_tech", "e_tank_1", "e_tank_2"]}, extra=EXTRA)

    def test_cost_clicks_multiply_and_ramp(self):
        r = variant.cost_clicks(self.sit, "e_carrier_1")
        self.assertEqual(len(r), 1)                                     # e_rel_up raises cost: not a cost click
        x = r[0]
        self.assertEqual(x["upgrade"], "e_cost_up")
        top = x["levels"][-1]
        self.assertEqual(top["level"], 5)
        self.assertAlmostEqual(top["ic"], 4.0)                          # 8 x (1 - 5 x 0.1): the legacy rule multiplies
        self.assertAlmostEqual(top["reliability"], 0.6)                 # 0.8 x (1 - 5 x 0.05)
        self.assertAlmostEqual(top["xp"], 10 + 15 + 20 + 25 + 30)      # base + levels taken x ramp
        self.assertEqual(x["chosen"], 5)                                # value per IC still rises at 5 (no org define)

    def test_conversion_cost_from_the_rules(self):
        b = {"turret_type_slot": "e_turret_small", "main_armament_slot": "e_mg", "engine_type_slot": "e_engine_petrol"}
        f = dict(b, main_armament_slot="e_cannon", special_type_slot_1="e_radio")
        ic, xp, rows = variant.conversion(self.sit, "mt_chassis_1", b, {}, "mt_chassis_1", f, {"e_armor_up": 2})
        # 0.9 x chassis 3 IC + cannon converted from the MG 0.5 + radio new 1 + 2 armour levels x 0.2
        self.assertAlmostEqual(ic, 0.9 * 3 + 0.5 + 1 + 0.4)
        self.assertAlmostEqual(xp, 3.0 + 5.0)                           # one convert, one add
        self.assertTrue(any(r.get("how") == "can_convert_from" for r in rows))

    def test_lines_shape_and_run_file(self):
        data = {"clicks": variant.cost_clicks(self.sit, "e_carrier_1"), "country": "AAA", "date": "1936.1.1"}
        rows = lines.lines("variant", data, ["variant", "--country", "AAA", "--lines"])
        self.assertEqual(len(rows), 1)
        row = rows[0]
        for k in ("decision", "chosen", "value", "unit", "alternatives", "reasoning", "inputs", "command", "source"):
            self.assertIn(k, row)
        self.assertEqual(row["source"], "computed")
        self.assertNotIn("--lines", row["command"])
        self.assertTrue(all({"option", "value", "terms"} <= set(a) for a in row["alternatives"]))
        unknown = lines.lines("no_adapter", {"x": 1})
        self.assertEqual(unknown[0]["inputs"], {"x": 1})               # never silently nothing
        run = tempfile.mkdtemp(prefix="e119_run_")
        mod = fx.write_mod(EXTRA)
        state = os.path.join(mod, "state.json")
        with open(state, "w") as fh:
            fh.write('{"techs": ["e_carrier_tech"]}')
        out = io.StringIO()
        with contextlib.redirect_stdout(out):
            from hoi4facts import evaluate
            evaluate.main(["variant", "--country", "AAA", "--game", fx.GAME, "--mod", mod, "--state", state,
                           "--dlc", "none", "--run", run])
        self.assertIn("VARIANT cost clicks", out.getvalue())
        with open(os.path.join(run, "evaluations.jsonl")) as fh:
            got = [json.loads(x) for x in fh if x.strip()]
        self.assertTrue(got and got[0]["evaluator"] == "variant")


@unittest.skipUnless(fx.real_situation("GER") is not None, "installed game not found (HOI4_EVAL_GAME / HOI4_DIR)")
class VariantRealGameTest(unittest.TestCase):
    def test_germany_cheap_mechanised_and_base_convert(self):
        sit = fx.real_situation("GER", "1940.1.1", bookmark="1936.1.1")
        mech = [x for x in variant.cost_clicks(sit) if x["equipment"].startswith("mechanized_equipment")]
        self.assertTrue(mech)
        top = [r for r in mech[0]["levels"] if r["level"] == mech[0]["chosen"]][0]
        self.assertEqual(mech[0]["chosen"], 5)                          # the owner's screenshot: every click on cost
        self.assertAlmostEqual(top["ic"], 4.0)                          # his 4.00
        self.assertAlmostEqual(top["reliability"], 0.6)                 # his 60%
        sit = fx.real_situation("GER", "1939.5.1", bookmark="1936.1.1")
        r = variant.base_and_convert(sit, "medium_tank_chassis", "1941.6.1", enemies=["SOV"])
        self.assertEqual(r["base"]["legal"], [])
        self.assertLess(r["conversion_ic"], r["ic_final"])
        self.assertGreater(r["recovered_per_base"], 0)


if __name__ == "__main__":
    unittest.main()
