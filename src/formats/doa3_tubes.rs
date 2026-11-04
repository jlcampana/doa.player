//! Soft tubes, such as Hitomi's trouser legs (section 4 of a CAT): rings of
//! particles stacked along a bone, bending around the next joint.
//!
//! Follows `sub_00042D20`, `sub_00042FD0`, `sub_000430F0` and `sub_000418E0`.
//! Only rings of "mode 0" are ported, the ones in the costumes that can be
//! picked. Mode 1 goes through a spline solver (`sub_00044620`) and those
//! records are refused.

use glam::{Mat4, Vec3};

use super::doa3_chains::{find_sections, parse_shapes, read_f32, read_u32, read_vec3, with_length, Shape};
use super::doa3_cloth::{
    from_angle, joint_point, parse_direct_vertices, parse_offset_vertices, to_angle, wrap_angle, OffsetPoint,
};
use super::doa3_wind::Wind;

const TUBE_SECTION: u8 = 4;
const MAX_RINGS: usize = 16;
const MAX_COLUMNS: usize = 16;
const NO_NODE: u8 = 0xFF;

/// Where a tube turns to follow a joint, up to two joints along its length.
#[derive(Debug, Clone, Copy)]
struct TubeFold {
    node_from: u8,
    node_to: u8,
    pivot: f32,
    turn_start: f32,
    turn_end: f32,
    node_next: u8,
    pivot_next: f32,
    turn_next_start: f32,
    turn_next_end: f32,
}

impl TubeFold {
    fn parse(cat: &[u8], at: usize) -> Option<Self> {
        Some(Self {
            node_from: *cat.get(at)?,
            node_to: *cat.get(at + 1)?,
            pivot: read_f32(cat, at + 4)?,
            turn_start: read_f32(cat, at + 8)?,
            turn_end: read_f32(cat, at + 0xC)?,
            node_next: *cat.get(at + 0x10)?,
            pivot_next: read_f32(cat, at + 0x14)?,
            turn_next_start: read_f32(cat, at + 0x18)?,
            turn_next_end: read_f32(cat, at + 0x1C)?,
        })
    }

    /// The angle between two nodes in the bone's space, `sub_0003F1F0` and on.
    fn angle(from: u8, to: u8, nodes: &[Mat4], to_bone: &Mat4, reversed: bool) -> i32 {
        let local = to_bone.transform_vector3(joint_point(to, nodes) - joint_point(from, nodes));
        let local = if reversed { -local } else { local };
        wrap_angle(to_angle(local.x.atan2(local.z)))
    }
}

/// Keeps a ring point within an angle and a radius of the axis (`sub_0003CF00`).
#[derive(Debug, Clone, Copy)]
struct RingLimit {
    centre: (f32, f32),
    max_angle: i32,
    min_radius: f32,
    max_radius: f32,
}

impl RingLimit {
    fn parse(cat: &[u8], at: usize) -> Option<Self> {
        Some(Self {
            centre: (read_f32(cat, at)?, read_f32(cat, at + 4)?),
            max_angle: read_u32(cat, at + 8)? as i32,
            min_radius: read_f32(cat, at + 0xC)?,
            max_radius: read_f32(cat, at + 0x10)?,
        })
    }

    fn apply(&self, rest_angle: i32, point: &mut Vec3) {
        let mut a = point.x - self.centre.0;
        let mut b = point.y - self.centre.1;
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
            1.0
        } else if radius > self.max_radius {
            self.max_radius / radius
        } else if radius < self.min_radius {
            self.min_radius / radius
        } else {
            1.0
        };
        point.x = a * scale + self.centre.0;
        point.y = b * scale + self.centre.1;
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct TubeParticle {
    position: Vec3,
    segment: Vec3,
    velocity: Vec3,
    last_pull: Vec3,
}

#[derive(Debug, Clone)]
enum TubeVertices {
    Direct(Vec<Vec<usize>>),
    Offset(Vec<Vec<OffsetPoint>>),
}

/// One ring. Its shape is kept in the plane of the ring's own space.
#[derive(Debug, Clone)]
struct TubeRing {
    step: f32,
    spacing: f32,
    shape: Vec<Vec3>,
    translation: Vec3,
    angle: i32,
}

#[derive(Debug, Clone)]
pub struct SoftTube {
    pub bone: usize,
    pub object: usize,
    columns: usize,
    start: Vec3,
    gravity: f32,
    pull: f32,
    damping: f32,
    wind_factor: f32,
    max_force: f32,
    first_ring: Vec<Vec3>,
    rest_angles: Vec<i32>,
    rings: Vec<TubeRing>,
    fold: Option<TubeFold>,
    limit: RingLimit,
    shapes: Vec<Shape>,
    vertices: TubeVertices,
    particles: Vec<TubeParticle>,
}

/// Reads every tube in a CAT that this port can run.
pub fn parse_tubes(cat: &[u8]) -> Vec<SoftTube> {
    let Some((base, sections)) = find_sections(cat) else { return Vec::new() };
    sections
        .iter()
        .filter(|(kind, _)| *kind == TUBE_SECTION)
        .filter_map(|&(_, offset)| parse_record(cat, base, base + offset))
        .collect()
}

/// A list of (count, value) bytes, one value per ring, or the default without a list.
fn run_list(cat: &[u8], list: Option<usize>, rings: usize, default: u8) -> Option<Vec<u8>> {
    let Some(mut at) = list else { return Some(vec![default; rings]) };
    let mut values = Vec::with_capacity(rings);
    while values.len() < rings {
        let (count, value) = (*cat.get(at)?, *cat.get(at + 1)?);
        if count == 0 {
            return None;
        }
        values.extend(std::iter::repeat(value).take(count as usize));
        at += 2;
    }
    values.truncate(rings);
    Some(values)
}

fn parse_record(cat: &[u8], base: usize, at: usize) -> Option<SoftTube> {
    let header = cat.get(at..at + 8)?;
    let (bone, ring_count, columns) = (header[1] as usize, header[2] as usize, header[3] as usize);
    let object = u16::from_le_bytes([header[4], header[5]]) as usize;
    if bone >= 23 || !(1..=MAX_RINGS).contains(&ring_count) || !(3..=MAX_COLUMNS).contains(&columns) {
        return None;
    }
    let pointer = |offset: usize| -> Option<Option<usize>> {
        let value = read_u32(cat, at + offset)? as usize;
        Some((value != 0).then_some(base + value))
    };
    let (default_step, default_spacing) = (read_f32(cat, at + 8)?, read_f32(cat, at + 0xC)?);

    let modes = run_list(cat, pointer(0x60)?, ring_count, 1)?;
    if modes.iter().any(|&mode| mode != 0) {
        return None;
    }
    let column_counts = match pointer(0x58)? {
        Some(list) => (0..ring_count).map(|i| cat.get(list + i).copied()).collect::<Option<Vec<_>>>()?,
        None => vec![columns as u8; ring_count],
    };
    if column_counts.iter().any(|&count| count as usize != columns) {
        return None;
    }

    let first_ring: Vec<Vec3> =
        (0..columns).map(|i| read_vec3(cat, pointer(0x44)?? + i * 16)).collect::<Option<_>>()?;
    let rest_angles = first_ring.iter().map(|p| to_angle(p.y.atan2(p.x)) & 0xFFFF).collect();
    let shape: Vec<Vec3> = first_ring.iter().map(|p| Vec3::new(p.x, p.y, 0.0)).collect();
    let steps = pointer(0x54)?;
    let spacings = pointer(0x5C)?;
    let rings = (0..ring_count)
        .map(|i| {
            Some(TubeRing {
                step: match steps {
                    Some(list) => read_f32(cat, list + i * 4)?,
                    None => default_step,
                },
                spacing: match spacings {
                    Some(list) => read_f32(cat, list + i * 4)?,
                    None => default_spacing,
                },
                shape: shape.clone(),
                translation: Vec3::ZERO,
                angle: 0,
            })
        })
        .collect::<Option<Vec<_>>>()?;

    let particle_count = columns * ring_count;
    let list_at = pointer(0x48)??;
    let vertices = match pointer(0x4C)? {
        Some(offsets) => TubeVertices::Offset(parse_offset_vertices(cat, list_at, offsets, particle_count)?),
        None => TubeVertices::Direct(parse_direct_vertices(cat, list_at, particle_count)?),
    };
    // `sub_00042D20`: the tube starts at the mean height of its first ring.
    let mut start = read_vec3(cat, at + 0x28)?;
    start.z = first_ring.iter().map(|p| p.z).sum::<f32>() / columns as f32;
    Some(SoftTube {
        bone,
        object,
        columns,
        start,
        gravity: read_f32(cat, at + 0x10)?,
        pull: read_f32(cat, at + 0x14)?,
        damping: read_f32(cat, at + 0x18)?,
        wind_factor: read_f32(cat, at + 0x1C)?,
        max_force: read_f32(cat, at + 0x24)?,
        first_ring,
        rest_angles,
        rings,
        fold: pointer(0x38)?.map(|block| TubeFold::parse(cat, block)).transpose_option()?,
        limit: RingLimit::parse(cat, pointer(0x40)??)?,
        shapes: match pointer(0x3C)? {
            Some(list) => parse_shapes(cat, list)?,
            None => Vec::new(),
        },
        vertices,
        particles: vec![TubeParticle::default(); particle_count],
    })
}

trait TransposeOption<T> {
    fn transpose_option(self) -> Option<Option<T>>;
}

impl<T> TransposeOption<T> for Option<Option<T>> {
    fn transpose_option(self) -> Option<Option<T>> {
        match self {
            Some(Some(value)) => Some(Some(value)),
            Some(None) => None,
            None => Some(None),
        }
    }
}

impl SoftTube {
    /// Whether it hangs from the torso.
    pub fn is_on_torso_front(&self) -> bool {
        self.bone == 0
    }

    /// Places each ring along the bone and around the joints (`sub_000430F0`).
    fn place_rings(&mut self, nodes: &[Mat4], to_bone: &Mat4) {
        let reversed = self.rings.first().is_some_and(|ring| ring.step < 0.0);
        let sign = if reversed { -1.0 } else { 1.0 };
        let angles = self.fold.map(|fold| {
            let first = TubeFold::angle(fold.node_from, fold.node_to, nodes, to_bone, reversed);
            let next = (fold.node_next != NO_NODE)
                .then(|| TubeFold::angle(fold.node_to, fold.node_next, nodes, to_bone, reversed));
            (first, next)
        });
        let mut along = self.start.z;
        let mut distance = self.start.z.abs();
        for ring in &mut self.rings {
            distance += ring.step.abs();
            let straight = |along: f32| Vec3::new(self.start.x, self.start.y, along);
            let (Some(fold), Some((first, next))) = (self.fold, angles) else {
                along += ring.step;
                ring.translation = straight(along);
                ring.angle = 0;
                continue;
            };
            let pivot = sign * fold.pivot;
            ring.translation = if distance < fold.pivot {
                along += ring.step;
                straight(along)
            } else {
                let past_next = next.filter(|_| distance >= fold.pivot_next + fold.pivot);
                let (sin, cos) = from_angle(first).sin_cos();
                match past_next {
                    Some(next) => {
                        let bend = Vec3::new(
                            self.start.x + sin * fold.pivot_next,
                            self.start.y,
                            pivot + sign * cos * fold.pivot_next,
                        );
                        let (sin_next, cos_next) = from_angle(next).sin_cos();
                        let past = sign * (distance - (fold.pivot_next + fold.pivot));
                        bend + Vec3::new(sin_next * past, 0.0, cos_next * past)
                    }
                    None => {
                        let past = sign * (distance - fold.pivot);
                        Vec3::new(self.start.x + sin * past, self.start.y, pivot + cos * past)
                    }
                }
            };
            let share = |from: f32, to: f32| (distance - from) / (to - from);
            let next = next.unwrap_or(first);
            ring.angle = if distance < fold.turn_start {
                0
            } else if distance < fold.turn_end {
                (share(fold.turn_start, fold.turn_end) * first as f32) as i32
            } else if distance < fold.turn_next_start {
                first
            } else if distance < fold.turn_next_end {
                (share(fold.turn_next_start, fold.turn_next_end) * (next - first) as f32) as i32 + first
            } else {
                next
            };
        }
    }

    fn ring_matrix(ring: &TubeRing, bone: &Mat4) -> Mat4 {
        *bone * Mat4::from_translation(ring.translation) * Mat4::from_rotation_y(from_angle(ring.angle))
    }

    /// `sub_00039D40`: pulls a point toward another when too far from it.
    fn pull_within(positions: &mut [Vec3], moved: usize, other: usize, spacing: f32) {
        let difference = positions[moved] - positions[other];
        let size = difference.length();
        if size <= spacing {
            return;
        }
        positions[moved] -= difference * ((size - spacing) / size);
    }

    /// Advances one 60 Hz game frame.
    pub fn step(&mut self, nodes: &[Mat4], wind: &Wind) {
        let Some(&bone) = nodes.get(self.bone) else { return };
        let to_bone = bone.inverse();
        self.place_rings(nodes, &to_bone);
        let columns = self.columns;
        let mut previous: Vec<Vec3> = Vec::new();
        for index in 0..self.rings.len() {
            let matrix = Self::ring_matrix(&self.rings[index], &bone);
            if index == 0 {
                previous = self.first_ring.iter().map(|p| matrix.transform_point3(*p)).collect();
            }
            let force_of_wind = wind.force_at_phase(index) * self.wind_factor;
            let (step, spacing) = (self.rings[index].step, self.rings[index].spacing);
            let first = index * columns;
            let mut positions = Vec::with_capacity(columns);
            for c in 0..columns {
                let particle = &mut self.particles[first + c];
                let pull = particle.position - (previous[c] + particle.segment);
                let mut velocity = (particle.velocity + pull - particle.last_pull) * self.damping;
                velocity.y -= self.gravity;
                particle.velocity = velocity;
                particle.last_pull = pull;
                let mut force = pull * self.pull + velocity + force_of_wind;
                let size = force.length();
                if size > self.max_force {
                    force *= self.max_force / size;
                }
                particle.segment = with_length(particle.segment + force, step.abs());
                positions.push(previous[c] + particle.segment);
            }

            let half = columns / 2 + 1;
            for c in 0..half {
                Self::pull_within(&mut positions, (c + 1) % columns, c, spacing);
            }
            let mut other = 0;
            let mut moved = columns - 1;
            for _ in 0..half {
                Self::pull_within(&mut positions, moved, other, spacing);
                other = moved;
                moved = (moved + columns - 1) % columns;
            }

            let to_ring = matrix.inverse();
            let shape: Vec<Vec3> = positions
                .iter()
                .enumerate()
                .map(|(c, position)| {
                    let mut local = to_ring.transform_point3(*position);
                    local.z = 0.0;
                    self.limit.apply(self.rest_angles[c], &mut local);
                    local
                })
                .collect();
            let mut world: Vec<Vec3> = shape.iter().map(|p| matrix.transform_point3(*p)).collect();
            self.rings[index].shape = shape;
            self.collide(nodes, &mut world);
            for c in 0..columns {
                let particle = &mut self.particles[first + c];
                particle.position = world[c];
                particle.segment = world[c] - previous[c];
            }
            previous = world;
        }
    }

    /// `sub_0003B9C0`: each edge of a ring against the collision shapes.
    fn collide(&self, nodes: &[Mat4], world: &mut [Vec3]) {
        for shape in &self.shapes {
            let Some(node) = nodes.get(shape.bone) else { continue };
            let inverse = node.inverse();
            let mut local: Vec<Vec3> = world.iter().map(|p| inverse.transform_point3(*p)).collect();
            for right in 0..local.len() {
                let left = (right + local.len() - 1) % local.len();
                let (mut a, mut b) = (local[left], local[right]);
                shape.push_edge_out(&mut a, &mut b);
                (local[left], local[right]) = (a, b);
            }
            for (out, p) in world.iter_mut().zip(&local) {
                *out = node.transform_point3(*p);
            }
        }
    }

    /// Vertex positions in the bone's space (`sub_0003ED00`, `sub_0003EE10`).
    pub fn vertex_positions(&self, nodes: &[Mat4]) -> Vec<(usize, Vec3)> {
        let to_bone = nodes.get(self.bone).map_or(Mat4::IDENTITY, Mat4::inverse);
        let mut out = Vec::new();
        for (index, particle) in self.particles.iter().enumerate() {
            let local = to_bone.transform_point3(particle.position);
            match &self.vertices {
                TubeVertices::Direct(lists) => {
                    if let Some(list) = lists.get(index) {
                        out.extend(list.iter().map(|&vertex| (vertex, local)));
                    }
                }
                TubeVertices::Offset(lists) => {
                    let ring = &self.rings[index / self.columns];
                    let (sin, cos) = from_angle(ring.angle).sin_cos();
                    for point in lists.get(index).map_or(&[][..], Vec::as_slice) {
                        let o = point.offset;
                        let turned = Vec3::new(o.x * cos + o.z * sin, o.y, o.z * cos - o.x * sin);
                        out.extend(point.vertices.iter().map(|&vertex| (vertex, turned + local)));
                    }
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::formats::doa3::parse_node_objects;

    const RECORD_SIZE: usize = 0x47D4;
    const RING_STRIDE: usize = 0x478;
    const RING_AT: usize = 0x38;

    fn afs() -> Option<Vec<u8>> {
        std::fs::read("assets/doa3/loadfile.afs").ok()
    }

    fn entry(data: &[u8], index: usize) -> &[u8] {
        let at = 8 + index * 8;
        let offset = read_u32(data, at).unwrap() as usize;
        let size = read_u32(data, at + 4).unwrap() as usize;
        &data[offset..offset + size]
    }

    #[test]
    fn hitomi_costumes_that_can_be_picked_have_tubes() {
        let Some(data) = afs() else { return };
        for model in [636, 641] {
            let tubes = parse_tubes(entry(&data, model + 1));
            assert_eq!(tubes.len(), 2, "entry {model}");
            let bones: Vec<usize> = tubes.iter().map(|t| t.bone).collect();
            assert_eq!(bones, [7, 13]);
            assert_eq!((tubes[0].object, tubes[1].object), (14, 28));
        }
    }

    /// Runs them on a walking bone: nothing may fly away or become NaN.
    #[test]
    fn tubes_stay_near_their_bone() {
        let Some(data) = afs() else { return };
        let count = read_u32(&data, 4).unwrap() as usize;
        let mut tested = 0;
        for index in 1..count {
            let cat = entry(&data, index);
            if !entry(&data, index - 1).starts_with(b"XPR0") || parse_node_objects(cat).is_err() {
                continue;
            }
            for mut tube in parse_tubes(cat) {
                tested += 1;
                let mut wind = Wind::new();
                for frame in 0..300 {
                    let t = frame as f32 / 60.0;
                    let moved = Mat4::from_translation(Vec3::new(t.sin(), 1.0, t.cos()))
                        * Mat4::from_rotation_y(t * 2.0);
                    let nodes = vec![moved; 23];
                    wind.step(1.0);
                    tube.step(&nodes, &wind);
                    let origin = moved.w_axis.truncate();
                    for particle in &tube.particles {
                        assert!(particle.position.is_finite(), "entry {index}");
                        assert!((particle.position - origin).length() < 2.0, "entry {index}");
                    }
                }
            }
        }
        println!("{tested} tubes");
        assert!(tested >= 4, "{tested}");
    }

    /// Against frames of the game: each ring's placement and the vertices made
    /// from its shape. The capture has no particles, so the dynamics are not
    /// checked here.
    #[test]
    fn rings_and_vertices_match_captured_frames() {
        let Ok(path) = std::env::var("DOA3_TUBE_CAPTURE") else { return };
        let Some(data) = afs() else { return };
        let capture = std::fs::read(path).unwrap();
        let mut tubes = parse_tubes(entry(&data, 642));
        let xpr = crate::formats::model::parse_xpr_model(entry(&data, 641)).unwrap();
        let counts: Vec<usize> = tubes.iter().map(|t| xpr[t.object].vertices.len()).collect();
        let frame_size = RECORD_SIZE * 2 * 2 + 64 * 23 + counts.iter().sum::<usize>() * 32;
        let frames = capture.len() / frame_size;
        let (mut placement, mut angle_error, mut vertices) = (0.0f32, 0, Vec::new());
        for f in 0..frames {
            let start = f * frame_size;
            let bones_at = start + RECORD_SIZE * 2;
            let nodes: Vec<Mat4> = capture[bones_at..bones_at + 64 * 23]
                .chunks_exact(64)
                .map(|m| {
                    let values: Vec<f32> = (0..16).map(|i| read_f32(m, i * 4).unwrap()).collect();
                    Mat4::from_cols_slice(&values)
                })
                .collect();
            let after = bones_at + 64 * 23;
            let mut vertices_at = after + RECORD_SIZE * 2;
            for (t, tube) in tubes.iter_mut().enumerate() {
                let record = after + t * RECORD_SIZE;
                let bone = nodes[tube.bone];
                let to_bone = bone.inverse();
                tube.place_rings(&nodes, &to_bone);
                for index in 0..tube.rings.len() {
                    let ring_at = record + RING_AT + index * RING_STRIDE;
                    let translation = read_vec3(&capture, ring_at + 0x14).unwrap();
                    let angle = read_u32(&capture, ring_at + 0x10).unwrap() as i32;
                    placement = placement.max((translation - tube.rings[index].translation).length());
                    angle_error = angle_error.max((wrap_angle(angle - tube.rings[index].angle)).abs());
                    // Use the game's own values to test the vertices alone.
                    tube.rings[index].translation = translation;
                    tube.rings[index].angle = angle;
                    let matrix = SoftTube::ring_matrix(&tube.rings[index], &bone);
                    let mut world: Vec<Vec3> = (0..tube.columns)
                        .map(|c| matrix.transform_point3(read_vec3(&capture, ring_at + 0x38 + c * 16).unwrap()))
                        .collect();
                    // The shape is kept before the collisions.
                    tube.collide(&nodes, &mut world);
                    for (c, position) in world.into_iter().enumerate() {
                        tube.particles[index * tube.columns + c].position = position;
                    }
                }
                for (vertex, position) in tube.vertex_positions(&nodes) {
                    let stored = read_vec3(&capture, vertices_at + vertex * 32).unwrap();
                    vertices.push((stored - position).length());
                }
                vertices_at += counts[t] * 32;
            }
        }
        vertices.sort_by(f32::total_cmp);
        let (median, largest) = (vertices[vertices.len() / 2], vertices[vertices.len() - 1]);
        println!("{frames} frames; ring placement {placement}, angle {angle_error}; vertices {median} {largest}");
        assert!(placement < 1e-3, "{placement}");
        assert!(angle_error < 64, "{angle_error}");
        assert!(median < 1e-3, "{median} {largest}");
    }

    /// One step against frames of the game, with its own wind ring, starting
    /// from the particles it had the frame before. Each capture holds a header of
    /// 6 u32 (fighter, record, costume, rings, columns, bytes), the record and its
    /// particles, 23 bone matrices, the same a frame later, and the wind ring.
    #[test]
    fn step_matches_captured_frames() {
        let Ok(path) = std::env::var("DOA3_TUBE_STEP_CAPTURE") else { return };
        let Some(data) = afs() else { return };
        let capture = std::fs::read(path).unwrap();
        let count = read_u32(&capture, 0).unwrap() as usize;
        let read_particles = |bytes: &[u8]| -> Vec<TubeParticle> {
            bytes
                .chunks_exact(0x40)
                .map(|p| TubeParticle {
                    position: read_vec3(p, 0).unwrap(),
                    segment: read_vec3(p, 0x10).unwrap(),
                    velocity: read_vec3(p, 0x20).unwrap(),
                    last_pull: read_vec3(p, 0x30).unwrap(),
                })
                .collect()
        };
        let mut at = 4;
        let (mut means, mut worst, mut moved) = (Vec::new(), 0.0f32, Vec::new());
        let mut worst_rows = vec![0.0f32; 16];
        for _ in 0..count {
            let field = |i: usize| read_u32(&capture, at + i * 4).unwrap() as usize;
            let (record, costume, rings, columns, size) = (field(1), field(2), field(3), field(4), field(5));
            at += 24;
            let before = &capture[at..at + size];
            at += size;
            let nodes: Vec<Mat4> = capture[at..at + 64 * 23]
                .chunks_exact(64)
                .map(|m| {
                    let values: Vec<f32> = (0..16).map(|i| read_f32(m, i * 4).unwrap()).collect();
                    Mat4::from_cols_slice(&values)
                })
                .collect();
            at += 64 * 23;
            let after = &capture[at..at + size];
            at += size;
            let ring: Vec<Vec3> = (0..32).map(|i| read_vec3(&capture, at + i * 16).unwrap()).collect();
            let head = read_u32(&capture, at + 0x200).unwrap() as usize;
            at += 0x204;

            let mut tube = parse_tubes(entry(&data, 616 + costume * 5 + 1)).swap_remove(record);
            assert_eq!((tube.rings.len(), tube.columns), (rings, columns));
            tube.particles = read_particles(&before[RECORD_SIZE..]);
            let game = read_particles(&after[RECORD_SIZE..]);
            for (index, r) in tube.rings.iter_mut().enumerate() {
                r.shape = (0..columns)
                    .map(|c| read_vec3(before, RING_AT + index * RING_STRIDE + 0x38 + c * 16).unwrap())
                    .collect();
            }
            let previous = tube.particles.clone();
            tube.step(&nodes, &Wind::from_ring(ring.try_into().unwrap(), head));
            let errors: Vec<f32> =
                tube.particles.iter().zip(&game).map(|(a, b)| (a.position - b.position).length()).collect();
            means.push(errors.iter().sum::<f32>() / errors.len() as f32);
            for (i, error) in errors.iter().enumerate() {
                let row = i / columns;
                worst_rows[row] = worst_rows[row].max(*error);
                worst = worst.max(*error);
            }
            moved.push(
                previous.iter().zip(&game).map(|(a, b)| (a.position - b.position).length()).sum::<f32>()
                    / errors.len() as f32,
            );
        }
        means.sort_by(f32::total_cmp);
        moved.sort_by(f32::total_cmp);
        println!(
            "{count} steps: mean error median {} largest {}, worst particle {worst}, worst by ring {:?}, movement median {}",
            means[means.len() / 2],
            means[means.len() - 1],
            &worst_rows[..5],
            moved[moved.len() / 2]
        );
        assert!(means[means.len() / 2] < 1e-4);
        assert!(worst < 1e-3, "{worst}");
    }
}
