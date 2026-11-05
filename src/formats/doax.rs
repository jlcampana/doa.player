//! Dead or Alive Xtreme Beach Volleyball, read as far as `docs/doax-notes.md` says.
//!
//! So far a character stands in the rest pose, with no motions, soft bodies, morphs or joint
//! bending. A costume is always a swimsuit on a body: the suit's pieces take the place of the
//! body's torso and hips, and the body is not offered without one.

use anyhow::{bail, Context, Result};
use glam::{Mat4, Vec3};
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
    positions.map(Mat4::from_translation)
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
        let float = |c: usize| read_u32(xbe, start + i * 16 + c * 4).map(f32::from_bits);
        *row = Vec3::new(float(0)?, float(1)?, float(2)?);
    }
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;

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
