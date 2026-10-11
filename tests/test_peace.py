"""The peace conference reader and tool (game/peaceview.py, hoi4_peace) against canned OCR."""

import unittest
from unittest import mock

import numpy as np

from helpers import FIXTURES  # noqa: F401

from hoi4_live.errors import ToolError
from hoi4_live.game import peaceview as pv
from hoi4_live.ocr.winocr import OcrLine, OcrResult
from hoi4_live.tools import politics_tools as pt


def line(text, x, y, w=100, h=14):
    return OcrLine(text, [x, y, w, h], [{"text": text, "box": [x, y, w, h]}])


class FakeSession:
    """A 1920x1080 client whose OCR answers by region from a scripted screen."""

    def __init__(self, screen):
        self.screen = screen            # callable -> list of OcrLine in client coordinates
        self.clicks = []
        self.input = self
        self.ocr = self
        self.window = mock.Mock(size=lambda: (1920, 1080))

    def grab(self, focus=None):
        return np.zeros((1080, 1920, 3), np.uint8)

    def recognize(self, img, region=None, scale=None, **kw):
        x0, y0, w, h = region
        lines = [l for l in self.screen() if x0 <= l.box[0] <= x0 + w and y0 <= l.box[1] <= y0 + h]
        return OcrResult(lines, list(region), scale or 1.0)

    def click(self, x, y, *a, **k):
        self.clicks.append((x, y))

    def park_cursor(self):
        pass

    def key(self, *a, **k):
        pass


def screen(selected=False, expanded=True, open_=True):
    def build():
        if not open_:
            return [line("Research", 100, 100)]
        out = [line("Confirm and Exit", 1700, 48), line("Treaty of Wien", 30, 40),
               line("Score: 610", 1040, 118), line("Conference turn: 1", 780, 160),
               line("Austria", 80, 400), line("Submit Demands 190" if selected else "Pass", 1700, 1030)]
        if expanded:
            out += [line("Select All" if not selected else "Cancel All", 120, 450).__class__(
                        ("190 Select All" if not selected else "190 Cancel All"), [60, 443, 120, 14], []),
                    line("23 Oberösterreich", 80, 474), line("18 Tirol", 80, 532), line("52 Niederösterreich", 80, 560)]
        return out
    return build


class ReadTests(unittest.TestCase):
    def test_closed(self):
        self.assertFalse(pv.is_open(FakeSession(screen(open_=False))))
        self.assertEqual(pv.read(FakeSession(screen(open_=False))), {"open": False})

    def test_read_score_turn_rows_and_select_all(self):
        st = pv.read(FakeSession(screen()))
        self.assertTrue(st["open"])
        self.assertEqual((st["score"], st["turn"]), (610, 1))
        self.assertEqual([(r["name"], r["cost"]) for r in st["rows"]],
                         [("Oberösterreich", 23), ("Tirol", 18), ("Niederösterreich", 52)])
        self.assertEqual(st["select_all"]["cost"], 190)

    def test_collapsed_loser_is_expanded_by_its_arrow(self):
        state = {"expanded": False}

        def build():
            return screen(expanded=state["expanded"])()
        s = FakeSession(build)
        orig = s.click

        def click(x, y, *a, **k):
            orig(x, y)
            if (x, y) == (287, 422):
                state["expanded"] = True
        s.click = click
        s.input = s
        st = pv.expand_loser(s, pv.read(s))
        self.assertEqual(len(st["rows"]), 3)
        self.assertIn((287, 422), s.clicks)


class TakeTests(unittest.TestCase):
    def test_take_all_clicks_select_all_once(self):
        s = FakeSession(screen())
        clicked, st = pv.take(s, [], True)
        self.assertEqual(clicked, ["all"])
        self.assertEqual(len(s.clicks), 1)
        self.assertEqual(s.clicks[0][1], 450)

    def test_take_all_when_already_selected_clicks_nothing(self):
        s = FakeSession(screen(selected=True))
        pv.take(s, [], True)
        self.assertEqual(s.clicks, [])

    def test_take_named_states(self):
        s = FakeSession(screen())
        clicked, _ = pv.take(s, ["Tirol", "Oberosterreich"], False)
        self.assertEqual(clicked, ["Tirol", "Oberösterreich"])

    def test_unknown_state_lists_the_rows(self):
        s = FakeSession(screen())
        with self.assertRaises(ToolError) as cm:
            pv.take(s, ["Bavaria"], False)
        self.assertIn("Tirol", str(cm.exception.details["rows"]))


class ToolTests(unittest.TestCase):
    def test_closed_conference_is_an_actionable_error(self):
        s = FakeSession(screen(open_=False))
        with mock.patch.object(pt.panels, "current", lambda session: "map"):
            with self.assertRaises(ToolError) as cm:
                pt.hoi4_peace(s, {"action": "status", "all": False, "submit": False, "exit": False})
        self.assertIn("peace conference", str(cm.exception))

    def test_take_requires_a_choice(self):
        s = FakeSession(screen())
        with mock.patch.object(pt.cc, "finish", lambda *a, **k: None):
            with self.assertRaises(ToolError):
                pt.hoi4_peace(s, {"action": "take", "all": False, "submit": False, "exit": False})


if __name__ == "__main__":
    unittest.main()
