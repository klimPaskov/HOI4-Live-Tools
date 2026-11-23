# Monitoring a run: issues, lessons, escalations and costs

A campaign makes mistakes a person would notice: an air force left over a country that has
already surrendered, an army group with no field marshal, 23 divisions attacking from one province
while 19 of them wait in reserve, the same refused order sent six times, saves failing, dockyards
idle. The run monitor finds these while the game runs, files each one as an **issue** with an owner
and a deadline, sends it to the part of the executor that can act on it, and remembers what worked
as a **lesson** for later runs. It needs no model for any of that. A model is asked only when a rule
cannot decide, and then on a one-page packet, within a fixed budget per game year.

## Starting it

```sh
python scripts/observe/observe.py watch RUN        # live, beside the run (stops when the run closes)
python scripts/observe/observe.py monitor RUN      # replay a finished run: what it would have caught, and when
python scripts/observe/observe.py costs RUN        # RUN/COSTS.md
python scripts/observe/observe.py coverage RUN     # RUN/COVERAGE.md
python scripts/observe/observe.py lessons learned seed            # install the seeded lessons
python scripts/observe/observe.py lessons learned list
python scripts/observe/observe.py lessons learned lint plan.json  # plan rows a lesson warns against
```

`watch` reads only what the run's logs gained since its last poll (every 10 seconds): the broker's
call log (`calls.jsonl`), the executor's orders and read-backs (`game-actions.jsonl`), the
server's action lines (`mcp.stderr`), the memory log (`health.log`) and the state recorder's
snapshots (`state/*.json`). It never sends input and never reads the game's memory itself. The
detector and its files are always on; nothing has to be enabled.

## Issues (Tier 0: rules, no model)

Every issue has an id, a class, a severity, first and last seen (game dates), the numbers that
show it, the executor lane that owns it, a deadline with its reason, and a state: open, acted (the
owning lane sent an order since), resolved, or recurring (back after it was resolved).

| class | what | examples |
|---|---|---|
| technical | the tools or the game misbehave | saves failing, a tool failing in a row, the date not moving while unpaused, memory near the limit, a snapshot read failing |
| tool use | a call that could not pass | wrong arguments, an order sent while the cause of its last refusal still held, a refused order sent again unchanged, an order read back without its change |
| duplicates | the same verified order again within a game week | a battle plan executed after every redraw |
| idling | something that should work does not | idle factories or dockyards, empty research slots, an army group without a field marshal, armies without orders, a war with no army order for three days |
| gameplay | the war or the economy goes wrong | overstacked provinces, divisions waiting in reserve, a losing exchange, victory points flat, air over a region with no enemy, a plan checkpoint missed, low planning at a declaration |

Deadlines depend on the class and on whether the country is at war (a battle issue gets three
game days, idle dockyards a week at peace). Past its deadline an issue is **overdue**; past twice
its deadline without action it is **stale**.

Files in the run folder:

- `issues.jsonl`: every event (open, acted, overdue, stale, resolved, recurred, escalated, lesson), append-only.
- `issues_open.json`: the open issues, newest state, with each issue's cause, lane and best lesson; the executor reads this file.
- `ISSUES.md`: the same for people, with counts per class per game year (`issues_counts.json`).
- `GATE.md` and `gates/`: the gate packet at each declaration of war, capitulation, new year, failure and the run's end.
- `COSTS.md`, `COVERAGE.md`, `monitor_stats.json` (the monitor's own time per snapshot).

## The response path

Each issue class names an executor lane (`hoi4_live/observe/issue_rules.py`): idle dockyards go to
the dockyards lane, an army group without a marshal to the command lane, a missed save to the save
flow. The executor reads `issues_open.json` at its tick and offers the lanes that own open issues
before its routine work, at most once per game day per lane. When an order of that lane passes,
the issue becomes "acted"; when the snapshot no longer shows it, "resolved".

"Acted" means an order of the lane passed, not that the lane had a turn. When the executor offers
a lane an issue it writes the lane's typed answer to `RUN/issue_outcomes.jsonl` (`acted`, `refused`,
`waiting` or `cannot`, with the lane's own note as the reason); the monitor keeps it on the issue
(`lane_outcome`, `lane_reason`; for an executor omission row the rule's own note is the reason).
An issue its lane was offered that is still open past its deadline is escalated whatever its
severity: the Tier 1 packet carries `LANE SAYS: <the lane's reason>` beside the allowed responses.
A lane's further overdue issues within two game weeks join that lane's packet (`ALSO ...`), so a
backlog is weighed in one answer inside the yearly budget.

### Sinks and slots

`sinks.py` judges every spendable or assignable thing the snapshot shows (with `gamerules.py`
reading the game's own files: experience caps, spirit slot costs, the mastery a subdoctrine's
rewards need, the laws with their costs, modifiers and the conditions a snapshot can judge):
`experience_unspent` (per pool, with the days to the pool's cap at the observed gain),
`doctrine_track_complete` (a track past its last reward reserves only part of its mastery),
`doctrine_track_no_mastery` (a subdoctrine whose arm never fights), `manpower_idle` (free men and
no division in training, with what binds: equipment or the training rows), `conscription_above_need`
(the law in force has a penalty in the files that a lower law drops while men sit idle),
`fleet_training_at_war`, `reserve_fleet_ships`, `operatives_home_only`, `decryption_unread` /
`decryption_idle`, `mio_state_unread`, `special_project_kind_without_facility`, and
`lane_wait_outlived` (an omission row whose rule has said the same thing past the row's own
deadline). The recorder also writes the derived read `politics.laws_affordable` (every law of each
group with its cost against the power and whether its conditions hold) into each snapshot. The
detector computes these on every full snapshot, so a replay of an older run shows them too.

A refused order is looked up in the lessons before it is retried: a cause a lesson calls transient
(an event was open) is retried soon and never given up; one it calls permanent is not sent again
unchanged, and the player is told what to do instead. The second identical refusal of an order
(the same tool, arguments and cause; or a read-back without the change, or a wrong call) writes a
lesson of the run (`scope: run:<run>`, confidence 0.5) with the order's arguments, what was tried,
the refusal and what to do instead: a wrong call or an order for what already holds is not sent
again, a precondition rests a week, a GUI or read-back failure of that order rests two days for a
war order and two weeks for any other. The executor uses a run's own lessons in that run only. Issues the executor cannot act on (a plan
decision, a tool fault) appear in the player's wake list with their packet.

## Escalation (Tier 1 and Tier 2)

A model is asked only when a rule cannot answer:

- a judgement between named alternatives (an overstacked front: widen it, split the formation, or
  wait for the attack to clear);
- the owning lane acted and the issue stayed past its deadline;
- an issue no executor lane owns;
- an issue the owning lane left past twice its deadline;
- an issue that keeps coming back after it was resolved.

The executor's own refusals are never escalated (its ledger and the lessons answer them), and one
escalation covers its kind for two game weeks. The budget is 24 escalations per game year
(`Monitor(per_year=...)`); the escalation log records the ones skipped for the budget.

The packet is built by code: what, since when, the deadline and why, the numbers, what was tried,
recent related calls, matching lessons, and the allowed answers, which are limited to one
executable action of the owning lane, one logged plan edit, or "leave it" with a reason. It is about
1,200 characters. By default it is delivered in the player's wake (no second model). With
`MONITOR_ANSWER_CMD` set (for example `claude -p --model <small model> --output-format json`), the
command gets the packet on stdin, and its answer is checked against the allowed answers. Every
escalation is logged with its tokens and seconds in `escalations.jsonl`.

At each gate (war declared, capitulation, new year, failure, run end) a one-page gate packet sums
up the issues since the last gate, the run's cleanliness, and the costs so far, for a review by a
stronger model or a person.

## Model calls and the budget

The monitor calls a model only for Tier 1 escalations and Tier 2 gate reviews, and only when an API
key is configured. Put your own key in a file readable only by you and point the monitor at it:

```sh
export HOI4_LIVE_ANTHROPIC_KEY_FILE=/path/to/anthropic.key   # chmod 600; never committed
```

The key is read from that file for each call only; it is never printed, logged or put in the
environment of other processes. Without the file no model is called and everything else works the
same. Calls need the `anthropic` Python package (`pip install anthropic`).

- Tier 1: `MONITOR_TIER1_MODEL` (default `claude-haiku-5-5`), low effort, at most 2,000 output
  tokens, 24 a game year.
- Tier 2: `MONITOR_TIER2_MODEL` (default `claude-opus-5-5`), medium effort, at most 6,000 output
  tokens, at a declaration of war, a capitulation, a new year, a failure or the run's end, at most 8
  a game year. The review is written beside the gate packet (`gates/<date>-<kind>-review.md`).
- A replay (`observe.py monitor`) never calls a model.
- `MONITOR_ANSWER_CMD` replaces the Tier 1 call with your own command (packet on stdin);
  `python -m hoi4_live.observe.answer --tier 1 --run NAME` is the built-in one in that form.

Every call billed to the key is written to a ledger beside the key file (`api-ledger.json`, or the
folder `HOI4_LIVE_API_LEDGER` names): per run, role (player, escalation, review) and model, the
tokens the response reports and the money at the list prices in `costs.py` (dated, to be verified
against your billing). An optional `budget.json` in the same folder sets the caps:

```json
{"total": 200, "run_cap": 15, "day_cap": 40, "floor": 20}
```

Before every call the monitor checks them: a run that has spent `run_cap`, or a day that has spent
`day_cap`, stops calling models and goes on with the executor only; when the remaining total falls
under `floor`, nothing more is called on the key. The player's spend is read from its own stream
log every five minutes; when a cap is reached the monitor writes `RUN/budget_stop.json` for the
launcher that runs the player. `observe.py budget status`, `observe.py budget check --run NAME`
(exit 0 allowed, 3 at a run or day cap, 4 under the floor) and `observe.py budget sync RUN` give the
same answers to scripts. The first line of `COSTS.md` and of every gate packet is the budget line:
spent this run, today, in all runs, and what remains.

## Lessons

Lessons live in the learned store beside the skills: `learned/lessons/<id>.json` (the store is
`HOI4_LIVE_LEARNED`, else a `learned` folder beside the run folder). Each lesson has a situation
(issue code, or tool and cause), a statement, what was tried, the outcome, evidence lines, a scope
(`run:<name>`, `plan:<name>`, `country:<tag>` or `any`), the game version and mod set it was
learned on, and a confidence. A lesson confirmed by a later run gains confidence; one contradicted
loses it and is retired below 0.15. Lessons learned on another game version or mod set are not used
unless marked version-independent; review them after a patch.

The monitor writes lessons itself:

- a refused order that later passed unchanged: its cause is transient for that tool;
- a refused order sent four or more times and never passed: it does not pass unchanged;
- a state issue that went past its deadline and cleared after its lane acted: what was done worked;
- a state issue back a second time: the remedy does not hold.

Lessons are used by the executor (before it retries a refused order), by escalation packets (the
matching lessons are listed), and by the plan checks: `validate_plan.py --lessons learned` and
`lint_plan.py --lessons learned` print a warning for each plan row a lesson warns against. A lesson
is evidence to weigh, never a rule: it never overrides safety checks or the fair-play rules, and a
plan may depart from it with its reason.

`hoi4_live/observe/lessons_seed.json` holds the seeded lessons (general findings from earlier
campaigns, each with its evidence); `observe.py lessons learned seed` installs the ones the store
does not have yet.

## Costs and coverage

`COSTS.md` reports the model cost by role (player, escalations, reviews), game time per real
minute at each speed, tool latency percentiles and success rates, executor actions per game day,
and the monitor's own time. Prices are in `hoi4_live/observe/costs.py` (override with
`MONITOR_PRICES`, a JSON file of per-million-token prices). `COVERAGE.md` lists every registered
tool with who called it, how often it passed and failed, and the tools never called in the run.
