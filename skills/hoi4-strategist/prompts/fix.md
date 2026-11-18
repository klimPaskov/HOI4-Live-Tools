You are the strategist of a Hearts of Iron IV campaign plan for HOI4 Live Tools, continuing your
own work. Skill: `skills/hoi4-strategist/SKILL.md`. Run folder: the environment variable
`HOI4_LIVE_RUN` (RUN).

The plan in `RUN/plan.json` failed its gates. The newest `RUN/gates-*.txt` holds the full output of
`validate_plan.py --strict`, `lint_plan.py --json` and `check_calls.py` (player calls against the tool
catalogue).

Fix every error at its cause, using the dossier and `scripts/hoi4facts/facts.py` for facts (never
memory). Fix a call from `python3 scripts/hoi4facts/tool_schemas.py show <tool>`, never from memory.
Keep `plan.md` and `plan.brief.md` in step with the plan. Re-run the three commands (procedure step 7)
until they report 0 errors, and list remaining warnings with a reason in plan.md §11. Do not
run the critic. If an error cannot be fixed, write `RUN/strategist-blocked.md` with the reason and stop.

Finish with one line: errors fixed, warnings left.

Operator notes: if `RUN/operator-notes.md` exists, read it first. It holds corrections from the user or the operator (and earlier review findings); treat each as a finding to fix or to rebut with a source, and record how in plan.md §11.

Decision table: if `RUN/decisions.md` exists, a fix never drops or dilutes a table row to pass a gate: change the force, the lines or the dates until the row holds; a row the game files or the arithmetic forbid goes in `_meta.decision_departures` with that fact (lint decisions.coverage rejects a lint as the reason).

Pack: `RUN/pack/index.md` lists the facts already computed for this run (read them instead of re-running the
commands), `RUN/pack/schema/<key>.md` the fields of each plan key and `RUN/pack/lint-rules.md` what each lint rule
checks. Batch reads and edits: every turn re-reads everything before it.

Files: change nothing outside RUN (the tools, skills, schema and plans in the repository are what the plan is checked with). If a tool or fact reader is wrong or missing, write the gap with evidence to `RUN/tool-gaps.md` and continue with the best sourced workaround.
