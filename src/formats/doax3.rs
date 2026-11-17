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

use super::model::{ModelMaterial, ModelPiece, ModelVertex};

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
}

/// Whether an object is drawn in the rest view. Read from names and bounds only, nothing from
/// the game's code: `WGT_zdmodel` and `*T_SHADOW_MODEL` have no normals (depth and shadow
/// passes, probably); `WGT_body_zbreak` and `WGT_mizugi_z` repeat the body and suit in the same
/// place, for a purpose not read; `WGT_faceparts` and the `*_sweat_*` objects are modelled
/// around their own node (Y 0.04 on Kasumi's eyes) and need the node bindings, not read yet.
pub fn drawn_at_rest(object: &str) -> bool {
    object.starts_with("WGT_")
        && !object.contains("zdmodel")
        && !object.contains("zbreak")
        && !object.ends_with("_z")
        && !object.contains("faceparts")
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

    // One piece per declaration; the k-th declaration in file order uses index buffer k.
    let mut pieces = Vec::new();
    let mut objects = Vec::new();
    let mut k = 0;
    for &geo in get("MdlGeo")?.children.iter().filter(|&&g| g != 0) {
        let raw = tmc.get(geo + 0x60..geo + 0x80).context("object name")?;
        let object = String::from_utf8_lossy(raw.split(|&b| b == 0).next().unwrap_or_default()).into_owned();
        for decl in declarations(tmc, geo)? {
            let (index_at, _) = *index_buffers.get(k).context("fewer index buffers than declarations")?;
            k += 1;
            let (vertex_at, vertex_size) = *vertex_buffers.get(decl.buffer).context("vertex buffer out of range")?;
            let indices: Vec<u32> = (0..decl.index_count).map(|i| u16_at(tmcl, index_base + index_at + i * 2).map(u32::from)).collect::<Result<_>>()?;
            let used = indices.iter().max().map_or(0, |&m| m as usize + 1).max(decl.vertex_count);
            ensure!(used * decl.stride <= vertex_size + decl.stride, "indices run past vertex buffer {}", decl.buffer);
            let find = |usage: u8| decl.elements.iter().find(|e| e.2 == usage && e.3 == 0).map(|e| (e.0, e.1));
            let position = find(0).context("no positions")?;
            let normal = find(3);
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
            pieces.push(ModelPiece { vertices, materials: vec![ModelMaterial { indices, textures: Vec::new(), cull_back: true, look: None }] });
            objects.push(object.clone());
        }
    }
    Ok(Model { name, nodes, pieces, objects })
}

/// Moves a part (face, hairstyle) onto a body, object by object. Some objects are modelled
/// around the head (every hairstyle read, and Honoka's face: Y -0.02 to 0.18), others in the
/// body's space (the other eight faces: Y 0.46 to 0.74), and nothing in the files read so far
/// tells them apart: headers, sub-meshes and `BnOfsMtx` (always `GlblMtx`'s inverse) are the
/// same, and a part's own `MOT01_Head` sits at the body's head or at the origin in either case.
/// So an object whose centre lies nearer the origin than the body's head height is taken as
/// around the head and goes on the body's `MOT01_Head`; the others stay in the body's space
/// (moved by the body's head times the inverse of the part's, which is the identity for every
/// face read). A rule from bounds, not from the game: measured on all nine characters' face and
/// first hairstyle.
pub fn place_on(body: &Model, part: &mut Model) -> Result<()> {
    let root = part.nodes.first().context("the part has no nodes")?;
    let target = body.nodes.iter().find(|n| n.name == root.name).with_context(|| format!("the body has no {}", root.name))?;
    let in_body = target.global * root.global.inverse();
    let on_head = target.global;
    let head_height = target.global.w_axis.y;
    let objects: Vec<String> = part.objects.clone();
    for name in objects.iter().collect::<std::collections::BTreeSet<_>>() {
        let pieces: Vec<usize> = (0..part.pieces.len()).filter(|&i| &part.objects[i] == name).collect();
        let (low, high) = pieces
            .iter()
            .flat_map(|&i| &part.pieces[i].vertices)
            .fold((f32::MAX, f32::MIN), |(lo, hi), v| (lo.min(v.position[1]), hi.max(v.position[1])));
        let centre = (low + high) / 2.0;
        let place = if centre.abs() < (centre - head_height).abs() { on_head } else { in_body };
        let turn = glam::Mat3::from_mat4(place);
        for &i in &pieces {
            for v in &mut part.pieces[i].vertices {
                v.position = place.transform_point3(Vec3::from(v.position)).into();
                v.normal = (turn * Vec3::from(v.normal)).normalize_or_zero().into();
            }
        }
    }
    for node in &mut part.nodes {
        node.global = in_body * node.global;
    }
    Ok(())
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
        assert_eq!(face.pieces[0].materials[0].indices.len(), 15348);
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
        assert!((top(&face) - 0.718).abs() < 0.01, "face {}", top(&face));
        assert!(top(&hair) > 0.6 && top(&hair) < 0.8, "hair {}", top(&hair));
        for m in [&body, &face, &hair] {
            for p in &m.pieces {
                assert!(p.materials[0].indices.iter().all(|&i| (i as usize) < p.vertices.len()));
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
