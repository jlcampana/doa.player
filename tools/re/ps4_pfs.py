#!/usr/bin/env python3
"""Reads a plain (decrypted, uncompressed) PS4 PFS image: the outer image of a PKG
(PkgTool pkg_extractouterpfs) or the inner one (ps4_pfsc.py output).

  ps4_pfs.py list IMAGE
  ps4_pfs.py extract IMAGE OUTDIR [GLOB]

Written because PkgTool v0.2 on .NET 6 or later leaves holes when it reads files
out of a PFS (single Read calls that may return short), and its pfs_extract and
pkg_extractinnerpfs output for DOAX3 Fortune had 2.1 GB of zeros.

Layout (little-endian): header at 0 with the magic 20130315 (0x1332A0B) at 8, mode
u16 at 0x1C (2 = 64-bit inodes, 1 = signed), block size u32 at 0x20, inode count
u64 at 0x30, inode blocks u64 at 0x40. Inodes start at block 1, packed per block.
A 32-bit inode is 0xA8 bytes: mode u16, links u16, flags u32, size u64, stored
size u64, times, then at 0x60 the block count u32, 12 direct blocks and 5
indirect blocks (u32). A file whose second direct block is 0xFFFFFFFF is stored
contiguously from the first. Signed images (mode bit 1, the outer image of a PKG)
put a 32-byte hash before each block number: pointers are 36 bytes, the inode 0x2C8,
and an indirect block holds block size / 36 of them. Inode 0 is the super root; its tree holds `uroot`.
A directory's blocks hold entries: inode u32, type u32 (2 file, 3 directory, 4 and
5 the dot entries), name length u32, entry size u32, then the name.
"""
import fnmatch
import os
import struct
import sys

NONE = 0xFFFFFFFF


class Pfs:
    def __init__(self, path):
        self.f = open(path, 'rb')
        h = self.f.read(0x50)
        if struct.unpack_from('<Q', h, 8)[0] != 0x1332A0B:
            raise ValueError('not a PFS image')
        self.mode = struct.unpack_from('<H', h, 0x1C)[0]
        self.block = struct.unpack_from('<I', h, 0x20)[0]
        if self.mode & 2:
            raise ValueError(f'mode {self.mode}: 64-bit inodes are not handled')
        self.signed = bool(self.mode & 1)
        self.pointer = 36 if self.signed else 4
        self.inode_size = 0x64 + 17 * self.pointer

    def inode(self, number):
        per = self.block // self.inode_size
        self.f.seek(self.block * (1 + number // per) + (number % per) * self.inode_size)
        raw = self.f.read(self.inode_size)
        mode, links, flags, size, stored = struct.unpack_from('<HHIQQ', raw, 0)
        count = struct.unpack_from('<I', raw, 0x60)[0]
        pointers = [struct.unpack_from('<I', raw, 0x64 + i * self.pointer + self.pointer - 4)[0] for i in range(17)]
        direct, indirect = tuple(pointers[:12]), tuple(pointers[12:])
        return {'mode': mode, 'flags': flags, 'size': size, 'stored': stored, 'count': count, 'direct': direct, 'indirect': indirect}

    def blocks(self, node):
        count = node['count']
        if count == 0:
            return []
        direct = node['direct']
        if len(direct) > 1 and direct[1] == NONE:
            return list(range(direct[0], direct[0] + count))
        out = [b for b in direct if b != NONE][:count]
        # Indirect pointer k is k + 1 levels deep.
        for level, root in enumerate(node['indirect']):
            if len(out) >= count:
                break
            if root in (0, NONE):
                continue
            self._collect(root, level, out, count)
        if len(out) < count:
            raise ValueError(f'{count - len(out)} blocks missing from the pointer blocks')
        return out

    def _pointer_block(self, number):
        self.f.seek(number * self.block)
        raw = self.f.read(self.block)
        return [struct.unpack_from('<I', raw, i + self.pointer - 4)[0] for i in range(0, self.block - self.pointer + 1, self.pointer)]

    def _collect(self, number, depth, out, count):
        for b in self._pointer_block(number):
            if len(out) >= count:
                return
            if b in (0, NONE):
                continue
            if depth == 0:
                out.append(b)
            else:
                self._collect(b, depth - 1, out, count)

    def read_into(self, node, out):
        """Writes a file's bytes as stored to `out`, a block at a time. The size at 0x08 is
        what is stored; for a compressed file (flag 1, such as the outer image's
        pfs_image.dat, a PFSC) the one at 0x10 is the unpacked size."""
        left = node['size']
        for b in self.blocks(node):
            if left <= 0:
                break
            self.f.seek(b * self.block)
            data = self.f.read(min(self.block, left))
            if len(data) != min(self.block, left):
                raise IOError(f'short read at block {b}')
            out.write(data)
            left -= len(data)

    def entries(self, number):
        node = self.inode(number)
        data = bytearray()
        for b in self.blocks(node):
            self.f.seek(b * self.block)
            data += self.f.read(self.block)
        at = 0
        while at + 16 <= len(data):
            ino, kind, length, size = struct.unpack_from('<IIII', data, at)
            if size == 0:
                break
            name = data[at + 16:at + 16 + length].decode('utf-8', 'replace')
            if kind in (2, 3):
                yield ino, kind, name
            at += size

    def walk(self, number=0, prefix=''):
        for ino, kind, name in self.entries(number):
            path = f'{prefix}/{name}' if prefix else name
            if kind == 3:
                yield from self.walk(ino, path)
            else:
                yield ino, path


def main():
    if len(sys.argv) < 3:
        sys.exit(__doc__)
    cmd, pfs = sys.argv[1], Pfs(sys.argv[2])
    if cmd == 'list':
        for ino, path in pfs.walk():
            node = pfs.inode(ino)
            print(f"{node['size']:12d} {node['stored']:12d} {node['flags']:#06x} {path}")
    elif cmd == 'extract':
        out_dir = sys.argv[3]
        pattern = sys.argv[4] if len(sys.argv) > 4 else '*'
        count = 0
        for ino, path in pfs.walk():
            if not fnmatch.fnmatch(path, pattern):
                continue
            dest = os.path.join(out_dir, path)
            os.makedirs(os.path.dirname(dest), exist_ok=True)
            with open(dest, 'wb') as w:
                pfs.read_into(pfs.inode(ino), w)
            count += 1
        print(f'{count} files')
    else:
        sys.exit(__doc__)


if __name__ == '__main__':
    main()
