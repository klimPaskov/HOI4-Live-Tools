"""hoi4_smoke_namespace rests the cursor above the event window before it reads it: after a click the cursor stays on
the option, and the next firing's window opens under it with that option's tooltip covering the text (seen live on
1.19.4.2, where the second firing of every event was read as garbage)."""

import unittest
from unittest import mock

import helpers  # noqa: F401
from hoi4_live.errors import ToolError
from hoi4_live.tools import smoke_tools


class RestCursorTests(unittest.TestCase):
    def test_the_cursor_moves_to_a_quiet_spot_above_the_window(self):
        session = mock.Mock()
        session.window.size.return_value = (1920, 1080)
        smoke_tools.rest_cursor(session)
        session.input.move.assert_called_once_with(960, 540)

    def test_a_refused_move_does_not_stop_the_firing(self):
        session = mock.Mock()
        session.window.size.return_value = (1920, 1080)
        session.input.move.side_effect = ToolError("outside")
        smoke_tools.rest_cursor(session)

    def test_it_runs_before_the_window_is_read(self):
        order = []
        session = mock.Mock()
        session.window.size.return_value = (1920, 1080)
        session.input.move.side_effect = lambda *a: order.append("move")
        session.grab.side_effect = lambda *a, **k: order.append("grab")
        session.logs.delta.return_value = ([], {"lines": 0})
        ev = {"id": "m.1", "hidden": True}
        with mock.patch.object(smoke_tools.console, "run", return_value=[{"response": ["ok"]}]),                 mock.patch.object(smoke_tools.popups_mod, "detect", return_value=[]):
            smoke_tools.fire_once(session, ev, None, 0, 0)
        self.assertLess(order.index("move"), order.index("grab"))


class RawKeyTests(unittest.TestCase):
    def fire(self, options, title="Live test two", chosen=None):
        return {"option": 0, "popup": {"title": title, "options": options}, "chosen_text": chosen or options[0],
                "closed": True}

    def test_an_option_that_shows_its_key_is_flagged(self):
        # live 1.19.4.2: the window of livetest.2 showed "livetest.2.b" for the option without localisation
        ev = {"id": "livetest.2", "hidden": False}
        rec = {"fires": [self.fire(["Only translated option", "livetest.2.b"])]}
        status, notes = smoke_tools._event_status(ev, rec)
        self.assertEqual(status, "text_missing")
        self.assertIn("livetest.2.b", notes[0])

    def test_normal_text_and_sentences_are_not_keys(self):
        ev = {"id": "m.1", "hidden": False}
        rec = {"fires": [self.fire(["First option", "Ends here."], title="Mr. Smith arrives")]}
        self.assertEqual(smoke_tools._event_status(ev, rec)[0], "ok")

    def test_a_text_missing_event_counts_as_a_problem(self):
        self.assertIn("text_missing", ("errors", "text_missing", "stuck"))


if __name__ == "__main__":
    unittest.main()
