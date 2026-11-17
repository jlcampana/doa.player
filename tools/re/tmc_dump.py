#!/usr/bin/env python3
"""Walks DOAX3 Fortune's TMC containers (also TMCL and other Koei Tecmo chunk files).

  tmc_dump.py FILE [DEPTH]

A chunk (little-endian): name[8] ("TMC", "MdlGeo", ...), version u32, header size
u32, chunk size u32, child count u32, a u32, a u32, then at 0x20 the offset of the
child offset table (u32, relative to the chunk), and at 0x24 an optional second
table. Children are chunks too, until a chunk's table holds offsets to plain data.
Prints the tree with offsets, sizes and counts, to read the layout from.
"""
import struct
import sys


def name_at(d, at):
    raw = d[at:at + 8]
    if not raw[:1].isalpha():
        return None
    text = raw.split(b'\0')[0]
    if not text or not all(32 < c < 127 for c in text):
        return None
    return text.decode()


def walk(d, at, depth, max_depth, indent=''):
    name = name_at(d, at)
    version, header, size, count = struct.unpack_from('<IIII', d, at + 8)
    extra = struct.unpack_from('<II', d, at + 0x18)
    table, table2 = struct.unpack_from('<II', d, at + 0x20)
    print(f'{indent}{at:#08x} {str(name):<9} ver {version:#010x} head {header:#x} size {size:#x} count {count} extra {extra} table {table:#x} table2 {table2:#x}')
    if depth >= max_depth or count == 0 or table == 0 or count > 100000:
        return
    offsets = struct.unpack_from(f'<{count}I', d, at + table)
    children = 0
    for i, off in enumerate(offsets):
        if off == 0:
            continue
        child = at + off
        if child + 0x30 <= len(d) and name_at(d, child):
            walk(d, child, depth + 1, max_depth, indent + '  ')
            children += 1
        elif i < 4:
            print(f'{indent}  [{i}] data at {child:#08x}: {d[child:child + 16].hex(" ", 4)}')
    if children == 0 and count > 4:
        print(f'{indent}  ... {count} data entries')


def main():
    d = open(sys.argv[1], 'rb').read()
    walk(d, 0, 0, int(sys.argv[2]) if len(sys.argv) > 2 else 2)


if __name__ == '__main__':
    main()
