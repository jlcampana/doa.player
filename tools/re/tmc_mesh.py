#!/usr/bin/env python3
"""Reads the geometry of a DOAX3 Fortune TMC + TMCL pair, for checking the layout.

  tmc_mesh.py MODEL_TMC.tmc [--obj OUT.obj]

TMC chunks (see tmc_dump.py): MdlGeo holds one ObjGeo per object, each with its
GeoDecl (vertex layouts) and its sub-meshes. LHeader lists the TMCL's blocks by
type (0xC0000002 textures, 3 vertices, 4 indices) with offsets and sizes;
VtxLay and IdxLay list the buffers inside the vertex and index blocks.

GeoDecl entry (from the table): +4 offset of its element list (relative to the
entry), +0x10 index count, +0x14 vertex count. The element list: vertex buffer
u32, stride u32, element count u32, then from +0x18 D3D9-style elements of 8 bytes: stream u16,
offset u16, type u8, method u8, usage u8, usage index u8. Types seen: 3 four
floats, 5 four bytes, 0xB four half floats. Usages: 0 position, 1 weights, 2
bone indices, 3 normal, 5 texture coordinates, 6 tangent.
"""
import struct
import sys

LUMP_VERTICES, LUMP_INDICES = 0xC0000003, 0xC0000004


def chunk(d, at):
    name = d[at:at + 8].split(b'\0')[0].decode()
    _ver, _head, size, count = struct.unpack_from('<IIII', d, at + 8)
    table = struct.unpack_from('<I', d, at + 0x20)[0]
    offsets = struct.unpack_from(f'<{count}I', d, at + table) if count and table else ()
    return name, size, count, [at + o if o else 0 for o in offsets]


def children(d, at):
    return {chunk(d, c)[0]: c for c in chunk(d, at)[3] if c}


def lumps(d, at):
    count = struct.unpack_from('<I', d, at + 0x14)[0]
    table, table2 = struct.unpack_from('<II', d, at + 0x20)
    offsets = struct.unpack_from(f'<{count}I', d, at + table)
    sizes = struct.unpack_from(f'<{count}I', d, at + table2)
    kinds = struct.unpack_from(f'<{count}I', d, at + 0x70)
    return {k: (o, s) for k, o, s in zip(kinds, offsets, sizes)}


def buffers(d, at):
    count = struct.unpack_from('<I', d, at + 0x14)[0]
    table, table2 = struct.unpack_from('<II', d, at + 0x20)
    return list(zip(struct.unpack_from(f'<{count}I', d, at + table), struct.unpack_from(f'<{count}I', d, at + table2)))


def decls(d, geo):
    """The GeoDecl inside an ObjGeo: list of (stride, vertex buffer, index buffer, index count, vertex count, elements)."""
    decl = next((c for c in range(geo + 0x30, geo + chunk(d, geo)[1], 0x10) if d[c:c + 8] == b'GeoDecl\0'), None)
    if decl is None:
        return []
    _, _, count, entries = chunk(d, decl)
    out = []
    for e in entries:
        index_count, vertex_count = struct.unpack_from('<II', d, e + 0x10)
        el = e + struct.unpack_from('<I', d, e + 4)[0]
        vbuf, stride, n = struct.unpack_from('<III', d, el)
        elements = [struct.unpack_from('<HHBBBB', d, el + 0x18 + 8 * i) for i in range(n)]
        # The index buffer is the declaration's place in file order.
        out.append((stride, vbuf, len(out), index_count, vertex_count, elements))
    return out


def meshes(tmc, tmcl):
    top = children(tmc, 0)
    lump = lumps(tmc, top['LHeader'])
    vbase, ibase = lump[LUMP_VERTICES][0], lump[LUMP_INDICES][0]
    vbufs, ibufs = buffers(tmc, top['VtxLay']), buffers(tmc, top['IdxLay'])
    result = []
    for obj, geo in enumerate(c for c in chunk(tmc, top['MdlGeo'])[3] if c):
        for stride, vb, ib, index_count, vertex_count, elements in decls(tmc, geo):
            pos = next(e for e in elements if e[4] == 0)
            start = vbase + vbufs[vb][0]
            verts = [struct.unpack_from('<3f', tmcl, start + i * stride + pos[1]) for i in range(vertex_count)]
            idx = struct.unpack_from(f'<{index_count}H', tmcl, ibase + ibufs[ib][0])
            result.append((obj, vb, verts, idx, elements, stride))
    return result


def main():
    path = sys.argv[1]
    tmc = open(path, 'rb').read()
    tmcl = open(path.replace('_TMC.tmc', '_TMCL.tmcl'), 'rb').read()
    out = None
    if '--obj' in sys.argv:
        out = open(sys.argv[sys.argv.index('--obj') + 1], 'w')
    base = 1
    for obj, vb, verts, idx, elements, stride in meshes(tmc, tmcl):
        xs, ys, zs = zip(*verts)
        print(f'object {obj} buffer {vb}: {len(verts)} vertices, {len(idx)} indices (max {max(idx)}), stride {stride}, '
              f'x {min(xs):.3f}..{max(xs):.3f} y {min(ys):.3f}..{max(ys):.3f} z {min(zs):.3f}..{max(zs):.3f}; '
              f'elements {[(e[1], e[2], e[4], e[5]) for e in elements]}')
        if out:
            out.write(f'o obj{obj}_{vb}\n')
            out.writelines(f'v {x} {y} {z}\n' for x, y, z in verts)
            out.writelines(f'f {idx[i] + base} {idx[i + 1] + base} {idx[i + 2] + base}\n' for i in range(0, len(idx) - 2, 3))
            base += len(verts)


if __name__ == '__main__':
    main()
