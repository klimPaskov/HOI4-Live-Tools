# Poland META standing plan, 1936-01-01 to 1938-01-01

Companion to `poland-meta-1936-1938.plan.json`. It has the same schema as
`germany-fair-1936-1938.plan.json`. Copy the JSON to the `plan.json` the runner reads
(`native/src/plan_run.rs`, re-read every tick). Keys that start with `_` are ignored by
the runner. `_validator` only feeds `scripts/validate_plan.py`.

```
python3 scripts/validate_plan.py skills/hoi4-campaign-playbook/plans/poland-meta-1936-1938.plan.json --strict
# checked: research=52, focus=27, production=3, construction=3, politics=5, decisions=3; 0 error(s), 0 warning(s)
```

**Scope.** This window is the build-up for the campaign goal in
[poland-meta](../references/poland-meta.md): survive, defeat Germany and the USSR, and form
Intermarium, plus the Commonwealth where the active DLC set supports it (see below). META means
the strongest intended mechanics plus exploits a player can do through the normal UI. It still
bans the console, save editing, memory writes, firing events and reloading to undo.

## Active DLC set (this decides the whole plan)

The running game's log reports **3 active DLCs**: Anniversary Pack, **Poland: United and Ready** (PUR)
and Rocket Launcher Unit Pack. These are recorded in `_validator.dlcs`. Other DLC folders are
installed but **not active**: No Step Back (NSB), By Blood Alone (BBA), Man the Guns (MTG),
La Résistance, Arms Against Tyranny, Götterdämmerung, Waking the Tiger and No Compromise, No
Surrender. `dlc_load.json` lists nothing as disabled, so it is not the source of truth here.
Every `has_dlc` below was checked against that set in HOI4 1.19.4.1.

| Area | Effect without NSB, BBA or MTG (PUR active) |
|---|---|
| Focus tree | `polish_focus` still loads: its `country` factor needs PUR **or** NSB. |
| Political roots | `allow_branch` hides `POL_assemble_the_regency_council` (needs NSB), `POL_organize_the_peasants_strike`, `POL_nationalist_constitution`, `POL_elect_a_PSL_leader`, `POL_demand_LIT_pavel` and `POL_demand_pomerania`. **The Regency/monarchy branch is gone**: no Fifth of November, no royal election (`NSB_poland_royal_election.*` is only fired from Fifth of November), no Wettin/Habsburg/Hohenzollern/Romanov lines, no Royal Sejm, Royal Guards, King of Lithuania or `POL_LIT_union`. Visible roots: `POL_complete_april_constitution` (Sanation) and `POL_restore_the_sejm` / `POL_internationalism` (`allow_branch = NOT NSB`, the PUR democratic tree). |
| Other hidden focuses | `POL_prepare_for_the_inevitable` (needs MTG), `POL_expand_polish_intelligence*` (needs La Résistance). `POL_adaptive_designs` needs Arms Against Tyranny in `available`. `POL_lithuanian_annexation` only reads the GER Oppose Hitler variant. |
| Start ideas | PUR adds `uncontrolled_exports`, `POL_april_constitution_1`, `POL_looming_peasants_strike` and both Sanation oppositions (history file, `has_dlc = "Poland: United and Ready"`). **The peasant-strike clock still runs**, so Agrarian Reform is still mandatory. |
| Intermarium | `POL_the_between_the_seas_concept` and its children have no DLC gate. The faction comes from `faction_template_miedzymorze_faction`. Only the manifest is NCNS-gated. |
| Commonwealth | Only the formable decision `form_commonwealth` (`formable_nation_decisions.txt`, no DLC gate) is left. It needs control of Lithuania's states 11 and 189 plus the 15 Polish core states, Lithuanian core compliance above 80, and no subject status. In practice that means taking Lithuania by war or through `POL_lithuanian_annexation` (after `POL_align_with_the_west`, which is not on the Sejm route) and then raising compliance. This is post-1938 work. |
| Air | The `bba_air_techs_folder` is unavailable and the legacy `air_techs_folder` is used (`common/technology_tags/00_technology.txt`). Poland starts with `early_fighter`, `early_bomber` and `CAS1` and a variant "PZL P.24" of **`fighter_equipment_0`**. `small_plane_*` keys and BBA techs (`engines_2`, `aircraft_construction`, `aa_hmg`, `range_improvements`) do not exist for this run. |
| Armour, navy | The legacy `armour_folder` (`gwtank`) and legacy naval techs are used. The plan uses neither. |
| Characters | Wojciechowski (available only via the NSB Regency, `POL_modus_vivendi` or the NSB-only `POL_reassemble_the_sejm`) and Lubomirski (Regency only) are out. The validator's current DLC check did not flag these advisors or the `small_plane_*` / BBA keys in a negative test, so they were removed by hand from the files. |

## Top-level keys

| Key | Value | Why |
|---|---|---|
| `stop_on_date` | `1938.01.01` | The plan's window. If you extend it, extend the focus tail and switch construction to military factories. |
| `event_options` | `{}` | No verified multi-option Polish event on this route needs automation. The royal election cannot happen without NSB. The Danzig crisis (`germany.86`, 1939) is for the model to decide. |
| `alarms` | PP 200, CG 0.40, idle civ 3, idle mil 2 | Poland has few factories, so 3 idle civilian factories is already a real loss. |

## research_priority (52 technologies)

The start techs come from `history/countries/POL - Poland.txt` without NSB, BBA or MTG:
`infantry_weapons1`, `tech_support`, `tech_engineers`, `tech_recon`, `tech_mountaineers`,
`tech_trucks`, `gw_artillery`, `interwar_antiair`, `fuel_silos`, `synth_oil_experiments`,
`basic_train`, `gwtank`, `early_fighter`, `early_bomber`, `CAS1`, `early_destroyer` and
`early_submarine`. Poland starts with **3 slots**. The 4th comes from `POL_additional_research_slot1`.

| Positions | Block | Reason |
|---|---|---|
| 1-3 | basic_machine_tools, construction1, electronic_mechanical_engineering | Day-one industry. The Four Year Plan's +100% construction bonus (about 1936-02-05) is kept for construction2. |
| 4-10 | support_weapons, construction2, radio, improved_machine_tools, interwar_artillery, concentrated_industry, interwar_antitank | Poland lacks support_weapons and interwar_artillery. interwar_antitank unlocks `anti_tank_equipment_1`. Concentrated industry for output. Dispersed is exclusive with it. |
| 11-25 | excavation1, mechanical_computing, concentrated_industry2, basic_fortification_tech, infantry_weapons2, support_weapons2, motorised_infantry (prerequisite of the support companies), logistics/field hospital/signal, improved_equipment_conversion, excavation2, improved_radio, computing_machine | Fortification tech comes before the 1938 bunker programme. |
| 26-52 | land_fort_tech_1, **fighter1** (legacy, `fighter_equipment_1`), encryption/decryption, fuel_refining, rubber_processing, construction3, advanced_machine_tools, concentrated_industry3, artillery1, antitank1, antiair1, improved_infantry_weapons, second-tier support, ... | Tail for 1938-39. |

## focus_path (27 focuses) and focus_fallback

One focus point is 7 days. The dates assume the Intermarium gate is not met in 1936.

| # | Focus | Cost | Done about | Note |
|---|---|---|---|---|
| 1 | POL_the_four_year_plan | 5 | 1936-02-05 | Construction tech bonus, +10% building speed for 1460 days. |
| 2 | **POL_restore_the_sejm** | 10 | 04-15 | The META political root without NSB. One focus removes `POL_april_constitution_1` (-15% PP, +15% law and advisor costs) **and** both Sanation oppositions. It adds `POL_empowered_sejm_idea` (+10% stability, -0.1 neutrality drift). It is exclusive only with `POL_complete_april_constitution`, which would instead *raise* the April constitution and both oppositions to level 2 and leave you to dismantle them over 6-8 more focuses. The ruling party stays neutral (elections are off). The follow-ups (`POL_internationalism` -> liberalism/authoritarianism) are not in the path. |
| 3 | POL_central_region_strategy | 5 | 05-20 | Instant factories and slots in Warsaw, Kielce, Lublin and Łódź. |
| 4-5 | POL_fill_the_railways_gaps, POL_agrarian_reform | 10+10 | 07-29, 10-07 | Agrarian Reform must finish before 1936-12-31 (Looming Peasants' Strike, 365 days). That leaves about 85 days of margin. |
| 6-9 | POL_the_between_the_seas_concept, POL_the_mediterranean, POL_coerce_czechoslovakia, POL_austro_hungarian_alliance | 10+5+5+5 | when the gate opens | Gate: `has_army_manpower > 699999`, not a subject, and unaligned or a faction leader. Until then the runner skips the focus and goes on. Once open, the chain runs ahead of the economy focuses so Austria is invited (poland.16) before the March 1938 Anschluss. `GER_anschluss` is bypassed if AUS is in a faction without GER. |
| 10 | POL_expansion_of_new_towns | 5 | 11-11 | Arms factories (Poznań, Gdynia, Kraków). |
| 11 | POL_additional_research_slot1 | 10 | 1937-01-20 | 4th slot. Free while Poland has 3 slots. |
| 12 | POL_national_defence_fund | 10 | 03-31 | **-35% consumer goods for 720 days.** |
| 13 | POL_start_central_industrial_region | 10 | 06-09 | Factories in Kraków, Stanisławów and Lwów. |
| 14-15 | POL_prepare_for_the_next_war, POL_plan_west | 5+10 | 07-14, 09-22 | -50% on one land doctrine. Bunker speed +30% (`POL_plan_modifier`) for 547 days in states that border Germany. |
| 16-18 | POL_invest_in_the_old_polish_region, POL_modernize_congressional_factories, POL_additional_research_slot2 | 5+10+10 | 1937-10 to 1938-03 | Slot2 needs **more than 50 owned factories** once Poland has 4 slots. |
| 19-27 | new_military_academy, standardisation_of_equipment, central_defence_of_poland (+25% bunker speed, 720 days), local_western_plans, silesia_fortified_area, the_prusya_line, expand_poznan_forts, hel_fortified_area, army_modernisation | | 1938 | Tail: instant bunkers and the `POL_defend_in_west_state` decision. |

`focus_fallback`: Katowice mines (+24 steel, +12 aluminium), Upper Silesia, expand/eastern industrial
region, anti-air, AT and artillery bonuses, air base, fighter modernisation (without BBA it gives
the `light_fighter` bonus), Warsaw railway station.

**Never add:** `POL_complete_april_constitution` (exclusive with the Sejm root); any NSB-hidden
focus (Regency, election lines, Royal Sejm, `POL_institute_royal_guards`,
`POL_support_monarchism_in_LIT`, `POL_king_of_lithuania`, `POL_LIT_union`,
`POL_prepare_for_the_inevitable`). Also `POL_plan_east`, which is blocked until
`POL_complete_plan_west`, and `POL_complete_plan_west` itself, which removes the bunker modifier and
closes the remaining Plan West focuses. Leave `POL_clamp_down_on_danzig` to the model. It gives
75 PP and 50 command power but starts 40 resistance in Danzig. Its follow-up,
`POL_integrate_gdansk_industries`, is the cheapest way to replace `uncontrolled_exports` with `export_focus`.

## politics

| Pick | Cost | Why |
|---|---|---|
| government 2 `partial_economic_mobilisation` | 150 (+15% until the Sejm focus) | Civilian Economy gives -30% civilian and military factory construction speed. Partial needs war support > 0.25 (Poland starts at the 0.5 default). It has no construction penalty and gives +10% arms speed. |
| government 3 `POL_eugeniusz_kwiatkowiski` | advisor | `captain_of_industry`: +10% civilian factory, infrastructure and railway construction. No conditions. |
| government 4 `POL_jozef_beck` | advisor | `smooth_talking_charmer`: +5% PP, +10% trade opinion. No conditions. |
| government 0 `limited_conscription` | 150 | War support > 0.1. Gives the manpower for the 700k gate. |
| government 5 `POL_jozef_haller` | advisor | `war_industrialist`: +10% arms factory construction, for the 1938 switch to military factories. |

All three advisors are recruited in the history file with no DLC condition. They are character
ids, and also their `idea_token`s.

## decisions

`POL_diplomatic_mission_to_CZE`, `_AUS`, `_HUN` (10 PP each, 35 days, -10% PP gain while running,
one at a time). Each appears after the focus before the invitation. Each adds +20 AI alliance
strategy toward POL. The invitation itself is the next focus's poland.16 event.

## training and armies

* `training`: `Dywizja Piechoty` (9 infantry + recon, `history/units/POL_1936.txt`, no DLC
  condition). 6 in the queue, 2 at a time, deployed in Warsaw (10). The start army is about 320k
  men. The Intermarium gate needs 700k **fielded** army manpower, about 40 more divisions. **META
  candidate (unverified):** deploy queued divisions early, with low equipment, to pass
  `has_army_manpower`. Test it once while watching the tooltip.
* `armies`: 24 or fewer divisions per army. Generals (recruited unconditionally): Rydz-Śmigły
  (field marshal, 4), Sikorski (field marshal, 3), Kutrzeba, Abraham, Anders and Sosnkowski.
  Haller is used as an advisor, not as a general.

## production

Start without BBA (`POL_1936.txt` plus `POL_1936_air_legacy.txt`): 11 military factories, 4 of them
on lines (`infantry_equipment_1` 2, `support_equipment_1` 1, **`fighter_equipment_0` 1**). **7 are idle.**
`production_min_idle: 8` keeps the rebalance off until rifles 4, artillery 2 and support 1 take
them. That gives rifles 6 / support 2 / artillery 2 / fighter 1. **Manual edits:** once the lines exist, set
`production_min_idle` to 2. When `fighter1` is researched, switch the fighter line to
`fighter_equipment_1`. Add `anti_tank_equipment_1` after interwar_antitank.

## construction

Free slots at the start: Kraków 88 4, Kielce 90 4, Stanisławów 89 4, Wołyń 93 4, Lwów 91 3, Łódź 87 2,
Lublin 92 2, Warsaw 0, Danzig 0, and the border states Poznań 86 5 and Katowice 762 5.

| Job | States | Reason |
|---|---|---|
| 1. `industrial_complex` x2 | 90, 88, 92, 87, 89, 91, 93 | Interior Central Industrial Region, behind the Vistula and San. |
| 2. `arms_factory` x2, fallback any own state | same | Put it first from early 1938. |
| 3. `infrastructure` | 94, 96, 95, 93, 90, 89, 92, 98, 97 | A sink that uses no slot, and supply for the east. |

## Pre-war checklist

* **By 1938-01:** Partial Mobilisation and Limited Conscription. Sejm restored (no April
  constitution, no oppositions). Peasant strike cancelled. National Defence Fund running. 4 research
  slots (the 5th above 50 factories). Roughly 30+ civilian and 18+ military factories
  (heuristic; check `hoi4_economy`).
* **Alliances:** Intermarium, not the Allies. `POL_join_allies` is only reachable through
  `POL_align_with_the_west`, which needs Sanation or Sejm -> liberalism -> draw closer to Britain,
  and it requires being in no faction. Invite Czechoslovakia, then Austria and Hungary, before the
  Anschluss (about 1938-03). Record Austria's faction status monthly. Never accept `germany.86`.
* **Fortifications (1938-39):** Plan West +30% and Central Defence +25% bunker speed. Instant
  bunkers in Silesia (88, 762), the Prussia line (97, 98, 807), Poznań/Łódź (86, 87) and Hel (807).
  Build by hand on the Narew-Vistula-San river towns. The runner has no bunker job.
* **Army by 1938:** 60+ equipped divisions on the way to 90 by mid-1939. A 4-6 division reserve.
  AT companies. A fighter wing over Warsaw-Łódź-Kraków.
* **Commonwealth (after 1938, if pursued):** conquer or annex Lithuania (states 11 and 189), hold
  the 15 Polish core states, raise Lithuanian core compliance above 80, then take `form_commonwealth`.

## Not verifiable from the files

* Whether early-deployed, under-equipped divisions count toward `has_army_manpower`.
* AI acceptance of Intermarium invitations, and the AI Anschluss date.
* Start political power. The exact instant-build counts of the industry focuses, which branch on
  free slots.
* Whether a bypassed `GER_anschluss` counts as completed for `POL_lithuanian_annexation`'s condition.
