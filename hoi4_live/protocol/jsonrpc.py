"""JSON-RPC 2.0 message helpers for the MCP stdio transport.

The stdio transport carries one JSON message per line (UTF-8, no embedded newlines). Requests have an id and get a
response; notifications have no id and get none. A JSON array is a batch (older protocol revisions).
"""

import json

PARSE_ERROR = -32700
INVALID_REQUEST = -32600
METHOD_NOT_FOUND = -32601
INVALID_PARAMS = -32602
INTERNAL_ERROR = -32603


class RpcError(Exception):
    def __init__(self, code, message, data=None):
        super().__init__(message)
        self.code = code
        self.message = message
        self.data = data


def response(msg_id, result):
    return {"jsonrpc": "2.0", "id": msg_id, "result": result}


def error(msg_id, code, message, data=None):
    err = {"code": code, "message": message}
    if data is not None:
        err["data"] = data
    return {"jsonrpc": "2.0", "id": msg_id, "error": err}


def notification(method, params=None):
    msg = {"jsonrpc": "2.0", "method": method}
    if params is not None:
        msg["params"] = params
    return msg


def encode(msg):
    """One line of compact JSON. ensure_ascii keeps the stream 7-bit safe on any console code page."""
    return json.dumps(msg, separators=(",", ":"), ensure_ascii=True) + "\n"


def decode(line):
    """Parse one line. Returns a dict or a list (batch); raises RpcError(PARSE_ERROR) on bad JSON."""
    try:
        return json.loads(line)
    except (ValueError, UnicodeDecodeError) as e:
        raise RpcError(PARSE_ERROR, f"parse error: {e}")


def check_request(msg):
    """Validate the envelope of a single message. Returns (kind, id, method, params)."""
    if not isinstance(msg, dict):
        raise RpcError(INVALID_REQUEST, "message must be a JSON object")
    if msg.get("jsonrpc") != "2.0":
        raise RpcError(INVALID_REQUEST, "jsonrpc must be '2.0'")
    if "method" in msg:
        method = msg["method"]
        if not isinstance(method, str):
            raise RpcError(INVALID_REQUEST, "method must be a string")
        params = msg.get("params", {})
        if params is None:
            params = {}
        if not isinstance(params, (dict, list)):
            raise RpcError(INVALID_PARAMS, "params must be an object")
        if "id" in msg and msg["id"] is not None:
            if not isinstance(msg["id"], (str, int)) or isinstance(msg["id"], bool):
                raise RpcError(INVALID_REQUEST, "id must be a string or integer")
            return "request", msg["id"], method, params
        return "notification", None, method, params
    if "result" in msg or "error" in msg:
        return "response", msg.get("id"), None, None
    raise RpcError(INVALID_REQUEST, "message has neither method nor result")
