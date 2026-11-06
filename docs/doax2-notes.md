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
0x06 8888 (bytes B, G, R, A after the swap), 0x12 DXT1, 0x13 DXT3, 0x14 DXT5 (most hair and
lash textures). Tiling as Xenia's
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
bone of its CAT number from where it sits at rest.

Compared with the running game (`live_poses_match_motions`, captures from
`tools/re/doax2_live.py`): Tina in a match played `comon_pyn.mot` 512 and 506; 4 of 12 samples
matched a whole frame within 6.9 mm at the worst bone (positions from the hips in the frame of
bone 15), the rest 4 to 21 cm off, taken between frames or while the game blends motions. Lei Fang in a match played
`leifang.mot` 1299: 6 of 12 samples matched consecutive frames within 2.7 mm, the rest within
0.8 to 4.6 cm. This checks the tracks, DOAX's solve and the rows from the node tree.

## Skinning

Pieces with blend weights have no bone list in the file; the game must hold one per CAT bone
number. Not found in the image (DOAX's table at `0x33F988` has no match). The viewer uses a
table inferred from the bodies (`blend_bones`):

- Group 0 is the piece's own bone.
- A group whose dominant vertices meet a rigid piece at rest takes that piece's bone: over 80
  motion frames, any other bone opens the seam by 6 mm or more (`blend_groups_from_seams`).
- The rest follow DOAX's groups for the same place: neck 1, 0, 19; spine 0, 2, 23; collars
  16, 8, 0, 30 and 20, 14, 0, 31. The forearms' groups lie at the elbow, the wrist and the middle
  (275 vertices over half weight, 8 to 16 cm from the elbow); the middle is DOAX's twist bone 24
  or 25. The shoulder, which the least stretch had picked, sank the forearm below the elbow, as
  the user saw.

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

## Heads

Per character `v<xx>AA`, `AB` and for some `BA`, `BB`. On Lisa and Lei Fang `AA` and `AB` look the
same from the front; `AA`'s eyes have 129 vertices against 496 in `AB` on Lei Fang, Lisa and
Christie. Lei Fang's `BA` and `BB` add braids. The viewer uses `AA`.

A head object holds three layouts: the face and scalp (opaque, stage 0 kind 1), small parts
such as brows and lashes, and hair strands (up to 31 cm long) whose stage 0 is kind 0, a colour
texture with alpha, and stage 1 kind 1 a grey band of shine, not a mask (taking it as a mask cut
most of Tina's hair away). The viewer draws those draws, and any other whose texture has over 1%
see-through texels (such as Tina's lash line, a black texture with alpha on a draw of kind 1), as
a translucent piece of their own, blended by the texture's alpha after the solid pieces.

DXT5 alpha was decoded into a quarter of each texture, block numbers taken for pixels, until
2026-11-06: hair, lashes and every alpha texture came out opaque but in one corner. That was the
wrong hair and too dark lashes the user saw, and why Lei Fang's fringe covered her eyes: with the
alpha right her fringe stops above them.

## Lights and materials

The shaders' constant tables, in the heap with each shader, name their registers: vertex
shaders take `AmbientLightP1` c68, `AmbientLightP2` c69, `DirLightVec` c71-74, `DirLightCol`
c75-78, `PointLightPos` c96-99, `PointLightCol` c100-103, `PointLightAtt` c104-107, `SpotLight`
c108 on, `DirLightBool` b5-8, `PointLightBool` b9-12; some take `SunDir` c200 and
`SunColorAndIntensity` c207 or `DirLightCol0/1` c10/c12. Blocks laid out in that order (two
ambients, four booleans, four directions, four colours, then point lights with range 100) sit in
the heap at `0x4007E270`, `0x4007E450` and `0x4007E630` during a match: three light sets, found in
no file on disk or in the image, so built at run time.

`DirLightVec` is the way the light travels in Direct3D's camera space (z into the screen): the
strongest light of each set comes down and away from the camera, lighting faces from the front
and above whatever the camera's angle. The viewer turns them with its own camera
(`set_lights_follow_camera`).

A draw's colours sit at the offset in its +0x20 (0x80 in all read): diffuse, ambient, specular
(vec4 each), then the power (1, 0.5, (0.3, 0.2, 0.2), 15 on skin; heads vary, power 50 on some
parts). Read at a fixed +0x70 at first, the heads' diffuse came out zero and the game lights
looked far too dark, as the user saw. The viewer lights DOAX2 like DOA2U's materials with the
three sets, ambient from `P1` (`P2` left out). "Game lights" in the Shading window switches to
the viewer's plain shading. Not compared with the game.

## Faces

A head CAT's word 3 is the offset of an `MPV` block of face shape sets, read like the hands'
(`hand_poses`), with two differences: list entries carry the vertex layout in bits 24 to 30
(the head has three layouts of strides 32, 20 and 24), and a face pose is 12 bytes a group,
the position alone. Each head has two sets on object 0:

- Eyes, 4 entries: 0 the lists, 1 the base (open, the mesh exactly), 2 shut, 3 shut smiling.
- Mouth, 8 entries: 1 the base, then A, I, U, E, O and a closed smile (seen on Tina).

No working buffer holds them in the running game (only the file's copies were found): the
blend is done on the GPU, so the weights in use cannot be read from memory.

Blinks: the scripts' op 22 comes every 20 to 30 frames, mostly with 2 or 3, which the viewer
takes as a blink to that eye entry: shut over 3 frames, held 2, open over 4 (`blink_weight`,
timing chosen, not read). Checked by eye on Tina's motion 77, shut at frame 10 and open at 30.
What drives the mouth is not known (op 94, two small arguments, is a candidate; voices may).

Script events are now taken from the first script that plays a motion only: several scripts
play the same motion, and their events were added up (Tina's hand events 454 to 426).


## Hair

Long hair is a cloth simulation described in the head CAT. Word 5 is the offset of a
`MULTI_CLOTH` block: +0x18 the number of cloths, from +0x40 their offsets from the block. Each
`CLOTH_DATA` (name at +0x70, such as `vksAAlong_l`) holds sections with 16-byte name tags:
`PROPERTY`, `GRID_DATA`, `EDGE`, `TRIANGLE`, `FACE_TABLE`, `FACE`, `FIX_GRID`, `GRID_WRITE`,
`THREAD_DATA`. Offsets inside are from the start of `GRID_DATA`.

- `GRID_DATA`: +0x24 the number of particles, from +0x40 one of 0x30 bytes each: position
  (head space, w 1), the number and offset of its edges, of its triangles, of its faces, its
  row (0 at the root) and a word the game turns into a pointer to its state.
- `EDGE`: per particle, entries of 16 bytes: the other particle (as an offset), the rest
  length (the file's distance to 1e-8), kind 1, 2 or 3, zero. Each edge is listed from both
  ends. On `vksAAlong_l` (14 strands of 25 rows): kind 1 joins neighbours in a row and along a
  strand, kind 2 the diagonals, kind 3 a particle with the one two rows down.
- `FIX_GRID`: +0x1C the count; entries of 0x20 bytes, a position and (1, particle, 2, 0). The
  14 fixed particles are row 0, at their file positions.
- `GRID_WRITE`: +0x20 the count; from +0x60 pairs (0x80000000 | particle, layout << 24 | byte
  offset in that layout's vertex buffer). Every particle writes one vertex of layout 2, and
  that vertex is the particle in the file. So the hair mesh's vertices are the particles.
- `THREAD_DATA`: chains of particles (per strand, 24 or 25 entries of (particle, two small
  numbers)) with angle limits (0.2618 = 15 degrees, 1.5708 = 90 degrees). Not decoded further.
- `PROPERTY`: floats such as 0.2875, -0.03, -0.022, 0.2, 0.1, 1.125, 0.4. Not decoded.

Heads with cloth: Kasumi (`vks` AA and AB: long, mid and uchi, left and right), Ayane, Kokoro,
Hitomi and Lei Fang (`vleAA` and `vleAB`, the same six names). Tina, Helena, Lisa and Christie
have none; Lei Fang's braided `BA` and `BB` have none.

Live (2026-11-06, `tools/re/doax2_hair_capture.py`): the loaded `CLOTH_DATA`'s word 10 points
to the particle states, 0x80 bytes each in particle order: world position at +0, a vector near
(0, -0.0115, 0) at +0x10 (zero on fixed particles), the normal at +0x20, the previous position
at +0x30 (equal to the position on fixed particles). The game writes them in bursts; between
quiet states the previous position is exactly the last position, so each burst is one step.
Pointers into guest 0xE0000000 and up are 0x1000 below where the data shows in the host view.

Fitting single steps of Kasumi's `long_l` (49 steps, little motion): velocity is kept partly
in the head's moving frame (inertia 0.85, carried by the head 0.63) and edges pull towards
their rest length (kinds 1 and 3 about 0.07 and 0.02 a step, kind 2 none). That leaves 0.19 mm
near the root and 1.2 to 1.4 mm in the middle rows, where the tips get a steady push the same
way each step, likely collision with the body. The field at +0x10 adds nothing to the step.
The game lets the hair stretch: near the root its segments are 12 to 15 percent longer than
at rest. So this is a fit, not the game's method yet.

### The game's cloth step (from the code, 2026-11-06)

Read from `assets/doax2/image_82000000.bin` with capstone and a small VMX128 decoder (capstone
does not decode VMX128; the opcodes are Xenia's). The tag strings are at `0x822A7A94` on; the
loader reads `GRID_DATA` at `0x8249834C`, `FIX_GRID` `0x8249854C`, `GRID_WRITE` `0x82498634`,
`THREAD_DATA` `0x824986CC`. The step is `0x82493920` (r3 the cloth; +0x20 its working
`PROPERTY`, +0x24 the grid, +0x28 the states, +0x34 the threads, +0x44 the colliders, +0x4C the
owner). `P[x]` below is the working `PROPERTY` at offset x from its tag.

1. The owner's 0xA0 bytes of property are copied to the working `PROPERTY`, then scaled: 0x34 by
   owner+0xC0, 0x38 by owner+0xA4, 0x40 by a time ratio (owner+0xA0), 0x44, 0x4C to 0x60 and 0x70
   to 0x78 by a global factor (`0x833B84CC`, 1 when 0).
2. The owner counts frames (+0xAC up to +0xA8): on frames in between, and when owner+0xC4 bit
   0x10 is set, the particles (position, previous position, normal) are only carried by the
   head's motion, the new matrix times the inverse of the last (kept at owner+0xF0). That is
   why the live states change in bursts.
3. On a step: previous = position, velocity = (position - previous - vm) * P[0x40] + vm, kept
   at +0x10 with the lock mask at +0x40; vm = (1 - owner[0x120]) * owner[0xB0], the frame's
   velocity. P[0x40] is 0.2 on Kasumi: most of the velocity relative to the head is lost.
4. `0x824949A8`, forces per particle: the relative velocity, plus gravity, plus air (wind from
   P[0x70..0x78], turned by the matrix if P[0x3C] bit 0x20; random noise from `0x825C9F98` if
   bit 0x01; drag against the particle's normal), plus springs over its edges,
   k * (|d| - L) along d, with k by kind and sign: kind 1 P[0x4C] stretched, P[0x50]
   compressed; kind 2 P[0x54], P[0x58]; kind 3 0 stretched, P[0x5C] compressed. The sum is cut
   to length P[0x44] and the velocity becomes it plus vm.
5. A callback of the owner (+0xCC), then `0x82495030`: collisions by collider type (1, 2, 3,
   4, 8, 9), each with its own function (`0x8249B430`, `0x824A2318`, `0x824A2490`,
   `0x824A4690`, `0x8249B750`...). Not read yet; where the position moves by the velocity is
   not found yet.
6. One or two passes (owner+0xC4 bit 0x08 gives two) of: threads (`0x82499900`, f1 = P[0x48]),
   `0x82494380`, `0x824948B8`, colliders of kinds 1 and 2 (`0x824AB4B8`, `0x824ABBE8`),
   `0x824944A8`, `0x82495FE8`. Thread type 1 (Kasumi's) caps each link at its edge's rest
   length times P[0x48] (1.125), root first, moving the child only (masked by +0x40). That is
   the 12 to 15 percent stretch seen near the root. Types 2 and 5 call `0x82498EE0`, 3 and 4
   `0x824988D0`, and every thread first calls `0x824988D0`.
7. `0x824970D8` and `0x82497450` then write the vertices.

### Checked against the live states

On `vksAAlong_l` (49 steps): the velocity a step leaves at +0x10 is, to 0.015 mm rms (0.07 max),
0.2 * (position - previous) + gravity + air + springs, with the fixed particles already at the
new head position, the stiffnesses and gravity at half the file's values (the global factor is
0.5 in DOAX2 Recomp's 60 fps mode: 0.1 and 0.05, gravity -0.011) and the air term with
P[0x38] 0.5 and P[0x64] 0.2. Owner+0xB0 was 0, so the frame velocity was 0.

The position is then position + velocity (`0x82495EA4` when there are no colliders), after
which threads, angle limits and colliders pull it back. Gravity alone moves the hair 11 mm a
step, and most of it is undone by collisions with the head, back and shoulders: integration
plus the two thread caps leave 5 to 14 mm, so colliders are needed to match.

`THREAD_DATA` after the thread list holds angle limits (records of 0x40: 1, 0x40, then 0.2618
or 0.5236 or 0.1222 and 0.1571, a count, the same again, 1.5708, a number, 0.9 or 0.75),
applied by `0x82499B40` through `0x82494380`. Not decoded.

Colliders: the loaded `CLOTH_DATA`+0x38 points to a chain of records (in the file right after
the angle limits): type, CAT bone, offset of the next, size, then 1, a flag, 0, an index, and
the shape:

- type 4, an ellipsoid: centre (x, y, z, 1) and radii. Head (bone 1): centre (0, 0.08,
  -0.005), radii (0.09, 0.105, 0.095). Neck (20), collars (18, 21), chest (0, (0, 0.2, -0.02)).
- type 8, a box: centre, two axis rows, half sizes. Chest (bone 0): (0.19, 0.2, 0.16).
- type 1, a capsule: start, length, radius, axis. Upper arms (bone 8 and others): 0.35 long,
  0.06 radius.
- type 2: a radius 0.05, or (0.03, 0.01, 0.16) on the head.

Each record keeps an offset (0x37C720 and up) that the game turns into a work area whose +0x100
holds the collider's world matrix. `0x82495030` runs a collision for each particle's move from
its position to position + velocity, by type: 4 `0x824A2490`, 8, 1 and 9 `0x8249B430`, 2
`0x824A2318`; a second round 4 `0x824A4870`, 8, 1 and 9 `0x8249EE88`, 2 `0x824A43E8`, 3
`0x824A4690`. `0x8249B750` (3800 instructions) is a closest point and segment test used by them.
None of these is decoded yet.

Other vertices of layout 2 (2844 to 3367 on `vksAA`) also move but are not particles. The head
CAT has blocks tagged 0x06010001 from 0x1680 (two values 0.3 and 0.8, a point behind the head,
offsets) that may drive them. Not decoded.
