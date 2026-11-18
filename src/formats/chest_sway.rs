//! The chest's sideways and forward sway: the viewer's own stand-in, for the games whose files
//! hold only the vertical shapes (DOA3, DOA2U, DOAX, DOA4). DOAX2 has all three axes from its
//! own code (`doax2::ChestSprings`) and does not use this.
//!
//! A two-axis spring in the chest node's frame, driven as DOA3's vertical spring is
//! (`doa3::ChestSpring`): by the change of a reference point's motion as the chest node sees
//! it, so the chest lags behind the body's accelerations and turns. Its value moves each vertex
//! group of a chest set sideways and forward by as much as the vertical shape moves that group
//! (the mask), times `REACH`. Nothing here is read from a game.

use glam::{Mat4, Vec2, Vec3};

use super::doa3_morphs::PoseHand;

/// The spring's state: the value (x sideways, y forward), each within -1 and 1.
#[derive(Debug, Clone, Copy)]
pub struct ChestSway {
    pub value: Vec2,
    velocity: Vec2,
    previous_point: Vec3,
    previous_move: Vec2,
    started: bool,
}

impl ChestSway {
    const STIFFNESS: f32 = 0.1;
    const DAMPING: f32 = 0.85;
    /// Per metre of change of motion per frame. At 25 the sway hit its limit on DOA3 Tina's
    /// strikes, past the game's own vertical spring (0.7 to 0.9).
    const DRIVE: f32 = 12.0;
    /// In front of the chest node, in metres (the viewer's spaces are about metres).
    const REFERENCE: Vec3 = Vec3::new(0.0, 0.0, 0.1);

    pub fn new() -> Self {
        Self { value: Vec2::ZERO, velocity: Vec2::ZERO, previous_point: Vec3::ZERO, previous_move: Vec2::ZERO, started: false }
    }

    /// One 60 Hz frame with the chest node's global; gives the value.
    pub fn step(&mut self, chest: &Mat4) -> Vec2 {
        if !self.started {
            self.previous_point = chest.transform_point3(Self::REFERENCE);
            self.started = true;
            return self.value;
        }
        let local = chest.inverse().transform_point3(self.previous_point);
        let moved = Vec2::new(Self::REFERENCE.x - local.x, Self::REFERENCE.z - local.z);
        let drive = moved - self.previous_move;
        self.previous_move = moved;
        self.previous_point = chest.transform_point3(Self::REFERENCE);
        self.velocity = self.velocity * Self::DAMPING - self.value * Self::STIFFNESS - drive * Self::DRIVE;
        self.value = (self.value + self.velocity).clamp(Vec2::splat(-1.0), Vec2::splat(1.0));
        self.value
    }
}

impl Default for ChestSway {
    fn default() -> Self {
        Self::new()
    }
}

/// How far, at most, the sway moves a group compared with the vertical shape's largest move.
pub const REACH: f32 = 0.6;

/// A chest set's sway: per vertex group, its sideways and forward offsets at a value of 1, in
/// the piece's space (x sideways, z forward, the chest node's axes).
#[derive(Debug, Clone)]
pub struct SwaySet {
    pub vertices: Vec<Vec<usize>>,
    pub base: Vec<Vec3>,
    pub side: Vec<Vec3>,
    pub forward: Vec<Vec3>,
}

impl SwaySet {
    /// From a set whose shape 0 is the rest and the others are the vertical extremes: each
    /// group's mask is its largest move over those shapes, over the largest of all groups.
    pub fn from_set(set: &PoseHand) -> Option<Self> {
        let rest = set.shapes.first()?;
        let moves: Vec<f32> = (0..rest.len())
            .map(|g| set.shapes.iter().skip(1).map(|s| (Vec3::from(s[g]) - Vec3::from(rest[g])).length()).fold(0.0, f32::max))
            .collect();
        let largest = moves.iter().copied().fold(0.0, f32::max);
        if largest <= 0.0 {
            return None;
        }
        let reach = largest * REACH;
        let side = moves.iter().map(|m| Vec3::X * (m / largest) * reach).collect();
        let forward = moves.iter().map(|m| Vec3::Z * (m / largest) * reach).collect();
        Some(Self { vertices: set.vertices.clone(), base: rest.iter().map(|&p| Vec3::from(p)).collect(), side, forward })
    }

    /// From DOA3's and DOA2U's chest groups: the same, with the positive and negative offsets.
    pub fn from_groups(groups: &[super::doa3::ChestMorphGroup]) -> Option<Self> {
        let set = PoseHand::new(
            0,
            groups.iter().map(|g| g.vertices.iter().map(|&v| v as usize).collect()).collect(),
            vec![
                groups.iter().map(|g| g.base.to_array()).collect(),
                groups.iter().map(|g| (g.base + g.positive).to_array()).collect(),
                groups.iter().map(|g| (g.base + g.negative).to_array()).collect(),
            ],
            Vec::new(),
            Vec::new(),
        );
        Self::from_set(&set)
    }

    /// Per vertex, the offset to add for a value.
    pub fn offsets(&self, value: Vec2) -> impl Iterator<Item = (usize, Vec3)> + '_ {
        self.vertices.iter().enumerate().flat_map(move |(g, list)| {
            let offset = self.side[g] * value.x + self.forward[g] * value.y;
            list.iter().map(move |&v| (v, offset))
        })
    }

    /// The two sets for exporting, sideways then forward: rest, plus and minus, blended as
    /// `doax::chest_weight` does not (linearly, by the value's size).
    pub fn export_sets(&self) -> [PoseHand; 2] {
        let make = |axis: &[Vec3]| {
            let shapes = vec![
                self.base.iter().map(|p| p.to_array()).collect(),
                self.base.iter().zip(axis).map(|(p, a)| (*p + *a).to_array()).collect(),
                self.base.iter().zip(axis).map(|(p, a)| (*p - *a).to_array()).collect(),
            ];
            PoseHand::new(0, self.vertices.clone(), shapes, Vec::new(), Vec::new())
        };
        [make(&self.side), make(&self.forward)]
    }

    /// The export state for one axis's value: shape 1 for positive, 2 for negative.
    pub fn export_state(value: f32) -> (usize, usize, f32) {
        (0, if value >= 0.0 { 1 } else { 2 }, value.abs().min(1.0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A push sideways makes the sway lag the other way, within its limits, and it settles.
    #[test]
    fn sway_lags_and_settles() {
        let mut sway = ChestSway::new();
        let mut x = 0.0f32;
        let mut speed = 0.0f32;
        let mut most = Vec2::ZERO;
        for frame in 0..240 {
            // Speed up to the right for 20 frames, then stand.
            if frame < 20 {
                speed += 0.002;
            } else if frame < 40 {
                speed -= 0.002;
            }
            x += speed;
            let v = sway.step(&Mat4::from_translation(Vec3::new(x, 1.0, 0.0)));
            if v.x.abs() > most.x.abs() {
                most = v;
            }
        }
        assert!(most.x.abs() > 0.1 && most.x.abs() <= 1.0, "{most}");
        assert!(sway.value.length() < 0.05, "{}", sway.value);
    }

    #[test]
    fn mask_follows_the_vertical_move() {
        let set = PoseHand::new(
            0,
            vec![vec![0], vec![1]],
            vec![vec![[0.0; 3], [0.0; 3]], vec![[0.0, -0.02, 0.0], [0.0, -0.01, 0.0]], vec![[0.0, 0.02, 0.0], [0.0; 3]]],
            Vec::new(),
            Vec::new(),
        );
        let sway = SwaySet::from_set(&set).unwrap();
        assert!((sway.side[0].x - 0.02 * REACH).abs() < 1e-6);
        assert!((sway.side[1].x - 0.01 * REACH).abs() < 1e-6);
    }
}
