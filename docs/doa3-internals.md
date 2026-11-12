# DOA3 internals

Notes for the viewer. Addresses are function names in the recompiled source at
https://github.com/parkerallan/doa3-recomp, under `doa3/src/game/recomp/gen`.

## Pose and animation (implemented)

| What | Where |
| --- | --- |
| Motion track evaluation | `sub_00095320`, `sub_00095400`, key types `sub_00095430`, `sub_000954E0`, `sub_00095580`, `sub_000955B0` |
| Track layout per motion | `sub_00090967` |
| Pelvis, chest and head | `sub_00091C30`, `sub_0008F970`, `sub_0008E6B0` |
| Limb targets and pole vectors | `sub_00091720`, `sub_00091BB0` |
| Limb IK and forearm twist | `sub_00094660`, node table at XBE `0x32EFC0` |
| Per-character bone offsets | Pointer table at XBE `0x32DE78` |

## Joint skinning (measured against the running game)

Measured by reading the live game's bone matrices and vertices, see
`docs/viewer-mode-feasibility.md`. The game's routine itself has not been found.
Errors below come from Jann Lee entry 198 unless noted.

A CAT record lists joint piece vertices grouped by `g`, a weight in 1/16 units.
The record header holds rest rotation rows at vec4 1, 3, 5 and points at vec4 6 to 9.

- `g == 16`: position = parent matrix * CAT vertex. Error 0.000.
- `g < 16`: the CAT vertex is in a frame with its own origin:
  - rotation = parent rotation * fraction(delta, 1 - g/16) * rest rotation, where delta is the node's rotation relative to the parent, relative to rest. Measured fractions match `1 - g/16` to about 0.001 on the chest; small groups are noisier.
  - origin = lerp(node matrix * own point, parent matrix * parent point, g/16).
  - Own and parent points are vec4 6 and 7, or vec4 9 and 8 on elbows and knees (nodes 4, 7, 10, 13).
  - Mean error is about 2 to 6 mm, so this is close but not exact. Max seen is about 1.5 cm.
- Hands use twist nodes 19 and 20 as parent. Some costumes store them crossed, so they are assigned by hand node.
- Optional streams, flag high nibble, slots at record +0xC and +0x10: an object index, then lists of its vertex followed by joint vertices that sit on it. The rigid piece does not move. Measured on Helena entry 412: covers all remaining changed vertices, error about 0.3 mm.

Routines checked that are not the joint code: `sub_0002A02F` (camera shake), `sub_0003F5F0` (ribbon or strip), `sub_0008D1B5` (stage collision), `sub_00115BDB` (particles).

Also seen changing in memory, not implemented:

- Jann Lee objects 14 and 29, the lower trouser legs: 149 vertices move up to 12 cm each frame. Probably the cloth below, as on Tina's legs; not checked on him.
- Helena object 35 (274 vertices, 1.8 cm) and Tina object 33 (271 vertices, 1.2 cm): not chains, and not cloth, whose records use other objects.
- Hand objects 10 and 25 and head object 1: many vertices differ from the file but stayed the same between two snapshots. Possibly hand poses and facial shapes, unconfirmed.

## Soft bodies

`sub_00043A60` runs them for each fighter, right after the bones. State per fighter at `0x3C8408`, stride `0x215A0`. Bytes +2 to +8 count the records of each kind below. Bit flags at +0 pick the setup routine on the first frame.

The CAT holds a list of 8-byte sections, each a type byte and a u32 offset, ending at type `0xFF` (`sub_00043680`). All its offsets are relative to the list's own start. Header word 2 points to it, but some CATs scramble the header, so `find_sections` scans for it.

| Type | Records | Setup, update | Status |
| --- | --- | --- | --- |
| 1 | Chains, `0x68` at +0xC | `sub_0003A0F0`, `sub_000422B0`, `sub_00042540` | Implemented when they write vertices |
| 2 | Cloth, `0x280` at +0x104C | `sub_0003ADF0`, `sub_0003FD20`, `sub_000403F0` | Implemented; see below |
| 0 | Worn objects, `0x70` at +0x244C | `sub_0003BB60`, `sub_0003BC70`, `sub_0003DCB0` | Drawn in place; being knocked off is not implemented |
| 4 | Tubes, `0x47D4` at +0x2D04 | `sub_00042D20`, `sub_00042FD0`, `sub_000430F0`, `sub_000418E0` | Implemented for ring mode 0; see below |
| 5 | Springs, `0xC` at +0x14C54 | `sub_0003CEB0`, `sub_00042960`, `sub_00042A90` | Implemented; see below |
| 3 | Extra objects, 8 bytes at +0x2CF4 | `sub_0003CE40`, `sub_0003EBA0` | Implemented; see below |
| `0xFA`-`0xFD` | `0x178` at +0x259C | `sub_0003EA70`, `sub_0003EB10` | Not implemented. Count 0 in the matches read, though the files have them, so they may only run in some game modes |

The particle chains at `0x44DB50` (`sub_0003CBF0`, `sub_000412F5`, `sub_00041780`) are not these. Their count was 0 in a match.

### Chains

Per frame, with `p` the bone's matrix applied to the offset and `D` the same for the direction:

    pull = particle.position - p
    velocity = (velocity + pull - last_pull) * damping, then y -= gravity * 0.5
    force = pull * k + velocity + D;  D *= bend
    force += (D - segment) * stiffness + wind * wind_factor, limited to max_force
    segment = normalise(segment + force) * length, then collisions and floor
    position = p;  p += segment

Checked frame by frame against the game on Helena: the root position has error 0, the rest about 2 mm, which is close to the sampling noise of reading a running process. Not proven exact.

Collision shapes are `0x1C` bytes: bone, type, then 6 floats, in the bone's space. Types: 0 box, 1 rounded box, 2 capsule, 3 sphere, 4 ellipsoid, 5 keep inside a box. Not individually verified against the game, though they run in the check above.

Vertices (`sub_0003F5F0`): each segment's frame in row vectors is `Rz(b) * Rx(a) * T(p)` in the bone's space, with `a = atan2(z, y) + pi`, `b = atan2(x, hypot(y, z))` of this segment plus the next. Points end at x = 999, and each has a list of VB byte offsets where the low bit means another follows. Head `0xFFFE` writes into an object already drawn; `0xFFFF` is an object drawn only through its chain, with the chain's offset added to the bone. Verified: error 0.00000 on 90 vertices of Helena's object 34, her ponytail.

Chains with any other head (46 of 702) draw whole objects instead (`sub_0003AA20`, called from `sub_000422B0` when the head's first word is neither 0xFFFE nor 0xFFFF). The game's chain is the file's record with the particle array put in at +0x48, so +0x48 onwards moves 4 bytes (+0x50 is the file's head at +0x4C). Each object is drawn with the chain bone's draw matrix moved by the offset at +0x14, then by the mode, the record's third byte, through the jump table at `0x3ADDC` (`0x3AAE3, 0x3AB72, 0x3AC81, 0x3AD3B, 0x3AAE3`; past 4 nothing is drawn):

- 0 and 4: turned by the floats at +0x24, +0x28, +0x2C (the chain's direction) read as degrees (times 182.044, `0x1ED788`), about z, then y, then x, and one segment length further down y after each object. The head holds one word per particle, the object number masked with 0x0FFF. The particles are not used.
- 1: each particle's segment, turned into the bone's space by `0x3C8400`, turns its objects as `sub_0003F5F0` turns a segment (x by `atan2(z, y)` plus a half turn, then z by `atan2(x, hypot(y, z))`), all at the chain's start. Each particle has a list: an optional 0xFFFD first (it changes draw state: `sub_00154610(0)`, `sub_00154690`, `sub_00154F70`, `sub_00154640(1)`, flag `0x454434`), then numbers whose top bit says another follows.
- 2: one word per particle; the game builds a frame from the negated segment and a cross product and then draws with the bone's matrix unchanged, so the objects are not turned.
- 3: one word per particle, turned about y by `atan2(x, z)` less a quarter turn, then about z by `atan2(y, hypot(x, z))` plus a quarter turn.

Every such chain in DOA3's costumes is mode 0, with a direction under 0.02, so their objects hang straight down the bone, and none of the objects is in the draw list (ignored test `survey_placing_chains`): before this they were not drawn at all. Ein's wrist chains (costume 2), Gen Fu's braid, Brad Wong's belt beads and sash, Lei Fang's, Christie's and Zack's are among them. In the viewer (`SoftChain::placed_objects`) each copy is its own piece on the chain's bone; modes 1 and 3 are placed again each frame. By eye only. The viewer keeps them for characters whose torso-front chains it drops. DOA2U has the same function, `sub_00050100` (called from its chain update `sub_000591A0`, jump table at `0x504F8`, degrees by 182.044 at `0x3C2CDC`), with one difference: its mode 2 turns the objects, about y by `atan2(x, z)` plus a half turn, then about x by `atan2(y, hypot(x, z))` plus a quarter turn (`PlacementCode::Doa2u`). DOA2U's costumes in DOA3's format use it since 2026-11-12: 17 chains, all mode 0 but Helena's `hikA0` (six of mode 1: the chest pendant and the head's pieces), in `tinA0`, `genA0`, `basA0`, `korA0`, `suiA0`, `suiA1`, `gokA0` and `gokA1` (ignored test `survey_doa2u_placing_chains`). By eye on `hikA0`. DOA2U's own costumes have 126 more (modes 0, 17 and 31 among them, Gen Fu's braid in `gen00` to `gen07`, Tina's `tin06`, `tin07`, Bass's `bas03`...), still left out by `parse_chain_records`.

Not implemented in the viewer:

- Wind. The game adds `0x90EF00`, a 32-frame buffer (`sub_001537E0`). It was about 1 cm per frame in the tested match, and the error doubled without it.
- World movement of the fighter, as the viewer stays in place.
- Normals of the moved vertices (`sub_0003F220`), which keep their rest direction.

### Cloth

Skirts, robes, sleeves and trouser legs: a grid of `columns` hanging chains of `rows` particles, linked sideways. Mode 0 is a closed ring, mode 1 an open sheet that solves outward from a pivot. Record: +0 mode, +1 axis, +2 bone, +4 object, +5 columns, +6 rows, +7 pivot, +8 flags, +0xC default length, +0x10 gravity, +0x14 pull, +0x18 damping, +0x1C radius, +0x20 reach, +0x24 bend, +0x28 stiffness, +0x2C wind factor, +0x30 max force, then pointers: +0x34 row lengths, +0x38 column lengths, +0x3C angles, +0x40 roots, +0x44 vertex offsets, +0x48 shapes, +0x4C normals, +0x50 unused here, +0x54 fold block, +0x58 vertex points.

Per row, each column runs the chain step, using the previous row's segment as target, with the floor at `0.002 - 0.001 * row`. Then:

1. Neighbouring tips are pulled to the row's length (`sub_00039DD0`). Flag `0x10` also pushes them apart.
2. With a fold block, each column is turned around the next joint by a share of its angle, its tip is limited in angle and radius (`sub_0003CCB0`), and it is turned back. The stiffness also drops on the squeezed side. Nodes past 14 in the block are points on the hands and feet, table `0x2E93B8`.
3. Edges between tips collide with shape kinds 2, 3, 4 and 7 (`sub_0003DA20`). Both ends move by the same amount. Flag `4` also collides the two end columns as points.

Flag `2` makes row 0 a ring over one column fewer. Flag `0x40` adds a cap row 5 mm inward. Flag `0x80` writes vertices with their own offset from the particle, turned with the row when there is a fold block; otherwise a vertex sits on its particle.

The wind is one force per record: `ring[(head - phase - 1) & 31]` times the record's wind factor (`sub_00153800`). The ring is at `0x90EF00` (32 vec4), the head at `0x90F108`, and the phase starts at 0 for each fighter and adds 3, modulo 8, per record. `sub_00153490` fills the ring each frame from per-stage settings and random numbers; it is not ported. The viewer's Wind option (`src/formats/doa3_wind.rs`) uses the same ring and phases with a stand-in gust of the measured size, about 0.011 per frame, and is off by default. The floor is the stage's height at the fighter (`sub_000D3E10`), not 0.

Checked with `matches_captured_frames` on 120 pairs of consecutive frames of Lei Fang (entry 537: head ribbons and skirt, mode 1, flags `0x90`, 4 and 0) and Hayate (entry 523: sleeves and robe, mode 0 flags `0xC0`, mode 1 flags 4 and 0), using the game's own wind ring:

- Vertices against particles: at most 0.005 mm.
- Particles after one step, mean per record: median 0.003 mm. Cloth moved 2.8 mm per frame in the median sample.
- Worst 0.75 mm mean, 15 mm on one tip: a skirt resting on the stage's floor at height 0.9177, which the test places at 0. With the floor set to 0.9227 (0.9177 plus 5 mm for its row) 30 frames off by more than 0.05 mm drop to 5; neighbouring heights leave 21 to 33.
- Without wind: mean 2.9 mm, so wind matters.

Also checked on 90 pairs of frames of two Hitomis, costumes 5 and 2, leaving out the torso records, with the ring: median 0.001 mm, worst 0.05 mm, covering shape kinds 2, 4 and 7. An older capture of Helena's sleeves and Tina's trouser legs has no ring and fits the wind per record: mean 0.14 mm, worst 0.7 mm.

Run it with `DOA3_CLOTH_CAPTURE=<file> cargo test --release matches_captured -- --nocapture`. The capture holds, per record, a header of 8 u32 (fighter, record, CAT entry, columns, rows, object vertex count, particle bytes, wind bytes), the first frame's particles, the second frame's 23 bone matrices, its particles and object vertices, and the wind ring plus head (older captures: wind bytes 0 and 12 bytes of unused wind).

Not checked against the game: flag 2. Kind 7 as a point test, used by the end columns, was not read to its end and follows the edge version. Records with flags `0x29` are refused: they only appear in false sections.

## Chest spring (implemented for a fixed list)

Only runs for the characters listed in `CHEST_SPRING_CHARACTER_IDS`. The list is fixed in code. `characters.toml` can turn it off but cannot extend it.

Spring, one per fighter, stride `0x74` at `0x46B748`. Setup `sub_00046030`, step `sub_00046500`, `sub_00046340`, `sub_00046170`. Per 60 Hz frame:

    target = 0.125 - 0.125 * chest.up.y
    v = 0.85 * v - 0.128 * (x - target) - 10 * drive
    x = clamp(x + v, -1, 1)

`drive` is the frame's change in the y of `(0, 0.2) - chest^-1 * previous point`, the point being (0, 0.2, 0.3) in chest space. The 10 is 20 times 0.5, the factor the game used while standing. Other motion states scale it differently, and that is not implemented. Verified on Tina: residual below 1e-9 over 360 frames.

Deformation: applied by `sub_00049B30`. The CAT's `LNK` section holds a table at +[+0x10], whose entry +0x10 is this section; male models have -1 there. Section: +4 lists, +8 base, +0xC positive, +0x10 negative. Each group is a u16 count, its vertex indices, and i16 triples: `(base + offset * |x|) / 32767`. Verified on Tina's object 1: error 0.00000 over 1405 vertices.

The viewer's Bounce slider multiplies `x` before this deformation, from 0 to 3, where 1 is the game's amount. It does not change the spring itself. `RUST_LOG=doa_player=debug` logs the values each frame.

Parameters come from the costume when present, else the defaults `0.128, 0.85, 20, 0.25`, which were the live values on Tina and Helena. The costume path has not been traced.

## Sections

The list of sections starts at the CAT's third header word. Scanning for it, as the code did first, could stop at empty bytes just before the real list, which shifted every record of that model. The scan remains for files whose header is scrambled.

## Springs

Code: `src/formats/doa3_springs.rs`. Points on a bone, each on a spring to its place, as on Jann Lee's and Helena's legs. Record: +0 bone, +2 object, +4 gravity, +8 pull, +0xC damping, +0x10 wind factor, +0x14 stiffness, +0x18 max distance, +0x1C rest points ending in x = 999, +0x20 per-vertex offsets (`sub_000417D0`, refused), +0x24 vertex lists, +0x28 normals, +0x2C max distance per point. Wind phase starts at 0 per record and adds 3 per point.

`step_matches_captured_frames`, with `DOA3_SPRING_CAPTURE`, ran 1200 single steps of Jann Lee and Helena fighting, with the game's wind ring: mean error median 0.00002 mm, while points moved 8.9 mm per frame. 6 steps, in two moments, are off by 1.4 to 5.4 cm for several records at once. Their cause is not known.

## Worn objects

Section 0 holds objects worn on a bone, such as hats, which the game can knock off. Record: +0 mode, +1 bone, +0x1C offset, +0x2C angles about x, y and z, +0x32 object. Modes 1 and 2 are read; 3 draws nothing. Seen in entries 234, 374, 485 and 651. The viewer leaves them on. Not compared with the game.

## Tubes

Code: `src/formats/doa3_tubes.rs`. Trouser legs made of stacked rings of particles, as in Hitomi's costumes 4 and 5. Record: +1 bone, +2 rings, +3 columns, +4 object, +8 step, +0xC spacing, +0x10 gravity, +0x14 pull, +0x18 damping, +0x1C wind factor, +0x24 max force, +0x28 start, then pointers: +0x38 fold, +0x3C shapes, +0x40 ring limit, +0x44 first ring, +0x48 vertex lists, +0x4C vertex offsets, +0x50 normals, +0x54 steps, +0x58 columns per ring, +0x5C spacings, +0x60 ring modes (count and value pairs). The start's height is replaced by the mean height of the first ring.

Each ring is placed along the bone at the distance of its steps and turned about y around the next joint (`sub_000430F0`). Its particles run the chain step, the same as cloth; neighbours are pulled within the spacing, sweeping both ways; points go into the ring's space, flat, and limited in angle and radius (`sub_0003CF00`); the flat shape is kept, then the points return to the world and their edges collide with the shapes (`sub_0003B9C0`).

Checked with `rings_and_vertices_match_captured_frames` on 90 frames of Hitomi's costume 5, using `DOA3_TUBE_CAPTURE`: ring placement to 0.00004 mm and 1 angle unit; vertices made from the game's ring shapes, including collisions, to 0.0012 mm. Vertices move up to 7.6 cm from the file's shape.

`step_matches_captured_frames`, with `DOA3_TUBE_STEP_CAPTURE`, runs 300 single steps of Hitomi's costume 5 while throwing punches, with the game's own wind ring: mean error median 0.0014 mm, worst particle 0.13 mm, with particles moving 7 mm per frame. Refused: rings of any mode but 0 (mode 1 goes through `sub_00044620`, a spline solver) and records whose rings differ in column count. Hitomi's costumes 0, 3 and 6 and model entry 239, whose character has not been looked up, have such rings.

## Extra objects

Section 3 (`sub_0003CE40`, `sub_0003EBA0`) draws an object on a bone. Record: +0 object (the top 4 bits select objects outside the model), +2 mode, +3 bone, +4 offset, +0x14 angles about x, y and z. Mode 1 is solid, such as Kasumi's sheathed knife. Mode 2 is translucent, on most characters a second layer of the head and hair, with textures of partial alpha (`sub_001562E0`). Mode 3 is not seen in real records. The viewer draws translucent objects last, blended, writing no depth, with the usual alpha blend; the game's exact blend state in `sub_00156160` has not been read.

Vertices of a detached object that no particle moves stay at the chain's anchor (`anchored_vertices`). Left at the bone's origin they stretch into long thin lines. This follows from the object's space; it has not been compared with the game.

The knife in her hand is two more objects, 7 and 40 on costume 0, one per grip, with a second set of chains in sections `0xFE` on bone 11. What switches between them is not known and they are not drawn.

## Back faces

A material block's +0xC is 1 when back faces are hidden. This is inferred, not read from the draw code: across every model, 878 of 889 flagged materials wind all their faces one way, against 2861 of 12778 without the flag. Skirts and robes need it: their lining sits against the outer cloth and would otherwise cover it, as on Kasumi's costume 0. The viewer draws other materials from both sides.

## Hands and faces

Code: `src/formats/doa3_morphs.rs`. The XPR's own hand and face are never drawn. The CAT's `LNK` block holds a table at LNK + [LNK + 0x10], with one section per use:

| Table entry | Section | Written to |
| --- | --- | --- |
| +0x04, +0x0C | Face positions | Head object |
| +0x10 | Chest spring, see below | Chest object |
| +0x18, +0x1C | Right and left hand positions | Hand objects |
| +0x20, +0x24 | Right and left hand normals | Hand objects |

A section: +0 target count, +4 vertex lists (i16 count, then i16 vertices, ending in count 0), +8 base shape (three i16 per list), +0xC target offsets. Values are scaled by 1/32767. Target values are offsets from the base.

Checked against the running game, for hands and faces of two fighters: base shape error 0 when the game is at rest. Hands at pose 1, on its three steps: error 0, except one 4 mm sample taken while the pose changed.

Hand state: `0x46CA70` and `0x46CCC8` + fighter * `0x4B0`. +4 progress, +8 rate per frame, +0xC weight, +0x10 step, +0x14 pose. Each of the 28 poses lists up to 6 targets, in a table in the CAT (+0 target count, +4 pose count 28, then a count and an offset per pose). Progress runs from 0, the open hand, to 1. Pose 1 is the fist: pressing guard in a match asks for it and it closes in 3 to 4 frames.

Poses come from the move scripts in section 21 of the character's `.bin`, run by `sub_000888B0`. The section starts with 1024 offsets from the start of the file. A script is a list of u16, ending in `0xFFFF`: `0 motion start` plays a motion, `1 frame` waits for a frame of it, `46 hand pose` and `76 hand pose time` ask for a pose (hand 0 left, 1 right, 2 both). `OP_ARGUMENTS` lists every opcode's size. A walk of Jann Lee's scripts ends exactly on the end marker in 421 of 431; opcodes `0x6902`, `0x6903` and `0x7777` are unknown and stop it.

Not checked against the game: that each motion's poses and timing match, since only the fist's shape and its blend were compared. Scripts can also change poses after jumping to another script, which is not followed. Moves with no pose in their script use the open hand. The time-to-rate rule follows `sub_0004B784`, with a simplified closing step.

Faces: `sub_00048C30` blends up to 32 weighted targets of the head's two position sections, +0x04 and +0x0C. Weights are at `0x46BAF0` + fighter * `0x188` + 0xD0 and `0x46C1A0` + fighter * `0x16C` + 0xD0, one per channel, and a map at +0x156 and +0x13A gives the target of each channel. With the game's own weights, these sections give its vertices with error 0 on Jann Lee and Helena.

In those matches channel 1 of the first state, target 0, moves the eye area once in a while, closing in about 6 frames. Channel 2 of the second, target 0, moves the mouth area every 94 frames, up in 8 and down in about 26. The viewer plays those two with these timings, a blink every 300 frames being a stand-in for the game's own interval. The other targets, and how moves change them (`sub_0004AD40`, `sub_0004CBF0`), are not ported. On the 6 models of Hayate, target 0 of the first section moves 11 cm against a median of 2.1 cm elsewhere; what it is has not been read, so they do not blink.

## Save file (`DOA3SAVE.DAT`)

Size `0x3C10`, kept in memory at `0x484D78`. The unlock tool's values were seen loaded in memory in a match. Under QuiverLauncher it lives in `~/.local/share/com.quiverlauncher.app/proton/DeadOrAlive3-Doa3Recomp/pfx/drive_c/users/steamuser/AppData/Local/DeadOrAlive3/TitleData/`, not in the game folder's `.steam-compat-data` prefix. Byte `0x3C0C` is the XOR of all previous bytes (`sub_00021930`). Tool: `tools/doa3_unlock_save.py`.

Each character ID has groups of costumes: `0x3680F0` points to the size of each group, `0x368088` to the costume of each position (`sub_000C81C2`). Kasumi has three groups of three, all nine models on disc. Hitomi has groups of 2 and 1, costumes 5, 4 and 2, though the disc holds eight models: the rest cannot be picked, which is where most of her visible defects are. Costumes past the groups come from files `T:\\kasNN.xpr` and `.cat` found at start-up (`sub_00080A00`), `NN` in hexadecimal. None exist here; what each file must contain has not been read.

| Offset | Meaning | Confidence |
| --- | --- | --- |
| `0x2A8E` + slot | Costume groups available per character slot, clamped to the count at XBE `0x36806C` | Read from code |
| `0x3AC1` + slot | Checked in `sub_0007E110` and `sub_00081E10`, probably "cleared with this character" | Inferred |
| `0x2AB5`, `0x3BEA` | Set when all of the above are non-zero. `0x3BEA` turns a menu list of 9 entries into 10 (`sub_000C8010`) | Which menu is not known |

No flag for hidden characters has been identified. A fresh save already has 2 in every costume byte, which is the maximum for all but Zack, Ein and Kasumi, so the unlock only adds their third group.

## Costumes on the disc

`survey_models_in_loadfile` and `survey_roster_and_gaps` list them. `loadfile.afs` has 1102 entries and 96 models, 69 of which sit in a character's range with a CAT that reads, and all 69 are in the viewer. The others are after Bayman's range, have placeholders where a CAT would be, and look like stages, going by their sizes and object counts; they were not opened.

| Character | Models | Offered by the menu |
| --- | --- | --- |
| Zack, Tina, Ein, Ryu, Helena, Leon, Hayate, Ayane, Brad Wong, Christie, Bayman | 3 each | all 3 |
| Gen Fu | 2 | both |
| Lei Fang | 5 | all 5 |
| Jann Lee | 4 | all 4 |
| Bass | 4 | 0, 2 and 3, so not 1 |
| Kasumi | 10 | 0 to 8, so not 9 (entry 345, smaller than the others) |
| Hitomi | 8 | 5, 4 and 2, so not 0, 1, 3, 6 and 7 |
| Genra | 2 | no menu list |
| Chin | 1 | no menu list |

IDs 10, 15, 16, 21 and 22 (Hakkyoku-ken, K-1, Pro Wrestler, Taekwondo, Demi Human) have no menu list and point at the files of Helena or Kasumi, so they have no models of their own. Tengu has its `.bin` and motions at entries 656 to 658 and only placeholders after them, so no model here.

DOA2U's `datadvd.afs` holds 25 more models in DOA3's format, the `A0` series, which are not byte for byte any of the 96: see "Costume files" in `docs/doa2u-notes.md`. They share part of their pieces and textures with DOA3's models, which is how `nin`, `sui` and `gok` were identified as Hayate, Brad Wong and Christie. They are listed under DOA2U in the viewer.

## Status and plan (2026-11-03)

Known defects:

- Soft body type `0xFA` is not ported, nor ring mode 1 of type 4, nor knocking off worn objects, nor the knife in Kasumi's hand.
- Tina's forearm bands look oversized; not compared with the game yet.
- Hitomi's costume 07 gloves use a wrong texture. Layer texture 0x888 is not in the XPR.
- Tengu has no model in `loadfile.afs`.

How the Rust port would continue:

1. Compare the viewer's joints against the live game on more characters.
2. Port the remaining soft body types, using the routines above and live memory to verify them.
3. Port the rest of the face channels and how moves drive them.
4. Move to DOA2U with github.com/NoRain211/doa2u-re.

The viewer mode study is in `docs/viewer-mode-feasibility.md`.
