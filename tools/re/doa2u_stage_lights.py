#!/usr/bin/env python3
"""Lists DOA2's table of lights per area, from the executable. Run from the repo root.

Two tables of 71 sets, by area number: the stage's own from 0x6C9BF8, read by sub_00118570,
then the fighters' straight after. A set is 0x140 bytes, of four 0x50 byte records. A record: +0 kind, +4 a color, +0x10 diffuse, +0x1C and +0x20 angles, +0x24 position,
+0x30 attenuation, +0x3C range, +0x40 falloff, +0x44 and +0x48 cone angles, +0x4C specular
factor. Direction comes from the angles, 0x10000 per turn."""
import math
import os
import struct
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from doa2u_live import xbe_sections

BASE = 0x6C9BF8
SETS = 71


def lights():
    d, sections = xbe_sections()

    def rd(va, n):
        for v, _, raw, size in sections:
            if v <= va < v + size:
                return d[raw + va - v:raw + va - v + n]
        raise ValueError(hex(va))

    table = []
    turn = 2 * math.pi / 0x10000
    for s in range(SETS * 2):
        records = []
        for r in range(4):
            data = rd(BASE + s * 0x140 + r * 0x50, 0x50)
            kind = struct.unpack_from('<I', data, 0)[0]
            f = struct.unpack_from('<20f', data, 0)
            a, b = struct.unpack_from('<2i', data, 0x1C)
            direction = (-math.cos(a * turn) * math.sin(b * turn), -math.sin(a * turn), -math.cos(a * turn) * math.cos(b * turn))
            records.append({'kind': kind, 'color': f[1:4], 'diffuse': f[4:7], 'direction': direction, 'position': f[9:12],
                            'attenuation': f[12:15], 'range': f[15], 'specular': f[19]})
        table.append(records)
    return table


def main():
    for s, records in enumerate(lights()):
        print('stage set' if s < SETS else 'fighter set', s % SETS)
        for r in records:
            print('  k%d color %s diffuse %s dir %s pos %s spec %.2f' % (
                r['kind'], tuple(round(x, 3) for x in r['color']), tuple(round(x, 3) for x in r['diffuse']),
                tuple(round(x, 3) for x in r['direction']), tuple(round(x, 2) for x in r['position']), r['specular']))


if __name__ == '__main__':
    main()
