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
An optimized Luna medium clock test passed speed changes, receipt replay,
unpause, a server wait to 3 January 1936 and verified pause. The wait took 19.60
seconds; unpause took 0.22 seconds and pause 0.76 seconds over persistent stdio/SSH.
Twenty fresh overviews measured 15.7 ms median RPC, with a 71.3 ms maximum in this
emulated guest. No player screenshots, cursor movement or process suspension
occurred. Yama stayed 1 and all inspected game threads had tracer 0.
These are bounded clock checks; strategic and campaign acceptance remain open.

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
Torn reads fail rather than return a cached or partially valid snapshot. Server
waits can retry transient torn reads within their deadline, with campaign context
and a nondecreasing accepted timeline pinned to the initial stable sample.

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
| `hoi4_research` | Own completed technology IDs, raw levels/thresholds and active signed counters; units remain unverified |
| `hoi4_technology_list` | Bounded loaded technology IDs, filter and pagination; no eligibility inference |
| `hoi4_input_geometry` | Own GUI pointer/dimension/scale diagnostics; client origin and click mapping remain unverified |
| `hoi4_research_ui` | Own slot/widget joins, selected slot, technology detail and exact button/render diagnostics; no click authorization |
| `hoi4_research_node_detail` | One exact own technology key's conditional raw renderer/cache fields and own detail state; no client rectangle or input authority |
| `hoi4_research_nodes` | Paged current own-tree keys and conditional ordinary-control associations; visibility, eligibility and routing unavailable |
| `hoi4_focus_list` | Own canonical focus keys with prefix filtering/pagination and selected detail diagnostics; no start authorization |
| `hoi4_wait_for` | Bounded wait, 1–50 seconds, for own focus/research changes, a date or research UI predicates |

The game uses a 365-day calendar, including February 28 in 1936. Its current-build
raw clock includes a 5000-year epoch; displayed hours range from 1 to 24.
Research progress and time remaining have not been normalized and return null.
Catalog queries accept `filter`, `offset` and `limit` (1–100). They validate the
canonical definition list and every returned definition's type/index/key, then
recheck the complete list and definition fields. A catalog entry is public
metadata, not proof that the own country can research it. No eligibility function
is called and no definition data is cached.

GUI diagnostics return the actual finalized physical/logical dimensions, their
ratios and the logical pointer, with explicit unverified client-origin, HiDPI,
control-transform and hit-routing fields. They do not send input or authorize
coordinate clicks. The live optimized catalog resolved `basic_machine_tools`
among 552 definitions in 88 ms over persistent stdio/SSH. The corrected
`CGameGui` reader passed in the installed game; measured pointer agreement
remains specific to the tested window and points, not universal click mapping.
The own research model checks exact primary types, status/slot ownership and both
canonical and own-array joins. Completed IDs come from the engine's membership
list, rather than assuming every positive level is complete. Empty slots have
null counters; active counters use the technology's signed 64-bit field.
Normalized progress, days remaining and eligibility remain unavailable. This
reader returned 47 completed IDs and four empty slots in the installed game.
An ordinary builder Research-button diagnostic assigned `support_weapons` and
verified raw counter 101,000. Luna subsequently advanced to 5 January and paused;
the same counter became 505,000 while completed membership remained unchanged.
This validates an active changing counter, not its normalized units or a
production strategic adapter.
Completed output accepts `offset` and `limit` (1–100), and reports total count
and next offset without skipping full model validation.
Wait snapshots bind coherent own-campaign context, reject context/timeline
changes and retry only torn observations inside the deadline. Persistent
instability returns an error. Ordinary timeout output reports the latest accepted
sample and its age; persisted identity is not load-generation proof. The wait
correction passed a fresh Luna clock wait, with no torn reads in that run;
the retry branch retains separate fixture evidence.
The own research UI reader identified all four slot widgets and their
`CButtonDrag` controls. Ordinary diagnostic clicks selected each own slot and
opened its tree, confirmed through memory. These were view-navigation checks,
not research assignments or proof of a production click adapter. A subsequent
live check read each drag's renderer at its exact field and identified plain
`CSprite` objects. That class safely returned unavailable field layout under
the earlier whitelist. Its direct-resource layout then passed in a fresh game,
with all four exact plain sprites resolving their resource to `CSpriteType`.
For a bounded UI wait, use `until_research_ui: {"view": "tree",
"selected_slot": 0}` with `timeout_seconds`. The view is `panel` or `tree`;
the optional slot is a current own-array index, not a persistent ID. Choose a UI
target or `until_date`, not both. Campaign, UI and slot identities remain pinned
across the wait. A match means active mode and local window visibility
predicates agree; it does not establish clipping, topmost routing or input
authorization. The UI wait passed in the installed game: one ordinary slot click
was followed by a verified tree/slot match in 289 ms, without repeated input.
A separate one-second wrong-view wait returned its latest stable sample and age.
The UI reader also reports retained ordinary technology Info windows. Nonnull
selections require canonical database and own-status membership. The named
`research` control must match its ordinary callback binding and exact button
type; raw disabled state and selected-control alias remain diagnostics.
Containers must belong to the owned GUI registry. Null selections and absent
containers are explicit unavailable states. Reader policies cap Info capacity
at 64, paired button/name capacity at 256, technology arrays at 8192 and names
and keys at 256 bytes; larger data is refused. These additions passed a live
disabled future-tech detail, an enabled own detail, closing visibility readback
and an ordinary builder Research-button diagnostic. They supply no assignment
adapter; `input_authorized` remains false.

The dedicated current-node query keeps its extra traversal out of ordinary clock
and UI waits. It requires the open own ordinary tree and a selected own slot,
then validates both current inline vectors, node ownership, canonical database
and own-status membership and unique public keys. Policy bounds permit capacity
2048 per vector and 512 aggregate records. Unknown node GUI classes supply type
metadata only. Membership does not establish visibility or a click target.
Queries accept an ASCII key prefix, offset 0–512 and limit 1–100 (default 25).
The full model is validated before filtering or paging. Current records use
the independently recovered item at byte 16; byte 32 is unrelated context.
Exact owned containers and Drag controls can supply a bounded callback/data
association. The container and control must belong to the own GUI registry.
Control data must equal the canonical own technology, and its single live listener must
match the independently recovered own-tree callback capture. Other layouts
remain unavailable. This structural association never authorizes input or
supplies coordinates. Node, technology and template type failures identify the
stage with bounded metadata, without exposing process addresses.
Within a request, validated shared primary RTTI and the two own-tree callback
captures can be reused. Every object primary, owner, control payload and listener
is read afresh, and all reused proof bytes are rechecked before returning the
model. No proof survives a request. This reduces duplicate reads; it does not
establish an atomic snapshot or event acknowledgement.
Final research-UI witnesses use bounded vectored Linux reads, with ownership,
exec and current mapping checks before and after the batch. A short read or
failed final check discards the entire result. Other backends retain a scalar
fallback. `freshness.read_calls` counts logical requests, not kernel syscalls;
`read_calls_kind` makes that distinction explicit. Batches are capped at 65,536
requests, 1 MiB per range and 32 MiB total, with at most 128 iovecs per syscall.

The one-ID diagnostic requires `technology_id` and an open own tree. Its narrower
scope validates public-key uniqueness and every current record's type, ownership
and canonical join before reading target GUI bindings. Unrelated GUI payloads
are outside this model; `validation_scope` and `full_gui_model_validated:false`
make that boundary explicit. Full-node queries retain their original validation.
Multiple distinct bound target items refuse. Only an exact owned
Drag/CSprite/CSpriteType branch supplies raw renderer and resource-cache fields.
Unsupported classes remain metadata-only. A bounded typed ancestor diagnostic
checks reciprocal parent/child links, own Container constructor positions and
owned graphics roots, stopping before unknown ancestor payloads. GUI hierarchy
provenance does not establish country ownership of every ancestor. Cached
dimensions, local visibility and a matching Info window do not establish a
client rectangle, effective visibility, routing or a fresh event.
A fresh recorded Linux run validated raw sprite/cache fields for
`support_weapons` and `basic_machine_tools`, including exact own-control links
and first-parent `C2dPositionObject` metadata. The diagnostic distinguished a
visible `support_weapons2` Info window from an absent `support_weapons` window,
then observed the former close. Both node tools refused after tree closure.
The view-only clicks included a background miss and a different technology than
intended; they do not prove ID-based selection. That run used the earlier full
GUI validation and first-parent metadata only; the target scope and typed ancestor
extension await a fresh installed-game check. The campaign remained at its
initial date with no assignments, exited with code 0, and recording integrity
passed. Client geometry, routing and strategic input remain unavailable.

The first target-scoped installed-game query refused an incorrect NULL sentinel
assumption before reading GUI payload. The corrected reader checks the actual
nonnull typed singleton against its fresh global and primary type, without
reading reserved payload. Full-node/catalog checks passed that run; the scoped
hierarchy extension still needs a fresh installed-game check. Its owned close
recorded normal exit code 6, which remains an incident.

`hoi4_input_geometry` additionally reports `input_source.type_encoding` when
bounded primary RTTI is available on the own idler receiver. This is metadata
only: receiver layout, virtual getter and client origin stay unverified. Full
snapshot witnesses cover all its bytes.

A subsequent run passed receiver metadata and full-node reads but refused the
first scoped query at a hierarchy size policy boundary. New source preserves
those limits and reports a valid overrun as an explicitly incomplete optional
subtree; malformed structure still refuses. The conditional point diagnostic
requires the exact receiver/getter and built-in dynamic implementation before
source coordinates. Raw source units/origin, event completion and click routing
remain unverified. Both additions await a fresh installed-game check.

The optional point diagnostic also follows a constructor-proved active SDL video
fingerprint, input-window magic and reciprocal X11 driver registry. It reports the
selected native window ID, dimensions and raw relative-mode flags only after
unique membership in a complete registry bounded to capacity 128. Other entry
payloads and the opaque display are never read. An unsupported driver or valid
policy overrun is explicitly unavailable; malformed links, pointers and torn
witnesses refuse. `owned_os_window_verified`, `client_origin_verified` and
`fresh_event_verified` remain false. This association awaits installed-game
validation and does not authorize input.

The focus query accepts an ASCII key prefix `filter`, `offset` 0–4096 and `limit`
1–100. It validates every own key before pagination, exact own-tree ownership and
global database pointer membership. Global duplicates/nulls grant no ownership.
Selected detail windows must belong to the own view and GUI registry. The named
ordinary `start` control supplies raw disabled state and flags; null selection
supplies no binding. Literal local visibility and a stricter animation predicate
are reported separately. Own capacity is capped at 4096, global membership at
20,000, detail windows at 32, named controls at 256 and keys/names at 256 bytes.
Hidden branch visibility, eligibility and input remain unavailable. A recorded
installed-game run verified all 438 own focus keys and selected detail/closing
state. That run exposed the incorrect research-node record offset in the
previous source. A fresh recorded run then validated all 411 current own
technology keys and 423 ordinary structural control associations. Its default
page took 409 ms RPC / 360 ms server, with 34,276 reads; this is the baseline
before request-local reuse. Closing the tree correctly refused further node
queries. The game exited with code 0 and both recording segments passed ffprobe.
Visibility, geometry, routing and strategic input remain unverified; see the
[live evidence](../docs/CLAUSEWITZ_VALIDATION.md).
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

The launcher also provides an explicit one-attempt `--close-window` lifecycle
request. See [its target, deadlines and delivery semantics](../docs/OWNED_LINUX_LAUNCHER.md#explicit-window-close-preview).
It sends no keys, does not focus or move the cursor, retains uncertain delivery
without replay and has no automatic signal fallback. Isolated no-XTEST fixtures
passed; a subsequent recorded game also accepted one request and exited with
code 0 in 2.31 seconds, with no signal fallback or replay. Earlier exit-code-6
incidents remain unexplained.

## Provenance

[PROVENANCE.md](PROVENANCE.md) names the upstream files used in this port.
Clausewitz-MCP is Copyright (c) 2026 smerss, MIT licensed; the complete notice is
retained in [LICENSE.clausewitz](LICENSE.clausewitz) and the project [NOTICE](../NOTICE).
