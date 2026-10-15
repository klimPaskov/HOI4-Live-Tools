# Clausewitz-MCP Linux validation

Updated 15 October 2026. This records preflight and partial licensed-game evidence for
the [Linux-first migration](LINUX_RUST_HANDOFF.md). One **gpt-6-luna player at
medium reasoning** used the guarded upstream MCP in an actual Linux Germany
campaign. Focus, four research assignments, unpause and one day of advancement
were verified. The final pause command timed out during posting; the adapter
is quarantined. A subsequent fresh Rust run passed Luna's ordinary clock test
through 3 January 1936, including verified pause. These are **bounded live tests,
not campaign acceptance or a successful campaign**. The Rust port is underway.
The latest Rust Focus worker gate, run041, passed Start, active-focus waiting,
idempotent updates, budget-preserving restarts and bounded digest/detail output.
Its recording finalized and the owned game exited normally; see the final section.

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


## Recorded own divisions and ID-derived research points

Fresh run 018 used immutable source `e557fe4`, Normal historical Germany and no
mods. The first video packet preceded the single campaign-start click. All reads
preserved the initial pause state. Six initial observation tools passed, including
the new own-division reader: **30 unique full handles**, all with valid selected
province joins. Five samples retained the same IDs and count, with RPC median
**53.9 ms**. Raw strength/organisation units and precise UI comparisons remain
unverified; a successful structural read does not prove those missing meanings.

The actual named Research Standard used exact CTextSprite/CTextSpriteType. Its
reciprocal owner, resource, bounded cache, zero transform and typed own-root
hierarchy passed. Width/height were 123/36, resource count and hit divisor both
1, scalar 1, cached translation 634/-47 and anchors 512/288. Conditional cache
consistency passed; frame and effective-routing authority remained false.

A separate builder diagnostic then closed Info, resolved `support_weapons` by
its canonical own key and computed the conservative local interior from fresh
Rust observations. Sprite origin 150/313 and center 186/349 quantized to client
**37/70**; one ordinary OS attempt selected the correct Info. Text origin 890/155
and center 951/173 quantized to client **190/35**; one Research attempt assigned
the canonical technology to own slot 0. Three bounded readback samples confirmed
it, the counter was 101000, and the other slots remained empty. This sequence
used no screenshot-derived coordinates, callback invocation, forced command or
replay. It used a private Python diagnostic around Rust MCP observations and
ordinary OS input; the production Rust command adapter is still unfinished.

Both H.264 512×288 video segments finalized and passed ffprobe: **600 seconds**
and **476.333 seconds**. One WM close returned actual normal exit **0**, no signal
or core dump, in **2.38 seconds**. Exact game/owner were absent and the old stdio
client closed. No Luna run, clean two-year acceptance or full campaign occurred.

## Portable research-point building block

The next source adds pure typed Rust candidate maths for the supported Sprite
and zero-angle Text branches. It preserves float32 origin/height/sign handling,
full-word orientation, resource/cache/mask guards, strict interiors, physical
quantization, clipping and owned SDL/X11 scale/window checks. An explicit
owner-reviewed origin profile is required; diagnostics cannot infer approval
from false authority fields. Every candidate retains frame, effective-routing
and input-authority flags false. No process read, input, raw-click MCP tool or
owner command registration is added by this library.

Six independent cases cover both successful coordinate examples, the recorded
Text control, unsupported/mixed branches and narrow targets/invalid mappings.
Portable and Rust 1.86 suites each passed **258 tests**. The optimized actual
Linux suite passed **315 reported checks** (314 substantive and one inert helper).
Python passed **953 tests, 37 skipped**, in 22.069 seconds; formatting, host/Linux
Clippy, MSRV Linux all-target compilation and protocol smoke passed. Fixtures
remain separate from run 018's earlier immutable binary. Next integrate leases,
durable one-attempt tokens and bounded canonical verification before Luna play.


## Owned ordinary research preview fixture gate

The assembled Rust owner now optionally registers a one-stage ID-based research
preview for the corroborated 512×288 builtin client profile. It borrows the clock
controller, lease and shutdown gate; routing never waits for the transaction
mutex. Fresh witnessed research/UI/candidate contexts are joined by own status
and full slot identities, pinned for the attempt. Ownership is checked before
active technology payloads. A final read-only stage check follows X11 preparation.
Durable intent precedes possible input; unknown delivery gets bounded canonical
readback without another event. Clear input is required before terminal success,
and historical receipts are labeled. No external callback or engine post occurs.

Read-only focus-node and requested-slot callback diagnostics use independently
proved class-specific bindings. They grant no geometry or input authority.
Review caught and fixed deadline, receipt-transition, capacity, nullable-window,
context-join, scope-pin and guard-order gaps. Fixture-only owner omissions and
an incorrect mutation length were corrected without weakening guards.

Portable and Rust 1.86 suites each passed **313 tests**. Optimized actual Linux
passed **374 reported checks** (373 substantive and one inert helper), including
zero-event final-check refusal and all five isolated X11 cases. Python passed
**953 tests, 37 skipped**, in 20.619 seconds; host Clippy, MSRV Linux compilation
and stdio smoke passed. These are fixture gates. The next fresh immutable
recorded game must verify this owner adapter with exclusive Luna medium input;
no clean two-year run or full campaign is established.


## Recorded run 019: exclusive Luna medium Rust research smoke

On 13 October 2026, a fresh immutable Linux game used source `4b7bdecc` and CLI
SHA-256 `7aa02849b63919c4d22dde2caba7433e1808cb7ad9b1466b80294687f6036e00`.
Germany used Normal difficulty, historical AI and no mods. Recording had a
verified first packet before the single campaign-start action. The builder
prepared only the menus and selected empty own slot 0, then bound the optional
research profile and relinquished input to one Luna player at medium reasoning.

Luna resolved `support_weapons` from the own technology-node list, then made
one View request and one Research request with separate durable IDs. Both
returned `Verified`, `current_state_verified=true`, and `historical=false`.
Independent own-state reads confirmed the requested technology in slot 0.
The RPCs took **774 ms** and **851 ms**, respectively, in the emulated VM.
This is a two-action sample, not a general latency benchmark. No screenshots,
raw coordinate arguments, external callbacks or native posts were used by Luna.

Speed 4, unpause and terminal pause verified through ordinary keys. One server
wait reached **3 January 1936 at 01:00** in **20.646 seconds**, with zero rejected
torn reads. Final state was **3 January at 05:00**, paused at speed 4. Slot 0's
signed engine counter increased from **101,000 to 303,000**; its units remain
unverified. Slots 1–3 stayed empty and completed technologies stayed at 47.
The exclusive lease was released and subsequent status showed zero remaining
time. Transport receipts still distinguish X11 delivery from game causality;
`game_action_verified` and `event_causality_verified` remain false.

The **311-second H.264 recording** finalized and passed ffprobe. One owned WM
close returned actual normal exit **0**, with no signal or core dump, in
**2.63 seconds**. Exact game/owner disappearance and terminal evidence were
checked. No command was replayed and no live binary was replaced.

The Windows capacity fixture initially failed because an append-only handle
cannot resize a file. A test-only writable, exclusively locked handle fixed
fixture preparation; production journal opening stayed unchanged. Source
`e8ec158a` then passed all three Rust CI jobs and all six Python CI jobs. Local
Rust tests, targeted MSRV stage tests, Clippy and **953 Python tests (37 skipped)**
also passed. The preceding Python workflow had five successful jobs and one
macOS runner cancellation before any steps; it was not a Python test failure.

This passes the narrow research/clock gate. Automatic research-slot navigation,
focus actions, daily plans, economy/army/naval commands, clean two-year acceptance
and the full Germany/China/Poland campaigns remain unfinished.


## Owned slot-selection fixture gate

The next source adds an ordinary `select_slot` stage to the research preview.
It requires an open own panel, canonical emptiness, no visible Info and the
independently proved slot-specific callback. Slot geometry uses the existing
Sprite resource/cache and typed reciprocal hierarchy reader; technology-node
observation retains its separate technology-data join. The internal slot adapter
returns the same witnessed body and campaign/slot context. Public diagnostics
keep all frame, routing and input-authority flags false.

The stage borrows the existing controller, lease, gate, durable journal and
final OS/canonical preflight. Success requires the exact empty own slot selected
in the ordinary tree and clear input. An uncertain selection blocks every new
stage and clock action across restart. Older receipts keep missing panel
visibility unknown; it cannot satisfy a selection precondition or receipt. Panel opening and category navigation remain unfinished.

Seven additional cases cover independent slot-vs-technology bindings, renderer
ownership, full-width orientation, same-reader context, late mutations, occupied
slots and restart/replay handling. A generated test's initial two-millisecond
budget expired before dispatch, correctly returning local refusal. Its fixture
budget was changed to one second to exercise uncertain post-dispatch recovery;
production deadlines stayed unchanged. Portable and Rust 1.86 suites each
passed **320 tests**; optimized actual Linux passed **381 reported checks**
(380 substantive plus one helper). Python passed **953 tests, 37 skipped**, in
20.129 seconds; host/Linux Clippy, MSRV Linux compilation and formatting passed.
These are fixture results. SelectSlot must pass a fresh recorded exclusive
Luna run before it can count as installed-game action validation.


## Recorded run 020: Luna selects research slots by ID

A fresh immutable game used source `552681ea` and CLI SHA-256
`f21d23c112f21e7029028b187518d8946ecb57c5e4d4927bbcf78049c4bb758b`.
Normal historical vanilla Germany began paused at 1 January 1936, with all four
slots empty. The builder prepared menus and opened the research panel once,
then relinquished input; it selected no slot or technology. Recording had a
verified first packet before the single campaign-start action.

One Luna player at medium reasoning made four single-stage requests with separate
durable IDs. SelectSlot 0 (**819 ms RPC**), View `support_weapons` (**659 ms**),
Research (**955 ms**) and SelectSlot 1 (**545 ms**) all returned `Verified`,
`current_state_verified=true` and `historical=false`. Separate UI and canonical
reads confirmed slot 0 researching the requested technology and slot 1 empty
and selected. No duplicate assignment, screenshot targeting, coordinates,
callback execution, native post or builder rescue occurred. These four samples
do not establish a general p50/p95 or upstream-command speedup.

Ordinary speed 4, unpause and terminal pause verified. One server wait reached
3 January at 01:00 in **20.348 seconds**, with zero rejected torn reads. Final
state was **3 January at 07:00**, paused at speed 4; the own raw research counter
increased from **101,000 to 303,000**. Other slots stayed empty and the completed
count stayed 47. Counter units and event causality remain unverified. Luna
released its lease and subsequent status showed zero remaining time.

The **528.333-second H.264 recording** finalized and passed ffprobe. One owned
WM close returned actual normal exit **0**, no signal/core dump, in **2.30 seconds**.
Exact game/owner disappearance and terminal evidence were checked, and both old
stdio clients closed. All three Rust and six Python CI jobs passed for the
recorded source. No live binary was replaced.

This validates own-slot selection and research assignment on the documented
Linux build/profile. Automatic panel/category navigation, focus actions, daily
plans, economic/army/naval commands, clean two-year acceptance and full campaigns
remain unfinished.


## Category-association reader: fixture checkpoint

A new read-only folder-ID query checks canonical folder definitions, the exact
reserved index-zero singleton, separate child-container/button name registries,
dense tab/container indices and the tab-specific observer capture. The selected
container and requested control must belong to the own registry. It reuses the
full own selected-slot/campaign reader and its final witnesses; it does not
switch a category or derive an input point. Unknown factory classes refuse.

Review corrected an unproved index-zero skip and the order of selected-container
ownership checks. Static tracing then sealed the singleton publication and its
exact primary type. A parent fixture exposed a long tab key beyond inline-string
capacity; its heap-string setup was corrected without changing production parsing.
Nine module cases, two same-context/late-mutation cases and one protocol-shape
case bring portable and MSRV suites to **332 passing tests**. Optimized actual
Linux passed **393 reported checks** (392 substantive plus one inert helper),
including five isolated X11 fixtures. Display ownership and ptrace policy were
unchanged, and the preceding owned game remained exited. Python passed **953
tests, 37 skipped**, in 20.323 seconds; host/Linux Clippy, MSRV Linux compilation
and formatting passed. Installed-game category validation is pending; current-node
membership alone never proves selected-category visibility.


## Panel-opening and category-geometry fixture gate

A fixed ordinary `W` stage now shares the research lease, mutex, durable journal
and final own-state/OS checks. It requires known closed own research views in
normal map mode, an empty requested slot and no Info. Contradictory mode/window
visibility refuses. An already open panel verifies without a toggle. Fresh panel
readback and clear input are required; uncertain delivery blocks all later
research stages and clock input across restart. The shortcut never moves the
pointer and exposes no arbitrary key argument.

An optional category Sprite diagnostic reuses the existing typed renderer,
resource/cache and bounded hierarchy reader only after complete own category
association proof. Exact Drag is supported; Standard geometry remains explicitly
unavailable. Default category responses and reads stay unchanged. Raw geometry
never supplies frame, routing or input authority.

Portable and Rust 1.86 suites each passed **345 tests**. Optimized actual Linux
passed **407 reported checks** (406 substantive plus one inert helper), including
unchanged-display isolated X11 fixtures that check the `W` keysym, matching key
pairs, clear keys and stationary pointer. Python passed **953 tests, 37 skipped**,
in 19.378 seconds. Host/Linux Clippy and MSRV Linux compilation passed. All three
Rust and six Python CI jobs passed for the preceding category commit `3e43476`.
Panel opening and category geometry still await installed-game validation; this
checkpoint does not validate category input or campaign acceptance.


## Run 021 canceled before campaign start; corrected observer gate

Review of prior own UI observations caught an unproved fixture assumption in
`cdf2ae8`: mode 0 was labeled the closed map context. Independent constructor,
getter and toggle tracing instead proves **23 is the no-managed-view sentinel**;
0 is a real view index. The future OpenPanel guard now requires 23 plus fresh
closed own views, no Info and canonical empty slot. It does not claim absence
of every modal or establish keyboard consumption from static routing.

Fresh immutable run 021 was canceled during frontend loading before campaign
start or input binding. One owned WM close returned actual normal exit **0**,
with no signal/core dump, in **1.97 seconds**. Exact game/owner disappearance was
checked. Its binary was never replaced, no Luna action ran, and this is no
installed-game OpenPanel acceptance. The assumption and cancellation evidence
are preserved privately. All nine CI jobs passed for its source.

The corrected source also adds a private category-point candidate using the
existing Sprite/codec maths and witnessed selected-slot emptiness. It has no
dispatch or input authority. Opt-in exact focus-node queries now expose separate
Standard Sprite/Text diagnostics after their own callback/owner checks. Standard
uses its independently proved renderer offset; the old Drag wrapper stays
unchanged. Default focus list/node/start metadata and reads stay unchanged.

Combined host and Rust 1.86 suites passed **358 tests**. Optimized actual Linux
passed **420 reported checks** (419 substantive plus one inert helper); ownership
and kernel policy stayed unchanged. Python passed **953 tests, 37 skipped**, in
19.994 seconds. Host/Linux Clippy and MSRV Linux compilation passed. Corrected
panel opening, category geometry/selection and focus geometry remain pending
installed-game validation; focus actions and daily plans remain unfinished.


## Recorded run 022: panel opening and no-input completion

Fresh immutable source `d7f9d8b` / CLI SHA-256
`7fa7b3b6786a7bf9084aeee434cd258447ea8b5a1510e91d37865e90f33ba43a`
started Normal historical vanilla Germany, paused at 1 January 1936. Recording
had a verified first packet before one Start. The builder prepared menus only;
fresh native reads confirmed mode 23 with closed own research views and no Info.
It opened no panel or slot. One Luna medium player then owned input exclusively.

OpenPanel (**468 ms RPC**), SelectSlot 0 (**503 ms**), View support weapons
(**624 ms**), Research (**700 ms**) and SelectSlot 1 (**528 ms**) returned Verified
with current own-state readback. A distinct OpenPanel repeat took **68 ms**,
returned delivery null and verified the already open state without a second key.
All six were fresh completions, not historical receipt replays. Causality flags
remain false; these single samples do not establish general p50/p95 performance.

Read-only industry/electronics category queries both verified associations with
**Standard** controls, selected infantry folder/index0, and unavailable geometry.
No category was switched, and no unsupported fields were guessed. The next
reader now reuses the separately proved Standard Sprite path and local disabled
state; its new geometry still needs installed-game validation.

Speed4, unpause, one wait and terminal pause verified. The wait reached
3 January at 01:00 in **16.860 seconds**, with zero torn reads; final state was
3 January at 20:00, paused at speed 4. Own raw research rose **101,000→303,000**
and the completed count stayed 47; counter units remain unverified. A tokenless
lease-release request was rejected before mutation; the corrected release and
status confirmed zero remaining time. This validation-only argument error is
preserved with the actual calls.

The **530-second H.264 recording** passed ffprobe. One owned WM close returned
actual normal exit **0**, no signal/core dump, in **2.18 seconds**. Exact game/owner
disappearance was checked and both stdio clients closed. All nine CI jobs passed
for the recorded source. No live binary was replaced or failed input replayed.

This passes bounded panel/slot/research control, not clean two-year acceptance
or a full campaign. Category input, focus actions, daily plans, economic/army/
naval commands and Windows/macOS native live adapters remain unfinished.

## Durable category-selection fixture gate

The new explicit folder-ID stage shares the existing controller, lease, journal
and one ordinary point dispatcher. Old receipt fields default unknown. It reads
full newer own UI/Info and selected-slot emptiness under final witnesses, then
rejects contradictions with canonical own assignments; older empty/no-Info
evidence cannot overwrite newer occupancy or Info. Separate snapshots still
do not supply atomicity, frame or event-causality proof.

Exact Standard tabs use their own renderer/state fields; unknown/disabled states
refuse candidates, while Drag never borrows those fields. A review caught the
initial stale-evidence gap and fixtures now prove zero dispatch for newer Info
and occupancy. Combined host and Rust 1.86 suites passed **368 tests**; optimized
actual Linux passed **430 reported checks** (429 substantive plus one inert
helper). Python passed **953 tests, 37 skipped**, in 20.024 seconds. Host/Linux
Clippy and MSRV Linux compilation passed. These are fixture results; category
selection and Standard geometry await a fresh exclusive Luna run.

## Recorded run 023: category success and verification deadline failure

Immutable source `9599e7f` / CLI SHA-256
`611db16b1ea1d181f02d99161a398ddce603876f230b3a517c28578bec6f60dc`
started Normal historical vanilla Germany with no mods or DLC. A first recording
packet was verified before one Start. The builder prepared menus only; native
reads confirmed 1 January 1936 at 12:00, paused, all four slots empty and own
research views closed in mode 23. Loading took several minutes; one read-only
capture and its copy timed out. Start was never retried.

One exclusive Luna medium player verified OpenPanel (**745 ms RPC**), SelectSlot0
(**1,119 ms**), View support weapons (**1,864 ms**) and Research (**2,800 ms**).
Industry/electronics reads verified Standard fields, callback associations and
the bounded cache/hierarchy diagnostics. Raw frame/routing/input-authority flags
remained false. Luna initially skipped category input based on these flags; that
phase is preserved separately, including the contract clarification.

Speed4 and unpause verified, but a Jan5 wait ended at its **50.003-second wall
deadline**, still on Jan1 at 20:00, with zero torn reads. Terminal pause and
overview showed Jan1 at 21:00. Support's raw counter remained **101,000** and the
completed count stayed 47; units are unverified. This is no multi-day acceptance.
The lease was released and status confirmed zero remaining time.

A separately authorized second phase used the same player, immutable binary,
paused campaign and recording. No category input had previously failed or been
attempted. A fresh lease and own-state preflight preceded SelectSlot1
(**3,500 ms**), industry-folder selection (**3,667 ms**) and a distinct same-folder
request (**453 ms**). All three returned fresh Verified state. The repeat had
delivery null and sent no input; causality flags remained false.

The single electronics-folder attempt then returned `action_uncertain`:
**Readback completed after bounded deadline**, at **5,139 ms RPC**. All further
input stopped. No retry, replacement token or post-failure game observation was
used to convert this into success. The resulting folder remains unknown; its
durable Intent is preserved. The lease was released and status again showed zero.
These samples are not general p50/p95 estimates or an upstream speed comparison.

Both H.264 segments finalized and passed ffprobe: **600.000 + 433.333 seconds**,
**10,908,339 bytes**, 512×288 at 3 fps. One owned WM close returned actual normal
exit **0**, no signal/core dump, in **3.45 seconds**. Exact game and owner
disappearance, current lease proof and unchanged Yama1 were checked; both stdio
clients closed. All nine CI jobs had passed for this recorded source. No live
binary was replaced.

Industry selection/no-input completion is corroborated on this exact profile.
Reliable complete category navigation remains unfinished. Focus actions, daily
plans, economics/army/naval commands, Windows/macOS native live adapters, clean
two-year acceptance and full campaigns remain open gates.

## Unified category reader and focus prerequisites: fixture checkpoint

Category verification now obtains the complete canonical own research model and
full category/UI body under one Reader and final witness set. It retains all
completed-membership, active-slot, canonical-key, owner/type/root and repeated
field checks. Default public research/UI/category outputs and read paths stay
unchanged. Generated late Info, slot and root mutations refuse with zero stage
dispatch. The deadline remains five seconds; run 023's uncertainty is not erased.

The exact former four-body sequence measured **1,649 logical reads**, versus
**684** for the unified bracket, a **58.5% reduction** on the same generated
fixture. These are logical requests, including final witnesses, rather than
kernel syscall counts or live latency. The parent also checks against a
conservative public-path baseline. Installed-game verification of the optimization
remains pending.

New private focus preparation retains the canonical own catalog, active-progress
owner, campaign and selected-detail witnesses. Producer-only facts cover the
known managed mode, typed own root/container flags and complete own focus-detail
and research-Info local blockers. Unknown/foreign scope refuses; no selected
foreign technology payload or global modal-clear claim is introduced. Sprite
points share existing maths. Hidden nonmatching detail bodies remain omitted;
complete producer counts supply their local blocker facts. No focus input tool
or dispatcher is enabled.

Combined portable and Rust 1.86 suites passed **396 tests**. Optimized actual
Linux passed **458 reported checks** (457 substantive plus one inert helper),
including five isolated X11 cases. The nested helper print is not counted again.
The preceding exact game/owner remained exited; display ownership and Yama1
stayed unchanged. Python passed **953 tests, 37 skipped**, in 22.830 seconds.
Host/Linux Clippy, MSRV Linux compilation, formatting and diff checks passed.
These are fixture/kernel checks; focus actions, a reliable repeated category
cycle and campaign acceptance remain unfinished.

## Recorded run 024: category cycle and four assignments

Immutable source `a8dd857` / CLI SHA-256
`1b527594f9ab09e1740aac3afe9047d8cccb89aaa61f5711507e8518a5eee73d`
started Normal historical vanilla Germany, no mods or DLC. Recording had a
verified first packet before one Start. A builder menu command reached its SSH
deadline during loading; its delivery remains unknown and it was not replayed.
Fresh native reads confirmed Germany, Jan1 at 12:00, paused, all four slots
empty, focus null, mode23 and closed own research views before input binding.

One exclusive Luna medium player completed **20 fresh Verified research stages**.
Industry, a distinct same-folder no-input completion, electronics and infantry
passed in **2,917 / 261 / 2,473 / 1,029 ms RPC**. Later industry/electronics
switches passed in **877 / 798 / 819 ms**. All four assignments were verified:
support weapons, basic machine tools, electronic mechanical engineering and
construction1. Event-causality and game-action flags remain false. These samples
validate this exact bounded run, not general p50/p95 speed or an upstream
performance advantage.

The speed4 request then returned `action_uncertain` in **5,149 ms**:
`Delivered key had no target witness; never resend blindly`. No further gameplay
input, clock retry, wait, pause or observation request followed. Required lease
release/status confirmed zero remaining time. Final clock state remains unknown;
the unresolved durable Dispatching receipt is preserved. No elapsed campaign
progress, clean two-year run or full victory is established.

Both H.264 segments finalized and passed ffprobe: **600.000 + 347.666 seconds**,
**10,402,449 bytes**, 512×288. One owned WM close returned actual normal exit
**0**, no signal/core dump, in **3.056 seconds**. Both exact game and owner were
absent afterward, Yama remained1, journals and lease proof were preserved, and
both clients closed. The binary was never replaced during the run.

Category navigation and four-slot assignment passed here. Reliable clock input,
live focus actions, daily execution, economic/army/naval commands, Windows/macOS
native adapters and the required campaigns remain open gates.

## Focus and daily-plan library fixture checkpoint

The new own-politics observer validates the positive own-country accessor,
complete paired child/button registries and exact ordinary politics-to-focus
callback before optional renderer diagnostics. Fixed Q transport means politics,
not direct focus. The focus-stage library borrows input authority, persists
intent before one dispatch and quarantines unresolved input across stages and
restart; no live backend or MCP focus action is registered. Positive local
hidden/shown facts require the literal flag plus both transition channels at0.
Transition1 or unknown values cannot supply readiness or clear blocker counts.

The daily-plan core retains typed entries, executes at most once per advancing
verified day, persists intent atomically, checks shared uncertainty and accepts
only matching fresh completion receipts. Its research converter excludes
panel/slot/category/view receipts and historical replay. The owned worker,
shared-controller wiring, safe plan updates and focus/economy/army/naval receipt
adapters remain unfinished. Non-Unix storage explicitly reports unavailable.
Typed future plan fields are not implemented gameplay commands.

Host stable and Rust1.86 suites passed **444 tests**. Optimized actual Linux
passed **506 reported checks** (505 substantive plus one inert helper), including
five isolated X11 cases with Q press/release and stationary pointer. The nested
helper print is not counted twice. Python passed **953 tests, 37 skipped**, in
20.054 seconds. Host/Linux Clippy, MSRV Linux compilation and formatting passed.
The previous exact game/owner remained absent and Yama1/display ownership stayed
unchanged. These are offline/kernel fixtures, not focus or governor live evidence.

The first Linux bundle failed one authored atomic-replacement assertion because
ext4 reused an earlier unlinked inode across multiple replacements. The corrected
fixture pins the original file open, then verifies a distinct current inode,
unchanged complete old-reader bytes and changed complete current bytes. Production
storage guards were not relaxed. Initial failing logs and the original immutable
bundle remain preserved privately.

## Fixed clock budgets and Linux executable readiness

Clock operations now retain one ten-second budget from call entry, capped by the
original lease. Each delivered key retains the existing 1.5-second witness budget.
Deadline checks run before and after observations, before final queueing and
before accepting a target. Blocking reads cannot be forcibly interrupted, but a
late result cannot authorize input or success. Required intermediate speed
witnesses remain unchanged. This does not explain run024's missing witness or
establish reliable gameplay control.

The first kernel fixture bundle failed executable readiness on an immediate
`/proc/<pid>/exe` NotFound during a generated threaded exec. The owned child was
preserved through its natural exit; original logs and bundle remain private.
An isolated tighter sampler subsequently reproduced genuine readlink/metadata
NotFound transitions. The corrected readiness path pins parent, user and creation
independently of the image, retries only those two executable-image NotFound
operations and requires two matching path/device/inode observations before
publishing identity. The original two-second deadline remains fixed. Permission
errors, other I/O failures, stale ownership and late results refuse.

The corrected optimized bundle passed **527 reported checks** on the actual Linux
kernel: **525 substantive plus two inert helpers**, including five isolated X11
cases. Nested helper output is not counted again. The previous exact game and
owner remained absent; display ownership and Yama1 stayed unchanged. Host stable
and Rust1.86 suites each passed **444 tests**; Python passed **953 tests, 37
skipped**, in 19.814 seconds. Host/Linux Clippy, Rust1.86 Linux compilation and
formatting passed. The research-stage timeout fixture now consumes its finite
deadline after recording dispatch, avoiding a CI scheduling-dependent pre-input
refusal; production research deadlines remain unchanged.

These are fixture checks. A fresh immutable Luna run is still needed for the
clock changes; focus actions, governor execution and full campaigns remain
unaccepted.

CI for `152bdf3` passed all six Python jobs and the Linux/macOS Rust jobs.
Windows passed the corrected timeout test but exposed the same pre-dispatch
scheduling assumption in the category restart fixture: its 20 ms budget expired
before the expected input. The follow-up fixture uses the same finite
post-dispatch expiration mechanism. Its 21 research-stage checks passed on host
stable and Rust1.86; Python passed 953 tests, 37 skipped. Production code and the
running immutable game binary were unchanged. Windows CI verification remains
pending for this follow-up.

The follow-up `aa94c9e` subsequently passed **all nine CI jobs**: Rust on Linux,
Windows and macOS, plus Python 3.9/3.13 on each platform. This verifies shared code
and offline fixtures, not Windows/macOS native gameplay adapters.

## Recorded run 025: clock and four-slot research passed

Immutable source `152bdf3` / CLI SHA-256
`5b605702768bccbec06e20582f26128d503c59dbab61c5044cfb7eabf03445b6`
launched the exact licensed Linux build with no mods or DLC. Startup reported
427,730 ms in the x86 emulated guest. One initial read-only client initialize
preceded endpoint creation and returned NotFound; a fresh client connected after
independent ownership verification. No gameplay input was involved or replayed.

Recording's first packet was verified before one campaign Start. Native reads
confirmed Normal historical Germany, 1 January at 12:00, paused speed0, focus null,
four empty research slots and closed own research views in mode23. One exclusive
Luna medium player then passed speed4 (**2,206 ms RPC**), unpause (**304 ms**),
a server wait to 3 January at 01:00 (**36.971 seconds**, zero torn reads), and
pause (**1,013 ms**), before opening research.

All **17 research stages** returned fresh Verified receipts. The four canonical
assignments were support weapons, basic machine tools, electronic mechanical
engineering and construction1. Stage RPC samples ranged **101–1,560 ms**;
the fastest was an already-selected category with no input. These are individual
samples, not a general performance distribution or matched upstream comparison.
Event-causality and transport game-action flags remain false; canonical current
state was verified by the stage adapter.

With the research panel still open, unpause, a wait to 5 January and pause also
passed. One torn read was rejected during that wait. Final own overview was
**1936.01.05.11, paused, speed4**. Each assigned raw research counter increased
from **303,000 to 505,000**; units remain unknown, and completed count stayed 47.
Release/status confirmed zero remaining lease time. No game screenshots, map
movement or root/helper input occurred during Luna's gameplay ownership.

The H.264 recording finalized and passed ffprobe: **452.666 seconds**,
**7,040,840 bytes**, 512×288 at 3fps. One owned WM close returned actual normal
exit **0**, no signal/core dump, in **2.515 seconds**. Both exact game and owner
were absent afterward; Yama1 and display ownership stayed unchanged. Journals,
release proof, recordings and sanitized calls remain private. Both clients closed.

This successful bounded Rust run does not explain run024's missing clock witness.
No clean two-year run, live focus/governor acceptance or full campaign has passed.

## Server-wait deadline regression

Wait observations now check the original budget after a successful read as well
as before starting it. A snapshot completed at or after the deadline returns
`unstable_observation`; it cannot report a reached target. Torn-read retries and
their sleeps consume the same budget. Ownership errors remain immediate and
are never retried. This does not forcibly interrupt a blocking OS read, pause the
game or introduce any input.

Four deterministic clock-injection regressions cover late success, capped retry
sleep, timely recovery and preservation of a late ownership error. The complete
host Rust suite passed **448 tests**; all seven protocol-unit checks passed on
Rust1.86. The optimized actual Linux bundle passed **531 reported checks**
(529 substantive plus two inert helpers), including five isolated X11 cases.
Nested helper output is not counted again. Source hashes matched all76 manifest
files; the previous game/owner remained exited and display ownership/Yama1 were
unchanged. Host/Linux Clippy, Rust1.86 Linux compilation and formatting passed.
The local Python working-tree suite passed953 tests,37 skipped, in19.305 seconds;
that checkout includes preserved Python WIP outside this Rust change.

These are offline/kernel regression checks. The new wait behavior has not yet
been exercised in a fresh installed-game run. Focus/governor service integration
and the full campaign gates remain unfinished.

## Focus service and prepared-research daily worker

The owned binding now separately enables ordinary focus stages and a daily
prepared-research worker. Both default off. Focus stages use settled own roots,
canonical ID joins, complete local blocker facts and supported Sprite geometry;
they share the existing lease, controller and uncertainty journals. Local facts
do not establish global modal clearance, current-frame or routing authority.
Unsupported renderers and unknown readiness refuse.

The worker uses the same read/controller gates and a weak owner reference.
Shutdown joins outside the controller; last-owner drop cannot self-join. It
probes unchanged days without rewriting the plan, persists observation sequence
before publication and executes only on advancing verified days. Research can
run only when the exact own slot/Info is already prepared, or its canonical
assignment already matches. Navigation receipts cannot finish an entry. Other
domains remain retained as unavailable. Lease renewal and automatic navigation
are not implemented. A failed daily witness after Research verification publishes
quarantine while the common controller is still held, preventing manual input
from entering that gap.

The plan-update library preserves every existing entry specification, attempts,
completion evidence and chronology. Changes require an increased revision;
identical retries do nothing. Historical requests retain their original revision
when reopened. Uncertainty, lost storage ownership, replacement/deletion and
forged revision/hash combinations refuse. Service update wiring is still pending.

Rust1.86 host passed **472 tests**. Host/Linux Clippy, Rust1.86 Linux compilation,
optimized cross-linking and formatting passed. The immutable optimized bundle
passed **560 reported checks** on the actual Linux kernel (**558 substantive plus
two inert helpers**), including five isolated X11 cases. Nested helper output is
not counted twice. All83 source files have a private hash manifest. The previous
owned game/owner remained exited; display identity and Yama1 stayed unchanged.
The local Python working-tree suite passed **953 tests, 37 skipped**, in22.278
seconds, including preserved Python WIP outside this Rust change.

An authored plan-update test initially expected identical data-file replacement
to fail. Atomic data replacement is allowed; storage ownership is pinned through
the private lock and parent. The corrected test displaces the pinned lock inode
and verifies refusal. Initial failure evidence is preserved; production storage
guards were unchanged. The focus timeout fixture now expires its finite budget
after dispatch is recorded, avoiding scheduling-dependent pre-input refusal.
Production focus deadlines are unchanged.

These are fixture/kernel results. Focus dispatch and the daily worker have not
yet passed an installed-game run. Full daily automation, Windows/macOS native
adapters, clean two-year acceptance and all full campaigns remain unfinished.

All nine CI jobs subsequently passed at `5aaceba`: three native Rust platforms
and Python3.9/3.13 on each platform. These remain offline results.

### Daily quarantine acquisition race

Review found that publishing daily quarantine while holding the controller was
insufficient: a manual call could read the flag before publication, then acquire
the controller after the worker released it. Clock, research, focus and worker
start now check uncertainty again after acquiring the common controller. A
deterministic channel/barrier regression reproduces publication between those
steps and verifies refusal without manual work. Busy acquisition remains
nonblocking; a poisoned controller reports uncertainty.

Rust1.86 host passed **474 tests**; host/Linux Clippy, Rust1.86 Linux compilation,
formatting and the local Python suite (**953 tests, 37 skipped**,22.539 seconds)
passed. Actual Linux fixtures for this correction are pending. The already
launched run026 keeps its original immutable `5aaceba` binary and has the daily
worker disabled before input binding. It will test focus/clock only; worker
validation requires a separate fresh binary and game. No hot-swap or campaign
acceptance is claimed.

## Recorded run026: focus opening passed; selection refused

Immutable source `5aaceba` / CLI SHA-256
`d188f8a4fa6b7dc3751cbc8d53583d3db788b2ef08ac9e81371e36b6275ee17d`
started Normal historical vanilla Germany, no mods or DLC. Recording's first
packet was verified before one campaign Start. Fresh own reads confirmed
1 January1936 at12:00, paused speed0, four empty research slots and no active
focus. The daily worker was disabled before input binding because the subsequent
quarantine correction was not this running binary.

One exclusive Luna medium player verified ordinary **OpenPolitics** in
**1,482 ms RPC** and named-button **OpenFocus** in **5,139 ms**. Readback confirmed
the corresponding managed mode and settled own politics/focus roots. The exact
own key was `GER_remilitarize_the_rhineland`; `rhineland` returned zero because
catalog filtering is by prefix.

**View** then refused in **257 ms** with `research_geometry_unavailable`:
`Owned graphics root closure required`. No focus-node click, Start, clock action,
worker start or further game observation followed. Final game state after the
error remains unknown. Saved calls contain no deep hierarchy result, so the
specific missing closure reason is not established. Bounds and unknown-state
guards were not relaxed. Lease release/status confirmed zero remaining time.

The H.264 recording finalized and passed ffprobe: **250.333 seconds**,
**3,033,764 bytes**,512×288 at3fps. One owned WM close returned actual normal
exit **0**, no signal/core dump, in **3.200 seconds**. Both exact game and owner
were absent afterward; Yama1 stayed unchanged. Journals, release proof,
sanitized calls and terminal evidence remain private; both clients closed.

Opening focus passed in this bounded trial. Focus selection/start, worker live
acceptance, the clean two-year run and full campaigns remain unfinished.

## Navigation and renderer expansion: kernel gate pending

The next composed source adds separate durable research preparation stages,
explicit optional folder hints and idempotent plan updates. Navigation history
stores and validates full entry/target/index/day bindings, chronology and matching
terminal receipts. Stored receipts remain historical and cannot complete a goal
or replace current UI observations. Persistence failures quarantine the navigator.
Research completion remains at most once per advancing day. Other daily domains
and arbitrary panel/Info closing remain unavailable.

Focus point preparation adds distinct guarded zero-angle Text and unmasked Frame
branches; unknown renderers, incomplete hierarchy and unproved routing still
refuse. These renderer fixtures do not explain run026's missing root closure.

The composed Rust1.86 host suite passed **497 tests**; host/Linux Clippy,
Rust1.86 Linux compilation, formatting and optimized cross-linking passed. Local
Python passed **953 tests,37 skipped**, in22.311 seconds, with preserved Python
WIP. The first immutable actual Linux bundle failed the existing threaded-exec
readiness fixture on `ownership_changed` after genuine executable-link NotFound
observations. The suite stopped after its library gate; remaining executables
were not run. The original bundle and failure logs are preserved privately.

Test-only ownership-sample logging is under investigation. The isolated diagnostic
passed12 generated execs and observed12 genuine NotFound errors, without an
ownership refusal. This does not explain the earlier failure. No production
ownership guard has been relaxed; the expanded runtime is not yet cleared for
a fresh worker trial.

The instrumented full-library diagnostic initially omitted the required fixture
CLI environment and failed only reconnect setup. That runner error is preserved;
it is not a production reconnect regression. A new complete immutable bundle
with its verified CLI environment then passed **585 reported checks** on the
actual Linux kernel (**583 substantive plus two inert helpers**), including five
isolated X11 cases. Display identity/Yama1 stayed unchanged and run026 remained
exited. Test-only logging did not reproduce the original ownership refusal;
that first failure remains unexplained. No production guard was relaxed.

Review independently found that point conversion labeled either closed graphics
root as root1f0, although equivalence of root350 coordinates has not been proved.
An explicit root-scope witness and refusal of root350 point conversion are being
prepared. This is not established as run026's cause: its closure flag was false.
Do not begin the next worker/focus trial until this correction is composed and
verified in a fresh immutable binary.


## Durable same-day research continuation and root scope

The next composed source can prepare and assign several own research slots on
one observed day, within the explicit assignment budget. Each unique persisted
goal intent counts once, including known NotQueued and unresolved reservations;
terminal records do not double count. Navigation uses separate verified intents
and does not consume assignment reservations. The worker does not pause, renew
its lease, bundle a UI chain or replace unfinished entry specifications. A local
refusal backs off that entry/day; uncertainty still quarantines shared input.

New goal request identities bind their reservation day. Reopening validates
chronology, unique identities and matching terminal transitions. Legacy unbound
history conservatively exhausts its recorded day until a fresh advancing day.
Updates and restarts preserve usage; raising the limit exposes only the remaining
difference. Digests label prior batches/preparation as historical, return the
current revision and fresh counts, and include the remaining assignment budget.
Only Research has an automatic completion adapter; other domains remain pending.

Point conversion now consumes the actual terminal hierarchy root role. A closed
root350 remains diagnostic and cannot be mislabeled root1f0 or authorize its
coordinate conversion. Existing type, traversal, depth and capacity bounds are
unchanged. Producer/consumer fixtures cover both roots and torn or unsupported
hierarchies. Bounded closure diagnostics retain the reason without raw pointers.
This independently found correction does not explain run026's closure refusal.

The composed Rust1.86 host suite passed **512 tests**. Host/Linux Clippy with
warnings denied, Rust1.86 Linux compilation, formatting and optimized cross-link
passed. Local Python passed **953 tests,37 skipped**, in24.306 seconds, including
preserved Python WIP. The fresh immutable full Linux-kernel bundle passed **602 reported checks**
(**600 substantive plus two inert helpers**), including five isolated X11 cases.
Nested helper output is not counted again. All86 source files are hashed privately;
Yama1 and display ownership remained unchanged and run026 stayed exited. Earlier
exec ownership failure remains unexplained; this run did not reproduce it. No
production guard was relaxed. A fresh recorded worker trial is next; these
fixture results do not establish installed-game or campaign acceptance.


## Recorded run027: worker preparation passed; bad hint refused

Immutable source `371e677` / CLI SHA-256
`2e6d3ded36baedebda84459e2089c8f21bf237af61e7bdc3b31323fd25d06645`
started Normal historical vanilla Germany, no mods or DLC. Recording's first
packet was verified before one Start. Fresh own state confirmed1January1936
at12:00, paused speed0, four empty slots and no active focus.

One exclusive Luna medium player verified speed4 while paused (**1,851 ms RPC**)
and started research plan revision1 (**223 ms**). The parent supplied incorrect
folder hints: `infantry` and `industry`, rather than the canonical
`infantry_folder` and `industry_folder` already verified in run025. This is a
builder test-setup error. Stored navigation receipts verify worker-only
**OpenPanel** and **SelectSlot**; they are preparation, not goal completion.

Three in-run digests refused `input_busy`; two intervening50-second server waits
still observed all four slots empty on the initial paused date. Luna stopped the
worker (**12 ms**, uncertainty retained false), released the lease and confirmed
remaining0. No update, restart, assignment or time advancement was attempted.
The parent made no game reads/input during Luna's ownership. After release, one
read-only diagnostic digest succeeded in74ms: last_error `research_precondition`,
current revision1, idle4, assignment attempts0/remaining4 and no quarantine.
The exact deeper association error was not retained; wrong hints are concrete
setup evidence, not proof that every runtime failure has been explained.

The H.264 recording finalized and passed ffprobe: **591.666 seconds**,
**6,960,787 bytes**,512×288 at3fps. One owned WM close returned actual normal
exit **0**, no signal/core dump, in **3.382 seconds**. Both exact game and owner
were absent afterward; Yama1 remained unchanged. Private receipts, journals,
release proof, calls and footage are preserved; both clients closed.

Worker preparation passed two stages. Same-day refill and campaign acceptance
remain unpassed. Correct the public setup example and next brief; add bounded
preparation-refusal reporting/backoff and read-only digest contention handling.
Do not replace the persisted bad specification or count a fresh trial as success
of this run. Future worker terminal-stop code remains a separate tested candidate.


## Public folder validation and worker refusal/scheduling correction

The read-only `hoi4_research_folders` tool now returns exact sorted loaded public
keys with fresh own context, without any open GUI dependency. It reuses the
existing canonical database/sentinel/type/key traversal and final witnesses;
no new offsets, technology mapping, eligibility or input authority are claimed.
Start/update compare explicit hints against this owned catalog before plan
persistence, rejecting labels/aliases. The setup example uses `infantry_folder`.

Known logical read-only preparation refusals are persisted by full entry/day
without a new navigation intent or goal-budget reservation. Their actual
control-free160-byte reason remains historical through success/restart and is
reported by digest/stop. Unknown delivery, I/O, ownership and torn reads retain
distinct handling; no hint is silently changed. Other entries can be considered,
but unsupported transitions may still refuse.

Digest now acquires the governor gate before producer, preventing observation
from making a required post-dispatch witness busy. Only counter acquisition may
wait within the original absolute two-second deadline; no body/input retry,
pause, stale cache or renewal occurs. Mutex poison reports quarantine. An idle
worker checks authority and stops on explicit ownership/session loss or expiry;
possible input takes precedence and quarantines. `running` is lifecycle only.
Generic I/O is not guessed into ownership loss. Stop drains in-flight work.

The composed Rust1.86 host suite passed **536 tests**. Host/Linux Clippy with
warnings denied, Rust1.86 Linux compilation, formatting and optimized linking
passed. Local Python passed **953 tests,37 skipped**, in21.090 seconds, including
preserved WIP. The immutable Linux bundle passed **630 reported checks**
(**628 substantive plus two inert helpers**), including five isolated X11 cases.
Nested helper output is not counted again. All88 source files are privately
hashed; prior owned processes remained gone and display identity/Yama1 were
unchanged. These are fixture/kernel results; fresh installed-game catalog and
worker refill acceptance are pending. Earlier failures remain preserved.


## Recorded run028: catalog, update and two automatic assignments passed

Immutable source `3297ce3` / CLI SHA-256
`f57cb93275499c288d6c1c8e71e2f9950693f36ffb70acb56a45a3912647711e`
ran Normal historical vanilla Germany, no mods or DLC. Recording's first packet
was verified before one Start; own state confirmed1January1936 at12:00,
paused speed0, four empty slots and no focus.

One exclusive Luna medium player queried the new catalog while the research
views were closed. It returned **16 exact public folder IDs in49ms RPC**.
Worker start revision1 passed in267ms. Worker-only navigation and Research
assigned **basic_machine_tools to slot1**, then after revision2 added entries,
**electronic_mechanical_engineering to slot2**, on the same paused day. Fresh
canonical reads and durable goal receipts matched; budget used2 of4.
Update passed in131ms; identical retry returned updated:false in115ms without
resetting consumption. Digest revision2 remained current.

Full refill did not pass. Slot0 SelectSlot and slot3 SelectCategory navigation
records ended terminalfalse, with no matching NotQueued Research-journal receipt.
Their original returned errors were not persisted. Later probes recorded
`input_busy: Daily producer currently reading`, masking the earlier stage reason.
No pre-navigation refusal records or quarantine were present. These facts do
not establish the exact failed-stage cause. Pending entries and unused budget
were preserved. No restart or clock-advance test followed.

Luna stopped the worker in34ms (no time change, uncertainty retained false),
released the lease and confirmed remaining0. The parent made no game reads or
inputs during player ownership. The original release proof used a nested release
reply and extra player metadata; a private cleanup-validator shape mismatch was
corrected by checking exact game identity plus actual returned release/status.
The original proof remained unchanged; no gameplay action was repeated.

Both H.264 segments finalized and passed ffprobe: **848.333 seconds total**,
**10,599,232 bytes**,512×288 at3fps. One owned WM close returned actual normal
exit **0**, no signal/core dump, in **3.247 seconds**. Both exact game and owner
were absent afterward; Yama1 stayed unchanged and both clients closed. Private
journals, calls, proof, recordings and the initial validator error are preserved.

Catalog discovery, idempotent plan update and two worker-only assignments passed
this bounded trial. Four-slot refill, clean two-year play and full campaigns
remain unpassed. Next retain known nonqueued stage code/reason durably and permit
only bounded proven-no-input transient retries; never retry uncertain delivery.

## Navigation failure evidence and bounded retry correction

The worker now stores the actual immediate stage error, bounded reason and
delivery evidence in the durable navigation record. Digest and stop expose
`last_stage_failure` independently of later probe errors. Legacy terminalfalse
records retain null cause and cannot authorize a retry.

Only an immediate `input_busy` error proved to precede the research journal's
Intent can retry automatically. The journal dispatches only after durable Intent;
post-dispatch read errors retain that Intent and become action_uncertain. A
matching NotQueued receipt also proves no input, but does not authorize this
transient retry policy. Unknown evidence stays unresolved and quarantines.

Retry requires the same full entry/day and exact target, fresh unchanged UI,
valid ownership/lease and shared input checks. Persisted one/two-second backoff
permits at most three proven-busy failures total, or two automatic retries;
changing targets, updating the plan or restarting cannot reset that allowance.
Permanent failures back off for the day. Assignment budget and unfinished goals
remain unchanged. These changes require a fresh immutable live trial; run028's
missing original errors remain unexplained.

The combined Rust1.86 host suite passed **541 tests**; host/Linux Clippy with
warnings denied, final Rust1.86 Linux compilation, formatting and optimized
linking passed. Python passed **953 tests,37 skipped**, in22.460 seconds, with
unrelated WIP preserved. The immutable actual-Linux bundle passed **636 reported
checks** (**634 substantive plus two inert helpers**), including five isolated
X11 cases. Nested helper results are not counted twice. Display ownership and
Yama1 were unchanged, and run028's exact processes remained gone. These fixture
results do not establish live refill or campaign acceptance.

## Recorded run029: failure evidence passed; refill failed

Immutable source `e4a9a8d`, CLI SHA-256
`124e7e33d68c51901a25523d39796d1e472e18c2172ded0ef779ab1e19d31cd3`
ran Normal historical vanilla Germany with no mods or DLC. The first video
packet was verified before one Start; own memory confirmed paused noon on
1January1936, speed0, four empty slots and no focus. One exclusive Luna medium
player owned all gameplay input. Focus was disabled.

Catalog discovery passed in53ms RPC. Worker start took259ms; it verified
OpenPanel and SelectSlot0, then refused support's View before any research
Intent: `torn_read`, "Research UI witness changed", known_no_input:true,
retry_count0. The specific changed witness remains unidentified. The policy
retries only pre-intent input_busy, so this failure did not retry. The original
code/reason survived both later probes and plan revision2 in last_stage_failure.

Tools, electronics and construction were durably refused before navigation with
research_precondition, "Unknown/other own UI must be resolved; no blind navigation".
Update passed in131ms; identical retry returned updated:false in112ms without
resetting consumption. Both50-second waits independently confirmed all four
slots empty, same paused day at speed4; assignment budget remained0/4. Neither
restart nor clock advancement was attempted. Stop passed in12ms, with no time
change or retained uncertainty; canonical lease release/status proved remaining0.

After the completed handoff, one parent read-only UI query passed in60ms: mode6,
own tree visible, panel hidden, selected slot0, no Info. Source review confirms
the selector lacks a return-to-panel transition for another pending slot and
SelectSlot requires the panel. This explains that transition gap; it does not
identify the earlier changing witness. No parent gameplay rescue occurred.

Both H.264 segments finalized and passed ffprobe: **664.333 seconds**,
**7,714,883 bytes**,512×288 at3fps. One owned WM close returned actual normal
exit **0**, no signal/core dump, in **2.912 seconds**. Both exact process paths
were absent afterward, Yama1 was unchanged and both clients closed. Calls,
journals, proof and recordings remain private.

Durable error reporting and idempotent updates passed; four-slot refill and
campaign acceptance remain unpassed. Next bound retries of proved pre-intent
transient torn reads without weakening witnesses, and explicitly report the
missing return-to-panel route. New input transitions need separate live proof.

## Bounded pre-intent transient follow-up

New BeforeIntent input_busy and torn_read failures now share the same full
entry/day allowance: three failures total, at most two automatic retries, with
the existing persisted one/two-second backoff, exact target and fresh scope,
lease/UI checks. Alternating error codes cannot reset consumption. Final
witnesses are unchanged; unknown/post-dispatch or confirmed NotQueued delivery,
ownership errors and permanent geometry failures never authorize this retry.

Run029's previously stored torn_read with zero allowance stays nonretryable;
neither migration nor restart manufactures permission. Legacy missing causes
remain unknown. The known own open-tree/different-empty-slot case now reports
capability_unavailable with the concrete return-to-panel-not-implemented reason.
Contradictory or other UI states retain their original refusal. No new keyboard
or pointer transition is enabled. Fresh installed-game validation is required.

The combined Rust1.86 host suite passed **545 tests**; host/Linux Clippy,
Rust1.86 Linux compilation, formatting and optimized linking passed. Python
passed **953 tests,37 skipped**, in22.125 seconds. The immutable actual-Linux
bundle passed **640 reported checks** (**638 substantive plus two inert helpers**),
including five isolated X11 cases. Nested helper output is not counted twice;
display identity/Yama1 stayed unchanged and run029's exact processes remained
gone. These are fixture results, not evidence that live torn reads or refill
have been resolved. Record a fresh exclusive Luna medium trial next.

## Recorded run030: worker refill, persistence and counter advancement passed

Immutable source `6060216`, CLI SHA-256
`31da4e7f40cf6ffd399b6a50b08061e9bda0a1f151fc3f944c351401b6691dcc`
ran Normal historical vanilla Germany, no mods or DLC. First video packet was
verified before one Start; own memory confirmed paused noon on1January1936,
speed0, four empty slots and no focus. Focus stayed disabled. One exclusive
Luna medium player owned gameplay; the parent made no game reads/input during
that ownership.

The worker alone navigated and assigned support_weapons to slot0,
basic_machine_tools to slot1, electronic_mechanical_engineering to slot2 and
construction1 to slot3 on the same paused day. Fresh canonical reads matched
all four durable goal completions; idle0, budget used4/4, remaining0. Navigation
retains14 steps:12 verified and two pre-intent View torn_read failures for support,
with counts1 and2. The third fresh attempt passed without weakened witnesses;
both original reasons remain historical. No pre-navigation refusal or quarantine
was present. This proves bounded recovery, not identification of the changed
byte or a new return-to-panel route.

Catalog RPC was65ms and initial start362ms. Revision2 update passed in214ms;
the immediate identical retry returned known nonqueued input_busy in19ms.
After a server wait, the identical retry returned updated:false in235ms without
resetting consumption. Stop/restart retained revision2 and budget4/4 with no
replay. Digests took92–115ms. Worker stop took9–13ms and changed no game time.

After stopping the worker, ordinary unpause and a bounded server wait reached
3January at01:00 in32.87 seconds; normal pause verified the final04:00 state.
All four raw counters advanced **101000 to303000**; their units remain unverified.
Lease release/status proved remaining0 with the canonical identity/reply shape.

Two H.264 segments finalized and passed ffprobe: **884.333 seconds**,
**11,780,918 bytes**,512×288 at3fps. One owned WM close returned actual normal
exit **0**, no signal/core dump, in **3.156 seconds**. Both exact process paths
were absent afterward, Yama1 was unchanged and both clients closed. Private
calls, journals, proof and recordings are preserved. All nine CI jobs passed
for the exact source commit; offline gates remain separate from this live proof.

Same-day four-slot worker refill, idempotence, restart persistence and subsequent
counter advancement passed this bounded run. Research completion queues, focus,
economy, army/naval automation, independent Windows/macOS live adapters, clean
two-year acceptance and full campaigns remain unfinished. Next take a fresh
exclusive Luna focus Open/read-only geometry gate before any View/Start attempt.

## Recorded run031: focus opening passed; geometry limit blocked selection

The same immutable `6060216` native source and CLI used in run030 launched
Normal historical vanilla Germany, without mods or DLC. Recording began before
Start; memory confirmed paused 1 January 1936 at noon, four empty research slots
and no active focus. Focus was enabled and the daily worker disabled. Luna medium
held exclusive gameplay input; the parent made no game reads or input during it.

The own catalog confirmed `GER_remilitarize_the_rhineland`. OpenPolitics verified
in 2.558 seconds and OpenFocus in 7.091 seconds. Before View, one exact-node
geometry observation found both Sprite controls stopped at the same traversal
policy boundary: two ancestors observed, then a typed reciprocal ancestor with
4,687 children and capacity 5,395 exceeded the ordinary limit of 2,048. Its child
payload was not read. The graphics root remained unproved and the conditional
unit-cache diagnostic unavailable. Luna stopped before View or Start; no rescue,
clock advancement, research or further game observation followed the refusal.

This identifies run031's concrete boundary. Run026 had no corresponding geometry
observation, so its earlier generic failure still cannot be assigned this cause.
Traversal policy was byte-identical in both sources; diagnostics were not a fix.

Lease release and canonical status confirmed zero remaining authority. The H.264
recording passed ffprobe: 201.333 seconds, 2,508,836 bytes, 512×288 at 3fps. The
recorder's requested stop returned code 255; this is retained separately from the
game exit. One owned window close returned actual normal game exit 0, without
signal or core dump, in 2.803 seconds. Both exact process paths were absent,
Yama remained 1 and both protocol clients closed. Private geometry, calls,
receipts and footage are preserved. Focus selection and campaign acceptance
remain unpassed.

## Ordered research queues: fixture validation

The next candidate keeps one pending head per own slot. Plans can append distinct
technologies while preserving the existing per-slot order, completed assignment
receipts and consumed daily budget. Occupied slots wait for fresh empty-slot
evidence without a day-long refusal; other slots may progress. Missing slots
refuse explicitly, and repeated technology keys are rejected. No completion ETA,
dependency eligibility or new input route is inferred.

Rust 1.86 host tests passed **550 checks**; Python passed **953 tests**, with
**37 skipped**, in 20.656 seconds. Formatting, host/Linux Clippy, MSRV Linux
compilation and release linking passed. The actual Linux bundle reported
**645 passing checks** (**643 substantive and two inert helpers**), including
five isolated X11 cases; display identity and Yama1 remained unchanged.

The first parallel Linux library run failed the existing generated threaded-exec
stress fixture: 302 passed, one failed, 23 ignored. The rejected anchor retained
PID, UID and start time but reported parent0, so the production ownership guard
refused; the fixture had unconditionally required success. The same immutable
binary passed the full serial rerun. Both outputs are retained. The separate
endpoint worker resource-error message was emitted by an intentional worker-limit
test and must not be attributed to this anchor failure. No production ownership
guard was weakened. Fixture refinement remains separate work.

These tests establish queue mechanics, not installed-game refill after a
technology finishes. Focus policy changes are not included in this candidate;
run031's limits remain unchanged until separately reviewed and tested.

## Scoped focus traversal and exec-stress expectation: fixture validation

The next source opts in only from verified own-focus node/start producers. Its
fixed request-local policy allows capacity8,192 only for exact generic Position
ancestors outside the owned container positions; the whole path is bounded to
16,384 pointer comparisons. Ordinary callers keep their original limits. Sibling
pointers are compared, never dereferenced. Depth16, structural validation, unique
reciprocity, final witnesses and cache checks remain unchanged. A closed root350
still cannot supply input coordinates. Six new fixtures cover the recorded-sized
vector, default refusal, aggregate/capacity bounds, malformed/duplicate links,
wrong root, unknown/cyclic/detached ancestry and torn fields. Generated ancestry
beyond the observed boundary is a fixture, not installed-game proof.

The threaded-exec stress fixture now recognizes only a structured same-anchor
parent0 refusal captured at the exact guard. It verifies the prior executable
identity remains unpublished, performs no readiness retry or signal, and waits
for the generated child's natural exit. Success still requires the configured
image and device/inode. Other errors or identity changes fail the test. This
capture and expectation code is test-only; production guards are unchanged.

Combined Rust1.86 host tests passed **556 checks**. Python passed **953 tests**,
**37 skipped**, in22.400 seconds; formatting, host/Linux Clippy, MSRV Linux
compilation and release linking passed. The immutable Linux bundle passed its
parallel run: **652 reported checks**, **650 substantive plus two inert helpers**,
including five isolated X11 cases. Display identity and Yama1 were unchanged.
No nested helper result is counted twice. No new live focus selection, closure,
technology-completion refill or campaign acceptance is established by these tests.
Record a fresh exclusive Luna medium focus gate next.

## Recorded run032: focus selection passed; Start callback guard refused

Immutable source `b3abe90`, CLI SHA-256
`b95db519a4869d4fa0acfb192c74b9d421b6ee7520b9ee1ac137dff6340a452a`
launched Normal historical vanilla Germany, no mods or DLC. Recording's first
packet was verified before one Start; own memory confirmed paused noon on
1 January 1936, speed0, four empty research slots and no focus. The daily worker
was disabled. Luna medium held exclusive input; the parent made no game reads
or input during that ownership.

OpenPolitics verified in2.180 seconds and OpenFocus in8.432 seconds. The exact
Rhineland node diagnostic took168ms: both Sprite chains contained ten ancestors,
closed at owned graphics root1f0, and had consistent conditional unit caches,
cache-equivalent textures and no mask. View verified in3.843 seconds and selected
`GER_remilitarize_the_rhineland` in the own detail. This is live evidence that the
scoped policy handled this recorded hierarchy and supported exact-node selection.
It does not establish run026's cause or other unobserved render branches.

The single Start request refused in397ms with `focus_geometry_unavailable`,
"Exact own selected start callback required." No retry or further game read
followed; no active focus change is claimed. That combined guard did not identify
which binding field was unavailable. Preserve the uncertainty: the next fresh
run needs one selected own-detail geometry observation before Start, and the
guard's diagnostic should expose bounded producer reasons without granting input.

Actual lease release/status proved zero remaining authority. The H.264 recording
passed ffprobe:298.333 seconds,3,536,110 bytes,512×288 at3fps. Requested recorder
stop returned255, retained separately from actual normal game exit0. One owned
window close took3.449 seconds, without signal/core dump; both exact process paths
were absent, Yama1 unchanged and both clients closed. Private calls, geometry,
receipts and footage are preserved. Focus start, clean two-year acceptance and
full campaigns remain unpassed.

## Canonical occupancy filter and bounded Start diagnostics: fixture validation

The next source skips full research UI reads only when a fully validated canonical
probe proves every eligible queue head is occupied by a different technology.
It reuses the complete own canonical reader, scope/build/identity/chronology and
final witness checks. It publishes no sequence, digest, completion or input
authority. Empty or same-target slots still get an independent full UI read;
probe errors refuse and a changed day defers without altering consumed budgets.

Generated fixtures measured367 logical read requests for the occupied probe
against593 for the old canonical/public-UI lower bound, saving226. The idle
probe plus independent canonical/UI reads used864 against the old lower bound545,
adding319. The reference omits additional private context/selection reads. These
are fixture request counts, not syscalls or live timings; the complete catalog
still costs work. Occupied-slot savings and idle overhead must both remain clear.

The unchanged Start callback guard now reports only allowlisted producer reasons,
renderer types, tri-state binding flags and selected-key equality. Missing values
remain unknown; arbitrary strings, keys and pointer values are not echoed.
Four new fixtures verify useful known reasons, absent/wrong-typed fields,
sanitization, and unchanged refusal/success. These diagnostics cannot establish
which field failed in the earlier run032 or grant an input retry.

Combined Rust1.86 host tests passed **564 checks**; Python passed **953 tests**,
**37 skipped**, in24.482 seconds. Formatting, host/Linux Clippy, MSRV Linux
compilation and release linking passed. The immutable actual Linux bundle passed
**660 reported checks** (**658 substantive and two inert helpers**) in parallel,
including five isolated X11 cases; display identity/Yama1 stayed unchanged and
run032's exact processes remained gone. Nested helper output is not counted twice.
Runtime filter latency, completed-technology refill and Start remain unverified.
Next take the recorded fresh selected-detail diagnostic before any Start input.

## Additional Start callback path: stricter refusal

The subsequent static audit found that the exact event-glue consumer also checks
an alternate manager at+1d8, invoking its+1e0 function on the+1c8 capture before
the expected event callback. The existing member-constructor slot list omitted
that path. Both own Start glues now require this manager to be zero and refuse
`additional_event_variant_unavailable` before reading the alternate capture,
invoker or renderer. This tightens the callback whitelist; it does not add a
callback variant or establish what failed during run032.

Three actual-reader fixtures cover both glues with and without geometry, skipped
payload reads, the unchanged known branch, and final-witness mutation on valid
and refused paths. The audit also corrects terminology: the no-op invoker reads
an integer argument, and the Start member receives the event's control pointer.
No direct call or native posting was executed or enabled by this static evidence.

Combined Rust1.86 host tests passed **567 checks**; Python passed **953 tests**,
**37 skipped**, in24.274 seconds. Formatting, host/Linux Clippy, MSRV Linux
compilation and release linking passed. The immutable actual Linux bundle passed
**663 reported checks** (**661 substantive and two inert helpers**) in parallel,
including five isolated X11 cases. Display identity/Yama1 were unchanged and
run032's exact processes remained gone. These stricter-guard fixtures do not
establish the runtime callback association or successful Start.


## Recorded run033: selected Start association unavailable before input

The stricter source `f469990` ran with one exclusive Luna medium player in vanilla
Normal/historical Germany. OpenPolitics verified in 1.346 seconds, OpenFocus in
4.824 seconds and View in 6.594 seconds. Both pre-View Sprite paths closed at the
own graphics root with consistent conditional caches and unmasked resources.
The selected detail matched the exact Rhineland key, but Start association
reported `event_binding_unavailable`, false callback/renderer association flags,
null renderer type and nested `focus_geometry_binding_unavailable`. Start was
not attempted. No further game observation or clock change followed this gate.
This identifies the unavailable association category, not its underlying cause
or the specific combined-guard failure in run032.

Canonical lease release and zero remaining lease time passed. The H.264 recording
was 478 seconds, 5,387,633 bytes, 512×288 at 3 fps and passed ffprobe. Requested
recording stop returned255 and remains preserved. One ordinary owned window close
produced actual game exit0 without signal or core dump in 2.621 seconds; both exact
owned processes disappeared, Yama remained1 and both protocol brokers closed.
No campaign-success claim follows this bounded gate.

## Own equipment-line observer: generated fixtures before live comparison

The new read-only producer uses independently recovered Linux constructors,
serialization and handle witnesses for the exact installed ELF. It validates
own production status, the whole scoped equipment-line array and reciprocal
ownership for known military/naval layouts before reading full eight-byte handles
and separate signed active/damaged/requested counters. Unknown layouts return
bounded static metadata without assuming ownership or reading their payload.
Every observed field is rechecked before paging; indices and handles are scoped
to the fresh snapshot and do not establish cross-load identity.

The default page is32 and maximum128. Generated tests check unsupported metadata,
full-width string handles, anomalies, changed roots/owners/arrays/counters,
malformed pages before process access, and complete validation outside the returned
page. Known/unknown/mixed maximum pages fit the entire escaped MCP response bound.
Equipment/public ID joins, general production and allocation remain unavailable.
Combined host Rust1.86 tests passed582; Python passed953 with37 skips; Linux
compiler, both Clippy targets and formatting passed. Actual Linux execution
passed678 reported checks (676 substantive), including five isolated X11 cases.
The original library worker-limit fixture stderr remains preserved. Live own-line
comparison remains pending; generated tests add no input authority and are not
installed-game evidence.


## Focus Start topology diagnostics

Run033 did not record its bounded listener count or target membership. Static
ELF evidence shows that named Start lookup and a separate generic listener
registration traverse different collections; it does not establish the actual
run033 topology or authorize an alternative route. A diagnostic now reports
count and expected-target match booleans after complete bounded-list integrity
checks. It exposes no pointers, reads no unknown target payload, and preserves
the existing exact-two listener action guard. Four new Reader fixtures cover
unavailable singleton/other-target branches, unchanged admitted topology,
malformed structures and final changed witnesses. Capabilities also explicitly
mark own equipment-line observations as awaiting installed-game validation.

The composed source passed586 Rust1.86 host tests,682 reported actual Linux
checks (680 substantive),953 Python tests (37 skips), both Clippy targets, format,
MSRV Linux checks and release linking. These checks do not identify live listener
topology or prove focus Start.


## Recorded run034: own production lines and the actual singleton listener

With one Luna medium player in vanilla Normal/historical Germany, one own
production page returned21 supported lines, no unsupported layouts, and complete
coverage of that scoped collection. Full string handles and separate signed
active/damaged/requested counters are preserved privately. Equipment IDs, UI
comparison, allocation, idle capacity and cross-load identity remain unavailable.
OpenPolitics verified in2.017seconds, OpenFocus in5.108seconds and exact Rhineland
View in2.881seconds. Pre-View paths closed at the own graphics root with consistent
conditional caches. The selected-detail diagnostic then showed exactly one
selected-event target, no NOOP target and no unknown target, failing the current
exact-two topology guard. Start was forbidden and not attempted; no further game
read or clock change occurred. This cause belongs to run034 only.

Canonical release/status receipts confirmed zero remaining authority. Luna first
wrote their proof payload only on the host while reporting the canonical guest
path. The cleanup guard caught the absent guest file before any close input. The
actual persisted RPC payload was transferred and read back, with the mistake and
failed precondition retained. No game calls or repeated lease release were used
to repair it. The H.264 recording passed ffprobe at345.333seconds/4,198,103bytes;
requested stop255 is preserved. One ordinary owned close then gave actualexit0,
no signal/core, in2.422seconds; both exact processes disappeared and Yama remained1.

## Selected-event singleton binding and selector prerequisite

Independent saved ELF witnesses prove the ordinary selected-event branch reaches
the selected member without a contribution from the second known listener.
Other event kinds can differ, so this is a local callback proof, not global
observer equivalence. The producer now admits exactly the selected target alone
or the prior exact pair after complete list validation. It never reads an absent
NOOP capture, and unknown/extra/NOOP-only targets still refuse.

The callback consumes the named control's signed selector; both routes now require
a fresh witnessed zero before capture or renderer traversal. This tightens the
prior two-target route as well. Existing owner/member/alternate-manager guards
and typed hierarchy/cache/input checks remain. Four new Reader fixtures cover
the singleton route, absent-payload avoidance, guarded refusals, both selectors
and final changed witnesses. A live focus Start has not passed yet.

The composed source passed 590 Rust 1.86 host tests, 686 reported actual Linux
checks (684 substantive), 953 Python tests (37 skips), both Clippy targets,
format, MSRV Linux checks and release linking. Live Start remains unverified.


## Recorded run035: fields-present player parsing failure

The selector-checked source `b35f7e7` ran with exclusive Luna medium input in
vanilla Normal/historical Germany. Focus opens and exact Rhineland View verified.
Pre-View and selected-detail observations took 99.7/95.0 ms. The saved selected
response contains positive available/callback/renderer associations at
`details[0].window.start.ordinary_binding`, with the admitted selected-only
listener, zero-selector prerequisite, TextSprite, closed own root1f0 and
consistent conditional cache.

Luna instead inspected the separate node symbol and reported the required Start
fields missing. That report is incorrect; this is a player parsing failure, not a
producer refusal. The original report/metrics and full response are preserved,
with a separate parent correction. Start was not attempted, and no later game
read or input occurred. Positive read-only metadata does not prove a live Start.

Canonical lease proof was transferred and read back correctly. Recording passed
ffprobe at 322.333 seconds, 3,903,554 bytes; requested recorder stop255 is retained.
One ordinary owned close gave actual exit0/no signal/no core in 2.761 seconds;
exact owner/game processes disappeared, Yama remained1, and brokers closed.

## Compact selected-focus window

`hoi4_focus_selected_detail` reuses exactly one original fresh node/detail read
and complete witness sequence, then projects the matched selected window. Start
is directly at `window.start.ordinary_binding`; the unrelated node symbol and
catalog are omitted. Missing, different or multiple selected details refuse
explicitly; missing/unavailable Start fields keep their actual reasons and flags.
No cache, new offsets, coordinate or input authority is introduced. Optional raw
geometry retains the original diagnostics. The singleton success label now
reflects the selected handler alone; the existing pair keeps its prior label.
Seven producer fixtures and one protocol fixture cover path separation, strict
arguments, unchanged reads/brackets, unavailable/ambiguous selection and final
changed witnesses. Daily Focus foundation work remains private and disabled.

The composed source passed 598 host Rust tests, 694 reported actual Linux checks
(692 substantive), 953 Python tests (37 skips), both Clippy targets, format,
MSRV Linux checks and release linking. The first fixture launch raced installation
and found no library binary before any native case ran. That orchestration failure
is preserved; after all ten payload hashes were verified, the same immutable
binaries passed. No live Start is inferred from these tests.


## Recorded run036: exact focus Start passed

Source `c1aa7ff` ran with exclusive Luna medium control in vanilla Normal/historical
Germany. OpenPolitics verified in 1.489 seconds, OpenFocus in 4.756 seconds and
exact Rhineland View in 2.483 seconds. Pre-View geometry took 108.4 ms; the new
compact selected-detail read took 98.3 ms and was parsed at the correct path.
The selected-only listener, fresh selector-zero requirement, callback and renderer
associations, zero-angle TextSprite, closed own root1f0 and consistent conditional
cache passed. One ordinary Start returned Verified/current-state readback in
1.651 seconds, with exact own active focus `GER_remilitarize_the_rhineland`.
No post-Start game observation or clock change occurred.

The lease released and canonical proof was transferred/read back against actual
RPC responses. H.264 recording passed ffprobe at 247 seconds/3,414,190 bytes;
requested recording stop255 remains preserved. One ordinary owned close gave
actual exit0/no signal/no core in 2.319 seconds; exact game/owner processes
disappeared, Yama remained1 and both brokers closed. Earlier failures and the
separate run035 parsing correction remain retained. This proves a bounded
ordinary Start, not focus completion, eligibility modeling or campaign acceptance.

## Disabled daily Focus foundation

The next source extracts a borrowed Focus executor under the existing common
controller and original absolute deadline, without mutex/RPC re-entry or a new
input route. A strict converter accepts only a fresh exact Verified Start,
canonical Active key and unchanged full own domain context before completing an
assignment. Open/View/preparation/history cannot complete it. Ordered pending
heads and append-prefix updates preserve completed entries and consumed budgets.
A separate durable preparation ledger retains full entry/target/day/revision/
binding/receipt or actual failure; unknown/lost outcomes remain blocked across
restart. Research storage/navigation schemas are untouched. Daily Focus remains
hard-unavailable; worker wiring, explicit default-off enablement, trusted final
producer plumbing and cross-domain UI handoff are separate future gates.

The composed disabled foundation passed 606 Rust 1.86 host tests, 703 reported
actual Linux checks (701 substantive), 953 Python tests (37 skips), both Clippy
targets, formatting, MSRV Linux checks and release linking. Fixture archives
were completely installed and hash-verified before execution. The manual Focus
extraction remains a new live-regression boundary; daily Focus stays disabled.


## Bounded public-topology foundation — 14 October 2026

The independent Rust graph kernel validates complete symmetric adjacency over
explicit supplied node IDs, computes ordered connected components and performs
multi-source breadth-first search with deterministic smallest-source ties.
Unknown nodes, malformed edges and work/count bounds refuse; depth truncation is
explicit. It consults Clausewitz-MCP's graph ideas with retained attribution,
without importing its hidden-enemy payload producers or executor.

Combined checks passed614 Rust1.86 host tests,711 reported actual Linux checks
(709 substantive; nested/inert fixture helpers counted separately),953 Python
tests (37 skips), format, host/Linux Clippy, MSRV Linux compilation and optimized
Linux linking. The Linux archive had11 hash-verified payloads; all generated tests
ran after the previous owned game exited normally, under unchanged Yama1. Five
X11 tests used their isolated fixture display. No game was launched or controlled
for this gate. There is no map producer, military access/supply inference, army
action integration, tactical live evidence or campaign acceptance from these tests.


## Daily Focus worker preview and late-return quarantine — 14 October 2026

The composed worker requires independent default-off owner `daily_focus`, an
existing Focus profile and explicit request `focus_permission`. Request permission
is immutable while running; stop/restart loads both ledgers and retains core-plan
serialization, completion and shared budget. Positive other-active Focus heads
wait without navigation, assignment reservation or day refusal, even at exhausted
budget. Preparations and actual Start audits stay separate; only a fresh strict
canonical Start receipt can complete an assignment. Cross-domain panel switching
is still unavailable. Matching-active goals use the actual fresh no-input stage
path rather than manufactured receipts; this boundary has no live proof yet.

Focus and research use separate absolute ceilings from one batch origin:10 and5
seconds respectively, both clamped to the lease. Late preparation and manual
returns retain actual Verified evidence and separately append checksum-bound
quarantine. A committed marker blocks reopening and all same-game inputs, including
old request IDs. A failed marker commit poisons the current owner; an unchanged
Verified-only file cannot prove an uncommitted marker after a crash. Replacing the
owner does not authorize adopting the orphaned game or clearing uncertainty.

The final source passed624 Rust1.86 host tests,723 reported actual Linux checks
(721 substantive),953 Python tests (37 skips), compiler/format/Clippy/MSRV gates
and release linking. The initial host Clippy failure identified Linux-only unused
helpers on macOS; narrow platform/test gates fixed it and original logs remain.
All11 bundle payloads were hash-verified before actual Linux execution; five X11
cases used the isolated fixture display under unchanged Yama1. No game was running
for these checks. Recorded run037 must reverify the borrowed path, worker-only
Focus Start, positive-active waiting, idempotent update and budget-preserving
restart before any live worker acceptance claim.


## Recorded daily Focus run037 — 14 October 2026

Exclusive Luna medium started the Rhineland-only worker in vanilla Normal,
historical FAIR Germany at1January1936 noon, paused with four empty research
slots. OpenPolitics, OpenFocus and View each have an actual Intent followed by
Verified/ServerConfirmed preparation. The core Start goal then recorded one
reservation and terminalfalse with known nonqueued `torn_read`: entry Pending,
attempts1, reason_day706640, used1/remaining0. There is no Start Intent/receipt
in the Focus action journal, no Start audit or completion, and no revision2 test.
The exact changed witness and error message were not retained; concurrency is
not an established cause. The old terminalfalse history must remain intact.

The single15-second wait refused a snapshot completing after its deadline at
15.014seconds. Luna stopped without requesting a digest or further observations.
STOP showed only the latest historical View preparation and omitted the core
refusal. The durable reservation and same-day exclusion prevented progression;
this was more than simply stopping before the worker reached its goal branch.
Future handling must preserve original cause and bound only trusted pre-input
retries within the same still-inflight attempt, without refunds, new IDs or
reopening an already terminalfalse reservation. Late wait payloads remain rejected.

Actual release=true/statusremaining0 were transferred to the canonical guest
file and independently read back against saved RPC receipts. The442.333-second
H264512x288 recording passed ffprobe; intentional recorder shutdown returned255.
One ordinary owned window close produced exit0 in2.317seconds, signalnull and
core_dumpedfalse, with no signal fallback or replay. Exact owner/game process
paths were absent afterward under unchanged Yama1; both brokers closed130.
No postfailure game observation or rescue occurred. Worker Start, positive-active
waiting, update/restart acceptance and full campaigns remain unpassed.


## Run037 retry and wait corrections — offline gates

The new Focus adapter permits at most two immediately proven BeforeIntent
`torn_read` retries inside one still-inflight core attempt, using its original
request and reservation. Every iteration requires fresh complete own contexts,
producer sequence, unchanged scope/day and lease/gate checks under the original
absolute deadline.100/200ms backoff never resets that deadline. Intent, receipt,
confirmed nonqueued delivery, unknown or poison cannot qualify. Original and
latest errors remain in separate checksummed Focus history, while digest/stop
expose core pending reasons and reserved budget. Run037's finished terminalfalse
remains immutable; no budget refund, new request or restart rescue is introduced.

A later wait read exceeding its deadline now yields historical timeout evidence
only when an earlier timely sample exists. Late payload/context are discarded;
state_historical=true/current_state_verified=false/input_authorized=false and
sample age distinguish the old sample. Any deadline-refusal diagnostic retains
the original error. Strict initial/no-stable refusals remain; real ownership,
scope and backend errors propagate even when late. This does not cancel a
synchronous read or promise exact RPC return latency.

The composed source passed634 Rust1.86 host tests,733 reported actual Linux
checks (731 substantive),953 Python tests (37 skips), compiler/format/Clippy/MSRV
and release linking. All11 optimized bundle payloads were hash-verified before
Linux execution; five X11 cases used an isolated display under unchanged Yama1.
These synthetic checks establish no corrected live worker acceptance. Fresh038
must verify actual Start, active-focus waiting and budget-preserving update/restart.


## Recorded daily Focus run038 — 14 October 2026

Exact source `0b221d68179ff747b86e80dccd1038b131434f49` passed all nine
Linux/Windows/macOS CI jobs after full reachable-history audit. One exclusive Luna
medium player started the Focus-only worker in vanilla Normal, historical FAIR
Germany at1January1936 noon, paused with four empty research slots. Recording
began before the single campaign Start; frontend startup took395.085seconds
under CPU emulation. Worker start RPC took328.5ms. The first45-second bounded
wait returned `focus_changed` after8.443seconds, with fresh canonical Rhineland
and zero rejected torn reads.

Independent saved-journal review confirms one Start Intent followed by
Verified/ServerConfirmed, canonical active `GER_remilitarize_the_rhineland`, a
matching separate Focus goal audit, and core entry Complete with terminaltrue,
after_sequence6 and exactly one consumed reservation. All three preparations
were separately verified. No Start failure/retry, refusal or quarantine occurred.
The transient-retry branch therefore has no live-trigger evidence from this run.

Luna's subsequent fresh digests returned `verified:[]` and only
`latest_verified_ids_historical:[focus-rhineland]`; their preparation summary
correctly said goal_completedfalse, while the Start summary omitted completion.
The player interpreted that presentation as incomplete and stopped without the
revision2 update, positive-active waiting or restart. Preserve the original player
result and the separate offline correction; the worker did complete the
assignment, but the reporting and intended acceptance sequence failed. The second
15-second wait returned normal wall_timeout with state_historicaltrue,
current_state_verifiedfalse, age110ms and deadline_refusalnull. It validates the
normal historical timeout response, not the late-read deadline-refusal branch.

STOP retained used1/remaining0 and no pending core entries. Actual release=true
and remaining0 were transferred to the canonical guest proof and independently
read back against saved RPC receipts. The233-second H264512x288 recording passed
ffprobe; requested recorder shutdown returned255. One ordinary owned window close
produced exit0 in2.349seconds, signalnull/core_dumpedfalse, without fallback or
replay. Exact owner/game identities were absent afterward under unchanged Yama1;
both brokers exited130. No postfailure game observation or rescue occurred.
Durable completion must be explicit beside separately fresh current-state evidence
before the next Focus update/restart gate. No two-year or full campaign acceptance
is established.


## Durable completion reporting and disabled panel foundation — offline gates

Digest and STOP now project checked durable assignment state directly from the
locked governor, independent of the latest batch. Counts and latest completion
(entry, original request, sequence and date) survive updates and restart; optional
completed IDs truncate explicitly. All completion evidence remains historical,
current_state_verifiedfalse/input_authorizedfalse. A fresh overview separately
proves current active Focus. Unknown persistence refuses digest and blocks input;
STOP preserves unavailable-summary and uncertainty information. Core Plan/state
serialization, request identities, permissions and consumed budgets are unchanged.

Meaningful generated checks cover actual completion, an empty later batch, plan
update/restart without replay, known nonqueued and unresolved entries, externally
replaced storage and retained poison, and256 completed assignments with bounded
IDs/latest evidence. An initial large-batch test exposed the old formatter adding
retained_count after optional IDs filled the2KB bound; essential counts now reserve
space first. The original failure is preserved. Fixture-only timing and method-name
failures, then a Linux immutable-lock compile failure and Clippy inspect_err issue,
were corrected without relaxing production deadlines, persistence or input guards.
Duplicate Focus waits and optional empty history are compacted; essential completion,
budget and uncertainty remain. Generated active-wait and restarted-report shapes
fit2KB, but the next recorded run must establish their live behavior.

A separate ClosePolitics foundation remains hard-disabled in public parsing and
the borrowed service. It admits only the exact canonical active requested Focus,
settled own mode12 PoliticsReady/FocusHidden/full-zero detail/info blockers, and
preserves that key through settled mode23 hidden readback. Generated tests verify
pre-input refusal, post-input uncertainty, journal reopening and nonreplay. The
installed-build static Q route is shortcut provenance, not live callback binding;
OS preflight likewise is not callback proof. No cross-domain action is enabled.

The final composed source passed646 Rust1.86 host checks,748 reported actual Linux
checks (746 substantive, excluding two fixture helpers),953 Python tests (37
skips), format, host/Linux Clippy, Linux MSRV compilation and optimized release
linking. All11 bundle payloads were hash-verified before Linux execution. Five X11
cases used their isolated display; the owned game had exited normally and exact
processes were absent. Ownership and Yama1 were unchanged before/after. Initial
failed logs remain separate from final passing logs. No live Focus update/restart,
panel handoff, two-year run or full campaign is established by these fixtures.

Historical count correction: summing every saved `test result` line, including
the nested exec helper and generated WM helper, gives724 reported/722 substantive
checks for worker-v21 and734/732 for retry-v22. Earlier723/721 and733/731 totals
were arithmetic/helper-count errors. The original logs, cases and pass/fail
outcomes are unchanged; the new748/746 total uses the same explicit two-helper
exclusion consistently.


## Recorded daily Focus run039 — 14 October 2026

Exact source4cf3bb1 passed all nine cross-platform CI jobs and full reachable-history
audit before exclusive Luna medium play. Vanilla Normal/historical FAIR Germany
started at1January1936noon, paused with four empty research slots. Recording's first
packet preceded the single campaign Start; frontend startup took394.152seconds.
The first bounded45-second wait returned focus_changed after10.300seconds with
fresh canonical Rhineland and zero rejected wait torn reads.

Saved journals independently verify one immediately trusted BeforeIntent Start
torn_read, reason `Focus witness changed`, index1/retry_plannedtrue, linked to the
original core Attempt/request. It then produced one actual Start Intent followed
by Verified/ServerConfirmed and canonical active Rhineland, a matching separate
Focus goal audit, core Complete with terminaltrue/after_sequence7 and exactly one
consumed reservation. There is no terminalfalse, new request or budget refund.
This is live evidence for the same-in-flight pre-input retry; precise changed
witness remains unidentified beyond the retained reason. Historical retry-planned
metadata never grants later retry authority.

The required fresh digest failed with digest_error, Essential digest exceeds bound.
It combined durable completion with retained pre-input failure history, a shape
missing from the previous compact fixtures. STOP preserved assignments completed1,
latest Complete/original request/sequence7, used1/remaining0, original/latest torn
cause, historicaltrue/current_state_verifiedfalse and retry_authorityfalse. Luna
stopped without further game observations, revision2 update or restart.

Actual release=true/statusremaining0 were transferred and independently read back
against saved RPC responses. The310.333-second H264512x288 recording passed ffprobe;
requested recorder shutdown returned255. One ordinary owned window close produced
exit0 in2.448seconds, signalnull/core_dumpedfalse, with no fallback or replay. Exact
owner/game processes were absent afterward under Yama1; both brokers exited130.
No postfailure observation or UI rescue occurred.

The correction references an exactly matching completed Start through the essential
latest-completion record and omits only successful preparation. Full STOP and
durable journals remain; failure, refusal, uncertainty and budget stay visible.
A generated recovered-Start failure shape and mismatched-request negative gate
exercise this boundary. Combined gates and fresh live update/restart validation
are still required. No two-year/full campaign or cross-domain acceptance is claimed.

The final overflow correction also shares a failure identity only when it exactly
matches the completed Start, retaining all original/latest causes, evidence and
retry-authority labels. Initial Linux fixtures exposed a stale required-truncation
expectation, then a genuine maximum-length-ID/two-reason overflow. Both failed
bundles/logs remain separate; the latter prompted identity sharing rather than
cause deletion. Three direct Linux digest checks passed before the full suite.
Final gates passed646 Rust1.86 host checks,749 reported actual Linux checks
(747 substantive excluding two helpers),953 Python tests (37 skips), compiler,
format, host/Linux Clippy, MSRV and release linking. All11 payload hashes and
unchanged display ownership/Yama1 passed; no game was active. These fixture gates
do not establish the pending live Focus update/restart or panel handoff.


## Recorded daily Focus run040 and recovery — 14–15 October 2026

Exact source8082a18 passed all nine CI jobs before exclusive Luna medium play.
The worker verified Politics/tree/View, then persisted one Start Intent followed
by NotQueued. Core state remains Pending with one consumed reservation, no goal
completion and no retry authority. The45-second wait returned historical timeout
with zero rejected torn reads; the fresh digest failed on pending-failure history
exceeding2KB. No update/restart or further game observations followed. STOP
preserved the pending state and `input_not_queued`/ConfirmedNotQueued evidence.
The exact underlying transport cause was discarded by the adapter and is unknown.

The lease release and zero remaining duration were independently read back from
the guest before interruption. The Mac subsequently rebooted; on15October the
owned VM was absent and its SSH port refused connections. Cold-booting the same
private guest disk recovered the original journals and54 readable H264512x288
segments. The recorder had continued after the stopped worker; no finalization
record or normal game exit was present. Exact old owner/game processes are absent
and Yama remains1. Do not count this as clean shutdown or campaign acceptance.

The correction retains an exact bounded in-memory copy of the sanitized digest
before pruning. If essential histories exceed2KB, it returns a compact summary
with counts/budget/uncertainty and historical failure codes, plus a content-hash
lookup for complete historical sections. Details perform no game reads/input;
expired IDs fail rather than substituting newer history. STOP and durable journals
remain unchanged. Focus now includes the transport refusal cause in its error
without changing terminal classification or allowing a finalized attempt to retry.
Generated crowded-history, exact-lookup, stale-ID, pending-Start and terminal
nonreplay tests cover these changes; fresh installed-game acceptance is pending.

The composed source passed650 Rust1.86 host tests,754 reported actual Linux
checks (752 substantive, excluding two fixture helpers),953 Python tests
(37 skips), format, host/Linux Clippy, Linux MSRV compilation and release linking.
All11 payload hashes passed before execution; guest boot identity, old-process
absence and Yama1 matched before/after. The first Linux suite exposed a fixture
that assumed a short history necessarily overflowed; its log remains preserved.
The strengthened maximum-ID case passed directly, followed by the full suite.
These are offline gates and do not establish fresh live behavior.


## Recorded daily Focus run041 — 15 October 2026

Exact source `2a36278` passed all nine three-OS CI jobs before this recorded
vanilla Normal/historical Germany test with one exclusive Luna medium player.
The recording had a verified packet before the single campaign Start. The game
remained paused at 1 January 1936; no clock, console, effect, native-post or
manual focus command was used by the player.

The private single-accept broker initially refused two concurrent connections
before either request was sent. The original failure, lease release and saved
report remain preserved. A local socket fixture reproduced the queue saturation;
serializing complete calls removed it without retrying a sent request. After
independent verification that the worker had never started, Luna continued in
the same game with the unchanged native binary. This was a harness correction,
not a gameplay retry or reload.

The daily worker verified OpenPolitics, OpenFocus, View and one Start. The saved
Focus journal contains exactly one Intent/Verified pair per stage. The core
assignment recorded one reservation followed by terminal Complete at sequence 6;
fresh overview confirmed canonical `GER_remilitarize_the_rhineland`. No pre-input
retry, Start refusal or quarantine occurred.

Revision2 retained that completion and added `GER_the_four_year_plan`. With
Rhineland active, the next entry waited with zero attempts, no navigation and no
additional budget use. An identical update returned `updated:false`; both update
responses retained the consumed budget. Two worker restarts preserved the same
revision, completion and used 1/limit 1 budget. Offline journal inspection found
no extra Focus action rows or replay. A historical transient `input_busy` from
producer contention remained visible; no failure or refusal was erased.

Five fresh digest payloads measured **1,556, 1,241, 1,241, 1,913 and 2,008 bytes**;
RPC times ranged **49.7–62.4 ms** through the persistent local SSH/broker path.
Two used the compacted-history fallback. Eight exact section lookups returned
historical data with current-state/input authority false. These five samples
establish the bounded gate, not broad throughput or campaign performance.
One server wait observed `focus_changed` after 10.22 seconds; two later 45-second
waits returned normal historical timeouts, followed by fresh digests. The first
offline report mistakenly filtered the name `hoi4_wait` instead of actual
`hoi4_wait_for`; its empty wait list was corrected against original RPC records,
with both reports preserved.

Final STOP changed no game time. The latest actual release/status receipts were
persisted to the guest and independently read back before close. Both H264
512×288 recording segments finalized and passed ffprobe, totaling 834 seconds.
One ordinary window-close request returned normal exit 0 after 2.19 seconds, without
signal fallback or replay; exact owner/game absence and unchanged Yama 1 passed.

**Remaining limits:** a rapid restart followed by STOP can expose a null cached
budget, although fresh digests and durable journals prove the consumed attempt.
No new transport refusal occurred, so the cause-preservation correction remains
offline-tested only. ClosePolitics is still disabled. Cross-domain panel handoff,
focus completion over game time, completed-technology refill, strategic warfare,
Windows/macOS native live support and full campaign acceptance remain open.

## STOP history and compact Focus UI foundation — 15 October 2026

STOP now reads budget and assignment summaries from one checked durable state,
independent of the latest worker report. It reports the historical accepted day,
date and sequence without reading the game. Before the first accepted daily
observation, day/usage/remaining stay null. Store replacement or corruption makes
both summaries unavailable and blocks input. Generated tests cover immediate
restart, identical updates, unchanged journal bytes, no game reads, nonqueued and
uncertain reservations, conservative legacy budgets and persistent refusal after
an invalid store is restored.

The new optional `hoi4_focus_ui` projects the existing witnessed own Focus/Politics
model into a compact read-only response: active canonical focus, managed mode,
settled local roots and complete own detail/research-Info counts. It needs an
owned Focus binding but no input lease. Unknown blockers stay null; private
process/campaign identities, raw addresses, geometry and command authority are
not exposed. The ClosePolitics parser and runtime still refuse that stage.

The composed source passed **656 Rust 1.86 host tests**, **760 reported actual
Linux checks** (758 substantive, excluding two fixture helpers), **953 Python
tests** (37 skips), format, host/Linux Clippy, Linux MSRV compilation and optimized
release linking. All 11 Linux payload hashes passed before execution. Guest boot
identity, prior exact-process absence, prior normal exit and unchanged Yama 1
matched before/after. These additions still require a fresh recorded live gate.


## Recorded Focus UI and immediate STOP run042 — 15 October 2026

Exact source `7f71e92` passed all nine three-OS CI jobs before this recorded vanilla
Normal/historical Germany gate with one exclusive Luna medium player. There were
no mods. The recording had a verified packet before the single campaign Start;
the game remained paused on 1 January 1936, with no clock or manual Focus input.

Before acquiring a lease, Luna queried `hoi4_focus_ui`: canonical Idle, managed
mode23, hidden Politics/Focus roots and zero visible/animating own detail and
research-Info objects. After the worker completed Rhineland, the same observation
reported the canonical active focus, mode12, settled Politics Ready, Focus Hidden
and both blocker counts zero. Payloads were **770/807 bytes**, RPC times
**99.85/104.88 ms**, and server times **93.55/99.30 ms**. These are two bounded
samples. Global modal clearance, effective routing, eligibility and input
authority remained explicitly unavailable or false.

The saved Focus journal contains exactly one Intent/Verified pair for each of
OpenPolitics, OpenFocus, View and Start. The core journal has one reservation and
one terminal Complete at sequence6. Fresh overview confirmed canonical active
Rhineland. Revision2 retained it and appended Four Year Plan, still Pending with
zero attempts. The first update committed; the exact duplicate returned
`input_busy` with an explicit not-queued message. Luna preserved the refusal,
stopped and released its lease without retrying that update.

After independent checks of the committed plan and clean STOP, Luna continued in
the same game with the same binary and a new lease. It started the exact committed
revision2 and made STOP the immediately following RPC, with no intervening game
read. Both STOP responses retained the same completion identity and historical
budget **used1/limit1/remaining0**. Neither changed game time or reported
uncertainty. The final journal has no replay or additional Focus actions. Run041's
successful duplicate no-op update remains separate evidence; run042 does not
claim that its refused duplicate succeeded.

The latest actual release/status receipts were persisted and independently read
back from the guest before close. Two H264 recording segments finalized and
passed ffprobe, totaling **739.333 seconds**. One ordinary window-close request
returned actual normal exit0 after **2.23 seconds**, with no signal fallback or
replay. Exact owner/game absence and unchanged Yama1 passed; local brokers closed.
An initial offline collector used the wrong STOP budget field and failed before
producing a report; the corrected verifier used original `daily_budget` responses.

This passes the compact observation and immediate STOP budget gates. No Q shortcut
or ClosePolitics action was attempted. Cross-domain panel switching, focus and
technology completion over game time, strategic warfare, Windows/macOS native
live support and full campaign acceptance remain open.
