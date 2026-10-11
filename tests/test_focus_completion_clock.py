"""A predicted deadline or generic notice is not campaign completion evidence."""
import unittest
from unittest import mock
import helpers  # noqa: F401
from hoi4_live.errors import ToolError
from hoi4_live.game import clock
from test_clock import ClockCase


def batch(day, completed=None, **extra):
    value = dict(tag='GER', tree='tree', generation='g', day=day, source='telemetry',
                 completed=completed or [])
    value.update(extra)
    return value


class FocusCompletionTests(unittest.TestCase):
    def test_closed_completion_transition_preserves_ids_and_date(self):
        event = clock.focus_completion_event(batch(10, ['A']), batch(11, ['A', 'B']))
        self.assertEqual(event['focus_ids'], ['B'])
        self.assertEqual(event['as_of_day'], 11)
        self.assertIsNone(clock.focus_completion_event(batch(11, ['A']), batch(12, ['A'])))

    def test_unknown_timeline_change_and_regression_do_not_invent_completion(self):
        for current in (None, batch(10, ['B']), batch(9, ['B']), batch(11, ['B']),
                        batch(11, ['A', 'B'], generation='other'),
                        batch(11, ['A', 'B'], tag='POL'), batch(11, ['A', 'B'], tree='other'),
                        batch(11, ['A', 'B'], source='timer')):
            self.assertIsNone(clock.focus_completion_event(batch(10, ['A']), current))


class CampaignFocusClockTests(ClockCase):
    def test_campaign_never_uses_estimated_timer_or_focus_notice_as_fact(self):
        self.session.mode = 'campaign'
        self.dismissed = ['National Focus Completed']
        self.state['paused_by_game'] = True
        self.feed()
        with mock.patch.object(clock.focustimer, 'due') as timer:
            _, result = self.run_loop(budget_days=3, stop_on=['focus_done'])
        timer.assert_not_called()
        self.assertEqual(result['reasons'][0]['code'], 'done')

    def test_campaign_idle_is_unavailable_before_starting_clock(self):
        self.session.mode = 'campaign'
        with self.assertRaisesRegex(ToolError, 'verified UI'):
            self.run_loop(stop_on=['focus_idle'])
        self.assertFalse(self.state['running'])
        self.state['running'] = True
        with self.assertRaisesRegex(ToolError, 'verified UI'):
            self.run_loop(stop_on=['focus_idle'])
        self.assertTrue(self.state['running'])

    def test_campaign_completion_stops_on_closed_batch_and_leaves_running(self):
        self.session.mode = 'campaign'
        self.feed()
        def observed(tag, day):
            return batch(day, ['A'] if day >= 101 else [])
        with mock.patch.object(self.store, 'focus_snapshot', side_effect=observed):
            _, result = self.run_loop(budget_days=6, stop_on=['focus_done'])
        self.assertEqual(result['reasons'][0]['code'], 'focus_done')
        self.assertEqual(result['reasons'][0]['focus_ids'], ['A'])
        self.assertTrue(self.state['running'])


if __name__ == '__main__':
    unittest.main()
