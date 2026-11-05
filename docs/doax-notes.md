# DOAX notes

Dead or Alive Xtreme Beach Volleyball. Data read: `assets/doax/` (`default.xbe`, `datahdd.afs`, `datadvd.afs`), ignored by git, copied from the install's `private/setup-b54dcdf118d4/imported-disc/disc`. Generated C: `private/setup-5a3ce4c907d9/generated`. The port's own `docs/` describe body joints, body morphs, the skeleton and swimsuits.

Status of each statement below: "live" was measured on the running game, "file" was read from the data, "code" from the C or the port's docs, "guess" is not checked.

## Files

- Bodies are `v<girl>NN.xpr` and `.cat` in `datadvd.afs` (`vtn00`, `vtn01`). Swimsuits and other worn models are `md<owner>NN<wearer>.xpr` and `.cat` in `datahdd.afs`, with `.itm` beside some. `mdTn00ht` is suit `Tn00` for wearer `ht` (guess, from the counts per suffix). Motions are `<name>.mot` and `m_<name>.mot` in `datadvd.afs`. (file)
- XPR: `MDL\0` at +0x14, object count at +0x24 and offsets from +0x28, as DOA2U. Not encrypted: `OBJ\0` and vertices read plainly, though the first word is not the magic and +4 is not the file's size. Objects read like DOA2U's: +0x04 extra floats per vertex, +0x20 vertices, +0x24 their offset, +0x28 indices, +0x2C their offset, material blocks from +0xA0. (file)
- Objects 40 to 58 of `vtn00` span -1 in z: probably shadow volumes (guess). Objects 0 to 39 are in their bone's space. (file)
- CAT: +4 is the base, 0x14. Records from the base are (kind, value), the other way round from DOA2U, ending with kind 0. Kind 1, value + base: 8 byte (count, offset) pairs. Kind 3: material colors. Kind 4: a list, read next. (file)
- Kind 4 on a body: 10 header bytes, then triples of (kind, slot, object). Kind 1 is rigid on bone `slot`, where 0x80 is a flag; kind 6 and kind 2 are other piece kinds; 2 goes with objects that have extra floats. Two-byte `0A n` entries divide it into groups. Meaning of the header, of the groups and of kinds 2 and 6 is not read. (file, guess on meaning)
- A kind 4 list also has `0B`, one byte, dividing it further. Slots from 0x80 are bones 24 and up. Kind 2 slots come from the table at `0x33F988`, 16 bytes a slot, as DOA2U's at `0x64C8B0`: a count, then lists ended by 0xFF, each a bone followed by 0x80 + a bone to add that bone's offset or 0x40 + a bone to take it away. A piece's vertices hold one extra float less than the count, a weight per list, and the last list takes what is left. (file; the weights' meaning fits the pieces' shapes) Slot 15, the neck, counts 4 lists but holds 3. Kind 6 pieces are joints, which the records at CAT word 4 describe.
- Group 0 holds feet in two pieces with toes, and group 2 whole feet. The viewer draws group 2. Which one the game draws, and when, is not read. (guess)
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

The values start 0xC into the block at `0x5E06A0 + n * 0x5C`, and the layout follows DOA3's where it can: 1 height, 3 to 5 hips, 6 to 8 chest, 9 to 11 head, limbs of 8 from 12, 20, 28 and 36, 44 to 46 waist (bone 23), 47 to 49 neck (19), 50, 51 one collar (20), 52, 53 the other (16), 56 to 59 toes. (live)

Measured from those values against the game's matrices, `solve_game_space`, test `doax_pose_matches_capture` with `tools/re/doax_capture.py`. Hips, ankles and wrists are within 0.3 mm in the good samples. Bones 23, 0, 19, 1, 16 and 20 fit their formulas, to 0.2 mm where the bone table was right. Limbs are DOA3's solve followed by a fixed change of axes, which was the same on all 4 characters. (live) Elbow roll is approximate, half of the hand's, within 0.08 radians. Bones 26, 27, 30 and 31, which roll along the upper arm, and 28 and 29 are not worked out: they sit on their base bone, and only 30 and 31 carry a piece. The later samples showed bigger errors on arms and legs, up to 0.2 in a matrix element, not explained: another scene started between them and the first.

The evaluator is confirmed by `doax_tracks_match_capture`: two characters of a demo were found playing `tina.mot` 77, from frame 187 to 194, and `gokei.mot` 77, from 566 to 573, across two samples 0.2 s apart. That also confirms `gokei` is Christie's. Between 14 and 31 of 57 tracks stayed more than 0.003 off. Tracks 23 and 25 differ by pi on one hand and so may be another form of the same rotation, 9 to 11 and 47 to 49 are the head and neck, which the game turns itself, and 0 and 2 are the position, which the viewer leaves out. Not all of the others were explained. (live)

A table starts four rows before `0x2CFD58 + table * 0x180`, with its rows 20 to 23. (live)

Joint pieces read with DOA2U's parser, with each record's own parent, and kind 2 pieces blend by the slot table: not compared with the game. Not done: morphs; soft bodies; mirrored motions; blending; hands.

## In the viewer

`src/formats/doax.rs`, `--doax`, and the DOAX button. A body with a suit and motions, for 5 characters. The up and down keys walk the costume list, for any game. A suit's pieces replace the body's hips and chest, and its waist when the suit has one, inferred from their sizes. Of the body's chest piece, only the triangles above 0.17 in bone space or on the back are drawn, for the shoulders, back and upper chest around the suit's top. The game was not seen to cut it: what it really draws under a suit is not read, and should be checked in `sub_00064300`'s area or by a capture. The viewer never shows a body without a suit.

Not done: morphs, soft bodies, `.itm` items, suits' objects not in their list, the second body file (`vtn01`), export. Lights use DOA2U's list as a stand-in. Which bone table is whose is inferred from piece sizes (`ROSTER`). Nothing has been compared with the game's picture.

## Limits

Kasumi, Ayane and Hitomi are left out of the viewer's DOAX roster, as is anything built on them, such as exports. DOAX shows its characters almost only in swimwear and the project treats these three as teenagers or young. Claude's decision; it told the user. `CLAUDE.md`'s limit on soft bodies also applies. Validate on Tina, Helena, Lei Fang, Christie and Lisa, and use full body views at a distance.
