"""Live-speed contract: observations and declared routine work never pause."""
import unittest
from unittest import mock

from helpers import FIXTURES  # noqa: F401
from hoi4_live.errors import ModeRefused
from hoi4_live.game import clock
from hoi4_live.protocol.schema import validate
from hoi4_live.tools import front_tools, registry
import test_clock as fixtures


class LiveNoPauseTests(unittest.TestCase):
    def clock_case(self):
        case = fixtures.ClockCase()
        case.setUp()
        self.addCleanup(case.doCleanups)
        self.addCleanup(case.tearDown)
        return case

    def test_policy_answer_happens_while_running_without_pause_toggle(self):
        case = self.clock_case()
        case.feed()
        case.detected.append(object())
        states = []

        def handle(*args, **kwargs):
            states.append(case.state['running'])
            case.detected.clear()
            return [{'kind': 'news', 'title': 'News', 'answered': True}]

        with mock.patch.object(clock.game_popups, 'handle_all', side_effect=handle):
            run, result = case.run_loop(budget_days=4)
        self.assertEqual(states, [True])
        self.assertEqual(case.session.input.keys, [])
        self.assertTrue(case.state['running'])
        self.assertEqual(result['data']['agent_pause_count'], 0)
        self.assertEqual(result['data']['agent_pause_seconds_in_run'], 0)

    def test_wall_deadline_returns_without_pausing(self):
        case = self.clock_case()
        _, result = case.run_loop(budget_days=30, max_seconds=0.08)
        self.assertEqual(result['reasons'][0]['code'], 'time_limit')
        self.assertTrue(case.state['running'])
        self.assertEqual(result['data']['agent_pause_count'], 0)
        self.assertIsNone(result['data']['wall_seconds_per_game_year'])

    def test_unresolved_popup_records_blocking_pause(self):
        case = self.clock_case()
        case.feed()
        case.detected.append(object())
        with mock.patch.object(clock.game_popups, 'handle_all', return_value=[
                {'kind': 'event', 'title': 'Choose', 'answered': False}]):
            _, result = case.run_loop(budget_days=30)
        self.assertFalse(case.state['running'])
        self.assertEqual(result['data']['agent_pause_count'], 1)
        self.assertEqual(result['data']['agent_pause_events'][0]['reason'], 'popup')
        self.assertTrue(result['data']['agent_pause_ongoing'])

    def test_campaign_fresh_front_refuses_before_save_or_ui(self):
        session = mock.Mock(mode='campaign')
        with mock.patch.object(front_tools.saves, 'save') as save:
            with self.assertRaises(ModeRefused):
                front_tools.read_situation(session, 'GER', fresh=True)
        save.assert_not_called()
        session.telemetry.snapshot.assert_not_called()

    def test_unknown_clock_never_sends_a_toggle(self):
        session = mock.Mock()
        with mock.patch.object(clock, 'icon_state', return_value='unknown'):
            self.assertFalse(clock.set_running(session, True))
            self.assertFalse(clock.set_running(session, False))
        session.input.key.assert_not_called()

    def test_standing_rule_schema_defaults_to_running(self):
        args = validate(registry.get('hoi4_standing').schema, {'action': 'set'})
        self.assertFalse(args['pause'])


if __name__ == '__main__':
    unittest.main()
