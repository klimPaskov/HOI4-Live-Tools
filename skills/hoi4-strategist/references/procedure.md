# Step 5: the 29-item campaign checklist

A strong player decides all of the items below for every campaign, so the plan decides them too. Work
through them in order. `F` below is `python3 scripts/hoi4facts/facts.py` with the run's global flags
(`--date`, `--mode`, and `--game` / `--userdir` / `--mod` when set). `C` is
`python3 scripts/hoi4facts/calc.py`; run `C <subcommand> --help` for its arguments. Lint rules are the
`rule` names of `scripts/hoi4facts/lint_plan.py --json`.

## Resolution rule (hard rule, SKILL.md step 5)

Every item ends in exactly one of:

* **executor**: a `plan.json` key whose schema tag is `x-owner: executor` (preferred);
* **player**: a player key in `plan.json` with `call: {tool, args}` where the schema allows it, and the
  same call as one line in the brief (`args` without `request_id` / `lease_token`, which the player
  adds). Write the call from `python3 scripts/hoi4facts/tool_schemas.py show <tool>` (argument names,
  types, enums), never from memory, and check it with `tool_schemas.py check-call <tool> '<json>'`;
  step 7 runs `python3 scripts/hoi4facts/check_calls.py RUN` over the plan, the brief and plan.md.
  Shorthand such as `hoi4_air_rebase {wing, state_id}` in this file lists argument names only; never
  copy it as a call;
* **not applicable: <reason>**, where the reason cites a fact (a facts output, a dossier line, the DLC
  list) or a missing tool (the audit row id, e.g. "no tool: production line MIO chooser"). "Later",
  "TBD" and "if needed" are not resolutions.

Items 1, 2 and 29 are never "not applicable". Write every resolution into the coverage table, plan.md
§25 (all 29 rows: resolution, where it lives, the facts command used, the check).

**Key or atom not in the schema yet.** Some items name keys whose executor step a later build item adds
(schema `x-executor-planned`: `xp_budget`, `stockpile_targets`, `sea_transfer`, `garrison_template`,
`production[].remove` / `replace`, `manage.convoys_free`, `manage.fuel_floor`), keys that do not exist
yet (`commanders`), or predicate atoms the executor cannot read yet (x-atoms `readable: false`:
`convoys_free`, `stockpile_days:<equipment>`, `var:<name>`, `bop:<id>`, `mission_days:<id>`). Check the
capability list (SKILL.md Inputs). When the key or atom is absent, never write it into `plan.json` (lint
`schema.keys`); write the decision in its plan.md section and put it in the brief as a player line: the
trigger in words, the read that checks it (for example `hoi4_country_status {}`), and the exact call.
The coverage table then says `player (key pending: <key>)`. The wave-2 keys `army_group`, `hold_targets`,
`navy`, `invasions`, `designs`, `production[].design`, `diplomacy`, `faction`, `intelligence`, `mio`,
`trade`, `research_bonuses` and the casualty and air-superiority `manage` thresholds are executor keys:
write them in the shapes below (the schema descriptions and `native/src/plan_military.rs` /
`plan_affairs.rs` module docs are the contract), never as brief lines.

## Commands

| Command | Answers |
|---|---|
| `F dossier build --tag T --date D --dlc auto --mods auto --out RUN/dossier` / `F dossier show <section>` | builds the dossier / prints one digest section (0 run context ... 9 theatres) |
| `F focus <id>` / `F focus-tree <TAG> [--branch id] [--depth N]` | focus detail / tree outline |
| `F focus-timeline <TAG> --date D <ids...>` | start and end dates, blocked, exclusive, runtime gates |
| `F tech <id>` / `F research-calendar <TAG> --date D --years Y1-Y2` / `F research-timeline RUN/plan.json` | eligibility, cost, days; startable techs by year; finish dates of the plan's path |
| `F state <id>` / `F states <TAG>` | slots, infrastructure, buildings (naval base, air base), resources, VP, cores, neighbours |
| `F province <id>` / `F route <from> <to> [--avoid-rivers]` | terrain, river edges, port, hub, railway / cheapest path with crossings |
| `F vp <TAG>` | VP list, total, surrender threshold with modifiers, capital, mandatory-control rules |
| `F oob <TAG>` / `F characters <TAG> [--role general\|marshal\|admiral\|advisor\|leader]` | templates, divisions per state, wings, fleets / commanders and advisors with traits |
| `F laws <TAG>` / `F idea <id>` / `F decisions <TAG>` / `F missions <TAG>` | costs, gates (`needs has_war_support > ...`), conditions, timers |
| `F hooks <TAG>` / `F event <id>` | scenario reference graph / event options with name, loc text, trigger, effects |
| `F grep <token> [--folder <dir>]` / `F defines <KEY>` | every reference to a key with file:line and mod / define values with source |
| `C build-order` / `C equipment` / `C pp` | factory compounding / equipment need per template and division count / PP timeline |
| `python3 scripts/hoi4facts/tool_schemas.py show <tool>` / `check-call <tool> '<json>'` | a tool's required and allowed arguments with types and enums / one call checked against the catalogue |

Fact readers being added (audit F-1 to F-11). Use them when `F --help` lists them; until then use the
fallback named here and tag the result with the file it came from:

| Command | Answers | Until available |
|---|---|---|
| `F terrain-width <TAG\|theatre>` | combat and support width per terrain, terrain mix of the axis provinces | `F province <id>` terrain for each axis province (from `F route`), widths from `F grep combat_width --folder common/terrain` |
| `F units [--category <c>]` | battalion and support stats, width, equipment, regimental-support eligibility, special forces | `F grep <unit key> --folder common/units`; `C equipment --regiments ... --support ...` |
| `F weather <region>` | weather periods per strategic region (mud, snow, cold, heat) | `F grep <region key> --folder map/strategicregions` |
| `F occupation-laws` | occupation laws: requirements, resistance and compliance effects, garrison need | `F grep <law key> --folder common/occupation_laws`; `supply-logistics-deployment.md` §7 |
| `F bop <TAG>` | balance-of-power ranges, sides, the focuses and decisions that move it | `F grep <TAG> --folder common/bop`; `F grep set_power_balance --folder history/countries` |
| `F ports <TAG>` | naval bases and port capacity per state, sea routes, adjacency rules (straits, canals) | `F state <id>` naval base level, `F province <id>` port; `F grep <canal or strait> --folder map` |
| starting experience (dossier §1) | army, air and navy XP at the start date | `F grep army_experience --folder history/countries` and the XP rewards in `F focus <id>`; live `hoi4_doctrines {}` at reconcile |
| law gates (dossier §7) | war support and stability gates as numbers | `F laws <TAG>` (`needs` text), `F idea <id>` |

The strategy knowledge stays in the playbook. Read the reference named in each item; do not copy it into
the plan, and re-check any game number it gives with `F` before you use it. Playbook references (all
under `skills/hoi4-campaign-playbook/references/`): `generic-playbook.md`, `strategy.md`,
`economy-build-orders.md`, `research-doctrines-projects.md`, `war-operating-loop.md`,
`war-air-naval-peace.md`, `supply-logistics-deployment.md`, `army-organisation.md`,
`templates-equipment-production.md`, `politics-diplomacy-espionage.md`, `rules-and-modes.md`; a
scenario reference from `INDEX.md` when one exists. Tool status (what is live-verified):
`docs/LIVE_VERIFICATION_1_19_4.md`.

Each item below gives: **Facts** (commands), the steps, **Resolution** (the key or call), **Check**
(lint rule or rubric row).

## A. Goal and situation

### 1. Goal
Facts: `F decisions <TAG>`, `F missions <TAG>`, `F hooks <TAG>`, `F vp <ENEMY>`, `F grep <token>`.
SKILL.md step 2 and [goal-patterns.md](goal-patterns.md). Resolution: `goal` (predicates, deadline,
fail predicates, reading, alternatives); plan.md §0; brief "Win" line. Check: lint `win.goal`,
`win.deadline`; rubric row 1.

### 2. Situation
Facts: dossier §1-4 and §8-9; `F hooks <TAG>`, `F event <id>`, `F oob <ENEMY>` (FAIR: counts and
templates only), `F states <TAG>`, `C pp`. SKILL.md step 3. Resolution: `war.enemies`, the first phase's
`enter` and the months-to-war number used by items 3, 10 and 17; plan.md §1 and §7. Check: step 3 gate;
rubric row 2.

## B. Force posture, doctrines, experience

### 3. Force posture
Read: `strategy.md` "Force architecture", "Air and naval support"; `templates-equipment-production.md`
§7. Facts: `F dossier show 3` (enemy aggregates: divisions, templates, planes, ships), `F oob <TAG>`
(own divisions, wings, fleets), `F states <TAG>` (dockyards, naval and air bases), `C build-order`
(military factories and dockyards by date).
1. Per phase, the share of military factories on land, air and naval equipment and the share of
   dockyards on convoys, screens, capitals, submarines. Derive it from the goal and the enemy aggregates
   (an enemy with no planes needs no fighters beyond what CAS escort needs; an overseas theatre needs
   convoys before anything else).
2. Name the arms the goal does not need and why; their research and doctrine folders stay out.
Resolution: executor `production` (factories per line, `from`), `production_priority`,
`phases[].overrides.production_priority`, `research_fallback_exclude`, `doctrines` folders; plan.md §12.
Check: lint `production.factories`, `research.folders`; rubric row 12.

### 4. Doctrine path
Read: `research-doctrines-projects.md` §2, §2.2. Facts: `F dossier show 6` (doctrines at start, grand
doctrines and tracks), `F grep <doctrine key> --folder common/doctrines` (`xp_cost`), `F characters
<TAG> --role advisor` (theorists).
1. Per folder (land, air, naval, special forces): the grand doctrine (keep the starting one unless the
   gain beats the mastery reset; a new grand doctrine resets mastery in every track), one subdoctrine
   per track in XP order, which track to master first.
2. A theorist before an expensive adoption (cost reduction) as a `politics` item.
3. Special forces: which branch, or not applicable with the reason (no special-forces templates planned).
Resolution: executor `doctrines` (`id`, `folder`, `grand_doctrine` or `track` + `subdoctrine`, `xp`) and
`politics` (theorist); plan.md §13. Check: `validate_plan.py` doctrine check; rubric row 13.

### 5. XP budget
Read: `research-doctrines-projects.md` §2.3. Facts: starting experience (dossier §1; until available
`F grep army_experience --folder history/countries` and the XP rewards of the planned focuses),
`F defines MAX_ARMY_EXPERIENCE` (pool cap per branch), `F defines TRAINING_MAX_DAILY_COUNTRY_EXP`.
1. Forecast XP per branch (army, air, navy) by month: start + focus rewards + training/exercise + combat
   estimate [H]. No pool above the cap: spend before it fills.
2. Order every spend per branch with its month: doctrines (item 4), template edits (item 6), designs
   (item 7), officer-corps spirits. No month spends more than the forecast; keep the reserve for
   designs that the playbook names.
Resolution: the `xp` fields of `doctrines` (and of `templates` once that key has one) in spend order,
`xp_budget` (key pending); plan.md §13 table. Check: rubric row 12.

## C. Army forces and equipment

### 6. Templates
Read: `templates-equipment-production.md` §1 (width, the terrain-mix method), §3, §4, §8;
`army-organisation.md` §1, §3. Facts: `F terrain-width <TAG|theatre>`, `F units [--category]`,
`F oob <TAG>` (start templates), `F tech <id>` (support-company unlocks), `C equipment`.
1. One template per role and phase: line, assault, mobile, garrison (cheap suppression, item 23),
   mountain or marine where the theatre needs it. Width target from the axis terrain mix, within 10%
   of a clean fit for the main terrain.
2. Support companies and the 1.19 regimental-support row, each with the reason (the enemy's armour,
   air, terrain). The template tools have no regimental-support argument yet (audit F2): record the
   regimental row in plan.md and leave it out of the call.
3. Reinforcement priority (elite for the strike army, reserve for garrisons), the XP cost (item 5),
   the equipment it needs (`C equipment`, unlocked by that date), and when fielded divisions switch.
Resolution: player `templates` (`name`, `regiments`, `support`, `when`, `call` `hoi4_template_create
{name, regiments, support, priority}` or `hoi4_template_edit {template, regiments, support, priority,
allow_in_use}`); `training.template` (executor); plan.md §14. Not applicable only for a goal with no
land war. Check: lint `schema.player_call`, `production.equipment`; rubric rows 6 and 13.

### 7. Equipment designs
Read: `research-doctrines-projects.md` §6.2; `templates-equipment-production.md` §2, §5. Facts: dossier
§0 (NSB tank, BBA plane, MtG ship designers), `F designs <TAG> [--kind tank|plane|ship]` (bases with their
module slots, modules unlocked at the date, XP costs, start variants), `F tech <id>` (modules by date).
1. Which designs, when, from which XP (item 5), and which existing line switches to each (a variant
   change keeps 90% efficiency; a new chassis much less).
2. Not applicable when the plan builds no tanks, planes or ships of a designer family, or the DLC is
   not active (cite dossier §0).
Resolution: executor `designs` (`id`, `kind` tank / plane / ship / other, `base` an equipment key from
`F designs` or a start variant name, `name`, `modules` {slot: module} for the module designers or
`upgrades` {row: level} without one, `xp`, `when`), made once per id with an XP guard; the line switch is
`designs[].production` (the line's equipment) or `production[].design` (a designs id, with
`accept_progress_loss`); plan.md §15. Check: validate_plan (kind against the base's designer, slots,
modules, upgrade rows); rubric row 6.

### 8. Production
Read: `templates-equipment-production.md` §5-§8; `strategy.md` "Production and shortage repair";
`supply-logistics-deployment.md` §6. Facts: `C equipment` (templates × divisions × reinforcement),
`C build-order` (military factories by date), `F tech <id>` (unlocks), `F ports <TAG>` (convoy need).
1. Lines by phase, sized to cover templates × divisions plus attrition; sum ≤ military factories at that
   date; equipment unlocked by then.
2. Removals and replacements of starting lines (obsolete equipment): `production[].remove` / `replace`
   when the schema has them (W1-9); until then a player `hoi4_production_set {line, equipment,
   factories}`; 0 factories empties or removes the line (the descriptor says both; no live record of a
   removal yet [?]).
3. Stockpile targets in days of cover per item (item 28 defaults), trucks for every `war.motorization`
   level (truck stock ≥ 1.5× wanted), convoys for overseas supply and trade (item 21), aircraft to keep
   wings at strength (item 20).
4. `training`: only what equipment covers (`army-organisation.md` §3); `training_rules` for the guard.
Resolution: executor `production`, `production_priority`, `production_min_idle`, `training`; player
`training_rules`; `stockpile_targets` (key pending); plan.md §5. Check: lint `production.factories`,
`production.equipment`, `production.trucks`; rubric row 6.

### 9. MIOs
Read: `research-doctrines-projects.md` §6.1; `politics-diplomacy-espionage.md` "Designers and MIOs".
Facts: dossier §0 (Arms Against Tyranny active), `F mio <TAG>` (organisations allowed for the tag, trait
trees, policies, defines).
1. Which organisation per big line (best efficiency trait on the biggest line), the trait order, the
   policies (25 PP each, 180-day lock), the design-team slot.
2. A trait is its 0-based `trait_index` as `hoi4_mio_trait {"list":true}` numbers the tree (trait entries
   carry no text); a line's MIO cannot be chosen by a tool: `mio[].line` and plan.md say "no tool (audit
   C7)".
3. Not applicable when AAT is not active (dossier §0).
Resolution: executor `mio` (`organisation`, `traits` [trait_index, ...] in queue order, `policy`,
`design_team` true or a slot, `slot`, `when`); plan.md §15. Check: validate_plan (organisation allowed for
the tag, policy, trait index range); rubric row 6.

## D. Economy and research

### 10. Economy timing
Read: `economy-build-orders.md` §1.2, §2, §3, §7, §8; `supply-logistics-deployment.md` §5. Facts:
`C build-order`, `F states <TAG>`, `F state <id>`, `F province <id>`, `F route <from> <to>`.
1. Months to the first war (item 2). Civ/mil/dockyard timing from the §1.2 table; run `C build-order`
   with the start factories, free slots and the switch date (15-factory cap per project,
   infrastructure multiplier). Copy factory counts by date into plan.md §4 as `[D build-order]`.
2. Factory states from the top of the dossier ranking (§2): no state within 2 province steps of an
   enemy or planned enemy border; infrastructure 2 or less gets `infrastructure` first or is not used.
3. Slot gains (focuses, techs, decisions) before the build order needs them; energy: coal states and
   one energy or industrial keystone per state where the files offer it; refineries and fuel silos for
   an oil-poor country (item 19).
4. Resources: per deficit on a planned line, automatic importing with a budget (executor
   `auto_import {budget}`) or manual purchases (executor `trade`: a list of `{id, resource, amount
   (a number, "max" or "needed"), from (a tag or the name hoi4_trade lists; absent: the partner offering
   most), when}`; automatic importing blocks manual trades while it is on); International Market
   purchases have no tool (audit B16).
5. Every construction item states its cost in points (`base_cost`, table in `economy-build-orders.md`
   §1.1; `F grep base_cost --folder common/buildings`) and its estimated finish date from
   `C build-order` (5 points per civilian factory per day, at most 15 factories per project,
   × (1 + 0.2 × infrastructure), law and focus speed modifiers as `--bonus`).
6. Supply construction on the attack axis is item 19: railways first, then a port where no supply
   reaches the shore, trucks, and a hub only after the cost and time check there. Idle-factory check: no date where civilian factories have no queued project
   (`construction` filler list).
Resolution: executor `construction`, `construction_plan` (stable `id`s), `construction_min_idle`,
`construction_queue_depth`, `auto_import`, `trade`; plan.md §4. Check: lint `construction.front`,
`construction.infrastructure`; rubric row 4.

### 11. Research
Read: `research-doctrines-projects.md` §1, §3.1, §3.5, §4.4, §7.
1. `F research-calendar <TAG> --date <start> --years <Y1-Y2>`. Blocks by year: industry and
   construction first (one slot on them until the next tier is done), then what the war plan needs
   (support companies, artillery, logistics). Nothing with `start_year` after its block year.
2. `F tech <id>` for any tech with several parents; order the path so the ALL reading holds.
3. `F research-timeline RUN/plan.json`; every block finishes before the next block year. Copy finish
   dates into plan.md §3 as `[D research-timeline]`.
4. **Every slot busy, all the time (user rule).** Research serves the whole campaign, not only the current
   war: the game goes on after the goal, and the economy compounds. List enough techs that no slot waits
   until the horizon: when a block runs out before the next block year, pull the next tier forward (one year
   ahead costs little: `F tech <id>` shows the ahead-of-time days) instead of leaving slots empty. Order:
   industry (production, construction, industry folder: e.g. the concentrated/dispersed industry line,
   machine tools, construction), then the equipment the planned templates use, then what the war plan
   needs. Never cut an industry tech because "it does not help this war". Lint `research.idle` fails a
   plan with more than 25% of slot-time unplanned, with or without a fallback.
5. `research_fallback` false, or restricted with `research_fallback_prefixes` to the same families; it is
   a safety net, not the plan. No naval or air folder techs unless item 3 needs them.
6. Limited-use research bonuses (focus and decision rewards): `research_bonuses` {"<bonus category or
   name>": [techs]} (`F tech <id>` gives the categories), or `true` for the category word match only.
Resolution: executor `research_path`, `research_priority`, `research_fallback*`, `research_bonuses`,
`special_projects` (a facility at start, else not applicable with the reason). Check: lint `research.start_year`,
`research.prerequisites`, `research.throughput`, `research.idle`, `research.fallback`, `research.folders`;
rubric row 5.

### 12. Focus and balance of power
Read: the country section of `economy-build-orders.md` §10 when it exists; dossier §5. Facts:
`F focus-tree <TAG> --depth 3`, `F focus <id>`, `F focus-timeline <TAG> --date <start> <ids...>`,
`F bop <TAG>`.
1. `focus_path` in order: no exclusive pair; each focus after its prerequisites; every runtime
   `available` gate true in the phase that reaches it. Copy dates into plan.md §6 as
   `[D focus-timeline]`. `focus_fallback` for a blocked focus.
2. Balance of power, when the tag has one at the start date or a planned focus creates one: the side
   the plan wants, the range edges that matter, and the focuses and decisions that move it; consistent
   with the focus path. Not applicable: `F bop <TAG>` (until available `F grep <TAG> --folder common/bop`)
   finds no balance for the tag and no planned focus sets one.
Resolution: executor `focus_path`, `focus_fallback`; player `decision_rules` (BoP decisions with `when`;
atom `bop:<id>` pending); plan.md §6 and §16. Check: lint `focus.exclusive`, `focus.order`,
`focus.gates`; rubric row 3.

## E. Politics

### 13. Gated laws
Read: `economy-build-orders.md` §5; `politics-diplomacy-espionage.md` "Economy, trade and conscription
laws". Facts: `F laws <TAG>` (current law and the `needs` gate of each), `F idea <id>`, `C pp`.
1. Economy, trade and conscription laws in buy order with `cost` and a PP date (`C pp`, plan.md §2.1
   as `[D pp]`). Never bank past a purchase that is needed.
2. Each law with a gate (war support, stability, war, government) gets `when` as a predicate; until
   the `war_support` / `stability` atoms exist, the brief carries it: "when `hoi4_country_status {}`
   shows war support > X: `hoi4_politics_choose {category, slot, idea}`".
3. Output cost of the top conscription laws against the manpower plan (item 17).
Resolution: executor `politics` (`id`, `category`, `slot`, `idea`, `cost`) with player-owned `when`.
Check: rubric rows 3 and 10.

### 14. Stability and war support
Read: `politics-diplomacy-espionage.md` §2. Facts: dossier §1 (stability, war support, spirits with
their exits), `F defines BASE_STABILITY_WAR_FACTOR`, `F idea <spirit>`, `F decisions <TAG>`.
1. The gates the plan needs (item 13 laws, item 22 justification) and what keeps each value above them:
   advisors, spirit removals, decisions, propaganda. At war: an offensive war costs war support, a
   defensive one adds it (`politics-diplomacy-espionage.md` §2 "Drivers").
2. Stability floor (50% unless the plan argues otherwise) and what happens below it.
Resolution: executor `politics`, `decisions`; player `decision_rules`; `contingencies` with the atom
pending; plan.md §16. Check: rubric row 9.

### 15. Advisors and high command
Read: `politics-diplomacy-espionage.md` §4. Facts: `F characters <TAG> --role advisor`, dossier §7.
1. Political advisors, theorists (item 4), designers (items 7-9), army, navy and air chiefs and high
   command, in buy order with cost and the date from `C pp`; each with the slot and the reason.
Resolution: executor `politics`. Check: rubric row 10.

## F. Army organisation and manpower

### 16. Army organisation
Read: `army-organisation.md` §1-§5; `war-operating-loop.md` §1, §3, §5b; `supply-logistics-deployment.md`
§4, §8. Facts: `F oob <TAG>`, `F characters <TAG> --role general`, `F characters <TAG> --role marshal`,
dossier §9.
1. `army_layout` uses only start template names, `count` ≤ divisions of those templates in that state,
   `role` strike or guard. Every own state bordering an enemy has a guard.
2. Strike armies ≤ 24 divisions, also sized to the supply on the axis.
3. One general per army, chosen once by traits for its role (no reassignment later). Field marshals and
   army groups for a front with more than one army: executor `army_group` (`id`, `name`, `armies`: 2 to
   12 army_layout name_hints, `marshal`: a name or names tried in order, `when`), grouped once and led
   once, never churned. A marshal who is only a general is promoted by the tool for command power; one
   who also commands a layout army is skipped (`F characters <TAG> --role marshal`).
4. Planned trait assignments and command-power use (15 CP per trait, 20 per preferred tactic): no tool
   yet (audit G6, G8): plan.md only, `commanders` key pending.
5. `disband`: far divisions whose equipment the front needs; keep a home guard.
Resolution: executor `armies`, `army_layout`, `army_group`, `disband`, `war.guards`, `war.guard_states`;
plan.md §2 army table. Check: lint `army.templates`, `army.counts`, `army.guards`, `army.strike_size`,
`army.generals` (marshals too); validate_plan (group armies are layout name_hints, one group per army);
rubric rows 6 and 8.

### 17. Manpower
Read: `politics-diplomacy-espionage.md` "Conscription law". Facts: dossier §1-2 (manpower, state
population), `F laws <TAG>`, `C equipment` (manpower per template).
1. Divisions planned per phase × manpower per division, plus 2 months of losses at war [H]; the
   conscription law dates that cover it, with their gates (item 13).
2. What stops when manpower runs short (training first, then the next law).
Resolution: executor `politics` (conscription with `when`), `training`; atom `manpower` pending for the
contingency; plan.md §16. Check: rubric row 6.

## G. War

### 18. Operations
Read: `war-operating-loop.md` §2-§6; `war-air-naval-peace.md` §1-§4; `strategy.md` "Land operations";
`generic-playbook.md` §4. Facts: `F vp <ENEMY>`, `F route <from> <to> [--avoid-rivers]`,
`F province <id>`, `F weather <region>`, `F terrain-width <theatre>`.
1. VP order: targets by value and adjacency until cumulative VP ≥ threshold with and without modifiers;
   every mandatory control state. `by` dates from distance, terrain and river counts (`[D route]`),
   non-decreasing per army; more than 2 river crossings on a leg is a warning to answer.
2. Weather and season: no offensive planned into mud, snow or extreme cold on the axis (from
   `F weather <region>`); the season named per phase.
3. Planning time before each offensive (the planning bonus grows per day to its cap) and the rest
   thresholds (`war.rest_below`, `war.resume_above`, `war.supply_floor`, `war.stall_days`).
4. Defence: hold targets on rivers and mountains, the corridor rule when targets run more than 2
   provinces past own land (a guard follows the strike army), encirclement opportunities from
   `hoi4_encirclement_candidates {enemy}` at war (no executor step yet: player call). A hold post is
   `hold_targets` {`id`, `army` (army_layout name_hint), `provinces` along the corridor, rear first, each
   touching the one before, `when`, `until`}: the army stands in the most forward listed province the
   player controls and follows as the corridor grows; it replaces the layout guard post of that army.
5. Phase-specific `war` in `phases[].overrides.war`.
6. Naval invasions are item 21 (`invasions`).
Resolution: executor `war`, `vp_targets`, `phases`, `hold_targets`; player `retake_rules`; plan.md §7,
§7.1, §8. Check: lint `conquest.threshold`, `conquest.mandatory`, `conquest.adjacency`,
`conquest.by_order`, `conquest.rivers`, `army.guards` (a hold post outside its army's guard states);
validate_plan (hold corridors touch on the map graph, armies known); rubric rows 7 and 8.

### 19. Supply and logistics
Read: `supply-logistics-deployment.md` §1, §5, §6, §10; `strategy.md` "Economy compounds only if the
army survives". Facts: `F province <id>` (hub, railway, port), `F route <from> <to>`, `F ports <TAG>`,
`F states <TAG>` (oil, naval base levels).
Supply order (`supply-logistics-deployment.md` §5 "Supply construction order"); every item states its
cost in points, its estimated finish date (`C build-order`) and the source:
1. **Railway levels** (170 points plus 130 per extra level, days each) on each axis in
   `construction_plan`, from existing or captured hubs and ports along the `F route` path, with capture
   triggers (`province_controlled:<p>`, `state_fully_controlled:<s>`, `owner:<s>=<TAG>`, or a date).
2. **A port, or a port level**, on a coastal province near the front where no supply reaches the shore
   (`naval_base`, 5000 points plus 1000 per extra level: about 7-10 weeks with 15 civilian factories;
   initial supply flow 3.3 against a hub's 2.8, but reach falls off faster inland). Conditions: the
   province is coastal and owned or controlled; free convoys cover 0.25 per supply point
   (`SUPPLY_CONVOY_FACTOR`); the sea route is not blocked by enemy navy or air. `F ports <TAG>` and
   `F province <id>` (coastal, port).
3. **Motorisation and trucks**: `war.motorization` per army and a truck line from day 1, with the truck
   stock it needs (item 8): the cheap way to carry supply beyond the railhead.
4. **A supply hub** (`supply_node`, 20000 points: about 190-270 days with 15 civilian factories at
   infrastructure 2 to 0, longer under a construction-speed malus) only when
   `C build-order --order supply_node:1` shows it finishes before the date it is needed and no port or
   railway can serve the same provinces. Never on day 1 of a short war; only where the game can build
   it when the trigger fires (no enemy-held province in the state).
5. Overseas theatres: naval base level and throughput at each landing port, convoys (item 8) and
   escort (item 21).
6. Fuel: refinery or import plan when the plan runs motorised, air or naval forces on little oil.
Resolution: executor `construction_plan`, `war.motorization`, `war.supply_floor`, `war.supply_first`;
`stockpile_targets` (key pending); plan.md §4 and §17. Check: lint `construction.hub`,
`construction.hub_cost`, `construction.railway`, `construction.trigger_route`, `production.trucks`;
rubric rows 7 and 14.

### 20. Air plan
Read: `war-air-naval-peace.md` §5; `strategy.md` "Air and naval support". Facts: `F oob <TAG>` (wings),
`F dossier show 3` (enemy planes), `F states <TAG>` (air bases), dossier §9 (strategic regions).
1. Wings, base per region (base capacity per level; build air bases with `construction`), missions by
   phase (air superiority before an offensive, CAS over the axis, interception, naval or port strikes),
   the switch trigger per wing, ground AA when the enemy has planes.
   Wing names are made at runtime: write `air_plan[].wing` as `"<variant> at <state id>"` from the
   dossier OOB (`F oob <TAG>`: each wing's variant and base state). `scripts/strategist/reconcile.py`
   expands each selector to the live wings on day one (SKILL.md step 9). `region` in
   `hoi4_air_mission` is the region's name (a string).
2. Not applicable only when the country has no wings, plans none, and the enemy has no planes
   (cite `F oob` and dossier §3).
Resolution: executor `air.war_missions`; player `air_plan` (`wing`, `base_state`, `mission`, `region`,
`switch_when`, `call` `hoi4_air_mission {wing, mission, region}` or `hoi4_air_rebase {wing,
state_id}`); plan.md §18. Check: lint `schema.player_call`; rubric row 14.

### 21. Naval plan
Read: `war-air-naval-peace.md` §6, §7; `strategy.md` "Naval invasion state machine";
`economy-build-orders.md` §8. Facts: `F oob <TAG>` (fleets), dossier §1 (convoys at start),
`F ports <TAG>`, `F states <TAG>` (dockyards, naval bases).
1. Fleets and task forces, missions per sea region (patrol, strike_force, convoy_raiding,
   convoy_escort, mine_laying, mine_sweeping, training, invasion_support, hold), admirals, repair rule;
   dockyard and hull plan (item 3). `navy` {`posture`, `fleets`: [{`fleet` (a start fleet name from
   `F oob`), `admiral`, `state_id`, `missions`: [{`mission`, `regions` (sea region display names, or
   {`region`, `state_id`}), `task_force`, `when`}]}], `convoy_escort` {`fleet`, `regions`, `when`},
   `repair_below` / `repair_until`}: per task force the first mission entry whose `when` holds.
2. Overseas theatres (mandatory): sea transfers of armies (player `hoi4_army_move {army, state_id}`;
   the readback reports `sea_transfer_unverified` until the troops land), convoys and escort for the
   route, the landing ports (item 19). Naval invasions: `invasions` [{`id`, `from_state` (own coastal
   state), `target_state` (`F province <id>` names the state of the landing province), `target_province`,
   `armies`, `fleet` (its task forces get invasion_support), `when`}], one at a time; list the armies in
   `war.reserve` too.
3. Or "no navy: <reason>" (`{"posture": "none", "reason": ...}`; no coastline or naval base in
   `F states <TAG>`; no overseas theatre and no naval threat in dossier §3).
Resolution: executor `navy`, `invasions`; `sea_transfer` (player key, executor step pending W1-14):
`hoi4_army_move`; convoy lines in executor `production`; plan.md §19. Check: lint `navy.required`,
`navy.overseas`, `army.generals` (admirals); validate_plan (fleets in the start OOB, sea region names,
coastal invasion states); rubric row 14.

## H. Diplomacy, occupation, intelligence, peace

### 22. Diplomacy
Read: `politics-diplomacy-espionage.md` §6, §7; `war-air-naval-peace.md` §8. Facts: dossier §1
(guarantees, faction, access, subjects), dossier §3, `F diplomacy <TAG>` (world tension, ideology tension
gates, war goals), `F factions <TAG>` (templates), `F defines TENSION_CB_WAR` and
`F defines WARGOAL_WORLD_TENSION_REDUCTION` (tension and justification costs).
1. War goals: target, justification start and declaration dates against the world-tension gates of the
   ideology; who joins the target (the executor checks `hoi4_war_risk` before declaring).
2. Factions (join, create, goals), guarantees, non-aggression pacts, access, volunteers, lend-lease;
   the answer policy for incoming proposals (player: `hoi4_diplomacy_incoming` is not live-verified, so
   the policy goes in the brief).
3. Not applicable for any part only with the reason (for example "already at war on day 1 with the
   only enemy; no faction is open to the tag").
Resolution: executor `diplomacy`, in list order, once per `id`: [{`id`, `action` (justify, declare,
guarantee, nap, access, offer_access, docking, improve_relations, send_attache, invite_to_faction, or a
hoi4_diplomacy row), `target` (tag), `when`, `world_tension_max`; justify: `war_goal` (the type as
hoi4_diplomacy lists it) and 1-5 `states` names; declare: `justify_days`, `joiners_ok`, `widen_ratio`,
`accept_widening`, `expiry_warn_days`, `rejustify`}]; a declaration waits for the target's justify items
and the widening check. Executor `faction` {`join` (tag or ANY), `create` {`name`, `template`, `invite`},
`when`}. Plan.md §20. Check: validate_plan (tags, justify fields); rubric row 10.

### 23. Occupation
Read: `supply-logistics-deployment.md` §7; `army-organisation.md` §6; `strategy.md` "Occupation, peace
and endgame". Facts: `F occupation-laws`, dossier §2 (own non-core states and their compliance;
colonies can resist from day one), `F vp <ENEMY>` (VP states that will resist).
1. Default law per phase (laws the government can use, from `F occupation-laws`); the switch rules:
   resistance ≥ 50% (garrison damage doubles) → a harsher law; compliance ≥ 40% with resistance < 25%
   → a milder law; supply routes and states the armies cross get the strongest suppression.
2. A garrison template (item 6) and its count; a collaboration government when compliance can reach
   80% (La Resistance).
3. Not applicable only when the plan conquers nothing and no own state resists (dossier §2).
Resolution: player `occupation` (`law`, `when`, `call` `hoi4_occupation_law {law}`), `templates`
(garrison); `garrison_template` (key pending); plan.md §22. Check: rubric row 15.

### 24. Intelligence
Read: `politics-diplomacy-espionage.md` §8. Facts: dossier §0 (La Resistance active), dossier §1 (agency
at start), `F agency` (upgrades by branch with their levels, operations, defines).
1. Agency yes or no (it costs civilian factories for about a month), the upgrade order, operatives and
   their missions (network, counter-intelligence), the decryption target (plan.md only: no tool).
2. Not applicable: LaR inactive (dossier §0), or the goal ends before an agency pays off (state the
   dates).
Resolution: executor `intelligence` {`agency`, `upgrades` in order ("<branch>:<upgrade>" or the key; a
key twice is its second level), `operatives` (a count, or one entry per operative: `mission` with
`state_id` for build_intel_network, quiet_intel_network, root_out_resistance, `target` for
control_trade, diplomatic_pressure, propaganda, counter_intelligence), `when`}; plan.md §21. Check:
validate_plan (upgrade keys, branch, levels, mission targets); rubric row 15.

### 25. Peace aims
Read: `war-air-naval-peace.md` §9; `generic-playbook.md` §5. Facts: `F vp <ENEMY>`, `F hooks <TAG>`
(scripted peace events and mediation), `F event <id>`.
1. Per enemy: ranked demands (states to take, puppet, release), annex vs puppet with the reason,
   mandatory states of the goal first.
2. The answer to scripted peace offers and white peace (an `event_rules` entry each) and the rule for
   when to accept.
Resolution: player `peace` (`take`, `puppet`; the brief carries `hoi4_peace_take {states, puppet,
release, max_cost}`, `hoi4_peace_submit {}`, `hoi4_peace_exit {}`); `event_rules`; plan.md §23. Every
enemy in `war.enemies` has a peace entry. Check: rubric row 15.

## I. Events, contingencies, continuous management, FAIR

### 26. Events and decisions
Read: `politics-diplomacy-espionage.md` §5, §10; the scenario reference when one exists. Facts:
`F hooks <TAG>`, `F event <id>`, `F decisions <TAG>`, `F missions <TAG>`.
1. `event_rules` by `option_name` (never by index), with `why`, a `condition` predicate when the choice
   depends on the state, and `fallback_option_name` when the option can be hidden. `auto_events` for
   news and single-option events; `event_options` only where an index is certain.
2. `decisions`: only decisions that are always good when enabled. Guarded or targeted ones go to
   `decision_rules` with `when` / `target`.
3. **Never-take list**: every decision or event option whose effect ends the war on bad terms, cedes
   states, or fails a mission (search the effects with `F decisions <TAG>` and `F event <id>`):
   `decision_rules` entry with `never: true`, and a do-not line in plan.md §10 and the brief.
4. Missions with timers become checkpoints at their deadlines (item 27).
Resolution: executor `decisions`, `auto_events`, `event_options`; player `event_rules`,
`decision_rules`. Check: lint `events.options`; rubric row 9.

### 27. Checkpoints and contingencies
Read: [lessons.md](lessons.md); `war-operating-loop.md` §3; `strategy.md` "Economy compounds only if
the army survives".
1. One `checkpoint` per mission deadline, per phase target date and per VP-target `by` that matters;
   `must` uses schema predicates; `else` is `revise`, `phase:<id>` or `contingency:<id>`.
2. One `contingency` per likely failure: own core or border province lost, an axis stalls past its
   date, a scripted escalation, a new war or ally call, supply below the floor on the axis, manpower or
   equipment short. Each has a `trigger` predicate and a `phase`, `overrides` or `revise` response.
3. Every lesson that applies is covered by a lint rule, a contingency or a do-not item; name which in
   plan.md §9 or §10.
Resolution: `checkpoints`, `contingencies`, `phases` (player keys; executor evaluation is build item
15). Check: rubric row 9.

### 28. Continuous management
Read: `strategy.md` "Production and shortage repair"; `operating-loop.md`; `war-operating-loop.md` §4,
§5b. For each rule below give the campaign's value or "default" with a reason in plan.md §24. `manage`
thresholds run in `plan_manage.rs` and `plan_military.rs` (the casualty and air-superiority rules); a
threshold the schema marks `x-executor-planned` (`convoys_free`, `fuel_floor`) is not run yet: until it is,
the rule is a brief line (read, threshold, call) when the player must act.

| Rule | Default threshold (source) | Key today |
|---|---|---|
| Idle research slot, focus, factories, PP | civs ≥ 4 idle, mils ≥ 2, PP > 150 (`native/src/alarms.rs`) | `alarms` |
| Game alert row (`hoi4_alerts {}`) | any named alert → its fix; officer corps, MIO, admiral alerts wake the player | `manage` |
| Armies and generals | ≤ 24 divisions, general on armies ≥ 8 (`defines` `CORPS_COMMANDER_DIVISIONS_CAP`) | `armies` |
| Rest and resume | organisation < 12 or supply < 25%; at most 20 days | `war.rest_below`, `war.supply_floor`, `war.rest_max_days` |
| Stalled attack / forming pocket | 8 days / encirclement risk ≥ 70% | `war.stall_days`, `war.avoid_pockets` |
| Supply line first at war | supply construction ahead of other lines | `war.supply_first` |
| Front equipment fill | army mean < 80% → disband home divisions, elite priority, factories to the short item (`strategy.md`) | `manage` |
| Stockpile days of cover | < 7 critical, < 30 warning (`strategy.md`) | `stockpile_targets` |
| Training only what equipment covers | stockpile guard and cap | `training_rules` |
| Trucks | stock < 1.5× wanted → truck line, lower motorisation (`supply-logistics-deployment.md` §10) | `manage` |
| Manpower | free manpower < 2 months of losses → next conscription law if its gate holds | `manage` |
| Laws when the gate opens | the `needs` gate of `F laws` | `politics[].when` |
| Stability / war support drift | below a plan gate or stability < 50% | `manage` |
| Resource deficit on a priority line | any → auto-import budget, else trade | `auto_import` |
| Occupation | resistance ≥ 50% → harsher; compliance ≥ 40% and resistance < 25% → milder (`common/resistance_compliance_modifiers`) | `occupation` |
| Air wings idle at war / air plan switch | standing-by wing; `switch_when`; base over capacity | `air.war_missions` / `air_plan` |
| Enemy air superiority on our front | > 60% → fighters to the region, delay the offensive (reported unknown until a tool maps it) | `manage.enemy_air_superiority`, `air_front_regions`, `air_hold_days` |
| Convoys for overseas armies | free convoys < 10% of total, or overseas supply < floor | `manage` |
| Fuel | < 25% of capacity (`defines` fuel penalty) → stop exercises, import oil | `manage` |
| Doctrines and template edits | XP ≥ the next planned spend | `doctrines` / `templates` |
| Scenario variable, mission timer, BoP | the plan's thresholds (escalation level, mission days < 15, BoP range edge) | `contingencies` |
| Peace conference open | open → peace step or wake the player | `peace` |
| Casualty ratio | own/enemy losses above the limit (2.0) over 14 days, at least 1000 own losses → the front's attacking armies rest 7 days | `manage.casualty_ratio_max`, `casualty_window_days`, `casualty_min_losses`, `casualty_rest_days` |
| War goal expiry, world-tension gates | goal older than ~50 days; tension crosses a plan gate | `diplomacy[].expiry_warn_days`, `rejustify`, `world_tension_max` |
| Idle task forces, damaged ships | no mission; strength < 50% → mission; port | `navy.fleets[].missions`, `navy.repair_below` / `repair_until` |
| Incoming proposals | pop-up → answer by the item 22 policy | brief: `hoi4_diplomacy_incoming` |
| Command power near the cap | ≥ 70 of 80 → planned trait or ability; wake the player | `manage` |
| Agency upgrade idle | no upgrade running → next upgrade | `intelligence.upgrades` |
| Refused action | 3 refusals in 10 days → wake the player with the evidence (failure ledger) | executor default |

Resolution: executor `alarms`, `war` thresholds, `training_rules`, `manage`, `navy`, `diplomacy`,
`intelligence`; brief lines for what no key runs yet; plan.md §24. Check: lint `manage.thresholds`; rubric
row 16.

### 29. FAIR boundary
Read: `rules-and-modes.md`. SKILL.md hard rule 3. Every enemy fact used in items 2, 3, 6, 18 and 20-22
is `public` or `aggregate` in fair mode, and plan.md tags it. Resolution: the source tags in plan.md (the run
spec's `mode` decides the classes allowed); no per-division enemy position, no `common/ai_strategy`. Check: rubric
row 2.
