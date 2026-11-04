import struct,sys,collections
import numpy as np
BASE='/home/pandi/Ports/Apps/DeadOrAlive2Ultimate-Doa2uRe/private/play-disc/DOA2/'
def ks(seed,n):
    m=np.zeros(624,dtype=np.uint64); 
    x=seed&0xffffffff; L=[x]
    for i in range(1,624):
        x=(1812433253*(x^(x>>30))+i)&0xffffffff; L.append(x)
    m=L; out=[]
    while len(out)<n:
        a=np.array(m,dtype=np.uint32)
        # standard generate (in place, sequential)
        for k in range(624):
            y=(m[k]&0x80000000)|(m[(k+1)%624]&0x7fffffff)
            m[k]=m[(k+397)%624]^(y>>1)^(0x9908b0df if y&1 else 0)
        y=np.array(m,dtype=np.uint32)
        y=y^(y>>11); y=y^((y<<7)&0x9d2c5680); y=y^((y<<15)&0xefc60000); y=y^(y>>18)
        out.extend(((y.astype(np.uint64)+0x31415926)&0xffffffff).astype(np.uint32).tolist())
    return np.array(out[:n],dtype=np.uint32)
def dec(data,off,length,seed):
    n=length>>2
    a=np.frombuffer(data,dtype='<u4',count=n,offset=off).copy()
    a^=ks(seed,n)
    b=bytearray(data); b[off:off+n*4]=a.tobytes(); return bytes(b)
def open_afs(name):
    f=open(BASE+name,'rb'); h=f.read(8); n=struct.unpack('<I',h[4:])[0]
    t=struct.unpack('<%dI'%(2*n),f.read(8*n)); return f,n,t
if __name__=='__main__':
  for name in ('datahdd.afs','datahdd2.afs','datadvd.afs'):
    f,n,t=open_afs(name); c=collections.Counter(); ex={}
    for i in range(n):
        f.seek(t[2*i]); d=f.read(min(t[2*i+1],64))
        if len(d)<32: c['tiny']+=1; continue
        w=struct.unpack('<8I',d[:32])
        k=None
        if d[:4]==b'XPR0': k='plainXPR'+('+MDL' if d[20:24]==b'MDL\0' else '')
        elif w[0]==0x20: k='plainCAT'
        elif w[1]==t[2*i+1] and w[2]<=w[1] and (w[0]^0x30525058)<1<<24: k='encXPR'
        elif (w[0]^0x20)<1<<24 and w[0]!=0x20 and w[1]<=t[2*i+1]+0x1000: k='encCAT'
        else: k='other:%08x'%w[0] if w[0]<0x200 else 'other'
        c[k]+=1; ex.setdefault(k,i)
    print(name,n,c.most_common(14)); print(' first:',ex)
