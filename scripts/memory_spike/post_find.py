import pickle, numpy as np, collections
tds,cols,vt=pickle.load(open('rtti.pkl','rb'))
fstart,fend,cs,ct,ls,lt=pickle.load(open('code.pkl','rb'))
def func_of(rva):
    i=np.searchsorted(fstart,rva,side='right')-1
    if i>=0 and rva<fend[i]: return int(fstart[i])
def funcs_leaing(vtrva):
    m=lt==vtrva
    return sorted(set(func_of(int(x)) for x in ls[m]))
def callers(f):
    m=ct==f
    return [int(x) for x in cs[m]]
callees_of=collections.defaultdict(set)
for s,t in zip(cs,ct):
    f=func_of(int(s))
    if f is not None: callees_of[f].add(int(t))
name='.?AVCPauseGame@@'
print(vt[name])
for vtrva,off in vt[name]:
    fl=funcs_leaing(vtrva); print(hex(vtrva),[hex(f) for f in fl])
    for f in fl:
        cl=callers(f); print(' fn',hex(f),'callers',[hex(c) for c in cl][:10])
