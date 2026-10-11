# HOI4 Live Tools

[![Discord](https://img.shields.io/badge/Discord-Join%20Community-7289da?logo=discord&logoColor=white)](https://discord.gg/rAXesGcT2t)
[![YouTube](https://img.shields.io/badge/YouTube-Chaos%20Redux-ff0000?logo=youtube&logoColor=white)](https://www.youtube.com/@ChaosRedux)
[![Ko-fi](https://img.shields.io/badge/Ko--fi-Support%20the%20Project-ff5e5b?logo=kofi&logoColor=white)](https://ko-fi.com/klimpaskov)
[![Download](https://img.shields.io/github/v/release/klimPaskov/HOI4-Live-Tools?label=Download&logo=github&color=2ea44f)](https://github.com/klimPaskov/HOI4-Live-Tools/releases/latest)
[![Issues](https://img.shields.io/badge/Bugs%20%26%20Ideas-Issues-24292f?logo=github&logoColor=white)](https://github.com/klimPaskov/HOI4-Live-Tools/issues)

An MCP server that lets your coding agent play Hearts of Iron IV.
It launches the game, loads a save, fires events, answers popups, runs time forward, reads the screen and error log, and captures what happened, so you can test a mod without sitting at the keyboard.
It can also play a campaign through the real game interface: focuses, research, production, construction, politics, diplomacy and armies.
The campaign playbook is in [skills/hoi4-live-campaign](skills/hoi4-live-campaign/SKILL.md).

Live Tools works on Windows and was tested on HOI4 1.19.2. macOS support is in development.
See [docs/STATUS.md](docs/STATUS.md) for what works and the known limits.

Part of the HOI4 modding toolkit: [HOI4 Mod Setup](https://github.com/klimPaskov/HOI4-Mod-Setup) installs it for you, [Agentic HOI4 Modding](https://github.com/klimPaskov/Agentic-HOI4-Modding) gives your agent modding skills, and [HOI4 Agent Tools](https://github.com/klimPaskov/hoi4-agent-tools) builds and previews mod content.

## Requirements

- Windows 10 or 11, Hearts of Iron IV (Steam), run windowed or borderless at any resolution with the UI scale at 1.
- Python 3.9 or newer. The installer creates a private environment with `numpy`, `Pillow` and
  `opencv-python-headless` (downloaded from PyPI once, about 100 MB); the MCP protocol is implemented in the
  standard library.
- The Windows OCR English language component (installed with an en-US Windows display language; check with
  `hoi4-live-tools.cmd selftest`).
- `ffmpeg` on PATH (optional): the fallback screen capture (`ddagrab`) when PrintWindow returns a blank frame.
- For save queries: plain-text saves (`save_as_binary=no` in `Documents/Paradox Interactive/Hearts of Iron IV/settings.txt`).
  The server never edits that file; it reports when saves are binary.

## Install

The quickest route is the release archive:

1. Download `hoi4-live-tools-<version>.zip` from the [releases page](https://github.com/klimPaskov/HOI4-Live-Tools/releases)
   and unpack it anywhere (the folder is only the installer's source).
2. Run, from that folder:

   ```
   python install.py
   ```

   It copies the program to `%LOCALAPPDATA%\hoi4-live-tools\app`, creates the Python environment in `venv\` next to
   it, starts the server once to confirm it lists its tools, and registers it for your user in every coding client
   it finds (Claude Code, Codex, Cursor, OpenCode). Options: `--register claude,codex` (or `all`, `none`),
   `--prefix <folder>`; `python install.py status` shows what is installed and registered.
   Running it again upgrades in place and keeps your `config.json`, `artifacts\` and registrations.
3. Restart the coding client and check the server is listed (Claude Code: `/mcp`). Check the game side without
   touching the game: `%LOCALAPPDATA%\hoi4-live-tools\app\hoi4-live-tools.cmd selftest`.

The [HOI4 Mod Setup](https://github.com/klimPaskov/HOI4-Mod-Setup) app installs the same release for you when you
select **HOI4 Live Tools** under the optional workflows, together with the playtest skill of the
[Agentic HOI4 Modding](https://github.com/klimPaskov/Agentic-HOI4-Modding) package.

To remove everything the installer made: `python install.py uninstall` (add `--purge` to delete `config.json` and
`artifacts\` too). Registrations it wrote are removed; a `hoi4-live` entry that belongs to another program is left
alone.

### From a checkout

Clone this repository and install the dependencies yourself (`pip install -r requirements.txt`, or run
`python install.py` from the checkout to get the same layout as a release). The launcher
`hoi4-live-tools.cmd` picks its Python in this order: `HOI4_LIVE_PYTHON`, `..\venv` (installed layout) or `.venv`
(checkout), then a `python` or `py -3` on PATH that can import the three packages. Settings come from
`config.json` next to it (paths are `auto`, see Configuration below).

### Registering by hand

Use the launcher path of the install (`%LOCALAPPDATA%\hoi4-live-tools\app\hoi4-live-tools.cmd`) or of your checkout.

#### Claude Code

```
claude mcp add --scope user hoi4-live -- "C:\Users\<you>\AppData\Local\hoi4-live-tools\app\hoi4-live-tools.cmd"
```

or in a project `.mcp.json`:

```json
{
  "mcpServers": {
    "hoi4-live": {
      "command": "C:\\Users\\<you>\\AppData\\Local\\hoi4-live-tools\\app\\hoi4-live-tools.cmd",
      "args": []
    }
  }
}
```

Claude Code's default tool timeout is long enough for loading a save and running weeks of game time.

#### Codex

In `~/.codex/config.toml`:

```toml
[mcp_servers.hoi4-live]
command = 'C:\Users\<you>\AppData\Local\hoi4-live-tools\app\hoi4-live-tools.cmd'
args = []
startup_timeout_sec = 30
tool_timeout_sec = 1800
```

Codex's default tool timeout (60 s) is too short for `hoi4_load` and `hoi4_time step`; keep `tool_timeout_sec`.

### Command line

The same tools run without an MCP client, which is handy for scripts and debugging:

```
hoi4-live-tools.cmd list
hoi4-live-tools.cmd call hoi4_status
hoi4-live-tools.cmd call hoi4_time "{\"action\": \"step\", \"days\": 7}"
python -m hoi4_live call hoi4_click @args.json        (JSON from a file avoids shell quoting)
```

## Tools

Full reference with every argument: [docs/tools.md](docs/tools.md) (generated from the code with
`python -m hoi4_live docs`). Coordinates are game client pixels: (0, 0) is the top-left of the game picture.

| family | tool | what it does |
|---|---|---|
| session | `hoi4_status` | show the current situation: process, window, playset, mode, lock holder, recent saves, screen class, date, popups |
| | `hoi4_launch` | start the game (normal or `-debug`) with the current playset, wait for the main menu |
| | `hoi4_new_game` | Single Player, bookmark, country by name, Play (test mode: `tag` to switch country) |
| | `hoi4_load` | load any save through the game's own load screen (front end or pause menu); verifies the date |
| | `hoi4_save` | save as `hoi4live_<name>` (console in test mode, Save dialog in campaign mode) |
| | `hoi4_shutdown` | quit through the game menu (`confirm` required) |
| | `hoi4_lock` | hold or release the shared desktop lock; switch test/campaign mode |
| screen and input | `hoi4_screenshot` | PNG or JPEG of the client or a region (full image kept under `artifacts/`) |
| | `hoi4_ocr` | Windows OCR lines with boxes; `white_text` preprocessing for the top bar and console |
| | `hoi4_click`, `hoi4_key`, `hoi4_type`, `hoi4_scroll`, `hoi4_drag` | input inside the game window; each re-reads the screen and reports what changed |
| | `hoi4_hover` | read a tooltip at (x, y) or of a named top-bar item: the box is found on screen, OCR'd, split into label: value pairs |
| test mode | `hoi4_console` | run console commands and read their output back |
| | `hoi4_eval_effect` | `eval_effect` with optional scope, plus the error.log delta |
| | `hoi4_fire_event` | `event <id> [TAG]`, then read (or answer) the popups it opened |
| | `hoi4_logs` | error/game log deltas since a mark, folded and filterable |
| | `hoi4_save_query` | plain-text save queries: info, keys, get, find, variables, flags, diff |
| | `hoi4_scenario` | run a list of tool calls and write a Markdown/JSON report |
| time | `hoi4_time` | status, pause, run, speed, step N days with popup handling |
| popups | `hoi4_popups` | list, read (OCR title/text/options), answer by index or text, answer all |
| understanding | `hoi4_state` | one model of a country from telemetry (current), the newest save (deep) and the top bar, with the source and age of every field |
| | `hoi4_briefing` | one-page situation report: what needs a decision now, government, wars and fronts, economy, military, focus and research, world, events, changes since the last briefing |
| | `hoi4_screen` | screen class, open panel (and its key), other windows, console, popups, date, pause, speed, top-bar values |
| | `hoi4_map` | map modes (named from their buttons), camera to a state / the capital / the front, zoom, region screenshots, map tooltip at a point, front overview |
| | `hoi4_telemetry` | the companion mod: install, enable (dlc_load.json, backed up), disable, restore, watch variables and flags, read records |
| campaign | `hoi4_focus` | the focus tree from the files with each focus's status (available, locked, excluded, completed) from the save; the game's detail window (days, requirements met/unmet, effects); start, progress, cancel |
| | `hoi4_research` | research slots; technologies that can be researched now (path and dependency rules, ahead-of-time years); start one in a slot through the tech tree |
| | `hoi4_production` | production lines and factory counts; add a line (equipment list, MIO choice), assign factories, change equipment, remove |
| | `hoi4_construction` | the construction queue; the building buttons named from their tooltips; queue a building in a named state through the map camera |
| | `hoi4_politics` | laws and advisor slots (tooltips), their choice lists with costs and red marks; change a law, hire an advisor |
| | `hoi4_decisions` | the decision panel as the game shows it (categories, cost, can it be taken, timers) matched to the files; details; take one |
| | `hoi4_diplomacy` | a country's diplomacy view (actions with cost and ok/blocked marks) and acting on it (improve relations, guarantee, justify, faction invite, volunteers, lend-lease ...); the trade panel |
| military | `hoi4_army` | armies, army groups, templates and unassigned divisions (save) and the army cards; the template list; the division designer (duplicate, rename, add or remove battalions, save); recruit and deploy; create an army from the divisions in a state, assign divisions, put a general in command, rename, group armies |
| | `hoi4_orders` | an army's battle plan: front line against the country holding a named state, offensive or spearhead line through a named state, execute, hold, delete; status from the toolbar, the execute button, telemetry, the map tooltip and the save (war score, captured provinces) |
| | `hoi4_navy_air` | fleets and air wings (save); a fleet's mission in a named sea region; an air wing's mission over a state |
| campaign loop | `hoi4_clock` | the background advance: status, join, stop, pause, resume (telemetry, no screen) |
| | `hoi4_standing` | standing orders the clock executes (research, focus, construction, production queues on stop conditions) |
| | `hoi4_peace` | the peace conference: status, take (all states or named), submit, exit |
| | `hoi4_advance` | run time in short chunks until a decision point (technology done, focus completed, free factories, war started or ended, states lost, losing, low stability ...), answer events by policy (news and one-option events automatically, real choices returned to the agent, rules by title or text), then report what changed, the attention items and the tool calls that answer them |
| | `hoi4_journal` | the campaign log per country: notes, plans, decisions and lessons the agent writes; every verified action, advance and answered event the server writes; read, search, the standing plan, a run report |

### Examples

A typical event test (an agent would send these as `tools/call` requests):

```json
{"name": "hoi4_lock", "arguments": {"action": "acquire", "purpose": "event 001 test"}}
{"name": "hoi4_load", "arguments": {"save": "qa_001_fresh_head"}}
{"name": "hoi4_logs", "arguments": {"set_mark": "evt001"}}
{"name": "hoi4_fire_event", "arguments": {"event": "my_mod.1", "tag": "USA"}}
{"name": "hoi4_popups", "arguments": {"action": "answer", "option_text": "intervention"}}
{"name": "hoi4_time", "arguments": {"action": "step", "days": 10, "popups": "first"}}
{"name": "hoi4_save", "arguments": {"name": "evt001_post"}}
{"name": "hoi4_save_query", "arguments": {"save": "qa_001_fresh_head", "other": "hoi4live_evt001_post", "query": "diff", "path": "countries/USA/variables"}}
{"name": "hoi4_logs", "arguments": {"log": "error", "since": "evt001"}}
{"name": "hoi4_lock", "arguments": {"action": "release"}}
```

The same run as one call with a report: `hoi4_scenario` with `{"name": "evt001", "steps": [{"tool": ..., "args": ...}, ...]}`.

Reading what is on screen:

```json
{"name": "hoi4_screen", "arguments": {}}
{"name": "hoi4_popups", "arguments": {"action": "read"}}
{"name": "hoi4_ocr", "arguments": {"pattern": "Load Game"}}
{"name": "hoi4_hover", "arguments": {"target": "factories"}}
```

Knowing what is going on (the first calls of an agent's campaign turn):

```json
{"name": "hoi4_briefing", "arguments": {}}
{"name": "hoi4_state", "arguments": {"sections": ["economy", "construction", "production"], "screen": true}}
{"name": "hoi4_map", "arguments": {"action": "front"}}
{"name": "hoi4_map", "arguments": {"action": "view", "mode": "supply", "target": "Texas", "zoom": "region"}}
```

Playing a turn (campaign mode: every action goes through the game's interface and is verified on screen; the
next save shows it in `hoi4_state` and `hoi4_briefing`):

```json
{"name": "hoi4_focus", "arguments": {"action": "list"}}
{"name": "hoi4_focus", "arguments": {"action": "select", "focus": "USA_continue_the_new_deal"}}
{"name": "hoi4_research", "arguments": {"action": "list", "ahead": false}}
{"name": "hoi4_research", "arguments": {"action": "start", "tech": "basic_machine_tools"}}
{"name": "hoi4_production", "arguments": {"action": "add", "equipment": "support_equipment_1", "factories": 2}}
{"name": "hoi4_construction", "arguments": {"action": "build", "building": "industrial_complex", "state": "Michigan"}}
{"name": "hoi4_politics", "arguments": {"action": "options", "slot": "political_advisor"}}
{"name": "hoi4_politics", "arguments": {"action": "hire", "advisor": "Harold L. Ickes"}}
{"name": "hoi4_decisions", "arguments": {"action": "list", "available": true}}
{"name": "hoi4_decisions", "arguments": {"action": "take", "decision": "USA_statehood_for_alaska"}}
{"name": "hoi4_diplomacy", "arguments": {"action": "act", "country": "MEX", "do": "improve_relations"}}
{"name": "hoi4_time", "arguments": {"action": "step", "days": 30}}
```

Working efficiently: the game is slow to load (one to three minutes) and shared, so an agent reuses the running game and
the loaded position (`hoi4_status` first), loads a save only to start or to change the position, saves only when a deep
read is needed (about one save per briefing) and repeats a failed step on its own instead of restarting a whole
sequence.

Playing on (campaign mode; the loop of the playbook: advance, read what stopped the run, act, record):

```json
{"name": "hoi4_journal", "arguments": {"action": "add", "kind": "plan", "text": "Industry first, then the Rio Grande border."}}
{"name": "hoi4_advance", "arguments": {"days": 30, "stop_on": ["tech_done", "focus_done", "war_started", "losing"]}}
{"name": "hoi4_advance", "arguments": {"until": "1 Jan 1937", "popup_rules": [{"match": "Earhart", "option": "^No"}]}}
{"name": "hoi4_journal", "arguments": {"action": "report"}}
```

Fighting a war (campaign mode; armies are found by the name on their panel, places by their localised names):

```json
{"name": "hoi4_army", "arguments": {"action": "list"}}
{"name": "hoi4_army", "arguments": {"action": "design", "template": "Infantry Division", "duplicate": true, "new_name": "Infantry Division II", "add": [{"type": "infantry", "column": 4}]}}
{"name": "hoi4_army", "arguments": {"action": "recruit", "template": "Infantry Division II", "count": 2, "state": "Louisiana"}}
{"name": "hoi4_army", "arguments": {"action": "create_army", "state": "San Antonio", "name": "Rio Grande Army", "general": "George S. Patton"}}
{"name": "hoi4_orders", "arguments": {"action": "front", "army": "Rio Grande Army", "target": "Coahuila"}}
{"name": "hoi4_orders", "arguments": {"action": "offensive", "army": "Rio Grande Army", "target": "Coahuila"}}
{"name": "hoi4_orders", "arguments": {"action": "execute", "army": "Rio Grande Army"}}
{"name": "hoi4_navy_air", "arguments": {"action": "naval_mission", "fleet": "Battle Force", "mission": "patrol", "region": "Western Seaboard", "near": "California"}}
{"name": "hoi4_time", "arguments": {"action": "step", "days": 30, "speed": 5}}
{"name": "hoi4_orders", "arguments": {"action": "status", "army": "Rio Grande Army", "target": "Coahuila"}}
{"name": "hoi4_army", "arguments": {"action": "deploy", "template": "Infantry Division II", "early": true}}
```

The campaign tools read structure from the game's and the enabled mods' files (focus trees, technologies and the
tech tree layout, decision categories, laws, advisors, buildings, equipment), so they work for any country and
mod that keeps the vanilla interface; the screen is used to locate and confirm. The first call after a playset
change builds small indexes of those files (10-20 s each, cached on disk afterwards).

## The telemetry companion mod

`hoi4_state` and `hoi4_briefing` work from the save and the screen alone, but they are then only as current as the
newest save. The companion mod `hoi4live_telemetry` makes them current to the last in-game day: the server writes
it into the user mod folder, and once it is in the playset the game logs the player's numbers every day and a world
summary every week as `HOI4LIVE` lines in `game.log`, which the server reads incrementally.

```json
{"name": "hoi4_telemetry", "arguments": {"action": "enable"}}
{"name": "hoi4_launch", "arguments": {"restart": true}}
{"name": "hoi4_telemetry", "arguments": {"action": "status"}}
{"name": "hoi4_telemetry", "arguments": {"action": "watch", "variables": ["my_mod_counter"], "flags": ["my_mod_started"]}}
{"name": "hoi4_telemetry", "arguments": {"action": "disable"}}
```

- It is tiny and additive: on_actions (`on_daily` and `on_weekly` for the player only; war, peace, faction,
  government and state-control events), a scripted effects file and one localisation file per language, all
  prefixed `hoi4live_`. No vanilla file is overridden and no game state changes (only temporary variables). It
  works with any mod: resources and equipment archetypes are taken from the game's and the enabled mods' files.
- `enable` backs up `dlc_load.json` (to `Documents/Paradox Interactive/Hearts of Iron IV/hoi4live_backups/`) and
  appends the mod after the existing ones; `disable` removes only that entry; `restore` puts back the original
  file. The game reads mods at launch, so restart it after `enable`, `watch` or `install`. Starting the game through
  the Paradox launcher rewrites `dlc_load.json` from the launcher's own playset (the mod is then off until enabled
  again).
- Every `day` line carries the generation id of the files, so `hoi4_telemetry status` tells whether the running
  game loaded the current files, and it lists any error.log line that points at the mod.
- Line format: `[time][date][effectbase.cpp:1783]: HOI4LIVE "k":"day","t":"USA","n":706650,"pp":13.38,...` (flat
  JSON members: the game's localisation drops `{`). Kinds: `day`, `pol`, `fac`, `subject`, `res`, `eq`, `war`,
  `watch` (daily), `wc`, `we`, `wld`, `front`, `lost` (weekly), `ev` (events). See
  `hoi4_live/telemetry/modgen.py`.

## Working on a shared desktop

- **Desktop lock.** Every acting tool holds `%TEMP%\hoi4_live_desktop.lock` (`{pid, purpose, time}`) only while it
  acts; a holder that died or has not refreshed it for 10 minutes is stale. Hold it across a multi-step run with
  `hoi4_lock action=acquire` and always release it. Other agents on the same desktop take turns through it.
- **Only inside the game.** Input goes to the HOI4 window only: the window is found by its process (`hoi4.exe`),
  brought to the foreground first (the tool aborts instead of typing into another window), and every point must
  lie inside the client area.
- **Saves.** Writes only `hoi4live_*` saves. Any save can be loaded or queried; nothing is deleted. Save files are
  memory-mapped only for the duration of one query, so the game can always overwrite them.
- **Settings and playset.** `settings.txt` is never edited. `dlc_load.json` changes only through
  `hoi4_telemetry enable|disable|restore` (backed up first); `hoi4_launch` starts the game with whatever it enables.
- **Modes.** `test` (default) allows console commands, events and effects; `campaign` refuses them so an agent
  plays with human-equivalent actions. Switch with `hoi4_lock action=set_mode`.
- **Shared machine.** The server starts nothing heavy until used: the OCR host (one PowerShell process) stops
  after 10 idle minutes, ffmpeg runs only for a fallback capture. The game is never quit or relaunched unless a
  tool is asked to; leave it paused on the map when you finish.
- Clipboard: console commands are pasted and the previous clipboard text is restored.

## Configuration

`config.json` (every key optional; environment overrides `HOI4_LIVE_GAME_DIR`, `HOI4_LIVE_USER_DIR`,
`HOI4_LIVE_ARTIFACTS`, `HOI4_LIVE_MODE`, `HOI4_LIVE_CONFIG`):

| key | default | meaning |
|---|---|---|
| `game_dir`, `user_dir` | `auto` | install folder (Steam registry and libraries) and Documents/Paradox Interactive/Hearts of Iron IV |
| `artifacts_dir` | `artifacts` | screenshots, popup crops, scenario reports and the server log (`artifacts/logs/hoi4_live.log`) |
| `mode` | `test` | initial session mode |
| `save_prefix` | `hoi4live_` | prefix of every save the server writes |
| `lock.path`, `lock.stale_seconds` | `%TEMP%/hoi4_live_desktop.lock`, 600 | the shared desktop lock |
| `window.pin_client_to_origin` | true | keep a windowed game's client area at the screen's top-left (as other desktop tools on the machine expect) |
| `capture.method` | `auto` | `printwindow`, `ddagrab` or `auto` (PrintWindow, falling back to ffmpeg ddagrab) |
| `console.toggle_key` | `grave` | the console key (US layout `` ` ``) |
| `ocr.language`, `ocr.idle_shutdown_seconds` | `en-US`, 600 | Windows OCR language and host idle timeout |
| `screenshot.max_width` | 960 | default width of returned images |
| `launch.args`, `launch.debug_args` | `[]`, `["-debug"]` | game arguments |
| `time.poll_seconds` | 0.8 | screen polling while time runs |

## Development

```
cd tests
python -m unittest discover -s . -p "test_*.py"
```

Releases are built and checked as described in [docs/RELEASING.md](docs/RELEASING.md); how the HOI4 Mod Setup app and
the Agentic HOI4 Modding package install and use the server is in [docs/integration.md](docs/integration.md).

Tests cover the protocol, argument validation, save parsing, popup, top bar and console detection, the desktop lock, log deltas, the telemetry mod and its parsers, the state model and briefing, the script-file readers behind the campaign tools, the installer, the popup rules, the journal and the advance loop. The screen-reader tests need the Windows OCR host and the game's files.

Layout:

```
hoi4_live/
  protocol/    jsonrpc.py, server.py (stdio MCP), schema.py (argument validation)
  tools/       registry.py, dispatch.py, session_tools.py, lowlevel.py, testmode.py, time_tools.py,
               telemetry_tools.py, understanding.py (state, briefing, screen, map)
  desktop/     win32.py, window.py, capture.py, inputs.py, clipboard.py, lock.py, images.py
  ocr/         winocr.py + ocr_host.ps1 (persistent Windows.Media.Ocr host)
  game/        process.py, logs.py, console.py, timectl.py, saves.py, menu.py, popups.py, ui.py, mapview.py
  perception/  popups.py, topbar.py (date, pause, speed, resource bar), glyphs.py (game-font digits),
               tooltip.py, screen.py (screen class, panels, windows)
  state/       savefile.py (indexed text-save reader), queries.py, model.py (country model from a save),
               assemble.py (telemetry + save + screen), briefing.py
  telemetry/   modgen.py (the companion mod), playset.py (dlc_load.json), parser.py (HOI4LIVE lines)
  gamedata/    loc.py (localised names), scripts.py (idea slots, technology costs), scriptdb.py (effective script
               files, @constants, cached indexes), focus.py, tech.py (+ tech tree layout), decisions.py,
               politics.py (laws, advisors, slots), economy.py (buildings, equipment)
  game/    panels.py (open/close, dialogs, text colours), focusview.py, researchview.py, productionview.py,
               constructionview.py, politicsview.py, decisionsview.py, diplomacyview.py (and trade)
  tools/   campaign_common.py, politics_tools.py (focus, politics, decisions, diplomacy),
               economy_tools.py (research, production, construction)
  game/    armyview.py (army cards and panel, commanders, battle plan toolbar, plan lines), deployview.py
               (Recruit & Deploy, training lines, the division designer), navalairview.py (fleets, air wings, missions)
  state/   military.py (templates, divisions, armies and orders, army groups, fleets, air wings from a save)
  tools/   military_tools.py (hoi4_army, hoi4_orders, hoi4_navy_air)
  state/   advance.py (stop conditions and suggested tool calls), journal.py (the campaign log and report)
  tools/   campaign_loop_tools.py (hoi4_advance, hoi4_journal, the action-journal hook of the dispatcher);
               game/popups.py holds the popup policy
  actions/     verify.py (the verified-action primitive for campaign actions)
install.py     installer: private environment, per-user client registration, upgrade, uninstall
scripts/       build_release.py (the release archive), mcp_client.py, exit_check_phase1.py, exit_check_phase2.py, phase2_extra.py, exit_check_phase3.py,
               phase3_extra.py, exit_check_phase4.py, campaign_demo.py (example campaign run)
skills/        hoi4-live-campaign (the campaign playbook: the loop, which tool answers which situation)
```

## License

Apache License 2.0; see [LICENSE](LICENSE) and [NOTICE](NOTICE).
Hearts of Iron IV belongs to Paradox Interactive; this project is unaffiliated with Paradox Interactive.
