# Italy conquers Ethiopia — FAIR campaign plan (1936-01-01, HOI4 1.19.4.1, By Blood Alone)

Executor plan: `italy-plan.json` (validated with `scripts/validate_plan.py --strict`: research 16, focus 13,
production 5, 0 errors; the whole 1936-1938 research path also validates). Mechanics and every file:line
proof: `italo-ethiopian-war.md` (cited below as MECH §n). Tags: [V] game file, [L] seen in runs w22-w24,
[H] judgement, [?] unverified. Dates use the game's 365-day calendar; focus cost 5 = 35 days.

**Win condition.** Ethiopia capitulates while Italy controls state **271 Shewa** (else Italy gets the
defeat flag) [V MECH §0.3], before the "Conquer Ethiopia" deadline about **22 December 1936** [V MECH §1].
Ethiopia capitulates at **26 of its 32 victory points** held by Italy (21 once it has "Demoralized High
Command") [V MECH §3]. Victory points live in single provinces: the army aims at provinces, not states.

## 0. Plan keys (existing and NEW)

Existing executor keys keep their meaning (`active`, `auto_events`, `event_options`, `focus_path`,
`focus_fallback`, `research_priority`, `research_fallback`, `research_fallback_prefixes`, `construction`,
`construction_*`, `production`, `production_priority`, `production_min_idle`, `politics`, `decisions`,
`armies`, `disband`, `air`, `alarms`, `war{enemies, axes, theatre, guard_states, guards, direct_moves,
supply_first, stall_days, rest_below, resume_above, supply_floor, motorization, min_army, per_target,
cooldown_hours, reserve}`). `research_fallback` is **false**: in w22-w24 the fallback filled slots with
artillery2-5 and legacy air techs [L].

NEW keys (not read by the executor yet; the player applies them, the executor can be extended to follow them):

| Key | Shape | Meaning for an executor extension |
|---|---|---|
| `event_rules` | list of `{event, option_name, condition?, else_option_name?, fallback_option_name?, why}` | choose an event option by its localisation key (`<event>.a/.b/.c`), not by index; `condition` is a plain-language test the player checks; `fallback_option_name` when the named option is not shown |
| `research_path` | list of `{year, not_before, techs[]}` | research only techs of blocks whose `not_before` date has passed, in order; never start a tech whose `start_year` is later than the current year |
| `construction_plan` | ordered list of `{order, building, state, province?, path?, from_state?, to_state?, when, priority, why}` | `when` is `"YYYY.MM.DD"`, `province_controlled:<id>`, `state_fully_controlled:<id>` or `owner:<state>=<TAG>`; `priority: "top"` moves the project to the top of the queue the moment it is queued; `path` lists the provinces a railway must follow |
| `doctrines` | list of `{folder, track, subdoctrine, when}` | adopt with `hoi4_doctrine_adopt` when the army XP condition holds |
| `army_layout` | list of `{name_hint, state, templates, count, general, role, axis or guard_states_in_order, when?}` | how to split the per-state armies into strike and guard armies on day one |
| `army_group` | `{name_hint, field_marshal, fallback_field_marshal, members}` | army group over the northern armies |
| `air_plan` | list of `{base_state, airframe, version, count, mission, region, then_region, switch_when}` plus rules | air missions per wing type and when to move them |
| `war_by_phase` | `{phase_id: {axes, guard_states, guards}}` | the exact `plan set` payload for the `war` object at each phase switch (section 6) |
| `vp_targets` | ordered list of `{order, province, name, state, vp, army, by, mission?, path?}` | the province each strike army goes for, with the date it should fall |
| `surrender_goal` | numbers from MECH §3 | stop condition for the VP hunt |
| `retake_rules` | list of `{if, do}` | own-land and corridor recapture rules (section 7) |
| `phases` | list of `{id, from, until/exit, goal}` | phase machine (section 6) |

## 1. Checkpoints

| Date | Must be true | If not |
|---|---|---|
| 1936-01-01 | Setup done while paused (section 2): 20 military factories on 4 lines, 4 research slots, focus `ITA_ethiopian_war_logistics_bba`, armies split, generals, air missions, fronts on every Eritrean, Somali and Jubaland border province; then unpause at speed 3 | — |
| 1936-01-16 | All 8 Eritrean provinces, all 12 Somali and both Jubaland provinces Italian; event .01 answered **b** | retake first (section 7); no offensive while any Eritrean province is enemy-held |
| 1936-02-05 | Logistics focus done: Eritrea infrastructure 3, Massaua naval base 5, Mogadiscio 4, Kismaayo 4 [V NFITA:65-150] | if Eritrea was not Italian-controlled at completion, its part is lost: never let Eritrea's controller change |
| 1936-02-10 | Adwa 13238 and Aksum 13237 Italian; railway Asmara 13250 -> Adwa queued at the top | strike army rests, then direct move into 13238 |
| 1936-02-20 | Wal Wal 12812 Italian (South mission 1/2) | south strike army holds 2020/1964 and tries again after resting |
| 1936-03-05 | Macalle 2072 Italian, Tigray fully Italian: hub at Macalle and railway Adwa -> Macalle queued | see section 9 "Tigray stalls" |
| 1936-03-31 | Aussa annexed (event .400 a); Bale desert 4995 Italian | if no event .400 within 25 days of full Tigray: check that every Tigray province is Italian |
| 1936-04-10 | Gondar 1984 Italian: North mission success (Macalle + Gondar) | attack Gondar from 13251, 13237 and 2072 at once with strike army + Gondar group |
| 1936-04-30 | Dessie 5097 Italian (via Afar 5041 if Afar is Italian, else 7944) | — |
| 1936-05-04 | Escalation level at most 1 (only the time-based step) | if Italy has > 24 regular divisions in the theatre states, stop sending more |
| 1936-06-15 | Addis Ababa 5010 Italian and state 271 controlled; railway 13240 -> 13239 done so the Addis hub is fed | section 9 "Addis stalls" |
| 1936-06-30 | Moyale 13247 (or Gaba 10775) Italian: South mission success before ~07-14 | accept the failure; answer the event with option c (Struggle) |
| 1936-07-31 | Dire Dawa 7980 and Harar 13249 Italian (20 VP) | — |
| 1936-08-31 | Debre Markos 13242, Gaba 10775, Irgalem 13246 Italian (25 VP) | if Ethiopia already shows "Demoralized High Command", 21 VP suffice |
| 1936-09-15 | Yabello 8019 Italian (26 VP) -> Ethiopia capitulates | add Gambella 13244 if Ethiopia moved its capital (MECH §3.1) |
| 1936-12-22 | absolute deadline (Conquer Ethiopia) | — |

## 2. Opening — setup while paused on 1 January 1936

Do it in three batches, then unpause at speed 3 and never pause again.

**Batch 1 — read.** `sitrep`, `hoi4_armies`, `hoi4_production_lines`, `hoi4_research`, `hoi4_events`.

**Batch 2 — economy.**
* Production (20 military factories; start: infantry_equipment_1 x4, support_equipment_1 x1, L3/35 x1,
  motorized_equipment_1 x2, Ba.65 x1, Ca.111 x1 [V MECH §9.2]): remove the L3/35 (light tank) and both
  CAS lines, then set **infantry_equipment_1 9, artillery_equipment_1 4, support_equipment_1 2,
  motorized_equipment_1 5**. Trucks matter: motorized supply spends about 40 trucks per hub per level
  [V playbook supply-logistics-deployment.md].
* Research, one tech per slot: slot 1 `basic_machine_tools`, slot 2 `construction1`, slot 3
  `interwar_artillery`, slot 4 `tech_military_police` (occupation needs it from March).
* Focus: `ITA_ethiopian_war_logistics_bba` (35 days).
* Construction: supply hub (`supply_node`) in Somaliland 559 at province 2020 at the top, railway
  12991 -> 2020 second, then `arms_factory` in Emilia-Romagna 161 (infra 4, 7 free slots) [V history/states].
* Laws: save political power for `war_economy` (government slot 2, 150 PP; available: war support > 50% and
  fascist [V common/ideas/_economic.txt:479-615]). Buy it the day PP reaches 150.

**Batch 3 — army and air.**
* The executor makes one army per state (Eritrea 18 divisions, Somaliland 6, Jubaland 1). Split them
  with `hoi4_supply {"army": ...}` (lists division ids and templates) and `hoi4_army_detach`:

| Army | Divisions (start positions) | General | Role |
|---|---|---|---|
| North strike (keep the executor's "Army 1") | 5 Divisione di Fanteria (13251, 12766 x2, 13250, 8043), 2 Divisione Ascari Eritrea (13250, 12766), Truppe Irregolari a Cavallo (13251) = 8 | Giovanni Messe (skill 4, attack 4, logistics 4) | strike: Tigray, Gondar, Dessie, Addis |
| Gondar group | Divisione Alpina (12723) + the second Fanteria at 13251 = 2 | Carlo Vecchiarelli (mountaineer) | holds 13251/12723, joins the Gondar attack |
| North guard | 5 Camicie Nere, Banda Irregolare Libica, 2 Banda Irregolare Eritrea = 8 | Alessandro Pirzio Biroli | guard Eritrea, then follows the strike army as garrison |
| South (Army 2) | Divisione Ascari Libica, Divisione Ascari Somala, Camicie Nere, Truppe Irregolari a Cavallo = 4 | Amedeo Guillet (desert fox, irregulars officer) | Wal Wal, Bale desert 4995, later Moyale/Gaba/Harar |
| South guard | 2 Banda Irregolare Somala (1964) | — | holds 1964 and 2020 |
| Jubaland guard | Divisione di Fanteria (11014) | — | holds 11014 (guard_states 844) |

* Army group "Africa Orientale" over the three northern armies with field marshal **Rodolfo Graziani**
  (offensive doctrine, desert fox); Pietro Badoglio if Graziani is refused. Only names from the
  `hoi4_army_commander` error list are accepted [L].
* Fronts (P1 hold): every army gets `hoi4_army_front` toward ETH on its border; at least 2 divisions in
  each Eritrean border province (13251, 12723, 13250, 12766, 8043), 2 in 1964, 2 in 2020, 1 in 2063,
  1 in 11014.
* Air [V MECH §9.2; mission names L]: CR.32 fighters (40, small_plane_airframe_0) `air_superiority` over
  region 273 Danakil; Ca.111 (48, small_plane_cas_airframe_0) `cas` over 273; SM.81 (32,
  medium_plane_airframe_1) `cas`, if refused `air_superiority`. When Dessie falls, move the fighter and
  CAS missions to region 17 (Ethiopian Highlands); if refused for range, use region 274 for the Harar push.
  **Never base more than 299 aircraft in regions 17/273/274** (escalation trigger [V ONA:763-777]).
* Disband up to 14 home divisions (Fanteria, Alpina, Camicie Nere, Cavalleria, Celere outside the
  theatre) in the first weeks so their equipment refills Africa [L w16, w24]; keep 7 at home.
* Doctrine (army XP): infantry track `assault_infantry` first (100 XP), then `fire_concentration` and
  `mission_type_tactics` [H; read the tooltip].

## 3. Research path

Rules: never research a tech before its `start_year`; prerequisites first (all lists validated); one slot
always on industry or construction until both 1937 techs are done.

| Year | Order (4 slots) |
|---|---|
| 1936 | basic_machine_tools, construction1, interwar_artillery, tech_military_police, support_weapons, tech_recon, concentrated_industry, range_improvements (aircraft range for Addis), tech_logistics_company, tech_field_hospital, excavation1, electronic_mechanical_engineering, synth_oil_experiments, radio, tech_maintenance_company, tech_signal_company |
| 1937 (from 01-01) | improved_machine_tools, construction2, concentrated_industry2, improved_equipment_conversion, excavation2, fuel_refining2, oil_processing, rubber_processing, mechanical_computing |
| 1938 (from 01-01) | infantry_weapons2, support_weapons2, improved_radio, computing_machine |

Do not research: artillery1/2 before 1939/1940, any `early_*`/`*_fighter` legacy air key (By Blood Alone
uses `bba_air_techs`), anything naval [V common/technologies].

## 4. Construction order

Civilian factories go to one project at a time (max 15 per project). Hubs in 0-infrastructure Ethiopian
states take months [H MECH §7.4], railways days: **railways first, then hubs**.

| # | Build | Where | When | Why |
|---|---|---|---|---|
| 1 | supply_node | Somaliland 559, province 2020 | 1 Jan | only hub buildable on day one; touches Mogadiscio port 12991, Wal Wal 12812 and Bale 4995 |
| 2 | rail_way | 12991 -> 2020 | 1 Jan | links the hub to the port |
| 3 | rail_way | Asmara 13250 -> Adwa 13238 | Adwa Italian | top of queue |
| 4 | rail_way | 13238 -> Macalle 2072 | Macalle Italian | top |
| 5 | supply_node | Macalle 2072 (842) | Tigray fully Italian (the game refuses while Ethiopians hold a province) [L] | top; feeds Wello, Gondar, Afar corridor |
| 6 | rail_way | Massaua 12766 -> 8043 -> 13236 -> 5041 -> Aussa 13240 | Afar owned by Italy | own land, no river |
| 7 | rail_way | 13240 -> 13239 | 13239 Italian | joins the Addis-Dire Dawa line [V railways.txt:884]: captured Addis 5010 and Dire Dawa 7980 hubs get Italian supply |
| 8 | rail_way | 2072 -> 7944 -> Dessie 5097 | Dessie Italian | second link north |
| 9 | rail_way | 2020 -> 12812 -> 13253 -> 12714 -> Harar 13249 -> Dire Dawa 7980 | each province as it falls | southern link to the Dire Dawa hub |
| 10 | infrastructure | Tigray 842 | after the hub | supply flow |
| filler | arms_factory | Emilia-Romagna 161, Abruzzo 157, Sicily 115, Tuscany 162, Sardinia 114 | whenever no hub or railway can be built | equipment |

After the war: `ITA_develop_ethiopia/eritrea/somaliland` give infrastructure and slots [V NFITA:259-966].

## 5. Production

| Line | Factories | From |
|---|---|---|
| infantry_equipment_1 | 9 | 1 Jan |
| artillery_equipment_1 | 4 | 1 Jan |
| support_equipment_1 | 2 | 1 Jan |
| motorized_equipment_1 | 5 | 1 Jan |
| artillery_equipment_2 | 3 (the factories from Army Primacy, Fiocchi, Artillery Production) | after interwar_artillery (about June) |

Every military factory is on a line (`production_min_idle 1`). No new divisions until every African
division is above 90% equipment; trucks before planes.

## 6. Focus path

ITA_ethiopian_war_logistics_bba (to 02-05) -> ITA_army_primacy_bba (03-12, +2 mil factories) ->
ITA_italian_highways_bba (04-16) -> ITA_fiocchi_munizioni (05-21, +2 mil) ->
ITA_increase_artillery_production (07-30, +1 mil, artillery bonus) -> ITA_preserve_army_traditions (10-08)
-> ITA_railway_innovations (12-17) -> ITA_air_innovations_bba; after the war ITA_ministry_of_italian_africa
-> ITA_develop_ethiopia/eritrea/somaliland -> ITA_regional_development [V NFITA, validated]. The Ministry
needs the war flag cleared, so the executor must skip it to the fallback while the war runs.

## 7. War phases

`plan set {"war": <war_by_phase[...] merged into war>}` at each switch; fill the `<...>` guard army names
from `hoi4_armies`.

**P1 Hold the Christmas offensive (1-15 January).** Axes empty, guard_states 550, 559, 844. No attack.
Every border province keeps 2+ divisions dug in. Ethiopia's +10% attack/breakthrough ends about 15 January
[V MECH §1]. Exit: date >= 16 January AND no Italian province enemy-held.

**P2 Tigray (16 January -> Tigray fully Italian, target 5 March).** Strike army: direct move into Adwa
13238 (from 13250/12766/8043), then Aksum 13237, then Macalle 2072 (no river from Adwa). Gondar group holds
13251. North guard holds all Eritrean border provinces until Adwa and Aksum are taken, then moves 2
divisions each into 13238 and 13237. South strike: Wal Wal 12812 from 1964 and 2020 (desert, one step).
Event .01 (~15 Jan): **option b** "methodical" — 180-day missions (to ~14 July), -5% supply consumption,
-15% heat attrition, +10% organisation regain for 240 days, no escalation [V WAR:105-141]. Exit: 13237,
13238, 2072 Italian.

**P3 Gondar and Afar (target 10 April).** Strike army + Gondar group attack Gondar 1984 together from
13251, 13237 and 2072 (all river crossings: rest to full organisation first, attack all at once). Event
.400 (Aussa) **option a**; the 2 bands that appear at Aussa 13240 form the Afar guard. South: Bale desert
4995 (shortens the Somali/Jubaland front to two provinces). Exit: Gondar Italian AND Afar owned by Italy.

**P4 Addis Ababa (target 15 June).** Strike army: 13236 -> 5041 -> Dessie 5097 (no river from 5041),
then 5041/13240 -> 13239 -> Addis Ababa 5010 -> Asella 8036. If Afar is not Italian, go 2072 -> 7944 ->
5097 -> 13239 (three river crossings). Before entering 13239 every corridor province behind the army
(13238, 2072 or 13236, 5041, 13240, 5097) holds one guard division. Railways 6 and 7 follow the army.
Exit: state 271 controlled by Italy.

**P5 Harar and the rest (to capitulation).** From Addis/Aussa: Dire Dawa 7980 (one step from 13240 and
13239), Harar 13249; south army from Wal Wal: 13253 -> 12714 -> Harar. Then Debre Markos 13242 (from 5010
or 13239, river), Gaba 10775 (from 8036 or 13249), Irgalem 13246 (from 8036), Yabello 8019 and Moyale 13247
(from 13246 or from Jubaland 11014 -> 8062). Stop as soon as Ethiopia capitulates.

**P6 Peace.** `hoi4_peace_options`, take every Ethiopian state (271, 835-843), `hoi4_peace_submit`,
`hoi4_peace_exit`; `hoi4_war_stats` for the casualty ratio. Then Ministry of Italian Africa and the develop
focuses; occupation law and garrisons for compliance.

### 7.1 Victory-point order (strike army unless noted)

| # | Province | VP | Running VP | By |
|---|---|---|---|---|
| 1-2 | Adwa 13238, Aksum 13237 | 1+1 | 2 | 02-10 |
| 3 | Wal Wal 12812 (south) | 1 | 3 | 02-20 |
| 4 | Macalle 2072 | 2 | 5 | 03-05 |
| 5 | Gondar 1984 (with Gondar group) | 2 | 7 | 04-10 |
| 6 | Dessie 5097 | 1 | 8 | 04-30 |
| 7-8 | Addis Ababa 5010, Asella 8036 | 5+1 | 14 | 06-15/06-30 |
| 9 | Moyale 13247 (south, via 11014-8062) | 1 | 15 | 06-30 |
| 10-11 | Dire Dawa 7980, Harar 13249 | 2+3 | 20 | 07-31 |
| 12 | Debre Markos 13242 | 2 | 22 | 08-15 |
| 13-14 | Gaba 10775, Irgalem 13246 | 1+2 | 25 | 08-31 |
| 15 | Yabello 8019 | 1 | 26 = 81% | 09-15 |

With "Demoralized High Command" (Ethiopia controls 5 or fewer of its 10 states for 90 days [V MECH §3.2])
21 VP end the war; after step 12 (22 VP) check `hoi4_wars`/surrender every tick.

## 8. Holding ground and avoiding encirclement

* **Retake first.** Any enemy unit in Eritrea, Somaliland, Jubaland or Afar: the guard army of that state
  retakes the province before anything else; if Asmara 13250 or Massaua 12766 is enemy-held, the strike
  army stops and retakes it (the only Eritrean supply source) [L w22].
* **Guards follow the army.** North guard order: Eritrea 550 -> Tigray 842 -> Wello 843 -> Shewa 271
  (`war_by_phase`). It moves only when the next state is fully Italian and leaves one Banda in 13251 and
  8043. Afar guard (2 bands) stays in 908 [L w24: Afar and Wello were retaken behind the army].
* **Corridor rule.** The strike army never advances more than two provinces beyond the last province held
  by a guard division; the provinces linking it to Asmara/Massaua or the Macalle hub always hold one
  division.
* **Supply rule.** `supply_floor 30`: below 30% the strike army stops attacking (executor rest); at
  motorization 2 from day one; if supply stays below 30% for 5 days, detach the two weakest divisions
  (irregulars, cavalry) to the guard army [playbook war-operating-loop §6].
* **Organisation rule.** Attack only above ~35 organisation (`resume_above 35`), rest below 20.
* **South.** The south strike army never leaves 2020/1964/12812 uncovered: Somaliland and Jubaland were
  entered in w24 the moment their armies moved [L]. After 4995 is taken, the guard line is 1964-12812-4995
  plus 11014.
* **Division count.** Keep at most 24 non-irregular divisions in states 550, 559, 844, 271, 835-843
  (escalation trigger [V ONA:748-762]); Afar 908 is not counted, the bands are not counted.

## 9. Contingencies

| Trigger | Do |
|---|---|
| Tigray stalls: Macalle not Italian on 03-15 | Gondar group joins the strike army; attack Macalle from Adwa only (no river); keep the Gondar plan for after |
| Addis stalls: strike army in 13239 below 30% supply | stop; finish railway 7 first; Addis hub only works once linked [MECH §7.4] |
| Ethiopia retakes a corridor province | guard retakes it; strike army holds in place, does not retreat |
| A mission is about to fail | let it; answer `BBA_italy_missiolinis.02_north`/`.04_south` with **c** "A staggering failure!" (Struggle in Ethiopia) [V BBAITA:982-1014] |
| Event `ETH_events.1` (Ethiopia took 550, 559 and 844) | **b** refuse while Italy still has an army in Africa |
| Event .120 (Fait Accompli) | **b** only if Italy controls 271 or holds 12+ VP; else a (refusing costs -25% war support, -20% stability, -250 PP) [V WAR:1179-1226] |
| Event .110 (LoN mediation) | a unless Ethiopia's surrender share is at least 0.5 (refusing gives democratic majors a topple wargoal on Italy) [V WAR:973-1028] |
| Escalation reaches 5 (Army of Africa disheartened) | take `ITA_servizio_informazione_militare` as soon as it is unlocked (-1 escalation) [V NFITA:8437-8497] |
| Ethiopia moves its capital to Gambella | add Gambella 13244 (VP 5 now) to the targets |

## 10. Do-not list

* No offensive during 1-15 January; no army leaves Eritrea while an Eritrean province is enemy-held.
* No naval transport of divisions (one division per order arrived in earlier runs [L]).
* No research filler (`research_fallback` stays false); no ahead-of-time tech.
* No more than 299 aircraft in East Africa; no more than 24 regular divisions in the theatre states.
* No peace before Italy controls state 271.
