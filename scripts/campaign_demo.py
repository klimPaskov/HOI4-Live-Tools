"""Phase 5 example run: a small rule-based agent plays a country through the MCP stdio server with the campaign loop.

    "C:/Program Files/Python39/python.exe" scripts/campaign_demo.py --days 120 [--tag USA] [--save qa_001_fresh_head]

An LLM agent would decide what the rules below decide: it follows skills/hoi4-live-campaign/SKILL.md. The loop is the one the playbook
describes: briefing (what needs a decision) -> act on each attention item with the campaign tools -> write the plan
and the reasons to the journal -> hoi4_advance (time runs until something needs the agent) -> repeat. The rules are
deliberately simple (cheapest research, a production line for the equipment the army lacks, the first free
construction slot, the first focus without prerequisites); the point is that the loop, the stop conditions, the
popup policy and the journal work together over many in-game months. The run is written to
docs/evidence/phase5/ (run_report_<TAG>.md from hoi4_journal action=report, steps.json, transcript.jsonl).

Test mode is used only to load the start save; everything after that is campaign mode (no console).
"""

import argparse
import json
import os
import sys
import time

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)
sys.path.insert(0, HERE)
from mcp_client import McpClient  # noqa: E402

EVIDENCE = os.path.join(ROOT, "docs", "evidence", "phase5")
PREFS = {
    "USA": {"states": ["Pennsylvania", "Michigan", "Illinois", "Ohio", "California"], "building": "industrial_complex",
            "equipment": "Infantry Equipment"},
    "MEX": {"states": ["Distrito Federal", "Nuevo Leon", "Jalisco"], "building": "industrial_complex",
            "equipment": "Infantry Equipment"},
}
MAX_ACTIONS_PER_ROUND = 8


class Agent:
    def __init__(self, client, tag, days, steps_path):
        self.c = client
        self.tag = tag
        self.days = days
        self.prefs = PREFS.get(tag, PREFS["USA"])
        self.steps = []
        self.steps_path = steps_path

    def call(self, tool, args=None, note=""):
        t0 = time.time()
        try:
            res = self.c.call(tool, args or {}, timeout=3000)
        except Exception as e:
            res = {"content": [{"type": "text", "text": f"{type(e).__name__}: {e}"}], "isError": True}
        text = "\n".join(x["text"] for x in res["content"] if x["type"] == "text")
        rec = {"tool": tool, "args": args or {}, "ok": not res.get("isError"), "seconds": round(time.time() - t0, 1),
               "summary": text.splitlines()[0][:220] if text else "", "note": note}
        self.steps.append(rec)
        print(f"[{'ok' if rec['ok'] else 'FAIL'}] {tool} {json.dumps(args or {})[:110]} -> {rec['summary'][:150]} "
              f"({rec['seconds']} s)", flush=True)
        with open(self.steps_path, "w", encoding="utf-8") as f:
            json.dump(self.steps, f, indent=1)
        return res.get("structuredContent") or {}, rec

    def note(self, kind, text):
        self.call("hoi4_journal", {"action": "add", "kind": kind, "text": text})

    # ------------------------------------------------------------------ the rules
    def act(self, attention):
        done = 0
        for item in attention:
            if done >= MAX_ACTIONS_PER_ROUND:
                break
            code = item.get("code")
            handler = getattr(self, f"on_{code}", None)
            if handler and handler(item):
                done += 1
        return done

    def on_no_focus(self, item):
        d, _ = self.call("hoi4_focus", {"tag": self.tag, "limit": 60})
        rows = d.get("focuses", [])
        # the cheapest political-power focus first (it pays for the first advisor), then the rest as listed
        rows.sort(key=lambda r: (0 if "add_political_power" in (r.get("effects") or "") else 1, r.get("days") or 999))
        for pick in rows[:3]:
            r, rec = self.call("hoi4_focus", {"tag": self.tag, "action": "select", "focus": pick["id"],
                                              "replace": True})
            if rec["ok"]:
                self.note("decision", f"Focus {pick['name']}: {pick.get('days')} days, "
                                      f"{'pays political power' if 'add_political_power' in (pick.get('effects') or '') else 'first available'}.")
                return True
        return False

    def on_idle_research(self, item):
        slots, _ = self.call("hoi4_research", {"action": "slots"})
        idle = [s for s in slots.get("slots", []) if s.get("state") != "busy"]
        if not idle:
            return False
        d, _ = self.call("hoi4_research", {"tag": self.tag, "action": "list", "ahead": False, "limit": 120})
        techs = [t for t in d.get("technologies", []) if not t.get("allow") and not str(t["id"]).startswith("sp_")
                 and "chemical" not in t["folder"] and "biological" not in t["folder"]]
        techs.sort(key=lambda t: t.get("cost") or 9999)
        started, failures = 0, 0
        used_folders = set()
        for t in techs:
            if started >= len(idle) or failures >= 2:
                break
            if t["folder"] in used_folders:
                continue
            r, rec = self.call("hoi4_research", {"tag": self.tag, "action": "start", "tech": t["id"]})
            if rec["ok"] and r.get("changed") is False:      # the save was older than the slot: already running
                used_folders.add(t["folder"])
                continue
            if rec["ok"]:
                started += 1
                used_folders.add(t["folder"])
                self.note("decision", f"Research {t['name']} ({t['folder']}): cheapest available, one slot per folder.")
            else:
                failures += 1
        return started > 0

    def on_idle_production(self, item):
        r, rec = self.call("hoi4_production", {"tag": self.tag, "action": "add", "equipment": self.prefs["equipment"],
                                               "factories": max(1, min(3, item.get("free") or 1))})
        if rec["ok"]:
            self.note("decision", f"Production: {self.prefs['equipment']} on the idle military factories.")
        return rec["ok"]

    def on_idle_construction(self, item):
        for state in self.prefs["states"]:
            r, rec = self.call("hoi4_construction", {"action": "build", "building": self.prefs["building"],
                                                     "state": state})
            if rec["ok"]:
                self.note("decision", f"Construction: {self.prefs['building']} in {state}.")
                return True
        return False


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--tag", default="USA")
    ap.add_argument("--save", default="qa_001_fresh_head")
    ap.add_argument("--days", type=int, default=120)
    ap.add_argument("--advance", type=int, default=30, help="days per hoi4_advance call")
    ap.add_argument("--no-load", action="store_true", help="play the game as it is")
    args = ap.parse_args()
    os.makedirs(EVIDENCE, exist_ok=True)
    transcript = os.path.join(EVIDENCE, f"transcript_{args.tag}.jsonl")
    if os.path.exists(transcript):
        os.remove(transcript)
    started = time.time()
    with McpClient(image_dir=os.path.join(EVIDENCE, "raw_images"), transcript=transcript) as c:
        c.initialize()
        a = Agent(c, args.tag, args.days, os.path.join(EVIDENCE, f"steps_{args.tag}.json"))
        a.call("hoi4_lock", {"action": "acquire", "purpose": "phase 5 campaign demo"})
        if not args.no_load:
            a.call("hoi4_lock", {"action": "set_mode", "mode": "test"})
            a.call("hoi4_load", {"save": args.save})
        a.call("hoi4_lock", {"action": "set_mode", "mode": "campaign"})
        a.note("plan", f"Play {args.tag} for {args.days} days: keep research, production and construction busy, "
                       "take focuses, answer events with their first option unless a rule says otherwise.")
        rounds = 0
        run_days = 0.0
        while run_days < args.days:
            rounds += 1
            save_name = f"demo_{args.tag}"
            a.call("hoi4_save", {"name": save_name})
            brief, _ = a.call("hoi4_briefing", {"tag": args.tag, "save": f"hoi4live_{save_name}", "remember": True})
            attention = brief.get("attention", [])
            print(f"-- round {rounds}: {len(attention)} attention item(s): "
                  + "; ".join(i["text"][:60] for i in attention[:6]), flush=True)
            a.act(attention)
            adv, rec = a.call("hoi4_advance", {"days": min(args.advance, int(args.days - run_days) or 1),
                                               "popups": "choices", "chunk_days": 2,
                                               "stop_on": ["tech_done", "focus_done", "war_started", "war_ended",
                                                           "territory_lost", "losing", "low_stability"]})
            if adv.get("events_waiting"):
                seen_titles = set()
                for ev in adv["events_waiting"]:
                    if ev.get("title") in seen_titles:       # a stacked window can be detected twice
                        continue
                    seen_titles.add(ev.get("title"))
                    opts = [o.get("text") for o in ev.get("options", [])]
                    a.note("decision", f"Event '{ev.get('title')}': chose '{opts[0] if opts else '?'}' (first option; "
                                       f"options were {opts}).")
                a.call("hoi4_popups", {"action": "answer_all"})
            run_days += adv.get("days_run") or 0.0
            if not rec["ok"] and rounds > 3 and run_days == 0:
                print("the loop does not advance; stopping", flush=True)
                break
        res = c.call("hoi4_journal", {"action": "report"})
        text = "\n".join(x["text"] for x in res["content"] if x["type"] == "text").split("\n{\n")[0] + "\n"
        with open(os.path.join(EVIDENCE, f"run_report_{args.tag}.md"), "w", encoding="utf-8", newline="\n") as f:
            f.write(text)
        print(f"done: {run_days:.0f} days in {rounds} rounds, {round((time.time() - started) / 60, 1)} min", flush=True)


if __name__ == "__main__":
    main()
