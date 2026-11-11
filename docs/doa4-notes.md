# Dead or Alive 4 notes

Xbox 360, read from the install of a recompiled build
(`/home/pandi/Ports/Apps/DeadOrAlive4Online-DeadOrAlive4Online/game`, or the folder in
`DOA4_GAME`). Everything here comes from the files alone (2026-11-11): the game did not run on
the user's machine yet, so nothing is compared with it. Code: `src/formats/doa4.rs`, on top of
DOAX2's XPR2 reader in `src/formats/doax2.rs`.

## Archives

AFS, as the other games. `chrone.afs`, `chrtwo.afs` and `chrthree.afs` hold the models, each a
`.tpr` (XPR2) followed by its `.cat`; `dmy.bin` entries of 16 bytes (`wwww`) fill unused places.
160 models: costumes `kas00`, `kas01`... and heads with a hairstyle `kasAA`, `kasBA`... Some
names repeat (`leiCA` seven times, `helAA` five).

Others, not read yet: `mot.afs` (`zack.mot`, `m_zack.mot`, `c_zack.mot` per character, with
DOA3's and DOA2U's internal names: `karate`, `hakyo`, `ninja`...), `anm.afs` (a `.bin` per
character), `cmn.afs` (`comon.mot`, `rtmlight.bin`, `cam_raw.bin`), `stg.afs` (stages, a `.tpr`
and a `_dat.bin` each), `avt.afs` (lobby avatars), `spr.afs` (screens), `snd.afs` and `SOUND/`
(XACT wave banks per character).

## Models

XPR2 with the same `MdlGeo`, `ObjGeo`, `GeoDecl` and `Hie` blocks as DOAX2: objects, vertex
layouts, draws, materials and the node tree read with DOAX2's code unchanged. One difference:
the resource headers are 12 bytes shorter. A buffer's is 0x14 bytes with its fetch constant
(address, size) at +0xC, against 0x20 and +0x18 in DOAX2; a texture's fetch constant is at +0x10
against +0x1C. `Xpr2::parse` tells them apart by the size of the first buffer's header.

A costume is one file with the whole character, where DOAX2 has a body, a head and a suit. It
holds three levels of detail (`zac00`: objects 31 to 67 the full one, 16 to 30 and 1 to 15 the
lower ones, 63607 vertices in the first).

Node tree of `kas00` (70 nodes): two roots, the pelvis at the origin and the chest at (0, 0.18,
0.005), each with its limbs as children; the same skeleton again twice more under nodes 0 and 1
for the lower levels. The arms are out to the sides at rest.

### Materials

A draw's stages are (stage, kind, texture) as in DOAX2. Seen in DOA4: `(0, 0, t), (1, 1, none)` on
simple draws; `(0, 1, a), (1, 1, b), (2, 6, c)` on most of a costume, where c is a normal map
(mean colour (128, 128, 255)) and the viewer draws a alone; `(0, 3, none)` on one object of a
head. Hair strands use the three-stage form with a pair of textures in either order, one pure
white with the strands' shape in its alpha and one with the hair's colour (`hakAA`: 8 and 9).
`doa4::load` joins such a pair into one texture; before that Kokoro's hair drew white (the
user's report, 2026-11-11). What stage 1 is on the other draws (often one texture shared by a
whole costume) is not known. The ignored test `costume_textures` writes a model's textures as
PNGs to `target/captures/`.

## CAT

The directory is DOAX2's (8-byte entries from the offset in word 1: `0x0100`, the kind shifted
by 8, an offset). Kinds seen: 2 in all 160, 5 in 159, 4 in 147, 6 in 113, 9 in 44, 3 in 20, 8 in
4, 1 in one. Only kind 2 is read.

Kind 2, the draw lists, is a byte stream here, not DOAX2's table of lists:

| Bytes | Meaning |
|---|---|
| `90 bone object` | A draw. One object per bone, the first run of every list. |
| `91 bone object` | A draw. Follows the `90` run with a second object per bone; on a body without a head it is the only draw on bone 1 (the neck). |
| `93 bone object` | Spartan-458 only. |
| `A0`, `A1`, `A2 n object` | Draws whose number counts up over a run (Jann Lee: `A1` 0 to 7, then `A0` 0), so it may not be a bone. Zack's are a coil of rope and a long line that sit 2 m under the floor on their nodes at rest: something else places them. Left out of the viewer. |
| `D4 bone object 00` | `hel05` only. |
| `47 n` | Starts group n; `07` ends the groups. `zac00` has four: 0 and 1 with the same draws, 2 and 3 with the same. The viewer draws the first. |
| `60 n` | Before one or more `90` draws. Jann Lee: `60 20`, two draws, `60 21`, the same two. `hel05`: `60 10` with one object, `60 11` with another on the same bone. Read as alternatives; the viewer draws the first. |
| `04`, then `02` | The next level of detail follows. |
| `00` | The end. |

What `90` and `91` mean is not known; in DOA3 and DOA2U a bone has a rigid piece and a joint
piece, which would fit.

Bones, from where `kas00` puts their objects: 0 chest, 1 head, 2 pelvis, 6 and 12 thighs, 7 and
13 shins, 3 and 9 feet, 8 and 14 upper arms, 4 and 10 forearms, 5 and 11 hands (the lower number
at +x). Also used: 16, 17, 20, 21 (17 and 21 the eyes, in the head files), 24, 25, 32, 33.

## Heads

A costume with a head has a `90` draw on bone 1 (Zack, Tina). One without (Kasumi) has only the
`91` neck there, and the head is a separate file with a hairstyle: `kasAA` to `kasDA`, `leiAA` to
`leiGA`, `lisAA` to `lisFA`... The head file's root is at the origin and its eyes at (±0.03,
0.111, 0.077); the body's eye nodes are at the same offsets from its bone 1 node, so the head's
root goes on that node. Some head files have that root at the origin (`kasAA`), others already at
the body's place, (0, 0.468, -0.025) in `ayaAA`: the viewer takes the head file's own root out
first (Ayane's head floated above her trunk before, the user's report). Which hairstyle goes with which costume is not known: the viewer takes
the first.

## DOAX2 swimsuits

The port can build DOAX2's swimsuits for DOA4 from the user's DOAX2 copy (Esc menu, Extras). It
writes them as loose DOA4 models, outside the install: `doax2_suits/` in the Documents folder of
its Windows prefix (`~/.local/share/com.quiverlauncher.app/proton/DeadOrAlive4Online-.../pfx/
drive_c/users/steamuser/Documents/dead_or_alive_4/`, or the folder in `DOA4_SUITS`), 34 pairs
named after DOAX2's suit and colour (`vtn_cs_e_0.tpr`, `.cat`) and a `manifest.txt`:

    suit <character> <own:N | append> <tpr entry> <cat entry> <head tpr> <head cat> <suit> <colour> <tpr> <size> <sha1> <cat> <size> <sha1>

The character is the game's number: 1 Tina, 5 Kasumi, 7 Helena, 10 Kokoro, 12 Lei Fang, 13 Ayane,
15 La Mariposa, 20 Christie, 21 Hitomi. The head entries count over the three costume archives
one after another (509 is `chrthree.afs` entry 308) and are `-` for Tina, whose suits carry her
head. They tell which hairstyle the port pairs with the suits; the game's own costumes' pairing
is still not known. The built lists have one level of detail. Helena's have `60 60 11 D4 14 10
00` where `hel05`, their template, has `60 10 90 14 01 60 11 D4 14 10 00`; the reader skips the
first 0x60.

The viewer lists them after each character's own costumes (`swimsuits_load`, ignored test).

## Roster

By file prefix; names inferred, not read from the game's tables: `kas` Kasumi, `aya` Ayane,
`ryu` Hayabusa, `hyt` Hayate, `ein` Ein, `hel` Helena, `hak` Kokoro, `kor` Hitomi, `lei` Lei
Fang, `tin` Tina, `lis` La Mariposa, `cri` Christie, `boy` Eliot, `jan` Jann Lee, `zac` Zack,
`bas` Bass, `bay` Bayman, `leo` Leon, `bra` Brad Wong, `gen` Gen Fu, `tng` Tengu, `alp`
Alpha-152, `spa` Spartan-458, `rtm` unknown. Seen in the viewer and recognizable: Kasumi, Zack,
Tina, Kokoro (`hak00`), Hayabusa, Helena, the Spartan.

## Motions

`mot.afs` has, per character, `zack.mot`, `m_zack.mot` and `c_zack.mot`, plus tag files
(`t_ay_ks.mot`...), under the series' internal names (`doa4::motion_file`: `ninja` Hayate,
`karate` Ein, `hika` Helena, `hakyo` Kokoro, `koryu` Hitomi, `gokei` Christie, `suiken` Brad
Wong, `sambo` Bayman, `bayman` Leon; the last two as in DOA2U, not checked here).

The layout is DOA3's in big-endian:

- The file starts with a table of block offsets, one per motion number; unused numbers point at
  a placeholder block, the first word's. The table's length is the placeholder's first track
  offset over 4 (1002 entries in `zack.mot`, 377 of them motions).
- A block is 44 track offsets, DOA3's count. Tracks may be shared, so the offsets do not always
  rise; a block is taken when every track ends inside the file.
- A track is DOA3's key records: a 16-bit head with the kind in its low 2 bits and the frames to
  the next key in the 10 above; sizes 10, 4, 6 and 8 bytes. The two 24-bit slopes of a kind 0 key
  are read as if each 16-bit word had been swapped (high bytes at +7 and +6): a guess.

The 44 values go through DOA3's pose solve (`doa3::solve_pose`) unchanged: the draw lists' bone
numbers are DOA3's nodes (upper arm 8, forearm 4, hand 5; thigh 6, shin 7, foot 3; 14, 10, 11 and
12, 13, 9 on the other side; 17 and 21 the eyes). The skeleton's 17 offsets come from where the
bones sit in the files' rest pose (`doa4::skeleton`).

DOA3's pieces are modelled in the solve's frames; DOA4's are modelled standing with the arms
out and every node unturned. So each piece is moved by the solved node times the inverse of the
frame the solve would give that bone in the rest stance (`doa4::rest_frame`): the identity for
chest, head, pelvis, hands and feet, and for a limb z along it with the roll set by taking the
elbows to bend back and the knees forward. With that, the first motions of Kasumi, Zack and
Tina are their fighting stances and strikes, with limbs meeting at the joints (by eye,
2026-11-11). Nothing is compared with the game.

Joint pieces: the `91` draws carry blend weights in their vertices (usage 1, two or three
floats, as DOAX2's blended pieces). The first weight is the bone itself and the second its
parent: in `kas00` the vertices heavy on the first sit down the limb from the joint, those
heavy on the second towards the body (ignored test `costume_weights`). The viewer blends them
linearly between the two solved bones, and knees, elbows, shoulders, hips and neck bend without
lumps (by eye). A third weight on the elbow pieces is taken to be the forearm's twist node; no
vertex of `kas00` is heavy on it. Drawn rigid still, each logged when loaded: knee pieces with
three weights (Zack's and Kokoro's trousers), pieces on bones 32 and 33, and the chest's `91`
piece, whose weights are of another type (`0x1A2360`) that the reader does not take.

Missing: hair and cloth do not move; hands and faces have no shapes; pieces on bones the solve
does not have (16, 20, 24, 25, 32, 33) follow the nearest node above them; the tag motions and
`comon.mot` are not listed; `rtm` has no motion file of its own.

## In the viewer

`--doa4`, or the DOA4 button: a costume with its textures, lit by the viewer's own stage
lights. All 160 models load (`doax2_readers_on_costumes`, ignored test).

Motions play from the animation list (the character's three files). Not done: joint bending, the lower levels of detail, the loose draws, groups and
conditions, hairstyle choice for the game's own costumes, the CAT's other sections (cloth, faces and hands are likely among
them), materials beyond what DOAX2's reader takes, stages, export of motions.

## Tests

Ignored, reading the install: `doax2_readers_on_costumes` (every model), `costume_objects`
(layouts, draws and buffers of one, `DOA4_COSTUME="chrone.afs zac00"`), `costume_nodes` (its node
tree with each node's bone).
