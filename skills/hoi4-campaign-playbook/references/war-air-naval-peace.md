# War, air, naval and peace reference

Scope: how wars are won against the historical AI under FAIR rules, and which
tool acts on each step. Read the tables first; read the prose only for the step
you are on. Companion files: `army-organisation.md`, `supply-logistics-deployment.md`,
`generic-playbook.md`, `strategy.md`, the country files, `rules-and-modes.md`.

FAIR in one line: strong, clever, normal-UI play. Forbidden: space marines,
invasion or paradrop spam, exploit templates, AI-baiting, engine bugs.
Everything below is FAIR unless it says META.

---------------------------------------------------------------------------

## 1. Land combat: what decides battles

### 1.1 Numbers that matter

| Mechanic | Value |
|---|---|
| Hit dice | org damage d4 x 0.053, strength damage d2 x 0.06 |
| Armour outclassing enemy piercing | org die becomes d6 (about +40% org damage) |
| Attacks per hour | hardness-weighted attack / 10; defences = defence / 10 (breakthrough instead of defence when attacking) |
| Hit chance | 10% while the target has defences left, 40% when none |
| Planning bonus | +2% a day to a 30% cap (15 days); decays 1% a day under a battle plan, 3% a day under manual orders |
| Planning with no deployed HQ | cap and speed x0.8; HQ 0-4 provinces behind the front x1.0 to x0.8 |
| Dig-in | +2% defence per level |
| Fort | -15% attack and breakthrough per level for the attacker; each extra attack direction cancels one level (never the last) |
| Rivers (attacker only) | small -30% attack (-25% speed); large -60% (-50% speed) |
| Terrain attack penalty (attacker) | forest -15%, hills -25%, urban -30%, jungle -30%, marsh -40%, mountain -50%, plains/desert 0 |
| Combat width per province | plains/hills/desert 70, urban 80, forest/jungle 60, mountain/marsh 50 (extra side 35/40/30/25) |
| Battalion width | infantry 2, artillery 3, support 0 |
| Over-width penalty | -1% per % over width, hard cap -33% (cannot join beyond that) |
| Stacking penalty | -2% per division above 5 (+3 allowed per extra direction) |
| Reserve join chance | 2% an hour per idle reserve; speed and radio help |
| Two simultaneous combats (defender) | -50% |
| Encircled | -30% (all neighbouring land provinces enemy-controlled); no manpower, equipment or fuel even with air supply; routed units with no retreat shatter |
| Out of supply | attacker -25% attack / -65% defence; defender -35% / -15%; max org capped at 35% |
| Supply grace | 72 hours of carried supply |
| Enemy air superiority | -35% defence, -30% speed; terrain shrinks it: urban -50%, jungle -25%, forest and mountain -10%, hills -5%, plains and desert full |
| Taking a province | conqueror loses 20% of max org per province taken |
| Night | -50% attacks, both sides |
| Commander | about +2.5% per skill point; 24 divisions per general, 5 armies per marshal; marshal stats reach armies at 50% |
| Division air attack | cancels 0.75 x AA^1.5 / (AA^1.5 + 625) of the enemy-air-superiority penalty: 20 AA 9%, 40 22%, 60 32%, 100 46%, never above 75% |

### 1.2 How to read a battle in one glance

1. Org is the clock. A division at 0 org retreats; with nowhere to retreat it is
   destroyed. Wars are won by draining org faster than the enemy, not by
   killing manpower.
2. Defence and breakthrough are the counter pair: attackers with more
   breakthrough than the defender has defence hit 40% instead of 10%. Armour
   spearheads crack infantry lines; defenders want high defence, forts and dig-in.
3. Piercing against armour: infantry without anti-tank takes the d6 org hit.
   Add AT and engineers against armour; division AA against CAS (§5.3).
4. Width: attackers beyond width waste strength, and more than 5 divisions per
   stack adds -2% each. Many thin attacks from several provinces beat one stack.
5. Supply: a spearhead beyond supply range fights on 72 hours of carried supply,
   then max org falls to 35%.

### 1.3 Combat tactics and the counter wheel

A tactic is chosen per combat every 12 hours; better commanders (skill
advantage) unlock the stronger options and pick counters.

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
`defensive_doctrine` or skill > 2), backhand blow (skill > 4 or
`defensive_doctrine` with skill > 3; defender +25%, attacker -20%), tactical
withdrawal (counters encirclement). Practical rule: best attack generals on the
decisive armies, best defence generals on the line the enemy will hit.

---------------------------------------------------------------------------

## 2. Orders and battle plans

Tools: `hoi4_army_front` draws a front line, `hoi4_army_offensive` an offensive
line, `hoi4_army_plan` executes or stops, `hoi4_army_attack` does all three in
one call, `hoi4_army_move` is a manual order, `hoi4_front_plan` ranks attack axes
and holding lines per border.

| Order | Use | Notes |
|---|---|---|
| Front line | Hold or screen a border; units spread across the line | Needs a foothold: the army must already stand on or in the enemy state |
| Offensive line | Attack through the front into a named state | Drawn after the front; execute afterwards |
| Spearhead | One fast army ordered at a deep target | Single armies only; no tool |
| Area defence | Defend a province set; units weigh VPs, ports, hubs (capital 50) | No tool |
| Manual move | Mop up, reach a foothold, plug a hole | Planning decays 3% a day instead of 1% |
| Execute modes | careful / balanced / rush: attack-score thresholds 25 / 0 / -200; careful needs full supply and refuses forts 5+ | No tool switches the mode |

Rules of thumb:

- Plan first, attack second. Draw front and offensive line, wait 12-15 days
  for planning before executing. Execute at 0% only before the enemy digs in.
- The planner spreads 2-3 divisions per province; one army over a very long
  front silently thins the attack.
- Give every army a general; small armies (under about ten divisions) often
  start without one.
- At most 24 divisions per army and 5 armies per army group.
- Do not edit orders daily: each new order resets that army's planning; act
  only on a trigger (section 13).

---------------------------------------------------------------------------

## 3. How to encircle (FAIR, human-doable)

Encirclement means trapping enemy divisions in a pocket by taking the provinces
behind them; no engine tricks. `hoi4_encirclement_candidates` lists pockets,
necks, pincer arms and the divisions each arm needs.

1. Find the neck: an enemy salient whose only land route back passes one or two
   provinces you can reach. The game rechecks pockets every 72 hours per army.
   Typical Polish case: the western bulge between Silesia and East Prussia is
   pinched at Lodz-Warsaw.
2. Two pincers, one anvil. Two armour or motorised armies (hardness above 0.5
   unlocks the breakthrough and blitz tactics) attack on either side of the neck;
   an infantry army pins the pocket from the front, so the defender fights in
   several directions (cancels fort levels, -50% multiple-combat penalty).
3. Plan: a front for the anvil and offensive lines for both pincers aimed at the
   same rear province, not at the pocket. Execute all three on one day after
   planning is high; air superiority and CAS over the pincers.
4. Close and hold. Once the pincers meet, turn them to a front facing out and let
   infantry compress the pocket (pocket reduction: OP13).
5. Do not overextend. Pincers stay in supply; stop a spearhead one province
   before supply breaks and keep a second echelon to close behind it. Push on
   only with org above about 50% of max; manual orders decay planning 3% a day.
6. Avoid the two classic traps: a pincer across a large river at -60%, and
   pocket-closers pushed so far that they are encircled through the same neck
   mirrored (fall back: OP16).

---------------------------------------------------------------------------

## 4. Defensive lines and reserves

| Situation | Setup |
|---|---|
| Static defence | Front line on rivers, mountains, forest; one fort level is already -15% for the attacker; engineers speed dig-in; 24 divisions max per army |
| Wide front, thin forces | Defend only terrain-favourable provinces; give up open plains with a fighting retreat, keep org and equipment |
| Mobile reserve | A motorised or armoured army one province behind the hub, not on the line; counter-attacks pockets and breakthroughs; reserves join at 2% an hour, so they are no emergency fix |
| Holding VPs | Capital and high-VP cities weigh most in area defence (capital 50, high VP 10) |
| Surrender pressure | Countries capitulate at 80% loss of core VPs (section 9); protect VP-dense cities first |
| Do not | Leave a lone division in a pocket-prone salient; defend a coast with the main army while the land front is empty; assault forts with infantry without artillery |

Cheap defender stack: river + fort + dig-in + air superiority + a defence general
(`defensive_doctrine`, `unyielding_defender`) + AT and engineers. A defender in two
simultaneous combats is at -50%: avoid lines where one division is attacked from
two provinces.

Under enemy air superiority infantry retreats 30% slower and is easy to pocket; under about 10 divisions per front tile
treat the line as open to tanks, so win or contest the air before holding a plain front. With enough divisions hold in
two lines so the rear line regains organisation while the front fights.

---------------------------------------------------------------------------

## 5. Air war

### 5.1 Priorities

1. Air superiority first: fighters on `air_superiority` over the decisive region
   1-2 weeks before an offensive (enemy superiority: -35% defence, -30% speed).
2. CAS over the decisive battles, at most 3 CAS per enemy combat width; CAS at
   night works at 10%. CAS damage is small since 1.17: CAS is for the +25%
   air-support bonus and for superiority. Against AA-heavy enemies keep CAS low.
3. Tactical bombers: logistics strike behind the front, enemy air bases; slow to
   settle after a region change (5.3).
4. Strategic bombers: only in a war longer than a year.
5. Interception at home only while enemy bombers operate there.

### 5.2 Mission cheat sheet (`hoi4_air_mission`)

Missions: `air_superiority, cas, interception, strategic_bombing, naval_strike,
kamikaze, port_strike, logistics_strike, air_supply, training, naval_mines,
naval_minesweeping, recon, naval_patrol`, given over the strategic region that
contains a named state. `hoi4_air_hold` stops a wing.

| Wing type | Default mission | Where | Notes |
|---|---|---|---|
| Fighter / heavy fighter | air_superiority | the region over the decisive front | Keep it flying; interception only against bombers |
| CAS | cas | the region with own attacks | Needs superiority or is shot down; logistics_strike when no own attack for 3 days |
| Tactical bomber | logistics_strike | enemy rail, hubs, air bases behind the front | strategic_bombing only in long wars |
| Naval bomber | naval_strike / port_strike | sea region with the enemy fleet / its port | Needed for the Channel |
| Carrier air | naval_patrol / naval_strike | with the task force | Only with carriers |
| Recon / naval patrol | recon, naval_patrol | the target sea or invasion area | Spots fleets; naval intel feeds dominance (6.1) |

### 5.3 Range, bases, efficiency

- A wing flies only over regions in range of its base; rebase forward as the
  front moves with `hoi4_air_rebase`; `hoi4_air_wing_create/split/merge/disband`
  shape wings, `hoi4_air_regions` and `hoi4_air_region_probe` show where wings and
  enemy planes are.
- Base capacity is 200 planes per level; overcrowding costs efficiency.
  Missions burn fuel per plane; without fuel missions run at 25%.
- A region change cuts efficiency to 90%, then it regains per day: fighters and
  CAS 0.888, tactical and naval bombers 0.192, strategic bombers 0.072. Re-region
  fighters and CAS freely; leave bombers where the war will stay.
- Division AA cuts CAS damage up to 75% at about 55 division air attack; token AA
  does little. Static AA hits bombers, port and logistics strikes, not CAS.
- Air base access is a diplomatic request: ask allies and co-belligerents before an air
  campaign; each access already granted makes the AI less willing to grant the next.

### 5.4 Air versus ships

Naval and port strike need superiority; carrier planes get x12 naval strike
damage when their carrier is present. Land-based naval bombers need base range to
the sea region.

---------------------------------------------------------------------------

## 6. Naval warfare

Tools: `hoi4_fleets` reads fleets, task forces, missions and regions;
`hoi4_fleet_mission` sets a task force's mission; `hoi4_fleet_region` adds or
removes a fleet's sea regions; `hoi4_fleet_create/split/merge/assign/reserve/
balance/delete` shape fleets; `hoi4_fleet_admiral` assigns admirals.

### 6.1 Concepts

| Term | Meaning |
|---|---|
| Sea control | A region is controlled at 60% of the needed dominance over enemies; minimum dominance per sea zone 250 ocean, 100 shallow sea or fjord |
| Dominance by mission | Only patrol builds dominance; a strike force multiplies it (1000 strike-force dominance +60%, -5% per extra region); raiding lowers the enemy's; escort adds none; hold and training 0 |
| Dominance speed | Gains 2% a day, decays 4% a day: about 50 days of patrol to full value |
| Naval intel | Below 40% naval intel dominance is reduced, to half at 10% or less |
| Dominance modifiers | Radar +5%, air base +2%, naval base +1% per level toward the region target |
| Air superiority over the sea | Adds up to +100% to a fleet's effective ship count |
| Convoy | Each division moved by sea and each supply shipment needs convoys; losses cut efficiency by 1.25x the loss |
| Piercing against armour | 100% damage when piercing is at least the armour, 70% at 75-99%, 40% at 50-74%, 30% at 10-49%, 10% below; heavy battery I-IV piercing 31/36/40/45; battleship armour I-III 30/36/40, battlecruiser 22/28/34, super-heavy 55; heavy battery II is the minimum against 1936 Royal Navy battleships |

### 6.2 Missions and what to use them for

| Mission | Use |
|---|---|
| Patrol | The dominance builder: one task force per region you want |
| Strike force | Capitals and carriers; engages only after an own patrol spots the enemy; at most 2 regions |
| Convoy raiding | Submarines cutting enemy imports; lowers enemy dominance; war score by IC sunk |
| Convoy escort | Destroyers protecting own routes; mandatory on a long supply route; adds no dominance |
| Invasion support | Escorts and shore bombardment (up to -33% for the defender) for a landing |
| Mines / minesweeping | Near useless against the AI |
| Training | Peace-time XP |

Fleet composition:

- Surface task force: full screening needs 3 screens per capital or carrier and
  1 capital per carrier (a heavy cruiser is the cheapest capital). Order of the
  fight: light cruisers kill the enemy screens, then torpedo destroyers reach the
  capitals and carriers; cheap one-gun destroyers soak light attack (NA48, NA49).
  Carriers about 4 per fleet; no battleships; air superiority over the battle. In 1939-41 land-based naval bombers and carriers decide
  fleet fights more than another battleship.
- Submarines: task forces of 8-12, one per raid region; shallow seas give +33%
  submarine visibility, deep ocean -15%.
- Convoy defence: destroyers on escort in every region your supply or troop
  convoys cross; keep spare convoy production (`hoi4_convoys`).

### 6.3 Blockading or defeating Britain

1. Britain is an island supply problem. A blockade alone rarely capitulates it;
   use it to starve production, drain the Royal Navy and gain war score before an
   invasion.
2. Sequence: (a) build torpedo destroyers, U-boats, naval bombers and fighters;
   (b) win air superiority over the North Sea, Channel and Western Approaches from
   French and Dutch bases; (c) U-boat task forces of 8-12 raiding in the deep-ocean
   approaches; (d) naval bombers on ports and fleets; (e) only then plan a landing.
3. Invasion trigger (all must hold):
   - France has capitulated and the Channel bases are yours.
   - Air superiority over the landing zone for at least two weeks.
   - The Home Fleet is destroyed or contained; 60% control in every route region.
   - 40%+ naval intel (recon, radar, patrols).
   - Landing craft researched (invasion capacity, §7).
   - A port as the objective: without one the troops run out of supply after 72 hours.
4. The Sea Lion focus `GER_operation_sea_lion` needs war with ENG, army manpower
   over 1,000,000 and more than 90,000 infantry equipment in armies. Its 180-day
   idea gives +25% naval invasion prep speed, +25% shore bombardment, -15% naval
   invasion penalty, +10% strategic bombing and +5% attack against ENG.

---------------------------------------------------------------------------

## 7. Naval invasion (one proper landing)

FAIR means one well-prepared operation with follow-up reinforcement through the
captured port. Not allowed: repeated empty-port landings, landings used to reset
AI behaviour, or space-marine / paratrooper width tricks. Tool: `hoi4_naval_invasion`
(one order; `with_harbor` for the floating-harbour variant).

| Parameter | Value |
|---|---|
| Capacity by tech (divisions / plans / prep days) | base 4/1/60; transport 6/2/50; landing craft 10/5/40; tank landing craft 15/9/30; marine subdoctrines add more |
| Prep time | does not depend on the number of divisions: fill the cap |
| Launch gates | Sea control (60% of the needed dominance) in every crossed region and 40% naval intel, checked at launch only; once launched the landing goes ahead |
| Army orders | an army with a front or fallback line never starts the invasion: clear them first |
| Landing penalty | -50% attack, reduced by landing-craft techs; worse for heavy equipment |
| Shore bombardment | up to -33% for the defender; heavy attack counts twice light attack; split over every land tile the sea region touches (NA38) |
| Transports spotted | invasion convoys are spotted 10x more easily |
| Supply on shore | 72 hours without a port; a floating harbour gives about 30 days |

Procedure:

1. Pick a landing province on or next to a port with low forts and low enemy
   density (plains ports, not urban); avoid fortified beaches.
2. Gates in order: air superiority over the landing region and the sea route;
   60% control in every route region (patrols started 50+ days earlier); 40%
   naval intel; destroyers on escort; naval bombers and fighters on port strike
   and superiority.
3. Detach the best divisions into a fresh army with no front or fallback line
   (marines only up to the cap; special-forces width exploits are forbidden). Use
   the full cap; engineers and armour only if landing craft allow.
4. Order the invasion, wait out prep, keep air and fleet missions active until
   the landing completes.
5. After landing: take the port within the first days, ship supply and the second
   wave through it, widen the beachhead with front and offensive lines, keep
   fighters over it and escort its convoys.
6. Never repeat the operation as a feint or stack landings to exhaust the AI.

Landing on a defended coast (strong-player checklist, FAIR):

- Months before: build the landing force (T18 marines, T19 follow-up, T16 armour after the port falls), 100+
  factories of cheap CAS, heavy-battery cruisers for bombardment (NA42), ports linked by level-5 rail on the home
  coast, the navy chief and command-power advisors for the operation; raise encryption a month before launch.
- Pick the target for its port and terrain: a level-10 port on a peninsula (reserve waves land behind it and close
  the pocket), under your own air cover, with sea regions that touch few land tiles (NA38).
- Launch: every planned landing on the same day (NA41), 2 divisions per landing tile, never more marines than the
  tiles need; Force Attack the first wave or hand it to the commander with command power; backup waves already
  planned.
- After landfall: take the port, motorise the hubs, ship supply; push out on several axes so the defender cannot
  mass; keep rangers and mountaineers on fronts rather than battle plans across rivers (OP49).

---------------------------------------------------------------------------

## 8. War declaration, timing and diplomacy

### 8.1 Mechanics

| Item | Value |
|---|---|
| War goal needed | Declaring war needs a war goal (focus, justification, event) |
| Justification time | at least 10 days; shorter with high world tension (-50% cost at 100% tension), cheaper against a major already at war (-75%) |
| Justify cost | 0.2 PP a day base; scales with the number of wars and justifications |
| Threat | war with goal 10, without 14; guarantee -5; faction join +4; join attacker +2; annexation +0.1 to +0.2 per state |
| Truce | 180 days base; breaking it by joining costs 200 PP scaled |
| Guarantee cost | 15 PP scaled by the number already guaranteed |
| Breaking a guarantee | -0.2 war support |
| Focus war goals | some never time out (e.g. `GER_war_with_france`) |
| Justified goals | Conquer goals expire 60 days after completion; claimed and core goals never expire |

Declaration rule: complete justification or focus, check the goal exists, then
declare on the first possible day; never run fast across the completion date.
Declare only with armies on the border, planning built (section 2) and
`hoi4_war_risk` showing who joins the enemy.

### 8.2 Faction and guarantee behaviour

- UK and France guarantee Poland from 1939-01-01; in a 1936 start the guarantee
  comes later through their focus and event chains. The AI honours a guarantee by
  declaring war on the attacker: expect UK and France in the war when Germany
  invades Poland.
- The historical Soviet AI stays out of Poland before 1941; its hostility to
  Germany starts only with the anti-fascist focus and after 1942-01-01 or an
  existing war. Assume no timetable beyond that.

---------------------------------------------------------------------------

## 9. Capitulation, peace conference

### 9.1 Capitulation (when a country quits)

- Default surrender limit: 80% of core victory points lost. Countries with no
  cores use 70% of that; the minimum non-forced limit is 20%. No capitulation in
  the first 7 days of a war.
- War support below 50% lowers the limit up to 30%; collaboration lowers it up to
  30%. National spirits change it (e.g. Belgium -50%, France's Disjointed
  Government -50%).
- The country capitulates to the enemy that dealt the most war score; the
  capital holder gets x1.5 plus 150 score, so take the capital.
- Capitulation does not end a war while major allies fight on; a minor can be
  forced out when its majors fall.
- Capitulation gives the winner 50% of the loser's stockpile; annexation 100%.

### 9.2 Peace conference

Opens when a war is won. Tools: `hoi4_peace_status` and `hoi4_peace_options` read
the window (score, turn, demand kinds, states); `hoi4_peace_take` adds take,
puppet or liberate demands and reports each cost; `hoi4_peace_submit` ends the
round; `hoi4_peace_exit` leaves (accepted bids persist); `hoi4_war_goal_targets`
ranks the states worth demanding.

| Rule | Detail |
|---|---|
| Points | about 1.35 x enemy state value x participation share; conditional surrender scales by surrender progress |
| Timing | a sole victor gets full score on turn 1; several victors are paid in 20% steps over five turns |
| Bids | demands on the same state are contested; the price rises x1.2 per contested bid |
| Refund | full on a round-one contest, then 92/84/76% |
| Uncontested price | rises 1.15-1.60x per turn: bid early on key states |
| Pass / Done | Pass gives others their turn; Done leaves and accepted bids persist; contested bids can pull you back |
| State you do not control | x10 |
| State an ally controls | x0.5 |
| Influence | 0.7-1.0 by distance to capital, cores and controlled land; +0.05 per adjacent uncontested state: grow outward, do not snake |
| Occupied state | x0.8; compliance x0.9 at 30-70%, x0.8 above 70% |
| Core / claim | own core x0.25 (x0.5 if the taker is a puppet); claim x0.9 |
| Ally major's core | x3.0 |
| Ideology | fascism take x0.9, puppet x0.95, liberate x1.3; communism puppet x0.9, liberate x1.3; democracy take (non-core) x1.3, puppet x1.2 |
| Defensive war | take x1.1 (liberate, force government x0.95) |
| Add-ons | demilitarised zone, war reparations, resource rights, dismantle industry: +25% each |
| Ships | 0.005 score per IC of capital ship |
| Actions | take state, puppet, force government, liberate, take navy, add-ons |

Order of operations:

1. Occupy first: occupied states x0.8, unoccupied x10.
2. Turn 1: bid the high-value states allies also want; leave states nobody else
   wants for later.
3. Take cores, claims and industrial or VP-rich land; puppet the rest when you
   cannot afford it.
4. A contested bid: forfeit and take the refund, or re-bid only if the state is
   worth the higher price.
5. When score is spent or nothing worthwhile remains, exit.

How to claim each target (exact states: `hoi4_war_goal_targets`, `hoi4_states`):

| Target | How |
|---|---|
| Poland | Occupy western Poland and Warsaw; German cores and claims first (cores x0.25); take or puppet the rest; if the USSR also wins, bid first on states it cannot reach |
| France | Occupy Paris and the northeast; Alsace-Lorraine and other cores/claims cheap; puppet the remainder; ships via take navy |
| UK | Occupy the home islands and port cities; colonies cost more with distance; capital ships on bid |
| USSR | Occupy to the Urals line before capitulation; far states are dear; puppet the east; a Russian successor puppet is cheaper than annexation |
| Japan | Occupy the home islands; its USA-only cost modifiers do not apply to other victors; puppet if annexation is too dear |
| Chinese warlords (CHI) | CHI takes from the Reformed National Government at x0.75 |

---------------------------------------------------------------------------

## 10. How the historical AI behaves

The game rules set each major's AI behaviour; campaigns use the historical
option (check the game rules screen).

| Country | Typical behaviour |
|---|---|
| Poland (AI) | Defends the western front, relies on the UK/FR guarantee |
| UK / France | Both guarantee Poland from 1939-01-01; France concentrates at the Maginot Line; the AI is careful with early invasions |
| Soviet Union | Stays out of Poland until 1941, builds up, focus-driven ultimatums to the Baltics and Finland; war with Germany follows its own focus chain |
| Italy, Japan | Follow their historical focus chains |
| China AI (KMT) | Subjugate-the-warlords events with puppet/refuse choice; Japan attacks by its own focus chain |

General AI traits (more in OP41): it reacts to visible threat, shifts units to
the most threatened front, builds large fixed lines and defends poorly against
an air-and-armour breakthrough. Never bait it (FAIR).

---------------------------------------------------------------------------

## 11. Greater German Reich

Decision `form_greater_german_reich`.

- Needs Waking the Tiger; without it the decision does not appear.
- Visible for original GER under fascism once France has capitulated while at
  war with GER, or GER owns and fully controls Paris (state 16), or controls
  Moscow (219).
- Available only when GER is not a subject and fully controls (itself or via a
  subject) Paris 16, Prague 9, Warsaw 10, Leningrad 195, Moscow 219, Stalingrad
  217, and every owned state is fully controlled (no occupied, split-control or
  resistance-held state).
- Cost 25 PP. Reward: +5% war support, +5% stability, the Greater German Reich
  tag and a Germania victory point; cores on some states.

Never a goal in itself: finish the Soviet war and reach full control first.

---------------------------------------------------------------------------

## 12. War-by-war timelines and checklists

Dates are for a 1936 start with Germany: targets, not requirements; triggers
matter more than dates.

### 12.1 Germany versus Poland (1939)

Prereqs: Austria and Czechoslovakia handled by focus (`germany-fair.md`).
`GER_danzig_or_war` needs at least 950,000 army manpower (about 100 divisions),
Danzig (state 85) still Polish and no Polish civil war; it fires event
`germany.80`, where Poland cedes Danzig/807 or fights. Poland usually fights.

Checklist:
1. Armies on the border: north (East Prussia/Pomerania), south (Silesia, Slovakia
   if puppet), centre. Armour spearheads on both flanks.
2. Planning built; fighters on air superiority over the Polish western regions.
3. Declare (war goal active). UK and France are likely to join.
4. Day 1-10: pincers toward Warsaw (state 10) from north and south; infantry pins
   the Posen bulge; avoid the Vistula and Narew unless bridged.
5. Day 10-30: close the pocket west of the Vistula, hold; take Warsaw.
6. After capitulation: occupy all Polish states before the conference.
7. Move the army west before the Allies attack the Rhine; never leave only a few
   divisions facing France.

### 12.2 Germany versus France (1940)

Prereqs: `GER_around_maginot` and `GER_operation_weserubung` completed before
`GER_war_with_france`; not in a faction with FRA. The focus gives a take-state war
goal that never expires and 5 threat.

Checklist:
1. War goals against Belgium and the Netherlands as needed.
2. Hold the Maginot front with infantry; its forts make an assault a grind.
3. Main thrust: armour and motorised through the Ardennes toward the Channel coast
   and Paris; air superiority over Belgium and northern France first; infantry in
   the second echelon.
4. Pincer on the Allied armies in Belgium: close the pocket on the coast.
5. Take Paris (state 16); if France holds on, check the 80% core-VP loss (section 9).
6. Peace conference: occupy first; claim cores; puppet the remainder.

### 12.3 Germany versus Britain (invasion gates)

See sections 6.3 and 7. Sea Lion only with: France down and the Channel bases
held; air superiority over the Channel for 14 days; the Royal Navy neutralised by
naval bombers; landing craft (10 divisions, 5 plans, 40 days prep); 60% control in
every route region at launch; 40% naval
intel; radar on the coast; a port as the objective; the Sea Lion focus for its
bonuses. If the air war over the Channel is not won within about 2-3 months,
postpone and keep up the naval and air pressure.

### 12.4 Germany versus USSR (1941+)

Prereqs: `GER_war_with_the_ussr` (needs `GER_anti_comintern_pact`, excludes
`GER_alliance_with_the_ussr`) and a Soviet border via Poland. The historical AI
attacks only with 1,200,000+ manpower and no enemy neighbours; use the same
standard.

Checklist:
1. Finish the western enemies; leave occupation and garrison forces; keep naval
   and air units in the west.
2. Supply: rail and trucks; the east has poor infrastructure and mud.
3. Three army groups: north (Leningrad 195), centre (Moscow 219), south
   (Kiev/Stalingrad 217); armour spearheads in each, pockets at the largest salients.
4. Air superiority first; CAS on the pockets.
5. Winter: no offensives in mud, snow or cold (OP06); consolidate, keep a reserve.
6. Capitulation needs 80% of Soviet core VPs; Moscow adds the capital bonus. Peace:
   occupy all, then claim or puppet.
7. Greater German Reich (section 11) only after Moscow, Leningrad and Stalingrad
   fall and all states are fully controlled.

### 12.5 Nationalist China (warlords, communists, Japan)

- `CHI_subjugate_the_warlords` (needs `CHI_prioritize_the_interior`) sends an
  event to GXC, YUN, SHX, XSM, SIK. Accept: they become puppets. Refuse: China gets
  a puppet war goal at once (`wtt_china.3`): declare immediately and fight a short
  war. Integrating puppets (`CHI_integrate_the_warlords`) costs PP (150 on one
  entry; check the decision tooltip).
- `CHI_anti_communism`: war goal against the communists (PRC); fight only with
  surplus force.
- `CHI_pick_a_fight_with_japan`: immediate annex-everything war goal against Japan
  or its Manchurian puppet; needs `CHI_united_front` and `CHI_anti_communism`.
  Never provoke before the defences are up.

Checklist:
1. Peace years: unity route, infantry, AA, engineers; fortify Shanghai and Nanjing
   (`CHI_fortify_shanghai`).
2. Warlords: negotiate or fight one at a time; never two fronts.
3. When Japan lands: hold river lines, ports, mountains; reserves plug
   breakthroughs; no air superiority early.
4. Counter-offensive only with armour, artillery and air parity; encircle landed
   Japanese groups; retake Manchuria and Taiwan later.
5. Peace: Chinese cores x0.25; MAN is a puppet to take or release.

### 12.6 Poland META defence against Germany and the USSR

Use `poland-meta.md` for the route. War specifics:
- Fight on rivers (attacker -30%/-60%) and forts; yield open plains; mobile
  reserve behind hubs; AT, AA and engineers.
- Protect Warsaw, Krakow, Lodz and Lwow: they weigh most for surrender.
- Do not defend the whole border; Germany attacks about September 1939 with armour
  and air; the historical USSR does not attack before 1941 (section 8.2).
- UK and France declare war but help only by a Rhine offensive; never plan on rescue.
- Peace: if the Reich capitulates, claim Prussian and Silesian cores (x0.25).

---------------------------------------------------------------------------

## 13. Quick decision table

| Trigger | Action |
|---|---|
| Planning under 90% of the cap and under 15 days | wait; do not execute |
| Planning at 90% of the cap or 15 days, air superiority held | `hoi4_army_attack` (or `hoi4_army_plan` execute) |
| Spearhead supply under 50% or org under 30% of max | stop the plan (`hoi4_army_plan` execute false), rest |
| Enemy armour seen, own AT low | AT batteries and production; no assaults |
| Pocket found (`hoi4_encirclement_candidates`) | pincers at the same rear province |
| Enemy wings outnumber mine in a region | more fighters on air superiority there |
| Wing out of range of the front | `hoi4_air_rebase` |
| War declared by the enemy | `hoi4_events` / `hoi4_event_choose` |
| Enemy capitulated | occupy, then `hoi4_peace_options` and bid |
| Landing planned | air and naval gates first (section 7), then `hoi4_naval_invasion` |

---------------------------------------------------------------------------

## 14. End matter

### (a) Automatable standing rules

1. Every division belongs to an army with a general; armies of 24 divisions or fewer.
2. Every army has a front toward each enemy it faces; execute only at 90% of the
   planning cap (or after 15 days) with air superiority held.
3. Fighters on air superiority over the front regions; CAS over active battles.
4. Stop plans when the spearhead's supply or org falls below the safe level.
5. Keep reserves behind high-VP hubs; never strip the capital region.
6. Declare war on the first day after the war goal appears and the border checks pass.
7. On capitulation, occupy before the conference and open it the same day.
8. Keep convoys escorted and convoy production non-zero.

### (b) Needs live decisions

- Whether to start a war or invasion at all (readiness, FAIR review).
- Which of two simultaneous threats gets the reserves.
- Choice of pincers and encirclement target.
- Naval invasion go/no-go and landing site.
- Peace bids (take versus puppet), contested-bid decisions.
- Event options with political consequences (faction, ideology, ceding states).
- Switch from attack to defence after a failed offensive.

### (c) Tools

- Land: `hoi4_armies`, `hoi4_army`, `hoi4_supply` (per-division supply, fill,
  strength), `hoi4_front_plan`, `hoi4_encirclement_candidates`, `hoi4_army_front`,
  `hoi4_army_offensive`, `hoi4_army_plan`, `hoi4_army_attack`, `hoi4_army_move`,
  `hoi4_army_create`, `hoi4_army_split`, `hoi4_army_merge`, `hoi4_army_detach`,
  `hoi4_army_assign`, `hoi4_army_assign_plan`, `hoi4_army_group_create`,
  `hoi4_army_commander`, `hoi4_commanders`, `hoi4_army_motorization`,
  `hoi4_division_disband`, `hoi4_deploy_locations`, `hoi4_train`, `hoi4_map_mode`,
  `hoi4_alerts`, `hoi4_wars`, `hoi4_war_stats`.
- Air: `hoi4_air`, `hoi4_air_wings`, `hoi4_air_regions`, `hoi4_air_region_probe`,
  `hoi4_air_mission`, `hoi4_air_hold`, `hoi4_air_rebase`, `hoi4_air_wing_create`,
  `hoi4_air_wing_split`, `hoi4_air_wing_merge`, `hoi4_air_wing_disband`.
- Navy: `hoi4_fleets`, `hoi4_fleet_mission`, `hoi4_fleet_region`,
  `hoi4_fleet_create`, `hoi4_fleet_split`, `hoi4_fleet_merge`, `hoi4_fleet_assign`,
  `hoi4_fleet_reserve`, `hoi4_fleet_balance`, `hoi4_fleet_delete`,
  `hoi4_fleet_admiral`, `hoi4_naval_invasion`, `hoi4_convoys`,
  `hoi4_naval_production_options`, `hoi4_production_add` (hulls and convoys).
- War and peace: `hoi4_war_risk`, `hoi4_war_goal_targets`, `hoi4_diplomacy`,
  `hoi4_diplomacy_act`, `hoi4_events`, `hoi4_event_choose`, `hoi4_peace_status`,
  `hoi4_peace_options`, `hoi4_peace_take`, `hoi4_peace_submit`, `hoi4_peace_exit`,
  `hoi4_occupation`, `hoi4_occupation_law`.

Real gaps (no tool; use the workaround):

- Battle-plan execution mode and army cohesion: the default (balanced) applies.
- Fallback line, area-defence and spearhead orders: use front lines.
- Planning % and entrenchment reads: count days since the order (+2% a day).
- Army HQ deploy: none.
- Sea-region dominance and naval intel read: patrol 50+ days per region before relying on control.
- Air superiority by region: `hoi4_air_region_probe` counts planes on both sides.
- Dockyard repair share and ship refit: none.
- Licence, lend-lease and volunteer dialogs: none.
- Per-state occupation law and garrison template assignment: `hoi4_occupation_law` sets the default law only.
- Operations (collaboration government, coup, steal tech) and decryption: none.
- Equipment stockpile read: use `hoi4_supply` equipment fill and `hoi4_production_shortages`.

Never improvise raw input for a gap.

---------------------------------------------------------------------------

## 15. Tactics: land operations

| ID | Tactic | How it works | When | Mode |
|---|---|---|---|---|
| OP01 | Plan before executing | Planning +2% a day to 30% (15 days), x0.8 with no deployed HQ; +1% attack and breakthrough per 1%; decays 1% a day under a plan and 3% a day under manual orders. Execute at 90% of the cap or after 15 days, at 0% only before the enemy digs in | Every offensive | FAIR |
| OP02 | Staff Office Plan | The ability gives +400% planning speed for 7 days | Short planning windows | FAIR |
| OP03 | Stall fallback replans | At a stall: rest when supply or org is under 50%; else draw the offensive on the next target with two attack directions; a direct move only into an empty adjacent province with a guard behind. Blind moves burn planning and lose flank cover | Stalled armies | FAIR |
| OP04 | Two or more attack directions | The defender fights at -50% in several combats; each extra direction adds half a base width and cancels one fort level (never the last); three directions count as flanking for traits | Every attack | FAIR |
| OP05 | Rivers | Attacking across a small river -30% (speed -25%), large -60% (-50%), the attacker only; take engineers; defend behind large rivers | Axis choice | FAIR |
| OP06 | No offensives in mud, snow or cold | Mud: -40% attack, -50% speed, +70% attrition, trucks x5 attrition; deep snow: -40% attack, -30% breakthrough, -50% speed, -40% supply; extreme cold: -20% attack, -30% breakthrough, -60% entrenchment. The target tile's weather applies; acclimatisation halves it at best | Autumn, winter, rains | FAIR |
| OP07 | Night | Both sides attack at -50%; CAS works at 10%; never judge a battle over one night | Every battle | FAIR |
| OP08 | Rest and resume by share | Rest when org is under 25-30% of max or strength under 60%; resume at 80% org and 80% strength. Org regain doubles below 25% and halves above 80%; each province taken costs 20% of max org; stop after about 3 provinces in one bound | Every offensive | FAIR |
| OP09 | Supply floor | No new offensive below 50% supply; below 35% (attrition starts) stop and pull 25-50% of the divisions out; resume at 70%. Out of supply the attacker loses 25% attack and 65% defence; careful execution needs full supply | Every offensive | FAIR |
| OP10 | Split overstacked armies | Overstacking a province is the usual cause of starvation: when a province is overdrawn on two reads, split the army and move half next door | Strike armies | FAIR |
| OP11 | Advance in bounds | End each push just past a captured hub or port and wait until it connects (captured rail is dead 10 days, 5 on cores; the timer restarts when the path changes; river links work at once); rest org and planning, then plan the next hub | Every deep offensive | FAIR |
| OP12 | Encircle, do not push | An encircled unit fights at -30%, gets no manpower, equipment or fuel even with air supply, and shatters when routed with no retreat; neutral or allied contact cancels the encirclement | Every war | FAIR |
| OP13 | Reduce the pocket | Widen the junction (one tank each way plus infantry on the neck), hold the outer ring two deep, take ports inside the pocket, then attack every pocket province explicitly: the battle plan only clears empty pockets of 2 provinces or fewer | After a pocket closes | FAIR |
| OP14 | Pocket size by strength | Small repeated pockets when forces are equal; large ones only when much stronger; against a large country kill divisions first, against a small one take VPs | Choosing operations | FAIR |
| OP15 | Never leave a tile empty behind the spear | Hold the corridor when a strike leg runs more than 2 provinces; holders on flank tiles within one tile of the path; salients 1-3 tiles wide invite encirclement | Every advance | FAIR |
| OP16 | Withdraw before the pocket closes | Fall back at 50% hostile share around the army or when the neck to the rear is 2 own provinces or fewer (70% is too late); the HQ goes first; pin the unit that would close the neck | Salients | FAIR |
| OP17 | Pin the neighbours | Attack every enemy tile next to the breach with a careful-mode attack so the AI cannot shift reserves; pin with weak high-org infantry | Every breakthrough | FAIR |
| OP18 | Spear and net | Armour breaks and keeps moving toward rail junctions, hubs and the capital; infantry follows and holds | Encirclements | FAIR |
| OP19 | Retake with the reserve | A lost own state is retaken by the reserve or the corridor guard; draw a strike army only when no reserve is within 2 states and the state is on its own corridor | Defence behind an advance | FAIR |
| OP20 | Equipment fill gates the attack | Every missing 1% of equipment costs about 1% attack; no offensive under 90% mean fill; under 80% merge or disband weak divisions into the strike armies | Before every offensive | FAIR |
| OP21 | Keep the tanks moving | After a breach, no rest while org is above 50% of max and supply above 70%: an enemy without time to dig in keeps retreating | Exploitation | FAIR |
| OP22 | Rotate attack groups | Two strike armies on one axis alternate: B attacks while A rests | Dug-in lines, mountains | FAIR |
| OP23 | Read the trend | Judge a battle by the predicted end and the trend, not the colour; stop an attack whose org falls faster than the enemy's over two reads 12 hours apart | Live battles | FAIR |
| OP24 | Stack modifier categories | Modifier categories multiply; plan at least three positive ones per offensive: planning, air support, general, experience, terrain | Attack preparation | FAIR |
| OP25 | Rest after a long move | Strategic redeployment drops org to 10% and is used for paths of 8+ provinces; rest to 80% org before attacking | Moving armies to a front | FAIR |
| OP26 | Air first | Enemy air superiority: -35% defence, -30% speed; an attack waits while the enemy holds the target region and no own fighters are assigned | Every theatre with enemy air | FAIR |
| OP27 | Cohesion | Guard and holding armies rigid (keeps entrenchment), strike armies balanced; immobile halves planning speed | Every front | FAIR |
| OP28 | Execution mode | Careful (needs full supply, refuses forts 5+) when supply is under 70% or forts are on the axis; balanced otherwise; rush only to close a pocket within 2 provinces | Every executed plan | FAIR |
| OP29 | One marshal line | Several army fronts side by side drift and leave gaps; draw long holding fronts for the army group as one line; adding or removing an army from a split line leaves tiles unguarded for weeks | Long borders | FAIR |
| OP30 | Narrow strike sectors | The planner spreads 2-3 divisions per province; give a strike army about 1 province per 2 divisions | Long fronts | FAIR |
| OP31 | Fallback lines | Divisions on a fallback line entrench and counter-attack if the line breaks but get no planning; give the fallback army a front and offensive line on the same position; pre-draw them behind rivers | Defensive wars | FAIR |
| OP32 | Area defence for the rear | An area-defence order weighs VPs, ports, hubs and rail, recaptures lost ground and lets one general hold 72 divisions | Rear, coasts, occupied land | FAIR |
| OP33 | Guards stay dug in | +2% per entrenchment level, lost on any move; give guards their front once and never re-issue it while the border is unchanged | Holding fronts | FAIR |
| OP34 | Poke defence and support attacks | Attack the tile in front and halt before winning: the enemy retreats while you keep planning and entrenchment; a support attack joins a battle without advancing | Defensive chokes | FAIR |
| OP35 | Counter-attack after a failed attack | The attacker's org is lowest right after a repulsed attack; let the AI bleed on rivers and forest, then counter-attack that province; never defend open plains | Defensive wars | FAIR |
| OP36 | Mobile reserve one state behind | Reserves join a battle at about 2% an hour and do not recover org in reserve; keep a mobile reserve behind threatened hubs | Defence | FAIR |
| OP37 | Force Attack and Last Stand | +20% attack and +25% breakthrough (or +20% defence and +25% entrenchment) for 7 days; no organisation damage, so a losing division cannot break and retreat and fights on with +60% strength damage until it dies, every casualty costing 20% more war support; costs per battalion since 1.19, so only on a small army at the decisive tile | Closing a pocket, holding a choke; never on a tile you are clearly losing | FAIR |
| OP38 | Overrun | Entering a province a unit retreats into destroys it; motorised divisions follow the tanks to catch retreats | After a breakthrough | FAIR |
| OP39 | Intel network on the target | Own planning speed rises and enemy divisions lose max planning and up to 5 entrenchment levels in a networked state | Attacking a prepared line (La Resistance) | FAIR |
| OP40 | Hit tanks on the move | A moving defender has no entrenchment while the attacker has planning | Armoured wars on plains | FAIR |
| OP41 | How the AI fights | It splits into small armies, reassigns generals, keeps garrisons on core VPs, throws green divisions in when losing and barely notices encirclements; since 1.19.4 it concentrates its best divisions on one point: watch for massing and move the reserve there | Every war against the AI | FAIR |
| OP42 | Port guards | One small division per port delays a landing; AI invasions are more aggressive since 1.18.3 and aim at high-VP coasts; a 10-wide coast guard on every port in reach of an enemy navy | Every coast at war | FAIR |
| OP43 | Static AA against paratroopers | Static AA cuts a dropped division to 69/58/45/35% strength at AA level 1/2/3/5 | Defending hub provinces | FAIR |
| OP44 | Manual rotation | Pull a defender near 0 org one province back when a fresh division of the same army stands next to it | Long defensive battles | FAIR |
| OP45 | Spread a big army by fallback line | Draw the fallback lines with one division, then deploy the mass of new infantry into that army and let it spread itself along the lines (one click on the leader); fix marsh and key tiles by hand | Defending a long front (USSR 1941) | FAIR |
| OP46 | Marsh and edge stacking | Put only 2-4 infantry divisions in a marsh tile (width 50, attackers at -40%); keep tanks at the marsh edges on the plains tiles behind it, never in the marsh | Pripyat-type fronts | FAIR |
| OP47 | Last stand where it decides | Since last stand costs command power per battalion, hold it for mechanised and infantry on the hubs and cities that decide the front (two mech divisions per key city), and take cheap-last-stand focuses (Japan's Senjinkun code). 2-wide last-stand divisions break the 10-width rule (BAN in FAIR) | Holding key tiles | FAIR |
| OP48 | Undeployed reserve batches | Train about 20 batches of infantry before the war and leave them undeployed; deploy one batch onto a breakthrough the front cannot close | Defensive war start | FAIR |
| OP49 | Special forces on fronts, not plans | Rangers and mountaineers on battle plans across rivers and through many tiles lose equipment fast; give them a front line and attack tile by tile | Rangers in forests, mountaineers inland | FAIR |
| OP50 | Probe before the assault | A cheap preliminary attack strips a strong tile's organisation before the main assault; the Probing Attack ability keeps the prober's dig-in | Strong defended tiles | FAIR |

## 16. Tactics: air

| ID | Tactic | How it works | When | Mode |
|---|---|---|---|---|
| AI01 | Superiority first | Fighters on air superiority over the attack region 1-2 weeks before the offensive with about 1.5x the enemy's fighters; never fight under red air. Germany against an AI Britain: 5 fighter factories from day 1, 15-25 by 1939, never most of industry | Every air war | FAIR |
| AI02 | Read the colours | Under 40% red, 40-60% contested, over 60% ours | Every air war | FAIR |
| AI03 | Do not overstack | Each side engages at most 3x the planes the other side has detected; over about 1000 fighters saturate a small region; cap a region at max(3 x enemy fighters seen, 300) and send the rest to the next region | Large air forces | FAIR |
| AI04 | CAS to the width cap | At most 3 CAS per enemy combat width per battle; the +25% air support needs the count, not quality; CAS damage is small since 1.17, so CAS is for the bonus and superiority. Against AA-heavy enemies (regimental AA gives a division 100-200 air attack) keep CAS at 6% of production or less | CAS builds | FAIR |
| AI05 | Move fighters freely, bombers rarely | A region change cuts efficiency to 90%, regained per day: fighters and CAS 0.888, tactical and naval bombers 0.192, strategic bombers 0.072; move bombers at most once a month; a newly assigned wing starts weak | Every air war | FAIR |
| AI06 | Interception only against bombers | Fly air superiority; switch a home wing to interception only while enemy bombers operate there; never give one wing two missions | Every air war | FAIR |
| AI07 | Central bases, never full | Overcrowding costs 2% per 1% over capacity (200 planes per base level); keep bases at 70% or less; a central base raises efficiency | Every air war | FAIR |
| AI08 | Fighter choice | Light short-range fighters for Europe and defence, heavy fighters for range and islands; range from fuel tanks or a range-focused MIO, and one fuel tank traded for an armour plate when the enemy fields many fighters; never two engines on a small airframe (cost 21 to 35); fighter 3 only with engine 4; 1940 fighter: small airframe 2, engine 3, two or three heavy MG, self-sealing tanks, armour | Plane designs | FAIR |
| AI09 | CAS design by air colour | CAS airframe, engine 3. Uncontested (our) air: three bomb locks and nothing defensive (air defence ~8, ~24 IC), the most damage per IC; contested or enemy air: two bomb locks plus armour plate and defensive guns (air defence ~17, ~29 IC) to lose fewer planes; heavy bomb locks are cheaper. The CAS MIO's ground-attack traits add up to about +58% | Plane designs | FAIR |
| AI10 | Naval bomber design | Naval bomber airframe with the best air torpedo (the air-torpedo tech is most of the damage), floats for detection, engine 3, drop or extra fuel tanks, radar once researched; no armour or self-sealing (only agility protects against ship AA); only one naval armament category counts; medium airframe only for ocean range | Plane designs | FAIR |
| AI11 | Logistics strike when idle | With no own attack in a region for 3 days, CAS and tactical bombers strike trucks, trains and hubs (500 CAS killed 48 trains and 200 trucks in a month); back to CAS when the attack starts | Air-superior attacker | FAIR |
| AI12 | Bomb contested air bases | Tactical bombers on enemy air bases: repairs sit low in the queue, capacity falls and overcrowding spirals | Contested regions | FAIR |
| AI13 | Static AA is not anti-CAS | Static AA hits strategic bombers, port strikes, logistics strikes and air supply, not CAS; -12% bombing damage per level; build it only on factory and air-base states under bombing; use division AA against CAS | Defenders | FAIR |
| AI14 | Strategic bombing only in long wars | Enemy war support falls at most 30% in total; about 1000 bombers are needed to beat repairs; no strategic bombers in a war shorter than a year | Long wars | FAIR |
| AI15 | Aces and wing size | Ace bonuses are tuned for 100-plane wings (up to x2 on smaller ones); train wings in peace toward 300 XP | Fighter wings | FAIR |
| AI16 | Mind the path | Wings are intercepted over every region they cross; base on your side and do not route through enemy-held sky | Bombers, CAS | FAIR |
| AI17 | Rear cover | About 100 fighters over each rear region with the capital or industry against bombing and paradrops | Majors | FAIR |
| AI18 | Ground crews | 20 command power gives a wing +10% efficiency | Big air wars | FAIR |
| AI19 | Day-only missions | Fly CAS and bombers by day only to save fuel; without fuel missions run at 25% | Oil-poor air forces | FAIR |
| AI20 | Naval bombers against a home fleet | Land-based naval bombers with air superiority beat the Royal Navy: 300-600 by mid-1940 in Belgium, the Netherlands and northern France, held back until ready; the damaged Home Fleet goes to repair, which is the window to take the sea | Germany against Britain | FAIR |
| AI21 | Spotting | One 100-plane naval patrol wing per target sea region; the rest on naval strike | Naval air | FAIR |
| AI22 | Port strike | Over the port's land region while the enemy fleet is in port and own superiority is 70%+; Base Strike +40% | GER, JAP, UK, USA | FAIR |
| AI23 | CAS factory for a landing | The cheapest CAS design on 100+ military factories by the landing year, in lines of 10 (PR41); CAS to the width cap over every landing tile with fighters above | Allied landings in France, Axis Barbarossa | FAIR |
| AI24 | Speed wins fighter duels | A wing faster than its opponent gets top speed x 0.025 as a damage bonus (600 km/h = +15%) on top of up to +162% from the speed gap | Pick the fighter design, designer and doctrine that end faster than the enemy's main fighter | FAIR |
| AI25 | Fighters past 3:1 are wasted | Detection needed = our fighters / (3 x enemy fighters): about 33% at equal numbers, 100% at 3:1; past that, extra fighters cannot engage | Move the surplus or add radar instead of stacking | FAIR |
| AI26 | Agility and speed trade | About +10% agility over the enemy is worth +4.5% defence and +10% speed about +6.5% attack; rank fighter designs by attack times defence per IC | Fighter designs | FAIR |
| AI27 | Drop tanks by distance | Fighters over own regions need no drop tank; escorts or patrols over distant regions need them, or mission efficiency falls with range | Fighter designs | FAIR |
| AI28 | Scout planes | A basic scout airframe, buildable on day 1, on recon over the enemy front gives the intel advantage in combat and reveals enemy plans; it is rarely targeted, so it needs no defence | Any land war | FAIR |
| AI29 | Aluminium-free strategic bomber | Basic large airframe (3 aluminium) with the non-strategic-materials module (-3 aluminium) costs no aluminium; three bomb bays and engine II, buildable at the start; keep the airframe and refit engine III later (it frees a fourth bomb slot); electronics techs raise bombing damage. Targets: air bases, forts, ports, factories, never refineries | Allies in the Mediterranean and Pacific, long wars (AI14) | FAIR |
| AI30 | Naval heavy fighters | Basic heavy-fighter airframe with torpedoes or depth charges (range ~1,960 km) on naval patrol hunts submarines and ships far out; with machine guns instead of sealed tanks it is buildable on day 1 | Navies facing submarines or distant fleets | FAIR |

## 17. Tactics: navy and invasions

| ID | Tactic | How it works | When | Mode |
|---|---|---|---|---|
| NA01 | Patrol builds dominance | Only patrol builds dominance; a strike force multiplies it (1000 strike-force dominance gives +60%, -5% per extra region); raiding lowers the enemy's; escort adds none. Dominance grows 2% a day and decays 4%: one patrol task force per region (fewer rotate and never reach invasion level), started 50+ days before an invasion or blockade | Every navy that wants control | FAIR |
| NA02 | Control at 60% | A region is controlled at 60% of the needed dominance; minimum per sea zone 250 ocean, 100 shallow sea or fjord; below 40% naval intel dominance is reduced (half at 10% or less); radar +5%, air base +2%, naval base +1% per level toward the region: radar 2-3 on the route coast 3 months ahead | Invasions, blockades | FAIR |
| NA03 | Strike force with a patrol | A strike force engages only after a patrol of yours spots the enemy and waits in port meanwhile (fuel x0.25); at most 2 regions | Capital fleets | FAIR |
| NA04 | Adjacent fleet regions | With adjacent regions the whole fleet joins one battle; a gap means part of it fights alone | All navies | FAIR |
| NA05 | Screens | Full screening needs 3 screens per capital or carrier and 1 capital per carrier, plus 3-6 spares; partial screening gives only part of the bonus | Surface fleets | FAIR |
| NA06 | No doom stacks | Positioning -25% per +100% ship-count ratio (max -75%); fight with 1-2x the enemy's ships per task force | Surface battles | FAIR |
| NA07 | Torpedo destroyers | Default screen: improved (1940) hull, engine III, cheapest battery, two torpedo IV (about 1,460 IC; basic hull 650-950 IC early); never the 1944 hull (it adds chromium); spam them to hit capitals once the enemy screens are thin; pair 1 light or heavy cruiser per 4 destroyers since capitals now hit small ships more | GER, ITA, minors | FAIR |
| NA08 | ASW destroyers and hunter patrols | Depth charges 15+ and sonar on a 1936 hull (~1,000 IC, buildable early) make a sub hunter; naval heavy fighters (AI30) add air cover; 1 per 12 convoys on raided routes; hunt submarines on patrol (they are revealed), not on escort (they start hidden): 3-5 ASW destroyers with a spotter cruiser and naval bombers | Navies facing submarines | FAIR |
| NA09 | Where submarines raid | Only against a convoy-dependent enemy (UK, Japan, overseas armies); deep ocean (-15% visibility), never shallow seas (+33%), the Channel or the North Sea (land-based air) | Raiders | FAIR |
| NA10 | Submarines work on 1.19.4 | The 1.17.5 detection nerf was reversed in 1.18; measure losses against kills by IC over 30 days and move regions or pause the line when losses exceed kills | Germany, Italy, Japan | FAIR |
| NA11 | Submarine task forces of 8-12 | One task force per raid region (a raider covers 1.5 regions at full efficiency); never mix old and new hulls; never put submarines in a surface task force | Raiders | FAIR |
| NA12 | Raider design | FAIR: basic (1936) hull (Submarine II) with the best torpedoes and the best engine, no AIP or coating; never build the early hull. META only: the improved hull with three tubes (over 60 torpedo attack sinks a 60-HP convoy in one hit), the cruiser submarine, any later hull, anechoic tiles | Germany | FAIR |
| NA13 | Raid where the convoys are | The sea-region info shows the estimated enemy convoys even at 0 intel; move each raider group every 7 days to the in-range region with most convoys and no warships | Raiders | FAIR |
| NA14 | Raider settings | Medium risk; always engage only to kill a named enemy fleet | Raiders | FAIR |
| NA15 | Convoy efficiency | Losses cut efficiency by 1.25x the loss; recovery starts after 7 quiet days at 4% a day; raiding an overseas front starves it | Enemies with overseas fronts | FAIR |
| NA16 | Escort sizing | One escort ship covers 12 convoys, one escort task force 5 regions; escort gains positioning under own dominance | Overseas supply (Italy in East Africa) | FAIR |
| NA17 | Repairs | Repair at the biggest ports (a level-8 base repairs 8 ships at once); raise the dockyard repair share at war (it starts at 25%) | Naval wars | FAIR |
| NA18 | Never sail without fuel | Out of fuel: speed -75%, attack -50%, torpedoes -80%; in a fuel shortage keep only the two key regions on patrol | GER, ITA, JAP | FAIR |
| NA19 | Kill the AI's patrols | The AI sends unescorted patrols and keeps capitals in port; an anti-patrol group (4-6 cruisers or 8 destroyers) on always engage in each wanted region collapses its dominance | GER, ITA, JAP against AI navies | FAIR |
| NA20 | The AI sees invasion orders | Draw invasion orders only when the dominance and air gates hold; with the plan cap above 1, launch 2-3 landings on separate routes at once | Every landing against a major | FAIR |
| NA21 | Invasion capacity by tech | Divisions / plans / prep days: base 4/1/60; transport 6/2/50; landing craft 10/5/40; tank landing craft 15/9/30; marine subdoctrines add more; prep does not depend on size, so fill the cap | Every landing | FAIR |
| NA22 | Clear army orders first | An army with a front or fallback line never starts the invasion (dashed arrow, no divisions assigned): detach the invasion divisions into a fresh army first | Every landing | FAIR |
| NA23 | Gates at launch only | Launch needs sea control (60% of the needed dominance) in every crossed region and 40% naval intel at that moment; once launched the landing goes ahead; later troops cross through the captured port | Every landing | FAIR |
| NA24 | Land on or beside a port | First wave on the port, second wave on the next coastal tiles to widen the battle; plains ports, not urban; several landing tiles so the landing is not itself encircled | Landings | FAIR |
| NA25 | Shore bombardment | Capitals on invasion support give the defender up to -33% | Landings | FAIR |
| NA26 | Escort the landing's convoys | The 1.19.4 AI raids the convoys that feed a landing | After landfall | FAIR |
| NA27 | Route regions | An invasion needs control in every region crossed; the click order changes the path | Sea Lion, Pacific | FAIR |
| NA28 | Carriers only with a fleet | The AI attacks carriers with its whole fleet: 2-4 carriers per strike force with screens and a capital per carrier; past 80 planes in one naval battle each extra plane cuts sorties 0.5%, so about 4 carriers per fleet; most hangars the hull takes, AA and radar, no armour; carriers are the best sea control per IC, battleships are not worth building; the stance applies only in the first 24 hours | UK, USA, Japan | FAIR |
| NA29 | Re-role old ships | Old destroyers become ASW escorts, cruisers spotters, old battleships hold safe waters; delete slow hulls the plan cannot use | Start fleets | FAIR |
| NA30 | Admirals | One admiral per fleet with at most 10 task forces; the submarine fleet gets its own | All navies | FAIR |
| NA31 | Mines | Near useless against the AI | All | FAIR |
| NA32 | Coastal patrols and pride of the fleet | Patrols beside own coasts give up to +10% stability; the pride of the fleet gives +5% war support and -10% for a while if sunk: never risk it | Stability-bound countries | FAIR |
| NA33 | Straits | Surface ships on a strait block land attacks across it | Danish straits, Bosporus | FAIR |
| NA34 | Numbers beat quality against the Royal Navy | About 100k IC of mostly torpedo destroyers and light cruisers (100-150 destroyer-equivalents plus 6-12 cruisers by mid-1940) beats it | GER or ITA against UK | FAIR |
| NA35 | Target options | Main fleet ignores submarines and convoys; ASW groups hunt submarines; check that the setting does not make fleets disengage | All navies | FAIR |
| NA36 | Escorts with detectors | A task force's submarine detection is the average of its ships | Convoy escort | FAIR |
| NA37 | Fleet range | A fleet's range is its shortest-range ship | Blue-water fleets | FAIR |
| NA38 | Shore bombardment geometry | A sea region splits its bombardment over every land tile it touches; a land tile touching two sea regions gets both. Each heavy attack point is worth twice a light one; the total caps at -33%. Pick landing tiles whose sea region touches few tiles, or split the bombarding fleet so each tile's regions carry enough heavy attack | Landings on defended coasts | FAIR |
| NA39 | No fleet battle in bad conditions | Never engage in storms, without own air cover, under enemy air superiority or on low supply: the fleet is destroyed | Every fleet battle | FAIR |
| NA40 | Escort doctrine before a fleet battle | Switch the fighter subdoctrine to the escort branch (Medium-Range Escort: +15% sortie efficiency, carrier fighter agility and speed) and take a sortie-efficiency advisor before a carrier engagement | Carrier navies (Japan, USA) | FAIR |
| NA41 | Simultaneous landings | Launch every landing the same day so the defender's command power and reserves cannot answer all; 2 divisions per tile, backup waves planned, Force Attack or a commander with command power on the first wave | Big amphibious operations | FAIR |
| NA42 | Heavy batteries for bombardment | Refit heavy cruisers' secondary slots and torpedo or plane slots to heavy cruiser batteries and build new heavy-battery cruisers; admiral and high-command bombardment bonuses (naval gunfire support subdoctrine, a Ground Pounder admiral, Naval Liaison generals) stack toward the cap | Before a landing | FAIR |
| NA43 | Merchant-raider refit has a price | The decision costs 25 PP, 20 convoys and 10 navy experience each time and returns an early cruiser after 25 days | A screen source, not free | FAIR |
| NA44 | Five AA on carriers and capitals | Naval bombers prefer ships with under 5 anti-air and weight carriers and capitals heavily; give every carrier and capital at least 5 AA | Ship designs | FAIR |
| NA45 | Crew from the keel | Since 1.19.4 a ship holds its manpower from keel-laying: queue only hulls the manpower pool can crew | Naval production | FAIR |
| NA46 | Peace patrols for XP | In peace a patrolling fleet earns navy experience; patrol only while the fuel stock allows | Navies at peace | FAIR |
| NA47 | Strike after the big battle | Keep obsolete capital ships out of losing fights; after a big battle the AI fleet goes to repair, which is the moment to strike or land | Fleet war | FAIR |
| NA48 | Light cruisers kill screens | Screens shield capitals and carriers; light cruisers with four light batteries (light attack ~29, ~4,900 IC) strip the enemy screens first. Light battery III costs 150 IC per mount against 120 for II for 2 against 1.5 light attack and no extra resource (ship modules carry none in 1.19.4) | Surface fleets | FAIR |
| NA49 | Sponge destroyers | Cheap early-hull destroyers with one light battery and a depth charge (~550 IC) absorb enemy light attack so the cruisers and torpedo destroyers survive | Surface fleets | FAIR |

## 18. Tactics: peace conference

| ID | Tactic | How it works | When | Mode |
|---|---|---|---|---|
| PC01 | Take what you occupy | A state you do not control costs x10, one a faction member controls x0.5; occupied states x0.9 at 30-70% compliance, x0.8 above 70% | Every peace | FAIR |
| PC02 | Bid order | Own cores x0.25, claims x0.9, occupied x0.8; never an allied major's core (x3); grow outward from own land, never snake | Every peace | FAIR |
| PC03 | Contested and uncontested bids | Bid contested cores in round one (full refund on a round-one contest, then 92/84/76%); bid uncontested wants early (their price grows 1.15-1.60x a turn); contest a state at most 1-2 turns, then forfeit | Several winners | FAIR |
| PC04 | Participation | War score comes from land damage, captures and casualties; the capital holder at capitulation gets x1.5 plus 150; give the capital and VP provinces to your own armies | Coalition wars | FAIR |
| PC05 | Cut off the AI ally | In turn 1 bid for what an allied AI major will want next to it (coasts, borders) | An allied AI major present | FAIR |
| PC06 | Puppet lock | Repeated puppet or government-change demands on one loser get cheaper (x0.85 each): take one cheap state and puppet one in turn 1, then puppet the rest | Single winner | FAIR |
| PC07 | Add-ons only on puppets | Resource rights, reparations (50% of civilian factories), demilitarised zone, dismantle: +25% cost each, 5 years; only on puppets you will not develop | Peace | FAIR |
| PC08 | 1.19.4 peace | Minors take part more and spend points; democracies may annex without a matching war goal | 1.19.4 | FAIR |
| PC09 | Fight alone when you can | AI allies take a score share; winners under 5% participation hand theirs on | Short wars | FAIR |
| PC10 | Puppet or annex by what the land gives | Annexation hands over the whole stockpile; puppets keep their industry for themselves until annexed (300 PP) | Every peace | FAIR |
| PC11 | Take the fleet last | Capital ships cost 0.005 points per IC and bring their screens; ships are outside the score split, so take them after the states | After a naval enemy capitulates | FAIR |
| PC12 | Collaboration before the kill | Finish collaboration governments before the enemy capitulates (Japan in China) | Long conquests | FAIR |
| PC13 | Check the participation rules | Game rules can pass faction members' war score to the leader and low participants' score on; read them at setup before planning the peace | Faction wars | FAIR |
