#!/usr/bin/env python3
"""Runs DOA3's tube ring solver (sub_00045080, mode 1 rings) from the game's own code, built by
tools/re/doa3_tube_ring_build.sh, on made-up rings, and writes the inputs and results as test
vectors for the Rust port (`doa3_tubes::ring_solver`).

doa3_tube_ring.py VECTORS.json [COUNT]

The call, as `sub_000418E0` makes it: sub_00045080(state, p0, p1, length, count, flags, target).
The state holds two lists of 2D points (16 bytes a point) at +0 and +4, and a sub-mode byte at
+0x419 (1 or 4). Results: both lists, the flags (a byte per point) and eax.
"""
import ctypes, json, os, random, struct, sys

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
lib = ctypes.CDLL(os.path.join(ROOT, 'target/doa3_tube_ring/libring.so'))
lib.h_init.restype = ctypes.POINTER(ctypes.c_uint8)
lib.h_fp.restype = ctypes.c_double
lib.h_esp.restype = ctypes.c_uint32
lib.h_eax.restype = ctypes.c_uint32
mem_ptr = lib.h_init()
SIZE = 64 << 20
mem = (ctypes.c_uint8 * SIZE).from_address(ctypes.addressof(mem_ptr.contents))

# The game's image, for its constants.
xbe = open(os.path.join(ROOT, 'assets/doa3/default.xbe'), 'rb').read()
base = struct.unpack_from('<I', xbe, 0x104)[0]
count = struct.unpack_from('<I', xbe, 0x11C)[0]
headers = struct.unpack_from('<I', xbe, 0x120)[0] - base
for i in range(count):
    flags, va, vsize, raw, rawsize = struct.unpack_from('<5I', xbe, headers + i * 56)
    if va + vsize < SIZE:
        mem[va:va + rawsize] = xbe[raw:raw + rawsize]

STACK, STATE, A, B, P0, P1, FLAGS = 0x3F00000, 0x3E00000, 0x3E01000, 0x3E02000, 0x3E03000, 0x3E03100, 0x3E03200


def w32(at, v): struct.pack_into('<I', mem, at, v & 0xFFFFFFFF)
def wf(at, v): struct.pack_into('<f', mem, at, v)
def rf(at): return struct.unpack_from('<f', mem, at)[0]


def run(a, b, p0, p1, length, target, mode):
    n = len(a)
    for at in range(STATE, STATE + 0x420, 4):
        w32(at, 0)
    w32(STATE, A)
    w32(STATE + 4, B)
    mem[STATE + 0x419] = mode
    for list_at, points in ((A, a), (B, b)):
        for i, (x, y) in enumerate(points):
            wf(list_at + i * 16, x); wf(list_at + i * 16 + 4, y); wf(list_at + i * 16 + 8, 0.0); wf(list_at + i * 16 + 12, 0.0)
    wf(P0, p0[0]); wf(P0 + 4, p0[1]); wf(P1, p1[0]); wf(P1 + 4, p1[1])
    for i in range(32):
        mem[FLAGS + i] = 0xAA
    esp = STACK
    for value in reversed([STATE, P0, P1, struct.unpack('<I', struct.pack('<f', length))[0], n, FLAGS, struct.unpack('<I', struct.pack('<f', target))[0]]):
        esp -= 4
        w32(esp, value)
    lib.h_set_esp(esp)
    lib.h_fp_clear()
    assert lib.h_call(0x45080) == 0
    return {
        'a': [[rf(A + i * 16), rf(A + i * 16 + 4)] for i in range(n)],
        'b': [[rf(B + i * 16), rf(B + i * 16 + 4)] for i in range(n)],
        'flags': list(mem[FLAGS:FLAGS + n]),
        'eax': lib.h_eax() & 0xFF,
    }


def ring(n, radius, wobble, rng):
    import math
    return [[radius * (1 + rng.uniform(-wobble, wobble)) * math.cos(2 * math.pi * i / n),
             radius * (1 + rng.uniform(-wobble, wobble)) * math.sin(2 * math.pi * i / n)] for i in range(n)]


def main():
    out = sys.argv[1]
    total = int(sys.argv[2]) if len(sys.argv) > 2 else 200
    rng = random.Random(1)
    vectors = []
    for k in range(total):
        n = rng.choice([4, 5, 6, 8, 10, 12])
        radius = rng.uniform(0.04, 0.12)
        a = ring(n, radius, 0.2, rng)
        b = [[x + rng.uniform(-0.02, 0.02), y + rng.uniform(-0.02, 0.02)] for x, y in a]
        p0 = [rng.uniform(-0.15, 0.15), rng.uniform(-0.15, 0.15)]
        p1 = [p0[0] + rng.uniform(-0.05, 0.05), p0[1] + rng.uniform(-0.05, 0.05)]
        length = rng.uniform(0.01, 0.2)
        target = rng.uniform(0.005, 0.08)
        mode = rng.choice([1, 4])
        inputs = {'a': a, 'b': b, 'p0': p0, 'p1': p1, 'length': length, 'target': target, 'mode': mode}
        # Round the inputs to f32 as the game holds them.
        f = lambda v: struct.unpack('<f', struct.pack('<f', v))[0]
        inputs = {key: ([[f(x), f(y)] for x, y in v] if key in 'ab' else [f(x) for x in v] if isinstance(v, list) else (f(v) if isinstance(v, float) else v)) for key, v in inputs.items()}
        result = run(inputs['a'], inputs['b'], inputs['p0'], inputs['p1'], inputs['length'], inputs['target'], inputs['mode'])
        vectors.append({'in': inputs, 'out': result})
    json.dump(vectors, open(out, 'w'))
    changed = sum(1 for v in vectors if v['out']['b'] != v['in']['b'])
    flagged = sum(1 for v in vectors if any(x == 1 for x in v['out']['flags']))
    print(f'{total} vectors, {changed} with the second list changed, {flagged} with a flag set')


if __name__ == '__main__':
    main()
