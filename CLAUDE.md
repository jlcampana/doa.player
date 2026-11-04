# DOA Player

Native Linux viewer in Rust (wgpu 22, winit 0.30, egui 0.29, glam) for Dead or Alive 3 and DOA2U characters. The user writes in Spanish; answer in Spanish.

## Rules from the user

- Work on `main`, never `develop`. Assets live in gitignored `assets/`.
- Do not throw away existing work.
- Reading game memory is fine, read-only. Never write to it.

## Where things are

- `docs/doa2u-notes.md`: DOA2U formats and cipher. `docs/doa3-internals.md`: DOA3 internals. `docs/viewer-mode-feasibility.md`: reading live memory.
- `src/formats/doa2u.rs`, `doa3*.rs`, `model.rs`: parsers. `src/app.rs`: roster and loading. `src/renderer/mesh.rs`: pose and skinning.
- `tools/re/`: `doa2u_crypt.py` (decrypt, AFS), `doa2u_obj_dump.py` (dump an OBJ), `doa3_live.py` (read-only DOA3 memory oracle), `shot.sh` (render to PNG).
- DOA2U install: `/home/pandi/Ports/Apps/DeadOrAlive2Ultimate-Doa2uRe/private/`. `play-disc/DOA2/` has the archives, `program/runtime-sources/recomp_00NN.c` the generated C. The user's builds may rewrite `generated`. The public repo github.com/NoRain211/doa2u-re has no game code.
- DOA3 generated C: github.com/parkerallan/doa3-recomp, `doa3/src/game/recomp/gen`.

## Commands

```bash
cargo test
DOA2U_DISC=<dir> cargo test -- --ignored --nocapture    # all encrypted costumes
./build.sh --run --load "Zack" 0 --doa2u
EXTRA="--doa2u" tools/re/shot.sh out "Zack|0|0|200|3|1"
```

## Current work: DOA2U leg pose

State at `ccdc2da`: 268 encrypted XPR files parse. Costumes draw with textures, joints and motions. Seen in renders: Zack, Helena, Bass, Jann Lee, Gen Fu, Bayman, Ein, Leon.

**Bug:** above the waist looks right, but knees splay outward and shins look stretched and thin at the ankle, even with the pelvis at 0.98. `try_load_doa2u_character` reuses `Doa3Pose` and `doa3::solve_pose`, so DOA3's limb solve or pole is the suspect. Joint pieces are a second suspect.

Already searched for the solver: readers of `0x85073C` and π constants. `sub_000EDBF0` and `sub_000ED680` are not it. DOA3's equivalents are `sub_00091720` and `sub_00094660`.

**Next step:** either find the solver in the generated C, or read bones from the running game and compare. The user was asked to start Zack against Bayman in sparring and say when they are fighting. Do this before guessing. Matrices have stride 0x40; the per-fighter struct is at `0x850738 + 0x44*f` (+4 S table, +0x28 probably matrices). The guest offset in the process must be found as in `tools/re/doa3_live.py`, whose addresses are DOA3's and whose process pattern looks for `DOA3.exe`.

Characters face away at yaw 20; the front is near yaw 200. Check whether this is the same bug.

## Development notes

- Investigate the chest spring implementation in DOA2U.
- Improve close-up framing for body renders. Use Kasumi as the reference character during validation.
- Investigate additional costume availability in DOA2U by exploring `datadvd.afs` alongside the currently supported `datahdd2.afs` archives.

## Unverified in DOA2U

- Nothing is compared with the running game yet.
- Character IDs in `app.rs` are inferred from tables.
- Back-face flag: OBJ material +0xC also holds 5 and 8.
- Not rendered yet: Kasumi, Ayane, Tina, Ryu, Lei Fang, Kō Ryū, Tengu.
- Blend slots 18, 22, 134, 135 and objects with 2 or 3 extra floats are not handled. Blend table entries using matrices 16, 19, 20, 23, 30, 31 are unmapped.
- Plain `stage_XX.xpr` files do not parse with the costume parser.

## Backlog after the legs

1. DOA2U: hair, cloth, faces, hands, translucent layers, stages.
2. DOA3: other face targets, ring mode 1 in tubes, the knife in Kasumi's hand, real stage wind, vertex normals, 0xFA sections, visual defects (Tina's forearm bands, Zack's chin item).
3. Waiting on the user: whether Kasumi's DOA3 fringe covers her eyes too much in the game.
4. Unlocking everything: last of all, by user request.

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
