import pickle, numpy as np, collections, struct
tds,cols,vt=pickle.load(open('rtti.pkl','rb'))
fstart,fend,cs,ct,ls,lt=pickle.load(open('code.pkl','rb'))
IB=0x140000000
exec(open('pe_rtti.py').read().split("print(hex")[0])
def rva2off(rva):
    for n,va,vs,rp,rs,ch in secs:
        if va<=rva<va+max(vs,rs): return rp+rva-va
def code(rva,n): 
    o=rva2off(rva); return data[o:o+n]
fidx=np.searchsorted(fstart,cs,side='right')-1
call_func=fstart[fidx]
callees_of=collections.defaultdict(list)
for s,f,t in zip(cs,call_func,ct): callees_of[int(f)].append((int(s),int(t)))
callers_of=collections.defaultdict(list)
for s,t in zip(cs,ct): callers_of[int(t)].append(int(s))
def func_of(rva):
    i=np.searchsorted(fstart,rva,side='right')-1
    if i>=0 and rva<fend[i]: return int(fstart[i])
def fend_of(f):
    i=np.searchsorted(fstart,f); return int(fend[i])
def funcs_leaing(vtrva):
    m=lt==vtrva
    return sorted(set(func_of(int(x)) for x in ls[m]))
def hexs(b): return ' '.join('%02x'%x for x in b)
