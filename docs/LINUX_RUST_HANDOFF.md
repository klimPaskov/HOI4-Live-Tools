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


The bounded pointer transport and Frame/Instant resource diagnostics now pass
227 portable/MSRV and 284 actual standard Linux checks (283 substantive), plus
953 Python tests with 37 skips. The first Linux pointer fixture refused locally
because requests had not left x11rb's buffer; a local-only flush fixed it before
any game input. Full-width handles and byte orientation were corrected without
relaxing guards. The pointer library registers no raw MCP command and never
claims game-state verification. Compare the new observations in a fresh immutable
recorded game, integrate ordinary ID-based actions with canonical readback, then
resume the exclusive Luna medium player. Campaign acceptance is still pending.


Correction to the preceding checkpoint: the byte-orientation static claim was
wrong. Exact dimension/hit/mask methods read a full four-byte field. The source
now restores full-word diagnostics and witnesses high-byte changes. Run 017
launched with the earlier immutable binary, so its derived dimensions cannot
support actions; do not hot-swap it or count prior fixtures as live proof of the
correction. Keep the explicit correction in the live-evidence history.


Corrected full-word source passed 229 portable/MSRV tests and 286 actual standard
Linux checks (285 substantive), plus 953 Python tests with 37 skips and the
compiler/protocol checks. It is a separate tested binary; run 017 retains source
9ffac46. Do not use that earlier run's derived dimensions for actions.


Recorded run 017 verified own economy observations, complete owned-root closure
and unchanged full technology keys. Its actual Research renderer was CTextSprite,
so the old geometry branch refused. One observed-screen Research attempt verified
canonical slot assignment; no ID-based adapter or Luna acceptance is claimed.
Both videos finalized and one WM close returned actual normal exit code 0.
The next own-division and separate Text/Frame raw readers require a fresh immutable
installed-game comparison. Text leaf support keeps the ancestor whitelist and
all input/routing authority boundaries intact.


The combined own-army and Text/Frame source passed 252 portable/MSRV tests and
309 actual standard Linux checks (308 substantive), plus 953 Python tests with
37 skips and all compiler/protocol checks. Continue with one fresh immutable
recorded run to compare own divisions and the actual Text Research control,
then verified ordinary ID-based actions and exclusive Luna medium acceptance.


Recorded run 018 passed own-division snapshots (30 unique IDs, all selected
location joins; 54 ms RPC median) and the actual Text Research diagnostics.
An ID-derived builder sequence computed Sprite and Text interior points from
Rust observations, selected the requested Info and verified canonical slot
assignment after one Research event. No screenshot targeting or replay occurred
in that sequence. The diagnostic OS wrapper was private Python; production Rust
strategic dispatch and Luna acceptance remain unfinished. Both videos finalized,
normal game exit was 0, and old client authority was closed.

The pure Rust point building block passed 258 portable/MSRV tests, 315 actual
Linux checks (314 substantive), 953 Python tests with 37 skips and compiler/
protocol checks. It registers no input command. Finish the borrowed lease/gate,
durable one-stage tokens and bounded readback service, then integrate the owner
and give the exclusive Luna medium player verified strategic tools.


The optional owned research-stage adapter now passes 313 portable/MSRV tests,
374 actual Linux checks (373 substantive) and 953 Python tests with 37 skips.
It shares the clock controller and lease, pins witnessed own-slot identities,
persists one intent per stage and verifies canonical state plus clear input.
Read-only focus-node and slot-control diagnostics are also integrated. Run one
fresh immutable recorded Germany game, prepare only the initial own empty slot
and give Luna medium exclusive input for ID-derived selection/assignment and
clock advancement. This remains a bounded strategic gate; daily plans, broader
commands and campaign acceptance are unfinished. Native posting stays quarantined.


Recorded run 019 passed the owned research preview with one exclusive Luna medium
player: exact-ID View/Research verified by canonical slot state, then the counter
advanced through 3 January using ordinary clock control and a server wait. Both
research RPCs were under one second in this two-action sample. Recording passed
ffprobe and one WM close returned actual normal exit 0. Three Rust and six Python
CI jobs passed after a Windows test-only capacity-fixture correction. Next remove
the builder's initial slot-navigation prerequisite through verified own-slot
actions, add focus/economic/army plans and resume the clean two-year gate.
This bounded run is not campaign acceptance; native posting stays quarantined.


The next Rust preview adds ID-based own-slot selection with shared typed Sprite
geometry and a same-reader ownership context. The durable stage refuses occupied
slots and quarantines uncertain selection across restart. Fixture gates pass
320 portable/MSRV and 381 reported actual Linux checks (380 substantive), plus
953 Python tests with 37 skips. Prepare only the initial research panel, then
give Luna medium exclusive input for SelectSlot/View/Research and a subsequent
empty-slot selection. Record the real result; panel/category navigation, focus
actions and the daily governor remain to be implemented.


Recorded run 020 passed that slot-selection gate with exclusive Luna medium
input: SelectSlot 0, View/Research and SelectSlot 1 verified, followed by counter
advancement through 3 January. All four strategic RPC samples were under one
second; the recording passed ffprobe and one WM close returned normal exit 0.
The builder opened only the initial panel. Implement panel/category navigation
and focus/economic/daily plans next; keep the full acceptance objective intact.


The next fixed `open_panel` stage and optional category Sprite diagnostic pass
portable/MSRV and actual Linux fixtures. Launch a fresh immutable recorded run,
prepare menus only, and let exclusive Luna medium input open the panel itself
before SelectSlot/View/Research. Observe category association/geometry by ID
without switching tabs. Keep unknown input quarantined and never hot-swap a
live binary. Category input, focus actions and daily plans remain unfinished.


Run 021 was canceled before campaign start/input binding after review disproved
the OpenPanel fixture's mode-zero assumption. It exited normally with code 0.
The next source requires independently proved no-managed-view sentinel 23 and
adds read-only category candidates plus opt-in focus geometry. Combined fixtures
pass; start a fresh immutable run for Luna OpenPanel/SelectSlot/View/Research.
Never reuse or replace run 021's binary to hide the canceled preflight.


Recorded run 022 verified exclusive Luna medium OpenPanel/no-input repeat,
SelectSlot/View/Research and a second empty-slot selection, then Jan 3 clock
advancement. Recording passed; one WM close exited normally with code 0 and exact owned
processes are gone. Actual category tabs were Standard, with geometry unavailable
in that recorded source. The next Standard reader and durable SelectCategory
stage pass combined fixtures. Next: a fresh immutable recorded Luna category
selection gate, then focus actions and daily plans; full campaigns remain pending.

Run024 subsequently passed the complete industry/electronics/infantry category
cycle, a distinct same-folder no-input completion and all four research
assignments with one exclusive Luna medium player. The later speed4 request
returned action_uncertain because no target witness followed delivered input;
no gameplay retry or post-failure observation request followed. The lease was
released, recording passed ffprobe and one owned WM close returned normal exit0.
Both exact owned processes are gone. Preserve that unresolved clock journal;
the final clock state is unknown. This is bounded research acceptance only.

New focus-stage, own-politics and daily-plan libraries remain offline
prerequisites. No focus action or governor worker is enabled. Continue by fixing
and freshly validating clock scheduling, joining the focus producers to the
shared controller, then wiring daily execution and remaining domain adapters.
Windows/macOS native live support, clean two-year acceptance and the Germany,
China and Poland campaigns remain required. Keep the full goal active.
