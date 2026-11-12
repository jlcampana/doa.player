#!/usr/bin/env python3
"""Records DOA3's tube state (section 4: rings of particles, such as Hitomi's trouser legs)
from the running game, read-only, for porting ring mode 1 (`sub_00044620`).

  doa3_tube_capture.py FIGHTER FRAMES OUT

Per read: the frame counter word, the fighter's 23 draw matrices (0x4785D0 + fighter * 0x5C0) and
inverse rotations (0x488BA0 + ...), the wind ring (0x90EF00, 32 vec4, head at 0x90F108) and the
soft body state's tube area (0x3C8408 + fighter * 0x215A0 + 0x2D04, 2 * 0x47D4 bytes). Each read
is written as a u32 length then the bytes, in that order. Reads are taken whenever the matrices
change, so consecutive reads are usually one game frame apart.
"""
import struct, sys, time
sys.path.insert(0, __file__.rsplit('/', 1)[0])
from doa3_live import G

fighter, frames, out = int(sys.argv[1]), int(sys.argv[2]), sys.argv[3]
g = G()
state = 0x3C8408 + fighter * 0x215A0
with open(out, 'wb') as f:
    last = None
    taken = 0
    while taken < frames:
        draw = g.r(0x4785D0 + fighter * 0x5C0, 0x5C0)
        if draw == last:
            time.sleep(0.002)
            continue
        last = draw
        blocks = [draw, g.r(0x488BA0 + fighter * 0x5C0, 0x5C0), g.r(0x90EF00, 0x20C), g.r(state + 0x2D04, 2 * 0x47D4)]
        for b in blocks:
            f.write(struct.pack('<I', len(b)))
            f.write(b)
        taken += 1
print(f'{taken} reads to {out}')
