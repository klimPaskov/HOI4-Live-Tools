"""Campaign focus identity is unique label evidence, never fuzzy similarity."""
from contextlib import ExitStack, nullcontext
import unittest
from unittest import mock

import helpers  # noqa: F401
from hoi4_live.errors import ToolError
from hoi4_live.game import focusview
from hoi4_live.tools import politics_tools as tool


class FocusTitleIdentityTests(unittest.TestCase):
    def test_idle_requires_complete_idle_button_label(self):
        session = mock.Mock()
        for text, idle in (("Select a National Focus", True),
                           ("National Focus", False),
                           ("National Focus Production", False),
                           ("", False)):
            with self.subTest(text=text):
                session.ocr.recognize.return_value.lines = [mock.Mock(text=text)]
                result = focusview.read_button(session, img=object())
                self.assertEqual(bool(result.get("text") and not result.get("current")
                                      and not result.get("unread")), idle)

    def test_exact_unique_title_and_near_prefix(self):
        names = {"F": "War Plans Division", "AIR": "Air War Plans Division"}
        self.assertEqual(focusview.title_focus_id("WAR PLANS DIVISION", names), "F")
        self.assertEqual(focusview.title_focus_id("Air War Plans Division", names), "AIR")
        self.assertIsNone(focusview.title_focus_id("War Plans Divisio", names))

    def test_duplicate_localisations_are_not_ids(self):
        self.assertIsNone(focusview.title_focus_id("Rhineland", {"A": "Rhineland", "B": "Rhineland"}))
        self.assertIsNone(focusview.title_focus_id("", {"A": "Rhineland"}))

    def test_dynamic_name_needs_static_evidence_and_unique_match(self):
        names = {"A": "Establish [GetCompany] Works"}
        self.assertEqual(focusview.title_focus_id("Establish Steel Works", names), "A")
        names["B"] = "Establish Steel Works"
        self.assertIsNone(focusview.title_focus_id("Establish Steel Works", names))
        self.assertIsNone(focusview.title_focus_id("Anything", {"A": "[GetName]"}))


class FocusSelectionIdentityTests(unittest.TestCase):
    def setUp(self):
        self.session = mock.Mock(mode="campaign")
        self.session.lock.hold.return_value = nullcontext()
        self.args = {"action": "select", "focus": "F", "save": "latest", "force": False,
                     "replace": False, "if_idle": False, "keep_open": False}
        self.before = {"current": None, "text": "No national focus set"}
        self.after = {"current": "War Plans Division", "text": "War Plans Division"}
        self.detail = {"title": "War Plans Division", "start_enabled": True, "unmet": [], "days": 70}
        self.row = {"id": "F", "name": "War Plans Division", "status": "available", "days": 70,
                    "prerequisites": [], "mutually_exclusive": [], "requires": [], "effects": [], "war_with": []}

    def run_action(self):
        stack = ExitStack()
        self.addCleanup(stack.close)
        def patch(obj, name, **kw):
            return stack.enter_context(mock.patch.object(obj, name, **kw))
        patch(tool, "_focus_context", return_value=("GER", "tree", {"F": {}, "AIR": {}}, {}, {}))
        patch(tool.cc, "names", return_value={"F": "War Plans Division", "AIR": "Air War Plans Division"})
        patch(tool.focus_data, "plan", return_value=[self.row])
        patch(tool.cc, "finish", side_effect=lambda session, summary, data, *a, **k: data)
        patch(tool.cc, "fail", side_effect=ToolError("verification failed"))
        patch(tool.panels, "open_panel")
        patch(tool.panels, "current", return_value="politics")
        patch(tool.panels, "close")
        patch(tool.focusview, "open_tree")
        patch(tool.focusview, "close_detail")
        patch(tool.focusview, "open_detail", return_value=(self.detail, [], []))
        patch(tool.focusview, "read_button", side_effect=[self.before, self.after])
        self.start = patch(tool.focusview, "start")
        stack.enter_context(mock.patch("hoi4_live.state.focustimer.start"))
        return tool.hoi4_focus.__wrapped__(self.session, self.args)

    def test_success_records_verified_id(self):
        result = self.run_action()
        self.assertEqual(result["verified_focus_id"], "F")
        self.start.assert_called_once()

    def test_unread_current_never_means_idle(self):
        self.before = {"current": None, "text": "", "unread": True}
        with self.assertRaises(ToolError):
            self.run_action()
        self.start.assert_not_called()

    def test_name_and_force_are_rejected_before_click(self):
        self.args["focus"] = "War Plans Division"
        with self.assertRaises(ToolError):
            self.run_action()
        self.start.assert_not_called()

    def test_force_cannot_skip_requirements(self):
        self.args["force"] = True
        with self.assertRaises(ToolError):
            self.run_action()
        self.start.assert_not_called()

    def test_wrong_detail_or_unknown_start_is_not_clicked(self):
        self.detail["title"] = "Air War Plans Division"
        with self.assertRaises(ToolError):
            self.run_action()
        self.start.assert_not_called()

    def test_unread_start_is_not_clicked(self):
        self.detail["start_enabled"] = None
        with self.assertRaises(ToolError):
            self.run_action()
        self.start.assert_not_called()

    def test_nearby_focus_after_click_is_not_success(self):
        self.after = {"current": "Air War Plans Division", "text": "Air War Plans Division"}
        with self.assertRaises(ToolError):
            self.run_action()
        self.start.assert_called_once()

    def test_idempotent_retry_does_not_click_start(self):
        self.before = self.after
        result = self.run_action()
        self.assertFalse(result["changed"])
        self.start.assert_not_called()


class FocusContextFreshnessTests(unittest.TestCase):
    def context(self, observed_date="1936.1.2.1", screen_date="1936.1.2.12", present=True):
        session = mock.Mock(mode="campaign")
        observed = {"tag": "GER", "day": 2, "tree": "live_tree", "completed": ["F"],
                    "as_of_date": observed_date, "current": None, "current_known": False,
                    "progress": {"OTHER": {"lower_exclusive": 0.1}}}
        session.telemetry.snapshot.return_value = {"n": 2, "focus": observed} if present else None
        with mock.patch.object(tool.cc, "player_tag", return_value="GER"), \
                mock.patch.object(tool.cc, "save_model") as saved, \
                mock.patch.object(tool.focus_data, "for_session", return_value={}), \
                mock.patch.object(tool.focus_data, "tree_for_tag", return_value="static_tree"), \
                mock.patch.object(tool.focus_data, "tree_focuses", return_value={}), \
                mock.patch("hoi4_live.game.timectl.read_date", return_value=({"hoi4": screen_date}, None)):
            result = tool._focus_context(session, None, "telemetry")
            saved.assert_not_called()
            return result

    def test_fresh_closed_observation_supplies_tree_and_completed_not_current(self):
        _, tree, _, data, src = self.context()
        self.assertEqual(tree, "live_tree")
        self.assertEqual(data["completed"], ["F"])
        self.assertFalse(data["current_known"])
        self.assertIsNone(data["current"])
        self.assertEqual(src["focus"], "telemetry")

    def test_old_future_missing_or_unknown_date_never_falls_back_to_unrelated_save(self):
        for kwargs in ({"observed_date": "1936.1.1.1"},
                       {"observed_date": "1936.1.3.1"},
                       {"screen_date": None}, {"present": False}):
            with self.subTest(kwargs=kwargs):
                _, tree, _, data, src = self.context(**kwargs)
                self.assertEqual(tree, "static_tree")
                self.assertEqual(data, {})
                self.assertEqual(src["focus"], "unknown")


if __name__ == "__main__":
    unittest.main()
