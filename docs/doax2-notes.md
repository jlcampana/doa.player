# Dead or Alive Xtreme 2 notes

Xbox 360 game, played here through DOAX2 Recomp (`/home/pandi/Ports/Apps/DeadOrAliveXtreme2-DOAX2Recomp`).
Its disc files are in `extracted game files here/`. The viewer reads copies in `assets/doax2/`:
`chr.afs`, `mot.afs`, `itm.afs`, `cmn.afs`, `werone.afs`, `wertwo.afs`, `default.xex`. All data is
big-endian. Nothing here is compared with the running game yet. The viewer shows the body, the first head
(`AA`) and a suit in the files' rest pose (T pose), with base textures; no motions, lights,
skinning across bones or morphs yet. Do not assume a format matches
DOAX or DOA3 because the names do.

## Archives

AFS, little-endian table, 48-byte name entries. Names repeat in the wear archives, so entries are
read by their place.

- `chr.afs`: per character `v<xx>1.tpr` and `.cat` (the body without a head) and `v<xx><A-D><A-B>`
  (`.tpr` and `.cat`, the head: face and hair, and two eyes), plus `hand_*.bin`.
- `werone.afs`, `wertwo.afs`: suits, `v<xx>_<kind>_<letter>_m`, an XPR2 and a CAT each, usually three
  in a row with the same name (colours or states).
- `mot.afs`: motions, `anime_cm.bin`, `comon*.mot`, `<name>.bin` and `.mot`.
- `itm.afs`: accessories and items (`acc_hea`, `acc_wri`, presents).
- Characters: `tn` Tina, `ks` Kasumi, `he` Helena, `co` Kokoro, `le` Lei Fang, `ay` Ayane,
  `ls` Lisa, `cr` Christie, `ht` Hitomi. The limit on soft bodies and the DOAX roster rule apply:
  only Tina, Helena, Lei Fang, Lisa and Christie are in the viewer. Kokoro is left out with
  Kasumi, Ayane and Hitomi, as she is young in the series.

## XPR2 (`.tpr`)

Header: `XPR2`, header size, data size, resource count, then per resource 16 bytes: tag, offset
from +12, size, name offset. The data section starts at 12 + header size; buffer and texture
addresses are relative to it.

Resources: four `USER` blocks (`MdlInfo` count header, `MdlGeo`, `MdlInfo`, `Hie`), `TX2D`, `VBUF`,
`IBUF`. Buffer headers end in Xenos fetch constants: +0x18 address with the type in its low bits,
+0x1C size (vertex buffers: in words, shifted by 2).

### MdlGeo

A list of `ObjGeo` records (offsets from +0x30). Each:

- `ObjGeo`: +0x14 the number of draws, +0x28 the offset of its `GeoDecl`, from +0x40 the draws'
  offsets, all from the record's start.
- `GeoDecl`: +0x14 the number of vertex layouts, from +0x30 their offsets from the `GeoDecl`.
- A layout: +0 its stream list's offset from the layout, +4 the stream count, +8 the index
  buffer, +0xC the index count, +0x10 the element count plus 2, +0x14 the vertex count,
  elements from +0x30 (`D3DVERTEXELEMENT9`, 12 bytes, ended by stream 0xFF). A stream entry is
  16 bytes: the `VBUF` index and the stride.
- A draw: +8 its layout, +0x10 its first index, +0x14 its index count, +0x18 the stage count,
  +0x1C the stages' offset from the draw. A stage is 0x40 bytes: stage, kind, texture. Stage 0
  kind 1 is the base colour; kind 6 on stage 2 is probably a normal map; kind 3 with texture
  -1 is on empty objects.

Element types: `0x2A23B9` float3, `0x2C23A5` float2, `0x1A23A6` float4, `0x2C235F` half2,
`0x2A2190` three signed normalized values in 11, 11 and 10 bits from the low bits (normals and
tangents: length 0.998 to 1.0 on Tina's body this way, 0.4 to 1.4 the other way round).
Usages: 0 position, 1 blend weight (float2, float3 or float4 after the 32 bytes), 3 normal,
5 texcoord (three sets), 6 tangent. Extra streams 1 to 3 hold more positions (usage 0 with
index 1 to 3): morph shapes on the chest and some other objects. Six such streams on some
objects: probably positions and normals.

Indices: 16-bit, strips restarted by 0xFFFF; in Tina's body every strip is one triangle.

Some `VBUF`s are not named by any object (two of 3492 vertices and three of one after object 12
of `vtn1`). Not known what they are.

### Hie

Per node 0x50 bytes: a 4x4 matrix, row-major with the translation in the last row, then shorts:
the object the node draws, the parent (-1 for none), the child count and the children. On
Tina's body every rotation is identity: node 0 is the pelvis, 40 the spine, 5 the chest, 23 the
neck, 6 the head with two eye nodes, 9 and 26 the collars, 20 and 37 the shoulders, then upper
arm, forearm and hand; 16 and 33 the thighs, then knee, ankle, toes. Nodes 1 to 4 have no parent
and sit at the feet. The arms are out to the side (T pose), 1.56 m between hands.

### Textures

`TX2D`: a 28-byte resource header, then the fetch constant: word 0 bit 31 tiled, word 1 format
(low 6 bits), endian (bits 6 and 7: 1 swaps bytes in 16 bits, 2 in 32 bits), base address
(bits 12 up), word 2 width - 1 (13 bits) and height - 1 (13 bits from bit 13). Formats seen:
0x06 8888 (bytes B, G, R, A after the swap), 0x12 DXT1, 0x13 DXT3, 0x14 DXT5. Tiling as Xenia's
`TiledOffset2D`. Checked by eye on Tina's body textures: skin, nails and the body map come out
whole.

## CAT

Big-endian. The body's (`vtn1.cat`, 604 bytes) holds from 0x20 a list of (game bone, object) pairs
with markers: -5 and -6 before alternatives (the same foot in two shapes), -7 before shared
pieces, -1 at the end. Objects 0, 1, 5, 8, 24 and 27 are not in the list: 0 is the chest, with
morph shapes, the others are empty. Then two materials from 0x184 and a directory at the offset
in word 1. Not read further yet.

Section kind 2 of the directory is a table of draw list offsets (zero for none). Each list is
(game bone, object) pairs ended by -1, with a negative marker after some pairs: -4 eyes, -5 and
-6 the two shapes of the same foot (-5 on the leg nodes, -6 on the parentless nodes 1 to 4),
-7 toes. Bodies and heads have one list, at the table's fourth entry; suits have up to five,
each a whole set of the suit's objects, not told apart yet (the viewer draws the first).
Bones seen: 0 chest, 1 head, 2 pelvis, 16 and 17 eyes, 18 and 21 collars, 20 neck, 23 spine.

The head file's root node has no parent. Its offset matches the body's head node from the neck
(0.076 m against 0.061 m up, 1.5 cm forward in both on Tina), so the viewer hangs the head from
the body's bone 20. Suits carry their own copy of the body's node tree.

Suit CATs are 200 to 600 KB. Suit models hold their own copy of the hierarchy and several
variants of the chest and pelvis objects (187 objects in `vtn_tk_b_m`).
