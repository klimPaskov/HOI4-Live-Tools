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

The Windows native adapter is planned (see [the Windows handoff](docs/WINDOWS_HANDOFF.md)).
No full campaign has been accepted on any runtime yet; the macOS runs so far ended
in recorded defeats that are kept in the record. Status details are below.

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

## Native Rust preview (Linux)

The preview is a new runtime built on [Clausewitz-MCP](https://github.com/smerss/Clausewitz-MCP).
It reads the game's own state directly instead of using screenshots and OCR, so
reads take tens of milliseconds and never pause the game. Every input it sends
is an ordinary player action, checked against fresh game state before and after.

Build it with Rust 1.86 or newer:

```sh
cargo build --manifest-path native/Cargo.toml --release --locked
cargo test --manifest-path native/Cargo.toml --locked
```

Then follow [the native setup guide](native/README.md) and
[the owned Linux launch instructions](docs/OWNED_LINUX_LAUNCHER.md). Only the
documented Linux x86_64 game build (1.19.3) has been tested. Unknown builds,
platforms and capabilities refuse explicitly, and the launcher never takes over a
game it did not start.

### What works today (recorded live tests on Linux)

- **Observation:** country overview, research, Focus, production lines and
  construction, returned as compact digests (under 2 KB) in about 15–80 ms.
- **Time:** pause, resume, speed and day advancement with verified readback.
- **Production:** opening the panel and changing one factory on an exact own line,
  with verified counts, historical replays and no-op checks. Production004 passed
  10→11→10 on 21 October 2026; changes took about 0.77 seconds each. Production006
  passed the daily worker gate on 22 October: 14 factories across three days,
  daily budgets and completed-plan restart without repeating input. Production008
  verified manual panel closing and the Research/Focus round trip, with unchanged
  allocations and replay/no-op checks. Mixed-panel worker planning and full
  campaign acceptance remain pending.
- **Research and Focus:** exact-ID assignments in all four research slots,
  automatic refill when research completes, Focus search and start, and durable
  daily plans that survive restarts without repeating input.
- **Construction:** opening the construction panel and selecting a building type
  (dockyard), with replay-safe receipts. Run077 passed this gate on 17 October 2026.
- **Map search:** opening the game's own Find view, typing a state ID and jumping
  the camera to that state, verified from game memory (Run086, 17 October 2026).
- **Building placement:** with a building selected and the camera on an own
  state, one map click queues it there. The result is verified as the exact
  queued amount of that building in that state. Run091 placed an arms factory in
  Brandenburg on 18 October 2026, and Run092 placed a dockyard in West Prussia,
  a coastal state, the same day.

ResearchClose002 passed direct Research-panel closing with Luna medium: 14 steps,
19 calls without errors, preserved assignments and historical replay/no-op checks.
Closing took about one second; reads and replays took milliseconds. It uses
Research permission alone, without assigning a Focus to switch panels.
Mixed004 completed research-assignment and production goals across three days.
A player path error prevented its final checks; a faster follow-up exposed an
opening-readiness gap. The correction and completed-plan restart still need a
full recorded mixed-plan test.

Mixed production/research standing plans now have an explicit opt-in developer
preview with persistent selections and separate panel-navigation budgets. Their
recorded advancing-time live gate is still pending.

Future campaign tests and recordings will use native **1920×1080 or higher**
with readable UI. The new Linux 1080p input profile still needs live qualification;
earlier low-resolution runs are diagnostic tests, not filmable campaign evidence.

### macOS preview (live-tested on one build)

The macOS adapter attaches to one owned game process through its task port and
sends only ordinary mouse and keyboard events to the front-most owned window.
Every click target is read from the live GUI tree and every action is checked
from memory afterwards, with a journal that never replays an unresolved input.
Tested on the Steam 1.19.3 x86_64 build under Rosetta at 1920x1080:
time, research, Focus, production, construction (with free-slot reads), laws,
advisors and designers, decisions, division training with deployment locations,
armies (create, split, generals, army groups and field marshals), front and
offensive lines, move orders, plan execution, events, diplomacy (war goals,
declaring war), air wings and missions, and a standing-plan runner
(`hoi4_plan`, `hoi4_plan_run`). The peace conference and naval invasions are not
supported yet. It needs a Developer Tools grant and an ad-hoc signed binary
(`native/macos/debugger.entitlements`).

### Not done yet

- Linux production scrolling, new lines, bulk allocation and standing economy plans.
- Linux provincial buildings (naval bases, forts, coastal forts), army, air and naval control.
- Economy and warfare automation (standing plans, shortage repair, invasions).
- A native Windows adapter.
- The clean two-year run and the recorded FAIR Germany and China campaigns.

Measurements, failed runs and open limits are recorded in
[the measured comparison](docs/CLAUSEWITZ_COMPARISON.md),
[the validation record](docs/CLAUSEWITZ_VALIDATION.md) and
[the current acceptance plan](docs/LINUX_RUST_HANDOFF.md).

## Campaign rules

Campaigns run on Normal difficulty with vanilla rules and historical AI. There are
no cheats, console commands, save editing, hidden enemy information or
reload-to-undo.

- **FAIR** (default) follows common multiplayer house rules but still plays well:
  encirclements, concentrated attacks and supported invasions.
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
| Rust / macOS | Owned runs on one build: reads, economy, armies, diplomacy, air, standing plans | Peace conference, naval invasions, campaign acceptance |
| Rust / Windows | Portable protocol and fixture tests in CI | Native adapter and live tests |

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
