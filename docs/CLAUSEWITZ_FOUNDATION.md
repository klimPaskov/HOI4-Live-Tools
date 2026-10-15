# Clausewitz foundation and extensions

The user selected Clausewitz-MCP as the foundation on 11 October 2026. Reuse its
game model, structured observations, command vocabulary and automation rather
than developing another parallel game engine. Keep the upstream MIT license and
file provenance; acknowledgement is at the end of the project README.

## Implemented migration checkpoint

The pinned runtime source and MCP entry point are in
[`third_party/clausewitz_mcp`](../third_party/clausewitz_mcp/README.integration.md).
The commit and per-file hashes are recorded in `UPSTREAM.json`. No binary caches,
game dumps or generated game tables were imported. The upstream source remains
unchanged so subsequent ports can be reviewed against an exact baseline.

The Clausewitz typed-memory primitives now supply Windows, macOS and Linux
readers through `hoi4_live/engine/clausewitz/memory.py`. Each platform still owns
its native handle, identity validation and bounded exact reads. No platform is
automatically attached by importing the shared engine. This is the first active
component, **not completion of the game-engine migration**.

The first Rust observation port now lives in [`native/`](../native/README.md).
It ports this foundation's typed reads, own-country focus/research semantics and
calendar, with independently recovered current Linux build layouts. A real
upstream Luna smoke passed selected ordinary actions and one day of advancement,
but its final pause timed out; that native-posting adapter remains quarantined.
A subsequent Rust Luna medium test passed ordinary X11 clock controls, advancement
to 3 January 1936 and verified pause. Later runs verified exact-ID research
panel/slot/category navigation and canonical assignment. Run030 passed four-slot
daily worker assignment, idempotent updates, budget-preserving restart and
research-counter advancement. Run041 passed worker-driven Focus Start, waiting
on another active focus, idempotent updates and two restarts without replay.
Its fresh digests stayed below 2 KB and retained exact historical detail
lookups. Run042 verified compact own UI observations and restart budgets.
Run047 passed worker-only Focus-to-research navigation and assignment. Run049
passed the reverse route as part of a three-assignment plan, both explicit
target-wait cases and a separately recorded restart/STOP with budget 3/3. Its
original player stop over a missing summary receipt remains preserved; independent
stopped journals proved the actual earlier transition.

This remains a development preview. Actual technology/Focus completion and queued
refill, economic recovery, warfare and native Windows/macOS live support remain
unfinished. Catalogs and targeted renderer diagnostics alone grant no input
authority. See [actual live evidence and failures](CLAUSEWITZ_VALIDATION.md);
no clean two-year run or full campaign has passed.

## Expansion contract

| Area | Foundation and required extension | Evidence required |
| --- | --- | --- |
| Platform support | Clausewitz game semantics over Windows, macOS and Linux adapters; no cross-platform offset reuse | Native read/control checks and recorded runs per supported build |
| Observation | Structured state, compact fresh digests and event-driven waits; telemetry fallback | Field agreement with own-country telemetry/UI; unknown and torn-read tests |
| Player actions | Upstream ordinary command vocabulary, batch submission and verified readback | Installed-build eligibility and actual resulting state; queued is not completed |
| Autonomy | Day-driven execution, durable standing plans, resumable idempotent work and shortage repair | Long unattended runs without empty routine queues or builder rescue |
| Warfare | Army grouping/fronts, supply, encirclements, air/naval coordination and invasion sequence | Actual encirclement and successful landing/reinforcement outcomes |
| Play styles | FAIR default, explicit META, comprehensive country playbooks | FAIR Germany/China and META Poland recorded showcases |
| Mod QA | Separate debug/test features and repeatable reports | QA tests plus proof campaign dispatch cannot reach debug/effect routes |
| Reliability | Ownership, capability discovery, recording, checkpoints and config restoration | Process replacement, denied access, interruption and restart tests |
| Performance | Bounded batches and changed-state waits, minimal screenshots and no cursor parking | Per-action p50/p95, progress rate, verified success and pause time |

## Integration gates

1. Port engine components into the Rust runtime, preserving provenance and
   explicit platform boundaries. Retain the legacy Python 3.9+ package during
   migration; importing shared code must not attach to a game or load foreign APIs.
2. Replace upstream implicit PID discovery and unbounded reads with existing
   owned adapters. Bind layouts to independently verified executable build/ABI.
3. Adapt structured own-country observations and the event loop. Missing data
   remains unknown; hidden enemy memory must not enter campaign observations.
4. Port ordinary command handlers only after build-specific constructors,
   eligibility and result readback are verified. Keep memory writes, injected
   debug calls and unverified commands outside campaign dispatch.
5. Connect durable plans, shortage repair and military automation to those
   handlers. Keep a telemetry/UI fallback for native capability denial.
6. Run the same live objectives against upstream and the expanded implementation.
   Publish outcomes and failures; do not infer superiority from copied code,
   advertised tools or offline test totals.

[Measured comparison](CLAUSEWITZ_COMPARISON.md) records the current component
results and Linux runtime limitations. No completed campaign is claimed.
