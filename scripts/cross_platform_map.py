#!/usr/bin/env python3
"""Map HOI4 macOS (Mach-O x86_64) addresses and struct field offsets to the Linux ELF build.

Static, read-only: both game binaries are parsed as data and never executed.

usage:
  cross_platform_map.py MACHO ELF ITEMS.json [--out OUT.json] [--cache DIR] [--min-votes N]
  cross_platform_map.py MACHO ELF ITEMS.json --classify-only   (Mach-O side only; ELF may be '-')
  cross_platform_map.py collect MACHO RUST_PATHS... [--root .] [--out ITEMS.json]
      (lists the Mach-O image addresses used as hex literals in the Rust sources, classified)

The first run disassembles both __text sections (a few minutes each) and matches functions
(about ten minutes); both are cached under --cache (default ~/.cache/hoi4-xmap).

ITEMS.json is a list (or {"items": [...]}) of Mach-O items:
  {"name": "gs_vtable",  "kind": "vtable",   "address": "0x35044e8"}
  {"name": "gs_global",  "kind": "global",   "address": "0x372b040"}
  {"name": "some_fn",    "kind": "function", "address": "0x200e720"}
  {"name": "gs.date",    "kind": "field",    "offset": "0x428", "vtable": "0x35044e8"}
  {"name": "gs.date",    "kind": "field",    "offset": "0x428", "class": "17CCurrentGameState"}
  {"name": "x",          "kind": "field",    "offset": "0x428", "anchor": "0x200e720"}
  {"name": "gs.date",    "kind": "field",    "offset": "0x428", "vtable": "0x35044e8",
   "context": ["0x372b040"]}   context: pointer globals holding the object (adds their users)
  {"kind": "auto", "address": ...}  classifies the address first (vtable/global/function/...).
Optional "expect" (ELF value) turns the run into a validation and is scored per item.

Addresses are image-relative (Mach-O VM address minus 0x100000000; absolute values are
accepted too). ELF values are image-relative (the PIE is linked at 0).

Methods
  vtable    Itanium RTTI: the address point's typeinfo name is looked up in the ELF and the
            same address-point ordinal is taken (secondary vtables differ in offset-to-top).
  function  vtable slot pairs, then functions sharing a unique string literal, then callee
            propagation from matched pairs (call lists aligned with known pairs as anchors).
  global    every Mach-O instruction that references the address (directly or via its __got
            slot) in a matched function is aligned with the ELF twin (normalised instruction
            shapes); ELF RIP targets vote, one vote per function pair.
  field     instructions using [reg + offset] in matched functions of the class (virtual
            slots, constructors/destructors that store the vtable), the anchor function, or
            users of a context global are aligned; only displacements on a register that holds
            the object (rdi at entry, or the pointer loaded from the context global, tracked
            through copies and frame spills) vote on both sides. Offsets differ per struct
            (libc++ string 24 bytes vs libstdc++ 32), so no constant delta is used.
            Weak spot: inlined std::string code touches different sub-offsets on each ABI.
A value is reported only when the evidence passes the vote rule; otherwise the item is
"unresolved" with the best candidate and the tallies in "notes". Nothing is guessed.

Requires the capstone disassembler (pip install capstone, e.g. in a private venv).
"""
import argparse
import bisect
import collections
import difflib
import hashlib
import json
import os
import pathlib
import pickle
import re
import struct
import sys

MACHO_BASE = 0x100000000
CACHE_VERSION = 3

# ---------------------------------------------------------------------------
# Pure helpers (unit tested without game binaries)
# ---------------------------------------------------------------------------

def uleb128(data, i):
    value = shift = 0
    while True:
        b = data[i]
        i += 1
        value |= (b & 0x7F) << shift
        shift += 7
        if b < 0x80:
            return value, i


RTTI_NAME = re.compile(rb'^(?:N?[0-9]+[A-Za-z_][A-Za-z0-9_]*|N[0-9A-Za-z_]+E|[0-9]+[A-Za-z_][A-Za-z0-9_]*I[0-9A-Za-z_]+E)$')


def is_rtti_name(raw):
    """True for a mangled class name as stored in std::type_info::__name (e.g. 17CCurrentGameState)."""
    return bool(raw) and len(raw) < 1024 and bool(RTTI_NAME.match(raw))


def demangle_simple(name):
    """Best-effort readable form of a plain or nested mangled class name (display only)."""
    def parts(s, i):
        out = []
        while i < len(s) and s[i].isdigit():
            j = i
            while s[j].isdigit():
                j += 1
            n = int(s[i:j])
            out.append(s[j:j + n])
            i = j + n
        return out, i
    if name.startswith('N'):
        p, _ = parts(name, 1)
        return '::'.join(p) or name
    p, i = parts(name, 0)
    return '::'.join(p) + ('<...>' if i < len(name) else '') if p else name


REG64 = set('rax rbx rcx rdx rsi rdi rbp rsp r8 r9 r10 r11 r12 r13 r14 r15 rip'.split())
REG32 = set('eax ebx ecx edx esi edi ebp esp r8d r9d r10d r11d r12d r13d r14d r15d'.split())
REG16 = set('ax bx cx dx si di bp sp r8w r9w r10w r11w r12w r13w r14w r15w'.split())
REG8 = set('al bl cl dl sil dil bpl spl ah bh ch dh r8b r9b r10b r11b r12b r13b r14b r15b'.split())
MEM = re.compile(r'(?:(byte|word|dword|qword|xmmword|ymmword|tbyte|zmmword) ptr )?(?:[a-z]s:)?\[([^\]]*)\]')
DISP_IN_MEM = re.compile(r'^([a-z0-9]+)(?: \+ [a-z0-9]+(?:\*\d+)?)? ([+-]) (0x[0-9a-f]+|\d+)$')
RIP = re.compile(r'\[rip ([+-]) (0x[0-9a-f]+|\d+)\]')
NUMBER = re.compile(r'^-?(?:0x[0-9a-f]+|\d+)$')


def operand_shape(op):
    op = op.strip()
    m = MEM.fullmatch(op) or (MEM.search(op) if '[' in op else None)
    if m:
        inner = m.group(2)
        kind = 'mr' if inner.startswith('rip') else 'm'
        return kind + (m.group(1) or '')[:1]
    if op in REG64:
        return 'q'
    if op in REG32:
        return 'd'
    if op in REG16:
        return 'w'
    if op in REG8:
        return 'b'
    if op.startswith(('xmm', 'ymm', 'zmm')):
        return 'x'
    if op.startswith('st'):
        return 'f'
    if NUMBER.match(op):
        return 'i'
    return 'o'


def token(mnemonic, op_str):
    """Instruction shape without registers or numbers, comparable across compilers' register allocation."""
    if not op_str:
        return mnemonic
    return mnemonic + ' ' + ','.join(operand_shape(o) for o in split_operands(op_str))


def split_operands(op_str):
    out, depth, cur = [], 0, []
    for ch in op_str:
        if ch == '[':
            depth += 1
        elif ch == ']':
            depth -= 1
        if ch == ',' and depth == 0:
            out.append(''.join(cur))
            cur = []
        else:
            cur.append(ch)
    out.append(''.join(cur))
    return [o.strip() for o in out]


def mem_disps(op_str):
    """[(base, disp)] for every memory operand with a displacement (rip/rsp/rbp excluded)."""
    out = []
    for m in MEM.finditer(op_str):
        d = DISP_IN_MEM.match(m.group(2))
        if not d:
            continue
        base, sign, value = d.groups()
        if base in ('rip', 'rsp', 'rbp', 'esp', 'ebp'):
            continue
        v = int(value, 0)
        out.append((base, -v if sign == '-' else v))
    return out


def rip_target(address, size, op_str):
    m = RIP.search(op_str)
    if not m:
        return None
    v = int(m.group(2), 0)
    return address + size + (-v if m.group(1) == '-' else v)


def branch_target(op_str):
    op_str = op_str.strip()
    if op_str.startswith('0x'):
        try:
            return int(op_str, 16)
        except ValueError:
            return None
    return None


_CANON = {}
for _r in ('ax', 'bx', 'cx', 'dx', 'si', 'di', 'bp', 'sp'):
    for _alias in ('r' + _r, 'e' + _r, _r):
        _CANON[_alias] = 'r' + _r
for _a, _r in (('al', 'rax'), ('bl', 'rbx'), ('cl', 'rcx'), ('dl', 'rdx'), ('sil', 'rsi'), ('dil', 'rdi'),
               ('bpl', 'rbp'), ('spl', 'rsp'), ('ah', 'rax'), ('bh', 'rbx'), ('ch', 'rcx'), ('dh', 'rdx')):
    _CANON[_a] = _r
for _n in range(8, 16):
    for _s in ('', 'd', 'w', 'b'):
        _CANON[f'r{_n}{_s}'] = f'r{_n}'
CALL_CLOBBERED = {'rax', 'rcx', 'rdx', 'rsi', 'rdi', 'r8', 'r9', 'r10', 'r11'}
NO_DEST = {'cmp', 'test', 'push', 'call', 'jmp', 'ret', 'nop', 'ucomiss', 'ucomisd', 'comiss', 'comisd', 'bt'}


def canon(reg):
    return _CANON.get(reg)


def object_regs(insns, seeds=('rdi',), globals_=()):
    """Linear (flow-insensitive) tracking of registers holding the object pointer.
    insns: [(address, size, mnemonic, op_str)]. Seeds: registers holding it at entry; globals_: absolute
    addresses of pointer variables whose loaded value is the object. Returns, per instruction, the set of
    64-bit register names holding the object before that instruction executes."""
    held = set(seeds)   # registers holding the object pointer
    addr = set()        # registers holding the address of a context pointer variable (lea rX, [rip + G])
    slots = set()       # frame slots ([rbp - 0x48]) holding the object pointer
    out = []
    gl = set(globals_)
    for address, size, mn, ops in insns:
        out.append(frozenset(held))
        if mn == 'call':
            held -= CALL_CLOBBERED
            addr -= CALL_CLOBBERED
            continue
        if mn in NO_DEST or mn.startswith('j') or not ops or mn == 'xchg':
            continue
        parts = split_operands(ops)
        src = parts[1] if len(parts) > 1 else ''
        if '[' in parts[0]:
            m = MEM.search(parts[0])
            if m and re.match(r'^(rbp|rsp)( [+-] (0x[0-9a-f]+|\d+))?$', m.group(2)):
                slots.discard(m.group(2))
                if mn == 'mov' and src in REG64 and canon(src) in held:
                    slots.add(m.group(2))
            continue
        dst = canon(parts[0])
        if dst is None:
            continue
        obj = ptr = False
        if parts[0] in REG64:
            if mn == 'mov' and src in REG64:
                obj, ptr = canon(src) in held, canon(src) in addr
            elif mn == 'mov' and src.startswith('qword ptr ['):
                inner = MEM.search(src).group(2)
                if gl and inner.startswith('rip') and rip_target(address, size, src) in gl:
                    obj = True
                elif inner in addr or inner in slots:
                    obj = True
            elif mn == 'lea' and gl and src.startswith('[rip') and rip_target(address, size, src) in gl:
                ptr = True
        held.discard(dst)
        addr.discard(dst)
        if obj:
            held.add(dst)
        if ptr:
            addr.add(dst)
    return out


def align(tokens_a, tokens_b, limit=8000):
    """Aligned index pairs (i, j) of two token lists (difflib matching blocks)."""
    if not tokens_a or not tokens_b or len(tokens_a) > limit or len(tokens_b) > limit:
        return []
    sm = difflib.SequenceMatcher(None, tokens_a, tokens_b, autojunk=False)
    out = []
    for blk in sm.get_matching_blocks():
        out.extend((blk.a + k, blk.b + k) for k in range(blk.size))
    return out


def decide(votes, min_votes=2, ratio=2.0):
    """Vote rule. Returns (value or None, confidence, note).
    high: >= max(min_votes, 3) votes and >= ratio x runner-up and >= 75% of all votes.
    medium: >= min_votes and >= ratio x runner-up.  low: a single uncontested vote.
    Everything else is unresolved (value None) with the tallies in the note."""
    if not votes:
        return None, None, 'no evidence'
    ranked = votes.most_common()
    best, n = ranked[0]
    second = ranked[1][1] if len(ranked) > 1 else 0
    total = sum(votes.values())
    tally = ', '.join(f'{fmt(k)}:{v}' for k, v in ranked[:4])
    if n >= max(min_votes, 3) and n >= ratio * second and n >= 0.75 * total:
        return best, 'high', tally
    if n >= min_votes and n >= ratio * second:
        return best, 'medium', tally
    if n == 1 and total == 1:
        return best, 'low', tally
    return None, None, 'ambiguous: ' + tally


def fmt(v):
    return hex(v) if isinstance(v, int) else str(v)


def parse_int(v):
    if v is None:
        return None
    if isinstance(v, int):
        return v
    return int(str(v).replace('_', ''), 0)


def rel_macho(v):
    v = parse_int(v)
    return v - MACHO_BASE if v is not None and v >= MACHO_BASE else v


def pair_unique(candidates):
    """candidates: {a: Counter(b: votes)}. Keep a->b where b is a's only candidate and a is b's only one."""
    back = collections.defaultdict(set)
    for a, c in candidates.items():
        for b in c:
            back[b].add(a)
    out = {}
    for a, c in candidates.items():
        if len(c) == 1:
            b = next(iter(c))
            if len(back[b]) == 1:
                out[a] = b
    return out


# ---------------------------------------------------------------------------
# Binary images
# ---------------------------------------------------------------------------

class Section:
    __slots__ = ('seg', 'name', 'addr', 'size', 'off', 'nobits', 'kind')

    def __init__(self, seg, name, addr, size, off, nobits, kind):
        self.seg, self.name, self.addr, self.size, self.off, self.nobits, self.kind = seg, name, addr, size, off, nobits, kind

    def __repr__(self):
        return f'{self.seg},{self.name}'


class Image:
    """Common view: sections, relocated pointer reads, imports, function starts. Addresses are absolute VM."""
    kind = None
    base = 0

    def __init__(self, path):
        self.path = str(path)
        self.data = pathlib.Path(path).read_bytes()
        self.sections = []
        self.imports = {}      # stub/plt address -> symbol name (leading '_' stripped on Mach-O)
        self.slot_names = {}   # pointer slot address -> symbol name
        self.starts = []
        self.identity = None
        self._sorted = None

    # sections
    def section_of(self, addr):
        if self._sorted is None:
            self._sorted = sorted((s for s in self.sections if s.size), key=lambda s: s.addr)
            self._keys = [s.addr for s in self._sorted]
        i = bisect.bisect_right(self._keys, addr) - 1
        if i >= 0:
            s = self._sorted[i]
            if s.addr <= addr < s.addr + s.size:
                return s
        return None

    def read(self, addr, n):
        s = self.section_of(addr)
        if s is None or s.nobits:
            return None
        fo = s.off + (addr - s.addr)
        return self.data[fo:fo + min(n, s.addr + s.size - addr)]

    def raw_u64(self, addr):
        b = self.read(addr, 8)
        return struct.unpack('<Q', b)[0] if b and len(b) == 8 else None

    def ptr(self, addr):
        return self.raw_u64(addr)

    def sym_at(self, addr):
        return self.slot_names.get(addr)

    def cstr(self, addr, limit=512):
        s = self.section_of(addr)
        if s is None or s.nobits or s.kind not in ('cstring', 'rodata'):
            return None
        fo = s.off + (addr - s.addr)
        end = self.data.find(b'\0', fo, fo + limit)
        if end < 0:
            return None
        return self.data[fo:end]

    def text_sections(self):
        return [s for s in self.sections if s.kind == 'text']

    def is_code(self, addr):
        s = self.section_of(addr)
        return s is not None and s.kind == 'text'

    def rel(self, addr):
        return addr - self.base

    def absolute(self, rel):
        return rel + self.base if rel < self.base else rel

    def function_end(self, start):
        i = bisect.bisect_right(self.starts, start)
        end = self.starts[i] if i < len(self.starts) else start + 0x1000
        s = self.section_of(start)
        if s is not None:
            end = min(end, s.addr + s.size)
        return min(end, start + 0x40000)

    def containing_function(self, addr):
        i = bisect.bisect_right(self.starts, addr) - 1
        return self.starts[i] if i >= 0 else None

    def sha256(self):
        return hashlib.sha256(self.data).hexdigest()


class MachO(Image):
    kind = 'macho'

    def __init__(self, path):
        super().__init__(path)
        d = self.data
        magic, cputype, _, _, ncmds, _, _, _ = struct.unpack_from('<IiiIIIII', d, 0)
        if magic != 0xFEEDFACF or cputype != 0x01000007:
            raise SystemExit(f'{path}: not a thin x86_64 Mach-O')
        self.base = MACHO_BASE
        off = 32
        symtab = dysymtab = fstarts = None
        raw_sects = []
        self.chained = False
        for _ in range(ncmds):
            cmd, size = struct.unpack_from('<II', d, off)
            if cmd == 0x19:
                segname = d[off + 8:off + 24].rstrip(b'\0').decode()
                vmaddr = struct.unpack_from('<Q', d, off + 24)[0]
                if segname == '__TEXT':
                    self.base = vmaddr
                nsects = struct.unpack_from('<I', d, off + 64)[0]
                so = off + 72
                for _ in range(nsects):
                    sect = d[so:so + 16].rstrip(b'\0').decode()
                    seg = d[so + 16:so + 32].rstrip(b'\0').decode()
                    addr, ssize, foff = struct.unpack_from('<QQI', d, so + 32)
                    flags, r1, r2 = struct.unpack_from('<III', d, so + 64)
                    raw_sects.append((seg, sect, addr, ssize, foff, flags, r1, r2))
                    so += 80
            elif cmd == 0x2:
                symtab = struct.unpack_from('<IIII', d, off + 8)
            elif cmd == 0xB:
                dysymtab = struct.unpack_from('<II', d, off + 56)
            elif cmd == 0x26:
                fstarts = struct.unpack_from('<II', d, off + 8)
            elif cmd == 0x1B:
                u = d[off + 8:off + 24].hex().upper()
                self.identity = f'{u[:8]}-{u[8:12]}-{u[12:16]}-{u[16:20]}-{u[20:]}'
            elif cmd == 0x80000034:
                self.chained = True
            off += size
        if self.chained:
            raise SystemExit(f'{path}: chained fixups are not supported by this tool yet')
        for seg, sect, addr, ssize, foff, flags, r1, r2 in raw_sects:
            stype = flags & 0xFF
            nobits = stype in (0x1, 0xC) or foff == 0
            if sect == '__text' or (flags & 0x80000000 and seg == '__TEXT' and sect not in ('__stubs', '__stub_helper')):
                kind = 'text'
            elif sect in ('__stubs',):
                kind = 'stubs'
            elif sect == '__cstring':
                kind = 'cstring'
            elif seg == '__TEXT':
                kind = 'rodata'
            elif nobits:
                kind = 'bss'
            else:
                kind = 'data'
            self.sections.append(Section(seg, sect, addr, ssize, foff, nobits, kind))
        # symbols
        self.names = {}
        self.symbols = []
        if symtab:
            symoff, nsyms, stroff, strsize = symtab
            for i in range(nsyms):
                strx, ntype, nsect, ndesc, value = struct.unpack_from('<IBBHQ', d, symoff + 16 * i)
                end = d.find(b'\0', stroff + strx)
                name = d[stroff + strx:end].decode(errors='replace')
                self.symbols.append(name)
                if ntype & 0xE == 0xE and value:
                    self.names.setdefault(value, name)
        # indirect symbols: stubs and pointer slots
        if dysymtab and symtab:
            ioff, n = dysymtab
            indirect = struct.unpack_from(f'<{n}I', d, ioff) if n else ()
            for seg, sect, addr, ssize, foff, flags, r1, r2 in raw_sects:
                stype = flags & 0xFF
                if stype == 0x8 and r2:
                    step = r2
                elif stype in (0x6, 0x7):
                    step = 8
                else:
                    continue
                for k in range(ssize // step):
                    idx = indirect[r1 + k] if r1 + k < len(indirect) else 0x80000000
                    if idx & 0xC0000000:
                        continue
                    name = self.symbols[idx]
                    name = name[1:] if name.startswith('_') else name
                    if stype == 0x8:
                        self.imports[addr + k * step] = name
                    else:
                        self.slot_names[addr + k * step] = name
        if fstarts:
            doff, dsize = fstarts
            a, i, starts = self.base, doff, []
            while i < doff + dsize:
                delta, i = uleb128(d, i)
                if delta == 0:
                    break
                a += delta
                starts.append(a)
            self.starts = starts


class ELF(Image):
    kind = 'elf'

    def __init__(self, path):
        super().__init__(path)
        d = self.data
        if d[:4] != b'\x7fELF' or d[4] != 2 or struct.unpack_from('<H', d, 18)[0] != 62:
            raise SystemExit(f'{path}: not an ELF64 x86_64 file')
        shoff = struct.unpack_from('<Q', d, 0x28)[0]
        shentsize, shnum, shstrndx = struct.unpack_from('<HHH', d, 0x3A)
        raw = []
        for i in range(shnum):
            o = shoff + i * shentsize
            name, stype, flags, addr, foff, size, link, info, align_, entsize = struct.unpack_from('<IIQQQQIIQQ', d, o)
            raw.append([name, stype, flags, addr, foff, size, link, info, entsize])
        stroff = raw[shstrndx][4]
        self.by_name = {}
        for r in raw:
            end = d.find(b'\0', stroff + r[0])
            r[0] = d[stroff + r[0]:end].decode()
            self.by_name[r[0]] = r
        for name, stype, flags, addr, foff, size, link, info, entsize in raw:
            if not addr or not size:
                continue
            nobits = stype == 8
            if flags & 0x4:
                kind = 'stubs' if name.startswith('.plt') else 'text'
            elif name.startswith('.rodata'):
                kind = 'rodata'
            elif nobits:
                kind = 'bss'
            elif flags & 0x1:
                kind = 'data'
            else:
                kind = 'meta'
            self.sections.append(Section('', name, addr, size, foff, nobits, kind))
        self.base = 0
        # build id
        note = self.by_name.get('.note.gnu.build-id')
        if note:
            namesz, descsz, _ = struct.unpack_from('<III', d, note[4])
            o = note[4] + 12 + ((namesz + 3) & ~3)
            self.identity = d[o:o + descsz].hex()
        # dynamic symbols and relocations
        dynsym, dynstr = self.by_name.get('.dynsym'), self.by_name.get('.dynstr')
        syms = []
        if dynsym and dynstr:
            for k in range(dynsym[5] // 24):
                st_name = struct.unpack_from('<I', d, dynsym[4] + 24 * k)[0]
                end = d.find(b'\0', dynstr[4] + st_name)
                syms.append(d[dynstr[4] + st_name:end].decode(errors='replace'))
        self.relative = {}
        for rname in ('.rela.dyn', '.rela.plt'):
            r = self.by_name.get(rname)
            if not r:
                continue
            for k in range(r[5] // 24):
                off, info, addend = struct.unpack_from('<QQq', d, r[4] + 24 * k)
                rtype, sym = info & 0xFFFFFFFF, info >> 32
                if rtype == 8:  # R_X86_64_RELATIVE
                    self.relative[off] = addend
                elif rtype in (1, 6, 7) and sym < len(syms):  # 64, GLOB_DAT, JUMP_SLOT
                    self.slot_names[off] = syms[sym] + (f'+{addend}' if rtype == 1 and addend else '')
        # PLT entries: decode 'jmp [rip+X]' (ff 25) and name by the slot's relocation
        for s in self.sections:
            if s.kind != 'stubs':
                continue
            blob = self.data[s.off:s.off + s.size]
            for k in range(0, len(blob) - 6):
                if blob[k] == 0xFF and blob[k + 1] == 0x25:
                    disp = struct.unpack_from('<i', blob, k + 2)[0]
                    slot = s.addr + k + 6 + disp
                    name = self.slot_names.get(slot)
                    if name:
                        start = s.addr + (k & ~0xF) if s.name == '.plt' else s.addr + (k & ~0x7)
                        if blob[k - 4:k] == b'\xf3\x0f\x1e\xfa':
                            start = s.addr + k - 4
                        self.imports.setdefault(start, name)
        # function starts from .eh_frame_hdr
        hdr = self.by_name.get('.eh_frame_hdr')
        if hdr:
            o = hdr[4]
            ver, ptr_enc, cnt_enc, tbl_enc = d[o:o + 4]
            if ver != 1 or cnt_enc != 0x03 or tbl_enc != 0x3B or ptr_enc != 0x1B:
                raise SystemExit(f'{path}: unsupported .eh_frame_hdr encoding')
            count = struct.unpack_from('<I', d, o + 8)[0]
            starts = set()
            for k in range(count):
                loc, _ = struct.unpack_from('<ii', d, o + 12 + 8 * k)
                starts.add(hdr[3] + loc)
            self.starts = sorted(starts)

    def ptr(self, addr):
        if addr in self.relative:
            return self.relative[addr]
        if addr in self.slot_names:
            return None
        return self.raw_u64(addr)

    def cstr(self, addr, limit=512):
        s = self.section_of(addr)
        if s is None or s.nobits or s.kind != 'rodata':
            return None
        fo = s.off + (addr - s.addr)
        end = self.data.find(b'\0', fo, fo + limit)
        if end < 0:
            return None
        return self.data[fo:end]


def load_image(path):
    with open(path, 'rb') as f:
        head = f.read(4)
    return ELF(path) if head == b'\x7fELF' else MachO(path)


# ---------------------------------------------------------------------------
# RTTI vtables
# ---------------------------------------------------------------------------

def typeinfo_name(img, ti, cache):
    if ti in cache:
        return cache[ti]
    name = None
    s = img.section_of(ti)
    if s is not None and s.kind == 'data':
        np_ = img.ptr(ti + 8)
        if np_:
            raw = img.cstr(np_) if img.kind == 'elf' else _macho_name(img, np_)
            if raw is not None and is_rtti_name(raw):
                name = raw.decode()
    cache[ti] = name
    return name


def _macho_name(img, addr):
    s = img.section_of(addr)
    if s is None or s.nobits or s.seg != '__TEXT':
        return None
    fo = s.off + (addr - s.addr)
    end = img.data.find(b'\0', fo, fo + 1024)
    return img.data[fo:end] if end >= 0 else None


def vtables(img):
    """class name -> sorted address points; address point -> (class, ordinal, offset_to_top)."""
    cache = {}
    points = collections.defaultdict(list)
    meta = {}
    for s in img.sections:
        if s.kind != 'data' or s.nobits:
            continue
        a = s.addr + ((-s.addr) % 8)
        end = s.addr + s.size - 8
        while a < end:
            ti = img.ptr(a + 8)
            if ti and img.section_of(ti) is not None:
                b = img.read(a, 8)
                top = struct.unpack('<q', b)[0] if b and len(b) == 8 else 1
                if -0x100000 <= top <= 0 and top % 8 == 0:
                    n = typeinfo_name(img, ti, cache)
                    if n:
                        points[n].append(a + 16)
                        meta[a + 16] = top
            a += 8
    index = {}
    for n, pts in points.items():
        pts.sort()
        for i, p in enumerate(pts):
            index[p] = (n, i, meta[p])
    return points, index


def vtable_slots(img, point, limit=600):
    """Function addresses in the vtable starting at an address point (stops at the first non-code word).
    Pure-virtual / imported slots are returned as the symbol name string."""
    out = []
    for k in range(limit):
        a = point + 8 * k
        sym = img.sym_at(a)
        if sym is not None:
            if 'pure_virtual' in sym or 'deleted_virtual' in sym:
                out.append(sym)
                continue
            break
        v = img.ptr(a)
        if v is None or not img.is_code(v):
            break
        out.append(v)
    return out


# ---------------------------------------------------------------------------
# Function index (capstone, cached)
# ---------------------------------------------------------------------------

def _capstone():
    try:
        import capstone
    except ImportError:
        raise SystemExit('capstone is required: python3 -m venv VENV && VENV/bin/pip install capstone')
    md = capstone.Cs(capstone.CS_ARCH_X86, capstone.CS_MODE_64)
    md.syntax = capstone.CS_OPT_SYNTAX_INTEL
    md.skipdata = True
    return md


def build_index(img, cache_dir):
    """{start: (ninsn, calls, refs)}; calls are direct call / tail-jump targets, refs RIP-relative targets."""
    key = f'{img.kind}-{img.sha256()[:24]}-v{CACHE_VERSION}.pickle'
    path = pathlib.Path(cache_dir) / key if cache_dir else None
    if path and path.exists():
        with open(path, 'rb') as f:
            return pickle.load(f)
    md = _capstone()
    index = {}
    total = len(img.starts)
    for n, start in enumerate(img.starts):
        if not img.is_code(start):
            continue
        end = img.function_end(start)
        code = img.read(start, end - start)
        calls, refs, count = [], [], 0
        for a, size, mn, ops in md.disasm_lite(code, start):
            count += 1
            if 'rip' in ops:
                t = rip_target(a, size, ops)
                if t is not None:
                    refs.append(t)
            if mn == 'call' or (mn == 'jmp' and ops.startswith('0x')):
                t = branch_target(ops)
                if t is not None and not (start <= t < end):
                    calls.append(t)
        index[start] = (count, tuple(calls), tuple(refs))
        if n % 20000 == 0:
            print(f'  indexed {n}/{total} {img.kind} functions', file=sys.stderr)
    if path:
        path.parent.mkdir(parents=True, exist_ok=True)
        with open(path, 'wb') as f:
            pickle.dump(index, f, protocol=pickle.HIGHEST_PROTOCOL)
    return index


class Disasm:
    """On-demand per-function instruction lists: [(address, size, mnemonic, op_str)]."""

    def __init__(self, img):
        self.img = img
        self.md = None
        self.cache = {}

    def __call__(self, start):
        if start in self.cache:
            return self.cache[start]
        if self.md is None:
            self.md = _capstone()
        end = self.img.function_end(start)
        code = self.img.read(start, end - start) or b''
        ins = list(self.md.disasm_lite(code, start))
        if len(self.cache) > 4000:
            self.cache.clear()
        self.cache[start] = ins
        return ins


# ---------------------------------------------------------------------------
# Cross-binary matcher
# ---------------------------------------------------------------------------

class Matcher:
    def __init__(self, mac, elf, cache_dir=None, min_votes=2, log=True):
        self.mac, self.elf = mac, elf
        self.min_votes = min_votes
        self.log = log
        self.say('loading RTTI vtables')
        self.mvt, self.mvt_index = vtables(mac)
        self.evt, self.evt_index = vtables(elf)
        self.say(f'  vtable classes: macho {len(self.mvt)}, elf {len(self.evt)}, common {len(set(self.mvt) & set(self.evt))}')
        self.say('indexing functions (cached after the first run)')
        self.mix = build_index(mac, cache_dir)
        self.eix = build_index(elf, cache_dir)
        self.mdis, self.edis = Disasm(mac), Disasm(elf)
        self.mrefs = self._invert(self.mix)
        self.erefs = self._invert(self.eix)
        self.fmap = {}      # mac function -> elf function
        self.rmap = {}      # elf function -> mac function
        self.how = {}       # mac function -> evidence string
        path = None
        if cache_dir:
            path = pathlib.Path(cache_dir) / f'pairs-{mac.sha256()[:16]}-{elf.sha256()[:16]}-v{CACHE_VERSION}.pickle'
        if path and path.exists():
            with open(path, 'rb') as f:
                self.fmap, self.how = pickle.load(f)
            self.rmap = {g: f for f, g in self.fmap.items()}
            self.say(f'  function pairs from cache: {len(self.fmap)}')
        else:
            self._match_functions()
            if path:
                with open(path, 'wb') as f:
                    pickle.dump((self.fmap, self.how), f, protocol=pickle.HIGHEST_PROTOCOL)

    def say(self, msg):
        if self.log:
            print(msg, file=sys.stderr)

    @staticmethod
    def _invert(index):
        inv = collections.defaultdict(list)
        for f, (_, _, refs) in index.items():
            for t in set(refs):
                inv[t].append(f)
        return inv

    def _accept(self, pairs, how):
        n = 0
        for f, g in pairs.items():
            if f in self.fmap or g in self.rmap:
                continue
            if not self._size_ok(f, g):
                continue
            self.fmap[f], self.rmap[g], self.how[f] = g, f, how
            n += 1
        return n

    def _size_ok(self, f, g):
        a, b = self.mix.get(f), self.eix.get(g)
        if not a or not b:
            return False
        x, y = a[0], b[0]
        return min(x, y) * 4 + 8 >= max(x, y)

    # -- seeds ---------------------------------------------------------------
    def vtable_pairs(self):
        cand = collections.defaultdict(collections.Counter)
        for cls, mpts in self.mvt.items():
            epts = self.evt.get(cls)
            if not epts or len(epts) != len(mpts):
                continue
            for mp, ep in zip(mpts, epts):
                ms, es = vtable_slots(self.mac, mp), vtable_slots(self.elf, ep)
                if len(ms) != len(es):
                    continue
                for a, b in zip(ms, es):
                    if isinstance(a, int) and isinstance(b, int):
                        cand[a][b] += 1
        return cand

    def _string_users(self, img, index):
        users = collections.defaultdict(set)
        for f, (_, _, refs) in index.items():
            for t in refs:
                s = img.cstr(t)
                if s is not None and len(s) >= 4:
                    users[s].add(f)
        return users

    def string_pairs(self):
        mu = self._string_users(self.mac, self.mix)
        eu = self._string_users(self.elf, self.eix)
        cand = collections.defaultdict(collections.Counter)
        for s, fs in mu.items():
            gs = eu.get(s)
            if gs and len(fs) == 1 and len(gs) == 1:
                cand[next(iter(fs))][next(iter(gs))] += 1
        self.mstr, self.estr = mu, eu
        return cand

    def _match_functions(self):
        n = self._accept(pair_unique(self.vtable_pairs()), 'vtable-slot')
        self.say(f'  function pairs from vtable slots: {n}')
        n = self._accept(pair_unique(self.string_pairs()), 'unique-string')
        self.say(f'  function pairs from unique strings: {n}')
        for rnd in range(12):
            n = self._propagate()
            self.say(f'  callee propagation round {rnd + 1}: +{n} (total {len(self.fmap)})')
            if n == 0:
                break

    def _label(self, img, t, side):
        if img.kind == 'macho':
            name = img.imports.get(t)
            if name:
                return 'I:' + name
            if t in self.fmap:
                return 'M:%x' % t
        else:
            name = img.imports.get(t)
            if name:
                return 'I:' + name
            if t in self.rmap:
                return 'M:%x' % self.rmap[t]
        return 'U'

    def _propagate(self):
        cand = collections.defaultdict(collections.Counter)
        for f, g in list(self.fmap.items()):
            ca, cb = self.mix[f][1], self.eix[g][1]
            if not ca or not cb:
                continue
            la = [self._label(self.mac, t, 0) for t in ca]
            lb = [self._label(self.elf, t, 1) for t in cb]
            if 'U' not in la or 'U' not in lb:
                continue
            sm = difflib.SequenceMatcher(None, la, lb, autojunk=False)
            blocks = sm.get_matching_blocks()
            for blk in blocks:
                anchored = blk.size >= 2 and any(la[blk.a + k] != 'U' for k in range(blk.size))
                if not anchored and not (len(la) == len(lb) and blk.size == len(la)):
                    # lone unknowns between two anchors: accept when both gaps have exactly one call
                    continue
                for k in range(blk.size):
                    if la[blk.a + k] == 'U':
                        a, b = ca[blk.a + k], cb[blk.b + k]
                        if self.mac.is_code(a) and self.elf.is_code(b):
                            cand[a][b] += 1
            # single unknown call sandwiched between equal anchors on both sides
            prev = None
            for blk in blocks:
                if prev is not None:
                    ga = range(prev.a + prev.size, blk.a)
                    gb = range(prev.b + prev.size, blk.b)
                    if len(ga) == 1 and len(gb) == 1 and la[ga[0]] == 'U' and lb[gb[0]] == 'U':
                        cand[ca[ga[0]]][cb[gb[0]]] += 1
                prev = blk
        # drop candidates involving already matched functions; require a consistent winner
        clean = collections.defaultdict(collections.Counter)
        for a, c in cand.items():
            if a in self.fmap:
                continue
            for b, v in c.items():
                if b not in self.rmap:
                    clean[a][b] += v
        pairs = {}
        back = collections.defaultdict(collections.Counter)
        for a, c in clean.items():
            for b, v in c.items():
                back[b][a] += v
        for a, c in clean.items():
            (b, v), *rest = c.most_common(2)
            if rest and rest[0][1] * 2 > v:
                continue
            (a2, v2), *rest2 = back[b].most_common(2)
            if a2 != a or (rest2 and rest2[0][1] * 2 > v2):
                continue
            pairs[a] = b
        return self._accept(pairs, 'callee-propagation')

    # -- instruction alignment ---------------------------------------------
    def aligned(self, f, g):
        ia, ib = self.mdis(f), self.edis(g)
        ta = [token(m, o) for _, _, m, o in ia]
        tb = [token(m, o) for _, _, m, o in ib]
        return ia, ib, align(ta, tb)

    # -- items -----------------------------------------------------------------
    def map_vtable(self, rel):
        a = self.mac.absolute(rel)
        hit = self.mvt_index.get(a)
        if not hit:
            return dict(status='unresolved', notes=['not a Mach-O vtable address point (no RTTI typeinfo before it)'])
        cls, i, top = hit
        epts = self.evt.get(cls)
        info = dict(rtti=cls, readable=demangle_simple(cls), ordinal=i, offset_to_top=top)
        if not epts:
            return dict(status='unresolved', evidence=info, notes=[f'no ELF vtable with RTTI name {cls}'])
        mp = self.mvt[cls]
        if len(epts) != len(mp):
            # fall back on the primary vtable only when both have one with offset_to_top 0 at ordinal 0
            if i == 0 and self.evt_index[epts[0]][2] == 0:
                return dict(status='mapped', elf=epts[0], method='rtti-primary', confidence='medium', evidence=info,
                            notes=[f'address-point count differs (macho {len(mp)}, elf {len(epts)}); primary taken'])
            return dict(status='unresolved', evidence=info,
                        notes=[f'address-point count differs (macho {len(mp)}, elf {len(epts)})'])
        ep = epts[i]
        ms, es = vtable_slots(self.mac, a), vtable_slots(self.elf, ep)
        notes = [] if len(ms) == len(es) else [f'slot count differs (macho {len(ms)}, elf {len(es)})']
        return dict(status='mapped', elf=ep, method='rtti', confidence='high' if not notes else 'medium',
                    evidence=dict(info, slots=len(ms)), notes=notes)

    def map_function(self, rel):
        a = self.mac.absolute(rel)
        if a in self.fmap:
            return dict(status='mapped', elf=self.fmap[a], method=self.how[a],
                        confidence='high' if self.how[a] in ('vtable-slot', 'unique-string') else 'medium')
        if a not in self.mix:
            f = self.mac.containing_function(a)
            if f is not None and f in self.fmap:
                return self._map_code_inside(f, a)
            return dict(status='unresolved', notes=['address is not a known Mach-O function start'])
        return dict(status='unresolved', notes=['no ELF counterpart found by vtable slots, unique strings or callee propagation'])

    def _map_code_inside(self, f, a):
        g = self.fmap[f]
        ia, ib, pairs = self.aligned(f, g)
        idx = {x[0]: k for k, x in enumerate(ia)}
        if a not in idx:
            return dict(status='unresolved', notes=['address is not an instruction boundary'])
        k = idx[a]
        for i, j in pairs:
            if i == k:
                return dict(status='mapped', elf=ib[j][0], method='aligned-instruction', confidence='medium',
                            notes=[f'inside function {fmt(self.mac.rel(f))} -> {fmt(self.elf.rel(g))}'])
        return dict(status='unresolved', notes=['instruction has no aligned twin in the matched ELF function'])

    def map_global(self, rel):
        a = self.mac.absolute(rel)
        s = self.mac.section_of(a)
        if s is not None and s.kind == 'cstring':
            raw = self.mac.cstr(a)
            hits = [t for t, fs in self.erefs.items() if self.elf.cstr(t) == raw]
            if len(hits) == 1:
                return dict(status='mapped', elf=hits[0], method='cstring', confidence='high')
        users, targets = self.global_users(a)
        votes = collections.Counter()
        seen = unmatched = 0
        for f in sorted(users):
            g = self.fmap.get(f)
            if g is None:
                unmatched += 1
                continue
            seen += 1
            ia, ib, pairs = self.aligned(f, g)
            want = {k for k, (ad, sz, mn, ops) in enumerate(ia) if 'rip' in ops and rip_target(ad, sz, ops) in targets}
            got = collections.Counter()
            for i, j in pairs:
                if i in want:
                    ad, sz, mn, ops = ib[j]
                    t = rip_target(ad, sz, ops) if 'rip' in ops else None
                    if t is not None:
                        got[self.elf_data_target(t)] += 1
            if not got:
                got = self._ref_order_vote(f, g, a)
            for t, n in got.items():
                votes[t] += 1  # one vote per function pair
        value, conf, note = decide(votes, self.min_votes)
        notes = [f'{len(users)} referencing Mach-O functions, {seen} matched, {unmatched} unmatched', note]
        if value is None:
            return dict(status='unresolved', notes=notes, candidates={fmt(self.elf.rel(k)): v for k, v in votes.most_common(4)})
        return dict(status='mapped', elf=value, method='aligned-rip-refs', confidence=conf, notes=notes)

    def _ref_order_vote(self, f, g, a):
        """Fallback: align the ordered data-reference lists (strings by content, vtables by class)."""
        def labels(img, refs, vt_index):
            out = []
            for t in refs:
                s = img.cstr(t)
                if s is not None:
                    out.append('S:' + s[:40].decode(errors='replace'))
                elif t in vt_index:
                    out.append('V:' + vt_index[t][0])
                elif img.is_code(t):
                    out.append('C')
                else:
                    out.append('D')
            return out
        ra, rb = self.mix[f][2], self.eix[g][2]
        la, lb = labels(self.mac, ra, self.mvt_index), labels(self.elf, rb, self.evt_index)
        got = collections.Counter()
        for i, j in align(la, lb):
            if ra[i] == a and la[i] == 'D':
                got[rb[j]] += 1
        return got

    def class_functions(self, cls):
        """Mach-O functions whose 'this' is the class: primary vtable slots and code that stores the vtable."""
        out = set()
        pts = self.mvt.get(cls, [])
        if pts:
            for v in vtable_slots(self.mac, pts[0]):
                if isinstance(v, int):
                    out.add(v)
            for p in pts:
                out.update(self.mrefs.get(p, []))
        return out

    def global_users(self, a):
        """Mach-O functions referencing a data address directly or through a __got slot that holds it."""
        users = set(self.mrefs.get(a, []))
        name = self.mac.names.get(a)
        name = name[1:] if name and name.startswith('_') else name
        for slot, sname in self.mac.slot_names.items():
            if name and sname == name:
                users.update(self.mrefs.get(slot, []))
        return users, {a} | {s for s, n in self.mac.slot_names.items() if name and n == name}

    def elf_data_target(self, t):
        """An ELF RIP target, seen through a .got slot when the slot holds a local address."""
        s = self.elf.section_of(t)
        if s is not None and s.name == '.got' and t in self.elf.relative:
            return self.elf.relative[t]
        return t

    def map_field(self, offset, cls=None, anchors=(), context=()):
        """Votes from class functions (object in rdi), anchors (rdi) and functions that load a context
        global holding the object pointer. Only displacements on registers that hold the object count."""
        jobs = {}
        for x in anchors:
            jobs[self.mac.absolute(x)] = (('rdi',), (), ('rdi',), ())
        if cls:
            for f in self.class_functions(cls):
                jobs[f] = (('rdi',), (), ('rdi',), ())
        for c in context:
            ca = self.mac.absolute(c)
            users, slots = self.global_users(ca)
            for f in users:
                if f in self.fmap:
                    jobs.setdefault(f, ((), tuple(slots), (), None))
        votes = collections.Counter()
        seen = matched = 0
        for f in sorted(jobs):
            g = self.fmap.get(f)
            if g is None:
                continue
            ia = self.mdis(f)
            if not any(d == offset for _, _, _, ops in ia if '[' in ops for _, d in mem_disps(ops)):
                continue
            seen += 1
            ia, ib, pairs = self.aligned(f, g)
            mseeds, mglobals, eseeds, eglobals = jobs[f]
            if eglobals is None:
                # the ELF twin's context global: whatever its aligned instructions load where the Mach-O loads it
                eglobals = set()
                for i, j in pairs:
                    ad, sz, mn, ops = ia[i]
                    if 'rip' in ops and rip_target(ad, sz, ops) in mglobals:
                        bd, bs, bm, bo = ib[j]
                        if 'rip' in bo:
                            eglobals.add(rip_target(bd, bs, bo))
            oa = object_regs(ia, mseeds, mglobals)
            ob = object_regs(ib, eseeds, eglobals)
            got = collections.Counter()
            for i, j in pairs:
                da = mem_disps(ia[i][3]) if '[' in ia[i][3] else ()
                if not da or not any(d == offset and canon(b) in oa[i] for b, d in da):
                    continue
                db = mem_disps(ib[j][3]) if '[' in ib[j][3] else ()
                if len(da) != len(db):
                    continue
                for (bx, x), (by, y) in zip(da, db):
                    if x == offset and canon(bx) in oa[i] and canon(by) in ob[j]:
                        got[y] += 1
            if got:
                matched += 1
                for y, n in got.items():
                    votes[y] += 1
        value, conf, note = decide(votes, self.min_votes)
        notes = [f'{len(jobs)} class/anchor/context functions, {seen} matched pairs use +{offset:#x}, '
                 f'{matched} gave aligned votes on the object register', note]
        if value is None:
            return dict(status='unresolved', notes=notes, candidates={fmt(k): v for k, v in votes.most_common(4)})
        return dict(status='mapped', elf=value, method='aligned-displacements', confidence=conf, notes=notes)


# ---------------------------------------------------------------------------
# Classification (Mach-O only)
# ---------------------------------------------------------------------------

def classify(mac, rel, mvt_index, starts_set=None):
    """Kind of a Mach-O image-relative value: vtable / typeinfo / function / code / cstring / global / field."""
    a = mac.absolute(rel)
    if a in mvt_index:
        cls, i, top = mvt_index[a]
        return dict(kind='vtable', rtti=cls, readable=demangle_simple(cls), ordinal=i)
    s = mac.section_of(a)
    if s is None:
        return dict(kind='field', note='outside the Mach-O image; treated as a struct offset or plain number')
    if s.kind == 'text':
        if starts_set is not None and a in starts_set:
            return dict(kind='function', section=repr(s))
        if rel < 0x100000:
            return dict(kind='field', note='small value inside the __text range but not a function start; '
                                           'treated as a struct offset or plain number')
        return dict(kind='code', section=repr(s), note='inside a function, not a function start')
    if s.kind == 'cstring':
        raw = mac.cstr(a) or b''
        return dict(kind='cstring', section=repr(s), text=raw[:60].decode(errors='replace'))
    # vtable interior: within a vtable but not at an address point
    near = [p for p in mvt_index if p - 16 <= a < p + 8 * 600]
    if near:
        p = max(x for x in near if x - 16 <= a)
        slots = vtable_slots(mac, p)
        if p - 16 <= a < p + 8 * len(slots):
            cls = mvt_index[p][0]
            return dict(kind='vtable-entry', rtti=cls, readable=demangle_simple(cls), slot=(a - p) // 8,
                        section=repr(s))
    if s.name.startswith('__objc') or s.name == '__cfstring':
        return dict(kind='objc', section=repr(s), note='Objective-C runtime data; the Linux build has no counterpart')
    name = mac.names.get(a)
    if name and name.startswith('__ZTI'):
        return dict(kind='typeinfo', symbol=name, section=repr(s))
    d = dict(kind='global', section=repr(s))
    if name:
        d['symbol'] = name
    return d


# ---------------------------------------------------------------------------
# Driver
# ---------------------------------------------------------------------------

def load_items(path):
    data = json.loads(pathlib.Path(path).read_text())
    return data['items'] if isinstance(data, dict) else data


def resolve_class(m, item):
    if item.get('class'):
        return item['class']
    if item.get('vtable') is not None:
        hit = m.mvt_index.get(m.mac.absolute(rel_macho(item['vtable'])))
        return hit[0] if hit else None
    return None


def run_item(m, item):
    kind = item.get('kind', 'auto')
    out = {k: item[k] for k in ('name', 'kind', 'address', 'offset', 'class', 'vtable', 'anchor', 'anchors', 'context', 'expect') if k in item}
    addr = rel_macho(item.get('address'))
    if kind == 'auto':
        c = classify(m.mac, addr, m.mvt_index, set(m.mix))
        kind = {'vtable': 'vtable', 'function': 'function', 'code': 'function', 'global': 'global',
                'cstring': 'global', 'typeinfo': 'global', 'vtable-entry': 'global'}.get(c['kind'], 'unsupported')
        out['classified'] = c
    if kind == 'vtable':
        r = m.map_vtable(addr)
    elif kind == 'function':
        r = m.map_function(addr)
    elif kind == 'global':
        r = m.map_global(addr)
    elif kind == 'field':
        cls = resolve_class(m, item)
        anchors = [rel_macho(x) for x in item.get('anchors', [])] + ([rel_macho(item['anchor'])] if item.get('anchor') else [])
        if not cls and not anchors and not item.get('context'):
            r = dict(status='unresolved', notes=['field needs "class", "vtable", "anchor" or "context"'])
        else:
            context = [rel_macho(x) for x in item.get('context', [])]
            r = m.map_field(parse_int(item['offset']), cls, anchors, context)
            if cls:
                r.setdefault('evidence', {})['rtti'] = cls
    else:
        r = dict(status='unresolved', notes=[f'unsupported kind {kind}'])
    if r.get('elf') is not None and kind != 'field':
        r['elf'] = m.elf.rel(r['elf'])
    if r.get('elf') is not None:
        r['elf'] = hex(r['elf'])
    out.update(r)
    out.setdefault('elf', None)
    if 'expect' in item:
        exp = parse_int(item['expect'])
        if out['status'] != 'mapped':
            out['verdict'] = 'unresolved'
        else:
            out['verdict'] = 'correct' if int(out['elf'], 16) == exp else 'wrong'
    return out


def summary(results):
    s = collections.Counter(r['status'] for r in results)
    out = dict(items=len(results), mapped=s['mapped'], unresolved=s['unresolved'])
    v = [r for r in results if 'verdict' in r]
    if v:
        out['validation'] = dict(collections.Counter(r['verdict'] for r in v))
        by = collections.defaultdict(collections.Counter)
        for r in v:
            by[r.get('confidence') or 'none'][r['verdict']] += 1
        out['by_confidence'] = {k: dict(c) for k, c in by.items()}
        bk = collections.defaultdict(collections.Counter)
        for r in v:
            bk[r['kind']][r['verdict']] += 1
        out['by_kind'] = {k: dict(c) for k, c in bk.items()}
    return out


def classify_only(mac_path, items):
    mac = load_image(mac_path)
    _, index = vtables(mac)
    starts = set(mac.starts)
    out = []
    for it in items:
        if it.get('kind') == 'field' or it.get('address') is None:
            out.append(dict(it, classified=dict(kind='field')))
            continue
        out.append(dict(it, classified=classify(mac, rel_macho(it['address']), index, starts)))
    return out


HEX_LITERAL = re.compile(r'(?<![0-9A-Za-z_])0x([0-9a-fA-F_]{3,12})(?![0-9A-Za-z_])')


def is_test_path(path):
    parts = pathlib.PurePath(path).parts
    return any(p == 'tests' or p == 'tests.rs' or p.endswith('_tests.rs') for p in parts)


def scan_literals(paths, root=None):
    """{value: [(relative file, line, is_test)]} for every hex literal in the Rust files under paths.
    Code after a top-level '#[cfg(test)]' line counts as test code."""
    found = collections.defaultdict(list)
    files = []
    for p in paths:
        p = pathlib.Path(p)
        files.extend(sorted(p.rglob('*.rs')) if p.is_dir() else [p])
    for f in files:
        test = is_test_path(f)
        rel = os.path.relpath(f, root) if root else str(f)
        for n, line in enumerate(f.read_text(errors='replace').splitlines(), 1):
            if line.startswith('#[cfg(test)]'):
                test = True
            code = line.split('//')[0]
            for m in HEX_LITERAL.finditer(code):
                found[int(m.group(1).replace('_', ''), 16)].append((rel, n, test))
    return found


def collect(mac_path, paths, root=None):
    """Item list for every hex literal that lands inside the Mach-O image (vtables, globals, code, strings).
    Literals below the first __text address are struct offsets or plain numbers and are counted, not listed."""
    mac = load_image(mac_path)
    _, index = vtables(mac)
    starts = set(mac.starts)
    text_lo = min(s.addr for s in mac.text_sections()) - mac.base
    image_hi = max(s.addr + s.size for s in mac.sections) - mac.base
    lits = scan_literals(paths, root)
    items, small, fields = [], collections.Counter(), []

    def offset(v, prod, where):
        small['production' if prod else 'tests_only'] += 1
        if prod and v >= 8:
            fields.append(dict(offset=hex(v), sources=sorted({f'{f}:{n}' for f, n, _ in prod})[:12]))

    for v in sorted(lits):
        where = lits[v]
        prod = [w for w in where if not w[2]]
        if v < text_lo:
            offset(v, prod, where)
            continue
        if v >= image_hi:
            continue
        c = classify(mac, v, index, starts)
        if c['kind'] == 'field':
            offset(v, prod, where)
            continue
        kind = {'vtable': 'vtable', 'function': 'function', 'global': 'global', 'typeinfo': 'global',
                'cstring': 'global', 'vtable-entry': 'global', 'code': 'function', 'objc': 'unsupported'}.get(c['kind'], 'auto')
        items.append(dict(name=c.get('readable') or c.get('symbol') or f'{c["kind"]}_{v:x}', kind=kind,
                          address=hex(v), classified=c, tests_only=not prod,
                          sources=sorted({f'{f}:{n}' for f, n, _ in (prod or where)})[:8]))
    return dict(macho=dict(file=os.path.basename(mac_path), uuid=mac.identity),
                image_range=[hex(text_lo), hex(image_hi)],
                small_literals=dict(small), items=items,
                fields_needing_owner=dict(
                    note='Literals below the image range in production code: struct offsets, sizes, flags or plain '
                         'numbers. A field offset can only be mapped once its owning class (vtable) or an anchor '
                         'function is attached; add it as {"kind": "field", "offset", "vtable"|"anchor"}.',
                    values=fields))


def main(argv=None):
    argv = sys.argv[1:] if argv is None else argv
    if argv and argv[0] == 'collect':
        p = argparse.ArgumentParser(prog='cross_platform_map.py collect',
                                    description='List Mach-O image addresses used as hex literals in Rust sources.')
        p.add_argument('macho')
        p.add_argument('paths', nargs='+')
        p.add_argument('--root', help='make source paths relative to this directory')
        p.add_argument('--out')
        a = p.parse_args(argv[1:])
        doc = collect(a.macho, a.paths, a.root)
        c = collections.Counter(i['classified']['kind'] + (' (tests only)' if i['tests_only'] else '') for i in doc['items'])
        doc['counts'] = dict(sorted(c.items()))
        print(json.dumps(doc['counts'], indent=1), file=sys.stderr)
        text = json.dumps(doc, indent=1)
        if a.out:
            pathlib.Path(a.out).write_text(text + '\n')
        else:
            print(text)
        return
    p = argparse.ArgumentParser(description=__doc__.split('\n')[0])
    p.add_argument('macho')
    p.add_argument('elf')
    p.add_argument('items')
    p.add_argument('--out')
    p.add_argument('--cache', default=os.environ.get('XMAP_CACHE', str(pathlib.Path.home() / '.cache' / 'hoi4-xmap')))
    p.add_argument('--min-votes', type=int, default=2)
    p.add_argument('--classify-only', action='store_true')
    a = p.parse_args(argv)
    items = load_items(a.items)
    if a.classify_only:
        result = dict(macho=os.path.basename(a.macho), items=classify_only(a.macho, items))
    else:
        mac, elf = load_image(a.macho), load_image(a.elf)
        if mac.kind != 'macho' or elf.kind != 'elf':
            raise SystemExit('expected MACHO then ELF')
        m = Matcher(mac, elf, a.cache, a.min_votes)
        results = [run_item(m, it) for it in items]
        result = dict(macho=dict(file=os.path.basename(a.macho), uuid=mac.identity),
                      elf=dict(file=os.path.basename(a.elf), build_id=elf.identity),
                      function_pairs=len(m.fmap), summary=summary(results), items=results)
        print(json.dumps(result['summary'], indent=1), file=sys.stderr)
    text = json.dumps(result, indent=1)
    if a.out:
        pathlib.Path(a.out).write_text(text + '\n')
    else:
        print(text)


if __name__ == '__main__':
    main()
