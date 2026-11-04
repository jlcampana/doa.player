//! Character geometry stored in the `MDL` block of DOA3 `.xpr` files.
//!
//! Layout (all values little endian, all offsets absolute within the file):
//!   0x14: "MDL\0"
//!   0x24: object count
//!   0x28: u32 offset of each `OBJ\0` record
//! `OBJ\0` record:
//!   +0x10: vertex count
//!   +0x14: vertex buffer offset (32-byte vertices: position, normal, uv)
//!   +0x18: index count
//!   +0x1c: index buffer offset (u16 triangle strip stitched with degenerates)
//!
//!   +0x60: material blocks, up to the index buffer. Each block, from its
//!     marker dword: +0x68 layer count, +0x6C 16-byte layers (texture first),
//!     then the batch count and 16-byte batches (_, _, first index, triangles).
//!     Blocks are separated by one dword.
//!
//! Each object is stored in the local space of its bone.

use anyhow::{bail, Context, Result};

pub(super) const VERTEX_SIZE: usize = 32;

#[repr(C)]
#[derive(Copy, Clone, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct ModelVertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
}

#[derive(Debug, Clone, Default)]
pub struct ModelPiece {
    pub vertices: Vec<ModelVertex>,
    pub materials: Vec<ModelMaterial>,
}

/// A part of a piece drawn with one texture.
#[derive(Debug, Clone, Default)]
pub struct ModelMaterial {
    /// Triangle list indices, relative to the piece's `vertices`.
    pub indices: Vec<u32>,
    /// Layer texture indices, base layer first. Values beyond the model's
    /// texture list refer to textures the game supplies at runtime.
    pub textures: Vec<usize>,
    /// Only the front of each triangle is drawn. The lining of a skirt relies
    /// on this to stay hidden from outside.
    pub cull_back: bool,
    /// How the game lights this material. Only DOA2U reads it so far.
    pub look: Option<MaterialLook>,
}

/// A material as the game draws it: `D3DMATERIAL8` plus its texture stages.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MaterialLook {
    pub diffuse: [f32; 4],
    pub ambient: [f32; 3],
    pub specular: [f32; 3],
    pub emissive: [f32; 3],
    pub power: f32,
    pub specular_on: bool,
    /// Flag 8: does not write depth.
    pub no_depth_write: bool,
    /// Stage 0 takes alpha from the texture. Otherwise it is the material's.
    pub alpha_from_texture: bool,
    /// A texture blended over the base by its own alpha, with its stage's coordinates.
    pub layer: Option<usize>,
    pub base_reflects: bool,
    pub layer_reflects: bool,
}

impl ModelMaterial {
    /// First layer with a texture of the model.
    pub fn base_texture(&self, texture_count: usize) -> Option<usize> {
        self.textures.iter().copied().find(|&t| t < texture_count)
    }
}

#[derive(Debug, Clone)]
pub struct ModelTexture {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

pub(super) fn read_u32(data: &[u8], offset: usize) -> Result<u32> {
    let bytes = offset
        .checked_add(4)
        .and_then(|end| data.get(offset..end))
        .with_context(|| format!("read past end of file at {offset:#x}"))?;
    Ok(u32::from_le_bytes(bytes.try_into()?))
}

pub(super) fn slice(data: &[u8], offset: usize, len: usize) -> Result<&[u8]> {
    offset
        .checked_add(len)
        .and_then(|end| data.get(offset..end))
        .with_context(|| format!("range {offset:#x}+{len:#x} is outside the file"))
}

pub fn parse_xpr_model(data: &[u8]) -> Result<Vec<ModelPiece>> {
    if slice(data, 0, 4)? != b"XPR0" {
        bail!("not an XPR0 file");
    }
    if slice(data, 0x14, 4)? != b"MDL\0" {
        bail!("XPR has no MDL block");
    }

    let count = read_u32(data, 0x24)? as usize;
    (0..count)
        .map(|i| {
            let record = read_u32(data, 0x28 + i * 4)? as usize;
            parse_object(data, record).with_context(|| format!("object {i} at {record:#x}"))
        })
        .collect()
}

fn parse_object(data: &[u8], record: usize) -> Result<ModelPiece> {
    if slice(data, record, 4)? != b"OBJ\0" {
        bail!("missing OBJ magic");
    }
    let vertex_count = read_u32(data, record + 0x10)? as usize;
    let vertex_offset = read_u32(data, record + 0x14)? as usize;
    let index_count = read_u32(data, record + 0x18)? as usize;
    let index_offset = read_u32(data, record + 0x1c)? as usize;

    let vertex_bytes = vertex_count
        .checked_mul(VERTEX_SIZE)
        .context("vertex count overflow")?;
    let vertices = slice(data, vertex_offset, vertex_bytes)?
        .chunks_exact(VERTEX_SIZE)
        .map(bytemuck::pod_read_unaligned)
        .collect();

    let index_bytes = index_count.checked_mul(2).context("index count overflow")?;
    let strip: Vec<u32> = slice(data, index_offset, index_bytes)?
        .chunks_exact(2)
        .map(|b| u16::from_le_bytes([b[0], b[1]]) as u32)
        .collect();

    if let Some(bad) = strip.iter().find(|&&i| i as usize >= vertex_count) {
        bail!("index {bad} out of range for {vertex_count} vertices");
    }

    // A few objects have blocks in another layout. Draw those with one texture.
    let materials = parse_materials(data, record + 0x60, index_offset, &strip).unwrap_or_else(|e| {
        log::debug!("Object at {record:#x} uses one material: {e:#}");
        let texture = read_u32(data, record + 0xCC).map_or(usize::MAX, |t| t as usize);
        vec![ModelMaterial { indices: strip_to_triangles(&strip), textures: vec![texture], cull_back: false, look: None }]
    });
    Ok(ModelPiece { vertices, materials })
}

/// Offset in a material block of the flag that hides back faces. Inferred: see
/// `culling_flag_marks_consistent_winding`.
const MATERIAL_CULL_FLAG: usize = 0xC;

fn parse_materials(data: &[u8], first_block: usize, end: usize, strip: &[u32]) -> Result<Vec<ModelMaterial>> {
    // (first strip index, layer textures, back culling) of each block
    let mut blocks: Vec<(usize, Vec<usize>, bool)> = Vec::new();
    let mut block = first_block;
    let mut covered = 0usize;

    // Fewer than three remaining indices cannot form a triangle.
    while covered + 2 < strip.len() && block + 0x70 <= end {
        let layers = read_u32(data, block + 0x68)? as usize;
        if layers == 0 || layers > 8 {
            bail!("bad layer count {layers} at {block:#x}");
        }
        let textures = (0..layers)
            .map(|i| Ok(read_u32(data, block + 0x6C + i * 16)? as usize))
            .collect::<Result<Vec<_>>>()?;

        let count_at = block + 0x6C + layers * 16;
        let batches = read_u32(data, count_at)? as usize;
        let batch = |i: usize, field: usize| read_u32(data, count_at + 4 + i * 16 + field * 4).map(|v| v as usize);
        if batches == 0 {
            bail!("material without batches at {block:#x}");
        }
        let first = batch(0, 2)?;
        let last = batch(batches - 1, 2)? + batch(batches - 1, 3)? + 2;
        if first > last || last > strip.len() {
            bail!("batch range {first}..{last} of {}", strip.len());
        }

        blocks.push((first, textures, read_u32(data, block + MATERIAL_CULL_FLAG)? == 1));
        covered = last;
        block = count_at + 4 + batches * 16 + 4;
    }
    if blocks.is_empty() {
        bail!("object has no material blocks");
    }

    // Batches do not list every strip index: real triangles also use the
    // indices between blocks. Each material therefore runs up to the next one.
    // The two windows that cross into the next block are left out: they join
    // separate strands, like the band of hair across Tina's face.
    let starts: Vec<usize> = blocks.iter().map(|(first, _, _)| *first).collect();
    Ok(blocks
        .into_iter()
        .enumerate()
        .map(|(i, (first, textures, cull_back))| {
            // Blocks are not always in strip order: an earlier next block gives nothing.
            let end = starts.get(i + 1).map_or(strip.len(), |next| (*next).min(strip.len())).max(first);
            ModelMaterial { indices: strip_to_triangles(&strip[first..end]), textures, cull_back, look: None }
        })
        .collect())
}

/// Decodes the textures listed after the MDL block. Resources are 12-byte
/// vertex buffer headers followed by 20-byte texture headers, ending in -1.
pub fn parse_xpr_textures(data: &[u8]) -> Result<Vec<ModelTexture>> {
    let header_size = read_u32(data, 8)? as usize;
    let mut offset = 0x14 + read_u32(data, 0x10)? as usize;
    let mut textures = Vec::new();

    while offset + 12 <= header_size {
        let common = read_u32(data, offset)?;
        if common == u32::MAX {
            break;
        }
        match (common >> 16) & 7 {
            0 => offset += 12,
            4 => {
                let pixels = read_u32(data, offset + 4)? as usize + header_size;
                let format = read_u32(data, offset + 12)?;
                textures.push(decode_texture(data, pixels, format));
                offset += 20;
            }
            other => bail!("unknown resource type {other} at {offset:#x}"),
        }
    }
    Ok(textures)
}

fn decode_texture(data: &[u8], offset: usize, format: u32) -> ModelTexture {
    let width = 1u32 << ((format >> 20) & 0xF);
    let height = 1u32 << ((format >> 24) & 0xF);
    let (w, h) = (width as usize, height as usize);
    let pixels = match (format >> 8) & 0xFF {
        0x0C => decode_dxt(data.get(offset..), w, h, 8),
        0x0F => decode_dxt(data.get(offset..), w, h, 16),
        0x06 => decode_swizzled_argb(data.get(offset..), w, h),
        _ => None,
    };
    let rgba = pixels.unwrap_or_else(|| [200u8, 200, 200, 255].repeat(w * h));
    ModelTexture { width, height, rgba }
}

fn decode_swizzled_argb(data: Option<&[u8]>, w: usize, h: usize) -> Option<Vec<u8>> {
    let data = data?.get(..w * h * 4)?;
    let mut rgba = vec![0u8; w * h * 4];
    for (i, texel) in data.chunks_exact(4).enumerate() {
        // Morton order: bits alternate between X and Y up to the shorter side.
        let (mut x, mut y, mut bit, mut remaining) = (0usize, 0usize, 0usize, i);
        let (mut x_bits, mut y_bits) = (w.trailing_zeros(), h.trailing_zeros());
        while x_bits > 0 || y_bits > 0 {
            if x_bits > 0 {
                x |= (remaining & 1) << bit;
                remaining >>= 1;
                x_bits -= 1;
            }
            if y_bits > 0 {
                y |= (remaining & 1) << bit;
                remaining >>= 1;
                y_bits -= 1;
            }
            bit += 1;
        }
        let out = (y * w + x) * 4;
        rgba[out..out + 4].copy_from_slice(&[texel[2], texel[1], texel[0], texel[3]]);
    }
    Some(rgba)
}

/// Decodes DXT1 (8-byte blocks) or DXT4/5 (16-byte blocks).
fn decode_dxt(data: Option<&[u8]>, w: usize, h: usize, block_size: usize) -> Option<Vec<u8>> {
    let (blocks_x, blocks_y) = ((w + 3) / 4, (h + 3) / 4);
    let data = data?.get(..blocks_x * blocks_y * block_size)?;
    let mut rgba = vec![0u8; w * h * 4];

    for (b, block) in data.chunks_exact(block_size).enumerate() {
        let (bx, by) = (b % blocks_x * 4, b / blocks_x * 4);
        let (alpha_block, color) = block.split_at(block_size - 8);

        let c0 = u16::from_le_bytes([color[0], color[1]]);
        let c1 = u16::from_le_bytes([color[2], color[3]]);
        let expand = |c: u16| {
            [
                ((c >> 11) & 31) as u32 * 255 / 31,
                ((c >> 5) & 63) as u32 * 255 / 63,
                (c & 31) as u32 * 255 / 31,
            ]
        };
        let (e0, e1) = (expand(c0), expand(c1));
        let mix = |a: u32, b: u32, x: u32, y: u32, d: u32| {
            [(e0[0] * a + e1[0] * b) * x / d / y, (e0[1] * a + e1[1] * b) * x / d / y, (e0[2] * a + e1[2] * b) * x / d / y]
        };
        // Colour-only blocks with c0 <= c1 use the 3-colour mode with transparency.
        let three_colour = block_size == 8 && c0 <= c1;
        let palette: [[u32; 3]; 4] = if three_colour {
            [e0, e1, mix(1, 1, 1, 1, 2), [0, 0, 0]]
        } else {
            [e0, e1, mix(2, 1, 1, 1, 3), mix(1, 2, 1, 1, 3)]
        };
        let selectors = u32::from_le_bytes([color[4], color[5], color[6], color[7]]);

        let alpha: [u8; 16] = if block_size == 16 {
            let (a0, a1) = (alpha_block[0] as u32, alpha_block[1] as u32);
            let bits = alpha_block[2..8].iter().rev().fold(0u64, |acc, &v| (acc << 8) | v as u64);
            std::array::from_fn(|i| {
                let code = ((bits >> (i * 3)) & 7) as u32;
                (match code {
                    0 => a0,
                    1 => a1,
                    c if a0 > a1 => ((8 - c) * a0 + (c - 1) * a1) / 7,
                    6 => 0,
                    7 => 255,
                    c => ((6 - c) * a0 + (c - 1) * a1) / 5,
                }) as u8
            })
        } else {
            [255; 16]
        };

        for i in 0..16 {
            let (x, y) = (bx + i % 4, by + i / 4);
            if x >= w || y >= h {
                continue;
            }
            let code = ((selectors >> (i * 2)) & 3) as usize;
            let c = palette[code];
            let a = if three_colour && code == 3 { 0 } else { alpha[i] };
            let out = (y * w + x) * 4;
            rgba[out..out + 4].copy_from_slice(&[c[0] as u8, c[1] as u8, c[2] as u8, a]);
        }
    }
    Some(rgba)
}

/// Expands a triangle strip to a list, dropping the degenerate stitching
/// triangles and keeping a consistent winding.
pub(super) fn strip_to_triangles(strip: &[u32]) -> Vec<u32> {
    let mut triangles = Vec::with_capacity(strip.len().saturating_sub(2) * 3);
    for (i, t) in strip.windows(3).enumerate() {
        let (a, b, c) = (t[0], t[1], t[2]);
        if a == b || b == c || a == c {
            continue;
        }
        if i % 2 == 0 {
            triangles.extend([a, b, c]);
        } else {
            triangles.extend([b, a, c]);
        }
    }
    triangles
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_skips_degenerates_and_flips_odd_triangles() {
        assert_eq!(
            strip_to_triangles(&[0, 1, 2, 3, 3, 4, 4, 5, 6]),
            vec![0, 1, 2, 2, 1, 3, 4, 5, 6]
        );
    }

    #[test]
    fn parses_minimal_model() {
        let record = 0x30usize;
        let vertices = record + 0x20;
        let indices = vertices + 3 * VERTEX_SIZE; // both overlap unused record fields
        let mut f = vec![0u8; record + 0x100];
        f[0..4].copy_from_slice(b"XPR0");
        f[0x14..0x18].copy_from_slice(b"MDL\0");
        f[0x24..0x28].copy_from_slice(&1u32.to_le_bytes());
        f[0x28..0x2c].copy_from_slice(&(record as u32).to_le_bytes());
        f[record..record + 4].copy_from_slice(b"OBJ\0");
        for (off, v) in [(0x10, 3u32), (0x14, vertices as u32), (0x18, 3), (0x1c, indices as u32)] {
            f[record + off..record + off + 4].copy_from_slice(&v.to_le_bytes());
        }
        f[vertices..vertices + 4].copy_from_slice(&2.5f32.to_le_bytes());
        for (i, v) in [0u16, 1, 2].iter().enumerate() {
            f[indices + i * 2..indices + i * 2 + 2].copy_from_slice(&v.to_le_bytes());
        }

        // Material block: one layer using texture 7, one batch of one triangle.
        // Indices move past it so that the block fits before them.
        let block = record + 0x60;
        let indices = block + 0x6C + 16 + 4 + 16 + 4;
        let mut f = f;
        f.resize(indices + 6, 0);
        f[record + 0x1c..record + 0x20].copy_from_slice(&(indices as u32).to_le_bytes());
        for (i, v) in [0u16, 1, 2].iter().enumerate() {
            f[indices + i * 2..indices + i * 2 + 2].copy_from_slice(&v.to_le_bytes());
        }
        let vertices_end = vertices + 3 * VERTEX_SIZE;
        assert!(vertices_end > block, "vertices overlap the block in this test");
        let vertices = indices + 6;
        f.resize(vertices + 3 * VERTEX_SIZE, 0);
        f[record + 0x14..record + 0x18].copy_from_slice(&(vertices as u32).to_le_bytes());
        f[vertices..vertices + 4].copy_from_slice(&2.5f32.to_le_bytes());
        for b in block..indices {
            f[b] = 0;
        }
        f[block + 0x68..block + 0x6C].copy_from_slice(&1u32.to_le_bytes());
        f[block + 0x6C..block + 0x70].copy_from_slice(&7u32.to_le_bytes());
        f[block + 0x7C..block + 0x80].copy_from_slice(&1u32.to_le_bytes());
        f[block + 0x8C..block + 0x90].copy_from_slice(&1u32.to_le_bytes());

        let pieces = parse_xpr_model(&f).unwrap();
        assert_eq!(pieces.len(), 1);
        assert_eq!(pieces[0].vertices[0].position[0], 2.5);
        assert_eq!(pieces[0].materials.len(), 1);
        assert_eq!(pieces[0].materials[0].indices, vec![0, 1, 2]);
        assert_eq!(pieces[0].materials[0].textures, vec![7]);
    }

    #[test]
    fn materials_keep_triangles_between_batches() {
        // Two materials: batches cover indices 0..4 and 6..10, leaving 4 and 5 out.
        let (first_block, layer_size) = (0x10usize, 16usize);
        let second_block = first_block + 0x6C + layer_size + 4 + 16 + 4;
        let end = second_block + 0x6C + layer_size + 4 + 16 + 4;
        let mut f = vec![0u8; end];
        for (block, texture, first) in [(first_block, 3u32, 0u32), (second_block, 4, 6)] {
            f[block + 0x68..block + 0x6C].copy_from_slice(&1u32.to_le_bytes());
            f[block + 0x6C..block + 0x70].copy_from_slice(&texture.to_le_bytes());
            f[block + 0x7C..block + 0x80].copy_from_slice(&1u32.to_le_bytes());
            f[block + 0x88..block + 0x8C].copy_from_slice(&first.to_le_bytes());
            f[block + 0x8C..block + 0x90].copy_from_slice(&2u32.to_le_bytes());
        }
        let strip: Vec<u32> = (0..10).collect();
        let materials = parse_materials(&f, first_block, end, &strip).unwrap();

        // Every triangle appears once, except the two that cross into the next block.
        let total: usize = materials.iter().map(|m| m.indices.len() / 3).sum();
        assert_eq!(total, strip.len() - 2 - 2);
        assert_eq!(materials[0].textures, vec![3]);
        assert_eq!(materials[1].textures, vec![4]);
    }

    #[test]
    fn rejects_out_of_range_index() {
        let record = 0x30usize;
        let mut f = vec![0u8; record + 0x40];
        f[0..4].copy_from_slice(b"XPR0");
        f[0x14..0x18].copy_from_slice(b"MDL\0");
        f[0x24..0x28].copy_from_slice(&1u32.to_le_bytes());
        f[0x28..0x2c].copy_from_slice(&(record as u32).to_le_bytes());
        f[record..record + 4].copy_from_slice(b"OBJ\0");
        for (off, v) in [(0x10, 0u32), (0x14, 0x30), (0x18, 1), (0x1c, 0x30)] {
            f[record + off..record + off + 4].copy_from_slice(&v.to_le_bytes());
        }
        assert!(parse_xpr_model(&f).is_err());
    }

    /// Every costume of every DOA3 character must load, with valid texture
    /// references and an object list. Skipped when the game assets are missing.
    #[test]
    fn doa3_costumes_parse() {
        let root = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/doa3/");
        let Ok(xbe) = std::fs::read(format!("{root}default.xbe")) else { return };
        let Ok(archive) = crate::formats::AfsArchive::from_file(format!("{root}loadfile.afs")) else { return };
        let xbe = crate::formats::doa3::Xbe::from_bytes(xbe).unwrap();

        for character in crate::app::doa3_characters(&xbe, &archive) {
            let (mut failures, mut no_joints, mut materials, mut untextured) = (Vec::new(), 0, 0, 0);
            for &entry in &character.afs_indices {
                let xpr = archive.get_by_index(entry).unwrap();
                let cat = archive.get_by_index(entry + 1).unwrap();
                let result = (|| -> Result<()> {
                    let pieces = parse_xpr_model(xpr)?;
                    let textures = parse_xpr_textures(xpr)?;
                    let nodes = crate::formats::doa3::parse_node_objects(cat)?;
                    if nodes.skins.is_empty() {
                        no_joints += 1;
                    }
                    for skin in &nodes.skins {
                        let piece = pieces.get(skin.object).context("skin object out of range")?;
                        if let Some(v) = skin.vertices.iter().find(|v| v.vertex >= piece.vertices.len()) {
                            bail!("skin of object {}: vertex {} of {}", skin.object, v.vertex, piece.vertices.len());
                        }
                    }
                    for &(_, object) in &nodes.rigid {
                        let piece = pieces.get(object).context("rigid object out of range")?;
                        materials += piece.materials.len();
                        untextured += piece
                            .materials
                            .iter()
                            .filter(|m| m.base_texture(textures.len()).is_none())
                            .count();
                    }
                    Ok(())
                })();
                if let Err(e) = result {
                    failures.push(format!("{entry}: {e:#}"));
                }
            }
            println!(
                "{:14} {:3} costumes, {materials} materials, {untextured} untextured, {} without joint pieces, {} failed {:?}",
                character.display_name,
                character.afs_indices.len(),
                no_joints,
                failures.len(),
                failures.first()
            );
            assert!(failures.is_empty(), "{}: {:?}", character.display_name, failures);
        }
    }

    /// Materials with the flag have all their faces wound the same way against
    /// their normals, and most others do not: the sign that the flag means culling.
    #[test]
    fn culling_flag_marks_consistent_winding() {
        let Ok(d) = std::fs::read("assets/doa3/loadfile.afs") else { return };
        let r = |o: usize| u32::from_le_bytes(d[o..o + 4].try_into().unwrap()) as usize;
        let count = r(4);
        let (mut flagged, mut flagged_clean, mut plain, mut plain_clean, mut opposing) = (0, 0, 0, 0, 0);
        for index in 0..count {
            let at = 8 + index * 8;
            let data = &d[r(at)..r(at) + r(at + 4)];
            if !data.starts_with(b"XPR0") || data.get(0x14..0x18) != Some(b"MDL\0") {
                continue;
            }
            let Ok(pieces) = parse_xpr_model(data) else { continue };
            for piece in &pieces {
                for m in piece.materials.iter().filter(|m| m.indices.len() >= 30) {
                    let (mut with, mut against) = (0, 0);
                    for t in m.indices.chunks_exact(3) {
                        let v = |i: usize| glam::Vec3::from(piece.vertices[t[i] as usize].position);
                        let n = |i: usize| glam::Vec3::from(piece.vertices[t[i] as usize].normal);
                        let face = (v(1) - v(0)).cross(v(2) - v(0));
                        if face.dot(n(0) + n(1) + n(2)) > 0.0 { with += 1 } else { against += 1 }
                    }
                    let clean = with.min(against) * 10 <= with + against;
                    if m.cull_back {
                        flagged += 1;
                        flagged_clean += clean as u32;
                        opposing += (against > with) as u32;
                    } else {
                        plain += 1;
                        plain_clean += clean as u32;
                    }
                }
            }
        }
        println!("flag: {flagged_clean} of {flagged} consistent, {opposing} against their normals; no flag: {plain_clean} of {plain}");
        assert!(flagged > 0 && flagged_clean * 10 >= flagged * 9);
    }
}
