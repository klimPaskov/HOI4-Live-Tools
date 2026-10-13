# Clausewitz-MCP Linux validation

Updated 13 October 2026. This records preflight and partial licensed-game evidence for
the [Linux-first migration](LINUX_RUST_HANDOFF.md). One **gpt-6-luna player at
medium reasoning** used the guarded upstream MCP in an actual Linux Germany
campaign. Focus, four research assignments, unpause and one day of advancement
were verified. The final pause command timed out during posting; the adapter
is quarantined. A subsequent fresh Rust run passed Luna's ordinary clock test
through 3 January 1936, including verified pause. These are **bounded live tests,
not campaign acceptance or a successful campaign**. The Rust port is underway.

## Reference and test environment

The reference is [smerss/Clausewitz-MCP at
97c8543](https://github.com/smerss/Clausewitz-MCP/tree/97c85437081904d5ad07bae1623ec58d1c0a5b9e).
Its offset table targets Linux HOI4 **1.19.2**, ELF build ID
`2a79e51358b8b71c09cfb5ddefa0c03649067a4b`. These are upstream's declared
identifiers; they have not been matched to an installed Linux game here.

The owned full-system guest runs Ubuntu 24.04.5 LTS, x86_64 kernel
6.8.0-142, Python 3.12.3, GCC 13.3.0 and NumPy 1.26.4. QEMU 11.1.2 uses TCG CPU
emulation on the Mac host. This provides the actual Linux x86_64 register ABI
missing from the earlier Rosetta container test. It does not establish usable
HOI4 frame rate or campaign throughput. The generated-process preflight used
children with `ptrace_scope=1`. The subsequent owned-game companion test
temporarily used `ptrace_scope=0` inside the isolated guest, with an exact-process
restoration monitor. The original setting was restored after that read test;
the later software-only validation adapter used a separate timed guest-local
lease with a restoration monitor and independent watchdog. The intended public
command runtime must own its launched game directly. No host
process-access setting changed; the MCP and game run as an ordinary user.

Anonymous official SteamCMD metadata for app `394360` lists Linux depot `394363`
on public build `25205862` and the published `1.19.1.0` rollback. The response
contains no `1.19.2` branch or corresponding advertised manifest. This does not
prove an older licensed depot can never be obtained; it means the tested public
metadata route did not supply the matching build. The user subsequently
authenticated SteamCMD and installed the licensed public Linux build. Credentials
and authentication output were not read or retained. Its independently inspected
ELF build ID is `a4971c9e8ac9a6f2487dea109e01f9839ee59db7`; SHA-256 is
`56fc2945cdbf2475a316db3285eb3458ae5d577db149db6dfdba6f004de3a88a`.
Upstream's older offsets were not treated as compatible.

## Licensed-game checkpoint

The game launched windowed without mods after two startup problems were resolved:
its default data-directory parent did not exist, then its generated borderless
and multisample settings failed to create a software-rendered window. Creating
the ordinary data-directory parents and using windowed mode without multisampling
resolved those failures. No game binary, save or campaign rule was modified.

A private guarded adapter uses upstream's process backend and MCP protocol,
with current-build globals, fields and command layouts recovered independently
from the installed ELF. It binds the recorded PID, creation time, executable,
owner and parent supervisor. FAIR dispatch excludes console, raw calls, foreign
country details and unverified subsystems. Observations use bounded reads and
never attach to the game. Commands remain gated on confirmed Normal historical
single-player setup and actual own-country state.

At **17:49 UTC**, upstream initialize, ping, tool discovery, refusal of a debug
console request and 20 observations passed against the owned game. Local Unix
socket round trips measured **8.9 ms median**, **23.6 ms maximum**; these timings
exclude SSH and model overhead. The game's tracer was zero after the reads.
Game-state, idler and technology-database roots were still null during GUI
loading. This proves real process access and protocol dispatch, not campaign
observations, ordinary command execution or Luna acceptance.

Initial startup used software rendering. An independent hidden OpenGL raster
fixture measured roughly **1.1 frames/s** at 1280×720 and **4.5 frames/s** at
640×360 with llvmpipe; these were fixture measurements, not HOI4 frame rate.
Some pre-campaign processes failed to exit within a graceful shutdown deadline.
They and their settings were preserved, without force-killing the game.

The subsequent validation environment uses an isolated **8 GiB / four-vCPU**
x86 guest and a private virgl transport to the host's Metal-backed renderer.
The original guest is paused with its unfinished processes preserved. The new
512×288 windowed run reached a real first-run dialog over the main-menu
background. A persistent stdio MCP session over pinned localhost SSH measured
**10.2 ms median**, **15.7 ms maximum** across 20 actual process observations,
with no stderr output or game suspension. This remains menu/process evidence;
it does not establish simulation throughput or a successful player action.

Read-only type evidence identified `CCurrentGameState` as a direct nonvirtual
`CGameState` subclass at offset zero. The reviewed adapter accepts only its
exact current-build primary vtable, and distinguishes frontend from in-game
idlers. Frontend observations do not bind the campaign adapter; commands still
require the verified in-game type and setup evidence. A separate fake-backend
suite passed **24 checks** for these changes and the previous ownership,
quarantine, research and epoch guards. Those checks are offline evidence.

The frontend run exited after an owned termination request. Both recording
segments passed ffprobe, and settings/playset backups matched the restored
files. The subsequent fresh run entered a real 1936 single-player Germany
campaign through the normal UI. Own-country reads confirmed Normal difficulty
(value 2), historical AI (value 1), no mods and the correct player. Vanilla
default rules were left untouched in the fresh user directory. The final
settings screen was not independently captured; do not describe those values
as a visual settings check. Private recordings and diagnostics remain outside Git.

## Actual Luna play and native-command failure

The current build's serializer subtracts 43,800,000 hours (5000 × 8760) and
displays hours as 1–24. Independently recovered serializer/parser instructions
and the normal 1936 bookmark resolved the older upstream date mismatch.
The starting raw value `60759371` corresponds to `1936.01.01.12`. No game data
was edited to correct the software decoder.

A first ordinary speed command changed live speed from 0 to 4 while paused.
Readback verified the change; all target threads had tracer zero afterward.
Luna then received exclusive input ownership and performed these actual actions:

- Selected `GER_remilitarize_the_rhineland`; own focus readback matched.
- Started `basic_machine_tools`, `construction1`,
  `electronic_mechanical_engineering` and `support_weapons` in four slots; each
  assignment and a subsequent research snapshot matched.
- The game rejected `dispersed_industry` as ineligible before posting. Luna
  selected the different returned ID `support_weapons`; it did not force or
  repeat the rejected technology.
- Unpaused normally at speed 4. A bounded server wait reached
  `1936.01.02.01` after 10.523 seconds, with focus and research still active.
- The final ordinary pause request timed out in `CCommandQueue::Post` after
  roughly six seconds including cleanup. No native retry was attempted.

Successful focus/research actions took approximately 1.7–2.7 seconds through
the persistent stdio/SSH path; unpause took 1.15 seconds. The campaign remained
running afterward. Kernel inspection found every game thread detached, and
pure reads observed further time advancement. The adapter retained its
quarantine: restored CPU context and continuing simulation do not prove that
an interrupted engine call left no locks or other side effects.

After the matched Rust benchmarks, the software adapter closed only after its
shutdown guard verified no retained thread or injected context. The guest-local
process-access setting returned to its original value 1; Rust then correctly
returned `permission_denied` instead of cached state. The whole owned VM was
paused to preserve the failed game and recorder without killing either. The
simulation had reached April during subsequent reader tests; that unattended
progress is not Luna campaign acceptance. Configuration backups remained pending
the exact game's exit at that checkpoint.

Static follow-up shows that Post performs virtual checks, allocation and logging
before virtual backend submission. Blocking mutex calls exist in the logging
subsystem; the specific cause of this timeout is not proven. Arbitrary-thread
posting while all other threads are stopped requires further scheduling and
recovery proof. Increasing the timeout or blindly resuming workers is not a
validated fix. Rust command posting remains unavailable pending that proof.

## Owned Rust launch and incident preservation

The failed native-post run was preserved in a private full-VM incident snapshot
before shutdown. The snapshot job completed, and an offline archive check reported
zero check errors with one leaked cluster; the archive was kept unchanged. It is
diagnostic evidence, not a campaign checkpoint or a verified restore. External
graphics helpers are outside that VM snapshot.

After preservation, an exact owned OS termination request let the failed game
exit with code 0. The recorder exceeded its initial shutdown deadline, then exited
with code 123. The old supervisor exited and both settings/playset backup hashes
matched their restored files. Six recording segments have probeable durations;
the final segment lacks a duration and is not claimed as valid footage. No native
command was retried, and the old adapter's quarantine remains intact.

A fresh vanilla game now runs as the direct child of the public Rust preview's
ordinary-user supervisor. Actual executable/build/header validation and bounded
parent reads passed with the guest's original `ptrace_scope=1`, without an access
lease. Two independent stdio `--connect` clients each made 20 capability requests
and disconnected; the owner and game identities stayed unchanged. Guest-local
capability medians were **3.6 ms** and **3.0 ms**. These are frontend capability
checks, not campaign snapshots or command latency. The overview correctly refused
the frontend as `campaign_unavailable`; every inspected game thread had tracer 0.

The fresh run subsequently entered Germany through the normal single-player
menus. A live Rust-owned snapshot confirmed **1936.01.01.12**, player `GER`,
Normal difficulty (2), historical AI, paused initial speed 0 and four empty
research slots. Its actual bounded reads took **13.6 ms** (85 calls, 945 bytes)
under unchanged Yama 1. This is one initial campaign observation, not a latency
distribution or Luna play. A private recording started before the normal Start
click; a video packet was probeable. Final segment integrity remains unverified
until recording closes.
This game used the launcher before the latest exec-witness and worker hardening.
Those changes have separate generated-process evidence and need another fresh
owned-game check before command acceptance; the running owner was not replaced.

The new launcher uses fresh isolated userdata, private peer-authenticated sockets,
bounded client workers, pidfd-owned graceful shutdown and a single child reaper.
Generated-process tests cover reconnects, disconnected/malformed clients, nonce
and identity refusal, replacement-socket preservation and hung shutdown. The
reader refreshes bounded mappings and captures a private boot/`AT_RANDOM` witness
to refuse same-file exec across snapshot boundaries. These safeguards do not make
running snapshots atomic or establish ordinary command safety. Setup and remaining
limitations are in [the launcher preview](OWNED_LINUX_LAUNCHER.md).

## First Rust observation port

`native/` now ports the bounded typed reader, own-country tag mapping, focus,
research, current-build calendar and Linux process interface. Its stdio tools
are `hoi4_capabilities`, `hoi4_overview` and `hoi4_wait_for`. Observations do not
attach, suspend, write, capture screenshots or parse saves. Unsupported builds
and unavailable subsystems return explicit errors. Windows/macOS native process
adapters, strategic commands and the daily governor are still unimplemented.

A statically linked Linux x86_64 binary built on the Mac read the actual running
campaign. Twenty guest-local stdio snapshots measured **4.8 ms median**,
**16.1 ms maximum**, with empty stderr and tracer zero. These samples exclude
SSH and model overhead. The first probe explicitly refused an incorrect ELF
load-base assumption; the corrected implementation validates the file-offset-zero
header mapping, executable inode, build ID and full SHA-256 before binding.

A matched comparison used **20 overview samples per runtime** over the same
persistent host stdio/localhost-SSH boundary in the running campaign. The guarded
Python adapter measured **294.9 ms median / 390.1 ms maximum**; the Rust reader
measured **7.5 ms median / 13.3 ms maximum**, roughly **40× faster** for these
snapshots. The guarded Python path includes its validation overhead; this is not
a benchmark of unmodified upstream or a promise about command/strategic latency.
The Rust snapshots check identity at transaction boundaries rather than running
process checks for every primitive read. Neither path paused the game.

Portable Rust tests cover protocol isolation, denied raw commands, malformed
arrays, changed clocks, process replacement and the player tag-index mapping.
Linux-specific parser tests additionally run in the actual guest. The legacy
Python suite still passes **952 tests, 37 skipped**. Offline checks are not live
acceptance. See [Rust setup and limitations](../native/README.md).

## Measured preflight

| Check | Evidence and limit |
| --- | --- |
| Linux process access | Generated child: `process_vm_readv`, read-only `/proc/PID/mem`, `PTRACE_SEIZE`, `INTERRUPT`, `GETREGS`, `GETREGSET`, identical `SETREGS`, and `DETACH` passed. |
| Upstream typed process layer | Generated child: backend write delegation, eight-byte write/readback and register roundtrip passed after the write fix below. |
| Upstream injected call | Generated C function: ten calls with eight arguments returned the expected values and restored registers exactly. The child exited normally. |
| Scratch mapping prerequisite | Initial call failed with `EFAULT` because upstream places scratch roughly 1 MiB below the current stack pointer without allocating it. A second fixture first touched and released a deeper stack frame. Passing calls prove only this mapped-stack case; the actual game mapping still needs inspection. |
| Timeout recovery | Original timeout path failed with `ESRCH` during `SETREGS`; registers were not restored. The fixture required explicit stop/restore rescue. The local fix passed the same timeout case and normal-call regression. |
| Stdio protocol | Initialize, ping, tools/list and an unknown-tool request passed. All stdout lines were JSON; stderr was empty. Registry contained 107 tools, including five debug tools. No tool accessed a game. |

In the final generated-function regression, individual calls took about
**2.1–3.9 ms**. The separate 100-sample register-read fixture measured median
**0.049 ms** and 95th percentile **0.092 ms**. These are synthetic process
measurements under CPU emulation, not game-action or strategic-turn latency.
Private scripts and full reports remain outside Git.

## Disclosed local fixes

The original reference commit remains preserved. A separate scratch validation
branch contains two commits; it has not been pushed to upstream:

- `5a30a3e`: remove the duplicate `Proc.write` method that overrides backend
  delegation and refers to undefined `ctypes`, `iovec` and `libc` names.
- `03ad232`: use monotonic time and interrupt/wait for a ptrace stop before
  restoring registers on call timeout. Bound that recovery wait to two seconds.

The timeout test covers a recoverable stopped child. It does not prove recovery
from target exit, a failed interrupt, arbitrary game-function side effects or
every injector exception. Those remain separate acceptance cases. The vendored
upstream source in this repository is still unchanged, with its MIT license and
file hashes retained. Any live run using the validation branch must record both
the original pin and these local patches.

## Changes required before FAIR player access

The upstream architecture supplies structured game objects, ordinary command
posting, turn waits and province-graph automation. Its advertised play/debug
split is not by itself our campaign policy boundary. Source inspection found:

- `hoi4_pause_in_hours` invokes the console despite lacking a debug prefix.
  Campaign dispatch must exclude this path and all console/effect/raw-call tools.
- `hoi4_send_command` accepts raw field offsets, explicit pointers and a `force`
  option that bypasses its unverified-layout check. This option is distinct from
  the lower-level command queue's force argument. Neither bypass belongs in FAIR
  dispatch; only validated ordinary commands with typed, owned IDs may enter it.
- Many tools accept an arbitrary country tag. Detailed foreign economy and unit
  reads need visibility checks; defaulting to the player does not enforce them.
- Combat and rage-micro code reads exact enemy organisation, reserves and
  out-of-supply counters. Command-channel compatibility does not prove those
  observations are available to a normal player. FAIR automation needs a verified
  visibility model before using them for decisions.
- `Game` initializes from discovered PID and the server reconnects after a failed
  read. There is no exact launch-ownership lease or creation-identity check on
  that path. Validation must bind one owned executable instance and fail on exit
  or replacement rather than discovering another user's game.
- The offset module declares a build ID, but `Game` initialization does not
  compare it to the loaded executable. Layouts and caches need independent
  platform/build validation before process reads or calls.
- Some summary getters and eligibility checks inject calls that stop all game
  threads. The turn-loop read path deliberately avoids these calls. The Rust
  observation path must preserve that separation and reject torn snapshots.
- Turn-loop helpers sometimes return zero after failed reads. Missing deployment
  or production observations must remain unknown, so failures cannot trigger
  false completion or burn a durable plan entry.
- Screenshot fallback captures the whole desktop; save comparison can expose
  foreign hidden state. Neither belongs in routine FAIR observation. Recording
  and diagnostics must remain scoped to the owned game.

These are migration and validation requirements, not a claim that the new
controls are implemented. The licensed game and short Luna smoke now supply limited live evidence.
Reliable command scheduling, full routine coverage and the clean two-year run
remain open acceptance gates; neither the smoke nor the Rust reader satisfies
them.

## Rust action and input foundations

The next port checkpoint adds a durable ordinary-action journal and a Linux X11
keyboard fallback building block. The journal commits a dispatch receipt before
sending and verifies the desired state through an injected backend. Restarting
returns an existing verified receipt or refuses an uncertain dispatch; changing
the request ID cannot bypass process quarantine. Failed preflight remains
retryable. Neither library is an enabled live MCP command tool yet.

The separately owned, authenticated Xvfb transport fixture verified the three
installed keyboard bindings (Space, keypad minus/plus), matching key releases,
unchanged pointer position, and refusal on lost focus, a wrong window PID and
held keys. The display connection also passed an unresponsive-server deadline
test. All 21 Rust checks passed in the real Linux guest, including that opt-in
fixture; 17 portable checks passed on macOS and Rust 1.86. Legacy Python passed
952 tests with 37 explicit skips. These are transport/recovery fixtures, not
HOI4 action readback or campaign acceptance.

The failed game and recorder remained OS-stopped while these tests used a
separate display. The guest process-access setting remained restored to its
original value. No failed native transaction was retried.

Further static review corrected a scheduling assumption: an earlier candidate
was the in-game idler constructor, not its recurring update. The actual update
reacquires its queue before normal engine-owned posting, but an external safe
callback, lifetime and cancellation interface remains unproved. Logging and
allocation remain reachable from Post. Arbitrary-thread calls are still
quarantined; ordinary input fallback requires owned-lease integration and live
state readback before it can become a command capability.

## Optional Rust clock adapter checkpoint

The Linux owned supervisor now supports an explicit ordinary-menu/window binding,
exclusive configured-controller leases and typed desired pause or internal speed.
It verifies the private Xauthority file and actual X11 peer, focused owned window,
clear input state and current own-campaign context. The decision is refreshed
immediately before queuing each key pair. Pointer position is never changed.

The real Linux ordinary suite passed 43 library checks, 16 integration checks and
one isolated authenticated Xvfb test. Thirteen opt-in kernel checks were excluded
from this pass; their separate evidence is recorded in the launcher document.
The new cases cover automatic pause, changing speed, stale no-op receipts, busy
input, shutdown between pairs, expired authority and invalid final pause bytes.
Python 3.9 passed 952 tests with 37 explicit skips. The initial Python check used
an unconfigured interpreter and failed on missing dependencies; the configured
project environment supplied the complete passing run.

Those are transport and recovery fixtures, separate from the subsequent installed-
game clock test below. Input guards check a persisted playthrough ID, roots and
observed timeline; this does not prove same-campaign load generation. Strategic
commands, daily automation and full campaign acceptance remain open. The earlier
failed engine posting adapter was never retried.

## Passed Luna Rust clock test — 13 October 2026

A fresh game ran under the public Rust supervisor from commit
`5b594ef969e99c64bd5bb0c8eebee0cc92d905f8`, with the exact Linux build above.
One **gpt-6-luna player at medium reasoning** held the exclusive input lease.
Ordinary singleplayer setup selected 1936 Germany, Normal difficulty and
historical AI, with a fresh vanilla playset and no mods. Recording began before
Start; the player used the actual persistent Rust stdio MCP over localhost SSH.

| Verified operation | RPC time |
| --- | ---: |
| Speed 0 → 4 while paused | 2.72 s |
| Speed 4 → 3 | 1.05 s |
| Speed 3 → 4 | 1.01 s |
| Unpause | 0.86 s |
| Wait until 3 January 1936 | 19.94 s |
| Pause at test endpoint | 1.27 s |

The server wait reached `1936.01.03.01` in 19.834 seconds. Final separate readback
confirmed `1936.01.03.10`, paused, speed 4, player GER, Normal and historical AI.
All actions used fresh process state and ordinary Space/keypad keys. The player
made no screenshot/save reads or cursor movements. Pointer position matched
before and after; all inspected game threads had tracer 0 and Yama stayed 1.
This was a clock test: focus and research remained idle, with no strategic actions
or two-year/full-campaign acceptance claim. Receipt replay was not exercised live.

The first lease release included `holder`, which the strict per-operation parser
rejected before process access or mutation. The advertised schema had not made
that distinction clear. A corrected release containing only `op` and
`lease_token` succeeded, and status reported zero remaining lease time. The failed
call remains in the private journal; it sent no game input. The schema now lists
separate allowed fields for acquire, renew, release and status.

Both recording segments finalized after owned recorder shutdown and passed
ffprobe with positive duration and 512×288 video. The recorder returned 255 after
the requested stop; the owned game remained running and paused. Source manifests,
recording integrity, call receipts and the player journal are private evidence.

After the player released its lease, 20 fresh paused overviews measured **67.1 ms
median RPC** and **56.4 ms median reported snapshot time** in this emulated guest.
Each overview reported 85 logical reads / 945 bytes; those counters exclude
backend exec-witness bytes and procfs work. These samples use a newer ownership
mechanism and different load conditions than the earlier 7.5 ms Rust comparison.
They do not demonstrate a speedup over that older run. Clock context previously
nested two complete snapshot brackets; the follow-up refactor shares a private
typed body within one local bracket while retaining all structural and final
ownership/exec checks. This change needs its own measured live evidence.

## Optimized Rust follow-up — 13 October 2026

A fresh owned run used the optimized release binary from commit `0493108`, at
the same exact Linux game build, Normal historical vanilla Germany. One Luna
medium player again held the exclusive input lease. The catalog returned
`basic_machine_tools` among **552** loaded definitions; this is public metadata,
not research eligibility. Its five post-test queries measured **61.1 ms median
RPC** and **54.4 ms median reported snapshot time**. Twenty fresh paused
overviews measured **15.8 ms median RPC**, **10.4 ms median reported time** and
18.9 ms maximum RPC. Samples include persistent stdio/SSH, and the logical read
counters still exclude backend/procfs work. Different launch/load conditions and
the release profile prevent attributing the entire change to one refactor.

| Verified clock operation | RPC time |
| --- | ---: |
| Speed 0 → 4 | 1.46 s |
| Speed 4 → 3 | 0.35 s |
| Speed 3 → 4 | 0.28 s |
| Unpause | 0.17 s |
| Final pause | 0.60 s |

The bounded wait returned `torn_read` after 18.151 seconds. A fresh overview
confirmed `1936.01.03.01`, running at speed 4, before Luna sent one finish-pause
request. Its verified receipt and separate final overview confirmed
`1936.01.03.06`, paused. Lease release succeeded, with zero remaining authority.
No uncertain action was repeated. This is a bounded clock smoke with a wait
failure and successful verified endpoint, **not a clean campaign acceptance**.

The first GUI diagnostics refused the real game because the reader required
base `CGui`, while independent constructor evidence shows ordinary `CGameGui`
installs its derived primary vtable. The correction awaits a fresh owned run;
client origin, control transforms and effective hit routing remain unproved.
The own research reader also awaits installed-game validation. It exposes engine
completed membership and signed active counters, with unverified units and no
eligibility calls. Its paginated output retains complete validation before slicing.

The subsequent wait fix retries only transient torn observations within its
deadline. Payload and own-campaign context share one bracket; player,
playthrough ID, roots, settings and nondecreasing timeline must agree with the
baseline. Persistent instability and context changes return errors. A normal
wall timeout reports the latest accepted sample and its age. The persisted ID
still does not establish load generation. This fix has fixture evidence and
requires a fresh live wait test.

Pointer position matched before/after play; all inspected threads had tracer 0
and Yama remained 1. No player screenshots, saves or cursor movement occurred.
The initial static-menu recording stayed buffered; it was finalized successfully
before Start. A replacement with zero-latency encoding and explicit segment
flushing supplied a verified first packet before Start. Its two finalized
512×288 segments passed ffprobe, with durations 600.0 and 57.666 seconds. All
footage, calls, source manifests, incidents and journals remain private.

## Own research and corrected GUI live check — 13 October 2026

A fresh optimized run used commit `633524e` and the same exact Linux game build.
Ordinary singleplayer setup selected Normal historical vanilla Germany. The
recording's first packet was verified before Start. Own reads returned **47
completed technology IDs** with level/threshold 1 and four empty slots. This
validates the initial completed-membership model; active signed counters and
their units still need an actual research assignment and later observation.

The corrected GUI reader accepted the game's derived primary type and returned
physical **512×288**, logical **2560×1440**, scale **5** and logical pointer
`[1280, 40]`. This agrees with the stationary client pointer `[256, 8]` at one
point. It does not establish a general client conversion, control geometry or
hit routing. `input_mapping_verified` remains false.

One Luna medium player held exclusive clock input. Speed 0→4, 4→3, 3→4,
unpause and final pause all returned verified receipts. Replaying the first
fully verified speed request returned the same receipt in **15.5 ms**. The
durable journal retained one dispatch/verified transition for that request,
with no additional action record. No uncertain request was repeated.

The campaign-bound server wait reached **1936.01.03.01** in **19.595 seconds**,
with **zero rejected torn reads**. This is a passed live wait, but does not
exercise the transient-retry branch; that branch retains separate fixture
evidence. Unpause took 0.223 seconds and pause 0.756 seconds over persistent
stdio/SSH. A separate overview confirmed **1936.01.03.06**, paused at speed 4.
The lease was released and status reported zero remaining authority.

Twenty fresh paused overviews measured **15.7 ms median RPC**, **9.9 ms median
reported snapshot time** and **71.3 ms maximum RPC**. Ten own research queries
measured **27.0 / 19.3 ms** median RPC/reported time; ten GUI diagnostics measured
**14.2 / 9.2 ms**. The guest uses QEMU TCG and these timings are not native Linux
hardware throughput. Each overview reported 87 logical reads / 953 bytes after
the added settings witnesses; backend/procfs work remains outside those counters.

Pointer coordinates matched before and after play, every inspected thread had
tracer 0 and Yama stayed 1. No player screenshots, save reads or cursor movement
occurred. Both finalized 512×288 recording segments passed ffprobe, with
durations **600.0 and 38.0 seconds**. Calls, journal, source manifests and footage
remain private. This is bounded clock/model validation; strategic play, the clean
two-year test and complete campaign acceptance remain unfinished.

The owned shutdown request later ended this game with exit code 6. Its log
reported a rendering-connection error; the specific cause is unproved. The
recordings had already finalized, and their passed integrity checks and the
clock receipts remain valid. The shutdown incident is preserved privately.

## Own research UI navigation — 13 October 2026

A fresh owned run used `730bc98`, Normal historical vanilla Germany and the
same exact Linux game build. Its recording supplied a verified first packet
before Start. Initial own-country reads confirmed 1936.01.01.12, paused, 47
completed technologies and four empty slots.

The read-only UI probe checked all four slot/widget ownership joins and found
one `CButtonDrag` per slot. The ordinary `w` shortcut opened the research panel:
active mode 5 and its local visible predicate matched. A bounded visual
diagnostic located the normal slot rows. Builder diagnostic clicks selected
own slots 0–3 and opened the ordinary tree, verified by selected-slot membership,
mode 6 and the tree's visible predicate. No technology was assigned and the
campaign clock stayed paused at its initial date. These are navigation checks,
not Luna strategic play or campaign acceptance.

Two immediate harness assertions failed during UI transitions. In one case
slot 1 was already selected while mode 5 remained; a later fresh read confirmed
mode 6 without another click. The second occurred before a requested panel
finished opening, so no subsequent click was sent. Both failures are preserved.
The remaining navigation checks used bounded fresh observations to await the
transition, with no repeated input. This motivated the server-side UI wait
validated in the subsequent run below.

Pointer reads agreed with client `[50,70]`, `[50,110]` and `[50,130]` at the
observed scale 5. This is evidence at those points in the tested window,
not a universal control transform or hit-test proof. The probe's
`input_authorized` remains false. Independent constructor/getter analysis found
the drag renderer at `+110`; standard buttons use `+e8`, and drag state is not
a boolean disabled flag. The new exact renderer/resource diagnostics retain
fixture evidence pending a fresh installed-game check. Recordings, calls,
source manifests and failed assertions remain private.

## Passed research UI wait — 13 October 2026

A fresh owned run used `0ada7e1`, the same exact Linux build and Normal
historical vanilla Germany. Recording had a verified first packet before Start.
Initial own-country reads again confirmed 1936.01.01.12, paused, 47 completed
technologies and four empty slots. Each exact drag renderer field identified
`CSprite`; its unknown field layout correctly remained unavailable under that
build's renderer whitelist. No resource fields were borrowed from another class.

One ordinary `w` opened the panel. A server wait matched mode 5 and the panel's
local visibility. After a fresh panel precheck, one diagnostic slot-0 click was
followed by `until_research_ui: {"view": "tree", "selected_slot": 0}`. The wait
returned the verified tree, selected own slot and coherent visibility predicates
in **288.5 ms RPC / 280.4 ms server time**, with no torn-read retries or repeated
input. A separate one-second wait for the closed panel returned `wall_timeout`
in **1.009 seconds RPC**, reporting its latest accepted sample's age of 193 ms.
These timings include the emulated Linux guest and persistent SSH stdio path;
they are bounded UI tests, not a native-hardware throughput benchmark.

The final own-country snapshot remained paused at the initial date with all
four slots empty. No research was assigned. The pointer read matched client
`[50,70]` at the observed scale 5; this does not authorize a general control
transform. Two builder diagnostic refusals are retained: an initial observation
during campaign loading returned `campaign_unavailable`, and a misspelled wait
argument returned `invalid_arguments`. Neither caused repeated input. Calls,
recordings and source manifests remain private. Exact plain-sprite resource
diagnostics and strategic input still require further live validation.

## Research detail and active-counter check — 13 October 2026

A fresh owned run used `780da62`, the same exact Linux build and Normal
historical vanilla Germany. Recording supplied a verified first packet before
Start. All four exact plain-sprite renderers resolved their direct resource to
`CSpriteType`. One ordinary slot selection was followed by a coherent own-tree
wait in 19.5 ms RPC, with no repeated input. These are observations at the tested
window and points, not general hit-routing or coordinate authorization.

Two view-only technology selections validated the new detail reader.
`support_weapons2` had a disabled ordinary Research control; `support_weapons`
had raw state 0 and the positively named ordinary control matched its callback
binding. Canonical IDs came from both database and own-status joins. Closing the
first detail changed its observed visibility without discarding its identity.

A builder diagnostic then clicked the visible ordinary Research button once.
Fresh memory confirmed `support_weapons` in own slot 0, with raw counter
101,000; the other three slots stayed empty and the clock stayed paused at the
initial date. This was ordinary GUI validation, using a bounded visual setup
diagnostic. It was not a production ID-based command or Luna research assignment.
The Rust diagnostic still reports `input_authorized: false`.

One Luna medium player then held exclusive ordinary-clock input. Speed 4,
unpause and final pause returned verified receipts. The original server wait
reached 1936.01.05.01 in **39.582 seconds RPC / 39.540 seconds server time**,
with zero torn-read retries. The player's tool wrapper initially discarded the
ongoing command's session ID at its 30-second yield; the persistent protocol log
later supplied the successful original response. An unnecessary additional
read-only wait returned the same date predicate. Both the original incident and
its correction remain in the private journal; the original wrapper process exit
status is unavailable. No uncertain action was repeated.

Final independent reads confirmed 1936.01.05.06, paused at speed 4, and the same
slot-0 technology with raw counter **505,000**. Completed membership stayed at
47 and the other slots stayed empty. This proves an active counter changes
during play; normalized units, progress and remaining days remain unverified.
The lease was released, status reported zero remaining time, pointer coordinates
matched before/after Luna play, every inspected thread had tracer 0 and Yama
stayed 1. Luna used no screenshots or cursor movement.

Both finalized 512×288 recordings passed ffprobe, with durations 600.0 and
518.333 seconds. The subsequent owned shutdown request completed, but the game
again returned normal exit status 6. Its rendering-client error is preserved;
the exact cause remains unproved. A completed close request is not a clean game
exit. Recordings, source manifests, calls, UI diagnostics, intent/readback and
shutdown incidents remain private. Strategic adapters, the clean two-year run
and full campaign acceptance remain unfinished.

## Subsequent focus validation and node failure

A fresh owned Normal historical Germany run returned all 438 own canonical
focus keys across five pages. The first query took 81.5 ms RPC / 67.6 ms server;
the five full-model page queries took 56–74 ms RPC. Filtering resolved the actual
key `GER_remilitarize_the_rhineland`; the older guessed `GER_rhineland` matched
nothing. No eligibility was inferred from catalog membership.

An ordinary view-only card click opened `GER_bribe_senior_officers`. The reader
joined its own detail and GUI registry and read the exact named Start control:
disabled raw state 3. Closing first left a retained detail during animation,
then the detail array became empty. No focus was started.

Research slot navigation again passed: the server confirmed own slot 0 and the
visible ordinary tree in 285.7 ms, without repeated input. The new node query
then failed its exact type check in 22.3 ms. Static review found a register
tracking error: the 40-byte inline record stores the saved item at byte 16;
byte 32 holds a subsequently overwritten context register. The earlier synthetic
fixtures mirrored the wrong assumption. The extraction is corrected with
distinct context decoys. The subsequent corrected live check is recorded below.
Exact type and own-country checks have not been relaxed.

The campaign remained paused on 1 January with no assigned focus or research.
Two recording segments passed ffprobe (600 and 337 seconds). Owned shutdown
returned successfully, while the game's sole reaper recorded normal exit code
6, no terminating signal and no core dump. This confirms the earlier exit-code
distinction; it does not resolve the nonzero shutdown cause. No Luna player or
strategic campaign acceptance is claimed for this diagnostic run.

## Corrected current-node validation — 13 October 2026

A fresh owned run used `a81455b`, the same verified Linux build and Normal
historical vanilla Germany. Recording supplied a verified first packet before
Start. The corrected reader returned **411 canonical own technology keys from
423 records**, and all **423 ordinary structural control associations** passed.
Five full-model pages, prefix filters and the final short page agreed. Each
query validated all records before filtering or paging; no own-country or exact
type check was relaxed. These observations include materialized folders and do
not prove that every node is currently visible or selectable.

The default 25-node page took **409.2 ms RPC / 360.0 ms server**, with **34,276
reads and 512,416 bytes**. Five 100-node pages took 326–559 ms RPC. These timings
include the emulated guest and persistent SSH path and precede request-local
shared-proof reuse. The own focus query returned 438 keys in 61.4 ms RPC.
The stationary-pointer tree wait took 21.0 ms RPC; after one ordinary Escape,
the closed-tree node query correctly returned `gui_unavailable` in 15.1 ms.

Final own state remained paused at 1936.01.01.12 with no focus or research
assignments. Escape cleared the research views but did not open the quit menu;
that diagnostic limitation is preserved. Recording finalized before owned OS
shutdown. Both 512×288 segments passed ffprobe (600.0 and 231.333 seconds), and
the sole game reaper recorded **exit code 0, no signal and no core dump**.
Yama remained 1 and all 14 inspected game threads had tracer 0 before close.
Earlier nonzero shutdowns remain unexplained. This run had no Luna player and
provides no strategic-input or campaign acceptance. Calls, source manifests,
recordings and terminal evidence remain private.

## Request-local reuse live comparison — 13 October 2026

A fresh recorded run used `28e0967` and the same Normal historical vanilla
Germany setup. All 411 keys matched the preceding run exactly, and all 423
structural associations remained verified. Prefix filters, five full-model
pages and closed-tree refusal passed. Default-page logical reads fell from
34,276 to 26,686 and bytes from 512,416 to 299,776. Five RPC samples ranged from
344.3 to 423.8 ms, median 383.2 ms; the preceding single sample was 409.2 ms.
Different fresh processes and host load limit the latency comparison. See the
[measured comparison](CLAUSEWITZ_COMPARISON.md#current-node-query-comparison--13-october-2026).

The final campaign stayed paused at 1936.01.01.12 with no focus or research
assignments. One Escape closed the research tree; the node query then refused
with `gui_unavailable` in 20.8 ms. Recording finalized before owned shutdown,
passed ffprobe at 371.333 seconds, and the sole reaper recorded exit code 0,
no signal and no core dump. All 14 inspected threads had tracer 0 and Yama
remained 1. This run did not query the focus catalog, use a Luna player or
establish strategic-input/campaign acceptance. Its private evidence is retained.

## Bounded vectored reader and one-ID diagnostic fixture gate

The next source slice batches final research-UI witness reads on Linux while
preserving every recorded byte comparison and process/exec/mapping brackets.
A dedicated exact-key diagnostic exposes conditional raw renderer/cache fields
and typed own Info state; it still supplies no client rectangle or input authority.
All 176 standard optimized Linux fixtures and 19 generated-child checks passed
under ordinary UID and unchanged Yama 1. The latter include 18 substantive cases
and the private helper; six new cases exercise vectored bytes, partial reads,
mapping/protection changes and same-image exec. Portable/MSRV Rust tests passed
135 cases; Python passed 952 with 37 skips.

The first kernel trial passed 17 checks and failed two expected error-category
assertions: the fixtures consumed an acknowledgement sent before exec, so the
reader refused with `io_error` during replacement. Waiting for the replacement's
nonce-checked ready/pid message fixed the tests; both then returned the expected
`ownership_changed`. Production guards were unchanged, and failed logs are
preserved. These generated-process and synthetic UI checks are separate from
installed-game evidence.

## Recorded vectored reader and exact-ID diagnostics — 13 October 2026

A fresh owned Linux game used the immutable tested binary from source
`c4c8404`. Normal historical vanilla Germany, no mods, was recorded before the
single Start action. All 411 current technology keys and 423 structural control
associations matched the prior two runs; all five pages and both filters passed.
Five default-page RPC samples were 450.6, 362.5, 423.1, 351.3 and 335.1 ms
(median 362.5 ms), with server times 382.5, 305.7, 361.1, 293.7 and 280.5 ms.
Logical requests/bytes remained 26,686 / 299,776, now explicitly labelled as
logical metrics. Different fresh processes and host load prevent a controlled
speedup claim.

Exact `support_weapons` and `basic_machine_tools` queries each resolved one
bound own item and verified CSprite/CSpriteType raw fields, finite scalars,
cache dimensions and the no-mask branch. Their first parent was an exact
`C2dPositionObject` primary; no parent payload was traversed. Cached world
translations were negative and remained raw diagnostics, never client points.
Their RPC times were 325.2 and 305.4 ms. Rectangle, routing and input authority
stayed false.

A builder view-only click landed in the background gap: immediate and later
readback both found no Info window. A newly observed icon click then selected
`support_weapons2` rather than the intended `support_weapons`; canonical readback
identified the mismatch and its disabled Research button. No assignment was
attempted. The exact-ID diagnostic correctly reported one locally visible Info
window for the actual selected key and zero for the intended key, then zero
after one Escape closed the Info window. A separate Escape closed the tree;
the node query refused with `gui_unavailable` in 19.0 ms and the one-ID
diagnostic in 24.1 ms. This validates the diagnostic and refusal behavior,
not an ID-based selection adapter or fresh event acknowledgement.

Final state remained 1936.01.01.12, paused at speed 0, with no focus or research
assignments. All 14 game threads had TracerPid 0; Yama remained 1. The recording
finalized as one 512×288 H.264 segment, 565 seconds, 6,882,761 bytes, and passed
ffprobe. Explicit owned close returned success; the sole reaper recorded actual
exit code 0, no signal and no core dump. No Luna player was assigned to this
diagnostic run. Earlier exit-code-6 incidents remain unresolved, and strategic
dispatch, the daily governor and campaign acceptance are unfinished.

## Target scope and typed hierarchy fixture gate

The next development slice retains full-node validation and adds a narrower
exact-ID path. It resolves the requested public key across the complete bounded
catalog and validates every current item's type, ownership and canonical join
before target GUI work. Unrelated GUI bindings are outside that declared model.
The result explicitly reports its validation scope and
`full_gui_model_validated:false`; no proof is cached across requests.

A typed ancestor diagnostic checks constructor-created own Container positions,
reciprocal parent/child membership and independently proved graphics roots.
Unknown ancestor primaries stop before payload; null position control pointers
are permitted. Bounds are 16 ancestor steps, 2,048 child capacity per header and
4,096 aggregate entries. Hierarchy provenance does not establish own-country
semantics of every ancestor, geometry projection, visibility, routing or input.

All 201 standard optimized Linux fixtures passed under the ordinary UID and
unchanged Yama 1. Portable and Rust 1.86 tests passed 160 cases each; Python
passed 952 with 37 skips. Existing 19 generated-child kernel checks passed at
the preceding vectored-reader gate; they were not rerun for these unchanged
kernel primitives. The first combined host/MSRV trial passed 94 snapshot cases
and failed one metric assertion comparing a target with raw hierarchy work
against a full-node query without that work. That non-comparable assertion was
removed; poisoned unrelated-GUI read spies and all ownership, ambiguity and
mutation cases remain. Failure logs are preserved. Installed-game validation
and target-specific latency are pending in a fresh immutable run.

## Target-scoped installed-game refusal and correction — 13 October 2026

A fresh recorded game running source `0618678` verified the unchanged full-node
model: 411 keys and 423 structural associations, all pages and filters. Five
RPC samples were 326.2, 333.3, 336.8, 449.0 and 505.5 ms (median 336.8 ms).
Logical requests/bytes remained 26,686 / 299,776. The first target-scoped
`support_weapons` query refused in 20.1 ms before reading target payload:
its catalog guard incorrectly required index zero to be NULL. The independent
public catalog query passed with 552 usable definitions in 59.2 ms RPC.
No target-scoped hierarchy result passed this run, and the failure was retained.

Independent current-build constructor/getter analysis established that index
zero holds a nonnull typed null-object singleton. The corrected reader requires
that entry to equal the fresh singleton global and have its exact primary type;
it reads no reserved-object payload. Positive canonical IDs, complete current
record ownership and final byte witnesses remain mandatory. Null, mismatched,
aliased and mutated sentinel fixtures refuse. A fresh installed-game check is
still required.

Final Germany state stayed at 1936.01.01.12, Normal/historical, paused at speed 0,
with no mods, focus or research assignments. Closed-tree queries refused in
20.8/23.0 ms. All 14 threads had tracer 0 and Yama remained 1. The recording
passed ffprobe: one 512×288 H.264 segment, 566 seconds, 6,928,231 bytes.
Owned signal close completed, but the sole reaper recorded **normal exit code
6**, no signal and no core dump. This is not a clean exit. The rendering log
reported a failed read with EINTR; its causal relation to exit code 6 remains
unproved. No Luna player was assigned to this diagnostic run.

## Singleton, input metadata and window-close fixture gate

The corrected source also adds metadata-only identification of the own idler's
input receiver. It infers no receiver payload, virtual getter layout or client
origin; full byte witnesses cover the diagnostic. The explicit owned window-close
preview has fixed deadlines, retained uncertain delivery and no automatic signal
fallback or replay. Four isolated no-XTEST runtime cases passed, including the
complete owner/reaper success path and severed/withheld wire replies.

Portable and Rust 1.86 suites each passed 164 tests. The optimized Linux standard
suite passed 212 reported checks (211 substantive and one inert helper), with
its game display identity and Yama 1 unchanged. Existing read-only kernel
primitives were unchanged and their preceding 19-check gate was not repeated.
Clippy, formatting, Linux/MSRV compilation and Python protocol smoke passed.
A full Python run exposed a real background-clock deque mutation race. Taking
a shallow history snapshot before filtering fixed it; a deterministic fixture
verifies that concurrently appended records remain available on the next poll.
The final Python suite passed 953 tests with 37 skips; the failed run is retained.
These are source/fixture results, not new installed-game or campaign acceptance.

## Recorded receiver and window-close check — 13 October 2026

The first launch attempt refused an overlength Unix socket pathname before
spawning a game. Its partial private userdata and error were preserved. A fresh
shorter-path run then used immutable source `b34899a`, Normal historical vanilla
Germany with no mods, and recording started before the single campaign Start.

Own receiver primary metadata identified `CPdxMouse`. The diagnostic reported
512×288 physical and 2560×1440 logical dimensions, inverse ratios 5/5, and logical
pointer 1280/40 after parking the ordinary client pointer at 256/8. This validates
metadata and the observed arithmetic, not client-origin or event authority.
Independent analysis corrected an earlier getter label: its tail callee returns
packed coordinates. A new conditional point-source diagnostic checks the exact
receiver/getter and built-in dynamic implementation before reading signed source
coordinates. Replacement or unsupported implementations remain unavailable;
client origin, window association and event acknowledgment stay unverified.

All 411 keys and 423 associations matched prior full-node results. Five RPC
samples were 323.4, 331.5, 335.3, 353.3 and 322.7 ms (median 331.5 ms), with
270.7–291.7 ms server times and unchanged 26,686 logical requests / 299,776 bytes.
The first exact-ID query passed the corrected catalog preflight, then refused
`invalid_gui_hierarchy_bound` in 86.3 ms before child payload. It did not complete
its target model or final witness pass. The actual capacity was unavailable in
that source, and the failed result is retained.

The next source keeps capacity 2,048 and aggregate 4,096 limits. A structurally
valid policy overrun now produces an explicitly incomplete hierarchy subtree
with bounded counts and completed joins only. Alignment, header, ownership,
reciprocal-link, cycle, read and torn-witness errors still refuse the result.
No uninspected payload is read and no geometry/input authority is added.

One Escape closed the tree; node/detail calls refused in 42.8/21.6 ms. Final
state remained paused at 1936.01.01.12, speed 0, with focus and all research empty.
All 14 threads had tracer 0 and Yama remained 1. Recording passed ffprobe: one
512×288 H.264 segment, 360.333 seconds, 4,586,610 bytes. One explicit owned
`WM_DELETE_WINDOW` request completed in 2.31 seconds; the sole reaper recorded
exit code 0, no signal and no core dump. No signal fallback or replay occurred.
The close client did not persist its successful wire-delivery classification;
actual exit is the recorded result. This proves that ordinary close path on the
installed build, without explaining earlier exit-code-6 incidents. No Luna
player, strategic dispatch or campaign acceptance is claimed for this run.

## Optional hierarchy, point-source and endpoint fixture gate

The combined source passed 171 portable and Rust 1.86 tests, including 105
snapshot cases. The optimized Linux standard suite passed 225 reported checks
(224 substantive and one inert helper), including six new path cases, and all
four isolated no-XTEST close cases passed again. Actual boundary tests accept
107 pathname bytes plus NUL, reject 108 with a typed error, handle raw byte
lengths and refuse overlength launch paths before creating run/userdata files.
Display identity and Yama 1 stayed unchanged. Python passed 953 tests with
37 skips; formatting, Clippy, MSRV Linux compilation and protocol smoke passed.
The new optional hierarchy and point-source outputs still need a fresh immutable
installed-game check. The preceding installed-game close evidence used `b34899a`.

## Source-window association fixture gate

The next conditional diagnostic joins the constructor-proved built-in receiver
and point source to active-video callbacks, current window magic, reciprocal X11
driverdata and unique selected membership in a complete registry capped at 128.
It reports a native window ID, dimensions and raw relative-mode flags; unrelated
entry payloads and display internals are never read. Unsupported branches and
valid policy overruns are unavailable; malformed pointers, links or headers and
changed witnesses refuse. OS ownership, client origin, fresh events and input
mapping remain explicitly unverified.

Portable and Rust 1.86 suites passed 185 tests, including 119 snapshot cases.
The actual optimized Linux suite passed 239 reported checks (238 substantive
and one inert helper), under unchanged display ownership and Yama 1. Python
passed 953 tests with 37 skips; Clippy, formatting, MSRV Linux compilation and
protocol smoke passed. The unchanged close transport's separate four no-XTEST
cases passed at the preceding 225-check gate and were not repeated here.

An independent offsetof probe against the exact distro header version matching
the installed Xlib confirmed XEvent size 192, XMotionEvent size 96, native window
offset 32 and signed coordinate offsets 64/68. Header packages were extracted
privately without installing system packages. The
[X.Org specification](https://www.x.org/releases/X11R7.5/doc/libX11/libX11.html)
supports event-window-relative motion coordinates. This ABI evidence and the
fixtures do not establish fresh-event acknowledgment, backing scale or canonical
control routing. Installed-game association validation remains pending.

## Recorded scoped IDs and source-window association — 13 October 2026

A fresh owned Linux game used immutable source `e4ca810`, Normal historical
vanilla Germany with no mods. Its first video packet preceded the single Start.
The built-in receiver/getter, dynamic implementation and reciprocal X11 window
association passed live validation. An independent read-only OS comparison
matched native window ID 4194326, dimensions 512×288, zero relative-mode flags,
raw point 256/8 and logical point 1280/40. That comparison took 34.1 ms RPC;
the initial geometry query took 48.5 ms RPC / 41.1 ms server, with 95 logical
reads and 984 bytes. This corroborates one current window/profile; event,
origin, routing and production-input fields stay false.

The corrected target-scoped reader completed all five samples for each key,
including final witnesses. `support_weapons` RPC samples were 215.1, 133.3,
113.9, 116.3 and 114.5 ms (median 116.3 ms); server times were 104.3–197.8 ms.
`basic_machine_tools` samples were 115.7, 120.2, 122.9, 120.2 and 122.4 ms
(median 120.2 ms), with 106.1–111.7 ms server times. Each used 10,903 logical
requests, with 185,562 and 186,266 bytes respectively. The declared scope
validates all current-record ownership but excludes unrelated GUI payloads;
`full_gui_model_validated` remains false. These are successful narrower queries,
not a matched speedup over the previous complete-GUI diagnostic.

Six typed reciprocal position ancestors and own Container joins passed. The
observer then stopped before the owned graphics root's child payload: capacity
5,395 and count 4,142 exceed its 2,048/4,096 policies. It retained prior verified
entries (53/97), reported the exact boundary and left root closure and input
false. This is an explicitly incomplete hierarchy result, not malformed data
or permission to widen every array bound.

The full-node model again matched 411 keys and 423 associations, all pages and
filters. Five RPC samples were 308.0, 304.8, 312.7, 333.5 and 319.2 ms (median
312.7 ms), with 255.8–285.0 ms server times and unchanged 26,686 requests /
299,776 bytes. Separate processes, load and scope still limit comparisons.

One builder view-only selection from the observed screen selected the intended
`support_weapons`; canonical readback reported one matching visible Info window
in 119.4 ms, then zero after one Escape in 124.6 ms. Research was not assigned.
A separate Escape closed the tree; node/detail calls refused in 22.1/19.4 ms.
Final state stayed at 1936.01.01.12, paused at speed 0, with focus and all four
research slots empty. No Luna player or strategic adapter was used in this run.

All 14 inspected threads had tracer 0; Yama remained 1. Both 512×288 H.264
segments passed ffprobe: 600 and 339 seconds, 6,975,543 and 4,289,564 bytes.
One owned WM close completed in 2.28 seconds; the sole reaper recorded exit code
0, no signal and no core dump, without signal fallback or replay. Earlier exit
code 6 remains unexplained. Clean two-year and full campaign acceptance remain
unfinished. Calls, source manifest, footage and actual failures stay private.

## Owned-root and conditional cache fixture gate

The next source separates the measured graphics root from ordinary ancestors.
Only exact owned embedded graphics roots permit capacity and aggregate limits of
8,192, including preceding inspected entries. Ordinary ancestors retain
2,048/4,096. Complete pointer arrays are witnessed; sibling payloads are never
dereferenced. Fixtures cover the actual 5,395/4,142 size, exact aggregate boundary,
unmapped overruns, duplicate/missing membership and final-byte mutation.

A closed typed reciprocal chain also supplies an optional unit-scale cache
diagnostic. Recovered root seeds, float32 translation addition and sampled
visibility/descent branches determine whether cache comparison is available.
Matching caches do not prove a current frame, atomic publication, routing or
event acknowledgement. Nonunit, nonfinite, skipped and incomplete branches
remain explicitly unavailable; input authority stays false.

Portable and Rust 1.86 suites each passed 194 tests, including 128 snapshot
cases. The actual optimized Linux suite passed 248 reported checks (247
substantive and one inert X11 helper), with display ownership and Yama 1
unchanged. Python passed 953 tests with 37 skips; formatting, Clippy,
MSRV Linux compilation and protocol smoke passed. The preceding four no-XTEST
close cases remain separate unchanged-transport evidence. These new hierarchy
and cache outputs still require a fresh immutable installed-game run; they do
not establish strategic dispatch or campaign acceptance.

## Ordinary Research button and own economy fixture gate

The exact-ID reader now inspects only its unique locally visible matching own
Info's named Research control. It checks Standard ownership, the normal alias,
bounded reciprocal listeners, the independently recovered first void callback
capture and conditional renderer ownership. Standard's renderer offset and
void callback differ from the node Drag's. Unsupported variants remain
unavailable; no geometry, eligibility, confirmation reader or action is added.
Fifteen new snapshot fixtures cover those distinctions, guard order and mutations.

The new no-argument `hoi4_economy` reader resolves only the own country and
verifies exact country/tag, production and political-status owner joins before
payload. It returns three named factory trigger counts, separate civilian
project capacity and signed political-power units as an exact rational. The
current-build formulas and fixed-point divisor were recovered independently;
upstream offsets and property interpretations were not copied. UI factory
totals, idle builders, shortages and final stability/war support remain unavailable.
Eight separate fixtures exercise arithmetic, negative PP, boundaries, owner/type
refusal, final witnesses, context changes and argument validation.

Portable and Rust 1.86 suites each passed 217 tests. The actual optimized Linux
suite passed 271 reported checks (270 substantive and one inert X11 helper),
including 143 snapshot and eight economy cases. Display ownership and Yama 1
remained unchanged. Python passed 953 tests with 37 skips; formatting, Clippy,
MSRV Linux compilation and protocol smoke passed. These new outputs require a
fresh installed-game comparison. Strategic actions and campaign acceptance
remain unfinished.

## Attribution

The findings above refer to upstream `calling/{mem,process/linux,api,game,
commands,combat,ragemicro,turnloop}.py` and `mcp/server.py` at the pinned commit.
Clausewitz-MCP is Copyright (c) 2026 smerss, MIT licensed. See [NOTICE](../NOTICE)
and [the retained provenance](../third_party/clausewitz_mcp/UPSTREAM.json).


## Bounded pointer transport and renderer widths

The next source slice adds a separate Linux X11 pointer library for one motion,
left-button press and release. It checks the owned process/window, authenticated
peer, actual keyboard focus, clear input state, client geometry, parent chain,
translated origin and fresh root bounds. Preparation, repeated guards, dispatch
and reply share one fixed deadline. Every possible wire write makes delivery
uncertain until acknowledged; severed or withheld replies never trigger replay.
Server acknowledgement always retains `game_action_verified: false`. No raw
pointer MCP command is registered; a strategic adapter still needs the exclusive
input gate, durable attempt and canonical game-state readback.

The first actual isolated Linux run refused both new pointer tests before input:
x11rb retained the three requests in its output buffer rather than the helper's
local queue. Explicitly flushing into that local-only queue before arming dispatch
fixed the issue. The corrected isolated tests and final combined suite passed.
Tests covered a moved bordered parent, resize, changed PID/class/focus, exactly
one translated motion/button pair, partial delivery and withheld acknowledgement.
Root-bound arithmetic was tested separately; no actual game or root-screen resize
was used by these fixtures. Keyboard and authentication bodies remain unchanged.

Matching Research-button diagnostics now distinguish exact Frame and Instant
renderer constructors and reciprocal resource aliases. Instant scale fields are
floats, never resource pointers. Only the exact Frame resource permits bounded
raw grid and cached-dimension fields, with the recovered lazy-cache condition.
Plain-sprite diagnostics also correct the handle sentinel to signed 64 bits and
orientation to one byte. Neighboring-byte and high-handle mutations are covered.
An initial orientation fixture had the wrong expected synthetic cell dimensions;
correcting that test preserved the production guards. These observations compute
no click rectangle and establish no current-frame, routing or action authority.

Portable and Rust 1.86 suites each passed **227 tests**, including 153 snapshot
cases. The actual optimized Linux suite passed **284 reported checks** (283
substantive and one inert X11 helper), including eight economy cases and five
isolated X11 cases. The owned game/display remained separate, Yama scope remained
1, and the previous exact game and owner were absent. Python passed **953 tests,
37 skipped**, in 18.923 seconds. Formatting, host/Linux Clippy and MSRV Linux
all-target compilation passed. Installed-game comparison and strategic assignment
remain pending; no clean two-year run or full campaign is established.


## Correction: resource orientation is a full word

The byte-orientation claim in the preceding checkpoint was incorrect. Further
exact-build disassembly showed that the dimension helper, hit-bound method and
mask method all read four bytes at `+0x10c`; no byte orientation getter was
proved. The original claim and its fixture are retained as a failed evidence
interpretation, rather than reclassified as a separate valid getter. Source now
reads the full unsigned word in both resource diagnostics. A high-byte boundary
case leaves plain-sprite derived dimensions unavailable, and high-byte mutation
must fail the final witness. Frame/Instant diagnostics retain the full raw value
without calculating a hit rectangle.

Fresh recorded run 017 had already launched with immutable source `9ffac46`
before this correction. Its derived dimensions are not accepted for actions;
the binary will not be replaced inside that run. Economy, ownership and separate
transport checks remain distinct evidence. Corrected-source test and live gates
are reported separately below when actually completed.


The corrected source passed **229 portable/MSRV tests** and **286 actual
standard Linux checks** (285 substantive and one inert helper), including 155
snapshot cases. Final-word mutations and neighboring-byte values passed their
regression guards. Python passed **953 tests with 37 skips** in 20.829 seconds;
formatting, host/Linux Clippy, MSRV Linux all-target compilation and protocol
smoke passed. The first new fixture used a nonexistent output field in its
assertion; correcting that test's field name required no production change.
These corrected-source checks remain separate from immutable live run 017.


## Recorded own-economy, root closure and actual Research renderer

Fresh run 017 retained immutable source `9ffac46`, Normal historical Germany and
no mods. Its video packet was verified before the single campaign-start click.
All observations left the initial pause state unchanged. Own economy returned
35 civilian, 28 military and 10 naval factory trigger counts, project capacity
20 and signed PP 232400/100000. Precise comparison with UI totals remains
pending; trigger semantics are not relabeled as a general factory total.

The owned graphics root closed through its 4,142-entry array. The visible
`support_weapons` path returned a consistent conditional unit cache; hidden
`basic_machine_tools` correctly returned unavailable traversal-cache status.
Exact-key RPC medians were **131/159 ms** across five samples each. The full
model retained all **411 keys and 423 associations**, with median **366 ms**,
26,686 logical reads and 299,776 bytes. The scopes and concurrent emulation
conditions differ; no equivalent-model speedup is established.

One observed-screen view-only selection returned the correct support-weapons
Info. Its named ordinary Standard control, alias and void callback verified,
but its actual renderer was **CTextSprite**, outside the implemented renderer
branches. This is a concrete unsupported class, not a successful hit model.
The running binary's derived dimensions remained unaccepted after the separate
orientation-width correction. A single observed-screen Research click then
assigned the requested canonical technology to own slot 0: two readback samples
confirmed it, the raw counter was 101000, and the other three slots stayed empty.
There was no replay, engine call, production ID-based adapter or Luna run.

Both H.264 512×288 video segments finalized and passed ffprobe: **600 seconds**
and **116.333 seconds**. One ordinary WM close produced actual normal exit code
**0**, no signal/core dump, in **2.18 seconds**. Exact game and owner were absent,
and the old stdio client closed. This is bounded diagnostic evidence; campaign
acceptance and precise economy UI comparison remain pending.

## Own-division snapshot and separate renderer diagnostics

The next source adds a no-argument own-division snapshot. It validates the own
country list and owner before unit payload, preserves both opaque handle words
and joins only each own location's selected province-catalog entry. Handles are
scoped to this owned process/campaign observation; generation and persistent-ID
semantics remain unavailable. Signed strength/organisation are lossless decimal
strings with no guessed absolute units. No foreign-unit, full-map or registry
traversal and no army action are introduced.

Frame and actual Text renderers have distinct raw diagnostics after reciprocal
Standard/control/resource gates. Text uses its own count, cache and transform
fields; it is not unwrapped as Frame. Bounded common fields and an explicit Text
leaf gate permit typed reciprocal hierarchy observation while keeping the
ancestor whitelist unchanged. Unknown types stop before payload. No font/text
contents, origin calculation, client rectangle, frame acknowledgement, effective
routing or input authority are exposed by these readers.

The prior Linux CI failed a forwarding-fixture assertion that required one
received motion after a write followed by immediate disconnect. A captured wire
write with no server reply permits zero or one observed motion. The corrected
test retains exactly one captured motion request, no buttons/reconnect/replay,
unknown delivery and no game-action claim; the withheld-reply case still requires
three actual events. Production transport guards were unchanged.


The combined source passed **252 portable/MSRV tests**, including 170 snapshot
cases and eight independent own-army cases. The actual optimized Linux suite
passed **309 reported checks** (308 substantive and one inert helper), including
all five isolated X11 cases. Python passed **953 tests with 37 skips** in 19.985
seconds; formatting, host/Linux Clippy, MSRV Linux all-target compilation and
protocol smoke passed. Own-army and new Text/Frame outputs still await a fresh
immutable game; these fixtures do not establish their live comparison or an
ID-based strategic command.
