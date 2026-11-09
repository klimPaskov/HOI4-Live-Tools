# <Country> <goal in a few words> — <FAIR|META> campaign plan (<start date>, HOI4 <build>, <DLC that matters>)

Executor plan: `plan.json` (`validate_plan.py --strict`: <n> errors; `lint_plan.py`: <n> errors, <n>
warnings). Dossier hash `<hash>`; DLC <list>; mods <list or none>. Tags: [V file:line] from the dossier
or facts CLI, [D method] computed (build-order, equipment, pp, research-timeline, focus-timeline,
route), [H] judgement. Dates use the game's 365-day calendar.

**Win condition.** <predicates in one or two sentences, with ids> [V ...], before <deadline> [V ...].
<surrender arithmetic: VP needed of total, with and without modifiers> [V ...]. <mandatory control rule,
if any> [V ...].

## 0. Goal reading and plan keys

| | |
|---|---|
| User's words | "<goal verbatim>"; clarifications: <verbatim or none> |
| Chosen reading | <one sentence> — because <in-game formalisation found / pattern used> [V ...] |
| Predicates | `<predicate>`, ... |
| Deadline | <date> [V ...] (scenario timer) or <horizon> (run spec) |
| Fail conditions | `<predicate>`, ... |
| Alternatives | <reading> — rejected because <reason>; ... |

Executor-owned keys used: <list>. Player-owned keys used, with the tool call each needs: <list>.

## 1. Checkpoints

| Date | Must be true | If not |
|---|---|---|
| <start> | setup done while paused (section 2); unpause at speed 3 | — |
| <date> [D ...] | <predicate in words, ids> | <phase / contingency / revise> |

## 2. Opening — setup while paused on <start date>

**Batch 1 — read.** <read-only tool calls>.

**Batch 2 — economy.** Production, research (one tech per slot), focus, construction, laws.

**Batch 3 — army and air.** Army layout table, generals, fronts, air missions, disband list, doctrine.

| Army | Divisions (templates, start states) | General | Role |
|---|---|---|---|
| <name_hint> | <templates × count> [V oob] | <name> [V characters] | strike / guard: <targets or states> |

### 2.1 Political power and laws

| Purchase | Cost | Condition | Date |
|---|---|---|---|
| <law or advisor> | <PP> [V ...] | <predicate or none> | <date> [D pp] |

## 3. Research path

Rules: never before `start_year`; prerequisites first; one slot on industry or construction until <tier>.

| Year | Order (<n> slots) | Finish dates |
|---|---|---|
| <year> | <tech ids> | [D research-timeline] |

Do not research: <list with reason>.

## 4. Construction order

<civ/mil timing in one paragraph: months to war, switch date> [D build-order].

| # | Build | Where | When (trigger) | Why |
|---|---|---|---|---|
| <id> | <building> | <state / province / path> | <trigger> | <reason> [V ...] |

## 5. Production

| Line | Factories | From | Covers |
|---|---|---|---|
| <equipment> | <n> | <date> | <templates × divisions> [D equipment] |

## 6. Focus path

<focus> (<end date>) -> ... [D focus-timeline]. Runtime gates and how the phases satisfy them: <list>.

## 7. War phases

Alternatives considered for the overall approach: <option> — rejected because <reason>; ...

**P1 <name> (<enter> -> <exit>).** <goal, armies, guards, events>. Exit: `<predicate>`.

### 7.1 Victory-point order

| # | Province | VP | Running VP | Route (rivers) | By |
|---|---|---|---|---|---|
| <id> | <name> <province> | <vp> [V vp] | <sum> | <from -> to> (<n>) [D route] | <date> [D route] |

## 8. Holding ground and avoiding encirclement

<guards per border state, corridor rule, supply rule, organisation rule; lesson ids covered>.

## 9. Contingencies

| Trigger | Do | Lesson |
|---|---|---|
| `<predicate>` (<words>) | <phase / overrides / revise> | <lesson id or —> |

## 10. Do-not list

* <rule> (<lesson id or source>).

## 11. Lint warnings, critic answers, reconcile

| Item | Answer |
|---|---|
| lint <rule>: <message> | <reason it stays> |
| critic F<n> (<severity>): <claim> | fixed: <change> / rebutted: <source> |
| reconcile | <0 unresolved / not run> |

## 12. Sources

Commands run with their key outputs (`facts.py ...`, `calc.py ...`), and the playbook references used.
