# Nationalist China FAIR standing plan, 1936-01-01 to 1938-01-01

Companion to `china-fair-1936-1938.plan.json` (JSON has no comments, so every
section is explained here). Copy the JSON to the `plan.json` the runner reads
(see `native/src/plan_run.rs`; the file is re-read every tick). Keys that start
with `_` are ignored by the runner; `_validator` only feeds
`scripts/validate_plan.py`.

```
python3 scripts/validate_plan.py skills/hoi4-campaign-playbook/plans/china-fair-1936-1938.plan.json --strict
checked: research=30, focus=10, production=2, construction=3, politics=2, decisions=0; 0 error(s), 0 warning(s)
```

## The DLC set decides everything

The running game's log (`~/Documents/Paradox Interactive/Hearts of Iron IV/logs/system.log`,
"Active DLC Count: 3") shows only **Anniversary Pack, Poland: United and Ready, Rocket
Launcher Unit Pack** active. Waking the Tiger, No Compromise No Surrender, By Blood Alone,
La Resistance, No Step Back, Arms Against Tyranny and Gotterdammerung are installed
folders but inactive (`dlc_load.json` does not prove activation). So:

* CHI gets **`generic_focus`** (`common/national_focus/generic.txt`, factor 1, default).
  `china_nationalist_sea_focus` needs NCNS (weight 1000) and `china_nationalist_focus`
  needs Waking the Tiger (weight 10). No `CHI_*` focus, no warlord faction or integration
  system, no Gung Ho, no Soviet/American support focuses.
* Every `has_dlc` branch for the inactive DLCs is false in history, states, characters,
  events and decisions.

`_validator.dlcs` records the set. With it, `validate_plan.py` picks the focus tree the game
really loads for the tag (a focus from another tree is an error), follows the history
file's if/else branches for start techs and the start OOB, drops advisors in disallowed
character `instance` blocks, and warns about construction/deploy states the tag does not
own at the start. If a DLC is enabled later, update `dlcs` and re-validate: the plan will
fail loudly instead of silently using the wrong tree.

## Start position (verified for this DLC set)

* 28 states (no NCNS warlord transfers: Beiping 608, Hebei 614, Shandong 597/1038,
  Chongqing 1037, Chengdu 1041, Xi'an 744 are CHI). **16 civilian, 11 military, 2 naval
  factories.**
* Production (`history/units/CHI_1936.txt`): one line, `infantry_equipment_0` on 2 factories,
  so **9 military factories are idle on day one**.
* Army: 57 divisions: 28 "Juntuán" (4 inf, garrison), 24 "Sanjiao Jun" (6 inf), 5 "Qibing Jun"
  (cavalry).
* Laws: `free_trade` (added by history), `civilian_economy`, volunteer only. Laws cost a plain
  150 PP.
* Permanent spirits (no generic-tree way to remove them): `CHI_army_corruption_1` (attack and
  defence -30%, mobilisation speed -35%), `CHI_ineffective_bureaucracy` (conscription -35%),
  `CHI_incompetent_officers` (command power gain -70%), `CHI_hyper_inflation_1` (consumer goods
  +10%, factory output -5%), `german_advisors` (land doctrine cost -10%).
* Consumer goods: 35% (civilian economy) + 10% (hyper-inflation) = 45% of 27 factories, so about
  12 of 16 civilian factories go to consumer goods and **~4 are free** on day one.
* Start techs: infantry_weapons, gw_artillery, interwar_antiair, basic_train, gwtank,
  early_fighter, early_bomber, pre-Man-the-Guns ships. **No tech_support or tech_trucks.**
  2 research slots.

## Runner behaviour that shapes the plan

* One action per tick: events, research, focus, politics, decisions, training,
  military-factory rebalance, `production` lines, construction, armies. A refused action waits
  1-3 game days.
* Focus/research: the first not-completed key the game accepts. Politics: picks are tried in
  list order every tick and a refused pick waits a day.
* `stop_on_date` pauses the game on 1938.01.01.

## Top-level keys

| Key | Value | Why / when to deviate |
|---|---|---|
| `active`, `stop_on_failure` | true / false | A failed action must not freeze the routine. |
| `stop_on_date` | `"1938.01.01"` | Window end. |
| `auto_events`, `wait_events_days` | true / 3 | Single-option events get option 0; multi-option ones wait 3 days for the model. |
| `event_options` | `china.1: 0` | Marco Polo Bridge (`events/China.txt`). Option 0 "Breaking point!": Japan declares `annex_everything` on CHI (and SHX/SND/HBC if they exist). Option 1 hands Japan Beiping 608, Shandong 597, Jinan 1038, Hebei 614 and Qingdao 743 for a 180-day truce. The FAIR plan fights. |
| `alarms` | PP > 200, consumer goods > 0.45, idle civ >= 3, idle mil >= 2, unassigned >= 2 | PP 200 = a law plus margin unspent. |

## research_priority (30 technologies)

Two slots, so about 14-18 techs finish in two years; entries past ~18 are tail.

| Positions | Block | Reason |
|---|---|---|
| 1-3 | infantry_weapons1, construction1, basic_machine_tools | Rifle model (`infantry_equipment_1`), +10% construction, efficiency cap. The Industrial Effort bonus (+100%, industry, one use, ~03-11) lands on whichever industry tech starts next. |
| 4-7 | tech_support (missing 1918 tech), tech_engineers, support_weapons, interwar_antitank | Support companies are impossible without tech_support; engineers for river defence; anti-tank for Japanese armour. |
| 8-13 | concentrated_industry, construction2, interwar_artillery, electronic_mechanical_engineering, basic_fortification_tech, tech_recon | +15% factory output and +20% building slots; 1937 construction; fort tech before bunkers. |
| 14-30 | improved/advanced machine tools, concentrated 2/3, radio, excavation, support_weapons2, infantry_weapons2, land_fort_tech_1, artillery1, antitank1, tech_engineers2, conversion, computing, construction3, improved_infantry_weapons | 1938-39 tail so the list never runs dry. |

Equipment Effort (~1937-07) gives +100% to one infantry-weapons or artillery tech: have
infantry_weapons2 or artillery1 ready to start then.

## focus_path (10 generic focuses, 70 days each) and focus_fallback

| # | Focus | Done about | Reward |
|---|---|---|---|
| 1 | industrial_effort | 1936-03-11 | +100% research bonus, one industry tech |
| 2 | construction_effort | 05-20 | +1 civilian factory and +1 slot in a random home state |
| 3 | construction_effort_2 | 07-29 | +1 civilian factory |
| 4 | infrastructure_effort | 10-07 | +1 infrastructure in two random states |
| 5 | construction_effort_3 | 12-16 | +2 civilian factories |
| 6 | production_effort | 1937-02-24 | +1 military factory |
| 7 | army_effort | 05-05 | 25 army XP, -50% one land doctrine |
| 8 | equipment_effort | 07-14 | +100% one infantry weapons/artillery tech |
| 9 | production_effort_2 | 09-22 | +1 military factory |
| 10 | political_effort | 12-01 | +120 PP (bank for war laws) |

Civilian first because each instant factory is worth about a year of China's construction
(4-7 free civs x ~5 IC/day against 10,800 per factory). If Japan attacks before 1937-03,
move production_effort and army_effort ahead of construction_effort_3.
`focus_fallback`: production_effort_3, doctrine_effort, equipment_effort_2, doctrine_effort_2,
equipment_effort_3, motorization_effort.
Never: the collectivist/nationalism/militarism chain (fascism drift on a neutral China,
little value) or the navy/aviation branches in this window.
Note: the generic factory rewards go to a **random** home state with a free slot, which may
be a coastal state Japan will take.

## politics

| Pick | Cost | Why |
|---|---|---|
| government 2 `partial_economic_mobilisation` | 150 PP | War support 0.32 > 0.25 qualifies on day one. Consumer goods 35% -> 25% (about 2.7 factories), no construction penalty, mil construction +10%; strictly better than Early Mobilisation. |
| government 3 `CHI_h_h_kung` | default advisor cost (check in game) | Captain of industry: civilian factory, infrastructure, railway construction +10%. Allowed without NCNS and has no focus condition. |

Already held: `free_trade`. Not picked: conscription laws (volunteer pool of 28 states is
enough for the rifles we make). By hand in `military_staff` (no verified slot numbering): army
chief `CHI_li_zongren` (defensive 3, 200 PP) or `CHI_he_yingqin` (100 PP); high command
`CHI_xiao_yisu` (infantry 2). Soong Mei-ling (first lady: stability and war support +5%) is a
cheap second political advisor once PP allows.
Unavailable in this install: Chen Guofu, Chen Lifu, Du Yuesheng, Weng Wenhao, T. V. Soong
(their roles need NCNS or old-tree focuses), Falkenhausen as advisor.

## decisions

Empty. The CHI war decisions (war zones, bog them down, demonstrate resolve, evacuations,
MIO relocation) belong to the NCNS / Waking the Tiger content or need its focuses.
Generic `war_propaganda` (150 PP, visible at war) is a by-hand option for war support.

## training and armies

* `training`: 1 line of the stock `Sanjiao Jun` (6 infantry; in `CHI_1936`), deploying in Henan 607
  (Zhengzhou junction behind the northern front). The 57 starting divisions are short of rifles;
  raise `queue` only with a stockpile above ~3k and the field army above ~80% equipment. Build a
  custom template (e.g. 6 infantry + engineer + artillery support) by hand after tech_support and
  tech_engineers.
* `armies`: armies of at most 24, absorb on, generals for armies of 8+, by English display name:
  Hu Zongnan (4), Sun Li Jen (4), Wei Lihuang, Gu Zhutong, Fu Zuoyi, Tu Yu-ming, Tai An-lan (3),
  field marshals Hsueh Yueh and Zhang Xueliang (3), Wang Yao-wu (2).

## production

Day one: 9 idle military factories. `production_min_idle: 10` keeps the rebalance off so the two
lines take exactly the 9: a second `infantry_equipment_0` line (6) and `artillery_equipment_1` (3).
Rifles run on 8 factories, artillery on 3.

Manual edits:
1. Once both lines exist, set `production_min_idle` to 2.
2. After `infantry_weapons1` (~mid 1936) switch rifle lines to `infantry_equipment_1`
   (`hoi4_production_set`).
3. After `interwar_antitank` add `anti_tank_equipment_1`; after `tech_support`,
   `support_equipment_1` for support companies.

## construction

`construction_min_idle: 2` (only ~4 civs free before the law, ~7 after). One level per order.

| Job | States | Reason |
|---|---|---|
| 1. `industrial_complex` | 1037 Chongqing, 1036 Wuhan, 1041 Chengdu, 602 Hunan, 750 Changde, 620 Hubei, 600 Jiangxi, 744 Xi'an | Interior states (about 25 free slots), Chongqing/Chengdu being the fallback capital region. Nothing on the coast (613, 1034, 1035, 596, 598, 595, 743, 597) or in the north (608, 614, 1038). |
| 2. `arms_factory`, `fallback_any_own_state` | 1037, 620, 603, 1036, 602, 750, 1041, 605 | Interior. |
| 3. `infrastructure` | 620, 600, 603, 748, 1036, 1037, 602, 750, 607, 606 | Infra-1 states first, then supply for the Henan/Anhui front. |

Job 2 is reached only when job 1's states are full, which will not happen in this window.
**Manual switch** around 1937-03 (or as soon as Japan takes `JAP_reinforce_the_beijing_garrison`):
move job 2 ahead of job 1. Bunkers are province buildings and not automated: build them by hand
on the Yellow River crossings in Henan 607, around Wuhan and on the Yangtze above Nanjing after
`basic_fortification_tech`.

## 1937 war-start checklist

Timing: Japan's tree (`japan_wtt_focus`) loads without DLC. `china.1` comes from Japan's
`JAP_reinforce_the_beijing_garrison` focus (AI usually mid-1937) or the Kwantung mission
timeout (725 days) if Japan's focuses started it. There is no guaranteed date: watch Japan's
focus and the news.

Before (from ~1937-03):
1. Construction job 2 first; every free military factory on rifles; bank 150+ PP
   (political_effort adds 120).
2. Front: Beiping 608 and Hebei 614 are CHI but open plain next to Japanese Manchukuo/Jehol: screen
   them, hold the main line on the Yellow River (Henan 607, Jinan 1038) with 2 armies; 2-3
   divisions each in Shanghai 613, Nanjing 1035, Qingdao 743, Zhejiang 596, Fujian 595, Jiangsu 598;
   a reserve army at Wuhan 1036. Army corruption (-30% attack and defence) is permanent: fight from
   rivers, forts, hills, never attack across plains.
3. Japanese landings: counter-attack before the beachhead is supplied.

At the event (option 0 is set):
4. Laws: War Economy needs war support > 0.5 and a war with a country at IC ratio above 0.4 (or
   fascism/communism); take it when the tooltip allows. Raise conscription only with rifle surplus.
5. Focus slot keeps running the generic path; swap production/army focuses forward if not done.
6. Train only with rifle surplus; new divisions to the Wuhan reserve.

## What could not be verified from the files

* Starting PP and H. H. Kung's actual cost (no cost in his non-NCNS instance); read
  `hoi4_politics_options` on day one.
* Whether Japan's AI takes the Beijing garrison focus in 1937 in this DLC set.
* Which home states the random generic focus rewards pick.
* Whether the runner matches the accented "Juntuán" names; `Sanjiao Jun` was chosen partly because
  it has no accent.
