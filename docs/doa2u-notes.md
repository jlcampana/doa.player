# DOA2U: what is known

Function addresses are from the generated C in `private/program/runtime-sources` of the local doa2u-re install. `src/formats/doa2u.rs` holds the code.

## Archives

In `private/play-disc/DOA2/` of the install. AFS entries have names.

| File | Contents |
| --- | --- |
| `datahdd.afs` | Motions (`zack.mot`, `m_zack.mot`, `c_zack.mot`, `comon.mot`), `.bin` files, plain stage XPR files, menus |
| `datahdd2.afs` | Encrypted costumes: `kas00.xpr` and `kas00.cat`. Numbers have gaps, and placeholder entries repeat a name such as `kor00` |
| `datadvd.afs` | More encrypted costumes (`kas20` and on), `char_doa3` models and plain stage XPR files |

The viewer lists only `datahdd2.afs` costumes, up to the select screen's count for each character (table at `0x6E0F08`).

## Encryption

`sub_00289E40` (XPR), `sub_00289DE0` (CAT), `sub_00289DA0` (stream). The XPR and CAT of a costume share a seed.

- Generator: standard MT19937 (`sub_0001FD20`, `sub_0001FEF0`). Each stream word is its output plus `0x31415926`, XORed into the file.
- XPR: seed = word 0 XOR `XPR0`. The stream runs from byte 12 for word 1 minus 12 bytes.
- CAT: seed = word 0 XOR `0x20`. The stream runs from byte 0x14, for as many bytes as the first non-zero of words 4, 3, 2, 1.

All 268 encrypted XPR files decrypt and parse.

## Models

The XPR has DOA3's outer layout. Records differ: see the top of `doa2u.rs`. Word +4 of `OBJ` gives extra floats per vertex: 1 on most weighted objects, 2 or 3 on a single costume's objects, which are not handled.

CAT words 1 to 4 are section offsets:

- Word 1: `(value, type)` records, read by `sub_0005B9F0`. Value is relative to the section. It ends at type 0.
  - Type 1: the draw list, used here. Type 9 has the same form with low detail objects. Types 3, 5, 6, 7, 10, 11, 12 and 18 are not read yet.
  - Draw list: three bytes per object, kind, slot, object. Kind 1 is one matrix (`sub_0005AF30`). Kind 2 blends matrices by vertex weight (`sub_0005B940`, table at `0x64C8B0`).
  - Slots below 0x80 are matrices. 0x80 and up go through the table at `0x64C9B0`: 128 and 129 are 24 and 25.
- Word 4: the same node table as DOA3, with joint pieces. An entry of 1 means no piece (`sub_000639D0`).
- Words 2 and 3 (`sub_00060360`, and the section read at `0x87CFE8`) are not read yet.

## Bones and motions

- `.mot` files use DOA3's track format.
- Bones: pointers at `0x6978E0`, by character ID, 24 rows of vec4. Rows 0 to 14 mean what they do in DOA3. The eyes are rows 17 and 21, and matrices 17 and 21.
- Character IDs in `app.rs` were worked out from the select screen counts and the order of file names at `0x694288`. Not checked against the running game.

## Not done

- Pose is not right yet. Characters look correct above the waist, but knees splay outward. The likely cause is that the leg solve is DOA3's, not read from this game. Compare against the bones of the running game.
- Nothing has been compared with the running game.
- Hair, cloth, face and hand morphs, translucent layers, and the costumes in `datadvd.afs`.
- The back-face flag is taken to be the same as DOA3's; the same word also holds 5 and 8 here.
- No chest spring: it is not ported for DOA2U.
