"""The segment review: at a segment boundary, one strong-model call reads what the segment left behind
(RUN/ISSUES.md, the last gate packet, the lessons this run wrote) and proposes plan and executor-rule
changes for the engineer to judge. Built by code, bounded in size, billed to the key through answer.py
(budget checked first, the call in the ledger under the role `segment_review`).

usage: python -m hoi4_live.observe.segment_review RUN_DIR [--dry-run]
Writes RUN/segment-review-<date>.md (the packet's sources, the answer, tokens and cost) and one row in
RUN/reviews.jsonl. --dry-run writes the packet only and asks nothing.
"""
from __future__ import annotations

import argparse
import json
import os
import sys
import time

from . import answer
from .lessons import LessonStore, store_dir

MAX_ISSUES_CHARS = 14000
MAX_GATE_CHARS = 6000
MAX_LESSONS = 25


def _read(path, limit):
    try:
        with open(path, encoding="utf-8") as f:
            text = f.read()
    except OSError:
        return ""
    return text if len(text) <= limit else text[:limit] + f"\n[... cut at {limit} of {len(text)} chars]\n"


def run_lessons(run_dir, store=None):
    """The lessons whose evidence names this run, newest first."""
    name = os.path.basename(os.path.abspath(run_dir))
    rows = LessonStore(store or store_dir(run_dir)).all()
    mine = [le for le in rows if any(str(e).split(" ", 1)[0] == name for e in le.get("evidence") or [])]
    mine.sort(key=lambda le: str(le.get("updated") or le.get("created") or ""), reverse=True)
    return mine[:MAX_LESSONS]


def build_packet(run_dir, store=None):
    lines = [f"SEGMENT REVIEW of run {os.path.basename(os.path.abspath(run_dir))}", ""]
    issues = _read(os.path.join(run_dir, "ISSUES.md"), MAX_ISSUES_CHARS)
    lines += ["## ISSUES.md", issues or "(none written)", ""]
    gate = _read(os.path.join(run_dir, "GATE.md"), MAX_GATE_CHARS)
    lines += ["## The last gate packet (GATE.md)", gate or "(none written)", ""]
    lessons = run_lessons(run_dir, store)
    lines.append(f"## Lessons this run wrote ({len(lessons)})")
    for le in lessons:
        lines.append(f"- {le.get('id')} (confidence {float(le.get('confidence') or 0):.2f}, outcome "
                     f"{le.get('outcome') or '-'}): {str(le.get('statement') or '')[:300]}")
    if not lessons:
        lines.append("(none)")
    return "\n".join(lines) + "\n"


def review(run_dir, dry_run=False, store=None, ask=None):
    packet = build_packet(run_dir, store)
    stamp = time.strftime("%Y%m%d-%H%M")
    path = os.path.join(run_dir, f"segment-review-{stamp}.md")
    row = {"kind": "segment_review", "role": "segment_review", "packet_chars": len(packet),
           "at": time.strftime("%Y-%m-%d %H:%M:%S")}
    if dry_run:
        out = {"error": "dry run: nothing asked", "skipped": True}
    else:
        out = (ask or answer.ask)(packet, tier=3, run=os.path.basename(os.path.abspath(run_dir)),
                                  role="segment_review")
    if "error" in out:
        row["error"] = out["error"]
        body = f"# Segment review (not asked: {out['error']})\n\n## Packet\n\n{packet}"
    else:
        u = out["usage"]
        row.update({"model": out["model"], "input": u["input_tokens"], "output": u["output_tokens"],
                    "seconds": out["seconds"], "usd": out["usd"]})
        body = (f"# Segment review ({out['model']}, {u['input_tokens']} in / {u['output_tokens']} out, "
                f"${out['usd']:.3f}, {out['seconds']} s)\n\n{out['result']}\n\n## Packet\n\n{packet}")
    with open(path, "w", encoding="utf-8") as f:
        f.write(body)
    with open(os.path.join(run_dir, "reviews.jsonl"), "a", encoding="utf-8") as f:
        f.write(json.dumps(row) + "\n")
    return path, row


def main(argv=None):
    ap = argparse.ArgumentParser(description="one segment review on the key (packet from the run folder)")
    ap.add_argument("run")
    ap.add_argument("--dry-run", action="store_true")
    a = ap.parse_args(argv)
    path, row = review(a.run, a.dry_run)
    print(json.dumps(dict(row, file=path)))
    return 0 if "error" not in row or a.dry_run else 2


if __name__ == "__main__":
    sys.exit(main())
