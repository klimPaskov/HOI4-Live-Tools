"""hoi4facts: facts from the effective HOI4 game content (install + enabled mods), stdlib only.

Modules:
    script     Clausewitz reader (parse, get, get1), static triggers (Ctx, trigger), run_effects,
               history_walk (dated history blocks up to a start date)
    content    mod layering like native modlayer.rs: Content(game_dir, mods), mods_from_userdir,
               parse_descriptor, parse_enabled_mods, effective_files; events/ included
    game       Game(root, mods): technologies, focus trees, country history, states, VPs,
               OOBs, doctrines, events, ideas, decisions, buildings, equipment
    tech       eligible(tech, done, rule="all"|"any"), missing, multi_parent, readings
    adjacency  province adjacency from map/provinces.bmp + adjacencies.csv
    validate   plan checks (scripts/validate_plan.py is the CLI)

Callers outside scripts/ put the scripts folder on sys.path and `import hoi4facts`.
"""
from .content import Content, Mod, effective_files, mods_from_userdir, parse_descriptor, parse_enabled_mods
from .game import Game
from .script import Ctx, get, get1, history_walk, parse, run_effects, trigger, trigger_one
from .tech import eligible

__all__ = [
    "Content", "Mod", "effective_files", "mods_from_userdir", "parse_descriptor", "parse_enabled_mods",
    "Game", "Ctx", "get", "get1", "history_walk", "parse", "run_effects", "trigger", "trigger_one",
    "eligible",
]
