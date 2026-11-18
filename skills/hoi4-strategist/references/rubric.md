# Critic rubric and answer protocol (step 8)

The critic runs in a fresh context. It gets `RUN/run_spec.json`, `RUN/dossier/dossier.md` (and the facts
CLI), `RUN/plan.json`, `RUN/plan.md`, `RUN/plan.brief.md`, `RUN/lint.json`, this rubric and, in round 2,
the previous round's findings and answers. It does not get the strategist's reasoning or output log.
It may run any read-only `facts.py` / `calc.py` command, `lint_plan.py` and `check_calls.py RUN` to check
a claim, with the same `--mode` (the FAIR boundary in SKILL.md applies to the critic too). It never edits the plan.

## 1. Scores

Each row scores 0, 1 or 2 (17 rows, maximum 34). **Pass: total ≥ 24 of 34 and no 0 in rows 1, 2, 7, 13 or
17.** The standard is dominance: the game's AI is a sanity floor, and a plan that only matches it, plans a
war it cannot win fast, or doubts or scales down a goal fails row 17. A score needs evidence: a plan item, a facts or lint output, or a dossier line. "Looks fine" is
not evidence. Rows 12-16 check the checklist items of `procedure.md` that rows 1-11 do not; the
coverage table (plan.md §25) is the starting point for them and for row 10. A goal without a land war
scores row 13 on its doctrines alone; a row whose whole subject is not applicable with a cited
reason scores 2.

| # | Row | 2 | 1 | 0 |
|---|---|---|---|---|
| 1 | Goal fidelity | predicates match the user's words and clarifications; in-game formalisation used when one exists (sources cited); alternatives stated with reasons; deadline equals the scenario timer | right target but a missing mandatory rule, a weak alternatives list, or a deadline without source | predicates test something else than the user asked, or ids not in the dossier |
| 2 | Grounding | every checked number is tagged and matches the facts output; [H] share ≤ 30%; FAIR boundary kept; every force, economy, air and political decision names the playbook row, research row or guide section it follows, or why it departs (lint `playbook.refs`), and the cited row says what the decision does | a few untagged or [H] numbers that could be [V]; a few decisions without a source; no contradiction | any fact contradicted by the dossier or facts CLI, an invented id, private enemy facts used in FAIR, or a decision that contradicts or ignores the playbook (a row the plan's situation matches) without a written reason |
| 3 | Timing realism | focus, research and construction dates in plan.md equal `focus-timeline`, `research-timeline` (both in `RUN/plan.timelines.md`, written by the gates) and `calc build-order` output | small drift (under ~10 days) or one domain not run through its timeline | dates that the timelines contradict by weeks, or a focus or tech planned before it can start |
| 4 | Economy | civ/mil timing fits the months to war (economy-build-orders §1.2); factory states safe (lint clean); no idle-factory gap (a free factory always has a line: by need, else the priority order, else an item that never goes obsolete); political power allocated over the whole run by value (`facts.pp_budget`: early the best affordable pick is bought at once; a balance is held only for a named costlier pick; nothing reserved for a pick whose gate never opens; no threshold without a named target); stability and war support held at 50%+ with named sources; laws jump to the target; no conscription law the manpower plan does not need | right shape, one gap or one questionable state | builds on fronts or in low infrastructure, or no build order |
| 5 | Research | the order follows the techs that pay most for this campaign (industry and construction usually early, the bonus targets of focuses and decisions kept for them); nothing past `start_year` without a stated pay-off; block sizes fit the slots; fallback off or restricted; slot time is planned, and a slot held empty names the technology or date it waits for (`research_waits`) | one block too long or one off-goal tech | ahead-of-time techs, missing prerequisites, or open fallback |
| 6 | Production and army | the main line and armour templates BEAT the game's own AI's and the enemy's main template on each war day in soft attack and breakthrough by the plan's margin (`facts.force.firepower` from divstats.py; composition in `facts.force.quality`; no `force.template_floor`, `force.firepower`, `force.staff`, `force.staff_domain`, `force.exercise`, `force.regular`, `force.idle_xp`, `force.idle_pp`, `force.cas_generation` or `force.air_range` error), officer-corps spirits and every staff seat planned, armies exercised to Regular; `force_design` states per war the force its tasks need (breakthrough armour and mobile divisions, line infantry with support, air, sea) with counts and ready dates, `facts.force.table` shows each row fielded by its date and no `force.*` error stands (one template carrying the army, armour or air missing for an offensive, a template created after its war are blockers); `calc equipment` shows lines cover templates × divisions + reinforcement; `facts.equipment_balance` in lint.json has every family covered within 90 days of war (a family with no line, or covered later, is a blocker or major finding); trucks with motorisation; disband plan; layout templates and counts exist; day-0 factory step, line audit at start and per phase, items in progress finished first, a switch-or-ramp plan per tier change, stockpile-cover targets | coverage short in one equipment or no disband plan | lines exceed factories, equipment not unlocked, or layout names not in the OOB |
| 7 | Conquest | `goal.capitulation` states the aim and what it rests on; every lever in `facts.capitulation_levers` (enemy war support, collaboration operations) is used with start, cost and effect, or answered with why not, and a lever that pays within the run left unused without a reason is a major finding; a lever no tool carries is in RUN/tool-gaps.md; the main targets are the cheapest set to the aim in value-for-difficulty order (no `conquest.vp_selection` error, running totals given), reserves after them reach the unlowered threshold; mandatory states included; routes and river counts given; supply construction triggered along the axis; every `supply.axis_reach` warning on a state that a checkpoint or the goal needs is answered with the port, railway to a captured node or pacing that supplies it before that date (otherwise a major finding); planning wait before each offensive, supply floor 50, bounds after captured hubs, a pocket reduction step; every land war against more than about 30 enemy divisions is an operation (`operations[]`): one group front, one group offensive whose target line with its phase lines takes the surrender threshold (`facts.force.operations[].surrender`), the armour in mobile armies on the main axis, the war-day placement by template equal to the forecast, the attack at its ratio against the enemy estimate, plans drawn 30 days before the war day, and no `war.operation_*`, `war.attack_force` or `war.axes_missing` error | order works but one leg unjustified or supply follows late; an operation whose placement or line is valid but thin (the attack only just at its ratio, a single phase line to the threshold) | threshold not reached, mandatory state missing, targets not reachable, or a large land war with no operation |
| 8 | Defence | every enemy-bordering own state guarded; on each war day every border with a country at war or likely to join (joiners, guarantors) is a named hold of an operation, in its own army group, with 2 or more divisions a border province (3 or more facing a major) and no `war.hold_sector` or `war.guarantor` error; corridor rule when targets go beyond 2 provinces; encirclement risks named with a response | one border or corridor thin | an enemy-bordering state without a guard, a hostile border held only by a reserve, or guards removed during a phase |
| 9 | Contingencies | each likely failure (lessons that apply, scenario escalations, stalls, core loss, new war) has a trigger predicate and a phase / overrides / revise response; checkpoints at mission deadlines | some failures covered only in text | none, or triggers the executor cannot evaluate and no player fallback |
| 10 | Executor readiness | most decisions in executor-owned keys; player items name exact tool calls that `check_calls.py RUN` accepts; ids stable; schema and lint clean; the coverage table resolves all 29 items (executor key, player call, or not applicable with a cited reason) | some player text that an executor key could carry, or one item resolved without a fact | key decisions only in prose, player items without calls, any call `check_calls.py` rejects, or checklist items missing from the coverage table |
| 11 | Brief usefulness | ≤ 60 lines; win check, next 3 checkpoints, what the executor does, player calls, judgement events with predicates, do-not list, `plan_revised` step; actionable in one batch per turn | complete but long-winded or one part missing | over 60 lines, or the player cannot tell what to do |
| 12 | Force posture and XP budget | land / air / naval shares by phase follow the goal and the enemy aggregates; production, research folders and doctrine folders match them; XP forecast per branch with every spend dated, no month over the forecast, no pool left at the cap | posture stated but one arm unjustified, or the XP schedule not checked against the forecast | no posture, or planned XP spends the forecast cannot pay before they matter |
| 13 | Templates and doctrines | a template per role and phase with width from the theatre's terrain mix (fill table: at least 0.95 on the main terrain; 18, 24 or 30 by regimental columns, 25 on one-direction mountain axes), support and regimental support justified, priority and XP cost given, equipment unlocked and covered, fielded divisions' switch dated; doctrine per folder and track order from the dossier, no grand-doctrine change without a reason that beats the mastery reset | templates exist but one role is missing or the width is not derived from terrain | a land war with no template decision, widths that do not fit the main terrain, or doctrines not in the dossier |
| 14 | Air, naval and overseas supply | the air force as an arm: fighters sized to the front's air force as far as the economy pays, CAS over the breakthrough axes, naval air for landings, each with its factory share, airframe research, design, doctrine, bases on our side of the border with capacity, deploy entries, fuel, and use by phase (superiority first, then support, following the front; what happens when superiority is lost), or `force_design.air_why`; no `force.air*` error; the navy as an arm: every sea operation the goal needs in `force_design.sea` with the means that win it by its date against the enemy's start fleet (surface parity, naval strike planes, carrier air with its groups, submarines), patrols per region, screens at the ratio, lift, landing techs, ports, a dated building programme by hull type, designs, doctrine, admirals, and a fallback for every lever no tool carries; no `force.sea*`, `force.navy*` or `force.carrier_air` error; or "no navy" with a reason; every overseas theatre has ports, convoys, escort and sea transfers | one of these thin (a base over capacity, convoys not sized) | an overseas theatre without a convoy and port plan, a landing or sea goal without the sea control it needs, or an air force with no plan |
| 15 | Occupation, peace, intelligence | occupation law per phase and per state with the resistance-target switch rule and a garrison template by hardness; peace demands ranked per enemy in bidding order with the answer to scripted peace offers; never-take list; intelligence decided (agency and upgrades, or not applicable) | one of these missing | a conquest with no occupation item, or an enemy with no peace item |
| 16 | Continuous-management thresholds | every rule of procedure item 28 is a game fact (cited), a project safety rule, or a value judgement with its reason and exception (rest and resume relative to max organisation, supply floor at the game's attrition start, pocket fall-back); no bare number that holds back a better action; rules the executor does not watch yet are brief lines with read, value and call | some rules left at default without a reason | no thresholds decided (`alarms` untouched and no `manage` decisions) |
| 17 | Dominance and decisive wars | economy and force from the country's best build, well above the AI baseline on every date (`facts.force.dominance`: attack templates, air over the operation, military factories and divisions above the AI by `force_design.dominance_margin` on each war day, no baseline date under the AI; any `force.dominance` error = 0 unless the plan names the axis, its best reachable figure and its cost); each offensive war has `duration_days` and `decisive` reasons that the numbers back: breakthrough met against the enemy grown to the war's day, fighters met, supply, divisions at least half the enemy's estimate and above the AI's; bombers where the war is long; designs costed with their modules; every goal reached in full; no `force.decisive`, `force.bombers` or `force.design_cost` error | one war short on one measure with a dated remedy | a war planned to grind (no duration or months of attrition), a breakthrough short against the enemy's real strength, or a goal doubted or scaled down |

## 2. Output format

The critic writes `RUN/review/critic-<n>.json` (n = 1 + the number of existing `critic-*.json` files):

```json
{
  "round": 1,
  "scores": {"1": 2, "2": 1, "3": 2, "4": 1, "5": 2, "6": 1, "7": 2, "8": 2, "9": 1, "10": 2, "11": 2,
             "12": 1, "13": 2, "14": 1, "15": 2, "16": 1, "17": 2},
  "total": 27,
  "max": 34,
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

`max` is 34 (2 points × 17 rows); `scripts/strategist/strategize.py` reads `total`, `max`, `scores` and
the open blocker and major findings (a finding's `status` is `open` unless the critic marks it `fixed`).

Severities:
* `blocker`: the plan cannot reach the goal or breaks a hard rule (wrong goal, contradicted fact, FAIR
  breach, threshold not reached, unguarded border, lint error missed, a player call that
  `check_calls.py RUN` rejects, a checklist item with no resolution, an overseas theatre with no convoy
  plan). Any open blocker fails the gate.
* `major`: likely to cost the goal or weeks of game time (timing off, supply late, a likely failure
  without a contingency). An open major finding sends the plan back to the strategist in round 1 even
  when the total passes; after the last round the plan ships with its open majors listed in the report.
* `minor`: clarity, tagging or efficiency.

Each finding has one claim, the evidence that shows it (command output or file line), and a concrete
fix. No finding without evidence. In round 2 the critic re-checks only the open findings and the parts
the answers changed, keeps the ids, and sets each finding's `status` to `fixed` or `open`.

## 3. Answer protocol (strategist)

1. For each finding write an entry in `RUN/review/answers-<n>.json`:
   `{"id": "F1", "action": "fixed" | "rebutted", "change": "what changed in which file", "source": "facts output or file:line for a rebuttal"}`.
   A rebuttal needs a source; "I disagree" is not an answer.
2. Re-run step 7 (validate and lint) after the fixes.
3. Run the critic for round 2 with the same command. It reads the answers.
4. At most 2 rounds. Gate: no open blocker, no open major (round 1), total ≥ 24 of 34, no 0 in rows 1, 2, 7, 13, 17. If round 2
   fails, write `RUN/strategist-blocked.md` (failed rows, open findings, what would fix them) and stop;
   do not run a third round and do not freeze.
5. List every finding and its answer in plan.md §11.
