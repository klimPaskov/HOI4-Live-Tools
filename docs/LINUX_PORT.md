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
notch per 120 pixels (verified live on 1.19.4.1). Modifiers are held 150 ms around a key
or click and presses last 60 ms, because the game samples modifiers once per frame.

`MacClock::bind_with_input` builds the full tool server from any `GameInput`, so the
Linux server can host the same tools.

## 2. Memory layout (done for 1.19.4.1)

The tools hard-code about 340 Mach-O image addresses (vtables, globals) and many
struct field offsets for the macOS 1.19.4.1 build (Mach-O UUID DFA875A5). The Linux
executable has different addresses, and libstdc++ types (`std::string` 32 bytes,
containers) shift some field offsets relative to libc++.

1. `scripts/gen_linux_layout.py` maps Mach-O items to the ELF (`scripts/cross_platform_map.py`):
   vtables by RTTI name, globals and functions by matched code, field offsets by matched
   instruction displacements, plus owner hints in `scripts/data/xlayout-hints-1.19.4.1.json`.
   A gap between two neighbours with the same shift is filled with that shift.
2. `scripts/data/xlayout-live-1.19.4.1.json` holds every entry checked or found in the
   running game, each with its evidence; `--live` applies it over the static result.
3. The result ships as `native/layouts/linux-<build id>.json` (`xlayout::LINUX_TABLE`). The
   Linux server binds the tools only for exactly that build; an entry missing from it fails
   that read with `layout_unmapped`, never a guess.

Every tool module reads through `xlayout` (`img` for image addresses, `fld` with an owning
`Class` for field offsets); the GUI widget tree (`gui_tree`) translates its layout the same
way. Edit boxes have no UTF-8 text copy on Linux; their text is the editor's UTF-32
`std::wstring`. The special-projects breakthrough map is a libstdc++ red-black tree on
Linux. `construction_place`'s macOS window internals stay macOS-only.

Core modules (`game`, `research`, `own_focus`, `production_lines`, `construction`,
`army`, `gui_registry`, `research_ui`) moved from Linux 1.19.3 to 1.19.4.1 by a
same-platform migration (`scripts/linux_build_migrate.py`). Linux 1.19.3 is no longer
accepted. The older Linux-only GUI diagnostics written for 1.19.3 fail closed on 1.19.4.1
(classes they use were removed); the management tools replace them.

A developer build with `--features dev-table` reads a table from `HOI4_LIVE_DEV_TABLE`
(reloaded when it changes) and logs unmapped keys to `HOI4_LIVE_DEV_MISSING`; release
builds always use the embedded table.

## 3. Live verification (done)

[LIVE_VERIFICATION_LINUX_1_19_4.md](LIVE_VERIFICATION_LINUX_1_19_4.md) records every tool
run against the Linux 1.19.4.1 game and the fixes it needed.

## Launch rules

Binding: `hoi4-live-native --bind-management RUN_DIRECTORY BINDING` with
`{"window": <X11 id>, "window_class": "<exact WM_CLASS bytes>", "client": [1920, 1080]}`
and the run directory's `input.json` (controller, player) as on macOS.

Unchanged: never start the game while Steam or steamcmd runs, no Steam variables,
private `HOME`, owned display, owned process identity.
