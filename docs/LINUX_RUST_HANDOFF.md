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
and a fresh Luna run still require live verification. Strategic actions and the
clean two-year campaign remain unfinished; the failed native posting path is
still quarantined.
