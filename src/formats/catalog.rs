/// Parser for .cat (per-costume 3D geometry) files.
///
/// CAT file layout:
///   0x00-0x13: Header (5 x u32 LE)
///     [0] = format ID (0x3cf3a8 + costume_index)
///     [1] = 0x14 (header size = 20)
///     [2] = mesh info (count/offset, varies)
///     [3] = compressed data info
///     [4] = offset to uncompressed metadata section
///   0x14 .. header[4]: Compressed geometry (unknown compression, ~7.8 bits entropy)
///   header[4]+0x14 .. EOF: Uncompressed section
///     Contains bone bind matrices (4x4 float), LOD info, index buffers
///
/// The compressed geometry section has not yet been decoded.
/// Until decompression is understood, we provide skeleton data from the metadata section.

use anyhow::Result;
use byteorder::{LE, ReadBytesExt};
use glam::{Mat4, Vec3};
use std::io::{Cursor, Read, Seek, SeekFrom};

#[derive(Debug, Clone)]
pub struct BoneTransform {
    pub matrix: Mat4,
}

#[derive(Debug, Default)]
pub struct CatMesh {
    /// Bone bind-pose transforms extracted from the uncompressed section.
    pub bones: Vec<BoneTransform>,
    /// Index buffer data (u32 with 0xFFFF_FFFF restart markers for triangle strips).
    pub indices: Vec<u32>,
    /// Metadata: submesh descriptors from uncompressed section.
    pub submesh_count: u32,
}

impl CatMesh {
    pub fn from_bytes(data: &[u8]) -> Result<Self> {
        if data.len() < 20 {
            return Ok(Self::default());
        }

        let mut cur = Cursor::new(data);
        let _format_id = cur.read_u32::<LE>()?;
        let _header_size = cur.read_u32::<LE>()?; // always 0x14
        let _mesh_info = cur.read_u32::<LE>()?;
        let _compressed_info = cur.read_u32::<LE>()?;
        let uncompressed_offset = cur.read_u32::<LE>()? as usize;

        let meta_start = uncompressed_offset + 0x14; // skip 20-byte sub-header
        if meta_start >= data.len() {
            return Ok(Self::default());
        }

        let meta = &data[meta_start..];
        let mut meta_cur = Cursor::new(meta);

        // First DWORD of meta section seems to be submesh_count
        let submesh_count = meta_cur.read_u32::<LE>().unwrap_or(0);

        // Scan for identity matrix patterns to find bone transforms
        let bones = extract_bone_matrices(meta);

        // Scan end of file for index buffer data
        let indices = extract_index_buffer(data);

        Ok(Self { bones, indices, submesh_count })
    }
}

fn extract_bone_matrices(data: &[u8]) -> Vec<BoneTransform> {
    let mut bones = Vec::new();
    let len = data.len();
    if len < 64 {
        return bones;
    }

    let mut i = 0usize;
    while i + 64 <= len {
        // Check if we have a 4x4 float matrix with reasonable values (all finite, |v| < 1000)
        let chunk = &data[i..i + 64];
        let mut cols = [[0f32; 4]; 4];
        let mut valid = true;
        for col in 0..4 {
            for row in 0..4 {
                let off = (col * 4 + row) * 4;
                let v = f32::from_le_bytes(chunk[off..off + 4].try_into().unwrap());
                if !v.is_finite() || v.abs() > 1000.0 {
                    valid = false;
                    break;
                }
                cols[col][row] = v;
            }
            if !valid {
                break;
            }
        }
        if valid {
            let w = cols[3][3];
            if (w - 1.0).abs() < 0.01 {
                // Verify the 3×3 rotation block: each of the first 3 columns
                // must be approximately a unit vector (length within [0.85, 1.15])
                let mut rot_ok = true;
                for col in 0..3 {
                    let len_sq = cols[col][0] * cols[col][0]
                        + cols[col][1] * cols[col][1]
                        + cols[col][2] * cols[col][2];
                    if !(0.72..=1.32).contains(&len_sq) {
                        rot_ok = false;
                        break;
                    }
                }
                if rot_ok {
                    let matrix = Mat4::from_cols_array_2d(&cols);
                    bones.push(BoneTransform { matrix });
                    i += 64;
                    continue;
                }
            }
        }
        i += 4;
    }

    bones
}

fn extract_index_buffer(data: &[u8]) -> Vec<u32> {
    let mut indices = Vec::new();
    if data.len() < 8 {
        return indices;
    }

    // Find runs of u32 values that look like vertex indices (< 0x40000) or restart markers
    let aligned_len = (data.len() / 4) * 4;
    let mut i = aligned_len.saturating_sub(4096);
    i = i / 4 * 4; // align

    while i + 4 <= data.len() {
        let v = u32::from_le_bytes(data[i..i + 4].try_into().unwrap());
        if v == 0xFFFF_FFFF || v < 0x40000 {
            indices.push(v);
        }
        i += 4;
    }

    indices
}
