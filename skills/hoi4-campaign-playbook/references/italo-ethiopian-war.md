# The Second Italo-Ethiopian War (1936 start, By Blood Alone) — mechanics reference

Scope: Normal, historical AI, FAIR. Active DLC: Anniversary Pack, Arms Against Tyranny, **By Blood Alone**,
Gotterdammerung, Historical German Portraits, La Resistance, No Compromise No Surrender, No Step Back, Poland: United
and Ready, Rocket Launcher Unit Pack. Integrated, always active: Together for Victory, Death or Dishonor, Waking the
Tiger, **Man the Guns**. Everything below is the By Blood Alone version of the war; the pre-BBA content is dead.

One focus point is 7 days, so cost 5 = 35 days, cost 10 = 70 days. The game calendar has 365 days (no 29 February);
all dates below use it.

## 0. The ten facts that decide this war

1. The war is already on at 1 January 1936: Italy's history declares `annex_everything` on ETH when the
   start date is before 1936.01.02. The global flag `second_italo_ethiopian_war_flag` is set at start (its
   history clear dated 1936.05.05 never runs in a 1936 game).
2. Ethiopia capitulates only by victory points: base surrender limit 0.8. Ethiopia's own
   10 states hold **32 VP**, so Italy needs **26 VP** (80%), or **21 VP** once Ethiopia has the hidden
   spirit "Demoralized High Command" (`surrender_limit = -0.15`, 65%). Section 3.
3. Italy only counts as the winner if, at the moment the war ends, Italy (or its subject) controls state
   **271 Shewa** (Addis Ababa). Otherwise Italy gets `ITA_defeat_in_ethiopia_flag` even after a
   capitulation (checked at capitulation and at peace).
4. The "Christmas offensive" is a 14-day Ethiopian bonus (+10% attack vs Italy, +10% breakthrough),
   ended by `ETH_christmas_offensive_mission` about **15 January**, which then fires
   Mussolini's event `BBA_italo_ethiopian_war_events.01`. Expect Ethiopia to use the bonus against Eritrea at
   once. Unguarded, every Eritrean province can be Ethiopian by mid-January and Italy's army starves.
5. Event .01 starts two Italian missions and two Ethiopian counter-missions with a timer of **120 days**
   (option a, to about 15 May) or **180 days** (option b, to about 14 July):
   North = control 2 of Gondar 1984, Macalle 2072, Dessie 5097; South = control 2 of Wal Wal 12812,
   Gaba 10775, Moyale 13247. The first mission that fails forces the focus
   "The Abyssinian Fiasco" or "Struggle in Ethiopia". Section 4.
6. A second Mussolini mission "Conquer Ethiopia" is scheduled from history about **5-10 July 1936**
   (185 days plus 0-5 random); it needs the war ENDED without Italian defeat within
   **170 days** (deadline about 22-27 December 1936).
7. War escalation is a counter (`ETH.ETH_war_escalation_level`) that rises with time and with every
   Ethiopian "hold" mission Italy fails. At >4 Italy's African army gets -15% defence against Ethiopia,
   at >7 every Italian division in East Africa loses 35% of its strength.
   Section 5.
8. Ethiopian states have **no infrastructure** except Shewa 271 (2) and Afar 908 (1). Only four supply hubs
   exist: Asmara 13250, Addis Ababa 5010, Dire Dawa 7980, Djibouti 8124 (French). Railways: Djibouti-Dire Dawa-Addis
   and Massaua-Asmara. Section 7.
9. Ethiopia wins outright with the decision **ETH_defeat_italy** ("Defeat the Colonizers") once it fully
   controls Eritrea 550, Somaliland 559 and Jubaland 844 while Italy is in no faction; Italy's AI always
   accepts (ai_chance 10 vs 0) and hands over the three states with a white peace. Section 6.
10. Aussa (tag AFA, state 908) is a separate neutral country between Eritrea and Addis. Once Italy fully
    controls an Ethiopian core state touching 908 (Tigray, Wello, Hararghe or Shewa), Italy gets an event
    to demand its subjugation; Aussa's AI almost always folds and Italy annexes 908 with two free bands.
    Section 8.

## 1. Calendar of scripted events (both sides)

Missions activate on the first daily check, so a date can shift by a day.

| Date (365-day calendar) | What fires |
|---|---|
| 1936-01-01 | War ITA-ETH (annex_everything). ETH has spirits Conservative High Command, Heroes of Adwa, Chitet law, Christmas Offensive, On Our Own; war support 30%; OOB `ETH_1936_bba` |
| 1936-01-01 | `ETH_christmas_offensive_mission` (14 days) and `ETH_time_based_war_escalation_mission` (123 days) activate on `has_war = yes` + war flag |
| ~1936-01-15 | Christmas offensive idea removed; Mussolini unit-leader event `BBA_italo_ethiopian_war_events.01` "End the Christmas Offensive" 2-8 hours later |
| ~1936-01-15 | Option a "By any means necessary": missions 120 days, ITA_offensive_in_ethiopia (+15% attack vs ETH, 180 days), ETH_italian_offensive_idea (+75% to Italy's air-superiority bonus against Ethiopia, 180 days), escalation +1. Option b "We need to be methodical and deliberate": missions 180 days, ITA_logistics_in_ethiopia (-15% heat attrition, -5% supply consumption, +10% org regain, 240 days), no escalation |
| ~1936-02-14 | Mussolini's 30-day "activated recently" flag from event .01 expires |
| ~1936-05-04 | First time-based escalation (+1); the timer then becomes 170 days: ~10-21, ~1937-04-09, ~1937-09-26 ... while level < 10 |
| ~1936-05-15 (a) / ~07-14 (b) | North/South missions time out. Ethiopia +1 escalation for each mission it held |
| 1936-07-05..10 | `BBA_italy_missiolinis.11` activates `ITA_missiolini_conquer_ethiopia` if the war flag is still set and Mussolini's cooldown flag is clear |
| ~1936-12-22..27 | "Conquer Ethiopia" deadline: success = war ended and no defeat flag (+50 PP, +5% stability, +5% war support, 25 army XP, 25 CP); failure = -25 PP, -5% war support, -5% stability, -5 compliance in occupied ETH cores |
| from 1937-01-01 | Italy may take decision `ITA_withdraw_from_ethiopia` (15 PP, once); its AI weight is 0 before 1937-03-01 |

## 2. How the war can end

| Way | Trigger | Result |
|---|---|---|
| Ethiopia capitulates | surrender progress >= limit (section 3) | peace conference; `ITA_end_the_italo_ethiopian_war`: victory flag if Italy or a subject controls state 271, else **defeat flag** |
| Ethiopia's decision `ETH_defeat_italy` | ETH has full control of 550, 559, 844, at war with ITA, ITA in no faction | event `ETH_events.1` to Italy: a = ETH gets 550, 559, 844, white peace, Italy -25% fascism, -10% stability, completes Struggle in Ethiopia; AI ai_chance a 10, b 0 |
| Italy's decision `ITA_withdraw_from_ethiopia` | date > 1937-01-01 OR Italy lacks full control of 550, 559 or 844; 15 PP; once | event `BBA_italy_second_italo_ethiopian_war.1` to ETH: accept = ETH gets each of 550/559/844 it fully controls, plus Afar 908 if Italian-owned, white peace. AI ETH never accepts if stronger than Italy |
| Ethiopian focus `ETH_fait_accompli` | all ETH-owned states controlled by ETH or ally, Italy not in civil war | event .120 to Italy: a = ETH gets every Italian African state ETH controls, white peace (AI a 10 x modifiers vs b 2); b = Italy -25% war support, -20% stability, -250 PP, -20% ruling popularity, escalation +1 |
| Ethiopian focus `ETH_international_mediation` (exile branch) | escalation > 6, ETH controls > 4 states, LoN active | event .110 to Italy: a = white peace; b = -5% war support, -100 PP, Italy leaves the LoN and every democratic LoN major gets event .113 (topple_government wargoal on Italy) |
| Western peace deal | ENG/FRA ask Italy for peace before helping it (events .500-.503) | forced peace deal; states 550, 908, 559, 844 flagged for ETH |

Italy's mission "Conquer Ethiopia" (section 1) is the clock: the war must END (any way) with Italy holding
state 271 before about 22 December 1936.

## 3. Surrender arithmetic

Surrender limit 0.8, minimum 0.2. No Ethiopian spirit, leader trait (`king_of_kings` has none) or dynamic
modifier changes it except **ETH_demoralized_ras_idea** (-0.15). Progress is the share of Ethiopia's victory
points held by the enemy (Tigray + Wello held = 5 of 32 = 0.156).

### 3.1 Every Ethiopian victory point (states owned by ETH at start)

| State | Name | VP provinces (VP) | State VP | Other provinces | Category / slots | Infra |
|---|---|---|---|---|---|---|
| 271 | Shewa (capital) | 5010 Addis Ababa (5), 8036 Asella (1) | 6 | 10734, 13239 | town 4 | 2 |
| 835 | Hararghe | 13249 Harar (3), 7980 Dire Dawa (2), 12812 Wal Wal (1) | 6 | 12714, 12757, 13253 | rural 2 | 0 |
| 842 | Tigray | 2072 Macalle (2), 13237 Aksum (1), 13238 Adwa (1) | 4 | — | pastoral 1 | 0 |
| 837 | Sidamo | 13246 Irgalem (2), 8019 Yabello (1), 13247 Moyale (1) | 4 | 8062 | rural 2 | 0 |
| 838 | Illubabor-Kaffa | 13244 Gambella (2), 7912 Jiren (1) | 3 | 13245 | pastoral 1 | 0 |
| 839 | Welega | 2009 Asosa (2), 13243 Saio (1) | 3 | 12856 | pastoral 1 | 0 |
| 840 | Gojjam | 13242 Debre Markos (2) | 2 | 4954 | pastoral 1 | 0 |
| 841 | Begemder | 1984 Gondar (2) | 2 | 2040, 13137, 13241 | pastoral 1 | 0 |
| 836 | Bale | 10775 Gaba (1) | 1 | 4995, 13248 | pastoral 1 | 0 |
| 843 | Wello | 5097 Dessie (1) | 1 | 7944 | pastoral 1 | 0 |
| | **Total** | | **32** | | | |

Ethiopian states also carry `ETH_state_development_dynamic_modifier` (-10% local slots and build speed in
Shewa, -30% elsewhere) and, outside Shewa, `ETH_state_decentralization_dynamic_modifier` (+15% attack,
+5% defence, +10% army speed, -10% infrastructure build speed for Ethiopia).

What changes the total: `ETH_a_last_stand` (needs surrender progress > 0.5 and 838 controlled) moves the
capital to Illubabor and adds **+3 VP to Gambella 13244** (total 35).

| Ethiopian state | Surrender limit | VP Italy must hold (of 32) | of 35 after a last stand |
|---|---|---|---|
| normal | 0.80 | 26 | 28 |
| Demoralized High Command | 0.65 | 21 | 23 |

### 3.2 How Ethiopia gets "Demoralized High Command"

`ETH_retake_territory_mission`: activates when Ethiopia controls **at most half of the states it owns**
(`num_owned_controlled_states <= controlled_owned_50`, recomputed daily; Ethiopia owns 10 states, so 5 or fewer)
AND its balance of power is on the Mesafint side at -0.15 or lower (initial value -0.25) AND it does not already
have the spirit. It lasts **90 days**; if Ethiopia has not regained more than half by then: -10% war support,
-10% stability and the spirit. The spirit itself stays valid only while Ethiopia is at war and controls less than
75% of its owned states.

## 4. Missions and the "Abyssinian Fiasco"

### 4.1 North and South (Italy "Conquer Northern/Southern Ethiopia", Ethiopia "Hold the North/South")

| Mission | Goal (any 2 of 3 provinces) | Timer | Italian success | Ethiopian hold (timeout) |
|---|---|---|---|---|
| North | Gondar 1984 (841), Macalle 2072 (842), Dessie 5097 (843) | `ETH_hold_the_north_time` 120 (a) / 180 (b) | ITA +3% stability, +3% war support, 15 army XP, 15 CP; ETH -10% war support | ETH escalation +1, success flag |
| South | Wal Wal 12812 (835), Gaba 10775 (836), Moyale 13247 (837) | `ETH_hold_the_south_time` 120 / 180 | same | same; if ETH still controls 835 when Italy succeeds, Harar missions start |

Two Italian successes complete the focus `ITA_solid_progress` (+25 PP, +3% stability, +3% war support, +5% fascism).

### 4.2 The Abyssinian Fiasco (exactly)

`ITA_the_abyssinian_fiasco` is a focus with `available = always no`; it is only completed by script. It happens:

* when the FIRST Italian North/South mission fails (timeout or cancel without success): event
  `BBA_italy_missiolinis.02_north` / `.04_south` offers b "We need a change of leadership!" = complete
  The Abyssinian Fiasco (-15% fascism, -3% war support) or c "A staggering failure!" = complete
  `ITA_struggle_in_ethiopia` (-5% fascism, -3% stability, -25 PP); option a appears only for the second
  failure. AI picks c unless socialist/communist. The first failure shows only options b and c: pick by
  option name (plan `event_rules`), not by index;
* or when Italy makes peace through the withdrawal decision before any of the three focuses is done (event
  `BBA_italy_second_italo_ethiopian_war.2` option b);
* `ETH_events.1` (accepting Ethiopia's demand) completes Struggle in Ethiopia instead.

Neither focus ends the war. Struggle unlocks `ITA_undermine_the_duce`; all three lead to
`ITA_servizio_informazione_militare`, which **lowers escalation by 1**. Missing the North/South timer does not
lose the war: it fails a mission, raises escalation and forces a focus outcome. The real deadline is "Conquer
Ethiopia" (~22-27 December 1936).

### 4.3 Addis Ababa and Harar missions

* `ETH_hold_addis_ababa_mission` / Italy `ETH_conquer_addis_ababa_mission`: activate when both North and
  South Ethiopian flags exist (set at success or timeout) AND an Ethiopian core state touching 271 is not
  Ethiopian-controlled. Timer `ETH_hold_the_capital_time` = **180 days**. Italy controls 271:
  ETH -20% war support. Ethiopia holds: escalation +1.
* `ETH_hold_harar_mission` / `ETH_conquer_harar_mission`: start only if Italy wins the South while Ethiopia
  still controls 835; goal 835 controlled by Italy; timer = south timer.
* `ITA_attack_addis` AI front on 271 while Italy's Addis mission runs.

**Pacify Ethiopia** (after the war, 720 days): succeeds when every original Ethiopian state has compliance above 60% or
is controlled by an Italian subject (+75 PP, +5% stability, +35 command power; failure -25 PP). Handing every Ethiopian
state to puppets completes it at once; a state Italy keeps needs a compliance plan.

## 5. War escalation

Counter `ETH.ETH_war_escalation_level`, starts 0. Every +1 comes from `ETH_escalate_the_war_effect`.

| Cause of +1 | When |
|---|---|
| Event .01 option a | ~15 Jan |
| Time-based mission | every 123 days, then every 170 days, while level < 10 |
| Hold the North / Hold the South held by ETH | at their timeout |
| Hold Addis / Hold Harar held by ETH | at timeout |
| Italy has more than **24** non-irregular divisions in states 550 559 271 835-844 (once) | daily check (Afar 908 not in the list) |
| Italy has more than **299** aircraft in strategic regions 17, 273, 274 (once) | daily check |
| Italy refuses Fait Accompli (event .120 b) | event |
| De-escalation -1 | Italian focus ITA_servizio_informazione_militare |

At start Italy (its own OOB; the Italian player's count) has 25 divisions in 550/559/844, 4 of them irregular bands (21 count), and 120 aircraft in Eritrea.

| Level reached | Ethiopia gets (each once, if not fascist) | Italy gets (each once) |
|---|---|---|
| > 2 | +150 PP (.200) | |
| > 3 | (unlocks focuses needing level 4: arms purchases, mercenaries) | Italian public concerned: -10% ruling popularity (.300) |
| > 4 | (closing the Suez focus) | **Army of Africa disheartened**: -5% attack and -15% defence vs ETH, removes ITA_offensive/logistics spirit (.301) |
| > 5 | Army morale: +10% defence, +5% org (.201) | Unrest in colonies: -15 compliance, +15 resistance target in every resisting state (.302) |
| > 6 | (international brigades focus) | Public questions war effort: -30% PP, -1% war support weekly (.303) |
| > 7 | Enemy desertions: +15,000 manpower, 1,200 infantry_equipment_1, 500 support, 50 artillery (.202) | **Mass desertions: every Italian army unit in regions 17, 273, 274, 216, 217 loses 35% strength** (.304) |
| > 8 | Survival against all odds: +20% attack and defence vs Italy, +10,000 manpower (.203) | |
| > 9 | People's War: +45 resistance in occupied cores, 2 offsite arms factories (.204) | Uproar in colonies: -30 compliance, resistance target 80 for 730 days (.305) |

## 6. What Ethiopia can do (focuses, decisions, help)

Ethiopia uses `ethiopia_tree`. Two research slots; research -15% from Conservative High Command and -25% from the
corrupt bureaucracy variable. Starting techs: infantry_weapons, tech_mountaineers, tech_trucks, basic_train (no
support, no artillery, no anti-tank). Industry: Shewa 2 arms + 1 civilian, Hararghe 1 civilian; production starts as
2 factories of infantry_equipment_0. The international market is closed by the LoN arms embargo until focus
`ETH_arms_purchases`. No air force, no navy (5 convoys). Stay out of every faction until the peace: Italy
accepting a white peace while Ethiopia is in no faction is the scripted Ethiopian victory (the achievement flag).

### 6.1 War branch (needs the war flag)

| Focus (cost) | Effect |
|---|---|
| ETH_the_second_italo-ethiopian_war (5) | ETH_war_support_idea 180 days (+20% war support factor, +10% org regain, +5% defence vs majors), +10,000 manpower |
| ETH_renewed_offensive (5) XOR ETH_falling_back | +10% attack infantry and irregular infantry, -5% defence |
| ETH_northern_thrust (5) XOR ETH_victory_in_the_desert | 90 days in 841, 842, 843 and **550 Eritrea**: +25% attack, +50% breakthrough (Desert: 835, 836, **559, 844**) |
| ETH_mercenaries (10; escalation > 3; controls 843/835/838/837) | 3 Congolese Mercenaries (4 infantry, 30% XP) at Gambella if 838/837 held; 2 Arab Mercenaries |
| ETH_the_defense_of_addis_ababa / ETH_the_hindenburg_wall (10) | level-2 bunkers at Addis 5010 and 8036 / at Dire Dawa 7980 and Harar 13249 |
| ETH_a_last_stand (5; surrender > 0.5) | capital to 838, +3 VP Gambella, 120 days +15% core defence |
| ETH_the_lessons_of_war (10; escalation > 1) | 150 army XP, +10% war support, +10% defence, dig-in |

### 6.2 Military branch (no war needed)

ETH_ecole_militaire_haile_selassie_1er (5) -> ETH_scavenging_tactics (5: **+40% equipment capture**,
scavenger trait to 3 generals); ETH_keep_the_chitet (10) -> ETH_regional_training_centers (10) ->
ETH_regional_supply_depots (10: **-25% attrition, -15% supply consumption, -10% out-of-supply**) and
ETH_expand_the_levy (Chitet cap 48, +6 irregular battalions to the template).

### 6.3 Emperor branch: stay or go

`ETH_rally_around_the_emperor` (5) -> **XOR**:
* `ETH_the_emperor_stays` (5; +75 PP, +5% war support, +10% stability) -> `ETH_the_lion_stands_firm` (10:
  +100 PP, +50 CP) -> heroes or learning (10) -> `ETH_volunteers` (10: +20,000 manpower, 2,000 rifles) ->
  `ETH_fait_accompli` (10). This is the fighting path.
* `ETH_boarding_the_train` (5; needs surrender > 0.1 and a host nation from decision
  `ETH_host_country_decision`, ENG/JAP/GER/SWE) -> addressing the LoN -> `ETH_arms_purchases` (escalation > 3;
  opens the market and the decisions "buy rifles" 2,000 infantry_equipment_0 per 30 days for 50 PP) ->
  `ETH_request_embargo` (LoN embargo on Italy) -> `ETH_international_brigades` (escalation > 5; LoN members
  allow volunteers and send an International brigade) / `ETH_closing_the_suez` (escalation > 4; UK may close
  Suez to Italy, which then cannot pass the canal) -> `ETH_international_mediation`. This is the exile path;
  foreign volunteers and lend-lease to Ethiopia exist only on it.

### 6.4 Units Ethiopia can raise

`Chitet Sefari` (6 irregular infantry) is force-allowed for recruiting, cap 20 (48 after Expand the Levy).
Irregular infantry: 80 rifles per battalion (480 per division), 30 days training, supply 0.04 (regular 0.06), +25%
attack in desert, +15% in hills/mountains/forest, max organisation 45.

Communist route: `ETH_military_communism` excludes `ETH_rally_around_the_emperor`; then `ETH_in_the_name_of_the_people`
-> `ETH_soviet_aid` (excludes `ETH_callout_to_the_world`): with a communist Soviet Union not in civil war and Ethiopia
at war with Italy, the Soviets send 10,000 infantry equipment I.

## 7. Map: provinces, terrain, rivers, supply

`~R` = river on that border (the attacker takes the river penalty).

### 7.1 The northern axis (Eritrea -> Addis)

| Province | State | Terrain | Neighbours (river ~R) |
|---|---|---|---|
| 13250 Asmara (VP3, hub) | 550 | hills | 5091, 12723, 12766, **13237, 13238** |
| 12766 Massaua (VP1, port nb 3) | 550 | mountain | 5091, 8043, 13250, **13238** |
| 12723 Biscia (VP1) | 550 | hills | 5017, 5091, 13250, 13251, **13237**, 2088 (Sudan) |
| 13251 | 550 | mountain | 12723, **1984 ~R, 13237 ~R**, 2088 |
| 8043 | 550 | desert | 12766, **13238**, 5041 (908), 13236 (908), 4985 (Djibouti) |
| 5017, 5047, 5091 | 550 | hills/mountain | inner Eritrea (5091 touches 13250, 12766, 12723) |
| 13237 Aksum (VP1) | 842 | hills | 12723, 13250, 13251 ~R, 1984 ~R, 2072 ~R, 13238 |
| 13238 Adwa (VP1) | 842 | hills | 8043, 12766, 13250, 2072, 13237, 13236 (908) |
| 2072 Macalle (VP2) | 842 | mountain | 1984 ~R, 13237 ~R, 13238, 7944 ~R, 13236 (908) |
| 1984 Gondar (VP2) | 841 | mountain | 13251 ~R, 2040, 13137, 13241 ~R, 2072 ~R, 13237 ~R, 7944 ~R |
| 7944 | 843 | mountain | 1984 ~R, 13241 ~R, 2072 ~R, 5097 ~R, 5041, 13236 |
| 5097 Dessie (VP1) | 843 | mountain | 13239 ~R, 13242 ~R, 13241 ~R, 7944 ~R, **5041, 13240** |
| 13236 | 908 Afar | desert | 8043, 2072, 13238, 7944, 5041 |
| 5041 | 908 Afar | desert | 4985, 8124, 8043, 5097, 7944, 13236, 13240 |
| 13240 Aussa (VP2, AFA) | 908 Afar | mountain | 8124, **13239**, 7980, 5097, 5041 |
| 13239 | 271 | mountain | 5010, 8036, 7980, 13249, 13242 ~R, 5097 ~R, **13240** |
| 5010 Addis Ababa (VP5, hub) | 271 | mountain | 8036, 10734, 13239, 12856 ~R, 13242 ~R |
| 8036 Asella (VP1) | 271 | mountain | 5010, 10734, 13239, 13249, 10775, 13246 |

Routes from Eritrea to Addis (moves after the border province):
* Tigray-Wello: 13238 -> 2072 -> ~R 7944 -> ~R 5097 -> ~R 13239 -> 5010: **3 river crossings**, all mountain.
* Afar (needs 908 Italian, section 8): 8043 or 13238 -> 13236 -> 5041 -> 13240 -> 13239 -> 5010:
  **no river**, desert then mountain; Dessie 5097 is one step from 5041 or 13240, Dire Dawa 7980 one step
  from 13240.

### 7.2 The southern axis (Somaliland/Jubaland -> Hararghe, Bale, Sidamo)

| Province | State | Terrain | Neighbours |
|---|---|---|---|
| 12991 Mogadiscio (VP3, port nb 2) | 559 | plains | 2020, 2063, 5065, 10928 |
| 2020 | 559 | desert | 1964, 2063, 5065, 12840, 12991, **12812, 4995** |
| 1964 | 559 | desert | 2020, 12840, 10818, 4909, **12812**, 1905 (Br. Somaliland) |
| 2063 | 559 | desert | 2020, 8164 ~R, 10928, 12991, **4995 ~R, 11014 ~R** |
| 11014 | 844 | plains | 2063 ~R, 8164 ~R, **4995 ~R, 8062**, 12941 ~R, Kenya 8133/10761/12927 |
| 12941 Kismaayo (VP1, port nb 2) | 844 | plains | 8164 ~R, 11014 ~R, Kenya 10761/10976 |
| 12812 Wal Wal (VP1) | 835 | desert | 1905, 1964, 2020, 12757, 13253, 4995 |
| 4995 | 836 | desert | 2020, 2063 ~R, 12812, 13253, 13248 ~R, 8062 ~R, 11014 ~R |
| 13253 / 12714 | 835 | desert / hills | 13253: 12714, 12757, 12812, 4995, 13248; 12714: 12757, 13249, 13253, 13248, Br. Somaliland |
| 13249 Harar (VP3) | 835 | mountain | 8036, 13239, 7980, 12714, 10775, 13248, 10777 |
| 7980 Dire Dawa (VP2, hub) | 835 | mountain | 8124, 10777, 13239, 13249, 13240 |
| 8062 | 837 | hills | 4995 ~R, 10775 ~R, 13248 ~R, 13246, 13247, 11014, Kenya 8133 |
| 13247 Moyale (VP1) | 837 | hills | 8019, 8062, 13246, Kenya 2066/8133 |
| 10775 Gaba (VP1) | 836 | hills | 8036, 13249, 13248 ~R, 8062 ~R, 13246 ~R |

Somaliland's northern provinces (1966, 4909, 10818, 10891, 12840) only touch each other, 1964 and British
Somaliland; Ethiopia reaches them only through 1964.

### 7.3 Strategic regions (air missions, escalation count)

273 Danakil = Eritrea, Tigray, Wello, Afar, Djibouti; 17 Ethiopian Highlands = Shewa, Begemder, Gojjam,
Sidamo, Welega, Illubabor; 274 Ogaden = Hararghe, Bale, Somaliland, Jubaland.

### 7.4 Supply network at start

| Asset | Where |
|---|---|
| Supply hub (level 1) | Asmara 13250 (Eritrea) |
| Supply hub | Addis Ababa 5010 (Shewa, Ethiopian) |
| Supply hub | Dire Dawa 7980 (Hararghe, Ethiopian) |
| Supply hub | Djibouti 8124 (French) |
| Railway level 1 | Addis 5010 - 13239 - Dire Dawa 7980 - Djibouti 8124 |
| Railway level 1 | Massaua 12766 - Asmara 13250 |
| Ports | Massaua 12766 nb 3; Mogadiscio 12991 nb 2; Kismaayo 12941 nb 2 (all Italian); Djibouti 8124 nb 2 (French) |
| Infrastructure | 550 Eritrea 2 (3 after `ITA_ethiopian_war_logistics_bba`), 559 Somaliland 2, 844 Jubaland 2, 271 Shewa 2, 908 Afar 1, every other Ethiopian state 0 |
| Air bases | 550: 3, 559: 1, 271: 1, 835: 1 |

Building facts: supply_node 20,000 points, one per province, allied build, speed scales with infrastructure;
rail_way 170 + 130 per level; infrastructure 6,000; naval_base 5,000 + 1,000 per level. One project takes at most 15
civilian factories. At 5 points per civilian factory per day (4 unpowered) a hub with 15 factories in a
0-infrastructure state takes roughly **9 months** and a railway province a few days (read the queue's own days).
Captured railways are unusable for 10 days.

Supply reach (estimate; read the supply map mode): from Asmara (2.8 start, -0.5/-0.7 per province, hills +10%,
mountain +12%, river +0.2) Adwa and Aksum are well supplied, Macalle about 1.5, Wello below 0.5. With
trucks (motorization +2.2) the reach ends around Dessie. **Addis Ababa can only be supplied by the captured
Addis/Dire Dawa hubs once a railway links them to an Italian port** (the existing line runs to French
Djibouti). The cheapest link is Massaua 12766 -> 8043 -> 13236 -> 5041 -> 13240 -> 13239 (five provinces,
four of them in Afar).

## 8. Aussa (Afar 908)

* 908 is owned by AFA at start, cores ETH and AFA, infrastructure 1, one arms factory, VP Aussa 13240 = 2.
* Event `BBA_italo_ethiopian_war_events.400` "Time to deal with Aussa" (not triggered-only; checked by the
  game while its trigger holds): Italy, war flag set, AFA independent, and Italy **fully controls an
  Ethiopian core state neighbouring 908** (Tigray, Wello, Hararghe or Shewa).
  a = ultimatum -> AFA event .401 after 2 days: a "There's no point in resisting" (AI 10, b 1, b 0 with
  historical focus) -> Italy event .402 4-12 h later: Italy annexes AFA and gets 2 "Banda Indigena
  Irregolare" at Aussa 13240; b -> Italy declares war on AFA. b at event .400 costs 50 PP and -5% popularity.
* Timing: Tigray taken in early March brings the ultimatum about 18 days later. An Italian Afar needs a garrison:
  Ethiopia attacks it.
* Peace deals give Afar to Ethiopia whenever Italy owns it.

## 9. Starting forces

FAIR: a side's own forces are listed in full only for the player of that side. The enemy's start forces are
aggregate in FAIR (division count and templates, as `facts.py oob <ENEMY>` prints them); per-division positions,
the enemy's generals and its AI strategy files are private. What the enemy holds where is read live from the map.

### 9.1 Ethiopia (`ETH_1936_bba`, 21 divisions)

Aggregate (public in FAIR): 1 Kebur Zabagna (4 infantry + 1 cavalry, locked), 8 Mehal Sefari (4 infantry),
2 Mountain Division (4 mountaineers), 10 Chitet Sefari (6 irregular infantry, cap 20). Equipment 45-100%.
No air force. Positions and generals: Ethiopia's own `facts.py oob ETH` / `facts.py characters ETH` when
playing Ethiopia; an Italian player reads them from the map in game.

### 9.2 Italy in East Africa (`ITA_1936_nsb`, No Step Back)

For the Italian player only. Playing Ethiopia in FAIR, use the aggregate: 46 divisions in total, templates as
`facts.py oob ITA` prints them; positions are read from the map.

| Where | Divisions |
|---|---|
| Eritrea 550 (18) | 6 Divisione di Fanteria (6 inf + engineer + artillery) at 13251 x2, 12766 x2, 13250, 8043; 1 Divisione Alpina (12723); 5 Camicie Nere (12723, 13250, 12766, 8043 x2); 2 Divisione Ascari Eritrea (13250, 12766); Banda Irregolare Libica (8043), 2 Banda Irregolare Eritrea (8043, 12766), Truppe Irregolari a Cavallo (13251) |
| Somaliland 559 (6) | Divisione Ascari Libica (2063), Divisione Ascari Somala (2020), Camicie Nere (2063), Truppe Irregolari a Cavallo (2020), 2 Banda Irregolare Somala (1964) |
| Jubaland 844 (1) | Divisione di Fanteria (11014) |

Italy has 46 divisions in total; the other 21 are in Italy, Libya and the islands. Air in Eritrea
(`ITA_1936_air_bba`): 40 CR.32 fighters (small_plane_airframe_0), 32 SM.81 (medium_plane_airframe_1),
48 Ca.111 CAS (small_plane_cas_airframe_0); none in Somaliland.
Production at start: infantry_equipment_1 x4, support_equipment_1 x1, L3/35 light tank x1,
motorized_equipment_1 x2, Ba.65 CAS x1, Ca.111 CAS x1. Industry about 21 civilian, 20 military, 15 dockyards
(read `hoi4_economy`). Starting stockpiles: check them in game. Four research slots, stability 60%, war support
65%, laws limited conscription, partial economic mobilisation, limited exports. Starting land techs include
infantry_weapons1, tech_support, tech_engineers, tech_mountaineers, motorised_infantry, gw_artillery,
interwar_antiair, basic_train; grand doctrine grand_battleplan. Eritrea, Somaliland and Jubaland have resistance
on, compliance 20.

Navy at start: finish Montecuccoli, the two Cavour rebuilds, the Maestrale destroyers, the Abruzzi cruisers and the
Calvi submarine; remove the two Littorio lines in an Ethiopia-only plan. One escort task force (destroyers or old
light cruisers on `convoy_escort`) on the Red Sea route (Suez to Massaua and Mogadiscio) from day 1; 2 convoy lines,
no submarine or capital lines.

Italian generals available from the start: field marshals Rodolfo Graziani (offensive doctrine, desert fox), Pietro
Badoglio (organizer), Emilio De Bono, Italo Balbo; corps commanders Giovanni Messe (skill 4, attack 4, logistics 4),
Amedeo Guillet (skill 3, desert fox, irregulars officer), Hamid Idris Awate and Ibrahim Farag Mohammed (ascari
officer, desert fox), Carlo Vecchiarelli (mountaineer), Alessandro Pirzio Biroli (harsh leader), Ugo Cavallero,
Prince Filiberto.

## 10. Italian focuses that matter during the war

| Focus | Cost | Effect |
|---|---|---|
| ITA_ethiopian_war_logistics_bba | 5 | at completion, for each of 550/559/844 that Italy owns and controls (`is_owned_and_controlled_by`, the state's controller): +1 infrastructure and +2 naval base (Massaua 12766, Mogadiscio 12991, Kismaayo 12941), instant; bypassed if none of the three is Italian-controlled |
| ITA_army_primacy_bba | 5 | +25 army XP, +2 military factories (instant) |
| ITA_fiocchi_munizioni | 5 | +2 military factories in Lombardy |
| ITA_increase_artillery_production | 10 | +50% artillery research x2, +1 military factory |
| ITA_preserve_army_traditions XOR ITA_a_bandits_war | 10 | +150 infantry-track mastery, +5% org, +5% infantry defence / mobile bonuses |
| ITA_ministry_of_italian_africa | 5 | **needs the war flag cleared** (after the war) |
| ITA_army_leaders | 10 | needs the war flag cleared |

## 11. Italy failure modes

| Failure | Lesson |
|---|---|
| The Christmas offensive took all 8 Eritrean provinces by mid-January; the 18-division army sat at 1.4% supply, attacked Tigray at 5% supply / org 8 on 7 March and lost 9 divisions (59k vs 13k casualties) | hold every Eritrean border province from day one; no new offensive below 50% supply |
| Battle plans advanced ~40 days per state (Tigray 7 March, Wello 16 April, Shewa never); hubs for Tigray/Wello queued behind factories with no civilian factories assigned; army at 9% supply in Wello in May | attack single tiles from several directions; hubs and railways at the top of the queue; supply caps the army size |
| Tigray by 10 March (1.7k vs 17.7k casualties), Shewa entered 28 March, 3 of 4 Shewa provinces by 10 May, but **Addis Ababa 5010 itself never taken**; Somaliland 559 and Jubaland 844 partly lost from mid-February; Afar 908 lost in May; Wello retaken behind the army 30 May; final surrender share 5/32 = 15.6% | aim at VP provinces, not states; a guard army behind the strike army; guards that actually hold the south |
| `research_priority` held a non-existent key (`industrial_complex_expansion1`) and two techs Italy already had; `research_fallback` then picked artillery2-5 ahead of artillery1 and air techs from the legacy tree | validated research lists, fallback off |

## 12. Strong-player scripts

| ID | Tactic | How it works | When | Mode |
|---|---|---|---|---|
| SC01 | Italy: one fast war | Win by April-May 1936 before Ethiopia's exile focus (about six months) or no conference opens. North: mountaineers and infantry attack hard toward Addis Ababa with all tactical bombers and fighters; south: probe and pin with the weaker troops; ship the three tank divisions from Venice to the front and close 2-3 pockets; attack single tiles from several directions; never let Ethiopia dig in | Italy 1936 | FAIR |
| SC02 | Italy: supply | Motorisation 2 from day 1, railway levels on the Eritrean and Somali trunk lines (weeks, not a hub), an Organizer marshal; one escort task force on the Red Sea route from day 1; the rest of the mainland army is not needed | Italy 1936 | FAIR |
| SC03 | Italy: politics and peace | Keep the balance of power on the Mussolini side (0.6+); accept the Tigray submission ultimatum (free land and units); colonial police occupation laws once unlocked; peace: split Ethiopia into small puppets (no single Ethiopia puppet), take reparations and resource rights, release Eritrea and Somaliland (the puppets get the Horn of Africa tree, several of whose focuses add 8 oil, steel, aluminium or rubber each, which flows to Italy with resource rights); remove the two Littorio lines in an Ethiopia-only plan | Italy 1936 | FAIR |
| SC04 | Ethiopia: defend first | Convert to mountaineers; north: 3 divisions on the Aussa border and the rest on a mountain fallback line around Gondar; abandon Tigray to three hill tiles; south: hold the Harar river line and keep 4-6 divisions on the southern ports; all factories on infantry equipment; trade with France over land | Ethiopia 1936 | FAIR |
| SC05 | Ethiopia: counter-attack | Let Italy bleed, then counter-attack only where Italian supply is poor and the ground flatter, massed on one front; walk into Aussa once Italy holds Tigray (+1 military factory); take Massawa (Eritrea's only port) and the north collapses; Northern Thrust (+25% attack, +50% breakthrough for 90 days) for the push; Force Attack only to take Aussa or save a breaking key tile, because its casualties cost 20% extra war support | Ethiopia 1936 | FAIR |
| SC06 | Ethiopia: politics and peace | Mesafint power (-0.35 to -0.65) during the war; the 10 PP recall decision for instant units; refuse Italy's peace; the Fait Accompli focus (`ETH_fait_accompli`) cedes held Eritrea, Somaliland and Jubaland; afterwards infrastructure focuses before the Federal Empire, reintegrating puppets at 75 PP | Ethiopia 1936 | FAIR |
| SC07 | Italy: after the conquest | Harsh Quotas and forced labour on the conquered rubber states; colonial police occupation law once unlocked; a general fighting the real mountain war earns the Mountaineer trait; then Servizio Informazioni Militare and Triumph in Africa once the war flag clears; hold Ethiopia with cheap port garrisons and move the strike divisions home | Italy after the war | FAIR |
