"""Phase 1 exit check, driven through the MCP stdio server exactly as an agent would use it.

    "C:/Program Files/Python39/python.exe" scripts/exit_check_phase1.py [--save qa_001_fresh_head] [--event my_mod.1]

Steps: hold the lock, status, screenshot, OCR of the in-game date, load the save, mark the logs, fire an event
through the console, answer its popup, run 10 days answering every popup, save hoi4live_smoke, query that save
(player, date, a variable, a diff against the loaded save), read the error.log delta, confirm the game is paused,
release the lock.

Evidence goes to docs/evidence/phase1/: exit_check.md (report), results.json (structured results),
transcript.jsonl (every JSON-RPC message, images replaced by file names) and images/ (JPEG copies).
"""

import argparse
import datetime
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

EVIDENCE = os.path.join(ROOT, "docs", "evidence", "phase1")


def to_jpeg(src, dst_dir, name, max_width=960):
    os.makedirs(dst_dir, exist_ok=True)
    im = Image.open(src).convert("RGB")
    if im.width > max_width:
        im = im.resize((max_width, round(im.height * max_width / im.width)), Image.LANCZOS)
    dst = os.path.join(dst_dir, name + ".jpg")
    im.save(dst, "JPEG", quality=80)
    return os.path.relpath(dst, EVIDENCE).replace("\\", "/")


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--save", default="qa_001_fresh_head")
    ap.add_argument("--event", default="my_mod.1")
    ap.add_argument("--tag", default="USA")
    ap.add_argument("--days", type=int, default=10)
    ap.add_argument("--variable", default="communism_controlled_states")
    args = ap.parse_args()

    if os.path.isdir(EVIDENCE):
        for name in ("images", "raw_images"):
            shutil.rmtree(os.path.join(EVIDENCE, name), ignore_errors=True)
        for name in ("transcript.jsonl", "results.json", "exit_check.md"):
            if os.path.exists(os.path.join(EVIDENCE, name)):
                os.remove(os.path.join(EVIDENCE, name))
    os.makedirs(EVIDENCE, exist_ok=True)
    raw_images = os.path.join(EVIDENCE, "raw_images")
    transcript = os.path.join(EVIDENCE, "transcript.jsonl")
    steps = []
    started = time.time()

    with McpClient(image_dir=raw_images, transcript=transcript) as c:
        init = c.initialize()
        tools = c.request("tools/list")["result"]["tools"]

        def step(label, tool, targs=None, check=None):
            t0 = time.time()
            n_before = c.image_count
            res = c.call(tool, targs or {})
            text = "\n".join(x["text"] for x in res["content"] if x["type"] == "text")
            data = res.get("structuredContent") or {}
            ok = not res.get("isError")
            note = ""
            if ok and check:
                ok, note = check(data, text)
            imgs = [x["file"] for x in res["content"] if x["type"] == "image" and x.get("file")]
            rec = {"label": label, "tool": tool, "args": targs or {}, "ok": ok, "seconds": round(time.time() - t0, 1),
                   "summary": text.splitlines()[0] if text else "", "note": note, "data": data, "images": imgs}
            steps.append(rec)
            print(f"[{'ok' if ok else 'FAIL'}] {label}: {rec['summary']} ({rec['seconds']} s) {note}", flush=True)
            return rec

        step("acquire the desktop lock", "hoi4_lock", {"action": "acquire", "purpose": "phase 1 exit check"})
        st = step("status", "hoi4_status", {"screenshot": True},
                  lambda d, t: (d.get("game", {}).get("running") is True, f"screen={d.get('screen')}"))
        step("screenshot", "hoi4_screenshot", {"name": "exit_check_screenshot", "max_width": 960})
        w = (st["data"].get("window") or {}).get("client", [0, 0, 1920, 1080])[2]
        step("OCR of the in-game date (white-text filter)", "hoi4_ocr",
             {"region": [w - 300, 6, 170, 28], "preprocess": "white_text"},
             lambda d, t: (bool(d.get("text")), f"read '{d.get('text')}'"))
        step("time status (date OCR + pause icon)", "hoi4_time", {"action": "status"})
        load = step(f"load {args.save}", "hoi4_load", {"save": args.save},
                    lambda d, t: (d.get("date_matches") is True,
                                  f"map {d.get('date_on_screen')} vs save {d.get('save_date')}"))
        if not load["ok"]:
            print("load failed; stopping", flush=True)
        else:
            step("mark the logs", "hoi4_logs", {"set_mark": "exit_check"})
            step(f"fire {args.event} {args.tag} (console) and read the popup", "hoi4_fire_event",
                 {"event": args.event, "tag": args.tag},
                 lambda d, t: (len(d.get("popups", [])) >= 1,
                               "popups: " + "; ".join(f"{p.get('kind')} '{p.get('title')}'" for p in d.get("popups", []))))
            step("answer the event popup", "hoi4_popups", {"action": "answer", "screenshot": True},
                 lambda d, t: (d.get("closed") is True, f"'{d.get('title')}' -> '{d.get('chosen_text')}'"))
            run = step(f"run {args.days} days, answering every popup", "hoi4_time",
                       {"action": "step", "days": args.days, "speed": 5, "popups": "first"},
                       lambda d, t: (d.get("elapsed_hours", 0) >= args.days * 24 - 1 and
                                     all(p.get("closed") for p in d.get("popups", []) if p.get("answered")),
                                     f"{d.get('start')} -> {d.get('end')}, {len(d.get('popups', []))} popup(s)"))
            step("save hoi4live_smoke", "hoi4_save", {"name": "smoke"},
                 lambda d, t: (d.get("name") == "hoi4live_smoke", f"{d.get('player')} {d.get('date')} {d.get('size_mb')} MB"))
            step("query the save: player and date", "hoi4_save_query", {"save": "hoi4live_smoke", "query": "info"},
                 lambda d, t: (d.get("info", {}).get("player") == args.tag,
                               f"player {d.get('info', {}).get('player')}, date {d.get('info', {}).get('date')}"))
            step(f"query the save: {args.tag} variable {args.variable}", "hoi4_save_query",
                 {"save": "hoi4live_smoke", "query": "variables", "tag": args.tag, "pattern": "^" + args.variable + "$"},
                 lambda d, t: (args.variable in d.get("values", {}),
                               f"{args.variable} = {d.get('values', {}).get(args.variable)}"))
            step(f"diff {args.save} -> hoi4live_smoke (communism variables)", "hoi4_save_query",
                 {"save": args.save, "other": "hoi4live_smoke", "query": "diff",
                  "path": f"countries/{args.tag}/variables", "pattern": "communism", "limit": 40},
                 lambda d, t: (True, f"counts {d.get('counts')}"))
            step("error.log delta since the mark", "hoi4_logs", {"log": "error", "since": "exit_check", "limit": 60})
            step("game left paused", "hoi4_time", {"action": "status"},
                 lambda d, t: (d.get("running") is False, f"{d.get('text')}"))
        step("final status", "hoi4_status", {"screenshot": True})
        step("release the desktop lock", "hoi4_lock", {"action": "release"},
             lambda d, t: (not (d.get("lock") or {}).get("ours"), "released"))
        progress_notes = [n for n in c.notifications if n.get("method") == "notifications/progress"]

    # images: JPEG copies for the repository, raw PNGs dropped
    img_dir = os.path.join(EVIDENCE, "images")
    for i, s in enumerate(steps):
        s["images"] = [to_jpeg(p, img_dir, f"step{i + 1:02d}_{os.path.splitext(os.path.basename(p))[0]}") for p in s["images"]]
        shots = []
        for key in ("screenshot",):
            if isinstance(s["data"].get(key), str) and os.path.exists(s["data"][key]):
                shots.append(s["data"][key])
        for p in s["data"].get("popups", []) if isinstance(s["data"].get("popups"), list) else []:
            if isinstance(p, dict) and p.get("screenshot") and os.path.exists(p["screenshot"]):
                shots.append(p["screenshot"])
        s["popup_images"] = [to_jpeg(p, img_dir, f"step{i + 1:02d}_{os.path.splitext(os.path.basename(p))[0]}", 700)
                             for p in shots]
    shutil.rmtree(raw_images, ignore_errors=True)

    ok = all(s["ok"] for s in steps)
    with open(os.path.join(EVIDENCE, "results.json"), "w", encoding="utf-8") as f:
        json.dump({"ok": ok, "server": init.get("serverInfo"), "protocol": init.get("protocolVersion"),
                   "tools": len(tools), "progress_notifications": len(progress_notes), "steps": steps}, f, indent=1,
                  default=str)
    write_report(ok, init, tools, steps, progress_notes, time.time() - started, args)
    print("exit check", "PASS" if ok else "FAIL", flush=True)
    return 0 if ok else 1


def write_report(ok, init, tools, steps, progress, seconds, args):
    now = datetime.datetime.now().strftime("%Y-%m-%d %H:%M")
    L = [f"# Phase 1 exit check: {'PASS' if ok else 'FAIL'}", "",
         f"Run {now}, {round(seconds)} s, through the MCP stdio server ({init.get('serverInfo', {}).get('name')} "
         f"{init.get('serverInfo', {}).get('version')}, protocol {init.get('protocolVersion')}, {len(tools)} tools, "
         f"{len(progress)} progress notifications). Driver: `scripts/exit_check_phase1.py`. Every call below is a "
         "`tools/call` request; `transcript.jsonl` holds the full JSON-RPC exchange and `results.json` the "
         "structured results.", "",
         "| # | step | tool | result | s | detail |", "|---|---|---|---|---|---|"]
    for i, s in enumerate(steps, 1):
        detail = (s["note"] or s["summary"]).replace("|", "/")[:150]
        L.append(f"| {i} | {s['label']} | `{s['tool']}` | {'ok' if s['ok'] else '**FAIL**'} | {s['seconds']} | {detail} |")
    L += ["", "## Evidence", ""]
    for i, s in enumerate(steps, 1):
        d = s["data"]
        if s["tool"] == "hoi4_time" and d.get("popups") is not None and "end" in d:
            L += [f"### Step {i}: {s['label']}", "",
                  f"From {d.get('start')} to {d.get('end')} ({d.get('elapsed_hours')} h, pause_in_hours armed: "
                  f"{d.get('armed_pause_in_hours')}, {d.get('wall_seconds')} s wall time).", ""]
            for p in d.get("popups", []):
                L.append(f"- {p.get('kind')} **{p.get('title')}**: answered '{p.get('chosen_text')}' "
                         f"(options: {', '.join(repr(o) for o in p.get('options', []))}); closed: {p.get('closed')}")
            L.append("")
        if s["tool"] == "hoi4_fire_event":
            L += [f"### Step {i}: {s['label']}", "", f"Console: `{d.get('command')}` -> {d.get('response')}", ""]
            for p in d.get("popups", []):
                L.append(f"- {p.get('kind')} **{p.get('title')}**: {p.get('text', '')[:300]}")
                L.append(f"  - options: {[o.get('text') for o in p.get('options', [])]}")
            L.append("")
        if s["tool"] == "hoi4_save_query":
            body = {k: v for k, v in d.items() if k not in ("elapsed_s",)}
            L += [f"### Step {i}: {s['label']}", "", "```json", json.dumps(body, indent=1, default=str)[:2500], "```", ""]
        if s["tool"] == "hoi4_logs" and "info" in d:
            L += [f"### Step {i}: {s['label']}", "", f"{d['info'].get('lines')} new line(s), "
                  f"{d['info'].get('unique')} unique:", ""]
            for row in d.get("lines", [])[:40]:
                cnt = f" (x{row['count']})" if row.get("count") else ""
                L.append(f"- `{row['line'][:220]}`{cnt}")
            L.append("")
        for img in s["images"] + s.get("popup_images", []):
            L.append(f"![step {i}]({img})")
        if s["images"] or s.get("popup_images"):
            L.append("")
    with open(os.path.join(EVIDENCE, "exit_check.md"), "w", encoding="utf-8") as f:
        f.write("\n".join(L) + "\n")


if __name__ == "__main__":
    sys.exit(main())
