# DOAX3 Fortune (PS4) notes

Dead or Alive Xtreme 3 Fortune, CUSA04555, base 1.00. Asked for by the user on 2026-11-17. Nothing is in the viewer yet; this file holds what was read from the files. Nothing is compared with the running game (the port in `/home/pandi/Ports/Apps/DeadOrAliveXtreme3Fortune-Doax3FortunePc` needs update 1.19 and three PS4 system modules the user does not have).

## Getting the files out of the PKG

The game data is `game/base-clean/uroot` in the port's folder, extracted on 2026-11-17. LibOrbisPkg's PkgTool v0.2 is the only PKG tool at hand and runs only on .NET 10 here (`DOTNET_ROLL_FORWARD=Major`, `~/Ports/Runtimes`). On .NET 6 and later its reads come back short and it leaves zeros: the tail of every compressed 64 KiB block in `pkg_extract`, and about 2.1 GB of holes when it reads `pfs_image.dat` out of the outer image (`pkg_extractinnerpfs`). Its `pkg_validate` reports "PFS Image Digest" invalid and the PKG has 22 MiB of zeros at 0x3700000, but read the way below every one of the 288022 inner blocks unpacks, so that damage, if it is any, falls outside the data.

1. `PkgTool pkg_extractouterpfs --passcode 00000000000000000000000000000000 GAME.pkg outer.pfs`: only decrypts (without the passcode it fails with "key is not a valid size").
2. `tools/re/ps4_pfs.py extract outer.pfs outer_files uroot/pfs_image.dat`: the outer image has signed inodes (36-byte block pointers) and the file needs double indirection.
3. `tools/re/ps4_pfsc.py outer_files/uroot/pfs_image.dat inner.pfs --bad bad.txt`: PFSC, 64 KiB blocks, zlib with the small-window header `48 89`, or raw when stored at full size.
4. `tools/re/ps4_pfs.py extract inner.pfs base-clean 'uroot/*'`: 3317 files, all stored contiguously.
5. `sce_sys/param.sfo` lives in the PKG's header entries, not in the PFS: `PkgTool pkg_extractentry GAME.pkg 11 param.sfo`.

## Archives

`uroot/00/Archive.psarc` (2144 files, 0.9 GB) and `uroot/01/Archive.psarc` (4140 files, 14.8 GB): plain PSARC 1.4, zlib, 64 KiB blocks, with the manifest as entry 0 (`tools/re/psarc.py list|extract`). Loose files sit beside them under `00/` and `01/` (`.ywrk`, `.phyd`, `.sktm`, `.misc` and a few models). `tools/re/psarc_repair.py` rebuilds a lost table of contents from the block table; it was written while the extraction was still broken and is not needed with the files above.

Characters, three-letter codes: AYA, HEL, HON, HTM, KAS, KOK, MAR, MOM, NYO (archive 01, 460 files each: 230 costumes as `TMC` and `TMCL`, every outfit fitted to each body, such as `KOK_KAS_CM_00_B`). Archive 00, per character: `face/face_00`, `hair/hair_00` to `hair_05`, one costume (`KAS_fr_00_a`), `animation/*_INGAME_MPM.mpm`, `*_FACE_MPM.mpm`, `*_DAT.bin`; loose: `animation/*_INGAME_SKTM.sktm`, `*_FACE_RIG_MPM.mpm`, `*_PHYD.phyd`, `*_YWRK_PHYD.phyd`, `*_MISC.misc`. Costume folders under each character hold `.ywrk` files loose. None of those other formats is read yet.

Working copies for Kasumi are in `assets/doax3/` (gitignored), extracted with `psarc.py`.

## TMC models

Koei Tecmo's chunk container, the family of DOA5's models. `tools/re/tmc_dump.py` prints the tree; `tools/re/tmc_mesh.py` reads the geometry (and writes an OBJ with `--obj`).

A chunk: name[8], version u32 (0x01010000), header size u32, chunk size u32, child count u32, then at 0x20 the offset of its offset table (relative to the chunk) and at 0x24 a second table, used by `VtxLay`, `IdxLay` and `LHeader` for sizes. Children at offset 0 are absent. `TMC`'s children, in order: `MdlGeo`, `TTGM`, `VtxLay`, `IdxLay`, `MtrCol`, `MdlInfo`, `HieLay`, `LHeader`, `NodeLay`, `GlblMtx`, `BnOfsMtx`, `cpf`, `RENPACK` (Kasumi's face, 14 of 18 slots used). The TMC's header has the model's name at 0x50 (`KAS_face_00`) and floats after it, probably bounds.

- `LHeader` describes the `TMCL` beside it, which is raw data: kinds at +0x70 (0xC0000002 textures, 0xC0000003 vertices, 0xC0000004 indices), offsets in the first table and sizes in the second. The TMCL starts with u32 3 and its own size.
- `VtxLay`, `IdxLay`: buffers inside the vertex and index blocks, offsets in the first table and sizes in the second.
- `MdlGeo` holds one `ObjGeo` per object. An `ObjGeo` has its sub-meshes in its table (from +0x70) and a `GeoDecl` chunk inside; a name such as `WGT_face` sits at +0x60.
- `GeoDecl`: one entry per declaration. Entry +0 is the offset of its element list (from the `GeoDecl`), +0x10 the index count, +0x14 the vertex count. The element list: vertex buffer u32, stride u32, element count u32, then from +0x18 elements of 8 bytes in D3D9's layout: stream u16, offset u16, type u8, method u8, usage u8, usage index u8. Types seen: 2 three floats, 3 four floats, 5 four bytes, 0xB four half floats. Usages: 0 position, 1 weights, 2 bone indices, 3 normal, 5 texture coordinates, 6 tangent.
- **Buffers**: the k-th declaration in file order (over all objects) uses index buffer k; its vertex buffer is the one in its element list, and several declarations can share one (their indices then run past their own vertex count). Checked on the face, a hairstyle and five costumes: every index buffer's size fits its declaration's index count. Vertex buffers no declaration names exist (1 and 3 in `KAS_KAS_BI_03_A`); what they hold is not read.
- Indices are u16 triangle lists. Positions are in metres.
- Kasumi's face, object 0: stride 128, positions float4 (w 1), normals float4, three float4 weight sets with three byte-index sets (12 bones a vertex), a tangent and two UV sets as half floats (the first two halves of each are the UV). Other objects use stride 52 to 80 with float3 positions and half-float normals and weights.
- **Space**: costumes and the face share one space, the hips near the origin (a body runs from Y -0.93 to 0.48 and the face from 0.51 to 0.72). Hairstyles are modelled around their first node, `MOT01_Head` (one runs from Y -0.39 to 0.18), so they go on the body's head node.
- `NodeLay`: one `NodeObj` chunk per node: +0x34 0xFFFFFFFF on those read, +0x38 the node's number, name at +0x50 (`MOT01_Head`, `OPT_acs_fronthair...`). The face has 52 nodes, a hairstyle 203 (its physics bones, probably). `HieLay` has as many entries (the hierarchy, not read), `GlblMtx` and `BnOfsMtx` one 0x40 matrix each (rest globals and, probably, their inverses; not checked).
- `TTGM` and the TMCL's first block: textures, not read. PS4 textures are usually tiled, so they will need detiling.

## Next

1. Nodes: parent links (`HieLay`), rest matrices (`GlblMtx`, `BnOfsMtx`), which node each object or weight index refers to (`ObjInfo` in `MdlInfo`, the sub-meshes' tables), so the hair can go on the head.
2. A Rust reader (`src/formats/doax3.rs`: PSARC, TMC, TMCL) and a `--doax3` source in the viewer showing the rest mesh: body, face and hair.
3. Textures (`TTGM`, PS4 tiling) and materials (`MtrCol`).
4. Skeleton and motions (`SKTM`, `MPM`), physics (`PHYD`).
