# Step 5: per-domain checklists

Work through every list in order. Each item names the command that gives the facts or the check, and
the lint rule (`scripts/hoi4facts/lint_plan.py`, groups as in `lint_plan.py --json`) the result must
pass. `F` below is `python3 scripts/hoi4facts/facts.py` with the run's global flags (`--date`,
`--mode`, and `--game` / `--userdir` / `--mod` when set). `C` is `python3 scripts/hoi4facts/calc.py`;
run `C <subcommand> --help` for its arguments.

| Command | Answers |
|---|---|
| `F dossier build --tag T --date D --dlc auto --mods auto --out RUN/dossier` / `F dossier show <section>` | builds the dossier / prints one digest section (0 run context ... 9 theatres) |
| `F focus <id>` / `F focus-tree <TAG> [--branch id] [--depth N]` | focus detail / tree outline |
| `F focus-timeline <TAG> --date D <ids...>` | start and end dates, blocked, exclusive, runtime gates |
| `F tech <id>` / `F research-calendar <TAG> --date D --years Y1-Y2` / `F research-timeline RUN/plan.json` | eligibility, cost, days; startable techs by year; finish dates of the plan's path |
| `F state <id>` / `F states <TAG>` | slots, infrastructure, buildings, resources, VP, cores, neighbours |
| `F province <id>` / `F route <from> <to> [--avoid-rivers]` | terrain, river edges, port, hub, railway / cheapest path with crossings |
| `F vp <TAG>` | VP list, total, surrender threshold with modifiers, capital, mandatory-control rules |
| `F oob <TAG>` / `F characters <TAG> [--role general]` | templates, divisions per state, wings / commanders and advisors with traits |
| `F laws <TAG>` / `F idea <id>` / `F decisions <TAG>` / `F missions <TAG>` | costs, conditions, timers |
| `F hooks <TAG>` / `F event <id>` | scenario reference graph / event options with name, loc text, trigger, effects |
| `F grep <token>` / `F defines <KEY>` | every reference to a key with file:line and mod / define values with source |
| `C build-order` / `C equipment` / `C pp` | factory compounding / equipment need / PP timeline |

The strategy knowledge stays in the playbook. Read the reference named in each section; do not copy it
into the plan, and re-check any game number it gives with `F` before you use it.

Playbook references (all under `skills/hoi4-campaign-playbook/references/`):
`generic-playbook.md`, `strategy.md`, `economy-build-orders.md`, `research-doctrines-projects.md`,
`war-operating-loop.md`, `supply-logistics-deployment.md`, `army-organisation.md`,
`templates-equipment-production.md`, `politics-diplomacy-espionage.md`; a scenario reference from
`INDEX.md` when one exists.

## 1. Economy (plan keys: `construction`, `construction_plan`, `construction_min_idle`, `construction_queue_depth`)

Read: `economy-build-orders.md` §1.2 (decision table), §2 (slots), §3 (infrastructure first);
`supply-logistics-deployment.md` §5.

1. Months to the first war from step 3. Choose civ/mil timing from the §1.2 table for that number.
2. Run `C build-order` with the start factories, free slots and the planned switch date. It compounds
   over dates, applies the 15-factory cap per project and the infrastructure multiplier. Copy the
   factory counts by date into plan.md §4 as `[D build-order]`.
3. Factory states: `F states <TAG>`, then `F state <id>` for each one you pick; take them from the top of the dossier ranking (§2: infrastructure,
   free slots, distance from enemy borders). No state within 2 province steps of an enemy or planned
   enemy border. A state with infrastructure 2 or less gets `infrastructure` first or is not used.
   Lint: construction, front distance and infrastructure.
4. Supply on the attack axis: hubs (`supply_node`) and railways in `construction_plan` with triggers
   (`province_controlled:<p>`, `state_fully_controlled:<s>`, `owner:<s>=<TAG>`, or a date). Use
   `F province <id>` (hub, railway, port) and `F route <from> <to>` for the railway `path`. Railways
   before hubs where hubs take months. A hub only where the game can build it (no enemy-held province
   in that state when the trigger fires). Lint: construction, path contiguity, trigger on a target
   route, hub buildable.
5. `construction_plan` runs in peace too (`plan.schema.json` description). Items keep stable `id`s.
6. Idle-factory check: no date where civilian factories have no queued project (`construction`
   filler list). Lint: none; critic row 4.

## 2. Research (plan keys: `research_path`, `research_priority`, `research_fallback*`, `doctrines`, `special_projects`)

Read: `research-doctrines-projects.md` §1, §2, §3.1, §3.5, §4.4, §7.

1. `F research-calendar <TAG> --date <start> --years <Y1-Y2>` for the horizon. Pick blocks by year:
   industry and construction first (one slot on them until the next tier is done), then what the war
   plan needs (support companies, artillery, logistics). Nothing with `start_year` after its block year.
2. `F tech <id>` for any tech with several parents: the dossier reports both readings (ANY and ALL);
   order the path so ALL holds. Lint: research, prerequisites.
3. `F research-timeline RUN/plan.json` after writing `research_path`; every block must finish before
   the next block year (warning otherwise). Copy finish dates into plan.md §3 as `[D research-timeline]`.
   Lint: research, start year and throughput.
4. `research_fallback` false, or restricted with `research_fallback_prefixes`. No naval or air folder
   techs unless the goal needs them. Lint: research, fallback and folders.
5. Doctrines: `{id, folder, grand_doctrine | track + subdoctrine, xp}` with names from the dossier §6.
   The starting doctrine comes from the dossier, not from memory.

## 3. Focus (plan keys: `focus_path`, `focus_fallback`, `skip_hours`)

Read: the country section of `economy-build-orders.md` §10 when it exists; dossier §5.

1. `F focus-tree <TAG> --depth 3` for the branches; `F focus <id>` for every focus you list (cost,
   prerequisites, exclusive, `available`, `bypass`, reward summary).
2. `F focus-timeline <TAG> --date <start> <ids...>` for `focus_path` in order. No exclusive pair; each
   focus starts after its prerequisites; every runtime `available` gate is true in the phase that
   reaches it (for example a focus that needs a war flag cleared does not sit in a war phase).
   Lint: focus, exclusive, timeline and runtime gates.
3. Copy start and end dates into plan.md §6 as `[D focus-timeline]`. Factory and slot rewards feed the
   economy and production numbers.
4. `focus_fallback` lists what to take when a listed focus is blocked.

## 4. Laws and advisors (plan keys: `politics`, `decisions`)

Read: `politics-diplomacy-espionage.md` §1, §3, §4; `economy-build-orders.md` §5, §6.

1. `F laws <TAG>` and `F idea <id>` for each law or spirit you plan around (cost, `available`).
   `F characters <TAG>` for advisors (slot, cost, traits, `allowed`).
2. PP timeline: `C pp` with the start PP, PP per day and the purchase list. Each purchase gets a date
   in plan.md §2 as `[D pp]`. Never bank past a purchase that is needed.
3. `politics` items in buy order with `cost`. A law with a runtime condition (war support, ideology)
   gets `when` as a predicate and the condition in `why`.
4. `F decisions <TAG>`: list in `decisions` only decisions that are always good to take when enabled;
   guarded ones go to `decision_rules` with a predicate.

## 5. Production (plan keys: `production`, `production_priority`, `production_min_idle`, `training`)

Read: `templates-equipment-production.md` §5-§8; `strategy.md` "Production and shortage repair".

1. Start lines are not in the history files; the dossier says what is known. Lines to remove are a
   player item until the executor can remove lines.
2. `C equipment` with the templates from `F oob <TAG>`, the division counts by theatre and the
   reinforcement rate. Size lines to cover templates × divisions plus attrition. Copy as
   `[D equipment]`.
3. Sum of factories on lines ≤ military factories at that date (from the build order and focus
   rewards). Equipment exists and is unlocked by that date's techs. Trucks line when `war.motorization`
   is set. Lint: production, factory sum, equipment unlocked, trucks.
4. `training`: train only what equipment covers (`army-organisation.md` §3).

## 6. Army (plan keys: `armies`, `army_layout`, `disband`, `war.guard_states`, `war.guards`)

Read: `army-organisation.md` §1-§5; `war-operating-loop.md` §1, §3, §5b; `supply-logistics-deployment.md` §4, §8.

1. `F oob <TAG>`: templates and divisions per state. `army_layout` entries use only these template
   names, `count` no more than the divisions of those templates in that state, `role` strike or guard.
2. Every own state bordering an enemy (dossier §9) has a guard: a layout guard entry or
   `war.guard_states`. Lint: army, border guards.
3. Strike armies at most 24 divisions, sized also to the supply on the axis. Lint: army, strike size.
4. Generals: `F characters <TAG> --role general`; one per army, chosen once by traits for the role
   (no reassignments later). Names are checked live in step 9. Lint: army, generals exist, no
   duplicates.
5. `disband`: divisions far from any theatre whose equipment the front needs (`army-organisation.md`
   §2). Keep a home guard.
6. Corridor rule when `vp_targets` go more than 2 provinces past own land: a guard follows the strike
   army (`guard_states_in_order`) and plan.md §8 states the rule. Lint: army, corridor.

## 7. Conquest (plan keys: `vp_targets`, `war`, `construction_plan` triggers)

Read: `war-operating-loop.md` §2, §4, §6; `strategy.md` "Land operations"; `generic-playbook.md` §4.

1. `F vp <ENEMY>`: VP list, total, surrender threshold with and without modifiers, capital, and
   mandatory-control rules found in hooks.
2. `F route <from> <to>` from our border to each candidate VP province (add `--avoid-rivers` to
   compare). Order targets by value and adjacency: each target is adjacent to own land or an earlier
   target, or reachable on its `path`. Add targets until cumulative VP ≥ threshold, with and without
   modifiers. Include every mandatory control state. Lint: conquest, threshold, mandatory states,
   adjacency.
3. `by` dates from distance, terrain and river counts (`[D route]`), non-decreasing per army. List the
   river crossings per leg; more than 2 is a warning to answer. Lint: conquest, dates and rivers.
4. Supply construction follows the axis: each captured hub or railway end has a `construction_plan`
   item triggered by its capture (economy item 4).
5. `war`: `enemies`, `guard_states`, `supply_floor`, `rest_below`, `resume_above`, `motorization`,
   `stall_days` per `war-operating-loop.md` §5b. Phase-specific `war` goes in `phases[].overrides.war`.

## 8. Events (plan keys: `event_rules`, `event_options`, `auto_events`)

Read: `politics-diplomacy-espionage.md` §10; the scenario reference when one exists.

1. `F hooks <TAG>` for the events the tag will get; `F event <id>` for each one that matters (option
   name keys, loc text, trigger, effect summary).
2. `event_rules` by `option_name` (never by index), with `why`, a `condition` predicate when the choice
   depends on the state, and `fallback_option_name` when the option can be hidden.
   Lint: events, option exists, condition readable.
3. Keep `auto_events` for news and single-option events.

## 9. Contingencies (plan keys: `contingencies`, `checkpoints`, `phases`)

Read: [lessons.md](lessons.md); `war-operating-loop.md` §3; `strategy.md` "Economy compounds only if the
army survives".

1. One `checkpoint` per mission deadline, per phase target date and per VP-target `by` that matters;
   `must` uses schema predicates; `else` is `revise`, `phase:<id>` or `contingency:<id>`.
2. One `contingency` per likely failure: own core or border province lost, an axis stalls past its
   date, a scripted escalation, a new war or ally call, supply below the floor on the axis. Each has a
   `trigger` predicate and a `phase`, `overrides` or `revise` response the executor can switch to at
   once.
3. Every lesson that applies (lessons.md) is covered by a lint rule, a contingency or a do-not item;
   name which in plan.md §9 or §10.
