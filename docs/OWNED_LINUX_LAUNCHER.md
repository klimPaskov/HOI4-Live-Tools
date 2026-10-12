# Owned Linux launcher preview

The Rust supervisor launches one verified Linux game and remains its ordinary-user
parent. MCP clients connect over a private local socket, so closing or restarting
a client does not relaunch the game or remove its owner. Observations execute in
that parent process and never attach, suspend, capture screenshots or parse saves.

This is development source. Three observation tools are available. An explicit
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

```sh
native/target/release/hoi4-live-native --bind-input /absolute/private/run /absolute/private/input.json
```

The binding is immutable for that owner. Stop all builder input before binding.
Menu evidence is an operator responsibility; the reader does not prove every
multiplayer flag. The private binding records this limitation.

The MCP then advertises `hoi4_input_lease` and `hoi4_time`. Acquire with
`{"op":"acquire","holder":"luna-linux-player","seconds":300}`. Only the
configured controller may acquire; the owner generates the returned token.
Renew or release with that token. Leases expire after 30–900 seconds. Observations
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
