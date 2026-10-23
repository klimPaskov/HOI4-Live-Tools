# Durable Mac controller and background plan

The native Mac controller retains its campaign identity and unfinished daily
plan across stdio server restarts. The optional background worker shares the
same controller and lease as manual tools. It never pauses the game.

Run the preview with an exact owned session and a private input directory:

```sh
hoi4-live-native --session /private/run/session.json --mac-input /private/run/input
```

The directory must be private to its owner (mode `0700`). Its `input.json` names
the configured controller and own country. Worker permissions default to false:

```json
{
  "controller": "player",
  "player": "GER",
  "daily_research": true,
  "daily_production": true,
  "daily_focus": false
}
```

Binding creates `controller-epoch.json` only for a directory without previous
nonempty action or daily history. It records the original epoch, exact process
creation timestamp, own player, playthrough, context roots, game setup and the
controller's highest observed date. Reopening retains the original nonce and
plan, but restores neither a lease nor a running worker. Competing writers,
replaced state files, different campaigns and date regression refuse.

A same-date check validates the retained identity without rewriting or syncing
it. State files and action history are private runtime data and stay outside Git.
Older directories with history but no controller identity refuse automatic
migration; replacing their files is not a recovery procedure.

## Background worker preview

Use `hoi4_daily_governor` after acquiring the ordinary exclusive lease. `start`
requires `lease_token` and an explicit `plan`. For example, this research-only
plan queues two technologies in the same slot:

```json
{
  "op": "start",
  "lease_token": "<current lease>",
  "plan": {
    "version": 1,
    "revision": 1,
    "entries": [
      {"id": "electronics", "intent": {"kind": "Research", "technology_id": "electronic_mechanical_engineering", "slot": 2}},
      {"id": "radio", "intent": {"kind": "Research", "technology_id": "radio", "slot": 2}}
    ],
    "max_actions_per_day": 2,
    "max_observation_age_ms": 1000,
    "review_day": null
  }
}
```

Resolve technology IDs, available slots and prerequisites from the owned game;
this example is not a universal strategy or permission to replace busy slots.
The worker preserves ordered queue heads and waits for natural completion before
refilling a slot. An entry becomes Complete when its assignment is verified;
that does not mean the technology itself has finished.

The default-off `daily_focus` preview admits explicit `Focus` entries with a
canonical `focus_id`; capabilities report `focus_enabled`. One typed observation
binds the own campaign, country/status, complete Focus membership and exact target.
The ordinary UI path must retain that binding before preparation and through
its final target checks. Completion of an entry means a verified idle-to-active
Start. A later idle queue head requires fresh completed membership for the
previous assignment; an unexplained idle Focus stops for a decision. Reconnect
uses the historical key only as a hint and independently binds current membership;
it does not claim that historical object pointers survived. Successive live
worker qualification passed a recorded Mac mixed component gate; broader
campaign qualification remains pending.

Production additionally requires `plan.production_permission: true`. An entry
uses `kind: Economy`, with a `Production` target containing the exact observed
`line_id` (the full `line_handle`), `equipment_id` and `total_factories`.
The preview permits an existing military line's matching allocation or one
factory increase/decrease, with one production reservation per game day. It pins
the original line before panel preparation and verifies all own line identities
and requested allocations around the single click. It cannot add lines, operate
naval lines, or borrow Linux panel layouts. Other plan domains and navigation
hints explicitly refuse.

The worker checks the own clock once per second. Same-day busy Focus and slot checks do
not allocate observation sequences or sync plan files. A new game day or an
available queue head triggers a fresh durable observation and bounded action.
Review deadlines, completed assignments, blocking decisions and lease/ownership
loss stop the worker. Terminal notifications integrate with server-side waits.

- `digest` returns fresh own observation data with separately marked durable
  assignment, budget and worker history.
- `stop` cancels, wakes and joins the worker. It returns history and leaves the
  game clock unchanged.
- `update` requires a stopped worker and a compatible higher plan revision; it
  persists that revision and starts the worker immediately. Preserve completed
  entries and queue ordering. Consumed daily action budget carries forward.
- After reconnect, acquire a new lease and explicitly start the identical
  retained plan. Unfinished entries and action reservations remain intact.

The older `hoi4_plan_run` remains a separate synchronous runner. It shares the
input gate; it is not the durable worker described here.

## Construction observation foundation

`hoi4_construction_target` is a compact read-only tool shared with the inspected
Linux layout. It binds a canonical public building and own state to the complete
queue in one witnessed snapshot. Typed location and amount facts precede public
JSON and pagination. Unavailable building identity cannot prove a zero total;
partial evidence returns null amounts. The legacy Mac counter uses this observer,
removing its former first-128-row limit.

The separate read-only `hoi4_construction_ui_probe` now retains typed own
panel/selection/choice facts alongside the same catalog, state and full queue.
It checks independently mapped view/idler/manager links and complete bounded
choice membership before reading the requested button. Hidden or missing paths
do not expose a target control. Mac containers, grids and the requested button
must retain the own GUI-registry link before dependent payload. Root local
transition states remain separate from engine visibility; neither authorizes
input or proves settled geometry. Unverified modes, geometry, settled
readiness and placement authority remain explicit;
this probe is not connected to worker input.

Optional `include_callback` binds only the requested item's exact single
observer, Legacy glue and member capture. All other callable slots must be empty,
and override/notification/input-handler paths refuse. Its byte-wide
`disable_requested` flag is separate from visual state 3; neither proves a fresh
prerequisite evaluation. Callback binding stays inside the original witness
bracket, without executing the callback or authorizing input. Default reads add
no callback access. A recorded ordinary T opener and subsequent read-only samples
verified the visible four-grid list and requested building callback. The separate
conversion item is retained only as the own shared-grid singleton; its embedded
callback is never treated as a building template. Ordinary selection has passed
the bounded component test below; placement remains unqualified.

Independent `include_geometry` now adds the requested button's own renderer,
full client containment, visible idle ancestors and ordinary UI gates inside that
same witness bracket. Nonnull graphical parents must join the exact own item
frame and renderer; null parents require no attachment reads. The profile is
native/logical 1920×1080 at scale 1. `include_routing` requires geometry and adds
complete bounded source/hit lists and cursor-cache correlation. Only active
entries count toward the 4,096-entry limit; the complete allocation header still
has the reader's separate 20,000-capacity bound. Neither option evaluates fresh
prerequisites or proves atomic cache completion, physical delivery, global modal
clearance or input permission. Default reads retain their previous memory access.

The observed total is not a durable assignment receipt. The legacy adapter still
discards private typed identity/context between calls, and its UI placement stages
are not connected to the worker. A future adapter must retain and revalidate those
identities across ordinary input. Readback success does not authorize placement.

## Find and placement readback foundation

`hoi4_construction_find_view` now dispatches to independently recovered Linux
or Mac layouts. The Mac path verifies own campaign/production, registry/manager,
Find/idler and window ownership in one witnessed snapshot. It reports finite
camera coordinates and an optional own state's canonical template anchor.
A foreign target exposes no anchor payload. Hidden views skip retained queries
and rows. A visible first row must match the first result entry and current state
catalog; count zero returns no current first row even if an old cache is retained.
Other result kinds expose metadata without following their objects.

The Mac layout uses its own 0x40-byte result entries, camera owner link and
embedded edit receiver. Optional `include_focus: true` checks the exact named own
box, complete child membership, embedded receiver and keyboard/text aliases within
the same witness. Default reads skip these fields; hidden/unmaterialized views
skip the retained box entirely. A recorded one-F qualification harness verified
visible empty-query focus with unchanged campaign state and sub-millisecond MCP
readback. It creates no released Find action permission. Typed retained evidence
pins original ownership/state while allowing independently validated current
window reconstruction. The separately retained navigation adapter below now has
one owned-state typing/results/activation component test. Settled/current-frame,
wrapped-map equivalence, general input routing and placement eligibility remain
pending.

The internal `SelectionAnchor::placement_anchor` additionally requires the exact
requested ordinary template already selected. Its replacement queue observation
validates original non-queue edges, catalog and selection before new row payload,
then completes the same witness and process checks. It returns Pending for a
complete unchanged queue. Success requires exactly one matching old line amount
increment or one matching new amount-one line in the requested own state, with
all original row identities, amounts and relative order otherwise unchanged.
Unsupported/incomplete queues or any unrelated mutation refuse.

This foundation has no registered placement action or worker permission. A future
adapter must retain the original lease/deadline, prove actual state/map targeting,
persist intent before possible input and preserve uncertainty on failed readback.
An observed queue delta alone does not prove input delivery or completed building
construction. Legacy placement remains a separate unqualified path.

## Construction opener qualification

`hoi4_construction_panel_probe` accepts independent `include_shortcut: true`.
It checks the named button's current plain-T binding and the supported keyboard
receivers before it, including nonmatching controls that could consume the key.
Focused text, unknown predecessors and changed owners refuse. The complete
source collection and inspected bindings share the final read witness. These
observations do not prove atomic cache completion or physical input delivery.

The manual `hoi4_construction_open` preview additionally requires
`construction_panel: true` in the private `input.json`, plus `request_id` and
`lease_token`. This permission defaults off. It retains the original typed panel,
button and shortcut route under one five-second deadline bounded by the lease.
The intent is synced before at most one ordinary T down/up. Already-selected
panels only wait for visibility; they never receive a toggle. No mouse, Escape,
popup dismissal, placement, pause or modifier-repair event is sent. Key-up is
allocated before key-down and released without a new ownership check.

Optional `already_open_only: true` requires the panel to be selected, visible and
locally idle at the original observation. It also forbids key dispatch if later
preparation reports a closed panel. This is enforced natively, so an earlier
harness preflight is not relied on to suppress input. The mode records transport
`none` and has a distinct action identity; reusing an ordinary-opener request ID
with this option fails with an idempotency conflict. Omitted or false preserves
the existing ordinary-T action and historical receipts. Other value types refuse.
The same original deadline, lease, campaign checks and final readback still apply.

Each opener proof may replace up to three wholly discarded `torn_read`
observations, with 2 ms waits clipped to the original action deadline. The
retained panel and shortcut binding must still match. Coherent changes, other
read errors and boundary-check failures stop immediately. Input dispatch stays
outside this recovery loop, so recovery cannot send another key. Errors retain
the observation phase, attempt count and elapsed time.

This adapter uses strict version-2 records in the shared game-action journal.
Verified retries return a historical receipt and explicitly do not certify the
current panel. An uncertain attempt permanently blocks this controller; later
reached UI cannot settle it, even through the legacy API. A positive pre-input
refusal remains recorded and permits a new request after its cause is repaired.
The key transport itself now supplies positive `NotQueued` only for failed
checks or event allocation before entering key-down. These failures post neither
down nor a cleanup up. Once down is entered, transport errors remain unknown;
successful posting is also unacknowledged and requires the original attempt's
fresh panel readback. Error codes and unchanged UI never establish no delivery.
An old uncertain receipt cannot be reclassified using this newer transport.
The complete original typed binding stays in memory for the attempt; its SHA-256
and compact scope are retained in the private intent. One recorded ordinary T
attempt now has a same-attempt Verified receipt. Its broader gate failed at the
subsequently corrected choice reader. A later recorded continuation verified exact
historical receipt replay and the guarded already-open path with zero new keys,
retaining the original failure and successful opener. The separate selection
component is described below; placement and worker construction integration
remain pending. Legacy adapters are preserved.

## Construction selection qualification

`hoi4_construction_select` additionally requires `construction_selection: true`
in private `input.json`. This strict boolean defaults off. Pass `building_id`,
`state_id`, `request_id` and `lease_token`. The state must be owned and controlled
by the player; here it binds the original scope, not a map-click destination.
This tool does not select a state, place construction or run the daily worker.

Initial canonical reads retain typed process/campaign/own-state, catalog, exact
ordinary target and complete ordered queue identity. Mode 0 never follows an
inactive template pointer; mode 1 must name a canonical ordinary template.
Unsupported selection modes and incomplete or unsupported queues refuse. Queue
progress may advance normally, while identity, amounts and membership must match.
Private journal evidence includes compact scope and a fingerprint; serialized
JSON is not used as authority for later readback.

If the exact template is already selected, fresh canonical evidence can verify
it without reading old choice controls or sending input. Otherwise the initially
open panel, requested callback, button and rectangle remain fixed through one
ordinary move/click. After cursor settling, the OS cursor and complete observed
route must match the requested target and ordinary gates. These caches are not
atomic delivery acknowledgments. A successful receipt additionally requires this
attempt's fresh canonical mode-1 target and unchanged complete queue.

The original five-second deadline is narrowed by the lease; recovery replaces
only discarded torn reads. Intent is durable before cursor movement. Errors in
the possible-input stage remain uncertain, even if no button delivery is known.
There is no input retry or later settlement. Historical Verified replays report
`current_selection_verified: false`. Move/down/up events are allocated before
movement; the last guard follows allocation and release needs no allocation
following possible down.

The recorded Luna-medium test passed with one ordinary move/click, no screenshots,
clock calls or observation pauses. Its selection RPC took 240.087 ms at native
1920×1080. Same-attempt readback verified the original template with the full queue
unchanged. An already-selected zero-input path has fixture coverage but has not
yet had its own live component gate. This is selection qualification; placement,
continuous-play acceptance and full campaigns remain unfinished.

## Construction Find qualification

`hoi4_construction_find_state` requires `construction_navigation: true` in private
`input.json`, a strict boolean that defaults off. Arguments are `building_id`,
`state_id`, `request_id` and `lease_token`. It starts only with an already-open,
locally idle, exact focused Find box, empty query and inactive construction
selection. It never opens Find, clears text, selects a building or places it.
The target must be owned and controlled by the player; the building binds the
original catalog and complete queue, without asserting placement eligibility.

The original process/campaign, Find/window, complete control membership, exact
named edit receiver, owned state/template and map pair remain fixed. Both focus
aliases and the exact expected decimal prefix are checked freshly before each
ordinary digit. Return additionally requires the full query and the current
first result joined to the original state pointer and ID. It is not enough for a
replacement state to have the same numeric ID. After Return, success requires
Find closed and camera horizontal coordinates equal to that original map pair.
Hidden/teardown readback skips the old edit payload; a replacement root refuses.
The complete construction queue and selection are checked both before and after
each Find proof, including pending reads and final verification.

The entire attempt shares one original five-second deadline narrowed by the
lease. Every digit and Return is posted at most once using preallocated down/up
events with unconditional release. Only complete torn observations can be
replaced; keys are never retried. A refusal before the first possible key records
NotApplied. Any later refusal preserves Dispatching and quarantines input across
reconnects. Later camera/query observations cannot certify that attempt.
A successful same-ID replay is historical and reports
`current_camera_verified: false`; it appends nothing and sends no input.

The recorded Luna-medium native 1920×1080 component test typed `64` and Return,
verified Brandenburg's original camera anchor and replayed the same request.
The action RPC took 549.335 ms and the full 17-call driver took 2.172 seconds.
No cursor movement, screenshots, activation, observation pauses or construction
changes occurred. This does not enable worker construction, establish a map-click
point, or qualify arbitrary modal routing and campaign integration.

## Action limits and recovery

`hoi4_input_geometry` can separately observe cached Mac source coordinates with
`include_source_point: true`. This strict boolean defaults off and changes no
default reads or controller permissions. It validates the own SDL window, exact
mouse getter/current dispatch, initialization and ordinary absolute mode inside
the complete witnessed geometry read. Linux refuses this option before snapshot
or memory access. The typed foundation can retain the original provider and
window for a future adapter; public JSON never becomes input authority.

The stationary test measured 0.257 ms median RPC and found that the cached point
differed from the OS cursor. Initialization and mouse focus do not prove the
latest event. A future click requires separate original-scope OS/client/cache
correlation, GUI/scene routing and same-attempt action readback. This option sends
no input and grants no current-event, frame, delivery or placement authority.

Each Mac worker cycle admits at most one durable input intent under its original
five-second deadline, narrowed by the existing lease and cancellation flag. The
new-day readiness scan still skips waiting or unavailable entries; an active
Focus cannot starve ready Research or production. After an attempt, later entries
retain their Pending state. Existing same-day continuation can handle them in a
later cycle while the shared daily budget permits. A zero-input refusal still
consumes its reservation and ends that cycle. Linux retains its existing batch
behavior. Nested reads, panel preparation,
geometry checks and completion proof cannot renew that deadline. Production
settles the complete row/button/rectangle witness across two reads 60 ms apart,
within the original deadline. The final guard retains that exact candidate
after pointer movement and reports which part changed. A bounded read
that finishes after expiry is discarded. Click cancellation during pointer
settling prevents mouse-down; release cleanup remains unconditional after a
possible dispatch. Reads never move the cursor.

The pre-input clock guard may replace a discarded `torn_read` observation up to
three times, with 2 ms waits clipped to that same deadline. Stop, lease and exact
process identity are checked around each observation and wait. Coherent scope
changes and other errors refuse immediately; late results cannot authorize input.
This recovery repeats only reads, never a UI action or an uncertain intent.

The typed production adapter applies the same bounded discarded-read recovery
to complete canonical observations and final target checks. The final check
retains the original line, allocation and captured button; a coherent difference
refuses immediately even after a discarded sample. Movement and click dispatch
stay outside the recovery loop. Failure diagnostics identify the phase and read
stage, attempts and elapsed time while preserving the underlying error. They do
not establish delivery or resolve any earlier uncertain attempt.

The worker persists its intent before any UI preparation. Research completion
requires a fresh, typed empty-to-assigned transition in the exact own slot.
Focus requires a fresh exact idle-to-active transition with unchanged canonical
binding and monotonic completed membership, followed by a newer durable own
observation. A receipt from a previous invocation cannot satisfy this proof.
Production uses its typed ordinary-action journal and original-line progress
record. Historical JSON receipts cannot complete a new attempt.

Clock, gameplay and retained daily uncertainty block competing input. The daily
state is loaded even when worker permissions are disabled. Missing, malformed,
replaced or foreign history cannot be bypassed through another lease or restart.
Possible-input failures remain quarantined; no automatic retry, reset or rollback
is offered. Worker shutdown still joins if its wake notification fails, and a
stop notification cannot conceal retained uncertainty.

## Qualification limits

`hoi4_focus_input_probe` inspects an exact unfinished own Focus in an already-open
idle tree without an input controller. Its default path checks every item for
ownership and ambiguity, then reads only the requested node's geometry. Optional
`include_pan: true` also validates complete obstacle geometry. It reports payload
logical reads separately from whole-observation time, which includes final
witnesses. It never opens a panel, moves the cursor, pauses or authorizes input.
Other platforms explicitly refuse this Mac-only diagnostic.
The optional pan result includes `required`, a predicted `plan` (`from`/`to`, or
null when unnecessary), and `prediction_only: true`. This is geometry, not proof
that a drag occurred or the node became visible.

Manual node readiness and click checks use the same target-only path. An actual
pan still requires a fresh settled complete Tree and revalidates that entire
candidate after pointer movement, before mouse-down. The lighter node result
cannot authorize a background drag.

On the reviewed Mac build, the Focus view's country field is a nullable override.
A nonzero value must equal the independently witnessed own country; zero resolves
through the already validated positive player tag and country map. Observer
fallback is unavailable. This does not make an unbound view ready: exact view,
own country/status/tree, canonical item membership and final witnesses still
apply. This corrects the preceding assumption that zero meant an opening view.
The investigation followed a recorded readiness timeout with the German tree
visible; the exact field value during that wait was not retained.
The pure planner examines at most 1,232 origins, retaining 24-pixel widget
clearance and inset endpoints. It moves only axes outside the safe area and
chooses the available travel closest to the needed displacement. An already
visible center needs no pan; a blocked grid refuses. Full-tree settlement and
the original deadline still govern the actual input.

Mac Focus progress now comes from one complete typed observation; it no longer
merges separately witnessed public JSON pages. The public completion tool keeps
its existing pagination. This model is not an input receipt or a durable Focus
adapter. Manual Focus now pins own node/button identities and rechecks complete
candidates after cursor settlement under one original five-second deadline.
Panning requires complete obstacle geometry and a fresh origin check before
mouse-down; later motion retains ownership checks and release cleanup. The first
recorded gate expired with an unresolved intent while the tree was open. That
controller remains blocked. Subsequent errors include a static stage label,
stage/total elapsed time and requested pan count; a requested pan does not prove
mouse-down was delivered. Stage observation windows are bounded by the original
deadline and have distinct timeout codes. Native ownership refusals retain their
cause even if the read finishes late. These diagnostics cannot explain an earlier
binary's failure. The revised bounded planner passed a fresh recorded manual
Economic Growth assignment in 3.65 seconds, with independent active-Focus
readback. The typed durable adapter remains a default-off preview. A subsequent recorded
mixed gate verified successive natural Focus completion, Research refill, a
factory increase and same-plan restart. A same-controller higher-revision gate
also passed, retaining old receipts and the consumed daily budget while filling
three empty Research slots and adding successive Focuses. Broader routine action
batches, the two-year run and full campaign qualification remain required.

Generated tests cover persistence, ownership, chronology, slot waits, typed
proofs, unchanged allocations, original deadlines, cancellation and shutdown.
Live results belong in [CLAUSEWITZ_VALIDATION.md](CLAUSEWITZ_VALIDATION.md).
The two-year Luna run, broader plan domains and full recorded campaigns remain
required; a bounded worker test does not establish campaign acceptance.
