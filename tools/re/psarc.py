#!/usr/bin/env python3
"""PSARC (PS3/PS4 archive) reader, for DOAX3 Fortune's 00/ and 01/Archive.psarc.

  psarc.py list ARCHIVE [GLOB]           paths and sizes
  psarc.py extract ARCHIVE OUTDIR GLOB   writes matching files under OUTDIR

Layout (big-endian): "PSAR", version u16 u16, compression ("zlib"/"lzma"),
TOC length u32, entry size u32 (30), entry count u32, block size u32, flags u32.
An entry is MD5[16], first block u32, length u40, offset u40. After the entries
comes one size per block (u16 for 64 KiB blocks); 0 means a full block, and a
block whose size equals its stored length and does not start with a zlib
header is stored raw. Entry 0 is the manifest: the other entries' paths, one
per line, in order.
"""
import fnmatch
import os
import struct
import sys
import zlib


class Psarc:
    def __init__(self, path):
        self.f = open(path, 'rb')
        h = self.f.read(32)
        if h[:4] != b'PSAR':
            raise ValueError('not a PSARC')
        self.compression = h[8:12]
        toc_len, entry_size, count, self.block, self.flags = struct.unpack('>IIIII', h[12:32])
        toc = self.f.read(toc_len - 32)
        self.entries = []
        for i in range(count):
            e = toc[i * entry_size:(i + 1) * entry_size]
            first = struct.unpack('>I', e[16:20])[0]
            length = int.from_bytes(e[20:25], 'big')
            offset = int.from_bytes(e[25:30], 'big')
            self.entries.append((first, length, offset))
        width = 2 if self.block <= 0x10000 else (3 if self.block <= 0x1000000 else 4)
        sizes = toc[count * entry_size:]
        self.sizes = [int.from_bytes(sizes[j:j + width], 'big') for j in range(0, len(sizes) - width + 1, width)]
        manifest = self.read(0).decode('utf-8', 'replace')
        self.names = [''] + manifest.split('\n')

    def read(self, index):
        first, length, offset = self.entries[index]
        out = bytearray()
        self.f.seek(offset)
        b = first
        while len(out) < length:
            size = self.sizes[b] or self.block
            chunk = self.f.read(size)
            want = min(self.block, length - len(out))
            if size == want and not (chunk[:1] == b'\x78'):
                out += chunk
            else:
                try:
                    out += zlib.decompress(chunk)
                except zlib.error:
                    out += chunk
            b += 1
        return bytes(out[:length])

    def files(self, pattern='*'):
        for i in range(1, len(self.entries)):
            name = self.names[i] if i < len(self.names) else f'entry_{i}'
            if fnmatch.fnmatch(name.lower(), pattern.lower()):
                yield i, name, self.entries[i][1]


def main():
    if len(sys.argv) < 3:
        sys.exit(__doc__)
    cmd, arc = sys.argv[1], Psarc(sys.argv[2])
    if cmd == 'list':
        for _, name, size in arc.files(sys.argv[3] if len(sys.argv) > 3 else '*'):
            print(f'{size:12d} {name}')
    elif cmd == 'extract':
        out, pattern = sys.argv[3], sys.argv[4]
        n = 0
        for i, name, _ in arc.files(pattern):
            dest = os.path.join(out, name.lstrip('/'))
            os.makedirs(os.path.dirname(dest), exist_ok=True)
            with open(dest, 'wb') as w:
                w.write(arc.read(i))
            n += 1
        print(f'{n} files')
    else:
        sys.exit(__doc__)


if __name__ == '__main__':
    main()
