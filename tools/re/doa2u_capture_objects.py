#!/usr/bin/env python3
"""Records the objects the running DOA2U game rewrites, read-only. Run from the repo root.

doa2u_capture_objects.py OUT.pkl STEM0,STEM1 FRAMES

STEM0 and STEM1 are candidate costume prefixes, such as hel,tin. The pickle holds the
fighters' costumes, the rewritten objects, their vertices from the file ('rest') and, per
read, the matrices and those objects' vertices. Reads are not tied to the game's frames."""
import os
import pickle
import struct
import sys
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import doa2u_crypt as c
from doa2u_live import Guest


def u32(data, offset):
    return struct.unpack_from('<I', data, offset)[0]


def main():
    out, prefixes, frames = sys.argv[1], sys.argv[2].split(','), int(sys.argv[3])
    g = Guest()
    afs, n, toc = c.open_afs('datahdd2.afs')
    afs.seek(8 + 8 * n)
    name_offset, name_size = struct.unpack('<II', afs.read(8))
    afs.seek(name_offset)
    table = afs.read(name_size)
    names = {table[i * 48:i * 48 + 32].split(b'\0')[0].decode('latin1'): i for i in range(n)}

    def load(name):
        i = names[name]
        afs.seek(toc[2 * i])
        return afs.read(toc[2 * i + 1])

    def cat(stem):
        d = load(stem + '.cat')
        w = struct.unpack('<5I', d[:20])
        length = next((x for x in (w[4], w[3], w[2], w[1]) if x), 0)
        return c.dec(d, 0x14, min(length, len(d) - 0x14), w[0] ^ 0x20)

    def xpr(stem):
        d = load(stem + '.xpr')
        w = struct.unpack('<2I', d[:8])
        return c.dec(d, 12, w[1] - 12, w[0] ^ 0x30525058)

    cats = {}
    for name in names:
        if name[:3] in prefixes and name.endswith('.cat'):
            try:
                m = cat(name[:-4])
            except (ValueError, struct.error):
                continue
            base = u32(m, 4)
            record = base
            draw_list = None
            while record + 8 <= len(m):
                value, kind = struct.unpack_from('<II', m, record)
                if kind == 0:
                    break
                if kind == 1 and draw_list is None:
                    draw_list = value + base
                record += 8
            if draw_list is not None:
                cats.setdefault(name[:-4], (m, draw_list))

    fighters = []
    for f in range(2):
        words = struct.unpack('<17I', g.r(0x850738 + 0x44 * f, 0x44))
        scores = []
        for stem, (m, draw_list) in cats.items():
            live = g.r((words[2] & 0x0FFFFFFF) - draw_list, len(m))
            if len(live) == len(m):
                scores.append((sum(a == b for a, b in zip(live[::7], m[::7])) / len(m[::7]), stem))
        scores.sort(reverse=True)
        print('fighter', f, 'id', g.r(0x87AE31 + 0x94 * f, 1)[0], scores[:2])
        fighters.append((f, scores[0][1], words[0]))

    rest, rewritten = {}, {}
    for f, stem, objects in fighters:
        x = xpr(stem)
        info = []
        for i in range(u32(x, 0x24)):
            record = u32(x, 0x28 + i * 4)
            count, offset = struct.unpack_from('<II', x, record + 0x20)
            stride = 32 + u32(x, record + 4) * 4
            pointer = u32(g.r((objects + i * 4) & 0x0FFFFFFF, 4), 0)
            buffer = u32(g.r((pointer + 0x24) & 0x0FFFFFFF, 4), 0)
            data = u32(g.r((buffer + 4) & 0x0FFFFFFF, 4), 0) & 0x0FFFFFFF
            info.append((i, count, stride, data, x[offset:offset + count * stride]))
        samples = []
        for _ in range(6):
            samples.append({i: g.r(a, v * s) for i, v, s, a, _ in info})
            time.sleep(0.1)
        rest[f] = {i: file for i, _, _, _, file in info}
        rewritten[f] = [(i, v, s, a) for i, v, s, a, file in info
                        if any(sample[i] != file for sample in samples)]
        print(stem, 'objects', len(info), 'rewritten', [(i, v) for i, v, _, _ in rewritten[f]])

    start = time.time()
    reads = []
    for _ in range(frames):
        reads.append({'t': time.time() - start, 'm': g.r(0x875C20, 0x1000),
                      'v': {f: {i: g.r(a, v * s) for i, v, s, a in l} for f, l in rewritten.items()}})
        time.sleep(0.012)
    with open(out, 'wb') as o:
        pickle.dump({'fighters': [(f, stem) for f, stem, _ in fighters], 'dyn': rewritten,
                     'frames': reads, 'rest': rest}, o)
    print('reads', len(reads), 'distinct', len({r['m'] for r in reads}))


if __name__ == '__main__':
    main()
