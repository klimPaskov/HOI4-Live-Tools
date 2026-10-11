"""Generate docs/tools.md from the live registry, so the tool reference never drifts from the code."""

import json
import os

from .config import ROOT
from .tools import registry

FAMILIES = [("session", "Session and safety"), ("lowlevel", "Screen and input"), ("testmode", "Test mode"),
            ("time", "Time"), ("popups", "Popups"),
            ("understanding", "Understanding"), ("campaign", "Campaign actions (politics and economy)"),
            ("military", "Campaign actions (military)"), ("campaign_loop", "Campaign loop")]


def _param_rows(schema):
    req = set(schema.get("required", []))
    rows = []
    for name, p in schema.get("properties", {}).items():
        t = p.get("type", "")
        if "enum" in p:
            t = " \\| ".join(str(e) for e in p["enum"])
        elif t == "array":
            t = f"array of {p.get('items', {}).get('type', 'any')}"
        default = json.dumps(p["default"]) if "default" in p else ""
        desc = p.get("description", "")
        rows.append(f"| `{name}` | {t} | {'yes' if name in req else ''} | {default} | {desc} |")
    return rows


def write_tools_md(path=None):
    path = path or os.path.join(ROOT, "docs", "tools.md")
    tools = registry.all_tools()
    out = ["# Tool reference", "",
           "Generated from the server's registry (`python -m hoi4_live docs`). Coordinates are game client pixels; "
           "`lock` says whether the tool takes the shared desktop lock (`acting`: for the whole call, `self`: only "
           "for its acting part, `none`: read-only). Test-only tools are refused in campaign mode.", ""]
    for fam, title in FAMILIES:
        group = sorted([t for t in tools if t.family == fam], key=lambda t: t.name)
        if not group:
            continue
        out += [f"## {title}", ""]
        for t in group:
            flags = [f"lock: {t.lock}"]
            if t.test_only:
                flags.append("test mode only")
            out += [f"### `{t.name}`", "", t.description, "", "_" + ", ".join(flags) + "_", ""]
            rows = _param_rows(t.schema)
            if rows:
                out += ["| argument | type | required | default | description |", "|---|---|---|---|---|"] + rows + [""]
            for ex in t.examples:
                note = f" ({ex['note']})" if ex.get("note") else ""
                out.append(f"- `{t.name} {json.dumps(ex.get('args', {}))}`{note}")
            out.append("")
    if registry.PLANNED:
        out += ["## Planned (later phases)", ""]
        for phase, names in sorted(registry.PLANNED.items()):
            out.append(f"- Phase {phase}: " + ", ".join(f"`{n}`" for n in names))
        out.append("")
    with open(path, "w", encoding="utf-8", newline="\n") as f:
        f.write("\n".join(out))
    return path
