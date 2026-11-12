//! Loose points on a bone (section 5 of a CAT): each vertex group is a particle
//! on a spring to its place on the bone, such as the loose cloth of a trouser leg.
//!
//! Setup `sub_0003CEB0`, first frame `sub_00042960`, update `sub_00042A90`,
//! vertex writes `sub_0003EC50`. Records with their own offsets per vertex
//! (`sub_000417D0`) are refused.

use glam::{Mat4, Vec3};

use super::doa3_chains::{find_sections, read_f32, read_u32, read_vec3};
use super::doa3_cloth::parse_direct_vertices;
use super::doa3_wind::Wind;

const SPRING_SECTION: u8 = 5;
const LIST_END: f32 = 999.0;
const MAX_POINTS: usize = 512;
const MAX_BONE: u8 = 22;
const PHASE_STEP: usize = 3;

#[derive(Debug, Clone, Copy, Default)]
struct SpringPoint {
    position: Vec3,
    stretch: Vec3,
    velocity: Vec3,
}

#[derive(Debug, Clone)]
pub struct SoftSprings {
    pub bone: usize,
    pub object: usize,
    gravity: f32,
    pull: f32,
    damping: f32,
    wind_factor: f32,
    stiffness: f32,
    max_distance: f32,
    rest: Vec<Vec3>,
    /// Each point's own maximum distance, when the record has a list.
    distances: Option<Vec<f32>>,
    vertices: Vec<Vec<usize>>,
    points: Vec<SpringPoint>,
    ready: bool,
}

pub fn parse_springs(cat: &[u8]) -> Vec<SoftSprings> {
    let Some((base, sections)) = find_sections(cat) else { return Vec::new() };
    sections
        .iter()
        .filter(|(kind, _)| *kind == SPRING_SECTION)
        .filter_map(|&(_, offset)| parse_record(cat, base, base + offset))
        .collect()
}

/// DOA4's springs: the same records with big-endian words, at offsets from `base`. Vertices
/// are left as the file's words, for `map_vertices`.
/// DOA2U's kind 5: the same record, at offsets from `base` (`sub_0004E200` sets it up,
/// `sub_00059970` steps it with DOA3's formula, the wind's phase moving 3 a point).
pub fn parse_spring_records(cat: &[u8], base: usize, offsets: &[usize]) -> Vec<SoftSprings> {
    offsets.iter().filter_map(|&offset| parse_record(cat, base, base + offset)).collect()
}

pub fn parse_spring_records_big_endian(cat: &[u8], base: usize, offsets: &[usize]) -> Vec<SoftSprings> {
    super::doa3_chains::with_big_endian(|| offsets.iter().filter_map(|&offset| parse_record(cat, base, base + offset)).collect())
}

fn parse_record(cat: &[u8], base: usize, at: usize) -> Option<SoftSprings> {
    let bone = *cat.get(at)?;
    // DOA2U's bones from 0x80 are matrices from 24, mapped by its reader.
    if bone > MAX_BONE && bone < 0x80 {
        return None;
    }
    let object = [*cat.get(at + 2)?, *cat.get(at + 3)?];
    let object = if super::doa3_chains::big_endian() { u16::from_be_bytes(object) } else { u16::from_le_bytes(object) } as usize;
    let pointer = |offset: usize| -> Option<Option<usize>> {
        let value = read_u32(cat, at + offset)? as usize;
        Some((value != 0).then_some(base + value))
    };
    if pointer(0x20)?.is_some() {
        return None;
    }
    let rest_at = pointer(0x1C)??;
    let mut rest = Vec::new();
    while read_f32(cat, rest_at + rest.len() * 16)? < LIST_END {
        rest.push(read_vec3(cat, rest_at + rest.len() * 16)?);
        if rest.len() > MAX_POINTS {
            return None;
        }
    }
    let count = rest.len();
    if count == 0 {
        return None;
    }
    let distances = match pointer(0x2C)? {
        Some(list) => Some((0..count).map(|i| read_f32(cat, list + i * 4)).collect::<Option<Vec<_>>>()?),
        None => None,
    };
    Some(SoftSprings {
        bone: bone as usize,
        object,
        gravity: read_f32(cat, at + 4)?,
        pull: read_f32(cat, at + 8)?,
        damping: read_f32(cat, at + 0xC)?,
        wind_factor: read_f32(cat, at + 0x10)?,
        stiffness: read_f32(cat, at + 0x14)?,
        max_distance: read_f32(cat, at + 0x18)?,
        rest,
        distances,
        vertices: parse_direct_vertices(cat, pointer(0x24)??, count)?,
        points: vec![SpringPoint::default(); count],
        ready: false,
    })
}

impl SoftSprings {
    /// Whether it sits on the torso.
    pub fn is_on_torso_front(&self) -> bool {
        self.bone == 0
    }

    /// `sub_00042960`: at rest, stretched by its own weight.
    fn settle(&mut self, bone: &Mat4) {
        let sag = if self.damping > 0.0 && self.damping < 1.0 {
            self.gravity / (1.0 - self.damping)
        } else {
            self.gravity
        };
        for (point, rest) in self.points.iter_mut().zip(&self.rest) {
            point.position = bone.transform_point3(*rest);
            point.stretch = Vec3::new(0.0, -sag, 0.0) * self.stiffness;
            point.velocity = Vec3::ZERO;
        }
    }

    /// Advances one 60 Hz game frame (`sub_00042A90`).
    pub fn step(&mut self, nodes: &[Mat4], wind: &Wind) {
        self.step_with(nodes, |i| wind.force_at_phase(i * PHASE_STEP));
    }

    /// `step` with the wind of each point given.
    fn step_with(&mut self, nodes: &[Mat4], wind_at: impl Fn(usize) -> Vec3) {
        let Some(&bone) = nodes.get(self.bone) else { return };
        if !self.ready {
            self.settle(&bone);
            self.ready = true;
            return;
        }
        for i in 0..self.points.len() {
            let point = &mut self.points[i];
            let place = bone.transform_point3(self.rest[i]);
            let old = point.position;
            let pull = old - (place + point.stretch);
            let mut force = pull * self.pull + point.velocity;
            force += wind_at(i) * self.wind_factor;
            force.y -= self.gravity;
            let mut position = old + force;
            position += (place - position) * self.stiffness;

            let limit = self.distances.as_ref().map_or(self.max_distance, |list| list[i]);
            let difference = position - place;
            let size = difference.length();
            if size > limit {
                position -= difference * ((size - limit) / size);
            }
            point.position = position;
            point.velocity = (position - old) * self.damping;
            point.stretch = position - place;
        }
    }

    /// Renumbers the vertices the points move.
    pub fn map_vertices(&mut self, map: impl Fn(usize) -> usize) {
        self.vertices.iter_mut().flatten().for_each(|vertex| *vertex = map(*vertex));
    }

    /// The vertices the points move.
    pub fn vertices(&self) -> impl Iterator<Item = usize> + '_ {
        self.vertices.iter().flatten().copied()
    }

    /// Vertex positions in the bone's space.
    pub fn vertex_positions(&self, nodes: &[Mat4]) -> Vec<(usize, Vec3)> {
        let to_bone = nodes.get(self.bone).map_or(Mat4::IDENTITY, Mat4::inverse);
        let mut out = Vec::new();
        for (point, list) in self.points.iter().zip(&self.vertices) {
            let local = to_bone.transform_point3(point.position);
            out.extend(list.iter().map(|&vertex| (vertex, local)));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One step of DOA2U's springs against the game's points. DOA2U_PAIRS comes from
    /// `tools/re/doa2u_spring_pairs.py`, DOA2U_RECORDS lists "fighter costume".
    #[test]
    #[ignore]
    fn doa2u_spring_single_steps() {
        use crate::formats::doa2u::{decrypt_cat, soft_springs};
        let dir = std::env::var("DOA2U_DISC").expect("DOA2U_DISC");
        let archive = crate::formats::AfsArchive::from_file(format!("{dir}/datahdd2.afs")).unwrap();
        let mut costumes = std::collections::HashMap::new();
        for pair in std::env::var("DOA2U_RECORDS").expect("DOA2U_RECORDS").split(',') {
            let (fighter, stem) = pair.split_once(' ').unwrap();
            let mut cat = archive.get_by_name(&format!("{stem}.cat")).unwrap().to_vec();
            decrypt_cat(&mut cat).unwrap();
            costumes.insert(fighter.to_string(), soft_springs(&cat).unwrap());
        }
        let text = std::fs::read_to_string(std::env::var("DOA2U_PAIRS").expect("DOA2U_PAIRS")).unwrap();
        let points = |v: &[f32]| -> Vec<SpringPoint> {
            v.chunks_exact(16)
                .map(|p| SpringPoint { position: Vec3::new(p[0], p[1], p[2]), stretch: Vec3::new(p[4], p[5], p[6]), velocity: Vec3::new(p[8], p[9], p[10]) })
                .collect()
        };
        let (mut records, mut nodes, mut before) = (Vec::new(), std::collections::HashMap::<String, Vec<Mat4>>::new(), Vec::new());
        let (mut head, mut ring) = (0usize, Vec::<f32>::new());
        let mut results: std::collections::BTreeMap<usize, Vec<(f32, f32)>> = Default::default();
        for line in text.lines() {
            let mut words = line.split_whitespace();
            let kind = words.next().unwrap();
            let rest: Vec<&str> = words.collect();
            let floats = |from: usize| rest[from..].iter().map(|v| v.parse().unwrap()).collect::<Vec<f32>>();
            match kind {
                "rec" => records.push((rest[0].to_string(), rest[1].parse::<usize>().unwrap())),
                "wind" => {
                    head = rest[0].parse().unwrap();
                    ring = floats(1);
                }
                "matrices" => {
                    let m = floats(1);
                    let game = |i: usize| Mat4::from_cols_slice(&m[i * 16..i * 16 + 16]);
                    let mut list = vec![Mat4::IDENTITY; 23];
                    for (i, node) in list.iter_mut().enumerate().take(15) {
                        *node = game(i);
                    }
                    list[19] = game(24);
                    list[20] = game(25);
                    nodes.insert(rest[0].to_string(), list);
                }
                "before" => before = points(&floats(1)),
                "after" => {
                    let k: usize = rest[0].parse().unwrap();
                    let after = points(&floats(1));
                    let (fighter, record) = &records[k];
                    let Some(spring) = costumes.get_mut(fighter).and_then(|c| c.get_mut(*record)) else { continue };
                    if spring.points.len() != after.len() {
                        println!("record {k}: {} points here, {} in the game", spring.points.len(), after.len());
                        continue;
                    }
                    spring.ready = true;
                    spring.points.copy_from_slice(&before);
                    // The phase starts at 0 for each record and moves 3 a point, modulo 8.
                    let wind = |i: usize| {
                        let index = (head as i64 - ((i * 3) & 7) as i64 - 1).rem_euclid(64) as usize * 4;
                        Vec3::new(ring[index], ring[index + 1], ring[index + 2])
                    };
                    spring.step_with(&nodes[fighter], wind);
                    let error = spring.points.iter().zip(&after).map(|(a, b)| (a.position - b.position).length()).fold(0.0f32, f32::max) * 1000.0;
                    let still = before.iter().zip(&after).map(|(a, b)| (a.position - b.position).length()).fold(0.0f32, f32::max) * 1000.0;
                    results.entry(k).or_default().push((error, still));
                }
                _ => {}
            }
        }
        for (k, mut list) in results {
            list.sort_by(|a, b| a.0.total_cmp(&b.0));
            let (ours, still): (Vec<f32>, Vec<f32>) = list.iter().copied().unzip();
            let mut sorted_still = still.clone();
            sorted_still.sort_by(f32::total_cmp);
            println!("record {k} {:?}: {} pairs; worst point error median {:.3} mm, 90% {:.3} mm, worst {:.3} mm; points moved median {:.3} mm",
                records[k], ours.len(), ours[ours.len() / 2], ours[ours.len() * 9 / 10], ours[ours.len() - 1], sorted_still[sorted_still.len() / 2]);
        }
    }

    fn entry(data: &[u8], index: usize) -> &[u8] {
        let at = 8 + index * 8;
        let offset = read_u32(data, at).unwrap() as usize;
        &data[offset..offset + read_u32(data, at + 4).unwrap() as usize]
    }

    /// One step against frames of the game. Each capture holds a header of 6 u32
    /// (record, model entry, points, object vertices, bytes, object), the points,
    /// 23 bone matrices, the points a frame later, the object's vertices, and the
    /// wind ring with its head.
    #[test]
    fn step_matches_captured_frames() {
        let Ok(path) = std::env::var("DOA3_SPRING_CAPTURE") else { return };
        let Ok(data) = std::fs::read("assets/doa3/loadfile.afs") else { return };
        let capture = std::fs::read(path).unwrap();
        let count = read_u32(&capture, 0).unwrap() as usize;
        let read_points = |bytes: &[u8]| -> Vec<SpringPoint> {
            bytes
                .chunks_exact(0x40)
                .map(|p| SpringPoint {
                    position: read_vec3(p, 0).unwrap(),
                    stretch: read_vec3(p, 0x10).unwrap(),
                    velocity: read_vec3(p, 0x20).unwrap(),
                })
                .collect()
        };
        let mut at = 4;
        let (mut means, mut moved, mut vertex_errors) = (Vec::new(), Vec::new(), Vec::new());
        let mut outliers = Vec::new();
        for step in 0..count {
            let field = |i: usize| read_u32(&capture, at + i * 4).unwrap() as usize;
            let (record, model, points, vertices, size) = (field(0), field(1), field(2), field(3), field(4));
            at += 24;
            let before = read_points(&capture[at..at + size]);
            at += size;
            let nodes: Vec<Mat4> = capture[at..at + 64 * 23]
                .chunks_exact(64)
                .map(|m| {
                    let values: Vec<f32> = (0..16).map(|i| read_f32(m, i * 4).unwrap()).collect();
                    Mat4::from_cols_slice(&values)
                })
                .collect();
            at += 64 * 23;
            let after = read_points(&capture[at..at + size]);
            at += size;
            let stored = &capture[at..at + vertices * 32];
            at += vertices * 32;
            let ring: Vec<Vec3> = (0..32).map(|i| read_vec3(&capture, at + i * 16).unwrap()).collect();
            let head = read_u32(&capture, at + 0x200).unwrap() as usize;
            at += 0x204;

            let mut springs = parse_springs(entry(&data, model + 1)).swap_remove(record);
            assert_eq!(springs.points.len(), points);
            springs.ready = true;
            springs.points = before.clone();
            springs.step(&nodes, &Wind::from_ring(ring.try_into().unwrap(), head));
            let errors: Vec<f32> =
                springs.points.iter().zip(&after).map(|(a, b)| (a.position - b.position).length()).collect();
            let mean = errors.iter().sum::<f32>() / errors.len() as f32;
            means.push(mean);
            if mean > 1e-4 {
                outliers.push((step, record, mean));
            }
            moved.push(
                before.iter().zip(&after).map(|(a, b)| (a.position - b.position).length()).sum::<f32>() / points as f32,
            );
            springs.points = after;
            for (vertex, position) in springs.vertex_positions(&nodes) {
                vertex_errors.push((read_vec3(stored, vertex * 32).unwrap() - position).length());
            }
        }
        means.sort_by(f32::total_cmp);
        moved.sort_by(f32::total_cmp);
        vertex_errors.sort_by(f32::total_cmp);
        println!(
            "{count} steps: mean error median {} largest {}, off {} ({:?}), movement median {}; vertices median {} largest {}",
            means[means.len() / 2],
            means[means.len() - 1],
            outliers.len(),
            &outliers[..outliers.len().min(12)],
            moved[moved.len() / 2],
            vertex_errors[vertex_errors.len() / 2],
            vertex_errors[vertex_errors.len() - 1]
        );
        // A few single moments are far off for every record of a fighter at once.
        // Their cause is not known; a restart of the round would look like this.
        assert!(means[means.len() / 2] < 1e-5);
        assert!(outliers.len() * 50 < count, "{outliers:?}");
        assert!(vertex_errors[vertex_errors.len() / 2] < 1e-5);
    }

    #[test]
    fn every_model_stays_near_its_bone() {
        let Ok(data) = std::fs::read("assets/doa3/loadfile.afs") else { return };
        let count = read_u32(&data, 4).unwrap() as usize;
        let mut tested = 0;
        for index in 1..count {
            if !entry(&data, index - 1).starts_with(b"XPR0") {
                continue;
            }
            for mut springs in parse_springs(entry(&data, index)) {
                tested += 1;
                let mut wind = Wind::new();
                for frame in 0..300 {
                    let t = frame as f32 / 60.0;
                    let moved = Mat4::from_translation(Vec3::new(t.sin(), 1.0, t.cos())) * Mat4::from_rotation_y(t * 2.0);
                    wind.step(1.0);
                    springs.step(&vec![moved; 23], &wind);
                    for point in &springs.points {
                        assert!(point.position.is_finite(), "entry {index}");
                        assert!((point.position - moved.w_axis.truncate()).length() < 2.0, "entry {index}");
                    }
                }
            }
        }
        println!("{tested} records");
        assert!(tested > 20, "{tested}");
    }
}
