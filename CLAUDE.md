# DOA Player

Native Linux viewer in Rust (wgpu 22, winit 0.30, egui 0.29, glam) for Dead or Alive 3, DOA2U, DOAX, DOAX2 and DOA4 characters. The user writes in Spanish; answer in Spanish.

## Rules from the user

- Work on `main`, never `develop`. Assets live in gitignored `assets/`.
- Do not throw away existing work.
- Reading game memory is fine, read-only. Never write to it.

## Where things are

- `docs/doa2u-notes.md`: DOA2U formats and cipher. `docs/doa3-internals.md`: DOA3 internals. `docs/viewer-mode-feasibility.md`: reading live memory.
- `src/formats/doa2u.rs`, `doa3*.rs`, `doax.rs`, `doax2*.rs`, `doa4.rs`, `model.rs`: parsers. `src/app.rs`: roster and loading. `src/renderer/mesh.rs`: pose and skinning.
- `tools/re/`: `doa2u_crypt.py` (decrypt, AFS), `doa2u_obj_dump.py` (dump an OBJ), `doa2u_live.py` and `doa3_live.py` (read-only memory oracles; more under "Tools for this work"), `shot.sh` (render to PNG). DOA2U data the viewer reads: `assets/doa2u/` (`DOA2.xbe`, `datahdd.afs`, `datahdd2.afs`, `datadvd.afs`, `voice.afs`), copied from the install. Other archives, such as `bgm.afs`, are only in the install.
- DOA2U install: `/home/pandi/Ports/Apps/DeadOrAlive2Ultimate-Doa2uRe/private/`, through a link to the Stuff disk, which is slower. `play-disc/DOA2/` has the archives, `program/runtime-sources/recomp_00NN.c` the generated C. The user's builds may rewrite `generated`. The public repo github.com/NoRain211/doa2u-re has no game code.
- DOA3 generated C: github.com/parkerallan/doa3-recomp, `doa3/src/game/recomp/gen`.

## Commands
```bash
cargo test
DOA2U_DISC=assets/doa2u cargo test -- --ignored --nocapture    # all encrypted costumes
./build.sh --run --load "Zack" 0 --doa2u
EXTRA="--doa2u" tools/re/shot.sh out "Zack|0|0|200|3|1"
cargo build --profile fast                              # quick test build, target/fast/doa_player
cargo test --profile fast                               # quick tests; build.sh stays optimized
BIN=target/fast/doa_player EXTRA="--doa2u" tools/re/shot.sh out "Zack|0|0|200|3|1"
EXTRA="--doax" tools/re/shot.sh out "Tina Armstrong|0|0|200|3.2|1"                 # a DOAX suit and motion
EXTRA="--doax2" tools/re/shot.sh out "Tina Armstrong|2|0|200|2.2|1.1"              # a DOAX2 suit and motion
EXTRA="--doa4 --hair 2" tools/re/shot.sh out "Kasumi|0|1|200|3.6|0.9"              # a DOA4 costume and motion, with her third head file
EXTRA="--doa2u --view hands" tools/re/shot.sh out "Helena Douglas|0|0|200|3|1"   # frames a part of the pose
EXTRA="--doa2u --mesh-only" tools/re/shot.sh out "Zack|0|0|200|3|1"              # also --no-textures

target/fast/doa_player --load "Zack" 0 --doa2u --export [--all-motions]            # writes exports/\<game>/\<character>/\<costume>.glb

target/fast/doa_player --export-all                                                 # every costume, no visible window; DOA2U and DOA3 about 3.5 minutes, DOAX far longer
target/fast/doa_player --export-all --doax                                          # only DOAX
target/fast/doa_player --export-all --doax2                                         # only DOAX2, not measured for time or memory

target/release/doa_player --install-icon                                            # icon and desktop entry in ~/.local/share, needed on Wayland

```

## Current state of DOA2U (2026-11-04)
Measured against the running game; see `docs/doa2u-notes.md` for numbers and code addresses.

- Pose: limbs, hands, forearm twists, upper arms and motion tracks. `doa2u::solve_pose`.
- Hand shapes: `doa2u::hands`, from `apv` entries 0 and 1, driven by the move scripts in `.bin`.
- Joint pieces: measured on `kas00` only. Shoulders are off by up to 2.6 cm, unexplained.
- Cloth (kind 2), chains (kind 1, hair), extra objects (kind 3) and vertex copies (kind 250) are in the viewer. Single steps match the game on most sheets and chains, not all; see the table in the notes. Wind was 0 in those captures.
- `nin`, `sui` and `gok` are Hayate, Brad Wong and Christie, with DOA3's bones and motions.
- Chest spring: Tina, Helena and Lei Fang only. Measured on Helena and Lei Fang.
- Roster: Zack 0, Tina 1, Leon 8, Lei Fang 12, Hitomi 21, Bayman 24 measured in matches. Internal names differ: `sambo` is Bayman, `bayman` is Leon, `koryu` is Hitomi.

Characters face away at yaw 20 and the front is near yaw 200. DOA3 does the same, so this is the viewer's convention, not a pose bug.

## Next steps, in order
1. **Rest of the lighting.** All stages' lights come from the executable (see the notes). Open: drawing in the game's order, fog, timed lightning (a held flash is in the list), real point lights with a fighter position, and which areas change their table in code.
2. **Compare with the game.** Lighting was compared by eye on `hel08` and `kas00` only. Check `zac01`, `tin09` and `lei03`, for which screenshots exist, once the lighting is in. `kas00` hands after the normals fix.
3. **Remaining soft body error.** `hel00` chain 7 (5 mm) and chains 0, 1, 6, 8 (under 0.4 mm), and shoulders (up to 2.6 cm, not looked at again). The head strips and `tin09` cape match on 1 pair each; a new capture should confirm it, because the rest of their pairs are not one step apart. Wind was 0 in the cloth captures, so it is untested.
4. **Other soft body kinds:** 4, 5, 7, 8, 9, 12 and 14. They need those costumes in a match. Kind 0xFE variants are read but only variant 0 is shown; the notes list what `sub_0005B050` tests, but not what those values mean in play.
5. **Hands against scripts.** Shapes match the game; scripts were not compared frame by frame. Motions with no script open the hand, where the game probably keeps the last pose. Pose 0's character substitution was not found.
6. **DOA3 lighting.** Left for a session with only DOA3 running: the user has one controller. Do not assume it works like DOA2U.

## Tools for this work
- `tools/re/doa2u_live.py`: read-only guest memory. `capture` writes pose frames.
- `tools/re/doa2u_capture_objects.py`: records the objects the game rewrites.
- `tools/re/doa2u_cloth_pairs.py`, `tools/re/doa2u_chain_pairs.py`: consecutive particle frames. They wait for quiet, because reads otherwise catch sheets half written.
- `tools/re/doa2u_copy_check.py`: checks kind 250 copies against a capture.
- `tools/re/doa2u_unlocks.py`: prints the save's unlock bytes from the running game.
- `tools/re/doa2u_lights.py`: lists the D3D lights the game enables.
- `tools/re/doa2u_stage_lights.py`: lists the light records in the executable.
- `tools/re/doa2u_normal_check.py`: per object in a capture, how many positions and normals the game changes.
- `tools/re/doa2u_fn.py`: prints a generated C function without flag bookkeeping.
- Ignored tests: `list_costume_objects`, `joint_pieces_match_captured_vertices`, `captured_tracks_are_found_in_motions`, `cloth_follows_captured_vertices`, `doa2u_single_steps`, `doa2u_chain_single_steps`. Each reads its inputs from environment variables, named in its comment.
- Captures are in `target/captures/`, not tracked. `cargo clean` deletes them, and the tools above recreate them.

## Exporting to Blender
`src/export.rs` writes a `.glb` per costume: a mesh per piece, an armature of 25 bones with the pose solve's hierarchy, skin weights, embedded PNG textures, and baked motions at 60 frames per second. `exports/` is gitignored: the files hold game data and must not be committed or published. The Costume and All costumes buttons are in the character panel, and All costumes does one costume per frame so the window keeps responding. Checked by importing in `blender -b`: DOA2U, an `A0` model and DOA3, and all 698 motions of `zac00` (25 MB, 8 s); a frame of `zac00` was compared with the viewer by eye.

DOAX exports its 32 bones from the rest pose, arms out, with hands as shape keys and blended pieces by their weights. Checked in `blender -b` on two costumes, and one frame of `tina 0077` by eye against the viewer. Bulk export over DOAX was measured for 97 s only: 227 costumes, memory flat.

Soft body animations are not exported for DOA3 or DOA2U yet. Soft-body-driven pieces currently remain attached to their bones and retain their undeformed shape from the files. Exporting their animated deformations is still pending.

Objects that chains draw whole are exported where they hang in DOA3 (placed at load), but at their own rest node in DOA4, where the viewer places them each frame (2026-11-12, not checked in Blender).

DOA3 and `A0` hand poses and facial animations are also not exported. DOA2U hands are exported as shape keys, driven by the move scripts.

The chest spring is exported for all four games as shape keys weighted per frame by the spring, which starts at rest on each motion's first frame and follows the Bounce slider. DOAX: its sets on the chest, upper arms and suit (checked in `blender -b` on `mdAy00tn`). DOAX2: the game's three springs (read from its code) on all six shapes of each chest piece, as three sets of base, positive and negative keys (checked on Tina `on_s_m_1`, 2026-11-10). DOA3 and DOA2U: the chest groups as base, positive and negative keys, blended linearly as `deform_piece` does (checked on DOA3 Tina 0 and `hel00`, 2026-11-10). DOAX, DOA3 and DOA2U: only the vertical axis, as in the viewer.

Do not run a visible window for long from tests: a window that does not process events looks hung on the user's desktop.

GPU memory does not show in any process's memory use, and `--export-all` runs in one loop with no frames. Without a submit and a poll per costume it grew 28 MB per costume until the desktop froze (2026-11-05). When changing a bulk path, watch `MemAvailable` and `mem_info_vram_used`/`mem_info_gtt_used` under `/sys/class/drm/card*/device/`, and kill the run below a floor. The All costumes button goes through frames: 49 costumes of Lei Fang left VRAM flat (805 to 861 MB).

## Development notes
- Anime shading (`cel_shaded`, `vs_outline` in `mesh.rs`) is the viewer's own and applies to all games: two cool shadow tones from the strongest light, a hard highlight, a thin rim, and an inverted hull ink line. The user asked for something more worked, in the style of Fire Emblem. Its values were chosen without seeing them against that game.

- Framing: the Background panel's View buttons, or `--view full|head|hands|feet`, frame from the posed character.

- `datadvd.afs` costumes and the extra characters are in, see "Costume files" in the notes. Not compared with the game.
- Lighting: DOA2U is done as far as the notes say. DOA3 keeps the viewer's own shading; its materials may work the same way, but this is not checked.

## Unverified in DOA2U
- Only limbs, pelvis, chest, hands and motion tracks are compared with the running game. Joint pieces are compared on `kas00` only. Head, feet on slopes and drawing are not.
- Character IDs in `app.rs` are inferred from tables. Measured in matches: Zack is ID 0 (`zack.mot`, `zac00`), Bayman 24 (`sambo.mot`, `bay00`), Leon 8 (`bayman.mot`, `leo00`), Hitomi 21 (`koryu.mot`, `kor00`). Tina is ID 1 and Lei Fang 12, without checking their files. The other IDs are inferred.
- Blend slots 18, 22, 134, 135 and objects with 2 or 3 extra floats are not handled. Blend table entries using matrices 16, 19, 20, 23, 30, 31 are unmapped.
- Plain `stage_XX.xpr` files do not parse with the costume parser.

## Backlog
1. DOA2U: stages, other soft body kinds.
2. DOA3: other face targets, ring mode 1 in tubes, the knife in Kasumi's hand, real stage wind, vertex normals (6 to 12 percent of the file's are over 60 degrees from their surface, measured on Kasumi, Ayane and Bayman with faces turned to the file's side; an earlier count of 40 to 53 percent was wrong, as it took strips' alternating winding as the surface's side. The viewer works normals out from the posed triangles each frame for materials without the game's lighting, over the whole model, in `smooth_normals`), 0xFA sections, visual defects (Tina's forearm bands, Zack's chin item).
3. Kasumi's DOA3 fringe: the user thinks it looks the same as in the game (2026-11-04). No change needed unless a closer comparison shows otherwise.
4. Complete chest bounce animations: implement the remaining two bounce axes in the viewer and support exporting the complete bounce animation to Blender, including all axes and shape deformations. Export of the vertical axis is done for all games (2026-11-10). DOAX2's three axes are in the viewer and the export, from the game's code (see "Chest shapes" in `docs/doax2-notes.md`); open there: which pieces use the second weight set (point 1). **Pending, needs the user:** put Tina, Lisa or Helena on screen in DOAX2, capture with `tools/re/doax2_live.py array target/captures/doax2_live.txt`, then run the ignored test `live_spring_bones_match` to compare bones 0, 8 and 14 with the game frame by frame (their axes and roll already match, 2026-11-10). The sideways and forward axes exist only in DOAX2's files.
5. Voices: DOA2U's clips play from the Voices window. Not done: DOA3's clips, which have no names, and playing clips with moves.
6. DOAX: support Dead or Alive Xtreme Beach Volleyball in the viewer, asked for by the user (2026-11-05). Install: `/home/pandi/Ports/Apps/DeadOrAliveXtremeBeachVolleyball-DOAXVBNativePCPort`. In the viewer: 8 characters with suits and motions, data in `assets/doax/`. The motion evaluator is confirmed against the game, and joints and blended pieces are in. The pose solve matches the game on every bone (median 0.001 in a matrix element over a 3500-sample demo capture, 2026-11-11); bone tables are measured for all eight characters. Findings in `docs/doax-notes.md`; `tools/re/doax_live.py` and `doax_capture.py` read its memory. Do not assume its formats match DOA2U or DOA3.
7. DOAX2: Dead or Alive Xtreme 2 (Xbox 360), asked for by the user (2026-11-06). Install: `/home/pandi/Ports/Apps/DeadOrAliveXtreme2-DOAX2Recomp`, data copied to `assets/doax2/` (with a 32 MB copy of the game's image from memory). `src/formats/doax2.rs` reads XPR2 models, the node tree, Xbox 360 textures and CAT draw lists by body type; the viewer shows body, head and suit, plays the motions with DOAX's solve, blends weighted pieces by an inferred table, and drives the chest's six shapes with the game's three springs (`--doax2`). Export writes a node per piece, with the chest shapes as shape keys. `tools/re/doax2_live.py` reads bone arrays from the running game; `tools/re/xbox360_ppc.py` disassembles the image with VMX128; `tools/re/doax2_shaders.py` lists and disassembles its Xenos shaders. Findings in `docs/doax2-notes.md`. Order set by the user (2026-11-07): 1 complete chest bounce animations (remaining axes in the viewer and full bounce export), 2 lights, 3 hair animation, 4 facial animation, 5 choosing hairstyles (done 2026-11-11: the panel's Hair row lists the character's heads in `chr.afs`), 6 the hand file per scene, 7 the game's material colours. Then DOAX: hand intermediate shapes, arm roll, light sets per stage. Details: lights (the vertex shaders' lighting is decoded, see "What the shaders do with them" in the notes; open: how the CPU folds the materials' colours into the constants, needs the game running; shader constants found: `DirLightVec` c71-74, `DirLightCol` c75-78, `AmbientLightP1/P2` c68-69, point and spot lights after; a table of light sets lives in the guest heap from `0x4007E280`, 0x1E0 bytes a set: ambient, 4 directions, 4 colours; its source file is being looked for), hair animation (in the viewer since 2026-11-10 for heads with cloth, `src/formats/doax2_cloth.rs`: the game's velocity step and thread caps, every collider shape checked against the game's matrices, collisions on the move's segment, the threads' angle limits from the code; the hair's other vertices are rigid with the head, as only fixed particles share triangles with them, test `layout_two_writers`; open: what the game does after a hit, the 14 small particle-less pieces on Kasumi's `AA`, export; suit cloths (skirts, sleeves, `base`, `tare`; a set per body type in the suit CAT's `tdpack`) run through the same simulation since 2026-11-11, by eye only; open there: collider type 9, the `base` and `sode` roots following the chest's shapes, the item cloth `zgra_pre`, see "Suit cloths" in the notes; Helena's ponytail has no cloth and sticks out rigid), facial animation (blinks done from script op 22; the mouth's vowel shapes are read, their trigger is not). Then: other heads, which hand file per scene, the game's material colours. Poses are checked live (Tina, Lei Fang).
8. DOA4: Dead or Alive 4 (Xbox 360), asked for by the user (2026-11-10). Install: `/home/pandi/Ports/Apps/DeadOrAlive4Online-DeadOrAlive4Online`, read in place from `game/` (`DOA4_GAME` overrides it; nothing is copied to `assets/`). The game did not run on the user's machine on 2026-11-11, so everything is from the files and nothing is compared with it. In the viewer (`--doa4`): all 160 costumes with textures, heads attached for costumes without one, plus the 34 DOAX2 swimsuits the port builds (loose files in its Windows prefix, `DOA4_SUITS` overrides the folder). `src/formats/doa4.rs` reads the CAT's draw list (a byte stream, not DOAX2's) and uses DOAX2's XPR2 reader, whose headers are 12 bytes shorter here. Findings in `docs/doa4-notes.md`. Motions play (2026-11-11): `mot.afs` is DOA3's layout in big-endian and the 44 tracks go through `doa3::solve_pose` unchanged, with the skeleton from the rest pose and a fixed frame per limb bone (`doa4::rest_frame`); stances and strikes look right by eye. Joint pieces (`91` draws) blend by the weights in their vertices, own bone then parent. Next, in order: 1 the chest's joint piece (since 2026-11-12 its first two weights are the chest and the pelvis, from 111 of 116 costumes; the third and fourth go half to each, a guess; the knees' third weight and the forearm pieces on bones 32 and 33 are guesses too, see the notes), 2 the other soft body kinds (hair chains, kind 1, run through DOA3's simulation since 2026-11-11: the CAT's word 2 is DOA3's section list in big-endian; springs, kind 5, likewise; soft bodies take each solved bone turned back by `doa4::rest_frame`; cloth, kind 2, is DOA2U's record with two fields moved, read by `parse_cloth_records_doa4`; kind 250 copies only feed the lower levels of detail; kinds 0 and 3, worn objects and overlays, are DOA3's sections 0 and 3 and drawn since 2026-11-12; kind 254 is DOA2U's 0xFE, a variant of the record before it (skipped, as in DOA2U); the 10 sheets the parser refuses are lower levels of detail (their objects carry 0x1000); next kinds 7, 9, 14, 15 and 251; chains that draw whole objects are in since 2026-11-12 for DOA3, DOA4 and DOA2U's costumes in DOA3's format, by `sub_0003AA20` (DOA2U's `sub_00050100` differs in mode 2), except DOA4's modes 5, 8 and 31, whose meaning is not known, and DOA2U's own costumes, 126 more), 3 faces and chest shapes (the `MPV` sets at CAT word 3 are read; hands follow the scripts of `anm.afs`, DOA3's `.bin` in big-endian, since 2026-11-11, with pose p taken as entry p + 1; the face blinks and moves its mouth on DOA3's timing as a stand-in; the chest's set is not used), 4 which hairstyle each of the game's own costumes wears (the panel's Hair row and `--hair N` pick any head, in DOA4 and DOAX2, since 2026-11-11), the loose draws (0xA0 to 0xA2), tag motions, export of motions.

## Development principles
- Costume discovery should cover all available archives and variations, not just those exposed by the original game menus. Keep archive enumeration independent from the original select screen roster.
- Keep format parsing comprehensive. Document all known CAT record types, sections, matrix slots and vertex layouts, including those not currently used by the viewer.
- Do not assume unimplemented sections or data fields are irrelevant. Investigate their purpose before deciding whether they can be ignored.
- Treat implementation limitations and technical assumptions as provisional. Do not assume hardcoded character lists, supported modes or rejected record variants reflect actual game limitations. Investigate whether they are imposed by the game, the file format, the current implementation or simply missing research.
- Treat inferred character IDs, material flags and rendering behavior as provisional until confirmed against the game or additional evidence.
- Do not assume DOA2U and DOA3 share identical animation, skeleton or rendering behavior just because they share some data structures or code.
- Verify shared DOA2U/DOA3 formats and structures at the semantic level, not just by comparing their binary layouts.
- Prefer game-specific implementations when the underlying formats or behavior differ.
- Keep future features in mind when making architectural decisions. Deferring a feature does not mean designing the architecture to exclude it.
- Existing priorities should guide development, not prevent independent investigations or improvements.
- Partial implementations should remain extensible. Supporting a subset of characters, animation channels, record types or simulation modes must not prevent adding further cases.
- Distinguish between behavior verified against the running game, behavior inferred from measurements and behavior that has only been implemented without comparison.
- Do not consider a system fully implemented when only a subset of its known variants or behaviors is supported.
- Keep DOA3 and DOA2U development independent. Progress on one game should not unnecessarily block work on the other.
