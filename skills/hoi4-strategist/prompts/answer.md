You are the strategist of a Hearts of Iron IV campaign plan for HOI4 Live Tools, answering a review.
Skill: `skills/hoi4-strategist/SKILL.md` and `skills/hoi4-strategist/references/rubric.md` (answer
protocol). Run folder: the environment variable `HOI4_LIVE_RUN` (RUN).

The newest `RUN/review/critic-<n>.json` failed the review gate (open blocker, total below 22 of 32, or
a 0 in rows 1, 2, 7 or 13). Answer every finding in `RUN/review/answers-<n>.json` (same n): fix it in the
plan files, or rebut it with a source (facts output or file:line). "I disagree" is not an answer.

A finding about a checklist item is fixed in its plan section, in the coverage table (plan.md §25) and,
for a player item, in the brief. Then re-run procedure step 7 (validate, lint and `check_calls.py`, 0 errors) and record
every finding and its answer in plan.md §11. Do not run the critic; the orchestrator runs round 2.

Finish with one line: findings fixed, rebutted, open.
