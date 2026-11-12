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
EXTRA="--doa2u --variant 1" tools/re/shot.sh out "Kasumi|0|0|200|3|1"           # records' variant 1 (0xFE), e.g. the sword in the hand
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
1. **Rest of the lighting.** All stages' lights come from the executable (see the notes). Open: drawing in the game's order, fog, the lightning's own rule (the storm now flashes on timing measured in a match, 2026-11-12), real point lights with a fighter position, and which areas change their table in code.
2. **Compare with the game.** Lighting was compared by eye on `hel08` and `kas00` only. Check `zac01`, `tin09` and `lei03`, for which screenshots exist, once the lighting is in. `kas00` hands after the normals fix.
3. **Remaining soft body error.** Chains match with wind, `hel00` chain 7 included (2026-11-12, storm stage). Cloth with wind: every sheet of `hel00` matches at 0.00 mm with Helena still; her coat's sheets are 6 to 9 mm off only while she is thrown or hit (the game seems to hold the coat then, not found in the code); Gen Fu's sheets 0.5 mm in a fight. Springs (kind 5) match at 0.001 mm. Shoulders (up to 2.6 cm, not looked at again). The head strips and `tin09` cape match on 1 pair each.
4. **Other soft body kinds:** kind 5 (springs, 282 records) is DOA3's spring, read from the code and running since 2026-11-12, checked live at 0.001 mm. Kinds 8 (Helena's item and chain 9, live only in some phases of a round), 9 (a trail effect), 12 (a particle emitter) and 14 (an object aimed at a body point) are read from the code and documented in the notes ("Springs, kind 5" and after); none is a soft body, none is ported. Left: 32 (`pdg`). Kind 0xFE variants are read but only variant 0 is shown; the notes list what `sub_0005B050` tests, but not what those values mean in play.
5. **Hands against scripts.** Shapes match the game; scripts were not compared frame by frame. Motions with no script open the hand, where the game probably keeps the last pose. DOA2U has no per-character substitution of pose 0 (DOA3's `sub_0004B63C`; checked 2026-11-12).
6. **DOA3 lighting.** Left for a session with only DOA3 running: the user has one controller. Do not assume it works like DOA2U.

## Quick wins for the next session (2026-11-12)

Each is small and needs no game running. In rough order of value for effort:

1. ~~Point lights in the viewer.~~ Done 2026-11-12: `SceneLights::points` and `fighter`, snowy night uses them (`snowy_night_lights`). Not compared with the game by eye. The storm's flash could use the same list once its light type is read.
2. ~~DOA4 export of the face.~~ Done 2026-11-12: eyes and mouth exported as shape keys at rest (`doa4::face_shapes`), not driven; checked in `blender -b` on `kas00` (2 pieces, 12 keys, up to 2.4 cm).
3. ~~DOA2U kind 0xFE variants in the panel.~~ Done 2026-11-12: the Variant row and `--variant N` (`doa2u::with_soft_body_variant`). Groups without that variant are dropped, a guess.
4. ~~DOA3 Kasumi's knife in the hand.~~ Done 2026-11-12, the same row (`doa3_chains::with_section_variant`): variant 1. DOA4's kind 254 goes through the same row since 2026-11-12 (`doa4::with_variant`); `kas00` has 3 variants, and no difference shows at full-body framing, not looked into.
5. **Stage light check by table.** Partly done 2026-11-12 (notes, "Who sets the fighters' lights"): a third table at `0x6D4D78` used under conditions not understood, and the stage object functions that rewrite the first fighters' light, found through descriptors at `0x6E4774`, `0x6E800C`, `0x78158C`. Left: which area each descriptor belongs to.
6. ~~Export check of DOA2U springs.~~ Done 2026-11-12: `kas02`, one motion, opened in `blender -b`: 11 pieces with keys, the largest move 41 cm (hair and ribbons over the motion), nothing flies off. Not compared with the viewer frame by frame.

Bigger, also code-only: DOA3's tube ring mode 1 (`sub_00044620` and four helpers, about 1000 lines of generated C; it also resamples the ring and changes its column count; a capture to check it is in `target/captures/doa3_hitomi_tubes.bin`, see `docs/doa3-internals.md`, "Tubes"); DOA2U kind 14 (needs the 32 body points per fighter at `0x8793F0`, probably from the collision capsules); DOA4 from its image (`tools/re/doa4_xex.py`): the chest's real spring, the face's driver, which hairstyle each costume wears, the props on bones 20 and 16.

Needs the game: DOA2U `ryu00` in a match (kind 14; the first costume in the menu is `ryu06`, tell costumes apart with the ignored test `costume_counts`), when Helena's chain 9 runs, hands frame by frame; DOA3 lighting; DOAX2 chest capture (`live_spring_bones_match`).

## Tools for this work
- `tools/re/doa2u_live.py`: read-only guest memory. `capture` writes pose frames.
- `tools/re/doa2u_capture_objects.py`: records the objects the game rewrites.
- `tools/re/doa2u_cloth_pairs.py`, `tools/re/doa2u_chain_pairs.py`: consecutive particle frames. They wait for quiet, because reads otherwise catch sheets half written.
- `tools/re/doa2u_copy_check.py`: checks kind 250 copies against a capture.
- `tools/re/doa2u_unlocks.py`: prints the save's unlock bytes from the running game.
- `tools/re/doa2u_lights.py`: lists the D3D lights the game enables.
- `tools/re/doa2u_stage_lights.py`: lists the light records in the executable.
- `tools/re/doa2u_normal_check.py`: per object in a capture, how many positions and normals the game changes.
- `tools/re/doa2u_fn.py`: prints a generated C function without flag bookkeeping. It works on DOA3's generated C too (run it from that folder).
- `tools/re/doa2u_spring_pairs.py`: consecutive frames of spring points, for `doa2u_spring_single_steps`.
- `tools/re/doa2u_light_positions.py`: the lights while each fighter draws, with the fighter's position.
- `tools/re/doa3_tube_capture.py`: DOA3's tube state with matrices and wind, for porting ring mode 1.
- `tools/re/doa4_xex.py`: DOA4's game image from `default.xex`, for `xbox360_ppc.py` with `IMAGE=`.
- Ignored tests: `list_costume_objects`, `joint_pieces_match_captured_vertices`, `captured_tracks_are_found_in_motions`, `cloth_follows_captured_vertices`, `doa2u_single_steps`, `doa2u_chain_single_steps`. Each reads its inputs from environment variables, named in its comment.
- Captures are in `target/captures/`, not tracked. `cargo clean` deletes them, and the tools above recreate them.

## Exporting to Blender
`src/export.rs` writes a `.glb` per costume: a mesh per piece, an armature of 25 bones with the pose solve's hierarchy, skin weights, embedded PNG textures, and baked motions at 60 frames per second. `exports/` is gitignored: the files hold game data and must not be committed or published. The Costume and All costumes buttons are in the character panel, and All costumes does one costume per frame so the window keeps responding. Checked by importing in `blender -b`: DOA2U, an `A0` model and DOA3, and all 698 motions of `zac00` (25 MB, 8 s); a frame of `zac00` was compared with the viewer by eye.

DOAX exports its 32 bones from the rest pose, arms out, with hands as shape keys and blended pieces by their weights. Checked in `blender -b` on two costumes, and one frame of `tina 0077` by eye against the viewer. Bulk export over DOAX was measured for 97 s only: 227 costumes, memory flat.

Soft bodies of DOA3 and DOA2U are exported since 2026-11-12 when one motion is exported (the Costume button, or `--export` without `--all-motions`): the viewer runs the motion from the state the costume loads in, a game frame at a time, and every piece whose rest positions change (chains, cloth, tubes, springs, vertex copies, and also DOA3's hands and both games' faces) gets one shape key per frame, weighted 1 on its frame (`bake_soft_frames` in `app.rs`). Checked in `blender -b` on `hel00` (3 pieces) and DOA3 Kasumi 0 (15 pieces, rendered at two frames): nothing flies off. Normals stay the file's. With all motions they stay rigid, as before: a key per frame of hundreds of motions would be too large. Bulk exports (All costumes, `--export-all`) never bake, as baking steps the viewer and writes to the GPU every frame. DOA4's chains, cloth and springs are baked the same way (checked on `kas00`: 4 pieces), and so are DOAX2's hair and suit cloths, hands and blinks (checked on Kasumi's `ch_d_m_1`: the hair's two pieces, moving up to 23 cm over 117 frames).

Objects that chains draw whole are exported where they hang (placed at load in DOA3 and DOA2U, at export in DOA4).

DOA4 exports since 2026-11-12: a node per mesh node, as DOAX2, each the solved bone it follows times its place, motions baked at 60 frames per second, hands as shape keys driven by the scripts, and worn and chain-drawn objects where they sit at rest. Checked in `blender -b` on `zac00` (35 meshes, one motion, every piece near the body but object 63, an 11-vertex draw on bone 16, which the solve does not have and which the viewer puts there too). The chest's shape is a set of shape keys driven by the stand-in spring; soft bodies are baked for a single motion, as for DOA3; the face's eyes and mouth are shape keys left at rest (2026-11-12).

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
- Blend slots 18, 22, 134, 135 and objects with 2 or 3 extra floats are not handled. Blend table entries using matrices 16, 19, 20, 23, 30, 31 are unmapped. Over every costume (ignored test `survey_unknown_slots`, 2026-11-12) only `hks00` uses those slots (and kind 2 slots 0, 2, 3, 15), and only Tengu's `tng00` to `tng02` lose joint pieces, 3 each, on matrices 26, 27 and 30; where the game builds those was not found.
- Plain `stage_XX.xpr` files do not parse with the costume parser.

## Backlog
1. DOA2U: stages, other soft body kinds.
2. DOA3: other face targets, ring mode 1 in tubes, the knife in Kasumi's hand, real stage wind, vertex normals (6 to 12 percent of the file's are over 60 degrees from their surface, measured on Kasumi, Ayane and Bayman with faces turned to the file's side; an earlier count of 40 to 53 percent was wrong, as it took strips' alternating winding as the surface's side. The viewer works normals out from the posed triangles each frame for materials without the game's lighting, over the whole model, in `smooth_normals`), 0xFA sections, visual defects (Tina's forearm bands, Zack's chin item).
3. Kasumi's DOA3 fringe: the user thinks it looks the same as in the game (2026-11-04). No change needed unless a closer comparison shows otherwise.
4. Complete chest bounce animations: implement the remaining two bounce axes in the viewer and support exporting the complete bounce animation to Blender, including all axes and shape deformations. Export of the vertical axis is done for all games (2026-11-10). DOAX2's three axes are in the viewer and the export, from the game's code (see "Chest shapes" in `docs/doax2-notes.md`); open there: which pieces use the second weight set (point 1). **Pending, needs the user:** put Tina, Lisa or Helena on screen in DOAX2, capture with `tools/re/doax2_live.py array target/captures/doax2_live.txt`, then run the ignored test `live_spring_bones_match` to compare bones 0, 8 and 14 with the game frame by frame (their axes and roll already match, 2026-11-10). The sideways and forward axes exist only in DOAX2's files.
5. Voices: DOA2U's clips play from the Voices window. Not done: DOA3's clips, which have no names, and playing clips with moves.
6. DOAX: support Dead or Alive Xtreme Beach Volleyball in the viewer, asked for by the user (2026-11-05). Install: `/home/pandi/Ports/Apps/DeadOrAliveXtremeBeachVolleyball-DOAXVBNativePCPort`. In the viewer: 8 characters with suits and motions, data in `assets/doax/`. The motion evaluator is confirmed against the game, and joints and blended pieces are in. The pose solve matches the game on every bone (median 0.001 in a matrix element over a 3500-sample demo capture, 2026-11-11); bone tables are measured for all eight characters. Findings in `docs/doax-notes.md`; `tools/re/doax_live.py` and `doax_capture.py` read its memory. Do not assume its formats match DOA2U or DOA3.
7. DOAX2: Dead or Alive Xtreme 2 (Xbox 360), asked for by the user (2026-11-06). Install: `/home/pandi/Ports/Apps/DeadOrAliveXtreme2-DOAX2Recomp`, data copied to `assets/doax2/` (with a 32 MB copy of the game's image from memory). `src/formats/doax2.rs` reads XPR2 models, the node tree, Xbox 360 textures and CAT draw lists by body type; the viewer shows body, head and suit, plays the motions with DOAX's solve, blends weighted pieces by an inferred table, and drives the chest's six shapes with the game's three springs (`--doax2`). Export writes a node per piece, with the chest shapes as shape keys. `tools/re/doax2_live.py` reads bone arrays from the running game; `tools/re/xbox360_ppc.py` disassembles the image with VMX128; `tools/re/doax2_shaders.py` lists and disassembles its Xenos shaders. Findings in `docs/doax2-notes.md`. Order set by the user (2026-11-07): 1 complete chest bounce animations (remaining axes in the viewer and full bounce export), 2 lights, 3 hair animation, 4 facial animation, 5 choosing hairstyles (done 2026-11-11: the panel's Hair row lists the character's heads in `chr.afs`), 6 the hand file per scene, 7 the game's material colours. Then DOAX: hand intermediate shapes, arm roll, light sets per stage. Details: lights (the vertex shaders' lighting is decoded, see "What the shaders do with them" in the notes; open: how the CPU folds the materials' colours into the constants, needs the game running; shader constants found: `DirLightVec` c71-74, `DirLightCol` c75-78, `AmbientLightP1/P2` c68-69, point and spot lights after; a table of light sets lives in the guest heap from `0x4007E280`, 0x1E0 bytes a set: ambient, 4 directions, 4 colours; its source file is being looked for), hair animation (in the viewer since 2026-11-10 for heads with cloth, `src/formats/doax2_cloth.rs`: the game's velocity step and thread caps, every collider shape checked against the game's matrices, collisions on the move's segment, the threads' angle limits from the code; the hair's other vertices are rigid with the head, as only fixed particles share triangles with them, test `layout_two_writers`; open: what the game does after a hit, the 14 small particle-less pieces on Kasumi's `AA`, export; suit cloths (skirts, sleeves, `base`, `tare`; a set per body type in the suit CAT's `tdpack`) run through the same simulation since 2026-11-11, by eye only; open there: collider type 9, the `base` and `sode` roots following the chest's shapes, the item cloth `zgra_pre`, see "Suit cloths" in the notes; Helena's ponytail has no cloth and sticks out rigid), facial animation (blinks done from script op 22; the mouth's vowel shapes are read, their trigger is not). Then: other heads, which hand file per scene, the game's material colours. Poses are checked live (Tina, Lei Fang).
8. DOA4: Dead or Alive 4 (Xbox 360), asked for by the user (2026-11-10). Install: `/home/pandi/Ports/Apps/DeadOrAlive4Online-DeadOrAlive4Online`, read in place from `game/` (`DOA4_GAME` overrides it; nothing is copied to `assets/`). The game did not run on the user's machine on 2026-11-11, so everything is from the files and nothing is compared with it. In the viewer (`--doa4`): all 160 costumes with textures, heads attached for costumes without one, plus the 34 DOAX2 swimsuits the port builds (loose files in its Windows prefix, `DOA4_SUITS` overrides the folder). `src/formats/doa4.rs` reads the CAT's draw list (a byte stream, not DOAX2's) and uses DOAX2's XPR2 reader, whose headers are 12 bytes shorter here. Findings in `docs/doa4-notes.md`. `tools/re/doa4_xex.py` writes the game's image from `default.xex` (2026-11-12) for `xbox360_ppc.py` with `IMAGE=`, so DOA4's code can be read without the game running. Motions play (2026-11-11): `mot.afs` is DOA3's layout in big-endian and the 44 tracks go through `doa3::solve_pose` unchanged, with the skeleton from the rest pose and a fixed frame per limb bone (`doa4::rest_frame`); stances and strikes look right by eye. Joint pieces (`91` draws) blend by the weights in their vertices, own bone then parent. Next, in order: 1 the chest's joint piece (since 2026-11-12 its first two weights are the chest and the pelvis, from 111 of 116 costumes; the third and fourth go half to each, a guess; the knees' third weight and the forearm pieces on bones 32 and 33 are guesses too, see the notes), 2 the other soft body kinds (hair chains, kind 1, run through DOA3's simulation since 2026-11-11: the CAT's word 2 is DOA3's section list in big-endian; springs, kind 5, likewise; soft bodies take each solved bone turned back by `doa4::rest_frame`; cloth, kind 2, is DOA2U's record with two fields moved, read by `parse_cloth_records_doa4`; kind 250 copies only feed the lower levels of detail; kinds 0 and 3, worn objects and overlays, are DOA3's sections 0 and 3 and drawn since 2026-11-12; kind 254 is DOA2U's 0xFE, a variant of the record before it (skipped, as in DOA2U); the 10 sheets the parser refuses are lower levels of detail (their objects carry 0x1000); next kinds 7, 9, 14, 15 and 251; chains that draw whole objects are in since 2026-11-12 for DOA3, DOA4 and DOA2U's costumes in DOA3's format, by `sub_0003AA20` (DOA2U's `sub_00050100` differs in mode 2), and DOA2U's own costumes, 126 more; DOA4's `0x8233A900` is DOA2U's code, read from its image. The mode is the file's byte +3, as the loader swaps bytes 2 and 3 (seen in DOA3's memory, 2026-11-12): nearly all are mode 1, objects on the particles, following one another down the chain), 3 faces and chest shapes (the `MPV` sets at CAT word 3 are read; hands follow the scripts of `anm.afs`, DOA3's `.bin` in big-endian, since 2026-11-11, with pose p taken as entry p + 1; the face blinks and moves its mouth on DOA3's timing as a stand-in; the chest's set is one shape, down, driven since 2026-11-12 by DOA3's spring as a stand-in, in the viewer and the export; DOA4's own driver is not found), 4 which hairstyle each of the game's own costumes wears (the panel's Hair row and `--hair N` pick any head, in DOA4 and DOAX2, since 2026-11-11), the loose draws (0xA0 to 0xA2), tag motions, pieces on bones the solve does not have (16, 20, 24, 25: `zac00`'s object 63 on bone 16 sits a metre from the body). Export of motions is done (2026-11-12).

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
