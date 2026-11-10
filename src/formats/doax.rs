//! Dead or Alive Xtreme Beach Volleyball, read as far as `docs/doax-notes.md` says.
//!
//! A character moves as rigid pieces, without soft bodies, morphs or joint bending. A costume is always a swimsuit on a body, drawn over it as in the game,
//! and the body is not offered without one.

use anyhow::{bail, Context, Result};
use glam::{Mat3, Mat4, Quat, Vec3};
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use super::model::read_u32;
use crate::renderer::mesh::SceneLights;

pub const BONE_COUNT: usize = 32;
const BONE_ROWS_VA: u32 = 0x2C_FD58;
const BONE_ROWS_STRIDE: u32 = 0x180;
const SLOT_TABLE_VA: u32 = 0x33_F988;

/// Bone, parent and row of its offset, matched against the running game's matrices. Bone 2,
/// the hips, is the root. Where two rows hold the same value, the row is taken by symmetry.
pub const BONE_LINKS: [(usize, usize, usize); 22] = [
    (23, 2, 19), (0, 23, 20), (19, 0, 15), (1, 19, 21),
    (16, 0, 12), (8, 16, 4), (4, 8, 0), (5, 4, 1),
    (20, 0, 16), (14, 20, 10), (10, 14, 6), (11, 10, 7),
    (6, 2, 2), (7, 6, 3), (3, 7, 5), (18, 3, 14),
    (12, 2, 8), (13, 12, 9), (9, 13, 5), (22, 9, 18),
    (17, 1, 13), (21, 1, 17),
];

/// Bones at the origin of another in the game's matrices, at rest with its axes.
pub const BONES_ON_BONES: [(usize, usize); 8] = [(24, 4), (25, 10), (26, 8), (27, 14), (28, 5), (29, 11), (30, 8), (31, 14)];

/// Height of the foot bone above the ground in the game's matrices, about 0.1.
const FOOT_HEIGHT: f32 = 0.1;

/// A character the viewer offers, with the file code in her models' names and her table of
/// bone offsets. The table numbers are inferred from her piece sizes, and not confirmed.
pub const ROSTER: [(&str, &str, u32); 8] = [
    ("Kasumi", "ks", 4),
    ("Ayane", "ay", 1),
    ("Hitomi", "ht", 5),
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

    /// Reads an entry by its place, for archives where names repeat.
    pub fn read_index(&self, index: usize) -> Result<Vec<u8>> {
        let (_, offset, size) = self.entries.get(index).with_context(|| format!("no entry {index}"))?;
        let mut file = std::fs::File::open(&self.path)?;
        file.seek(SeekFrom::Start(*offset))?;
        let mut data = vec![0u8; *size];
        file.read_exact(&mut data)?;
        Ok(data)
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

/// The file endings of the suits that fit a character. Bodies of the same shape share them:
/// Helena's chest piece has the size of Lisa's, and Lei Fang's that of the `ht` body. Inferred
/// from those sizes. The game's own table of who can wear what, at 0x3003E8, was not followed.
fn suit_endings(code: &str) -> &'static [&'static str] {
    match code {
        "tn" => &["tn"],
        "cr" => &["cr"],
        "ls" => &["ls"],
        "he" => &["he", "ls"],
        "le" => &["le", "ht"],
        "ay" => &["ay", "ht"],
        "ht" => &["ht"],
        "ks" => &["ks", "ht"],
        _ => &[],
    }
}

/// The suits for a character, by name: `md`, the owner, a number, then the ending. They are
/// sorted so that the list stays the same.
pub fn costumes<'a>(names: impl Iterator<Item = &'a str>, code: &str) -> Vec<String> {
    let endings = suit_endings(code);
    let mut stems: Vec<String> = names
        .filter_map(|name| name.strip_suffix(".xpr"))
        .filter(|stem| stem.starts_with("md") && stem.len() == 8 && endings.contains(&&stem[6..]))
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
    /// A kind 2 slot: the piece blends between bones, by `multi_bend`.
    pub slot: Option<u8>,
}

/// The value of the girl's mode byte (`0x4C9744`) that the viewer draws. 0 has feet in two
/// pieces with toes. What sets it for a suit was not read: mode 2 has a whole foot instead.
const MODE: u8 = 0;

/// Reads the pieces of a CAT, as `0x000633E0` walks them. Records from +0x14 are (kind, value),
/// and kind 4 is the list: a header up to three bytes of 0xFE, then (kind, slot, object).
/// `0A n` draws what follows only in mode n, up to the next `0A` or `0B`.
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
    let (mut at, mut mode) = (start, None);
    while let Some(&kind) = list.get(at) {
        if kind == 0 {
            break;
        }
        if kind == 0x0A {
            mode = list.get(at + 1).copied();
            at += 2;
            continue;
        }
        if kind == 0x0B {
            mode = None;
            at += 1;
            continue;
        }
        let (Some(&slot), Some(&object)) = (list.get(at + 1), list.get(at + 2)) else { break };
        at += 3;
        let object = object as usize;
        if mode.is_some_and(|mode| mode != MODE) {
            continue;
        }
        match kind {
            1 | 6 => {
                // Slots from 0x80 are bone 24 and up, as in DOA2U.
                let bone = if slot >= 0x80 { slot as usize - 0x80 + 24 } else { slot as usize };
                if bone < BONE_COUNT {
                    placed.push(Placed { object, bone, slot: None });
                }
            }
            2 => placed.push(Placed { object, bone: 0, slot: Some(slot) }),
            _ => {}
        }
    }
    Ok(placed)
}

/// A body's joint pieces. The records read as DOA2U's, from the table at CAT word 4, except that
/// a hand's parent is the one its record names: the DOA3 parser's special case for hands
/// points at nodes that are other bones here. Records marked by bit 3 of the top byte go with
/// another mode: the port's docs say mode 2 draws only those on a foot, and others skip them.
pub fn joint_skins(cat: &[u8]) -> Result<Vec<super::doa3::JointSkin>> {
    let table = read_u32(cat, 0x10)? as usize;
    let mut flags_of = std::collections::HashMap::new();
    for i in 0.. {
        let entry = read_u32(cat, table + i * 4)?;
        if entry == 0 {
            break;
        }
        if entry != 1 {
            let flags = read_u32(cat, table + entry as usize + 8)?;
            flags_of.insert((flags & 0xFF) as usize, flags);
        }
    }
    let mut skins = super::doa3::parse_sparse_joint_skins(cat, table);
    skins.retain(|skin| !flags_of.get(&skin.object).is_some_and(|flags| flags >> 24 & 8 != 0));
    for skin in &mut skins {
        if let Some(flags) = flags_of.get(&skin.object) {
            skin.parent = ((flags >> 16) & 0xFF) as usize;
        }
    }
    Ok(skins)
}

const LIGHTS_START_VA: u32 = 0x28_7000;
const LIGHTS_END_VA: u32 = 0x2C_0000;
const LIGHT_RECORD: usize = 0x58;

/// One light record of the executable: +0 kind (1 directional, 2 point), +8 color, +0x18
/// diffuse, +0x24 and +0x28 angles, 65536 per turn, +0x2C position, +0x38 attenuation, +0x44
/// range, +0x54 specular factor. Returns its color and the light as seen from the stage's
/// center. Points are made directional there, as for DOA2U.
fn light_record(record: &[u8]) -> Option<([f32; 3], ([f32; 3], [f32; 3], [f32; 3]))> {
    let word = |at: usize| Some(u32::from_le_bytes(record.get(at..at + 4)?.try_into().ok()?));
    let float = |at: usize| word(at).map(f32::from_bits);
    let triple = |at: usize| Some(Vec3::new(float(at)?, float(at + 4)?, float(at + 8)?));
    let kind = word(0)?;
    let (color, diffuse, factor) = (triple(8)?, triple(0x18)?, float(0x54)?);
    let in_range = |v: Vec3| v.min_element() >= -0.01 && v.max_element() <= 4.0;
    let (a, b) = (word(0x24)? as i32, word(0x28)? as i32);
    if !matches!(kind, 1 | 2) || !in_range(color) || !in_range(diffuse) || !(0.0..=2.0).contains(&factor) || a.abs() > 0x10000 || b.abs() > 0x10000 {
        return None;
    }
    let turn = std::f32::consts::TAU / 65536.0;
    let (a, b) = (a as f32 * turn, b as f32 * turn);
    let mut direction = Vec3::new(-a.cos() * b.sin(), -a.sin(), -a.cos() * b.cos());
    let mut scale = 1.0;
    if kind == 2 {
        let offset = Vec3::Y - triple(0x2C)?;
        let distance = offset.length();
        direction = offset.normalize_or_zero();
        let attenuation = triple(0x38)?;
        let denominator = attenuation.x + attenuation.y * distance + attenuation.z * distance * distance;
        scale = if distance <= float(0x44)? && denominator > 0.0 { 1.0 / denominator } else { 0.0 };
    }
    let diffuse = diffuse * scale;
    Some((color.to_array(), (direction.to_array(), diffuse.to_array(), (diffuse * factor).to_array())))
}

/// A set with the game's usual specular, 0.3 of the diffuse.
fn measured_lights(ambient: [u8; 3], lights: [([f32; 3], [f32; 3]); 4]) -> SceneLights {
    SceneLights {
        ambient: ambient.map(|c| c as f32 / 255.0),
        lights: lights.map(|(direction, diffuse)| (direction, diffuse, diffuse.map(|c| c * 0.3))),
    }
}

/// DOAX's lights for the viewer: the sets most often enabled in a running demo first, then those
/// of the executable. Runs of records there hold the stage's four, then four for characters,
/// which fits the one set matched to a live read. The first record's color is the ambient.
/// Which set belongs to which stage and time of day is not known.
pub fn stage_lights(xbe: &[u8]) -> Vec<(String, SceneLights)> {
    let mut list = vec![
        ("Demo, sun".to_string(), measured_lights([0x60; 3], [
            ([-0.297, -0.942, 0.159], [0.9, 0.88, 0.86]),
            ([0.164, 0.904, -0.395], [0.23, 0.2, 0.18]),
            ([0.765, 0.634, 0.113], [0.2, 0.11, 0.11]),
            ([-0.742, -0.596, -0.307], [0.0, 0.0, 0.0]),
        ])),
        ("Demo, soft".to_string(), measured_lights([0x66, 0x5B, 0x56], [
            ([-0.142, -0.243, 0.959], [0.6, 0.55, 0.52]),
            ([-0.878, 0.098, -0.469], [0.38, 0.38, 0.33]),
            ([0.981, 0.0, 0.195], [0.3, 0.32, 0.34]),
            ([0.634, -0.049, -0.772], [0.29, 0.3, 0.35]),
        ])),
        ("Demo, low sun".to_string(), measured_lights([0x30, 0x38, 0x44], [
            ([-0.215, -0.672, 0.709], [1.29, 1.2, 0.98]),
            ([0.176, -0.428, 0.887], [0.41, 0.55, 0.48]),
            ([0.0, -1.0, 0.0], [0.0, 0.0, 0.0]),
            ([0.0, -1.0, 0.0], [0.0, 0.0, 0.0]),
        ])),
    ];
    let (Some(start), Some(end)) = (xbe_offset(xbe, LIGHTS_START_VA), xbe_offset(xbe, LIGHTS_END_VA - 1)) else { return list };
    let mut at = start;
    let mut number = 0;
    while at + LIGHT_RECORD <= end {
        let mut records = Vec::new();
        while let Some(record) = xbe.get(at + records.len() * LIGHT_RECORD..at + (records.len() + 1) * LIGHT_RECORD).and_then(light_record) {
            records.push(record);
        }
        if records.len() < 8 {
            at += 4;
            continue;
        }
        at += records.len() * LIGHT_RECORD;
        let character = &records[4..8];
        let lights = SceneLights { ambient: character[0].0, lights: [character[0].1, character[1].1, character[2].1, character[3].1] };
        let known = list.iter().any(|(_, other)| other.ambient == lights.ambient && other.lights == lights.lights);
        if !known {
            number += 1;
            list.push((format!("Table set {number}"), lights));
        }
    }
    list
}

/// The file offset of an address in the executable.
fn xbe_offset(xbe: &[u8], wanted: u32) -> Option<usize> {
    let base = read_u32(xbe, 0x104).ok()?;
    let count = read_u32(xbe, 0x11C).ok()? as usize;
    let headers = (read_u32(xbe, 0x120).ok()? - base) as usize;
    (0..count).find_map(|i| {
        let at = headers + i * 56;
        let (va, raw, raw_size) = (read_u32(xbe, at + 4).ok()?, read_u32(xbe, at + 12).ok()?, read_u32(xbe, at + 16).ok()?);
        (va..va + raw_size).contains(&wanted).then(|| (raw + wanted - va) as usize)
    })
}

/// The parents and names of the bones, for exporting.
pub fn skeleton() -> (Vec<Option<usize>>, Vec<String>) {
    let mut parents = vec![None; BONE_COUNT];
    for (bone, parent, _) in BONE_LINKS {
        parents[bone] = Some(parent);
    }
    for (bone, base) in BONES_ON_BONES {
        parents[bone] = Some(base);
    }
    let names = (0..BONE_COUNT)
        .map(|bone| match bone {
            2 => "hips".to_string(),
            23 => "waist".to_string(),
            0 => "chest".to_string(),
            19 => "neck".to_string(),
            1 => "head".to_string(),
            16 | 20 => format!("collar_{bone}"),
            8 | 14 => format!("arm_upper_{bone}"),
            4 | 10 => format!("arm_lower_{bone}"),
            5 | 11 => format!("hand_{bone}"),
            6 | 12 => format!("leg_upper_{bone}"),
            7 | 13 => format!("leg_lower_{bone}"),
            3 | 9 => format!("foot_{bone}"),
            18 | 22 => format!("toes_{bone}"),
            17 | 21 => format!("eye_{bone}"),
            15 => "root".to_string(),
            _ => format!("twist_{bone}"),
        })
        .collect();
    (parents, names)
}

/// A bone's offset from its parent at rest.
fn bone_offset(bone: usize, rows: &[Vec3; 24]) -> Vec3 {
    BONE_LINKS.iter().find(|link| link.0 == bone).map_or(Vec3::ZERO, |link| rows[link.2])
}

/// The groups of a kind 2 slot, from the table at 0x33F988, 16 bytes a slot: a count, then that
/// many lists ended by 0xFF. A list is a bone, then offsets: 0x80 + a bone adds its offset and
/// 0x40 + a bone takes it away.
pub fn slot_groups(xbe: &[u8], slot: u8, rows: &[Vec3; 24]) -> Option<Vec<(usize, Vec3)>> {
    let entry = xbe.get(xbe_offset(xbe, SLOT_TABLE_VA + slot as u32 * 16)?..)?.get(..16)?;
    let mut lists = entry[1..].split(|&b| b == 0xFF);
    (0..entry[0])
        .map(|_| {
            let (&bone, offsets) = lists.next()?.split_first()?;
            let offset = offsets.iter().fold(Vec3::ZERO, |sum, &op| {
                let other = bone_offset((op & 0x3F) as usize, rows);
                if op >= 0x80 { sum + other } else { sum - other }
            });
            Some((bone as usize, offset))
        })
        .collect()
}

/// A kind 2 piece's blend: its slot's groups and each vertex's extra floats, one fewer than
/// the groups.
pub fn multi_bend(xpr: &[u8], placed: &Placed, xbe: &[u8], rows: &[Vec3; 24]) -> Option<crate::renderer::mesh::MultiBend> {
    let mut groups = slot_groups(xbe, placed.slot?, rows)?;
    let record = read_u32(xpr, 0x28 + placed.object * 4).ok()? as usize;
    let extra = read_u32(xpr, record + 4).ok()? as usize;
    let count = read_u32(xpr, record + 0x20).ok()? as usize;
    let vertices = read_u32(xpr, record + 0x24).ok()? as usize;
    // Slot 15, the neck, counts 4 lists but holds 3, and its pieces have weights for 3.
    if extra + 1 > groups.len() {
        log::warn!("Object {} has {extra} weights for {} groups", placed.object, groups.len());
        return None;
    }
    groups.truncate(extra + 1);
    let weights = (0..count)
        .map(|v| {
            (0..extra)
                .map(|w| read_u32(xpr, vertices + v * (32 + extra * 4) + 12 + w * 4).map(f32::from_bits).ok())
                .collect::<Option<Vec<f32>>>()
        })
        .collect::<Option<Vec<_>>>()?;
    Some(crate::renderer::mesh::MultiBend { groups, weights })
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

/// A hand pose change in a motion's script: hand 0, 1 or 2 for both, over `time` frames.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HandEvent {
    pub frame: u16,
    pub hand: u16,
    pub pose: u16,
    pub time: u16,
}

const SCRIPT_OPS_VA: u32 = 0x2C_F888;
const SCRIPT_OP_COUNT: usize = 0x4B;
const POSE_SHAPES_VA: u32 = 0x33_EE38;
const SCRIPT_TABLE_ENTRIES: usize = 1000;
/// The motion of a script in `anime_cm.bin` is this plus its number in the shared `.mot` files.
pub const SHARED_MOTION_FLAG: u16 = 0x2000;

/// The hand events of each motion in a `.bin`. Word 0 gives a table of 1000 script offsets. A
/// script is 16 bit words: an operation, then as many arguments as the table at 0x2CF888
/// says, up to 0xFF. 0 plays a motion, 1 waits for a frame, 12 sets a hand and 13 does so over
/// some frames (`0x000B22F0`).
pub fn hand_events(bin: &[u8], xbe: &[u8]) -> Result<std::collections::HashMap<u16, Vec<HandEvent>>> {
    let ops = xbe_offset(xbe, SCRIPT_OPS_VA).context("no script table")?;
    let mut sizes = std::collections::HashMap::new();
    for i in 0..SCRIPT_OP_COUNT {
        let entry = xbe.get(ops + i * 6..ops + i * 6 + 4).context("short script table")?;
        sizes.insert(u16::from_le_bytes([entry[0], entry[1]]), entry[2] as usize);
    }
    let table = read_u32(bin, 0)? as usize;
    let mut starts: Vec<usize> =
        (0..SCRIPT_TABLE_ENTRIES).filter_map(|i| read_u32(bin, table + i * 4).ok()).map(|v| v as usize).collect();
    starts.sort_unstable();
    starts.dedup();
    let word = |at: usize| Some(u16::from_le_bytes([*bin.get(at)?, *bin.get(at + 1)?]));
    let mut events: std::collections::HashMap<u16, Vec<HandEvent>> = std::collections::HashMap::new();
    for start in starts {
        let (mut at, mut motion, mut frame) = (start, None, 0);
        while let Some(op) = word(at) {
            let Some(&size) = sizes.get(&op) else { break };
            if op == 0xFF {
                break;
            }
            let argument = |i: usize| word(at + 2 + i * 2);
            match op {
                0 => {
                    motion = argument(0);
                    frame = 0;
                    // The first script to play a motion decides its events.
                    if let Some(motion) = motion {
                        events.entry(motion).or_default();
                    }
                }
                1 => frame = argument(0).unwrap_or(frame),
                12 | 13 => {
                    if let (Some(motion), Some(hand), Some(pose)) = (motion, argument(0), argument(1)) {
                        let time = if op == 13 { argument(2).unwrap_or(0) } else { 0 };
                        events.entry(motion).or_default().push(HandEvent { frame, hand, pose, time });
                    }
                }
                _ => {}
            }
            at += 2 + size * 2;
        }
    }
    Ok(events)
}

/// The chest spring's parameters: those at 0x33FB90, 0.128, 0.85, 20 and 0.25, the same as
/// DOA3's, with the half `0x00062040` takes of the rest and of the drive in standing states.
pub const CHEST_SPRING: super::doa3::ChestSpringParameters = super::doa3::ChestSpring::DOA3;

/// How far a chest shape is blended in for a spring value (`0x00064AF0`): (1 - k) |v| + k
/// sqrt |v|, with k 0.3 in most states. Shape 1 for a positive value, 2 for a negative one.
/// Matched to live reads of the value and the weight, within their rounding.
pub fn chest_weight(value: f32) -> (usize, f32) {
    const K: f32 = 0.3;
    let size = value.abs().min(1.0);
    (if value >= 0.0 { 1 } else { 2 }, (1.0 - K) * size + K * size.sqrt())
}

/// The sets of 3 shapes in a CAT's `apv` section that the chest spring drives: on a body the
/// chest and the two upper arms, on a suit its chest piece. Hands, the first two of a body,
/// and faces, with more shapes, are left out.
pub fn chest_sets(cat: &[u8], xpr: &[u8], body: bool) -> Vec<super::doa3_morphs::PoseHand> {
    let first = if body { 2 } else { 0 };
    (first..super::doa2u::morph_set_count(cat))
        .filter_map(|entry| super::doa2u::morph_set(cat, xpr, entry).ok().flatten())
        .filter(|set| set.shapes.len() == 3)
        .collect()
}

/// The shape each hand pose rests on, from the table at 0x33EE38, 8 bytes a pose.
pub fn pose_shapes(xbe: &[u8]) -> Result<Vec<usize>> {
    let table = xbe_offset(xbe, POSE_SHAPES_VA).context("no pose table")?;
    (0..32).map(|i| Ok(read_u32(xbe, table + i * 8)? as usize)).collect()
}

/// Where a hand is at a frame: shapes to blend and how far along. The game also passes through
/// shapes in between on some changes, by the table's second word, which this leaves out.
pub fn hand_blend(events: &[HandEvent], slot: u16, t: f32, shapes: &[usize]) -> (usize, usize, f32) {
    let shape = |pose: u16| shapes.get(pose as usize).copied().unwrap_or(0);
    let (mut from, mut to, mut weight) = (shape(0), shape(0), 1.0);
    for event in events.iter().filter(|e| e.hand == slot || e.hand == 2) {
        if event.frame as f32 > t {
            break;
        }
        from = to;
        to = shape(event.pose);
        weight = if event.time == 0 { 1.0 } else { ((t - event.frame as f32) / event.time as f32).min(1.0) };
    }
    (from, to, weight)
}

/// A character's `.mot` files in `datadvd.afs`, by her code. Going by their names: `gokei` is
/// taken to be Christie, as nothing else fits her.
pub fn motion_files(code: &str) -> Vec<String> {
    let stem = match code {
        "tn" => "tina",
        "ay" => "ayane",
        "ht" => "hitomi",
        "ks" => "kasumi",
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

/// A 16-bit field of a motion: little-endian in DOAX, big-endian in DOAX2.
fn field16(data: &[u8], at: usize, big: bool) -> Option<u32> {
    let bytes = [*data.get(at)?, *data.get(at + 1)?];
    Some(if big { u16::from_be_bytes(bytes) } else { u16::from_le_bytes(bytes) } as u32)
}

/// A key's value: 20 bits, the head's top four and 16 more (`0x000AE980`).
fn key_value(data: &[u8], at: usize, big: bool) -> Option<f32> {
    let head = field16(data, at, big)?;
    let low = field16(data, at + 2, big)?;
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
///
/// In DOAX2 the file is big-endian: each 16-bit field is swapped, and so are the two slope
/// bytes at +8 and +9, as if they were one 16-bit field (measured: slopes of consecutive keys
/// agree 30 times better this way).
fn eval_track(data: &[u8], start: usize, t: f32, angle: bool, big: bool) -> Option<f32> {
    let u16_at = |at: usize| field16(data, at, big);
    let (hi0, hi1) = if big { (9, 8) } else { (8, 9) };
    let (mut at, mut segment_start) = (start, 0f32);
    loop {
        let head = u16_at(at)?;
        let (kind, dt) = (head & 3, ((head >> 2) & 0x3FF) as f32);
        if (kind == 0 && dt == 0.0) || kind == 3 {
            return key_value(data, at, big);
        }
        let size = [10, 4, 6][kind as usize];
        if t < segment_start + dt {
            let (v0, mut v1) = (key_value(data, at, big)?, key_value(data, at + size, big)?);
            // Angles wrap between keys: 0.01 to 6.27 is a small turn back, and going through pi
            // would flip a bone between two frames.
            if angle {
                v1 = v0 + (v1 - v0 + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI;
            }
            let x = t - segment_start;
            // A line of one frame is how an angle changes to an equal form, such as all three
            // of a bone's angles moving by pi. Between two frames it would be a wrong turn.
            if angle && kind == 1 && dt <= 1.0 {
                return Some(v0);
            }
            let slope = (v1 - v0) / dt;
            return Some(match kind {
                0 => {
                    let m0 = key_slope(u16_at(at + 4)? | ((*data.get(at + hi0)? as u32) << 16));
                    let m1 = key_slope(u16_at(at + 6)? | ((*data.get(at + hi1)? as u32) << 16));
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

/// The root's position and the limbs' targets. The rest are angles.
fn is_position_track(track: usize) -> bool {
    track < 3 || LIMBS.iter().any(|limb| (limb.first_track..limb.first_track + 3).contains(&track))
}

/// Evaluates a motion's tracks. Their order in the file is the order in memory, unless a
/// motion is mirrored, which swaps sides by the table at 0x2D1360.
pub fn eval_motion(data: &[u8], motion: &Motion, frame: f32) -> Result<[f32; TRACK_COUNT]> {
    eval_motion_endian(data, motion, frame, false)
}

/// `eval_motion` for either byte order: DOAX2 keeps the same tracks big-endian.
pub fn eval_motion_endian(data: &[u8], motion: &Motion, frame: f32, big: bool) -> Result<[f32; TRACK_COUNT]> {
    let word = |at: usize| -> Result<u32> {
        let bytes: [u8; 4] = data.get(at..at + 4).context("motion out of bounds")?.try_into()?;
        Ok(if big { u32::from_be_bytes(bytes) } else { u32::from_le_bytes(bytes) })
    };
    let mut values = [0f32; TRACK_COUNT];
    for (i, value) in values.iter_mut().enumerate() {
        let offset = word(motion.block + 4 + i * 4)? as usize;
        *value = eval_track(data, motion.block + offset, frame, !is_position_track(i), big).context("motion track out of bounds")?;
    }
    Ok(values)
}

/// `list_motions` for DOAX2: 8000 big-endian words of block offsets.
pub fn list_motions_big(data: &[u8]) -> Vec<Motion> {
    let word = |at: usize| data.get(at..at + 4).map(|b| u32::from_be_bytes(b.try_into().unwrap()));
    let Some(placeholder) = word(0) else { return Vec::new() };
    (0..8000)
        .filter_map(|id| {
            let block = word(id * 4)?;
            if block == placeholder {
                return None;
            }
            let head = word(block as usize)?;
            (head >> 16 == TRACK_COUNT as u32).then_some(Motion { id, block: block as usize, frames: head & 0xFFFF })
        })
        .collect()
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

/// `frame` turned by the shortest arc until its x axis is `onto`'s, at `onto`'s origin
/// (`0x0017C8D0`).
fn aligned_frame(frame: Mat4, onto: Mat4) -> Mat4 {
    let arc = Quat::from_rotation_arc(frame.x_axis.truncate().normalize(), onto.x_axis.truncate().normalize());
    let mut aligned = Mat4::from_quat(arc) * frame;
    aligned.w_axis = onto.w_axis;
    aligned
}

/// Halfway from one rotation to another, at the first's origin (`0x0017C780`).
fn halfway_frame(from: Mat4, to: Mat4) -> Mat4 {
    let (a, b) = (Quat::from_mat4(&from), Quat::from_mat4(&to));
    let mut halfway = Mat4::from_quat(a.slerp(b, 0.5));
    halfway.w_axis = from.w_axis;
    halfway
}

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
            upper * at(limb.mid_row) * Mat4::from_rotation_y(if limb.negate { bend } else { -bend })
        } else {
            upper * at(limb.mid_row) * Mat4::from_rotation_x(bend)
        };
    }

    // Measured within 0.0002. Tracks 56 to 59 were zero in those samples, and their use by
    // `0x000B20A0` was not read.
    bones[18] = bones[3] * at(14) * Mat4::from_rotation_x(tracks[55]);
    bones[22] = bones[9] * at(18) * Mat4::from_rotation_x(tracks[54]);
    // `0x000B17D0`: arm bones share the roll of the bone after them, half each. The aligned
    // bones hold that bone's frame turned to lie along this one, at this one's origin.
    for (upper, collar, aligned, halfway) in [(8, 16, 26, 30), (14, 20, 27, 31)] {
        bones[aligned] = aligned_frame(bones[collar], bones[upper]);
        bones[halfway] = halfway_frame(bones[upper], bones[aligned]);
    }
    for (mid, hand, twist, aligned) in [(4, 5, 24, 28), (10, 11, 25, 29)] {
        bones[aligned] = aligned_frame(bones[hand], bones[mid]);
        bones[mid] = halfway_frame(bones[mid], bones[aligned]);
        bones[twist] = bones[mid];
    }
    bones
}

/// A frame's bones for the renderer, which works with the game's z flipped.
pub fn solve_pose(tracks: &[f32; TRACK_COUNT], rows: &[Vec3; 24]) -> [Mat4; BONE_COUNT] {
    solve_game_space(tracks, rows).map(|bone| Mat4::from_scale(Vec3::new(1.0, 1.0, -1.0)) * bone)
}

/// Reads a character's offsets from the executable.
pub fn bone_rows(xbe: &[u8], table: u32) -> Result<[Vec3; 24]> {
    let start = xbe_offset(xbe, BONE_ROWS_VA + table * BONE_ROWS_STRIDE).context("bone offsets are not in the executable")?;
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

    /// Compares the solve with `tools/re/doax_capture.py`. DOAX_CAPTURE is its file.
    #[test]
    #[ignore]
    fn doax_pose_matches_capture() {
        let text = std::fs::read_to_string(std::env::var("DOAX_CAPTURE").expect("DOAX_CAPTURE")).unwrap();
        let xbe = std::fs::read("assets/doax/default.xbe").unwrap();
        const CHECKED: [usize; 20] = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 16, 19, 20, 23, 17];
        let tables: Vec<[Vec3; 24]> = (0..8).map(|t| bone_rows(&xbe, t).unwrap()).collect();
        let mut skipped = 0;
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
            // In menus and some scenes the matrices do not come from these tracks.
            if errors(&tables[best])[2] > 0.002 {
                skipped += 1;
                continue;
            }
            *chosen.entry((character, best)).or_default() += 1;
            for (slot, error) in worst.iter_mut().zip(errors(&tables[best])) {
                *slot = slot.max(error);
            }
        }
        println!("tables chosen: {chosen:?}, {skipped} samples skipped");
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

    /// Prints the biggest turn of a bone between half frames, over a `.mot`. A flip shows as
    /// close to pi. DOAX_MOT is its name.
    #[test]
    #[ignore]
    fn doax_motions_turn_smoothly() {
        let archive = LazyArchive::open("assets/doax/datadvd.afs").unwrap();
        let xbe = std::fs::read("assets/doax/default.xbe").unwrap();
        let rows = bone_rows(&xbe, 0).unwrap();
        let data = archive.read(&std::env::var("DOAX_MOT").expect("DOAX_MOT")).unwrap();
        let mut found = Vec::new();
        for motion in list_motions(&data) {
            let mut before: Option<[Mat4; BONE_COUNT]> = None;
            for half in 0..=motion.frames * 2 {
                let pose = solve_game_space(&eval_motion(&data, &motion, half as f32 / 2.0).unwrap(), &rows);
                if let Some(old) = before {
                    for bone in [2usize, 23, 0, 19, 1, 16, 20, 3, 9, 5, 11] {
                        let turn = glam::Quat::from_mat4(&(old[bone].inverse() * pose[bone]).normalize_rotation()).angle_between(glam::Quat::IDENTITY);
                        found.push((turn, motion.id, half as f32 / 2.0, motion.frames, bone));
                    }
                }
                before = Some(pose);
            }
        }
        found.sort_by(|a, b| b.0.total_cmp(&a.0));
        println!("{} samples, over 0.5 rad: {}", found.len(), found.iter().filter(|f| f.0 > 0.5).count());
        for f in found.iter().take(8) {
            println!("turn {:.2} motion {} at {} of {} bone {}", f.0, f.1, f.2, f.3, f.4);
        }
    }

    trait NormalizeRotation {
        fn normalize_rotation(self) -> Mat4;
    }
    impl NormalizeRotation for Mat4 {
        fn normalize_rotation(self) -> Mat4 {
            let (_, rotation, _) = self.to_scale_rotation_translation();
            Mat4::from_quat(rotation)
        }
    }

    fn record_bytes(kind: u32, angles: (i32, i32), diffuse: [f32; 3], factor: f32) -> Vec<u8> {
        let mut record = vec![0u8; LIGHT_RECORD];
        record[0..4].copy_from_slice(&kind.to_le_bytes());
        for (i, value) in diffuse.iter().enumerate() {
            record[0x18 + i * 4..0x1C + i * 4].copy_from_slice(&value.to_le_bytes());
        }
        record[0x24..0x28].copy_from_slice(&angles.0.to_le_bytes());
        record[0x28..0x2C].copy_from_slice(&angles.1.to_le_bytes());
        record[0x54..0x58].copy_from_slice(&factor.to_le_bytes());
        record
    }

    #[test]
    fn light_records_read_directions_and_specular() {
        // A quarter turn down, as the formula gives for an angle of 0x4000.
        let (_, (direction, diffuse, specular)) = light_record(&record_bytes(1, (0x4000, 0), [0.5, 0.25, 1.0], 0.3)).unwrap();
        assert!((direction[1] + 1.0).abs() < 1e-5, "{direction:?}");
        assert_eq!(diffuse, [0.5, 0.25, 1.0]);
        assert!((specular[0] - 0.15).abs() < 1e-5);
        assert!(light_record(&record_bytes(0, (0, 0), [0.5; 3], 0.3)).is_none());
        assert!(light_record(&record_bytes(1, (0, 0), [9.0; 3], 0.3)).is_none());
    }

    /// The live set of (0.9, 0.88, 0.88) must come out of the table. Reads the executable.
    #[test]
    #[ignore]
    fn table_holds_a_set_read_from_the_game() {
        let xbe = std::fs::read("assets/doax/default.xbe").unwrap();
        let list = stage_lights(&xbe);
        println!("{} sets", list.len());
        let found = list.iter().find(|(_, set)| set.lights[0].1 == [0.9, 0.88, 0.88]).expect("no such set");
        assert!((found.1.ambient[0] - 0x59 as f32 / 255.0).abs() < 0.01, "{:?}", found.1.ambient);
        assert!((found.1.lights[0].0[0] + 0.528).abs() < 0.002, "{:?}", found.1.lights[0].0);
    }

    /// Lists the chest sets a body and a suit give. Reads the game's files.
    #[test]
    #[ignore]
    fn chest_sets_are_found() {
        let dvd = LazyArchive::open("assets/doax/datadvd.afs").unwrap();
        let hdd = LazyArchive::open("assets/doax/datahdd.afs").unwrap();
        let body = dvd.read("vtn00.cat").unwrap();
        for entry in 0..super::super::doa2u::morph_set_count(&body) {
            let set = super::super::doa2u::morph_set(&body, &dvd.read("vtn00.xpr").unwrap(), entry);
            println!("body set {entry}: {:?}", set.as_ref().map(|s| s.as_ref().map(|s| (s.object, s.shapes.len()))));
        }
        let suit = hdd.read("mdAy00tn.cat").unwrap();
        println!("body chest sets {:?}", chest_sets(&body, &dvd.read("vtn00.xpr").unwrap(), true).iter().map(|s| s.object).collect::<Vec<_>>());
        println!("suit chest sets {:?}", chest_sets(&suit, &hdd.read("mdAy00tn.xpr").unwrap(), false).iter().map(|s| s.object).collect::<Vec<_>>());
    }

    #[test]
    fn chest_weights_match_live_reads() {
        // Values and weights read together from the running game.
        for (value, weight) in [(0.009, 0.034), (0.084, 0.146), (0.645, 0.693), (0.759, 0.793)] {
            assert_eq!(chest_weight(value).0, 1);
            assert!((chest_weight(value).1 - weight).abs() < 0.002, "{value}");
            assert_eq!(chest_weight(-value).0, 2);
        }
    }

    #[test]
    fn hands_blend_between_their_events() {
        let shapes = [0, 1, 4, 5];
        let events = [
            HandEvent { frame: 10, hand: 0, pose: 2, time: 20 },
            HandEvent { frame: 50, hand: 2, pose: 3, time: 0 },
        ];
        assert_eq!(hand_blend(&events, 0, 5.0, &shapes), (0, 0, 1.0));
        assert_eq!(hand_blend(&events, 0, 20.0, &shapes), (0, 4, 0.5));
        assert_eq!(hand_blend(&events, 0, 40.0, &shapes), (0, 4, 1.0));
        assert_eq!(hand_blend(&events, 0, 60.0, &shapes), (4, 5, 1.0));
        // Only the event for both hands reaches the other one.
        assert_eq!(hand_blend(&events, 1, 40.0, &shapes), (0, 0, 1.0));
        assert_eq!(hand_blend(&events, 1, 60.0, &shapes), (0, 5, 1.0));
    }

    #[test]
    fn packed_values_unpack_as_the_game_does() {
        assert_eq!(unpack(0, 19, 0x7E, 0), 0.0);
        // Head 0x0058 and low 0xE7DF, no exponent: a fraction, the hips' height in the game.
        let value = key_value(&[0x58, 0x00, 0xDF, 0xE7], 0, false).unwrap();
        assert!((value - 0.9057).abs() < 1e-3, "{value}");
        // The head's top bit is the sign.
        assert!(key_value(&[0x00, 0x80, 0x00, 0x10], 0, false).unwrap() < 0.0);
        // Exponent 1: from 1 to 2.
        let value = key_value(&[0x00, 0x10, 0x00, 0x80], 0, false).unwrap();
        assert_eq!(value, 1.5);
    }

    #[test]
    fn slot_lists_add_and_take_away_offsets() {
        // A count of 2: bone 23 with bone 0's offset, then bone 2 taking away bone 23's.
        let mut xbe = vec![0u8; 0x400];
        xbe[0x104..0x108].copy_from_slice(&0x10000u32.to_le_bytes());
        xbe[0x11C..0x120].copy_from_slice(&1u32.to_le_bytes());
        xbe[0x120..0x124].copy_from_slice(&0x10200u32.to_le_bytes());
        xbe[0x204..0x208].copy_from_slice(&SLOT_TABLE_VA.to_le_bytes());
        xbe[0x20C..0x210].copy_from_slice(&0x300u32.to_le_bytes());
        xbe[0x210..0x214].copy_from_slice(&0x100u32.to_le_bytes());
        xbe[0x300..0x307].copy_from_slice(&[2, 23, 0x80, 0xFF, 2, 0x40 + 23, 0xFF]);
        let mut rows = [Vec3::ZERO; 24];
        rows[20] = Vec3::Y;
        rows[19] = Vec3::X;
        assert_eq!(slot_groups(&xbe, 0, &rows).unwrap(), [(23, Vec3::Y), (2, -Vec3::X)]);
    }

    #[test]
    fn suits_are_found_for_their_wearer_only() {
        let names = ["mdTn00tn.xpr", "mdTn00tn.cat", "mdKs01tn.xpr", "mdTn00ht.xpr", "mdSun1tn.bin", "vtn00.xpr"];
        assert_eq!(costumes(names.into_iter(), "tn"), ["mdKs01tn", "mdTn00tn"]);
        assert_eq!(costumes(["mdTn00ls.xpr", "mdTn00he.xpr", "mdTn00tn.xpr"].into_iter(), "he"), ["mdTn00he", "mdTn00ls"]);
        // Hitomi (`ht`) wears her own suits; a code that is no character's gives nothing.
        assert_eq!(costumes(names.into_iter(), "ht"), ["mdTn00ht"]);
        assert!(costumes(names.into_iter(), "zz").is_empty());
    }

    #[test]
    fn pieces_skip_the_header_and_other_modes() {
        // Records from +0x14: kind 4 with value 0x10, then the end. List at 0x24.
        let mut cat = vec![0u8; 0x24];
        cat[4] = 0x14;
        cat[0x14] = 4;
        cat[0x18] = 0x10;
        cat.extend_from_slice(&[1, 0xFE, 0xFE, 0xFE, 1, 0x02, 3, 0x0A, 0, 1, 3, 9, 0x0A, 2, 1, 3, 10, 0x0B, 2, 0, 39, 6, 4, 7, 0]);
        let placed = placed_objects(&cat).unwrap();
        assert_eq!(placed, [
            Placed { object: 3, bone: 2, slot: None },
            Placed { object: 9, bone: 3, slot: None },
            Placed { object: 39, bone: 0, slot: Some(0) },
            Placed { object: 7, bone: 4, slot: None },
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

#[cfg(test)]
mod chest_set_check {
    use super::*;

    /// Each chest set's base shape is its piece's own vertices, for every character and suit.
    /// Reading the lists as 32-byte vertices put the arm and suit sets on the wrong vertices.
    #[test]
    #[ignore]
    fn chest_base_shapes_are_their_pieces() {
        let dvd = LazyArchive::open("assets/doax/datadvd.afs").unwrap();
        let hdd = LazyArchive::open("assets/doax/datahdd.afs").unwrap();
        let names: Vec<String> = hdd.names().map(str::to_string).collect();
        let worst = std::cell::Cell::new(0f32);
        let check = |name: &str, xpr: &[u8], cat: &[u8], body: bool| -> usize {
            // A few suit files do not parse as models; the viewer cannot load them either.
            let Ok(model) = super::super::doa2u::parse_model(xpr) else {
                println!("{name}: not a model");
                return 0;
            };
            let sets = chest_sets(cat, xpr, body);
            for set in &sets {
                let piece = &model.pieces[set.object];
                for (group, value) in set.vertices.iter().zip(&set.shapes[0]) {
                    for &v in group {
                        let rest = piece.vertices.get(v).unwrap_or_else(|| panic!("{name}: vertex {v} out of object {}", set.object));
                        let error = glam::Vec3::from(*value).distance(glam::Vec3::from(rest.position));
                        // Lei Fang's `mdTn90le` to `mdTn92le` have 66 vertices up to 4 mm from
                        // their base shape, in the file; every other set is within 0.03 mm.
                        assert!(error < 5e-3, "{name} object {} vertex {v}: {error}", set.object);
                        worst.set(worst.get().max(error));
                    }
                }
            }
            sets.len()
        };
        for (_, code, _) in ROSTER {
            let body = check(code, &dvd.read(&format!("v{code}00.xpr")).unwrap(), &dvd.read(&format!("v{code}00.cat")).unwrap(), true);
            let mut suits = 0;
            for stem in costumes(names.iter().map(String::as_str), code) {
                // Some, such as `mdEar_tn`, have no CAT.
                let Ok(cat) = hdd.read(&format!("{stem}.cat")) else { continue };
                suits += check(&stem, &hdd.read(&format!("{stem}.xpr")).unwrap(), &cat, false);
            }
            println!("{code}: {body} body sets, {suits} suit sets, largest error {:.5}", worst.get());
        }
    }
}
