# Economy and build orders

Scope: peace-time and early-war industry (FAIR, Normal, historical AI; no exploits). Combat is out of scope.
Read it as a lookup table: find the trigger, apply the rule, check the "does NOT apply" note.

## 0. The five numbers to check every session

| Number | Where to read | Why |
|---|---|---|
| Free civs = civs - trade - consumer goods - already in projects | `hoi4_economy_detail` | Idle civs are the commonest defect. Target 0 idle. |
| Civs per project | rule: 15 max | One project never uses more than 15 civs. Parallelism = free civs / 15. |
| Free slots in the best states | `hoi4_state_slots` | Slots, not civs, often block you (section 2). |
| PP and PP/day | `hoi4_country_status`, section 6 | PP drives laws and advisors. |
| Months to the planned first war | the campaign plan | Decides civ vs mil (section 1). |

## 1. Civ vs mil vs infrastructure vs dockyard

### 1.1 Cost and speed facts

| Building | Base cost (construction points) | Cap per state | Shares slots |
|---|---|---|---|
| Civilian factory | 10800 | 20 | yes |
| Military factory | 7200 | 20 | yes |
| Dockyard | 6400, coastal only | 20 | yes |
| Infrastructure | 6000 | level 5 | no |
| Synthetic refinery | 14500 | 3 per state | yes |
| Fuel silo | 5000, each further level 500 cheaper | no state cap | yes |
| Energy / industrial infrastructure ("keystone") | 12000 | 1 per state, either one | yes |
| Railway | 170 + 130 per level | province | no |
| Naval base (port) | 5000 + 1000 per level, coastal provinces only | 10 per province | no |
| Supply hub | 20000 | 1 per province | no |

With 15 civilian factories (`calc.py build-order`, no speed bonus) a railway level takes 2-3 days, a first port
level 48-67 days, a supply hub 191-268 days at state infrastructure 2 to 0 (about 270-380 days under Civilian
Economy's -30%). Supply construction follows `supply-logistics-deployment.md` §5: railways, then a port where no
supply reaches the shore, then trucks, and a hub only when the calculation shows it finishes before it is needed.
Every construction recommendation states its cost in points and its estimated finish date.

Speed: a civilian factory gives 5 construction points a day powered, 4 unpowered. State infrastructure multiplies
it by 1 + 0.2 x level (x2 at level 5). Speed bonuses stack additively, bounded about -70% to +65%.

Time = cost / (civs x 5 x infra multiplier x (1 + bonuses)). Example: a civ factory with 15 civs, state infra 3
(x1.6), +20% bonuses: 10800 / (15 x 5 x 1.6 x 1.2) = 75 days. A mil factory under the same conditions: 50 days.

Rule: **never put more than 15 civs on one project; open a second project instead.**
Fewer than 15 civs on a project is fine; do not wait to collect 15.

### 1.2 Decision table (when to build what)

Switch date = war date - 2.9 x civilian payback, in practice about 2-3 years before the first war.

| Situation | Build | Why |
|---|---|---|
| Before the switch date, civ share of factories < 60% | Civilian factories | Compounding wins early. |
| After the switch date | Military factories in new slots | A late civ repays too slowly. |
| Under 18 months to war, or war started | Military factories only (plus infra in the war-front supply path) | Civilian factories queued in the last 18 months never repay. |
| Country at war in 1939 (GER, ENG, POL, ITA) | Military early | The war comes before late civs repay. |
| Civ count already > target of section 1.3 | Military factories | Do not over-build civs. |
| State infra <= 2 and >= 5 factory levels planned in that state | Infrastructure first | Level 3 -> 4 costs 6000 and saves ~12% of 43200+ points; break-even is ~5 factory builds, plus supply and resource gains. |
| Coastal, no sea-trade need, no navy plan | No dockyards | Dockyards make nothing for a land war (section 8). |
| Free civs > 0 and no free slot anywhere | Infra in the full states (uses no slot) or `industrial_infrastructure`/`energy_infrastructure` where unlocked | Keeps civs busy while slots are gained from focuses and industry techs. |

**Does NOT apply:** countries with a scripted factory-gifting focus tree (Germany, Poland, China, section 10):
the focus gives instant factories, so spend your own civs only where the focus did not. Also not when a focus or
event deadline demands a certain mil count in a given month.

Small nations: infrastructure first only in states with many free slots and few civilian factories assigned; a 5-slot
state gains one slot per industry tech, so with 9 or more civilian factories building there, build factories directly.

### 1.3 Target factory counts by year (generic mid-size; adjust to the country)

Counts are totals (civ / mil). Approximate 1936 starts: Germany ~35 civ / 28 mil / 10 dockyards, Poland ~18 / 11 /
3, China ~13 / 11 / 2, Romania ~11 / 7 / 2. Read the real start values with `hoi4_industry`.

| Date | Civ target (share of all factories) | Mil | Notes |
|---|---|---|---|
| Jan 1936 | current | current | Fix laws, queue civs, research construction. |
| End 1936 | +25% over start | start | All civs on civ projects. |
| End 1937 | +50% over start; ~60% civ share | start +20% | Mil from the switch date (1.2), first in slot-rich states. |
| End 1938 | no civs queued within 18 months of the war | +40% to 80% over start | Most new slots go to mil. |
| Mid 1939 | civs frozen | mil at or near the target for the war plan | Law -> Partial/War mobilisation (section 5). |

The percentages are guidance, not game constants. Cut them if the first war is early (Poland 1939.9, China 1937.7).

## 2. Slots, state categories and shared slots

| State category | Slots |
|---|---|
| tiny_island, wasteland, enclave | 0 |
| pastoral, small_island | 1 |
| rural | 2 |
| large_island | 3 |
| town | 4 |
| large_town | 5 |
| city | 6 |
| large_city | 8 |
| metropolis | 10 |
| megalopolis | 12 |

Rules:
- Civ, mil, dockyard, refinery, silo, facility and reactor **share** one slot pool per state. Infrastructure, rails
  and forts do not. Shared slots cap at 25 per state.
- Every concentrated or dispersed industry level gives +20% building slots: five levels double a state's slots.
  Count these future slots, but rank factory states by current infrastructure first and free slots second:
  infrastructure built ahead for slots that open years later rarely repays, except on Civilian Economy (factories
  -30% speed, infrastructure unaffected).
- Other extra slots: focuses (many in the Germany, Poland and China trees), some ideas, dams (+15%) and reactors (+15%).
- Building infrastructure does not raise a state's category.
- Buildings need ~50% of the state's VP controlled; occupied and bombed states rebuild only under your control.

Choose states: **highest current infrastructure first**, then most free slots (now and after industry techs), then a rail
or naval link to the capital (+20% resources). Read free slots with `hoi4_state_slots`, queue with
`hoi4_construction_add`.

## 3. Infrastructure, rails and supply first - when

| Trigger | Action |
|---|---|
| Industrial state with infra <= 2 and >= 5 planned factory levels | Infra to 4 before factories (section 1.2). |
| State with a strategic resource (oil, rubber, steel, chromium, aluminium) and infra < 5 | +15% extracted resources per infra level; worth building when the resource is short. |
| Front-line or invasion axis within 12 months | Railway levels on the supply path from existing hubs and ports; a port on the coast nearest the front; a supply hub only when `calc.py build-order` finishes it before the war (`supply-logistics-deployment.md` §5). |
| Factory state without a rail line to the capital | Add rail; connected states get +20% resources. |
| Enemy air or naval raids on the state | Rebuild only after the threat; order repairs by type (§15 EC08). |

Rule: if free civs > 0 and no slot is free, queue infra (up to 5) in the largest industrial states. If infra is
already 5 everywhere, queue railway levels along the planned attack axis (a supply hub only after the cost and time
check).

**Does NOT apply:** states you will lose in the first war (border states of a doomed front): spend nothing there.

## 4. Capital and VP

- Capital supply grows with factories (base + 0.3 per civ, plus mil and dockyard terms). Do not move the capital to
  chase this.
- Capital VP states get infra and rails first: the whole network connects there.
- Strategic bombing picks factories by importance (civ 50, mil 70, naval 30): front-adjacent factory clusters are the
  first raid targets. In war build new factories outside enemy bomber range.

## 5. Laws: economy, trade, conscription - cost, effect, timing

### 5.1 Economy laws (150 PP each)

| Law | Consumer goods | Construction speed | Energy use | Fuel gain | Other | Available when |
|---|---|---|---|---|---|---|
| Civilian Economy (default) | 35% | civ and mil -30% | -30% | -40% | conversion +30% | default |
| Early Mobilization (`low_economic_mobilisation`) | 30% | civ and mil -10% | -15% | -25% | | war support > 15% |
| Partial Mobilization | 25% | mil +10% | 0 | -10% | conversion -10% | war support > 25% |
| War Economy | 20% | mil +20% | +25% | 0 | conversion -20% | war support > 50% and (fascist or communist, or at war with an enemy whose industry is over 40% of yours) |
| Total Mobilization | 15% | mil +30% | +50% | 0 | conversion -30%, recruitable population -3% | at war, war support > 80%, enemy industry over 50% of yours |

Special laws: Germany's Totaler Krieg (15% consumer goods, +50% energy); National Defence State +45% energy.

How it plays:
- Civilian -> Early is +29% construction speed on civs (0.9/0.7) and 5 points less consumer goods.
- Partial's +10% is for **mil** construction only; civ construction is back to normal (no penalty).
- Consumer goods are a share of civilian plus military factories, paid in civilian factories: each law step frees
  about 5 factories per 100.

Timing rule:
1. Day 1: if war support allows, Civilian Economy -> Early as soon as available.
2. If war support is below 15%, raise it with the available focuses or decisions (propaganda, nationalism,
   mobilisation) before the law.
3. Partial when mil construction dominates the queue (about 12 to 18 months before war).
4. War Economy and Total Mobilization the moment their gates open (table); check coal first (energy +25% / +50%).
5. Do not go from Early to Partial while still building civs: the net gain is only the 5% consumer goods.

**Does NOT apply:** Germany's focus-driven consumer-goods variable (section 10.1), and countries with special
economy laws.

### 5.2 Trade laws (150 PP each)

| Law | Factory and dockyard output | Construction speed | Research | Export share | Available |
|---|---|---|---|---|---|
| Free Trade | +15% | +15% | +10% | 80% of surplus | every country; Germany loses it after `GER_autarky_efforts`; China under the Nine Power Treaty is locked on it |
| Export Focus | +10% | +10% | +5% | 50% | every country except China under the Nine Power Treaty |
| Limited Exports | +5% | +5% | +1% | 25% | a democracy at war with a strong enemy, or a non-democracy in Partial Mobilization or higher |
| Autarky (Germany focus) | 0 | +5% | +5% | 15% | `autarkic_economy_improved` adds +5% output |
| Closed Economy | 0 | market-credit bonus -10% | 0 | 0 | at war, fascist or communist, War Economy or higher; no market access |

Free Trade is the cheapest +15% output and +15% construction in peace: take it (or Export Focus) with the first
150 PP in 1936. The export share is reserved even with no buyer: step down a law when exports starve your own lines.

### 5.3 Conscription laws

150 PP each. Training time: Extensive +10%, Service by Requirement +20%, All Adults Serve +30%, Scraping the Barrel
+50%. Output cost: Service by Requirement -10%, All Adults Serve -30%, Scraping the Barrel -40% factory, dockyard and
construction output. Take only the law the manpower plan needs; one step (Limited -> Extensive) about 12 months before
war. Gates: `politics-diplomacy-espionage.md` §3.

A new law raises only the ceiling: recruitable population then climbs about 0.01 percentage points a day (up to 30%
faster with war support above 50%, up to half as fast below it), so Limited to Extensive (2.5% to 5%) fills in about
6-10 months; time the step from the date the manpower is needed.

## 6. Political power: spend vs bank

Base gain 2 PP a day, but a running national focus costs 1 PP a day, so a country that always runs a focus nets about
+1 a day plus factors (advisors, ideas, stability). Cap 2000. Laws cost 150 and do not require the previous law:
jump straight to the target law.

Spend order:
1. Chief of army early (daily army XP).
2. Trade law and economy law (section 5).
3. A PP or stability advisor.
4. Construction advisor while building civilian factories (Germany: Schacht, 75 PP, leaves when Mefo ends).
5. Conscription law when the manpower plan needs it.
6. Bank: 150 PP from 18 months before war for the war-time law switch; 100 PP margin for event choices.

Never waste PP on diplomacy spam (improve relations 10, attaché 100): an attaché only for a war-support gate or XP (PO19).
**Does NOT apply:** when an event needs a particular PP price; read the option first.

## 7. Resources, trade, fuel and repair

Resources: oil, aluminium, rubber, tungsten, steel, chromium, coal. Trade: 8 units per civilian factory; sea trade
needs 0.1 convoy per unit. At most 80% of factories may be traded away.

| Shortage | Immediate action | Structural fix |
|---|---|---|
| Any line red | Buy the missing resource (a few civs) or let automatic importing cover it. | Conquest of a resource state, or a trade partner in the same faction. |
| Steel (most equipment) | Trade for steel first. | Excavation techs (+10% each, 5 levels). |
| Oil / fuel | Buy oil; build a fuel silo and refineries. | Synthetic refinery after `synth_oil_experiments` (1936): 48 fuel a day, +50% per oil-processing tech. |
| Rubber | Trade, then refineries with `rubber_processing` (1937). | Each refinery gives 1 rubber, +1 per rubber-processing tech, up to 5. |
| Aluminium | Trade; conquest. | Free Trade partner. |
| Chromium | Trade from a faction ally. | Conquest. |
| Energy / coal | Coal makes energy; unpowered factories lose output (§15 EC11). | Keep coal states; energy keystone in factory states. |

Each missing resource unit costs 5% output, lowest-priority lines first. The first red line is a trade order or a
re-prioritisation, not a new factory.

Trade rules:
- Keep trade civs < 20% of civs in peace (free civs matter more).
- Sea trade needs convoys and is cut by an enemy navy; land trade is not.
- A seller must be willing: opinion and distance matter; a country at war with you or behind a blockade will not sell.

Refineries share slots with factories: build them only when fuel or rubber is really short, in fast (infra 5) states.
Silos store fuel for a long war; stack them in one spare state. Keystones: `energy_infrastructure` (-7.5% factory
energy per infra level) and `industrial_infrastructure` (+10% local resources per infra level), 12000 each, one per
state; worth it only in states with 6+ factories or large resource output.

## 8. Dockyards and naval production

- Build dockyards only for a real naval plan (UK, Japan, USA, Italy), convoys when imports travel by sea, or a
  naval-invasion plan.
- A dockyard makes 2.0 IC a day (more when powered) and has no production efficiency; a powered military factory
  makes 4.5. A dockyard not building ships or convoys is dead weight.
- 5 to 10 dockyards cover a convoy programme for a mid-size trader.
- Land-war countries (Germany, Poland, China): dockyards only through focus gifts; convert nothing, ignore them.

## 9. Advisors, designers, MIOs

- Advisors: hire political economy advisors early if they add construction speed or factory output; skip purely
  military ones until war (default military advisor cost 50), except the chief of army.
- MIOs: a design team on a research slot costs 0.1 PP a day; a manufacturer on a line costs nothing. Funds come from
  research and from IC produced (0.1 per IC a day, cap 100). Value: +5% research on the matching tech, and traits.
  Rule: one MIO on each active equipment line and one design team per matching research slot while PP/day is positive.
- **Does NOT apply:** if PP is under the 150 bank and a law is still missing.

## 10. Country notes (focus and idea modifiers that change the order)

### 10.1 Germany

- Start: Mefo Bills. `GER_industrial_consumer_goods_factor` starts at **+0.10** and rises about **+0.03 per month**;
  at 100% it adds +10 points of consumer goods and -20% factory and dockyard output. Focuses and events (price
  controls, Vichy France, non-core states) change the monthly rate.
- Mefo gives +10% arms-factory and dockyard construction speed. It ends when Germany enters a war, or with
  `GER_prioritize_economic_growth` (which also adds `civilian_economy`, so skip it on a rearmament path).
- `GER_prioritize_economic_growth` and `GER_the_four_year_plan` are mutually exclusive. Four Year Plan: 5 weeks,
  instant civ in 2 core states, a tech bonus. Prefer it on a war path; the economic-growth route trades faster peace
  growth for the civilian law.
- Industrial chain: `currency_reforms` (-0.20 consumer-goods factor), `lower_taxes` (-0.15, +5% stability),
  `abolish_price_controls`, `housing_developments`, `industrial_expansion` (+5% factory output). Each gives instant
  factories plus +1 slot in random core states. Take them in this order while the factor is above ~0.6.
- Free Trade is open until `GER_autarky_efforts`, which adds `autarkic_economy` and locks out Free Trade and Export
  Focus. Take Free Trade first; accept the lockout only if you plan Autarky.
- Plan: Jan 1936 law + trade law; Four Year Plan; construction techs 1-2; military construction from the switch date;
  Mefo's +10% mil speed ends with the first war.

### 10.2 Poland

- Focuses `POL_central_region_strategy` (civ + slots), `POL_start_central_industrial_region` (70 days),
  `POL_expansion_of_new_towns` (mil), `POL_fill_the_railways_gaps` (infra), `POL_develop_upper_silesia`,
  `POL_the_four_year_plan` (construction tech bonus). `uncontrolled_exports` is unlocked by
  `POL_integrate_gdansk_industries` / `POL_expand_gdynia_seaport`.
- War is 1939.9: civs until early 1938 at most, mil afterwards. Start ~18 civ / 11 mil. Build railway gaps early;
  Poland has weak infra.

### 10.3 Nationalist China

- Start ~13 civ / 11 mil, mostly `town` states (4 slots), low infra. Infra first in the few industrial states (1.2).
- Focuses: `CHI_rural_reconstruction_movement` (3 infra projects), `CHI_taiyuan_arsenal` /
  `CHI_develop_the_hanyan_arsenal` (+3 slots, mil), `CHI_financial_policy` (civ + slots),
  `CHI_unified_industrial_planning` (tech bonus), `CHI_industrial_evacuations`. The Japanese war starts around July
  1937; prepare mil from 1936.7; build civs only in states far from the coast.
- Trade: while `CHI_nine_power_treaty` is active China is locked on Free Trade (no Export Focus, Limited Exports or
  Closed Economy); stay on it even at war.

## 11. First 3 game years - dated checklist (generic mid-size country)

| Date | Action | Trigger/why |
|---|---|---|
| 1936.1.1 | Read free civs, PP, laws (`hoi4_economy_detail`). Queue civs in the best slot states. | No idle civ on day 1. |
| 1936.1.1 | Research: construction I, basic machine tools, fuel silos if oil-short. | Construction I is +10%. |
| 1936.1.1 | Pick the industry focus with instant factories. | Section 10. |
| 1936.1 | Trade law (150 PP) when PP allows. | Section 5.2. |
| 1936.2 to 1936.6 | Economy law to Early at war support > 15%. | Section 5.1. |
| 1936.7 | Concentrated or Dispersed Industry (one). | Concentrated per level: +15% factory, +10% dockyard, +20% slots. Dispersed per level: +10% factory, +10% dockyard, +10% retention, +5% start efficiency, less bombing damage, +20% slots. |
| 1937.1 | Construction II. Review resource shortages; start trade. | +10% construction. |
| 1937.1 to 1937.12 | Civs near 60% share; infra in key states. | Section 1.3. |
| Switch date (1.2) | New construction goes to mil, slot-rich states first. | Section 1.2. |
| 1938.6 | If war inside 18 months: Partial Mobilization at war support > 25%. | Section 5.1. |
| 1938.12 | Civs frozen; refineries and silos if fuel short. | Section 7. |
| 1939.1 | Construction III. Bank 150+ PP. | Section 6. |
| 1939.x | War Economy when its gate opens. | Section 5.1. |

**Does NOT apply:** early-war countries (China 1937.7, Poland 1939.9, and any country that was attacked). Shift each
row earlier by the missing months.

## 12. (a) Automatable standing rules

A daily standing plan (`hoi4_plan`, run by `hoi4_plan_run`) can do these in this order:
1. Free civs > 0: queue the next building by the table in 1.2 in the states with most free slots (plan
   `construction` entries; by hand `hoi4_state_slots` then `hoi4_construction_add`).
2. Industrial state with infra <= 2 and >= 5 planned factory levels: infra first.
3. PP >= 150 and Free Trade/Export Focus not active and not locked: take it (plan `politics`).
4. PP >= 150 and Early/Partial mobilization available, not active, no focus forbids it: take it.
5. A production line short of a resource (`hoi4_production_shortages`): buy it (plan `trade`) or let
   `auto_import` cover it; never build civs for this.
6. A free research slot: construction, machine tools, then fuel/silos.
7. A decision with +factory or +slot costing <= 50% of PP: take it.
8. Dockyards with no ship or convoy line: report; build no more.
9. Each weekly digest: free civs, free slots, PP, consumer-goods share.

## 13. (b) Needs a live decision

- Civ-to-mil switch date (needs the war plan and war support).
- Mutually exclusive focuses (Germany Four Year Plan vs Economic Growth; Autarky vs Free Trade).
- Autarky, Closed Economy and special economic laws.
- Trade partners and convoy escorts at war.
- Dockyard and naval programme size.
- Refinery vs trade for fuel (needs the real fuel curve).
- Spending PP below the 150 bank (events, advisors with big trade-offs).
- Moving factories to safer states under bombing.

## 14. (c) Tools

| Need | Tool |
|---|---|
| Factory counts, PP, stability, war support, fuel | `hoi4_industry`, `hoi4_economy`, `hoi4_country_status` |
| Laws, consumer goods, free civs, modifiers, fuel | `hoi4_economy_detail` |
| Free slots per state | `hoi4_state_slots` |
| Queue a building or railway | `hoi4_construction_add` (factories, infra, refineries, silos, facilities, hubs, ports), `hoi4_railway_build` |
| Construction queue: read, reorder, remove | `hoi4_construction_queue`, `hoi4_construction_order` |
| Resources and line shortages | `hoi4_resources`, `hoi4_production_shortages` |
| Trade | `hoi4_trade`, `hoi4_trade_set` (amount 0 cancels), `hoi4_auto_import` (on/off and budget) |
| Laws and advisors | `hoi4_politics_options`, `hoi4_politics_choose` |
| Decisions, focuses | `hoi4_decisions_list`, `hoi4_decision_take`, `hoi4_focus_start` |
| Production lines | `hoi4_production_lines`, `hoi4_production_add`, `hoi4_production_set`, `hoi4_design_set_production`, `hoi4_naval_production_options` |
| MIOs | `hoi4_mios`, `hoi4_mio_trait`, `hoi4_mio_policy`, `hoi4_mio_design_team` |
| Occupation | `hoi4_occupation`, `hoi4_occupation_law` (default law) |
| Standing plan | `hoi4_plan`, `hoi4_plan_run` |
| Calculators | `calc.py build-order`, `calc.py pp` |

No tool yet:
- equipment stockpile read; per-line efficiency and unit-progress read
- production line reorder
- International Market (sell or buy equipment)
- equipment conversion and ship refit
- licence, lend-lease and volunteer dialogs
- per-state occupation law
- auto-import priority order and accepted shortage per resource
- energy balance and PP-per-day reads
- MIO line assignment and per-MIO funds, size and trait reads

## 15. Tactics: construction, energy, trade and fuel

| ID | Tactic | How it works | When | Mode |
|---|---|---|---|---|
| EC01 | Civilian-to-military switch date | Switch new construction to military factories about 2-3 years before the war: switch date = war date - 2.9 x civilian payback; civilian factories queued in the last 18 months never repay. Countries at war in 1939 (GER, ENG, POL, ITA) go military very early | Build plan | FAIR |
| EC02 | No civilian factories at war | Once the war runs, new civilian factories pay back in years; the filler becomes military factories, railways and infrastructure on the axis, air bases where the air plan needs room, AA in bombed states, unless the war will outlast a year | War | FAIR |
| EC03 | Convert, do not build, when slots are full | Civilian to military conversion costs 4000 against 7200 for a new military factory and needs no slot; never convert military to civilian (9000). Conversion cost +30% on Civilian Economy, -20% War Economy, -30% Total Mobilisation | Late war | FAIR |
| EC04 | Slot techs | Every concentrated or dispersed industry level adds +20% building slots (five levels double a state's slots); count future slots when ranking states | State ranking | FAIR |
| EC05 | Expensive buildings in fast states | Refineries, facilities and keystones go to infrastructure-5 or dam states (dam: +15% construction, +15% slots, -65% local energy, +10% resources); facilities follow the infrastructure multiplier | Facility, refinery builders | FAIR |
| EC06 | Infrastructure where slots are many | Infrastructure pays back in about 450-570 days and only in states with many current and future slots; rule of thumb: level 4 to 5 is almost always worth it, 3 to 5 when the state has 6+ building slots, below that only for resources; Total Mobilisation and production advisors make it pay less, Civilian Economy more; economy-law penalties do not slow it, so it is the build under Civilian Economy or Undisturbed Isolation; skip where a focus will max it | Slot-rich states | FAIR |
| EC07 | Resource infrastructure | +15% resource output per infrastructure level, +20% with a rail or naval link to the capital; worth it only when about 14+ extra units are used early. Industrial keystone (+10% resources per infrastructure level) in resource states, energy keystone (-7.5% energy per level) in factory states; 12000, one per state, better than infrastructure only at level 4-5 | Resource-short majors | FAIR |
| EC08 | Repair queue | 1.19.4: order repairs by type: rails, hubs and ports first during an advance, factories first after bombing | At war | FAIR |
| EC09 | Silos and refineries | Fuel silos stack in one spare state (5000, each further level 500 cheaper, no per-state cap). Refinery 14500, 3 per state: 48 fuel a day (+50% per oil-processing tech) and 1 rubber (+1 per rubber-processing tech, up to 5); build before the war, then cancel rubber imports | GER, ITA, JAP | FAIR |
| EC10 | Build in puppets | Building in a puppet lowers its autonomy and the factories are safe from bombing aimed at you | Puppet holders | FAIR |
| EC11 | Energy | Energy = 10 + 9 per coal (+15% per energy tech); demand per factory about 0.25 + 0.0225 x total factories, capped at 6.6. Unpowered: military 3.5 instead of 4.5 IC a day, civilian 4 instead of 5 construction points, and positive output modifiers shrink. Exported coal makes no energy | Large or conquering empires | FAIR |
| EC12 | Energy before a law step | War Economy +25% and Total Mobilisation +50% energy use: check coal before raising the economy law; step down in a deficit | GER, ITA, JAP | FAIR |
| EC13 | Concern for energy | Industrial Concern: +6 coal, +5% coal gain; Electronics Concern: -10% factory energy use | Coal-short countries | FAIR |
| EC14 | Trade in whole blocks | 8 resource units per civilian factory; a seller with less surplus still costs a full factory; buy from several sellers (buying 50%+ of one seller's surplus is blocked); prefer neutrals, allies and puppets; never buy from a planned enemy | Resource-short countries | FAIR |
| EC15 | Automatic importing (1.19.4) | Set a budget (default 25% of usable civilian factories), a resource priority and an accepted shortage per resource; no sea imports below 40% convoy efficiency; oil is topped up when fuel is under 50% until 90%; manual trades are blocked while it is on | Every country | FAIR |
| EC16 | Reset inherited trades | Cancel start deals on day 0 and rebuy only what the planned lines need | Day 0 | FAIR |
| EC17 | Only fuel stockpiles | Other resource surpluses vanish daily; stop oil purchases near 90% fuel; buy oil right before a big naval or armoured battle | All | FAIR |
| EC18 | Exports earn factories | Buyers' civilian factories work for the exporter; do not build infrastructure only to export | Resource-rich minors | FAIR |
| EC19 | Fuel facts | 48 fuel a day per surplus oil plus 48 base; capacity 50k + 100k per silo; economy laws cut fuel gain (Civilian -40%, Early -25%, Partial -10%); penalties start below 25%; idle armies burn none | Oil-poor countries | FAIR |
| EC20 | Consumer-goods floor | Consumer goods take at least 10% of factories and at least 1; below about 20 civilian factories a 5% cut may free nothing; plunder or gold only to sit at the floor | GER, minors | FAIR |
| EC21 | Spread factories under bombing | Bombing damage grows with the square of a state's undamaged share: many factories in one state take most of it | Bombed theatres | FAIR |
| EC22 | Early dockyards for the USA | Consumer goods count civilian and military factories only and dockyards have no efficiency to grow, so dockyards are cheap to build early | USA | FAIR |
| EC23 | Refineries a year before the war | About 12 refineries from mid-1938 for an Axis major (more if the air force is large); switch the construction MIO to the chemical designer while they build and back after; then cancel rubber imports | GER, ITA, JAP 1938 | FAIR |
| EC24 | Build in conquered land before its gold | Decisions that seize a capitulated country's gold or reserves lower consumer goods more the more factories you already own there: build military factories in the conquered states first, then click the decision | Germany after Poland and France | FAIR |
| EC25 | Forward bases before an overseas war | A year before the war: ports on the far shore and the stepping-stone islands, level-2 railways linking every hub and port to the network, motorised hubs, max air bases near the front; spare IC to AA and coastal forts at the exposed bases | Japan 1940 (Malaya, Taiwan, Hainan, Siam), any overseas offensive | FAIR |
| EC26 | Never build where the front will pass | New factories go behind the planned defence line (the USSR builds east of Moscow, never in the western border states; Japan never in Korea); max infrastructure in the fuel state before the war | Countries expecting an invasion | FAIR |
| EC27 | Re-set the auto-import budget | The budget is a factory count that shrinks when usable civilian factories fall (consumer-goods rises, spirits, lost states) and never grows back by itself; in peace it may buy from future enemies, handing them the factories | After any change in usable civilian factories | FAIR |
| EC28 | Split long railways | Each project takes at most 15 civilian factories: when free factories exceed 15 per open project, split a long railway into segments so each gets its own 15 | Railway pushes | FAIR |
