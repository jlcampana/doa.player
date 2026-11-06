#!/usr/bin/env python3
"""Records the hair cloth particles of a running DOAX2 Recomp. Read-only: never writes to the game.

    doax2_hair_capture.py <head> <seconds> <out.npz>     e.g. vleAA 5 target/captures/vle.npz

Finds the head CAT's loaded `MULTI_CLOTH` block by the first cloth's name (`<head>long_l`, or
the first `CLOTH_DATA` name given with --name), follows each `CLOTH_DATA`'s pointer to its
particle states (0x80 bytes each: position at +0, a step at +0x10, the normal at +0x20, the
previous position at +0x30) and saves every distinct quiet snapshot with its time.

Pointers into the 0xE0000000 range of guest memory are 0x1000 below where the data shows in
the host's view of it, so `read` adds 0x1000 to them.
"""
import argparse
import os
import struct
import sys
import time

import numpy as np

sys.path.insert(0, os.path.dirname(__file__))
from doax2_live import Guest, HOST_BASE  # noqa: E402

PARTICLE = 0x80


class Pointers(Guest):
    def at(self, pointer, size):
        """Reads at a guest pointer from the 0xE0000000 range."""
        return self.read(pointer + 0x1000, size)


def find(guest, name, lo=0xA0000000, hi=0xC0000000, step=0x1000000):
    """Guest addresses (in the 0xE0000000 view) of MULTI_CLOTH blocks whose first cloth is `name`."""
    needle = name.encode() + b'\0'
    found = []
    for start in range(lo, hi, step):
        try:
            raw = guest.read(start, step + 64)
        except OSError:
            continue
        i = raw.find(needle)
        while i >= 0:
            # The first cloth's name is at MULTI_CLOTH + 0xD0.
            block = start + i - 0xD0
            if raw[i - 0xD0:i - 0xD0 + 11] == b'MULTI_CLOTH':
                found.append(block - 0xA0000000 + 0xE0000000 - 0x1000)
            i = raw.find(needle, i + 1)
    return found


def cloths(guest, block):
    """Per cloth: name, particle count and the guest pointer to its particle states."""
    words = struct.unpack('>24I', guest.at(block, 0x60))
    count = words[6]
    out = []
    for k in range(count):
        data = words[16 + k]
        header = struct.unpack('>16I', guest.at(data, 0x40))
        name = guest.at(data + 0x70, 16).split(b'\0')[0].decode()
        grid = header[9]
        particles = struct.unpack('>I', guest.at(grid + 0x24, 4))[0]
        states = header[10]
        out.append((name, particles, states))
    return out


def main():
    p = argparse.ArgumentParser()
    p.add_argument('head')
    p.add_argument('seconds', type=float)
    p.add_argument('out')
    p.add_argument('--name', help="the first cloth's name, if not <head>long_l")
    a = p.parse_args()
    guest = Pointers()
    blocks = find(guest, a.name or f'{a.head}long_l')
    if not blocks:
        sys.exit('cloth not loaded')
    block = blocks[0]
    parts = cloths(guest, block)
    for name, n, states in parts:
        print(f'{name}: {n} particles, states at {states:#x}')
    total = sum(n for _, n, _ in parts)
    frames, times = [], []
    last = None
    t0 = time.time()
    while time.time() - t0 < a.seconds:
        raw = b''.join(guest.at(states, n * PARTICLE) for _, n, states in parts)
        # Keep only quiet states: a second read the same, so no frame is caught half written.
        again = b''.join(guest.at(states, n * PARTICLE) for _, n, states in parts)
        if raw == again and raw != last:
            frames.append(np.frombuffer(raw, dtype='>f4').reshape(total, 32).astype(np.float32))
            times.append(time.time() - t0)
            last = raw
        if len(frames) > 20000:
            break
    np.savez_compressed(a.out, frames=np.array(frames), times=np.array(times),
                        names=np.array([n for n, _, _ in parts]),
                        counts=np.array([c for _, c, _ in parts]))
    print(f'{len(frames)} snapshots in {a.seconds} s, {total} particles -> {a.out}')


if __name__ == '__main__':
    main()
