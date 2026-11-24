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
the later stages: `INPROCESS-FEASIBILITY-1123.md` in the harness folder; code: `inproc/` on branch
`research/inproc-feasibility` (build `inproc/build.sh`, offline proof `inproc/test_offline.sh`).

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
`CARGO_BUILD_JOBS=2 ./buildbin120.sh ~/Projects/_scratch/dev-wt/inproc-e71 v120-e71a` for 1.20 or the usual new-v* build for
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
