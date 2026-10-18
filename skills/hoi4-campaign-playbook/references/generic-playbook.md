# Country-agnostic campaign playbook (HOI4 1.19, FAIR)

Verified facts come from a cited deep-research run cross-checked against the
installed 1.19.3 defines, terrain and trait files; items marked *(live)* were
observed in the Mac campaign runs; items marked *(unverified)* are common
community advice. Re-check version-dependent numbers in the game.
Germany, Poland and China appear only as worked examples.

## 1. Army organisation

- A general commands **24 divisions** at full strength (30 with the Skilled
  Staffer trait, 72 on an Area Defense order). Above the cap the general's
  bonuses and XP shrink proportionally; a 43-division army showed
  "all stats reduced by 79%" *(live)*. Build armies of **24 or fewer**.
- A **field marshal** leads an **army group** of up to **5 armies** (7 with
  Expert Delegator). His skill and non-personality traits reach his armies at
  **50%**; personality traits pass down in full. A marshal who leads a single
  army directly loses his marshal traits. Make army groups, not one marshal
  per army.
- The unit-selection list cannot be range-selected with synthetic input, so
  **form armies as divisions arrive**: deploy new divisions to several border
  states and create one army per state of at most 24 divisions at once.
  Splitting an oversized army later is not possible through the tools.
- A training line without a **deployment location** never deploys its finished
  division *(live: twenty divisions waited for months)*. Set a location on
  every line (`hoi4_train` with `deploy_state_id`, or `hoi4_deploy_locations`).
- Spearheads can only be ordered to single armies, not to an army group.

## 2. Division templates (rules of thumb)

- Widths follow terrain: plains/hills/desert 70 (+35 per extra attack direction),
  forest/jungle 60 (+30), mountain/marsh 50 (+25), urban 80 (+40). Pick template
  widths that divide these, e.g. 10, 15, 20, 30, 35, 40.
- A non-HQ template takes up to **5 support companies**, one of each type; they
  add no width and do not slow the division. Engineers, recon, artillery, AA and
  AT supports are worth their slots; they change organisation, supply use and
  hardness.
- Infantry is the backbone (about 7 infantry + 2 artillery early; add
  anti-tank/AA companies as the enemy fields armour/air). Add an armoured group
  of fast, supplied divisions to break through; keep it at or below about 35 width
  for supply and organisation *(community)*.

## 3. Building up in peace (any country)

1. **Check requirements in the game files before committing.** Annexation and
   expansion focuses often gate on army strength, not only on prerequisites. In
   the 1.19.3 files the German Anschluss focus needs fighting-army strength
   above **24 times Austria's** when the Götterdämmerung DLC is active, plus
   500 000 manpower and 50 000 infantry equipment otherwise *(live, read from
   `common/national_focus`)*. A requirement can be an army-size gate, a political
   gate or a faction gate: read `available`/`bypass` first, then plan the build.
2. **Economy first, then convert.** Early civilian factories and construction
   research pay back; start shifting new construction to military factories and
   arms lines about one to two years before the first planned war, as measured
   by equipment stockpiles and the army size the next focus demands, not by a
   calendar date *(unverified)*.
3. **Never idle a slot.** Keep every research slot, the focus slot, every
   production line and the construction queue busy. Check `hoi4_economy` for
   idle civilian factories each session; if construction is refused in a state
   (no free slot), try newly gained states.
4. **Laws and advisors:** trade law and economy law first, conscription as the
   army needs; advisors are in the government slots (political advisors) and
   military staff (chiefs); designers/companies in research & production.
   Political power is cheap to bank and expensive to waste: spend it on laws,
   advisors and good decisions every session.
5. **Research by priority:** industry/construction/electronics early, then
   infantry weapons and support, artillery, radio, then doctrine and the
   armour/air line the plan needs. Later technologies sit lower in each folder
   (the tool scrolls the tree).
6. **Production:** most military factories on the current infantry equipment,
   then support, artillery, fighters and CAS; add tanks/trucks as the strike
   force grows. Use the newest design key offered (`_2`, `_3`) for new lines.
7. **Train continuously** (15–20 divisions in the queue) with locations set, and
   form armies and army groups as they deploy.

## 4. War execution

- **Planning before attack.** Planning builds 2% per day to a 30% cap (about 15
  days), decays 1%/day under a battle plan and 3%/day under manual orders, and
  starts at ×0.8 with no deployed HQ. Draw the plan (front line, then offensive
  line), let it sit about two weeks, then execute.
- **Fronts first.** Give every army a front toward the country it faces
  (`hoi4_army_front`), then offensive lines into the enemy (`hoi4_army_offensive`),
  then execute (`hoi4_army_plan`). Spread armies across the border; do not stack
  everything on one approach.
- **Rivers and terrain.** Attacking across a small river costs −30% (−25% speed),
  across a large river −60% (−50% speed); the defender never pays it. Draw the
  main thrust over open, river-free terrain.
- **Encirclement.** Attack a province from more than one side to add combat
  width; a defender whose land neighbours are all enemy-controlled takes −30%;
  cut-off divisions get no reinforcements and routed divisions with nowhere to
  retreat are destroyed. To find pockets: look for enemy provinces whose land
  neighbours are mostly yours, thin salients, necks between two advancing fronts
  and groups whose only route to the capital passes one or two provinces
  (Clausewitz-MCP's encirclement search runs the same adjacency test). Use fast
  divisions as the pincers and infantry to close and hold the ring.
- **Supply.** Land units start attrition when supply falls to about 35%; watch the
  spearhead and pause before it starves.
- **Air.** Fighters on **air superiority** over the front region first
  (`hoi4_air_mission`), then CAS over the decisive battles (CAS adds a combat
  bonus scaled by the planes present). Keep wings inside range of the target
  region; rebase as the front moves.
- **Naval invasion (unverified numbers).** The AI's 4-division order limit and
  supremacy/intel requirements were not confirmed; read the in-game tooltip
  before launching, and secure ports, escorts and air cover first.
- **Orders versus manual control.** Prefer battle-plan execution to micromanaging;
  manual orders drain planning three times faster.

## 5. Peace conference (reworked blind-bid system, 1.12+)

All victors bid simultaneously each turn; states bid on by several victors are
contested, refunded and dearer next turn; each victor's point pool scales with
war participation (damage and territory taken), paid in 20% steps over five
turns when several victors share. Bid first on your core targets that allies do
not want, avoid repeated contests that raise the price, and finish with Done.
*(live: the conference window has no tool support yet; stop and hand back.)*

## 6. Common mistakes

- Training divisions with no deployment location; leaving armies above 24
  divisions; one army group for everything; field marshals not assigned.
- Letting research slots, the focus slot or construction sit empty for months.
- Banking political power; ignoring idle civilian factories.
- Declaring war without fronts drawn and armies on the border.
- Attacking across large rivers; stacking one approach; chasing victory points
  with unsupported tanks; ignoring supply.
- Skipping events: an open event window blocks all other input.

## 7. Sources

HOI4 wiki pages Commanders, Battle plan, Land battle, Terrain, Logistics,
Support company, Air missions, Peace conference (tagged 1.9–1.16; numbers were
checked against the installed 1.19.3.0 `00_defines.lua`, terrain and unit-leader
trait files); the 2022 peace-conference dev diary; community guides for templates;
smerss/Clausewitz-MCP for the encirclement search.
Not covered by verified research and left as game-file or live observations:
focus order, laws/advisors, research order by year, civilian-to-military timing,
naval invasion and air-superiority values.
