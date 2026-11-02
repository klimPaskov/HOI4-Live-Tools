"""Pre-game review does not accidentally start a campaign or issue console input."""
import json
from pathlib import Path
from types import SimpleNamespace
import tempfile
import unittest
from unittest import mock

import helpers  # noqa: F401
from hoi4_live.errors import ToolError
from hoi4_live.game import menu
from hoi4_live.telemetry import modgen, playset
from hoi4_live.tools import session_tools


class MenuPreparationTests(unittest.TestCase):
    def test_prepare_stops_before_start_or_date_read(self):
        session = mock.Mock()
        lines = SimpleNamespace(lines=[SimpleNamespace(text="START"), SimpleNamespace(text="Back to menu")])
        with mock.patch.object(menu.ui, "classify", return_value=({"screen": "unknown"}, None)), \
                mock.patch.object(menu.ui, "ocr_screen", return_value=(None, lines)), \
                mock.patch.object(menu.ui, "click_text") as click, \
                mock.patch("hoi4_live.perception.topbar.read_date") as date:
            result = menu.new_game(session, start=False)
        self.assertTrue(result["prepared"])
        self.assertFalse(result["started"])
        click.assert_not_called()
        date.assert_not_called()

    def test_default_still_starts_and_waits_for_date(self):
        session = mock.Mock()
        lines = SimpleNamespace(lines=[SimpleNamespace(text="START"), SimpleNamespace(text="Back to menu")])
        with mock.patch.object(menu.ui, "classify", return_value=({"screen": "unknown"}, None)), \
                mock.patch.object(menu.ui, "ocr_screen", return_value=(None, lines)), \
                mock.patch.object(menu.ui, "click_text") as click, \
                mock.patch("hoi4_live.perception.topbar.read_date", return_value={"hoi4": "1936.1.1.12"}):
            result = menu.new_game(session)
        click.assert_called_once_with(session, "start", exact=True, prefer="bottom")
        self.assertEqual(result["date"], "1936.1.1.12")

    def test_start_is_clicked_again_while_the_ready_screen_stays(self):
        # seen live on 1.19.4.1: a START click sometimes did not register and the game waited on the ready screen
        session = mock.Mock()
        lines = SimpleNamespace(lines=[SimpleNamespace(text="START"), SimpleNamespace(text="Back to menu")])
        dates = [{"error": "no date"}, {"error": "no date"}, {"hoi4": "1936.1.1.12"}]
        with mock.patch.object(menu.ui, "classify", return_value=({"screen": "unknown"}, None)), \
                mock.patch.object(menu.ui, "ocr_screen", return_value=(None, lines)), \
                mock.patch.object(menu.ui, "click_text") as click, \
                mock.patch.object(menu, "START_RETRY_SECONDS", -1), \
                mock.patch("hoi4_live.perception.topbar.read_date", side_effect=dates):
            result = menu.new_game(session)
        self.assertEqual(click.call_count, 3)
        self.assertEqual(result["date"], "1936.1.1.12")

    def test_rules_review_requires_ready_and_never_starts(self):
        session = mock.Mock()
        with mock.patch.object(menu.ui, "find_text", return_value=(None, [], None)), \
                mock.patch.object(menu.ui, "click_text") as click:
            with self.assertRaises(ToolError):
                menu.review_setup_rules(session, reset=True)
        click.assert_not_called()

    def test_rules_reset_uses_exact_buttons_and_returns_evidence(self):
        session = mock.Mock()
        session.artifact.return_value = "rules.png"
        with mock.patch.object(menu.ui, "find_text", return_value=(None, [object()], None)), \
                mock.patch.object(menu.ui, "click_text") as click, \
                mock.patch.object(menu, "_wait_text"), \
                mock.patch.object(menu.ui, "ocr_screen", return_value=(None, SimpleNamespace(lines=[]))):
            result = menu.review_setup_rules(session, reset=True)
        self.assertEqual([call.args[1] for call in click.call_args_list],
                         [r"custom\s*game\s*rules", "reset", "apply"])
        self.assertTrue(all(call.kwargs.get("exact") for call in click.call_args_list))
        self.assertTrue(result["reset_requested"])
        self.assertFalse(result["all_rule_values_verified"])
        self.assertEqual(result["screen"], "rules.png")

    def test_rules_and_start_rejected_before_menu_navigation(self):
        session = mock.Mock(mode="campaign")
        with mock.patch.object(session_tools, "apply_mode"), mock.patch.object(menu, "new_game") as start:
            with self.assertRaises(ToolError):
                session_tools.hoi4_new_game.__wrapped__(session, {"rules": "reset_defaults", "start": True})
        start.assert_not_called()

    def test_main_menu_quit_does_not_press_escape(self):
        session = mock.Mock()
        with mock.patch.object(menu.ui, "classify", return_value=({"screen": "main_menu"}, None)), \
                mock.patch.object(menu.ui, "click_text") as click, \
                mock.patch.object(menu, "open_esc_menu") as escape:
            result = menu.quit_to_desktop(session)
        self.assertTrue(result["exit_requested"])
        click.assert_called_once_with(session, "quit", exact=True)
        escape.assert_not_called()

    def test_exit_confirmation_is_read_with_the_cursor_above_the_dialog(self):
        # -debug shows a GUI-inspector tooltip under the cursor; on 1.19.4.1 it covered the exit dialog's OK button
        session = mock.Mock()
        session.window.size.return_value = (1920, 1080)
        events = []
        session.input.move.side_effect = lambda x, y: events.append(("move", x, y))
        ok = SimpleNamespace(text="OK", center=(1052, 677))

        def read(*args, **kwargs):
            events.append(("read",))
            return None, SimpleNamespace(lines=[ok])
        session.grab.return_value = mock.Mock(shape=(1080, 1920, 3))
        with mock.patch.object(menu.ui, "ocr_screen", side_effect=read):
            self.assertEqual(menu._click_ok(session), "OK")
        self.assertEqual(events[0], ("move", 960, 129))
        self.assertEqual(events[1], ("read",))
        session.input.click.assert_called_once_with(1052, 677)

    def test_prepared_tool_does_not_read_map_or_telemetry(self):
        session = mock.Mock(mode="campaign")
        session.window.find.return_value = None
        with mock.patch.object(session_tools, "apply_mode"), \
                mock.patch.object(menu, "new_game", return_value={"prepared": True, "started": False}) as prepare, \
                mock.patch.object(session_tools.timectl, "read_date") as date, \
                mock.patch.object(session_tools, "attach", side_effect=lambda result, *args: result):
            result = session_tools.hoi4_new_game.__wrapped__(session, {"start": False})
        prepare.assert_called_once_with(session, country=None, bookmark=None, start=False)
        self.assertTrue(result.data["prepared"])
        date.assert_not_called()
        session.telemetry.drop_after.assert_not_called()


class CampaignPlaysetTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.cfg = SimpleNamespace(user_dir=self.tmp.name)
        self.path = Path(playset.dlc_load_path(self.tmp.name))

    def test_missing_or_empty_playset_is_vanilla(self):
        session_tools._validate_campaign_playset(self.cfg)
        self.path.write_text(json.dumps({"enabled_mods": []}))
        session_tools._validate_campaign_playset(self.cfg)

    def test_other_or_malformed_mods_refused_without_name_disclosure(self):
        for enabled in (["mod/private_example.mod"], [playset.ENTRY, "mod/private_example.mod"], "invalid", None):
            self.path.write_text(json.dumps({"enabled_mods": enabled}))
            with self.assertRaises(ToolError) as error:
                session_tools._validate_campaign_playset(self.cfg)
            self.assertNotIn("private_example", str(error.exception))
        self.path.write_text("{broken")
        with self.assertRaises(ToolError):
            session_tools._validate_campaign_playset(self.cfg)

    def test_only_campaign_generated_descriptor_is_accepted(self):
        self.path.write_text(json.dumps({"enabled_mods": [playset.ENTRY]}))
        with self.assertRaises(ToolError):
            session_tools._validate_campaign_playset(self.cfg)
        modgen.generate(self.tmp.name, None, campaign=True, focuses={}, techs=[])
        session_tools._validate_campaign_playset(self.cfg)
        Path(modgen.descriptor_path(self.tmp.name)).write_text('path="other"\n')
        with self.assertRaises(ToolError):
            session_tools._validate_campaign_playset(self.cfg)


if __name__ == "__main__":
    unittest.main()
