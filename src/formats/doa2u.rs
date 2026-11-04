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

use super::doa3::{parse_sparse_joint_skins, JointSkin, Xbe, NODE_COUNT, NODE_EYES, NODE_TWIST};
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
    }

    /// Runs DOA3's cloth over matrices read from the game and compares its vertices
    /// with the game's. DOA2U_CLOTH is an export of a capture, for a fighter and
    /// object given by DOA2U_CLOTH_ID ("fighter object costume").
    #[test]
    #[ignore]
    fn cloth_follows_captured_vertices() {
        use crate::formats::doa3_cloth::parse_cloth_records;
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
