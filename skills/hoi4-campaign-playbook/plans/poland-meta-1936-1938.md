# Poland META standing plan, 1936-01-01 to 1938-01-01

Companion to `poland-meta-1936-1938.plan.json`. It uses the same schema as
`germany-fair-1936-1938.plan.json`. Copy the JSON to the `plan.json` that the runner reads
(`native/src/plan_run.rs`, which re-reads it every tick). The runner ignores keys that start with `_`.
`_validator` is read only by `scripts/validate_plan.py`.

```
python3 scripts/validate_plan.py --strict skills/hoi4-campaign-playbook/plans/poland-meta-1936-1938.plan.json
# checked: research=53, focus=30, production=3, construction=3, politics=5, decisions=3; 0 error(s), 0 warning(s)
```

`--strict` turns warnings into errors, so the plan has no warnings to explain.

**Scope.** This window is the build-up for the campaign goal in
[poland-meta](../references/poland-meta.md). The goal: survive, defeat Germany and the USSR, and form
the Polish-Lithuanian Commonwealth and the Intermarium. META allows the strongest intended mechanics
and any exploit a player can do through the normal UI. It still bans the console, save editing,
memory writes, firing events and reloading to undo.

## Active DLC set (rewritten 1 November 2026)

`_validator.dlcs` lists the 10 installed DLCs by their exact `has_dlc` names: Anniversary Pack,
Arms Against Tyranny (AAT), By Blood Alone (BBA), Gotterdammerung, Historical German Portraits, La
Resistance, **No Compromise, No Surrender** (one DLC, with the comma in its name), No Step Back (NSB),
Poland: United and Ready (PUR) and Rocket Launcher Unit Pack. The validator adds the integrated DLCs
from `integrated_dlc/` itself: Together for Victory, Death or Dishonor, Waking the Tiger and Man the
Guns (MTG). The brief counted 11 DLCs, which splits "No Compromise, No Surrender" in two. The
`dlc/` folder has exactly 10 `.dlc` files.

This reverses most of the earlier 3-DLC plan:

| Area | With NSB + BBA + AAT + MTG active |
|---|---|
| Focus tree | `polish_focus` (PUR or NSB). The NSB political roots are visible again: `POL_assemble_the_regency_council`, `POL_complete_april_constitution`, `POL_organize_the_peasants_strike` and `POL_nationalist_constitution`. `POL_restore_the_sejm` is now **hidden** (`allow_branch = NOT NSB`), and so is its democratic tree. |
| Monarchy | Regency -> Fifth of November -> the `NSB_poland_royal_election.*` events -> the Wettin, Habsburg, Hohenzollern, Bermondt or Romanov branches. Royal Sejm, Royal Guards, King of Lithuania and `POL_LIT_union` all work. |
| Start ideas | PUR still adds `uncontrolled_exports`, `POL_april_constitution_1`, `POL_looming_peasants_strike` and both Sanation oppositions (level 1). Agrarian Reform is still mandatory. |
| Start techs | BBA air: `iw_small_airframe`, `basic_small_airframe`, `engines_1`, `aa_lmg`, `aa_cannon_1` and `early_bombs`. NSB armour: `gwtank_chassis`. MTG naval hull techs. No `early_fighter`, `fighter1`, `gwtank` or `early_destroyer`. |
| Air OOB / designs | `POL_1936_air_bba`: one line on `small_plane_airframe_0` "PZL P.11" (an obsolete variant). Designs: "PZL P.24" (`small_plane_airframe_0`) and "PZL P.23" (`small_plane_cas_airframe_1`). There is **no** fighter design on `small_plane_airframe_1`, so draw one in the designer. |
| Land designs | NSB light tanks "7TP dw" and "7TP jw" (`light_tank_chassis_0`). The plan does not produce them in this window. |
| Exile tree | `POL_prepare_for_the_inevitable` (needs MTG) and `POL_expand_polish_intelligence` (LaR + MTG) are available, but only once Poland is losing. |
| Equipment | AAT industrial manufacturers (MIOs) are active. The start infantry line carries `mio:POL_fb_organization`. The runner does not manage MIOs, so the model assigns them. |

## Route chosen (the META opening)

**Economy focuses first, then politics, then the Wettin-Lithuania chain. Intermarium as soon as its gate
opens.**

* **Regency Council** is the strongest political root for this goal. One 5-point focus removes both
  Sanation oppositions, makes the government neutral with elections off, and unlocks two
  top-tier advisors: Wojciechowski (`noble_beurocrat`, +15% PP) and Lubomirski (`staunch_aristocrat`,
  -20% consumer goods). Fifth of November (5) removes `POL_april_constitution_1` (-15% PP, +15% law and
  advisor costs). Sanation (`POL_complete_april_constitution`) needs 6-8 more focuses to clear the same
  maluses. The peasant-strike root and the nationalist root lead away from the endpoint.
* **Wettin (Friedrich Christian)** is the only monarch branch that ends in `POL_LIT_union`, which
  annexes a Lithuanian subject and runs `activate_decision = form_commonwealth`. That is the Commonwealth
  goal. He is the **first** offer (event `NSB_poland_royal_election.1`, 35 days after Fifth of November),
  so this route costs no "keep looking" delay. Option 0 accepts him and runs
  `unlock_national_focus = POL_claiming_lithuania`. That is why `event_options` maps it to 0.
  Habsburg (Czechoslovakia) is the stronger industrial grab, but it is mutually exclusive with
  `POL_claiming_lithuania`. It would push the Commonwealth back to a conquest of Lithuania and the formable decision.
* **Economy before the king.** `POL_central_region_strategy` (+2 civilian), `POL_expansion_of_new_towns`
  (+2 military), `POL_invest_in_the_old_polish_region` (+1 military), `POL_start_central_industrial_region`
  (+1/+1) and `POL_modernize_congressional_factories` (+2/+2) add **11 instant factories** for 45 focus
  points. Every month they come earlier compounds. The monarchy chain only pays off in 1938-39.

## Top-level keys

| Key | Value | Why |
|---|---|---|
| `stop_on_date` | `1938.01.01` | The window. If you extend it, see the tail notes below. |
| `event_options` | `{"NSB_poland_royal_election.1": 0}` | Accepts Friedrich Christian (Wettin). Without this entry, `auto_events` would wait 3 days and then also pick option 0, but the explicit entry protects the route if the defaults change. |
| `alarms` | PP 200, CG 0.40, idle civ 3, idle mil 2 | Poland is small, so 3 idle civilian factories is already a real loss. |
| `doctrines` | `defensive_postures` (infantry track, xp 50), `fire_concentration` (combat_support, xp 100) | Land subdoctrines under the start grand doctrine `grand_battleplan` (`available = always`). xp 50 assumes the -50% from `POL_prepare_for_the_next_war`. Before that focus the tool refuses with `insufficient_experience` and the runner retries 15 days later. **`hoi4_doctrine_adopt` is marked UNVALIDATED in its own tool description.** Check the first adoption by hand. |
| `special_projects` | `{"priority": []}` | Fills any empty facility with the first visible project. Poland has no facility in this window, so this is a no-op until one exists. |
| `air.war_missions` | true | Only acts in war. |

## research_priority (53 technologies)

Start techs with this DLC set (validator, DLC-aware): `infantry_weapons(1)`, `tech_support`,
`tech_engineers`, `tech_recon`, `tech_mountaineers`, `tech_trucks`, `gw_artillery`, `interwar_antiair`,
`fuel_silos`, `synth_oil_experiments`, `basic_train`, `gwtank_chassis`, the BBA set above, and MTG naval
techs. Poland has **3 slots**. The 4th comes from `POL_additional_research_slot1`, about 1937-02-24.

| Positions | Block | Reason |
|---|---|---|
| 1-3 | basic_machine_tools, construction1, electronic_mechanical_engineering | Day-one industry. |
| 4-13 | support_weapons, construction2 (Four Year Plan +100% construction bonus, about 02-05), concentrated_industry (Central Region Strategy +100% industry bonus, about 04-15), improved_machine_tools, interwar_artillery, interwar_antitank, radio, excavation1, concentrated_industry2, mechanical_computing | Poland starts without support_weapons, interwar_artillery or electronics. interwar_antitank unlocks `anti_tank_equipment_1` before its dated line (1936-11-15). |
| 14-26 | infantry_weapons2, support_weapons2, motorised_infantry (needed by the logistics, field hospital and signal companies), basic_fortification_tech, improved_equipment_conversion, excavation2, support companies, improved_radio, computing_machine, fuel_refining, rubber_processing | |
| 27-28 | **engines_2, aircraft_construction** (BBA) | The BBA air path. Poland already has `basic_small_airframe`. Fighter work goes into the designer, not into legacy `fighter1`. |
| 29-53 | land_fort_tech_1, encryption/decryption, construction3, advanced_machine_tools, concentrated_industry3, artillery1, antitank1, antiair1, **aa_hmg** (BBA), improved_infantry_weapons, second-tier support, wartime_train, ... | The tail for 1938-39. Nothing is more than one year ahead of 1938 in the first 45 entries. |

There are no legacy keys (`fighter1`, `early_fighter`, `gwtank`, `basic_light_tank`), and no start techs.

## focus_path (30) and focus_fallback (14)

1 focus point = 7 days. Dates assume nothing is blocked.

| # | Focus | Cost | Done about | Note |
|---|---|---|---|---|
| 1 | POL_the_four_year_plan | 5 | 1936-02-05 | +100% construction tech bonus, +10% building speed for 1460 days. |
| 2 | POL_assemble_the_regency_council | 5 | 03-11 | Removes both oppositions before their first escalation (175 days). Neutral government, elections off. Unlocks Wojciechowski, Lubomirski and Kakowski. |
| 3 | POL_central_region_strategy | 5 | 04-15 | 2 instant civilian factories (Warsaw 10, Kielce 90) and slots in Lublin 92 and Łódź 87. |
| 4 | POL_fulfil_fifth_of_november | 5 | 05-20 | Needs neutrality > 20% and democratic < 50% (start 38/32). Removes the April constitution. The election event arrives about 06-24. |
| 5-6 | POL_fill_the_railways_gaps, POL_agrarian_reform | 10+10 | 07-29, 10-07 | Agrarian Reform is selected on about 07-29, before the 365-day strike mission ends (12-31), and **selecting** it already adds 70 days to the mission. |
| 7-10 | POL_the_between_the_seas_concept, POL_the_mediterranean, POL_coerce_czechoslovakia, POL_austro_hungarian_alliance | 10+5+5+5 | when the gate opens | Gate: `has_army_manpower > 699999`, not a subject, and unaligned or a faction leader. The runner skips them until then. Placed early so they take priority once the gate opens, ahead of the March 1938 Anschluss. |
| 11-12 | POL_expansion_of_new_towns, POL_invest_in_the_old_polish_region | 5+5 | 11-11, 12-16 | +2 military (Poznań 86, Gdynia 807) and +1 military (Kraków 88). |
| 13 | POL_additional_research_slot1 | 10 | 1937-02-24 | 4th slot. No condition while Poland has 3 slots. |
| 14-15 | POL_start_central_industrial_region, POL_modernize_congressional_factories | 10+10 | 05-05, 07-14 | +1/+1 (Kraków), then +2 military and +2 civilian (Warsaw, Kielce, Płock). |
| 16 | POL_national_defence_fund | 10 | 09-22 | -35% consumer goods for 720 days, which covers the 1939 war. |
| 17 | POL_prepare_for_the_next_war | 5 | 10-27 | -50% on one land doctrine (see `doctrines`). Prerequisite of Plan West. |
| 18-23 | POL_claiming_lithuania -> royal_sejm -> institute_royal_guards -> support_monarchism_in_LIT -> king_of_lithuania -> LIT_union | 10+10+10+10+5+10 | 1938-01-05 onward | Claiming is unlocked by the election event. Royal Guards gives 2 divisions at 1.0 experience. `POL_king_of_lithuania` needs Lithuanian monarchist support > 79 (or Lithuania as a subject). That comes from **targeted** decisions the runner cannot take (see decisions). |
| 24 | POL_additional_research_slot2 | 10 | when allowed | Needs > 50 owned factories once Poland has 4 slots. Skipped until then. |
| 25-30 | POL_plan_west, local_western_plans, silesia_fortified_area, the_prusya_line, expand_poznan_forts, hel_fortified_area | | 1938 | Bunker speed +30% for 547 days, instant bunkers, the `POL_defend_in_west_state` decision. |

`focus_fallback`: expand the central industrial region, Katowice mines (+steel, +aluminium), Central
Defence of Poland, Upper Silesia, eastern investment, the military academy chain (academy ->
standardisation -> army modernisation -> AT/artillery), invest in anti-air, air bases, fighter
modernisation, Warsaw railway station.

**Never add:** `POL_restore_the_sejm` (hidden under NSB; the validator does not flag it),
`POL_complete_april_constitution`, `POL_organize_the_peasants_strike`, `POL_nationalist_constitution`
(exclusive with the Regency), `POL_habsburg`, `POL_hohenzollern`, `POL_cossack_king` or `POL_romanov`
(exclusive with `POL_claiming_lithuania`; the validator does flag these), `POL_plan_east` (blocked until
`POL_complete_plan_west`), or `POL_complete_plan_west` (it ends the bunker bonus).
`POL_clamp_down_on_danzig` vs `POL_develop_polish_ship_building` is the model's call. The Danzig line
(`POL_integrate_gdansk_industries`) is the only peacetime way to drop `uncontrolled_exports`.

## politics

The runner skips a pick until political power reaches the pick's `cost`.

| Pick | Cost | Why |
|---|---|---|
| government 2 `partial_economic_mobilisation` | 173 (150 +15% April constitution) | Civilian Economy: 35% consumer goods and **-30%** civilian and military factory construction speed. Partial: 25% consumer goods, +10% arms speed, no construction penalty. Needs war support > 0.25. |
| government 3 `POL_stanislaw_wojciechowski` | 173 | `noble_beurocrat`: +15% PP, +5% stability. Available after the Regency (`allowed` needs NSB or PUR). |
| government 4 `POL_eugeniusz_kwiatkowiski` | 150 | `captain_of_industry`: +10% civilian factory, infrastructure and rail construction. |
| government 5 `POL_zdzislaw_lubomirski` | 150 | `staunch_aristocrat`: -20% consumer goods. Needs the Regency and a neutral or fascist government. |
| government 0 `limited_conscription` | 150 | Manpower for the 700k Intermarium gate. War support > 0.1. |

The slot numbers follow the earlier Poland and Germany plans: 0 manpower, 2 economy, 3-5 political
advisors. Not in the plan: Beck (+5% PP; no free political slot), Haller (+10% arms construction; swap
him in for Kwiatkowski when construction turns to military factories in 1938), and military staff
(Sosnkowski `army_chief_defensive_2`, theorist Łupaszko). PP is short through 1937.

## decisions

`POL_diplomatic_mission_to_CZE`, `_AUS`, `_HUN`: 10 PP, 35 days, -10% PP gain, one at a time. Each
appears after the focus before the invitation. The runner's `hoi4_decision_take` has **no target
argument**, so the targeted monarchy decisions are **manual**:
`POL_propagandise_monarchist_sentiment` (50 PP, +10), `POL_double_propagandise_monarchist_sentiment`
(100 PP, +15, at 25+), `POL_sway_lithuanian_generals` (25 PP + 25 CP, +10) and `POL_invest_in_lithuania`
(3 civilian factories for 30 days, +10). Target **LIT**. They become visible after
`POL_support_monarchism_in_LIT` (`POL_is_valid_to_boost_monarchy`). Take them until support > 79, then
`POL_king_of_lithuania`.

## training and armies

`training`: `Dywizja Piechoty` (9 infantry + recon, `history/units/POL_1936.txt`). 6 in the queue,
2 at a time, deployed in Warsaw (10). The start army is about 320k. The 700k gate is about 40 more
divisions in the field, so the Intermarium chain probably opens only in 1938-39. META candidate
(unverified): deploy under-equipped divisions early to pass `has_army_manpower`, then check the tooltip.
`armies` is unchanged: at most 24 per army, generals Rydz-Śmigły, Sikorski, Kutrzeba, Abraham, Anders,
Sosnkowski.

## production

The start lines (`POL_1936` + `POL_1936_air_bba`): 11 military factories. `infantry_equipment_1` 2,
`support_equipment_1` 1, `small_plane_airframe_0` "PZL P.11" 1. **7 are idle.** `production_min_idle: 8`
keeps the rebalance off on the first tick, so the plan's lines get the idle factories first:
`artillery_equipment_1` 4 and `anti_air_equipment_1` 2 (1 idle left). `anti_tank_equipment_1` 2 is dated
`from 1936.11.15`, after `POL_expansion_of_new_towns` adds 2 military factories and interwar_antitank is
researched. **Manual edits:** once those three lines exist, set `production_min_idle` to 2 so new
factories go to rifles. Switch the P.11 line to the "PZL P.24" design, or to a new
`small_plane_airframe_1` fighter you design. Assign MIOs (AAT) by hand.

## construction

Unchanged: civilian factories in the interior Central Industrial Region (90, 88, 92, 87, 89, 91, 93),
then military factories in the same states, then infrastructure in the east. From early 1938, put
`arms_factory` first.

## Pre-war checklist (by 1938-01)

* Partial Mobilisation, Wojciechowski, Kwiatkowski, Lubomirski and Limited Conscription. No
  oppositions, no April constitution, the strike cancelled, Friedrich Christian king, the National
  Defence Fund running, 4 research slots.
* About 11 focus factories plus construction. Check `hoi4_economy`.
* Monarchy decisions on Lithuania started as soon as `POL_support_monarchism_in_LIT` is done.
* Intermarium: invite Czechoslovakia, then Austria and Hungary, before the Anschluss (about 1938-03).
  Record Austria's faction monthly. Never accept `germany.86`.

## Not verifiable from the files

* Starting political power, and so the exact dates of the politics picks.
* Whether under-equipped divisions count toward `has_army_manpower`.
* AI acceptance of Intermarium invitations and of the Lithuanian crown (`NSB_poland_monarchist_events.5`).
* Whether `hoi4_doctrine_adopt` works live (the tool says it is unvalidated).
* Whether a bonus earned while a tech is already being researched applies to that tech. The order assumes
  it applies to the next tech started.
