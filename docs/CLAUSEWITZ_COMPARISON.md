# Clausewitz-MCP comparison checkpoint

Latest checkpoint, 22 October 2026: Production006 passed the bounded daily
production worker with budgeted 10→14 allocation across three days and completed-
plan restart without repeated input. Worker starts took 125–146 ms, digests
60–80 ms and STOP 8–17 ms; final lines took 44 ms and overview 16 ms. The game
advanced 97 hours over 73.639 seconds between the unpause response and final
overview at speed 3 under x86 CPU emulation. No pause action was sent. Two
read-only clock tears recovered in separate worker lifetimes; 20 player RPCs
had no errors. These are short-run samples, not an annual throughput benchmark,
a matched upstream comparison or proof of general superiority.
See [the Production006 evidence](CLAUSEWITZ_VALIDATION.md#production006-daily-worker-passed--22-october-2026).

Previous checkpoint, 21 October 2026: Production004 passed ordinary Linux production
opening and one-factory changes with one Luna medium player. The persistent
MCP/SSH bridge measured 2,080 ms for opening, 765/773 ms for increase/decrease,
76 ms for the exact control observation, 43 ms for final lines and 16 ms for an
overview. Historical replays took 6–8 ms; no-op checks took 37–39 ms. The journal
confirmed three input intents and no clock actions. Recording and normal close
passed. These are single paused samples under x86 CPU emulation, not a matched
upstream comparison, a general speedup claim or continuous campaign throughput.
See [the recorded production gate](CLAUSEWITZ_VALIDATION.md#production004-ordinary-factory-changes-passed--21-october-2026).

Earlier checkpoint, 16 October 2026: Run071 verified one manually queued dockyard
across advancing days. Its two Luna construction reads took 31.375/39.373 ms RPC
(25.017/32.912 ms producer). The original line identity remained stable and
produced work increased; 14 player RPCs had no errors. A single target wait
returned after 15.504 seconds. Precise UI comparison and automated placement
remain unverified; there was no matched upstream construction benchmark.
See [the scoped Run071 result](CLAUSEWITZ_VALIDATION.md#run071-dockyard-observation-and-clock-passed--16-october-2026-utc).

Run070 passed automatic Rhineland completion
followed by searched Four Year Plan assignment and duplicate-free restart/STOP.
The unpause-to-target interval was 382.121 seconds under CPU emulation. Independent
journals verified 12 ordinary Focus actions and two assignment reservations;
92 RPCs included one read-only busy response that succeeded on the permitted
identical retry. No matched upstream successive-Focus benchmark was run, so this
establishes a bounded Rust capability, not comparative speed or superiority.
See [the complete Run070 evidence](CLAUSEWITZ_VALIDATION.md#run070-successive-focus-worker-and-restart-passed--16-october-2026-utc).

Earlier, on 15 October 2026, run057's initial segment stopped early after
two busy reads. Its separately granted continuation confirmed actual Rhineland
completion, but Four Year Plan View refused before input because its candidate
was outside the owned client. Post-stop geometry closed through ten ancestors,
confirming the capacity correction in that observation; ordinary navigation to
offscreen Focus nodes was then missing. A production sample returned base equipment
keys for all 21 own lines in 67.818 ms RPC / 53.575 ms producer time. UI comparison
and allocation are unverified. Run054's truncated quantities remain unknown.
Earlier run050 verified research completion/refill in
27m 37s under x86 CPU emulation, and run052 verified actual Rhineland membership
and restart without duplicate input. These are bounded tests, with no matched
upstream completion benchmark or accepted campaign.

Run049 completed three worker assignments
across research→Focus→research in 23.668 seconds after the start reply. One explicit
target wait took 21.777 seconds including worker execution; the identical already-
matching request took 15.670 ms RPC (10.232 ms server). A 1243-byte fresh digest took
73.126 ms; two overviews took 15.649/14.818 ms. This is a small sample under x86 CPU
emulation, not a matched upstream benchmark or campaign throughput result.

The player initially stopped because the Focus summary showed only the latest
preparation. Independent saved journals proved the earlier reverse transition;
a separate restart/STOP supplement retained all completions and budget 3/3 without
replay. Recording and normal exit passed. The original upstream posting timeout
stays quarantined. Full campaigns remain open; run050's research completion/refill
and run052's Focus completion evidence are recorded below.
See [the complete run049 evidence](CLAUSEWITZ_VALIDATION.md#recorded-reverse-worker-run049--15-october-2026).

This is a development comparison, not proof of overall superiority. Required
targets are Linux, Windows and macOS; the native Windows adapter remains
unfinished and the separate macOS preview has no accepted campaign. The dated
sections below preserve earlier measurement conditions
and limitations rather than current setup instructions.

Reference: [smerss/Clausewitz-MCP](https://github.com/smerss/Clausewitz-MCP), commit
`97c85437081904d5ad07bae1623ec58d1c0a5b9e`. See [NOTICE](../NOTICE) for attribution.
The benchmark used a separate reference checkout against generated test processes,
without game offsets, memory writes or injected game commands. Subsequent user
steering selected it as the engine foundation; the pinned source and first active
ported component are described in [the migration status](CLAUSEWITZ_FOUNDATION.md).

## Licensed-game comparison — 12 October 2026

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
at that checkpoint exposed observations and bounded waits only. Later Rust runs
added ordinary clock/research/focus controls and bounded daily workers;
Windows/macOS native live adapters remain unfinished. See [live evidence and
limitations](CLAUSEWITZ_VALIDATION.md) and [the Rust preview](../native/README.md).

## Owned Rust clock checkpoint — 13 October 2026

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

## Node-query comparison — 13 October 2026

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

## Daily worker and Focus checkpoints — 14–15 October 2026

Run030 verified four-slot worker assignment, idempotent updates and restart with
the same consumed daily budget. Run036 subsequently verified exact focus View
and Start. Run041 extended the Focus worker evidence through active-focus waiting,
plan updates and two restarts, with no replay. Both the latest research and Focus
gates finalized their recordings and exited normally.

| Operation | Observed RPC time | Sample and scope |
| --- | ---: | --- |
| Selected Focus detail, run036 | 98.3 ms | One compact exact selected-window read |
| Focus View / Start, run036 | 2,483 / 1,651 ms | One verified ordinary action each |
| Daily digest, run041 | 49.7–62.4 ms | Five fresh payloads, 1,241–2,008 bytes |
| Focus-change server wait, run041 | 10.23 s | Includes worker navigation, Start and wait detection |
| Research input stages, run043 | 389.91–763.62 ms | Four individual samples after the ordinary Q diagnostic; category no-input 55.61 ms |
| Compact Focus UI, run042 | 99.85/104.88 ms | Two read-only samples, 770/807 bytes; 93.55/99.30 ms server-side |

Run041 also passed eight exact historical section lookups. Two later 45-second
waits expired normally because the game stayed paused with an active focus; their
elapsed time is waiting time, not snapshot latency. Research/focus panel switching,
actual technology/focus completion over game time and broad strategic throughput
remained unverified at this checkpoint. Run047/run049 later verified bounded
panel switching; completion over game time remains open. These results do not support an overall superiority claim.

## Generated-process read benchmark — 11 October 2026

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

## Historical Linux runtime checks — 11 October 2026

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

## Historical protocol smoke — 11 October 2026

Both actual stdio servers initialized and listed tools in the isolated amd64
Linux container with no game running. Upstream advertised 107 tools, including
five debug-prefixed tools; our campaign server advertised 47. Both emitted
protocol JSON on stdout. A single cold startup/initialize/list run took 1.36 s
for upstream and 2.85 s for ours; this is diagnostic timing, not a distribution
or a steady-state latency comparison. Tool availability does not establish that
its actions work on the installed game.

## Advancing-time Rust research gate

Run050 independently verified actual support-weapons completion, automatic
same-slot basic-machine-tools refill, one-reservation-per-day limits and restart
persistence. Target match took 27m 37s under x86 CPU emulation. Successful digest
RPC median was 64.811 ms (range 53.834–161.549 ms); nine read-only busy responses,
known zero-input navigation failures and one extra refused core reservation
remain in the evidence. Its exact original core refusal reason was not retained.
Recording and normal exit passed. This extends the Rust evidence beyond paused
assignments; no matched upstream completion/refill benchmark was run, and it
does not establish overall superiority or campaign competence.

## Advancing-time Rust Focus gate

Run052 on source `64f3634` completed Rhineland and independently verified its
engine membership on 4 February 1936. The assignment wait took 11.106 seconds;
unpause reply to Focus-change reply took 336.284 seconds under x86 CPU emulation.
The completed-membership reader itself took 18.123/19.466 ms, versus
25.889/26.666 ms including MCP transport. These are two samples, not percentiles.
Five intermediate timeout responses were explicitly historical. Background
busy/torn diagnostics persisted, but all 32 player RPCs succeeded. The adjacent
restart/STOP replies were 35.070 ms apart and preserved the original assignment
receipt and historical budget, with no duplicate action in the saved journals.
The 702.666-second recording and normal exit passed. This does not establish
successive Focus automation, matched upstream performance or campaign competence.

## Successive-Focus capacity diagnosis

Run055's paused full-geometry RPCs returned in 85.839/87.857 ms. Run056 then
reproduced the advancing gate failure with a complete original BeforeIntent
reason: count 3,806, allocated capacity 12,138 versus cap 8,192. The first assignment
wait took 12.155 seconds; unpause reply to final historical wait took 357.077
seconds, reaching 4 February 07:00 with no active Focus. A separate paused read
on 6 February 22:00 verified Rhineland in completed membership. The second Focus
was never selected or assigned. Both runs retained recordings and normal exits.
These are diagnostics of the Rust implementation, not a matched upstream
benchmark or evidence of successful successive Focus automation.

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
