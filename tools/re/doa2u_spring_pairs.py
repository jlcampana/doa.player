#!/usr/bin/env python3
"""Reads consecutive frames of spring points (kind 5) from the running DOA2U game, read-only, as
text for the doa2u_spring_single_steps test. Run from the repo root.

doa2u_spring_pairs.py OUT.txt [PAIRS_PER_RECORD]

Springs are at +0x2938 of the fighter's soft body state, count in byte +8, entries of 0xC bytes:
+0 point count, +4 the record, +8 the points (0x40 bytes: position, stretch, velocity). A pair is
kept when the points change within one frame of the last change and then stay quiet for 3 ms."""
import os
import struct
import sys
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from doa2u_live import Guest

STATE = 0x80B7A8
FIGHTER_STATE = 0xF994
SPRINGS = 0x2938
MATRICES = 0x875C20
WIND_RING = 0xE5AA40
QUIET = 0.003
ONE_FRAME = 0.021


def main():
    out = sys.argv[1]
    want = int(sys.argv[2]) if len(sys.argv) > 2 else 40
    g = Guest()
    records = []
    for f in range(2):
        state = STATE + f * FIGHTER_STATE
        for i in range(g.r(state, 12)[8]):
            entry = g.r(state + SPRINGS + i * 0xC, 0xC)
            count = struct.unpack_from('<H', entry, 0)[0]
            points = struct.unpack_from('<I', entry, 8)[0] & 0x0FFFFFFF
            print('fighter', f, 'spring', i, 'points', count, 'id', g.r(0x87AE31 + 0x94 * f, 1)[0])
            records.append((f, i, points, count * 0x40))

    def read(r):
        return g.r(MATRICES, 0x1000), g.r(r[2], r[3]), g.r(WIND_RING, 0x408)

    with open(out, 'w') as o:
        for r in records:
            print('rec', r[0], r[1], file=o)
        for k, r in enumerate(records):
            count, last, start = 0, None, time.time()
            while count < want and time.time() - start < 12:
                x = read(r)
                t = time.time()
                if last is None:
                    last = (x, t)
                elif x[1] != last[0][1]:
                    quiet = time.time()
                    while time.time() - quiet < QUIET:
                        y = read(r)
                        if y[1] != x[1]:
                            x, quiet = y, time.time()
                    if t - last[1] < ONE_FRAME:
                        print('wind', struct.unpack_from('<I', x[2], 0x404)[0], *struct.unpack_from('<256f', x[2]), file=o)
                        for f in range(2):
                            print('matrices', f, *struct.unpack_from('<512f', x[0], f * 0x800), file=o)
                        print('before', k, *struct.unpack('<%df' % (r[3] // 4), last[0][1]), file=o)
                        print('after', k, *struct.unpack('<%df' % (r[3] // 4), x[1]), file=o)
                        count += 1
                    last = (x, time.time())
            print('record', k, 'pairs', count)


if __name__ == '__main__':
    main()
