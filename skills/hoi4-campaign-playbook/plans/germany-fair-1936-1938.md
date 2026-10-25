# Germany FAIR standing plan, 1936-01-01 to 1938-01-01

Companion to `germany-fair-1936-1938.plan.json` (JSON has no comments, so every
section is explained here). Copy the JSON to the `plan.json` the runner reads
(see `native/src/plan_run.rs`; the file is re-read every tick). Keys that start
with `_` are ignored by the runner; `_validator` only feeds
`scripts/validate_plan.py`.

```
python3 scripts/validate_plan.py skills/hoi4-campaign-playbook/plans/germany-fair-1936-1938.plan.json
```

Every key below was checked against HOI4 1.19.3 in the installed game files
(technologies, focus tree `german_focus`, equipment, buildings, history states,
ideas, characters, decisions). The validator output is clean (0 errors, 0
warnings). What the files cannot prove is listed at the end.

Evidence sources: [economy-build-orders](../references/economy-build-orders.md),
[research-doctrines-projects](../references/research-doctrines-projects.md),
[templates-equipment-production](../references/templates-equipment-production.md),
[supply-logistics-deployment](../references/supply-logistics-deployment.md),
[politics-diplomacy-espionage](../references/politics-diplomacy-espionage.md),
[germany-fair](../references/germany-fair.md) (checkpoint table). Where the
playbook and the installed files disagree, the files won (see "Corrections").

## Runner behaviour that shapes the plan

* Each tick does one action, in this order: events, research, focus, politics,
  decisions, training, military-factory rebalance, `production` lines,
  construction, armies. A refused action is skipped for 1-3 game days.
* Research and focus lists are priority lists: the runner takes the first
  not-completed key **the game accepts**. It only tries the first 3 unresearched
  research keys (8 focus keys) per tick, so a locked head costs days. The list is
  therefore ordered so that prerequisites always come first.
* `stop_on_date` ends the plan on 1938.01.01. Extend it (and the focus tail) for
  the Sudetenland/Czech phase.

## Top-level keys

| Key | Value | Why / when to deviate |
|---|---|---|
| `active`, `stop_on_failure` | true / false | A failed action must not freeze the routine. Set `stop_on_failure` true only while debugging a new build. |
| `stop_on_date` | `"1938.01.01"` | The plan's window. Move it only together with the focus tail and the construction order. |
| `auto_events`, `wait_events_days` | true / 3 | News and single-option events are answered with option 0. Multi-option events wait 3 game days for the model to look; then option 0 is used. |
| `event_options` | `germany_mefo_bills.1: 0` | Option 0 is "Rearmament has always been our goal" (Economy of Conquest). Only relevant at the first war; choose 1 only for a deliberate peace economy. Oster Conspiracy, Czech/Munich and Anschluss events are left to the model on purpose. |
| `alarms` | PP 200, consumer goods 0.45, idle civ 4, idle mil 2, unassigned divisions 2 | Digest thresholds only. Lower the PP alarm if you want laws bought faster. |

## research_priority (71 technologies)

Germany starts with 4 slots (5th from Heraeus). At roughly 100 days per cost
point, two years finish about 25-30 techs, so entries beyond ~35 are the tail
for 1938-39; the runner's catalog fallback (`research_fallback: true`) fills any
slot the list cannot. Start techs are taken from `history/countries/GER - Germany.txt`
(1936 block only): the game already has infantry_weapons1, tech_engineers, tech_recon,
tech_support, tech_trucks, motorised_infantry, gw_artillery, interwar_antiair, fuel_silos,
fuel_refining and the Panzer I/II line.

| Positions | Block | Reason |
|---|---|---|
| 1-4 | basic_machine_tools, construction1, electronic_mechanical_engineering, interwar_artillery | Playbook day-one four. Industry first. The Four Year Plan tech bonus (industry, 1 use) lands around 1936-02-05: it is spent on the next industry tech started, improved_machine_tools or construction2. |
| 5-17 | radio, improved_machine_tools, construction2, interwar_antitank, logistics and maintenance companies, concentrated_industry (+2), armor_tech_1, improved_equipment_conversion, excavation1, infantry_weapons2, support_weapons | 1937 economy and support. concentrated_industry (+15% factory output) over dispersed because Germany builds in few big states. infantry_weapons2 is a year ahead of date at this point and is accepted deliberately (every division uses it). `support_weapons` is not a German start tech; support_weapons2 needs it. |
| 18-32 | fuel_refining2, signal/field-hospital companies, mechanical_computing, oil/rubber processing, excavation2, improved_radio, radio_detection, basic_fortification_tech, armor_tech_2, engine_tech_2, basic_medium_tank_chassis, land_fort_tech_1, computing_machine | Resource independence, signals, the medium-tank chassis (1938, needs only gwtank_chassis). |
| 33-45 | basic_medium_tank, artillery1, antiair1, antitank1, improved_infantry_weapons, advanced_machine_tools, construction3, concentrated_industry3, advanced_equipment_conversion, excavation3, tech_engineers2, tech_recon2 | 1939 on-date items; by 1938 they are 0.5-1 year ahead. |
| 46-71 | second-tier support companies, wartime_train, aa_hmg, range_improvements, encryption/decryption, cavity_magnatron, military police, third oil/rubber/fuel tiers, 1940 aircraft and artillery, mechanised_infantry, improved_infantry_weapons_2 | Tail so the list never runs dry. 1940 items are flagged by the validator only beyond position 45 and only if more than two years ahead. |

Deviate when: Chief of Army changes or a war starts (move artillery1/antitank1 up);
you decide on dispersed industry (replace the concentrated_industry chain, they are
exclusive); a focus bonus (see below) makes a specific tech nearly free; the
digest shows an idle slot (the fallback already covers this, but the order then
is the game's, not ours). Doctrines are XP, not slots, and are not in this list.

Focus tech bonuses worth timing: Four Year Plan (industry), Heraeus (electronics),
Advanced Panzer Research (medium armor, only in the fallback list here).

## focus_path (23 focuses) and focus_fallback

All keys exist in `common/national_focus/germany.txt`; each prerequisite block has a
member earlier in the path; no two exclusive focuses appear together. Costs are in
focus points (7 days each).

| # | Focus | Cost | Note |
|---|---|---|---|
| 1 | GER_the_four_year_plan | 5 | Exclusive with Prioritize Economic Growth (never listed). |
| 2 | GER_remilitarize_the_rhineland | 5 | Needs fascism and control of states 42, 51, 978. Exclusive with the Oppose Hitler roots (never listed). |
| 3-4 | GER_accelerate_the_rearmament_program, GER_establish_production_targets | 5+5 | +2 and +1 arms factories. |
| 5-6 | GER_construct_the_reichsautobahn, GER_kdf_wagen_factories | 5+5 | Unlocks Heraeus. |
| 7-8 | GER_develop_modern_maneuver_warfare, GER_fortify_the_vaterland | 5+5 | Fortify unlocks the Westwall decision. |
| 9 | GER_reorganize_the_wehrmacht | 5 | Exclusive with Heed von Neurath (not listed); only this one leads to the Molotov-Ribbentrop path. |
| 10 | GER_expanding_the_luftwaffe | 5 | |
| 11 | GER_concentrated_armament_program | 5 | |
| 12 | GER_develop_heraeus_facilities | 10 | Only available with more than 49 civilian factories; refused until then and retried after each focus. 5th research slot. |
| 13 | GER_anschluss | 10 | Deliberately before the fillers: when the Austrian strength ratio (or 500k manpower and 50k rifles) is met it is taken at the next free focus slot instead of being delayed by cheap focuses. Refused daily until then. |
| 14-20 | aeronautical_research_institute, form_the_jagdwaffe, adopt_new_panzer_doctrine, panzer_troops_school, bribe_senior_officers, autonomy_in_the_kriegsschulen, expand_the_truppenschulen | 5 each | Fillers with real value (arms factory, air/armor bonuses, manpower, support tech). `bribe_senior_officers` is required by autonomy_in_the_kriegsschulen (the playbook list omitted it). |
| 21-23 | GER_demand_sudetenland, GER_first_vienna_award, GER_fate_of_czechoslovakia | 10, 5, 10 | After the window: Sudetenland needs 725k army manpower and a completed Anschluss, Vienna Award a friendly Hungary, Fate of Czechoslovakia 750k. They only start if everything above is done or refused. |

`focus_fallback` (tried when the path offers nothing): fallschirmjager,
develop_the_knickebein, reorganize_the_luftwaffe, advanced_panzer_research,
strengthen_the_kriegsmarine, re_establish_the_seekriegsleitung, trade_interdiction.
They are low-value but harmless, never exclusive with the path, and keep the focus
slot busy (the runner's `focus_fallback` support is being added concurrently; if
your build ignores the key it is simply unused).

Never add: `GER_oppose_hitler*`, `GER_re_establish_free_elections*`,
`GER_start_the_proletarian_revolution`, `GER_prioritize_economic_growth`,
`GER_autarky_efforts` (locks out Free Trade and Export Focus, see politics),
`GER_danzig_for_slovakia`, `GER_influence_the_benelux`, `GER_befriend_czechoslovakia`.

Deviate when: Anschluss is still refused at the end of 1937 (no change needed, the
fillers run); you prefer Heed von Neurath (war against the USSR first): swap focus 9 for
`GER_heed_von_neuraths_concerns` and drop Molotov-Ribbentrop later; you want the
Prussian Legacy branch: replace adopt_new_panzer_doctrine and panzer_troops_school by
`GER_the_prussian_legacy` (exclusive).

## politics

`hoi4_politics_choose` takes `category` (government | research_production |
military_staff), `slot` and `idea`. For `government` the tool documents slot 0
mobilisation law, 1 trade law, 2 economy law, 3-5 political advisors.

| Pick | Cost | Why |
|---|---|---|
| government 3 `GER_hjalmar_schacht` | 75 PP | Arms and dockyard speed while the MEFO modifier runs; disappears when MEFO ends, which is fine for this window. |
| government 1 `free_trade` | 150 PP | Replaces `limited_exports`: +15% factory output and construction speed, +10% research. Available only while GER_autarky_efforts is NOT completed, which is why that focus is excluded. |
| government 0 `extensive_conscription` | 150 PP | Fascist government qualifies; larger pool for the 725k/750k/950k manpower gates. No factory cost. |
| government 4 `GER_joachim_von_ribbentrop` | 50 PP | Becomes available only after Reorganize the Wehrmacht (late 1936); until then the pick fails and retries daily. |

Not picked on purpose: `war_economy` (needs war support above 0.5; consumer goods 20% instead of 25%,
arms speed +20% but conversion -20%; switch about six months before the first war, i.e. late 1938), `totaler_krieg_economy`, Walther Funk (needs Schacht
not hired), Goering (inner-circle only). Slots of `research_production` (designers) and
`military_staff` (chiefs, theorists, high command) have no verified slot numbering, so
nothing is automated there: call `hoi4_politics_options` once per slot and add picks (army
chief `GER_ludwig_beck` at 100 PP is the obvious first one; Wever/Udet/Guderian theorists
later).

PP budget: start about 130-150 PP, roughly +2/day. The order above buys Schacht first (cheap
and immediate), then Free Trade, then conscription. Decisions draw from the same pool.

## decisions

`GER_construct_the_westwall` (25 command power and 15 PP, 90 days, uses 2 civilian
factories during the 90 days): becomes enabled when Fortify the Vaterland is done
(about 1936-10) and states 42, 51, 978 are controlled and not demilitarised.
It runs as soon as it is enabled, so civilian construction loses 2 factories for 90
days in late 1936. If that bothers you, remove it and take it by hand in 1938.

Price-control decisions (`GER_import_tariffs` 15 PP, -0.15 consumer-goods factor;
`GER_guns_before_butter`, `GER_arbeit_und_brot`, `GER_collect_scrap_metal` 30 PP, -0.05
each) are intentionally **not** in the list: they fire the moment they are enabled and
the factor starts at only 0.10 (+0.03 per month), so an early take wastes PP. Add them
to `decisions` when the digest's `consumer_goods_ratio` alarm shows the factor above ~0.4.
`GER_implement_rations` and `GER_implement_wage_controls` cost 2.5% stability each: leave
them to the model. `GER_cancel_mefos` is never automated.

## training and armies

* `training`: keep 4 lines of the stock `Infanterie-Division` template (name verified in
  `history/units/GER_1936*.txt`), 2 at a time, deploying in Brandenburg (state 64). This
  grows the army from about 30 divisions toward the playbook's 45 (end 1936) and 60 (end
  1937) only as fast as rifles allow. Replace the template with a custom 7 infantry + 2
  artillery INF-20 (generic-playbook section 2) once you build it; the runner cannot
  design templates. Raise `queue` when rifle stock exceeds about 20k.
* `armies`: every division in an army of at most 24, `absorb` folds unassigned
  divisions into the army standing there, generals named from the 1936 officer corps are
  given to armies of 8 or more divisions. The names are the English display names in the
  localisation; if a name is not available in the save the tool refuses and the runner
  moves on.

## production

Start state (verified in `history/units/GER_1936_nsb.txt`): 28 military factories,
17 on lines (10 rifles `infantry_equipment_1`, 2 `support_equipment_1`, 1
`artillery_equipment_1`, 2 `light_tank_chassis_2` Panzer II, 2 `motorized_equipment_1`),
so **11 are idle on day one**. The runner's rebalance step (idle factories go to the first
`production_priority` line that exists) would push all 11 into rifles before any
`production` line is added, so the plan sets `production_min_idle: 12` for the start:
the rebalance stays off and the five `production` entries consume exactly the 11
idle factories: fighters (`small_plane_airframe_1`) 3, CAS (`small_plane_cas_airframe_1`) 2,
a second artillery line 3, a second rifle line 2, trucks 1. Targets after day one:
rifles 12, support 2-3, artillery 4, trucks 3, light tanks 2, fighters 3, CAS 2 (the 1936-37 row of
templates-equipment-production section 7).

**One manual edit is required** once all five lines exist (about a week into the game):
set `production_min_idle` to 3, otherwise factories from focuses and construction pile
up idle until 12 are free. After that, `production_priority`
(`infantry_equipment`, `artillery_equipment`, `support_equipment`) sends each batch of
new factories to the first line that exists, i.e. rifles. When a phase needs another mix
(tanks in 1938-39, trucks for motorised divisions), change the order or use
`hoi4_production_set` by hand; the digest alarm `idle_military_factories` shows the need.

Equipment keys are the variant types the production selection window reports.
`small_plane_cas_airframe_1` is a generated duplicate-archetype key
(`x_plane_airframes.txt`). Naval lines are unsupported by the tool.

## construction

Ordered jobs; the first job with a usable state is served (states with no free slot are
remembered full for 14 days). Idle civilian factories needed first: 4.

| Job | States (id, name) | Reason |
|---|---|---|
| 1. `industrial_complex` x2 | 60 Thueringen, 55 Hessen, 53 Niederbayern, 54 Franken, 68 Lubusz (Ostmark), 52 Oberbayern, 50 Wuerttemberg, 57 Westfalen | Interior states with most free slots (about 37 slots by the history files); away from the French and Polish borders. Target 50+ civilian factories by end 1937 (also the Heraeus condition). |
| 2. `arms_factory` x2, `fallback_any_own_state` | 59 Hannover, 64 Brandenburg, 56 Weser-Ems, 51 Rhineland, 42 Moselland, 978 Baden, 66 Lower Silesia, 67 Upper Silesia | Reached only after job 1's states are full, then spills to any own state. This is the 1938 switch from civilian to military the playbook asks for. |
| 3. `infrastructure` | 66, 67, 5, 61, 62, 63, 68, 50, 53, 56, 58, 60, 763, 909, 978 | Infrastructure uses no building slot, so it is the sink that keeps civilian factories working when everything else is full (the east and Silesia are at level 3). |

States are given as ids so the plan does not depend on which localised name
`hoi4_states` resolves (German players see `GER_STATE_n` variants such as
Niederoesterreich).

Deviate when: war is within 12 months (move job 2 first); a state is occupied or lost
(remove it); `hoi4_state_slots` shows a different free-slot count than the history files
(focus rewards add slots at random: the Four Year Plan and Autobahn give some).

## Corrections to the playbook found while building the plan

* `infantry_weapons1` is already a German start tech here; `support_weapons` is not.
* `GER_autonomy_in_the_kriegsschulen` requires `GER_bribe_senior_officers`.
* `GER_anschluss` needs `GER_reorganize_the_wehrmacht` or Heed von Neurath (the plan uses the
  former).
* The price-control decisions have no focus requirement in `GER.txt`; only
  `GER_ersatz_material` needs Coal Liquefaction.
* Military factories on day one: 28 total, 17 on lines (matches the playbook), 11 idle.

## What could not be verified from the files

* The advisor key `hoi4_politics_choose` expects. The tool reads the character key from
  the game's UI object; the plan uses the character id (`GER_hjalmar_schacht`). The
  characters file also defines `idea_token`s (`hjalmar_schacht`, `joachim_von_ribbentrop`).
  Check with `hoi4_politics_options category=government slot=3` and swap if the options
  list the tokens (the validator accepts both and warns for a bare token).
* Slot numbering for `research_production` and `military_staff` (not used).
* Whether `focus_fallback` and `stop_on_date` behave as described (being added to the runner
  concurrently), and whether the live `hoi4_production_add` accepts the exact keys above
  (it matches the selection window's variant type; a refusal lists the offered keys and the
  line is retried after 14 game days).
* Exact research durations and whether the game accepts ahead-of-time techs from the
  priority list at the moment a slot frees (it does for any researchable tech).
* Start-state free slots are computed from `history/states`; the live game adds
  focus-granted slots.
