# Critic rubric and answer protocol (step 8)

The critic runs in a fresh context. It gets `RUN/run_spec.json`, `RUN/dossier/dossier.md` (and the facts
CLI), `RUN/plan.json`, `RUN/plan.md`, `RUN/plan.brief.md`, `RUN/lint.json`, this rubric and, in round 2,
the previous round's findings and answers. It does not get the strategist's reasoning or output log.
It may run any read-only `facts.py` / `calc.py` command and `lint_plan.py` to check a claim, with the
same `--mode` (the FAIR boundary in SKILL.md applies to the critic too). It never edits the plan.

## 1. Scores

Each row scores 0, 1 or 2 (16 rows, maximum 32). **Pass: total ≥ 22 of 32 and no 0 in rows 1, 2, 7 or
13.** A score needs evidence: a plan item, a facts or lint output, or a dossier line. "Looks fine" is
not evidence. Rows 12-16 check the checklist items of `procedure.md` that rows 1-11 do not; the
coverage table (plan.md §25) is the starting point for them and for row 10. A goal without a land war
scores row 13 on its doctrines alone; a row whose whole subject is not applicable with a cited
reason scores 2.

| # | Row | 2 | 1 | 0 |
|---|---|---|---|---|
| 1 | Goal fidelity | predicates match the user's words and clarifications; in-game formalisation used when one exists (sources cited); alternatives stated with reasons; deadline equals the scenario timer | right target but a missing mandatory rule, a weak alternatives list, or a deadline without source | predicates test something else than the user asked, or ids not in the dossier |
| 2 | Grounding | every checked number is tagged and matches the facts output; [H] share ≤ 30%; FAIR boundary kept | a few untagged or [H] numbers that could be [V]; no contradiction | any fact contradicted by the dossier or facts CLI, an invented id, or private enemy facts used in FAIR |
| 3 | Timing realism | focus, research and construction dates in plan.md equal `focus-timeline`, `research-timeline` and `calc build-order` output | small drift (under ~10 days) or one domain not run through its timeline | dates that the timelines contradict by weeks, or a focus or tech planned before it can start |
| 4 | Economy | civ/mil timing fits the months to war (economy-build-orders §1.2); factory states safe (lint clean); no idle-factory gap | right shape, one gap or one questionable state | builds on fronts or in low infrastructure, or no build order |
| 5 | Research | industry and construction first; nothing past `start_year`; block sizes fit the slots; fallback off or restricted | one block too long or one off-goal tech | ahead-of-time techs, missing prerequisites, or open fallback |
| 6 | Production and army | `calc equipment` shows lines cover templates × divisions + reinforcement; trucks with motorisation; disband plan; layout templates and counts exist | coverage short in one equipment or no disband plan | lines exceed factories, equipment not unlocked, or layout names not in the OOB |
| 7 | Conquest | VP order by value and adjacency to the threshold (with and without modifiers); mandatory states included; routes and river counts given; supply construction triggered along the axis | order works but one leg unjustified or supply follows late | threshold not reached, mandatory state missing, or targets not reachable |
| 8 | Defence | every enemy-bordering own state guarded; corridor rule when targets go beyond 2 provinces; encirclement risks named with a response | one border or corridor thin | an enemy-bordering state without a guard, or guards removed during a phase |
| 9 | Contingencies | each likely failure (lessons that apply, scenario escalations, stalls, core loss, new war) has a trigger predicate and a phase / overrides / revise response; checkpoints at mission deadlines | some failures covered only in text | none, or triggers the executor cannot evaluate and no player fallback |
| 10 | Executor readiness | most decisions in executor-owned keys; player items name exact tool calls; ids stable; schema and lint clean; the coverage table resolves all 29 items (executor key, player call, or not applicable with a cited reason) | some player text that an executor key could carry, or one item resolved without a fact | key decisions only in prose, player items without calls, or checklist items missing from the coverage table |
| 11 | Brief usefulness | ≤ 60 lines; win check, next 3 checkpoints, what the executor does, player calls, judgement events with predicates, do-not list, `plan_revised` step; actionable in one batch per turn | complete but long-winded or one part missing | over 60 lines, or the player cannot tell what to do |
| 12 | Force posture and XP budget | land / air / naval shares by phase follow the goal and the enemy aggregates; production, research folders and doctrine folders match them; XP forecast per branch with every spend dated, no month over the forecast, no pool left at the cap | posture stated but one arm unjustified, or the XP schedule not checked against the forecast | no posture, or planned XP spends the forecast cannot pay before they matter |
| 13 | Templates and doctrines | a template per role and phase with width from the theatre's terrain mix (within 10% of a clean fit), support and regimental support justified, priority and XP cost given, equipment unlocked and covered, fielded divisions' switch dated; doctrine per folder and track order from the dossier, no grand-doctrine change without a reason that beats the mastery reset | templates exist but one role is missing or the width is not derived from terrain | a land war with no template decision, widths that do not fit the main terrain, or doctrines not in the dossier |
| 14 | Air, naval and overseas supply | air plan (wings, bases with capacity, regions, missions by phase, superiority before offensives) or not applicable with a reason; naval plan or "no navy" with a reason; every overseas theatre has ports, convoys, escort and sea transfers | one of these thin (a base over capacity, convoys not sized) | an overseas theatre without a convoy and port plan, or an air force with no plan |
| 15 | Occupation, peace, intelligence | occupation law per phase with resistance and compliance switch rules and a garrison template; peace demands ranked per enemy with the answer to scripted peace offers; never-take list; intelligence decided (agency and upgrades, or not applicable) | one of these missing | a conquest with no occupation item, or an enemy with no peace item |
| 16 | Continuous-management thresholds | every rule of procedure item 28 has a campaign value or "default" with a reason; rules the executor does not watch yet are brief lines with read, threshold and call | some rules left at default without a reason | no thresholds decided (`alarms` untouched and no `manage` decisions) |

## 2. Output format

The critic writes `RUN/review/critic-<n>.json` (n = 1 + the number of existing `critic-*.json` files):

```json
{
  "round": 1,
  "scores": {"1": 2, "2": 1, "3": 2, "4": 1, "5": 2, "6": 1, "7": 2, "8": 2, "9": 1, "10": 2, "11": 2,
             "12": 1, "13": 2, "14": 1, "15": 2, "16": 1},
  "total": 25,
  "max": 32,
  "pass": true,
  "findings": [
    {
      "id": "F1",
      "severity": "blocker",
      "row": 7,
      "claim": "cumulative VP in vp_targets is 24, below the threshold 26",
      "evidence": "facts.py vp <ENEMY>: threshold 26 [src]; plan.json vp_targets sum 24",
      "fix": "add the next VP province on the route after target 12, or show the modifier reaching 21"
    }
  ],
  "summary": "one paragraph"
}
```

`max` is 32 (2 points × 16 rows); `scripts/strategist/strategize.py` reads `total`, `max`, `scores` and
the open blockers.

Severities:
* `blocker`: the plan cannot reach the goal or breaks a hard rule (wrong goal, contradicted fact, FAIR
  breach, threshold not reached, unguarded border, lint error missed, a checklist item with no
  resolution, an overseas theatre with no convoy plan). Any open blocker fails the gate.
* `major`: likely to cost the goal or weeks of game time (timing off, supply late, a likely failure
  without a contingency).
* `minor`: clarity, tagging or efficiency.

Each finding has one claim, the evidence that shows it (command output or file line), and a concrete
fix. No finding without evidence. In round 2 the critic re-checks only the open findings and the parts
the answers changed, and keeps the ids.

## 3. Answer protocol (strategist)

1. For each finding write an entry in `RUN/review/answers-<n>.json`:
   `{"id": "F1", "action": "fixed" | "rebutted", "change": "what changed in which file", "source": "facts output or file:line for a rebuttal"}`.
   A rebuttal needs a source; "I disagree" is not an answer.
2. Re-run step 7 (validate and lint) after the fixes.
3. Run the critic for round 2 with the same command. It reads the answers.
4. At most 2 rounds. Gate: no open blocker, total ≥ 22 of 32, no 0 in rows 1, 2, 7, 13. If round 2
   fails, write `RUN/strategist-blocked.md` (failed rows, open findings, what would fix them) and stop;
   do not run a third round and do not freeze.
5. List every finding and its answer in plan.md §11.
