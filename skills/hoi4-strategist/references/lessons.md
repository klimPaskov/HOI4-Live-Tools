# Lessons from failed runs

`skills/hoi4-campaign-playbook/references/lessons.jsonl` holds one JSON object per line:

| Field | Meaning |
|---|---|
| `id` | stable lesson id; cite it in plan.md |
| `failure` | what went wrong in a live run |
| `runs` | the runs where it happened |
| `prevention` | what a plan must contain so it does not happen again |
| `lint_rule` | the `lint_plan.py` rule that checks it, or null when only the critic can |

Read it at step 4 (before the skeleton):

```
python3 -c "import json,sys;[print(o['id'],'|',o['failure'],'|',o['prevention'],'|',o.get('lint_rule')) for o in map(json.loads,filter(str.strip,open(sys.argv[1])))]" skills/hoi4-campaign-playbook/references/lessons.jsonl
```

How to use it:
* Decide for each lesson whether it applies to this goal (a land war, a defensive goal, a peaceful
  build-up). Most land-war lessons apply to every land war, not only to the scenario they came from.
* A lesson with a `lint_rule` is covered once lint is clean. A lesson without one needs a plan item: a
  guard, a contingency, a checkpoint or a do-not line. Name the lesson id next to it in plan.md §8, §9
  or §10.
* The critic checks applicable lessons under rubric rows 8 and 9. A missed one that applies is a
  `major` finding, a `blocker` when it decides the goal (for example an unguarded border).
* Never weaken a prevention to make a schedule fit; change the schedule.

## Why each lesson matters

These are the failures the lessons record (design section 4.4). The ids are in the file.

| Failure | Why it matters for a plan |
|---|---|
| Encircled or cut off behind the front: border states lost once their armies left; provinces retaken behind the strike army | The executor moves strike armies toward targets and leaves what is not guarded. Every border state needs a guard role, guards are never removed while an enemy borders the state, and a corridor rule keeps the line of supply held. |
| A scripted enemy offensive at the start took every border province | Scenario hooks give the enemy timed bonuses. The dossier lists them; the first phase holds with guards on every border province until the bonus ends. |
| Building on fronts or in low infrastructure | Factories near the front are lost or slow; low infrastructure multiplies build time. Construction lint checks front distance and infrastructure. |
| Random far-future research (ahead-of-time techs, legacy keys, a non-existent key) | An open fallback fills slots with useless techs at a large penalty. `research_fallback` off or restricted; `research_path` validated. |
| Wrong win condition (a province instead of the mandatory state; wrong mission timing) | The run can "win" the war and still lose the scenario. Goal predicates come from the hooks with sources; critic row 1. |
| Supply starvation at the front (single-digit supply, hubs queued behind factories) | Armies without supply lose organisation and stall. Hubs and railways go to the top of `construction_plan` with capture triggers, `supply_first`, `supply_floor`, motorisation from day one, strike army size capped by supply. |
| Armies scattered by stall moves (one army over seven states) | Scattered armies cannot take provinces. Concentrate in the layout, give each army its own `vp_targets`, keep the executor's stall backoff. |
| Duplicate and failing actions repeated | Wrong keys make the executor retry until its failure ledger stops it. Use schema keys only and stable ids. |
| Player too slow (one turn spans 10-30 game days) | Anything left to the player happens late. Give the executor every decision it can carry; keep the brief short and procedural. |
| Mission timers ignored | A mission that times out can fail the goal or add penalties. Missions with timers from the dossier become checkpoints at their deadlines. |
| No equipment at the front | Divisions without equipment do not fight. Disband far divisions early, size production with `calc equipment`, guard training on stockpile. |
