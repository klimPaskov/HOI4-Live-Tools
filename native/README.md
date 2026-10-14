# Native Rust development preview

Linux-first HOI4 process observations and ordinary controls, based on
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
| Linux x86_64 | Owned launch/reconnects, exact-build reads, waits, clock keys, research, and opt-in focus/daily previews |
| Windows / macOS | Portable protocol and fixtures; native live adapters return explicit unavailable errors |

Three-OS CI validates shared code. It does not establish Windows/macOS live
compatibility. Layouts must be recovered and verified separately for each build.

Focus stages and a daily prepared-research worker now have optional owned MCP
services. Their installed-game validation is pending. They share the clock and
research controller, lease, shutdown gate and uncertainty journals. The worker
retains unfinished entries and requires fresh assignment receipts; navigation
receipts cannot complete research work. It prepares each research stage separately
and supports idempotent plan updates. It completes at most one research entry per
advancing day. Daily focus, economy, army and naval adapters remain unfinished. Private daily storage
supports Unix only; other platforms return an explicit capability error.

## Connect an owned Linux game

Follow [owned Linux launch setup](../docs/OWNED_LINUX_LAUNCHER.md). The supervisor
launches its own game with `--serve PRIVATE_CONFIG`; stdio clients reconnect with
`--connect RUN_DIRECTORY`. It requires an explicitly owned authenticated X11
display and refuses to discover or take over an unrelated game.

Executable readiness retains the child's creation/parent/user pin through
transient Linux `exec` link disappearance. Only executable-link `NotFound` is
retried within the original two-second deadline; success requires two matching
path/device/inode observations. Permission errors and ownership changes refuse.

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
| `hoi4_army` | Own divisions with full snapshot-scoped handles, selected province joins and lossless raw status; recorded structural reads, units unverified |
| `hoi4_economy` | Own named factory trigger counts, civilian project capacity and exact signed political-power units; live comparison pending |
| `hoi4_technology_list` | Canonical loaded technology IDs with filtering/pagination; no eligibility inference |
| `hoi4_input_geometry` | GUI dimensions, scale, pointer and conditional source-window diagnostics; no click authority |
| `hoi4_research_folders` | Exact loaded public folder IDs, independent of open GUI; no technology mapping or eligibility authority |
| `hoi4_research_ui` | Own slot/widget joins, selected slot, retained Info windows and typed controls |
| `hoi4_research_slot_control` | One own slot's independently verified callback and control metadata; no input authority |
| `hoi4_research_category` | One public folder ID, named own tab and dense selection/callback association; optional Sprite diagnostics, read-only |
| `hoi4_research_node_detail` | One exact own key's binding, raw renderer/cache and typed hierarchy diagnostics |
| `hoi4_research_nodes` | Paged current own-tree keys and conditional control associations |
| `hoi4_focus_list` | Own canonical focus keys and selected detail diagnostics; no start authority |
| `hoi4_focus_node_detail` | One own focus key's current-item/control associations; opt-in raw Sprite/Text/Frame diagnostics, awaiting live validation |
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

Wait reads check the original deadline before starting and after completion.
A successful blocking read returned at or after that deadline is refused rather
than labeled a reached target. Identity errors remain immediate and never retry;
retry sleeps consume only the remaining budget. Reads do not pause the game.

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
Clock actions use one ten-second deadline capped by the original lease. Each
delivered key still requires its exact intermediate state within 1.5 seconds;
reads returned after either deadline cannot verify success. Blocking OS reads
are not forcibly interrupted. Uncertain input remains quarantined without replay.
It never activates a window or moves the cursor. Bounded X11 delivery is separate
from authoritative game-state readback.

The portable research-point library computes conditional Sprite/Text interior
candidates with explicit scale/profile guards. It does not register input
commands or grant frame/routing authority. A recorded builder diagnostic verified
ID-derived selection and assignment. An explicit owner profile can now enable
the bounded `hoi4_research_attempt` preview. In recorded run 019, an exclusive
Luna medium player opened and assigned `support_weapons` by ID. Separate reads
confirmed slot 0 and its advancing counter through 3 January 1936. View and
Research RPCs took 774 and 851 ms. The recorded source required the
own empty slot tree to be selected first. A new `select_slot` stage can derive
the ordinary slot control from an open own panel. Recorded run 020 verified
Luna selecting slots 0 and 1, assigning research, and advancing its counter
through 3 January; all four strategic RPC samples were under one second.
Recorded run 022 verified ordinary `W` panel opening from known closed own views
with no managed view selected, followed by slot selection and research. A distinct
repeat request verified without another key. Recorded run024 verified a complete
industry/electronics/infantry cycle, a same-folder no-input request, and all four
research assignments. Its later speed-change request became uncertain, so
reliable combined clock/research play remains unfinished.
Its internal observation adapter checks the requested own key/slot and fresh
renderer, callback, hierarchy and SDL/X11 associations before candidate maths.
The owner supplies the reviewed origin profile; player arguments cannot supply
coordinates. Clock and pointer operations share one controller lock, lease and
shutdown gate. Intent persists before input; unresolved delivery blocks new IDs
and stages. Canonical readback plus clear input is required for success; replayed
receipts are labeled historical. See [preview setup](../docs/OWNED_LINUX_LAUNCHER.md#ordinary-research-preview).

Selected own focus details also expose a bounded start-binding diagnostic. It
distinguishes the event start callback from the separate no-op listener. This
metadata grants no action authority; the separate opt-in service supplies the
lease, journal and verified ordinary dispatch boundary.

The optional `hoi4_focus_attempt` service provides `open_politics`, `open_focus`,
`view` and `start` stages for an exact own focus ID. Its point preparer shares
canonical own-focus, active-progress, campaign and local UI witnesses. It requires
settled own roots and complete local blocker facts, then uses exact Sprite,
zero-angle Text or unmasked Frame geometry. Unknown facts and unsupported renderers refuse. These facts do not
prove global modal clearance or routing. Installed-game focus dispatch is pending.

The optional `hoi4_daily_governor` service starts a durable explicit plan, returns
a fresh compact digest, updates its plan, or stops without changing game time.
It prepares OpenPanel/SelectSlot/optional SelectCategory/View as separately
persisted and freshly verified stages, then executes Research. An explicit
folder hint must be an exact ID returned by `hoi4_research_folders`. Start and
update validate explicit hints before storing a plan; display labels are rejected. It probes exhausted days without rewriting
the plan and completes prepared research entries within its persisted daily
assignment budget. Navigation stages are recorded separately and do not spend
that assignment budget. Updates preserve used reservations and exact entry
specifications; stored preparation evidence remains historical. Known read-only
preparation refusals are retained per entry/day with their actual bounded reason.
Digest reads cannot acquire the producer during the worker’s post-action witness;
producer contention may wait within the existing two-second read deadline.
Other domains remain retained with an unavailable reason. See the
[focus and daily setup](../docs/OWNED_LINUX_LAUNCHER.md#focus-and-daily-previews).

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

Recorded run 025 passed clock-first control, all four research assignments, then
unpause/advancement/pause with the research panel open. All 17 research stages
verified fresh canonical state. The recording passed ffprobe and the owned game
exited normally with code 0. Earlier run024 uncertainty remains unexplained;
this bounded run does not establish campaign or governor acceptance.

New owned-root/cache diagnostics have passed portable/MSRV and actual Linux
fixtures. A fresh recorded game verified owned-root closure and conditional cache
checks, and observed own economy counts. Precise economy UI comparison remains
pending. Its Research button used a separate text renderer. A subsequent recorded run
verified the Text and own-division structural diagnostics and an ID-derived
point sequence; Frame diagnostics still await a matching installed-game branch. Detailed test counts,
failures, recordings and measurement limits are in [live validation](../docs/CLAUSEWITZ_VALIDATION.md)
and [the comparison](../docs/CLAUSEWITZ_COMPARISON.md).

Research ID stages have bounded live evidence. Focus actions, durable daily
execution, economic/army/naval-invasion control, recovery and independent
Windows/macOS adapters remain unfinished.
FAIR is the campaign default; META strategy and mod QA retain separate boundaries.
No clean two-year Luna run or full campaign has passed acceptance. Follow the
[Linux acceptance gates](../docs/LINUX_RUST_HANDOFF.md).

## Provenance and acknowledgements

This foundation builds on [smerss/Clausewitz-MCP](https://github.com/smerss/Clausewitz-MCP),
Copyright (c) 2026 smerss, MIT licensed. [PROVENANCE.md](PROVENANCE.md) identifies
derived files and independently recovered layouts. The complete upstream notice
is retained in [LICENSE.clausewitz](LICENSE.clausewitz) and [NOTICE](../NOTICE).
