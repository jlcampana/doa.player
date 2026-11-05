# DOAX notes

Dead or Alive Xtreme Beach Volleyball. Data read: `assets/doax/` (`default.xbe`, `datahdd.afs`, `datadvd.afs`), ignored by git, copied from the install's `private/setup-b54dcdf118d4/imported-disc/disc`. Generated C: `private/setup-5a3ce4c907d9/generated`. The port's own `docs/` describe body joints, body morphs, the skeleton and swimsuits.

Status of each statement below: "live" was measured on the running game, "file" was read from the data, "code" from the C or the port's docs, "guess" is not checked.

## Files

- Bodies are `v<girl>NN.xpr` and `.cat` in `datadvd.afs` (`vtn00`, `vtn01`). Swimsuits and other worn models are `md<owner>NN<wearer>.xpr` and `.cat` in `datahdd.afs`, with `.itm` beside some. `mdTn00ht` is suit `Tn00` in the fit `ht`. Five fits hold nearly all suits: `tn`, `cr`, `ls`, `ht`, `ay`. Bodies of the same chest size share a fit: Helena's is Lisa's, and Lei Fang's is that of `ht`. `he` and `le` add a few of their own. (file for the sizes; that this is how the game picks them is inferred, as the table of who wears what at `0x3003E8` was not followed) Motions are `<name>.mot` and `m_<name>.mot` in `datadvd.afs`. (file)
- XPR: `MDL\0` at +0x14, object count at +0x24 and offsets from +0x28, as DOA2U. Not encrypted: `OBJ\0` and vertices read plainly, though the first word is not the magic and +4 is not the file's size. Objects read like DOA2U's: +0x04 extra floats per vertex, +0x20 vertices, +0x24 their offset, +0x28 indices, +0x2C their offset, material blocks from +0xA0. (file)
- Objects 40 to 58 of `vtn00` span -1 in z: probably shadow volumes (guess). Objects 0 to 39 are in their bone's space. (file)
- CAT: +4 is the base, 0x14. Records from the base are (kind, value), the other way round from DOA2U, ending with kind 0. Kind 1, value + base: 8 byte (count, offset) pairs. Kind 3: material colors. Kind 4: a list, read next. (file)
- Kind 4 on a body: 10 header bytes, then triples of (kind, slot, object). Kind 1 is rigid on bone `slot`, where 0x80 is a flag; kind 6 and kind 2 are other piece kinds; 2 goes with objects that have extra floats. Two-byte `0A n` entries divide it into groups. Meaning of the header, of the groups and of kinds 2 and 6 is not read. (file, guess on meaning)
- A kind 4 list also has `0B`, one byte, dividing it further. Slots from 0x80 are bones 24 and up. Kind 2 slots come from the table at `0x33F988`, 16 bytes a slot, as DOA2U's at `0x64C8B0`: a count, then lists ended by 0xFF, each a bone followed by 0x80 + a bone to add that bone's offset or 0x40 + a bone to take it away. A piece's vertices hold one extra float less than the count, a weight per list, and the last list takes what is left. (file; the weights' meaning fits the pieces' shapes) Slot 15, the neck, counts 4 lists but holds 3. Kind 6 pieces are joints, which the records at CAT word 4 describe.
- `0x000633E0` walks the list. `0A n` draws what follows only when the girl's mode byte (`0x4C9744`) is n, up to the next `0A` or `0B`. On a body, mode 0 has feet in two pieces with toes, and mode 2 whole feet. The viewer uses 0. What sets the mode was not read, and the port's docs say mode 1 also exists. (code)
- `.mot`: a table of 1000 words, 0xFA4 for unused IDs. A motion's block is a word, frames in the low half and 60 tracks in the high half, then 60 track offsets relative to the block. Keys read as DOA3's: the sizes walk exactly to each track's end on the tracks tried. (file)

## Skeleton

`0x4D3650 + n * 0x800` holds each character's 32 row-vector world matrices, 64 bytes each (`0x000B01E0` builds them). `tools/re/doax_live.py bones` prints them. (code, live)

Bone offsets are 24 rows of 16 bytes per table, tables 0x180 apart from `0x2CFD58`, as (x, y, z, 0). Eight tables fit; which girl has which is not settled. Matching live matrices to them, parent by parent, under 0.3 mm (live, on 4 characters of one scene):

| Bone | Parent | Row |
|---|---|---|
| 2 | hips, no match | |
| 23, 0, 19, 1 | 2, 23, 0, 19 | 19, 20, 15, 21 |
| 16, 8, 4, 5 | 0, 16, 8, 4 | 12, 4, 0, 1 |
| 20, 14, 10, 11 | 0, 20, 14, 10 | 16, 10, 6, 7 |
| 6, 7, 3, 18 | 2, 6, 7, 3 | 2, 3, 5, 14 |
| 12, 13, 9, 22 | 2, 12, 13, 9 | 8, 9, 5, 18 |
| 17, 21 | 1, 1 | 13, 17 |

Where two rows hold the same value, the row is taken by symmetry, not measured. Bones 24 to 31 sit on 4, 10, 8, 14, 5, 11, 8, 14 and turn with them, so they look like twists. Bone 15 sits on the ground under the hips.

## Motions

`.mot` keys are not DOA3's, though they look alike. Values are packed floats (`0x000AE980`): 20 bits, the head's top four and the next 16, with sign bit 19, exponent `((v >> 16) & 7) + 0x7E` and mantissa `(v & 0xFFFF) << 7`. Slopes are 24 bits (`0x000AE9F0`) or 16 (`0x000AE910`). With an exponent field of 0, a value is its low 16 bits over 65536, and slopes use a smaller range with their own exponent base. Kinds by the table at `0x2D1248`: 0 cubic, size 10; 1 line, 4; 2 parabola, 6; 3 constant, 0. (code) Tracks are in memory's order, unless the motion is mirrored, by the table at `0x2D1360`. The viewer does not play mirrored ones. (code)

The values start 0xC into the block at `0x5E06A0 + n * 0x5C`, and the layout follows DOA3's where it can: 1 height, 3 to 5 hips, 6 to 8 chest, 9 to 11 head, limbs of 8 from 12, 20, 28 and 36, 44 to 46 waist (bone 23), 47 to 49 neck (19), 50, 51 one collar (20), 52, 53 the other (16), 54 the toes of bone 22, 55 those of bone 18. Tracks 56 to 59 were zero in the samples. (live)

Measured from those values against the game's matrices, `solve_game_space`, test `doax_pose_matches_capture` with `tools/re/doax_capture.py`. Hips, ankles and wrists are within 0.3 mm in the good samples. Bones 23, 0, 19, 1, 16 and 20 fit their formulas, to 0.2 mm where the bone table was right. Limbs are DOA3's solve followed by a fixed change of axes, which was the same on all 4 characters. (live) Arm roll follows `0x000B17D0`: bone 26 is the collar's frame turned by the shortest arc onto the upper arm's x axis, 30 is halfway from the upper arm to 26, 28 is the hand's frame turned onto the forearm, the forearm becomes halfway to 28, and 24 copies it; the other side likewise. On a usable scene, positions of the bones checked were exact to 0.001 and the trunk and legs within 1.6 mm in a matrix element, while arm rotations were about 0.03 radians off in one capture and reached 0.19 in a matrix element on bones 8, 10 and 14 in another, and 0.54 on 28 and 29, not explained. `0x0017C780` builds the relative rotation, takes its axis and angle and scales the angle, which equals a slerp, so it is not the cause. In a later capture bone 26's x axis was 0.998 along bone 8's, not exactly along it, so 26 does not come from the frames as the viewer builds it, though the arc's direction and side were checked: other orders give errors of 20 to 130 degrees, this one about 5 on 26 and 2.3 on 30. The branch on `0x4CB648` was not followed. Some scenes do not drive the matrices from these tracks, and the test skips their samples.

Angles change form inside motions: keys go from 6.27 to 3.14 by a line of one frame, with the bone's other angles moving too, to an equal or nearly equal rotation (`tina.mot` 210, frame 21). The viewer plays fractions of a frame, so it holds such a line's first value, and takes the short way round between other keys. How the game treats fractions was not read. `doax_motions_turn_smoothly` prints the biggest turns: over half frames, `tina.mot` went from 3.14 to 0.86 radians at most, and `comon.mot` still reaches 2.97 on hands, in motions such as 646, not explained.

The evaluator is confirmed by `doax_tracks_match_capture`: two characters of a demo were found playing `tina.mot` 77, from frame 187 to 194, and `gokei.mot` 77, from 566 to 573, across two samples 0.2 s apart. That also confirms `gokei` is Christie's. Between 14 and 31 of 57 tracks stayed more than 0.003 off. Tracks 23 and 25 differ by pi on one hand and so may be another form of the same rotation, 9 to 11 and 47 to 49 are the head and neck, which the game turns itself, and 0 and 2 are the position, which the viewer leaves out. Not all of the others were explained. (live)

A table starts four rows before `0x2CFD58 + table * 0x180`, with its rows 20 to 23. (live)

Joint pieces read with DOA2U's parser, with each record's own parent, and kind 2 pieces blend by the slot table: not compared with the game. Hands: the first two morph sets of the `apv` section at CAT word 3, on objects 30 and 12, with 34 shapes and no normals, read by DOA2U's reader. Scripts are in `<name>.bin`, and for shared motions in `anime_cm.bin`, whose motion numbers have 0x2000 added: a table of 1000 offsets at word 0, then words of an operation and its arguments, sizes by the table at `0x2CF888`. 0 plays a motion, 1 waits for a frame, 12 and 13 set a hand (0, 1, or 2 for both) to a pose, 13 over some frames. `0x33EE38` gives each pose's shape: 0, 1, 4, 5, 17, 14, 8, 11. (code) Live, the poses' rest shapes matched that table, 0 to 5. The viewer blends straight from shape to shape, by frame. The game passes through shapes in between on some changes, seen live, and it adds a step for scenes through `0x5D8AA8`'s records, neither of which the viewer does. Not compared frame by frame. Not done: morphs other than hands; soft bodies; mirrored motions; blending; hands.

## In the viewer

`src/formats/doax.rs`, `--doax`, and the DOAX button. A body with a suit and motions, for 5 characters. The up and down keys walk the costume list, for any game. The body's chest, hips and waist are in the common part of the list, so the game draws them whatever the suit, with the suit over them, and so does the viewer. (code) The viewer offers no way of removing the suit.

Not done: morphs, soft bodies, `.itm` items, suits' objects not in their list, the second body file (`vtn01`), export. Which bone table is whose is inferred from piece sizes (`ROSTER`). Nothing has been compared with the game's picture.

## Lights

D3D's fixed function lights, as DOA2U: `tools/re/doax_lights.py` lists the ones enabled, from the list at device `+0x398`, device pointer at `0x1F2978`, ambient at `0x1F2D54`. Live: 4 directional lights, specular 0.3 of diffuse. The sets are in the executable, records of 0x58 bytes around `0x287000` to `0x2C0000`, listed by `tools/re/doax_stage_lights.py`, and stage functions such as `0x00177800` name them by address. Each run has the stage's four, with points, then four directional: one live set, of (0.9, 0.88, 0.88), matches there in color, direction and ambient, in test `table_holds_a_set_read_from_the_game`. Other live sets were not found in that form, and some have a direction the table's does not give, so some come from elsewhere, or change at run time.

The viewer's list has the three sets most often seen in a demo first, by hand, then the table's. Which set is a character's during a frame was not settled: a check on the world transform passes nearly always, so it told nothing. Which set belongs to which stage and time of day is not known. Sets are not tied to a stage in the viewer. Not done: stages themselves, sun shadows, fog. Not compared with the game's picture.

## Limits

Kasumi, Ayane and Hitomi are left out of the viewer's DOAX roster, as is anything built on them, such as exports. DOAX shows its characters almost only in swimwear and the project treats these three as teenagers or young. Claude's decision; it told the user. `CLAUDE.md`'s limit on soft bodies also applies. Validate on Tina, Helena, Lei Fang, Christie and Lisa, and use full body views at a distance.
