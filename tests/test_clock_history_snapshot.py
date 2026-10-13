import collections
import types
import unittest

from hoi4_live.game.clock import _records_since


class ClockHistorySnapshotTests(unittest.TestCase):
    def test_append_during_record_filter_is_deferred_without_losing_the_record(self):
        history = collections.deque()

        class AppendDuringRead(dict):
            appended = False

            def get(self, key, default=None):
                if key == "n" and not self.appended:
                    self.appended = True
                    history.append({"n": 3})
                return super().get(key, default)

        history.extend([AppendDuringRead(n=1), {"n": 2}])
        store = types.SimpleNamespace(history={"GER": history})
        first = _records_since(store, "GER", 0)
        self.assertEqual([record["n"] for record in first], [1, 2])
        self.assertEqual([record["n"] for record in history], [1, 2, 3])
        self.assertEqual(_records_since(store, "GER", 2), [{"n": 3}])


if __name__ == "__main__":
    unittest.main()
