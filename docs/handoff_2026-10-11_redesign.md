# Handoff: small-model redesign, memory backend spike (11 October 2026)

Historical report at the original stop time. Later WIP and static corrections
are summarized in [HANDOFF_MAC.md](HANDOFF_MAC.md). The public NOTICE now
acknowledges the consulted reference's ideas; the older attribution assessment
below records the state before publication.

Stopped by the coordinator on 11 Oct 2026 (about 08:15) because the work moves to another agent. Step A (study and Windows feasibility spike) is done to the point of a clear decision. Step B (governor, digest, event waits, war automation) and step C (Haiku acceptance run) were NOT started. Nothing in the existing tools changed. 324 existing tests were not re-run; the only new test is `tests/test_memory_pe.py` (passes).

## Machine state at the stop

- My game (pid 49756, launched 07:38, Germany 1 Jan 1936 loaded from `hoi4live_ger3_day0`) disappeared at about 07:58 with no crash dump, the same way run 3 did. At 07:58:45 a new `hoi4.exe -debug` was started by `explorer.exe` (a person, not an MCP server) and at 08:04 a new 1936 game was begun in it, in a window on a second monitor (client at x = -1938). I did not touch that instance. It is still running (pid 22000) and is not mine to close; the desktop lock is released.
- `dlc_load.json` and `pdx_settings.txt` are byte-identical to the backups in `<videos>/hoi4_live_demo/backups/run_governor/`. `settings.txt` had been rewritten by a game run and was restored from the same folder (the rewritten copy is kept as `settings.txt.changed_by_game`). The running instance may rewrite it again on exit.
- Scratch clone of smerss/Clausewitz-MCP (MIT, read only, nothing executed, nothing copied into the repo, so no NOTICE entry is needed yet): `<projects>/_scratch/Clausewitz-MCP`. Spike scripts and pickles: `<projects>/_scratch/spike` (copies of the scripts are in `scripts/memory_spike/`).

## What smerss/Clausewitz-MCP does (key files)

Linux only (ELF, ptrace), HOI4 1.19.2, 107 tools. Same game version as the installed one here (`1.19.2.0.a729`).

- `research/FINDINGS.md` (read it first, 750 lines): globals, struct offsets, handle types, the command queue, order types, combat objects, post-mortems of every crash.
- `calling/commands.py`: the command path. `queue = idler->vtable[+0x90](idler)`, `cmd = operator new(size)`, build the object, `Post(queue, cmd, force=0)`. A command can be built without its constructor (vtable pointer plus a fixed base init plus fields), or with the game's own constructor (preferred after a crash caused by a hand-built diplomatic action). `calling/tables/commands.txt` lists 358 constructors with argument kinds (`handle`, `deref`, `int`, `ptr`) and field offsets.
- `calling/turnloop.py`: `hoi4_wait_for` (sleep until focus, research, production idle, construction empty, deployment, war, political power, or a day limit; pure reads). Worth copying as a design.
- `calling/ragemicro.py`, `combat.py`, `doctrine.py`: the per-day micro engine (one tick about 46 ms), combat read from the game's own damage numbers, terrain and river data from game files, stacking and width rules, encirclement search over the province graph. `mapdata.py`: province graph and rivers from `rivers.bmp`.
- `calling/fieldmap.py`: field names from the game's serializer (uses objdump; not reusable on Windows without a disassembler).
- Safety rules worth keeping: only call functions whose signature was recovered; check handle types before posting (CCharacter 73 vs CArmyLeader 4713 crashed the game once); no injected call inside a loop; command vtable layout is not universal (do not assume CanExecute is slot 0x50); read back every command; the pause command is a toggle (read `idler+0x6C1` first).

## Windows feasibility: findings with evidence

Everything below was run against the installed `hoi4.exe` (size 55,680,632, PE timestamp 0x6a427fb9, build key `6a427fb9_3519e78`) and a live game (Germany, 1 Jan 1936, a local QA playset). Spike code: `scripts/memory_spike/`; the reusable part is `hoi4_live/memory/pe.py`.

1. **MSVC RTTI works.** `pe.read_rtti` finds 14,040 classes with vtables in 0.2 s, including 370 `C...Command` classes (Linux: 358), `CCommandQueue` has no vtable of its own in the table. Test: `tests/test_memory_pe.py`.
2. **ReadProcessMemory works** with a plain `OpenProcess(VM_READ|QUERY_INFORMATION)`; no anti-cheat blocks it. ASLR: the image base was `0x7ff6b1690000` (delta from `0x140000000`). Full heap scans for a vtable take about 145 s in numpy (slow; use pointer chains from globals instead).
3. **Global roots (RVAs for this build).** `CCurrentGameState*` at `base+0x3316e90`; `CInGameIdler*` at `base+0x33172e0`; `CSession*` at `base+0x3438e48`. Found by scanning `.data` for pointers whose first qword is a known vtable (about 5,000 candidates, a few seconds). Do not hardcode without checking the build key.
4. **Layouts mostly match the Linux tables.** Verified live:
   - `CCurrentGameState`: countries `CPdxArray<CCountry*>` at `+0x310` (array length varies with the playset), majors at `+0x328`, tag index array at `+0x340`, active armies/divisions `+0x640`. The played country index is at **`+0x520`** (Linux: `+0x518`), value 103 = "German Reich" (id is the index).
   - `CCountry`: id `+0x08`, name `std::string` at `+0x10` (MSVC string: 16-byte buffer, size at `+0x10`, capacity at `+0x18` of the string), divisions array `+0x290` (30 at the 1936 start), states `+0x460`. After `+0xE60` the layout shifts by **+0x28** compared with Linux: `CTechnologyStatus +0xF60`, `CProductionStatus +0xF68`, `CDeploymentStatus +0xF70`, `CReinforcementStatus +0xF78`, `CArmyUpgradesStatus +0xF80`, `CDiplomacyStatus +0xF88`, `CPoliticalStatus +0xF90`, `CLogisticsStatus +0xF98`, `CDecisionStatus +0xFA0`, `CProgramStatus@NProject +0xFA8`, `CCountryCharacters +0xFF0`, capital state id `+0xFF0+0x28 = +0x1018`... (read as `c+0xFF0+0x28` gave 64 = Berlin's state), `CCountryResources +0x11F8`, `CNuke +0x1310`, `CBuildingStatus +0x1350`, `CNationalFocusTree +0x1370`, `CNationalFocusProgress +0x1380`, `CCountryFuelStatus +0x1580`, `CCountryExperienceStatus +0x1588`, stability base `+0x10A8+0x28`, war support base `+0x10B0+0x28` (read 0.7 and 0.3 as fractions).
   - Sub-objects keep the Linux layout: `CProductionStatus`: military lines array `+0x58` (21 lines, capacity 28), military factories `+0x2B8` = 28, dockyards `+0x318` = 10, civilian `+0x378` = 35 (the known German start numbers); `CPoliticalStatus +0xE0` political power fixed point x1e5 = 1.324.
   - `CArmy` (a division): strength `+0x420` (227.2), organisation `+0x428` (53.45), experience `+0x430`, handle `+0x18` (`{51, id}`).
   - Fixed point is `int64 / 100000`.
5. **The date was not found.** `CInGameClock` in memory is a GUI widget. An `int32` hour counter `(year*365+day)*24+hour` near the 1/24 float signature (the Linux method) gave 0 hits in the whole heap; a range scan gave 513 candidates, one of which (`0x28b1dfc5d10`, value base+11 hours) was never examined because the game vanished. Next step: examine that candidate, or diff two scans one game day apart. A fallback is the telemetry day record.
6. **CCommandQueue::Post exists on Windows and was identified statically.** `CPauseGame` constructor `0xdd1780` is called from three functions; two call `0x223bbc0` right after it, and `0x223bbc0` has 591 callers. Its prologue matches Post(queue, cmd, bool): it calls `cmd->vtable[+0x80]` and `cmd->vtable[+0x60]` and returns early on non-zero, and ends by calling `cmd->vtable[+0x0](cmd, 1)` (delete). The queue comes from `idler->vtable[+0x88](idler)` (Linux `+0x90`) with `idler = [base+0x33172e0]`, seen in the code of the caller at `0xef1a60`. Operator new for a `0x50`-byte command is called as `new(0x50)` (the allocator thunk is the call before the constructor).
   - Base command init is identical to Linux: byte `[+8]=0`, `[+0xC]=-1`, `[+0x10]=0`, dword `[+0x14]=0xffff0000`, word `[+0x18]=0`, qword `[+0x1C]=0`, vptr at `[+0]`, then member init at `+0x28`.
   - **Argument layouts differ from Linux** in places: `CAddProductionLineFactoriesCommand` is `0x38` bytes (Linux `0x30`), the handle is at `+0x28` (8 bytes) and the amount at `+0x30` (Linux `+0x24` and `+0x2C`). Do not copy Linux offsets; call the game's own constructor with the arguments the Linux `commands.txt` lists (MSVC ABI: rcx this, rdx, r8, r9, then stack) and let it fill the object.
   - Not done: calling anything. Planned method: suspend the main thread (the window's thread) only while it sits in an OS wait (RIP outside `hoi4.exe`), point RIP at a stub that saves all registers and xmm0-5, calls a function with up to four register arguments and stores the result, restores everything and jumps back; or `CreateRemoteThread` with the same stub. Test first on harmless calls (get the queue pointer, then a speed command). The risks are a thread suspended inside an allocator lock and a spurious wake of an interrupted wait; both are unverified.
7. **No disassembler is installed** (no objdump, dumpbin, capstone). Statistical call-graph analysis with `.pdata` function bounds and `E8` scanning worked well (`scripts/memory_spike/pe_code.py`, `post_find.py`, `codelib.py`). Serializer field maps (Linux `fieldmap.py`) would need a small x64 decoder; MSVC code for the serializers is long and inlined, so empirical discovery with the generic dumpers (`dump.py`, `objs.py`) plus telemetry as ground truth was judged faster for the fields we need.

## Decision (evidence based)

- **Memory reads: feasible and worth building.** The layouts largely match Linux, values were correct on the first try (factories, political power, division strength and organisation). What remains is the discovery of about 40 more offsets (research slots, focus progress, construction queue, deployment queue, equipment stockpile, wars, army groups and orders, supply, events/popups) and the date. Each needs a live game and a ground truth (telemetry day record, `hoi4_state` from a save, or the screen). Read-only and safe.
- **Command posting: feasible in principle, unproven.** Post and the constructors are located; nothing was executed. It carries real crash risk and each command needs argument verification. It is the only way to make production, research, construction and army assignment take milliseconds instead of 20 to 40 seconds of UI, so it is worth a bounded second spike: queue pointer, then a speed command, then production factories, then `COrderAssign`/`COrderGroup`/`COrderNewRoot`/`COrderNewFront`/`COrderExecute`.
- Keep the UI actions as the fallback, made id based, until commands are proven.

## What was built

- `hoi4_live/memory/__init__.py` (package overview) and `hoi4_live/memory/pe.py` (PE sections, MSVC RTTI, `.pdata` function table, per-build cache). Tested by `tests/test_memory_pe.py`.
- `scripts/memory_spike/*.py`: the research scripts (process wrapper `winmem.py`, RTTI, call graph, struct dumper, instance scan). Scratch quality, not part of the package; run with a PID in `HPID`.
- This handoff. Nothing else changed; no tool behaviour changed; the release zip and pins were not touched.

## Half done and not done (files)

- Uncommitted: none after the commit that carries this handoff.
- Not started: `winproc` access class in the package (see `scripts/memory_spike/winmem.py` for a working draft), `layout.py`, `reader.py`, `calls.py`, `commands.py`, the governor, digest, event wait, war automation, rewritten skill, Haiku acceptance, release packaging, the docs/plans/windows-memory-backend.md write-up (this handoff replaces it for now).

## Recommended next steps

1. Own the game first: check `tasklist` for a `hoi4.exe` that was not started by the MCP (the user starts debug games by hand); the lock does not stop a person. Two games of mine vanished while others were using the machine.
2. Memory reader for the digest: finish the date (examine the 513 candidates or diff two scans), then discover offsets with telemetry as ground truth. Use pointer chains from the three global RVAs (re-found by the `.data` scan when the build key changes), never full heap scans.
3. Event-driven waiting on memory: date, pause flag (`idler+0x6C1` on Linux; find the Windows value), research slots, focus progress, production lines, construction queue, deployment, wars. Then a governor that reads memory every game day without pausing.
4. Command spike as described above, on a throwaway save, with the game started by the tool.
5. Design for small models as in the brief: one digest of 1 to 2 KB with alerts that each carry a one-call fix; standing plan JSON; `hoi4_wait` that sleeps in the server; id based verbs; war verbs (army per front region, front line, offensive line, split by state, reinforce).
6. Acceptance with a Haiku subagent only after the tools run cleanly with a Sonnet-driven smoke test.
