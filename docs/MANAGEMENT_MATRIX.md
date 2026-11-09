# Management matrix: what a campaign must manage, and what the tools do today

Refreshed 9 November 2026 from the coverage audit of that day (193 decisions in 17 domains, 83 of them needed to
win a 1936 land war; HOI4 1.19.4.1 macOS with 10 DLCs). Status words: **works** (the executor carries it with
live-verified tools), **partial** (tools live, but the executor or the plan does not carry all of it), **missing**
(no tool, or a tool nobody drives). Evidence per tool: [live verification](LIVE_VERIFICATION_1_19_4.md). Build
items (W1-n wave 1, W2-n wave 2, W3-n wave 3) are named in the gap column. A row closes only after a
live-verified action; offline tests alone do not establish live support.

The strategist skill (`skills/hoi4-strategist/`) now has to decide every row below for every campaign: its
29-item checklist ends each decision in an executor key, a player call, or "not applicable" with a reason.

| Domain | Status | What exists | Gap to close |
| --- | --- | --- | --- |
| Goal, phases, scenario mechanics | partial | `goal`, `phases`, `checkpoints`, `contingencies` in the plan schema; `stop_on_date`; events answered by index or as news | The executor evaluates no goal, phase, checkpoint or contingency (W1-3); events by option name (W1-5); script variables, mission timers and never-take decisions (W1-18) |
| Economy and construction | partial | Construction lists and `construction_plan` with triggers, in peace and at war; supply lines first at war; `hoi4_railway_build` and `hoi4_construction_order` live (6 Nov); auto-import and trade tools live (31 Oct) | Province buildings (port, bunker, hub) live test pending; per-job dates; auto-import rule on deficits (W1-12); International Market (W3-3); repair priority and factory conversion (W3-13) |
| Production, designers, MIOs | partial | Lines added by plan and the idle-factory rebalance; tank, plane and ship designs live; MIO list, details and traits live (6 Nov) | Line removal (`hoi4_production_set` with 0 factories) not live-verified and not in the executor (W1-9); no stockpile read (W1-10); designs not driven by the plan (W2-2); MIO traits by plan (W2-8); line MIO chooser and MIO policy (W3-1) |
| Research and special projects | works | Research by year with only startable techs, industry first; special-project step | Research bonuses and the MIO design team (W2-6); scientist rewards (W3-13) |
| Doctrines | partial | Executor adopts doctrines in order when XP allows; land adoption live (1-2 Nov) | Air and naval adoption never verified (W2-14); the XP budget across doctrines, templates and designs is a strategist item only |
| Army forces: templates, training, manpower | partial | Training of one template with a location; disband live (5 Nov); template create and edit live (31 Oct) | No template step in the executor and no regimental-support row in the tools (W1-6); multi-template training with a stockpile guard (W1-4); manpower and conscription rule (W1-4); army exercises (W3-7) |
| Army command | works | Every division in an army of at most 24; strike and guard layout; generals once per army; army groups and field-marshal promotion live (1-2 Nov) | Groups and marshals by plan (W2-1); leader traits, command power, officer corps, recruiting generals (W3-6) |
| Land operations | partial | Fronts, offensives, VP targets, rest and resume, stall moves, pocket avoidance, retaking own provinces; `hoi4_army_attack` live (8 Nov) | Defence lines, fallback and area defence (W1-15); encirclement offensives (W1-16); sea transfer readback (W1-14); naval invasion by plan (W2-11); spearheads (W3-5) |
| Supply and logistics | partial | Railways in `construction_plan`, supply floor, motorisation live (5 Nov) | Ports and convoys for overseas theatres (W1-14); truck and fuel rules (W1-10); supply order railways, ports, trucks, hubs last (strategist rule, 9 Nov) |
| Air | partial | War missions (CAS, air superiority) over the first hostile border state; region readback live (8 Nov) | `air_plan` bases, regions and switch triggers (W1-13); bombing, naval and port strikes, superiority rule (W2-5); carriers and wing settings (W3-8) |
| Navy and convoys | missing | Fleet create, split, merge, mission, region and admiral tools live (31 Oct) | No navy key or executor step (W2-4); convoy need and losses unmapped (W1-14, W3-9); repair and engagement settings (W3-9) |
| Politics | partial | Laws and advisors by PP and cost; focus path; always-good decisions | Gated laws (`politics[].when`) and guarded, targeted and never-take decisions (W1-4); stability and war-support rule; balance of power read and predicates (W1-17) |
| Diplomacy and factions | partial | Justify and declare live (1-2 Nov); faction creation path (6 Nov, round notes) | Diplomacy by plan with the widening check (W2-3); faction by plan (W2-12); incoming proposals and automatic responses (W3-4); lend-lease, volunteers, licences (W3-2) |
| Occupation and resistance | partial | `hoi4_occupation` read and `hoi4_occupation_law` live (5 Nov) | Occupation step with resistance and compliance thresholds, garrison template action (W1-7) |
| Intelligence | partial | Agency creation and upgrades, operative recruit and assign live (1-6 Nov) | Agency upgrade order by plan (W2-7); operations, decryption, intel ledger (W3-11) |
| Peace conference | partial | Take, submit and exit live (Luxembourg 1-2 Nov; Albania 6 Nov in round notes) | The executor stops at an open conference (W1-8); other demands and blind-bid rounds (W3-12) |
| Continuous signals | partial | Alarm table (idle slots, factories, PP, unassigned divisions, events); failure ledger; `hoi4_alerts` read live (5 Nov) | Game alerts mapped to fixes (W1-11); equipment fill, stockpile, manpower, convoy, stability, fuel and casualty rules with plan thresholds (`manage`, W1-2, W1-10) |

## Playbook shape (flexible, not a script)

Each playbook (generic, then a module per major country) states **goals**, **decision rules** and **fallbacks**,
never a fixed order. A turn of play is:

1. Read the state (digest, alarms, game alerts, map facts, enemy strength that is visible to a human player).
2. Check the **domain checklist** above: research slots, focus, production, construction, designs,
   air, navy, intelligence, politics, diplomacy, occupation, army assignment. Anything idle is a defect.
3. Choose the next action from the rules; if a precondition fails, take the fallback (another focus
   branch, another target, a defensive posture) instead of repeating the same step.
4. Verify from the game state, log, and move on.

A country module lists its historical options (for Germany: Anschluss, Sudetenland, Poland, France, Sea Lion,
USSR) as *branches with entry conditions and exits*, so a game where Austria goes another way, Poland is guaranteed
earlier or France falls late still has a legal next step.

## Build order (waves)

1. Wave 1: the strategist checklist and schema keys (W1-1, W1-2); predicates, goal, phases, checkpoints and
   contingencies in the executor (W1-3); gated laws and decisions, events by name (W1-4, W1-5); templates,
   occupation and peace steps (W1-6 to W1-8); production removal, stockpile read, alerts, auto-import (W1-9 to
   W1-12); air plan, sea transfer and convoys, defence and encirclement (W1-13 to W1-16); balance of power,
   scenario variables and the P0 fact readers (W1-17 to W1-19); live verification of the open P0 tools (W1-20).
2. Wave 2: army groups, designs, diplomacy, navy, air strikes, research bonuses, agency, MIO traits, casualty
   rule, trade, naval invasion, factions, the P1 fact readers, air and naval doctrines.
3. Wave 3: tools that do not exist yet (MIO line chooser and policy, lend-lease and volunteers, International
   Market, incoming proposals, spearheads, officer corps and command power, exercises, carriers, naval settings,
   justification timer and war score, operations, other peace demands, theatres and the rest).

## Continuation checkpoint (21 October 2026, history)

m009 is no longer active. The later m010 FAIR Germany run from 1936 also failed:
the government changed, divisions disappeared and Poland remained unconquered.
The cause is unresolved, and no full campaign or clean two-year acceptance is
claimed. Preserve the recorded failures.

Before another campaign gate, finish recovery that preserves uncertain input
and returns unresolved decisions to the player. The journal and plan-runner
regressions now have offline coverage; automatic recovery of additive actions
still needs durable original baselines and live verification. Continue the
Linux-first work and the domain gaps above under the
[current handoff](LINUX_RUST_HANDOFF.md).
