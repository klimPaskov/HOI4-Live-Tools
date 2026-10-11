# Clausewitz-MCP comparison checkpoint

11 October 2026. This is a development comparison, not proof of campaign success
or a claim that HOI4 Live Tools is faster overall. Required targets are Windows,
macOS and Linux. Linux game control remains unfinished.

Reference: [smerss/Clausewitz-MCP](https://github.com/smerss/Clausewitz-MCP), commit
`97c85437081904d5ad07bae1623ec58d1c0a5b9e`. See [NOTICE](../NOTICE) for attribution.
No upstream source was copied into this repository. The reference checkout was
executed separately against generated test processes, without game offsets,
memory writes or injected game commands.

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
