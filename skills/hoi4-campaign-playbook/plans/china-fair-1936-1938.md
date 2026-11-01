# Nationalist China FAIR standing plan, 1936-01-01 to 1938-01-01

Companion to `china-fair-1936-1938.plan.json` (JSON has no comments, so every
section is explained here). Copy the JSON to the `plan.json` the runner reads
(see `native/src/plan_run.rs`; the file is re-read every tick). Keys that start
with `_` are ignored by the runner; `_validator` only feeds
`scripts/validate_plan.py`.

```
python3 scripts/validate_plan.py skills/hoi4-campaign-playbook/plans/china-fair-1936-1938.plan.json --strict
checked: research=30, focus=25, production=2, construction=3, politics=2, decisions=0; 0 error(s), 0 warning(s)
```

Every key was checked against the installed HOI4 1.19.4.1 files (technologies,
focus tree, equipment, buildings, history states, ideas, characters, events).
Installed DLC (`dlc/`, `dlc_load.json` disables none): No Compromise No Surrender,
No Step Back, By Blood Alone, Arms Against Tyranny, Gotterdammerung, La Resistance.
**Waking the Tiger and Man the Guns are not installed.** So the CHI tree is
`china_nationalist_sea_focus` (`china_nationalist_sea.txt`, factor 1000 with NCNS);
the old `china_nationalist_focus` needs Waking the Tiger and never loads.

The validator needed one fix for this plan: 1.19 characters wrap roles in
per-DLC `instance = { ... }` blocks (Chen Guofu's advisor role is only there), so
`scripts/validate_plan.py` now also reads advisors inside `instance` blocks.

## Start position (verified, NCNS)

* NCNS `1936.1.1` blocks in `history/states` hand 11 of the 28 CHI-owned states to
  warlords on day one: Beiping 608, Hebei 614, Hebei-Chahar 1039 to HBC; Shandong
  597, Jinan 1038 to SND; Chongqing 1037, Chengdu 1041, Sichuan 605, Liangshan 751,
  Ganzi 752 to SIC; Xi'an 744 to XIC. **CHI starts with 17 states, 11 civilian /
  8 military / 2 naval factories** (Nanjing 2 civ; Wuhan 1+1; Hunan 1+1; Changde
  1+1; Guizhou 1+2; Hubei 1 mil; Shanghai 2 mil + 1 dock; Jiangsu 1 dock; 1 civ
  each in Anhui, Henan, Jiangxi, Fujian, Qingdao).
* Laws: defaults `civilian_economy`, `export_focus`, volunteer only. Research slots: 2
  (`BASE_RESEARCH_SLOTS`), research speed -20% (Neglected Countryside).
* Corruption in the Administration: economy cost +65% (a 150 PP law costs ~248 PP),
  consumer goods +25%, construction -10%, +0.35 PP/day. With civilian economy +35%
  and Illicit Trade -15% the consumer-goods share is **45%**: of 19 factories only
  ~2-3 civilian factories are free on day one. Cutting consumer goods is the first
  economic lever, ahead of building.
* Production (`history/units/CHI_1936_sea.txt`): one line, `infantry_equipment_0` on 2
  factories, so **6 military factories are idle on day one**.
* Army: 85 divisions (48 "Sanjiao Jun" 6-inf, 17 "Juntuán Reformed" 6-inf + support
  artillery, 14 "Juntuán" 4-inf garrison, 6 "Qibing Jun" cavalry), low equipment.

## Runner behaviour that shapes the plan

* One action per tick, in order: events, research, focus, politics, decisions,
  training, military-factory rebalance, `production` lines, construction, armies.
  A refused action is skipped for 1-3 game days.
* Focus: the first not-completed key the game accepts; a key the game blocks
  (not available, prerequisite, exclusive) is skipped for 6 game hours and the next
  key is tried at once. War-only focuses are therefore placed early in the path: they
  are refused in peace and taken at the first free focus slot once Japan attacks.
* Politics: picks are tried in list order every tick; a refused pick waits one day,
  so a cheap later pick can be bought while PP is saved for an earlier one.
* `stop_on_date` pauses the game on 1938.01.01.

## Top-level keys

| Key | Value | Why / when to deviate |
|---|---|---|
| `active`, `stop_on_failure` | true / false | A failed action must not freeze the routine. |
| `stop_on_date` | `"1938.01.01"` | Window end. Extend together with the focus tail. |
| `auto_events`, `wait_events_days` | true / 3 | Single-option events (e.g. `SEA_china_improvement.1` from Crackdown) get option 0; multi-option events wait 3 days for the model. |
| `event_options` | `china.1: 0` | Marco Polo Bridge (`events/China.txt`). Option 0 "Breaking point!" = Japan declares `annex_everything` on CHI, SHX, SND, HBC. Option 1 (concessions) gives Japan Qingdao 743 and any of 608/597/1038/614 we control for a 180-day truce: the war still comes, with less land. FAIR plan fights. |
| `alarms` | PP > 250, consumer goods > 0.45, idle civ >= 2, idle mil >= 2, unassigned >= 2 | PP 250 because a law costs ~230-248 PP here; idle civ 2 because `construction_min_idle` is 1. |

## research_priority (30 technologies)

Only 2 slots at -20% speed: two years finish roughly 10-14 techs, so entries past
~15 are tail. Start techs (validator reads the history file): `infantry_weapons`,
`gw_artillery`, `interwar_antiair`, `basic_train`, `tech_support`, `tech_trucks`,
`gwtank_chassis` and the BBA air set. `infantry_weapons1` is **not** a start tech.

| Positions | Block | Reason |
|---|---|---|
| 1-4 | infantry_weapons1, construction1, tech_engineers, basic_machine_tools | Rifle model (`infantry_equipment_1`), +10% construction, engineers for river/fort defence, factory efficiency cap. |
| 5-8 | support_weapons (1918 tech, missing), interwar_antitank (`anti_tank_equipment_1`), concentrated_industry, interwar_artillery | Japan's armour and artillery; +15% factory output (+20% building slots). interwar_artillery is a +5% soft-attack tech in 1.19.4; `artillery_equipment_1` already comes from gw_artillery. |
| 9-16 | construction2, electronic_mechanical_engineering, basic_fortification_tech, tech_recon, improved_machine_tools, concentrated_industry2, radio, excavation1 | 1937 economy; fort tech before any bunker work. |
| 17-30 | support_weapons2, infantry_weapons2, land_fort_tech_1, artillery1, antitank1, tech_engineers2, conversion, computing, excavation2, construction3, advanced_machine_tools, concentrated_industry3, improved_infantry_weapons, tech_recon2 | 1938-39 items, tail so the list never runs dry. |

Deviate when: the war starts (move antitank1/artillery1 up); you pick dispersed
industry instead (exclusive with the concentrated chain). Extend Compulsory Education
gives a one-use 25% electronics bonus: if it lands early, move radio up.

## focus_path (25 focuses) and focus_fallback

All keys are in `china_nationalist_sea_focus`; each prerequisite block has a member
earlier in the path; no exclusive pair is listed. Dates assume no idle days and a
Japanese attack around mid-1937.

| # | Focus | Cost | Note |
|---|---|---|---|
| 1 | CHI_dang_guo | 5 | ~02-05. Stability +10%, communism/fascism -5%. Exclusive with End Military Primacy and Strengthening our Resolve. Keeps Chiang. |
| 2 | CHI_promote_the_national_currency | 5 | ~03-11. +25 PP, economy cost -10%, consumer goods -5%, PP gain -0.05. Before the first law. |
| 3 | CHI_denounce_feudalism | 5 | ~04-15. Stability +5%, warlord opinion, unlocks Chen Guofu. Exclusive with Prepare for a Change in Leadership (Wang Jingwei). |
| 4 | CHI_one_nation_undivided | 10 | ~06-24. Creates the Chinese United Front and invites every eligible warlord (not HBC, not Sinkiang during its war) with event SEA_china.7. A year before the war for the AI to accept. |
| 5-8 | CHI_demonstrate_our_resolve, CHI_initiate_the_gung_ho_movement, CHI_secure_the_soviet_border, CHI_send_the_first_lady_abroad | 5, 10, 10, 10 | **War-gated** (has_war / war with Japan; Soviet Union resp. a democratic US/UK/France not at war with us). Refused in peace, taken first when the war starts. Gung Ho: +10% arms-factory speed, +10% war support, +2 arms factories in inland states. |
| 9 | CHI_decree_the_end_to_corruption | 5 | ~07-29. Construction +10% (cancels the -10%), consumer goods -5%, economy cost -10%. |
| 10 | CHI_the_three_year_plan | 10 | ~10-07. Needs peace. Spirit (1095 days): consumer goods -5%, +10% construction boost, +10% efficiency gain; the spirit only works while at peace. Can be bypassed (manually) at war, so its follow-ups stay reachable. |
| 11 | CHI_fund_the_plan_domestically | 10 | ~12-16. +2 civilian factories (instant, extra slots). Exclusive with Find Foreign Financiers (needs a major with opinion > 19; Japan does not count for the reward). |
| 12-14 | CHI_military_affairs_commission_sea, CHI_protracted_warfare, CHI_defence_in_depth | 5, 10, 10 | ~1937-01-20 / 03-31 / 06-09. Morale +10%, defensive_postures subdoctrine, defence +5% and dig-in +1. The defensive core before the war. Decisive Battles is locked while Protracted Warfare is done until Serve the Country is done (and vice versa). |
| 15-17 | CHI_formalize_field_promotions, CHI_crackdown_on_corruption_in_recruitment, CHI_guerrilla_warfare | 10, 10, 5 | Training -25% (over NCO School's -15%), morale/org, conscription event, dig-in +1. |
| 18-19 | CHI_monopolise_resource_extraction, CHI_central_steel_and_machine_works | 5, 10 | +3 civilian factories and steel. Tail for 1938 unless the war is late. |
| 20-25 | extend_compulsory_education, relocate_factories_from_the_coast, the_4_defence_areas, bog_them_down_in_the_centre, expand_the_tongzi_jun, inspect_the_troops | | Research/communism fix (starts the Neglected Countryside mission), coastal and central defence modifiers, +10% war support (Tongzi over Baojia's +5% conscription/+5% training time), troop inspection (needs a faction). |

`focus_fallback`: national registry of property (economy cost -10%, construction
+10%), rely on locals, reorganise the train, coastal forts (bunkers in Qingdao/
Shanghai/Zhejiang/Fujian/Jiangsu and a 75% fort tech bonus), Burma Road, Hanoi Route,
ROCAF. Harmless, not exclusive with the path.

Never add: `CHI_end_military_primacy`, `CHI_strengthening_our_resolve`,
`CHI_prepare_for_a_change_in_leadership` (Wang Jingwei), `CHI_preemptive_strike`
while Japan is the main front, `CHI_crack_down_on_illicit_goods` (-15% stability) in
this window. `CHI_controlling_our_skies` needs an air grand doctrine CHI does not
have in 1936. `CHI_turn_a_blind_eye` (consumer goods -15%, output -5%, economy cost
+10%) is a legal alternative if consumer goods hurt more than output.

## politics

| Pick | Cost | Why |
|---|---|---|
| government 2 `partial_economic_mobilisation` | 150 x economy cost (≈232 PP after the currency focus) | War support 0.32 > 0.25 qualifies on day one. Consumer goods 35% -> 25% and no construction penalty: strictly better than Early Mobilisation (the game's own AI weights say the same). Freed factories go straight into construction. |
| government 3 `CHI_chen_guofu` | 100 PP | Consumer goods -10%, war goal justification -10%, fascism drift +0.06. Available after Denounce Feudalism; may be bought before the law if PP is short, which is fine. |

Not picked: Free Trade (only +5% output/construction over the default Export Focus
for ~220 PP), conscription laws (volunteer pool is enough for the equipment we have),
Du Yuesheng (consumer goods -15% but stability -10%; needs the Shanghai elites focus).
`military_staff` picks have no verified slot numbering: by hand,
`CHI_alexander_von_falkenhausen` (army chief offensive 2, 50 PP, available until
Germany recalls him) and theorist `CHI_jiang_baili` (defence +5%, 100 PP) are the
obvious ones. Bank 100+ PP from mid-1937 for war decisions.

## decisions

Empty on purpose. Every useful CHI war decision (`CHI_demonstrate_our_resolve_decision`,
`CHI_bog_them_down`, `CHI_declare_war_zone`, `CHI_holding_state_mission`) is
state-targeted, and the runner only takes untargeted keys. MIO relocations
(`CHI_relocate_mio_shanghai_arsenal`, `CHI_relocate_mio_jinling_arsenal`, 50 PP,
35 days, available in peace) would fire on day one and eat the law's PP; take them
by hand (war checklist). `CHI_industrial_evacuations_*` is not allowed for CHI with
NCNS.

## training and armies

* `training`: 1 line of the stock `Juntuán Reformed` (6 infantry + support artillery,
  name verified in `CHI_1936_sea.txt`), deploying in Henan 607 (Zhengzhou junction,
  behind the northern front). The 85 starting divisions are under-equipped, so new
  divisions only dilute rifles: raise `queue` only when the rifle stockpile exceeds
  ~3k with the field army above ~80% equipment.
* `armies`: armies of at most 24, absorb on, generals for armies of 8+. Names are the
  English localisation of 1936-visible NCNS commanders: Hu Zongnan (4), Chen Cheng,
  Wei Lihuang, Zhang Fakui, Sun Li Jen, Feng Yuxiang, field marshal Hsueh Yueh,
  Alexander von Falkenhausen (4, removed if Germany recalls him), Wang Yao-wu.

## production

Day one: 6 idle military factories. `production_min_idle: 7` keeps the rebalance off so
the two `production` lines take exactly the 6: a second `infantry_equipment_0` line (4)
and `artillery_equipment_1` (2). Rifles then run on 6 factories, artillery on 2.

**Manual edits required:**
1. Once both lines exist (first week), set `production_min_idle` to 2 so new factories
   go to `production_priority` (rifles first).
2. When `infantry_weapons1` completes (~mid 1936), switch the rifle lines to
   `infantry_equipment_1` with `hoi4_production_set`.
3. After `interwar_antitank`, add `anti_tank_equipment_1` (1-2 factories);
   `support_equipment_1` once engineers/recon companies are in a template.

## construction

`construction_min_idle: 1` because only 2-9 civilian factories are ever free in this
window (4 would never trigger). One level per order.

| Job | States | Reason |
|---|---|---|
| 1. `industrial_complex` | 1036 Wuhan, 602 Hunan, 750 Changde, 620 Hubei, 600 Jiangxi, 749 Huangshan, 603 Guizhou, 748 Zunyi | Inland CHI-owned states (about 20 free slots); away from the Shanghai-Nanjing coast Japan lands on. Do not build in 613, 1034, 1035, 596, 598, 595, 743. |
| 2. `arms_factory`, `fallback_any_own_state` | 620, 603, 1036, 602, 750, 600, 748 | Inland. |
| 3. `infrastructure` | 620, 600, 603, 748 (level 1), 1036, 602, 750, 607, 606 | Sink and supply for the Henan/Anhui front. |

Job 2 is never reached while job 1 has slots. **Manual switch:** around 1937-03 (or as
soon as Japan's Beijing garrison focus or the Kwantung mission is near), move job 2 in
front of job 1. Bunkers are province buildings and are not automated: build them by hand
on river lines (Yellow River crossings in Henan 607, the Yangtze above Nanjing, Wuhan
approaches) after `basic_fortification_tech`.

## 1937 war-start checklist (Marco Polo, `china.1`)

Timing: Japan's Kwantung mission (`JAP_kwantung_army_impatience`, 725 days) fires
`china.1` by ~1937-12-27; Japan's `JAP_reinforce_the_beijing_garrison` focus can fire it
earlier. CHI receives it because HBC holds Beiping/Hebei and CHI holds Qingdao 743.

Before (from ~1937-03):
1. Construction job 2 (arms) first; bank 150+ PP; rifles on every free military factory.
2. Front lines: Henan 607 and Anhui 606 (Yellow River / Longhai railway) with 2 armies;
   2-3 divisions each in Shanghai 613, Nanjing 1035, Zhejiang 596, Fujian 595, Qingdao 743,
   Jiangsu 598; a reserve army at Wuhan 1036. Beiping/Hebei/Shandong are HBC/SND land: do
   not plan to hold them with CHI divisions.
3. Relocate MIOs out of Shanghai and Nanjing by hand (50 PP each, 35 days, aborted if the
   state is lost).

At the event (option 0 is set in the plan):
4. Focus slot: the runner takes Demonstrate our Resolve, Gung Ho, Secure the Soviet
   Border, Send the First Lady Abroad in that order. If the Three-Year Plan is running or
   not done, bypass it by hand so Fund Domestically stays reachable.
5. By hand: `CHI_demonstrate_our_resolve_decision` (25 PP), `CHI_bog_them_down` (25
   command power, needs Bog Them Down in the Centre) and `CHI_declare_war_zone` (command
   power, read the tooltip) on the threatened states; read `CHI_breach_the_yellow_river`'s tooltip before ever using it.
6. Laws: War Economy needs war support > 0.5 and a war with a country at IC ratio
   above 0.4, or fascism/communism; take it the moment the tooltip allows. Raise
   conscription only when rifles exist.
7. Train: raise `queue` only with rifle surplus; move new divisions to the Wuhan reserve.
8. Do not counter-attack across open plains; hold rivers, keep a supplied reserve,
   counter every coastal landing before it is supplied.

## What could not be verified from the files

* Starting PP and exact daily PP (history sets none); read `hoi4_economy` on day one.
* Whether `hoi4_politics_choose` wants the character id (`CHI_chen_guofu`) or the
  idea token (same string here).
* Which warlords accept the faction invitation (AI choice).
* Exact research durations and focus-granted slot counts (random states).
* Whether the runner's template match handles the accented `Juntuán Reformed` name; if
  the tool lists it differently, copy the name it reports.
