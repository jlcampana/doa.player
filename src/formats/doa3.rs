//! DOA3 character data: roster and skeleton tables from `default.xbe`, motion
//! tracks from `.mot` files, and the pose solver.
//!
//! Everything here follows the game's own code as recompiled in
//! github.com/parkerallan/doa3-recomp. Function names in comments refer to it.

use anyhow::{bail, Context, Result};
use glam::{Mat3, Mat4, Quat, Vec3};

/// 15 skeleton nodes, the forearm twist nodes 19 and 20, the two eyes, and two
/// nodes only DOA2U uses (`doa2u::NODE_TWIST_ENDS`).
pub const NODE_COUNT: usize = 25;
pub const NODE_TWIST: [usize; 2] = [19, 20];
pub const NODE_EYES: [usize; 2] = [21, 22];
const NODE_ELBOWS_AND_KNEES: [usize; 4] = [4, 7, 10, 13];
const TRACK_COUNT: usize = 44;

/// Index = character ID used by the tables in `default.xbe`. Variants that
/// share another character's files are left out by `xbe_roster`.
const CHARACTER_NAMES: [&str; 25] = [
    "Zack", "Tina", "Jann Lee", "Ein", "Ryu Hayabusa", "Kasumi", "Gen Fu", "Helena", "Leon",
    "Bass", "Hakkyoku-ken", "Hayate", "Lei Fang", "Ayane", "Hitomi", "K-1", "Pro Wrestler",
    "Tengu", "Genra", "Brad Wong", "Christie", "Taekwondo", "Demi Human", "Chin", "Bayman",
];

/// Characters to use the chest spring
const CHEST_SPRING_CHARACTER_IDS: [usize; 8] = [1, 5, 7, 12, 13, 14, 15, 20];

pub fn has_chest_spring(character_id: usize) -> bool {
    CHEST_SPRING_CHARACTER_IDS.contains(&character_id)
}
const TORSO_FRONT_CHAIN_CHARACTER_IDS: [usize; 8] = [1, 5, 7, 12, 13, 14, 15, 20];

/// Whether a character may use a soft chain hanging from the front of the
/// torso. Fixed in code, like the chest spring's list.
pub fn torso_front_chain_allowed(character_id: usize) -> bool {
    TORSO_FRONT_CHAIN_CHARACTER_IDS.contains(&character_id)
}

const SKELETON_TABLE_VA: u32 = 0x32DE78;
const MOTION_TABLE_A_VA: u32 = 0x308568;
const MOTION_TABLE_B_VA: u32 = 0x3085D0;
/// Costume tables, per character ID (`sub_000C81C2`): groups the save can
/// unlock, costumes in each group, and the costume of each position.
const COSTUME_GROUPS_VA: u32 = 0x36806C;
const COSTUME_GROUP_SIZES_VA: u32 = 0x3680F0;
const COSTUME_LISTS_VA: u32 = 0x368088;

// ─── XBE ──────────────────────────────────────────────────────────────────────

pub struct Xbe {
    data: Vec<u8>,
    /// (virtual address, file offset, raw size)
    sections: Vec<(u32, u32, u32)>,
}

fn read_u32(data: &[u8], offset: usize) -> Option<u32> {
    let bytes = data.get(offset..offset.checked_add(4)?)?;
    Some(u32::from_le_bytes(bytes.try_into().ok()?))
}

fn read_u16(data: &[u8], offset: usize) -> Option<u16> {
    let bytes = data.get(offset..offset.checked_add(2)?)?;
    Some(u16::from_le_bytes(bytes.try_into().ok()?))
}

impl Xbe {
    pub fn from_bytes(data: Vec<u8>) -> Result<Self> {
        if data.get(..4) != Some(b"XBEH") {
            bail!("not an XBE file");
        }
        let base = read_u32(&data, 0x104).context("truncated XBE")?;
        let count = read_u32(&data, 0x11C).context("truncated XBE")? as usize;
        let headers = read_u32(&data, 0x120)
            .context("truncated XBE")?
            .checked_sub(base)
            .context("bad section table")? as usize;

        let sections = (0..count)
            .map(|i| {
                let h = headers + i * 0x38;
                Some((read_u32(&data, h + 4)?, read_u32(&data, h + 12)?, read_u32(&data, h + 16)?))
            })
            .collect::<Option<Vec<_>>>()
            .context("truncated section table")?;

        Ok(Self { data, sections })
    }

    fn offset(&self, va: u32) -> Option<usize> {
        self.sections.iter().find_map(|&(start, raw, size)| {
            (va >= start && va - start < size).then(|| (raw + (va - start)) as usize)
        })
    }

    /// Reads `count` rows of a character's bone table, from a table of pointers.
    /// Bytes at a virtual address, for sections stored in the file.
    pub fn bytes(&self, va: u32, length: usize) -> Option<&[u8]> {
        let start = self.offset(va)?;
        self.data.get(start..start + length)
    }

    pub fn bone_table(&self, table_va: u32, character_id: usize, count: usize) -> Result<Vec<Vec3>> {
        let table = self.offset(table_va).context("no skeleton table")?;
        let pointer = read_u32(&self.data, table + character_id * 4).context("bad character")?;
        let start = self.offset(pointer).context("bad skeleton pointer")?;
        (0..count)
            .map(|i| {
                let at = |c: usize| {
                    read_u32(&self.data, start + i * 16 + c * 4)
                        .map(f32::from_bits)
                        .context("truncated skeleton")
                };
                Ok(Vec3::new(at(0)?, at(1)?, at(2)?))
            })
            .collect()
    }

    /// Reads the bone offsets of a character (`sub_00091C30`).
    pub fn skeleton(&self, character_id: usize) -> Result<[Vec3; 17]> {
        let rows = self.bone_table(SKELETON_TABLE_VA, character_id, 17)?;
        let mut bones = [Vec3::ZERO; 17];
        bones.copy_from_slice(&rows);
        Ok(bones)
    }

    /// Returns the characters with their own files, sorted by name.
    /// The costumes a player can pick, in the game's order. The disc holds more
    /// models than this for some characters. `None` if the tables do not say.
    pub fn selectable_costumes(&self, id: usize) -> Option<Vec<usize>> {
        let pointer = |table: u32| {
            let at = self.offset(table)? + id * 4;
            let value = read_u32(&self.data, at)?;
            self.offset(value)
        };
        let groups = *self.data.get(self.offset(COSTUME_GROUPS_VA)? + id)? as usize;
        let sizes = pointer(COSTUME_GROUP_SIZES_VA)?;
        let total: usize = (0..groups).map(|g| self.data.get(sizes + g).map(|&n| n as usize)).sum::<Option<_>>()?;
        let list = pointer(COSTUME_LISTS_VA)?;
        (0..total).map(|i| self.data.get(list + i).map(|&c| c as usize)).collect()
    }

    pub fn roster(&self) -> Result<Vec<RosterEntry>> {
        let table_a = self.offset(MOTION_TABLE_A_VA).context("no motion table")?;
        let table_b = self.offset(MOTION_TABLE_B_VA).context("no motion table")?;

        let mut roster: Vec<RosterEntry> = Vec::new();
        for (id, name) in CHARACTER_NAMES.iter().enumerate() {
            let read = |base: usize, field: usize| {
                read_u16(&self.data, base + id * 4 + field).map(usize::from).context("truncated table")
            };
            let (bin, motion_a) = (read(table_a, 0)?, read(table_a, 2)?);
            let motion_b = read(table_b, 2)?;
            if !roster.iter().any(|r| r.bin_entry == bin) {
                roster.push(RosterEntry { id, name, bin_entry: bin, motion_files: [motion_a, motion_b] });
            }
        }
        roster.sort_by_key(|r| r.name);
        Ok(roster)
    }
}

#[derive(Debug, Clone)]
pub struct RosterEntry {
    pub id: usize,
    pub name: &'static str,
    pub bin_entry: usize,
    pub motion_files: [usize; 2],
}

// ─── Motion tracks ────────────────────────────────────────────────────────────

/// Offsets of the 44 tracks of one motion within its file.
pub type MotionTracks = [u32; TRACK_COUNT];

fn record_value(data: &[u8], offset: usize) -> Option<f32> {
    let head = read_u16(data, offset)? as i16 as i32;
    let low = read_u16(data, offset + 2)? as i32;
    Some((((head & !0xFFF) << 4) | low) as f32 * 1e-4)
}

fn read_i32_24(data: &[u8], low: usize, high: usize) -> Option<f32> {
    let value = ((*data.get(high)? as i8 as i32) << 16) | read_u16(data, low)? as i32;
    Some(value as f32)
}

/// Evaluates a track at frame `t` (`sub_00095320`, `sub_00095400`).
fn eval_track(data: &[u8], start: u32, t: f32) -> Option<f32> {
    let mut offset = start as usize;
    let mut segment_start = 0f32;
    loop {
        let head = read_u16(data, offset)?;
        let dt = ((head >> 2) & 0x3FF) as f32;
        if head & 0xFFF == 0 {
            return record_value(data, offset);
        }
        let kind = head & 3;
        let size = [10, 4, 6, 8][kind as usize];
        if t < segment_start + dt {
            let v0 = record_value(data, offset)?;
            let v1 = record_value(data, offset + size)?;
            let x = t - segment_start;
            let slope = (v1 - v0) / dt;
            let hermite = |m0: f32, m1: f32| {
                let c2 = (3.0 * slope - 2.0 * m0 - m1) / dt;
                let c3 = (m0 + m1 - 2.0 * slope) / (dt * dt);
                ((c3 * x + c2) * x + m0) * x + v0
            };
            return Some(match kind {
                0 => hermite(
                    read_i32_24(data, offset + 4, offset + 6)? * 1e-6,
                    read_i32_24(data, offset + 8, offset + 7)? * 1e-6,
                ),
                1 => slope * x + v0,
                2 => {
                    let m = read_u16(data, offset + 4)? as i16 as f32 / 32768.0;
                    ((-m * x) + (slope + m * dt)) * x + v0
                }
                _ => hermite(
                    read_u16(data, offset + 4)? as i16 as f32 / 131072.0,
                    read_u16(data, offset + 6)? as i16 as f32 / 131072.0,
                ),
            });
        }
        segment_start += dt;
        offset += size;
    }
}

/// Lists the motions of a `.mot` file as (motion ID, track offsets).
pub fn list_motions(data: &[u8]) -> Vec<(usize, MotionTracks)> {
    let tracks_at = |block: usize| -> Option<MotionTracks> {
        let mut tracks = [0u32; TRACK_COUNT];
        let mut previous = 0;
        for (i, track) in tracks.iter_mut().enumerate() {
            *track = read_u32(data, block + i * 4)?;
            if *track <= previous || *track as usize >= block {
                return None;
            }
            previous = *track;
        }
        Some(tracks)
    };

    // The first table entry points at a placeholder motion, used for every
    // unused ID. Its first track starts right after the table.
    let Some(placeholder) = read_u32(data, 0) else { return Vec::new() };
    let Some(first_track) = read_u32(data, placeholder as usize) else { return Vec::new() };
    let table_len = (first_track as usize / 4).min(data.len() / 4);

    (0..table_len)
        .filter_map(|id| {
            let block = read_u32(data, id * 4)?;
            if block == placeholder {
                return None;
            }
            Some((id, tracks_at(block as usize)?))
        })
        .collect()
}

/// Number of frames until the last track reaches its final key.
pub fn motion_length(data: &[u8], tracks: &MotionTracks) -> u32 {
    tracks
        .iter()
        .map(|&start| {
            let (mut offset, mut frames) = (start as usize, 0u32);
            while let Some(head) = read_u16(data, offset) {
                if head & 0xFFF == 0 {
                    break;
                }
                frames += ((head >> 2) & 0x3FF) as u32;
                offset += [10, 4, 6, 8][(head & 3) as usize];
            }
            frames
        })
        .max()
        .unwrap_or(0)
}

pub fn eval_motion(data: &[u8], tracks: &MotionTracks, frame: f32) -> Result<[f32; TRACK_COUNT]> {
    let mut values = [0f32; TRACK_COUNT];
    for (value, &track) in values.iter_mut().zip(tracks) {
        *value = eval_track(data, track, frame).context("motion track out of bounds")?;
    }
    Ok(values)
}

// ─── Pose ─────────────────────────────────────────────────────────────────────

fn euler(angles: &[f32]) -> Mat4 {
    Mat4::from_rotation_z(angles[2]) * Mat4::from_rotation_y(angles[1]) * Mat4::from_rotation_x(angles[0])
}

fn translation(v: Vec3) -> Mat4 {
    Mat4::from_translation(v)
}

struct Limb {
    upper: usize,
    mid: usize,
    end: usize,
    first_track: usize,
    parent: usize,
    length_axis: usize,
    negate: bool,
}

/// `sub_00090967` for the track layout, `0x32EFC0` for the node chains.
const LIMBS: [Limb; 4] = [
    Limb { upper: 14, mid: 10, end: 11, first_track: 12, parent: 0, length_axis: 0, negate: true },
    Limb { upper: 8, mid: 4, end: 5, first_track: 20, parent: 0, length_axis: 0, negate: false },
    Limb { upper: 12, mid: 13, end: 9, first_track: 28, parent: 2, length_axis: 1, negate: true },
    Limb { upper: 6, mid: 7, end: 3, first_track: 36, parent: 2, length_axis: 1, negate: false },
];

/// Builds the node matrices for a frame (`sub_00091C30`, `sub_00091720`,
/// `sub_00094660`). The game uses a left-handed space; the viewer's Z flip is
/// applied here so that the result can be drawn directly.
pub fn solve_pose(tracks: &[f32; TRACK_COUNT], skeleton: &[Vec3; 17]) -> [Mat4; NODE_COUNT] {
    let mut nodes = [Mat4::IDENTITY; NODE_COUNT];

    nodes[2] = translation(Vec3::new(0.0, tracks[1], 0.0)) * euler(&tracks[3..6]);
    nodes[0] = nodes[2] * translation(skeleton[0]) * euler(&tracks[6..9]);
    nodes[1] = nodes[0] * translation(skeleton[1]) * euler(&tracks[9..12]);

    for limb in &LIMBS {
        let values = &tracks[limb.first_track..limb.first_track + 8];
        let target = Vec3::new(values[0], values[1], values[2]);
        let (pole_a, pole_b) = (values[6], values[7]);

        let upper_length = skeleton[limb.mid][limb.length_axis].abs();
        let lower_length = skeleton[limb.end][limb.length_axis].abs();
        let reach = upper_length + lower_length;

        let origin = nodes[limb.parent].transform_point3(skeleton[limb.upper]);
        let to_target = target - origin;
        let distance = to_target.length().max(1e-6);
        let z = to_target / distance;

        let (end_position, mut upper_angle, mut lower_angle) = if distance >= reach {
            (origin + z * reach, 0.0, 0.0)
        } else {
            let cosine = |near: f32, far: f32| {
                ((distance * distance + near * near - far * far) / (2.0 * near * distance))
                    .clamp(-1.0, 1.0)
                    .acos()
            };
            (target, cosine(upper_length, lower_length), cosine(lower_length, upper_length))
        };
        if limb.negate {
            upper_angle = -upper_angle;
            lower_angle = -lower_angle;
        }

        let pole = Vec3::new(
            pole_b.sin() * pole_a.cos(),
            -pole_a.sin(),
            pole_b.cos() * pole_a.cos(),
        );
        let y = (pole - z * pole.dot(z)).normalize_or_zero();
        let x = y.cross(z);

        let upper = translation(origin)
            * Mat4::from_mat3(Mat3::from_cols(x, y, z))
            * Mat4::from_rotation_y(upper_angle);
        nodes[limb.upper] = upper;
        nodes[limb.mid] = upper
            * translation(Vec3::new(0.0, 0.0, upper_length))
            * Mat4::from_rotation_y(std::f32::consts::PI - lower_angle - upper_angle);
        nodes[limb.end] = translation(end_position) * euler(&values[3..6]);
    }

    // Forearm twist: rolls the forearm's end towards the hand (end of `sub_00094660`).
    for (out, mid, end, sign) in [(20, 10, 11, -1.0f32), (19, 4, 5, 1.0)] {
        let hand_z = nodes[mid].inverse().transform_vector3(nodes[end].z_axis.truncate());
        let angle = (sign * hand_z.y).atan2(sign * hand_z.x);
        nodes[out] = nodes[mid] * Mat4::from_rotation_z(angle);
    }

    for (eye, offset) in NODE_EYES.into_iter().zip([skeleton[15], skeleton[16]]) {
        nodes[eye] = nodes[1] * translation(offset);
    }

    let flip_z = Mat4::from_scale(Vec3::new(1.0, 1.0, -1.0));
    nodes.map(|node| flip_z * node)
}

// ─── CAT: which objects belong to which node ──────────────────────────────────

/// Which objects belong to which node.
#[derive(Debug, Clone, Default)]
pub struct NodeObjects {
    /// (node, object) for pieces that move rigidly with one node.
    pub rigid: Vec<(usize, usize)>,
    /// Pieces that also weld some vertices to the parent node.
    pub skins: Vec<JointSkin>,
}

#[derive(Debug, Clone)]
pub struct JointSkin {
    pub node: usize,
    pub parent: usize,
    pub object: usize,
    pub blend: JointBlend,
    pub vertices: Vec<SkinVertex>,
    pub welds: Vec<JointWeld>,
}

/// How a joint piece bends between its node and the parent node. Formula
/// measured from the running game: see docs/doa3-internals.md.
#[derive(Debug, Clone, Copy)]
pub struct JointBlend {
    /// Takes the node's axes to the parent's at rest.
    pub rest_rotation: Mat3,
    /// Start of the line the frame origins follow, in the node's space.
    pub own_point: Vec3,
    /// End of that line, in the parent's space.
    pub parent_point: Vec3,
}

impl JointBlend {
    /// Returns the rotation and origin for a vertex group of the given weight.
    pub fn frame(&self, own: &Mat4, parent: &Mat4, weight: f32) -> (Mat3, Vec3) {
        let parent_rotation = Mat3::from_mat4(*parent);
        let relative = parent_rotation.inverse() * Mat3::from_mat4(*own);
        let delta = Quat::from_mat3(&(relative * self.rest_rotation.inverse()));
        let partial = Quat::IDENTITY.slerp(delta, 1.0 - weight);
        let rotation = parent_rotation * Mat3::from_quat(partial) * self.rest_rotation;
        let origin = own
            .transform_point3(self.own_point)
            .lerp(parent.transform_point3(self.parent_point), weight);
        (rotation, origin)
    }
}

/// A vertex of a joint piece, grouped by weight.
#[derive(Debug, Clone, Copy)]
pub struct SkinVertex {
    /// Vertex of the object.
    pub vertex: usize,
    pub position: Vec3,
    pub normal: Vec3,
    /// In 1/16 units. At 1, position and normal are in the parent's space;
    /// below that they are in the frame given by `JointBlend::frame`.
    pub weight: f32,
}

/// Vertices of a joint piece that sit on a vertex of a rigid piece.
#[derive(Debug, Clone)]
pub struct JointWeld {
    pub object: usize,
    pub links: Vec<(usize, Vec<usize>)>,
}

fn read_vec3(data: &[u8], offset: usize) -> Option<Vec3> {
    let at = |i: usize| read_u32(data, offset + i * 4).map(f32::from_bits);
    Some(Vec3::new(at(0)?, at(1)?, at(2)?))
}

fn find_node_table(cat: &[u8]) -> Option<usize> {
    let valid = |table: usize| -> Option<Vec<u32>> {
        let first = read_u32(cat, table)?;
        if first % 4 != 0 || !(0x10..=0x100).contains(&first) {
            return None;
        }
        let count = first as usize / 4;
        let entries: Vec<u32> = (0..count).map(|i| read_u32(cat, table + i * 4)).collect::<Option<_>>()?;
        let (last, rest) = entries.split_last()?;
        if *last != 0 || rest.windows(2).any(|w| w[0] >= w[1]) {
            return None;
        }
        for &entry in rest {
            let index_offset = read_u32(cat, table + entry as usize)?;
            let vertex_offset = read_u32(cat, table + entry as usize + 4)?;
            if vertex_offset <= entry || index_offset <= vertex_offset {
                return None;
            }
        }
        Some(entries)
    };

    // Some files scramble the 5-word header, so fall back to a scan.
    if read_u32(cat, 0) == Some(1) && read_u32(cat, 4) == Some(0x14) {
        let table = read_u32(cat, 16)? as usize;
        if valid(table).is_some() {
            return Some(table);
        }
    }
    (0x14..cat.len().saturating_sub(0x40)).step_by(4).find(|&table| valid(table).is_some())
}

/// Reads a node's joint record: 10 header vec4s, then position and normal
/// pairs in the parent's space. A list groups them by weight in 1/16 units,
/// each with the byte offsets of the 32-byte object vertices they replace.
fn parse_joint_skin(cat: &[u8], table: usize, entry: usize, end: usize) -> Option<JointSkin> {
    let record = table + entry;
    let list_offset = read_u32(cat, record)? as usize;
    let data_offset = read_u32(cat, record + 4)? as usize;
    let flags = read_u32(cat, record + 8)?;
    let count = (list_offset.checked_sub(data_offset)? / 16).checked_sub(10)? / 2;

    let mut vertices = Vec::new();
    let mut offset = table + list_offset;
    let list_end = table + end;
    let mut index = 0;
    while index < count && offset + 4 <= list_end {
        let weight = read_u32(cat, offset)? as f32 / 16.0;
        offset += 4;
        let mut empty = true;
        while offset + 4 <= list_end {
            let value = read_u32(cat, offset)?;
            offset += 4;
            if value == u32::MAX {
                if empty {
                    break;
                }
                index += 1;
                empty = true;
                continue;
            }
            empty = false;
            if index < count {
                let pair = table + data_offset + (10 + index * 2) * 16;
                vertices.push(SkinVertex {
                    vertex: value as usize / 32,
                    position: read_vec3(cat, pair)?,
                    normal: read_vec3(cat, pair + 16)?,
                    weight,
                });
            }
        }
    }

    let node = ((flags >> 8) & 0xFF) as usize;
    let parent = match node {
        // Hands bend against their own forearm's twist node. Some costumes (Ayane
        // 01 and 02, Jann Lee 00) store the two twist nodes crossed.
        5 => NODE_TWIST[0],
        11 => NODE_TWIST[1],
        _ => ((flags >> 16) & 0xFF) as usize,
    };

    // Header: rest rotation rows in pairs 0..=2, and two pairs of line ends.
    // Elbows and knees use the second pair, measured against the game.
    let header = |i: usize| read_vec3(cat, table + data_offset + i * 16);
    let rest_rotation = Mat3::from_cols(header(1)?, header(3)?, header(5)?);
    let (own_point, parent_point) = if NODE_ELBOWS_AND_KNEES.contains(&node) {
        (header(9)?, header(8)?)
    } else {
        (header(6)?, header(7)?)
    };

    // Optional streams: a rigid object, then lists of its vertex followed by
    // the joint vertices that sit on it.
    let mut welds = Vec::new();
    for slot in 0..((flags >> 28) as usize).min(2) {
        let stream = read_u32(cat, record + 12 + slot * 4)? as usize;
        if stream == 0 {
            continue;
        }
        let mut offset = table + stream;
        let object = read_u32(cat, offset)? as usize;
        offset += 4;
        let (mut links, mut list) = (Vec::new(), Vec::new());
        while offset + 4 <= list_end {
            let value = read_u32(cat, offset)?;
            offset += 4;
            if value != u32::MAX {
                list.push(value as usize / 32);
            } else if let Some((&source, rest)) = list.split_first() {
                links.push((source, rest.to_vec()));
                list.clear();
            } else {
                break;
            }
        }
        welds.push(JointWeld { object, links });
    }

    Some(JointSkin {
        node,
        parent,
        object: (flags & 0xFF) as usize,
        blend: JointBlend { rest_rotation, own_point, parent_point },
        vertices,
        welds,
    })
}

/// Every object named in the CAT's group lists, including low detail versions.
pub fn grouped_objects(cat: &[u8]) -> Vec<usize> {
    let Some(start) = read_u32(cat, 0x1C).map(|offset| 0x14 + offset as usize) else { return Vec::new() };
    let Some(list) = cat.get(start..) else { return Vec::new() };
    let end = list.iter().position(|&b| b == 0xFF).unwrap_or(list.len());
    list[..end].iter().filter(|&&b| b < 0xE0).map(|&b| b as usize).collect()
}

pub fn parse_node_objects(cat: &[u8]) -> Result<NodeObjects> {
    // Group lists: a marker byte >= 0xF0, its object IDs, and 0xFF at the end.
    let list_offset = 0x14 + read_u32(cat, 0x1C).context("truncated CAT")? as usize;
    let list = cat.get(list_offset..).context("truncated CAT")?;
    let end = list.iter().position(|&b| b == 0xFF).context("unterminated CAT group list")?;
    if list.first() != Some(&0xFE) {
        bail!("CAT has no rigid object list");
    }

    let mut rigid = Vec::new();
    let mut marker = 0;
    let mut position = 0;
    for &byte in &list[..end] {
        if byte >= 0xF0 {
            marker = byte;
            position = 0;
            continue;
        }
        let object = byte as usize;
        // 0xFE: one object per skeleton node. 0xFD: the eyes. 0xFB: forearm ends.
        let node = match marker {
            0xFE => Some(position),
            0xFD => NODE_EYES.get(position).copied(),
            0xFB => NODE_TWIST.get(position).copied(),
            _ => None,
        };
        if let Some(node) = node {
            rigid.push((node, object));
        }
        position += 1;
    }

    let mut skins = Vec::new();
    if let Some(table) = find_node_table(cat) {
        let count = read_u32(cat, table).context("truncated CAT")? as usize / 4 - 1;
        let entries: Vec<usize> = (0..count)
            .map(|i| read_u32(cat, table + i * 4).map(|e| e as usize))
            .collect::<Option<_>>()
            .context("truncated CAT")?;
        for (i, &entry) in entries.iter().enumerate() {
            let end = entries.get(i + 1).copied().unwrap_or(cat.len().saturating_sub(table));
            skins.extend(parse_joint_skin(cat, table, entry, end));
        }
    }
    Ok(NodeObjects { rigid, skins })
}

/// Reads the joint pieces of a node table whose entries may be 1, meaning no
/// piece, as in DOA2U (`sub_000639D0`). The table ends at a zero entry.
pub fn parse_sparse_joint_skins(cat: &[u8], table: usize) -> Vec<JointSkin> {
    let mut entries = Vec::new();
    for i in 0.. {
        match read_u32(cat, table + i * 4) {
            None | Some(0) => break,
            Some(1) => {}
            Some(entry) => entries.push(entry as usize),
        }
    }
    let file_end = cat.len().saturating_sub(table);
    entries
        .iter()
        .enumerate()
        .filter_map(|(i, &entry)| parse_joint_skin(cat, table, entry, entries.get(i + 1).copied().unwrap_or(file_end)))
        .collect()
}

// ─── Chest spring ─────────────────────────────────────────────────────────────

/// Vertices that share a position, with their offsets for each spring direction.
#[derive(Debug, Clone)]
pub struct ChestMorphGroup {
    pub vertices: Vec<u16>,
    pub base: Vec3,
    pub positive: Vec3,
    pub negative: Vec3,
}

/// Reads the chest deformation from the CAT's `LNK` section. Returns `None`
/// when the model has none. Verified against the live game: error 0.00000.
pub fn parse_chest_morph(cat: &[u8]) -> Option<Vec<ChestMorphGroup>> {
    const SCALE: f32 = 1.0 / 32767.0;
    let link = cat.windows(4).position(|w| w == b"LNK\0")?;
    let table = link + read_u32(cat, link + 0x10)? as usize;
    let section_offset = read_u32(cat, table + 0x10)?;
    if section_offset == u32::MAX {
        return None;
    }
    let section = table + section_offset as usize;
    let at = |field: usize| Some(section + read_u32(cat, section + field)? as usize);
    let (mut list, mut base, mut positive, mut negative) = (at(4)?, at(8)?, at(0xC)?, at(0x10)?);

    let vector = |offset: usize| -> Option<Vec3> {
        let c = |i: usize| read_u16(cat, offset + i * 2).map(|v| v as i16 as f32 * SCALE);
        Some(Vec3::new(c(0)?, c(1)?, c(2)?))
    };
    let mut groups = Vec::new();
    loop {
        let count = read_u16(cat, list)? as usize;
        list += 2;
        if count == 0 {
            return Some(groups);
        }
        let vertices = (0..count).map(|i| read_u16(cat, list + i * 2)).collect::<Option<Vec<_>>>()?;
        list += count * 2;
        groups.push(ChestMorphGroup {
            vertices,
            base: vector(base)?,
            positive: vector(positive)?,
            negative: vector(negative)?,
        });
        base += 6;
        positive += 6;
        negative += 6;
    }
}

/// A game's parameters for the chest spring.
#[derive(Debug, Clone, Copy)]
pub struct ChestSpringParameters {
    pub stiffness: f32,
    pub damping: f32,
    pub drive: f32,
    pub rest: f32,
}

/// One-dimensional spring driven by the chest bone (`sub_00046030`,
/// `sub_00046340`, `sub_00046170`). Advance it once per 60 Hz game frame.
/// Equation verified against the game's own state: residual below 1e-9.
#[derive(Debug, Clone, Copy)]
pub struct ChestSpring {
    pub value: f32,
    velocity: f32,
    previous_point: Vec3,
    previous_local_y: f32,
    parameters: ChestSpringParameters,
}

impl ChestSpring {
    /// Drive is 20 in the game's parameters, times the 0.5 it applies to standing states.
    pub const DOA3: ChestSpringParameters =
        ChestSpringParameters { stiffness: 0.128, damping: 0.85, drive: 10.0, rest: 0.125 };
    const REFERENCE: Vec3 = Vec3::new(0.0, 0.2, 0.3);

    fn target(&self, chest: &Mat4) -> f32 {
        self.parameters.rest - self.parameters.rest * chest.y_axis.y
    }

    pub fn new(chest: &Mat4) -> Self {
        Self::with_parameters(chest, Self::DOA3)
    }

    pub fn with_parameters(chest: &Mat4, parameters: ChestSpringParameters) -> Self {
        let mut spring = Self {
            value: 0.0,
            velocity: 0.0,
            previous_point: chest.transform_point3(Self::REFERENCE),
            previous_local_y: 0.0,
            parameters,
        };
        spring.value = spring.target(chest);
        spring
    }

    pub fn step(&mut self, chest: &Mat4) -> f32 {
        let local = chest.inverse().transform_point3(self.previous_point);
        let local_y = Self::REFERENCE.y - local.y;
        let drive = local_y - self.previous_local_y;
        self.previous_local_y = local_y;
        self.previous_point = chest.transform_point3(Self::REFERENCE);

        self.velocity = self.parameters.damping * self.velocity
            - self.parameters.stiffness * (self.value - self.target(chest))
            - self.parameters.drive * drive;
        self.value += self.velocity;
        if self.value >= 1.0 {
            self.value = 1.0;
            self.velocity = self.velocity.min(0.0);
        } else if self.value <= -1.0 {
            self.value = -1.0;
            self.velocity = self.velocity.max(0.0);
        }
        self.value
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Prints every character ID's files and menu costumes, and the kinds of entries in the
    /// archive that are not costume models.
    #[test]
    #[ignore]
    fn survey_roster_and_gaps() {
        let archive = crate::formats::AfsArchive::from_file("assets/doa3/loadfile.afs").unwrap();
        let xbe = Xbe::from_bytes(std::fs::read("assets/doa3/default.xbe").unwrap()).unwrap();
        let table = xbe.offset(MOTION_TABLE_A_VA).unwrap();
        for (id, name) in CHARACTER_NAMES.iter().enumerate() {
            let bin = read_u16(&xbe.data, table + id * 4);
            println!("id {id} {name}: bin {bin:?}, menu {:?}", xbe.selectable_costumes(id));
        }
        let kind = |i: usize| -> String {
            let Some(d) = archive.get_by_index(i) else { return "none".into() };
            let head = d.get(..4).unwrap_or(&[]);
            let text = if head.iter().all(|b| b.is_ascii_graphic()) { String::from_utf8_lossy(head).to_string() } else { format!("{:02x}{:02x}", head.first().unwrap_or(&0), head.get(1).unwrap_or(&0)) };
            let mdl = d.get(0x14..0x18) == Some(b"MDL\0");
            format!("{text}{}:{}", if mdl { "+MDL" } else { "" }, d.len() / 1024)
        };
        println!("first entries {:?}", (0..120).step_by(6).map(kind).collect::<Vec<_>>());
        println!("Kasumi run {:?}", (292..352).map(|i| format!("{i}={}", kind(i))).collect::<Vec<_>>());
        println!("after Bayman {:?}", (742..770).map(|i| format!("{i}={}", kind(i))).collect::<Vec<_>>());
    }

    /// Lists what `loadfile.afs` holds around the costumes: every model, with its owner and
    /// whether the viewer offers it.
    #[test]
    #[ignore]
    fn survey_models_in_loadfile() {
        let archive = crate::formats::AfsArchive::from_file("assets/doa3/loadfile.afs").unwrap();
        let xbe = Xbe::from_bytes(std::fs::read("assets/doa3/default.xbe").unwrap()).unwrap();
        let roster = xbe.roster().unwrap();
        let mut starts: Vec<(usize, &str)> = roster.iter().map(|r| (r.bin_entry, r.name)).collect();
        starts.sort_unstable();
        println!("entries {}, first starts {:?}", archive.entries().len(), &starts[..3.min(starts.len())]);
        for i in 0..archive.entries().len() {
            let Some(data) = archive.get_by_index(i) else { continue };
            if data.get(..4) != Some(b"XPR0") || data.get(0x14..0x18) != Some(b"MDL\0") {
                continue;
            }
            let owner = starts.iter().rev().find(|(start, _)| *start <= i).map_or("before all", |(_, name)| name);
            let next = archive.get_by_index(i + 1).unwrap_or(&[]);
            let nodes = parse_node_objects(next);
            let pieces = crate::formats::model::parse_xpr_model(data);
            let vertices: usize = pieces.as_ref().map_or(0, |p| p.iter().map(|x| x.vertices.len()).sum());
            println!(
                "{i} {owner} size {} next {:?} next_size {} nodes {} objects {} vertices {vertices}",
                data.len() / 1024,
                next.get(..4).map(|b| String::from_utf8_lossy(b).to_string()),
                next.len(),
                nodes.as_ref().map_or_else(|e| format!("no: {e}"), |n| format!("rigid {} skins {}", n.rigid.len(), n.skins.len())),
                pieces.as_ref().map_or(0, |p| p.len()),
            );
        }
    }


    fn record(head: u16, low: u16) -> Vec<u8> {
        [head.to_le_bytes(), low.to_le_bytes()].concat()
    }

    #[test]
    fn linear_track_interpolates_then_holds() {
        // Type 1, 10 frames, from 1000 to 2000 (0.1 to 0.2).
        let data = [record((10 << 2) | 1, 1000), record(0, 2000)].concat();
        assert!((eval_track(&data, 0, 0.0).unwrap() - 0.1).abs() < 1e-6);
        assert!((eval_track(&data, 0, 5.0).unwrap() - 0.15).abs() < 1e-6);
        assert!((eval_track(&data, 0, 99.0).unwrap() - 0.2).abs() < 1e-6);
    }

    #[test]
    fn value_uses_high_head_bits_as_sign() {
        let data = record(0xF000, 0xFFFF);
        assert!((record_value(&data, 0).unwrap() + 1e-4).abs() < 1e-7);
    }

    #[test]
    fn chest_spring_settles_and_reacts_to_movement() {
        let upright = Mat4::IDENTITY;
        let mut spring = ChestSpring::new(&upright);
        for _ in 0..600 {
            spring.step(&upright);
        }
        assert!(spring.value.abs() < 1e-4, "settles at {}", spring.value);

        // One frame of upward movement pushes the value down, then it recovers.
        let moved = Mat4::from_translation(Vec3::new(0.0, 0.05, 0.0));
        let kicked = spring.step(&moved);
        assert!(kicked < -0.1, "{kicked}");
        for _ in 0..600 {
            spring.step(&moved);
        }
        assert!(spring.value.abs() < 1e-3, "{}", spring.value);
    }

    #[test]
    fn chest_morph_reads_real_tina_data() {
        let root = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/doa3/");
        let Ok(archive) = crate::formats::AfsArchive::from_file(format!("{root}loadfile.afs")) else { return };
        let groups = parse_chest_morph(archive.get_by_index(161).unwrap()).expect("Tina has a chest morph");
        assert_eq!(groups.iter().map(|g| g.vertices.len()).sum::<usize>(), 1405);
        // Jann Lee's model has none.
        assert!(parse_chest_morph(archive.get_by_index(204).unwrap()).is_none());
    }

    #[test]
    fn joint_blend_rotates_a_fraction_and_interpolates_the_origin() {
        let blend = JointBlend {
            rest_rotation: Mat3::IDENTITY,
            own_point: Vec3::new(0.0, 1.0, 0.0),
            parent_point: Vec3::new(0.0, 3.0, 0.0),
        };
        let parent = Mat4::IDENTITY;
        let own = Mat4::from_rotation_z(std::f32::consts::FRAC_PI_2);

        let (rotation, origin) = blend.frame(&own, &parent, 0.5);
        let turned = rotation * Vec3::X;
        let expected = std::f32::consts::FRAC_PI_4;
        assert!((turned - Vec3::new(expected.cos(), expected.sin(), 0.0)).length() < 1e-5, "{turned}");
        // Own point (0, 1, 0) rotates to (-1, 0, 0); midway to the parent's (0, 3, 0).
        assert!((origin - Vec3::new(-0.5, 1.5, 0.0)).length() < 1e-5, "{origin}");

        // Weight 0 follows the node completely.
        let (rotation, origin) = blend.frame(&own, &parent, 0.0);
        assert!((rotation * Vec3::X - Vec3::Y).length() < 1e-5);
        assert!((origin - Vec3::new(-1.0, 0.0, 0.0)).length() < 1e-5);
    }

    #[test]
    fn joint_skin_reads_weights_and_targets() {
        // Table with one record, 10 header vec4s, 2 vertices, then the list.
        let (table, record) = (0x20usize, 8usize);
        let data = record + 16;
        let list = data + (10 + 4) * 16;
        let mut cat = vec![0u8; table + list + 32];
        let put = |cat: &mut Vec<u8>, at: usize, v: u32| cat[at..at + 4].copy_from_slice(&v.to_le_bytes());
        put(&mut cat, table + record, list as u32);
        put(&mut cat, table + record + 4, data as u32);
        put(&mut cat, table + record + 8, 0x0002_0305);
        put(&mut cat, table + data + 10 * 16, 2.5f32.to_bits());
        put(&mut cat, table + data + 12 * 16, 7.5f32.to_bits());
        // Weight 16: vertex 1. Weight 8: vertices 2 and 3.
        for (i, v) in [16, 0x20, u32::MAX, u32::MAX, 8, 0x40, 0x60, u32::MAX].iter().enumerate() {
            put(&mut cat, table + list + i * 4, *v);
        }
        let skin = parse_joint_skin(&cat, table, record, list + 32).unwrap();
        assert_eq!((skin.object, skin.node, skin.parent), (5, 3, 2));
        let found: Vec<_> = skin.vertices.iter().map(|v| (v.vertex, v.weight, v.position.x)).collect();
        assert_eq!(found, vec![(1, 1.0, 2.5), (2, 0.5, 7.5), (3, 0.5, 7.5)]);
    }

    #[test]
    fn limb_reaches_target_and_bends_within_reach() {
        let mut skeleton = [Vec3::ZERO; 17];
        skeleton[6] = Vec3::new(0.1, 0.0, 0.0);
        skeleton[7] = Vec3::new(0.0, -0.4, 0.0);
        skeleton[3] = Vec3::new(0.0, -0.4, 0.0);
        skeleton[12] = Vec3::new(-0.1, 0.0, 0.0);
        skeleton[13] = Vec3::new(0.0, -0.4, 0.0);
        skeleton[9] = Vec3::new(0.0, -0.4, 0.0);
        for i in [4, 5, 10, 11] {
            skeleton[i] = Vec3::new(0.3, 0.0, 0.0);
        }
        let mut tracks = [0f32; 44];
        tracks[1] = 0.8;
        tracks[36..39].copy_from_slice(&[0.1, 0.1, 0.0]);
        tracks[28..31].copy_from_slice(&[-0.1, 0.1, 0.0]);
        let nodes = solve_pose(&tracks, &skeleton);

        // Foot sits at the target, and the knee stays one thigh away from the hip.
        let foot = nodes[3].transform_point3(Vec3::ZERO);
        assert!((foot - Vec3::new(0.1, 0.1, 0.0)).length() < 1e-4, "{foot}");
        let hip = nodes[6].transform_point3(Vec3::ZERO);
        let knee = nodes[7].transform_point3(Vec3::ZERO);
        assert!((hip.distance(knee) - 0.4).abs() < 1e-4);
        assert!((knee.distance(foot) - 0.4).abs() < 1e-4, "shin {}", knee.distance(foot));
    }

    /// Checks the whole pipeline against the real game files, when installed:
    /// every character must load, and standing motions must keep feet near the floor.
    #[test]
    fn real_characters_stand_on_the_floor() {
        let root = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/doa3/");
        let Ok(xbe) = std::fs::read(format!("{root}default.xbe")) else { return };
        let Ok(archive) = crate::formats::AfsArchive::from_file(format!("{root}loadfile.afs")) else { return };
        let xbe = Xbe::from_bytes(xbe).unwrap();

        for entry in xbe.roster().unwrap() {
            let skeleton = xbe.skeleton(entry.id).unwrap();
            let data = archive.get_by_index(entry.motion_files[0]).unwrap();
            let motions = list_motions(data);
            assert!(!motions.is_empty(), "{} has no motions", entry.name);

            let mut lowest: Vec<f32> = motions
                .iter()
                .map(|(_, tracks)| {
                    let values = eval_motion(data, tracks, 0.0).unwrap();
                    let nodes = solve_pose(&values, &skeleton);
                    nodes[3].w_axis.y.min(nodes[9].w_axis.y)
                })
                .collect();
            lowest.sort_by(f32::total_cmp);
            let median = lowest[lowest.len() / 2];
            println!("{:14} id {:2}: {:4} motions, median ankle height {median:.3}", entry.name, entry.id, motions.len());
            // Tengu stands higher than everyone else (0.28).
            assert!((0.0..0.3).contains(&median), "{}: ankle at {median}", entry.name);
        }
    }

    #[test]
    fn selectable_costumes_follow_the_game_tables() {
        let Ok(data) = std::fs::read("assets/doa3/default.xbe") else { return };
        let xbe = Xbe::from_bytes(data).unwrap();
        assert_eq!(xbe.selectable_costumes(5).unwrap(), [3, 2, 0, 5, 4, 1, 8, 7, 6], "Kasumi");
        assert_eq!(xbe.selectable_costumes(14).unwrap(), [5, 4, 2], "Hitomi");
        assert_eq!(xbe.selectable_costumes(1).unwrap(), [1, 2, 0], "Tina");
    }
}
