"""HOI4 Live Tools: an MCP server that lets AI agents test Hearts of Iron IV mods and play campaigns on Windows.

Package layout (one module per layer, see docs/PLAN.md):

    protocol/    JSON-RPC 2.0 framing, the MCP stdio server and a JSON-schema validator
    tools/       the tool registry and every MCP tool, grouped by family
    desktop/     window discovery, capture (ffmpeg ddagrab / PrintWindow), SendInput, clipboard, desktop lock
    ocr/         Windows.Media.Ocr through a persistent PowerShell host
    game/        paths, process, logs, console, time control, saves, menus: driving the game itself
    perception/  reading the screen: popups, top bar, screen classifier, text finding
    state/       plain-text save parsing (streaming index + subtree parser); telemetry in phase 2
    actions/     verified UI actions (phase 3+ campaign actions build on actions.verify)
"""

__version__ = "1.1.0"
SERVER_NAME = "hoi4-live-tools"
