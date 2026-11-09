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
Every checklist item is resolved in the coverage table (§25).

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

| # | Build | Where | When (trigger) | Cost (points) | Finish | Why |
|---|---|---|---|---|---|---|
| <id> | <building> | <state / province / path> | <trigger> | <base cost> [V buildings] | <date> [D build-order] | <reason> [V ...] |

Supply construction in the order railways, ports, trucks, hubs; a hub row shows that it finishes before
it is needed and that no port or railway serves the same provinces.

## 5. Production

| Line | Factories | From | Covers | Stockpile target |
|---|---|---|---|---|
| <equipment> | <n> | <date> | <templates × divisions> [D equipment] | <days of cover> |

Lines to remove or replace at the start: <line -> replacement, call>. Trucks for motorisation: <n>.
Convoys: <n> for <overseas route / trade>.

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

## 12. Force posture

| Phase | Land | Air | Naval / convoys | Why (enemy aggregates, goal) |
|---|---|---|---|---|
| <phase id> | <n> mils [D build-order] | <n> mils | <n> dockyards | <enemy divisions / planes / ships> [V dossier §3] |

Arms not needed and why: <list>. Research and doctrine folders left out: <list>.

## 13. Doctrine path and XP budget

| Branch | Start XP | Forecast by <date> | Spends in order (month: item, cost) | Cap reached? |
|---|---|---|---|---|
| army | <n> [V ...] | <n> [H/D] | <month>: <subdoctrine> 100 [V ...]; <month>: template <name> <n> | no |
| air / navy | <n> | <n> | <...> or not applicable: <reason> | |

Grand doctrine per folder (kept or changed, and why); track order; theorist date; special forces branch.

## 14. Templates

| Template | Role, phase | Regiments + support (+ regimental row) | Width vs terrain target | Priority | XP | When / fielded divisions switch |
|---|---|---|---|---|---|---|
| <name> | line, P1 | <...> | <w> vs <target> [D terrain-width] | regular | <n> | <date or predicate> |

Garrison template: <name and count> (section 22).

## 15. Equipment designs and MIOs

| Design or MIO | Family / line | When | XP or PP | Line that switches | Call |
|---|---|---|---|---|---|
| <design> | tank / plane / ship | <date> | <n> XP | <line index> | `hoi4_design_create {"kind":"tank","base":"<base>","name":"<design>"}` |
| <organisation> | <line> | <date> | traits in order | — | `hoi4_mio_trait {"organization":"<organisation>","trait_index":0}` |

## 16. Stability, war support, manpower and balance of power

| Value | Start [V dossier §1] | Gate needed (for) | Kept above by | If below |
|---|---|---|---|---|
| stability | <n> | 50% | <advisor, spirit removal, decision> | <contingency> |
| war support | <n> | <gate> (<law>) | <...> | <...> |
| manpower | <n> | <divisions × manpower + 2 months of losses> [D equipment] | <conscription law date> | <...> |
| balance of power | <value / not applicable: reason> [V bop] | <side and range edge> | <focuses, decisions> | <...> |

## 17. Supply and logistics

Railway levels, then ports where no supply reaches the shore, then trucks, then hubs (section 4 rows with
cost and finish date); ports per overseas theatre (naval base level, throughput, free convoys at 0.25 per
supply point) [V ports], convoys and escort, motorisation per army with the truck stock it needs, fuel
plan.

## 18. Air

| Wing | Base (capacity) | Phase | Mission | Region | Switch when | Call |
|---|---|---|---|---|---|---|
| <variant> at <state id> [V oob] | <state> (<n>) [V states] | P1 | air_superiority | <region> | `<predicate>` | `hoi4_air_mission {"wing":"<wing>","mission":"air_superiority","region":"<region>"}` |

Wings are written as "<variant> at <state id>" from the dossier OOB (live wing names are made at
runtime); `scripts/strategist/reconcile.py` resolves them on day one. Superiority target before
offensives; ground AA. Or: not applicable: <reason>.

## 19. Navy

Fleets and missions per sea region, convoy escort, sea transfers (`hoi4_army_move`), naval invasions,
dockyards, repair rule. Or: **no navy: <reason>** [V states / dossier §3].

## 20. Diplomacy

| Action | Target | When (gate) | Call | Why |
|---|---|---|---|---|
| <justify / declare / join faction / guarantee / access> | <TAG> | <date or predicate> | `hoi4_diplomacy_act {"country":"<TAG>","action":"<action>"}` | <reason> |

Answer policy for incoming proposals: <accept / refuse by kind>.

## 21. Intelligence

Agency (yes/no, date), upgrade order, operatives and missions, decryption target. Or: not applicable:
<reason>.

## 22. Occupation

| Phase | Default law | Switch rule (resistance / compliance) | Garrison template | Call |
|---|---|---|---|---|
| <phase> | <law> [V occupation-laws] | ≥ 50% resistance → <law>; ≥ 40% compliance and < 25% resistance → <law> | <name × n> | `hoi4_occupation_law {"law":"<law>"}` |

## 23. Peace aims

| Enemy | Ranked demands (take / puppet / release) | Scripted peace offers and white peace: answer | Calls |
|---|---|---|---|
| <TAG> | <states in order> [V vp] | `<event>`: `<option_name>` if `<predicate>` | `hoi4_peace_take`, `hoi4_peace_submit`, `hoi4_peace_exit` |

## 24. Continuous management

| Rule (procedure item 28) | Value for this campaign | Key / brief line |
|---|---|---|
| <rule> | <threshold> or default (<reason>) | `alarms` / `war.<key>` / `manage` (pending) / brief: <read, call> |

## 25. Coverage table

Every row is resolved: `executor: <key>`, `player: <key> call <tool>`, `player (key pending: <key>):
<tool>`, or `not applicable: <reason with a fact>`. No empty row, no "later".

| # | Item | Resolution | Where | Facts used | Check |
|---|---|---|---|---|---|
| 1 | Goal | executor/player: `goal` | §0 | `facts.py vp`, `hooks`, `missions` | lint win.*; rubric 1 |
| 2 | Situation | `war.enemies`, `phases` | §1, §7 | dossier §1-4, §8-9 | rubric 2 |
| 3 | Force posture | | §12 | | rubric 12 |
| 4 | Doctrine path | | §13 | | rubric 13 |
| 5 | XP budget | | §13 | | rubric 12 |
| 6 | Templates | | §14 | | rubric 13 |
| 7 | Equipment designs | | §15 | | rubric 6 |
| 8 | Production | | §5 | | rubric 6 |
| 9 | MIOs | | §15 | | rubric 6 |
| 10 | Economy timing | | §4 | | rubric 4 |
| 11 | Research | | §3 | | rubric 5 |
| 12 | Focus and balance of power | | §6, §16 | | rubric 3 |
| 13 | Gated laws | | §2.1 | | rubric 3, 10 |
| 14 | Stability and war support | | §16 | | rubric 9 |
| 15 | Advisors and high command | | §2.1 | | rubric 10 |
| 16 | Army organisation | | §2 | | rubric 6, 8 |
| 17 | Manpower | | §16 | | rubric 6 |
| 18 | Operations | | §7, §8 | | rubric 7, 8 |
| 19 | Supply and logistics | | §4, §17 | | rubric 7, 14 |
| 20 | Air plan | | §18 | | rubric 14 |
| 21 | Naval plan | | §19 | | rubric 14 |
| 22 | Diplomacy | | §20 | | rubric 10 |
| 23 | Occupation | | §22 | | rubric 15 |
| 24 | Intelligence | | §21 | | rubric 15 |
| 25 | Peace aims | | §23 | | rubric 15 |
| 26 | Events and decisions | | §9, §10 | | rubric 9 |
| 27 | Checkpoints and contingencies | | §1, §9 | | rubric 9 |
| 28 | Continuous management | | §24 | | rubric 16 |
| 29 | FAIR boundary | tags in this file | all | `rules-and-modes.md` | rubric 2 |

## 26. Sources

Commands run with their key outputs (`facts.py ...`, `calc.py ...`), and the playbook references used.
