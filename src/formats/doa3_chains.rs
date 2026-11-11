//! Particle chains: DOA3's hair, ribbons and hanging cloth.
//!
//! Update `sub_00042540`, setup `sub_000422B0`, vertex writes `sub_0003F5F0`,
//! collision `sub_0003D7A0`. See `docs/doa3-internals.md`.

use glam::{Mat4, Vec3};

const CHAIN_SECTION: u8 = 1;
const KNOWN_SECTIONS: [u8; 11] = [0, 1, 2, 3, 4, 5, 0xFA, 0xFB, 0xFC, 0xFD, 0xFE];
const SECTION_END: u8 = 0xFF;
/// The game ends each segment's point list with this value, 999.0.
pub(super) const POINT_LIST_END: u32 = 0x4479_C000;
pub(super) const NO_VERTEX: u32 = u32::MAX;
pub(super) const VERTEX_STRIDE: u32 = 32;
const SHAPE_SIZE: usize = 0x1C;
/// Height of the floor in the viewer plus the game's 0.003 margin.
const FLOOR: f32 = 0.003;
/// Radians to the game's 16-bit angles.
pub(super) const ANGLE_UNITS: f32 = 10430.378;
const FULL_TURN_HALF: i32 = 0x8000;

thread_local! {
    /// Set while a DOA4 CAT is read: the same records with big-endian words.
    static BIG_ENDIAN: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

fn big_endian() -> bool {
    BIG_ENDIAN.with(std::cell::Cell::get)
}

pub(super) fn read_u32(data: &[u8], offset: usize) -> Option<u32> {
    let bytes: [u8; 4] = data.get(offset..offset.checked_add(4)?)?.try_into().ok()?;
    Some(if big_endian() { u32::from_be_bytes(bytes) } else { u32::from_le_bytes(bytes) })
}

fn read_u16(data: &[u8], offset: usize) -> Option<u16> {
    let bytes: [u8; 2] = data.get(offset..offset.checked_add(2)?)?.try_into().ok()?;
    Some(if big_endian() { u16::from_be_bytes(bytes) } else { u16::from_le_bytes(bytes) })
}

pub(super) fn read_f32(data: &[u8], offset: usize) -> Option<f32> {
    read_u32(data, offset).map(f32::from_bits)
}

pub(super) fn read_vec3(data: &[u8], offset: usize) -> Option<Vec3> {
    Some(Vec3::new(read_f32(data, offset)?, read_f32(data, offset + 4)?, read_f32(data, offset + 8)?))
}

/// One collision shape in a bone's space (`sub_0003A1B0` to `sub_0003A940`).
#[derive(Debug, Clone, Copy)]
pub struct Shape {
    pub bone: usize,
    pub kind: u8,
    pub values: [f32; 6],
    /// DOA2U: the node and quarter turns behind a slot of 0x80 and up.
    pub slot: Option<(usize, Mat4)>,
}

/// DOA2U's limb slots (`sub_0004B920`): slot, matrix and quarter turns about X, Y and Z.
/// They give shapes the limb frames of DOA3. Read against the game's table at `0x849E40`.
const LIMB_SLOTS: [(usize, usize, [f32; 3]); 8] = [
    (0x82, 6, [1.0, 0.0, 1.0]),
    (0x83, 7, [1.0, 2.0, -1.0]),
    (0x84, 8, [0.0, 1.0, 0.0]),
    (0x85, 4, [0.0, -1.0, 0.0]),
    (0x86, 12, [1.0, 0.0, -1.0]),
    (0x87, 13, [1.0, 2.0, 1.0]),
    (0x88, 14, [0.0, -1.0, 0.0]),
    (0x89, 10, [0.0, 1.0, 0.0]),
];

/// Resolves DOA2U's limb slots. Others, such as 0x80 and 0x81, stay without a node.
pub(super) fn resolve_limb_slots(shapes: &mut [Shape]) {
    for shape in shapes {
        if let Some(&(_, node, turns)) = LIMB_SLOTS.iter().find(|entry| entry.0 == shape.bone) {
            let angle = |turn: f32| turn * std::f32::consts::FRAC_PI_2;
            let turn = Mat4::from_rotation_x(angle(turns[0])) * Mat4::from_rotation_y(angle(turns[1])) * Mat4::from_rotation_z(angle(turns[2]));
            shape.slot = Some((node, turn));
        }
    }
}

impl Shape {
    /// The space of this shape.
    pub(super) fn matrix(&self, nodes: &[Mat4]) -> Option<Mat4> {
        match self.slot {
            Some((node, turn)) => nodes.get(node).map(|m| *m * turn),
            None => nodes.get(self.bone).copied(),
        }
    }

    /// Pushes `point` out of the shape. Returns whether it moved it.
    pub(super) fn push_out(&self, point: &mut Vec3) -> bool {
        let f = &self.values;
        match self.kind {
            // Box: leaves through the nearest face.
            0 => {
                let depths = [
                    point.x - f[0],
                    f[1] - point.x,
                    point.y - f[2],
                    f[3] - point.y,
                    point.z - f[4],
                    f[5] - point.z,
                ];
                if depths.iter().any(|&d| d < 0.0) {
                    return false;
                }
                let (face, depth) = depths
                    .iter()
                    .copied()
                    .enumerate()
                    .fold((0, f32::MAX), |best, (i, d)| if d < best.1 { (i, d) } else { best });
                let change = if face % 2 == 0 { -depth } else { depth };
                match face / 2 {
                    0 => point.x += change,
                    1 => point.y += change,
                    _ => point.z += change,
                }
                true
            }
            // Box with rounded edges; the radius is half its depth.
            1 => {
                let inside = point.x >= f[0]
                    && point.x <= f[1]
                    && point.y >= f[2]
                    && point.y <= f[3]
                    && point.z >= f[4]
                    && point.z <= f[5];
                if !inside {
                    return false;
                }
                let radius = (f[5] - f[4]) * 0.5;
                let beyond = |v: f32, low: f32, high: f32| {
                    if high - radius < v {
                        v - (high - radius)
                    } else if low + radius > v {
                        v - (low + radius)
                    } else {
                        0.0
                    }
                };
                let offset = Vec3::new(
                    beyond(point.x, f[0], f[1]),
                    beyond(point.y, f[2], f[3]),
                    point.z - (f[4] + radius),
                );
                push_from(point, offset, radius)
            }
            // Capsule: axis, centre on the other two axes, radius, axis range.
            2 => {
                let (axis, a, b) = match f[0] as i32 {
                    0 => (point.x, point.z, point.y),
                    1 => (point.y, point.x, point.z),
                    _ => (point.z, point.x, point.y),
                };
                let along = if axis > f[5] {
                    axis - f[5]
                } else if axis < f[4] {
                    axis - f[4]
                } else {
                    0.0
                };
                let (da, db) = (a - f[1], b - f[2]);
                let distance = (along * along + da * da + db * db).sqrt();
                if distance >= f[3] {
                    return false;
                }
                let scale = if distance != 0.0 { (f[3] - distance) / distance } else { 1.0 };
                let (along, da, db) = (along * scale, da * scale, db * scale);
                match f[0] as i32 {
                    0 => *point += Vec3::new(along, db, da),
                    1 => *point += Vec3::new(da, along, db),
                    _ => *point += Vec3::new(da, db, along),
                }
                true
            }
            // Sphere.
            3 => {
                let centre = Vec3::new(f[0], f[1], f[2]);
                push_from(point, *point - centre, f[3])
            }
            // Ellipsoid.
            4 => {
                let centre = Vec3::new(f[0], f[1], f[2]);
                let radii = Vec3::new(f[3], f[4], f[5]);
                let scaled = (*point - centre) / radii;
                let distance = scaled.length();
                if distance >= 1.0 {
                    return false;
                }
                let scale = if distance != 0.0 { (1.0 - distance) / distance } else { 1.0 };
                *point += scaled * scale * radii;
                true
            }
            // Keeps the point inside a box.
            5 => {
                let before = *point;
                *point = point.clamp(Vec3::new(f[0], f[2], f[4]), Vec3::new(f[1], f[3], f[5]));
                before != *point
            }
            // Cone along z: the radius goes from f[2] to f[3] over the range.
            7 => {
                let z = point.z;
                let (along, radius) = cone_at(z, f);
                let offset = Vec3::new(point.x - f[0], point.y - f[1], along);
                push_from(point, offset, radius)
            }
            _ => false,
        }
    }

    /// Pushes an edge out of the shape, moving both ends the same
    /// (`sub_0003B0F0` to `sub_0003B7F0`). Only these four kinds act on edges.
    pub(super) fn push_edge_out(&self, a: &mut Vec3, b: &mut Vec3) {
        let f = &self.values;
        let closest = |start: Vec3, along: Vec3| -> Option<f32> {
            let size = along.length_squared();
            (size != 0.0).then(|| (-along.dot(start) / size).clamp(0.0, 1.0))
        };
        let change = match self.kind {
            2 => {
                let (axis, u, v) = match f[0] as i32 {
                    0 => (0, 2, 1),
                    1 => (1, 0, 2),
                    _ => (2, 0, 1),
                };
                let start = Vec3::new(a[u] - f[1], a[v] - f[2], 0.0);
                let along = Vec3::new(b[u] - a[u], b[v] - a[v], 0.0);
                let Some(t) = closest(start, along) else { return };
                let near = start + along * t;
                let position = (b[axis] - a[axis]) * t + a[axis];
                let beyond = if position > f[5] {
                    position - f[5]
                } else if position < f[4] {
                    position - f[4]
                } else {
                    0.0
                };
                let distance = (near.length_squared() + beyond * beyond).sqrt();
                if distance >= f[3] {
                    return;
                }
                let scale = if distance != 0.0 { (f[3] - distance) / distance } else { 1.0 };
                let mut change = Vec3::ZERO;
                change[u] = near.x * scale;
                change[v] = near.y * scale;
                change[axis] = beyond * scale;
                change
            }
            3 => {
                let start = *a - Vec3::new(f[0], f[1], f[2]);
                let along = *b - *a;
                let Some(t) = closest(start, along) else { return };
                let near = start + along * t;
                let distance = near.length();
                if distance >= f[3] {
                    return;
                }
                near * if distance != 0.0 { (f[3] - distance) / distance } else { 1.0 }
            }
            4 => {
                let radii = Vec3::new(f[3], f[4], f[5]);
                let start = (*a - Vec3::new(f[0], f[1], f[2])) / radii;
                let along = (*b - *a) / radii;
                let Some(t) = closest(start, along) else { return };
                let near = start + along * t;
                let distance = near.length();
                if distance >= 1.0 {
                    return;
                }
                near * if distance != 0.0 { (1.0 - distance) / distance } else { 1.0 } * radii
            }
            7 => {
                let start = Vec3::new(a.x - f[0], a.y - f[1], 0.0);
                let along = Vec3::new(b.x - a.x, b.y - a.y, 0.0);
                let Some(t) = closest(start, along) else { return };
                let near = start + along * t;
                let (beyond, radius) = cone_at((b.z - a.z) * t + a.z, f);
                let offset = Vec3::new(near.x, near.y, beyond);
                let distance = offset.length();
                if distance >= radius {
                    return;
                }
                offset * if distance != 0.0 { (radius - distance) / distance } else { 1.0 }
            }
            _ => return,
        };
        *a += change;
        *b += change;
    }
}

/// How far past a cone's z range a point is, and the cone's radius there.
fn cone_at(z: f32, f: &[f32; 6]) -> (f32, f32) {
    if z > f[5] {
        (z - f[5], f[3])
    } else if z < f[4] {
        (z - f[4], f[2])
    } else {
        (0.0, (z - f[4]) / (f[5] - f[4]) * (f[3] - f[2]) + f[2])
    }
}

fn push_from(point: &mut Vec3, offset: Vec3, radius: f32) -> bool {
    let distance = offset.length();
    if distance >= radius {
        return false;
    }
    let scale = if distance != 0.0 { (radius - distance) / distance } else { 1.0 };
    *point += offset * scale;
    true
}

/// A point in a segment's frame and the vertices that sit on it.
#[derive(Debug, Clone)]
pub struct ChainPoint {
    pub position: Vec3,
    pub vertices: Vec<usize>,
}

/// The vertices of one object that a chain moves.
#[derive(Debug, Clone)]
pub struct ChainTarget {
    pub object: usize,
    /// Whether this object is only drawn by the chain, in the space of the
    /// bone moved by the chain's offset (`0xFFFF` in the file).
    pub detached: bool,
    /// One list per particle.
    pub segments: Vec<Vec<ChainPoint>>,
}

#[derive(Debug, Clone, Copy, Default)]
struct Particle {
    position: Vec3,
    segment: Vec3,
    velocity: Vec3,
    last_pull: Vec3,
}

#[derive(Debug, Clone)]
pub struct SoftChain {
    pub bone: usize,
    pub offset: Vec3,
    pub targets: Vec<ChainTarget>,
    pub shapes: Vec<Shape>,
    segment_length: f32,
    gravity: f32,
    pull: f32,
    damping: f32,
    direction: Vec3,
    bend: f32,
    stiffness: f32,
    max_force: f32,
    particles: Vec<Particle>,
    ready: bool,
    /// DOA2U's factor for the wind. It goes in the pull, not the force.
    wind_factor: Option<f32>,
    root_vertex: Option<(usize, usize)>,
}

/// Finds the CAT's list of physics sections and returns its base and entries.
/// Every offset in these sections is relative to the base. Some CATs scramble
/// the header that would point here, so this scans for a list that validates.
pub(super) fn find_sections(cat: &[u8]) -> Option<(usize, Vec<(u8, usize)>)> {
    const SCAN_END: usize = 0x4000;
    const MAX_SECTIONS: usize = 600;
    let chain_is_valid = |base: usize, offset: usize| -> bool {
        let at = base + offset;
        let (Some(&count), Some(&bone)) = (cat.get(at), cat.get(at + 1)) else { return false };
        let pointers_fit = (0x48..=0x60)
            .step_by(4)
            .all(|field| read_u32(cat, at + field).is_some_and(|v| (v as usize) < cat.len()));
        (1..=64).contains(&count)
            && (bone <= 22 || bone == 0xFF)
            && pointers_fit
            && read_u32(cat, at + 0x4C).is_some_and(|head| head != 0)
    };
    // The header's third word points here unless it is scrambled. Scanning alone
    // can stop at empty bytes just before the real list, shifting every record.
    let from_header = read_u32(cat, 8).map(|base| base as usize).filter(|base| base % 4 == 0 && *base >= 0x14);
    let scanned = (0x14..SCAN_END.min(cat.len().saturating_sub(8))).step_by(4);
    from_header.into_iter().chain(scanned).find_map(|base| {
        let mut sections = Vec::new();
        let mut at = base;
        loop {
            let kind = *cat.get(at)?;
            if kind == SECTION_END {
                break;
            }
            let offset = read_u32(cat, at + 4)? as usize;
            let valid = KNOWN_SECTIONS.contains(&kind) && offset != 0 && base + offset < cat.len();
            if !valid || sections.len() >= MAX_SECTIONS {
                return None;
            }
            sections.push((kind, offset));
            at += 8;
        }
        let chains_valid = sections
            .iter()
            .filter(|(kind, _)| *kind == CHAIN_SECTION)
            .all(|&(_, offset)| chain_is_valid(base, offset));
        // A lone entry is too easy to find by chance.
        (chains_valid && sections.len() >= 2).then_some((base, sections))
    })
}

pub(super) fn parse_shapes(cat: &[u8], mut at: usize) -> Option<Vec<Shape>> {
    let mut shapes = Vec::new();
    loop {
        let kind = *cat.get(at + 1)?;
        if kind == 0xFF {
            return Some(shapes);
        }
        let mut values = [0.0; 6];
        for (i, value) in values.iter_mut().enumerate() {
            *value = read_f32(cat, at + 4 + i * 4)?;
        }
        shapes.push(Shape { bone: *cat.get(at)? as usize, kind, values, slot: None });
        at += SHAPE_SIZE;
    }
}

/// Reads the object index and point lists of a chain. Chains that place whole
/// objects instead of writing vertices (`sub_0003AA20`) return `None`.
fn parse_target(cat: &[u8], base: usize, count: usize, head: u32, points: u32, offsets: u32) -> Option<ChainTarget> {
    let head_at = base + head as usize;
    let kind = read_u16(cat, head_at)?;
    if kind != 0xFFFE && kind != 0xFFFF {
        return None;
    }
    let object = (read_u16(cat, head_at + 2)? & 0x0FFF) as usize;
    let mut point_at = base + points as usize;
    let mut offset_at = base + offsets as usize;
    let mut segments = Vec::with_capacity(count);
    for _ in 0..count {
        let mut points = Vec::new();
        while read_u32(cat, point_at)? != POINT_LIST_END {
            let position = read_vec3(cat, point_at)?;
            point_at += 16;
            let mut value = read_u32(cat, offset_at)?;
            offset_at += 4;
            let mut vertices = Vec::new();
            if value != NO_VERTEX {
                // The low bit marks that another vertex follows.
                // DOA4's words also name a vertex layout and its strides vary, so they are
                // kept whole there, for `parse_chain_records_big_endian`'s caller to turn.
                let vertex = |value: u32| if big_endian() { (value & !1) as usize } else { (value / VERTEX_STRIDE) as usize };
                while value & 1 != 0 {
                    vertices.push(vertex(value & !1));
                    value = read_u32(cat, offset_at)?;
                    offset_at += 4;
                }
                vertices.push(vertex(value));
            }
            points.push(ChainPoint { position, vertices });
        }
        point_at += 16;
        segments.push(points);
    }
    Some(ChainTarget { object, detached: kind == 0xFFFF, segments })
}

const PROP_SECTION: u8 = 0;
const PROP_RECORD_SIZE: usize = 0x34;
const PROP_MODE_WORN: u8 = 1;
const PROP_MODE_FOLLOWS: u8 = 2;
const MAX_BONE: u8 = 22;
const MAX_PROP_OFFSET: f32 = 1.0;

/// An object worn on a bone, such as a hat (section 0 of a CAT, `sub_0003BB60`).
/// The game can knock these off (`sub_0003DCB0`); here they stay on.
/// Most entries of this section in the files are empty or garbage.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WornProp {
    pub bone: usize,
    pub object: usize,
    pub offset: Vec3,
    /// Game angles about x, y and z.
    pub turn: [u16; 3],
}

impl WornProp {
    /// `sub_0003BC70`: the object's place in the bone's space.
    pub fn matrix(&self) -> Mat4 {
        let angle = |a: u16| a as f32 / ANGLE_UNITS;
        Mat4::from_translation(self.offset)
            * Mat4::from_rotation_z(angle(self.turn[2]))
            * Mat4::from_rotation_y(angle(self.turn[1]))
            * Mat4::from_rotation_x(angle(self.turn[0]))
    }
}

pub fn parse_props(cat: &[u8], object_count: usize) -> Vec<WornProp> {
    let Some((base, sections)) = find_sections(cat) else { return Vec::new() };
    sections
        .iter()
        .filter(|(kind, _)| *kind == PROP_SECTION)
        .filter_map(|&(_, offset)| {
            let record = cat.get(base + offset..base + offset + PROP_RECORD_SIZE)?;
            let (mode, bone) = (record[0], record[1]);
            let object = read_u16(record, 0x32)? as usize;
            let offset = read_vec3(record, 0x1C)?;
            let valid = (mode == PROP_MODE_WORN || mode == PROP_MODE_FOLLOWS)
                && bone <= MAX_BONE
                && object < object_count
                && offset.abs().max_element() < MAX_PROP_OFFSET;
            valid.then(|| WornProp {
                bone: bone as usize,
                object,
                offset,
                turn: [
                    read_u16(record, 0x2C).unwrap_or(0),
                    read_u16(record, 0x2E).unwrap_or(0),
                    read_u16(record, 0x30).unwrap_or(0),
                ],
            })
        })
        .collect()
}

const OVERLAY_SECTION: u8 = 3;
const OVERLAY_RECORD_SIZE: usize = 0x1A;
const OVERLAY_SOLID: u8 = 1;
const OVERLAY_TRANSLUCENT: u8 = 2;
/// Above these bits the game means objects outside the model (`sub_0003D150`).
const OBJECT_CLASS_MASK: u16 = 0xF000;

/// An extra object on a bone (section 3 of a CAT, `sub_0003CE40`, `sub_0003EBA0`),
/// such as a sheathed knife or the translucent layer of the hair.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Overlay {
    pub prop: WornProp,
    pub translucent: bool,
}

pub fn parse_overlays(cat: &[u8], object_count: usize) -> Vec<Overlay> {
    let Some((base, sections)) = find_sections(cat) else { return Vec::new() };
    sections
        .iter()
        .filter(|(kind, _)| *kind == OVERLAY_SECTION)
        .filter_map(|&(_, offset)| parse_overlay(cat, base + offset, object_count))
        .collect()
}

/// Reads extra objects at offsets from `base`, for DOA2U, whose list is read elsewhere.
pub fn parse_overlay_records(cat: &[u8], base: usize, offsets: &[usize], object_count: usize) -> Vec<Overlay> {
    offsets.iter().filter_map(|&offset| parse_overlay(cat, base + offset, object_count)).collect()
}

fn parse_overlay(cat: &[u8], at: usize, object_count: usize) -> Option<Overlay> {
    let record = cat.get(at..at + OVERLAY_RECORD_SIZE)?;
    let id = read_u16(record, 0)?;
    let (mode, bone) = (record[2], record[3]);
    let object = (id & !OBJECT_CLASS_MASK) as usize;
    let offset = read_vec3(record, 4)?;
    let valid = id & OBJECT_CLASS_MASK == 0
        && (mode == OVERLAY_SOLID || mode == OVERLAY_TRANSLUCENT)
        && bone <= MAX_BONE
        && object < object_count
        && offset.abs().max_element() < MAX_PROP_OFFSET;
    valid.then(|| Overlay {
        prop: WornProp {
            bone: bone as usize,
            object,
            offset,
            turn: [
                read_u16(record, 0x14).unwrap_or(0),
                read_u16(record, 0x16).unwrap_or(0),
                read_u16(record, 0x18).unwrap_or(0),
            ],
        },
        translucent: mode == OVERLAY_TRANSLUCENT,
    })
}

/// Reads every chain in a CAT that moves vertices.
/// Reads chains at offsets from `base`, for DOA2U, whose list is read elsewhere.
pub fn parse_chain_records(cat: &[u8], base: usize, offsets: &[usize]) -> Vec<SoftChain> {
    offsets
        .iter()
        .filter_map(|&offset| {
            let at = base + offset;
            let mut chain = parse_chain(cat, base, at)?;
            resolve_limb_slots(&mut chain.shapes);
            chain.wind_factor = Some(read_f32(cat, at + 0x3C)?);
            // A root on a vertex of the object at +2, by byte offset (`sub_000591A0`).
            let root = read_u32(cat, at + 0x44)?;
            if root != 0 {
                chain.root_vertex = Some((*cat.get(at + 2)? as usize, root as usize / VERTEX_STRIDE as usize));
            }
            Some(chain)
        })
        .collect()
}

/// DOA4's chains: DOA3's records with big-endian words, at offsets from `base`. Each vertex
/// of a target is left as the file's word, the layout in its top byte and a byte offset below.
/// The colliders' bones are DOA3's nodes, as the draw lists' are.
pub fn parse_chain_records_big_endian(cat: &[u8], base: usize, offsets: &[usize]) -> Vec<SoftChain> {
    BIG_ENDIAN.with(|flag| flag.set(true));
    let chains = offsets
        .iter()
        .filter_map(|&offset| {
            let mut chain = parse_chain(cat, base, base + offset)?;
            chain.wind_factor = Some(read_f32(cat, base + offset + 0x3C)?);
            Some(chain)
        })
        .collect();
    BIG_ENDIAN.with(|flag| flag.set(false));
    chains
}

pub fn parse_chains(cat: &[u8]) -> Vec<SoftChain> {
    let Some((base, sections)) = find_sections(cat) else { return Vec::new() };
    sections
        .iter()
        .filter(|(kind, _)| *kind == CHAIN_SECTION)
        .filter_map(|&(_, offset)| parse_chain(cat, base, base + offset))
        .collect()
}

fn parse_chain(cat: &[u8], base: usize, at: usize) -> Option<SoftChain> {
    let count = *cat.get(at)? as usize;
    let bone = *cat.get(at + 1)? as usize;
    // The fourth byte turns the chain on. Bone 0xFF is a stage object.
    if *cat.get(at + 3)? == 0 || bone == 0xFF {
        return None;
    }
    let field = |offset: usize| read_u32(cat, at + offset);
    let shapes_at = field(0x48)?;
    let shapes = if shapes_at != 0 { parse_shapes(cat, base + shapes_at as usize)? } else { Vec::new() };

    let mut targets = vec![parse_target(cat, base, count, field(0x4C)?, field(0x50)?, field(0x54)?)?];
    let second = field(0x60)?;
    if second != 0 {
        let second_at = base + second as usize;
        let (head, points, offsets) =
            (read_u32(cat, second_at)?, read_u32(cat, second_at + 4)?, read_u32(cat, second_at + 8)?);
        if let Some(mut target) = parse_target(cat, base, count, head, points, offsets) {
            target.detached = false;
            targets.push(target);
        }
    }

    Some(SoftChain {
        bone,
        offset: read_vec3(cat, at + 0x14)?,
        targets,
        shapes,
        segment_length: read_f32(cat, at + 4)?,
        gravity: read_f32(cat, at + 8)? * 0.5,
        pull: read_f32(cat, at + 0xC)?,
        damping: read_f32(cat, at + 0x10)?,
        direction: read_vec3(cat, at + 0x24)?,
        bend: read_f32(cat, at + 0x34)?,
        stiffness: read_f32(cat, at + 0x38)?,
        max_force: read_f32(cat, at + 0x40)?,
        particles: vec![Particle::default(); count],
        ready: false,
        wind_factor: None,
        root_vertex: None,
    })
}

impl SoftChain {
    /// DOA2U: the object and vertex this chain starts on, if any.
    pub fn root_vertex(&self) -> Option<(usize, usize)> {
        self.root_vertex
    }

    /// Moves the chain's start, in the bone's space, for `root_vertex`.
    pub fn set_root(&mut self, position: Vec3) {
        self.offset = position;
    }

    /// Whether this chain hangs from the front of the torso.
    pub fn is_on_torso_front(&self) -> bool {
        self.bone == 0 && self.offset.z > 0.0
    }

    /// Advances one 60 Hz game frame. `wind` is added to the force, as the
    /// game does with its stage wind.
    pub fn step(&mut self, nodes: &[Mat4], wind: Vec3) {
        let Some(bone) = nodes.get(self.bone) else { return };
        if !self.ready {
            self.settle(nodes, bone);
            self.ready = true;
            return;
        }
        let mut point = bone.transform_point3(self.offset);
        let mut direction = bone.transform_vector3(self.direction);
        // DOA2U adds the wind to the pull, so it also feeds the velocity.
        let (pull_wind, wind) = match self.wind_factor {
            Some(factor) => (wind * factor, Vec3::ZERO),
            None => (Vec3::ZERO, wind),
        };
        for i in 0..self.particles.len() {
            let particle = self.particles[i];
            let pull = particle.position - point + pull_wind;
            let mut velocity = (particle.velocity + pull - particle.last_pull) * self.damping;
            velocity.y -= self.gravity;

            let mut force = pull * self.pull + velocity + direction;
            direction *= self.bend;
            // DOA2U pulls towards the segment before, and only the first towards the direction.
            let target = match i.checked_sub(1) {
                Some(before) if self.wind_factor.is_some() => self.particles[before].segment,
                _ => direction,
            };
            force += (target - particle.segment) * self.stiffness + wind;
            let size = force.length();
            if size > self.max_force {
                force *= self.max_force / size;
            }

            let segment = self.finish_segment(nodes, point, particle.segment + force);
            self.particles[i] = Particle { position: point, segment, velocity, last_pull: pull };
            point += segment;
        }
    }

    /// Places the chain at rest (`sub_000422B0`).
    fn settle(&mut self, nodes: &[Mat4], bone: &Mat4) {
        let mut point = bone.transform_point3(self.offset);
        let mut direction = bone.transform_vector3(self.direction);
        let rest_velocity = if self.damping > 0.0 && self.damping < 1.0 {
            -self.gravity / (1.0 - self.damping)
        } else {
            -self.gravity
        };
        for i in 0..self.particles.len() {
            let velocity = Vec3::new(0.0, rest_velocity, 0.0);
            let target = direction + velocity;
            let segment = target + (direction - target) * self.stiffness;
            direction *= self.bend;
            let segment = self.finish_segment(nodes, point, segment);
            self.particles[i] = Particle { position: point, segment, velocity, last_pull: Vec3::ZERO };
            point += segment;
        }
    }

    /// Restores the segment's length and applies collisions and the floor.
    fn finish_segment(&self, nodes: &[Mat4], point: Vec3, segment: Vec3) -> Vec3 {
        let segment = with_length(segment, self.segment_length);
        let segment = self.collide(nodes, point, segment);
        keep_above_floor(point.y, segment, self.segment_length, FLOOR)
    }

    /// `sub_0003D7A0`: shapes on the same bone are tested together.
    fn collide(&self, nodes: &[Mat4], point: Vec3, segment: Vec3) -> Vec3 {
        if self.shapes.is_empty() {
            return segment;
        }
        let mut target = point + segment;
        let mut shapes = self.shapes.iter().peekable();
        let mut moved = false;
        let mut hit = false;
        let mut current = usize::MAX;
        let mut local = target;
        let mut matrix = Mat4::IDENTITY;
        while let Some(shape) = shapes.next() {
            if shape.bone != current {
                current = shape.bone;
                matrix = shape.matrix(nodes).unwrap_or(Mat4::IDENTITY);
                local = matrix.inverse().transform_point3(target);
                hit = false;
            }
            hit |= shape.push_out(&mut local);
            let next_differs = shapes.peek().map_or(true, |next| next.bone != current);
            if next_differs && hit {
                target = matrix.transform_point3(local);
                moved = true;
            }
        }
        if moved && self.wind_factor.is_some() {
            // DOA2U leaves the moved tip where it is, as on cloth (`sub_0004BFE0`).
            target - point
        } else if moved {
            with_length(target - point, self.segment_length)
        } else {
            segment
        }
    }

    /// Positions of `target`'s vertices in the space of the chain's bone.
    pub fn vertex_positions<'a>(
        &'a self,
        target: &'a ChainTarget,
        nodes: &[Mat4],
    ) -> impl Iterator<Item = (usize, Vec3)> + 'a {
        let inverse = nodes.get(self.bone).map_or(Mat4::IDENTITY, Mat4::inverse);
        let segments: Vec<Vec3> =
            self.particles.iter().map(|p| inverse.transform_vector3(p.segment)).collect();
        let mut point = if target.detached { Vec3::ZERO } else { self.offset };
        let frames: Vec<Mat4> = (0..segments.len())
            .map(|i| {
                point += segments[i];
                let along = segments[i] + segments.get(i + 1).copied().unwrap_or(Vec3::ZERO);
                segment_frame(point, along)
            })
            .collect();
        // Detached objects hang from the offset, and the viewer draws them with
        // the bone's matrix.
        let shift = if target.detached { self.offset } else { Vec3::ZERO };
        target.segments.iter().zip(frames).flat_map(move |(points, frame)| {
            points.iter().flat_map(move |p| {
                let position = frame.transform_point3(p.position) + shift;
                p.vertices.iter().map(move |&vertex| (vertex, position))
            })
        })
    }
}

impl SoftChain {
    /// Where the vertices of a detached object that no particle moves belong:
    /// they stay at the anchor, or they would stretch to the bone's origin as
    /// long thin triangles. `rest` are the object's positions.
    pub fn anchored_vertices<'a>(
        &'a self,
        target: &ChainTarget,
        rest: &'a [Vec3],
    ) -> impl Iterator<Item = (usize, Vec3)> + 'a {
        let moved: std::collections::HashSet<usize> =
            target.segments.iter().flatten().flat_map(|point| point.vertices.iter().copied()).collect();
        rest.iter()
            .enumerate()
            .filter(move |(vertex, _)| !moved.contains(vertex))
            .map(|(vertex, position)| (vertex, *position + self.offset))
    }
}

pub(super) fn with_length(vector: Vec3, length: f32) -> Vec3 {
    let size = vector.length();
    if size != 0.0 {
        vector * (length / size)
    } else {
        vector
    }
}

/// `sub_00039FE0`: keeps the end of a segment above the floor.
pub(super) fn keep_above_floor(y: f32, segment: Vec3, length: f32, floor: f32) -> Vec3 {
    if y + segment.y >= floor {
        return segment;
    }
    let height = floor - y;
    if y < floor && height >= length {
        return Vec3::new(0.0, length, 0.0);
    }
    let across = (length * length - height * height).max(0.0).sqrt();
    let mut flat = Vec3::new(segment.x, 0.0, segment.z);
    if segment.x == 0.0 && segment.z == 0.0 {
        flat.x = 1.0;
    }
    let flat = with_length(flat, across);
    Vec3::new(flat.x, height, flat.z)
}

/// `sub_0003F5F0`: the frame of a segment. In the game's row vectors it is
/// `Rz(b) * Rx(a) * T(point)`. Verified: error 0.00000 on Helena's object 34.
fn segment_frame(point: Vec3, along: Vec3) -> Mat4 {
    let to_game_angle = |radians: f32| (radians * ANGLE_UNITS) as i32;
    let to_radians = |angle: i32| angle as f32 / ANGLE_UNITS;
    let a = to_game_angle(along.z.atan2(along.y)) + FULL_TURN_HALF;
    let b = to_game_angle(along.x.atan2((along.y * along.y + along.z * along.z).sqrt()));
    Mat4::from_translation(point)
        * Mat4::from_rotation_x(to_radians(a))
        * Mat4::from_rotation_z(to_radians(b))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One step of DOA2U chains against particles from the game. DOA2U_PAIRS comes
    /// from `tools/re/doa2u_chain_pairs.py`, and DOA2U_RECORDS lists "fighter costume".
    #[test]
    #[ignore]
    fn doa2u_chain_single_steps() {
        use crate::formats::doa2u::{decrypt_cat, soft_body_offsets};
        let dir = std::env::var("DOA2U_DISC").expect("DOA2U_DISC");
        let archive = crate::formats::AfsArchive::from_file(format!("{dir}/datahdd2.afs")).unwrap();
        let mut costumes = std::collections::HashMap::new();
        for pair in std::env::var("DOA2U_RECORDS").expect("DOA2U_RECORDS").split(',') {
            let (fighter, stem) = pair.split_once(' ').unwrap();
            let mut cat = archive.get_by_name(&format!("{stem}.cat")).unwrap().to_vec();
            decrypt_cat(&mut cat).unwrap();
            let (base, offsets) = soft_body_offsets(&cat, 1).unwrap();
            println!("{stem}: {} kind 1 records", offsets.len());
            costumes.insert(fighter.to_string(), parse_chain_records(&cat, base, &offsets));
        }
        let text = std::fs::read_to_string(std::env::var("DOA2U_PAIRS").expect("DOA2U_PAIRS")).unwrap();
        let particles = |v: &[f32]| -> Vec<Particle> {
            v.chunks_exact(16)
                .map(|p| Particle {
                    position: Vec3::new(p[0], p[1], p[2]),
                    segment: Vec3::new(p[4], p[5], p[6]),
                    velocity: Vec3::new(p[8], p[9], p[10]),
                    last_pull: Vec3::new(p[12], p[13], p[14]),
                })
                .collect()
        };
        let mut records = Vec::new();
        let mut nodes: std::collections::HashMap<String, Vec<Mat4>> = Default::default();
        let mut before: Vec<Particle> = Vec::new();
        let (mut head, mut ring) = (0usize, Vec::<f32>::new());
        let mut results: std::collections::BTreeMap<usize, Vec<(f32, f32, f32)>> = Default::default();
        for line in text.lines() {
            let mut words = line.split_whitespace();
            let kind = words.next().unwrap();
            let rest: Vec<&str> = words.collect();
            match kind {
                "rec" => records.push((rest[0].to_string(), rest[1].parse::<usize>().unwrap())),
                "wind" => {
                    head = rest[0].parse().unwrap();
                    ring = rest[1..].iter().map(|v| v.parse().unwrap()).collect();
                }
                "matrices" => {
                    let m: Vec<f32> = rest[1..].iter().map(|v| v.parse().unwrap()).collect();
                    let game = |i: usize| Mat4::from_cols_slice(&m[i * 16..i * 16 + 16]);
                    let mut list = vec![Mat4::IDENTITY; 23];
                    for (i, node) in list.iter_mut().enumerate().take(15) {
                        *node = game(i);
                    }
                    list[19] = game(24);
                    list[20] = game(25);
                    nodes.insert(rest[0].to_string(), list);
                }
                "before" => before = particles(&rest[1..].iter().map(|v| v.parse().unwrap()).collect::<Vec<f32>>()),
                "after" => {
                    let k: usize = rest[0].parse().unwrap();
                    let after = particles(&rest[1..].iter().map(|v| v.parse().unwrap()).collect::<Vec<f32>>());
                    let (fighter, record) = &records[k];
                    let Some(chain) = costumes.get_mut(fighter).and_then(|c| c.get_mut(*record)) else { continue };
                    if chain.particles.len() != after.len() {
                        println!("record {k}: {} particles here, {} in the game", chain.particles.len(), after.len());
                        continue;
                    }
                    chain.ready = true;
                    chain.particles.copy_from_slice(&before);
                    let phase = (*record * 3) & 7;
                    let index = (head as i64 - phase as i64 - 1).rem_euclid(64) as usize * 4;
                    chain.step(&nodes[fighter], Vec3::new(ring[index], ring[index + 1], ring[index + 2]));
                    // A pair more than a step apart can still have the root in place. Its last
                    // pull, which the step computes from the root alone, gives it away.
                    let one_step = (chain.particles[0].last_pull - after[0].last_pull).length() < 1e-4;
                    if (chain.particles[0].position - after[0].position).length() < 0.0005 && one_step {
                        let error = |list: &[Particle]| list.iter().zip(&after).map(|(a, b)| (a.segment - b.segment).length()).sum::<f32>() / after.len() as f32 * 1000.0;
                        let worst = chain.particles.iter().zip(&after).map(|(a, b)| (a.position - b.position).length()).fold(0.0, f32::max) * 1000.0;
                        results.entry(k).or_default().push((error(&chain.particles), error(&before), worst));
                    } else {
                        results.entry(k).or_default();
                    }
                }
                _ => {}
            }
        }
        for (k, mut list) in results {
            let median = |v: &mut Vec<f32>| {
                v.sort_by(f32::total_cmp);
                v.get(v.len() / 2).copied().unwrap_or(f32::NAN)
            };
            let count = list.len();
            let mut ours: Vec<f32> = list.iter().map(|r| r.0).collect();
            let mut still: Vec<f32> = list.iter().map(|r| r.1).collect();
            let mut worst: Vec<f32> = list.drain(..).map(|r| r.2).collect();
            println!("record {k} {:?}: {count} pairs usable; segment error {:.2} mm, without stepping {:.2} mm, worst position {:.2} mm", records[k], median(&mut ours), median(&mut still), median(&mut worst));
        }
    }


    fn entry(data: &[u8], index: usize) -> &[u8] {
        let at = 8 + index * 8;
        let offset = read_u32(data, at).unwrap() as usize;
        let size = read_u32(data, at + 4).unwrap() as usize;
        &data[offset..offset + size]
    }

    fn shape(kind: u8, values: [f32; 6]) -> Shape {
        Shape { bone: 0, kind, values, slot: None }
    }

    #[test]
    fn limb_slots_turn_the_game_s_matrices() {
        let mut shapes = [shape(0, [0.0; 6])];
        shapes[0].bone = 0x82;
        resolve_limb_slots(&mut shapes);
        let mut nodes = vec![Mat4::IDENTITY; 23];
        nodes[6] = Mat4::from_translation(Vec3::new(1.0, 2.0, 3.0));
        let matrix = shapes[0].matrix(&nodes).unwrap();
        assert!(matrix.transform_point3(Vec3::ZERO).distance(Vec3::new(1.0, 2.0, 3.0)) < 1e-5);
        // Slot 0x82: quarter turns about X then Z, on the local axes.
        let expected = Mat4::from_rotation_x(std::f32::consts::FRAC_PI_2) * Mat4::from_rotation_z(std::f32::consts::FRAC_PI_2);
        assert!(matrix.transform_vector3(Vec3::X).distance(expected.transform_vector3(Vec3::X)) < 1e-5);
        shapes[0].bone = 0x80;
        shapes[0].slot = None;
        resolve_limb_slots(&mut shapes);
        assert!(shapes[0].matrix(&nodes).is_none());
    }

    #[test]
    fn shapes_push_points_out() {
        let mut point = Vec3::new(0.1, 0.0, 0.0);
        assert!(shape(3, [0.0, 0.0, 0.0, 0.5, 0.0, 0.0]).push_out(&mut point));
        assert!((point.length() - 0.5).abs() < 1e-5);

        let mut point = Vec3::new(0.0, 0.1, 0.0);
        assert!(shape(2, [0.0, 0.0, 0.0, 0.5, -1.0, 1.0]).push_out(&mut point));
        assert!((point.y - 0.5).abs() < 1e-5, "{point}");

        let mut point = Vec3::new(0.9, 0.0, 0.0);
        assert!(shape(0, [-1.0, 1.0, -1.0, 1.0, -1.0, 1.0]).push_out(&mut point));
        assert!((point.x - 1.0).abs() < 1e-5);

        let mut point = Vec3::new(5.0, 0.0, 0.0);
        assert!(!shape(3, [0.0, 0.0, 0.0, 0.5, 0.0, 0.0]).push_out(&mut point));
        assert!(shape(5, [-1.0, 1.0, -1.0, 1.0, -1.0, 1.0]).push_out(&mut point));
        assert_eq!(point.x, 1.0);
    }

    #[test]
    fn floor_lifts_a_segment() {
        let lifted = keep_above_floor(0.01, Vec3::new(0.0, -0.05, 0.0), 0.05, FLOOR);
        assert!(0.01 + lifted.y >= FLOOR - 1e-5, "{lifted}");
        assert!((lifted.length() - 0.05).abs() < 1e-5);
        assert_eq!(keep_above_floor(1.0, Vec3::new(0.0, -0.05, 0.0), 0.05, FLOOR), Vec3::new(0.0, -0.05, 0.0));
    }

    #[test]
    fn helena_chains_match_the_live_game() {
        let Ok(data) = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/doa3/loadfile.afs")) else {
            return;
        };
        // The game's state in a match had these same counts.
        let mut chains = parse_chains(entry(&data, 413));
        assert_eq!(chains.len(), 11);
        let ponytail = &chains[8];
        assert_eq!((ponytail.bone, ponytail.particles.len()), (1, 16));
        assert_eq!(ponytail.targets[0].object, 34);
        assert!(ponytail.targets[0].detached);
        let moved: usize =
            ponytail.targets[0].segments.iter().flatten().map(|p| p.vertices.len()).sum();
        assert_eq!(moved, 90);

        let nodes = vec![Mat4::from_translation(Vec3::new(0.0, 1.4, 0.0)); 23];
        for _ in 0..600 {
            for chain in &mut chains {
                chain.step(&nodes, Vec3::ZERO);
            }
        }
        for chain in &chains {
            for target in &chain.targets {
                for (_, position) in chain.vertex_positions(target, &nodes) {
                    assert!(position.is_finite());
                    assert!(position.length() < 2.0, "{position}");
                }
            }
        }
    }

    #[test]
    fn scrambled_header_still_finds_the_chains() {
        let Ok(data) = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/doa3/loadfile.afs")) else {
            return;
        };
        // Tina's costume 1: the game had 13 chains here.
        let chains = parse_chains(entry(&data, 161));
        assert_eq!(chains.len(), 13);
    }

    /// Ribbons and cords must not stretch back to the bone's origin.
    #[test]
    fn detached_objects_stay_in_one_piece() {
        let Ok(data) = std::fs::read("assets/doa3/loadfile.afs") else { return };
        let entry = |i: usize| {
            let at = 8 + i * 8;
            let offset = read_u32(&data, at).unwrap() as usize;
            &data[offset..offset + read_u32(&data, at + 4).unwrap() as usize]
        };
        let nodes = vec![Mat4::IDENTITY; 23];
        let (mut checked, mut longest) = (0, 0.0f32);
        for model in [296, 336] {
            let pieces = crate::formats::model::parse_xpr_model(entry(model)).unwrap();
            let mut chains = parse_chains(entry(model + 1));
            for _ in 0..120 {
                for chain in &mut chains {
                    chain.step(&nodes, Vec3::ZERO);
                }
            }
            for chain in &chains {
                for target in chain.targets.iter().filter(|t| t.detached) {
                    let piece = &pieces[target.object];
                    let rest: Vec<Vec3> = piece.vertices.iter().map(|v| Vec3::from(v.position)).collect();
                    let mut now = rest.clone();
                    for (vertex, position) in chain.anchored_vertices(target, &rest) {
                        now[vertex] = position;
                    }
                    for (vertex, position) in chain.vertex_positions(target, &nodes) {
                        now[vertex] = position;
                    }
                    for material in &piece.materials {
                        for triangle in material.indices.chunks_exact(3) {
                            for (a, b) in [(0, 1), (1, 2), (2, 0)] {
                                let (a, b) = (triangle[a] as usize, triangle[b] as usize);
                                let grown = (now[a] - now[b]).length() - (rest[a] - rest[b]).length();
                                longest = longest.max(grown);
                            }
                        }
                    }
                    checked += 1;
                }
            }
        }
        println!("{checked} detached objects, longest growth {longest}");
        assert!(checked >= 4);
        assert!(longest < 0.1, "{longest}");
    }

    #[test]
    fn worn_props_are_read_and_empty_entries_refused() {
        let Ok(data) = std::fs::read("assets/doa3/loadfile.afs") else { return };
        let entry = |i: usize| {
            let at = 8 + i * 8;
            let offset = read_u32(&data, at).unwrap() as usize;
            &data[offset..offset + read_u32(&data, at + 4).unwrap() as usize]
        };
        let objects = |props: &[WornProp]| props.iter().map(|p| (p.bone, p.object)).collect::<Vec<_>>();
        assert_eq!(objects(&parse_props(entry(652), 67)), [(3, 14), (9, 30), (1, 0)]);
        assert_eq!(objects(&parse_props(entry(486), 71)), [(1, 1)]);
        assert_eq!(objects(&parse_props(entry(156), 62)), []);
        assert_eq!(objects(&parse_props(entry(199), 63)), []);
    }

    #[test]
    fn overlays_place_kasumis_knife_and_hair() {
        let Ok(data) = std::fs::read("assets/doa3/loadfile.afs") else { return };
        let at = 8 + 297 * 8;
        let offset = read_u32(&data, at).unwrap() as usize;
        let cat = &data[offset..offset + read_u32(&data, at + 4).unwrap() as usize];
        let found: Vec<_> =
            parse_overlays(cat, 72).iter().map(|o| (o.prop.object, o.prop.bone, o.translucent)).collect();
        assert_eq!(found, [(0, 1, true), (8, 2, false)]);
    }
}
