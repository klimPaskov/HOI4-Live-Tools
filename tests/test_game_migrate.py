"""scripts/game_migrate.py on synthetic executables: identification, the static check, confidence,
modules and scoring. No game binaries and no disassembler are needed."""

import importlib.util
import json
import os
import pathlib
import struct
import sys
import tempfile
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / 'tests'))
from test_cross_platform_map import synthetic_elf  # noqa: E402

_spec = importlib.util.spec_from_file_location('game_migrate', ROOT / 'scripts' / 'game_migrate.py')
gm = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(gm)


def synthetic_pe(guid=bytes(range(16)), age=3):
    """PE32+ x86_64 with one section holding a CodeView RSDS debug record."""
    pe = 0x80
    opt_size = 112 + 16 * 8
    sect_off = pe + 24 + opt_size
    raw = 0x200
    data = bytearray(raw + 0x100)
    data[:2] = b'MZ'
    struct.pack_into('<I', data, 0x3c, pe)
    data[pe:pe + 4] = b'PE\0\0'
    struct.pack_into('<HHIIIHH', data, pe + 4, 0x8664, 1, 0, 0, 0, opt_size, 0x22)
    opt = pe + 24
    struct.pack_into('<H', data, opt, 0x20b)
    struct.pack_into('<I', data, opt + 108, 16)
    # debug directory (index 6) at RVA 0x1000: one entry pointing at the RSDS record at file 0x240
    struct.pack_into('<II', data, opt + 112 + 6 * 8, 0x1000, 28)
    data[sect_off:sect_off + 8] = b'.rdata\0\0'
    struct.pack_into('<IIII', data, sect_off + 8, 0x100, 0x1000, 0x100, raw)
    struct.pack_into('<IIHHIIII', data, raw, 0, 0, 0, 0, 2, 24, 0x1040, raw + 0x40)
    data[raw + 0x40:raw + 0x44] = b'RSDS'
    data[raw + 0x44:raw + 0x54] = guid
    struct.pack_into('<I', data, raw + 0x54, age)
    return bytes(data)


class Identify(unittest.TestCase):
    def test_pe_codeview_guid_is_the_build_id(self):
        with tempfile.TemporaryDirectory() as d:
            path = os.path.join(d, 'hoi4.exe')
            pathlib.Path(path).write_bytes(synthetic_pe())
            pathlib.Path(d, 'launcher-settings.json').write_text(
                json.dumps({'rawVersion': '1.19.4.2', 'version': 'Operation Postern v1.19.4.2'}))
            got = gm.identify(path)
        self.assertEqual(got['system'], 'windows')
        self.assertEqual(got['format'], 'pe')
        # GUID fields are little-endian in the record: 03020100-0504-0706-08090A0B0C0D0E0F, age 3
        self.assertEqual(got['build_id'], '0302010005040706' + '08090A0B0C0D0E0F' + '3')
        self.assertEqual(got['game_version'], '1.19.4.2')
        self.assertFalse(got['canonical'])

    def test_elf_is_linux_and_garbage_is_refused(self):
        blob, _ = synthetic_elf()
        with tempfile.TemporaryDirectory() as d:
            path = os.path.join(d, 'hoi4')
            pathlib.Path(path).write_bytes(blob)
            got = gm.identify(path)
            self.assertEqual((got['system'], got['format']), ('linux', 'elf'))
            self.assertIsNone(got['game_version'])
            bad = os.path.join(d, 'x')
            pathlib.Path(bad).write_bytes(b'hello world')
            with self.assertRaises(SystemExit):
                gm.identify(bad)

    def test_table_name_is_system_and_build(self):
        self.assertEqual(gm.table_name({'system': 'macos', 'build_id': 'AB-CD'}), 'macos-AB-CD')


class StaticCheck(unittest.TestCase):
    def image(self):
        blob, at = synthetic_elf(slots=2)
        d = tempfile.mkdtemp()
        path = os.path.join(d, 'hoi4')
        pathlib.Path(path).write_bytes(blob)
        return gm.xmap.load_image(path), at

    def test_vtable_entry_must_be_the_same_class(self):
        img, at = self.image()
        point = hex(at['data'] + 0x20)
        report = {'image': {'0x100': {'kind': 'vtable', 'rtti': '17CCurrentGameState'},
                            '0x200': {'kind': 'vtable', 'rtti': '10COtherView'},
                            '0x300': {'kind': 'vtable', 'rtti': '17CCurrentGameState'},
                            '0x400': {'kind': 'global'}}}
        table = {'image': {'0x100': point, '0x200': point, '0x300': hex(at['data']), '0x400': '0x9999999'},
                 'fields': {}}
        problems, counts = gm.static_check(table, report, img)
        self.assertEqual(counts['image'], 4)
        text = '\n'.join(problems)
        self.assertNotIn('0x100 ', text)
        self.assertIn('image 0x200', text)
        self.assertIn('the key is 10COtherView', text)
        self.assertIn('image 0x300', text)
        self.assertIn('not a vtable address point', text)
        self.assertIn('image 0x400', text)
        self.assertIn('outside every section', text)

    def test_code_and_data_keys_land_in_their_sections(self):
        img, at = self.image()
        report = {'image': {'0x1': {'kind': 'function'}, '0x2': {'kind': 'global'}, '0x3': {'kind': 'global'}}}
        table = {'image': {'0x1': hex(at['data']), '0x2': hex(at['text']), '0x3': hex(at['data'])}}
        problems, _ = gm.static_check(table, report, img)
        self.assertEqual(len(problems), 2)
        self.assertTrue(any('0x1' in p and 'outside the text' in p for p in problems))
        self.assertTrue(any('0x2' in p and 'inside the text' in p for p in problems))

    def test_fields_keep_member_order_alignment_and_size(self):
        img, _ = self.image()
        table = {'image': {}, 'fields': {
            'Good': {'0x10': '0x18', '0x20': '0x28', '0x428': '0x488'},
            'Swapped': {'0x10': '0x30', '0x20': '0x28'},
            'Odd': {'0x18': '0x1a'},
            'Huge': {'0x8': '0x200000'}}}
        problems, counts = gm.static_check(table, {}, img)
        self.assertEqual(counts['fields'], 7)
        self.assertFalse([p for p in problems if 'Good' in p])
        self.assertTrue([p for p in problems if 'Swapped' in p and 'order' in p])
        self.assertTrue([p for p in problems if 'Odd' in p and 'misaligned' in p])
        self.assertTrue([p for p in problems if 'Huge' in p and 'beyond' in p])


class Tables(unittest.TestCase):
    def test_confidence_comes_from_the_report_and_live_entries_win(self):
        table = {'image': {'0x1': '0x2', '0x5': '0x6'}, 'fields': {'C': {'0x10': '0x18', '0x20': '0x28', '0x30': '0x38'}}}
        report = {'image': {'0x1': {'confidence': 'high'}, '0x5': {'confidence': 'medium'}},
                  'classes': {'C': {'confidence': {'0x10': 'high', '0x20': 'filled'}}},
                  'live': [('C+0x30', None, '0x38'), ('image 0x5', '0x7', '0x6')]}
        c = gm.confidence(table, report)
        self.assertEqual(c['image'], {'0x1': 'high', '0x5': 'live'})
        self.assertEqual(c['fields']['C'], {'0x10': 'high', '0x20': 'filled', '0x30': 'live'})

    def test_a_module_is_enabled_only_when_every_key_is_mapped(self):
        keys = {'image': {'0x100': ['launch/macos/air.rs:3'], '0x200': ['launch/macos/navy.rs:9']},
                'classes': {'C': {'offsets': ['0x10', '0x20'], 'declared': ['launch/macos/air.rs:5']},
                            'D': {'offsets': ['0x8'], 'declared': ['launch/macos/navy.rs:2']}}}
        table = {'image': {'0x100': '0x1', '0x200': '0x2'}, 'fields': {'C': {'0x10': '0x1', '0x20': '0x2'}}}
        mods = gm.modules(table, keys)
        self.assertTrue(mods['launch/macos/air.rs']['enabled'])
        self.assertEqual(mods['launch/macos/air.rs']['keys'], 3)
        self.assertFalse(mods['launch/macos/navy.rs']['enabled'])
        self.assertEqual(mods['launch/macos/navy.rs']['missing'], ['D+0x8'])

    def test_score_counts_right_wrong_and_unknown(self):
        table = {'image': {'0x1': '0x2'}, 'fields': {'C': {'0x10': '0x18', '0x20': '0x99', '0x30': '0x30'}}}
        truth = {'0x1': {'0x2': 1}, '0x10': {'0x18': 2, '0x14': 1}, '0x20': {'0x28': 1}}
        res, wrong = gm.score(table, truth)
        self.assertEqual(res, {'right': 2, 'wrong': 1, 'no answer': 1})
        self.assertIn('C 0x20', wrong[0])


class GotSlot(unittest.TestCase):
    def test_a_mach_o_global_maps_to_itself_not_to_its_got_slot(self):
        S = gm.xmap.Section
        got = S('__DATA', '__got', 0x100, 0x10, 0, False, 'data')
        data = S('__DATA', '__data', 0x200, 0x100, 0, False, 'data')

        class Img:
            kind = 'macho'

            def section_of(self, a):
                return next((s for s in (got, data) if s.addr <= a < s.addr + s.size), None)

            def ptr(self, a):
                return {0x100: 0x240, 0x108: 0}.get(a)

        class M:
            elf = Img()
        self.assertEqual(gm.xmap.Matcher.elf_data_target(M(), 0x100), 0x240)
        self.assertEqual(gm.xmap.Matcher.elf_data_target(M(), 0x108), 0x108)   # an import: stays the slot
        self.assertEqual(gm.xmap.Matcher.elf_data_target(M(), 0x220), 0x220)


class RefCheck(unittest.TestCase):
    def test_tables_without_a_reference_are_skipped_and_others_checked(self):
        blob, at = synthetic_elf()
        blob = blob + b'\0' * (1 << 20)          # a reference executable is larger than 1 MiB
        with tempfile.TemporaryDirectory() as d:
            tables, refs = pathlib.Path(d, 'layouts'), pathlib.Path(d, 'refs')
            tables.mkdir()
            (refs / 'linux-abc').mkdir(parents=True)
            (refs / 'linux-abc' / 'hoi4').write_bytes(blob)
            (tables / 'linux-abc.json').write_text(json.dumps(
                {'build_id': None, 'image': {'0x10': hex(at['data'] + 0x20)}, 'fields': {}}))
            (tables / 'linux-abc.coverage.json').write_text(json.dumps(
                {'image': {'0x10': {'kind': 'vtable', 'rtti': '17CCurrentGameState'}}}))
            (tables / 'linux-def.json').write_text(json.dumps({'build_id': 'def', 'image': {}, 'fields': {}}))
            res = {r['table']: r for r in gm.refcheck(tables, refs)}
        self.assertEqual(res['linux-abc.json']['status'], 'checked')
        self.assertEqual(res['linux-abc.json']['checked'], {'image': 1})
        self.assertEqual(res['linux-def.json']['status'], 'skipped')
        self.assertNotIn('linux-abc.coverage.json', res)


if __name__ == '__main__':
    unittest.main()
