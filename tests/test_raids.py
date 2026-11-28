"""raids.py: raid types from common/raids for any country and mod, the success formula and the prerequisites.

A mini game folder holds one category gated by a DLC and a tech, a country raid (tag and DLC allowed, a focus to see
it, a province target, battalions, transports) and a generic air raid (bombers by equipment type, a building target),
plus a raid of another tag that must not be listed.
"""
import os
import sys
import tempfile
import unittest

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.join(ROOT, "scripts"))

from hoi4facts import raids  # noqa: E402
from hoi4facts.game import Game  # noqa: E402
from hoi4facts.script import Ctx  # noqa: E402

CATS = """
categories = {
    para_raids = { visible = { has_dlc = "Raids DLC" } available = { has_tech = paratroopers } }
    air_raids = { visible = { has_dlc = "Raids DLC" } }
}
"""
TYPES = """
types = {
    AAA_fort_raid = {
        category = para_raids
        days_to_prepare = 30
        command_power = 20
        fire_only_once = yes
        allowed = { has_dlc = "Raids DLC" original_tag = AAA }
        visible = { has_completed_focus = AAA_air_force has_tech = paratroopers }
        launchable = { has_war_with = BBB }
        unit_requirements = { battalion_types = { paratrooper = { min = 2 } } }
        essential_equipment = { transport_plane_equipment = 5 }
        additional_equipment = { support_equipment = 100 transport_plane_equipment = 3 }
        target_type = { province = 11519 }
        starting_point = { types = { air_base } }
        success_factors = {
            success = {
                base = 0.75
                experience = { weight = 0.25 start_weight = -0.2 reference = 1.0 }
                air_superiority = { reference = 1 start_reference = 0.5 weight = 0.2 }
                enemy_units = { reference = 2 weight = -0.4 }
            }
        }
        success_levels = {
            success = {
                victim_effects = {
                    var:target_state = { damage_building = { type = bunker damage = 4 province = var:ROOT.target_province } }
                }
                actor_effects = { raid_add_unit_experience = 0.2 }
            }
        }
    }
    bomb_facility = {
        category = air_raids
        days_to_prepare = 90
        command_power = 25
        unit_requirements = { equipment = { type = { tactical_bomber } amount = { min = 70 } } }
        target_type = { building = { tags = facility } }
        success_factors = { success = { base = 0.65 } disaster = { base = 0.1 } critical = { base = 0.5 } }
    }
    CCC_only = {
        category = air_raids
        allowed = { original_tag = CCC }
        success_factors = { success = { base = 0.5 } }
    }
}
"""


def mini_game():
    root = tempfile.mkdtemp(prefix="raids_")
    for rel, text in {"common/raids/categories/c.txt": CATS, "common/raids/r.txt": TYPES}.items():
        p = os.path.join(root, rel)
        os.makedirs(os.path.dirname(p), exist_ok=True)
        with open(p, "w") as f:
            f.write(text)
    return Game(root, cache_dir=root)


class RaidsTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.r = raids.Raids(mini_game(), cooldown=365)
        cls.ctx = Ctx("AAA", {"Raids DLC"})

    def test_types_and_categories_load(self):
        self.assertEqual(set(self.r.types), {"AAA_fort_raid", "bomb_facility", "CCC_only"})
        self.assertEqual(set(self.r.categories), {"para_raids", "air_raids"})

    def test_row_reads_requirements_equipment_targets_and_outcomes(self):
        row = self.r.row("AAA_fort_raid")
        self.assertEqual(row["command_power"], 20)
        self.assertEqual(row["days_to_prepare"], 30)
        self.assertTrue(row["fire_only_once"])
        self.assertEqual(row["cooldown_days"], 365)
        self.assertEqual(row["units"], [{"battalions": {"paratrooper": 2.0}}])
        # the larger of essential and additional counts
        self.assertEqual(row["equipment"], {"transport_plane_equipment": 5.0, "support_equipment": 100.0})
        self.assertEqual(row["targets"], [{"province": ["11519"]}])
        self.assertEqual(row["starting_points"], ["air_base"])
        self.assertIn("damage_building type = bunker damage = 4 province = var:ROOT.target_province",
                      row["outcomes"]["success"]["victim"])
        bomb = self.r.row("bomb_facility")
        self.assertEqual(bomb["units"][0]["equipment"]["types"], ["tactical_bomber"])
        self.assertEqual(bomb["units"][0]["equipment"]["min"], 70)
        self.assertEqual(bomb["targets"][0]["building"], "facility")

    def test_success_formula_is_linear_and_clamped(self):
        f = self.r.row("AAA_fort_raid")["success_factors"]["success"]
        # experience 0.5: -0.2 + 0.45 x 0.5 = 0.025; air 0.75: half of 0.2 = 0.1; one enemy division: -0.2
        p, parts, missing = raids.chance(f, {"experience": 0.5, "air_superiority": 0.75, "enemy_units": 1})
        self.assertAlmostEqual(parts["experience"], 0.025)
        self.assertAlmostEqual(parts["air_superiority"], 0.1)
        self.assertAlmostEqual(parts["enemy_units"], -0.2)
        self.assertAlmostEqual(p, 0.675)
        self.assertEqual(missing, [])
        # below start_reference: start_weight (0); far above reference: full weight; total clamped to 1
        self.assertEqual(raids.chance(f, {"air_superiority": 0.2})[1]["air_superiority"], 0.0)
        self.assertEqual(raids.chance(f, {"experience": 5, "air_superiority": 3})[0], 0.75 + 0.25 + 0.2 > 1 and 1.0)
        # an input not given adds no term and is listed
        self.assertEqual(raids.chance(f, {})[2], ["experience", "air_superiority", "enemy_units"])

    def test_outcome_probabilities_draw_disaster_first(self):
        sf = self.r.row("bomb_facility")["success_factors"]
        o = raids.outcome_probabilities(sf, {})
        self.assertAlmostEqual(o["disaster"], 0.1)
        self.assertAlmostEqual(o["success_or_better"], 0.9 * 0.65)
        self.assertAlmostEqual(o["critical"], 0.9 * 0.65 * 0.5)

    def test_rows_for_a_country_list_its_raids_and_missing_prerequisites(self):
        rows = {r["id"]: r for r in self.r.rows(self.ctx, techs=set(), focuses=set())}
        self.assertEqual(set(rows), {"AAA_fort_raid", "bomb_facility"})  # CCC_only is another tag's
        self.assertEqual(rows["AAA_fort_raid"]["needs"], {"tech": ["paratroopers"], "focus": ["AAA_air_force"]})
        self.assertTrue(rows["AAA_fort_raid"]["allowed_known"])
        held = {r["id"]: r for r in self.r.rows(self.ctx, techs={"paratroopers"}, focuses={"AAA_air_force"})}
        self.assertEqual(held["AAA_fort_raid"]["needs"], {"tech": [], "focus": []})
        # without the DLC the country raid is not allowed
        self.assertNotIn("AAA_fort_raid", {r["id"] for r in self.r.rows(Ctx("AAA", set()))})


if __name__ == "__main__":
    unittest.main()
