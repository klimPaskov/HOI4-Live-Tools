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

    def test_per_kind_counts_elements_seconds_and_unchanged(self):
        wings = {"kind": "air_mission", "wings": ["a", "b", "c", "d"], "mission": "cas"}
        line = {"kind": "production_set", "line": 1, "equipment": "x", "factories": 9}
        kinds = cleanliness.per_kind([
            {"id": "1", "action": wings, "status": "Dispatching", "unix_ms": 1000},
            {"id": "1", "action": wings, "status": "Diverged", "unix_ms": 5000,
             "detail": {"unchanged": [{"element": "d", "why": "out of range"}]}},
            {"id": "2", "action": line, "status": "Dispatching", "unix_ms": 6000},
            {"id": "2", "action": line, "status": "Verified", "unix_ms": 7500},
            {"id": "3", "action": line, "status": "Dispatching", "unix_ms": 8000},
            {"id": "3", "action": line, "status": "NotApplied", "unix_ms": 9000},
        ])
        air = kinds["air_mission"]
        self.assertEqual((air["calls"], air["elements"], air["changed"]), (1, 4, 3))
        self.assertAlmostEqual(air["seconds"], 4.0)
        self.assertAlmostEqual(air["per_element"], 4.0 / 3)
        prod = kinds["production_set"]
        self.assertEqual((prod["calls"], prod["changed"]), (2, 1))
        self.assertAlmostEqual(prod["per_element"], 2.5)
        rows = cleanliness.render_per_kind(kinds)
        self.assertIn("air_mission: 1 calls, 4 asked, 3 changed, 4 s, 1.3 s/element", "\n".join(rows))
        # a receipt's own count wins
        self.assertEqual(cleanliness._changed({"status": "Verified", "detail": {"changed": 2}}, 5), 2)


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
