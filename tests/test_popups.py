"""Popup detection on real screenshots (captured at 1920x1080): event, news and super-event windows,
stacked popups, popups over open panels, no-popup frames, and OCR reading of title/text/options."""

import glob
import json
import os
import sys
import unittest

import numpy as np
from PIL import Image

from helpers import EXTERNAL_POPUPS, FIXTURES, needs_fixture

from hoi4_live.perception import popups as P

DIR = os.path.join(FIXTURES, "popups")


def load(name):
    return np.asarray(Image.open(os.path.join(DIR, name)).convert("RGB"))


@needs_fixture("popups", "labels.json")
class DetectTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        with open(os.path.join(DIR, "labels.json"), encoding="utf-8") as f:
            cls.labels = json.load(f)

    def test_labelled_fixtures(self):
        for name, want in self.labels.items():
            with self.subTest(fixture=name):
                got = P.detect(load(name))
                self.assertEqual([(p.kind, len(p.buttons)) for p in got], [(w["kind"], w["options"]) for w in want])
                for p, w in zip(got, want):
                    cx, cy = p.buttons[0].center
                    self.assertLessEqual(abs(cx - w["first_option_center"][0]), 6)
                    self.assertLessEqual(abs(cy - w["first_option_center"][1]), 6)

    def test_button_geometry(self):
        # every option button is the same 42 px tall sprite at gui_scale 1
        for name in self.labels:
            for p in P.detect(load(name)):
                for b in p.buttons:
                    self.assertTrue(38 <= b.box[3] <= 46, (name, b.box))
                    self.assertTrue(300 <= b.box[2] <= 380, (name, b.box))

    def test_four_options_in_order(self):
        p = P.detect(load("evt028_pop00.jpg"))[0]
        ys = [b.box[1] for b in p.buttons]
        self.assertEqual(ys, sorted(ys))
        self.assertTrue(all(40 <= b - a <= 55 for a, b in zip(ys, ys[1:])))

    def test_stacked_order_topmost_first(self):
        got = P.detect(load("evt002_pop00.jpg"))
        self.assertEqual([p.kind for p in got], ["event", "super"])
        self.assertTrue(all(p.stacked for p in got))

    def test_no_popup(self):
        self.assertEqual(P.detect(load("evt001_pop08.jpg")), [])
        for screen in ("load_screen.jpg",):
            img = np.asarray(Image.open(os.path.join(FIXTURES, "screens", screen)).convert("RGB"))
            self.assertEqual(P.detect(img), [], screen)

    def test_other_client_sizes(self):
        # HOI4 draws its UI at a fixed pixel size (gui_scale 1) whatever the resolution, so a smaller or larger
        # client shows the same popup pixels: crop to 1600x900 and pad to 2560x1440
        full = load("evt028_pop00.jpg")
        small = full[90:990, 160:1760]
        got = P.detect(small)
        self.assertEqual([(p.kind, len(p.buttons)) for p in got], [("event", 4)])
        big = np.zeros((1440, 2560, 3), np.uint8)
        big[:, :, 2] = 90                                   # a dark blue "sea" around the picture
        big[180:1260, 320:2240] = full
        got = P.detect(big)
        self.assertEqual([(p.kind, len(p.buttons)) for p in got], [("event", 4)])
        self.assertEqual(got[0].buttons[0].center, (1068 + 320, 473 + 180))

    def test_to_dict(self):
        p = P.detect(load("evt028_pop00.jpg"))[0]
        d = p.to_dict(0)
        self.assertEqual(d["index"], 0)
        self.assertEqual(len(d["options"]), 4)
        self.assertIn("center", d["options"][0])


@unittest.skipUnless(os.path.isdir(EXTERNAL_POPUPS) and os.path.exists(os.path.join(DIR, "external_labels.json")),
                     "external popup screenshots not available")
class ExternalRegressionTests(unittest.TestCase):
    """A folder of popup screenshots (HOI4_LIVE_POPUP_FIXTURES) checked against external_labels.json."""

    def test_all(self):
        with open(os.path.join(DIR, "external_labels.json"), encoding="utf-8") as f:
            labels = json.load(f)
        bad = []
        for name, want in labels.items():
            path = os.path.join(EXTERNAL_POPUPS, name)
            if not os.path.exists(path):
                continue
            got = [[p.kind, len(p.buttons)] for p in P.detect(np.asarray(Image.open(path).convert("RGB")))]
            if got != want:
                bad.append((name, want, got))
        self.assertEqual(bad, [])


def _ocr_available():
    try:
        from hoi4_live.ocr.winocr import OcrHost
        h = OcrHost()
        try:
            h.recognize(np.full((40, 200, 3), 255, np.uint8))
            return True
        finally:
            h.close()
    except Exception:
        return False


@unittest.skipUnless(sys.platform == "win32" and _ocr_available(), "Windows OCR not available")
@needs_fixture("popups", "labels.json")
class ReadPopupTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        from hoi4_live.ocr.winocr import OcrHost
        cls.ocr = OcrHost()

    @classmethod
    def tearDownClass(cls):
        cls.ocr.close()

    def test_event_with_four_options(self):
        img = load("evt028_pop00.jpg")
        p = P.read_popup(img, P.detect(img)[0], self.ocr)
        self.assertEqual(p.title, "Asteroid Incoming")
        self.assertIn("impact coordinates", p.text)
        self.assertTrue(p.buttons[0].text.startswith("Strike"))
        self.assertIn("near miss", p.buttons[3].text)

    def test_news(self):
        img = load("evt001_pop06.jpg")
        p = P.read_popup(img, P.detect(img)[0], self.ocr)
        self.assertEqual(p.kind, "news")
        self.assertIn("Aussa", p.title)
        self.assertIn("Unsurprising", p.buttons[0].text)


if __name__ == "__main__":
    unittest.main()
