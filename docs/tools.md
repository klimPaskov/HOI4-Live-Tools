# Tool reference

Generated from the server's registry (`python -m hoi4_live docs`). Coordinates are game client pixels; `lock` says whether the tool takes the shared desktop lock (`acting`: for the whole call, `self`: only for its acting part, `none`: read-only). Test-only tools are refused in campaign mode.

## Session and safety

### `hoi4_launch`

Start Hearts of Iron IV (normal, or -debug) with the current playset and wait for the main menu. If the game already runs it is left alone (restart=true quits it through its own menu first). The playset and settings are never changed.

_lock: acting_

| argument | type | required | default | description |
|---|---|---|---|---|
| `debug` | boolean |  | false | launch with -debug |
| `wait_menu` | boolean |  | true |  |
| `restart` | boolean |  | false | quit a running game first |
| `timeout_s` | integer |  | 900 |  |
| `mode` | test \| campaign |  |  | switch the session mode first (campaign refuses console cheats) |

- `hoi4_launch {}` (normal launch)
- `hoi4_launch {"debug": true}` (debug build)

### `hoi4_load`

Load a save (by name, without .hoi4) through the game's Load Game dialog, from the map or the main menu. Verifies that the map date afterwards matches the save. Any save may be loaded; none is modified.

_lock: acting_

| argument | type | required | default | description |
|---|---|---|---|---|
| `save` | string | yes |  | save name without .hoi4 |
| `mode` | test \| campaign |  |  | switch the session mode first (campaign refuses console cheats) |
| `timeout_s` | integer |  |  |  |

- `hoi4_load {"save": "qa_001_fresh_head"}` (fresh 1936 USA test save)

### `hoi4_lock`

Desktop lock and session safety. status: who holds %TEMP%/hoi4_live_desktop.lock. acquire: hold it across several tool calls (a multi-step test run); release: give it back (always do this at the end). set_mode: test (console, events, eval allowed) or campaign (human-equivalent play only). record: append one JSON line per tool call (time, tool, argument summary, outcome) to `path` for overlaying a screen recording; an empty path stops it.

_lock: none_

| argument | type | required | default | description |
|---|---|---|---|---|
| `action` | status \| acquire \| release \| set_mode \| record \| set_images \| set_pace |  | "status" |  |
| `path` | string |  |  | record: the .jsonl file to append to; empty stops recording |
| `pace` |  |  |  | set_pace: careful (1.0), normal (0.6), fast (0.35), faster (0.2) or a number 0.05-2: the factor on every fixed wait of the UI tools |
| `images` | all \| errors \| none |  |  | set_images: which results carry a screenshot (errors: only failures; faster) |
| `purpose` | string |  | "hoi4-live-tools run" |  |
| `wait_s` | integer |  | 20 |  |
| `mode` | test \| campaign |  |  |  |

- `hoi4_lock {"action": "acquire", "purpose": "event 006 test"}`
- `hoi4_lock {"action": "release"}`
- `hoi4_lock {"action": "set_mode", "mode": "campaign"}`
- `hoi4_lock {"action": "record", "path": "C:/recordings/campaign/calls.jsonl"}`

### `hoi4_new_game`

Start a new game from the main menu (or from a running game with exit_to_menu=true): Single Player, New Game, the scenario (bookmark label, e.g. '1939'), a major country by its name on the Select Country screen, START. In test mode `tag` switches to any country afterwards with the console. Waits until the map is up.

_lock: acting_

| argument | type | required | default | description |
|---|---|---|---|---|
| `bookmark` | string |  |  | scenario label regex, e.g. '1936' or 'August, 1939' |
| `country` | string |  |  | a major's name as shown, e.g. 'United States', 'German Reich' |
| `exit_to_menu` | boolean |  | false | resign the running game first (unsaved progress is lost) |
| `tag` | string |  |  | switch to this tag (test mode) |
| `mode` | test \| campaign |  |  | switch the session mode first (campaign refuses console cheats) |

- `hoi4_new_game {"country": "United States"}` (1936 USA)
- `hoi4_new_game {"country": "German Reich", "tag": "SWE"}` (start, then tag-switch to Sweden (test mode))
- `hoi4_new_game {"bookmark": "1939", "country": "Soviet Union", "exit_to_menu": true}`

### `hoi4_save`

Save the running game. The name always gets the hoi4live_ prefix (other saves are never written). Test mode uses the console `save`; campaign mode uses the Save Game dialog. Waits until the file is complete and reports its player and date.

_lock: acting_

| argument | type | required | default | description |
|---|---|---|---|---|
| `name` | string | yes |  | letters, digits, _ and - |
| `overwrite` | boolean |  | true |  |

- `hoi4_save {"name": "smoke"}` (writes save games/hoi4live_smoke.hoi4)

### `hoi4_shutdown`

Quit Hearts of Iron IV through its own menu. Needs confirm=true. force=true terminates a frozen game, but only the process this server launched. The game is shared: prefer leaving it running and paused.

_lock: acting_

| argument | type | required | default | description |
|---|---|---|---|---|
| `confirm` | boolean | yes |  | must be true |
| `force` | boolean |  | false |  |

- `hoi4_shutdown {"confirm": true}` (graceful quit)

### `hoi4_status`

Situation check without acting: is the game running, its window and version, the playset, the session mode, who holds the desktop lock, recent saves, log freshness and (look=true) what the screen shows: screen class, in-game date, running/paused, open popups and console. Call it first.

_lock: none_

| argument | type | required | default | description |
|---|---|---|---|---|
| `look` | boolean |  | true | read the screen (date, popups, screen class) |
| `screenshot` | boolean |  | false | attach a screenshot |

- `hoi4_status {}` (full status)
- `hoi4_status {"look": false}` (process/lock only, instant)

## Screen and input

### `hoi4_click`

Click inside the game window at client (x, y), or on the text matching `text` (OCR). Verifies by re-reading the screen: reports how much changed overall and near the click, with a screenshot.

_lock: acting_

| argument | type | required | default | description |
|---|---|---|---|---|
| `x` | integer |  |  |  |
| `y` | integer |  |  |  |
| `text` | string |  |  | regex of a visible label to click instead of x/y |
| `button` | left \| right \| middle |  | "left" |  |
| `clicks` | integer |  | 1 |  |
| `verify` | boolean |  | true | re-read the screen and attach a screenshot |

- `hoi4_click {"x": 960, "y": 540}`
- `hoi4_click {"text": "Load Game"}`
- `hoi4_click {"x": 100, "y": 200, "button": "right"}`

### `hoi4_drag`

Drag with a mouse button from (x0, y0) to (x1, y1) over `seconds` (map panning with right/middle, box selection with left, drawing front lines later).

_lock: acting_

| argument | type | required | default | description |
|---|---|---|---|---|
| `x0` | integer | yes |  |  |
| `y0` | integer | yes |  |  |
| `x1` | integer | yes |  |  |
| `y1` | integer | yes |  |  |
| `button` | left \| right \| middle |  | "left" |  |
| `seconds` | number |  | 0.6 |  |
| `verify` | boolean |  | true | re-read the screen and attach a screenshot |

- `hoi4_drag {"x0": 900, "y0": 500, "x1": 700, "y1": 500, "button": "middle"}` (pan the map)

### `hoi4_hover`

Read a tooltip: move the cursor to (x, y) or onto a named top-bar item (target), wait, find the tooltip box that appeared (screen difference) and OCR it. Returns the title, the lines and 'label: value' pairs, with a crop of the tooltip. The cursor is parked afterwards (stay=true leaves it, so the tooltip stays open). Targets: political_power, stability, war_support, manpower, factories, fuel, supply, convoys, command_power, army_experience, navy_experience, air_experience, world_tension, date.

_lock: acting_

| argument | type | required | default | description |
|---|---|---|---|---|
| `x` | integer |  |  |  |
| `y` | integer |  |  |  |
| `target` | air_experience \| army_experience \| command_power \| convoys \| date \| factories \| fuel \| manpower \| navy_experience \| political_power \| stability \| supply \| war_support \| world_tension |  |  |  |
| `wait_s` | number |  | 0.9 |  |
| `ocr` | boolean |  | true |  |
| `stay` | boolean |  | false |  |
| `radius` | integer |  | 420 |  |

- `hoi4_hover {"target": "factories"}` (factories in use by type, energy)
- `hoi4_hover {"target": "political_power"}`
- `hoi4_hover {"x": 960, "y": 540}` (a map province)

### `hoi4_key`

Press a key or chord in the game: 'esc', 'space', 'enter', 'f1', 'q', 'ctrl+s', 'shift+f5', 'add' (numpad +), 'grave' (console key). times repeats it.

_lock: acting_

| argument | type | required | default | description |
|---|---|---|---|---|
| `keys` | string | yes |  |  |
| `times` | integer |  | 1 |  |
| `verify` | boolean |  | true | re-read the screen and attach a screenshot |

- `hoi4_key {"keys": "esc"}`
- `hoi4_key {"keys": "space"}` (pause/unpause)

### `hoi4_ocr`

Read text on the game screen with Windows OCR. Returns lines with boxes in client pixels; pattern keeps only matching lines; words=true adds word boxes. Small regions are upscaled automatically for small game fonts. preprocess=white_text keeps only white glyphs (top bar, console, tooltips over busy backgrounds).

_lock: none_

| argument | type | required | default | description |
|---|---|---|---|---|
| `region` | array of integer |  |  | [x, y, width, height] in game client pixels |
| `scale` | number |  |  |  |
| `pattern` | string |  |  | regex filter (case-insensitive) |
| `words` | boolean |  | false |  |
| `preprocess` | none \| white_text \| invert |  | "none" |  |

- `hoi4_ocr {"pattern": "Load Game"}`
- `hoi4_ocr {"region": [600, 250, 700, 450]}`
- `hoi4_ocr {"region": [1620, 6, 170, 28], "preprocess": "white_text"}` (the date at 1080p)

### `hoi4_screenshot`

Screenshot of the game client (optionally a region). Returns a downscaled PNG (max_width, 0 = full size) and saves the full image under artifacts/. focus=true brings the game to the front first (takes the lock).

_lock: self_

| argument | type | required | default | description |
|---|---|---|---|---|
| `region` | array of integer |  |  | [x, y, width, height] in game client pixels |
| `max_width` | integer |  | 960 |  |
| `format` | png \| jpeg |  | "png" |  |
| `name` | string |  | "screenshot" |  |
| `focus` | boolean |  | false |  |

- `hoi4_screenshot {}`
- `hoi4_screenshot {"region": [1500, 0, 420, 60], "max_width": 0}` (top-right, full size)

### `hoi4_scroll`

Mouse wheel at client (x, y): positive notches scroll up / zoom the map in, negative down / out.

_lock: acting_

| argument | type | required | default | description |
|---|---|---|---|---|
| `x` | integer | yes |  |  |
| `y` | integer | yes |  |  |
| `notches` | integer | yes |  |  |
| `verify` | boolean |  | true | re-read the screen and attach a screenshot |

- `hoi4_scroll {"x": 960, "y": 540, "notches": -5}` (zoom the map out)

### `hoi4_type`

Type text into the focused game field. paste=true uses the clipboard (restored afterwards), which is reliable while the game is busy and supports any character; enter=true presses Enter after.

_lock: acting_

| argument | type | required | default | description |
|---|---|---|---|---|
| `text` | string | yes |  |  |
| `paste` | boolean |  | false |  |
| `enter` | boolean |  | false |  |
| `verify` | boolean |  | true | re-read the screen and attach a screenshot |

- `hoi4_type {"text": "hoi4live_campaign_1", "enter": true}`

## Test mode

### `hoi4_console`

Run console commands in the game (test mode). Commands are pasted, executed in order, and the console output that appeared after each one is read back with OCR. The console is closed again afterwards and new error.log lines are reported.

_lock: acting, test mode only_

| argument | type | required | default | description |
|---|---|---|---|---|
| `commands` | array of string | yes |  |  |
| `read_output` | boolean |  | true |  |
| `keep_open` | boolean |  | false |  |

- `hoi4_console {"commands": ["time"]}`
- `hoi4_console {"commands": ["observe", "fow"]}`

### `hoi4_eval_effect`

Execute an inline effect through the console (test mode): `eval_effect <scope> = { <effect> }`, or without a scope on the player. Reports the console response and new error.log lines.

_lock: acting, test mode only_

| argument | type | required | default | description |
|---|---|---|---|---|
| `effect` | string | yes |  | effect script, e.g. 'add_political_power = 50' |
| `scope` | string |  |  | country tag or state id; omit for the player |

- `hoi4_eval_effect {"effect": "add_political_power = 100", "scope": "USA"}`
- `hoi4_eval_effect {"effect": "set_country_flag = hoi4live_probe"}`

### `hoi4_fire_event`

Fire an event through the console (test mode): `event <id> [TAG]`. Then waits for popups, reads them (kind, title, text, options) and with answer='first' answers them. Reports new error.log lines.

_lock: acting, test mode only_

| argument | type | required | default | description |
|---|---|---|---|---|
| `event` | string | yes |  | event id, e.g. my_mod.1 |
| `tag` | string |  |  |  |
| `answer` | none \| first |  | "none" |  |
| `wait_s` | number |  | 2.0 |  |

- `hoi4_fire_event {"event": "my_mod.1", "tag": "USA"}`
- `hoi4_fire_event {"event": "news.1", "answer": "first"}`

### `hoi4_logs`

Read the game logs (error.log, game.log, ...): the delta since a mark (session start, launch, load, or one you set), folded so repeated lines show once with a count, optionally filtered by regex. set_mark records a new mark now.

_lock: none_

| argument | type | required | default | description |
|---|---|---|---|---|
| `log` | error \| game \| system \| setup \| text \| exceptions |  | "error" |  |
| `since` | string |  | "session" | mark name: session | launch | load | console | event | eval | your own, or 'all' |
| `grep` | string |  |  |  |
| `limit` | integer |  | 100 |  |
| `tail` | integer |  |  | just the last N raw lines |
| `set_mark` | string |  |  |  |

- `hoi4_logs {"log": "error", "since": "load"}`
- `hoi4_logs {"set_mark": "before_fix"}`
- `hoi4_logs {"log": "game", "tail": 20}`

### `hoi4_save_query`

Query a plain-text save (read only): info (player, date, version, mods), keys (children of a path), get (parsed subtree), find (regex over the raw text, hits mapped to paths), variables / flags (a country's or the global ones), diff (leaves that differ between two saves under a path). save may be a name, a path, or 'latest'. Paths: countries/USA/variables, states/64, countries/*/flags.

_lock: none_

| argument | type | required | default | description |
|---|---|---|---|---|
| `save` | string | yes |  |  |
| `query` | info \| keys \| get \| find \| variables \| flags \| diff |  | "info" |  |
| `path` | string |  |  |  |
| `tag` | string |  |  | country tag for variables/flags (omit for global) |
| `pattern` | string |  |  | regex for find, or a name filter for variables/flags/diff |
| `other` | string |  |  | second save for diff (the newer one) |
| `limit` | integer |  | 200 |  |

- `hoi4_save_query {"save": "hoi4live_smoke", "query": "info"}`
- `hoi4_save_query {"save": "hoi4live_smoke", "query": "variables", "tag": "USA", "pattern": "my_flag"}`
- `hoi4_save_query {"save": "latest", "query": "find", "pattern": "last_fired_event_id"}`
- `hoi4_save_query {"save": "hoi4live_pre", "other": "hoi4live_post", "query": "diff", "path": "countries/USA"}`

### `hoi4_scenario`

Run a deterministic list of tool calls and write a report (Markdown + JSON) under artifacts/. Each step is {tool, args, label?, expect?, continue_on_error?}; expect.text / expect.not_text are regexes on the step's text output, expect.error=true asserts that the step fails. Images are kept as files. Holds the desktop lock for the whole run.

_lock: acting_

| argument | type | required | default | description |
|---|---|---|---|---|
| `name` | string |  | "scenario" |  |
| `steps` | array of object | yes |  |  |
| `stop_on_error` | boolean |  | true |  |

- `hoi4_scenario {"name": "smoke", "steps": [{"tool": "hoi4_status"}, {"tool": "hoi4_fire_event", "args": {"event": "my_mod.1", "tag": "USA", "answer": "first"}}, {"tool": "hoi4_time", "args": {"action": "step", "days": 10}}, {"tool": "hoi4_save", "args": {"name": "smoke"}}, {"tool": "hoi4_logs", "args": {"since": "session"}}]}`

## Time

### `hoi4_time`

Game time. status: date and whether time runs. pause / run: set the state (verified by the clock). speed: 1-5. step: run `days`/`hours` forward and stop paused; popups that appear are handled by `popups` (first = answer each with its first option, choices = answer news and one-option events, stop at a real choice, stop = stop and report them, ignore). In test mode step arms `pause_in_hours` so the game stops itself on time. Returns every popup seen with title, text and options.

_lock: acting_

| argument | type | required | default | description |
|---|---|---|---|---|
| `action` | status \| pause \| run \| speed \| step |  | "status" |  |
| `days` | integer |  | 0 |  |
| `hours` | integer |  | 0 |  |
| `speed` | integer |  |  |  |
| `popups` | first \| choices \| stop \| ignore |  | "first" |  |
| `max_seconds` | integer |  | 600 |  |
| `screenshot` | boolean |  | true |  |

- `hoi4_time {"action": "status"}`
- `hoi4_time {"action": "step", "days": 10}`
- `hoi4_time {"action": "step", "days": 30, "speed": 4, "popups": "stop"}`
- `hoi4_time {"action": "pause"}`

## Popups

### `hoi4_popups`

Event, news and super-event windows on screen. list: kinds, boxes and option buttons (fast, no OCR). read: also OCR title, text and option labels. answer: click one option (popup index, option index or option_text regex) and verify the window closed. answer_all: answer every popup with its first option. Screenshots of answered popups are saved.

_lock: self_

| argument | type | required | default | description |
|---|---|---|---|---|
| `action` | list \| read \| answer \| answer_all |  | "read" |  |
| `popup` | integer |  | 0 |  |
| `option` | integer |  | 0 |  |
| `option_text` | string |  |  |  |
| `screenshot` | boolean |  | false |  |

- `hoi4_popups {"action": "read"}`
- `hoi4_popups {"action": "answer", "option": 1}`
- `hoi4_popups {"action": "answer", "option_text": "near miss"}`
- `hoi4_popups {"action": "answer_all"}`

## Understanding

### `hoi4_briefing`

One-page situation report for the player: date and game speed, what needs a decision now (popups, idle focus or research, free factories, unspent political power, equipment and resource shortfalls, losing wars), government, wars and fronts, economy, military, focus and research, world, recent events (telemetry events, answered popups, optional game.log notes) and what changed since the previous briefing. Built from hoi4_state with the screen read; each source is dated. Markdown in `markdown`, facts as data.

_lock: self_

| argument | type | required | default | description |
|---|---|---|---|---|
| `tag` | string |  |  |  |
| `save` | string |  | "latest" | deep data source: 'latest' (newest save of this country), 'none', 'fresh' (test mode: write hoi4live_state now, ~20 s), or a save name |
| `screen` | boolean |  | true | read the top bar, pause state, popups |
| `world` | boolean |  | true | include the weekly world summary |
| `log_notes` | integer |  | 0 | add the last N non-telemetry game.log lines (mod messages) |
| `remember` | boolean |  | true | store this briefing's facts for the next one's deltas |

- `hoi4_briefing {}` (full briefing for the player)
- `hoi4_briefing {"save": "fresh"}` (test mode: write a save first so focus and research are current)
- `hoi4_briefing {"screen": false, "save": "none"}` (telemetry only (fast, no screen read))

### `hoi4_map`

The map. modes: the map modes on the F-key buttons (read from their tooltips once, cached). view: switch the map mode (name or F-key), move the camera to a state (id or name, 'capital', or 'front'), zoom (close, province, region, country, continent, world, or notches) and take a screenshot (optionally a region). front: the front overview from telemetry (enemy-held states touching ours by enemy, our front states, lost states, wars) plus a view centred on the front. at: the tooltip of the map at (x, y) (province, state, owner, what the map mode shows).

_lock: acting_

| argument | type | required | default | description |
|---|---|---|---|---|
| `action` | modes \| view \| front \| at |  | "view" |  |
| `mode` | string |  |  | map mode name (default, supply, terrain, resistance, ...) or F1-F10 |
| `target` | string |  |  | state id, state or province name, 'capital' or 'front' |
| `zoom` | string |  | "region" | close | province | region | country | continent | world | notches like -5 |
| `x` | integer |  |  |  |
| `y` | integer |  |  |  |
| `region` | array of integer |  |  |  |
| `max_width` | integer |  | 1280 |  |
| `recalibrate` | boolean |  | false |  |

- `hoi4_map {"action": "modes"}`
- `hoi4_map {"action": "view", "target": "capital", "zoom": "country"}`
- `hoi4_map {"action": "view", "mode": "supply", "target": "Texas", "zoom": "region"}`
- `hoi4_map {"action": "front"}`
- `hoi4_map {"action": "at", "x": 960, "y": 540}`

### `hoi4_screen`

What is on screen, without acting: the screen class (map, esc_menu, main_menu, load_screen, loading, ...), the open panel or full-screen view on the map (politics, decisions, research, trade, construction, production, deployment, logistics, officer_corps, faction, army/navy/air overview, focus_tree, diplomacy) with the key that toggles it, any other window in the centre with its title, the console, popups, the date, pause and speed, and the top-bar values (political power, stability, war support, manpower, factories, fuel, supply, convoys, command power, experience, world tension).

_lock: none_

| argument | type | required | default | description |
|---|---|---|---|---|
| `topbar` | boolean |  | true |  |
| `popups` | boolean |  | true | read popup titles and options (OCR) |
| `screenshot` | boolean |  | false |  |

- `hoi4_screen {}`
- `hoi4_screen {"topbar": false, "screenshot": true}`

### `hoi4_state`

One structured model of a country, merged from the telemetry mod (current to the last in-game day), the newest plain-text save (deep: focus, research, production, construction, laws, wars, states) and optionally the top bar on screen. Every section lists its sources with their in-game date (freshness per field). sections: overview, politics, economy, military, diplomacy, focus, research, production, construction, decisions, states, world, script. Read-only.

_lock: self_

| argument | type | required | default | description |
|---|---|---|---|---|
| `tag` | string |  |  | default: the player |
| `sections` | array of string |  | ["overview", "politics", "economy", "military", "diplomacy", "focus", "research"] |  |
| `save` | string |  | "latest" | deep data source: 'latest' (newest save of this country), 'none', 'fresh' (test mode: write hoi4live_state now, ~20 s), or a save name |
| `screen` | boolean |  | false | also read the top bar on screen |
| `limit` | integer |  | 40 |  |
| `raw` | boolean |  | false | include the raw source records |

- `hoi4_state {}` (the player: overview, politics, economy, military, diplomacy, focus, research)
- `hoi4_state {"sections": ["economy", "construction", "production"], "screen": true}`
- `hoi4_state {"tag": "GER", "sections": ["overview", "focus", "research"], "save": "autosave"}`
- `hoi4_state {"sections": ["world", "script"], "save": "none"}` (telemetry only, instant)

### `hoi4_telemetry`

The telemetry companion mod hoi4live_telemetry: a tiny generated mod that logs the player's numbers daily (political power, stability, war support, manpower, factories, divisions, fuel, convoys, resources, equipment, wars) and a weekly world summary as HOI4LIVE lines in game.log, read by hoi4_state and hoi4_briefing. status: files, playset, whether the running game loaded them, records seen, suspect error.log lines. install: (re)generate the files. enable: install, then add it last to dlc_load.json (backed up first). disable: remove it from dlc_load.json. restore: put back the dlc_load.json from before the first enable. watch: add (or remove=true) player/global variables and flags to log daily. read: the latest telemetry records. The game loads mods only at launch: after install/enable/watch, restart it (hoi4_launch restart=true) and load a save.

_lock: none_

| argument | type | required | default | description |
|---|---|---|---|---|
| `action` | status \| install \| enable \| disable \| restore \| watch \| read |  | "status" |  |
| `variables` | array of string |  |  | watch: player country variables |
| `global_variables` | array of string |  |  |  |
| `flags` | array of string |  |  | watch: player country flags |
| `global_flags` | array of string |  |  |  |
| `remove` | boolean |  | false | watch: remove the names instead |
| `kinds` | array of string |  |  | read: record kinds (day, pol, fac, res, eq, war, watch, wc, we, wld, front, lost, ev) |
| `raw` | boolean |  | false | read: raw records instead of the snapshot |
| `limit` | integer |  | 40 |  |

- `hoi4_telemetry {}` (is telemetry installed, enabled and live?)
- `hoi4_telemetry {"action": "enable"}` (generate and add to the playset (restart the game after))
- `hoi4_telemetry {"action": "watch", "variables": ["my_mod_counter"], "flags": ["my_mod_started"]}`
- `hoi4_telemetry {"action": "read", "kinds": ["day", "war"]}`
- `hoi4_telemetry {"action": "disable"}`

## Campaign actions (politics and economy)

### `hoi4_construction`

Construction. queue: the construction queue (state, factories assigned) and civilian factories used/total. buttons: the building buttons of the panel, named from their tooltips (cached). build: queue a building (id such as industrial_complex, arms_factory, infrastructure, or its localised name) in a state (id or name): the building's button is pressed, the map camera moves to the state through the Find view, the state is clicked `count` times and the queue is read back. plan: queue `levels` of a building over the states with the most free building slots (from the telemetry's weekly state scan: no map reading), at most `per_state` in each; `states` limits the choice, `choose` orders it (free_slots, industry, infrastructure), dry_run only lists the plan. Several builds in one call.

_lock: acting_

| argument | type | required | default | description |
|---|---|---|---|---|
| `action` | queue \| buttons \| build \| plan |  | "queue" |  |
| `levels` | integer |  | 6 | plan: total levels |
| `per_state` | integer |  | 3 | plan: most per state |
| `states` | array of string |  |  | plan: only these states (ids or names) |
| `choose` | free_slots \| industry \| infrastructure |  | "free_slots" | plan: free_slots (most free slots first), industry (most factories first), infrastructure (best infrastructure first) |
| `dry_run` | boolean |  | false | plan: list the plan, build nothing |
| `building` | string |  |  | build: building id or localised name |
| `state` | string |  |  | build: state id or localised name |
| `count` | integer |  | 1 | build: how many levels (one click each) |
| `refresh` | boolean |  | false | buttons/build: re-read the tooltips |

- `hoi4_construction {"action": "queue"}`
- `hoi4_construction {"action": "build", "building": "industrial_complex", "state": "Michigan"}`
- `hoi4_construction {"action": "build", "building": "Infrastructure", "state": "376", "count": 2}`

### `hoi4_decisions`

Decisions. list: the decision panel's categories and decisions as the game shows them (cost, whether it can be taken: green row = conditions met, red cost = not enough political power, timers of decisions in progress), matched to their ids and file data (conditions, effects, cooldowns). details: the tooltip of one decision. take: take a decision (id or localised name), verified by the row and the political power.

_lock: acting_

| argument | type | required | default | description |
|---|---|---|---|---|
| `action` | list \| details \| take |  | "list" |  |
| `decision` | string |  |  | details/take: decision id or localised name |
| `category` | string |  |  | list: only this category (name or regex) |
| `available` | boolean |  | false | list: only decisions that can be taken |
| `force` | boolean |  | false | take: click even when the row looks unavailable |

- `hoi4_decisions {}`
- `hoi4_decisions {"available": true}`
- `hoi4_decisions {"action": "details", "decision": "Small Lobbying Effort"}`
- `hoi4_decisions {"action": "take", "decision": "USA_small_lobby_effort"}`

### `hoi4_diplomacy`

Diplomacy with a country (tag), and trade. view: open the country's diplomacy view (camera to its capital, state view, owner flag) and list its actions with cost and a mark (ok / blocked). act: perform an action by name or alias (improve_relations, guarantee, justify, invite_to_faction, volunteers, lend_lease, military_access, non_aggression_pact, attache, embargo, license; declare_war only with confirm_war=true): blocked actions return their tooltip; a follow-up window is answered with `option` (a war goal type ...) or its confirm button; the action list, header and political power are compared before and after. trade: the trade panel (civilian factories for trade, resources, exporting countries).

_lock: acting_

| argument | type | required | default | description |
|---|---|---|---|---|
| `action` | view \| act \| trade |  | "view" |  |
| `country` | string |  |  | target country tag |
| `do` | string |  |  | act: action name or alias |
| `option` | string |  |  | act: text of the choice in a follow-up window |
| `states` | array of string |  |  | justify: the states of the war goal window to select (names as shown, or ['all'] for the first five) |
| `via` | string |  |  | a state of that country as the game displays it (its name or a sibling's), used when the capital's name is not found in the Find view; a name that worked is remembered per country |
| `confirm_war` | boolean |  | false |  |

- `hoi4_diplomacy {"action": "view", "country": "CAN"}`
- `hoi4_diplomacy {"action": "act", "country": "MEX", "do": "improve_relations"}`
- `hoi4_diplomacy {"action": "act", "country": "CUB", "do": "justify", "option": "Puppet"}`
- `hoi4_diplomacy {"action": "trade"}`

### `hoi4_focus`

National focus. list: the focuses of the country's tree with status (available / current / locked / excluded / completed, from the files and the newest save), days, prerequisites, mutually exclusive focuses, script requirements and rewards; screen=true also reads the current focus on screen. details: the game's detail window for one focus (requirements met/unmet in colour, days, description, effects). select: start a focus through the tree view (search box, detail window, Start), verified on the politics panel. progress: the current focus and its progress. cancel: cancel the focus in progress. The focus is an id or its localised name.

_lock: self_

| argument | type | required | default | description |
|---|---|---|---|---|
| `action` | list \| details \| select \| progress \| cancel |  | "list" |  |
| `focus` | string |  |  | focus id or localised name (details, select) |
| `status` | available \| all \| locked \| completed \| current |  | "available" |  |
| `filter` | string |  |  | regex on id, name, rewards or search filters (list) |
| `search` | string |  |  | select/details: text to type in the tree's search box when the focus name has placeholders the game fills in |
| `if_idle` | boolean |  | false | select: do nothing (deferred=true, no error) when a focus is in progress; for standing orders, which retry on the next firing |
| `replace` | boolean |  | false | select: cancel the focus in progress first |
| `force` | boolean |  | false | select even when the detail window shows unmet requirements |
| `screen` | boolean |  | false | list: also read the current focus on screen |
| `keep_open` | boolean |  | false | leave the focus tree open |
| `limit` | integer |  | 40 |  |
| `tag` | string |  |  | default: the player |
| `save` | string |  | "telemetry" | where completed focuses, researched technologies and similar history come from: 'telemetry' (the telemetry mod's technology scan where a tool has one, else the newest save), 'latest' (newest save of this country), 'none', or a save name |

- `hoi4_focus {}` (available focuses of the player's tree)
- `hoi4_focus {"action": "details", "focus": "War Plans Division"}`
- `hoi4_focus {"action": "select", "focus": "USA_continue_the_new_deal"}`
- `hoi4_focus {"action": "progress"}`
- `hoi4_focus {"action": "list", "status": "all", "filter": "industr", "limit": 20}`

### `hoi4_peace`

The peace conference that opens when a war is won (the clock stops with peace_conference). status: score, turn, the loser and its states with their costs. take: select demands: all=true (Select All) or states=[...] (names as listed); submit=true then submits them (final for the turn). submit: submit what is selected. exit: Confirm and Exit (OK). pass: pass the turn. Use it as soon as the clock reports peace_conference; taking every state of a defeated country annexes it.

_lock: acting_

| argument | type | required | default | description |
|---|---|---|---|---|
| `action` | status \| take \| submit \| exit \| pass |  | "status" |  |
| `all` | boolean |  | false | take: Select All |
| `states` | array of string |  |  | take: states by name |
| `submit` | boolean |  | false | take: submit right away |
| `exit` | boolean |  | false | take/submit: leave the conference after |

- `hoi4_peace {"action": "status"}`
- `hoi4_peace {"action": "take", "all": true, "submit": true, "exit": true}` (annex the defeated country completely and leave)
- `hoi4_peace {"action": "take", "states": ["Tirol", "Vorarlberg"], "submit": true}`

### `hoi4_politics`

Laws, advisors and ideas. view: the government (laws, national spirits, ideas and advisors from the newest save; the slots of the politics panel read from their tooltips: laws, political advisors, and with rows=all the designers, theorist and military staff). options: the selection list of a slot (laws with their political power cost, advisors with traits and cost; 'blocked' when the game shows a red X or a red cost). change_law: adopt a law (id or name) through its slot. hire: hire an advisor (name or idea token) into a slot. Every change is verified on the slot's tooltip and the political power on the top bar.

_lock: self_

| argument | type | required | default | description |
|---|---|---|---|---|
| `action` | view \| options \| change_law \| hire |  | "view" |  |
| `slot` | string |  |  | options/hire: economy, trade_laws, mobilization_laws, political_advisor, theorist, industrial_concern, army_chief, ... |
| `law` | string |  |  | change_law: law idea id or localised name |
| `advisor` | string |  |  | hire: advisor name or idea token |
| `rows` | government \| all \| none |  | "government" | view: which rows of slots to read on screen |
| `force` | boolean |  | false | click even when the choice is blocked |
| `tag` | string |  |  | default: the player |
| `save` | string |  | "telemetry" | where completed focuses, researched technologies and similar history come from: 'telemetry' (the telemetry mod's technology scan where a tool has one, else the newest save), 'latest' (newest save of this country), 'none', or a save name |

- `hoi4_politics {}` (laws, spirits and advisors)
- `hoi4_politics {"action": "options", "slot": "economy"}`
- `hoi4_politics {"action": "change_law", "law": "partial_economic_mobilisation"}`
- `hoi4_politics {"action": "hire", "advisor": "Harold L. Ickes"}`

### `hoi4_production`

Military production. lines: every production line (number, equipment, type, factories) and the used/total military factories and dockyards. add: add a line for an equipment (variant name, equipment id or type name; category land/armor/air/naval or auto), choose 'produce without' or a named military industrial organisation, then set its factories. assign: set a line's factories (factories = target count, or delta). change: switch a line to other equipment (its progress is lost). remove: delete a line. Lines are found by number or name. fill: put the idle military factories on the lines `targets` names, in order, up to each line's max (one call instead of reading and assigning line by line). Every action re-reads the line and the factory header.

_lock: acting_

| argument | type | required | default | description |
|---|---|---|---|---|
| `action` | lines \| add \| assign \| change \| remove \| fill |  | "lines" |  |
| `targets` | array of object |  |  | fill: lines in priority order, each with the most factories it should reach; the free military factories go to the first line below its max, then the next |
| `equipment` | string |  |  | add/change: variant name, equipment id or type name |
| `category` | auto \| land \| armor \| air \| naval |  | "auto" |  |
| `line` | string |  |  | assign/change/remove: line number or equipment name |
| `factories` | integer |  |  | add: factories for the new line (default 1); assign: target count |
| `delta` | integer |  |  | assign: change by this many |
| `mio` | string |  |  | add/change: military industrial organisation to use |
| `tag` | string |  |  | default: the player |

- `hoi4_production {"action": "lines"}`
- `hoi4_production {"action": "add", "equipment": "Support Equipment", "factories": 3}`
- `hoi4_production {"action": "assign", "line": "M1 Garand", "factories": 4}`
- `hoi4_production {"action": "change", "line": "24", "equipment": "infantry_equipment_1"}`

### `hoi4_research`

Research. slots: the research panel's slots (idle, or the technology and its days left). list: technologies the country can research now (prerequisites from the files, researched ones from the newest save), by folder with year, cost and an ahead-of-time flag. start: research a technology (id or localised name) in a slot: the slot opens the tech tree, the folder tab and the tile are found from the interface files (calibrated on screen), the detail window's title is checked, Research is pressed and the slot is read back.

_lock: self_

| argument | type | required | default | description |
|---|---|---|---|---|
| `action` | slots \| list \| start |  | "slots" |  |
| `tech` | string |  |  | technology id or localised name (start) |
| `slot` | integer |  |  | slot number (default: the first idle slot) |
| `replace` | boolean |  | false | start: allow a busy slot |
| `folder` | string |  |  | list: only this folder (e.g. industry_folder) |
| `filter` | string |  |  | list: regex on id, name or categories |
| `ahead` | boolean |  | true | list: include technologies ahead of time |
| `limit` | integer |  | 60 |  |
| `tag` | string |  |  | default: the player |
| `save` | string |  | "telemetry" | where completed focuses, researched technologies and similar history come from: 'telemetry' (the telemetry mod's technology scan where a tool has one, else the newest save), 'latest' (newest save of this country), 'none', or a save name |

- `hoi4_research {"action": "slots"}`
- `hoi4_research {"action": "list", "ahead": false}`
- `hoi4_research {"action": "start", "tech": "basic_machine_tools"}`
- `hoi4_research {"action": "start", "tech": "Electronic Engineering", "slot": 2}`

## Campaign actions (military)

### `hoi4_army`

Land forces through the game's interface. list: armies (divisions, general, orders, executing), army groups, templates and unassigned divisions (newest save) plus the army cards on screen. templates: the Division Templates panel (names as shown) with each template's composition from the save. design: open a template in the division designer and change it: duplicate (save as a new template, the original stays), new_name, add battalions (type id or name, column 1-5; support companies with support=true), remove (column/row or type), then Save (costs army experience; refused with the game's reason). recruit: train count divisions of a template (optionally deploying to a state). deploy: deploy the ready divisions of a template's training line. create_army: select the divisions in a state (box on the map) and make a new army of them (optional name and general). assign: add the divisions in a state to an army. general: put a general (by name, or the first listed) in command. rename: rename an army. army_group: group named armies under a new army group. Every action is verified on screen (panels, cards, lists).

_lock: acting_

| argument | type | required | default | description |
|---|---|---|---|---|
| `action` | list \| templates \| design \| recruit \| deploy \| create_army \| assign \| general \| rename \| army_group |  | "list" |  |
| `template` | string |  |  | design/recruit/deploy: template name as shown |
| `duplicate` | boolean |  | false | design: save as a new template |
| `new_name` | string |  |  | design: template name; rename: army name |
| `add` | array of object |  |  | design: battalions to add |
| `remove` | array of object |  |  | design: slots to empty (column+row, or support_row) |
| `count` | integer |  | 1 | recruit: divisions |
| `state` | string |  |  | recruit/deploy: deployment state; create_army/assign: the state whose divisions are selected (localised name or id) |
| `box` | array of integer |  |  | create_army/assign: half width and height of the selection box around the state's centre at region zoom (default [220, 160]) |
| `army` | string |  |  | army name as on its panel |
| `army_index` | integer |  |  | army card number (left = 1) |
| `armies` | array of string |  |  | army_group: army names |
| `name` | string |  |  | create_army: name for the new army |
| `general` | string |  |  | general/create_army: commander name (default: first listed) |
| `early` | boolean |  | false | deploy: also deploy divisions that are only partly trained (at least 20 %) |
| `screen` | boolean |  | true | list: also read the army cards |
| `tag` | string |  |  | default: the player |
| `save` | string |  | "latest" | save the lists come from: 'latest' (newest save of the country), 'none', or a save name |

- `hoi4_army {"action": "list"}`
- `hoi4_army {"action": "templates"}`
- `hoi4_army {"action": "design", "template": "Infantry Division", "duplicate": true, "new_name": "Infantry Division II", "add": [{"type": "infantry", "column": 4}]}`
- `hoi4_army {"action": "recruit", "template": "Infantry Division", "count": 2, "state": "Texas"}`
- `hoi4_army {"action": "deploy", "template": "Infantry Division"}`
- `hoi4_army {"action": "create_army", "state": "Texas", "name": "Rio Grande Army", "general": "George S. Patton"}`
- `hoi4_army {"action": "general", "army": "Army 1", "general": "Omar Bradley"}`
- `hoi4_army {"action": "army_group", "armies": ["Army 1", "Army 2"]}`

### `hoi4_navy_air`

Basic navy and air missions. list: fleets (task forces, ships, mission) and air wings (base, planes, mission) from the newest save. naval_mission: in the Strategic Navy map mode select a fleet by name, press a mission (patrol, strike_force, convoy_raiding, naval_exercises, convoy_escort, minelaying, minesweeping, naval_invasion_support, hold) and right-click a sea region: region (its name, found by opening the region views around the camera) near a coastal state or province the Find view knows. air_mission: in the Strategic Air map mode click the air wings at the air base in base (a state), press a mission (air_superiority, cas, interception, strategic_bombing, naval_strike, port_strike, naval_patrol, recon ...) and right-click the region at region_at (a state). Verified on the fleet panel (Active in Regions, the task forces' 'assigned region' lines) and the air wing panel (each wing's region).

_lock: acting_

| argument | type | required | default | description |
|---|---|---|---|---|
| `action` | list \| naval_mission \| air_mission |  | "list" |  |
| `fleet` | string |  |  | naval_mission: fleet name |
| `mission` | string |  |  | mission name (see the description) |
| `region` | string |  |  | naval_mission: sea region name (e.g. Western Seaboard) |
| `near` | string |  |  | naval_mission: coastal state/province to centre the camera |
| `base` | string |  |  | air_mission: state with the air base |
| `region_at` | string |  |  | air_mission: state inside the region to cover (default: the base's state) |
| `tag` | string |  |  | default: the player |
| `save` | string |  | "latest" | save the lists come from: 'latest' (newest save of the country), 'none', or a save name |

- `hoi4_navy_air {"action": "list"}`
- `hoi4_navy_air {"action": "naval_mission", "fleet": "Battle Force", "mission": "patrol", "region": "Western Seaboard", "near": "San Diego"}`
- `hoi4_navy_air {"action": "air_mission", "base": "Texas", "mission": "air_superiority", "region_at": "Coahuila"}`

### `hoi4_orders`

Battle plans for an army, through the battle plan toolbar. status: the army's orders from the newest save, the toolbar title, whether its plan executes (the execute button's animation), the war score and states from telemetry and, with target, the map tooltip there (owner/controller). front: draw a front line against the country that holds target (a state on the other side of the border). offensive (or spearhead): paint an offensive line through target (a state in enemy territory beyond the front); the game draws the arrow from the front. execute / hold: start or stop the plan. delete: remove all of the army's orders. Every action is verified on screen (toolbar, plan lines near the target, execute animation). attack: the whole plan of one army in one call: (delete the old orders), front line against the owner of front_target, offensive line through target, execute. batch: attack for several armies (`orders`), failures listed per army.

_lock: acting_

| argument | type | required | default | description |
|---|---|---|---|---|
| `action` | status \| front \| offensive \| spearhead \| execute \| hold \| delete \| attack \| batch |  | "status" |  |
| `front_target` | string |  |  | attack: a state of the enemy along the border (the front line is drawn against its owner) |
| `replace` | boolean |  |  | front/offensive/spearhead: redraw even when the army already executes a plan (default: leave it, answer unchanged). attack: delete the old orders first (default true) |
| `orders` | array of object |  |  | batch: one attack per entry {army|army_index, front_target, target}; every army is tried, failures are listed |
| `army` | string |  |  | army name as on its panel |
| `army_index` | integer |  |  | army card number (left = 1) |
| `target` | string |  |  | front: a state of the enemy along the border; offensive: the state to advance to (localised name or id) |
| `length` | integer |  | 150 | offensive: length of the painted line in pixels (at region zoom) |
| `zoom` | close \| province \| region \| country \| continent |  |  | camera zoom for drawing (default: country for a front, region for an offensive) |
| `tag` | string |  |  | default: the player |
| `save` | string |  | "latest" | save the lists come from: 'latest' (newest save of the country), 'none', or a save name |

- `hoi4_orders {"action": "front", "army": "Army 1", "target": "Coahuila"}`
- `hoi4_orders {"action": "offensive", "army": "Army 1", "target": "Coahuila"}`
- `hoi4_orders {"action": "execute", "army": "Army 1"}`
- `hoi4_orders {"action": "status", "army": "Army 1", "target": "Coahuila"}`
- `hoi4_orders {"action": "hold", "army": "Army 1"}`

## Campaign loop

### `hoi4_advance`

Run game time until something needs a decision, then say what and what to do about it. Time runs in short chunks; after each the telemetry's daily numbers are compared with the start and the run stops on the first stop_on condition (a technology finished, a focus completed, free civilian or military factories, a war started or ended, states lost or gained, surrender progress, low stability, high political power, low fuel, a change at the front) or when `days` or the date `until` is reached. Popups are answered by the policy (popups: choices = news and one-option events are answered, an event with a real choice stops the run and is returned with its text and options for the agent to answer; rules can pre-decide events by title or text). The result lists why it stopped, what changed, the popups answered and still open, the attention items and the tool calls that answer them; each run is written to the journal. Without the telemetry mod only the date, popups and focus notices stop a run.

_lock: acting_

| argument | type | required | default | description |
|---|---|---|---|---|
| `days` | integer |  |  | run at most this many days (default 14, or up to `until`) |
| `until` | string |  |  | stop at this date: 1936.3.1, 1936-03-01 or '1 Mar 1936' |
| `stop_on` | array of string |  |  | conditions that stop the run ([] = only the date, popups and errors); default: tech_done, focus_done, construction_done, production_idle, war_started, war_ended, territory_lost, losing |
| `thresholds` | object |  |  |  |
| `popups` | choices \| first \| stop \| ignore |  | "choices" |  |
| `popup_rules` | array of object |  |  | first matching rule decides; the popups policy handles the rest |
| `speed` | integer |  | 5 |  |
| `chunk_days` | integer |  | 2 | days between looks at the numbers (a stop is noticed at the end of a chunk) |
| `max_seconds` | integer |  | 900 | wall-clock limit for the whole call |
| `situation` | boolean |  | false | attention items from the full briefing (reads the newest save) instead of the telemetry alone |
| `journal` | boolean |  | true | write the run and answered events to the journal |
| `screenshot` | boolean |  | false |  |
| `background` | boolean |  | false | start the run and return at once: the game keeps running while you act (research, production, construction ...); collect the result with hoi4_clock action=join |
| `engine` | auto \| fast \| screen |  | "auto" | fast: telemetry-driven clock (no OCR in the loop); screen: the older chunked loop that reads the date by OCR (no telemetry needed); auto: fast when telemetry day records exist |

- `hoi4_advance {"days": 30}` (a month, stopping at the first decision point)
- `hoi4_advance {"days": 90, "stop_on": ["war_started", "losing"], "popups": "first"}` (quiet period: only wars and a losing war interrupt)
- `hoi4_advance {"until": "1 Jan 1937", "popup_rules": [{"match": "Earhart", "option": "No"}]}` (run to a date; one event decided in advance)

### `hoi4_clock`

The background advance. status: date, whether a background run goes on and its progress, from the telemetry (no screen). join: wait up to wait_s for a background hoi4_advance to stop and return its result (the same as a foreground hoi4_advance); while it still runs the call returns its status. stop: end the run now (the game is paused) and return its result. pause / resume: the game clock alone.

_lock: none_

| argument | type | required | default | description |
|---|---|---|---|---|
| `action` | status \| join \| stop \| pause \| resume |  | "status" |  |
| `wait_s` | integer |  | 60 |  |

- `hoi4_clock {"action": "status"}`
- `hoi4_clock {"action": "join", "wait_s": 120}`

### `hoi4_journal`

The campaign journal of the country being played (artifacts/journal/journal_<TAG>.jsonl). add: write a note, plan, decision (a choice and why) or lesson, stamped with the in-game date. read: the newest entries, filtered by kind, date or a regex; the server itself writes `action` (every verified campaign action), `advance` (each hoi4_advance run) and `popup` (answered events) entries. plan: the standing plan (the newest plan entry). report: a markdown run report of the campaign so far.

_lock: none_

| argument | type | required | default | description |
|---|---|---|---|---|
| `action` | add \| read \| plan \| report \| summary |  | "read" |  |
| `kind` | note \| plan \| decision \| lesson \| action \| advance \| popup |  | "note" | add: the entry kind (note, plan, decision, lesson) |
| `text` | string |  |  | add: the entry |
| `tags` | array of string |  |  |  |
| `kinds` | array of string |  |  | read: only these kinds |
| `since` | string |  |  | read: entries from this in-game date on (1936.3.1) |
| `query` | string |  |  | read: regex over text and tags |
| `limit` | integer |  | 20 |  |
| `auto` | boolean |  | true | read: include the entries the server wrote |
| `date` | string |  |  | add: in-game date (default: the telemetry's latest day) |
| `tag` | string |  |  | default: the player |

- `hoi4_journal {"action": "add", "kind": "plan", "text": "Industry first: 6 factories, then Army reform."}`
- `hoi4_journal {"action": "add", "kind": "decision", "text": "Declined the Mexican oil concession: we need the political power for the draft law."}`
- `hoi4_journal {"action": "read", "limit": 10, "auto": false}`
- `hoi4_journal {"action": "report"}`

### `hoi4_standing`

Standing orders: decisions you declare once and the clock executes when their situation comes up, so game time never waits for your next turn. A rule says WHEN (research_idle, tech_done, focus_done, construction_done, production_idle, pp_high, low_stability, war_started, ...) and WHAT: `do` (tool calls run every time) or `queue` (a list of calls; each firing runs the next entry, so you write your priorities once; entries may be lists of calls). They run inside hoi4_advance (also a background one) while it watches the telemetry; the game is paused around them unless pause=false. An empty queue or a failed call stops the run with standing_queue_empty / standing_failed and says why. set with append=true adds queue entries to an existing rule. Orders execute decisions; they choose nothing.

_lock: none_

| argument | type | required | default | description |
|---|---|---|---|---|
| `action` | list \| set \| remove \| clear |  | "list" |  |
| `name` | string |  |  |  |
| `when` | array of string |  |  | stop-condition codes |
| `do` | array of object |  |  | calls run every time the rule fires |
| `queue` | array of any |  |  | one entry (a call or a list of calls) per firing |
| `append` | boolean |  | false | set: add `queue` entries to the rule |
| `pause` | boolean |  | true | pause the game while the rule runs |
| `max_fires` | integer |  |  |  |
| `on_fail` | stop \| skip |  | "stop" | queue rules: stop the run on a failed call (default), or skip the entry (kept under failed) and try the next in the same firing |
| `min_gap_days` | integer |  | 0 |  |

- `hoi4_standing {"action": "set", "name": "research", "when": ["research_idle"], "queue": [{"tool": "hoi4_research", "args": {"action": "start", "tech": "basic_machine_tools"}}, {"tool": "hoi4_research", "args": {"action": "start", "tech": "construction1"}}]}`
- `hoi4_standing {"action": "list"}`
- `hoi4_standing {"action": "remove", "name": "research"}`
