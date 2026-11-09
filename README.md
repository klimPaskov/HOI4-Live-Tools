<h1 align="center">HOI4 Live Tools</h1>

<p align="center">
  An <a href="https://modelcontextprotocol.io/">MCP</a> server that lets your coding agent play and test <em>Hearts of Iron IV</em>.
</p>

<p align="center">
  <a href="https://github.com/klimPaskov/HOI4-Live-Tools/releases/latest"><img alt="Download" src="https://img.shields.io/github/v/release/klimPaskov/HOI4-Live-Tools?label=Download&logo=github&color=2ea44f"></a>
  <a href="https://discord.gg/rAXesGcT2t"><img alt="Discord" src="https://img.shields.io/badge/Discord-Join%20Community-7289da?logo=discord&logoColor=white"></a>
  <a href="https://www.youtube.com/@ChaosRedux"><img alt="YouTube" src="https://img.shields.io/badge/YouTube-Chaos%20Redux-ff0000?logo=youtube&logoColor=white"></a>
  <a href="https://ko-fi.com/klimpaskov"><img alt="Ko-fi" src="https://img.shields.io/badge/Ko--fi-Support%20the%20Project-ff5e5b?logo=kofi&logoColor=white"></a>
  <a href="https://github.com/klimPaskov/HOI4-Live-Tools/issues"><img alt="Issues" src="https://img.shields.io/badge/Bugs%20%26%20Ideas-Issues-24292f?logo=github&logoColor=white"></a>
</p>

<p align="center">
  <a href="#which-version-should-i-use">Which version</a> ·
  <a href="#what-works-where">What works where</a> ·
  <a href="#quick-start-windows-release">Quick start</a> ·
  <a href="#launch-order-for-a-mod-test">Launch order</a> ·
  <a href="#native-rust-preview">Native preview</a> ·
  <a href="docs/tools.md">Tool reference</a> ·
  <a href="CHANGELOG.md">Changelog</a>
</p>

Your agent can test mods without you sitting at the keyboard: it launches the game,
loads a save, fires events, answers popups, runs time forward and reads the error
log. It can also play campaigns through the real game interface, from focuses,
research and production to construction, politics, diplomacy and armies.

Part of the HOI4 modding toolkit: [HOI4 Mod Setup](https://github.com/klimPaskov/HOI4-Mod-Setup)
installs it for you, [Agentic HOI4 Modding](https://github.com/klimPaskov/Agentic-HOI4-Modding)
gives your agent modding skills, and [HOI4 Agent Tools](https://github.com/klimPaskov/hoi4-agent-tools)
builds and previews mod content.

## Which version should I use?

| You want to… | Use | Platform | Maturity |
| --- | --- | --- | --- |
| Test mods and try agent play today | **`hoi4-live-tools-1.4.0.zip`** from the [1.4.0 release](https://github.com/klimPaskov/HOI4-Live-Tools/releases/latest) (Python) | Windows 10/11 | Stable release |
| Agent play with the full management toolset | **`hoi4-live-native-1.4.0-macos-arm64.tar.gz`** ([native downloads](#native-downloads)) | macOS on Apple silicon, Steam HOI4 1.19.4.1 | Experimental |
| Agent play on Linux | **`hoi4-live-native-1.4.0-linux-x86_64.tar.gz`** ([native downloads](#native-downloads)) | Linux x86_64, HOI4 1.19.4.1 | Experimental |

The Windows native game adapter is unfinished. An internal read-only foundation
is available for development; see [the Windows port](docs/WINDOWS_HANDOFF.md).
No full campaign has been accepted on any runtime yet. Earlier Mac campaign
defeats remain preserved alongside the later qualification tests. Status details
are below.

## What works where

Mod testing (launch the game, fire events, run effects, read `error.log`) is a
feature of the **Python package on Windows**. The native Linux and macOS builds
read the game's memory to play a campaign. They have no console, no events and no
`error.log` tools, and they start the game without your mods. Use this table to
pick:

| What you want to do | Windows, Python package | Linux | macOS |
| --- | --- | --- | --- |
| Install | `python install.py`, registers your coding client | Native `.tar.gz`; the Python installer refuses | Native `.tar.gz` (Apple silicon); the Python installer refuses |
| Launch with your mods, `-debug`, new game, load, save, quit | **Works** (tested live on 1.19.4.1) | No. The Python package refuses; the native launcher starts the game with an empty playset | Not accepted. The Python code exists but has no live run; the native build has no launcher |
| Console, `hoi4_eval_effect`, `hoi4_fire_event`, answer popups | **Works** in test mode | No | Not tested |
| Smoke tests, trigger and focus checks, localisation sweep (they use the console) | In test mode. Offline-tested on recorded fixtures; the first live run is pending | No | Not tested |
| Read `error.log` and `game.log`, query plain-text saves | **Works** | Plain file reads that should work; not tested live | Plain file reads that should work; not tested live |
| Screenshots, OCR, click and type in the game window | **Works** (needs the English Windows OCR component) | No in Python | Not tested |
| Play a campaign through the interface | Python tools, recorded attempts only | Native, about 104 tools tested live, experimental | Native, about 120 tools tested live, experimental |
| Game builds accepted | No build gate; tested on 1.19.4.1 | HOI4 1.19.4.1 x86_64 only | HOI4 1.19.4.1 x86_64 under Rosetta only |

Things every modder should know:

- **Test mode.** The server starts in campaign mode, which refuses the console,
  events and effects. Set `HOI4_LIVE_MODE=test` (or `"mode": "test"` in
  `config.json`) before you start it. A campaign session cannot go back to test mode.
- **DLC and Steam.** A scratch user folder needs `hoi4_prepare` for the DLC to load: follow the
  [launch order](#launch-order-for-a-mod-test). Career Profile and Awards need the Steam client.
- **Windows setup.** Windowed or borderless game, UI scale 1, English Windows OCR,
  console key on the `` ` `` key of a US layout. A 1920×1080 borderless window is
  what the live runs used.
- **Mods and `-debug`.** The Python package launches the game with whatever your
  playset enables. The native Linux launcher starts it with no mods.
- **Not working yet.** The Windows native adapter, ARM64 Linux, and a clean
  two-year campaign on any system. The status section below keeps the details.

### Mod checks (Windows, test mode)

With the game running in test mode, an agent can run these. They are new: tested
on fakes and recorded log lines, with the first run in the live game still pending.

| Tool | What it answers |
| --- | --- |
| `hoi4_smoke_namespace` | Fires every event of a namespace through the console, answers each option (one firing per option), takes the decisions of a category or prefix, and reads `error.log` after each step. One report. |
| `hoi4_check_trigger` | Is this trigger true for this country? True or false, and the condition that is not met. |
| `hoi4_check_focus` | Can this country take this focus? Names the unmet prerequisite, exclusive focus, `available` or `allow_branch` condition. |
| `hoi4_sweep_loc` | Visits the map and each panel, reads the screen with OCR and lists `[missing_key]`, raw keys, unresolved `$VARIABLES$` and colour codes shown as text, plus localisation warnings in the logs. |

## Quick start: Windows release

You need Windows 10 or 11, HOI4 from Steam (windowed or borderless, UI scale 1),
Python 3.9 or newer, and the Windows English OCR component. FFmpeg is optional.

1. Download `hoi4-live-tools-1.4.0.zip` from the
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

## Launch order for a mod test

A game started without the Steam client finds no DLC in a user folder that
lacks two files the Paradox launcher keeps there, `launcher-v2.sqlite` and
`dlc_signature`. Your normal user folder has them. A scratch folder (set with
`HOI4_LIVE_USER_DIR`, or a `gameDataPath` redirect) does not, and the game then
loads 0 DLC. Mod testing needs the server in test mode (`HOI4_LIVE_MODE=test`, or
`"mode": "test"` in `config.json`): the default is campaign mode, which refuses the
console, events and `hoi4_prepare`. The order is:

1. **`hoi4_prepare`** copies the two DLC files from your normal user folder into the
   run's folder. Every file it replaces is backed up byte for byte first. When the
   run's folder is your normal one it reports that there is nothing to copy.
   `hoi4_prepare {"action": "status"}` shows what each folder holds.
2. **`hoi4_launch`** starts the game (`debug: true` for the console tools).
   `hoi4_launch {"prepare_dlc": true}` does steps 1 and 2 in one call.
3. **Test.** Fire events, run effects, read `error.log`.
4. **`hoi4_shutdown`** quits through the game's own menu and puts the run's folder
   back as it was. If the server was restarted in between, run
   `hoi4_prepare {"action": "restore"}` once the game has exited. A file that
   changed after the copy is never overwritten; it is reported instead, and
   `force: true` restores the original and keeps the changed file next to the backup.

Career Profile and Awards need the Steam client, so achievement checks cannot run
this way. Everything in the journal stays under `artifacts/dlc_backups/`.

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

Follow [native setup](native/README.md) and the platform notes below. The native
runtime is experimental, not a replacement for the Windows package.

### Native downloads

Each release carries prebuilt `hoi4-live-native` archives, `SHA256SUMS.txt` and
GitHub build attestations, so nothing needs to be built from source:

```sh
sha256sum -c SHA256SUMS.txt --ignore-missing      # macOS: shasum -a 256 -c
gh attestation verify hoi4-live-native-1.4.0-linux-x86_64.tar.gz --repo klimPaskov/HOI4-Live-Tools
tar -xzf hoi4-live-native-1.4.0-linux-x86_64.tar.gz
```

The macOS binary is ad-hoc signed with `debugger.entitlements` (included). Memory
access still needs Developer Tools access for the running user; see
[native setup](native/README.md).

No Steam variable or overlay reaches the game, and the native Linux launcher gives
it an empty private `HOME`.

The macOS and Linux builds have the same management toolset (about 120 tools: armies,
fronts and encirclement planning, air, navy and naval invasions, designers, doctrine,
intelligence, peace conferences, decisions, events and the plan runner; see
[the tool index](docs/TOOLS_MACOS.md)) on HOI4 1.19.4.1.

### Linux

The tested native Linux game is HOI4 1.19.4.1 x86_64 (GNU build ID `2265e6e7...`); the
build gate refuses any other build, including 1.19.3. The
[owned launcher](docs/OWNED_LINUX_LAUNCHER.md) verifies process ownership and never
takes over an unrelated game. `--bind-management` binds the full management toolset to
the owned X11 window. The tools read the game through a per-build address table
(`native/layouts/`), generated from the macOS build and checked live
([Linux port](docs/LINUX_PORT.md)).

[Live verification on Linux](docs/LIVE_VERIFICATION_LINUX_1_19_4.md) records 104 tools run
against the game (economy, research, focus, politics, trade, armies and fronts in a war,
air, navy, special projects, the intelligence agency, events, diplomacy and the plan
runner), the refusals reached for the rest, and the fixes the Linux build needed. Peace
conferences and the operative tools were not reached on Linux. The older Linux-only GUI
diagnostics written for 1.19.3 refuse on 1.19.4.1.

## Quick start: macOS (Python server)

You need a Mac with HOI4 from Steam and Python 3.9 or newer (Apple's command-line tools
Python or python.org). FFmpeg is optional.

1. Download `hoi4-live-tools-<version>.zip` from the
   [latest release](https://github.com/klimPaskov/HOI4-Live-Tools/releases/latest) and unpack it.
2. In that folder, run `python3 install.py`. It installs a private Python environment, with numpy, Pillow,
   OpenCV and PyObjC, under `~/Library/Application Support/hoi4-live-tools`, and registers the server with
   the coding clients it finds.
3. Restart your coding client and check the game side:

   ```
   ~/Library/Application\ Support/hoi4-live-tools/app/hoi4-live-tools selftest
   ```

The app that starts the server (your terminal or coding client) needs Screen Recording and Accessibility
permission in System Settings; a tool that lacks one returns an error that names it. To uninstall, run
`python3 install.py uninstall`.

ARM64 Linux through FEX has verified own-state reads and lower-level stationary
Research keys at 1080p. Its pinned Rust input adapter remains disabled by default:
the latest retained-goal Research action still timed out at five seconds. The
explicitly pinned unified GL profile reached the actual 1920×1080 game and
own-country reads passed, but Research readiness did not verify. The latest
private diagnostic captured changing animation values under the original bound;
they do not establish readiness or update cadence. The uncertain Intent remains
preserved without retry or later settlement, and ordinary closure passed.
No campaign or annual throughput result is accepted. See
[FEX setup](docs/OWNED_LINUX_LAUNCHER.md#arm64-linux-with-fex) and
[the bounded animation check](docs/CLAUSEWITZ_VALIDATION.md#fex-bounded-animation-diagnostic--24-october-2026).

### macOS

The native preview supports one Steam **HOI4 1.19.4.1 (checksum f584) x86_64 build under Rosetta**
(see [the 1.19.4 update notes](docs/HOI4_1_19_4_UPDATE.md); the 1.19.3 build is no longer accepted on macOS)
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
conferences and naval invasions have since been added and live-tested on 1.19.4.1
([live verification](docs/LIVE_VERIFICATION_1_19_4.md)); the clean two-year acceptance
run and full campaigns remain open. Broader legacy adapters remain separate from the durable worker. The [controller contract](docs/MAC_CONTROLLER.md) describes current
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
| Python / Windows (1.2.0) | Mod-testing tools and recorded play attempts | Clean campaign acceptance |
| Python / macOS | Platform work and limited diagnostic play | Reliable live acceptance |
| Rust / Linux | Owned 1080p runs on 1.19.4.1: 104 live-tested tools of the management toolset, including a declared war with fronts, offensives and plans | Peace conferences, operatives, the two-year acceptance run and campaigns ([live verification](docs/LIVE_VERIFICATION_LINUX_1_19_4.md)) |
| Rust / macOS | Owned 1080p runs on 1.19.4.1: about 120 live-tested tools (economy, armies and fronts, air, navy, invasions, designers, doctrine, intelligence, peace conferences, plan runner) and short training wars | Open defects in [live verification](docs/LIVE_VERIFICATION_1_19_4.md), the two-year acceptance run and campaigns |
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
