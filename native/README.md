# Native Rust development preview

Linux-first HOI4 process observations and ordinary clock controls, based on
Clausewitz-MCP. This is development source; there is no Rust release or completed
campaign acceptance yet.

The primary path uses fresh process state and compact stdio MCP JSON. Reads and
server waits never pause the game. No screenshots, save parsing, raw memory
addresses, arbitrary process writes or injected engine calls are exposed as tools.

## Build

Use Rust 1.86 or newer:

```sh
cargo build --manifest-path native/Cargo.toml --release --locked
cargo test --manifest-path native/Cargo.toml --locked
cargo clippy --manifest-path native/Cargo.toml --all-targets --locked -- -D warnings
```

Run `native/target/release/hoi4-live-native` as a stdio MCP server. Without a
session, initialize, ping, tool discovery and capabilities work; game observations
return a capability error. Logs and setup errors go to stderr.

| Platform | Current native support |
| --- | --- |
| Linux x86_64 | Owned launch/reconnects, exact-build process reads, bounded waits and optional ordinary clock keys |
| Windows / macOS | Portable protocol and fixtures; native live adapters return explicit unavailable errors |

Three-OS CI validates shared code. It does not establish Windows/macOS live
compatibility. Layouts must be recovered and verified separately for each build.

## Connect an owned Linux game

Follow [owned Linux launch setup](../docs/OWNED_LINUX_LAUNCHER.md). The supervisor
launches its own game with `--serve PRIVATE_CONFIG`; stdio clients reconnect with
`--connect RUN_DIRECTORY`. It requires an explicitly owned authenticated X11
display and refuses to discover or take over an unrelated game.

The only live-tested game layout is Linux x86_64:

- GNU build ID: `a4971c9e8ac9a6f2487dea109e01f9839ee59db7`.
- SHA-256: `56fc2945cdbf2475a316db3285eb3458ae5d577db149db6dfdba6f004de3a88a`.
- Own-country observations have been exercised in Germany. Other builds refuse.

The diagnostic `--session OWNED_RUN_MANIFEST` path accepts these recorded facts:

| Field | Meaning |
| --- | --- |
| `game` | `pid`, `/proc` `start_ticks` string, absolute `exe`, numeric `uid` |
| `supervisor` | Same identity fields for the actual game parent |
| `build_id` | Executable GNU build ID |
| `sha256` | Full executable SHA-256 |

The reader checks ordinary-user ownership, creation identity, parent relation,
executable device/inode, full hash, build ID and mapped ELF header. A private
boot/auxiliary-vector witness rejects same-image exec replacement. Snapshot
boundaries recheck identity, mappings and observed fields; torn or short reads
fail rather than returning a partial model. Final research-UI witnesses use
bounded vectored reads, with ordered scalar fallback on other backends.

Installed-game parent reads work under unchanged Yama scope 1. Access denial
returns `permission_denied`; the runtime does not invoke sudo or change kernel
policy. Do not run the whole agent as root. Persisted campaign identity and
anchors do not establish a load-generation token.

## Tools

| Tool | Result |
| --- | --- |
| `hoi4_capabilities` | Platform, owned-session binding and explicit missing features |
| `hoi4_overview` | Own focus/research IDs and clock, freshness and unavailable fields |
| `hoi4_research` | Own completed technologies and active signed counters; normalized progress remains unavailable |
| `hoi4_army` | Own divisions with full snapshot-scoped handles, selected province joins and lossless raw status; live comparison pending |
| `hoi4_economy` | Own named factory trigger counts, civilian project capacity and exact signed political-power units; live comparison pending |
| `hoi4_technology_list` | Canonical loaded technology IDs with filtering/pagination; no eligibility inference |
| `hoi4_input_geometry` | GUI dimensions, scale, pointer and conditional source-window diagnostics; no click authority |
| `hoi4_research_ui` | Own slot/widget joins, selected slot, retained Info windows and typed controls |
| `hoi4_research_node_detail` | One exact own key's binding, raw renderer/cache and typed hierarchy diagnostics |
| `hoi4_research_nodes` | Paged current own-tree keys and conditional control associations |
| `hoi4_focus_list` | Own canonical focus keys and selected detail diagnostics; no start authority |
| `hoi4_wait_for` | Server wait, 1–50 seconds, for a date, focus/research changes or research UI predicates |

Catalog queries accept `filter`, `offset` and `limit` (1–100). Research completed
membership is paged separately. Full models are validated before output paging;
request-local shared proofs are rechecked before return and never survive a call.
`freshness.read_calls` counts logical requests, not kernel syscalls.

The current game uses a 365-day calendar, including February 28 in 1936. Its raw
clock includes a 5000-year epoch and displays hours as 1–24. Raw research counters
are verified observations; normalized progress, time remaining and eligibility
remain unavailable. Unknown is not zero.

Waits pin own campaign context, UI anchors and a nondecreasing accepted timeline.
Only torn observations retry within the deadline. A timeout reports its latest
accepted sample and age; persistent instability returns an error. Example UI
predicate: `until_research_ui: {"view":"tree","selected_slot":0}`. A slot index
is tied to the current owned array, not a persistent ID. Local window predicates
are not effective visibility or input routing.

## GUI diagnostics

Readers require exact current-build primary types, own registry/status links,
canonical technology joins and named ordinary controls before known payloads.
Unsupported classes remain metadata-only. Node-detail scope validates all current
record ownership and public-key uniqueness, then only the requested GUI payload;
`full_gui_model_validated:false` makes that narrower scope explicit.

Typed hierarchy checks reciprocal child membership and constructor-created own
Container positions. Ordinary ancestors have capacity/aggregate limits of
2,048/4,096; only exact independently owned graphics roots permit 8,192/8,192,
including preceding inspected entries. Sibling pointers are compared without
reading their payloads. Valid policy overruns retain an explicit incomplete
subtree; malformed structure or changed witnesses refuse the result.

A closed typed path can report conditional unit-scale cache consistency using
recovered float32 translations and sampled traversal flags. A matching cache
does not establish current-frame freshness or atomic publication. Incomplete,
nonunit and skipped branches remain unavailable.

The conditional point observer validates the built-in receiver/getter, dynamic
implementation and reciprocal X11 source-window registry before reporting a
native window ID, dimensions and relative-mode flags. Independent OS comparison
corroborated one installed-game profile. Client origin, effective clipping,
topmost routing, fresh events and strategic input authority remain unverified.
A local visible Info window or disabled-button flag is not assignment proof.
Only a unique locally visible matching own Info can supply the ordinary Research
button's named Standard/void-callback and renderer-ownership diagnostics.
Standard's renderer field and void callback differ from the node Drag control;
unknown variants remain unavailable. This reader sends no input.
Exact Frame and Instant branches also report class-specific resource aliases.
Only a verified Frame resource supplies bounded local grid, orientation and
signed 64-bit handle/cache facts; pending lazy caches remain unavailable.
These facts do not calculate a click point or establish frame freshness.

The economy tool requires exact own-country/tag and production/politics owner
joins before payload. Factory values retain their recovered named-trigger
semantics. Civilian project capacity is separate from idle builders. Political
power reports signed raw units and an exact rational at 100,000 units per point.
UI factory totals, shortages and final stability/war support remain unavailable.

## Ordinary controls and lifecycle

An explicit owned window/campaign binding can enable `hoi4_input_lease` and
`hoi4_time`; see [clock setup](../docs/OWNED_LINUX_LAUNCHER.md#ordinary-clock-input).
The observation-only server has no command adapter. Each clock action needs an
exclusive configured-controller lease, a stable request ID, fresh own-state
checks and a durable receipt before dispatch. Failed preflight remains retryable;
a verified request returns without another event. An uncertain dispatch survives
restart and blocks replay, including attempts with a new request ID.

Clock transport sends only Space and keypad minus/plus. It checks exact process,
window PID/class, focus, clear keys/modifiers and the authenticated display peer.
It never activates a window or moves the cursor. Bounded X11 delivery is separate
from authoritative game-state readback.

The separate Linux pointer library permits one bounded motion/left-button pair
for a future verified adapter. It pins client/root geometry and ownership,
reports uncertain delivery without replay and does not focus, park or scroll.
It registers no raw-click MCP tool. Isolated client and severed-reply tests
validate transport only; strategic game-state verification remains separate.

The launcher also has an explicit one-attempt `--close-window` request. It sends
no keys and has no automatic signal fallback or replay. See [shutdown semantics](../docs/OWNED_LINUX_LAUNCHER.md#explicit-window-close-preview).
Owned installed-game closes returned actual exit code 0; earlier exit-code-6
incidents remain unresolved.

On Linux, the separate opt-in transport fixture owns its Xvfb/authentication and
never uses the normal desktop or HOI4 display:

```sh
python3 native/tests/run_x11_fixture.py
```

## Live evidence and remaining work

One Luna medium player verified upstream focus, four research assignments and
unpause. The final native pause timed out; **that posting adapter is quarantined**.
It must not be blindly retried or ported as safe.

Later Rust Luna tests verified ordinary clock changes through 5 January 1936,
including research-counter advancement. Recorded builder diagnostics validated
438 own focus keys, 411 technology keys and 423 structural associations. Exact-key
RPC medians were 116/120 ms for a narrower model, versus 313 ms for full nodes;
those scopes are not an equivalent-model speed comparison.

New owned-root/cache diagnostics have passed portable/MSRV and actual Linux
fixtures. A fresh recorded game verified owned-root closure and conditional cache
checks, and observed own economy counts. Precise economy UI comparison remains
pending. Its Research button used a separate text renderer; the new text/Frame
and own-division diagnostics await a fresh immutable installed-game check. Detailed test counts,
failures, recordings and measurement limits are in [live validation](../docs/CLAUSEWITZ_VALIDATION.md)
and [the comparison](../docs/CLAUSEWITZ_COMPARISON.md).

Strategic ID-based actions, durable daily plans, economic/army/naval-invasion
control, recovery and independent Windows/macOS adapters remain unfinished.
FAIR is the campaign default; META strategy and mod QA retain separate boundaries.
No clean two-year Luna run or full campaign has passed acceptance. Follow the
[Linux acceptance gates](../docs/LINUX_RUST_HANDOFF.md).

## Provenance and acknowledgements

This foundation builds on [smerss/Clausewitz-MCP](https://github.com/smerss/Clausewitz-MCP),
Copyright (c) 2026 smerss, MIT licensed. [PROVENANCE.md](PROVENANCE.md) identifies
derived files and independently recovered layouts. The complete upstream notice
is retained in [LICENSE.clausewitz](LICENSE.clausewitz) and [NOTICE](../NOTICE).
