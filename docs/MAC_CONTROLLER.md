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
worker qualification remains pending.

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
- `update` requires a stopped worker and a compatible higher plan revision.
- After reconnect, acquire a new lease and explicitly start the identical
  retained plan. Unfinished entries and action reservations remain intact.

The older `hoi4_plan_run` remains a separate synchronous runner. It shares the
input gate; it is not the durable worker described here.

## Action limits and recovery

Research, Focus and production use one original five-second action deadline, narrowed
by the existing lease and cancellation flag. Nested reads, panel preparation,
geometry checks and completion proof cannot renew that deadline. Production
settles the complete row/button/rectangle witness across two reads 60 ms apart,
within the original deadline. The final guard retains that exact candidate
after pointer movement and reports which part changed. A bounded read
that finishes after expiry is discarded. Click cancellation during pointer
settling prevents mouse-down; release cleanup remains unconditional after a
possible dispatch. Reads never move the cursor.

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
readback. The new typed durable adapter remains a default-off preview; successive
natural completion and mixed-plan live qualification are still required.

Generated tests cover persistence, ownership, chronology, slot waits, typed
proofs, unchanged allocations, original deadlines, cancellation and shutdown.
Live results belong in [CLAUSEWITZ_VALIDATION.md](CLAUSEWITZ_VALIDATION.md).
The two-year Luna run, broader plan domains and full recorded campaigns remain
required; a bounded worker test does not establish campaign acceptance.
