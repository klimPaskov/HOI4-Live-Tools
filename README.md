# HOI4 Live Tools

MCP tools for observing and playing Hearts of Iron IV, and for testing HOI4 mods.
The project is building a **Linux-first Rust runtime based on Clausewitz-MCP**,
with Windows and macOS as required targets. It uses fresh process observations,
verified ordinary player controls, durable plans and compact state changes.

## Status

The **1.1.0 Python release** has been exercised on Windows. The Rust runtime is a
development preview with live Linux evidence. **No clean two-year run or full
campaign has passed, and there is no Rust release yet.** Windows/macOS native live
adapters still need independent implementation and validation.

| Runtime | Verified evidence | Still required |
| --- | --- | --- |
| Python / Windows | Baseline tools and recorded play attempts | Clean campaign acceptance; development changes remain unfinished |
| Python / macOS | Platform work and limited diagnostic play | Reliable live acceptance; screenshot-driven testing is stopped |
| Rust / Linux | Owned launch, own-state reads, ordinary clock/research/Focus controls, daily plans and restart persistence | Successive Focus automation, economy/army/naval controls and campaign acceptance |
| Rust / Windows and macOS | Portable protocol and fixture tests in CI | Build-specific native adapters and live tests |

One Luna player at medium reasoning verified upstream Focus and research on Linux.
A later native posting request timed out; that path remains quarantined. The Rust
preview uses ordinary input with fresh canonical state readback. Its recorded
vanilla Normal/historical Germany tests have verified:

- Exact-ID Focus and research assignments, including four research slots and
  worker transitions between the two panels.
- Actual research completion with automatic same-slot refill, plus actual
  Rhineland Focus completion confirmed by engine membership.
- Durable receipts, daily budgets, plan updates and restart/STOP without duplicate
  input.
- Compact digests below 2 KB, exact historical details and bounded server waits.
- Focus text search and two ordinary Tab selection steps. A later step refused
  before input; complete search-to-View navigation remains under validation.

Fresh overviews measured 15.7 ms median RPC under x86 CPU emulation; five recent
compact digests returned in 50–62 ms. These are scoped measurements, not a promise
for every tool or machine. See [the measured comparison](docs/CLAUSEWITZ_COMPARISON.md)
and [the validation record](docs/CLAUSEWITZ_VALIDATION.md), including failed runs
and unresolved limitations. Passing fixtures or listing a capability does not
establish successful live play.

## Use the existing Windows release

Requirements: Windows 10 or 11, an owned Steam copy of HOI4, Python 3.9+, and the
Windows English OCR component. Run the game windowed or borderless at UI scale 1.
FFmpeg is optional for capture and recording.

1. Unpack an archive from [Releases](https://github.com/klimPaskov/HOI4-Live-Tools/releases).
2. Run `python install.py` to create a private Python environment and register the
   stdio MCP server with supported coding clients.
3. Restart the client and run `hoi4-live-tools.cmd selftest`.

See [Windows setup and usage](docs/LEGACY_WINDOWS_USAGE.md) and
[the tool reference](docs/tools.md). This installer installs the Python release.
The preserved [`wip/governor-memory`](https://github.com/klimPaskov/HOI4-Live-Tools/tree/wip/governor-memory)
branch is unfinished development work.

## Build the Rust preview

Use Rust 1.86 or newer:

```sh
cargo build --manifest-path native/Cargo.toml --release --locked
cargo test --manifest-path native/Cargo.toml --locked
```

Follow [the native setup guide](native/README.md) and
[owned Linux launch instructions](docs/OWNED_LINUX_LAUNCHER.md) to create a private
session and connect through stdio MCP. Only the documented Linux x86_64 game
build has been tested. Unsupported platforms, builds and capabilities refuse
explicitly; the launcher does not take over an unrelated game.

The primary observation path does not require saves or repeated screenshots.
Reads and waits never pause the game. Optional ordinary controls require an
owned session, exclusive lease, persisted intent and verified state readback.
Uncertain input is retained across restart and never blindly replayed.

Production observations include optional base equipment type keys with explicit
unsupported coverage. A Linux sample returned keys for all 21 own lines in
67.8 ms RPC; comparison against the production UI, custom variant identity and
allocation controls remain pending.
Army, economy and naval automation are still in development. See
[the expansion plan](docs/CLAUSEWITZ_FOUNDATION.md) and
[current acceptance work](docs/LINUX_RUST_HANDOFF.md).

## Campaign play and mod testing

**FAIR is the default.** It permits strong ordinary strategy, including
encirclements, concentrated attacks and supported invasions, under the documented
house rules. **META** permits authorized single-player metas and UI exploits.
Both use Normal difficulty, vanilla rules and historical AI, with no privileged
cheats, hidden enemy information or reload-to-undo. The
[campaign playbook](skills/hoi4-campaign-playbook/SKILL.md) defines the rules.

Mod QA is separate: its console, scripted-effect and scenario setup features
must remain inaccessible to campaign automation. Game assets, saves, recordings,
credentials and private journals stay outside the public repository.

## Contributing

Python compatibility remains 3.9+. For legacy development, install with
`python -m pip install -e .` and run `python -m unittest discover -s tests -v`.
Native checks and platform limits are documented in [native/README.md](native/README.md).
Follow [AGENTS.md](AGENTS.md) and [publication requirements](docs/RELEASING.md).
Portable CI does not replace live acceptance; releases and downstream pins move
only after their separate acceptance gates pass.

## License

Project code is licensed under [Apache-2.0](LICENSE). Imported and derived
Clausewitz-MCP components retain their MIT notices, license and provenance in
[NOTICE](NOTICE) and [the upstream manifest](third_party/clausewitz_mcp/UPSTREAM.json).
HOI4 is a trademark of Paradox Interactive AB. This independent project is not
affiliated with or endorsed by Paradox.

## Acknowledgements

This project builds on [smerss/Clausewitz-MCP](https://github.com/smerss/Clausewitz-MCP),
Copyright (c) 2026 smerss, licensed under MIT. Its structured game model, process
interface, player-command research and event-driven automation form the foundation
of the native migration. We retain the [original MIT license](third_party/clausewitz_mcp/LICENSE)
and [file-level provenance](third_party/clausewitz_mcp/UPSTREAM.json).
