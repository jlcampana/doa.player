//! DOAX3 Fortune's motions: `MPM` files (`*_INGAME_MPM.mpm`, `*_FACE_MPM.mpm`, the scenes'
//! `CNT_*_BODY_*_MPM.mpm`). Read 2026-11-18 from the files and from the game's code (the curve
//! stepper at 0x5702A0 in `eboot.bin`, a fake-signed SELF whose segments are plain).
//!
//! Layout: a `char_dat` holding a `char_dat` (at 0x40) with six children. Child 0 is a list of
//! u64 offsets to the motions, ended by 0; child 3 starts with a flag word, the motion count,
//! the bone count and then the first bone's name (`MOT00_Hips` for the body, `OPT_Face_Root`
//! for faces). A motion is `bones * 9` u64 offsets to curves, then 0: per bone the position
//! (x, y, z), the rotation (x, y, z, radians) and the scale (x, y, z), each a local value, not
//! a change from the rest pose. Every offset, the list's and the motions', is from child 0.
//! Children 4 and 5 (a hierarchy and a `g` chunk) are not read.

use anyhow::{bail, ensure, Context, Result};
use glam::{Mat4, Quat, Vec3};

/// One motion: `bones * 9` channels of one value per frame each.
#[derive(Debug, Clone)]
pub struct Motion {
    pub frames: usize,
    pub channels: Vec<Vec<f32>>,
}

#[derive(Debug, Clone)]
pub struct MotionFile {
    pub bones: usize,
    /// The first bone's name, which names the set: `MOT00_Hips` or `OPT_Face_Root`.
    pub root: String,
    pub motions: Vec<Motion>,
}

fn u32_at(d: &[u8], at: usize) -> Result<u32> {
    Ok(u32::from_le_bytes(d.get(at..at + 4).context("past the end")?.try_into()?))
}

fn u64_at(d: &[u8], at: usize) -> Result<u64> {
    Ok(u64::from_le_bytes(d.get(at..at + 8).context("past the end")?.try_into()?))
}

fn s8(b: u8) -> i32 {
    b as i8 as i32
}

/// A curve as the game steps it (0x5702A0): an integer accumulator, one value a frame, times
/// 1/16384. The stream starts 00 07. Each command has a 16-bit count in bytes 0 and 1 and its
/// code in byte 2:
/// - 0x80: last value, 24 bits (bytes 0, 1, 3); 0x7A: last value, 32 bits (0, 1, 3, 4).
/// - 0x7F: `count` frames of 8-bit deltas, the first in byte 3, the rest one byte a frame.
/// - 0x7E: the same with 16-bit deltas (bytes 3 and 4, then two bytes a frame).
/// - 0x7D, 0x7C: one delta (8 bits in byte 3; 16 bits in 3 and 4) added `count` frames; the
///   stream stays on it and skips one byte after the run, as the game does.
/// - 0x7B: a value of 32 bits (bytes 0, 1, 3, 4) for one frame.
/// - anything else: a value of 24 bits (bytes 0 to 2, the top byte signed) for one frame.
///
/// The delta codes are not taken on the first frame while the accumulator is still 0, as in
/// the game (they read as values then).
pub fn decode_curve(d: &[u8], start: usize) -> Result<Vec<f32>> {
    ensure!(d.get(start..start + 2) == Some(&[0, 7][..]), "curve at {start:#x} does not start 00 07");
    let byte = |p: usize| -> Result<u8> { d.get(p).copied().with_context(|| format!("curve runs past the end at {p:#x}")) };
    let mut p = start + 2;
    let mut out: Vec<i32> = Vec::new();
    let mut acc: i32 = 0;
    let mut run: u32 = 0;
    let mut repeat = false;
    let mut first = true;
    loop {
        ensure!(out.len() < 1_000_000, "curve at {start:#x} does not end");
        if run != 0 {
            let delta = if run >= 0x10000 {
                let delta = byte(p)? as i32 | (s8(byte(p + 1)?) << 8);
                run = if run == 0x10001 { 0 } else { run - 1 };
                if !repeat {
                    p += 2;
                }
                delta
            } else {
                let delta = s8(byte(p)?);
                if !repeat {
                    p += 1;
                }
                run -= 1;
                delta
            };
            acc = acc.wrapping_add(delta);
            out.push(acc);
            continue;
        }
        if repeat {
            p += 1;
            repeat = false;
        }
        let (b0, b1, op) = (byte(p)?, byte(p + 1)?, byte(p + 2)?);
        let count = b0 as u32 | (b1 as u32) << 8;
        let special = first && acc == 0;
        first = false;
        match op {
            0x80 => {
                out.push(b0 as i32 | (b1 as i32) << 8 | s8(byte(p + 3)?) << 16);
                break;
            }
            0x7A if !special => {
                out.push(i32::from_le_bytes([b0, b1, byte(p + 3)?, byte(p + 4)?]));
                break;
            }
            0x7F if !special => {
                acc = acc.wrapping_add(s8(byte(p + 3)?));
                p += 4;
                run = count.saturating_sub(1);
            }
            0x7E if !special => {
                acc = acc.wrapping_add(byte(p + 3)? as i32 | (s8(byte(p + 4)?) << 8));
                p += 5;
                run = if count == 1 { 0 } else { count + 0xFFFF };
            }
            0x7D if !special => {
                acc = acc.wrapping_add(s8(byte(p + 3)?));
                p += 3;
                repeat = true;
                run = count.saturating_sub(1);
            }
            0x7C if !special => {
                acc = acc.wrapping_add(byte(p + 3)? as i32 | (s8(byte(p + 4)?) << 8));
                p += 4;
                repeat = true;
                run = if count == 1 { 0 } else { count + 0xFFFF };
            }
            0x7B => {
                acc = i32::from_le_bytes([b0, b1, byte(p + 3)?, byte(p + 4)?]);
                p += 5;
            }
            _ => {
                acc = b0 as i32 | (b1 as i32) << 8 | s8(op) << 16;
                p += 3;
            }
        }
        out.push(acc);
    }
    Ok(out.into_iter().map(|v| v as f32 / 16384.0).collect())
}

pub fn parse_mpm(d: &[u8]) -> Result<MotionFile> {
    ensure!(d.get(..8) == Some(b"char_dat".as_slice()), "not a char_dat");
    let inner = 0x40;
    ensure!(d.get(inner..inner + 8) == Some(b"char_dat".as_slice()), "no inner char_dat");
    let children = u32_at(d, inner + 0x14)? as usize;
    ensure!(children >= 4, "{children} children");
    let table: Vec<usize> = (0..children).map(|i| u32_at(d, inner + 0x30 + i * 4).map(|v| v as usize)).collect::<Result<_>>()?;
    let info = inner + table[3];
    let bones = u32_at(d, info + 8)? as usize;
    ensure!((1..1000).contains(&bones), "{bones} bones");
    let root = String::from_utf8_lossy(d.get(info + 0x10..info + 0x30).context("bone name")?.split(|&b| b == 0).next().unwrap_or_default()).into_owned();
    let list = inner + table[0];
    let mut motions = Vec::new();
    for k in 0.. {
        let offset = u64_at(d, list + k * 8)? as usize;
        if offset == 0 {
            break;
        }
        let mut channels = Vec::with_capacity(bones * 9);
        for c in 0..bones * 9 {
            let curve = u64_at(d, list + offset + c * 8)? as usize;
            channels.push(decode_curve(d, list + curve).with_context(|| format!("motion {k} channel {c}"))?);
        }
        let frames = channels.iter().map(Vec::len).max().unwrap_or(0);
        if channels.iter().any(|c| c.len() != frames) {
            bail!("motion {k}: channels of different lengths");
        }
        motions.push(Motion { frames, channels });
    }
    Ok(MotionFile { bones, root, motions })
}

/// The rotation from the three angles, applied x first, then y, then z (Maya's default `xyz`).
/// Not read from the game's code: chosen as the order that sinks the feet least into the
/// ground over Kasumi's 51 in-game motions (5th percentile of the lowest foot or toe node:
/// xyz 0.007, xzy 0.003, yxz -0.011, the other three about -0.07), and it looks right by eye.
/// `DOAX3_EULER` (xyz, xzy, yxz, yzx, zxy or zyx) overrides it, for checking.
pub fn rotation(rx: f32, ry: f32, rz: f32) -> Quat {
    static ORDER: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    let order = ORDER.get_or_init(|| std::env::var("DOAX3_EULER").unwrap_or_else(|_| "xyz".into()));
    let (x, y, z) = (Quat::from_rotation_x(rx), Quat::from_rotation_y(ry), Quat::from_rotation_z(rz));
    // Applied first to last: the last one multiplies on the left.
    match order.as_str() {
        "xyz" => z * y * x,
        "xzy" => y * z * x,
        "yxz" => z * x * y,
        "yzx" => x * z * y,
        "zxy" => y * x * z,
        _ => x * y * z,
    }
}

impl Motion {
    /// Whether the motion places the hips. Kasumi's motion 0 (100 frames) does not: every
    /// channel holds the rest pose and the hips stay at the origin, so the body would stand half
    /// under the ground; the others start with the hips 0.87 to 0.93 m up.
    pub fn places_root(&self) -> bool {
        (0..3).any(|k| self.channels.get(k).is_some_and(|v| v.iter().any(|x| x.abs() > 1e-4)))
    }

    /// A channel at a frame, linear between whole frames, held at the ends.
    pub fn value(&self, channel: usize, frame: f32) -> f32 {
        let Some(values) = self.channels.get(channel) else { return 0.0 };
        if values.is_empty() {
            return 0.0;
        }
        let f = frame.clamp(0.0, (values.len() - 1) as f32);
        let i = f.floor() as usize;
        let j = (i + 1).min(values.len() - 1);
        let t = f - i as f32;
        values[i] + (values[j] - values[i]) * t
    }

    /// A bone's local matrix at a frame: translation, rotation, scale. Angles are interpolated
    /// along the shorter way, as they wrap at 2π in the files (0 → 6.283 on some channels).
    pub fn local(&self, bone: usize, frame: f32) -> Mat4 {
        let c = bone * 9;
        let angle = |k: usize| {
            let values = &self.channels[c + k];
            if values.is_empty() {
                return 0.0;
            }
            let f = frame.clamp(0.0, (values.len() - 1) as f32);
            let i = f.floor() as usize;
            let j = (i + 1).min(values.len() - 1);
            let mut d = values[j] - values[i];
            d -= (d / std::f32::consts::TAU).round() * std::f32::consts::TAU;
            values[i] + d * (f - i as f32)
        };
        let t = Vec3::new(self.value(c, frame), self.value(c + 1, frame), self.value(c + 2, frame));
        let (rx, ry, rz) = (angle(3), angle(4), angle(5));
        let s = Vec3::new(self.value(c + 6, frame), self.value(c + 7, frame), self.value(c + 8, frame));
        let r = rotation(rx, ry, rz);
        Mat4::from_scale_rotation_translation(s, r, t)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Runs the hair stand-in over a motion, printing how far the physical nodes stray.
    /// `DOAX3_MOTION=14 cargo test spring_bones_over_a_motion -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn spring_bones_over_a_motion() {
        use super::super::doax3::{self, Archives};
        let archives = Archives::open(&doax3::game_dir()).unwrap();
        let read = |tmc: &str| doax3::parse_model(&archives.read(tmc).unwrap(), &archives.read(&doax3::tmcl_path(tmc)).unwrap()).unwrap();
        let names: Vec<String> = archives.names().map(str::to_string).collect();
        let code = std::env::var("DOAX3_CHAR").unwrap_or("KAS".into());
        let mut body = read(&doax3::costumes(&names, &code)[0]);
        doax3::place_objects(&mut body, Mat4::IDENTITY, |_| Mat4::IDENTITY);
        let mut hair = read(&doax3::hairstyles(&names, &code)[0]);
        doax3::place_on(&body, &mut hair).unwrap();
        let file = parse_mpm(&archives.read(&format!("00/character/{code}/animation/{code}_INGAME_MPM.mpm")).unwrap()).unwrap();
        let index: usize = std::env::var("DOAX3_MOTION").ok().and_then(|v| v.parse().ok()).unwrap_or(14);
        let motion = &file.motions[index];
        let mut spring = SpringBones::new(&hair, true).unwrap();
        let hold: Option<usize> = std::env::var("DOAX3_HOLD").ok().and_then(|v| v.parse().ok());
        for step in 0..motion.frames.min(600) {
            let frame = hold.unwrap_or(step);
            let body_posed = pose_nodes(&body.nodes, |_| None, |i| body_bone(&body.nodes[i].name).map(|b| motion.local(b, frame as f32)));
            let head = body.nodes.iter().position(|n| n.name == "MOT01_Head").unwrap();
            let animated = pose_nodes(&hair.nodes, |i| (hair.nodes[i].name == "MOT01_Head").then(|| body_posed[head]), |_| None);
            let at = |name: &str, offset: Vec3, r: f32| (body_posed[body.nodes.iter().position(|n| n.name == name).unwrap()].transform_point3(offset), r);
            let colliders = [at("MOT01_Head", Vec3::new(0.0, 0.07, 0.0), 0.095), at("MOT15_Neck", Vec3::ZERO, 0.06), at("MOT02_Chest", Vec3::new(0.0, 0.08, 0.0), 0.12)];
            let out = spring.step(&hair, &animated, &colliders);
            let worst = out.iter().map(|(n, m)| (m.w_axis.truncate() - animated[*n].w_axis.truncate()).length()).fold(0.0f32, f32::max);
            let turn = out
                .iter()
                .map(|(n, m)| {
                    let a = animated[*n].to_scale_rotation_translation().1.normalize();
                    let b = m.to_scale_rotation_translation().1.normalize();
                    (a.angle_between(b).to_degrees(), *n)
                })
                .fold((0.0f32, 0), |x, y| if y.0 > x.0 { y } else { x });
            if step % 40 == 0 {
                let fin = pose_nodes(&hair.nodes, |i| out.iter().find(|(n, _)| *n == i).map(|(_, m)| *m).or_else(|| (hair.nodes[i].name == "MOT01_Head").then(|| body_posed[head])), |_| None);
                let (dev, at) = fin.iter().zip(&animated).enumerate().map(|(i, (f, a))| ((f.w_axis - a.w_axis).truncate().length(), i)).fold((0.0f32, 0), |x, y| if y.0 > x.0 { y } else { x });
                println!("  final: worst place {dev:.3} on {}", hair.nodes[at].name);
                let change = |g: &[Mat4]| -> Vec<Mat4> { g.iter().zip(&hair.nodes).map(|(p, n)| *p * n.global.inverse()).collect() };
                let (cf, ca) = (change(&fin), change(&animated));
                let mut worst = (0.0f32, 0usize, 0usize);
                for (pi, skins) in hair.skins.iter().enumerate() {
                    if skins.is_empty() || !hair.drawn(pi) { continue; }
                    let a = skin(&hair.pieces[pi].vertices, skins, &ca);
                    let b = skin(&hair.pieces[pi].vertices, skins, &cf);
                    for (v, ((pa, _), (pb, _))) in a.iter().zip(&b).enumerate() {
                        let d = (*pa - *pb).length();
                        if d > worst.0 { worst = (d, pi, v); }
                    }
                }
                let s = &hair.skins[worst.1][worst.2];
                println!("  vertices: worst {:.3} m, piece {} vertex {}, nodes {:?}", worst.0, worst.1, worst.2, s.iter().filter(|x| x.1 > 0.0).map(|(n, w)| (hair.nodes[*n as usize].name.clone(), *w)).collect::<Vec<_>>());
                println!("  worst turn {:.1} degrees on {}", turn.0, hair.nodes[turn.1].name);
            }
            if step % 20 == 0 || worst > 0.5 {
                println!("step {step} frame {frame}: worst {worst:.3} m over {} nodes", out.len());
            }
        }
    }

    /// The stand-in spring lags a moving node, stays within its limit and settles.
    #[test]
    fn jiggle_lags_and_settles() {
        let mut j = Jiggle::new(0);
        let mut most: f32 = 0.0;
        for f in 0..120 {
            let target = Vec3::new(0.0, 1.0 + 0.1 * (f as f32 * 0.3).sin(), 0.0);
            most = most.max(j.step(target).length());
        }
        assert!(most > 0.005 && most <= Jiggle::MAX_OFFSET + 1e-6, "{most}");
        let still = Vec3::new(0.0, 1.0, 0.0);
        for _ in 0..300 {
            j.step(still);
        }
        assert!(j.step(still).length() < 1e-4);
    }

    /// Every channel of a motion has the same length, which checks the stepper end to end.
    #[test]
    fn kasumi_ingame_motions_decode() {
        let Ok(d) = std::fs::read("assets/doax3/00/character/KAS/animation/KAS_INGAME_MPM.mpm") else { return };
        let file = parse_mpm(&d).unwrap();
        assert_eq!(file.bones, 21);
        assert_eq!(file.root, "MOT00_Hips");
        assert_eq!(file.motions.len(), 51);
        assert_eq!(file.motions[0].frames, 100);
        assert_eq!(file.motions[44].frames, 2410);
        // Hips height and the left thigh's place, as in the model's rest pose.
        let m = &file.motions[14];
        assert!((m.value(1, 0.0) - 0.879).abs() < 0.01);
        assert!((m.value(6 * 9, 0.0) - 0.077).abs() < 0.002);
        assert!(!file.motions[0].places_root());
        assert!(file.motions[1..].iter().all(Motion::places_root));
    }
}

/// The motion's bone for a node: `MOTnn_*` is bone nn of a body motion.
pub fn body_bone(name: &str) -> Option<usize> {
    let digits = name.strip_prefix("MOT")?.get(..2)?;
    digits.parse().ok()
}

/// The face motions' bone for each node of a face model: its `OPT_Face_*` nodes in model
/// order, then `OPT_Face_Root` (44 on Kasumi). The file's hierarchy agrees: its bone 6 (the
/// jaw) is the parent of bones 1, 4, 18 and 36 (the lower lips and the tongue). The motions
/// put the root on the head and its children where rest puts them under the root, so their
/// locals replace the model's whole.
pub fn face_bones(nodes: &[super::doax3::Node]) -> Vec<Option<usize>> {
    let mut out = vec![None; nodes.len()];
    let mut next = 0;
    for (i, node) in nodes.iter().enumerate() {
        if node.name.starts_with("OPT_Face_") && node.name != "OPT_Face_Root" {
            out[i] = Some(next);
            next += 1;
        }
    }
    if let Some(root) = nodes.iter().position(|n| n.name == "OPT_Face_Root") {
        out[root] = Some(next);
    }
    out
}

/// Posed globals of a model's nodes. `fixed(node)` gives a node's global from elsewhere (a
/// part's node that the body also has); `local(node)` its local from a motion; other nodes keep
/// their rest local (from the rest globals) under their posed parent. With `fixed` and `local`
/// both empty, this gives the rest globals back.
pub fn pose_nodes(nodes: &[super::doax3::Node], fixed: impl Fn(usize) -> Option<Mat4>, local: impl Fn(usize) -> Option<Mat4>) -> Vec<Mat4> {
    let mut out: Vec<Option<Mat4>> = vec![None; nodes.len()];
    fn solve(i: usize, nodes: &[super::doax3::Node], out: &mut Vec<Option<Mat4>>, fixed: &dyn Fn(usize) -> Option<Mat4>, local: &dyn Fn(usize) -> Option<Mat4>, depth: usize) -> Mat4 {
        if let Some(m) = out[i] {
            return m;
        }
        let m = if let Some(m) = fixed(i) {
            m
        } else {
            let parent = nodes[i].parent.filter(|&p| p < nodes.len() && p != i && depth < 512);
            let parent_posed = parent.map(|p| solve(p, nodes, out, fixed, local, depth + 1));
            let rest_local = match parent {
                Some(p) => nodes[p].global.inverse() * nodes[i].global,
                None => nodes[i].global,
            };
            let own = local(i).unwrap_or(rest_local);
            parent_posed.map_or(own, |pp| pp * own)
        };
        out[i] = Some(m);
        m
    }
    for i in 0..nodes.len() {
        solve(i, nodes, &mut out, &fixed, &local, 0);
    }
    out.into_iter().map(|m| m.unwrap_or(Mat4::IDENTITY)).collect()
}

/// Skins rest vertices: each by the weighted sum of its nodes' change from rest
/// (`posed * rest⁻¹`), positions and normals.
pub fn skin(rest: &[super::model::ModelVertex], skins: &[super::doax3::Skin], change: &[Mat4]) -> Vec<(Vec3, Vec3)> {
    rest.iter()
        .zip(skins)
        .map(|(v, s)| {
            let (p, n) = (Vec3::from(v.position), Vec3::from(v.normal));
            let mut out_p = Vec3::ZERO;
            let mut out_n = Vec3::ZERO;
            let mut total = 0.0;
            for &(node, w) in s.iter().filter(|(_, w)| *w > 0.0) {
                let Some(m) = change.get(node as usize) else { continue };
                out_p += m.transform_point3(p) * w;
                out_n += m.transform_vector3(n) * w;
                total += w;
            }
            if total <= 0.0 {
                (p, n)
            } else {
                (out_p / total, out_n.normalize_or_zero())
            }
        })
        .collect()
}

/// The viewer's stand-in for the chest's soft body (DOAX3's `YWRK`, not read): a damped point
/// per breast that follows its base node (`OPT_Breast_Left_base`, `OPT_Breast_Right_base`, under
/// which hang the 18 `OPT_ywrk_*` points of each side) with a lag, once per 60 Hz frame. Its
/// lag moves the base, and with it the points, by up to `MAX_OFFSET`. Not the game's model.
#[derive(Debug, Clone)]
pub struct Jiggle {
    pub node: usize,
    position: Vec3,
    velocity: Vec3,
    started: bool,
}

impl Jiggle {
    const STIFFNESS: f32 = 0.12;
    const DAMPING: f32 = 0.82;
    const MAX_OFFSET: f32 = 0.03;

    pub fn new(node: usize) -> Self {
        Self { node, position: Vec3::ZERO, velocity: Vec3::ZERO, started: false }
    }

    /// One frame toward `target` (the node's posed place); gives the offset to add, scaled.
    pub fn step(&mut self, target: Vec3) -> Vec3 {
        if !self.started {
            self.position = target;
            self.started = true;
        }
        self.velocity = self.velocity * Self::DAMPING + (target - self.position) * Self::STIFFNESS;
        self.position += self.velocity;
        let offset = (self.position - target).clamp_length_max(Self::MAX_OFFSET);
        self.position = target + offset;
        offset
    }

    pub fn offset(&self, target: Vec3) -> Vec3 {
        if self.started { (self.position - target).clamp_length_max(Self::MAX_OFFSET) } else { Vec3::ZERO }
    }

    pub fn reset(&mut self) {
        self.started = false;
        self.velocity = Vec3::ZERO;
    }
}

/// The viewer's stand-in for DOAX3's hair and accessory physics (`ACSCLS`, whose parameters are
/// not read): every node the file lists as physical becomes a particle that follows its
/// animated place with some stiffness, keeps its rest distance to its parent, falls a little,
/// and stays out of the colliders; parents with one physical child turn toward it. Stepped at
/// 60 Hz. Not the game's model, and not exported.
#[derive(Debug, Clone)]
pub struct SpringBones {
    /// Physical nodes, parents before children.
    nodes: Vec<usize>,
    current: Vec<Vec3>,
    previous: Vec<Vec3>,
    started: bool,
    /// Whether colliders apply (hair) or not (a suit's strings).
    pub collide: bool,
}

impl SpringBones {
    const STIFFNESS: f32 = 0.08;
    const DRAG: f32 = 0.2;
    /// Metres a frame squared: real gravity at 60 Hz. Against the stiffness, a 3 cm segment
    /// the animation holds level sags by about 48°; one that hangs is not moved.
    const GRAVITY: f32 = 9.8 / 3600.0;
    const MIN_AIM_LENGTH: f32 = 0.03;
    const MAX_STRAY: f32 = 0.12;
    const MAX_AIM: f32 = 40.0 * std::f32::consts::PI / 180.0;

    pub fn new(model: &super::doax3::Model, collide: bool) -> Option<Self> {
        if model.dynamic.is_empty() {
            return None;
        }
        let depth = |mut i: usize| {
            let mut d = 0;
            while let Some(p) = model.nodes[i].parent.filter(|_| d < 512) {
                i = p;
                d += 1;
            }
            d
        };
        let mut nodes: Vec<usize> = model.dynamic.iter().copied().filter(|&n| model.nodes[n].parent.is_some()).collect();
        nodes.sort_by_key(|&n| depth(n));
        let n = nodes.len();
        Some(Self { nodes, current: vec![Vec3::ZERO; n], previous: vec![Vec3::ZERO; n], started: false, collide })
    }

    pub fn reset(&mut self) {
        self.started = false;
    }

    /// One frame. `animated` holds the model's posed globals without physics; colliders are
    /// spheres (centre, radius). Gives the simulated globals of the physical nodes and of the
    /// parents turned toward them.
    pub fn step(&mut self, model: &super::doax3::Model, animated: &[Mat4], colliders: &[(Vec3, f32)]) -> Vec<(usize, Mat4)> {
        let nodes = &model.nodes;
        let mut result: Vec<Mat4> = animated.to_vec();
        let index: std::collections::HashMap<usize, usize> = self.nodes.iter().enumerate().map(|(i, &n)| (n, i)).collect();
        let child_count = |p: usize| self.nodes.iter().filter(|&&n| nodes[n].parent == Some(p)).count();
        let mut touched: Vec<usize> = Vec::new();
        for (i, &n) in self.nodes.iter().enumerate() {
            let Some(p) = nodes[n].parent else { continue };
            let rest_local = nodes[p].global.inverse() * nodes[n].global;
            let parent_now = result[p];
            // The animated offset from the parent, from where the parent is now: the hair keeps
            // its animated shape and only lags. (Taken through the parent's turned frame
            // instead, the chain had nothing pulling it back and swung like a loose rope,
            // tips 0.5 m off.)
            let target = parent_now.w_axis.truncate() + (animated[n].w_axis.truncate() - animated[p].w_axis.truncate());
            if !self.started {
                self.current[i] = target;
                self.previous[i] = target;
            }
            let x = self.current[i];
            let velocity = (x - self.previous[i]) * (1.0 - Self::DRAG);
            let mut next = x + velocity + (target - x) * Self::STIFFNESS - Vec3::Y * Self::GRAVITY;
            // Never further than MAX_STRAY from the target, losing the speed past it: a jump
            // in the motion (frame 0 to 200) otherwise left tips 0.8 m off for seconds.
            let stray = next - target;
            if stray.length() > Self::MAX_STRAY {
                next = target + stray.normalize() * Self::MAX_STRAY;
                self.previous[i] = next;
            }
            let anchor = parent_now.w_axis.truncate();
            let length = rest_local.w_axis.truncate().length();
            let keep = |point: Vec3| anchor + (point - anchor).normalize_or(target - anchor) * length;
            next = keep(next);
            if self.collide {
                for &(centre, radius) in colliders {
                    // Never wider than where the animated place puts the node: hair near the
                    // scalp lies inside the spheres at rest and would otherwise be pushed out.
                    let radius = radius.min((target - centre).length());
                    let away = next - centre;
                    if away.length() < radius {
                        next = keep(centre + away.normalize_or(Vec3::Y) * radius);
                    }
                }
            }
            if (next - target).length() < Self::MAX_STRAY {
                self.previous[i] = x;
            }
            self.current[i] = next;
            // Turn a parent with this one physical child toward it; the child takes the
            // parent's turned frame with its rest local rotation.
            // Only over segments of 3 cm or more, and by 40° at most: on short ones a centimetre
            // off turned the bone by over 100° and twisted the strands' vertices.
            if child_count(p) == 1 && length > Self::MIN_AIM_LENGTH {
                let (scale, rotation, translation) = parent_now.to_scale_rotation_translation();
                // From where the parent's present frame puts the child (it already carries the
                // turns of the nodes above), not from the animated place, which counted them twice.
                let from = ((parent_now * rest_local).w_axis.truncate() - anchor).normalize_or_zero();
                let to = (next - anchor).normalize_or_zero();
                if from != Vec3::ZERO && to != Vec3::ZERO {
                    let arc = Quat::from_rotation_arc(from, to);
                    let (axis, angle) = arc.to_axis_angle();
                    let arc = Quat::from_axis_angle(axis, angle.min(Self::MAX_AIM));
                    let turned = arc * rotation;
                    result[p] = Mat4::from_scale_rotation_translation(scale, turned, translation);
                    if index.contains_key(&p) || !touched.contains(&p) {
                        touched.push(p);
                    }
                }
            }
            let (scale, rotation, _) = (result[p] * rest_local).to_scale_rotation_translation();
            result[n] = Mat4::from_scale_rotation_translation(scale, rotation, next);
            touched.push(n);
        }
        self.started = true;
        touched.sort_unstable();
        touched.dedup();
        touched.into_iter().map(|n| (n, result[n])).collect()
    }
}

