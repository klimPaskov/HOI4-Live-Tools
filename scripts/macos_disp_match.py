#!/usr/bin/env python3
"""Vote on how memory displacements moved between two builds.

Inputs are llvm-objdump Intel-syntax disassemblies of both __text sections and
the two Mach-O files (for LC_FUNCTION_STARTS). Every function is fingerprinted
by its instruction stream with all numbers removed; functions whose fingerprint
is unique in both builds are paired, and each target displacement found in an
old function is paired with the displacement at the same position in the new
one. Optional context words (e.g. another displacement that must appear in the
same function) narrow a vote to one class. Static evidence only.

usage: macos_disp_match.py OLD_BIN OLD_ASM NEW_BIN NEW_ASM 0x7b5 0x18f0 ... [--with 0x9b9]
"""
import bisect
import collections
import hashlib
import importlib.util
import pathlib
import re
import sys

HERE = pathlib.Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location('fm', HERE / 'macos_field_migrate.py')
fm = importlib.util.module_from_spec(spec)
spec.loader.exec_module(fm)

LINE = re.compile(r'^\s*([0-9a-f]+):\s+(\S+)\s*(.*)$')
NUM = re.compile(r'0x[0-9a-f]+|\b\d+\b')
DISP = re.compile(r'\[([a-z0-9]+)(?: \+ [a-z0-9]+\*\d)? ([+-]) (0x[0-9a-f]+)\]')


def functions(binpath, asmpath):
    starts = fm.function_starts(fm.mig.MachO(binpath))
    cur, insns, start = None, [], None
    idx = 0
    with open(asmpath, errors='replace') as f:
        for line in f:
            m = LINE.match(line)
            if not m:
                continue
            a = int(m.group(1), 16)
            while idx < len(starts) and starts[idx] <= a:
                if insns:
                    yield start, insns
                start, insns = starts[idx], []
                idx += 1
            ops = m.group(3).split('#')[0].strip()
            insns.append((m.group(2), ops))
    if insns:
        yield start, insns


def fingerprint(insns):
    h = hashlib.blake2b(digest_size=12)
    for mn, ops in insns:
        h.update(mn.encode()); h.update(NUM.sub('#', ops).encode()); h.update(b'\n')
    return h.digest()


def disps(insns):
    out = []
    for _, ops in insns:
        out.append([(r, int(v, 16) * (1 if s == '+' else -1)) for r, s, v in DISP.findall(ops)])
    return out


def main():
    args = sys.argv[1:]
    context = set()
    if '--with' in args:
        i = args.index('--with'); context = {int(x, 16) for x in args[i+1].split(',')}; del args[i:i+2]
    func = None
    if '--func' in args:
        i = args.index('--func'); func = {int(x, 16) for x in args[i+1].split(',')}; del args[i:i+2]
    ob, oa, nb, na = args[:4]
    if func:
        return show(ob, oa, nb, na, func)
    if '--fuzzy' in args:
        i = args.index('--fuzzy'); addrs = [int(x, 16) for x in args[i+1].split(',')]; del args[i:i+2]
        t = {int(x, 16) for x in args[4:]} or None
        return fuzzy(ob, oa, nb, na, addrs, t)
    targets = {int(x, 16) for x in args[4:]}
    old_fp, old_keep = collections.Counter(), {}
    for start, insns in functions(ob, oa):
        fp = fingerprint(insns); old_fp[fp] += 1
        ds = disps(insns)
        flat = {d for row in ds for _, d in row}
        if flat & targets and (not context or flat & context):
            old_keep.setdefault(fp, []).append((start, ds))
    new_fp, new_keep = collections.Counter(), {}
    for start, insns in functions(nb, na):
        fp = fingerprint(insns); new_fp[fp] += 1
        if fp in old_keep:
            new_keep.setdefault(fp, []).append((start, disps(insns)))
    votes = collections.defaultdict(collections.Counter)
    where = collections.defaultdict(list)
    for fp, olds in old_keep.items():
        if old_fp[fp] != 1 or new_fp[fp] != 1 or fp not in new_keep:
            continue
        (os_, od), (ns, nd) = olds[0], new_keep[fp][0]
        for ro, rn in zip(od, nd):
            for (_, x), (_, y) in zip(ro, rn):
                if x in targets:
                    votes[x][y] += 1
                    where[x].append((os_, ns))
    for t in sorted(targets):
        print(hex(t), {hex(k): v for k, v in votes[t].most_common(4)},
              ' e.g.', ', '.join(f'{a:#x}->{b:#x}' for a, b in where[t][:2]))


def containing(starts, a):
    return starts[bisect.bisect_right(starts, a) - 1]


def fuzzy(ob, oa, nb, na, addrs, targets=None):
    """Align functions containing ADDRS with their most similar new twin near matched anchors."""
    import difflib
    old = dict(functions(ob, oa)); new = dict(functions(nb, na))
    ofp = collections.Counter(fingerprint(i) for i in old.values())
    nfp = {}
    for s_, i in new.items():
        nfp.setdefault(fingerprint(i), []).append(s_)
    anchors = sorted((s_, nfp[fingerprint(i)][0]) for s_, i in old.items()
                     if ofp[fingerprint(i)] == 1 and len(nfp.get(fingerprint(i), [])) == 1 and len(i) > 8)
    akeys = [a for a, _ in anchors]; ostarts = sorted(old); nstarts = sorted(new)
    for a in addrs:
        o = containing(ostarts, a)
        k = bisect.bisect_right(akeys, o)
        lo = anchors[k-1][1] if k else nstarts[0]
        hi = anchors[k][1] if k < len(anchors) else nstarts[-1]
        cand = [x for x in nstarts[bisect.bisect_left(nstarts, lo):bisect.bisect_right(nstarts, hi)]][:400]
        om = [m for m, _ in old[o]]
        best = max(cand, key=lambda c: difflib.SequenceMatcher(None, om, [m for m, _ in new[c]], autojunk=False).ratio())
        sm = difflib.SequenceMatcher(None, om, [m for m, _ in new[best]], autojunk=False)
        od, nd = disps(old[o]), disps(new[best])
        pairs = collections.Counter()
        for blk in sm.get_matching_blocks():
            for j in range(blk.size):
                for (_, x), (_, y) in zip(od[blk.a + j], nd[blk.b + j]):
                    if targets is None or x in targets:
                        pairs[(x, y)] += 1
        moved = {f'{x:#x}->{y:#x}': n for (x, y), n in sorted(pairs.items()) if x != y or (targets and x in targets)}
        print(f'{a:#x} in {o:#x} -> {best:#x} ratio {sm.ratio():.2f}', moved)


def show(ob, oa, nb, na, func):
    """Pair each named old function with its unique-fingerprint new twin and list moved displacements."""
    olds = {s: i for s, i in functions(ob, oa) if s in func}
    fps = {fingerprint(i): s for s, i in olds.items()}
    for s, insns in functions(nb, na):
        fp = fingerprint(insns)
        if fp in fps:
            o = fps.pop(fp)
            moved = sorted({(x, y) for ro, rn in zip(disps(olds[o]), disps(insns)) for (_, x), (_, y) in zip(ro, rn) if x != y})
            print(f'{o:#x} -> {s:#x}', [(hex(x), hex(y)) for x, y in moved])
    for fp, o in fps.items():
        print(f'{o:#x} -> no identical-shape twin')


if __name__ == '__main__':
    main()
