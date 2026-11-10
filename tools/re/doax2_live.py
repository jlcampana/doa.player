#!/usr/bin/env python3
"""Read-only memory oracle for the running DOAX2 Recomp. Never writes to the game.

`doax2_live.py bones` finds the characters' bone arrays (32 matrices of 64 bytes, big-endian,
translation in the last row) and prints each bone's position. Array index = CAT bone number:
0-15 DOAX's bones, 16 17 eyes, 18 right collar, 19 right toe, 20 neck, 21 left collar,
22 left toe, 23 spine, 24-31 DOAX's arm roll bones. Found at guest 0x93AC4630, 0x93AD1E90 and
0x93AFA7B0 in a jet ski race; they move with the heap, so this scans for them.

`doax2_live.py array OUT [SAMPLES]` writes the game's own bone array (guest 0x833B5310, 32
bones a character slot, the one the chest springs read) every 0.4 s, in the format of
`target/captures/doax2_live.txt` (address, sample, 32 matrices by rows), for the ignored tests
`live_poses_match_motions` and `live_spring_bones_match`.

Guest address `a` is at host `0x200000000 + a` in the doax2.exe process.
"""
import subprocess
import sys

import numpy as np

HOST_BASE = 0x200000000
SCAN = (0x90000000, 0xA0000000)


def pid():
    out = subprocess.run(['pgrep', '-f', 'doax2.exe'], capture_output=True, text=True).stdout.split()
    for p in out:
        try:
            if 'doax2.exe' in open(f'/proc/{p}/cmdline', 'rb').read().decode('latin1'):
                return int(p)
        except OSError:
            pass
    sys.exit('DOAX2 is not running')


class Guest:
    def __init__(self):
        self.pid = pid()
        self.mem = open(f'/proc/{self.pid}/mem', 'rb', buffering=0)

    def read(self, address, size):
        self.mem.seek(HOST_BASE + address)
        return self.mem.read(size)

    def matrices(self, address, count):
        """Matrices as column-vector 4x4 arrays."""
        raw = np.frombuffer(self.read(address, 64 * count), dtype='>f4').reshape(count, 4, 4)
        return [m.astype(np.float64).T for m in raw]

    def bone_arrays(self):
        """Starts of runs of 32 or more bone-like matrices, 64 bytes apart."""
        hits = []
        step = 0x1000000
        for start in range(SCAN[0], SCAN[1], step):
            try:
                raw = self.read(start, step)
            except OSError:
                continue
            f = np.frombuffer(raw, dtype='>f4')
            for align in range(4):
                usable = (len(f) - align * 4) // 16 * 16
                m = f[align * 4:align * 4 + usable].reshape(-1, 4, 4)
                with np.errstate(all='ignore'):
                    r = m[:, :3, :3]
                    ortho = np.abs(np.einsum('nij,nkj->nik', r, r) - np.eye(3)).max(axis=(1, 2)) < 1e-3
                    last = (np.abs(m[:, :3, 3]).max(axis=1) < 1e-6) & (np.abs(m[:, 3, 3] - 1) < 1e-6)
                    moved = np.abs(r - np.eye(3)).max(axis=(1, 2)) > 1e-3
                    sel = np.isfinite(m).all(axis=(1, 2)) & ortho & last & moved
                hits.extend(start + align * 16 + i * 64 for i in np.nonzero(sel)[0])
        hits.sort()
        runs, run = [], []
        for h in hits:
            if run and h - run[-1] == 64:
                run.append(h)
            else:
                if len(run) >= 32:
                    runs.append(run[0])
                run = [h]
        if len(run) >= 32:
            runs.append(run[0])
        return runs


def main():
    guest = Guest()
    if len(sys.argv) > 1 and sys.argv[1] == 'bones':
        for start in guest.bone_arrays():
            bones = guest.matrices(start, 32)
            arm = np.linalg.norm(bones[4][:3, 3] - bones[8][:3, 3])
            shin = np.linalg.norm(bones[3][:3, 3] - bones[7][:3, 3])
            print(f'{start:#x}: upper arm {arm:.3f}, shin {shin:.3f}')
            for i, m in enumerate(bones):
                print(f'  {i:2d}', np.round(m[:3, 3], 3))
    elif len(sys.argv) > 2 and sys.argv[1] == 'array':
        capture_array(guest, sys.argv[2], int(sys.argv[3]) if len(sys.argv) > 3 else 12)
    else:
        print(__doc__)


BONE_ARRAY = 0x833B5310


def capture_array(guest, out, samples):
    """The slots of the game's bone array that hold rotations, sampled `samples` times."""
    import time
    def slot(c):
        return np.frombuffer(guest.read(BONE_ARRAY + c * 0x800, 0x800), dtype='>f4').astype(np.float64).reshape(32, 4, 4)
    used = []
    for c in range(8):
        m = slot(c)
        r = m[:, :3, :3]
        if np.isfinite(m).all() and np.abs(np.einsum('nij,nkj->nik', r, r) - np.eye(3)).max() < 1e-3:
            used.append(c)
    print('slots', used)
    lines = []
    for sample in range(samples):
        for c in used:
            numbers = ' '.join(f'{v:.6f}' for m in slot(c) for v in m.T.reshape(-1))
            lines.append(f'{BONE_ARRAY + c * 0x800:x} {sample} {numbers}')
        time.sleep(0.4)
    open(out, 'w').write('\n'.join(lines) + '\n')
    print(len(lines), 'lines')


if __name__ == '__main__':
    main()
