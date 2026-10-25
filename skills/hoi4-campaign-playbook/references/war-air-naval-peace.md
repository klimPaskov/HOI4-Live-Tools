# War, air, naval and peace reference (HOI4 1.19.3, FAIR)

Scope: how wars are won against the historical AI under FAIR rules, and which
tool can act on each step. Written for fast scanning by an automated player
while the game runs. Read the tables first; read the prose only for the step
you are on.

Legend. `[D]` = read from the installed game files (`GAME` = the Steam
`Hearts of Iron IV` folder; `defines` = `GAME/common/defines/00_defines.lua`).
`[W]` = hoi4.paradoxwikis.com, possibly out of date. `[?]` = uncertain or
inferred; confirm in the in-game tooltip before betting a campaign on it.
`[L]` = live observation recorded in the other playbook files. Companion
files: `generic-playbook.md` (army organisation, templates), `strategy.md`,
`germany-fair.md`, `china-fair.md`, `poland-meta.md`, `rules-and-modes.md`.

FAIR in one line: strong, clever, normal-UI play. Forbidden: space marines,
invasion or paradrop spam, exploit templates, AI-baiting, engine bugs
(`hoi4_live/game/play_policy.json`, `fair_prohibited`). Everything below is
FAIR unless it says META.

---------------------------------------------------------------------------

## 1. Land combat: what decides battles

### 1.1 Numbers that matter (all `[D]` unless marked)

| Mechanic | Value | Source / note |
|---|---|---|
| Hit dice | org damage d4 x 0.053, strength damage d2 x 0.06 | defines `LAND_COMBAT_*` |
| Armor outclassing enemy piercing | org die becomes d6 (about +40% org damage) | defines `LAND_COMBAT_ORG_ARMOR_ON_SOFT_DICE_SIZE`; piercing detail `[W]` |
| Attacks per hour | hardness-weighted attack / 10; defenses = defense / 10 (breakthrough instead of defense when attacking) | `[W]` Land battle |
| Hit chance | 10% while the target has defenses left, 40% when none | `[W]`; defines `BASE_CHANCE_TO_AVOID_HIT` 90, `...AT_NO_DEF` 60 |
| Planning bonus | +2%/day to a 30% cap (15 days); decays 1%/day under a battle plan | defines `PLANNING_GAIN/MAX/DECAY` |
| Planning with no deployed HQ | cap and speed x0.8; HQ 1-4 provinces behind front x0.95-0.8 | defines `PLANNING_*_COMMS_SCALING` |
| Dig-in | +2% defence per level | defines `DIG_IN_FACTOR` |
| Fort | -15% attack and breakthrough per level for the attacker; each extra attack direction cancels one level (never the last) | defines `BASE_FORT_PENALTY`, `[W]` |
| Rivers (attacker only) | small -30% attack (-25% speed); large -60% (-50% speed) | defines `RIVER_CROSSING_*` |
| Terrain attack penalty (attacker) | forest -15%, hills -25%, urban -30%, jungle -30%, marsh -40%, mountain -50%, plains/desert 0 | `GAME/common/terrain/00_terrain.txt` |
| Combat width per province | plains/hills/desert 70, urban 80, forest/jungle 60, mountain/marsh 50 (extra side 35/40/30/25) | same file |
| Infantry battalion width | 2 (artillery 3, support 0) | `[W]`; `common/units/infantry.txt` |
| Over-width penalty | -1% per % over width, hard cap -33% (cannot join beyond that) | defines `COMBAT_OVER_WIDTH_PENALTY*` |
| Stacking penalty | -2% per division above 5 (+3 allowed per extra direction) | defines `COMBAT_STACKING_*` |
| Reserve join chance | base 2%/hour per idle reserve; speed and radio help | defines `REINFORCE_CHANCE`, `[W]` |
| Two simultaneous combats (defender) | -50% | defines `MULTIPLE_COMBATS_PENALTY` |
| Encircled | -30% (all neighbouring land provinces enemy-controlled); no resupply; routed units with no retreat shatter | defines `ENCIRCLED_PENALTY`, `[W]` |
| Out of supply | attacker -25% attack / -65% defence; defender -35% / -15%; max org capped at 35% | defines `COMBAT_SUPPLY_LACK_*`, `SUPPLY_ORG_MAX_CAP` |
| Supply grace | 72 hours of carried supply | defines `SUPPLY_GRACE` |
| Enemy air superiority | -35% defence, -30% speed (scaled by the terrain figures below) | defines `ENEMY_AIR_SUPERIORITY_*` |
| Taking a province | conqueror loses 20% of max org per province taken | defines `ORG_LOSS_FACTOR_ON_CONQUER` |
| Manual move | org clamped to 66% | defines `MAX_ORG_ON_MANUAL_MOVE` |
| Night | -50% attacks, both sides | `[W]` |
| Commander | about +2.5% per skill point; cap 24 divisions per general or field marshal, 5 armies per marshal; marshal stats reach armies at 50% | defines `CORPS_COMMANDER_DIVISIONS_CAP`, `FIELD_MARSHAL_*` |

Terrain also scales how much enemy air superiority helps: the terrain file
lists `enemy_army_bonus_air_superiority_factor` of -0.5 (urban), -0.25 (jungle),
-0.10 (forest, mountain), -0.05 (hills), none on plains/desert. `[?]` Read as:
air superiority hurts you most on plains and least in cities; verify in play.

### 1.2 How to read a battle in one glance

1. Org is the clock. A division at 0 org retreats; with nowhere to retreat it is
   destroyed. Most wars are won by draining org faster than the enemy, not by
   killing manpower.
2. Defense stat and breakthrough are the counter pair: attackers with more
   breakthrough than the defender has defense hit 40% instead of 10%. This is
   why armor/mechanized spearheads crack infantry lines and why defenders want
   high defense plus forts plus dig-in. `[W]`
3. Piercing versus armor: infantry with too little anti-tank gets the d6 org
   hit applied to it. Add anti-tank and engineer supports when the enemy has
   armor. Add AA when the enemy has tactical/CAS wings.
4. Width: more attackers than width wastes strength (penalty) and more than
   about 5 divisions per front stack adds the -2% per division penalty. Many
   thin attacks from several provinces beat one huge stack.
5. Supply: a spearhead beyond rail and supply range loses fight capability
   within about three days of carried supply (72 h), then org cap 35%.

### 1.3 Tactics (combat_tactics.txt) and the counter wheel

`GAME/common/combat_tactics.txt`. A tactic is chosen per combat each 12 hours
(`TACTIC_SWAP_FREQUENCEY`); better commanders (skill advantage) unlock the
stronger options and pick counters.

| Attacker tactic | Needs | Effect (dmg to defender / taken) | Countered by |
|---|---|---|---|
| Basic attack | always | +5% | counterattack |
| Assault | artillery share > 10%, close-combat phase | +25% | counterattack |
| Encirclement | frontage full, reserves, skill advantage or panzer_leader/trickster | width +50%, +25% | tactical withdrawal |
| Shock | attacker | defender -25% | ambush |
| Breakthrough | hardness > 0.5 or skill advantage > 1 | speed +50%, +25%, defender -15% | backhand blow |
| Blitz | hardness > 0.5, skill > 2 or panzer_leader | speed +50%, +15%, defender -15% | elastic defense |

Defender counters: counterattack (+25%, skill advantage), ambush (-25% for
the attacker; skill advantage > 1 or skill > 2), elastic defense (needs
`defensive_doctrine` or skill > 2), backhand blow (skill > 4 or trait
`defensive_doctrine` with skill > 3, defender +25%, attacker -20%), tactical
withdrawal (counters encirclement). Practical rule: put your best attack/skill
generals on the decisive armies and your best defence generals on the line the
enemy will hit; do not worry about the wheel beyond that.

---------------------------------------------------------------------------

## 2. Orders and battle plans

Mapping to the tools in section 14: `hoi4_army_front` draws a front line,
`hoi4_army_offensive` an offensive line, `hoi4_army_plan` executes or stops,
`hoi4_army_move` is a manual order.

| Order | Use | Notes |
|---|---|---|
| Front line | Hold or screen a border; units spread across the line | Needs a foothold: the army must already stand on or in the enemy state `[L]` |
| Offensive line | Attack through the front into a named state | Drawn after the front; execute afterwards `[L]` |
| Spearhead | One fast army ordered at a deep target | Only to single armies `[L]`; no tool yet |
| Area defense | Defend a province set, units concentrate on value (VP, ports, capital weight 50) | No tool yet; scoring in defines `PLAN_*DEFENSE*` |
| Manual move | Mop up, reach a foothold, plug a hole | Plan progress decays 3%/day under manual orders `[L]`; org cap 66% |
| Execute modes | careful / balanced / rush: attack-score thresholds 25 / 0 / -200; careful refuses forts >= 5 | defines `PLAN_EXECUTE_*`; the UI mode switch has no tool `[?]` |

Rules of thumb:

- Plan first, attack second. Draw front and offensive line, wait 10-15 days
  for planning to reach 30%, then execute. Never execute at 0% planning unless
  the enemy is about to reinforce.
- A divisions-per-province density of 2-3 is what the planner distributes
  against (`PLAN_PROVINCE_PRIO_DISTRIBUTION_DPP_*`); stretching one army over a
  very long front silently thins the attack.
- Give every army a general. Armies under about ten divisions get none `[L]`.
- Keep each army at 24 divisions or fewer and each army group at 5 armies or
  fewer.
- Do not edit orders daily. Each new order resets that army's effective
  planning; check once per game day and act only on a trigger (section 12).

---------------------------------------------------------------------------

## 3. How to encircle (FAIR, human-doable)

Encirclement here means trapping enemy divisions inside a pocket by taking the
provinces behind them; no engine tricks.

1. Find the neck. Look at the front for an enemy salient or a province whose
   only land route back passes one or two provinces you can reach. The game
   itself rechecks potential pockets every 72 h per army
   (`HOURS_BETWEEN_ENCIRCLEMENT_DISCOVERY`). Typical Polish case: the western
   bulge between Silesia and East Prussia is pinched at Lodz-Warsaw.
2. Two pincers, one anvil. Two spearhead armies of armor/motorized/mechanized
   divisions (fast, high breakthrough, hardness above 0.5 triggers the
   breakthrough and blitz tactics) attack on either side of the neck. A slower
   infantry army pins the pocket from the front so the defender is attacked
   from several directions (this also cancels fort levels and doubles the
   defender's multiple-combat penalty).
3. Plan: front line for the anvil and offensive lines for the two pincers
   aimed at the same rear province, not at the pocket itself. Execute all
   three on one day after planning is high. Support with air superiority and
   CAS over the pincers.
4. Close and hold. Once pincers meet, switch pincers to a front line facing out
   and let infantry compress the pocket. Pocketed divisions take -30%, get no
   reinforcement and shatter when routed with no retreat.
5. Do not overextend. The pincers must stay in supply. Stop a spearhead one
   province before supply breaks, wait for trucks/rail to catch up, and keep a
   second echelon ready to close behind it. Check org of the spearhead above
   about 50% before the next push; manual moves cap org at 66%.
6. Avoid the two classic traps: (a) attacking across a large river at -60% with
   the pincer (pick bridges or dry approaches), and (b) pushing so far that
   your pocket-closers are themselves encircled (an enemy counterattack needs
   only the same neck mirrored).

Both pincers and the anvil on one front can be ordered with existing tools;
the pocket search itself has no tool (section 14).

---------------------------------------------------------------------------

## 4. Defensive lines and reserves

| Situation | Setup |
|---|---|
| Static defence | Front line on rivers/mountains/forest; 1 fort level is already -15% for the attacker; engineers speed dig-in; keep infantry 24 div max per army |
| Wide front, thin forces | Defend only terrain-favourable provinces; give up open plains with a fighting retreat, keep org and equipment |
| Mobile reserve | A mobile army (motorized/armor) one province behind the hub, not on the line; counterattack pockets/breakthroughs; reserves also join combats only at 2% per hour per division so do not rely on them as an emergency fix |
| Holding VPs | Capital and high-VP cities weigh most in area defense (capital 50, high VP 10); `PLAN_PROVINCE_*_VP_*` |
| Surrender pressure | Countries capitulate at 80% loss of core VPs (section 9); protect VP-dense cities first |
| Do not | Leave a lone division in a pocket-prone salient; defend a coast with the main army while the land front is empty; trigger fort assaults with infantry without artillery |

Defender advantage stack (cheap): river + fort + dig-in + air superiority +
defense general with `defensive_doctrine`/`unyielding_defender` + AT and
engineers. A defender in two simultaneous combats is -50%: avoid lines where
the same division is attacked from two provinces.

---------------------------------------------------------------------------

## 5. Air war

### 5.1 Priorities

1. Air superiority first. Fighters on `air_superiority` over the decisive
   strategic region 1-2 weeks before an offensive. Enemy air superiority costs
   your ground units -35% defence and -30% speed (section 1.1).
2. CAS over the decisive battles. CAS adds attack damage scaled by plane count;
   each combat accepts only about 3 CAS/TAC planes per enemy width
   (`LAND_AIR_COMBAT_MAX_PLANES_PER_ENEMY_WIDTH`), so more CAS than that is
   wasted; CAS at night works at 10% (`CAS_NIGHT_ATTACK_FACTOR`).
3. Tactical bombers: bomb bases/infrastructure/logistics in range; slow to
   re-establish after region change (see 5.3).
4. Strategic bombers and logistics strikes: only when the war is long; they
   gain little in a short blitz.
5. Interception on defence when the enemy has bombers and you lack fighters
   in the air.

### 5.2 Mission cheat sheet (tool `hoi4_air_mission`)

Missions the tool accepts: `air_superiority, cas, interception,
strategic_bombing, naval_strike, kamikaze, port_strike, logistics_strike,
air_supply, training, naval_mines, naval_minesweeping, recon, naval_patrol`
(native/src/launch/macos.rs). The tool gives a mission over the strategic region
that contains a given state ID.

| Wing type | Default mission | Where | Notes |
|---|---|---|---|
| Fighter / heavy fighter | air_superiority | the region over the decisive front | Maintain 24/7; interception when defending |
| CAS | cas | the region with active friendly attacks | Needs superiority or is shot down |
| Tactical bomber | logistics_strike / strategic_bombing | enemy rail/supply behind the front | `[?]` choose logistics_strike for a blitz, strategic for industry |
| Naval bomber | naval_strike / port_strike | sea region with the enemy fleet / a port | Needed for the Channel |
| Carrier air | naval_patrol / naval_strike | with the task force | Only with carriers |
| Recon | recon | the target sea or invasion area | Raises naval intel (needed for supremacy, 5.4) |

### 5.3 Range, bases, efficiency

- A wing flies only over regions within its range from its air base; rebase
  forward as the front moves. No tool does rebasing yet (section 14).
- Air base level and capacity limit the number of wings; overstuffed bases
  reduce effectiveness. Bombers fuel use is per plane (about 1.0 per mission
  in `MISSION_FUEL_COSTS`, x0.35 global).
- Changing a wing's region costs efficiency (x0.9) and then regains per day:
  fighters and CAS regain fast (0.888/day), tactical and naval bombers slowly
  (0.192), strategic bombers very slowly (0.072). So: re-region fighters and
  CAS freely; leave bombers in place and fly them only where the war will stay.
- Lack of fuel cuts mission efficiency to 25% (`MISSION_EFFICIENCY_MULT_AT_LACK_OF_FUEL`).
- Anti-air: AA brigades/supports on divisions reduce plane damage; ground AA
  absorbs part of CAS losses. Add AA supports where enemy tactical air is
  strong.

### 5.4 Air versus ships

Naval strike and port strike need superiority; carrier planes get x12 naval
strike damage when their carrier is present
(`NAVAL_STRIKE_CARRIER_MULTIPLIER`). Land-based naval bombers need base range
to the sea region.

---------------------------------------------------------------------------

## 6. Naval warfare

### 6.1 Concepts

| Term | Meaning | Source |
|---|---|---|
| Naval supremacy / dominance | The share of dominance points in a sea region; a region is controlled at 60% of the needed dominance over enemies (`DOMINANCE_CONTROLLED_THRESHOLD_RATIO`) | defines |
| Mission contribution to supremacy | Strike force and invasion support 100%, patrol 75%, convoy escort 50%, convoy raiding/mines 25%; hold/exercises 0 | `[W]` |
| Naval intel | Needed to gain supremacy; below 40% intel efficiency supremacy is penalised | `[W]` |
| Air superiority over the sea | Adds up to +100% to fleet effective ship count `[W]`; radar and airbase levels add to dominance (0.05 / 0.02 per level) | `[W]`, defines `RADAR_DOMINANCE_FACTOR`, `AIR_BASE_DOMINANCE_FACTOR` |
| Convoy | Each division moved by sea or each supply shipment needs convoys; losing convoys cuts efficiency 1.25x the loss | defines |

### 6.2 Missions and what to use them for

| Mission | Use |
|---|---|
| Patrol | Hold a sea region; fight everything that enters; main supremacy builder for screens/cruisers |
| Strike force | Capital ships/carriers hunting the enemy fleet; multiplier from dominance |
| Convoy raiding | Submarines/cruisers cutting enemy imports; war score +0.05 per sunk convoy |
| Convoy escort | Destroyers protecting your own routes; mandatory on a long supply route |
| Naval invasion support | Escorts and shore bombardment for a landing (only mission that gives full supremacy for it) |
| Mines / minesweeping | Niche; mine enemy ports or sweep your own approaches |
| Exercise (training) | Peace-time XP |

Fleet composition for a historical AI game (this is the AI's own ratio
guidance in `common/ai_strategy/*.txt` and a safe house rule):

- Surface task force: a few capital ships/carriers plus a screen of
  destroyers/cruisers. The game documents a capitals-to-screens ratio of 0.25
  (`BEST_CAPITALS_TO_SCREENS_RATIO`); a carrier or land-based naval bombers
  decide fleet fights more than another battleship in the 1939-41 window `[?]`.
- Submarines: wolfpacks of 20-30 (`MAX_SUBMARINES_PER_AUTO_TASK_FORCE` = 30) for
  convoy raiding; shallow seas are easier to spot for them.
- Convoy defense: destroyers on escort in every sea region where your supply
  or troop convoys travel; keep spare convoy production.

### 6.3 Blockading or defeating Britain

1. Britain is an island supply problem, not a short campaign. A blockade
   alone is unlikely to capitulate it `[?]`; use it to starve production,
   drain the Royal Navy and gain war score before an invasion.
2. Sequence: (a) build destroyers, subs, naval bombers and fighters, (b) win air
   superiority over the North Sea, Channel and Western Approaches from French
   and Dutch bases, (c) put wolfpacks on convoy raiding in the approaches,
   (d) use naval bombers against ports and fleets, (e) only then plan a landing.
3. Realistic invasion trigger (all must hold):
   - France has capitulated and bases on the Channel coast are yours.
   - Air superiority above the landing zone for at least two weeks.
   - The Home Fleet is destroyed or contained and supremacy in the Channel
     region exceeds 50% `[W]`.
   - 40%+ naval intel (recon/radar) `[W]`.
   - Transport capacity for the divisions you will land (heavier divisions need
     more convoys `[W]`).
   - Ports to capture: a landing needs a port or the troops run out of supply
     after about 72 h (`SUPPLY_GRACE`).
4. In the 1.19.3 files the German Sea Lion focus asks for: war with ENG,
   army manpower above 1,000,000, and more than 90,000 infantry equipment in
   armies (`GAME/common/national_focus/germany.txt`, `GER_operation_sea_lion`).
   Its 180-day idea gives +25% naval invasion prep speed, +25% shore
   bombardment, -15% naval invasion penalty, +10% strategic bombing and +5%
   attack against ENG (`GAME/common/ideas/GER.txt`).

---------------------------------------------------------------------------

## 7. Naval invasion (one proper landing)

FAIR means one well-prepared operation with follow-up reinforcement through the
captured port. Not allowed: repeated empty-port landings, landings used to
reset AI behaviour, or space-marine / paratrooper width tricks.

| Parameter | Value | Source |
|---|---|---|
| Divisions per invasion order | base 4; raised by naval techs (landing craft etc.) | defines `BASE_NAVAL_INVASION_DIVISION_CAP`; `[W]` |
| Simultaneous plans | base 1 | defines `NAVAL_INVASION_PLAN_CAP` |
| Prep time | defines say 60 days base; the wiki says about 7 days per division and landing-craft techs shorten it | defines `NAVAL_INVASION_PREPARE_DAYS` vs `[W]`: `[?]` mismatch, read the UI |
| Planning bonus gain | 2%/day in defines; wiki says normally nullified for naval invasions | defines `NAVAL_INVASION_PLANNING_BONUS_GAIN`; `[W]` |
| Landing penalty | -50% attack, reduced by landing-craft invasion defense; more for heavy equipment | defines `AMPHIBIOUS_LANDING_PENALTY`; `[W]` |
| Transports spotted | invasion convoys are spotted 10x more easily | defines `NAVAL_INVASION_SPOTTING_SPEED_MULT` |
| Supply on shore | about 72 h without a port | defines |
| Floating harbour | temporary hub (No Step Back) | `[W]` |

Procedure:

1. Pick a landing province next to a port or city with low fort and low enemy
   density; Britain's southeast coast or an undefended port fits. Avoid fortified
   beaches.
2. Pre-conditions in order: air superiority over the landing region and the
   sea route; fleet supremacy above 50% across every sea region on the route
   `[W]`; destroyers on escort; naval bombers and fighters on port strike /
   superiority; enough convoys for the divisions' weight.
3. Choose the best divisions (not marines unless you already have normal
   marine divisions; special-forces width exploits are forbidden). Use up to the
   order cap, support with engineers and armor only if landing craft allow.
4. Start the order, wait out prep (units travel to the port during prep), keep
   air and fleet missions active until the landing completes. If supremacy is
   lost during the voyage, you may cancel and retry only after the cause is
   fixed.
5. After landing: take the port within the first days, ship in the supply and
   the second wave through it, expand the beachhead with front/offensive lines
   (the same tools as land war), keep fighters over it.
6. Do not repeat the operation as a feint or stack landings to exhaust the AI.
   One large operation plus reinforcement through the captured port.

Needed: no tool exists for the order itself; this is a defect for any plan that
depends on it (section 14).

---------------------------------------------------------------------------

## 8. War declaration, timing and diplomacy

### 8.1 Mechanics

| Item | Value | Source |
|---|---|---|
| War goal needed | Declare war requires a wargoal (focus, justification, event) except in sandbox cases | game design `[?]` |
| Justification time | at least 10 days; shorter with high world tension (-50% cost at 100% tension), cheaper for wargoal vs a major already at war (-75%) | defines `MIN_WARGOAL_JUSTIFY_COST`, `WARGOAL_*` |
| Justify cost | 0.2 PP/day base; scales with number of wars and justifications | defines `BASE_GENERATE_WARGOAL_DAILY_PP`, `WARGOAL_PER_JUSTIFY_AND_WAR_COST_FACTOR` |
| Threat | war with CB 10, without CB 14; guarantee -5; faction join +4; join attacker +2; annexation +0.1 to +0.2 per state | defines `TENSION_*` |
| Truce | 180 days base; breaking by joining costs 200 PP scaled | defines `BASE_TRUCE_PERIOD`, `TRUCE_BREAK_COST_PP` |
| Guarantee cost | 15 PP scaled by the number already guaranteed | defines `GUARANTEE_COST` |
| Breaking a guarantee | war-support penalty -0.2 | defines |
| Focus wargoals | `expire = 0` in the focus means it does not time out (e.g. `GER_war_with_france`) | national_focus |
| Justified goals from the diplomacy screen | Time out about two months after completion `[L]` | |

Declaration rule for the agent: complete justification or focus, verify the
goal exists, then declare on the first possible day. Do not let the game run
at high speed across the completion date. Declare only when armies are on the
border with planning built (section 2) and the build-up checklists below pass.

### 8.2 Faction and guarantee behaviour (1939 start and 1936 start)

- UK and France both hold `give_guarantee = POL` in their 1939.1.1 history
  blocks (`GAME/history/countries/ENG - Britain.txt` line 373 and
  `FRA - France.txt` line 259). Germany's own focuses also reference the
  Polish guarantee; for a 1936 start the guarantee comes later through focus
  and event chains `[?]`.
- The AI honours a guarantee by declaring war on the attacker (typical
  behaviour) `[?]`. Expect UK and France in the war when Germany invades
  Poland.
- The Soviet AI with historical focus enabled has a `declare_war` -300 against
  Poland before 1941 (`GAME/common/ai_strategy/SOV.txt`, `SOV_historical_dont_attack_poland_too_early`).
  Its German hostility (`germany_enemy`) only starts with the anti-fascist
  policy focus and after 1942-01-01 or an existing war. Do not assume a
  timetable beyond that.

---------------------------------------------------------------------------

## 9. Capitulation, peace conference

### 9.1 Capitulation (when a country quits)

- Default surrender limit: 80% of core victory points lost (defines
  `BASE_SURRENDER_LIMIT` 0.8; `[W]`). Countries with no cores use 70% of that
  limit; the minimum non-forced limit is 20%. No capitulation in the first 7
  days of a war (`DAYS_OF_WAR_BEFORE_SURRENDER`).
- Low war support (below 50%) lowers the limit up to 30%; collaboration lowers
  it up to 30% `[W]`. National spirits change it (e.g. Belgium -50% in
  `common/ideas/belgium.txt`; France's Disjointed Government idea exists in
  `common/ideas/france.txt` and is listed as -50% on the wiki `[W]`).
- When the limit is crossed, the country capitulates to the enemy that dealt
  the most war score; the capital holder gets +50% and +150 bonus score
  (`CAPITAL_CAPITULATE_BONUS_*`), so take the capital.
- Allies: capitulation does not end a war while major allies fight on; a
  minor can be forced to surrender when its majors fall `[W]`.
- Capitulation gives the winner 50% of stockpiled equipment (`CAPITULATE_STOCKPILES_RATIO`).

### 9.2 Peace conference in 1.19

Opens when a war is won (capitulation or white peace by surrender). Use tools
`hoi4_peace` (legacy Python) only: the native MCP has no peace tool.

| Rule | Detail | Source |
|---|---|---|
| Points | A victor's score is about 1.35 x enemy state value x participation share; conditional surrender scales by surrender progress | `[W]` |
| Timing | A sole victor gets full score on turn 1; several victors are paid in 20% steps over five turns | `[W]`; loc `PEACE_SCORE_*` |
| Bids | Every victor submits demands each turn; demands on the same state are contested, the spent score is refunded and the price rises (x1.2 per contested bid) | defines `PEACE_COST_FACTOR_CONTESTED_BID`; `[W]` |
| Refund | Full on the first contested turn, falling to a floor of 76% | `[W]` |
| Uncontested price | Rises 1.15 to 1.60 x per turn it stays uncontested (step 0.15) `[?]` read as "bid early on key states" | defines |
| Pass/Done | Pass gives others their turn; Done leaves the conference and your accepted bids persist; contested bids can pull you back | loc `PEACE_DONE_*` |
| Take uncontrolled state | factor 10 (taking a state you do not control) | defines `BASE_PEACE_TAKE_UNCONTROLLED_STATE_FACTOR` |
| Taking an ally-controlled state | 0.5 | defines |
| Influence | 0.7-1.0 by distance to capital, core, controlled land; +0.05 per adjacent uncontested state: blob, do not snake | defines `INFLUENCE_*` |
| Occupation discount | state you control: x0.8 (and compliance steps 1.0/0.9/0.8) | cost_modifiers `generic_occupies_state`; defines |
| Core | state that is your core: x0.25 (x0.5 if taker is a puppet); claim: x0.9 | `cost_modifiers/00_generic_peace.txt` |
| Ally major's core | x3.0 if a major on your side has a core on the state | same file |
| Ideology | fascism take x0.9, puppet x0.95, liberate x1.3; communism puppet x0.9, liberate x1.3; democracy take (non-core) x1.3, puppet x1.2 | same file |
| Defensive war | take x1.1 (liberate/force government x0.95) | same file |
| Stackables | demilitarised zone, war reparations, resource rights, dismantle industry +25% each | defines |
| Ships | 0.005 score per IC of capital ship taken | defines |
| Actions | take_states, puppet, force_government, liberate, take navy, stackables | loc `PEACE_*_DESC` |

Order of operations at a conference:

1. Occupy first. Occupied states are 0.8x; unoccupied are 10x.
2. Turn 1: bid the high-value states you want and that allies also want;
   leave states nobody else wants for later. Use the cost tooltip on each
   action before submitting.
3. Prefer take-state on your cores/claims and on industrial or VP-rich land;
   puppet the rest when you cannot afford it (puppet costs follow similar
   modifiers; a puppet adds a loyal buffer but gives less land).
4. If a bid is contested, either forfeit and take the refund, or re-bid at the
   higher price only if the state is worth it.
5. When score is spent or nothing worthwhile remains, press Done.

How to claim each target (all `[?]` on exact state sets; use `hoi4_states`):

| Target | How |
|---|---|
| Poland | Occupy all western Poland and Warsaw; take German cores/claims first (cost 0.25 for cores); take or puppet the rest; if the USSR is also a victor, bid first on states it cannot reach |
| France | Occupy Paris and the northeast; Alsace-Lorraine and other cores/claims cheap; puppet or Vichy-style remainder; ships via take-navy |
| UK | Occupy the home islands, port cities; colonies/dominions are far from your influence (cost rises with distance); capital ships on bid |
| USSR | Occupy to the Urals line before capitulation; far states are expensive; puppet to cover the east; a Russian successor puppet is cheaper than annexation |
| Japan | Occupy home islands; USA-only modifiers in `JAP_peace.txt` do not apply to non-US victors; puppet if full annexation is too dear |
| Chinese warlords (CHI) | CHI gets x0.75 to take from RNG (Reformed National Government) (`CHI_peace.txt`) |

---------------------------------------------------------------------------

## 10. How the historical AI behaves

Game rules `GAME/common/game_rules/00_game_rules.txt` have per-country
`*_AI_BEHAVIOR` options (GER, SOV, JAP, ITA, FRA, POL, ENG, CHI, PRC and more).
The campaign uses the historical option `[?]` (confirm the active value).

| Country | Typical behaviour | Source |
|---|---|---|
| Poland (AI) | Defends the western front, relies on the UK/FR guarantee | `ai_strategy/POL.txt`, history |
| UK / France | Both guarantee Poland from 1939.1.1; France concentrates at the Maginot Line; the AI avoids early invasions (`ENG_ALLIES_careful_with_early_invasions`) | history, `ai_strategy/ENG.txt` |
| Soviet Union | Historical: stays out of Poland until 1941, builds up, focus-driven ultimatums to the Baltics and Finland; war with Germany follows its own focus chain | `ai_strategy/SOV.txt`, national focus |
| Italy, Japan | Follow their historical focus chains | game rules |
| China AI (KMT) | Subjugate-the-warlords events with puppet/refuse choice; Japan attacks by its own focus chain | `china_nationalist.txt`, `wtt_china.*` events |

General AI traits: it reacts to visible threat, shifts units to the most
threatened front, builds large fixed lines, rarely counterattacks pockets
decisively, and has limited ability to defend against an air-and-armor
breakthrough. It never knows your plan; do not use feints to bait it (FAIR).

---------------------------------------------------------------------------

## 11. Greater German Reich

Exists in 1.19.3: decision `form_greater_german_reich` in
`GAME/common/decisions/formable_nation_decisions.txt` (line ~9802).

- Needs the Waking the Tiger DLC (`allowed`). If it is missing, the decision
  does not appear.
- Visible for original GER under fascism, once either France has capitulated
  while at war with GER, or GER owns Paris (state 16) fully controlled, or
  Moscow (219) is controlled.
- Available only when GER is not a subject and fully controls (itself or via
  subject) Paris 16, Prague 9, Warsaw 10, Leningrad 195, Moscow 219, Stalingrad
  217, and every owned state is fully controlled (no occupied, split-control or
  resistance-held state). State IDs are from the file's comments.
- Cost 25 PP. Reward: +5% war support, +5% stability, the Greater German Reich
  tag and a Germania victory point; cores on some states.

This decision is never a goal in itself: finish the Soviet war and reach full
control first. Leningrad and Stalingrad are far east targets.

---------------------------------------------------------------------------

## 12. War-by-war timelines and checklists

Dates are for a 1936 start with Germany. Use them as targets, not requirements;
triggers matter more than dates.

### 12.1 Germany versus Poland (1939)

Prereqs: Austria and Czechoslovakia handled by focus (see `germany-fair.md`).
`GER_danzig_or_war` requires at least 950,000 army manpower (about 100
divisions), Danzig (state 85) still Polish, no Polish civil war; it grants an
event where Poland cedes Danzig/807 or fights (`germany.txt`, event germany.80).
Reality check: Poland usually refuses and fights.

Checklist:
1. Armies on border: north (East Prussia/Pomerania), south (Silesia, Slovakia
   if puppet), centre. Armor spearheads at both flanks.
2. Planning at 30%; fighters on air superiority over the Polish western regions.
3. Declare (war goal active). UK and France are likely to join.
4. Day 1-10: pincers toward Warsaw (state 10) from north and south; infantry
   pins the Posen bulge; avoid rivers (Vistula, Narew) unless bridges.
5. Day 10-30: close the pocket west of the Vistula, hold; take Warsaw.
6. After capitulation: occupy all Polish states before the conference.
7. Move the army west before the Allies attack the Rhine; do not leave
   a few divisions facing France.

### 12.2 Germany versus France (1940)

Prereqs: `GER_around_maginot` and `GER_operation_weserubung` completed before
`GER_war_with_france`; not in a faction with FRA (national_focus). The focus
gives a `take_state_focus` wargoal with `expire = 0` and 5 threat.

Checklist:
1. Wargoals against Belgium/Netherlands as needed (focus or justification).
2. Hold the Maginot front with infantry; fort levels there make an assault a
   grind `[L]`.
3. Main thrust: armor/motorized through the Ardennes toward the Channel coast
   and Paris; air superiority over Belgium/northern France first; infantry
   behind in second echelon.
4. Pincer on the Allied armies in Belgium: close the pocket on the coast.
5. Take Paris (state 16): France's capitulation is likely at the capital
   bonus; verify loss of 80% core VPs if France resists (section 9).
6. Peace conference: occupy first; claim cores; consider puppet/Vichy remainder.

### 12.3 Germany versus Britain (naval and air requirements for invasion)

See sections 6.3 and 7. Minimum gates: France down and Channel bases captured;
air superiority over the Channel for 14 days; Royal Navy neutralised with
naval bombers plus escorts; supremacy above 50% in each sea region on the
route; 40%+ intel; transport convoys for the landing divisions; a port as the
objective; the Sea Lion focus (if on the German tree) for its bonuses.
Realistic window: only after the fleet and air build-up are in the field; if
the air war over the Channel is not won within about 2-3 months of fighting,
postpone and keep strategic pressure instead `[?]`.

### 12.4 Germany versus USSR (1941+)

Prereqs: `GER_war_with_the_ussr` (needs `GER_anti_comintern_pact`, excludes
`GER_alliance_with_the_ussr`) and a Soviet border; a land bridge via Poland.
Historical AI would only attack with 1,200,000+ manpower and no neighbour
enemies; a human should use the same standard.

Checklist:
1. Finish western enemies; leave occupation and garrison forces; maintain naval
   and air units in the west.
2. Supply: plan rail and trucks; the east has poor infrastructure and mud.
3. Three army groups north (Leningrad 195), centre (Moscow 219), south
   (Kiev/Stalingrad 217); armor spearheads from each, pockets at the largest
   salients.
4. Air superiority first; destroy the Soviet air force on the ground and in the
   air; CAS on the pockets.
5. Winter: expect attrition and supply issues `[?]`; consolidate, keep a
   reserve.
6. Capitulation needs 80% of Soviet core VPs lost; Moscow capture adds
   capital bonus. Peace: occupy all, then claim or puppet.
7. Greater German Reich (section 11) only after Moscow, Leningrad and
   Stalingrad fall and all states are fully controlled.

### 12.5 Nationalist China (vs warlords, communists, Japan)

From `china_nationalist.txt` and events `wtt_china.*`:
- `CHI_subjugate_the_warlords` (needs `CHI_prioritize_the_interior`): sends
  event to GXC, YUN, SHX, XSM, SIK. Accept: they become puppets. Refuse: China
  gets a puppet wargoal at once (`wtt_china.3`), so declare immediately and
  fight a short war. Integrating puppets via decisions
  (`CHI_integrate_the_warlords`) costs PP (file shows 150 on one entry) `[?]`.
- `CHI_anti_communism`: wargoal vs the communists (PRC); fight only with
  surplus force.
- `CHI_pick_a_fight_with_japan`: gives an immediate `annex_everything` wargoal
  vs Japan or its Manchurian puppet; needs `CHI_united_front` and
  `CHI_anti_communism`; Japan's own attack comes by its focus chain. Do not
  provoke before defences are up.

Checklist:
1. Peace years: take the unity route, build infantry, AA, engineers, fortify
   Shanghai/Nanjing (`CHI_fortify_shanghai`).
2. Warlords: negotiate or fight one at a time; avoid war on two fronts.
3. When Japan lands: hold river lines, ports, mountains; use reserve divisions
   to plug breakthroughs; air superiority is not expected early.
4. Counter-offensive only with armor, artillery and air parity; encirclement
   of landed Japanese groups; retake Manchuria and Taiwan later.
5. Peace: Chinese-core states are x0.25; MAN is a puppet to take or release.

### 12.6 Poland META defence vs Germany/USSR

Use `poland-meta.md` for route choice. War specifics:
- Fight on rivers (attacker -30%/-60%) and forts; yield open plains; keep
  mobile reserve behind hubs; AT and AA supports; engineers.
- Keep Warsaw, Krakow, Lodz, Lwow protected: they weigh most for surrender.
- Do not defend the whole border; Germany attacks about September 1939 with
  heavy armor and air; the USSR is not scheduled to attack before 1941 under
  the historical AI (section 8.2) `[?]`.
- UK/FR guarantee: they declare war but can only help via a Rhine offensive;
  do not plan on rescue.
- Only META-permitted cheats the policy allows via the UI; not covered here.
- Peace: if the Reich capitulates, claim Prussian/Silesian cores (x0.25).

---------------------------------------------------------------------------

## 13. Quick decision table (for an event-driven agent)

| Trigger | Action |
|---|---|
| Army idle on a front for > 3 days, planning < 30% | wait; do not execute |
| Planning >= 25% and air superiority held | `hoi4_army_offensive` then `hoi4_army_plan` execute |
| Spearhead supply low or org < 40% | stop plan (`hoi4_army_plan execute=false`), wait |
| Enemy armor seen, my AT low | add AT supports/production; avoid assaults |
| Pocket found (enemy neck, two armies able to meet) | order pincers at same rear province |
| Enemy air wings > mine in a region | fighters to air superiority there |
| Wing out of range of front | rebase (no tool) / stop a mission |
| War declared by enemy | event window: handle with `hoi4_events`/`hoi4_event_choose` |
| Enemy capitulated | check peace conference; occupy; then bid |
| Landing planned | check air + naval gates; do not start without them |

---------------------------------------------------------------------------

## 14. End matter

### (a) Automatable standing rules

1. Every division belongs to an army with a general; armies <= 24 divisions.
2. Every army has a front toward each enemy it faces; offensive lines only
   when planning >= 25% and air superiority is held.
3. Fighters on air superiority over the front regions; CAS over active battles.
4. Stop plans when the spearhead's supply or org falls below the safe level.
5. Keep reserve divisions behind high-VP hubs; do not strip the capital region.
6. Declare war on the first available day after the war goal appears and the
   border checks pass.
7. On capitulation, occupy before the conference and open it the same day.
8. Keep convoys escorted and convoy production non-zero.

### (b) Needs live decisions

- Whether to start a war or invasion at all (readiness, FAIR review).
- Which of two simultaneous threats gets the reserves.
- Choice of pincers and encirclement target.
- Naval invasion go/no-go and landing site.
- Peace bids (what to claim versus puppet), contested-bid decisions.
- Event options with political consequences (faction, ideology, ceding states).
- Switch from attack to defence after a failed offensive.

### (c) Tools

Existing (from `native/src/launch/macos.rs`): `hoi4_armies`, `hoi4_army_front`,
`hoi4_army_offensive`, `hoi4_army_plan`, `hoi4_army_move`, `hoi4_army_create`,
`hoi4_army_split`, `hoi4_army_group_create`, `hoi4_army_commander`,
`hoi4_air_wings` (read only), `hoi4_air_mission`, `hoi4_diplomacy` (read),
`hoi4_diplomacy_act`, `hoi4_events`, `hoi4_event_choose`, `hoi4_states`,
`hoi4_division_templates`, `hoi4_train`, `hoi4_deploy_locations`,
`hoi4_production_add/set`.

No native tool exists for (checked against `native/src` and `docs/`):

| Gap | Evidence |
|---|---|
| Navy fleets, task forces, missions, naval production | `docs/MANAGEMENT_MATRIX.md` lists "Dockyards, fleets, navy missions, invasions: missing"; no navy tool in `macos.rs` |
| Naval invasion planning/launch | same; `docs/handoff_2026-10-11_campaign_run3.md` says invasion and convoy escort were NOT built or tested (legacy Python tool `hoi4_navy_air` covers only basic naval missions) |
| Peace conference bidding | `docs/MANAGEMENT_MATRIX.md` "window is detected, nothing more"; legacy Python `hoi4_peace` exists but is not in the native list |
| Encirclement search | No native tool; Clausewitz-MCP has one; this file's section 3 is the manual method |
| Theatre / army-group order scope | `hoi4_army_group_create` exists; no theatre assignment, reinforcement priority, area-defense or spearhead order in the native list |
| Air wing creation, rebasing, air-base changes | `hoi4_air_wings` is read-only; `hoi4_air_mission` only assigns missions |
| Execute-mode switch (careful/rush), army group orders | none |

Until these exist, wars that need a naval landing, a conference bid or a
rebasing step must stop and hand back with a clear blocker; do not improvise
raw input.
