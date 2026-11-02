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

## 2. Memory layout (code done, table pending)

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
3. The Linux server binds the tools only when a table for exactly the running build
   ships (`xlayout::require_linux`); an entry missing from it fails that read with
   `layout_unmapped`, never a guess.

Status (2 November 2026): every tool module reads through `xlayout`
(`img` for image addresses, `fld` with an owning `Class` for field offsets); the
GUI widget tree (`gui_tree`) translates its layout the same way. About 200 owner
classes are tagged, 73 with a macOS vtable (mapped automatically by RTTI and
matched code) and about 127 without one (records, strides and owners named from
code comments; these need an anchor function or a live check on Linux).

Known ABI-specific spots needing a live check: the special-projects breakthrough
map walks a libc++ tree node (libstdc++ nodes keep colour, parent, left, right,
then the value); `construction_place` reads SDL/Cocoa window internals and stays
macOS-only.

Core modules (`game`, `research`, `own_focus`, `production_lines`, `construction`,
`army`, `gui_registry`, `research_ui`) keep their own verified Linux 1.19.3 layouts.
They move to 1.19.4.1 by a same-platform migration (as macOS did from 1.19.3),
not through the cross-platform table.

## 3. Live verification (blocked)

Needs the Linux HOI4 1.19.4.1 executable (and the `.so` files beside it). The game
data can come from the macOS install. Each tool is then verified live on Linux, as
on macOS (`docs/LIVE_VERIFICATION_1_19_4.md`).

## Launch rules

Binding: `hoi4-live-native --bind-management RUN_DIRECTORY BINDING` with
`{"window": <X11 id>, "window_class": "<exact WM_CLASS bytes>", "client": [1920, 1080]}`
and the run directory's `input.json` (controller, player) as on macOS.

Unchanged: never start the game while Steam or steamcmd runs, no Steam variables,
private `HOME`, owned display, owned process identity.
