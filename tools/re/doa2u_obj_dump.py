import sys,struct
sys.path.insert(0,__import__('os').path.dirname(__file__))
from doa2u_crypt import *
f,n,t=open_afs(sys.argv[1]); i=int(sys.argv[2])
f.seek(t[2*i]); d=f.read(t[2*i+1]); w=struct.unpack('<3I',d[:12])
p=dec(d,12,w[1]-12,w[0]^0x30525058) if d[:4]!=b'XPR0' else d
cnt=struct.unpack('<I',p[0x24:0x28])[0]
print('count',cnt,[hex(x) for x in struct.unpack('<5I',p[0x14:0x28])],'size',hex(w[1]),hex(w[2]))
offs=struct.unpack('<%dI'%cnt,p[0x28:0x28+4*cnt])
o=offs[int(sys.argv[3])]
L=int(sys.argv[4],16)
for i in range(0,L,16):
    r=p[o+i:o+i+16]; ws=struct.unpack('<4I',r); fs=struct.unpack('<4f',r)
    print('%6x %4x'%(o+i,i),' '.join('%08x'%x for x in ws),' '.join('%.3g'%f if 1e-4<abs(f)<1e4 else '.' for f in fs))
