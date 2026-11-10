# Mid-game revision

The game never pauses for a revision. The executor keeps running the current plan while you work, and
its `phases` and `contingencies` cover the gap. At speed 3 a game day is about 2.5 to 3 s, so five
minutes of thinking cost about 100 to 120 game days: write the plan so the executor can switch at once
without you.

## 1. Triggers

Detected by the server or broker, never by player judgement; each arrives as a feed change kind and
starts `scripts/strategist/run.py --role strategist --revision <mode> --run RUN --prompt skills/hoi4-strategist/prompts/revise.md`.

| Trigger | Detection | Mode |
|---|---|---|
| `checkpoint_missed` | the executor evaluates `checkpoints[].must` at `by` | full |
| `war_started` / `war_declared_on_us` / `ally_called` | `hoi4_wars` enemy set changes | full |
| `capitulated` / `peace_concluded` | wars list and events (an enemy, an ally or a war ended; peace conference closed) | full: next goal or the post-war phase |
| `army_lost` | divisions in a theatre fall more than 30% in 10 game days, or an army is gone | quick |
| `own_core_lost` | `own_state_occupied` alarm on a core state | quick (contingency first) |
| `phase_exit` | a phase `exit` holds and the next phase has `revise_on_enter` | full |
| `plan_exhausted` | `research_slot_idle_plan_exhausted` / `no_active_focus_plan_exhausted` alarms | quick (extend lists) |
| `construction_item_blocked` | the executor gave up a `construction_plan` item after its tries (feed change and alarm, with the last refusal) | quick: replace the item with another way to the same purpose (another start or a shorter line, a port, trucks, a hub) |
| `army_out_of_supply` | a driven army stayed below `war.supply_floor` for `war.relief_report_days` after the executor's own railway and truck levers (feed change and alarm) | full: pace the axis to its supply reach, bring supply forward, or pull divisions back |
| `periodic` | every 6 game months in peace, every 2 in war | full |
| `player_request` | the player sends `hoi4_plan {op:"request_revision", reason}` | quick |

## 2. The packet (`RUN/revision_packet.json`)

Built by `scripts/strategist/packet.py` (build item 21) from lease-free reads. It holds:
* `mode` (quick | full), `trigger` with its evidence;
* the current plan, its `_meta.version`, and the diff since version 1;
* a state snapshot: date, economy, factories, PP, research done and running, focuses done, laws,
  states owned and controlled with changes since the last revision, VP share per enemy, armies (size,
  supply, organisation, location), wars, open alarms;
* executor digest lines since the last revision: actions, the failure ledger, `focus_blocked`,
  construction digest;
* the player log tail;
* the checkpoint status table;
* the cached dossier plus a delta (ownership changes).

The snapshot is the truth about the present; the dossier is the truth about rules and the map. A
packet flag "previous revision raised failure alarms within 10 game days" means look at that revision
first.

## 3. Modes

* **quick**: extend lists (research, focus, construction), retarget `vp_targets` or guards, switch a
  phase. Smaller packet, no critic, target 3 minutes wall or less. Do only what the trigger needs.
* **full**: the whole procedure from step 2 (goal, situation, skeleton, domains, emit, validate, lint,
  critic). The dossier is reused (no step 1) unless mods or DLC changed. The plan is checked against the
  state at the time it is applied, not at the time the packet was built.

Record the wall time of the revision in `revision.md` (section 5). If quick revisions take more than
about 60 game days, say so there; do not change the configured effort yourself.

## 4. Safe application protocol

You write a candidate; the apply step (`scripts/strategist/revise.py`, build item 21) applies it. Write
the candidate so it passes every check below.

1. **Validate against now.** Schema, `validate_plan.py --strict`, `lint_plan.py` with 0 errors, and these
   **revision invariants** (each violation is an error):
   * no `war.guard_states` entry or guard layout removed while an enemy borders that state (a past run
     lost two border states after guards were removed mid-run);
   * no completed focus, researched tech or taken VP target listed again as pending;
   * `construction_plan` items keep their `id`; new items get new ids; never reuse an `order` or an id;
   * no new general for an army that already has one (a reassignment resets the general's bonuses);
   * no army claimed by the player (`war.player_claim_seconds`) changes role;
   * no target refers to a state the snapshot says is already ours;
   * every unchanged item keeps its `id` (the executor keys its done-memory on ids).
2. **Compare-and-swap.** `hoi4_plan {op:"replace", plan, if_version:N}` writes only when the live
   `_meta.version` is still N (build item 20). On a conflict, rebuild the candidate from the newer plan
   and re-check. Until `op:replace` exists the apply step reads `hoi4_plan {op:"get"}`, compares
   `_meta.version`, and writes the changed top-level keys with `{op:"set"}`; that is not atomic, so it
   re-reads after the write and reports a mismatch.
3. **Version.** The candidate has `_meta.version` = N + 1 and `_meta.parent` = N, a new `created`, the
   same `dossier_hash` unless the dossier changed, and a fresh `lint_summary`.
4. **Announce.** The apply step emits the feed change `plan_revised {version, summary, brief_changed}`,
   journals the diff and installs the re-rendered `plan.brief.md`. The player re-reads the brief on
   that event only, so the brief must say what changed in its first lines.
5. **History.** The previous plan is kept as `RUN/plan.v<N>.json`. There is no automatic rollback:
   rolling back mid-war can be worse.

## 5. Output of a revision

Write into `RUN/revisions/v<N+1>/`:
* `plan.json`: the full candidate (not a patch);
* `plan.md`: the updated human plan (change the affected sections; §11 lists the revision; the
  coverage table §25 is re-checked for every checklist item the change touches, and in a full
  revision for all 29);
* `plan.brief.md`: the re-rendered brief, ≤ 60 lines, first line "Revised v<N+1>: <one-line summary>";
* `revision.md`: trigger, mode, what changed and why (one line per changed key), invariants checked,
  lint summary, wall time, and anything you could not fix.

If a full revision's critic fails twice, write `revision.md` with the open findings and leave
`plan.json` out; the executor keeps the current plan and its contingencies.
