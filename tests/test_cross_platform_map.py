"""Pure parts of scripts/cross_platform_map.py; synthetic bytes only, no game binaries, no capstone."""

import collections
import importlib.util
import os
import pathlib
import struct
import tempfile
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[1]
_spec = importlib.util.spec_from_file_location('cross_platform_map', ROOT / 'scripts' / 'cross_platform_map.py')
xmap = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(xmap)


def synthetic_elf(classname=b'17CCurrentGameState', slots=2):
    """ELF64 x86_64 with .text, .rodata (type name), .data.rel.ro (typeinfo + vtable) and .rela.dyn."""
    text_addr, ro_addr, data_addr = 0x1000, 0x2000, 0x3000
    text = b'\xc3' * 0x40
    rodata = classname + b'\0'
    # typeinfo at data_addr: {vptr (symbolic, left 0), name}; vtable at data_addr + 0x10: {top, ti, slot...}
    data = bytearray(0x10 + 0x10 + 8 * slots + 8)
    relocs = [(data_addr + 8, ro_addr), (data_addr + 0x18, data_addr)]
    relocs += [(data_addr + 0x20 + 8 * k, text_addr + 0x10 * k) for k in range(slots)]
    rela = b''.join(struct.pack('<QQq', off, 8, val) for off, val in relocs)
    shstr = b'\0.text\0.rodata\0.data.rel.ro\0.rela.dyn\0.shstrtab\0'
    names = {n: shstr.index(n.encode() + b'\0') for n in ('.text', '.rodata', '.data.rel.ro', '.rela.dyn', '.shstrtab')}
    blobs = [('.text', 1, 0x6, text_addr, text), ('.rodata', 1, 0x2, ro_addr, rodata),
             ('.data.rel.ro', 1, 0x3, data_addr, bytes(data)), ('.rela.dyn', 4, 0x2, 0, rela),
             ('.shstrtab', 3, 0, 0, shstr)]
    body, offsets, pos = b'', [], 0x40
    for _, _, _, _, b in blobs:
        offsets.append(pos + len(body))
        body += b + b'\0' * (-len(b) % 16)
    shoff = 0x40 + len(body)
    header = bytearray(0x40)
    header[:4] = b'\x7fELF'
    header[4], header[5], header[6] = 2, 1, 1
    struct.pack_into('<HHI', header, 16, 3, 62, 1)
    struct.pack_into('<Q', header, 0x28, shoff)
    struct.pack_into('<HHHHHH', header, 0x34, 0x40, 0, 0, 0x40, len(blobs) + 1, len(blobs))
    sh = bytes(0x40)
    for (name, stype, flags, addr, b), off in zip(blobs, offsets):
        sh += struct.pack('<IIQQQQIIQQ', names[name], stype, flags, addr, off, len(b), 0, 0, 8, 0)
    return bytes(header) + body + sh, dict(text=text_addr, data=data_addr)


class PureHelpers(unittest.TestCase):
    def test_uleb128(self):
        self.assertEqual(xmap.uleb128(bytes([0xE5, 0x8E, 0x26]), 0), (624485, 3))
        self.assertEqual(xmap.uleb128(bytes([0x00, 0x7F]), 1), (127, 2))

    def test_rtti_names(self):
        for good in (b'17CCurrentGameState', b'N8NProject15SProjectHistoryE', b'8CCountry'):
            self.assertTrue(xmap.is_rtti_name(good), good)
        for bad in (b'', b'hello world', b'CCountry', b'17CCurrent GameState'):
            self.assertFalse(xmap.is_rtti_name(bad), bad)
        self.assertEqual(xmap.demangle_simple('N8NProject15SProjectHistoryE'), 'NProject::SProjectHistory')
        self.assertEqual(xmap.demangle_simple('17CCurrentGameState'), 'CCurrentGameState')

    def test_operand_parsing(self):
        self.assertEqual(xmap.split_operands('qword ptr [rax + rcx*8 + 0x10], rdx'),
                         ['qword ptr [rax + rcx*8 + 0x10]', 'rdx'])
        self.assertEqual(xmap.mem_disps('qword ptr [rdi + 0x428], rax'), [('rdi', 0x428)])
        self.assertEqual(xmap.mem_disps('eax, dword ptr [rbx + rcx*4 + 0x30]'), [('rbx', 0x30)])
        self.assertEqual(xmap.mem_disps('rax, qword ptr [rdi + 8]'), [('rdi', 8)])
        self.assertEqual(xmap.mem_disps('rax, qword ptr [rbp - 0x28]'), [])
        self.assertEqual(xmap.mem_disps('rax, qword ptr [rip + 0x1000]'), [])
        self.assertEqual(xmap.mem_disps('rax, qword ptr [rdi]'), [])

    def test_tokens_ignore_registers_and_numbers(self):
        a = xmap.token('mov', 'qword ptr [rdi + 0x428], rax')
        b = xmap.token('mov', 'qword ptr [rbx + 0x460], r12')
        self.assertEqual(a, b)
        self.assertNotEqual(a, xmap.token('mov', 'dword ptr [rdi + 0x428], eax'))
        self.assertEqual(xmap.token('lea', 'rax, [rip + 0x10]'), 'lea q,mr')
        self.assertEqual(xmap.token('ret', ''), 'ret')

    def test_rip_and_branch_targets(self):
        self.assertEqual(xmap.rip_target(0x1000, 7, 'rax, qword ptr [rip + 0x20]'), 0x1027)
        self.assertEqual(xmap.rip_target(0x1000, 7, 'rax, qword ptr [rip - 0x20]'), 0xFE7)
        self.assertIsNone(xmap.rip_target(0x1000, 7, 'rax, qword ptr [rdi + 0x20]'))
        self.assertEqual(xmap.branch_target('0x4010'), 0x4010)
        self.assertIsNone(xmap.branch_target('rax'))

    def test_object_register_tracking(self):
        ins = [(0x10, 3, 'mov', 'rbx, rdi'),
               (0x13, 4, 'mov', 'eax, dword ptr [rbx + 0x428]'),
               (0x17, 5, 'call', '0x100'),
               (0x1c, 4, 'mov', 'rdi, qword ptr [rbx + 0x10]'),
               (0x20, 7, 'mov', 'rcx, qword ptr [rip + 0x1000]'),
               (0x27, 3, 'mov', 'ebx, 1'),
               (0x2a, 3, 'ret', '')]
        regs = xmap.object_regs(ins)
        self.assertEqual(regs[1], {'rdi', 'rbx'})
        self.assertEqual(regs[3], {'rbx'})          # call clobbers rdi, rbx is callee-saved
        self.assertEqual(regs[4], {'rbx'})          # rdi was overwritten by a field load
        self.assertEqual(regs[6], set())            # 32-bit write clobbers rbx
        g = xmap.object_regs(ins, seeds=(), globals_=(0x1027,))
        self.assertEqual(g[5], {'rcx'})
        spill = [(0x10, 7, 'lea', 'rax, [rip + 0x1000]'),        # rax = &G (0x1017)
                 (0x17, 3, 'mov', 'rax, qword ptr [rax]'),        # rax = G
                 (0x1a, 4, 'mov', 'qword ptr [rbp - 0x48], rax'),
                 (0x1e, 5, 'call', '0x100'),
                 (0x23, 4, 'mov', 'rcx, qword ptr [rbp - 0x48]'),
                 (0x27, 6, 'mov', 'ecx, dword ptr [rcx + 0x428]')]
        s = xmap.object_regs(spill, seeds=(), globals_=(0x1017,))
        self.assertEqual(s[2], {'rax'})
        self.assertEqual(s[4], set())
        self.assertEqual(s[5], {'rcx'})
        self.assertEqual(xmap.canon('r12d'), 'r12')
        self.assertEqual(xmap.canon('dil'), 'rdi')

    def test_align_pairs_common_runs(self):
        a = ['push q', 'mov q,q', 'call i', 'mov md,d', 'ret']
        b = ['push q', 'mov q,q', 'xor d,d', 'call i', 'mov md,d', 'ret']
        pairs = xmap.align(a, b)
        self.assertIn((3, 4), pairs)
        self.assertIn((0, 0), pairs)
        self.assertEqual(xmap.align([], b), [])

    def test_vote_rule_never_guesses(self):
        C = collections.Counter
        self.assertEqual(xmap.decide(C({0x460: 5}))[:2], (0x460, 'high'))
        self.assertEqual(xmap.decide(C({0x460: 2, 0x40: 1}))[:2], (0x460, 'medium'))
        self.assertEqual(xmap.decide(C({0x460: 1}))[:2], (0x460, 'low'))
        self.assertIsNone(xmap.decide(C({0x460: 2, 0x40: 2}))[0])
        self.assertIsNone(xmap.decide(C({0x460: 1, 0x40: 1}))[0])
        self.assertEqual(xmap.decide(C()), (None, None, 'no evidence'))

    def test_pair_unique(self):
        C = collections.Counter
        cand = {1: C({10: 2}), 2: C({20: 1, 21: 1}), 3: C({30: 1}), 4: C({30: 1})}
        self.assertEqual(xmap.pair_unique(cand), {1: 10})

    def test_addresses(self):
        self.assertEqual(xmap.rel_macho('0x10372b040'), 0x372B040)
        self.assertEqual(xmap.rel_macho('0x372b040'), 0x372B040)
        self.assertEqual(xmap.parse_int('0x3_72b_040'), 0x372B040)
        self.assertIsNone(xmap.parse_int(None))


class SourceScan(unittest.TestCase):
    def test_literals_and_test_code(self):
        with tempfile.TemporaryDirectory() as d:
            p = pathlib.Path(d)
            (p / 'a.rs').write_text('const V: u64 = 0x362_e210; // 0xdead\n#[cfg(test)]\nmod t { const X: u64 = 0x3613100; }\n')
            (p / 'tests').mkdir()
            (p / 'tests' / 'b.rs').write_text('let y = 0x359f820;\n')
            found = xmap.scan_literals([p], d)
            self.assertEqual(found[0x362E210], [('a.rs', 1, False)])
            self.assertNotIn(0xDEAD, found)
            self.assertEqual(found[0x3613100], [('a.rs', 3, True)])
            self.assertEqual(found[0x359F820], [(os.path.join('tests', 'b.rs'), 1, True)])


class SyntheticElf(unittest.TestCase):
    def test_relocated_vtable_and_rtti(self):
        blob, at = synthetic_elf(slots=2)
        with tempfile.TemporaryDirectory() as d:
            path = os.path.join(d, 'elf')
            with open(path, 'wb') as f:
                f.write(blob)
            img = xmap.load_image(path)
        self.assertEqual(img.kind, 'elf')
        self.assertEqual(img.ptr(at['data'] + 8), 0x2000)
        points, index = xmap.vtables(img)
        self.assertEqual(points['17CCurrentGameState'], [at['data'] + 0x20])
        self.assertEqual(index[at['data'] + 0x20], ('17CCurrentGameState', 0, 0))
        self.assertEqual(xmap.vtable_slots(img, at['data'] + 0x20), [at['text'], at['text'] + 0x10])


if __name__ == '__main__':
    unittest.main()
