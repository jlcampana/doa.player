#!/usr/bin/env python3
"""Read-only memory oracle for the running DOAX recompilation. Never writes to the game.

`doax_live.py bones` prints each character's 32 world matrices (0x4D3650 + n * 0x800)."""
import os
import struct
import subprocess
import sys

ROOT = os.path.join(os.path.dirname(os.path.abspath(__file__)), '..', '..')
XBE = os.path.join(ROOT, 'assets', 'doax', 'default.xbe')
BONES = 0x4D3650


def xbe_sections(path=XBE):
    d = open(path, 'rb').read()
    base = struct.unpack_from('<I', d, 0x104)[0]
    n = struct.unpack_from('<I', d, 0x11C)[0]
    hdr = struct.unpack_from('<I', d, 0x120)[0] - base
    return d, [struct.unpack_from('<5I', d, hdr + i * 56)[1:] for i in range(n)]


def pid():
    out = subprocess.run(['pgrep', '-f', 'recomp_program_runner'], capture_output=True, text=True).stdout.split()
    for p in out:
        try:
            if 'DeadOrAliveXtreme' in os.readlink(f'/proc/{p}/exe'):
                return int(p)
        except OSError:
            pass
    sys.exit('DOAX is not running')


class Guest:
    def __init__(self):
        self.pid = pid()
        self.mem = open(f'/proc/{self.pid}/mem', 'rb', buffering=0)
        self.off = self.find()
        if self.off is None:
            sys.exit("Could not find DOAX's guest memory")

    def raw(self, a, n):
        self.mem.seek(a)
        return self.mem.read(n)

    def find(self):
        # A window of the XBE's largest section identifies the guest base.
        d, secs = xbe_sections()
        va, vs, ra, rs = max(secs, key=lambda s: s[3])
        probe = d[ra + 0x1000:ra + 0x1040]
        for line in open(f'/proc/{self.pid}/maps'):
            a = line.split()
            lo, hi = [int(v, 16) for v in a[0].split('-')]
            name = a[5] if len(a) > 5 else ''
            if a[1][0] != 'r' or hi - lo < 0x100000 or name.startswith('/usr') or 'dri' in name:
                continue
            try:
                i = self.raw(lo, hi - lo).find(probe)
            except OSError:
                continue
            if i >= 0:
                return lo + i - (va + 0x1000)
        return None

    def r(self, va, n):
        return self.raw(self.off + va, n)


def main():
    g = Guest()
    print(f'pid {g.pid} guest base {g.off:#x}')
    for n in range(4):
        rows = struct.unpack('<512f', g.r(BONES + n * 0x800, 2048))
        filled = [rows[b * 16:b * 16 + 16] for b in range(32)]
        used = sum(1 for m in filled if any(abs(v) > 1e-6 for v in m))
        print(f'character {n}: {used} of 32 matrices set')
        if used and len(sys.argv) > 1:
            for b, m in enumerate(filled):
                print(f'  {b:2} origin {m[12]:8.3f} {m[13]:8.3f} {m[14]:8.3f}  x axis {m[0]:6.2f} {m[1]:6.2f} {m[2]:6.2f}')


if __name__ == '__main__':
    main()
