# Templates, equipment and production (HOI4 1.19.3, FAIR)

Source of truth: the installed game (`Hearts of Iron IV/common/...`, paths below
are relative to it). Numbers marked **[V?]** are version-dependent or come from
the wiki (page cites 1.16) and must be re-checked in game. "IC" is the game's
build cost unit. Equipment IC and division IC below are **equipment only**
(manpower is separate). Division totals were computed from file values with a
script; the in-game tooltip is authoritative and may differ by a few percent.
Overlaps with `economy-build-orders.md` and `supply-logistics-deployment.md`:
here only templates, equipment, production lines and training are covered.

## 1. Combat width

- Terrain base width (`common/terrain/00_terrain.txt`): plains, hills, desert
  **70**; forest, jungle **60**; mountain, marsh **50**; urban **80**. Each extra
  attack direction adds half a base width (+35 on plains) *(user-verified)*.
- Defines (`common/defines/00_defines.lua`): `COMBAT_STACKING_START = 5`
  (a stacking penalty of -2% per division starts above 5, +3 per extra
  direction), `COMBAT_OVER_WIDTH_PENALTY = -1`, max `-0.33` **[V?]** (overflow
  hurts, capped at about -33%), `ENGAGEMENT_WIDTH_PER_WIDTH = 2.0` (a unit may
  engage up to twice its own width of enemy front). The front line is filled
  first-come; divisions that do not fit wait as reserve and rotate in when
  front divisions lose organisation.
- Battalion widths: infantry, motorized, mechanized, cavalry, mountaineers,
  light/medium/heavy armour **2**; artillery brigade **3**; AT brigade and
  AA brigade **1**; tank destroyer / SP artillery (armour group) **2**;
  support companies **0**; regimental support **0** (explicitly adds no width
  and no speed, see `localisation/english`). Division width is the sum of the
  battalion widths in the 5x5 grid.
- Designer limits: `MAX_DIVISION_BRIGADE_WIDTH/HEIGHT = 5/5`, minimum height 4,
  **5 divisional support slots** (width 1, height 5), and a new
  **regimental support row of 5 slots**, one per regiment column; a column needs
  at least 3 battalions to carry one (`REGIMENTAL_SUPPORT_REQUIRED_BATTALIONS`).
  Regimental support items: `field_guns` (6 artillery eq), `rocket_battery`,
  `anti_air_battery` (16 AA eq), `anti_tank_battery` (16 AT eq), `fire_support`
  and `mot_fire_support`, plus SP AA and tank-destroyer supports on armour
  columns. The game's own AI templates (`common/ai_templates/generic.txt`)
  use 9 infantry + 1-2 artillery brigades + engineer/recon/artillery/logistics
  and regimental AT, AA and fire support.

### Width design rules (resolve from terrain, not folklore)

| Front terrain | Base | Clean fits | Note |
|---|---|---|---|
| Plains/hills/desert | 70 | 3x20 leaves 10 empty; 2x35; 5x14; 7x10 | Second direction: 105 = 5x21 or 3x35 |
| Forest/jungle | 60 | **3x20**, 2x30, 4x15 | 20-wide is perfect here |
| Mountain/marsh | 50 | 2x25, 5x10, 3x16 | Prefer small 10-16 wide mountaineers |
| Urban | 80 | **4x20**, 2x40 | 20-wide again |

Role guidance (contested community advice, so a heuristic): **20-21 wide**
(7 inf + 2 art, or 6 inf + 3 art) is the default line division: it divides 60
and 80 exactly, is cheap to replace and spreads damage. **10-16 wide** for
defending mountains, marsh and garrison. **30-35 wide** for concentrated
attacks and armour spearheads, because a wide division brings more guns into
one combat. **40+ wide** only with high industry; it is slow to retrain, loses
whole divisions on a bad fight and wastes width on narrow fronts. Armour and
motorized groups at about 20-24 width keep organisation and supply manageable.
Attackers want width filled (3 divisions of 20-21 on plains plus a 4th as the
reserve); defenders benefit more from several small divisions on one line than
from one wide one (more reserve rotation). **[V?]**

## 2. Equipment statistics (per item, `common/units/equipment/*.txt`)

| Item | Year | IC | Key stats (soft/hard/AP, def, breakthrough) | Steel etc. |
|---|---|---|---|---|
| infantry_equipment_0 | 1918 | ~0.43 | pre-1936 rifle (China starts on it) | steel 2 |
| infantry_equipment_1 | 1936 | 0.50 | 6 / 1 / 4, def 22, BT 3 | steel 2 |
| infantry_equipment_2 | 1939 | 0.58 | 9 / 1.5 / 5, def 28, BT 4 | steel 3 |
| infantry_equipment_3 | 1942 | 0.69 | 12 / 2 / 10, def 34, BT 5 | steel 4 |
| support_equipment_1 | 1918 | 4.0 | shared by all support companies | steel 2, aluminium 1 |
| artillery_equipment_1/2/3 | 1934/39/42 | 3.5/4.0/4.5 | soft 26/30/34, hard 2/3/4, def 11/15/19, BT 10/13/16 | tungsten 1, steel 2-3 |
| anti_tank_equipment_1/2/3 | 1936/40/43 | 4/5/6 | hard 20/22/30, AP 60/90/125 | tungsten 2, chromium 1 from II |
| anti_air_equipment_1/2/3 | 1936/40/43 | 4/5/6 | air 19/25/32, AP 25/60/88 | steel 2-3 |
| motorized_equipment (truck) | 1936 | 2.5 | speed 10-12, fuel 1.2, also the supply truck | steel 1, rubber 1 |
| mechanized_equipment_1/2/3 | 1940/42/44 | 8/10/12 | def 26/30/34, armour 10/15/20 | steel 2-4, rubber 1 |
| light tank chassis 2/3 | 1936/42 | 2.7/3.0 + modules | armour 20/25 | steel 1-2 |
| medium chassis 1/2/3 | 1936/40/43 | 3.75/4.0/4.25 + modules | armour 30/40/50 | steel, tungsten, chromium by modules |
| heavy chassis 2/3 | 1940/43 | 11/12 + modules | armour 60/75 | |
| convoy | 1910 | 70 | built on dockyards, max 15 per line | steel 1 |
| airframes small/medium/large (gen 1/2/3) | 1936/39/42 | 5/6/7, 8/10/13, 18/20/22 + modules | | |

Tank variants are modular (`modules/00_tank_modules.txt`): final IC is chassis
+ module costs, multiplied by the armour type (riveted -20%, cast +20%). From
the module files: a 1936 light tank (HMG, one-man turret, bogie, riveted,
gasoline) is about **4.2 IC**; a 1939-41 medium with small/medium cannon,
torsion bar, riveted or cast armour is about **7-13 IC**; a 1943 medium with a
long gun is about **12-13 IC**; a 1941-43 heavy about **27-29 IC**. Adding a
module costs 5 army XP, replacing 6, converting a related module 3, removing 1
(`EQUIPMENT_MODULE_*_XP_COST`). **[V?]** Plane modules were not itemised: read
the cost from the designer.

## 3. Battalions (`common/units/*.txt`)

| Battalion | Width | Org | Manpower | Train days | Equipment per battalion | Supply use | Notes |
|---|---|---|---|---|---|---|---|
| Infantry | 2 | 60 | 1000 | 90 | 100 inf | 0.06 | cheapest, 58 IC at 1939 gear |
| Cavalry | 2 | 70 | 1000 | 120 | 120 inf | 0.06 | speed 0.6, weak in forest/hills/mountain |
| Mountaineers | 2 | 70 | 1000 | 120 | 140 inf | 0.05 | +35% attack, +10% defence mountain; +20% attack hills; breakthrough +0.3 |
| Motorized | 2 | 60 | 1200 | 90 | 100 inf + 35 trucks | 0.065 | 1936; forest/jungle/marsh movement -50/-30% |
| Mechanized | 2 | 60 | 1200 | 120 | 100 inf + 40 mech | 0.14 | tech 1940; own soft attack +0.1, hard +4 |
| Artillery brigade | 3 | 0 | 500 | 120 | 24 art | 0.21 | mot. version 24 art + 36 trucks |
| Anti-tank brigade | 1 | 0 | 500 | 120 | 36 AT | 0.10 | |
| Light / medium / heavy armour | 2 | 10 | 500 | 180 | 60 / 50 / 40 chassis | 0.22 / 0.25 / 0.32 | breakthrough 0.15, suppression 2.5 |
| Medium tank destroyer | 2 | 0 | 500 | 180 | 40 TD chassis | 0.22 | tech 1939 |
| Medium SP artillery | 2 | 0 | 500 | 180 | 40 chassis | 0.46 | tech 1939 |
| Armoured car | 2 | 20 | 500 | 180 | 60 cars | 0.14 | |

Division organisation is the average of its line battalions (infantry about
60, mountaineers 70, armour-heavy templates about 30-45); armour and
artillery add punch but little staying power. Division training time is the
longest component: 90 days for pure infantry or motorized, **120 days** with
any support company, artillery or mechanized, **180 days** with any armour. A
division also needs its full equipment before it can be fielded.

### Support companies (`common/units/{engineer,recon,logistics,maintenance,field_hospital,signal,military_police,flame_tank}.txt`)

Cost = own equipment at 1939 prices. All have width 0 and are limited to one of
each type (`same_support_type`) and 5 per division. Compare an infantry
battalion at 58 IC: **one support company costs 1-3 battalions**.

| Company | Tech | Equipment | ~IC | Effect (from files) | Take it when |
|---|---|---|---|---|---|
| Engineer | tech_engineers (start) | 10 inf, 30 support | 125 | entrenchment +2, +20% entrenchment on light infantry, defence +25% in forest, jungle, marsh and river, attack +10% vs forts, +25% river defence | Always on attack and defence divisions; first slot |
| Recon | tech_recon (start) | 40 inf, 10 supp (mot. variant +20 trucks) | 110 | recon skill 1 (better tactic pick), +10% artillery soft attack, speed | Second slot for line divisions that fight enemies with good tactics; mot_recon for armour |
| Artillery (company) | start | 12 art | 48 | adds a little fire without width | Cheap extra firepower; first choice in the early 9-infantry template |
| Anti-tank / anti-air company | 1939 | 24 AT / 24 AA | 120 / 120 | hard attack, air attack | Only against heavy enemy armour or air; regimental versions are 16 eq and cheaper |
| Logistics | tech_logistics_company | 20 supp, 10 trucks | 105 | **-10% division supply use**, -5% fuel | Mobile or far-from-rail divisions; skip in static defence |
| Maintenance | tech_maintenance_company | 25 supp | 100 | reliability +5%, 5% captured equipment | Armour/mechanized only; armour variant for tank-heavy |
| Field hospital | tech_field_hospital | 30 supp, 20 trucks | 170 | casualty trickleback 20%, XP loss -15%, infantry strength +10% | Long wars and elite/veteran divisions; poor value for cheap garrisons |
| Signal | tech_signal_company | 20 supp, 10 trucks | 105 | initiative +0.1 | Attack divisions in the spearhead |
| Military police | tech_military_police | 10 supp, 40 inf | 60 | suppression +0.2, +20% morale for infantry battalions | Garrison templates, the only support that matters there |
| Flame tank | special project | chassis | high | terrain attack bonuses; poor stats | Skip |

Support companies share one pool (`support_equipment`, 4 IC each), so adding
engineer + recon + logistics + hospital + signal to a 100-division army needs
about **13,000** support equipment, nearly 52,000 IC. Treat the fifth slot as
optional.

## 4. Named templates (all values computed from the files)

Notation: `Wnn` width, `MP` manpower, `IC` equipment IC at the stated gear
level, `Org` division average, `Sup` summed battalion supply use (no unit
tooltip scale). Equipment totals are per division. Regimental support goes in
the single row under the regiment columns (needs 3+ battalions in that column).

**T1 INF-18 (1936 start, vanilla Germany/China/Poland)**: 9 infantry (3 columns
x 3), support engineer + artillery. W18, MP 9,600, Org 60, Sup 0.72,
**inf 910, support 30, art 12**, IC ~620. The game's own starting German
division. Poland's variant uses recon instead of engineer/art (IC ~510).

**T2 INF-20 (1939, the default line)**: 7 infantry + 2 artillery brigades, support
engineer + recon + artillery. W20, MP 9,100, Org 60, **inf 750, art 60,
support 40**, IC ~835. Shares: infantry 52%, artillery 29%, support 19%.

**T3 INF-20 late (1942-44)**: T2 plus regimental anti-tank battery x2 + field guns
x1 and an AA company. W20, MP 9,940, **inf 750, art 66, AT 32, AA 24, support
40**, IC ~1,310 with gen-3 gear. Add regimental AT only once the enemy fields
armour; drop AA company if you hold air superiority.

**T4 INF-21 (1941+ breakthrough infantry)**: 6 infantry + 3 artillery brigades
(W21), support engineer, recon, field hospital, logistics, maintenance; regimental
AT + field guns. MP 10,160, inf 650, art 78, AT 16, support 115, trucks 30,
IC ~1,430. Better attack per width; uses 30% more artillery.

**T5 GARRISON-8 (any year)**: 4 infantry in one column + military police. W8, MP
4,500, Org 60, inf 440, support 10, **IC ~260**, Sup 0.26. Use a W8-10 template
for occupation (suppression comes from infantry 1.5 + MP 0.2). Never
engineer/recon/artillery on garrisons. 100 such divisions need 44,000 inf eq.
The game's own garrison score is `(IC + manpower x 0.05) / suppression`
(`GARRISON_TEMPLATE_SCORE_*`), lower is better.

**T6 MTN-18 (1936)**: 9 mountaineers, engineer + recon. W18, Org 70, inf 1,310
(140 each), IC ~815. For mountain and hill fronts only (Alps, Pyrenees, Carpathians,
Caucasus). W20 variant: 7 mountaineers + 2 artillery, IC ~1,000. Costs about
40% more equipment than plain infantry: use 6-9 divisions, not more.

**T7 MOT-19 (1936 motorized)**: 8 motorized + 1 motorized artillery brigade, mot.
recon + engineer + artillery. W19, MP 11,200, Org 60, inf 850, **trucks ~326**,
art 36, IC ~1,600. Trucks (35 per battalion) are the bottleneck: allow the
truck line 8-10% of military factories early.

**T8 MECH-19 (1942+)**: 8 mechanized + 1 motorized artillery; recon, engineer,
maintenance, logistics. W19, MP 11,900, mech 320, IC ~4,350, supply 1.4.
Expensive and steel/rubber-hungry: build a few for spearheads, not the army.

**T9 PANZER-12 (1936 German start)**: 4 light armour + 2 motorized, support mot.
recon + engineer + artillery. W12, MP 5,500, Org ~27, light tanks 240, trucks ~80,
inf 250, IC ~1,500. Fine for 1936-38 only; the German AI target template moves from
this to medium armour in 1939.

**T10 PANZER-16 (1939 medium)**: 3 medium + 1 light + 4 motorized, support mot. recon,
engineer, artillery, signal. W16, MP 8,400, Org ~35, medium 150, light 60,
inf 450, trucks ~160, IC ~2,500.

**T11 PANZER-21 (1941-43)**: 4 medium armour + 5 motorized + 1 motorized artillery;
support mot. recon, engineer, signal, logistics, maintenance; regimental AT.
W21, MP ~11,000, Org ~38, **medium tanks 200**, inf 550, trucks ~241, support
105, IC ~4,100 at ~12.5 IC per tank. Add 1 medium TD brigade for 2 columns
facing tanks (the AI does this around 1940+).

**T12 HEAVY-20 (1943+)**: 3 heavy + 6 mechanized + 1 SP artillery brigade; recon,
engineer, maintenance, logistics, signal. Heavy tanks 120, mech 240, IC ~7,000,
Org ~43. Costs about 5x a line division; only a few for breakthrough
(tanks 40/battalion at ~27 IC each).

**T13 TD-support 20**: T3 with one medium TD brigade (40 chassis ~10-14 IC each)
instead of two artillery brigades: IC ~1,440. Use only as the anti-armour
screen division in the line; regimental AT batteries (16 AT eq, ~100 IC) are
usually the cheaper answer.

**T14 ART-BREAKTHROUGH-20 (1942+)**: 4 infantry + 4 artillery brigades: W20 (8+12), support engineer, recon, artillery, signal, field hospital;
regimental AT. Art 108, inf 450, IC ~1,330. Soft attack mass for set-piece
battles. Poor org after a lost battle.

**T15 CAV-8 / marines (FAIR)**: 4 cavalry + recon + artillery is W8 and IC ~340; it
is only a scouting/garrison screen. Marines are allowed only as ordinary
historical amphibious infantry **one landing at a time, never spam or
space-marine stacks** (`fair_prohibited` in `hoi4_live/game/play_policy.json`).

### Evolution path (recommended by era; changes are template edits, see 9)

| Date | Line infantry | Armour | Support slots |
|---|---|---|---|
| 1936 | T1 9 inf + eng + art (W18) | T9 | engineer, artillery |
| 1937-38 | T2 7/2 + eng, rec, art (needs no extra tech) | T9 | + recon |
| 1939 | T2, equipment II | T10 | + signal/logistics where tech exists |
| 1940-41 | T3 with AT battery (antitank1 1939, antitank2 1940) | T11 | + maintenance (armour), logistics |
| 1942 | T3/T4 equipment III (infantry_equipment_3, artillery_3) | T11 on medium II | + hospital |
| 1943-44 | T4 plus AA/AT | T11 or T12 | full 5 slots on elite divisions |

Tech years from `common/technologies`: infantry_weapons1 1936, weapons2 1938,
artillery2 1940, artillery3 1941, artillery4 1942, antitank1/2/3 1939/40/41,
antiair1/2/3 1939/40/41, motorised 1936, mechanised 1940, basic medium tank
1939, improved medium 1941, advanced medium 1943, basic medium TD 1939, rocket
artillery 1940. Researching early costs a penalty; take it when a template is
waiting for it.

## 5. Production lines and efficiency

From `common/defines/00_defines.lua` (NProduction) and the wiki **[V?]**:

- Military factory output: **3.5 IC/day unpowered, 4.5 powered**; civilian 4/5;
  dockyard 2/2.5. This build has an **energy** system (coal gives energy:
  `BASE_COUNTRY_ENERGY_PRODUCTION = 10`, per-factory cost 0.25 scaled, cap
  6.6, coal x 9): check energy before adding factories, an energy-short state
  loses the +1 output **[V?]**.
- Start efficiency **10%**, base cap **50%**; each of six industry techs
  (basic/improved/advanced machine tools 1936/37/39, assembly line 1941,
  flexible line and streamlined line 1943) raises the cap by 0.1 **[V?]**
  (whether additive points or a multiplier was not confirmed). Dispersed
  industry +5% start efficiency.
- Growth (wiki 1.16 formula **[V?]**): daily gain = 0.001 x cap^2 / current
  (fraction). That gives a 50% cap line about **105 days to 25%**, **~300 days
  to 40%**, and nearly 500 days to the cap. Treat lines as year-long
  investments.
- Efficiency is tracked per factory slot: **new factories start at 10%**, old
  ones keep theirs. Unused slots lose 1 efficiency point per day.
- Retention when changing a line (defines: variant 90%, same family 70%,
  parent/child 30%, archetype 20%; wiki: variant 90, same chassis other model
  70, direct upgrade 30, indirect 20, other 10; floor 10% start) **[V?]**.
  Flexible line (+0.2 line-change factor) softens it. Read the preview in the
  game before changing.
- Max factories per line: 150 military, 15 convoys, 10 screens/subs, 5 capital
  ships. Naval and convoy lines need dockyards and are not affected by
  production efficiency.
- Resource shortage: `PRODUCTION_RESOURCE_LACK_PENALTY = -0.05` per factory lacking
  a resource (a few missing units slow every factory on that line).
- Conversion: obsolete or captured equipment of the same chassis can be converted
  without efficiency loss (cost `0.9 x chassis` plus modules **[V?]**).
- Licences: base 1 civilian factory +1 per year after 1936, output -25%
  (`LICENSE_EQUIPMENT_BASE_SPEED`), -10% more out of faction, -50% with no
  licence, 30-day minimum, double XP to upgrade. Use only to fill a gap
  (artillery, AA, trucks) when allied.
- Lend-lease: deliveries every 30 days (`LEND_LEASE_DELIVERY_TOTAL_DAYS`); lend
  cost per item e.g. infantry 1, artillery 4, trucks 5, tanks 10+. Use
  received stock for garrisons; do not plan on it.

### Rules for opening or changing lines

1. **One line per base equipment type**: infantry, support, artillery, AT, AA,
   trucks, each tank class. Open a new line only when the stockpile
   target cannot be met by the old one.
2. **Do not switch a mature line for a marginal upgrade.** A direct upgrade
   keeps only about 30% efficiency **[V?]**. Switch infantry equipment I->II (1939)
   when the line is young or the new gear brings +50% soft attack (II) / +100%
   (III). Late stock converts: older gear remains valid in divisions that ask
   for the archetype.
3. **Grow a line gradually**: every new factory is at 10%; adding 20 at once to
   a line with 60 existing factories only lowers the average if the line was
   at the cap. Add in steps of 5-10.
4. **Keep lines non-empty**: a line with 0 factories and no progress is wasted;
   keep unused slots from sitting (1 efficiency point lost per day).
5. **Production queue sanity**: if `hoi4_production_lines` shows a line with
   damaged or requested factories > active, there is a shortage (resource,
   energy or bombed factories). Fix the shortage before adding lines.

## 6. Equipment per division and per 100 divisions

| Template | Inf eq | Support | Art | AT | AA | Trucks | Mech/tanks | IC |
|---|---|---|---|---|---|---|---|---|
| T1 INF-18 1936 | 910 | 30 | 12 | | | | | ~620 |
| T2 INF-20 1939 | 750 | 40 | 60 | | | | | ~835 |
| T3 INF-20 late | 750 | 40 | 66 | 32 | 24 | | | ~1,310 |
| T5 GARRISON-8 | 440 | 10 | | | | | | ~260 |
| T6 MTN-18 | 1,310 | 40 | | | | | | ~815 |
| T7 MOT-19 | 850 | 40 | 36 | | | 326 | | ~1,600 |
| T8 MECH-19 | 850 | 85 | 24 | | | 46 | 320 mech | ~4,350 |
| T11 PANZER-21 | 550 | 105 | 24 | 16 | | 241 | 200 medium | ~4,100 |
| T12 HEAVY-20 | 650 | 105 | | | | 20 | 120 heavy + 240 mech + 40 SP | ~7,100 |

**Per 100 divisions multiply by 100.** Example (T2, 1939 gear): 75,000 infantry
equipment (43,500 IC), 6,000 artillery (24,000 IC), 4,000 support (16,000 IC);
**about 83,500 IC**.

Worked formula: monthly IC = factories x 4.5 x efficiency x 30 (3.5 if
unpowered). 40 factories at 40% efficiency make 40 x 4.5 x 0.4 x 30 =
**2,160 IC/month**, i.e. **2.6 T2 divisions a month**, so 100 T2 divisions
take about **39 months with 40 factories** or **16 months with 100 factories**
at that efficiency, before replacements, newer gear and non-army lines. To
field 20 T2 divisions every 6 months you need about 2,800 IC/month, which is
about 52 factories at 40% efficiency. Recompute from the live efficiency.

Ratio trick: for a pure-infantry army the factory split is the IC share of the
template: **T2 = 52% infantry equipment, 29% artillery, 19% support**; T3 =
39 / 23 / AT 15 / AA 11 / support 12 (infantry/art/AT/AA/support); an armoured
army T11 is tanks 61%, trucks 15%, support 10%, infantry 9%, artillery/AT 5%.

## 7. Dated factory targets (share of military factories)

Absolute numbers depend on the actual economy: **read `hoi4_economy` first**.
`history/states` sums give roughly Germany 35 civ / 28 mil, Poland 18/11,
China 16/11 at 1936 **[V?]**, so scale the shares below. Starting lines
(`history/units/*_1936*.txt`): Germany 10 factories on infantry eq, 2 support,
1 artillery, 2 light tanks (Panzer II), 2 trucks; Poland 2 infantry eq, 1
support; China 2 on infantry_equipment_0 (efficiency 90, progress 0.5).

| Country/date | Inf eq | Support | Art | Trucks | AT | AA | Tanks | Air |
|---|---|---|---|---|---|---|---|---|
| Germany 1936-37 | 38% | 8% | 9% | 7% | 0 | 0 | 16% light | fighter 10%, CAS 6%, spare 6% |
| Germany 1938-39 | 32% | 8% | 10% | 8% | 2% | 2% | 16% light/medium | fighter 10%, CAS 8%, tac 4% |
| Germany 1940-41 | 26% | 8% | 10% | 10% | 5% | 3% | 20% medium (+TD) | fighter 10%, CAS 6%, bomber 2% |
| Germany 1942-44 | 22% | 8% | 10% | 8% | 6% | 4% | 24% medium/heavy | fighter 12%, CAS 6%, bomber 0-2% |
| Poland 1936-39 | 55% | 15% | 12% | 0 | 5% | 3% | 0 | fighter 10% (as unit counts allow) |
| Nationalist China 1936-38 | 75% | 10% | 8% | 0 | 0 | 0 | 0 | 7% fighter after 1938 |
| Nationalist China 1939+ | 62% | 10% | 10% | 3% | 5% | 0 | 0 | fighter 10% |
| Generic major | 35% | 8% | 10% | 8% | 3% | 2% | 18% | 16% |

Reasons: Germany needs motorized and light tanks only after trucks exist;
fighters protect production; Poland has about 30 divisions at the 1939
mobilisation and can never close the gap in tanks, so equipment depth beats
variety; China, on infantry_equipment_0 until infantry_weapons1, should keep
production on the new infantry line (research 1936 gear first) and run
support/artillery at about 18% total, because 60%+ of Chinese divisions
start under-equipped (`start_equipment_factor 0.5-1.0`).

Civilian factories: the minimum consumer goods share is the floor
`MINIMUM_NUMBER_OF_FACTORIES_TAKEN_BY_CONSUMER_GOODS_*` (1 or 10%); do not
over-build civilians after the first war plan is fixed (see
`economy-build-orders.md`).

## 8. Training and reinforcement

- `hoi4_train(template, count<=24, deploy_state_id)`: queues divisions at
  the template's training time, one click per division, and sets a location on
  every line. **Lines with no location never deploy** *(live: 20 divisions
  waited months)*.
- Size the queue to the stockpile: queue only as many divisions as the
  **current stockpile of the binding item** supports. The binding item is the
  smallest of infantry eq, support eq, artillery, trucks, tanks (divide each
  stock by the template need). Every queued division draws its equipment on
  completion, and manpower is held during training.
  `MIN_POSSIBLE_TRAINING_MANPOWER = 100,000` and `MIN_FIELD_TO_TRAINING_MANPOWER_RATIO
  = 0.75` set the training cap: up to 75% of field manpower can be training,
  and at least 100,000 manpower.
- Training gives experience: `UNIT_EXPERIENCE_PER_TRAINING_DAY = 0.0015`, max
  `TRAINING_MAX_LEVEL = 2`, deploy training level capped at 1; unit XP levels at
  0.1 / 0.3 / 0.75 / 0.9. Divisions take about `training_time` days to finish
  (see 3) and a division in training suffers `TRAINING_ATTRITION`.
- Deploy to a **front-adjacent own state**, not the capital, to avoid strategic
  redeployment. Group arriving divisions with `hoi4_army_create` and keep
  armies at 24 divisions.
- Reinforcement: equipment delivery speed modifier 0.3, manpower chunk 10% per
  delivery (`REINFORCEMENT_*`); templates have 3 priority levels (reserve,
  normal, elite). Replacement draws equipment from the stockpile, so a
  war without a stockpile starves reinforcement before it starves new units.
  Reserve about **15-20% of line IC** for replacement in active war **[V?]**.
- Upgrading old divisions: change the template (adds a regimental AT, swaps a
  support company), then upgrade equipment with army XP (not the same as
  retraining); both reduce strength until stock arrives. Keep at least one
  unchanged old template for reserves rather than converting everything at
  once. The game's AI delays upgrades when equipment would take over 60 days
  (`UPGRADES_DEFICIT_LIMIT_DAYS`), a good rule.
- FAIR: training and template changes are ordinary player actions; no exploit
  templates (single-battalion wall divisions, 0-IC spam, etc.).

## 9. Tools, automation and gaps

### Tool facts (verified in `native/src/launch/macos.rs` and `hoi4_live/tools`)

- `hoi4_division_templates`: **read-only**. Lists own template names and queued
  training lines. It does not return the composition, width, or IC.
- `hoi4_train`, `hoi4_deploy_locations`, `hoi4_army*`: act on existing
  templates only.
- `hoi4_production_add(equipment, factories)` opens a line for a **base
  equipment type** using "the design in the selection window"; which variant
  it picks is not controllable. `hoi4_production_set(line, equipment, factories)`
  changes factories (0 keeps the line). `hoi4_production_lines` is a read of
  lines with signed active/damaged/requested counters; "custom variant
  identity, general production, allocation/idle capacity, eligibility"
  are reported **unavailable**. Naval lines are not supported.
- **There is no native tool for division template design/editing and none for
  equipment variant design.** The native protocol lists no create/edit/duplicate
  template, no support-company placement, no regimental support, no variant
  designer, no line variant switch. The legacy Python `hoi4_army` action
  `design` (duplicate/add/remove battalions through the designer UI) exists in
  `hoi4_live/tools/military_tools.py` but is unproven on native macOS 1.19 and
  predates the regimental support row. Until a native tool exists, templates are
  limited to the game's start templates and whatever the player changes by hand.

### (a) Automatable standing rules

1. Keep every line non-empty; fill idle military factories in the order
   infantry eq, support, artillery, trucks, then others; never leave factories
   idle for more than 3 days.
2. Hold line mix within +-5 points of the dated target table (section 7);
   adjust upward on the binding item of the template in production.
3. Rebalance when stock of any item exceeds 60 days of use or falls below 15
   days (needs a stockpile read).
4. Never switch a line with efficiency over 25% for a marginal variant unless
   the upgrade is a generation jump (I->II->III) and the line is below 30%.
5. Keep the training queue at `floor(binding_stock / template_need)` minus
   reserve, always with `deploy_state_id` set; run `hoi4_deploy_locations` as
   a sweep.
6. Create armies of at most 24 for newly deployed divisions.
7. Run a weekly check of damaged factories (bombing), energy and resource
   shortage warnings.
8. FAIR gate: marines, paratroopers and naval invasion templates only through
   the policy file; no hollow or single-battalion templates.

### (b) Needs live decisions

- Which template to design next and when to retire a template (tech, enemy
  composition: armour, air, fortified).
- Switching a line to a new variant versus waiting (efficiency loss).
- When to move from peace to war production split (target mix change at
  focus or war date).
- Licences, lend-lease and trade for missing resources (steel, tungsten,
  chromium, rubber).
- Where to deploy and how to form armies; terrain-width choices for a specific
  offensive.
- Spending army XP: new template vs equipment upgrade vs tank module change.

### (c) Tools needed (minimum surface to automate section 4-9)

1. `hoi4_division_template_get(name)`: composition grid, divisional and regimental
   support, width, manpower, equipment needs, IC. Read-only, no lease.
2. `hoi4_division_template_save(name, regiments[col,row,type], support[...],
   regimental[...], priority, duplicate_from)`: create or edit through the
   designer UI with read-back verification; refuses a template that fails the
   FAIR policy (exploit patterns) or over the grid limits.
3. `hoi4_template_assign(division_ids, template)` / `hoi4_template_upgrade`:
   change a division's template and report its XP and equipment cost.
4. `hoi4_equipment_variants(type)` and `hoi4_equipment_variant_create(chassis,
   modules, name)`: list and create variants; report IC/XP cost.
5. `hoi4_production_set_variant(line, variant)` and `hoi4_production_priority`:
   choose the variant of a line and its priority.
6. `hoi4_stockpile(type)`: stock, daily production, daily use per equipment
   type; `hoi4_production_lines` extended with efficiency, per-line IC/day,
   resource-short flags, energy.
7. `hoi4_training_lines` (read) and `hoi4_training_cancel(line)`.
8. Naval: `hoi4_production_add` for convoys and escorts (dockyards).

Items 1 and 6 unblock most automation even without 2-5.
