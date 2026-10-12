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
Luna smoke passed selected ordinary actions and one day of advancement, but its
final pause timed out. The Rust command channel remains unavailable pending
safe scheduling and recovery proof. See [actual live evidence](CLAUSEWITZ_VALIDATION.md).

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

1. Port pure engine components into package-relative modules with Python 3.9+
   compatibility and lazy native dependencies. Preserve provenance for each port.
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
