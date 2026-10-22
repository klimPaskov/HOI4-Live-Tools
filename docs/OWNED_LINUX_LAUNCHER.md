# Owned Linux launcher preview

The Rust supervisor launches one verified Linux game and remains its ordinary-user
parent. MCP clients connect over a private local socket, so closing or restarting
a client does not relaunch the game or remove its owner. Observations execute in
that parent process and never attach, suspend, capture screenshots or parse saves.

This is development source. Read-only tools cover own state, focus/research
catalogs, GUI diagnostics and bounded waits. An explicit
owned binding can additionally enable exclusive leases and ordinary clock keys.
Engine commands, strategic actions, daily automation, display provisioning and
recording integration remain unfinished. Windows/macOS launch adapters return an explicit
unavailable error. See [current live evidence](CLAUSEWITZ_VALIDATION.md).

## Prerequisites

- Rust 1.86+, an ordinary Linux user, a licensed installed game and its normal
  runtime libraries.
- The exact Linux x86_64 build identified in [native setup](../native/README.md).
  A different executable hash or GNU build ID is refused before launch.
- A task-owned local X11 server with its actual PID, creation ticks, executable
  and UID, plus a private Xauthority file. The current launcher accepts this
  display configuration; it does not provision or discover a desktop.
- An existing private run parent directory, owned by the runtime user, mode
  `0700`. Each launch uses a new run directory. Configuration files use `0600`.
- On Linux, the complete `run_directory/control.sock` pathname must fit 107 raw
  bytes plus its terminating NUL. The launcher checks it before creating userdata;
  choose a short private path. This is a Linux pathname-socket limit.

The launcher does not invoke sudo, change Yama, alter the game binary or install
mods. A parent relationship does not override every kernel security policy;
denied reads still return explicit errors. The installed game launched under its
Rust owner with unchanged `ptrace_scope=1`; actual build/header reads and two
stdio-client reconnects passed. These frontend checks do not prove game commands.
The fresh run then entered Normal historical Germany through the ordinary menus;
its initial own-country snapshot also passed under the same unchanged policy.

## Configuration and lifecycle

Save a private JSON configuration with these fields:

| Field | Value |
| --- | --- |
| `executable` | Absolute installed game executable path |
| `run_directory` | Absolute new directory under the private run parent |
| `display.number` | Local numeric X11 display |
| `display.server` | Actual `pid`, string `start_ticks`, absolute `exe`, numeric `uid` |
| `display.xauthority` | Absolute private regular Xauthority file |
| `settings_seed` | Optional absolute regular settings file, bounded to 1 MiB |
| `renderer` | Optional `default`, `llvm_pipe` or `virpipe` |
| `gpu_transport` | Optional private owned Unix socket; required for `virpipe` |
| `fex` | Optional explicit ARM64 translator configuration described below; omitted for native launch |

The settings seed is copied into new isolated userdata. The only game launch
argument is `-userdir=...`; the fresh playset enables no mods. Default user
settings and playsets are never modified. Game output goes to a private file.

### ARM64 Linux with FEX

The experimental ARM64 runtime can launch the same verified x86 game through
one reviewed FEX binary. Install and configure FEX and its x86 root filesystem
separately; neither is bundled. Add this field to the private launch configuration:

```json
"fex": {
  "executable": "/usr/bin/FEX",
  "sha256": "bb7d1d4147e224ab83eef02c3fae59e86a297fa829c847a4afcc273f106f6057"
}
```

This pin identifies the tested ARMv8.4 Ubuntu package `2610-1~n`. A caller-supplied
hash cannot approve another translator. `executable` at the top level still
identifies the licensed x86 game; its existing build ID and SHA-256 remain required.
The kernel child identity names FEX, while the session's optional `translated`
descriptor identifies the separate guest image. Omitting `fex` preserves native
launching and the previous session format.

Readiness waits only for missing initial guest load coverage, within one ten-second
deadline. Wrong hashes, ownership, offsets and permission errors do not retry.
Snapshot checks pin guest file identity, load coverage and immutable headers;
normal loader permission tightening is allowed. A changed bound image or kernel
execution witness refuses without automatic rebinding. Duplicate detection covers
native instances and mapped games under the reviewed FEX binary, including renamed
copies, without inspecting every unrelated process's memory.

Generated ARM-parent/x86-child tests verified scalar and batched reads, stale
observation refusal after guest `execve` and FD-based `execveat`, duplicate
detection, ordinary fixture exit and native CLI reconnects. The FD-based test
also confirmed a FEX limitation: its restarted guest receives `argc=0`. The fixture
records this explicitly and allows that marked generation only to report its
nonce and quit; this is not evidence of correct guest argument handling.

**The first actual HOI4/FEX launch failed before the menu**, reporting a missing
OpenGL version and SIGSEGV. Its stuck crash handler required separately recorded
forced cleanup after the owner's SIGTERM timeout. A later run with GL forwarding
disabled and guest llvmpipe reached the native 1920x1080 frontend in 50.7 seconds
and exited normally through ordinary window close. A prior attempt exhausted the
four-GiB VM; temporary guest swap was present in the successful run. These checks
do not qualify game observations, recording or gameplay. Input binding is
explicitly unavailable for translated sessions. Owned
lifecycle/window-close operations remain available for startup qualification.
Future campaigns must use native 1920×1080 or higher after independent validation.

Build and start the supervisor in its own persistent terminal/service:

```sh
cargo build --manifest-path native/Cargo.toml --release --locked
native/target/release/hoi4-live-native --serve /absolute/private/launch.json
```

Configure the MCP client to execute:

```sh
native/target/release/hoi4-live-native --connect /absolute/private/run
```

Use absolute executable paths in the client configuration. Stdio output contains
only JSON protocol replies; launch diagnostics go to stderr. The owner ignores
terminal hangup. This is not a guarantee against SIGKILL or service-manager
cleanup; a later owner never automatically adopts an orphaned survivor.

For an explicit owned shutdown:

```sh
native/target/release/hoi4-live-native --close /absolute/private/run
```

The owner verifies the exact child identity and parent, sends SIGTERM through a
pidfd, and waits up to 20 seconds. It never force-kills a hung game. A timeout
leaves the owner, endpoint and userdata available for diagnosis. SIGINT/SIGTERM
to the owner requests the same graceful shutdown. Unexpected startup failures
after spawning preserve the parent until its launched child exits.

A successful close confirms that the owned child exited; it does not imply a
clean game exit. The same child reaper records `exit_code`, `signal`,
`core_dumped` and a formatted `wait_status` in `exited.json` and the close
response. A normal exit code 6 and termination by signal 6 are distinct outcomes.
This evidence does not change the shutdown policy or resolve a game failure.

## Explicit window-close preview

An alternative owned lifecycle request sends one ICCCM `WM_DELETE_WINDOW`
message to a supplied, verified game window:

```sh
native/target/release/hoi4-live-native --close-window /absolute/private/run /absolute/private/window.json
```

The private regular configuration is bounded to 64 KiB and contains only
`game` (the endpoint's exact `pid`, string `start_ticks`, `exe` and `uid`),
nonzero `window`, and complete `window_class` bytes, including both NUL
terminators. The owner uses its configured authenticated display. It verifies
process ownership, display peer, window PID/class, viewable InputOutput state,
root and advertised WM protocol before sending. It never activates the window,
moves the pointer or uses XTEST. Client properties are checked declarations,
not cryptographic window ownership or protection from all same-user races.

The immutable attempt has a 20-second owner deadline and a wire deadline of
at most two seconds after preparation. It drains active reads, disables input
and retains the attempt across client disconnection. Delivery is reported as
`not_queued`, `unknown` or `server_confirmed`; a server acknowledgement does
not prove application consumption or exit. Uncertain delivery is never replayed,
and changing the retained target is refused. There is no automatic signal
fallback. An ignored request or timeout preserves the child, owner and endpoint.
The separate explicit `--close` signal policy above remains available.

The sole child reaper supplies actual exit metadata. Command success establishes
child exit, not exit code 0. Isolated Xvfb tests with XTEST disabled verified
normal exit through the complete owner/reaper path and uncertain delivery after
severed or withheld server replies, without a second request. These tests use
generated clients. A fresh recorded installed-game check subsequently verified
one window-close request in 2.31 seconds, with actual exit code 0, no signal and
no core dump. No signal fallback or replay occurred; previous nonzero signal-close
incidents remain unexplained. See [the recorded result](CLAUSEWITZ_VALIDATION.md#recorded-receiver-and-window-close-check--13-october-2026).

## Ordinary clock input

After recording ordinary singleplayer menu setup, bind one already focused game
window using a private `0600` JSON file. Binding does not activate a window or
send input. It requires the current owned game identity, exact `WM_CLASS` bytes,
Normal difficulty, historical AI and the selected own-player tag.

| Binding field | Value |
| --- | --- |
| `game` | Actual game identity from this run's endpoint |
| `controller` | Exclusive player identifier, e.g. `luna-linux-player` |
| `player` | Own country tag, e.g. `GER` |
| `window` | Verified owned X11 window ID |
| `window_class` | Exact complete `WM_CLASS` property as a byte array |
| `menu_evidence_ids` | IDs in the private recording/journal proving ordinary singleplayer setup |
| `research_profile` | Optional reviewed client profile below; enables ordinary Research controls |
| `production_profile` | Optional matching client profile; enables ordinary production controls |

```sh
native/target/release/hoi4-live-native --bind-input /absolute/private/run /absolute/private/input.json
```

The binding is immutable for that owner. Stop all builder input before binding.
Menu evidence is an operator responsibility; the reader does not prove every
multiplayer flag. The private binding records this limitation.

Research, Focus, construction and production accept two exact builtin profiles:

| Profile | Physical client | Logical graphics | Status |
| --- | --- | --- | --- |
| `linux_builtin_absolute_1920x1080_v1` | 1920×1080 | 1920×1080 | New profile; live and recording qualification pending |
| `linux_builtin_absolute_512x288_v1` | 512×288 | 2560×1440 | Preserved diagnostic profile; not for future campaign tests |

All enabled domain profiles must agree. The owner pins the actual X11 client
dimensions; resizing refuses subsequent keys and pointer actions before dispatch.
Fresh engine graphics and SDL/X11 association must match the chosen profile.
Coordinates come from named renderer geometry, never from scaled old pixels.
Unknown sizes, logical scaling and mixed profiles refuse explicitly. Omitting all
domain profiles preserves the legacy clock-only binding.

Future campaigns and recordings require native 1920×1080 or higher and readable
UI. The current profile does not yet support arbitrary larger sizes. Validate
actual game geometry, UI and recording performance before campaign Start;
selecting a profile alone is not live qualification.

The production preview requires the chosen owned client and matching logical
graphics. Its `production-actions.jsonl` keeps the original line identity and
count before a click. An unresolved intent blocks all shared input domains;
do not delete it, change request IDs or create a fresh binding to bypass it.
See [the tool contract](../native/README.md#tools) for arguments and recovery
limits. The separate opening tool can open production; equipment-line creation
remains unavailable.

The MCP then advertises `hoi4_input_lease` and `hoi4_time`. Acquire with
`{"op":"acquire","holder":"luna-linux-player","seconds":300}`. Only the
configured controller may acquire; the owner generates the returned token.
Renew with `{"op":"renew","lease_token":"RETURNED_TOKEN","seconds":300}`
or release with `{"op":"release","lease_token":"RETURNED_TOKEN"}`. Do not
include `holder` in renew/release. Status is `{"op":"status"}`. Leases expire
after 30–900 seconds. Observations
neither need a lease nor extend it. Competing acquisition and delayed stale
tokens are refused.

Send `hoi4_time` with `lease_token`, a stable `request_id`, and exactly one of
`paused: true/false` or internal `speed: 0..4`. It uses Space or keypad minus/plus,
checks focus and clear input state, refreshes its decision before queuing keys,
and verifies memory state. It never moves the cursor. Failed preflight remains
retryable. An uncertain dispatch quarantines new input, including a new request
ID; losing a reply never causes automatic redispatch. Reusing a verified request
ID returns its historical receipt, not a promise that state has stayed unchanged.

The guard checks process identity, own country, game roots, persisted playthrough
ID, Normal/historical settings and the observed date. Changes invalidate input.
The persisted ID does **not** prove same-campaign load generation. Do not load or
switch campaigns under this binding; use a fresh owned run. Full generation,
strategic actions and campaign acceptance remain open gates.

The recorded Luna medium clock test passed speed changes, unpause, a server wait
to 3 January 1936 and verified pause under unchanged Yama 1. It used no screenshots
or cursor movement. This verifies the installed-game clock adapter on that build;
it does not verify strategic actions or a complete campaign. See the
[actual calls and measurements](CLAUSEWITZ_VALIDATION.md#passed-luna-rust-clock-test--13-october-2026).

## Ordinary research preview

The owner may add `"research_profile": "linux_builtin_absolute_1920x1080_v1"`
to the private binding after corroborating its owned client. This requires the
exact physical/logical dimensions and builtin absolute SDL/X11 association listed
above. Installed-game qualification of this resolution remains pending. Recorded
run 019 used the older diagnostic profile and passed the assembled owner adapter with
exclusive Luna medium input: exact-ID selection/assignment, independent canonical
slot readback and counter advancement. Later runs verified panel opening and
empty-slot navigation. Full campaign acceptance remains unfinished. See
[live evidence](CLAUSEWITZ_VALIDATION.md).

`hoi4_research_attempt` takes the planned `technology_id`, own `slot`,
`stage` (`select_slot`, `view` or `research`), a separate stable `request_id`
for each stage, and `lease_token`. The new `open_panel` stage requires known
closed own views with no managed view selected and sends one ordinary `W` pair without
pointer movement. An already open panel verifies without toggling it. This stage
passed recorded run 022. `select_slot` requires an open own panel
and verifies the selected empty own tree; View/Research require that tree.
Selection retains the planned technology ID in its durable target. It accepts
no coordinates.
Recorded run 020 verified SelectSlot 0, View/Research and SelectSlot 1 with
exclusive Luna medium input and independent own-state readback. Each stage derives its point from fresh internal observations and
pins campaign, own status and full slot identities. A final read-only check
follows X11 preparation. It attempts at most one motion/left-button pair.

The `select_category` stage uses explicit `folder_id` instead of `technology_id`
and requires a selected empty own tree with no Info. It verifies the public
folder selection and makes no technology-to-folder relationship claim. Newer
full UI/Info and slot-emptiness evidence shares final witnesses and rejects
contradictions with canonical assignments. Standard tabs use their own renderer
and state fields; Drag fields stay separate.

Raw category diagnostics retain false frame/routing/input-authority flags even
when their typed Sprite fields are available. These flags never authorize a
click. The opt-in action adapter separately checks its owned profile, candidate,
lease and current state. Run 023 verified industry selection and a no-input
completion, then quarantined an electronics switch that exceeded the five-second
deadline. Do not retry an uncertain attempt.

Intent is durable before possible input. Unknown delivery gets bounded canonical
readback, never another event. Success also requires clear input state; uncertain
attempts survive restart and block new tokens/stages and clock actions. Known
local refusal leaves the plan unfinished and permits a fresh attempt token.
Verified retries return explicitly historical receipts. This does not establish
atomic frame/routing authority, confirmation-dialog support, daily automation
or campaign acceptance.

## Focus and daily previews

The private owned input binding can separately enable
`"focus_profile": "linux_builtin_absolute_1920x1080_v1"` and
`"daily_prepared_research": true`. Both default off. The daily preview also
requires the research profile. These are owner settings, not player-supplied
geometry; they require the same reviewed client profile used by research.
Installed-game acceptance at 1080p is pending.

`hoi4_focus_attempt` accepts `stage`, `focus_id`, `request_id` and `lease_token`.
Stages are `open_politics`, `close_politics`, `open_focus`, `view` and `start`. Each attempts at
most one ordinary event and verifies canonical own state. Exact Sprite,
zero-angle Text and unmasked Frame branches are supported. Unsupported renderers,
unsettled roots and incomplete local blocker observations refuse. Intent is
persisted first; uncertain input blocks clock, research, focus and daily work.
No new token can bypass that quarantine.

`close_politics` requires the requested focus to be active, settled Politics
mode12, a hidden Focus root and zero complete own detail/Info blockers. It sends
one ordinary Q and requires settled mode23 with both roots hidden and the same
active focus. Run043 verified the ordinary route with a private diagnostic;
the new Rust service and automatic worker handoff still need recorded acceptance.

Start the daily preview with an existing exclusive lease and an explicit plan:

```json
{
  "op": "start",
  "lease_token": "<existing private lease>",
  "plan": {
    "version": 1,
    "revision": 1,
    "entries": [{
      "id": "research-slot-0",
      "intent": {"kind": "Research", "technology_id": "support_weapons", "slot": 0},
      "navigation": {"folder_id": "infantry_folder"}
    }],
    "max_actions_per_day": 1,
    "max_observation_age_ms": 2000,
    "review_day": null
  }
}
```

The player cannot supply trusted process/campaign scope. Entry IDs and technology
IDs are explicit and bounded; requested slots must belong to the own current
array. Navigation prepares one ordinary panel/slot/category/view stage per
worker cycle, persisting intent and requiring fresh stage verification before
continuing. Discover keys with the read-only `hoi4_research_folders` tool, which does not
require an open research view. Start/update validate explicit hints before plan
persistence. Use canonical public folder IDs, such as `infantry_folder`, `industry_folder`
and `electronics_folder` on the verified build, rather than display labels.
An optional explicit folder hint selects the category; omitted hints
preserve legacy entry serialization. Unknown or unrelated UI refuses; the worker
does not blindly close panels or Info. Only fresh Research completion can finish
an entry within the persisted daily assignment budget. Several slots may refill
on the same day; navigation stages do not spend assignment reservations. It
neither acquires nor renews its
lease automatically. Daily focus, economy, army and naval domains remain unfinished.

Use `{"op":"digest"}` for fresh own counts and the last batch, or
`{"op":"stop"}` to join the worker. Digests stay within 2 KB and label prior
verification as historical. Stop does not pause the game or clear uncertainty.
Known read-only preparation refusals retain an entry/day backoff and the actual
bounded reason in digest/stop. This does not silently correct an existing hint.
An idle worker checks lease authority and stops on explicit ownership/session
loss or expiry; `running` alone never grants input authority. Digest reads take
the governor gate before producer, avoiding interference with post-action
verification. Counter contention alone may wait inside the existing two-second
deadline; poison is reported separately.
Private sequence and plan files preserve chronology across restarts. Use
`{"op":"update","lease_token":"<existing private lease>","plan":{...}}`
to add entries or change bounded policy with an increased revision. Exact retries
are idempotent. Updates preserve existing specifications, navigation hints,
attempts, consumed days and completion evidence; unresolved input refuses.
This preview requires installed-game verification and is not full daily
automation or campaign acceptance.

## Ownership and recovery checks

A private per-user launch lock prevents competing native supervisors. Existing
instances of the installed executable are refused rather than adopted. The owner
waits up to two seconds for the configured executable device/inode, creation
and direct-parent checks before publishing the child identity. It then
records actual child and parent identities, executable device/inode, verified
build, endpoint version and a fresh nonce. Both peers verify ordinary-user socket
credentials; the client also verifies the recorded owner's identity. Same-UID
credentials and private files are not an adversarial same-user security boundary.
The configured X11 socket's peer must match the recorded display server before
launch, preventing a display number from silently selecting a different server.

Client handling is limited to eight workers, bounded records and absolute read/write
deadlines. EOF, malformed, oversized, partial or disconnected requests stay local
to that connection. Exit monitoring continues during waits. The owner drains
active observations before reaping its child, then refuses further game reads.
Socket cleanup checks the saved inode and does not unlink a replacement socket.
The proxy never automatically retries a request after losing its reply.
Connection establishment is nonblocking and bounded; OS refusal to create a
worker drops that client without unwinding the owner or bypassing the read gate.

Each snapshot refreshes bounded process mappings and verifies ownership before
and after its typed reads. Newly allocated campaign objects can be read without
reopening the process. Permission changes, unmapping and partial kernel reads
produce errors rather than cached data. Running-game snapshots are not atomic;
changed anchors remain a typed failure. The reader privately captures the kernel
boot ID and the owned process's `AT_RANDOM` address and 16 bytes. Snapshot
boundaries compare fresh bounded samples, refusing a same-file exec even when
PID, creation ticks and executable identity stay unchanged. This is an exec
change witness, not a frozen address space or an atomic snapshot guarantee.

## Fixture evidence

The Linux generated-child tests verified two actual CLI client reconnects with
one unchanged child, malformed/disconnected/idle/oversized client survival,
nonce and stale-owner refusal, replacement-socket preservation, graceful exit
and a hung SIGTERM that kept its owner reachable. The opt-in kernel tests
verified parent reads, new mappings, permissions/unmap, partial reads, bounds,
identity changes, hash mismatch, different-image exec and child exit under Yama 1.
Three additional cases passed for same-image exec, actual random-byte changes,
and false boot/address/content witnesses. The twelve substantive kernel cases
took 101 seconds under CPU emulation; their private helper is counted separately.

Hosted Linux CI exposed a pre-exec child identity race after the first checkpoint.
The launcher now waits for the configured image; a generated child blocked on a
private pipe and then executing a different file verifies that readiness barrier.

An isolated helper also lowered only its own soft process limit to refuse a new
worker. The owner survived, drained its existing real nonce read, restored the
limit and accepted later clients. The outer suite and live game kept their
identities and limits. The final Linux ordinary suite passed 25 library checks,
12 integration checks and the isolated Xvfb check; 13 opt-in kernel checks were
excluded from that ordinary pass and ran separately as described above.

These are generated-process fixtures. They do not prove HOI4 commands or Luna
campaign acceptance. The previous native posting failure remains quarantined.

Run the ordinary suite first, then the opt-in memory tests on an ordinary Linux
x86_64 account where the existing Yama setting is 1:

```sh
cargo test --manifest-path native/Cargo.toml --locked
cargo test --manifest-path native/Cargo.toml --locked --lib \
  platform::linux::tests::generated:: -- --ignored --nocapture --test-threads=1
```

Cross-linked musl fixture executables also need a GNU build ID; pass
`-C link-arg=--build-id=sha1` through the target's Rust flags if the linker omits
it. The initial fixture run explicitly failed on missing metadata; the build
was corrected without weakening the production whitelist. Allow longer harness
deadlines under CPU emulation: the initial nine serial kernel cases took about a
minute.
