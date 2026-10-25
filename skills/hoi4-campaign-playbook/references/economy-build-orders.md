# Economy and build orders (HOI4 1.19.3, FAIR, Normal, historical AI)

Scope: peace-time and early-war industry only. Combat is out of scope.
Everything here is plain player play (no exploits). Read it as a lookup table:
find the trigger, apply the rule, check the "does NOT apply" note.

Evidence tags: **[F]** read from the installed game files; **[W]** paradoxwikis,
paraphrased; **[D]** derived by this playbook from [F]/[W] numbers (an estimate,
re-check on screen); **[?]** uncertain or version-dependent, verify in the tooltip.
`$G` = `.../Steam/steamapps/common/Hearts of Iron IV`. Numbers in tables are
1.19.3; other builds differ.

## 0. The five numbers to check every session

| Number | Where to read | Why |
|---|---|---|
| Free civs = civs - trade - consumer goods - already in projects | `hoi4_economy` | Idle civs are the commonest defect. Target 0 idle. |
| Civs per project | rule: 15 max | One project never uses more than 15 civs [F: `MAX_CIV_FACTORIES_PER_CONTRACT = 15`, `defines/00_defines.lua:687`, and user-verified for construction]. Parallelism = free civs / 15. |
| Free slots in the best states | `hoi4_states` | Slots, not civs, often block you (section 2). |
| PP and PP/day | `hoi4_economy` | PP drives laws and advisors (section 6). |
| Months to the planned first war | the campaign plan | Decides civ vs mil (section 1). |

## 1. Civ vs mil vs infrastructure vs dockyard

### 1.1 Cost and speed facts

| Building | Base cost (construction points) | Cap per state | Shares slots | Source |
|---|---|---|---|---|
| Civilian factory | 10800 | 20 | yes | [F] `common/buildings/00_buildings.txt` (`industrial_complex`) |
| Military factory | 7200 | 20 | yes | [F] same (`arms_factory`) |
| Dockyard | 6400, coastal only | 20 | yes | [F] same |
| Infrastructure | 6000 | level 5 | no | [F] same |
| Synthetic refinery | 14500 | 3 per state | yes | [F] same |
| Fuel silo | 5000 | no state cap | yes | [F] same |
| Energy / industrial infrastructure ("keystone") | 12000 | 1 per state, either one | yes | [F] same (`energy_infrastructure`, `industrial_infrastructure`) |
| Railway | 170 + 130 per level | province | no | [W]; `per_level_extra_cost = 130` [F] |

Speed (approximate): a civ gives roughly 4 to 5 points a day (unpowered or powered;
`BASE_FACTORY_SPEED 4`, `POWERED_FACTORY_SPEED 5`, `defines/00_defines.lua:630-635`)
[?: those defines are for production, the wiki states 4 for construction]. State
infrastructure multiplies it by about 1 + 0.2 x level (x2 at level 5) [W]; the
define `INFRA_MAX_CONSTRUCTION_COST_EFFECT = 1` is consistent [F]. Speed bonuses
stack additively and are bounded about -70% to +65% [W].

[D] Time = cost / (civs x ~5 x infra multiplier x (1 + bonuses)). Example: a civ
factory with 15 civs, state infra 3 (x1.6), +20% bonuses: 10800 / (15 x 5 x 1.6 x 1.2)
= 75 days. A mil factory under the same conditions: 50 days.

Rule: **never put more than 15 civs on one project; open a second project instead.**
Fewer than 15 civs on a project is fine; do not wait to collect 15.

### 1.2 Decision table (when to build what)

| Situation | Build | Why |
|---|---|---|
| More than ~24 months to the first war, civ share of factories < 60% | Civilian factories | A civ's own output (about 5 pts/day) repays its cost in roughly 2 years under typical bonuses [D]; compounding wins early. |
| 12 to 24 months to war | Mix 50:50, mil lines first in states with spare slots | Conversion time and equipment stockpile need lead time. |
| Under 12 months, or war started | Military factories only (plus infra in the war-front supply path) | New civs finish too late to pay back [D]. |
| Civ count already > target of section 1.3 | Military factories | Do not over-build civs. |
| State infra <= 2 and >= 5 factory levels planned in that state | Infrastructure first | Level 3 -> 4 costs 6000 and saves ~12% of 43200+ points [D]; break-even is ~5 factory builds, plus supply and resource gains. |
| Coastal, no sea-trade need, no navy plan | No dockyards | Dockyards cannot make anything for land war; see section 8. |
| Free civs > 0 and no free slot anywhere | Infra in the full states (does not use slots) or `industrial_infrastructure`/`energy_infrastructure` [?] where unlocked | Keeps civs busy while slots are gained from focuses. |

**Does NOT apply:** countries with a scripted factory-gifting focus tree (Germany,
Poland, China, section 10) - the focus gives instant factories, so only spend your
own civs where the focus did not. Also does not apply when a focus or
event deadline demands a certain mil count in a given month.

### 1.3 Target factory counts by year (generic mid-size; adjust to the country)

Counts are totals (civ / mil). Starting sample from `history/states` base blocks
[F, approximate: dated additions excluded] showed Germany ~35 civ / 28 mil / 10
dockyards, Poland ~18 / 11 / 3, China ~13 / 11 / 2, Romania ~11 / 7 / 2. Read the real
start values with `hoi4_economy`.

| Date | Civ target (share of all factories) | Mil | Notes |
|---|---|---|---|
| Jan 1936 | current | current | Fix laws, queue civs, research construction. |
| End 1936 | +25% over start | start | All civs on civ projects. |
| End 1937 | +50% over start; ~60% civ share | start +20% | Begin mil only in slot-rich states. |
| End 1938 | stop civs when ~55% civ share is reached or <18 months to war | +40% to 80% over start | Most new slots go to mil. |
| Mid 1939 | civs frozen | mil at or near the target for the war plan | Law -> Partial/War mobilisation (section 5). |

The year-by-year percentages are [D] playbook guidance, not game constants.
Cut them if the first war is early (Poland 1939.9, China 1937.7).

## 2. Slots, state categories and shared slots

| State category | Slots | Source |
|---|---|---|
| tiny_island, wasteland, enclave | 0 | [F] `common/state_category/*.txt` |
| pastoral, small_island | 1 | [F] |
| rural | 2 | [F] |
| large_island | 3 | [F] |
| town | 4 | [F] |
| large_town | 5 | [F] |
| city | 6 | [F] |
| large_city | 8 | [F] |
| metropolis | 10 | [F] |
| megalopolis | 12 | [F] |

Rules:
- Civ, mil, dockyard, refinery, silo, reactors etc. **share** one slot pool per state
  (`shares_slots = yes`, [F]). Infrastructure, rails and forts do not. Total slots
  cap at 25 (`MAX_SHARED_SLOTS`, `defines:754`) [F].
- Extra slots come from focuses (`add_extra_state_shared_building_slots`, widely
  used in Germany, Poland and China files [F]), some ideas, and buildings such as
  reactors (+15% slots) [F `00_buildings.txt:464`]. I found no `local_building_slots_factor` in
  `common/technologies` [F], although the wiki says industry techs raise limits
  [W, conflicting]; check a live tooltip before assuming.
- A state category does not grow by building infrastructure in the files read here
  [?]; do not count on it.
- Queue order (AI default priorities, useful as a sanity check): supply buildings
  1.1, civ 0.8, mil 0.7, infrastructure 0.2; core territory x2 [F `defines:3200-3210`].
- Buildings need ~50% of the state's VP controlled [W]; occupied and bombed states
  can rebuild only with your control.

Choose states: **most free slots first**, then highest infrastructure, then closest to
capital (supply +20% resources over rail to the capital, `SUPPLY_ROUTE_RESOURCE_BONUS 0.2`
[F]). That equals `hoi4_construction` `plan` with `choose=free_slots` (default).

## 3. Infrastructure, rails and supply first - when

| Trigger | Action |
|---|---|
| Industrial state with infra <= 2 and >= 5 planned factory levels | Infra to 4 before factories (section 1.2). |
| State with strategic resource (oil, rubber, steel, chromium, aluminium) and infra < 5 | +15% extracted resources per infra level [F `INFRASTRUCTURE_RESOURCE_BONUS 0.15`]; worth building when the resource is short. |
| Front-line or invasion axis within 12 months | Rails and infra on the supply path, supply hubs [F prio 1.1, 4.0 for rail]. |
| Factory state without a rail line to the capital | Add rail; connected states get +20% resources [F]. |
| Enemy air/naval raids on the state | Rebuild only after the threat; repairs are low priority (x0.3) [F]. |

Dummy-proof rule: if `free civs` > 0 and no slot is free, queue infra (up to 5) in
the largest industrial states. If infra is already 5 everywhere, queue rail and
supply hubs along the planned attack axis.

**Does NOT apply:** states you will lose in the first war (border states of a
doomed front) - spend nothing there.

## 4. Capital and VP

- Capital supply scales with factories: base + 0.3 per civ, plus mil and dockyard
  terms [F `defines:4267-4269`]. More factories in or near the capital raise supply
  throughput; do not move your capital to chase this.
- Capital VP states should receive infra and rails first, because the whole network
  connects there [D].
- Strategic bombing targets factories by importance (civ 50, mil 70, naval 30)
  [F `defines:3012-3014`]: front-adjacent factory clusters are the first raid targets.
  In war build new factories away from enemy bomber range.

## 5. Laws: economy, trade, conscription - cost, effect, timing

### 5.1 Economy laws (all cost 150 PP; `removal_cost = -1`) [F `common/ideas/_economic.txt`]

| Law | Consumer goods (base share of factories) | Construction speed | Other | Availability |
|---|---|---|---|---|
| Civilian Economy (default) | 35% | civ and mil -30% | conversion +30%, fuel gain -40% | default |
| Early Mobilization (`low_economic_mobilisation`) | **30%** | civ and mil -10% | fuel gain -25% | war support > 15% |
| Partial Mobilization | **25%** | mil +10% | conversion -10% | war support > 25% |
| War Economy | **20%** | mil +20% | conversion -20%, energy +25% | war support > 50% and fascism/communism, or war with ratio conditions |
| Total Mobilization | **15%** | mil +30% | conversion -30%, conscription -3% | at war and war support > 80% |

Resolved: Total Mobilization consumer-goods share is **15%** in 1.19.3, not 10%
(`consumer_goods_expected_value = 0.15`, `_economic.txt:914`; Totaler Krieg is also 0.15).

How it plays (the column reads "construction speed" because `production_speed_*_factor`
in these laws speeds building construction [F]):
- Moving Civilian -> Early is +29% construction speed on civs (0.9/0.7) [D] and 5
  points less consumer goods. It needs war support above 15%.
- Partial's +10% is for **mil** construction only; civ construction is back to normal
  (no penalty).
- Consumer goods are a share of factories (civs absorb it first) [W, ?]. 5 points on
  100 factories frees about 5 factories. Hence each step of law saves ~5 factories.

Timing rule:
1. Day 1: if war support allows, switch off Civilian Economy -> Early as soon as available.
2. If war support is below 15%, raise it via the available focus/decisions (look for
   propaganda, nationalism, or mobilisation focuses) before the law.
3. Move to Partial when mil factory construction dominates your queue (about 12 to 18
   months before war).
4. War Economy/Total only at war (Total needs a war); take the law the moment it is
   available if you are fighting a ratio-5 enemy.
5. Do not go from Early to Partial while still building civs: Partial loses nothing for
   civs, but the net gain is only the 5% consumer goods.

**Does NOT apply:** Germany's focus-driven CG variable (section 10.1), and
countries with `closed_economy`/special rules.

### 5.2 Trade laws (150 PP) [F `_economic.txt`]

| Law | Factory output | Construction speed | Research | min_export | Note |
|---|---|---|---|---|---|
| Free Trade | +15% | +15% | +10% | 80% of surplus | best peace-time; general availability |
| Export Focus | +10% | +10% | +5% | 50% | gated by country |
| Limited Exports | +5% | +5% | +1% | 25% | |
| Autarky (Germany focus) | 0 | +5% | +5% | 15% | `autarkic_economy_improved` has +5% output |
| Closed Economy | 0 | -10% CIC | 0 | 0 | no market access |

"min_export" is the share of your surplus that must be sold; if you have no surplus
Free Trade costs you nothing [D]. Free Trade is the cheapest +15% output and +15%
construction speed available in peace; drop it only when sanctions or blockades matter.

Timing: take Free Trade (or Export Focus) with the first available 150 PP in 1936.

### 5.3 Conscription laws

All 150 PP; removal free [F `common/ideas/_manpower.txt`]. Do not change before the
army needs recruits. One step up (Limited -> Extensive) about 12 months before war.
Each step up lengthens training (+10% and +20% on the two highest) [F lines ~700, ~1010].

## 6. Political power: spend vs bank

Base gain is 2 PP in the defines (`BASE_POLITICAL_POWER_INCREASE`, comment says
weekly) [F `defines:725`]; in-game it is shown per day, verify with `hoi4_economy`
[?]. Cap 2000 [F]. Laws cost 150.

Spend order (cheapest big payoff first):
1. Trade law Free Trade/Export Focus (150).
2. Economy law as soon as available (150; section 5.1).
3. Economic/industry advisor if it gives construction or factory output (typical 75 to
   150 PP; Schacht is 75 and disappears when the Mefo mechanism ends [F `characters/GER.txt:2422`]).
4. Conscription law when needed.
5. Bank: 150 PP at all times from 18 months before war (war-time law switch);
   100 PP safety margin for event choices.

Never: waste PP on diplomacy spam (improve relations costs 10, send attache 100 [F]).
**Does NOT apply:** when an event needs a particular PP price; read the option first.

## 7. Resources, trade, fuel and repair

Resources: oil, aluminium, rubber, tungsten, steel, chromium, coal [F
`common/resources/*.txt`]. Trade: 0.125 CIC per unit (8 units per civ) [F `resources` cic;
derived] and convoys 0.1 per unit over sea [F]. At most 80% of factories may be traded
away [F `TRADEABLE_FACTORIES_FRACTION`].

| Shortage | Immediate action | Structural fix |
|---|---|---|
| Any lines red | Buy the missing resource by trade (a few civs). | Conquest of a resource state or trade partner in the same faction. |
| Steel (most equipment) | Trade for steel first. | Excavation techs (+10% each, 5 levels) [F `excavation1`]. |
| Oil / fuel | Buy oil; build a fuel silo and refineries. | Synthetic refinery (14500, 3 per state; fuel +2/hour/level) [F] after `synth_oil_experiments` (1936) [F]. |
| Rubber | Trade, then refinery with `rubber_processing` (1937) [F, `local_resources_rubber = 1` per level; amount depends on tech - ?]. | Same. |
| Aluminium | Trade; conquest. | Exports/Free Trade partner. |
| Chromium | Trade from a faction ally. | Conquest. |
| Energy/coal | Factories need energy; powered +25% output [W,F `defines:630-636`]. | Build/keep coal states; coal is the energy resource [F `defines:24`]. |

Shortage penalty is applied per line (`PRODUCTION_RESOURCE_LACK_PENALTY -0.05` [F]); the
wiki gives -5% per missing resource unit up to -100% [W, ?]. Practical rule: the first
red line is a trade order or a re-prioritisation, not a new factory.

Trade rules:
- Keep trade civs < 20% of civs in peace (free civs matter more).
- Sea trade needs convoys and is cut by enemy navy; land trade is not.
- Trade needs a partner willing to sell; opinion and distance matter
  (`BASE_TRADE_FACTOR`, `DISTANCE_TRADE_FACTOR` [F]). A country at war with you or
  behind a blockade will not sell.

Refineries: stack up to 3 per state; they share slots with factories, so put them in
slot-rich states only when fuel is genuinely short. Silo: fuel storage for a long war.

**Resource-capped / keystone buildings:** the 1.19 files also have `energy_infrastructure`
(-7.5% factory energy per infra level) and `industrial_infrastructure` (+10% local resource
gain per infra level), 12000 each, one per state [F `00_buildings.txt:715-753`].
Worth it only in states with 6+ factories or large resource output [D].

## 8. Dockyards and naval production

- Build dockyards only for: a real naval plan (UK, Japan, USA, Italy), convoys when
  imports travel by sea, or a naval-invasion plan.
- A dockyard produces at ~2 IC/day (2.5 powered) vs 4.5 for a mil factory (defines
  `BASE_FACTORY_SPEED_NAV 2.0`, `_MIL 3.5`) [F]. If it is not building ships or convoys it
  is dead weight.
- 5 to 10 dockyards cover a convoy program for a mid-size trader [D].
- Land-war countries (Germany, Poland, China): dockyards only through focus gifts;
  convert nothing, ignore them.

## 9. Advisors, designers, MIOs

- Advisors: hire political economy advisors early if they add construction speed or
  factory output; skip purely military ones until war. Default military advisor cost 50 [F].
- Designers/MIOs: assigning a design team to a research slot costs 0.1 PP/day
  [F `ASSIGN_DESIGN_TEAM_PP_COST_PER_DAY`]; assigning a manufacturer to a line costs 0 [F].
  Funds come from research and from IC produced (0.1 per IC/day, cap 100) [F].
  Value: small research bonus (+5% on matching tech) [F], plus trait unlocks at 700 funds.
  Rule: assign one MIO to each active equipment line and one design team per matching
  research slot; never leave one idle when PP/day is positive.
- **Does NOT apply:** if PP is under the 150 bank and a law is still missing.

## 10. Country notes (focus and idea modifiers that change the order)

### 10.1 Germany (1.19.3 tree, Gotterdammerung-era) [F `common/national_focus/germany.txt`, `history/countries/GER - Germany.txt`]

- Start: Mefo Bills dynamic modifier. `GER_industrial_consumer_goods_factor` starts at
  **+0.10** and rises about **+0.03 per month** (`GER_monthly_consumer_goods_start`); at 100%
  it adds +10 points of expected consumer goods and -20% factory and dockyard output
  [F, history lines 185-199, `GER_scripted_effects.txt:818-845`]. The monthly rate is
  changed by focuses and events (price controls, Vichy France, non-core states) [F].
- Mefo gives +10% arms-factory and dockyard construction speed [F]. It ends when
  Germany enters war (event `germany_mefo_bills.1`, 2 hours after the war relation)
  [F `on_actions:4915-4960`], or by `GER_prioritize_economic_growth` (which also adds
  `civilian_economy`, so skip it for a rearmament path).
- `GER_prioritize_economic_growth` and `GER_the_four_year_plan` are mutually exclusive
  [F]. Four Year Plan: 5 weeks (`cost = 5`), instant civ in 2 core states, a tech
  bonus. Prefer the Four Year Plan route for a war path; the economic-growth route
  trades faster peace growth for the civilian law.
- Industrial chain: `currency_reforms` (-0.20 CG factor), `lower_taxes` (-0.15, +5%
  stability), `abolish_price_controls`, `housing_developments`, `industrial_expansion`
  (+5% factory output) [F]. Each gives instant factories plus +1 slot in random core states.
  Take them in this order while the CG variable is above ~0.6.
- `GER_autarky_efforts` adds `autarkic_economy`, but locks out Free Trade/Export Focus
  [F `_economic.txt:1217`]. Take Free Trade first, accept the lockout only if you plan
  Autarky.
- Plan: Jan 1936 law + trade law; Four Year Plan; keep construction techs 1-2; from
  1938 swap new construction to mil; Mefo disappears when the first war starts, so
  do not rely on its +10% mil speed beyond that.

### 10.2 Poland [F `poland.txt`]

- Focuses `POL_central_region_strategy` (civ + slots), `POL_start_central_industrial_region`
  (cost 10 = 70 days), `POL_expansion_of_new_towns` (mil), `POL_fill_the_railways_gaps`
  (infra), `POL_develop_upper_silesia`, `POL_the_four_year_plan` (construction tech bonus).
  `uncontrolled_exports` is unlocked by `POL_integrate_gdansk_industries` /
  `POL_expand_gdynia_seaport` [F `_economic.txt:1160`].
- War is 1939.9: keep civ building until early 1938 only, mil afterwards.
  Start totals ~18 civ / 11 mil (see 1.3). Build railway gaps early; Poland has weak infra.

### 10.3 Nationalist China [F `china_nationalist.txt`]

- Start totals ~13 civ / 11 mil, mostly `town` states (4 slots), low infra [F history
  sample]. Infra first in the few industrial states (1.2).
- Focuses: `CHI_rural_reconstruction_movement` (3 infra projects),
  `CHI_taiyuan_arsenal` / `CHI_develop_the_hanyan_arsenal` (+3 slots, mil), `CHI_financial_policy`
  (civ + slots), `CHI_unified_industrial_planning` (tech bonus), `CHI_industrial_evacuations`.
  The Japanese war starts around July 1937 in history; prepare mil conversion from
  1936.7, civs are only built in states far from the coast.
- Trade: export focus is gated by `CHI_nine_power_treaty` for China with Waking the
  Tiger [F `_economic.txt:1316`]. Check the idea before planning Export Focus.

## 11. First 3 game years - dated checklist (generic mid-size country)

| Date | Action | Trigger/why |
|---|---|---|
| 1936.1.1 | Read `hoi4_economy`: free civs, PP, laws. Queue civs in best slot states. | No idle civ at day 1. |
| 1936.1.1 | Research: construction I, basic machine tools, fuel silos if oil-short. | Construction I is +10% [F `industry.txt:1657-1673`]. |
| 1936.1.1 | Pick the industry focus with instant factories. | Section 10. |
| 1936.1 | Trade law (150 PP) when PP allows. | Section 5.2. |
| 1936.2 to 1936.6 | Economy law to Early at war support > 15%. | Section 5.1. |
| 1936.7 | Dispersed vs Concentrated Industry (choose one). | Concentrated: +15% factory output; Dispersed: +10% and efficiency [F]. |
| 1937.1 | Construction II (1937). Review resource shortages; start trade. | +10% construction [F]. |
| 1937.1 to 1937.12 | Keep civs near 60% share; infra in key states. | Section 1.3. |
| 1938.1 | Start mil factories in slot-rich states (Construction III arrives 1939). | Section 1.2. |
| 1938.6 | If war inside 18 months: Partial Mobilization when war support > 25%. | Section 5.1. |
| 1938.12 | Civs frozen; refineries/silos if fuel short. | Section 7. |
| 1939.1 | Construction III (1939). Bank 150+ PP. | Section 6. |
| 1939.x | War Economy only at war. | Section 5.1. |

**Does NOT apply:** early-war countries (China 1937.7, Poland 1939.9, and any country
that was attacked). Shift each row earlier by the missing months.

## 12. (a) Automatable standing rules

A daily standing plan can do these without thinking, in this order:
1. If free civs > 0: `hoi4_construction` `plan` the next building by the table in 1.2
   (choose `free_slots`; `per_state` 3).
2. If an industrial state has infra <= 2 and >= 5 planned factory levels: build infra first.
3. If PP >= 150 and Free Trade/Export Focus is not active and not locked: take it.
4. If PP >= 150 and Early/Partial mobilization is available, not active, and no focus
   forbids it: take it.
5. If any production line shows a missing resource: queue a trade request; never
   build civs for this.
6. If a fresh research slot is free: construction, machine tools, then fuel/silos.
7. If a decision with +factory or +slot is available and costs <= 50% of PP: take it.
8. If dockyards exist but no ship or convoy line is active: report; do not build more.
9. Each weekly digest: report free civs, free slots, PP, CG share.

## 13. (b) Needs a live decision

- Civ-to-mil crossover date (needs the war plan and war support).
- Focus selection where two paths are mutually exclusive (Germany Four Year Plan vs
  Economic Growth; Autarky vs Free Trade).
- Autarky/Closed Economy/Special economic law choices.
- Trade partners and convoy escorts at war.
- Dockyard and naval program size.
- Refinery vs trade for fuel (needs the real fuel curve).
- Spending PP below the 150 bank (events, advisors with big trade-offs).
- Moving factories to safer states under bombing.

## 14. (c) Tools needed

The task names tools; the checkout's registry exposes the actions below
(`hoi4_live/tools/economy_tools.py`, `politics_tools.py`). Check the registry before assuming a
name.

| Need | Named in the brief | Existing in the checkout | Enough? |
|---|---|---|---|
| Queue buildings | `hoi4_construction_add` | `hoi4_construction` actions `build`, `plan`, `queue` (state id, count, `choose`) | Yes for factories, infra, refineries. No per-state free-slot or cost read-back beyond the weekly scan. |
| Production lines | `hoi4_production_add/set` | `hoi4_production` | Yes. |
| Laws, advisors | `hoi4_politics_choose` | `hoi4_politics` | Yes if it lists law availability and cost; needs a read of war support. |
| Decisions | `hoi4_decisions_list/take` | `hoi4_decisions` | Yes. |
| Economy read | `hoi4_economy` | telemetry digest | Needs: free civs, trade civs, CG share, PP/day. |
| State read | `hoi4_states` | weekly state scan | Needs per-state free slots, infra, category, resource output. |
| Focus | `hoi4_focus_start` | `hoi4_focus` | Yes. |
| Missing | trade orders (resource purchase) | none found | Add `hoi4_trade` (list partners, buy/sell resource by civ count). |
| Missing | MIO and designer assignment | none found | Add `hoi4_mio`. |
| Missing | per-line resource shortage read | none found | Add to `hoi4_economy`. |

Version note: every number marked [W] or [?] above must be re-read on screen before it
drives a campaign decision. Wiki checked 25 Oct 2026; no wiki prose copied.
