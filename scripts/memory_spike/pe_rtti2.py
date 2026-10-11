import struct, re, pickle, time
import numpy as np
exec(open('pe_rtti.py').read().split("print(hex")[0])
img = bytearray(0x3768000+0x70000)
for n,va,vs,rp,rs,ch in secs:
    img[va:va+min(rs,vs)] = data[rp:rp+min(rs,vs)]
img = bytes(img)
t=time.time()
# type descriptors: names ".?AV...@@"
tds={}
for m in re.finditer(rb'\.\?A[VU][A-Za-z0-9_@\?\$]+?@@\x00', img):
    off = m.start()-16
    if off<0: continue
    nm = m.group()[:-1].decode()
    tds[off]=nm
print(len(tds), time.time()-t)
# COLs: signature=1 (x64), offset, cdoffset, pTypeDescriptor rva, pClassDesc rva, pSelf rva
arr = np.frombuffer(img, dtype=np.uint32)
cols={}
td_rvas=np.array(sorted(tds), dtype=np.uint32)
# candidate: positions i where arr[i]==1 and arr[i+3] in td set and arr[i+5]==i*4
idx = np.nonzero(arr[:-6]==1)[0]
tdset=set(tds)
for i in idx:
    if arr[i+3] in tdset and arr[i+5]==i*4:
        cols[i*4]=(int(arr[i+1]), int(arr[i+2]), int(arr[i+3]))
print('cols',len(cols), time.time()-t)
# vtables: qword == imagebase+col_rva in .rdata
a64 = np.frombuffer(img[:len(img)//8*8], dtype=np.uint64)
colva = {imagebase+c:c for c in cols}
keys=np.array(sorted(colva), dtype=np.uint64)
mask=np.isin(a64, keys)
pos=np.nonzero(mask)[0]
vt={}
for p in pos:
    c=colva[int(a64[p])]
    off,cd,td=cols[c]
    vt.setdefault(tds[td],[]).append((int(p)*8+8, off))
print('classes with vtables',len(vt), time.time()-t)
pickle.dump((tds,cols,vt),open('rtti.pkl','wb'))
for nm in ['.?AVCArmy@@','.?AVCCountry@@','.?AVCGameState@@','.?AVCCommandQueue@@','.?AVCInGameIdler@@','.?AVCPauseGame@@','.?AVCLandCombat@@','.?AVCState@@','.?AVCProvince@@','.?AVCOrdersGroup@@','.?AVCMilitaryProductionLine@@']:
    print(nm, [(hex(imagebase+a),o) for a,o in vt.get(nm,[])])
