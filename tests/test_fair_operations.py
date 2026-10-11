"""Deterministic FAIR checks from trusted snapshots, never live acceptance."""
from dataclasses import replace
import copy
import unittest

from hoi4_live.errors import ModeRefused
from hoi4_live.game.play_policy import LandObservation, PlayPolicy


class FairOperationTests(unittest.TestCase):
    def setUp(self):
        self.policy = PlayPolicy(session_id='run', monotonic=lambda: 100)
        self.observation = LandObservation('run', 'GER', 706650, 99, 'own_country_telemetry',
            ({'id': 41, 'name': 'Army 1', 'owner': 'GER', 'template_ids': [3],
              'order_types': ['front_line', 'offensive']},),
            tuple({'id': n, 'controller': 'POL', 'visible': True} for n in range(1, 8)),
            ({'id': 3, 'owner': 'GER', 'complete': True,
              'line': [{'type': 'infantry', 'role': 'leg_infantry', 'count': 9}]},))

    def args(self, action='front', target='1', **values):
        return dict({'action': action, 'army': 'Army 1', 'target': target}, **values)

    def authorize(self, args=None, observation=None):
        return self.policy.authorize('hoi4_orders', args or self.args(),
                                     observation=observation or self.observation)

    def refused(self, args=None, observation=None, reason=None):
        with self.assertRaises(ModeRefused) as error:
            self.authorize(args, observation)
        if reason:
            self.assertEqual(error.exception.details['reason'], reason)

    def test_normal_front_offensive_and_execute_are_reviewable(self):
        for action in ['front', 'offensive', 'spearhead', 'execute']:
            self.assertTrue(self.authorize(self.args(action))['allowed'])
        self.assertEqual(self.policy.audit_events()[-1]['reason'], 'reviewed_action')

    def test_combined_arms_tanks_with_motorized_are_permitted(self):
        template = copy.deepcopy(self.observation.templates[0])
        template['line'] = [{'type': 'medium_armor', 'role': 'armor', 'count': 6},
                            {'type': 'motorized', 'role': 'motorized', 'count': 6}]
        self.assertTrue(self.authorize(observation=replace(self.observation, templates=(template,)))['allowed'])

    def test_space_marine_composition_is_refused(self):
        template = copy.deepcopy(self.observation.templates[0])
        template['line'].append({'type': 'heavy_armor', 'role': 'armor', 'count': 1})
        self.refused(observation=replace(self.observation, templates=(template,)), reason='fair_space_marines')

    def test_stale_future_foreign_session_and_unknown_source_are_refused(self):
        for obs in [replace(self.observation, observed_monotonic=94),
                    replace(self.observation, observed_monotonic=101),
                    replace(self.observation, session_id='other'),
                    replace(self.observation, source='model_intent')]:
            self.refused(observation=obs, reason='land_observation_untrusted_or_stale')

    def test_numeric_public_state_ids_are_required(self):
        self.refused(self.args(target='Warsaw'), reason='land_state_id_required')
        self.refused(self.args(target='999'), reason='land_target_unknown_or_not_visible')
        states = ({'id': 1, 'visible': False, 'controller': 'POL'},)
        self.refused(observation=replace(self.observation, states=states), reason='land_target_unknown_or_not_visible')

    def test_ambiguous_name_or_card_index_is_refused(self):
        army = dict(self.observation.armies[0], id=42)
        self.refused(observation=replace(self.observation, armies=self.observation.armies+(army,)),
                     reason='land_army_unknown_or_ambiguous')
        self.refused(self.args(army_index=1), reason='land_army_stable_binding_required')

    def test_incomplete_templates_and_foreign_armies_are_refused(self):
        self.refused(observation=replace(self.observation, templates=()), reason='land_templates_unknown')
        self.refused(observation=replace(self.observation, armies=(dict(self.observation.armies[0], owner='SOV'),)),
                     reason='land_foreign_units_excluded')

    def test_execute_cannot_hide_an_invasion_or_paradrop(self):
        for kind in ['naval_invasion', 'paradrop']:
            obs = replace(self.observation, armies=(dict(self.observation.armies[0], order_types=[kind]),))
            self.refused(self.args('execute'), obs, 'land_existing_orders_unknown_or_special')

    def test_identical_retries_do_not_consume_target_budget(self):
        for _ in range(12):
            self.assertTrue(self.authorize()['allowed'])
        for target in ['2', '3', '4']:
            self.authorize(self.args(target=target))
        self.refused(self.args(target='5'), reason='fair_land_target_churn_limit')
        self.assertTrue(self.authorize(self.args(target='5'), replace(self.observation, game_day=706651))['allowed'])

    def test_atomic_batch_rejection_does_not_consume_budget(self):
        self.refused({'action': 'batch', 'orders': [self.args(front_target='2'), self.args(front_target='2')]}, reason='land_duplicate_army_order')
        self.assertIsNone(self.policy._land_day)
        self.assertEqual(self.policy._land_targets, {})

    def test_timeline_rewind_requires_new_campaign_review(self):
        self.authorize()
        self.refused(observation=replace(self.observation, game_day=706649),
                     reason='land_campaign_identity_or_timeline_changed')

    def test_raw_dictionary_or_unreviewed_adapter_cannot_authorize(self):
        self.refused(observation={'all_fair': True}, reason='trusted_land_observation_required')
        self.refused(self.args('paint'), reason='land_action_adapter_unavailable')

    def test_meta_lifts_house_composition_check_but_not_cheats(self):
        meta = PlayPolicy('meta', session_id='run', monotonic=lambda: 100)
        self.assertTrue(meta.authorize('hoi4_orders', self.args())['allowed'])
        with self.assertRaises(ModeRefused):
            meta.authorize('hoi4_launch', {'debug': True}, observation=self.observation)


if __name__ == '__main__':
    unittest.main()
