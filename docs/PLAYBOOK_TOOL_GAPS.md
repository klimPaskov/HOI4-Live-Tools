# Playbook tool gaps: what the MCP supports versus what expert play needs

Date: 25 October 2026. Scope: the native Rust server run with `--mac-input` (68 tools) as
exposed to the Haiku player through the private broker `macplay.py`, plus the legacy Python
package where it matters for Windows/Linux.

How this was built (so you know what to trust):

- Tool names, arguments and descriptions come from the tool descriptors in
  `native/src/protocol.rs` (observation tools) and `native/src/launch/macos.rs` (action tools,
  lines ~385-430), `native/src/launch/macos/daily.rs` (governor) and `native/src/plan_run.rs`
  (standing plan keys and step order).
- "Seen live" evidence comes only from the private 25 October gate run's `calls.jsonl` (415 calls,
  one two-year Germany gate that was **not** accepted) and the `OUTCOME.md` files of m005-m010.
  Anything marked **unverified** is read from a descriptor or source only, or comes from
  documentation I could not tie to a live run.
- The player allowlist (`READ`/`ACT` in `macplay.py`) is a subset of the 68 tools. Tools not on
  it are marked "not player-visible". `ACT` includes read-style tools that must open a panel
  (they need the lease token): `hoi4_diplomacy`, `hoi4_decisions_list`, `hoi4_politics_options`,
  `hoi4_air_wings`, `hoi4_state_slots`.
- I did not run any tool for this document. Quality statements are descriptor claims plus the
  logged outcomes, not fresh measurements.

## Contents

1. [Summary and headline findings](#1-summary-and-headline-findings)
2. [Tool inventory by player visibility](#2-tool-inventory-by-player-visibility)
3. [Domain gap table](#3-domain-gap-table)
4. [Prioritized build order (top 12)](#4-prioritized-build-order-top-12)
5. [Reliability defects](#5-reliability-defects)
6. [Fast-paced play: standing plans versus model wake-ups](#6-fast-paced-play-standing-plans-versus-model-wake-ups)
7. [Cross-platform notes](#7-cross-platform-notes)
8. [What I could not verify](#8-what-i-could-not-verify)

---

## 1. Summary and headline findings

1. **The tool surface is wide on routine economy and ordinary army plumbing, and empty on
   everything that wins wars.** Research, focus, politics, decisions, events, construction,
   production lines, training, army/army-group creation, fronts and offensive lines, air
   missions and diplomacy all have an action. There is no tool at all for: navy (fleets, ships,
   missions, invasion), division template editing, equipment/ship/plane designer, special
   projects, MIOs, espionage, doctrines beyond `research_start`, peace conference, factions as
   a first-class object, supply/logistics, convoys, resources, stability/war support/manpower,
   and any enemy or battle-situation read.
2. **Reads are the larger problem than actions.** Every failed campaign (m005-m010, the
   qualification gate) lost for reasons a human sees at a glance and the tools hide: undefended
   borders (no war/front/threat read), expiring war goals (no justification timer read),
   supply 22 percent (no supply read), consumer goods starvation (economy reports counts only;
   `unavailable.shortages`, `stability`, `war_support`, `ui_factory_totals`, `idle_builders` are
   all explicitly true in `hoi4_economy`), research ETA unknown (`progress` and `days_remaining`
   are `null` in `hoi4_overview`; `capabilities_unverified` lists `progress_days`).
3. **Action tools are click automation with memory readback.** They work, but each is a staged
   GUI script (panel, button, Find view, click at the screen centre). Failure rate in the
   gate: 31 errors in 415 calls, 29 of them on the action tools, and ~10 of the 31 were the
   same construction/army-create no-op class (section 5). Latency is 1-7 s per action; pace
   was 17-26 real minutes per game year, driven by model turns, not tool latency.
4. **The governor/standing plan covers the cheap domains only** (research refill, focus path,
   politics one-shots, decisions, training queue, factory rebalance, construction, army
   hygiene, a simple war executive). It is the right shape. It does not cover anything that
   needs a read the tools lack.
5. **Biggest single lever for a strong campaign: a defensive-posture and threat read**
   (front coverage per border, enemy-held and contested states, units in range) plus an
   all-borders front-line action. Three of six recorded losses were an undefended heartland.

---

## 2. Tool inventory by player visibility

Legend: R = read, A = action (needs `lease_token`, broker injects it), RA = read that must open a
panel and so needs the lease. "Seen" = appears in the qualification `calls.jsonl`.

### Player-visible (broker `READ` set, 15 tools)

`hoi4_overview` R (seen 60), `hoi4_research` R (42), `hoi4_technology_list` R, `hoi4_economy` R (33),
`hoi4_production_lines` R (4), `hoi4_construction_queue` R (11), `hoi4_focus_completed` R (47),
`hoi4_wait_for` R (32, mean 36.9 s wall), `hoi4_capabilities` R, `hoi4_division_templates` R (2),
`hoi4_focus_list` R, `hoi4_armies` R (3), `hoi4_army` R, `hoi4_states` R (11), `hoi4_events` R (34).

### Player-visible (broker `ACT` set, 28 tools)

`hoi4_time` A, `hoi4_research_start` A, `hoi4_focus_start` A, `hoi4_production_add` A,
`hoi4_production_set` A, `hoi4_construction_add` A, `hoi4_politics_options` RA, `hoi4_politics_choose` A,
`hoi4_decisions_list` RA, `hoi4_decision_take` A, `hoi4_train` A, `hoi4_deploy_locations` A,
`hoi4_army_create` A, `hoi4_army_front` A, `hoi4_army_plan` A, `hoi4_army_offensive` A,
`hoi4_army_move` A, `hoi4_plan_run` A, `hoi4_army_commander` A, `hoi4_army_group_create` A,
`hoi4_army_split` A, `hoi4_state_slots` RA, `hoi4_event_choose` A, `hoi4_diplomacy` RA,
`hoi4_diplomacy_act` A, `hoi4_air_wings` RA, `hoi4_air_mission` A.

Of these, seen in the gate: construction_add 29, focus_start 25, research_start 20,
politics_options 16, event_choose 11, politics_choose 9, army_create 7, time 5, train 4,
production_set 4, production_add 1, deploy_locations 1, decision_take 1, decisions_list 2.
**Never called in the gate** (so no live evidence from it): `army_front`, `army_plan`,
`army_offensive`, `army_move`, `army_commander`, `army_group_create`, `army_split`, `state_slots`,
`diplomacy`, `diplomacy_act`, `air_wings`, `air_mission`, `plan_run`. Their only live evidence is
m005-m010 notes, which are narrative, not measurements.

### Not player-visible (40 tools in the 68)

- Diagnostics/probes for engineering: `hoi4_input_geometry` (player got `not_allowed`, harmless),
  `hoi4_research_slot_control`, `hoi4_research_category`, `hoi4_research_folders` (native macOS
  returns `unsupported_platform`), `hoi4_research_ui`, `hoi4_research_node_detail`,
  `hoi4_research_nodes`, `hoi4_focus_search_state`, `hoi4_focus_node_detail`,
  `hoi4_focus_selected_detail`, `hoi4_focus_input_probe`, `hoi4_construction_controls`,
  `hoi4_construction_find_view`, `hoi4_construction_target`, `hoi4_construction_ui_probe`,
  `hoi4_construction_panel_probe`, `hoi4_production_controls`.
- Staged construction primitives the `construction_add` macro is built from:
  `hoi4_construction_open`, `_select`, `_find_state`, `_place`, `_clear_selection`, `_find_open`.
- Standing-plan plumbing the player reaches only through the broker: `hoi4_plan` (get/set
  `plan.json`), `hoi4_daily_governor` (start/update/digest/stop), `hoi4_input_lease`.
- `hoi4_research_slot_control`, `_nodes`, `_category` exist to support `research_start` and are
  not intended as player tools.

---

## 3. Domain gap table

Priority: **P0** blocks any strong campaign; **P1** needed for a strong campaign but a weaker one
survives; **P2** polish or late game. Effort: **S** (days, mostly composition of existing
reads/stages), **M** (a new staged GUI flow plus memory readback, a week or two), **L** (new memory
structures to reverse engineer or a new multi-stage UI domain; weeks). Platform column:
all action tools below are macOS-only today (`launch/macos.rs`); Linux has a separate set
(research/focus/production/construction under the governor, with panel layouts not shared with
Mac); Windows runs the legacy Python OCR package. See section 7.

### 3.1 Economy and construction

| Field | Content |
| --- | --- |
| Exists? | `hoi4_economy` R, `hoi4_construction_queue` R (+`include_location`), `hoi4_state_slots` RA, `hoi4_construction_add` A. Staged primitives (not player-visible): `hoi4_construction_open/select/find_state/place/clear_selection/find_open`. |
| Quality / known defects | `economy`: named factory trigger counts (civilian/military/naval), `civilian_factories_available_for_projects`, exact PP. Says `unavailable`: idle builders, shortages, stability, war support, UI factory totals. `construction_queue`: exact building types, produced/cost, `live_validation: pending`, no allocation, no railway paths, no ETA. `construction_add`: 7 of 29 calls failed with `input_readback_failed` ("The centre click queued nothing more in this state"), 3 more `events_open`, 1 `ui_blocked`. The player never called `state_slots` in the gate, so it queued blind. State buildings only; railways, supply hubs and ports unsupported. |
| What the playbook needs | Per-state slots, infrastructure, resources, and which buildings are allowed; civilian/military/dockyard ratio with consumer-goods share; ability to cancel or reprioritise a queue item; convert civilian to military factories; refinery/fuel/rubber/steel balance; railway and supply-hub projects; AA/radar/naval base/air base; shortage and building-slot alarms. |
| Proposed tool design | (a) `hoi4_state_overview` R `{state_ids?: int[], owned_only?: bool}` returning per state: slots total/used/free, infrastructure, buildings by type, resources, province/coast flags, owner/controller. Receipt: snapshot id + read counts. Prefer a memory read; fall back to the existing state-view GUI read in `state_slots`. (b) Make `construction_add` pre-check slots and return typed codes `no_free_slot`, `building_not_allowed`, `state_not_owned` before any click, and report `queued: n` vs `requested: n`. (c) `hoi4_construction_cancel` A `{handle}` using the snapshot-scoped handle already returned by `construction_queue`. (d) `hoi4_construction_priority` A. (e) `hoi4_factory_convert` A `{from, to, count}` (unverified that the game offers a UI for this in the tested version; in HOI4 1.14+ this is by constructing, so check first). (f) Extend `economy` with `consumer_goods_ratio`, `shortages[]`, `stability`, `war_support`, `manpower`. Reads from memory where an owned-country trigger or variable exists, otherwise a bounded OCR/GUI read marked `source:"gui"`. |
| Effort | State overview S-M (state view already read by `state_slots`); pre-check S; cancel/priority M; shortages/consumer goods/stability/war support M-L (needs new memory offsets validated against own-country telemetry; the repo rules forbid copying offsets between platforms); railway/hub projects L. |
| Priority | P0 for pre-check, shortages, consumer goods, stability/war support (m008 lost two years of civilian output to MEFO bills and consumer goods; m005 had 14-20 factories on infantry equipment). P1 for cancel/priority/convert. P2 for railway projects until a supply read exists (then P1). |
| Platform | Mac only today. Linux has its own verified construction layouts (descriptor says they do not transfer to Mac). Windows legacy `hoi4_construction` is OCR/save based and not re-verified here (unverified). Memory offsets must be re-derived per OS and build. |

### 3.2 Production lines (land, air, naval)

| Field | Content |
| --- | --- |
| Exists? | `hoi4_production_lines` R, `hoi4_production_add` A, `hoi4_production_set` A. |
| Quality / known defects | Read: exact Military/Naval collections with snapshot-scoped handles, signed active/damaged/requested factory counters, base equipment type key (`infantry_equipment_1` style ids). Says unavailable: custom variant identity, general production collection, allocation and idle capacity, eligibility, stockpile, output/efficiency/progress, cross-load identity. `production_add`: **military only; "Naval lines are not supported yet"**; chooses "the design in the selection window" (which design it picks when several exist is **unverified**). `production_set` addresses by `entry_index` + `equipment_id` (index can shift when lines are added or removed: unverified). Only 1 add and 4 sets in the gate; production was weak (414-670 PP unspent, ~31 divisions unassigned, no naval production). No remove-line, no priority, no license/lend-lease. |
| What the playbook needs | Requested versus actual output per line; equipment stockpile and deficit per type; efficiency/ramp-up; ability to pick a specific variant; naval and air lines; remove/reorder lines; lend-lease and license production; "what do my divisions need" computed from templates. |
| Proposed tool design | (a) `hoi4_equipment_stockpile` R returning stock, needed-by-divisions, per-day net, per equipment type. (b) Extend `production_lines` with `efficiency`, `output_per_day`, `progress` where memory fields can be validated; mark `source`. (c) `hoi4_production_add` gains `variant?: string` and `kind: land|air|naval` with typed refusals (`no_producible_design`, `variant_not_found`). (d) `hoi4_production_remove` A `{line_handle}`; `hoi4_production_priority` A. Receipt: the post-action `production_lines` line collection diff, not just a count. |
| Effort | Stockpile M-L (needs a new memory structure); naval/air add M (same staged flow, different category button); variant select M-L (depends on designer, 3.3); remove/priority M. |
| Priority | P0 stockpile + deficit read and naval add if the playbook builds a navy at all; P1 variant, remove, priority. |
| Platform | Mac action only. Linux has `production` under the governor (one-factory step per entry, Production006 gate; not verified for long play). Windows legacy `hoi4_production` unverified. |

### 3.3 Equipment variants and designer (tank, ship, plane)

| Field | Content |
| --- | --- |
| Exists? | **None.** |
| Quality / known defects | n/a. Only indirect: the production selection window picks an existing design. `production_lines` says custom variant identity is unavailable. |
| What the playbook needs | Design a tank (chassis, gun, armour, engine), a ship, a plane; see existing variants and their stats; choose which variant a production line builds. Experts do this every year after each module tech. |
| Proposed tool design | `hoi4_designs` R `{kind: tank|ship|plane|...}` listing designs with id, name, chassis, modules, stats, producible flag. `hoi4_design_create` A `{kind, chassis, modules: {slot: module}, name, request_id}` with receipt = design present in the design collection with matching modules. Likely a GUI-staged flow (designer window has module drop-downs). |
| Effort | L (new window, module slot geometry, name text entry). A read-only designs list first is M. |
| Priority | P1 (a Germany campaign without upgraded tank/plane variants is weak but survives; naval and air campaigns are blocked without it). |
| Platform | Mac, Linux, Windows each need their own layout; keyboard text entry must go behind `hoi4_live/platform/`. |

### 3.4 Division templates (editing)

| Field | Content |
| --- | --- |
| Exists? | `hoi4_division_templates` R (names + `queued_lines` only). Editing: **none**. |
| Quality / known defects | Read returns names only. No composition, width, stats, equipment need, supply use. In the gate it returned five German starting templates. |
| What the playbook needs | Edit or create templates (for example 7/2 or 9/1 infantry with artillery and support companies, 10/4 or 12/6 tanks, mountain/marine variants, width by front); read the composition; copy template; replace old template and upgrade divisions. FAIR mode: no exploit templates (the policy file rejects them). |
| Proposed tool design | `hoi4_template_get` R `{template}` returning regiments by column, support companies, width, hp/org/attack/defence, equipment need, manpower, supply. `hoi4_template_edit` A `{template?: new|existing, name, regiments: [[type...]...], support: [...]}` with receipt = readback composition equal to request. Include a policy check against `play_policy.json` before any input. |
| Effort | Read: M (memory structure for templates). Edit: L (drag and drop into grid; Mac drag already exists in input stack for army cards). |
| Priority | P0 for the read (the training and equipment planning depend on it); P1 for edit. |
| Platform | Mac/Linux/Windows layouts differ; legacy Windows package had no template editor either (unverified). |

### 3.5 Training and deployment

| Field | Content |
| --- | --- |
| Exists? | `hoi4_train`, `hoi4_deploy_locations`, `hoi4_division_templates`. |
| Quality / known defects | `train`: template name, `count` 1-24, `deploy_state_id`; each click verified by one more line and each location by deployment province. 4 calls in the gate, mean 7.3 s. Known history: m005 trained ~250 divisions with no equipment behind them; divisions undeployed for months before `deploy_locations` was added. No read of manpower, of equipment cover per template, of line ETA or of reinforcement. |
| What the playbook needs | Manpower, equipment availability before queuing; training ETA; stop/cancel lines; train by role quotas; set deployment per front; reinforcement/priority. |
| Proposed tool design | `hoi4_training_queue` R: lines with template, state, progress, ETA, equipment shortage. `hoi4_train_cancel` A. Add `manpower` and `equipment_cover` to the `train` receipt and refuse with `equipment_short` if the playbook sets `require_equipment: true`. |
| Effort | S-M for the read (the plan runner already counts lines), M for cancel. |
| Priority | P1 (P0 if the equipment and manpower reads from 3.1/3.2 are skipped, because they are the cause of the m005 collapse). |
| Platform | Mac only. |

### 3.6 Supply and logistics reads

| Field | Content |
| --- | --- |
| Exists? | **None.** `hoi4_army` returns raw signed strength and organisation per division only. `hoi4_overview` has no supply field. |
| Quality / known defects | m007 found Poland armies at supply 22 percent, attrition 18 percent, with no way to see it from the tools. The Windows telemetry mod (legacy) logged fuel and convoys daily; **not available** in the native Mac path (unverified whether the telemetry mod can be loaded in a campaign: AGENTS.md allows "observational telemetry only"). |
| What the playbook needs | Supply per army/state (percent, hub, railway reach), fuel stock and burn, railway/port capacity, unsupplied divisions, attrition. |
| Proposed tool design | `hoi4_supply` R `{scope: country|army|state, ids?}` returning supply percentage and source (hub/port/rail), attrition flag per division, fuel stock/day; `hoi4_logistics_alarms` R boiled down to a short list for the digest. Receipt: read counts and snapshot; no input. |
| Effort | L (needs new memory structures: per-division supply, supply area). Fallback M: GUI read of the logistics map mode and the army card supply number via OCR/sprite, labelled lower confidence. |
| Priority | P0 for a strong campaign (supply is the main land-war mechanic and a cause in m007). |
| Platform | Memory offsets per OS/build; GUI fallback depends on Retina scale on Mac. |

### 3.7 Army, army group and theatre orders

| Field | Content |
| --- | --- |
| Exists? | `hoi4_armies` R, `hoi4_army` R, `hoi4_army_create`, `_split`, `_group_create`, `_commander`, `_front`, `_offensive`, `_move`, `_plan`. Theatres: **none**. |
| Quality / known defects | `army_create` is box selection around a state; 3 of 7 calls gave `input_readback_failed` ("No new army: the divisions around this state may already belong to an army ... or none are there") and 1 gave `invalid_target` ("a new army would delete it"). m006: reassign dialog merged armies; m010: army selection failed on pop-ups, only one of three east armies advanced. `army_move` changes the target field but stacks did not move (m008) although the descriptor says it is verified by "divisions heading for the state". `army_front` is "nearest highlighted border" (one click), so it cannot be told which border or enemy it drew against; fronts existed only toward France in m008 and the heartland fell. `army_offensive` needs an existing front line and fails from the army's own state (m007). `army_commander` takes a general's name "as listed" but no tool lists the available generals or their traits (unverified how the player learns names; m007 found field marshals must be promoted and there is no tool for promotion). No `army_group` order beyond creation/commander; no theatre; no unit retreat/reinforce/rotate; no stance; no battle-plan edit/delete/waypoint. `hoi4_army` explicitly has "no persistent IDs, normalized units, enemies or actions". Division ids look like `00000033:00000005` and are per observation. |
| What the playbook needs | An ID-based assign (`division -> army`), whole-border front coverage, multi-leg offensive plans, per-army stance and retreat/hold, field marshal/general roster with traits and promote, theatre creation, rotation of tired divisions, army composition summaries (infantry vs armour, equipment, supply). |
| Proposed tool design | `hoi4_commanders` R (name, rank, traits, skill, assigned army, promotable). `hoi4_commander_promote` A. `hoi4_army_assign` A `{divisions: id[], army}` using the unit list panel plus Shift selection, receipt: army membership readback; falls back to `army_create` otherwise. `hoi4_fronts_cover` A `{army_group, borders: [{country|state_id}]}` iterating `army_front` for each border and returning which borders have a front order and which armies cover them. `hoi4_army_stance` A. `hoi4_army_orders` R: battle plan orders (type, province/state, status). `hoi4_army_retreat` A. `hoi4_theatre_create` A. |
| Effort | Roster/promotion M; assign M-L; fronts cover S-M (composition of existing `army_front` and a border list from `hoi4_states`); orders read M; stance/retreat M; theatre M-L. |
| Priority | P0: fronts cover (all borders) and commander roster/promotion, because a loss is certain without them; P1: assign, orders read, stance/retreat; P2: theatre. |
| Platform | Mac only; the same GUI cards exist on all OSs but input coordinates are platform-specific. Legacy Windows `hoi4_orders` (front/offensive/execute/hold/delete) is OCR + save based: unverified under current campaign rules. |

### 3.8 Land battle planning and encirclement support

| Field | Content |
| --- | --- |
| Exists? | **None live.** `hoi4_states` gives state id, English name, owner, controller, and `x`,`z` centre coordinates (the war executive uses these). Upstream smerss/Clausewitz-MCP has an offline encirclement search (per docs); not wired live. |
| Quality / known defects | No province adjacency, terrain, no unit positions for enemies (`hoi4_army`: "no enemies"; "hidden enemy intelligence" is forbidden by AGENTS.md, so the read must be limited to what a human sees on the map: visible units at the front), no front-line geometry, no battle results or combat width. `war_step` in the plan runner sends armies at the nearest enemy-held state by centre distance, which is a straight-line target picker, not a battle plan. |
| What the playbook needs | An adjacency graph (state or province), terrain and infrastructure, current front line segments, visible enemy stacks, pocket detection and pincer line placement; breakthrough and encirclement candidates; tracking of which of my divisions are in contact. |
| Proposed tool design | `hoi4_map_graph` R (static, cached): state adjacency, terrain, coastal flags, victory points; sourced from game files, not memory. `hoi4_front_state` R `{between?: country|tag}`: list of front segments: my state, enemy state, who controls, contact. `hoi4_visible_enemy_units` R: only units a human sees (on-map banner positions and strengths; validate that it honours FAIR visibility). `hoi4_encirclement_candidates` R: runs the offline search on the above. `hoi4_army_offensive` extended to `waypoints: state_id[]`. |
| Effort | Graph S-M (parse game files, cache); front state M; visible enemy L (memory structure or screen read); encirclement search S once inputs exist (code exists offline). |
| Priority | P0 for `front_state` (it replaces the m007/m008/m010 blind spots); P1 for graph + encirclement; visible enemies P1 and must be policy reviewed. |
| Platform | Graph is OS independent (game files, pure Rust/Python). Memory reads per OS/build. |

### 3.9 Air

| Field | Content |
| --- | --- |
| Exists? | `hoi4_air_wings` RA, `hoi4_air_mission` A. |
| Quality / known defects | `air_wings` opens the Air Forces Overview and lists wings with base, region, missions. `air_mission` takes wing name, mission and a state id and does a right-click on the strategic region containing the state. Missions: air_superiority, cas, interception, strategic_bombing, naval_strike, kamikaze, port_strike, logistics_strike, air_supply, training, naval_mines, naval_minesweeping, recon, naval_patrol. **Never called in the gate; no live evidence from it** (unverified quality). Verified "when the wing shows a changed region or mission" only. No plane count, range, fuel, org, wing creation, rebasing, air-zone view, air-superiority read. |
| What the playbook needs | Wing creation and rebasing, aircraft count and variant per wing, range check, air superiority per region, air base capacity, mission reassignment when fronts move, scramble/interception. |
| Proposed tool design | `hoi4_air_overview` R (per wing: base, planes, variant, range, mission, region, org, fuel; per region: my/their air superiority if visible). `hoi4_air_rebase` A, `hoi4_air_wing_create` A. `hoi4_air_mission` extended with `region?: string` instead of state-centre only. |
| Effort | M (overview reads exist); rebasing M; wing creation M-L. |
| Priority | P1 (needed for FAIR "air superiority" play, which the playbook asks for); P0 if the campaign nation is Britain/Japan. |
| Platform | Mac only; strategic region right-click depends on map mode state. |

### 3.10 Naval (fleets, missions, production, invasion)

| Field | Content |
| --- | --- |
| Exists? | **None** for fleets/missions/invasion/ship production. Related: dockyard via `construction_add`; `production_lines` can show naval handles but `production_add` refuses naval ("Naval lines are not supported yet"). |
| Quality / known defects | The qualification gate noted "no naval production". Legacy Windows `hoi4_navy_air` (naval_mission, air_mission, list from newest save; OCR; Strategic Navy map mode) exists in docs; not tested under current rules (unverified). |
| What the playbook needs | Fleet/task force list (ships, mission, location), create task force, assign mission (patrol, strike force, convoy raiding/escort, minelaying, naval invasion support), build ships (and design them), convoy health, port/base management, **naval invasion** (prepare, load, launch) as FAIR-legal play (no exploit spam), supply by sea. |
| Proposed tool design | `hoi4_fleets` R, `hoi4_fleet_create` A, `hoi4_fleet_mission` A `{fleet, mission, region|state_id}`, `hoi4_naval_production_add` A (same flow as land with the naval category), `hoi4_convoys` R, `hoi4_invasion_prepare` / `hoi4_invasion_launch` A. All with memory receipts from the fleet collection. |
| Effort | L for the whole domain; fleets read M, mission M, production M, invasion L. |
| Priority | P1 for naval presence (convoys, home defence), P0 only for island/naval nations or any plan that needs an amphibious assault; for Germany P2 except for dockyards and some submarines. |
| Platform | Mac/Linux/Windows layouts differ; memory structures per OS/build. |

### 3.11 Special projects

| Field | Content |
| --- | --- |
| Exists? | **Yes since 26 October 2026** (macOS only): six tools in `native/src/launch/macos/special_projects.rs`, live-validated on the owned Germany run (January to August 1938). See "3.11.1 What now exists". |
| What the playbook needs | List available projects/prototypes, allocate scientists, claim rewards. |
| Open | Reward choices (`multiple_selection_reward_window`) are not driven: an unanswered choice falls back to the option marked default after the game's timeout. Facility state names need the panel. Which scientists the roster window hides (two level-3 land scientists were absent from it) is unexplained. Linux/Windows have nothing. |

#### 3.11.1 What now exists (macOS 089EA519, `--mac-input`)

| Tool | Kind | What it does |
| --- | --- | --- |
| `hoi4_special_projects` | read, memory only, no lease, no input | `specializations[]` (breakthrough points and progress percent, facilities, visible projects), `facilities[]` (facility_id, building, specialization, scientist, running project with `progress_percent`, `iterations_completed`, `iteration_gain_percent`, `stopping`, `unread_iteration_reports`), `scientists[]` (character id, name, skills nuclear/naval/air/land, traits, `assigned_facility_id`), `recruitable{cost_political_power, candidates[]}`, `projects[]` (visible pool: key, specialization, base_days, `running_in_facility_id`; with `rules:true` also the vanilla gates read from the install: breakthrough cost, parent projects, required technologies, the `available` trigger, prototype resources, time class). |
| `hoi4_special_projects_panel` | lease, opens/closes the Research panel (W) | state name, speed and card text of each facility card; the roster's listed names and which memory scientists it hides; with `facility_id` also the project window of that facility (every visible project with `available_now`, display name, approximate time, requirement text, breakthrough cost) or the running project's progress/phase text. |
| `hoi4_special_project_assign` | action | `{facility_id, scientist?, project?}`: scientist portrait, roster row, facility icon, project entry, Start. Verified from the facility's own scientist and project slots. |
| `hoi4_special_project_stop` | action | stop button plus the game's "Stop project" confirmation (accepted); verified by the project's state machine entering the stopping state. A stop takes 10 days and refunds the breakthrough point when no iteration finished. |
| `hoi4_special_project_unassign` | action | the small button on the scientist portrait; verified when the scientist slot is empty. Refused while a project runs. |
| `hoi4_special_project_recruit` | action | Scientists, Recruit Scientist, the candidate's row and the game's "Spend N to recruit" confirmation; verified by the character joining the own scientist list. Cost 25/50/75/100 political power by idle scientists. |

A new facility is a normal construction job: `hoi4_construction_add {building: "air_facility"|"land_facility"|"naval_facility"|"nuclear_facility", state_id}` works unchanged (Experimental Facilities section of the construction panel, verified live: queue grew, one slot used). Priority of that line is not controllable by any tool yet: a facility queued behind five factory lines got zero factories until shift+click on its green priority arrow moved it to the top (manual ordinary input, 27 June 1938).

Typed refusals before any input: `facility_unknown`, `unknown_scientist` (closest names), `scientist_wrong_specialization`, `scientist_busy`, `unknown_project`, `project_wrong_specialization`, `project_busy`, `facility_has_project`, `scientist_required`, `already_assigned`, `no_project`, `already_stopping`, `no_scientist`, `unknown_candidate`, `already_recruited`, `insufficient_political_power`. Refusals at the screen without starting anything: `scientist_not_offered`, `project_not_offered`, `project_not_startable`, `unexpected_dialog`, `ui_blocked`.

Validated live (memory against the screen, 26 October 2026, a private run on 26 October 2026): facility, scientist and project counts; breakthrough 35.4 percent against the tooltip; 12.96/80.00/116.24/35.40 raw values against the bars and the 0/0/1/1 counters; recruit cost 100 with fourteen idle scientists; project progress 21 percent and one iteration against the report window; unread reports 2 after iterations 2 and 3; stop state and the 10 day stop; scientist change in a crewed facility; project start spent the land point (1 to 0) and an iteration reward added 25 percent of progress. Also live: a Germany run from 10 January to 4 October 1938 finished sp_land_military_engineering_vehicles (six iterations, iteration gaps of 14 to 40 game days, +25 percent land breakthrough progress per iteration reward, a scientist reaching level 3), then the air facility queued with `hoi4_construction_add` completed (facility 3545), a greyed Jet Engine was refused with `project_not_startable` and Radar started (1 air point spent, refunded on stop). A "Special Project Completed" notice (`CSpecialProjectFinishedPopUpWindow`, OK button) is now acknowledged by `clear_notices` (`ui.rs` NOTICE_VTABLES, the captured variant is added unseen).

Not validated: scientist experience (`xp`), reward choices, the facility card state for a facility captured or dismantled, behaviour with two or more facilities (list scrolling is implemented but untested), supportive scientists, nuclear/naval facilities.

Ordinary UI routes (game 1920x1080; coordinates come from the live GUI tree, never fixed): `W` opens Research; the tab button "Experimental Facilities" (`facilities_tab`); a facility card has two click targets: the facility icon (opens that facility's window) and the scientist portrait (opens the Scientists roster in select mode, one row per fitting scientist, click the row). In a facility window: an available project row, then the Start button; a running project shows a stop button, the iteration reports, Output and Iterations. The roster's `Recruit Scientist` button opens the candidate list (four candidates). `Escape` closes the topmost window.

### 3.12 Research and doctrines

| Field | Content |
| --- | --- |
| Exists? | `hoi4_research` R, `hoi4_technology_list` R, `hoi4_research_start` A, plus non-visible `hoi4_research_ui/nodes/node_detail/slot_control/category/folders`. |
| Quality / known defects | `research` returns completed tech ids and raw counters; **progress and days remaining are `null` live** (unverified `progress_days`). `technology_list` is catalog membership, not eligibility. `research_start` succeeds only when the slot holds the technology; refuses researched / assigned / busy slot; a tile scrolled out of view is refused (m005: "research tool could not scroll" left research idle for long stretches). 20 calls, 1 error (`invalid_target` already researched). Doctrine folders **not exercised** (MANAGEMENT_MATRIX). `hoi4_research_folders` is unsupported on native macOS. Slot argument 0..15; slot count reading depends on `hoi4_overview` research array. |
| What the playbook needs | List of currently researchable techs (eligible, with cost/ETA/bonus), tree scroll for off-screen tiles, doctrine selection and coherence check, research-slot expansion via focus/ideas, research sharing, tech-sharing, license acquisition. |
| Proposed tool design | `hoi4_research_available` R `{folder?}` returning eligible keys with cost, current progress, days left (when validated), prerequisites, bonuses. `hoi4_research_start` gains `scroll: true` (drag-pan the tree like `focus_start` already does) and refuses with `not_eligible`. `hoi4_doctrine_plan` R/A: pick a doctrine track and validate. Receipt remains slot readback. |
| Effort | Eligibility read M-L (needs visibility/eligibility memory join; descriptors list "eligibility unavailable"); tile scroll M (the focus pan flow exists); doctrine M. |
| Priority | P0 for scroll + eligibility (idle research was a recorded cause), P1 for doctrines. |
| Platform | Mac only for actions; Linux has research folder reads and governor refill that are validated there (separate layout). |

### 3.13 Military-industrial organizations (MIOs)

| Field | Content |
| --- | --- |
| Exists? | **None.** |
| What the playbook needs | Pick and fund MIOs per equipment category, assign traits. DLC-dependent (Arms Against Tyranny). |
| Proposed tool design | `hoi4_mios` R and `hoi4_mio_trait` A; staged-panel flow. |
| Effort | L (new window, DLC dependency check). Priority P2. Platform: per-OS layout; DLC presence must be detected, not assumed. |

### 3.14 Laws, advisors, designers

| Field | Content |
| --- | --- |
| Exists? | `hoi4_politics_options` RA, `hoi4_politics_choose` A. |
| Quality / known defects | `politics_options` lists ideas per slot with game-enabled state and the slot's current idea; categories `government` (slots 0 mobilization, 1 trade, 2 economy, 3-5 political advisors), `research_production` (designers), `military_staff`. 16 option calls, 9 chooses, 2 `input_readback_failed` ("idea taken: expected game change not observed"). m006: a stuck Fn flag stopped politics for 7 months. Refuses ideas the game disables. No cost (PP) or availability reason, no advisor traits, no hire/fire for specific advisors beyond slot, no cancel of an idea (unverified), no country leader or national-unity/party popularity read. |
| What the playbook needs | Cost and effects of each idea, PP budget, stability/war support impact, advisor traits, timing (conscription step-ups). |
| Proposed tool design | Extend `politics_options` with `cost_pp`, `traits`, `modifiers`, `disabled_reason`; add `party_popularity` and `stability` to the overview. Receipt remains the slot idea readback. |
| Effort | S-M (hover tooltips are OCR-able; names come from the game files for modifiers). |
| Priority | P1. |
| Platform | Mac; reads of game files are OS independent. |

### 3.15 Decisions

| Field | Content |
| --- | --- |
| Exists? | `hoi4_decisions_list` RA, `hoi4_decision_take` A. |
| Quality / known defects | List: key, `enabled`, `active_timer`. Take: refuses disabled decisions, verified when timer started or entry gone. 2 lists, 1 take in the gate, no errors. No cost, no category, no remaining days, no target selection for targeted decisions (for example per-state or per-country), no visibility for decisions that are shown but blocked, and no cancel of an active mission. m008: MEFO bills left uncancelled until 1939 (the decision `GER_cancel_mefos` appeared in the list as enabled). |
| What the playbook needs | Decision with cost, days left, effects text, targeted decisions, and a rule engine that takes cheap beneficial decisions automatically. |
| Proposed tool design | Extend list to include `cost_pp`, `days_remaining`, `category`, `needs_target`, `tooltip_text`. `hoi4_decision_take` gains `target: state_id|tag`. |
| Effort | S-M. Priority P1 (the standing plan already handles `decisions` keys, so reads are the gap). Platform: Mac; wording from localisation files is OS independent. |

### 3.16 Espionage and operatives

| Field | Content |
| --- | --- |
| Exists? | **None.** |
| What the playbook needs | Intelligence agency upgrades, operative recruitment, missions (boost ideology, infiltrate, steal tech, etc.), counter-intelligence; relevant to Germany/UK/USSR openings. |
| Proposed tool design | `hoi4_agency` R, `hoi4_agency_upgrade` A, `hoi4_operatives` R, `hoi4_operative_mission` A `{operative, mission, target}`. |
| Effort | L. Priority P2 (P1 for a campaign that wants coups/civil wars). Platform: per-OS; DLC check. |

### 3.17 Diplomacy, war goals, factions

| Field | Content |
| --- | --- |
| Exists? | `hoi4_diplomacy` RA (view actions for the owner of a state), `hoi4_diplomacy_act` A (exact listed name; Declare war, Justify War goal, Guarantee independence, Improve relations; `war_goal` plus 1..5 states by name). |
| Quality / known defects | Lists actions with enabled state, cost, whether the AI would refuse. Verified when the offered actions change or PP is spent. **Never called in the gate** (so no evidence there). m007/m008: justification took ~160 days at 50 percent world tension and ~10 days at 91 percent; a completed goal **expires in about two months** and the pollers missed it twice (m007) and once (m008). State names must be typed exactly. Addressing is by a state id of the target, so a country with no known state id cannot be addressed. Factions, volunteers, lend-lease, trade, military access, embargo: possible only if the game lists them as an action and the name matches; not verified. No reads for: current war goals and their expiry, justification progress, opinion, world tension, who is in which faction, who guarantees whom, wars list. |
| What the playbook needs | Faction create/invite/join; guarantee and call-to-arms tracking; war goal inventory with timers; world tension; alliance map; trade deals; puppets and release; capitulation and annexation. |
| Proposed tool design | `hoi4_wars` R (see 3.28), `hoi4_war_goals` R `{owner: me}` returning target, goal type, state list, status (justifying with ETA / complete with expiry date), `hoi4_world_tension` R, `hoi4_factions` R (members, leader, guarantees), `hoi4_diplomacy_act` gains `country: TAG` (so no state id is needed). Wake events: `war_goal_complete`, `war_goal_expiring`. |
| Effort | War goals read M (the justification objects exist in country memory, unverified offsets); factions M-L; tension M; `country` addressing S-M. |
| Priority | P0 for war-goal inventory with expiry and world tension (directly caused the loss of years in m005, m007, m008, m010); P1 for factions and guarantees. |
| Platform | Mac; offsets per OS. Legacy Windows `hoi4_diplomacy` has `invite_to_faction`, `lend_lease`, `military_access`, `trade` actions in docs (unverified in current builds). |

### 3.18 Peace conference

| Field | Content |
| --- | --- |
| Exists? | **Detection only**: `hoi4_events` returns `peace_conference_open`; the plan runner returns early. Legacy Windows `hoi4_peace` (status/take/submit/exit/pass) exists in docs, not in the native server. |
| Quality / known defects | n/a. MANAGEMENT_MATRIX lists "bid actions, done button" missing. |
| What the playbook needs | See war score and available demands/costs, take states, submit, pass, exit; FAIR rules on puppeting and annexation timing. |
| Proposed tool design | `hoi4_peace_status` R, `hoi4_peace_take` A `{states?: name[], all?: bool}`, `hoi4_peace_submit` A, `hoi4_peace_exit` A. Receipt: conference state change and state ownership change after the conference. |
| Effort | M-L (new window with per-state cost; GUI-only likely). Priority P1 (the first won war will hit it; without it the game stays at a stalled conference). Platform: per-OS layout. |

### 3.19 Events

| Field | Content |
| --- | --- |
| Exists? | `hoi4_events` R, `hoi4_event_choose` A; plan keys `auto_events`, `event_options`, `event_prefix_options`, `wait_events`. |
| Quality / known defects | `events`: windows in order with `title`, `options`, `answerable`, `on_top`. `event_choose` takes event id + 0-based option; verified when exactly that event window closes. 34 reads, 11 chooses, 1 `invalid_target` ("Answer the event on top first"). Notices can cover event windows (m007). Open events block every other input (`events_open`). Event consequences are not shown (option text only). m010: a country collapse with two `country_capitulated.0` events came without explanation. |
| What the playbook needs | Per-event policy (a JSON mapping event id -> option, with reasons), option effect text/tooltips, logging of every event title and choice with date. |
| Proposed tool design | `hoi4_events` returns `option_tooltips` (hover OCR, cached by event id). A policy file `event_policy.json` consumed by the plan runner (it currently has only the three keys above). `hoi4_event_log` R (date, id, title, chosen option) as a durable journal so the model can read the history without having watched. |
| Effort | S-M. Priority P1. Platform: all OS; tooltips need per-OS hover. |

### 3.20 Time control

| Field | Content |
| --- | --- |
| Exists? | `hoi4_time` A (pause/speed 0..4 with keys, readback), date and speed in `hoi4_overview`, `hoi4_wait_for` R. |
| Quality / known defects | "Sent only while the owned game window is front-most", each verified by readback; "uncertain dispatches are never replayed". 5 calls, 1 `invalid_arguments` ("Supply exactly one clock target"). `wait_for` max 50 s wall; waits on focus/research change, game date, research UI, or listed assignments. **No wait on in-game events, war state changes, battle events, alarm conditions, new open event windows or supply**. m006: the player kept speed 3 during the war; m007: speed 2-3 unattended at declaration. Pause count/seconds are required reports by AGENTS.md; a counter per session is not in any tool I read (unverified). |
| What the playbook needs | Wake on any condition in section 6; speed policy by context (fast peacetime, slow for war declaration and first battles), auto-slow on danger. |
| Proposed tool design | `hoi4_wait_for` gains `until_events: [...]` families listed in section 6 and returns the matched event with details. `hoi4_time` gains `policy: {peace: 4, war: 2, danger: 1}` handled by the server (action only when the lease holder allows it). `hoi4_session_clock` R (pause count, paused seconds, real seconds per game year). |
| Effort | wait_for events M (depends on reads); policy S; clock stats S. |
| Priority | P0 for events in wait_for (this is the "wake the model only for events" requirement of AGENTS.md); P1 for policy. |
| Platform | Key sending is per OS behind `hoi4_live/platform/`; frontmost check differs (section 5). |

### 3.21 Standing plans and the governor

| Field | Content |
| --- | --- |
| Exists? | `hoi4_plan` (get/set), `hoi4_plan_run` (foreground, up to 300 s), `hoi4_daily_governor` (start/update/digest/stop). |
| Quality / known defects | `plan_run` order: events, research, focus, politics one-shots, decisions, training, factory rebalance on priority lines, production lines added once, army hygiene (create/split/generals), war executive, construction. Plan keys (from `plan_run.rs`): `active`, `stop_on_failure`, `event_prefix_options`, `auto_events`, `event_options`, `wait_events`, `research_priority`, `research_fallback`, `focus_path`, `politics`, `decisions`, `training`, `production_priority`, `production_min_idle`, `production`, `construction`, `construction_min_idle`, `war`, `armies`. It returns early when a choice event needs the model or a peace conference opens. `hoi4_daily_governor` on Mac is a **preview**: research refill, one-factory production steps, focus queue, all default-off, "long campaigns remain unverified". It requires the lease and a stopped worker for `update`. m008: skip timers were wall-clock, so focus/research stalled for game-years; the war executive's `war_step` is a straight-line target picker; no naval, air, diplomacy, peace. `hoi4_plan_run` is a blocking tool call, so the model cannot do anything else while it runs. |
| What the playbook needs | Rules evaluated in game time, not wall time; every domain in the table above available as a rule step; a report of what ran; no overlap between the model and the runner on the same input device. |
| Proposed tool design | Move skip timers to game days. Add plan keys: `air` (mission rules per front), `diplomacy` (war-goal justify/declare windows with expiry guard), `border_cover` (fronts for all borders), `alarms` (thresholds that return control to the model), `peace` (policy). Make `plan_run` a background worker (it exists as the governor) with `digest` returning compact changes since the last digest. |
| Effort | Game-day timers S; new plan steps are as large as their underlying tools; unify plan_run/governor M. |
| Priority | P0 for game-day timers and alarms; P1 for the rest. |
| Platform | Governor on Mac and Linux are separate implementations (`launch/macos/daily.rs`, `launch/linux/clock/daily.rs`); Windows uses the legacy `hoi4_standing` (telemetry-triggered, pauses the game around rules unless `pause=false`, which conflicts with "never pause"; unverified in current form). |

### 3.22 Telemetry and observation reads

| Read | Exists? | Notes |
| --- | --- | --- |
| Date, speed, pause, country, difficulty | `hoi4_overview` | Works; 0 ms. |
| Research slot progress and ETA | `hoi4_overview`/`hoi4_research` | **null** (unverified `progress_days`). |
| Factories (civilian/military/naval) | `hoi4_economy` | Trigger counts only; no breakdown by use. |
| Political power | `hoi4_economy` | Exact rational. |
| Stability, war support, manpower | none | Explicitly unavailable for the first two; manpower not present in sampled output. |
| Resources and shortages | none | `unavailable.shortages: true`. Legacy telemetry mod covered resources on Windows (unverified in the native path). |
| Convoys / trade | none | Legacy telemetry only. |
| Supply and fuel | none | See 3.6. |
| Wars, fronts, enemy-held states | partial | Only derived from `hoi4_states` owner vs controller. No war list, war score, participants or fronts. |
| Enemy-visible frontline / units | none | Must respect human visibility (AGENTS.md forbids hidden enemy intelligence). |
| Factions, guarantees, opinion, tension | none | See 3.17. |
| Casualties and combat results | none | |
| Equipment stockpile | none | See 3.2. |
| Divisions | `hoi4_army` / `hoi4_armies` | Raw strength/org, no supply, no equipment; ids are per observation. |
| Air / naval OOB | `hoi4_air_wings` RA only | No naval. |

| Field | Content |
| --- | --- |
| Proposed tool design | `hoi4_country_status` R: stability, war support, manpower (available/total), consumer goods ratio, resource balance table (produced/consumed/imported), fuel, convoy count/escort, supply summary; each field with `source: memory|gui` and `confidence`. `hoi4_wars` R: war id, participants by side, my front states, war score if shown. `hoi4_world` R: tension, major powers' factions. All pure reads under the 0 ms read policy where memory-backed. |
| Effort | M-L (new offsets); some by GUI read of the top bar as a fallback (M). |
| Priority | P0 (stability/war support/manpower/resources/wars), P1 (convoys, world). |
| Platform | Memory offsets are per OS and build and may not be copied; unknown builds must fail explicitly (AGENTS.md). |

---

### 3.23 Doctrines, MIOs, intelligence agency and research extras (native macOS reads, new)

Four pure reads (no lease, no input, never pause; macOS build 089EA519 only, other builds fail with
`capability_unavailable`). Each result carries `source`, `build_id`, `game_time`, `freshness`, a
`validated` block and a `not_read` or `state_read:false` block naming what is not mapped. Code:
`native/src/launch/macos/{doctrines,mios,intelligence,research_extras}.rs`, shared helpers in `mgmt.rs`,
the tolerant script reader in `script.rs`, live checks in `live_reads_mgmt.rs`.

| Tool | Returns | Live-validated (26 October 2026, Germany) | Not validated or not mapped |
| --- | --- | --- | --- |
| `hoi4_doctrines {include_catalog?}` | per folder (land, naval, air, special_forces): grand doctrine (id, loc key, XP cost and type, track list), each track's subdoctrine, mastery, banked mastery, reward estimate; army/navy/air experience; temporary doctrine cost reductions with uses left; with `include_catalog`, adoptable grand doctrines or the track's subdoctrines with script cost and cost after the best reduction | grand doctrines equal the `set_grand_doctrine` lines of the history file (new_mobile_warfare, new_convoy_raiding, new_battlefield_support); experience equals the top bar (166/76/50 shown, 166.19/76.50/50.00 read); cost reductions equal the focus scripts (-0.5, one use, land and air) | track mastery fields (`validated:false`): no track held a subdoctrine on screen, scale 1/32768 inferred (banked values of all countries stay under the 200 cap, an AI track holds 549 mastery against 500 of rewards); the raw flag at track `+0x10` (`flag_0x10`) has no established meaning, so `completed_estimate` uses mastery against the script's reward thresholds only; doctrine cost modifiers (`<folder>_doctrine_cost_factor`) and availability triggers are not evaluated |
| `hoi4_mios {tag?, include_all?}` | own active designer ideas, organisation catalogue for the own tag (category, equipment, trait count, icon, matching designer idea, `allowed_for_tag` true/false/null), template count in memory | active ideas equal the politics slots (`GER_daimler_benz` plus laws and spirits); 552 organisation templates in memory | per-organisation state (size, funds, unlocked and selectable traits, policy, design team, production lines): no runtime organisation object exists on this build state, so `mio_system.state_read:false`. Designers are the active mechanism here (inference: the organisation DLC system is not in use) |
| `hoi4_intelligence {include_operations?}` | agency name, the 5 upgrade branches and 21 upgrades from the loaded database with levels, factory use and prerequisites from the scripts, operative mission keys, agency defines, optional operations catalogue | agency name "Abwehr" (and MI6 / NKVD for other countries) matches the install's agency script; branch and upgrade keys read from the live database | created flag, upgrade levels and progress, operative roster (name, nationality, skill, traits, location, operation), intel network levels towards other countries: not mapped, reported `state_read:false` |
| `hoi4_research_extras` | limited-use technology bonuses ("free research bonus": name, uses left, bonus, categories), limited-use cost reductions, idea research bonuses, technology-sharing factions | the six bonuses equal the focus scripts (four-year plan 1.0 industry spent, fortify the Vaterland 0.5 cat_fortification with 2 uses, Fallschirmjager 1.0 para_tech, ...); armor 0.1 from the Daimler-Benz idea; faction "Axis" | ahead-of-time penalty, licences (one `CEquipmentProductionLicense` exists, owner unmapped), scientists per specialization (see `hoi4_special_projects`) |

Operator notes: add the four names to the broker `READ` set (done in the private `macplay.py`); read
live with the opt-in test `cargo test --release -- --ignored live_mgmt` (sign the test binary with
`native/macos/debugger.entitlements`, `HOI4_LIVE_SESSION=<owned session.json>`).

---

## 4. Prioritized build order (top 12)

Order is by "campaign lost without it" first, then by cheapest wins that unblock the rest.

1. **Pre-checks and typed failures in the existing action tools** (S). `construction_add`
   (`no_free_slot`, `building_not_allowed`), `army_create` (`already_in_army`, `no_own_divisions`),
   `focus_start` (`not_eligible` instead of `invalid_target` after a click), `research_start`
   (`stale_target`). Targets the 10 of 31 gate errors in the construction and army_create classes and gives the model something to act on.
2. **Game-time standing plans and alarms** (S-M). Replace wall-clock skip timers with game days;
   add `alarms` thresholds to the governor digest; make the digest compact and one-call-fix
   oriented (section 6).
3. **`hoi4_country_status`** (M-L): stability, war support, manpower, consumer goods, resource balance, fuel.
4. **`hoi4_wars` + `hoi4_front_state`** (M): war list, front segments, enemy-held and contested
   states, threatened own states. This is what m006-m010 lacked.
5. **`hoi4_fronts_cover`** (S-M): one call that draws front lines for every border listed (NL, BEL,
   DEN, SWI, FRA, POL, coast), returns uncovered borders. Depends on 4 for the border list.
6. **`hoi4_war_goals` + `hoi4_world_tension` + `diplomacy_act country:`** (M): war-goal inventory with
   timers/expiry, tension, address by tag.
7. **`hoi4_supply`** (L; M with GUI fallback).
8. **Event-driven `hoi4_wait_for`** (M): `until_events` families in section 6.
9. **Research: tile scroll and eligibility** (M) plus doctrine list/plan.
10. **Division template read, then editor** (M then L); equipment stockpile/deficit read (M-L).
11. **Commander roster, promote, `army_assign`** (M-L).
12. **Peace conference actions** (M-L); then air overview/rebase (M), naval fleets/production (L),
    special projects/MIO/espionage (L, P2).

Items 1, 2, 5 and 8 are mostly compositions of what exists; 3, 4, 6, 7 need new memory or GUI
reads that must be validated against own-country screen state before use (repo rule: no copied offsets).

---

## 5. Reliability defects

Error codes are exactly as logged in the private 25 October gate run's `calls.jsonl` (31 errors in 415 calls;
counts below are from that file). "Likely cause" is my inference unless marked as documented.

| Code / symptom | Tool (count) | Observed message | Likely cause | Fix direction |
| --- | --- | --- | --- | --- |
| `input_readback_failed` | `hoi4_construction_add` (7 of 29) | "The centre click queued nothing more in this state (no free building slot, or the building is not allowed there)" | State had no free slot or building not allowed; the player did not call `state_slots` first; click-at-centre cannot distinguish slot exhaustion from a misplaced click. The OUTCOME calls these "select-building readback failures"; the logged code is the centre-click readback, not a select stage (the select stage may also fail: unverified). | Pre-check slots and eligibility; return typed codes; stop retrying a state on failure. |
| `input_readback_failed` | `hoi4_army_create` (3 of 7) | "No new army: the divisions around this state may already belong to an army (see hoi4_armies), or none are there" | Box selection picks only unassigned divisions; the state's divisions already belong to an army, or the box misses stacked units; the call is a no-op yet still costs 3.7 s average. | ID-based assign; pre-check against `hoi4_armies` (`unassigned` ids and their `states`). |
| `invalid_target` | `hoi4_army_create` (1) | "Divisions of an existing army stand here, so a new army would delete it" | Safe guard working as designed. | Surface in the digest so the model uses `army_split`. |
| `input_readback_failed` | `hoi4_focus_start` (2) | "open focus detail: expected game change not observed" | Detail window did not open: tree pan did not bring the node in view, a popup stole the click, or the node was clicked while the game was animating. Cause unconfirmed. | Re-observe with `hoi4_focus_selected_detail` before declaring failure; retry with search stage (the Linux flow has a search stage; Mac uses pan). |
| `invalid_target` | `hoi4_focus_start` (3) | "The game disables Start for this focus (requirements unmet or exclusive)" | No eligibility read, so the player tried focuses with unmet requirements (for example Anschluss needs >50k infantry equipment per m008). | Expose `focus_list` eligibility and unmet requirements text. |
| `input_target_not_frontmost` | `hoi4_focus_start` (2) | "The owned game window is not the front-most window; no key was sent" | **Foreground theft** by the Claude app and `loginwindow` (documented in OUTCOME). A shell loop `frontguard.sh` refronted the game every ~3 s (log shows refronts #1-#7 in 15 s) so the guard is a workaround outside the product. | Native frontmost keeper that activates the game before each stage and refuses with a retry-after; run the game on a dedicated display or Space; prevent screen lock/loginwindow (display sleep); distinguish "stolen" from "game window absent". |
| `ui_blocked` | `hoi4_construction_add`, `hoi4_focus_start` (1 each) | "Notice pop-ups did not close" | Notices can sit on top of event windows or cover the click points (m007); the closer clicked the wrong thing or notices re-spawned. | A notice classifier in `hoi4_events` (kind: news/notice/event) and a dismiss primitive that returns what it closed. |
| `events_open` | `hoi4_construction_add` (3) | "Answer the open event(s) first: news.288" | Guard is correct; the player acted without checking `hoi4_events` first. | Return the events list and `suggested_option` in the error body; let plan runner pre-answer. |
| `input_readback_failed` | `hoi4_politics_choose` (2) | "idea taken: expected game change not observed" | Idea possibly not applied (insufficient PP, slot locked, time too short for the readback) or the click missed. Cause unconfirmed. m006: a stuck Fn modifier stopped politics for 7 months. | Include PP cost and `disabled_reason`; clear modifier keys before every input (documented cause in m006). |
| `input_geometry_unverified` | `hoi4_focus_start` (1) | "Owned window is not an unscaled windowed game of the expected size" | The window was resized, moved or scaled (display switch to 1920x1243 or the display holder dying); 1 occurrence. | Re-apply the window geometry profile (window size, position, scale) automatically before stage 1; restore the display mode if the holder died. |
| `invalid_target` | `hoi4_event_choose` (1) | "Answer the event on top first: mtg_news.3" | Order guard; player picked the wrong window. | Accept `event: "top"`. |
| `invalid_arguments` | `hoi4_time` (1), `hoi4_states` (1) | "Supply exactly one clock target"; "Empty state query" | Model misuse; schema allows both unset. | Tighten `oneOf` schemas so the client rejects early. |
| `invalid_target` | `hoi4_research_start` (1) | "Technology already researched" | Stale view. | Include `researched` in research slot suggestion; refresh before calls. |
| `not_allowed` | `hoi4_input_geometry` (1) | "Tool not available to the player" | Broker allowlist. | Intended. |

Process and infrastructure defects (from the OUTCOME and run logs, not from error codes):

- **Session restart killing processes.** Session 01 was interrupted by a tool-session restart
  that killed the broker, the recorder, the display holder and the launch supervisor. The game
  survived and paused itself at 1937.10.31; the recording has a gap (14:39-15:07). Likely cause:
  the helper processes were children of the tool session's process group and received its
  teardown signal. Fix: start them under `launchd` (a user LaunchAgent) or at minimum `setsid`
  with detached stdio; on restart, a resume routine that re-acquires the lease (the lease token
  dies with the broker) and re-attaches to the game by PID plus creation time (AGENTS.md: check
  PID creation and executable ownership, not PID alone).
- **Display holder not detached.** `displayhold` (the helper that keeps the app-only 1920x1243
  display switch alive) was not detached from the session, so it died with it; the game window
  then no longer matched the expected geometry. Fix: detach it (own session, LaunchAgent) and
  have the broker verify the display mode before each session; restore the user's mode when the
  owned game exits.
- **Foreground theft** (above) is the largest single source of missed input; also documented in
  the memory note "Claude app holds input/capture permissions".
- **Recording gaps** when helpers die; the qualification run is explicitly "not accepted".
- **Pace**: 17-26 real minutes per game year while the player runs; 415 calls for two game
  years, with `hoi4_wait_for` averaging 36.9 s wall per call. The pace target in AGENTS.md
  ("very fast") is not met; the cost is model turns and low-yield waits, not tool latency.
- **Wall-clock skip timers** in the governor (m008): stalled focus/research for game-years.
- **Pollers and watchers crashing** (m007, m008): war goal expired unseen. The watcher must be
  part of the server, not a script.
- **Unexplained collapse** (m005, m010): government flipped to a "German Republic" and divisions
  vanished. m010's cause is not found (two `country_capitulated.0` events). Needs an event log
  (3.19) and a war/alarm read to diagnose; do not assume it was tooling.
- **`army_move`** (m008): target field changed but stacks did not move; the verification
  ("divisions heading for the state or stopping short") accepted it. Verify position change over
  time, not the order field.

---

## 6. Fast-paced play: standing plans versus model wake-ups

Principle (AGENTS.md): the game keeps running; reads never pause; the model wakes only for
events, deadlines or real decisions; avoidable pauses are defects. Today `hoi4_wait_for` cannot
wake on most of these, so the model polls. The tables propose which rules to hand to a durable
standing plan (governor) and which to push to the model, plus the event triggers needed.
Everything in "needs" is **not implemented** unless the first column names an existing plan key.

### 6.1 Run as durable daily standing plans (no model)

| Rule | Existing support | Needs |
| --- | --- | --- |
| Fill empty research slots from a priority list, doctrine order | `research_priority`, `research_fallback`; governor research refill (Mac preview) | Eligibility read; tile scroll; game-day timers |
| Keep focus slot filled from `focus_path` | `focus_path`; governor focus (preview) | Eligibility; skip focuses blocked by unmet requirements |
| Answer news and single-option events, apply an event policy | `auto_events`, `event_options`, `event_prefix_options` | Event policy file and event log |
| Take cheap beneficial decisions; cancel harmful ones | `decisions` | Cost/days-left reads |
| Keep training queue topped up per template quota, with deployment state | `training` | Manpower and equipment-cover guard |
| Move idle military factories to priority equipment lines | `production_priority`, `production_min_idle` | Stockpile/deficit read |
| Spend idle civilian factories on construction from a state list | `construction`, `construction_min_idle` | Slots pre-check; consumer goods guard |
| Keep every division in an army, size limits, generals | `armies` | ID-based assign; commander roster |
| Draw and execute fronts toward enemy-held states | `war` (preview executor) | `fronts_cover` for every border; reserve/home-defence policy |
| Politics one-shots (conscription steps, economy law, designers) | `politics` | Cost/PP guard |
| Justify war goals as soon as tension allows; declare inside the validity window | none | `war_goals` read with expiry; tension read |
| Air missions per front, rebase after front moves | none | Air overview, rebase |
| Consumer goods and shortage rebalancing | none | Shortage read |
| Speed policy (peace fast, war slow) | none | Time policy in server |

### 6.2 Wake the model (decisions or alarms)

Proposed triggers for `hoi4_wait_for` (`until_events`) and the digest `alarms` list. Each wake
carries a compact payload and the single recommended one-call fix.

| Trigger | Why the model | Payload |
| --- | --- | --- |
| `choice_event`: event with more than one option and no policy entry | A real decision | event id, title, options, tooltips, deadline if any |
| `war_started` / `war_declared_on_us` / `ally_called` | Strategy change | belligerents, my fronts, nearest enemy-held states |
| `front_uncovered`: a border with enemy-reachable provinces has no front order or no army | Home defence (m006/m008/m009) | border, nearest armies, spare divisions |
| `own_state_lost` / `own_state_contested` | Loss | state, by whom, divisions nearby |
| `landing_or_paradrop_detected` near own territory (visible only) | Coastal defence (m007) | location, visible strength |
| `war_goal_complete` and `war_goal_expiring` (for example 14 days left) | Declare or lose it (m007, m008) | target, states, expiry date |
| `peace_conference_open` | Demand selection | war score, available states |
| `capitulation_risk`: war score or states lost beyond threshold, or `country_capitulated.*` event | Prevent collapse (m005, m010) | current war support/stability, losses |
| `supply_low`: army supply below threshold | Reposition | army, percent, nearest hub |
| `equipment_deficit`: divisions short beyond a threshold | Re-plan production | type, deficit, lines |
| `consumer_goods_high` / `shortage`: resource or goods below threshold | Rebalance | table |
| `stability_low` / `war_support_low` / `pp_overflow` | Politics decisions | values |
| `no_research` / `no_focus` longer than N game days with the plan unable to fill | Plan defect | reason code |
| `plan_blocked`: governor step repeatedly refused | Defect | step, code, last error |
| `lease_lost` / `game_not_frontmost_for_N_s` / `display_mismatch` | Infrastructure | what was lost, recovery done |
| `deadline`: scheduled game date reached (for example a planned declaration window) | Planned decision | plan note |

### 6.3 Pacing and speed policy

- Peacetime: speed 4, wake only on events, daily plan on; model turn every few game months for a
  health review (digest only).
- Before a declaration: set speed 2, confirm borders covered, units positioned; then proceed.
- War: speed 2-3 with `front_uncovered`, `own_state_lost`, `landing_*` always armed; model reviews
  every game week at most; never unattended without alarms armed (m006/m007/m008/m009 all lost
  while unattended).
- Report per session: pause count, pause seconds, real minutes per game year. This needs a server
  counter (3.20); I did not find one.

---

## 6b. Naval, air and design reads (macOS, memory only)

Added on `codex/mac-management-tools` (26 October 2026), all pure reads: no lease, no input, no cursor,
never pause. Code: `native/src/launch/macos/{fleetmem,naval,airread,designs}.rs`; opt-in live checks in
`live_reads_navalair.rs`. Offsets are macOS 089EA519 only (Germany, April 1938 save state) and are listed in
the module constants; nothing is copied from Linux or Windows.

| Tool | Returns | Evidence |
|---|---|---|
| `hoi4_fleets {ships?, fleet?}` | fleets, task forces (port province, state, strategic region, ship count by type, summed and percent strength, raw mission code), optional ships (id, name, type, hull, variant, strength, max strength, max organisation, experience, morale) | Names, hulls, variants and port locations (provinces 241/6332/6389 -> states 56/763/58) match `history/units/GER_1936_naval_mtg.txt`; max organisation matches `common/units/*.txt` (heavy/light cruiser 40, destroyer 35) |
| `hoi4_convoys {}` | convoy count, own naval bases with level, convoy production lines | Count 194 equals the top bar on 10 April 1938; base levels 241=8, 6332=5 match `history/states` |
| `hoi4_air {wing?}` | wings (aircraft type, variant, aircraft, manpower, base, missions, region) and air bases (state, level = capacity/200, capacity, stationed, free) | Wing names per base match `history/units/GER_1936_air_bba.txt`; capacity/200 matches states 50 and 51 |
| `hoi4_designs {kind?, custom_only?, detail?, name?}` | the 48 own designs: id, name, hull/chassis, parent design, manpower, production lines using it; detail adds non-zero stats and module per slot | Names and hulls match the vanilla ship/plane designs; 168 default-variant stat values match the equipment files (27 differ by modules/bonuses) |

Registered in `tools()`, `supports()` and `call` of `launch/macos.rs`; for the broker add the four names to the
READ set of `macplay.py`. `hoi4_air_wings` (panel read with input) is unchanged.

Not validated (`validated:false` in the output): the mission code of a task force (0 = idle, 1 = AI task force
holding a region are the only observed readings), ship morale and experience points, air mission mask and
region id (decoded from other countries' wings; no own wing had a mission), aircraft count growth over time.
Not found: admiral of a fleet (the only leader pointers found belonged to foreign fleets), fuel, supply, current
organisation, needed/escort/lost convoys and convoy routes, the obsolete and producible flags of a design,
air superiority per region, aces. Design creation and fleet/air orders are untouched (see 3.3, 3.9, 3.10).

---

## 7. Cross-platform notes

- **macOS (this audit)**: all action tools are in `native/src/launch/macos.rs`, built from ordinary
  input (mouse, keyboard, drag) located from live GUI memory and verified by memory readback.
  `hoi4_research_folders` is `unsupported_platform` on native macOS; `hoi4_focus_list` reports
  the catalog only. Foreground theft is a macOS-specific problem (Claude app, `loginwindow`,
  Spaces); display switching and the display holder are macOS-specific. Task-access denial must
  fall back to telemetry/UI (AGENTS.md); `hoi4_input_lease` is the shared lock.
- **Linux**: has its own native launcher set (`launch/linux`), verified research/focus/production
  and construction layouts, a governor with navigation stages and a digest with `details`;
  descriptors say army, naval and other economy domains are unavailable there and that Mac panel
  layouts must not be borrowed. Memory offsets are not interchangeable with Mac.
- **Windows**: the downstream consumer; today the legacy Python package (`hoi4_live/tools`) with
  OCR, a telemetry mod, saves, and tools such as `hoi4_navy_air`, `hoi4_peace`, `hoi4_orders`,
  `hoi4_standing`, `hoi4_advance` is documented in `docs/tools.md`. That package also exposes
  console/`hoi4_eval_effect`/`hoi4_fire_event`, which campaign rules forbid; those must stay
  inaccessible to campaign automation. I did not run or verify any of the Windows tools for this
  document: treat their descriptions as unverified. The Windows machine is reserved for separate
  mod testing, so none of it should be driven from the Mac continuation.
- Design rule for every new tool above: put native APIs behind `hoi4_live/platform/` (Python) or
  `native/src/platform/` (Rust); capability errors rather than eager imports; per-OS marker tests;
  never copy offsets across platforms or builds; unknown builds must fail with a typed error.

---

## 8. What I could not verify

- Whether `hoi4_production_add` picks the newest, the first, or a specific design when several
  exist.
- Whether `hoi4_army_commander` can learn the general names on its own (there is no roster tool;
  names may be returned in a refusal) and how field marshal promotion is done (m007 says it must
  be done, no tool is listed).
- Behaviour of `army_front`, `army_plan`, `army_offensive`, `army_move`, `army_group_create`,
  `army_split`, `state_slots`, `diplomacy(_act)`, `air_wings`, `air_mission` and `plan_run` in the
  qualification gate: never called there. The only evidence is the narrative notes in m005-m010,
  which pre-date later fixes.
- Whether `manpower`, `stability` and `war_support` can be read from memory on the verified build;
  the economy tool says unavailable for the latter two, and I found no manpower field in the
  sampled outputs.
- Whether factions can be created or joined through `hoi4_diplomacy_act` (depends on the action
  names the game lists).
- Whether the Windows legacy tools still work against the current build and their compatibility
  with campaign rules (observational telemetry only).
- Exact source of the "select-building readback failures" label in the OUTCOME; the logged code
  for all seven failures is the centre-click readback.
- The cause of the m010 government flip and of the focus-detail readback failures.
- Pause counters: I did not find a tool that reports pause count/seconds and real time per game
  year, though AGENTS.md requires the report.

## Resources, shortages and industry reads (26 October 2026)

Three pure memory reads, no lease, no input, never pause (`native/src/launch/macos/resources.rs`,
registered in `launch/macos.rs`; also reachable without any input binding through
`native/src/bin/macprobe.rs SESSION TOOL...`):

- `hoi4_resources`: per oil/aluminium/rubber/tungsten/steel/chromium/coal `produced` (gross per
  day), `available_for_production`, `needed_by_production`, `balance`, `shortage{short,deficit}`
  and the ten raw pool `slots`; plus `fuel{stock,capacity,ratio}`. Imports/exports, shortage
  penalty percent, fuel gain/days left are `null` with a reason.
- `hoi4_production_shortages`: every military/naval line with the resources it was allocated per
  day and which of them are in country-wide deficit. Per-line efficiency penalty is `null`.
- `hoi4_industry`: civilian/military/dockyard counts, PP; consumer goods, output modifier,
  trade-civilian usage, exact stability/war support, PP daily gain are `null` with reasons
  (stability and war support are computed on demand: no stored fixed or float value in the
  country or politics object; use `hoi4_country_status` for the UI-rounded top bar text).

Offsets (macOS 089EA519 only): `country+0xf80` CCountryResources (ten embedded
CStrategicResourcePool, found by vtable scan), resource ids from the resource database global
`0x34d0bd0`, `country+0x12e0` CCountryFuelStatus (`+8` stock, `+0x10` capacity), line `+0x58`
CProductionResourceCost.

Validated live (Germany, a private run on 26 October 2026): slot3 + slot5 equals the Production
view resource strip for all six shown resources in two screenshots (aluminium 13, rubber -9,
tungsten -6, steel -13, chromium -1, coal 35); aluminium need equals the sum of line
allocations; resource ids match state resource sums; industry total equals the top bar number
(85); fuel stock rises about 33 per game day. Not validated: meaning of slots 1, 2, 6, 7, 8
(trade, 80% pool), fuel against the top bar "days" text, any trade/convoy value (Trade and
Logistics views were never open), consumer goods.
## 9. Update: war and diplomacy state reads (macOS, 25 Oct 2026)

New read tool `hoi4_diplomacy_state` and Stage B of `hoi4_wars`
(`native/src/launch/macos/diplomacy_state.rs`). Pure memory reads: no input, no lease, no
pause. Offsets were found by vtable scans on a live Germany 1936 game (provenance in the module
header), not copied from Linux or Windows.

- Found and live-checked: `CCountry+0xd30` -> `CDiplomacyStatus`; list of relation objects at
  `+0x20` (each relation is held by both parties: pointer identity pairs wars); relation
  classes war, guarantee, subject (with autonomy key), non-aggression pact, military access;
  `GS+0x3b8` faction system (name, members; Axis currently has Germany); `GS+0x650` world
  tension (`/100000`); date raw to calendar (no leap years). Relation lists matched
  `history/countries` (FRA/CZE/ROM/YUG/ITA guarantees, ENG and other subjects, ENG-IRQ access).
- Not found (null with `offset_not_validated`): war sides and leaders, war score, goal
  justification progress and expiry (goal objects `CWarGoal` are listed unclassified), faction
  leader/goals/rules, opinion (`CRelationStatus` array at `CCountry+0x1a00`, modifier layout
  unknown), volunteers, the diplomacy action list. The own war list was only exercised at
  peace and structurally on the Spanish civil war: re-check when the player has a war.
- 3.17 gap closed: wars list, faction membership, guarantees given and received, world
  tension. Still open: war goal timers (P0), `hoi4_diplomacy_act` by country.

---

## 9. Unit-level reads added on macOS (26 October 2026)

Pure memory reads (no lease, no input, no cursor, no pause; 5 to 20 ms each). Code:
`native/src/launch/macos/{units,template_get,supply,commanders}.rs`. Offsets are for Mach-O 089EA519
only. Opt-in live checks: `HOI4_LIVE_SESSION=<session.json> cargo test --release live_ -- --ignored`
(the test binary needs the debugger entitlement signature).

- `hoi4_template_get {template?}`: columns (x then rows), support companies, width, hit points, base
  organisation (plus the live organisation and the country modifier factor from a live division),
  manpower, supply use, equipment need per archetype, held variants with per-piece stats, and an
  equipment-derived stat estimate (`validated:false`). Closes the "read" half of 3.4.
- `hoi4_supply {strength_below?}`: per division id/army/template/state/province, hit points against
  the template, manpower, equipment have/need/fill per archetype and shortage list, state
  infrastructure; per army means; per state building levels. The supply percentage, flow, railway
  level, hub and state supply capacity are **not readable yet** (`supply_available:false`, the
  `below_70`/`below_50` lists are empty and say nothing about real supply). Partly closes 3.5/3.6.
- `hoi4_commanders` now returns skill, attack, defense, planning, logistics, traits and character id
  for every army leader of the country (36 for Germany, including leaders the army panel does not
  list). Base values from the character template; rank of unassigned leaders and levels gained
  in the campaign are not available.
- Validation (live, Germany 1938): composition equals `history/units/GER_1936.txt` for all five
  templates; sub-unit stats offset-voted against `common/units` (99 definitions); hit points and
  manpower of all 38 divisions equal the template sums exactly; organisation equals the template mean
  times one factor (1.13); equipment held equals need on every division; leader skills and traits
  equal `common/characters` for 35 of 36 leaders (the 36th is a CHI-id character).
