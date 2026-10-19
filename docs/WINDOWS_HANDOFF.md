# Windows handoff: port the native macOS campaign server

Audience: the Windows maintainer (or agent) who brings the native Rust MCP
server to Windows. Everything below was learned while the macOS adapter was
built and used to play recorded FAIR Germany campaigns at 1920x1080. Nothing
here has been run on Windows. Treat every offset, key code and constant as
build-specific, and re-verify it on the Windows build you target.

The macOS and Linux adapters are the reference. Read
[LINUX_RUST_HANDOFF.md](LINUX_RUST_HANDOFF.md) for the architecture and the
rules that stay in force: Normal difficulty, vanilla rules, historical AI,
FAIR by default, observation-only telemetry, no console, debug, eval,
`fire_event`, save editing, hidden enemy information or reload-to-undo; stdout
is protocol JSON only; never take over or kill a game the user did not start
for this run. The shared Windows game is reserved for mod testing and must not
be controlled by campaign automation: use an owned, separately launched
instance with its own run directory.

## What exists and what does not

`native/` is a stdio MCP server (`hoi4-live-native`). Portable code reads the
game through the `platform::Process` trait and describes each build with a
`Layout` table chosen at run time. Adapters:

| Part | Linux | macOS | Windows |
|------|-------|-------|---------|
| process attach, bounded reads | `platform/linux` (`/proc`) | `platform/macos.rs` (task port) | **missing** |
| input | X11/SDL | `platform/macos_input.rs` (CGEvent) | **missing** |
| launch / close owned game | `launch/linux` | private broker scripts | **missing** |
| per-build layout | ELF a4971c9e | Mach-O 089EA519 | **missing** |
| string ABI | libstdc++ | libc++ | **missing (MSVC STL)** |

Not yet built on any platform: the peace conference window, naval and
invasion actions, a reliable state-slot read. The macOS tools themselves live in
`native/src/launch/macos.rs` (schemas, dispatch) and `native/src/launch/macos/*.rs`
(one module per area: armies, air, diplomacy, events, deployment, decisions,
politics, production, construction, focus, research, states, ui, journal). The
Windows port should reuse them: they are written against the live GUI tree and
a `MacKeys`-style input object, so most of the work is the adapters and the
layout tables, not the tool logic.

## Port plan, in order

1. **Process adapter** (`platform/windows.rs`, behind `cfg(windows)`).
   `OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_VM_READ)`,
   `ReadProcessMemory`, `NtQueryInformationProcess`/`QueryFullProcessImageName`.
   Ownership is the PID **plus** creation time (`GetProcessTimes`) **plus**
   executable path and SHA-256, checked on every call; a PID alone is never
   enough. Never request write or inject rights.
2. **String ABI.** Add `StringAbi::Msvc` (16-byte small-string buffer, size at
   +0x10, capacity at +0x18) next to `LibStdCxx` and `LibCxx`. Every reader that
   decodes a string already goes through the ABI, so unknown ABIs refuse.
3. **Build identity.** Identify the executable by PE `TimeDateStamp`,
   `SizeOfImage` and a SHA-256 of the file; map it to a `Layout`. Unknown builds
   and stale or torn reads must fail explicitly. Do not copy offsets from
   Linux or macOS.
4. **Layout discovery.** The macOS offsets were mapped live from RTTI vtables,
   sole globals (the idler pointer is the root of everything), value changes
   across actions (unpause, speed, player switches) and the back-references of
   objects. On MSVC the RTTI type descriptors carry `.?AV...@@` names; walk
   `RTTICompleteObjectLocator` to name a vtable. Start with the game clock and
   own country (`hoi4_overview`), then research, focus, economy, production,
   construction, and only then the GUI tree.
5. **GUI tree.** Widget geometry is read from live memory, not guessed from
   pixels: a widget base holds its GUI type (name) at +0x50 on macOS, containers
   list children, visibility and flags are bits, button state is 0 normal, 2
   toggled, 3 disabled, and the rectangle comes from the renderer sprite's
   translation (centre-origin, y up) plus the sprite type's size. Re-map each
   of these on Windows (`gui_tree.rs` `Layout`) and check against a screenshot.
   Some widgets draw a sprite whose rectangle is scaled oddly (a leader row's
   Promote button, event options): derive their position from a sibling and the
   `.gui` file offsets instead, and say so in a comment.
6. **Input adapter** (`platform/windows_input.rs`). `SendInput` with absolute
   mouse coordinates mapped to the game's client area (check
   `GetClientRect`/`ClientToScreen`, DPI awareness and the window's monitor),
   real key events (scan codes), a real Shift for range selection, right click
   and right drag (offensive lines), and scroll wheel. Input must go only to
   the owned window: verify `GetForegroundWindow` belongs to the owned PID
   before every event and refuse otherwise. Do not use `PostMessage` shortcuts
   that the game would not see as player input.
7. **Launch and close of the owned game.** Launch through Steam with a
   per-run settings backup and a private run directory (see
   [OWNED_LINUX_LAUNCHER.md](OWNED_LINUX_LAUNCHER.md) for the contract), keep
   the display at 1920x1080 windowed, and restore settings only after the
   owned process exits. A Steam sign-in dialog is the one blocker that needs a
   human.
8. **Recording.** ffmpeg `gdigrab` of the game window (or Windows Graphics
   Capture) at 1920x1080 into 30-minute segments; keep recordings outside Git.
9. **CI.** Add a Windows job beside the macOS and Linux ones (cargo fmt, clippy
   with `-D warnings`, unit tests with fake processes), and keep portable code
   free of Windows imports at module scope.

## Lessons from the macOS runs (these will repeat on Windows)

- **Hotkeys depend on focus.** Escape on a free map opens the in-game menu, so
  every dismissal waits for the previous Escape to take effect. A stuck modifier
  turns letters into other shortcuts (on macOS a stuck Fn made `Q` open a quick
  note); release modifiers before and verify.
- **Open panels cover clicks.** Before any map click close: the game menu, the
  state view, the construction panel (it turns clicks into placements), the
  strategic air region window (it sits over the screen centre), leftover
  confirmation dialogs, news notices and event windows. Events block all other
  input; answer them first.
- **Confirmation dialogs are separate windows** found in the game's popup list
  by vtable: accept dialogs (laws, advisors, promotions), reassign-commander
  and "Reassign Armies". Declining the last one when creating an army avoids
  folding other armies in.
- **Armies.** The cap is 24 divisions per army; a field marshal commands up to 5
  armies; only **field marshals** can lead army groups (promote a general first,
  it costs command power and asks for confirmation). A training line without a
  deployment location never deploys. Creating an army in a state where another
  army stands merges them unless the Reassign dialog is declined.
- **War order.** Front line first (Z), offensive line second (X, right-button
  drag), plan execution last. Planning builds about two weeks before attack.
  Declaring war on Poland pulls in France and Britain.
- **Anschluss (1.19, Götterdämmerung).** The properly equipped army must exceed
  Austria's by a factor that falls weekly, so equipment, not division count, is
  the gate.
- **Verification rules.** Every action records intent in the journal, runs one
  input burst, then verifies from memory and returns a receipt; an unresolved
  dispatch is only settled by retrying its own request id; refusals raised
  before input settle as not applied.
- **Paused game.** Some readbacks (political power after a decision) update
  only when the clock runs; an action can look failed while the game is paused.
  Check with a read after unpausing before retrying.

## Acceptance for Windows

A clean two-year run played by one Haiku 5.5 (medium) player on the owned
instance at 1920x1080, then the recorded FAIR campaigns in
[LINUX_RUST_HANDOFF.md](LINUX_RUST_HANDOFF.md). Log real failures. Offline tests
are not live evidence. Keep recordings, `calls.jsonl`, journals, dumps, saves,
credentials and private paths outside Git; audit the full reachable history of
every pushed branch; never push `private/*`, `--all`, `--mirror` or arbitrary
tags. Downstream pins for HOI4 Mod Setup and Agentic HOI4 Modding move only
after a release is accepted.

## Standing plans

`hoi4_plan` and `hoi4_plan_run` (see `native/src/plan_run.rs`) are portable:
they only call the server's own tools. Once the Windows input tools exist they
work unchanged. The run directory's `plan.json` holds research priorities, the
focus path, construction jobs, training, production priorities, politics picks
and decisions; `hoi4_plan_run` executes them under the caller's input lease and
returns when a choice event needs the model or a peace conference opens.
