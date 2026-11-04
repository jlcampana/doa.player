use anyhow::{bail, Result};
use byteorder::{BE, LE, ReadBytesExt};
use std::io::{Cursor, Read, Seek, SeekFrom};

#[derive(Debug, Clone)]
pub struct XprTexture {
    pub width: u32,
    pub height: u32,
    pub format: XprFormat,
    pub rgba: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum XprFormat {
    Dxt1,
    Dxt2,
    Dxt4,
    A8R8G8B8,
    Unknown(u8),
}

impl XprTexture {
    /// Parse an XPR0 (Xbox Packed Resource) texture file.
    pub fn from_xpr0(data: &[u8]) -> Result<Vec<Self>> {
        let mut cur = Cursor::new(data);

        let mut magic = [0u8; 4];
        cur.read_exact(&mut magic)?;
        if &magic != b"XPR0" {
            bail!("Not an XPR0 file");
        }

        let header_size = cur.read_u32::<LE>()? as usize;
        let _data_size = cur.read_u32::<LE>()?;

        if header_size > data.len() {
            bail!("XPR0 header_size {} > data length {}", header_size, data.len());
        }

        // After the 12-byte file header, texture descriptors follow.
        // Each Xbox D3DTexture structure is 36 bytes (common format).
        // We scan for "MDL" section which contains texture count info,
        // or parse D3DResource headers directly.

        let mut textures = Vec::new();
        let mut pos = 12usize;

        // Find the MDL section tag
        while pos + 4 <= header_size {
            cur.seek(SeekFrom::Start(pos as u64))?;
            let tag = cur.read_u32::<LE>()?;
            if tag == 0x004c444d {
                // "MDL\0" little endian
                let _unk = cur.read_u32::<LE>()?;
                let count = cur.read_u32::<LE>()? as usize;
                let _unk2 = cur.read_u32::<LE>()?;
                pos = cur.stream_position()? as usize;

                for _ in 0..count {
                    cur.seek(SeekFrom::Start(pos as u64))?;
                    let data_offset = cur.read_u32::<LE>()? as usize;
                    pos += 4;

                    if let Ok(tex) = Self::parse_xbox_texture(data, header_size + data_offset) {
                        textures.push(tex);
                    }
                }
                break;
            }
            pos += 4;
        }

        if textures.is_empty() {
            // Fallback: try to parse first texture directly at header end
            if let Ok(tex) = Self::parse_xbox_texture(data, header_size) {
                textures.push(tex);
            }
        }

        Ok(textures)
    }

    /// Parse an Xbox D3DTexture structure and decompress/decode it.
    fn parse_xbox_texture(data: &[u8], data_offset: usize) -> Result<Self> {
        // Xbox D3DTexture is 36 bytes but we need the pixel data descriptor.
        // For Xbox 1, the format word is at offset 12 in the D3DResource.
        // We need to find the texture dimensions and format from the header.
        //
        // Xbox D3DResource layout (simplified):
        //   +0x00: Common (4 bytes)
        //   +0x04: Data (4 bytes) - pointer to pixel data (relative to data section)
        //   +0x08: Lock (4 bytes)
        //   +0x0C: Format (4 bytes) - packed format word
        //   +0x10: Size (4 bytes)
        //   For Texture2D:
        //   +0x14: Width (bits), Height (bits), Depth (bits) in packed format

        if data_offset + 20 > data.len() {
            bail!("Texture descriptor out of bounds");
        }

        let mut cur = Cursor::new(data);
        cur.seek(SeekFrom::Start(data_offset as u64))?;

        // Try reading as a direct DXT/RGBA buffer assuming we know nothing
        // For now, just mark it as raw data needing format info from elsewhere
        let _common = cur.read_u32::<LE>()?;
        let pixel_data_offset = cur.read_u32::<LE>()? as usize;
        let _lock = cur.read_u32::<LE>()?;
        let format_word = cur.read_u32::<BE>()?; // Xbox stores format BE in some places

        let width = 256u32; // placeholder
        let height = 256u32;

        // Decode Xbox format word:
        // bits [7:0]  = format
        // bits [11:8] = mip levels
        // bits [19:12] = width exponent (1 << w_exp = width)
        // bits [27:20] = height exponent
        let fmt_byte = (format_word & 0xFF) as u8;
        let mips = ((format_word >> 8) & 0xF) as u32;
        let w_exp = ((format_word >> 12) & 0xFF) as u32;
        let h_exp = ((format_word >> 20) & 0xFF) as u32;

        let width = if w_exp > 0 { 1u32 << w_exp } else { width };
        let height = if h_exp > 0 { 1u32 << h_exp } else { height };

        let format = match fmt_byte {
            0x0C => XprFormat::Dxt1,
            0x0E => XprFormat::Dxt2,
            0x10 => XprFormat::Dxt4,
            0x06 => XprFormat::A8R8G8B8,
            other => XprFormat::Unknown(other),
        };

        let pixel_start = data_offset + pixel_data_offset;
        let rgba = decode_texture(data, pixel_start, width, height, format);

        Ok(Self { width, height, format, rgba })
    }
}

fn decode_texture(data: &[u8], offset: usize, width: u32, height: u32, format: XprFormat) -> Vec<u8> {
    let pixel_count = (width * height) as usize;
    match format {
        XprFormat::Dxt1 => decode_dxt1(data, offset, width, height),
        XprFormat::Dxt2 | XprFormat::Dxt4 => decode_dxt4(data, offset, width, height),
        XprFormat::A8R8G8B8 => {
            let byte_count = pixel_count * 4;
            if offset + byte_count <= data.len() {
                let raw = &data[offset..offset + byte_count];
                // ARGB -> RGBA swizzle
                let mut rgba = vec![0u8; byte_count];
                for i in 0..pixel_count {
                    let a = raw[i * 4];
                    let r = raw[i * 4 + 1];
                    let g = raw[i * 4 + 2];
                    let b = raw[i * 4 + 3];
                    rgba[i * 4] = r;
                    rgba[i * 4 + 1] = g;
                    rgba[i * 4 + 2] = b;
                    rgba[i * 4 + 3] = a;
                }
                rgba
            } else {
                vec![255u8; pixel_count * 4]
            }
        }
        XprFormat::Unknown(_) => vec![128u8; pixel_count * 4],
    }
}

fn decode_dxt1(data: &[u8], offset: usize, width: u32, height: u32) -> Vec<u8> {
    let blocks_x = ((width + 3) / 4) as usize;
    let blocks_y = ((height + 3) / 4) as usize;
    let block_size = 8; // DXT1 = 8 bytes/block
    let needed = blocks_x * blocks_y * block_size;

    if offset + needed > data.len() {
        return vec![0u8; (width * height * 4) as usize];
    }

    let mut rgba = vec![0u8; (width * height * 4) as usize];

    for by in 0..blocks_y {
        for bx in 0..blocks_x {
            let block_offset = offset + (by * blocks_x + bx) * block_size;
            let block = &data[block_offset..block_offset + block_size];
            decode_dxt1_block(block, &mut rgba, bx * 4, by * 4, width as usize);
        }
    }

    rgba
}

fn decode_dxt1_block(block: &[u8], rgba: &mut [u8], bx: usize, by: usize, width: usize) {
    let c0 = u16::from_le_bytes([block[0], block[1]]);
    let c1 = u16::from_le_bytes([block[2], block[3]]);
    let bits = u32::from_le_bytes([block[4], block[5], block[6], block[7]]);

    let [r0, g0, b0] = rgb565_to_rgb888(c0);
    let [r1, g1, b1] = rgb565_to_rgb888(c1);

    let colors: [[u8; 4]; 4] = if c0 > c1 {
        [
            [r0, g0, b0, 255],
            [r1, g1, b1, 255],
            [
                ((2 * r0 as u32 + r1 as u32) / 3) as u8,
                ((2 * g0 as u32 + g1 as u32) / 3) as u8,
                ((2 * b0 as u32 + b1 as u32) / 3) as u8,
                255,
            ],
            [
                ((r0 as u32 + 2 * r1 as u32) / 3) as u8,
                ((g0 as u32 + 2 * g1 as u32) / 3) as u8,
                ((b0 as u32 + 2 * b1 as u32) / 3) as u8,
                255,
            ],
        ]
    } else {
        [
            [r0, g0, b0, 255],
            [r1, g1, b1, 255],
            [
                ((r0 as u32 + r1 as u32) / 2) as u8,
                ((g0 as u32 + g1 as u32) / 2) as u8,
                ((b0 as u32 + b1 as u32) / 2) as u8,
                255,
            ],
            [0, 0, 0, 0],
        ]
    };

    for py in 0..4usize {
        for px in 0..4usize {
            let idx = ((bits >> (2 * (py * 4 + px))) & 3) as usize;
            let out_x = bx + px;
            let out_y = by + py;
            if out_x < width && out_y * width + out_x < rgba.len() / 4 {
                let out_off = (out_y * width + out_x) * 4;
                rgba[out_off..out_off + 4].copy_from_slice(&colors[idx]);
            }
        }
    }
}

fn decode_dxt4(data: &[u8], offset: usize, width: u32, height: u32) -> Vec<u8> {
    let blocks_x = ((width + 3) / 4) as usize;
    let blocks_y = ((height + 3) / 4) as usize;
    let block_size = 16; // DXT4/5 = 16 bytes/block
    let needed = blocks_x * blocks_y * block_size;

    if offset + needed > data.len() {
        return vec![0u8; (width * height * 4) as usize];
    }

    let mut rgba = vec![0u8; (width * height * 4) as usize];

    for by in 0..blocks_y {
        for bx in 0..blocks_x {
            let block_offset = offset + (by * blocks_x + bx) * block_size;
            let block = &data[block_offset..block_offset + block_size];
            let alpha_block = &block[..8];
            let color_block = &block[8..];
            let alphas = decode_dxt5_alpha(alpha_block);
            decode_dxt1_block(color_block, &mut rgba, bx * 4, by * 4, width as usize);
            // Apply pre-multiplied alpha correction for DXT2
            for py in 0..4usize {
                for px in 0..4usize {
                    let out_x = bx + px;
                    let out_y = by + py;
                    if out_x < width as usize && out_y * width as usize + out_x < rgba.len() / 4 {
                        let out_off = (out_y * width as usize + out_x) * 4;
                        rgba[out_off + 3] = alphas[py * 4 + px];
                    }
                }
            }
        }
    }

    rgba
}

fn decode_dxt5_alpha(block: &[u8]) -> [u8; 16] {
    let a0 = block[0];
    let a1 = block[1];
    let bits: u64 = block[2] as u64
        | ((block[3] as u64) << 8)
        | ((block[4] as u64) << 16)
        | ((block[5] as u64) << 24)
        | ((block[6] as u64) << 32)
        | ((block[7] as u64) << 40);

    let alphas: [u8; 8] = if a0 > a1 {
        [
            a0, a1,
            ((6 * a0 as u32 + 1 * a1 as u32) / 7) as u8,
            ((5 * a0 as u32 + 2 * a1 as u32) / 7) as u8,
            ((4 * a0 as u32 + 3 * a1 as u32) / 7) as u8,
            ((3 * a0 as u32 + 4 * a1 as u32) / 7) as u8,
            ((2 * a0 as u32 + 5 * a1 as u32) / 7) as u8,
            ((1 * a0 as u32 + 6 * a1 as u32) / 7) as u8,
        ]
    } else {
        [
            a0, a1,
            ((4 * a0 as u32 + 1 * a1 as u32) / 5) as u8,
            ((3 * a0 as u32 + 2 * a1 as u32) / 5) as u8,
            ((2 * a0 as u32 + 3 * a1 as u32) / 5) as u8,
            ((1 * a0 as u32 + 4 * a1 as u32) / 5) as u8,
            0, 255,
        ]
    };

    let mut out = [0u8; 16];
    for i in 0..16 {
        out[i] = alphas[((bits >> (3 * i)) & 7) as usize];
    }
    out
}

fn rgb565_to_rgb888(c: u16) -> [u8; 3] {
    let r = ((c >> 11) & 0x1F) as u8;
    let g = ((c >> 5) & 0x3F) as u8;
    let b = (c & 0x1F) as u8;
    [
        (r << 3) | (r >> 2),
        (g << 2) | (g >> 4),
        (b << 3) | (b >> 2),
    ]
}
