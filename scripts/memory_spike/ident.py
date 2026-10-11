import pickle,sys,struct
sys.path.insert(0,'.')
import winmem
tds,cols,vt=pickle.load(open('rtti.pkl','rb'))
IB=0x140000000
vt_name={}
for n,l in vt.items():
    for a,o in l: vt_name[IB+a]=(n,o)
def ident(p,delta,addr):
    try: v=p.u64(addr)
    except Exception: return None
    return vt_name.get(v-delta)
