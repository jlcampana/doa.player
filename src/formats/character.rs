/// Parser for .bin (character physics/logic) files.
///
/// BIN file layout:
///   0x00-0x87: Offset table (34 x u32 LE) pointing to data sections within the file
///     [0]  = draw call table start (0x88)
///     [1]  = texture lookup table
///     [2]  = index buffer / face data
///     [3]  = more face data
///     [4]  = collision capsule data
///     [32] = compressed section start
///     [33] = file size - 8
///
/// The file contains:
///   - Collision capsules (hitboxes) for bones: center + radius as floats
///   - Draw call records: 8-byte entries with primitive type and index range
///   - Bone hierarchy (parent-child relationships)
///
/// This is primarily game logic data, not 3D geometry.

use anyhow::Result;
use byteorder::{LE, ReadBytesExt};
use glam::Vec3;
use std::io::{Cursor, Read, Seek, SeekFrom};

#[derive(Debug, Clone)]
pub struct CollisionCapsule {
    pub bone_idx: u32,
    pub center: Vec3,
    pub radius: f32,
    pub height: f32,
}

#[derive(Debug, Default)]
pub struct CharacterBin {
    pub collision_capsules: Vec<CollisionCapsule>,
    pub section_offsets: Vec<u32>,
}

impl CharacterBin {
    pub fn from_bytes(data: &[u8]) -> Result<Self> {
        if data.len() < 0x88 {
            return Ok(Self::default());
        }

        let mut cur = Cursor::new(data);
        let mut section_offsets = Vec::new();

        // Read 34 section offsets
        for _ in 0..34 {
            section_offsets.push(cur.read_u32::<LE>()?);
        }

        // Section [4] contains collision capsule data
        let collision_capsules = if section_offsets.len() > 4 {
            parse_collision_capsules(data, section_offsets[4] as usize)
        } else {
            Vec::new()
        };

        Ok(Self { collision_capsules, section_offsets })
    }
}

fn parse_collision_capsules(data: &[u8], offset: usize) -> Vec<CollisionCapsule> {
    let mut capsules = Vec::new();
    if offset + 24 > data.len() {
        return capsules;
    }

    let mut cur = Cursor::new(&data[offset..]);
    let count = match cur.read_u32::<LE>() {
        Ok(c) => c,
        Err(_) => return capsules,
    };
    let _next_offset = cur.read_u32::<LE>().unwrap_or(0);

    for _ in 0..count.min(128) {
        let x = cur.read_f32::<LE>().unwrap_or(0.0);
        let y = cur.read_f32::<LE>().unwrap_or(0.0);
        let z = cur.read_f32::<LE>().unwrap_or(0.0);
        let radius = cur.read_f32::<LE>().unwrap_or(0.0);

        if !x.is_finite() || !y.is_finite() || !z.is_finite() || !radius.is_finite() {
            continue;
        }

        // Skip entries with unreasonable values for DOA characters
        if x.abs() > 100.0 || y.abs() > 100.0 || z.abs() > 100.0 || radius > 50.0 {
            continue;
        }

        capsules.push(CollisionCapsule {
            bone_idx: capsules.len() as u32,
            center: Vec3::new(x, y, z),
            radius,
            height: radius * 2.0,
        });
    }

    capsules
}
