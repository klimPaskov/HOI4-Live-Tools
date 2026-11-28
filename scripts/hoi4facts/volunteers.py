#!/usr/bin/env python3
"""volunteers.py: the volunteer window and cap for a plan, from the game's defines (any country, any mod).

Usage:
    python3 -m scripts.hoi4facts.volunteers GAME_DIR --first-war Y.M.D --divisions N [--own-divisions N]
        [--receiver-provinces N] [--mod DIR ...]

Prints JSON: send_until (the last day a force sent now can still be recalled before our own war: the first war minus
the arrival recall lock, the transfer days and the recall days of the force), recall_from (the first war minus the
force's recall days and one transfer as the margin), and the cap the game computes (receiver provinces x
VOLUNTEERS_PER_TARGET_PROVINCE + own divisions x VOLUNTEERS_PER_COUNTRY_ARMY) with the divisions-required floor.
Every number comes from common/defines; nothing is assumed. Stdlib only.
"""
import argparse
import datetime
import json
import math
import os
import sys

if __package__ in (None, ""):
    sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__)))))
    from scripts.hoi4facts.derive import define  # noqa: E402
    from scripts.hoi4facts.game import Game  # noqa: E402
else:
    from .derive import define
    from .game import Game

NAMES = ("RECALL_VOLUNTEERS_AFTER_ARRIVAL_COOLDOWN", "VOLUNTEERS_TRANSFER_SPEED", "VOLUNTEERS_RECALL_DAYS_PER_UNIT",
         "VOLUNTEERS_PER_TARGET_PROVINCE", "VOLUNTEERS_PER_COUNTRY_ARMY", "VOLUNTEERS_DIVISIONS_REQUIRED",
         "SEND_VOLUNTEERS_AGAIN_COOLDOWN")


def parse_date(text):
    y, m, d = (int(x) for x in text.split("."))
    return datetime.date(y, m, d)


def fmt(day):
    return f"{day.year}.{day.month}.{day.day}"


def window(defs, first_war, divisions, own_divisions=None, receiver_provinces=None):
    """The plan's window and the cap from the defines `defs` ({NAME: number})."""
    missing = [n for n in NAMES if defs.get(n) is None]
    if missing:
        raise ValueError("defines missing: " + ", ".join(missing))
    recall_days = math.ceil(defs["VOLUNTEERS_RECALL_DAYS_PER_UNIT"] * divisions)
    transfer = math.ceil(defs["VOLUNTEERS_TRANSFER_SPEED"])
    lock = math.ceil(defs["RECALL_VOLUNTEERS_AFTER_ARRIVAL_COOLDOWN"])
    send_until = first_war - datetime.timedelta(days=lock + transfer + recall_days + transfer)
    recall_from = first_war - datetime.timedelta(days=recall_days + transfer)
    out = {"send_until": fmt(send_until), "recall_from": fmt(recall_from), "recall_days": recall_days,
           "transfer_days": transfer, "recall_lock_days": lock,
           "divisions_required": defs["VOLUNTEERS_DIVISIONS_REQUIRED"],
           "resend_cooldown_days": defs["SEND_VOLUNTEERS_AGAIN_COOLDOWN"], "defines": {n: defs[n] for n in NAMES}}
    if own_divisions is not None and receiver_provinces is not None:
        out["cap"] = math.floor(receiver_provinces * defs["VOLUNTEERS_PER_TARGET_PROVINCE"] +
                                own_divisions * defs["VOLUNTEERS_PER_COUNTRY_ARMY"])
        out["may_send"] = own_divisions >= defs["VOLUNTEERS_DIVISIONS_REQUIRED"]
    return out


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("game_dir")
    ap.add_argument("--first-war", required=True)
    ap.add_argument("--divisions", type=int, required=True)
    ap.add_argument("--own-divisions", type=int)
    ap.add_argument("--receiver-provinces", type=int)
    ap.add_argument("--mod", action="append", default=[])
    a = ap.parse_args(argv)
    game = Game(a.game_dir, a.mod)
    defs = {n: define(game, n) for n in NAMES}
    print(json.dumps(window(defs, parse_date(a.first_war), a.divisions, a.own_divisions, a.receiver_provinces), indent=1))


if __name__ == "__main__":
    main()
