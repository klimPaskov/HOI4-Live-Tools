# Strategic decision rules

These are operating heuristics, not a fixed patch meta. Read actual tooltips,
costs, terrain, equipment and losses before choosing a design. Numerical review
thresholds below are conservative starting policies, not engine constants.

## Economy compounds only if the army survives

Choose construction by war deadline. Civilian investment is useful only if its
remaining payback supports the intended campaign. Germany can develop early;
China and Poland facing earlier existential war usually need military output,
equipment and usable supply sooner. Prefer safe, productive states with usable
slots and good construction modifiers. Do not blindly max infrastructure in
every state; compare the time spent improving it with the factories it helps.

Prioritize bottleneck rail/port repairs and critical supply over marginal new
factories during operations. Build new hubs only when reach cannot be solved by
motorization, a usable captured hub/port or force redistribution. A rail line
alone is not a new supply source. Protect key hubs, train reserves and transport
routes. Use current supply-map bottleneck evidence instead of remembered costs.

Research industry/electronics and the equipment actually entering production.
Keep combat equipment, support and logistics coherent with war dates. Do not
research every aircraft/tank class, chase a bonus for an irrelevant technology,
or buy future superiority at the price of having no equipment at war start.
Choose the current doctrine/subdoctrine system by force and terrain; do not
assume the pre-2025 doctrine-tree interface still exists.

Spend political power on high-return laws/advisors and required political
decisions while reserving enough for a known approaching crisis. Verify current
costs and eligibility. Conscription raises a reserve only when mobilization has
time to work; do not wait for the manpower pool to hit zero. Avoid penalties
from premature over-mobilization without a real need.

## Production and shortage repair

Keep established lines efficient. Upgrade models when their benefit justifies
the conversion/efficiency cost; do not churn designs every month. Maintain
minimum gun, support, artillery, AA, truck/train and convoy output appropriate
to the force, then fund the offensive arm and air force. Count reinforcement,
upgrades, training and garrisons as competing demands.

For each important stockpile track stock, daily net change, reinforcement
deficit and production/transport constraints. If net change is negative,
`days of cover = usable stock / abs(net daily change)` is an estimate; if stock
or trend is unknown, coverage is unknown. Initial review policy: critical below
7 days, warning below 30 days; adjust to theater and observed volatility.

Repair in this order, choosing only steps that address the actual cause:

1. Stop avoidable exercises and wasteful attacks; narrow recruitment/upgrades
   that starve active formations. Preserve a fully equipped core.
2. Restore missing resources, power/energy where the installed build has it,
   transport, damaged factories or supply paths. Import useful quantities over
   routes likely to survive; reassess after supplier/sea-route changes.
3. Allocate idle factories, then move bounded capacity from genuine surplus
   lines without destroying core production. Verify resource demand after moves.
4. Use affordable in-game purchase/lend-lease/subject support if available and
   permitted by mode; delivery is not guaranteed by an accepted request.
5. Simplify nonessential template requirements or postpone expansion. A cheap,
   fully supplied army beats an impressive paper order of battle.

Never solve equipment shortage by raising manpower alone, supply shortage by
adding more divisions, or fuel shortage by building tanks faster. Avoid the
opposite mistake of halting all activity when one low-priority item is short.

## Force architecture

Maintain distinct roles: economical line holders, concentrated assault force,
mobile exploitation, port/coast guards, local reserve and occupation garrison.
Use generals within their observed command limits and assign only verified unit
subsets. Keep a theater reserve instead of filling every battle with everything.

Foot infantry emphasizes organization, HP, defense and sustainable equipment.
Support artillery/engineers/AA are useful only when the costs fit the job;
support AT is a response to visible armor, not a universal purchase. Build tank
formations with enough motorized/mechanized support, organization, HP, speed,
fuel and supply to sustain fighting. In FAIR keep line armor separate from foot
infantry/cavalry according to the house rule. In META evaluate hybrid armor by
actual partial-piercing and loss results, not a promise of invulnerability.

No universal 7/2, 14/4, 20-, 21- or 40-width recipe. Match the installed terrain
width, reinforcement behavior, intended front and equipment. Compare affordable
formations at equivalent industrial cost; a higher stat card on one enormous
division does not prove a better army. Specialized marines/mountaineers are for
the terrain that needs them, within displayed caps in FAIR.

## Land operations

An offensive needs a purpose beyond a green battle-plan arrow: secure a hub,
port, crossing, airfield, pocket or victory-point chain. Define a short axis,
holding force, breakthrough force, exploitation force and abort condition.

1. Reconnoiter with player-visible information. Locate weak frontage, terrain,
   supply and flanks; do not substitute hidden enemy division data.
2. Concentrate attack and air on a narrow useful sector. Pin adjacent defenders
   when necessary. Avoid broad attacks across rivers/mountains without a reason.
3. Break through toward reachable supply and link up the pincers. Infantry
   follows to hold the corridor; fast units do not outrun their fuel or escape
   route. Keep enough width and reserve to prevent a counter-encirclement.
4. Close the pocket, confirm enemy retreat routes are cut, then reduce it while
   protecting the outer front. Stop pointless attacks once isolation does the
   work. In FAIR do not delete own trapped units to evade a loss. META may use
   a verified normal-UI resource-recovery behavior if the installed game permits
   it; never assume a trapped-unit loophole still works.
5. Reorganize fronts after borders/ownership change. Reassign short explicit
   sectors so the planner does not shuffle the entire army. Repair supply,
   replenish equipment and move air before the next bound.

Attack only with adequate organization, equipment, supply and a viable route.
Useful initial review triggers are assault strength below 80%, organization
below half, or persistent worsening supply; use live battle trend to decide
whether to rotate, hold or withdraw. They are not universal auto-retreat rules.
A local loss can be accepted to preserve the main army or close a larger pocket.

For defense use rivers, hills, cities and a shorter line; retain reserves behind
threatened hubs/ports. Withdraw before a salient closes. Counterattack exposed
flanks or exhausted units instead of ordering a whole-front revenge offensive.
Strategic redeployment near active battles risks arriving without organization;
use it for safe rear transfers and allow recovery.

## Air and naval support

Aircraft need suitable range, base capacity, mission coverage, fuel and supply.
Concentrate fighters where the decisive ground/sea operation occurs; disperse
only to cover a necessary second task. Add CAS when it can actually reach the
battles. Check losses and mission efficiency rather than counting assigned wings.
Use ground AA when contesting the entire sky is unaffordable. Move bases with
the front and avoid crowding airfields. Radar and logistics strikes serve a
theater objective, not an automatic construction/research checklist.

Fleet plans need a role: scouting, protected strike force, escort, raiding,
mines or landing support. Check fuel, range, repair status, engagement settings,
screening and mission availability in the current UI. Do not feed obsolete
capital ships into repeated losing engagements for a supremacy number. Isolate
a theater using air/naval pressure and protect own supply routes. Build escorts,
convoys and suitable air before an expensive fleet that will finish too late.

## Naval invasion state machine

Keep `prepare -> planned -> staged -> launchable -> crossing -> landfall ->
supplied bridgehead -> reinforcement -> exploitation -> complete`, plus
`blocked/recovering`. An order drawn is not an invasion launched.

| Gate | Required observed evidence | Response if unmet |
|---|---|---|
| Plan | Tech, current plan/division capacity, assigned unit IDs, legal origin/target, planning progress | Reduce/split according to actual UI rules or research missing capability |
| Stage | Units at origin, organization/equipment, convoys, fuel and route range | Wait in server, repair shortages, protect staging port |
| Launch | Required current naval/air conditions on every traversed region, orders activated | Shift air/fleet, escort/recon, choose another viable route; never claim a launch from a click |
| Land | Troops actually ashore; port captured or a verified temporary supply plan | Secure port urgently, isolate defenders; stop feeding failed beach attacks |
| Reinforce | Usable port throughput, convoy path, reserve troops, local air | Repair port and escorts, feed only sustainable forces |
| Expand | Secure perimeter, connected supply, defended port, inland objective | Advance in bounds; landfall alone is not theater victory |

Do not assume the old global 10-division invasion cap: the official NCNS release
changed invasion capacity structure. Read current plan limits and the launch
tooltip. Air/sea threshold values, temporary harbors and transport behavior are
also build/DLC dependent. A transit route may include more regions than the
visually obvious strait.

## Occupation, peace and endgame

Supply garrisons and choose sustainable occupation policy; resistance can
destroy apparent industrial gains. Use a suitable suppression template and
check manpower/equipment delivery. Do not drain the field army unnoticed.

Before a peace conference rank required formable/core states, coherent supply
access, strategic ports/resources and remaining war goals. Inspect actual costs
and competing claims. Verify final ownership/controller separately; a battle
capture is not necessarily a permanent settlement. Track all major enemies and
remaining wars. Capitulating one country may leave its faction fighting.
