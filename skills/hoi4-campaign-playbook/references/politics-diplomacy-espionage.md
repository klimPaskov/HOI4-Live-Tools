# Politics, diplomacy, factions and espionage (HOI4 1.19.3, FAIR)

Source of truth: the installed game (`Hearts of Iron IV/common/...`, `history/countries`, `events`, `localisation/english`).
Build string `Operation Postern v1.19.3.0.c01a (5632)`. Wiki (hoi4.paradoxwikis.com) used only as a cross-check.
Everything here is observational reading of game files, not live evidence. Items marked **[UNSURE]** were inferred or not
confirmed in a running game; verify with the in-game tooltip before relying on them.

Installed DLC folders (`Hearts of Iron IV/dlc`): German Historical Portraits, Polish Content Pack, Rocket Launcher Pack,
La Resistance, No Step Back, By Blood Alone, Arms Against Tyranny, Gotterdammerung, No Compromise No Surrender.
Not in `dlc/`: Man the Guns, Death or Dishonor, Together for Victory, Waking the Tiger (but those four ship in `integrated_dlc/`, i.e. they are part of the base game and `has_dlc` checks for them pass); Poland: United and Ready is `dlc/dlc002`.
Many branches below are gated on `has_dlc`; the gate decides what exists. Re-check the playset before day one.

## 1. Political power (PP)

| Fact | Value | Source |
|---|---|---|
| Base daily PP | +2/day (define is commented "weekly" but the wiki and play show daily) **[UNSURE on comment]** | `defines/00_defines.lua` `NPolitics.BASE_POLITICAL_POWER_INCREASE = 2` |
| Cap / floor | +2000 / -500 | `NGame.POLITICAL_POWER_UPPER_CAP / LOWER_CAP` |
| Stability >50% | PP +10% (scaled, full at 100%) | `modifiers/00_static_modifiers.txt` `stability_good_modifier` |
| Stability <50% | PP -20% (scaled, full at 0%) | `stability_bad_modifier` |
| Difficulty | Normal = baseline, nothing added | wiki, `diff_*` modifiers |
| Leader/advisor boosts | `political_power_factor` +5% to +20% on traits (see section 4) | `country_leader/00_traits.txt` |
| MIO design team on a research slot | costs 0.1 PP/day each (production lines 0) | `NIndustrialOrganisation.ASSIGN_DESIGN_TEAM_PP_COST_PER_DAY` |
| Tooling | `hoi4_politics`, `hoi4_decisions`, `hoi4_diplomacy` all show cost/blocked state | repo tools |

Approximate income: 2 x (1 + 0.1 at 100% stability, or -0.2 at 0%) x (1 + sum of advisor/leader factors). A realistic
mid-game figure with two PP advisors is about +2.6 to +3.0/day. Over a game year that is ~950-1100 PP.

### What PP buys, in priority order (peacetime 1936-1939)
1. **Advisors that pay back**. A 100-150 PP advisor with +10% construction/production speed or +15% research-adjacent effects
   returns more than most decisions. Hire the economy advisor first (section 4).
2. **Law changes (150 PP each)** only when the new law's effect beats the cost within about a year (section 3).
3. **Focus-linked and decision costs** that unlock real assets (fortification, factory slots, claims, formables).
4. **Diplomacy** at the moment it is needed: guarantee 15 PP + 15 per country already guaranteed; improve relations 10 PP start
   plus upkeep; send attache 100 PP plus 50 command power (`NDiplomacy`: `BASE_SEND_ATTACHE_COST`); wargoal generation
   0.2 PP/day base (`BASE_GENERATE_WARGOAL_DAILY_PP`), reduced by world tension.
5. **War-time stability/war-support decisions** (section 2) when a threshold is about to cost production.

### Banking rules (standing)
- Keep a reserve of **150 PP** at all times from the start of a war-prep year (one law swap) and **300 PP** during an active
  war (law swap + a crisis decision). Spend anything above the reserve on the highest-value item in the table above.
- Never sit at the 2000 cap. Anything above ~400 PP in peacetime with nothing queued is a defect: buy the next advisor or law.
- Do not bank PP for a faction-leader takeover (200 PP, `ASSUME_FACTION_LEADERSHIP_PP_COST`) unless the section 7 criteria are met.
- Negative PP is possible down to -500. Stay positive; crisis events and many decisions require positive PP.

## 2. Stability and war support

Exact modifiers from `modifiers/00_static_modifiers.txt` (scaled linearly between 50% and the extreme):

| State | Stability effect | War support effect |
|---|---|---|
| High (toward 100%) | factory + dockyard output +20%, consumer goods -20%, PP +10% | mobilization speed +30%, core-territory attack/defence +10%, command power gain +50% |
| Low (toward 0%) | factory + dockyard output **-50%**, PP -20% | mobilization speed -50%, command power gain -95%, surrender limit -30% |
| During war | `war_support_during_war`: stability -30% scaled by war support (not split) **[UNSURE how wiki phrases it]** | |

Drivers (defines): defensive war +0.2 war support, offensive war -0.2, world tension up to 0.4 scale effect on war support,
`BASE_STABILITY_WAR_FACTOR -0.2`, bombing and convoy raiding cut war support (caps: bombing -0.3, convoys -0.5, casualties -0.3).
Propaganda operations by enemy operatives cap at -0.2 stability and -0.2 war support.

**Crisis system** (`events/stability_events.txt`, `stability.3` hidden selector). Fires only while **at war, not civil war**, and
when stability <50% OR war support <50%. It is a 360-day mean-time event, x0.75 below 40%, x0.75 again below 20%, doubled for
non-democracies. It randomly adds `draft_dodging` (conscription x -0.2/-0.3/-0.4), `strikes_1..3` (factory output -20/-30/-50%,
consumer goods +40/60/80%) or `mutiny_1..3` (attack -25/-50/-75%, org -10/-20/-50%, training time +). It is suppressed if the
nation is in `disarmed_nation`/`volunteer_only` plus a civilian/isolation economy. A neglected level-3 crisis at stability <0.21
can start a civil war (`draft_dodging_mission` timeout, communist revolt). Crisis answers arrive as decisions in `stability_war_support.txt`
(category `crisis`, cost 0) that raise `stability.21/22/23`.

Healthy-range targets for the agent:
- Stability **>= 50%** always; **>= 60%** before law/MIO optimisation matters; < 40% is an emergency (factories -50% scaled).
- War support **>= 50%** to avoid crisis exposure; the mobilisation laws gate on it (section 3).
- Check both every game week during war (`hoi4_telemetry`).

Healing tools (all in `decisions/_generic_decisions.txt` unless noted):

| Decision | PP | Effect | Gate / timing |
|---|---|---|---|
| `improved_worker_conditions` | 100 | +0.005 stability/week for 180 days, consumer goods +10%, factory output -10%; re-enable after 540 days | stability <100%. Use only if stability <45% and no war in 3 months |
| `war_propaganda` | 150 | +0.005 war support/week for 90 days; re-enable 180 | war support <50%, visible only in war/threat |
| `war_propaganda_casualties` | 100 | +0.006/week casualty war-support offset, 180 days | casualties war-support <0 |
| `war_propaganda_radio_industry` | 75 | one-off +5% war support | needs radio tech and `threat > 0.25`. Take when it first unlocks |
| `war_propaganda_film_industry` | 100 | one-off +10% war support | at war, major or ally of a major, `threat > 0.3` (fascism) or 0.5 (others) |
| `war_bonds` | 50 (+25 each repeat) | consumer goods -20% for 180 days, small war support | war + high war support (democratic/communist 0.8, fascist 0.9 or surrender progress) |
| `desperate_defense` | 50 | defence +20%, dig-in +50% for 45 days | surrender progress >50% |
| `emergency_factory_conversion_*` | 100 | converts civilian to arms factories; war support -5% (offensive also stability -3%) | needs >5 civ factories; 90-day cooldown. **Use once** when going to total war, not as routine |
| `war_building_slot` | 50 | +1 shared slot in one state; war support -2.5%, stability -2.5% | war only; repeat while stability >60% |
| `infrastructure_building_slot` | 100 | +1 shared slot in a state with no free infra slot | peacetime ok, 30-day cooldown |

Rules of thumb: do not buy a `-war support` decision when war support is within 0.07 of 50% or of a law threshold you still need.
Ideology-focused spirits (`why_we_fight_focus` stability +10%, war support +10%; `indoctrination_focus` stability +5%) are
focus rewards, take the focus if it is on the path anyway. Advisors with stability/war-support traits are in section 4.
Capital/factory bombing wrecks war support, so air superiority over home states is also a stability policy.

## 3. Economy, trade and conscription laws

All laws cost **150 PP** (`ideas/_economic.txt`, `_manpower.txt`). Price modifiers: advisor traits `economy_cost_factor`,
`trade_laws_cost_factor`, `mobilization_laws_cost_factor` (Ludwig Erhard -33%, Shi Liang/Zhou Fohai -10%).
Laws are gated on war support (WS), government and war state:

### Economy law
| Law | Gate | Key modifiers |
|---|---|---|
| civilian_economy | none | consumer goods +35%, construction -30%, conversion +30% (worst for war) |
| low_economic_mobilisation | WS > 0.15 | consumer goods +30%, construction/arms -10% |
| partial_economic_mobilisation | WS > 0.25 | consumer goods +25%, arms-factory speed +10%, conversion -10% |
| war_economy | WS > 0.5 and (fascism or communism, or at war with a country that has IC ratio >0.4 to us) | consumer goods +20%, arms speed +20%, conversion -20%, factory energy +25% |
| tot_economic_mobilisation | at war and WS > 0.8 (0.9 for some variants) **[UNSURE exact]** | consumer goods 10-15%; strongest conversion and output |
| GER `totaler_krieg_economy` | focus `GER_totaler_krieg` and WS > 0.8 | consumer goods 15%, arms/dockyard speed +20%, factory output +5% |

(Consumer goods percentages are the engine's `consumer_goods_expected_value`; the lower, the more factories build things.)

Germany starts 1936 in `partial_economic_mobilisation` + `limited_conscription` + `limited_exports` (history/countries/GER). Upgrade
to `war_economy` as soon as WS > 0.5 and the 150 PP is available (fascist gate is satisfied by government alone) unless it
would drop PP below the reserve.

### Trade law
| Law | Gate | Effects |
|---|---|---|
| free_trade | needs a special focus (GER `autarky_efforts`, etc.) | factory +15%, research +10%, min export 0.8 |
| export_focus | needs focus or CHI nine-power spirit | factory +10%, research +5%, export 0.5 |
| limited_exports | democratic, at war, or after partial mobilisation | factory +5%, export 0.25 |
| autarkic_economy | none | research +5%, min export 0.15 |
| closed_economy | at war, fascism/communism/neutrality, or war economy | min export 0 |
| uncontrolled_exports (POL start spirit with Poland DLC; AAT) | | export 0.25, intel to others +40 |

Practical rule: **trade law to the highest factory output available** that does not starve your own resource needs; `export_focus`
or `free_trade` is worth the 150 PP only if you have the focus unlock. Otherwise `limited_exports` is the baseline.

### Conscription law (manpower, `conscription` factor of population)
| Law | Gate | Recruitable pop | Side cost |
|---|---|---|---|
| volunteer_only | none | 1.5% | none |
| limited_conscription | WS > 0.10 | 2.5% | none |
| extensive_conscription | fascism/communism, or at war with WS > 0.2 | 5% | training +10% |
| service_by_requirement | fascism/communism, or war with WS > 0.6 | 10% | factory/dockyard -10%, construction -10%, training +20% |
| all_adults_serve | war and WS > 0.7 | 20% | factory -30%, construction -30%, training +30% |
| scraping_the_barrel | war and WS > 0.85 | 25% | factory -40%, construction -40%, training +50% |

Timing: step up when idle manpower is below ~3 months of planned divisions, not earlier. `service_by_requirement` and above cut
factory output; take them only in a war where manpower, not equipment, is the bottleneck. Step down after the war.
`women_in_the_workforce` (decision, 100 PP) adds +3% conscription and -5% stability under `tot_economic_mobilisation` (GER: or
`totaler_krieg_economy`); requires WS >= 0.8-0.9 depending on ideology.

## 4. Advisors by role

Advisors are characters (`common/characters/<TAG>.txt`, `advisor = { slot idea_token cost traits }`; trait modifiers in
`country_leader/00_traits.txt`). `hoi4_politics` view lists slots, `hire` by idea token. PP default when `cost` is absent:
150 political, 50 military (`DEFAULT_PP_COST_*`). Hiring is gated by `allowed/available` triggers (focus, flag, government).
Pre-1.19 "generic" advisor lists no longer exist; each country has named characters.

### Role priority
1. **Political advisor slot 1: economy/construction** (production speed). 2. **Slot 2: PP or stability** (compounds). 3. **Slot 3:
war-time (resource, factory output) or intelligence**. 4. **Theorist** (doctrine cost -10% or mastery gain +15%): hire when doctrine
research starts. 5. **High command / chiefs**: cheap, +experience gain, +planning; hire as PP allows, never before the PP-positive political advisors.

### Germany (`characters/GER.txt`; start 1936 has no advisors hired)
| Advisor (token) | Slot / PP | Effect | Gate |
|---|---|---|---|
| Hjalmar Schacht (`hjalmar_schacht`) | political / 75 | arms + dockyard production speed +10%, consumer goods -10% | needs MEFO bills dynamic modifier active; not communist; gone when `GER_mefo_bills_ended` |
| Walther Funk (`walther_funk`) | political / default | consumer goods -10%, supply node speed +15%, free repair +15%, resource penalty -5% | fascism |
| Fritz Todt (`todt`) | political | industrial + infrastructure + rail speed +10% | flag `GER_reinstated_nazi_leadership` |
| Albert Speer (`speer`) | political | building speed +5% | same flag |
| Hermann Goring political (`GER_hermann_goring`) | political | PP +5%, local resources +5% | fascism + `GER_ascension_of_goring` |
| Rudolf Hess (`rudolf_hess`) | political | PP +10%, opinion gain doubles | `GER_party_chancellor_hess` + flag |
| Martin Bormann (`martin_bormann`) | political | PP +10%, political advisor cost -10% | `GER_party_chancellor_bormann` + flag |
| Joseph Goebbels (`joseph_goebbels`) | political | war support +5%, fascism drift | flag + `GER_ascension_of_goebbels` |
| Joachim v. Ribbentrop (`joachim_von_ribbentrop`) / 50 | political | justify wargoal time -15%, same-ideology opinion +5/month | `GER_reorganize_the_wehrmacht` + fascism + flag |
| Ludwig Erhard (`GER_ludwig_erhard`) | political | **economy law and trade law cost -33%**, consumer goods -15% | `GER_prioritize_economic_growth` |
| Wilhelm Canaris (`GER_wilhelm_canaris`) | political | +1 operative slot, agency upgrade time -20% | agency owned + `GER_reorganize_secret_services` (+ communist path variant) |
| Theorists: Guderian (`heinz_guderian`, 200), Manstein (`manstein`, 150) | theorist | mobile-warfare mastery gain +15% | Guderian needs `GER_adopt_new_panzer_doctrine` |
| Theorists: Wever (`walther_wever`, 100), Udet (`ernst_udet`, 100) | theorist | air doctrine cost -10% | Udet needs `GER_dive_bombers` |
| Military: Rundstedt (100, `gerd_von_rundstedt`), Fritsch (100), Beck (`ludwig_beck`), Halder, Keitel, Kesselring (air), Raeder (navy, 100), Donitz (navy, 200) | high command / chiefs | experience, planning, org | mostly open |

Germany 1939 bookmark start adds `walther_funk`, `GER_franz_seldte`, `ludwig_beck`, `gerd_von_rundstedt`, `goering` and sets
`war_economy` + `extensive_conscription` with +1000 PP, so the 1936 start is the only start where the order above matters.

### Poland (`characters/POL.txt`; focus tree enabled by No Step Back or Poland: United and Ready)
| Advisor (token) | Slot / PP | Effect | Gate |
|---|---|---|---|
| Eugeniusz Kwiatkowski (`POL_eugeniusz_kwiatkowiski`) | political | industrial + infra + rail speed +10% (captain of industry) | no focus gate in file |
| Henryk Floyar-Rajchman (`POL_henryk_floyar_rajchman`) | political | same, plus synthetic refinery +10% | needs `POL_the_sanation_right` and a Swedish trigger **[UNSURE]** |
| Jozef Haller (`POL_jozef_haller`) | political | arms + dockyard speed +10%, fuel silo +10% (war industrialist) | none |
| Jozef Beck (`POL_jozef_beck`) | political | PP +5%, improve-relations upkeep -50% | none |
| Ignacy Moscicki (`POL_ignacy_moscicki`) | political | PP +10%, neutrality drift | `POL_ozon` + `POL_dissolve_the_bbwr` |
| Stanislaw Wojciechowski (`POL_stanislaw_wojciechowski`) | political | PP +15%, stability +5% | `POL_modus_vivendi` + `POL_reassemble_the_sejm` + `POL_assemble_the_regency_council` |
| Aleksander Kakowski (`POL_aleksander_kakowski`) | political | stability +15% | regency focus, neutrality/fascism |
| Jan Kowalewski (`POL_jan_kowalewski`) | political (intel) | +1 operative slot, agency upgrade -15% | needs agency |
| Rydz-Smigly (`POL_edward_rydz_smigly`, 100), Lupaszko (`POL_tomislaw_lupaszko`, 100) | theorist | land doctrine cost -10% | Rydz needs `POL_the_sanation_right` |

Poland order: Kwiatkowski -> Haller -> Beck (if relations/PP matter) -> Lupaszko when doctrine starts.

### China, Nationalist (`characters/CHI.txt`; many gated by the NCNS tree)
| Advisor (token) | Slot / PP | Effect | Gate |
|---|---|---|---|
| Zhang Qun (`CHI_zhang_qun`) | political | **PP gain +15%** (`political_power_gain`) | none in file |
| Zhu Jiahua (`CHI_zhu_jiahua`) | political | infra/rail +10%/+5%, supply node speed +10% | none |
| Mei Siping (`CHI_mei_siping`) | political | industrial/infra/rail +10% | visible after `CHI_look_for_allies` |
| Chen Lifu (`CHI_chen_lifu`) | political | research +6%, industrial speed +5% | visible after `CHI_end_military_primacy` |
| Wang Chonghui (`CHI_wang_chonghui`) | political | improve-relations -10%, PP +5% | none |
| Dai Jitao (`CHI_dai_jitao`) / Ju Zheng (`CHI_ju_zheng`), 100 | political | PP gain +5% / +10% | `CHI_revive_the_western_hills_group` |
| Mao Renfeng (`CHI_mao_renfeng`) | political (intel) | +1 operative, agency upgrade -20%, intel network +20% | `CHI_empower_the_bureau_of_investigation_and_statistics` |
| Bo Yibo (`CHI_bo_yibo`), Jiang Baili (`CHI_jiang_baili`) | theorist, 100 | land doctrine -10% / -5% + defence +5% | open |

China order: Zhang Qun (PP) -> Zhu Jiahua / Mei Siping (infra for the long rebuild) -> Chen Lifu. Avoid Du Yuesheng (stability -10%).

### Generic traits (other countries; same trait names reused)
`captain_of_industry` (industrial/infra/rail +10%), `war_industrialist` (arms/dock +10%), `silent_workhorse` (PP +15%), `popular_figurehead`
(stability +15%), `backroom_backstabber` (justify time -10%, subversion upkeep -25%), `smooth_talking_charmer` (PP +5%, improve relations
-50% upkeep), `ideological_crusader` (party popularity -> stability), `political_commissar` (communism, +30 command power cap).
Chiefs: `army_chief_*_1` etc. give small experience/org effects; take them last.

### Designers and MIOs
Old "designer" ideas (`tank_manufacturer`, `generic_*_manufacturer`, cost 150) still sit in `zzz_generic.txt` as legacy shells; in 1.19
the active system is **Military Industrial Organizations** (`common/military_industrial_organization`). MIOs are not bought with PP.
- Fund rules: +500 funds x research cost when a matching design team finishes; +0.1 funds per IC/day from a manufacturer assigned to a
  production line (cap 100/day); size-up costs 700 funds scaled by level (`NIndustrialOrganisation`).
- Each size-up grants 1 trait point. Policies cost 25 PP to attach (default), cooldown 180 days; some need MIO size >5.
- Design team bonus is +5% research speed for matching techs; the design team costs 0.1 PP/day on a research slot.
- Rule: assign a design team only to a tech you would research anyway; assign the MIO with the best production-efficiency traits to
  the biggest line (infantry equipment, then artillery/trucks). Review once per game quarter, not daily.

## 5. Decisions worth taking (by year)

Use `hoi4_decisions list available=true`. Costs from `decisions/*.txt`.

**Every year**: `infrastructure_building_slot` (100, per state flag, 30-day cooldown) when there is an infra-limited state; `war_building_slot` in war;
`restructure_supply_system` (once, free, supply-node speed x4 for 120 days) when supply nodes are the limit.

**Germany** (`decisions/GER.txt`):
- 1936-37: `GER_mefo_bills_mission` is a running mission (do not cancel, `GER_cancel_mefos` is free but ends Schacht's bonus). `GER_seize_gold_reserves` (25 PP) when available. Price-control missions (`GER_guns_before_butter`, `GER_arbeit_und_brot`, `GER_collect_scrap_metal`, `GER_ersatz_material`, 30 PP; `GER_import_tariffs`, `GER_implement_rations`, `GER_implement_wage_controls`, 15 PP) are available after `GER_the_four_year_plan` -> `GER_institute_price_controls`: take the production/resource ones that match the current bottleneck.
- Pre-war: `GER_dismantle_czechoslovakian_forts` (50) after Czechoslovakia, `GER_dismantle_maginot` (50) after France.
- After conquest: `GER_reichskommissariat_*` (25 each) turn conquered territory into compliant subjects; `GER_develop_puppet` / `GER_exploit_puppet` (120).
- Faction: `GER_invite_to_faction_decision` (25) to bring minors in.

**China** (`decisions/CHI_decisions.txt`): `CHI_army_reform` / `CHI_lessons_of_war` (0 PP) first; `CHI_60_divisions_plan` (90); `CHI_forced_loans` (100, for PP-starved starts); warlord integration `CHI_integrate_the_army` / `_administration` / `CHI_full_integration` (150 each); `CHI_flying_tigers`, `CHI_expand_the_burma_road`, `CHI_expand_the_ledo_road` (25 each) to improve supply; industrial evacuation decisions (25) only when a province is about to fall.

**Poland** (`decisions/POL.txt`): `POL_miedzymorze_decisions` missions (10 PP each) improve ties with Nordic/Balkan targets of the Intermarium; `POL_defend_in_east_state` / `_west_state` (50) set defence plans; `POL_sideline_the_left` / `_right` (100) steer the Sanation split; `POL_unification_propaganda` (75) for Danzig.

## 6. Diplomacy

### Costs and numbers (defines)
| Action | Number |
|---|---|
| Guarantee | 15 PP + 15 per country already guaranteed; revoke 15; `TENSION_GUARANTEE -5` (lowers world tension) |
| Improve relations | 10 PP start; upkeep 0.2 PP (same ideology group) or 0.4 PP (different) per update |
| Send attache | 100 PP + 50 command power |
| Justify wargoal | 0.2 PP/day base, -50% at 100% world tension, -75% versus a major already at war (`WARGOAL_VERSUS_MAJOR_AT_WAR_REDUCTION`); minimum cost 2.0 (~10 days). Typical real durations are 30-60 days; the UI shows days. **[UNSURE: exact day count depends on the wargoal base cost 200-125 and per-state 50-10, and on production size]** |
| Wargoal types (`common/wargoals`) | `take_state` (base 200, +50 per state, threat 2), `take_claimed_state` (base 125, +10/state, threat 0.15, takes -50%), `take_core_state` (base 125, threat 0.1, -80%), `puppet_wargoal_focus`, `liberate_wargoal`, `annex_everything` (threat 0.5), `topple_government` (force government, threat 0.5), `civil_war`. Use claimed/core goals: cheaper and lowest threat |
| Claims | focus-granted claims (`add_state_claim`) give `take_claimed_state` |
| Tension | war with CB +10, no-CB war +14, join faction +4, annex a claimed state +0.1, core -0.5, state annexation 0.1 |
| NAP | expires in 48 months, unbreakable for 12 months, breaking early costs war support 0.2 (same penalty for breaking a guarantee) |
| Kick/rejoin faction | 90 days before kicking a new member, 60-day truce after kick |
| Volunteers | need 30 divisions to send; tension 0.2 per division |
| Subject autonomy | change autonomy level costs 50 PP base; annex a subject 300 PP (`AUTONOMY_LEVEL_CHANGE_PP_ANNEX`) |

### Ideology rules (`common/ideologies`)
Democracy: `can_only_justify_war_on_threat_country`, cannot puppet, cannot declare war on same ideology, cannot send volunteers
(the allied democracies are slow). Fascism/communism/neutrality may force government, puppet and send volunteers.
Joining a faction of an ideology needs that ideology popularity >= 30% (`IDEOLOGY_JOIN_FACTION_MIN_LEVEL 0.3`; halved to 0.15 in a defensive war via `JOIN_FACTION_LIMIT_CHANGE_AT_WAR 0.5`) **[UNSURE on the exact scaling]**.

### Puppets and annexation
- Autonomy levels (`common/autonomous_states`): puppet (min freedom 0.4, manpower share 0.9, can_master_build), satellite (0.8, manpower 0.4; needs Death or Dishonor, which is integrated, so it exists), integrated puppet (requires Together for Victory, also integrated), dominion (0.8, 0 manpower).
- Puppet via a war (peace conference) or a wargoal of `puppet` type. Autonomy rises from resources sent to overlord, puppet war score and lend-lease; falls with master-built construction and lend-lease to puppet.
- Annex when the diplomacy action is available (PP 300) or via focus effects. Annex only after the puppet is no longer needed for building slots (`can_master_build_for_us`).

### Historical AI reactions (Normal, historical focus on)
- AI countries follow `ai_strategy` plans and their focus tree; defence of guaranteed targets depends on faction and power projection. The AI does not undo guarantees merely because of a pending wargoal.
- Democratic AIs call allies with desire +50 and rarely declare offensive wars without a threat country; they give each other military access when facing a common enemy (`ACCESS_SCORE_FOR_DEMOCRATIC_COUNTRIES 500`).
- An AI is more likely to accept faction invites if ideologies match (`DIPLOMACY_FACTION_WRONG_IDEOLOGY_PENALTY 60`, neutrality -50), the invite is during war (`DIPLOMACY_FACTION_WAR_WANTS_HELP +50`), and tension is high. Human players get +20 (`DIPLOMACY_FACTION_PLAYER_JOIN`).
- Asking a democracy for a guarantee or alliance below the world-tension threshold fails; check the tooltip (`hoi4_diplomacy view`).

## 7. Factions

- Germany starts as leader of the Axis (`create_faction_from_template = faction_template_axis`, `add_to_faction = GER`). Rules: joining non-communist, `faction_peace_rule_conquest_focus`, `call_to_war_rule_independent_only`, leader change by manpower, dismissal when in offensive war. Goals: `faction_goal_one_germany`, `faction_goal_secure_european_continent`, `faction_goal_resource_control`.
- Poland and China do not start in a faction. Both create theirs through focus (`faction_template_miedzymorze_faction`; `faction_template_chinese_united_front`: rules china-only, non-fascist, goals control the coast / no Japanese in China).
- **Creation**: a country with a visible template (`common/factions/templates/generic_factions.txt`) can found it via diplomacy; the AI creates with factor 0.75 vs joining (`DIPLOMACY_CREATE_FACTION_FACTOR`). The fascist regional template needs a fascist government in Europe; `industrial_focus` is open to all. **No tool for faction creation exists** (only `invite_to_faction`).
- **Joining**: you must not be at war with a member, the leader must accept; rules are enforced from templates (neighbors only, no offensive war, same ideology...).
- **Leadership takeover**: costs 200 PP, needs manpower ratio >= 2 and factory ratio >= 1.5 vs the leader, 180-day cooldown (`ASSUME_FACTION_LEADERSHIP_*`). Do not trigger unless the AI leader is passive and failing.
- **Goals and initiative**: up to 1 short, 1 medium, 1 long-term goal; initiative generates 0.015/day scaled by manifest progress; replacing an unfinished goal costs 1 initiative; changing a rule costs 1; theater commanders unlock for 1; spymaster 1 (`NFactions`).
- Rules of play: join/own a faction before the war you plan; the faction decides who is called to war and who inherits peace-conference points. Never kick a strong member 3 months before a campaign.

## 8. Intelligence agency and operatives (La Resistance, installed)

- **Create the agency**: 30 days, uses 5 civilian factories (`NOperatives.AGENCY_CREATION_*`). Germany gets one automatically in the 1939 start (`create_intelligence_agency = yes`) but not in 1936.
- **Upgrades** (`intelligence_agency_upgrades`): each is 30 days and ties up 5-10 civilian factories while in progress (`modifiers_during_progress.civilian_factory_use`); one slot per branch at a time is a safe assumption **[UNSURE]**. Every 5 upgrades unlock a bonus operative slot (max +1 via upgrades).
- Recommended upgrade order:
  1. `upgrade_training_centers` (+1 operative slot, +25% recruit chance) - most important.
  2. `upgrade_form_department` (crypto, decryption +25, unlocks decryption boost).
  3. `upgrade_passive_defense` (defense 1.5/1.25/1/1) - only if enemies are infiltrating you.
  4. `upgrade_army_department` (+25% army intel, helps the war room), then `upgrade_decryption_boost`.
  5. `upgrade_blueprint_stealing` and `upgrade_invisible_ink` when you are tech-behind a major.
  6. Skip `upgrade_suicide_pills`, `upgrade_diplo_training`, `upgrade_psycho_warfare` unless using those missions.
- **Spymaster**: needs >= 3 upgrades and 50 PP (or 1 faction initiative with the faction DLC) (`BECOME_SPYMASTER_*`), 180-day cooldown.
- **Operative missions** (assigned per operative, not operations): build intel network (the default; +0.4 network/day per operative; detectable only after coverage thresholds; 15 strength offset per operative), quiet network, counter-intelligence (defense +1 base), root out resistance, boost ideology (up to 0.25 drift/day per operative; detection x3 so risky), control trade (cost -), diplomatic pressure, propaganda (0.0005 stab/war-support per day per operative; stacks 0.5; cap -0.2). 
- **Operations** (`common/operations/00_operations.txt`; need network strength in the target; slots for all):

| Operation | Days | Operatives | Risk | Network | Notes |
|---|---|---|---|---|---|
| infiltrate civilian / army / navy / air | 90 / 75 | 2 | 0.1 | 35 / 50 | grants a token that steal-tech needs |
| steal tech (civilian/army/navy/air) | 120 | 3 | 0.2 | 35 / 50 | needs the matching token; random tech bonus. **The best peacetime payoff when behind** |
| capture cipher | 75 | 2 | 0.2 | 40 | speeds decryption |
| fake intel | 60 | 2 | 0 | 40 | defensive, rarely worth it |
| collaboration government | 90 | 2 | 0.1 | 50 | needs 200 support equipment, 1000 infantry equipment and 10 civ factories for 60 days. Late-war only |
| coup government | 180 | 2 | 0.3 | 70 | needs 12000 infantry equipment; target stability <0.7 and 20% party; effectively unusable early |
| coordinated strike | 3 | 2 | 0.3 | 70 | starts a war; 15 civ factories for 5 days **[UNSURE: may count as an exploit]** |
| sabotage industry/infra/resources | 90 | 3 | 0.2 | 35 | in occupied lands with resistance |
| boost resistance / resistance contacts | 50 / 60 | 2 | 0.2 / 0.1 | 35 / 40 | only in enemy-occupied states |

Payoff ranking for a campaign: (1) build intel networks on the main enemy to unlock their army/division detail and operations;
(2) steal tech while 1-2 tech levels behind; (3) counter-intelligence if your agency defense is low and you lead a faction;
(4) boost ideology only on small targets in a real ideological contest (low payback otherwise).
FAIR rules allow all of these as ordinary UI actions; the coup/collaboration paths are legitimate but not a free win condition.

## 9. Civil wars, ideologies, and national paths

- Ideology popularity drifts weekly from party spirits/advisors/events; ruling party changes by election (if allowed), coup, or civil war (`NPolitics.REVOLTER_PARTY_POPULARITY 0.4`). A civil war starts when a party has >60% support and an event/decision fires. Stability <0.21 with unresolved crisis can also start one (`stability_events.txt`).
- **Decisions `political_decisions.txt`** let a player push an ideology civil war: `prepare_for_*_civil_war` (50 PP), `ignite_the_*_civil_war` (50 PP), plus `expand_civil_*_support`, `ban_*_party` (100 PP), `institute_press_censorship_*` (150 PP), `anti_*_raids` (120 PP). Treat civil-war ignition as not applicable to the FAIR showcases unless the focus tree demands it.
- **Germany (Anschluss)**: focus `GER_anschluss` (10 days). With Gotterdammerung installed it needs `fighting_army_strength_ratio` over Austria above `global.deployed_manpower_ratio_needed_for_anschluss`, initialised at **24x** and lowered weekly with Austrian neutrality popularity (`common/on_actions/12_wuw_on_actions.txt`, `AUS_anschluss_ratio_ticking_down`). Without that DLC the rule is >= 500,000 army manpower and >= 50k infantry equipment. Also `has_war = no`, not subject, no `rhineland_challenge_met`. On completion Austria gets event `germany.28` (accept = annexed with troops; refuse = Germany gets a `take_state_focus` wargoal on states 4, 152, 153, 848, 975, 976). Rewards on accepted path: +75 PP, +10% war support, +5% stability, +10 army XP.
- Germany Czechoslovakia paths: `GER_demand_sudetenland` needs army manpower > 725,000 (claims states 69, 972...), then `GER_first_vienna_award` / `GER_fate_of_czechoslovakia` (manpower > 750,000), or `GER_befriend_czechoslovakia` (mutually exclusive). The rest of Czech is a war or the `integrate` path.
- **China**: unification runs through the NCNS focus tree and decisions in `CHI_decisions.txt`; warlord integration decisions cost 150 PP each; `CHI_join_united_front` is 5 PP; xian incident 25 PP. Communists (PRC) and Japan interactions depend on `CHI_communist_infiltration` decisions (detect 100 PP). Do not assume old "Subjugate the Warlords" focuses exist; read the live focus list.
- **Greater German Reich** (`decisions/formable_nation_decisions.txt`, `form_greater_german_reich`, 25 PP): requires DLC **Waking the Tiger** (`allowed`). Waking the Tiger is in `integrated_dlc/dlc022_waking_the_tiger` (part of the base game), so the decision **exists** in this install. Conditions: not a subject, states 16 (Paris), 9 (Prague), 10 (Warsaw), 195 (Leningrad), 219 (Moscow), 217 (Stalingrad) controlled by Germany or its subjects, and every owned state fully controlled; visible only when fascist and (France capitulated or owns Paris or controls Moscow). Reward: war support +5%, stability +5%, cosmetic tag `GER_greater_german_reich`, Germania victory point. This is the Germany showcase end-state.
- **Intermarium / Commonwealth**: there are two things.
  - `form_commonwealth` (formable): POL or LIT, not a subject, controls LIT states 11 and 189 and Polish states 86, 87, 88, 90, 98, 10, 97, 92, 91, 89, 93, 94, 95, 96, 784, plus `core_compliance` over the other country > 80 (must be POL occupying LIT or vice versa). Cosmetic tag `PLC_UNIFIED`. The AI is told not to take it in historical focus mode.
  - Intermarium (Miedzymorze) faction: focus `POL_the_between_the_seas_concept` (10 days): not a subject, major or faction leader or unaligned, army manpower >= 700,000, plus conditional prerequisites on Baltic/Romanian/Morges alliance focuses. Reward: leaves any faction, grants `can_create_factions`, creates `faction_template_miedzymorze_faction`. Then invite via the `POL_miedzymorze_decisions` missions (10 PP each) and focuses such as `POL_romanian_alliance`, `POL_invite_sweden`, `POL_invite_norway`.
  - Note: `poland_dlc_*` text and `nsb_*` events exist; the Poland tree is gated on No Step Back or Poland: United and Ready. NSB is installed, so the tree works, but some `Poland: United and Ready` specific items (e.g. `POL_april_constitution_*`) won't appear.

## 10. Events and pop-ups

General rule when the agent must choose (`hoi4_popups`):
1. Prefer options with no permanent penalty, then options that spend no PP, then the option that matches the playbook goal.
2. Prefer "accept" for deals that raise stability/war support/PP or add factories; refuse offers that give away states, force a government, or break an NAP/guarantee (war support -0.2).
3. Pop-up events named with AI-ish `ai_chance` can be read: the file often indicates the preferred option (`ai_chance` weights).
4. Never answer a pop-up with a click that spends command power or military units without a plan.

Flagged specific events:
- `stability.21` (draft dodging): options "nothing", "arrest evaders (military crackdown)", "reform" chains; the crackdown ends in 60/40 success when stability >0.3 (`stability.31` success, `stability.32` riot). Reform paths end in `stability.34/37/40` (80% success). Prefer reform for lasting effect, crackdown only when stability >0.4 and you cannot afford the PP.
- `stability.22` (strikes) and `stability.23` (mutiny): same structure; choose the one that does not use the "military control of factories" if factory output matters.
- `germany.28` (Austria) is shown to the AI Austria; Germany sees the follow-up `germany.126` only in the Austrian civil-war branch.
- News and alert pop-ups (`news_event` type, `hidden = yes` events) require no answer.
- Anything offering a foreign power's puppet deal or "request to join" to a regime that conflicts with your faction rules should be refused unless the playbook lists it.

## (a) Automatable standing rules

1. Every game day: if PP >= reserve and a priority advisor is hireable, hire it (priority list per country above).
2. Every week: if stability <50% or war support <50% at war, run the healing ladder (section 2) within the reserve. Cap `improved_worker_conditions` usage to once per 540 days.
3. At each law gate: upgrade economy law (partial -> war_economy at WS > 0.5 and fascism/communism), conscription step-up when manpower <3 months of divisions; cost always 150 PP.
4. If an agency exists: queue upgrades in the order in section 8 when civ factory usage allows; keep operatives on build-intel-network for the primary enemy.
5. Answer pop-ups with the general rule (section 10); stop for the flagged exceptions.
6. Take cheap one-shot decisions (<= 50 PP) that unlock real assets or claims whenever PP > reserve + cost.

## (b) Needs live decisions

- Which wargoal target and when to justify (tension, readiness, timeline).
- Declaring war, accepting a peace deal, puppet vs annex.
- Faction creation (no tool), faction leadership change, kicking members.
- Civil war ignition or backing.
- Coup/collaboration operations (large equipment cost).
- Any pop-up that offers an irreversible concession.
- Choosing doctrine/spirit when two advisors share a slot.

## (c) Tools

Existing (verified in `hoi4_live/tools/*.py`): `hoi4_politics` (view, change_law, hire), `hoi4_decisions` (list, details, take), `hoi4_diplomacy` (view, act, trade; alias list includes improve_relations, guarantee, justify, invite_to_faction, volunteers, lend_lease, military_access, non_aggression_pact, attache, embargo, license, declare_war), `hoi4_popups` (list, read, answer, answer_all), `hoi4_focus` (list/progress/start), `hoi4_peace`, `hoi4_standing`.
The task text's names `hoi4_politics_options/choose`, `hoi4_decisions_list/take`, `hoi4_events/event_choose` map to those actions; I did not find tools with those exact names.

Gaps (checked `native/src` and `hoi4_live`; matches were only advisor slot reading in `native/src/launch/macos/politics.rs`, `hoi4_live/tools/politics_tools.py`):
- **Espionage/operatives/agency**: no tool to create an agency, queue an upgrade, assign an operative mission, or launch an operation. Needs a new `hoi4_intelligence` tool (UI automation of the Intelligence panel).
- **Faction creation, goals, rules, leadership change**: no tool (only `invite_to_faction`).
- **Advisor details**: `hoi4_politics view` shows traits and cost from tooltips; there is no direct data-file advisor catalog tool, so trait comparison across candidates relies on the tooltip OCR. The tables above can be loaded as static data.
- **Autonomy change/annex of a subject**: only through `hoi4_diplomacy` actions if the game exposes them; not specifically supported.
- **MIO management** (design team assignment, policies): not covered.
- **Decision popups for crisis chains** (`stability.21/22/23`): handled through `hoi4_popups answer`, not through the decision tool.
