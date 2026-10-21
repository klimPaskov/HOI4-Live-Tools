# Linux-first native MCP continuation

The user replaced the Mac testing objective on 12 October 2026: build a fast
Linux-first Rust MCP on Clausewitz-MCP, then support Windows and macOS with
independently verified native adapters. Preserve the legacy Python package.
Do not resume Mac campaigns or control the shared Windows game.

## Current checkpoint

The Linux daily worker now has an explicit production allocation preview.
It requires the existing owned production profile, owner `daily_production: true`,
plan `production_permission: true` and `max_production_steps_per_day` from 1 to 16.
Defaults preserve the previous Research/Focus worker. The production permission
and step limit cannot change during a worker lifetime.

A private durable progress file binds the original full own line/variant,
equipment, campaign, roots and starting count. Opening and intermediate one-click
steps consume the separate production step budget, never goal completion.
The final step or matching-count no-op uses the governor's request and completion
budget, with typed ordinary-journal proof, canonical readback and a later daily
observation. Larger changes can span days. Reopen preserves counts, budgets,
original failures and the date of late read-only settlement. Changed identities,
external allocation changes and uncertain delivery cannot establish a new baseline.
Plan updates preserve the existing target order for each full line ID.

The worker executes at most one production step per batch under the existing
shared controller, lease, input gate and absolute deadline. Production runs before
Research/Focus when opted in, bounded by its separate daily step limit and the
remaining shared goal budget. Cross-panel handoffs are not implemented: an
incompatible active panel refuses safely. A preparation error stops this preview;
a known no-input final refusal retains the goal for a later day. Unresolved
progress blocks all shared input, including restart with production permission off.
STOP and compact digests retain production history and its dated budget; historical
progress is not a fresh factory observation or input authority.

Local validation passed 1,089 Rust tests, 953 Python tests (37 skips), formatting,
host/Linux Clippy and Windows cross-compilation. All 34 targeted offline tests
also passed inside the owned Linux VM, including the Linux-only worker permission
and digest tests. No game input was used. Tests cover multi-day increases,
decreases, no-ops, retained budgets, invalid jumps, changed identities, no-input
refusals, uncertainty, stale receipts, chronology and failed durable writes.
These are offline results. Production004 remains the latest live production gate;
no new game was launched. The previous foundation commit passed Linux/Windows
native CI and all six Python jobs; its macOS native job was still queued at the
last check, so a complete CI pass is not claimed.

Next: require exact-commit CI and run a bounded recorded advancing-time production
worker gate with one Luna medium player, including multiple daily steps, final
allocation, restart/STOP and no-replay evidence. Do not repeat manual Production004
alone. Worker live acceptance, broader panel routing, two-year/full campaigns,
and Windows/macOS native live adapters remain incomplete.

### Previous standing-production foundation

Standing-production foundation, 22 October 2026: factory executions now retain
an internal typed journal proof, excluded from public JSON. The plan converter
requires the exact original own line/variant, request, campaign, equipment and
final count, a fresh canonical line read and a later daily observation sequence.
Panel opening, intermediate targets and historical replays cannot complete the
allocation. Mutating a public summary cannot retarget its private proof or turn
a historical receipt into fresh evidence. Public receipt and journal formats
remain unchanged.

Production targets are ordered per full line ID. A refused head blocks later
targets on that line while unrelated lines can proceed. Tests use the real
durable production journal and governor with a generated backend: completion
consumes budget once and survives reopen without replay. The existing manual
Linux route was factored into a shared owner-held executor with the same lease,
input gate, deadline, journal and scoped checks. No new MCP action or worker
permission is enabled by this foundation.

Local validation passed 1,077 Rust tests, 953 Python tests (37 skips), formatting
and warning-free host/Linux Clippy. This is offline evidence, not a new live
run. Production004 remains the latest successful bounded live production test.
Its three previously queued macOS CI jobs were later cancelled with no test
steps; Linux/Windows jobs passed. It does not have a complete CI pass.

Next: wire an explicit default-off production permission into the daily worker,
with durable original-line preparation, bounded one-click progress toward the
full requested count, daily budgets, shared uncertainty exclusion and compact
fresh digests. Opening and intermediate steps must not complete the goal; errors
must retain unfinished work and its original identity. Then use one Luna medium
player for a recorded advancing-time worker gate, including restart/STOP and
no-replay evidence. Do not repeat the passed manual production gate alone.

### Latest recorded production gate

Production004, 21 October 2026 (`df474ca`), passed all ten bounded production
steps with one Luna player at medium reasoning. The recorded fresh start was
vanilla FAIR Germany, Normal difficulty, historical AI, paused at 1 January 1936.
One ordinary topbar click opened production in 2.080 seconds. The exact infantry
equipment line then changed 10→11 and 11→10 requested factories in 765/773 ms,
with canonical readback after each ordinary click. Final state had 10 requested
and 10 active factories. Same-ID opening/increase replays were historical and
sent no new input; new-ID already-open and matching-count requests were no-ops.
The control observation took 76 ms, line read 43 ms and overview 16 ms over the
persistent MCP/SSH bridge. These few samples are not a matched upstream benchmark.

The stopped journal contains exactly three intents, five verified terminal
requests, no unresolved action and no clock entries. Luna released its lease and
status confirmed zero remaining time. The 259.333-second recording passed
integrity checks; recorder exit 255 remains separate from normal game exit 0.
Exact game, supervisor, recorder and bridge absence was verified. Private evidence
and prior failed gates are retained. No screenshots drove the campaign tool steps;
menu setup and the recording are separate. This paused diagnostic does not
establish continuous-play throughput, universal routing or event causality.

Local checks passed 1,070 Rust tests, 953 Python tests (37 skips), formatting and
warning-free host/Linux Clippy. Linux/Windows CI passed on the tested commit;
three macOS jobs were queued at the grant, with local macOS checks passed. This
is not release acceptance. Next: extend standing economy plans and the remaining
ordinary Linux adapters. Scrolling, new production lines, provincial placement,
army/air/naval control and clean two-year/full campaigns remain open. Do not
repeat the passed bounded production gate alone.

### Previous production parent-gate checkpoint

Production003, 21 October 2026 (`855d5fb`): one Luna medium player opened
production through one ordinary topbar click in 2.18 seconds. Canonical state
verified the shown panel. Same-ID historical replay took 6 ms; a new-ID
already-open no-op took 35 ms and delivered no input. The recorded fresh start
was vanilla FAIR Germany, Normal difficulty and historical AI. No clock action
was issued; this paused diagnostic does not measure continuous-play performance.

The first factory increment refused before intent or input in 85 ms. Captured
controls showed both named buttons enabled, but their nonnull optional parent
gates failed the reader's null-only predicate. Those gate identities and payloads
were not captured, so the failed run cannot establish their type or clearance.
The corrected observer now reuses the opener's independently mapped exact
own-container position join, with the same snapshot witnesses and dispatch
identity scope. Foreign pointers, unknown types, false/nonboolean flags and
other blockers still refuse. Factory dispatch remains unverified live.

Luna stopped immediately, released its lease and confirmed zero remaining time.
The journal contains a verified opening and no-op, no factory intent and no
unresolved action. The 482.333-second recording passed integrity checks; recorder
exit 255 was retained separately from game exit 0 after ordinary window close.
Exact game, supervisor, recorder and bridge absence was verified; evidence stays
private. Linux and Windows native CI and all six Python jobs passed on `855d5fb`;
the macOS native job was cancelled, so this was not a complete CI pass. Local
macOS checks passed before the gate. Two-year and full-campaign acceptance remain
open. Next: validate the parent-gate correction and run a fresh bounded recorded
Luna medium factory gate.
Local correction checks passed 1,070 Rust tests, 953 Python tests (37 skips),
formatting and warning-free host/Linux Clippy. These are offline results.

### Previous production phase checkpoint

Production002, 21 October 2026: all nine CI jobs passed on `28afe8f`, including
19 Linux parent-reader and five Xvfb fixtures. Its rebuilt release binary was
byte-identical to `2ad13d0`, already launched before the test-only correction;
both source identities and binary hashes were retained in private evidence.
The fresh recorded vanilla FAIR Germany start was Normal/historical. An early
loading read reported speed zero and the initial date with `paused=false`; a
later read confirmed the settled paused start without any clock input.

Luna medium held exclusive input and stopped at the first production-open
refusal in 82 ms. The diagnostic isolated `idler_can_update_raw=0`; the other
reported blockers and named opener were clear. Geometry was not collected
because this prerequisite failed. Both action journals were empty. Luna released
the lease, the 304.666-second recording finalized, and ordinary window close
produced game exit 0; exact game/supervisor/recorder absence was verified.
No factory change, final count/date read or campaign acceptance occurred.

Static setter/caller evidence now classifies `CanUpdateGui` as the engine's
internal update phase. It is diagnostic-only for ordinary X11 actions; the
actual transition, ownership, own-dialog and callback guards remain enforced.
See `native/PROVENANCE.md`. No flag write, engine call or pause was added.
Local validation passed 1,067 Rust tests, 953 Python tests (37 platform skips),
formatting and warning-free host/Linux Clippy. This correction still requires
exact-commit CI and a fresh recorded Luna gate.

### Earlier diagnostic and fixture checkpoints

The diagnostic checkpoint `2ad13d0` then failed ten Windows CI fixtures: nine
production cases used a 50 ms budget and one construction terminal-tear case
used 300 ms, including durable journal writes. They expired before dispatch,
returning `input_not_queued` instead of reaching the intended post-input checks.
Both production fixture helpers and the construction case now use the existing
two-second test allowance. Runtime deadlines, durable writes and assertions
about dispatch counts, uncertainty and no replay are unchanged. Preserve the
failed run and require fresh exact-commit CI before the next input gate.

Recorded Linux production gate, 21 October 2026 (`87fc595`, all nine exact-commit
CI jobs passed): one Luna player at medium reasoning held exclusive input in a
vanilla FAIR Germany start, Normal difficulty with historical AI. The first
`hoi4_production_open` refused with `production_precondition` in 78 ms. Its
message combined several checks, so the specific failing predicate is unknown.
Luna stopped immediately; both production and clock action journals were empty.
No factory allocation, replay, final count or date check was attempted.

Luna released the lease and status confirmed zero remaining seconds. The private
recording finalized as two valid 512×288 segments (740 seconds total); recorder
exit 255 was retained separately from the game's normal exit 0. Exact game,
supervisor and recorder absence was verified, and evidence was archived privately.
The game started paused; no clock action was issued. This is a failed bounded
input gate, not continuous-play performance or campaign acceptance.

Opening refusals now report bounded, typed checks from the same captured
observation: manager mode, engine blockers, own panels/dialogs, opener callback
and local state, and geometry presence. Unknown values remain null. This adds
no memory reads or input and preserves every existing authorization predicate.
The first run cannot retroactively establish those missing diagnostics. Next:
run a fresh bounded recorded Luna gate after exact-commit CI. Local validation
passed 1,066 Rust tests, 953 Python tests (37 platform skips), formatting and
warning-free host/Linux Clippy. The initial Python command used the system
interpreter and failed imports due to missing dependencies; the repository
virtual environment passed the complete suite.

### Previous offline opening checkpoint

The first published opening checkpoint (`69d77ce`) exposed a test deadline
problem on macOS CI: the transition-recovery fixture combined durable fsyncs
and a real 20 ms backoff within a 50 ms test budget. It correctly returned
`action_uncertain` when that budget expired. The recovery fixture now allows
two seconds of scheduling headroom; runtime deadlines and uncertainty behavior
are unchanged. Preserve this failed run; verify the correction on fresh CI.

Linux production panel opening preview, 21 October 2026: `hoi4_production_open`
now uses the exact own named topbar button, with the same opt-in 512×288 profile
and exclusive lease as factory allocation. It opens only from idle manager mode
23 with a positively hidden, settled root. Mode 2 with the original shown root
is a no-op. Original panel identity is durable; uncertain opening and factory
changes block each other and all shared input domains. Same-ID recovery is
read-only, and old factory v1 journal records retain their format.

Focused static evidence exposed a readiness error: production uses manager
mode 2, while the previously assumed 5 is an unrelated constructor update flag.
The predicate and fixture are corrected before live production acceptance.
Both opening and factory clicks now check typed settings/menu/overview blockers.
A nonnull unmapped auxiliary manager object refuses input. No native function
calls or additional game pauses were introduced.

Local validation passed 1,063 Rust tests, 953 Python tests with 37 platform skips,
formatting and warning-free host/Linux Clippy. The first Clippy run caught a
range-style issue; the corrected runs passed. Rust stdio smoke passed in the
suite. The previous published checkpoint passed all three native and five Python
CI jobs; macOS Python 3.9 was cancelled without a runner or test steps. Fresh
exact-commit CI is required for this checkpoint.

No game or VM was launched. These are static/fixture results, not a successful
production gate or campaign. Next: a bounded recorded opening and one-factory
gate with one Luna medium player and exclusive input. The 1080p QEMU performance
limit, two-year acceptance and full campaigns remain unresolved. Scrolling,
line creation and wider economy/warfare adapters remain unfinished.

Linux production action preview, 21 October 2026: the owned controller can now
opt into `hoi4_production_attempt` through the 512×288 production profile.
It changes one requested factory on an exact current own line, or records a
matching-count no-op. The panel must already be open and the row materialized;
panel navigation, new lines, bulk allocation and live acceptance remain open.
The minimum target is one, matching the recovered +/- callback behavior.

The private journal persists original campaign, line/variant identity and count
before input. It retains uncertainty across reopen; same-ID recovery only
observes the original target, never clicks again or takes a new baseline.
Unresolved production input blocks clock, research, Focus, construction and
daily workers. The controller still does not support restarting a supervisor
around a surviving game. Do not confuse journal recovery with that capability.

Before dispatch, fresh canonical state, callback identities and the selected
renderer read transcript must match. X11 rechecks idle keys/modifiers and the
owned window after the final memory callback, before sending buffered input;
review found that the earlier pre-callback check left a modifier-change gap.
Generated Xvfb coverage exercises modifier and window changes during the
callback. Target readback confirms observed state, not event causality or
universal UI routing. Direct native posting remains disabled.

Local checks passed 1,047 Rust tests, formatting, host/Linux warning-free Clippy,
and 953 Python tests with 37 platform skips. The Rust suite includes stdio
protocol smoke. Cross-platform and Xvfb CI must pass on the published commit.
No game or VM was launched for this step. Next: finish the ordinary production
panel-opening route, then a bounded recorded gate with one Luna medium player
and exclusive input. The 1080p host limitation and two-year/full campaigns
remain unresolved; these offline tests are not live acceptance.

Linux production port, 21 October 2026: `hoi4_production_controls` now provides
a read-only same-snapshot join from an exact current own equipment-line handle
and canonical equipment key to its materialized production row and named
one-factory +/- controls. It reuses the existing full collection validator and
batched GNU name lookup. Independently recovered Linux view, row, alias,
callback and selection-dialog layouts are recorded in `native/PROVENANCE.md`.
Optional geometry reuses the existing exact Sprite/Text/Frame diagnostics; no
generic GUI-tree traversal is needed for this narrow route.

The observer rejects foreign owners, duplicate/mismatched full handles and
aliases, alternate callbacks and changing witnesses. Local control gates and
the two equipment-selection dialogs remain explicit; unknown/null dialog roots
do not establish clearance. These observations grant no input. Static callback
analysis shows a minimum allocation of one factory and modifier-dependent step
sizes. At that checkpoint, the Linux action adapter still needed a durable original baseline,
same-scope/point rechecks, ordinary-modifier checks, global journal exclusion
and own-line readback after each click (now implemented in the preview above).
Do not expose the macOS journal or
direct native command queue as a shortcut. New-line creation is separate.

No game or VM was launched for this port step. The existing REA initial
analysis timed out at 30 minutes; its empty analysis snapshot and actual failure
were preserved privately, and the owned provider was closed. Focused static
ELF evidence supplies the mappings, not successful REA decompilation or live
execution. The 1080p Linux-host limitation and two-year/full campaign gates
remain open. The next input gate still uses one Luna at medium reasoning.
Local validation passed all 1,030 Rust tests, formatting and warning-free
Clippy; the Python suite ran 953 tests with 37 platform skips, and stdio smoke
passed. New coverage includes full-collection-before-UI ownership, unrelated
line and UI witness changes, typed row joins, ambiguous aliases, callback
selectors and positive hidden-dialog requirements. Cross-platform CI must be
checked on the published commit; this checkpoint is not live acceptance.

Linux reader follow-up, 21 October 2026: research, politics and construction
now share a batched GNU name-registry lookup. It reads inline names together,
retains bounded external strings and final snapshot witnesses, and checks the
whole requested registry before returning a unique match. The 256-inline-name
fixture uses four logical lookup reads; pointer membership and final validation
are additional. No live latency improvement is claimed yet. The existing
button, child-container and instant-text arrays remain distinct. In particular,
the instant-text array must not be reused as a generic child-widget list.
Local validation passed 1,007 Rust tests, formatting and warning-free Clippy;
953 Python tests ran with 37 platform skips, and stdio protocol smoke passed.
The fault fixtures still reject changed unrelated names at final validation,
now within a batched header block. Construction retains its error category.

Static Linux GUI mapping remains in progress. Icon/Standard name getters and
renderers have independent ELF evidence, but generic child traversal and sprite
geometry still need complete evidence before a Linux GUI-tree input port.
No new game or campaign was launched for this reader follow-up. The prior
1080p QEMU performance limitation and failed campaign acceptance remain open.

Continuation checkpoint, 21 October 2026: the latest recorded native macOS
campaign, m010 (FAIR Germany from 1936), failed in April–June 1940. Germany
became the democratic German Republic under Adenauer, divisions disappeared,
and two `country_capitulated.0` events were observed. The cause is unresolved;
Poland was not conquered. This is neither a successful campaign nor two-year
acceptance. No game was running when this continuation inspected the state.
The current user instructions select one Luna player at medium reasoning for
future gates; the older Haiku entries below describe historical runs.

Recovery review found that the macOS journal could clear an unresolved intent
when a read failed with a lookup/capability error. The journal now retains that
intent across restart, categorizes failed post-input readback as
`action_uncertain`, and refuses to infer nonapplication from a retry's newly
captured baseline. A retry may settle an unresolved intent only when its target
is observed reached. Actions whose verification exists only in the dispatch
call retain uncertainty if that verification is lost. The plan runner makes
one same-ID settlement attempt and returns remaining uncertainty to its caller;
it does not fall through to a different research choice or army-move command.
Missing or nonterminal receipts also stop the runner. No game pause is added.

The first pushed checkpoint (`635f19d`) passed macOS/Windows Rust and all six
Python CI jobs, but Linux Rust failed its delayed-clock restart regression.
That exposed the same unsafe nonapplication inference in the shared clock
journal. Its retry path now also preserves uncertainty until the target is
observed; readback and ownership failures retain their original causes without
becoming retryable refusals. The Linux fixture still asserts exactly one key
delivery and now permits one read-only settlement of the original request.

These changes are offline-tested recovery work, not new live acceptance.
Local validation including the shared clock fix passed: all 996 Rust tests,
formatting and warning-free Clippy;
the Python suite ran 953 tests with 37 platform-dependent skips and no failures.
Durable original baselines are still needed for automatic recovery of additive
actions such as training, construction and front-line creation. Do not clear a
journal, change request IDs or infer failure merely to bypass a blocked intent.
Next: validate the recovery changes, continue Linux adapters and the management
matrix, then resume the recorded gates with exclusive input ownership. Linux
1080p remains blocked by the QEMU TCG performance described below; Windows native
actions and full campaign acceptance remain incomplete. Do not resume the old
Mac campaign objective.

macOS checkpoint, 18 October 2026 (native Steam 1.19.3 x86_64 under Rosetta,
windowed 1920x1080, owned test runs only): the native server reads the
overview, research, technology catalog, economy, equipment lines,
construction queue and Focus completion through the task port in 0.1–8 ms.
Each reader takes its offsets from a per-build layout table; the macOS values
were mapped live (RTTI vtables, sole globals, back-references) and ambiguous
fields were resolved by aligning the same virtual methods of both builds.
Input does not port the Linux X11/SDL stages. Click targets come from the live
GUI tree: widget names from GUI types, rectangles from laid-out sprite
renderers (centre-origin translation plus frame or text-sprite size, zoom-aware
in the focus tree). CGEvent keys, clicks and drags go only to the front-most
owned window, and every action is verified by memory readback.
`hoi4_time`, `hoi4_research_start` (790 ms, slot verified) and
`hoi4_focus_start` (1.17 s including a tree pan, active focus verified) passed
live on the Germany test run, with a game-action journal that records intent
before input and never replays an unresolved dispatch.

Later the same day the macOS server gained every action a 1936–1938 campaign
needs, each verified live on the Germany test run m004 from memory, never from
pixels alone: production add/set, construction add, laws, designers and
advisors (with the game's confirmation dialog), decisions, training,
armies (create, commander, front line, offensive line, execute/stop plan),
event answers, diplomacy (list, Justify War goal with type and states,
Improve relations and other sent actions) and air missions. Supporting reads
list armies, air wings, open events and states by name with owner and
controller. Front lines use a click inside the target state in front-line
mode; offensive lines use a right-button drag oriented by the install's own
province positions; event options and war-goal rows have no laid-out sprite,
so their centres come from a live anchor plus live local positions or a
measured in-dialog offset, and each choice is checked from the dialog's own
text. Declare war and the peace conference are not yet exercised (no war
goal had completed), nor are naval actions. A fresh recorded FAIR Germany
campaign (m005) started on 18 October 2026 with the Haiku 5.5 medium player.

Session checkpoint, 18 October 2026: Run091 on `7f0728c` (package v86) passed
the map placement gate with the Haiku 5.5 player at 512x288. The server opened
the construction panel, selected arms_factory, used the game's own Find view to
centre the camera on Brandenburg (state 64), and queued one arms_factory with a
single map click, verified from the same-snapshot queue. Runs 087–090 found and
fixed three faults on the way: an over-strict broker check of Find receipts; a
location reader that accepted only the constructor status flags 1/2 while a live
state status carried 6; and a torn confirmation read after input-idle that was
not retried. The private broker now allows two read-only queue diagnostics, only
after an uncertain place.

Run092 on `b0b9b35` (package v87) then placed one dockyard in West Prussia
(state 63) the same way. Dockyards are coastal-only state buildings in this game
data, so no province target was needed. The broker pinned the state's Find
anchor to the offline-predicted coastal point (3059, 1599), and the live anchor
matched. Next: a true provincial building (naval base or coastal bunker), which
needs a province target, then production changes and standing economy plans.

Session checkpoint, 17 October 2026: the construction recovery follow-up is
committed as `5e61fec` with nine successful exact-head CI jobs. Run077 on that
commit passed the construction action gate: ensure_open and select_building
dockyard each delivered one server-confirmed ordinary click with canonical
readback, identical replays returned historical receipts, and both no-op IDs
verified with no delivery. Each action discarded one torn `opener` snapshot in
the after-intent phase before input. Canonical lease release, normal exit 0 and
exact absence of four guest and six host instances passed.

The Run077 recording FAILED: the guest clock stepped forward about 5 h 42 m
between first-packet verification and the first menu click, ffmpeg x11grab used
wall-clock timestamps and spent the run encoding duplicate frames, and no frame
after the step was recorded. Next runs must refuse to start the recorder unless
guest/host clock skew is below two seconds. User direction (17 October): the
single exclusive-input player is one Haiku 5.5 agent at medium reasoning (not
Luna), and later runs should use a standard 1920x1080 game window. The current
input profiles are 512x288 only, so a 1080p profile must be built and verified
before such a run. Run077 is not campaign acceptance; economy/warfare automation,
the clean two-year gate and the full campaigns remain pending.

1080p calibration, 17 October 2026: branch `codex/linux-1080p-profile`
(`6f0a77d`) adds a fixture-tested `linux_builtin_absolute_1920x1080_v1` profile,
with nine passing CI jobs and passing actual Linux fixtures. An owned calibration
game at 1920x1080 (`gui_scale=1.0`, display resized to 2560x1440) rendered a true
1080p window but stayed on "Loading GUI": the progress bar moved from 2.8% to
2.9% between minutes 22 and 32. At 512x288 the whole startup takes about six
minutes. No menu input, recorder, binding or player grant happened; the owner
closed its own child once and the display was restored to 1280x720. 1080p live
play is not practical on this QEMU TCG VM. It needs a KVM or native Linux host,
or a native Windows/macOS adapter on real hardware.

Latest: Run076 on `7181858` failed its first `ensure_open` action with
`torn_read: Snapshot witness changed; section=opener; read=171; size=4;
changed_byte=1` in 113.942 ms. Initial controls passed in 71.913 ms. The six
player RPCs followed the declared order, stopped on refusal and canonically
released control. All four action journals are empty; immutable intent-before-
dispatch ordering proves no player input. The ordinal is request-relative;
its semantic field remains unproved. Do not attribute it to Run075's failure.

First preflight passed. Preserve one early `campaign_unavailable` response and
the additional read-only capture while the UI was loading. There was no setup
misclick or pregrant retry. Independent audit passed 62 checks: source 136 inputs,
26 helpers and nine exact-head CI jobs; recording before Start, finalized
233.333-second recording (recorder exit 255), normal exit 0 in 2.296 seconds,
and absence of all four guest and six host instances and both broker sockets.

The follow-up adds bounded whole-producer recovery for torn pre-input snapshots:
at most four reads per explicit initial/candidate/before-intent/after-intent/
dispatch-check phase, one second after the first rejection, 25 ms intervals,
and the original lease/call deadline. Context acceptance and all state, scope,
point, journal and input decisions remain outside recovery. Rejected samples
retain their phase and cause; terminal error categories never become retryable
action evidence. Post-input readback is unchanged. Validate this source before
one bounded recorded action/no-op/replay gate with the single medium player.
This recovery policy does not establish the changing field or live correctness.

Run075 on `583ca49` failed its first `ensure_open` action with
`torn_read: Snapshot witness changed` in 38.892 ms. The preceding controls read
passed in 43.774 ms, including the opener's exact optional-parent join and clear
local gate. Six player RPCs followed the declared order; Luna stopped on the
refusal, made no further game observations, and released control. All four
action journals are empty. The immutable adapter writes intent before dispatch,
so this refusal occurred before input. The exact changing witness was not saved.
No opening, selection, replay or no-op acceptance was established.

The recorded Normal/historical vanilla Germany setup remained paused. One
pregrant controls read also refused a changing snapshot; a single read-only
preflight retry passed before the grant. Preserve both responses. The recorder
finalized without the prior manifest race (316.333 seconds, recorder exit 255);
the game closed normally with exit 0. All owned game/recording processes and
both brokers are closed. Preserve the initial capture-copy basename refusal
and host cleanup identity-format refusal; neither caused an extra game input.
The original 136 source inputs and 26 helpers were verified unchanged after close.

The diagnostic follow-up added producer section, first-read ordinal, size and
changed-byte position to construction torn-read errors, without addresses or
payload. It changes no witness predicate and adds no retry. Do not claim that
diagnostics resolve the underlying instability or recover Run075's missing field.
Run076 above records the resulting annotated refusal unchanged.

Run074 on `7e36594` passed its single combined construction-controls
read in 81.060 ms: exact opener/dockyard callback bindings, canonical ordinary
selection, settled panel and optional geometry returned. Five player RPCs all
succeeded; acquire preceded the one capture and sole observation. Root made no
competing reads/input. Both local dispatch-ready flags remained false because
optional parent pointers were nonnull. No construction input or placement occurred.
Audit verified canonical release, empty action journals, unchanged 127 source
inputs/26 helpers, the 286.333-second recording, normal exit 0 and closure of all
owned processes. The stop helper raced a manifest rewrite; read-only recovery
verified finalization without another signal, then one actual normal close ran.
Preserve the initial close-guard refusal and corrected socket-path audit.

Exact static follow-up binds the optional parent to the named own container's
`e78` position and its traversal byte. The next observer handles only that typed
identity join; unknown objects remain unread. Run075 verified the opener branch;
the choice branch remains unverified. The construction adapter connects separate EnsureOpen and SelectBuilding
receipts to the existing shared controller. It uses a private typed identity
scope from the same witnessed observation, an exact Standard/Sprite candidate
and one ordinary pointer attempt. All input domains block on unresolved
construction delivery; no-ops and successful historical replays send no input.
The profile is independently opted in and workers remain off for its first gate.
Live action acceptance is pending after Run075's refusal. Diagnose the changing
witness before the next action/no-op/replay gate; do not repeat the passed named-binding probe alone.
Map placement remains separate.

Foundation commit `db7e2a3` passed host and actual Linux fixtures, but Windows CI
found three test-only file-access assumptions: reads through a second handle
while the journal held its exclusive lock, and `set_len` on its append-only
handle. The correction releases the journal before inspection and fills the
capacity fixture through the locked writer; production locking stays unchanged.
All nine CI jobs passed the correction `6add9f5`. Preserve the original failed
CI run rather than treating it as a transient failure.

Run073's original global-name uniqueness failure remains recorded; the exact
collection and duplicate name were not retained. Run074 does not recover them.

Run072 on `0822dcd` matched a paused dockyard's own state 56 and stored
UI label Weser-Ems against installed public state/localization files. Luna's UI
reads took 35–37 ms; all 12 RPCs succeeded and no game input occurred. It acquired
the delimiting lease after the observations, so the prescribed gate did not pass
cleanly. Preserve that ordering deviation; no observation replay followed. Root
made no competing game reads/input. Canonical release, a 535.666-second recording,
normal exit 0 and absence of all owned processes/brokers were audited. The setup
Load Game misclick/close and one early campaign_unavailable response remain
recorded. One state-level location/label is evidence; province/railway coverage,
general freshness and construction actions remain unverified. Do not repeat this
observer-only gate merely to erase the deviation. Next: canonical building
selection and verified ordinary controls, then economy plans and warfare.

The next Rust preview adds `ui.selection` under `include_ui`: inactive mode 0,
ordinary mode 1 and railway mode 3 with fresh public template membership, ordinal
and cache agreement. It works independently of queued lines or panel visibility.
Unknown modes remain explicit and avoid template payload. Static/generated
evidence does not establish live selection, placement eligibility or map routing.
The combined `hoi4_construction_controls` preview now observes the nested topbar
opener and one canonical choice, exact callback bindings, scoped local blockers
and optional geometry in one snapshot. It enables no input. Next: one bounded
combined Linux probe with the lease acquired before game observations, then
ordinary opener/choice stages using the existing durable input machinery.

Run071 on `a6a458e` passed a bounded construction-observation/clock gate.
One manually queued dockyard retained its full handle and produced work rose
from raw `0` to `27600000` by paused 3 January 1936 07:00. Luna made 14 RPCs with
no errors; queue reads took 31–39 ms. Independent audit verified three ordinary
clock actions, canonical release, the 582-second recording and normal exit 0.
All owned game/recording instances and both brokers are closed. The refused
inland setup placement, initial icon misidentification and a separate startup
`campaign_unavailable` response remain recorded. UI evidence proves a queue row,
not precise numeric agreement; manual setup is not automated construction input.

Run070 already passed Rhineland completion, searched Four Year Plan assignment
and receipt/budget preservation through restart. Next: prove construction
location identity and ordinary economy controls, expand standing plans, then
warfare and clean two-year/full campaigns. Do not repeat passed Focus-only or
single-line observer gates without a relevant change or unresolved concern.

The construction preview adds `hoi4_construction_queue`: bounded own-status
membership, exact building/railway layouts, full snapshot-scoped handles,
separately named scalar fields and fresh canonical building-template keys.
Independent current-ELF inspection corrects upstream's progress alias: `+0x28`
is cost and `+0x38` is produced work. Default reads omit locations; optional own-territory IDs are described below.
Railway paths, allocation and construction actions are unavailable. Run071 covers one dockyard across advancing
days; railway payloads and exact numeric UI comparison remain unverified. The
immutable build's static `live_validation: pending` flag does not describe a
later audit result. Keep native command posting quarantined.

The next optional `include_location` preview verifies selected state/province IDs
through exact current catalogs, own ownership/control and the embedded status's
reciprocal building/template slot. It defaults off and never enumerates geography.
National overrides, unknown flags, allied/foreign territory and railway paths
stay unavailable. Static/generated evidence does not establish live location
correctness; compare a legible own-location UI before acceptance. Optional
`include_ui` now implies those location guards and reads the panel's named row
text after exact own handle joins within the same witnessed snapshot. Stored
state-name text may be stale and does not prove unique name agreement or input
readiness. Run072 provides the bounded state-level observation above, with its protocol
deviation. General freshness and province/railway coverage remain unverified.

Run047 passed worker-only Focus-to-research assignment and adjacent same-plan
restart/STOP, preserving both completion IDs and budget 2/2/0. Its one BeforeIntent
research torn-read and read-only digest busy retry remain recorded.

Run048 failed the reverse research-to-Focus-to-research gate. Catalog preflight
caught nonexistent `tools1` before any lease/action; the actual plan used canonical
`basic_machine_tools`. Both research assignments completed, but Focus navigation
refused before intent with `focus_precondition`. The exact failed predicate was
not retained. A separate post-stop read-only observation showed settled roots;
it cannot establish the state at the original refusal. The historical target
wait timed out, STOP retained budget 2/3/1, and the 477-second recording, canonical
lease release, normal exit 0 and exact-process absence passed. No action retry,
second wait or restart followed. Preserve this failed run unchanged.

Recorded source `a9f708c` adds bounded observed refusal facts and read-only Focus
navigation readiness before any intent/reservation: at most four observations,
a one-second window after the first precondition refusal, and the original
absolute deadline. Positive preflight remains mandatory. Reader, context and
invalid-producer errors propagate; uncertain actions never retry. Composed
checks passed 695 Rust 1.86 host tests, 802 reported actual Linux checks (800
substantive), five X11 cases, 953 Python tests (37 skips), compiler/format/Clippy/
MSRV/release gates and all nine Windows/Linux/macOS CI jobs.

Run049 passed worker research-to-Focus-to-research assignment and both target-wait
cases on that immutable source. Three assignments completed in 23.668 seconds;
the already-matching wait returned in 15.670 ms RPC. Luna initially stopped because
the Focus summary omitted an earlier preparation. Independent stopped journals
proved that receipt. A separately recorded same-plan restart/immediate STOP
preserved all three completions and budget 3/3/0 with unchanged action, plan and
navigation files. The 561-second recording, canonical release, normal exit 0 and exact
process absence passed. No game or player currently owns input. The original
player stop and the separate supplement remain distinct evidence.

Run050 on immutable source `2307096` passed actual research completion and queued
same-slot refill. Support weapons entered completed membership; basic machine
tools was assigned on 15 June and the target observed on 16 June 1936. The
27m 37s unpaused-to-target interval stayed within its fixed bound. Restart/STOP
preserved both completion receipts and final budget 1/1/0. Independent journals
show three core reservations on distinct days, including a terminal NotQueued
reservation on 14 June with no research-action intent. Its original refusal
reason was overwritten by later completion and remains unknown. Preserve this
limitation alongside the known zero-input navigation refusals, nine read-only
busy responses and one rejected late wait snapshot. The 1,982-second recording,
canonical release, normal exit 0 and exact-process absence passed. No game or
player owns input now.

Run051 on immutable source `b72e0bd` verified the new completed-Focus reader's
empty baseline, then failed Start before possible input. OpenPolitics, OpenFocus
and View were verified; Start retained `torn_read` in the `node_binding` section
(four-byte witness; exact field unknown). One reservation remained Pending with
budget 1/1/0. There was no unpause, restart or actual Focus completion. STOP,
canonical release, the 596-second recording, normal exit 0 and exact-process
absence passed. The private menu harness omitted its dedicated Start artifact
because its coordinate guard did not match the actual successful menu click;
original recording/menu timestamps remain available. Preserve this limitation.

Run052 on immutable source `64f3634` passed actual Rhineland completion. The
worker assigned it once; a Focus-change wait woke on 4 February 01:00, and a
fresh stopped/paused membership read at 19:00 contained only Rhineland, with no
active Focus. Final adjacent same-plan restart/STOP preserved the original
receipt and historical last-attempt-day budget 1/1/0 without duplicate input.
Fresh advancing-day digests reported 0/1/1. Background busy/torn diagnostics
remain preserved; no player RPC returned an error. The 702.666-second recording,
dedicated campaign-Start artifact, canonical release, normal exit 0 and exact
process absence passed. Both brokers are closed; no player owns input.

Run053 on immutable `3ade3b0` failed the successive-Focus gate while Rhineland
was still active. The independent journals retain five read-only refusals on
2–6 January: three name the game-clock witness and two have a generic snapshot
witness message. Four Year Plan stayed Pending with zero attempts and no input.
The player saw the latest refusal after one advancing wait and stopped; it did
not retry an action. Canonical release, the 622-second recording, normal exit0
and exact-process absence passed. Both brokers are closed. The next change
retries wholly discarded torn snapshots only during read-only Focus readiness,
within the existing four-read/one-second bound; run054 exercised this recovery.

Run054 on immutable `87f761c` verified recovered torn readiness reads across
advancing days, but failed the successive-Focus gate. Four Year Plan OpenFocus
passed; View refused before input on three days with `research_geometry_unavailable`.
The last accepted wait was 5 February 20:00, with no active Focus. Final completed
membership was not read under the terminal stop contract, so this run alone does
not prove actual Rhineland completion. The core retains one assignment receipt,
Four Year Plan Pending with zero attempts, and no uncertainty. Four read-only
digest RPCs returned busy. Canonical release, the 718.333-second recording, normal
exit0 and exact-process absence passed; both brokers are closed. The original
160-byte reason lost the hierarchy boundary numbers. New compact diagnostics
retain every policy quantity within that bound without changing traversal limits.


Run055 on `6d4daa4` passed paused geometry for both Focus nodes, closing through
10 ancestors to the owned graphics root. It did not reproduce the later failure.
Run056 on the same immutable runtime then retained the actual advancing failure:
Four Year Plan View refused before intent with count 3,806, allocated capacity
12,138, capacity limit 8,192, prior aggregate 11 and aggregate limit 16,384. A prior
View refusal retained a clock-witness torn read. STOP had no uncertainty and the
second entry had zero assignment attempts. Under its pre-granted diagnostic
extension, one ordinary pause preceded separately timestamped membership and
geometry reads: Rhineland was completed on 6 February 22:00, no Focus was active,
and the same geometry boundary was present. Canonical release, 652.666-second
recording, normal exit 0 and exact-process absence passed. Both brokers are closed.

Source `c67aa66` permits capacity 16,384 only for the existing opt-in exact Focus
position ancestor class. Aggregate sibling-pointer reads remain capped at 16,384;
owned-root and ordinary/container capacity bounds stay 8,192 and 2,048. Types,
reciprocal membership, final witnesses and input guards remain mandatory. The
read-only production port adds optional base equipment keys; generated tests
cover it, but live production comparison and allocation remain pending.

Run057 on that immutable source had two distinct segments. The initial player
stopped early after two read-only digest busy responses, while Rhineland was
still active. An independently audited clean STOP allowed a separately granted
same-game continuation; preserve the original incomplete report. The supplement
confirmed Rhineland completed on 4 February 24:00, but Four Year Plan View refused
BeforeIntent with `quantized candidate outside owned client`. No second assignment
attempt or uncertain input occurred. Its permitted post-stop geometry observation
closed through ten ancestors with no policy boundary; the node was offscreen to
the left. The capacity correction therefore passed that later observation, but
successive-Focus assignment still failed. Add ordinary verified navigation for
offscreen nodes; do not clamp coordinates or treat this as a completed gate.

All 21 production lines returned base equipment keys in 67.818 ms RPC / 53.575 ms
producer time, with no unavailable IDs. This is one read sample, not UI comparison
or allocation evidence. Independent audit preserved seven actual RPC errors
across both segments, one original assignment receipt, five verified Focus input
pairs and no research actions. Canonical release, the 884.333-second recording,
normal exit 0, exact-process absence and unchanged 103 native inputs/24 helpers
passed. Both brokers are closed; no game or player owns input.

The read-only `hoi4_focus_search_state` preview joins search matches/highlights to
the complete current own-item set and canonical keys. It has static and generated
evidence, with 744 host and 852 reported actual Linux checks passing; live search
validation remains pending. It does not type queries or authorize input. Signed
offscreen diagnostics now retain projected coordinates without clamping them.

The next optional `include_control` observation adds bounded query text, exact
named search-field/listener joins and reciprocal renderer diagnostics. Default
search reads omit that payload. Keyboard focus, typing and Enter routing remain
unverified. Run058 on immutable `1531b4b` verified OpenPolitics and OpenFocus,
then its first search-state read refused with `invalid_focus_type`. The original
generic error does not identify the failing object; no query text or geometry
was returned. Luna stopped without a retry, comparison or final overview, as
required by its pre-granted terminal contract. The two input pairs, one RPC
error, canonical release, 418.666-second recording, normal exit 0 and exact
process absence passed independent audit. All 106 native inputs and 24 helpers
remained unchanged. Both brokers are closed; no game or player owns input.

Run059 on immutable `5ef90cb` confirmed the failing object: `query_container`,
expected table `0x4347d58`, observed `0x4349f00` (CEditBoxType). Both opening
stages passed; the first search read refused in 54.044 ms. No retry, comparison,
final overview or assignment followed. Independent audit verified the two input
pairs, one RPC error, canonical release, 422-second recording, normal exit 0,
exact-process absence and unchanged 106 native inputs/24 helpers. Both brokers
are closed. This pinpoints the same class of mismatch but does not retroactively
identify Run058's original unlabeled refusal.

The corrected observer follows the owned Focus root's named `find_view` window,
then its typed edit-box collection for `to_find_text`. The prior `edit+0x60`
pointer is a definition, never a parent window, and is no longer read. Complete
paired names, exact types/registries, alias/listener joins and final witnesses
remain mandatory. Run060 on immutable `c1e0d9c` passed that paused live probe:
both opening stages and both search reads succeeded, with 14 player RPCs and
zero errors. The optional query read took 75.559 ms RPC / 64.288 ms producer and
verified the named field/listener with an empty query. The alternate renderer
was unavailable, and the search cache remained unmaterialized (zero rows for
262 current items). No typing, selection or assignment was attempted. Final
overview stayed at the paused baseline. Canonical release, the 356.666-second
recording, normal exit 0, exact-process absence and unchanged 106 native inputs/
24 helpers passed audit; both brokers are closed.

Run061 on immutable `e35fefe` passed the paused renderer/receiver observation:
15 player RPCs, zero errors/retries, both opening receipts and unchanged final
baseline. The exact InstantText renderer closed through five owned ancestors
with consistent conditional unit caches. The named control was enabled but
neither keyboard nor text receiver was focused. Search cache remained empty.
The detailed read took 75.460 ms RPC / 63.990 ms producer. Independent audit
verified canonical release, the 293.333-second recording, normal exit 0,
exact-process absence and unchanged 107 native inputs/24 helpers. Both brokers
are closed; no game or player owns input.

Run062 on immutable `0419644` passed the empty `search_field` activation and exact
same-ID historical replay: 18 RPCs, zero errors/retries, three verified input
pairs, both receiver identities true and unchanged paused baseline. Activation
took 1.764 seconds; replay took 5.702 ms without input. Canonical release,
307.333-second recording, normal exit 0, exact-process absence and unchanged
107 inputs/24 helpers passed audit. Both brokers are closed. Native macOS CI
first hit a 50 ms fixture deadline before dispatch; one unchanged-source rerun
passed. The original failure is retained, and the next source replaces that
timing race with an explicit failing read after the rejected post-state.

The next `search_query` preview types explicit bounded lowercase ASCII words
into the focused empty own control. It pins the same bindings, checks XKB state,
persists the exact query in the target and requires exact query text plus the
requested canonical selection afterward. Unknown delivery remains quarantined;
only identical successful requests replay historically. Typing, Enter, successive
Focus choices and campaign acceptance remain unverified.

Run063 on immutable `23117fd` passed OpenPolitics, OpenFocus and SearchField,
then SearchQuery refused before possible input: a four-byte `search_control`
witness changed. The exact field remains unknown. Its persisted intent ended
NotQueued; no mutation retry, replay, post-query read, View or final overview
followed. Independent audit retained 14 RPCs and one error, canonical release,
274.666-second recording, normal exit 0, exact-process absence and unchanged
109 inputs/24 helpers. The recorder helper retained an outdated purpose string
and expected content hash; both defects are recorded separately, with installed
script bytes independently matched to its frozen literal before campaign Start.
Both brokers are closed. The next source adds field labels and bounded recovery
of wholly discarded read-only snapshots before the single SearchQuery dispatch.
It does not retry input or reopen a terminal request. Live query validation is pending.

Run064 startup on `82ec261` was aborted before campaign Start or player grant:
Windows CI found that the new portable read-recovery tests referenced the
Unix-only navigation formatter. No gameplay input or player RPC occurred.
One ordinary window close exited normally with code 0; the source/helpers and
exact-process absence were audited. The correction keeps bounded diagnostic
formatting inside the portable observer and adds Windows cross-compilation to
the local checks. Run064 is an aborted setup, not a live query result.

Run065 on immutable `430bb05` passed the three opening/field stages, then its
only SearchQuery returned `action_uncertain` after the ten-second readback
deadline. One wholly discarded pre-input read identified `search_edit+0x74`
(four bytes); the retry recovered before possible input. The post-input states
and transport acknowledgement were not retained in the failure response, so
the actual query/selection remain unverified. Its durable Intent remains
unresolved. No action retry, historical replay, later observation or View followed.
Independent audit verified 14 RPCs, one error, canonical release, a 280-second
recording, normal exit 0, exact-process absence and unchanged 110 inputs/24 helpers.
All nine CI jobs passed. Both brokers are closed. The next source preserves
bounded already-read post-input facts and transport classification on failure;
it changes neither input, deadlines nor acceptance predicates.

Run066 on immutable `cbd60c5` passed the three opening/field stages, then the
sole SearchQuery reached `action_uncertain` after 10,007.615 ms. New diagnostics
retained server-confirmed text transport and 87 readback reads (75 accepted,
12 rejected). The last accepted snapshot contained exact `four year plan` text,
both focused receivers and six matches, including the requested Focus exactly
once. The selected key was `GER_prioritize_the_four_year_plan`, not
`GER_the_four_year_plan`. The strict selection predicate correctly withheld
success. No pre-input read was discarded. The original Run065 post-state remains
unknown; these observations belong only to Run066.

Luna stopped without replay, a later search read, View or final overview.
Independent audit verified 14 RPCs, one error, three verified input pairs and
one unresolved query Intent; no worker, clock or research actions occurred.
Canonical release, the 316.666-second recording, normal exit 0 in 2.360 seconds,
exact-process absence and unchanged 111 inputs/24 helpers passed. All nine CI
jobs passed. Both brokers are closed.

The next source adds separate `search_text` and `search_next` stages. Text entry
verifies the exact query and complete bounded canonical matches without claiming
the requested selection. One ordinary Tab is permitted only for the immediate
next requested match; unchanged order, exact selection, receivers and active
Focus must verify afterward. The original `search_query` semantics and Run066
uncertainty remain unchanged. Static analysis and generated checks are not live
Tab or successive-Focus evidence.

Run067 on immutable `1fa1268` verified SearchText, its identical historical replay,
and two ordinary Tab steps through the six canonical matches. Text entry took
1,417.285 ms; the two steps took 875.374 and 1,161.722 ms. The third step refused
BeforeIntent in 178.671 ms with `torn_read` at `search_edit+0x74` (four bytes).
The original initial/candidate/final phase was not retained. Its request has no
journal row and no input was sent; all six earlier input pairs were verified,
with no pending Intent. The last verified selection was Ascension of Göring.
No retry, final Tab replay, later search read, View or final overview followed.

Independent audit retained 18 RPCs and one error, canonical release/status,
327.666-second recording beginning before campaign Start, normal exit 0 in
2.406 seconds, and absence of all four guest and four host processes. All 113
native inputs and 24 helpers stayed unchanged; all nine CI jobs passed. Both
brokers are closed. This verifies two Tab steps, not the complete search-to-View
sequence, successive Focus assignment or campaign acceptance.

The next source extends recovery to each initial, candidate and final pre-Intent
search observation. Each phase permits at most four whole reads, a one-second
window after its first torn read and 25 ms intervals, all under the original
absolute deadline. The final pre-queue check keeps its separate existing bound.
Only producer `torn_read` snapshots are discarded; ownership acceptance, scope
pinning, predicates, journal execution and input are never retried. Rejected
reads retain their phase and grant no input authority. Live validation is pending.

Run068 on immutable `67bd4c4` passed the complete paused search-to-View gate:
24 player RPCs, zero errors/retries, eight verified input pairs and no pending
Intent. SearchText took 1,419.205 ms; three ordinary Tab steps took 1,012.609,
1,090.946 and 1,015.285 ms, selecting Four Year Plan in the unchanged six-key
list. Exact SearchText and final Tab replays returned identical historical
receipts in 5.558 and 6.490 ms without input. Fresh full-coverage selection,
Rhineland comparison, node detail and ordinary View passed; View took 2,373.680 ms.
The final overview retained the paused January 1936 baseline and idle Focus.

One torn final pre-input snapshot recovered before SearchText; its post-input
reads retained six rejections among ten reads. Tab readbacks retained one, zero
and one rejected snapshots respectively. No pre-intent snapshot was rejected,
so the newly added pre-intent recovery phases were not exercised in this run.
Independent audit verified canonical release, the 283.666-second recording,
normal exit 0 in 2.259 seconds, absence of all guest/host instances and unchanged
113 native inputs/24 helpers. All nine CI jobs passed; both brokers are closed.
This completes manual search navigation, not successive worker assignment or
campaign acceptance.

The next worker preview accepts an explicit optional `focus_search` entry hint.
It retains the assignment key while selecting each immediate canonical search
match; every stage and binding is durable, and reopening validates the complete
ordered chain. Empty-field entry occurs once. A changed query/order/selection,
failed or unresolved search stage cannot start another chain. The query is never
inferred from the Focus ID. Existing omitted hints and stored hashes retain their
meaning. Run070 subsequently passed worker live validation; replacing an existing
query remains unsupported.

## Immediate acceptance work

Run069 on `3659c24` remains failed. The initial Luna segment dropped a running
terminal session ID despite a complete normal broker wait response; preserve its
early STOP as incomplete. A separately granted continuation retained the original
wall bound and corrected terminal-session polling. Its Four Year Plan OpenFocus
was NotQueued on day 706674, succeeded on 706675, then search preparation refused
with an aggregate command predicate on days 706675 and 706676. The final wait
showed idle Focus on 5 February 23:00; no later completed-membership read was
authorized. Audit retained 44 RPCs/zero MCP errors, five verified Focus pairs,
one NotQueued pair, zero unresolved intents, the unchanged first assignment,
canonical release, 696-second recording and normal exit 0. Both brokers and all
owned processes are closed; 114 inputs/24 helpers stayed unchanged.

The correction in `eadf3a2` follows the exact Linux getter's null country override:
resolve through the already-witnessed positive own player, never observer or
foreign state. All original own-country/item/control witnesses and input
predicates remain required. New compact predicate diagnostics preserve future
refusals. Static evidence does not identify Run069's original failed predicate;
Run070 passed the corrected worker route. Its receipts do not identify which
override branch was exercised. Future Luna calls must
retain `exec_command.session_id` and poll that same process with `write_stdin`;
an ordinary yield is neither a missing MCP response nor permission for a new call.

1. The manual Run068 and worker Run070 search gates passed. Move to ordinary
   economy/construction/production controls and wider standing plans. Validate
   exact own target identity, eligibility and state changes against the installed
   build before admitting actions. Keep one Luna medium player, fixed
   bounds, recorded campaign Start and independent stopped-journal audit.
   Any post-failure diagnostic exception must be fixed before the input grant.
   Earlier missing evidence remains unknown; later success does not repair it.
2. Preserve durable assignment receipts and budgets through advancing days and
   queued heads. Prove finished technology membership/empty slot before refill;
   an active Focus or assigned slot alone does not prove completion. Use fresh
   own-state evidence and explicit bounded waits.
3. `details.focus` returns the latest-per-kind Focus summary, including only the
   latest preparation. It is not a full navigation history. Do not require an
   earlier receipt in that summary; independently audit stopped durable journals
   for full stage order. Compacted sections remain exact snapshot lookups.
4. On permanent or uncertain failure, stop the gate. Only STOP, release/status
   and canonical proof transfer follow; no blind repairs or new action IDs.
   After explicit player handback, independently verify release, finalize the
   recording and perform one ordinary owned close. Preserve actual failures.
   Core NotQueued history now retains new original reasons after completion and
   restart. Older absent causes remain unknown; run050 exposed that evidence gap.
5. Finish economy, army, air and naval-invasion adapters, recovery and long-running
   standing plans. Then pass the clean two-year and full recorded campaign gates.
   Paused assignment completion is not completed research or a finished Focus.

## Full objective and work order

- Reuse Clausewitz-MCP's game semantics, structured reads, command vocabulary
  and automation. Upstream is pinned at
  `97c85437081904d5ad07bae1623ec58d1c0a5b9e`; unchanged source, MIT notices and
  per-file provenance are retained under `third_party/clausewitz_mcp`.
- Keep the primary live path independent of saves and repeated screenshots.
  Use compact fresh state, event-driven waits, durable plans and verified normal
  player actions. Optional recordings, diagnostics and denied-access fallbacks
  must not determine routine agent latency. Measure latency and success rates;
  Rust alone does not prove speed or correctness.
- Validate Linux first, then Windows and macOS. Recover each supported build's
  ABI/layout independently. The native Windows adapter remains unavailable;
  the separate macOS preview is recorded in the preserved Mac history. Portable
  CI does not establish live compatibility or campaign acceptance.
- Run one Luna medium player with exclusive input. First pass the clean two-year
  gate, then the recorded FAIR Germany and Nationalist China campaigns and META
  Poland showcase. Preserve the original campaign specifications in the
  [Mac handoff](HANDOFF_MAC.md) and [campaign playbook](../skills/hoi4-campaign-playbook/SKILL.md).
- FAIR is the default: Normal difficulty, historical AI, vanilla rules and
  observational live-play helpers only. No console/debug/effects, hidden enemy
  intelligence, save editing or reload-to-undo. Strong ordinary strategy,
  encirclements and supported invasions are required. Mod QA stays inaccessible
  to campaign automation.
- Prepare an audited public repository with readable installation/status docs.
  Keep Clausewitz acknowledgements at the end of README. Release acceptance,
  version/changelog, archives and downstream pins must agree; no release or
  Windows downstream pin moves before acceptance.

## Ownership, persistence and evidence

Work autonomously and preserve unrelated Python WIP and `wip/governor-memory`.
The active broad goal remains incomplete. State concrete external blockers;
do not substitute fixture success or advertised capabilities for live evidence.

Pin exact PID creation, executable and user ownership, build identity and game
parentage. Never adopt or kill an unrelated process. Keep process access under
ordinary permissions; do not disable SIP, relax kernel guards, re-sign the game
or run the whole agent as root. Keep stdout protocol-only and unsupported
capabilities explicit. Bounded torn/stale reads must refuse.

Persist intent before input, retain uncertainty across restart, and preserve
unfinished entries and consumed budgets on idempotent retries. Observations
never pause the game. Only independently verified normal player commands may
enter campaign dispatch; the failed native posting path stays disabled.

Keep game assets, credentials, private paths, saves, recordings, journals and
logs outside Git. Audit the full reachable history of every pushed branch/tag;
never push `private/*`, `--all`, `--mirror` or arbitrary tags. Follow
[AGENTS.md](../AGENTS.md) for tests, commit trailers and release boundaries.

Detailed evidence is in [Linux validation](CLAUSEWITZ_VALIDATION.md),
[the measured comparison](CLAUSEWITZ_COMPARISON.md) and
[the expansion contract](CLAUSEWITZ_FOUNDATION.md). The prior 577-line continuation
brief is preserved in [historical checkpoints](LINUX_RUST_CHECKPOINT_HISTORY.md),
including superseded instructions and original failures. Run046's lost
post-input cause and run040's interrupted cleanup remain unresolved; later
success does not repair their evidence.
