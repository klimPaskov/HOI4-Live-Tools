import ctypes, ctypes.wintypes as wt, struct, numpy as np, pickle
k32 = ctypes.WinDLL('kernel32', use_last_error=True)
PROCESS_VM_READ=0x10; PROCESS_QUERY_INFORMATION=0x400
k32.OpenProcess.restype=wt.HANDLE
k32.ReadProcessMemory.argtypes=[wt.HANDLE, ctypes.c_void_p, ctypes.c_void_p, ctypes.c_size_t, ctypes.POINTER(ctypes.c_size_t)]
class MBI(ctypes.Structure):
    _fields_=[('BaseAddress',ctypes.c_void_p),('AllocationBase',ctypes.c_void_p),('AllocationProtect',wt.DWORD),('PartitionId',wt.WORD),('RegionSize',ctypes.c_size_t),('State',wt.DWORD),('Protect',wt.DWORD),('Type',wt.DWORD)]
k32.VirtualQueryEx.argtypes=[wt.HANDLE, ctypes.c_void_p, ctypes.POINTER(MBI), ctypes.c_size_t]
class Proc:
    def __init__(self, pid):
        self.pid=pid
        self.h=k32.OpenProcess(PROCESS_VM_READ|PROCESS_QUERY_INFORMATION, False, pid)
        if not self.h: raise OSError(ctypes.get_last_error())
    def read(self, addr, n):
        buf=ctypes.create_string_buffer(n); got=ctypes.c_size_t()
        ok=k32.ReadProcessMemory(self.h, ctypes.c_void_p(addr), buf, n, ctypes.byref(got))
        if not ok and got.value==0: raise OSError('read fail %x'%addr)
        return buf.raw[:got.value]
    def u64(self,a): return struct.unpack('<Q',self.read(a,8))[0]
    def i64(self,a): return struct.unpack('<q',self.read(a,8))[0]
    def i32(self,a): return struct.unpack('<i',self.read(a,4))[0]
    def regions(self, writable_only=True):
        a=0; mbi=MBI()
        while a<0x7fffffffffff:
            if not k32.VirtualQueryEx(self.h, ctypes.c_void_p(a), ctypes.byref(mbi), ctypes.sizeof(mbi)): break
            if mbi.State==0x1000 and not (mbi.Protect & 0x101):  # committed, not noaccess/guard
                if (not writable_only) or (mbi.Protect & 0xCC):  # RW / WC / EXECUTE_RW
                    yield mbi.BaseAddress or 0, mbi.RegionSize
            a=(mbi.BaseAddress or 0)+mbi.RegionSize
    def scan_qwords(self, targets, writable_only=True, chunk=1<<24):
        """yield (addr, value) for aligned qwords in targets"""
        keys=np.array(sorted(targets),dtype=np.uint64); res={}
        for base,size in self.regions(writable_only):
            off=0
            while off<size:
                n=min(chunk,size-off)
                try: b=self.read(base+off,n)
                except OSError: off+=n; continue
                a=np.frombuffer(b[:len(b)//8*8],dtype=np.uint64)
                m=np.isin(a,keys)
                for i in np.nonzero(m)[0]:
                    res.setdefault(int(a[i]),[]).append(base+off+int(i)*8)
                off+=n
        return res
def module_base(pid, name='hoi4.exe'):
    class ME(ctypes.Structure):
        _fields_=[('dwSize',wt.DWORD),('th32ModuleID',wt.DWORD),('th32ProcessID',wt.DWORD),('GlblcntUsage',wt.DWORD),('ProccntUsage',wt.DWORD),('modBaseAddr',ctypes.c_void_p),('modBaseSize',wt.DWORD),('hModule',wt.HMODULE),('szModule',ctypes.c_char*256),('szExePath',ctypes.c_char*260)]
    k32.CreateToolhelp32Snapshot.restype=wt.HANDLE
    s=k32.CreateToolhelp32Snapshot(0x8|0x10,pid)
    me=ME(); me.dwSize=ctypes.sizeof(me)
    ok=k32.Module32First(s,ctypes.byref(me))
    while ok:
        if me.szModule.decode().lower()==name: return me.modBaseAddr, me.modBaseSize
        ok=k32.Module32Next(s,ctypes.byref(me))
