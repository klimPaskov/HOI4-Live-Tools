"""Tool registry: every MCP tool is a function registered with @tool.

A tool declares:
    name, description          shown to the agent in tools/list
    schema                     JSON schema of its arguments (validated before the call, defaults filled in)
    family                     session | lowlevel | testmode | time | popups | understanding | campaign | military |
                               campaign_loop (docs grouping)
    lock                       "acting": the dispatcher holds the desktop lock for the whole call
                               "self":   the tool takes the lock itself around the acting part only
                               "none":   read-only, never touches the desktop
    test_only                  refused in campaign mode (console cheats and other test shortcuts)
    examples                   [{"args": {...}, "note": "..."}] for docs/tools.md and the README
    phase                      the plan phase that delivered it

The handler signature is handler(session, args) -> ToolResult | dict | str.

Later phases register more tools the same way (import their module in tools/__init__.py). PLANNED lists the tools
of later phases so `hoi4_status` and docs can show the roadmap without exposing unimplemented tools.
"""

import base64
import io
import json
import functools

from ..protocol.schema import check_schema

_TOOLS = {}

PLANNED = {}


class Tool:
    def __init__(self, name, description, schema, handler, family, lock, test_only, examples, phase, annotations):
        self.name = name
        self.description = description
        self.schema = schema
        self.handler = handler
        self.family = family
        self.lock = lock
        self.test_only = test_only
        self.examples = examples or []
        self.phase = phase
        self.annotations = annotations or {}

    def listing(self):
        entry = {"name": self.name, "description": self.description, "inputSchema": self.schema}
        ann = dict(self.annotations)
        ann.setdefault("readOnlyHint", self.lock == "none")
        ann.setdefault("openWorldHint", False)
        entry["annotations"] = ann
        return entry


def tool(name, description, schema=None, family="misc", lock="acting", test_only=False, examples=None, phase=1,
         annotations=None):
    schema = schema or {"type": "object", "properties": {}, "additionalProperties": False}
    schema.setdefault("type", "object")
    schema.setdefault("additionalProperties", False)
    check_schema(schema, name)
    if lock not in ("acting", "self", "none"):
        raise ValueError(f"{name}: bad lock mode {lock}")

    def deco(fn):
        if name in _TOOLS:
            raise ValueError(f"duplicate tool {name}")
        @functools.wraps(fn)
        def guarded(session, args):
            # Direct handler calls from background/compound tools share the
            # campaign boundary with stdio dispatch. QA fixtures may inject a
            # simpler session; production Session always supplies this method.
            if hasattr(session, "authorize_tool"):
                from ..protocol.schema import validate
                args = validate(schema, args, "arguments")
                session.authorize_tool(name, args, test_only)
                from . import dispatch
                dispatch._campaign_visibility(session, name, args)
                if (getattr(session, "owned_process_only", False) and lock != "none"
                        and name not in ("hoi4_launch", "hoi4_telemetry")):
                    session.require_owned_game()
                if test_only or name == "hoi4_save":
                    from ..game import gamemode
                    gamemode.check(session, name)
                # Direct nested/background calls retain the same UI exclusion as
                # stdio calls; both locks are reentrant within the owning session.
                if lock == "acting":
                    with session.ui_mutex, session.lock.hold(name):
                        return fn(session, args)
                if lock == "self":
                    with session.ui_mutex:
                        return fn(session, args)
            return fn(session, args)
        _TOOLS[name] = Tool(name, description, schema, guarded, family, lock, test_only, examples, phase, annotations)
        return guarded
    return deco


def all_tools():
    return list(_TOOLS.values())


def get(name):
    return _TOOLS.get(name)


class ToolResult:
    """What a tool returns: a short human summary, structured data and optional PNG images (bytes)."""

    def __init__(self, summary="", data=None, images=None, is_error=False):
        self.summary = summary
        self.data = data if data is not None else {}
        self.images = images or []
        self.is_error = is_error

    def add_image(self, png_bytes, mime="image/png"):
        self.images.append((png_bytes, mime))
        return self

    def to_mcp(self):
        text = self.summary or ""
        if self.data:
            dumped = json.dumps(self.data, indent=1, ensure_ascii=False, default=str)
            text = (text + "\n" + dumped) if text else dumped
        content = [{"type": "text", "text": text or "ok"}]
        for img, mime in self.images:
            content.append({"type": "image", "data": base64.b64encode(img).decode("ascii"), "mimeType": mime})
        out = {"content": content, "isError": bool(self.is_error)}
        if isinstance(self.data, dict) and self.data:
            out["structuredContent"] = json.loads(json.dumps(self.data, default=str))
        return out


def coerce_result(value):
    if isinstance(value, ToolResult):
        return value
    if isinstance(value, dict):
        return ToolResult(value.pop("_summary", ""), value)
    if value is None:
        return ToolResult("ok")
    return ToolResult(str(value))


def png_bytes(image):
    """PIL image -> PNG bytes."""
    buf = io.BytesIO()
    image.save(buf, format="PNG", optimize=False)
    return buf.getvalue()
