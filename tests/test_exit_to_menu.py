"""hoi4_new_game exit_to_menu on 1.19.4.2 (issue #28): resigning shows a Playthrough Overview window (tabs Playthrough
Overview, Awards, Statistics) whose Exit to Menu button leads to the main menu. Scene-driven fake screens."""

import re
import unittest
from types import SimpleNamespace
from unittest import mock

import helpers  # noqa: F401
from hoi4_live.errors import ToolError
from hoi4_live.game import menu


def line(text, center=(960, 900)):
    x, y = center
    return SimpleNamespace(text=text, center=center, box=[x - 60, y - 15, 120, 30])


SCENES = {
    "overview": [line("Playthrough Overview", (300, 100)), line("Awards", (500, 100)), line("Statistics", (700, 100)),
                 line("Exit to Menu", (1500, 980))],
    "overview_misread": [line("Playthrough Overview", (300, 100)), line("Awards", (500, 100)),
                         line("Exit*o Menu", (1500, 980))],
    "statistics": [line("Statistics", (700, 100)), line("Quit", (1500, 980))],
    "menu_drawing": [line("Quit", (960, 800)), line("Loading", (900, 500))],
    "loading": [line("Loading", (900, 500))],
    "menu": [line("Single Player", (960, 350)), line("Multiplayer", (960, 400))],
}


class Screens:
    """The visible screen changes when the named button is clicked at its centre."""

    def __init__(self, scene, transitions):
        self.scene, self.transitions, self.clicks = scene, transitions, []

    def result(self):
        return SimpleNamespace(lines=list(SCENES[self.scene]))

    def ocr_screen(self, session, img=None, region=None, scale=None):
        return None, self.result()

    def find_text(self, session, pattern, img=None, region=None, scale=None, exact=False):
        res = self.result()
        rx = re.compile((r"^\W*" + pattern + r"\W*$") if exact else pattern, re.I)
        return None, [l for l in res.lines if rx.search(l.text)], res

    def click(self, x, y):
        self.clicks.append((x, y))
        for text, target in self.transitions.items():
            if any(l.text == text and l.center == (x, y) for l in SCENES[self.scene]):
                self.scene = target
                return


class ExitToMenuTests(unittest.TestCase):
    def run_exit(self, screens, timeout=240):
        session = mock.Mock()
        session.input.click.side_effect = screens.click
        with mock.patch.object(menu, "open_esc_menu", return_value=[]), \
                mock.patch.object(menu.ui, "click_text"), \
                mock.patch.object(menu, "_confirm_if_asked", return_value="OK"), \
                mock.patch.object(menu.ui, "ocr_screen", side_effect=screens.ocr_screen), \
                mock.patch.object(menu.ui, "find_text", side_effect=screens.find_text):
            return menu.quit_to_main_menu(session, timeout=timeout), session

    def test_the_playthrough_overview_exit_button_is_clicked(self):
        screens = Screens("overview", {"Exit to Menu": "menu"})
        out, session = self.run_exit(screens)
        self.assertTrue(out["overview_exit"])
        self.assertFalse(out["statistics_quit"])
        self.assertEqual(screens.clicks, [(1500, 980)])

    def test_a_misread_button_label_still_matches(self):
        # live 1.19.4.2: OCR at 1.5x read the button as "Exit*o Menu"
        screens = Screens("overview_misread", {"Exit*o Menu": "menu"})
        out, _ = self.run_exit(screens)
        self.assertTrue(out["overview_exit"])
        self.assertEqual(screens.clicks, [(1500, 980)])

    def test_the_label_is_read_at_two_times_or_more(self):
        screens = Screens("overview", {"Exit to Menu": "menu"})
        scales = []
        original = screens.find_text

        def spy(session, pattern, img=None, region=None, scale=None, exact=False):
            scales.append(scale)
            return original(session, pattern, img, region, scale, exact)
        screens.find_text = spy
        self.run_exit(screens)
        self.assertIn(2.0, scales)
        self.assertNotIn(1.5, scales[:1])

    def test_the_old_statistics_screen_still_uses_quit(self):
        screens = Screens("statistics", {"Quit": "menu"})
        out, _ = self.run_exit(screens)
        self.assertTrue(out["statistics_quit"])
        self.assertFalse(out["overview_exit"])

    def test_the_main_menus_own_quit_button_is_never_clicked(self):
        # live 1.19.4.2: the main menu was still being drawn, Single Player not yet read, and Quit exited the game
        screens = Screens("menu_drawing", {"Quit": "menu"})
        times = iter(range(0, 100))
        with mock.patch.object(menu.time, "time", side_effect=lambda: next(times)):
            with self.assertRaises(ToolError):
                self.run_exit(screens, timeout=30)
        self.assertEqual(screens.clicks, [])

    def test_the_overview_button_is_not_clicked_when_that_window_is_not_up(self):
        screens = Screens("loading", {})
        SCENES["pause"] = [line("Exit to Menu", (960, 600)), line("Resume")]
        self.addCleanup(SCENES.pop, "pause")
        screens.scene = "pause"
        with mock.patch.object(menu.time, "time", side_effect=[0, 0, 0, 1, 1, 1000, 1000]):
            with self.assertRaises(ToolError):
                self.run_exit(screens, timeout=5)
        self.assertEqual(screens.clicks, [])

    def test_a_button_that_does_not_register_is_clicked_at_most_three_times(self):
        screens = Screens("overview", {})                # the click never takes effect
        times = iter(range(0, 100))
        with mock.patch.object(menu.time, "time", side_effect=lambda: next(times)):
            with self.assertRaises(ToolError):
                self.run_exit(screens, timeout=40)
        self.assertEqual(len(screens.clicks), menu.OVERVIEW_EXIT_CLICKS)


if __name__ == "__main__":
    unittest.main()
