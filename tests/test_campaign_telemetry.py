"""Offline generation/configuration fixtures only; never touch a real HOI4 install."""

import json
from pathlib import Path
import re
import tempfile
import threading
from types import SimpleNamespace
import unittest
from unittest import mock

from hoi4_live.errors import ToolError
from hoi4_live.state.visibility import PUBLIC_COUNTRY, PUBLIC_WAR
from hoi4_live.telemetry import modgen, playset
from hoi4_live.tools import telemetry_tools


class Config(dict):
    def __init__(self, user, artifacts):
        super().__init__(window={"process_name": "offline-hoi4-fixture"})
        self.user_dir = str(user)
        self.artifacts_dir = str(artifacts)
        self.game_dir = ""


class CampaignGenerationTests(unittest.TestCase):
    def spec(self, campaign=True):
        return {"generation": "fixture", "campaign": campaign, "watch": modgen.empty_watch(),
                "resources": ["steel"], "equipment": ["infantry_equipment"], "techs": ["basic_train"]}

    def fields(self, line):
        return set(re.findall(r'"([^"\n]+)":', line))

    def test_foreign_templates_have_only_public_fields_at_source(self):
        lines = modgen.templates(self.spec())
        for kind in ("wc", "wc_fac"):
            self.assertLessEqual(self.fields(lines["hoi4live_tlm_" + kind]), PUBLIC_COUNTRY)
        self.assertLessEqual(self.fields(lines["hoi4live_tlm_war"]), PUBLIC_WAR)
        for kind in ("wc", "wc_fac", "war"):
            self.assertNotRegex(lines["hoi4live_tlm_" + kind],
                                r'num_divisions|num_of_.*factories|manpower_k|stability|has_war_support')
        self.assertNotIn("enemies_strength_ratio", "\n".join(lines.values()))
        self.assertNotIn("alliance_strength_ratio", "\n".join(lines.values()))
        self.assertIn("num_divisions", lines["hoi4live_tlm_day"])
        self.assertIn("num_of_civilian_factories", lines["hoi4live_tlm_day"])
        self.assertIn("hoi4live_tlm_eq", lines)
        self.assertNotIn("hoi4live_tlm_watch", lines)

    def test_qa_keeps_diagnostic_templates(self):
        lines = modgen.templates(self.spec(campaign=False))
        self.assertIn("num_divisions", lines["hoi4live_tlm_wc"])
        self.assertIn("num_divisions", lines["hoi4live_tlm_war"])
        self.assertIn("enemies_strength_ratio", lines["hoi4live_tlm_day"])

    def test_campaign_state_economy_requires_own_control(self):
        text = modgen.effects_text(self.spec())
        self.assertIn("hoi4live_telemetry_states = {\n\tevery_owned_state = {\n"
                      "\t\tlimit = { is_controlled_by = ROOT }", text)
        self.assertIn("log = hoi4live_tlm_lost", text)
        self.assertNotIn("hoi4live_telemetry_states = {\n\tevery_owned_state = {\n"
                         "\t\tlimit = { is_controlled_by = ROOT }", modgen.effects_text(self.spec(False)))

    def test_campaign_spec_rejects_watches_before_rendering(self):
        for fn in (modgen.templates, modgen.effects_text):
            spec = self.spec()
            spec["watch"]["variables"] = ["private_counter"]
            with self.subTest(function=fn.__name__), self.assertRaises(ToolError):
                fn(spec)

    def test_campaign_generation_discards_inherited_qa_watches(self):
        with tempfile.TemporaryDirectory() as tmp:
            qa = modgen.generate(tmp, "", watch={"variables": ["private_counter"]})
            self.assertFalse(modgen.campaign_compatible(qa))
            with mock.patch.object(modgen, "mod_paths", side_effect=AssertionError("must ignore other mods")):
                man = modgen.generate(tmp, "", campaign=True)
            self.assertTrue(modgen.campaign_compatible(man))
            self.assertEqual(man["watch"], modgen.empty_watch())
            self.assertNotEqual(man["generation"], qa["generation"])
            for rel in man["files"]:
                text = (Path(modgen.mod_dir(tmp)) / rel).read_text(encoding="utf-8")
                self.assertNotIn("private_counter", text)
            with self.assertRaises(ToolError):
                modgen.generate(tmp, "", campaign=True, watch={"global_flags": ["hidden_flag"]})

    def test_campaign_index_uses_only_vanilla_sources(self):
        with tempfile.TemporaryDirectory() as tmp, \
                mock.patch.object(modgen, "game_resources", return_value=["steel"]) as resources, \
                mock.patch.object(modgen, "equipment_archetypes", return_value=set()) as equipment, \
                mock.patch.object(modgen, "game_techs", return_value=["basic_train"]) as techs:
            modgen.generate(tmp, "/offline/fixture", campaign=True)
            for lookup in (resources, equipment, techs):
                lookup.assert_called_once_with("/offline/fixture", [])

    def test_old_or_incomplete_manifest_is_not_campaign_proof(self):
        man = {"campaign": True, "schema": modgen.SCHEMA, "generation": "G", "watch": modgen.empty_watch(),
               "visibility": "player_visible_v1"}
        self.assertTrue(modgen.campaign_compatible(man))
        for key in man:
            bad = dict(man)
            del bad[key]
            self.assertFalse(modgen.campaign_compatible(bad))
        self.assertFalse(modgen.campaign_compatible(None))


class CampaignConfigurationTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        self.user = self.root / "user"
        self.user.mkdir()
        self.original = b'{"enabled_mods": ["mod/private.mod"], "disabled_dlcs": ["keep"], "other": 17}\n'
        (self.user / "dlc_load.json").write_bytes(self.original)
        (self.user / "settings.txt").write_bytes(b"original settings\r\n")
        (self.user / "pdx_settings.txt").write_bytes(b"original pdx\n")
        self.session = SimpleNamespace(cfg=Config(self.user, self.root / "evidence"), mode="campaign",
                                       ui_mutex=threading.RLock(), config_session=None, _telemetry=None)
        self.pids = mock.patch.object(telemetry_tools.process, "running_pids", return_value=[]).start()
        self.addCleanup(mock.patch.stopall)

    def call(self, action, **kwargs):
        args = {"action": action, "raw": False, "remove": False, "limit": 40}
        args.update(kwargs)
        return telemetry_tools.hoi4_telemetry(self.session, args)

    def test_enable_only_telemetry_and_restore_byte_exact_config(self):
        result = self.call("enable")
        written = json.loads((self.user / "dlc_load.json").read_text())
        self.assertEqual(written, {"enabled_mods": [playset.ENTRY], "disabled_dlcs": ["keep"], "other": 17})
        self.assertEqual((self.user / "settings.txt").read_bytes(), b"original settings\r\n")
        self.assertEqual((self.user / "pdx_settings.txt").read_bytes(), b"original pdx\n")
        self.assertTrue(Path(result.data["backup_path"]).is_dir())
        self.assertTrue(modgen.campaign_compatible(result.data["manifest"]))
        restored = self.call("restore")
        self.assertEqual(restored.data["conflicts"], {})
        self.assertEqual((self.user / "dlc_load.json").read_bytes(), self.original)
        self.assertIsNone(self.session.config_session)

    def test_install_then_enable_reuses_prelaunch_backup(self):
        installed = self.call("install")
        self.assertEqual((self.user / "dlc_load.json").read_bytes(), self.original)
        enabled = self.call("enable")
        self.assertEqual(installed.data["backup_path"], enabled.data["backup_path"])

    def test_every_campaign_config_mutation_rejects_running_game(self):
        self.pids.return_value = [123]
        with mock.patch.object(modgen, "generate") as generate:
            for action in ("install", "enable", "disable", "restore"):
                with self.subTest(action=action), self.assertRaises(ToolError) as raised:
                    self.call(action)
                self.assertEqual(raised.exception.details["error"], "game_running")
            generate.assert_not_called()
        self.assertIsNone(self.session.config_session)
        self.assertEqual((self.user / "dlc_load.json").read_bytes(), self.original)

    def test_restore_requires_owned_exit_even_when_process_list_is_empty(self):
        self.call("enable")
        config = self.session.config_session
        config.mark_game_starting("owned-pid-and-creation")
        config.mark_game_started("owned-pid-and-creation")
        with self.assertRaises(ToolError):
            self.call("restore")
        with self.assertRaises(ToolError):
            self.call("enable")
        config.mark_game_exited("owned-pid-and-creation")
        self.call("restore")
        self.assertEqual((self.user / "dlc_load.json").read_bytes(), self.original)

    def test_restore_never_uses_legacy_playset_backup(self):
        with mock.patch.object(playset, "restore") as legacy:
            with self.assertRaises(ToolError) as raised:
                self.call("restore")
            self.assertEqual(raised.exception.details["error"], "config_session_missing")
            legacy.assert_not_called()

    def test_external_playset_change_is_preserved(self):
        self.call("enable")
        changed = b'{"enabled_mods": ["mod/user-new-choice.mod"]}'
        (self.user / "dlc_load.json").write_bytes(changed)
        result = self.call("restore")
        self.assertIn("dlc_load.json", result.data["conflicts"])
        self.assertIsNotNone(self.session.config_session)
        self.assertEqual((self.user / "dlc_load.json").read_bytes(), changed)

    def test_game_starting_during_generation_cannot_change_playset(self):
        self.pids.side_effect = [[], [], [42]]
        with self.assertRaises(ToolError) as raised:
            self.call("enable")
        self.assertEqual(raised.exception.details["error"], "game_running")
        self.assertEqual((self.user / "dlc_load.json").read_bytes(), self.original)
        self.assertIsNotNone(self.session.config_session)

    def test_direct_campaign_tool_cannot_use_watches_or_raw(self):
        for action, args in (("watch", {}), ("read", {"raw": True})):
            with self.subTest(action=action), self.assertRaises(ToolError):
                self.call(action, **args)

    def test_campaign_read_requires_matching_active_generation(self):
        with self.assertRaises(ToolError) as raised:
            self.call("read")
        self.assertEqual(raised.exception.details["error"], "campaign_telemetry_required")
        man = self.call("enable").data["manifest"]
        store = mock.Mock()
        store.status.return_value = {"generation": "old-qa-generation"}
        self.session.telemetry = store
        with self.assertRaises(ToolError) as raised:
            self.call("read")
        self.assertEqual(raised.exception.details["error"], "campaign_telemetry_not_active")
        store.snapshot.assert_not_called()
        store.status.return_value = {"generation": man["generation"]}
        store.player.return_value = "GER"
        store.snapshot.return_value = {"tag": "GER", "date": "1936.1.1.12", "week": {
            "countries": [{"c": "POL", "div": 70, "civ": 9, "name": "Poland"}]}}
        result = self.call("read")
        foreign = result.data["snapshot"]["week"]["countries"][0]
        self.assertEqual(foreign, {"c": "POL", "name": "Poland"})

    def test_qa_mode_keeps_legacy_playset_behavior(self):
        self.session.mode = "test"
        self.call("enable")
        data = json.loads((self.user / "dlc_load.json").read_text())
        self.assertEqual(data["enabled_mods"], ["mod/private.mod", playset.ENTRY])
        self.assertIsNone(self.session.config_session)
        self.assertFalse(modgen.campaign_compatible(modgen.read_manifest(str(self.user))))


if __name__ == "__main__":
    unittest.main()
