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

## Standing plans and observations

An owned session can enable prepared research, Focus and durable daily plans.
Recorded run030 verified four-slot worker assignment, idempotent updates,
restart persistence and research-counter advancement. Run047 passed automatic
Focus-to-research navigation. Run049 passed research-to-Focus-to-research with
three assignments, both explicit target-wait cases and a separate restart/STOP
supplement preserving budget 3/3. Run050 verified actual support-weapons completion,
automatic same-slot refill with basic machine tools, advancing-day budgets and
restart persistence. Run052 verified actual Rhineland completion on 4 February
1936 and a same-plan restart/STOP without duplicate input.
Run070 passed the successive-Focus worker route: Rhineland completed, the worker
searched for and started the Four Year Plan, and adjacent identical-plan
restart/STOP preserved both receipts and the daily budget. Independent journals
verified 12 ordinary Focus input pairs and two assignment reservations, with no
failed navigation, unresolved input or duplicate Start. One read-only digest busy
response and recovered pre-intent torn reads remain part of the evidence.

Focus requires owner `daily_focus: true`, a verified Focus profile and
`focus_permission: true` in every start/update request. A running worker rejects
permission changes; stop/restart preserves histories and consumed shared budget.
Research has independent permission. Cross-domain navigation requires both owner
permissions and profiles. Each preparation has its own durable receipt and does
not spend the assignment budget. A Complete entry means verified assignment,
not a finished technology or Focus. Completed entries never reopen.

Fresh own state governs every action. Focus batches have a ten-second ceiling,
research five seconds, both capped by the current lease. Unknown roots/blockers
refuse. Before any Focus navigation intent or assignment reservation, a refused
well-formed UI state or a torn read may receive at most three additional fresh
observations, 100 ms apart, within one second after the first refusal and the
original deadline. Torn snapshots are discarded in full; every new read rechecks
lease, ownership, scope and day. Other reader/context errors propagate and late
positives are discarded. `focus_readiness_historical` retains the latest batch
of rejected reads in digest/details and STOP for the current worker lifetime;
it is not durable action history or retry authority. Run053 exposed clock-tick
refusals before this recovery change. Run054 observed successful recovery across
advancing days, then failed Four Year Plan View geometry before input. Geometry
policy refusals now preserve stage, scope, count, capacity and aggregate limits
within the existing durable reason bound. Run056 then identified retained Focus
position capacity 12,138 for 3,806 children. That exact ancestor class now permits
capacity 16,384 while retaining the 16,384 aggregate pointer-read limit; ordinary
containers and owned-root bounds remain unchanged. Run070 subsequently passed
successive-Focus navigation, search and assignment on the corrected reader.

Only proven BeforeIntent busy/torn reads qualify for bounded automatic retries;
a possible delivered input never does. Focus Start retries retain the same
in-flight reservation/request and original deadline. Durable causes, pending
reasons and consumed budgets survive restart. Uncertain input quarantines the
owner; changing request IDs or replacing the owner cannot clear it. Failed
persistence poisons the live controller. Private daily storage currently supports
Unix only; unsupported platforms return explicit capability errors.

Digest and STOP include a historical `assignments` summary: completed/pending/
in-flight counts, completion IDs and the latest request/sequence/date. This
survives empty worker batches and does not prove current state; use a fresh
overview for that. STOP reports the last accepted durable day and budget without
a game read. Before any accepted observation, those values remain unknown.

If accumulated history cannot fit the 2 KB digest, `history.snapshot_id` points
to the exact unabridged sanitized report. Retrieve each listed section before
another digest replaces the cache:

```json
{"op":"details","snapshot_id":"<hash from digest>","section":"focus"}
```

Details are historical, with no input/retry authority. Stale IDs return
`digest_snapshot_expired`; stop preserves the cache, restart clears it. The
`focus` section is a **latest-per-kind summary**: `preparation` contains only the
latest navigation step, alongside the latest Start, failures and wait. It is not
the complete navigation ledger. Full stage-order audits use stopped durable
journals. This distinction caused Luna's original run049 stop; independent
journals proved the earlier receipt before a separate restart supplement.

Wait for a known active Focus/current research assignment state with:

```json
{"timeout_seconds":45,"until_assignments":{"focus_id":"GER_remilitarize_the_rhineland","research":[{"slot":0,"technology_id":"support_weapons"}]}}
```

All targets must match one accepted own snapshot. The first sample counts;
already-matching work returns immediately. Either domain can be omitted, but at
least one target is required. Slot IDs must be unique and in 0–31. This option
cannot be combined with date or UI predicates. It proves current assignment,
not finished technologies/Focus or durable governor completion. Both successful
cases passed run049; run048 preserves historical-timeout evidence.

Timeouts return only the last timely accepted sample, marked
`state_historical: true`, `current_state_verified: false` and
`input_authorized: false`, with age and any deadline refusal. Late payloads are
discarded; ownership/scope errors propagate. A wait with no timely stable sample
refuses. Reads and waits never pause or confer input authority.

The independent `graph_kernel` library provides bounded components and
multi-source breadth-first search over public/generated topology. It has no
live army integration and supplies no movement or supply authority.

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
| `hoi4_production_lines` | Paged own line handles, separate signed factory counters and optional base equipment keys; key joins have fixture evidence, live comparison and allocation remain pending |
| `hoi4_technology_list` | Canonical loaded technology IDs with filtering/pagination; no eligibility inference |
| `hoi4_input_geometry` | GUI dimensions, scale, pointer and conditional source-window diagnostics; no click authority |
| `hoi4_research_folders` | Exact loaded public folder IDs, independent of open GUI; no technology mapping or eligibility authority |
| `hoi4_research_ui` | Own slot/widget joins, selected slot, retained Info windows and typed controls |
| `hoi4_research_slot_control` | One own slot's independently verified callback and control metadata; no input authority |
| `hoi4_research_category` | One public folder ID, named own tab and dense selection/callback association; optional Sprite diagnostics, read-only |
| `hoi4_research_node_detail` | One exact own key's binding, raw renderer/cache and typed hierarchy diagnostics |
| `hoi4_research_nodes` | Paged current own-tree keys and conditional control associations |
| `hoi4_focus_list` | Own canonical focus keys and selected detail diagnostics; no start authority |
| `hoi4_focus_completed` | Preview: own engine completion-membership IDs and active key without opening GUI; bounded sorted pages; actual Rhineland membership verified in run052 |
| `hoi4_focus_node_detail` | One own focus key's current-item/control associations; opt-in raw Sprite/Text/Frame diagnostics; paused Sprite closure and the later capacity refusal recorded |
| `hoi4_focus_selected_detail` | Compact exact selected window; Start is `window.start.ordinary_binding`, distinct from the node symbol; read-only with optional geometry |
| `hoi4_focus_ui` | Optional owned Focus binding: compact canonical active focus, managed mode, local roots and complete own blocker counts; no lease or input |
| `hoi4_wait_for` | Server wait, 1–50 seconds, for a date, focus/research changes, explicit assignment targets or research UI predicates |

Catalog queries accept `filter`, `offset` and `limit` (1–100). Research completed
membership is paged separately. Full models are validated before output paging;
request-local shared proofs are rechecked before return and never survive a call.
`freshness.read_calls` counts logical requests, not kernel syscalls.

`hoi4_production_lines` accepts `offset` (0–2,048) and `limit` (1–128; default 32).
It validates the entire own equipment-line collection before paging. Known
military/naval members require a reciprocal owner; other subtypes return bounded
metadata with ownership unproved. Handles are lossless hexadecimal strings scoped
to the current snapshot, not persistent IDs. Active, damaged and requested signed
counters remain separate; anomalous values are preserved and marked unavailable.
Known own lines may expose `equipment_id` as a **base equipment type key**.
It is not a custom variant or designer identity. The reader joins the line's
variant to fresh reviewed base-catalog membership, checks exact types and matching
intern keys, and reads only selected names. Unrelated definitions and variant
owner/design payloads are not traversed. Catalog state is never cached across
requests, and all join bytes participate in the final snapshot witness.

An unresolved supported line has a null key and `equipment_unavailable_reason`;
unknown line subtypes stay metadata-only. `equipment_ids_available` describes
all scoped lines, not just the returned page; `equipment_id_unavailable_count`
includes unsupported lines. Other equipment databases remain unreviewed. These
joins have generated host/Linux fixture evidence only; a live UI comparison is
pending. Custom variant identity, general production and allocation controls
remain unavailable. No production action is authorized by this read-only tool.

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

The `hoi4_focus_attempt` preview also accepts `search_field`: one ordinary click
in an empty, enabled own Focus search field. It preserves the current Focus and
verifies both keyboard and text receiver identities. Run062 verified activation
and identical-request historical replay without duplicate input. The separate
`search_query` preview types an explicit query into the already focused empty
field and requires exact text plus the selected canonical Focus ID in readback.
It currently supports 1–48 lowercase ASCII letters/spaces, with no surrounding
spaces, and requires an unmodified first keyboard group. Run063 refused before
typing because a final search-control snapshot changed. Run065 recovered one
such pre-input read, then reached an uncertain post-input deadline; its intent
remains unresolved and exact query/selection are unverified. Failure diagnostics
now retain the transport classification, bounded rejected-read causes and last
witnessed query/selection from existing reads. Before its one text dispatch,
the adapter can discard up to
four torn read-only snapshots within one second and the original deadline.
Every accepted snapshot must still satisfy all identity and input preconditions;
no input or terminal attempt is retried. Neither stage presses Enter or assigns a Focus.
`hoi4_focus_search_state` remains a read-only diagnostic.

Run066 observed the exact query but selected a different one of six matching
Focuses. The new `search_text` stage verifies text plus a complete canonical
match list (up to 64); it does not claim the requested Focus is selected.
`search_next` sends one ordinary Tab when `focus_id` is the immediate next match,
including wraparound. It requires the same query, unchanged match order, exact
selected ID and unchanged active Focus. Each step has its own durable receipt;
there is no automatic input retry. Run067 stopped before its third Tab; Run068
verified all three steps, historical replays and ordinary View of Four Year Plan.
Each initial/candidate/final pre-intent search read can
discard torn snapshots within its own four-read/one-second bound and the original
absolute deadline; input, state predicates and ownership checks are never retried.
The original `search_query` predicate and failed historical receipts are unchanged.

A daily Focus entry can opt into this route with `"focus_search": "four year plan"`.
The query must match the installed localization and the current lowercase ASCII
adapter. Omit it for existing direct node navigation. The worker retains the
assignment ID while traversing the observed order, and validates each recorded
stage on restart. It types only into the verified empty field once; query
replacement is unavailable. A changed query, match order, selected key or control
binding refuses. Run070 verified worker integration through actual Rhineland
completion, searched Four Year Plan assignment and a duplicate-free restart.

The optional `hoi4_research_attempt` preview derives exact technology/slot/category
controls from fresh own state. Its OpenPanel, SelectSlot, SelectCategory, View and
Research stages have bounded Luna live evidence, including four-slot assignment
and counter advancement. The preparer verifies renderer, callback, hierarchy and
SDL/X11 associations before deriving an ordinary point. The owner supplies the
reviewed origin profile; player arguments cannot supply coordinates.

Clock and pointer operations share one controller lock, lease and shutdown gate.
Intent persists before input. Canonical readback and clear input are required for
success; replayed receipts are explicitly historical. Unknown delivery blocks
new IDs and stages. See [research setup](../docs/OWNED_LINUX_LAUNCHER.md#ordinary-research-preview)
and [recorded results and original failures](../docs/CLAUSEWITZ_VALIDATION.md).

Selected own focus details also expose a bounded start-binding diagnostic. It
distinguishes the event start callback from the separate no-op listener. This
metadata grants no action authority; the separate opt-in service supplies the
lease, journal and verified ordinary dispatch boundary.

The optional `hoi4_focus_attempt` service provides `open_politics`,
`open_politics_from_research`, `close_politics`, `open_focus`,
`view` and `start` stages for an exact own focus ID. Its point preparer shares
canonical own-focus, active-progress, campaign and local UI witnesses. It requires
settled own roots and complete local blocker facts, then uses exact Sprite,
zero-angle Text or unmasked Frame geometry. Unknown facts and unsupported renderers refuse. These facts do not
prove global modal clearance or routing. Run036 passed open/select and one
ordinary Start with canonical active-focus readback. Run041/042 passed bounded
daily Focus execution and restart persistence. Run047 passed Focus-to-research and run049 passed research-to-Focus-to-research. Run052 verified actual Rhineland completion over game time. Successive Focus automation and campaign acceptance remain open.

Only the verified own-focus node/start producers opt into a fixed traversal
policy for the larger focus-tree container: exact generic Position ancestors
allow capacity 8192, with 16384 pointer comparisons across the whole path.
Ordinary callers retain their existing limits. Depth, reciprocal membership,
final witnesses, conditional cache checks and root1f0 conversion remain required;
root350 is diagnostic only. Run032 verified the actual ten-ancestor hierarchy,
root1f0 closure, conditional cache and exact-node selection. Other unobserved
branches remain unproved; unsupported callback associations refuse.

The optional `hoi4_daily_governor` service starts a durable explicit plan, returns
a fresh compact digest, updates its plan, or stops without changing game time.
It prepares OpenPanel/SelectSlot/optional SelectCategory/View as separately
persisted and freshly verified stages, then executes Research. An explicit
folder hint must be an exact ID returned by `hoi4_research_folders`. Start and
update validate explicit hints before storing a plan; display labels are rejected. It probes exhausted days without rewriting
the plan and completes prepared research entries within its persisted daily
assignment budget. Navigation stages are recorded separately and do not spend
that assignment budget. Plans may queue distinct technologies for the same slot
using distinct entry IDs. Updates must retain that slot's existing order as a
prefix; repeated technology IDs are rejected. Occupied heads wait without a
day-long refusal, and another slot may progress. Complete means verified
assignment, not finished technology; completed entries never reopen. Updates
preserve used reservations and exact entry
specifications; stored preparation evidence remains historical. Known read-only
preparation refusals are retained per entry/day with their actual bounded reason.
Stage errors remain visible in `last_stage_failure` after later probe errors.
Only proven pre-intent busy or torn-read failures permit up to two automatic
retries, with persisted backoff and one shared entry/day limit. Unknown delivery
stays quarantined; legacy records without a cause or retry allowance never
authorize retries. Returning from an open tree to another empty slot remains an
explicit unavailable transition.
Digest reads cannot acquire the producer during the worker’s post-action witness;
producer contention may wait within the existing two-second read deadline.
Other domains remain retained with an unavailable reason. See the
[focus and daily setup](../docs/OWNED_LINUX_LAUNCHER.md#focus-and-daily-previews).

While all eligible queue heads are occupied by different technologies, the
worker can use a fully validated canonical probe and skip the full UI read.
The probe publishes no sequence, digest, receipt or input authority. Empty or
same-target slots still require an independent full UI observation; unavailable
or torn data refuses. This saves reads in occupied-slot fixtures and adds work
when slots are idle. Run050 verified completion and same-slot refill; a matched
benchmark of this probe optimization remains unavailable.

Start callback refusals include only allowlisted producer reasons, renderer types
and tri-state binding flags. Missing fields remain unknown. Diagnostics never
override the existing callback/ownership checks or authorize retries.
Both Start event glues additionally require their alternate manager+1d8 to be
zero. A nonzero value refuses before its captured payload or renderer is read;
the existing supported callback route is the only accepted branch.

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

Upstream Luna play verified Focus, four research assignments and unpause. Its
final native posting request timed out; that adapter remains quarantined.
Later Rust Luna runs verified ordinary clock controls, canonical research and
Focus assignments, both panel transitions, explicit target waits and durable
restart behavior. Run049 completed three assignments in 23.668 seconds under
x86 CPU emulation; its matching-first-sample wait returned in 15.670 ms RPC.
Recording and normal exit passed. These are bounded samples, not a general
latency distribution or campaign-throughput benchmark.

Run050 reached the queued research target after 27m 37s of unpaused game time
under emulation. Completed membership grew from 47 to 48, adding support weapons;
basic machine tools occupied the same slot. There were three core reservations
on three days: two succeeded, and one was refused with known zero input. Its
original reason was replaced by later completion and is unavailable. The daily
limit held, and all historical navigation/read refusals remain recorded.

Run052 independently verified Rhineland in completed membership after five
historical timeout waits and a Focus-change wake on 4 February. Its final
membership read took 26.666 ms RPC; adjacent restart/STOP preserved the original
assignment receipt. STOP reported the historical last-attempt-day budget 1/1/0;
fresh advancing-day digests reported 0/1/1. Background busy/torn diagnostics
remain recorded, with no player RPC errors. Recording and normal exit passed.

Economic recovery, army/air/naval-invasion control and independent Windows/macOS live adapters
remain open. FAIR is the default; META and mod QA retain separate boundaries.

The preview `hoi4_focus_search_state` reader accepts an exact own `focus_id` and
reports the current search highlight, match counts and whether that highlight
matches the request. It validates cache indices through current own items and
canonical keys. It does not type, navigate, open a detail or start a Focus.
Run068 verified these observations alongside the separate ordinary search
actions; Run070 verified the worker route. Optional `include_control: true` adds
bounded query text and exact own search-field listener/renderer diagnostics.
Ordinary typing and Tab require the separate action adapter's focus/routing
checks; Enter remains unavailable. Run057's offscreen-node failure motivated
this prerequisite for ordinary navigation.
No clean two-year Luna run or full campaign has passed. Follow the
[acceptance gates](../docs/LINUX_RUST_HANDOFF.md),
[complete live evidence](../docs/CLAUSEWITZ_VALIDATION.md) and
[measured comparison](../docs/CLAUSEWITZ_COMPARISON.md).

Core assignment history retains each new terminal NotQueued reason in the
durable plan journal even after the entry later completes. Assignment summaries
include the refusal count and latest refusal; compact digests keep the count and
provide the exact assignment detail lookup. Older records without a saved reason
remain unknown. This changes evidence retention, not retry or budget policy.

Focus selected-detail and Start observations skip unrelated tree-node controls.
View still validates its exact node and callback. Start keeps the own-country,
canonical selection, window, named Start button, callback, visibility, geometry
and final stability checks. A narrower read does not authorize retrying an old
refused or uncertain action.

## Provenance and acknowledgements

This foundation builds on [smerss/Clausewitz-MCP](https://github.com/smerss/Clausewitz-MCP),
Copyright (c) 2026 smerss, MIT licensed. [PROVENANCE.md](PROVENANCE.md) identifies
derived files and independently recovered layouts. The complete upstream notice
is retained in [LICENSE.clausewitz](LICENSE.clausewitz) and [NOTICE](../NOTICE).
