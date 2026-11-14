"""`hoi4-live play` on the native runtime: attach to the user's game, supervise the run, serve the player.

    runs        the run folder: private files, the active-run pointer, the brief, the user-file check
    game        find the user's game process, ask Steam to start it, the display the runtime needs
    native      one native runtime process spoken to over JSON-RPC on its standard streams
    supervisor  the background owner of a run: lease, executor, duplicate guard, idle guard, clean stop
    proxy       the `hoi4-live-play` MCP server the coding client starts; forwards to the supervisor
    report      the end-of-run report
    start       the macOS start path of `hoi4-live play`

Standard library only: the command and the proxy run with the Python that installed the program, without the
Python server's dependencies. Design: docs/PLAY_FLOW.md.
"""
