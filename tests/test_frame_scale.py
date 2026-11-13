"""Clicks land correctly when the capture is smaller than the game window (issue #27: a 2560x1440 borderless window
captured as a 1920x1080 frame). Points read from the frame are scaled up to the client area."""

import sys
import unittest
from unittest import mock

import helpers  # noqa: F401
from hoi4_live.errors import ToolError

WIN = unittest.skipUnless(sys.platform == "win32", "Windows native window backend")


def make_window(client, frame=None, origin=(0, 0)):
    from hoi4_live.platform.windows_window import GameWindow
    w = object.__new__(GameWindow)
    w.frame = frame
    w.client = lambda: (origin[0], origin[1], client[0], client[1])
    return w


@WIN
class FrameScaleTests(unittest.TestCase):
    def test_a_frame_the_size_of_the_client_changes_nothing(self):
        w = make_window((1920, 1080), (1920, 1080), origin=(100, 50))
        self.assertEqual(w.frame_scale(), (1.0, 1.0))
        self.assertEqual(w.to_screen(961, 352), (1061, 402))
        self.assertEqual(w.size(), (1920, 1080))

    def test_before_any_capture_nothing_is_scaled(self):
        w = make_window((2560, 1440))
        self.assertEqual(w.to_screen(10, 20), (10, 20))
        self.assertEqual(w.size(), (2560, 1440))

    def test_the_issue_27_geometry_scales_the_ocr_point_to_the_client(self):
        w = make_window((2560, 1440), (1920, 1080))
        self.assertAlmostEqual(w.frame_scale()[0], 2560 / 1920)
        self.assertEqual(w.to_screen(961, 352), (1281, 469))        # the point that worked by hand in the issue
        self.assertEqual(w.size(), (1920, 1080))                    # tools work in the frame's space

    def test_the_window_origin_is_added_after_scaling(self):
        w = make_window((2560, 1440), (1920, 1080), origin=(-2560, 0))
        self.assertEqual(w.to_screen(960, 540), (-2560 + 1280, 720))

    def test_points_outside_the_frame_are_refused_but_can_be_clamped(self):
        w = make_window((2560, 1440), (1920, 1080))
        with self.assertRaises(ToolError):
            w.to_screen(1920, 100)
        self.assertEqual(w.to_screen(5000, 5000, clamp=True), (round(1919 * 2560 / 1920), round(1079 * 1440 / 1080)))

    def test_from_screen_is_the_inverse(self):
        w = make_window((2560, 1440), (1920, 1080), origin=(10, 20))
        x, y = w.from_screen(*w.to_screen(600, 300))
        self.assertAlmostEqual(x, 600, delta=1)
        self.assertAlmostEqual(y, 300, delta=1)

    def test_a_crop_of_another_shape_is_never_stretched(self):
        w = make_window((2560, 1440), (1000, 1000))
        self.assertEqual(w.frame_scale(), (1.0, 1.0))
        self.assertEqual(w.to_screen(500, 500), (500, 500))

    def test_the_capturer_tells_the_window_the_frame_size(self):
        import numpy as np
        from hoi4_live.platform.windows_capture import Capturer
        window = mock.Mock()
        cap = Capturer(window)
        with mock.patch.object(Capturer, "_grab", return_value=np.zeros((1080, 1920, 3), np.uint8)):
            cap.grab()
        window.note_frame.assert_called_once_with(1920, 1080)

    def test_input_clicks_the_scaled_screen_point(self):
        from hoi4_live.platform import windows_input
        w = make_window((2560, 1440), (1920, 1080))
        inp = windows_input.Input(w, settle=0)
        w.ensure_foreground = lambda: None
        with mock.patch.object(windows_input, "move_screen") as move, mock.patch.object(windows_input, "_send"), \
                mock.patch.object(windows_input.time, "sleep"):
            inp.click(961, 352)
        move.assert_called_with(1281, 469)


if __name__ == "__main__":
    unittest.main()
