"""Reading the running game's state from process memory, and posting the player's own commands to its command queue.

Both are what the Linux project smerss/Clausewitz-MCP (MIT) does; this package is the Windows version, built from the
same ideas and checked against the installed hoi4.exe (see docs/plans/windows-memory-backend.md and NOTICE).

    pe.py        the PE image: sections, MSVC RTTI (class name -> vtable), function table
    process.py   ReadProcessMemory wrapper, module base, region scan
    layout.py    the offsets of the fields this tool reads (one table per game build)
    reader.py    the game state: date, country, economy, production, research, divisions, wars ...
    calls.py     running a game function on the game's main thread (used to post commands)
    commands.py  the player commands (the same objects a click builds)

Nothing here writes game state directly. Reads never stop the game. Commands go through the game's own
CCommandQueue::Post, so every legality check a click gets applies.
"""
