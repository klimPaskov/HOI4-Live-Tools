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

## Attribution

The findings above refer to upstream `calling/{mem,process/linux,api,game,
commands,combat,ragemicro,turnloop}.py` and `mcp/server.py` at the pinned commit.
Clausewitz-MCP is Copyright (c) 2026 smerss, MIT licensed. See [NOTICE](../NOTICE)
and [the retained provenance](../third_party/clausewitz_mcp/UPSTREAM.json).
