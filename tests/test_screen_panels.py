"""Panel names from title lines, and the generic centre-window detector."""

import os
import unittest

import numpy as np
from PIL import Image

from helpers import FIXTURES, needs_fixture

from hoi4_live.perception import screen


class Line:
    def __init__(self, text, box):
        self.text = text
        self.box = box

    @property
    def center(self):
        return self.box[0] + self.box[2] // 2, self.box[1] + self.box[3] // 2


class PanelTests(unittest.TestCase):
    def detect(self, *texts, y=100):
        return screen.detect_panel([Line(t, [40, y, 200, 20]) for t in texts], 1920, 1080)[0]

    def test_titles_seen_live(self):
        # OCR of the panel title strips on HOI4 1.19.2 (English UI), one per panel hotkey
        cases = {"POLITICAL": "politics", "DECISIONS": "decisions", "INTELLIGENCE AGENCY": "intelligence",
                 "RESEARCH": "research", "TRADE": "trade", "CONSTRUCTION": "construction",
                 "PRODUCTION": "production", "RECRUIT & DEPLOY": "deployment", "LOGISTICS": "logistics",
                 "OFFICER CORPS": "officer_corps", "FACTION": "faction", "DIVISION OVERVIEW": "army_overview",
                 "21 Fleets": "navy_overview", "FORCES OVERVIEW": "air_overview",
                 "UNITED STATES FOCUS TREE": "focus_tree"}
        for text, name in cases.items():
            self.assertEqual(self.detect(text), name, text)

    def test_only_the_title_strip_counts(self):
        self.assertIsNone(self.detect("RESEARCH", y=600))         # a word lower on the map is not a title
        self.assertIsNone(self.detect("Toronto", "Detroit"))
        self.assertEqual(screen.PANEL_KEYS["production"], "y")


class WindowTests(unittest.TestCase):
    @needs_fixture("screens", "exit_dialog_960.jpg")
    def test_exit_dialog_is_a_window(self):
        img = np.asarray(Image.open(os.path.join(FIXTURES, "screens", "exit_dialog_960.jpg")).convert("RGB"))
        img = np.asarray(Image.fromarray(img).resize((1920, 1080)))
        box = screen.dark_window_box(img)
        self.assertIsNotNone(box)
        x, y, w, h = box
        self.assertTrue(x < 800 < 1150 < x + w and y < 420 < 700 < y + h, box)

    def test_map_is_not_a_window(self):
        img = np.zeros((1080, 1920, 3), np.uint8)
        img[..., 2] = 120                                   # blue sea
        img[..., 1] = 60
        self.assertIsNone(screen.dark_window_box(img))


if __name__ == "__main__":
    unittest.main()


class SplitTitleTests(unittest.TestCase):
    def test_recruit_and_deploy_split_into_words_next_to_division_templates(self):
        from hoi4_live.perception import screen
        from hoi4_live.ocr.winocr import OcrLine

        def L(t, x, y):
            return OcrLine(t, [x, y, 100, 19], [])
        lines = [L("RECRUIT", 42, 96), L("DEPLOY", 202, 96), L("DIVISION", 584, 96), L("TEMPLATES", 728, 96)]
        self.assertEqual(screen.detect_panel(lines, 1920, 1080)[0], "deployment")
        self.assertEqual(screen.detect_panel([L("POLITICAL", 42, 96)], 1920, 1080)[0], "politics")
