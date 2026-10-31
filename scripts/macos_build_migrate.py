#!/usr/bin/env python3
"""Map image-relative constants of one macOS HOI4 Mach-O build to another (static, read-only).

usage: macos_build_migrate.py OLD_BINARY NEW_BINARY [--src native/src] [--json out.json]

Every hex literal in the Rust sources that falls inside the old image is classified:
  vtable    old value is a vtable address point (8 bytes after a pointer to a __ZTI typeinfo symbol);
            mapped by class name and the same address-point index in the new binary
  symbol    old value is the address of a named symbol; mapped by name
  cstring   old value points into __cstring; mapped by identical string content
  section   old value is a section start/end; mapped to the same section boundary
  unmapped  anything else (heap-rooted globals, code addresses, struct values): needs live mapping
Image base is 0x100000000; constants are image-relative (address - base). No offsets are guessed.
"""
import re, struct, sys, json, pathlib, subprocess, collections

BASE = 0x100000000

class MachO:
    def __init__(self, path):
        self.data = pathlib.Path(path).read_bytes()
        d = self.data
        magic, cputype, _, _, ncmds, _, _, _ = struct.unpack_from('<IiiIIIII', d, 0)
        assert magic == 0xfeedfacf, 'not a 64-bit Mach-O'
        self.sections = []  # (segname, sectname, addr, size, fileoff)
        off = 32
        for _ in range(ncmds):
            cmd, size = struct.unpack_from('<II', d, off)
            if cmd == 0x19:  # LC_SEGMENT_64
                segname = d[off+8:off+24].rstrip(b'\0').decode()
                nsects = struct.unpack_from('<I', d, off+64)[0]
                so = off + 72
                for i in range(nsects):
                    sect = d[so:so+16].rstrip(b'\0').decode(); seg = d[so+16:so+32].rstrip(b'\0').decode()
                    addr, ssize, foff = struct.unpack_from('<QQI', d, so+32)
                    self.sections.append((seg, sect, addr, ssize, foff))
                    so += 80
            off += size
        self.syms = {}   # addr -> [names]
        self.byname = {}
        out = subprocess.run(['nm', '-n', path], capture_output=True, text=True).stdout
        for line in out.splitlines():
            p = line.split()
            if len(p) == 3 and p[1] not in 'Uu':
                a = int(p[0], 16); self.syms.setdefault(a, []).append(p[2]); self.byname.setdefault(p[2], a)
    def section_of(self, addr):
        for s in self.sections:
            if s[2] <= addr < s[2] + s[3]:
                return s
        return None
    def read(self, addr, n):
        s = self.section_of(addr)
        if not s or s[4] == 0 and s[1] in ('__bss', '__common'):
            return None
        fo = s[4] + (addr - s[2])
        return self.data[fo:fo+n]
    def u64(self, addr):
        b = self.read(addr, 8)
        return struct.unpack('<Q', b)[0] if b and len(b) == 8 else None
    def typeinfo_name(self, ti):
        """RTTI: typeinfo = {vptr, const char* name}; the name is the mangled class name (e.g. 17CCurrentGameState)."""
        np = self.u64(ti + 8)
        if np is None:
            return None
        s = self.cstring(np)
        if not s or not re.match(rb'^(N?[0-9]+[A-Za-z_][A-Za-z0-9_]*|N[0-9A-Za-z_]+E)$', s):
            return None
        return s.decode()
    def vtables(self):
        """Every vtable address point: data word at A-8 is a typeinfo whose name is a mangled class name and the
        word at A-16 (offset-to-top) is 0 or a small negative multiple of 8. Returns class -> [address points]."""
        lo = min(s[2] for s in self.sections if s[0].startswith('__DATA'))
        hi = max(s[2] + s[3] for s in self.sections if s[0].startswith('__DATA'))
        names = {}
        points = collections.defaultdict(list)
        for seg, sect, addr, size, foff in self.sections:
            if not seg.startswith('__DATA') or foff == 0:
                continue
            blob = self.data[foff:foff+size]
            for i in range(8, len(blob) - 7, 8):
                v = struct.unpack_from('<Q', blob, i)[0]
                if not (lo <= v < hi):
                    continue
                top = struct.unpack_from('<q', blob, i - 8)[0]
                if top > 0 or top < -0x10000 or top % 8:
                    continue
                if v not in names:
                    names[v] = self.typeinfo_name(v)
                n = names[v]
                if n:
                    points[n].append(addr + i + 8)
        return points
    def cstring(self, addr):
        s = self.section_of(addr)
        if not s or s[1] not in ('__cstring', '__const') or s[4] == 0:
            return None
        fo = s[4] + (addr - s[2]); end = self.data.find(b'\0', fo)
        if end < 0 or end - fo > 512:
            return None
        return self.data[fo:end]


def text_section(m):
    return [x for x in m.sections if x[0] == '__TEXT' and x[1] == '__text'][0]

def rip_refs(m, target, limit=40):
    """Positions (file offsets of disp32) of RIP-relative operands that resolve to `target` (old binary)."""
    seg, sect, addr, size, foff = text_section(m)
    d = m.data
    out = []
    # disp32 = target - (ip_after); ip_after = addr + (p - foff) + 4 + imm; try imm sizes 0,1,4
    for imm in (0, 1, 4):
        lo = target - (addr + size) - 8; hi = target - addr
        # brute force over all 4-byte windows is too slow; scan for each candidate via struct unpack of a sliding view
        pass
    import array
    return out

def global_by_signature(old, new, target, ctx=10):
    """Map a data-section address by voting over RIP-relative references in __text."""
    import collections, struct
    seg, sect, oaddr, osize, ofoff = text_section(old)
    nseg, nsect, naddr, nsize, nfoff = text_section(new)
    od, nd = old.data, new.data
    refs = []
    # find candidate disp positions: modrm with mod=00 rm=101 (RIP-relative) at p-1
    for imm in (0, 1, 4):
        start = ofoff + 2
        # disp needed at position p: target - (oaddr + (p - ofoff) + 4 + imm)
        # iterate over modrm bytes using bytes.find for each of the 8 reg encodings
        for modrm in (0x05, 0x0d, 0x15, 0x1d, 0x25, 0x2d, 0x35, 0x3d):
            pos = od.find(bytes([modrm]), start, ofoff + osize)
            while pos >= 0 and len(refs) < 64:
                p = pos + 1
                disp = struct.unpack_from('<i', od, p)[0]
                if oaddr + (p - ofoff) + 4 + imm + disp == target:
                    refs.append((p, imm))
                pos = od.find(bytes([modrm]), pos + 1, ofoff + osize)
    votes = collections.Counter()
    for p, imm in refs:
        before = od[p - ctx:p]
        after = od[p + 4 + imm:p + 4 + imm + ctx]
        q = nd.find(before, nfoff, nfoff + nsize)
        n = 0
        while q >= 0 and n < 50:
            np_ = q + ctx
            if nd[np_ + 4 + imm:np_ + 4 + imm + ctx] == after:
                disp = struct.unpack_from('<i', nd, np_)[0]
                votes[naddr + (np_ - nfoff) + 4 + imm + disp] += 1
            q = nd.find(before, q + 1, nfoff + nsize); n += 1
    return refs, votes


def resolve_imports(old_path, new_path, result):
    """__got / __la_symbol_ptr slots: map by the indirect symbol bound to the slot (otool -Iv)."""
    def table(path):
        out = subprocess.run(['otool', '-Iv', path], capture_output=True, text=True).stdout
        t = {}
        for line in out.splitlines():
            p = line.split()
            if len(p) >= 3 and p[0].startswith('0x'):
                t[int(p[0], 16)] = p[2]
        return t
    to, tn = table(old_path), table(new_path)
    by = {v: k for k, v in tn.items()}
    for k, e in result.items():
        if e.get('new') or e['kind'] != 'unmapped':
            continue
        a = int(k, 16) + BASE
        if a in to and to[a] in by:
            e.update(kind='import', name=to[a], new=hex(by[to[a]] - BASE))

def neighbour_deltas(result, window=0x800):
    """Unmapped __DATA addresses next to mapped ones in the same section get the neighbours' common delta (inferred)."""
    mapped = sorted((int(k, 16), int(e['new'], 16) - int(k, 16), e.get('section_hint')) for k, e in result.items()
                    if e.get('new') and e['kind'] in ('signature', 'symbol', 'import'))
    for k, e in result.items():
        if e.get('new') or not (e.get('section') or '').startswith('__DATA') and e['kind'] not in ('signature?',):
            continue
        a = int(k, 16)
        near = [d for (m, d, _) in mapped if abs(m - a) <= window]
        if near and len(set(near)) == 1:
            e.update(kind='neighbour', new=hex(a + near[0]), inferred_from=len(near))


def function_by_signature(old, new, target, n=48):
    """Map a code address by its first n bytes, masking rel32 operands of call/jmp (E8/E9) and RIP-relative disp32."""
    seg, sect, oaddr, osize, ofoff = text_section(old)
    nseg, nsect, naddr, nsize, nfoff = text_section(new)
    od, nd = old.data, new.data
    fo = ofoff + (target - oaddr)
    blob = bytearray(od[fo:fo+n])
    mask = [True] * len(blob)
    i = 0
    while i < len(blob) - 4:
        if blob[i] in (0xE8, 0xE9):
            for k in range(1, 5): mask[i+k] = False
            i += 5; continue
        if (blob[i] & 0xC7) == 0x05 and i > 0:
            for k in range(1, 5):
                if i + k < len(blob): mask[i+k] = False
        i += 1
    # anchor on the longest unmasked run
    runs, cur = [], None
    for j, m in enumerate(mask):
        if m and cur is None: cur = j
        if (not m or j == len(mask) - 1) and cur is not None:
            end = j if not m else j + 1; runs.append((end - cur, cur)); cur = None
    length, start = max(runs)
    anchor = bytes(blob[start:start+length])
    hits = []
    q = nd.find(anchor, nfoff, nfoff + nsize)
    while q >= 0 and len(hits) < 20:
        cand = q - start
        ok = all((not mask[j]) or nd[cand + j] == blob[j] for j in range(len(blob)))
        if ok: hits.append(naddr + (cand - nfoff))
        q = nd.find(anchor, q + 1, nfoff + nsize)
    return hits


def function_by_vtable_slot(old, new, target, ovt, nvt):
    """A code address stored in a vtable: new = same slot of the same class's address point in the new binary."""
    pt_class = {a: (c, i) for c, pts in ovt.items() for i, a in enumerate(pts)}
    starts = sorted(pt_class)
    import bisect
    found = set()
    for seg, sect, addr, size, foff in old.sections:
        if not seg.startswith('__DATA') or foff == 0:
            continue
        blob = old.data[foff:foff+size]
        needle = struct.pack('<Q', target)
        q = blob.find(needle)
        while q >= 0:
            if q % 8 == 0:
                a = addr + q
                k = bisect.bisect_right(starts, a) - 1
                if k >= 0 and a - starts[k] < 0x2000:
                    c, i = pt_class[starts[k]]
                    pts = nvt.get(c, [])
                    if i < len(pts):
                        v = new.u64(pts[i] + (a - starts[k]))
                        if v: found.add(v)
            q = blob.find(needle, q + 1)
    return sorted(found)

def find_constants(src):
    lo, hi = (0x200000 if '--text' in sys.argv else 0x2a00000), 0x3800000
    found = collections.defaultdict(set)
    for f in pathlib.Path(src).rglob('*.rs'):
        for m in re.finditer(r'0x([0-9a-fA-F_]{6,9})\b', f.read_text(errors='replace')):
            v = int(m.group(1).replace('_', ''), 16)
            if lo <= v < hi:
                found[v].add(str(f))
    return found

def main():
    old, new = MachO(sys.argv[1]), MachO(sys.argv[2])
    src = sys.argv[sys.argv.index('--src') + 1] if '--src' in sys.argv else 'native/src'
    consts = find_constants(src)
    ovt, nvt = old.vtables(), new.vtables()
    vt_index = {a: (c, i) for c, pts in ovt.items() for i, a in enumerate(pts)}
    newstr = {}
    for seg, sect, addr, size, foff in new.sections:
        if sect == '__cstring':
            blob = new.data[foff:foff+size]; pos = 0
            for part in blob.split(b'\0'):
                newstr.setdefault(part, addr + pos); pos += len(part) + 1
    result = {}
    for rel in sorted(consts):
        a = rel + BASE
        entry = {'old': hex(rel), 'files': sorted(consts[rel])}
        if a in vt_index:
            c, i = vt_index[a]
            pts = nvt.get(c, [])
            if i < len(pts) and len(pts) == len(ovt[c]):
                entry.update(kind='vtable', name=c, index=i, new=hex(pts[i] - BASE))
            else:
                entry.update(kind='vtable', name=c, index=i, new=None, note=f'address points old {len(ovt[c])} new {len(pts)}')
        elif a in old.syms:
            n = old.syms[a][0]; na = new.byname.get(n)
            entry.update(kind='symbol', name=n, new=hex(na - BASE) if na else None)
        elif (s := old.cstring(a)) is not None and s in newstr and old.section_of(a)[1] == '__cstring':
            entry.update(kind='cstring', name=s[:60].decode(errors='replace'), new=hex(newstr[s] - BASE))
        else:
            hit = None
            for s in old.sections:
                for edge, label in ((s[2], 'start'), (s[2] + s[3], 'end')):
                    if edge == a:
                        hit = (s, label)
            if hit:
                s, label = hit
                ns = [t for t in new.sections if t[0] == s[0] and t[1] == s[1]]
                if ns:
                    t = ns[0]
                    entry.update(kind='section', name=f'{s[0]},{s[1]} {label}', new=hex((t[2] if label == 'start' else t[2] + t[3]) - BASE))
            if 'kind' not in entry:
                sec = old.section_of(a)
                if sec and sec[0] == '__DATA' and '--signatures' in sys.argv:
                    refs, votes = global_by_signature(old, new, a)
                    if votes:
                        (best, n), *rest = votes.most_common(2) + [(None, 0)]
                        second = rest[0][1] if rest else 0
                        ok = n >= 2 and n >= 2 * second
                        entry.update(kind='signature' if ok else 'signature?', section=f'{sec[0]},{sec[1]}', refs=len(refs), votes=n, runner_up=second,
                                     new=hex(best - BASE) if ok else None, candidate=hex(best - BASE))
                if sec and sec[1] == '__text' and 'kind' not in entry:
                    vs = function_by_vtable_slot(old, new, a, ovt, nvt)
                    if len(vs) == 1:
                        entry.update(kind='vslot', section='__TEXT,__text', new=hex(vs[0] - BASE))
                if sec and sec[1] == '__text' and 'kind' not in entry:
                    hits = function_by_signature(old, new, a)
                    if len(hits) == 1:
                        entry.update(kind='function', section='__TEXT,__text', new=hex(hits[0] - BASE))
                    else:
                        entry.update(kind='function?', section='__TEXT,__text', candidates=[hex(h - BASE) for h in hits[:5]], new=None)
                if 'kind' not in entry:
                    entry.update(kind='unmapped', section=f'{sec[0]},{sec[1]}' if sec else None, new=None)
        result[hex(rel)] = entry
    resolve_imports(sys.argv[1], sys.argv[2], result)
    neighbour_deltas(result)
    kinds = collections.Counter(e['kind'] + ('' if e.get('new') else ':missing') for e in result.values())
    print(json.dumps(dict(kinds), indent=1), file=sys.stderr)
    out = sys.argv[sys.argv.index('--json') + 1] if '--json' in sys.argv else None
    text = json.dumps(result, indent=1)
    if out:
        pathlib.Path(out).write_text(text)
    else:
        print(text)

if __name__ == '__main__':
    main()
