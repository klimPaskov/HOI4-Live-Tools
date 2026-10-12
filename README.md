# HOI4 Live Tools

MCP tools for observing and playing Hearts of Iron IV, and for testing HOI4 mods.
The project is migrating to a **Linux-first Rust runtime based on Clausewitz-MCP**,
with Windows and macOS as required targets.

## Current status

The existing **1.1.0 Python release** has been exercised on Windows. The native
runtime is in development. One Luna player at medium reasoning verified focus,
research and time advancement in a real licensed Linux game. Its final pause
command timed out, so campaign acceptance remains failed. The first Rust
observation runtime has started; there is no Rust release yet.

| Runtime | Verified | Remaining |
| --- | --- | --- |
| Python / Windows | Baseline tools and recorded play attempts | Clean campaign acceptance; current development changes are unfinished |
| Python / macOS | Platform work and limited diagnostic play | Reliable live acceptance; screenshot-driven testing is stopped |
| Native / Linux | Upstream ordinary actions and one-day Luna smoke; Rust own-country snapshots | Reliable running-game command scheduling, governor and campaign acceptance |
| Native / Windows and macOS | Architecture and reference research | Independently verified build adapters and live tests |

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

The first [Rust runtime](native/README.md) provides owned-process snapshots and
bounded waits over stdio MCP. It uses the current verified Linux ELF layout and
returns capability errors for unsupported builds. Native commands, the daily
governor, and Windows/macOS process adapters are still unimplemented in Rust.
An explicit owned binding can enable ordinary clock keys with exclusive input
leases, durable receipts and memory readback. Strategic action adapters remain
unfinished; the previous engine posting failure stays quarantined.
The Linux preview also has a persistent [owned launcher](docs/OWNED_LINUX_LAUNCHER.md)
with reconnectable clients and guarded graceful shutdown.
Linux, Windows and macOS CI pass the portable code and platform fixtures.
Those builds do not prove Windows/macOS live compatibility.

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
