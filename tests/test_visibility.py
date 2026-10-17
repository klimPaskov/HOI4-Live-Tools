"""Campaign visibility uses synthetic structured models and no live game files."""

from copy import deepcopy
from types import SimpleNamespace
import unittest
from unittest import mock

from hoi4_live.state import front
from hoi4_live.state.visibility import campaign_view


class VisibilityTests(unittest.TestCase):
    def test_snapshot_preserves_player_and_public_foreign_fields(self):
        snapshot = {"tag": "GER", "date": "1939.9.5", "day": {"k": "day", "t": "GER",
                    "mp_k": 150, "mil": 42, "div": 50, "enemy_str": 1.234, "ally_str": 2.456},
                    "wars": [{"k": "war", "t": "GER", "e": "POL", "name": "Poland", "sp": 0.2,
                              "div": 32, "civ": 12, "mil": 15, "mp_k": 98, "secret_new_force_stat": 123}],
                    "week": {"countries": [
                        {"k": "wc", "t": "GER", "c": "POL", "name": "Poland", "ideo": "neutrality",
                         "major": 0, "fleader": "ENG", "ally": 0, "nb": 1, "enemies": 1,
                         "mp_k": 75, "div": 33, "mil": 18, "stab": 0.9, "ws": 0.8,
                         "organisation": 65, "ai_strategy": {"GER": "invade"}, "future_secret": "hidden"},
                        {"k": "wc", "t": "GER", "c": "GER", "mp_k": 150, "mil": 42, "div": 50}],
                        "front": {"enemy": [{"s": 11, "name": "Warsaw", "ctrl": "POL", "owner": "POL"}]},
                        "enemy_pairs": [["GER", "POL"]]},
                    "watch": {"gvar": {"pol_ai_goal": 1}}, "events": [{"k": "ev", "e": "declare_war",
                    "a": "GER", "b": "POL", "n": 5}]}
        original = deepcopy(snapshot)
        view = campaign_view(snapshot, "GER")
        self.assertEqual(snapshot, original)
        self.assertEqual(view["day"]["mp_k"], 150)
        self.assertEqual(view["day"]["div"], 50)
        self.assertNotIn("enemy_str", view["day"])
        self.assertNotIn("ally_str", view["day"])
        self.assertEqual(view["wars"], [{"k": "war", "t": "GER", "e": "POL", "name": "Poland", "sp": 0.2}])
        foreign, own = view["week"]["countries"]
        self.assertEqual(foreign["name"], "Poland")
        self.assertEqual(foreign["ideo"], "neutrality")
        self.assertEqual(foreign["fleader"], "ENG")
        for hidden in ("mp_k", "div", "mil", "stab", "ws", "organisation", "ai_strategy", "future_secret"):
            self.assertNotIn(hidden, foreign)
        self.assertEqual(own["mil"], 42)
        self.assertEqual(view["week"]["front"], snapshot["week"]["front"])
        self.assertEqual(view["week"]["enemy_pairs"], [["GER", "POL"]])
        self.assertEqual(view["events"], snapshot["events"])
        self.assertNotIn("watch", view)

    def test_assembled_model_removes_hidden_wrappers_and_keeps_player_details(self):
        model = {"tag": "GER", "overview": {"manpower": 150000},
                 "military": {"divisions": 50, "average_organisation": 63.2,
                              "army_list": [{"name": "Heer A", "organisation": 63.2}]},
                 "economy": {"factories": {"military": 42}},
                 "diplomacy": {"enemy_strength_ratio": 1.2, "alliance_strength_ratio": 2.3,
                               "wars": [{"enemy": "POL", "enemy_name": "Poland", "since": "1939.9.1",
                                         "their_manpower": 345678, "their_organisation": 44}],
                               "_sources": {"telemetry": {"as_of": "1939.9.5", "fields": ["wars", "enemy_strength_ratio"]}}},
                 "world": {"countries": [{"c": "POL", "name": "Poland", "mil": 18, "div": 33}]},
                 "script": {"gflag": {"pol_ai_invade": 1}}, "_raw": {"save": "unparsed secret dump"},
                 "raw": {"telemetry": {"secret": 99}}, "AI_strategy": {"foreign_goal": "annex"}}
        view = campaign_view(model, "GER")
        self.assertEqual(view["overview"], model["overview"])
        self.assertEqual(view["military"], model["military"])
        self.assertEqual(view["economy"], model["economy"])
        self.assertEqual(view["diplomacy"]["wars"][0],
                         {"enemy": "POL", "enemy_name": "Poland", "since": "1939.9.1"})
        self.assertEqual(view["diplomacy"]["_sources"]["telemetry"]["fields"], ["wars"])
        for hidden in ("script", "_raw", "raw", "AI_strategy"):
            self.assertNotIn(hidden, view)

    def test_foreign_top_country_and_mapping_use_allowlists(self):
        foreign = {"tag": "POL", "overview": {"name": "Poland", "military_factories": 99},
                   "politics": {"ideology": "neutrality", "political_power": 100},
                   "military": {"divisions": 30}, "focus": {"current": "secret_attack_plan"},
                   "faction": {"name": "Allies", "leader": "ENG", "members": ["POL", "ENG"], "secret": 99}}
        view = campaign_view(foreign, "GER")
        self.assertEqual(view["overview"], {"name": "Poland"})
        self.assertEqual(view["politics"], {"ideology": "neutrality"})
        self.assertNotIn("military", view)
        self.assertNotIn("focus", view)
        self.assertNotIn("secret", view["faction"])
        countries = campaign_view({"countries": {"POL": {"name": "Poland", "mil": 99},
                                                "GER": {"mil": 42}}}, "GER")["countries"]
        self.assertEqual(countries, {"POL": {"name": "Poland"}, "GER": {"mil": 42}})

    def test_raw_foreign_day_watch_unknown_records_fail_closed(self):
        records = [{"k": "day", "t": "POL", "name": "Poland", "mp_k": 66, "mil": 88},
                   {"k": "watch", "t": "GER", "n": 3, "gvar": {"foreign_ai": 123}},
                   {"k": "future_kind", "t": "GER", "n": 3, "hidden_forces": 123},
                   {"k": "day", "t": "GER", "mp_k": 150, "div": 50}]
        view = campaign_view(records, "GER")
        self.assertNotIn("mp_k", view[0])
        self.assertNotIn("mil", view[0])
        self.assertEqual(view[1], {"k": "watch", "t": "GER", "n": 3})
        self.assertEqual(view[2], {"k": "future_kind", "t": "GER", "n": 3})
        self.assertEqual(view[3], records[3])

    def test_public_map_keeps_control_and_vps_without_foreign_state_economy(self):
        value = {"states": [{"state": 11, "name": "Warsaw", "owner": "POL", "controller": "POL",
                             "vp": 20, "mil": 8, "manpower": 34567}],
                 "touch": [{"state": 11, "enemy_divisions": 4, "enemy_org": 45, "name": "Warsaw"}]}
        view = campaign_view(value, "GER")
        self.assertEqual(view["states"], [{"state": 11, "name": "Warsaw", "owner": "POL",
                                          "controller": "POL", "vp": 20}])
        self.assertIsNone(view["touch"][0]["enemy_divisions"])
        self.assertIsNone(view["touch"][0]["enemy_org"])

    def test_requires_trusted_player_tag_and_is_idempotent(self):
        for tag in (None, "", "unknown", "*"):
            with self.assertRaises(ValueError):
                campaign_view({"mil": 42}, tag)
        value = {"tag": "GER", "world": {"countries": [{"c": "POL", "mil": 50, "name": "Poland"}]}}
        once = campaign_view(value, "GER")
        self.assertEqual(campaign_view(once, "GER"), once)


class FrontVisibilityTests(unittest.TestCase):
    def setUp(self):
        self.idx = {"states": {
            "10": {"owner": "GER", "provinces": [1], "anchor": [0, 0], "neighbours": [11], "vp": {}},
            "11": {"owner": "POL", "provinces": [2], "anchor": [10, 0], "neighbours": [10], "vp": {"2": 20}}},
            "provinces": {"1": {"state": 10, "pixels": 10}, "2": {"state": 11, "pixels": 10}}}
        self.sf = mock.Mock()
        self.sf.span_text.return_value = b""
        self.own_div = {"id": 1, "location": 1, "strength": 0.9, "organisation": 0.8, "executing": False}
        self.army = {"name": "Heer A", "division_ids": [1], "divisions": 1, "orders": [],
                     "executing_divisions": 0, "avg_strength": 0.9, "avg_organisation": 0.8}

    def situation(self, details=False):
        with mock.patch.object(front.military, "divisions", return_value=[self.own_div]) as divisions, \
                mock.patch.object(front.military, "armies", return_value={"armies": [self.army], "army_groups": []}):
            sit = front.situation(self.sf, "GER", self.idx, ["POL"], include_enemy_details=details)
        return sit, divisions

    def test_campaign_never_reads_foreign_divisions_and_unknown_is_none(self):
        sit, divisions = self.situation()
        divisions.assert_called_once_with(self.sf, "GER")
        self.assertEqual(sit["enemy"]["POL"], {"divisions": None, "avg_strength": None,
                         "avg_organisation": None, "top_states": [], "at_sea_or_unplaced": None})
        self.assertIsNone(sit["armies"][0]["touch"][0]["enemy_divisions"])
        self.assertIsNone(sit["armies"][0]["touch"][0]["enemy_org"])
        self.assertIsNone(sit["fronts"]["POL"][0]["enemy_divisions"])
        self.assertIsNone(sit["objectives"][0]["enemy_divisions"])
        self.assertEqual(sit["armies"][0]["avg_organisation"], 0.8)
        sit["_idx"] = self.idx
        self.assertEqual(front.suggest_orders(sit)["orders"][0]["target"], 11)

    def test_test_mode_still_supports_enemy_details(self):
        sit, divisions = self.situation(details=True)
        self.assertEqual([c.args[1] for c in divisions.call_args_list], ["GER", "POL"])
        self.assertEqual(sit["enemy"]["POL"]["divisions"], 1)

    def test_tool_campaign_forwarding_and_none_sort(self):
        from hoi4_live.tools import front_tools
        session = SimpleNamespace(mode="campaign", telemetry=mock.Mock())
        session.telemetry.snapshot.return_value = {}
        sf = mock.Mock()
        sf.header.return_value = {"date": "1939.9.5"}
        with mock.patch.object(front_tools.assemble, "find_save", return_value=("synthetic", None)), \
                mock.patch.object(front_tools.mapdata, "for_session", return_value=self.idx), \
                mock.patch.object(front_tools.queries, "open_save", return_value=sf), \
                mock.patch.object(front_tools.queries, "close_all"), \
                mock.patch.object(front_tools, "_enemies", return_value=["POL"]), \
                mock.patch.object(front_tools.cc, "names", return_value={}), \
                mock.patch.object(front_tools.front, "situation", return_value={}) as situation:
            front_tools.read_situation(session, "GER")
        self.assertFalse(situation.call_args.kwargs["include_enemy_details"])
        sit, _ = self.situation()
        sit["as_of"] = "1939.9.5"
        brief = front_tools._brief(sit)
        self.assertIn("(unknown)", brief["fronts"]["POL"][0])


if __name__ == "__main__":
    unittest.main()
