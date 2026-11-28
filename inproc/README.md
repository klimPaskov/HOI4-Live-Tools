# The in-process module

An optional macOS module that lets the MCP tools post the game's own UI commands instead of moving the
mouse. It is a dynamic library loaded into the HOI4 process at launch. It finds the game's command types
by scanning the game image in memory, and for each request it builds one command with the game's own
factory or UI constructor, runs the game's own "can do" check, and hands the command to the game's
`Post`, exactly as a click in the UI would. The module never writes game state: the game executes the
command and stamps its sender. The MCP tool keeps its name, schema and Verified read-back from memory.
Only the route changes, and the mouse path stays the fallback inside the same call.

## Status

- **macOS only.** Builds covered by the field table in `core/caps.c`: 1.19.4.1 (f584), 1.20.0.0 (0c4f)
  and 1.20.1.0 (724a). On another build the scan refuses the capabilities it cannot prove, and the
  families that need them stay off.
- **Windows and Linux:** there is no module. The server builds and runs on both. Its in-process
  transport (a Unix socket, `native/src/launch/macos/inproc.rs`) never connects on Windows, and finds no
  module on Linux, so every tool takes its mouse path. A call that only exists in process reports
  "in-process module not available on this OS yet".
- **Single player only.** Every post checks that the session's server is the single-player one and
  refuses otherwise. Multiplayer stays closed until a two-client test (sessions S1-MP and S2-MP in
  `docs/LIVE_VALIDATION_QUEUE.md`) has passed.
- **Not loaded by the published package.** Nothing in the released server or launcher injects the
  module. It is a maintainer opt-in (below).

## How it loads, and what stays off

1. Build it with Apple clang: `inproc/build.sh` writes `build/libhoi4live_inproc.dylib` (x86_64: the
   macOS game runs under Rosetta on Apple silicon), plus the offline scanner and test hosts. Build
   products are not committed.
2. The launcher starts the game with `DYLD_INSERT_LIBRARIES` set to that library. The game's files are
   never changed. In its constructor the module stays inert unless the host is the HOI4 executable and
   `HOI4_INPROC_LOG` names a log. It also removes `DYLD_INSERT_LIBRARIES` from the environment, so
   programs the game starts do not load it.
3. Reads (stage 0) start after a delay. The channel (stage 1+) is served only when the launcher passes
   `HOI4_INPROC_SOCK`, a socket path in the run folder. The socket is owner-only, every peer is checked
   to be the same user, and a fresh token per game process is written beside it (`<sock>.token`).
4. Posting is off unless the launcher also names the families in `HOI4_INPROC_POST` (comma-separated,
   from the allow-list in `core/ipc.c`). A refusal the module cannot explain switches posting off for the
   rest of the process. Kill switch: a file named `<log>.off` makes the module inert within about 2 s.

On the MCP side, `HOI4_INPROC_SOCK` points the server at the channel (without it the backend is off),
and `HOI4_BACKEND` picks the route per call:

| `HOI4_BACKEND` | route |
|---|---|
| unset or `auto` (the default) | in process only for a family that is opted in **and** marked live-passed in the module (`IPC_LIVE_PASSED` in `core/ipc.c`); everything else by mouse |
| `inprocess` | in process for every opted-in family the module resolved, proven live or not (live validation only) |
| `gui` | never in process |

So with nothing set, or on Windows and Linux, every tool runs exactly as it did without the module.

## Families (HOI4 1.20.1.0, 28 November 2026)

Live-passed, so used under `auto` once opted in: `set_game_speed`, production, air, airwing,
production2, orders, orders2, units, picks, decisions, diplomacy, holds, research, politics,
construction, mio and navy.

Not live-passed, so used only under `inprocess`:

- training: stopped live. The line count was not exact, and a refused add left a queue behind. The fix
  (lines posted as the remainder after each read, and a failed queue cancelled) waits for its re-run.
- events: stopped live. The answer executes, but the event window stays open.
- doctrine: S18-DOCTRINE has not passed. Doctrine picks take the mouse path on 1.20.1.0.
- projects: S19-PROJECTS has not been run.

Each family's session, steps and pass rule are in `docs/LIVE_VALIDATION_QUEUE.md` (S0 to S19).

## Limits

- One request at a time, rate-limited. Each request is one command per listed object, posted from the
  main thread, and only when the game's thread rule allows game-state access.
- Every object a request names must resolve through the command's own typed resolver and belong to the
  player before a command is made.
- Reads of game memory return an error instead of faulting. Game functions are called only at
  addresses whose code pattern the scan proved, on objects whose dynamic type was checked first.
- The log is bounded at 8 MiB per game process.

## Tests

- `inproc/test_offline.sh EXE_1_19_4_1 EXE_1_19_3 [EXE_1_20_0_0]` runs the scanner over the game's
  executable files (kept outside the repository) and checks it against hand-found values and the
  external runtime's tables. It also loads the module into a stand-in host that is not the game, and
  runs the channel and policy tests (`tools/ipc_test.py`, `tools/ipc_selftest.c`).
- `cargo test --lib inproc` covers the MCP side: the route switch, payloads and read-backs.
