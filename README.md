# HOI4 Live Tools

[![Discord](https://img.shields.io/badge/Discord-Join%20Community-7289da?logo=discord&logoColor=white)](https://discord.gg/rAXesGcT2t)
[![YouTube](https://img.shields.io/badge/YouTube-Chaos%20Redux-ff0000?logo=youtube&logoColor=white)](https://www.youtube.com/@ChaosRedux)
[![Ko-fi](https://img.shields.io/badge/Ko--fi-Support%20the%20Project-ff5e5b?logo=kofi&logoColor=white)](https://ko-fi.com/klimpaskov)
[![Download](https://img.shields.io/github/v/release/klimPaskov/HOI4-Live-Tools?label=Download&logo=github&color=2ea44f)](https://github.com/klimPaskov/HOI4-Live-Tools/releases/latest)
[![Issues](https://img.shields.io/badge/Bugs%20%26%20Ideas-Issues-24292f?logo=github&logoColor=white)](https://github.com/klimPaskov/HOI4-Live-Tools/issues)

An MCP server that lets your coding agent play and test **Hearts of Iron IV**.

- **Test mods** without sitting at the keyboard: launch the game, load a save,
  fire events, answer popups, run time forward and read the error log.
- **Play campaigns** through the real game interface: focuses, research,
  production, construction, politics, diplomacy and armies.

Part of the HOI4 modding toolkit: [HOI4 Mod Setup](https://github.com/klimPaskov/HOI4-Mod-Setup)
installs it for you, [Agentic HOI4 Modding](https://github.com/klimPaskov/Agentic-HOI4-Modding)
gives your agent modding skills, and [HOI4 Agent Tools](https://github.com/klimPaskov/hoi4-agent-tools)
builds and previews mod content.

## Which version should I use?

| You want to… | Use | Platform | Maturity |
| --- | --- | --- | --- |
| Test mods and try agent play today | **[1.1.0 release](https://github.com/klimPaskov/HOI4-Live-Tools/releases/latest)** (Python) | Windows 10/11 | Stable release |
| Help build the fast native runtime | **Rust preview** (build from source, [`native/`](native/README.md)) | Linux x86_64, macOS (Steam 1.19.3 x86_64 under Rosetta) | Developer preview |

The Windows native game adapter is unfinished. An internal read-only foundation
is available for development; see [the Windows port](docs/WINDOWS_HANDOFF.md).
No full campaign has been accepted on any runtime yet. Earlier Mac campaign
defeats remain preserved alongside the later qualification tests. Status details
are below.

## Quick start: Windows release

You need Windows 10 or 11, HOI4 from Steam (windowed or borderless, UI scale 1),
Python 3.9 or newer, and the Windows English OCR component. FFmpeg is optional.

1. Download `hoi4-live-tools-1.1.0.zip` from the
   [latest release](https://github.com/klimPaskov/HOI4-Live-Tools/releases/latest)
   and unpack it anywhere.
2. In that folder, run:

   ```
   python install.py
   ```

   It installs a private Python environment under `%LOCALAPPDATA%\hoi4-live-tools`
   and registers the server with the coding clients it finds (Claude Code, Codex,
   Cursor, OpenCode).
3. Restart your coding client and check the game side:

   ```
   %LOCALAPPDATA%\hoi4-live-tools\app\hoi4-live-tools.cmd selftest
   ```

To uninstall, run `python install.py uninstall`. See
[Windows setup and usage](docs/LEGACY_WINDOWS_USAGE.md) for manual registration,
configuration and the command line, and [the tool reference](docs/tools.md) for
every tool and argument.

## Native Rust preview

The native runtime builds on [Clausewitz-MCP](https://github.com/smerss/Clausewitz-MCP).
It reads structured state directly from the owned game process. Reads do not
pause, take screenshots or move the cursor. Inputs use ordinary player controls
and fresh state checks; unknown builds and unsupported operations refuse explicitly.
The primary live path does not depend on save parsing or screenshot polling.

Build with Rust 1.86 or newer:

```sh
cargo build --manifest-path native/Cargo.toml --release --locked
cargo test --manifest-path native/Cargo.toml --locked
```

Follow [native setup](native/README.md) and the platform notes below. This branch
is a developer preview, not a replacement release for the Windows package.

### Linux

The tested native Linux game is HOI4 1.19.3 x86_64. The
[owned launcher](docs/OWNED_LINUX_LAUNCHER.md) verifies process ownership and never
takes over an unrelated game. Recorded tests cover:

- Structured country, Research, Focus, production and construction reads.
- Verified clock control, Research and Focus assignments, natural Research
  refill, and durable daily plans that preserve completed work across restart.
- One-factory changes on an existing own production line, panel transitions,
  per-day budgets and completed-plan restart without repeated input.
- Construction-panel selection, state search, and verified arms-factory and
  dockyard placement in owned states.

The combined production/Research worker remains an opt-in preview with an
unfinished advancing-time acceptance gate. Provincial buildings, new production
lines, bulk allocation and combat control remain open.

ARM64 Linux through FEX reached historical Normal Germany at 1080p and verified
own-state reads plus stationary Research-panel keys. Five paused-game samples per
reader measured local medians of 2.9–6.1 ms with recording active. The pinned FEX
adapter is disabled by default and still needs live adapter acceptance. See
[FEX setup](docs/OWNED_LINUX_LAUNCHER.md#arm64-linux-with-fex) and
[the qualification record](docs/CLAUSEWITZ_VALIDATION.md#fex-own-state-and-stationary-input-qualification--22-october-2026).

### macOS

The native preview supports one Steam **HOI4 1.19.3 x86_64 build under Rosetta**
with a verified **1920×1080, UI scale 1** input profile. It checks the owned game
and window before input. [Window recording](docs/MAC_RECORDING.md) captures
1920×1108, including the titlebar around the game client.

Recorded Luna-medium component tests passed Research refill, Focus handoffs,
one-factory production changes, durable plan revisions/reconnects, construction
opening and ordinary building-template selection. The selection call took
240 ms. A recorded one-F Find-focus test also passed, with visible Find reads
below 1 ms. The native Find action then typed an owned state ID and Return in
549 ms, with a zero-input historical replay. Stationary Find/camera reads measured
0.48 ms per RPC with zero input. These are measurements of specific tested paths. See [the validation record](docs/CLAUSEWITZ_VALIDATION.md) for scope and
preserved failures.

The read-only construction target tool now offers a Mac center-terrain candidate
with `include_map_point: true`. Stationary native 1080p testing measured 2.36 ms
median RPC, compared with 2.00 ms without the option. It leaves the game and
cursor unchanged and supplies no placement permission; see the
[resident observer evidence](docs/CLAUSEWITZ_VALIDATION.md#mac-resident-center-terrain-mcp--24-october-2026).

The default-off [`hoi4_construction_place` qualification](docs/MAC_CONTROLLER.md#construction-placement-qualification)
uses one ordinary click with the exact template already selected and an owned
state centered. It verifies an exact +1 queue change, retains durable once-only
receipts and returns compact route diagnostics. A recorded private Rust component
passed at native 1080p: its 17-call driver took 4.661 seconds, including guards;
placement took 2.326 seconds with zero cursor displacement or observation pauses.
The source port remains a qualification preview; construction worker integration
and advancing-time acceptance remain unfinished. The Find action requires an
already-open focused empty box and explicit qualification permission. Peace
conferences, naval invasions, the clean two-year acceptance run and full campaigns
remain open. Broader legacy adapters remain separate from the durable worker. The [controller contract](docs/MAC_CONTROLLER.md) describes current
permissions, receipts and recovery.

The default-off [`hoi4_construction_clear_selection` qualification](docs/MAC_CONTROLLER.md#construction-cancellation-qualification)
now verifies ordinary Escape cancellation with the complete own queue unchanged.
A native 1080p action took 686 ms; a separate historical-only replay took 76 ms,
sent no input and left the original journal unchanged. The failed raw-timestamp
comparison between those phases is retained. This does not enable construction
standing plans or establish campaign acceptance.

The default-off `hoi4_construction_find_open` qualification now opens the exact
named Find button with one ordinary click. A recorded native 1080p test verified
it in 603 ms, preserving the full queue; historical replay sent no input.
Pointer actions also reject windows covering the intended point, including
floating panels. Decimal character admission has separate checks; mixed
construction plans remain under development. See [current evidence](docs/CLAUSEWITZ_VALIDATION.md).

Mac memory access requires Developer Tools authorization and a signed tool with
`native/macos/debugger.entitlements`; the game is never re-signed. Missing access
returns a capability error for telemetry/UI fallback. Screenshots and saves are
not the primary native observation path. The public Research-folder diagnostic
remains Linux-only.

### Remaining acceptance work

- Qualify broader routine action batches and expand economy, shortage repair
  and warfare.
- Implement and live-test the native Windows adapter.
- Complete a clean two-year Luna run, then the recorded FAIR Germany and China
  campaigns. Final showcase videos must come from macOS at 1080p or higher.
- Validate long campaign recording and publish only after release acceptance.

All nine CI jobs passed on the preceding placement checkpoint (`0b963fb`) across
Linux, Windows and macOS. Offline tests do not prove native game behavior.
For measurements, retained failures and the next gate, use
[the comparison](docs/CLAUSEWITZ_COMPARISON.md),
[validation record](docs/CLAUSEWITZ_VALIDATION.md) and
[current handoff](docs/LINUX_RUST_HANDOFF.md).

## Campaign rules

Campaigns run on Normal difficulty with vanilla rules and historical AI. There are
no cheats, console commands, save editing, hidden enemy information or
reload-to-undo.

- **FAIR** (default) follows the reviewed multiplayer-style house policy and
  permits normal tactical play, including encirclements and concentrated attacks.
- **META** allows single-player metas and UI tricks a human could use.

The [campaign playbook](skills/hoi4-campaign-playbook/SKILL.md) defines both modes.
Mod testing features (console, scripted effects, scenario setup) are kept separate
and cannot be reached by campaign automation.

## Project status

| Runtime | Verified so far | Still required |
| --- | --- | --- |
| Python / Windows (1.1.0) | Mod-testing tools and recorded play attempts | Clean campaign acceptance |
| Python / macOS | Platform work and limited diagnostic play | Reliable live acceptance |
| Rust / Linux | Owned launch, fast state reads, time, research, Focus, daily plans, one-factory production changes, construction panel, map search and state-building placement | Provincial placement, economy, army and naval control; campaign acceptance |
| Rust / macOS | Owned 1080p runs: structured reads, Research/Focus/clock, factory changes, recording and durable-plan component checks | Mixed-plan acceptance, wider automation, peace conferences, naval invasions and campaigns |
| Rust / Windows | Portable protocol and dormant owned-reader fixtures | Native game adapter, input and live tests |

Passing tests or listing a tool does not mean live play works. Only recorded live
runs count, and failures are kept in the record. See the [changelog](CHANGELOG.md)
for every run.

## Contributing

Bug reports and ideas are welcome in [Issues](https://github.com/klimPaskov/HOI4-Live-Tools/issues).

- Python 3.9+: `python -m pip install -e .`, then
  `python -m unittest discover -s tests -v`.
- Rust: the checks above, plus the platform notes in [native/README.md](native/README.md).
- Follow [AGENTS.md](AGENTS.md) and the [release process](docs/RELEASING.md).
  Releases and downstream pins move only after live acceptance.

Game assets, saves, recordings, credentials and private journals never go in this
repository.

## License

Project code is licensed under [Apache-2.0](LICENSE). Imported and derived
Clausewitz-MCP components keep their MIT notices, license and provenance in
[NOTICE](NOTICE) and [the upstream manifest](third_party/clausewitz_mcp/UPSTREAM.json).
HOI4 is a trademark of Paradox Interactive AB. This independent project is not
affiliated with or endorsed by Paradox.

## Acknowledgements

This project builds on [smerss/Clausewitz-MCP](https://github.com/smerss/Clausewitz-MCP),
Copyright (c) 2026 smerss, licensed under MIT. Its structured game model, process
interface, player-command research and event-driven automation form the foundation
of the native runtime. We keep the [original MIT license](third_party/clausewitz_mcp/LICENSE)
and [file-level provenance](third_party/clausewitz_mcp/UPSTREAM.json).
