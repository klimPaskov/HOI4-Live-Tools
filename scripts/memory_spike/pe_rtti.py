import struct, sys, re, pickle, time
import numpy as np
EXE = r"C:/Program Files (x86)/Steam/steamapps/common/Hearts of Iron IV/hoi4.exe"
data = open(EXE,'rb').read()
e_lfanew = struct.unpack_from('<I', data, 0x3c)[0]
assert data[e_lfanew:e_lfanew+4]==b'PE\0\0'
machine, nsec, ts, symptr, nsym, optsz, chars = struct.unpack_from('<HHIIIHH', data, e_lfanew+4)
opt = e_lfanew+24
magic = struct.unpack_from('<H', data, opt)[0]
imagebase = struct.unpack_from('<Q', data, opt+24)[0]
secs=[]
so = opt+optsz
for i in range(nsec):
    name, vsize, vaddr, rsize, rptr = struct.unpack_from('<8sIIII', data, so+40*i)[:5]
    ch = struct.unpack_from('<I', data, so+40*i+36)[0]
    secs.append((name.rstrip(b'\0').decode(), vaddr, vsize, rptr, rsize, ch))
print(hex(machine), hex(magic), hex(imagebase), ts)
for s in secs: print(s[0], hex(s[1]), hex(s[2]), hex(s[3]), hex(s[4]), hex(s[5]))
