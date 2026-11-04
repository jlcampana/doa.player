# DOA Player

Native Linux viewer in Rust (wgpu 22, winit 0.30, egui 0.29, glam) for Dead or Alive 3 and DOA2U characters. The user writes in Spanish; answer in Spanish.

## Rules from the user

- Work on `main`, never `develop`. Assets live in gitignored `assets/`.
- Do not throw away existing work.
- Reading game memory is fine, read-only. Never write to it.

## Where things are

- `docs/doa2u-notes.md`: DOA2U formats and cipher. `docs/doa3-internals.md`: DOA3 internals. `docs/viewer-mode-feasibility.md`: reading live memory.
- `src/formats/doa2u.rs`, `doa3*.rs`, `model.rs`: parsers. `src/app.rs`: roster and loading. `src/renderer/mesh.rs`: pose and skinning.
- `tools/re/`: `doa2u_crypt.py` (decrypt, AFS), `doa2u_obj_dump.py` (dump an OBJ), `doa2u_live.py` and `doa3_live.py` (read-only memory oracles), `shot.sh` (render to PNG).
- DOA2U install: `/home/pandi/Ports/Apps/DeadOrAlive2Ultimate-Doa2uRe/private/`. `play-disc/DOA2/` has the archives, `program/runtime-sources/recomp_00NN.c` the generated C. The user's builds may rewrite `generated`. The public repo github.com/NoRain211/doa2u-re has no game code.
- DOA3 generated C: github.com/parkerallan/doa3-recomp, `doa3/src/game/recomp/gen`.

## Commands

```bash
cargo test
DOA2U_DISC=<dir> cargo test -- --ignored --nocapture    # all encrypted costumes
./build.sh --run --load "Zack" 0 --doa2u
EXTRA="--doa2u" tools/re/shot.sh out "Zack|0|0|200|3|1"
```

## Current state: DOA2U pose

Limbs use the game's own solve, `doa2u::solve_pose`, measured against a running CPU match with `tools/re/doa2u_live.py` (read-only). `pose_matches_captured_frames` holds the captured frames. Details and what is not ported: `docs/doa2u-notes.md`.

Characters face away at yaw 20 and the front is near yaw 200. DOA3 does the same, so this is the viewer's convention, not a pose bug.

## Current work: Kasumi's costume in DOA2U

The user reports faults from the head down on `kas00`: chest see-through, feet and shin pieces wrong. Not solved. Known so far:

- The chest and foot objects are drawn, and their textures have no transparent pixels, so the shader's alpha discard is not the cause.
- Objects 0, 12 to 18, 25, 26, 37, 38, 42 and 43 are in no draw list. CAT types 3 and 6 are the next thing to read.
- Type 7 is only a list of texture slots (`sub_00060880`), not objects.

`list_costume_objects` lists a costume's objects, materials and textures. Diagnose this from data rather than from close-up renders of the body.

## Development notes

- Investigate the chest spring implementation in DOA2U.
- Improve close-up framing for body renders. Use Kasumi as the reference character during validation.
- Investigate additional costume availability in DOA2U by exploring `datadvd.afs` alongside the currently supported `datahdd2.afs` archives.

## Unverified in DOA2U

- Only limbs, pelvis, chest, hands and motion tracks are compared with the running game. Head, feet on slopes, drawing and joint pieces are not.
- Character IDs in `app.rs` are inferred from tables. Measured in matches: Zack is ID 0 (`zack.mot`, `zac00`), Bayman 24 (`sambo.mot`, `bay00`), Leon 8 (`bayman.mot`, `leo00`), Hitomi 21 (`koryu.mot`, `kor00`). Tina is ID 1 and Lei Fang 12, without checking their files. The other IDs are inferred.
- Back-face flag: OBJ material +0xC also holds 5 and 8.
- Not rendered yet: Ryu, Lei Fang, Hitomi (files `kor*`, `koryu`), Tengu.
- Blend slots 18, 22, 134, 135 and objects with 2 or 3 extra floats are not handled. Blend table entries using matrices 16, 19, 20, 23, 30, 31 are unmapped.
- Plain `stage_XX.xpr` files do not parse with the costume parser.

## Backlog after the legs

1. DOA2U: hair, cloth, faces, hands, translucent layers, stages.
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
