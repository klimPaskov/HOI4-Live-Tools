# Linux port of the management tools

Goal: the Linux native build offers the same tools as macOS (about 120 management
tools) on HOI4 1.19.4.1. The tools live in `native/src/launch/macos/` for historical
reasons; they are platform-neutral except for two things, handled as below.

## 1. Input (done)

Tools drive the game only through `platform::game_input::GameInput`:

| Backend | Platform | Mechanism |
| --- | --- | --- |
| `MacKeys` | macOS | CGEvent, front-most window and occlusion checks |
| `X11Input` | Linux | XTEST on the owned X11 display; owned PID, window class, viewable, focused and active window, fixed client size |

Keys are platform-neutral `vk` codes (numerically the macOS ANSI codes);
`X11Input` maps them to keysyms. Wheel scrolling converts macOS pixel scrolls to one
notch per 120 pixels; this needs live calibration on Linux.

`MacClock::bind_with_input` builds the full tool server from any `GameInput`, so the
Linux server can host the same tools.

## 2. Memory layout (open)

The tools hard-code about 340 Mach-O image addresses (vtables, globals) and many
struct field offsets for the macOS 1.19.4.1 build (Mach-O UUID DFA875A5). The Linux
executable has different addresses, and libstdc++ types (`std::string` 32 bytes,
containers) shift some field offsets relative to libc++.

Plan:

1. `scripts/cross_platform_map.py` maps Mach-O items to the ELF: vtables by RTTI
   name, globals and functions by matched code, field offsets by matched instruction
   displacements. Validated on 1.19.3, where both platforms' values are known.
2. Every Mach-O constant in the tools moves into a per-build table selected by the
   process string ABI, as `game::Layout` already does for the core reads.
3. The Linux server registers a tool only when every table entry it uses is mapped
   for the running build. Unmapped tools stay unlisted, never guessed.

## 3. Live verification (blocked)

Needs the Linux HOI4 1.19.4.1 executable (and the `.so` files beside it). The game
data can come from the macOS install. Each tool is then verified live on Linux, as
on macOS (`docs/LIVE_VERIFICATION_1_19_4.md`).

## Launch rules

Unchanged: never start the game while Steam or steamcmd runs, no Steam variables,
private `HOME`, owned display, owned process identity.
