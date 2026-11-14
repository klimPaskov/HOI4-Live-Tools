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
  list) or a missing tool (name it, e.g. "no tool: production line MIO chooser"). "Later",
  "TBD" and "if needed" are not resolutions.

Items 1, 2 and 29 are never "not applicable". Write every resolution into the coverage table, plan.md
§25 (all 29 rows: resolution, where it lives, the facts command used, the check).

**Key or atom not in the schema yet.** Some items name keys whose executor step a later build item adds
(schema `x-executor-planned`: `xp_budget`, `sea_transfer`, `garrison_template`,
`production[].remove` / `replace`, `manage.convoys_free`, `manage.fuel_floor`), keys that do not exist
yet (`commanders`), or predicate atoms the executor cannot read yet (x-atoms `readable: false`:
`var:<name>`, `mission_days:<id>`). Check the
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
| `F hooks <TAG>` / `F event <id>` | scenario reference graph (conditions in full) / event options with name, loc text, trigger, effects, and every caller: where it fires, the receiving scope, delay, conditions |
| `F effect <name>` | a scripted effect's (or trigger's) full body with source, and where it is called |
| `F resources <TAG>` | per-resource balance at the start: owned states, trade-law export share, start lines |
| `F grep <token> [--folder <dir>] [-C N]` / `F defines <KEY>` | every reference to a key with file:line, mod and N context lines (FAIR leaves out `common/ai_*`) / define values with source |
| `C build-order` / `C equipment` / `C pp` | factory compounding / equipment need per template and division count / PP timeline |
| `python3 scripts/hoi4facts/tool_schemas.py show <tool>` / `check-call <tool> '<json>'` | a tool's required and allowed arguments with types and enums / one call checked against the catalogue |

Fact readers being added. Use them when `F --help` lists them; until then use the
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
`templates-equipment-production.md`, `politics-diplomacy-espionage.md`, `rules-and-modes.md`, `exploits.md` (META only),
`nation-openers.md`; a scenario reference from `INDEX.md` when one exists. `tactics.md` lists every tactic in
one line with its id (PR01, OP13, ...): scan it once before item 3; the steps below name the ids each item must
decide. Tool status (what is live-verified): `docs/LIVE_VERIFICATION_1_19_4.md`.

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
`enter` and the months-to-war number used by items 3, 10 and 17; the tag's opener in `nation-openers.md` when
it has one (compare, do not copy); plan.md §1 and §7. Check: step 3 gate;
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
3. Every start line, wing and fleet the posture does not use is marked for removal in item 8 (PR08, L18).
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
4. Land: Mass Assault for a defensive war, Grand Battle Plan for an offensive one; naval and air doctrines follow
   the build (RE06-RE09).
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
3. XP sources with their costs: exercises of a few cheap divisions to Regular (stop 14 days before the war), one
   air wing on training, naval exercises to about 100 XP, attachés (100 PP + 50 command power), volunteers
   (AR15-AR17, RE10).
Resolution: the `xp` fields of `doctrines` (and of `templates` once that key has one) in spend order,
`xp_budget` (key pending); plan.md §13 table. Check: rubric row 12.

## C. Army forces and equipment

### 6. Templates
Read: `templates-equipment-production.md` §1 (width, the terrain-mix method), §3, §4, §8;
`army-organisation.md` §1, §3. Facts: `F terrain-width <TAG|theatre>`, `F units [--category]`,
`F oob <TAG>` (start templates), `F tech <id>` (support-company unlocks), `C equipment`.
1. One template per role and phase: line, assault, mobile, garrison (item 23), mountain or marine where the
   theatre needs it. Width from the axis terrain with the fill table (`templates-equipment-production.md` §1):
   line divisions 14-18 for mountains and hills, 20 elsewhere; no 34-36 or 40+ outside plains; fill at least
   0.95 on the main terrain (AR01, L21).
2. Support companies and the 1.19 regimental-support row, each with the reason (the enemy's armour,
   air, terrain). The template tools have no regimental-support argument yet: record the
   regimental row in plan.md and leave it out of the call.
3. Reinforcement priority (elite for the strike army, reserve for garrisons), the XP cost (item 5),
   the equipment it needs (`C equipment`, unlocked by that date), and when fielded divisions switch.
4. Regimental row: columns of exactly 3 battalions; heavy weapons company by default, AA batteries only up to
   about 55 division air attack against enemy CAS, AT batteries against armour; no line TD or SPA in line
   divisions (AR02-AR04, AR09).
5. Garrison template by hardness: one cavalry battalion, or 2-4 obsolete light tanks or armoured cars; no
   military police (AR10).
6. Fielded veteran divisions get a new template instead of a battalion edit (up to 50% XP lost);
   `allow_in_use` only when the stockpile covers the change for every division (AR13).
Resolution: player `templates` (`name`, `regiments`, `support`, `when`, `call` `hoi4_template_create
{name, regiments, support, priority}` or `hoi4_template_edit {template, regiments, support, priority,
allow_in_use}`); `training.template` (executor); plan.md §14. Not applicable only for a goal with no
land war. Check: lint `schema.player_call`, `production.equipment`, `army.width_terrain`; rubric rows 6 and 13.

### 7. Equipment designs
Read: `research-doctrines-projects.md` §6.2; `templates-equipment-production.md` §2, §5. Facts: dossier
§0 (NSB tank, BBA plane, MtG ship designers), `F designs <TAG> [--kind tank|plane|ship]` (bases with their
module slots, modules unlocked at the date, XP costs, start variants), `F tech <id>` (modules by date).
1. Which designs, when, from which XP (item 5), and which existing line switches to each (a variant
   change keeps 90% efficiency; a new chassis much less).
2. Not applicable when the plan builds no tanks, planes or ships of a designer family, or the DLC is
   not active (cite dossier §0).
3. For each new design: its switch class (variant 90%, family 70%, tier 30%) and whether its line switches at
   once or ramps (item 8); keep one chassis or airframe per role and change modules (PR03-PR05); 1.19.4 tank
   rules (PR35); one design per role per phase (PR27); ships: torpedo destroyer, ASW destroyer, raider
   submarine over 60 torpedo attack (NA07, NA08, NA12); planes: never two engines on a small airframe
   (AI08-AI10).
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
   factories}`; 0 factories empties or removes the line (removal is not live-verified yet).
3. Stockpile targets in days of cover per item (item 28 defaults), trucks for every `war.motorization`
   level (truck stock ≥ 1.5× wanted), convoys for overseas supply and trade (item 21), aircraft to keep
   wings at strength (item 20).
4. `training`: only what equipment covers (`army-organisation.md` §3); `training_rules` for the guard.
5. **Day 0 (paused, before the first unpause):** every free or freed military factory goes onto a line that
   exists at the start (PR01, L19): `production` entries on start lines, applied by the executor's first pass
   while the game is still paused; otherwise day-0 brief lines with the exact `hoi4_production_set` calls.
6. **Line audit at the start and at every phase change:** every live line is kept, resized or removed with a
   reason; a line no template, wing, fleet or stockpile target uses is removed (PR08, L18).
7. **Finish items in progress first:** start naval lines finish the hull that is nearly done, then go
   (`remove` with a `from` after it); hulls under about 50% the plan does not need go at once; an expensive
   tank or plane in progress finishes before its line switches (PR06, PR07).
8. **Line-switch plan per tier:** for every tier change in the research path (infantry equipment I to II,
   artillery, AT, AA, tank chassis, airframes) the date, switch or ramp, and the reason. Ramp while the old
   gear is still needed in quantity: 3-5 new factories (about 10% of the old line) on the new model, the old
   line untouched; switch the old line whole when its gear covers about 30 days of field demand or quality
   binds. A young line (under 15%) switches at once; variants any time (PR04, L17).
9. **Rebalance in steps:** idle factories to the line with the fewest days of cover in chunks of
   max(2, 20% of the line); factories leave a line only with 90+ days of cover, at most one pull per line per
   30 days; priority lines on top of the list (PR09-PR11, L20).
10. **Stockpile cover targets** per item: critical under 7 days, warning under 30, 30+ days before a
    declaration (PR28, CP01); spare trucks 1.5x, trains 1.2x, convoys 0.25 per sea supply (PR29).
Resolution: executor `production`, `production_priority`, `production_min_idle`, `training`; player
`training_rules`; `stockpile_targets`; plan.md §5. Check: lint `production.factories`,
`production.equipment`, `production.trucks`, `production.day0`, `production.cull`, `production.mature_switch`;
rubric row 6.

### 9. MIOs
Read: `research-doctrines-projects.md` §6.1; `politics-diplomacy-espionage.md` "Designers and MIOs".
Facts: dossier §0 (Arms Against Tyranny active), `F mio <TAG>` (organisations allowed for the tag, trait
trees, policies, defines).
1. Which organisation per big line (best efficiency trait on the biggest line), the trait order, the
   policies (25 PP each, 180-day lock), the design-team slot.
2. A trait is its 0-based `trait_index` as `hoi4_mio_trait {"list":true}` numbers the tree (trait entries
   carry no text); a line's MIO cannot be chosen by a tool: `mio[].line` and plan.md say "no tool
   for this").
3. Not applicable when AAT is not active (dossier §0).
4. Trait order: production first (cap, then cost or output, growth last), then the role stat; combat MIO on
   the design, production MIO on the line; queue traits so the policy prompt appears (RE13-RE17).
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
4. Resources (`F resources <TAG>` for the start balance): per deficit on a planned line, automatic importing with a budget (executor
   `auto_import {budget}`) or manual purchases (executor `trade`: a list of `{id, resource, amount
   (a number, "max" or "needed"), from (a tag or the name hoi4_trade lists; absent: the partner offering
   most), when}`; automatic importing blocks manual trades while it is on); International Market
   purchases have no tool.
5. Every construction item states its cost in points (`base_cost`, table in `economy-build-orders.md`
   §1.1; `F grep base_cost --folder common/buildings`) and its estimated finish date from
   `C build-order` (5 points per civilian factory per day, at most 15 factories per project,
   × (1 + 0.2 × infrastructure), law and focus speed modifiers as `--bonus`). Name the state of every
   item that is not a factory from the ranking (`naval_base:1@1#<state>`, `supply_node:1#<state>`,
   `infrastructure:1#<state>`): the calculator takes that state's infrastructure, and a date printed
   under `WARNING` or with `ASSUMED` rests on a guessed level and may not go into the plan.
6. Supply construction on the attack axis is item 19: railways first, then a port where no supply
   reaches the shore, trucks, and a hub only after the cost and time check there. Idle-factory check: no date where civilian factories have no queued project
   (`construction` filler list).
7. Switch date from the payback: war date - 2.9 x civilian payback; no civilian factories queued in the last
   18 months before the war or once it runs unless it will last over a year; at war convert civilian to
   military factories when slots are full (EC01-EC03).
8. Slots: every concentrated or dispersed industry level adds +20% building slots; count them when ranking
   states (EC04). Expensive buildings in infrastructure-5 or dam states (EC05).
9. Energy: coal and energy techs for the planned factory count; check coal before each economy-law step
   (EC11, EC12).
10. Trades in blocks of 8 from several sellers, never from a planned enemy; auto-import budget, priority and
    accepted shortages; obsolete gear sold on the market when a tool exists (EC14, EC15, PR22).
11. Under bombing: repair order and factory spread (EC08, EC21, PR33).
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
   until the horizon. When a block runs out before the next block year, fill the slots first with in-year
   techs of other folders the campaign uses (industry, construction, electronics, support companies,
   logistics, equipment the templates use). Pull a tier forward only when nothing in-year remains, and say so
   in plan.md §3: research speed is 1 / (2 x years ahead + 1), so half a year ahead doubles the time and one
   year triples it, and ordinary bonuses do not reduce it (only ahead-of-time bonuses do; `F tech <id>` shows
   the days) (RE01). Order: industry (production, construction, industry folder: the concentrated/dispersed
   line, machine tools, construction), then the equipment the planned templates use, then what the war plan
   needs. Never cut an industry tech because "it does not help this war". Lint `research.idle` fails a
   plan with more than 25% of slot-time unplanned, with or without a fallback.
5. `research_fallback` false, or restricted with `research_fallback_prefixes` to the same families; it is
   a safety net, not the plan. No naval or air folder techs unless item 3 needs them.
6. Limited-use research bonuses (focus and decision rewards): `research_bonuses` {"<bonus category or
   name>": [techs]} (`F tech <id>` gives the categories), or `true` for the category word match only.
   Spend a limited bonus on the most expensive matching tech (RE02).
7. Concentrated or dispersed industry with the reason: dispersed when two or more big line swaps a year,
   bombing, licences or war within a year; else concentrated (PR15).
8. A plan that builds ships lists its naval techs itself: hull, snorkel, torpedo, transport and landing craft,
   radar (RE18).
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
atom `bop:<id>`); plan.md §6 and §16. Check: lint `focus.exclusive`, `focus.order`,
`focus.gates`; rubric row 3.

## E. Politics

**Political power is budgeted over the whole horizon** (L26): laws, advisors and decisions together, so that
the projected balance never sits above about 100 PP with no planned buy for more than 60 days (lint
`politics.pp_budget`). The plan's `politics` picks are the dated buys; `politics_auto` is the standing order for
what they leave unspent (default: chief of army, economy law, trade law). Name the political advisors that fit
the plan (PP, stability, war support or production traits from `F characters <TAG> --role advisor`) in
`politics_auto.order`, since the executor cannot read an advisor's trait from its token; list in
`politics_auto.never` the ideas the plan must not take.

### 13. Gated laws
Read: `economy-build-orders.md` §5; `politics-diplomacy-espionage.md` "Economy, trade and conscription
laws". Facts: `F laws <TAG>` (current law and the `needs` gate of each), `F idea <id>`, `C pp`.
1. Economy, trade and conscription laws in buy order with `cost` and a PP date (`C pp`, plan.md §2.1
   as `[D pp]`). A running focus costs 1 PP a day: if `C pp` does not subtract it, subtract it yourself
   (PO01). Never bank past a purchase that is needed.
2. Each law with a gate (war support, stability, war, government) gets `when` as a predicate; until
   the `war_support` / `stability` atoms exist, the brief carries it: "when `hoi4_country_status {}`
   shows war support > X: `hoi4_politics_choose {category, slot, idea}`".
3. Output cost of the top conscription laws against the manpower plan (item 17).
4. **Law jumps:** plan only the target law of each phase; gates never require the previous law (PO04).
5. Gates as the files state them, including the enemy-industry ratio of War Economy and Total Mobilisation and
   surrender progress for the top conscription laws; war-only laws (Closed Economy, National Defence State)
   before the peace (PO12, PO13, PO15).
6. Trade law with the reason: Free Trade (open to every country) in the 1936 build-up unless the export share
   starves the plan's own lines (EC14).
7. **No conscription law the plan does not need:** Service by Requirement -10%, All Adults Serve -30%, Scraping
   the Barrel -40% output; set `manage.conscription_laws` to the laws the manpower plan allows (PO14, L22).
Resolution: executor `politics` (`id`, `category`, `slot`, `idea`, `cost`) with player-owned `when`;
`politics_auto`; `manage.conscription_laws`. Check: lint `politics.unneeded_conscription`,
`politics.pp_budget`; rubric rows 3 and 10.

### 14. Stability and war support
Read: `politics-diplomacy-espionage.md` §2. Facts: dossier §1 (stability, war support, spirits with
their exits), `F defines BASE_STABILITY_WAR_FACTOR`, `F idea <spirit>`, `F decisions <TAG>`.
1. The gates the plan needs (item 13 laws, item 22 justification) and what keeps each value above them:
   advisors, spirit removals, decisions, propaganda. At war: an offensive war costs war support, a
   defensive one adds it (`politics-diplomacy-espionage.md` §2 "Drivers").
2. Stability floor (50% unless the plan argues otherwise) and what happens below it.
3. **Stability and war support at or above 50%:** both are multipliers (stability above 50% up to +20%
   factory output, below it down to -50%; war support below 50% lowers the surrender limit 30% and cuts
   command power gain 95%) (PO16, PO17). Name the source that holds each above 50%.
4. **War-support routes** for democracies and neutrals short of a 25% or 50% gate: an attaché to the Spanish
   Civil War or the Sino-Japanese War after improving relations to 20 (`diplomacy` `improve_relations`, then
   `send_attache`), war propaganda, focuses (PO19, PO21). Improve Worker Conditions when stability is under
   80% and war is over 12 months away, at most twice (PO18).
Resolution: executor `politics`, `decisions`, `diplomacy` (attaché); player `decision_rules`; `contingencies` with the atom
pending; plan.md §16. Check: rubric row 9.

### 15. Advisors and high command
Read: `politics-diplomacy-espionage.md` §4. Facts: `F characters <TAG> --role advisor`, dossier §7.
1. Political advisors, theorists (item 4), designers (items 7-9), army, navy and air chiefs and high
   command, in buy order with cost and the date from `C pp`; each with the slot and the reason.
2. The chief of army among the first three hires of a land power; each advisor passes the payback test
   (cost / daily gain under half the horizon); officer-corps spirits bought before the hires they discount
   (PO07, PO08, PO10).
Resolution: executor `politics`; the remaining hires in buy order in `politics_auto.order`. Check: lint
`politics.pp_budget`; rubric row 10.

## F. Army organisation and manpower

### 16. Army organisation
Read: `army-organisation.md` §1-§5; `war-operating-loop.md` §1, §3, §5b; `supply-logistics-deployment.md`
§4, §8. Facts: `F oob <TAG>`, `F characters <TAG> --role general`, `F characters <TAG> --role marshal`,
dossier §9.
0. **Concentrate command.** Generals gain experience and traits from the battles of the divisions they
   lead, a field marshal from the armies of his group, and an army without a general gets no leader
   bonus at all. So plan a few standing formations per theatre (by default at most 3 under one field
   marshal, each of at least 3 divisions), each with a sector, a purpose and a general who keeps it all
   war. Guard posts, corridor holds and border watches are tasks of a formation (`task_of`: the
   formation's name_hint; its front line covers those states), not armies of their own. Plans written
   as one army per task get concentrated by the executor anyway (`armies.max_per_group`,
   `armies.min_divisions`, `armies.concentrate: false` to keep a layout as written).
1. `army_layout` uses only start template names, `count` ≤ divisions of those templates in that state,
   `role` strike or guard. Every own state bordering an enemy is covered: by a formation's own state,
   `axis`, `covers` or `guard_states_in_order`, or by a task (`task_of`) whose state is there.
2. Strike armies ≤ 24 divisions, also sized to the supply on the axis.
3. One general per formation, chosen once by traits for its role (no reassignment later); the executor
   gives every other army the best free general, so commanders do not sit idle. One army group per
   theatre over all its formations: executor `army_group` (`id`, `name`, `armies`: 2 to 12 army_layout
   name_hints, `marshal`: a name or names tried in order, `when`), grouped once and led once, never
   churned; when no listed marshal is free the executor takes the best free field marshal. A marshal who
   is only a general is promoted by the tool for command power; one who also commands a layout army is
   skipped (`F characters <TAG> --role marshal`).
3a. Offensives are battle plans: front, offensive line, planning, execute. `vp_targets` name the state the
   offensive goes for; direct province moves only with `war.direct_moves` or as the stall response. An
   army keeps a move for `war.move_commit_days` (3) and gets at most `war.move_budget` (3) moves in
   `war.move_window_days` (10); every move costs organisation.
4. Planned trait assignments and command-power use (15 CP per trait, 20 per preferred tactic): no tool
   yet: plan.md only, `commanders` key pending.
5. `disband`: far divisions whose equipment the front needs; keep a home guard; never divisions overseas
   (75% of their equipment is lost) or in a pocket.
6. One deployed HQ per strike army close to its front (no HQ costs 20% planning cap and speed); generals
   chosen once, divisions swapped between generals rather than unassigned (GN03, GN04); no HQ tool yet:
   plan.md and a brief line.
7. Guards also on ports and hubs in reach of an enemy navy (OP42).
Resolution: executor `armies`, `army_layout`, `army_group`, `disband`, `war.guards`, `war.guard_states`;
plan.md §2 army table. Check: lint `army.templates`, `army.counts`, `army.guards`, `army.strike_size`,
`army.generals` (marshals too), `army.formations`, `army.marshal`; validate_plan (group armies are layout name_hints, one group per army);
rubric rows 6 and 8.

### 17. Manpower
Read: `politics-diplomacy-espionage.md` "Conscription law". Facts: dossier §1-2 (manpower, state
population), `F laws <TAG>`, `C equipment` (manpower per template).
1. Divisions planned per phase × manpower per division, plus 2 months of losses at war [H]; the
   conscription law dates that cover it, with their gates (item 13).
2. What stops when manpower runs short (training first, then the next law).
3. The highest conscription law the plan allows and the trigger for it (training blocked by manpower or the
   front under 80% manpower), with its output cost (item 13, L22).
Resolution: executor `politics` (conscription with `when`), `training`, `manage.conscription_laws`; atom `manpower` pending for the
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
3. **Planning wait:** per offensive, draw the front and offensive line and wait 12-15 game days (planning
   +2% a day to 30%, x0.8 without a deployed HQ) before executing; exceptions named (deadline or mission timer
   under 20 days, an enemy about to dig in) (OP01, L13). Keys `war.planning_wait` (default on, waits for 90%
   of the 30% cap), `war.planning_max_days` (default 15) and `war.planning_skip_days` (default 20).
3a. **Rest, resume, stall:** rest at about 30% of max org, resume at about 80% (executor defaults
   `war.rest_below_pct` 0.3 and `war.resume_above_pct` 0.8 of the highest organisation seen; the absolute
   `war.rest_below` / `war.resume_above` replace them when set) or rest at 60% strength; at a
   stall rest if supply or org is under 50%, else replan on the next target with two directions; a direct move
   only into an empty adjacent province with a guard behind (OP03, OP08).
3b. **Supply floor:** `war.supply_floor` 50 (no new offensive below it); below 35% stop and pull 25-50% of the
   divisions out of the overdrawn province; resume at 70% (OP09, OP10, L14).
3c. **Advance in bounds:** each push ends just past a captured hub or port and rests until it connects (about
   8-12 days) with its railway queued on top (OP11, L16).
3d. Two attack directions where possible, engineers for river crossings, pinning attacks beside the breach,
   equipment fill 90%+ before an offensive (OP04-OP06, OP17, OP20).
4. Defence: hold targets on rivers and mountains, the corridor rule when targets run more than 2
   provinces past own land (a guard follows the strike army), encirclement opportunities from
   `hoi4_encirclement_candidates {enemy}` at war (no executor step yet: player call). A hold post is
   `hold_targets` {`id`, `army` (army_layout name_hint), `provinces` along the corridor, rear first, each
   touching the one before, `when`, `until`}: the army stands in the most forward listed province the
   player controls and follows as the corridor grows; it replaces the layout guard post of that army.
   Only a formation holds a corridor; a hold named for a task is covered by its formation's front.
4a. **Pocket step:** for each pocket from `hoi4_encirclement_candidates`, the pincers at the neck from two
   directions, the anvil pinning, then the reduction: an explicit offensive into every pocket province (the
   battle plan clears only empty pockets of 2 provinces or fewer), pocket ports first (OP13). Own fall-back
   at 50% hostile share or a neck of 2 own provinces or fewer, the HQ first (OP16, L15).
4b. Defensive sectors: fallback lines behind rivers, a mobile reserve one state behind, retakes by the reserve
   not the strike army (OP19, OP31, OP36).
4c. **War-readiness gate** before each declaration: fronts on every border, plans drawn 15 days earlier,
   fighters up 7-14 days earlier, laws at their gates, stockpile cover 30+ days (CP01).
5. Phase-specific `war` in `phases[].overrides.war`.
6. Naval invasions are item 21 (`invasions`).
Resolution: executor `war`, `vp_targets`, `phases`, `hold_targets`; player `retake_rules`; plan.md §7,
§7.1, §8. Check: lint `conquest.threshold`, `conquest.mandatory`, `conquest.adjacency`,
`conquest.by_order`, `conquest.rivers`, `army.guards` (a hold post outside its army's guard states),
`war.planning_wait`, `war.supply_floor`; validate_plan (hold corridors touch on the map graph, armies
known); rubric rows 7 and 8.

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
   `C build-order --order supply_node:1#<state>` shows it finishes before the date it is needed and no port or
   railway can serve the same provinces. Never on day 1 of a short war; only where the game can build
   it when the trigger fires (no enemy-held province in the state).
5. Overseas theatres: naval base level and throughput at each landing port, convoys (item 8) and
   escort (item 21).
6. Fuel: refinery or import plan when the plan runs motorised, air or naval forces on little oil.
7. Railway level 5 on the main axis before a large offensive; a fully motorised hub uses 60 trucks; consumption
   cuts (logistics companies, a Logistics Wizard marshal) in poor-supply theatres (LO04, LO06, LO09).
8. **Pace the advance to supply reach.** Supply flow starts at a node (capital 5.0, port 3.3, hub 2.8
   plus up to 2.2 when motorised) and loses a share at every province it enters, more in rough terrain,
   less at high infrastructure, more across rivers; about 1.0 fully supplies a province. Only hubs,
   ports and the capital are nodes: a railway carries supply between nodes and adds throughput, it does
   not feed the provinces between them. So for each axis state ask which node will feed it when the
   armies arrive (held at the start, built by then, or captured earlier on the axis) and how far it is.
   Lint `supply.axis_reach` prints the estimate per axis state in `facts.supply_reach`; where it is
   below 1.0, stop the axis at the last supplied state until a port, a railway to a nearer node or
   trucks are in place, or write why the shortfall is accepted (a short push to a capital that
   surrenders).
9. **A railway starts at a node.** The first `path` province of a railway item is a supply hub, a naval
   base, the capital, or a hub or port the plan builds there first; the last is the province to reach
   (`F province <id>` shows hub and port). The executor moves a start that is no node to the nearest fed
   node and says so, so the line built is not the one planned.
10. **What the executor does on its own.** An army below `war.supply_floor` for
   `war.relief_after_days` (2) gets a railway level from the nearest fed node to its province, then one
   more motorisation level; after `war.relief_report_days` (10) it raises `army_out_of_supply`
   (`war.supply_relief: false` keeps only the report). A `construction_plan` item refused three times is
   given up and raises `construction_item_blocked`; a revision that changes the item (another start,
   a port, trucks) brings it back. Plan for these levers instead of counting on them: they react after
   the army has started to starve.
Resolution: executor `construction_plan`, `war.motorization`, `war.supply_floor`, `war.supply_first`,
`war.supply_relief`, `war.relief_after_days`, `war.relief_report_days`; `stockpile_targets`; plan.md §4
and §17. Check: lint `construction.hub`, `construction.hub_cost`, `construction.railway`,
`construction.railway_node`, `construction.trigger_route`, `supply.axis_reach`, `production.trucks`;
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
3. **Composition rules:** fighters first, about 1.5x the enemy's fighters over the decisive region 1-2 weeks
   before the offensive; CAS at most 3 per enemy combat width of the expected battles (6% of production or
   less against AA-heavy enemies); naval bombers only against fleets; no strategic bombers in a war shorter
   than a year; bases at 70% capacity or less; logistics strike when no attack runs; interception only
   against bombers; bombers change region at most monthly (AI01-AI14).
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
4. **Composition rules:** one patrol task force per region to control, started 50+ days before an invasion
   or blockade; a strike force only where a patrol runs, at most 2 regions; 3 screens per capital or carrier;
   torpedo destroyers with 1 cruiser per 4; submarines in groups of 8-12, one per deep-ocean region, only
   against a convoy-dependent enemy; escorts 1 per 12 convoys per region; carriers only for UK, USA and
   Japan with a full fleet (NA01-NA17, NA28).
5. **Invasions:** capacity and preparation from the techs owned (divisions/plans/days 4/1/60, 6/2/50,
   10/5/40, 15/9/30); the invasion army detached from front and fallback lines; 60% control and 40% naval
   intel on every route region; radar on the route coast (NA20-NA23, L23).
6. Start naval queue: each start ship line finished, cut or removed with a date (PR07).
Resolution: executor `navy`, `invasions`; `sea_transfer` (player key, executor step pending W1-14):
`hoi4_army_move`; convoy lines in executor `production`; plan.md §19. Check: lint `navy.required`,
`navy.overseas`, `army.generals` (admirals); validate_plan (fleets in the start OOB, sea region names,
coastal invasion states), lint `navy.invasion_prep`; rubric row 14.

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
4. **World-tension gates** by ideology for every justify, guarantee, faction join, lend-lease and volunteer
   item (`when` with `world_tension`); justify on claims and cores (Conquer expires 60 days after completion);
   one justification at a time; no AI allies called into the plan's wars (DI01-DI03, DI08).
5. Attaché, volunteer and lend-lease routes with their costs and recall dates (PO19, DI07, DI09).
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
1. Default law per phase (laws the government can use, from `F occupation-laws`, including ideology laws
   such as colonial police); the switch rules: a state's resistance target at 25 or more → a harsher law;
   milder only at peace (Local Police, then Civilian Oversight at about 20% compliance); supply routes and
   states the armies cross get the strongest suppression (OC02-OC05).
2. **Occupation per state:** Harsh Quotas on factory states, Forced Labour on resource states, Military
   Governor on populous states, Martial Law on supply routes (OC01); no per-state tool yet: plan.md and a
   brief line, the default law meanwhile.
2a. A garrison template by hardness (item 6, AR10) and its count; accept the collaboration-government event
   at 80% compliance unless the plan needs that land's dockyards (OC06).
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
3. Upgrades: training centres, then cheap upgrades to 5 for the extra operative; intel networks in the states
   just behind the front of the main offensive 3-6 months ahead; collaboration operations on the main target
   from the year before the war (non-democracies; no tool yet); decryption timed to the main offensive
   (IN02, IN03, IN06, IN10).
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
3. **Bidding:** occupy first; take order own cores, claims, occupied states, uncontrolled last (x10);
   contested cores in round one, uncontested wants early, a contest at most 1-2 turns; capital and VP
   provinces to own armies for participation; the faction peace rule set before the conference; capital ships
   last (PC01-PC06, PC11, DI13).
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
   A phase with neither `enter` nor `exit` lasts only while a contingency that names it is active; then
   the phases are judged again from the first. Give a holding phase an `exit` only when it must outlast
   its trigger, never leave the offensive waiting on a phase nothing ends.
3. Every lesson that applies is covered by a lint rule, a contingency or a do-not item; name which in
   plan.md §9 or §10.
4. One phase per expected capitulation with overrides for war (free the guards, new axes), production (the
   cull of item 8) and occupation (CP07, CP09).
Resolution: `checkpoints`, `contingencies`, `phases` (player keys; executor evaluation is build item
15). Check: rubric row 9.

### 28. Continuous management
Read: `strategy.md` "Production and shortage repair"; `operating-loop.md`; `war-operating-loop.md` §4,
§5b. For each rule below give the campaign's value or "default" with a reason in plan.md §24. `manage`
thresholds run in `plan_manage.rs` and `plan_military.rs` (the casualty and air-superiority rules); a
threshold the schema marks `x-executor-planned` (`convoys_free`, `fuel_floor`) is not run yet: until it is,
the rule is a brief line (read, threshold, call) when the player must act.

| Rule | Default threshold | Key today |
|---|---|---|
| Idle research slot, focus, factories, PP | civs ≥ 4 idle, mils ≥ 2, PP > 150 (`native/src/alarms.rs`) | `alarms` |
| Game alert row (`hoi4_alerts {}`) | any named alert → its fix; officer corps, MIO, admiral alerts wake the player | `manage` |
| Armies and generals | few formations (≤ 3 per marshal, ≥ 3 divisions, ≤ 24), small roles are tasks inside them, a general on every army | `armies`, `army_layout` |
| Rest and resume | rest below 30% of the army's highest organisation seen, resume at 80% and 70% supply; at most 20 days (not while supply is under 35%). These are the executor defaults; absolute `rest_below` / `resume_above` replace the shares | `war.rest_below_pct`, `war.resume_above_pct`, `war.supply_resume`, `war.rest_max_days` |
| Supply floor | no new offensive below 50% supply; below 35% nothing attacks (attrition). Executor defaults | `war.supply_floor`, `war.supply_hold` |
| Planning wait | the execute order waits until the planning bonus reaches 90% of its 30% cap (about 14 game days), at most 15 days; skipped near the goal deadline | `war.planning_wait`, `war.planning_cap`, `war.planning_share`, `war.planning_gain`, `war.planning_max_days`, `war.planning_skip_days` |
| Advance in bounds | after a captured hub, port or railway the army's next advance waits until it feeds the front (rail dead about 10 days, 5 on cores), at most 12 days | `war.bound`, `war.bound_rail_days`, `war.bound_core_days`, `war.bound_max_days` |
| Stalled attack / forming pocket | 8 days without progress: a second direction first, then the direct move; fall back at 50% of the surrounding land held by others (the neck is not read) | `war.stall_days`, `war.second_direction`, `war.pocket_share`, `war.avoid_pockets` |
| Factory rebalance | idle military factories to the line the divisions lack most, 3 per action, re-chosen; never pulled from a line; day 0 to the kept starting lines | `production_priority`, `production_min_idle`, `manage.rebalance_step`, `manage.day0_production` |
| Supply line first at war | supply construction ahead of other lines | `war.supply_first` |
| Front equipment fill | army mean < 80% → disband home divisions, elite priority, factories to the short item (`strategy.md`) | `manage` |
| Stockpile days of cover | < 7 critical, < 30 warning (`strategy.md`) | `stockpile_targets` |
| Training only what equipment covers | stockpile guard and cap | `training_rules` |
| Trucks | stock < 1.5× wanted → truck line, lower motorisation (`supply-logistics-deployment.md` §10) | `manage` |
| Manpower | free manpower < 2 months of losses → next conscription law only if the plan allows it (output cost) | `manage.manpower_months`, `manage.conscription_laws` |
| Laws when the gate opens | the `needs` gate of `F laws` | `politics[].when` |
| Stability / war support drift | below a plan gate, or either below 50% (both are multipliers) | `manage.stability_floor` |
| Resource deficit on a priority line | any → auto-import budget, else trade | `auto_import` |
| Occupation | resistance target ≥ 25 → harsher; milder only at peace | `occupation` |
| Air wings idle at war / air plan switch | standing-by wing; `switch_when`; base over capacity | `air.war_missions` / `air_plan` |
| Enemy air superiority on our front | > 60% → fighters to the region, delay the offensive (reported unknown until a tool maps it) | `manage.enemy_air_superiority`, `air_front_regions`, `air_hold_days` |
| Convoys for overseas armies | free convoys < 10% of total, or overseas supply < floor | `manage` |
| Fuel | < 25% of capacity → stop exercises, import oil | `manage` |
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

META checklist (only when the run spec says `mode: meta`): read `exploits.md` and decide each line with status
**works** for this tag (use it, or say in plan.md §11 why not); an **unclear** line is used only as a journalled one-time
test with a FAIR fallback; never a **patched** line or one marked *needs pause*. List the chosen exploits with their
MX id or name in plan.md §11, and keep the house bans in `rules-and-modes.md` for every line a fair run would refuse.
