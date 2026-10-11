"""Phase 2 supplementary live checks through the MCP server: a war (telemetry war, front, lost-state and event
lines; the briefing's war section; hoi4_map front), map views and modes, then the QA save is loaded again so the
game is left as the exit check found it.

    "C:/Program Files/Python39/python.exe" scripts/phase2_extra.py

The war is staged with console effects (test mode): the USA declares war on Mexico, Texas is handed to Mexican
control, eight days run, then a white peace. Evidence: docs/evidence/phase2/extra/ (report.md, results.json,
images/).
"""

import json
import os
import shutil
import sys
import time

from PIL import Image

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)
sys.path.insert(0, HERE)
from mcp_client import McpClient  # noqa: E402

OUT = os.path.join(ROOT, "docs", "evidence", "phase2", "extra")

STEPS = [
    ("lock", "hoi4_lock", {"action": "acquire", "purpose": "phase 2 supplementary checks"}),
    ("map modes from the button tooltips", "hoi4_map", {"action": "modes", "recalibrate": True}),
    ("capital, country zoom, default mode", "hoi4_map", {"action": "view", "mode": "default", "target": "capital",
                                                         "zoom": "country", "max_width": 1280}),
    ("Texas, region zoom, terrain mode", "hoi4_map", {"action": "view", "mode": "terrain", "target": "Texas",
                                                      "zoom": "region", "max_width": 1280}),
    ("what is at the screen centre (terrain mode)", "hoi4_map", {"action": "at", "x": 960, "y": 540}),
    ("supply mode, same view", "hoi4_map", {"action": "view", "mode": "supply", "max_width": 1280}),
    ("back to the default mode", "hoi4_map", {"action": "view", "mode": "default", "max_width": 640}),
    ("stage a war: USA declares war on Mexico", "hoi4_eval_effect",
     {"effect": "declare_war_on = { target = MEX type = annex_everything }"}),
    ("stage a lost state: Texas under Mexican control", "hoi4_eval_effect",
     {"effect": "375 = { set_state_controller_to = MEX }", "scope": "USA"}),
    ("run 8 days (a weekly tick for the front lines)", "hoi4_time",
     {"action": "step", "days": 8, "speed": 3, "popups": "first"}),
    ("telemetry: wars, fronts, lost states, events", "hoi4_telemetry",
     {"action": "read", "kinds": ["day", "war", "front", "lost", "wld", "ev"]}),
    ("hoi4_state diplomacy and world (telemetry only)", "hoi4_state",
     {"sections": ["overview", "diplomacy", "world"], "save": "none"}),
    ("briefing at war", "hoi4_briefing", {"save": "none"}),
    ("front overview with a view of the front", "hoi4_map", {"action": "front", "zoom": "region", "max_width": 1280}),
    ("white peace with Mexico", "hoi4_eval_effect", {"effect": "white_peace = { tag = MEX }"}),
    ("run 1 day", "hoi4_time", {"action": "step", "days": 1, "speed": 3, "popups": "first"}),
    ("telemetry events after the peace", "hoi4_telemetry", {"action": "read", "raw": True, "kinds": ["ev"], "limit": 10}),
    ("suspect error.log lines", "hoi4_telemetry", {"action": "status"}),
    ("reload the QA save (leave the game as found)", "hoi4_load", {"save": "qa_001_fresh_head"}),
    ("run 4 days as the exit check did", "hoi4_time", {"action": "step", "days": 4, "speed": 3, "popups": "first"}),
    ("map back to the capital", "hoi4_map", {"action": "view", "mode": "default", "target": "capital", "zoom": "country",
                                             "max_width": 960}),
    ("final screen", "hoi4_screen", {"screenshot": True}),
    ("release", "hoi4_lock", {"action": "release"}),
]


def main():
    shutil.rmtree(OUT, ignore_errors=True)
    os.makedirs(os.path.join(OUT, "images"), exist_ok=True)
    raw = os.path.join(OUT, "raw")
    rows = []
    with McpClient(image_dir=raw, transcript=os.path.join(OUT, "transcript.jsonl")) as c:
        c.initialize()
        for label, tool, args in STEPS:
            t0 = time.time()
            res = c.call(tool, args)
            text = "\n".join(x["text"] for x in res["content"] if x["type"] == "text")
            imgs = []
            for x in res["content"]:
                if x["type"] == "image" and x.get("file"):
                    name = f"{len(rows) + 1:02d}_{tool}.jpg"
                    im = Image.open(x["file"]).convert("RGB")
                    if im.width > 1280:
                        im = im.resize((1280, round(im.height * 1280 / im.width)), Image.LANCZOS)
                    im.save(os.path.join(OUT, "images", name), "JPEG", quality=85)
                    imgs.append("images/" + name)
            ok = not res.get("isError")
            rows.append({"label": label, "tool": tool, "args": args, "ok": ok, "seconds": round(time.time() - t0, 1),
                         "text": text, "data": res.get("structuredContent"), "images": imgs})
            print(f"[{'ok' if ok else 'FAIL'}] {label}: {text.splitlines()[0][:150] if text else ''}", flush=True)
    shutil.rmtree(raw, ignore_errors=True)
    with open(os.path.join(OUT, "results.json"), "w", encoding="utf-8") as f:
        json.dump(rows, f, indent=1, default=str)
    L = ["# Phase 2 supplementary checks", "", "Driver: `scripts/phase2_extra.py` (MCP stdio). " +
         ("All steps succeeded." if all(r["ok"] for r in rows) else "Some steps FAILED."), "",
         "| # | step | tool | ok | s | summary |", "|---|---|---|---|---|---|"]
    for i, r in enumerate(rows, 1):
        summ = (r["text"].splitlines()[0] if r["text"] else "").replace("|", "/")[:150]
        L.append(f"| {i} | {r['label']} | `{r['tool']}` | {'yes' if r['ok'] else 'NO'} | {r['seconds']} | {summ} |")
    for i, r in enumerate(rows, 1):
        if r["tool"] in ("hoi4_briefing",):
            L += ["", f"## Step {i}: {r['label']}", "", "```markdown", r["data"].get("markdown", ""), "```"]
        if r["tool"] == "hoi4_map" and (r["data"] or {}).get("front"):
            L += ["", f"## Step {i}: {r['label']}", "", "```json", json.dumps(r["data"]["front"], indent=1)[:3000], "```"]
        if r["tool"] == "hoi4_telemetry" and r["args"].get("action") == "read":
            L += ["", f"## Step {i}: {r['label']}", "", "```json", json.dumps(r["data"], indent=1, default=str)[:4000], "```"]
        for img in r["images"]:
            L += ["", f"Step {i} ({r['label']}): ![step {i}]({img})"]
    with open(os.path.join(OUT, "report.md"), "w", encoding="utf-8") as f:
        f.write("\n".join(L) + "\n")
    return 0 if all(r["ok"] for r in rows) else 1


if __name__ == "__main__":
    sys.exit(main())
