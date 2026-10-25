# Supply, logistics, deployment and occupation

Scope: HOI4 1.19.3 (Operation Postern), singleplayer vs historical AI, Normal, FAIR.
Numbers come from the installed game at
`~/Library/Application Support/Steam/steamapps/common/Hearts of Iron IV`
(abbreviated `$G`). Paths: `$G/common/defines/00_defines.lua` (blocks `NSupply`,
`NResistance`, `NMilitary`, `NAI`), `$G/common/buildings/00_buildings.txt`,
`$G/common/terrain/00_terrain.txt`, `$G/common/modifiers/00_static_modifiers.txt`,
`$G/common/occupation_laws/occupation_laws.txt`, `$G/common/unit_leader/00_traits.txt`,
`$G/common/resistance_activity/resistance_activity.txt`, `$G/common/weather.txt`.
Items marked **[uncertain]** are inference, not a file value. Thresholds marked
**[playbook]** are decision rules chosen here, not engine constants. Exact engine
formulas are compiled code; defines give inputs only.

## 1. How supply flows (read this first)

Supply is three layers. A province's supply reach is the best of them, and a unit is
"supplied" when received supply meets its consumption.

| Layer | What it is | File values |
|---|---|---|
| Local state supply | Each state yields supply from infrastructure, VPs and population | `INFRA_TO_SUPPLY` 0.3 per infra level (max 5 levels = 1.5); damaged infra counts 0.15 per level (`SUPPLY_FROM_DAMAGED_INFRA`); VP bonus 0.2 base +0.05 per VP (rounded) (`VP_TO_SUPPLY_*`); population term 0.18 (own) / 0.2 (occupier) (`AVAILABLE_MANPOWER_STATE_SUPPLY`, `NON_CORE_MANPOWER_STATE_SUPPLY`); all scaled by `SUPPLY_BASE_MULT` 0.2 [uncertain how the terms combine] |
| Supply hubs and capital | Nodes that project supply out along provinces and are fed through railways | Capital strength = 5.0 + 0.3 per civ factory + 0.6 per mil factory + 0.4 per dockyard. Capital flow 5.0, hub flow 2.8, naval base 3.3, floating harbour 2.6 |
| Air supply | Transport planes drop supply into an encircled or cut-off pocket | `AIR_SUPPLY_CONVERSION_SCALE` 0.01, drop lasts 168 h (`AIR_SUPPLY_DROP_EXPIRATION_HOURS`) |

### Reach: how far a node's supply carries
Flow starts at a node's initial value and loses a penalty per province:

| Source | Start flow | First-province penalty | Each extra province |
|---|---|---|---|
| Capital | 5.0 | 0.5 | +1.2 |
| Supply hub | 2.8 | 0.5 | +0.7 |
| Naval base (port) | 3.3 | 0.84 | +1.1 |
| Floating harbour | 2.6 | 0.8 | +0.8 |

Reach >= 1.0 counts as "perfect". Penalties are modified by terrain, rivers, infrastructure and a
distance sigmoid (`SUPPLY_FLOW_*`). Back-of-envelope on flat ground **[uncertain]**:
a plain hub gives >= 1.0 for about 2 provinces; a fully motorised hub about 5-6; the capital about 3
(but it is also fed by rail across the whole country, see below). Practical rule **[playbook]**: a hub
more than 3 provinces behind the frontline is not supplying that line unless motorised.

Modifiers on penalty per province (terrain `supply_flow_penalty_factor`):
urban -0.20 (helps), forest 0.08, hills 0.10, mountain 0.12, desert 0.14, jungle 0.16, marsh 0.16,
plains 0. Max infrastructure cuts drop-off by 30% (`SUPPLY_FLOW_DROP_REDUCTION_AT_MAX_INFRA`);
crossing a river adds 0.20 (`SUPPLY_FLOW_PENALTY_CROSSING_RIVERS`). So low-infra marsh or jungle shrinks
a hub's useful radius to 1-2 provinces.

### Hubs, rails, motorisation
- Supply hub: building, one per province (`level_cap province_max = 1`), `base_cost` 20000, must sit on
  the rail network; it only gets large supply when connected to the capital or a port by rail
  (`RAILWAY_BASE_FLOW` comment). Cost is about 1.9 civ factories (civ factory 10800) per file; check the
  construction queue's own cost number before relying on this.
- Rail throughput into a hub: `10 + 5 x level` = levels 1-5 give 15/20/25/30/35, minus 5 per damaged
  railway, floor 5. A route is capped by its weakest link. Hub capacity also rises 0.34 per rail level
  of its bottleneck (`NODE_FLOW_BONUS_PER_RAIL_LEVEL`).
- Rivers act as level-1 rail between nodes (`RIVER_RAILWAY_LEVEL`).
- Motorisation: spends trucks from stockpile to extend a hub up to +2.2 flow (60 trucks for full
  effect, `SUPPLY_HUB_FULL_MOTORIZATION_*`), with diminishing returns per level (decay 1.6). Army
  card has a Motorization Priority cycle (`ARMY_MOTORIZATION_PRIORITY` loc). Use it when the
  frontline outruns hubs (fast advances).
- Trains carry hub supply from the capital: 1.0 supply per train, need grows with rail distance
  (`NUM_RAILWAYS_TRAIN_FACTOR` 0.03 per rail province). Zero trains in stock applies a 0.5 factor
  (`MIN_TRAIN_SUPPLY_FACTOR`); total need of 2 or less is free (`MIN_TRAIN_REQUIREMENT`). Train
  equipment costs 70 IC (basic), 50 IC (1940 cheap), 105 IC (armoured). Starting buffers: 1.5 trucks
  and 1.0 trains per unit of need (`DEFAULT_STARTING_*_RATIO`).
- Captured railways are unusable for 10 days (5 for your own cores, 0 in civil war)
  (`RAILWAY_CONVERSION_COOLDOWN*`). Expect a supply gap after every deep breakthrough; plan trucks
  and a new hub, do not just push on.
- Moving the supply capital: needs a hub in the target province, surrender progress above 0.15,
  30 day cooldown, supply starts after 7 days, full after 21 (`COOLDOWN_DAYS_AFTER_MOVING_SUPPLY_CAPITAL`).
  Only for a collapsing country.

### Ports and naval supply
- Port throughput: 3 supply per naval base level (`SUPPLY_PORT_LEVEL_THROUGHPUT`); naval node flow
  5 + 3 per level (`NAVAL_BASE_FLOW`, `NAVAL_FLOW_PER_LEVEL`).
- Overseas supply uses convoys: 0.25 convoy per supply point (`SUPPLY_CONVOY_FACTOR`), scaled by range.
  Minimum naval delivery efficiency 0.1. Escorted convoy lanes are required; raiders cutting them
  cut the front.
- Floating harbour (naval invasion support): 15 supply, 21 day life, decays faster under enemy air or
  naval superiority (`FLOATING_HARBOR_*`).
- Naval supply hub (ships, not army): needs naval base level 8 (loc `naval_supply_hub_desc`).
- Rule: after every landing, capture the nearest port with level >= 2 before anything else.

### Terrain, weather, infrastructure
| Condition | Supply effect | Other effect |
|---|---|---|
| Snow / blizzard / ground snow medium / high | -10 / -25 / -20 / -40% supply | speed -30% / -70% on ground snow; attack -35% / -75% |
| Mud | -15% | speed -50%, attrition 0.7, truck attrition x5 |
| Flooded | -40% | speed -50%, truck attrition x3 |
| Extreme cold | -15% | winter attrition 0.4, dig-in -60%, attack -20% |
| Very cold | none | winter attrition 0.25, dig-in -30% |
| Sandstorm | -15% | truck attrition x3 |
| Very hot / extreme hot | -5% / none | heat attrition 0.1 / 0.2; extreme hot supply use +50% |

Terrain attrition: marsh 0.35, mountain 0.30, jungle 0.20, desert 0.15. Truck attrition factor:
marsh 4, jungle 3, mountain 1, desert 0.5, forest/hills 0.2. Infrastructure cuts mud growth by up to 80%
(`INFRASTRUCTURE_MUD_EFFECT`) and speed falls 5% per missing infra level
(`INFRASTRUCTURE_MOVEMENT_SPEED_IMPACT`). Weather temperature thresholds: extreme cold below -12 C, very
cold -12 to -4 C; mountains run 7 C colder, hills 3 C (`weather.txt`). Winter specialist halves
winter attrition.

## 2. What undersupply does (defines)

| Effect | Value | Source define |
|---|---|---|
| Free grace before penalties | 36 h on Normal for the player (default 72 h); 24 h hard | `diff_normal_player` in static_modifiers; `SUPPLY_GRACE` **[uncertain which applies]** |
| Max organisation at zero supply | x0.35 | `SUPPLY_ORG_MAX_CAP` |
| Attrition begins below | 35% supply | `SUPPLY_THRESHOLD_FOR_ARMY_ATTRITION` |
| Max supply attrition | 0.20, ramping over 30 days | `OUT_OF_SUPPLY_ATTRITION`, `MAX_OUT_OF_SUPPLY_DAYS` |
| Speed | up to -80% | `OUT_OF_SUPPLY_SPEED` |
| Organisation regain | up to -20% | `OUT_OF_SUPPLY_MORALE` |
| Attrition damage | org 0.08 per tick; equipment loss chance 0.005 scaled by attrition and reliability; x1.2 while moving | `ATTRITION_*` |
| Combat, attacker out of supply | attack -25%, defence -65% | `COMBAT_SUPPLY_LACK_ATTACKER_*` |
| Combat, defender out of supply | attack -35%, defence -15% | `COMBAT_SUPPLY_LACK_DEFENDER_*` |
| Outside own cores | speed -50% (-25% air) | `NON_CORE_SUPPLY_SPEED` |
| Training units | +0.05 attrition | `TRAINING_ATTRITION` |
| Game alerts | 50% low, 20% very low | `ALERT_*_SUPPLY_LEVEL` |

Reading it: below 35% the army bleeds equipment and org, and attacking out of supply costs about
-25% attack; being the attacker out of supply is far worse for defence (-65%). Do not start or continue
an offensive from a province whose supply is falling.

Consumption: per battalion about 0.05-0.06 (infantry), 0.16-0.35 (tanks and heavy armour), 0.1-0.15 anti-air,
so armoured spearheads use 3-5 times the supply of infantry **[approximate, from `common/units`]**.
Logistics wizard on the field marshal cuts consumption 15%.

## 3. Detecting low supply from own telemetry, and what to do

Needed data (see section 9 for what exists): per division, supply ratio received; per province or state,
supply value and source (state/hub/capital/air); hub list with province, damage and motorisation; rail
level per link; truck/train stock vs need. Until those reads exist, use proxies:

| Proxy (own data) | Reading | Meaning |
|---|---|---|
| Division org not rising for 2+ reads, not in combat, not moving | Plateau below max | Cap of 35% max means severe shortage |
| Division strength falling with no combat | Slow drop | Attrition (supply, weather, terrain) |
| Top-bar supply tooltip (OCR) | Used vs available | National pool, not per-province |
| Supply map mode screenshot | Colour per province | Visual only |
| Army speed far below normal, many provinces without gains | Slow | Supply plus weather |

Rule table **[playbook]**:

| Province supply | Action |
|---|---|
| >= 100% | Normal |
| 70-99% | Hold offensives; queue hub or rail work; reduce army size in that province |
| < 70% | Stop attacking; shift 25-50% of divisions to better-supplied provinces or defensive stance |
| < 50% | Emergency: pull the army back to the nearest >= 70% province; motorise or build hub; rest to recover org |
| < 35% | Attrition active; withdraw anything not holding a critical line |
| 20% or less | Collapse; withdraw or air-supply if encircled |

Moving out: send to a province in the same state with a hub, then a neighbouring state with infra >= 3
and rail connection. Divisions per supplied province: aim for <= 8-10 in low-infra provinces
**[playbook]**. Overstacking in a province consumes its supply capacity; spread divisions along the
front rather than stacking.

## 4. Where to train and deploy

- Train near the capital or major rail hub: reinforcements travel from the capital by rail and trucks
  (`REINFORCEMENT_*`, `RAILWAY_DISTANCE_FACTOR_FOR_REINFORCEMENT_SPEED` 0.3 per rail unit; max supply
  path 15). Divisions deployed straight to a well-supplied border state skip a long march.
- Never deploy to a state with infra < 2 unless it has a hub; check `hoi4_state_slots` gives only
  slots, not supply, so prefer states with a known factory or VP.
- The AI itself will not train when supply ratio < 0.30-0.40 (`NAI.START/STOP_TRAINING_SUPPLY_LEVEL`);
  follow the same gate for the player.
- Training: deployment training adds 1 per day (`BASE_DEPLOYMENT_TRAINING`); deploy at least 20%
  trained when time is critical (the `early` option), otherwise let them reach their level cap
  (`TRAINING_MAX_LEVEL` 2, deploy-training cap 1).
- Training lines with no deployment location never deploy. Always call `hoi4_deploy_locations`.
- Keep the manpower and equipment stockpile non-zero for reinforcement; empty stockpile starves
  reinforcement even in good supply.

## 5. Infrastructure vs railway vs hub: construction priorities

| Building | Cost (file) | Effect | When |
|---|---|---|---|
| Infrastructure level (1-5) | 6000 per level | +0.3 supply, +15% resources per level, fewer mud, speed, cheaper shared builds | In war theatre states and any state you will fight over. 3+ before the offensive, 4-5 in industrial cores |
| Railway level (1-5) | 170 + 130 per level (path based) | Throughput 15-35; connects hubs to capital | Upgrade the one trunk route to each planned front to level 3-4. The weakest link sets throughput |
| Supply hub | 20000, one per province | New supply node | Place within 1-3 provinces of the planned frontline on an existing railway, in a province where the enemy cannot reach it quickly |
| Port/naval base | per file | +3 throughput per level | Overseas fronts |
| Bunker/coastal fort | 500 + 500 per level | Defence, not supply | Defence lines |

AI construction priorities (shows what it values): railway 4.0, supply buildings 1.1, infrastructure 0.2
(`CONSTRUCTION_PRIO_*`). Infrastructure also gives +15% resources per level and a +20% resource
bonus when the state is rail-linked to the capital (`INFRASTRUCTURE_RESOURCE_BONUS`,
`SUPPLY_ROUTE_RESOURCE_BONUS`), so it pays back economically.

Dated checklist **[playbook]** (adapt to nation; do not starve factories in 1936-37):
1. 1936 to mid 1937: infrastructure only where it unlocks factory slots or on a planned corridor.
2. Late 1937 to mid 1938: rail to the planned first-war border (level 3), one hub 1-2 provinces behind it.
3. Early 1939: train and truck production to cover the planned army (trucks stock >= need x1.5); verify
   hub is complete.
4. Month before the war: motorise hubs nearest the frontline; keep construction queue short and
   finishing ones in front.
5. During the war: after each major advance, queue a hub in the newly held rail line, then the rail
   upgrade; expect 10 day conversion cooldown after capture.
6. Before winter (September for the Eastern Front): raise infra and hubs in held territory; plan
   pullback lines if the front outruns supply.

## 6. Trucks, trains, attrition

- Trucks (`supply_truck = yes`, motorized equipment) fill the gap between a hub and the army; they suffer
  attrition (`TRUCK_ATTRITION` 0.003 x 0.65) scaled by terrain (mud x5, marsh x4, jungle x3). Mechanised and
  motorised divisions use the same truck pool for movement; keep stock above need.
- Wanted buffers: AI keeps trucks at 1.5x use and trains at 1.2x use
  (`DEFAULT_SUPPLY_TRUCK_BUFFER_RATIO`, `DEFAULT_SUPPLY_TRAIN_NEED_FACTOR`). Use the same as player minimum.
- Enemy logistics strikes damage trucks and trains (`AIR_WING_ATTACK_LOGISTICS_*`), so keep air
  superiority above the rail line and hubs; the AI bombs hubs with about 3 planes each
  (`STR_BOMB_PLANES_PER_SUPPLY_HUB`).

## 7. Garrison and occupation

Resistance and compliance per state (`NResistance`):
- New state: resistance 1%, compliance 0%. Resistance target base 35%, modified by stability (+0.2
  per % below 100), compliance (-0.5 per %), peace (-10), claim (-5), VPs in state (+5/+10/+20 by
  size), capitulated enemy (+10), low population (-20 below 10000, -50 below 1000).
- Resistance above 25% is flagged dangerous (`RESISTANCE_IMPORTANT_LEVEL`). Activity chance at max
  resistance 31% per tick; it sabotages infrastructure (damage 0.5-1.0), factories, resources
  and other buildings (`resistance_activity.txt`). Resistance spreads to neighbours at half strength.
- Suppression needed: 0.75 points per 1% resistance, clamped 10-50%.
- Garrison efficiency drops with manpower shortage squared and equipment shortage cubed
  (`GARRISON_STR_POW_*`). Under-garrisoned states use the missing-garrison penalty (+10% resistance
  target, -0.045 compliance gain).

Occupation law choice (all for a non-democratic player; no_garrison and autonomous are democratic-only):

| Law | Resistance target | Compliance gain | Garrison need | Notes |
|---|---|---|---|---|
| Military governor (default) | -35% | -0.045 | normal | +10% resources, +8% manpower. Safe default |
| Martial law | -60% | -0.055 | normal | Strongest suppression without extra garrison need. Use in hot spots and on supply routes |
| Forced labour | -40% | -0.08 | +15% | +40% resources, +30% damage to garrison |
| Harsh quotas | -40% | -0.08 | +15% | +25% factories, +50% damage to garrison |
| Brutally oppressive (fascist) | -75% | -0.11 | +25% | Strongest suppression, garrison takes +100% damage; no compliance |
| Secret police oversight | -30% | -0.04 | -15% | Cheap garrison, mild gains |
| Local police | -15% | -0.025 | -35% | Weak; only for already quiet states |

Choice rule **[playbook]**: states on your supply route, or that your armies must cross: martial law
(or brutally oppressive if fascist) until resistance < 10%, then switch to military governor.
Quiet rear states with factories you want: forced labour or harsh quotas only if resistance stays below
25% for 60 days. Resistance rising despite law = add garrison divisions or switch up one tier.
Some laws are conditional on government or focus (`visible` blocks): verify what the law panel lists.

Garrison strength: 1 garrison division (cheap template, high suppression) per VP-rich state; the
`GARRISON_ORDER_ARMY_CAP_FACTOR` of 3 lets an army on garrison orders exceed the usual cap. Garrison
configuration can guard supply hubs and railway junctions (loc `GARRISON_CONF_RAIL`); do that first.

Why partisans wreck supply: resistance sabotages infrastructure and railways; damaged infrastructure
supports only 0.15 of a level in supply; each damaged railway cuts flow by 5; and a captured rail needs
the cooldown. Result: hubs behind the line starve. **[uncertain]** The files show sabotage and garrison
damage but no automatic flip of province control to partisans in vanilla, so "partisan control" here
means sabotage plus lost garrison, not enemy-held provinces.

## 8. Command structure, fronts, AI behaviour

Structure (`NMilitary`):
- Field marshal: up to 24 divisions in one army, or an army group of up to 5 armies. Corps commander:
  up to 24 divisions; cannot command army groups. Keep every army <= 24 divisions (the plan runner
  splits armies above 24, using `hoi4_army_split`).
- Army group of 2-5 armies per front; one field marshal each; promote cost 40 command power
  (`PROMOTE_LEADER_CP_COST`). Command power cap 80, +0.4 per day.
- Deployed HQ (new in this version): a leader can be deployed through an Army HQ template. Planning cap
  and speed scale 1.0 / 0.95 / 0.9 / 0.85 / 0.8 by HQ distance (0 to 4 provinces behind the line) and
  0.8 with no HQ. Deploy and withdraw take 1-3 days. The AI deploys HQs only for armies of 10+ divisions,
  at most 5. No tool for HQ deployment exists in this repository.
- Planning bonus: +0.02 per day under an order, -0.01 decay, -0.03 for every manual order, base
  max 0.3 (techs add). Do not reissue front orders unnecessarily.

Traits worth picking (`00_traits.txt`):
| Role | Trait | Effect |
|---|---|---|
| Field marshal | logistics_wizard | -15% supply consumption, extra supplies ability (parent organizer) |
| Field marshal | defensive_doctrine | +30% max dig-in |
| Field marshal | offensive_doctrine | -30% org loss when moving, +1 attack skill |
| Corps commander | organizer | +10% planning speed |
| Corps commander | winter_specialist | -50% winter attrition |
| Corps commander | panzer_leader | armour +16% attack, +5% speed |
| Personality | brilliant_strategist | +1 attack and planning, avoid trait_cautious (-20% planning speed) |
Choose: logistics_wizard on the main offensive FM, defensive_doctrine on the static front, winter_specialist
for northern or Eastern Front armies, panzer_leader for armour corps.

Front-line orders: use front lines along the border plus offensive lines for targets; one army per
front segment. Reserve vs front: keep 20-25% of divisions in reserve on a defensive front **[playbook]**;
rotate divisions below 40% org or 50% strength to a supplied rear province for 5-10 days and reinforce
before returning. Reinforcement priority has three tiers (reserve, normal, elite): set elite
templates for frontline.

Fortification: defensive lines on rivers, hills and forests with bunkers (500 + 500 per level) give
more than open plains; terrain AI-importance: mountain 10, marsh 8, desert 8, jungle 6, forest 5. Fort
penalty base -15% when attacking over forts.

AI behaviour (from `NAI`): fronts allocate 1.0 division per available supply point and keep >= 1
division per province; it targets supply hubs for defence (importance 4) and bombs enemy hubs;
fixes bottlenecks weekly if saturation > 85%; motorises when supply < 80% (checked every 52 h); reinforces
fronts by theatre priority. Expect it to counter-attack where your hub or railway is thin.

Air supply: use only for pockets you intend to relieve within a week; aircraft supply scale is small
(0.01 per plane) so pockets of more than a few divisions cannot hold. **[uncertain]** transport plane
payload is not in the defines.

## 9. Per-theatre habits

| Theatre | Supply facts | Habits |
|---|---|---|
| Poland 1939 | Dense infra and rail; Polish roads strong, Carpathians in south | Two army groups, short supply line from Silesia and East Prussia. Capture rail junctions, build one hub behind each; stop at the river lines for the 10 day rail cooldown |
| France 1940 | Ardennes forest and hills, low infra; Low Countries high infra; Maginot | Armour with motorised support through the Ardennes only with hubs at the border; capture ports; infantry follows by rail; accept a gap while captured rail converts |
| USSR 1941+ | Soviet infra 1-2, few rails, non-core -50% speed, mud autumn, extreme cold from November | Build hubs on every captured trunk line; halt at hub range; winter specialist, pre-stocked trucks; do not outrun the railhead by more than 2-3 provinces |
| China | Low infra, jungle and mountain south and west, rivers; Japan supplied by sea | As China: defend cities (urban improves supply flow), use mountains, keep armies near rail towns, Japanese push suffers attrition. As Japan: capture ports, keep convoys escorted |

## 10. Rules and requirements

### (a) Automatable standing rules
1. Any army above 24 divisions: split (`hoi4_army_split`); any army of >= 8 divisions without a
   general: assign a general with `hoi4_army_commander` (already in the plan runner).
2. Unassigned divisions in a state: `hoi4_army_create` or assign; verify result.
3. Training lines without a location: `hoi4_deploy_locations` to a state in the home hub chain.
4. Weekly: list construction queue; if no supply-related item (railway, hub, infrastructure) is in
   the top three while at war, queue one for the front state.
5. At war: every 24 h compare division org/strength to the previous read; flag stagnant or falling
   divisions not in combat as suspected undersupply (proxy table, section 3).
6. When a new state is occupied: check its occupation law; set martial law on route states, military
   governor elsewhere.
7. Before December in the north and Eastern Front: confirm winter specialist generals are assigned.
8. Keep truck and train stock >= 1.5x and 1.2x of wanted levels (production line check).

### (b) Needs live decisions
- Which front to attack and when, based on hub reach and rail cooldown.
- Whether to stop an offensive at supply < 70%, or pause for a hub.
- Which general gets logistics_wizard or defensive_doctrine; who to promote.
- Occupation law changes in states with active resistance (balance resources vs revolt).
- Retreat versus air supply for an encircled pocket.
- Whether to deploy HQs (no tool), motorise a hub, or reroute a rail.
- Naval invasion: port choice, floating harbour, convoy escorts.

### (c) Tools: what exists and what is missing

Existing (all listed in `native/src/launch/macos.rs` and used by `native/src/plan_run.rs`):
`hoi4_armies` (roster: unassigned_by_state, per army division_count, commander, main_state,
battle_plan_orders, plan_executing), `hoi4_army` (Rust: bounded own divisions with province IDs and raw
strength/organisation only), `hoi4_army_move`, `hoi4_army_front`, `hoi4_army_create`,
`hoi4_army_group_create`, `hoi4_army_split`, `hoi4_army_commander` (general or field marshal),
`hoi4_states` (owner, controller, position per state), `hoi4_state_slots` (building slots only),
`hoi4_deploy_locations`, `hoi4_construction_queue` (building and railway entries, no path).
Legacy Python only: `hoi4_hover` target `supply` (top-bar tooltip OCR), `hoi4_map` mode `supply`
(screenshot, no numbers), weekly state scan with `infra` (`hoi4_live/telemetry/modgen.py`,
`parser.py`; mod-based, check it is allowed under the campaign telemetry rules).
`hoi4_live/memory/reader.py` lists "supply" as an unverified Windows field, and
`native/src/army.rs` has no supply field at all.

Missing reads (none are exposed by `native/src`, and `grep supply` in `native/src` finds no army
or state supply read):
1. Per-division supply ratio (and max organisation, to make the org proxy exact).
2. Per-province and per-state supply received/needed and its source.
3. Supply hub map: province, owner, damage, motorisation, connected or cut.
4. Railway level and damage per link; route bottleneck to capital.
5. Per-state infrastructure level and damage (only the legacy weekly scan).
6. Truck and train stock versus need; reinforcement rate.
7. Per-state resistance, compliance, active occupation law, garrison strength.
8. Division weather and attrition state; per-province temperature or mud.
9. Army HQ deployment state and tool; no action to set occupation law or motorisation priority.
10. Per-army supply summary (min and average of its divisions).

Without 1-4 the agent cannot follow the section 3 table directly; use the proxies, screenshots of the
supply map mode, and conservative stops.
