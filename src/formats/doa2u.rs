//! Dead or Alive 2 Ultimate model files.
//!
//! Costumes are an XPR and a CAT, both encrypted with the same keystream. The
//! game decrypts them in place: `sub_00289E40` (XPR), `sub_00289DE0` (CAT) and
//! `sub_00289DA0` (the stream). The stream is MT19937 seeded with the file's
//! first word XORed with the plain magic, plus 0x31415926 on each output word.
//!
//! `OBJ\0` record, which differs from DOA3's:
//!   +0x04: extra floats after each vertex's position (32 bytes + 4 each)
//!   +0x20: vertex count
//!   +0x24: vertex buffer offset (position, weights, normal, uv)
//!   +0x28: index count
//!   +0x2C: index buffer offset (u16 triangle strip)
//!   +0xA0: material blocks up to the index buffer. Each block: +0x04 size,
//!     +0x0C flag, +0x64 layer count, +0x70 16-byte layers (texture first), and
//!     as its last 16 bytes: _, first index, triangles, _.

use anyhow::{bail, Context, Result};

use glam::Vec3;

use super::doa3::Xbe;
use super::model::{read_u32, slice, strip_to_triangles, ModelMaterial, ModelPiece, ModelVertex, VERTEX_SIZE};

const XPR_MAGIC: u32 = 0x3052_5058;
const CAT_MAGIC: u32 = 0x20;
const STREAM_OFFSET: u32 = 0x3141_5926;
const LIST_END: u32 = 0x1234_5678;

/// The standard MT19937 generator, as the game's `sub_0001FD20` and `sub_0001FEF0`.
struct Mt19937 {
    state: [u32; 624],
    next: usize,
}

impl Mt19937 {
    fn new(seed: u32) -> Self {
        let mut state = [0u32; 624];
        state[0] = seed;
        for i in 1..624 {
            let previous = state[i - 1];
            state[i] = 0x6C07_8965u32.wrapping_mul(previous ^ (previous >> 30)).wrapping_add(i as u32);
        }
        Self { state, next: 624 }
    }

    fn output(&mut self) -> u32 {
        if self.next == 624 {
            for k in 0..624 {
                let y = (self.state[k] & 0x8000_0000) | (self.state[(k + 1) % 624] & 0x7FFF_FFFF);
                let twist = if y & 1 == 1 { 0x9908_B0DF } else { 0 };
                self.state[k] = self.state[(k + 397) % 624] ^ (y >> 1) ^ twist;
            }
            self.next = 0;
        }
        let mut y = self.state[self.next];
        self.next += 1;
        y ^= y >> 11;
        y ^= (y << 7) & 0x9D2C_5680;
        y ^= (y << 15) & 0xEFC6_0000;
        y ^ (y >> 18)
    }
}

/// XORs `length / 4` words from `offset`. A range past the end of the file is cut short.
fn apply_stream(data: &mut [u8], offset: usize, length: usize, seed: u32) {
    let mut stream = Mt19937::new(seed);
    let end = offset.saturating_add(length / 4 * 4).min(data.len());
    let Some(range) = data.get_mut(offset..end) else { return };
    for word in range.chunks_exact_mut(4) {
        let key = stream.output().wrapping_add(STREAM_OFFSET);
        let plain = u32::from_le_bytes([word[0], word[1], word[2], word[3]]) ^ key;
        word.copy_from_slice(&plain.to_le_bytes());
    }
}

/// Decrypts an XPR in place. Files with a plain magic are left alone.
pub fn decrypt_xpr(data: &mut [u8]) -> Result<()> {
    let first = read_u32(data, 0)?;
    if first == XPR_MAGIC {
        return Ok(());
    }
    let size = read_u32(data, 4)? as usize;
    if size != data.len() {
        bail!("not an encrypted XPR: size field {size:#x} in a file of {:#x}", data.len());
    }
    data[..4].copy_from_slice(&XPR_MAGIC.to_le_bytes());
    apply_stream(data, 12, size - 12, first ^ XPR_MAGIC);
    Ok(())
}

/// Decrypts a CAT in place. The stream's length is the last section offset that is set.
pub fn decrypt_cat(data: &mut [u8]) -> Result<()> {
    let first = read_u32(data, 0)?;
    if first == CAT_MAGIC {
        return Ok(());
    }
    let length = [0x10, 0xC, 8, 4]
        .iter()
        .map(|&at| read_u32(data, at))
        .collect::<Result<Vec<_>>>()?
        .into_iter()
        .find(|&word| word != 0)
        .context("CAT has no section offsets")? as usize;
    data[..4].copy_from_slice(&CAT_MAGIC.to_le_bytes());
    apply_stream(data, 0x14, length, first ^ CAT_MAGIC);
    Ok(())
}

/// Bones per character, indexed by the character's ID (`sub_0005C240`).
const SKELETON_TABLE_VA: u32 = 0x6978E0;

/// A decrypted costume's objects. They are in the local space of their bones.
pub struct Model {
    pub pieces: Vec<ModelPiece>,
    /// The blend weight of each vertex, for objects that have one.
    pub weights: Vec<Option<Vec<f32>>>,
}

/// An object placed on the skeleton, using this viewer's node numbers.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DrawnObject {
    pub object: usize,
    pub node: usize,
    pub bend: Option<Bend>,
}

/// A vertex at weight `w` follows `w` of its node and `1 - w` of the parent
/// node moved by `offset` (`sub_0005AF80` and the table at 0x64C8B0).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Bend {
    pub parent: usize,
    pub offset: Vec3,
}

/// Reads a character's bones. Rows 15 and 16 are the eyes, as in DOA3's layout.
pub fn skeleton(xbe: &Xbe, character_id: usize) -> Result<[Vec3; 17]> {
    let bones = xbe.bone_table(SKELETON_TABLE_VA, character_id, 22)?;
    let mut skeleton = [Vec3::ZERO; 17];
    skeleton[..15].copy_from_slice(&bones[..15]);
    skeleton[15] = bones[17];
    skeleton[16] = bones[21];
    Ok(skeleton)
}

/// Reads which object is drawn on which node from the CAT's first draw list.
/// The list holds three bytes per object: kind, slot and object.
pub fn parse_drawn_objects(cat: &[u8], skeleton: &[Vec3; 17]) -> Result<Vec<DrawnObject>> {
    let base = read_u32(cat, 4)? as usize;
    let mut record = base;
    let list = loop {
        let value = read_u32(cat, record)? as usize;
        match read_u32(cat, record + 4)? {
            0 => bail!("CAT has no draw list"),
            1 => break base + value,
            _ => record += 8,
        }
    };

    let mut objects = Vec::new();
    for entry in cat.get(list..).context("draw list is outside the file")?.chunks_exact(3) {
        let (kind, slot, object) = (entry[0], entry[1], entry[2] as usize);
        let placed = match kind {
            1 => rigid_node(slot).map(|node| DrawnObject { object, node, bend: None }),
            2 => weighted_slot(slot, skeleton).map(|(node, bend)| DrawnObject { object, node, bend: Some(bend) }),
            _ => break,
        };
        match placed {
            Some(placed) => objects.push(placed),
            None => log::warn!("Object {object} uses unknown slot {slot} of kind {kind}"),
        }
    }
    Ok(objects)
}

/// Turns a kind 1 slot into a node of `doa3::solve_pose`. 128 and 129 go through
/// the table at 0x64C9B0 to the game's nodes 24 and 25, the forearm twists.
fn rigid_node(slot: u8) -> Option<usize> {
    match slot {
        0..=14 => Some(slot as usize),
        17 => Some(21),
        21 => Some(22),
        128 => Some(19),
        129 => Some(20),
        _ => None,
    }
}

/// Reads a kind 2 slot: thighs and shins, from the table at 0x64C8B0.
fn weighted_slot(slot: u8, skeleton: &[Vec3; 17]) -> Option<(usize, Bend)> {
    let (node, parent, offset) = match slot {
        6 => (6, 2, 6),
        7 => (12, 2, 12),
        8 => (7, 6, 7),
        9 => (13, 12, 13),
        _ => return None,
    };
    Some((node, Bend { parent, offset: skeleton[offset] }))
}

/// Reads the objects of a decrypted XPR.
pub fn parse_model(data: &[u8]) -> Result<Model> {
    if slice(data, 0x14, 4)? != b"MDL\0" {
        bail!("XPR has no MDL block");
    }
    let count = read_u32(data, 0x24)? as usize;
    let mut model = Model { pieces: Vec::with_capacity(count), weights: Vec::with_capacity(count) };
    for i in 0..count {
        let record = read_u32(data, 0x28 + i * 4)? as usize;
        let (piece, weights) = parse_object(data, record).with_context(|| format!("object {i} at {record:#x}"))?;
        model.pieces.push(piece);
        model.weights.push(weights);
    }
    Ok(model)
}

fn parse_object(data: &[u8], record: usize) -> Result<(ModelPiece, Option<Vec<f32>>)> {
    if slice(data, record, 4)? != b"OBJ\0" {
        bail!("missing OBJ magic");
    }
    let vertex_count = read_u32(data, record + 0x20)? as usize;
    let vertex_offset = read_u32(data, record + 0x24)? as usize;
    let index_count = read_u32(data, record + 0x28)? as usize;
    let index_offset = read_u32(data, record + 0x2C)? as usize;

    let extra = read_u32(data, record + 4)? as usize;
    if extra > 4 {
        bail!("{extra} extra floats per vertex");
    }
    let stride = VERTEX_SIZE + extra * 4;
    let raw = slice(data, vertex_offset, vertex_count.checked_mul(stride).context("vertex count overflow")?)?;
    let vertices: Vec<ModelVertex> = raw
        .chunks_exact(stride)
        .map(|vertex| {
            let mut packed = [0u8; VERTEX_SIZE];
            packed[..12].copy_from_slice(&vertex[..12]);
            packed[12..].copy_from_slice(&vertex[12 + extra * 4..]);
            bytemuck::pod_read_unaligned(&packed)
        })
        .collect();
    // More than one weight is used by a single costume's objects, not handled yet.
    let weights = (extra == 1).then(|| {
        raw.chunks_exact(stride).map(|v| f32::from_le_bytes([v[12], v[13], v[14], v[15]])).collect()
    });
    let strip: Vec<u32> = slice(data, index_offset, index_count.checked_mul(2).context("index count overflow")?)?
        .chunks_exact(2)
        .map(|b| u16::from_le_bytes([b[0], b[1]]) as u32)
        .collect();
    if let Some(bad) = strip.iter().find(|&&i| i as usize >= vertex_count) {
        bail!("index {bad} out of range for {vertex_count} vertices");
    }

    // (first strip index, one past its last, texture layers, back culling)
    let mut blocks = Vec::new();
    let mut block = record + 0xA0;
    while block < index_offset {
        let size = read_u32(data, block + 4)? as usize;
        // Some models end the list with this word, 8 bytes before the indices.
        if size == LIST_END as usize {
            break;
        }
        if size < 0x80 || block + size > index_offset {
            bail!("bad material block size {size:#x} at {block:#x}");
        }
        let layers = read_u32(data, block + 0x64)? as usize;
        if 0x80 + layers * 16 != size {
            bail!("{layers} layers do not fit a block of {size:#x} at {block:#x}");
        }
        let textures = (0..layers)
            .map(|i| Ok(read_u32(data, block + 0x70 + i * 16)? as usize))
            .collect::<Result<Vec<_>>>()?;
        let first = read_u32(data, block + size - 12)? as usize;
        let triangles = read_u32(data, block + size - 8)? as usize;
        if first + triangles + 2 > strip.len() {
            bail!("batch {first}+{triangles} of {} indices at {block:#x}", strip.len());
        }
        blocks.push((first, first + triangles + 2, textures, read_u32(data, block + 0xC)? == 1));
        block += size;
    }

    let materials = blocks
        .into_iter()
        .map(|(first, end, textures, cull_back)| ModelMaterial {
            indices: strip_to_triangles(&strip[first..end]),
            textures,
            cull_back,
        })
        .collect();
    Ok((ModelPiece { vertices, materials }, weights))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn draw_list_places_rigid_and_weighted_objects() {
        let mut cat = vec![0u8; 0x60];
        cat[4..8].copy_from_slice(&0x14u32.to_le_bytes());
        // Records from 0x14: (0x10, type 3), (0x20, type 1), end.
        cat[0x14..0x18].copy_from_slice(&0x10u32.to_le_bytes());
        cat[0x18..0x1C].copy_from_slice(&3u32.to_le_bytes());
        cat[0x1C..0x20].copy_from_slice(&0x20u32.to_le_bytes());
        cat[0x20..0x24].copy_from_slice(&1u32.to_le_bytes());
        cat[0x34..0x40].copy_from_slice(&[1, 0, 12, 1, 17, 3, 1, 128, 7, 2, 8, 8]);
        let mut skeleton = [Vec3::ZERO; 17];
        skeleton[7] = Vec3::new(0.0, -0.4, 0.0);
        let objects = parse_drawn_objects(&cat, &skeleton).unwrap();
        assert_eq!(objects[0], DrawnObject { object: 12, node: 0, bend: None });
        assert_eq!(objects[1], DrawnObject { object: 3, node: 21, bend: None });
        assert_eq!(objects[2], DrawnObject { object: 7, node: 19, bend: None });
        assert_eq!(
            objects[3],
            DrawnObject { object: 8, node: 7, bend: Some(Bend { parent: 6, offset: skeleton[7] }) }
        );
        assert_eq!(objects.len(), 4);
    }

    #[test]
    fn generator_matches_the_reference_output() {
        // First output of MT19937 with its default seed, from the reference code.
        assert_eq!(Mt19937::new(5489).output(), 3_499_211_612);
    }

    #[test]
    fn stream_matches_a_word_taken_from_the_game() {
        // Seed and first stream word of the costume at datadvd.afs entries 0 and 1.
        assert_eq!(Mt19937::new(0x003C_F38E).output().wrapping_add(STREAM_OFFSET), 0x40FF_6B25);
    }

    #[test]
    fn decrypting_restores_the_magic_and_is_its_own_inverse() {
        let seed = 0x0012_3456;
        let plain: Vec<u8> = (0..64u8).collect();
        let mut file = plain.clone();
        file[..4].copy_from_slice(&(XPR_MAGIC ^ seed).to_le_bytes());
        file[4..8].copy_from_slice(&64u32.to_le_bytes());
        apply_stream(&mut file, 12, 52, seed);
        decrypt_xpr(&mut file).unwrap();
        assert_eq!(&file[..4], b"XPR0");
        assert_eq!(&file[12..], &plain[12..]);
    }

    /// Run with `DOA2U_DISC=<dir with the .afs files> cargo test -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn every_encrypted_costume_parses() {
        let dir = std::env::var("DOA2U_DISC").expect("DOA2U_DISC");
        for name in ["datahdd2.afs", "datadvd.afs"] {
            let archive = crate::formats::AfsArchive::from_file(format!("{dir}/{name}")).unwrap();
            let (mut models, mut failures) = (0, 0);
            for i in 0..archive.entries().len() {
                let Some(entry) = archive.get_by_index(i) else { continue };
                if entry.len() < 32 {
                    continue;
                }
                let first = read_u32(entry, 0).unwrap();
                let is_xpr = read_u32(entry, 4).unwrap() as usize == entry.len()
                    && first != XPR_MAGIC
                    && (first ^ XPR_MAGIC) < 1 << 24;
                if !is_xpr {
                    continue;
                }
                let mut data = entry.to_vec();
                decrypt_xpr(&mut data).unwrap();
                match parse_model(&data) {
                    Ok(model) => {
                        models += 1;
                        let vertices: usize = model.pieces.iter().map(|p| p.vertices.len()).sum();
                        println!("{name} {i}: {} objects, {vertices} vertices", model.pieces.len());
                    }
                    Err(e) => {
                        failures += 1;
                        println!("{name} {i}: {e:#}");
                    }
                }
            }
            println!("{name}: {models} models, {failures} failures");
            assert_eq!(failures, 0);
        }
    }
}
