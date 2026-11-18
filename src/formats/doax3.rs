//! Dead or Alive Xtreme 3 Fortune (PS4): PSARC archives and Koei Tecmo `TMC` models.
//!
//! Layouts are in `docs/doax3-notes.md`; the Python tools in `tools/re/` (`psarc.py`,
//! `tmc_dump.py`, `tmc_mesh.py`) read the same things. Only geometry at rest is read: nodes
//! with their rest matrices, and one piece per vertex declaration with positions, normals and
//! the first texture coordinates. Textures, materials, weights, motions and physics are not.

use std::collections::HashMap;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use anyhow::{bail, ensure, Context, Result};
use glam::{Mat4, Vec3};

use super::model::{ModelMaterial, ModelPiece, ModelTexture, ModelVertex};

/// The extracted game, `uroot` of CUSA04555 1.00 (see the notes for how it was extracted).
const DEFAULT_GAME_DIR: &str = "/home/pandi/Ports/Apps/DeadOrAliveXtreme3Fortune-Doax3FortunePc/game/base-clean/uroot";

/// Where the game's files are read from, unless `DOAX3_GAME` names another `uroot`.
pub fn game_dir() -> PathBuf {
    std::env::var_os("DOAX3_GAME").map_or_else(|| DEFAULT_GAME_DIR.into(), Into::into)
}

/// The two archives, in the order their paths start with (`00/...`, `01/...`).
pub const ARCHIVES: [&str; 2] = ["00/Archive.psarc", "01/Archive.psarc"];

/// The nine characters by their file code, with display names.
pub const ROSTER: [(&str, &str); 9] = [
    ("KAS", "Kasumi"),
    ("MAR", "Marie Rose"),
    ("HON", "Honoka"),
    ("AYA", "Ayane"),
    ("NYO", "Nyotengu"),
    ("KOK", "Kokoro"),
    ("HTM", "Hitomi"),
    ("MOM", "Momiji"),
    ("HEL", "Helena"),
];

// ─── PSARC ────────────────────────────────────────────────────────────────────

/// A PSARC read on demand: version 1.4, zlib, entry 0 the manifest of the other entries' paths.
pub struct Psarc {
    path: PathBuf,
    block: usize,
    /// First block, length and offset of each entry.
    entries: Vec<(usize, u64, u64)>,
    /// Stored size of each block; 0 is a full raw block.
    sizes: Vec<u32>,
    names: HashMap<String, usize>,
    order: Vec<String>,
}

impl Psarc {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        let mut file = std::fs::File::open(&path).with_context(|| format!("opening {}", path.display()))?;
        let mut head = [0u8; 32];
        file.read_exact(&mut head)?;
        ensure!(&head[..4] == b"PSAR", "{} is not a PSARC", path.display());
        ensure!(&head[8..12] == b"zlib", "{}: only zlib archives are read", path.display());
        let word = |at: usize| u32::from_be_bytes(head[at..at + 4].try_into().unwrap()) as usize;
        let (toc_len, entry_size, count, block) = (word(12), word(16), word(20), word(24));
        ensure!(entry_size == 30 && toc_len > 32 && count > 0, "{}: unexpected table of contents", path.display());
        let mut toc = vec![0u8; toc_len - 32];
        file.read_exact(&mut toc)?;
        let be = |bytes: &[u8]| bytes.iter().fold(0u64, |v, &b| v << 8 | b as u64);
        let entries: Vec<(usize, u64, u64)> = (0..count)
            .map(|i| {
                let e = &toc[i * 30..i * 30 + 30];
                (be(&e[16..20]) as usize, be(&e[20..25]), be(&e[25..30]))
            })
            .collect();
        let width = if block <= 0x10000 { 2 } else if block <= 0x100_0000 { 3 } else { 4 };
        let sizes = toc[count * 30..].chunks_exact(width).map(|c| be(c) as u32).collect();
        let mut archive = Psarc { path, block, entries, sizes, names: HashMap::new(), order: Vec::new() };
        let manifest = archive.read_entry(0)?;
        archive.order = String::from_utf8_lossy(&manifest).split('\n').map(str::to_string).collect();
        archive.names = archive.order.iter().enumerate().map(|(i, name)| (name.to_ascii_lowercase(), i + 1)).collect();
        Ok(archive)
    }

    /// Paths of the entries, as the manifest writes them (`01/character/KAS/...`).
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.order.iter().map(String::as_str)
    }

    pub fn contains(&self, name: &str) -> bool {
        self.names.contains_key(&name.to_ascii_lowercase())
    }

    pub fn read(&self, name: &str) -> Result<Vec<u8>> {
        let index = *self.names.get(&name.to_ascii_lowercase()).with_context(|| format!("{name} is not in {}", self.path.display()))?;
        self.read_entry(index)
    }

    fn read_entry(&self, index: usize) -> Result<Vec<u8>> {
        let &(first, length, offset) = self.entries.get(index).context("no such entry")?;
        let mut file = std::fs::File::open(&self.path)?;
        file.seek(SeekFrom::Start(offset))?;
        let mut out = Vec::with_capacity(length as usize);
        let mut block = first;
        while (out.len() as u64) < length {
            let stored = *self.sizes.get(block).context("block table ends early")? as usize;
            let want = self.block.min((length - out.len() as u64) as usize);
            let mut chunk = vec![0u8; if stored == 0 { self.block } else { stored }];
            file.read_exact(&mut chunk)?;
            if stored != 0 && chunk.first() == Some(&0x78) {
                let mut inflated = Vec::with_capacity(self.block);
                if flate2::read::ZlibDecoder::new(chunk.as_slice()).read_to_end(&mut inflated).is_ok() {
                    out.extend_from_slice(&inflated);
                    block += 1;
                    continue;
                }
            }
            chunk.truncate(want);
            out.extend_from_slice(&chunk);
            block += 1;
        }
        out.truncate(length as usize);
        Ok(out)
    }
}

/// The game's files: both archives, and the files that lie loose beside them under `00/` and
/// `01/` (some models are only there, such as Nyotengu's, Hitomi's and Momiji's faces). A loose
/// file is read before an archive's file of the same path.
pub struct Archives {
    root: PathBuf,
    archives: Vec<Psarc>,
    loose: Vec<String>,
}

impl Archives {
    pub fn open(root: &Path) -> Result<Self> {
        let archives = ARCHIVES.iter().map(|name| Psarc::open(root.join(name))).collect::<Result<_>>()?;
        let mut loose = Vec::new();
        for top in ["00", "01"] {
            let mut stack = vec![root.join(top)];
            while let Some(dir) = stack.pop() {
                for entry in std::fs::read_dir(&dir)?.flatten() {
                    let path = entry.path();
                    if path.is_dir() {
                        stack.push(path);
                    } else if let Ok(relative) = path.strip_prefix(root) {
                        let name = relative.to_string_lossy().replace('\\', "/");
                        if !ARCHIVES.contains(&name.as_str()) && !name.ends_with("package_data.bin") {
                            loose.push(name);
                        }
                    }
                }
            }
        }
        loose.sort();
        Ok(Archives { root: root.to_path_buf(), archives, loose })
    }

    /// Every path, loose files first, then the archives' (a path can appear in both).
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.loose.iter().map(String::as_str).chain(self.archives.iter().flat_map(Psarc::names))
    }

    pub fn read(&self, name: &str) -> Result<Vec<u8>> {
        if self.loose.iter().any(|l| l == name) {
            return std::fs::read(self.root.join(name)).with_context(|| format!("reading {name}"));
        }
        let archive = self.archives.iter().find(|a| a.contains(name)).with_context(|| format!("{name} is in neither archive nor loose"))?;
        archive.read(name)
    }
}

// ─── Characters ───────────────────────────────────────────────────────────────

/// A model's two files, `..._TMC.tmc` and `..._TMCL.tmcl`, by the TMC's path.
pub fn tmcl_path(tmc: &str) -> String {
    tmc.strip_suffix("_TMC.tmc").map_or_else(|| tmc.to_string(), |stem| format!("{stem}_TMCL.tmcl"))
}

/// A character's costumes: every TMC under `character/<code>/costume/`, in both archives,
/// sorted by folder name (`KAS_bi_02_a`, then other characters' outfits fitted to this body,
/// such as `AYA_mk_00_a`).
pub fn costumes(names: &[String], code: &str) -> Vec<String> {
    let mut list: Vec<String> = names
        .iter()
        .filter(|n| {
            let mut parts = n.split('/');
            parts.nth(1) == Some("character") && parts.next() == Some(code) && parts.next() == Some("costume") && n.ends_with("_TMC.tmc")
        })
        .cloned()
        .collect();
    list.dedup();
    list.sort_by_key(|n| n.rsplit('/').nth(1).unwrap_or_default().to_ascii_lowercase());
    list
}

/// The folder name of a costume's TMC, such as `KAS_bi_02_a`.
pub fn costume_label(tmc: &str) -> String {
    tmc.rsplit('/').nth(1).unwrap_or(tmc).to_string()
}

/// A character's hairstyles: `00/character/<code>/hair/hair_NN/...`, in order.
pub fn hairstyles(names: &[String], code: &str) -> Vec<String> {
    let prefix = format!("00/character/{code}/hair/");
    let mut list: Vec<String> = names.iter().filter(|n| n.starts_with(&prefix) && n.ends_with("_TMC.tmc")).cloned().collect();
    list.sort();
    list
}

pub fn face(code: &str) -> String {
    format!("00/character/{code}/face/face_00/{code}_FACE_00_TMC.tmc")
}

// ─── TMC ──────────────────────────────────────────────────────────────────────

/// A node at rest: its name, its parent and its place in the model's space.
#[derive(Debug, Clone)]
pub struct Node {
    pub name: String,
    pub parent: Option<usize>,
    pub global: Mat4,
}

/// A model at rest: one piece per vertex declaration, in the model's space.
#[derive(Debug, Clone, Default)]
pub struct Model {
    pub name: String,
    pub nodes: Vec<Node>,
    pub pieces: Vec<ModelPiece>,
    /// The name of the object each piece belongs to (`ObjGeo` +0x60), such as `WGT_body`.
    pub objects: Vec<String>,
    /// How many textures the TMCL holds; materials' `textures` index them.
    pub texture_count: usize,
    /// Per piece, per material: whether its sub-mesh lists a colour texture alone, with no
    /// maps (lashes and lid shadows; skin, suits and hair list their maps too).
    pub colour_only: Vec<Vec<bool>>,
    /// Per piece: whether its declaration has normals. Those without (`WGT_zdmodel`, the shadow
    /// models, Nyotengu's `WGT_face1`, which also has no texture coordinates) are depth or
    /// shadow passes, probably.
    pub has_normals: Vec<bool>,
    /// Per piece, per vertex: up to 12 (node, weight) pairs, the weights summing to 1, with the
    /// object's palette applied. Empty for pieces without weights or palette.
    pub skins: Vec<Vec<Skin>>,
    /// Per piece: left out by the chosen variant (`choose_variant`).
    pub hidden: Vec<bool>,
}

/// A vertex's influences: node numbers and weights; unused slots have weight 0.
pub type Skin = [(u16, f32); 12];

impl Model {
    /// Whether piece `i` is drawn in the rest view: by its object's name and its normals.
    pub fn drawn(&self, i: usize) -> bool {
        self.objects.get(i).is_some_and(|o| drawn_at_rest(o)) && self.has_normals.get(i).copied().unwrap_or(false) && !self.hidden.get(i).copied().unwrap_or(false)
    }

    /// How many variants the model has: 2 when it holds wet hair (`WGT_wet_asobi` and
    /// `WGT_wet_asobi1`, in Kasumi's hairstyles 2, 4 and 5), else 1.
    pub fn variant_count(&self) -> usize {
        if self.objects.iter().any(|o| o.contains("wet")) { 2 } else { 1 }
    }

    /// Variant 0 draws the dry hair and hides the wet objects; variant 1 the wet objects
    /// instead of `WGT_hair`. The wet hair lies over the dry one in the same file, so drawn
    /// together a long wet fringe hung over the eyes. When the game shows which is not read.
    pub fn choose_variant(&mut self, variant: usize) {
        let wet = self.variant_count() > 1 && variant == 1;
        let has_wet = self.variant_count() > 1;
        self.hidden = self.objects.iter().map(|o| has_wet && if wet { o == "WGT_hair" } else { o.contains("wet") }).collect();
    }

    /// The drawn pieces with their index.
    pub fn drawn_pieces(&self) -> impl Iterator<Item = (usize, &ModelPiece)> {
        self.pieces.iter().enumerate().filter(|(i, _)| self.drawn(*i))
    }
}

/// Whether an object is drawn in the rest view. Read from names and bounds only, nothing from
/// the game's code: `WGT_zdmodel` and `*T_SHADOW_MODEL` have no normals (depth and shadow
/// passes, probably); `WGT_body_zbreak` and `WGT_mizugi_z` repeat the body and suit in the same
/// place, for a purpose not read; the `*_sweat_*` objects are sweat, shown in some scenes only.
/// `WGT_faceparts` (eyes, lashes, teeth) is drawn, placed on its own node by `place_on`.
pub fn drawn_at_rest(object: &str) -> bool {
    let lower = object.to_ascii_lowercase();
    object.starts_with("WGT_")
        && !lower.contains("shadow")
        && !object.contains("zdmodel")
        && !object.contains("zbreak")
        && !object.ends_with("_z")
        && !object.contains("sweat")
}

const LUMP_VERTICES: u32 = 0xC000_0003;
const LUMP_INDICES: u32 = 0xC000_0004;

fn u32_at(d: &[u8], at: usize) -> Result<u32> {
    let bytes = d.get(at..at + 4).with_context(|| format!("read past the end at {at:#x}"))?;
    Ok(u32::from_le_bytes(bytes.try_into()?))
}

fn f32_at(d: &[u8], at: usize) -> Result<f32> {
    Ok(f32::from_bits(u32_at(d, at)?))
}

fn u16_at(d: &[u8], at: usize) -> Result<u16> {
    let bytes = d.get(at..at + 2).with_context(|| format!("read past the end at {at:#x}"))?;
    Ok(u16::from_le_bytes(bytes.try_into()?))
}

/// A chunk: name[8], version, header size, size, child count, then at 0x20 its offset table
/// (relative to the chunk) and at 0x24 a second table.
struct Chunk {
    at: usize,
    name: String,
    children: Vec<usize>,
}

fn chunk(d: &[u8], at: usize) -> Result<Chunk> {
    let raw = d.get(at..at + 8).context("chunk outside the file")?;
    let name = String::from_utf8_lossy(raw.split(|&b| b == 0).next().unwrap_or_default()).into_owned();
    let count = u32_at(d, at + 0x14)? as usize;
    let table = u32_at(d, at + 0x20)? as usize;
    ensure!(count < 100_000, "{name} at {at:#x}: {count} children");
    let children = if count == 0 || table == 0 {
        Vec::new()
    } else {
        (0..count).map(|i| u32_at(d, at + table + i * 4).map(|o| if o == 0 { 0 } else { at + o as usize })).collect::<Result<_>>()?
    };
    Ok(Chunk { at, name, children })
}

/// Offsets from the first table and sizes from the second, for `VtxLay`, `IdxLay` and `LHeader`.
fn tables(d: &[u8], c: &Chunk) -> Result<Vec<(usize, usize)>> {
    let first = u32_at(d, c.at + 0x20)? as usize;
    let second = u32_at(d, c.at + 0x24)? as usize;
    (0..c.children.len()).map(|i| Ok((u32_at(d, c.at + first + i * 4)? as usize, u32_at(d, c.at + second + i * 4)? as usize))).collect()
}

/// One vertex layout: D3D9-style elements, stride, the buffer it reads, and its counts.
struct Declaration {
    buffer: usize,
    stride: usize,
    vertex_count: usize,
    index_count: usize,
    /// Offset, type and usage index by usage.
    elements: Vec<(usize, u8, u8, u8)>,
}

fn declarations(d: &[u8], geo: usize) -> Result<Vec<Declaration>> {
    // The ObjGeo's +0x28 points at its GeoDecl.
    let decl = geo + u32_at(d, geo + 0x28)? as usize;
    ensure!(d.get(decl..decl + 8) == Some(b"GeoDecl\0".as_slice()), "no GeoDecl at {decl:#x}");
    let c = chunk(d, decl)?;
    c.children
        .iter()
        .map(|&entry| {
            // The element list's offset is at +4, from the entry. (With one entry, at 0x40, this
            // is also +0's 0x80 from the GeoDecl, which is how it was first misread.)
            let list = entry + u32_at(d, entry + 4)? as usize;
            let (buffer, stride, count) = (u32_at(d, list)? as usize, u32_at(d, list + 4)? as usize, u32_at(d, list + 8)? as usize);
            let elements = (0..count)
                .map(|i| {
                    let at = list + 0x18 + i * 8;
                    Ok((u16_at(d, at + 2)? as usize, d[at + 4], d[at + 6], d[at + 7]))
                })
                .collect::<Result<_>>()?;
            Ok(Declaration { buffer, stride, vertex_count: u32_at(d, entry + 0x14)? as usize, index_count: u32_at(d, entry + 0x10)? as usize, elements })
        })
        .collect()
}

/// Up to three values of an element, by its type: 2 three floats, 3 four floats, 0xA two half
/// floats (texture coordinates at the end of 52 to 76 byte vertices), 0xB four half floats. Other types are not used for positions, normals or coordinates.
fn element(d: &[u8], at: usize, kind: u8) -> Result<[f32; 3]> {
    Ok(match kind {
        2 | 3 => [f32_at(d, at)?, f32_at(d, at + 4)?, f32_at(d, at + 8)?],
        0xA => [super::doax2::half(u16_at(d, at)?), super::doax2::half(u16_at(d, at + 2)?), 0.0],
        0xB => [super::doax2::half(u16_at(d, at)?), super::doax2::half(u16_at(d, at + 2)?), super::doax2::half(u16_at(d, at + 4)?)],
        _ => bail!("element type {kind:#x} is not read"),
    })
}

const LUMP_TEXTURES: u32 = 0xC000_0002;

/// A texture's description, from the GNF header's T# (GCN's `SQ_IMG_RSRC_WORD0..7`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextureInfo {
    /// Data format (`WORD1` bits 20 to 25): 41 BC7, 35 BC1, 10 RGBA8.
    pub format: u32,
    /// Number format (`WORD1` bits 26 to 29): 9 sRGB, 0 unorm.
    pub number: u32,
    pub width: usize,
    pub height: usize,
    /// Row pitch in texels (`WORD4` bits 13 to 26), padded past the width on small textures.
    pub pitch: usize,
    /// Tile mode index (`WORD3` bits 20 to 24): 13, `Thin_1dThin`, on every texture read.
    pub tile: u32,
    /// Resource type (`WORD3` bits 28 to 31): 9 a 2D texture; 10 (2175 in the game, one level,
    /// twice a 2D level's data: a 3D texture of depth 2, probably) is not read. Kasumi's face
    /// uses one as a kind-1 map.
    pub kind: u32,
    pub last_level: u32,
}

/// The GNFs in a model's TMCL, from the TTGL table (TTGM's +0x28 points at it; TTGH follows
/// TTGM's header): offsets from the TMCL's texture block and sizes.
pub fn texture_files<'a>(tmc: &[u8], tmcl: &'a [u8]) -> Result<Vec<&'a [u8]>> {
    let top = chunk(tmc, 0)?;
    let find = |name: &str| -> Result<Chunk> {
        for &child in top.children.iter().filter(|&&c| c != 0) {
            let c = chunk(tmc, child)?;
            if c.name == name {
                return Ok(c);
            }
        }
        bail!("no {name} chunk")
    };
    let lumps = find("LHeader")?;
    let lump_tables = tables(tmc, &lumps)?;
    let Some(block) = (0..lump_tables.len()).find(|&i| u32_at(tmc, lumps.at + 0x70 + i * 4).ok() == Some(LUMP_TEXTURES)).map(|i| lump_tables[i].0) else {
        return Ok(Vec::new());
    };
    let group = find("TTGM")?;
    let list = group.at + u32_at(tmc, group.at + 0x28)? as usize;
    ensure!(tmc.get(list..list + 4) == Some(b"TTGL".as_slice()), "no TTGL at {list:#x}");
    let count = u32_at(tmc, list + 0x14)? as usize;
    let (offsets, sizes) = (u32_at(tmc, list + 0x20)? as usize, u32_at(tmc, list + 0x24)? as usize);
    (0..count)
        .map(|k| {
            let at = block + u32_at(tmc, list + offsets + k * 4)? as usize;
            let size = u32_at(tmc, list + sizes + k * 4)? as usize;
            tmcl.get(at..at + size).with_context(|| format!("texture {k} past the TMCL"))
        })
        .collect()
}

/// A GNF's header: "GNF ", the header's size after 8 bytes (0x28), version 2 and one texture,
/// the file's size, then the T#. The data follows the header, level 0 first.
pub fn texture_info(gnf: &[u8]) -> Result<(TextureInfo, &[u8])> {
    ensure!(gnf.get(..4) == Some(b"GNF ".as_slice()), "not a GNF");
    let header = 8 + u32_at(gnf, 4)? as usize;
    let w: Vec<u32> = (0..8).map(|i| u32_at(gnf, 0x10 + i * 4)).collect::<Result<_>>()?;
    let info = TextureInfo {
        format: (w[1] >> 20) & 0x3F,
        number: (w[1] >> 26) & 0xF,
        width: (w[2] & 0x3FFF) as usize + 1,
        height: ((w[2] >> 14) & 0x3FFF) as usize + 1,
        pitch: ((w[4] >> 13) & 0x3FFF) as usize + 1,
        tile: (w[3] >> 20) & 0x1F,
        kind: (w[3] >> 28) & 0xF,
        last_level: (w[3] >> 16) & 0xF,
    };
    Ok((info, gnf.get(header..).context("GNF shorter than its header")?))
}

/// Level 0 of a GNF as RGBA. Read: BC7 (format 41), BC1 (35) and RGBA8 (10), in tile mode 13
/// (`Thin_1dThin`), which covers every 2D texture of the game (2224 models: 26016 BC7, 21
/// RGBA8, 1 BC1). Elements (4x4 blocks, or texels for RGBA8) are kept in micro tiles of 8x8
/// elements, in rows over the padded pitch, and inside a tile in Morton order, x in bit 0
/// (AddrLib's non-displayable thin order: x0 y0 x1 y1 x2 y2, the same for every element size).
/// Checked by eye on BC7 only.
pub fn decode_texture(gnf: &[u8]) -> Result<ModelTexture> {
    let (info, data) = texture_info(gnf)?;
    let (block, bytes) = match info.format {
        41 => (4, 16),
        35 => (4, 8),
        10 => (1, 4),
        other => bail!("texture format {other} is not read"),
    };
    ensure!(info.tile == 13, "tile mode {} is not read", info.tile);
    ensure!(info.kind == 9, "resource type {} is not read", info.kind);
    let (blocks_x, blocks_y) = (info.width.div_ceil(block), info.height.div_ceil(block));
    let tiles_x = info.pitch.div_ceil(block).div_ceil(8);
    let tiles_y = blocks_y.div_ceil(8);
    let tile_bytes = 64 * bytes;
    ensure!(data.len() >= tiles_x * tiles_y * tile_bytes, "GNF data short of level 0");
    let (width, height) = (info.width, info.height);
    let mut rgba = vec![0u8; width * height * 4];
    let mut texels = [0u8; 64];
    for by in 0..blocks_y {
        for bx in 0..blocks_x {
            let tile = (by / 8) * tiles_x + bx / 8;
            let (x, y) = (bx % 8, by % 8);
            let morton = (x & 1) | ((y & 1) << 1) | ((x & 2) << 1) | ((y & 2) << 2) | ((x & 4) << 2) | ((y & 4) << 3);
            let at = tile * tile_bytes + morton * bytes;
            let source = &data[at..at + bytes];
            match info.format {
                41 => bcdec_rs::bc7(source, &mut texels, 16),
                35 => bcdec_rs::bc1(source, &mut texels, 16),
                _ => texels[..4].copy_from_slice(source),
            }
            for row in 0..block {
                let py = by * block + row;
                if py >= height {
                    break;
                }
                for col in 0..block {
                    let px = bx * block + col;
                    if px < width {
                        let out = (py * width + px) * 4;
                        rgba[out..out + 4].copy_from_slice(&texels[row * 16 + col * 4..row * 16 + col * 4 + 4]);
                    }
                }
            }
        }
    }
    Ok(ModelTexture { width: width as u32, height: height as u32, rgba })
}

/// The node named after an object, from a node list (see `object_node`).
fn object_node_in(nodes: &[Node], object: &str) -> Option<usize> {
    match object.strip_prefix('*') {
        Some(tail) => nodes.iter().position(|n| n.name.ends_with(tail) && n.name.starts_with("WGT")),
        None => nodes.iter().position(|n| n.name == object),
    }
}

/// The node palette of an object: in its `NodeObj` (named after it, see `object_node`), the one
/// child block, whose +4 is the count and whose +0x60 starts the node numbers.
fn object_palette(tmc: &[u8], nodes: &[Node], node_objects: &[usize], object: &str) -> Result<Option<Vec<u32>>> {
    let found = object_node_in(nodes, object);
    let Some(at) = found.and_then(|i| node_objects.get(i)).copied() else { return Ok(None) };
    // The block is the `NodeObj`'s only child, through its own table (+0x14 count, +0x20 the
    // table): at +0x70, or at +0x80 when the name (from +0x50) runs past 16 characters. Nodes
    // without a palette have none.
    let node = chunk(tmc, at)?;
    let Some(&block_at) = node.children.first().filter(|&&c| c != 0) else { return Ok(None) };
    let block = block_at - at;
    let count = u32_at(tmc, at + block + 4)? as usize;
    // Some blocks read otherwise (Kasumi's `WGT_sweat_cheek_l`: 128 entries past the node
    // count); such objects are left unskinned.
    if count >= 1024 {
        return Ok(None);
    }
    let list: Vec<u32> = (0..count).map(|i| u32_at(tmc, at + block + 0x60 + i * 4)).collect::<Result<_>>()?;
    Ok(list.iter().all(|&n| (n as usize) < nodes.len()).then_some(list))
}

/// A sub-mesh, one of an `ObjGeo`'s children: +4 its `MtrCol` entry, +0x10 the number of
/// texture entries and from +0x14 their offsets from the sub-mesh, +0x3C the declaration it
/// draws from (within the object), +0x80 its first index, index count, first vertex and vertex
/// count. A texture entry is slot, kind and texture: kind 0 colour (a second kind-0 entry, as on
/// Kasumi's face and body, is a detail layer not drawn here), 1 and 2 maps whose use is not
/// read (pink and blue: normals, probably), 3 a second colour on the eyes.
struct SubMesh {
    declaration: usize,
    first_index: usize,
    index_count: usize,
    /// The first kind-0 texture.
    base: Option<usize>,
    /// How many texture entries it lists.
    entries: usize,
}

fn sub_meshes(d: &[u8], geo: usize) -> Result<Vec<SubMesh>> {
    chunk(d, geo)?
        .children
        .iter()
        .filter(|&&e| e != 0)
        .map(|&e| {
            let count = u32_at(d, e + 0x10)? as usize;
            ensure!(count < 64, "sub-mesh at {e:#x}: {count} textures");
            let mut base = None;
            for j in 0..count {
                let entry = e + u32_at(d, e + 0x14 + j * 4)? as usize;
                if u32_at(d, entry + 4)? == 0 && base.is_none() {
                    base = Some(u32_at(d, entry + 8)? as usize);
                }
            }
            Ok(SubMesh { declaration: u32_at(d, e + 0x3C)? as usize, first_index: u32_at(d, e + 0x80)? as usize, index_count: u32_at(d, e + 0x84)? as usize, base, entries: count })
        })
        .collect()
}

/// Whether a material's colour texture carries coverage in its alpha. Hair's do (strands), and
/// so do the lashes and lid shadows (Kasumi's texture 3, black with mean alpha 0.41), whose
/// sub-meshes list the colour texture alone; the skin's alpha is a mask of some other use
/// (Kasumi's face and body: mean 0.22 and 0.33 on opaque surfaces) and suits read are opaque.
/// No flag for this was found in `MtrCol`: a stand-in from the object's name and the sub-mesh's
/// entries. A texture also used by an opaque material stays opaque (the face's atlas, which
/// the eyeballs use too).
pub fn alpha_is_coverage(object: &str, colour_only: bool) -> bool {
    object.contains("hair") || colour_only
}

/// The model's textures, decoded only where `wanted` says so (a 1x1 white stands in for the
/// others, and for any that fail to decode), with alpha made opaque where `coverage` is false.
pub fn decode_textures(tmc: &[u8], tmcl: &[u8], wanted: &[bool], coverage: &[bool]) -> Result<Vec<ModelTexture>> {
    let white = || ModelTexture { width: 1, height: 1, rgba: vec![255; 4] };
    Ok(texture_files(tmc, tmcl)?
        .iter()
        .enumerate()
        .map(|(k, gnf)| {
            if !wanted.get(k).copied().unwrap_or(false) {
                return white();
            }
            match decode_texture(gnf) {
                Ok(mut t) => {
                    if !coverage.get(k).copied().unwrap_or(false) {
                        t.rgba.chunks_exact_mut(4).for_each(|p| p[3] = 255);
                    }
                    t
                }
                Err(e) => {
                    log::warn!("DOAX3 texture {k}: {e:#}");
                    white()
                }
            }
        })
        .collect())
}

/// Reads a model from its TMC and TMCL.
pub fn parse_model(tmc: &[u8], tmcl: &[u8]) -> Result<Model> {
    ensure!(tmc.get(..4) == Some(b"TMC\0".as_slice()), "not a TMC");
    let top = chunk(tmc, 0)?;
    let mut by_name: HashMap<String, Chunk> = HashMap::new();
    for &child in top.children.iter().filter(|&&c| c != 0) {
        let c = chunk(tmc, child)?;
        by_name.insert(c.name.clone(), c);
    }
    let get = |name: &str| by_name.get(name).with_context(|| format!("no {name} chunk"));
    let name = String::from_utf8_lossy(tmc.get(0x50..0x70).unwrap_or_default().split(|&b| b == 0).next().unwrap_or_default()).into_owned();

    // Nodes: names from NodeLay, parents from HieLay (+0x40), rest places from GlblMtx.
    let node_lay = get("NodeLay")?;
    let hierarchy = get("HieLay")?;
    let globals = get("GlblMtx")?;
    let mut nodes = Vec::new();
    for (i, &entry) in node_lay.children.iter().enumerate() {
        let raw = tmc.get(entry + 0x50..entry + 0x70).context("node name")?;
        let node_name = String::from_utf8_lossy(raw.split(|&b| b == 0).next().unwrap_or_default()).into_owned();
        let parent = hierarchy.children.get(i).map(|&h| u32_at(tmc, h + 0x40)).transpose()?.filter(|&p| p != u32::MAX).map(|p| p as usize);
        let at = *globals.children.get(i).context("fewer matrices than nodes")?;
        let values: Vec<f32> = (0..16).map(|k| f32_at(tmc, at + k * 4)).collect::<Result<_>>()?;
        nodes.push(Node { name: node_name, parent, global: Mat4::from_cols_slice(&values) });
    }

    // Buffers in the TMCL.
    let lumps = get("LHeader")?;
    let lump_tables = tables(tmc, lumps)?;
    let lump = |kind: u32| -> Result<usize> {
        let index = (0..lump_tables.len()).find(|&i| u32_at(tmc, lumps.at + 0x70 + i * 4).ok() == Some(kind)).with_context(|| format!("no TMCL block {kind:#x}"))?;
        Ok(lump_tables[index].0)
    };
    let (vertex_base, index_base) = (lump(LUMP_VERTICES)?, lump(LUMP_INDICES)?);
    let vertex_buffers = tables(tmc, get("VtxLay")?)?;
    let index_buffers = tables(tmc, get("IdxLay")?)?;

    // One piece per declaration; the k-th declaration in file order uses index buffer k. Its
    // sub-meshes (the ObjGeo's children) become the piece's materials.
    let mut pieces = Vec::new();
    let mut objects = Vec::new();
    let mut colour_only = Vec::new();
    let mut has_normals = Vec::new();
    let mut skins = Vec::new();
    let node_objects: Vec<usize> = node_lay.children.clone();
    let mut k = 0;
    for &geo in get("MdlGeo")?.children.iter().filter(|&&g| g != 0) {
        let raw = tmc.get(geo + 0x60..geo + 0x80).context("object name")?;
        let object = String::from_utf8_lossy(raw.split(|&b| b == 0).next().unwrap_or_default()).into_owned();
        let subs = sub_meshes(tmc, geo)?;
        for (d, decl) in declarations(tmc, geo)?.into_iter().enumerate() {
            let (index_at, _) = *index_buffers.get(k).context("fewer index buffers than declarations")?;
            k += 1;
            let (vertex_at, vertex_size) = *vertex_buffers.get(decl.buffer).context("vertex buffer out of range")?;
            let indices: Vec<u32> = (0..decl.index_count).map(|i| u16_at(tmcl, index_base + index_at + i * 2).map(u32::from)).collect::<Result<_>>()?;
            let used = indices.iter().max().map_or(0, |&m| m as usize + 1).max(decl.vertex_count);
            ensure!(used * decl.stride <= vertex_size + decl.stride, "indices run past vertex buffer {}", decl.buffer);
            let find = |usage: u8| decl.elements.iter().find(|e| e.2 == usage && e.3 == 0).map(|e| (e.0, e.1));
            let position = find(0).context("no positions")?;
            let normal = find(3);
            has_normals.push(normal.is_some());
            let uv = find(5);
            let vertices = (0..used)
                .map(|v| {
                    let at = vertex_base + vertex_at + v * decl.stride;
                    let p = element(tmcl, at + position.0, position.1)?;
                    let n = normal.map(|(o, t)| element(tmcl, at + o, t)).transpose()?.unwrap_or([0.0, 1.0, 0.0]);
                    let t = uv.map(|(o, t)| element(tmcl, at + o, t)).transpose()?.unwrap_or_default();
                    Ok(ModelVertex { position: p, normal: n, uv: [t[0], t[1]] })
                })
                .collect::<Result<Vec<_>>>()?;
            // Weights (usage 1) and node indices (usage 2), three sets of four, through the
            // palette in the object's node (`NodeObj`'s child block: the count at +4 and u32
            // node numbers from +0x60). Checked: the heaviest node lies 3 to 6 cm from its
            // vertices, median, on Kasumi's body, suit and hair.
            let palette = object_palette(tmc, &nodes, &node_objects, &object)?;
            let weight_sets: Vec<(usize, u8)> = (0..3).filter_map(|i| decl.elements.iter().find(|e| e.2 == 1 && e.3 == i).map(|e| (e.0, e.1))).collect();
            let joint_sets: Vec<(usize, u8)> = (0..3).filter_map(|i| decl.elements.iter().find(|e| e.2 == 2 && e.3 == i).map(|e| (e.0, e.1))).collect();
            let skin = match &palette {
                Some(palette) if !weight_sets.is_empty() && weight_sets.len() == joint_sets.len() => (0..used)
                    .map(|v| {
                        let at = vertex_base + vertex_at + v * decl.stride;
                        let mut out: Skin = [(0, 0.0); 12];
                        for (set, (&(wo, wt), &(jo, jt))) in weight_sets.iter().zip(&joint_sets).enumerate() {
                            ensure!(jt == 5, "node indices of type {jt:#x}");
                            for i in 0..4 {
                                let w = match wt {
                                    0xB => super::doax2::half(u16_at(tmcl, at + wo + i * 2)?),
                                    3 => f32_at(tmcl, at + wo + i * 4)?,
                                    other => bail!("weights of type {other:#x}"),
                                };
                                let j = *tmcl.get(at + jo + i).context("node index")? as usize;
                                if w > 0.0 {
                                    if let Some(&node) = palette.get(j) {
                                        out[set * 4 + i] = (node as u16, w);
                                    }
                                }
                            }
                        }
                        Ok(out)
                    })
                    .collect::<Result<Vec<_>>>()?,
                // Without weights or palette (the 31 `WGT_*_head` of `MAR_bi_08`), rigid on the
                // object's own node, which follows its parents.
                _ => match object_node_in(&nodes, &object) {
                    Some(node) => {
                        let mut one: Skin = [(0, 0.0); 12];
                        one[0] = (node as u16, 1.0);
                        vec![one; used]
                    }
                    None => Vec::new(),
                },
            };
            skins.push(skin);
            let drawn: Vec<&SubMesh> = subs.iter().filter(|sub| sub.declaration == d && sub.first_index + sub.index_count <= indices.len()).collect();
            colour_only.push(if drawn.is_empty() { vec![false] } else { drawn.iter().map(|sub| sub.entries == 1).collect() });
            let mut materials: Vec<ModelMaterial> = drawn
                .iter()
                .map(|sub| ModelMaterial {
                    indices: indices[sub.first_index..sub.first_index + sub.index_count].to_vec(),
                    textures: sub.base.into_iter().collect(),
                    cull_back: true,
                    look: None,
                })
                .collect();
            // A declaration no sub-mesh names is drawn whole, untextured.
            if materials.is_empty() {
                materials.push(ModelMaterial { indices, textures: Vec::new(), cull_back: true, look: None });
            }
            pieces.push(ModelPiece { vertices, materials });
            objects.push(object.clone());
        }
    }
    let texture_count = texture_files(tmc, tmcl).map_or(0, |t| t.len());
    let hidden = vec![false; pieces.len()];
    Ok(Model { name, nodes, pieces, objects, texture_count, colour_only, has_normals, skins, hidden })
}

/// The node named after an object. Object names longer than 15 characters are cut from the
/// front and start with `*`: `*_sweat_cheek_l` is node `WGT_sweat_cheek_l`, `*T_SHADOW_MODEL`
/// is `WGT_SHADOW_MODEL`.
pub fn object_node(model: &Model, object: &str) -> Option<usize> {
    object_node_in(&model.nodes, object)
}

/// Moves a part (face, hairstyle) onto a body: the part's `MOT01_Head` onto the body's, and each
/// object by the node named after it (`object_node`), whose rest place says where the object's
/// vertices go: Kasumi's `WGT_face` at Y -0.046 brings her face's chin (0.507 in the file) under
/// her lip nodes (0.50); `WGT_faceparts` on her head puts the eyes (Y 0.04 to 0.07 in the file)
/// at her eye nodes' height (0.56) and the teeth at the lips'; every hairstyle's `WGT_hair` is
/// its head. Read from the nodes and checked against the part's own face nodes, not against
/// the game. Objects without such a node fall back to a rule from bounds: around the head (centre
/// nearer the origin than the head's height) on the head, otherwise where they are.
pub fn place_on(body: &Model, part: &mut Model) -> Result<()> {
    let root = part.nodes.first().context("the part has no nodes")?;
    let target = body.nodes.iter().find(|n| n.name == root.name).with_context(|| format!("the body has no {}", root.name))?;
    let in_body = target.global * root.global.inverse();
    let head_height = target.global.w_axis.y;
    let on_head = target.global;
    place_objects(part, in_body, |centre| if centre.abs() < (centre - head_height).abs() { on_head } else { in_body });
    for node in &mut part.nodes {
        node.global = in_body * node.global;
    }
    Ok(())
}

/// Puts each object of a model on its node, times `base`; objects without a node get
/// `fallback` of the centre of their height.
pub fn place_objects(model: &mut Model, base: Mat4, fallback: impl Fn(f32) -> Mat4) {
    let objects: Vec<String> = model.objects.clone();
    for name in objects.iter().collect::<std::collections::BTreeSet<_>>() {
        let pieces: Vec<usize> = (0..model.pieces.len()).filter(|&i| &model.objects[i] == name).collect();
        let place = match object_node(model, name) {
            Some(node) => base * model.nodes[node].global,
            None => {
                let (low, high) = pieces
                    .iter()
                    .flat_map(|&i| &model.pieces[i].vertices)
                    .fold((f32::MAX, f32::MIN), |(lo, hi), v| (lo.min(v.position[1]), hi.max(v.position[1])));
                fallback((low + high) / 2.0)
            }
        };
        let turn = glam::Mat3::from_mat4(place);
        for &i in &pieces {
            for v in &mut model.pieces[i].vertices {
                v.position = place.transform_point3(Vec3::from(v.position)).into();
                v.normal = (turn * Vec3::from(v.normal)).normalize_or_zero().into();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn archives() -> Option<Archives> {
        Archives::open(&game_dir()).ok()
    }

    fn model(archives: &Archives, tmc: &str) -> Model {
        parse_model(&archives.read(tmc).unwrap(), &archives.read(&tmcl_path(tmc)).unwrap()).unwrap()
    }

    /// The manifests list every model with its TMCL, and each character has its 230 costumes.
    #[test]
    fn archives_list_every_character() {
        let Some(archives) = archives() else { return };
        let names: Vec<String> = archives.names().map(str::to_string).collect();
        assert!(names.len() >= 2144 + 4140);
        for (code, _) in ROSTER {
            let list = costumes(&names, code);
            assert_eq!(list.len(), 231, "{code}");
            assert!(list.iter().all(|tmc| names.contains(&tmcl_path(tmc))), "{code}");
            assert!(names.contains(&face(code)), "{code}");
            assert!(!hairstyles(&names, code).is_empty(), "{code}");
        }
    }

    /// Kasumi's face as read in Python: 3028 vertices on its first piece, at the body's head
    /// height; the hair goes on the body's head; every index stays within its piece.
    #[test]
    fn kasumi_face_hair_and_costume() {
        let Some(archives) = archives() else { return };
        let body = model(&archives, "00/character/KAS/costume/KAS_fr_00_a/KAS_KAS_FR_00_A_TMC.tmc");
        let mut face = model(&archives, &face("KAS"));
        let mut hair = model(&archives, "00/character/KAS/hair/hair_00/KAS_HAIR_00_TMC.tmc");
        assert_eq!(face.name, "KAS_face_00");
        assert_eq!(face.pieces[0].vertices.len(), 3028);
        // Two sub-meshes over the face's declaration, both on texture 4, the face's colour.
        assert_eq!(face.pieces[0].materials.iter().map(|m| m.indices.len()).sum::<usize>(), 15348);
        assert_eq!(face.pieces[0].materials.iter().map(|m| m.textures.clone()).collect::<Vec<_>>(), [vec![4], vec![4]]);
        assert_eq!(face.texture_count, 12);
        assert_eq!(body.nodes[0].name, "MOT00_Hips");
        assert_eq!(body.nodes[1].parent, Some(15));
        let head = body.nodes.iter().find(|n| n.name == "MOT01_Head").unwrap().global.w_axis;
        assert!((head.y - 0.504).abs() < 0.001, "{head}");
        place_on(&body, &mut face).unwrap();
        place_on(&body, &mut hair).unwrap();
        let top = |m: &Model| {
            m.pieces.iter().zip(&m.objects).filter(|(_, o)| drawn_at_rest(o)).flat_map(|(p, _)| &p.vertices).map(|v| v.position[1]).fold(f32::MIN, f32::max)
        };
        assert_eq!(face.objects[0], "WGT_face");
        assert_eq!(hair.objects.iter().filter(|o| drawn_at_rest(o)).count(), 3);
        // 0.718 in the file, moved down by `WGT_face`'s node (-0.046).
        assert!((top(&face) - 0.672).abs() < 0.01, "face {}", top(&face));
        // The eyes on their node: at the eye nodes' height (0.56).
        let eyes = face.objects.iter().position(|o| o == "WGT_faceparts").unwrap();
        let ys: Vec<f32> = face.pieces[eyes].materials[0].indices.iter().map(|&i| face.pieces[eyes].vertices[i as usize].position[1]).collect();
        let mid = ys.iter().sum::<f32>() / ys.len() as f32;
        assert!((mid - 0.56).abs() < 0.01, "eyes {mid}");
        assert!(top(&hair) > 0.6 && top(&hair) < 0.8, "hair {}", top(&hair));
        for m in [&body, &face, &hair] {
            for p in &m.pieces {
                assert!(p.materials.iter().flat_map(|m| &m.indices).all(|&i| (i as usize) < p.vertices.len()));
                assert!(p.vertices.iter().all(|v| v.position.iter().all(|x| x.is_finite() && x.abs() < 3.0)));
            }
        }
    }

    /// Honoka's face is modelled around the head and Ayane's hair too, though Ayane's hair file
    /// has its head at the body's height: both must end up on the head.
    #[test]
    fn parts_land_on_the_head() {
        let Some(archives) = archives() else { return };
        let names: Vec<String> = archives.names().map(str::to_string).collect();
        for (code, part) in [("HON", face("HON")), ("AYA", "00/character/AYA/hair/hair_00/AYA_HAIR_00_TMC.tmc".to_string()), ("MAR", face("MAR"))] {
            let body = model(&archives, &costumes(&names, code)[0]);
            let head = body.nodes.iter().find(|n| n.name == "MOT01_Head").unwrap().global.w_axis.y;
            let mut part = model(&archives, &part);
            place_on(&body, &mut part).unwrap();
            let ys: Vec<f32> = part.pieces.iter().zip(&part.objects).filter(|(_, o)| drawn_at_rest(o)).flat_map(|(p, _)| &p.vertices).map(|v| v.position[1]).collect();
            let top = ys.iter().cloned().fold(f32::MIN, f32::max);
            assert!(top > head + 0.1 && top < head + 0.3, "{code}: top {top}, head {head}");
        }
    }

    /// Every costume of every character parses. Slow: reads 2079 models.
    #[test]
    #[ignore]
    fn every_costume_parses() {
        let Some(archives) = archives() else { return };
        let names: Vec<String> = archives.names().map(str::to_string).collect();
        let mut failed = Vec::new();
        for (code, _) in ROSTER {
            for tmc in costumes(&names, code).iter().chain(&hairstyles(&names, code)).chain([&face(code)]) {
                let result = archives.read(tmc).and_then(|a| Ok((a, archives.read(&tmcl_path(tmc))?))).and_then(|(a, b)| parse_model(&a, &b));
                if let Err(e) = result {
                    failed.push(format!("{tmc}: {e:#}"));
                }
            }
        }
        assert!(failed.is_empty(), "{} failed: {:?}", failed.len(), &failed[..failed.len().min(10)]);
    }
}

#[cfg(test)]
mod texture_tests {
    use super::*;

    #[test]
    #[ignore]
    fn faceparts_bounds() {
        let archives = Archives::open(&game_dir()).unwrap();
        for code in ["KAS", "AYA", "HEL", "MAR", "NYO", "HON"] {
            let tmc = face(code);
            let m = parse_model(&archives.read(&tmc).unwrap(), &archives.read(&tmcl_path(&tmc)).unwrap()).unwrap();
            for (piece, object) in m.pieces.iter().zip(&m.objects) {
                for (k, mat) in piece.materials.iter().enumerate() {
                    let (mut lo, mut hi) = (Vec3::splat(9.0), Vec3::splat(-9.0));
                    for &i in &mat.indices { let p = Vec3::from(piece.vertices[i as usize].position); lo = lo.min(p); hi = hi.max(p); }
                    println!("{code} {object} m{k} tex {:?} {} tris lo {lo:.3} hi {hi:.3}", mat.textures, mat.indices.len() / 3);
                }
            }
        }
    }

    /// Parses every model in the archives and prints those that fail, with counts of pieces
    /// without a palette.
    #[test]
    #[ignore]
    fn every_model_parses() {
        let archives = Archives::open(&game_dir()).unwrap();
        let names: Vec<String> = archives.names().filter(|n| n.ends_with("_TMC.tmc")).map(str::to_string).collect();
        let (mut failed, mut unskinned, mut pieces) = (0, 0, 0);
        for name in &names {
            let (Ok(tmc), Ok(tmcl)) = (archives.read(name), archives.read(&tmcl_path(name))) else { failed += 1; continue };
            match parse_model(&tmc, &tmcl) {
                Ok(m) => {
                    for (i, _) in m.drawn_pieces() {
                        pieces += 1;
                        if m.skins[i].is_empty() {
                            unskinned += 1;
                            if unskinned < 20 {
                                println!("{name}: {} has no skin", m.objects[i]);
                            }
                        }
                    }
                }
                Err(e) => {
                    failed += 1;
                    println!("{name}: {e:#}");
                }
            }
        }
        println!("{} models, {failed} failed; {pieces} drawn pieces, {unskinned} without skin", names.len());
    }

    /// Lists the object names of every hairstyle and face, with how many files use each.
    #[test]
    #[ignore]
    fn head_object_names() {
        let archives = Archives::open(&game_dir()).unwrap();
        let names: Vec<String> = archives.names().filter(|n| n.ends_with("_TMC.tmc") && (n.contains("/hair/") || n.contains("/face/"))).map(str::to_string).collect();
        let mut counts: std::collections::BTreeMap<String, usize> = Default::default();
        for name in &names {
            let (Ok(tmc), Ok(tmcl)) = (archives.read(name), archives.read(&tmcl_path(name))) else { continue };
            let Ok(m) = parse_model(&tmc, &tmcl) else { continue };
            let mut seen: Vec<&String> = m.objects.iter().collect();
            seen.dedup();
            if seen.iter().any(|o| o.contains("wet") || o.ends_with("_a") || o.ends_with("_b") || o.contains("hel") || o.contains("FaceOff") || o.contains("face1") || o.contains("bandana") || o.contains("hair02")) {
                println!("FILE {name}: {seen:?}");
            }
            for o in seen {
                *counts.entry(o.clone()).or_default() += 1;
            }
        }
        for (o, n) in counts {
            println!("{n:3} {o}");
        }
    }

    /// Counts texture formats, tile modes and resource types over every model in the archives.
    #[test]
    #[ignore]
    fn texture_formats() {
        let archives = Archives::open(&game_dir()).unwrap();
        let names: Vec<String> = archives.names().filter(|n| n.ends_with("_TMC.tmc")).map(str::to_string).collect();
        let mut counts: std::collections::BTreeMap<(u32, u32, u32), usize> = Default::default();
        let mut failed = 0;
        for name in &names {
            let (Ok(tmc), Ok(tmcl)) = (archives.read(name), archives.read(&tmcl_path(name))) else { failed += 1; continue };
            match texture_files(&tmc, &tmcl) {
                Ok(files) => {
                    for gnf in files {
                        let (info, _) = texture_info(gnf).unwrap();
                        *counts.entry((info.format, info.tile, info.kind)).or_default() += 1;
                    }
                }
                Err(e) => {
                    failed += 1;
                    println!("{name}: {e:#}");
                }
            }
        }
        println!("{} models, {failed} failed; (format, tile, type): count {counts:?}", names.len());
    }

    /// Writes level 0 of every texture of a model as PNG, to look at the detiling.
    /// `DOAX3_TMC=path/to/X_TMC.tmc DOAX3_PNG=dir cargo test dump_textures -- --ignored`
    #[test]
    #[ignore]
    fn dump_textures() {
        let path = std::env::var("DOAX3_TMC").unwrap_or("assets/doax3/00/character/KAS/face/face_00/KAS_FACE_00_TMC.tmc".into());
        let out = std::env::var("DOAX3_PNG").unwrap_or("target/doax3_textures".into());
        std::fs::create_dir_all(&out).unwrap();
        let tmc = std::fs::read(&path).unwrap();
        let tmcl = std::fs::read(path.replace("_TMC.tmc", "_TMCL.tmcl")).unwrap();
        for (k, gnf) in texture_files(&tmc, &tmcl).unwrap().iter().enumerate() {
            let (info, _) = texture_info(gnf).unwrap();
            match decode_texture(gnf) {
                Ok(t) => {
                    let file = std::fs::File::create(format!("{out}/{k:02}.png")).unwrap();
                    let mut e = png::Encoder::new(file, t.width, t.height);
                    e.set_color(png::ColorType::Rgba);
                    e.write_header().unwrap().write_image_data(&t.rgba).unwrap();
                    println!("{k}: {info:?}");
                }
                Err(e) => println!("{k}: {info:?}: {e}"),
            }
        }
    }
}
