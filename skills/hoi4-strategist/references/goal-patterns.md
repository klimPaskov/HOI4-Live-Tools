# Step 2: from the user's words to goal predicates

The goal is a check on the game state, not a description. Write it as `goal.predicates` (all must hold),
`goal.deadline`, `goal.fail_predicates` (any one means the goal is lost) and `goal.sources`. Predicate
atoms and composites are defined in `skills/hoi4-campaign-playbook/plan.schema.json`
(`definitions.predicate`):

| Atom | Meaning |
|---|---|
| `YYYY.MM.DD` | the date is reached |
| `province_controlled:<p>` | we control province p |
| `state_fully_controlled:<s>` | we control every province of state s |
| `owner:<s>=<TAG>` | TAG owns state s |
| `state_controller:<s>=<TAG>` | TAG controls every province of state s |
| `province_controller:<p>=<TAG>` | TAG controls province p |
| `at_war:<TAG>`, `not_at_war:<TAG>`, `capitulated:<TAG>`, `exists:<TAG>`, `not_exists:<TAG>` | war and country state |
| `vp_share:<TAG><op><x>` | share of TAG's own VPs TAG still controls (`hoi4_states`), op `<= >= < >` |
| `focus_completed:<key>`, `tech_researched:<key>` | done |
| `phase:<id>` | that phase is active |
| `{"all":[...]}`, `{"any":[...]}`, `{"not": p}` | composites |

Only the first four atoms are evaluated by the executor today (construction triggers); the rest are
evaluated from build item 15 on. Write them anyway; the player checks them until then.

## 1. Search order for an in-game formalisation

`F` is `python3 scripts/hoi4facts/facts.py` with the run's global flags. The game often defines the
goal exactly. Look in this order and stop at the first that matches the
user's words; record what you found even when you do not use it.

1. **Formable nation decisions.** `F decisions <TAG>` and `F grep form_` for decisions whose name or
   loc text matches the goal (for example a `form_<nation>` decision). Its `available` block lists the
   exact `controls_state` / `owns_state` ids, and its `allowed` block says which tags may form it.
   Each `controls_state = S` becomes `state_controller:S=<TAG>` (or `state_fully_controlled:S`).
2. **Scenario win decisions and flags.** `F hooks <TAG>` and `F decisions <TAG>`: decisions that end a
   scripted war (for example `<TAG>_defeat_<ENEMY>`), win and defeat flags, and the conditions that set
   them (for example "the defeat flag is set unless TAG controls state S at the enemy's capitulation").
   `F event <id>` and `F grep <flag>` to follow the chain to the condition.
3. **Missions with timers.** `F missions <TAG>`: mission success conditions and `days_mission_timeout`.
   A mission that fails the goal when it times out sets the deadline and a checkpoint.
4. **War goals and wars at start.** Dossier §3-4: wars at start, `annex_everything` or other war goals,
   scripted war declarations with dates.
5. **Surrender arithmetic.** `F vp <ENEMY>`: VP total, the surrender threshold with and without
   modifiers, and the capital. Needed for any "capitulate" reading.

## 2. Phrase patterns

`<TAG>` is ours, `X` the enemy, `S` a list of state ids, `R` a region (a set of states from the
dossier), `D` a date.

| User says | Template | Notes |
|---|---|---|
| "conquer / defeat / capitulate X" | `capitulated:X` plus every mandatory control rule from hooks (`state_controller:S=<TAG>`) | VP order from `F vp X`; deadline from a mission or war timer if one exists, else the horizon |
| "take / control S", "own S" | `state_fully_controlled:s` for each s (control) or `owner:s=<TAG>` (ownership: needs a peace or annexation) | say which of control or ownership the user meant |
| "push X out of R", "drive X from R" | `{"all":[{"not": "state_controller:s=X"} ...]}` over every state of R that X controls at start, or `state_fully_controlled:s` for each | R is ambiguous when X holds several regions of that name; list every reading (section 3) |
| "survive until D", "hold out" | predicates: date D; `fail_predicates`: `capitulated:<TAG>` and core losses that matter (`{"not":"state_fully_controlled:<capital state>"}`) | the plan is defensive: guards and contingencies carry it |
| "form N", "unify N" | `state_controller:s=<TAG>` for each `controls_state` of the formable decision (search step 1); taking the decision is a player item (`decision_rules` with `when` = those predicates) | the `allowed` block must allow our tag; there is no "decision taken" atom yet |
| "defend the core", "lose nothing" | `fail_predicates`: `{"not":"state_fully_controlled:s"}` for each core state at start (`F states <TAG>`) | add a deadline (horizon) |
| "win war W" | the war's goal: capitulation of the war leader, or the scripted win decision if one exists (search step 2) | wars at start come from dossier §4 |
| "quickly", "as fast as possible" | same predicates; the deadline is the earliest feasible date from the timelines, the scenario deadline stays a fail condition | record both dates |

## 3. Stating the reading and the alternatives

* `goal.summary`: the user's words, verbatim.
* `goal.reading`: the chosen reading in one sentence, with the ids, and why (an in-game formalisation
  beats a generic pattern; the user's clarification beats both).
* `goal.alternatives`: every other plausible reading, one line each, with what it would change (for
  example "X's other African territory: adds states ...; out of reach before D").
* `goal.sources`: the file:line of each formalisation used (from facts output).
* plan.md §0 repeats this as a short table: reading, predicates, deadline, fail conditions,
  alternatives and why rejected.
* `run_spec.goal_clarifications` from the user always win; quote them in §0.

A plan whose reading changes between two strategist runs on the same input is a defect (design 4.1):
prefer the reading the game files formalise, and the narrowest one the words allow.
