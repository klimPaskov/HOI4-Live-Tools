"""The rolling planner: keeps a campaign plan filled and correct while the game runs (docs/ROLLING_PLAN.md).

    state      the game state from the runtime's read tools and its change feed, as one compact dict
    horizon    game days of planned work left in each queue the plan owns
    triggers   when the plan needs an update (horizon low, checkpoints, schedule, deviations, wars), with
               cooldowns and hysteresis
    ledger     what was tried and how it ended
    patch      the update format (extend, replace a tail, set, add) and the strategy/detail split
    store      the plan file: versions, compare-and-swap, history, rollback
    budget     the model-time and token ceiling per wall hour

Stdlib only, Python 3.9+. Nothing here touches the game: every read comes in as a tool body.
"""
