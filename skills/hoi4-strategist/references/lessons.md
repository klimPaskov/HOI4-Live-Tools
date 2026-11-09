# Lessons from failed runs

`skills/hoi4-campaign-playbook/references/lessons.jsonl` holds one JSON object per line:

| Field | Meaning |
|---|---|
| `id` | stable lesson id; cite it in plan.md |
| `failure` | what went wrong in a live run |
| `runs` | the runs where it happened |
| `prevention` | what a plan must contain so it does not happen again |
| `lint_rule` | the `lint_plan.py` rules that check it (empty when only the critic or the executor can) |
| `executor_rule` | the executor standing rules that enforce it at run time (optional; empty when none) |

Read it at step 4 (before the skeleton):

```
python3 -c "import json,sys;[print(o['id'],'|',o['failure'],'|',o['prevention'],'|',o.get('lint_rule'),'|',o.get('executor_rule')) for o in map(json.loads,filter(str.strip,open(sys.argv[1])))]" skills/hoi4-campaign-playbook/references/lessons.jsonl
```

How to use it:
* Decide for each lesson whether it applies to this goal (a land war, a defensive goal, a peaceful
  build-up). Most land-war lessons apply to every land war, not only to the scenario they came from.
* A lesson with a `lint_rule` is covered once lint is clean (a rule the installed lint does not list yet
  counts as absent). A lesson without one needs a plan item: a guard, a contingency, a checkpoint, a
  threshold or a do-not line. An `executor_rule` runs only once the executor has it and the plan sets its
  key (check the schema; `war.supply_floor` exists today); until then the rule is a brief line with the
  read, the threshold and the call, and plan.md §24 gives the value. Name the lesson id next to it in
  plan.md §8, §9 or §10.
* The critic checks applicable lessons under rubric rows 8 and 9. A missed one that applies is a
  `major` finding, a `blocker` when it decides the goal (for example an unguarded border).
* Never weaken a prevention to make a schedule fit; change the schedule.
* Each lesson belongs to a checklist item of [procedure.md](procedure.md); its prevention is part of
  that item's resolution:

| Lesson | Checklist item |
|---|---|
| `L01_cut_off_behind_front`, `L02_christmas_offensive` | 16 army organisation (guards), 18 operations (corridor, hold targets), 27 contingencies |
| `L03_build_on_front` | 10 economy timing |
| `L04_far_future_research` | 11 research |
| `L05_wrong_win_condition` | 1 goal |
| `L06_supply_starvation` | 19 supply and logistics, 8 production (trucks) |
| `L07_scattered_armies` | 16 army organisation, 18 operations |
| `L08_repeated_failing_actions` | resolution rule (schema keys only), 26 events and decisions |
| `L09_player_too_slow` | resolution rule (executor keys first, exact calls), 28 continuous management |
| `L10_mission_timers_ignored` | 26 events and decisions, 27 checkpoints |
| `L11_no_equipment_at_front` | 8 production, 6 templates, 16 army organisation (disband) |
| `L12_early_hub_cost` | 10 economy timing (cost and finish date), 19 supply and logistics (supply order) |
| `L13_planning_burned` | 18 operations (planning wait, stall step), 16 army organisation (HQ) |
| `L14_supply_floor_too_low`, `L16_no_bound_after_hub` | 18 operations, 19 supply and logistics, 28 continuous management |
| `L15_pocket_fallback_late` | 18 operations (pocket step), 16 army organisation (guards), 28 continuous management |
| `L17_mature_line_switched`, `L18_unused_start_lines`, `L19_day0_grace_missed`, `L20_factory_dump` | 7 equipment designs, 8 production |
| `L21_width_against_terrain` | 6 templates |
| `L22_unneeded_conscription` | 13 gated laws, 17 manpower |
| `L23_invasion_prep_default` | 21 naval plan |

## Why each lesson matters

These are the failures the lessons record (design section 4.4). The ids are in the file.

| Failure | Why it matters for a plan |
|---|---|
| Encircled or cut off behind the front: border states lost once their armies left; provinces retaken behind the strike army | The executor moves strike armies toward targets and leaves what is not guarded. Every border state needs a guard role, guards are never removed while an enemy borders the state, and a corridor rule keeps the line of supply held. |
| A scripted enemy offensive at the start took every border province | Scenario hooks give the enemy timed bonuses. The dossier lists them; the first phase holds with guards on every border province until the bonus ends. |
| Building on fronts or in low infrastructure | Factories near the front are lost or slow; low infrastructure multiplies build time. Construction lint checks front distance and infrastructure. |
| Random far-future research (ahead-of-time techs, legacy keys, a non-existent key) | An open fallback fills slots with useless techs at a large penalty. `research_fallback` off or restricted; `research_path` validated. |
| Wrong win condition (a province instead of the mandatory state; wrong mission timing) | The run can "win" the war and still lose the scenario. Goal predicates come from the hooks with sources; critic row 1. |
| Supply starvation at the front (single-digit supply, hubs queued behind factories) | Armies without supply lose organisation and stall. Railway levels go to the top of `construction_plan` with capture triggers, then a port where no supply reaches the shore, motorisation and trucks from day one, `supply_first`, `supply_floor`, strike army size capped by supply. |
| Armies scattered by stall moves (one army over seven states) | Scattered armies cannot take provinces. Concentrate in the layout, give each army its own `vp_targets`, keep the executor's stall backoff. |
| Duplicate and failing actions repeated | Wrong keys make the executor retry until its failure ledger stops it. Use schema keys only and stable ids. |
| Player too slow (one turn spans 10-30 game days) | Anything left to the player happens late. Give the executor every decision it can carry; keep the brief short and procedural. |
| Mission timers ignored | A mission that times out can fail the goal or add penalties. Missions with timers from the dossier become checkpoints at their deadlines. |
| No equipment at the front | Divisions without equipment do not fight. Disband far divisions early, size production with `calc equipment`, guard training on stockpile. |
| Offensives without planning; stall moves that burn it | Planning adds up to 30% attack and breakthrough; executing the tick after drawing wastes it, and blind moves lose it and the flanks. |
| Attacking below 50% supply; pushing past a fresh hub; pocket fall-back at 70% | Attrition starts at 35% and a captured railway is dead for 10 days; a pocket closes before 70% hostile share. Floors and bounds stop the bleed. |
| Mature lines switched at once; unused start lines kept; day-0 factories on new lines; every idle factory dumped on one line | Efficiency is per factory: a tier switch keeps 30%, a moved or new factory starts at 10%; dead lines waste 4.5 IC a day per factory. |
| Widths that ignore terrain; conscription the plan does not need; invasions on a 14-day prep | Width decides how much of an army fights; each conscription step costs factory output; landings fail when caps and prep come from guesses. |
| Day-1 or wartime supply hubs (20000 points) that cannot finish in a short war | A hub takes about 190-270 days with 15 civilian factories and blocks the factory build-up meanwhile. Railways first, then ports, then trucks; a hub only when `calc build-order` shows it finishes before it is needed, with its cost and finish date in plan.md §4. |
