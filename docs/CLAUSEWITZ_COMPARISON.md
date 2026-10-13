# Clausewitz-MCP comparison checkpoint

11 October 2026. This is a development comparison, not proof of campaign success
or a claim that HOI4 Live Tools is faster overall. Required targets are Windows,
macOS and Linux. Linux game control remains unfinished.

Reference: [smerss/Clausewitz-MCP](https://github.com/smerss/Clausewitz-MCP), commit
`97c85437081904d5ad07bae1623ec58d1c0a5b9e`. See [NOTICE](../NOTICE) for attribution.
The benchmark used a separate reference checkout against generated test processes,
without game offsets, memory writes or injected game commands. Subsequent user
steering selected it as the engine foundation; the pinned source and first active
ported component are described in [the migration status](CLAUSEWITZ_FOUNDATION.md).

## Current licensed-game comparison — 12 October 2026

The earlier sections below preserve the 11 October synthetic checkpoint. A
licensed Linux game has since launched. One Luna medium player verified focus,
four research assignments and a day of advancement; the final pause timed out
and the adapter is quarantined. That failure prevents campaign acceptance.

The first Rust reader was compared with the guarded upstream Python adapter in
the same running German campaign, through persistent host stdio MCP connections
over pinned localhost SSH. Each runtime returned 20 fresh own-country overviews.

| Runtime | Median / maximum, ms | Boundary |
| --- | ---: | --- |
| Guarded upstream Python validation adapter | 294.9 / 390.1 | Persistent host stdio and SSH |
| Rust native observation port | 7.5 / 13.3 | Persistent host stdio and SSH |

Rust was about 40× faster for these snapshots, with no screenshot or suspension.
The Python adapter includes per-read validation overhead; this does not benchmark
unmodified upstream, prove faster commands, or establish completeness. Rust
currently exposes observations and bounded waits only. Windows/macOS native
adapters, commands and the governor remain unfinished. See [live evidence and
limitations](CLAUSEWITZ_VALIDATION.md) and [the Rust preview](../native/README.md).

## Current owned Rust clock checkpoint — 13 October 2026

A subsequent Luna medium test passed ordinary clock controls and advancement to
3 January 1936, including pause. Pause took 1.27 seconds; single-step speed changes
took 1.01–1.05 seconds through persistent stdio/SSH. Strategic actions and full
campaign acceptance remain unfinished; native engine posting stays quarantined.

Twenty paused overviews on the newer Rust supervisor measured **67.1 ms median
RPC**, with **56.4 ms median reported snapshot time**, under x86 CPU emulation.
The supervisor now checks private exec witnesses and current mappings. The 85
logical reads / 945 bytes exclude witness bytes and procfs work. These samples
have different ownership and load conditions from the 7.5 ms comparison above;
they are not a matched regression or a new upstream speedup claim. See
[the recorded evidence](CLAUSEWITZ_VALIDATION.md#passed-luna-rust-clock-test--13-october-2026).

## Current node-query comparison — 13 October 2026

Two fresh owned Normal historical vanilla Germany runs validated the same
411 own current technology keys and 423 structural control associations.
The second build reused shared RTTI and own-tree callback proofs within each
request while retaining fresh object fields and final witnesses.

| Measure | Before reuse | After reuse |
| --- | ---: | ---: |
| Logical reads per default page | 34,276 | 26,686 |
| Bytes read per default page | 512,416 | 299,776 |
| Default-page RPC latency | 409.2 ms, one baseline sample | 383.2 ms median of five samples |

The new RPC samples ranged from 344.3 to 423.8 ms, with server times 289.2–351.6
ms. Read traffic fell 22% by count and 41% by bytes. The latency samples come
from separate fresh processes under emulation, with different host load; they
support a modest, variable improvement, not a controlled throughput or upstream
speedup claim. All five full-model pages and exact key-set comparison passed.
Both runs left focus/research empty and the game paused at 1936.01.01.12, then
exited with code 0. The second recording passed ffprobe at 371.333 seconds.
No Luna player or strategic-input acceptance is claimed for these diagnostics.
A third fresh run validated vectored final-witness batching and the same exact
key set. Five default-page RPC samples ranged from 335.1 to 450.6 ms, with a
362.5 ms median and 280.5–382.5 ms server times. Logical reads and bytes remained
26,686 / 299,776; these metrics count requests rather than syscalls. The samples
overlap the previous run and do not establish a major speedup. Exact-ID raw
sprite/cache diagnostics and typed own Info state also passed. The game exited
with code 0, and its 565-second recording passed ffprobe. Two builder view-only
clicks included a background miss and a different technology than intended;
readback identified both. No ID-based action adapter or Luna acceptance is
claimed for this run.

A fourth fresh run again matched all keys and associations, with five RPC
samples 323.4–353.3 ms (median 331.5 ms) and server times 270.7–291.7 ms.
Logical reads and bytes stayed unchanged. These overlapping emulated-game samples
still do not establish a controlled speedup. Its first new scoped query refused a
hierarchy bound; refusal latency is not successful-query performance. The run
verified receiver metadata and ordinary window close, with actual exit code 0
and a valid 360.333-second recording.

A subsequent fresh run validated narrower exact-ID queries: medians 116.3 ms
for support weapons and 120.2 ms for machine tools, versus 312.7 ms for its
full-node default page. The queries have different declared scopes; these numbers
show their observed cost, not equivalent-model speedup. Exact-ID requests validate
all current-record ownership and target GUI fields, excluding unrelated GUI
payloads. The source-window/OS comparison passed at 34.1 ms RPC. The run stayed
at its initial date with no assignments, verified one ordinary view-only selection,
finalized both recording segments and exited normally after one WM close.

## Owned ID-based research checkpoint — 13 October 2026

Recorded run 019 used the Rust owned supervisor in the x86 emulated Linux VM.
One exclusive Luna medium player made a single View request and a single Research
request for `support_weapons`, at **774 ms** and **851 ms RPC**, respectively.
Both verified current canonical own state. Separate reads confirmed assignment
and counter advancement through 3 January; one server wait took 20.646 seconds.
The recorder finalized and the owned game exited normally with code 0.

These two samples are successful end-to-end research observations, not p50/p95
estimates or a matched comparison against upstream command latency. They include
fresh own-state/context checks, durable intent, typed renderer/hierarchy reads,
ordinary pointer delivery and bounded readback. The adapter requires a selected
empty own slot; automatic navigation and broader strategic automation remain
unfinished. Delivery receipts do not establish event causality. See
[the recorded gate and limits](CLAUSEWITZ_VALIDATION.md#recorded-run-019-exclusive-luna-medium-rust-research-smoke).

## Recorded clock and research checkpoint — run 025

Recorded run 025 passed clock-first controls, all four research
assignments and a second unpause/wait/pause cycle through 5 January. Its 17
fresh Verified research-stage samples ranged 101–1,560 ms RPC, including an
already-selected no-input category. Speed4 took 2,206 ms; initial unpause 304 ms
and pause 1,013 ms. These samples validate the exact owned emulated Linux run;
they do not establish a general latency distribution or an upstream command
advantage. Run024's earlier clock uncertainty remains unexplained. See
[the full result and recording checks](CLAUSEWITZ_VALIDATION.md#recorded-run-025-clock-and-four-slot-research-passed).

## Measured process reads

Both readers ran against the same generated child process and checked every
returned byte. The environment was a native ARM64 Linux Docker VM on Apple
Silicon, Python 3.13, non-root. The fixture mapped a known buffer in the reader's
conservative 47-bit address range. Each case had 20 warmups and 500 timed samples.
The upstream Linux reader uses `process_vm_readv`; ours uses a read-only pinned
`/proc/PID/mem` descriptor and verifies boot/start/executable identity before and
after each read. These measurements include those checks in our reader. They do
not measure HOI4, x86 gameplay, MCP round trips or the upstream command injector.

| Bytes | Upstream median / p95, ms | Ours median / p95, ms |
| --- | ---: | ---: |
| 8 | 0.0037 / 0.0115 | 0.2999 / 0.5295 |
| 4,096 | 0.0048 / 0.0143 | 0.2744 / 0.5370 |
| 65,536 | 0.0281 / 0.0982 | 0.3283 / 0.5558 |

Upstream is faster for these isolated reads. Our ownership validation has a
measurable cost. A bounded `read_many` implementation now amortizes that cost while preserving
identity checks around the complete batch. It validates all ranges and the
aggregate bound before reading, returns nothing on any partial read or changed
identity, and does not claim an atomic game snapshot. Layout-specific freshness
and torn-read witnesses remain necessary.

A separate warmed run measured 64 eight-byte fields per sample, with 10 warmups
and 200 samples in the same native ARM Linux environment:

| Method | Median / p95, ms per 64-field batch |
| --- | ---: |
| Upstream 64 individual reads | 0.0903 / 0.1945 |
| Ours 64 individually guarded reads | 3.9225 / 4.0692 |
| Ours one guarded `read_many` batch | 0.0959 / 0.1023 |

Batching reduced our median overhead about 41-fold in this fixture. This is a
process-reader microbenchmark, not a live-game speed claim. The separate runs
have different warm-cache and host-load conditions; compare methods within each
table, not individual reads across tables.

## Linux runtime checks

An isolated Ubuntu 24.04 amd64 container ran x86-64 programs through Rosetta.
Non-root Xvfb with Mesa llvmpipe supplied OpenGL 4.5 software rendering. A generated
child buffer was read successfully by both upstream `process_vm_readv` and
`/proc/PID/mem` primitives. These checks establish runtime primitives only.

Two limitations prevent treating this environment as a native Linux acceptance:

- `/proc/PID/exe` identifies Rosetta, not the requested x86 executable. Our exact
  native executable ownership check rejects it. It has not been weakened.
- Upstream `PTRACE_SEIZE` succeeded on the generated child, but `PTRACE_GETREGS`
  returned `EIO`. The upstream x86 register-based command path therefore cannot
  be assumed to work in this environment.

The installed game is the Mac build. A separate licensed Linux depot and a
verified supported execution environment are still needed for live comparison.
There is no successful Linux HOI4 run in this checkpoint.

## Protocol smoke

Both actual stdio servers initialized and listed tools in the isolated amd64
Linux container with no game running. Upstream advertised 107 tools, including
five debug-prefixed tools; our campaign server advertised 47. Both emitted
protocol JSON on stdout. A single cold startup/initialize/list run took 1.36 s
for upstream and 2.85 s for ours; this is diagnostic timing, not a distribution
or a steady-state latency comparison. Tool availability does not establish that
its actions work on the installed game.

## Live comparison still required

Use the same installed Linux build, starting state, difficulty and ordinary
player-visible objectives for both implementations. Keep campaign and experiment
runs separate. Record game progress per wall minute, tool median/p95 latency,
verified action success, screenshot/OCR counts, deliberate and injector-induced
pause time, stale reads, failures and recovery. Include research, focus,
construction, production, shortages, army orders, encirclements and naval
invasions. Tool count alone is not completeness or strategic competence.

The implementation work draws on upstream's compact structured state, day-clock
waits, batched operations and verified-order cache. Our current Mac UI actions
still take seconds and sometimes tens of seconds. Removing a discarded final
capture is implemented, but it does not resolve the entire latency gap. No clean
two-year acceptance or completed showcase campaign is claimed.
