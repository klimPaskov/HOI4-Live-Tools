# Poland META standing plan, 1936-01-01 to 1938-01-01

Companion to `poland-meta-1936-1938.plan.json`. It has the same schema as
`germany-fair-1936-1938.plan.json`. Copy the JSON to the `plan.json` the runner reads
(`native/src/plan_run.rs`, re-read every tick). Keys that start with `_` are ignored by
the runner. `_validator` only feeds `scripts/validate_plan.py`.

```
python3 scripts/validate_plan.py skills/hoi4-campaign-playbook/plans/poland-meta-1936-1938.plan.json --strict
# checked: research=55, focus=30, production=4, construction=3, politics=5, decisions=3; 0 error(s), 0 warning(s)
```

**Scope.** This window is the build-up for the campaign goal in
[poland-meta](../references/poland-meta.md): survive, defeat Germany and the USSR, and
form Intermarium and the Commonwealth. META means the strongest intended mechanics plus
exploits a player can do through the normal UI. It still bans the console, save editing,
memory writes, firing events and reloading to undo.

**Game build checked.** HOI4 1.19.4.1 (`launcher-settings.json`), vanilla. `dlc_load.json` has no
disabled DLC, so every installed DLC is active. That includes No Step Back and Poland: United and
Ready (`dlc002`, `dlc034`), By Blood Alone, Man the Guns, La Resistance, Arms Against Tyranny
and No Compromise, No Surrender. The tree is `polish_focus` (`common/national_focus/poland.txt`).
It loads for POL with either "Poland: United and Ready" or "No Step Back". Without both DLCs Poland
uses the generic tree and this plan does not apply. Every key in the plan comes from the installed
files: technologies, `polish_focus`, equipment, buildings, `history/states`, ideas,
`common/characters/POL.txt`, `common/decisions/POL.txt` and `events/NSB_Poland.txt`.

## Top-level keys

| Key | Value | Why |
|---|---|---|
| `stop_on_date` | `1938.01.01` | The plan's window. If you extend it, extend the focus tail and the construction order too (switch construction to military factories). |
| `event_options` | `NSB_poland_royal_election.1: 0` | Option 0 (`.1.a`) accepts Friedrich Christian (Wettin). It promotes him and runs `unlock_national_focus = POL_claiming_lithuania`, which completes the Lithuania/Commonwealth branch for free. Option 1 waits 35 days for the Habsburg offer. No other event is automated. The Danzig crisis (`germany.86`, 1939) is for the model to decide. |
| `alarms` | PP 200, CG 0.40, idle civ 3, idle mil 2 | Poland has few factories, so 3 idle civilian factories is already a real loss. |

## research_priority (55 technologies)

The start techs come from `history/countries/POL - Poland.txt`:
`infantry_weapons1`, `tech_support`, `tech_engineers`, `tech_recon`, `tech_mountaineers`,
`tech_trucks`, `gw_artillery`, `interwar_antiair`, `fuel_silos`, `synth_oil_experiments`,
`basic_train` and `gwtank_chassis`. With By Blood Alone it also has `iw_small_airframe`,
`basic_small_airframe`, `engines_1`, `early_bombs`, `aa_lmg` and `aa_cannon_1`. Poland starts with
**3 slots**. The 4th comes from `POL_additional_research_slot1` (about January 1937).

| Positions | Block | Reason |
|---|---|---|
| 1-3 | basic_machine_tools, construction1, electronic_mechanical_engineering | Day-one industry. The Four Year Plan gives a +100% construction bonus around 1936-02-05. It is kept for the next construction tech, construction2 (position 5). |
| 4-10 | support_weapons, construction2, radio, improved_machine_tools, interwar_artillery, concentrated_industry, interwar_antitank | Poland lacks support_weapons and interwar_artillery (Germany has both). interwar_antitank unlocks `anti_tank_equipment_1`, the anti-armour answer for 1939. Concentrated beats dispersed for output. Dispersed is exclusive with it, so never add it. |
| 11-25 | excavation1, mechanical_computing, concentrated_industry2, basic_fortification_tech, infantry_weapons2, support_weapons2, motorised_infantry, logistics/field-hospital/signal companies, improved_equipment_conversion, excavation2, improved_radio, computing_machine | Fortification tech comes before the 1938 bunker programme. motorised_infantry is a prerequisite of the three support companies (the validator caught this). |
| 26-55 | land_fort_tech_1, engines_2, aircraft_construction, basic_encryption/decryption, fuel_refining, rubber_processing, construction3, advanced_machine_tools, concentrated_industry3, artillery1, antitank1, antiair1, improved_infantry_weapons, second-tier support, aa_hmg, ... | Tail for 1938-39, so the slots never fall back to the catalogue. |

`POL_standardisation_of_equipment` (+100% infantry weapons, once) and
`POL_army_modernisation` (+100% infantry weapons, -50% land doctrine) come late in the window.
If you take them earlier, start infantry_weapons2 or improved_infantry_weapons right after them.

## focus_path (30 focuses) and focus_fallback

One focus point is 7 days. The dates assume the Intermarium gate is not met in 1936.

| # | Focus | Cost | Done about | Note |
|---|---|---|---|---|
| 1 | POL_the_four_year_plan | 5 | 1936-02-05 | Construction tech bonus, +10% building speed for 1460 days. |
| 2 | POL_assemble_the_regency_council | 5 | 03-11 | Removes both Sanation oppositions and installs the monarchy (Regency). It is exclusive with the April-constitution, Sejm-restoration, peasant-strike and nationalist roots. |
| 3 | POL_fulfil_fifth_of_november | 5 | 04-15 | Removes `POL_april_constitution_1` (-15% PP, +15% law and advisor costs). Starts the election (event `.1` 35 days later). Needs neutrality > 20% and democratic < 50%. |
| 4 | POL_claiming_lithuania | (0) | ~05-20 | Never started by the runner: its `available` is `always = no`. The election event completes it. It is listed only because `POL_restoration_of_the_royal_sejm` needs it earlier in the path. |
| 5 | POL_central_region_strategy | 5 | 05-20 | Instant civilian and military factories plus extra slots in Warsaw, Kielce, Lublin and Łódź. |
| 6-7 | POL_fill_the_railways_gaps, POL_agrarian_reform | 10+10 | 07-29, 10-07 | Agrarian Reform must finish before 1936-12-31. Otherwise the Looming Peasants' Strike (365-day mission) becomes the Great Peasants' Strike. That leaves about 85 days of margin. |
| 8-11 | POL_the_between_the_seas_concept, POL_the_mediterranean, POL_coerce_czechoslovakia, POL_austro_hungarian_alliance | 10+5+5+5 | when the gate opens | Gate: `has_army_manpower > 699999` and not a subject. Until then the runner skips the focus every 6 game hours and goes on. Once it is open, the chain runs ahead of everything below, so Austria gets invited (event poland.16) before Germany's March 1938 Anschluss. `GER_anschluss` is bypassed if AUS is in a faction that does not include GER. |
| 12 | POL_expansion_of_new_towns | 5 | 11-11 | Arms factories (Poznań, Gdynia, Kraków). Unlocks slot2 and the Katowice mines. |
| 13 | POL_additional_research_slot1 | 10 | 1937-01-20 | 4th slot. Free while Poland has 3 slots. |
| 14 | POL_national_defence_fund | 10 | 03-31 | **-35% consumer goods for 720 days.** The best economy focus in the tree. poland-meta.md does not mention it. |
| 15 | POL_start_central_industrial_region | 10 | 06-09 | Civilian and military factories in Kraków, Stanisławów and Lwów. |
| 16-17 | POL_prepare_for_the_next_war, POL_plan_west | 5+10 | 07-14, 09-22 | -50% on one land doctrine. Bunker build speed +30% (`POL_plan_modifier`) for 547 days in states that border Germany, so it lasts until about 1939-03. |
| 18-20 | POL_invest_in_the_old_polish_region, POL_modernize_congressional_factories, POL_additional_research_slot2 | 5+10+10 | 1937-10 to 1938-03 | Slot2 needs **more than 50 owned factories** once Poland has 4 slots. It is refused until then. |
| 21-30 | new_military_academy, standardisation_of_equipment, restoration_of_the_royal_sejm, central_defence_of_poland (+25% bunker speed, 720 days), local_western_plans, silesia_fortified_area, the_prusya_line, expand_poznan_forts, hel_fortified_area, army_modernisation | | 1938 | Tail for after the window: instant bunkers in 88/762, 97/98/807, 86/87 and Hel, plus the `POL_defend_in_west_state` decision. |

`focus_fallback`: Katowice mines (+24 steel, +12 aluminium), Upper Silesia, expand/eastern
industrial region, anti-air bonus, AT and artillery bonuses, air base and fighter bonus, then
royal guards, support monarchism in Lithuania, Warsaw railway station. None is exclusive with the path.

**Never add:** `POL_complete_april_constitution`, `POL_restore_the_sejm`,
`POL_organize_the_peasants_strike`, `POL_nationalist_constitution` (exclusive roots);
`POL_habsburg`, `POL_hohenzollern`, `POL_cossack_king`, `POL_romanov` (exclusive with the Wettin line);
`POL_plan_east` (blocked until `POL_complete_plan_west`); `POL_complete_plan_west` (cost 0, but it
removes the bunker modifier and makes every remaining Plan West focus unavailable). Also leave out
`POL_clamp_down_on_danzig` and `POL_develop_polish_ship_building`, which are exclusive with each
other. Clamp Down gives 75 PP and 50 command power, but it starts resistance 40 in Danzig (2
military factories, 3 dockyards). Its follow-up, `POL_integrate_gdansk_industries`, is the only
cheap way to replace `uncontrolled_exports` with `export_focus`. That is a model decision, not routine.

## politics

| Pick | Cost | Why |
|---|---|---|
| government 2 `partial_economic_mobilisation` | 150 (+15% until Fifth of November) | Civilian Economy gives **-30% civilian and military factory construction speed**. Partial only needs war support > 0.25 (Poland starts at the 0.5 default). It has no construction penalty and gives +10% arms speed. Low Economic Mobilisation is worse (-10% construction). |
| government 3 `POL_stanislaw_wojciechowski` | advisor | `noble_beurocrat`: +15% PP, +5% stability. Available after the Regency Council. |
| government 4 `POL_eugeniusz_kwiatkowiski` | advisor | `captain_of_industry`: +10% civilian factory, infrastructure and railway construction speed. No conditions. |
| government 0 `limited_conscription` | 150 | War support > 0.1. Gives the manpower for the 700k Intermarium gate. Extensive conscription needs fascism or communism. |
| government 5 `POL_zdzislaw_lubomirski` | advisor | `staunch_aristocrat`: consumer goods factor -20%. Available after the Regency, while neutral or fascist. |

Advisor keys are character ids. They are also the `idea_token`s, so `hoi4_politics_options`
should list them. Army chief `POL_kazimierz_sosnkowski` (defensive 2) and the theorist/high
command picks are left to the model, because `military_staff` slots have no verified numbering.

## decisions

`POL_diplomatic_mission_to_CZE`, `_AUS`, `_HUN` (10 PP each, 35 days, -10% PP gain while
running). Each one shows up after the focus before the invitation (Mediterranean -> CZE, Coerce
Czechoslovakia -> AUS and HUN). Each adds an AI alliance strategy (+20) toward POL in the target.
Only one runs at a time (`POL_completing_miedzymorze_decision`). The invitation itself is sent by the
next focus. The Sanation "sideline" and Danzig decisions are not used: the Regency removes the
oppositions, and the Danzig decisions need Clamp Down. `POL_defend_in_west_state` (wartime) is left to the model.

## training and armies

* `training`: `Dywizja Piechoty` (9 infantry + recon, verified in `history/units/POL_1936.txt`).
  6 in the queue, 2 at a time, deployed in Warsaw (state 10). The start army of 28 infantry, 2
  mountain and 10 cavalry brigades is about 320k men. The Intermarium gate needs 700k
  **fielded** army manpower, which means about 40 more 9-battalion divisions. Raise `queue`
  while rifle stocks allow. **META candidate (unverified):** deploy queued divisions early, with
  low equipment, to push `has_army_manpower` over the gate. Test it once with a handful of
  divisions while watching the Between-the-Seas tooltip. Never call a paper army ready.
  The runner cannot design templates. Build the 7/2 infantry template by hand and swap the name.
* `armies`: 24 or fewer divisions per army, `absorb`. Field marshals Rydz-Śmigły (skill 4) and
  Sikorski (3), then Kutrzeba, Abraham (4), Anders (4) and Sosnkowski. These are the
  English localisation names of characters recruited in the history file.

## production

Start (`POL_1936.txt`): 11 military factories, 3 of them on lines (`infantry_equipment_1` 2,
`support_equipment_1` 1). **8 are idle on day one.** `production_min_idle: 9` keeps the
rebalance off until the four lines take exactly those 8: rifles 4, artillery 2, support 1,
fighters (`small_plane_airframe_1`) 1. That gives rifles 6 / support 2 / artillery 2 / fighter 1,
close to the Poland row in templates-equipment-production §7 (55/15/12/0/5/3/0/10). **Manual
edit once the lines exist:** set `production_min_idle` to 2 so new factories go to rifles,
then support, then artillery. Add `anti_tank_equipment_1` by hand after interwar_antitank.

## construction

At the start, free slots (category slots minus factories, from `history/states`) are: Warsaw 0,
Danzig 0, Kraków 88 4, Kielce 90 4, Stanisławów 89 4, Wołyń 93 4, Lwów 91 3, Łódź 87 2, Lublin 92 2,
Poznań 86 5, Katowice 762 5. Focuses add more.

| Job | States | Reason |
|---|---|---|
| 1. `industrial_complex` x2 | 90 Kielce, 88 Kraków, 92 Lublin, 87 Łódź, 89 Stanisławów, 91 Lwów, 93 Wołyń | Interior (the Central Industrial Region), behind the Vistula and San. Poznań, Katowice, Gdynia and Danzig are left out because they will be front line in 1939. |
| 2. `arms_factory` x2, fallback any own state | same interior states | Reached only when job 1 is full. From early 1938 put it first (economy-build-orders §10.2: civilian factories until early 1938, then military). |
| 3. `infrastructure` | 94, 96 (level 2), 95, 93, 90, 89, 92, 98, 97 | A sink that uses no slot. It also feeds the eastern screen's supply. |

## Pre-war checklist (what to have by 1938-01-01, and by the war)

* **Economy by 1938-01:** Partial Mobilisation and Limited Conscription. National Defence Fund
  running. Roughly 30+ civilian and 18+ military factories (heuristic; check `hoi4_economy`). 4 research slots, and the 5th as soon as
  Poland has more than 50 factories. Peasant strike cancelled. Regency done with the Wettin king elected.
* **Alliances and faction:** create Intermarium (Between the Seas) and **not** the Allies.
  The tree's route to the Allies (`POL_align_with_the_west` -> `POL_join_allies`) goes through
  the Sanation or Sejm/liberal branches, and this plan does not take them. Intermarium leadership
  also requires not being in another faction.
  Invite Czechoslovakia, then Austria and Hungary, **before Germany's Anschluss (about 1938-03)**.
  Record Austria's faction status every month. Then pick up Yugoslavia (`POL_protect_yugoslavia`)
  and the Baltic/Lithuania sub-foci. The UK/France guarantee still comes from German tension. Never
  accept `germany.86` (it gives away Danzig and Gdynia).
* **Fortification lines (1938-39, after this window):** Plan West's +30% and Central Defence's
  +25% bunker speed stack. Instant bunkers: Silesia (88, 762, 72 if held), Prussia line (97, 98,
  807), Poznań/Łódź (86, 87), Hel (807). Build by hand on the Narew-Vistula-San river towns and
  the Silesian gap. Do not wall the whole border. The runner's construction has no bunker job.
  Do that by hand after `land_fort_tech_1`.
* **Army by 1938:** 60+ equipped divisions on the way to 90 by mid-1939. 4-6 division
  counter-attack reserve. Cavalry brigades merged. AT companies once interwar_antitank is done.
  A fighter wing for Warsaw-Łódź-Kraków.
* **Commonwealth (after 1938):** Royal Sejm -> Royal Guards -> Support Monarchism in Lithuania
  (LIT monarchist support 80, through the Lithuanian decisions) -> King of Lithuania (personal
  union) -> `POL_LIT_union`. It needs neutral government and no subject status.

## Where poland-meta.md disagrees with the 1.19.4.1 files

* `POL_claiming_lithuania` is not a 10-point focus to take. Its `available` is `always = no`.
  Accepting Friedrich Christian (`NSB_poland_royal_election.1` option a) completes it for free.
  The same applies to habsburg, hohenzollern and the others for their candidates.
* `POL_plan_east` cannot be run alongside Plan West. It needs Plan West untaken or
  `POL_complete_plan_west`, and that focus removes the bunker modifier and closes every remaining
  western sub-focus. Pick one plan at a time.
* The Intermarium mission decisions do not invite anyone. They add +20 AI alliance strategy, only
  one runs at a time, and they cost -10% PP gain for 35 days. The invitations are the focus events (poland.16).
* The economy-law advice is incomplete. In 1.19.4 Civilian Economy has -30% construction and Low
  Mobilisation -10%, while Partial Mobilisation has no construction penalty. Poland, at war support
  0.5, should go straight to Partial. This also contradicts economy-build-orders.md
  ("Early first, Partial 12-18 months before war").
* `POL_national_defence_fund` (-35% consumer goods for 720 days) and the advisors Lubomirski
  (-20% consumer goods), Kwiatkowski (+10% civilian construction) and Wojciechowski (+15% PP) are missing.
* The reference header says 1.19.3. The installed game is 1.19.4.1. The keys used here were
  checked against 1.19.4.1.

## Not verifiable from the files

* Whether early deployment of under-equipped divisions counts toward `has_army_manpower`
  (META candidate, see training).
* The AI's acceptance of Intermarium invitations, and the date of AI Germany's Anschluss.
* Start political power, and whether `hoi4_politics_choose` takes the character id or the token
  (they are the same string for these POL advisors).
* The exact instant-build counts of the industry focuses. Their rewards branch on free slots
  (`random_owned_controlled_state` with `prioritize`).
