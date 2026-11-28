"""Shared fixture for the evaluator tests: the dossier fixture game (tests/fixtures/dossier/game: AAA at war with BBB)
with a small mod layered over it that adds a tank designer family, its modules, upgrades and techs, an AI design
template for BBB, and the economy the construction, research, production and focus evaluators read. Every file is
written here in the game's formats; nothing is copied from the game.
"""
import os
import sys
import tempfile

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.join(ROOT, "scripts"))

from hoi4facts.content import Mod  # noqa: E402
from hoi4facts.game import Game  # noqa: E402

GAME = os.path.join(ROOT, "tests", "fixtures", "dossier", "game")
REAL = os.environ.get("HOI4_EVAL_GAME") or os.environ.get("HOI4_DIR") or ""

TANKS = """
equipments = {
    mt_chassis = { is_archetype = yes is_buildable = no type = armor year = 1930
        module_slots = {
            turret_type_slot = { required = yes allowed_module_categories = { e_turret } }
            main_armament_slot = { required = yes allowed_module_categories = { e_small_gun } }
            engine_type_slot = { required = yes allowed_module_categories = { e_engine } }
            special_type_slot_1 = { required = no allowed_module_categories = { e_special e_radio } }
            special_type_slot_2 = { required = no allowed_module_categories = { e_special e_radio } }
        }
        module_count_limit = { category = e_radio count < 2 }
        maximum_speed = 4 build_cost_ic = 2 reliability = 0.5 armor_value = 20 hardness = 0.85 }
    mt_chassis_1 = { archetype = mt_chassis year = 1936 module_slots = inherit
        upgrades = { e_engine_up e_armor_up }
        maximum_speed = 5 build_cost_ic = 3 reliability = 0.8 armor_value = 30
        module_add_stats_multiplier = { soft_attack = -0.1 hard_attack = -0.1 } }
    mt_chassis_2 = { archetype = mt_chassis parent = mt_chassis_1 year = 1940 module_slots = inherit
        maximum_speed = 6 build_cost_ic = 4 reliability = 1 armor_value = 40 }
}
"""
MODULES = """
equipment_modules = {
    e_turret_small = { category = e_turret allowed_module_categories = { main_armament_slot = { e_small_gun } }
        add_stats = { build_cost_ic = 1 breakthrough = 10 } }
    e_turret_big = { category = e_turret allowed_module_categories = { main_armament_slot = { e_small_gun e_big_gun } }
        add_stats = { build_cost_ic = 2 breakthrough = 20 maximum_speed = -0.5 } }
    e_mg = { category = e_small_gun add_stats = { soft_attack = 10 ap_attack = 5 build_cost_ic = 1 } }
    e_cannon = { category = e_small_gun allow_equipment_type = anti_tank
        add_stats = { soft_attack = 9 hard_attack = 8 ap_attack = 40 build_cost_ic = 2 } }
    e_howitzer = { category = e_big_gun forbid_equipment_type_exact_match = armor allow_equipment_type = artillery
        add_stats = { soft_attack = 25 hard_attack = 2 ap_attack = 10 build_cost_ic = 3 } }
    e_engine_petrol = { category = e_engine add_stats = { build_cost_ic = 1 } }
    e_engine_diesel = { category = e_engine add_stats = { build_cost_ic = 2 reliability = 0.2 } }
    e_radio = { category = e_radio add_stats = { build_cost_ic = 1 } multiply_stats = { breakthrough = 0.2 } }
    e_plate = { category = e_special add_stats = { armor_value = 10 maximum_speed = -0.5 build_cost_ic = 1 } }
}
"""
UPGRADES = """
upgrades = {
    e_engine_up = { max_level = 10 level_requirements = { 5 = { has_tech = e_engine_tech } }
        reliability = -0.02 add_stats = { maximum_speed = 0.2 build_cost_ic = 0.1 } }
    e_armor_up = { max_level = 10 level_requirements = { 5 = { has_tech = e_armor_tech } }
        reliability = -0.01 add_stats = { armor_value = 3 breakthrough = 1 maximum_speed = -0.2 build_cost_ic = 0.2 } }
}
"""
TECHS = """
technologies = {
    e_tank_1 = { start_year = 1936 research_cost = 1.5
        enable_equipments = { mt_chassis_1 }
        enable_equipment_modules = { e_turret_small e_mg e_engine_petrol e_radio }
        path = { leads_to_tech = e_tank_2 research_cost_coeff = 1 }
        categories = { armor } }
    e_tank_2 = { start_year = 1940 research_cost = 2
        enable_equipments = { mt_chassis_2 }
        enable_equipment_modules = { e_turret_big e_cannon e_howitzer e_engine_diesel e_plate }
        categories = { armor } }
    e_armor_tech = { start_year = 1938 research_cost = 1 categories = { armor } }
    e_engine_tech = { start_year = 1938 research_cost = 1 categories = { armor } }
}
"""
AI = """
BBB_tanks = {
    category = land
    blocked_for = {}
    available_for = { BBB }
    roles = { land_medium_tank }
    priority = { factor = 1 }
    bbb_basic = {
        priority = { factor = 1 }
        target_variant = { match_value = 1 type = mt_chassis_1
            modules = { turret_type_slot = e_turret_small main_armament_slot = { any_of = { e_cannon e_mg } }
                engine_type_slot = e_engine_petrol }
            upgrades = { e_armor_up = { base = 2 } } }
    }
}
"""
DEFINES = """
NDefines.NMilitary.PIERCING_THRESHOLDS = { 1.00 0.75 0.50 0.00 }
NDefines.NMilitary.PIERCING_THRESHOLD_DAMAGE_VALUES = { 1.00 0.80 0.65 0.50 }
NDefines.NMilitary.LAND_COMBAT_ORG_DICE_SIZE = 4
NDefines.NMilitary.LAND_COMBAT_ORG_ARMOR_ON_SOFT_DICE_SIZE = 6
NDefines.NMilitary.RELIABILITY_ORG_REGAIN = -0.3
"""

FILES = {
    "common/units/equipment/e112_tanks.txt": TANKS,
    "common/units/equipment/modules/e112_modules.txt": MODULES,
    "common/units/equipment/upgrades/e112_upgrades.txt": UPGRADES,
    "common/technologies/e112_tanks.txt": TECHS,
    "common/ai_equipment/e112_bbb.txt": AI,
    "common/defines/zz_e112.lua": DEFINES,
}


def write_mod(extra=None):
    root = tempfile.mkdtemp(prefix="e112_mod_")
    for rel, text in dict(FILES, **(extra or {})).items():
        p = os.path.join(root, rel)
        os.makedirs(os.path.dirname(p), exist_ok=True)
        with open(p, "w", encoding="utf-8") as fh:
            fh.write(text)
    return root


def game(extra=None):
    mod = write_mod(extra)
    return Game(GAME, [Mod(mod, name="e112 fixture")], cache_dir=tempfile.gettempdir())


def situation(tag="AAA", date="1936.1.1", state=None, extra=None):
    from hoi4facts.evaluators.context import Situation
    return Situation.load(game(extra), tag, date, dlcs=(), state=state)


def real_situation(tag, date="1936.1.1", bookmark=None, state=None):
    """A situation on the installed game (local tests only); None without one."""
    if not REAL or not os.path.isdir(os.path.join(REAL, "common", "units")):
        return None
    from hoi4facts.evaluators.context import Situation
    return Situation.load(REAL, tag, date, bookmark=bookmark, state=state)
