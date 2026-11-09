"""hoi4facts: facts from the effective HOI4 game content (install + enabled mods), stdlib only.

Modules:
    script     Clausewitz reader (parse, get, get1), static triggers (Ctx, trigger), run_effects,
               history_walk (dated history blocks up to a start date)
    content    mod layering like native modlayer.rs: Content(game_dir, mods), mods_from_userdir,
               parse_descriptor, parse_enabled_mods, effective_files; events/ included
    game       Game(root, mods): technologies, focus trees, country history, states, VPs,
               OOBs, doctrines, events, ideas, decisions, buildings, equipment
    tech       eligible(tech, done, rule="all"|"any"), missing, multi_parent, readings
    mapgraph   province graph (adjacency, rivers, supply, routes); Game.map_graph() layers it
    schema_check  JSON Schema subset validator for plan.schema.json
    validate   plan checks (scripts/validate_plan.py is the CLI)
    derive     computed facts: 365-day calendar, defines, localisation, front distance, border
               states, surrender threshold, commanders, scenario mission timers
    lint_plan  strategy lint of a plan (design 4.2); CLI python3 scripts/hoi4facts/lint_plan.py PLAN.json
    defines    common/defines Lua reader with file:line of every key (dotted mod overrides too)
    world      World(game, tag, date, dlcs, mode): sourced country, state, idea, law, character,
               OOB, decision and VP/surrender facts at a start date (FAIR levels)
    focus      the focus tree that loads (DLC-aware), per-focus detail, outline, timeline
    research   techs with DLC folders, research calendar (both parent readings), plan timeline
    scenario   scenario hooks (wars, on_actions, outcome flags, missions, deadlines, goals,
               thresholds), events, grep, ai_strategy
    theatre    per-enemy borders, hubs, railways, ports, infra-0 states, routes to enemy VPs
    dossier    build / render_md / write of RUN/dossier (design 2.4)
    facts      the strategist's facts CLI (python3 scripts/hoi4facts/facts.py --help)
    calc       build-order, equipment and pp calculators (python3 scripts/hoi4facts/calc.py --help)

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
