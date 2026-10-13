# Native Rust development preview

This is the first Linux observation port of the Clausewitz-MCP foundation.
It is development source, not a replacement release for the existing Python MCP.

The runtime reads the owned player's clock, focus and research directly from
process state. It emits compact stdio MCP JSON and waits inside the server for
own-state changes or a requested date. It never pauses for an observation.
No screenshots, saves, process discovery or raw memory addresses are exposed as
tools. Unknown progress and unavailable subsystems remain explicit.

## Build and protocol check

Use Rust 1.86 or newer:

```sh
cargo build --manifest-path native/Cargo.toml --release --locked
cargo test --manifest-path native/Cargo.toml --locked
cargo clippy --manifest-path native/Cargo.toml --all-targets --locked -- -D warnings
```

Run `native/target/release/hoi4-live-native` as a stdio MCP server. With no session
it supports initialize, ping, discovery and capability reports; game observations
return a capability error. Logs and setup errors go to stderr.

The shared protocol builds on Linux, Windows and macOS. Linux live process reads
are implemented. Windows and macOS native process adapters return an explicit
unavailable error; compiling on those systems is not live compatibility proof.
A three-OS CI matrix runs shared protocol and fixture tests.

## Owned Linux observations

The Linux preview can launch its own game with `--serve PRIVATE_CONFIG` and
accept reconnecting stdio clients through `--connect RUN_DIRECTORY`. See
[owned Linux launch setup](../docs/OWNED_LINUX_LAUNCHER.md). It requires an
already owned X11 display and the exact verified game build below. The launcher
and process-reader fixtures pass. The installed Linux game also ran under its
actual Rust parent at unchanged Yama scope 1, with two reconnecting clients;
this does not establish campaign acceptance.
The subsequent Luna medium clock test passed speed changes, unpause, advancement
to 3 January 1936 and pause. It used actual memory readback, with no screenshots,
cursor movement or process suspension. Pause took 1.27 seconds over persistent
stdio/SSH; single-step speed changes took about one second. The guest's Yama scope
stayed 1 and all inspected game threads had tracer 0.

The diagnostic `--session OWNED_RUN_MANIFEST` path remains available. Its
manifest records these actual launch facts:

| Field | Meaning |
| --- | --- |
| `game` | `pid`, `/proc` `start_ticks` string, absolute `exe`, numeric `uid` |
| `supervisor` | The same identity fields for the game's actual parent |
| `build_id` | Recorded executable GNU build ID |
| `sha256` | Recorded full executable SHA-256 |

The adapter checks the actual ordinary-user identities, parent relation,
executable device/inode, full file hash, GNU build ID and mapped ELF header.
It captures a private boot/`AT_RANDOM` witness and checks it at snapshot boundaries
to refuse same-image exec address-space replacement.
It does not discover or take over another running game. PID replacement fails
explicitly, and observations check ownership and campaign anchors at both ends.
Torn reads fail rather than return a cached or partially valid snapshot.

The only live-tested layout is Linux x86_64 build ID
`a4971c9e8ac9a6f2487dea109e01f9839ee59db7`, SHA-256
`56fc2945cdbf2475a316db3285eb3458ae5d577db149db6dfdba6f004de3a88a`.
Own-country reads have been exercised in Germany. Every other build is refused.
These offsets are not Windows or macOS offsets.

Process-access denial returns `permission_denied`. The public runtime does not
invoke sudo or change process-access settings. The private non-parent Linux
validation environment used a disclosed, timed guest-local access lease; the
native launcher reads in the actual ordinary-user parent, including its worker
threads. Generated-child tests exercised that access under unchanged Yama scope
1. Do not run the whole runtime as root.

## Implemented tools

| Tool | Result |
| --- | --- |
| `hoi4_capabilities` | Platform, session binding and explicit missing features |
| `hoi4_overview` | Fresh own focus/research IDs and clock; read metrics and unavailable fields |
| `hoi4_wait_for` | Bounded wait, 1–50 seconds, for own focus/research changes or `until_date: [year, month, day]` |

The game uses a 365-day calendar, including February 28 in 1936. Its current-build
raw clock includes a 5000-year epoch; displayed hours range from 1 to 24.
Research progress and time remaining have not been normalized and return null.
The mode default is FAIR; a mode label alone is not proof of campaign settings.

## Command port and expansion

The owned Linux supervisor can optionally enable `hoi4_input_lease` and
`hoi4_time` after an explicit verified window and campaign binding. See
[clock input setup](../docs/OWNED_LINUX_LAUNCHER.md#ordinary-clock-input).
The standalone observation server has no command adapter. The action library records a
durable, exclusively locked receipt before dispatch, verifies the desired state
through an injected observation backend, and preserves an uncertain dispatch
across restarts. A new request ID cannot bypass quarantine. Failed preflight
leaves an entry retryable; a verified receipt is returned without redispatch.
The clock backend uses this journal; the daily governor and strategic command
adapters remain unfinished.

The Linux X11 fallback building block sends only Space, keypad minus and keypad
plus. It checks the owned process and window, focus, class and clear keyboard
state. It never activates a window or moves the cursor, and display operations
have a bounded deadline. Transport delivery is not a verified game action.
Each clock action requires an exclusive configured-controller lease and fresh
own-campaign state checks. Authentication uses the explicit private Xauthority
file and verifies the actual display peer before transport writes. The separate authenticated Xvfb fixture proved key
press/release delivery, unchanged pointer position and refusal on focus loss,
wrong window PID or held keys. It is **not** a HOI4 live test.

On Linux, install Xvfb and xauth and run the opt-in transport check:

```sh
python3 native/tests/run_x11_fixture.py
```

The helper owns and removes its separate fixture display and authentication file.
It does not launch HOI4 or use the normal desktop display.

Private upstream validation verified
ordinary focus/research/speed/unpause, but a final running-game pause timed out
inside posting. Restoring CPU context and detaching threads does not undo engine
side effects. The failed adapter is quarantined; the transaction must not be
blindly repeated or ported as safe. See [the actual failure and measurements](../docs/CLAUSEWITZ_VALIDATION.md).

Next steps are strategic installed-game controls, broader recovery,
durable daily plans, idempotent research/focus/economy repairs, compact event
digests, army and naval-invasion control, then independently validated Windows
and macOS adapters. FAIR remains the campaign default; META strategy and mod-QA
abilities must retain distinct policy boundaries. No clean two-year run or full
campaign has passed acceptance.

## Provenance

[PROVENANCE.md](PROVENANCE.md) names the upstream files used in this port.
Clausewitz-MCP is Copyright (c) 2026 smerss, MIT licensed; the complete notice is
retained in [LICENSE.clausewitz](LICENSE.clausewitz) and the project [NOTICE](../NOTICE).
