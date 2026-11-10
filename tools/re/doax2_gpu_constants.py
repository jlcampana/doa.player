#!/usr/bin/env python3
"""Read-only: the vertex shader constants DOAX2 writes into its GPU command buffers.

The Xbox 360 D3D layer writes shader constants as PM4 type-0 packets to registers 0x4000 + 4 * c
(vertex shader float constants). The command buffers sit in physical memory, seen at guest
0xA0000000 to 0xC0000000. The ring keeps about 16 frames before it is overwritten.

  doax2_gpu_constants.py lights        distinct light sets (c68-c79), hemisphere (c4-c6),
                                       specular (c67), most common first
  doax2_gpu_constants.py chest [secs]  per draw of a body whose chest base buffer is found:
                                       the bound shape streams and PosWeight (c49)

`chest` needs the chest streams' heads, written by the ignored test `chest_stream_heads`
(target/captures/doax2_chest_streams.txt). The skin shader blends
pos = x * base + y * stream1 + z * stream2 + w * stream3 with c49 = (x, y, z, w), where stream 1
is shape 1 or 4 (sideways), stream 2 shape 2 or 3 (forward, back), stream 3 shape 5 or 6 (up,
down). Vertex fetch constant 95 - n is stream n.
"""
import collections
import sys
import time

import numpy as np

HOST_BASE = 0x200000000
COMMANDS = (0xA0000000, 0xC0000000)
sys.path.insert(0, __file__.rsplit('/', 1)[0])
from doax2_live import pid  # noqa: E402


class Guest:
    def __init__(self):
        self.pid = pid()
        self.mem = open(f'/proc/{self.pid}/mem', 'rb', buffering=0)

    def read(self, address, size):
        self.mem.seek(HOST_BASE + address)
        return self.mem.read(size)

    def regions(self, low, high):
        for line in open(f'/proc/{self.pid}/maps'):
            span, perm = line.split()[:2]
            s, e = (int(x, 16) - HOST_BASE for x in span.split('-'))
            if low <= s < high and 'rw' in perm:
                yield s, e

    def chunks(self, low, high, size=0x4000000):
        for s, e in self.regions(low, high):
            for a in range(s, e, size):
                n = min(size, e - a)
                try:
                    yield a, np.frombuffer(self.read(a, n)[: n // 4 * 4], '>u4')
                except OSError:
                    continue


def constant_packets(words):
    """Indices, first registers and register counts of type-0 writes to float constants."""
    header = (words >> 30) == 0
    base = words & 0x7FFF
    count = ((words >> 16) & 0x3FFF) + 1
    hit = header & (base >= 0x4000) & (base < 0x4800) & ((words & 0x8000) == 0) & (count % 4 == 0) & (count <= 256) & (base % 4 == 0)
    for i in np.nonzero(hit)[0]:
        yield i, (int(base[i]) - 0x4000) // 4, int(count[i]) // 4


def block(words, i, first, register, size):
    at = i + 1 + (register - first) * 4
    return np.array(words[at:at + 4 * size], dtype='>u4').view('>f4').astype(np.float64)


def lights(guest):
    tables = {'c68-c79 ambient P1, P2, -, light directions c71-74, colours c75-78': (68, 12),
              'c4-c6 HemisphereMat': (4, 3), 'c67 SpeqObjCol': (67, 1)}
    found = {name: collections.Counter() for name in tables}
    for _, words in guest.chunks(*COMMANDS):
        for i, first, count in constant_packets(words):
            for name, (register, size) in tables.items():
                if first <= register and register + size <= first + count:
                    values = block(words, i, first, register, size)
                    if np.isfinite(values).all() and np.abs(values).max() < 100:
                        found[name][tuple(np.round(values, 3))] += 1
    for name, counter in found.items():
        print(f'== {name}: {len(counter)} distinct, {sum(counter.values())} writes')
        for values, n in counter.most_common(6):
            print(n, np.array(values).reshape(-1, 4).tolist())


def chest(guest, seconds):
    heads = [line.split() for line in open('target/captures/doax2_chest_streams.txt')]
    # Where each body's chest streams are, by their first bytes.
    streams = {}
    for a, words in guest.chunks(*COMMANDS):
        raw = words.tobytes()
        for name, obj, stream, _, head in heads:
            if obj != '0':
                continue
            at = raw.find(bytes.fromhex(head))
            if at >= 0:
                streams[(a + at) - 0xA0000000] = (name, int(stream))
    bodies = {name for name, s in streams.values() if s == 0}
    print('chest buffers found for', sorted(bodies))
    rows = []
    start = time.time()
    while time.time() - start < seconds:
        for a, words in guest.chunks(*COMMANDS):
            packets = [(i, f, c) for i, f, c in constant_packets(words) if f <= 49 < f + c]
            starts = np.array([p[0] for p in packets])
            for base, (name, stream) in streams.items():
                if stream != 0:
                    continue
                for i in np.nonzero(words == base | 3)[0]:
                    bound = [streams.get(int(words[i - 2 * k]) & ~3, (None, -1))[1] for k in range(1, 4)]
                    before = np.searchsorted(starts, i) - 1
                    if before < 0 or i - starts[before] > 4000:
                        continue
                    j, first, _ = packets[before]
                    weights = block(words, j, first, 49, 1)
                    if np.isfinite(weights).all() and abs(weights.sum() - 1) < 1e-3:
                        rows.append((name, a + 4 * int(i), tuple(bound), tuple(np.round(weights, 4))))
    for row in sorted(set(rows), key=lambda r: r[1]):
        print(*row)


def main():
    guest = Guest()
    if len(sys.argv) > 1 and sys.argv[1] == 'lights':
        lights(guest)
    elif len(sys.argv) > 1 and sys.argv[1] == 'chest':
        chest(guest, float(sys.argv[2]) if len(sys.argv) > 2 else 2.0)
    else:
        print(__doc__)


if __name__ == '__main__':
    main()
