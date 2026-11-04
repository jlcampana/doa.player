#!/usr/bin/env python3
"""Lists the D3D lights the running DOA2U game enables, read-only. doa2u_lights.py SECONDS

The lights change during a frame, so it prints every distinct set seen. Run from the repo root."""
import os
import struct
import sys
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from doa2u_live import Guest

DEVICE_GLOBAL = 0x3554F8
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
    print('device', hex(device))
    seen = {}
    end = time.time() + float(sys.argv[1])
    while time.time() < end:
        slots = struct.unpack('<8I', g.r(device + 0x7AC, 32))
        key = tuple(light(g, s) for s in slots if s)
        seen[key] = seen.get(key, 0) + 1
        time.sleep(0.002)
    for key, count in sorted(seen.items(), key=lambda kv: -kv[1])[:12]:
        print('set seen', count, 'times')
        for one in key:
            print('  %s diffuse %s specular %s ambient %s position %s direction %s range %s' % one)


if __name__ == '__main__':
    main()
