# Handoff: German campaign runs 2 and 3 (11 October 2026)

Stopped by the coordinator on 11 Oct 2026, 07:31, because the tooling is to be redesigned so that a small model can play in real time. No campaign goal was reached. Nothing here was staged. The game is closed, the lock is released, and `dlc_load.json`, `settings.txt` and `pdx_settings.txt` are byte-identical to the backups (sha256 checked).

## State of the machine and the footage

- Game closed (the user closed it; I did not relaunch it after the stop message). Last saves: `hoi4live_ger3_day0` (1 Jan 1936), `autosave` (1 Sep 1936 of run 3), `hoi4live_ger2_*` and `hoi4live_front` (run 2, 24 Mar 1939).
- Footage under `<videos>/hoi4_live_demo/raw/`: `GER_run1` (first attempt, May 1940, 1.5 GB), `GER_run2_aborted` (about 1 GB, 1936 to 24 Mar 1939, calls.jsonl and images), `GER` (run 3, 99 MB, 1 Jan 1936 to Aug 1936). `incidents.log` lists the incidents. The run 2 journal is `artifacts/journal/journal_GER_run2_aborted.jsonl`.
- Run 3 incident, recorded correctly: at about 07:26 hoi4.exe disappeared with no crash dump (no `crashes/` folder, no error line). The coordinator says the user closed the game, so this is not a crash. A real crash with a dump did happen in run 2 (06:55, `crashes/hoi4_20261011_065553`, access violation on a worker thread, with the Esc menu open for a save). Windows logged LiveKernelEvent 141/193 (GPU timeouts) at 06:55 and 07:12.
- Memory at 07:29: 7.9 GB free of 31.6 GB. Other sessions held about 9 GB (node 4.2 + 2.1 + 1.1 GB, ChatGPT 2.1 GB, two claude processes 1.3 GB, blender 0.5 GB). The game needs about 8 GB. Not proven to be the cause of anything.
- Recording: the AMD AMF encoder was replaced by x264 (`--encoder x264 --crf 28`) after the 06:55 crash. Unproven either way.

## What was built since 1.1.0 (commits e19deba, bf300ec; 324 tests pass)

- State ids everywhere: `mapview.go_to_id` (the Find view lists the state with exactly that id first, so no name OCR), `gamedata/mapdata.py` (province centroids, state anchors, victory points, state neighbours, air base and naval base positions, strategic regions with sea aim points, cached under `%LOCALAPPDATA%/hoi4_live/script_index/`), minimap calibration (`hoi4_map action=minimap`, residual under 1 px) and `hoi4_map target=sea:<region id>` or `point:x,y`.
- Air: `hoi4_navy_air air_mission` by base state and wing type, with a `plan` list. Wing rows are selected by pixel position; air base states come from the save (`strategic_air/air_base`). Tested on the May 1940 save: fighters assigned over Western Poland.
- Navy: `naval_mission region_id=`. Tested: the Kriegsmarine on patrol in the North Sea. Naval invasion and convoy escort were NOT built or tested.
- `hoi4_front` (situation, plan, order): armies, where they stand, enemy-held states beside them, enemy divisions per state, victory-point objectives, one suggested order per army, from a fresh save (`fresh=true`, about 30 s). `state/front.py` is pure and unit tested; checked against the May 1940 save and the Austrian war.
- `hoi4_orders`: `paint` (polyline with any plan tool), `move` (right click on a state's main province), offensive stroke check by picture change (the red-pixel check missed yellow and pink arrows), front line probe with owner check.
- `hoi4_army create_army zoom=province` (one state's divisions only), recruit fallback when OCR cannot read the line, construction and diplomacy by state id, click `modifiers` (see problems), standing orders defer on an open event window and on "no idle research slot", save menu fallback click, and `find_save` follows the screen date (it had picked the newest save of an older run, so research and capitals were read from a 1940 save in a 1937 game).
- Helpers for shell-driven play in `scripts/campaign_helpers/` (`q.py` filter client, `wait.sh`, `adv.sh`, `restart.sh`, standing order json, popup rules).
- NOT done: production templates for equipment upgrades, army group or theatre automation (the game has no AI toggle: checked `interface/theatreselector.gui`, `battleplantools.gui` and the localisation), naval invasion, live tests of `hoi4_front`.

## What failed (in order)

1. Run 2, Austria: the war goal on one state (48 PP, 240 days instead of 87 PP, 435 days) was the right trade, but all 30 divisions were massed in one stack and sent into Austria. Nothing held the home states and no army had a front line. Austria (8 divisions) walked into 14 German states. Germany reached 74 % surrender progress, factories fell from 52 civilian and 36 military to 27 and 15, and the stack in Vienna was cut off (division tooltip: "Reinforcement blocked: no supply from capital", organisation 23 of 53, attrition +13 %, 280 infantry equipment missing). After 20 months the war was still on. The run was abandoned, not reloaded.
2. Run 2 crash at 06:55 (see above). The reload cost 4 minutes and 3 weeks of game time.
3. Run 3 restarted on 1 Jan 1936 with fixed procedures and reached about 1 Sep 1936 in roughly 25 minutes of wall time including setup, then the game was closed.

## Slowest steps (calls.jsonl, run 2: 729 calls in 2.41 h)

- `hoi4_clock join`: 52 calls, 2294 s (44 s average; this is the game simulating).
- `hoi4_focus select`: 85 calls, 704 s, 43 failed (queue entries not yet available, or blocked by an open event).
- `hoi4_research start`: 75 calls, 468 s, 23 failed (techs not available, off-screen tiles, busy slot).
- `hoi4_front situation fresh` 22 s per call (the save is about 20 s). `hoi4_production fill` 26-40 s. `construction plan` 34 s. `hoi4_save` 23 s. `hoi4_orders front` 22 s. `orders batch` 82 s for two armies. `politics hire` 21 s.
- Run 3: 109 calls in 0.39 h; clock joins 742 s over 14 calls; launch 71 s; new game 54 s.
- Pace in peace: about 50 game days per 3-4 minutes of wall time with the clock in the background and standing orders.

## Worst usability problems (what a small model will hit first)

1. The clock stops every few days (`pp_high`, `popup`, `standing_queue_empty`, `focus_done`, `construction_done`). In Jan-Apr 1936 runs of 300 days were cut to 12-57 days each time, and every stop is a model turn. The server should own a policy table (popup rules, queue refill, PP spending) and stop only for real decisions.
2. Unit assignment. Selecting a subset of divisions did not work: Ctrl+click and Shift+click on the unit list did nothing, although the tooltip says "Ctrl+Click to toggle selection". `hoi4_click modifiers` presses the key 0.35 s before the click and still fails, so the key probably needs another input path. Armies cannot be split once they stand in one province; they must be formed per state at the start. A plan with several offensive arrows gives the divisions to the first arrow only ("No divisions" on the others), and a unit with a manual move order keeps `player_order` and ignored the plan.
3. Verification by pixels is wrong in both directions: "no front line appeared" while the save showed the line, "no offensive line" while the arrow was on the map. The truth is in a save (30 s) or in the Theatre panel row tooltips, which already say "Very low supplies", "Enemy has Air Superiority", "No offensive combat".
4. Shortages are invisible until too late. The division tooltip lists missing equipment and "no supply from capital"; the telemetry does not. The briefing should report equipment on hand versus needed per type, supply state per army and fuel.
5. Coordinates and OCR: names with diacritics, the Esc menu label (twice OCR missed "Save Game" in the darkened menu and the menu stayed open; the next stray click hit the menu), tech tiles off screen, the recruit panel line list unreadable with 10 queued divisions. Every id based solution (Find by state id, minimap) was faster and exact; the remaining OCR spots should get the same treatment.
6. Standing orders burn queue entries on any failure (open event, no free slot). Two cases are fixed; the design is still fragile. A declarative desired state (research preference list, focus order, build ratios) that the server re-evaluates would be safer than a consumed queue.
7. Ahead-of-time technologies show costs of 700 to 1000 days and are listed as researchable; the model must filter them.
8. Peace conference, event and notice windows need a decision policy declared once.

## Top recommendations

1. Make the server own the routine (research, focus, production balance, construction, advisors, laws, popups) with preferences declared once and a briefing that lists only what needs a decision. Target: one model turn per game month in peace.
2. Make war a data problem. `hoi4_front` already reads armies, enemy strength and objectives. Add the other half: one army per region from the start, each with a front line (probe plus tooltip), one offensive arrow, re-issued on a schedule; refuse to declare war until every home region has an army with a front line and every army has supply and equipment.
3. Find a working way to select a subset of divisions (virtual-key SendInput, raw input, PostMessage, double-click "select all of the same template", dragging from the army card), or avoid the need by deploying new divisions per state.
4. Read state from the game instead of pictures: extend the telemetry mod (per-army supply, equipment shortage per type, enemy divisions per front state) and use the Theatre panel tooltips.
5. Use a save diff for verification only where nothing else works; make saves cheap or avoid them through telemetry.
6. Keep recording on x264 and keep memory headroom (other sessions use about 9 GB).
