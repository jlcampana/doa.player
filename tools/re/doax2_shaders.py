#!/usr/bin/env python3
"""DOAX2's Xenos shaders in the game's image: list them by constant names, disassemble one.

  doax2_shaders.py list [NAME ...]    containers whose constant table has all NAMEs:
                                      container, profile, microcode address and size, names
  doax2_shaders.py regs CONTAINER     the constant table: register, name, count, set
  doax2_shaders.py dis CONTAINER      control flow, ALU and fetch instructions (approximate:
                                      fetch formats and offsets are not decoded, scalar
                                      constant ops print their temp register as r?)

A container starts with 0x102A1100 (or 0x1101), its constant table at +0x28, the virtual part
of `virtualSize` (+4), the shader record at +0x18 (physical offset, size); the microcode sits
at container + virtualSize + physical offset. Swizzles are Xenos' relative ones; the compiler
permutes components freely. Field layouts as Xenia's ucode.h. Reads
assets/doax2/image_82000000.bin.
"""
import re
import struct
import sys

d = open(__file__.rsplit('/', 3)[0] + '/assets/doax2/image_82000000.bin', 'rb').read()
B = 0x82000000
def cstr(o):
    e=d.index(b'\0',o); return d[o:e].decode('latin1')
def try_ctab(base):
    try: size,creator,ver,n,info,flags,target=struct.unpack_from('>7I',d,base)
    except: return None
    if size!=0x1c or n==0 or n>300 or info>0x8000: return None
    out=[]
    for i in range(n):
        name,rs,ri,rc,res,ti,dv=struct.unpack_from('>IHHHHII',d,base+info+i*20)
        if name>0x10000: return None
        s=cstr(base+name)
        if not re.match(r'^[A-Za-z_$][A-Za-z0-9_\[\]$]*$',s): return None
        out.append((s,rs,ri,rc))
    return out,cstr(base+creator),cstr(base+target)
VOP=['add','mul','max','min','seq','sgt','sge','sne','frc','trunc','floor','mad','cndeq','cndge','cndgt','dp4','dp3','dp2add','cube','max4','setp_eq_push','setp_ne_push','setp_gt_push','setp_ge_push','kill_eq','kill_gt','kill_ge','kill_ne','dst','mova','?30','?31']
SOP={0:'adds',1:'adds_prev',2:'muls',3:'muls_prev',4:'muls_prev2',5:'maxs',6:'mins',7:'seqs',8:'sgts',9:'sges',10:'snes',11:'frcs',12:'truncs',13:'floors',14:'exp',15:'logc',16:'log',17:'rcpc',18:'rcpf',19:'rcp',20:'rsqc',21:'rsqf',22:'rsq',23:'movas',24:'movas_floor',25:'subs',26:'subs_prev',27:'setp_eq',28:'setp_ne',29:'setp_gt',30:'setp_ge',31:'setp_inv',32:'setp_pop',33:'setp_clr',34:'setp_rstr',35:'kills_eq',36:'kills_gt',37:'kills_ge',38:'kills_ne',39:'kills_one',40:'sqrt',42:'mulsc',43:'mulsc',44:'addsc',45:'addsc',46:'subsc',47:'subsc',48:'sin',49:'cos',50:'retain_prev'}
def sw(s,n=4):
    return ''.join('xyzw'[((s>>(2*i))+i)&3] for i in range(n))
def src(reg,sel,swz,neg,absc):
    if sel: t=('|r%d|' if reg&0x80 else 'r%d')%(reg&0x3F)
    else: t=('|c%d|' if absc else 'c%d')%reg
    s=sw(swz)
    return ('-' if neg else '')+t+('' if s=='xyzw' else '.'+s)
def alu(w0,w1,w2,vs=True):
    vop=(w2>>24)&31; sop=(w0>>26)&63
    regs=[(w2>>16)&255,(w2>>8)&255,w2&255]; sels=[(w2>>31)&1,(w2>>30)&1,(w2>>29)&1]
    swz=[(w1>>16)&255,(w1>>8)&255,w1&255]; neg=[(w1>>26)&1,(w1>>25)&1,(w1>>24)&1]
    absc=(w0>>7)&1
    vdst=w0&63; sdst=(w0>>8)&63; exp=(w0>>15)&1
    vmask=(w0>>16)&15; smask=(w0>>20)&15
    pred=''
    if (w1>>28)&1: pred='(p%s) '%('' if (w1>>27)&1 else '!')
    def dn(r): 
        if not exp: return 'r%d'%r
        return {62:'oPos',63:'oPts'}.get(r,'o%d'%r) if vs else 'oC%d'%r
    out=[]
    S=[src(regs[i],sels[i],swz[i],neg[i],absc) for i in range(3)]
    nsrc={'add':2,'mul':2,'max':2,'min':2,'seq':2,'sgt':2,'sge':2,'sne':2,'frc':1,'trunc':1,'floor':1,'mad':3,'cndeq':3,'cndge':3,'cndgt':3,'dp4':2,'dp3':2,'dp2add':3,'cube':2,'max4':1,'dst':2,'mova':1}.get(VOP[vop],2)
    if vmask:
        out.append(f"{pred}{VOP[vop]}{'_sat' if (w0>>24)&1 else ''} {dn(vdst)}.{''.join('xyzw'[i] for i in range(4) if vmask>>i&1)}, {', '.join(S[:nsrc])}")
    if smask or (sop not in (50,) and not vmask):
        sname=SOP.get(sop,'s?%d'%sop)
        if sop in (42,43,44,45,46,47):
            # const op: src3 is a constant, temp register in other bits
            t=((w2>>0)&0)|0
            sarg=f"{S[2]} , r{(swz[2]&0x3c)>>2 | ((w1>>24)&1)<<0}?"
        else:
            sarg=S[2]
        sd=dn(sdst) if not exp else dn(sdst)
        out.append(f"{pred}{sname}{'_sat' if (w0>>25)&1 else ''} {sd}.{''.join('xyzw'[i] for i in range(4) if smask>>i&1)}, {sarg}")
    return ' + '.join(out) if out else 'nop'
def fetch(w0,w1,w2):
    op=w0&31
    if op==0:
        src_=(w0>>5)&63; dst=(w0>>12)&63; ci=(w0>>20)&31; cs=(w0>>25)&3
        dsw=w1&0xFFF; comps=''.join('xyzw01?_'[(dsw>>(3*i))&7] for i in range(4))
        fmt=(w1>>16)&63; stride=w2&0xFF; off=(w2>>8)&0x7FFFFF
        return f"vfetch r{dst}.{comps}, r{src_}.{'xyzw'[(w2>>30)&3] if False else 'x'}, vf{ci*3+cs} (fmt {fmt}, stride {stride}, offset {off})"
    if op==1:
        src_=(w0>>5)&63; dst=(w0>>12)&63; ci=(w0>>20)&31
        dsw=w1&0xFFF; comps=''.join('xyzw01?_'[(dsw>>(3*i))&7] for i in range(4))
        return f"tfetch r{dst}.{comps}, r{src_}, tf{ci}"
    return f"fetch? op{op}"
def cf_list(base,size):
    W=struct.unpack_from('>%dI'%(size//4),d,base-B)
    cfs=[]
    i=0
    while i+2<len(W):
        a=W[i]|(W[i+1]&0xFFFF)<<32; b=(W[i+1]>>16)|W[i+2]<<16
        for c in (a,b):
            cfs.append(c)
        i+=3
        if any(((c>>44)&15) in (2,4,6,14) for c in (a,b)): break
    return W,cfs
def run(base,size,vs=True):
    W,cfs=cf_list(base,size)
    names={0:'nop',1:'exec',2:'exec_end',3:'cond_exec',4:'cond_exec_end',5:'cond_pred_exec',6:'cond_pred_exec_end',7:'loop_start',8:'loop_end',9:'cond_call',10:'return',11:'cond_jmp',12:'alloc',13:'cond_exec_pc',14:'cond_exec_pc_end',15:'vs_fetch_done'}
    for c in cfs:
        op=(c>>44)&15
        if op in (1,2,3,4,5,6,13,14):
            addr=c&0xFFF; cnt=(c>>12)&7; seq=(c>>16)&0xFFF
            extra=''
            if op in (3,4,13,14): extra=f" b{(c>>34)&0xFF} cond={(c>>42)&1}"
            if op in (5,6): extra=f" p cond={(c>>42)&1}"
            print(f"-- {names[op]} addr {addr} count {cnt}{extra}")
            for k in range(cnt):
                o=(addr+k)*3
                w0,w1,w2=W[o],W[o+1],W[o+2]
                isf=(seq>>(2*k))&1
                print(f"   {addr+k:3d} "+(fetch(w0,w1,w2) if isf else alu(w0,w1,w2,vs)))
        elif op==12:
            print(f"-- alloc {['?','position','interpolators','colors'][(c>>41)&3]} size {(c)&0xF}")
        elif op in (7,8,9,10,11):
            print(f"-- {names[op]} addr {c&0x3FF} raw {c:012x}")
        else:
            print(f"-- {names[op]} {c:012x}")


def containers():
    out = []
    for m in re.finditer(rb'\x10\*\x11[\x00\x01]', d):
        c = m.start()
        if c % 4:
            continue
        r = try_ctab(c + 0x28)
        if not r:
            continue
        vsize = struct.unpack_from('>I', d, c + 4)[0]
        soff = struct.unpack_from('>I', d, c + 0x18)[0]
        poff, size = struct.unpack_from('>II', d, c + soff)
        out.append((B + c, r[2], B + c + vsize + poff, size, r[0]))
    return out


def main():
    if len(sys.argv) > 1 and sys.argv[1] == 'list':
        for c, profile, code, size, table in containers():
            names = [t[0] for t in table]
            if all(w in names for w in sys.argv[2:]):
                print(hex(c), profile or '?', hex(code), hex(size), ' '.join(names))
    elif len(sys.argv) > 2 and sys.argv[1] in ('regs', 'dis'):
        want = int(sys.argv[2], 16)
        for c, profile, code, size, table in containers():
            if c == want:
                if sys.argv[1] == 'regs':
                    for name, regset, index, count in sorted(table, key=lambda t: (t[1], t[2])):
                        print(f"{ {0: 'b', 1: 'i', 2: 'c', 3: 's'}.get(regset, '?') }{index}", name, count)
                else:
                    run(code, size, profile != 'ps_3_0')
                return
        sys.exit('no container there')
    else:
        print(__doc__)


if __name__ == '__main__':
    main()
