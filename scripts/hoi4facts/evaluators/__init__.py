"""General evaluators: decisions computed from the game's files and the live situation, for any country or mod.

Every evaluator is a pure function over a Situation (context.Situation: the effective game files of the install and
its mods, one country at one date, and an optional live-state snapshot that overrides what the files say at the
bookmark). Nothing here names a country: Germany is a test case, not a path. Each result is a dict with the answer,
the reasoning (`why`: the numbers and the game keys they came from) and what the evaluator left out (`left_out`).

    from hoi4facts.evaluators import context, design, template, research, production, construction, force, focus
    sit = context.Situation.load(game_dir, "GER", "1936.1.1", mods=(), state=None)
    design.best(sit, "tank_main")              # 1. designer-based equipment by role against the enemy's designs
    template.best(sit, "line")                 # 2. division templates by role, width by the theatre's terrain
    research.picks(sit)                        # 3. the next technologies per slot by value per day
    production.mix(sit, force)                 # 4. factories per line by shortfall and time to need
    construction.evaluate(sit, war_date=...)   # 5. civilian against military break-even, where to build, when to stop
    force.needed(sit, enemy) / force.doctrines(sit)   # 6. force against an enemy; doctrine path comparison
    focus.next(sit, goals=...)                 # 7. the next focus by value per day under the player's goals

The CLI is scripts/hoi4facts/evaluate.py: `evaluate.py <what> --country TAG [--date D] [--mod PATH] [--state JSON]`.
Stdlib only, Python 3.9+.
"""
