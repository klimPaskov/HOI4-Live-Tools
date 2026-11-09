# Templates, equipment and production

IC is the game's build cost unit. Equipment and division IC below count equipment only (manpower is
separate) and are estimates within a few percent; `hoi4_template_get` and the in-game tooltip give the live
figures. Economy and supply are in `economy-build-orders.md` and `supply-logistics-deployment.md`; this file
covers templates, equipment, production lines and training.

## 1. Combat width

- Terrain base width: plains, hills, desert **70**; forest, jungle **60**; mountain, marsh **50**; urban
  **80**. Each extra attack direction adds half a base width (+35 on plains).
- Stacking: more than 5 divisions on one side of a combat cost -2% per extra division; each extra attack
  direction allows 3 more. Width over the combat width costs -1% per 1% over, capped at -33%. A unit may
  engage up to twice its own width of enemy front. The front line fills first-come; divisions that do not
  fit wait in reserve and rotate in when front divisions lose organisation.
- Battalion widths: infantry, motorised, mechanised, cavalry, mountaineers, light/medium/heavy armour **2**;
  artillery brigade **3**; AT and AA brigade **1**; tank destroyer / SP artillery **2**; support companies
  **0**; regimental companies **0** (they add no width and no speed). Division width is the sum of its
  battalions.
- Designer: 5 x 5 battalion grid, **5 divisional support slots** and a **regimental row of 5 slots**, one
  per column; a column of 3 or more battalions carries one. Regimental items: `fire_support` (heavy weapons
  company) and `mot_fire_support`, `field_guns` (6 artillery eq), `rocket_battery`, `anti_air_battery`
  (16 AA eq), `anti_tank_battery` (16 AT eq), plus SP AA and tank-destroyer supports on armour columns. The
  game's AI templates use 9 infantry + 1-2 artillery brigades, engineer/recon/artillery/logistics and
  regimental AT, AA and fire support.

### Width by terrain

Fill is the share of the combat width used after the over-width and stacking penalties:

| Width | Average, 1 direction | Average, 2 directions | Plains, hills | Forest | Mountain, 1 direction | Mountain, 2 directions | Urban |
|---|---|---|---|---|---|---|---|
| 14 | 0.99 | 0.99 | 1.00 | 0.97 | 0.99 | 0.99 | 0.98 |
| 15 | 0.98 | 1.00 | 0.99 | 1.00 | 0.96 | 1.00 | 0.96 |
| 16 | 0.98 | 1.00 | 0.98 | 1.00 | 0.96 | 1.00 | 1.00 |
| 18 | 0.99 | 0.99 | 1.00 | 0.96 | 0.99 | 0.96 | 0.98 |
| 20 | 0.98 | 0.99 | 0.98 | 1.00 | 0.96 | 1.00 | 1.00 |
| 25 | 0.97 | 0.98 | 0.99 | 0.94 | 1.00 | 1.00 | 0.94 |
| 30 | 0.96 | 0.98 | 0.92 | 1.00 | 0.96 | 0.96 | 0.98 |
| 35 | 0.91 | 0.97 | 1.00 | 0.97 | 0.70 | 0.93 | 0.90 |
| 40 | 0.86 | 0.96 | 0.98 | 0.67 | 0.80 | 1.00 | 1.00 |

Line divisions: **14-18 wide in mountains and hills, 20 elsewhere**. Avoid 34-36 and 40-46 outside plains.

**Width for one axis** (terrain-mix method):
1. Count the axis provinces by terrain: `facts.py terrain-width <TAG | state ids | --route A B>`, or
   `facts.py route` plus `facts.py province <id>`.
2. For each candidate width `w` in the role's range (line 14-20, assault 20-35) and each terrain with share
   `s` and base width `W`: `n = min(floor(W / w), 5)` divisions fit without the stacking penalty,
   `fill = n * w / W`.
3. Score `sum(s * fill)`; take the best, the smaller width on a tie (cheaper replacement, more reserves to
   rotate). Re-check the main terrain with one extra attack direction (`W * 1.5`).

Example: an axis of 60% mountain (50) and 40% hills (70), range 14-21: w 16 scores 0.94 (3 × 16 = 48 of 50,
4 × 16 = 64 of 70), w 14 0.90, w 21 0.86, w 20 0.82, so a 16-wide line division.

Roles: holders 14-20 (several small divisions rotate as an organisation wall); assault 20-35 (a wide
division brings more guns into one combat and routs defenders faster); 40+ only with high industry (slow to
retrain, loses whole divisions on a bad fight, wastes width on narrow fronts). Armour and motorised groups
about 20-24 wide keep organisation and supply manageable; exploitation and pocket closing with 14-wide
motorised. Attackers want the width filled plus one division in reserve to rotate in.

## 2. Equipment statistics (per item)

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

Tanks with No Step Back are modular (without it the designer has upgrade rows): final IC is chassis + module
costs, multiplied by the armour type (riveted -20%, cast +20%). A 1936 light tank (HMG, one-man turret,
bogie, riveted, gasoline) is about **4.2 IC**; a 1939-41 medium with small/medium cannon, torsion bar,
riveted or cast armour **7-13 IC**; a 1943 medium with a long gun **12-13 IC**; a 1941-43 heavy **27-29 IC**.
Adding a module costs 5 army XP, replacing 6, converting a related module 3, removing 1. Since 1.19.4 the
chassis tier multiplies module attack (interwar -5%, basic 0, improved +5%, advanced +10%, modern +15%), so
newer chassis pay off; tank HP scales with reliability, so keep reliability at 80%+. Plane module costs:
read them with `hoi4_design_options`.

## 3. Battalions

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

Division organisation is the average of its line battalions (infantry about 60, mountaineers 70,
armour-heavy templates about 30-45); armour and artillery add punch but little staying power. Training time
is the longest component: 90 days for pure infantry or motorised, **120 days** with any support company,
artillery or mechanised, **180 days** with any armour. A division also needs its full equipment before it
can be fielded.

### Support companies

Cost = own equipment at 1939 prices. All have width 0, one of each type, 5 per division. Against an infantry
battalion at 58 IC, **one support company costs 1-3 battalions**.

| Company | Tech | Equipment | ~IC | Effect | Take it when |
|---|---|---|---|---|---|
| Engineer | tech_engineers (start) | 10 inf, 30 support | 125 | entrenchment +2, +20% entrenchment on light infantry, defence +25% in forest, jungle, marsh and river, attack +10% vs forts, +25% river defence | Always on attack and defence divisions; first slot |
| Recon | tech_recon (start) | 40 inf, 10 supp (mot. variant +20 trucks) | 110 | recon skill 1 (better tactic pick), +10% artillery soft attack, speed | Second slot for line divisions that fight enemies with good tactics; mot_recon for armour |
| Artillery (company) | start | 12 art | 48 | adds a little fire without width | Cheap extra firepower; first choice in the early 9-infantry template |
| Anti-tank / anti-air company | 1939 | 24 AT / 24 AA | 120 / 120 | hard attack, air attack | AT against enemy armour; AA only to reach about 55 division air attack under enemy CAS, else none; regimental batteries (16 eq) are cheaper |
| Logistics | tech_logistics_company | 20 supp, 10 trucks | 105 | **-10% division supply use**, -5% fuel | Mobile or far-from-rail divisions; skip in static defence |
| Maintenance | tech_maintenance_company | 25 supp | 100 | reliability +5%, 5% captured equipment | Armour/mechanised only; armour variant for tank-heavy |
| Field hospital | tech_field_hospital | 30 supp, 20 trucks | 170 | casualty trickleback 20%, XP loss -15%, infantry strength +10% | Long wars and elite/veteran divisions; poor value for cheap divisions |
| Signal | tech_signal_company | 20 supp, 10 trucks | 105 | initiative +0.1 | Attack divisions in the spearhead |
| Military police | tech_military_police | 10 supp, 40 inf | 60 | suppression +0.2, +20% morale for infantry battalions | Skip: on garrisons it raises losses (garrisons go by hardness, T5) |
| Flame tank | special project | chassis | high | terrain attack bonuses; poor stats | Skip |

Support companies share one pool (`support_equipment`, 4 IC each): engineer + recon + logistics + hospital +
signal on a 100-division army needs about **13,000** support equipment, nearly 52,000 IC. Treat the fifth
slot as optional.

## 4. Named templates

Notation: `Wnn` width, `MP` manpower, `IC` equipment IC at the stated gear level, `Org` division average,
`Sup` summed battalion supply use. Equipment totals are per division. Regimental support goes in the row
under the regiment columns (one slot per column of 3 or more battalions).

**T1 INF-18 (1936 start, Germany/China/Poland)**: 9 infantry (3 columns x 3), support engineer + artillery.
W18, MP 9,600, Org 60, Sup 0.72, **inf 910, support 30, art 12**, IC ~620. The game's starting German
division. Poland's variant uses recon instead of engineer/art (IC ~510).

**T2 INF-20 (1939, line division outside mountains and hills)**: 7 infantry + 2 artillery brigades, support
engineer + recon + artillery. W20, MP 9,100, Org 60, **inf 750, art 60, support 40**, IC ~835. Shares:
infantry 52%, artillery 29%, support 19%.

**T3 INF-20 late (1942-44)**: T2 plus regimental anti-tank battery x2 + field guns x1 and an AA company.
W20, MP 9,940, **inf 750, art 66, AT 32, AA 24, support 40**, IC ~1,310 with gen-3 gear. Add regimental AT
only once the enemy fields armour; drop the AA company if you hold air superiority.

**T4 INF-21 (1941+ breakthrough infantry, not for mountains)**: 6 infantry + 3 artillery brigades (W21),
support engineer, recon, field hospital, logistics, maintenance; regimental AT + field guns. MP 10,160,
inf 650, art 78, AT 16, support 115, trucks 30, IC ~1,430. Better attack per width; uses 30% more artillery.

**T5 GARRISON (any year)**: chosen by hardness: resistance losses fall 1% per 1% template hardness, up to
-90%. **1 cavalry battalion** (W2, MP 1,000, 120 inf eq, ~70 IC, suppression 2) where manpower is
plentiful; else **2-4 obsolete light tanks or armoured cars** (hardness about 80%, from old stock). No
military police (it raises losses), no engineer/recon/artillery. Reserve priority.

**T6 MTN-18 (1936)**: 9 mountaineers, engineer + recon. W18, Org 70, inf 1,310 (140 each), IC ~815. For
mountain and hill fronts only (Alps, Pyrenees, Carpathians, Caucasus). W20 variant: 7 mountaineers + 2
artillery, IC ~1,000. Costs about 40% more equipment than plain infantry; stay within the special-forces cap
of max(24, 5% of other battalions).

**T7 MOT-19 (1936 motorised)**: 8 motorised + 1 motorised artillery brigade, mot. recon + engineer +
artillery. W19, MP 11,200, Org 60, inf 850, **trucks ~326**, art 36, IC ~1,600. Trucks (35 per battalion)
are the bottleneck: give the truck line 8-10% of military factories early.

**T8 MECH-19 (1942+)**: 8 mechanised + 1 motorised artillery; recon, engineer, maintenance, logistics. W19,
MP 11,900, mech 320, IC ~4,350, supply 1.4. Expensive and steel/rubber-hungry: a few for spearheads, not the
army.

**T9 PANZER-12 (1936 German start)**: 4 light armour + 2 motorised, support mot. recon + engineer +
artillery. W12, MP 5,500, Org ~27, light tanks 240, trucks ~80, inf 250, IC ~1,500. Fine for 1936-38 only;
the German AI moves to medium armour in 1939.

**T10 PANZER-16 (1939 medium)**: 3 medium + 1 light + 4 motorised, support mot. recon, engineer, artillery,
signal. W16, MP 8,400, Org ~35, medium 150, light 60, inf 450, trucks ~160, IC ~2,500.

**T11 PANZER-21 (1941-43)**: 4 medium armour + 5 motorised + 1 motorised artillery; support mot. recon,
engineer, signal, logistics, maintenance; regimental AT. W21, MP ~11,000, Org ~38, **medium tanks 200**,
inf 550, trucks ~241, support 105, IC ~4,100 at ~12.5 IC per tank. Against enemy armour put regimental AT or
TD support under the armour columns.

**T12 HEAVY-20 (1943+)**: 3 heavy + 6 mechanised + 1 SP artillery brigade; recon, engineer, maintenance,
logistics, signal. Heavy tanks 120, mech 240, IC ~7,000, Org ~43. Costs about 5x a line division; only a
few for breakthrough (tanks 40/battalion at ~27 IC each).

**T14 ART-BREAKTHROUGH-20 (1942+)**: 4 infantry + 4 artillery brigades: W20 (8+12), support engineer, recon,
artillery, signal, field hospital; regimental AT. Art 108, inf 450, IC ~1,330. Soft attack mass for
set-piece battles. Poor org after a lost battle.

**T15 CAV-8 / marines**: 4 cavalry + recon + artillery is W8 and IC ~340: a scouting screen only (garrisons
use T5). Marines are allowed only as ordinary historical amphibious infantry, **one landing at a time, never
spam or space-marine stacks**.

**Wide multiplayer templates (T16-T20)**: strong multiplayer players run 36-38-wide assault and holding divisions
with all five support slots and a full regimental row (AR23). Widths below are the game's own (2 per battalion);
mods with narrower special forces fit more battalions in the same width. Mountaineer, marine and ranger templates
count against the special-forces cap (AR11, AR19): plan the division count from the cap.

**T16 ARMOUR-38 (1939+ breakthrough armour)**: 12 tank + 8 mechanised (5 columns), regimental row 3 tank or TD
companies under the tank columns and 2 SPAA under the mechanised; support engineer, recon (light tank), flame or
armoured support, artillery (super-heavy howitzer later), logistics. W38, 4 km/h, MP ~18,600; with medium-II gear
soft ~720, hard ~690, breakthrough ~1,260, armour ~70, piercing ~100. Variants: a Barbarossa build of 11 light and
medium tanks + 8 mech (soft ~845, piercing ~170); a coastal-defence build of 11 heavy tanks + 8 mech (hard ~960,
armour ~65, piercing ~185). Separate from infantry (no space marines).

**T17 MOUNTAIN-36 (holder and line filler, hills and mountains)**: 18 mountaineers, support artillery, recon,
engineer, AA, flame tank. W36, org ~88, defence ~1,040, soft ~610 with late gear. Variant ANTI-TANK: 15 mountaineers +
3-5 line AT for key ports and narrow fronts facing armour. In a one-direction mountain fight 25 wide fits best (AR11).

**T18 MARINE-36 (landing assault)**: 10-12 marines + 4-5 mechanised + 1-2 line AT; support artillery, super-heavy
howitzer, light-tank recon, flame tank, engineer. W36, MP ~21,700, soft ~510, hard ~130-170, breakthrough ~455.
Pacific variant: 5 marines + 12-15 mountaineers + 1 line AA (no terrain penalty on landings, rivers and mountains).
Only as many marines as the landing needs (AR12); the follow-up waves use T19.

**T19 RANGER-36 (inland after a landing, forests and hills)**: 16 rangers + 3 line AT (W35, MP ~10,000, soft ~470,
hard ~170, breakthrough ~480) or 15 mountaineers + 4 mechanised + 1 line AT; same support as T18. Hard attack rises
with each AT battalion; cut AT when AT equipment runs short, use towed instead of motorised AT when trucks run short.

**T20 PORT-36 (port and island garrison)**: 18 infantry, support artillery, engineer or AT, AA. W36, about 1,000-1,500
IC: a cheap holder for ports, islands and colonial coasts; reserve priority so it draws the oldest rifles (PR17); add
support AT where the enemy lands armour. Remove a company rather than disable its equipment (MX15).

### Designs used with these templates (1.19.4 designer)

| Role | Design | Notes |
|---|---|---|
| Medium tank | Improved medium chassis, medium gun II, radio III, diesel, armour 17 clicks, engine 0 | Soft ~50, piercing ~104, armour ~89, ~15.7 IC; 4 km/h, matches the slowest tank in the division |
| Heavy tank | Basic or improved heavy, high-velocity gun III, radio III, petrol, armour 17, engine 1-5 | Piercing 205-240, armour 110-116, ~23 IC; coastal defence and breakthrough |
| Tank destroyer | Basic medium TD, gun III, no engine or armour clicks | Piercing ~275 for ~13.8 IC: the cheapest piercing |
| Light recon | Improved light chassis, engine 1, armour 17 | Armour ~84, ~16.5 IC; recon support company |
| Flame tank | Basic medium flame chassis, flamethrower, radio, armour 17, engine to 4 km/h | ~19 IC; support company |
| SPAA | Basic light SPAA, AA gun II, no clicks | Air attack ~43 for ~4 IC; regimental row |
| Amphibious tank | Three-man turret, high-velocity gun II, radio III, amphibious drive, auto-loader, easy maintenance, diesel, torsion bars, armour 14-17, engine to 4 km/h | Landing support where amphibious tanks are allowed |
| CAS | Improved CAS airframe, bombs in every weapon slot, best engine, cheapest extras | Ground attack ~36, ~24 IC; the cheapest CAS in quantity for landings |
| Fighter | Improved small airframe, three heavy-MG packs, best engine, self-sealing tank, armour plates | Air attack ~54, agility ~45, ~27 IC; start as a base design and add modules (PR40) |
| Naval bomber | Improved naval bomber, torpedo III, best engine, fuel tanks, armour | Range ~1,365 km, ~29 IC |
| Destroyer | Basic or improved hull, torpedoes in every slot, cheapest light battery, best engine | Torpedo attack ~86, ~1,200 IC (NA07) |
| Mechanised (cost variant) | Every click on production cost, nothing else | The first variant of an own mechanised design, before the MIO (PR38) |

### Evolution path (by era; changes are template edits, see §8 and AR13)

| Date | Line infantry | Armour | Support slots |
|---|---|---|---|
| 1936 | T1 9 inf + eng + art (W18) | T9 | engineer, artillery |
| 1937-38 | T2 7/2 + eng, rec, art (needs no extra tech) | T9 | + recon |
| 1939 | T2, equipment II | T10 | + signal/logistics where tech exists |
| 1940-41 | T3 with AT battery (antitank1 1939, antitank2 1940) | T11 | + maintenance (armour), logistics |
| 1942 | T3/T4 equipment III (infantry_equipment_3, artillery_3) | T11 on medium II | + hospital |
| 1943-44 | T4 plus AA/AT | T11 or T12 | full 5 slots on elite divisions |

Tech years: infantry_weapons1 1936, weapons2 1938, artillery2 1940, artillery3 1941, artillery4 1942,
antitank1/2/3 1939/40/41, antiair1/2/3 1939/40/41, motorised 1936, mechanised 1940, basic medium tank 1939,
improved medium 1941, advanced medium 1943, basic medium TD 1939, rocket artillery 1940. Research ahead of
time only when a template is waiting for the tech.

## 5. Production lines and efficiency

- Output: military factory **3.5 IC a day unpowered, 4.5 powered**; civilian 4/5 construction; dockyard
  **2.0 IC a day** (more when powered) with no efficiency. Energy: see `economy-build-orders.md`; check it
  before adding factories, an unpowered factory loses the +1 output.
- Efficiency starts at **10%**, base cap **50%**. The six industry techs (basic/improved/advanced machine
  tools 1936/37/39, assembly line 1941, flexible line and streamlined line 1943) each add +10 points of cap
  (50% to 110%); flexible line +0.2 retention, streamlined line +0.1 growth. Each dispersed-industry level
  +0.1 retention and +5% start efficiency.
- Growth: daily gain = 0.001 x cap^2 / current (fractions). A 50% cap line takes about **105 days to 25%**,
  **~300 days to 40%** and nearly 500 days to the cap: lines are year-long investments.
- Efficiency is per factory: a new factory starts at 10%, old ones keep theirs; an unused slot loses 1 point
  a day.
- Retention on a line change: same-model variant 90%, same family (other model, same chassis) 70%,
  parent/child tier (I to II) 30%, same archetype 20%, unrelated 10%, never below the 10% start.
  Kept = base + (1 - base) x bonus. Read the game's preview before changing.
- Max factories per line: 150 military; per dockyard line 15 convoys, 10 screens or subs, 5 capital ships.
- Resource shortage: -5% output per missing resource unit on a factory, lowest lines first.
- Conversion: obsolete or captured equipment of the same chassis converts at 0.9 x chassis plus modules and
  keeps line efficiency (PR24).
- Licences: cost 1 civilian factory +1 per year after 1936; output -25%, -35% outside the faction, -50%
  unlicensed, -5% per year the design is ahead (up to 4 years); +20% research on the matching tech; MIO
  traits do not apply; 30-day minimum; upgrades cost double XP. Only to fill a gap (artillery, AA, trucks)
  when allied.
- Lend-lease: deliveries every 30 days; lend cost per item e.g. infantry 1, artillery 4, trucks 5, tanks
  10+. Use received stock for garrisons; do not plan on it.

### Rules for opening or changing lines

1. **One line per equipment type**: infantry, support, artillery, AT, AA, trucks, each tank class. A second
   line only for a tier change (rule 2) or when the old line cannot meet the stock target.
2. **Never move factories off a mature line.** Start a new tier on new or idle factories (3-5, about 10% of
   the old line) and keep the old line while its gear is still needed in quantity. When the old gear's stock
   covers about 30 days of field demand, or quality becomes the binding need, switch the old line whole (old
   efficiency x 30% for a tier switch, e.g. 50% to 15%, beats new factories at 10%). Variant switches (90%)
   any time; a young line (under 15%) switches at once; finish an expensive unit in progress first. Older
   gear stays valid in divisions that ask for the archetype.
3. **Adding factories is safe**: it never lowers the existing factories' efficiency; each new one starts at
   10% (+5% per dispersed level); only the displayed average drops.
4. **Keep lines non-empty**: a line with 0 factories and no progress is wasted, and an empty slot loses 1
   point a day.
5. **Queue sanity**: when `hoi4_production_lines` shows damaged or requested factories above active, or
   `hoi4_production_shortages` shows a deficit, fix the shortage (resource, energy, bombed factories) before
   adding lines.

## 6. Equipment per division and per 100 divisions

| Template | Inf eq | Support | Art | AT | AA | Trucks | Mech/tanks | IC |
|---|---|---|---|---|---|---|---|---|
| T1 INF-18 1936 | 910 | 30 | 12 | | | | | ~620 |
| T2 INF-20 1939 | 750 | 40 | 60 | | | | | ~835 |
| T3 INF-20 late | 750 | 40 | 66 | 32 | 24 | | | ~1,310 |
| T5 GARRISON cavalry | 120 | | | | | | | ~70 |
| T5 GARRISON light tanks | | | | | | | 120-240 old light tanks | from stock |
| T6 MTN-18 | 1,310 | 40 | | | | | | ~815 |
| T7 MOT-19 | 850 | 40 | 36 | | | 326 | | ~1,600 |
| T8 MECH-19 | 850 | 85 | 24 | | | 46 | 320 mech | ~4,350 |
| T11 PANZER-21 | 550 | 105 | 24 | 16 | | 241 | 200 medium | ~4,100 |
| T12 HEAVY-20 | 650 | 105 | | | | 20 | 120 heavy + 240 mech + 40 SP | ~7,100 |

**Per 100 divisions multiply by 100.** Example (T2, 1939 gear): 75,000 infantry equipment (43,500 IC),
6,000 artillery (24,000 IC), 4,000 support (16,000 IC); **about 83,500 IC**.

Worked formula: monthly IC = factories x 4.5 x efficiency x 30 (3.5 if unpowered). 40 factories at 40%
efficiency make 40 x 4.5 x 0.4 x 30 = **2,160 IC/month**, i.e. **2.6 T2 divisions a month**, so 100 T2
divisions take about **39 months with 40 factories** or **16 months with 100 factories** at that efficiency,
before replacements, newer gear and non-army lines. Fielding 20 T2 divisions every 6 months needs about
2,800 IC/month, about 52 factories at 40%. Recompute from the live efficiency.

Ratio trick: for a pure-infantry army the factory split is the IC share of the template: **T2 = 52% infantry
equipment, 29% artillery, 19% support**; T3 = 39 / 23 / AT 15 / AA 11 / support 12
(infantry/art/AT/AA/support); an armoured army T11 is tanks 61%, trucks 15%, support 10%, infantry 9%,
artillery/AT 5%.

## 7. Dated factory targets (share of military factories)

Absolute numbers depend on the economy: **read `hoi4_industry` first**. Start counts are roughly Germany 35
civilian / 28 military, Poland 18/11, China 16/11 in 1936; scale the shares below. Start lines: Germany 10
factories on infantry eq, 2 support, 1 artillery, 2 light tanks (Panzer II, `light_tank_chassis_2` with No
Step Back), 2 trucks, and with By Blood Alone 1 each on Bf 109 D (`small_plane_airframe_1`), Ju 87
(`small_plane_cas_airframe_1`) and He 111 (`medium_plane_airframe_1`) (20 of 28 factories; without By Blood
Alone no air lines); Poland 2 infantry eq, 1 support; China 2 on infantry_equipment_0 (efficiency 90,
progress 0.5).

**Day 0**: before the first unpause put the free military factories on EXISTING start lines (day-0 grace,
PR01) and handle the start naval queue (PR07); cull lines the plan does not use (PR08).

| Country/date | Inf eq | Support | Art | Trucks | AT | AA | Tanks | Air |
|---|---|---|---|---|---|---|---|---|
| Germany 1936-37 | 38% | 8% | 9% | 7% | 0 | 0 | 16% light | fighter 10%, CAS 6%, spare 6% |
| Germany 1938-39 | 32% | 8% | 10% | 8% | 2% | 2% | 16% light/medium | fighter 10%, CAS 8%, tac 4% |
| Germany 1940-41 | 26% | 8% | 10% | 10% | 5% | 3% | 20% medium (+TD support) | fighter 10%, CAS 6%, bomber 2% |
| Germany 1942-44 | 22% | 8% | 10% | 8% | 6% | 4% | 24% medium/heavy | fighter 12%, CAS 6%, bomber 0-2% |
| Poland 1936-39 | 55% | 15% | 12% | 0 | 5% | 3% | 0 | fighter 10% (as unit counts allow) |
| Nationalist China 1936-38 | 75% | 10% | 8% | 0 | 0 | 0 | 0 | 7% fighter after 1938 |
| Nationalist China 1939+ | 62% | 10% | 10% | 3% | 5% | 0 | 0 | fighter 10% |
| Generic major | 35% | 8% | 10% | 8% | 3% | 2% | 18% | 16% |

Reasons: Germany needs motorised and light tanks only after trucks exist; fighters protect production
(new air lines on the 1937-39 airframes, PR16); Poland has about 30 divisions at the 1939 mobilisation and
can never close the gap in tanks, so equipment depth beats variety; China, on infantry_equipment_0 until
infantry_weapons1, keeps production on the new infantry line (research 1936 gear first) and runs
support/artillery at about 18% total, because 60%+ of Chinese divisions start under-equipped (50-100% of
their equipment).

Civilian factories: consumer goods take at least 10% of factories and at least 1; do not over-build
civilians after the first war plan is fixed (see `economy-build-orders.md`).

## 8. Training and reinforcement

- `hoi4_train(template, count ≤ 24, deploy_state_id)` queues the divisions with a location; **lines with no
  location never deploy** (`hoi4_deploy_locations` gives every such line a state).
- Size the queue to the stockpile: queue only as many divisions as the **current stock of the binding item**
  supports (the smallest of infantry eq, support eq, artillery, trucks, tanks, each stock divided by the
  template need). Every queued division draws its equipment on completion, and manpower is held during
  training. Up to 75% of field manpower may be in training, and at least 100,000.
- Training adds 0.0015 experience a day; a division deploys at most Trained, and exercises raise it to
  Regular (AR15); level thresholds 0.1 / 0.3 / 0.75 / 0.9. Divisions train for the template's training time
  (§3) and suffer attrition meanwhile.
- Deploy to a **front-adjacent own state**, not the capital, to avoid strategic redeployment. Group arrivals
  with `hoi4_army_create`; armies of 24 divisions.
- Reinforcement: equipment delivery speed 0.3, manpower 10% per delivery, a draw every 7 days; templates
  have 3 priorities (reserve, regular, elite; `hoi4_template_edit priority`). Replacements draw from the
  stockpile, so a war without stock starves reinforcement before new units. Reserve about **15-20% of line
  IC** for replacements in active war.
- Upgrading old divisions: a template edit with `allow_in_use` reorganises every division of that template
  at once and changed battalions lose up to 50% of their experience (AR13). Prefer a new template for new
  divisions, keep the old one for reserves, and upgrade only when stock covers the new need within 60 days.
- FAIR: training and template changes are ordinary player actions; no exploit templates (single-battalion
  wall divisions, 0-IC spam, etc.).

## 9. Tools, automation and gaps

### Tools

| Job | Tools |
|---|---|
| Templates | `hoi4_template_get` (columns, support, width, HP, org, manpower, supply, equipment need, stats estimate; no input), `hoi4_template_options` (unlocked battalions and companies, XP), `hoi4_template_create`, `hoi4_template_edit` (layout, priority, name; `allow_in_use` reorganises fielded divisions), `hoi4_template_rename`, `hoi4_template_delete`, `hoi4_division_templates` (names, queued lines). Saving costs 5 army XP per battalion, 10 per company, 25 per extra column |
| Designs | `hoi4_designs` (own variants and the lines building them), `hoi4_design_options` (upgrade rows or module slots, XP), `hoi4_design_create` (saves as new), `hoi4_design_set_production` (switch a line inside its family; progress loss must be confirmed), `hoi4_design_obsolete` |
| Lines | `hoi4_production_lines`, `hoi4_production_add` (base type, optional `design`; also ship hulls and convoys), `hoi4_naval_production_options`, `hoi4_production_set` (factories; 0 removes the line), `hoi4_production_shortages` (resource deficits per line) |
| Inputs | `hoi4_industry`, `hoi4_resources`, `hoi4_trade`, `hoi4_trade_set`, `hoi4_auto_import`, `hoi4_convoys`, `hoi4_mios`, `hoi4_mio_design_team`, `hoi4_mio_trait`, `hoi4_mio_policy` |
| Forces | `hoi4_train`, `hoi4_deploy_locations`, `hoi4_army_create`, `hoi4_division_disband`, `hoi4_army_motorization`, `hoi4_alerts` (missing_equipment_production, no_divisions_in_training, free_dockyards) |

### (a) Automatable standing rules

1. Keep every line non-empty; give idle military factories to the line with the fewest days of cover
   (PR10); never leave factories idle more than 3 days.
2. Hold the line mix within ±5 points of the dated target table (§7); adjust upward on the binding item of
   the template in production.
3. Move factories only from a line with more than 90 days of cover to one under 7 days (PR09).
4. Tier switches follow §5 rule 2 (PR04); variant switches (90%) any time.
5. Keep the training queue at `floor(binding_stock / template_need)` minus reserve, always with
   `deploy_state_id`; run `hoi4_deploy_locations` as a sweep.
6. Form armies of at most 24 from newly deployed divisions.
7. Weekly: damaged factories (bombing), energy, `hoi4_production_shortages` and `hoi4_alerts`.
8. FAIR gate: marines, paratroopers and invasion templates only as the FAIR policy allows; no hollow or
   single-battalion templates.

### (b) Needs live decisions

- Which template to design next and when to retire one (tech; enemy armour, air, forts).
- Switching a line to a new design versus waiting (§5 rule 2).
- When to move from the peace to the war production split (target mix change at a focus or war date).
- Licences, lend-lease and trade for missing resources (steel, tungsten, chromium, rubber).
- Where to deploy and how to form armies; the width for a specific offensive.
- Spending army XP: new template versus tank design.

### (c) Gaps (no tool yet)

- Equipment stockpile read (stock, daily production and use per item): days of cover are estimated.
- Per-line efficiency and unit-progress read.
- Production line reorder (line priority, PR11).
- Naval line quantity (ships per line).
- Regimental-support argument in the template tools.
- International Market (sell or buy equipment, PR22-PR23).
- Equipment conversion and ship refit (PR24, PR26).
- Dockyard repair share and the repair queue order (PR33).
- Licence production, lend-lease and volunteer dialogs.
- Garrison template assignment for occupied states.

## 10. Tactics: production lines, designs and stockpile

| ID | Tactic | How it works | When | Mode |
|---|---|---|---|---|
| PR01 | Day-0 production grace | Before the first unpause, factories added to a line that already exists come in at that line's efficiency. Put every idle or freed military factory on an existing start line; delete an unwanted start line only to move its factories onto another existing line | Day 0 of every campaign, game still paused | FAIR |
| PR02 | Efficiency is per factory | Each factory keeps its own efficiency; a new factory starts at 10% (+5% per dispersed-industry level). Adding factories never lowers the old ones; only the displayed average drops. An empty slot loses 1 point a day | Every line change | FAIR |
| PR03 | Retention on a line switch | Kept efficiency: same-model variant 90%, same family 70%, parent/child tier (I to II) 30%, same archetype 20%, unrelated 10%. Bonus: kept = base + (1 - base) x bonus; flexible line +0.2, each dispersed level +0.1, so a tier switch with flexible line and 5 dispersed levels keeps 79%. Switching back does not restore it | Before any switch | FAIR |
| PR04 | Tier change on a mature line | Never move factories off a mature line. Start the new model on new or idle factories (3-5, or about 10% of the old line) while the old gear is still needed in quantity; keep the old line. When old-gear stock covers about 30 days of field demand, or quality becomes the binding need, switch the old line whole (50% x 30% = 15% beats new factories at 10%). A young line (under 15%) switches at once | Infantry equipment I to II/III, artillery, AT, AA, tank and plane generations | FAIR |
| PR05 | Variants are nearly free | A variant switch keeps 90%: keep one chassis or airframe per role and change modules. Never change chassis, engine type, armour type or suspension on a line of 20+ factories | Tank, plane and ship designs | FAIR |
| PR06 | Finish the unit in progress | A switch or removal cancels the unit in progress; a ship loses all its progress. Wait while an expensive unit (slower than one a week) is over 10% done or a cheap one over 75% | Every switch or removal | FAIR |
| PR07 | Start naval queue | Finish ships that are nearly done, then remove the line; remove hulls under about 50% the plan does not need. Germany: keep the four Type II subs, two Z1934 destroyers and Deutschland; remove Hipper and both Scharnhorst unless the plan wants capitals. Italy: finish Montecuccoli, the two Cavour rebuilds, the Maestrale destroyers, the Abruzzi cruisers and the Calvi sub; remove the two Littorio in an Ethiopia-only plan. The executor removes a naval line only from an explicit `from` date | Day 0, every naval country | FAIR |
| PR08 | Cull unused lines | Each military factory makes 4.5 IC a day. At the start and at every phase change remove every line no template, air wing, fleet or stockpile target uses (Germany: the light-tank, fighter and He 111 start lines in a land-first plan; Italy: L3 tankettes and interwar CAS; USSR: light tanks, naval and air lines; Japan: tank lines). Switch an air line inside its airframe family (90%) rather than removing it when the plane is still wanted | Day 0 and each phase change | FAIR |
| PR09 | No factory shuffling | A moved factory restarts at 10%. Move factories only from a line with more than 90 days of cover to a line under 7 days, at most one pull per line per 30 days and never more than 20% of a line | Any rebalance | FAIR |
| PR10 | Idle factories by deficit | Give idle or new factories to the line with the fewest days of cover, in chunks of max(2, 20% of that line), not all to one line | A factory finishes, a line is removed | FAIR |
| PR11 | Line order is priority | Lines at the top get factories and resources first; shortages, bombing and lost factories hit the bottom first; a new line joins the bottom. Keep rifles, support, artillery, trucks and fighters on top | Shortages, bombing | FAIR |
| PR12 | Fix resources before adding factories | Each missing resource unit costs -5% on a factory, lowest lines first. Buy or auto-import first; a 1-2 unit deficit on a minor line costs less than a civilian factory, so accept it | Any red line | FAIR |
| PR13 | Efficiency buff order | Daily growth = 0.001 x cap^2 / current; pick cap first, then cost, then output, growth last; +10% cap overtakes +10% output after about 150-200 days | MIO traits, techs, advisors | FAIR |
| PR14 | Machine tools on time | Six industry techs each add +10 points of cap (50% to 110%); the first four also +20% conversion speed; flexible line +0.2 retention, streamlined line +0.1 growth | Research plan | FAIR |
| PR15 | Concentrated or dispersed | Concentrated per level: +15% factory, +10% dockyard, +20% building slots. Dispersed per level: +10% factory, +10% dockyard, +10% retention, +5% start efficiency, less bombing damage, +20% building slots. Dispersed when two or more big line swaps a year are planned, bombing is expected, licences are used or war is within a year; otherwise concentrated | 1936 industry choice | FAIR |
| PR16 | Early idle factories on gear that does not age | Trucks, support equipment, trains and convoys first; no planes on day 1 (1937-39 airframes outclass them); fighters age worst | 1936-37 | FAIR |
| PR17 | Old rifle line for garrisons | Keep 10-20% of an old line running; reserve-priority garrison templates draw the oldest variant, elite templates the newest | Occupiers 1939+ | FAIR |
| PR18 | Seed line for a planned variant | Run a small second line of the same model (e.g. 10 + 3 factories); later turn the small one into the TD or flame variant on the same chassis (70-90% kept) instead of a new line at 10% | Tank programmes | FAIR |
| PR19 | Line caps | 150 military factories per line; per dockyard line capitals 5, screens and subs 10, convoys 15. Dockyards make 2.0 IC a day and have no efficiency; run parallel ship lines for a large class | Navies | FAIR |
| PR20 | Licences | Cost 1 civilian factory, +1 per year after 1936; output -25%, -35% outside the faction, -50% unlicensed, -5% per year the design is ahead (up to 4); +20% research on the matching tech; MIO traits do not apply; upgrades cost double XP | Minors in a faction (CHI, POL) | FAIR |
| PR21 | Lend-lease | Send obsolete gear to allies and use received gear for garrisons; deliveries every 30 days; the receiver pays the convoys and lend-lease outranks its trade, so send small repeated amounts; the sender gains war score and faction influence | Faction wars | FAIR |
| PR22 | Sell obsolete gear on the market | The International Market pays about 2x IC in civilian credit; credit speeds construction up to +25% (+5% with Free Trade, +10% with Export Focus). Old planes, tanks, spare convoys and trains sell to the AI; one sale run gave about 4000 construction IC. No tool yet | Majors at start (ENG, GER, ITA, FRA) | FAIR |
| PR23 | Market purchases | At most 15 civilian factories per contract, delivery every 30 days, convoys needed | Minors short of one item | FAIR |
| PR24 | Convert instead of scrapping | Conversion costs 0.9 x chassis plus modules and keeps line efficiency; only stockpiled items convert (disband wings first); retire the old design; a conversion to the same design with only another MIO never finishes (change something real) | Old light tanks, captured tanks | FAIR |
| PR25 | Placeholder chassis | Produce the final chassis early with cheap real modules to grow the line, switch the line to the full variant later (90%) and convert the stock. Never change engine type, armour type or suspension. Designs with empty slots made only to skip resources are META | Tank programmes | FAIR |
| PR26 | Refit ships cheaply | A refit costs 20% of the hull plus the changed modules; cancel only below 20% progress. Refit radar, fire control, AA, torpedoes and sonar only; never engine, armour or main battery; refit only when it costs under 30% of a new hull | Navies with older ships | FAIR |
| PR27 | Few designs | One design per ship role and per stage; one artillery, one TD, one recon design; every new design is a new line at 10% | All | FAIR |
| PR28 | Stockpile cover drives everything | Track days of cover per item: under 7 critical, under 30 warning. Reinforcement draws every 7 days. Training, line moves and disbanding follow the binding item, not the average efficiency column | Every war | FAIR |
| PR29 | Spare trucks, trains and convoys | Trucks at 1.5x need, trains at 1.2x, convoys 0.25 per point of sea supply; full motorisation of one hub uses 60 trucks | Motorised or overseas wars | FAIR |
| PR30 | Capture stockpiles | A capitulation hands over 50% of the loser's stockpile and fuel; an annexation 100% of stockpile plus 25% of field equipment and fuel | After each capitulation | FAIR |
| PR31 | Resource-saving modules | Drop rubber or aluminium modules on lines whose resource is short (a fighter without self-sealing tanks uses less rubber) | Resource-short air forces | FAIR |
| PR32 | Pre-warm a line | Efficiency keeps growing during a resource shortage, so open a line the war will need before its resource deal arrives | Lines needed at war start | FAIR |
| PR33 | Bombing and repairs | Bombed factories lose efficiency and damage hits the newest lines first; build new military factories outside enemy bomber range; 1.19.4 repair queue: rails and hubs first in an advance, factories first after bombing | Under bombing | FAIR |
| PR34 | Generation jumps only | Infantry equipment II is about +50% soft attack for about 16% more IC; skip marginal upgrades | Equipment planning | FAIR |
| PR35 | 1.19.4 tank rules | Chassis module-attack multiplier: interwar -5%, basic 0, improved +5%, advanced +10%, modern +15%; tank HP scales with reliability, so keep reliability at 80%+. Defaults: riveted armour, best 3-man turret, radio, engine only to the division's speed; skip stabiliser, skirts, smoke and fuel drums. Against the AI a cheap howitzer medium gives most of the soft attack for about 40% of the cost | Tank designs | FAIR |
| PR36 | Swaps are instant and final | A line swap applies at once even while paused and swapping back does not restore efficiency; switch only from a plan entry, never as a probe | All | FAIR |
| PR37 | Absorb a surplus with battalions | A large surplus (e.g. artillery) goes into elite templates, not into moving factories | Over-production | FAIR |
| PR38 | Swap to an own design without the MIO first | Switching a licensed or old line straight to a new design with an MIO attached loses the whole efficiency. Create a cost-only variant (or a chain of interim designs that change one thing each), switch the line to it with no MIO, then attach the MIO in the designer. After a cost-cutting MIO policy, upgrade the existing equipment once and never again | Own gun, mechanised or tank replaces a licensed or older line | FAIR |
| PR39 | New line for the next gun tier | Keep the mature infantry-equipment line running and open a separate line for the next tier; switching the old line starves the stockpile while it regrows. Merge only when the stockpile is safe | Gun 2 to gun 3, artillery tiers | FAIR |
| PR40 | Base airframe, then convert | Put many factories on a base design of the final airframe with the modules researched so far; add each new module as a variant (90%); when the final design is done, convert every base plane in stock to it | Fighters, CAS 1937-41 | FAIR |
| PR41 | Lines of ten for later swaps | Run big plane lines as several lines of about 10 factories so part can switch role (fighter to CAS) without touching the rest | Allied CAS build-up, Axis Barbarossa CAS | FAIR |
| PR42 | Skip secondary turrets on gun tanks | Secondaries multiply the whole tank's attack down (small cannon x0.95 soft and hard, HMG x0.96 soft) | Any design with 100+ soft attack before secondaries | FAIR |
| PR43 | Armour type | Welded: +30% armour, +5% HP, +4 defence and breakthrough, no IC multiplier, 1 chromium; cast: +30% armour, +10% HP, +20% IC; riveted: -20% IC, no armour gain | Line tanks welded when chromium is covered, else riveted; cast only when HP binds | FAIR |
| PR44 | Reliability for long drives | Reliability also sets organisation loss while moving and the weight of weather penalties | Armour that drives far or fights in autumn mud and winter snow: about 75-80% | FAIR |

## 11. Tactics: division design, training and experience

| ID | Tactic | How it works | When | Mode |
|---|---|---|---|---|
| AR01 | Width from the fill table | Average fill over all terrains: widths 14, 15, 16, 18 and 20 fill 98-99%; 25: 97%; 30: 96%; 35: 91% (70% attacking a mountain from one side); 40: 86% (67% in forest). Line divisions 14-18 for mountains and hills, 20 elsewhere; avoid 34-36 and 40+ outside plains | Every template | FAIR |
| AR02 | Regimental row | A column of 3 or more battalions gets one regimental slot (5 columns, 5 slots); slot XP is halved. Heavy weapons company by default (keeps org), AA battery under enemy CAS, AT battery against armour, field guns for single-player offence; infantry guns and rockets were cut 75% in 1.19.3; batteries have 0 org and lower division org | Line templates from 1936 | FAIR |
| AR03 | Division AA all or nothing | The full 75% cut to CAS damage needs about 55 division air attack; token AA does little. Reach 55 with regimental AA batteries or none | Enemy CAS over the front | FAIR |
| AR04 | Piercing against armour | Unpierced target takes half damage (piercing at 50% of armour: 65%, at 75%: 80%); division armour and piercing = 40% best battalion + 60% average, so one AT battalion lifts piercing. AT battery in every line division facing armour | Enemy armour seen | FAIR |
| AR05 | Soft attack wins against the AI | Most fights are against infantry; assault templates maximise soft attack and breakthrough; hard attack only where enemy armour is seen | Single-player | FAIR |
| AR06 | Breakthrough over enemy attacks | Hits land at 10% while defence or breakthrough remains, 40% after; assault templates need breakthrough above the defender's attacks per hour | Forts, mountains, dug-in lines | FAIR |
| AR07 | Organisation floor | Division org is the average of line battalions (infantry 60, mountaineers 70, tanks 10, artillery 0); pushers need 30+, holders 50+ | Armour and artillery-heavy templates | FAIR |
| AR08 | Line artillery is back | Since 1.19.3 line artillery has breakthrough 10/13/16 and more HP: 7 infantry + 2 artillery (20) or 6 + 3 (21, not for mountains). SP artillery can be cheaper per gun but doubles supply and fuel | Infantry offence 1939+ | FAIR |
| AR09 | TD, SPA and SPAA are support | Line TD and SPA battalions carry -75% to -90% breakthrough; since 1.19.3 TDs belong in the regimental row only. No TD screen division | Armour design | FAIR |
| AR10 | Garrison by hardness | Resistance damage falls 1% per 1% hardness (to -90%). Garrison template: 1 cavalry battalion where manpower is plentiful (suppression 2, cheapest), else 2-4 obsolete light tanks or armoured cars (hardness about 80%). Never infantry plus military police: MP raises losses | Every occupation | FAIR |
| AR11 | Mountaineers for the mountain axis | +35% attack and +10% defence in mountains, +20% attack in hills; width 25 or less so two fit a one-direction mountain fight; special forces cap max(24, 5% of other battalions) | Ethiopia, Alps, Balkans, Caucasus | FAIR |
| AR12 | Marines to the cap only | About +50% amphibious attack cancels the landing penalty; more is wasted; fill the rest with normal assault battalions | Landings | FAIR |
| AR13 | Do not edit veterans | Changing a division's battalions loses up to 50% of its experience; unchanged battalions lose none. Make a new template for new divisions; editing a template used by many divisions makes them all understrength at once; upgrade equipment only when stock covers it within 60 days | Mid-war template changes | FAIR |
| AR14 | Reinforcement priority | Strike templates elite at war, garrisons reserve; elite takes equipment first | Every war | FAIR |
| AR15 | Train to Regular before war | Experience levels: Green -25%, Trained 0, Regular +25%, Seasoned +50%, Veteran +75%. Exercises reach Regular only, hold org at 20%, add attrition and burn fuel; exercise a few cheap divisions, stop 14 days before the war | Peace, more than 30 days to war | FAIR |
| AR16 | Air and naval training | One small wing on training for air XP (about 7 XP a day per wing); naval exercises only to about 100 navy XP and to level 3; stop 30 days before war | Designs need XP | FAIR |
| AR17 | Volunteers for XP | Sending needs 30 divisions; volunteers gain XP at 30%, bring 95% of their equipment home, give +3 opinion per division; recall is locked 180 days; air volunteers up to 20% of the air force | Spain, China 1936-39 | FAIR |
| AR18 | Spend XP before the cap | Army, air and navy XP cap at 500 each | Always | FAIR |
| AR19 | Special forces within the cap | Paratroopers need 70% air superiority on origin, route and target and fight at -30% for 48 hours after landing | Supported airborne operations | FAIR |
| AR20 | Armour force against the AI | 4-8 tank divisions are enough; cheap mediums on the newest chassis, reliability 80%+, armour 25-30, one speed class per division; since 1.19.4 the AI fields real armour, so line divisions get AT batteries | Strike armies | FAIR |
| AR21 | Template roles | Assault 20-35 wide (concentrates damage, routs faster); holders 14-20 (many small holders rotate as an org wall); exploitation and pocket closing 14-wide motorised | Template set | FAIR |
| AR22 | Support companies by job | Engineer, artillery, recon in 1936; signal on assault templates; logistics on armour and colonial theatres; field hospital on strike templates from 1940; maintenance on armour | Every template | FAIR |
| AR23 | Wide multiplayer templates | 36-38-wide armour (T16), mountaineer (T17), marine (T18), ranger (T19) and port (T20) divisions with all support slots and a full regimental row; two fill a plains fight with an extra direction. Use them where the front is plains or multi-direction; keep 14-25 for one-direction mountain and marsh fights | Multiplayer-scale wars, landings, 1939+ armour | FAIR |
| AR24 | Regimental row on armour | Tank or TD companies under the tank columns, SPAA under the mechanised columns; a column needs 3 or more battalions for its slot | T16 and other armour | FAIR |
| AR25 | Wide infantry against armour | Once a narrow division's defence runs out it takes hits 4x as often, and two narrow divisions pay for two sets of support companies | Against armour-heavy attackers: 20-30 width line infantry with AT and AA, not 10-width screens | FAIR |
| AR26 | Role battalion penalties | TD battalions -75% breakthrough and -10% soft attack; SP artillery -75% breakthrough and -10% hard attack; SPAA -90% breakthrough | Never count them toward a template's breakthrough | FAIR |
