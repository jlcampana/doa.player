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
    }
}

/// The motion's bone for a node: `MOTnn_*` is bone nn of a body motion.
pub fn body_bone(name: &str) -> Option<usize> {
    let digits = name.strip_prefix("MOT")?.get(..2)?;
    digits.parse().ok()
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
