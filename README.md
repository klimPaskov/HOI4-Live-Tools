# HOI4 Live Tools

MCP tools for observing and playing Hearts of Iron IV, and for testing HOI4 mods.
The project is migrating to a **Linux-first Rust runtime based on Clausewitz-MCP**,
with Windows and macOS as required targets.

## Current status

The existing **1.1.0 Python release** has been exercised on Windows. The Rust
runtime is a development preview with live evidence on Linux; there is no Rust
release and no accepted full campaign yet. Windows and macOS native live adapters
still need independent validation.

One Luna player at medium reasoning verified upstream focus and research. Its
final native posting request timed out, so that adapter remains quarantined.
The Rust preview uses verified ordinary input with fresh process readback.
Recorded Linux tests have passed:

- Clock controls, exact-ID research navigation and four-slot worker assignment.
- Exact focus selection and Start, followed by a pending next-focus plan.
- Idempotent plan updates and worker restarts without replay or budget resets.
- Immediate restart/STOP with the durable budget intact, plus read-only Focus UI
  observations requiring no input lease.
- Rust Politics-close and research assignment, preserving the active focus; an
  identical close request returned its saved receipt without another dispatch.
- Worker-only Focus-to-research navigation and assignment, followed by immediate
  restart/STOP with both completions and the shared 2/2 budget intact.
- Worker-only research-to-Focus-to-research assignment, explicit target waits,
  and a separately verified restart/STOP with all three completions and budget 3/3.
- Research completion over advancing game time, automatic same-slot refill and
  restart persistence under a one-assignment-per-day budget.
- Actual Rhineland Focus completion, verified in engine membership, followed by
  an immediate same-plan restart/STOP without duplicate input.
- Digests below 2 KB with exact historical detail lookup; five recent samples
  returned in 50–62 ms under x86 CPU emulation.

These are bounded tests in vanilla Normal/historical Germany. Run050 advanced
from January into June 1936: support weapons entered completed membership and
the worker filled its slot with basic machine tools. The target was observed
after 27m 37s under x86 CPU emulation. Restart preserved receipts and budget;
recording and normal exit passed. Known zero-input refusals and one additional
budget-consuming refused attempt remain recorded. Economic/army/naval commands
and full campaigns remain open.
Original failures remain in the validation report; later success does not explain them.
Run051 read the empty completed-Focus baseline, then refused Start before input
because a tree-node witness changed. Run052 then verified actual Rhineland
completion on 4 February 1936 using narrower Start observations. Its completed-
Focus reads took 25.889/26.666 ms RPC; restart, recording and normal exit passed.
Run053 stopped the successive-Focus gate on read-only snapshot changes; the
second entry never attempted input. Run054 verified bounded fresh-read recovery,
then stopped at Four Year Plan's View geometry before any selection click. Its
exact hierarchy size limit was lost to diagnostic truncation; that reporting
bug is fixed, while the geometry failure and successive-Focus gate remain open.

| Runtime | Verified | Remaining |
| --- | --- | --- |
| Python / Windows | Baseline tools and recorded play attempts | Clean campaign acceptance; current development changes are unfinished |
| Python / macOS | Platform work and limited diagnostic play | Reliable live acceptance; screenshot-driven testing is stopped |
| Native / Linux | Own reads/catalogs, owned launch, bounded Luna clock/research/focus tests, research completion/refill, actual Focus completion, both panel transitions and restart persistence | Successive Focus automation, economics/army/naval control and campaign acceptance |
| Native / Windows and macOS | Portable protocol and fixture tests in CI | Independently verified process/build adapters and live tests |

Read [the acceptance gates](docs/LINUX_RUST_HANDOFF.md),
[Linux validation evidence](docs/CLAUSEWITZ_VALIDATION.md) and
[the measured comparison](docs/CLAUSEWITZ_COMPARISON.md). Offline tests and a listed
capability do not establish successful live play. The preserved
[`wip/governor-memory`](https://github.com/klimPaskov/HOI4-Live-Tools/tree/wip/governor-memory)
branch is unfinished development work.

## Install the existing Windows release

Requirements: Windows 10 or 11, an owned Steam copy of HOI4, Python 3.9+, and the
Windows English OCR component. Run the game windowed or borderless with UI scale
1. FFmpeg is optional for capture and recording.

1. Download and unpack an archive from [Releases](https://github.com/klimPaskov/HOI4-Live-Tools/releases).
2. Run `python install.py`. The installer creates a private Python environment
   and registers the stdio MCP server with supported coding clients.
3. Restart the client, then run `hoi4-live-tools.cmd selftest` to check setup.

The [Windows runtime reference](docs/LEGACY_WINDOWS_USAGE.md) covers manual MCP
registration, configuration, CLI calls, upgrades and uninstalling. The
[tool reference](docs/tools.md) lists argument schemas. These instructions apply
to the existing Python release; they do not install the future Rust engine.

## Campaign play and mod testing

**FAIR is the campaign default.** It permits strong ordinary strategy, including
encirclements, concentrated attacks and supported invasions, under the documented
house rules. **META** permits authorized single-player metas and UI exploits.
Both use Normal difficulty, vanilla rules and historical AI, with no privileged
cheats, hidden enemy information or reload-to-undo. See the
[campaign playbook](skills/hoi4-campaign-playbook/SKILL.md).

Mod QA is a separate workflow. Its console, scripted-effect and scenario setup
features must remain inaccessible to campaign automation. Recordings, journals,
saves, credentials and game files remain outside the public repository.

The native migration aims to provide structured process observations and
verified ordinary player commands, durable daily plans, compact state changes,
event waits, economic recovery, army control and naval invasions. It will fail
explicitly on unsupported builds and unavailable capabilities. See the
[foundation and expansion plan](docs/CLAUSEWITZ_FOUNDATION.md).

## Rust development preview

The [Rust preview](native/README.md) provides exact-build owned-process snapshots,
bounded server waits and optional ordinary clock controls with exclusive leases,
durable receipts and canonical state readback. Its persistent
[Linux launcher](docs/OWNED_LINUX_LAUNCHER.md) supports reconnecting clients and
guarded graceful shutdown. It replaces save/screenshot dependence in this
observation path.

Recorded Linux checks include Luna medium clock play through 5 January 1936,
own research-counter advancement, focus/research catalogs and narrower exact-ID
queries. Fresh overviews measured 15.7 ms median RPC under x86 CPU emulation;
exact-key diagnostics measured 116/120 ms. Scope, hardware and validation limits
are documented in [the comparison](docs/CLAUSEWITZ_COMPARISON.md) and
[live evidence](docs/CLAUSEWITZ_VALIDATION.md), including failures and clean exits.

Research uses exact technology IDs, verified panel/slot/category navigation and
a persisted shared daily budget. Focus has separate owner and plan permissions;
its worker has recorded Start, plan-update and restart evidence. The two worker
panel transitions now have bounded live evidence: run047 passed Focus-to-research,
and run049 passed research-to-Focus-to-research with three assignments. Explicit
assignment waits accept a matching first sample. Durable completion means the
assignment was verified. Separate runs have verified actual research completion
and queued refill, plus actual Focus completion; successive Focus automation and
full campaign acceptance remain open. Earlier failures, including unresolved clock uncertainty,
are preserved in the validation report. Three-OS CI checks shared code and fixtures;
Windows/macOS live adapters still need independent validation.

## Development

Python compatibility remains 3.9+. Install the legacy package from a checkout
with `python -m pip install -e .` and run:

```sh
python -m unittest discover -s tests -v
```

Follow [AGENTS.md](AGENTS.md), the [Linux continuation brief](docs/LINUX_RUST_HANDOFF.md)
and [publication requirements](docs/RELEASING.md). Build layouts are specific to
an executable and platform; source inclusion does not authorize native commands.
Release acceptance and downstream integration pins remain separate from a
passing development test suite.

## License

Project code is licensed under [Apache-2.0](LICENSE). Imported and derived
Clausewitz-MCP components retain their MIT notices, license and provenance in
[NOTICE](NOTICE) and [the upstream manifest](third_party/clausewitz_mcp/UPSTREAM.json).
HOI4 is a trademark of Paradox Interactive AB; this independent project is not
affiliated with or endorsed by Paradox.

## Acknowledgements

This project builds on [smerss/Clausewitz-MCP](https://github.com/smerss/Clausewitz-MCP),
Copyright (c) 2026 smerss, licensed under MIT. Its structured game model,
process interface, player-command research and event-driven automation form the
foundation of the native migration. We retain the [original MIT license](third_party/clausewitz_mcp/LICENSE)
and [file-level provenance](third_party/clausewitz_mcp/UPSTREAM.json).
