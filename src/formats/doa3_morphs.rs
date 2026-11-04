//! The base shapes of DOA3's hands and faces.
//!
//! The game never draws these vertices as the XPR has them: `sub_00048190` and
//! `sub_000486E0` overwrite them with the base shape of a section in the CAT's
//! `LNK` block, and `sub_00048C30` blends expressions on top. Only the base
//! shapes are applied here. Verified against the running game: error 0.

use super::doa3_chains::read_u32;
use super::model::ModelPiece;
use std::collections::HashMap;

const LINK_MAGIC: &[u8] = b"LNK\0";
const TABLE_OFFSET: usize = 0x10;
const NO_SECTION: u32 = u32::MAX;
const SCALE: f32 = 1.0 / 32767.0;
#[derive(Clone, Copy, PartialEq, Debug)]
enum Field {
    Position,
    Normal,
}

const NODE_HEAD: usize = 1;
const NODE_RIGHT_HAND: usize = 5;
const NODE_LEFT_HAND: usize = 11;

/// Table offsets of the sections with a base shape. The entry at +0x10 belongs
/// to the chest spring (`sub_00049B30`) and is deliberately left out.
const SECTIONS: [(usize, usize, Field); 6] = [
    (0x04, NODE_HEAD, Field::Position),
    (0x0C, NODE_HEAD, Field::Position),
    (0x18, NODE_RIGHT_HAND, Field::Position),
    (0x1C, NODE_LEFT_HAND, Field::Position),
    (0x20, NODE_RIGHT_HAND, Field::Normal),
    (0x24, NODE_LEFT_HAND, Field::Normal),
];

/// Vertices that share one value of a section's base shape.
#[derive(Debug, Clone, PartialEq)]
struct BaseGroup {
    vertices: Vec<usize>,
    value: [f32; 3],
}

fn read_i16(data: &[u8], offset: usize) -> Option<i16> {
    Some(i16::from_le_bytes(data.get(offset..offset + 2)?.try_into().ok()?))
}

/// A section: +0 target count, +4 vertex lists, +8 base shape (three i16 per list).
fn read_base_groups(cat: &[u8], section: usize) -> Option<Vec<BaseGroup>> {
    let mut list = section + read_u32(cat, section + 4)? as usize;
    let base = section + read_u32(cat, section + 8)? as usize;
    let mut groups = Vec::new();
    loop {
        let count = read_i16(cat, list)?;
        list += 2;
        if count == 0 {
            return Some(groups);
        }
        if count < 0 {
            return None;
        }
        let vertices = (0..count as usize)
            .map(|i| read_i16(cat, list + i * 2).and_then(|v| usize::try_from(v).ok()))
            .collect::<Option<Vec<_>>>()?;
        list += count as usize * 2;
        let at = base + groups.len() * 6;
        let value = [
            read_i16(cat, at)? as f32 * SCALE,
            read_i16(cat, at + 2)? as f32 * SCALE,
            read_i16(cat, at + 4)? as f32 * SCALE,
        ];
        groups.push(BaseGroup { vertices, value });
    }
}

fn find_table(cat: &[u8]) -> Option<usize> {
    let link = cat.windows(LINK_MAGIC.len()).position(|w| w == LINK_MAGIC)?;
    Some(link + read_u32(cat, link + TABLE_OFFSET)? as usize)
}

fn sections(cat: &[u8]) -> Vec<(usize, Field, Vec<BaseGroup>)> {
    let Some(table) = find_table(cat) else { return Vec::new() };
    SECTIONS
        .iter()
        .filter_map(|&(entry, node, field)| {
            let offset = read_u32(cat, table + entry)?;
            if offset == NO_SECTION {
                return None;
            }
            Some((node, field, read_base_groups(cat, table + offset as usize)?))
        })
        .collect()
}

/// Writes the base shapes into the pieces drawn on the head and the hands.
/// `rigid` lists (node, object) as `parse_node_objects` gives them.
pub fn apply_base_shapes(cat: &[u8], rigid: &[(usize, usize)], pieces: &mut [ModelPiece]) {
    for (node, field, groups) in sections(cat) {
        let Some(&(_, object)) = rigid.iter().find(|&&(n, _)| n == node) else { continue };
        let Some(piece) = pieces.get_mut(object) else { continue };
        // A section that names missing vertices belongs to another mesh.
        if groups.iter().flat_map(|g| &g.vertices).any(|&v| v >= piece.vertices.len()) {
            log::warn!("base shape of node {node} does not fit object {object}");
            continue;
        }
        for group in &groups {
            for &vertex in &group.vertices {
                let target = &mut piece.vertices[vertex];
                match field {
                    Field::Position => target.position = group.value,
                    Field::Normal => target.normal = group.value,
                }
            }
        }
    }
}

// ─── Hand poses ───────────────────────────────────────────────────────────────

const POSE_COUNT: usize = 28;
const MAX_POSE_TARGETS: u32 = 6;
const HAND_SECTIONS: [(usize, usize, usize); 2] =
    [(NODE_RIGHT_HAND, 0x18, 0x20), (NODE_LEFT_HAND, 0x1C, 0x24)];

/// A section's base shape with the offsets of each of its targets.
#[derive(Debug, Clone)]
struct MorphSection {
    groups: Vec<BaseGroup>,
    targets: Vec<Vec<[f32; 3]>>,
}

fn read_section(cat: &[u8], section: usize) -> Option<MorphSection> {
    let groups = read_base_groups(cat, section)?;
    let count = read_u32(cat, section)? as usize;
    let targets = (0..count)
        .map(|target| {
            let at = section + read_u32(cat, section + 0xC + target * 4)? as usize;
            (0..groups.len())
                .map(|group| {
                    let value = |axis: usize| read_i16(cat, at + group * 6 + axis * 2).map(|v| v as f32 * SCALE);
                    Some([value(0)?, value(1)?, value(2)?])
                })
                .collect::<Option<Vec<_>>>()
        })
        .collect::<Option<Vec<_>>>()?;
    Some(MorphSection { groups, targets })
}

impl MorphSection {
    /// `sub_00048300` and `sub_000484C0`: base, plus two neighbouring targets.
    /// Verified against the running game on pose 1: error 0.
    fn values<'a>(&'a self, list: &[usize], phase: f32) -> impl Iterator<Item = (&'a [usize], [f32; 3])> + 'a {
        let place = phase * list.len() as f32;
        let (frame, weight) = match place as usize {
            frame if frame < list.len() => (frame, place.fract()),
            _ => (list.len().saturating_sub(1), 1.0),
        };
        let from = frame.checked_sub(1).and_then(|f| list.get(f)).and_then(|&t| self.targets.get(t));
        let to = list.get(frame).and_then(|&t| self.targets.get(t));
        self.groups.iter().enumerate().map(move |(i, group)| {
            let mut value = group.value;
            for axis in 0..3 {
                value[axis] += from.map_or(0.0, |t| t[i][axis] * (1.0 - weight));
                value[axis] += to.map_or(0.0, |t| t[i][axis] * weight);
            }
            (group.vertices.as_slice(), value)
        })
    }
}

/// Finds the tables of targets per pose (`sub_00049CC0`): +0 target count,
/// +4 pose count, then a count and an offset per pose.
fn find_pose_tables(cat: &[u8]) -> Vec<(usize, Vec<Vec<usize>>)> {
    let read_table = |at: usize| -> Option<(usize, Vec<Vec<usize>>)> {
        let targets = read_u32(cat, at)? as usize;
        if read_u32(cat, at + 4)? as usize != POSE_COUNT || targets == 0 || targets > 64 {
            return None;
        }
        let poses = (0..POSE_COUNT)
            .map(|pose| {
                let count = read_u32(cat, at + 8 + pose * 8)?;
                if count > MAX_POSE_TARGETS {
                    return None;
                }
                let list = at + read_u32(cat, at + 12 + pose * 8)? as usize;
                (0..count as usize)
                    .map(|i| {
                        let target = read_i16(cat, list + i * 4)?;
                        usize::try_from(target).ok().filter(|&t| t < targets)
                    })
                    .collect::<Option<Vec<_>>>()
            })
            .collect::<Option<Vec<_>>>()?;
        poses.iter().any(|p| !p.is_empty()).then_some((targets, poses))
    };
    (0..cat.len().saturating_sub(16)).step_by(4).filter_map(read_table).collect()
}

#[derive(Debug, Clone)]
pub struct HandMorph {
    pub object: usize,
    positions: MorphSection,
    normals: MorphSection,
    poses: Vec<Vec<usize>>,
    pose: usize,
    /// Progress along the pose: 0 is the open hand.
    phase: f32,
}

impl HandMorph {
    /// Advances one game frame, closing the current pose before opening another.
    pub fn step(&mut self, target: usize, rate: f32) {
        let target_is_open = self.poses.get(target).map_or(true, Vec::is_empty);
        if self.pose != target && self.phase > 0.0 || target_is_open {
            self.phase = (self.phase - rate).max(0.0);
            if self.phase == 0.0 {
                self.pose = target;
            }
        } else {
            self.pose = target;
            self.phase = (self.phase + rate).min(1.0);
        }
    }

    fn list(&self) -> &[usize] {
        self.poses.get(self.pose).map_or(&[], Vec::as_slice)
    }

    pub fn positions(&self) -> impl Iterator<Item = (&[usize], [f32; 3])> {
        self.positions.values(self.list(), self.phase)
    }

    pub fn normals(&self) -> impl Iterator<Item = (&[usize], [f32; 3])> {
        self.normals.values(self.list(), self.phase)
    }
}

/// A DOA2U hand: each pose is one absolute shape, and the game blends from the last one
/// to the new one (`apv` entries 0 and 1). Compared with the game: its hand vertices match
/// these shapes with error 0 at rest. Blending while a blend runs was not compared.
#[derive(Debug, Clone)]
pub struct PoseHand {
    pub object: usize,
    pub vertices: Vec<Vec<usize>>,
    pub shapes: Vec<Vec<[f32; 3]>>,
    pub normals: Vec<Vec<[f32; 3]>>,
    from: usize,
    to: usize,
    /// Progress from `from` to `to`.
    weight: f32,
}

impl PoseHand {
    pub fn new(object: usize, vertices: Vec<Vec<usize>>, shapes: Vec<Vec<[f32; 3]>>, normals: Vec<Vec<[f32; 3]>>) -> Self {
        Self { object, vertices, shapes, normals, from: 0, to: 0, weight: 1.0 }
    }

    /// Advances one game frame. Poses the costume does not have are ignored.
    pub fn step(&mut self, target: usize, rate: f32) {
        if target != self.to && target < self.shapes.len() {
            self.from = self.to;
            self.to = target;
            self.weight = 0.0;
        }
        self.weight = (self.weight + rate).min(1.0);
    }

    fn blend<'a>(&'a self, list: &'a [Vec<[f32; 3]>]) -> impl Iterator<Item = (&'a [usize], [f32; 3])> + 'a {
        let (from, to, weight) = (&list[self.from], &list[self.to], self.weight);
        self.vertices.iter().enumerate().map(move |(i, group)| {
            let mix = |axis: usize| from[i][axis] + (to[i][axis] - from[i][axis]) * weight;
            (group.as_slice(), [mix(0), mix(1), mix(2)])
        })
    }

    pub fn positions(&self) -> impl Iterator<Item = (&[usize], [f32; 3])> {
        self.blend(&self.shapes)
    }

    pub fn normals(&self) -> impl Iterator<Item = (&[usize], [f32; 3])> {
        self.blend(&self.normals)
    }
}

/// The two hands of a model, when it has poses for them.
pub fn parse_hands(cat: &[u8], rigid: &[(usize, usize)]) -> Vec<HandMorph> {
    let Some(table) = find_table(cat) else { return Vec::new() };
    let tables = find_pose_tables(cat);
    HAND_SECTIONS
        .iter()
        .enumerate()
        .filter_map(|(hand, &(node, position_entry, normal_entry))| {
            let &(_, object) = rigid.iter().find(|&&(n, _)| n == node)?;
            let section = |entry: usize| {
                let offset = read_u32(cat, table + entry)?;
                (offset != NO_SECTION).then(|| read_section(cat, table + offset as usize))?
            };
            let (positions, normals) = (section(position_entry)?, section(normal_entry)?);
            let count = positions.targets.len();
            // The tables come in hand order; hands with different targets have their own count.
            let same_count: Vec<_> = tables.iter().filter(|(targets, _)| *targets == count).collect();
            let &(_, poses) = &same_count.get(hand).or(same_count.first())?;
            Some(HandMorph { object, positions, normals, poses: poses.clone(), pose: 0, phase: 0.0 })
        })
        .collect()
}

// ─── Faces ────────────────────────────────────────────────────────────────────

const FACE_EYES_ENTRY: usize = 0x04;
const FACE_MOUTH_ENTRY: usize = 0x0C;
/// Target 0 of each section: the eyelids closed and the mouth's usual movement.
const FACE_TARGET: [usize; 1] = [0];
/// Timing in game frames, measured in a match. The game also changes both with
/// each move (`sub_0004AD40`, `sub_0004CBF0`), which is not ported.
/// Hayate's first target of this section moves 11 cm, so it is not an eyelid. What
/// it is has not been read, and heads like his do not blink here.
const MAX_EYELID_MOVE: f32 = 0.06;
const BLINK_PERIOD: u64 = 300;
const BLINK_CLOSE: u64 = 6;
const BLINK_OPEN: u64 = 8;
const MOUTH_PERIOD: u64 = 94;
const MOUTH_RISE: u64 = 8;
const MOUTH_FALL: u64 = 26;

/// A triangle in time: 0 to 1 over `rise` frames, back to 0 over `fall`.
fn pulse(frame: u64, period: u64, rise: u64, fall: u64) -> f32 {
    let at = frame % period;
    if at < rise {
        at as f32 / rise as f32
    } else if at < rise + fall {
        1.0 - (at - rise) as f32 / fall as f32
    } else {
        0.0
    }
}

/// Absolute shapes of one face section.
pub struct FaceShapes {
    pub vertices: Vec<Vec<usize>>,
    pub base: Vec<[f32; 3]>,
    pub target: Vec<[f32; 3]>,
}

/// The blinking eyes and moving mouth of a head.
#[derive(Debug, Clone)]
pub struct FaceMorph {
    pub object: usize,
    eyes: Option<MorphSection>,
    mouth: MorphSection,
}

pub fn parse_face(cat: &[u8], rigid: &[(usize, usize)]) -> Option<FaceMorph> {
    let table = find_table(cat)?;
    let &(_, object) = rigid.iter().find(|&&(node, _)| node == NODE_HEAD)?;
    let section = |entry: usize| {
        let offset = read_u32(cat, table + entry)?;
        (offset != NO_SECTION).then(|| read_section(cat, table + offset as usize))?
    };
    let (eyes, mouth) = (section(FACE_EYES_ENTRY)?, section(FACE_MOUTH_ENTRY)?);
    let largest_move = |section: &MorphSection| {
        section.targets.first().map_or(f32::MAX, |target| {
            target.iter().map(|value| glam::Vec3::from(*value).length()).fold(0.0, f32::max)
        })
    };
    let eyes = (largest_move(&eyes) < MAX_EYELID_MOVE).then_some(eyes);
    (!mouth.targets.is_empty()).then_some(FaceMorph { object, eyes, mouth })
}

impl FaceMorph {
    /// Builds a face from absolute shapes, as DOA2U stores them: per section the
    /// vertices and base of each group, and the shape it blends to.
    pub fn from_shapes(object: usize, eyes: Option<FaceShapes>, mouth: FaceShapes) -> Self {
        let section = |shapes: FaceShapes| {
            let offsets: Vec<[f32; 3]> = shapes
                .base
                .iter()
                .zip(&shapes.target)
                .map(|(base, target)| [target[0] - base[0], target[1] - base[1], target[2] - base[2]])
                .collect();
            MorphSection {
                groups: shapes
                    .vertices
                    .into_iter()
                    .zip(shapes.base)
                    .map(|(vertices, value)| BaseGroup { vertices, value })
                    .collect(),
                targets: vec![offsets; FACE_TARGET.iter().max().map_or(1, |t| t + 1)],
            }
        };
        Self { object, eyes: eyes.map(section), mouth: section(mouth) }
    }

    /// Vertex positions at a game frame. Verified: with the game's own weights
    /// these sections give its vertices with error 0.
    pub fn positions(&self, frame: u64) -> impl Iterator<Item = (&[usize], [f32; 3])> {
        let blink = pulse(frame, BLINK_PERIOD, BLINK_CLOSE, BLINK_OPEN);
        let mouth = pulse(frame, MOUTH_PERIOD, MOUTH_RISE, MOUTH_FALL);
        let eyes = self.eyes.iter().flat_map(move |section| section.values(&FACE_TARGET, blink));
        eyes.chain(self.mouth.values(&FACE_TARGET, mouth))
    }
}

// ─── Move scripts ─────────────────────────────────────────────────────────────

const SCRIPT_SECTION: usize = 21;
const SECTION_TABLE_ENTRIES: usize = 34;
const SCRIPT_END: u16 = 0xFFFF;
const SCRIPT_NOP: u16 = 0x6666;
const OP_PLAY_MOTION: u16 = 0;
const OP_WAIT_FRAME: u16 = 1;
const OP_HAND_POSE: u16 = 46;
const OP_HAND_POSE_TIMED: u16 = 76;
/// Arguments of each opcode of `sub_000888B0`, read from its handlers. A walk
/// of Jann Lee's scripts ends on the end marker in 421 of 431.
const OP_ARGUMENTS: [u8; 107] = [2, 1, 1, 1, 0, 0, 0, 1, 1, 0, 1, 0, 1, 1, 0, 0, 0, 1, 1, 0, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 0, 1, 1, 1, 1, 1, 1, 1, 0, 0, 1, 2, 0, 0, 1, 1, 0, 1, 1, 1, 1, 1, 0, 0, 1, 1, 1, 1, 1, 0, 2, 1, 0, 0, 2, 1, 1, 0, 1, 1, 1, 3, 2, 0, 0, 0, 1, 1, 2, 0, 1, 1, 1, 1, 1, 1, 1, 0, 0, 1, 0, 1, 0, 1, 1, 1, 2, 0, 1, 1, 0, 2];
const EXTENDED_OPS: [(u16, usize); 6] =
    [(0x6800, 1), (0x6801, 1), (0x6802, 0), (0x6803, 1), (0x6900, 1), (0x6901, 1)];
const BOTH_HANDS: u16 = 2;
const DEFAULT_TIME_POSE: i32 = 9;
const DEFAULT_TIME_OPEN: i32 = 25;

/// A hand pose asked for by a move script.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HandEvent {
    pub frame: u16,
    /// 0 is the left hand, 1 the right one, 2 both.
    pub hand: u16,
    pub pose: u16,
    /// Frames to reach it, or -1 for the default.
    pub time: i16,
}

impl HandEvent {
    pub fn targets(&self, hand: usize) -> bool {
        self.hand == BOTH_HANDS || self.hand as usize == hand
    }

    /// `sub_0004B784`: progress per frame.
    pub fn rate(&self) -> f32 {
        let frames = match self.time as i32 {
            time if time < 0 && self.pose != 0 => DEFAULT_TIME_POSE,
            time if time < 0 => DEFAULT_TIME_OPEN,
            time => time,
        };
        1.0 / (frames + 1) as f32
    }
}

/// The hand poses of each motion in a character's `.bin`. Each script starts
/// by playing a motion, then waits for frames of it and acts.
pub fn parse_hand_events(bin: &[u8]) -> HashMap<u16, Vec<HandEvent>> {
    let mut motions: HashMap<u16, Vec<HandEvent>> = HashMap::new();
    let Some(table) = read_u32(bin, SCRIPT_SECTION * 4).map(|t| t as usize) else { return motions };
    let Some(first) = read_u32(bin, table).map(|f| f as usize) else { return motions };
    let is_header = (0..SECTION_TABLE_ENTRIES).all(|i| read_u32(bin, i * 4).is_some());
    if !is_header || first <= table {
        return motions;
    }
    let mut starts: Vec<usize> = (0..(first - table) / 4)
        .filter_map(|i| read_u32(bin, table + i * 4))
        .filter(|&offset| offset != 0)
        .map(|offset| offset as usize)
        .collect();
    starts.sort_unstable();
    starts.dedup();
    let word = |at: usize| Some(u16::from_le_bytes(bin.get(at..at + 2)?.try_into().ok()?));
    for start in starts {
        let mut at = start;
        let (mut motion, mut frame) = (None, 0);
        let mut events = Vec::new();
        while let Some(op) = word(at) {
            if op == SCRIPT_END {
                break;
            }
            if op == SCRIPT_NOP {
                at += 2;
                continue;
            }
            let arguments = if (op as usize) < OP_ARGUMENTS.len() {
                OP_ARGUMENTS[op as usize] as usize
            } else if let Some(&(_, size)) = EXTENDED_OPS.iter().find(|(code, _)| *code == op) {
                size
            } else {
                break;
            };
            let argument = |i: usize| word(at + 2 + i * 2);
            match op {
                OP_PLAY_MOTION => {
                    if let (Some(m), false) = (motion, events.is_empty()) {
                        motions.entry(m).or_insert_with(|| std::mem::take(&mut events));
                    }
                    events.clear();
                    motion = argument(0);
                    frame = 0;
                }
                OP_WAIT_FRAME => frame = argument(0).unwrap_or(frame),
                OP_HAND_POSE | OP_HAND_POSE_TIMED => {
                    if let (Some(hand), Some(pose)) = (argument(0), argument(1)) {
                        let time = if op == OP_HAND_POSE_TIMED { argument(2).unwrap_or(0) as i16 } else { -1 };
                        events.push(HandEvent { frame, hand, pose, time });
                    }
                }
                _ => {}
            }
            at += 2 + arguments * 2;
        }
        if let (Some(m), false) = (motion, events.is_empty()) {
            motions.entry(m).or_insert(events);
        }
    }
    motions
}

/// `sub_0004B63C`: some characters replace the open hand with another pose.
pub fn character_pose(character: usize, pose: u16) -> usize {
    match (character, pose) {
        (0 | 3, 0) => 1,
        (0x11, 0) => 0x14,
        (0x12, 0) => 9,
        _ => pose as usize,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::formats::doa3::parse_node_objects;
    use crate::formats::model::parse_xpr_model;

    fn afs() -> Option<Vec<u8>> {
        std::fs::read("assets/doa3/loadfile.afs").ok()
    }

    fn entry(data: &[u8], index: usize) -> &[u8] {
        let at = 8 + index * 8;
        let offset = read_u32(data, at).unwrap() as usize;
        let size = read_u32(data, at + 4).unwrap() as usize;
        &data[offset..offset + size]
    }

    /// Welded vertices share a position in the XPR, and the base shape stays
    /// close to it, so each section is really about the object it writes.
    #[test]
    fn base_shapes_fit_every_model() {
        let Some(data) = afs() else { return };
        let count = read_u32(&data, 4).unwrap() as usize;
        let (mut models, mut largest_move) = (0, 0.0f32);
        for index in 0..count - 1 {
            let xpr = entry(&data, index);
            let cat = entry(&data, index + 1);
            if !xpr.starts_with(b"XPR0") || sections(cat).is_empty() {
                continue;
            }
            let (Ok(pieces), Ok(nodes)) = (parse_xpr_model(xpr), parse_node_objects(cat)) else { continue };
            models += 1;
            for (node, field, groups) in sections(cat) {
                let Some(&(_, object)) = nodes.rigid.iter().find(|&&(n, _)| n == node) else { continue };
                let vertices = &pieces[object].vertices;
                for group in &groups {
                    for &vertex in &group.vertices {
                        assert!(vertex < vertices.len(), "entry {index} node {node}");
                    }
                    if field == Field::Position {
                        let rest = glam::Vec3::from(vertices[group.vertices[0]].position);
                        for &vertex in &group.vertices {
                            let other = glam::Vec3::from(vertices[vertex].position);
                            assert!((rest - other).length() < 1e-3, "entry {index} node {node}");
                        }
                        largest_move = largest_move.max((rest - glam::Vec3::from(group.value)).length());
                    }
                }
            }
        }
        println!("{models} models, largest base shape move {largest_move}");
        assert!(models > 50, "{models}");
        assert!(largest_move < 0.15, "{largest_move}");
    }

    #[test]
    fn pose_hand_blends_from_the_last_pose() {
        let shape = |x: f32| vec![[x, 0.0, 0.0]];
        let mut hand = PoseHand::new(3, vec![vec![7]], vec![shape(0.0), shape(1.0), shape(2.0)], vec![shape(0.0), shape(0.0), shape(0.0)]);
        let x = |hand: &PoseHand| hand.positions().next().unwrap().1[0];
        assert_eq!(x(&hand), 0.0);
        hand.step(2, 0.25);
        assert_eq!(x(&hand), 0.5);
        for _ in 0..3 {
            hand.step(2, 0.25);
        }
        assert_eq!(x(&hand), 2.0);
        hand.step(1, 0.5);
        assert_eq!(x(&hand), 1.5);
        // A pose past the costume's list keeps the current one.
        hand.step(9, 0.5);
        assert_eq!(x(&hand), 1.0);
    }

    #[test]
    fn hand_poses_read_from_every_model() {
        let Some(data) = afs() else { return };
        let count = read_u32(&data, 4).unwrap() as usize;
        let (mut models, mut with_hands, mut fist_moves) = (0, 0, 0);
        for index in 0..count - 1 {
            let xpr = entry(&data, index);
            let cat = entry(&data, index + 1);
            if !xpr.starts_with(b"XPR0") || sections(cat).is_empty() {
                continue;
            }
            let Ok(nodes) = parse_node_objects(cat) else { continue };
            models += 1;
            let mut hands = parse_hands(cat, &nodes.rigid);
            if hands.len() == 2 {
                with_hands += 1;
            }
            for hand in &mut hands {
                let open: Vec<[f32; 3]> = hand.positions().map(|(_, v)| v).collect();
                for _ in 0..8 {
                    hand.step(1, 0.25);
                }
                let shut: Vec<[f32; 3]> = hand.positions().map(|(_, v)| v).collect();
                let moved = open.iter().zip(&shut).map(|(a, b)| (glam::Vec3::from(*a) - glam::Vec3::from(*b)).length()).fold(0.0, f32::max);
                assert!(moved < 0.2, "entry {index}: {moved}");
                if moved > 0.005 {
                    fist_moves += 1;
                }
            }
        }
        println!("{models} models, {with_hands} with two hands, {fist_moves} hands that close");
        assert!(with_hands * 10 >= models * 9, "{with_hands} of {models}");
    }

    #[test]
    fn jann_lee_motions_have_hand_poses() {
        let Some(data) = afs() else { return };
        let events = parse_hand_events(entry(&data, 194));
        println!("{} motions with hand poses", events.len());
        assert!(events.len() > 100);
        assert_eq!(events[&50][0], HandEvent { frame: 78, hand: 2, pose: 1, time: -1 });
    }

    #[test]
    fn pulses_rise_and_fall_within_their_period() {
        assert_eq!(pulse(0, 94, 8, 26), 0.0);
        assert_eq!(pulse(8, 94, 8, 26), 1.0);
        assert_eq!(pulse(21, 94, 8, 26), 0.5);
        assert_eq!(pulse(34, 94, 8, 26), 0.0);
        assert_eq!(pulse(94, 94, 8, 26), 0.0);
    }

    /// Eyelids and mouths stay small on every model, and only Hayate's has no eyelids.
    #[test]
    fn faces_read_from_every_model() {
        let Some(data) = afs() else { return };
        let count = read_u32(&data, 4).unwrap() as usize;
        let (mut models, mut faces, mut without_eyelids) = (0, 0, 0);
        for index in 0..count - 1 {
            let cat = entry(&data, index + 1);
            if !entry(&data, index).starts_with(b"XPR0") || sections(cat).is_empty() {
                continue;
            }
            let Ok(nodes) = parse_node_objects(cat) else { continue };
            models += 1;
            let Some(face) = parse_face(cat, &nodes.rigid) else { continue };
            faces += 1;
            let mouth = face.mouth.targets[0].iter().map(|t| glam::Vec3::from(*t).length()).fold(0.0, f32::max);
            assert!(mouth > 0.001 && mouth < 0.03, "entry {index}: {mouth}");
            without_eyelids += face.eyes.is_none() as usize;
            assert!(face.positions(BLINK_CLOSE).count() > 0);
        }
        println!("{faces} faces in {models} models, {without_eyelids} without eyelids");
        assert!(faces * 10 >= models * 9, "{faces} of {models}");
        assert_eq!(without_eyelids, 6);
    }
}
