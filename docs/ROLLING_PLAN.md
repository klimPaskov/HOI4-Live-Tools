# Rolling plan: a thorough opening, then continuous updates while the game runs

A plan written once before the game cannot see how the campaign unfolds. In ten Italy-Ethiopia shakedowns a
plan that passed validation, lint and the critic (29/32) still lost: divisions at about 30% equipment,
artillery stock zero, political power idle for months, a research list that ran out in November and an
advance that stalled in the highlands. Every one of those was visible in the game state weeks before it
mattered, and the executor even raised most of them on its change feed (`stockpile_low`,
`equipment_fill_low`, `political_power_idle`, `checkpoint_missed`, `research_slot_idle`). Nobody acted on
them: `hoi4_plan {op:"request_revision"}` went to the feed and stopped there.

The rolling plan closes that loop. The opening plan keeps the whole campaign's strategy and details the
first horizon; a planner worker in the play supervisor reads the same state the executor reads, decides
when the plan needs more or different work, produces an update in the background, checks it, and swaps it
in atomically. The game never waits.

## 1. Decisions

| # | Decision | Why |
| --- | --- | --- |
| D1 | **Two layers in one plan.** Strategy keys (`goal`, `phases`, `checkpoints`, `war` axes and enemies, `force_posture`, `horizons[].intent`) are fixed by the opening and change only on a named strategy trigger. Detail keys (research, focus, production, construction, politics, training, air, doctrines, designs) change freely. | The executor follows details; strategy flip-flopping costs more than a late correction. |
| D2 | **The opening details one horizon.** `_meta.stop_date` is the detail horizon (about one game year, or the first major checkpoint). Later horizons are `horizons[]` entries: `{id, from, until, intent, targets}` with targets as numbers (divisions, factories, equipment fill, PP reserve, states held), no item lists. The opening keeps every gate (strict validation, lint, critic). | Item lists past the first year were guesses that the lint could not check against a state that did not exist yet; the update fills them from the real state. |
| D3 | **The planner worker lives in the play supervisor**, beside the executor, not in the native runtime. It reads through the runtime's read lane (no input lease, no input) and writes `RUN/plan.json`, which the executor already re-reads every tick. | Reads are free (0 to 30 ms). The executor re-reads the plan every tick, so an atomic file swap is an atomic plan swap. No change to the runtime is needed. |
| D4 | **"Horizon low" is computed by the planner, not raised by the executor.** For each owned queue the planner estimates the game days of planned work left from the plan, the state and the game's costs; below a set number of days it raises `horizon_low:<domain>`. The executor keeps its existing lagging alarms (`*_plan_exhausted`, `research_slot_idle`) and default policies (`politics_auto`, `production_priority`, the restricted research fallback, the scheduler) until the extension lands. | The lead time needs costs the executor does not hold (research days per tech, focus days, equipment need per template); the planner has them. A leading signal fires weeks before the slot is empty, which is what lets a model call finish in time. *Departure from the agreed design (which put the signal in the executor), for that reason.* |
| D5 | **Every update has a deterministic answer first.** For each trigger the planner builds a menu with the calculators (in-time techs ranked industry first, available focuses, equipment deficits in days of cover, PP spend options, free construction slots) and a default pick. The model only chooses and trades off within the menu; if it is off, over budget, slow or its patch fails the gates, the default pick is applied. | Queues never wait for a model. Numbers come from calculators; the model's tokens go to choices. |
| D6 | **An update is a patch, not a plan.** Allowed operations: extend a list, replace a list's not-yet-done tail, set a number, add an item with a new id. Ids of unchanged items are kept (the executor keys its done-memory on them). Strategy keys may appear only in an escalated update with a recorded reason. | Small packets, small outputs, and a diff a person can read. |
| D7 | **Gates: no new errors, plus checks anchored at now.** The candidate passes strict validation, and lint finds no rule with more errors than in the plan in force (the lint is anchored at the scenario start, so errors that come from the start state are in both and cancel); the planner's own anchored checks (research, focus, PP and equipment horizons from today's state) must hold. | The full lint compares against the start, which a mid-game plan has left behind; a ratchet keeps every rule meaningful without re-anchoring all 3,800 lines of lint. |
| D8 | **Atomic, versioned apply with rollback.** Compare-and-swap on the plan file's hash under the supervisor's lock (the player's `hoi4_plan set` goes through the same lock), `RUN/plans/plan.v<N>.json` history, `_meta.version`/`_meta.parent`, a journal line per update. Rollback to v<N-1> when the executor cannot read the new plan, or when the update's own domain fails more often in the next 10 game days than in the 10 before. | The player and the planner both write the plan; the lock makes the swap safe. A bad update must not cost a campaign. |
| D9 | **Stability: cooldowns, hysteresis, ledger.** Each trigger has a cooldown in game days; a deviation trigger fires when its measure crosses the threshold and is re-armed only after it recovers past a margin; strategy changes need two consecutive confirmations at least 14 game days apart. A ledger of what was tried and how it ended (executor failures, given-up items, rejected updates, rolled-back updates) goes into every packet, and the default picks never re-propose a ledgered failure inside its cooldown. | Flip-flopping and retry loops were the most common waste in the shakedowns (the same politics pick five times, the same air mission given up and re-planned). |
| D10 | **Cost ceiling, reported.** Per wall hour of play: at most `max_model_updates` model calls and `max_model_seconds` of model time, plus a token ceiling; past either, updates are deterministic until the hour rolls. Updates use the `planner` role (a cheaper, faster model); strategy changes escalate to the `strategist` role. Time and tokens of every call go to `RUN/planner/journal.jsonl` and the run report. | The opening is the expensive step and is paid once; updates must stay cheap enough to run all campaign. |
| D11 | **Provider-neutral.** The planner calls models only through the strategist adapters (`scripts/strategist/adapters`, any configured launcher). With no planner model configured, the update goes to the user's own agent: the supervisor notes "plan update waiting", the agent reads `RUN/planner/request-<n>.md` and answers with `hoi4_run {op:"plan_patch"}`; the same gates apply, and the default pick is applied if no answer comes in time. | The product's player is the user's agent; it must be enough. |
| D12 | **Dossier cached per game build and mod set.** `facts.py dossier build` output is keyed by the effective game files (install plus mods, by path, size and time), the DLCs, the country, date, mode and enemies, and the hoi4facts code itself; it is kept in the hoi4facts user cache and copied into a run (`--fresh` builds anyway). | The dossier is deterministic; rebuilding it per run spent 10 to 20 s of every opening and every re-run; a copy takes a quarter of a second. |

## 2. Data flow

```
                      opening (once, thorough)                       while the game runs (background)
                      ------------------------                       --------------------------------
run_spec ─► dossier (cached) ─► strategist ─► gates ─► critic ─► plan v1 ──► RUN/plan.json ◄── executor reads every tick
                                                                                  ▲
         supervisor ── planner worker ── every 15 s wall:                          │ atomic swap (CAS, lock)
              │            reads: overview, research, economy, states, armies,     │
              │                   technology_list(available), observe feed         │
              │            state ─► horizon per queue ─► triggers (cooldown,       │
              │                                         hysteresis, budget)        │
              │            trigger ─► packet (compact state, menus, ledger) ─► proposer:
              │                         default pick (always) │ planner model │ user's agent
              │                       ─► patch ─► merge ─► invariants ─► gates ─► apply v<N+1>
              │                                                     └─ fail ─► default pick ─► gates ─► apply
              └── report.md: updates, triggers, model time and tokens per hour, ceiling hits, rollbacks
```

## 3. Triggers

| Trigger | Source | Class | Cooldown (game days) | What the update does |
| --- | --- | --- | --- | --- |
| `horizon_low:research` | planned research days left / slots < `research_days` (60) | detail | 20 | extend `research_path` from the in-time menu |
| `horizon_low:focus` | focus days left (remaining `focus_path`+`focus_fallback` costs) < `focus_days` (35) | detail | 20 | extend `focus_path` from available focuses |
| `horizon_low:politics` | PP stock above the reserve with nothing planned to buy for `politics_idle_days` (20), or the feed's `political_power_idle` | detail | 20 | extend `politics_auto.order` (advisors, laws, decisions open now) |
| `horizon_low:production` | feed `stockpile_low` / `equipment_fill_low` for an equipment family | detail | 15 | move or add factories (`production`, `production_priority`), lower the training rate (`training_rules`) |
| `horizon_low:construction` | feed `civilian_factories_idle` or no queued/planned construction left | detail | 20 | extend `construction` from free slots (infrastructure and railways on the axis, then industry) |
| `checkpoint_due` | a checkpoint's `by` passed (met or missed; feed `checkpoint_missed`) | detail; strategy after two misses | 0 | retime the next checkpoints, re-weight the axis |
| `schedule` | `every_days` since the last update (60 in peace, 30 at war) | detail | - | extend every queue to the horizon, correct the remainder |
| `deviation:supply` | an army on an axis below `war.supply_floor` (feed `army_out_of_supply`) | detail | 15 | pace the axis, railway/port/trucks |
| `deviation:casualties` | feed `casualty_ratio` | detail | 15 | rest, re-weight the axis, templates |
| `war_changed` | feed `war_started`, `war_ended`, `peace_conference_opened` with a country the plan's `war.enemies` does not expect, or `phase_changed` with `revise_on_enter` | strategy | 0 | escalated update |
| `goal_at_risk` | feed `goal_at_risk` | strategy | 30 | escalated update |
| `player_request` | feed `revision_requested` | detail | 10 | the reason goes into the packet |
| `blocked` | feed `construction_item_blocked`, `order_given_up` | detail | 10 | replace the item with another way to the same purpose |

Detail triggers that fire together are merged into one update. A strategy trigger is confirmed (D9) before it
escalates; until then it is handled as a detail update.

## 4. The gate subset

Each candidate runs, in order, and stops at the first failure:

1. **Patch shape**: only allowed operations; strategy keys only in an escalated update with a reason.
2. **Invariants**: ids of unchanged items kept; nothing completed (tech, focus, built item) listed as pending;
   no guard or held state dropped while an enemy borders it; no new general for an army that has one; no item
   the ledger shows failed inside its cooldown.
3. **Strict validation**: `validate_plan.py --strict` (schema keys, game keys, tool calls), 0 errors.
4. **Lint ratchet**: `lint_plan.py --json` on candidate and incumbent; for every rule the candidate's error
   count is at most the incumbent's. The rules that guard the shakedown failures (`production.balance`,
   `production.factories`, `politics.pp_budget`, `politics.available`, `research.*`, `focus.*`) are in it.
5. **Anchored horizons**: from today's state, research and focus have at least the low-water mark of planned
   days, PP has a planned use or a reserve reason, and no equipment family in `stockpile_low` is left without
   a line or a factory change.

Steps 3 and 4 need the repository's planning tools (`scripts/hoi4facts`) and the game files; they run in
about 9 s together. Where they are not present (an install without the planning tools) steps 1, 2 and 5
run and the update is marked `gates: partial` in the journal and the report.

## 5. Model and cost policy

| Role | Claude default | Used for | Ceiling per wall hour |
| --- | --- | --- | --- |
| `strategist` | sonnet, xhigh | the opening; escalated (strategy) updates at `revision.full_effort` | 2 escalations |
| `critic` | sonnet, high | the opening only | - |
| `planner` | haiku, high | detail updates | 6 calls, 600 s, 400k tokens |
| none configured | the user's agent through `hoi4_run plan_patch` | detail updates | the same |

Each provider configures its own models (`config.json` `agents.providers.<name>.planner`). A model update has a
wall timeout (180 s); the default pick is applied if it has not answered. The packet is compact (a few
kilobytes: the state summary, the menus with their numbers, the ledger tail, the plan's strategy keys and the
current tails of the lists the trigger touches); the model writes only the patch.

Reported in `RUN/report.md` and `RUN/planner/journal.jsonl`: per update the trigger, proposer, wall seconds,
tokens (input, output, cache), cost when the launcher reports it, gate results, the version applied or the
reason it was not; per hour the model time and tokens against the ceiling.

## 6. What the opening changes

The strategist's procedure stays whole (dossier, 29-item checklist, gates, critic, answers). It adds the
`horizons` key and sets `_validator.stop_date` to the end of the first horizon (procedure item 27.5);
research, focus and PP checks are measured to that date. Later horizons carry `intent` and numeric
`targets` only. The plan says in each later horizon's `intent` what the first update in it extends first.

`horizons` has its own reader in the plan-key reconcile: owner `planner`, the rolling planner
(`hoi4_live/planner/targets.py`). Twenty-one game days before the horizon in force ends the planner raises
`horizon_end:<id>` (a schedule-class update over every domain, with the next horizon's intent and targets
in the packet); when a horizon ends it compares the targets the state answers (divisions, PP, VP and states
held, equipment fill; factories and manpower when read) and raises `deviation:horizon:<id>` for one ended
behind, a strategy trigger when two end behind in a row. Lint `plan.horizons` checks order, overlap, gaps,
the first horizon at the start, the last at the goal deadline and `stop_date` at the first one's end.

## 7. Live checks

Everything above is tested offline behind seams, including a replay of the recorded runs sk9 and sk10 through
the triggers. What only a running game can show is scripted in `scripts/live_checks/rolling_plan.py`.
