# Play a campaign with an AI agent

This page is for players. It walks through one campaign from the first command to the report: what to type, what
you will see, how long each part takes, how to carry a long campaign over several sittings, and what to do when
something stops. Everything here is what the `hoi4-live` command does today.

## What you need

- A Mac with Apple silicon (M1 or later). This is the only system where a campaign is played end to end today:
  Linux and Windows have runtimes in the install, but neither has played a campaign on the current game version.
- Hearts of Iron IV from Steam, version 1.19.4.1. `hoi4-live doctor` tells you if your version is not the one the
  runtime reads.
- A coding client with a model that can use tools, for example Claude Code. The agent in your client is the
  player; it follows a written plan and the program does the routine work.
- About 16 GB of memory. A machine that is short of memory plays at half the speed or less, and the report says so.

## 1. Install and check

```sh
curl -fsSL https://github.com/klimPaskov/HOI4-Live-Tools/releases/latest/download/install.sh | sh
hoi4-live doctor
```

The doctor lists each thing that is not in place with the one step that fixes it, for example allowing memory
access or Accessibility for your terminal. Do the steps and run `hoi4-live doctor` again until it has nothing left.
Restart your coding client once after installing, so it loads the `hoi4-live-play` tools.

## 2. Start the game

1. Start Hearts of Iron IV from Steam as you always do (or add `--launch` to the play command and it asks Steam).
2. In Options > Graphics choose Windowed, 1920x1080, UI scaling 1.0. On a laptop screen choose "More Space" in
   System Settings > Displays if the doctor asks for it.
3. Start a new campaign as your country (Normal difficulty, historical AI, no console) and leave it paused on the
   first day.

## 3. Play

```
hoi4-live play ITA "conquer Ethiopia before 1937"
```

The country is a tag or a name. Reviewed plans ship for Italy, Ethiopia, Germany and China (FAIR rules) and Poland
(META rules). You see something like:

```
Campaign: Italy (ITA), FAIR, 1936.1.1 to 1937.1.1
Goal: conquer Ethiopia before 1937
Plan: shipped reviewed plan italy-ethiopia-1936
Checks: the native runtime is ready
Run folder: .../hoi4-live-tools/runs/20261118-101500-ITA
Game found (process 4242).
Attached. Waiting for your campaign as Italy (start or load it; the run begins when the game shows it)...

The run is live: Italy on 1936.01.01.12, the plan's executor is on, the supervisor watches the run.
...
    Read .../runs/20261118-101500-ITA/BRIEF.txt and follow it word for word.
```

Give that last line to your coding client. The agent reads its brief, does the opening while the game is paused,
then unpauses at speed 3 and plays: speed 3 while it builds in peace, speed 2 while it manages a war (it sets this
when a war begins and ends; a speed you choose yourself is kept). The game keeps running from then on: the agent
never pauses it to think. You
can watch, or leave the computer alone. Do not click inside the game while the agent plays; it moves the mouse.

Useful options:

- `--until 1938.01.01` ends the run at that game date, with the game paused and saved there.
- `--idle-pause MINUTES` (default 15): with no call from the agent for that long the game is paused until its
  next call. `0` switches it off.
- `--plan FOLDER` plays a plan the strategist wrote (see "Plans" below) instead of a shipped one.

## How long it takes and what it costs

- **A shipped plan needs no planning.** The run starts within a minute of the game showing your campaign.
- **A plan for any other country, start date or mod** is written by the strategist first: a separate step that
  reads the game files with a strong model for a long time, typically 20 to 60 minutes, and costs a model bill to
  match; a second model then reviews the plan. It runs from a copy of the repository
  (`skills/hoi4-strategist` explains it). Then: `hoi4-live play TAG "goal" --plan FOLDER`.
- **During the run** the player is one agent in your client; its usage is your client's. The program also keeps
  the plan up to date with small updates; the agent answers them, and the report lists how many there were and
  what they used.
- **Game time.** At speed 3 a free machine plays about 9 to 11 game days per minute: a game year takes roughly
  35 to 45 minutes of play. The report states the pace your machine reached.

## When the run ends

The run ends by itself, and the game is never closed for you. Why it ended is the first thing in the report:

| Ended | What happened | What you do |
| --- | --- | --- |
| `goal_reached` | the plan's goal was reached | nothing: read the report |
| `deadline` / `end_date` | the plan's deadline or your `--until` date came | continue if you want more (below) |
| `game_memory` | the game's memory grew past 10 GB: the end of a segment | quit the game, start it again, continue (below) |
| `player_silent` | the agent made no call for 45 minutes: a usage limit, or the client was closed | continue when your client works again |
| `stopped_by_player` / `stopped_by_user` | the agent called stop, or you ran `hoi4-live stop` | continue if you want more |
| `game_closed` / `game_stalled` | the game closed or stopped moving | start the game, load its newest save, continue |
| `system_dialog` | a macOS prompt stayed on screen for 5 minutes | answer the prompt, then continue |

At its own ends (goal, deadline, end date, a silent player, the memory limit) the program pauses the game and saves
it with the game's own save. The save is named by the game (country and date) and the report gives its name.

## Continue a long campaign

A campaign of several years is played in segments, because the game's memory grows. To continue:

1. If the run ended at the memory limit, quit the game (Esc > Quit > Quit to desktop) and start it again from
   Steam. Otherwise you can keep the running game.
2. Load the save the run wrote: the launcher's Continue loads the newest save, or use Load Game in the game.
   Leave it paused.
3. Run:

```
hoi4-live play --continue
```

It takes the last run that ended (or `hoi4-live play --continue RUN` for another one) and carries over the plan in
force and everything the program remembered: what it already did, what it waits for, what it gave up on. It prints
a new instruction for your coding client, as at the start. It refuses a save older than where the last segment
ended, because that would replay turns. You can add `--until` again for the new segment.

## Read the report

Each run writes `report.md` in its run folder: the goal, why it ended, the game dates and the pace, the agent's
calls, what every action did (verified, not applied), actions that failed more than once, pauses, the war at the
end, the save to continue from, and whether your own game files changed.

For the full picture, at any time during the run or after it:

```
hoi4-live report
```

It prints, and keeps in the run folder as `audit.txt`:

- **CAMPAIGN**: the country, the goal and each segment with its dates, why it ended and its save.
- **BAR AUDIT**: one line per thing a well-run campaign gets right. The numbered lines count pauses the program
  made itself (should be 0), failed and repeated actions (a repeat is a defect, not bad luck), research slots left
  idle, political power left unspent, armies short of equipment or supply, what was done in each area (battle, air,
  navy, espionage, doctrine, designer, construction, industrial organisations, decisions, trade) and land lost.
  "alerts open" lists the game's alerts that stayed open, for how many days, and whether the plan gives a reason;
  `[NO REASON]` marks one nobody handled. "pace" is game days per minute. The P lines check production, S1 how
  often states were opened one by one, M each management alert.
- **GATE**: where the country stands now: factories and divisions (against the game's own AI on the same date
  where a reference ships, today Germany 1936; or against marks you give with `--marks CIV MIL DIVS`), how well
  the divisions are equipped (a division count only counts when they are equipped), the force by type against the
  plan's force table, and the air force.

## When something stops

- **A macOS prompt appears** (a permission or password dialog). The game takes no input while it is on screen.
  Answer it yourself; nothing in the program touches it. The agent is told at once; after 5 minutes the run ends.
- **Your model's usage limit is reached.** The agent stops calling. After 15 minutes the game is paused; after 45 the
  run ends with the game saved. When the limit resets, load the save and run `hoi4-live play --continue`.
- **The game is not in front.** The program brings it to the front before it acts; if another window takes the
  front again, orders are refused until the game is in front. Click the game window, and keep other windows from
  opening on top of it.
- **The game closes or freezes.** The run ends by itself. Start the game, load its newest save (the game autosaves
  monthly) and continue.
- **The game is slow.** Close other programs: a machine short of memory plays at half its pace or less.
- **The run does not start.** `hoi4-live play` prints why (still in the menus, another country loaded, the window
  size) and waits; `hoi4-live doctor` names any step that is missing.

## Your files

The program never changes your game settings, mods or playset, and never edits a save. It writes one thing outside
its run folder: the save at the end of a run, through the game's own Save, under the name the game gives. The report
compares your settings files and the save list with the start of the run and names anything that changed.
