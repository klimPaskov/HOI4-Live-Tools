import struct, re, pickle, time
import numpy as np
exec(open('pe_rtti.py').read().split("print(hex")[0])
text_va, text_sz, text_raw = 0x1000, 0x26efc28, 0x400
txt = np.frombuffer(data[text_raw:text_raw+text_sz], dtype=np.uint8)
t=time.time()
# .pdata
pd_va,pd_sz,pd_raw=0x35cf000,0x194da8,0x3315a00
pd=np.frombuffer(data[pd_raw:pd_raw+pd_sz],dtype=np.uint32).reshape(-1,3)
fstart=pd[:,0].copy(); fend=pd[:,1].copy()
order=np.argsort(fstart); fstart=fstart[order]; fend=fend[order]
print('functions',len(fstart))
def func_of(rva):
    i=np.searchsorted(fstart,rva,side='right')-1
    if i>=0 and rva<fend[i]: return int(fstart[i])
    return None
# E8 calls
idx=np.nonzero(txt[:-5]==0xE8)[0]
rel=txt[idx+1].astype(np.uint32)|(txt[idx+2].astype(np.uint32)<<8)|(txt[idx+3].astype(np.uint32)<<16)|(txt[idx+4].astype(np.uint32)<<24)
rel=rel.astype(np.int32).astype(np.int64)
tgt=(idx.astype(np.int64)+text_va+5+rel)
isfs=np.isin(tgt,fstart)
calls_site=idx[isfs]+text_va; calls_tgt=tgt[isfs]
print('calls',len(calls_site),time.time()-t)
# lea rip-relative: (48|4C) 8D modrm(mod=00,rm=101) rel32
ridx=np.nonzero(((txt[:-7]==0x48)|(txt[:-7]==0x4C))&(txt[1:-6]==0x8D)&((txt[2:-5]&0xC7)==0x05))[0]
r2=txt[ridx+3].astype(np.uint32)|(txt[ridx+4].astype(np.uint32)<<8)|(txt[ridx+5].astype(np.uint32)<<16)|(txt[ridx+6].astype(np.uint32)<<24)
r2=r2.astype(np.int32).astype(np.int64)
lea_site=ridx+text_va; lea_tgt=ridx.astype(np.int64)+text_va+7+r2
print('lea',len(lea_site),time.time()-t)
pickle.dump((fstart,fend,calls_site,calls_tgt,lea_site,lea_tgt),open('code.pkl','wb'))
