"""A declared campaign routine and one-call condition waits (no injected state)."""

from ..errors import ToolError
from ..game import governor
from .registry import ToolResult, tool


@tool("hoi4_governor",
      "Declare a durable standing plan; status and digest read the cached observation. "
      "start requires campaign mode, a game launched by this session, a sticky desktop lease, "
      "and completed own-country telemetry. Research, verified focus preflight and explicit popup policy are supported; "
      "nonempty construction/production/army plans are refused until live state is implemented. "
      "start runs the existing fast clock in the background. Restarts never resume automatically.",
      {"properties": {
          "action": {"type": "string", "enum": ["plan", "status", "digest", "start", "stop"], "default": "status"},
          "plan": {"type": "object", "description": "version=1; research/focus IDs, events mode/rules, retry bounds"},
          "days": {"type": "integer", "minimum": 1, "maximum": 730, "default": 90},
          "speed": {"type": "integer", "minimum": 1, "maximum": 5, "default": 5},
          "max_seconds": {"type": "integer", "minimum": 30, "maximum": 7200, "default": 900}}},
      family="campaign_loop", lock="none", phase=8)
def hoi4_governor(session, args):
    runtime = governor.get(session)
    action = args["action"]
    if action == "plan":
        if "plan" not in args:
            raise ToolError("plan action needs plan")
        data = runtime.set_plan(args["plan"])
    elif action == "start":
        return runtime.start(args["days"], args["speed"], args["max_seconds"])
    elif action == "stop":
        data = runtime.stop()
    elif action == "digest":
        return ToolResult(runtime.compact_digest())
    else:
        data = runtime.status()
    return ToolResult("governor " + action + ("; active" if data["active"] else "; inactive"), data)


@tool("hoi4_wait_for",
      "Wait inside the server for governor daily predicates or its stop notification, at most 60 seconds. "
      "Give OR predicates: {event:research_idle|tech_done|focus_idle|focus_done|production_idle|popup}, "
      "{after_days:positive_integer}, or {section,field,op:gte|lte|eq,value}. "
      "Unknown or stale values never satisfy a predicate. No screen reads or saves.",
      {"properties": {
          "predicates": {"type": "array", "minItems": 1, "maxItems": 64, "items": {"type": "object"}},
          "timeout_s": {"type": "number", "minimum": 0, "maximum": 60, "default": 60}},
       "required": ["predicates"]}, family="campaign_loop", lock="none", phase=8)
def hoi4_wait_for(session, args):
    cancel = getattr(getattr(session, "_call", None), "cancel", None)
    result = governor.get(session).wait_for(args["predicates"], args["timeout_s"], cancel)
    return ToolResult("governor wait: " + result["state"], result)
