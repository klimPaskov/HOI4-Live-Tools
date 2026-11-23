# Live validation queue

What must still be shown in the running game (HOI4 1.19.4.1, macOS), in an order where one loaded save serves
many probes. Each probe names its question, its calls and the read that answers it. Tick a probe with the run
that answered it and move the tool's evidence into `LIVE_VERIFICATION_1_19_4.md`. Navy, sea landings and trade
have their own queue.

## Proof state of the registered tools (23 November 2026)

Counted from the action receipts and read replies of every recorded run since the 1.19.4 migration (31
October): 181 registered tools.

| proof state | player tools | diagnostic tools |
|---|---:|---:|
| a Verified receipt or an answered read on 1.19.4.1 | 124 | 2 |
| documented live 31 Oct - 8 Nov only | 12 | 3 |
| live only on 1.19.3 | 6 | 0 |
| tried, never succeeded | 7 | 4 |
| never run | 5 | 18 |

Live but failing more often than working: hoi4_operative_assign, hoi4_operation_start, hoi4_agency_upgrade,
hoi4_general_trait (no pickable trait), hoi4_template_create (layout not saved), hoi4_doctrine_adopt (the
special-forces folder), hoi4_army_area_defense, hoi4_army_halt, hoi4_design_create, hoi4_save,
hoi4_research_start and hoi4_event_choose (mostly argument faults, eased in this change).

Tried, never succeeded: hoi4_mio_policy, hoi4_mio_design_team, hoi4_faction_create, hoi4_faction_leave,
hoi4_naval_invasion, hoi4_trade_set (refused while auto import is on, by design), hoi4_deploy_locations.
Never run: hoi4_faction_invite_accept, hoi4_fleet_balance, hoi4_fleet_delete, hoi4_peace_options (with a
conference open), hoi4_template_delete. Live only on 1.19.3: hoi4_air_hold, hoi4_air_wing_merge,
hoi4_design_obsolete, hoi4_special_project_recruit, hoi4_special_project_stop,
hoi4_special_project_unassign.

## Session 1: Germany at war, May 1940 (about 95 minutes, paused unless said, executor off)

1. New arguments (5 min): `hoi4_event_choose` with `option` set to one of the event's `option_keys`;
   `hoi4_research_start` without `slot` while a slot is empty; `hoi4_air_mission` with `region` as a number;
   `hoi4_politics_options {category:"advisor", slot:3}`.
2. Espionage (15 min): `hoi4_operative_assign` one operative to counter-intelligence and one to an intel network
   in an enemy state, the agency view watched; `hoi4_intelligence` two game days later; `hoi4_operations` and,
   when ready, `hoi4_operation_start` (collaboration government). Question: which click or read-back fails.
3. Design companies (10 min): `hoi4_mios`, `hoi4_mio_policy` on one organisation with the view watched,
   `hoi4_mio_design_team` once. Question: where the organisation list lives on 1.19.4.1.
4. Templates (8 min): `hoi4_template_create` one infantry template with the designer watched. Question: why the
   layout is not saved.
5. Special-forces doctrine (5 min): `hoi4_doctrines`, then `hoi4_doctrine_adopt special_forces_quantity` with
   the doctrine window watched. Question: where the folder's slot is.
6. Command power (5 min): the top bar's command power tooltip (value and cap) beside `hoi4_country_status`;
   `hoi4_general_trait {general, list:true}` for three generals. Question: the real cap; whether any trait can be
   picked.
7. Defensive orders (10 min): `hoi4_army_fallback`, `hoi4_army_area_defense`, `hoi4_army_halt` on three armies,
   `hoi4_armies` after each. Question: why area defence reads no battle-plan orders after the click.
8. Special projects (8 min): `hoi4_special_project_recruit`, `_unassign`, `_assign`, `_stop`.
9. Air missions never flown on 1.19.4 (15 min): one wing each for interception, logistics strike, port strike,
   naval strike and strategic bombing, `hoi4_air` after each.
10. Events left open (5 min, speed 2): when a news event opens, `hoi4_event_choose` with the window watched.
11. Saves (10 min, speed 2): ten `hoi4_save` calls a game day apart while events arrive; first-try successes and
    each failure's code.
12. Focus (10 min): executor on with only the focus lane for a game week; when a start is not applied, read
    `hoi4_focus_available` for it and look at the tree.

## Session 2: a short war that ends in a peace conference (about 40 minutes)

A save where a small country falls alone (a 1936 start with a war goal on an isolated minor).

13. Peace (30 min): executor on with take, puppet and liberate in the plan's peace keys; `hoi4_peace_status`,
    `hoi4_peace_options` with the conference window open, then `hoi4_peace_take`, `hoi4_peace_submit`,
    `hoi4_peace_exit`. Note the force-government control (not built yet).
14. Occupation (10 min): `hoi4_occupation`; name the Occupied Territories view's garrison template control and
    its per-state law control (both not built yet).

## Session 3: factions and proposals (about 20 minutes)

15. `hoi4_faction_create`, `hoi4_faction_leave` (watch why its read-back fails), `hoi4_faction_invite_accept`
    on an invitation, `hoi4_diplomacy_incoming` while a proposal is open.

## Session 4: BATCHING PROBES (about 85 minutes)

Each bulk path below is off until its probe passes (`manage.bulk` in the plan, or `bulk: true` on the call);
the single path stays in use until then. A passed probe adds its name to the plan's `manage.bulk`. Seconds
are wall seconds from the receipt (dispatch to receipt) and from the `[order]` timing lines; the situation
report's cleanliness block shows calls, elements changed and seconds per element per action kind.

The May 1940 Germany bed source (1 May 1940, paused, executor off unless said):

16. Factory chords (5 min): `hoi4_production_lines`; a line with a few factories:
    `hoi4_production_set {line, equipment, factories: current + 23, bulk: true}`, then back to the old count
    with `bulk: true`. Expected: Verified both ways, 5 clicks each way (two Shift, three plain; the phase
    timing shows the click count), no trace "chords off". Before: 1.3 s + about 0.3 s a factory (23 factories
    about 8 s); after about 2.5 s. When a chord moves the line by one only, the game on macOS reads another
    modifier: note which (Ctrl or Command) and leave `production_chords` off.
17. Air missions by base (10 min): `hoi4_air`; a base whose wings are all fighters (three or more) and one
    fighter at a base it shares with another type. `hoi4_air_mission {wings: [the base's wings and the shared
    one], mission: "air_superiority", region, bulk: true}` with the map watched. Expected: Verified; the result's
    `by_base` row for the clean base ok, the shared base's wing ordered by the single path; one base click, one
    mission click and one region click for the clean base. Before: 2.5-5 s a wing; after about 4 s a base. The
    same call again clicks nothing (every wing flies it already).
18. Air rebase by base (5 min): `hoi4_air_rebase {wings: [every wing of one base], state_id: another own base
    with room, bulk: true}`. Expected: each wing's transfer_to_base_id is the target; one base click and one
    right click. Before: 1.8-3.4 s a wing; after about 3 s a base.
19. Selection questions for the next step (5 min, by hand, nothing kept): in air mode, Shift+Click a second
    base circle while one base is selected (are its wings added?), and Shift+Click (then Ctrl+Click) a wing row
    of the selection view (`airwing_selection_item`): is that wing taken out of the selection? Either answer lets
    mixed bases join the bulk path.
20. Design-company traits in one visit (10 min): `hoi4_mio_trait {organization, list: true}`, then
    `hoi4_mio_trait {organization, trait_indices: [three enabled, visible traits], bulk: true}` with the tree
    window watched; then `hoi4_mio_trait {organization: another, auto: 2, bulk: true}`. Expected: Verified, the
    three traits read queue numbers in the result's `entries`, the second organisation has two enabled traits
    queued. Before: 8.3 s a trait plus 8 s for a listing visit; after about 10 s a visit. Note how often the
    detail window had to be opened again after a click.
21. Executor bed with every bulk path (25 min): a STUDY bed of 7 war days from the same source with the plan's
    `manage.bulk: ["all"]`, hands off; compare its cleanliness table with a bed of the same plan without it
    (air_mission, air_rebase, production_set, mio_trait: calls, elements changed, s per element). Expected: air
    orders at least three times fewer calls, no order repeated after a Verified read-back.

A 1936 start (Germany, paused unless said):

22. Construction fill (5 min): `hoi4_state_slots` for a state with three or more free slots for arms_factory;
    `hoi4_construction_add {building: "arms_factory", state_id, count: <free slots>, fill: true, bulk: true}`.
    Expected: Verified, the state's queue grew by the free slots after one Shift+Click. Before: about 1 s a
    building after the first (a 600 ms settle and a read); after one click.
23. Peace years with every bulk path (20 min): executor on with `manage.bulk: ["all"]` for 60 game days at speed
    3; the cleanliness table against the 1936 campaign baseline (production_set about 1.3 s a line, mio_trait
    8.3 s a trait, construction_add 2.9 s a state).

Total: about 240 minutes in four sessions.

## Session S0: the in-process module, stage 0 (read-only; about 5 minutes, then a 30-minute soak)

The owner lifted the "no memory writes" rule for this module only (23 Nov). Stage 0 writes nothing and calls
nothing in the game: the module scans the game's image in memory, finds the game-state and idler roots by their
RTTI type, and logs three reads. It is loaded only through the opt-in `HOI4_INPROC` switch of the harness launcher
`launch-probe-cont.py` (default off: every other run is unaffected); no game file is changed. Design, evidence and
the later stages: `~/Projects/_scratch/hoi4-macos-native/INPROCESS-FEASIBILITY-1123.md`; code: `inproc/` on branch
`research/inproc-feasibility` (build `inproc/build.sh`, offline proof `inproc/test_offline.sh`).

Needs: the game slot, no game running, Steam closed, a run folder that saved (`PREV/userdata/continue_game.json`).

Part A (automatic, about 5 minutes):

    cd ~/Projects/_scratch/hoi4-macos-native
    ./inproc_stage0.sh PREV s0a      # optional 3rd argument: the module dylib (default: the branch's build)

Pass when `s0a/stage0-result.json` has `"pass_": true`: the module loaded and did not go inert; 8 of 8 capabilities
resolved; dlsym finds the same research-command factory as the scan; the idler root found by RTTI is the global the
UI posts commands through; the save loaded; and the module's date (raw and text), speed, pause and player equal the
external runtime's `hoi4_overview` on the paused game. Also look at the main menu and the loaded save on screen: both
must look and behave as usual (the module draws nothing).

Part B (soak, 30 minutes, by hand, same game): start the broker as usual (`./deploybin.sh s0a bins/<build>`), then
`macplay.py s0a call hoi4_time '{"speed":4}'` (the game's speed 5) and `'{"paused":false}'`. Let it run 30 minutes
with no orders; the game's monthly autosave must happen (a new autosave in `s0a/userdata/save games`). Every few
minutes compare `tail -1 s0a/inproc.log` with `macplay.py s0a call hoi4_overview` (dates within a few game days at
speed 5). Then `./endrun.sh s0a`.

Pass when: no crash or hang, the date kept advancing, an autosave was written, `inproc.log` ends without an
`inert` event and is under 8 MiB, `s0a/closed.json` shows the game exited and settings restored, and no new
`hoi4` crash report appeared (`ls -t ~/Library/Logs/DiagnosticReports | head`; the game's own crash folder in
`s0a/userdata` is empty).

STOP at once (`./endrun.sh s0a`) on: any Part A check false; the game freezing, stuttering or crashing; the date
not advancing while unpaused; an `inert` event; anything on screen that differs from a normal run. A stop is a
finding, not a retry: record it in NOTES-1116.md and leave stage 1 closed. Kill switch without closing the game:
`touch s0a/inproc.log.off` (the module goes inert within 2 s).

After a pass: tick here, record the numbers in NOTES-1116.md ("stage 0 passed"), and stage 1 (one harmless command
through the game's own command queue, with read-back) may be built.
