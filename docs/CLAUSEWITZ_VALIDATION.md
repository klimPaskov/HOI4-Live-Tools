# Clausewitz-MCP Linux validation

12 October 2026. This is preflight evidence for the
[Linux-first migration](LINUX_RUST_HANDOFF.md), not HOI4 live acceptance.
The licensed Linux game has now launched and the upstream backend has read its
live process. Ordinary commands and Luna play remain unverified; the game was
still loading its GUI at the latest checkpoint.

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
future dispatch runs in the owned game launcher to retain parent access. No host
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

The x86 guest has no GPU acceleration on this ARM host. An independent hidden
OpenGL raster fixture measured roughly **1.1 frames/s** at 1280×720 and **4.5
frames/s** at 640×360 with llvmpipe. These are fixture measurements, not HOI4
frame rate. A bounded startup stack sample found the main thread waiting while
Mesa raster workers were active; the game continued changing loading backgrounds.
After about 38 minutes of GUI loading, graceful shutdown exceeded its 20-second
deadline. This pre-campaign process was suspended and preserved for diagnosis;
it was not force-killed. A monitor will restore its backed-up configuration only
after that exact process exits. Four recording segments passed ffprobe checks.
Playable startup remains unresolved. Private recordings, layouts and diagnostics
remain outside Git. No Rust engine code or Luna campaign has been started.

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
controls are implemented. The next live gate still requires a licensed Linux
installation matching the validated layout, ordinary-command readback, recording
and one Luna player at medium reasoning. The Rust engine port follows that gate.

## Attribution

The findings above refer to upstream `calling/{mem,process/linux,api,game,
commands,combat,ragemicro,turnloop}.py` and `mcp/server.py` at the pinned commit.
Clausewitz-MCP is Copyright (c) 2026 smerss, MIT licensed. See [NOTICE](../NOTICE)
and [the retained provenance](../third_party/clausewitz_mcp/UPSTREAM.json).
