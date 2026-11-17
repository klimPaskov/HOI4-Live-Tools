# One command to a playing agent

`hoi4-live play <TAG> "<goal>"` takes a fresh install to an agent playing a campaign on the native runtime. This
page is the design: what the command does, what it needs from the game, who plays, what runs in the background,
what it never touches, and what each system can do today. It lands in steps: the runtime's attach mode first,
then the supervisor, the `hoi4-live-play` proxy and the command.

## The flow

```
you                         hoi4-live play ITA "conquer Ethiopia"           your coding client
---                         ---------------------------------               ------------------
                            1. doctor for the native runtime
                            2. plan: the shipped reviewed plan (or --plan)
                            3. run folder: plan, brief, prompt
start HOI4 from Steam  -->  4. waits for the game (or --launch asks Steam)
start Italy 1936, paused    5. attaches to that game (no restart, no menus)
                            6. starts the supervisor in the background:
                               native runtime, input lease, the plan's names
                               checked against the live game, executor on
                            7. prints one instruction                  -->  "Read <run>/BRIEF.txt and follow it"
                                                                            plays through the hoi4-live-play tools
                            supervisor: duplicate guard, idle guard,
                            stops when the game closes, writes report.md
```

## 1. Attach, do not launch

A public user starts the game the normal way: Steam, their own profile, their own mods. The command attaches to
that game; it does not start, restart or configure it.

- **Finding the game.** The game's executable comes from the install's config (the Steam library, or
  `hoi4-live game-dir`). `play` lists the processes of the current user that run exactly that executable. None:
  it prints what to do and waits (default 15 minutes). More than one: it stops and names them.
- **Attaching.** `hoi4-live-native --attach PID RUN` checks the process (same user, not root, the supported build
  by hash and Mach-O UUID) and writes the run's session file, marked `attached`. An attached session is held by
  the game's identity alone (process ID, start time, executable, user); every read checks it again, so a reused
  process ID is never read. The owned launch of the developer rig (a game that is the supervisor's child) stays as
  it was.
- **Launching through Steam** is opt-in: `--launch` hands `steam://rungameid/394360` to the system (Steam starts
  the game as it always does). Nothing launches the game binary directly, and nothing is launched without the flag.
- **No menus.** Menu navigation by screen position is not part of the product. The user starts or loads the
  campaign; `play` waits (up to 30 minutes) until the runtime reads a loaded campaign of the right country, then
  hands over. While it waits it prints each new reason with its step: still in the menus, another country loaded
  (the runtime refuses it with `invalid_input_binding`), the wrong window mode, the game behind another window
  (it brings the game forward itself).

What the native runtime needs from the game, and how it is checked without fixed screen coordinates:

| Need | Why | Checked by |
| --- | --- | --- |
| Windowed, 1920x1080, UI scale 1.0 | the only reviewed input profile; GUI elements are located from the game's memory, then mapped into that window | the doctor reads `pdx_settings.txt` and `settings.txt` before the game starts; the runtime reads the game's own GUI and window geometry from memory and refuses input with `input_geometry_unverified` when it differs |
| A screen that holds the window (macOS: at least 1920x1108 points) | the window's content must not be shrunk by the system | the doctor reads the main display's size; the step is System Settings > Displays > "More Space" (never changed by the tool) |
| Game window shown and focused while the agent acts | input goes to the frontmost window only | the runtime checks before every input; the supervisor reports `input_target_not_frontmost` |

The tool never edits the game's settings to get there; the doctor prints the in-game step (Options > Graphics).

## 2. Who plays

The product ships the MCP server, the executor and the supervisor. The player is the user's own agent in their
coding client. After `play` the agent has, in one step:

- **the plan** in the run folder: a shipped reviewed plan when one matches the country and mode, or one the user
  generated with the strategist (`--plan FOLDER`). The strategist is an explicit step of its own, not hidden inside
  `play`: it runs a strong model for a long time (tens of minutes, a sizeable model bill) and must be started on
  purpose. `play` prints that note when no shipped plan matches.
- **the brief** (`BRIEF.txt`), rendered from the playbook's player template with the plan's notes and the MCP tool
  names the agent has (no shell commands);
- **the plan checked against the live game** once, before the executor starts (reads only): a template,
  general, air wing or production name that differs from the game's only in spelling is patched in `plan.json`
  (the original kept as `plan.pre-reconcile.json`), wings named "<variant> at <state>" become the live wings,
  and what the game does not have, or a first research block not researchable yet, reaches the player as a
  note; `reconcile.json` holds the result. A failed read leaves the plan as written;
- **the executor switched on** by the supervisor once it holds the input lease;
- **one instruction** to paste: `Read <run>/BRIEF.txt and follow it word for word.` It is also in `prompt.txt`.

The client reaches the game through `hoi4-live-play`, an MCP server the installer registers next to `hoi4-live`.
It is a small proxy: it lists the native runtime's tools (from the runtime's own catalogue, so the list exists
before any game runs), plus `hoi4_batch` (several calls in one turn) and `hoi4_run` (status, stop), and forwards
every call to the supervisor of the active run. Without an active run, calls answer with the command to start one.

## 3. Supervision lives in the product

The supervisor (`python -m hoi4_live.play.supervisor`) is started by `play` and runs in the background for the
whole campaign. It does what the developer rig's broker, plan loop and monitor did:

- owns the one native runtime process and its input lease (acquired once, renewed every five minutes);
- injects the lease token and a request id where a tool's schema takes them, so the agent never sees either;
- refuses an identical action that succeeded in the last 45 seconds and answers with the earlier result
  (`already_done`), so a repeated turn costs nothing and undoes nothing;
- **never pauses on its own**: a real campaign keeps running while the agent plays, through slow model turns and
  short silences. The game is paused only when the player or the user asks for it, or by the **idle guard** below;
- **idle guard** (a last resort for a player that has died): with no player call at all for half the idle pause
  the supervisor raises a `wake_player` change (reason `idle`) and records it in `status.json` and
  `supervisor.log`; the player gets it in the feed of its next `hoi4_observe` or `hoi4_wait_change` reply. With
  no call for the whole idle pause it turns the executor off and pauses a running game, recording when, the game
  date and why. The next player call lifts that pause: the executor goes back on and the game runs again (unless
  that call is itself a pause). Any player call resets the timer. A game that was already paused is left as it
  is, and a pause the player or the user asked for is never lifted by the supervisor;
- the idle pause is `idle_pause_minutes` in the run's `settings.json`, 15 by default. `hoi4-live play
  --idle-pause MINUTES` sets it for a run, `--idle-pause 0` switches it off, and an edit of `settings.json`
  during the run takes effect at the next check (every five seconds);
- passes supervisor notes (idle wake, idle pause and its lift, executor stopped, game closed) on the player's
  next reply;
- **stops cleanly** when the game closes or the runtime loses it: executor off, lease released, report written,
  the active-run pointer removed. `hoi4-live stop` and `hoi4_run {"op":"stop"}` do the same and leave the game
  running;
- **ends the run** when the executor reports the plan's goal reached, or once the game date passes the plan's
  `goal.deadline` (reasons `goal_reached`, `deadline`); `HOI4_LIVE_END=off` keeps the run going past both;
- **ends the run at the user's end date**: `hoi4-live play ... --until 1938.01.01` (reason `end_date`);
- **ends the run when the player is gone**: with no player call for `player_silent_minutes` (45 by default, in
  the run's `settings.json`; 0 switches it off), for example after the model's usage limit or a closed client,
  the run ends as `player_silent` instead of leaving the executor to play on alone for hours;
- **watches for a system prompt** (a permission or password dialog: owners `UserNotificationCenter`,
  `CoreServicesUIAgent`, `SecurityAgent`, read from the window list with no permission). The runtime refuses input
  while one is on screen; the supervisor tells the player and the status at once, never touches the prompt, and
  ends the run as `system_dialog` when it is still there after five minutes;
- brings the game to the front once just before the executor starts (an order sent while another window is in
  front is refused);
- notes the machine's load and swap at the start (a machine short of memory plays at half its pace or less)
  and **ends the run with a plain message** when the game's memory passes `HOI4_LIVE_GAME_MEMORY_GB` (default
  10) or its date stands still for `HOI4_LIVE_STALL_MINUTES` (default 5) while it is not paused (reasons
  `game_memory`, `game_stalled`; 0 switches either off);
- writes `watch.jsonl` once a minute (game date, war stats, each army's supply and organisation; every five
  minutes also the production lines, stockpile and factories) and `feed.jsonl` (every change of the runtime's
  feed: alerts, wakes, lost land, the executor's orders); both read only, for `hoi4-live report`;
- **at the end of the run** keeps what the gate reads (`gate/<date>.json`: factories, armies, equipment,
  templates, air, navy, alerts); at its own ends (goal, deadline, end date, silence, memory limit) pauses the game;
  then saves it with the game's own save (`hoi4_save`), so the campaign can continue in a new segment. No save is
  tried when input cannot work (the game closed or hung, a system prompt). `HOI4_LIVE_SAVE_AT_END=off` skips it;
- writes `calls.jsonl` (token-free), `supervisor.log` and, at the end, `report.md`: wall time, game dates, calls,
  action receipts by outcome, duplicates refused, idle wakes, idle pauses (each with its time, game date, reason
  and when it was lifted), pauses the player asked for, self-pauses the runtime recorded, why it ended, and the
  user-file check below.

### Every pause in the flow

| Pause | When | Kept because |
| --- | --- | --- |
| The player's `hoi4_time {"paused":true}` | the player asks | an explicit request; counted in the report |
| The user's own pause in the game | the user asks | the user's; nothing undoes it |
| Idle guard | no player call for `idle_pause_minutes` (15; 0 = off) | the only pause the product makes itself; lifted by the next call |
| The plan's end date | never in a `hoi4-live play` run: `play` and `--continue` remove a plan's `stop_on_date` (a strategist's review date) and log it in the plan's `_meta.edits` | a real campaign never pauses itself |
| The run's end | the supervisor's own ends (goal, deadline, `--until`, silent player, memory limit): paused, then saved | nobody plays after the run; the save is where the next segment starts |
| Pause guard in the runtime (`pause_guard`) | an Escape or Space of an action changed the pause state | it puts back the state the action started with, running or paused; it never adds a pause |

The user starts the campaign paused and the player's opening runs while paused; the player unpauses once its
armies are placed, and from then on it never pauses. The Python server's step tools (`hoi4_advance`,
`hoi4_time {"action":"step"}`) run the clock for the days they are asked for and stop paused: that is their request,
and the native play flow does not use them.

## 3b. Segments and the report

A campaign of several game years cannot be one game process on a 16 GB machine: the game's memory grows. It is
played in segments. A segment ends at the memory limit (or at `--until`, or when the player is gone) with the game
paused and saved by the game's own save. The user quits the game, starts it again and loads that save (the
launcher's Continue loads the newest save), then runs `hoi4-live play --continue`. That makes a new run folder
and carries over the plan in force (`plan.json`, with the names reconciled on day one and any revision),
the executor's memory (`plan_state.json`: done work, timers, the failure ledger, the phase), the plan notes, the
player's log and the run's settings. The supervisor refuses a loaded game older than where the last segment
ended (more than 45 days: a save from before the boundary would replay turns). A continued segment is never a
reload to get a better result.

`hoi4-live report [RUN]` reads a run, live or after it ended: the campaign's segments (dates, why each ended, the
save it wrote), the bar audit from the run's files (`play/audit.py`: self-pauses, repeated failures, research
idle, political power, armies, actions by domain, lost land, alerts open with or without a written reason, pace,
production, per-state interface work) and the gate (`play/gate.py`: factories and divisions against the game's
own AI where a baseline ships, or against `--marks CIV MIL DIVS`; equipment fill; force composition against the
plan's force table; air). On a live run the gate is read through the supervisor (`snapshot`, reads only, not a
player call); after the run from `gate/<date>.json`.

## 4. The user's files

`play` changes no file outside its run folder except the save it writes at the end of a run with the game's own
save (named by the game: country and date; the report names it). The game's settings, saves, mods and playset are
the user's:

- the steps for display mode, resolution and UI scale are printed for the user to do in the game;
- before attaching, `play` records the SHA-256 of `settings.txt`, `pdx_settings.txt`, `dlc_load.json` and the
  save-game folder listing, and keeps a private copy of the three settings files in the run folder;
- the report compares them at the end and names any file that changed during the run (the game itself writes
  settings when the user changes an option), with the copy to restore from. Nothing is restored behind the
  user's back, because a change made in the game's own menu is the user's.

## 5. Systems

The flow has the same shape everywhere: doctor, plan, run folder, game, attach, supervisor, one instruction.
What each piece needs per system:

| Piece | macOS (Apple silicon) | Linux x86_64 | Windows |
| --- | --- | --- | --- |
| Find the user's game | yes (`ps`) | yes (`ps`) | yes (`tasklist`) |
| Attach (`--attach`) | yes, HOI4 1.19.4.1 | not yet: the runtime holds only games its owned launcher started under its own X display | no native runtime yet |
| Input into the user's window | yes, windowed 1920x1080 | not yet: input is bound to the owned X display | no native runtime yet |
| Supervisor and `hoi4-live-play` proxy | yes | yes (stdlib, same code) | yes (named pipe; same code) |
| `play` today | attaches and hands over | prints what is missing | hands the campaign to the Python server |

The doctor ends with one line saying what `hoi4-live play` does on the system it runs on; the README has the
same table.

## Live checks

Everything above is unit-tested behind seams (process listing, the native runtime as a fake JSON-RPC child, the
clock, the endpoint). What only a running game can prove is listed and scripted in
`scripts/live_checks/play_attach.py` (run by the owner of a live test machine, never in CI).
`scripts/live_checks/product_campaign.py` checks a campaign end to end through the product: a clean install,
the doctor, a run to an end date, the report, a forced segment boundary and `--continue`, and the silent player.
