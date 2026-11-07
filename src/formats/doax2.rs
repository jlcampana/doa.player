//! Dead or Alive Xtreme 2 (Xbox 360): models in XPR2 files, big-endian.
//!
//! What is read so far, from Tina's body `vtn1.tpr` in `chr.afs` (see `docs/doax2-notes.md`):
//! - The XPR2 resource list: `USER` blocks, `TX2D` textures, `VBUF` and `IBUF` buffers whose
//!   headers end in Xenos fetch constants.
//! - `MdlGeo`, one `ObjGeo` per object with its `GeoDecl`: vertex declaration, vertex buffers
//!   with their strides, index buffer and counts.
//! - `Hie`, the node hierarchy: a matrix per node, the object it draws, its parent and children.
//! - Textures: DXT1, DXT3, DXT5 and 8888, untiled.
//!
//! Not read: `MdlInfo`, materials, the CAT's draw lists and slots, motions.

use anyhow::{bail, Context, Result};
use glam::{Mat4, Vec3};

use super::model::{ModelMaterial, ModelPiece, ModelTexture, ModelVertex};

fn be32(data: &[u8], at: usize) -> Result<u32> {
    let bytes = data.get(at..at + 4).with_context(|| format!("read past end at {at:#x}"))?;
    Ok(u32::from_be_bytes(bytes.try_into()?))
}

fn be16(data: &[u8], at: usize) -> Result<u16> {
    let bytes = data.get(at..at + 2).with_context(|| format!("read past end at {at:#x}"))?;
    Ok(u16::from_be_bytes(bytes.try_into()?))
}

fn bef32(data: &[u8], at: usize) -> Result<f32> {
    be32(data, at).map(f32::from_bits)
}

/// An XPR2 file: its resources, each with its tag and the offset of its header in the file.
pub struct Xpr2<'a> {
    pub file: &'a [u8],
    /// Where the data section starts; buffer and texture addresses are relative to it.
    pub data: usize,
    pub resources: Vec<([u8; 4], usize, usize)>,
}

impl<'a> Xpr2<'a> {
    pub fn parse(file: &'a [u8]) -> Result<Self> {
        if file.get(..4) != Some(&b"XPR2"[..]) {
            bail!("not an XPR2 file");
        }
        let header = be32(file, 4)? as usize;
        let count = be32(file, 12)? as usize;
        let resources = (0..count)
            .map(|i| {
                let at = 16 + i * 16;
                let tag: [u8; 4] = file.get(at..at + 4).context("resource table")?.try_into()?;
                Ok((tag, 12 + be32(file, at + 4)? as usize, be32(file, at + 8)? as usize))
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(Self { file, data: 12 + header, resources })
    }

    fn of_kind(&self, tag: &[u8; 4]) -> impl Iterator<Item = usize> + '_ {
        let tag = *tag;
        self.resources.iter().filter(move |r| r.0 == tag).map(|r| r.1)
    }

    /// The `USER` block whose record starts with `name`.
    fn user(&self, name: &[u8]) -> Option<usize> {
        self.of_kind(b"USER").find(|&at| self.file.get(at..at + name.len()) == Some(name))
    }

    /// A vertex or index buffer: its address in the file and size, from the fetch constant
    /// at +0x18 (address with the type in the low bits) and +0x1C.
    fn buffer(&self, header: usize, vertex: bool) -> Result<(usize, usize)> {
        let address = be32(self.file, header + 0x18)? as usize & !3;
        let size = be32(self.file, header + 0x1C)? as usize;
        Ok((self.data + address, if vertex { (size >> 2) * 4 } else { size }))
    }
}

/// A vertex element, as `D3DVERTEXELEMENT9` on the Xbox 360.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Element {
    pub stream: u16,
    pub offset: u16,
    pub kind: u32,
    pub usage: u8,
    pub usage_index: u8,
}

/// Element types seen in the files.
pub const FLOAT3: u32 = 0x2A23B9;
pub const FLOAT2: u32 = 0x2C23A5;
pub const FLOAT4: u32 = 0x1A23A6;
pub const HALF2: u32 = 0x2C235F;
/// Three signed normalized values in 11, 11 and 10 bits, x in the low bits.
pub const HEND3N: u32 = 0x2A2190;

pub const USAGE_POSITION: u8 = 0;
pub const USAGE_BLENDWEIGHT: u8 = 1;
pub const USAGE_NORMAL: u8 = 3;
pub const USAGE_TEXCOORD: u8 = 5;

/// A vertex layout of an object, with its buffers.
#[derive(Debug, Clone)]
pub struct Decl {
    pub elements: Vec<Element>,
    /// Per stream: the `VBUF` index and the stride.
    pub streams: Vec<(usize, usize)>,
    pub index_buffer: usize,
    pub index_count: usize,
    pub vertex_count: usize,
}

/// A part of an object drawn with one material: a range of its declaration's indices and
/// texture stages as (stage, kind, texture). Kind 1 on stage 0 is the base colour.
#[derive(Debug, Clone)]
pub struct Draw {
    pub decl: usize,
    pub first_index: usize,
    pub index_count: usize,
    pub stages: Vec<(u32, u32, usize)>,
    /// Diffuse, ambient, specular (vec4 each) and the specular power, at the offset in +0x20.
    pub diffuse: [f32; 3],
    pub ambient: [f32; 3],
    pub specular: [f32; 3],
    pub power: f32,
}

/// One `ObjGeo`.
#[derive(Debug, Clone)]
pub struct Object {
    pub decls: Vec<Decl>,
    pub draws: Vec<Draw>,
}

impl Object {
    pub fn vertex_count(&self) -> usize {
        self.decls.iter().map(|d| d.vertex_count).sum()
    }
}

/// The objects of `MdlGeo`.
///
/// `ObjGeo`: +0x14 the number of draws, +0x28 the offset of its `GeoDecl`, from +0x40 the
/// offsets of the draws, all from the `ObjGeo`'s start.
///
/// `GeoDecl`: +0x14 the number of layouts, from +0x30 their offsets from the `GeoDecl`. A
/// layout: +0 the offset of its stream list from the layout, +4 the number of streams, +8 the
/// index buffer, +0xC the index count, +0x14 the vertex count, its elements from +0x30, ended
/// by stream 0xFF. A stream entry is 16 bytes: the `VBUF` index and the stride.
///
/// A draw: +8 its layout, +0x10 the first index, +0x14 the index count, +0x18 the number of
/// texture stages, +0x1C their offset from the draw. A stage is 0x40 bytes: stage, kind,
/// texture.
pub fn objects(xpr: &Xpr2) -> Result<Vec<Object>> {
    let file = xpr.file;
    let model = xpr.user(b"MdlGeo").context("no MdlGeo")?;
    let count = be32(file, model + 0x14)? as usize;
    (0..count)
        .map(|i| {
            let at = model + be32(file, model + 0x30 + i * 4)? as usize;
            if file.get(at..at + 6) != Some(&b"ObjGeo"[..]) {
                bail!("object {i} is not an ObjGeo");
            }
            let geo = at + be32(file, at + 0x28)? as usize;
            if file.get(geo..geo + 7) != Some(&b"GeoDecl"[..]) {
                bail!("object {i} has no GeoDecl at {geo:#x}");
            }
            let decls = (0..be32(file, geo + 0x14)? as usize)
                .map(|d| {
                    let head = geo + be32(file, geo + 0x30 + d * 4)? as usize;
                    let mut elements = Vec::new();
                    let mut p = head + 0x30;
                    loop {
                        let stream = be16(file, p)?;
                        if stream == 0xFF {
                            break;
                        }
                        elements.push(Element {
                            stream,
                            offset: be16(file, p + 2)?,
                            kind: be32(file, p + 4)?,
                            usage: file[p + 9],
                            usage_index: file[p + 10],
                        });
                        p += 12;
                        if elements.len() > 32 {
                            bail!("object {i}: declaration does not end");
                        }
                    }
                    let list = head + be32(file, head)? as usize;
                    let streams = (0..be32(file, head + 4)? as usize)
                        .map(|s| Ok((be32(file, list + s * 16)? as usize, be32(file, list + s * 16 + 4)? as usize)))
                        .collect::<Result<Vec<_>>>()?;
                    Ok(Decl {
                        elements,
                        streams,
                        index_buffer: be32(file, head + 8)? as usize,
                        index_count: be32(file, head + 0xC)? as usize,
                        vertex_count: be32(file, head + 0x14)? as usize,
                    })
                })
                .collect::<Result<Vec<_>>>()?;
            let draws = (0..be32(file, at + 0x14)? as usize)
                .map(|d| {
                    let draw = at + be32(file, at + 0x40 + d * 4)? as usize;
                    let stage_at = draw + be32(file, draw + 0x1C)? as usize;
                    let stages = (0..be32(file, draw + 0x18)?.min(16) as usize)
                        .map(|k| {
                            let p = stage_at + k * 0x40;
                            Ok((be32(file, p)?, be32(file, p + 4)?, be32(file, p + 8)? as usize))
                        })
                        .collect::<Result<Vec<_>>>()?;
                    let colours = draw + be32(file, draw + 0x20)? as usize;
                    let vec3 = |at: usize| -> Result<[f32; 3]> { Ok([bef32(file, at)?, bef32(file, at + 4)?, bef32(file, at + 8)?]) };
                    Ok(Draw {
                        decl: be32(file, draw + 8)? as usize,
                        first_index: be32(file, draw + 0x10)? as usize,
                        index_count: be32(file, draw + 0x14)? as usize,
                        stages,
                        // +0x20 holds the colours' offset from the draw (0x80 in those read).
                        diffuse: vec3(colours)?,
                        ambient: vec3(colours + 0x10)?,
                        specular: vec3(colours + 0x20)?,
                        power: bef32(file, colours + 0x30)?,
                    })
                })
                .collect::<Result<Vec<_>>>()?;
            Ok(Object { decls, draws })
        })
        .collect()
}

/// A node of `Hie`: its matrix relative to the parent, the object it draws, its parent.
#[derive(Debug, Clone)]
pub struct Node {
    pub local: Mat4,
    pub object: usize,
    pub parent: Option<usize>,
    pub children: Vec<usize>,
}

/// `Hie`: per node 0x50 bytes, a row-major matrix with the translation in the last row, then
/// shorts: the object, the parent (-1 for none), the child count and the children.
pub fn hierarchy(xpr: &Xpr2) -> Result<Vec<Node>> {
    let file = xpr.file;
    let hie = xpr.user(b"Hie\0").context("no Hie")?;
    let count = be32(file, hie + 0x14)? as usize;
    (0..count)
        .map(|i| {
            let at = hie + be32(file, hie + 0x30 + i * 4)? as usize;
            let mut m = [0f32; 16];
            for (k, value) in m.iter_mut().enumerate() {
                *value = bef32(file, at + k * 4)?;
            }
            // Rows of the file are the columns of glam's column-major matrix.
            let local = Mat4::from_cols_array(&m);
            let short = |k: usize| -> Result<i16> { Ok(be16(file, at + 64 + k * 2)? as i16) };
            let child_count = short(2)?.max(0) as usize;
            Ok(Node {
                local,
                object: short(0)?.max(0) as usize,
                parent: usize::try_from(short(1)?).ok(),
                children: (0..child_count.min(5)).map(|k| short(3 + k).map(|c| c.max(0) as usize)).collect::<Result<_>>()?,
            })
        })
        .collect()
}

/// Each node's matrix in model space, in the rest pose.
pub fn rest_pose(nodes: &[Node]) -> Vec<Mat4> {
    let mut world: Vec<Option<Mat4>> = vec![None; nodes.len()];
    fn resolve(k: usize, nodes: &[Node], world: &mut Vec<Option<Mat4>>, depth: usize) -> Mat4 {
        if let Some(m) = world[k] {
            return m;
        }
        let m = match nodes[k].parent {
            Some(p) if p < nodes.len() && depth < 64 => resolve(p, nodes, world, depth + 1) * nodes[k].local,
            _ => nodes[k].local,
        };
        world[k] = Some(m);
        m
    }
    (0..nodes.len()).map(|k| resolve(k, nodes, &mut world, 0)).collect()
}

/// An IEEE half float.
fn half(bits: u16) -> f32 {
    let sign = if bits & 0x8000 != 0 { -1.0 } else { 1.0 };
    let exponent = ((bits >> 10) & 0x1F) as i32;
    let mantissa = (bits & 0x3FF) as f32;
    sign * match exponent {
        0 => mantissa * 2f32.powi(-24),
        31 => if mantissa == 0.0 { f32::INFINITY } else { f32::NAN },
        e => (1.0 + mantissa / 1024.0) * 2f32.powi(e - 15),
    }
}

/// Three signed normalized values in 11, 11 and 10 bits, x in the low bits. Measured: Tina's
/// body normals come out with length 0.998 to 1.0 this way.
fn hend3n(word: u32) -> [f32; 3] {
    let signed = |value: u32, bits: u32| -> f32 {
        let max = (1i32 << (bits - 1)) - 1;
        let mut v = (value & ((1 << bits) - 1)) as i32;
        if v > max {
            v -= 1 << bits;
        }
        (v as f32 / max as f32).max(-1.0)
    };
    [signed(word, 11), signed(word >> 11, 11), signed(word >> 22, 10)]
}

/// An object's vertices and triangles, its layouts one after another. Positions are in the
/// node's space. Each draw becomes a material with its base texture.
pub fn object_piece(xpr: &Xpr2, object: &Object) -> Result<ModelPiece> {
    let file = xpr.file;
    let vbufs: Vec<usize> = xpr.of_kind(b"VBUF").collect();
    let ibufs: Vec<usize> = xpr.of_kind(b"IBUF").collect();
    let mut vertices = Vec::new();
    let mut firsts = Vec::new();
    for decl in &object.decls {
        firsts.push(vertices.len());
        let start = vertices.len();
        vertices.resize(start + decl.vertex_count, ModelVertex { position: [0.0; 3], normal: [0.0, 1.0, 0.0], uv: [0.0; 2] });
        let out = &mut vertices[start..];
        for element in &decl.elements {
            let first = decl.elements.iter().find(|e| e.usage == element.usage && e.usage_index == 0 && e.stream == 0);
            if element.stream != 0 || Some(element) != first {
                continue;
            }
            let &(buffer, stride) = decl.streams.first().context("no stream 0")?;
            let (at, size) = xpr.buffer(*vbufs.get(buffer).context("VBUF out of range")?, true)?;
            if stride == 0 || size < decl.vertex_count * stride {
                bail!("vertex buffer {buffer} holds {size} bytes, short of {} vertices of {stride}", decl.vertex_count);
            }
            for (v, out) in out.iter_mut().enumerate() {
                let p = at + v * stride + element.offset as usize;
                match (element.usage, element.kind) {
                    (USAGE_POSITION, FLOAT3) => out.position = [bef32(file, p)?, bef32(file, p + 4)?, bef32(file, p + 8)?],
                    (USAGE_NORMAL, HEND3N) => out.normal = hend3n(be32(file, p)?),
                    (USAGE_NORMAL, FLOAT3) => out.normal = [bef32(file, p)?, bef32(file, p + 4)?, bef32(file, p + 8)?],
                    (USAGE_TEXCOORD, HALF2) => out.uv = [half(be16(file, p)?), half(be16(file, p + 2)?)],
                    (USAGE_TEXCOORD, FLOAT2) => out.uv = [bef32(file, p)?, bef32(file, p + 4)?],
                    _ => {}
                }
            }
        }
    }
    let mut materials = Vec::new();
    for draw in &object.draws {
        let decl = object.decls.get(draw.decl).context("draw's layout out of range")?;
        let base = firsts[draw.decl] as u32;
        // Strips, restarted by 0xFFFF.
        let (at, size) = xpr.buffer(*ibufs.get(decl.index_buffer).context("IBUF out of range")?, false)?;
        if size < (draw.first_index + draw.index_count) * 2 {
            bail!("index buffer {} is short", decl.index_buffer);
        }
        let mut indices = Vec::new();
        let mut strip: Vec<u32> = Vec::new();
        let flush = |strip: &mut Vec<u32>, indices: &mut Vec<u32>| {
            for k in 2..strip.len() {
                let (a, b, c) = if k % 2 == 0 { (strip[k - 2], strip[k - 1], strip[k]) } else { (strip[k - 1], strip[k - 2], strip[k]) };
                if a != b && b != c && a != c {
                    indices.extend([a, b, c]);
                }
            }
            strip.clear();
        };
        for k in draw.first_index..draw.first_index + draw.index_count {
            let index = be16(file, at + k * 2)?;
            if index == 0xFFFF {
                flush(&mut strip, &mut indices);
            } else if (index as usize) < decl.vertex_count {
                strip.push(base + index as u32);
            } else {
                bail!("index {index} beyond {} vertices", decl.vertex_count);
            }
        }
        flush(&mut strip, &mut indices);
        // Stage 0 of kind 0 (hair strands, and dark strand-shaped shadows over them) is a colour
        // texture blended by its alpha; stage 1's grey texture there is a band of shine, not a
        // mask (seen on Tina's and Lei Fang's). Such materials list both textures, which marks
        // them for blending (`is_blended`); the viewer draws the first. Otherwise stage 0's.
        let stage = |n: u32| draw.stages.iter().find(|s| s.0 == n);
        let textures = match (stage(0), stage(1)) {
            (Some(&(_, 0, color)), Some(&(_, 1, mask))) if mask != u32::MAX as usize => vec![color, mask],
            (Some(&(_, _, color)), _) => vec![color],
            _ => Vec::new(),
        };
        // The draw's colours, lit as the viewer lights DOA2U's materials. DOAX2's shaders also
        // take a two-tone ambient and point lights, which this leaves out.
        let sane = |c: [f32; 3]| c.iter().all(|v| v.is_finite() && (0.0..=8.0).contains(v));
        let look = (sane(draw.diffuse) && sane(draw.ambient) && sane(draw.specular)).then(|| super::model::MaterialLook {
            diffuse: [draw.diffuse[0], draw.diffuse[1], draw.diffuse[2], 1.0],
            ambient: draw.ambient,
            specular: draw.specular,
            emissive: [0.0; 3],
            power: if draw.power.is_finite() { draw.power } else { 0.0 },
            specular_on: draw.specular.iter().any(|&v| v > 0.0) && draw.power > 0.0,
            no_depth_write: false,
            alpha_from_texture: textures.len() == 2,
            layer: None,
            base_reflects: false,
            layer_reflects: false,
            unlit: false,
        });
        materials.push(ModelMaterial { indices, textures, cull_back: false, look });
    }
    Ok(ModelPiece { vertices, materials })
}

/// Each vertex's blend weights, the layouts one after another as in `object_piece`: the
/// blend weight element (usage 1 on stream 0), two to four floats that add up to one (scaled
/// to, where a file's do not). Empty for objects without them.
pub fn object_weights(xpr: &Xpr2, object: &Object) -> Result<Vec<Vec<f32>>> {
    let file = xpr.file;
    let vbufs: Vec<usize> = xpr.of_kind(b"VBUF").collect();
    let mut out = Vec::new();
    for decl in &object.decls {
        let element = decl.elements.iter().find(|e| e.stream == 0 && e.usage == USAGE_BLENDWEIGHT);
        let count = match element.map(|e| e.kind) {
            Some(FLOAT2) => 2,
            Some(FLOAT3) => 3,
            Some(FLOAT4) => 4,
            _ => 0,
        };
        let &(buffer, stride) = decl.streams.first().context("no stream 0")?;
        let (at, _) = xpr.buffer(*vbufs.get(buffer).context("VBUF out of range")?, true)?;
        for v in 0..decl.vertex_count {
            let mut weights = Vec::with_capacity(count);
            if let Some(element) = element {
                for k in 0..count {
                    weights.push(bef32(file, at + v * stride + element.offset as usize + k * 4)?);
                }
            }
            // Some suits' weights add up to more than one: a single 2.307 in Tina's `on_s_m`,
            // or 1.16 to 1.23 over two groups. Taken as they are, those vertices fly off; the
            // game shows no such spikes, so they are scaled to add up to one. Not checked
            // against the game's shader.
            let sum: f32 = weights.iter().sum();
            if sum > 1e-4 && (sum - 1.0).abs() > 1e-3 {
                weights.iter_mut().for_each(|w| *w /= sum);
            }
            out.push(weights);
        }
    }
    Ok(out)
}

/// An object's six extra position shapes, per vertex in `object_piece`'s order, for objects
/// whose layouts have seven streams: streams 1 to 6 hold whole positions, in pairs along an
/// axis (on Tina's chest, 1 and 4 move the vertices sideways by +-1.5 cm on average, 2 and 3
/// forward and back, 5 up by 3.5 cm and 6 down by 2.3 cm). The declaration binds three of
/// them, so the game picks one of each pair. Layouts without them keep the base positions.
pub fn object_shapes(xpr: &Xpr2, object: &Object, base: &[ModelVertex]) -> Result<Option<Vec<[[f32; 3]; 6]>>> {
    let file = xpr.file;
    if !object.decls.iter().any(|d| d.streams.len() == 7) {
        return Ok(None);
    }
    let vbufs: Vec<usize> = xpr.of_kind(b"VBUF").collect();
    let mut out: Vec<[[f32; 3]; 6]> = base.iter().map(|v| [v.position; 6]).collect();
    let mut first = 0;
    for decl in &object.decls {
        if decl.streams.len() == 7 {
            for (shape, &(buffer, stride)) in decl.streams[1..].iter().enumerate() {
                let (at, size) = xpr.buffer(*vbufs.get(buffer).context("VBUF out of range")?, true)?;
                if stride < 12 || size < decl.vertex_count * stride {
                    bail!("shape buffer {buffer} is short");
                }
                for v in 0..decl.vertex_count {
                    let p = at + v * stride;
                    if let Some(slot) = out.get_mut(first + v) {
                        slot[shape] = [bef32(file, p)?, bef32(file, p + 4)?, bef32(file, p + 8)?];
                    }
                }
            }
        }
        first += decl.vertex_count;
    }
    Ok(Some(out))
}

/// The shapes the vertical chest spring uses: up (stream 5) and down (stream 6).
pub const SHAPE_UP: usize = 4;
pub const SHAPE_DOWN: usize = 5;

/// Whether a material is blended by its texture's alpha: hair strands (stage 0 of kind 0,
/// listed with their shine texture as a second) and, after `load`, draws whose texture has
/// over 1% see-through texels (listed twice).
pub fn is_blended(material: &ModelMaterial) -> bool {
    material.textures.len() == 2
}

/// Xenos texture formats seen so far.
const FMT_8888: u32 = 0x06;
const FMT_DXT1: u32 = 0x12;
const FMT_DXT3: u32 = 0x13;
const FMT_DXT5: u32 = 0x14;

/// Byte offset of block (x, y) in a tiled surface, as the Xenos lays it out. `width` is in
/// blocks; `log2_bpp` is the log2 of the bytes per block. From Xenia's `TiledOffset2D`.
fn tiled_offset(x: u32, y: u32, width: u32, log2_bpp: u32) -> u32 {
    let aligned = (width + 31) & !31;
    let macro_ = ((x >> 5) + (y >> 5) * (aligned >> 5)) << (log2_bpp + 7);
    let micro = ((x & 7) + ((y & 0xE) << 2)) << log2_bpp;
    let offset = macro_ + ((micro & !0xF) << 1) + (micro & 0xF) + ((y & 1) << 4);
    ((offset & !0x1FF) << 3) + ((y & 16) << 7) + ((offset & 0x1C0) << 2) + (((((y & 8) >> 2) + (x >> 3)) & 3) << 6) + (offset & 0x3F)
}

/// The textures, in file order, each as RGBA. Formats not read come out grey.
pub fn textures(xpr: &Xpr2) -> Vec<ModelTexture> {
    xpr.of_kind(b"TX2D").map(|at| texture(xpr, at).unwrap_or_else(|e| {
        log::warn!("DOAX2 texture at {at:#x}: {e:#}");
        ModelTexture { width: 4, height: 4, rgba: vec![128; 64] }
    })).collect()
}

fn texture(xpr: &Xpr2, header: usize) -> Result<ModelTexture> {
    let file = xpr.file;
    // The fetch constant follows the 28-byte resource header.
    let word = |k: usize| be32(file, header + 28 + k * 4);
    let (w0, w1, w2) = (word(0)?, word(1)?, word(2)?);
    let tiled = w0 >> 31 != 0;
    let format = w1 & 0x3F;
    let endian = (w1 >> 6) & 3;
    let base = xpr.data + (w1 >> 12 << 12) as usize;
    let width = (w2 & 0x1FFF) + 1;
    let height = ((w2 >> 13) & 0x1FFF) + 1;
    let (block, bytes) = match format {
        FMT_8888 => (1, 4),
        FMT_DXT1 => (4, 8),
        FMT_DXT3 | FMT_DXT5 => (4, 16),
        _ => bail!("format {format:#x} not read"),
    };
    let (bw, bh) = (width.div_ceil(block), height.div_ceil(block));
    let log2 = (bytes as u32).trailing_zeros();
    // Untile into a linear little-endian block list.
    let mut linear = vec![0u8; (bw * bh) as usize * bytes];
    for y in 0..bh {
        for x in 0..bw {
            let from = if tiled { tiled_offset(x, y, bw, log2) as usize } else { ((y * bw.next_multiple_of(32) + x) as usize) * bytes };
            let src = file.get(base + from..base + from + bytes).context("texture data past the end")?;
            let dst = &mut linear[((y * bw + x) as usize) * bytes..][..bytes];
            dst.copy_from_slice(src);
            match endian {
                1 => dst.chunks_exact_mut(2).for_each(|c| c.swap(0, 1)),
                2 => dst.chunks_exact_mut(4).for_each(|c| c.reverse()),
                3 => dst.chunks_exact_mut(4).for_each(|c| {
                    c.swap(0, 2);
                    c.swap(1, 3);
                }),
                _ => {}
            }
        }
    }
    let rgba = match format {
        FMT_8888 => linear.chunks_exact(4).flat_map(|p| [p[2], p[1], p[0], p[3]]).collect(),
        FMT_DXT1 => super::xpr::decode_blocks(&linear, width, height, 1),
        FMT_DXT3 => super::xpr::decode_blocks(&linear, width, height, 3),
        _ => super::xpr::decode_blocks(&linear, width, height, 5),
    };
    Ok(ModelTexture { width, height, rgba })
}

/// Each object's node: `Hie` names the object a node draws.
pub fn object_nodes(nodes: &[Node], object_count: usize) -> Vec<Option<usize>> {
    let mut out = vec![None; object_count];
    for (k, node) in nodes.iter().enumerate() {
        if let Some(slot) = out.get_mut(node.object) {
            slot.get_or_insert(k);
        }
    }
    out
}

/// Model-space bounds of all objects at rest, for checks.
pub fn bounds(xpr: &Xpr2) -> Result<(Vec3, Vec3)> {
    let objects = objects(xpr)?;
    let nodes = hierarchy(xpr)?;
    let rest = rest_pose(&nodes);
    let placed = object_nodes(&nodes, objects.len());
    let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
    for (object, node) in objects.iter().zip(placed) {
        let Some(node) = node else { continue };
        for v in object_piece(xpr, object)?.vertices {
            let p = rest[node].transform_point3(v.position.into());
            lo = lo.min(p);
            hi = hi.max(p);
        }
    }
    Ok((lo, hi))
}

/// The characters in the viewer, with their code for the wear archives and the viewer's label.
pub const ROSTER: [(&str, &str); 9] = [
    ("Tina Armstrong", "tn"),
    ("Helena Douglas", "he"),
    ("Lei Fang", "le"),
    ("Lisa", "ls"),
    ("Christie", "cr"),
    ("Kasumi", "ks"),
    ("Ayane", "ay"),
    ("Hitomi", "ht"),
    ("Kokoro", "co"),
];

/// A suit: which wear archive, the entry of its XPR2 (its CAT follows), and a label.
#[derive(Debug, Clone)]
pub struct Suit {
    pub archive: usize,
    pub entry: usize,
    pub label: String,
}

/// The suits of a character in the wear archives, `v<code>_...`. Names repeat, an XPR2 then
/// its CAT, so entries are taken in pairs and numbered within their name.
pub fn suits(archives: &[Vec<String>], code: &str) -> Vec<Suit> {
    let prefix = format!("v{code}_");
    let mut out = Vec::new();
    for (archive, names) in archives.iter().enumerate() {
        let mut entry = 0;
        while entry < names.len() {
            let name = &names[entry];
            if !name.starts_with(&prefix) || names.get(entry + 1) != Some(name) {
                entry += 1;
                continue;
            }
            let number = out.iter().filter(|s: &&Suit| s.label.starts_with(&name[4..])).count() + 1;
            out.push(Suit { archive, entry, label: format!("{} {number}", &name[4..]) });
            entry += 2;
        }
    }
    out
}

/// A piece in a CAT draw list: the game's bone, the object, and the marker that follows the
/// pair, if any (-4 eyes, -5 and -6 the two shapes of a foot, -7 toes).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Placed {
    pub slot: u32,
    pub object: usize,
    pub mark: Option<i32>,
}

/// The CAT's directory, at the offset in word 1: 8-byte entries, `0x0100`, the kind shifted by
/// 8, and an offset, until a zero entry.
fn cat_section(cat: &[u8], kind: u16) -> Option<usize> {
    let mut at = be32(cat, 4).ok()? as usize;
    while at + 8 <= cat.len() {
        let (tag, k, offset) = (be16(cat, at).ok()?, be16(cat, at + 2).ok()?, be32(cat, at + 4).ok()?);
        if tag == 0 {
            return None;
        }
        if k >> 8 == kind {
            return Some(offset as usize);
        }
        at += 8;
    }
    None
}

/// The draw lists of a CAT by body type: section kind 2 holds their offsets, zero for none.
/// Each list is (bone, object) pairs, a negative marker after some pairs, ended by -1. A body
/// and its heads have one list, at their body type: Kasumi and Ayane 0, Lei Fang, Kokoro and
/// Hitomi 1, Helena and Lisa 2, Tina 3, Christie 4. A suit has a list per type, the same
/// pieces fitted to each body.
pub fn draw_lists_by_type(cat: &[u8]) -> Result<Vec<Option<Vec<Placed>>>> {
    if cat.get(..4) != Some(&b"CAT\0"[..]) {
        bail!("not a CAT");
    }
    let table = cat_section(cat, 2).context("no draw list table")?;
    let mut lists = Vec::new();
    // Six entries: in heads the next section follows right after; suits have five lists
    // and zeros.
    for k in 0..6 {
        let offset = be32(cat, table + k * 4)?;
        if offset < 0x18 || offset % 4 != 0 || offset as usize >= cat.len() {
            lists.push(None);
            continue;
        }
        let mut at = offset as usize;
        let mut list: Vec<Placed> = Vec::new();
        loop {
            let value = be32(cat, at)? as i32;
            if value == -1 {
                break;
            }
            if value < 0 {
                if let Some(last) = list.last_mut() {
                    last.mark = Some(value);
                }
                at += 4;
            } else {
                list.push(Placed { slot: value as u32, object: be32(cat, at + 4)? as usize, mark: None });
                at += 8;
            }
            if list.len() > 512 {
                bail!("draw list at {offset:#x} does not end");
            }
        }
        lists.push(Some(list));
    }
    Ok(lists)
}

/// The lists a CAT has, in order of body type.
pub fn draw_lists(cat: &[u8]) -> Result<Vec<Vec<Placed>>> {
    Ok(draw_lists_by_type(cat)?.into_iter().flatten().collect())
}

/// A body CAT's body type: the place of its one list.
pub fn body_type(cat: &[u8]) -> Result<usize> {
    draw_lists_by_type(cat)?.iter().position(Option::is_some).context("no draw list")
}

/// A model file ready to draw: its pieces with their nodes, its textures and rest pose.
pub struct Loaded {
    /// (piece, node) per drawn object; the piece's texture indices are the file's own.
    pub pieces: Vec<(ModelPiece, usize, Placed)>,
    pub textures: Vec<ModelTexture>,
    pub rest: Vec<Mat4>,
    pub nodes: Vec<Node>,
    /// Per piece, each vertex's blend weights; empty lists for pieces on one bone.
    pub weights: Vec<Vec<Vec<f32>>>,
    /// Per piece, its six shapes if it has them.
    pub shapes: Vec<Option<Vec<[[f32; 3]; 6]>>>,
    /// The model's objects, for reading pose sets against.
    pub objects: Vec<Object>,
}

/// `load_for` with the file's own first list.
pub fn load(tpr: &[u8], cat: &[u8]) -> Result<Loaded> {
    load_for(tpr, cat, None)
}

/// The body's neck bone. The body's head node is the neck node's child.
pub const NECK_SLOT: u32 = 20;

impl Loaded {
    /// The node of the first piece on a game bone.
    pub fn slot_node(&self, slot: u32) -> Option<usize> {
        self.pieces.iter().find(|(_, _, placed)| placed.slot == slot).map(|(_, node, _)| *node)
    }

    /// Where a head file's root goes: on the body's head node, the neck's child. The head's
    /// root has no parent in its file and its own offset is not the body's; placed this way
    /// the head's neck edge meets the body's with no gap on all five characters
    /// (`head_meets_neck`), against 2 to 29 mm from the neck with the head's offset.
    pub fn head_frame(&self) -> Option<Mat4> {
        let neck = self.slot_node(NECK_SLOT)?;
        let head = *self.nodes.get(neck)?.children.first()?;
        Some(self.rest[head])
    }
}

/// Reads a model and its CAT's draw list for a body type, or its first if it has none for
/// that type. Of a foot's two shapes, the one on the leg (marker -5) is drawn and the one on
/// the free foot nodes (-6) is left out.
pub fn load_for(tpr: &[u8], cat: &[u8], body: Option<usize>) -> Result<Loaded> {
    let xpr = Xpr2::parse(tpr)?;
    let objects = objects(&xpr)?;
    let nodes = hierarchy(&xpr)?;
    let rest = rest_pose(&nodes);
    let placed_nodes = object_nodes(&nodes, objects.len());
    let lists = draw_lists_by_type(cat)?;
    let list = body
        .and_then(|b| lists.get(b).cloned().flatten())
        .or_else(|| lists.iter().flatten().next().cloned())
        .context("no draw list")?;
    let mut pieces = Vec::new();
    let mut weights = Vec::new();
    let mut shapes = Vec::new();
    for placed in list {
        if placed.mark == Some(-6) {
            continue;
        }
        let (Some(object), Some(Some(node))) = (objects.get(placed.object), placed_nodes.get(placed.object)) else {
            log::warn!("DOAX2: object {} is not in the model", placed.object);
            continue;
        };
        let piece = object_piece(&xpr, object)?;
        shapes.push(object_shapes(&xpr, object, &piece.vertices)?);
        pieces.push((piece, *node, placed));
        weights.push(object_weights(&xpr, object)?);
    }
    let textures = textures(&xpr);
    // Other draws whose texture has some see-through texels are blended too, such as Tina's
    // lash line, a black texture with alpha on a draw of kind 1, which drawn solid looked far
    // too dark (the user's report). The second entry marks them, as for hair.
    let see_through: Vec<bool> = textures
        .iter()
        .map(|t| t.rgba.chunks_exact(4).filter(|p| p[3] < 250).count() * 100 > t.rgba.len() / 4)
        .collect();
    for (piece, _, _) in &mut pieces {
        for material in &mut piece.materials {
            if let [texture] = material.textures[..] {
                if see_through.get(texture).copied().unwrap_or(false) {
                    material.textures.push(texture);
                    if let Some(look) = &mut material.look {
                        look.alpha_from_texture = true;
                    }
                }
            }
        }
    }
    Ok(Loaded { pieces, textures, rest, nodes, weights, shapes, objects })
}

/// The body's node for each DOAX bone: DOAX2 has DOAX's skeleton, the same 24 offsets (Tina's
/// match DOAX's table to the millimetre), in a node tree. Bone 15, DOAX's point on the ground,
/// has no node. Checked on the five bodies by `bone_nodes_follow_the_links`.
pub const BONE_NODES: [Option<usize>; 24] = [
    Some(5), Some(6), Some(0), Some(12), Some(13), Some(14), Some(16), Some(19), Some(20), Some(29), Some(30), Some(31),
    Some(33), Some(36), Some(37), None, Some(9), Some(11), Some(21), Some(23), Some(26), Some(28), Some(38), Some(40),
];

/// The DOAX bone of each CAT bone number. 0 to 15 are DOAX's own; 16 to 23 follow from where
/// the body and head put them: 16 and 17 the eyes, 18 and 21 the collars, 19 and 22 the
/// toes, 20 the neck, 23 the spine.
pub const SLOT_BONES: [usize; 24] = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 17, 21, 16, 18, 19, 20, 22, 23];

/// DOAX's offset rows, from the body's rest nodes.
pub fn bone_rows(body_rest: &[Mat4]) -> Result<[Vec3; 24]> {
    let position = |bone: usize| -> Result<Vec3> {
        let node = BONE_NODES[bone].with_context(|| format!("bone {bone} has no node"))?;
        Ok(body_rest.get(node).context("body has too few nodes")?.w_axis.truncate())
    };
    let mut rows = [Vec3::ZERO; 24];
    for (bone, parent, row) in super::doax::BONE_LINKS {
        rows[row] = position(bone)? - position(parent)?;
    }
    Ok(rows)
}

/// Each DOAX bone at rest with the hips at the origin, as translations.
pub fn bone_rest(rows: &[Vec3; 24]) -> [Mat4; super::doax::BONE_COUNT] {
    let mut positions = [Vec3::ZERO; super::doax::BONE_COUNT];
    for (bone, parent, row) in super::doax::BONE_LINKS {
        positions[bone] = positions[parent] + rows[row];
    }
    for (bone, base) in super::doax::BONES_ON_BONES {
        positions[bone] = positions[base];
    }
    positions.map(Mat4::from_translation)
}

/// The DOAX bones of a blended piece's weight groups, by its CAT number and group count.
///
/// Inferred from the bodies, not read from the game: group 0 is the piece's own bone; a group
/// whose vertices meet a rigid piece at rest takes that piece's bone (`blend_groups_from_seams`,
/// clear by 6 mm or more over 80 frames); the rest are DOAX's groups for the same place
/// (`0x33F988` in DOAX: neck 19, 1, 0; spine 23, 2, 0; shoulder 30, 0, 19, 16). The forearms'
/// groups lie at the elbow (0), the wrist (1) and the middle (2, 8 to 16 cm from the elbow):
/// the middle takes DOAX's forearm twist bone, 24 or 25, which sits on the elbow. Taking the
/// shoulder there, as the least stretch suggested, sank the forearm below the elbow. Suits use
/// only combinations the bodies have.
pub fn blend_bones(slot: u32, groups: usize) -> Option<&'static [usize]> {
    Some(match (slot, groups) {
        (3, 2) => &[3, 7],
        (4, 2) => &[4, 8],
        (4, 3) => &[4, 5, 24],
        (5, 2) => &[5, 4],
        (6, 2) => &[6, 2],
        (7, 2) => &[7, 6],
        (9, 2) => &[9, 13],
        (10, 2) => &[10, 14],
        (10, 3) => &[10, 11, 25],
        (11, 2) => &[11, 10],
        (12, 2) => &[12, 2],
        (13, 2) => &[13, 12],
        (18, 4) => &[16, 8, 0, 30],
        (19, 2) => &[18, 3],
        (20, 3) => &[1, 0, 19],
        (21, 4) => &[20, 14, 0, 31],
        (22, 2) => &[22, 9],
        (23, 3) => &[0, 2, 23],
        _ => return None,
    })
}

/// Argument counts of the 96 script operations, from the table at `0x82BDDC78` in the game's
/// image (6 bytes an operation: number, count, two more bytes), the same layout as DOAX's.
const SCRIPT_OP_ARGUMENTS: [u8; 96] = [
    2, 1, 1, 0, 0, 0, 1, 1, 0, 1, 1, 0, 2, 3, 1, 2, 1, 1, 1, 1, 1, 2, 1, 1, 1, 2, 2, 1, 1, 1, 1, 1, 1, 1, 0, 0, 1, 1, 1, 1, 0, 1,
    1, 1, 1, 2, 1, 0, 0, 1, 1, 0, 0, 0, 1, 1, 1, 1, 1, 1, 0, 1, 1, 1, 1, 1, 0, 3, 0, 1, 0, 0, 1, 0, 1, 1, 0, 0, 1, 2, 2, 1, 1, 1,
    2, 1, 0, 1, 0, 2, 1, 1, 0, 1, 2, 2,
];

/// The hand events of each motion in a script file (`<name>.bin`, `anime_cm.bin`): DOAX's
/// scripts, big-endian. Word 0 is the offset of a table of script offsets; a script is 16-bit
/// words, an operation and its arguments, up to 0xFF. 0 plays a motion (its first argument),
/// 1 waits for a frame, 12 sets a hand's pose and 13 does so over some frames. Hand 2 is both.
/// Motions of the shared files are their number plus 0x2000 in `anime_cm.bin`.
pub fn hand_events(bin: &[u8]) -> std::collections::HashMap<u16, Vec<super::doax::HandEvent>> {
    let mut events: std::collections::HashMap<u16, Vec<super::doax::HandEvent>> = std::collections::HashMap::new();
    let Ok(table) = be32(bin, 0) else { return events };
    let mut starts: Vec<usize> = (0..8000).filter_map(|i| be32(bin, table as usize + i * 4).ok()).map(|v| v as usize).filter(|&v| v < bin.len()).collect();
    starts.sort_unstable();
    starts.dedup();
    for start in starts {
        let (mut at, mut motion, mut frame) = (start, None, 0u16);
        for _ in 0..1000 {
            let Ok(op) = be16(bin, at) else { break };
            let Some(&count) = SCRIPT_OP_ARGUMENTS.get(op as usize) else { break };
            let argument = |i: usize| be16(bin, at + 2 + i * 2).ok();
            match op {
                0 => {
                    // The first script to play a motion decides its events; others that play
                    // it again add none.
                    motion = argument(0).filter(|m| !events.contains_key(m));
                    frame = 0;
                    if let Some(motion) = motion {
                        events.entry(motion).or_default();
                    }
                }
                1 => frame = argument(0).unwrap_or(frame),
                12 | 13 => {
                    if let (Some(motion), Some(hand), Some(pose)) = (motion, argument(0), argument(1)) {
                        let time = if op == 13 { argument(2).unwrap_or(0) } else { 0 };
                        events.entry(motion).or_default().push(super::doax::HandEvent { frame, hand, pose, time });
                    }
                }
                _ => {}
            }
            at += 2 + count as usize * 2;
        }
    }
    events
}

/// The blinks of each motion in a script file: op 22's argument at its frame, the eye shape
/// to close to (entry 2, eyes shut, or 3, shut smiling, of the head's three-entry face set).
/// Inferred: op 22 comes every 20 to 30 frames, mostly with 2 or 3, as blinks do; not compared
/// with the game.
pub fn eye_events(bin: &[u8]) -> std::collections::HashMap<u16, Vec<(u16, u16)>> {
    let mut events: std::collections::HashMap<u16, Vec<(u16, u16)>> = std::collections::HashMap::new();
    let Ok(table) = be32(bin, 0) else { return events };
    let mut starts: Vec<usize> = (0..8000).filter_map(|i| be32(bin, table as usize + i * 4).ok()).map(|v| v as usize).filter(|&v| v < bin.len()).collect();
    starts.sort_unstable();
    starts.dedup();
    for start in starts {
        let (mut at, mut motion, mut frame) = (start, None, 0u16);
        for _ in 0..1000 {
            let Ok(op) = be16(bin, at) else { break };
            let Some(&count) = SCRIPT_OP_ARGUMENTS.get(op as usize) else { break };
            let argument = |i: usize| be16(bin, at + 2 + i * 2).ok();
            match op {
                0 => {
                    // The first script to play a motion decides its events; others that play
                    // it again add none.
                    motion = argument(0).filter(|m| !events.contains_key(m));
                    frame = 0;
                    if let Some(motion) = motion {
                        events.entry(motion).or_default();
                    }
                }
                1 => frame = argument(0).unwrap_or(frame),
                22 => {
                    if let (Some(motion), Some(shape)) = (motion, argument(0)) {
                        events.entry(motion).or_default().push((frame, shape));
                    }
                }
                _ => {}
            }
            at += 2 + count as usize * 2;
        }
    }
    events
}

/// How far a blink has closed the eyes, `t` frames after it starts: shut over 3 frames, held
/// for 2, open over 4. The game's timing is not read.
pub fn blink_weight(t: f32) -> f32 {
    match t {
        t if t < 0.0 => 0.0,
        t if t < 3.0 => t / 3.0,
        t if t < 5.0 => 1.0,
        t if t < 9.0 => 1.0 - (t - 5.0) / 4.0,
        _ => 0.0,
    }
}

/// A hand's pose file block (`MPP`): its object in the body, its vertex groups and, per pose
/// number, each group's position.
#[derive(Debug, Clone)]
pub struct HandPoses {
    pub object: usize,
    pub groups: Vec<Vec<usize>>,
    pub poses: Vec<Option<Vec<[f32; 3]>>>,
}

/// The pose sets of an `MPV` block: a `hand_*.bin`'s two hands, or a head CAT's face shapes
/// (from the offset in its word 3). Each `MPP` block: +0xC its object, +0x14 the pose count,
/// then the poses' offsets from the block. Pose 0 is the vertex list: entries with the vertex
/// layout in bits 24 to 30 and a byte offset in that layout below, a set bit 31 starting each
/// group of copies, ended by -1. Every other pose is 24 bytes a group, position then normal, or
/// 12, the position alone (face sets).
/// `objects` gives the layouts' strides, to number vertices as `object_piece` does.
pub fn hand_poses(file: &[u8], objects: &[Object]) -> Result<Vec<HandPoses>> {
    let mut hands = Vec::new();
    let mut at = 0;
    while at + 4 <= file.len() {
        if &file[at..at + 4] != b"MPP\0" {
            at += 4;
            continue;
        }
        let object = be32(file, at + 0xC)? as usize;
        let count = be32(file, at + 0x14)? as usize;
        // Each layout's first vertex and stride.
        let layouts: Vec<(usize, usize)> = objects
            .get(object)
            .map(|o| {
                let mut first = 0;
                o.decls
                    .iter()
                    .map(|d| {
                        let entry = (first, d.streams.first().map_or(32, |s| s.1.max(1)));
                        first += d.vertex_count;
                        entry
                    })
                    .collect()
            })
            .unwrap_or_default();
        let offsets = (0..count).map(|i| Ok(be32(file, at + 0x18 + i * 4)? as usize)).collect::<Result<Vec<_>>>()?;
        let mut groups: Vec<Vec<usize>> = Vec::new();
        let mut p = at + *offsets.first().context("hand block without poses")?;
        loop {
            let value = be32(file, p)?;
            p += 4;
            if value == u32::MAX {
                break;
            }
            if value & 0x8000_0000 != 0 || groups.is_empty() {
                groups.push(Vec::new());
            }
            let layout = ((value >> 24) & 0x7F) as usize;
            let (first, stride) = layouts.get(layout).copied().unwrap_or((0, 32));
            groups.last_mut().unwrap().push(first + (value & 0xFF_FFFF) as usize / stride);
        }
        // 24 bytes a group (position and normal) in the hands' files, 12 (position) in the
        // heads' face sets: the smallest gap between poses tells.
        let mut distinct: Vec<usize> = offsets.iter().skip(1).copied().collect();
        distinct.sort_unstable();
        distinct.dedup();
        let gap = distinct.windows(2).map(|w| w[1] - w[0]).min().unwrap_or(usize::MAX);
        let size = if groups.is_empty() || gap >= groups.len() * 24 { 24 } else { 12 };
        let poses = offsets
            .iter()
            .enumerate()
            .map(|(i, &offset)| {
                if i == 0 {
                    return Ok(None);
                }
                (0..groups.len())
                    .map(|g| {
                        let q = at + offset + g * size;
                        Ok([bef32(file, q)?, bef32(file, q + 4)?, bef32(file, q + 8)?])
                    })
                    .collect::<Result<Vec<_>>>()
                    .map(Some)
            })
            .collect::<Result<Vec<_>>>()?;
        hands.push(HandPoses { object, groups, poses });
        at += 4;
    }
    Ok(hands)
}

/// The hand file entry a script's pose number takes: pose p is entry p + 1. Inferred, not read
/// from the game: entry 1 is the base mesh, as pose 0 is in DOAX, and entries 2 to 13 are the
/// first ones with their own data, matching poses 1 to 12 in the scripts. In a jet ski race both
/// hands were on entry 13 exactly.
pub fn hand_entry(pose: u16) -> usize {
    pose as usize + 1
}

/// DOAX2's light sets for the viewer, read from the running game while a match played: the
/// three sets of the shaders' constants (`AmbientLightP1` c68, `DirLightVec` c71-74,
/// `DirLightCol` c75-78), found together in the heap. Directions as stored, towards the light. The ambient is `AmbientLightP1`;
/// `P2`, a second tone, is left out. Which set is which scene is not known.
pub fn stage_lights() -> Vec<(String, crate::renderer::mesh::SceneLights)> {
    let set = |ambient: [f32; 3], lights: [([f32; 3], [f32; 3]); 4]| crate::renderer::mesh::SceneLights {
        ambient,
        // Directions as stored: the way the light travels in Direct3D's camera space (see
        // `set_lights_follow_camera`). The strongest comes down and away from the camera.
        lights: lights.map(|(direction, diffuse)| (direction, diffuse, diffuse.map(|c| c * 0.3))),
    };
    vec![
        ("Match, set 1".to_string(), set([0.33, 0.32, 0.3], [
            ([-0.031, -0.537, 0.843], [1.4, 1.15, 0.8]),
            ([0.532, 0.485, 0.694], [0.75, 0.7, 0.55]),
            ([-0.592, 0.407, -0.696], [0.71, 0.61, 0.65]),
            ([0.0, -1.0, 0.0], [0.0, 0.0, 0.0]),
        ])),
        ("Match, set 2".to_string(), set([0.4, 0.36, 0.3], [
            ([0.02, -0.284, 0.959], [1.55, 1.15, 0.9]),
            ([0.001, -0.059, -0.998], [0.4, 0.5, 0.65]),
            ([-0.991, -0.079, -0.107], [0.8, 0.6, 0.4]),
            ([0.981, -0.179, 0.077], [0.7, 0.66, 0.5]),
        ])),
        ("Match, set 3".to_string(), set([0.38, 0.38, 0.4], [
            ([-0.949, -0.296, 0.113], [1.45, 1.34, 1.3]),
            ([0.992, -0.038, -0.124], [0.7, 0.8, 0.95]),
            ([-0.025, -0.105, -0.994], [0.65, 0.6, 0.5]),
            ([0.103, 0.028, 0.994], [0.74, 0.71, 0.53]),
        ])),
    ]
}

/// The name a character's motions go by.
pub fn motion_name(code: &str) -> Option<&'static str> {
    Some(match code {
        "tn" => "tina",
        "he" => "hika",
        "le" => "leifang",
        "ls" => "lisa",
        "cr" => "gokei",
        "ay" => "ayane",
        "ko" => "kokoro",
        "hi" => "hitomi",
        "ka" => "kasumi",
        _ => return None,
    })
}

/// A character's motion files: her own, her bed and gravure ones, and the shared `comon` ones.
pub fn motion_files(names: &[String], code: &str) -> Vec<String> {
    let Some(name) = motion_name(code) else { return Vec::new() };
    let own = |n: &str| n == format!("{name}.mot") || n == format!("bed_{name}.mot") || (n.starts_with(&format!("gr_{name}")) && n.ends_with(".mot"));
    let mut files: Vec<String> = names.iter().filter(|n| own(n)).cloned().collect();
    files.extend(names.iter().filter(|n| n.starts_with("comon") && n.ends_with(".mot")).cloned());
    files.dedup();
    files
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::formats::doax::LazyArchive;

    #[test]
    fn tiled_offsets_cover_a_surface_once() {
        let (w, h) = (64u32, 64u32);
        let mut seen = std::collections::HashSet::new();
        for y in 0..h {
            for x in 0..w {
                assert!(seen.insert(tiled_offset(x, y, w, 2)));
            }
        }
    }

    #[test]
    #[ignore]
    fn draw_lists_read() {
        let chr = LazyArchive::open("assets/doax2/chr.afs").unwrap();
        for name in ["vtn1.cat", "vtnAA.cat"] {
            println!("{name}: {:?}", draw_lists(&chr.read(name).unwrap()).unwrap());
        }
        let wear: Vec<LazyArchive> = ["werone", "wertwo"].iter().map(|n| LazyArchive::open(format!("assets/doax2/{n}.afs")).unwrap()).collect();
        let names: Vec<Vec<String>> = wear.iter().map(|w| w.names().map(str::to_string).collect()).collect();
        for (_, code) in ROSTER {
            let suits = suits(&names, code);
            println!("{code}: {} suits, {:?}", suits.len(), suits.iter().take(6).map(|s| &s.label).collect::<Vec<_>>());
            for suit in suits.iter().take(2) {
                let lists = draw_lists(&wear[suit.archive].read_index(suit.entry + 1).unwrap()).unwrap();
                println!("  {}: {} lists, first {:?}", suit.label, lists.len(), lists.first());
            }
        }
        let loaded = load(&chr.read("vtn1.tpr").unwrap(), &chr.read("vtn1.cat").unwrap()).unwrap();
        println!("vtn1: {} pieces", loaded.pieces.len());
        let file = chr.read("vtnAA.tpr").unwrap();
        let xpr = Xpr2::parse(&file).unwrap();
        let nodes = hierarchy(&xpr).unwrap();
        let rest = rest_pose(&nodes);
        for (k, node) in nodes.iter().enumerate() {
            println!("head node {k}: object {} parent {:?} children {:?} at {:?}", node.object, node.parent, node.children, rest[k].w_axis);
        }
        let loaded = load(&file, &chr.read("vtnAA.cat").unwrap()).unwrap();
        for (piece, node, placed) in &loaded.pieces {
            let (mut lo, mut hi) = (Vec3::splat(9.0), Vec3::splat(-9.0));
            for v in &piece.vertices {
                lo = lo.min(v.position.into());
                hi = hi.max(v.position.into());
            }
            println!("head piece {placed:?} node {node}: {} vertices, local {lo:?} {hi:?}, {} draws", piece.vertices.len(), piece.materials.len());
        }
    }

    #[test]
    #[ignore]
    fn head_meets_neck() {
        let chr = LazyArchive::open("assets/doax2/chr.afs").unwrap();
        for (_, code) in ROSTER {
            let body = load(&chr.read(&format!("v{code}1.tpr")).unwrap(), &chr.read(&format!("v{code}1.cat")).unwrap()).unwrap();
            let head = load(&chr.read(&format!("v{code}AA.tpr")).unwrap(), &chr.read(&format!("v{code}AA.cat")).unwrap()).unwrap();
            let neck = body.slot_node(NECK_SLOT).unwrap();
            let points: Vec<Vec3> = body.pieces.iter().filter(|p| p.1 == neck).flat_map(|(piece, node, _)| piece.vertices.iter().map(|v| body.rest[*node].transform_point3(v.position.into()))).collect();
            let top = points.iter().map(|p| p.y).fold(f32::MIN, f32::max);
            let (piece, node, _) = &head.pieces[0];
            let m = body.head_frame().unwrap() * head.rest[0].inverse() * head.rest[*node];
            let head_points: Vec<Vec3> = piece.vertices.iter().map(|v| m.transform_point3(v.position.into())).collect();
            for rim in points.iter().filter(|p| p.y > top - 0.005) {
                let gap = head_points.iter().map(|p| p.distance(*rim)).fold(f32::MAX, f32::min);
                assert!(gap < 1e-4, "{code}: gap {gap}");
            }
        }
    }

    #[test]
    #[ignore]
    fn bone_nodes_follow_the_links() {
        let chr = LazyArchive::open("assets/doax2/chr.afs").unwrap();
        for (_, code) in ROSTER {
            let file = chr.read(&format!("v{code}1.tpr")).unwrap();
            let nodes = hierarchy(&Xpr2::parse(&file).unwrap()).unwrap();
            for (bone, parent, _) in crate::formats::doax::BONE_LINKS {
                // The bone's node descends from its parent's, through nodes at the same place.
                let (node, parent_node) = (BONE_NODES[bone].unwrap(), BONE_NODES[parent].unwrap());
                let mut up = nodes[node].parent;
                let mut steps = 0;
                while up != Some(parent_node) && steps < 4 {
                    up = up.and_then(|u| nodes[u].parent);
                    steps += 1;
                }
                assert_eq!(up, Some(parent_node), "{code}: bone {bone} node {node} is not under {parent_node}");
            }
            let rows = bone_rows(&rest_pose(&nodes)).unwrap();
            println!("{code}: shin {:.3}, upper arm {:.3}", rows[3].length(), rows[0].length());
        }
        let mot = LazyArchive::open("assets/doax2/mot.afs").unwrap();
        let names: Vec<String> = mot.names().map(str::to_string).collect();
        for (_, code) in ROSTER {
            let files = motion_files(&names, code);
            let count: usize = files.iter().map(|f| crate::formats::doax::list_motions_big(&mot.read(f).unwrap()).len()).sum();
            println!("{code}: {} files, {count} motions", files.len());
        }
    }

    /// For each weight group of the body's blended pieces, the DOAX bone that keeps its seam
    /// vertices on the rigid pieces they touch at rest, over many motion frames.
    #[test]
    #[ignore]
    fn blend_groups_from_seams() {
        use crate::formats::doax;
        let chr = LazyArchive::open("assets/doax2/chr.afs").unwrap();
        let mot = LazyArchive::open("assets/doax2/mot.afs").unwrap();
        let code = std::env::var("DOAX2_CODE").unwrap_or("tn".into());
        let tpr = chr.read(&format!("v{code}1.tpr")).unwrap();
        let body = load(&tpr, &chr.read(&format!("v{code}1.cat")).unwrap()).unwrap();
        let xpr = Xpr2::parse(&tpr).unwrap();
        let objects = objects(&xpr).unwrap();
        let rows = bone_rows(&body.rest).unwrap();
        let rest_bones = bone_rest(&rows);
        let pelvis = body.rest[0].w_axis.truncate();
        let rest_of = |node: usize| Mat4::from_translation(-pelvis) * body.rest[node];
        // Frames from the character's own motions.
        let data = mot.read(&format!("{}.mot", motion_name(&code).unwrap())).unwrap();
        let mut frames = Vec::new();
        for motion in doax::list_motions_big(&data).iter().take(40) {
            for f in [0, motion.frames / 2] {
                let values = doax::eval_motion_endian(&data, motion, f as f32, true).unwrap();
                frames.push(doax::solve_pose(&values, &rows));
            }
        }
        // Rigid pieces' vertices at rest and posed.
        let mut rigid: Vec<(Vec3, usize, Vec3)> = Vec::new(); // rest position, bone, local to bone
        for (piece, node, placed) in &body.pieces {
            if !object_weights(&xpr, &objects[placed.object]).unwrap().iter().all(|w| w.is_empty()) {
                continue;
            }
            let bone = SLOT_BONES[placed.slot as usize];
            for v in &piece.vertices {
                let p = rest_of(*node).transform_point3(v.position.into());
                rigid.push((p, bone, rest_bones[bone].inverse().transform_point3(p)));
            }
        }
        // The head file's pieces, on the body's head node, follow bone 1.
        let head = load(&chr.read(&format!("v{code}AA.tpr")).unwrap(), &chr.read(&format!("v{code}AA.cat")).unwrap()).unwrap();
        let head_place = Mat4::from_translation(-pelvis) * body.head_frame().unwrap() * head.rest[0].inverse();
        for (piece, node, placed) in &head.pieces {
            let bone = SLOT_BONES[placed.slot as usize];
            for v in &piece.vertices {
                let p = (head_place * head.rest[*node]).transform_point3(v.position.into());
                rigid.push((p, bone, rest_bones[bone].inverse().transform_point3(p)));
            }
        }
        for (piece, node, placed) in &body.pieces {
            let weights = object_weights(&xpr, &objects[placed.object]).unwrap();
            let groups = weights.first().map_or(0, |w| w.len());
            if groups == 0 {
                continue;
            }
            println!("object {} slot {} node {node}: {groups} groups", placed.object, placed.slot);
            let mut chosen: Vec<Option<usize>> = vec![None; groups];
            for g in 0..groups {
                // Seam vertices dominated by this group, with their rigid partners.
                let mut pairs = Vec::new();
                for (v, w) in piece.vertices.iter().zip(&weights) {
                    if w[g] < 0.95 {
                        continue;
                    }
                    let p = rest_of(*node).transform_point3(v.position.into());
                    if let Some(partner) = rigid.iter().find(|r| r.0.distance(p) < 0.001) {
                        pairs.push((p, partner.1, partner.2));
                    }
                }
                let dominant = weights.iter().filter(|w| w[g] >= 0.95).count();
                if pairs.is_empty() {
                    println!("  group {g}: {dominant} dominant vertices, no seam");
                    continue;
                }
                let mut scores: Vec<(f32, usize)> = (0..doax::BONE_COUNT)
                    .map(|b| {
                        let mut total = 0.0;
                        for bones in &frames {
                            for (p, partner_bone, partner_local) in &pairs {
                                let mine = bones[b] * rest_bones[b].inverse();
                                total += mine.transform_point3(*p).distance(bones[*partner_bone].transform_point3(*partner_local));
                            }
                        }
                        (total / (frames.len() * pairs.len()) as f32, b)
                    })
                    .collect();
                scores.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
                let partners: std::collections::BTreeSet<usize> = pairs.iter().map(|p| p.1).collect();
                println!("  group {g}: {dominant} dominant, {} seam pairs on bones {partners:?}, best {:?}", pairs.len(), &scores[..3].iter().map(|(s, b)| (*b, (s * 1000.0).round() / 1000.0)).collect::<Vec<_>>());
                chosen[g] = Some(scores[0].1);
            }
            // Group 0 is the piece's own bone when no seam says otherwise.
            if chosen[0].is_none() {
                chosen[0] = Some(SLOT_BONES[placed.slot as usize]);
            }
            // The rest: the bone that stretches the piece's edges least, in turn.
            let rest_points: Vec<Vec3> = piece.vertices.iter().map(|v| rest_of(*node).transform_point3(v.position.into())).collect();
            let edges: Vec<(u32, u32)> = piece.materials.iter().flat_map(|m| m.indices.chunks_exact(3).flat_map(|t| [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])])).collect();
            let stretch = |assign: &[usize]| -> f32 {
                let mut total = 0.0;
                for bones in frames.iter().step_by(3) {
                    let posed: Vec<Vec3> = rest_points
                        .iter()
                        .zip(&weights)
                        .map(|(p, w)| w.iter().zip(assign).map(|(&wt, &b)| (bones[b] * rest_bones[b].inverse()).transform_point3(*p) * wt).sum())
                        .collect();
                    for &(a, b) in &edges {
                        let rest = rest_points[a as usize].distance(rest_points[b as usize]).max(1e-4);
                        total += (posed[a as usize].distance(posed[b as usize]) - rest).abs() / rest;
                    }
                }
                total
            };
            for g in 0..groups {
                if chosen[g].is_some() {
                    continue;
                }
                let base: Vec<usize> = chosen.iter().map(|c| c.unwrap_or(SLOT_BONES[placed.slot as usize])).collect();
                // Bones that move as one already chosen would make the group pointless.
                let same = |a: usize, b: usize| frames.iter().all(|bones| bones[a].abs_diff_eq(bones[b], 1e-4));
                let mut scores: Vec<(f32, usize)> = (0..doax::BONE_COUNT)
                    .filter(|&b| b != 15 && !chosen.iter().flatten().any(|&c| same(c, b)))
                    .map(|b| {
                        let mut assign = base.clone();
                        assign[g] = b;
                        (stretch(&assign), b)
                    })
                    .collect();
                scores.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
                println!("  group {g} by stretch: best {:?}", &scores[..5].iter().map(|(s, b)| (*b, s.round())).collect::<Vec<_>>());
                chosen[g] = Some(scores[0].1);
            }
            println!("  => {:?}", chosen.iter().map(|c| c.unwrap()).collect::<Vec<_>>());
        }
    }

    #[test]
    #[ignore]
    fn suit_weight_groups() {
        let wear: Vec<LazyArchive> = ["werone", "wertwo"].iter().map(|n| LazyArchive::open(format!("assets/doax2/{n}.afs")).unwrap()).collect();
        let names: Vec<Vec<String>> = wear.iter().map(|w| w.names().map(str::to_string).collect()).collect();
        let mut seen = std::collections::BTreeMap::new();
        for (_, code) in ROSTER {
            for suit in suits(&names, code) {
                let tpr = wear[suit.archive].read_index(suit.entry).unwrap();
                let cat = wear[suit.archive].read_index(suit.entry + 1).unwrap();
                let xpr = Xpr2::parse(&tpr).unwrap();
                let objects = objects(&xpr).unwrap();
                for list in draw_lists(&cat).unwrap() {
                    for placed in list {
                        let groups = object_weights(&xpr, &objects[placed.object]).unwrap().first().map_or(0, |w| w.len());
                        *seen.entry((placed.slot, groups)).or_insert(0) += 1;
                    }
                }
            }
        }
        println!("(slot, groups): pieces {seen:?}");
        for index in [20usize, 21, 22] {
            let suit = &suits(&names, "tn")[index];
            let tpr = wear[suit.archive].read_index(suit.entry).unwrap();
            let cat = wear[suit.archive].read_index(suit.entry + 1).unwrap();
            let xpr = Xpr2::parse(&tpr).unwrap();
            let objects = objects(&xpr).unwrap();
            for (k, list) in draw_lists(&cat).unwrap().iter().enumerate() {
                let summary: Vec<(u32, usize)> = list.iter().map(|p| (p.slot, objects[p.object].vertex_count())).collect();
                println!("suit {index} {} list {k}: {summary:?}", suit.label);
            }
        }
        let suit = &suits(&names, "tn")[2];
        let tpr = wear[suit.archive].read_index(suit.entry).unwrap();
        let cat = wear[suit.archive].read_index(suit.entry + 1).unwrap();
        let loaded = load(&tpr, &cat).unwrap();
        for ((piece, node, placed), weights) in loaded.pieces.iter().zip(&loaded.weights) {
            if weights.first().map_or(true, |w| w.is_empty()) {
                continue;
            }
            let sums: Vec<f32> = weights.iter().map(|w| w.iter().sum()).collect();
            let bad = weights.iter().filter(|w| w.iter().any(|x| !x.is_finite() || *x < -0.01 || *x > 1.01)).count();
            let lo = sums.iter().cloned().fold(f32::MAX, f32::min);
            let hi = sums.iter().cloned().fold(f32::MIN, f32::max);
            for w in weights.iter().filter(|w| w.iter().sum::<f32>() > 1.01).take(4) {
                println!("   odd weights {w:?}, first ones add up to {}", w[..w.len() - 1].iter().sum::<f32>());
            }
            let partial_bad = weights.iter().filter(|w| { let s: f32 = w[..w.len() - 1].iter().sum(); !(-0.01..=1.01).contains(&s) }).count();
            println!("   with the last implied: {partial_bad} out of range");
            println!("{} {placed:?} node {node}: {} vertices, sums {lo:.3}..{hi:.3}, {bad} odd, layouts {}", suit.label, piece.vertices.len(), Xpr2::parse(&tpr).map(|x| objects(&x).unwrap()[placed.object].decls.len()).unwrap());
        }
    }

    #[test]
    #[ignore]
    fn hands_and_scripts_read() {
        let chr = LazyArchive::open("assets/doax2/chr.afs").unwrap();
        let mot = LazyArchive::open("assets/doax2/mot.afs").unwrap();
        let body_file = chr.read("vtn1.tpr").unwrap();
        let body_objects = objects(&Xpr2::parse(&body_file).unwrap()).unwrap();
        let hands = hand_poses(&chr.read("hand_vacance.bin").unwrap(), &body_objects).unwrap();
        for hand in &hands {
            let own = hand.poses.iter().filter(|p| p.is_some()).count();
            println!("hand object {}: {} groups, {own} poses", hand.object, hand.groups.len());
        }
        assert_eq!(hands.iter().map(|h| h.object).collect::<Vec<_>>(), vec![31, 12]);
        let body = chr.read("vtn1.tpr").unwrap();
        let xpr = Xpr2::parse(&body).unwrap();
        let objects = objects(&xpr).unwrap();
        for hand in &hands {
            let piece = object_piece(&xpr, &objects[hand.object]).unwrap();
            let base = hand.poses[1].as_ref().unwrap();
            for (group, position) in hand.groups.iter().zip(base) {
                for &v in group {
                    let d = Vec3::from(piece.vertices[v].position).distance(Vec3::from(*position));
                    assert!(d < 1e-5, "object {} vertex {v}: {d}", hand.object);
                }
            }
        }
        for name in ["tina.bin", "anime_cm.bin"] {
            let events = hand_events(&mot.read(name).unwrap());
            let count: usize = events.values().map(Vec::len).sum();
            println!("{name}: {} motions, {count} hand events", events.len());
        }
    }

    #[test]
    #[ignore]
    fn heads_differ() {
        let chr = LazyArchive::open("assets/doax2/chr.afs").unwrap();
        let names: Vec<String> = chr.names().map(str::to_string).collect();
        for (_, code) in ROSTER {
            for name in names.iter().filter(|n| n.starts_with(&format!("v{code}")) && n.ends_with(".tpr") && n.len() == 9) {
                let file = chr.read(name).unwrap();
                let xpr = Xpr2::parse(&file).unwrap();
                let objects = objects(&xpr).unwrap();
                let textures = textures(&xpr);
                println!("{name}: objects {:?}, textures {:?}", objects.iter().map(Object::vertex_count).collect::<Vec<_>>(), textures.iter().map(|t| (t.width, t.height)).collect::<Vec<_>>());
                if name == "vleAA.tpr" {
                    for (i, (t, at)) in textures.iter().zip(xpr.of_kind(b"TX2D")).enumerate() {
                        let w1 = be32(xpr.file, at + 32).unwrap();
                        let alpha: Vec<u8> = t.rgba.chunks_exact(4).map(|p| p[3]).collect();
                        let below = alpha.iter().filter(|&&a| a < 26).count();
                        println!("  texture {i}: {}x{} format {:#x} endian {} alpha {}..{}, {below} below 0.1", t.width, t.height, w1 & 0x3F, (w1 >> 6) & 3, alpha.iter().min().unwrap(), alpha.iter().max().unwrap());
                    }
                    let piece = object_piece(&xpr, &objects[0]).unwrap();
                    let weights = object_weights(&xpr, &objects[0]).unwrap();
                    let mut first = 0;
                    for (d, decl) in objects[0].decls.iter().enumerate() {
                        let vs = &piece.vertices[first..first + decl.vertex_count];
                        let (lo, hi) = vs.iter().fold((Vec3::splat(9.0), Vec3::splat(-9.0)), |(l, h), v| (l.min(v.position.into()), h.max(v.position.into())));
                        let w = &weights[first];
                        println!("  layout {d}: {} vertices, streams {:?}, weights {}, box {lo:?} {hi:?}, usages {:?}", decl.vertex_count, decl.streams, w.len(), decl.elements.iter().map(|e| (e.stream, e.usage, e.usage_index)).collect::<Vec<_>>());
                        first += decl.vertex_count;
                    }
                    for (k, draw) in objects[0].draws.iter().enumerate() {
                        println!("  draw {k}: layout {}, indices {}, stages {:?}", draw.decl, draw.index_count, draw.stages);
                    }
                }
            }
        }
    }

    /// Finds the motion and frame that best match bone arrays read from the running game
    /// (`target/captures/doax2_live.txt`: address, sample, then 32 matrices of 16 numbers, rows
    /// of column-vector matrices). Positions are compared from the hips, in the frame of bone
    /// 15, the character's root.
    #[test]
    #[ignore]
    fn live_poses_match_motions() {
        use crate::formats::doax;
        let text = std::fs::read_to_string("target/captures/doax2_live.txt").unwrap();
        let chr = LazyArchive::open("assets/doax2/chr.afs").unwrap();
        let mot = LazyArchive::open("assets/doax2/mot.afs").unwrap();
        let names: Vec<String> = mot.names().map(str::to_string).collect();
        let codes: Vec<String> = std::env::var("DOAX2_CODES").map(|c| c.split(',').map(str::to_string).collect()).unwrap_or(vec!["tn".into(), "he".into(), "ls".into()]);
        let flip = Mat4::from_scale(Vec3::new(1.0, 1.0, -1.0));
        let compared: Vec<usize> = (0..24).filter(|&b| b != 15).collect();
        // Per character: (file, motion, frame, positions from the hips).
        let mut candidates = Vec::new();
        for code in &codes {
            let body = load(&chr.read(&format!("v{code}1.tpr")).unwrap(), &chr.read(&format!("v{code}1.cat")).unwrap()).unwrap();
            let rows = bone_rows(&body.rest).unwrap();
            for file in motion_files(&names, code) {
                let data = mot.read(&file).unwrap();
                for motion in doax::list_motions_big(&data) {
                    for frame in 0..motion.frames.max(1) {
                        let Ok(values) = doax::eval_motion_endian(&data, &motion, frame as f32, true) else { continue };
                        let bones = doax::solve_pose(&values, &rows).map(|b| flip * b);
                        let hips = bones[2].w_axis.truncate();
                        // DOAX bones by CAT number, as the game's arrays hold them.
                        let by_slot: Vec<Vec3> = SLOT_BONES.iter().map(|&b| bones[b].w_axis.truncate() - hips).collect();
                        candidates.push((code.clone(), file.clone(), motion.id, frame, by_slot));
                    }
                }
            }
        }
        println!("{} candidate frames", candidates.len());
        for line in text.lines() {
            let parts: Vec<&str> = line.split_whitespace().collect();
            let numbers: Vec<f32> = parts[2..].iter().map(|v| v.parse().unwrap()).collect();
            let matrices: Vec<Mat4> = numbers.chunks_exact(16).map(|m| Mat4::from_cols_array(m.try_into().unwrap()).transpose()).collect();
            let root = matrices[15];
            let hips = matrices[2].w_axis.truncate();
            let live: Vec<Vec3> = matrices[..24].iter().map(|m| root.inverse().transform_vector3(m.w_axis.truncate() - hips)).collect();
            let mut best = (f32::MAX, String::new());
            for (code, file, id, frame, by_slot) in &candidates {
                let error = compared.iter().map(|&b| by_slot[b].distance(live[b])).fold(0f32, f32::max);
                if error < best.0 {
                    best = (error, format!("{code} {file} {id} frame {frame}"));
                }
            }
            println!("{} {}: best {:.4} m at {}", parts[0], parts[1], best.0, best.1);
        }
    }

    #[test]
    #[ignore]
    fn hair_coordinates() {
        let chr = LazyArchive::open("assets/doax2/chr.afs").unwrap();
        for code in ["tn", "le", "ls", "he", "cr"] {
            let file = chr.read(&format!("v{code}AA.tpr")).unwrap();
            let xpr = Xpr2::parse(&file).unwrap();
            let object = &objects(&xpr).unwrap()[0];
            let vbufs: Vec<usize> = xpr.of_kind(b"VBUF").collect();
            for (d, decl) in object.decls.iter().enumerate() {
                let uvs: Vec<&Element> = decl.elements.iter().filter(|e| e.usage == USAGE_TEXCOORD).collect();
                let draws: Vec<_> = object.draws.iter().filter(|dr| dr.decl == d).map(|dr| dr.stages.iter().map(|s| (s.0, s.1)).collect::<Vec<_>>()).collect();
                let mut differ = 0;
                if uvs.len() >= 2 {
                    let (at, _) = xpr.buffer(vbufs[decl.streams[0].0], true).unwrap();
                    let stride = decl.streams[0].1;
                    for v in 0..decl.vertex_count {
                        let a = be32(xpr.file, at + v * stride + uvs[0].offset as usize).unwrap();
                        let b = be32(xpr.file, at + v * stride + uvs[1].offset as usize).unwrap();
                        if a != b { differ += 1; }
                    }
                }
                let tex = textures(&xpr);
                if code == "tn" && d == 1 {
                    if let Ok(dir) = std::env::var("DOAX2_TEXTURES") {
                        for i in [1usize, 2, 11, 12, 15, 16] {
                            let t = &tex[i];
                            std::fs::write(format!("{dir}/tn{i:02}_{}x{}.rgba", t.width, t.height), &t.rgba).unwrap();
                        }
                    }
                }
                for dr in object.draws.iter().filter(|dr| dr.decl == d).take(6) {
                    let st = |n: u32| dr.stages.iter().find(|s| s.0 == n).map(|s| s.2);
                    if let (Some(c), Some(m)) = (st(0), st(1)) {
                        if let (Some(ct), Some(mt)) = (tex.get(c), tex.get(m)) {
                            let ca = ct.rgba.chunks_exact(4).filter(|p| p[3] > 25).count() as f32 / (ct.rgba.len() / 4) as f32;
                            let mg = mt.rgba.chunks_exact(4).filter(|p| p[0] > 25).count() as f32 / (mt.rgba.len() / 4) as f32;
                            let mean = mt.rgba.chunks_exact(4).map(|p| p[0] as f32).sum::<f32>() / (mt.rgba.len() / 4) as f32;
                            println!("   draw: colour {c} {}x{} opaque {ca:.2}; mask {m} {}x{} over 0.1 {mg:.2}, mean grey {mean:.0}", ct.width, ct.height, mt.width, mt.height);
                        }
                    }
                }
                println!("{code} layout {d}: {} uv sets, uv0 != uv1 on {differ} of {}, stage kinds {:?}", uvs.len(), decl.vertex_count, draws.iter().take(4).collect::<Vec<_>>());
            }
        }
    }

    #[test]
    #[ignore]
    fn forearm_groups() {
        let chr = LazyArchive::open("assets/doax2/chr.afs").unwrap();
        let file = chr.read("vtn1.tpr").unwrap();
        let xpr = Xpr2::parse(&file).unwrap();
        let objects = objects(&xpr).unwrap();
        for k in [11usize, 7, 20] {
            let piece = object_piece(&xpr, &objects[k]).unwrap();
            let weights = object_weights(&xpr, &objects[k]).unwrap();
            let groups = weights[0].len();
            for g in 0..groups {
                let xs: Vec<f32> = piece.vertices.iter().zip(&weights).filter(|(_, w)| w[g] > 0.5).map(|(v, _)| v.position[0]).collect();
                let total: f32 = weights.iter().map(|w| w[g]).sum();
                if xs.is_empty() { println!("object {k} group {g}: none over 0.5, total {total:.1}"); continue; }
                let (lo, hi) = xs.iter().fold((9f32, -9f32), |(l, h), &x| (l.min(x), h.max(x)));
                println!("object {k} group {g}: {} over 0.5, x {lo:.3}..{hi:.3}, mean {:.3}, total {total:.1}", xs.len(), xs.iter().sum::<f32>() / xs.len() as f32);
            }
            let (lo, hi) = piece.vertices.iter().fold((9f32, -9f32), |(l, h), v| (l.min(v.position[0]), h.max(v.position[0])));
            println!("object {k}: x {lo:.3}..{hi:.3}");
        }
    }

    #[test]
    #[ignore]
    fn head_draw_alpha() {
        let chr = LazyArchive::open("assets/doax2/chr.afs").unwrap();
        let code = std::env::var("DOAX2_CODE").unwrap_or("tn".into());
        let file = chr.read(&format!("v{code}AA.tpr")).unwrap();
        let xpr = Xpr2::parse(&file).unwrap();
        let tex = textures(&xpr);
        for (k, object) in objects(&xpr).unwrap().iter().enumerate() {
            for (i, draw) in object.draws.iter().enumerate() {
                let base = draw.stages.iter().find(|s| s.0 == 0).copied();
                let Some((_, kind, t)) = base else { continue };
                let Some(texture) = tex.get(t) else { continue };
                let n = (texture.rgba.len() / 4) as f32;
                let partial = texture.rgba.chunks_exact(4).filter(|p| p[3] > 5 && p[3] < 250).count() as f32 / n;
                let clear = texture.rgba.chunks_exact(4).filter(|p| p[3] <= 5).count() as f32 / n;
                if let Ok(dir) = std::env::var("DOAX2_TEXTURES") {
                    std::fs::write(format!("{dir}/d{i:02}_t{t}_{}x{}.rgba", texture.width, texture.height), &texture.rgba).unwrap();
                }
                println!("   colours: diffuse {:?} ambient {:?} specular {:?} power {}", draw.diffuse, draw.ambient, draw.specular, draw.power);
                println!("object {k} draw {i}: layout {} indices {} kind {kind} texture {t} {}x{} partial {partial:.2} clear {clear:.2}", draw.decl, draw.index_count, texture.width, texture.height);
            }
        }
    }

    #[test]
    #[ignore]
    fn lash_texture_alpha() {
        let chr = LazyArchive::open("assets/doax2/chr.afs").unwrap();
        let file = chr.read("vtnAA.tpr").unwrap();
        let xpr = Xpr2::parse(&file).unwrap();
        let tex = textures(&xpr);
        let headers: Vec<usize> = xpr.of_kind(b"TX2D").collect();
        for i in [11usize, 12, 1, 15] {
            let w1 = be32(xpr.file, headers[i] + 32).unwrap();
            let mut hist = [0usize; 8];
            for p in tex[i].rgba.chunks_exact(4) { hist[(p[3] / 32) as usize] += 1; }
            let mut rgb = [0usize; 8];
            for p in tex[i].rgba.chunks_exact(4) { rgb[(p[0] / 32) as usize] += 1; }
            println!("texture {i}: format {:#x} endian {} tiled {} alpha by eighths {hist:?} red {rgb:?}", w1 & 0x3F, (w1 >> 6) & 3, be32(xpr.file, headers[i] + 28).unwrap() >> 31);
        }
        let objects = objects(&xpr).unwrap();
        for (i, d) in objects[0].draws.iter().enumerate().skip(12) { println!("draw {i}: {:?}", d.stages); }
    }

    #[test]
    #[ignore]
    fn see_through_textures() {
        let chr = LazyArchive::open("assets/doax2/chr.afs").unwrap();
        let wear: Vec<LazyArchive> = ["werone", "wertwo"].iter().map(|n| LazyArchive::open(format!("assets/doax2/{n}.afs")).unwrap()).collect();
        let names: Vec<Vec<String>> = wear.iter().map(|w| w.names().map(str::to_string).collect()).collect();
        let suit = &suits(&names, "tn")[2];
        let _ = &chr;
        let loaded = load_for(&wear[suit.archive].read_index(suit.entry).unwrap(), &wear[suit.archive].read_index(suit.entry + 1).unwrap(), Some(3)).unwrap();
        for (piece, _, placed) in &loaded.pieces {
            for m in &piece.materials {
                if is_blended(m) {
                    let t = &loaded.textures[m.textures[0]];
                    let n = t.rgba.len() / 4;
                    let low = t.rgba.chunks_exact(4).filter(|p| p[3] < 250).count();
                    println!("body object {} blended: texture {} {}x{}, {:.1}% below 250", placed.object, m.textures[0], t.width, t.height, 100.0 * low as f32 / n as f32);
                }
            }
        }
    }

    #[test]
    #[ignore]
    fn normals_point_out() {
        let chr = LazyArchive::open("assets/doax2/chr.afs").unwrap();
        for name in ["vtnAA.tpr", "vtn1.tpr"] {
            let file = chr.read(name).unwrap();
            let xpr = Xpr2::parse(&file).unwrap();
            for (k, object) in objects(&xpr).unwrap().iter().enumerate().take(3) {
                let piece = object_piece(&xpr, object).unwrap();
                if piece.vertices.len() < 100 { continue; }
                let centre = piece.vertices.iter().fold(Vec3::ZERO, |a, v| a + Vec3::from(v.position)) / piece.vertices.len() as f32;
                let out = piece.vertices.iter().filter(|v| Vec3::from(v.normal).dot(Vec3::from(v.position) - centre) > 0.0).count();
                // Faces by winding against stored normals.
                let mut agree = 0; let mut total = 0;
                for m in &piece.materials { for t in m.indices.chunks_exact(3) {
                    let p: Vec<Vec3> = t.iter().map(|&i| Vec3::from(piece.vertices[i as usize].position)).collect();
                    let face = (p[1] - p[0]).cross(p[2] - p[0]);
                    let n: Vec3 = t.iter().map(|&i| Vec3::from(piece.vertices[i as usize].normal)).sum();
                    total += 1; if face.dot(n) > 0.0 { agree += 1; }
                }}
                println!("{name} object {k}: {:.0}% normals point out from the centre, {:.0}% of faces wind with their normals", 100.0 * out as f32 / piece.vertices.len() as f32, 100.0 * agree as f32 / total.max(1) as f32);
            }
        }
    }

    #[test]
    #[ignore]
    fn head_nodes() {
        let chr = LazyArchive::open("assets/doax2/chr.afs").unwrap();
        for name in ["vheAA", "vcrAA", "vtnAA"] {
            let file = chr.read(&format!("{name}.tpr")).unwrap();
            let xpr = Xpr2::parse(&file).unwrap();
            let nodes = hierarchy(&xpr).unwrap();
            let objects = objects(&xpr).unwrap();
            let rest = rest_pose(&nodes);
            println!("{name}: {} nodes, {} objects", nodes.len(), objects.len());
            for (k, node) in nodes.iter().enumerate().take(12) {
                println!("  node {k}: object {} parent {:?} at {:?}", node.object, node.parent, rest[k].w_axis.truncate());
            }
            for (k, o) in objects.iter().enumerate() {
                let weights = object_weights(&xpr, o).unwrap();
                println!("  object {k}: {} vertices, layouts {}, weights per vertex {:?}", o.vertex_count(), o.decls.len(), o.decls.iter().map(|d| d.elements.iter().filter(|e| e.usage == USAGE_BLENDWEIGHT).count()).collect::<Vec<_>>());
                let _ = weights;
            }
        }
    }

    #[test]
    #[ignore]
    fn face_poses_read() {
        let chr = LazyArchive::open("assets/doax2/chr.afs").unwrap();
        for code in ["he", "tn", "le"] {
            let cat = chr.read(&format!("v{code}AA.cat")).unwrap();
            let start = be32(&cat, 12).unwrap() as usize;
            println!("v{code}AA.cat: word 3 at {start:#x}, MPV count {}", be32(&cat, start + 12).unwrap());
            let file = chr.read(&format!("v{code}AA.tpr")).unwrap();
            let xpr = Xpr2::parse(&file).unwrap();
            let objects = objects(&xpr).unwrap();
            for hand in hand_poses(&cat[start..], &objects).unwrap() {
                if let Ok(dir) = std::env::var("DOAX2_FACE_DUMP") {
                    let lines: Vec<String> = hand.poses.iter().flatten().map(|pose| pose.iter().flat_map(|p| p.iter().map(|v| format!("{:08x}", v.to_bits()))).collect::<Vec<_>>().join(" ")).collect();
                    std::fs::write(format!("{dir}/face_{code}_{}.txt", hand.groups.len()), lines.join("\n")).unwrap();
                }
                let piece = object_piece(&xpr, &objects[hand.object]).unwrap();
                let members: usize = hand.groups.iter().map(Vec::len).sum();
                let top = hand.groups.iter().flatten().max().copied().unwrap_or(0);
                print!("  block: object {}, {} groups ({members} vertices, highest {top}), {} entries:", hand.object, hand.groups.len(), hand.poses.len());
                for (e, pose) in hand.poses.iter().enumerate() {
                    let Some(pose) = pose else { print!(" {e}:lists"); continue };
                    let mut worst = 0f32;
                    for (group, p) in hand.groups.iter().zip(pose) {
                        if let Some(v) = piece.vertices.get(group[0]) { worst = worst.max(Vec3::from(v.position).distance(Vec3::from(*p))); }
                    }
                    let diffs: Vec<Vec3> = hand.groups.iter().zip(pose).filter_map(|(g, p)| piece.vertices.get(g[0]).map(|v| Vec3::from(*p) - Vec3::from(v.position))).collect();
                    let mean = diffs.iter().fold(Vec3::ZERO, |a, d| a + *d) / diffs.len().max(1) as f32;
                    let residual = diffs.iter().map(|d| (*d - mean).length()).fold(0f32, f32::max);
                    let moved = diffs.iter().filter(|d| (**d - mean).length() > 0.001).count();
                    print!(" {e}:mean {:?} residual {residual:.4} moved {moved}", (mean * 1000.0).round() / 1000.0);
                }
                println!();
            }
        }
    }

    #[test]
    #[ignore]
    fn hair_needles() {
        let chr = LazyArchive::open("assets/doax2/chr.afs").unwrap();
        let names: Vec<String> = chr.names().map(str::to_string).collect();
        let dir = std::env::var("DOAX2_OUT").unwrap();
        let mut out = String::new();
        for name in names.iter().filter(|n| n.ends_with(".tpr") && n.len() == 9 && n.starts_with('v')) {
            let file = chr.read(name).unwrap();
            let xpr = Xpr2::parse(&file).unwrap();
            let object = &objects(&xpr).unwrap()[0];
            let vbufs: Vec<usize> = xpr.of_kind(b"VBUF").collect();
            // Layout 2's vertex 100: its position bytes as stored.
            let Some(decl) = object.decls.get(2) else { continue };
            let (buffer, stride) = decl.streams[0];
            let (at, _) = xpr.buffer(vbufs[buffer], true).unwrap();
            let p = at + 100 * stride;
            out += &format!("{name} {} {}\n", stride, file[p..p + 12].iter().map(|b| format!("{b:02x}")).collect::<String>());
        }
        std::fs::write(format!("{dir}/hair_needles.txt"), out).unwrap();
    }

    /// Writes a head's layouts for study: per layout its elements, stride and vertex count, and
    /// the raw vertex and index buffers. `DOAX2_HEAD` names the head, `DOAX2_OUT` the folder.
    #[test]
    #[ignore]
    fn head_layout_dump() {
        let chr = LazyArchive::open("assets/doax2/chr.afs").unwrap();
        let head = std::env::var("DOAX2_HEAD").unwrap();
        let dir = std::env::var("DOAX2_OUT").unwrap();
        let file = chr.read(&format!("{head}.tpr")).unwrap();
        let xpr = Xpr2::parse(&file).unwrap();
        let object = &objects(&xpr).unwrap()[0];
        let vbufs: Vec<usize> = xpr.of_kind(b"VBUF").collect();
        let ibufs: Vec<usize> = xpr.of_kind(b"IBUF").collect();
        let mut out = String::new();
        for (d, decl) in object.decls.iter().enumerate() {
            let (buffer, stride) = decl.streams[0];
            out += &format!("layout {d} vertices {} stride {stride} streams {:?} indices {}\n", decl.vertex_count, decl.streams, decl.index_count);
            for e in &decl.elements {
                out += &format!("  {e:?}\n");
            }
            let (at, _) = xpr.buffer(vbufs[buffer], true).unwrap();
            std::fs::write(format!("{dir}/{head}_v{d}.bin"), &file[at..at + decl.vertex_count * stride]).unwrap();
            if let Some(&ib) = ibufs.get(decl.index_buffer) {
                let (at, _) = xpr.buffer(ib, false).unwrap();
                std::fs::write(format!("{dir}/{head}_i{d}.bin"), &file[at..at + decl.index_count * 2]).unwrap();
            }
        }
        for dr in &object.draws {
            out += &format!("draw layout {} first {} count {} stages {:?}\n", dr.decl, dr.first_index, dr.index_count, dr.stages);
        }
        std::fs::write(format!("{dir}/{head}_layouts.txt"), out).unwrap();
        std::fs::write(format!("{dir}/{head}.cat"), chr.read(&format!("{head}.cat")).unwrap()).unwrap();
    }

    #[test]
    #[ignore]
    fn suit_reads() {
        let wear = LazyArchive::open("assets/doax2/wertwo.afs").unwrap();
        let names: Vec<String> = wear.names().map(str::to_string).collect();
        let index = names.iter().position(|n| n == "vtn_tk_b_m").unwrap();
        let file = wear.read_index(index).unwrap();
        let xpr = Xpr2::parse(&file).unwrap();
        let objects = objects(&xpr).unwrap();
        let nodes = hierarchy(&xpr).unwrap();
        println!("{} objects, {} nodes", objects.len(), nodes.len());
        let rest = rest_pose(&nodes);
        for (i, (object, node)) in objects.iter().zip(object_nodes(&nodes, objects.len())).enumerate() {
            let piece = object_piece(&xpr, object).unwrap();
            let (mut lo, mut hi) = (Vec3::splat(9.0), Vec3::splat(-9.0));
            for v in &piece.vertices {
                let p = node.map_or(Vec3::from(v.position), |n| rest[n].transform_point3(v.position.into()));
                lo = lo.min(p);
                hi = hi.max(p);
            }
            let bases: Vec<_> = piece.materials.iter().map(|m| m.textures.first().copied()).collect();
            println!("object {i} node {node:?}: {} vertices in {} layouts, {:?} {:?}, textures {bases:?}", piece.vertices.len(), object.decls.len(), (lo * 100.0).round() / 100.0, (hi * 100.0).round() / 100.0);
        }
        for (i, t) in textures(&xpr).iter().enumerate() {
            println!("texture {i}: {}x{}", t.width, t.height);
        }
    }

    #[test]
    #[ignore]
    fn tina_body_reads() {
        let chr = LazyArchive::open("assets/doax2/chr.afs").unwrap();
        let file = chr.read("vtn1.tpr").unwrap();
        let xpr = Xpr2::parse(&file).unwrap();
        let objects = objects(&xpr).unwrap();
        let nodes = hierarchy(&xpr).unwrap();
        assert_eq!(objects.len(), nodes.len());
        for (i, object) in objects.iter().enumerate() {
            let piece = object_piece(&xpr, object).unwrap();
            let bad = piece.vertices.iter().filter(|v| (Vec3::from(v.normal).length() - 1.0).abs() > 0.01).count();
            let triangles: usize = piece.materials.iter().map(|m| m.indices.len() / 3).sum();
            let stages: Vec<_> = object.draws.iter().map(|d| d.stages.clone()).collect();
            println!("object {i}: {} vertices, {triangles} triangles, {bad} normals off unit, stages {stages:?}", piece.vertices.len());
        }
        let (lo, hi) = bounds(&xpr).unwrap();
        println!("bounds {lo:?} {hi:?}");
        for (i, t) in textures(&xpr).iter().enumerate() {
            println!("texture {i}: {}x{}", t.width, t.height);
            if let Ok(dir) = std::env::var("DOAX2_TEXTURES") {
                std::fs::write(format!("{dir}/tex{i}_{}x{}.rgba", t.width, t.height), &t.rgba).unwrap();
            }
        }
    }
}
