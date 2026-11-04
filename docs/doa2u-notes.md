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
  - Type 1: the draw list, used here. Type 9 has the same form with low detail objects. Type -1 is a version, used for the forearm twist. Type 10 sets upper arm flags. Types 3, 5, 6, 7, 11, 12 and 18 are not read yet. Type 6 starts with the magic `apa` and is read by `sub_000602F0`.
  - The switch is at `0x5BBAC`, indexed by type + 1. Slots at 0x80 and up in a draw list go through `0x64C930` (`sub_0005AF30`).
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
- Upper arms: CAT record type 10 (`0x10100` in 74 costumes) sets flags at `0x850710`. With them on, upper arms 14 and 8 take the chest's frame turned to the arm's X, moved 0.3 of the way back to the arm's own frame (`sub_00285B40`).
- Matrices 28 and 29 are the hand ends of the twists: the rotation of 24 and 25 at the hand. Joint pieces of forearms use them as parents.

### Measurements

Against frames of a CPU match, Zack (ID 0) against Bayman (ID 24), costumes without the upper arm flags:

| Check | Worst error |
| --- | --- |
| Legs: matrices 6, 7, 12, 13 | 0.003 |
| Arms: matrices 4, 8, 10, 14 | 0.012 |
| Hands: matrices 5, 11 | 0.002 |
| Twists: matrices 24, 25 | 0.003 |
| `eval_motion` against the game's tracks | 0.0001 |

The last line comes from `captured_tracks_are_found_in_motions`: 13 of 20 frames of Zack were found in `zack.mot` or `comon.mot`. Frames not found are taken to be blends between motions, which was not checked. Tina (ID 1) against Lei Fang (ID 12), both with the upper arm flags on, 12 frames: arms 0.011, and 0.64 without the flags; hands 0.002; twists 0.003. Legs are within 0.002 except in one frame, where one leg is off by 0.12 and its foot by 5 cm. Foot rotations also differ, by up to 0.10, in 4 frames, with their positions right. A sloped floor would explain the rotations, but this was not checked, and the one bad frame is unexplained.

Joint pieces of `kas00`, with the game's matrices, against vertices read from the game over 8 frames: worst 7 mm, median under 1 mm, except the shoulders, objects 20 and 32, with a median of 1 cm and a worst of 2.6 cm. The cause is unknown.

A read can also catch the tracks one frame ahead of the matrices, so `doa2u_live.py capture` drops frames whose pelvis does not match its tracks.

Internal names differ from the shown ones: `sambo` is Bayman, `bayman` is Leon and `koryu` is Hitomi, per the user. The user selected Bayman, whose fighter had ID 24. It matched 13 frames in `sambo.mot` and 10 in `bayman.mot`, with motion 184 found only in `sambo.mot`, and the CAT in memory was `bay00` (97% of sampled bytes equal, the next costume outside Bayman's about 5%). In a later match Leon had ID 8 with `leo00` in memory (98%), and Hitomi ID 21 with `kor00` (96%). Their poses were found in `bayman.mot` (6 of 36 frames) and in `koryu.mot` or `comon.mot` (8 of 24). The counts at `0x6E0F08` agree: 7 costumes for ID 8, 6 for ID 24.

## Soft bodies

CAT word 2 is a list of 8-byte entries: a kind and an offset from the list, ending at kind 0xFF (`sub_0005A250`, installed by `sub_00057370`). DOA3's section scan rejects it, as kind 1 records differ, so `soft_body_offsets` reads it. Kinds 0xFA and 0xFE also appear, and bodies after them are still installed.

State per fighter is at `0x80B7A8`, stride `0xF994`, and `sub_0005A740` runs it. Bytes +2, +3, +4, +5, +9 and +0xA count bodies of each kind. Bit 2 of byte +0 selects `sub_00051110` for cloth on a first frame; `sub_00051F20` is the normal update.

Reads catch the game mid-write: a sheet is written row by row, with gaps. `tools/re/doa2u_cloth_pairs.py` keeps a pair only after 7 ms of quiet. Early measurements without this were wrong, in both directions.

### Cloth, kind 2

State at +0x2090, stride `0x7C`. The file's record has DOA3's cloth layout (`doa3_cloth.rs`). At run time, flags move from +8 to +0xC and later fields shift by 4 (`sub_0004CAD0`). Particles match DOA3's: position, segment, velocity, last pull, 0x40 bytes each. Differences from DOA3:

- Wind is added to the pull, so it also feeds the velocity. DOA3 adds it to the force. The ring is at `0xE5AA40`, 64 vec4, head at `0xE5AE44`, phase +3 modulo 8 per record (`sub_0026BB80`). A push from `0xE5AE48` also exists; it was not used in these checks.
- `(flags >> 28) + 1` rows stay a closed ring under flag 2, not only the first. The ring's last column copies its first. Skirts rely on this.
- Flag 8: a column may start on a vertex of the object, by byte offset in the list at +0x50, and -1 keeps the record's root. Those vertices can be written by an earlier sheet, which chains sheets together, as on Helena's coat.
- Flags 0x180 both write vertices with their own offsets (`sub_00050A50`).
- Sheets can have 17 columns or more; the game limits columns to 27 and rows to 23. The floor is the stage's height (`sub_00146920`); the viewer has no stage.

One step against the game's particles, wind included, median over 60 pairs per record, from `doa2u_single_steps` on the last capture:

| Sheet | Pairs used | Segment error | Without stepping | Worst position |
| --- | --- | --- | --- | --- |
| `tin16` skirt, 17 x 6, flags 0x20000102 | 60 of 60 | 2.01 mm | 5.49 mm | 43.51 mm |
| `hel00` coat 1, 7 x 4, flags 0x180 | 60 of 60 | 3.49 mm | 5.30 mm | 18.85 mm |
| `hel00` coat 2, 4 x 5, flags 0x188 | 58 of 60 | 1.03 mm | 6.37 mm | 14.46 mm |
| `hel00` coat 3, 4 x 5, flags 0x188 | 60 of 60 | 0.00 mm | 5.39 mm | 0.01 mm |
| `hel00` object 0, 4 x 1, flags 0x80 | 60 of 60 | 0.00 mm | 2.06 mm | 0.00 mm |
| `hel00` object 0, 4 x 1, flags 0x80 | 60 of 60 | 0.00 mm | 2.45 mm | 0.00 mm |

Repeated captures differ: the first coat sheet gave 0.00 mm in one and 3.49 mm in another, so some situation is not covered. The stage's floor is the first suspect, not checked. Which sheets have shape collisions was not checked either, so the wording of commit `a9cb787`, that sheets without collisions match exactly, is not verified.

Other costumes were not measured, including Lei Fang's. The viewer's Wind option uses a stand-in, as in DOA3, and it is off by default.

### Chains, kind 1

Hair and similar. The file's record has DOA3's chain layout: count, bone, object, on, and fields from +4 (`sub_0004BED0`, which swaps bytes 2 and 3 at run time). State at +0x10 of the fighter's state, count in byte +2. `sub_000591A0` is the normal update and `sub_00058D30` the first frame. Differences from DOA3:

- Wind is added to the pull, with its factor at +0x3C.
- A chain can start on a vertex: +0x44 is a byte offset into the object at +2. The viewer sets this only at load, and the measured chains I checked have 0 there.
- Bones from 0x80 are matrices from 24.

One step against the game's particles, 40 pairs per record, wind included (`doa2u_chain_single_steps`): of the 21 records measured on `tin16` and `hel00`, 12 have a median segment error of 0.00 mm, and 7 more are at or under 0.68 mm. `tin16` chain 11 has 1.62 mm. `hel00` chain 7 has 4.97 mm, worse than the 4.15 mm of not stepping at all. Both are unexplained.

Detached objects, marked `0xFFFF` in a chain's target, are only drawn through the chain, and the viewer does this as in DOA3.

### Other kinds

The switch in `sub_00057370` installs kinds 1, 2, 4, 5, 7 and 12 (table at `0x57448`). `sub_0005A740` also calls `sub_00059750` (byte +5), `sub_00053680` (byte +4), `sub_00059C90` (byte +9) and `sub_00057550` (byte +0xA). Not ported, and which kind each runs was not fully matched.

## Holes in some costumes

Not solved. See "Next steps" in `CLAUDE.md`. Measured on `hel08` and `kas00`: triangle winding agrees with the stored normals on nearly every material, flag 0 or 1, so the back-face flag looks right: flag 1 hides back faces, and turning culling off shows linings from inside. Flags 5, 8 and 9 also occur, and are not culled.

## Not done

- Not ported from the pose: the head's `sub_000EDBF0`, feet on slopes, the scale at `0x8A73B4`, and the fighter's root turn.
- Soft bodies other than kinds 1 and 2, face and hand morphs, translucent layers, and the costumes in `datadvd.afs`.
- Costumes hold many objects the viewer does not draw. `kas00` has 75, of which 31 are drawn: 19 in the draw list and 12 joint pieces. Type 9's low detail list accounts for some of the rest. In `kas00`, type 5 lists objects 45 to 59, which have no textures and reach down Z by 1: probably shadows, going by their shape. Type 7 lists objects 12, 13 and 14 (`sub_00060880`, up to 4 objects); of 9 costumes sampled, only `kas00`, `kas01` and `kas02` have it, and its use is unknown. Each list ends in `0xFE`.
- The back-face flag is taken to be the same as DOA3's; the same word also holds 5 and 8 here.
- No chest spring: it is not ported for DOA2U.
