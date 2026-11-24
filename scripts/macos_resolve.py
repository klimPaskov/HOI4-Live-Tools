#!/usr/bin/env python3
"""Resolve old macOS image addresses in a new macOS build by aligned code (static, read-only).

usage: macos_resolve.py OLD NEW MAP.json ADDR... [--cache DIR]
  MAP.json: a migration map (its "image" table supplies the local deltas) or a flat {old: {new}} table.
  Add each answer to the map's "image" table with its confidence; cross-check it against the neighbouring
  delta (the entry below it) before calling a medium answer high.
  function/code address: the containing function is paired with the new function nearest the anchors'
  local delta whose instruction shapes match best; the instruction is mapped through the alignment.
  data address: every old instruction that addresses it RIP-relative (exact, or the nearest referenced
  address at most 0x400 below it) votes through its aligned twin in the paired new function.
Prints one JSON object {addr: {new, method, confidence, evidence}}."""
import bisect, collections, difflib, json, pathlib, sys

args = [a for a in sys.argv[1:]]
scripts = str(pathlib.Path(__file__).resolve().parent)
cache = args[args.index('--cache') + 1] if '--cache' in args else str(pathlib.Path.home() / 'Projects/_scratch/xmap-cache')
sys.path.insert(0, scripts)
import cross_platform_map as X  # noqa: E402

pos = [a for i, a in enumerate(args) if not a.startswith('--') and (i == 0 or args[i - 1] != '--cache')]
old, new = X.load_image(pos[0]), X.load_image(pos[1])
anchors = json.load(open(pos[2]))
anchors = anchors.get('image', anchors) if isinstance(anchors.get('image'), dict) else anchors
wanted = [int(a, 16) for a in pos[3:]]
oix, nix = X.build_index(old, cache), X.build_index(new, cache)
odis, ndis = X.Disasm(old), X.Disasm(new)
nstarts = sorted(s for s in new.starts if new.is_code(s))

# local deltas from the anchors (old rel -> new rel), code anchors only for code, all for data
pts = sorted((int(k, 16), int(v['new'], 16) - int(k, 16)) for k, v in anchors.items() if v.get('new'))
pkeys = [p[0] for p in pts]


def deltas_near(rel):
    i = bisect.bisect_right(pkeys, rel)
    out = []
    for j in (i - 2, i - 1, i, i + 1):
        if 0 <= j < len(pts):
            out.append(pts[j][1])
    return sorted(set(out))


def toks(ins):
    return [X.token(m, o) for _, _, m, o in ins]


def pair_function(f_old):
    """Best new function for old function start f_old: (g, ratio, runner_up_ratio)."""
    ia = odis(f_old)
    ta = toks(ia)
    rel = f_old - old.base
    cands = set()
    for d in deltas_near(rel):
        guess = f_old + d
        k = bisect.bisect_left(nstarts, guess)
        for j in range(max(0, k - 6), min(len(nstarts), k + 6)):
            cands.add(nstarts[j])
    scored = []
    for g in cands:
        tb = toks(ndis(g))
        sm = difflib.SequenceMatcher(None, ta, tb, autojunk=False)
        scored.append((sm.ratio(), g))
    scored.sort(reverse=True)
    if not scored:
        return None, 0, 0
    best = scored[0]
    second = scored[1][0] if len(scored) > 1 else 0
    return best[1], best[0], second


def exact_ops(i_old, i_new):
    """Same mnemonic and operands apart from absolute/rip numbers."""
    (_, sa, ma, oa), (_, sb, mb, ob) = i_old, i_new
    return ma == mb and sa == sb and X.token(ma, oa) == X.token(mb, ob)


def map_code(addr_abs):
    f = old.containing_function(addr_abs)
    if f is None:
        return dict(new=None, notes=['no containing function'])
    g, ratio, second = pair_function(f)
    if g is None or ratio < 0.8 or ratio - second < 0.05:
        return dict(new=None, notes=[f'no clear new function (best {ratio:.3f}, runner-up {second:.3f})'])
    ia, ib = odis(f), ndis(g)
    idx = {a: n for n, (a, *_rest) in enumerate(ia)}
    if addr_abs not in idx:
        return dict(new=None, notes=['not an instruction boundary'])
    i = idx[addr_abs]
    pairs = dict(X.align(toks(ia), toks(ib)))
    if i not in pairs:
        return dict(new=None, notes=['instruction has no aligned twin'], function_ratio=round(ratio, 4))
    j = pairs[i]
    window = all(pairs.get(i + k) == j + k for k in (-2, -1, 1, 2) if 0 <= i + k < len(ia))
    same = exact_ops(ia[i], ib[j])
    conf = 'high' if ratio >= 0.97 and window and same else 'medium'
    return dict(new=ib[j][0] - new.base, method='aligned-code', confidence=conf,
                evidence=dict(old_function=hex(f - old.base), new_function=hex(g - new.base),
                              function_ratio=round(ratio, 4), runner_up=round(second, 4), window=window,
                              same_instruction=same, offset_old=hex(addr_abs - f), offset_new=hex(ib[j][0] - g)))


refs = collections.defaultdict(set)
for fs, (_, _, rs) in oix.items():
    for t in rs:
        refs[t].add(fs)
ref_keys = sorted(refs)


def map_data(addr_abs):
    exact = addr_abs in refs
    if exact:
        target = addr_abs
    else:
        i = bisect.bisect_right(ref_keys, addr_abs) - 1
        target = ref_keys[i] if i >= 0 and addr_abs - ref_keys[i] <= 0x400 else None
    if target is None:
        return dict(new=None, notes=['no RIP-relative reference at or below within 0x400'])
    votes, ev = collections.Counter(), []
    for f in sorted(refs[target])[:12]:
        g, ratio, second = pair_function(f)
        if g is None or ratio < 0.8 or ratio - second < 0.05:
            continue
        ia, ib = odis(f), ndis(g)
        pairs = dict(X.align(toks(ia), toks(ib)))
        for n, (a, s, m, o) in enumerate(ia):
            if 'rip' in o and X.rip_target(a, s, o) == target and n in pairs:
                b, s2, m2, o2 = ib[pairs[n]]
                t2 = X.rip_target(b, s2, o2)
                if t2 is not None:
                    votes[t2 - new.base + (addr_abs - target)] += 1
                    ev.append(dict(old_function=hex(f - old.base), new_function=hex(g - new.base), ratio=round(ratio, 3)))
    if not votes:
        return dict(new=None, notes=['no aligned referencing instruction'], referenced_at=hex(target - old.base))
    (best, n), *rest = votes.most_common(2)
    agree = not rest
    conf = 'high' if exact and n >= 2 and agree else 'medium' if n >= 1 and (agree or n >= 2 * rest[0][1]) else 'low'
    return dict(new=best, method='aligned-rip-refs' + ('' if exact else f' (struct base {hex(target - old.base)} +{hex(addr_abs - target)})'),
                confidence=conf, evidence=dict(votes={hex(k): v for k, v in votes.items()}, sample=ev[:4]))


out = {}
for rel in wanted:
    a = old.absolute(rel)
    r = map_code(a) if old.is_code(a) else map_data(a)
    if r.get('new') is not None:
        r['new'] = hex(r['new'])
    out[hex(rel)] = r
print(json.dumps(out, indent=1))
