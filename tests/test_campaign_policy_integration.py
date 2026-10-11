"""Campaign policy guards both stdio and direct background tool handlers."""
import os
import tempfile
import unittest
from unittest import mock

import helpers  # noqa: F401
from hoi4_live import config
from hoi4_live.errors import ModeRefused, ToolError
from hoi4_live.session import Session
from hoi4_live.tools import dispatch, military_tools, registry, session_tools


class CampaignPolicyIntegration(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.backend = mock.Mock()
        self.backend.make_window.return_value.find.return_value = None
        self.cfg = config.load(overrides={
            'mode': 'campaign', 'game_dir': self.tmp.name, 'user_dir': self.tmp.name,
            'artifacts_dir': os.path.join(self.tmp.name, 'artifacts'),
            'lock': {'path': os.path.join(self.tmp.name, 'lock'), 'wait_seconds': 0}})
        self.session = Session(self.cfg, backend=self.backend)
        self.addCleanup(self.session.shutdown)
        self.session._telemetry = mock.Mock()
        self.session._telemetry.player.return_value = 'GER'
        processes = mock.patch.object(session_tools.process, 'running_pids', return_value=[])
        processes.start()
        self.addCleanup(processes.stop)

    def test_campaign_is_owned_and_fair_immediately(self):
        self.assertTrue(self.session.owned_process_only)
        self.assertEqual(self.session.play_policy.mode, 'fair')
        self.assertIs(self.session.lock.backend, self.backend)

    def test_campaign_test_escape_is_refused_in_both_entrypoints(self):
        args = {'action': 'set_mode', 'mode': 'test'}
        result = dispatch.call(self.session, 'hoi4_lock', args)
        self.assertTrue(result['isError'])
        with self.assertRaises(ModeRefused):
            session_tools.hoi4_lock(self.session, args)
        self.assertEqual(self.session.mode, 'campaign')
        with self.assertRaises(ModeRefused):
            self.session.set_mode('test')

    def test_direct_enemy_country_access_cannot_bypass_visibility(self):
        with mock.patch.object(self.session, 'require_owned_game') as owned, \
             mock.patch.object(type(self.session), 'telemetry', new_callable=mock.PropertyMock,
                               return_value=self.session._telemetry):
            with self.assertRaises(ModeRefused):
                military_tools.hoi4_army(self.session, {'action': 'list', 'tag': 'SOV'})
        owned.assert_not_called()

    def test_direct_acting_handler_checks_ownership_before_ui(self):
        with self.assertRaises(ToolError) as error:
            military_tools.hoi4_army(self.session, {'action': 'create_army', 'name': 'Home'})
        self.assertEqual(error.exception.details.get('error'), 'unowned_process')
        self.session.input.click.assert_not_called()

    def test_configured_debug_flags_are_refused_before_launch(self):
        self.cfg.data['launch']['args'] = ['-debug']
        with mock.patch.object(session_tools.process, 'launch') as launch:
            with self.assertRaises(ModeRefused):
                session_tools.hoi4_launch(self.session, {'wait_menu': False})
        launch.assert_not_called()
        self.assertIsNone(self.session.launched_pid)

    def test_readonly_status_needs_no_owned_game_and_direct_defaults_apply(self):
        with mock.patch.object(session_tools.process, 'running_pids', return_value=[]), \
                mock.patch.object(self.session, 'require_owned_game') as owned:
            direct = session_tools.hoi4_status(self.session, {'look': False})
            result = dispatch.call(self.session, 'hoi4_status', {'look': False})
        owned.assert_not_called()
        self.assertFalse(result['isError'])
        self.assertEqual(direct.data['mode'], 'campaign')
        self.assertEqual(result['structuredContent']['play_mode'], 'fair')
        self.assertEqual(result['structuredContent']['policy_sha256'], self.session.play_policy.sha256)


if __name__ == '__main__':
    unittest.main()
