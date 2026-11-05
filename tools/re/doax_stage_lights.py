#!/usr/bin/env python3
"""Lists the light sets in DOAX's executable. Run from the repo root.

A record is 0x58 bytes: +0 kind (1 directional, 2 point), +8 color, +0x18 diffuse, +0x24 and
+0x28 angles, 65536 per turn, +0x2C position, +0x54 specular factor."""
import math
import os
import struct
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from doax_live import xbe_sections

START, END = 0x287000, 0x2C0000
SIZE = 0x58


def record(d, at):
    k = struct.unpack_from('<I', d, at)[0]
    if k not in (1, 2):
        return None
    f = struct.unpack_from('<22f', d, at)
    a, b = struct.unpack_from('<2i', d, at + 0x24)
    if not all(-0.01 <= x <= 4 for x in f[6:9] + f[2:5]) or not 0 <= f[21] <= 2 or abs(a) > 0x10000 or abs(b) > 0x10000:
        return None
    t = 2 * math.pi / 65536
    direction = (-math.cos(a * t) * math.sin(b * t), -math.sin(a * t), -math.cos(a * t) * math.cos(b * t))
    return {'kind': k, 'color': f[2:5], 'diffuse': f[6:9], 'direction': direction, 'position': f[11:14], 'specular': f[21], 'at': at}


def sets():
    d, secs = xbe_sections()
    va, vs, raw, size = next(s for s in secs if s[0] <= START < s[0] + s[3])
    found, at = [], raw + START - va
    stop = raw + min(END - va, size)
    while at + SIZE <= stop:
        run = []
        p = at
        while p + SIZE <= stop and (r := record(d, p)):
            r['va'] = va + p - raw
            run.append(r)
            p += SIZE
        if 3 <= len(run) <= 12:
            found.append(run)
            at = p
        else:
            at += 4
    return found


def main():
    found = sets()
    print(len(found), 'runs; lengths', sorted(set(len(r) for r in found)))
    for run in found[:int(sys.argv[1]) if len(sys.argv) > 1 else 0]:
        print(hex(run[0]['va']), len(run))
        for r in run:
            print('  k%d color %s diffuse %s dir %s spec %.2f' % (r['kind'], [round(x, 2) for x in r['color']], [round(x, 2) for x in r['diffuse']], [round(x, 3) for x in r['direction']], r['specular']))


if __name__ == '__main__':
    main()
