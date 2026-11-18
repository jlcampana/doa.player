#!/bin/bash
# Builds, under target/doa3_tube_ring (not tracked: it holds code generated from the game), DOA3's
# tube ring solver (sub_00045080 and what it calls, mode 1 rings) as plain C from the generated
# sources of the user's DOA3 install, with a minimal runtime, so that a port can be checked
# against it. Nothing here is committed but the build steps.
set -e
ROOT=$(cd "$(dirname "$0")/../.." && pwd)
DOA3=${DOA3_RE:-/home/pandi/Ports/Apps/DeadOrAlive3-Doa3Re}
G=$(ls -d "$DOA3"/private/program-*/generated | head -1)
H=$ROOT/target/doa3_tube_ring
mkdir -p "$H"
cp "$G/recomp_types.h" "$G/recomp_funcs.h" "$DOA3/recomp-runtime/recomp_math.c" "$DOA3/recomp-runtime/recomp_math.h" "$H/"
python3 - "$G" "$H" <<'PY'
import sys, re, glob
G, H = sys.argv[1:]
srcs = {f: open(f).read() for f in sorted(glob.glob(G + '/recomp_000*.c'))}
def find(a):
    for src in srcs.values():
        i = src.find(f'\nvoid sub_{a}(void)')
        if i >= 0:
            j = src.find('\n/**\n * sub_', i + 10)
            return src[i:j if j > 0 else len(src)]
todo, done = ['00045080'], {}
while todo:
    a = todo.pop()
    if a in done: continue
    body = find(a); done[a] = body
    # Stop at the C runtime (0x160000 and up), but for _ftol2, which calls nothing.
    if int(a, 16) >= 0x160000 and a != '0018DB30': continue
    for c in re.findall(r'(?:RECOMP_ABI_CALL\(0x|sub_)([0-9A-F]{8})', body or ''):
        if c not in done and c != a: todo.append(c)
keep = [b for a, b in done.items() if b and (int(a, 16) < 0x160000 or a == '0018DB30')]
open(H + '/ring_funcs.c', 'w').write('#define RECOMP_GENERATED_CODE\n#include "recomp_funcs.h"\n#include <math.h>\n' + '\n'.join(keep))
PY
cat > "$H/runtime.c" <<'C'
#include "recomp_types.h"
ptrdiff_t g_xbox_mem_offset; int g_force_return; uint32_t g_xbox_code_lo, g_xbox_code_hi;
RECOMP_TLS uint32_t g_eax, g_ecx, g_edx, g_esp, g_ebx, g_esi, g_edi;
RECOMP_TLS double g_fp_stack[8]; RECOMP_TLS int g_fp_top;
RECOMP_TLS uint32_t g_fs_base, g_seh_ebp, g_ebp; RECOMP_TLS int g_df; RECOMP_TLS uint32_t g_eflags;
RECOMP_TLS uint16_t g_fp_control_word = 0x27F; RECOMP_TLS int g_fp_cmp; RECOMP_TLS uint16_t g_fp_cc;
volatile uint32_t g_icall_trace[ICALL_TRACE_SIZE]; volatile uint32_t g_icall_trace_idx; volatile uint64_t g_icall_count;
RECOMP_TLS RecompXmm g_xmm0, g_xmm1, g_xmm2, g_xmm3, g_xmm4, g_xmm5, g_xmm6, g_xmm7;
volatile uint64_t g_icall_guard_hits, g_icall_guard_misses;
C
cat > "$H/stubs.c" <<'C'
#define RECOMP_GENERATED_CODE
#include "recomp_funcs.h"
#include <math.h>
/* _CIacos: st(0) = acos(st(0)), returning to the caller. */
void sub_0018E9D8(void) { g_fp_stack[g_fp_top] = acos(g_fp_stack[g_fp_top]); g_esp += 4; }
C
cat > "$H/harness.c" <<'C'
/* Calls the ring functions on a guest memory block, for tools/re/doa3_tube_ring.py. */
#define RECOMP_GENERATED_CODE
#include "recomp_funcs.h"
#include <stdlib.h>
#include <string.h>
#define GUEST_SIZE (64u << 20)
static uint8_t *memory;
uint8_t *h_init(void) {
    if (!memory) memory = calloc(GUEST_SIZE, 1);
    g_xbox_mem_offset = (ptrdiff_t)memory;
    return memory;
}
void h_set_esp(uint32_t value) { g_esp = value; }
uint32_t h_esp(void) { return g_esp; }
uint32_t h_eax(void) { return g_eax; }
void h_fp_clear(void) { g_fp_top = 0; for (int i = 0; i < 8; i++) g_fp_stack[i] = 0; }
double h_fp(int i) { return g_fp_stack[(g_fp_top + i) & 7]; }
int h_fp_top(void) { return g_fp_top; }
void sub_00043D80(void); void sub_00043DE0(void); void sub_00043E40(void); void sub_00043E80(void);
void sub_00044110(void); void sub_00044190(void); void sub_00044270(void); void sub_00044620(void);
void sub_00044AE0(void); void sub_00044C00(void); void sub_00044ED0(void); void sub_00045080(void);
/* Pushes a return address and calls; the callee pops it. */
int h_call(uint32_t address) {
    g_esp -= 4; MEM32(g_esp) = 0xDEAD0000u;
    switch (address) {
    case 0x43D80: sub_00043D80(); break; case 0x43DE0: sub_00043DE0(); break;
    case 0x43E40: sub_00043E40(); break; case 0x43E80: sub_00043E80(); break;
    case 0x44110: sub_00044110(); break; case 0x44190: sub_00044190(); break;
    case 0x44270: sub_00044270(); break; case 0x44620: sub_00044620(); break;
    case 0x44AE0: sub_00044AE0(); break; case 0x44C00: sub_00044C00(); break;
    case 0x44ED0: sub_00044ED0(); break; case 0x45080: sub_00045080(); break;
    default: return -1;
    }
    return 0;
}
C
cd "$H"
for f in ring_funcs runtime stubs recomp_math harness; do gcc -c -O1 -fPIC -w $f.c -o $f.o; done
gcc -shared -o libring.so ring_funcs.o runtime.o stubs.o recomp_math.o harness.o -lm
echo "Built $H/libring.so: tools/re/doa3_tube_ring.py calls it."
