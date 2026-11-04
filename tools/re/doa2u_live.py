#!/usr/bin/env python3
"""Read-only memory oracle for the running DOA2U recompilation. Never writes to the game."""
import re, struct, subprocess, sys

XBE = '/home/pandi/Ports/Apps/DeadOrAlive2Ultimate-Doa2uRe/private/play-disc/DOA2.xbe'


def xbe_sections(path=XBE):
    d = open(path, 'rb').read()
    base = struct.unpack_from('<I', d, 0x104)[0]
    n = struct.unpack_from('<I', d, 0x11C)[0]
    hdr = struct.unpack_from('<I', d, 0x120)[0] - base
    out = []
    for i in range(n):
        fl, va, vs, ra, rs = struct.unpack_from('<5I', d, hdr + i * 56)
        out.append((va, vs, ra, rs))
    return d, out


def pid():
    o = subprocess.run(['pgrep', '-f', 'recomp_program_runner'], capture_output=True, text=True).stdout.split()
    return int(o[0])


class Guest:
    def __init__(s):
        s.pid = pid()
        s.mem = open(f'/proc/{s.pid}/mem', 'rb', buffering=0)
        s.maps = []
        for l in open(f'/proc/{s.pid}/maps'):
            a = l.split()
            lo, hi = [int(v, 16) for v in a[0].split('-')]
            s.maps.append((lo, hi, a[1], a[5] if len(a) > 5 else ''))
        s.off = s.find()

    def raw(s, a, n):
        s.mem.seek(a)
        return s.mem.read(n)

    def find(s):
        # A window of the XBE's largest read-only section identifies the guest base.
        d, secs = xbe_sections()
        va, vs, ra, rs = max(secs, key=lambda x: x[3])
        probe = d[ra + 0x1000:ra + 0x1040]
        for lo, hi, pr, nm in s.maps:
            if pr[0] != 'r' or hi - lo < 0x100000 or nm.startswith('/usr') or 'dri' in nm:
                continue
            try:
                buf = s.raw(lo, hi - lo)
            except OSError:
                continue
            i = buf.find(probe)
            if i >= 0:
                return lo + i - (va + 0x1000)
        return None

    def r(s, va, n):
        return s.raw(s.off + va, n)

    def u32(s, va):
        return struct.unpack('<I', s.r(va, 4))[0]

    def floats(s, va, n):
        return struct.unpack(f'<{n}f', s.r(va, n * 4))




def pelvis_error(tracks, matrices, f):
    """How far the pelvis matrix is from its tracks. The game writes the tracks
    before it builds the matrices, so a read can catch them a frame apart."""
    import math
    x, y, z = tracks[3:6]
    cx, sx, cy, sy, cz, sz = math.cos(x), math.sin(x), math.cos(y), math.sin(y), math.cos(z), math.sin(z)
    # Local rotation of matrix 2 under matrix 15, in the game's row vectors.
    want = [[cz * cy, sz * cy, -sy], [cz * sy * sx - sz * cx, sz * sy * sx + cz * cx, cy * sx],
            [cz * sy * cx + sz * sx, sz * sy * cx - cz * sx, cy * cx]]
    root = struct.unpack_from('<16f', matrices, f * 0x800 + 15 * 0x40)
    pelvis = struct.unpack_from('<16f', matrices, f * 0x800 + 2 * 0x40)
    worst = 0.0
    for i in range(3):
        for j in range(3):
            have = sum(want[i][k] * root[k * 4 + j] for k in range(3))
            worst = max(worst, abs(have - pelvis[i * 4 + j]))
    return worst


def capture(g, out, frames=12):
    """Writes frames of tracks, bones and matrices of both fighters, for the Rust pose test.
    A frame is kept only if two reads in a row agree and the pelvis matches its tracks."""
    import math
    import time
    angle = 2 * math.pi / 65536
    ids = [g.r(0x87AE31 + 0x94 * f, 1)[0] for f in range(2)]
    blocks = [g.u32(0x8A7550 + 4 * f) for f in range(2)]
    bones = [g.floats(g.u32(0x6978E0 + 4 * i), 22 * 4) for i in ids]

    def read():
        return g.r(0x875C20, 0x1000), [g.r(p, 0xD0) for p in blocks]

    kept = 0
    with open(out, 'w') as o:
        while kept < frames:
            first = read()
            while first != (second := read()):
                first = second
            matrices, tracks = first
            for f in range(2):
                b = tracks[f]
                t = [0.0] * 44
                t[0:3] = struct.unpack_from('<3f', b, 0)
                rots = struct.unpack_from('<9i', b, 0x10)
                t[3:6] = [v * angle for v in rots[6:9]]
                t[6:9] = [v * angle for v in rots[0:3]]
                t[9:12] = [v * angle for v in rots[3:6]]
                for l in range(4):
                    r = 0x34 + l * 0x24
                    t[12 + 8 * l:15 + 8 * l] = struct.unpack_from('<3f', b, r)
                    rr = struct.unpack_from('<5i', b, r + 0x10)
                    t[15 + 8 * l:20 + 8 * l] = [v * angle for v in rr]
                if pelvis_error(t, matrices, f) > 0.0005:
                    continue
                kept += 1
                m = struct.unpack_from('<512f', matrices, f * 0x800)
                print('frame', ids[f], file=o)
                print('tracks', *t, file=o)
                print('bones', *bones[f], file=o)
                print('matrices', *m, file=o)
            time.sleep(0.25)


if __name__ == '__main__':
    g = Guest()
    print('pid', g.pid, 'offset', hex(g.off) if g.off is not None else None)
    if g.off is None:
        sys.exit(1)
    if len(sys.argv) > 2 and sys.argv[1] == 'capture':
        capture(g, sys.argv[2], int(sys.argv[3]) if len(sys.argv) > 3 else 12)
        sys.exit()
    for f in range(2):
        b = 0x850738 + 0x44 * f
        print('fighter', f, ' '.join(f'{v:08x}' for v in struct.unpack('<17I', g.r(b, 0x44))))
