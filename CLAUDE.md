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
- DOA2U install: `/home/pandi/Ports/Apps/DeadOrAlive2Ultimate-Doa2uRe/private/`. `play-disc/DOA2/` has the archives, `program/runtime-sources/recomp_00NN.c` the generated C. The user's builds may rewrite `generated`. The public repo github.com/NoRain211/doa2u-re has no game code.
- DOA3 generated C: github.com/parkerallan/doa3-recomp, `doa3/src/game/recomp/gen`.

## Commands

```bash
cargo test
DOA2U_DISC=<dir> cargo test -- --ignored --nocapture    # all encrypted costumes
./build.sh --run --load "Zack" 0 --doa2u
EXTRA="--doa2u" tools/re/shot.sh out "Zack|0|0|200|3|1"
cargo build --profile fast                              # quick test build, target/fast/doa_player
cargo test --profile fast                               # quick tests; build.sh stays optimized
BIN=target/fast/doa_player EXTRA="--doa2u" tools/re/shot.sh out "Zack|0|0|200|3|1"
```

## Current state of DOA2U (2026-11-04)

Measured against the running game; see `docs/doa2u-notes.md` for numbers and code addresses.

- Pose: limbs, hands, forearm twists, upper arms and motion tracks. `doa2u::solve_pose`.
- Joint pieces: measured on `kas00` only. Shoulders are off by up to 2.6 cm, unexplained.
- Cloth (kind 2) and chains (kind 1, hair) are in the viewer. Single steps match the game on most records, not all.
- Roster: Zack 0, Tina 1, Leon 8, Lei Fang 12, Hitomi 21, Bayman 24 measured in matches. Internal names differ: `sambo` is Bayman, `bayman` is Leon, `koryu` is Hitomi.

Characters face away at yaw 20 and the front is near yaw 200. DOA3 does the same, so this is the viewer's convention, not a pose bug.

## Limit on soft bodies

Claude set this and told the user: no chest spring, cloth or other body soft bodies for Kasumi, Ayane and Hitomi, skirts included, as they are teenagers or young in the series. Their hair stays on: `NO_SOFT_CLOTH_IDS` in `doa2u.rs` keeps only chains on the head. Develop and validate cloth on adult characters: Helena, Tina (`tin16`), Lei Fang. Diagnose those three from data, and use an adult for close-up body renders.

`CHEST_SPRING_CHARACTER_IDS` and `TORSO_FRONT_CHAIN_CHARACTER_IDS` in `doa3.rs`, and `characters.toml`, do include them, against an older note in memory. The user was asked how that came about and has not answered. It was left unchanged, and is not to be carried over to DOA2U.

## Next steps, in order

1. **Holes in some costumes.** The user showed one at the waist of `hel08`, and reported Kasumi's `kas00` chest as see-through, with feet and shin pieces looking wrong.
   - Ruled out: alpha discard (those textures have no clear pixels); back-face culling (winding agrees with the normals on nearly every flagged and unflagged material, and turning culling off only shows linings).
   - Suspect, not tested: the game rewrites the torso's front vertices by about 1 cm from the file's shape. In DOA3 that comes from a morph's base shape in the CAT (`parse_chest_morph`, `base + offset * |x|`). If the file's shape is not the rest shape, applying the base with no movement could close the gaps. Test on `hel08` first. Only the still shape is wanted for Kasumi.
   - Also unread: objects in no draw list that no chain draws, such as `kas00` 0, 15, 16, 42, 43 and `hel08` 30, 31, 35 to 38, and CAT types 3 and 6.
2. **Remaining soft body error.** The `tin16` skirt, the first sheet of `hel00`'s coat, and chain records 11 and 19 of the last capture (`tin16` 11, `hel00` 7). The stage's floor height is the first suspect, not checked.
3. **Other soft body kinds:** 3, 4, 5, 7 and 12, with updaters listed in the notes.
4. Faces (eyes and mouth exist in DOA3, not in DOA2U), hands and lighting.

## Tools for this work

- `tools/re/doa2u_live.py`: read-only guest memory. `capture` writes pose frames.
- `tools/re/doa2u_capture_objects.py`: records the objects the game rewrites.
- `tools/re/doa2u_cloth_pairs.py`, `tools/re/doa2u_chain_pairs.py`: consecutive particle frames. They wait for quiet, because reads otherwise catch sheets half written.
- `tools/re/doa2u_fn.py`: prints a generated C function without flag bookkeeping.
- Ignored tests: `list_costume_objects`, `joint_pieces_match_captured_vertices`, `captured_tracks_are_found_in_motions`, `cloth_follows_captured_vertices`, `doa2u_single_steps`, `doa2u_chain_single_steps`. Each reads its inputs from environment variables, named in its comment.
- Captures are in `target/captures/`, not tracked. `cargo clean` deletes them, and the tools above recreate them.

## Development notes

- Investigate the chest spring implementation in DOA2U.
- Improve close-up framing for body renders. Use Kasumi as the reference character during validation.
- Investigate additional costume availability in DOA2U by exploring `datadvd.afs` alongside the currently supported `datahdd2.afs` archives.
- Lighting: try to recreate the game's lighting so that colors look similar to the game. DOA2U materials hold color and shininess values that the viewer does not use yet.

## Unverified in DOA2U

- Only limbs, pelvis, chest, hands and motion tracks are compared with the running game. Joint pieces are compared on `kas00` only. Head, feet on slopes and drawing are not.
- Character IDs in `app.rs` are inferred from tables. Measured in matches: Zack is ID 0 (`zack.mot`, `zac00`), Bayman 24 (`sambo.mot`, `bay00`), Leon 8 (`bayman.mot`, `leo00`), Hitomi 21 (`koryu.mot`, `kor00`). Tina is ID 1 and Lei Fang 12, without checking their files. The other IDs are inferred.
- Back-face flag: OBJ material +0xC also holds 5 and 8.
- Not rendered yet: Ryu, Lei Fang, Hitomi (files `kor*`, `koryu`), Tengu.
- Blend slots 18, 22, 134, 135 and objects with 2 or 3 extra floats are not handled. Blend table entries using matrices 16, 19, 20, 23, 30, 31 are unmapped.
- Plain `stage_XX.xpr` files do not parse with the costume parser.

## Backlog

1. DOA2U: faces, hands, translucent layers, stages, other soft body kinds.
2. DOA3: other face targets, ring mode 1 in tubes, the knife in Kasumi's hand, real stage wind, vertex normals, 0xFA sections, visual defects (Tina's forearm bands, Zack's chin item).
3. Waiting on the user: whether Kasumi's DOA3 fringe covers her eyes too much in the game.
4. Unlocking everything: last of all, by user request.
5. After the viewer is finished: try playing the voices (`voice.afs`, see `README.md`). The user raised this as a possibility.

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
