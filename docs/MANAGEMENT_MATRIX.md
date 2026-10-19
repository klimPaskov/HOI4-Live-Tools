# Management matrix: what a campaign must manage, and what the tools do today

Status words: **works** (live-verified on the owned macOS run), **partial**, **missing**.
A playbook is only as good as the tools behind it, so every gap below is a build item,
not a reason to skip the domain.

| Domain | Status | What exists | Gap to close |
| --- | --- | --- | --- |
| Research, incl. doctrine | partial | `hoi4_research_start` by key; the plan runner fills slots from a priority list | Doctrine folders not yet exercised; no year-by-year doctrine order; no check that a doctrine path is coherent |
| Special projects | missing | none | Panel reader and start/assign action |
| Espionage (agency, operatives) | missing | none | Agency upgrades, operative recruitment and missions |
| Industry: civilian/military balance | partial | construction add, free-slot reads, production lines add/set, standing-plan balancing | Consumer-goods and slot-limit checks should be automatic; conversion of factories |
| Production of naval and air equipment | partial | air lines work; naval lines refused | Naval production lines |
| Dockyards, fleets, navy missions, invasions | missing | dockyards can be queued as buildings | Fleet creation, task forces, missions, naval invasion |
| Air force | partial | wing list, mission per wing | Wings per front, air superiority before offensives, rebasing, automatic reassignment |
| Land: armies, groups, marshals | works | create, split, generals, groups, marshals (promotion), fronts, offensives, move, plan execute | Fewer, bigger armies; keep **all** divisions assigned; generals picked by trait; reserve groups |
| Encirclements | missing live | upstream search (`third_party/clausewitz_mcp/calling/doctrine.py`) is offline only | Read province ownership and unit positions live, run the adjacency test, draw the pincer lines |
| Division designer | missing | training uses templates that already exist | Template editor: width by terrain, support companies, per-role designs |
| Training and deployment | works | train with locations, deployment fix | Template-aware quotas by role |
| Diplomacy: war goals, guarantees, trade, access | partial | any listed diplomatic action toward a state's owner, war goals with states | Factions (create, invite, join), lend-lease, subjects, timing of war goals (they expire) |
| Events | works | answer by option; news windows | Per-event policy file instead of option 0 |
| Politics: laws, advisors, designers | works | choose by key | Policy by situation (war economy, conscription), advisor choice by role |
| Peace conference | missing | window is detected, nothing more | Bid actions, done button |
| Occupation and puppets | missing | none | Manage occupied territories, set up governments |

## Playbook shape (flexible, not a script)

Each playbook (generic, then a module per major country) states **goals**, **decision rules** and **fallbacks**,
never a fixed order. A turn of play is:

1. Read the state (digest, alarms, map facts, enemy strength that is visible to a human player).
2. Check the **domain checklist** above: research slots, focus, production, construction, designs,
   air, navy, intelligence, politics, diplomacy, army assignment. Anything idle is a defect.
3. Choose the next action from the rules; if a precondition fails, take the fallback (another focus
   branch, another target, a defensive posture) instead of repeating the same step.
4. Verify from the game state, log, and move on.

A country module lists its historical options (for Germany: Anschluss, Sudetenland, Poland, France, Sea Lion,
USSR) as *branches with entry conditions and exits*, so a game where Austria goes another way, Poland is guaranteed
earlier or France falls late still has a legal next step.

## Immediate build order

1. Keep every division in an army (absorb/split rules), fewer generals and groups.
2. Division designer (templates), then template-aware training.
3. Air: assign wings per front automatically.
4. Factions and diplomacy automation including war-goal timing.
5. Naval production, fleets, invasions; special projects; espionage.
6. Live encirclement search and pincer lines.
7. Peace conference.

## Current goal (set 19 October 2026)

Play m009 (FAIR Germany from 14 August 1939, recorded) to the conquest of Poland and France while closing the matrix rows in
build order: all divisions assigned, division designer, air per front, factions/war-goal timing, naval production, live
encirclement search. A row counts as closed only after a live-verified action; the game keeps running while tools are built.
