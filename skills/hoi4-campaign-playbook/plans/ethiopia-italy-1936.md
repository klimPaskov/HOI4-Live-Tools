# Ethiopia pushes Italy out of Africa — FAIR campaign plan (1936-01-01, HOI4 1.19.4.1, By Blood Alone)

Executor plan: `ethiopia-plan.json` (validated with `scripts/validate_plan.py --strict`: research 9,
focus 13, production 1, politics 1, decisions 1, 0 errors; the 1936-1938 research path validates too).
Mechanics and every file:line proof: `italo-ethiopian-war.md` (MECH §n). Tags: [V] game file, [L] seen in
runs, [H] judgement, [?] unverified. Dates use the game's 365-day calendar; focus cost 5 = 35 days.

**Win condition.** Full control (every province) of **Eritrea 550, Italian Somaliland 559 and Jubaland 844**,
then the decision **`ETH_defeat_italy`** "Defeat the Colonizers": Italy's AI always accepts (option a 10,
b 0) and transfers the three states with a white peace, as long as Italy is in no faction [V MECH §2,
DETH:3-61, events/Ethiopia.txt:9-55]. Second route: Italy's withdrawal offer (AI from 1937-03-01) —
accept only when it hands over both 550 and 559. Third route: focus `ETH_fait_accompli` (needs every
Ethiopian state controlled) [V MECH §2].

**Honest expectation [H].** Ethiopia starts with 21 weak divisions, 2 military and 2 civilian factories,
2 slow research slots, no artillery, no support companies, no air force and a closed arms market
[V MECH §6, §9.1]. Italy has 46 divisions (FAIR aggregate), an air force and reinforcements; how many face
Ethiopia is read from the map. The plan
therefore survives first (defend mountains and hills, let Italy's missions fail and the war escalate), and
attacks Eritrea only inside the 90-day Northern Thrust window, when Italy's African army should already
carry escalation penalties.

## 0. Plan keys (existing and NEW)

Existing executor keys keep their meaning (see italy-plan.md §0). `research_fallback` is false.
NEW keys, for an executor extension (the player applies them until then):

| Key | Shape | Meaning |
|---|---|---|
| `event_rules` | `{event, option_name, condition, else_option_name, why, note}` | choose the option by localisation key; `condition` is a plan.schema.json predicate (the original wording is in `note`), checked by the player |
| `research_path` | `{year, not_before, techs[]}` | research blocks only from their `not_before` date, in order |
| `construction_plan` | ordered `{id, order, building, state, province?, when, why, note}` | as in italy-plan.md; the Shewa arms factory waits in `contingencies` for focus ETH_develop_the_capital_region |
| `training_rules` | `{only_if_stockpile: {archetype: n}, cap_template, cap}` | queue `training` only when the stockpile holds n of that equipment; never above the template cap |
| `doctrines` | `{id, folder, grand_doctrine or track+subdoctrine, xp}` | adopt once army XP reaches `xp` |
| `army_layout` | `{name_hint, state, templates, count, general, role: guard, guard_states_in_order, note}` | one guard army per region holding its state; `note` keeps the planned role, divisions and provinces to hold. Layout guards stay bound to their post, so the P3/P4 attacks need the phase overrides (build item 15) or the player |
| `army_group` | list of `{id, name, armies, marshal}` (executor, plan_military.rs) | two fronts; `armies` are layout name_hints, `marshal` a name or names tried in order |
| `war_by_phase` | `{phase: {axes, guard_states}}` | the exact `plan set` payload for `war` at each phase switch |
| `hold_targets` | `{id, army, provinces[], until?}` (executor, plan_military.rs) | one army per province whose loss fails an Ethiopian mission (provinces rear first; the post is the most forward one held); `note` names the mission and how many provinces it must keep |
| `vp_targets` | ordered `{order, province, name, state, from[]}` | the provinces to take, and from where |
| `win_condition` | `{decision, needs}` | take the decision the day it is available |
| `retake_rules`, `phases` | as in italy-plan.md | phase machine (section 6) |

## 1. Checkpoints

| Date | Must be true | If not |
|---|---|---|
| 1936-01-01 | Setup while paused (section 2); unpause at speed 3 | — |
| 1936-01-15 | Christmas offensive over; no Ethiopian army destroyed; Gondar 1984, Macalle 2072, Dessie 5097, Wal Wal 12812, Gaba 10775, Moyale 13247 Ethiopian | pull back to the mountain provinces behind (section 6 P1) |
| 1936-02-05 | Focus "The Second Italo-Ethiopian War" done (+10,000 manpower, war-support spirit) | — |
| ~1936-05-15 | Hold the North: 2 of Gondar/Macalle/Dessie still Ethiopian; Hold the South: 2 of Wal Wal/Gaba/Moyale (Italy's AI picks option a, 120 days [V WAR:38-45]) | if a mission is lost, keep the other; war support falls 10% |
| ~1936-05-15 | Escalation level 4-5 (a, time step, 2 holds, Italian divisions > 24) | — |
| 1936-06-25 | Northern Thrust done: 90 days of +25% attack, +50% breakthrough in 550/841/842/843 | if late, delay the Eritrea offensive until it is done |
| 1936-09-22 | Asmara 13250 and Massaua 12766 Ethiopian; Eritrea fully Ethiopian or Italian army there below 6 divisions | continue P3 without the bonus; do not start P4 |
| 1936-12-31 | Eritrea 550 fully Ethiopian | defend Eritrea, keep escalating; Italy's withdrawal offer comes after 1937-03-01 |
| 1937-06-30 | Somaliland 559 and Jubaland 844 fully Ethiopian -> decision ETH_defeat_italy | accept Italy's offer if it covers 550 + 559 |

## 2. Opening — setup while paused on 1 January 1936

**Batch 1 — read.** `sitrep`, `hoi4_armies`, `hoi4_research`, `hoi4_production_lines`, `hoi4_focuses`.

**Batch 2 — economy.**
* Production: both military factories stay on `infantry_equipment_0` (cheaper than _1, numbers matter;
  the starting line is already there [V history/units/ETH_1936_bba.txt:594-604]).
* Research (2 slots): slot 1 `tech_support` (support_equipment_1, gates engineers/recon), slot 2
  `basic_machine_tools`.
* Focus: `ETH_the_second_italo-ethiopian_war` (35 days).
* Construction: 1 project, `infrastructure` in Shewa 271 (the only state with a hub and a railway; Ethiopia
  has no free factory slot [V MECH §3.1]).
* Political power: save 150 for `partial_economic_mobilisation` (government slot 2; needs war support
  > 25%, Ethiopia has 30% [V common/ideas/_economic.txt:304-306, HETH:8]).

**Batch 3 — army.** The executor makes one army per state. Generals only from the `hoi4_army_commander`
list. Set the layout below and give every army `hoi4_army_front` toward ITA on its border.

| Army | Divisions at start (own OOB, `facts.py oob ETH`) | General | Holds |
|---|---|---|---|
| Army of Tigray | 2 Mountain Division (Aksum 13237), 3 Mehal Sefari (Adwa 13238 x2, Macalle 2072), 2 Chitet (2072, 13238) | Seyoum Mengesha (defence 3) | Aksum 13237 and Adwa 13238 (hills) with 3 each, Macalle 2072 with 1+; never fewer than 2 in Macalle after Italy enters Tigray |
| Army of Begemder | Mehal Sefari + Chitet at Gondar 1984 | Ayalew Birru (mountaineer) | Gondar 1984 only: every Italian approach crosses a river [V MECH §7.1] |
| Army of Wello | 3 Mehal Sefari (Dessie 5097), 2 Chitet (7944) | Mulugeta Yeggazu (attack 3) | Dessie 5097 (3) and 7944 (2); reserve for Macalle |
| Imperial reserve | Kebur Zabagna (Addis 5010) + every new Chitet | Kassa Haile Darge | Addis Ababa 5010; never empty |
| Army of the Ogaden | Mehal Sefari (Dire Dawa 7980), 2 Chitet (Wal Wal 12812, 12757) | Nasibu Zeamanuel (hill fighter) or Beyene Merid (desert fox) | Wal Wal 12812 (2), Harar/Dire Dawa |
| Army of Bale and Sidamo | 2 Chitet (4995), 1 Chitet (8062) | Desta Damtew (attack 3) | Bale 4995 (blocks 2020/2063/11014), Sidamo 8062 (blocks 11014 -> Moyale) |

Army groups: Northern front (Tigray, Begemder, Wello) under field marshal **Hailu Tekle Haymanot** (hill
fighter; Haile Selassie if refused); Southern front (Ogaden, Bale-Sidamo) under **Abba Jofir** (desert fox).

## 3. Research path (2 slots; -15% Conservative High Command, -25% bureaucracy [V MECH §6])

| Year | Order |
|---|---|
| 1936 | tech_support, basic_machine_tools, tech_engineers, gw_artillery, construction1, infantry_weapons1, support_weapons, tech_recon, interwar_artillery |
| 1937 (from 01-01) | improved_machine_tools, construction2, tech_military_police, interwar_antitank, excavation1, concentrated_industry |
| 1938 (from 01-01) | infantry_weapons2, support_weapons2 |

Expect about five or six of the 1936 techs in 1936 [H]. Engineers go into Mehal Sefari only when the
stockpile has support equipment for them.

## 4. Construction and production

* Construction (about one civilian factory left after consumer goods [H]): Shewa 271 infrastructure, then
  an arms factory in Shewa when a slot appears (focus `ETH_develop_the_capital_region` adds two civilian
  factories and two slots [V NFETH:1354]), then Tigray 842 infrastructure. No supply hub, also not in Tigray
  842 after Eritrea falls: a hub costs 20000 points [V common/buildings/00_buildings.txt] and Ethiopia's one
  or two free civilian factories would need years [D calc.py build-order]; captured railways and ports
  (Asmara, Massaua) supply the advance [lesson L12_early_hub_cost].
* Production: 2 x infantry_equipment_0 for the whole plan. Equipment comes mainly from capture:
  `ETH_scavenging_tactics` gives +40% equipment capture [V IETH:742-745]; escalation level 8 gives 1,200
  infantry_equipment_1, 500 support, 50 artillery [V WAR:1354-1383]; `ETH_volunteers` gives 2,000 rifles
  [V NFETH:4053].
* Training: one `Chitet Sefari` at a time (480 rifles, 30 days, cap 20 [V MECH §6.4]) only when the
  stockpile holds 600+ infantry equipment; new divisions join the Imperial reserve.
* Doctrine: when army XP allows, grand doctrine `mass_assault` (48 h no-supply grace, -10% out-of-supply
  penalty [V playbook research-doctrines-projects.md §2.1]), then infantry track `irregulars` (50 XP, NCNS)
  [V common/doctrines/subdoctrines/land/infantry_subdoctrines.txt]; Conservative High Command raises land
  doctrine cost by 50% [V IETH:228-232].

## 5. Focus path

| # | Focus | Done | Why |
|---|---|---|---|
| 1 | ETH_the_second_italo-ethiopian_war | 02-05 | +10,000 manpower, war-support spirit 180 days [V NFETH:2623] |
| 2 | ETH_renewed_offensive | 03-12 | +10% attack infantry and irregulars [V NFETH:2648] |
| 3 | ETH_ecole_militaire_haile_selassie_1er | 04-16 | army XP / academy, prerequisite of 4 [V NFETH:43] |
| 4 | ETH_scavenging_tactics | 05-21 | +40% equipment capture before the big battles [V NFETH:131] |
| 5 | ETH_northern_thrust | 06-25 | 90-day +25% attack, +50% breakthrough in Eritrea 550, 841-843 [V NFETH:2716] |
| 6 | ETH_mercenaries | 09-03 | 3 Congolese + 2 Arab mercenary divisions (needs escalation > 3 and 838/837 held) [V NFETH:2893] |
| 7 | ETH_rally_around_the_emperor | 10-08 | +50 PP |
| 8 | ETH_the_emperor_stays | 11-12 | +75 PP, +5% war support, +10% stability (closes the exile branch on purpose) |
| 9 | ETH_the_lion_stands_firm | 1937-01-21 | +100 PP, +50 command power |
| 10 | ETH_the_lessons_of_war | 1937-04-01 | 150 army XP, +10% defence, dig-in |
| 11 | ETH_learning_from_the_enemy | 1937-06-10 | +10% factory output, tank/air tech bonus |
| 12 | ETH_volunteers | 1937-08-19 | +20,000 manpower, 2,000 rifles |
| 13 | ETH_fait_accompli | 1937-10-28 | only if every Ethiopian state is Ethiopian-controlled |

Fallbacks: ETH_the_abuna, ETH_keep_the_chitet -> ETH_regional_training_centers ->
ETH_regional_supply_depots (-25% attrition, -15% supply consumption: take it before Somaliland if the focus
slot is free), ETH_expand_the_levy, ETH_develop_the_capital_region, ETH_university_of_capital.
If `ETH_mercenaries` is not yet available (escalation <= 3), run `ETH_rally_around_the_emperor` first.

## 6. War phases

`plan set {"war": <war_by_phase[...] merged into war>}` at each switch.

**P1 Hold (1 January -> end of the Hold missions, about 15 May).** Axes empty; guard states 842, 841, 843,
835, 836, 837. Hold the hills and mountains: Aksum/Adwa (hills), Macalle, Gondar, Dessie (mountains), Wal
Wal (desert, irregulars +10% defence there), Bale 4995. Italy's missions ask for Gondar, Macalle, Dessie
(north) and Wal Wal, Gaba, Moyale (south) [V MECH §4.1]: those provinces get the first reinforcements.
* Christmas opportunity rule (1-14 January, Ethiopia +10% attack and breakthrough vs Italy [V IETH:72-90]):
  attack an Eritrean province only if it shows at most 1 Italian division and 3+ Ethiopian divisions are
  adjacent; the targets that matter are Asmara 13250 (hub) and Massaua 12766 (port).
* Hold targets: keep at least 2 of Gondar 1984, Macalle 2072, Dessie 5097 and 2 of Wal Wal 12812, Gaba
  10775, Moyale 13247 until the missions end (each held mission +1 escalation, each lost one -10% war
  support [V DETH:1063-1143]).
* Retake: an Ethiopian province Italy enters is counter-attacked by the guard army only with at least as
  many divisions as Italy has there; otherwise hold the next mountain province.
Exit: the Hold missions have ended (decision panel) AND Ethiopia still holds Addis 5010.

**P2 Counter-attack and refit (mid-May -> 25 June).** Retake Ethiopian provinces Italy took, starting with
VP provinces; train Chitet; refill. Escalation should now be 4-5: at > 4 Italy's African army gets -15%
defence against Ethiopia [V MECH §5]. Exit: focus Northern Thrust done.

**P3 Eritrea (25 June -> state 550 fully Ethiopian).** `war_by_phase.P3_eritrea`: axis [550], guards on
841, 843 and the south. Army of Tigray + Army of Wello + Imperial reserve attack:
Asmara 13250 (from Aksum 13237 and Adwa 13238) and Massaua 12766 (from Adwa 13238) first — taking both cuts
the Italian Eritrean army off its hub and port [V MECH §7.4]; then Biscia 12723, 13251 (from 12723, no
river), 8043 (desert: irregulars +25% attack), 5091, 5017, 5047. Army of Begemder pins 13251 from Gondar
(river: do not attack across it). Keep 2 divisions in Macalle and 2 in Dessie against an Italian thrust
through Afar.
Exit: every province of 550 Ethiopian.

**P4 Somaliland and Jubaland (after Eritrea).** `war_by_phase.P4_south`. Ogaden army: Wal Wal 12812 ->
2020 -> Mogadiscio 12991 (VP 3, port) -> 5065, 10928; and 1964 -> 12840 -> 10818 -> 4909 -> 10891 -> 1966
(the northern strip has no other entry [V MECH §7.2]). Bale-Sidamo army: 8062 -> 11014 -> Kismaayo 12941,
then 8164 and 2063. Eritrea keeps a guard (2-3 divisions in 13250 and 12766).
Exit: 550, 559 and 844 all fully Ethiopian.

**P5 Finish.** Take `ETH_defeat_italy` immediately (it is in `decisions`). If Italy's withdrawal event
`BBA_italy_second_italo_ethiopian_war.1` comes first: option a only with full control of 550 and 559,
else b (the decision is once only on Italy's side [V DITA:139]).

## 7. Holding ground and avoiding encirclement

* Defend on the river lines Italy must cross: Gondar (all approaches), 7944/Dessie (from Macalle), Debre
  Markos and the Welega border of Addis [V MECH §7.1].
* Never leave Addis Ababa 5010 empty: Italy wins only with state 271 in its hands at the end [V MECH §0.3].
* In Eritrea and Somaliland, every taken province on the path back to Tigray (or to Wal Wal) keeps one
  division; ETH supply is local only (Ethiopian infrastructure 0, no captured hub is linked to an Ethiopian
  port), so limit each attacking army to about 8 divisions and rest below 15 organisation [H].
* Do not chase Italian units into Afar (908 belongs to Aussa until Italy annexes it; attacking it
  starts a new war).

## 8. Contingencies

| Trigger | Do |
|---|---|
| Italy takes Tigray before 15 March | Army of Wello holds Dessie and 7944; Tigray army falls back to Macalle if it still holds it, else to 7944 |
| Italy annexes Aussa (Afar Italian) | 2 divisions of the Imperial reserve to 13239 (Addis's eastern gate, touches Aussa) and 1 to Dire Dawa 7980 |
| Ethiopia controls 5 or fewer of its 10 states | the 90-day "retake" mission starts: retake one state within 90 days or get "Demoralized High Command" (-15% surrender limit) [V MECH §3.2] |
| Surrender progress > 0.5 and 838 held | focus `ETH_a_last_stand` is available (capital to Gambella); take it only if Addis is about to fall |
| Italy's peace offer arrives | rule in section 6 P5 |
| Escalation reaches 8 | Italian army loses 35% strength in East Africa: start P3/P4 attacks that week [V WAR:1526-1558] |

## 9. Do-not list

* No early Eritrean offensive outside the Christmas opportunity rule and the Northern Thrust window.
* No `ETH_boarding_the_train` (the exile branch; it closes the Emperor Stays path).
* No training into an empty stockpile; no unsupplied stacks of more than ~8 divisions.
* No peace that leaves Eritrea or Somaliland Italian unless the war is being lost.
