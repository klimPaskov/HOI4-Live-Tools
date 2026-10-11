"""Known-mistake hints on error.log deltas."""

import os
import shutil
import tempfile
import unittest

import helpers  # noqa: F401

from hoi4_live.game import log_hints
from hoi4_live.game.logs import Logs


class LogHintTests(unittest.TestCase):
    def test_each_documented_message_has_a_fix(self):
        samples = {
            "event_triggered_only_with_mtth": "Event is triggered only, but does not have a 1 base-factor. [events/a.txt]",
            "event_fires_every_day": "Event is set to trigger every day: my_event.1",
            "event_namespace_missing": "Malformed token: my_event.12, near line 4",
            "event_numeric_id_invalid": "Failed to create id 12300000 50. Already exists in game. This might crash the game.",
        }
        for rule_id, line in samples.items():
            hint = log_hints.hint_for(line)
            self.assertEqual(hint["id"], rule_id)
            self.assertGreater(len(hint["fix"]), 20)
        self.assertIsNone(log_hints.hint_for("Unknown effect: add_powr in common/decisions/x.txt"))

    def test_error_log_delta_carries_the_hint_and_other_logs_do_not(self):
        directory = tempfile.mkdtemp()
        try:
            with open(os.path.join(directory, "error.log"), "w", encoding="utf-8") as f:
                f.write("[00:00:01][event.cpp:12]: Malformed token: my_event.12, near line 4\n"
                        "[00:00:02][effect.cpp:9]: Unknown effect add_powr\n")
            with open(os.path.join(directory, "game.log"), "w", encoding="utf-8") as f:
                f.write("Malformed token: my_event.12\n")
            logs = Logs(directory)
            rows, _ = logs.delta("error")
            self.assertEqual(rows[0]["known_mistake"]["id"], "event_namespace_missing")
            self.assertNotIn("known_mistake", rows[1])
            rows, _ = logs.delta("game")
            self.assertNotIn("known_mistake", rows[0])
        finally:
            shutil.rmtree(directory, ignore_errors=True)


if __name__ == "__main__":
    unittest.main()
