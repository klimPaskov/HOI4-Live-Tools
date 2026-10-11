"""Top bar (date parsing, pause/run icon, speed bar) and console detection on live-captured crops."""

import os
import unittest

import numpy as np
from PIL import Image

from helpers import FIXTURES, needs_fixture

from hoi4_live.perception import screen, topbar

SCREENS = os.path.join(FIXTURES, "screens")


def top_right(name, width=1920):
    """Paste a 460x60 top-right crop into a blank client image of the given width."""
    crop = np.asarray(Image.open(os.path.join(SCREENS, name)).convert("RGB"))
    img = np.zeros((1080, width, 3), np.uint8)
    img[:60, width - 460:] = crop
    return img


class DateParseTests(unittest.TestCase):
    def test_plain(self):
        d = topbar.parse_date("12:00, 1 Jan, 1936")
        self.assertEqual((d["year"], d["month"], d["day"], d["hour"]), (1936, 1, 1, 12))
        self.assertEqual(d["hoi4"], "1936.1.1.12")

    def test_ocr_noise(self):
        self.assertEqual(topbar.parse_date("23: 00, 31 Jan, 1936")["day"], 31)
        self.assertEqual(topbar.parse_date("x 4:00, 6 March, 1936 y")["month"], 3)
        self.assertEqual(topbar.parse_date("8:00, a Feb, 1936")["day"], 4)       # 'a' read for 4

    def test_rejects(self):
        for t in ("", "1936", "25:00, 9 Mar, 1936", "12:00, 31 Feb, 1936", "12:00, 1 Foo, 1936"):
            self.assertIsNone(topbar.parse_date(t), t)

    def test_midnight_is_24(self):
        # HOI4 shows midnight as 24:00 of the day that ends
        d = topbar.parse_date("24:00, 9 Mar, 1936")
        self.assertEqual(d["hour"], 24)
        self.assertEqual(topbar.date_key(d), (1936, 3, 10, 0))
        e = topbar.parse_date("1:00, 10 Mar, 1936")
        self.assertEqual(topbar.hours(e) - topbar.hours(d), 1)

    def test_lenient_hour(self):
        d = topbar.parse_date("27:00, 9 Mar, 1936", lenient=True)
        self.assertEqual((d["month"], d["day"], d["hour"]), (3, 9, 0))
        self.assertTrue(d["hour_uncertain"])
        self.assertEqual(topbar.parse_date("za:oo, 9 Mar, 1936")["hour"], 24)

    def test_hours_before_1970(self):
        a = topbar.parse_date("12:00, 1 Jan, 1936")
        b = topbar.parse_date("12:00, 11 Jan, 1936")
        self.assertEqual(topbar.hours(b) - topbar.hours(a), 240)


class PauseStateTests(unittest.TestCase):
    def test_paused_bright_and_dim(self):
        for name in ("topright_paused.png", "topright_paused_dim.png"):
            state, ev = topbar.pause_state(top_right(name))
            self.assertEqual(state, "paused", (name, ev))

    def test_running(self):
        state, ev = topbar.pause_state(top_right("topright_running.png"))
        self.assertEqual(state, "running", ev)

    def test_other_width(self):
        self.assertEqual(topbar.pause_state(top_right("topright_paused.png", 2560))[0], "paused")

    def test_speed_bar(self):
        self.assertEqual(topbar.speed_level(top_right("topright_paused.png")), 1)


class ConsoleTests(unittest.TestCase):
    @needs_fixture("screens", "console_open.jpg")
    def test_console_frame(self):
        img = np.asarray(Image.open(os.path.join(SCREENS, "console_open.jpg")).convert("RGB"))
        r = screen.console_rect(img)
        self.assertIsNotNone(r)
        x0, y0, x1, y1 = r
        self.assertLess(abs(x0 - 10), 4)
        self.assertLess(abs(y0 - 169), 4)
        self.assertLess(abs(x1 - 499), 4)
        self.assertLess(abs(y1 - 510), 4)

    def test_no_console(self):
        for name in ("load_screen.jpg",):
            img = np.asarray(Image.open(os.path.join(SCREENS, name)).convert("RGB"))
            self.assertIsNone(screen.console_rect(img), name)


if __name__ == "__main__":
    unittest.main()
