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
Subsequent Rust runs verified ordinary clock controls, exact-ID research actions
and memory readback. Recorded run030 passed worker-only four-slot research refill,
idempotent plan updates, restart without resetting its daily budget, and counter
advancement through 3 January 1936.

Recorded run032 verified opening Politics and the focus tree, then selecting the
exact Rhineland focus after hierarchy and cache checks passed. Starting it refused
at an unverified callback association. Run033 repeated selection and confirmed
that association remained unavailable before any Start input. The failures are
preserved without retry. Focus start and broader strategic automation remain unfinished.

| Runtime | Verified | Remaining |
| --- | --- | --- |
| Python / Windows | Baseline tools and recorded play attempts | Clean campaign acceptance; current development changes are unfinished |
| Python / macOS | Platform work and limited diagnostic play | Reliable live acceptance; screenshot-driven testing is stopped |
| Native / Linux | Own reads/catalogs, owned launch, bounded Luna clock/research tests, four-slot worker refill, restart persistence and exact focus selection | Focus start, broader daily plans, economics/army/naval control and campaign acceptance |
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

The research preview opens the own panel, selects empty slots and navigates
folders with verified state readback. Recorded run 025 passed ordinary clock
control, four research assignments and advancement to 5 January 1936. The earlier
run 024 clock uncertainty remains unexplained. Opt-in focus stages and a daily
worker now pass fixtures. Recorded run026 verified focus opening, then refused
selection because owned root closure was missing; that cause remains unresolved.
The current worker adds durable panel/slot/category navigation, plan updates and
several research assignments within a persisted daily budget. Recorded run030
passed worker-only refill of all four slots on one paused day, idempotent plan
updates and restart without replay or budget reset. Bounded pre-input retries
preserved the original torn-read history. Earlier failed runs remain documented;
returning from a failed open tree to another slot is still unavailable. Broader
strategic adapters and Windows/macOS live adapters remain
unfinished. Three-OS CI tests
shared code and fixtures; compilation is not live compatibility or campaign acceptance.

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
