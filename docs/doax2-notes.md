# Dead or Alive Xtreme 2 notes

Xbox 360 game, played here through DOAX2 Recomp (`/home/pandi/Ports/Apps/DeadOrAliveXtreme2-DOAX2Recomp`).
Its disc files are in `extracted game files here/`. The viewer reads copies in `assets/doax2/`:
`chr.afs`, `mot.afs`, `itm.afs`, `cmn.afs`, `werone.afs`, `wertwo.afs`, `default.xex`. All data is
big-endian. Nothing here is compared with the running game yet. The viewer shows the body, the
first head (`AA`) and a suit with base textures, plays the motions with DOAX's solve and blends
weighted pieces across bones. No lights, hands, morphs or chest spring yet. Do not assume a
format matches DOAX or DOA3 because the names do; where DOAX2 does match DOAX, it is measured
below.

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
Usages: 0 position, 1 blend weight (float2, float3 or float4 after the 32 bytes: two to four
groups, the floats add up to one; see "Skinning"), 3 normal,
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
-7 toes. The table is indexed by body type. A body and its heads have one list, at their type:
Kasumi and Ayane 0, Lei Fang, Kokoro and Hitomi 1, Helena and Lisa 2, Tina 3, Christie 4. A suit
has a list per type, the same pieces fitted to each body; the viewer takes the body's type.
The pair's first number is a bone: see "Skeleton".

The head file's root node has no parent, and its own offset is not the body's. Put on the body's
head node (the child of the neck node, CAT bone 20), the head's neck edge meets the body's with no gap on
all five characters (`head_meets_neck`); hung from the neck with the head's own offset, the gap
was 2 to 29 mm. Suits carry their own copy of the body's node tree.

Suit CATs are 200 to 600 KB. Suit models hold their own copy of the hierarchy and several
variants of the chest and pelvis objects (187 objects in `vtn_tk_b_m`).

## Skeleton

DOAX's 32-bone skeleton. Tina's node offsets are DOAX's offset rows for her to the millimetre
(the rows are also in the game's image, `0x82BE21F0` on, 25 rows of 16 bytes per table with a
256.0 marker, rows 20 to 23 first as in DOAX). DOAX bone to body node: 0 chest 5, 1 head 6,
2 hips 0, 3 right ankle 12, 4 elbow 13, 5 wrist 14, 6 thigh 16, 7 knee 19, 8 shoulder 20,
9 left ankle 29, 10 30, 11 31, 12 33, 13 36, 14 37, 16 right collar 9, 17 eye 11, 18 toe 21,
19 neck 23, 20 left collar 26, 21 eye 28, 22 toe 38, 23 spine 40 (`BONE_NODES`; the same on the
five bodies, `bone_nodes_follow_the_links`). Other nodes sit on these with no offset.

CAT bone numbers 0 to 15 are DOAX bones. 16 to 23, from where bodies and heads put them: 16 and
17 the eyes (DOAX 17, 21), 18 and 21 the collars (16, 20), 19 and 22 the toes (18, 22), 20 the
neck (19), 23 the spine (23) (`SLOT_BONES`).

## Motions

`mot.afs`: per character `<name>.mot`, `bed_<name>.mot` and `gr_<name>NN<s|m|l|x>.mot`, with names
`tina`, `hika` (Helena), `leifang`, `lisa`, `gokei` (Christie), `kasumi`, `ayane`, `kokoro`,
`hitomi`; shared `comon*.mot`. About 550 motions per character.

DOAX's format, big-endian: 8000 words of block offsets (unused ones point at the placeholder
the first holds), a block word with 60 tracks in its high half and the frames in its low half,
60 track offsets, and DOAX's packed keys. Every 16-bit field is swapped, and so are the two
slope bytes at +8 and +9 of a cubic key, as if they were one 16-bit field: with that order,
consecutive keys' slopes agree within 0.0002 (median), against 0.0064 the other way. Values
come out as DOAX's: hips at 0.86 to 0.91 m, angles from 0 to 2 pi.

The viewer feeds them to DOAX's solve with the body's rows. Each rigid piece follows the DOAX
bone of its CAT number from where it sits at rest. Not compared with the game.

## Skinning

Pieces with blend weights have no bone list in the file; the game must hold one per CAT bone
number. Not found in the image (DOAX's table at `0x33F988` has no match). The viewer uses a
table inferred from the bodies (`blend_bones`):

- Group 0 is the piece's own bone.
- A group whose dominant vertices meet a rigid piece at rest takes that piece's bone: over 80
  motion frames, any other bone opens the seam by 6 mm or more (`blend_groups_from_seams`).
- The rest follow DOAX's groups for the same place: neck 1, 0, 19; spine 0, 2, 23; collars
  16, 8, 0, 30 and 20, 14, 0, 31. The forearms' third group (13 vertices) is the shoulder, the
  bone that stretched them least; the forearm still narrows near the elbow in some poses, so this
  one is doubtful.

Suits use only (bone number, group count) pairs that bodies have. Some suit weights add up to
more than one (a single 2.307, or 1.16 to 1.23 over two groups, in Tina's `on_s_m`); they are
scaled to one, since the game shows no spikes. Not checked against the game's shader.

## Live memory

DOAX2 Recomp runs under Wine. The guest's memory is mapped at host `0x200000000`: guest
address `a` is at `0x200000000 + a`, and physical memory shows four times, at guest
`0xA0000000`, `0xC0000000`, `0xE0000000` and higher. The XEX image is at guest `0x82000000`;
`assets/doax2/image_82000000.bin` is a copy of 32 MB from there, read from a running game.
Loaded CATs keep their lists but have their offsets turned into pointers.

`tools/re/doax2_live.py bones` finds the characters' bone arrays: 32 matrices of 64 bytes,
row-major with the translation in the last row, in the guest heap around `0x93A00000` during a
jet ski race. The array index is the CAT bone number (measured from the distances between
bones: bones 0 to 14 and 23 to 31 at DOAX's places, 16 and 17 at the eyes, 18 and 21 at the
collars, 19 and 22 at the toes, 20 at the neck, 15 under the hips on the ground). This confirms
`SLOT_BONES`. DOAX's arm roll relations (bones 24 to 31) did not hold on the jet ski pose
(errors of 0.4 to 0.6, except 0.03 for the hand's aligned bone); whether the race poses arms
another way or DOAX2's roll differs is not known. The hips sat at exactly 0.6 m with 0.75 and
0.01 next to them, which look like the race's constants, not motion tracks.

## Chest shapes

Pieces whose layouts have seven streams (the body's chest, collars and spine, and the suits'
pieces on the same bones) hold six whole position shapes after the base: streams 1 and 4
sideways (+-1.5 cm on average on Tina's chest), 2 and 3 forward and back, 5 up (3.5 cm) and 6
down (2.3 cm). The declaration binds three, so the game picks one of each pair: a spring along
three axes. How DOAX2 drives them is not read. The viewer drives only up and down, with DOAX's
vertical spring and weight on the chest bone: a positive value lowers the chest (DOAX's shape 1
moves down 1.3 cm), a negative one raises it. Checked running: values from 0 to 0.91 in one of
Tina's motions. The sideways and forward pairs stay at rest.

## Hands

`hand_vacance.bin`, `hand_gravure.bin`, `hand_volley.bin`, `hand_pwc.bin` in `chr.afs`, one per kind
of scene. `MPV\0`, then `MPP\0` blocks, one per hand: +0xC the hand's object in the body (31 the
left hand, 12 the right), +0x14 the pose count (122 and 125 in `hand_vacance`), then the poses'
offsets from the block. The first offset is a list of the hand's vertex byte offsets (stride
32), a set bit 31 starting each group of copies of one vertex, ended by -1: 689 groups, whose
members sit at the same place (checked on Tina's left hand). Each pose is 689 x 24 bytes,
position and normal per group, in the hand object's own space, plus 8 bytes. The pose most ids
share (79 of 122) is the base mesh exactly; the others move the fingers by up to 10 cm.

Which pose a motion asks for is in the scripts (`<name>.bin`, `anime_cm.bin` in `mot.afs`):
DOAX's script format, big-endian, with the argument counts of its 96 operations in the table at
`0x82BDDC78` of the image (6 bytes each, as DOAX's). Op 0 plays a motion, 1 waits for a frame,
12 and 13 set a hand's pose (hand 0, 1 or 2 for both; pose; frames). Tina's scripts hold 454 hand
events, the shared ones 789. Poses used: 0 to 6, 12 to 14 and 16.

The pose number to file entry mapping is inferred: entry p + 1 (entry 1 is the base mesh, as
pose 0 is in DOAX; entries 2 to 13 are the first with their own data). DOAX's pose table at
`0x33EE38` has no match in the image. Which script hand is which block (the viewer takes hand 0
as the first block, the left hand) is not checked either. In a jet ski race both hands' working
buffers (24 bytes a group, in the heap near `0x116C60000`) held entry 13 of `hand_pwc.bin`
exactly; the race's scripts ask for no pose there, so the game sets it in code.

The viewer uses `hand_vacance.bin` for every motion; the game picks the file by scene.

