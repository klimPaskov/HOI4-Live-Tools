import sys; sys.path.insert(0,'.')
import winmem, ident, struct
import os; pid=int(os.environ.get("HPID","49756"))
p=winmem.Proc(pid); base,_=winmem.module_base(pid); delta=base-0x140000000
def arrays(addr,span=0x2000):
    b=p.read(addr,span)
    out=[]
    for off in range(0,span-0x18,8):
        ptr,cap,size=struct.unpack_from('<QII',b,off)
        if 0x10000000000<ptr<0x7ff000000000 and 0<size<=cap<100000 and cap<1000000:
            try: first=p.u64(ptr)
            except Exception: continue
            r=ident.ident(p,delta,first) if first>0x10000000000 else None
            out.append((off,size,cap,r))
    return out
if __name__=='__main__':
    gs=p.u64(base+0x3316e90)
    print(hex(gs), ident.ident(p,delta,gs))
    for o,s,c,r in arrays(gs,0x3000):
        if r: print(hex(o),s,c,r)
