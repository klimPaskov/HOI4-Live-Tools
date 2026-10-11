"""Phase 3 supplementary live checks through the MCP server: the paths the exit check does not need (a focus whose
requirements are unmet, the game's refusal coming back as an error; a production line added, switched to other
equipment and removed; laws' choice list; all politics rows; decision details; diplomacy view and actions with a
blocked one; construction of two levels of infrastructure; filtered research list). From the QA save in
campaign mode; the QA save is loaded again at the end.

    "C:/Program Files/Python39/python.exe" scripts/phase3_extra.py

Evidence: docs/evidence/phase3/extra/ (report.md, results.json, images/).
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

OUT = os.path.join(ROOT, "docs", "evidence", "phase3", "extra")
SAVE = "qa_001_fresh_head"


def main():
    shutil.rmtree(OUT, ignore_errors=True)
    os.makedirs(OUT)
    raw = os.path.join(OUT, "raw")
    steps = []
    with McpClient(image_dir=raw) as c:
        c.initialize()

        def step(label, tool, targs, expect="ok", check=None):
            t0 = time.time()
            res = c.call(tool, targs, timeout=2400)
            text = "\n".join(x["text"] for x in res["content"] if x["type"] == "text")
            data = res.get("structuredContent") or {}
            err = bool(res.get("isError"))
            ok = (not err) if expect == "ok" else err
            note = ""
            if ok and check:
                ok, note = check(data, text)
            imgs = [x["file"] for x in res["content"] if x["type"] == "image" and x.get("file")]
            steps.append({"label": label, "tool": tool, "args": targs, "expect": expect, "ok": bool(ok),
                          "seconds": round(time.time() - t0, 1), "summary": text.splitlines()[0] if text else "",
                          "note": note, "data": data, "images": imgs})
            print(f"[{'ok' if ok else 'FAIL'}] {label}: {text.splitlines()[0][:170] if text else ''} {note}", flush=True)
            return data

        step("lock", "hoi4_lock", {"action": "acquire", "purpose": "phase 3 supplementary checks"})
        step("test mode for the setup", "hoi4_lock", {"action": "set_mode", "mode": "test"})
        step(f"load {SAVE}", "hoi4_load", {"save": SAVE}, check=lambda d, t: (d.get("date_matches") is True, ""))
        step("campaign mode", "hoi4_lock", {"action": "set_mode", "mode": "campaign"})
        step("details of a focus with an unmet prerequisite", "hoi4_focus",
             {"action": "details", "focus": "Air War Plans Division", "save": SAVE},
             check=lambda d, t: (bool(d.get("detail", {}).get("unmet")), f"unmet {d.get('detail', {}).get('unmet')}"))
        step("starting it is refused with the game's reason", "hoi4_focus",
             {"action": "select", "focus": "Air War Plans Division", "save": SAVE}, expect="error",
             check=lambda d, t: ("cannot be started" in t, t.splitlines()[0][:120]))
        step("all politics rows (laws, advisors, designers, theorist, military staff)", "hoi4_politics",
             {"rows": "all", "save": SAVE}, check=lambda d, t: (len(d.get("slots", [])) >= 12, f"{len(d.get('slots', []))} slots"))
        step("economy law choices with costs", "hoi4_politics", {"action": "options", "slot": "economy"},
             check=lambda d, t: (len(d.get("choices", [])) >= 3, t.splitlines()[0][:150]))
        step("a law the country cannot afford is refused", "hoi4_politics",
             {"action": "change_law", "law": "war_economy"}, expect="error",
             check=lambda d, t: ("cannot be chosen" in t or "not in the" in t, t.splitlines()[0][:120]))
        step("decision details (tooltip)", "hoi4_decisions", {"action": "details", "decision": "USA_research_grants"},
             check=lambda d, t: (len((d.get("tooltip") or {}).get("lines", [])) > 2, t.splitlines()[0][:150]))
        step("research list, industry only", "hoi4_research",
             {"action": "list", "folder": "industry_folder", "save": SAVE},
             check=lambda d, t: (len(d.get("technologies", [])) > 0, t.splitlines()[0][:150]))
        add = step("production: add a line", "hoi4_production", {"action": "add", "equipment": "Support Equipment",
                                                                 "category": "land", "factories": 2},
                   check=lambda d, t: (d.get("changed") is True and (d.get("line") or {}).get("factories") == 2,
                                       f"line {(d.get('line') or {}).get('number')} x{(d.get('line') or {}).get('factories')}"))
        n = str((add.get("line") or {}).get("number") or "")
        if n:
            step("production: switch that line to other equipment", "hoi4_production",
                 {"action": "change", "line": n, "equipment": "infantry_equipment_1"},
                 check=lambda d, t: (d.get("changed") is True, t.splitlines()[0][:150]))
            step("production: remove the line", "hoi4_production", {"action": "remove", "line": n},
                 check=lambda d, t: (d.get("removed") is True, t.splitlines()[0][:150]))
        step("construction: two levels of infrastructure in Texas", "hoi4_construction",
             {"action": "build", "building": "infrastructure", "state": "Texas", "count": 2},
             check=lambda d, t: (d.get("changed") is True, t.splitlines()[0][:150]))
        step("diplomacy view of Canada", "hoi4_diplomacy", {"action": "view", "country": "CAN"},
             check=lambda d, t: (len(d.get("actions", [])) >= 10, t.splitlines()[0][:150]))
        step("a blocked action returns its tooltip", "hoi4_diplomacy",
             {"action": "act", "country": "CAN", "do": "military_access"}, expect="error",
             check=lambda d, t: ("not possible" in t, t.splitlines()[0][:150]))
        step("declaring war needs confirm_war", "hoi4_diplomacy", {"action": "act", "country": "MEX", "do": "declare_war"},
             expect="error", check=lambda d, t: ("confirm_war" in t, ""))
        step("test mode to restore", "hoi4_lock", {"action": "set_mode", "mode": "test"})
        step(f"restore: load {SAVE}", "hoi4_load", {"save": SAVE}, check=lambda d, t: (d.get("date_matches") is True, ""))
        step("paused on the map", "hoi4_time", {"action": "status"}, check=lambda d, t: (d.get("running") is False, d.get("text")))
        step("release the lock", "hoi4_lock", {"action": "release"})
    img_dir = os.path.join(OUT, "images")
    os.makedirs(img_dir, exist_ok=True)
    for i, s in enumerate(steps, 1):
        out = []
        for p in s["images"]:
            im = Image.open(p).convert("RGB")
            if im.width > 1280:
                im = im.resize((1280, round(im.height * 1280 / im.width)))
            dst = os.path.join(img_dir, f"step{i:02d}_{os.path.splitext(os.path.basename(p))[0]}.jpg")
            im.save(dst, "JPEG", quality=82)
            out.append(os.path.relpath(dst, OUT).replace("\\", "/"))
        s["images"] = out
    shutil.rmtree(raw, ignore_errors=True)
    ok = all(s["ok"] for s in steps)
    with open(os.path.join(OUT, "results.json"), "w", encoding="utf-8") as f:
        json.dump({"ok": ok, "steps": steps}, f, indent=1, default=str)
    L = [f"# Phase 3 supplementary checks: {'all as expected' if ok else 'FAILURES'}", "",
         "Driver `scripts/phase3_extra.py`, through the MCP server, campaign mode after loading `" + SAVE + "`. "
         "`expect` error means the tool must refuse (with the game's reason).", "",
         "| # | step | tool | expect | result | s | detail |", "|---|---|---|---|---|---|---|"]
    for i, s in enumerate(steps, 1):
        L.append(f"| {i} | {s['label']} | `{s['tool']}` | {s['expect']} | {'as expected' if s['ok'] else '**NO**'} | "
                 f"{s['seconds']} | {(s['note'] or s['summary']).replace('|', '/')[:170]} |")
    L += ["", "## Screens", ""]
    for i, s in enumerate(steps, 1):
        for img in s["images"]:
            L.append(f"- step {i} ({s['label']}): ![step {i}]({img})")
    with open(os.path.join(OUT, "report.md"), "w", encoding="utf-8") as f:
        f.write("\n".join(L) + "\n")
    print("supplementary checks", "OK" if ok else "FAILED", flush=True)
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())
