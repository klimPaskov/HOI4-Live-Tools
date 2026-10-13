# Linux-first native MCP objective

User direction, 12 October 2026: stop the Mac screenshot-driven live tests and
replace the primary runtime with a fast Rust MCP based on Clausewitz-MCP.
Linux is the first live target; Windows and macOS remain required platforms.

## Ordered acceptance gates

1. Stop and preserve the owned Mac diagnostic run, recordings and settings.
   Do not launch another Mac acceptance campaign during this phase.
2. Run the pinned upstream Clausewitz-MCP against a licensed Linux HOI4 build.
   Verify its build identity, structured observations and ordinary player
   command channel. An emulated protocol or generated-process test does not
   satisfy this gate.
3. Give one `gpt-6-luna` player at medium reasoning exclusive Linux game input.
   Record actual play, action verification, latency, errors and decisions.
   FAIR remains the default; campaign automation cannot invoke debug/effects
   or use hidden enemy intelligence.
4. After upstream live verification, port its game semantics, structured reads,
   command vocabulary and automation into the Rust runtime. Expand these for
   durable plans, concise state changes, combat and naval-invasion control,
   recovery, and the wider Agentic HOI4 Modding workflows. Keep mod-QA abilities
   separate from campaign dispatch.
5. Validate Linux first, then add independently verified Windows and macOS
   process/build adapters. Never copy Linux offsets to another platform. Keep
   explicit capability errors on unsupported or unverified builds.
6. Prepare a tested public repository and readable setup/status documentation.
   Retain MIT copyright, license and per-file provenance; acknowledgements
   belong at the end of README. Audit all reachable history and release files.
   Move downstream release pins only after acceptance.

The primary live observation/action path must use process state and the normal
command queue rather than save parsing and repeated screenshots. Optional
recording, visual diagnostics and denied-access fallbacks may remain, but must
not dictate routine agent latency. Measure performance; Rust alone is not proof
of speed or correctness.

## Evidence at the direction change

- Upstream is already cloned; preserved baseline commit:
  `97c85437081904d5ad07bae1623ec58d1c0a5b9e`.
- Unmodified source provenance exists under `third_party/clausewitz_mcp`.
  Shared typed reads were integrated, but the full engine migration is unfinished.
- Upstream tables target Linux HOI4 1.19.2. The currently installed Mac game is
  1.19.3 and is not an interchangeable validation target.
- The Mac diagnostic run reached May 1936 and verified research, focus and one
  construction placement, but action latency and idle queues prevent acceptance.
- A Docker Linux environment tested upstream protocol and generated processes.
  Rosetta translation returned `EIO` for `PTRACE_GETREGS`; this environment does
  not prove the upstream command mechanism works.
- Linux HOI4 has not been launched, and no Luna Linux campaign has occurred.

The subsequent [Linux upstream preflight](CLAUSEWITZ_VALIDATION.md) passed real
x86_64 register access, generated-child calls and stdio protocol checks. It also
reproduced write-delegation, scratch-mapping and timeout-recovery failures.
Two fixes exist only in the disclosed scratch validation branch. These tests do
not satisfy the licensed-game or Luna live gates.

The user cleared the previous Mac objective and authorized this replacement
goal. The earlier campaign remains unaccepted; clearing its goal does not count
as campaign completion.

## Repository boundaries

Preserve the existing Python implementation and unrelated work during migration.
Python 3.9 compatibility remains relevant to that legacy package; the new native
runtime is Rust. Keep stdio MCP stdout restricted to protocol messages. Use one
game input owner, exact process/executable creation identity, build validation,
bounded state reads, verified commands and recoverable configuration changes.

Keep game files, credentials, recordings, logs and private machine paths outside
Git. Do not run the entire agent as root or change host SIP. Prepare and disclose
any required guest-local process-access settings. Do not treat advertised
upstream capabilities or offline tests as recorded live acceptance.

## Current continuation checkpoint

The licensed Linux game is installed and has run. One Luna medium player verified
focus, four research assignments and unpause, then advanced Germany to
2 January 1936. The final native pause timed out; that adapter remains quarantined.
The first Rust read-only stdio runtime in `native/` now reads the actual campaign.
See [live evidence and the failure](CLAUSEWITZ_VALIDATION.md) before continuing.
The older “Linux has not launched” paragraph above is historical direction-change
evidence, superseded by this checkpoint. Next: prove safe ordinary command
scheduling under a running game, then port commands and durable automation.

The subsequent Rust action journal and Linux keyboard transport have passed
restart/quarantine and isolated Xvfb fixtures. They remain library components,
not enabled live commands. The public Rust launcher now owns an actual Linux
game under unchanged Yama scope 1; frontend reads, client reconnections and the
initial Normal historical Germany snapshot passed. Its generated-process recovery
tests are separate evidence.
Integrate exclusive input leases and campaign readback, then prove reliable
running-game controls before resuming Luna acceptance. Native Post still lacks a
proved external scheduling/lifetime interface. The failed run was privately
archived and exited through owned OS shutdown; its adapter stays quarantined.

The optional Rust clock adapter now integrates an immutable owned window binding,
expiring exclusive controller leases, explicit display authentication, desired
pause/speed state, durable action receipts and fresh memory readback. Actual Linux
fixtures passed, including nonblocking busy refusal, shutdown between key pairs,
automatic pause/speed races and expired authority. Installed-game clock readback
and a fresh Luna run have now passed: speed changes, unpause, a server wait to
3 January 1936 and verified pause, using stationary ordinary keys and actual
memory readback. No screenshots, cursor movement, process suspension or
kernel-policy changes occurred. Both recording segments finalized and passed
ffprobe. This is bounded clock validation; strategic actions and the clean
two-year campaign remain unfinished. The failed native posting path stays
quarantined. See the latest live evidence before continuing.

The next bounded Luna run reached 5 January with an active research counter
increasing in the assigned own slot. A builder diagnostic selected that research
through the ordinary visible button and verified the canonical technology in
memory. Production ID-based strategic dispatch is still unfinished. New own
focus catalog and current technology-node observations passed 142 optimized
Linux fixtures, alongside 101 portable Rust tests and 952 Python tests (37
skipped). A subsequent recorded run verified all 438 own focus keys and selected
detail/closing state. Its research-node query refused an incorrect record offset;
the corrected byte-16 reader, bounded pages and own control/callback associations
then passed a fresh recorded run: all 411 own keys and 423 structural bindings.
The default page took 409 ms RPC / 360 ms server before request-local reuse.
Closed-tree queries refused promptly. Exact type checks remain mandatory;
structural binding does not authorize input. This run exited with code 0 and
both recordings passed ffprobe. Earlier game exit code 6 remains an unresolved
incident; a later clean exit does not explain it. A second fresh run confirmed
the identical key set and associations after request-local reuse: logical reads
fell 22% and bytes 41%, while measured latency improved modestly and varied.
It also exited cleanly. Bounded vectored final-witness reads and a one-ID raw
renderer/detail-state diagnostic now pass 176 standard Linux fixtures and 19
generated-child checks (18 substantive cases plus the helper), as well as 135
portable/MSRV Rust and 952 Python tests (37 skipped). A fresh recorded game
validated the same 411 keys/423 associations, exact-ID raw sprite/cache fields
and typed own Info open/closed state. Its default-page RPC median was 363 ms;
samples overlap the prior run, so no major speedup is established. A background
click and a different technology than intended were detected by readback; no
ID-based strategic input is accepted. Closed-tree queries refused, the game
exited with code 0, and its 565-second recording passed ffprobe. The first kernel
trial exposed two tests that checked exec
before its ready handshake; corrected tests passed without relaxing production
guards. The next target-scoped exact-ID reader and typed reciprocal hierarchy
diagnostic pass 201 standard Linux fixtures, 160 portable/MSRV Rust tests and
952 Python tests (37 skipped). Full-node validation remains unchanged; the
target scope excludes unrelated GUI payload and says so explicitly. Fresh live
validation remains pending, and these diagnostics authorize no input. Finish
verified strategic dispatch, then resume Luna acceptance.

The fresh target-scoped run refused its first exact-ID query because the new
reader incorrectly required a NULL catalog sentinel. Full-node reads and the
552-definition public catalog passed; no scoped hierarchy result passed. The
recording finalized, but actual normal exit code 6 remains an incident. A
constructor-verified typed-singleton correction, metadata-only input receiver
identification and explicit non-replaying window-close path now pass 164 portable
and MSRV tests, 212 standard Linux checks (211 substantive), four isolated
no-XTEST close cases, and 953 Python tests (37 skipped). Installed-game checks
for these additions remain pending. See the detailed failure and fixture gates
before a fresh immutable run. No strategic dispatch or campaign acceptance is
established by these tests.

A fresh recorded shorter-path run verified CPdxMouse metadata and the same full
411-key/423-association model. Its first scoped query passed catalog preflight,
then refused a hierarchy policy bound; the complete scoped result remains
unaccepted. One ordinary WM window-close request produced actual exit code 0
in 2.31 seconds, with no signal fallback or replay; recording passed ffprobe.
New optional hierarchy/point-source/path preflight code passes 171 portable/MSRV
tests, 225 standard Linux checks (224 substantive), four no-XTEST close cases and
953 Python tests (37 skipped). Its new observation outputs await a fresh
immutable game. Finish verified strategic input before the next Luna acceptance;
no two-year or full campaign has passed.

A conditional source-window association now passes 185 portable/MSRV and 239
standard Linux checks (238 substantive), including 119 snapshot tests. It verifies
built-in driver callbacks, window magic and reciprocal complete-registry membership,
while OS ownership, origin, event and input authority remain false. The exact Xlib
header ABI was independently compiled and checked. Its installed-game validation
is the next fresh-run gate; prior close transport fixtures remain distinct evidence.

The subsequent fresh game passed complete scoped-ID snapshots and the conditional
source-window observation, corroborated by independent X11 properties. Exact-key
RPC medians were 116/120 ms, versus 313 ms for full-node queries with different
scope. Six reciprocal ancestors passed; the owned root's 4,142 children remained
explicitly outside the current bounded hierarchy. One observed-screen view-only
selection returned the correct canonical key; no research was assigned. Closed
queries refused, both recording segments finalized, and one WM close returned
actual normal exit code 0. This is diagnostic acceptance only. Strategic actions,
daily plans, the clean two-year Luna run and full campaigns remain unfinished.

The next owned-root policy and conditional unit-scale cache diagnostic pass
194 portable/MSRV tests, 248 standard Linux checks (247 substantive) and 953
Python tests (37 skipped). Ordinary ancestor limits remain unchanged; only exact
owned graphics roots receive the separate bounded policy. Conditional cache
consistency does not establish current-frame freshness or input routing. These
outputs await a fresh immutable installed-game run. Continue with ordinary
ID-based strategic actions and canonical state readback, then Luna acceptance.

The next matching Info/Standard void-callback diagnostic and own-only economy
tool pass 217 portable/MSRV tests and 271 actual standard Linux checks (270
substantive), plus 953 Python tests with 37 skips. Economy retains named trigger
semantics and exact signed PP units; UI totals and shortages remain unavailable.
Button associations authorize no input. The fresh recorded-run gate must compare
both additions against the installed game before Luna strategic acceptance.
