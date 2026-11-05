# DOAX notes

Dead or Alive Xtreme Beach Volleyball. Data read: `assets/doax/` (`default.xbe`, `datahdd.afs`, `datadvd.afs`), ignored by git, copied from the install's `private/setup-b54dcdf118d4/imported-disc/disc`. Generated C: `private/setup-5a3ce4c907d9/generated`. The port's own `docs/` describe body joints, body morphs, the skeleton and swimsuits.

Status of each statement below: "live" was measured on the running game, "file" was read from the data, "code" from the C or the port's docs, "guess" is not checked.

## Files

- Bodies are `v<girl>NN.xpr` and `.cat` in `datadvd.afs` (`vtn00`, `vtn01`). Swimsuits and other worn models are `md<owner>NN<wearer>.xpr` and `.cat` in `datahdd.afs`, with `.itm` beside some. `mdTn00ht` is suit `Tn00` for wearer `ht` (guess, from the counts per suffix). Motions are `<name>.mot` and `m_<name>.mot` in `datadvd.afs`. (file)
- XPR: `MDL\0` at +0x14, object count at +0x24 and offsets from +0x28, as DOA2U. Not encrypted: `OBJ\0` and vertices read plainly, though the first word is not the magic and +4 is not the file's size. Objects read like DOA2U's: +0x04 extra floats per vertex, +0x20 vertices, +0x24 their offset, +0x28 indices, +0x2C their offset, material blocks from +0xA0. (file)
- Objects 40 to 58 of `vtn00` span -1 in z: probably shadow volumes (guess). Objects 0 to 39 are in their bone's space. (file)
- CAT: +4 is the base, 0x14. Records from the base are (kind, value), the other way round from DOA2U, ending with kind 0. Kind 1, value + base: 8 byte (count, offset) pairs. Kind 3: material colors. Kind 4: a list, read next. (file)
- Kind 4 on a body: 10 header bytes, then triples of (kind, slot, object). Kind 1 is rigid on bone `slot`, where 0x80 is a flag; kind 6 and kind 2 are other piece kinds; 2 goes with objects that have extra floats. Two-byte `0A n` entries divide it into groups. Meaning of the header, of the groups and of kinds 2 and 6 is not read. (file, guess on meaning)
- `.mot`: a table of 1000 words, 0xFA4 for unused IDs. A motion's block holds a word, then ascending track offsets, like DOA3's but with a leading word, so the DOA3 reader may not fit as it is. (file)

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
| 12, 13, 9, 22 | 2, 12, 13, 9 | 8, 9, 11, 18 |
| 17, 21 | 1, 1 | 13, 17 |

Where two rows hold the same value, the row is taken by symmetry, not measured. Bones 24 to 31 sit on 4, 10, 8, 14, 5, 11, 8, 14 and turn with them, so they look like twists. Bone 15 sits on the ground under the hips.

## Limits

Kasumi, Ayane and Hitomi are left out of the viewer's DOAX roster, as is anything built on them, such as exports. DOAX shows its characters almost only in swimwear and the project treats these three as teenagers or young. Claude's decision; it told the user. `CLAUDE.md`'s limit on soft bodies also applies. Validate on Tina, Helena, Lei Fang, Christie and Lisa, and use full body views at a distance.
