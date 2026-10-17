# Operating loop for the small player

## Capability gate

Discover the live tools, their exact argument schemas, action verification and
freshness fields. The native preview exposes `hoi4_daily_governor`,
`hoi4_wait_for` and `hoi4_time`; legacy backends have different interfaces.
Tool names do not promise that every backend implements every plan section.
Reject unsupported plan parts explicitly.
Do not submit made-up JSON for an aspirational adapter.

Session/lease, fresh own-country telemetry, verified IDs, ordinary input,
observed action results and non-pausing reads are prerequisites for daily
automation. Memory permission denial uses telemetry/UI fallback. Do not alter
SIP, sign the game, or run the entire agent as root. On missing telemetry retain
the standing plan, suppress state-dependent mutations, use permitted fresh UI
evidence, and raise a concrete capability incident.

Only one execution engine owns a given routine. Do not run a legacy standing
order engine beside a governor doing the same work. Preserve pending work across
interruptions; resume only after session, country, chronology and IDs validate.

## The standing plan

Compile strategy into the supported schema, preserving these semantics:

| Section | Durable intent | Assignment and outcome evidence |
|---|---|---|
| Research | Ordered eligible tech IDs with prerequisite/ahead-of-time policy and fallback | Assignment: exact technology in the slot. Finished research: fresh own completed membership |
| Focus | Ordered branch IDs plus conditions; never silently choose an exclusive alternative | Assignment: exact active focus. Finished Focus: fresh own completed membership; idle alone is insufficient |
| Construction | Building, state/province ID, quantity, slot/capacity and priority | Queue delta for the same destination; count already queued work |
| Production | Equipment/variant ID, factory targets, minimum floors, deficit priorities | Correct line and assignment; total allocations remain possible |
| Shortages | Repair order, buffer targets, permitted factory moves, useful trade limits | Improving net flow or relief action verified; do not equate an order with recovery |
| Military | Explicit unit/army IDs, objective, front/route, reserve and supply limits | Assignment/order exists on intended units; later outcome remains pending |
| Events | Narrow title/option or ID matches with declared choice and exception rules | Popup dismissed and consequence observed where available |
| Review | Material change predicates and latest strategic review date | Fresh digest reaches player; routine actions continue |

Separate `pending`, `eligible`, `attempted`, `verified`, `blocked` and `failed`.
An accepted click is not completion. In the native governor, `Complete` means a
verified assignment receipt; it does not mean the technology or Focus finished.
An idempotent duplicate may reuse a matching durable receipt under the adapter's
replay rules. Missing/stale evidence or uncertain delivery never authorizes a new
action ID. Inspect actual state without clearing uncertainty or resending input.

## Daily priority and compact digest

Run these checks in the background on each new verified game day:

1. Confirm chronology, country and observation freshness. Unknown is not zero.
2. Protect survival: collapsing front, imminent encirclement, threatened capital
   or sole invasion port, critical fuel/supply, severe convoy losses.
3. Execute declared routine event choices; leave an unknown real choice for the
   player. Never blanket-select the first option for a showcase.
4. Refill research/focus slots; keep construction and production working.
5. Repair shortages and supply; reinforce current formations before mass new
   recruitment; restore reserves before initiating another offensive.
6. Check war/invasion stages, assignments and objective completion. Trigger a
   strategic review only when the current intent no longer covers the situation.

Return about 1–2 KB: game date, UTC observation, source, age, plan revision,
mode, country, latest verified actions, top risks with severity and stable IDs,
blocked entries, current objective and legal concrete one-call fixes. Return
field-level unknowns. Avoid entire catalogs, repeated unchanged alerts, old
warnings presented as new and unsupported fixes.

A useful digest decision is: `rifles deficit worsening; line <observed ID>;
restore reinforcement priority; move <bounded count> idle factories; verify line`.
It is not `build more stuff` or a fabricated equipment ID.

## Event-driven cadence

At peace, provide at least a month's routine plan and wait for relevant events
or the review deadline. Research completion already handled by the queue is not
a strategic decision. War declaration, failed diplomacy, newly blocked focus,
uncovered event, major supply failure or completed theater objective is.

Use server-side bounded waits within the discovered schema (the native preview
caps an individual wait at 50 seconds). `until_assignments` proves active/current
assignment, including its first sample; it does not wait for research or Focus
completion. Date/change waits and fresh completion membership serve that separate
purpose. A timeout's last accepted sample is historical, not fresh authority.
An elapsed timeout is not a game decision or
reason to pause. Continue waiting under the declared policy while routine runs;
do not replace this with recurring screenshots and model analysis. Maintain
human progress updates during long work without disturbing the game.

During war, shorten the tactical deadline when an operation needs it, while
letting the server evaluate daily risk. Use the highest speed that preserves
timely verified control. Lower speed for a contested landing or collapsing front
when needed; speed is not a success metric. Routine observations never pause.

## Action transaction and recovery

Resolve IDs from a fresh own-state catalog; verify target, availability and
preconditions; perform one ordinary player action; read back the matching
postcondition; commit only that plan entry. Name matching is a UI bridge, not
entity identity. Refresh stale coordinates/selection instead of clicking harder.

Only positively proven zero-input or BeforeIntent failures qualify for retries
explicitly supported by the adapter and current acceptance contract. Retain the
original attempt, deadline, refusal evidence and consumed budget. Uncertain
delivery stops routine input; reconnecting or replacing IDs cannot clear it.
On repeated safe refusal, retain the failed target and evidence and continue
independent routine only when the contract permits. Do not
`on_fail=skip` a necessary focus, research, invasion or production commitment.

Pause is justified only when an unhandled decision prevents safe continuation,
or a bounded UI transaction genuinely requires it. Record why and restore the
prior running state. A lost state alone does not demand pausing if the standing
defense policy covers it. An external blocker such as missing permission is
reported concretely; it is not repaired by taking over another user's process.

## Evidence and terminal states

Keep recording, call log, manifest, incident log and campaign journal outside
Git. Monitor recording health and disk headroom. A paused/failed recorder is an
evidence incident, not proof that the game crashed. Preserve actual process and
exit evidence. Check recording segments with ffprobe and visual playback.

Journal each milestone with game date, UTC, relevant IDs, actual before/after,
mode, and evidence location. Save checkpoints at meaningful milestones or to
resume a real interruption; do not serialize the world every observation.

Victory requires the country card's territorial/war/peace predicates, shown in
fresh player-visible evidence. Record unmet goals, defeat, timeout, crash and
external interruption honestly. A two-year run proves its tested interval only;
offline unit tests never prove live campaign acceptance.

Distinguish a campaign's progress from an acceptance attempt. If a required
automation/capability is absent, or a failure requires manual rescue outside the
declared clean-acceptance contract, mark that clean attempt failed with the
specific reason. Independent safe campaign/development work may continue, but
cannot erase the failure or retroactively make that attempt clean. After repair,
record a separate clean acceptance interval/run. A bounded verified retry
already covered by the contract is not automatically an acceptance failure.
