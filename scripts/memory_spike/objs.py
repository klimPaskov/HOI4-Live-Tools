import sys; sys.path.insert(0,'.')
from probe import *
def objfields(addr,span,skip=()):
    b=p.read(addr,span); out=[]
    for off in range(0,len(b)-8,8):
        q,=struct.unpack_from('<Q',b,off)
        if 0x10000000000<q<0x7ff000000000:
            r=ident.ident(p,delta,q)
            if r and r[1]==0 and not r[0].startswith(('.?AVCModifier','.?AVCNameGroup')): out.append((off,r[0][4:-2],q))
    return out
if __name__=='__main__':
    a=int(sys.argv[1],16); span=int(sys.argv[2],16)
    for o,n,q in objfields(a,span): print(hex(o),n,hex(q))
