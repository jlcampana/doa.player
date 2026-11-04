#!/usr/bin/env python3
"""Checks kind 250 vertex copies against a pickle from doa2u_capture_objects.py. Run from the repo root.

doa2u_copy_check.py CAPTURE.pkl

Prints each copy's links and how many samples differ. A read can fall between the game's
write of the source and its copy, so a few mismatches are expected."""
import os
import pickle
import struct
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import doa2u_crypt as c


def main():
    afs, n, toc = c.open_afs('datahdd2.afs')
    afs.seek(8 + 8 * n)
    name_offset, name_size = struct.unpack('<II', afs.read(8))
    afs.seek(name_offset)
    table = afs.read(name_size)
    names = {table[i * 48:i * 48 + 32].split(b'\0')[0].decode('latin1'): i for i in range(n)}

    def cat(stem):
        i = names[stem + '.cat']
        afs.seek(toc[2 * i])
        d = afs.read(toc[2 * i + 1])
        w = struct.unpack('<5I', d[:20])
        length = next((x for x in (w[4], w[3], w[2], w[1]) if x), 0)
        return c.dec(d, 0x14, min(length, len(d) - 0x14), w[0] ^ 0x20)

    capture = pickle.load(open(sys.argv[1], 'rb'))
    for fighter, stem in capture['fighters']:
        m = cat(stem)
        base = struct.unpack_from('<I', m, 8)[0]
        dynamic = {i: (v, s) for i, v, s, _ in capture['dyn'][fighter]}
        at = base
        while True:
            kind, offset = struct.unpack_from('<II', m, at)
            at += 8
            if kind & 0xFF == 0xFF:
                break
            if kind != 250:
                continue
            _, source, target, list_offset = struct.unpack_from('<HHHxxI', m, base + offset)
            links = []
            cursor = base + list_offset
            start = struct.unpack_from('<I', m, cursor)[0]
            while start != 0xFFFFFFFF:
                cursor += 4
                value = struct.unpack_from('<I', m, cursor)[0]
                while value != 0xFFFFFFFF:
                    links.append((start, value & ~1))
                    cursor += 4
                    if not value & 1:
                        break
                    value = struct.unpack_from('<I', m, cursor)[0]
                start = struct.unpack_from('<I', m, cursor)[0]
            print(stem, 'from', source, 'to', target, 'links', len(links))
            if source not in dynamic or target not in dynamic:
                continue
            total = differing = 0
            worst = 0.0
            for frame in capture['frames']:
                a, b = frame['v'][fighter][source], frame['v'][fighter][target]
                for x, y in links:
                    gap = max(abs(p - q) for p, q in zip(struct.unpack_from('<3f', a, x), struct.unpack_from('<3f', b, y)))
                    total += 1
                    differing += gap > 1e-5
                    worst = max(worst, gap)
            print(f'  {differing} of {total} samples differ, worst {worst * 1000:.2f} mm')


if __name__ == '__main__':
    main()
