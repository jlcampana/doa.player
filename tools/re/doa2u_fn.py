#!/usr/bin/env python3
"""Prints a function of the DOA2U generated C without its flag bookkeeping.

Run from private/program/runtime-sources: doa2u_fn.py ADDR [FROM_LOC] [TO_LOC]
For example: doa2u_fn.py 51F20 52363 527CC"""
# Prints a generated function, stripped of flag bookkeeping. usage: fn.py ADDR [from_loc] [to_loc]
import re,sys,glob
addr=sys.argv[1].upper().zfill(8)
for fn in sorted(glob.glob('recomp_00*.c')):
    src=open(fn).read()
    i=src.find(f'\nvoid sub_{addr}(void)')
    if i<0: continue
    j=src.find('\n/**\n * sub_',i)
    lines=src[i:j].split('\n')
    p=len(sys.argv)<3
    for l in lines:
        if len(sys.argv)>2 and l.startswith('loc_'+sys.argv[2].upper().zfill(8)): p=True
        if len(sys.argv)>3 and l.startswith('loc_'+sys.argv[3].upper().zfill(8)): p=False
        if not p: continue
        if re.search(r'_fk =|g_ebp = ebp|g_seh_ebp = ebp|g_eflags =|^\s*$|#define|#undef|\(void\)',l): continue
        if re.match(r'\s*(_fa|_fb|_cf|_fas|_fbs|_flags)\b',l) and 'if' not in l[:8]:
            m=re.search(r'/\* (cmp|test)[^*]*\*/',l)
            if m: print('    //',m.group(0))
            continue
        print(l[:112])
    break
