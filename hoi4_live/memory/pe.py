"""The hoi4.exe image: sections and MSVC RTTI.

RTTI turns a vtable pointer found in live memory into a class name ("that object is a CArmy"). The walk is the one
the Linux project does for Itanium RTTI, here for the MSVC 64-bit layout:

    TypeDescriptor        {vfptr, spare, ".?AVCArmy@@"}                    in .data
    CompleteObjectLocator {signature=1, offset, cdOffset, pTypeDescriptor RVA, pClassDescriptor RVA, pSelf RVA}
    vtable                [-8] holds the VA of the locator, [0..] the virtual functions
"""

import os

from .. import platform
import pickle
import re
import struct
import time

import numpy as np

IMAGE_BASE_DEFAULT = 0x140000000


class Image:
    def __init__(self, path):
        self.path = path
        st = os.stat(path)
        self.size = st.st_size
        with open(path, "rb") as f:
            self.data = f.read()
        d = self.data
        e = struct.unpack_from("<I", d, 0x3C)[0]
        if d[e:e + 4] != b"PE\0\0":
            raise ValueError("not a PE file")
        _machine, nsec, self.timestamp, _sym, _nsym, optsz, _ch = struct.unpack_from("<HHIIIHH", d, e + 4)
        opt = e + 24
        self.image_base = struct.unpack_from("<Q", d, opt + 24)[0]
        self.sections = []
        so = opt + optsz
        for i in range(nsec):
            name, vsize, vaddr, rsize, rptr = struct.unpack_from("<8sIIII", d, so + 40 * i)
            self.sections.append((name.rstrip(b"\0").decode(), vaddr, vsize, rptr, rsize))
        self.build_key = "%x_%x" % (self.timestamp, self.size)
        self._img = None

    def section(self, name):
        for s in self.sections:
            if s[0] == name:
                return s
        raise KeyError(name)

    @property
    def image(self):
        """The image as mapped in memory (section contents at their RVAs)."""
        if self._img is None:
            end = max(va + max(vs, rs) for _n, va, vs, _rp, rs in self.sections)
            img = bytearray(end)
            for _n, va, vs, rp, rs in self.sections:
                m = min(rs, vs)
                img[va:va + m] = self.data[rp:rp + m]
            self._img = bytes(img)
        return self._img

    def rva_to_offset(self, rva):
        for _n, va, vs, rp, rs in self.sections:
            if va <= rva < va + max(vs, rs):
                return rp + rva - va
        return None

    def code(self, rva, n):
        o = self.rva_to_offset(rva)
        return self.data[o:o + n]

    def function_table(self):
        """Sorted (starts, ends) of the functions in .pdata (x64 unwind table)."""
        _n, va, vs, rp, rs = self.section(".pdata")
        t = np.frombuffer(self.data[rp:rp + rs], dtype=np.uint32)
        t = t[:len(t) // 3 * 3].reshape(-1, 3)
        order = np.argsort(t[:, 0])
        return t[order, 0].copy(), t[order, 1].copy()


def read_rtti(img):
    """{".?AVCArmy@@": [(vtable_rva, offset_in_object), ...]} for every class with a vtable and a locator."""
    im = img.image
    tds = {}
    for m in re.finditer(rb"\.\?A[VU][A-Za-z0-9_@\?\$]+?@@\x00", im):
        off = m.start() - 16
        if off >= 0:
            tds[off] = m.group()[:-1].decode()
    a32 = np.frombuffer(im[:len(im) // 4 * 4], dtype=np.uint32)
    cols = {}
    tdset = set(tds)
    for i in np.nonzero(a32[:-6] == 1)[0]:
        if int(a32[i + 3]) in tdset and int(a32[i + 5]) == int(i) * 4:
            cols[int(i) * 4] = (int(a32[i + 1]), int(a32[i + 3]))
    a64 = np.frombuffer(im[:len(im) // 8 * 8], dtype=np.uint64)
    colva = {img.image_base + c: c for c in cols}
    keys = np.array(sorted(colva), dtype=np.uint64)
    out = {}
    for p in np.nonzero(np.isin(a64, keys))[0]:
        c = colva[int(a64[p])]
        offset, td = cols[c]
        out.setdefault(tds[td], []).append((int(p) * 8 + 8, offset))
    return out


def cache_dir():
    base = platform.current().local_data_dir()
    d = os.path.join(base, "hoi4_live", "memory")
    os.makedirs(d, exist_ok=True)
    return d


def load_rtti(exe_path, cache=True):
    """(Image, rtti dict), with the RTTI table cached per build."""
    img = Image(exe_path)
    path = os.path.join(cache_dir(), "rtti_%s.pkl" % img.build_key)
    if cache and os.path.exists(path):
        try:
            with open(path, "rb") as f:
                return img, pickle.load(f)
        except Exception:
            pass
    t0 = time.time()
    rt = read_rtti(img)
    if cache:
        try:
            with open(path + ".tmp", "wb") as f:
                pickle.dump(rt, f)
            os.replace(path + ".tmp", path)
        except OSError:
            pass
    rt["__seconds__"] = time.time() - t0
    return img, rt
