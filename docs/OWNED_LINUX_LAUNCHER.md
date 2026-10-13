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

The settings seed is copied into new isolated userdata. The only game launch
argument is `-userdir=...`; the fresh playset enables no mods. Default user
settings and playsets are never modified. Game output goes to a private file.

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
| `research_profile` | Optional owner-reviewed `linux_builtin_absolute_512x288_v1`; omission keeps clock-only input |

```sh
native/target/release/hoi4-live-native --bind-input /absolute/private/run /absolute/private/input.json
```

The binding is immutable for that owner. Stop all builder input before binding.
Menu evidence is an operator responsibility; the reader does not prove every
multiplayer flag. The private binding records this limitation.

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

The owner may add `"research_profile": "linux_builtin_absolute_512x288_v1"`
to the private binding after corroborating its owned client. This narrow profile
requires a 512×288 client, 2560×1440 logical graphics and the verified builtin
absolute SDL/X11 association. Other profiles are unavailable. Omission preserves
clock-only input. Recorded run 019 passed the assembled owner adapter with
exclusive Luna medium input: exact-ID selection/assignment, independent canonical
slot readback and counter advancement. Automatic slot navigation and campaign
acceptance remain unfinished. See [the bounded live gate](CLAUSEWITZ_VALIDATION.md#recorded-run-019-exclusive-luna-medium-rust-research-smoke).

`hoi4_research_attempt` takes the planned `technology_id`, own `slot`,
`stage` (`select_slot`, `view` or `research`), a separate stable `request_id`
for each stage, and `lease_token`. `select_slot` requires an open own panel
and verifies the selected empty own tree; View/Research require that tree.
Selection retains the planned technology ID in its durable target. It accepts
no coordinates and performs no automatic panel or category navigation.
The new SelectSlot adapter is fixture-tested and awaits recorded live validation. Each stage derives its point from fresh internal observations and
pins campaign, own status and full slot identities. A final read-only check
follows X11 preparation. It attempts at most one motion/left-button pair.

Intent is durable before possible input. Unknown delivery gets bounded canonical
readback, never another event. Success also requires clear input state; uncertain
attempts survive restart and block new tokens/stages and clock actions. Known
local refusal leaves the plan unfinished and permits a fresh attempt token.
Verified retries return explicitly historical receipts. This does not establish
atomic frame/routing authority, confirmation-dialog support, daily automation
or campaign acceptance.

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
