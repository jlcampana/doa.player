#!/usr/bin/env python3
"""Lists the D3D lights the running DOA2U game enables while it draws fighters, read-only.

doa2u_lights.py SECONDS

The lights change during a frame, so it counts every distinct set. Run from the repo root."""
import os
import struct
import sys
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from doa2u_live import Guest

DEVICE_GLOBAL = 0x3554F8
DRAWN_OBJECT = 0xEA6694
AMBIENT = 0x35542C
TYPES = {1: 'point', 2: 'spot', 3: 'directional'}


def light(g, address):
    """D3DLIGHT8: type, diffuse, specular, ambient, position, direction, range and more."""
    d = g.r(address & 0x0FFFFFFF, 0x68)
    kind = struct.unpack_from('<I', d, 0)[0]
    f = struct.unpack_from('<24f', d, 4)
    return (TYPES.get(kind, kind), tuple(round(x, 3) for x in f[:3]), tuple(round(x, 3) for x in f[4:7]),
            tuple(round(x, 3) for x in f[8:11]), tuple(round(x, 3) for x in f[12:15]),
            tuple(round(x, 3) for x in f[15:18]), round(f[18], 2))


def main():
    g = Guest()
    device = struct.unpack('<I', g.r(DEVICE_GLOBAL, 4))[0] & 0x0FFFFFFF
    fighters = set()
    for f in range(2):
        words = struct.unpack('<17I', g.r(0x850738 + 0x44 * f, 0x44))
        fighters.update(p for p in struct.unpack('<70I', g.r(words[0] & 0x0FFFFFFF, 280)) if p)
    print('stage byte', g.r(0x87CFA4, 1)[0], 'at 0x87CFA4')
    seen = {}
    end = time.time() + float(sys.argv[1])
    while time.time() < end:
        drawn = struct.unpack('<I', g.r(DRAWN_OBJECT, 4))[0]
        if drawn not in fighters:
            continue
        ambient = struct.unpack('<I', g.r(AMBIENT, 4))[0]
        slots = struct.unpack('<8I', g.r(device + 0x7AC, 32))
        key = (ambient, tuple(light(g, s) for s in slots if s))
        if struct.unpack('<I', g.r(DRAWN_OBJECT, 4))[0] == drawn:
            seen[key] = seen.get(key, 0) + 1
    for (ambient, lights), count in sorted(seen.items(), key=lambda kv: -kv[1])[:6]:
        print('seen', count, 'times, ambient', hex(ambient))
        for one in lights:
            print('  %s diffuse %s specular %s ambient %s position %s direction %s range %s' % one)


if __name__ == '__main__':
    main()
