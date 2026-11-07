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

use glam::{Mat3, Mat4, Quat, Vec3};

use super::doa3::{
    parse_sparse_joint_skins, ChestMorphGroup, ChestSpringParameters, JointSkin, Xbe, NODE_COUNT, NODE_EYES, NODE_TWIST,
};
use super::doa3_chains::{parse_chain_records, parse_overlay_records, Overlay, SoftChain};
use super::doa3_cloth::{parse_cloth_records, SoftCloth};
use super::doa3_morphs::{FaceMorph, FaceShapes};
use super::model::{read_u32, slice, strip_to_triangles, MaterialLook, ModelMaterial, ModelPiece, ModelVertex, VERTEX_SIZE};

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
    // The `A0` series in `char_doa3` is plain, and starts with 1.
    if first == CAT_MAGIC || first == 1 {
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

/// Turns a kind 1 slot into a node. Slots 128 and up are matrix 24 and up.
fn rigid_node(slot: u8) -> Option<usize> {
    node_of_matrix(if slot >= 128 { slot - 128 + 24 } else { slot })
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

// ─── Pose ─────────────────────────────────────────────────────────────────────

const TRACK_COUNT: usize = 44;

/// The hand ends of the forearm twists: the game's matrices 28 and 29.
pub const NODE_TWIST_ENDS: [usize; 2] = [23, 24];

/// Turns a game matrix number into a viewer node. Slots 128 and up in a draw list
/// are matrix 24 and up (table at 0x64C9B0). 16, 18, 19, 20, 22, 23, 26, 27, 30 and
/// 31 are not built yet.
fn node_of_matrix(matrix: u8) -> Option<usize> {
    match matrix {
        0..=14 => Some(matrix as usize),
        17 => Some(NODE_EYES[0]),
        21 => Some(NODE_EYES[1]),
        24 => Some(NODE_TWIST[0]),
        25 => Some(NODE_TWIST[1]),
        28 => Some(NODE_TWIST_ENDS[0]),
        29 => Some(NODE_TWIST_ENDS[1]),
        _ => None,
    }
}

/// Reads the joint pieces, with their parents as viewer nodes. Pieces whose
/// parent has no node are left out, with a warning.
pub fn joint_skins(cat: &[u8]) -> Result<Vec<JointSkin>> {
    let table = read_u32(cat, 0x10)? as usize;
    Ok(parse_sparse_joint_skins(cat, table)
        .into_iter()
        .filter_map(|mut skin| {
            // parse_joint_skin has already mapped the hands' parents to viewer nodes.
            if skin.parent != NODE_TWIST[0] && skin.parent != NODE_TWIST[1] || matches!(skin.node, 5 | 11) == false {
                match node_of_matrix(skin.parent as u8) {
                    Some(parent) => skin.parent = parent,
                    None => {
                        log::warn!("Joint piece of object {} has parent matrix {}, which has no node", skin.object, skin.parent);
                        return None;
                    }
                }
            }
            Some(skin)
        })
        .collect())
}

/// The soft bodies of a costume: the list at CAT word 2 holds a kind and an
/// offset per body, relative to the list (`sub_0005A250`), up to a kind of 0xFF.
pub fn soft_body_offsets(cat: &[u8], wanted: u32) -> Result<(usize, Vec<usize>)> {
    let base = read_u32(cat, 8)? as usize;
    let mut offsets = Vec::new();
    for at in (base..).step_by(8) {
        let kind = read_u32(cat, at)?;
        if kind & 0xFF == 0 || kind == 0xFF {
            break;
        }
        if kind == wanted {
            offsets.push(read_u32(cat, at + 4)? as usize);
        }
    }
    Ok((base, offsets))
}

/// Whether a character's body may have soft bodies in the viewer.
pub fn allows_body_soft_bodies(character_id: usize) -> bool {
    return true;
}

/// The costume's cloth: kind 2 in the soft body list. Roots on vertices start
/// on the file's shape, and `SoftCloth::follow_vertices` keeps them up to date.
pub fn soft_cloth(cat: &[u8], model: &Model, character_id: usize) -> Result<Vec<SoftCloth>> {

    let (base, offsets) = soft_body_offsets(cat, 2)?;
    let mut cloth = parse_cloth_records(cat, base, &offsets);
    cloth.retain(|sheet| sheet.object < model.pieces.len());
    for sheet in &mut cloth {
        let vertices = &model.pieces[sheet.object].vertices;
        sheet.follow_vertices(|v| vertices.get(v).map(|vertex| Vec3::from_array(vertex.position)));
    }
    Ok(cloth)
}

/// The costume's chains: kind 1 in the soft body list, for hair and similar. On
pub fn soft_chains(cat: &[u8], model: &Model, character_id: usize) -> Result<Vec<SoftChain>> {
    let (base, offsets) = soft_body_offsets(cat, 1)?;
    let mut chains = parse_chain_records(cat, base, &offsets);
    for chain in &mut chains {
        // Bones from 0x80 are matrices from 24, as draw list slots are.
        if chain.bone >= 128 {
            chain.bone = node_of_matrix((chain.bone - 128 + 24) as u8).unwrap_or(usize::MAX);
        }
        if let Some((object, vertex)) = chain.root_vertex() {
            if let Some(position) = model.pieces.get(object).and_then(|piece| piece.vertices.get(vertex)) {
                chain.set_root(Vec3::from_array(position.position));
            }
        }
    }
    chains.retain(|chain| chain.bone != usize::MAX);
    Ok(chains)
}

/// Extra objects on a bone: kind 3 in the soft body list, with the layout of
/// DOA3's section 3. They are translucent hair layers and carried items.
pub fn extra_objects(cat: &[u8], object_count: usize) -> Result<Vec<Overlay>> {
    let (base, offsets) = soft_body_offsets(cat, 3)?;
    Ok(parse_overlay_records(cat, base, &offsets, object_count))
}

/// The game's lights for fighters in one stage area: ambient, then per light the direction
/// it travels, diffuse and specular.
#[derive(Debug, Clone, PartialEq)]
pub struct AreaLights {
    pub area: usize,
    pub name: String,
    pub ambient: [f32; 3],
    pub lights: [([f32; 3], [f32; 3], [f32; 3]); 4],
}

/// The fighters' table follows the stage's, by area number: 71 sets of four 0x50 byte records
/// from 0x6C9BF8 (`sub_00118570` reads the stage's).
const AREA_LIGHTS_VA: u32 = 0x6C9BF8 + 71 * 0x140;
const AREA_COUNT: usize = 71;

/// Area names by number, from the game's list of `ECUSTOM_` names, which runs down from 70.
/// Numbers 0 to 7 are doubtful: the list's names there do not fit its count, and the `?` says
/// so. Empty ones are named `X` numbers in the list, with no stage files.
const AREA_NAMES: [&str; AREA_COUNT] = [
    "GL1?", "AG1N?", "DS2?", "GL2?", "", "KL1?", "KL1 inner?", "DS1?", "OH", "OH2",
    "SN", "SN2", "SN3", "AM", "AG1D", "AG2 (cloudy)", "EL", "MI", "NK", "NK outer (snowy night)",
    "MO", "PO (blue sky)", "", "BP", "BL (laboratory)", "GL1Z", "AG2N", "MI3", "GC", "TF",
    "PO2", "PR (desert)", "TR", "KL2", "AG1S", "AG2S", "AL", "COL", "OL", "SA",
    "KK", "TE (storm)", "TE2", "HT", "OL2", "TF2", "DJ", "KM", "NC", "BC (beach)",
    "FC", "LT (disco)", "SV", "LC", "SEA", "DT", "SP", "SB", "", "",
    "EAR", "WL2", "WL3", "MISM", "MIAU", "MIWI", "", "", "", "", "",
];

/// Reads the fighters' lights of every named area. Kind 0 is off, 1 directional and 2 a point
/// (table at 0x7AFF38). The viewer has no point lights nor a place on the stage, so a point
/// becomes a directional light as seen from the stage's center, with its attenuation there.
pub fn area_lights(xbe: &Xbe) -> Result<Vec<AreaLights>> {
    let center = Vec3::new(0.0, 1.0, 0.0);
    let mut areas = Vec::new();
    for (area, name) in AREA_NAMES.iter().enumerate() {
        if name.is_empty() {
            continue;
        }
        let set = xbe.bytes(AREA_LIGHTS_VA + area as u32 * 0x140, 0x140).context("area lights are outside the file")?;
        let float = |at: usize| f32::from_le_bytes([set[at], set[at + 1], set[at + 2], set[at + 3]]);
        let word = |at: usize| u32::from_le_bytes([set[at], set[at + 1], set[at + 2], set[at + 3]]);
        let triple = |at: usize| Vec3::new(float(at), float(at + 4), float(at + 8));
        let mut lights = [([0.0; 3], [0.0; 3], [0.0; 3]); 4];
        for (i, light) in lights.iter_mut().enumerate() {
            let at = i * 0x50;
            let turn = std::f32::consts::TAU / 65536.0;
            let (a, b) = (word(at + 0x1C) as i32 as f32 * turn, word(at + 0x20) as i32 as f32 * turn);
            let mut direction = Vec3::new(-a.cos() * b.sin(), -a.sin(), -a.cos() * b.cos());
            let mut scale = match word(at) {
                1 => 1.0,
                2 => {
                    let offset = center - triple(at + 0x24);
                    let distance = offset.length();
                    direction = offset.normalize_or_zero();
                    let (att0, att1, att2) = (float(at + 0x30), float(at + 0x34), float(at + 0x38));
                    let denominator = att0 + att1 * distance + att2 * distance * distance;
                    if distance <= float(at + 0x3C) && denominator > 0.0 { 1.0 / denominator } else { 0.0 }
                }
                _ => 0.0,
            };
            if !scale.is_finite() {
                scale = 0.0;
            }
            let diffuse = triple(at + 0x10) * scale;
            *light = (direction.to_array(), diffuse.to_array(), (diffuse * float(at + 0x4C)).to_array());
        }
        areas.push(AreaLights { area, name: name.to_string(), ambient: triple(4).to_array(), lights });
    }
    Ok(areas)
}

/// Kind 251 in the soft body list (`sub_0004FD00`): rebuilds a moving object's normals from
/// triangle strips. +0 of the record is the object and +4 the offset of the data: +0 the
/// group count, +4 a position's byte offset per group, +8 seeds, +0xC strips, +0x10 where
/// each group's normal goes, +0x14 where its negative goes.
#[derive(Debug, Clone, PartialEq)]
pub struct NormalRebuild {
    pub object: usize,
    positions: Vec<Option<usize>>,
    /// Added to a group first: faces across a seam, from the file.
    seeds: Vec<(usize, Vec3)>,
    strips: Vec<NormalStrip>,
    outputs: Vec<Vec<usize>>,
    negated: Vec<Vec<usize>>,
}

/// Starts on two groups with a parity, or continues the last strip with a degenerate join.
#[derive(Debug, Clone, PartialEq)]
struct NormalStrip {
    start: Option<(usize, usize, usize)>,
    groups: Vec<usize>,
}

impl NormalRebuild {
    /// Each vertex with its new normal, given the object's current positions.
    pub fn normals(&self, position: impl Fn(usize) -> Option<Vec3>) -> Vec<(usize, [f32; 3])> {
        let count = self.positions.len();
        let points: Vec<Vec3> =
            self.positions.iter().map(|p| p.and_then(&position).unwrap_or(Vec3::ZERO)).collect();
        let mut sums = vec![Vec3::ZERO; count];
        for &(group, seed) in &self.seeds {
            sums[group] = seed;
        }
        let (mut edges, mut slots, mut current, mut parity) = ([Vec3::ZERO; 2], [0usize; 3], 1usize, 0usize);
        for strip in &self.strips {
            match strip.start {
                Some((first_parity, a, b)) => {
                    slots[0] = a;
                    slots[1] = b;
                    current = 1;
                    edges[first_parity] = points[b] - points[a];
                    parity = first_parity ^ 1;
                }
                None => {
                    let next = (current + 1) % 3;
                    slots[next] = slots[current];
                    edges[parity] = Vec3::ZERO;
                    parity ^= 1;
                    current = next;
                }
            }
            for &group in &strip.groups {
                let next = (current + 1) % 3;
                edges[parity] = points[group] - points[slots[current]];
                slots[next] = group;
                parity ^= 1;
                current = next;
                let normal = edges[0].cross(edges[1]);
                for slot in slots {
                    sums[slot] += normal;
                }
            }
        }
        let mut out = Vec::new();
        for (group, sum) in sums.iter().enumerate() {
            let normal = sum.normalize_or_zero();
            out.extend(self.outputs[group].iter().map(|&vertex| (vertex, normal.to_array())));
            if let Some(list) = self.negated.get(group) {
                out.extend(list.iter().map(|&vertex| (vertex, (-normal).to_array())));
            }
        }
        out
    }
}

/// Reads kind 251. Records that do not read cleanly are left out.
pub fn normal_rebuilds(cat: &[u8]) -> Result<Vec<NormalRebuild>> {
    let (base, offsets) = soft_body_offsets(cat, 251)?;
    let mut rebuilds = Vec::new();
    for offset in offsets {
        let at = base + offset;
        let object = u16::from_le_bytes(slice(cat, at, 2)?.try_into()?) as usize;
        if let Ok(rebuild) = read_normal_rebuild(cat, base, object, base + read_u32(cat, at + 4)? as usize) {
            rebuilds.push(rebuild);
        }
    }
    // A chain can name the same data at +0x5C (+0x60 at run time, `sub_00050510`), for
    // the object of its target. It also applies when the chain moves that object.
    let (_, chains) = soft_body_offsets(cat, 1)?;
    for offset in chains {
        let at = base + offset;
        let data = read_u32(cat, at + 0x5C)? as usize;
        if data == 0 {
            continue;
        }
        let head = base + read_u32(cat, at + 0x4C)? as usize;
        let object = u16::from_le_bytes(slice(cat, head + 2, 2)?.try_into()?) as usize & 0x0FFF;
        if let Ok(rebuild) = read_normal_rebuild(cat, base, object, base + data) {
            rebuilds.push(rebuild);
        }
    }
    Ok(rebuilds)
}

fn read_normal_rebuild(cat: &[u8], base: usize, object: usize, data: usize) -> Result<NormalRebuild> {
    let u16_at = |at: usize| -> Result<usize> { Ok(u16::from_le_bytes(slice(cat, at, 2)?.try_into()?) as usize) };
    let count = u16_at(data)?;
    let word = |i: usize| -> Result<usize> { Ok(read_u32(cat, data + i * 4)? as usize) };
    let list = base + word(1)?;
    let positions = (0..count)
        .map(|i| Ok(Some(read_u32(cat, list + i * 4)? as usize).filter(|&v| v != 0).map(|v| v / 32)))
        .collect::<Result<Vec<_>>>()?;
    let mut seeds = Vec::new();
    if word(2)? != 0 {
        let mut seed = base + word(2)?;
        while u16_at(seed)? != 0xFFFF {
            let group = u16_at(seed)?;
            let float = |i: usize| -> Result<f32> { Ok(f32::from_bits(read_u32(cat, seed + 4 + i * 4)?)) };
            if group >= count {
                bail!("seed for group {group} of {count}");
            }
            seeds.push((group, Vec3::new(float(0)?, float(1)?, float(2)?)));
            seed += 0x14;
        }
    }
    let mut strips = Vec::new();
    let mut cursor = base + word(3)?;
    loop {
        let length = read_u32(cat, cursor)? as usize;
        cursor += 4;
        if length == 0 {
            break;
        }
        let first = read_u32(cat, cursor)?;
        cursor += 4;
        let start = if first == u32::MAX {
            None
        } else {
            let (a, b) = (read_u32(cat, cursor)? as usize, read_u32(cat, cursor + 4)? as usize);
            cursor += 8;
            if first > 1 || a >= count || b >= count {
                bail!("bad strip start at {cursor:#x}");
            }
            Some((first as usize, a, b))
        };
        let groups = (0..length)
            .map(|i| Ok(read_u32(cat, cursor + i * 4)? as usize))
            .collect::<Result<Vec<_>>>()?;
        if groups.iter().any(|&g| g >= count) {
            bail!("bad strip group at {cursor:#x}");
        }
        cursor += length * 4;
        strips.push(NormalStrip { start, groups });
    }
    let read_outputs = |offset: usize| -> Result<Vec<Vec<usize>>> {
        let mut cursor = base + offset;
        let mut lists = Vec::with_capacity(count);
        for _ in 0..count {
            let length = read_u32(cat, cursor)? as usize;
            cursor += 4;
            let list = (0..length)
                .map(|i| Ok(read_u32(cat, cursor + i * 4)? as usize / 32))
                .collect::<Result<Vec<_>>>()?;
            cursor += length * 4;
            lists.push(list);
        }
        Ok(lists)
    };
    let outputs = read_outputs(word(4)?)?;
    let negated = if word(5)? != 0 { read_outputs(word(5)?)? } else { Vec::new() };
    Ok(NormalRebuild { object, positions, seeds, strips, outputs, negated })
}

/// Kind 250 in the soft body list: after the soft bodies run, positions of one object
/// are copied to another, raw (`sub_0004E390`). It keeps translucent layers and low
/// detail objects on the sheets that move them.
#[derive(Debug, Clone, PartialEq)]
pub struct VertexCopy {
    pub from: usize,
    pub to: usize,
    /// Source vertex and target vertex.
    pub links: Vec<(usize, usize)>,
}

fn vertex_stride(xpr: &[u8], object: usize) -> Result<usize> {
    let record = read_u32(xpr, 0x28 + object * 4)? as usize;
    Ok(VERTEX_SIZE + read_u32(xpr, record + 4)? as usize * 4)
}

/// Reads kind 250 records. +0 is a condition, +2 the source, +4 the target, +8 the offset
/// of a list: a source byte offset, then target byte offsets whose low bit means another
/// follows, and so on until -1.
pub fn vertex_copies(cat: &[u8], xpr: &[u8]) -> Result<Vec<VertexCopy>> {
    let (base, offsets) = soft_body_offsets(cat, 250)?;
    let object_count = read_u32(xpr, 0x24)? as usize;
    let mut copies = Vec::new();
    for offset in offsets {
        let at = base + offset;
        let u16_at = |at: usize| -> Result<u16> { Ok(u16::from_le_bytes(slice(cat, at, 2)?.try_into()?)) };
        let condition = u16_at(at)?;
        // The game skips 0 and odd values other than 0xFFFF.
        if condition == 0 || (condition != 0xFFFF && condition & 1 != 0) {
            continue;
        }
        let (from, to) = (u16_at(at + 2)? as usize, u16_at(at + 4)? as usize);
        if from >= object_count || to >= object_count {
            continue;
        }
        let (from_stride, to_stride) = (vertex_stride(xpr, from)?, vertex_stride(xpr, to)?);
        let mut at = base + read_u32(cat, at + 8)? as usize;
        let mut links = Vec::new();
        loop {
            let source = read_u32(cat, at)?;
            if source == u32::MAX {
                break;
            }
            at += 4;
            loop {
                let target = read_u32(cat, at)?;
                at += 4;
                if target == u32::MAX {
                    at -= 4;
                    break;
                }
                links.push((source as usize / from_stride, (target & !1) as usize / to_stride));
                if target & 1 == 0 {
                    break;
                }
            }
        }
        copies.push(VertexCopy { from, to, links });
    }
    Ok(copies)
}

/// The hands, `apv` entries 0 and 1, in the order the viewer's script mapping expects:
/// the one on node 5 first. A high bit on the object means the file's shapes are the other
/// hand's, with X negated (measured: error 0 on `lei04`). The table has two pointers per
/// target: the shape and its normals.
pub fn hands(cat: &[u8]) -> Result<Vec<crate::formats::doa3_morphs::PoseHand>> {
    let base = read_u32(cat, 0xC)? as usize;
    if slice(cat, base, 4)? != b"apv\0" || read_u32(cat, base + 4)? < 2 {
        return Ok(Vec::new());
    }
    let mut hands = Vec::with_capacity(2);
    for entry_index in [1, 0] {
        // Some entries of `pdg` and `yti` have other layouts, not read. They keep their hands.
        match read_hand(cat, base, entry_index, VERTEX_SIZE) {
            Ok(Some(hand)) => hands.push(hand),
            Ok(None) => {}
            Err(e) => log::warn!("Hand entry {entry_index} not read: {e:#}"),
        }
    }
    Ok(hands)
}

/// Reads any morph set of the `apv` section at CAT word 3, as `hands` reads the first two.
/// DOAX keeps its chest and swimsuit shapes in the same form. The lists hold byte offsets,
/// so the object's vertex size, with its extra weights, gives the index: pieces bent by
/// several bones have 36 or more bytes a vertex, not 32.
pub fn morph_set(cat: &[u8], xpr: &[u8], entry_index: usize) -> Result<Option<crate::formats::doa3_morphs::PoseHand>> {
    let base = read_u32(cat, 0xC)? as usize;
    if slice(cat, base, 4)? != b"apv\0" || read_u32(cat, base + 4)? as usize <= entry_index {
        return Ok(None);
    }
    let entry = base + read_u32(cat, base + 8 + entry_index * 4)? as usize;
    let stride = vertex_stride(xpr, (read_u32(cat, entry + 4)? & 0xFF) as usize)?;
    read_hand(cat, base, entry_index, stride)
}

pub(crate) fn morph_set_count(cat: &[u8]) -> usize {
    let Ok(base) = read_u32(cat, 0xC).map(|b| b as usize) else { return 0 };
    if slice(cat, base, 4).ok() != Some(&b"apv\0"[..]) {
        return 0;
    }
    read_u32(cat, base + 4).map_or(0, |n| n as usize)
}

fn read_hand(cat: &[u8], base: usize, entry_index: usize, stride: usize) -> Result<Option<crate::formats::doa3_morphs::PoseHand>> {
    {
        let entry = base + read_u32(cat, base + 8 + entry_index * 4)? as usize;
        let targets = read_u32(cat, entry)? as usize;
        let raw_object = read_u32(cat, entry + 4)?;
        let count = read_u32(cat, entry + 8)? as usize;
        let table = base + read_u32(cat, entry + 0x10)? as usize;
        let list = base + read_u32(cat, entry + 0x14)? as usize;
        let mirror = raw_object & 0x8000_0000 != 0;
        // Lists of byte offsets, to the position or to the normal 12 bytes after it. The
        // low bit means another follows. Normals have their own groups and count.
        let read_groups = |mut list: usize, count: usize| -> Result<Vec<Vec<usize>>> {
            let mut groups = Vec::with_capacity(count);
            for _ in 0..count {
                let mut group = Vec::new();
                loop {
                    let value = read_u32(cat, list)?;
                    list += 4;
                    group.push(((value & !1) as usize / stride) as usize);
                    if value & 1 == 0 {
                        break;
                    }
                }
                groups.push(group);
            }
            Ok(groups)
        };
        let vertices = read_groups(list, count)?;
        let normal_count = read_u32(cat, entry + 0xC)? as usize;
        let normal_vertices = read_groups(base + read_u32(cat, entry + 0x18)? as usize, normal_count)?;
        let read_list = |at: usize, count: usize| -> Result<Vec<[f32; 3]>> {
            (0..count)
                .map(|group| {
                    let bytes = slice(cat, at + group * 12, 12)?;
                    let value = |i: usize| f32::from_le_bytes([bytes[i], bytes[i + 1], bytes[i + 2], bytes[i + 3]]);
                    Ok([if mirror { -value(0) } else { value(0) }, value(4), value(8)])
                })
                .collect()
        };
        let mut shapes = Vec::with_capacity(targets);
        let mut normals = Vec::with_capacity(targets);
        for target in 0..targets {
            shapes.push(read_list(base + read_u32(cat, table + target * 8)? as usize, count)?);
            normals.push(read_list(base + read_u32(cat, table + target * 8 + 4)? as usize, normal_count)?);
        }
        if shapes.is_empty() {
            return Ok(None);
        }
        Ok(Some(crate::formats::doa3_morphs::PoseHand::new((raw_object & 0xFF) as usize, vertices, shapes, normal_vertices, normals)))
    }
}


/// `sub_0005ADA0`, with the table at 0x64C894. The rest value is half of the 0.3
/// there. Measured on Helena and Lei Fang: see the notes.
pub const CHEST_SPRING: ChestSpringParameters =
    ChestSpringParameters { stiffness: 0.075, damping: 0.9, drive: 30.0, rest: 0.15 };

/// Reads the chest's shapes, for the characters that have the spring: the object
/// and its groups. This is entry 4 of the `apv` section at CAT word 3 (`sub_0005D610`):
/// +0 targets, +4 object, +8 groups, +0x10 table of two pointers per target, +0x14
/// vertex lists. Targets 0, 1 and 2 are the base and the two extreme shapes, three
/// floats per group, with offsets from the section. A list holds vertex byte
/// offsets, where the low bit means another follows.
pub fn chest_morph(cat: &[u8], character_id: usize) -> Result<Option<(usize, Vec<ChestMorphGroup>)>> {
    let base = read_u32(cat, 0xC)? as usize;
    if slice(cat, base, 4)? != b"apv\0" || read_u32(cat, base + 4)? < 5 {
        return Ok(None);
    }
    let entry = base + read_u32(cat, base + 8 + 4 * 4)? as usize;
    if read_u32(cat, entry)? < 3 {
        return Ok(None);
    }
    let object = read_u32(cat, entry + 4)? as usize;
    let count = read_u32(cat, entry + 8)? as usize;
    let table = base + read_u32(cat, entry + 0x10)? as usize;
    let mut list = base + read_u32(cat, entry + 0x14)? as usize;
    let shape = |target: usize| -> Result<usize> { Ok(base + read_u32(cat, table + target * 8)? as usize) };
    let (first, positive, negative) = (shape(0)?, shape(1)?, shape(2)?);
    let vector = |at: usize, group: usize| -> Result<Vec3> {
        let bytes = slice(cat, at + group * 12, 12)?;
        let value = |i: usize| f32::from_le_bytes([bytes[i], bytes[i + 1], bytes[i + 2], bytes[i + 3]]);
        Ok(Vec3::new(value(0), value(4), value(8)))
    };
    let mut groups = Vec::with_capacity(count);
    for group in 0..count {
        let mut vertices = Vec::new();
        loop {
            let value = read_u32(cat, list)?;
            list += 4;
            vertices.push(((value & !1) / 32) as u16);
            if value & 1 == 0 {
                break;
            }
        }
        let rest = vector(first, group)?;
        groups.push(ChestMorphGroup {
            vertices,
            base: rest,
            positive: vector(positive, group)? - rest,
            negative: vector(negative, group)? - rest,
        });
    }
    Ok(Some((object, groups)))
}

/// Reads the shapes of an `apv` entry at CAT word 3: the vertices and floats of
/// each group for the targets asked for. See `chest_morph` for the layout.
fn apv_shapes(cat: &[u8], entry_index: usize, targets: &[usize]) -> Result<Option<(usize, Vec<Vec<usize>>, Vec<Vec<[f32; 3]>>)>> {
    let base = read_u32(cat, 0xC)? as usize;
    if slice(cat, base, 4)? != b"apv\0" || read_u32(cat, base + 4)? as usize <= entry_index {
        return Ok(None);
    }
    let entry = base + read_u32(cat, base + 8 + entry_index * 4)? as usize;
    let target_count = read_u32(cat, entry)? as usize;
    if targets.iter().any(|&t| t >= target_count) {
        return Ok(None);
    }
    let object = (read_u32(cat, entry + 4)? & 0xFF) as usize;
    let count = read_u32(cat, entry + 8)? as usize;
    let table = base + read_u32(cat, entry + 0x10)? as usize;
    let mut list = base + read_u32(cat, entry + 0x14)? as usize;
    let mut vertices = Vec::with_capacity(count);
    for _ in 0..count {
        let mut group = Vec::new();
        loop {
            let value = read_u32(cat, list)?;
            list += 4;
            group.push(((value & !1) / 32) as usize);
            if value & 1 == 0 {
                break;
            }
        }
        vertices.push(group);
    }
    let shapes = targets
        .iter()
        .map(|&target| {
            let at = base + read_u32(cat, table + target * 8)? as usize;
            (0..count)
                .map(|group| {
                    let bytes = slice(cat, at + group * 12, 12)?;
                    let value = |i: usize| f32::from_le_bytes([bytes[i], bytes[i + 1], bytes[i + 2], bytes[i + 3]]);
                    Ok([value(0), value(4), value(8)])
                })
                .collect::<Result<Vec<_>>>()
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(Some((object, vertices, shapes)))
}

/// Eyes and mouth: entries 2 and 3 of `apv`, on the head's object. Target 0 is the
/// base and target 1 the shape seen moving in a match. Their timing is DOA3's.
pub fn face(cat: &[u8]) -> Result<Option<FaceMorph>> {
    let section = |entry: usize| -> Result<Option<(usize, FaceShapes)>> {
        Ok(apv_shapes(cat, entry, &[0, 1])?.map(|(object, vertices, mut shapes)| {
            let target = shapes.pop().unwrap_or_default();
            let base = shapes.pop().unwrap_or_default();
            (object, FaceShapes { vertices, base, target })
        }))
    };
    let (Some((object, mouth))) = section(3)? else { return Ok(None) };
    let eyes = section(2)?.filter(|(eye_object, _)| *eye_object == object).map(|(_, shapes)| shapes);
    Ok(Some(FaceMorph::from_shapes(object, eyes, mouth)))
}

/// Limbs of `sub_000EB880`, from the table at 0x699600 (the origins come from
/// `sub_000E9430`). Each `*_turn` is three quarter turn counts for X, Y and Z.
struct Limb {
    upper: usize,
    mid: usize,
    end: usize,
    parent: usize,
    origin: usize,
    first_track: usize,
    length_axis: usize,
    /// The solve runs with bones along Z. These give the frames the meshes use:
    /// arm bones along X, and leg bones along -Y.
    upper_turn: [f32; 3],
    mid_turn: [f32; 3],
    negate: bool,
}

const LIMBS: [Limb; 4] = [
    Limb { upper: 14, mid: 10, end: 11, parent: 0, origin: 14, first_track: 12, length_axis: 0, upper_turn: [0.0, 1.0, 0.0], mid_turn: [0.0, -1.0, 0.0], negate: true },
    Limb { upper: 8, mid: 4, end: 5, parent: 0, origin: 8, first_track: 20, length_axis: 0, upper_turn: [0.0, -1.0, 0.0], mid_turn: [0.0, 1.0, 0.0], negate: false },
    Limb { upper: 12, mid: 13, end: 9, parent: 2, origin: 12, first_track: 28, length_axis: 1, upper_turn: [-1.0, 0.0, 1.0], mid_turn: [1.0, 0.0, 1.0], negate: true },
    Limb { upper: 6, mid: 7, end: 3, parent: 2, origin: 6, first_track: 36, length_axis: 1, upper_turn: [-1.0, 0.0, -1.0], mid_turn: [1.0, 0.0, -1.0], negate: false },
];

/// Below this cosine the solve's angle stays at a quarter turn (0x43E374). It
/// is also added to the squared distance to the target.
const SOLVE_EPSILON: f32 = 0.00097;

/// The factor given to `sub_00285B40` at the end of `sub_000EB880`.
const UPPER_ARM_BLEND: f32 = 0.3;

/// How a costume turns the forearm's end towards the hand, read from the CAT.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ForearmTwist {
    /// The hand's frame, turned so that its X meets the forearm's. Measured
    /// against the running game: see `pose_matches_captured_frames`.
    AlignHand,
    /// The forearm's frame rolled by the hand's direction (`atan2`). Follows
    /// the code; no costume using it has been compared with the game yet.
    RollForearm,
}

/// A costume's choices for the pose, from the records of the CAT's first section
/// (`sub_0005B9F0`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PoseStyle {
    pub twist: ForearmTwist,
    /// Whether upper arm 14 and upper arm 8 keep most of the chest's roll.
    pub upper_arms: [bool; 2],
}

/// Type -1 holds a version: from 2 on the twist aligns the hand, and a CAT
/// without it starts at -1. Type 10 sets flags at 0x850710: the low bit picks a
/// pair, and bits 8 to 11 and 16 to 19 are the pair's two flags. Pair 0 is the
/// upper arms; pair 1, at 0x850712, is not looked at.
pub fn pose_style(cat: &[u8]) -> Result<PoseStyle> {
    let mut record = read_u32(cat, 4)? as usize;
    let mut version = -1i32;
    let mut upper_arms = [false; 2];
    loop {
        let value = read_u32(cat, record)?;
        match read_u32(cat, record + 4)? {
            0 => break,
            0xFFFF_FFFF => version = value as i32,
            10 if value & 1 == 0 => {
                upper_arms = [(value >> 8) & 0xF != 0, (value >> 16) & 0xF != 0];
            }
            _ => {}
        }
        record += 8;
    }
    let twist = if version >= 2 { ForearmTwist::AlignHand } else { ForearmTwist::RollForearm };
    Ok(PoseStyle { twist, upper_arms })
}

fn euler(angles: &[f32]) -> Mat4 {
    Mat4::from_rotation_z(angles[2]) * Mat4::from_rotation_y(angles[1]) * Mat4::from_rotation_x(angles[0])
}

fn quarter_turns(turns: [f32; 3]) -> Mat4 {
    euler(&turns.map(|t| t * std::f32::consts::FRAC_PI_2))
}

/// `acos`, limited to a quarter turn as the solve does.
fn solve_angle(cosine: f32) -> f32 {
    if cosine >= 1.0 {
        0.0
    } else if cosine < SOLVE_EPSILON {
        std::f32::consts::FRAC_PI_2
    } else {
        cosine.acos()
    }
}

/// Builds the node matrices in the game's left handed space, relative to the
/// fighter's root: `sub_000E9430`, `sub_000EB880`. Not ported: the head's
/// `sub_000EDBF0` and feet following a sloped floor. The root is taken as having no turn.
fn solve_game_space(tracks: &[f32; TRACK_COUNT], skeleton: &[Vec3; 17], style: PoseStyle) -> [Mat4; NODE_COUNT] {
    let mut nodes = [Mat4::IDENTITY; NODE_COUNT];
    nodes[2] = Mat4::from_translation(Vec3::new(0.0, tracks[1], 0.0)) * euler(&tracks[3..6]);
    nodes[0] = nodes[2] * Mat4::from_translation(skeleton[0]) * euler(&tracks[6..9]);
    nodes[1] = nodes[0] * Mat4::from_translation(skeleton[1]) * euler(&tracks[9..12]);

    for limb in &LIMBS {
        let values = &tracks[limb.first_track..limb.first_track + 8];
        let target = Vec3::new(values[0], values[1], values[2]);
        let (pole_a, pole_b) = (values[6], values[7]);
        let upper_length = skeleton[limb.mid][limb.length_axis].abs();
        let lower_length = skeleton[limb.end][limb.length_axis].abs();

        let origin = nodes[limb.parent].transform_point3(skeleton[limb.origin]);
        let to_target = target - origin;
        let squared = to_target.length_squared() + SOLVE_EPSILON;
        let distance = squared.sqrt();
        let z = to_target / distance;

        let cosine = |near: f32, far: f32| (squared + near * near - far * far) / (near * distance * 2.0);
        let mut upper_angle = solve_angle(cosine(upper_length, lower_length));
        let mut lower_angle = solve_angle(cosine(lower_length, upper_length));
        if limb.negate {
            upper_angle = -upper_angle;
            lower_angle = -lower_angle;
        }

        let pole = Vec3::new(pole_b.sin() * pole_a.cos(), -pole_a.sin(), pole_b.cos() * pole_a.cos());
        let y = (pole - z * pole.dot(z)).normalize_or_zero();
        let x = y.cross(z);

        let upper = Mat4::from_translation(origin) * Mat4::from_mat3(Mat3::from_cols(x, y, z)) * Mat4::from_rotation_y(upper_angle);
        let mid = upper
            * Mat4::from_translation(Vec3::new(0.0, 0.0, upper_length))
            * Mat4::from_rotation_y(std::f32::consts::PI - lower_angle - upper_angle);
        let end = mid.transform_point3(Vec3::new(0.0, 0.0, -lower_length));

        nodes[limb.upper] = upper * quarter_turns(limb.upper_turn);
        nodes[limb.mid] = mid * quarter_turns(limb.mid_turn);
        nodes[limb.end] = Mat4::from_translation(end) * euler(&values[3..6]);
    }

    for (out, tip, mid, end, sign) in [
        (NODE_TWIST[1], NODE_TWIST_ENDS[1], 10, 11, -1.0f32),
        (NODE_TWIST[0], NODE_TWIST_ENDS[0], 4, 5, 1.0),
    ] {
        (nodes[out], nodes[tip]) = match style.twist {
            ForearmTwist::AlignHand => {
                let turn = Quat::from_rotation_arc(
                    nodes[end].x_axis.truncate().normalize(),
                    nodes[mid].x_axis.truncate().normalize(),
                );
                let mut matrix = Mat4::from_quat(turn) * Mat4::from_mat3(Mat3::from_mat4(nodes[end]));
                matrix.w_axis = nodes[mid].w_axis;
                let mut tip_matrix = matrix;
                tip_matrix.w_axis = nodes[end].w_axis;
                (matrix, tip_matrix)
            }
            ForearmTwist::RollForearm => {
                let hand_z = nodes[mid].inverse().transform_vector3(nodes[end].z_axis.truncate());
                let angle = (sign * hand_z.y).atan2(sign * hand_z.x);
                (nodes[mid] * Mat4::from_rotation_z(angle), nodes[mid] * Mat4::from_rotation_z(angle / 2.0))
            }
        };
    }

    // The arm's frame with the chest's roll, moved 0.3 of the way back to the arm's own.
    for (node, on) in [14, 8].into_iter().zip(style.upper_arms) {
        if on {
            let arm = nodes[node];
            let chest = Mat3::from_mat4(nodes[0]);
            let turn = Quat::from_rotation_arc(chest.x_axis.normalize(), arm.x_axis.truncate().normalize());
            let aligned = Quat::from_mat3(&Mat3::from_quat(turn).mul_mat3(&chest));
            let blended = aligned.slerp(Quat::from_mat4(&arm), UPPER_ARM_BLEND);
            let mut matrix = Mat4::from_quat(blended);
            matrix.w_axis = arm.w_axis;
            nodes[node] = matrix;
        }
    }

    for (eye, offset) in NODE_EYES.into_iter().zip([skeleton[15], skeleton[16]]) {
        nodes[eye] = nodes[1] * Mat4::from_translation(offset);
    }
    nodes
}

/// Builds the node matrices for a frame, with the viewer's Z flip applied.
pub fn solve_pose(tracks: &[f32; TRACK_COUNT], skeleton: &[Vec3; 17], style: PoseStyle) -> [Mat4; NODE_COUNT] {
    let flip_z = Mat4::from_scale(Vec3::new(1.0, 1.0, -1.0));
    solve_game_space(tracks, skeleton, style).map(|node| flip_z * node)
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

/// DOA2U's strips wind the other way from DOA3's, against the renderer's front
/// face. Without this, materials that hide back faces show their inner side: the
/// purple lining of `hel08`'s white skirt, compared with the game.
fn wound_for_viewer(mut triangles: Vec<u32>) -> Vec<u32> {
    for triangle in triangles.chunks_exact_mut(3) {
        triangle.swap(0, 1);
    }
    triangles
}

/// Reads a material block as `sub_00286C00` and `sub_00286D30` apply it. +0xC: the low two
/// bits are the cull mode, bit 2 turns fog off, bit 3 turns depth writes off. +0x20 is a
/// `D3DMATERIAL8`. +0x68: bit 1 turns specular on. +0x70 holds 16 bytes per stage: texture,
/// then at +8 a mode for the table at `0x7AFFC8`, and at +0xF how to get coordinates, where
/// 1 is a camera space reflection.
///
/// Modes seen: 0 is texture times diffuse with the material's alpha, 1 takes alpha from the
/// texture too, and 3, 4, 5 on three stages blend stage 1 over stage 0 by stage 1's alpha.
/// Modes 2 and 3 alone select the texture with no diffuse, going by the table.
fn material_look(data: &[u8], block: usize, flags: u32, layers: usize) -> Result<MaterialLook> {
    let float = |at: usize| -> Result<f32> { Ok(f32::from_bits(read_u32(data, block + at)?)) };
    let triple = |at: usize| -> Result<[f32; 3]> { Ok([float(at)?, float(at + 4)?, float(at + 8)?]) };
    let stage = |i: usize, at: usize| read_u32(data, block + 0x70 + i * 16 + at);
    let reflects = |i: usize| -> Result<bool> { Ok(i < layers && stage(i, 0xC)? >> 24 == 1) };
    let mode = if layers > 0 { stage(0, 8)? } else { 0 };
    // The layer is stage 1, except on four stages (two materials of `hel10`), where stage 1
    // repeats stage 0 over itself, which changes nothing, and the layer is stage 2.
    let layer_stage = if layers >= 3 && mode == 3 {
        let same_as_base = |i: usize| -> Result<bool> {
            Ok(stage(i, 0)? == stage(0, 0)? && stage(i, 0xC)? >> 24 == stage(0, 0xC)? >> 24)
        };
        let mut found = 1;
        for i in 1..layers {
            if stage(i, 8)? == 4 && !same_as_base(i)? {
                found = i;
                break;
            }
        }
        Some(found)
    } else {
        None
    };
    Ok(MaterialLook {
        diffuse: [float(0x20)?, float(0x24)?, float(0x28)?, float(0x2C)?],
        ambient: triple(0x30)?,
        specular: triple(0x40)?,
        emissive: triple(0x50)?,
        power: float(0x60)?,
        specular_on: read_u32(data, block + 0x68)? & 2 != 0,
        no_depth_write: flags & 8 != 0,
        alpha_from_texture: layers > 0 && mode % 2 == 1,
        layer: layer_stage.map(|i| stage(i, 0).map(|t| t as usize)).transpose()?,
        base_reflects: reflects(0)?,
        layer_reflects: reflects(layer_stage.unwrap_or(1))?,
        unlit: layers > 0 && layers < 3 && (mode == 2 || mode == 3),
    })
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
        let flags = read_u32(data, block + 0xC)?;
        blocks.push((first, first + triangles + 2, textures, flags & 3 == 1, material_look(data, block, flags, layers)?));
        block += size;
    }

    let materials = blocks
        .into_iter()
        .map(|(first, end, textures, cull_back, look)| ModelMaterial {
            indices: wound_for_viewer(strip_to_triangles(&strip[first..end])),
            textures,
            cull_back,
            look: Some(look),
        })
        .collect();
    Ok((ModelPiece { vertices, materials }, weights))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vertex_copies_read_links_as_the_game_does() {
        let words = |values: &[u32]| values.iter().flat_map(|v| v.to_le_bytes()).collect::<Vec<u8>>();
        // The list at 0x20: kind 250 at offset 0x10 and an end. The record, then its links.
        let mut cat = words(&[0, 0, 0x20, 0, 0, 0, 0, 0]);
        cat.extend(words(&[250, 0x10, 0xFF, 0]));
        cat.extend([0xFF, 0xFF, 1, 0, 0, 0, 0, 0]);
        cat.extend(words(&[0x24, 0]));
        cat.extend(words(&[0]));
        // Vertex 2 goes to 1 and 3, vertex 0 goes to 1.
        cat.extend(words(&[64, 33, 96, 0, 32, u32::MAX]));
        // Two objects whose records, at 0x40, have no extra floats.
        let mut xpr = vec![0u8; 0x60];
        xpr[0x24] = 2;
        xpr[0x28] = 0x40;
        xpr[0x2C] = 0x40;
        let copies = vertex_copies(&cat, &xpr).unwrap();
        assert_eq!(copies, vec![VertexCopy { from: 1, to: 0, links: vec![(2, 1), (2, 3), (0, 1)] }]);
    }

    /// Lists a character's hand poses by motion. DOA2U_BIN is a file stem, such as zack.
    #[test]
    #[ignore]
    fn list_hand_events() {
        let dir = std::env::var("DOA2U_DISC").expect("DOA2U_DISC");
        let stem = std::env::var("DOA2U_BIN").expect("DOA2U_BIN");
        let archive = crate::formats::AfsArchive::from_file(format!("{dir}/datahdd.afs")).unwrap();
        let events = crate::formats::doa3_morphs::parse_hand_events(archive.get_by_name(&format!("{stem}.bin")).unwrap());
        let short = &stem[..stem.len().min(6)];
        let motions: Vec<usize> = archive
            .get_by_name(&format!("{short}.mot"))
            .map(|data| crate::formats::doa3::list_motions(data).into_iter().map(|(id, _)| id).collect())
            .unwrap_or_default();
        let found = events.keys().filter(|id| motions.contains(&(**id as usize))).count();
        println!("{} motions with poses, {found} of them in {short}.mot, which has {}", events.len(), motions.len());
        let mut ids: Vec<_> = events.iter().collect();
        ids.sort_by_key(|(id, _)| **id);
        for (id, list) in ids {
            println!("{id}: {:?}", list.iter().map(|e| (e.frame, e.hand, e.pose, e.time)).collect::<Vec<_>>());
        }
    }

    /// Stages as (texture, mode, coordinates), as in `hel10` and the common three stage form.
    fn look_of_stages(stages: &[(u32, u32, u32)]) -> MaterialLook {
        let mut block = vec![0u8; 0x80 + stages.len() * 16];
        for (i, (texture, mode, coordinates)) in stages.iter().enumerate() {
            let at = 0x70 + i * 16;
            block[at..at + 4].copy_from_slice(&texture.to_le_bytes());
            block[at + 8..at + 12].copy_from_slice(&mode.to_le_bytes());
            block[at + 0xC..at + 0x10].copy_from_slice(&(coordinates << 24).to_le_bytes());
        }
        material_look(&block, 0, 0, stages.len()).unwrap()
    }

    #[test]
    fn four_stages_skip_the_stage_that_repeats_the_base() {
        let look = look_of_stages(&[(38, 3, 0), (38, 4, 0), (2, 4, 0), (2184, 5, 0)]);
        assert_eq!(look.layer, Some(2));
        assert!(!look.layer_reflects);
    }

    #[test]
    fn three_stages_keep_stage_one_as_the_layer() {
        let reflecting = look_of_stages(&[(38, 3, 0), (2, 4, 1), (2184, 5, 0)]);
        assert_eq!(reflecting.layer, Some(2));
        assert!(reflecting.layer_reflects);
        // The same texture on the same coordinates, with nothing else to choose.
        assert_eq!(look_of_stages(&[(38, 3, 0), (38, 4, 0), (2184, 5, 0)]).layer, Some(38));
    }

    #[test]
    fn rebuilt_normals_face_the_way_the_strip_winds() {
        let square = [Vec3::ZERO, Vec3::X, Vec3::Y, Vec3::new(1.0, 1.0, 0.0)];
        let rebuild = NormalRebuild {
            object: 0,
            positions: (0..4).map(Some).collect(),
            seeds: vec![(3, Vec3::new(0.0, 0.0, 5.0))],
            strips: vec![NormalStrip { start: Some((0, 0, 1)), groups: vec![2, 3] }],
            outputs: (0..4).map(|i| vec![i]).collect(),
            negated: vec![vec![9]],
        };
        let normals = rebuild.normals(|v| square.get(v).copied());
        assert!(normals.iter().filter(|n| n.0 < 4).all(|n| n.1[2] == 1.0), "{normals:?}");
        assert_eq!(normals.iter().find(|n| n.0 == 9).unwrap().1[2], -1.0);
    }

    /// Compares the executable's table with what was read from the running game. DOA2U_XBE
    /// is the executable.
    #[test]
    #[ignore]
    fn area_lights_match_the_measured_ones() {
        let xbe = Xbe::from_bytes(std::fs::read(std::env::var("DOA2U_XBE").expect("DOA2U_XBE")).unwrap()).unwrap();
        let areas = area_lights(&xbe).unwrap();
        // Area, ambient, first light's direction and diffuse, as measured.
        let measured = [
            (31, 94.0, [-0.243, -0.049, -0.969], [0.6, 0.48, 0.45]),
            (8, 63.0, [0.451, -0.29, -0.844], [0.78, 0.57, 0.42]),
            (21, 84.0, [-0.628, -0.147, -0.765], [0.72, 0.6, 0.52]),
            (48, 94.0, [0.643, -0.29, -0.709], [0.48, 0.54, 0.63]),
            (41, 76.0, [-0.593, -0.098, -0.799], [0.44, 0.52, 0.59]),
            (24, 63.0, [-0.98, -0.049, 0.195], [1.05, 0.6, 0.48]),
            (49, 94.0, [-0.539, -0.428, -0.726], [0.61, 0.53, 0.48]),
        ];
        for (area, ambient, direction, diffuse) in measured {
            let found = areas.iter().find(|a| a.area == area).unwrap();
            assert!((found.ambient[0] * 255.0 - ambient).abs() < 2.0, "area {area} ambient {:?}", found.ambient);
            for i in 0..3 {
                assert!((found.lights[0].0[i] - direction[i]).abs() < 0.002, "area {area} direction {:?}", found.lights[0].0);
                assert!((found.lights[0].1[i] - diffuse[i]).abs() < 0.002, "area {area} diffuse {:?}", found.lights[0].1);
                assert!((found.lights[0].2[i] - diffuse[i] * 0.3).abs() < 0.002, "area {area} specular {:?}", found.lights[0].2);
            }
        }
        // Disco's ambient is not gray, and its second and third lights are off.
        let disco = areas.iter().find(|a| a.area == 51).unwrap();
        assert!((disco.ambient[1] * 255.0 - 94.0).abs() < 2.0 && (disco.ambient[0] * 255.0 - 76.0).abs() < 2.0);
        assert_eq!(disco.lights[1].1, [0.0; 3]);
        println!("{} areas", areas.len());
    }

    #[test]
    fn material_blocks_read_as_the_game_applies_them() {
        let mut block = vec![0u8; 0x80 + 3 * 16];
        let put = |block: &mut Vec<u8>, at: usize, value: u32| block[at..at + 4].copy_from_slice(&value.to_le_bytes());
        put(&mut block, 0x20, 0.5f32.to_bits());
        put(&mut block, 0x60, 10f32.to_bits());
        put(&mut block, 0x68, 3);
        // Stage 0, mesh coordinates, mode 3. Stage 1, texture 7, reflection coordinates.
        put(&mut block, 0x78, 3);
        put(&mut block, 0x7C, 0x400);
        put(&mut block, 0x80, 7);
        put(&mut block, 0x8C, 0x0100_0400);
        let look = material_look(&block, 0, 9, 3).unwrap();
        assert_eq!(look.diffuse[0], 0.5);
        assert_eq!(look.power, 10.0);
        assert!(look.specular_on && look.no_depth_write && look.alpha_from_texture);
        assert_eq!(look.layer, Some(7));
        assert!(!look.base_reflects && look.layer_reflects);
        put(&mut block, 0x68, 1);
        put(&mut block, 0x78, 0);
        let plain = material_look(&block, 0, 1, 1).unwrap();
        assert!(!plain.specular_on && !plain.no_depth_write && !plain.alpha_from_texture);
        assert_eq!(plain.layer, None);
        assert!(!plain.unlit && !look.unlit);
        put(&mut block, 0x78, 3);
        assert!(material_look(&block, 0, 1, 1).unwrap().unlit);
    }

    #[test]
    fn triangles_are_wound_the_other_way() {
        assert_eq!(wound_for_viewer(vec![1, 2, 3, 4, 5, 6]), vec![2, 1, 3, 5, 4, 6]);
    }

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
    /// Compares frames read by `tools/re/doa2u_live.py capture`, as blocks of
    /// tracks, bones and the 32 matrices. Returns the worst error of arms, legs,
    /// hands and twists, and the legs' worst error per frame.
    fn worst_against_capture(capture: &str, style: PoseStyle) -> ([f32; 4], Vec<f32>) {
        let numbers = |line: &str| line.split_whitespace().skip(1).map(|v| v.parse::<f32>().unwrap()).collect::<Vec<_>>();
        let lines: Vec<&str> = capture.lines().collect();
        let mut worst = [0f32; 4];
        let mut legs_per_frame = Vec::new();
        for frame in lines.chunks(4) {
            let tracks: [f32; TRACK_COUNT] = numbers(frame[1]).try_into().unwrap();
            let bones = numbers(frame[2]);
            let mut skeleton = [Vec3::ZERO; 17];
            let row = |i: usize| Vec3::new(bones[i * 4], bones[i * 4 + 1], bones[i * 4 + 2]);
            for (i, bone) in skeleton.iter_mut().enumerate().take(15) {
                *bone = row(i);
            }
            skeleton[15] = row(17);
            skeleton[16] = row(21);
            let matrices = numbers(frame[3]);
            let game = |i: usize| Mat4::from_cols_slice(&matrices[i * 16..i * 16 + 16]);

            // The root: its turn from matrix 15, on the floor under the pelvis.
            let mut root = game(15);
            root.w_axis.y -= tracks[1];
            let nodes = solve_game_space(&tracks, &skeleton, style);
            let error = |ours: Mat4, theirs: Mat4| {
                (root * ours).to_cols_array().iter().zip(theirs.to_cols_array()).map(|(a, b)| (a - b).abs()).fold(0.0, f32::max)
            };
            let pairs: [&[(usize, usize)]; 4] = [
                &[(14, 14), (10, 10), (8, 8), (4, 4)],
                &[(12, 12), (13, 13), (6, 6), (7, 7)],
                &[(11, 11), (5, 5)],
                &[(NODE_TWIST[0], 24), (NODE_TWIST[1], 25), (NODE_TWIST_ENDS[0], 28), (NODE_TWIST_ENDS[1], 29)],
            ];
            for (slot, group) in worst.iter_mut().zip(pairs) {
                for &(ours, theirs) in group {
                    *slot = slot.max(error(nodes[ours], game(theirs)));
                }
            }
            legs_per_frame.push(pairs[1].iter().map(|&(ours, theirs)| error(nodes[ours], game(theirs))).fold(0.0, f32::max));
        }
        (worst, legs_per_frame)
    }

    /// Zack (ID 0) against Bayman (ID 24): costumes without the upper arm flags.
    #[test]
    fn pose_matches_captured_frames() {
        let style = PoseStyle { twist: ForearmTwist::AlignHand, upper_arms: [false; 2] };
        let ([arms, legs, hands, twists], _) = worst_against_capture(include_str!("doa2u_pose_capture.txt"), style);
        println!("worst: arms {arms}, legs {legs}, hands {hands}, twists {twists}");
        assert!(arms < 0.02 && legs < 0.005 && hands < 0.01 && twists < 0.01);
    }

    /// Tina (ID 1) against Lei Fang (ID 12): both have the flags on. The arms
    /// must match with the flags, and must not match without them. One frame of
    /// 12 has a leg off by up to 0.12, with its foot 5 cm away. The cause is unknown.
    #[test]
    fn upper_arms_match_captured_frames() {
        let capture = include_str!("doa2u_pose_capture_upper_arms.txt");
        let style = PoseStyle { twist: ForearmTwist::AlignHand, upper_arms: [true; 2] };
        let ([arms, legs, hands, twists], legs_per_frame) = worst_against_capture(capture, style);
        let ([without, ..], _) = worst_against_capture(capture, PoseStyle { upper_arms: [false; 2], ..style });
        println!("worst: arms {arms} ({without} without the flags), legs {legs}, hands {hands}, twists {twists}");
        assert!(arms < 0.02 && hands < 0.01 && twists < 0.01);
        assert!(without > 0.05);
        assert!(legs_per_frame.iter().filter(|&&e| e >= 0.005).count() <= 1, "{legs_per_frame:?}");
    }

    /// Looks for captured poses in the character's `.mot` files, to check that
    /// the viewer's motion tracks are the game's. DOA2U_CAPTURE is a file from
    /// `doa2u_live.py capture`, DOA2U_CAPTURE_ID the fighter's ID and DOA2U_MOTS
    /// a comma separated list of files. A fighter blending between motions finds no match.
    #[test]
    #[ignore]
    fn captured_tracks_are_found_in_motions() {
        use crate::formats::doa3::{eval_motion, list_motions, motion_length};
        let dir = std::env::var("DOA2U_DISC").expect("DOA2U_DISC");
        let archive = crate::formats::AfsArchive::from_file(format!("{dir}/datahdd.afs")).unwrap();
        let text = std::fs::read_to_string(std::env::var("DOA2U_CAPTURE").expect("DOA2U_CAPTURE")).unwrap();
        let lines: Vec<&str> = text.lines().collect();
        let wanted = format!("frame {}", std::env::var("DOA2U_CAPTURE_ID").expect("DOA2U_CAPTURE_ID"));
        let files = std::env::var("DOA2U_MOTS").expect("DOA2U_MOTS");
        let files: Vec<&str> = files.split(',').collect();
        let motions: Vec<(&str, usize, _, Vec<[f32; TRACK_COUNT]>)> = files
            .iter()
            .flat_map(|&name| {
                let data = archive.entries().iter().position(|e| e.name == name).and_then(|i| archive.get_by_index(i)).unwrap();
                list_motions(data).into_iter().map(move |(id, tracks)| {
                    let frames = (0..=motion_length(data, &tracks)).map(|f| eval_motion(data, &tracks, f as f32).unwrap()).collect();
                    (name, id, tracks, frames)
                })
            })
            .collect();
        println!("{} motions", motions.len());
        let mut errors = Vec::new();
        for frame in lines.chunks(4).filter(|f| f[0] == wanted) {
            let game: Vec<f32> = frame[1].split_whitespace().skip(1).map(|v| v.parse().unwrap()).collect();
            let difference = |ours: &[f32; TRACK_COUNT]| {
                let wrap = |d: f32| (d + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI;
                let mut worst = (ours[1] - game[1]).abs();
                for i in 3..TRACK_COUNT {
                    let position = i >= 12 && (i - 12) % 8 < 3;
                    let d = ours[i] - game[i];
                    worst = worst.max(if position { d.abs() } else { wrap(d).abs() });
                }
                worst
            };
            let best = motions
                .iter()
                .flat_map(|(name, id, _, frames)| frames.iter().enumerate().map(move |(f, v)| (difference(v), *name, *id, f)))
                .min_by(|a, b| a.0.total_cmp(&b.0))
                .unwrap();
            println!("{best:?}");
            errors.push(best.0);
        }
        errors.sort_by(f32::total_cmp);
        println!("best match, median {} of {}", errors[errors.len() / 2], errors.len());
    }

    /// Lists a costume's objects and where the CAT uses them. DOA2U_COSTUME is a file stem.
    #[test]
    #[ignore]
    fn list_costume_objects() {
        let dir = std::env::var("DOA2U_DISC").expect("DOA2U_DISC");
        let stem = std::env::var("DOA2U_COSTUME").expect("DOA2U_COSTUME");
        let archive = crate::formats::AfsArchive::from_file(format!("{dir}/datahdd2.afs")).unwrap();
        let mut xpr = archive.get_by_name(&format!("{stem}.xpr")).unwrap().to_vec();
        let mut cat = archive.get_by_name(&format!("{stem}.cat")).unwrap().to_vec();
        decrypt_xpr(&mut xpr).unwrap();
        decrypt_cat(&mut cat).unwrap();
        let model = parse_model(&xpr).unwrap();

        let base = read_u32(&cat, 4).unwrap() as usize;
        let mut lists: Vec<(u32, Vec<(u8, u8, u8)>)> = Vec::new();
        let mut record = base;
        loop {
            let (value, kind) = (read_u32(&cat, record).unwrap(), read_u32(&cat, record + 4).unwrap());
            if kind == 0 {
                break;
            }
            println!("record type {} value {value:#x}", kind as i32);
            if kind == 1 || kind == 9 {
                let list = cat[base + value as usize..]
                    .chunks_exact(3)
                    .take_while(|e| e[0] == 1 || e[0] == 2)
                    .map(|e| (e[0], e[1], e[2]))
                    .collect();
                lists.push((kind, list));
            }
            record += 8;
        }
        for (i, texture) in crate::formats::model::parse_xpr_textures(&xpr).unwrap().iter().enumerate() {
            let pixels = texture.rgba.chunks_exact(4);
            let total = pixels.len().max(1);
            let clear = pixels.filter(|p| p[3] < 26).count();
            println!("texture {i} {}x{} clear {}%", texture.width, texture.height, clear * 100 / total);
        }
        let skins = joint_skins(&cat).unwrap();
        for (i, piece) in model.pieces.iter().enumerate() {
            let (mut low, mut high) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
            for vertex in &piece.vertices {
                let position = Vec3::from_array(vertex.position);
                low = low.min(position);
                high = high.max(position);
            }
            let uses: Vec<String> = lists
                .iter()
                .flat_map(|(kind, list)| list.iter().filter(|e| e.2 as usize == i).map(move |e| format!("t{kind}:k{}s{}", e.0, e.1)))
                .chain(skins.iter().filter(|s| s.object == i).map(|s| format!("joint n{} p{}", s.node, s.parent)))
                .collect();
            let textures: Vec<_> = piece.materials.iter().map(|m| m.base_texture(usize::MAX)).collect();
            if std::env::var("DOA2U_OBJECTS").is_ok_and(|list| list.split(',').any(|o| o.parse() == Ok(i))) {
                let record = read_u32(&xpr, 0x28 + i * 4).unwrap() as usize;
                let index_offset = read_u32(&xpr, record + 0x2C).unwrap() as usize;
                let mut block = record + 0xA0;
                while block < index_offset {
                    let size = read_u32(&xpr, block + 4).unwrap() as usize;
                    if size == LIST_END as usize {
                        break;
                    }
                    let words: Vec<String> = (0..(size.min(0x80 + 16 * 4)) / 4).map(|w| format!("{:x}", read_u32(&xpr, block + w * 4).unwrap())).collect();
                    println!("  material {}", words.join(" "));
                    block += size;
                }
            }
            if std::env::var("DOA2U_WINDING").is_ok() {
                let record = read_u32(&xpr, 0x28 + i * 4).unwrap() as usize;
                let index_offset = read_u32(&xpr, record + 0x2C).unwrap() as usize;
                let mut block = record + 0xA0;
                for material in &piece.materials {
                    let (mut with, mut against) = (0, 0);
                    for t in material.indices.chunks_exact(3) {
                        let v = |k: usize| &piece.vertices[t[k] as usize];
                        let p = |k: usize| Vec3::from_array(v(k).position);
                        let face = (p(1) - p(0)).cross(p(2) - p(0));
                        let stored = Vec3::from_array(v(0).normal) + Vec3::from_array(v(1).normal) + Vec3::from_array(v(2).normal);
                        if face.dot(stored) >= 0.0 { with += 1 } else { against += 1 }
                    }
                    let flag = read_u32(&xpr, block + 0xC).unwrap();
                    let size = read_u32(&xpr, block + 4).unwrap() as usize;
                    let last = read_u32(&xpr, block + size - 4).unwrap();
                    println!("  winding obj {i} flag {flag} last {last} tris {} with normals {with} against {against}", material.indices.len() / 3);
                    block += size;
                    let _ = index_offset;
                }
            }
            println!("{i:3} v{:5} w{} [{:.2} {:.2} {:.2}]..[{:.2} {:.2} {:.2}] {uses:?} tex {textures:?}", piece.vertices.len(), model.weights[i].is_some() as u8, low.x, low.y, low.z, high.x, high.y, high.z);
        }
    }

    /// Compares joint pieces with vertices read from the game, using the game's
    /// own matrices. DOA2U_JOINTS is a capture of the object DOA2U_COSTUME.
    #[test]
    #[ignore]
    fn joint_pieces_match_captured_vertices() {
        let dir = std::env::var("DOA2U_DISC").expect("DOA2U_DISC");
        let stem = std::env::var("DOA2U_COSTUME").expect("DOA2U_COSTUME");
        let archive = crate::formats::AfsArchive::from_file(format!("{dir}/datahdd2.afs")).unwrap();
        let mut cat = archive.get_by_name(&format!("{stem}.cat")).unwrap().to_vec();
        decrypt_cat(&mut cat).unwrap();
        let skins = joint_skins(&cat).unwrap();
        let text = std::fs::read_to_string(std::env::var("DOA2U_JOINTS").expect("DOA2U_JOINTS")).unwrap();
        let mut nodes = [Mat4::IDENTITY; NODE_COUNT];
        let mut errors: std::collections::BTreeMap<usize, Vec<f32>> = Default::default();
        let mut normal_errors: std::collections::BTreeMap<usize, Vec<f32>> = Default::default();
        for line in text.lines() {
            let mut words = line.split_whitespace();
            let kind = words.next().unwrap();
            if kind == "matrices" {
                let m: Vec<f32> = words.map(|v| v.parse().unwrap()).collect();
                let game = |i: usize| Mat4::from_cols_slice(&m[i * 16..i * 16 + 16]);
                for i in 0..=14 {
                    nodes[i] = game(i);
                }
                nodes[NODE_TWIST[0]] = game(24);
                nodes[NODE_TWIST[1]] = game(25);
                nodes[NODE_TWIST_ENDS[0]] = game(28);
                nodes[NODE_TWIST_ENDS[1]] = game(29);
            } else {
                let object: usize = words.next().unwrap().parse().unwrap();
                let v: Vec<f32> = words.map(|v| v.parse().unwrap()).collect();
                let Some(skin) = skins.iter().find(|s| s.object == object) else { continue };
                let (own, parent) = (nodes[skin.node], nodes[skin.parent]);
                if kind == "normals" {
                    for vertex in &skin.vertices {
                        let live = own.transform_vector3(Vec3::new(v[vertex.vertex * 3], v[vertex.vertex * 3 + 1], v[vertex.vertex * 3 + 2]));
                        let ours = if vertex.weight >= 1.0 {
                            parent.transform_vector3(vertex.normal)
                        } else {
                            let (rotation, _) = skin.blend.frame(&own, &parent, vertex.weight);
                            rotation * vertex.normal
                        };
                        normal_errors.entry(object).or_default().push(ours.normalize_or_zero().distance(live));
                    }
                    continue;
                }
                for vertex in &skin.vertices {
                    let live = own.transform_point3(Vec3::new(v[vertex.vertex * 3], v[vertex.vertex * 3 + 1], v[vertex.vertex * 3 + 2]));
                    let ours = if vertex.weight >= 1.0 {
                        parent.transform_point3(vertex.position)
                    } else {
                        let (rotation, origin) = skin.blend.frame(&own, &parent, vertex.weight);
                        rotation * vertex.position + origin
                    };
                    errors.entry(object).or_default().push(ours.distance(live));
                }
            }
        }
        for (object, list) in &mut errors {
            list.sort_by(f32::total_cmp);
            let skin = skins.iter().find(|s| s.object == *object).unwrap();
            println!("object {object} node {} parent {}: {} samples, median {:.4}, worst {:.4}", skin.node, skin.parent, list.len(), list[list.len() / 2], list[list.len() - 1]);
        }
        for (object, list) in &mut normal_errors {
            list.sort_by(f32::total_cmp);
            println!("normals of object {object}: {} samples, median {:.4}, 95% {:.4}, worst {:.4}", list.len(), list[list.len() / 2], list[list.len() * 95 / 100], list[list.len() - 1]);
        }
    }

    /// Runs DOA3's cloth over matrices read from the game and compares its vertices
    /// with the game's. DOA2U_CLOTH is an export of a capture, for a fighter and
    /// object given by DOA2U_CLOTH_ID ("fighter object costume").
    #[test]
    #[ignore]
    fn cloth_follows_captured_vertices() {
        let dir = std::env::var("DOA2U_DISC").expect("DOA2U_DISC");
        let id = std::env::var("DOA2U_CLOTH_ID").expect("DOA2U_CLOTH_ID");
        let id: Vec<&str> = id.split_whitespace().collect();
        let (fighter, object, stem) = (id[0], id[1].parse::<usize>().unwrap(), id[2]);
        let archive = crate::formats::AfsArchive::from_file(format!("{dir}/datahdd2.afs")).unwrap();
        let mut cat = archive.get_by_name(&format!("{stem}.cat")).unwrap().to_vec();
        decrypt_cat(&mut cat).unwrap();
        let mut xpr = archive.get_by_name(&format!("{stem}.xpr")).unwrap().to_vec();
        decrypt_xpr(&mut xpr).unwrap();
        let rest: Vec<Vec3> = parse_model(&xpr).unwrap().pieces[object].vertices.iter().map(|v| Vec3::from_array(v.position)).collect();
        let (base, offsets) = soft_body_offsets(&cat, 2).unwrap();
        let sheets = parse_cloth_records(&cat, base, &offsets);
        println!("{} kind 2 records", offsets.len());
        println!("{} sheets: {:?}", sheets.len(), sheets.iter().map(|s| (s.bone, s.object)).collect::<Vec<_>>());
        let mut sheets: Vec<_> = sheets.into_iter().filter(|s| s.object == object).collect();
        assert!(!sheets.is_empty());

        let text = std::fs::read_to_string(std::env::var("DOA2U_CLOTH").expect("DOA2U_CLOTH")).unwrap();
        let (mut nodes, mut last, mut steps) = (vec![Mat4::IDENTITY; 23], f32::MIN, 0usize);
        let (mut time, mut previous): (f32, Vec<f32>) = (0.0, Vec::new());
        let (mut ours, mut still) = (Vec::new(), Vec::new());
        for line in text.lines() {
            let mut words = line.split_whitespace();
            match words.next().unwrap() {
                "read" => time = words.next().unwrap().parse().unwrap(),
                "matrices" if words.next().unwrap() == fighter => {
                    let m: Vec<f32> = words.map(|v| v.parse().unwrap()).collect();
                    if m == previous {
                        continue;
                    }
                    let game = |i: usize| Mat4::from_cols_slice(&m[i * 16..i * 16 + 16]);
                    for i in 0..=14 {
                        nodes[i] = game(i);
                    }
                    nodes[NODE_TWIST[0]] = game(24);
                    nodes[NODE_TWIST[1]] = game(25);
                    previous = m.clone();
                    // Reads miss some frames: step once for each 60 Hz frame gone by.
                    let frames = if last == f32::MIN { 1 } else { (((time - last) * 60.0).round() as usize).clamp(1, 4) };
                    last = time;
                    for _ in 0..frames {
                        for sheet in &mut sheets {
                            sheet.step(&nodes, Vec3::ZERO);
                        }
                        steps += 1;
                    }
                }
                "object" if words.next().unwrap() == fighter && words.next().unwrap().parse::<usize>().unwrap() == object && steps > 60 => {
                    let v: Vec<f32> = words.map(|v| v.parse().unwrap()).collect();
                    for sheet in &sheets {
                        for (vertex, position) in sheet.vertex_positions(&nodes) {
                            let live = Vec3::new(v[vertex * 3], v[vertex * 3 + 1], v[vertex * 3 + 2]);
                            ours.push(position.distance(live));
                            still.push(rest[vertex].distance(live));
                        }
                    }
                }
                _ => {}
            }
        }
        let summary = |list: &mut Vec<f32>| {
            list.sort_by(f32::total_cmp);
            (list[list.len() / 2], list[list.len() * 95 / 100], list[list.len() - 1])
        };
        println!("{steps} steps, {} samples", ours.len());
        println!("cloth (median, 95%, worst): {:?}", summary(&mut ours));
        println!("file's shape, no cloth:     {:?}", summary(&mut still));
    }

    /// Compares the `A0` series in DOA2U's `datadvd.afs` with DOA3's models: for each, the
    /// DOA3 model sharing the most identical pieces and textures. Needs DOA2U_DISC.
    #[test]
    #[ignore]
    fn compare_a0_series_with_doa3() {
        use crate::formats::model::{parse_xpr_model, parse_xpr_textures};
        use std::collections::{hash_map::DefaultHasher, HashSet};
        use std::hash::{Hash, Hasher};
        struct Prints { pieces: Vec<u64>, loose: Vec<u64>, textures: Vec<u64>, vertices: usize, height: f32 }
        let prints = |xpr: &[u8]| -> Option<Prints> {
            let pieces = parse_xpr_model(xpr).ok()?;
            let textures = parse_xpr_textures(xpr).ok()?;
            let hash = |piece: &crate::formats::model::ModelPiece, uv: bool| {
                let mut h = DefaultHasher::new();
                for v in &piece.vertices {
                    v.position.map(f32::to_bits).hash(&mut h);
                    if uv {
                        v.uv.map(f32::to_bits).hash(&mut h);
                    }
                }
                h.finish()
            };
            let ys = pieces.iter().flat_map(|p| p.vertices.iter().map(|v| v.position[1]));
            let (low, high) = ys.fold((f32::MAX, f32::MIN), |(a, b), y| (a.min(y), b.max(y)));
            Some(Prints {
                pieces: pieces.iter().filter(|p| !p.vertices.is_empty()).map(|p| hash(p, true)).collect(),
                loose: pieces.iter().filter(|p| !p.vertices.is_empty()).map(|p| hash(p, false)).collect(),
                textures: textures.iter().map(|t| { let mut h = DefaultHasher::new(); t.rgba.hash(&mut h); h.finish() }).collect(),
                vertices: pieces.iter().map(|p| p.vertices.len()).sum(),
                height: high - low,
            })
        };
        let doa3 = crate::formats::AfsArchive::from_file("assets/doa3/loadfile.afs").unwrap();
        let xbe = crate::formats::doa3::Xbe::from_bytes(std::fs::read("assets/doa3/default.xbe").unwrap()).unwrap();
        let roster = xbe.roster().unwrap();
        let mut starts: Vec<(usize, &str)> = roster.iter().map(|r| (r.bin_entry, r.name)).collect();
        starts.sort_unstable();
        let mut known = Vec::new();
        for i in 0..doa3.entries().len() {
            let Some(data) = doa3.get_by_index(i) else { continue };
            if data.get(..4) != Some(b"XPR0") || data.get(0x14..0x18) != Some(b"MDL\0") { continue }
            let Some(p) = prints(data) else { continue };
            let owner = starts.iter().rev().find(|(start, _)| *start <= i).map_or("?", |(_, n)| n);
            let cat = doa3.get_by_index(i + 1).unwrap_or(&[]).to_vec();
            known.push((i, owner, p, cat));
        }
        let dir = std::env::var("DOA2U_DISC").expect("DOA2U_DISC");
        let archive = crate::formats::AfsArchive::from_file(format!("{dir}/datadvd.afs")).unwrap();
        for entry in archive.entries() {
            let Some(stem) = entry.name.strip_suffix(".cat") else { continue };
            let Some(cat) = archive.get_by_name(&format!("{stem}.cat")) else { continue };
            if read_u32(cat, 0).ok() != Some(1) { continue }
            let Some(xpr) = archive.get_by_name(&format!("{stem}.xpr")) else { continue };
            let mut xpr = xpr.to_vec();
            if decrypt_xpr(&mut xpr).is_err() { continue }
            let Some(p) = prints(&xpr) else { println!("{stem}: no read"); continue };
            let shared = |a: &[u64], b: &[u64]| { let b: HashSet<_> = b.iter().collect(); a.iter().filter(|x| b.contains(x)).count() };
            let mut scored: Vec<_> = known.iter().map(|(i, owner, k, c)| {
                let same_cat = c.len() == cat.len() && c == cat;
                let bytes = if c.len() == cat.len() { c.iter().zip(cat).filter(|(a, b)| a == b).count() * 100 / cat.len().max(1) } else { 0 };
                (shared(&p.pieces, &k.pieces) + shared(&p.textures, &k.textures), shared(&p.pieces, &k.pieces), shared(&p.loose, &k.loose), shared(&p.textures, &k.textures), *i, *owner, k.pieces.len(), k.textures.len(), same_cat, bytes, c.len(), xpr.len() as i64 - doa3.get_by_index(*i).map_or(0, |d| d.len()) as i64)
            }).collect();
            scored.sort_by_key(|s| std::cmp::Reverse((s.0, s.2)));
            let b = &scored[0];
            println!("{stem}: pieces {} verts {} tex {} height {:.2} cat {} | best {} {} pieces {}/{} (no uv {}) textures {}/{} cat same {} {}% size {} xpr size diff {}",
                p.pieces.len(), p.vertices, p.textures.len(), p.height, cat.len(), b.4, b.5, b.1, b.6, b.2, b.3, b.7, b.8, b.9, b.10, b.11);
        }
    }

    /// Reads every section the viewer uses, for every costume of both archives. A0 files
    /// are plain, and the others encrypted. Prints what fails, without a body's data.
    #[test]
    #[ignore]
    fn every_costume_reads_as_the_viewer_does() {
        let dir = std::env::var("DOA2U_DISC").expect("DOA2U_DISC");
        for name in ["datahdd2.afs", "datadvd.afs"] {
            let archive = crate::formats::AfsArchive::from_file(format!("{dir}/{name}")).unwrap();
            let stems: Vec<String> = archive.entries().iter().filter_map(|e| e.name.strip_suffix(".cat").map(String::from)).collect();
            let (mut good, mut bad) = (0, 0);
            for stem in stems {
                let result = (|| -> Result<(usize, usize, usize)> {
                    let mut xpr = archive.get_by_name(&format!("{stem}.xpr")).context("no xpr")?.to_vec();
                    let mut cat = archive.get_by_name(&format!("{stem}.cat")).context("no cat")?.to_vec();
                    decrypt_xpr(&mut xpr)?;
                    decrypt_cat(&mut cat)?;
                    // The A0 series is DOA3's format, read by `doa3.rs`.
                    if read_u32(&cat, 0)? == 1 {
                        let pieces = crate::formats::model::parse_xpr_model(&xpr)?;
                        let nodes = crate::formats::doa3::parse_node_objects(&cat)?;
                        if nodes.rigid.is_empty() || pieces.iter().all(|p| p.vertices.is_empty()) {
                            bail!("nothing to draw in DOA3's format");
                        }
                        return Ok((0, 0, 0));
                    }
                    let model = parse_model(&xpr)?;
                    let drawn = parse_drawn_objects(&cat, &[Vec3::ZERO; 17]).context("draw list")?;
                    if drawn.is_empty() {
                        bail!("nothing in the draw list");
                    }
                    joint_skins(&cat).context("joints")?;
                    pose_style(&cat).context("style")?;
                    let hands = hands(&cat).context("hands")?;
                    let rebuilds = normal_rebuilds(&cat).context("rebuilds")?;
                    let copies = vertex_copies(&cat, &xpr).context("copies")?;
                    extra_objects(&cat, model.pieces.len()).context("extras")?;
                    face(&cat).context("face")?;
                    Ok((hands.len(), rebuilds.len(), copies.len()))
                })();
                match result {
                    Ok(_) => good += 1,
                    Err(e) => {
                        bad += 1;
                        println!("{name} {stem}: {e:#}");
                    }
                }
            }
            println!("{name}: {good} costumes read, {bad} failed");
        }
    }

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
