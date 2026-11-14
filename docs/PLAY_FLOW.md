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
                               native runtime, input lease, executor on
                            7. prints one instruction                  -->  "Read <run>/BRIEF.txt and follow it"
                                                                            plays through the hoi4-live-play tools
                            supervisor: duplicate guard, watchdog,
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
- **watchdog**: no call from the player for five minutes pauses a running game and turns the executor off (the
  only pause the product ever makes; the player itself never pauses after the first unpause). The next player
  call turns the executor back on; the player unpauses as its brief says;
- passes supervisor notes (watchdog pause, executor stopped, game closed) on the player's next reply;
- **stops cleanly** when the game closes or the runtime loses it: executor off, lease released, report written,
  the active-run pointer removed. `hoi4-live stop` and `hoi4_run {"op":"stop"}` do the same and leave the game
  running;
- writes `calls.jsonl` (token-free), `supervisor.log` and, at the end, `report.md`: wall time, game dates, calls,
  action receipts by outcome, duplicates refused, watchdog pauses, self-pauses the runtime recorded, why it ended,
  and the user-file check below.

## 4. The user's files

`play` changes no file outside its run folder. The game's settings, saves, mods and playset are the user's:

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
