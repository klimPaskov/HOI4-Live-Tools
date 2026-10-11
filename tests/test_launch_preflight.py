"""A known locked desktop must fail before a launch or configuration mutation."""
import unittest
from unittest.mock import Mock, patch
from types import SimpleNamespace

import helpers
from hoi4_live.errors import CapabilityUnavailable
from hoi4_live.tools import session_tools
from hoi4_live.platform.macos import MacOSPlatform


class LaunchPreflightTests(unittest.TestCase):
    def test_mac_launch_checks_interaction_before_capture(self):
        native = Mock()
        native.permission.side_effect = CapabilityUnavailable('input', 'macos', 'desktop is locked')
        with patch('hoi4_live.platform.macos_desktop.Native', return_value=native):
            with self.assertRaises(CapabilityUnavailable):
                MacOSPlatform().preflight_game_launch()
        native.permission.assert_called_once_with('input')
        native.displays.assert_not_called()

    def test_preflight_refusal_precedes_backup_and_process_launch(self):
        s = SimpleNamespace(mode='campaign',
            cfg={'window': {'process_name':'hoi4'}, 'launch': {'args': [], 'debug_args':['-debug']}},
            window=Mock(), platform=Mock(), play_policy=Mock(), prepare_config_session=Mock())
        s.window.find.return_value = False
        s.cfg = type('Config', (dict,), {})(s.cfg)
        s.cfg.user_dir = 'unused'
        s.platform.preflight_game_launch.side_effect = CapabilityUnavailable('input','macos','desktop is locked')
        args={'restart':False,'debug':False,'wait_menu':False,'timeout_s':60}
        with patch.object(session_tools.process,'running_pids',return_value=[]), \
             patch.object(session_tools.process,'crash_dirs',return_value=set()), \
             patch.object(session_tools.process,'launch') as launch:
            with self.assertRaises(CapabilityUnavailable):
                session_tools.hoi4_launch(s,args)
        launch.assert_not_called()
        s.prepare_config_session.assert_not_called()
