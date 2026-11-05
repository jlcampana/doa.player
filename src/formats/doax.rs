//! Dead or Alive Xtreme Beach Volleyball, read as far as `docs/doax-notes.md` says.
//!
//! A character moves as rigid pieces, without soft bodies, morphs or joint bending. A costume is always a swimsuit on a body: the suit's pieces take the place of the
//! body's torso and hips, and the body is not offered without one.

use anyhow::{bail, Context, Result};
use glam::{Mat3, Mat4, Vec3};
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use super::model::read_u32;

pub const BONE_COUNT: usize = 32;
const BONE_ROWS_VA: u32 = 0x2C_FD58;
const BONE_ROWS_STRIDE: u32 = 0x180;

/// Bone, parent and row of its offset, matched against the running game's matrices. Bone 2,
/// the hips, is the root. Where two rows hold the same value, the row is taken by symmetry.
const BONE_LINKS: [(usize, usize, usize); 22] = [
    (23, 2, 19), (0, 23, 20), (19, 0, 15), (1, 19, 21),
    (16, 0, 12), (8, 16, 4), (4, 8, 0), (5, 4, 1),
    (20, 0, 16), (14, 20, 10), (10, 14, 6), (11, 10, 7),
    (6, 2, 2), (7, 6, 3), (3, 7, 5), (18, 3, 14),
    (12, 2, 8), (13, 12, 9), (9, 13, 5), (22, 9, 18),
    (17, 1, 13), (21, 1, 17),
];

/// Bones that sit on another in the game's matrices, and turn with it.
const BONES_ON_BONES: [(usize, usize); 8] = [(24, 4), (25, 10), (26, 8), (27, 14), (28, 5), (29, 11), (30, 8), (31, 14)];

/// Height in the chest bone's space from which its piece is drawn under a suit.
const SHOULDERS_FROM: f32 = 0.17;

/// Height of the foot bone above the ground in the game's matrices, about 0.1.
const FOOT_HEIGHT: f32 = 0.1;

/// A character the viewer offers, with the file code in her models' names and her table of
/// bone offsets. The table numbers are inferred from her piece sizes, and not confirmed.
/// Kasumi, Ayane and Hitomi are left out: see "Limits" in the notes.
pub const ROSTER: [(&str, &str, u32); 5] = [
    ("Tina Armstrong", "tn", 0),
    ("Helena Douglas", "he", 2),
    ("Lei Fang", "le", 3),
    ("Christie", "cr", 7),
    ("Lisa", "ls", 6),
];

/// Reads files out of an AFS on demand, as the two archives are too big to keep in memory for
/// a game that may not be looked at.
pub struct LazyArchive {
    path: PathBuf,
    entries: Vec<(String, u64, usize)>,
}

impl LazyArchive {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        let mut file = std::fs::File::open(&path)?;
        let mut head = [0u8; 8];
        file.read_exact(&mut head)?;
        if &head[..4] != b"AFS\0" {
            bail!("{} is not an AFS", path.display());
        }
        let count = read_u32(&head, 4)? as usize;
        let mut table = vec![0u8; (count + 1) * 8];
        file.read_exact(&mut table)?;
        let (mut names_at, mut names_size) = (read_u32(&table, count * 8)?, read_u32(&table, count * 8 + 4)?);
        if names_at == 0 {
            let mut pointer = [0u8; 8];
            file.seek(SeekFrom::Start(read_u32(&table, 0)? as u64 - 8))?;
            file.read_exact(&mut pointer)?;
            (names_at, names_size) = (read_u32(&pointer, 0)?, read_u32(&pointer, 4)?);
        }
        let mut names = vec![0u8; names_size as usize];
        file.seek(SeekFrom::Start(names_at as u64))?;
        file.read_exact(&mut names)?;
        let entries = (0..count)
            .map(|i| {
                let name = names.get(i * 48..i * 48 + 32).context("names end early")?;
                let end = name.iter().position(|&b| b == 0).unwrap_or(32);
                Ok((
                    String::from_utf8_lossy(&name[..end]).into_owned(),
                    read_u32(&table, i * 8)? as u64,
                    read_u32(&table, i * 8 + 4)? as usize,
                ))
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(Self { path, entries })
    }

    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.entries.iter().map(|e| e.0.as_str())
    }

    pub fn read(&self, name: &str) -> Result<Vec<u8>> {
        let (_, offset, size) = self.entries.iter().find(|e| e.0 == name).with_context(|| format!("no {name}"))?;
        let mut file = std::fs::File::open(&self.path)?;
        file.seek(SeekFrom::Start(*offset))?;
        let mut data = vec![0u8; *size];
        file.read_exact(&mut data)?;
        Ok(data)
    }
}

/// The suits made for a character, by name: `md`, the owner, a number, then her code. They are
/// sorted so that the list stays the same.
pub fn costumes<'a>(names: impl Iterator<Item = &'a str>, code: &str) -> Vec<String> {
    let mut stems: Vec<String> = names
        .filter_map(|name| name.strip_suffix(".xpr"))
        .filter(|stem| stem.starts_with("md") && stem.len() == 8 && stem.ends_with(code))
        .map(str::to_string)
        .collect();
    stems.sort();
    stems.dedup();
    stems
}

/// A piece's place in a CAT's kind 4 list.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Placed {
    pub object: usize,
    pub bone: usize,
    /// On the torso or hips, which a suit replaces.
    pub replaced_by_suit: bool,
    /// Reaches over the waist. The body's own waist is left out under a suit that has one.
    pub waist: bool,
}

/// The bone of a kind 2 slot, going by each object's size and side. Not from the game's table.
fn weighted_bone(slot: u8) -> Option<usize> {
    Some(match slot {
        0 => 23,
        // On suits, by where their pieces reach over all of the suits.
        0x0A | 0x0D => 0,
        0x0B | 0x0C | 0x0E => 2,
        2 => 8,
        3 => 14,
        6 => 6,
        7 => 12,
        8 => 7,
        9 => 13,
        0x0F => 19,
        _ => return None,
    })
}

/// Reads the pieces of a CAT. Records from +0x14 are (kind, value), and kind 4 is the list: a
/// header up to three bytes of 0xFE, then (kind, slot, object). `0A n` starts group n and `0B` divides it further. Group 0
/// holds a foot in two pieces with its toes, and group 2 a whole foot, drawn here.
pub fn placed_objects(cat: &[u8]) -> Result<Vec<Placed>> {
    let base = read_u32(cat, 4)? as usize;
    let mut record = base;
    let list = loop {
        let (kind, value) = (read_u32(cat, record)?, read_u32(cat, record + 4)? as usize);
        match kind {
            0 => bail!("CAT has no piece list"),
            4 => break base + value,
            _ => record += 8,
        }
    };
    let list = cat.get(list..).context("piece list is outside the file")?;
    let start = list.windows(3).position(|w| w == [0xFE; 3]).context("piece list has no header end")? + 3;
    let mut placed = Vec::new();
    let (mut at, mut group) = (start, None);
    while let Some(&kind) = list.get(at) {
        if kind == 0 {
            break;
        }
        if kind == 0x0A {
            group = list.get(at + 1).copied();
            at += 2;
            continue;
        }
        // Divides the list again, by one byte. What it means is not known.
        if kind == 0x0B {
            at += 1;
            continue;
        }
        let (Some(&slot), Some(&object)) = (list.get(at + 1), list.get(at + 2)) else { break };
        at += 3;
        let object = object as usize;
        if group == Some(0) {
            continue;
        }
        match kind {
            1 | 6 => {
                // Slots from 0x80 are bone 24 and up, as in DOA2U.
                let bone = if slot >= 0x80 { slot as usize - 0x80 + 24 } else { slot as usize };
                if bone < BONE_COUNT {
                    placed.push(Placed { object, bone, replaced_by_suit: kind == 1 && (bone == 0 || bone == 2), waist: false });
                }
            }
            2 => match weighted_bone(slot) {
                Some(bone) => placed.push(Placed { object, bone, replaced_by_suit: false, waist: matches!(slot, 0 | 0x0B | 0x0C) }),
                None => log::warn!("Object {object} uses unknown slot {slot}"),
            },
            _ => {}
        }
    }
    Ok(placed)
}

/// Keeps the triangles of a body's chest piece that lie wholly above `SHOULDERS_FROM` or wholly
/// on the back. Faces look towards +z. A suit's
/// top is only the top: the shoulders and upper chest around it are this piece's. The game was
/// not seen to cut it, this is so that only those parts of the body show around the suit.
pub fn keep_shoulders(piece: &mut super::model::ModelPiece) {
    for material in &mut piece.materials {
        let kept: Vec<u32> = material
            .indices
            .chunks_exact(3)
            .filter(|t| {
                t.iter().all(|&v| piece.vertices[v as usize].position[1] >= SHOULDERS_FROM)
                    || t.iter().all(|&v| piece.vertices[v as usize].position[2] <= 0.0)
            })
            .flatten()
            .copied()
            .collect();
        material.indices = kept;
    }
}

/// A body's joint pieces. The records read as DOA2U's, from the table at CAT word 4, except that
/// a hand's parent is the one its record names: the DOA3 parser's special case for hands
/// points at nodes that are other bones here.
pub fn joint_skins(cat: &[u8]) -> Result<Vec<super::doa3::JointSkin>> {
    let table = read_u32(cat, 0x10)? as usize;
    let mut skins = super::doa3::parse_sparse_joint_skins(cat, table);
    for skin in &mut skins {
        let mut i = 0;
        while let Ok(entry) = read_u32(cat, table + i * 4) {
            i += 1;
            if entry == 0 {
                break;
            }
            let Ok(flags) = read_u32(cat, table + entry as usize + 8) else { continue };
            if entry != 1 && (flags & 0xFF) as usize == skin.object {
                skin.parent = ((flags >> 16) & 0xFF) as usize;
            }
        }
    }
    Ok(skins)
}

/// A thigh's bend towards the hips: its parent and its offset in rest, by its bone. Weights come
/// from the object's extra float. The other weighted pieces hold 2 or 3 floats, not worked out.
pub fn bend_of_bone(bone: usize, rows: &[Vec3; 24]) -> Option<(usize, Vec3)> {
    match bone {
        6 => Some((2, rows[2])),
        12 => Some((2, rows[8])),
        _ => None,
    }
}

/// The rest pose's matrices from a character's offsets, arms out, standing on the ground.
pub fn rest_pose(rows: &[Vec3; 24]) -> [Mat4; BONE_COUNT] {
    let mut positions = [Vec3::ZERO; BONE_COUNT];
    positions[2] = Vec3::Y * (FOOT_HEIGHT - rows[2].y - rows[3].y - rows[5].y);
    for (bone, parent, row) in BONE_LINKS {
        positions[bone] = positions[parent] + rows[row];
    }
    for (bone, base) in BONES_ON_BONES {
        positions[bone] = positions[base];
    }
    positions[15] = Vec3::new(positions[2].x, 0.0, positions[2].z);
    positions.map(|position| Mat4::from_scale(Vec3::new(1.0, 1.0, -1.0)) * Mat4::from_translation(position))
}

pub const TRACK_COUNT: usize = 60;

/// A character's `.mot` files in `datadvd.afs`, by her code. Going by their names: `gokei` is
/// taken to be Christie, as nothing else fits her.
pub fn motion_files(code: &str) -> Vec<String> {
    let stem = match code {
        "tn" => "tina",
        "he" => "hika",
        "le" => "leifang",
        "cr" => "gokei",
        "ls" => "lisa",
        _ => return Vec::new(),
    };
    [stem.to_string(), format!("m_{stem}"), format!("m_{stem}2"), format!("m_{stem}3"), "comon".to_string(), "m_comon".to_string(), "m_comon2".to_string(), "m_comon3".to_string()]
        .map(|name| format!("{name}.mot"))
        .to_vec()
}

/// A motion in a `.mot`: where its block is and how long it runs.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Motion {
    pub id: usize,
    pub block: usize,
    pub frames: u32,
}

/// Lists a `.mot`'s motions: 1000 words of block offsets, the first the unused placeholder. A
/// block is a word, frames in its low half and the track count in its high half, then the
/// tracks' offsets from the block.
pub fn list_motions(data: &[u8]) -> Vec<Motion> {
    let Ok(placeholder) = read_u32(data, 0) else { return Vec::new() };
    (0..1000)
        .filter_map(|id| {
            let block = read_u32(data, id * 4).ok()?;
            if block == placeholder {
                return None;
            }
            let head = read_u32(data, block as usize).ok()?;
            (head >> 16 == TRACK_COUNT as u32).then_some(Motion { id, block: block as usize, frames: head & 0xFFFF })
        })
        .collect()
}

/// Builds a float from a packed value's sign, exponent and mantissa. Zero stays zero.
fn unpack(raw: u32, sign_bit: u32, exponent: u32, mantissa: u32) -> f32 {
    if raw == 0 {
        return 0.0;
    }
    f32::from_bits((((raw >> sign_bit) & 1) << 31) | (exponent << 23) | mantissa)
}

/// A key's value: 20 bits, the head's top four and 16 more (`0x000AE980`).
fn key_value(data: &[u8], at: usize) -> Option<f32> {
    let head = u16::from_le_bytes([*data.get(at)?, *data.get(at + 1)?]) as u32;
    let low = u16::from_le_bytes([*data.get(at + 2)?, *data.get(at + 3)?]) as u32;
    let raw = ((head & 0xF000) << 4) | low;
    // With no exponent, the low bits are a fraction (`0x000AE9B5`, `0x000AE9D3`).
    if (raw >> 16) & 7 == 0 {
        let value = (raw & 0xFFFF) as f32 / 65536.0;
        return Some(if raw >> 19 & 1 != 0 { -value } else { value });
    }
    Some(unpack(raw, 19, ((raw >> 16) & 7) + 0x7E, (raw & 0xFFFF) << 7))
}

/// A 24 bit slope (`0x000AE9F0`).
fn key_slope(raw: u32) -> f32 {
    if (raw >> 19) & 0xF == 0 {
        // A smaller range (`0x000AEA24`), which holds zero as well.
        if raw & 0x7FFFF == 0 {
            return 0.0;
        }
        return unpack(raw, 23, ((raw >> 16) & 7) + 0x69, (raw & 0xFFFF) << 7);
    }
    unpack(raw, 23, ((raw >> 19) & 0xF) + 0x70, (raw & 0x7FFFF) << 4)
}

/// A 16 bit slope (`0x000AE910`).
fn key_short_slope(raw: u32) -> f32 {
    if (raw >> 12) & 7 == 0 {
        // A smaller range (`0x000AE947`), which holds zero as well.
        if raw & 0xFFF == 0 {
            return 0.0;
        }
        return unpack(raw, 15, ((raw >> 10) & 3) + 0x6E, (raw & 0x3FF) << 13);
    }
    unpack(raw, 15, ((raw >> 12) & 7) + 0x71, (raw & 0xFFF) << 11)
}

/// Evaluates a track at a frame. Keys start with a head: low two bits the kind, then ten of
/// duration, with the table at 0x2D1248 giving each kind's function and size. Kind 0 is a
/// cubic with two slopes, 1 a line, 2 a parabola and 3 a constant. A head with no kind or
/// duration ends the track. Kinds 0 to 2 end at the next key's value.
fn eval_track(data: &[u8], start: usize, t: f32) -> Option<f32> {
    let u16_at = |at: usize| Some(u16::from_le_bytes([*data.get(at)?, *data.get(at + 1)?]) as u32);
    let (mut at, mut segment_start) = (start, 0f32);
    loop {
        let head = u16_at(at)?;
        let (kind, dt) = (head & 3, ((head >> 2) & 0x3FF) as f32);
        if (kind == 0 && dt == 0.0) || kind == 3 {
            return key_value(data, at);
        }
        let size = [10, 4, 6][kind as usize];
        if t < segment_start + dt {
            let (v0, v1) = (key_value(data, at)?, key_value(data, at + size)?);
            let x = t - segment_start;
            let slope = (v1 - v0) / dt;
            return Some(match kind {
                0 => {
                    let m0 = key_slope(u16_at(at + 4)? | ((*data.get(at + 8)? as u32) << 16));
                    let m1 = key_slope(u16_at(at + 6)? | ((*data.get(at + 9)? as u32) << 16));
                    let c2 = (3.0 * slope - 2.0 * m0 - m1) / dt;
                    let c3 = (m0 + m1 - 2.0 * slope) / (dt * dt);
                    ((c3 * x + c2) * x + m0) * x + v0
                }
                1 => slope * x + v0,
                _ => {
                    let m = key_short_slope(u16_at(at + 4)?);
                    ((-m * x) + (slope + m * dt)) * x + v0
                }
            });
        }
        segment_start += dt;
        at += size;
    }
}

/// Evaluates a motion's tracks. Their order in the file is the order in memory, unless a
/// motion is mirrored, which swaps sides by the table at 0x2D1360.
pub fn eval_motion(data: &[u8], motion: &Motion, frame: f32) -> Result<[f32; TRACK_COUNT]> {
    let mut values = [0f32; TRACK_COUNT];
    for (i, value) in values.iter_mut().enumerate() {
        let offset = read_u32(data, motion.block + 4 + i * 4)? as usize;
        *value = eval_track(data, motion.block + offset, frame).context("motion track out of bounds")?;
    }
    Ok(values)
}

fn euler(values: &[f32]) -> Mat4 {
    Mat4::from_rotation_z(values[2]) * Mat4::from_rotation_y(values[1]) * Mat4::from_rotation_x(values[0])
}

struct Limb {
    upper: usize,
    mid: usize,
    end: usize,
    parent: usize,
    upper_row: usize,
    mid_row: usize,
    end_row: usize,
    first_track: usize,
    /// The angles' sign, as in DOA3's solve.
    negate: bool,
    /// From the frame the solve builds, with z towards the target, to the bone's. Measured.
    axes: [[f32; 3]; 3],
    arm: bool,
}

const LIMBS: [Limb; 4] = [
    Limb { upper: 14, mid: 10, end: 11, parent: 20, upper_row: 10, mid_row: 6, end_row: 7, first_track: 12, negate: true,
        axes: [[0.0, 0.0, -1.0], [0.0, 1.0, 0.0], [1.0, 0.0, 0.0]], arm: true },
    Limb { upper: 8, mid: 4, end: 5, parent: 16, upper_row: 4, mid_row: 0, end_row: 1, first_track: 20, negate: false,
        axes: [[0.0, 0.0, 1.0], [0.0, 1.0, 0.0], [-1.0, 0.0, 0.0]], arm: true },
    Limb { upper: 12, mid: 13, end: 9, parent: 2, upper_row: 8, mid_row: 9, end_row: 5, first_track: 28, negate: true,
        axes: [[0.0, 1.0, 0.0], [0.0, 0.0, -1.0], [-1.0, 0.0, 0.0]], arm: false },
    Limb { upper: 6, mid: 7, end: 3, parent: 2, upper_row: 2, mid_row: 3, end_row: 5, first_track: 36, negate: false,
        axes: [[0.0, -1.0, 0.0], [0.0, 0.0, -1.0], [1.0, 0.0, 0.0]], arm: false },
];

/// A frame's bones in the game's space, under the character's root. Formulas measured on the
/// running game, see the notes. The hips' x and z are left out, as the root holds them.
fn solve_game_space(tracks: &[f32; TRACK_COUNT], rows: &[Vec3; 24]) -> [Mat4; BONE_COUNT] {
    let at = |row: usize| Mat4::from_translation(rows[row]);
    let mut bones = [Mat4::IDENTITY; BONE_COUNT];
    bones[2] = Mat4::from_translation(Vec3::Y * tracks[1]) * euler(&tracks[3..6]);
    bones[23] = bones[2] * at(19) * euler(&tracks[44..47]);
    bones[0] = bones[23] * at(20) * euler(&tracks[6..9]);
    bones[19] = bones[0] * at(15) * euler(&tracks[47..50]);
    bones[1] = bones[19] * at(21) * euler(&tracks[9..12]);
    bones[17] = bones[1] * at(13);
    bones[21] = bones[1] * at(17);
    bones[16] = bones[0] * at(12) * Mat4::from_rotation_y(tracks[53]) * Mat4::from_rotation_z(tracks[52]);
    bones[20] = bones[0] * at(16) * Mat4::from_rotation_y(tracks[51]) * Mat4::from_rotation_z(tracks[50]);

    for limb in &LIMBS {
        let values = &tracks[limb.first_track..limb.first_track + 8];
        let target = Vec3::new(values[0], values[1], values[2]);
        let (pole_a, pole_b) = (values[6], values[7]);
        let upper_length = rows[limb.mid_row].length();
        let lower_length = rows[limb.end_row].length();
        let reach = upper_length + lower_length;
        let origin = bones[limb.parent].transform_point3(rows[limb.upper_row]);
        let to_target = target - origin;
        let distance = to_target.length().max(1e-6);
        let z = to_target / distance;
        let (end_position, upper_angle, lower_angle) = if distance >= reach {
            (origin + z * reach, 0.0, 0.0)
        } else {
            let cosine = |near: f32, far: f32| {
                ((distance * distance + near * near - far * far) / (2.0 * near * distance)).clamp(-1.0, 1.0).acos()
            };
            (target, cosine(upper_length, lower_length), cosine(lower_length, upper_length))
        };
        let signed = if limb.negate { -upper_angle } else { upper_angle };
        let pole = Vec3::new(pole_b.sin() * pole_a.cos(), -pole_a.sin(), pole_b.cos() * pole_a.cos());
        let y = (pole - z * pole.dot(z)).normalize_or_zero();
        let x = y.cross(z);
        let axes = Mat3::from_cols_array_2d(&limb.axes);
        let upper = Mat4::from_translation(origin)
            * Mat4::from_mat3(Mat3::from_cols(x, y, z))
            * Mat4::from_rotation_y(signed)
            * Mat4::from_mat3(axes);
        bones[limb.upper] = upper;
        bones[limb.end] = Mat4::from_translation(end_position) * euler(&values[3..6]);
        let bend = upper_angle + lower_angle;
        bones[limb.mid] = if limb.arm {
            let turned = upper * at(limb.mid_row) * Mat4::from_rotation_y(if limb.negate { bend } else { -bend });
            // The forearm takes about half of the hand's roll, within 0.08 radians as measured.
            let hand = turned.inverse().transform_vector3(bones[limb.end].z_axis.truncate());
            turned * Mat4::from_rotation_x(0.5 * (-hand.y).atan2(hand.z))
        } else {
            upper * at(limb.mid_row) * Mat4::from_rotation_x(bend)
        };
    }

    bones[18] = bones[3] * at(14) * Mat4::from_rotation_y(tracks[57]) * Mat4::from_rotation_x(tracks[56]);
    bones[22] = bones[9] * at(18) * Mat4::from_rotation_y(tracks[59]) * Mat4::from_rotation_x(tracks[58]);
    // Bones 26, 27, 30 and 31 roll along the upper arm by an amount that was not worked out.
    for (bone, base) in BONES_ON_BONES {
        bones[bone] = bones[base];
    }
    bones
}

/// A frame's bones for the renderer, which works with the game's z flipped.
pub fn solve_pose(tracks: &[f32; TRACK_COUNT], rows: &[Vec3; 24]) -> [Mat4; BONE_COUNT] {
    solve_game_space(tracks, rows).map(|bone| Mat4::from_scale(Vec3::new(1.0, 1.0, -1.0)) * bone)
}

/// Reads a character's offsets from the executable.
pub fn bone_rows(xbe: &[u8], table: u32) -> Result<[Vec3; 24]> {
    let base = read_u32(xbe, 0x104)?;
    let count = read_u32(xbe, 0x11C)? as usize;
    let headers = (read_u32(xbe, 0x120)? - base) as usize;
    let wanted = BONE_ROWS_VA + table * BONE_ROWS_STRIDE;
    let start = (0..count)
        .find_map(|i| {
            let at = headers + i * 56;
            let (va, raw, raw_size) = (read_u32(xbe, at + 4).ok()?, read_u32(xbe, at + 12).ok()?, read_u32(xbe, at + 16).ok()?);
            (va..va + raw_size).contains(&wanted).then(|| (raw + wanted - va) as usize)
        })
        .context("bone offsets are not in the executable")?;
    let mut rows = [Vec3::ZERO; 24];
    for (i, row) in rows.iter_mut().enumerate() {
        // A table starts four rows before `BONE_ROWS_VA + table * stride`, with its rows 20 to 23.
        let at = if i >= 20 { start + i * 16 - BONE_ROWS_STRIDE as usize } else { start + i * 16 };
        let float = |c: usize| read_u32(xbe, at + c * 4).map(f32::from_bits);
        *row = Vec3::new(float(0)?, float(1)?, float(2)?);
    }
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Compares the solve with `tools/re/doax_capture.py`. DOAX_CAPTURE is its file. Bones 26,
    /// 27, 30 and 31 are left out: their roll is not worked out.
    #[test]
    #[ignore]
    fn doax_pose_matches_capture() {
        let text = std::fs::read_to_string(std::env::var("DOAX_CAPTURE").expect("DOAX_CAPTURE")).unwrap();
        let xbe = std::fs::read("assets/doax/default.xbe").unwrap();
        const CHECKED: [usize; 20] = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 16, 19, 20, 23, 17];
        let tables: Vec<[Vec3; 24]> = (0..8).map(|t| bone_rows(&xbe, t).unwrap()).collect();
        let mut worst = [0f32; BONE_COUNT];
        let mut chosen = std::collections::BTreeMap::<(String, usize), usize>::new();
        for line in text.lines() {
            let mut words = line.split_whitespace();
            let character = words.nth(1).unwrap().to_string();
            let floats: Vec<f32> = words.skip(1).map(|w| w.parse().unwrap()).collect();
            let tracks: [f32; TRACK_COUNT] = floats[..TRACK_COUNT].try_into().unwrap();
            let game: Vec<Mat4> = floats[TRACK_COUNT..].chunks_exact(16).map(Mat4::from_cols_slice).collect();
            let errors = |rows: &[Vec3; 24]| -> [f32; BONE_COUNT] {
                let solved = solve_game_space(&tracks, rows);
                std::array::from_fn(|bone| {
                    (game[bone] - solved[bone]).to_cols_array().iter().fold(0f32, |a, b| a.max(b.abs()))
                })
            };
            let total = |rows: &[Vec3; 24]| { let e = errors(rows); CHECKED.iter().map(|&b| e[b]).sum::<f32>() };
            let best = (0..8).min_by(|&a, &b| total(&tables[a]).total_cmp(&total(&tables[b]))).unwrap();
            *chosen.entry((character, best)).or_default() += 1;
            for (slot, error) in worst.iter_mut().zip(errors(&tables[best])) {
                *slot = slot.max(error);
            }
        }
        println!("tables chosen: {chosen:?}");
        for (bone, error) in worst.iter().enumerate() {
            println!("bone {bone}: worst {error:.4}");
        }
    }

    /// Finds which motion and frame each capture line was playing, by its tracks, and prints how
    /// far each track is from it. DOAX_CAPTURE is the file of `tools/re/doax_capture.py`.
    #[test]
    #[ignore]
    fn doax_tracks_match_capture() {
        let text = std::fs::read_to_string(std::env::var("DOAX_CAPTURE").expect("DOAX_CAPTURE")).unwrap();
        let archive = LazyArchive::open("assets/doax/datadvd.afs").unwrap();
        let files: Vec<(String, Vec<u8>)> = archive
            .names()
            .filter(|n| n.ends_with(".mot"))
            .map(|n| (n.to_string(), archive.read(n).unwrap()))
            .collect();
        let distance = |track: usize, a: f32, b: f32| {
            let d = (a - b).abs();
            let _ = track;
            let wrapped = d % std::f32::consts::TAU;
            d.min(wrapped).min(std::f32::consts::TAU - wrapped)
        };
        for line in text.lines().take(8) {
            let words: Vec<&str> = line.split_whitespace().collect();
            let live: Vec<f32> = words[3..3 + TRACK_COUNT].iter().map(|w| w.parse().unwrap()).collect();
            let mut best = (f32::MAX, String::new(), 0, 0, [0f32; TRACK_COUNT]);
            for (name, data) in &files {
                for motion in list_motions(data) {
                    for frame in 0..motion.frames {
                        let Ok(values) = eval_motion(data, &motion, frame as f32) else { continue };
                        // Fewer close tracks is worse: the game adjusts some, such as the head.
                        let e = (3..TRACK_COUNT).filter(|&t| distance(t, values[t], live[t]) > 0.003).count() as f32;
                        if e < best.0 {
                            best = (e, name.clone(), motion.id, frame, values);
                        }
                    }
                }
            }
            let worst: Vec<(usize, f32)> = {
                let mut all: Vec<(usize, f32)> = (0..TRACK_COUNT).map(|t| (t, distance(t, best.4[t], live[t]))).collect();
                all.sort_by(|a, b| b.1.total_cmp(&a.1));
                all.into_iter().take(6).map(|(t, e)| (t, (e * 1000.0).round() / 1000.0)).collect()
            };
            println!("char {} tracks off {} of 57: {} id {} frame {} worst {:?}", words[1], best.0, best.1, best.2, best.3, worst);
        }
    }

    #[test]
    fn packed_values_unpack_as_the_game_does() {
        assert_eq!(unpack(0, 19, 0x7E, 0), 0.0);
        // Head 0x0058 and low 0xE7DF, no exponent: a fraction, the hips' height in the game.
        let value = key_value(&[0x58, 0x00, 0xDF, 0xE7], 0).unwrap();
        assert!((value - 0.9057).abs() < 1e-3, "{value}");
        // The head's top bit is the sign.
        assert!(key_value(&[0x00, 0x80, 0x00, 0x10], 0).unwrap() < 0.0);
        // Exponent 1: from 1 to 2.
        let value = key_value(&[0x00, 0x10, 0x00, 0x80], 0).unwrap();
        assert_eq!(value, 1.5);
    }

    #[test]
    fn suits_are_found_for_their_wearer_only() {
        let names = ["mdTn00tn.xpr", "mdTn00tn.cat", "mdKs01tn.xpr", "mdTn00ht.xpr", "mdSun1tn.bin", "vtn00.xpr"];
        assert_eq!(costumes(names.into_iter(), "tn"), ["mdKs01tn", "mdTn00tn"]);
    }

    #[test]
    fn pieces_skip_the_header_and_group_zero() {
        // Records from +0x14: kind 4 with value 0x10, then the end. List at 0x24.
        let mut cat = vec![0u8; 0x24];
        cat[4] = 0x14;
        cat[0x14] = 4;
        cat[0x18] = 0x10;
        cat.extend_from_slice(&[1, 0xFE, 0xFE, 0xFE, 1, 0x02, 3, 0x0A, 0, 1, 3, 9, 0x0A, 2, 1, 3, 10, 0x0B, 2, 0, 39, 6, 4, 7, 0]);
        let placed = placed_objects(&cat).unwrap();
        assert_eq!(placed, [
            Placed { object: 3, bone: 2, replaced_by_suit: true, waist: false },
            Placed { object: 10, bone: 3, replaced_by_suit: false, waist: false },
            Placed { object: 39, bone: 23, replaced_by_suit: false, waist: true },
            Placed { object: 7, bone: 4, replaced_by_suit: false, waist: false },
        ]);
    }

    #[test]
    fn rest_pose_stands_on_the_ground() {
        let mut rows = [Vec3::ZERO; 24];
        rows[3] = Vec3::Y * -0.4;
        rows[5] = Vec3::Y * -0.4;
            rows[9] = Vec3::Y * -0.4;
        let pose = rest_pose(&rows);
        assert!((pose[3].w_axis.y - FOOT_HEIGHT).abs() < 1e-5);
        assert!((pose[9].w_axis.y - FOOT_HEIGHT).abs() < 1e-5);
        assert_eq!(pose[15].w_axis.y, 0.0);
    }
}
