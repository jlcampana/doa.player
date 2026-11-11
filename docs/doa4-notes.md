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
root goes on that node. Which hairstyle goes with which costume is not known: the viewer takes
the first.

## Roster

By file prefix; names inferred, not read from the game's tables: `kas` Kasumi, `aya` Ayane,
`ryu` Hayabusa, `hyt` Hayate, `ein` Ein, `hel` Helena, `hak` Kokoro, `kor` Hitomi, `lei` Lei
Fang, `tin` Tina, `lis` La Mariposa, `cri` Christie, `boy` Eliot, `jan` Jann Lee, `zac` Zack,
`bas` Bass, `bay` Bayman, `leo` Leon, `bra` Brad Wong, `gen` Gen Fu, `tng` Tengu, `alp`
Alpha-152, `spa` Spartan-458, `rtm` unknown. Seen in the viewer and recognizable: Kasumi, Zack,
Tina, Kokoro (`hak00`), Hayabusa, Helena, the Spartan.

## In the viewer

`--doa4`, or the DOA4 button: a costume in the files' rest pose with its textures, lit by the
viewer's own stage lights. All 160 models load (`doax2_readers_on_costumes`, ignored test).

Not done: motions (`mot.afs`), the lower levels of detail, the loose draws, groups and
conditions, hairstyle choice, the CAT's other sections (cloth, faces and hands are likely among
them), materials beyond what DOAX2's reader takes, stages, export checks.

## Tests

Ignored, reading the install: `doax2_readers_on_costumes` (every model), `costume_objects`
(layouts, draws and buffers of one, `DOA4_COSTUME="chrone.afs zac00"`), `costume_nodes` (its node
tree with each node's bone).
