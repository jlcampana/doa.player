# DOA Player

Native Linux viewer in Rust (wgpu 22, winit 0.30, egui 0.29, glam) for Dead or Alive 3 and DOA2U characters. The user writes in Spanish; answer in Spanish.

## Rules from the user

- Work on `main`, never `develop`. Assets live in gitignored `assets/`.
- Do not throw away existing work.
- Reading game memory is fine, read-only. Never write to it.

## Where things are

- `docs/doa2u-notes.md`: DOA2U formats and cipher. `docs/doa3-internals.md`: DOA3 internals. `docs/viewer-mode-feasibility.md`: reading live memory.
- `src/formats/doa2u.rs`, `doa3*.rs`, `model.rs`: parsers. `src/app.rs`: roster and loading. `src/renderer/mesh.rs`: pose and skinning.
- `tools/re/`: `doa2u_crypt.py` (decrypt, AFS), `doa2u_obj_dump.py` (dump an OBJ), `doa2u_live.py` and `doa3_live.py` (read-only memory oracles; more under "Tools for this work"), `shot.sh` (render to PNG).
- DOA2U data the viewer reads: `assets/doa2u/` (`DOA2.xbe`, `datahdd.afs`, `datahdd2.afs`, `datadvd.afs`, `voice.afs`), copied from the install. Other archives, such as `bgm.afs`, are only in the install.
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
EXTRA="--doa2u --view hands" tools/re/shot.sh out "Helena Douglas|0|0|200|3|1"   # frames a part of the pose
EXTRA="--doa2u --mesh-only" tools/re/shot.sh out "Zack|0|0|200|3|1"              # also --no-textures
target/fast/doa_player --load "Zack" 0 --doa2u --export [--all-motions]            # writes exports/<game>/<character>/<costume>.glb
target/fast/doa_player --export-all                                                 # every costume, no visible window, about 3.5 minutes
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

## Limit on soft bodies

Claude set this and told the user: no chest spring, cloth or other body soft bodies for Kasumi, Ayane and Hitomi, skirts included, as they are teenagers or young in the series. `ytina` (`yti00`, ID 18), by her name Tina as a girl, is on the same list. Their hair stays on: `NO_SOFT_CLOTH_IDS` in `doa2u.rs` keeps only chains on the head. The DOA2U chest spring, at the user's request, is only for Tina, Helena and Lei Fang (`CHEST_SPRING_IDS`). Develop and validate cloth on adult characters: Helena, Tina (`tin16`), Lei Fang. Diagnose those three from data, and use an adult for close-up body renders.

`CHEST_SPRING_CHARACTER_IDS` and `TORSO_FRONT_CHAIN_CHARACTER_IDS` in `doa3.rs`, and `characters.toml`, do include them, against an older note in memory. The user was asked how that came about and has not answered. It was left unchanged, and is not to be carried over to DOA2U.

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
- `tools/re/doa2u_lights.py`: lists the D3D lights the game enables.
- `tools/re/doa2u_stage_lights.py`: lists the light records in the executable.
- `tools/re/doa2u_normal_check.py`: per object in a capture, how many positions and normals the game changes.
- `tools/re/doa2u_fn.py`: prints a generated C function without flag bookkeeping.
- Ignored tests: `list_costume_objects`, `joint_pieces_match_captured_vertices`, `captured_tracks_are_found_in_motions`, `cloth_follows_captured_vertices`, `doa2u_single_steps`, `doa2u_chain_single_steps`. Each reads its inputs from environment variables, named in its comment.
- Captures are in `target/captures/`, not tracked. `cargo clean` deletes them, and the tools above recreate them.

## Exporting to Blender

`src/export.rs` writes a `.glb` per costume: a mesh per piece, an armature of 25 bones with the pose solve's hierarchy, skin weights, embedded PNG textures, and baked motions at 60 frames per second. `exports/` is gitignored: the files hold game data and must not be committed or published. The Costume and All costumes buttons are in the character panel, and All costumes does one costume per frame so the window keeps responding. Checked by importing in `blender -b`: DOA2U, an `A0` model and DOA3, and all 698 motions of `zac00` (25 MB, 8 s); a frame of `zac00` was compared with the viewer by eye.

Not exported: soft bodies (they stay on their bone, in the file's shape), DOA3 and `A0` hand poses, and faces. DOA2U hands are shape keys, driven by the move scripts. Joint pieces bend by quaternion in the game and by weights here, so they differ slightly from the viewer away from the rest pose. The chest spring is not exported for anyone. If soft bodies are ever baked, the limit on soft bodies above applies to the export too.

Do not run a visible window for long from tests: a window that does not process events looks hung on the user's desktop.

GPU memory does not show in any process's memory use, and `--export-all` runs in one loop with no frames. Without a submit and a poll per costume it grew 28 MB per costume until the desktop froze (2026-11-05). When changing a bulk path, watch `MemAvailable` and `mem_info_vram_used`/`mem_info_gtt_used` under `/sys/class/drm/card*/device/`, and kill the run below a floor. The All costumes button goes through frames: 49 costumes of Lei Fang left VRAM flat (805 to 861 MB).

## Development notes

- Framing: the Background panel's View buttons, or `--view full|head|hands|feet`, frame from the posed character. Validate close-ups on an adult, as the limit above says, not on Kasumi, which an older version of this note suggested.
- `datadvd.afs` costumes and the extra characters are in, see "Costume files" in the notes. Not compared with the game.
- Lighting: DOA2U is done as far as the notes say. DOA3 keeps the viewer's own shading; its materials may work the same way, but this is not checked.

## Unverified in DOA2U

- Only limbs, pelvis, chest, hands and motion tracks are compared with the running game. Joint pieces are compared on `kas00` only. Head, feet on slopes and drawing are not.
- Character IDs in `app.rs` are inferred from tables. Measured in matches: Zack is ID 0 (`zack.mot`, `zac00`), Bayman 24 (`sambo.mot`, `bay00`), Leon 8 (`bayman.mot`, `leo00`), Hitomi 21 (`koryu.mot`, `kor00`). Tina is ID 1 and Lei Fang 12, without checking their files. The other IDs are inferred.
- Blend slots 18, 22, 134, 135 and objects with 2 or 3 extra floats are not handled. Blend table entries using matrices 16, 19, 20, 23, 30, 31 are unmapped.
- Plain `stage_XX.xpr` files do not parse with the costume parser.

## Backlog

1. DOA2U: stages, other soft body kinds.
2. DOA3: other face targets, ring mode 1 in tubes, the knife in Kasumi's hand, real stage wind, vertex normals, 0xFA sections, visual defects (Tina's forearm bands, Zack's chin item).
3. Kasumi's DOA3 fringe: the user thinks it looks the same as in the game (2026-11-04). No change needed unless a closer comparison shows otherwise.
4. Unlocking everything: DOA2U's costumes are already at the maximum in this save, and the viewer shows everything regardless. Save flags for hidden characters and others are read from code, see "Unlocks" in the notes; their state in this save and what locks the CG gallery are open. The save file is encrypted and its format was not read.
5. Voices: DOA2U's clips play from the Voices window. Not done: DOA3's clips, which have no names, and playing clips with moves.

## Development principles

- Costume discovery should cover all available archives and variations, not just those exposed by the original game menus. Keep archive enumeration independent from the original select screen roster and costume limits.
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
