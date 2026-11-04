#!/usr/bin/env python3
"""Lists the light records in DOA2's executable. Run from the repo root.

A record is 0x50 bytes: kind, a color, diffuse, two angles, position, attenuation, range,
falloff, two cone angles, and the specular factor. Direction comes from the angles."""
import math
import os
import struct
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from doa2u_live import xbe_sections

RECORD = 0x50


def records():
    d, sections = xbe_sections()
    found = []
    for va, _, raw, size in sections:
        data = d[raw:raw + size]
        for at in range(0, len(data) - RECORD, 4):
            kind = struct.unpack_from('<I', data, at)[0]
            if kind > 4:
                continue
            f = struct.unpack_from('<20f', data, at)
            # Falloff, cone angles and specular factor, which all the records seen share.
            if not (abs(f[16] - 1) < 1e-3 and abs(f[17] - 0.5236) < 1e-3 and abs(f[18] - 1.0472) < 1e-3 and 0 <= f[19] <= 2):
                continue
            a, b = struct.unpack_from('<2i', data, at + 28)
            if not all(math.isfinite(x) and abs(x) < 1000 for x in f[1:7]) or abs(a) > 0x10000 or abs(b) > 0x10000:
                continue
            turn = 2 * math.pi / 0x10000
            direction = (-math.cos(a * turn) * math.sin(b * turn), -math.sin(a * turn), -math.cos(a * turn) * math.cos(b * turn))
            found.append({'va': va + at, 'kind': kind, 'color': f[1:4], 'diffuse': f[4:7], 'direction': direction,
                          'angles': (a, b), 'position': f[9:12], 'range': f[15], 'specular': f[19]})
    return found


def main():
    list_ = records()
    print(len(list_), 'records')
    groups = []
    for r in list_:
        if groups and r['va'] - groups[-1][-1]['va'] == RECORD:
            groups[-1].append(r)
        else:
            groups.append([r])
    print(len(groups), 'runs, sizes', sorted(set(len(g) for g in groups)))
    for g in groups:
        print(hex(g[0]['va']), len(g))
        for r in g:
            print('   k%d color %s diffuse %s dir %s spec %.2f' % (
                r['kind'], tuple(round(x, 2) for x in r['color']), tuple(round(x, 2) for x in r['diffuse']),
                tuple(round(x, 3) for x in r['direction']), r['specular']))


if __name__ == '__main__':
    main()
