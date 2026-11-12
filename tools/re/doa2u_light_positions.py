#!/usr/bin/env python3
"""Records the D3D lights DOA2U enables while it draws each fighter, with the fighter's pelvis
position, read-only, to see how a stage's point lights follow the fighters.

doa2u_light_positions.py SECONDS OUT.txt

Each line: time, fighter, pelvis x y z, ambient, then per enabled light its type, diffuse,
position, direction, range and attenuation."""
import os
import struct
import sys
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from doa2u_live import Guest
from doa2u_lights import DEVICE_GLOBAL, DRAWN_OBJECT, AMBIENT


def main():
    seconds, out = float(sys.argv[1]), sys.argv[2]
    g = Guest()
    device = struct.unpack('<I', g.r(DEVICE_GLOBAL, 4))[0] & 0x0FFFFFFF
    owner = {}
    for f in range(2):
        words = struct.unpack('<17I', g.r(0x850738 + 0x44 * f, 0x44))
        for p in struct.unpack('<70I', g.r(words[0] & 0x0FFFFFFF, 280)):
            if p:
                owner[p] = f
    area = g.r(0x8783A9, 1)[0]
    lines = 0
    with open(out, 'w') as o:
        print('area', g.r(0x87CFA4 + area, 1)[0], file=o)
        end = time.time() + seconds
        t0 = time.time()
        while time.time() < end:
            drawn = struct.unpack('<I', g.r(DRAWN_OBJECT, 4))[0]
            if drawn not in owner:
                continue
            f = owner[drawn]
            ambient = struct.unpack('<I', g.r(AMBIENT, 4))[0]
            slots = [s & 0x0FFFFFFF for s in struct.unpack('<8I', g.r(device + 0x7AC, 32)) if s]
            pelvis = struct.unpack('<16f', g.r(0x875C20 + f * 0x800 + 2 * 0x40, 64))[12:15]
            parts = [f'{time.time() - t0:.3f}', str(f), *(f'{v:.4f}' for v in pelvis), hex(ambient)]
            for s in slots:
                d = g.r(s, 0x68)
                kind = struct.unpack_from('<I', d, 0)[0]
                v = struct.unpack_from('<25f', d, 4)
                # diffuse 0..3, specular 4..7, ambient 8..11, position 12..14, direction 15..17,
                # range 18, falloff 19, attenuation 20..22
                parts.append(f'L{kind}:' + ','.join(f'{x:.4f}' for x in (*v[0:3], *v[12:15], *v[15:18], v[18], *v[20:23])))
            print(' '.join(parts), file=o)
            lines += 1
    print(lines, 'reads to', out)


if __name__ == '__main__':
    main()
