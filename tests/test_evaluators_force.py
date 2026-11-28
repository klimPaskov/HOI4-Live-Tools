"""evaluators/force.py: force needed and the land doctrines compared; the installed game when present
(Germany against Poland, Hungary against Czechoslovakia)."""
import os
import sys
import unittest

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import evaluators_fixture as fx  # noqa: E402

from hoi4facts.evaluators import force  # noqa: E402


TRACKS = """
infantry = { mastery = { multiplier = 1.0 categories = { category_all_infantry } } }
support = { mastery = { multiplier = 1.0 categories = { category_artillery } } }
"""
SUBDOCTRINES = """
e_hold = { track = infantry xp_cost = 100 available = { always = yes }
    infantry = { defense = 0.2 }
    rewards = { r1 = { infantry = { soft_attack = 0.1 } } r2 = { infantry = { breakthrough = 0.5 } } } }
e_push = { track = infantry xp_cost = 100 available = { always = yes } xor = { e_guns }
    infantry = { breakthrough = 0.3 soft_attack = 0.2 }
    rewards = { r1 = { infantry = { soft_attack = 0.1 } } } }
e_locked = { track = infantry xp_cost = 100 available = { has_completed_special_project = e_project }
    infantry = { defense = 5.0 } }
e_guns = { track = support xp_cost = 100 available = { always = yes } xor = { e_push }
    infantry = { soft_attack = 0.05 } }
e_plain = { track = support xp_cost = 100 available = { always = yes }
    infantry = { soft_attack = 0.01 } }
"""


UNITS = """
sub_units = {
    infantry = { group = infantry combat_width = 2 manpower = 1000 max_organisation = 30 max_strength = 10
        need = { infantry_equipment = 100 } }
}
"""
EQUIPMENT = """
equipments = {
    infantry_equipment = { is_archetype = yes build_cost_ic = 0.43 soft_attack = 3 hard_attack = 0.5 defense = 20
        breakthrough = 2 }
    infantry_equipment_1 = { year = 1936 archetype = infantry_equipment build_cost_ic = 0.5 soft_attack = 4
        hard_attack = 0.5 defense = 22 breakthrough = 3 }
}
"""


class SubdoctrineFixtureTest(unittest.TestCase):
    def test_one_per_track_by_value_with_gate_and_xor(self):
        sit = fx.situation(state={"techs": ["e_inf"]}, extra={
            "common/doctrines/tracks/land_doctrine_tracks.txt": TRACKS,
            "common/doctrines/subdoctrines/land/e121.txt": SUBDOCTRINES,
            "common/units/infantry.txt": UNITS, "common/units/equipment/zz_e121_inf.txt": EQUIPMENT})
        r = force.subdoctrines(sit, ["BBB"], reach=1)
        tracks = {t["track"]: t for t in r["tracks"]}
        self.assertEqual(set(tracks), {"infantry", "support"})
        inf = {c["subdoctrine"]: c for c in tracks["infantry"]["candidates"]}
        self.assertIsNotNone(inf["e_locked"]["gate"])                      # a special project: never chosen
        self.assertNotEqual(tracks["infantry"]["chosen"], "e_locked")
        self.assertGreater(inf["e_hold"]["results"]["all"]["value"], inf["e_hold"]["results"]["base"]["value"])
        chosen = {t["chosen"] for t in r["tracks"]}
        self.assertFalse({"e_push", "e_guns"} <= chosen)                   # xor across tracks
        self.assertIn(tracks["support"]["chosen"], ("e_guns", "e_plain"))


@unittest.skipUnless(fx.real_situation("GER") is not None, "installed game not found (HOI4_EVAL_GAME / HOI4_DIR)")
class ForceRealGameTest(unittest.TestCase):
    def test_force_and_doctrines_germany_poland(self):
        sit = fx.real_situation("GER")
        r = force.needed(sit, "POL", "1939.9.1")
        self.assertGreater(r["enemy"]["divisions"], r["enemy"]["divisions_start"])
        self.assertEqual(r["front_provinces"], 25)
        self.assertGreaterEqual(r["total"], r["front_provinces"])
        d = force.doctrines(sit, ["POL"])
        names = {x["doctrine"] for x in d["rows"]}
        self.assertTrue({"new_mobile_warfare", "superior_firepower", "grand_battleplan", "mass_assault"} <= names)
        self.assertEqual(d["held"], ["new_mobile_warfare"])
        rows = {x["doctrine"]: x for x in d["rows"]}
        gbp = rows["grand_battleplan"]["attack"]["attacker"]["base"]["T14"]
        nmw = rows["new_mobile_warfare"]["attack"]["attacker"]["base"]["T14"]
        self.assertAlmostEqual(gbp["planning"], 0.3 + 0.1 - 0.01 * 14 / 2)      # PLANNING_MAX + max_planning - decay
        self.assertGreater(gbp["R"], nmw["R"])                                   # the higher cap, before milestones
        self.assertIn("coordination_bonus", rows["grand_battleplan"]["unvalued"]["all milestones"])

    def test_subdoctrines_germany_poland(self):
        r = force.subdoctrines(fx.real_situation("GER", "1939.9.1", bookmark="1936.1.1"), ["POL"],
                               grand="grand_battleplan")
        self.assertEqual(r["grand"], ["grand_battleplan"])
        self.assertEqual(len(r["tracks"]), 4)
        self.assertTrue(all(t["chosen"] for t in r["tracks"]))

    def test_force_minor(self):
        r = force.needed(fx.real_situation("HUN"), "CZE", "1938.1.1")
        self.assertGreater(r["total"], 0)

if __name__ == "__main__":
    unittest.main()
