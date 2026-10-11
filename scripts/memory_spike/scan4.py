import sys,time; sys.path.insert(0,'.')
import winmem, struct, numpy as np, pickle
p=winmem.Proc(49756)
lo=1936*365*24-24*40; hi=1936*365*24+24*400
c=[]
for base,size in p.regions():
    off=0
    while off<size:
        n=min(1<<24,size-off)
        try: b=p.read(base+off,n)
        except OSError: off+=n; continue
        a=np.frombuffer(b[:len(b)//4*4],dtype=np.int32)
        for i in np.nonzero((a>=lo)&(a<=hi))[0]: c.append((base+off+int(i)*4,int(a[i])))
        off+=n
pickle.dump(c,open('datecand.pkl','wb')); print('done',len(c),flush=True)
