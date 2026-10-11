import sys; sys.path.insert(0,'.')
from probe import *
import numpy as np
def msstr_try(a):
    try:
        b=p.read(a,32); size,cap=struct.unpack_from('<QQ',b,16)
        if 0<size<200 and size<=cap and cap<100000 and cap<16==(cap<16) :
            if cap<16: s=b[:size]
            else:
                ptr=struct.unpack_from('<Q',b,0)[0]; s=p.read(ptr,size)
            if all(32<=c<127 for c in s): return s.decode()
    except Exception: pass
def dump(addr, span, start=0):
    b=p.read(addr,span)
    for off in range(start,len(b)-8,8):
        q,=struct.unpack_from('<Q',b,off)
        lo,hi=struct.unpack_from('<ii',b,off)
        desc=[]
        if q>0x10000000000 and q<0x7ff000000000:
            r=ident.ident(p,delta,q)
            if r: desc.append('obj '+r[0][4:-2])
            else:
                try:
                    f=p.u64(q); r2=ident.ident(p,delta,f) if f>0x10000000000 else None
                    desc.append('ptr->' + (r2[0][4:-2] if r2 else hex(f)))
                except Exception: desc.append('ptr?')
            if off+0x18<=len(b):
                cap,size=struct.unpack_from('<ii',b,off+8)
                if 0<size<=cap<2000000: desc.append(f'ARR size={size} cap={cap}')
        elif q!=0:
            fx=struct.unpack_from('<q',b,off)[0]
            if abs(fx)>=1000 and abs(fx)<10**14 and fx%1000==0 or (abs(fx)>=100000 and abs(fx)<10**13): desc.append('fx %.3f'%(fx/1e5))
            if -1000000<lo<1000000 or -1000000<hi<1000000: desc.append(f'i32 {lo} {hi}')
            if q>>32==0 and lo==q: pass
            s=msstr_try(addr+off)
            if s: desc.append('STR '+s)
        else: continue
        print('%#06x %s'%(off+start,' | '.join(desc)))
if __name__=='__main__':
    c=int(sys.argv[1],16); span=int(sys.argv[2],16); st=int(sys.argv[3],16) if len(sys.argv)>3 else 0
    dump(c+st if False else c, span, st)
