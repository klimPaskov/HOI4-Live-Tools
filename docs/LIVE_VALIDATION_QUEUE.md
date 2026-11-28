# Live validation queue

What must still be shown in the running game (HOI4 1.19.4.1, macOS), in an order where one loaded save serves
many probes. Each probe names its question, its calls and the read that answers it. Tick a probe with the run
that answered it and move the tool's evidence into `LIVE_VERIFICATION_1_19_4.md`. Navy, sea landings and trade
have their own queue.

## The in-process module: family state (28 November 2026, macOS 1.20.1.0)

The module (`inproc/`, see `inproc/README.md`) is macOS only, single player only, and loaded only by a
maintainer's launcher. Under `HOI4_BACKEND=auto` (the default), a family goes in process only when its
`IPC_LIVE_PASSED` entry in `inproc/core/ipc.c` is 1. Otherwise, and on Windows and Linux, the tool takes
its mouse path.

| family | session | state |
|---|---|---|
| set_game_speed | S1 | passed 23 Nov |
| production, air | S2 | passed 24 Nov |
| airwing, production2 | S3-AIRWING, S3-PROD2 | passed 28 Nov (1940 and 1936 beds) |
| orders, orders2 | S4-ORDERS, S5-PLAN | passed 27 Nov |
| units | S6-UNITS | passed 24 Nov |
| picks | S7-PICKS | passed (F1/F2a/F4 24 Nov, F3 28 Nov) |
| events | S8-EVENTS | stopped 24 Nov: the answer executes, the window stays open |
| decisions, diplomacy | S9, S10 | passed 24 Nov |
| holds, research, politics | S11, S12, S13 | passed 24 Nov |
| construction | S14-CONSTRUCTION | passed 28 Nov (a count over the free slots reads Diverged) |
| training | S15-TRAINING | stopped 28 Nov: the line count was not exact, and a refused add left a queue. The remainder-per-read fix is merged, and its re-run is owed |
| mio | S16-MIO | passed 27 Nov |
| navy | S17-NAVY | passed 28 Nov |
| doctrine | S18-DOCTRINE | not passed: takes the mouse path |
| projects | S19-PROJECTS | not run |
| navyhq | S20-NAVYHQ | not runnable yet: the command is found, but no read gives a headquarters' address |
| move | S20-MOVE | not run: division moves by command (hoi4_army_move), off under `auto` |
| leader | S31-TRAIT | not run: a general's trait by its key and the promotion to field marshal (hoi4_general_trait trait / promote:true), off under `auto` |
| faction | S32-FACTION | not run: a faction goal by its key and the spymaster role (hoi4_diplomacy_act faction_goal / become_spymaster), off under `auto` |
| decrypt | S33-DECRYPT | not run: start / stop decrypting a country and activate a broken cipher (hoi4_agency_upgrade decryption), off under `auto` |
| multiplayer | S1-MP, S2-MP | not run (needs two clients); every post refuses outside single player |

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
the later stages: `INPROCESS-FEASIBILITY-1123.md` in the harness folder; code: `inproc/` on branch
`research/inproc-feasibility` (build `inproc/build.sh`, offline proof `inproc/test_offline.sh`).

Total: about 325 minutes in five sessions.
34. Air wings attached to an army (5 min): `hoi4_air_attach {wings, army}`.
    DONE v65m (23 Nov 2026): 2/2 CAS wings attached to Armee West Nord in 5.3 s, Verified from the wing's attached-army link (+0x950); the repeat call already:true in 19 ms, no input. Wings in pilot exercises are refused by the game.
    DONE w69d (24 Nov 2026, 7-day war bed from a 1 May 1940 save, executor `manage.air_attach`): by war day 7 twelve wings
    read attached from memory: Panzergruppe Ost 4 CAS + 1 fighter wing, Panzergruppe Nord 4 CAS + 1 fighter wing, Armee
    Pommern 2 CAS. As a plain military lane the rule first acted on war day 14 (two 21-day beds); it now runs at the
    head of the war air step.

Needs: the game slot, no game running, Steam closed, a run folder that saved (`PREV/userdata/continue_game.json`).

Part A (automatic, about 5 minutes):

    cd <harness folder>
    ./inproc_stage0.sh PREV s0a      # optional 3rd argument: the module dylib (default: the branch's build)

Pass when `s0a/stage0-result.json` has `"pass_": true`: the module loaded and did not go inert; every capability
resolved (8 at stage 0, 11 with the stage-1 build); dlsym finds the same research-command factory as the scan; the idler root found by RTTI is the global the
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

- [x] S0 PASSED 23 Nov (run s0a): 8/8 capabilities, scan 81 ms, reads equal hoi4_overview, 30-minute soak at speed 5
  over 3.7 game years clean, exit clean.

## Session S1: the in-process module, stage 1 (ONE command: set_game_speed; about 6 minutes, then two 15-minute soaks)

Stage 1 posts exactly one command type, `CSetGameSpeedCommand` (the game's speed, index 0..4 = speeds 1..5), through
the game's own command path, the way the UI's speed keys do: the module queues a job onto the process main thread
(the thread whose event pump delivers the UI's input), checks there the game's own thread rule
(`_ThreadForbidCount == 0`), that the session is single player (server `CDummyServer`) and that Post will stamp the
sender this command's authorisation requires, creates the command with the game's exported factory, fills its one
field, and calls the game's Post. The game stamps the sender and executes the command on its next command step; the
module never writes game state. Posting is OFF unless the launcher passes the explicit opt-in
(`HOI4_INPROC_CHANNEL=1 HOI4_INPROC_POST=set_game_speed`, set only by `inproc_stage1.sh`) AND every stage-1 capability
resolved on the running build. Allow-list: one command. Rate limit: 200 ms apart, 60 a minute, one in flight. Every
post is logged in `RUN/inproc.log` (`"event":"post"`: type id, fields, result, timings). Any refusal the module cannot
explain switches posting off for the rest of the process (`"event":"posting_off"`). The local channel is a Unix
socket in the 0700 run folder, owner-only, peer-uid checked, plus a per-process token file. Design and evidence:
`INPROCESS-FEASIBILITY-1123.md` section 8; the offline proof `inproc/test_offline.sh` (ALL PASS) covers the scans on
1.19.4.1, 1.20.0.0 and 1.19.3, the channel and policy against a mock game, and the main-thread route in an
SDL-style Cocoa event pump.

Needs: the game slot, no game running, Steam closed, a run folder that saved, the branch's module built
(`inproc/build.sh`; the scripts check it is the stage-1 build). Python client: `inproc/tools/inproc_client.py`
(copied into the run by the script).

Part A (automatic, about 6 minutes; the game loads PAUSED):

    cd <harness folder>
    ./inproc_stage1.sh PREV s1a

It prints the module's capability, policy and channel lines, then runs: hello; probe (no post); one post (speed
index 2, or 1 if already 2) read back by the module AND by the external runtime's `hoi4_overview`; a screenshot
`s1a/stage1-after-post.png`; the undo (back to the starting speed) checked the same way + `stage1-after-undo.png`;
20 repetitions of post + undo (40 posts) with timings. Pass when `s1a/stage1-result.json` has `"pass_": true`:
posting enabled with an allow-list of one; the probe ran on the main thread with the session `8CSession` and the
server `12CDummyServer`; every post `executed` with `speed_after` equal to the request; the external read equal after
the post and after the undo; 40 of 40 repetitions; the module's read-back median within one frame-time budget
(33 ms: two frames at 60 fps); the speed restored. Look at both screenshots: the top bar's speed indicator shows the
posted speed, then the original one (record what you see in the notes). Record from `timing`: `post_us` (the
game's Post call, microseconds), `main_wait_ms` (queue to main thread), `readback_ms`, `total_ms`, against today's
1.3-10 s per GUI order.

Part B, first soak (15 minutes, same game):

    zsh inproc_stage1_soak.sh s1a 15 7

Speed 5, unpaused; once a minute the module posts speed index 3 then 4, each read back; at minute 7 a manual save
through `hoi4_save`; at the end pause and a second manual save (the file the reload uses). Then `./endrun.sh s1a`.
Pass when `s1a/stage1-soak-result.json` has `"pass_": true` (about 30 posts, none failed), the watch lines show the
date advancing and the overview's speed 4 between posts, a manual save appeared, no `posting_off`/`inert`.

Reload in the middle (with the module): `./inproc_stage1.sh s1a s1b quick` (loads s1a's last save, the one made with
the module; probe + one post + undo: `s1b/stage1-quick.json` `"pass_": true`), then the second soak
`zsh inproc_stage1_soak.sh s1b 15 7` (the monthly autosave must appear in `s1b/userdata/save games`), then
`./endrun.sh s1b`.

The save loads in an UNMODIFIED game: `./inproc_stage1.sh s1b s1c plain` launches WITHOUT the module (no
`HOI4_INPROC`; the script checks no `inproc.json`/`inproc.log` exists in `s1c`) from s1b's last save (made with the
module after 30 minutes of module posts) and reads `hoi4_overview`. Pass when `s1c/plain-result.json` has
`"pass_": true` (the save loaded, the date and player read, no load error in the game log). Look at the screen: a
normal game. Then `./endrun.sh s1c`. (The module never writes game state and changes no data file, so the save
holds only what the game's own speed command wrote; this proves it on a clean process.)

STOP at once (`./endrun.sh RUN`) on: any check false; `posting_off` or `inert` in `RUN/inproc.log`; a crash, hang or
frozen date while unpaused; anything on screen that differs from a normal run (the module draws nothing). A stop is
a finding: record the module log's last `post`/`probe` lines in NOTES-1116.md and leave stage 2 closed. Kill switch
without closing the game: `touch RUN/inproc.log.off` (no new posts, the channel closes, within 2 s).

After a pass: tick here, record the numbers in NOTES-1116.md ("stage 1 passed"); the multiplayer check (Session S1-MP)
and stage 2 (INPROCESS-COMMANDS-1123.md) may follow.

## Session S1-MP (later; needs two clients on two machines): the in-process module in multiplayer

Not tonight. Stage 1's module REFUSES to post unless the server is `CDummyServer` (single player), so this session
first needs a build with the multiplayer gate opened for the host role only (same allow-list; speed is host-only in
the game: "only the host may change the game speed").
Roles: HOST = the Mac with the module (opt-in as S1), CLIENT = a second machine with an unmodified game (Linux VM or
a second Mac; its own licence), same version and checksum (the game's lobby shows both). Steps: host starts a
multiplayer game from a save; client joins in the lobby; play at speed 3 for 10 minutes with the module posting
speed 3 -> 2 -> 3 once a minute; then the client leaves and HOT-JOINS again while the game runs (the module must hold
posts while "ongoing hotjoin": a post refused by the game is logged, never retried blindly); then 60 minutes more.
Proof of no desync: the game's own out-of-sync indicator never appears on either screen; both `game.log`s show the
hourly checksum (`CClientChecksumCommand`) accepted and no "NOT in sync", "Validity of command changed
non-deterministically", "COMMAND LOST" or "Dropped" lines; the host never flags the client modified
(`CMarkModifiedClientCommand`); the speed changes appear on BOTH clients' top bars at the same game hour. Then the
client role with the module (its speed posts must be refused by the host's game, exactly like a client pressing
the speed keys: "only the host may change the game speed").

## Session S2: the in-process module, stage 2 (production and air families; about 70 minutes plus a 30-minute soak)

Stage 2 posts the commands that carry real play the way the game's own UI posts them: the command type's factory
(or the UI's own constructor for payloads with lists), the game's own "can do" check (the vtable slot the UI's post
helpers call before posting; refused = the module deletes the command, as the UI does), then `Post(session, cmd, 0)`
from the main thread. Before any command is made, every object the tool names must round-trip through the command's
own typed resolver (a live object of the right class) and belong to the player by the command's own owner getter
(both read from the command's authorisation slot). Five types: `production.add_factories`
(`CAddProductionLineFactoriesCommand`), `production.remove_line` (`CRemoveProductionLineCommand`), `air.transfer`
(`CStratAirTransferCommand`), `air.set_mission` (`CStratAirSetMissionCommand`), `air.enable_mission`
(`CStratAirEnableMissionCommand`). The MCP tools keep their names, schemas and Verified read-backs; the reader's
switch `HOI4_BACKEND=auto|inprocess|gui` picks the route (`auto` uses a family only once it is marked live-passed in
the module; until this session passes, only `inprocess` uses it). A refused or unanswered in-process request hands the
same call to the GUI path. Design and evidence: `INPROCESS-COMMANDS-1123.md` (stage 2 built) and
`INPROCESS-FEASIBILITY-1123.md` section 10; offline proof `inproc/test_offline.sh` ALL PASS (stage-2 scans 5/5 on
1.19.4.1 and 1.20.0.0 macOS, off on 1.19.3) and `cargo test --lib inproc` (the switch).

Needs: the game slot, no game running, Steam closed; the module built from the branch (`inproc/build.sh`, version
`stage2-1`); the READER built from the branch (it carries the switch) and named by `HOI4_BIN`; two saves: PREV-1940 (the
May 1940 Germany bed of Session 1) and PREV-1936 (any 1936 start that saved, a few days in).

Start (each bed): `./inproc_stage2.sh PREV s2a` (s2b for the 1936 bed). It launches with the opt-in
`HOI4_INPROC_POST=set_game_speed,production,air`, checks the hello (both families on), and writes `s2a/tool.sh`:
`zsh s2a/tool.sh gui|inprocess TOOL 'JSON-ARGS'` runs one tool with that backend and logs its body and wall time to
`s2a/tools.jsonl`. Take an input lease as usual (`hoi4_input_lease`) for the GUI calls; the in-process calls still go
through the same lease and journal.

P1 production, paused (10 min). Read the lines (`hoi4_production_lines`). For three lines of different equipment: `inprocess`
`hoi4_production_set` to requested+3, then back; then to 0 on a spare line (removes it). Each call must return
Verified (the line's factories in memory) and the module's `post2` lines in `s2a/inproc.log` must show
`"result":"posted"` and `"executed"` equal to `"posted"` (the module reads the line's factories itself; while paused it
reports "not executed yet (game paused)": then unpause one hour and read again). Then the same three changes with
`gui`. Record both wall times per call.
P2 production refusals (5 min): a delta that would go below 0 or above the line's cap must come back
`refused_by_game_check` (the game's own check), never posted, and the GUI path then refuses the same way; an address
that is not an own line (another country's line, if the reads expose one) must come back `not_own`.
P3 air rebase, paused (10 min): three own wings at one base: `inprocess` `hoi4_air_rebase` (`wings` = the three,
`state_id` = another own state with an air base) must be Verified (transfer base in memory); then back with `gui`.
A wing already at the target must be skipped (no command). A target state with no own base must be refused before
posting (the reads plan it).
P4 air missions, running at speed 3 (15 min): `inprocess` `hoi4_air_mission` with three fighter wings, `mission`
`air_superiority`, a region that holds an own base: one `air.set_mission` request (3 commands) then one
`air.enable_mission` request (3 commands), Verified (mask and region in memory); then the same wings to
`cas` in another region; then `gui` for the reverse. A region with no own base goes through the GUI
by design (the region object is only known from an own base); check that it does.
P5 timings (5 min): 20 alternating `hoi4_production_set` calls per backend, and 10 `hoi4_air_rebase` calls of 6 wings per
backend; report the median and the 90th percentile of the tool's wall time and the module's `total_ms`.
P6 war, 20 game days twice (20 min): from PREV-1940 at speed 3, the usual war loop for 20 game days with
`HOI4_BACKEND=inprocess` for production and air, then reload the same save and run the same 20 days with `gui`.
Compare: tool wall time per production/air call, calls per game day, the casualty ratio and territory change.
P7 soak (30 min): speed 3, once a minute one production change and one air mission change in process, each Verified;
one manual save in the middle and a reload of it (the channel must come back and the families must be on again).
P8 save without the module: `./inproc_stage2.sh s2a s2p plain` (the save made in P7) must load and read in an
unmodified game.

Stop at once (`endrun.sh RUN`) on: a crash or hang; any `posting_off`, `family_off` or `inert` event; a `post2` result
`unexplained`; an in-process call whose Verified read-back fails while the GUI path's succeeds; any "Dropped command"
or "posted during command execution" line in the game's `game.log`; a save that does not load in P8.

Pass: P1-P8 clean on both beds. Then set `IPC_LIVE_PASSED` for production and air in `inproc/core/ipc.c` (that turns
`auto` on for them), record the numbers in NOTES-1116.md ("stage 2 passed: production, air"), and the next families
follow (MIO, construction, training, air wing creation: INPROCESS-COMMANDS-1123.md).

- [x] S2 PASSED 24 Nov (runs s2c-s2i, w69b/w69c): production and air on the 1940 and 1936 beds; soak 30 min with 60
  in-process changes Verified (production median 47.6 ms, air missions 122.7 ms); reload with the families on; the
  save loads without the module; 21-day wars in process vs GUI 2.3 vs 1.7 actions per game day, air orders 12 in 65 s
  vs 3 in 66 s; `IPC_LIVE_PASSED` set for both, `auto` checked live (s2i). Numbers in NOTES-1116.md.

## Session S2-MP (later; two clients on two machines): stage 2 in multiplayer

Not before S1-MP and S2 pass. The module still refuses every stage-2 post unless the server is `CDummyServer`; this
session needs a build with that gate opened for the CLIENT role too (a player's production and air orders are not
host-only). Same roles and checks as S1-MP (no out-of-sync indicator, the hourly `CClientChecksumCommand` accepted,
no "NOT in sync", "Validity of command changed non-deterministically", "COMMAND LOST" or "Dropped" lines, never flagged
by `CMarkModifiedClientCommand`), with the module on the client: P1 and P4 above, each change seen on BOTH machines
(the host's reads of the client's country) at the same game hour.

## Session S3-AIRWING: the in-process module, the rest of air (family `airwing`; about 40 minutes plus a 15-minute soak)

Built by engineer 71 (24 Nov 2026), offline-proven only (`inproc/test_offline.sh` ALL PASS: 6 new types on 1.19.4.1 and
1.20.0.0 macOS, every constructor hand-checked in both disassemblies). One family, OFF under `auto` until this session
passes. Ops (module request -> command, the UI action it replaces, the MCP tool):
`airwing.split` -> `CStratAirSplitCommand(wing, aircraft/2)` (Split button; `hoi4_air_wing_split`);
`airwing.merge` -> ONE `CStratAirConsolidateCommand(list of wings)` (Merge button; `hoi4_air_wing_merge`);
`airwing.disband` -> `CDeleteAirWingCommand(&own tag, wing)` (Disband; `hoi4_air_wing_disband`);
`airwing.attach` -> `CAttachAirWingToArmyCommand(wing, army)` (right click on the army card; `hoi4_air_attach`);
`airwing.detach` -> `CDetachAirWingFromArmyCommand(wing)` (module only; no MCP tool yet);
`airwing.region` -> `CStratAirEnableMissionCommand(wing, region id, NULL)` with the region looked up by id the way the
command's own check reads it (game state +0x2a8 array, +0x2b4 count): a mission over a region with NO own air base
(`hoi4_air_mission`, which used the GUI for those) and region 0 = Hold (`hoi4_air_hold`; a wing flying a transfer stays
on the GUI: the game's hold helper cancels the transfer first).
Wing creation from the reserve (`CDeployAirWingCommand`, 0xa8 bytes, a nested equipment id) is NOT in this family.

Needs: the module built from branch e71/inproc-families (`inproc/build.sh`, sha in NOTES-1116.md "engineer 71"); the
reader built from the same branch (cargo under the lock: `mkdir cargo.lock.d`, then
`CARGO_BUILD_JOBS=2 ./buildbin120.sh <this worktree> v120-e71a` for 1.20 or the usual new-v* build for
1.19.4.1, then `rmdir cargo.lock.d`); the opt-in with the new family:
`HOI4_INPROC_FAMILIES=set_game_speed,production,air,airwing ./inproc_stage2.sh PREV s3a`; calls through the broker
(`zsh deploybin.sh s3a BIN HOI4_BACKEND=inprocess HOI4_INPROC_SOCK=$PWD/s3a/inproc.sock`) as in S2 (s2a/tool.sh lacks
`--mac-input`). Beds: the May 1940 Germany bed (s2c's source save) and a 1936 start.

A1 hold, paused (5 min): two wings on a mission: `hoi4_air_hold` in process -> Verified (region 0 in memory), `post2`
`airwing.region` posted 1; a wing already standing by -> `nothing_to_change` before posting; then a mission again via
`air` and Hold via `gui` for the timing.
A2 region with no own base, running speed 3 (5 min): three fighters `air_superiority` over a region with no own air base
(region 5 in the s2 beds): in process -> `air.set_mission` then `airwing.region` (vals = region id), Verified; then a region
out of range: must come back `refused_by_game_check` (the game's range check), nothing posted, no GUI retry.
A3 split and merge, paused (10 min): split a 100-plane wing -> two wings of 50 at the base, Verified; merge them back
(`hoi4_air_wing_merge` with `with`) -> one wing of 100, Verified; merge of a group of three of one type; a 1-plane wing
split -> `nothing_to_change` before posting. Compare each with `gui`.
A4 attach and disband (10 min): attach 2 CAS wings to an own army in process -> both links read the army (Verified);
attach a strategic bomber -> `refused_by_game_check`; disband a spare wing -> gone from memory, its aircraft in the reserve
(production view); a disbanded wing's address reused in a second request -> `not_a_live_object`.
A5 UI check: screenshot of the Air Forces Overview after A3 and A4 (the split/merged/attached wings as the module left them).
A6 soak (15 min, speed 3): each minute one hold + mission over a no-base region pair and one split + merge pair in process,
all Verified; one save and reload in the middle (the family on again after reload); P8-style plain load of the end save.
Timings: report median/p90 per op in process vs GUI (GUI medians from s2: hold ~8 s, mission ~7.8-25 s, split/merge
~10 s by the S2 pattern).
Stop at once on the S2 stop list (crash/hang, posting_off, family_off, inert, `unexplained`, an in-process call that is
not Verified while the GUI's is, "Dropped command" / "during command execution" in game.log, a save that will not load).
Pass: A1-A6 clean on both beds: set `IPC_LIVE_PASSED` for `airwing` (4th entry in `inproc/core/ipc.c`), rebuild, and
record "S3 PASS: airwing" in NOTES-1116.md with the numbers.

## Session S3-PROD2: the in-process module, new production lines and priority (family `production2`; about 30 minutes)

Built by engineer 71 (24 Nov 2026), offline-proven only (both types on with every check on 1.19.4.1 and 1.20.0.0 macOS;
the add-line constructor hand-checked: 1.20 0x1001ef670, 1.19.4.1 0x1001eef30). OFF under `auto` until this session passes.
`production2.add_line` -> `CAddProductionLineCommand(&own tag, design, NULL, at_top 0)` (the production view's Add with a
design chosen; the line's organisation stays empty: the module cannot name an organisation object yet), then the
factories through `production.add_factories` (live-passed). The command's authorisation is "the sender controls the tag";
the module checks the design with the game's lookup and the C++ cast the "can do" check makes (CEquipmentVariant); the
game's check decides whether the country may produce it. `production2.priority` ->
`CChangeProductionLinePriorityCommand(line, place)` with the absolute place (the UI passes 0 for top, count-1 for bottom).
MCP: `hoi4_production_add` WITH `design` (without it the GUI picks the selection window's first design: stays GUI) and
`hoi4_production_priority` (read-back in process from the country's own line order, no view needed).

Start: `HOI4_INPROC_FAMILIES=set_game_speed,production,air,production2 ./inproc_stage2.sh PREV s3p`; broker as in S2.
B1 paused: `hoi4_designs` -> pick an own design id of a type that has no line; `hoi4_production_add` in process with
`design` and `factories` 5 -> Verified (a new line of that design with 5 factories); the module log shows
`production2.add_line` posted 1, then `production.add_factories` posted 1. Same with `gui` for the timing.
B2 the at-top question: in B1 the new line lands at the BOTTOM of the list (flag 0); record where it lands (if at the top,
the flag's meaning is the reverse: fix the comment, the flag stays 0).
B3 refusals: a design of another country (if one is readable) -> `not_a_live_object`/`refused_by_game_check`; an
obsolete or unproducible design -> `refused_by_game_check`, nothing posted.
B4 priority: with 6+ lines: `hoi4_production_priority` top, down, up, bottom in process -> each Verified; compare the
country's own order (the in-process read-back) with the production view's shown order (open the view, screenshot): they
must agree; if the own list order is NOT the priority order, the route is wrong: stop and record it.
B5 soak 10 min at speed 3: one add (then remove via hoi4_production_set factories 0) and two priority moves per minute,
all Verified; save, reload, plain load.
Pass: B1-B5 clean on the 1940 and a 1936 bed -> `IPC_LIVE_PASSED` for `production2` (5th entry), "S3 PASS: production2".

## Session S4-ORDERS: the in-process module, battle plans (family `orders`; about 60 minutes plus a 20-day war twice)

Built by engineer 73 (24 Nov 2026), offline-proven only: the order-tree facts and the four types are on with every
check on 1.20.0.0 and 1.19.4.1 macOS (stage 2 17/17); `orders_test` (a plan laid out with the scanned offsets: tree,
lookup by id, the new-front division rule) ALL PASS on both. OFF under `auto` until this session passes.
- READ `orders` (op, not a command): the order tree of one own army or army group, between two frames, in the external
  read's model (lists top / fallback / area; per order kind, id, executing, spearhead, withdraw, locked, drawing group,
  provinces in order, divisions, children = arrows and target lines).
- `orders.delete` (`COrderDeleteCommand(group, id)`), `orders.execute` (`COrderExecuteCommand(group, id, execute)`;
  id 0 = the whole plan; flag 1 execute, 0 stop), `orders.set_path` (the UI's own `COrderSetPathCommand` constructor:
  an existing order gets a new province path, keeps its id; refused by the game while the order is locked),
  `orders.new_front` (the UI's own `COrderNewFrontCommand` constructor: a front under a parent order, with the parent's
  divisions that no child holds yet, as the front tool hands them; no spearhead, no withdraw, no "+0x6c" flag).
- MCP: `hoi4_army_front` WITH `provinces` (explicit, drawing order) or `against` (+ `own_states`), optional
  `parent_order` / `replace_order`. Without them the tool stays on the drag. The journal's Verified (one more order on
  the army, external read) is unchanged.

Needs: the game slot, Steam closed; module and reader built from branch e73/inproc-plans; the 1940 Germany bed of
Session 1 (PREV-1940) and a 1936 bed at war or one day before a war (PREV-WAR: e.g. Italy vs Ethiopia, the Italy runs).
Find a group's address: `HOI4_BIN/macprobe SESSION dbg_army_bytes:ARMY:8` -> `"address"`.

Start: `HOI4_INPROC_FAMILIES=set_game_speed,production,air,orders ./inproc_stage2.sh PREV-1940 s4o`; the hello must show
`"orders":{"on":true,"live_passed":false}` and `stage2.orders.ok:true`. Client:
`C="python3 -I inproc/tools/inproc_client.py s4o/inproc.sock"`.

O1 reader, paused (10 min). For one army with a front and an offensive, one army group, one army with no plan:
`$C orders GROUP` and `zsh s4o/tool.sh gui hoi4_armies '{}'` (external read) and a screenshot of the plan (UI).
Pass: same orders, ids, kinds, provinces (counts and ends), divisions, executing; read_ms < 5. An enemy or allied
group's address -> `not_own`; a random heap address -> `not_a_live_object`.
O2 delete and execute, paused (10 min). `$C post2 orders.execute GROUP --vals ID --flag 1` on one front -> the reply's
read-back `executing:true`, the UI's play button shows it; `--flag 0` -> stop. `--vals 0` -> the whole plan. Then
`orders.delete` on one offensive arrow (not the front): the arrow is gone in `orders`, in `hoi4_armies` and on the map;
the front keeps its id. Same by mouse (the arrow's X, the plan's play button) for the timing.
O3 new front with provinces, paused (10 min). On an army whose front covers its whole border: take the first 4
provinces of its front (`orders` output) and `zsh s4o/tool.sh inprocess hoi4_army_front '{"army":A,"state_id":S,
"provinces":[p1,p2,p3,p4],"request_id":"o3a","lease_token":L}'` -> Verified; `orders` shows one more child of the
parent with exactly those provinces and the free divisions; the map shows the line. Then `"against":"TAG"` for a
whole border (one front per connected stretch; record the chain counts the reply gives against the map) and with
`own_states` for a piece. Then `"replace_order":ID` with a shorter list: the same id, the new provinces.
O4 refusals (5 min): a province in a state the country does not control -> `invalid_arguments` before posting (the
game's check only counts provinces; its execution drops foreign ones, so the tool checks control first); then the
same list straight through `$C post2 orders.new_front` (no tool check): record what the game does (no order, or an
order with fewer provinces) and whether anything else changed; a parent id not in the plan -> refused before
posting; `replace_order` on a LOCKED order (executing with combat: set_path's check reads the lock handle) ->
`refused_by_game_check`; a 1025-province list -> invalid_arguments. Nothing posted in any case (inproc.log).
O5 the field-marshal question (the REQUEST in NOTES-1116 "engineer 73"): with an army group of 2+ armies, by mouse,
draw the group's front along one border WITHOUT Shift, `$C orders GROUP` and `$C orders ARMY` for each army; delete;
draw again WITH Shift (Shift+click), read again. Record per order: drawer, blitz, withdraw, parent, children, divisions;
and whether each army got its own sub-front. This answers what "without sub-army fronts" is in state.
O6 timings: 20 x (set_path back and forth) in process vs 5 x by mouse: wall ms per change.
O7 the 20-day war (PREV-WAR, twice, same save, speed 3, executor off, by hand from the same script): run A with the
drag (`gui`), run B with `inprocess` and `against` fronts + `orders.execute`. Record per run: fronts placed per minute,
fronts placed where asked (orders vs request), provinces taken by day 20, casualties ratio.
Stop at once (endrun.sh s4o) on: a crash, a hang, an `orders` read that disagrees with the external read on ids or
provinces, any post2 with result "unexplained", a front that appears where the request did not ask, any order that
changes on an ALLIED or enemy group.
Pass: O1-O7 clean on both beds -> `IPC_LIVE_PASSED` for `orders` (6th entry), "S4 PASS: orders".

## Session S5-PLAN: the in-process module, the rest of the battle plan (family `orders2`; about 75 minutes)

Built by engineer 74 (24 Nov 2026), offline-proven only: stage 2 25/25 types on with every check on 1.20.0.0 and
1.19.4.1 macOS; the 8 orders2 classes found by RTTI with the same protocol ids on Linux and Windows 1.20 (10/10 with
CAutoMergeOrders and COrderConnect); the front facts (theatre -> fronts -> sections -> provinces), the array allocator
slot and Assign's division facts scanned on both macOS builds; `orders_test` (fronts read, section check, holder,
root-on-section) ALL PASS on both. OFF under `auto` until this session passes. Run S4-ORDERS first (orders2 builds on
its tree read).
- READ `fronts GROUP`: the fronts of the group's theatre: `{"theatre","fronts":[{"front":ADDR,"sections":[{"id",
  "provinces":[..]}]}]}` (the things a plan root binds to; the module never makes a front).
- `orders2.new_root` (COrderNewRootCommand: group, divisions, the front's handle, section, range from/to 0..100000,
  split): a plan root for an army or army group. split 1 = an army group's line divided among its armies (what a plain
  click on the front does), 0 = one field-marshal line (Shift+click; the only value an army's click sends). Static
  answer to the O5 question of S4-ORDERS; O5 below checks it. Read-back: the group's root bound to (front, section).
- `orders2.assign` / `orders2.unassign` (divisions to an order / out of every order), read back on the order's own
  division list / the plan's holders. Every division is checked first the way the game's Assign check reads it (its
  handle resolves to it, CUnit cast, its army is a live own group).
- `orders2.fallback` (COrderNewFallbackCommand: group, divisions, provinces): read back as one more fallback order.
- `orders2.edit_root` (the root's range on its section): read back on the root's range.
- `orders2.spearhead` (the UI's NewFront constructor with the spearhead byte): read back as one more child whose line
  carries the spearhead byte.
- `orders2.invasion_target` / `orders2.paradrop_target` (group, order, province): posted after the game's check; the
  module has no read of the target field yet (reply `target_read:"not mapped"`): the external read is the proof.
- MCP: `hoi4_army_front` with `provinces`/`against` on an EMPTY plan now posts the root first (split 0), then the fronts;
  `hoi4_army_fallback` WITH `provinces`; `hoi4_army_offensive` WITH `target_provinces` and `spearhead:true` (new
  spearhead under `parent_order`) or `replace_order` (new path, family `orders`). Without the new arguments the tools
  stay on the mouse. The journal's Verified reads are unchanged.

Needs: the game slot, Steam closed; module and reader built from branch e74/inproc-plan-orders; PREV-1940 (Germany, an
army group of 2+ armies) and PREV-WAR. Addresses: `HOI4_BIN/macprobe SESSION dbg_army_bytes:ARMY:8` -> `"address"`;
divisions: the `orders` read's `division_ptrs`, or `macprobe SESSION dbg_army_words:ARMY` (division list).

Start: `HOI4_INPROC_FAMILIES=set_game_speed,production,air,orders,orders2 ./inproc_stage2.sh PREV-1940 s5p`; the hello
must show `"orders2":{"on":true,"live_passed":false}`, `stage2.fronts.ok:true`, `common.alloc_slot:16`,
`unit_group_off:192`. `C="python3 -I inproc/tools/inproc_client.py s5p/inproc.sock"`.

P1 fronts read, paused (5 min). `$C fronts GROUP` for an army group and one of its armies: the same theatre; every
front the map shows (screenshot, front-line map mode) with its provinces; read_ms < 5 (inproc.log `fronts_read`).
P2 root, paused (15 min). On an army with NO plan: `$C fronts ARMY`, pick the section on its border, then
`$C post2 orders2.new_root ARMY --vals SEC,0,100000,0 --obj2 FRONT --divs D1,D2,..` -> `root_id`; `$C orders ARMY`
shows one top order of kind 2 with the section's provinces and the divisions; `hoi4_armies` shows the front; the map
shows the line. Delete it (`orders.delete`). Same by mouse (front tool, one click) and compare the two `orders` reads
field by field (range, provinces, divisions, drawer). Then the army group, twice: `--vals SEC,0,100000,1` and
`...,0`: record per army whether it got its own sub-front (O5 of S4-ORDERS by command), and compare with a plain click
and a Shift+click by mouse. Then `hoi4_army_front` in process on an empty plan with `provinces` (root, then the front).
P3 assign/unassign, paused (10 min). Two divisions of the army: `orders2.unassign ARMY --divs D1,D2` ->
`divisions_moved:2`; `orders` shows them in no order; the army card's front counter drops. `orders2.assign ARMY --vals
ROOT --divs D1,D2` -> back. A division of ANOTHER army -> refused before posting ("not in an own army" is not enough:
the game's check refuses a division of another group; record which). A foreign division -> `not a live division`.
P4 fallback and edit_root, paused (10 min). `orders2.fallback ARMY --provs p1,..,p6` (own provinces behind the
front) -> `fallback_orders` +1; `hoi4_army_fallback` with `provinces` the same way; by mouse (C, one stroke) for the
timing. `orders2.edit_root ARMY --vals ROOT,20000,80000` -> `range:[20000,80000]`; the map line is shorter.
P5 spearhead, paused (10 min). `hoi4_army_offensive {"army":A,"state_id":S,"spearhead":true,"target_provinces":[..]}`
on an army with a root -> one more child with the spearhead byte; the map shows a spearhead arrow; by mouse (Shift+X).
Then `"replace_order":ID` with a new target line.
P6 invasion/paradrop target, paused (5 min). Draw a naval invasion by mouse (hoi4_naval_invasion), read `orders`, then
`orders2.invasion_target ARMY --vals ORDER,PROV` with another coastal enemy province: record what the external read and
the map show (a second target, or the target moved). Same for a paradrop with a paratrooper army if the bed has one.
P7 LIVE MAPPING (no command yet, observe only, 15 min): (a) a new plain OFFENSIVE by mouse (X, one stroke): `orders`
before and after, and the inproc.log of the game's own posts if the post hook logs types (expected: NewFront under the
root with the arrow/line pair, or COrderConnect 13233, or SetPath creating the target); (b) the execution mode buttons
(careful/balanced/rush) by mouse: which command type the post hook sees; (c) area defence (V): the CreateAreaDefense
list (+0x30) element kind and the +0x48 settings mask; (d) the invasion SOURCE command (+0x30 int and +0x50 byte).
Stop at once (endrun.sh s5p) on: a crash, a hang, any "unexplained" post2, a root or order on a group the request did
not name, any order that changes on an allied or enemy group, a `fronts` read that disagrees with the map.
Pass: P1-P6 clean on both beds -> `IPC_LIVE_PASSED` for `orders2` (7th entry), "S5 PASS: orders2".

## Session S6-UNITS: the in-process module, army structure (family `units`; about 45 minutes) -- FIRST in the queue

Built by engineer 75 (24 Nov 2026), offline-proven only: stage 2 33/33 types on with every check on 1.20.0.0 and
1.19.4.1 macOS (the 8 units types and their UI constructors found on both); Linux and Windows 1.20 8/8 classes by RTTI
with the same protocol ids. OFF under `auto` until this session passes. It does not need S4/S5 (no order is written; a
transfer only NAMES the order the UI would pick).
Which commands carry membership (static, both builds): COrderGroupCommand 11983 = a NEW army of the given divisions
(the new-army card: the constructor takes the selected divisions, the first one's theatre, no theater group; it keeps
only divisions the theatre's country owns and that may change army); COrderAssignCommand 12350 = divisions INTO an
existing army (the right click on its card posts Assign(divisions, that army, the order the UI picks or 0)); army groups:
CArmyGroupCommand 14433 (new, from armies of one theatre), CAssignToArmyGroupCommand 14470 (into, index -1 as every UI
caller), CRemoveFromArmyGroupCommand 14471 (out). CMoveCommand is the map MOVE order (not membership); CMergeArmiesCommand
merges DIVISIONS ("cannot merge less than 2 armies": CArmy is the division class), not armies.
- `units.new_army FIRST --divs D1,..` (FIRST = D1), read back: every division in ONE army none of them was in before
  (reply `army`, `divisions`). A division the game's own filter drops -> `refused_by_game_check`, nothing posted.
- `units.transfer ARMY --divs D,..` (up to 512 per command), read back: every division in ARMY.
- `units.leader GROUP --obj2 LEADER` (none = unassign), `units.rename GROUP --vals codes` (1..22 printable ASCII bytes),
  `units.army_group_new A1,A2,..`, `units.to_army_group A1,.. --obj2 AG`, `units.from_army_group A1,..`,
  `units.delete_group GROUP`: posted after the game's own check; the MCP tools' Verified reads are the proof.
- MCP (same names, schemas, Verified reads; mouse fallback in the same call): hoi4_army_detach (no `into` = one new
  army, `into` = one transfer), hoi4_army_merge (all of source into target, one transfer), hoi4_army_split (the first N
  roster divisions, one new army), hoi4_army_group_create (one command), hoi4_army_commander (leader by exact name),
  hoi4_army_rename (names up to 22 ASCII bytes; others typed by the mouse). Replies carry `in_process`.

Needs: the game slot, Steam closed; module and reader built from branch e75/inproc-picks; PREV-1939 = a run folder whose
last save is the 1939 bookmark start (Germany, the one army of ~121 divisions; e.g. a copy of t1939g's first save).
Start: `HOI4_INPROC_FAMILIES=set_game_speed,production,air,units ./inproc_stage2.sh PREV-1939 s6u`; the hello must
show `"units":{"on":true,"live_passed":false}`, `common.unit_theatre_off:664`, `group_parent_ptr_off:432`.
`C="python3 -I inproc/tools/inproc_client.py s6u/inproc.sock"`. Tool calls through the broker with
`HOI4_BACKEND=inprocess HOI4_INPROC_SOCK=$PWD/s6u/inproc.sock` (deploybin.sh, as S2 did), and the same with
`HOI4_BACKEND=gui` for the mouse timings.

U1 one new army, paused (5 min). `hoi4_armies`: pick the big army and 3 division ids. `hoi4_army_detach {"army":BIG,
"divisions":[3 ids]}` -> receipt Verified, `in_process.module.army`, tool ms. `hoi4_armies`: a new army of exactly
those 3; BIG lost exactly 3. Screenshot of the army bar.
U2 THE CARVE, paused (15 min): the 1939 plan's five named formations out of the 121-division army, in one call
sequence, each formation's divisions named by id (take them from `hoi4_armies`/`hoi4_supply`: e.g. Nord 18+6, Pommern
18+6, Mitte 18+6, Sued 18+6, Panzergruppe 14): per formation `hoi4_army_detach {"army":BIG,"divisions":[<=18]}`
(new army), then `hoi4_army_detach {"army":BIG,"divisions":[rest],"into":NEW}`, `hoi4_army_rename {"army":NEW,
"name":"Armee Nord"}`, `hoi4_army_commander {"army":"Armee Nord","general":G}`; then `hoi4_army_group_create
{"armies":[Nord,Pommern]}` and a field marshal on it. READ-BACK: `hoi4_armies` shows each named army with EXACTLY its
divisions, each general, the group; nothing else moved. TIME: total wall time and per call (inproc.log post2
`total_ms`), against the mouse path on the same save reloaded (`HOI4_BACKEND=gui`, the same sequence: the t1939f
mouse detach of 14 moved 2 and diverged; record what it does now).
U3 merge and split, paused (5 min). `hoi4_army_split {"army":"Armee Nord","count":4}` -> a new army of the roster's
first 4; `hoi4_army_merge {"source":<that>,"target":"Armee Nord"}` -> back; the empty shell: does it vanish (record); if
not, `$C post2 units.delete_group SHELL` and record the game check's answer.
U4 refusals (5 min). A division of a FOREIGN country in `--divs` -> `not a live division` or the game filter's refusal
(record which); `units.army_group_new` of two armies in different theatres -> refused before posting; a leader of
another country (obj2) -> refused before posting; a name of 23 bytes -> the mouse path types it.
U5 running speed 2 (5 min): repeat U1 and one transfer while the game runs; read-back within 3 s.
U6 save + reload with the family on; plain load without the module (the save opens; the carved armies stand).
Stop at once (endrun.sh s6u) on: a crash, a hang, any `unexplained` post2, a division in an army the request did not
name, any change to a foreign army, a Dropped command in game.log.
Pass: U1-U6 clean -> `IPC_LIVE_PASSED` for `units` (8th entry), "S6 PASS: units"; the executor's layout may then name
the divisions of each role in one call sequence (DETACH_MAX stays 18 per call: the tool's schema).

## Session S7-PICKS: the in-process module, the player's picks (family `picks`; about 15 minutes) -- after S6-UNITS

Built by engineer 75 (24 Nov 2026), offline-proven only: stage 2 34/34 types on 1.20.0.0 and 1.19.4.1 macOS; Linux and
Windows 1.20 CSetNationalFocusCommand by RTTI with the same id (13245). OFF under `auto` until this session passes.
- `picks.focus FOCUS` = CSetNationalFocusCommand(own tag +0x24, the focus by POINTER +0x28): what the focus tree's
  start button posts. The module checks the object is exactly a CNationalFocus (RTTI vtable), fills the player's tag,
  then the game's own check: the focus's virtual "can start" (vtable +0x50) with the player's country (prerequisites,
  exclusivity, availability, already done). Refused = deleted, never posted.
- MCP: hoi4_focus_start (same name, schema, Verified read: the active focus) takes the focus's address from the own
  tree walk (the catalog's validated walk: exact vtables, own tree membership, unique keys); reply `in_process`. The
  mouse path (open tree, pan, click) stays the fallback in the same call.

Needs: module and reader from branch e75/inproc-picks; PREV-1936 (Germany, no focus running) and PREV-1939.
Start: `HOI4_INPROC_FAMILIES=set_game_speed,production,air,picks ./inproc_stage2.sh PREV-1936 s7p`; the hello must
show `"picks":{"on":true,"live_passed":false}`. `C="python3 -I inproc/tools/inproc_client.py s7p/inproc.sock"`.
F1 a focus, paused (3 min). `hoi4_focus_start {"focus":"GER_rheinland"}` (any available root focus) with
`HOI4_BACKEND=inprocess`: receipt Verified, `in_process.module.per_object[0].result:"posted"`, tool ms (the mouse path:
3.7 s); the focus tree shows it running; no window opened.
F2 refusals (3 min). A focus whose prerequisite is not done -> `refused_by_game_check`, nothing posted (the active focus
unchanged); a focus while another runs -> the tool's own refusal before posting; a CNationalFocus of ANOTHER country's
tree by `$C post2 picks.focus ADDR` (address from a foreign tree) -> record the game check's answer (expected refused;
if it posts, STOP: the module must check own-tree membership first).
F3 running speed 2 (3 min): F1 while the game runs; the Verified read within 3 s.
F4 save + reload with the family on; plain load without the module.
Stop at once (endrun.sh s7p) on: a crash, a hang, any `unexplained` post2, a focus started that the request did not
name, F2's foreign focus posted.
Pass: F1-F4 clean -> `IPC_LIVE_PASSED` for `picks` (9th entry), "S7 PASS: picks".

## Session S8-EVENTS: the in-process module, event answers (family `events`; about 25 minutes) -- after S7-PICKS

Built by engineer 76 (24 Nov 2026), offline-proven only: stage 2 35/35 types on 1.20.0.0 and 1.19.4.1 macOS; Linux and
Windows 1.20 CSelectEventOptionCommand by RTTI with the same id (10645); the window facts are not ported there (the full
scan is macOS-only, as for every family). OFF under `auto` until this session passes.
- `events.answer WINDOW --vals PLACE,COUNT` = CSelectEventOptionCommand built by the event window's own constructor with
  the window's own fields (pending id +0x58, event +0x60, &country +0x68, &scope +0x70) and the option's own index, as
  the window's option click builds it. The module checks: the object is exactly a CEventWindow (RTTI vtable); the window
  accepts a click now (its byte +0x330, the click's own gate); its country is the player's; its event resolves (generic
  lookup, the auth slot's cast to CEvent); the game state holds the pending record of the id with that event; every
  option's own index equals its place; the game's option test (the window's per-frame test, 1.20 0x1005f4d00) passes
  exactly COUNT options (else GUI path) and the PLACE-th of them is taken (none: refused); then the command's "can do"
  and Post. Read-back in the module: the pending record is gone within 3 s.
- MCP: hoi4_event_choose (same name, schema, Verified read: exactly that window closes) takes the window from the events
  read (the popup entry's secondary base + offset-to-top) and the shown option count; reply `in_process`. The mouse path
  stays the fallback in the same call; two windows of one event id go to the mouse.

Needs: module and reader from branch e76/inproc-events; a save with an open choice event (PREV-1936 shortly before
1936.03.07 has the Rhineland chain; or any save with 2+ open events, a news event among them).
Start: `HOI4_INPROC_FAMILIES=set_game_speed,production,air,events ./inproc_stage2.sh PREV-1936 s8e`; the hello must show
`"events":{"on":true,"live_passed":false}`. `C="python3 -I inproc/tools/inproc_client.py s8e/inproc.sock"`.
E1 a two-option choice event, paused (5 min). `hoi4_events` -> pick option 1 by key; `hoi4_event_choose
{"event":ID,"option":KEY}` with `HOI4_BACKEND=inprocess`: receipt Verified, `in_process.module.per_object[0].result:
"posted"`, `executed:1`, `pending_id`, `option_index` = the key's index in the event script; the window closes on its
own within 2 s (if the window stays open while the record is gone: STOP, the window needs the click's own close: record
the window's +0x330 and visibility); the option's effect is applied (the tooltip's change, e.g. a flag or a law).
E2 a news event (one option), paused (2 min): the same call with option 0; window closes.
E3 refusals (5 min): an option whose trigger fails -> the window does not show it: COUNT differs -> `second_object_invalid`
and the mouse path answers (record it); `$C post2 events.answer WINDOW 0,COUNT` on a window already answered ->
`second_object_invalid` "does not accept a click now", nothing posted; a forged address -> `not_a_live_object`.
E4 stacked windows (5 min): three open events, answer the BOTTOM one first in process (the mouse path needs the covering
ones first: compare the receipts' tool ms and order); none of the others changes.
E5 running at speed 2 (3 min): E1 while the game runs; Verified within 3 s; then save + reload with the family on, and a
plain load without the module.
Stop at once (endrun.sh s8e) on: a crash, a hang, any `unexplained` post2, an option taken that the request did not name,
an event answered for another country, a window left open with its record gone.
Pass: E1-E5 clean -> `IPC_LIVE_PASSED` for `events` (10th entry), "S8 PASS: events".

## Session S9-DECISIONS: the in-process module, decisions (family `decisions`; about 15 minutes) -- after S8-EVENTS

Built by engineer 76 (24 Nov 2026), offline-proven only: stage 2 36/36 types on 1.20.0.0 and 1.19.4.1 macOS; Linux and
Windows 1.20 CSelectDecisionCommand by RTTI with the same id (14345). OFF under `auto` until this session passes.
- `decisions.take DECISION` = CSelectDecisionCommand(own tag +0x24, the decision by POINTER +0x28): what the decisions
  panel's select posts (after its confirmation). The module checks the object is exactly a CDecision (RTTI vtable),
  fills the player's tag, then the game's own "can do" (the decision's allowed/visible test with the player's country,
  active state, cost). Refused = deleted, never posted. Targeted decisions stay on the mouse path (another command).
- MCP: hoi4_decision_take (same name, schema, Verified read: PP spent, timer or entry gone) opens the panel as before,
  takes the row's decision object and posts it in process; reply `in_process`; the mouse path in the same call.
Needs: branch e76/inproc-events; PREV-1936 (Germany has untargeted decisions with a PP cost).
Start: `HOI4_INPROC_FAMILIES=set_game_speed,production,air,decisions ./inproc_stage2.sh PREV-1936 s9d`; hello shows
`"decisions":{"on":true,"live_passed":false}`. `C="python3 -I inproc/tools/inproc_client.py s9d/inproc.sock"`.
D1 an affordable untargeted decision, paused (3 min): `hoi4_decision_take {"decision":KEY}` with `HOI4_BACKEND=inprocess`:
receipt Verified, `in_process.module.per_object[0].result:"posted"`, PP spent by the script cost, no confirmation window
left open, tool ms (mouse: record).
D2 refusals (4 min): a decision whose cost is above the PP held (the tool's own refusal first; then `$C post2
decisions.take ADDR` -> `refused_by_game_check`, nothing posted); a decision already active -> refused; a CDecision of
another country's category by address -> record the check's answer (expected refused; if it posts, STOP).
D3 a targeted decision: the mouse path answers (reply without `in_process`).
D4 running at speed 2 + save/reload with the family on; a plain load without the module.
Stop at once (endrun.sh s9d) on: a crash, a hang, any `unexplained` post2, a decision taken that the request did not name,
D2's foreign decision posted.
Pass: D1-D4 clean -> `IPC_LIVE_PASSED` for `decisions` (11th entry), "S9 PASS: decisions".

## Session S10-DIPLOMACY: the in-process module, diplomacy (family `diplomacy`; about 30 minutes) -- after S9-DECISIONS

Built by engineer 78 (24 Nov 2026), offline-proven only: stage 2 42/42 types on 1.20.0.0 and 1.19.4.1 macOS; Linux and
Windows 1.20 CDiplomaticActionCommand by RTTI with the same id (10545) and the six action classes by unique RTTI vtables
(the full scan is macOS-only). OFF under `auto` until this session passes.
- Every op is ONE CDiplomaticActionCommand holding a CDiplomaticAction the module builds with the ACTION class's own
  constructor: sender = the player, receiver = the target, the game date (gamestate +0x428). Declare war: the first of the
  player's own war goals whose target is the receiver (country +0xd30 -> list +0x68, count +0x74, goal target +0x3c: the
  list the declare-war dialog shows), its handle into the action (+0x6c), the dialog's flag byte left at 0. The action
  goes through the game's own command maker (1.20 0x100f83c10: it stamps the date and builds the command), then the
  command's own "can do" (the action's legality virtual +0x140, the call that enables the view's Send button); refused =
  the game's destructor frees command and action, never posted.
- Ops: `diplomacy.guarantee`, `diplomacy.military_access` (ask), `diplomacy.offer_military_access`,
  `diplomacy.non_aggression`, `diplomacy.improve_relations`, `diplomacy.declare_war`; request: objs `0`, vals = the target's
  tag index (the number hoi4_diplomacy_state and the view use). Justify war goal, join faction, lend-lease, volunteers
  and call allies stay on the mouse (their dialogs carry choices the module does not make yet).
- MCP: hoi4_diplomacy_act (same name, schema, journal and Verified read-back: the relation snapshot) sends these keys in
  process for a `country` target WITHOUT opening the diplomacy view (the declare-war confirm and widening checks still
  run first); reply `dialog.backend:"inprocess"`; the module's handing back (`Gui`) opens the view in the same call.
Needs: the module and reader from branch e78/inproc-war; PREV-1936 (Germany, peace) and PREV-1939 (bed1939-0814, a
justified war goal on Poland).
Start: `HOI4_INPROC_FAMILIES=set_game_speed,production,air,diplomacy ./inproc_stage2.sh PREV-1936 s10d`; hello shows
`"diplomacy":{"on":true,"live_passed":false}`. `C="python3 -I inproc/tools/inproc_client.py s10d/inproc.sock"`.
G1 paused, `HOI4_BACKEND=inprocess` (5 min): `hoi4_diplomacy_act {"country":"AUS","action":"improve_relations"}` and
`{"country":"HUN","action":"guarantee"}`: receipt Verified, `dialog.module.per_object[0].result:"posted"`, the relation in
hoi4_diplomacy_state, no view opened, tool ms (mouse: record the same pair by HOI4_BACKEND=gui after revoking).
G2 AI-decided while running at speed 2 (5 min): `non_aggression_pact` and `ask_military_access` with a neighbour: posted;
the receipt is Verified or `sent_awaiting_ai_reply` (Diverged), never re-sent by the executor; the AI's answer (event or
relation) appears within a few days.
G3 refusals (4 min): guarantee a country already guaranteed / a major the game forbids -> `refused_by_game_check`, nothing
posted; `$C post2 diplomacy.guarantee 0 <own tag index>` -> refused by the module ("the player's own country"); a tag index
past the country table -> refused ("not a live country"); `diplomacy.declare_war` on a country with no own war goal ->
refused ("no war goal ... justify one first"), nothing posted.
G4 declare war from PREV-1939 (the 1939 test's gate; 5 min): Poland with the justified goal: `hoi4_diplomacy_act
{"country":"POL","action":"declare_war","confirm":"declare_war","accept_widening":true}` -> Verified at_war within 3 s, the
war in hoi4_wars, the guarantors' joins as the game decides; the war-goal list after (the goal is consumed or kept as the
game does); record the tool ms against the mouse path's ~10-20 s.
G5 save + reload with the family on; a plain load without the module.
Stop at once (endrun.sh s10d) on: a crash, a hang, any `unexplained` post2, an action sent to a country the request did not
name, a war declared without the requested confirm, G3's own-tag or no-war-goal request posted.

## Session S11-HOLDS: the in-process module, area-defence holds (family `holds`; about 25 minutes) -- FIRST of the e79 sessions

Built by engineer 79 (24 Nov 2026), offline-proven only: stage 2 43/43 types on 1.20.0.0 and 1.19.4.1 macOS; Linux and
Windows 1.20 CEditAreaDefenseStateCommand by RTTI with the same id (13873) (the full scan is macOS-only). OFF under `auto`
until this session passes. Why first: every army must hold an order before a declaration, and the mouse path's area
tool failed on led armies until the select-first fix (live engineer 77).
- The map tool's area button only arms the tool; EVERY LEFT CLICK on a state posts one CEditAreaDefenseStateCommand
  through the tool's own constructor (1.20 0x10094a330, UI 0x1025b7f80): (army, order id, the state's id, the tool's
  settings mask). Order id 0 (no area order being edited) -> the execution creates a NEW area-defence order (kind 5) of the
  army with that state; an order's id -> the state is TOGGLED in it. The module: one command per state; a new order takes
  exactly one state, then the MCP adds the others to the new order's id (the module reads it back: `area_order_id`). The
  module refuses a state already in the order (a click would remove it), an order of another kind or group, and checks
  the army with the game's resolver and owner; the game's own "can do" (the army resolves as a COrdersGroup) decides.
  The settings mask is the fresh tool's 0 for a new order and the order's own mask (+0xf8) for an added state.
- MCP: hoi4_army_area_defense (same name, schema, Verified read-back: new battle-plan orders on the army) refuses a
  leaderless army with `no_commander` and a state the player does not control with `invalid_arguments` before anything
  is posted; reply `in_process.module.area_order_id` and `in_process.added` (the other states). The module's handing back
  (`Gui`) before the first click runs the mouse path in the same call; after the first click nothing is redrawn by mouse.
Needs: the module and reader from branch e79/inproc-holds (contains e78); PREV-1939 = bed1939-0814 (armies carved by S6,
generals assigned) or any save with a led army and own states behind the front.
Start: `HOI4_INPROC_FAMILIES=set_game_speed,production,air,units,holds ./inproc_stage2.sh PREV-1939 s11h`; hello shows
`"holds":{"on":true,"live_passed":false}`. `C="python3 -I inproc/tools/inproc_client.py s11h/inproc.sock"`.
H1 paused, `HOI4_BACKEND=inprocess` (5 min): `hoi4_army_area_defense {"army":"<a led army>","states":[<one own state>]}`:
receipt Verified, `in_process.module.per_object[0].result:"posted"`, `area_order_id` > 0; hoi4_armies shows an
`area_defense` order with exactly that state; the map shows the area (screenshot). Tool ms against the mouse (~2-6 s).
H2 three states (5 min): `{"army":"<another led army>","states":[a,b,c]}` -> one new order holding a, b, c (in_process.added
posted 2); no second area order on the army.
H3 refusals (5 min): a leaderless army -> `no_commander`, nothing posted; a foreign state -> `invalid_arguments`; by the
client `$C post2 holds.area_defense <army> --vals <area order id>,<a state already in it>` -> refused by the module ("already in
area order"), the order unchanged; `$C post2 holds.area_defense <army> --vals 0,<s1>,<s2>` -> refused by policy (a new order
takes one state); an order id of a FRONT order -> refused ("not an area-defence order").
H4 running at speed 2 (5 min): H1 on a third army; Verified within 3 s; the divisions move to the area.
H5 save + reload with the family on (the area orders stand); a plain load without the module.
Stop at once (endrun.sh s11h) on: a crash, a hang, any `unexplained` post2, a state REMOVED from an order, an area order on
an army the request did not name, a second area order created by one call, a leaderless army given an order.
Pass -> IPC_LIVE_PASSED entry 13 (holds) = 1 and the family is used under `auto`.

## Session S12-RESEARCH: the in-process module, research in a slot (family `research`; about 15 minutes) -- after S11-HOLDS

Built by engineer 79 (24 Nov 2026), offline-proven only: stage 2 44/44 on 1.20.0.0 and 1.19.4.1 macOS; Linux/Windows 1.20
CSetResearchCommand by RTTI (12144). OFF under `auto` until this session passes.
- CSetResearchCommand through the research view's own constructor (1.20 0x100200640, 1.19.4.1 0x1001fff00): (own tag, the
  technology TEMPLATE (exact class 19CTechnologyTemplate, from the game's technology database by key), slot, XP boost 0,
  b35 1, UI byte 0) = the slot-selected click's flags and the factory's defaults. The game's "can do" (slot exists; the
  country can research it now) decides; refused -> the MCP's mouse path (an exclusive pair is switched by
  CSetXORResearchCommand, which the module does not post). b35 = 1 makes the slot run the game's follow-up routine
  (0x1017ae070) exactly as the view's click: UNVERIFIED what it changes besides the slot (R2 checks the other slots).
- MCP: hoi4_research_start (same schema, Verified = the slot holds the technology) routes in process without opening the
  research view; reply `in_process.module`.
Start: `HOI4_INPROC_FAMILIES=set_game_speed,production,air,research ./inproc_stage2.sh PREV-1936 s12r`.
R1 paused, `HOI4_BACKEND=inprocess`: `hoi4_research_start {"technology":"<an available tech>"}` into the first empty slot:
Verified, `per_object[0].result:"posted"`, the research view (opened by hand after) shows it; tool ms vs mouse (~3-6 s).
R2 replace: `{"technology":"<another>","slot":0,"replace":true}` on a busy slot: Verified; the old technology's progress
kept in its own record (hoi4_research shows it unresearched with its progress); no other slot changed.
R3 refusals: an already researched tech -> `already_researched` before posting; a tech whose prerequisites are missing ->
the game's check refuses, the mouse path then reports its own result (no in-process post); an exclusive alternative of a
tech in progress -> no in-process post (mouse).
R4 running at speed 3: R1 on another slot: Verified within 3 s. R5 save + reload; a plain load without the module.
Stop at once on: a crash, any `unexplained`, a slot changed that the request did not name, XP spent (army/navy/air XP drop).
Pass -> IPC_LIVE_PASSED entry 14 (research) = 1.

## Session S13-POLITICS: the in-process module, laws (family `politics`; about 15 minutes) -- after S12-RESEARCH

Built by engineer 79 (24 Nov 2026), offline-proven only: stage 2 46/46 on 1.20.0.0 and 1.19.4.1 macOS; Linux/Windows 1.20
CAddIdeaCommand 12303 / CReplaceIdeaCommand 13017 by RTTI. OFF under `auto` until this session passes.
- The politics view's own commands (UI 1.20 0x10234a8c0): the slot's current idea active -> ReplaceIdea(own tag, current,
  new) (ctor 1.20 0x100209df0 / 1.19.4.1 0x1002096b0); else AddIdea(own tag, idea) (0x100206130 / 0x1002059f0). The
  objects come from the game's lists, no panel: the new idea from TGameItemDatabase<CIdeaDatabase> (+0x60) by key, the
  current one = the own ACTIVE idea (country+0xd38 -> +0x50) of the same category (+0x908). The game's "can do" decides
  (same category, PP cost, the law's conditions). A key that is no idea (advisors, staff = characters) -> mouse path.
- MCP: hoi4_politics_choose (same schema; the slot index is not used in process: the idea's category is the slot);
  Verified = the idea among the own active ideas; reply `in_process.module`.
Start: `HOI4_INPROC_FAMILIES=set_game_speed,production,air,politics ./inproc_stage2.sh PREV-1936 s13p` (Germany 1936).
P1 paused, `HOI4_BACKEND=inprocess`, enough PP: `hoi4_politics_choose {"category":"government","slot":<economy>,
"idea":"partial_economic_mobilisation"}`: Verified; `per_object[0].result:"posted"`, `cmd` politics.replace; PP drops by
the law's cost; hoi4_politics_options shows it; the old law no longer active.
P2 trade law (`limited_exports`), P3 conscription (`limited_conscription`): same.
P4 refusals: a law the country may not take (e.g. `war_economy` in peace) -> `game_refused` (nothing posted, no PP
spent); too little PP -> `game_refused`; an active law -> `invalid_target` before posting.
P5 an advisor key -> the mouse path serves it in the same call (no in-process post). P6 save + reload; a plain load.
Stop at once on: a crash, any `unexplained`, PP spent without the law, two laws of one category active.
Pass -> IPC_LIVE_PASSED entry 15 (politics) = 1.

## Session S14-CONSTRUCTION: the in-process module, the construction queue (family `construction`; about 25 minutes) -- after S13-POLITICS

Built by engineer 82 (24 Nov 2026), offline-proven only: stage 2 49/49 on 1.20.0.0 and 1.19.4.1 macOS; Linux/Windows 1.20
CAddConstructionCommand 12151 / CRemoveConstructionCommand 13088 / CSetProductionLinePlacementCommand 17412 by RTTI (3/3
each). OFF under `auto` until this session passes.
- add: the map click's own command through its own constructor (1.20 0x1001f9e80, 1.19.4.1 0x1001f9740): (mode 2 = after
  the last active line, as a plain click; own tag; the building reference {location id, the template's id}; count 1), one
  command per click. The module reads the template (exact CBuildingTemplate from the game's building database by key):
  real (+0x18) and province count (+0x26c > 0 = a province building) as the game's reference constructors do, so a
  supply hub / naval base / bunker names the province, a factory / dockyard / refinery / infrastructure the state. The
  game's "can do" (free slots, own state, the building's own conditions) decides; refused -> nothing posted.
- remove: RemoveConstruction(line, its own level count) = the whole line leaves (no confirmation popup in process).
- move: SetProductionLinePlacement(line, index, placed-by-hand byte); the MCP computes the byte by the view's rule (1.20
  0x10204cc20: 0 when the line lands next to an active, not-hand-placed line of the same building, else 1).
- MCP: hoi4_construction_add (Verified = the state's queued amount grew by `count`; `fill` and a province building without
  a province_id stay on the mouse) and hoi4_construction_order (Verified = the line at its new index / gone); reply
  `in_process.module`. Railways (CBuildRailwayCommand carries the UI route finder's province path) stay on the mouse.
Start: `HOI4_INPROC_FAMILIES=set_game_speed,production,air,construction ./inproc_stage2.sh PREV-1936 s14c` (Germany 1936).
C1 paused, `HOI4_BACKEND=inprocess`: `hoi4_construction_add {"building":"industrial_complex","state_id":64,"count":2}`
(Brandenburg): Verified; `per_object` two "posted"; hoi4_construction_queue shows the line(s) with 2 levels in state 64.
C2 a province building: `{"building":"supply_node","state_id":<state>,"province_id":<a province with a free slot>}`:
Verified; the queue's line has that province as location. `{"building":"naval_base","state_id":64,"province_id":<inland>}`
-> `game_refused`, nothing posted.
C3 refusals: a state with no free slot (count 10 of `arms_factory` in a small state) -> the game refuses the first click
that does not fit (per_object shows posted then refused); a foreign state id -> `game_refused`; a province building without
a province_id -> the mouse path serves it.
C4 order: queue three lines A, B, A' (two of one building): `hoi4_construction_order {"line":2,"action":"top"}` ->
Verified `{from:2,to:0}`; the in-game row shows A' at the top grouped with A (no "placed by hand" mark); `up`/`down`/
`bottom` on B: Verified at the right index each time; compare the game's own drag of a row once (same result).
C5 `{"line":<i>,"action":"remove"}` on a 3-level line: Verified removed; the queue length drops by one; no other line changed.
C6 running at speed 3: C1 and C4 again: Verified within 3 s. C7 save + reload; a plain load without the module.
Stop at once on: a crash, any `unexplained`, a line placed in another state / province than named, a line changed that the
request did not name, civilian factories assigned wrongly (queue rows' factories differ from the game's own after C4).
Pass -> IPC_LIVE_PASSED entry 16 (construction) = 1.

## Session S15-TRAINING: the in-process module, training queues (family `training`; about 25 minutes) -- after S14-CONSTRUCTION

Built by engineer 82 (24 Nov 2026), offline-proven only: stage 2 55/55 on 1.20.0.0 and 1.19.4.1 macOS; Linux/Windows 1.20
CCreateConveyor 13479 / CSetConveyorSeries 13483 / CAddConveyorLine 13484 / CSetConveyorLocation 13481 / CDeployConveyor
13600 / CRemoveConveyor 13601 by RTTI (6/6 each). OFF under `auto` until this session passes.
- create: the template row's Train button: the module first calls the game's own can_train(deployment, template body, 1,
  0) (1.20 0x1014876a0, read from the add-line command's own check) and refuses without posting when it says no; then
  CreateConveyor(template, 0). series: SetConveyorSeries(queue, 1) (a new queue trains forever). add_lines: AddConveyorLine
  (queue, n) at most 5 per command (Shift-click); its "can do" is can_train with n. location: SetConveyorLocation(queue,
  province id) as the map click; read back from the queue's +0x3c (UNVERIFIED live: the panel's "No location set" label
  should match +0x3c == 0). deploy / remove: the queue's deploy / cancel buttons (module ops; no MCP tool names them).
- MCP: hoi4_train (Verified = the new queue's lines; the in-process reply lists the four posts) and hoi4_deploy_locations
  (in process: the queues' province field is the read-back; mouse: the panel's labels). The location is the deploy
  state's first province from the game files' state definition.
Start: `HOI4_INPROC_FAMILIES=set_game_speed,production,air,training ./inproc_stage2.sh PREV-1936 s15t` (Germany 1936).
T0 FIRST (engineer 123, 28 Nov; the coordinator's question decides the training shape): PARALLEL OR ONE AFTER ANOTHER.
The conveyor holds a series amount at +0x38 (each line trains that many divisions in turn; 0 = forever) and parallel
lines at +0x70 (mapped 1.19.4.1). By hand in the deployment view, paused: make queue A of Infanterie-Division with ONE line
and its + raised to 3 (amount 3), and queue B with THREE lines of amount 1. Unpause at speed 3 for 10 game days. Write per
queue the divisions in training shown and each one's progress %: if A shows 1 in progress and B shows 3, lines are
parallel and the amount is a series (the executor's shape: L lines of amount 1, L = `evaluate.py training`'s number);
if A shows 3 in progress, one line with amount N trains N at once (then tell tools 123's successor: hoi4_train sends
ONE line with amount N). Both are built for: `training[].queue` is the number in training at once either way. Cancel
both queues after.
T0b THE STOP AT FIVE (s115a: ask 7 stopped at 5, the 6th Add Unit post read posted and added nothing): at war in a 1939
save (the stop was seen there), `HOI4_BACKEND=inprocess`, `hoi4_train {"template":"Infanterie-Division","count":7,
"deploy_state_id":64}`. If the reply has `short` > 0: copy the reply's `posts` (each post's posted/refused/result) and the
module log's `post2` lines with op training.add_lines (the module log names each post's game check and result), then
click the SAME queue's Add Unit once by hand. Mouse adds the line -> the command path differs from the button (report
both, family stays off); mouse adds nothing -> a game rule caps the lines (read the button's tooltip text into the
notes; the tool then reports the cap instead of a short). Write manpower free and the template's manpower per division.
T1 paused, `HOI4_BACKEND=inprocess`: `hoi4_train {"template":"Infanterie-Division","count":3,"deploy_state_id":64}`:
Verified; `in_process.posts` = create, series, add_lines, location; open the panel by hand AFTER: one queue, 3 lines,
1 division each, location Brandenburg (not "No location set").
T2 count 7 (6 extra lines: the module posts AddConveyorLine 5 then 1): Verified, 7 lines.
T3 a template the game cannot train now (e.g. one needing equipment the country cannot produce, or no manpower) ->
`game_refused`, nothing posted, no queue added. A template name that does not exist -> the mouse path's own error.
T4 hoi4_deploy_locations: create a queue by hand without a location; `{"state_id":64}`: Verified, the label set.
T5 running at speed 3: T1 again, Verified within 3 s per step. T6 save + reload; a plain load without the module.
Stop at once on: a crash, any `unexplained`, two queues for one request, a queue at "No location set" while +0x3c > 0
(then the read-back offset is wrong: fix before passing), manpower or equipment changed by anything but the queue.
Pass -> IPC_LIVE_PASSED entry 17 (training) = 1.

## Session S16-MIO: the in-process module, military-industrial organisations (family `mio`; about 25 minutes) -- after S15-TRAINING

Built by engineer 84 (24 Nov 2026), offline-proven only: stage 2 59/59 on 1.20.0.0 and 1.19.4.1 macOS (identical offsets);
Linux/Windows 1.20 CAddIndustrialOrgTraitToQueueCommand 16412 / CUnlockIndustrialOrganisationTrait 14495 /
CAttachPolicyToIndustrialOrgCommand 16260 / CSetIndustrialManufacturerCommand 14496 and COrganisation by RTTI (5/5 each).
OFF under `auto` until this session passes.
- the read (`{"op":"mios"}`, no opt-in): the player's own organisation list (country_of(tag) +0xd10 -> +0x130/+0x13c),
  each organisation's key, funds_raw (+0x108, scale UNVERIFIED), size (+0x110), unspent trait points (+0x114), attached
  policy (+0x170), queue (+0x138/+0x144), the trait tree in the game's order (+0xf0 -> +0x28/+0x34, stride 0x380) with
  states u (the unlocked std::set +0x120) / q / a (the game's can_unlock, as the unlock command's check calls it) / l,
  and per organisation the policies the policy window's own gate (1.20 0x101ed8bf0) allows.
- unlock_trait / queue_trait: the tree's click; the MCP unlocks when a point is free and the trait is `a`, else queues.
  policy: the policy item's token; the module calls the item's gate first (cooldown, political power, allowed) and posts
  nothing when it says no; the attached policy's item detaches (the empty token). line: a production line's roster row
  (the line's resolver + owner, the organisation in the own list; "can do" checks the organisation makes the design).
- MCP: hoi4_mio_trait (trait_index, trait_indices, auto, list), hoi4_mio_policy, hoi4_mio_line (organization given as its
  KEY; the first offered organisation, a localised name, list:true and naval lines stay on the mouse path); hoi4_mios adds
  `state` and `mio_system.unspent_trait_points`.
Start: `HOI4_INPROC_FAMILIES=set_game_speed,production,air,mio ./inproc_stage2.sh PREV-1936 s16m` (Germany 1936).
T0 hoi4_mios (any backend): `mio_system.state_read` true; Germany's organisations listed with keys; compare ONE organisation
by hand in the politics view: size, trait points, attached policy, the queued traits; the trait tree's ORDER against the
window (index 0 = which icon?). If the order differs: record it, list:true and trait_index still agree with each other.
T1 paused, `HOI4_BACKEND=inprocess`: `hoi4_mio_trait {"organization":"<an org with trait_points>0>","trait_index":<an a
trait>}`: Verified; the window shows the trait unlocked, the points one less. Then an `l` trait: Verified as queued.
T2 `trait_indices` of 3 (one already queued): Verified, the queued one counted as reached, the others queued/unlocked.
T3 `hoi4_mio_policy` with a policy in `policies_allowed`: Verified, political power down by its cost; again at once ->
nothing posted, Verified (attached). A policy NOT in `policies_allowed` (cooldown) -> `refused_by_game_check`, nothing posted.
T4 `hoi4_mio_line {"line":<an infantry_equipment line>,"equipment":"infantry_equipment","organization":"<a small-arms org
key>"}`: Verified; the line's row shows the organisation. A tank organisation on that line -> `refused_by_game_check`.
T5 running at speed 3: T1 again, Verified within 3 s. T6 save + reload; a plain load without the module.
Stop at once on: a crash, any `unexplained`, a trait unlocked with no point (points negative), political power spent on a
refused policy, an organisation of another country in the read, a read whose sizes or points differ from the window.
Pass -> IPC_LIVE_PASSED entry 18 (mio) = 1.

## Session S17-NAVY: the in-process module, task forces and fleets (family `navy`; about 25 minutes) -- after S16-MIO

Built by engineer 85 (24 Nov 2026), offline-proven only: stage 2 65/65 on 1.20.0.0 and 1.19.4.1 macOS (identical type ids,
offsets and field ids); Linux/Windows 1.20 CNavalMissionSetTypeCommand 12272 / CMergeNaviesCommand 13176 /
CNavalMissionAddRegionCommand 12274 / CNavalMissionRemoveRegionCommand 12275 / CSetFleetLeaderCommand 12370 /
CSetFleetCommand 15165 by RTTI and type id (6/6 each). OFF under `auto` until this session passes.
- mission: the mission bar's command (one command for the selection; task force handles, the button's mission code,
  +0x45 = 1 as the button; +0x44 kept 0; codes 0-4 and 9 only). add_region / remove_region: the fleet and the strategic region id (the right
  click; the remove tool). leader: the fleet and the admiral (or none). assign: the receiving fleet and the task forces
  (each checked by the task force type's own resolver and owner getter). merge: the selection's task forces.
- MCP: hoi4_fleet_mission, hoi4_fleet_region (the region by its display name -> id from the game files; `state_id` is
  still required by the schema and only used by the mouse path), hoi4_fleet_admiral (assign/unassign; list:true stays on the
  mouse path), hoi4_fleet_assign, hoi4_fleet_merge route in process; split, balance, reserve, delete, create stay on the mouse.
Start: `HOI4_INPROC_FAMILIES=set_game_speed,production,air,navy ./inproc_stage2.sh PREV-1939 s17n` (Germany 1939 bookmark:
several fleets, submarines, an idle task force), calls with `HOI4_BACKEND=inprocess`.
N1 paused: `hoi4_fleet_region {"fleet":"<a U-boat fleet>","region":"<a sea region not assigned>","state_id":<any coastal>}`:
Verified (hoi4_fleets assigned_regions lists it; the fleet panel's Active in Regions too). The same again ->
already_assigned before posting. Then `remove:true`: Verified, gone from both.
N2 `hoi4_fleet_mission {"task_force":"<a submarine task force>","mission":"convoy_raiding"}`: Verified (code 3; the panel
shows the raiding icon). Then `patrol`, then `hold` (code 0): each Verified. `mine_laying` and `training` go by mouse by
design (the module never posts 5/6/7: the buttons' technology rule and training's setting byte are the UI's): the reply has no `in_process`.
N3 `hoi4_fleet_admiral {"fleet":"<fleet with no admiral>","admiral":"<free admiral>"}`: Verified; then `unassign:true`:
Verified; an admiral who leads another fleet -> Verified and the other fleet shows none (the UI asks first; record).
N4 `hoi4_fleet_assign {"task_force":"<tf>","to_fleet":"<other fleet>"}`: Verified (hoi4_fleets). `hoi4_fleet_merge
{"fleet":"<fleet with 2+ task forces in one port>"}`: Verified (one task force with all ships).
N5 running at speed 3: N2 again, Verified within 2 s. N6 save + reload; a plain load without the module.
Stop at once on: a crash, any `unexplained`, a task force of another country moved, a mission code outside the buttons'
set read back, a fleet left with no task force and not deleted by the game, an admiral who appears in two fleets.
Pass -> IPC_LIVE_PASSED entry 19 (navy) = 1.

## Session S18-DOCTRINE: the in-process module, grand doctrines and subdoctrines (family `doctrine`; about 15 minutes) -- after S17-NAVY

Built by engineer 85 (24-27 Nov 2026), offline-proven only: stage 2 67/67 on 1.20.0.0 and 1.19.4.1 macOS (identical type
ids, offsets and field ids); Linux/Windows 1.20 NDoctrines::CUnlockGrandDoctrineCommand 16777 /
NDoctrines::CUnlockSubDoctrineCommand 16779 by RTTI and type id (2/2 each). OFF under `auto` until this session passes.
- grand: the officer corps view's doctrine selection (1.20 0x101051db0): own tag, the grand doctrine template (from the
  game's grand doctrine database by name key, exact class). The game's own "can do" (the folder, the experience, its cost).
- sub: own tag, the track's place in the folder's grand doctrine, the subdoctrine template (the subdoctrine database), the
  folder template (the own doctrine status). The game's check: the subdoctrine fits the track at that place, the cost.
- MCP: hoi4_doctrine_adopt routes in process after its own typed refusals (already_adopted, grand_doctrine_set,
  track_occupied, insufficient_experience, ...); a module refusal of its own runs the mouse path in the same call.
Start: `HOI4_INPROC_FAMILIES=set_game_speed,production,air,doctrine ./inproc_stage2.sh PREV-1939 s18d` (Germany 1939: the
three folders have grand doctrines, special forces empty, experience held), calls with `HOI4_BACKEND=inprocess`.
D1 paused: `hoi4_doctrines {"include_catalog":true}`; pick a subdoctrine the land grand doctrine offers on an EMPTY track
whose cost the army experience covers. `hoi4_doctrine_adopt {"doctrine":"<it>"}` (with `track` when it fits several):
Verified (hoi4_doctrines shows it on THAT track; the officer corps view too; experience spent by the cost). RECORD the
track: it proves the track place (the doctrine read's script order) is the game's order. Wrong track = stop.
D2 a grand doctrine for special forces (if the DLC folder is present and the experience covers it): Verified.
D3 one the experience does not cover: insufficient_experience before posting, or the game's refusal (`refused_by_game_check`)
with nothing changed.
D4 running at speed 3: a second subdoctrine (navy or air folder): Verified within 2 s. D5 save + reload; a plain load
without the module.
Stop at once on: a crash, any `unexplained`, a doctrine set on another country, a subdoctrine on a track it does not
fit, experience spent with no doctrine set, two subdoctrines on one track.
Pass -> IPC_LIVE_PASSED entry 20 (doctrine) = 1.

## Session S19-PROJECTS: the in-process module, special projects (family `projects`; about 15 minutes) -- after S18-DOCTRINE

Built by engineer 85 (27 Nov 2026), offline-proven only: stage 2 71/71 on 1.20.0.0 and 1.19.4.1 macOS (identical type ids,
offsets and field ids); Linux/Windows 1.20 CStartProjectCommand 16402 / CStopProjectCommand 16403 / CAttachScientistCommand
16398 / CUnattachScientistCommand 16399 by RTTI and type id (4/4 each). OFF under `auto` until this session passes.
- attach: the facility (NProject::CProgram handle) and the scientist (CCharacter handle), flag 1 as both UI sites send it.
  start: the facility and the project (NProject::CProject of the own pool). stop: the project. unattach: the scientist and
  the facility. Each object through the game's own resolver; the facility / project / scientist owned by the player.
- MCP: hoi4_special_project_assign (attach, then start, as the panel does), hoi4_special_project_stop,
  hoi4_special_project_unassign route in process after their own typed refusals; hoi4_special_project_recruit stays on
  the mouse path (its command's authorisation also reads the candidate pool).
Start: `HOI4_INPROC_FAMILIES=set_game_speed,production,air,projects ./inproc_stage2.sh PREV-1939 s19p` (Germany 1939 with
La Resistance / By Blood Alone facilities; or a save with a built facility and a free scientist), `HOI4_BACKEND=inprocess`.
P1 paused: `hoi4_special_projects {}`: pick an empty facility and a free scientist of its specialization.
`hoi4_special_project_assign {"facility_id":<id>,"scientist":"<name>"}`: Verified (hoi4_special_projects shows the
scientist in it; the tab's card too).
P2 `hoi4_special_project_assign {"facility_id":<id>,"project":"<an offered project of that specialization>"}`: Verified
(the facility runs it; the facility window shows it).
P3 running at speed 3: `hoi4_special_project_stop {"facility_id":<id>}`: Verified (stopping); then, once stopped,
`hoi4_special_project_unassign {"facility_id":<id>}`: Verified (no scientist).
P4 a project of another specialization: the typed refusal before posting, or `refused_by_game_check` with nothing
changed. P5 save + reload; a plain load without the module.
Stop at once on: a crash, any `unexplained`, a scientist in two facilities, a project running in a facility of another
specialization, another country's facility changed.
Pass -> IPC_LIVE_PASSED entry 21 (projects) = 1.

## Session S20-MOVE: the in-process module, division moves (family `move`; about 25 minutes) -- after S19-PROJECTS

Built by engineer 105 (28 Nov 2026), offline-proven only: the scanner resolves `move.division` at full confidence on
1.19.4.1, 1.20.0.0 and 1.20.1.0 macOS (CMoveCommand 10402, the same constructor, action offsets and flag bytes in all
three; `inproc/test_offline.sh` checks them). OFF under `auto` until this session passes.
- What it posts: one CMoveCommand per division, built by the game's own province constructor with the constants of the
  right click's own call for a reachable province (normal move, kind 2, byte 1, the four stack bytes 0), then the game's
  own "can do"; a refusal is deleted, never posted. The module checks each division first (a live CUnit through the
  game's lookup and cast, owned by the player) and reads the built command back (handle, a one-province path, the bytes).
- MCP: hoi4_army_move with `province_id` (with or without `divisions`) and with `state_id` alone (the state's port, else
  its largest victory point, else its first province, as the mouse path aims) goes in process; `halt` stays on the
  mouse. Only divisions neither marching on the target nor standing in it are ordered. The Verified read-back (the
  divisions' headings) is unchanged; a partial post settles in process (no mouse pass orders a division twice).
Start: `HOI4_INPROC_FAMILIES=set_game_speed,production,air,move ./inproc_stage2.sh PREV-1939 s20m` (Germany 1939, or
any save with an own army at peace and a war at hand), `HOI4_BACKEND=inprocess`.
M1 paused, at peace: `hoi4_armies {}`; pick an army with at least 3 divisions standing in one province and an own
province next to it. `hoi4_army_move {"army":"<name>","state_id":<s>,"province_id":<p>,"divisions":[<3 ids>]}`:
Verified, the receipt carries `in_process` with `posted` 3; the three show the move arrow to <p> (the same arrow and
march time as a right click to <p> with the same divisions selected); the rest of the army keeps its orders.
M2 running at speed 2: the same three back to their province by `province_id`, then the whole army to a neighbouring own
state by `state_id` alone: Verified; every division heads for the chosen province; a second identical call sends
nothing ("already heading or standing there").
M3 at war (declare, or use a save at war): `hoi4_army_move` on an enemy-held province next to the army: the divisions
attack (a battle opens) exactly as a right click on that province attacks; record the battle's attackers.
M4 refusals: a province across the sea with no transport path, or a sea province: `refused_by_game_check` (or the
typed refusal), nothing posted, nothing moves; a division of another country's id: not_own, nothing posted.
M5 timings: ten moves of 3 divisions each by command, then the same ten with `HOI4_BACKEND=gui`: record the wall time
per call (the war step's measure). Expected: under 0.2 s by command against several seconds by mouse.
M6 save + reload; a plain load without the module.
Stop at once on: a crash, any `unexplained`, a division moving that was not named, a move arrow or march time that
differs from the right click's, a battle that a right click would not open, or a post the game's check refused.
Pass -> IPC_LIVE_PASSED entry 23 (move) = 1.

## Session S21-PARADROP: the in-process module, paradrops (family `orders2`, tool `hoi4_paradrop`; about 30 minutes) -- after S20-MOVE

Built by engineer 106 (28 Nov 2026), offline-proven only: `orders2.paradrop_source` (COrderSetParadropSourceCommand 12729,
0x50 bytes) resolves at full confidence on 1.19.4.1 and 1.20.0.0 macOS with the same field offsets as the existing
`orders2.paradrop_target` (`inproc/test_offline.sh`). The orders2 family itself passed live (S5-PLAN), so the gate is in
the tool: `hoi4_paradrop` refuses `paradrop_unvalidated` unless `HOI4_BACKEND=inprocess` until this session passes
(`paradrop::LIVE_PASSED` in native/src/launch/macos/paradrop.rs). There is no mouse path.
- What it posts: source with order 0 and the army's divisions (the paradrop tool's first click: the game removes the
  army's area-defence orders, makes a kind-4 order, sets the departure province and assigns the divisions; the module
  reads the new order's id back from the group's top list), then `orders2.paradrop_target` with that id and the drop
  province, then `orders.execute` (flag 1) unless `execute:false`. Each through the game's own "can do".
- The executor's `airborne` opportunity rule (native/src/plan_airborne.rs) sends it when transports reach, the region's
  air is ours by the region window's estimate, and (FAIR) a battle of ours lies within `link_up_km`.
Start: `HOI4_INPROC_FAMILIES=set_game_speed,production,air,orders2 ./inproc_stage2.sh <save> s21p`: a save at war with an
own army of trained paratrooper divisions (Germany 1940 with Fallschirmjäger, or train 2-3 para divisions first),
transport-plane wings at an own air base within range of an enemy province, and the region's air superiority ours
(fighters on air superiority first). `HOI4_BACKEND=inprocess`.
P1 paused: `hoi4_air {}` (transport wing, its `air_range_km`), `hoi4_air_regions {}` (coverage of the target region).
`hoi4_paradrop {"army":"<para army>","from_state":<base state>,"target_province":<enemy province in range>,
"execute":false}`: Verified; the receipt's `in_process.order_id`; the plan bar shows a paradrop arrow from the base to
the province, exactly as the tool's two clicks draw it; the army's other orders are gone only where the game's own
source click removes them (area defence). RECORD: hoi4_armies orders row `kind` = "paradrop".
P2 running at speed 2: the same with `execute` default: the divisions take off (the unit's paradrop state), land on
the province, a battle opens. RECORD the transport wings' mission and how many divisions dropped.
P3 refusals: a province beyond the transports' range; a region where the enemy holds the air
(PARADROP_AIR_SUPERIORITY_RATIO); an army without paratroopers: `paradrop_refused` (the game's check) or an inproc
refusal, nothing posted, no order drawn. Unexpected success on any of these = stop.
P4 `division_ids` naming 2 of 3 para divisions: only those 2 are on the order.
P5 save + reload; a plain load without the module.
Stop at once on: a crash, any `unexplained`, a drop on a province not named, divisions of another army on the order,
a paradrop order the game's own tool would not draw.
Pass -> set `LIVE_PASSED` true in native/src/launch/macos/paradrop.rs (auto then runs it).

## Air reads and the air plan (engineer 106, 28 Nov 2026): live checks, no new command

- air-coverage: `hoi4_air {}` per wing `air_range_km` and `coverage` against the game's own "Coverage in this region"
  (the region tooltip over a wing's base). Coverage is computed from map/unitstacks.txt land positions within the range
  of the base province at MAP_SCALE_PIXEL_TO_KM; record both numbers for 3 wings (short, medium, long range), and
  whether a range MIO's bonus shows in `air_range_km` (the variant's stats at fleetmem 0x260).
- air-superiority-read: the enemy's share per region is read only through the region window (hoi4_air_region_probe).
  Static lead for a direct read: routine 0x1009f8460 (1.20.0.0), stride 0xb0 entries at +0xe0, fields +0x88/+0x90/+0xa8;
  owner unconfirmed. Engineer 125's reading of the routine (static): rsi is the region (index at +0x58, size term at
  +0xd8); it walks two per-region wing lists of rdi (rdi+0x98 and rdi+0x140, each a {data,size} pair per region index,
  stride 0x18), takes only live wings (wing+0x14 != 0) and sums each wing's fixed-point value at wing+0xe8: the sum goes to
  entry+0x88 (capped), the sum over (region size term x a global define) to entry+0x90 (scaled, capped at 100000, at least
  1 when the sum is positive) and the wing count to entry+0xa8 (16 bit). So +0x88 is the region's air superiority value
  of rdi's side, +0x90 its share of what the region needs (x 100000), +0xa8 its wings. The caller 0x100a18e10 runs it per
  (object, region) next to the CStrategicAir allocator's type info: rdi is likely one country's strategic air object.
  Confirm against the region window for 2 regions (ours and the enemy's numbers), then the probe can be replaced; FAIR:
  read the enemy's entry only for regions our air map shows.
- air-situation (plan key `air_situation`): a war save; record each executor turn's orders against the region window
  (fighters sized to the ratio or kept out; CAS only over regions not lost; one forward rebase when out of range).
  Mission entries (engineer 125, written): `air_situation.missions:[{"id":"ls","mission":"logistics_strike","region":<a
  region of our battles>,"wings":2},{"id":"rc","mission":"recon","region":<same>,"wings":1}]`: the two CAS wings nearest are
  sent on logistics strike and the scout wing on recon before the fighters and CAS take the rest; with the enemy's fighters
  seen there above ours / fighter_ratio the strike waits (the note says so); a wing the game refuses for the mission is
  left out of it for 10 turns. Record each order and the region window.
- designs: `scripts/hoi4facts/facts.py design-best GER --role fighter --need air_range=1100` and the owner's Fw 190
  (1235 km): the designer's shown range, agility, attack and cost against the computed numbers for the same modules.
- S18-DOCTRINE also adopts a special-forces grand doctrine (D2): the paratrooper line needs it.

## Session S28-AIRTRAIN: air training that pays, and its stop (mouse; about 20 minutes plus a 40-day soak at speed 5) -- any time
Engineer 125 (written, not run): g115a's wings gained 6 air XP in four months because training (pilot exercises) has no
region, so the fuel cut never saw the training wings (fuel fell to 10%) and hoi4_air_hold refused them; and training past
the game's cap (AIR_WING_XP_TRAINING_MAX / AIR_WING_XP_TRAINING_MISSION_GAIN_DAILY: 42.9 days on 1.20.1,
`scripts/hoi4facts/facts.py air-training`) gives a wing nothing more. Bed: the 1936 Germany start, paused.
T1 `hoi4_air_mission {wings:[2 fighter wings], mission:"training"}` (no region): both Verified, mask has frame 12, region 0.
Record fuel, country air XP and each wing's experience (the wing card) at the start.
T2 `hoi4_air_hold {wing:<one of them>}`: the reply `training_stopped:true`; the wing card shows no mission; the other wing
still trains. Record whether the game took the click as a toggle (training off) or opened a mission picker.
T3 `hoi4_air_mission` training for the held wing again; run 10 game days at speed 5 with fuel above the plan's floor: record
air XP per day and fuel per day against g115a's 0.047 air XP a day.
T4 plan executor: a plan with `manage.air_training_days` set to the facts value (or 10 for a quick check) and the wings
on training: after that many game days the executor posts hoi4_air_hold for each with the note "trained N days, the
game's training cap"; a wing is re-trained only when its aircraft count rose (new planes). Record the days each stopped.
T5 a refused wing: LG Greifswald (g115a issue I0002, refused_by_game_check on training): its plane type, its base and
the mission button's tooltip, so the refusal can be named.
Pass: T1-T4. Then the air training rule may be turned on in the strategist's default (`manage.air_training_days`).

## Capture C-RAID: a raid made by hand, for the raid tool (mouse; about 20 minutes) -- before the raid tool is built (a 1940 need)
Not pass/fail: the offline engineer needs the live layout of a raid instance and of the create command to build `hoi4_raid`
(create, read, launch, cancel) for the Eben-Emael step (GER_eben_emael_raid: focus GER_expanding_the_luftwaffe, tech
paratroopers, a division with 2+ paratrooper battalions, 5 transport planes, 30 days to prepare, 20 command power,
Gotterdammerung). Static facts so far (1.20.0.0, engineer 120): CCreateRaidCommand constructor 0x1014d0740 (tag, CRaidType*,
SRaidTarget 0x38 B, CRaidUnit 16 B, SRaidStartingPoint 40 B, int, bool); the commands NRaids::NNet::CCreateRaidCommand,
CExecuteRaidCommand, CCancelRaidCommand, CRemoveRaidCommand, CSetRaidRiskLevelCommand, CSetRaidAutoLaunchOption,
CSetRaidAutoComplete; instances are NRaids::CRaidInstance (a reference object).
Bed: any save where a raid is offered (the 1936 Germany start has none: use a bed with the paratrooper tech and a
paratrooper division, or a nation with an air raid such as a strategic bomber raid if offered). Module log on, HOI4_TRACE=1.
R1 open the raid view (the raids button), screenshot the offered raid types and targets.
R2 create ONE raid by hand (record type, target, the division and planes assigned, risk level shown), screenshot; save.
R3 ask the module for the last posted command of each type above if the log shows it (type id and size), and save the
game; keep the save under the run folder (not in Git).
R4 cancel that raid by hand; save again.
Hand the three saves, the screenshots and the log to the air engineer: the save's raid block and the posted command bytes
give the layouts the tool needs.

## Session S23-WEATHER: the weather evaluator against the game's own weather (read only; about 10 minutes) -- any time
What it proves: hoi4_weather's estimate agrees with what the game shows, so the operation step's ground gate
(`operations[].weather_gate`) holds a declaration only for real mud or snow.
M1 a 1936 save, paused on 1 January: `hoi4_weather {"states":[<a Moscow-area state>,<a Paris-area state>,<a Libyan
state>]}`: the Russian region reads ground snow medium/high and an attack factor at or below -0.2, Paris none or light,
Libya clear. Compare with the weather map mode (screenshot): snow where the tool says ground snow.
M2 the same states with `"date":"1936.4.15"`: the Russian region in mud (mud_chance >= 0.5) and
bad_ground_clears_in_days > 0; then let the game run to mid-April (or load a save of that date) and check the map mode
shows mud there.
M3 `"date":"1936.7.15"`: no region on bad ground.
Pass: M1-M3 agree with the map mode on snow/mud regions. A mismatch: record the region, the date and the tool's water /
snow numbers in the notes (the model's drying or melting rate is then off).
## Session S21-ARMYHQ: army headquarters, whether the mechanic is in force (read only; about 2 minutes) -- any time
What it proves: `hoi4_army_hq` reads the install's DLC state and the defines the way the game loads them.
M1 any save, paused: `hoi4_army_hq {}`. On an install without Thunder at Our Gates (the owner's: 10 DLCs, none of
them TaoG): `active` false, `why` names the DLC and "returns 1.0"; `worth.with_hq` close 0.95 / medium 0.9 / far 0.85
for planning cap and speed, `without_hq` 0.8; `hq_battalion_dlcs` ["Thunder at Our Gates"]. Cross-check in game: an
army's planning tooltip shows no HQ line and no deploy button exists on the army card.
M2 (only on an install with the DLC): `active` true; the army card shows the deploy button; the planning tooltip of an
army without a deployed HQ shows the x0.8 line. Record both and the deploy days of the default HQ template.
Pass: M1 as stated (M2 when the DLC is present). Nothing is posted; no family to switch on.
## Session S24-ABILITIES: the ability catalogue against the army card (read only; about 5 minutes) -- any time
What it proves: hoi4_army_abilities lists what the game offers and its cost reading is right, before the order
(CTriggerAbilityCommand, module family to come) is built on it.
M1 any save with a general on an army: `hoi4_army_abilities {}`: 14 generic abilities (1.20.1.0) plus the country's
own; command_power equals the top bar. Open the army card's ability list: the same keys; hover staff_office_plan: the
tooltip's "Base cost 0.12 (each for N battalions)" -- write N into the notes (the battalion group the cost is per).
M2 the daily cost the tooltip shows for that army = 0.12 x (its battalions / N), rounded as the game rounds.
Pass: M1-M2. The order itself is a later session.
## Session S22-REGIMENTAL: the Division Designer's regimental support row (mouse; about 15 minutes) -- any time
What it proves: hoi4_template_create / hoi4_template_edit fill `regiment_support_grid` and read the row back from the
template data (support array + 0x20, read from the executable, not yet seen live).
M1 a save with army experience >= 60 (Germany 1936 has 40-ish: use 1938+), paused: `hoi4_template_create {"name":"Test
Regimental","regiments":[["infantry","infantry","infantry"],["infantry","infantry","infantry"]],"regimental_support":
["<a company the designer offers, e.g. field_guns or heavy_weapons_company>"]}`: Verified; `template_before` absent;
open the designer on "Test Regimental": the company stands under column 0, column 1's slot is empty.
M2 `hoi4_template_edit {"template":"Test Regimental","regimental_support":["","<same company>"]}`: Verified; the company
moved under column 1 (removed from 0). A second identical call: nothing_to_change.
M3 refusals before input: a column of 2 battalions with a company under it: regimental_slot_closed; a battalion key in
the row: unit_wrong_slot.
M4 `hoi4_template_edit` changing only `support` on a template with a regimental row: the row is unchanged (read back).
Stop at once on: a slot clicked that is not the row's, a mismatch error naming regimental, or Save with another layout.
Pass: M1-M4. Then delete the test template (hoi4_template_delete).
## Session S27-TRAINROUTE: fresh divisions to their armies by command (in process, family `units`; about 15 game days) -- any time
What it proves: hoi4_army_detach without `army` moves divisions that stand outside any army into a named army, and the
executor's training[].armies routing sends a row's fresh divisions to its listed armies, so one row per template suffices.
T1 a save with a division outside any army (train one, or detach one into a new army and dissolve it): `hoi4_armies` lists
it under unassigned; `hoi4_army_detach {"divisions":["<id>"],"into":"<an army with room>"}`: Verified; hoi4_armies shows it in
that army; the army card lists it. A division inside an army without `army`: invalid_target before any post.
T2 executor on, plan with ONE training row {"template":"<infantry>","queue":4,"batch":2,"armies":["<army A>","<army B>"]}:
the deployment view shows ONE queue per train action with its lines (no per-state rows); within a day of each deployment
the digest shows "trainees N <template> -> <army>" and the divisions stand in the army with fewer divisions.
Pass: T1-T2. Stop on: a division moved twice in a row between the listed armies, a refusal repeated within 7 days.

## Session S26-PLANKEYS: the plan keys of engineer 117 in one executor run (no new input path; about 20 game days) -- any time
What it proves: `production[].until` with `stock:<equipment>`, the atoms `divisions:<template>`, `mastery:<track>`,
`banked_mastery:<track>`, `project_done:<key>` and `focus_when` read what the screens show, before a campaign relies on them.
P1 a 1936 save of the plan's country, executor on, plan with: a production row `{"equipment":"<rifle variant>","factories":<n>,
"until":"stock:infantry_equipment >= <stock now + 300>"}`; `focus_when` holding the next focus until a date 10 days ahead; a
`special_projects` object; digest predicates `divisions:<a template> >= 0`, `mastery:<a track> >= 0`, `banked_mastery:<a track> >= 0`.
P2 the digest's predicate values: divisions equals the count of that template on the army screen; mastery and banked mastery
equal the doctrine view's track (write both); the focus lane line says "kept for their window: <focus> (until ...)".
P3 run to the day the store passes the figure: the next day's production tick removes that line (one hoi4_production_remove),
the notes show produntil:<i>:<equipment>; restart the server: the line is not added back. On the window date the focus starts.
P4 (when a project completes in the run, else later) project_done:<key> turns true the week it completes.
Pass: P2-P3 (P4 when seen). Stop on: a second removal of the same line, a line added back after the restart.

## Session S25-VOLUNTEERS: volunteers and the attaché through the one diplomacy tool (module family `diplomacy`; about 40 minutes) -- part of the diplomacy session, after S10-DIPLOMACY
Owner, 28 Nov: ONE dynamic diplomacy tool; there is no separate volunteers tool. The calls below are `hoi4_diplomacy_act`
with action `send_volunteers` / `recall_volunteers` (+ `army`, `division_ids`) and `send_attache`.
What it proves: `hoi4_diplomacy_act` sends and recalls volunteer divisions and sends an attaché by the diplomacy view's own
actions (no dialog), the game's own legality refuses what the view greys, and nothing else changes. Launch with
`HOI4_BACKEND=inprocess` and the opt-in naming `diplomacy`.
Bed: the 1936 start as Germany, run to the Spanish Civil War (it fires in 1936 in the vanilla files; the two Spanish
tags at war: SPR and SPD, or as the game names them in `hoi4_diplomacy_state`), paused. Germany holds >= the define's
VOLUNTEERS_DIVISIONS_REQUIRED divisions at the start.
V0 `hoi4_inproc_status` (or the hello): family `diplomacy` on, types diplomacy.send_volunteers / .recall_volunteers / .send_attache on with
act_units 0x70/0x90/2 and 0x70/0x90/4 (the offline scan's values) on 1.20.1.0.
V1 refusals before the war: `hoi4_diplomacy_act {action:send_volunteers, country:<a Spanish tag>, army:<a 2-division army>}` before the
civil war: refused_by_game, nothing posted, the army unchanged (`hoi4_armies`).
V2 the view's cap: open the diplomacy view on the side Germany may help (read only, no click) and note the volunteer
limit it shows; compare with 0.05 x the receiver's provinces + 0.05 x our divisions (defines VOLUNTEERS_PER_TARGET_PROVINCE,
VOLUNTEERS_PER_COUNTRY_ARMY): record both numbers.
V3 send 3 divisions of one army (division_ids): the reply shows 2 actions posted (2 + 1); within 2 game days the
diplomacy view lists 3 volunteer divisions in transfer (VOLUNTEERS_TRANSFER_SPEED days); world tension rises by about
3 x TENSION_VOLUNTEER_FORCE_DIVISION. Record where the divisions appear in our memory read (army, theatre, location) on
the day of sending and on arrival: this is the read-back the tool still lacks (journal says Unconfirmed today).
V4 over the cap: send more divisions than the shown limit: the game refuses the actions past the limit (refused_by_game
or a partial post: record which).
V5 recall: right after arrival `action:recall_volunteers` on those divisions: refused (RECALL_VOLUNTEERS_AFTER_ARRIVAL_COOLDOWN days);
then advance past the lock (or use a save past it) and recall 3 (`action:recall_volunteers`): the reply shows 1 action; the divisions come home in
about VOLUNTEERS_RECALL_DAYS_PER_UNIT x 3 days with most of their equipment (VOLUNTEERS_RETURN_EQUIPMENT).
V6 air volunteers (read and one try): `hoi4_air_rebase` of one fighter wing to a Spanish air base of the side we help:
record whether the tool accepts the foreign base and whether the game moves the wing (expected: the game's own transfer
check decides; the tool may refuse a base it does not list as own: record the refusal text).
V7 attaché: `hoi4_diplomacy_act {action:send_attache}` to the side we help (the view's own row; the module op diplomacy.send_attache is built and off until this session): PP drops by the define BASE_SEND_ATTACHE_COST and command power by
BASE_SEND_ATTACHE_CP_COST (or as modified), the diplomacy view shows our attaché; a second call is refused.
Stop at once on: any post the game did not refuse while the view greys the button, a division leaving that was not
named, an attaché or volunteers to a country not named, a module `unexplained` result.
Pass: V0, V1, V3, V5, V7 (V2, V4, V6 recorded). Then set LIVE_PASSED in native/src/launch/macos/volunteers.rs and the
`volunteers` entry of IPC_LIVE_PASSED in inproc/core/ipc.c to 1, with the session's numbers in the comment.

## Session S27-DIPLOMACY-ANY: every diplomatic action by its key, the catalog and the game's own answer (module family `diplomacy2`; about 30 minutes) -- part of the diplomacy session, with S25-VOLUNTEERS
Owner, 28 Nov: ONE dynamic diplomacy tool for any diplomatic action. What it proves: `hoi4_diplomacy {catalog:true}` lists
every diplomatic action class the game has (43 on 1.20.1.0: the module's scan of the game's own action classes) plus the
content's scripted actions, each with its shape and the game's own "can do" toward a country asked WITHOUT sending; and
`hoi4_diplomacy_act` posts a catalog key the module builds (family `diplomacy2`) only when the game's check passes.
Launch with `HOI4_BACKEND=inprocess` and the opt-in naming `diplomacy` and `diplomacy2` (HOI4_INPROC_POST=...,diplomacy,diplomacy2).
Bed: the 1939 bookmark as Germany (w92b or the game's own 1939 start), paused; Spain or a minor at peace as a neutral target.
D0 hello: family `diplomacy2` on, 14 types on (embargo, docking_rights, offer_docking_rights, air_base_access,
offer_air_base_access, naval_blockade, peace_proposal, reduce_autonomy, increase_autonomy,
request_access_to_license_production, cancel_licensed_production, return_all_expeditionary_forces, generate_war_goal,
send_expeditionary_force).
D1 catalog read: `hoi4_diplomacy {catalog:true, country:"SPR"}` (or the neutral tag): 43 rows (+ scripted rows if a mod adds
any), every row has shape and route; rows whose shape is unsupported are present (justify_war_goal constructor_args_1,
stage_coup dialog_fields, scripted_diplomatic scripted_type ...). Open the diplomacy view on the same country (read only,
no click) and compare: for each simple row, can_do true exactly where the view's button is enabled. Record any row
where they differ (key, can_do, the view's state, the tooltip text). Measure the call's time.
D2 nothing sent by the read: political power, relations (`hoi4_diplomacy_state`) and world tension unchanged after D1.
D3 a refusal: `hoi4_diplomacy_act {action:"increase_autonomy", country:"SPR"}` (not our subject): refused_by_game, nothing posted.
D4 a post: `hoi4_diplomacy_act {action:"embargo", country:<a neutral we do not trade with>}` while can_do is true: the reply
shows 1 posted; the view shows the embargo; record PP before and after. Then the SAME call again: it must NOT lift the
embargo silently: record whether the tool refuses (nothing_to_change) or the game toggles it off. If it toggles, the
`embargo` mapping in inproc_diplomacy::op_of must be removed before this family is switched on (report it).
D5 docking rights: `{action:"ask_docking_rights", country:<an ally or friendly minor>}`: posted only when can_do is true;
the AI answers when the game runs (Diverged until then); record the answer after 2 game days.
D6 a key the module cannot build: `{action:"justify_war_goal", war_goal:..., states:[...]}` (no war_goal_key) keeps the
mouse path (the view's dialog); a catalog key with an unsupported shape and no view row (e.g.
`{action:"request_foreign_manpower", country:...}`) is refused dialog_unsupported and nothing is sent.
D7 justify by key (engineer 125; written, module catalog row generate_war_goal, shape war_goal_type): D1's catalog shows
generate_war_goal with shape war_goal_type and `war_goal_types:true` in the module's hello. Against a neutral minor we have
no goal on (e.g. LIT or a minor the 1939 bed offers): `{action:"justify_war_goal", country:<tag>, war_goal_key:"take_state",
state_ids:[<one of its states>]}`: the reply shows 1 posted; the view's war goal list (or hoi4_diplomacy_state
own_war_goals) shows the justification in progress toward that country with that state; record PP and world tension
before and after, and the justification days the view shows. Then `war_goal_key:"no_such_goal"`: refused, nothing posted
(the module answers "no war goal type of that key"). Then `war_goal_key:"annex_everything"` with no state_ids against a
country where the view offers no such goal: refused_by_game, nothing posted. Stop at once if the justification names a
different goal type or state than asked.
D8 an expeditionary force (engineer 125; written, catalog row send_expeditionary_force, shape division_list): only on a
bed where an ally at war accepts expeditions (Germany and Italy at war, Italy in our faction; else Japan-China with a
faction ally): `{action:"expeditionary_force", country:<the ally>, army:<an army of 1-2 divisions>}`: the reply shows the
divisions posted; the divisions turn to the ally's control (the army card's flag, or hoi4_units owner); then
`{action:"return_expeditions", country:<the ally>}` returns them. Not at war or no ally: refused_by_game, nothing posted.
Record the division count before and after.
Stop at once on: any post while the view greys the button, any change after D1/D2, a module `unexplained` result, a
toggle that removed a relation we had.
Pass: D0-D4, D6, D7 (D5 and D8 recorded; D8 when the bed allows it). Then set the `diplomacy2` entry of IPC_LIVE_PASSED in
inproc/core/ipc.c to 1 with the numbers. D0's type count is 14 with engineer 125's module (generate_war_goal and
send_expeditionary_force added).

## Capture C-LICENCE: what the licence request action carries (read only plus one licence request; about 15 minutes) -- with the diplomacy session
Not pass/fail: it gives the offline engineer the meaning of CRequestLicensedProductionAction's two equipment lists
(+0x70 and +0x88) and its byte +0xa0, which the binary alone left open (one list is executed with 100000, the other with 0).
Server env: `HOI4_BACKEND=inprocess HOI4_TRACE=1`, module log on. Bed: 1939 Germany or 1936 Germany, paused; a country with
a licensable design we lack (Italy, Japan, the USA: the view's Request licence row).
L1 open the diplomacy view on that country, Request licence production: the dialog lists equipment; screenshot it.
L2 choose ONE equipment (record its name and version as shown) and send; then save the game (licence-before/after saves
are not needed in Git: keep them under the run folder).
L3 the module's `diplomacy2` catalog read toward the same country before and after (`hoi4_diplomacy {catalog:true,
country:<tag>}`): request_licensed_production row's can_do both times.
L4 the module log's last posted command trace for the action (the game builds it itself here, so ask the module for a dump
of the last action of type 0x37bf if the log has it; else note "not traced").
L5 record: equipment chosen, the AI's answer after 2 game days, the licence line in the production view (cost, whether the
licence shows under the equipment).
Hand the folder to the diplomacy engineer.

## Capture C-DESIGNER: why the designer base reads fail (read only, mouse opens and closes the designer; about 15 minutes) -- any time, before the next designer session
Not a pass/fail session: it records what an offline engineer needs. s115b (28 Nov) failed `hoi4_design_options` with a base:
tank 4517 and plane 1320 `read_bound` "Invalid address or snapshot read budget" (a pointer outside the address range or one
read over 1 MB, while reading the OPENED designer), plane 1323 `read_failed`, ship 1348 `design_not_offered` (1348 is not a
design id the Production list offers: probably an equipment type id passed as a design id).
Server env: `HOI4_TRACE=1 HOI4_TRACE_READS=1` (the second prints `[read_bound] address=0x.. size=.. snapshot_bytes=..` or
`[read_failed] address=0x.. size=..` on stderr for the refused read). Bed: the same save as s115b if kept (else w92b or the
1936 study end GER_1936_05_05_05), paused. Save everything under `<run>/designer-capture/`.
C1 the lists: `hoi4_design_options {kind:"tank"}`, `{kind:"plane"}`, `{kind:"ship"}` -> tank.json, plane.json, ship.json; and
`hoi4_designs {kind:...}` for the same three -> designs-<kind>.json (to see whether 1348/4517/1320/1323 are design ids, which
types, which designer: upgrade or modules).
C2 for each of: tank 4517 and the first listed default tank chassis (e.g. 1333), plane 1320, 1323 and the first listed default
plane airframe (e.g. 1318), ship 1348 and one listed ship hull id: call `hoi4_design_options {kind, base}`; save the reply as
<kind>-<base>.json and the server's stderr lines of that call (the [read_bound]/[read_failed] lines and the HOI4_TRACE steps) as
<kind>-<base>.trace.txt; take ONE screenshot while the designer is open (before the tool closes it; if it closes at once, the
screenshot right after the error) as <kind>-<base>.png.
C3 for the failing bases also run `{kind, base, slot:<one slot name from designs-<kind>.json>}`: does the one-slot scan pass?
(save as <kind>-<base>-slot.json).
C4 record the build (master sha of the server and module), the game version, and whether each base's designer is the
module designer (slots) or the upgrade designer (rows) on screen.
Hand the folder to the air engineer: the trace's address and the screenshot tell which reader followed a wrong pointer.

## Session S25-MIOKEY: organisation traits by key (in process, family `mio`; about 5 minutes) -- any time
What it proves: hoi4_mio_trait `trait` / `traits` resolve the scripts' trait keys to the tree through the module's
organisations read, and the executor's `mio[].traits` keys reach the game.
K1 Germany 1936, paused, `HOI4_BACKEND=auto` (mio passed live): `hoi4_mio_trait {"organization":"GER_henschel_organization",
"list":true}`: write the first five entries' `trait` keys. Then `hoi4_mio_trait {"organization":"GER_henschel_organization",
"trait":"<the key of an entry with unlockable_now true>"}`: Verified; the tree window shows that trait (by its icon and
tooltip name) unlocked or queued, the index in the receipt is the entry's index from the listing.
K2 `{"traits":["<two keys not held>","<one key already queued>"]}`: Verified, the held one counted as reached.
K3 refusals before input: `{"trait":"GER_mio_trait_nope"}` -> trait_unknown naming Henschel's keys; with
`HOI4_BACKEND=gui` -> trait_key_unresolved; nothing posted in either case.
K4 the window's ORDER: hover the tree icon the listing calls index 0 and index 3: their tooltip names match the keys'
localisation (this settles whether the mouse path's indices equal the module's; record it either way).
Pass: K1-K3 (K4 recorded). Then the plan's `mio[].traits` may carry keys.

## Session S28-REFUSALS: the refusal classes of g115a do not repeat (no new input path; part of the next short 1936 study) -- any time
What it proves (engineer 123, 28 Nov): a refusal changes the next decision instead of being retried. Read from the study's
issues.jsonl, game-actions.jsonl and the executor notes; nothing extra to click.
R1 navy: with ships in the reserve fleet, `hoi4_fleet_assign` out of it is sent at most ONCE by the executor; on the mouse
backend it is refused reserve_fleet and the navy_shape note says "its task forces cannot be moved on this backend"; with
`HOI4_INPROC_POST` naming navy it is posted in process (task force leaves the reserve; a new step of the passed `navy`
family: watch the first one by eye, the fleet list shows it in the receiving fleet).
R2 intelligence: an operative a `networks[]` entry names is not sent elsewhere by `operatives[]` (note "held by the
network entry for <TAG>"); `hoi4_operative_recruit` refused operative_slot_locked waits the days the refusal names.
R3 air: a wing the game refuses training for (refused_by_game_check) is tried once; its aircraft type is not sent
again (note "<type>: the game refuses training for it").
R4 research: after a research start that reads NotApplied, no second technology goes into that slot in the same tick.
R5 focus: after the game refuses a focus, no focus the files could not judge follows in that tick (focus_why "not judged").
R6 general trait: an assignment whose window observation is lost settles from the general's trait list (Reached or
Unchanged, never "Diverged unverified"); the mouse path still fails on 1.20.1 (trait tree window not found, s115/g115a):
report the error code; the in-process route (CLearnTraitCommand) is not built.
R7 idle table: a failing focus start counts in the focus row (tool faults map to the sink of their tool).
Pass: R1-R5 and R7 with no class repeated more than once; R6 reported.

## Session S29-SWAPS: doctrine replace by command, the research juggle, projects on idle facilities (about 30 game days) -- any time
Build: e123 (tools 123). A save with a grand doctrine already chosen (Germany: Mobile Warfare from history) and enough army XP.
D1 doctrine replace (family `doctrine` on): hoi4_doctrine_adopt {doctrine: grand_battleplan, replace: true}. Expect Verified
in 1-2 s, the land folder's grand doctrine GBP in hoi4_doctrines, army XP down by the game's own price; record the
mastery left on the old tracks. With the family off, the same call is refused grand_doctrine_set and nothing is posted.
D2 the same call again: already_adopted.
J1 research juggle: plan research_waits [{tech: <a tech that opens in a few days>, slot: <an idle slot>, fill: <a startable
tech>}]. Expect the filler started in that slot; when the awaited tech opens, one hoi4_research_start with replace:true
into that slot; the filler's progress is still shown when it resumes later (the game keeps it).
P1 hoi4_special_projects on an idle facility that once ran a project: project none, `stopped` true on that project;
hoi4_special_project_assign of an available project: Verified (rerun S19 P1-P5).
Pass: D1, D2, J1, P1.

## Session S30-FAULTS: the g126b, p126a and s126a faults at their roots (read and mouse; about 30 minutes plus 20 game days) -- any time
Build: e127 (tools 127). Any 1936 save of a country with operatives, trade partners and a few templates. Nothing new is posted
in process; every step is a tool call or an executor run.
F1 operative replace: hoi4_operative_assign {operative, mission, ..., replace: true} on an operative on a mission. Expect it
accepted by the argument check (it was invalid_arguments) and settled. The same call without replace on an operative the
save gave a real mission (not counter-intelligence at home, not idle): refused mission_running, nothing clicked.
F2 trade row below the list: hoi4_trade_set for a partner whose row is below the visible list (the 10th or later). Expect
the list wheeled until the row is inside the band, then the click; refused partner_row_offscreen if the list does not move.
It was "Point outside the owned game client area".
F3 one training queue: hoi4_train twice for the same template and province (2 then 6). Expect ONE deployment queue of 8
lines; the second reply has reused_queue and lines_before 2. A queue at another province is not reused.
F4 train line: a day-one executor run with a train production line already open: no second train line (note
"train line open"). Read hoi4_economy production_lines in the reply.
F5 focus refusal kept: a focus whose available block the files cannot judge (Germany: Anschluss before the army ratio) is
refused once; when another focus completes, it is NOT retried at once (it was retried after every completion).
F6 reconcile: a plan whose production names equipment that exists in the files but is not researched yet. Expect it in
research.production_not_yet_available, not in unresolved, and the run starts.
F7 operation ledger: a refused refinement in an operation shows "refused [<the tool's answer>]: <what>" in the ledger.
F8 paradrop guard: hoi4_paradrop under FAIR without roleplay: refused fair.paradrops BEFORE any in-process check (it was
in_process_unavailable first). With plan airborne.roleplay_request true: passes the guard. Under multiplayer rules: refused.
F9 template regimental row: hoi4_template_get on a template with field guns or another regimental company: regimental_support
lists them (compare the designer's row by eye). hoi4_template_options: regimental_companies lists the unit files' regimental
companies with unlocked matching the designer (a researched company unlocked true).
F10 abilities: hoi4_abilities: a country-limited ability shows its full requirement text (OR blocks included), no null keys.
F11 weather: hoi4_weather on a state in mud: clears_in_days a number, or bad_ground_clears_beyond_horizon true with the date;
bad_ground_all_year true only for a state whose files give bad ground the whole year.
Pass: F1-F11. Open (not built): the weather map mode; army_move by mouse in auto (needs S20 M6).

## Session S31-TRAIT: a general's trait by command (in process, family `leader`; about 15 minutes) -- any time
Build: e127 (tools 127). Module family `leader` named in HOI4_INPROC_POST and HOI4_BACKEND=inprocess. Any save with an own
general who has a free trait slot and enough command power (hoi4_general_trait list:true shows can_pick).
T0 hello: families.leader resolved (on true with the family named), live_passed false. scan evidence names "trait db got".
T1 hoi4_general_trait {general, trait: <a can_pick trait key>}: expect backend inprocess, assigned true within 2.5 s, the
trait in hoi4_commanders' trait list, command power down by the game's cost (UNIT_LEADER_ASSIGN_TRAIT_COST); no window
opened. The trait tree window afterwards shows it owned.
T2 the same call again: refused_by_game_check (already owned), nothing posted, no window.
T3 a trait whose prerequisite is missing, or with too little command power: refused_by_game_check, command power unchanged.
T4 an unknown key: trait_unknown before any post (the catalog); a key the module's trait database lacks (a mod trait not
loaded): the module refuses and the window path runs in the same call.
T5 by localised name (trait: "Desert Fox"): the same as T1 (the catalog gives the key).
PROMOTION (engineer 131, G004; the same family: it passes with T1-T9 or not at all). A save with an own general who is not a
field marshal (hoi4_commanders: Promote shown in the army panel's commander list) and command power at or above
PROMOTE_LEADER_CP_COST (40 on 1.20.1).
T6 hoi4_general_trait {general: <that general>, promote: true}: expect backend inprocess, promoted true within 2.5 s,
command power down by the cost; the army panel's commander list shows him with no Promote button (field marshal); he can
now be made an army group's commander (hoi4_army_commander on a group: no promotion step). No window opened by the tool.
T7 the same call again: promotion_refused (already a field marshal), command power unchanged, nothing posted.
T8 another general with command power below the cost (spend it first, or a save short of it): insufficient_command_power
"costs 40 command power and N are held", nothing posted.
T9 a field-marshal trait (for example logistics_wizard or offensive_doctrine, can_pick in list:true) on the general of T6:
assigned (T1's path); before T6 the same trait is refused for a corps commander.
Pass: T1-T9, no window opened in T1-T3 and T6-T8. Then IPC_LIVE_PASSED for leader may be set to 1 (the executor then
promotes the plan's commanders[].promote:true generals before their traits).

## Session S32-FACTION: faction goals and the spymaster role by command (in process, family `faction`; about 20 minutes) -- any time
Build: e131 (tools 131). Module family `faction` named in HOI4_INPROC_POST and HOI4_BACKEND=inprocess. A 1.20.x save where
we lead a faction with at least one empty goal slot and some faction initiative (any country: found one with
hoi4_faction_create first if needed), no spymaster yet. There is no mouse path: every step either posts or refuses.
T0 hello: families.faction resolved (on true with the family named), live_passed false; the scan evidence of faction.goal
names "goal db got" with entries +0x30 mask +0x3c stride 0x18 and "faction +0xd30/+0x2c0 goals +0x500/+0x50c x0x440".
T1 hoi4_diplomacy_act {action: faction_goal, list: true}: every goal of common/factions/goals with key, name, category,
group; nothing posted.
T2 hoi4_diplomacy_act {action: faction_goal, faction_goal: <a goal the faction view offers for an empty slot>}: expect
backend inprocess, posted 1, executed 1 (goal_in_list true) within 3 s; the faction view shows the goal in its slot and
the initiative drops by the goal's cost. Note the read-back time.
T3 the same goal again: refused_by_game_check (already set), nothing posted, the view unchanged.
T4 a goal of a category whose slots are all full: refused_by_game_check, nothing posted.
T5 a goal whose `visible` fails for us (another country's goal, for example faction_goal_one_germany outside Germany):
refused_by_game_check, nothing posted.
T6 by localised name ("Industrial Expansion"): the same as T2 for that goal (the catalogue gives the key); an unknown name:
faction_goal_unknown naming the closest keys, nothing posted.
T7 hoi4_diplomacy_act {action: become_spymaster}: posted 1; the faction view's intelligence panel names us spymaster and
the initiative drops by the defines' cost. The call again: refused_by_game_check (the slot is taken), nothing posted.
T8 outside a faction (leave it or another save): faction_goal and become_spymaster both refused_by_game_check, nothing
posted, no crash.
T9 the executor: a plan with faction.spymaster true and faction.goals [two goals, the first of a full category]: the
spymaster once, the first goal refused and waiting a week while the second goes; no repeat of a set goal.
Pass: T0-T9, no window opened by the tool, the goal's exact class accepted (a refusal "not_a_live_object" in T2 means the
goal database's value is not exactly a CFactionGoal: stop and report). Then IPC_LIVE_PASSED for faction may be set to 1.

## Session S33-DECRYPT: decryption start, stop and activation by command, and the cipher read (in process, family `decrypt`; about 25 minutes) -- any time
Build: e133 (tools 133). Module family `decrypt` named in HOI4_INPROC_POST and HOI4_BACKEND=inprocess. A 1.20.x save with
an agency (created) and preferably one cipher already broken; the 1939 bookmark run a few weeks (or s129a's 1940 bed) does.
There is no mouse path: every step either posts or refuses. FAIR: the cryptology view's own commands only.
T0 hello: families.decrypt resolved (on true with the family named), live_passed false; the scan evidence of decrypt.start
names CStartStopDecryptionCommand size 0x30, type 19064; decrypt.activate type 19072.
T1 hoi4_intelligence: `decryption` state_read true; `ciphers` lists every country the agency worked on (target, decrypting,
bonus_active, bonus_days_left, progress_raw) and `target` the one decrypted now; compare with the cryptology view (Shift+W,
decryption tab): the same country decrypting, the same broken ciphers. Note progress_raw of the decrypted country on two
days (the daily gain) and the view's percentage, so the threshold can be dated in a later build.
T2 hoi4_agency_upgrade {decryption: {target: <a country not decrypted, not broken>, action: start}}: expect backend
inprocess, posted 1; Verified by memory within the window (its record decrypting true); the view shows it decrypting. If
another country was being decrypted, note whether the game moved the work (one at a time) or refused.
T3 the same call again: nothing_to_change before any post.
T4 action stop for it: posted 1, Verified (decrypting false); the view agrees. Stop again: nothing_to_change.
T5 start our own tag: invalid_target, nothing posted; start a country with no agency or a released tag: refused_by_game_check.
T6 activate a broken cipher (the view's Activate button enabled): posted 1, Verified (bonus_active true, bonus_days_left the
defines' days); the view shows the bonuses active and the decryption of that country stopped. Activate again:
nothing_to_change. Activate a country never worked on: cipher_not_broken. Activate one being decrypted but not broken:
refused_by_game_check, nothing posted.
T7 plain load of a save made after T6: the bonus still active, days running down; the records read the same.
T8 the executor: intelligence.decrypt [<broken one>, <open one>] with no country decrypting: the first refused (broken)
and waiting 30 days while the second starts; intelligence.activate [{target: <broken one>, operation: <an op id whose
execute_on holds now>}]: activated once, no repeat.
Pass: T0-T8, no window opened by the tool, the read matching the view. Then IPC_LIVE_PASSED for decrypt may be set to 1.
