#!/usr/bin/env python3
"""DOAX3's motion files (MPM): lists motions and decodes their curves.

doax3_mpm.py FILE.mpm [MOTION]       summary, or one motion's 21 bones x 9 channels

Layout (read 2026-11-18): char_dat > char_dat with 6 children; child 0 is a list of u64 offsets
(relative to itself) to the motions, ended by 0; each motion is 21 bones x 9 channels of u64
offsets to curves (same base), then 0. Child 3 starts 00 00 01 00, motion count, bone count.
A curve starts 00 07 and is a byte stream decoded by the game at 0x5702A0 (see decode()).
"""
import struct, sys

def s8(b): return b - 256 if b > 127 else b

def decode(d, p, limit=100000):
    """Values per frame, as the game's stepper does: an integer accumulator times 1/16384.
    Commands: bytes 0-1 a count, byte 2 the opcode."""
    assert d[p] == 0 and d[p + 1] == 7, d[p:p + 2].hex()
    p += 2
    out = []
    acc = 0
    run = 0          # frames left in a delta run; >= 0x10000 for 16-bit deltas
    repeat = False   # the run's delta is the same every frame (the stream stays on it)
    first = True
    while len(out) < limit:
        if run:
            if run >= 0x10000:
                delta = d[p] | (s8(d[p + 1]) << 8)
                run = 0 if run == 0x10001 else run - 1
                if not repeat: p += 2
            else:
                delta = s8(d[p])
                if not repeat: p += 1
                run -= 1
            acc += delta
            out.append(acc)
            continue
        if repeat:
            p += 1
            repeat = False
        count = d[p] | d[p + 1] << 8
        op = d[p + 2]
        special = first and acc == 0  # the game's first step with nothing yet: deltas are values
        first = False
        if op == 0x80:
            acc = d[p] | d[p + 1] << 8 | s8(d[p + 3]) << 16
            out.append(acc)
            return out, True
        if not special and op == 0x7A:
            acc = d[p] | d[p + 1] << 8 | d[p + 3] << 16 | d[p + 4] << 24
            out.append(struct.unpack('<i', struct.pack('<I', acc))[0])
            return out, True
        if not special and op == 0x7F:
            acc += s8(d[p + 3]); p += 4; run = count - 1
        elif not special and op == 0x7E:
            acc += d[p + 3] | s8(d[p + 4]) << 8; p += 5; run = 0 if count == 1 else count + 0xFFFF
        elif not special and op == 0x7D:
            acc += s8(d[p + 3]); p += 3; repeat = True; run = count - 1
        elif not special and op == 0x7C:
            acc += d[p + 3] | s8(d[p + 4]) << 8; p += 4; repeat = True; run = 0 if count == 1 else count + 0xFFFF
        elif op == 0x7B:
            acc = struct.unpack('<i', bytes([d[p], d[p + 1], d[p + 3], d[p + 4]]))[0]; p += 5
        else:
            acc = d[p] | d[p + 1] << 8 | s8(d[p + 2]) << 16; p += 3
        out.append(acc)
    return out, False

def motions(d):
    base = 0x40 + 0x50
    q = lambda o: struct.unpack_from('<Q', d, base + o)[0]
    offs = []
    while q(8 * len(offs)):
        offs.append(q(8 * len(offs)))
    return base, [[q(o + 8 * k) for k in range(189)] for o in offs]

if __name__ == '__main__':
    d = open(sys.argv[1], 'rb').read()
    base, ms = motions(d)
    if len(sys.argv) < 3:
        for i, m in enumerate(ms):
            lens = [len(decode(d, base + c)[0]) for c in m]
            print(i, 'frames', min(lens), max(lens))
    else:
        m = ms[int(sys.argv[2])]
        for b in range(21):
            row = []
            for c in range(9):
                v, end = decode(d, base + m[b * 9 + c])
                f = [x / 16384 for x in v]
                row.append(f"{f[0]:+.3f}" + (f"..{min(f):+.2f}/{max(f):+.2f}" if max(f) != min(f) else ""))
            print(b, ' '.join(row))
