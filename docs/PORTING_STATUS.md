# Porting status: macOS, Linux and Windows

The native runtime (`native/`, Rust) is developed and played on macOS. This page says, per
platform, which of its tools exist, compile, have a memory layout for the current game version
and were last proven against the running game, and what is left to finish each port. It is kept
current by hand; the check in `tests/test_xlayout_coverage.py` keeps the Linux layout list honest.

Game version: HOI4 1.19.4.1 on macOS, 1.20.0.0 on Linux. The tool catalogue has 189 `hoi4_*` names (about
181 player tools plus debug tools; `hoi4-live-native --list-tools` prints them).

## Summary

| | macOS | Linux | Windows |
|---|---|---|---|
| Tools in the build | all | all (the same modules) | all compiled and unit-tested; none reachable (no Windows game attachment or input yet) |
| Compiles in CI | yes | yes (x86_64 and aarch64) | yes (the code compiled there) |
| Memory layout | 1.19.4.1, native (the canonical values) | 1.20.0.0: the 1.19.4.1 table moved by `linux_build_migrate.py --table`: 82 of 94 image keys, 441 of 618 field keys the sources use | vtables only, for Windows 1.19.5.0: 181 of 182 vtable keys by RTTI; no globals or fields |
| Input | CGEvent | X11 XTEST | none in the native runtime |
| Last proven live | continuously | 5 November 2026: 104 tools verified, 11 refusal-only, 8 not reached | native: never; the separate Python runtime: November 2026 on 1.19.4.2 |

## Linux

The tools in `native/src/launch/macos/` are compiled for Linux too and are served by the owned
Linux launcher (`--serve`, then `--bind-management` with an X11 window). Every memory address and
field offset a tool reads goes through `xlayout`: macOS values are the keys, and the Linux build
looks them up in `native/layouts/linux-<build id>.json`. A key missing from the table fails that
read with `layout_unmapped`; nothing is guessed.

What works:
- The 150 tools that existed on 5 November 2026 were run against the Linux game on that date
  ([record](LIVE_VERIFICATION_LINUX_1_19_4.md)).
- The executor (`hoi4_autopilot`) and the change watcher (`hoi4_observe`, `hoi4_wait_change`) run in
  the Linux supervisor once the management tools are bound, as they do beside the macOS server.
- The learning tools (`hoi4_ui_inspect`, `hoi4_ui_act`, skills and facts) scan the Linux window
  roots (+0x338 views, +0x330 pop-ups) measured on the Linux game.

What is missing:
- 39 tools were added after the Linux live check (battles, army orders such as gather, detach, attack and
  fallback, occupation and its laws, stockpile, war statistics, operations, railways, the learning
  tools, the executor and watcher). None has run on Linux.
- 12 image keys and 177 field keys the tools use are not in the Linux 1.20.0.0 table. They are
  listed by source file in `scripts/data/xlayout-unmapped-1.20.0.0.json`; the tools that need them
  refuse on Linux until they are mapped. 87 of the field keys were in the 1.19.4.1 table: the move to
  1.20.0.0 kept only what the matched code confirms (every confirmed field kept its offset), and the
  rest (struct strides, GUI helper objects without RTTI, a few single-vote members) waits for live
  reads.
- No screen recorder (the macOS recorder uses ScreenCaptureKit).

The Linux build is 1.20.0.0 (GNU build ID `71806afa707a46fb72f0738a719c3f1c3f5e2e42`, SHA-256
`f8fe17fb1047cc94a64e7cc8f13780494e8eb111b9c3a3468256bfbc4f499874`). Its table was moved from the
1.19.4.1 one (build ID `2265e6e7`) with `scripts/linux_build_migrate.py OLD NEW --table
native/layouts/linux-2265e6e7....json --hints scripts/data/xlayout-hints-1.19.4.1.json`: all 182 image
values mapped, 2,998 of 3,098 fields confirmed (all at their 1.19.4.1 offsets), 4 more by the bracketed
rule. The report is `native/layouts/linux-71806afa....coverage.json`. The core modules' own Linux
values moved with the same matcher; values in the older GUI diagnostics that no code match covers
took the shift of the mapped neighbours on both sides.

Steps to finish, in order:
1. Live reads on 1.20.0.0 for the keys the move left out, added as live entries with their evidence.
2. Run the new tools against the Linux game (any x86_64 Linux with an X server; a virtual display
   such as Xvfb with software rendering works but is slow in war) and add what differs to
   the live entries with its evidence.
3. A recorder for X11 (for example ffmpeg `x11grab` on the owned display) if Linux campaigns are to
   be filmed.

Without the executable, `python3 scripts/gen_linux_layout.py extend native/layouts/linux-<id>.json`
fills the keys whose class has mapped neighbours with the same shift (the generator's bracketed
rule) and prints the rest by source file; `--write` stores the fills, `--unmapped FILE` the list.

## Windows

Two runtimes exist:
- The Python runtime (`hoi4_live/`, 52 tools) reads the screen with Windows OCR and drives the game
  with ordinary input. It is the supported Windows path for mod testing and was last run on
  Windows HOI4 1.19.4.2. It has no memory reads and is not a campaign runtime.
- The native runtime has a read-only Windows process reader (`native/src/platform/windows/`) with
  an empty build allowlist. It attaches to no game, sends no input, has no launcher and no layout.
  The management tools are compiled and unit-tested on Windows, but nothing binds them there yet.
  Their journals hold the exclusive lock on a sibling `<journal>.lock` file on Windows (a Windows
  lock on the journal itself would stop every other reader); the daily plan store still refuses on
  Windows (its owner-only atomic writes are not verified there).

Steps to finish, in order:
1. Done: the tool modules compile on Windows and Windows CI runs their unit tests.
2. The private-file checks (controller state, daily plan store) for Windows: owner-only ACLs
   instead of POSIX modes.
3. A `Process` for an owned Windows game on top of the existing reader, and a launcher that starts
   the game without Steam variables in a private user folder, like the Linux launcher.
4. A `GameInput` for Windows (`SendInput` to the owned, foreground window; client geometry, DPI,
   keyboard layout), with the modifier timing the Linux backend needed.
5. A layout table for the Windows build. Started: `scripts/pe_rtti_map.py MACHO PE` maps every
   vtable key the sources use (the `Class` tags, the `img()` vtables and the `*vtable*` values of
   the hand-written core layouts) to the PE by RTTI name: the Itanium typeinfo name of the macOS
   address point is rewritten in MSVC form and looked up through the complete object locators.
   Against Windows 1.19.5.0 (`hoi4.exe` SHA-256 `709f916afbeb...01fce148`, the Steam build on the test
   machine, one version past macOS 1.19.4.1) it maps 177 keys with high confidence (primary
   vtable) and 4 with medium (secondary vtables matched by order), leaves one template
   (`TNullObject<CBuildingTemplate>`) and lists 8 globals it cannot map
   (`scripts/data/windows-1.19.5.0-vtables.json`, report beside it). RTTI names do not change
   between versions, so these hold for 1.19.5.0 even though the keys come from 1.19.4.1; a
   class whose fields changed in 1.19.5.0 still needs its fields checked live. The mapper
   (`scripts/cross_platform_map.py`)
   (`scripts/cross_platform_map.py`) reads Mach-O and ELF only. For a PE executable it can be
   extended for vtables (MSVC RTTI names translate from Itanium names) and for globals reached
   through matched functions, but MSVC code differs from clang code far more than gcc code does,
   so field offsets will mostly come from live reads: MSVC `std::string` is 32 bytes like
   libstdc++ but `std::map`, `std::list` and hash containers have other sizes and layouts.
6. Live checks of every tool on a Windows game, recorded like the Linux record.

## Continuous integration

| Workflow | macOS | Linux | Windows |
|---|---|---|---|
| Native Rust | fmt, clippy `-D warnings`, `cargo test` | the same, plus aarch64 clippy, the Yama-gated reader test and an Xvfb input fixture | the same full set (nine tests ignored there with the reason: the daily plan store and three that read a locked journal), plus the owned reader fixture |
| Python | unit tests on 3.9 and 3.13, plan-key check, protocol smoke | the same | the same |

`tests/test_xlayout_coverage.py` (Python workflow, every platform) fails when the sources use a
layout key that is neither in the Linux table nor listed as unmapped, and when a listed key has
been mapped, so each new macOS tool shows its Linux cost in its own pull request.

Known flaky on Windows runners: the focus and research stage timing tests. If only those fail and
the change does not touch them, rerun the failed job once before investigating.

No GitHub-hosted runner can run the game; live checks need a machine with the game installed.
