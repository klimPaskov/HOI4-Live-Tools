"""Focus scheduling uses closed batches plus ordinary UI preflight, not guessed idle state."""
import tempfile
import unittest
from unittest import mock

import helpers  # noqa: F401
import test_governor_runtime as runtime
from hoi4_live.errors import ToolError
from hoi4_live.game.governor import Governor, telemetry_snapshot
from hoi4_live.state.governor import StandingPlan, daily_events


def snapshot(day=100, completed=None, **fields):
    focus = dict(source='telemetry', as_of_day=day, tree='tree', generation='g',
                 selection_preflight=True, catalog_ids=['A', 'B'], preflight_candidates=['A', 'B'],
                 completed=completed or [], progress={}, current=None, current_known=False,
                 availability_verified=False)
    focus.update(fields)
    return dict(day=day, as_of_day=day, source='telemetry', tag='GER', focus=focus)


class FocusScheduleTests(unittest.TestCase):
    def test_completion_waits_do_not_cross_generation_or_tree(self):
        before = snapshot(100)
        self.assertIn('focus_done', daily_events(before, snapshot(101, completed=['A'])))
        for fields in ({'generation': 'other'}, {'tree': 'other'}, {'tag': 'POL'}):
            self.assertNotIn('focus_done', daily_events(before, snapshot(101, completed=['A'], **fields)))

    def test_preflight_is_explicit_and_does_not_claim_idle_or_available(self):
        plan = StandingPlan({'focus': ['A', 'B']})
        observed = snapshot()
        action = plan.reconcile(observed)['actions'][0]
        self.assertEqual(action['args'], {'action': 'select', 'focus': 'A', 'if_idle': True})
        self.assertEqual(action['reason'], 'focus_selection_preflight')
        self.assertNotIn('idle', observed['focus'])
        self.assertNotIn('available', observed['focus'])
        self.assertFalse(observed['focus']['current_known'])

    def test_missing_stale_or_unverified_candidates_never_authorize_preflight(self):
        for fields in ({'selection_preflight': False}, {'preflight_candidates': None},
                       {'completed': None}, {'as_of_day': 99}):
            observed = snapshot()
            observed['focus'].update(fields)
            self.assertEqual(StandingPlan({'focus': ['A']}).reconcile(observed)['actions'], [])

    def test_exact_success_waits_then_completion_schedules_next_preference(self):
        plan = StandingPlan({'focus': ['A', 'B']})
        action = plan.reconcile(snapshot())['actions'][0]
        status = plan.record(action['id'], {'structuredContent': {'verified_focus_id': 'A'}}, 100)
        self.assertEqual(status, 'succeeded')
        self.assertEqual(plan.reconcile(snapshot(101))['actions'], [])
        result = plan.reconcile(snapshot(102, completed=['A']))
        self.assertEqual(result['actions'][0]['args']['focus'], 'B')
        self.assertEqual(plan.plan['focus'], ['A', 'B'])

    def test_unverified_or_wrong_success_is_failure_not_consumption(self):
        for data in ({}, {'verified_focus_id': 'B'}):
            plan = StandingPlan({'focus': ['A']})
            action = plan.reconcile(snapshot())['actions'][0]
            self.assertEqual(plan.record(action['id'], {'structuredContent': data}, 100), 'failed')
            self.assertNotIn('focus_watch', plan.state)
            self.assertEqual(plan.state['tool_failures'], 1)
            self.assertEqual(plan.plan['focus'], ['A'])

    def test_verified_busy_deferrals_do_not_exhaust_failure_budget(self):
        plan = StandingPlan({'focus': ['A'], 'retry': {'max_attempts': 1}})
        for day in (100, 107, 114, 121):
            result = plan.reconcile(snapshot(day))
            self.assertEqual(result['stop_reasons'], [])
            action = result['actions'][0]
            self.assertEqual(plan.record(action['id'], {'structuredContent': {
                'deferred': True, 'verified_current_focus_id': 'B'}}, day), 'deferred')
        self.assertEqual(plan.state['tool_failures'], 0)
        self.assertEqual(plan.reconcile(snapshot(122))['actions'], [])
        self.assertEqual(plan.reconcile(snapshot(123, completed=['B']))['actions'][0]['args']['focus'], 'A')

    def test_unknown_busy_identity_is_not_trusted(self):
        plan = StandingPlan({'focus': ['A']})
        action = plan.reconcile(snapshot())['actions'][0]
        self.assertEqual(plan.record(action['id'], {'structuredContent': {'deferred': True}}, 100), 'failed')
        self.assertNotIn('focus_watch', plan.state)

    def test_progress_postpones_recheck_without_claiming_active(self):
        plan = StandingPlan({'focus': ['A']})
        action = plan.reconcile(snapshot())['actions'][0]
        plan.record(action['id'], {'structuredContent': {'verified_focus_id': 'A'}}, 100)
        observed = snapshot(106, progress={'A': {'lower_exclusive': .1}})
        self.assertEqual(plan.reconcile(observed)['actions'], [])
        self.assertFalse(observed['focus']['current_known'])
        self.assertEqual(plan.reconcile(snapshot(112))['actions'], [])
        self.assertEqual(plan.reconcile(snapshot(113))['actions'][0]['args']['focus'], 'A')

    def test_restart_preserves_watch_but_generation_change_revokes_it(self):
        plan = StandingPlan({'focus': ['A']})
        action = plan.reconcile(snapshot())['actions'][0]
        plan.record(action['id'], {'structuredContent': {'verified_focus_id': 'A'}}, 100)
        restored = StandingPlan(plan.plan, plan.state)
        self.assertEqual(restored.reconcile(snapshot(101))['actions'], [])
        self.assertEqual(restored.reconcile(snapshot(102, generation='new'))['actions'][0]['args']['focus'], 'A')


class FocusRuntimeObservationTests(unittest.TestCase):
    def setUp(self):
        folder = tempfile.TemporaryDirectory()
        self.addCleanup(folder.cleanup)
        self.session = runtime.FakeSession(folder.name)
        for target, value in (
            ('hoi4_live.gamedata.tech.for_session', {'techs': {}}),
            ('hoi4_live.gamedata.focus.for_session', {}),
            ('hoi4_live.gamedata.focus.tree_focuses', {
                'A': {'id': 'A'}, 'B': {'id': 'B', 'prereq': [['A']]}})):
            patch = mock.patch(target, return_value=value)
            patch.start(); self.addCleanup(patch.stop)

    def batch(self):
        store = self.session.store
        store.set_focus_catalog({'tree': ['A', 'B']}, 'test-generation')
        store.add(runtime.day(102))
        for kind, extra in (('fb', {'total': 2}), ('fc', {'fid': 0}),
                            ('fe', {'completed': 1, 'progressed': 0})):
            store.add(dict(k=kind, t='GER', n=102, g='test-generation', v=3, tree=0,
                           _date='1936.1.1.24', **extra))
        store.add(runtime.day(103))

    def test_completed_batch_produces_structural_candidates_only(self):
        self.batch()
        observed = telemetry_snapshot(self.session)['focus']
        self.assertEqual(observed['completed'], ['A'])
        self.assertEqual(observed['preflight_candidates'], ['B'])
        self.assertFalse(observed['current_known'])
        self.assertFalse(observed['availability_verified'])
        self.assertNotIn('idle', observed)

    def test_foreign_generation_and_missing_batch_stay_unknown(self):
        self.assertNotIn('focus', telemetry_snapshot(self.session))
        self.batch()
        self.session.store.history['GER'][-1]['g'] = 'foreign'
        self.assertNotIn('focus', telemetry_snapshot(self.session))

    def test_start_requires_closed_batch_and_current_tree_ids(self):
        governor = Governor(self.session)
        governor.set_plan({'focus': ['A']})
        with self.assertRaisesRegex(ToolError, 'complete current-tree'):
            governor.start()
        self.batch()
        governor.set_plan({'focus': ['UNKNOWN']})
        with self.assertRaisesRegex(ToolError, 'outside the observed'):
            governor.start()
        self.assertFalse(governor.active())

    def test_daily_runtime_executes_one_verified_preflight_and_keeps_pending_plan(self):
        self.batch()
        governor = Governor(self.session)
        self.session.governor = governor
        governor.set_plan({'focus': ['B']})
        calls = []
        def dispatch(session, name, args):
            calls.append((name, args))
            if name == 'hoi4_advance':
                session.clock = runtime.FakeRun()
                return {'isError': False}
            return {'structuredContent': {'verified_focus_id': 'B'}}
        with mock.patch('hoi4_live.tools.dispatch.call', side_effect=dispatch), \
                mock.patch('hoi4_live.perception.popups.detect', return_value=[]):
            governor.start()
            governor.on_day(self.session.clock, {})
            governor.on_day(self.session.clock, {})
        selections = [args for name, args in calls if name == 'hoi4_focus']
        self.assertEqual(selections, [{'action': 'select', 'focus': 'B', 'if_idle': True, 'save': 'telemetry'}])
        self.assertEqual(governor.policy.plan['focus'], ['B'])
        self.assertEqual(self.session.clock.run['auto'][0]['status'], 'succeeded')

    def test_active_plan_update_rejects_unknown_focus_before_replacement(self):
        self.batch()
        governor = Governor(self.session)
        governor.set_plan({'focus': ['A']})
        governor._active = True
        with self.assertRaisesRegex(ToolError, 'outside the observed'):
            governor.set_plan({'focus': ['UNKNOWN']})
        self.assertEqual(governor.policy.plan['focus'], ['A'])


if __name__ == '__main__':
    unittest.main()
