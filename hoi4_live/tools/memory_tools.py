"""Read-only memory tools: no PID parameter and no command-posting surface."""

from .registry import ToolResult, tool


@tool("hoi4_memory",
      "Read verified player-visible fields from this server's own Windows game launch. "
      "Refuses other processes, unknown builds, invalid pointers and inconsistent objects. "
      "Reports unavailable fields explicitly; date and command posting are not yet supported. "
      "No game state is written and the game is never paused.",
      {"properties": {}}, family="understanding", lock="none", phase=8)
def hoi4_memory(session, args):
    snapshot = session.memory_snapshot()
    country = snapshot.get("country") or {}
    return ToolResult("memory: " + (country.get("name") or "played country unavailable"), snapshot)
