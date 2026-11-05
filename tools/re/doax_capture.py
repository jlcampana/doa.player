#!/usr/bin/env python3
"""Records poses from the running DOAX for the test `doax_pose_matches_capture`. Read-only.

doax_capture.py OUT [frames]. Each line: `C` character, table, 60 track values, then 32 matrices
under the character's root (bone 15), as 16 floats each with column vectors."""
import os
import struct
import sys
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import numpy as np
import doax_live as L

VALUES = 0x5E06A0  # pointer per character, 0x5C apart; tracks start 0xC in


def main():
    out, frames = sys.argv[1], int(sys.argv[2]) if len(sys.argv) > 2 else 8
    g = L.Guest()
    d, secs = L.xbe_sections()

    def fo(a):
        return next(ra + a - va for va, vs, ra, rs in secs if va <= a < va + rs)

    tables = [np.array(struct.unpack_from('<96f', d, fo(0x2CFD58 + t * 0x180))).reshape(24, 4)[:, :3] for t in range(8)]
    with open(out, 'w') as f:
        for _ in range(frames):
            for c in range(4):
                ptr = struct.unpack('<I', g.r(VALUES + c * 0x5C, 4))[0]
                for _ in range(40):
                    a = g.r(L.BONES + c * 0x800, 2048)
                    v = g.r(ptr + 0xC, 240)
                    if g.r(L.BONES + c * 0x800, 64) == a[:64]:
                        break
                m = np.array(struct.unpack('<512f', a)).reshape(32, 4, 4).transpose(0, 2, 1)
                dist = lambda p, q: np.linalg.norm(m[p, :3, 3] - m[q, :3, 3])
                t = min(range(8), key=lambda t: sum(abs(dist(p, q) - abs(tables[t][r][i])) for p, q, r, i in
                                                    [(6, 7, 3, 1), (3, 7, 5, 1), (4, 5, 1, 0), (1, 19, 21, 1), (0, 19, 15, 1)]))
                inv = np.linalg.inv(m[15])
                local = np.array([inv @ b for b in m])
                vals = struct.unpack('<60f', v)
                f.write('C %d %d ' % (c, t) + ' '.join('%.6f' % x for x in vals) + ' '
                        + ' '.join('%.6f' % x for x in local.transpose(0, 2, 1).reshape(-1)) + '\n')
            time.sleep(0.2)


if __name__ == '__main__':
    main()
