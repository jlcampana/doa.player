#!/usr/bin/env python3
"""DOAX3's textures: GNF files inside a TMCL's texture block, listed through the TMC's TTGL.

gnf.py MODEL_TMC.tmc            lists each texture's T# fields
The T# is the GCN image resource (SQ_IMG_RSRC_WORD0..7). Decoding is in src/formats/doax3.rs
(`decode_texture`); its ignored test `dump_textures` writes a model's textures as PNG.
"""
import struct, sys

def u32(d, o): return struct.unpack_from('<I', d, o)[0]

def chunk(d, at, name):
    """A child of the chunk at `at` called `name`, or None."""
    count = u32(d, at + 0x14); table = u32(d, at + 0x20)
    for k in range(count):
        off = u32(d, at + table + 4 * k)
        if off and d[at + off:at + off + 8].rstrip(b'\0') == name:
            return at + off
    return None

def textures(tmc, tmcl):
    """(offset in tmcl, size) of each GNF."""
    lh = chunk(tmc, 0, b'LHeader')
    t1 = u32(tmc, lh + 0x20); n = u32(tmc, lh + 0x14)
    kinds = [u32(tmc, lh + 0x70 + 4 * k) for k in range(n)]
    block = u32(tmc, lh + t1 + 4 * kinds.index(0xC0000002))
    tg = chunk(tmc, 0, b'TTGM')
    tl = tg + u32(tmc, tg + 0x28)  # TTGH follows the header, TTGL at +0x28
    assert tmc[tl:tl + 4] == b'TTGL'
    count = u32(tmc, tl + 0x14); offs = u32(tmc, tl + 0x20); sizes = u32(tmc, tl + 0x24)
    return [(block + u32(tmc, tl + offs + 4 * k), u32(tmc, tl + sizes + 4 * k)) for k in range(count)]

def tsharp(g):
    w = struct.unpack_from('<8I', g, 0x10)
    return dict(
        fmt=(w[1] >> 20) & 0x3F, num=(w[1] >> 26) & 0xF,
        width=(w[2] & 0x3FFF) + 1, height=((w[2] >> 14) & 0x3FFF) + 1,
        sel=[(w[3] >> (3 * i)) & 7 for i in range(4)], base_level=(w[3] >> 12) & 0xF, last_level=(w[3] >> 16) & 0xF,
        tile=(w[3] >> 20) & 0x1F, type=(w[3] >> 28) & 0xF, pitch=((w[4] >> 13) & 0x3FFF) + 1, depth=(w[4] & 0x1FFF) + 1,
        words=w)

if __name__ == '__main__':
    path = sys.argv[1]
    tmc = open(path, 'rb').read()
    tmcl = open(path.replace('_TMC.tmc', '_TMCL.tmcl'), 'rb').read()
    for k, (at, size) in enumerate(textures(tmc, tmcl)):
        g = tmcl[at:at + size]
        assert g[:4] == b'GNF ', g[:4]
        t = tsharp(g)
        print(f"{k:2} at {at:#x} size {size:#x} fmt {t['fmt']} num {t['num']} {t['width']}x{t['height']} pitch {t['pitch']} "
              f"levels {t['base_level']}-{t['last_level']} tile {t['tile']} type {t['type']} sel {t['sel']} head {g[4:16].hex(' ')}")
