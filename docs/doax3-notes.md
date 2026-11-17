# DOAX3 Fortune (PS4) notes

Dead or Alive Xtreme 3 Fortune, CUSA04555, base 1.00. Asked for by the user on 2026-11-17. Since 2026-11-17 the viewer shows each costume at rest with the face and a hairstyle (`--doax3`, the DOAX3 button), untextured; this file holds what was read from the files. Nothing is compared with the running game (the port in `/home/pandi/Ports/Apps/DeadOrAliveXtreme3Fortune-Doax3FortunePc` needs update 1.19 and three PS4 system modules the user does not have).

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
- `GeoDecl` (at the `ObjGeo`'s +0x28): one entry per declaration. Entry +4 is the offset of its element list **from the entry** (with a single entry, at 0x40, that lands on the GeoDecl's 0x80, which is how it was first misread as +0 from the GeoDecl), +0x10 the index count, +0x14 the vertex count. The element list: vertex buffer u32, stride u32, element count u32, then from +0x18 elements of 8 bytes in D3D9's layout: stream u16, offset u16, type u8, method u8, usage u8, usage index u8. Types seen: 2 three floats, 3 four floats, 5 four bytes, 0xA two half floats, 0xB four half floats. Usages: 0 position, 1 weights, 2 bone indices, 3 normal, 5 texture coordinates, 6 tangent.
- **Buffers**: the k-th declaration in file order (over all objects) uses vertex buffer k and index buffer k, and its element list names buffer k too. Checked on 13 of Kasumi's models: every buffer holds exactly its declaration's vertex count times its stride. Indices stay below the vertex count.
- Indices are u16 triangle lists. Positions are in metres.
- Kasumi's face, object 0: stride 128, positions float4 (w 1), normals float4, three float4 weight sets with three byte-index sets (12 bones a vertex), a tangent and two UV sets as four half floats each (the first two are the UV). Other objects use stride 52 to 80 with float3 positions, half-float normals and weights, and the UV as two half floats (type 0xA).
- **Objects** (`ObjGeo` +0x60): `WGT_body`, `WGT_mizugi` (the suit), `WGT_face`, `WGT_hair` are the visible model. `WGT_zdmodel` (hair) and `*T_SHADOW_MODEL` (face) have no normals, probably depth and shadow passes. `WGT_body_zbreak` and `WGT_mizugi_z` repeat the body and suit in the same place; their purpose is not read. `WGT_faceparts` (eyes, mouth) and `*_sweat_*` are modelled around their own node (Kasumi's face parts at Y 0.04 to 0.07), so they need the node bindings. `doax3::drawn_at_rest` keeps the first group only, a choice from names and bounds.
- Some models lie loose beside the archives instead of in them: Nyotengu's, Hitomi's and Momiji's faces and the Aesthetician's head. `doax3::Archives` reads a loose file before an archive's.
- **Space**: costumes and the face share one space, the hips near the origin (a body runs from Y -0.93 to 0.48 and the face from 0.51 to 0.72). Hairstyles are modelled around their first node, `MOT01_Head` (one runs from Y -0.39 to 0.18), so they go on the body's head node.
- `NodeLay`: one `NodeObj` chunk per node: +0x34 0xFFFFFFFF on those read, +0x38 the node's number, name at +0x50 (`MOT01_Head`, `OPT_acs_fronthair...`). A costume has 162 nodes (`MOT00_Hips` first), the face 52, a hairstyle up to 203 (its physics bones, probably). `HieLay` entries are 0x90 bytes: the local matrix (16 floats, column-major, translation in 12 to 14), the parent u32 at +0x40 (0xFFFFFFFF for the root), then a count, the depth and the children's numbers (read loosely). `GlblMtx`: the global rest matrix per node; `BnOfsMtx`: its inverse (checked on the head: +0.504 and -0.504; with rotation on the feet, not checked).
- **Placing parts**: a part's first node is `MOT01_Head`; it sits where the body's does in most files (0, 0.504, -0.018 on Kasumi), at the origin in Kasumi's hairstyles. That does not tell where the vertices are. Measured on all nine characters' face and first hairstyle: every hairstyle is modelled around the head (Ayane's from Y -0.08 to 0.18 although its head node is at 0.504), every face in the body's space (Y 0.46 to 0.74) but Honoka's, which is around the head (-0.02 to 0.18). Headers, sub-meshes, `ObjInfo`'s fields and `BnOfsMtx` (always `GlblMtx`'s inverse) were compared between Kasumi's and Honoka's faces and show no flag for it, and skinning at rest with those matrices leaves vertices where they are, so the game must place them some way not read yet. `doax3::place_on` decides per object from its bounds (centre nearer the origin than the head height: on the head), which fits every case measured; it is a stand-in.
- In the viewer (2026-11-17): every costume, the face and the chosen hairstyle, lifted so the body's lowest point is on the ground. Helena's first hairstyle has a strand standing straight up from the head, probably a physics chain in its rest pose, not looked into. Hair strands and fringes show as solid sheets (no textures or alpha yet). Costume 0 is the same for everyone (`AYA_bi_02_a`, first by name), as every outfit exists for every body.
- `TTGM` and the TMCL's first block: textures, not read. PS4 textures are usually tiled, so they will need detiling.

## Next

1. Done: `src/formats/doax3.rs` reads the archives (with loose files), the nodes and one piece per declaration, and places face and hair on the body; the viewer shows it.
2. Which node each object or weight index refers to (`ObjInfo` in `MdlInfo`, the sub-meshes' tables), to place the face parts and the sweat, and for skinning.
3. Textures (`TTGM`, PS4 tiling) and materials (`MtrCol`).
4. Skeleton and motions (`SKTM`, `MPM`), physics (`PHYD`).
