"""Auto capture keeps retrying PrintWindow after a blank frame (seen live on 1.19.4.1: a blank frame while the game
loaded switched capture to ddagrab for good, and ddagrab timed out while PrintWindow worked)."""
import sys
import unittest
from unittest.mock import Mock, patch

import numpy as np


@unittest.skipUnless(sys.platform == "win32", "Windows capture backend")
class PrintWindowRetryTests(unittest.TestCase):
    def setUp(self):
        from hoi4_live.platform import windows_capture
        self.mod = windows_capture
        self.cap = windows_capture.Capturer(Mock(require=Mock(return_value=1)))
        self.frame = np.full((20, 30, 3), 128, np.uint8)
        self.frame[::2] = 30
        self.blank = np.zeros((20, 30, 3), np.uint8)
        self.cap._ddagrab = Mock(return_value=self.frame)

    def test_blank_frame_falls_back_then_printwindow_is_retried(self):
        self.cap._printwindow = Mock(side_effect=[self.blank, self.frame])
        with patch.object(self.mod.time, "monotonic", side_effect=[100.0, 100.0, 101.0, 116.0]):
            self.cap.grab()
            self.assertEqual(self.cap.last_method, "ddagrab")
            self.cap.grab()                       # within the retry interval: ddagrab only
            self.assertEqual(self.cap._printwindow.call_count, 1)
            self.cap.grab()                       # interval passed: printwindow again, and it works
        self.assertEqual(self.cap.last_method, "printwindow")
        self.assertTrue(self.cap.printwindow_ok)


if __name__ == "__main__":
    unittest.main()
