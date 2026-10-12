# Changelog

## Unreleased native preview

- Start a separate Rust stdio runtime with owned Linux process reads, bounded
  focus/research/clock snapshots, explicit capability errors and server waits.
- Verify selected upstream ordinary actions with one Luna medium player; record
  the final pause timeout and quarantine. No clean campaign acceptance is claimed.
- Measure Rust and guarded Python over the same persistent SSH path: 7.5 ms
  versus 294.9 ms median for 20 live overview samples each.
- Retain Clausewitz-MCP MIT provenance; add Rust tests and a three-OS CI matrix.
- Simplify README and preserve the existing Windows reference in a separate doc.
  Native commands, governor and Windows/macOS native adapters remain unfinished.

The Python release version and downstream release pins remain unchanged.

## 1.1.0 - 2026-10-11

Speed and long campaigns. 43 tools (new: `hoi4_clock`, `hoi4_standing`, `hoi4_peace`).

- Fast clock: `hoi4_advance` follows the telemetry's day records instead of pausing every two days and reading the date
  by OCR (a game day in 0.7 s instead of 3.3 s); `background=true` keeps the game running while the agent acts and
  `hoi4_clock` joins or stops the run. The clock stops by itself for the peace conference and answers notices.
- Standing orders (`hoi4_standing`): research, focus, construction and production queues the clock executes when their
  situation comes up (`research_idle`, `focus_done`, `focus_idle`, `construction_done`, `production_idle` ...).
- Telemetry schema 3: known and researched technologies, per-state factory levels and free building slots.
- Macros: `hoi4_construction plan`, `hoi4_production fill`, `hoi4_orders attack` and `batch`, `hoi4_peace`,
  `hoi4_diplomacy` `states` and `via`, `hoi4_focus` `if_idle` and `search`.
- Window guard before every campaign action; the clock state is restored after an action that left the game running.
- `hoi4_lock`: `record` (one JSON line per tool call with a time breakdown), `set_pace`, `set_images`.
- `hoi4_orders front/offensive/spearhead` leave an executing army alone unless `replace=true` (F1);
  `hoi4_status.game_mode` and refusal of console tools and manual saves in Ironman and multiplayer games (F2).
- Fixes: focus names with script placeholders, requirement groups and icons, the disabled Start button, NSB armour
  tiles, split panel titles, new army panels, the division designer's battalion lists, Find view OCR slips.
- Scripts: `scripts/bridge.py` (a persistent server for shell agents), `scripts/record_screen.py` (segmented screen
  recording with the GPU encoder).

## 1.0.0 - 2026-10-10

First release. 40 tools, live-verified on Hearts of Iron IV 1.19.2 with a large mod.

- Session and screen: launch, load, save and quit through the game's own screens; screenshots, OCR, tooltips,
  popups, input inside the game window under a shared desktop lock.
- Test mode: console, `eval_effect`, events, error and game log deltas, plain-text save queries and diffs,
  scenario reports, optional telemetry companion mod.
  Error-log lines the Paradox wiki explains (event timers, missing namespaces, duplicate event ids) carry
  `known_mistake` with the documented fix.
- Understanding: one state model of a country (telemetry, newest save, top bar) and a one-page briefing.
- Campaign mode: focus, research, production, construction, laws and advisors, decisions, diplomacy and trade,
  armies, battle plans, fleets and air wings, all verified on screen after acting.
- Campaign loop: `hoi4_advance` (run time until something needs a decision, answer events by policy) and
  `hoi4_journal`.
- Installer (`install.py`): private Python environment, launcher that needs no hard-coded Python, per-user
  registration for Claude Code, Codex, Cursor and OpenCode, upgrade in place, clean uninstall.
- Release archive builder and checklist (`scripts/build_release.py`, `docs/RELEASING.md`).

Known limits: see `docs/STATUS.md` (the phase 4 exit check was not repeated end to end on the final code; the
two-year campaign runs of the plan were not made).
