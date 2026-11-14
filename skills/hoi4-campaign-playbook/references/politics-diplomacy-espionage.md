# Politics, diplomacy, factions and espionage

Active DLCs: Historical German Portraits, Poland: United and Ready, Rocket Launcher Unit Pack, La Resistance, No Step Back, By Blood Alone, Arms Against Tyranny, Götterdämmerung, No Compromise, No Surrender, Anniversary Pack; Man the Guns, Death or Dishonor, Together for Victory and Waking the Tiger are part of the base game. Many branches below depend on these; re-check the playset before day one.

## 1. Political power (PP)

| Fact | Value |
|---|---|
| Base daily PP | +2 a day; a running focus costs 1 a day |
| Cap / floor | +2000 / -500 |
| Stability over 50% | PP up to +10% at 100% |
| Stability under 50% | PP down to -20% at 0% |
| Difficulty | player PP +50/+25/0/-10/-15% from the easiest to the hardest level; Normal adds nothing |
| Leader and advisor traits | `political_power_factor` +5% to +20% (section 4); some give a flat +0.05 to +0.15 a day |
| MIO design team on a research slot | 0.1 PP a day each (production lines cost nothing) |

Net income: (2 + flat gains) x (1 + factors) - 1 while a focus runs, so about +1 a day plus factors. With two PP advisors expect about +1.6 to +2.0 a day, about 600-700 PP a year.

### What PP buys, in priority order (peacetime 1936-1939)
1. **Buys that compound**, ranked by payback (PO03): chief of army (army XP every day), a PP advisor, the trade and economy laws, a production advisor, a construction advisor only while building civilian factories.
2. **Law changes (150 PP each)** straight to the target law (PO04), when the effect beats the cost within about a year (section 3).
3. **Focus-linked and decision costs** that unlock real assets (fortification, factory slots, claims, formables).
4. **Diplomacy** when needed: guarantee 15 PP + 15 per guarantee held; improve relations 10 PP plus upkeep; attaché 100 PP + 50 command power; justification 0.2 PP a day base, cheaper at high world tension.
5. **Wartime stability and war-support decisions** (section 2) when a threshold is about to cost production.

### Banking rules
- Bank only for named, dated purchases (PO05): typically 150 PP for one law switch in a war-prep year, 300 PP at war (a law plus a crisis decision). Spend the rest on the highest-value item above.
- Never sit at the 2000 cap. Over about 300 PP in peacetime with nothing queued is a defect: buy the next advisor, law or a PP sink (PO06).
- Do not bank for a faction-leadership takeover (200 PP) unless the section 7 criteria are met.
- PP can go to -500; stay positive, since crisis events and many decisions need positive PP.

## 2. Stability and war support

Effects scale linearly between 50% and the extreme:

| State | Stability effect | War support effect |
|---|---|---|
| High (toward 100%) | factory and dockyard output +20%, consumer goods -20%, PP +10% | mobilisation speed +30%, attack and defence on cores +10%, command power gain +50% |
| Low (toward 0%) | factory and dockyard output **-50%**, PP -20% | mobilisation speed -50%, command power gain -95%, surrender limit -30% |
| At war | -20%, plus up to -30% more scaled by war support (check the stability tooltip) | |

Each 10 points of stability above 50 is about +4% factory output; each 10 below 50 costs about 10%.

Drivers: defensive war +20% war support, offensive war -20%; world tension scales war support by up to 0.4; bombing, convoy losses and casualties cut war support (caps -30%, -50%, -30%). Enemy propaganda operatives cap at -20% stability and -20% war support.

**Crisis events.** Only at war (not in a civil war) and while stability or war support is under 50%. Mean time 360 days, x0.75 under 40%, x0.75 again under 20%, doubled for non-democracies. Each adds `draft_dodging` (conscription -20/-30/-40%), `strikes_1..3` (factory output -20/-30/-50%, consumer goods +40/60/80%) or `mutiny_1..3` (attack -25/-50/-75%, org -10/-20/-50%, longer training). Suppressed under Disarmed Nation or Volunteer Only plus a civilian or isolation economy. A neglected level-3 crisis at stability under 21% can start a civil war. Answers arrive as free decisions in the `crisis` category and as events `stability.21/22/23` (section 10).

Targets:
- Stability **50%+** always, **60%+** before law and MIO optimisation, about 80% as the comfort target; under 40% is an emergency.
- War support **50%+** at war to avoid crises; mobilisation laws gate on it (section 3).
- Read both every game week at war (`hoi4_country_status`).

Healing decisions:

| Decision | PP | Effect | Gate / timing |
|---|---|---|---|
| `improved_worker_conditions` | 100 | about +0.005 stability a week for 180 days (+13-16 in total), consumer goods +10%, factory output -10%; re-enabled after 540 days | stability under 100%. Take it when stability is under 80% and war is more than 12 months away, at most twice |
| `war_propaganda` | 150 | +0.005 war support a week for 90 days; re-enabled after 180 | war support under 50%, only at war or under threat |
| `war_propaganda_casualties` | 100 | +0.006 a week against the casualty penalty, 180 days | casualty war support below 0 |
| `war_propaganda_radio_industry` | 75 | one-off +5% war support | radio tech and threat over 0.25; take it when it unlocks |
| `war_propaganda_film_industry` | 100 | one-off +10% war support | at war, a major or ally of one, threat over 0.3 (fascism) or 0.5 (others) |
| `war_bonds` | 50 (+25 per repeat) | consumer goods -20% for 180 days, small war support | at war and high war support (democratic or communist 0.8, fascist 0.9) or surrender progress |
| `desperate_defense` | 50 | defence +20%, dig-in +50% for 45 days | surrender progress over 50% |
| `emergency_factory_conversion_*` | 100 | civilian to military factories; war support -5% (offensive variant also stability -3%) | more than 5 civilian factories; 90-day cooldown. Once, when going to total war |
| `war_building_slot` | 50 | +1 shared slot in one state; war support and stability -2.5% | war only; repeat while stability is over 60% |
| `infrastructure_building_slot` | 100 | +1 shared slot in a state with no free slot | peacetime too; 30-day cooldown |

Do not buy a war-support-costing decision within 0.07 of 50% or of a law threshold you still need. Focus spirits (`why_we_fight_focus` +10% stability and war support; `indoctrination_focus` +5% stability) are worth it only on the path. Air superiority over home states protects war support from bombing.

## 3. Economy, trade and conscription laws

Every law costs **150 PP**; laws never require the previous law, so jump to the target (PO04). Discounts: advisor traits `economy_cost_factor`, `trade_laws_cost_factor`, `mobilization_laws_cost_factor` (Ludwig Erhard -33%, Shi Liang and Zhou Fohai -10%).

### Economy law
| Law | Gate | Key modifiers |
|---|---|---|
| civilian_economy | none | consumer goods 35%, military-factory construction -30%, conversion +30%, energy -30% |
| low_economic_mobilisation | WS over 0.15 | consumer goods 30%, military-factory construction -10%, energy -15% |
| partial_economic_mobilisation | WS over 0.25 | consumer goods 25%, military-factory construction +10%, conversion -10% |
| war_economy | WS over 0.5 and (fascist or communist, or at war with an enemy whose industry is over 40% of yours) | consumer goods 20%, military-factory construction +20%, conversion -20%, energy +25% |
| tot_economic_mobilisation | at war, WS over 0.8, an enemy whose industry is over 50% of yours | consumer goods 15%, military-factory construction +30%, conversion -30%, energy +50%, recruitable population -3% |
| GER `totaler_krieg_economy` | focus `GER_totaler_krieg`, war with a major, WS over 0.8, an enemy over 50% of German industry | consumer goods 15%, military-factory and dockyard construction +20%, factory and dockyard output +5%, conversion -30%, energy +50%, recruitable population -2% |

Consumer goods is the share of factories reserved for the civilian economy: lower is better.

Germany starts 1936 in `partial_economic_mobilisation`, `limited_conscription` and `limited_exports`. Take `war_economy` once WS is over 0.5 (the fascist gate is met by the government alone) unless it drops PP below a named reserve.

### Trade law
| Law | Gate | Effects |
|---|---|---|
| free_trade | every country; blocked for Germany after `GER_autarky_efforts` | factory and dockyard output +15%, construction +15%, research +10%, export 80% |
| export_focus (default) | every country except China under the Nine Power Treaty and Germany after autarky | factory and dockyard +10%, construction +10%, research +5%, export 50% |
| limited_exports | a democracy at war with an enemy whose industry is over 20% of yours, or a non-democracy in Partial Mobilisation or higher; not China under the Nine Power Treaty | factory and dockyard +5%, construction +5%, research +1%, export 25% |
| autarkic_economy | Germany after `GER_autarky_efforts` | construction +5%, research +5%, export 15% |
| closed_economy | at war, fascist or communist, War Economy or higher; not China under the Nine Power Treaty | export 0 |
| uncontrolled_exports | Poland's start law, cannot be chosen | export 25%, consumer goods +5%, agency upgrades +15% slower, no market access |

China under the Nine Power Treaty is locked on Free Trade. Practical rule: Free Trade is the 1936 default for any country whose resource balance allows it; the export share is reserved even with no buyer, so step down (Export Focus, then Limited Exports) when exports starve your own lines.

### Conscription law
| Law | Gate | Recruitable pop | Side cost |
|---|---|---|---|
| volunteer_only | none | 1.5% | none |
| limited_conscription | WS over 0.10 | 2.5% | none |
| extensive_conscription | WS over 0.2 and (fascist or communist, or at war with a strong enemy) | 5% | training +10% |
| service_by_requirement | (fascist or communist, or at war) and (WS over 0.6 or any surrender progress) | 10% | factory, dockyard and construction output -10%, training +20% |
| all_adults_serve | at war and (WS over 0.7 or any surrender progress) | 20% | factory, dockyard and construction output -30%, training +30% |
| scraping_the_barrel | at war and (WS over 0.85 or surrender progress over 25%) | 25% | factory, dockyard and construction output -40%, training +50% |

Step up when idle manpower is below about 3 months of planned divisions; Service by Requirement and above only when manpower, not equipment, is the bottleneck (PO14). Step down after the war. `women_in_the_workforce` (decision, 100 PP): +3% conscription, -5% stability under `tot_economic_mobilisation` (Germany: also `totaler_krieg_economy`); needs WS 0.8-0.9 by ideology.

## 4. Advisors by role

Advisors are named characters hired by idea token: `hoi4_politics_options` lists a slot's tokens with their enabled state, `hoi4_politics_choose` hires. Cost when none is given: 150 PP political, 50 military. Hiring is gated by focus, flag or government. Pre-1.19 generic advisor lists no longer exist.

Each advisor is hired from a slot of its own kind, addressed by `category` and `slot` index (the slot's position in the category). Base game: `government` 0 conscription law, 1 trade law, 2 economy law, 3-5 political advisors; `research_production` 0-3 designers (Military Industrial Organizations with Arms Against Tyranny), 4 industrial concern, 5 theorist; `military_staff` 0 chief of army, 1 chief of navy, 2 chief of air force, 3-5 high command. A mod can reorder them: `hoi4_politics_options` returns every slot of the category with its index, and `facts.py politics-slots` prints the layout from the files.

### Role priority
1. **Chief of army** (0.2/0.3/0.4 army XP a day by tier): one of the first three hires of any land power.
2. **Political slot: PP or stability** (Silent Workhorse; Popular Figurehead when stability is short).
3. **Political slot: production** (war industrialist); construction (captain of industry) only while building civilian factories.
4. **Third political slot:** wartime (resources, factory output) or intelligence.
5. **Theorist** (doctrine cost -10% or mastery gain +15%) when doctrine research starts.
6. Other high command as PP allows.

### Germany (no advisors hired at the 1936 start)
| Advisor (token) | Slot / PP | Effect | Gate |
|---|---|---|---|
| Hjalmar Schacht (`hjalmar_schacht`) | political / 75 | military-factory and dockyard construction +10%, consumer goods -10% | MEFO bills modifier active; not communist; gone with `GER_mefo_bills_ended` |
| Walther Funk (`walther_funk`) | political / default | consumer goods -10%, supply node speed +15%, free repair +15%, resource penalty -5% | fascism |
| Fritz Todt (`todt`) | political | industrial, infrastructure and rail construction +10% | flag `GER_reinstated_nazi_leadership` |
| Albert Speer (`speer`) | political | building speed +5% | same flag |
| Hermann Göring, political (`GER_hermann_goring`) | political | PP +5%, local resources +5% | fascism + `GER_ascension_of_goring` |
| Rudolf Hess (`rudolf_hess`) | political | PP +10%, opinion gain doubled | `GER_party_chancellor_hess` + flag |
| Martin Bormann (`martin_bormann`) | political | PP +10%, political advisor cost -10% | `GER_party_chancellor_bormann` + flag |
| Joseph Goebbels (`joseph_goebbels`) | political | war support +5%, fascism drift | flag + `GER_ascension_of_goebbels` |
| Joachim von Ribbentrop (`joachim_von_ribbentrop`) | political / 50 | justify time -15%, same-ideology opinion +5 a month | `GER_reorganize_the_wehrmacht` + fascism + flag |
| Ludwig Erhard (`GER_ludwig_erhard`) | political | **economy and trade law cost -33%**, consumer goods -15% | `GER_prioritize_economic_growth` |
| Wilhelm Canaris (`GER_wilhelm_canaris`) | political | +1 operative slot, agency upgrade time -20% | agency + `GER_reorganize_secret_services` (also a communist-path variant) |
| Guderian (`heinz_guderian`, 200), Manstein (`manstein`, 150) | theorist | mobile-warfare mastery gain +15% | Guderian needs `GER_adopt_new_panzer_doctrine` |
| Wever (`walther_wever`, 100), Udet (`ernst_udet`, 100) | theorist | air doctrine cost -10% | Udet needs `GER_dive_bombers` |
| Rundstedt (`gerd_von_rundstedt`, 100), Fritsch (100), Beck (`ludwig_beck`), Halder, Keitel, Kesselring (air), Raeder (navy, 100), Dönitz (navy, 200) | high command / chiefs | experience, planning, org | mostly open |

The 1939 start already has `walther_funk`, `GER_franz_seldte`, `ludwig_beck`, `gerd_von_rundstedt` and `goering`, `war_economy`, `extensive_conscription` and +1000 PP, so the hiring order matters only from 1936.

### Poland
| Advisor (token) | Slot / PP | Effect | Gate |
|---|---|---|---|
| Eugeniusz Kwiatkowski (`POL_eugeniusz_kwiatkowiski`) | political | industrial, infrastructure and rail construction +10% (captain of industry) | none |
| Henryk Floyar-Rajchman (`POL_henryk_floyar_rajchman`) | political | same, plus synthetic refinery +10% | `POL_the_sanation_right` |
| Józef Haller (`POL_jozef_haller`) | political | military-factory and dockyard construction +10%, fuel silo +10% (war industrialist) | none |
| Józef Beck (`POL_jozef_beck`) | political | PP +5%, improve-relations upkeep -50% | none |
| Ignacy Mościcki (`POL_ignacy_moscicki`) | political | PP +10%, neutrality drift | `POL_ozon` + `POL_dissolve_the_bbwr` |
| Stanisław Wojciechowski (`POL_stanislaw_wojciechowski`) | political | PP +15%, stability +5% | `POL_modus_vivendi` + `POL_reassemble_the_sejm` + `POL_assemble_the_regency_council` |
| Aleksander Kakowski (`POL_aleksander_kakowski`) | political | stability +15% | regency focus, neutrality or fascism |
| Jan Kowalewski (`POL_jan_kowalewski`) | political (intelligence) | +1 operative slot, agency upgrade time -15% | agency |
| Rydz-Śmigły (`POL_edward_rydz_smigly`, 100), Łupaszko (`POL_tomislaw_lupaszko`, 100) | theorist | land doctrine cost -10% | Rydz-Śmigły needs `POL_the_sanation_right` |

Poland order: Kwiatkowski, chief of army, Haller, Beck if relations or PP matter, Łupaszko when doctrine starts.

### China, Nationalist (many gated by the No Compromise, No Surrender tree)
| Advisor (token) | Slot / PP | Effect | Gate |
|---|---|---|---|
| Zhang Qun (`CHI_zhang_qun`) | political | **PP +0.15 a day** | none |
| Zhu Jiahua (`CHI_zhu_jiahua`) | political | infrastructure +10%, rail +5%, supply node speed +10% | none |
| Mei Siping (`CHI_mei_siping`) | political | industrial, infrastructure and rail construction +10% | after `CHI_look_for_allies` |
| Chen Lifu (`CHI_chen_lifu`) | political | research +6%, industrial construction +5% | after `CHI_end_military_primacy` |
| Wang Chonghui (`CHI_wang_chonghui`) | political | improve-relations upkeep -10%, PP +5% | none |
| Dai Jitao (`CHI_dai_jitao`) / Ju Zheng (`CHI_ju_zheng`), 100 | political | PP +0.05 a day and stability +5% / PP +0.1 a day | `CHI_revive_the_western_hills_group` |
| Mao Renfeng (`CHI_mao_renfeng`) | political (intelligence) | +1 operative, agency upgrade time -20%, intel network +20% | `CHI_empower_the_bureau_of_investigation_and_statistics` |
| Bo Yibo (`CHI_bo_yibo`), Jiang Baili (`CHI_jiang_baili`) | theorist, 100 | land doctrine -10% / -5% and defence +5% | open |

China order: Zhang Qun, chief of army, Zhu Jiahua or Mei Siping (infrastructure for the long rebuild), Chen Lifu. Avoid Du Yuesheng (stability -10%).

### Generic traits (other countries)
`captain_of_industry` (industrial, infrastructure and rail construction +10%), `war_industrialist` (military-factory and dockyard construction +10%), `silent_workhorse` (PP +15%), `popular_figurehead` (stability +15%), `backroom_backstabber` (justify time -10%, subversion upkeep -25%), `smooth_talking_charmer` (PP +5%, improve-relations upkeep -50%), `ideological_crusader` (party popularity to stability), `political_commissar` (communism, +30 command power cap). Chiefs (`army_chief_*`) give experience and org; the chief of army is an early hire.

### Designers and MIOs
Designers are Military Industrial Organisations (MIOs), not bought with PP.
- Funds: +500 x research cost when a matching design team finishes; +0.1 per IC a day from a production line (cap 100 a day); a size-up costs 700 funds scaled by level and gives 1 trait point.
- Policies: 25 PP, 180-day lock; some need size 6.
- A design team gives +5% research speed on matching techs and costs 0.1 PP a day.
- Assign a design team only to a tech you would research anyway; put the MIO with the best production traits on the biggest line (infantry equipment, then artillery and trucks). Review once a game quarter.

## 5. Decisions worth taking (by year)

List with `hoi4_decisions_list`, take with `hoi4_decision_take`.

**Every year**: `infrastructure_building_slot` (100, 30-day cooldown) where slots are the limit; `war_building_slot` at war; `restructure_supply_system` (once, free, supply-node speed x4 for 120 days) when hubs are the limit.

**Germany**:
- 1936-37: `GER_mefo_bills_mission` is a running mission; do not cancel it (`GER_cancel_mefos` is free but ends Schacht's bonus). `GER_seize_gold_reserves` (25 PP) when offered. Price-control decisions (`GER_guns_before_butter`, `GER_arbeit_und_brot`, `GER_collect_scrap_metal`, `GER_ersatz_material` 30 PP; `GER_import_tariffs`, `GER_implement_rations`, `GER_implement_wage_controls` 15 PP) open after `GER_the_four_year_plan` and `GER_institute_price_controls`: take every one except Gömbös (PO25).
- Pre-war: `GER_dismantle_czechoslovakian_forts` (50) after Czechoslovakia, `GER_dismantle_maginot` (50) after France.
- After conquest: `GER_reichskommissariat_*` (25 each) turn conquered land into compliant subjects; `GER_develop_puppet` / `GER_exploit_puppet` (120).
- Faction: `GER_invite_to_faction_decision` (25) brings minors in.

**China**: `CHI_army_reform` and `CHI_lessons_of_war` (0 PP) first; `CHI_60_divisions_plan` (90); `CHI_forced_loans` (100, PP-starved starts); warlord integration `CHI_integrate_the_army`, `CHI_integrate_the_administration`, `CHI_full_integration` (150 each); `CHI_flying_tigers`, `CHI_expand_the_burma_road`, `CHI_expand_the_ledo_road` (25 each) for supply; industrial evacuation (25) only when a province is about to fall.

**Poland**: `POL_miedzymorze_decisions` missions (10 PP each) improve ties with the Intermarium targets; `POL_defend_in_east_state` / `_west_state` (50) set defence plans; `POL_sideline_the_left` / `_right` (100) steer the Sanation split; `POL_unification_propaganda` (75) for Danzig.

## 6. Diplomacy

### Costs and numbers
| Action | Number |
|---|---|
| Guarantee | 15 PP + 15 per guarantee held; revoke 15; world tension -5 |
| Improve relations | 10 PP start; upkeep 0.2 PP (same ideology group) or 0.4 PP (different) per update |
| Send attaché | 100 PP + 50 command power |
| Justify war goal | 0.2 PP a day base, -50% at 100% world tension, -75% against a major already at war, at least 10 days; usually 30-60 days. Preview the exact days and cost with `hoi4_diplomacy` action `justify_war_goal` |
| War goal types | `take_state` (Conquer), `take_claimed_state`, `take_core_state`, `puppet_wargoal_focus`, `liberate_wargoal`, `annex_everything` (threat 0.5), `topple_government` (threat 0.5), `civil_war`; costs, threat and expiry in DI01 |
| Claims | focus-granted claims allow `take_claimed_state` |
| Tension | war with a war goal +10, without +14, joining a faction +4, annexing a claimed state +0.1, a core -0.5 |
| Non-aggression pact | lasts 48 months, unbreakable for 12; breaking costs 20% war support (same for breaking a guarantee) |
| Kick / rejoin faction | 90 days before a new member can be kicked; 60-day truce after a kick |
| Volunteers | need 30 divisions to send; cap and tension in DI07 |
| Subjects | autonomy tier change 50 PP, annex 300 PP |

### Ideology rules
Democracies justify only on threat countries, cannot puppet, cannot declare on another democracy, and are slow allies. Fascists, communists and neutrals may force governments, puppet and send volunteers. Joining a faction needs at least 30% popularity of the leader's ideology; in a defensive war the world-tension join gate drops (DI03).

### Puppets and annexation
- Autonomy levels: puppet (freedom 0.4, manpower share 0.9, the overlord can build there), satellite (0.8, manpower 0.4), integrated puppet, dominion (0.8, no manpower).
- Puppet through a peace conference or a puppet war goal. Autonomy rises with resources sent to the overlord, the puppet's war score and lend-lease from it; it falls with overlord construction and lend-lease to the puppet (DI16).
- Annex (300 PP) or through focus effects, only once the puppet's building slots are no longer needed.

### AI reactions (historical focus on)
- AIs follow their strategy plans and focus tree; a pending war goal does not make an AI drop a guarantee.
- Democratic AIs call allies readily, rarely declare offensive wars without a threat country, and give each other military access against a common enemy.
- Faction invites: a different ideology -60 (neutrality -50), +50 when the invitee is at war and wants help, +20 for a human inviter; high tension helps.
- A democracy refuses a guarantee or alliance below its world-tension gate (DI03); preview with `hoi4_diplomacy`.

## 7. Factions

- Germany starts as Axis leader. Rules: no communists, conquest-focused peace, only independent members are called to war, leadership by manpower, members in an offensive war can be dismissed. Goals: `faction_goal_one_germany`, `faction_goal_secure_european_continent`, `faction_goal_resource_control`.
- Poland and China start outside factions and found theirs by focus (`faction_template_miedzymorze_faction`; `faction_template_chinese_united_front`: China-only, non-fascist, goals control the coast and no Japanese in China).
- **Creation**: any country with a visible template can found it (`hoi4_faction_create`, plan `faction.create`). The fascist regional template needs a fascist government in Europe; `industrial_focus` is open to all. Compare manifests first (DI12).
- **Joining**: not at war with a member, the leader must accept, and the template rules apply (neighbours only, no offensive war, same ideology).
- **Leadership takeover**: 200 PP, 2x the leader's manpower, 1.5x its factories and 40% faction influence; 180-day cooldown. Only when the AI leader is passive and failing (DI14).
- **Goals and initiative**: one short, one medium and one long goal; initiative 0.015 a day scaled by manifest progress; replacing an unfinished goal, changing a rule, unlocking theatre commanders or becoming spymaster costs 1 initiative each.
- Join or found the faction before the planned war; it decides who is called to war and who shares the peace. Never kick a strong member within 3 months of a campaign.
- **Faction theaters**: a leader can open up to four, each with a field marshal or admiral as theater commander; the assigned ally gets -1% supply use per commander skill level in the chosen regions, and an AI ally commits more forces there.

## 8. Intelligence agency and operatives (La Resistance)

- **Agency**: 30 days and 5 civilian factories (`hoi4_agency_create`). Germany gets one automatically only in the 1939 start.
- **Upgrades**: 30 days each, one at a time, 5-10 civilian factories while in progress. Every 5 upgrades give one extra operative slot (once).
- Upgrade order:
  1. `upgrade_training_centers` (+1 operative slot; 10 civilian factories).
  2. Four cheap upgrades the plan uses, to reach 5 upgrades for the extra operative: `upgrade_army_department` (+25% army intel), `upgrade_passive_defense`, `upgrade_anti_partisan` (before an occupation), 5 civilian factories each, and `upgrade_form_department` (decryption +25, opens the crypto branch; 8 factories).
  3. `upgrade_decryption_boost` when decryption is planned; `upgrade_blueprint_stealing` and `upgrade_invisible_ink` only when steal tech is planned.
  4. Skip `upgrade_suicide_pills`, `upgrade_diplo_training`, `upgrade_psycho_warfare` unless the plan uses those missions.
- **Spymaster**: 3+ upgrades and 1 faction initiative (50 PP without the faction system), 180-day cooldown.
- **Operative missions** (`hoi4_operative_assign` keys): `build_intel_network` (the default; +0.4 network a day for the first operative, less for each extra: IN03), `quiet_intel_network`, `counter_intelligence` (IN04), `root_out_resistance` (IN08), `boost_ideology` (up to 0.25 drift a day per operative, detection x3, risky), `control_trade`, `diplomatic_pressure`, `propaganda` (IN05).
- **Operations** need network strength in the target:

| Operation | Days | Operatives | Risk | Network | Notes |
|---|---|---|---|---|---|
| infiltrate civilian / army / navy / air | 90 / 75 | 2 | 0.1 | 35 / 50 | grants the token steal tech needs |
| steal tech (civilian / army / navy / air) | 120 | 3 | 0.2 | 35 / 50 | needs the matching token; random tech bonus. Situational: only when 3+ techs behind in that branch |
| capture cipher | 75 | 2 | 0.2 | 40 | speeds decryption |
| fake intel | 60 | 2 | 0 | 40 | defensive, rarely worth it |
| collaboration government | 90 | 2 | 0.1 | 50 | 200 support equipment, 1000 infantry equipment, 10 civilian factories for 60 days. A pre-war operation for non-democracies: start the year before the war (IN10) |
| coup government | 180 | 2 | 0.3 | 70 | 12000 infantry equipment; target stability under 70% and 20% party support (DI11) |
| coordinated strike | 3 | 2 | 0.3 | 70 | needs a justified war goal; declares war on completion and the port-strike and strategic-bombing missions prepared in the target region strike at once; 15 civilian factories for 5 days |
| sabotage industry / infrastructure / resources | 90 | 3 | 0.2 | 35 | in occupied land with resistance |
| boost resistance / resistance contacts | 50 / 60 | 2 | 0.2 / 0.1 | 35 / 40 | only in enemy-occupied states |

Payoff ranking: (1) intel networks on the main enemy, in the states just behind its front before an offensive; (2) collaboration government on the main target from the year before the war (non-democracies); (3) counter-intelligence when your agency defence is low and you lead a faction; (4) steal tech only when 3+ techs behind; (5) boost ideology only on small targets in a real ideological contest. All are ordinary UI actions under FAIR; coups and collaboration are legitimate but not a free win.

## 9. Civil wars, ideologies and national paths

- Ideology popularity drifts weekly from party spirits, advisors and events; the ruling party changes by election, coup or civil war. A civil war starts when a party has over 60% support and an event or decision fires; stability under 21% with an unresolved crisis can also start one.
- **Political decisions** push an ideology civil war: `prepare_for_*_civil_war` (50 PP), `ignite_the_*_civil_war` (50 PP), plus `expand_civil_*_support`, `ban_*_party` (100 PP), `institute_press_censorship_*` (150 PP), `anti_*_raids` (120 PP). Not used in the FAIR showcases unless the focus tree demands it.
- **Germany (Anschluss)**: focus `GER_anschluss` (10 days). With Götterdämmerung it needs a fighting army strength over Austria above a ratio that starts at **24x** and falls weekly with Austrian neutrality popularity; without that DLC, 500,000+ army manpower and 50k+ infantry equipment. Also no war, not a subject, no `rhineland_challenge_met`. Austria gets event `germany.28` (accept: annexed with its troops; refuse: Germany gets a war goal on states 4, 152, 153, 848, 975, 976). Accepted path: +75 PP, +10% war support, +5% stability, +10 army XP.
- Germany and Czechoslovakia: `GER_demand_sudetenland` needs army manpower over 725,000 (claims states 69, 972 and others), then `GER_first_vienna_award` / `GER_fate_of_czechoslovakia` (over 750,000), or `GER_befriend_czechoslovakia` (exclusive). The rest of Czechoslovakia is a war or the integrate path.
- **China**: unification runs through the No Compromise, No Surrender tree and decisions; warlord integration 150 PP each; `CHI_join_united_front` 5 PP; Xi'an incident 25 PP; the `CHI_communist_infiltration` decisions (detect 100 PP) shape the PRC and Japan interactions. Old "Subjugate the Warlords" focuses are gone; read the live tree (`hoi4_focus_list`).
- **Greater German Reich** (`form_greater_german_reich`, 25 PP): not a subject; states 16 (Paris), 9 (Prague), 10 (Warsaw), 195 (Leningrad), 219 (Moscow), 217 (Stalingrad) held by Germany or its subjects and every owned state fully controlled; visible only when fascist and France capitulated, Paris owned or Moscow controlled. Reward: war support +5%, stability +5%, tag `GER_greater_german_reich`, Germania victory point. The Germany showcase end state.
- **Intermarium / Commonwealth**:
  - `form_commonwealth`: POL or LIT, not a subject, controls Lithuanian states 11 and 189 and Polish states 86, 87, 88, 90, 98, 10, 97, 92, 91, 89, 93, 94, 95, 96, 784, with compliance over 80 in the other country's core (POL occupying LIT or the reverse). Tag `PLC_UNIFIED`. The AI skips it in historical mode.
  - Intermarium faction: focus `POL_the_between_the_seas_concept` (10 days): not a subject, a major, faction leader or unaligned, army manpower 700,000+, plus the Baltic, Romanian or Morges alliance prerequisites. It leaves any faction, grants `can_create_factions` and creates `faction_template_miedzymorze_faction`. Then invite through the `POL_miedzymorze_decisions` missions (10 PP each) and focuses such as `POL_romanian_alliance`, `POL_invite_sweden`, `POL_invite_norway`.
  - The Poland tree needs No Step Back or Poland: United and Ready; both are active, so the full tree (including `POL_april_constitution_*`) exists.

## 10. Events and pop-ups

Read events with `hoi4_events`; answer by option name in the plan's `event_rules` (CP05). General rule:
1. Prefer options with no permanent penalty, then options that spend no PP, then the option that matches the plan.
2. Accept deals that raise stability, war support or PP or add factories; refuse offers that give away states, force a government or break a pact or guarantee (war support -20%).
3. Never pick an option that spends command power or units without a plan.

Specific events:
- `stability.21` (draft dodging): "nothing", "arrest evaders" or "reform" chains. The crackdown succeeds 60/40 when stability is over 0.3 (`stability.31` success, `stability.32` riot); reform ends in `stability.34/37/40` (80% success). Prefer reform; crackdown only when stability is over 0.4 and PP is short.
- `stability.22` (strikes) and `stability.23` (mutiny): same structure; avoid "military control of factories" when factory output matters.
- `germany.28` (Austria) goes to the AI Austria; Germany sees `germany.126` only in the Austrian civil-war branch.
- News events need no decision.
- Refuse foreign puppet deals or join requests that conflict with your faction rules unless the plan lists them.

## (a) Automatable standing rules

1. Every game day: if PP covers the named reserve and the next priority advisor is hireable, hire it (section 4).
2. Every week: if stability is under 50%, or war support under 50% at war, run the healing ladder (section 2) within the reserve. `improved_worker_conditions` at most twice, only while war is more than 12 months away.
3. At each law gate: go straight to the target law (section 3); conscription steps up when idle manpower is under 3 months of divisions; 150 PP each.
4. With an agency: queue upgrades in the section 8 order when civilian factories allow; operatives on `build_intel_network` in the main enemy's states behind its front.
5. Answer events by the section 10 rule; stop for the listed exceptions.
6. Take cheap one-shot decisions (50 PP or less) that unlock real assets or claims whenever PP exceeds reserve + cost.

## (b) Needs live decisions

- Which war goal target and when to justify (tension, readiness, timeline).
- Declaring war, accepting a peace deal, puppet or annex.
- Faction template choice, leadership change, kicking members.
- Civil war ignition or backing.
- Coup and collaboration operations (large equipment cost).
- Any pop-up that offers an irreversible concession.
- Two advisors competing for one slot.

## (c) Tools

| Domain | Tools | Plan key |
|---|---|---|
| PP, stability, war support, world tension | `hoi4_country_status`, `hoi4_economy` (exact PP) | `alarms` |
| Laws, advisors, designers | `hoi4_politics_options` (slot ideas with enabled state, every slot of the category with its index), `hoi4_politics_choose` | `politics` |
| Decisions | `hoi4_decisions_list`, `hoi4_decision_take` (`target` for targeted rows) | `decision_rules`, `decisions` |
| Focuses | `hoi4_focus_list`, `hoi4_focus_available`, `hoi4_focus_start`, `hoi4_focus_completed` | `focus_path` |
| Events | `hoi4_events` (full text and options), `hoi4_event_choose` | `event_rules`, `auto_events` |
| Diplomacy | `hoi4_diplomacy` (offered actions, cost, AI refusal, justification preview), `hoi4_diplomacy_act`, `hoi4_diplomacy_countries`, `hoi4_diplomacy_state`, `hoi4_diplomacy_incoming`, `hoi4_war_risk` (who joins a war), `hoi4_wars` | `diplomacy`, `war` |
| Factions | `hoi4_faction_create`, `hoi4_faction_invite_accept`, `hoi4_faction_leave`; invite, join and kick through `hoi4_diplomacy_act` | `faction` |
| Intelligence | `hoi4_intelligence` (upgrade, mission and operation catalogue), `hoi4_agency_create`, `hoi4_agency_upgrade`, `hoi4_operative_recruit`, `hoi4_operative_assign` | `intelligence` |
| Occupation | `hoi4_occupation` (resistance and compliance per state), `hoi4_occupation_law` (default law) | `occupation` |
| MIOs | `hoi4_mios`, `hoi4_mio_trait`, `hoi4_mio_policy`, `hoi4_mio_design_team` | `mio` |
| Peace | `hoi4_peace_options`, `hoi4_peace_take`, `hoi4_peace_submit`, `hoi4_peace_exit`, `hoi4_war_goal_targets` | `peace` |
| Trade | `hoi4_trade`, `hoi4_trade_set`, `hoi4_auto_import` | `trade`, `auto_import` |

Gaps:
- Operations (collaboration government, coup, steal tech, capture cipher, coordinated strike) and the `boost_ideology` mission: `hoi4_operative_assign` refuses them.
- Decryption target and cipher activation.
- Licence, lend-lease and volunteer dialogs (`hoi4_diplomacy_act` returns `dialog_unsupported`).
- Faction goals, rules, initiative and influence: no read or set.
- Per-state occupation law (only the default law is set); the ideology laws (autonomous occupation, liberated workers, Italian colonial police) are not in `hoi4_occupation_law`'s list.
- Garrison template assignment (the plan's `garrison_template` has no action yet).
- Reads: balance of power, enemy industry ratio and surrender progress (law gates), agency upgrade levels, operatives and network strength, MIO size, funds and traits, foreign opinion and justification timers.

## 11. Tactics: political power, laws, stability and war support

| ID | Tactic | How it works | When | Mode |
|---|---|---|---|---|
| PO01 | Net PP | A running focus costs 1 PP a day: net PP is about (2 + gains) x (1 + factors) - 1, about half the gross figure; difficulty adds +50/+25/0/-10/-15% for the player | Every PP plan | FAIR |
| PO02 | Short focus gaps are free | Up to 10 days of focus progress are kept and the 1 PP a day stops; a gap is allowed only to hit a PP-critical date | Law or crisis decision just out of reach | FAIR |
| PO03 | Compounding buys first | Rank buys by payback days: chief of army, Silent Workhorse, trade and economy laws, war industrialist, financial expert, captain of industry only while building civilian factories, then Improve Worker Conditions | 1936-37 | FAIR |
| PO04 | Jump to the target law | Law gates check war support, government and war, never the previous law: go straight to the target (USSR: Civilian Economy to War Economy at 50% war support); saves 150-300 PP | Every law change | FAIR |
| PO05 | Reserve for named buys | Bank only for a dated purchase: 150 PP for the war-economy switch of a democracy at war; France banks 300 for Extensive Conscription and War Economy on war day | 3-6 months before a war | FAIR |
| PO06 | Late PP sinks | Formables and core decisions, slot decisions, annexing puppets (300), repeat Improve Worker Conditions or War Propaganda | PP above 300 with nothing queued | FAIR |
| PO07 | Chief of army early | 0.2/0.3/0.4 army XP a day by tier; one of the first three hires of any land power | Day 1 to month 1 | FAIR |
| PO08 | Advisor payback test | Hire when cost / daily gain is under half the horizon; Silent Workhorse (+0.3 PP a day for 150) pays back in about 500 days; construction advisors are diluted by other bonuses but positive while building | Advisor planning | FAIR |
| PO09 | Popular Figurehead | +15% stability at once: the stability fix | Stability under about 70% or war start | FAIR |
| PO10 | Officer-corps spirits as discounts | Cheap army, air and navy spirits cut later costs (army advisors -50%, air chief -75%, conscription laws -25%); buy them before the big hire or law | Before big hires | FAIR |
| PO11 | Grow your own high command | A commander at skill 4/6/8 can become a specialist, expert or genius advisor | Hire list lacks a role | FAIR |
| PO12 | Economy-law gates | War Economy: war support over 50% and fascist or communist, or at war with an enemy whose industry is over 40% of yours; Total Mobilisation: at war, war support over 80% and an enemy over 50% of yours; Total Mobilisation also costs 3% recruitable population (Women in the Workforce offsets it) | Law planning | FAIR |
| PO13 | Conscription gates | Extensive: war support over 20% and (fascist or communist, or at war with a strong enemy); Service by Requirement: war support over 60% or any surrender progress; All Adults Serve: at war and war support over 70% or any surrender progress; Scraping the Barrel: war support over 85% or surrender over 25%. A defender that has lost a VP unlocks the top laws at low war support | Defensive wars | FAIR |
| PO14 | No conscription law the plan does not need | Service by Requirement -10%, All Adults Serve -30%, Scraping the Barrel -40% factory, dockyard and construction output; take one only when manpower, not equipment, blocks training or the front is under 80% manpower | Every war | FAIR |
| PO15 | War-only laws before the peace | Closed Economy and National Defence State need a war; switch to them before the war ends | Japan in China, short wars | FAIR |
| PO16 | Stability is factory output | Above 50% stability scales to +20% factory and dockyard output and +10% PP at 100%; below 50% to -50% output and -20% PP at 0%: each 10 points above 50 is +4% output | Always | FAIR |
| PO17 | War support above 50% | Above 50%: +10% attack and defence on cores, +50% command power gain; below: -95% command power gain and a 30% lower surrender limit | Every war, defenders first | FAIR |
| PO18 | Improve Worker Conditions early | 100 PP, about +13-16 base stability over 180 days, -10% output while active, re-enabled after 540 days; take it when stability is under 80% and war is more than 12 months away, at most twice | 1936-37 | FAIR |
| PO19 | Attaché for war support | An attaché to the Spanish Civil War or the Sino-Japanese War gives about +10% war support and army XP; improve relations to opinion 20 first | Democracies and neutrals short of 25% or 50% | FAIR |
| PO20 | Base stability cap | Base stability caps at 100 and modifiers above it are a buffer: at the cap take event options that cost base stability rather than PP | Event choices | FAIR |
| PO21 | War Propaganda | 150 PP for 90 days; only while war support is under 50% at war or under threat | War support short | FAIR |
| PO22 | No bans for stability | Party bans, raids and censorship give little stability for their cost | Never for stability | FAIR |
| PO23 | Timer focuses first | Take first the focuses whose payoff starts a timer or arrives later (France Laissez-Faire, China Extend Compulsory Education, Japan Guide the Zaibatsus) and fill locked gaps | Focus order | FAIR |
| PO24 | Mission gates with real divisions | Army-size gates (Japan 750k, Italy 750k, Romania 400k-750k, Germany's Anschluss ratio) are met with trained divisions; one-battalion spam to pass them is META | Mission timers | FAIR |
| PO25 | Cheap one-shot decisions | Germany: the DNSAP event's 50 PP option (reliable Anschluss), every price-control decision except Gömbös, the 50 PP regional development decisions (open after the Reich labour service focus, either chancellor), gold seizures after each capitulation; USSR: High-Yield Agitprop, Spanish gold (25); Ethiopia: the 10 PP recall decision (an instant equipped unit) | When enabled | FAIR |
| PO26 | Known event traps | Answer by option name: USSR refuse Yezhov; USA Panay "Demand Compensation" (free war support); France take the 1 May 1936 election (free Early Mobilization) | Event pop-ups | FAIR |
| PO27 | Balance of power | Only DEN, ETH, FIN, ITA, PRC, SWE and SWI have an active one here (Brazil's and Czechoslovakia's need focus-tree DLCs this install lacks). Italy: keep the bar on the Mussolini side at 0.6+, avoid the Grand Council extremes; Ethiopia: Mesafint power (-0.35 to -0.65) during the war for war support, command power and local combat bonuses; Sweden: the centre gives +10% stability, the right side speeds military factories; PRC: avoid the unstable middle | BoP tags | FAIR |
| PO28 | Party popularity is stability | Ruling-party popularity adds up to +15% stability; war and laws cost stability, so keep popularity up before a stability-gated law | Stability planning | FAIR |
| PO29 | Intelligence advisor operative | Any intelligence advisor gives one extra operative slot without a faction; hire one when the plan uses collaboration or intel operations | Espionage plans | FAIR |
| PO30 | Total Mobilisation costs manpower | Total Mobilisation lowers recruitable population by 3%; on a low conscription law pair it with Women in the Workforce | Law steps | FAIR |

## 12. Tactics: diplomacy, factions and puppets

| ID | Tactic | How it works | When | Mode |
|---|---|---|---|---|
| DI01 | Claims and cores, not Conquer | Conquer costs 200 + 50 per state, threat 2, and expires 60 days after completion; claimed (125 + 10 per state, threat 0.15) and core goals (125, threat 0.1) never expire and make the peace cheaper | Every war plan | FAIR |
| DI02 | One justification at a time | Each nation already at war with you or being justified against makes the next goal 1.5x dearer; -50% at 100% world tension; -75% against a major already at war; fascists justify 80% faster during a major war; goals on a target that sent lend-lease (-25%), docking rights (-20%) or volunteers (-50%) against you are cheaper | Chained wars | FAIR |
| DI03 | World-tension gates by ideology | Democracy: justify only on threat countries, join a faction at 80% (50% defending), lend-lease 50%, volunteers 50%, guarantee 25%. Neutral: justify 50%, join 40%, lend-lease 60%, volunteers 40%, guarantee 40%. Isolation laws add +100% to the justify gate and +75% to guarantees | Democracies, neutrals, USA | FAIR |
| DI04 | Before 25% world tension | Democracies cannot guarantee below 25% tension; list who can guarantee a target and at what tension before declaring | 1936-38 expansion | FAIR |
| DI05 | Guarantees | A guarantee costs 15 + 15 per guarantee held and lowers tension by 5; breaking one costs 20% war support | Democracies | FAIR |
| DI06 | NAP timing | A pact lasts 48 months, is unbreakable for 12; then breaking needs border force 2:1, later 1:1, then 1:2 in 6-month steps; breaking costs 20% war support | SOV, GER, JAP pacts | FAIR |
| DI07 | Volunteers cap | Cap 0.05 per target-owned province plus 0.05 per own division; air volunteers 20 per 100 planes, max 20 per air-base level in the recipient; +0.2 tension per division | 1936-39 | FAIR |
| DI08 | Do not call AI allies | AI allies take provinces and peace score; fight one war at a time and declare without calling allies where the UI allows it | GER, ITA, JAP wars | FAIR |
| DI09 | Lend-lease and war score | Sender +0.003 war score per IC, receiver -0.002; faction influence grows with lend-lease | Coalition wars | FAIR |
| DI10 | Embargo and blockade | Embargo 100 PP, target threat 30+; naval blockade 50 PP plus 0.1 a day, threat 15+, and only with sea control | Pre-war pressure | FAIR |
| DI11 | Coups | 180 days, 12000 infantry equipment, 70% network; the target needs stability under 70% (minor) or 50% (major) and about 20% of your ideology | Subverting a guaranteed minor | FAIR |
| DI12 | Faction founding | As founder, compare manifests and pick one you can fulfil: fulfilment drives initiative (about 1 per 83 days at 80%) | POL, CHI, minors founding | FAIR |
| DI13 | Faction goals and rules | Leave goal slots empty until a goal is nearly done (replacing an unfinished one costs 1 initiative); first rule change: the leader decides goals; set the peace rule before the conference (encourage puppet x0.8, take states x0.9, claimed states x0.8); joint goals pay about 50 army XP per member | Faction leader | FAIR |
| DI14 | Faction leadership | A takeover costs 200 PP, needs 2x the leader's manpower, 1.5x its factories and 40% faction influence; spending influence (China's United Front) can hand the lead to a warlord | CHI, POL, ITA in the Axis | FAIR |
| DI15 | Faction peace discount | The faction whose power projection beats the next contester by 50% gets up to -25% peace costs | Multi-faction wars | FAIR |
| DI16 | Lower autonomy before annexing | Building in the puppet lowers autonomy (0.7 x construction), lend-lease of better gear lowers it daily, an attaché by 0.05; resources sent to you and war score raise it; tier change 50 PP, annex 300 | Puppet holders | FAIR |
| DI17 | Puppets by focus | Focus-driven devolution and reintegration (Yugoslavia's ten kingdoms, Ethiopia's Federal Empire) is ordinary play; release-and-annex spam is META | YUG, ETH | FAIR |
| DI18 | Supervised states | 1.19 democratic puppets may refuse a call to arms | Democracies | FAIR |
| DI19 | Faction goal order | Short: Joint Command Structure, Appoint a Spymaster, Develop Joint Research Practices (then every member puts scientists into the leader's land and air facilities and a second member's facility), Build Stronger Fortifications (30 bunker and coastal-bunker levels in the faction or level-2 fort techs); medium: Industrial Expansion (150 civilian factories faction-wide; members with 50+ get 2 x 50% industry), then the excavation goal; Arteries of Trade for the UK. Finish two annexations of a goal's targets shortly before the third to swap a mid-term goal without spending an initiative | Faction members and leaders | FAIR |
| DI20 | Spymaster as a relay | The leader claims spymaster for the faction goal and hands it to a member later (Germany to Bulgaria, UK to the USA); a member with the role gets the decryption and agency bonuses | Faction play | FAIR |
| DI21 | Subjects cannot stay out | Puppets, colonies, satellites, integrated puppets and reichskommissariats join their overlord's wars automatically; only dominions, supervised states and a few DLC subject types may stay out; a war on a major includes its non-dominion subjects | Choosing targets and buffers: an own puppet is never a neutral buffer | FAIR |
| DI22 | Volunteer recall timing | Recall takes 2 days per division; none in the first 180 days, no return to the same country for 180 days after a recall; 95% of equipment comes back | Recall while a port link exists, never with a deployed HQ among them (MX41) | FAIR |

## 13. Tactics: intelligence

| ID | Tactic | How it works | When | Mode |
|---|---|---|---|---|
| IN01 | Agency with a plan | 30 days and 5 civilian factories; founding at the start is enough for Germany to finish collaboration on France and part of Poland by 1939; economy-first minors wait | GER, JAP, SOV, ITA when operations are planned | FAIR |
| IN02 | Five upgrades fast | Every 5 upgrades +1 operative (once); training centres first (+1 slot), then four cheap upgrades the plan uses (5 civilian factories each, form department 8): army department, passive defence, form department, anti-partisan; spymaster needs 3 upgrades and 1 faction initiative | Agency owners | FAIR |
| IN03 | Networks behind the front | Several network states just behind the enemy front, 3-6 months before an offensive: enemy planning -100% there, -5 entrenchment, own planning up; extra operatives add 0.4/0.2/0.1/0 network a day | Offence and defence | FAIR |
| IN04 | Counter-intelligence | At most 2 operatives (stacking 1/0.5/0.25) | Majors facing spies | FAIR |
| IN05 | Propaganda | One operative per target (each more x0.5), up to -20% stability and war support | Before war with a major | FAIR |
| IN06 | Decryption timing | One target early; decryption costs 12000 + 4250 x the target's cryptology level and runs at half speed against nations at peace; keep own encryption upgrades until just before the war (each resets enemy progress); activate a cracked cipher for the main offensive: 30 days of +15% breakthrough and defence and +50% planning | Majors before a war | FAIR |
| IN07 | Steal tech only when far behind | Infiltrate 90 days, steal 120 days, 3 operatives; worth it only 3+ techs behind in a branch | Behind a major | FAIR |
| IN08 | Root out resistance | An operative cuts a state's resistance target by 10 (5 on neighbours); try it before a harsher law | Garrisons draining equipment | FAIR |
| IN09 | Spies survive | Since 1.18 operatives hide for 28 days instead of being captured as often; skip suicide pills and rescues | All | FAIR |
| IN10 | Collaboration before the war | 90 days, 2 operatives, 50% network, 200 support and 1000 infantry equipment and 10 civilian factories for 60 days; each run +30%; collaboration lowers the surrender limit and becomes compliance at capitulation; stop near 75%; keep a third operative on a quiet network. Non-democracies only. Germany: France and Poland first, then Belgium; Japan: China before it capitulates | Year before the war | FAIR |
| IN11 | Agency against a strong opponent | Defence and decryption before operative slots: Form Department, Passive Defence to max, Radio Interception x2, then Invisible Ink and Suicide Pills, Army Department; advisors that add an agency slot go into counter-intelligence and cryptology; decrypt the main enemy and its air leader | Multiplayer-strength enemies, majors | FAIR |
| IN12 | Collaboration and infiltration targets | Germany: France 3, Poland 3, Belgium, Netherlands, Denmark, Norway 2 each (never the USSR in FAIR); Japan: China 3 before it capitulates, then the Raj; USSR: Latvia, Estonia, then networks, army infiltration and decryption on Germany, Hungary, Romania; UK and USA: infiltrate the Axis armies (GER, ITA, BUL, HUN, ROM, FIN) and decrypt them before a landing | Year before each war | FAIR |

## 14. Tactics: occupation

| ID | Tactic | How it works | When | Mode |
|---|---|---|---|---|
| OC01 | Law per state | Harsh Quotas on factory states (factories +25%), Forced Labour on resource states (resources +40%), Military Governor on populous states (+8% manpower), Martial Law or Local Police elsewhere; in France selective quotas gave 44 of 49 possible extra factory points | Large conquests | FAIR |
| OC02 | Ideology laws | Democracy: autonomous occupation (garrison -40%); communism: liberated workers (factories +20%, resources +30%); Italy: colonial police, three tiers (resistance target up to -45, garrison -40%, factories and resources +25%) | ITA in Ethiopia, democracies, communists | FAIR |
| OC03 | Keep the resistance target at 25 or less | Thresholds: 25 organised, 50 garrison damage x2, 75 -50% supply and speed, 90 rebellion. Base target 35, -10 at peace, +10 once the enemy capitulated, -5 with a claim, -0.5 per compliance point. Step one law harsher when a state's target reaches 25 | Always | FAIR |
| OC04 | No Civilian Oversight at war | Wartime Civilian Oversight sat near 63 resistance against 23 for Harsh Quotas; go milder only at peace: Local Police, then Civilian Oversight at about 20% compliance | Conquests | FAIR |
| OC05 | Compliance needs a mild law | Base compliance gain 0.075 a day; Harsh Quotas and Forced Labour (-0.08) and Brutal (-0.11) never build compliance; states you keep long get milder laws early | Long holds | FAIR |
| OC06 | Accept the collaboration government | At 80% compliance the collaboration-government event makes a puppet that passes 75% of its factories; accept unless the plan needs that land's dockyards | After capitulation | FAIR |
| OC07 | Equipment emergency | A milder law for a while frees garrison equipment (Local Police -35% garrison) | Front-line shortage | FAIR |
| OC08 | Plunder to the floor | Occupation laws and gold seizures keep consumer goods at the 10% floor (Germany) | Conquerors | FAIR |
