#!/usr/bin/env python3
"""tool_schemas.py: the MCP tool catalogue (names, descriptions, input schemas) from the native sources.

Usage:
    tool_schemas.py build [--src native/src] [--out skills/hoi4-campaign-playbook/tools.schema.json]
    tool_schemas.py show TOOL                          # required and allowed arguments, description
    tool_schemas.py check-call TOOL '{"arg": ...}'     # validates one call against the catalogue

`build` asks the native binary (`hoi4-live-native --list-tools`, no game or session needed), which is
authoritative; without a built binary it falls back to scanning the json! literals in native/src (which
misses tools built by helper functions). Plans and briefs can then be checked offline: the tool exists, its
required arguments are present and no unknown argument is passed. `request_id` and `lease_token` are
filled by the broker and are never required of a plan. Stdlib only, Python 3.9+.
"""
import argparse
import json
import os
import re
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(os.path.dirname(HERE))
DEFAULT_SRC = os.path.join(ROOT, "native", "src")
DEFAULT_OUT = os.path.join(ROOT, "skills", "hoi4-campaign-playbook", "tools.schema.json")
BROKER_ARGS = {"request_id", "lease_token"}
START = re.compile(r'\{\s*"name"\s*:\s*"(hoi4_[a-z0-9_]+)"')


def _balanced(text, i):
    """End index (exclusive) of the JSON object starting at text[i] == '{', honouring strings."""
    depth, j, in_str, esc = 0, i, False, False
    while j < len(text):
        c = text[j]
        if in_str:
            if esc:
                esc = False
            elif c == "\\":
                esc = True
            elif c == '"':
                in_str = False
        elif c == '"':
            in_str = True
        elif c == "{":
            depth += 1
        elif c == "}":
            depth -= 1
            if depth == 0:
                return j + 1
        j += 1
    return -1


def _sanitize(literal):
    """Make a json! literal with Rust expressions parseable: `format!(...)` becomes a string and a bare
    identifier value becomes a marker object."""
    out, i = [], 0
    while True:
        j = literal.find("format!(", i)
        if j < 0:
            out.append(literal[i:])
            break
        out.append(literal[i:j])
        depth, k, in_str, esc = 0, j + len("format!"), False, False
        while k < len(literal):
            c = literal[k]
            if in_str:
                esc = (c == "\\") and not esc
                if c == '"' and not esc:
                    in_str = False
            elif c == '"':
                in_str = True
            elif c == "(":
                depth += 1
            elif c == ")":
                depth -= 1
                if depth == 0:
                    break
            k += 1
        out.append('"(formatted description)"')
        i = k + 1
    text = "".join(out)
    return re.sub(r'(:\s*)([A-Za-z_][A-Za-z0-9_:.()]*)(\s*[,}])', r'\1{"__rust_expr__": true}\3', text)


def extract(src=DEFAULT_SRC):
    """{tool: {"description", "inputSchema", "source"}} for every literal with an inputSchema."""
    tools = {}
    for dirpath, _, files in os.walk(src):
        for name in sorted(files):
            if not name.endswith(".rs"):
                continue
            path = os.path.join(dirpath, name)
            with open(path, encoding="utf-8") as f:
                text = f.read()
            for m in START.finditer(text):
                end = _balanced(text, m.start())
                if end < 0:
                    continue
                literal = text[m.start():end]
                if '"inputSchema"' not in literal:
                    continue
                try:
                    obj, unknown_schema = json.loads(literal), False
                except ValueError:
                    try:
                        obj, unknown_schema = json.loads(_sanitize(literal)), True
                    except ValueError:
                        continue
                    unknown_schema = obj.get("inputSchema") == {"__rust_expr__": True}
                tool = obj.get("name")
                if tool and tool not in tools:
                    entry = {"description": obj.get("description", ""),
                             "inputSchema": {} if unknown_schema else (obj.get("inputSchema") or {}),
                             "source": os.path.relpath(path, ROOT)}
                    if unknown_schema:
                        entry["schema_unknown"] = True  # a Rust value; only the tool's existence is checked
                    tools[tool] = entry
    return dict(sorted(tools.items()))


def check_call(catalogue, tool, args):
    """List of problems with one call (empty when it is valid)."""
    if tool not in catalogue:
        return ["unknown tool %s" % tool]
    if catalogue[tool].get("schema_unknown"):
        return []
    schema = catalogue[tool].get("inputSchema") or {}
    props = schema.get("properties") or {}
    problems = []
    if not isinstance(args, dict):
        return ["%s arguments must be an object" % tool]
    for req in schema.get("required") or []:
        if req not in BROKER_ARGS and req not in args:
            problems.append("%s: missing required argument %s" % (tool, req))
    if schema.get("additionalProperties") is False:
        for key in args:
            if key not in props and key not in BROKER_ARGS:
                problems.append("%s: unknown argument %s (allowed: %s)" % (tool, key, ", ".join(sorted(props))))
    for key, value in args.items():
        spec = props.get(key) or {}
        kind = spec.get("type")
        kinds = kind if isinstance(kind, list) else [kind]
        checks = {"string": isinstance(value, str),
                  "integer": isinstance(value, int) and not isinstance(value, bool),
                  "number": isinstance(value, (int, float)) and not isinstance(value, bool),
                  "boolean": isinstance(value, bool), "array": isinstance(value, list),
                  "object": isinstance(value, dict), "null": value is None}
        ok = kind is None or any(checks.get(k, True) for k in kinds)
        if not ok:
            problems.append("%s: argument %s should be %s" % (tool, key, kind))
        if isinstance(spec.get("enum"), list) and value not in spec["enum"]:
            problems.append("%s: argument %s=%r not in %s" % (tool, key, value, spec["enum"]))
    return problems


CALL_IN_TEXT = re.compile(r"`(hoi4_[a-z0-9_]+)\s*(\{.*?\})?`")


def calls_in_text(text):
    """[(tool, args or None, raw)] for `hoi4_tool {json}` spans in markdown (args None when not JSON)."""
    out = []
    for m in CALL_IN_TEXT.finditer(text):
        raw = m.group(2)
        args = None
        if raw:
            try:
                args = json.loads(raw)
            except ValueError:
                args = None
        out.append((m.group(1), args if raw else {}, m.group(0)))
    return out


DEFAULT_BINARY = os.path.join(ROOT, "native", "target", "release", "hoi4-live-native")


def from_binary(binary=DEFAULT_BINARY):
    """The authoritative catalogue: `hoi4-live-native --list-tools` (no game or session needed)."""
    import subprocess
    out = subprocess.run([binary, "--list-tools"], capture_output=True, text=True, timeout=60, check=True).stdout
    tools = {}
    for t in json.loads(out)["tools"]:
        tools.setdefault(t["name"], {"description": t.get("description", ""),
                                     "inputSchema": t.get("inputSchema") or {}, "source": "--list-tools"})
    return dict(sorted(tools.items()))


def load(path=DEFAULT_OUT):
    with open(path, encoding="utf-8") as f:
        return json.load(f)["tools"]


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    sub = ap.add_subparsers(dest="cmd", required=True)
    b = sub.add_parser("build")
    b.add_argument("--src", default=DEFAULT_SRC)
    b.add_argument("--binary", default=DEFAULT_BINARY,
                   help="native binary to ask (--list-tools); the source scan is used when it is missing")
    b.add_argument("--out", default=DEFAULT_OUT)
    sh = sub.add_parser("show")
    sh.add_argument("tool")
    sh.add_argument("--catalogue", default=DEFAULT_OUT)
    c = sub.add_parser("check-call")
    c.add_argument("tool")
    c.add_argument("args", nargs="?", default="{}")
    c.add_argument("--catalogue", default=DEFAULT_OUT)
    a = ap.parse_args(argv)
    if a.cmd == "build":
        tools = from_binary(a.binary) if os.path.exists(a.binary) else extract(a.src)
        with open(a.out, "w", encoding="utf-8") as f:
            json.dump({"_generated_by": "scripts/hoi4facts/tool_schemas.py build", "tools": tools}, f,
                      indent=1, ensure_ascii=False, sort_keys=True)
            f.write("\n")
        print("%d tools -> %s" % (len(tools), os.path.relpath(a.out, ROOT)))
        return 0
    if a.cmd == "show":
        tools = load(a.catalogue)
        if a.tool not in tools:
            close = [t for t in tools if a.tool.replace("hoi4_", "") in t]
            print("unknown tool %s; similar: %s" % (a.tool, ", ".join(close[:10])))
            return 1
        t = tools[a.tool]
        schema = t.get("inputSchema") or {}
        req = [r for r in schema.get("required") or [] if r not in BROKER_ARGS]
        args = {k: v for k, v in (schema.get("properties") or {}).items() if k not in BROKER_ARGS}
        print("%s  (required: %s)\n  args: %s\n  %s" % (a.tool, ", ".join(req) or "none",
              json.dumps(args, ensure_ascii=False), t.get("description", "")[:600]))
        return 0
    problems = check_call(load(a.catalogue), a.tool, json.loads(a.args))
    print("\n".join(problems) or "ok")
    return 1 if problems else 0


if __name__ == "__main__":
    sys.exit(main())
