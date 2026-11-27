# Game versions

Hearts of Iron IV updates itself through Steam. This page says which game builds HOI4 Live Tools
can read, what happens when a patch lands, how a new build is brought up, and how the ways of
reading the game compare.

## Supported builds

| System | Game version | Build id | Native runtime (memory) | Python runtime (screen) |
|---|---|---|---|---|
| macOS | 1.19.4.1 (checksum f584) | Mach-O UUID `DFA875A5-25BA-35E0-9736-627F8A5C4396` | supported (the canonical build) | any version, limited |
| Linux | 1.19.4.1 | GNU build ID `2265e6e7c1f527bf4b1f2c4b4ca3efd4371dc082` | supported (generated table) | not available |
| Windows | any | | not yet (see [Windows](#windows)) | supported, last run on 1.19.4.2 |
| Windows | 1.20.0.0 (checksum 0c4f) | CodeView `3A4878DC2F3A481EA5375E5943C5541C1` | vtables only (164 of 168); no attachment yet | supported |
| macOS | 1.20.0.0 (checksum 0c4f) | Mach-O UUID `ED135FE5-CDA8-38E8-8835-3F7381B79974` | its own runtime binary, generated from the migration map ([below](#macos-120-a-generated-runtime-per-build)); live self-check pending | |
| macOS | 1.20.1.0 (checksum 724a) | Mach-O UUID `B8BF4554-DC8D-3381-B277-9F4E6230CBA5` | its own runtime binary, generated from the 1.20.0.0 map: the same code in place (`game_migrate.py same-code`, [below](#a-rebuild-with-the-code-in-place)) | |
| Linux | 1.20.0.0 (checksum 0c4f) | GNU build ID `71806afa707a46fb72f0738a719c3f1c3f5e2e42` | table generated, not yet proven live or used by the runtime | not available |

`hoi4-live-native --supported-builds` prints the same list as JSON.

## Why the native runtime is tied to builds

The native runtime reads the game's memory. Every read needs to know where an object lives
(image addresses: about 216 vtables, globals and functions the tools use) and where a value sits
inside it (field offsets: 617 keys in 282 classes for the management tools, plus the core reads of
the clock, research, focus, production, construction, armies and the interface tree). A patch
recompiles the game: all image addresses move, and field offsets move wherever Paradox added or
removed a member. Nothing in the game's files names these values, so each build needs a table.

The values of the canonical macOS build are the keys. A table for any other build maps every key
to that build's value (`native/layouts/<system>-<build id>.json`). Tables sit side by side, keyed by
build id, so a supported build keeps working while a new one is brought up.

## What happens when a patch lands

Today, on a build without a table, the native runtime refuses before attaching (`unsupported_build`)
and names the builds it reads and what works meanwhile: a newer HOI4 Live Tools release when one
exists, an earlier game version through Steam (Properties > Betas) when Paradox offers it, or the
Python server, which reads the screen and has no build check. It never reads with a guessed table.

Until a table for the new build ships, keep a copy of the supported game outside Steam's folder
(Steam only updates its own folder) and point HOI4 Live Tools at it with `game_dir` in
`config.json` or `HOI4_LIVE_GAME_DIR`. Launch that copy directly, with Steam closed.

## Bringing up a new build (maintainers)

Everything below reads the game files as data; nothing runs the game until the live check.

1. Identify the executable: `python3 scripts/game_migrate.py identify PATH/TO/hoi4` prints the
   system, build id, SHA-256 and the version from `launcher-settings.json`.
2. Map every key: `python3 scripts/game_migrate.py migrate NEW_EXE --canonical CANONICAL_MACHO`
   (needs `pip install capstone`). The matcher pairs functions across the two builds (RTTI vtables by
   class name, functions sharing a unique string, callee propagation, vtable slots), maps globals by
   the instructions that reference them, and maps field offsets by aligning the displacements that
   matched functions take on the object's register. It writes the table with a confidence per entry
   (`high`, `medium`, `filled` from neighbours that moved together, or `live`) and a coverage report
   that lists every unresolved key with its reason and, per source file, whether all its keys mapped.
   First run: about half an hour (both executables are disassembled and indexed once, then cached);
   later runs take minutes.
3. Static check (part of `migrate`, or `game_migrate.py check TABLE EXE --canonical CANONICAL`): every
   image entry lands in a section, a vtable entry is an RTTI vtable of the same class whose first slot
   is code, a function entry is a function start, a global stays in the same section as its key (not the
   GOT slot its users load it through), and field entries keep member order, alignment and a plausible
   size. A table with problems is not used.
4. Live check, about 30 minutes on a paused test game: read the clock, political power, a factory
   count, an army's division count, the research slots, the national focus and the interface tree
   through the native tools, save the game as text at the same moment, and compare with
   `python3 scripts/layout_selfcheck.py SAVE TAG MEMORY.json`. A module that disagrees with the save
   stays disabled. Then the live validation queue (`docs/LIVE_VALIDATION_QUEUE.md`) is the regression
   suite for the tools.
5. With the same system's table for the previous build, `--previous TABLE` carries its values into
   classes whose mapped fields did not move, and its live-measured entries with them (confidence
   `carried` or `live`).
6. Unresolved keys get an owner hint (`scripts/data/xlayout-hints-*.json`) or a live entry with its
   evidence, and the table is regenerated.

`game_migrate.py refcheck` repeats the static check for every committed table whose executable is in
the private reference folder (`HOI4_REFERENCE_DIR`, default `~/.cache/hoi4-live/reference/<system>-<build id>/`).
Game executables are never committed; continuous integration runs the pipeline's tests on synthetic
executables only.

### Measured on a real update

The pipeline was run on the last real update (macOS 1.19.3 to 1.19.4.1, a minor version that moved
most layouts) and compared with the migration that had been done by hand and checked live:

| | Mapped | Agreeing with the hand migration where it has an answer |
|---|---|---|
| Image addresses | 213 of 216 (193 high, 20 medium) | 158 of 158 |
| Field offsets | 431 of 617 (70 %) | 42 of 42 |

The static check caught the only wrong entries of the first run (four globals mapped to their GOT
slots), and the matcher was fixed. The 186 unmapped field keys belong mostly to 51 classes with no
vtable or owner hint; per source file, 8 of 53 have every key. A tool whose key is missing refuses
that read (`layout_unmapped`); it never guesses. The hand migration of the same update took about
two working days plus live sessions.

The pipeline then ran on HOI4 1.20.0.0 (the "To The Bitter End" update) for Linux, from the
macOS 1.19.4.1 keys, with the Linux 1.19.4.1 table as the previous build (`--previous`):

| | Mapped | Notes |
|---|---|---|
| Image addresses | 212 of 216 (208 high, 4 medium) | every one moved (180 of 180 comparable entries), by about 0x7000 to 0x8000 |
| Field offsets | 389 of 617 from the matcher alone; 476 of 617 with the previous build's values carried into 47 classes whose mapped fields did not move | of 2,827 field entries both builds map, 2,821 are identical; 5 of the 6 others are the previous table's live corrections |

On macOS 1.20.0.0 (same system as the keys): image addresses 214 of 216 (213 high, 1 medium),
field offsets 503 of 617, and all 5,714 field entries the matcher wrote are unchanged.

So 1.20 moved the code and data, not the structures. The remaining keys are the ones the 1.19.4.1
table had from live measurement or owner hints for classes without a vtable. The static check also
found five static entries in the 1.19.4.1 Linux table that contradict its own live corrections
(unused by any tool); `migrate` now drops such entries.

A patch release (for example 1.19.4.1 to 1.19.4.2) moves every image address but usually few struct
members, as 1.20 shows.

## macOS 1.20: a generated runtime per build

The runtime's sources keep the macOS 1.19.4.1 values. A runtime for macOS 1.20.0.0 is generated from
them and the migration map, so both builds stay supported side by side and nothing is rewritten by hand:

    python3 scripts/macos_build_variant.py native/layouts/migrations/macos-DFA875A5-to-ED135FE5.json --out OUT
    (cd OUT/native && cargo build --release)

The generator copies the crate and replaces every production macOS image literal (vtables, globals,
functions, code addresses, strings and Objective-C ivar offsets in macOS code, and the canonical keys of
`variant.shared_key_files`) by the map's 1.20 value, and the build identity (Mach-O UUID, SHA-256 and
`builds::READER_BUILD`). Field offsets stay (`fields_default: identity`). It accepts only entries of
confidence `high` or `medium`; an unresolved or `low` literal stops it, unless the map lists that file in
`variant.disabled` with a reason: then that literal becomes an address in `memory::DISABLED_BASE`, which
the Reader refuses as `layout_disabled` with the reason (also in `--supported-builds`), so the module
stops with a message instead of reading another build's address. `variant.not_image` lists the numbers in
the image range that are not addresses (pointer floors, a CPU type, field offsets). The report
(`--report`) lists every rewritten site with its old and new value, method and confidence.

For 1.20.0.0 the map covers all 681 sites: 675 high, 1 medium (an address quoted in a tool's
description text), 5 identity strings; no module is disabled. Every vtable the macOS code uses (254)
has the same slot count and slot functions in 1.20. Of the 53 literals first left unresolved, 13 were
resolved by aligned code or aligned RIP references and checked against the neighbouring delta
(`scripts/macos_resolve.py`); the other 40 are Linux ELF literals in Linux-only checks or plain numbers.
A unit test fails when a macOS address in `native/src` has no 1.20 entry.

### A rebuild with the code in place

A hotfix often recompiles the game without moving anything: 1.20.1.0 (Steam build 26921304, 26 Nov 2026)
differs from 1.20.0.0 in 376 bytes of 75 MB: the Mach-O UUID, the version and build-stamp strings, one
byte of constant data and the static initializer that stores the version hashes; the changes that
matter are in the game's script files (Romania and Hungary content, two compliance values). Check and
carry such a build in seconds instead of running `migrate`:

    python3 scripts/game_migrate.py same-code native/layouts/migrations/macos-DFA875A5-to-ED135FE5.json \
        PREV/hoi4.app/Contents/MacOS/hoi4 NEW/hoi4.app/Contents/MacOS/hoi4 \
        --out native/layouts/migrations/macos-DFA875A5-to-<NEW>.json \
        --table native/layouts/macos-<PREV>.json --table-out native/layouts/macos-<NEW>.json

It requires the same section layout and file size, finds every changed byte (a code change marks its
whole function, a string change its whole string) and refuses, listing them, when any image value of
the map lies in or reads from a changed byte; then run `migrate`. The map it writes names its base
(`same_code_of`), so an address added to the base later reaches the rebuild too. Then: add the build to
`native/src/builds.rs` `SUPPORTED`, the in-process module's field row (`inproc/core/caps.c` `FIELDS`,
keyed by UUID), generate and build the runtime with the new map (`macos_build_variant.py`), and run the
live self-check. On 1.20.1.0: an 8-second download (an APFS clone of the previous full copy, then
`steamcmd +app_update` into it fetched 18 KB), the check in under a second, the generated runtime in
about 15 minutes (tests and build); the live self-check is the rest.

### Adding a macOS game address

A new macOS literal in `native/src` needs its 1.20 value in the map, or the test above fails:

    xmap-venv/bin/python scripts/macos_resolve.py CANONICAL_MACHO HOI4_1_20_MACHO native/layouts/migrations/macos-DFA875A5-to-ED135FE5.json 0xADDRESS

prints the new value, method and confidence; add it to the map's `image` table (`high` only when two
signals agree: aligned code or references, and the neighbouring delta).

What the generator rewrites, and the leftover check:

- a literal in a macOS context (a macOS path, or an item named MACOS/Mac) in the image range;
- anywhere else, a literal that is an exact key of the map's `image` table (a canonical key passed
  through `xlayout::img`, a match arm comparing a rewritten layout value), unless it stands in an item
  named for another platform (LINUX, ELF, WINDOWS) or its file or `file:line` is listed in
  `variant.not_macos` with a reason;
- test code is skipped per item: `#[cfg(test)]` covers the item after it (and the files of a
  `#[cfg(test)] mod x;`), never the rest of the file. (Before this rule, one `#[cfg(test)] use` near the
  top of `own_focus.rs`, `launch/macos/focus/binding.rs` or `construction/mac_find.rs` hid every literal
  below it: the 1.20 focus reads failed with `invalid_focus_type`.)

After writing the copy the generator re-reads it and refuses (exit 1, copy removed) when any production
literal is still an old key. The same check runs on its own:

    python3 scripts/macos_build_variant.py MAP.json --verify OUT_DIR

Build scripts run it before cargo, so a variant with a 1.19.4.1 address left in production code never
compiles. Flag values and offsets that fall in the image range go in `variant.not_image`.

### Planned: one binary for every build

The generated runtime is the small safe step. The runtime should instead choose the table when it
attaches, by the running build's UUID, so one binary reads every supported build:

1. `xlayout::img`/`fld` translate on macOS too: the table of the attached build (identity for 1.19.4.1,
   the migration map's `image` for 1.20), chosen by the UUID `platform::macos` already reads at attach.
2. The core layouts (`game`, `own_focus`, `construction`, `research`, `army`, `production_lines`,
   `gui_tree`, `launch/macos/units`) translate their image fields at first use per build, as
   `gui_tree` already does for Linux.
3. The remaining literals (about 250 sites outside `img()`: `base + 0x...` comparisons, match arms on
   vtables, Objective-C ivars, construction and Cocoa globals) go through `img()`; match arms compare
   the canonical key of the read vtable (the inverse table, exact entries only).
4. The macOS gate accepts any build in `builds::SUPPORTED` whose table ships; `DISABLED` becomes per
   build and per module, from the table's confidence.
5. `scripts/macos_build_variant.py` then becomes a check only (the map covers the sources).

## When the canonical build moves

The keys are the canonical macOS build's values, so when the macOS sources move to a new build
(their constants rewritten, as on every macOS update until the runtime selects tables by build),
every other system's table must be re-keyed. Each such move commits a migration map,
`native/layouts/migrations/macos-<from>-to-<to>.json`: for every image key and field key the old
value, the new value and the confidence, the literals no method resolved (`unresolved_image`), and
`fields_default: identity` when the structures did not move. One command applies it to a table:

    python3 scripts/game_migrate.py rekey native/layouts/linux-<build id>.json native/layouts/migrations/macos-DFA875A5-to-ED135FE5.json --write

With generated runtimes per macOS build (above) the canonical build no longer moves: the keys stay
the 1.19.4.1 values for good, as opaque names, and every new build on every system gets a table keyed
by them. No table is re-keyed for 1.20; the map and `rekey` stay for a table someone keyed otherwise.

## Planned: unknown builds on a user's machine

Not built yet. The design, so that patch day is a slower day instead of a broken one:

- The runtime identifies the build. With a shipped table it runs as today.
- Without one, it can make the table locally from the user's own executable (the mapping only reads
  the user's game file, takes minutes once the canonical reference data ships with the release),
  caches it, runs the static check, and enables only modules whose keys all mapped with high or
  medium confidence. Every other module refuses with a message naming what is unavailable.
- Each module proves itself once against the save before it is trusted (the live check above, made
  automatic at an autosave), and disables itself and records an issue if a later check disagrees.
- Each read falls back in order: memory (when its module is proven) -> the telemetry mod's output
  -> the newest save -> the screen. Every value carries its source and age, so the player knows
  how fresh it is.

What the runtime needs for this: the core reads (clock, research, focus, production, construction,
armies, units) still choose their macOS offsets by string ABI instead of by build; they must go
through the same per-build tables as the tool modules first.

## Ways to read the game

| Route | Coverage | Speed and freshness | Across patches | Across systems | Cost |
|---|---|---|---|---|---|
| Memory (native runtime) | everything the game holds, including the interface | microseconds, live | a table per build | one table per system and build | needs the system's permission: the task port on macOS (Developer Tools access), ptrace scope or same-user `process_vm_readv` on Linux, `OpenProcess` with read rights on Windows (no administrator rights for a process of the same user) |
| Telemetry mod (`hoi4live_telemetry`) | what game script can see: political power, stability, war support, manpower, factories, divisions, ships, planes, resources, equipment, research, focus, wars, states, events | daily or weekly lines in `game.log` | stable: game script changes rarely | identical on all three | changes the game checksum (no ironman achievements; in multiplayer every player needs the mod); cannot see the interface, positions of units or orders |
| Saves (`save_as_binary=no`) | the complete strategic state: countries, states, armies and divisions with templates and locations, air wings, ships, production, construction, research, focus, wars | parsing takes seconds to minutes; only as fresh as the last save | version-proof | identical on all three | text saves must be enabled; the binary format is not documented |
| Screen and OCR | what is visible | slow (a capture and recognition per read) | survives most patches; interface changes break it | per-system capture and OCR | needs screen-recording permission; misreads are possible |
| Code injected into the game | everything | fast | breaks every patch | per system | rejected: invasive, and the project never writes to the game's memory |

## Windows

The Python runtime is the supported Windows path today. For the native runtime on Windows:

- A `Process` implementation on the existing read-only reader: `OpenProcess` with
  `PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_VM_READ` for a game the runtime launched, the main
  module's base address (ASLR moves it every launch, so all reads are base-relative, as on macOS and
  Linux), 64-bit pointers, `ReadProcessMemory` in bounded chunks.
- Identification: the PE CodeView GUID and age (`game_migrate.py identify` prints it) plus SHA-256.
- A layout table: `game_migrate.py migrate` maps the vtable keys of a PE by MSVC RTTI (the Itanium
  class names translate to MSVC type descriptors; the vtable follows the slot holding the complete
  object locator). On the Windows 1.20.0.0 executable that gives 164 of the 168 vtable keys (the 4
  others are secondary vtables), each with code in its first slot; 258 of the 259 classes the tools
  name exist there. Globals, functions and field offsets need the matcher to read PE code: PE
  sections and imports, and the x64 Windows calling convention (the object is in `rcx`, not `rdi`). MSVC lays out standard containers differently
  (`std::map`, lists, hash tables), so many field offsets will come from live reads.
- Input (`SendInput` to the owned window) and a launcher that starts the game without Steam, as
  in [Porting status](PORTING_STATUS.md).
