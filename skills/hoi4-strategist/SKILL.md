---
name: hoi4-strategist
description: Write, review or revise a grounded Hearts of Iron IV campaign plan (plan.json, plan.md, plan.brief.md) for the standing-plan executor and a fast player, from a run_spec.json (tag, start date, the user's goal, fair|meta, DLC/mods, horizon) or a revision_packet.json. Use before a campaign starts, as the critic of such a plan, and for mid-game plan revisions. Facts come only from the dossier and the hoi4facts CLI. Not for playing the game; no game needed except the live reconcile step.
---

# HOI4 strategist

You turn one user goal into a plan that `native/src/plan_run.rs` (the executor) runs on its own and
that a fast player can finish with a short brief. You do not touch the game. The plan is only as good
as its facts, so every fact comes from the dossier or the facts CLI, and every number in `plan.md` says
where it came from.

Roles that use this skill (prompts in `prompts/`):
* **strategist**: the 10-step procedure below, from `RUN/run_spec.json`.
* **critic**: fresh context, scores the plan with [references/rubric.md](references/rubric.md) (step 8).
* **revise**: mid-game revision from `RUN/revision_packet.json`, per [references/revision.md](references/revision.md).

`RUN` is the run folder (environment variable `HOI4_LIVE_RUN`). Run every command from the repository
root. Pass the global flags `--date <start_date> --mode <fair|meta>` from the run spec to every
`facts.py` call, plus `--game`, `--userdir` or `--mod` when the run spec or the dossier manifest names
them. Use `--json` when you will parse the output.

## Hard rules

1. **Facts only from the dossier or the facts CLI.** Never state a game fact (id, cost, date, VP,
   trigger, template, character, define) from memory or from a playbook reference without checking it
   with `python3 scripts/hoi4facts/facts.py ...`. Never open game files directly; use `facts.py grep`.
   If a fact cannot be found, say so and tag the dependent number [H].
2. **Every number in plan.md is tagged**: `[V file:line]` (from the dossier or facts output, with the
   source it prints), `[D method]` (computed: `calc build-order`, `research-timeline`, `route`, ...),
   or `[H]` (your judgement). More than 30% [H] is a lint warning; explain it in plan.md §11.
3. **FAIR boundary (user decision).** In `mode: fair` use only `public` and `aggregate` enemy facts:
   map, VPs, states, factories, mechanics, enemy division counts and templates at start. Never use
   per-division enemy positions or `common/ai_strategy`. Do not grep `ai_strategy` and do not ask the
   facts CLI for enemy unit locations. `mode: meta` may use all three classes. Either way the campaign
   rules in `skills/hoi4-campaign-playbook/references/rules-and-modes.md` apply.
4. **Schema keys only.** `plan.json` uses only keys from `skills/hoi4-campaign-playbook/plan.schema.json`
   (plus `_`-prefixed keys). Prefer keys marked `x-owner: executor`; a `player` key is used only when no
   executor key can carry the decision, and then it names the exact tool call (`call: {tool, args}`)
   where the schema allows it. Give every list item a stable `id`.
5. **Validate and lint until 0 errors.** Every warning that remains is listed in plan.md §11 with a
   reason.
6. **Record the goal reading.** plan.md §0 and `goal.reading` / `goal.alternatives` state the chosen
   reading of the user's words, the other readings, and why this one ([references/goal-patterns.md](references/goal-patterns.md)).
7. **Read the lessons** ([references/lessons.md](references/lessons.md)) before step 4 and show in
   plan.md how each applicable lesson is prevented.
8. **Never wait for input.** If a gate cannot pass, write `RUN/strategist-blocked.md` with the gate,
   the evidence and what is needed, and stop.
9. **Forget nothing.** The user does not list every small decision; the plan makes all of them. Every
   item of the step 5 checklist is resolved in the coverage table, including the ones the goal's words
   do not mention (templates, XP, occupation, peace, thresholds).
10. **Calls from the tool catalogue, never from memory.** Write every player call (`call` in plan.json,
   every `hoi4_x {json}` in plan.brief.md and plan.md) from the output of
   `python3 scripts/hoi4facts/tool_schemas.py show <tool>`: exact argument names, types and enums. Check a
   call with `tool_schemas.py check-call <tool> '<json>'`; `check_calls.py` checks them all in step 7.

## Inputs

* `RUN/run_spec.json` (shape: [templates/run_spec.json](templates/run_spec.json)).
* The dossier (built in step 1): `RUN/dossier/dossier.md` (read in full), `dossier.json` (never read
  whole; query it through `facts.py`), `graph.json.gz`, `manifest.json`.
* The executor capability list: the `x-owner` tag of every key in
  `skills/hoi4-campaign-playbook/plan.schema.json`. Print it with
  `python3 -c "import json;s=json.load(open('skills/hoi4-campaign-playbook/plan.schema.json'));[print(k,v.get('x-owner'),v.get('description','')) for k,v in s['properties'].items()]"`.
* Strategy knowledge: the playbook references listed in [references/procedure.md](references/procedure.md),
  the tactic index `skills/hoi4-campaign-playbook/references/tactics.md` (every tactic in one line; scan it
  before step 5), the tag's entry in `skills/hoi4-campaign-playbook/references/nation-openers.md`, and a
  scenario reference when one exists (`skills/hoi4-campaign-playbook/references/INDEX.md`).
* Lessons: `skills/hoi4-campaign-playbook/references/lessons.jsonl` (path also in `run_spec.lessons`).

## Procedure (each step has a gate; do not start the next step before it passes)

**1. Build the dossier.**
`python3 scripts/hoi4facts/facts.py dossier build --tag <tag> --date <start_date> --dlc auto --mods auto --out RUN/dossier`
(use the run spec's `dlcs` / `mods` when they are not `auto`; the builder is
`scripts/hoi4facts/dossier.py`). Then read `RUN/dossier/dossier.md` in full;
`facts.py dossier show <section>` reprints one section later. The full command list is at the top of
[references/procedure.md](references/procedure.md).
*Gate:* `RUN/dossier/manifest.json` has a content hash (record it for `_meta.dossier_hash`), and its
DLC and mod list match the run spec (or the launch configuration when `auto`).

**2. Formalise the goal.** Follow [references/goal-patterns.md](references/goal-patterns.md): search
in-game formalisations first (`facts.py decisions <TAG>`, `missions <TAG>`, `hooks <TAG>`, `vp <ENEMY>`,
`grep <token>`), then the generic patterns. Write `goal` (`summary`, `reading`, `alternatives`,
`predicates`, `deadline`, `fail_predicates`, `sources`). *Gate:* every predicate uses real ids from the
dossier or facts output; every scenario rule that changes the outcome is cited (for example "the
attacker must control state S at capitulation"); the deadline equals the scenario timer when one
exists.

**3. Situation analysis.** From dossier §1-4 and §8-9: wars at start and scripted wars with dates
(`hooks <TAG>`, `event <id>`), guarantees and factions, enemy aggregates (`oob <ENEMY>` counts and
templates only in FAIR), own factories, free slots (`states <TAG>`), PP per day (`calc.py pp`),
research slots, manpower, OOB by theatre (`oob <TAG>`). *Gate:* a dated "first war" estimate with its
source, and months to that war.

**4. Strategy skeleton.** Phases with `enter` and `exit` predicates, what each is for, and at least two
alternatives considered with the reason each was rejected (plan.md §7 intro). *Gate:* every phase exit
is a schema predicate; the last phase's exit implies the goal predicates.

**5. Domain plans: the 29-item checklist.** Work through every item of
[references/procedure.md](references/procedure.md) in order: goal, situation, force posture, doctrine
path, XP budget, templates, equipment designs, production, MIOs, economy timing, research, focus and
balance of power, gated laws, stability and war support, advisors and high command, army organisation,
manpower, operations, supply and logistics, air plan, naval plan, diplomacy, occupation, intelligence,
peace aims, events and decisions, checkpoints and contingencies, continuous management, FAIR boundary.
Each item names the facts command to run and the check it must pass.
**Hard rule: every item ends in exactly one resolution:** an executor key (preferred); a player item
with an exact `call` (`{tool, args}`) and the same call in the brief; or `not applicable: <reason>`
where the reason cites a fact or a missing tool. A decision whose key or predicate atom is not in the
schema yet is a player line in the brief, never an invented key (procedure "Key or atom not in the
schema yet"). Nothing is left for the player to notice on their own.
*Gate:* the coverage table (plan.md §25) lists all 29 items, each with its resolution, where it lives
and the facts command used; no empty row, no "later" or "TBD", and items 1, 2 and 29 are never "not
applicable".

**6. Emit.**
* `RUN/plan.json`: valid against the schema; `_meta.version` 1 (step 10 fills the rest).
* `RUN/plan.md`: from [templates/plan.md](templates/plan.md), the section order of
  `skills/hoi4-campaign-playbook/plans/italy-ethiopia-1936.md`. Every number tagged.
* `RUN/plan.brief.md`: from [templates/brief.md](templates/brief.md), at most 60 lines.
*Gate:* the three files exist; `wc -l RUN/plan.brief.md` is 60 or less.

**7. Validate and lint.**
`python3 scripts/validate_plan.py RUN/plan.json --strict` (add `--userdir` / `--mod` when mods are
active), `python3 scripts/hoi4facts/lint_plan.py RUN/plan.json --json > RUN/lint.json` and
`python3 scripts/hoi4facts/check_calls.py RUN` (every player call against the tool catalogue). Fix and
re-run. Also `python3 scripts/hoi4facts/facts.py research-timeline RUN/plan.json` and
`facts.py focus-timeline <TAG> --date <start_date> <focus_path ids...>` and copy their dates into
plan.md. *Gate:* 0 errors from all three; every remaining warning is in plan.md §11 with a reason.

**8. Critic pass.** Normally `scripts/strategist/strategize.py` runs steps 1-10 in order and starts the
critic itself (prompts `strategist.md`, `fix.md`, `critic.md`, `answer.md`). Run standalone, start the
critic in a fresh context:
`python3 scripts/strategist/run.py --role critic --run RUN --prompt skills/hoi4-strategist/prompts/critic.md`.
It writes `RUN/review/critic-1.json`. Answer every finding in `RUN/review/answers-1.json` (fix, or rebut
with a source), re-run step 7, then run the critic again for round 2. At most 2 rounds; protocol in
[references/rubric.md](references/rubric.md). *Gate:* no open `blocker`, rubric total ≥ 22 of 32, and no
0 in rows 1, 2, 7 or 13. If round 2 still fails, write `RUN/strategist-blocked.md` and stop.

**9. Reconcile live.** Only when the game is loaded and paused on day 1 (allowed pre-start setup) and
the operator runs `python3 scripts/strategist/reconcile.py --run RUN --call-cmd "<broker call command>"`.
It compares plan references (templates, generals, air wings, production lines, available techs) with
read-only tools and patches one-to-one name mismatches; it lists everything else in
`RUN/reconcile.json`. Air wing names are made at runtime, so `air_plan[].wing` is written as
`"<variant> at <state id>"` from the dossier OOB; reconcile expands each selector to the live wings. Fix each listed
item in the plan, then repeat steps 7 and 10. *Gate:* 0 unresolved references. If the script is not
available, write "reconcile: not run" in plan.md §11 and continue.

**10. Freeze.** Set `_meta`: `version` 1, `parent` null, `campaign` (tag + start date + run folder
name), `dossier_hash` from the manifest, `models` from `RUN/agents-manifest.json` (`strategist` and
`critic`: provider, launcher, model, resolved_model, effort), `created` (UTC ISO time), `lint_summary`
(`errors`, `warnings`). Re-run step 7 once more. *Gate:* 0 errors. The operator then loads the plan
with `hoi4_plan` and starts the autopilot; you do not.

Time budget: steps 1 to 8 in about 30 minutes of wall time.

## Output contract

| File | Content | Checked by |
|---|---|---|
| `RUN/plan.json` | executor keys plus player items; `_meta`, `goal`, `phases`, `checkpoints`, `contingencies` | `validate_plan.py --strict`, `lint_plan.py` |
| `RUN/plan.md` | the human plan, [templates/plan.md](templates/plan.md); §0 goal reading and alternatives; §11 warnings and critic answers; §12-§24 the checklist sections; §25 coverage table (29 items) | critic (rubric rows 1-9, 12-16) |
| `RUN/plan.brief.md` | the player section, ≤ 60 lines, [templates/brief.md](templates/brief.md) | `check_calls.py`, critic (rubric rows 10-11) |
| `RUN/lint.json` | the last lint output | step 7 |
| `RUN/review/critic-<n>.json`, `answers-<n>.json` | critic findings and the answers | step 8 |

## References

* [references/procedure.md](references/procedure.md): the 29-item checklist with facts commands (step 5).
* [references/goal-patterns.md](references/goal-patterns.md): user phrases to predicates (step 2).
* [references/rubric.md](references/rubric.md): critic rubric, output format, answer protocol (step 8).
* [references/revision.md](references/revision.md): mid-game revision.
* [references/lessons.md](references/lessons.md): how to use the lessons from failed runs.
* [examples/README.md](examples/README.md): hand-written plans as reference outputs.
* Plan schema: `skills/hoi4-campaign-playbook/plan.schema.json`. Strategy knowledge:
  `skills/hoi4-campaign-playbook/references/INDEX.md`.
