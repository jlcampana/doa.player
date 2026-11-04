#!/usr/bin/env python3
"""Reads consecutive frames of cloth particles from the running DOA2U game, read-only, and
writes them as text for the doa2u_single_steps test. Run from the repo root.

doa2u_cloth_pairs.py OUT.txt [PAIRS_PER_RECORD]

The game writes a sheet row by row, with gaps, so a pair is kept only after its record has
been quiet for 7 ms. Each record is captured on its own: its particles are not from the
same frame as the next record's."""
import os
import struct
import sys
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from doa2u_live import Guest

STATE = 0x80B7A8
FIGHTER_STATE = 0xF994
CLOTH = 0x2090
CLOTH_SIZE = 0x7C
MATRICES = 0x875C20
WIND_RING = 0xE5AA40
QUIET = 0.007
ONE_FRAME = 0.021


def u32(g, va):
    return struct.unpack('<I', g.r(va & 0x0FFFFFFF, 4))[0]


def main():
    out = sys.argv[1]
    want = int(sys.argv[2]) if len(sys.argv) > 2 else 60
    g = Guest()
    records = []
    for f in range(2):
        state = STATE + f * FIGHTER_STATE
        for i in range(g.r(state, 12)[3]):
            record = g.r(state + CLOTH + i * CLOTH_SIZE, CLOTH_SIZE)
            columns, rows = record[5], record[6]
            table = struct.unpack_from('<I', record, 0x48)[0]
            first = u32(g, table + 0x14) & 0x0FFFFFFF
            size = (rows + 1) * 0x40 * columns
            vertices = None
            if record[0xC] & 8:
                objects = struct.unpack('<I', g.r(0x850738 + 0x44 * f, 4))[0]
                pointer = u32(g, objects + record[4] * 4)
                data = u32(g, u32(g, pointer + 0x24) + 4) & 0x0FFFFFFF
                vertices = (data, u32(g, pointer + 0x20) * 32)
            records.append((f, i, first, size, vertices))
            print('fighter', f, 'cloth', i, 'bone', record[2], 'object', record[4],
                  'columns', columns, 'rows', rows, 'flags', record[0xC:0x10].hex())

    def read(r):
        v = g.r(*r[4]) if r[4] else b''
        return g.r(MATRICES, 0x1000), g.r(r[2], r[3]), g.r(WIND_RING, 0x408), v

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
                        before = last[0]
                        print('wind', struct.unpack_from('<I', x[2], 0x404)[0],
                              *struct.unpack_from('<256f', x[2]), file=o)
                        for f in range(2):
                            print('matrices', f, *struct.unpack_from('<512f', x[0], f * 0x800), file=o)
                        print('vertices', *struct.unpack('<%df' % (len(x[3]) // 4), x[3]), file=o)
                        print('before', k, *struct.unpack('<%df' % (r[3] // 4), before[1]), file=o)
                        print('after', k, *struct.unpack('<%df' % (r[3] // 4), x[1]), file=o)
                        count += 1
                    last = (x, time.time())
            print('record', k, 'pairs', count)


if __name__ == '__main__':
    main()
