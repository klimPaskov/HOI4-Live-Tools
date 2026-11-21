"""Issue 311: one unreadable frame must not make hoi4_screen answer `unknown`."""

import unittest
from unittest import mock

from hoi4_live.tools import understanding


class FakeLogs:
    def idle_seconds(self, name):
        return 30.0


class FakeSession:
    def __init__(self):
        self.logs = FakeLogs()
        self.ocr = object()
        self.grabs = 0

    def grab(self, focus=True):
        self.grabs += 1
        return "frame-%d" % self.grabs


class FakeScreen:
    def __init__(self, answers):
        self.answers = list(answers)

    def classify(self, img, ocr, idle):
        return {"screen": self.answers.pop(0), "console_open": False}


class ClassifyRetryTests(unittest.TestCase):
    def run_case(self, answers, retries=2):
        session, screen = FakeSession(), FakeScreen(answers)
        with mock.patch("time.sleep"):
            img, st = understanding.classify_with_retry(session, screen, retries=retries)
        return session, img, st

    def test_one_unreadable_frame_is_retried_on_a_fresh_capture(self):
        session, img, st = self.run_case(["unknown", "main_menu"])
        self.assertEqual(st["screen"], "main_menu")
        self.assertEqual(session.grabs, 2)
        self.assertEqual(img, "frame-2")

    def test_a_readable_frame_is_not_captured_twice(self):
        session, _, st = self.run_case(["map"])
        self.assertEqual((st["screen"], session.grabs), ("map", 1))

    def test_a_persistent_unknown_stops_after_the_retries(self):
        session, _, st = self.run_case(["unknown", "unknown", "unknown"])
        self.assertEqual((st["screen"], session.grabs), ("unknown", 3))


if __name__ == "__main__":
    unittest.main()
