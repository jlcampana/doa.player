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

CAT word 2 is a list of 8-byte entries: a kind and an offset from the list, ending at kind 0xFF (`sub_0005A250`, installed by `sub_00057370`). DOA3's section scan rejects it, as kind 1 records differ, so `soft_body_offsets` reads it. Kinds 0xFA and 0xFB also appear, and bodies after them are still installed.

Kind 0xFE means another variant of the record before it, not another body. `sub_000587F0` gathers a record and its 0xFE followers into a group: up to 16 groups at state +0xF718, stride 0x24, byte +0xB is the count. A group holds the kind, the body's index and up to 8 record pointers, with -1 in unused ones. `sub_0005A740` compares the byte at `0x877CA0 + fighter` with the last value (state +1) and, when it changed, calls `sub_0004FC40` to reload every group with that variant. `sub_0005B050` writes the byte, with values from 0 to at least 3; what chooses them was not read.

`soft_body_offsets` skips 0xFE, so the viewer always shows variant 0. Read from the game on `kas00` and `kas01`: 6 and 3 groups, and the byte stayed 0 for 20 s in training, with no moves being done. Other values were not seen, so what the variants look like in play is inferred from the records.

State per fighter is at `0x80B7A8`, stride `0xF994`, and `sub_0005A740` runs it. Bytes +2, +3, +4, +5, +9 and +0xA count bodies of each kind. Bit 2 of byte +0 selects `sub_00051110` for cloth on a first frame; `sub_00051F20` is the normal update.

Reads catch the game mid-write: a sheet is written row by row, with gaps. `tools/re/doa2u_cloth_pairs.py` keeps a pair only after 7 ms of quiet. Early measurements without this were wrong, in both directions.

### Cloth, kind 2

State at +0x2090, stride `0x7C`. The file's record has DOA3's cloth layout (`doa3_cloth.rs`). At run time, flags move from +8 to +0xC and later fields shift by 4 (`sub_0004CAD0`). Particles match DOA3's: position, segment, velocity, last pull, 0x40 bytes each. Differences from DOA3:

- Wind is added to the pull, so it also feeds the velocity. DOA3 adds it to the force. The ring is at `0xE5AA40`, 64 vec4, head at `0xE5AE44`, phase +3 modulo 8 per record (`sub_0026BB80`). A push from `0xE5AE48` also exists; it was not used in these checks.
- `(flags >> 28) + 1` rows stay a closed ring under flag 2, not only the first. The ring's last column copies its first. Skirts rely on this.
- Flag 8: a column may start on a vertex of the object, by byte offset in the list at +0x50, and -1 keeps the record's root. Those vertices can be written by an earlier sheet, which chains sheets together, as on Helena's coat.
- Flags 0x180 both write vertices with their own offsets (`sub_00050A50`).
- Sheets can have 17 columns or more; the game limits columns to 27 and rows to 23. The floor is the stage's height (`sub_00146920`); the viewer has no stage.

One step against the game's particles, median per record, from `doa2u_single_steps`, with `DOA2U_FLOOR` at the stage's ground. Wind was 0 in these captures, so wind is not tested here.

| Sheet | Pairs | Segment error | Without stepping | Worst position |
| --- | --- | --- | --- | --- |
| `tin16` skirt | 6 | 0.00 mm | 2.79 mm | 0.01 mm |
| `hel00` coat, 3 sheets | 6 | 0.00 mm | 1.99 to 3.69 mm | 0.00 mm |
| `hel00` object 0, 2 sheets | 6 | 0.00 mm | 0.58, 0.66 mm | 0.00 mm |
| `lei00` skirt, 2 sheets | 20 | 0.00 mm | 14.57, 8.06 mm | 0.01, 0.00 mm |
| `lei04` skirt, 2 sheets | 20 | 0.00 mm | 14.08, 2.67 mm | 0.00 mm |
| `lei04` trouser cuffs, 2 sheets | 20 | 0.16, 0.83 mm | 0.49, 3.95 mm | 1.38, 7.15 mm |
| `tin09` cape | 8 | 1.03 mm | 1.36 mm | 6.01 mm |
| `lei00` and `lei04` head strips, 4 sheets, flags 0x90 | 20 | 2.88 to 6.76 mm | 4.67 to 16.21 mm | 13.24 to 32.24 mm |

The error between earlier captures was the stage's floor, at about 1.015 on that stage, which touches only sheets that reach it. The floor per row is the ground + 0.002 - 0.001 per row, as in DOA3. The game samples the ground once per sheet and frame (`sub_00146920`, plus the constant at `0x3C2E68`). 1.015 was read from where particles stop and then tried in steps of 0.002, and the stage's own height table was not read. The ground was flat under both fighters in this capture; slopes were not tried. The viewer's ground is 0 (`floor_height`), with the feet on it. Only the single step is measured, not a long run, where small errors could add up.

Other costumes were not measured, including Lei Fang's. The viewer's Wind option uses a stand-in, as in DOA3, and it is off by default.

### Chains, kind 1

Hair and similar. The file's record has DOA3's chain layout: count, bone, object, on, and fields from +4 (`sub_0004BED0`, which swaps bytes 2 and 3 at run time). State at +0x10 of the fighter's state, count in byte +2. `sub_000591A0` is the normal update and `sub_00058D30` the first frame. Differences from DOA3:

- Wind is added to the pull, with its factor at +0x3C.
- A chain can start on a vertex: +0x44 is a byte offset into the object at +2. The viewer sets this only at load, and the measured chains I checked have 0 there.
- Bones from 0x80 are matrices from 24.

One step against the game's particles, 40 pairs per record, wind included (`doa2u_chain_single_steps`): of the 21 records measured on `tin16` and `hel00`, 12 have a median segment error of 0.00 mm, and 7 more are at or under 0.68 mm. `tin16` chain 11 has 1.62 mm. `hel00` chain 7 has 4.97 mm, worse than the 4.15 mm of not stepping at all. Both are unexplained.

Detached objects, marked `0xFFFF` in a chain's target, are only drawn through the chain, and the viewer does this as in DOA3.

### Extra objects, kind 3

The layout of DOA3's section 3: +0 object, +2 mode, +3 bone, +4 offset, +0x14 angles. Mode 1 is solid and mode 2 translucent. Mode 0 records exist, such as `1f 00 00 0b` on `hel08`, and are left out. That one is variant 0 of a group whose variant 1 is `1f 00 01 0b`, the same object solid on bone 11, so object 31 is hidden by default. `kas00` has variants `10 00 01 02`, `2b 00 01 0b` and `0f 00 01 0b`: object 16 on bone 2 by default, or objects 43 and 15 on bone 11. The user's screenshot of `kas00` shows the sword sheathed at the waist with empty hands, which fits variant 0; that objects 15 and 43 are the sword in the hand is inferred from their shape and bone. Seen: `1e 00 02 01`, object 30 translucent on the head, on `hel08`, `hel00`, `aya00` and `kor00`. `10 00 01 02`, object 16 solid on bone 2, on `kas00`. Only the layout and these bytes were read; `sub_0004E920` and the draw were not compared with the game.

### Vertex copies, kind 250

`sub_0004E390`, run at the end of `sub_0005A740` for up to 8 records: +0 a condition (0 and odd values other than 0xFFFF skip it), +2 source object, +4 target object, +8 offset of a list. The list has a source byte offset, then target byte offsets whose low bit means another follows, and -1 ends it. Positions are copied raw, 12 bytes. They keep translucent layers and low detail objects on the sheets that move them: on `tin09`, object 16 is the translucent fur on the cape, and without this it stayed at rest as a loose strip. `vertex_copies` reads them.

Checked against captured vertices, with each copy read from the game's source and target objects:

| Copy | Links | Mismatches over 0.01 mm |
| --- | --- | --- |
| `tin16` 3 to 32 | 306 | 0 of 9180 |
| `tin09` 2 to 16 | 635 | 2450 of 38100, worst 30 mm |
| `tin09` 2 to 31 | 403 | 0 of 24180 |
| `lei03` 33 to 37 | 29 | 0 of 1740 |
| `hel00` 1 to 41 | 117 | 99 of 3510, worst 24 mm |
| `hel00` 32 to 40 | 108 | 0 of 3240 |

The mismatches are taken to be reads that fell between the source's write and the copy, since the game does not stop for reads. That was not proven.

### Limb slots in collisions

A shape's bone of 0x80 and up is a slot, not a bone. `sub_0004B920` builds them per fighter, into `0x84A0C0` and their inverses into `0x849E40`: matrix M times turns about X, then Y, then Z, in quarter turns:

| Slot | Matrix | Turns |
| --- | --- | --- |
| 0x82, 0x83 | 6, 7 | (1, 0, 1), (1, 2, -1) |
| 0x84, 0x85 | 8, 4 | (0, 1, 0), (0, -1, 0) |
| 0x86, 0x87 | 12, 13 | (1, 0, -1), (1, 2, 1) |
| 0x88, 0x89 | 14, 10 | (0, -1, 0), (0, 1, 0) |

They match the game's table exactly over 400 reads (largest difference 0.0000). The viewer used to skip these shapes, so skirts and capes went through the legs. A sheet's own bone can also be a slot in the game's code; records like that fail `plausible_record` and are not handled.

Two more differences from DOA3, read in `sub_00051F20` and `sub_0004BFE0`:

- A fold's length runs along the axis not in `axes` (`length_axis`). Legs run along Y. DOA3's port had Z fixed.
- The tip collision of flag 4 does not restore the segment's length.

### Normals, kind 251

`sub_0004FD00`, up to 4 records, just before kind 250. It rebuilds an object's normals: positions per group, seeds from the file for faces across a seam, strips whose face normals add up on all three of a triangle's groups, then normalize and write each group's normal and, from +0x14, its negative. `normal_rebuilds` reads it, and the viewer runs it every frame on all costumes.

Against captured skirts of `lei00` and `lei04`, largest component error per vertex, over 60 reads: median 0.04 and 0.06 with this, 0.56 with the file's normals, which is what the viewer had. p90 0.47 and 0.39. So it is far closer and not exact, and the rest is not explained: reads may catch a sheet between its positions and its normals, which was not separated.

### What changes normals

`tools/re/doa2u_normal_check.py` lists it per object. Measured on `lei00`, `lei03`, `lei04`, `tin09`, `tin16`, `hel00`, `hel08` and `kas00`:

- Hands: the shape tables, in the viewer, with 0 of 749 normals differing on `kas00`. Before this the viewer put them on the wrong vertices.
- Joint pieces: computed by the viewer. On `hel08`, median error under 0.01 and worst 0.03 per piece, from `joint_pieces_match_captured_vertices` with `tools/re/doa2u_joint_export.py`. Except the shoulders, objects 3 and 17, with a median of 0.04 and 0.06 and a worst of 0.13, where positions are also off.
- Sheets with kind 251: see that section. Sheets without it, and copies of kind 250, change no normals in the game either.
- Chains: a chain's +0x5C (+0x60 at run time) names the same data as kind 251, and `sub_00050510` runs `sub_0004FD00` with it after writing the vertices. On `hel08` only the ponytail has it. `normal_rebuilds` reads these too.
- The face (`apv` 2 and 3) and chest (`apv` 4) have a normal count of 0, so they have no normal tables.

### Head strips, unsolved

The two column sheets on the head of `lei00` and `lei04` (flags 0x90, stiffness 0) stay at 2.88 to 6.76 mm in a step. Tried, with no real change: no wind (it was 0), both ways spacing off, point collisions in place of edges, and stiffness of 0.1, 0.5 and 1. Removing shapes makes it worse, so collisions run. Sheets on the head of `hel00`, with four columns, match exactly. Fields +0x1C and +0x20 of the record are not used by the viewer, and their role was not read.

### Other kinds

The switch in `sub_00057370` fixes the offsets of kinds 1, 2, 4, 5, 7, 12, 15, 16, 250 and 251 (table at `0x57448`). It does not move kind 3, whose record has no offsets. `sub_0005A740` also calls `sub_00059750` (byte +5), `sub_00053680` (byte +4), `sub_00059C90` (byte +9) and `sub_00057550` (byte +0xA). Not ported, and which kind each runs was not fully matched. `sub_000587F0` sets the bodies up, with its own switch at `0x58BE4` and `0x58BF4`; kinds it has no case for go to `sub_00054890` and count in byte +5. Kind 8 appears on Helena's costumes: `05 00 23 00` on `hel08` names bone 5 and object 35, and bytes in the record look like object 36 and more. So objects 35 to 38 are probably one jointed item, not checked.

## Lighting

The game uses D3D's fixed function lighting (port: `recomp-runtime/d3d_draw_shader.cpp`). `tools/re/doa2u_lights.py` lists the lights it enables. Device at `0x355500`: active light records at +0x7AC, 8 slots; ambient at `0x35542C`; transforms at +0xC60, 0x40 each, with TEXTURE0 as state 2.

Reading those while the object at `0xEA6694` was a fighter's, on the desert stage: 4 directional lights and ambient `0xFF5E5E5E`, in 36490 of 36546 reads. Area 8 has ambient `0xFF3F3F3F` and other lights. The stage's own objects use other lights and ambients (`0xFF4C493F`, `0xFFAAAAAA`). Each light's specular is 0.3 of its diffuse. `sub_00285EC0` sets them, through the matrix at `0x877EB0`.

Formula, per vertex, with l toward the light: `rgb = emissive + ambient * material ambient + sum(material diffuse * light diffuse * max(0, n.l))`, clamped. `specular = sum(material specular * light specular * pow(n.h, power))` with an infinite viewer, for n.l > 0, clamped. Final color is texture times rgb, plus specular.

Material blocks are read by `material_look`, as `sub_00286C00` and `sub_00286D30` apply them:

- +0xC: low two bits select the cull mode through `0x7B0278` (0 none, 1 and 2 opposite windings). Bit 2 turns fog off. Bit 3 turns depth writes off. So 5 and 9 do hide back faces, which the viewer got wrong before.
- +0x20: `D3DMATERIAL8`. +0x64: stages. +0x68: bit 1 turns specular on; bit 0 was always set and was not read further.
- Stages, 16 bytes each: texture, then at +8 a mode for the table at `0x7AFFC8`, whose pairs of indices point at `(op, arg, arg)` in `0x7B0078`. +0xF selects coordinates through `0x7B018C`; 1 is a camera space reflection. Modes 0 and 1 are texture times diffuse, with alpha from the material or also from the texture. Modes 3, 4, 5 on three stages blend stage 1 over stage 0 by stage 1's alpha, then multiply by diffuse.
- Texture matrices during fighter draws: TEXTURE0 is 0.5, -0.5 with offset 0.5, TEXTURE1 is the identity, in all 49791 reads.

The viewer's own additions, not from the game: mipmaps and 16x anisotropic filtering on all textures, in DOA3 too, and an optional relief (Background panel, `--relief PERCENT`, off by default), a bump from each texture's brightness lit per pixel. It applies to every lit material, skin included, and looks grainy on detailed textures at the top of its range. Whether the game's own textures have mip levels was not checked.

Colors in the game have no gamma step. The viewer's textures and window are sRGB, so its shader converts to stored values and back.

Compared with the game: `hel08` and `kas00` by eye against one screenshot, in other poses and angles. Not measured by pixel. Differences to expect:

- The viewer holds the lights read so far (`DOA2U_LIGHTS`, picked in the Background panel or with `--lights N`). `0x87CFA4` indexed by `0x8783A9` does not identify a stage: lists `[8, 48]`, `[6, 48]`, `[41, 48]` and `[16, 15]` were read, and the value 6 came with two different sets of lights, one of them read with an older filter. `sub_000D79F0` sets those bytes, and what they mean was not read. So "Area 8" and "Area 48" are provisional names, to replace with what the user sees in each. To add a stage, run `tools/re/doa2u_lights.py 8` there and take the most frequent set. Fighters face a fixed way in the viewer, so the light falls differently from any given moment in play.
- The snowy night stage has point lights, with ambient `0xFF474747`. At (-3.441, 1.8, -16.983): diffuse (0.59, 0.39, 0.18), range 21.25, attenuation (0.25, 0.25, 0). At (-1.58, 1.8, -7.85): diffuse (0.6, 0.45, 0.24), range 22.25, the same attenuation. The viewer has no point lights and no fighter position, so "Snowy night" holds them as directional lights, worked out by hand for one read with the fighters near (-4.9, 0.75, -7.2). Other lights in that scene's reads, with their own positions, were seen a few hundred times against 30000 and left out. In play this light changes as fighters move. To do it properly the viewer needs point lights and a place on the stage for the fighter.
- The storm stage has lightning, which the user saw while these reads changed. During a flash, the three lights are at 0.2 of their values, ambient is `0xFF233131`, and the fourth light is white at about (1.248, 1.336, 1.336), direction (0.677, -0.29, -0.677). It is 579 reads against 7296 in one sample. The viewer has only the lights between flashes.
- Modes 2 and 3 alone (46 materials, such as on `zac04` and `aya18`) draw the texture with no diffuse, going by the tables at `0x7AFFC8` and `0x7B0078`. That was not compared with the game. Four stage materials on `hel10`, two of them, use only the first two stages.
- The game draws in list order, with all materials blended. The viewer draws flag 8 and kind 3 materials last, and discards texels under 0.1 of alpha, which the game was not seen to do.
- Normals of sheets without kind 251 stay as in the file, as in the game (`hel00` 0 and 1 changed none).
- No fog, shadows, or stage reflection. Stage reflection targets (`sub_0026D610`) were not read.

## Hands

`apv` entries 0 and 1 (`doa2u::hands`). Entry 1's object has the high bit set, which means its shapes are the other hand's with X negated. Each entry has 11 to 20 targets by costume, and the table's two pointers per target are the shape and its normals. +8 and +0x14 are the positions' group count and list. +0xC and +0x18 are the normals' own, whose offsets point at the normal, 12 bytes into the vertex.

A pose is the target of the same number. The move scripts in the character's `.bin` use DOA3's format (`parse_hand_events`). State per fighter is at `0x852F40`, stride 0xE0, one block per hand at +0 and +0x70: +0x14 and +0x18 point at the table entries being blended, +0x20 and +0x24 are their weights, +0x40 the script's time, +0x60 the pose.

Measured:

- Vertices of both hands on `lei00`, `lei04` and `lei03` read 0.00 from a target when no blend is running, on 60 reads each.
- Weights stepped by 1/11 and 1/5, matching script times 10 and 4 with DOA3's `1 / (time + 1)`.
- 78 of 96 scripted motions of `leifang.bin` and 87 of 108 of `zack.bin` are in the character's main `.mot`. The rest were not looked for in the other files.

Not measured: what a motion ID in the game is at a given moment, so no script was compared with the game's pose at a frame. The default times for -1 are DOA3's. A new pose during a blend starts from the old target, which is a guess. The viewer opens the hand on motions with no script, and the game probably keeps the last pose. DOA3's per-character substitution of pose 0 (`character_pose`) was not found in DOA2U and is not applied. Costumes with poses past their target count skip them, such as pose 16 in `tina.bin` on `tin09`.

## Chest spring

Only for Tina, Helena and Lei Fang in the viewer, fixed in code (`CHEST_SPRING_IDS`). Kasumi, Ayane and Hitomi have the data in the game, and the viewer does not use it for them.

State per fighter at `0x850538`, stride `0x70`, step in `sub_0005ADA0` and `sub_0005ABA0`. Parameters are copied from the table at `0x64C894`: stiffness 0.075, damping 0.9, drive 30, rest 0.3, of which half is used. DOA3's are 0.128, 0.85, 10 and 0.125. The formula has DOA3's form. A state field at +0x6C selects another path, which was not read, and was 0 in the fighters measured.

The shapes are entry 4 of the `apv` section at CAT word 3 (`sub_0005D610`): +0 targets, +4 object, +8 groups, +0x10 a table of two pointers per target, +0x14 vertex lists. Targets are absolute shapes as floats, 12 bytes per group, unlike DOA3's scaled i16 offsets. A list holds vertex byte offsets, with the low bit meaning another follows. Zack's section has only 4 entries.

Measured:

- `tools/re/doa2u_chest_check.py`, on `hel08` and `lei00`, 150 frames each: median error of the predicted value 0.00000. The 95th percentile is 0.65 and 1.04, and the worst 2.0. Those are taken to be reads a frame apart, which was not checked.
- Vertices against `base + (target - base) * |x|`, best `x` fitted per read: on `hel08` the worst residual was 0.00000 and on `lei00` a median of 0.00000 and a maximum of 2.4 mm. The base equals the file's shape on both.
- `tin` costumes and the viewer's spring running over time were not compared.

## Faces and hands

Entries of the `apv` section at CAT word 3: 0 and 1 the hands (object ORed with 0x80000000 on the second), 2 the eyelids and 3 the mouth, on the head's object, and 4 the chest where it exists. The layout is under "Chest spring". Fitting all targets to captured vertices of `hel08`, 30 reads, gave a residual of 0.00000 on both face sections, with target 1 reaching a weight of 1 and no other target used in that capture.

The viewer plays blinks and mouth movement with DOA3's timing. The real timing and the other mouth targets were not read. Hands are not ported.

## Winding and back faces

The viewer showed the inner side of materials that hide back faces, because DOA2U's triangles were wound against its renderer. `doa2u.rs` now reverses them. Found by comparing with a game screenshot of `hel08`: the skirt is white outside and purple inside, and the viewer had it the other way round, which looked like missing triangles at the waist.

An earlier check here concluded the flag and winding were right, since winding agrees with the stored normals. That compared the two with each other and not with the renderer's front face, so it could not see this. Compared with the game: `hel08` only, by eye. Other costumes changed too and look right, including the torso of `kas00`, which the user had reported as see-through, but they were not compared.

Material +0xC: 1 hides back faces. 0 does not. 4, 5, 8 and 9 also occur, and are not culled, which is unverified. Across the character costumes in `datahdd2.afs`: 0 on 40274 materials, 1 on 1651, 8 on 1305 in 124 costumes (17 on `tin09`), 9 on 134 in 16 costumes (80 on `zac01`, 40 on `zac05`, 1 on `lei03` and many Helena costumes), 5 on 12 in Kasumi's costumes only, 4 on 5.

## Costume files

The game's table at `0x694500` holds an `xpr` and `cat` path per costume, in order: Kasumi, Ayane, Tina, Lei Fang, Helena, Zack, Jann Lee, Ryu, Gen Fu, Leon, Bass, Bayman, Ein, Tengu, Hitomi, then `kas20` to `kas79`, `lei20` to `lei70`, `tin11`, `hel09`, `pdg00` to `pdg02`, `wba00`, `hks00`, `mam00`, `lor00` and `yti00`. Holes repeat another name: `kas00` and `kas18` among the later Kasumi ones, `bay00`, `ein00` and `kor00`. A second table at `0x695848` lists the `A0` series, in `char_doa3`.

The counts per ID at `0x6E0F08` are 8, 16, 8, 7, 9, 20, 8, 13, 7, 8, 1, 3, 20, 20, 1, 1, 1, 3, 1, 1, 1, 14, 1, 1, 6. The select screen uses them, so the viewer no longer does. `.bin` names in `0x694288` give mama, wakabs, loran, ytina and pdog. Their IDs are inferred by matching counts.

`datadvd.afs` has 122 costumes and all read: `every_costume_reads_as_the_viewer_does`. The viewer lists both archives, `datahdd2.afs` first, under each character. Costume numbers in `--load` are positions in that list.

The `A0` series is DOA3's format, both `cat` (plain, starting with 1) and `xpr` (plain). The viewer loads it through DOA3's parsers and solve, with DOA2U's bones and motions, DOA3's shading and no chest spring. That `bay` here is Leon and `sam` Bayman, and the IDs of `nin` (10), `sui` (22) and `gok` (23), are inferred from the second table's order; `hks` as 16 from the counts. `nin`, `sui`, `gok` and `hks` have no motions of their own, so they lie down in `comon.mot`'s first motion. Whether the game uses these files at all was not read: nothing was found referencing the second table. `yti` is taken to be Tina as a girl from the name `ytina`, not checked, and keeps no body soft bodies. Hands of `pdg00`, `pdg01` and `yti00` have other layouts and are skipped.

## Not done

- Not ported from the pose: the head's `sub_000EDBF0`, feet on slopes, the scale at `0x8A73B4`, and the fighter's root turn.
- Soft bodies of kinds other than 1, 2, 3, 250 and 251.
- Costumes hold many objects the viewer does not draw. `kas00` has 75, of which 31 are drawn: 19 in the draw list and 12 joint pieces. Type 9's low detail list accounts for some of the rest. In `kas00`, type 5 lists objects 45 to 59, which have no textures and reach down Z by 1: probably shadows, going by their shape. Type 7 lists objects 12, 13 and 14 (`sub_00060880`, up to 4 objects); of 9 costumes sampled, only `kas00`, `kas01` and `kas02` have it, and its use is unknown. Each list ends in `0xFE`.
