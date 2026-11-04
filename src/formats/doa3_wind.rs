//! Wind for hair and cloth.
//!
//! The game keeps the last 32 forces in a ring (`0x90EF00`) and each soft body
//! reads an older one, so a gust travels across them (`sub_00153800`). The ring
//! is filled from per-stage settings and random numbers (`sub_00153490`), which
//! are not ported: the gust here is a stand-in, with the size measured in a
//! match (about 0.011 per frame, level with the ground).

use glam::Vec3;
use std::f32::consts::TAU;

const RING: usize = 32;
const PHASE_STEP: usize = 3;
const PHASES: usize = 8;
const FORCE: f32 = 0.011;
const GUST_SHARE: f32 = 0.35;
const FRAMES_PER_SECOND: f32 = 60.0;
const DIRECTION_PERIOD: f32 = 37.0;
const GUST_PERIODS: [f32; 2] = [5.3, 2.1];

#[derive(Debug, Clone)]
pub struct Wind {
    ring: [Vec3; RING],
    head: usize,
    frame: u32,
}

impl Wind {
    pub fn new() -> Self {
        Self { ring: [Vec3::ZERO; RING], head: 0, frame: 0 }
    }

    /// A wind with the game's ring as it was captured, for tests.
    #[cfg(test)]
    pub fn from_ring(ring: [Vec3; RING], head: usize) -> Self {
        Self { ring, head, frame: 0 }
    }

    /// Advances one 60 Hz game frame.
    pub fn step(&mut self, scale: f32) {
        let seconds = self.frame as f32 / FRAMES_PER_SECOND;
        let wave = |period: f32| (seconds * TAU / period).sin();
        let yaw = 0.8 + wave(DIRECTION_PERIOD);
        let gust = 1.0 + GUST_SHARE * (wave(GUST_PERIODS[0]) + wave(GUST_PERIODS[1])) * 0.5;
        self.ring[self.head] = Vec3::new(yaw.sin(), 0.0, yaw.cos()) * FORCE * gust * scale;
        self.head = (self.head + 1) % RING;
        self.frame = self.frame.wrapping_add(1);
    }

    /// The force for the soft body at this position in its fighter's list.
    pub fn force(&self, body: usize) -> Vec3 {
        self.force_at_phase(body * PHASE_STEP % PHASES)
    }

    /// The force this many steps back in the ring.
    pub fn force_at_phase(&self, phase: usize) -> Vec3 {
        self.ring[(self.head + RING * 2 - phase % PHASES - 1) % RING]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn force_stays_near_the_measured_size() {
        let mut wind = Wind::new();
        for _ in 0..600 {
            wind.step(1.0);
            let size = wind.force(0).length();
            assert!(size > FORCE * 0.6 && size < FORCE * 1.4, "{size}");
        }
    }

    #[test]
    fn later_bodies_read_older_forces() {
        let mut wind = Wind::new();
        for _ in 0..3 {
            wind.step(1.0);
        }
        let newest = wind.ring[(wind.head + RING - 1) % RING];
        assert_eq!(wind.force(0), newest);
        assert_eq!(wind.force(1), Vec3::ZERO, "phase 3 is older than the third frame");
    }
}
