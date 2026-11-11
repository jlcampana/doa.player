//! Sheets and tubes of cloth: DOA3's skirts, sleeves and trouser legs.
//!
//! Section type 2 of the CAT. Setup `sub_0003ADF0`, first frame `sub_0003FD20`,
//! update `sub_000403F0`, vertex writes `sub_0003F890` and `sub_0003FA50`.
//! See `docs/doa3-internals.md`.

use glam::{Mat4, Vec3};

use super::doa3_chains::{
    find_sections, keep_above_floor, parse_shapes, read_f32, resolve_limb_slots, read_u32, read_vec3, with_length, Shape, ANGLE_UNITS,
    NO_VERTEX, POINT_LIST_END, VERTEX_STRIDE,
};

const CLOTH_SECTION: u8 = 2;
const MAX_COLUMNS: u8 = 16;
const MAX_ROWS: u8 = 24;
const MAX_MODE: u8 = 1;
const MAX_AXIS: u8 = 2;
const MAX_BONE: u8 = 22;
const NO_OBJECT: u8 = 0xFF;
const AUTO_PIVOT: u8 = 0xFF;
const NO_ANGLE: u32 = u32::MAX;
/// A column length of 99 in the file means the record's default.
const USE_DEFAULT_LENGTH: u32 = 0x42C6_0000;

/// First row is a closed ring over one column fewer; the rest are open.
const FLAG_RING_FIRST_ROW: u8 = 0x02;
/// Also collides the two end columns with the shapes.
const FLAG_COLLIDE_ENDS: u8 = 0x04;
/// Neighbouring columns also push apart, not only pull together.
const FLAG_KEEP_WIDTH: u8 = 0x10;
/// Adds a cap row that closes the end of a tube.
const FLAG_CAP: u8 = 0x40;
/// Vertices are written with their own offset from the particle.
const FLAG_OFFSET_VERTICES: u8 = 0x80;
/// DOA2U: columns may start on a vertex of the object.
const FLAG_VERTEX_ROOTS: u8 = 0x08;
const VERTEX_BYTES: usize = 32;
/// Bits the game reads but that no real record uses. Records with them were
/// not ported, and in practice these are false sections.
const UNSUPPORTED_FLAGS: u8 = 0x29;

const FLOOR: f32 = 0.002;
const FLOOR_PER_ROW: f32 = 0.001;
const CAP_DEPTH: f32 = 0.005;
const QUARTER_TURN: i32 = 0x4000;
const HALF_TURN: i32 = 0x8000;
const FULL_TURN: i32 = 0x10000;
/// Limit of the joint angle used to correct the stiffness, about 60 degrees.
const MAX_STIFFNESS_ANGLE: i32 = 0x2AAA;
/// Nodes with their own point, past the 15 main nodes (`0x2E93B8`).
const FIRST_END_POINT: usize = 15;
const END_POINTS: [(usize, Vec3); 4] = [
    (5, Vec3::new(0.2, 0.0, 0.0)),
    (11, Vec3::new(-0.2, 0.0, 0.0)),
    (3, Vec3::new(0.0, 0.0, 0.2)),
    (9, Vec3::new(0.0, 0.0, 0.2)),
];

pub(super) fn to_angle(radians: f32) -> i32 {
    (radians * ANGLE_UNITS) as i32
}

pub(super) fn from_angle(angle: i32) -> f32 {
    angle as f32 / ANGLE_UNITS
}

/// Wraps to the range the game keeps: -0x8000 to 0x8000.
pub(super) fn wrap_angle(angle: i32) -> i32 {
    let angle = angle & (FULL_TURN - 1);
    if angle > HALF_TURN {
        angle - FULL_TURN
    } else {
        angle
    }
}

/// Stops a tube folding through the joint below it (`sub_0003CCB0`).
#[derive(Debug, Clone, Copy)]
struct JointFold {
    axes: (usize, usize),
    centre: (f32, f32),
    max_angle: i32,
    min_radius: f32,
    max_radius: f32,
    node_from: u8,
    node_to: u8,
    bend_start: f32,
    bend_end: f32,
    /// Its sign says on which side the joint folds.
    limit: f32,
    stiffness_factor: f32,
    /// The component that runs along the tube. Z in DOA3's code; DOA2U's update chooses
    /// it from the same field as `axes`, and its leg bones run along Y.
    length_axis: usize,
    /// DOA2U's form of the limit, in `limit_tip`.
    moved_centre: bool,
}

impl JointFold {
    fn parse(cat: &[u8], at: usize) -> Option<Self> {
        let axes = match read_u32(cat, at)? {
            0 => (2, 1),
            1 => (0, 2),
            _ => (0, 1),
        };
        Some(Self {
            axes,
            centre: (read_f32(cat, at + 4)?, read_f32(cat, at + 8)?),
            max_angle: read_u32(cat, at + 0xC)? as i32,
            min_radius: read_f32(cat, at + 0x10)?,
            max_radius: read_f32(cat, at + 0x14)?,
            node_from: *cat.get(at + 0x18)?,
            node_to: *cat.get(at + 0x19)?,
            bend_start: read_f32(cat, at + 0x20)?,
            bend_end: read_f32(cat, at + 0x24)?,
            limit: read_f32(cat, at + 0x28)?,
            stiffness_factor: read_f32(cat, at + 0x30)?,
            length_axis: 2,
            moved_centre: false,
        })
    }

    /// The joint's angle in the bone's space, about y.
    fn angle(&self, nodes: &[Mat4], to_bone: &Mat4) -> i32 {
        let difference = joint_point(self.node_to, nodes) - joint_point(self.node_from, nodes);
        let local = to_bone.transform_vector3(difference);
        let angle = wrap_angle(to_angle(local.x.atan2(local.z)));
        if self.limit < 0.0 {
            wrap_angle(angle - HALF_TURN)
        } else {
            angle
        }
    }

    /// Keeps a segment's tip within an angle and a radius of the tube's axis.
    fn limit_tip(&self, rest_angle: i32, point: Vec3, segment: &mut Vec3) {
        let (u, v) = self.axes;
        // DOA2U measures from a centre moved towards the column by the small radius, with
        // only an outer limit (`sub_0004DF20`).
        let centre = if self.moved_centre {
            let (sin, cos) = from_angle(rest_angle).sin_cos();
            (self.centre.0 + cos * self.min_radius, self.centre.1 + sin * self.min_radius)
        } else {
            self.centre
        };
        let mut a = point[u] + segment[u] - centre.0;
        let mut b = point[v] + segment[v] - centre.1;
        let delta = wrap_angle(to_angle(b.atan2(a)) - rest_angle);
        let turn = if delta > self.max_angle {
            Some(self.max_angle - delta)
        } else if delta < -self.max_angle {
            Some(-(self.max_angle + delta))
        } else {
            None
        };
        if let Some(turn) = turn {
            let (sin, cos) = from_angle(turn).sin_cos();
            (a, b) = (a * cos - b * sin, a * sin + b * cos);
        }
        let radius = (a * a + b * b).sqrt();
        let scale = if radius == 0.0 {
            None
        } else if self.moved_centre {
            let reach = self.max_radius - self.min_radius;
            (radius > reach).then(|| reach / radius)
        } else if radius > self.max_radius {
            Some(self.max_radius / radius)
        } else if radius < self.min_radius {
            Some(self.min_radius / radius)
        } else {
            None
        };
        if turn.is_none() && scale.is_none() {
            return;
        }
        let scale = scale.unwrap_or(1.0);
        segment[u] = a * scale - point[u] + centre.0;
        segment[v] = b * scale - point[v] + centre.1;
    }
}

/// `0x480290` holds the main nodes, `0x454438` the points past them.
pub(super) fn joint_point(node: u8, nodes: &[Mat4]) -> Vec3 {
    let node = node as usize;
    let origin = |index: usize| nodes.get(index).map_or(Vec3::ZERO, |m| m.w_axis.truncate());
    match node.checked_sub(FIRST_END_POINT) {
        None => origin(node),
        Some(index) => END_POINTS.get(index).map_or(Vec3::ZERO, |&(bone, offset)| {
            nodes.get(bone).map_or(Vec3::ZERO, |m| m.transform_point3(offset))
        }),
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct ClothParticle {
    position: Vec3,
    segment: Vec3,
    velocity: Vec3,
    last_pull: Vec3,
}

/// A vertex offset from its particle, and the vertices at that place.
#[derive(Debug, Clone)]
pub(super) struct OffsetPoint {
    pub(super) offset: Vec3,
    pub(super) vertices: Vec<usize>,
}

#[derive(Debug, Clone)]
enum ClothVertices {
    /// One list per particle, plus the cap's.
    Direct(Vec<Vec<usize>>),
    /// One list per particle.
    Offset(Vec<Vec<OffsetPoint>>),
}

#[derive(Debug, Clone)]
pub struct SoftCloth {
    /// Height of the ground under the sheet. The viewer's is 0; the game's comes from
    /// the stage (`sub_00146920`), sampled once per sheet and frame.
    pub floor_height: f32,
    pub bone: usize,
    pub object: usize,
    is_ring: bool,
    columns: usize,
    rows: usize,
    pivot: usize,
    flags: u8,
    gravity: f32,
    pull: f32,
    damping: f32,
    bend: f32,
    stiffness: f32,
    wind_factor: f32,
    max_force: f32,
    row_lengths: Vec<f32>,
    column_lengths: Vec<f32>,
    roots: Vec<Vec3>,
    directions: Vec<Vec3>,
    angles: Vec<i32>,
    shapes: Vec<Shape>,
    fold: Option<JointFold>,
    vertices: ClothVertices,
    particles: Vec<ClothParticle>,
    ready: bool,
    /// DOA2U adds the wind to the pull, so it also feeds the velocity. DOA3 adds
    /// it to the force.
    wind_in_pull: bool,
    /// Rows that stay a closed ring under `FLAG_RING_FIRST_ROW`: the top four bits
    /// of the record's flags plus one in DOA2U. DOA3 keeps it to the first row.
    ring_rows: usize,
    /// DOA2U flag 0x08: columns rooted on a vertex of the object, by index. These
    /// can be vertices that another sheet writes, which chains the sheets.
    vertex_roots: Vec<Option<usize>>,
}

/// Reads every cloth record in a CAT that moves vertices.
pub fn parse_cloth(cat: &[u8]) -> Vec<SoftCloth> {
    let Some((base, sections)) = find_sections(cat) else { return Vec::new() };
    sections
        .iter()
        .filter(|(kind, _)| *kind == CLOTH_SECTION)
        .filter(|&&(_, offset)| plausible_record(cat, base + offset))
        .filter_map(|&(_, offset)| parse_record(cat, base, base + offset))
        .collect()
}

/// Reads cloth records at offsets from `base`, for files whose section list is
/// known to be real, so the limits of `plausible_record` do not apply. DOA2U has
/// sheets of 17 columns and flag 0x08. Records that cannot be read are left out.
pub fn parse_cloth_records(cat: &[u8], base: usize, offsets: &[usize]) -> Vec<SoftCloth> {
    offsets
        .iter()
        .filter_map(|&offset| {
            let at = base + offset;
            let mut cloth = parse_record(cat, base, at)?;
            resolve_limb_slots(&mut cloth.shapes);
            if let Some(fold) = &mut cloth.fold {
                fold.length_axis = 3 - fold.axes.0 - fold.axes.1;
                fold.moved_centre = true;
            }
            let all_flags = read_u32(cat, at + 8)?;
            let high_flags = (all_flags >> 8) as u8;
            cloth.wind_in_pull = true;
            cloth.ring_rows = (all_flags >> 28) as usize + 1;
            // Flags 0x180 both write vertices with their own offsets (`sub_00050A50`).
            if high_flags & 1 != 0 && cloth.flags & FLAG_OFFSET_VERTICES == 0 {
                let list_at = at_pointer(cat, base, at, 0x44)?;
                if let Some(offsets) = at_pointer(cat, base, at, 0x58) {
                    let caps = cloth.columns * cloth.rows;
                    cloth.vertices = ClothVertices::Offset(parse_offset_vertices(cat, list_at, offsets, caps)?);
                }
            }
            if cloth.flags & FLAG_VERTEX_ROOTS != 0 {
                let list = at_pointer(cat, base, at, 0x50)?;
                cloth.vertex_roots = (0..cloth.columns)
                    .map(|c| read_u32(cat, list + c * 4).map(|v| (v != u32::MAX).then_some(if super::doa3_chains::big_endian() { v as usize } else { v as usize / VERTEX_BYTES })))
                    .collect::<Option<_>>()?;
            }
            Some(cloth)
        })
        .collect()
}

/// DOA4's sheets. Its record is DOA2U's with big-endian words and two changes at the start:
///
/// | DOA3, DOA2U | DOA4 |
/// |---|---|
/// | +4 object, +5 columns, +6 rows, +7 pivot | +4 object (16 bits), +6 columns, +7 rows, +8 pivot |
/// | +8 flags, +0xC the floats, +0x34 the pointers | +0xC flags, +0x10 the floats, +0x38 the pointers |
///
/// Kasumi's skirt flaps show it: DOA3's record 297 and DOA4's `kas00` hold the same ten floats
/// (0.065, 0.02, 1.2, 0.45, 0, 0.01, 0.3, 0, 0.7, 0.05), 5 columns of 6 rows on bone 2, flags
/// 0x14 there and 0x194 here. Each record is laid out again in DOA2U's form after the file's
/// end and read by its parser; the lists it points to stay where they are. Vertices are left
/// as the file's words, for `map_vertices`.
pub fn parse_cloth_records_doa4(cat: &[u8], base: usize, offsets: &[usize]) -> Vec<SoftCloth> {
    const TAIL: usize = 0x60;
    let mut copy = cat.to_vec();
    let mut moved = Vec::new();
    for &offset in offsets {
        let at = base + offset;
        let Some(record) = cat.get(at..at + 0x10 + TAIL) else { continue };
        // The object's high byte has nowhere to go; no model has 256 objects.
        if record[4] != 0 {
            continue;
        }
        moved.push(copy.len() - base);
        copy.extend([record[0], record[1], record[2], record[3], record[5], record[6], record[7], record[8]]);
        copy.extend(&record[0xC..0x10 + TAIL]);
    }
    super::doa3_chains::with_big_endian(|| parse_cloth_records(&copy, base, &moved))
}

fn at_pointer(cat: &[u8], base: usize, at: usize, offset: usize) -> Option<usize> {
    let value = read_u32(cat, at + offset)? as usize;
    (value != 0).then_some(base + value)
}

fn floats(cat: &[u8], at: usize, count: usize) -> Option<Vec<f32>> {
    (0..count).map(|i| read_f32(cat, at + i * 4)).collect()
}

/// Whether a section found by scanning a DOA3 CAT looks like a real cloth record.
fn plausible_record(cat: &[u8], at: usize) -> bool {
    let Some(header) = cat.get(at..at + 9) else { return false };
    let (mode, axis, bone, object, columns, rows) = (header[0], header[1], header[2], header[4], header[5], header[6]);
    // The flags word's low byte, wherever the file's byte order puts it.
    let Some(flags) = read_u32(cat, at + 8).map(|word| word as u8) else { return false };
    mode <= MAX_MODE
        && axis <= MAX_AXIS
        && bone <= MAX_BONE
        && object != NO_OBJECT
        && (1..=MAX_COLUMNS).contains(&columns)
        && (1..=MAX_ROWS).contains(&rows)
        && flags & UNSUPPORTED_FLAGS == 0
}

fn parse_record(cat: &[u8], base: usize, at: usize) -> Option<SoftCloth> {
    let header = cat.get(at..at + 9)?;
    let (mode, axis, bone, object, columns, rows, pivot) = (header[0], header[1], header[2], header[4], header[5], header[6], header[7]);
    let flags = read_u32(cat, at + 8)? as u8;
    if columns == 0 || rows == 0 {
        return None;
    }
    let (columns, rows) = (columns as usize, rows as usize);
    let pointer = |offset: usize| -> Option<Option<usize>> {
        let value = read_u32(cat, at + offset)? as usize;
        Some((value != 0).then_some(base + value))
    };
    let default_length = read_f32(cat, at + 0xC)?;
    let radius = read_f32(cat, at + 0x1C)?;
    let reach = read_f32(cat, at + 0x20)?;

    let row_lengths = match pointer(0x34)? {
        Some(list) => floats(cat, list, rows)?,
        None => vec![default_length; rows],
    };
    let column_lengths = match pointer(0x38)? {
        Some(list) => (0..columns)
            .map(|i| {
                let value = read_u32(cat, list + i * 4)?;
                Some(if value == USE_DEFAULT_LENGTH { default_length } else { f32::from_bits(value) })
            })
            .collect::<Option<Vec<f32>>>()?,
        None => vec![default_length; columns],
    };

    let roots_at = pointer(0x40)??;
    let roots: Vec<Vec3> = (0..columns).map(|i| read_vec3(cat, roots_at + i * 16)).collect::<Option<_>>()?;
    let angle_list = pointer(0x3C)?;
    let mut angles = Vec::with_capacity(columns);
    let mut directions = Vec::with_capacity(columns);
    for (i, root) in roots.iter().enumerate() {
        let given = match angle_list {
            Some(list) => read_u32(cat, list + i * 4)?,
            None => NO_ANGLE,
        };
        let given = (given != NO_ANGLE).then_some(given as i32);
        // With a given angle, axes 1 and 2 turn it by a quarter.
        let (angle, direction): (i32, fn(f32, f32, f32) -> Vec3) = match axis {
            0 => (given.unwrap_or_else(|| to_angle(root.y.atan2(root.z))), |s, c, d| Vec3::new(-d, s, c)),
            1 => (
                given.map_or_else(|| to_angle(root.z.atan2(root.x)), |a| a + QUARTER_TURN),
                |s, c, d| Vec3::new(c, -d, s),
            ),
            _ => (
                given.map_or_else(|| to_angle(root.y.atan2(root.x)), |a| a + QUARTER_TURN),
                |s, c, d| Vec3::new(c, s, -d),
            ),
        };
        let (sin, cos) = from_angle(angle).sin_cos();
        angles.push(angle);
        directions.push(direction(sin * radius, cos * radius, reach));
    }

    let shapes = match pointer(0x48)? {
        Some(list) => parse_shapes(cat, list)?,
        None => Vec::new(),
    };
    let fold = match pointer(0x54)? {
        Some(block) => Some(JointFold::parse(cat, block)?),
        None => None,
    };
    let list_at = pointer(0x44)??;
    let vertices = match (flags & FLAG_OFFSET_VERTICES != 0, pointer(0x58)?) {
        (true, Some(offsets)) => ClothVertices::Offset(parse_offset_vertices(cat, list_at, offsets, columns * rows)?),
        _ => {
            let caps = if flags & FLAG_CAP != 0 { columns } else { 0 };
            ClothVertices::Direct(parse_direct_vertices(cat, list_at, columns * rows + caps)?)
        }
    };

    let pivot = match (pivot, mode) {
        (AUTO_PIVOT, 1) => columns / 2,
        (AUTO_PIVOT, _) => 0,
        (value, _) => value as usize,
    };
    Some(SoftCloth {
        bone: bone as usize,
        object: object as usize,
        is_ring: mode == 0,
        columns,
        rows,
        pivot,
        flags,
        gravity: read_f32(cat, at + 0x10)? * 0.5,
        pull: read_f32(cat, at + 0x14)?,
        damping: read_f32(cat, at + 0x18)?,
        bend: read_f32(cat, at + 0x24)?,
        stiffness: read_f32(cat, at + 0x28)?,
        wind_factor: read_f32(cat, at + 0x2C)?,
        max_force: read_f32(cat, at + 0x30)?,
        row_lengths,
        column_lengths,
        roots,
        directions,
        angles,
        shapes,
        fold,
        vertices,
        particles: vec![ClothParticle::default(); columns * (rows + 1)],
        floor_height: 0.0,
        ready: false,
        wind_in_pull: false,
        ring_rows: 1,
        vertex_roots: Vec::new(),
    })
}

/// Reads one vertex list. The low bit says another follows. `None` inside the
/// result is the list's end.
fn next_vertices(cat: &[u8], at: &mut usize) -> Option<Option<Vec<usize>>> {
    let mut value = read_u32(cat, *at)?;
    if value == NO_VERTEX {
        return Some(None);
    }
    let mut vertices = Vec::new();
    *at += 4;
    while value & 1 != 0 {
        vertices.push(super::doa3_chains::vertex_of(value & !1));
        value = read_u32(cat, *at)?;
        *at += 4;
    }
    vertices.push(super::doa3_chains::vertex_of(value));
    Some(Some(vertices))
}

pub(super) fn parse_direct_vertices(cat: &[u8], mut at: usize, count: usize) -> Option<Vec<Vec<usize>>> {
    let mut lists = Vec::with_capacity(count);
    for _ in 0..count {
        match next_vertices(cat, &mut at)? {
            Some(vertices) => lists.push(vertices),
            None => lists.push(Vec::new()),
        }
    }
    Some(lists)
}

pub(super) fn parse_offset_vertices(cat: &[u8], mut at: usize, mut offsets: usize, count: usize) -> Option<Vec<Vec<OffsetPoint>>> {
    let mut lists = Vec::with_capacity(count);
    for _ in 0..count {
        let mut points = Vec::new();
        while read_u32(cat, offsets)? != POINT_LIST_END {
            let offset = read_vec3(cat, offsets)?;
            offsets += 16;
            let vertices = next_vertices(cat, &mut at)?.unwrap_or_default();
            points.push(OffsetPoint { offset, vertices });
        }
        offsets += 16;
        lists.push(points);
    }
    Some(lists)
}

impl SoftCloth {
    /// Moves the roots of DOA2U's flag 0x08 onto the object's current vertices, in
    /// the bone's space. Call it before `step`, after any sheet that writes them.
    pub fn follow_vertices(&mut self, vertex: impl Fn(usize) -> Option<Vec3>) {
        for (root, source) in self.roots.iter_mut().zip(&self.vertex_roots) {
            if let Some(position) = source.and_then(&vertex) {
                *root = position;
            }
        }
    }

    /// Renumbers the vertices the sheet writes and those its columns start on.
    pub fn map_vertices(&mut self, map: impl Fn(usize) -> usize) {
        match &mut self.vertices {
            ClothVertices::Direct(lists) => lists.iter_mut().flatten().for_each(|v| *v = map(*v)),
            ClothVertices::Offset(lists) => lists.iter_mut().flatten().flat_map(|p| p.vertices.iter_mut()).for_each(|v| *v = map(*v)),
        }
        self.vertex_roots.iter_mut().flatten().for_each(|v| *v = map(*v));
    }

    /// The vertices the sheet writes.
    pub fn written_vertices(&self) -> Vec<usize> {
        match &self.vertices {
            ClothVertices::Direct(lists) => lists.iter().flatten().copied().collect(),
            ClothVertices::Offset(lists) => lists.iter().flatten().flat_map(|p| p.vertices.iter().copied()).collect(),
        }
    }

    pub fn has_vertex_roots(&self) -> bool {
        !self.vertex_roots.is_empty()
    }

    /// Whether any column starts on the front of the torso.
    pub fn is_on_torso_front(&self) -> bool {
        self.bone == 0 && self.roots.iter().any(|root| root.z > 0.0)
    }

    fn index(&self, column: usize, row: usize) -> usize {
        column * (self.rows + 1) + row
    }

    /// Advances one 60 Hz game frame. `wind` is the game's stage wind before
    /// this record's factor.
    pub fn step(&mut self, nodes: &[Mat4], wind: Vec3) {
        let Some(&bone) = nodes.get(self.bone) else { return };
        if !self.ready {
            self.settle(nodes, &bone);
            self.ready = true;
            return;
        }
        let wind = wind * self.wind_factor;
        let to_bone = bone.inverse();
        let joint_angle = self.fold.map(|fold| fold.angle(nodes, &to_bone));
        let stiffness: Vec<f32> =
            self.roots.iter().map(|root| self.column_stiffness(root.x, joint_angle)).collect();
        let mut points: Vec<Vec3> = self.roots.iter().map(|r| bone.transform_point3(*r)).collect();
        let mut directions: Vec<Vec3> = self.directions.iter().map(|d| bone.transform_vector3(*d)).collect();
        let length_axis = self.fold.map_or(2, |fold| fold.length_axis);
        let mut root_z: Vec<f32> = self.roots.iter().map(|r| r[length_axis]).collect();
        let mut segments = vec![Vec3::ZERO; self.columns];
        let mut floor = self.floor_height + FLOOR;

        for row in 0..self.rows {
            let (columns, is_ring) = self.row_shape(row);
            for c in 0..columns {
                let at = self.index(c, row);
                let particle = self.particles[at];
                let length = self.column_lengths[c];
                let pull = particle.position - points[c] + if self.wind_in_pull { wind } else { Vec3::ZERO };
                let mut velocity = (particle.velocity + pull - particle.last_pull) * self.damping;
                velocity.y -= self.gravity;

                let mut force = pull * self.pull + velocity + directions[c];
                directions[c] *= self.bend;
                let target = if row == 0 { directions[c] } else { self.particles[at - 1].segment };
                force += (target - particle.segment) * stiffness[c];
                if !self.wind_in_pull {
                    force += wind;
                }
                let size = force.length();
                if size > self.max_force {
                    force *= self.max_force / size;
                }
                let segment = with_length(particle.segment + force, length);
                segments[c] = keep_above_floor(points[c].y, segment, length, floor);
                self.particles[at].velocity = velocity;
                self.particles[at].last_pull = pull;
            }
            self.keep_column_spacing(row, columns, is_ring, &points, &mut segments);
            if let (Some(fold), Some(angle)) = (self.fold, joint_angle) {
                self.apply_fold(&fold, angle, columns, &bone, &to_bone, &mut root_z, &mut points, &mut segments);
            }
            self.collide(nodes, columns, is_ring, &points, &mut segments);
            self.finish_row(row, columns, &mut points, &segments);
            floor -= FLOOR_PER_ROW;
        }
        self.write_last_row(&points);
    }

    /// Places the cloth at rest (`sub_0003FD20`).
    fn settle(&mut self, nodes: &[Mat4], bone: &Mat4) {
        let mut points: Vec<Vec3> = self.roots.iter().map(|r| bone.transform_point3(*r)).collect();
        let mut directions: Vec<Vec3> = self.directions.iter().map(|d| bone.transform_vector3(*d)).collect();
        let mut segments = vec![Vec3::ZERO; self.columns];
        let rest_velocity = if self.damping > 0.0 && self.damping < 1.0 {
            -self.gravity / (1.0 - self.damping)
        } else {
            -self.gravity
        };
        for row in 0..self.rows {
            let (columns, is_ring) = self.row_shape(row);
            for c in 0..columns {
                let at = self.index(c, row);
                let velocity = Vec3::new(0.0, rest_velocity, 0.0);
                let start = directions[c] + velocity;
                let target = if row == 0 { directions[c] } else { self.particles[at - 1].segment };
                let segment = start + (target - start) * self.stiffness;
                let length = self.column_lengths[c];
                let segment = with_length(segment, length);
                segments[c] = keep_above_floor(points[c].y, segment, length, FLOOR);
                directions[c] *= self.bend;
                self.particles[at].velocity = velocity;
                self.particles[at].last_pull = Vec3::ZERO;
            }
            self.keep_column_spacing(row, columns, is_ring, &points, &mut segments);
            self.collide(nodes, columns, is_ring, &points, &mut segments);
            self.finish_row(row, columns, &mut points, &segments);
        }
        self.write_last_row(&points);
    }

    /// Stiffness gives way on the side of the tube that a bent joint squeezes.
    fn column_stiffness(&self, root_x: f32, joint_angle: Option<i32>) -> f32 {
        let (Some(fold), Some(angle)) = (self.fold, joint_angle) else { return self.stiffness };
        let angle = if fold.limit < 0.0 { wrap_angle(angle - HALF_TURN) } else { angle };
        let squeezed = (angle >= 0 && root_x > 0.0) || (angle < 0 && root_x < 0.0);
        if !squeezed {
            return self.stiffness;
        }
        let angle = angle.clamp(-MAX_STIFFNESS_ANGLE, MAX_STIFFNESS_ANGLE);
        self.stiffness - from_angle(angle).tan() * fold.stiffness_factor * root_x
    }

    /// Columns and shape of a row, which the ring flag changes for row 0.
    fn row_shape(&self, row: usize) -> (usize, bool) {
        if self.flags & FLAG_RING_FIRST_ROW == 0 {
            (self.columns, self.is_ring)
        } else if row < self.ring_rows {
            (self.columns - 1, true)
        } else {
            (self.columns, false)
        }
    }

    /// `sub_00039DD0` between neighbouring columns.
    fn keep_column_spacing(&self, row: usize, columns: usize, is_ring: bool, points: &[Vec3], segments: &mut [Vec3]) {
        let length = self.row_lengths[row];
        let both_ways = self.flags & FLAG_KEEP_WIDTH != 0;
        let mut pair = |right: usize, left: usize| {
            let difference = (points[right] + segments[right]) - (points[left] + segments[left]);
            let size = difference.length();
            if !both_ways && size <= length {
                return;
            }
            let divisor = if size == 0.0 { 1.0 } else { size };
            let change = difference * ((divisor - length) * 0.5 / divisor);
            segments[right] -= change;
            segments[left] += change;
        };
        if is_ring {
            for c in 0..columns {
                pair(c, (c + columns - 1) % columns);
            }
        } else {
            let pivot = self.pivot.min(columns.saturating_sub(1));
            for c in 0..pivot {
                pair(c, c + 1);
            }
            for c in ((pivot + 1)..columns).rev() {
                pair(c, c - 1);
            }
        }
    }

    /// Bends the tube around the joint for the fold's limits, then unbends it.
    #[allow(clippy::too_many_arguments)]
    fn apply_fold(
        &self,
        fold: &JointFold,
        joint_angle: i32,
        columns: usize,
        bone: &Mat4,
        to_bone: &Mat4,
        root_z: &mut [f32],
        points: &mut [Vec3],
        segments: &mut [Vec3],
    ) {
        for c in 0..columns {
            let mut point = to_bone.transform_point3(points[c]);
            let mut segment = to_bone.transform_vector3(segments[c]);
            let distance = root_z[c].abs();
            let turn = (distance > fold.bend_start).then(|| {
                let reach = distance.min(fold.bend_end);
                let pivot = if fold.limit < 0.0 { -reach } else { reach };
                let angle = if distance < fold.bend_end {
                    ((distance - fold.bend_start) / (fold.bend_end - fold.bend_start) * joint_angle as f32) as i32
                } else {
                    joint_angle
                };
                (angle, pivot)
            });
            if let Some((angle, pivot)) = turn {
                rotate_xz(&mut point, angle, pivot);
                rotate_xz(&mut segment, angle, 0.0);
            }
            let limit = root_z[c] + fold.limit;
            let tip = root_z[c] + segment[fold.length_axis];
            root_z[c] = tip;
            let past_limit = if fold.limit >= 0.0 { limit > tip } else { limit < tip };
            if past_limit {
                segment[fold.length_axis] += limit - tip;
                root_z[c] = limit;
            }
            fold.limit_tip(self.angles[c], point, &mut segment);
            if let Some((angle, pivot)) = turn {
                rotate_xz(&mut point, -angle, pivot);
                rotate_xz(&mut segment, -angle, 0.0);
            }
            points[c] = bone.transform_point3(point);
            segments[c] = bone.transform_vector3(segment);
        }
    }

    /// `sub_0003DA20`, and `sub_0003D7A0` for the end columns.
    fn collide(&self, nodes: &[Mat4], columns: usize, is_ring: bool, points: &[Vec3], segments: &mut [Vec3]) {
        if self.shapes.is_empty() || columns == 0 {
            return;
        }
        if !is_ring && self.flags & FLAG_COLLIDE_ENDS != 0 {
            let length = self.column_lengths[columns - 1];
            for c in [0, columns - 1] {
                // DOA2U leaves the moved tip where it is (`sub_0004BFE0`).
                let length = (!self.wind_in_pull).then_some(length);
                segments[c] = collide_tip(&self.shapes, nodes, points[c], segments[c], length);
            }
        }
        let mut tips: Vec<Vec3> = points[..columns].iter().zip(segments.iter()).map(|(p, s)| *p + *s).collect();
        for shape in &self.shapes {
            let Some(matrix) = shape.matrix(nodes) else { continue };
            let inverse = matrix.inverse();
            let mut local: Vec<Vec3> = tips.iter().map(|t| inverse.transform_point3(*t)).collect();
            let first_pair = if is_ring { 0 } else { 1 };
            for right in first_pair..columns {
                let left = (right + columns - 1) % columns;
                let (a, b) = pair_mut(&mut local, left, right);
                shape.push_edge_out(a, b);
            }
            for (tip, position) in tips.iter_mut().zip(&local) {
                *tip = matrix.transform_point3(*position);
            }
        }
        for c in 0..columns {
            segments[c] = tips[c] - points[c];
        }
    }

    fn finish_row(&mut self, row: usize, columns: usize, points: &mut [Vec3], segments: &[Vec3]) {
        for c in 0..columns {
            let at = self.index(c, row);
            self.particles[at].position = points[c];
            self.particles[at].segment = segments[c];
            points[c] += segments[c];
        }
        if columns < self.columns {
            // The ring's last column repeats its first one.
            let (first, last) = (self.index(0, row), self.index(columns, row));
            self.particles[last].position = self.particles[first].position;
            self.particles[last].segment = self.particles[first].segment;
            points[columns] = points[0];
        }
    }

    fn write_last_row(&mut self, points: &[Vec3]) {
        for (c, point) in points.iter().enumerate() {
            let at = self.index(c, self.rows);
            self.particles[at].position = *point;
        }
    }

    /// Vertex positions in the bone's space.
    pub fn vertex_positions(&self, nodes: &[Mat4]) -> Vec<(usize, Vec3)> {
        let to_bone = nodes.get(self.bone).map_or(Mat4::IDENTITY, Mat4::inverse);
        let local: Vec<Vec3> = (0..self.columns)
            .flat_map(|c| (1..=self.rows).map(move |row| (c, row)))
            .map(|(c, row)| to_bone.transform_point3(self.particles[self.index(c, row)].position))
            .collect();
        let mut out = Vec::new();
        match &self.vertices {
            ClothVertices::Direct(lists) => {
                let caps = self.cap_positions(&local);
                for (list, position) in lists.iter().zip(local.iter().chain(&caps)) {
                    out.extend(list.iter().map(|&vertex| (vertex, *position)));
                }
            }
            ClothVertices::Offset(lists) => {
                let turns = self.row_turns(&local);
                let mut next_turn = 0;
                for (list, position) in lists.iter().zip(&local) {
                    for point in list {
                        let mut offset = point.offset;
                        if let Some(turns) = &turns {
                            let (sin, cos) = turns[next_turn];
                            next_turn = (next_turn + 1) % self.rows;
                            offset = Vec3::new(offset.x * cos + offset.z * sin, offset.y, offset.z * cos - offset.x * sin);
                        }
                        out.extend(point.vertices.iter().map(|&vertex| (vertex, offset + *position)));
                    }
                }
            }
        }
        out
    }

    /// `sub_0003F890`: the last row, moved slightly toward the opposite column.
    fn cap_positions(&self, local: &[Vec3]) -> Vec<Vec3> {
        if self.flags & FLAG_CAP == 0 {
            return Vec::new();
        }
        let at_end = |c: usize| local[c * self.rows + self.rows - 1];
        (0..self.columns)
            .map(|c| {
                let opposite = at_end((c + self.columns / 2) % self.columns);
                at_end(c) + with_length(opposite - at_end(c), CAP_DEPTH)
            })
            .collect()
    }

    /// `sub_0003FA50`: how each row is turned, from its widest two columns.
    /// Only records with a fold have this.
    fn row_turns(&self, local: &[Vec3]) -> Option<Vec<(f32, f32)>> {
        self.fold?;
        let (mut high, mut low) = (0, 0);
        for c in 1..self.columns {
            if self.roots[c].x > self.roots[high].x {
                high = c;
            } else if self.roots[c].x < self.roots[low].x {
                low = c;
            }
        }
        Some(
            (0..self.rows)
                .map(|row| {
                    let difference = local[high * self.rows + row] - local[low * self.rows + row];
                    let angle = to_angle(difference.x.atan2(difference.z)) - QUARTER_TURN;
                    from_angle(angle).sin_cos()
                })
                .collect(),
        )
    }
}

/// The game's rotation in x and z around a point on z.
fn rotate_xz(value: &mut Vec3, angle: i32, pivot: f32) {
    let (sin, cos) = from_angle(angle).sin_cos();
    let (x, z) = (value.x, value.z - pivot);
    value.x = x * cos - z * sin;
    value.z = x * sin + z * cos + pivot;
}

fn pair_mut(values: &mut [Vec3], a: usize, b: usize) -> (&mut Vec3, &mut Vec3) {
    if a < b {
        let (low, high) = values.split_at_mut(b);
        (&mut low[a], &mut high[0])
    } else {
        let (low, high) = values.split_at_mut(a);
        (&mut high[0], &mut low[b])
    }
}

/// `sub_0003D7A0`: shapes on the same bone are tested together. The game
/// always moves back from the last group, so the segment ends with this length
/// even when nothing was hit.
fn collide_tip(shapes: &[Shape], nodes: &[Mat4], point: Vec3, segment: Vec3, length: Option<f32>) -> Vec3 {
    let mut tip = point + segment;
    let mut start = 0;
    while start < shapes.len() {
        let bone = shapes[start].bone;
        let end = shapes[start..].iter().position(|s| s.bone != bone).map_or(shapes.len(), |n| start + n);
        if let Some(matrix) = shapes[start].matrix(nodes) {
            let mut local = matrix.inverse().transform_point3(tip);
            let mut hit = false;
            for shape in &shapes[start..end] {
                hit |= shape.push_out(&mut local);
            }
            if hit {
                tip = matrix.transform_point3(local);
            }
        }
        start = end;
    }
    length.map_or(tip - point, |length| with_length(tip - point, length))
}

#[cfg(test)]
mod tests {
    use super::*;

    impl SoftCloth {
        fn particles_mut(&mut self) -> &mut [ClothParticle] {
            self.ready = true;
            &mut self.particles
        }

        fn particles(&self) -> &[ClothParticle] {
            &self.particles
        }
    }

    fn entry(data: &[u8], index: usize) -> &[u8] {
        let at = 8 + index * 8;
        let offset = read_u32(data, at).unwrap() as usize;
        let size = read_u32(data, at + 4).unwrap() as usize;
        &data[offset..offset + size]
    }

    fn afs() -> Option<Vec<u8>> {
        std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/doa3/loadfile.afs")).ok()
    }

    /// One step of DOA2U cloth against particles read from the game. DOA2U_PAIRS is
    /// an export of consecutive frames, and DOA2U_RECORDS lists "fighter costume".
    #[test]
    #[ignore]
    fn doa2u_single_steps() {
        use crate::formats::doa2u::{decrypt_cat, soft_body_offsets};
        let dir = std::env::var("DOA2U_DISC").expect("DOA2U_DISC");
        let archive = crate::formats::AfsArchive::from_file(format!("{dir}/datahdd2.afs")).unwrap();
        let mut costumes = std::collections::HashMap::new();
        for pair in std::env::var("DOA2U_RECORDS").expect("DOA2U_RECORDS").split(',') {
            let (fighter, stem) = pair.split_once(' ').unwrap();
            let mut cat = archive.get_by_name(&format!("{stem}.cat")).unwrap().to_vec();
            decrypt_cat(&mut cat).unwrap();
            let (base, offsets) = soft_body_offsets(&cat, 2).unwrap();
            costumes.insert(fighter.to_string(), parse_cloth_records(&cat, base, &offsets));
        }
        let text = std::fs::read_to_string(std::env::var("DOA2U_PAIRS").expect("DOA2U_PAIRS")).unwrap();
        let particles = |v: &[f32]| -> Vec<ClothParticle> {
            v.chunks_exact(16)
                .map(|p| ClothParticle {
                    position: Vec3::new(p[0], p[1], p[2]),
                    segment: Vec3::new(p[4], p[5], p[6]),
                    velocity: Vec3::new(p[8], p[9], p[10]),
                    last_pull: Vec3::new(p[12], p[13], p[14]),
                })
                .collect()
        };
        let mut records = Vec::new();
        let mut nodes: std::collections::HashMap<String, Vec<Mat4>> = Default::default();
        let mut before: Vec<ClothParticle> = Vec::new();
        let (mut head, mut ring) = (0usize, Vec::<f32>::new());
        let mut object = Vec::<f32>::new();
        let mut vertex_errors: std::collections::BTreeMap<usize, Vec<f32>> = Default::default();
        let use_wind = std::env::var("DOA2U_NO_WIND").is_err();
        let mut totals: std::collections::BTreeMap<usize, Vec<(f32, f32, f32)>> = Default::default();
        let mut roots: std::collections::BTreeMap<usize, Vec<f32>> = Default::default();
        let mut apart: std::collections::BTreeMap<usize, usize> = Default::default();
        for line in text.lines() {
            let mut words = line.split_whitespace();
            let kind = words.next().unwrap();
            let rest: Vec<&str> = words.collect();
            match kind {
                "rec" => records.push((rest[0].to_string(), rest[1].parse::<usize>().unwrap())),
                "vertices" => object = rest.iter().map(|v| v.parse().unwrap()).collect(),
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
                    let Some(cloth) = costumes.get_mut(fighter).and_then(|c| c.get_mut(*record)) else { continue };
                    cloth.ready = true;
                    cloth.floor_height = std::env::var("DOA2U_FLOOR").map_or(0.0, |v| v.parse().unwrap());
                    if !object.is_empty() {
                        cloth.follow_vertices(|v| object.get(v * 8..v * 8 + 3).map(|p| Vec3::new(p[0], p[1], p[2])));
                    }
                    cloth.particles_mut().copy_from_slice(&before);
                    // The phase starts at 0 for each fighter and adds 3, modulo 8, per record.
                    let phase = (record * 3) & 7;
                    let index = (head as i64 - phase as i64 - 1).rem_euclid(64) as usize * 4;
                    let wind = Vec3::new(ring[index], ring[index + 1], ring[index + 2]);
                    cloth.step(&nodes[fighter], if use_wind { wind } else { Vec3::ZERO });
                    // The vertices this sheet writes, from the game's own particles.
                    let stepped: Vec<ClothParticle> = cloth.particles().to_vec();
                    cloth.particles_mut().copy_from_slice(&after);
                    if !object.is_empty() {
                        for (vertex, position) in cloth.vertex_positions(&nodes[fighter]) {
                            if let Some(p) = object.get(vertex * 8..vertex * 8 + 3) {
                                vertex_errors.entry(k).or_default().push((position - Vec3::new(p[0], p[1], p[2])).length());
                            } else {
                                vertex_errors.entry(k).or_default().push(f32::INFINITY);
                            }
                        }
                    }
                    cloth.particles_mut().copy_from_slice(&stepped);
                    // Reads can catch particles and matrices a frame apart. Row 0 sits
                    // on the bone, so a mismatch there shows such a pair.
                    let rows = cloth.rows + 1;
                    let root_error = (0..cloth.columns)
                        .map(|c| (cloth.particles()[c * rows].position - after[c * rows].position).length())
                        .fold(0.0, f32::max);
                    roots.entry(k).or_insert_with(Vec::new).push(root_error);
                    if std::env::var("DOA2U_WRITTEN").is_ok() {
                        let list: Vec<String> = cloth.vertex_positions(&nodes[fighter]).iter().map(|(v, _)| v.to_string()).collect();
                        println!("written {k} {}", list.join(" "));
                    }
                    if std::env::var("DOA2U_PARTICLES").is_ok() {
                        // Particle by particle, to see where a sheet parts from the game.
                        println!("record {k}: {} x {} flags {:#x} shapes {:?} fold {:?} roots {:?} ring {} angle {:?}", cloth.columns, rows, cloth.flags, cloth.shapes.iter().map(|s| (s.bone, s.kind, s.values)).collect::<Vec<_>>(), cloth.fold, &cloth.roots[..2], cloth.is_ring, cloth.fold.map(|f| f.angle(&nodes[fighter], &nodes[fighter][cloth.bone].inverse())));
                        for (i, (ours, game)) in cloth.particles().iter().zip(&after).enumerate() {
                            let delta = ours.position - game.position;
                            if delta.length() > 0.001 {
                                println!("  c{} r{} game y {:.4} delta [{:.1} {:.1} {:.1}] mm", i / rows, i % rows, game.position.y, delta.x * 1000.0, delta.y * 1000.0, delta.z * 1000.0);
                            }
                        }
                    }
                    // A pair more than a step apart can still have row 0 in place. Its last
                    // pull, which the step computes from the root alone, gives it away.
                    let one_step = (0..cloth.columns).all(|c| (cloth.particles()[c * rows].last_pull - after[c * rows].last_pull).length() < 1e-4);
                    if !one_step {
                        apart.entry(k).and_modify(|n| *n += 1).or_insert(1);
                    }
                    if root_error < 0.0005 && one_step {
                        let error = |list: &[ClothParticle]| list.iter().zip(&after).map(|(a, b)| (a.segment - b.segment).length()).sum::<f32>() / after.len() as f32;
                        let worst = cloth.particles().iter().zip(&after).map(|(a, b)| (a.position - b.position).length()).fold(0.0, f32::max);
                        let total = totals.entry(k).or_insert_with(Vec::new);
                        total.push((error(cloth.particles()), error(&before), worst));
                    }
                }
                _ => {}
            }
        }
        for (k, list) in &mut vertex_errors {
            list.sort_by(f32::total_cmp);
            println!(
                "vertices of record {k}: {} samples, median {:.2} mm, 95% {:.2} mm, worst {:.2} mm",
                list.len(), list[list.len() / 2] * 1000.0, list[list.len() * 95 / 100] * 1000.0, list[list.len() - 1] * 1000.0
            );
        }
        for (k, list) in &mut roots {
            list.sort_by(f32::total_cmp);
            let mut samples = totals.remove(k).unwrap_or_default();
            let count = samples.len();
            let median = |values: &mut Vec<f32>| {
                values.sort_by(f32::total_cmp);
                values.get(values.len() / 2).copied().unwrap_or(f32::NAN) * 1000.0
            };
            let mut ours: Vec<f32> = samples.iter().map(|s| s.0).collect();
            let mut still: Vec<f32> = samples.iter().map(|s| s.1).collect();
            let mut worst: Vec<f32> = samples.drain(..).map(|s| s.2).collect();
            println!(
                "record {k} {:?}: {count} of {} pairs usable, {} not one step apart, median row 0 error {:.2} mm; segment error {:.2} mm, without stepping {:.2} mm, worst position {:.2} mm",
                records[*k], list.len(), apart.get(k).copied().unwrap_or(0), list[list.len() / 2] * 1000.0, median(&mut ours), median(&mut still), median(&mut worst)
            );
        }
    }

    #[test]
    fn angles_wrap_like_the_game() {
        assert_eq!(wrap_angle(0x8000), 0x8000);
        assert_eq!(wrap_angle(0x8001), 0x8001 - 0x10000);
        assert_eq!(wrap_angle(-1), -1);
    }

    #[test]
    fn helena_and_tina_match_the_game_records() {
        let Some(data) = afs() else { return };
        // Counts and values the game had loaded in a match.
        let helena = parse_cloth(entry(&data, 413));
        assert_eq!(helena.len(), 2);
        assert_eq!((helena[0].bone, helena[0].object, helena[0].columns, helena[0].rows), (4, 9, 16, 4));
        assert!((helena[0].directions[0] - Vec3::new(0.0027, 0.0096, -0.1)).length() < 1e-3, "{}", helena[0].directions[0]);
        let tina = parse_cloth(entry(&data, 161));
        assert_eq!(tina.len(), 2);
        assert_eq!((tina[0].bone, tina[0].object, tina[0].columns, tina[0].rows), (7, 15, 8, 10));
        assert!((tina[0].directions[0] - Vec3::new(0.0142, -0.0264, -0.1)).length() < 1e-3, "{}", tina[0].directions[0]);
    }

    #[test]
    fn cloth_stays_near_its_bone() {
        let Some(data) = afs() else { return };
        let nodes = vec![Mat4::from_translation(Vec3::new(0.0, 1.4, 0.0)); 23];
        for index in [161, 413] {
            for mut cloth in parse_cloth(entry(&data, index)) {
                for _ in 0..600 {
                    cloth.step(&nodes, Vec3::ZERO);
                }
                let positions = cloth.vertex_positions(&nodes);
                assert!(!positions.is_empty());
                for (_, position) in positions {
                    assert!(position.is_finite());
                    assert!(position.length() < 2.0, "{position}");
                }
            }
        }
    }

    #[test]
    fn every_model_stays_stable_while_moving() {
        let Some(data) = afs() else { return };
        let count = read_u32(&data, 4).unwrap() as usize;
        let mut records = 0;
        for index in 1..count {
            let cat = entry(&data, index);
            if !entry(&data, index - 1).starts_with(b"XPR0") || cat.starts_with(b"XPR0") {
                continue;
            }
            for mut cloth in parse_cloth(cat) {
                records += 1;
                for frame in 0..240 {
                    let angle = frame as f32 * 0.05;
                    let bone = Mat4::from_translation(Vec3::new(angle.sin() * 0.5, 1.0, 0.0))
                        * Mat4::from_rotation_y(angle.sin());
                    cloth.step(&vec![bone; 23], Vec3::ZERO);
                    let nodes = vec![bone; 23];
                    for (_, position) in cloth.vertex_positions(&nodes) {
                        assert!(position.is_finite(), "entry {index}");
                        assert!(position.length() < 3.0, "entry {index} frame {frame}: {position}");
                    }
                }
            }
        }
        assert!(records > 90, "{records}");
    }

    /// Compares the port with two consecutive frames read from the running
    /// game. Set `DOA3_CLOTH_CAPTURE` to the capture file to run it.
    #[test]
    fn matches_captured_frames() {
        let Ok(path) = std::env::var("DOA3_CLOTH_CAPTURE") else { return };
        let Some(data) = afs() else { return };
        let capture = std::fs::read(path).unwrap();
        let count = read_u32(&capture, 0).unwrap() as usize;
        let mut at = 4;
        let vec3s = |bytes: &[u8]| -> Vec<ClothParticle> {
            bytes
                .chunks_exact(0x40)
                .map(|p| ClothParticle {
                    position: read_vec3(p, 0).unwrap(),
                    segment: read_vec3(p, 0x10).unwrap(),
                    velocity: read_vec3(p, 0x20).unwrap(),
                    last_pull: read_vec3(p, 0x30).unwrap(),
                })
                .collect()
        };
        let mut position_errors = Vec::new();
        let mut windless = Vec::new();
        let mut with_wind = Vec::new();
        let mut moved_per_frame = Vec::new();
        let mut kinds = std::collections::BTreeSet::new();
        let mut hardest = 0.0f32;
        let mut has_ring = false;
        for _ in 0..count {
            let field = |i: usize| read_u32(&capture, at + i * 4).unwrap() as usize;
            let wind_size = field(7);
            let (record, cat, columns, rows, vertex_count, size) = (field(1), field(2), field(3), field(4), field(5), field(6));
            at += 32;
            let before = vec3s(&capture[at..at + size]);
            at += size;
            let nodes: Vec<Mat4> = capture[at..at + 64 * 23]
                .chunks_exact(64)
                .map(|m| {
                    let values: Vec<f32> = (0..16).map(|i| read_f32(m, i * 4).unwrap()).collect();
                    Mat4::from_cols_slice(&values)
                })
                .collect();
            at += 64 * 23;
            let after = vec3s(&capture[at..at + size]);
            at += size;
            let vertices = &capture[at..at + vertex_count * 32];
            at += vertex_count * 32;
            // Old captures keep 12 bytes of wind; new ones keep the game's ring.
            let ring = (wind_size != 0).then(|| capture[at..at + wind_size].to_vec());
            has_ring |= ring.is_some();
            at += if wind_size == 0 { 12 } else { wind_size };

            let mut cloth = parse_cloth(entry(&data, cat)).swap_remove(record);
            assert_eq!((cloth.columns, cloth.rows), (columns, rows));
            cloth.particles_mut().copy_from_slice(&after);
            for (vertex, position) in cloth.vertex_positions(&nodes) {
                let stored = read_vec3(vertices, vertex * 32).unwrap();
                position_errors.push((stored - position).length());
            }

            let error = |cloth: &SoftCloth| -> f32 {
                let total: f32 =
                    cloth.particles().iter().zip(&after).map(|(a, b)| (a.position - b.position).length()).sum();
                total / after.len() as f32
            };
            cloth.particles_mut().copy_from_slice(&before);
            cloth.step(&nodes, Vec3::ZERO);
            windless.push(error(&cloth));
            let wind = if let Some(ring) = &ring {
                // `sub_00153800`: the ring entry for this record's phase.
                let head = read_u32(ring, 0x200).unwrap() as i32;
                let phase = (record as i32 * 3) & 7;
                let slot = ((head - phase - 1) & 0x1F) as usize;
                read_vec3(ring, slot * 16).unwrap()
            } else {
                // The wind is one force per record: estimate it from the first row.
                let first_row: Vec<Vec3> = (0..columns)
                    .map(|c| after[c * (rows + 1)].segment - cloth.particles()[c * (rows + 1)].segment)
                    .collect();
                let mut wind = first_row.iter().sum::<Vec3>() / columns as f32 / cloth.wind_factor;
                // The force is usually at its limit, so the answer is not linear.
                for _ in 0..12 {
                    cloth.particles_mut().copy_from_slice(&before);
                    cloth.step(&nodes, wind);
                    let residual: Vec3 = (0..columns)
                        .map(|c| after[c * (rows + 1)].segment - cloth.particles()[c * (rows + 1)].segment)
                        .sum::<Vec3>()
                        / columns as f32;
                    wind += residual * 2.0;
                }
                wind
            };
            cloth.particles_mut().copy_from_slice(&before);
            cloth.step(&nodes, wind);
            with_wind.push(error(&cloth));
            moved_per_frame.push(
                after.iter().zip(&before).map(|(a, b)| (a.position - b.position).length()).sum::<f32>()
                    / after.len() as f32,
            );
            kinds.extend(cloth.shapes.iter().map(|shape| shape.kind));
            hardest = hardest.max(*with_wind.last().unwrap());
        }
        let summary = |mut values: Vec<f32>| {
            values.sort_by(f32::total_cmp);
            (values[values.len() / 2], values[values.len() - 1])
        };
        let vertices = summary(position_errors);
        let moved = summary(with_wind);
        println!("vertices against particles (median, largest): {vertices:?}");
        println!("mean particle error, no wind: {:?}", summary(windless));
        println!("mean particle error, with wind: {moved:?}");
        println!("mean particle movement per frame: {:?}", summary(moved_per_frame));
        println!("shape kinds: {kinds:?}, hardest record {hardest}");
        assert!(vertices.1 < 1e-4, "{vertices:?}");
        // A fitted wind can only approach the game's; the ring gives it exactly.
        // The worst samples are tips resting on the stage's floor, which is not at
        // height 0 like here: it is not in the capture.
        assert!(moved.0 < if has_ring { 1e-5 } else { 1e-3 }, "{moved:?}");
        assert!(moved.1 < 2e-3, "{moved:?}");
    }
}
