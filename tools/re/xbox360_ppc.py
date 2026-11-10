#!/usr/bin/env python3
"""Disassembles Xbox 360 PowerPC code from DOAX2's image, with the VMX128 instructions capstone does
not know (fields and opcodes as Xenia decodes them).

  xbox360_ppc.py ADDRESS [COUNT]     e.g. xbox360_ppc.py 82489d68 380 (the chest springs' step)

Reads assets/doax2/image_82000000.bin. vmaddfp128 and vnmsubfp128 use vD as the third operand;
vsel128 uses vD as the mask.
"""
import struct,sys
from capstone import *
d=open(__file__.rsplit('/', 3)[0] + '/assets/doax2/image_82000000.bin', 'rb').read()
md=Cs(CS_ARCH_PPC,CS_MODE_32|CS_MODE_BIG_ENDIAN)
def b(w,hi,lo):  # LSB numbering
    return (w>>lo)&((1<<(hi-lo+1))-1)
def vd(w): return b(w,25,21)|(b(w,3,2)<<5)
def va(w): return b(w,20,16)|(b(w,10,10)<<6)|(b(w,5,5)<<5)
def vb(w): return b(w,15,11)|(b(w,1,0)<<5)
MEM={0x003:'lvsl128',0x043:'lvsr128',0x083:'lvewx128',0x0C3:'lvx128',0x183:'stvewx128',0x1C3:'stvx128',0x2C3:'lvxl128',0x3C3:'stvxl128',0x403:'lvlx128',0x443:'lvrx128',0x503:'stvlx128',0x543:'stvrx128',0x603:'lvlxl128',0x643:'lvrxl128',0x703:'stvlxl128',0x743:'stvrxl128'}
OP5={0x010:'vaddfp128',0x050:'vsubfp128',0x090:'vmulfp128',0x0D0:'vmaddfp128',0x110:'vmaddcfp128',0x150:'vnmsubfp128',0x190:'vmsum3fp128',0x1D0:'vmsum4fp128',0x200:'vpkshss128',0x210:'vand128',0x240:'vpkshus128',0x250:'vandc128',0x280:'vpkswss128',0x290:'vnor128',0x2C0:'vpkswus128',0x2D0:'vor128',0x300:'vpkuhum128',0x310:'vxor128',0x340:'vpkuhus128',0x350:'vsel128',0x380:'vpkuwum128',0x390:'vslo128',0x3C0:'vpkuwus128',0x3D0:'vsro128'}
OP6A={0x230:'vcfpsxws128',0x270:'vcfpuxws128',0x2B0:'vcsxwfp128',0x2F0:'vcuxwfp128',0x330:'vrfim128',0x370:'vrfin128',0x3B0:'vrfip128',0x3F0:'vrfiz128',0x630:'vrefp128',0x670:'vrsqrtefp128',0x6B0:'vexptefp128',0x6F0:'vlogefp128',0x710:'vrlimi128',0x730:'vspltw128',0x770:'vspltisw128',0x7F0:'vupkd3d128',0x610:'vpkd3d128'}
OP6B={0x050:'vrlw128',0x0D0:'vslw128',0x150:'vsraw128',0x1D0:'vsrw128',0x280:'vmaxfp128',0x2C0:'vminfp128',0x300:'vmrghw128',0x340:'vmrglw128',0x380:'vupkhsb128',0x3C0:'vupklsb128'}
CMP={0x000:'vcmpeqfp128',0x080:'vcmpgefp128',0x100:'vcmpgtfp128',0x180:'vcmpbfp128',0x200:'vcmpequw128'}
# Unaligned vector loads and stores (standard VMX encodings capstone leaves out).
X31={519:'lvlx',551:'lvrx',647:'stvlx',679:'stvrx',775:'lvlxl',807:'lvrxl',903:'stvlxl',935:'stvrxl'}
def dec(w,pc):
    op=w>>26
    if op==4 and (w&0x7F3) in MEM:
        return f"{MEM[w&0x7F3]} v{vd(w)}, r{b(w,20,16)}, r{b(w,15,11)}"
    if op==5:
        if (w&0x210)==0: # vperm128
            return f"vperm128 v{vd(w)}, v{va(w)}, v{vb(w)}, v{b(w,8,6)}"
        k=w&0x3D0
        if k in OP5: return f"{OP5[k]} v{vd(w)}, v{va(w)}, v{vb(w)}"
    if op==6:
        if (w&0x630)==0x210:
            perm=b(w,20,16)|(b(w,8,6)<<5)
            return f"vpermwi128 v{vd(w)}, v{vb(w)}, {perm:#04x} ({'xyzw'[perm>>6&3]}{'xyzw'[perm>>4&3]}{'xyzw'[perm>>2&3]}{'xyzw'[perm&3]})"
        k=w&0x7F0
        if k in OP6A:
            n=OP6A[k]
            if n=='vrlimi128': return f"{n} v{vd(w)}, v{vb(w)}, {b(w,20,16):#x}, {b(w,7,6)}"
            if n in('vspltw128','vspltisw128','vupkd3d128','vpkd3d128') or n.startswith('vc'): return f"{n} v{vd(w)}, v{vb(w)}, {b(w,20,16)}"
            return f"{n} v{vd(w)}, v{vb(w)}"
        k=w&0x3D0
        if k in OP6B: return f"{OP6B[k]} v{vd(w)}, v{va(w)}, v{vb(w)}"
        k=w&0x390
        if k in CMP: return f"{CMP[k]}{'.' if w&0x40 else ''} v{vd(w)}, v{va(w)}, v{vb(w)}"
    if op==31 and ((w>>1)&0x3FF) in X31:
        return f"{X31[(w>>1)&0x3FF]} v{b(w,25,21)}, r{b(w,20,16)}, r{b(w,15,11)}"
    ins=list(md.disasm(struct.pack('>I',w),pc))
    return f"{ins[0].mnemonic} {ins[0].op_str}" if ins else '??'
def dis(va_,n):
    out=[]
    for k in range(n):
        pc=va_+4*k; w=struct.unpack_from('>I',d,pc-0x82000000)[0]
        out.append(f"{pc:08x}: {w:08x}  {dec(w,pc)}")
    return out
if __name__=='__main__':
    print("\n".join(dis(int(sys.argv[1], 16), int(sys.argv[2]) if len(sys.argv) > 2 else 40)))
