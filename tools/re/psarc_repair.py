#!/usr/bin/env python3
"""Rebuilds the lost first block of a PSARC whose table of contents was damaged.

  psarc_repair.py DAMAGED.psarc PREFIX.psarc [--write]

DOAX3 Fortune's PKG lost the first 64 KiB block of 01/Archive.psarc (PFS block
61049, the last in the damaged range; see ps4_pfsc.py). The rest of the archive is
intact, including the block size table (from byte 124262) and entries 2183 on.
PREFIX is an older, otherwise broken extraction whose first block still holds
the header and entries 0 to 319 before its zeros (PkgTool's partial inflate).

Entries in between are inferred: data is stored in entry order, each entry
starting at the block after the previous one's last, at the next byte offset.
An entry ends at its first block that unpacks to less than a full block. A block
size of 0 is a full raw block; a block starting with a zlib header unpacks to
its inflated size; any other block is raw, of its listed size. The inferred run
must land exactly on the first intact entry, or nothing is written. Inferred
entries get a zero MD5 (names are taken from the manifest, by order).

Without --write it only checks; with it, block 0 of DAMAGED is overwritten with
the rebuilt header and entries (that block is all zeros in the damaged file).
"""
import re
import struct
import sys
import zlib

ENTRY = 30


def unpacked_size(f, offset, stored, block):
    if stored == 0:
        return block, block
    f.seek(offset)
    data = f.read(stored)
    if data[:1] == b'\x78':
        try:
            return len(zlib.decompress(data)), stored
        except zlib.error:
            pass
    return stored, stored


def main():
    args = [a for a in sys.argv[1:] if a != '--write']
    write = '--write' in sys.argv
    if len(args) != 2:
        sys.exit(__doc__)
    damaged, prefix_path = args
    f = open(damaged, 'r+b' if write else 'rb')
    prefix = open(prefix_path, 'rb').read(0x10000)
    if prefix[:4] != b'PSAR':
        sys.exit('the prefix file has no PSARC header')
    toc_len, entry_size, count, block, _flags = struct.unpack('>IIIII', prefix[12:32])
    assert entry_size == ENTRY
    zeros = re.search(rb'\x00{64,}', prefix[32:])
    good_prefix = (zeros.start() if zeros else 0x10000 - 32) // ENTRY
    f.seek(0)
    toc = bytearray(f.read(toc_len))
    lost_end = 0x10000  # the damaged block
    # Entries whose position fields (bytes 16 to 30) lie past the lost block are intact.
    first_intact = next(i for i in range(count) if 32 + i * ENTRY + 16 >= lost_end)
    width = 2 if block <= 0x10000 else 3
    table = toc[32 + count * ENTRY:]
    sizes = [int.from_bytes(table[j:j + width], 'big') for j in range(0, len(table) - width + 1, width)]

    def fields(raw):
        return struct.unpack('>I', raw[16:20])[0], int.from_bytes(raw[20:25], 'big'), int.from_bytes(raw[25:30], 'big')

    entries = [bytes(prefix[32 + i * ENTRY:32 + (i + 1) * ENTRY]) for i in range(good_prefix)]
    first, length, offset = fields(entries[-1])
    # Walk on from the last good entry.
    b = first + (length + block - 1) // block
    position = offset
    for k in range(first, b):
        position += sizes[k] or block
    print(f'header and entries 0..{good_prefix - 1} from the prefix; inferring {good_prefix}..{first_intact - 1}')
    for index in range(good_prefix, first_intact):
        start, start_offset, total = b, position, 0
        while True:
            size, stored = unpacked_size(f, position, sizes[b], block)
            total += size
            position += stored
            b += 1
            if size < block:
                break
        entries.append(bytes(16) + struct.pack('>I', start) + total.to_bytes(5, 'big') + start_offset.to_bytes(5, 'big'))
        if index % 200 == 0:
            print(f'  entry {index}: block {start}, {total} bytes', file=sys.stderr)
    target = fields(toc[32 + first_intact * ENTRY:32 + (first_intact + 1) * ENTRY])
    print(f'next block {b} offset {position}; intact entry {first_intact} starts at block {target[0]} offset {target[2]}')
    if (b, position) != (target[0], target[2]):
        sys.exit('the inferred entries do not meet the intact ones: nothing written')
    rebuilt = bytearray(prefix[:32]) + b''.join(entries)
    head = bytes(rebuilt) + bytes(toc[32 + first_intact * ENTRY:lost_end])
    head = head[:lost_end]
    # The straddling entry keeps its intact tail from the damaged file.
    assert len(head) == lost_end
    if write:
        f.seek(0)
        f.write(head)
        print('block 0 written')
    else:
        print('check passed; run with --write to patch block 0')


if __name__ == '__main__':
    main()
