# DOA2U: what is known

Function addresses are from the generated C in `private/program/runtime-sources` of the local doa2u-re install. `src/formats/doa2u.rs` holds the code.

## Archives

In `private/play-disc/DOA2/` of the install. AFS entries have names.

| File | Contents |
| --- | --- |
| `datahdd.afs` | Motions (`zack.mot`, `m_zack.mot`, `c_zack.mot`, `comon.mot`), `.bin` files, plain stage XPR files, menus |
| `datahdd2.afs` | Encrypted costumes: `kas00.xpr` and `kas00.cat`. Numbers have gaps, and placeholder entries repeat a name such as `kor00` |
| `datadvd.afs` | More encrypted costumes (`kas20` and on), `char_doa3` models and plain stage XPR files |

The viewer currently lists only `datahdd2.afs` costumes, up to the select screen's count for each character (table at `0x6E0F08`). This is not the complete costume set: `datadvd.afs` also contains additional encrypted costumes (e.g. `kas20` and onwards). Both archives should be explored and supported when enumerating available costumes.

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
  - Type 1: the draw list, used here. Type 9 has the same form with low detail objects. Type -1 is a version, used for the forearm twist. Type 10 sets upper arm flags. Types 3, 5, 6, 7, 11, 12 and 18 are not read yet.
  - The switch is at `0x5BBAC`, indexed by type + 1.
  - Draw list: three bytes per object, kind, slot, object. Kind 1 is one matrix (`sub_0005AF30`). Kind 2 blends matrices by vertex weight (`sub_0005B940`, table at `0x64C8B0`).
  - Slots below 0x80 are matrices. 0x80 and up go through the table at `0x64C9B0`: 128 and 129 are 24 and 25.
- Word 4: the same node table as DOA3, with joint pieces. An entry of 1 means no piece (`sub_000639D0`).
- Words 2 and 3 (`sub_00060360`, and the section read at `0x87CFE8`) are not read yet.

## Bones and motions

- `.mot` files use DOA3's track format.
- Bones: pointers at `0x6978E0`, by character ID, 24 rows of vec4. Rows 0 to 14 mean what they do in DOA3. The eyes are rows 17 and 21, and matrices 17 and 21.
- Character IDs in `app.rs` were worked out from the select screen counts and the order of file names at `0x694288`. Not checked against the running game.

## Pose

`sub_000E9430` builds the pelvis and chest and calls the limb solve, `sub_000EB880`. Lengths are set up by `sub_000EB5B0` and poles by `sub_000E88E0`. Fighter data: matrices at `0x875C20` + fighter * `0x800`, 32 of them; tracks through the pointer at `0x8A7550`; limb structs at `0x8A77C0` + fighter * `0x1D0`, stride `0x74`.

The solve is DOA3's in outline, with these differences:

- It runs with bones along Z, then applies quarter turns from the table at `0x699600` to the stored upper and mid matrices. Leg bones end up along -Y and arm bones along X. This was the leg bug: the viewer used DOA3's solve without the turns, so the meshes and the bends between thigh and shin used wrong frames.
- The angles are limited: a cosine of 1 or more gives 0, and one under `0.00097` gives a quarter turn. The same constant is added to the squared distance.
- Hands and feet take their turn from matrix 15, the fighter's root, then the tracks. Feet also follow the floor's slope (`sub_00145240`).
- Forearm twist, matrices 24 and 25: chosen by the CAT's record of type -1, a version.
  - 2 or more (267 costumes): the hand's frame, turned so its X meets the forearm's, at the elbow.
  - Under 2 or absent (52 costumes): the forearm rolled by `atan2`, as in DOA3. Not compared with the game.
  - One file gives 119. It may not be a costume; not looked at.
- Matrices 28 and 29 are the hand ends of the twists. They are computed by the game and not by the viewer.

### Measurements

Against frames of a CPU match, with both fighters (IDs 0 and 24):

| Check | Worst error |
| --- | --- |
| Legs: matrices 6, 7, 12, 13 | 0.003 |
| Arms: matrices 4, 8, 10, 14 | 0.012 |
| Hands: matrices 5, 11 | 0.002 |
| Twists: matrices 24, 25 | 0.003 |
| `eval_motion` against the game's tracks | 0.0001 |

The last line comes from `captured_tracks_are_found_in_motions`: 13 of 20 frames of Zack were found in `zack.mot` or `comon.mot`. Frames not found are taken to be blends between motions, which was not checked. A read can also catch the tracks one frame ahead of the matrices, so `doa2u_live.py capture` drops frames whose pelvis does not match its tracks.

Fighter ID 24 matched 13 frames in `sambo.mot` and 10 in `bayman.mot`; motion 184 was found only in `sambo.mot`. So that ID uses the file the viewer calls Leon.

## Not done

- Not ported from the pose: the head's `sub_000EDBF0`, feet on slopes, the scale at `0x8A73B4`, and the fighter's root turn.
- CAT record type 10 (`0x10100` in 74 costumes) sets flags at `0x850710` that blend upper arms 8 and 14 with the chest's frame (`sub_00285B40`, factor 0.3). Not ported, and both captured fighters have them off.
- Hair, cloth, face and hand morphs, translucent layers, and the costumes in `datadvd.afs`.
- The back-face flag is taken to be the same as DOA3's; the same word also holds 5 and 8 here.
- No chest spring: it is not ported for DOA2U.
