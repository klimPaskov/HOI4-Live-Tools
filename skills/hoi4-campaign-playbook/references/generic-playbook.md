# Country-agnostic campaign playbook (FAIR)

Germany, Poland and China appear only as worked examples.

## 1. Army organisation

- A general commands **24 divisions** at full strength (30 with the Skilled
  Staffer trait, 72 on an Area Defense order). Above the cap the general's
  bonuses and XP shrink proportionally; a 43-division army shows
  "all stats reduced by 79%". Build armies of **24 or fewer**.
- A **field marshal** leads an **army group** of up to **5 armies** (7 with
  Expert Delegator). His traits reach every army in full, his skills at
  **50%**. A marshal who leads a single army directly loses his marshal traits.
  Make army groups, not one marshal per army.
- **Form armies as divisions arrive**, at most 24 each: deploy new divisions to
  several border states and create one army per state. An oversized army can be
  split with `hoi4_army_split` (first N divisions, N up to 19, into a new army).
  A stack of unassigned divisions in one province becomes one big army with
  `hoi4_army_create`; peel 19 at a time off it.
- A training line without a **deployment location** never deploys its finished
  division. Set a location on every line (`hoi4_train` with `deploy_state_id`,
  or `hoi4_deploy_locations`).
- Spearheads can only be ordered to single armies, not to an army group.

## 2. Division templates (rules of thumb)

- Widths follow terrain: plains/hills/desert 70 (+35 per extra attack direction),
  forest/jungle 60 (+30), mountain/marsh 50 (+25), urban 80 (+40). Line divisions
  14-18 wide for mountains and hills, 20 elsewhere; avoid 34-36 and 40+ outside
  plains.
- A non-HQ template takes up to **5 support companies**, one of each type; they
  add no width and do not slow the division. Engineers, recon, artillery, AA and
  AT supports are worth their slots; they change organisation, supply use and
  hardness.
- Infantry is the backbone (about 7 infantry + 2 artillery early; add anti-tank
  as the enemy fields armour). Division AA is all or nothing: about 55 air attack
  gives the full 75% cut to CAS damage, token AA does little. Add an armoured group
  of fast, supplied divisions to break through; keep it at 30 width or less for
  supply and organisation.

## 3. Building up in peace (any country)

1. **Check requirements before committing.** Annexation and expansion focuses
   often gate on army strength, not only on prerequisites. The German Anschluss
   focus needs fighting-army strength above **24 times Austria's** with the
   Götterdämmerung DLC, otherwise 500 000 manpower and 50 000 infantry equipment.
   A requirement can be an army-size gate, a political gate or a faction gate: read
   the focus's availability and bypass conditions first, then plan the build.
2. **Economy first, then convert.** Early civilian factories and construction
   research pay back; switch new construction to military factories about two to
   three years before the first planned war (switch date = war date - 2.9 x
   civilian payback; civilian factories queued in the last 18 months never repay),
   checked against equipment stockpiles and the army size the next focus demands.
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
8. **Generic-tree research slot.** `extra_tech_slot` needs `industrial_effort`,
   `construction_effort` I-II and `infrastructure_effort` I-II first (420 days,
   about late February 1937); construction and armament efforts are bypassed
   when no state has a free slot; `extra_tech_slot_2` needs more than 50
   factories.
9. **Minor fort focuses.** Instant-bunker focuses are a minor's cheapest
   defence: take them in the year before the expected attack, on river or
   mountain lines, with anti-air, because combat and bombing wear forts down.

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
  (`hoi4_encirclement_candidates` runs this test). Use fast
  divisions as the pincers and infantry to close and hold the ring.
- **Supply.** Attrition starts below 35% supply; no new offensive below 50%,
  resume at 70%. Watch the spearhead and pause before it starves.
- **Air.** Fighters on **air superiority** over the front region first
  (`hoi4_air_mission`), then CAS over the decisive battles (CAS adds a combat
  bonus scaled by the planes present). Keep wings inside range of the target
  region; rebase as the front moves (`hoi4_air_rebase`).
- **Naval invasion.** Capacity by tech (divisions / plans / prep days): base
  4/1/60, transport 6/2/50, landing craft 10/5/40, tank landing craft 15/9/30.
  Launch needs sea control (60% of the needed dominance) in every crossed region and 40% naval intel at that
  moment. Secure ports, escorts and air cover first (`hoi4_naval_invasion`).
- **Orders versus manual control.** Prefer battle-plan execution to micromanaging;
  manual orders drain planning three times faster.

## 5. Peace conference (blind bids)

All victors bid simultaneously each turn; states bid on by several victors are
contested, refunded and dearer next turn; each victor's point pool scales with
war participation (damage and territory taken), paid in 20% steps over five
turns when several victors share. Bid first on your core targets that allies do
not want, avoid repeated contests that raise the price, and finish with Done.
Tools: `hoi4_peace_status`, `hoi4_peace_options`, `hoi4_peace_take`,
`hoi4_peace_submit`, `hoi4_peace_exit`.

## 6. Common mistakes

- Training divisions with no deployment location; leaving armies above 24
  divisions; one army group for everything; field marshals not assigned.
- Letting research slots, the focus slot or construction sit empty for months.
- Banking political power; ignoring idle civilian factories.
- Declaring war without fronts drawn and armies on the border.
- Attacking across large rivers; stacking one approach; chasing victory points
  with unsupported tanks; ignoring supply.
- Skipping events: an open event window blocks all other input.

## 7. Practical rules

- **Front lines need a foothold.** A front toward a country is only drawn from armies that already stand on the
  border or inside the enemy state; from two states back the game highlights nothing. Move the army into the enemy
  state first (`hoi4_army_move`), then draw front, offensive line and execute.
- **A Conquer war goal expires 60 days after completion** (claimed and core goals never expire). Watch it every
  few game days and declare at once; do not leave the game running unattended at speed 2–3 across that moment.
- **Armies:** keep every division in an army (organise after every deployment), at most 24 each; do not fold standing
  armies into a new one unless that is intended, it dissolves their orders.
- **Generals:** only armies of about ten divisions or more get one; small armies are reserves.
- **Pop-ups cover the panels:** doctrine-milestone and capitulation windows must be acknowledged or army actions
  silently fail.
- **Maginot Line:** the Lorraine front grinds; push through Belgium/the Ardennes or the south, with air superiority first.
- **Air wings need missions:** assign fighters to air superiority and bombers to CAS over the active front in range
  of their bases; rebase with `hoi4_air_rebase` as the front moves.
