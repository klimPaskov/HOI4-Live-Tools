# Supply, logistics, deployment and occupation

Scope: singleplayer against the historical AI, FAIR. The exact supply formulas are compiled code; the
numbers below are their inputs.

## 1. How supply flows (read this first)

Supply is three layers. A province's supply reach is the best of them, and a unit is
"supplied" when received supply meets its consumption.

| Layer | What it is | Values |
|---|---|---|
| Local state supply | Each state yields supply from infrastructure, VPs and population | 0.3 per infrastructure level (max 5 levels = 1.5); damaged infrastructure counts 0.15 per level; VP bonus 0.2 base +0.05 per VP (rounded); population term 0.18 (own) / 0.2 (occupier); all scaled by 0.2 |
| Supply hubs and capital | Nodes that project supply out along provinces and are fed through railways | Capital strength = 5.0 + 0.3 per civilian factory + 0.6 per military factory + 0.4 per dockyard. Capital flow 5.0, hub flow 2.8, naval base 3.3, floating harbour 2.6 |
| Air supply | Transport planes drop supply into an encircled or cut-off pocket | 0.01 supply per plane; a drop lasts 168 h |

### Reach: how far a node's supply carries
Flow starts at a node's initial value and loses a penalty per province:

| Node | Start flow | First-province penalty | Each extra province |
|---|---|---|---|
| Capital | 5.0 | 0.5 | +1.2 |
| Supply hub | 2.8 | 0.5 | +0.7 |
| Naval base (port) | 3.3 | 0.84 | +1.1 |
| Floating harbour | 2.6 | 0.8 | +0.8 |

A port starts stronger than a hub but its reach falls off faster inland.

Reach >= 1.0 counts as "perfect". Penalties are modified by terrain, rivers, infrastructure and
distance. On flat ground a plain hub gives >= 1.0 for about 2 provinces, a fully motorised hub about
5-6, the capital about 3 (it is also fed by rail across the whole country). A hub more than 3 provinces
behind the front does not supply that line unless motorised.

Penalty per province by terrain: urban -0.20 (helps), forest 0.08, hills 0.10, mountain 0.12,
desert 0.14, jungle 0.16, marsh 0.16, plains 0. Maximum infrastructure cuts the drop-off by 30%;
crossing a river adds 0.20. Low-infrastructure marsh or jungle shrinks a hub's useful radius to 1-2
provinces.

### Hubs, rails, motorisation
- Supply hub: one per province, 20000 points (about 1.9 civilian factories at 10800), must sit on the
  rail network; it gets large supply only when connected to the capital or a port by rail.
- Rail throughput into a hub: 10 + 5 x level = 15/20/25/30/35 for levels 1-5, minus 5 per damaged
  railway, floor 5. A route is capped by its weakest link. Hub capacity also rises 0.34 per rail level
  of its bottleneck.
- Rivers act as level-1 rail between nodes.
- Motorisation: trucks from the stockpile extend a hub's reach. Full motorisation of one hub uses 60
  trucks for +2.2 flow, diminishing per level. Set it per army (`hoi4_army_motorization`). Use it when
  the front outruns the hubs.
- Trains carry hub supply from the capital: 1.0 supply per train; need grows 3% per rail province from
  the capital. Zero trains in stock halves supply; a total need of 2 or less is free. Train equipment
  costs 70 IC (basic), 50 IC (1940 cheap), 105 IC (armoured). Starting buffers: 1.5 trucks and 1.0
  trains per unit of need.
- Captured railways are unusable for 10 days (5 on own cores, 0 in a civil war). Expect a supply gap
  after every deep breakthrough; plan trucks and the next hub, do not just push on.
- Moving the supply capital: needs a hub in the target province and surrender progress above 15%;
  30-day cooldown; supply starts after 7 days, full after 21. Only for a collapsing country.

### Ports and naval supply
- Port throughput: 3 supply per naval base level; naval node flow 5 + 3 per level.
- Overseas supply uses convoys: 0.25 convoy per supply point, scaled by range; naval delivery
  efficiency never falls below 0.1. Escorted convoy lanes are required; raiders cutting them cut the
  front.
- Floating harbour (naval invasion support): 15 supply, 21 days of life, decays faster under enemy air
  or naval superiority.
- Naval supply hub (for ships, not the army): needs naval base level 8.
- After every landing, capture the nearest port of level 2 or more before anything else.

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
marsh 4, jungle 3, mountain 1, desert 0.5, forest and hills 0.2. Infrastructure cuts mud growth by up
to 80%; speed falls 5% per missing infrastructure level. Extreme cold is below -12 C, very cold -12 to
-4 C; mountains run 7 C colder, hills 3 C. A winter specialist halves winter attrition.

## 2. What undersupply does

| Effect | Value |
|---|---|
| Grace before penalties | 72 h |
| Max organisation at zero supply | x0.35 |
| Attrition begins below | 35% supply |
| Max supply attrition | 0.20, ramping over 30 days |
| Speed | up to -80% |
| Organisation regain | up to -20% |
| Attrition damage | org 0.08 per tick; equipment loss chance 0.005 scaled by attrition and reliability; x1.2 while moving |
| Combat, attacker out of supply | attack -25%, defence -65% |
| Combat, defender out of supply | attack -35%, defence -15% |
| Outside own cores | supply moves 50% slower (air supply 25%) |
| Training units | +0.05 attrition |
| Game alerts | 50% low, 20% very low |

Below 35% the army bleeds equipment and org; the attacker out of supply loses 25% attack and 65%
defence. Do not start or continue an offensive from a province whose supply is falling.

Consumption per battalion: about 0.05-0.06 (infantry), 0.16-0.35 (tanks and heavy armour), 0.1-0.15
(anti-air), so armoured spearheads use 3-5 times the supply of infantry. A Logistics Wizard field
marshal cuts consumption 15%.

## 3. Reading supply, and what to do

`hoi4_supply` reads it without pausing: per division `supply_percent`, `supply_band` and need and
received per day; per province supply, demand, headroom (negative = overdrawn), node flows, hub,
railway and naval base level; per state local supply and infrastructure; own `supply_nodes`; and the
lists `below_70`, `below_50` and `overdrawn_provinces`. Not read: hub and rail throughput, trucks and
trains, stored buffer, attrition. Proxies for those:

| Proxy | Meaning |
|---|---|
| Division strength falling with no combat | Attrition (supply, weather, terrain) |
| Organisation flat below max, not fighting or moving | Supply cap (zero supply caps max org at 35%) |
| `supply` map mode (`hoi4_map_mode`) | Route bottlenecks, visual only |
| Army far slower than normal | Supply plus weather |

| Division or province supply | Action |
|---|---|
| 70% or more | Normal; an offensive stopped for supply resumes |
| 50-69% | An offensive may go on; one stopped for supply waits for 70%; queue rail, port or motorisation and spread the stack |
| under 50% | No new offensive; stop executing in that sector; fix supply (motorise, rail level, port) |
| under 35% | Attrition starts: stop attacking and pull 25-50% of the divisions out to provinces with headroom |
| 20% or less | Collapse: withdraw, or air supply if encircled and relief comes within a week |

Moving out: send divisions to a province in the same state with a hub, then to a neighbouring state
with infrastructure 3+ and a rail link. Keep 8-10 divisions or fewer per low-infrastructure province.
Overstacking eats the province's supply: spread divisions along the front; an army whose province is
overdrawn on two reads splits and moves half next door.

## 4. Where to train and deploy

- Train near the capital or a major rail hub: reinforcements travel from the capital by rail and truck
  (rail distance counts 0.3 per rail unit; maximum supply path 15). Divisions deployed straight to a well-supplied border state skip a long
  march.
- Never deploy to a state with infrastructure under 2 unless it has a hub. `hoi4_supply` `states`
  gives local supply and infrastructure where own divisions stand; `hoi4_state_slots` gives slots only.
- The AI does not train while its supply ratio is under 0.30-0.40; use the same gate.
- Deployment training adds 1 a day; deploy at 20% (the `early` option) only when time is critical,
  otherwise let divisions reach their training cap.
- Training lines with no deployment location never deploy: set one with `hoi4_train`
  `deploy_state_id` or `hoi4_deploy_locations`.
- Keep the manpower and equipment stockpile above zero: an empty stockpile starves reinforcement even
  in good supply.

## 5. Infrastructure vs railway vs hub: construction priorities

| Building | Cost | Effect | When |
|---|---|---|---|
| Infrastructure level (1-5) | 6000 per level | +0.3 supply, +15% resources per level, less mud, speed, cheaper shared builds | In war theatre states and any state you will fight over. 3+ before the offensive, 4-5 in industrial cores |
| Railway level (1-5) | 170 + 130 per level, per province of the path | Throughput 15-35; connects hubs to capital | Upgrade the one trunk route to each planned front to level 3-4. The weakest link sets throughput |
| Supply hub | 20000, one per province | New supply node | Last resort: only after a cost and time check; within 1-3 provinces of the planned front on an existing railway, in a province the enemy cannot reach quickly |
| Port/naval base | 5000 + 1000 per level, coastal provinces only | +3 throughput per level; start flow 3.3 | Where no supply reaches a shore near the front; overseas fronts |
| Bunker/coastal fort | 500 + 500 per level | Defence, not supply | Defence lines |

Build time with 15 civilian factories (5 points per factory per day, at most 15 factories per project,
x (1 + 0.2 x state infrastructure)):

| Building | Infrastructure 0 | Infrastructure 2 |
|---|---|---|
| Railway level 1 | 3 days | 2 days |
| Port, first level | 67 days | 48 days |
| Supply hub | 268 days | 191 days |

Civilian Economy (-30% construction) stretches the hub to about 270-380 days. A short war is over
before a day-1 hub finishes.

**Supply fix order** (each construction step states its cost in points and the finish date from
`calc.py build-order`):
1. Motorisation and trucks (section 6): no construction.
2. Railway levels on the target route from existing or captured hubs and ports, moved to the top of
   the queue (`hoi4_construction_order`): new projects start at the bottom.
3. A new port, or a port level, on a coastal province near the front where no supply reaches the
   shore: a quarter of a hub's cost, a higher start flow, weeks to build. Conditions: the province is
   coastal and owned or controlled; convoys cover 0.25 per supply point; the sea route is not blocked
   by an enemy navy or air force; reach falls off faster inland than a hub's.
4. A supply hub only when the calculation shows it finishes before it is needed and no port or railway
   can serve the same provinces.

The AI weighs railway 4.0, supply buildings 1.1, infrastructure 0.2. Infrastructure also gives +15%
resources per level and +20% when the state is rail-linked to the capital, so it pays back
economically.

Dated checklist (adapt to the nation; do not starve factories in 1936-37):
1. 1936 to mid 1937: infrastructure only where it unlocks factory slots or on a planned corridor.
2. Late 1937 to mid 1938: rail to the planned first-war border (level 3); one hub 1-2 provinces behind it
   only when `calc.py build-order` finishes it before the war and no port or railway serves those provinces.
3. Early 1939: train and truck production to cover the planned army (truck stock 1.5x need); check the
   hub is complete.
4. Month before the war: motorise hubs nearest the front; keep the construction queue short with
   finishing items on top.
5. During the war: after each major advance, extend railway levels from the captured hub or port, add a
   port where the coast lies near the new front, motorise; a new hub only after the cost and time check
   (a wartime hub rarely finishes in time). Expect the 10-day conversion cooldown after capture.
6. Before winter (September for the Eastern Front): raise infrastructure and hubs in held territory;
   plan pullback lines if the front outruns supply.

## 6. Trucks, trains, attrition

- Trucks fill the gap between a hub and the army and take attrition scaled by terrain (mud x5, marsh
  x4, jungle x3). Motorised and mechanised divisions use the same truck pool; keep stock above need.
- Buffers: trucks at 1.5x need, trains at 1.2x (the AI's own targets).
- Enemy logistics strikes destroy trucks and trains: keep air superiority over the rail line and hubs.
  The AI bombs hubs with about 3 planes each.

## 7. Garrison and occupation

Resistance and compliance per state:
- New state: resistance 1%, compliance 0%. Resistance target base 35%, modified by stability (+0.2
  per % below 100), compliance (-0.5 per %), peace (-10), claim (-5), VPs in the state (+5/+10/+20 by
  size), capitulated enemy (+10), low population (-20 below 10000, -50 below 1000).
- Activity chance at maximum resistance is 31% per tick; it sabotages infrastructure (damage 0.5-1.0),
  factories, resources and other buildings. Resistance spreads to neighbours at half strength.
- Suppression needed: 0.75 points per 1% resistance, clamped 10-50%.
- Garrison efficiency falls with manpower shortage squared and equipment shortage cubed. An
  under-garrisoned state gets +10% resistance target and -0.045 compliance gain.

Garrison template by hardness: resistance damage to the garrison falls 1% per 1% hardness, up to
-90%. Use 1 cavalry battalion where manpower is plentiful (cheapest), else 2-4 obsolete light tanks or
armoured cars (hardness about 80%), at reserve reinforcement priority. Never infantry plus military
police: MP raises losses. An army on garrison orders may hold 3x the usual cap. Garrison
configuration can guard supply hubs and railway junctions; do that first.

Occupation laws (`hoi4_occupation_law` keys):

| Law | Resistance target | Compliance gain | Garrison need | Notes |
|---|---|---|---|---|
| Military governor (`military_governor_occupation`) | -35% | -0.045 | normal | +10% resources, +8% manpower |
| Martial law (`martial_law_occupation`) | -60% | -0.055 | normal | Strongest suppression without extra garrison need; supply routes |
| Forced labour (`forced_labor_occupation`) | -40% | -0.08 | +15% | +40% resources, +30% damage to garrison |
| Harsh quotas (`harsh_quotas_occupation`) | -40% | -0.08 | +15% | +25% factories, +50% damage to garrison |
| Brutally oppressive (`brutally_oppressive_occupation`, fascist) | -75% | -0.11 | +25% | Garrison takes +100% damage; no compliance |
| Secret police oversight (`secret_police_oversight`) | -30% | -0.04 | -15% | Cheap garrison, mild gains |
| Local police (`local_police_force_garrison`) | -15% | -0.025 | -35% | Only for quiet states |

Ideology laws: democracies autonomous occupation (garrison -40%); communists liberated workers
(factories +20%, resources +30%); Italy colonial police, three tiers (resistance target up to -45,
garrison -40%, factories and resources +25%). `no_garrison` and autonomous occupation are
democratic-only; the tool refuses a law the game disables (`action_not_applied`).

Law rule:
- Step one law harsher when a state's resistance target reaches 25; go milder only at peace.
- Per state by economy: Harsh Quotas on factory states, Forced Labour on resource states, Military
  Governor on populous states, Martial Law on supply routes.
- `hoi4_occupation_law` sets only the default law; pick it for the states that matter most (supply
  routes first).
- Never let a supply-route state reach 75% resistance.

Thresholds (each level switches off again 2 points below its threshold):

| Level | Effect on the state |
|---|---|
| Resistance 25% | garrison penetration chance +50% |
| Resistance 50% | resistance damage to the garrison +100% |
| Resistance 75% | for the controller: army speed -50%, local supplies -50%, attrition +30%, no strategic redeployment |
| Resistance 90% | local factories -3%; on a core state the resistance declares independence |
| Compliance 15% | enemy operative detection +25% |
| Compliance 25% | required garrison -25%, recruitable population +0.5% |
| Compliance 40% | local factories and resources +10% |
| Compliance 60% | recruitable population factor +20% |
| Compliance 80% | collaboration government possible (La Résistance) |

Why partisans wreck supply: resistance sabotages infrastructure and railways; damaged infrastructure
supports only 0.15 of a level in supply; each damaged railway cuts flow by 5; a captured rail needs
the cooldown. Hubs behind the line starve. Partisans do not take province control: the damage is
sabotage plus lost garrison.

## 8. Command structure, fronts, AI behaviour

- Field marshal: up to 24 divisions in one army, or an army group of up to 5 armies. Corps commander:
  up to 24 divisions; cannot command army groups. Keep every army at 24 divisions or fewer (the plan
  runner splits larger armies with `hoi4_army_split`).
- Army group of 2-5 armies per front, one field marshal each; promotion costs 40 command power.
  Command power cap 80, +0.4 a day.
- Deployed HQ (1.19): a leader is deployed through an Army HQ template. Planning cap and speed scale
  1.0 / 0.95 / 0.9 / 0.85 / 0.8 by HQ distance (0 to 4 provinces behind the line) and 0.8 with no HQ.
  Deploy and withdraw take 1-3 days. The AI deploys HQs only for armies of 10+ divisions, at most 5.
  No tool deploys an HQ yet.
- Planning: +2% a day under a plan to a base cap of 30% (techs add); it decays 1% a day under a plan
  and 3% a day under manual orders. Do not reissue front orders without need.

Traits worth picking:
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

Front-line orders: front lines along the border plus offensive lines for targets; one army per front
segment. Keep 20-25% of divisions in reserve on a defensive front; rotate divisions below about 30%
org or 60% strength to a supplied rear province for 5-10 days and reinforce before returning.
Reinforcement priority has three tiers (reserve, normal, elite): elite for front-line templates.

Fortification: defensive lines on rivers, hills and forests with bunkers (500 + 500 per level) give
more than open plains; AI terrain importance: mountain 10, marsh 8, desert 8, jungle 6, forest 5. Fort
penalty base -15% when attacking over forts.

AI behaviour: its fronts allocate 1.0 division per available supply point and keep at least 1 division
per province; it guards supply hubs (importance 4) and bombs enemy hubs; fixes bottlenecks weekly above
85% saturation; motorises below 80% supply (checked every 52 h); reinforces fronts by theatre priority.
Expect counter-attacks where your hub or railway is thin.

Air supply (`hoi4_air_mission` `air_supply`): only for pockets you will relieve within a week; 0.01
supply per plane, so pockets of more than a few divisions cannot hold. Check the transport plane's
payload in its tooltip.

## 9. Per-theatre habits

| Theatre | Supply facts | Habits |
|---|---|---|
| Poland 1939 | Dense infra and rail; Polish roads strong, Carpathians in south | Two army groups, short supply line from Silesia and East Prussia. Capture rail junctions, build one hub behind each; stop at the river lines for the 10 day rail cooldown |
| France 1940 | Ardennes forest and hills, low infra; Low Countries high infra; Maginot | Armour with motorised support through the Ardennes only with hubs at the border; capture ports; infantry follows by rail; accept a gap while captured rail converts |
| USSR 1941+ | Soviet infra 1-2, few rails, non-core -50% supply speed, mud autumn, extreme cold from November | Build hubs on every captured trunk line; halt at hub range; winter specialist, pre-stocked trucks; do not outrun the railhead by more than 2-3 provinces |
| China | Low infra, jungle and mountain south and west, rivers; Japan supplied by sea | As China: defend cities (urban improves supply flow), use mountains, keep armies near rail towns, Japanese push suffers attrition. As Japan: capture ports, keep convoys escorted |

## 10. Rules and requirements

### (a) Automatable standing rules
1. Any army above 24 divisions: split (`hoi4_army_split`); any army of 8+ divisions without a general:
   `hoi4_army_commander` (the plan runner does both).
2. Unassigned divisions in a state: `hoi4_army_assign_plan`, then `hoi4_army_assign` or
   `hoi4_army_create`.
3. Training lines without a location: `hoi4_deploy_locations` to a state in the home hub chain.
4. Weekly at war: if no railway, port, hub or infrastructure item is in the top three of
   `hoi4_construction_queue`, queue one for the front and move it up (`hoi4_construction_order`).
5. At war, every tick: `hoi4_supply`; act on `below_50`, `below_70` and `overdrawn_provinces` by the
   section 3 table.
6. Occupied states (`hoi4_occupation`): when a state's resistance target reaches 25, step the law one
   harsher; milder only at peace; garrison with the hardness template.
7. Before December in the north and on the Eastern Front: winter specialist generals assigned.
8. Truck and train stock at 1.5x and 1.2x of need (production line check).

### (b) Needs live decisions
- Which front to attack and when, based on hub reach and rail cooldown.
- Whether to stop an offensive under 50% supply, or pause for a rail level or port.
- Which general gets logistics_wizard or defensive_doctrine; who to promote.
- Occupation law changes in states with active resistance (resources against revolt).
- Retreat or air supply for an encircled pocket.
- Whether to motorise a hub or reroute a rail.
- Naval invasion: port choice, floating harbour, convoy escorts.

### (c) Tools
- Reads: `hoi4_supply` (section 3), `hoi4_armies` (division ids, motorization level, threat and
  `encirclement_risk`), `hoi4_occupation` (resistance, target, compliance per state, default law),
  `hoi4_construction_queue`, `hoi4_state_slots`, `hoi4_states`, `hoi4_convoys` (convoy count, naval
  bases), `hoi4_resources` (fuel stock), `hoi4_alerts` (`resistance`, `not_enough_garrison`),
  `hoi4_map_mode` `supply` / `resistance` / `compliance` (visual).
- Actions: `hoi4_army_motorization`, `hoi4_railway_build`, `hoi4_construction_add`,
  `hoi4_construction_order`, `hoi4_deploy_locations`, `hoi4_train`, `hoi4_army_create` / `assign` /
  `split` / `merge` / `detach` / `commander`, `hoi4_army_group_create`, `hoi4_occupation_law` (default
  law), `hoi4_template_create` / `edit` (garrison template), `hoi4_production_add` (trucks, trains,
  convoys), `hoi4_air_mission` (`air_supply`), `hoi4_naval_invasion`.
- Gaps: hub and rail throughput and the route bottleneck; truck, train and other equipment stockpile
  against need; stored supply buffer and attrition; garrison strength and suppression; per-state
  occupation law; garrison template assignment; Army HQ deploy; area-defence and fallback orders.

## 11. Tactics: supply and logistics

| ID | Tactic | How it works | When | Mode |
|---|---|---|---|---|
| LO01 | Supply fix order | Motorise first (no construction), then railway levels, then a port (connects by sea at once), a hub last (20000 points). Exceptions: a front beyond every hub's 2-3 province reach with no port; minors' railway reorganisation decision (+300% hub speed for 120 days) | Every supply problem | FAIR |
| LO02 | Rail work on top | New projects start at the bottom of the construction queue; move a railway level to the top | At war | FAIR |
| LO03 | Weakest link caps a hub | Rail throughput 15/20/25/30/35 by level; a hub gets the flow of the weakest segment to the capital; queue one level where an axis hub's railway is below 3 | Axis hubs | FAIR |
| LO04 | Motorise on day 1 and count trucks | Full motorisation of a hub needs 60 trucks (+2.2 flow, diminishing per level); generals added to a group later do not inherit its motorisation; re-check after merges. Trucks take x5 attrition in mud, x4 marsh, x3 jungle | Fronts past the railheads | FAIR |
| LO05 | Fuel only inside hub range | Fuel refills only in a hub's or port's range; out of fuel equipment fights at 10% and moves at 40%; keep fuel above 25% before an offensive | Armour advances | FAIR |
| LO06 | Cut consumption | Logistics company -10%, Logistics Wizard -15%, logistics skill -2.5% per point, Grand Battle Plan logistics focus -10%, Mass Assault milestone -15% (additive); Mass Assault also +48 hours of grace | Poor-supply theatres | FAIR |
| LO07 | Ports first after a landing | Landed troops have 72 hours of grace; a floating harbour gives about 30 days | Landings | FAIR |
| LO08 | Infrastructure on the route | +0.3 supply per infrastructure level; maximum infrastructure cuts flow drop-off by 30% and mud growth by 80% | Long axes | FAIR |
| LO09 | Railway 5 on the main axis | Bring the main axis railway to level 5 before a large offensive | GER 1941, USSR | FAIR |
| LO10 | Trains for distant fronts | Train need grows by 3% per rail province from the capital; hold trains at 1.2x need | USSR, China, long rail fronts | FAIR |
| LO11 | Non-core supply is slow | Supply moves 50% slower outside own cores | Deep advances | FAIR |
| LO12 | Check trains, trucks and convoys | Trains carry capital to hub, trucks hub to division, convoys by sea; supply is the weakest of the three | Every war | FAIR |
| LO13 | Resistance on the corridor | At 75% resistance a state gives -50% local supply, +30% attrition, -50% speed and no strategic redeployment; garrison corridor states and use a harsher law there at 50% | Occupied corridors | FAIR |
| LO14 | Air supply only for a relief | 0.01 supply per plane; a drop lasts 168 hours; only for a pocket you will relieve within a week | Pockets | FAIR |
| LO15 | Rear moves only | Strategic redeployment and sea transfer burn no fuel but arrive at low organisation; use them for rear moves only | Rear transfers | FAIR |
| LO16 | Barbarossa rail plan | From about four months before the attack: level-5 railways from the home hubs to the border, extended and joined to the captured hubs while advancing; ports on the Black Sea (Nikolayev) and the Baltic before Leningrad; supply hubs in front of Moscow and Stalingrad once the front nears them | Germany 1941 | FAIR |
| LO17 | Overdrawn supply starves armour | An area needing more than 1.5 times its maximum flow gets no fuel at all: tanks and motorised units drop to 10% of their equipment stats and 40% speed | Keep infantry off the armour's supply tiles; split an overdrawn stack at once | FAIR |
| LO18 | Capital pockets keep supply | An encircled enemy capital keeps supply as a hub: assault it, do not wait to starve it | Pockets with the enemy capital | FAIR |
| LO19 | Scorched earth on retreat | Where a scorched-earth decision exists, destroy hubs and railways before giving them up; the attacker must repair them before pushing on | Retreats | FAIR |
