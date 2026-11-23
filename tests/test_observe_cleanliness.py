import json
import pathlib
import tempfile
import unittest

from hoi4_live.observe import cleanliness


def _write(path, rows):
    path.write_text("\n".join(json.dumps(r) for r in rows) + "\n")


class CleanlinessTest(unittest.TestCase):
    def test_repeats_retries_still_stretches_and_player_share(self):
        with tempfile.TemporaryDirectory() as d:
            _check(pathlib.Path(d))


def _check(tmp_path):
    act = {"kind": "event_choose", "event": "e.1", "option": 0}
    _write(tmp_path / "game-actions.jsonl", [
        {"action": act, "status": "Dispatching", "unix_ms": 1000},
        {"action": act, "status": "NotApplied", "unix_ms": 1100},
        {"action": act, "status": "NotApplied", "unix_ms": 2100},
        {"action": {"kind": "air_mission", "wing": "w"}, "status": "Verified", "unix_ms": 3000},
    ])
    _write(tmp_path / "calls.jsonl", [
        {"utc": 0.5, "tool": "hoi4_overview", "source": "player", "args": {},
         "body": {"date": {"text": "1940.05.01.00"}}},
        {"utc": 1.5, "tool": "hoi4_wait_change", "source": "player", "args": {}},
        {"utc": 2.5, "tool": "hoi4_army_front", "source": "player", "args": {"request_id": "r"}},
    ])
    idle = {"economy": {"factories": {"military": 10}}, "production": {"military_assigned": 7}}
    snaps = [
        dict(idle, utc=0.0, date="1940.05.01.00", speed=2),
        dict(idle, utc=10.0, date="1940.05.08.00", speed=3),
    ]
    y = cleanliness.measure(str(tmp_path), snaps)["1940"]
    assert y["acts"] == 3 and y["refused"] == 2
    assert sum(y["retried"].values()) == 1
    assert sum(y["repeats"].values()) == 1
    assert y["idle_days"] == 7
    assert y["speed_seen"] == 1
    assert y["player"]["waits"] == 1 and y["player"]["acts"] == 1 and y["player"]["reads"] == 1
    assert "CLEANLINESS" in cleanliness.render(str(tmp_path), snaps)


if __name__ == "__main__":
    unittest.main()
