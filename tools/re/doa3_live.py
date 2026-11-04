import struct,re,subprocess,sys,math
def pid():
    out=subprocess.run("ps -eo pid,cmd | grep -E 'Z:.*DOA3\\.exe' | grep -v grep",shell=True,capture_output=True,text=True).stdout.split()
    return int(out[0])
class G:
    def __init__(s):
        s.p=pid();s.m=open(f'/proc/{s.p}/mem','rb',buffering=0);s.maps=[]
        for l in open(f'/proc/{s.p}/maps'):
            a=l.split();lo,hi=[int(v,16) for v in a[0].split('-')]
            s.maps.append((lo,hi,a[1],a[5] if len(a)>5 else ''))
        s.off=s.find()
    def raw(s,a,n):
        s.m.seek(a);return s.m.read(n)
    def find(s):
        want=struct.pack('<f',2*math.pi/65536);w2=struct.pack('<f',1/10000)
        for off in [0]+[lo-0x10000 for lo,hi,pr,nm in s.maps if pr[0]=='r']+[lo for lo,hi,pr,nm in s.maps if pr[0]=='r' and hi-lo>0x400000]:
            if off<0: continue
            try:
                if s.raw(off+0x1ED790,4)==want and s.raw(off+0x1ED798,4)==w2: return off
            except OSError: pass
        return None
    def r(s,va,n): return s.raw(s.off+va,n)
    def u32(s,va): return struct.unpack('<I',s.r(va,4))[0]
    def u8(s,va): return s.r(va,1)[0]
if __name__=='__main__':
    g=G();print('pid',g.p,'offset',hex(g.off) if g.off is not None else None,'maps',len(g.maps))
    if g.off is None: sys.exit()
    print('chars id',g.u8(0x484C49),g.u8(0x484C49+0x68),'state 47E727',g.u8(0x47E727),'47E795',g.u8(0x47E795))
    for f in (0,1):
        base=0x4785D0+f*0x5C0
        M=struct.unpack('<%df'%(16*3),g.r(base,16*3*4))
        print('fighter',f,'node0 row3',[round(v,3) for v in M[12:16]],'node2 row3',[round(v,3) for v in M[32+12:32+16]])
