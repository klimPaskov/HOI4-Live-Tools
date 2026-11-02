# Germany FAIR standing plan, 1936-01-01 to 1938-01-01

Companion to `germany-fair-1936-1938.plan.json` (JSON has no comments, so every
section is explained here). Copy the JSON to the `plan.json` the runner reads
(see `native/src/plan_run.rs`; the file is re-read every tick). Keys that start
with `_` are ignored by the runner; `_validator` only feeds
`scripts/validate_plan.py`.

```
python3 scripts/validate_plan.py --strict skills/hoi4-campaign-playbook/plans/germany-fair-1936-1938.plan.json
```

Every key below was checked against HOI4 1.19.4.1 in the installed game files
(technologies and their DLC folders, focus tree `german_focus`, equipment, start
designs, buildings, history states, ideas, characters, decisions, special
projects). The validator output is clean (0 errors, 0 warnings, also with
`--strict`). What the files cannot prove is listed at the end.

## Active DLC set (1 November 2026)

`_validator.dlcs` lists the ten active DLCs by their game-log names: Anniversary
Pack, Arms Against Tyranny, By Blood Alone, Gotterdammerung, Historical German
Portraits, La Resistance, No Compromise, No Surrender (one DLC; the comma is part
of its name in `dlc049.dlc`), No Step Back, Poland: United and Ready, Rocket
Launcher Unit Pack. Together for Victory, Death or Dishonor, Waking the Tiger and
Man the Guns ship in `integrated_dlc/` and are always active; the validator adds
them itself. Earlier versions of this plan assumed only three DLCs (no NSB, no
BBA); what changed because of the larger set:

| DLC | Effect on Germany's files | Plan consequence |
|---|---|---|
| No Step Back | `set_oob = "GER_1936_nsb"`; start techs `gwtank_chassis`, `basic_light_tank_chassis`, `improved_light_tank_chassis`, `basic_heavy_tank_chassis`, `engine_tech_1`; the `armour_folder` (legacy `basic_medium_tank` etc.) is hidden, `nsb_armour_folder` is shown (`common/technology_tags/00_technology.txt`); tank designs are chassis variants (`light_tank_chassis_2` "Panzer II", StuG III on `medium_tank_destroyer_chassis_1`) | Medium armour is `basic_medium_tank_chassis` + `armor_tech_*` / `engine_tech_*`; the medium line is `medium_tank_chassis_1` |
| By Blood Alone | `set_air_oob = "GER_1936_air_bba"`; start techs `basic_small_airframe`, `basic_medium_airframe`, `iw_large_airframe`, `engines_1/2`, `early_bombs`, `aircraft_construction`, `aa_lmg`, `aa_cannon_1`, `air_torpedoe_1`; `air_techs_folder` (fighter1/2, CAS1/2, tactical_bomber1/2) is hidden; start designs Bf 109 D (`small_plane_airframe_1`), Ju 87 (`small_plane_cas_airframe_1`), He 111 and Do 17 (`medium_plane_airframe_1`), Do 22 (`small_plane_naval_bomber_airframe_1`); starting production adds one factory each on Bf 109 D, Ju 87, He 111 | Air research is airframes, engines and modules; plane lines use airframe keys |
| La Resistance | `armored_car1` start tech; `photo_reconnaisance` (with BBA); `basic_encryption` / `basic_decryption` have `allow_branch = { NOT = { has_dlc = "La Resistance" } }` (cryptology moves to the intelligence agency) | Encryption techs removed from research |
| Arms Against Tyranny | `tech_special_forces` hidden (special forces doctrine folder instead); several focus rewards switch to MIO/doctrine variants | No research change in this window |
| Gotterdammerung | Anschluss needs a deployed-army strength ratio over Austria instead of 500k manpower; special projects; `radio_detection` and `cavity_magnatron` are only reachable through `sp_air_radar` (their `allow` needs the completed project) | Radar techs removed from research, `sp_air_radar` first in `special_projects.priority` |

The tree that loads for GER with this set is still `german_focus`
(`common/national_focus/germany.txt`); every `focus_path` and `focus_fallback` key
is in it, none sits behind a closed `allow_branch` (the Gotterdammerung-only
branches are the opposition/monarchist/HRE paths, which the plan never uses), so the
focus path did not change.

Evidence sources: [economy-build-orders](../references/economy-build-orders.md),
[research-doctrines-projects](../references/research-doctrines-projects.md),
[templates-equipment-production](../references/templates-equipment-production.md),
[supply-logistics-deployment](../references/supply-logistics-deployment.md),
[politics-diplomacy-espionage](../references/politics-diplomacy-espionage.md),
[germany-fair](../references/germany-fair.md) (checkpoint table). Where the
playbook and the installed files disagree, the files won (see "Corrections").

## Runner behaviour that shapes the plan

* Each tick does one action, in this order: events, research, focus, politics,
  decisions, training, military-factory rebalance, `production` lines (dated by
  `from`), construction, special projects / air missions in war, armies. A refused
  action is skipped for 1-3 game days (production lines 14).
* Research and focus lists are priority lists: the runner takes the first
  not-completed key **the game accepts**. It only tries the first 3 unresearched
  research keys (8 focus keys) per tick, so a locked head costs days. The list is
  therefore ordered so that prerequisites always come first.
* `stop_on_date` ends the plan on 1938.01.01. Extend it for the Sudetenland/Czech
  phase; the expansion chain and the 1938-39 production entries are already in the
  plan.

## Top-level keys

| Key | Value | Why / when to deviate |
|---|---|---|
| `active`, `stop_on_failure` | true / false | A failed action must not freeze the routine. Set `stop_on_failure` true only while debugging a new build. |
| `stop_on_date` | `"1938.01.01"` | The plan's window. Move it only together with the focus tail and the construction order. |
| `auto_events`, `wait_events_days` | true / 3 | News and single-option events are answered with option 0. Multi-option events wait 3 game days for the model to look; then option 0 is used. |
| `event_options` | `germany_mefo_bills.1: 0` | Option 0 is "Rearmament has always been our goal" (Economy of Conquest). Only relevant at the first war; choose 1 only for a deliberate peace economy. Oster Conspiracy, Czech/Munich and Anschluss events are left to the model on purpose. |
| `alarms` | PP 200, consumer goods 0.45, idle civ 4, idle mil 2, unassigned divisions 2 | Digest thresholds only. Lower the PP alarm if you want laws bought faster. |

## research_priority (66 technologies)

Germany starts with 4 slots (5th from Heraeus). At roughly 100 days per cost
point, two years finish about 25-30 techs, so entries beyond ~35 are the tail
for 1938-39; the runner's catalog fallback (`research_fallback: true`) fills any
slot the list cannot. Start techs are taken from `history/countries/GER - Germany.txt`
(1936 block, evaluated with the active DLCs): the game already has infantry_weapons1,
tech_engineers, tech_recon, tech_mountaineers, tech_support, tech_trucks,
motorised_infantry, gw_artillery, interwar_antiair, basic_train, fuel_silos,
fuel_refining, synth_oil_experiments, the NSB chassis line up to
improved_light_tank_chassis plus basic_heavy_tank_chassis and engine_tech_1, the BBA
air techs listed above, and armored_car1.

Every key was also checked against its technology folder and `allow` /
`allow_branch` with the active DLCs (the validator only checks that a key exists and
its prerequisites, not folder availability, so this was done separately). Keys
removed for that reason: `basic_medium_tank` (legacy armour folder),
`fighter2`, `CAS2`, `tactical_bomber2` (legacy air folder), `basic_encryption`,
`basic_decryption` (La Resistance), `radio_detection`, `cavity_magnatron` (special
project).

| Positions | Block | Reason |
|---|---|---|
| 1-4 | basic_machine_tools, construction1, electronic_mechanical_engineering, interwar_artillery | Playbook day-one four. Industry first. The Four Year Plan tech bonus (industry, 1 use) lands around 1936-02-05: it is spent on the next industry tech started, improved_machine_tools or construction2. |
| 5-18 | radio, improved_machine_tools, construction2, interwar_antitank, logistics and maintenance companies, concentrated_industry (+2), armor_tech_1, improved_equipment_conversion, excavation1, infantry_weapons2, support_weapons, support_weapons2 | 1937 economy and support. concentrated_industry (+15% factory output) over dispersed because Germany builds in few big states. infantry_weapons2 is a year ahead of date at this point and is accepted deliberately (every division uses it). `support_weapons` is not a German start tech; support_weapons2 needs it. armor_tech_1 (NSB armour module, 1936, cost 1) improves the Panzer II design. |
| 19-34 | fuel_refining2, signal/field-hospital companies, mechanical_computing, oil/rubber processing, excavation2, improved_radio, basic_fortification_tech, engine_tech_2, armor_tech_2, basic_medium_tank_chassis, land_fort_tech_1, computing_machine, range_improvements, armor_piercing_bombs | Resource independence, signals, the NSB engine/armour modules and the medium chassis (1938, needs only gwtank_chassis) so a Panzer III design can be made before the 1938-06 tank line; two cheap BBA modules (range for fighters over France/Britain, armour-piercing bombs for the Ju 87 successor). |
| 35-45 | artillery1, antiair1, antitank1, improved_infantry_weapons, advanced_machine_tools, construction3, concentrated_industry3, advanced_equipment_conversion, excavation3, tech_engineers2, tech_recon2 | 1939 on-date items; by 1938 they are 0.5-1 year ahead. |
| 46-66 | second-tier support companies, wartime_train, aa_hmg, military police, third oil/rubber/fuel tiers, improved_small_airframe, engines_3, improved_medium_airframe (BBA, 1940), mechanised_infantry, improved_infantry_weapons_2, artillery2, support_weapons3, antitank2, antiair2, armored_train, improved_medium_tank_chassis (NSB, 1940) | Tail so the list never runs dry. 1940 items are flagged by the validator only beyond position 45 and only if more than two years ahead. |

Deviate when: Chief of Army changes or a war starts (move artillery1/antitank1 up);
you decide on dispersed industry (replace the concentrated_industry chain, they are
exclusive); a focus bonus (see below) makes a specific tech nearly free; the
digest shows an idle slot (the fallback already covers this, but the order then
is the game's, not ours). Doctrines are XP, not slots, and are not in this list.

Focus tech bonuses worth timing: Four Year Plan (industry), Heraeus (electronics),
Advanced Panzer Research (medium armor, only in the fallback list here; it lands on
`basic_medium_tank_chassis` if that is still open).

Radar: with Gotterdammerung `radio_detection` is granted by the special project
`sp_air_radar` (available once `electronic_mechanical_engineering` is known, position 3),
and `cavity_magnatron` needs the completed project. See special_projects below.

## focus_path (26 focuses) and focus_fallback

All keys exist in `common/national_focus/germany.txt`; each prerequisite block has a
member earlier in the path; no two exclusive focuses appear together. Costs are in
focus points (7 days each).

The runner takes the first not-completed key the game accepts and skips a blocked key for
`skip_hours.focus_blocked`, so the expansion chain sits early in the list: each step is
taken the moment its gate opens and is skipped (not waited for) until then; the economy
and military focuses fill the slot meanwhile.

| # | Focus | Cost | Note |
|---|---|---|---|
| 1 | GER_the_four_year_plan | 5 | Exclusive with Prioritize Economic Growth (never listed). |
| 2 | GER_remilitarize_the_rhineland | 5 | Needs fascism and control of states 42, 51, 978. Exclusive with the Oppose Hitler roots (`GER_oppose_hitler_ww` with Gotterdammerung; never listed). |
| 3 | GER_reorganize_the_wehrmacht | 5 | Exclusive with Heed von Neurath (not listed); prerequisite of Anschluss and Molotov-Ribbentrop. |
| 4 | GER_anschluss | 10 | With Gotterdammerung active the requirement is `fighting_army_strength_ratio` over Austria above `global.deployed_manpower_ratio_needed_for_anschluss` (the 500k manpower / 50k rifles branch applies only without Gotterdammerung); no war, not a subject. Blocked and retried until then. |
| 5-7 | GER_demand_sudetenland, GER_first_vienna_award, GER_fate_of_czechoslovakia | 10, 5, 10 | Sudetenland needs 725k army manpower and a completed Anschluss (it can lead to war with CZE/FRA: the Munich events are left to the model), Vienna Award a friendly neutral or fascist Hungary and states 69/972, Fate of Czechoslovakia 750k and Prague (9) controlled. In practice after the 1938.01.01 window. |
| 8-9 | GER_reassert_eastern_claims, GER_danzig_or_war | 5, 10 | Danzig or War needs 950k army manpower and gives the Polish war goal; FAIR still forbids the declaration before July 1939 (see references/germany-fair.md). |
| 10 | GER_molotov_ribbentrop_pact | 5 | 725k manpower plus the Czech conditions, else 850k; exclusive with the Anti-Soviet Pact (not listed). |
| 11-12 | GER_accelerate_the_rearmament_program, GER_establish_production_targets | 5+5 | +2 and +1 arms factories. |
| 13-14 | GER_construct_the_reichsautobahn, GER_kdf_wagen_factories | 5+5 | Unlocks Heraeus. |
| 15-16 | GER_develop_modern_maneuver_warfare, GER_fortify_the_vaterland | 5+5 | Fortify unlocks the Westwall decision. |
| 17 | GER_expanding_the_luftwaffe | 5 | |
| 18 | GER_concentrated_armament_program | 5 | |
| 19 | GER_develop_heraeus_facilities | 10 | Only available with more than 49 civilian factories; refused until then and retried. 5th research slot. |
| 20-26 | aeronautical_research_institute, form_the_jagdwaffe, adopt_new_panzer_doctrine, panzer_troops_school, bribe_senior_officers, autonomy_in_the_kriegsschulen, expand_the_truppenschulen | 5 each | Fillers with real value (arms factory, air/armor bonuses, manpower, support tech). `bribe_senior_officers` is required by autonomy_in_the_kriegsschulen (the playbook list omitted it). |

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
fillers run); you prefer Heed von Neurath (war against the USSR first): swap focus 3 for
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

Start state with this DLC set (verified in `history/units/GER_1936_nsb.txt` and
`GER_1936_air_bba.txt`, factories from `history/states`): 28 military factories,
20 on lines: 10 rifles `infantry_equipment_1`, 2 `support_equipment_1`, 1
`artillery_equipment_1`, 2 Panzer II `light_tank_chassis_2`, 2 `motorized_equipment_1`,
1 Bf 109 D `small_plane_airframe_1`, 1 Ju 87 `small_plane_cas_airframe_1`, 1 He 111
`medium_plane_airframe_1`. **8 are idle on day one.**

How the runner treats lines (`native/src/plan_run.rs`, steps 6b and 7):

* `production` entries are added once, and since cf581aa a key that already has a line
  in the game counts as done. Every 1936 key Germany would want (rifles, artillery,
  trucks, fighters, CAS) already has a start line, so a `production` entry for it is
  never added. The old plan's day-one entries (second artillery/rifle/truck lines,
  `fighter_equipment_1`, `CAS_equipment_1`) therefore did nothing useful any more and
  the legacy plane keys do not exist with BBA. `production` now only holds keys
  that do not exist at the start, each with a `from` date.
* The rebalance (step 6b) runs before step 7: when at least `production_min_idle`
  factories are on no line, all of them go to the first `production_priority` prefix
  that has a line. With `production_min_idle: 2` and
  `infantry_equipment, artillery_equipment, support_equipment`, the 8 idle factories
  of day one and every later batch of new factories go to rifles.
* A dated line is added at the bottom of the production list. If no factory is free it
  requests factories it does not get yet; the game fills lines top-down, so it
  effectively takes the next new factories (the rebalance sees fewer than 2 idle
  meanwhile).

**One manual step on day one (recommended, before the runner's first tick)**: the
playbook's 1936-37 mix (rifles 12, artillery 4, fighters 3, CAS 2) cannot be reached by
the runner, because it cannot add a second line of an existing key or split idle
factories. Use `hoi4_production_set` once: Bf 109 D line 1 -> 3, Ju 87 line 1 -> 2,
artillery 1 -> 4 (6 of the 8 idle); the rebalance then gives the last 2 to rifles
(10 -> 12). Without the step all 8 go to rifles, which is safe but leaves the Luftwaffe
at one fighter factory.

| Entry | Factories | From | Producible when |
|---|---|---|---|
| `anti_tank_equipment_1` | 2 | 1936.10.01 | `interwar_antitank` (position 8) is researched. Legacy equipment, no design needed. |
| `anti_air_equipment_1` | 1 | 1937.06.01 | Already (interwar_antiair is a start tech). AA for the Ruhr and later divisions. |
| `medium_tank_chassis_1` | 6 | 1938.06.01 | `basic_medium_tank_chassis` (position 30) **and a Panzer III design**. Germany owns no `medium_tank_chassis_1` design in 1936 (the history file creates "Panzer III Ausf. F" only in the 1939.1.1 block), and `hoi4_production_add` refuses equipment with no producible design. Create one with `hoi4_design_create` before June 1938; until then the line is refused and retried every 14 game days. |
| `small_plane_airframe_2` | 6 | 1939.06.01 | `improved_small_airframe` (1940, position 56) **and a new fighter design** (Bf 109 E is only created by the 1939 bookmark). |
| `small_plane_cas_airframe_2` | 4 | 1939.09.01 | Same airframe tech **and a new CAS design** (the key is BBA's duplicate archetype, mapped to `CAS_equipment_2` in `x_plane_airframes.txt`). |

The 1938-39 entries are outside the 1938.01.01 window and only matter when the plan is
extended. Not listed but available without a design: Germany owns the StuG III design
(`medium_tank_destroyer_chassis_1`, created in the 1936 NSB block), producible once
`basic_medium_tank_chassis` is known; `scripts/validate_plan.py` does not expand the
numbered duplicate-archetype tank keys and rejects that key, so add it by hand with
`hoi4_production_add` if wanted.

When a phase needs another mix (tanks in 1938-39, trucks for motorised divisions), change
`production_priority` or use `hoi4_production_set` by hand; the digest alarm
`idle_military_factories` shows the need. Naval lines are not in the plan.

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

## special_projects and air

`special_projects.priority`: `sp_air_radar` (air; needs electronic_mechanical_engineering;
grants `radio_detection` and radar stations, opens `cavity_magnatron`),
`sp_land_military_engineering_vehicles` and `sp_land_flamethrower_tank` (land; need
tech_engineers, a start tech; the flamethrower tank is allowed with No Step Back). The runner
only acts when a facility is empty and a matching idle scientist exists; Germany has to
build a facility first (not automated). Everything else falls back to the first visible
project of that specialisation.

`air.war_missions: true`: in war, standing-by wings fly over the first own border state
with an enemy. The runner chooses the mission from the wing's `aircraft_type`: a type
containing `fighter` flies air superiority, one containing `cas` flies close air support.
With BBA the types are airframe keys: Ju 87 wings (`small_plane_cas_airframe_1`) match
`cas`, but Bf 109 wings (`small_plane_airframe_1`) contain neither word and are **skipped**,
so fighter missions must be set by the model until the runner maps `small_plane_airframe*`
to air superiority. Irrelevant before the first war (1939).

## Corrections to the playbook found while building the plan

* `infantry_weapons1` is already a German start tech here; `support_weapons` is not.
* `GER_autonomy_in_the_kriegsschulen` requires `GER_bribe_senior_officers`.
* `GER_anschluss` needs `GER_reorganize_the_wehrmacht` or Heed von Neurath (the plan uses the
  former).
* The price-control decisions have no focus requirement in `GER.txt`; only
  `GER_ersatz_material` needs Coal Liquefaction.
* Military factories on day one: 28 total, 17 on lines (matches the playbook), 11 idle.

## What could not be verified from the files

* Whether `hoi4_production_add` lists a chassis or airframe key once the tech is known but
  before a design exists (the plan assumes it does not; the refusal is harmless).
* The day-one idle count (8) is from the history files; focus and MIO effects are not
  simulated, read `hoi4_economy` before the manual production step.
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

## 1.19.4 gate notes (1 November 2026)

The first 1.19.4 gate ran without By Blood Alone: planes were the legacy types
(`fighter_equipment_1`, `CAS_equipment_1`) and airframe keys were refused. The game now runs
with By Blood Alone and No Step Back, so the opposite holds: the legacy plane and tank keys
and techs are hidden, and this plan uses the airframe/chassis keys above.
