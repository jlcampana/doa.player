#!/usr/bin/env python3
"""Reads consecutive frames of chain particles (hair and similar) from the running DOA2U
game, read-only, as text for the doa2u_chain_single_steps test. Run from the repo root.

doa2u_chain_pairs.py OUT.txt [PAIRS_PER_RECORD]

Works like doa2u_cloth_pairs.py: a pair is kept after its chain has been quiet for 3 ms."""
import os
import struct
import sys
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from doa2u_live import Guest

STATE = 0x80B7A8
FIGHTER_STATE = 0xF994
CHAINS = 0x10
CHAIN_SIZE = 0x68
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
        for i in range(g.r(state, 12)[2]):
            record = g.r(state + CHAINS + i * CHAIN_SIZE, CHAIN_SIZE)
            count = record[0]
            particles = struct.unpack_from('<I', record, 0x48)[0] & 0x0FFFFFFF
            print('fighter', f, 'chain', i, 'count', count, 'bone', record[1], 'on', record[2],
                  'object', record[3], 'root vertex', hex(struct.unpack_from('<I', record, 0x44)[0]),
                  'id', g.r(0x87AE31 + 0x94 * f, 1)[0])
            if count and record[2]:
                records.append((f, i, particles, count * 0x40))

    def read(r):
        return g.r(MATRICES, 0x1000), g.r(r[2], r[3]), g.r(WIND_RING, 0x408)

    with open(out, 'w') as o:
        for r in records:
            print('rec', r[0], r[1], file=o)
        for k, r in enumerate(records):
            n, last, start = 0, None, time.time()
            while n < want and time.time() - start < 5:
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
                        print('wind', struct.unpack_from('<I', x[2], 0x404)[0],
                              *struct.unpack_from('<256f', x[2]), file=o)
                        for f in range(2):
                            print('matrices', f, *struct.unpack_from('<512f', x[0], f * 0x800), file=o)
                        print('before', k, *struct.unpack('<%df' % (r[3] // 4), last[0][1]), file=o)
                        print('after', k, *struct.unpack('<%df' % (r[3] // 4), x[1]), file=o)
                        n += 1
                    last = (x, time.time())
            print('record', k, 'pairs', n)


if __name__ == '__main__':
    main()
