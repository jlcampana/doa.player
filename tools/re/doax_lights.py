#!/usr/bin/env python3
"""Lists the D3D lights the running DOAX enables, read-only.

doax_lights.py SECONDS. Lights change during a frame, so it counts every distinct set with its
ambient. A set read while the device's world transform sits on one of the characters' bones is
marked as a character's. Run from the repo root."""
import os
import struct
import sys
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from doax_live import Guest, BONES

DEVICE_GLOBAL = 0x1F2978
AMBIENT = 0x1F2D54
ACTIVE_LIGHTS = 0x398
TRANSFORMS = 0x810
NEXT = 0x8C


def lights(g, device):
    """The active list: records of a D3DLIGHT8 as floats, with the next one's address at +0x8C."""
    out = []
    node = struct.unpack('<I', g.r(device + ACTIVE_LIGHTS, 4))[0]
    for _ in range(8):
        if not node:
            break
        d = g.r(node & 0x0FFFFFFF, 0x90)
        f = struct.unpack_from('<24f', d, 4)
        out.append((tuple(round(x, 3) for x in f[:3]), tuple(round(x, 3) for x in f[15:18])))
        node = struct.unpack_from('<I', d, NEXT)[0]
    return tuple(out)


def main():
    g = Guest()
    device = struct.unpack('<I', g.r(DEVICE_GLOBAL, 4))[0] & 0x0FFFFFFF
    seen, on_bone = {}, {}
    end = time.time() + float(sys.argv[1])
    while time.time() < end:
        origins = set()
        for c in range(4):
            bones = struct.unpack('<512f', g.r(BONES + c * 0x800, 2048))
            origins.update((round(bones[b * 16 + 12], 2), round(bones[b * 16 + 13], 2)) for b in range(32))
        for _ in range(200):
            ambient = struct.unpack('<I', g.r(AMBIENT, 4))[0]
            key = (ambient, lights(g, device))
            seen[key] = seen.get(key, 0) + 1
            slots = struct.unpack('<192f', g.r(device + TRANSFORMS, 768))
            for s in range(12):
                if (round(slots[s * 16 + 12], 2), round(slots[s * 16 + 13], 2)) in origins:
                    on_bone[key] = on_bone.get(key, 0) + 1
                    break
    for key, count in sorted(seen.items(), key=lambda kv: -kv[1])[:8]:
        ambient, found = key
        print('seen', count, 'on a bone', on_bone.get(key, 0), 'ambient', hex(ambient), 'lights', len(found))
        for diffuse, direction in found:
            print('  diffuse', diffuse, 'direction', direction)


if __name__ == '__main__':
    main()
