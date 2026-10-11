import sys,time; sys.path.insert(0,'.')
import winmem, pickle, struct
pid=int(sys.argv[1])
base,_=winmem.module_base(pid); delta=base-0x140000000
p=winmem.Proc(pid)
tds,cols,vt=pickle.load(open('rtti.pkl','rb'))
want=['.?AVCCountry@@','.?AVCGameState@@','.?AVCArmy@@','.?AVCState@@','.?AVCInGameIdler@@','.?AVCProvince@@']
tg={}
for w in want:
    for a,o in vt[w]:
        if o==0: tg[0x140000000+a+delta]=w
t=time.time()
res=p.scan_qwords(tg.keys())
print('scan',time.time()-t)
out={}
for k,v in res.items(): out[tg[k]]=v; print(tg[k],len(v),[hex(x) for x in v[:3]])
pickle.dump((delta,out),open('inst.pkl','wb'))
