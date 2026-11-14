"""Production lines are counted in the pool that builds them: military factories for land and air
equipment, dockyards for naval equipment (ships and convoys). Runs on a small synthetic game."""
import os
import shutil
import sys
import tempfile
import unittest

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.join(ROOT, "scripts"))

from hoi4facts import derive, lint_plan  # noqa: E402
from hoi4facts.game import Game  # noqa: E402

BUILDINGS = """buildings = {
	arms_factory = { base_cost = 7200 military_production = 1 }
	industrial_complex = { base_cost = 10800 general_production = 1 }
	dockyard = { base_cost = 6400 naval_production = 1 }
	modded_shipyard = { base_cost = 9000 naval_production = 2 }
}
"""
EQUIPMENT = """equipments = {
	infantry_equipment = { is_archetype = yes type = infantry }
	infantry_equipment_1 = { archetype = infantry_equipment }
	convoy = { is_archetype = yes type = convoy }
	convoy_1 = { archetype = convoy }
	ship_hull_light = { is_archetype = yes type = screen_ship interface_category = interface_category_screen_ships }
	ship_hull_light_1 = { archetype = ship_hull_light }
	fighter_equipment = { is_archetype = yes type = fighter interface_category = interface_category_air }
	fighter_equipment_1 = { archetype = fighter_equipment }
}
"""


class FactoryPools(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.dir = tempfile.mkdtemp(prefix="pools")
        for rel, text in (("common/buildings/00_buildings.txt", BUILDINGS),
                          ("common/units/equipment/eq.txt", EQUIPMENT)):
            path = os.path.join(cls.dir, *rel.split("/"))
            os.makedirs(os.path.dirname(path), exist_ok=True)
            with open(path, "w") as fh:
                fh.write(text)
        cls.game = Game(cls.dir, cache_dir=tempfile.gettempdir())

    @classmethod
    def tearDownClass(cls):
        shutil.rmtree(cls.dir, ignore_errors=True)

    def lint(self, production):
        lt = lint_plan.Lint.__new__(lint_plan.Lint)
        lt.game, lt.plan, lt.start = self.game, {"production": production}, "1936.1.1"
        return lt

    def test_building_kinds_name_the_dockyards(self):
        kinds = derive.building_kinds(self.game)
        self.assertEqual(kinds["military"], {"arms_factory"})
        self.assertEqual(kinds["naval"], {"dockyard", "modded_shipyard"})  # read from naval_production

    def test_naval_equipment_is_built_by_dockyards(self):
        lt = self.lint([])
        pools = {eq: lt.factory_pool(eq) for eq in ("infantry_equipment_1", "fighter_equipment_1", "convoy_1",
                                                    "ship_hull_light_1")}
        self.assertEqual(pools, {"infantry_equipment_1": "military", "fighter_equipment_1": "military",
                                 "convoy_1": "dockyards", "ship_hull_light_1": "dockyards"})

    def test_lines_split_by_pool_and_date(self):
        lt = self.lint([{"equipment": "infantry_equipment_1", "factories": 12},
                        {"equipment": "fighter_equipment_1", "factories": 8},
                        {"equipment": "convoy_1", "factories": 7},
                        {"equipment": "ship_hull_light_1", "factories": 3, "from": "1936.6.1"}])
        day0 = lt.pool_lines(derive.day("1936.1.1"))
        self.assertEqual(sum(day0["military"].values()), 20)  # the convoys take none of the 20
        self.assertEqual(day0["dockyards"], {"convoy_1": 7})
        later = lt.pool_lines(derive.day("1936.6.1"))
        self.assertEqual(later["dockyards"], {"convoy_1": 7, "ship_hull_light_1": 3})


if __name__ == "__main__":
    unittest.main()
