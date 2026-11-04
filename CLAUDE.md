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
- Cloth (kind 2), chains (kind 1, hair) and extra objects (kind 3) are in the viewer. Single steps match the game on most records, not all.
- Chest spring: Tina, Helena and Lei Fang only. Measured on Helena and Lei Fang.
- Roster: Zack 0, Tina 1, Leon 8, Lei Fang 12, Hitomi 21, Bayman 24 measured in matches. Internal names differ: `sambo` is Bayman, `bayman` is Leon, `koryu` is Hitomi.

Characters face away at yaw 20 and the front is near yaw 200. DOA3 does the same, so this is the viewer's convention, not a pose bug.

## Limit on soft bodies

Claude set this and told the user: no chest spring, cloth or other body soft bodies for Kasumi, Ayane and Hitomi, skirts included, as they are teenagers or young in the series. Their hair stays on: `NO_SOFT_CLOTH_IDS` in `doa2u.rs` keeps only chains on the head. The DOA2U chest spring, at the user's request, is only for Tina, Helena and Lei Fang (`CHEST_SPRING_IDS`). Develop and validate cloth on adult characters: Helena, Tina (`tin16`), Lei Fang. Diagnose those three from data, and use an adult for close-up body renders.

`CHEST_SPRING_CHARACTER_IDS` and `TORSO_FRONT_CHAIN_CHARACTER_IDS` in `doa3.rs`, and `characters.toml`, do include them, against an older note in memory. The user was asked how that came about and has not answered. It was left unchanged, and is not to be carried over to DOA2U.

## Next steps, in order

1. **Check the winding fix on other costumes against the game.** It was checked on `hel08` by eye. Feet and shin pieces of `kas00` were also reported as wrong; compare them in the game. Objects in no draw list are still unread, such as `hel08` 31 and 35 to 38 and `kas00` 15, 42 and 43.
   Also compare the hair of `hel08` with the game. In the viewer the top of the head has olive patches, and it is pale in the game; the translucent layer (kind 3) or the lighting may be the cause. Not checked.
2. **Remaining soft body error.** The `tin16` skirt, the first sheet of `hel00`'s coat, and chain records 11 and 19 of the last capture (`tin16` 11, `hel00` 7). The stage's floor height is the first suspect, not checked.
3. **Other soft body kinds:** 4, 5, 7 and 12, with updaters listed in the notes.
4. Hands (`apv` entries 0 and 1, and move scripts) and lighting. Blink and mouth are in, with DOA3's timing.

## Tools for this work

- `tools/re/doa2u_live.py`: read-only guest memory. `capture` writes pose frames.
- `tools/re/doa2u_capture_objects.py`: records the objects the game rewrites.
- `tools/re/doa2u_cloth_pairs.py`, `tools/re/doa2u_chain_pairs.py`: consecutive particle frames. They wait for quiet, because reads otherwise catch sheets half written.
- `tools/re/doa2u_fn.py`: prints a generated C function without flag bookkeeping.
- Ignored tests: `list_costume_objects`, `joint_pieces_match_captured_vertices`, `captured_tracks_are_found_in_motions`, `cloth_follows_captured_vertices`, `doa2u_single_steps`, `doa2u_chain_single_steps`. Each reads its inputs from environment variables, named in its comment.
- Captures are in `target/captures/`, not tracked. `cargo clean` deletes them, and the tools above recreate them.

## Development notes

- Improve close-up framing for body renders. Use Kasumi as the reference character during validation.
- Investigate additional costume availability in DOA2U by exploring `datadvd.afs` alongside the currently supported `datahdd2.afs` archives.
- Lighting: try to recreate the game's lighting so that colors look similar to the game. DOA2U materials hold color and shininess values that the viewer does not use yet.

## Unverified in DOA2U

- Only limbs, pelvis, chest, hands and motion tracks are compared with the running game. Joint pieces are compared on `kas00` only. Head, feet on slopes and drawing are not.
- Character IDs in `app.rs` are inferred from tables. Measured in matches: Zack is ID 0 (`zack.mot`, `zac00`), Bayman 24 (`sambo.mot`, `bay00`), Leon 8 (`bayman.mot`, `leo00`), Hitomi 21 (`koryu.mot`, `kor00`). Tina is ID 1 and Lei Fang 12, without checking their files. The other IDs are inferred.
- Back-face flag: OBJ material +0xC also holds 5, 8 and 9, which are not culled.
- Not rendered yet: Ryu, Lei Fang, Hitomi (files `kor*`, `koryu`), Tengu.
- Blend slots 18, 22, 134, 135 and objects with 2 or 3 extra floats are not handled. Blend table entries using matrices 16, 19, 20, 23, 30, 31 are unmapped.
- Plain `stage_XX.xpr` files do not parse with the costume parser.

## Backlog

1. DOA2U: hands, stages, other soft body kinds.
2. DOA3: other face targets, ring mode 1 in tubes, the knife in Kasumi's hand, real stage wind, vertex normals, 0xFA sections, visual defects (Tina's forearm bands, Zack's chin item).
3. Kasumi's DOA3 fringe: the user thinks it looks the same as in the game (2026-11-04). No change needed unless a closer comparison shows otherwise.
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
