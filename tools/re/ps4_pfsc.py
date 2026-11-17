#!/usr/bin/env python3
"""Decompresses a PS4 PFSC image (the compressed inner PFS of a PKG) to a plain PFS image.

  ps4_pfsc.py IN.pfsc OUT.pfs [--bad BAD.txt]

With --bad, blocks that do not decompress to a full block are written as zeros and
their numbers and plain offsets listed in BAD.txt, instead of stopping. DOAX3
Fortune's PKG has 22 MiB of zeros inside its image (PkgTool pkg_validate: "PFS
Image Digest" invalid), which damages about 1400 compressed blocks.

Get IN.pfsc with LibOrbisPkg:
  PkgTool.Core pkg_extractinnerpfs --compressed GAME.pkg IN.pfsc
then list the files with:
  PkgTool.Core pfs_extract OUT.pfs OUTDIR

Why: PkgTool v0.2 run on .NET 6 or later (DOTNET_ROLL_FORWARD=Major) decompresses
blocks with a single DeflateStream.Read, which since .NET 6 may return less than a
block. Its pkg_extract then leaves the tail of every compressed 64 KiB block as
zeros (seen in DOAX3 Fortune: Archive.psarc's file table and manifest). Blocks
stored raw come out right, which hides the damage.

Header (little-endian): "PFSC", u32, u32, block size u32 at 0x0C, block size u64
at 0x10, offset of the block table u64 at 0x18, data start u64 at 0x20, plain size
u64 at 0x28. The table holds one u64 offset per block plus one; a block whose
stored size equals the block size is raw, a smaller one is zlib.
"""
import struct
import sys
import zlib


def main():
    args = sys.argv[1:]
    bad_path = None
    if '--bad' in args:
        i = args.index('--bad')
        bad_path = args[i + 1]
        del args[i:i + 2]
    if len(args) != 2:
        sys.exit(__doc__)
    sys.argv[1:] = args
    bad = []
    src = open(sys.argv[1], 'rb')
    h = src.read(0x30)
    if h[:4] != b'PFSC':
        sys.exit('not a PFSC image')
    block = struct.unpack_from('<I', h, 0x0C)[0]
    table, _data, plain = struct.unpack_from('<QQQ', h, 0x18)
    count = (plain + block - 1) // block
    src.seek(table)
    offsets = struct.unpack(f'<{count + 1}Q', src.read(8 * (count + 1)))
    zero = bytes(block)
    raw = packed = empty = 0
    with open(sys.argv[2], 'wb') as out:
        for i in range(count):
            size = offsets[i + 1] - offsets[i]
            want = min(block, plain - i * block)
            if size == 0:
                out.write(zero[:want]); empty += 1
                continue
            src.seek(offsets[i])
            chunk = src.read(size)
            if size >= block:
                data = chunk[:want]; raw += 1
            else:
                try:
                    stream = zlib.decompressobj()
                    data = stream.decompress(chunk)
                    if not stream.eof:
                        raise zlib.error('stream cut short')
                except zlib.error:
                    data = b''
                packed += 1
            if len(data) != want:
                if bad_path is None:
                    sys.exit(f'block {i}: {len(data)} bytes, expected {want}')
                bad.append(i)
                data = zero[:want]
            out.write(data)
            if i % 20000 == 0:
                print(f'{i}/{count}', file=sys.stderr)
    print(f'{count} blocks: {packed} zlib, {raw} raw, {empty} empty; {plain} bytes; {len(bad)} damaged')
    if bad_path:
        with open(bad_path, 'w') as w:
            w.writelines(f'{i} {i * block:#x}\n' for i in bad)


if __name__ == '__main__':
    main()
