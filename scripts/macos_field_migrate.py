#!/usr/bin/env python3
"""Infer struct field offset changes between two macOS HOI4 builds.

For each class whose vtable exists in both builds (matched by RTTI name), the
virtual functions in the same slots are disassembled with llvm-objdump. When a
pair has the same mnemonic sequence, memory displacements are paired by
position; each pairing is a vote old_disp -> new_disp for that class. Static
evidence only: verify each result against the live game before use.

usage: macos_field_migrate.py OLD_BIN NEW_BIN CLASS_REGEX [--disp 0x430,...] [--json OUT]
"""
import collections
import importlib.util
import json
import pathlib
import re
import struct
import subprocess
import sys

HERE = pathlib.Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location('mig', HERE / 'macos_build_migrate.py')
mig = importlib.util.module_from_spec(spec)
spec.loader.exec_module(mig)

OBJDUMP = subprocess.run(['xcrun', '--find', 'llvm-objdump'], capture_output=True, text=True).stdout.strip()
DISP = re.compile(r'\[([a-z0-9]+)(?: \+ [a-z0-9]+\*\d)? ([+-]) (0x[0-9a-f]+)\]')


def uleb(data, i):
    v = s = 0
    while True:
        b = data[i]; i += 1
        v |= (b & 0x7f) << s; s += 7
        if b < 0x80:
            return v, i


def function_starts(m):
    d = m.data
    ncmds = struct.unpack_from('<I', d, 16)[0]
    off = 32
    text_vm = None
    for _ in range(ncmds):
        cmd, size = struct.unpack_from('<II', d, off)
        if cmd == 0x19 and d[off+8:off+24].rstrip(b'\0') == b'__TEXT':
            text_vm = struct.unpack_from('<Q', d, off+24)[0]
        if cmd == 0x26:
            doff, dsize = struct.unpack_from('<II', d, off+8)
            starts, a, i = [], text_vm, doff
            while i < doff + dsize:
                delta, i = uleb(d, i)
                if delta == 0:
                    break
                a += delta; starts.append(a)
            return starts
        off += size
    raise SystemExit('no LC_FUNCTION_STARTS')


class Binary:
    def __init__(self, path):
        self.path = path
        self.m = mig.MachO(path)
        self.starts = function_starts(self.m)
        self.vt = self.m.vtables()
        self.cache = {}

    def end(self, a):
        import bisect
        i = bisect.bisect_right(self.starts, a)
        return self.starts[i] if i < len(self.starts) else a + 0x400

    def slots(self, vtable, n=200):
        out = []
        for k in range(n):
            try:
                p = self.m.u64(vtable + 8 * k)
            except Exception:
                break
            if not p or p not in set_starts(self):
                break
            out.append(p)
        return out

    def disasm(self, a):
        if a in self.cache:
            return self.cache[a]
        e = min(self.end(a), a + 0x3000)
        out = subprocess.run([OBJDUMP, '-d', '--no-show-raw-insn', '--x86-asm-syntax=intel',
                              f'--start-address={a:#x}', f'--stop-address={e:#x}', self.path],
                             capture_output=True, text=True).stdout
        ins = []
        for line in out.splitlines():
            p = line.split('\t')
            if len(p) >= 2 and p[0].strip().endswith(':') and re.match(r'\s*[0-9a-f]+:', p[0]):
                mn = p[1].strip(); ops = p[2] if len(p) > 2 else ''
                disps = [(r, int(v, 16) * (1 if s == '+' else -1)) for r, s, v in DISP.findall(ops)
                         if r not in ('rip', 'rsp', 'rbp')]
                ins.append((mn, disps))
        self.cache[a] = ins
        return ins


_SS = {}
def set_starts(b):
    if id(b) not in _SS:
        _SS[id(b)] = set(b.starts)
    return _SS[id(b)]


def votes(old, new, cls_re):
    res = collections.defaultdict(collections.Counter)
    for name, olds in old.vt.items():
        if not re.search(cls_re, name) or name not in new.vt:
            continue
        news = new.vt[name]
        for ov, nv in zip(olds[:1], news[:1]):
            for of, nf in zip(old.slots(ov), new.slots(nv)):
                a, b = old.disasm(of), new.disasm(nf)
                if len(a) != len(b) or [x[0] for x in a] != [x[0] for x in b]:
                    continue
                for (_, da), (_, db) in zip(a, b):
                    if len(da) != len(db):
                        continue
                    for (_, x), (_, y) in zip(da, db):
                        if x >= 0x10:
                            res[name][(x, y)] += 1
    return res


def main():
    args = sys.argv[1:]
    out = None
    want = None
    if '--json' in args:
        i = args.index('--json'); out = args[i+1]; del args[i:i+2]
    if '--disp' in args:
        i = args.index('--disp'); want = {int(x, 16) for x in args[i+1].split(',')}; del args[i:i+2]
    old, new, cls_re = Binary(args[0]), Binary(args[1]), args[2]
    res = votes(old, new, cls_re)
    table = {}
    for name, c in sorted(res.items()):
        best = {}
        for (x, y), n in c.items():
            if want and x not in want:
                continue
            best.setdefault(x, collections.Counter())[y] += n
        rows = {hex(x): {hex(y): n for y, n in ys.most_common(3)} for x, ys in sorted(best.items())}
        if rows:
            table[name] = rows
            changed = {k: v for k, v in rows.items() if list(v)[0] != k}
            print(name[:60], f'{len(rows)} fields, {len(changed)} moved')
            for k, v in changed.items():
                print('   ', k, '->', v)
    if out:
        pathlib.Path(out).write_text(json.dumps(table, indent=1))


if __name__ == '__main__':
    main()
