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
5. Unresolved keys get an owner hint (`scripts/data/xlayout-hints-*.json`) or a live entry with its
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

A patch release (for example 1.19.4.1 to 1.19.4.2) moves every image address but usually few struct
members, so more of its fields map than in this minor-version case.

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
- A layout table: the matcher reads Mach-O and ELF. For a PE it needs MSVC RTTI (vtables by class
  name translate from the Itanium names), PE sections and imports, and x64 calling-convention
  differences (the object is in `rcx`, not `rdi`). MSVC lays out standard containers differently
  (`std::map`, lists, hash tables), so many field offsets will come from live reads.
- Input (`SendInput` to the owned window) and a launcher that starts the game without Steam, as
  in [Porting status](PORTING_STATUS.md).
