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
  `ls` Lisa, `cr` Christie, `ht` Hitomi.

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

**Pending:** Validate the character-specific animations for Kasumi, Ayane, Kokoro and Hitomi. Kasumi's, Kokoro's and Hitomi's were not loaded at all until 2026-11-10: `motion_name` looked them up as `ka`, `ko` and `hi`, while their bodies are `ks`, `co` and `ht`. They load and play now (checked by eye on Kasumi and Hitomi); none is compared with the game.

Spring bones against the game (2026-11-10, `live_spring_bones_match`, the game's own array at
0x833B5310): Helena standing still matched no motion frame closer than 17 cm (likely a blend or
a pose the game adjusts). Kasumi sitting matched `gr_kasumi01m.mot` 12 frame 1046 within 8.8 cm
at the worst bone; there bone 0 turned 1.2 degrees from the game's, bone 8 2.3 and bone 14 16.7,
the springs' points 14 to 22 mm off. Not a clean frame match, so not conclusive for bone 14.

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
three axes, read from the code below ("The springs, read from the code"). Before that the viewer
drove only up and down with DOAX's vertical spring.

How the game blends them, read live on 2026-11-10 (`tools/re/doax2_gpu_constants.py`). The Xbox
360 D3D layer writes vertex shader constants as PM4 type-0 packets to registers 0x4000 + 4 c in
the command buffers (physical memory, guest 0xA0000000 on); the ring keeps about 16 frames.
Vertex fetch constant 95 - n holds stream n's buffer (physical address | 3).

- The chest declaration has four positions: `POSITION0` on stream 0 (the base) and `POSITION1`
  to `POSITION3` on streams 1 to 3. Per draw the game binds one shape of each pair to them:
  stream 1 shape 1 or 4 (sideways), stream 2 shape 2 or 3 (forward, back), stream 3 shape 5 or
  6 (up, down). All eight combinations were seen on Ayane's and Kasumi's body (`vay1`, `vks1`,
  the same data).
- The skin shader (microcode after the constant table at `0x82240500`: fetches of constants
  95 to 91 into r0, r3, r8, r5, r4, then `MUL`/`MAD` by c49) computes
  `pos = x * base + y * stream1 + z * stream2 + w * stream3` with `PosWeight` c49 = (x, y, z, w).
  The weights always add up to 1, so x = 1 - y - z - w: y, z and w are the shape weights, never
  below 0, the pair's member giving the sign. y reached 1.0 (a clamp) with x down to -0.72.
  Shaders with `BlendSup` repeat the sum with `PosWeight2` c105 and stream 4.
- Seen in a scene with three characters: y 0.02 to 1.0, z 0.2 to 0.47, w 0 to 0.41, changing
  smoothly frame to frame.

Each chest draw also carries the piece's `WorldMtx` (c0-c3, rows as axes, translation in c3)
set just before it, so weights and the chest's matrix come in pairs per frame
(`doax2_gpu_constants.py log`; captures `target/captures/doax2_chest_log1.pkl`, Tina and a
second character on her body, and `doax2_chest_log2.pkl`, Kasumi moving). The ring holds about 18
consecutive frames per character, so one read every 0.3 s gives runs of consecutive frames.

First readings (2026-11-10): the forward and back weight follows gravity seen from the chest.
Kasumi lying back on a deck chair, gravity -0.936 along the chest's z: shape 3 (back) at 0.465,
steady. Tina standing, gravity +0.21 to +0.38 along z: shape 2 (forward) at 0.40 times it,
following it frame by frame. The sideways weight swings widely while walking (0 to 1). A
per-axis second order fit over runs of frames leaves 0.001 (forward) and 0.012 (sideways) rms,
but its coefficients are near (2, -1), so the frames are smooth and the spring's constants are
not pinned down yet: this needs longer runs and the weight curve (DOAX uses (1 - k) v + k sqrt v).

Found in memory during a beach volleyball match with four players (2026-11-10, host view
0x933A0000 on, which is guest 0x833A0000 on, the bone arrays' heap):

- `0x933B4340`, records of 0x30 bytes, one per character in play: `PosWeight` (c49) as sent,
  then `PosWeight2` (c105), then 16 zero bytes. Every weight vector adds up to one. In jumps they
  reached (-2, 1, 1, 1): each shape weight clamps at 1, and the base weight takes the rest.
- `0x933A1000` on, blocks of 0x180 bytes per character (characters at 0x933A1010 + 0x180 k)
  holding the springs' state as clamped vectors (components in -1..1) with their previous
  values, deltas, a world point and a velocity. Over 40 simultaneous reads, each weight equals
  the absolute value of one field exactly: for the character at block k, sideways = |x| and
  forward = |z| of the vector at 0x933A1330 + 0x180 (k - 1), vertical = |y| of the vector at
  0x933A1254 + 0x180 (k - 1). So the signed spring values live there, and the pair's member is
  their sign. The layout of the rest of the block (sub-blocks of 0x70: current, previous,
  before that, delta, point, velocity) is a first reading, not checked. `tools/re/` has no
  reader for it yet; the scratch scripts read 0x600 bytes from 0x933A1000 with the bone arrays.

Per character the block holds sub-blocks of 0x70 bytes from 0x933A1010 + 0x70 n (vertical at
0x933A1250 for the second character, then one unused by the weights at 0x933A12C0, then the
sideways and forward one at 0x933A1330). In each, +0x10 is a new raw vector (clamped to -1..1),
+0x20 the raw one before it, and +0x00 the output, which moves 30 percent of the way to the new
raw value at each update: s = previous + 0.3 (new - previous), checked on all three components of
one update. The weights are |s|. The raw values jump by up to 0.3 between updates; where they
come from is not read. Some sub-blocks hold a point 0.2 to 0.3 m from a chest and fixed to it
(+0x40 or +0x50), perhaps the anchor.

A second log (683 states in 25 s, four characters on the beach) gives the sub-block layout, three
per character (0x70 bytes each, the first character's at 0x933A10D0): +00 the output, +10 the
new raw vector, +20 the raw one before it (it shifts in 97 percent of updates), +30 a vector
that never leaves -0.2..0.2 and changes smoothly, +40 a world point 0.17 m from that
character's chest (moving with it), +50 a small vector, +60 a parameter (0.03 on the first
sub-block). The first two sub-blocks' points sit at the chest's height about 10 cm apart, the
third lower. Linear fits of the raw value against +30, its previous step and itself fail
(rms 0.13 against 0.2), so the rule is not a plain second-order spring on these fields. A
search of the code for 0.3, 0.2, -0.2, 1 and -1 loaded together found nothing; the constants
may come from the block (+60) or a table.

### The springs, read from the code (2026-11-10)

The blocks are a static array in the image's data (guest `0x833A10D0` + 0x180 per character
slot, three points of 0x70 bytes, then a tail), which is why no pointer to them was found in the
heap. Searching the code for loads of addresses in that range finds the functions; the VMX128
instructions were decoded with `tools/re/xbox360_ppc.py` (capstone does not know them).
`ChestSprings` in `src/formats/doax2.rs` follows them.

- `0x8248A640`, per character each frame: unless the slot is off, runs the set-up below when
  asked, adds the frame's tick (`0x833B84CC`: 0.5 a frame with the recomp at 60 fps, so the
  springs step 30 times a second) to an accumulator (tail +0x174), runs `0x82489D68` once per
  whole tick and keeps the fraction. Each point's output (+00) is then
  `previous + (value - previous) * fraction`: the "30 percent" above was that fraction at the
  time, not a smoothing constant. Then `0x8248A348` writes the weights.
- `0x82489C40`, set-up: zeroes value, previous, velocity, output and offset, puts the point at
  its place on the bone and the accumulator at 1, so the first frame steps once.
- `0x82489D68`, one step. Point k hangs from bone `[8, 14, 0][k]` (table `0x82C37D8C`; the upper
  arms and the chest) at rest offset `(-0.1, 0, 0)`, `(0.1, 0, 0)`, `(0, 0.3, 1)` (`0x83C0A7F0`).
  With the bone's current matrix M and the point P where the bone held it at the last step:
  - offset `d = M^-1 P - rest`, push `a = K (d - d_last) + C value`, with K = (-7, -8, -3)
    and C = (-0.04, -0.08, -0.03) (parameter row picked by tail +0x15C, 0 for everyone seen;
    rows 1 and 2 differ only in damping y 0.96 and K x -8).
  - drift `(R^-1 g - g) * (-0.2, 0.01, 0.03)` with g = (0, -1, 0) and R M's rotation: how far
    gravity seen from the bone is from straight down.
  - quiet steps (+0x64): when |d| changes by more than 0.01 in a step, the counter goes to 10
    (if below), and counts down one a step. While it is 0, point 0 also pulls towards point 1's
    value by a coupling (+0x60) that moves halfway to 0.03 each step (from table `0x82D63A10`
    by +0x68, 0 in all reads); otherwise the coupling is 0.
  - `velocity = velocity_last + a`, `value = clamp(value + velocity + drift, -1, 1)`, then the
    velocity is damped by D = (0.9, 0.97, 0.5), clamped to -0.2..0.2 and zeroed on axes where
    the value hit the clamp. P becomes the rest point on the current bone, d_last = d.
  - Fields: +00 output, +10 value, +20 previous value, +30 velocity, +40 P, +50 d_last, +60
    coupling, +64 quiet steps. Tail: +0x150, +0x154 damping overrides for x and y, used when
    flag bits at +0x160 are set; +0x158 set-up request; +0x15C parameter row.
- `0x8248A348`, weights: sideways from point 2's x (shape 4 if below 0, else 1), forward from
  its z (shape 2 if above 0, else 3), vertical from point 0's y (shape 5, up, if below 0, else
  6); weights are the absolute values and the base takes 1 minus their sum. A second set uses
  point 1's y instead, written as `PosWeight2`; the body's chest layout (four position streams)
  does not use it, so which pieces do is not known.

Checked by stepping 55 logged states (`springlog`, four characters on the beach): 44 match the
next state exactly (value and velocity); the other 11 were read while the game was mid step or
several steps apart. The ignored test `chest_springs_match_logged_steps` replays them from
`target/captures/doax2_spring_steps.txt`.

The viewer and the export now use these springs on the game-space bones (the solve's z flip
undone): three shape pairs per chest piece, as shape keys in three sets in the export.

The bones' frames were compared without matching poses (ignored test `spring_bone_axes` against
the volleyball log): in both, the elbow lies on the upper arm's x axis (+x on bone 8, -x on 14,
so the points sit 10 cm inside the shoulders), the forearm stays in the upper arm's x-z plane
and bends towards +z on both arms, so the roll matches too, and from the chest y points to the
neck and x to bone 8. Not checked: the same frame of a motion side by side, which needs the game
running with Tina, Lisa or Helena (`doax2_live.py array`, then `live_spring_bones_match`; the old
`doax2_live.txt` matches no motion within 19 cm), and the parameter rows and damping overrides
other characters may use.

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

The command buffers hold the same constants per draw (`tools/re/doax2_gpu_constants.py lights`).
In a scene read on 2026-11-10 (the beach at sunset, on a deck chair in the shade, three characters; in the viewer as its fourth set): `AmbientLightP1`
(0.39, 0.32, 0.289), `AmbientLightP2` (-0.01, 0, 0.022), three fixed directions (0.549, -0.361,
-0.753), (-0.942, 0.334, 0.028), (-0.079, -0.991, 0.104) with colours (0.41, 0.3, 0.195),
(0.05, 0.05, 0.1), (0.34, 0.33, 0.2), and a fourth whose direction and colour change per object
(colours 0.35, 0.33, 0.25 or 0.7, 0.66, 0.5 or 0.322, 0.317, 0.265), perhaps a light placed for
each character. `HemisphereMat` c4-c6 is the identity on most draws and a rotation on others.
At the pool (2026-11-10, Kasumi on a deck chair, in the viewer as "Pool, deck chair"):
`P1` (0.465, 0.455, 0.5), `P2` (0.045, 0.055, 0), lights (-0.578, -0.79, 0.205) (0.445, 0.44,
0.401), (0.544, 0.291, 0.787) (0.165, 0.165, 0.145), (0.206, -0.954, -0.219) (0.155, 0.145,
0.15), and a fourth near (-0.867, -0.494, 0.065) (0.37, 0.355, 0.265). There `HemisphereMat`
was not the identity on many draws but a rotation drifting frame to frame, as if it followed the
character's body (she was lying down), so the hemisphere's "up" may be the character's.
`SpeqObjCol` c67 is a specular colour with the power in w (0.14 and 2; 0.574, 0.51, 0.477 and
17.7; and others).

### What the shaders do with them (2026-11-10, decoded from the image)

The shaders sit in the image as containers (`tools/re/doax2_shaders.py`: 456 of them, 79
vertex and 158 pixel shaders named `vs_3_0`/`ps_3_0`, the rest unnamed). Decoded: the skinned
vertex shader with specular (container `0x8206C5D0`, the body's chest layout) and a rigid one
(`0x820661E8`). Lighting is per vertex:

- Normal: the base normal and the three chest shape normals blended by `PosWeight`, like the
  positions, then skinned with `BlendMtxNml1-3` into world space and normalised.
- Ambient: `AmbientLightP1 + AmbientLightP2 * h`, h the normal's dot product with
  `HemisphereMat`'s second column (its y when the matrix is the identity, as on most draws): a
  hemisphere ambient, P2 added towards up and taken away towards down.
- Directional lights, each under its `DirLightBool` (b5-b8): `max(0, n . -DirLightVec) *
  DirLightCol`, in world space.
- Point lights (b9-b12) and spots (b24-b27): towards `PointLightPos`, within its range (w),
  times `1 / dot(PointLightAtt.xyz, (1, d, d^2))`, with a cone from `SpotLight` and `SpotRange`
  (an arccosine by polynomial).
- Specular: towards the camera (`CamPos` c16, a local viewer), per light
  `max(0, n . normalize(v - L))^SpeqObjCol.w * DirLightCol` (point lights with their
  attenuation), summed and times `SpeqObjCol.rgb`, added to the diffuse. No clamp.
- The pixel shader (`0x820B3CC8`, the plainest with a shadow map) takes the texture times that
  colour, darkened by the shadow map, then blends to the fog.

No shader has a material colour constant, and the vertices carry none: the draw's colours
(diffuse, ambient, specular, power at the draw's +0x20) must be folded into the light constants
by the CPU. That fits the fourth light's colours, (0.7, 0.66, 0.5) and (0.35, 0.33, 0.25), one
exactly half the other. How they are folded was not found: the register numbers 67 to 78 appear
together only in a generic jump table of the D3D layer (`0x82836148`). It needs the per-draw
constants read next to the draw's material, with the game running.

The viewer now adds the hemisphere term (`SceneLights::ambient_up`, the sunset set's P2; the
match sets were read without it), takes the specular colour as the light's own and the way to
the camera instead of an infinite viewer. It still multiplies by the material's colours as for
DOA2U, which may count them twice.

`DirLightVec` is the way the light travels in Direct3D's camera space (z into the screen): the
strongest light of each set comes down and away from the camera, lighting faces from the front
and above whatever the camera's angle. The viewer turns them with its own camera
(`set_lights_follow_camera`).

A draw's colours sit at the offset in its +0x20 (0x80 in all read): diffuse, ambient, specular
(vec4 each), then the power (1, 0.5, (0.3, 0.2, 0.2), 15 on skin; heads vary, power 50 on some
parts). Read at a fixed +0x70 at first, the heads' diffuse came out zero and the game lights
looked far too dark, as the user saw. The viewer lights DOAX2 like DOA2U's materials with the
sets, plus the hemisphere ambient and the specular above. "Game lights" in the Shading window
switches to the viewer's plain shading. Not compared with the game.

## Faces

A head CAT's word 3 is the offset of an `MPV` block of face shape sets, read like the hands'
(`hand_poses`), with two differences: list entries carry the vertex layout in bits 24 to 30
(the head has three layouts of strides 32, 20 and 24), and a face pose is 12 bytes a group,
the position alone. Each head has two sets on object 0:

- Eyes, 4 entries: 0 the lists, 1 the base (open, the mesh exactly), 2 shut, 3 shut smiling.
- Mouth, 8 entries: 1 the base, then A, I, U, E, O and a closed smile (seen on Tina).

No working buffer holds them in the running game (only the file's copies were found): the
blend is done on the GPU, so the weights in use cannot be read from memory.

A CPU blend of seven shapes exists in the code, though (read 2026-11-10, not run against the
game): the second half of `0x824969F0`, called on a `CLOTH_DATA` whose +0x58 is set (0 in the
files, so filled at run time). `[+0x58]+0x24` points to a record with seven arrays of 12-byte
positions at +0x1C to +0x34 (the mouth set has seven shapes: base, A, I, U, E, O, smile), the
vertex list at +0x18 (ended by -1, a set top bit starting each group, as in the `MPV` lists)
and the layouts at +0xC. Per group, with x the first array's x:

    left  = clamp((x + 0.03) / 0.06, 0, 1)      right = clamp((0.03 - x) / 0.06, 0, 1)
    vertex = left * sum(wl[i] * shape[i]) + right * sum(wr[i] * shape[i])

`wl` and `wr` are two arrays of seven floats, pointed to by the owner's +0xD0 and +0xD4 (the
owner is the cloth's +0x4C). So each half of the face has its own weights, with a soft seam 6 cm
wide. Whoever fills those two arrays is the mouth's (or the eyes') trigger: the next thing to
read for facial animation. It also means the "no working buffer" above may only be true of the
scenes looked at.

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
7. `0x824970D8` places the `FIX_GRID` particles (each entry's position through its bone's matrix,
   into the particle's state) and `0x82497450` runs `0x8249A100` over every particle; neither
   writes vertices, as first noted here. The vertices are written by `0x824974F8` (see "Who
   writes the hair's vertices").

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

### In the viewer (2026-11-10)

`src/formats/doax2_cloth.rs` reads the cloths and steps them; the viewer runs them on the head
for heads with cloth (Lei Fang's `AA` among the characters it shows), 30 steps a second at the
file's values in the game's space, the particles carried with the head between steps. Read
from the file this time:

- `GRID_WRITE` is a stream from +0x60: 0x80000000 | particle, then the vertices it writes
  (layout << 24 | byte offset), one or more; +0x20 counts the particles, +0x24 the vertices.
  Kasumi's `AB` writes two vertices for some particles.
- `FIX_GRID` names its particle by offset (as edges do), not by number.
- `TRIANGLE`: per particle, entries of 8 bytes, the other two corners as offsets. The viewer
  takes each particle's normal from them, for the air term.
- `THREAD_DATA`: records of 0x18 from +0x30 (type, size, entries' offset, count, angle limits'
  offset, 1); entries of 12 bytes, the particle first, root to tip (16 threads on Lei Fang's
  `long_l`, one per fixed particle).
- Colliders: the third word is the next record's offset from the chain's start. Records with
  the flag at +0x14 have at +0x18 the offset of a later record in the chain (or 0): Lei Fang's
  head sphere (15.5 cm) points past the head and neck colliders, the chest box past the chest
  and collars, each upper arm's capsule past that arm's colliders. They read as bounding
  volumes; the viewer skips the records they guard for particles outside them and does not
  collide with them.
- Each collider's work area (the pointer in its record) holds at +0x00 the matrix from its unit
  shape to the world (rows the shape's axes scaled by its sizes, translation its centre) and at
  +0x40 its inverse, rebuilt each frame (read on Kasumi's `AA`, 2026-11-10; `+0x100`, which the
  capture tool used to read, is near the identity and not the world matrix). From them:
  - Type 1, a capsule: from its start along a vector (-x on the right arm), the radius in w.
    The unit matrix is centred on the segment's middle with half its length.
  - Type 8, a box centred at +0x30: y is the axis at +0x40 times the half height at +0x64 (Lei
    Fang's chest box writes (0, 0.2, 0) and 1, Kasumi's (0, 1, 0) and 0.2), z the front at
    +0x50 made square to y times +0x68, x across times +0x60. A tilted box (axis (0, 0.06,
    -0.13), half 0.5) matched its matrix exactly.
- In that read, no particle was inside any unflagged collider except the head's ellipsoid,
  which held 65 free particles from rows 1 to 7, at 84 to 100 percent of its size; the
  flagged ones held many (540 in the head's sphere, 80 in the chest's box), as bounding
  volumes would. So the game does not push out particles that are already inside: it tests
  the move (the "segment" path of `0x82495030`).

The viewer's collision: a particle whose move enters a collider stops where the move crosses
its surface; one already inside may stay but no deeper than it sat at rest (the viewer's rule,
to keep the roots from sinking into the head; what the game does after a hit, sliding or the
move's rest, is not read).

A second read with Kasumi sitting up from a deck chair (1900 states in 12 s,
`target/captures/doax2_hair_kasumi_sit.npz`) puts hundreds of particles exactly on the surface
(within 0.1 percent) of every unflagged collider, boxes (158, 141) and the capsule (853)
included, which confirms those shapes. While she moves, many particles are also well inside:
down to 0.6 of the boxes' size, 0.74 of the capsule's, 0.85 of the head's ellipsoid. The game
only tests the particle's own move, so a body moving into the hair leaves it inside; the
viewer's "no deeper than at rest" is stricter than that. Consecutive states are never one step
apart in this read (the carry between steps moves the previous position too), so the response
to a hit could not be isolated. Pushing to the nearest surface point instead tore the locks by the
shoulders apart.

Angle limits (`0x82499B40`, read 2026-11-10; `AngleLimit` in `doax2_cloth.rs`): a thread record's
+0x10 is the offset of a list of limit records (0x40 bytes), +0x14 their number. A record: type
(1 applies), +0x10 first angle, +0x14 links at it, +0x18 step per later link (cumulative), +0x1C
max, +0x20 links, +0x24 first entry, +0x28 stride, +0x2C CAT bone, +0x30 factor. Lei Fang's
first: 15, 15, 30, 45, 60, 75 degrees over six links, factor 0.9. A link bent from the one before
(the first from its rest direction turned by the bone) beyond its limit is turned back to it,
and the child and every later entry move by the factor times the change. Applied in the viewer
after the thread caps.

Checked against the capture of Kasumi sitting up (`doax2_hair_kasumi_sit.npz`, all 66 threads of
`vksAA`, type 1 with one limit each): on the second link, 70 percent of the states sit 0 to 2
degrees over the limit and 18 percent below it, as a correction of 0.9 of the excess leaves
them; over all links 3.8 percent of the states are more than 2 degrees over (up to 48), where
collisions and the caps move the particles afterwards. By eye on Lei Fang and Kasumi the hair
hangs as before; a small dark lock still shows in front of Lei Fang's shoulder. The hair's
other vertices stay with the head (see below). Not compared with the game in motion; the
export does not carry the hair's motion.

### Who writes the hair's vertices (2026-11-10)

`0x824974F8` (r3 the `CLOTH_DATA`, a matrix in r6 to r10) is the only writer of the particles'
vertices: it walks the `GRID_WRITE` stream (a chain through +0x28, 0 in every file looked at),
and for each vertex after a `0x80000000 | particle` word stores a matrix times the state's
position (+0x00) and normal (+0x20) into the locked vertex buffer of its layout (the normal
packed or as floats, by layout). So the buffers hold head-space positions; a search of guest
memory for the particles' world positions finds only the states and their mirror at
`0xD8...`.

The rest of the hair is not written by anything. The ignored test `layout_two_writers` lists,
per head, who writes each vertex and how the triangles mix. Layout 2 is a vertex format
(stride 24), not "the hair": Tina's head, with no cloth, has 2126 vertices in it, 1008 of them
in face shape lists. On the hair materials of `vksAA`, `vksAB` and `vleAA`:

- Triangles are all particles (1048 on `vksAA`'s main material), none (1364), or mixed (56).
- In every mixed triangle the particle corners are fixed particles (row 0): no free particle
  shares a triangle with a vertex that is not a particle.
- The particles reach 30 to 38 cm below the head's origin; the other vertices stay above
  y 0.02 (scalp and fringe).

So the hair's other vertices are rigid with the head, joined to the locks only at their fixed
roots. The viewer had them follow their nearest particle (up to 11 cm away), which is now
removed. `vksAA` also has 14 small layout 2 materials with no particles (vertices 2650 to 3367,
56 to 84 triangles each); an early note said vertices 2844 to 3367 move in the game, which is
not explained: no list found so far names them. The head CAT's blocks tagged 0x06010001 from
0x1680 (two values 0.3 and 0.8, a point behind the head, offsets) are still not decoded and may
be theirs.

`0x82496F88` and the start of `0x824969F0` use `FIX_GRID`'s own list (+0x28, a word per entry,
-1 for none; only when +0x10 is 2 or more, +0x14 is 0x30 and +0x20 has bit 0): entries without
bit 0 at +0x18 copy their position to a vertex, or read it back from one.
