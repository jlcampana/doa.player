//! DOAX2's hair cloth: the `MULTI_CLOTH` block of a head CAT, and the game's step.
//!
//! The format and the step are read from the game's code (`0x82493920` on) and checked
//! against particle states read from the running game; `docs/doax2-notes.md`, "Hair", has the
//! details and what is verified. In short: the velocity step (damping, gravity, air, springs,
//! the cap on its length) matches the game to 0.015 mm a step on Kasumi's `long_l`; the
//! thread length caps follow the code; collisions are an approximation (the particle is
//! pushed out of each shape), as the game's collision code is not decoded; angle limits are
//! read but not applied.

use anyhow::{bail, Context, Result};
use glam::{Mat4, Vec3};

fn be32(data: &[u8], at: usize) -> Result<u32> {
    let bytes = data.get(at..at + 4).with_context(|| format!("read past the end at {at:#x}"))?;
    Ok(u32::from_be_bytes(bytes.try_into().unwrap()))
}

fn bef32(data: &[u8], at: usize) -> Result<f32> {
    Ok(f32::from_bits(be32(data, at)?))
}

fn vec3(data: &[u8], at: usize) -> Result<Vec3> {
    Ok(Vec3::new(bef32(data, at)?, bef32(data, at + 4)?, bef32(data, at + 8)?))
}

fn tag(data: &[u8], at: usize) -> &[u8] {
    let bytes = data.get(at..at + 16).unwrap_or(&[]);
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    &bytes[..end]
}

/// An edge: the two particles, the rest length and the kind (1 neighbours in a row and along
/// a strand, 2 diagonals, 3 two rows apart). Listed once per end in the file; kept per end here,
/// as the game sums the force on each particle over its own list.
#[derive(Debug, Clone, Copy)]
pub struct Edge {
    pub from: usize,
    pub to: usize,
    pub rest: f32,
    pub kind: u32,
}

/// A chain of particles from the root (`THREAD_DATA`): its type and, per entry, the particle
/// and the rest lengths of its edges to the next and previous entries (0 when none).
#[derive(Debug, Clone)]
pub struct Thread {
    pub kind: u32,
    pub particles: Vec<usize>,
    pub next_rest: Vec<f32>,
    pub previous_rest: Vec<f32>,
}

/// A collider on a CAT bone, in that bone's space.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Shape {
    /// Type 4: centre and radii.
    Ellipsoid { centre: Vec3, radii: Vec3 },
    /// Type 8: centre and half sizes. The two rows between them are not understood and the
    /// box is taken as aligned with the bone.
    Box { centre: Vec3, half: Vec3 },
    /// Type 1: from `start` along the bone's x for `length`, with `radius`.
    Capsule { start: Vec3, length: f32, radius: f32 },
    /// Type 2: centre and radius.
    Sphere { centre: Vec3, radius: f32 },
    /// Other types, kept by number.
    Other(u32),
}

#[derive(Debug, Clone, Copy)]
pub struct Collider {
    pub kind: u32,
    /// The CAT bone number.
    pub bone: usize,
    /// The word at +0x14: the game takes another path when it is set (a test of the move's
    /// segment rather than the kept contact). What it means for the shape is not known.
    pub flag: u32,
    pub shape: Shape,
}

/// One `CLOTH_DATA`.
#[derive(Debug, Clone)]
pub struct Cloth {
    pub name: String,
    /// The working `PROPERTY` from its tag, as floats: `property[k]` is at offset 4k. Integer
    /// words (0x30, 0x3C) are kept as their bits.
    pub property: Vec<f32>,
    /// Particle positions in the head's space, as the file has them.
    pub particles: Vec<Vec3>,
    /// Each particle's row, 0 at the root.
    pub rows: Vec<u32>,
    pub edges: Vec<Edge>,
    /// Particles held to the head (`FIX_GRID`).
    pub fixed: Vec<bool>,
    /// (particle, vertex layout, byte offset in that layout's vertex buffer).
    pub writes: Vec<(usize, u32, usize)>,
    pub threads: Vec<Thread>,
    pub colliders: Vec<Collider>,
}

impl Cloth {
    /// The working property at a byte offset from the `PROPERTY` tag.
    pub fn p(&self, offset: usize) -> f32 {
        self.property.get(offset / 4).copied().unwrap_or(0.0)
    }

    pub fn flags(&self) -> u32 {
        self.p(0x3C).to_bits()
    }
}

/// The cloths of a head CAT, or none when it has no `MULTI_CLOTH` block (word 5 zero).
pub fn read(cat: &[u8]) -> Result<Vec<Cloth>> {
    let block = be32(cat, 20)? as usize;
    if block == 0 || tag(cat, block) != b"MULTI_CLOTH" {
        return Ok(Vec::new());
    }
    let count = be32(cat, block + 0x18)? as usize;
    if count > 64 {
        bail!("{count} cloths");
    }
    (0..count)
        .map(|k| {
            let at = block + be32(cat, block + 0x40 + 4 * k)? as usize;
            read_cloth(cat, at).with_context(|| format!("cloth {k}"))
        })
        .collect()
}

fn read_cloth(cat: &[u8], at: usize) -> Result<Cloth> {
    if tag(cat, at) != b"CLOTH_DATA" {
        bail!("no CLOTH_DATA at {at:#x}");
    }
    let name = String::from_utf8_lossy(tag(cat, at + 0x70)).into_owned();
    let section = |word: usize| -> Result<usize> { Ok(at + be32(cat, at + word)? as usize) };
    let property_at = section(0x20)?;
    let grid = section(0x24)?;
    let fix = section(0x2C)?;
    let write = section(0x30)?;
    let thread = section(0x34)?;
    let colliders_at = be32(cat, at + 0x38)? as usize;
    if tag(cat, property_at) != b"PROPERTY" || tag(cat, grid) != b"GRID_DATA" || tag(cat, fix) != b"FIX_GRID" || tag(cat, write) != b"GRID_WRITE" || tag(cat, thread) != b"THREAD_DATA" {
        bail!("sections out of place in {name}");
    }
    let property = (0..0xA0 / 4).map(|k| bef32(cat, property_at + 4 * k)).collect::<Result<Vec<_>>>()?;

    // Particles, 0x30 bytes each from +0x40; offsets inside are from the GRID_DATA tag.
    let count = be32(cat, grid + 0x24)? as usize;
    if count > 4096 {
        bail!("{count} particles");
    }
    let index = |offset: u32| -> Result<usize> {
        let offset = offset as usize;
        if offset < 0x40 || (offset - 0x40) % 0x30 != 0 || (offset - 0x40) / 0x30 >= count {
            bail!("particle offset {offset:#x}");
        }
        Ok((offset - 0x40) / 0x30)
    };
    let mut particles = Vec::with_capacity(count);
    let mut rows = Vec::with_capacity(count);
    let mut edges = Vec::new();
    for i in 0..count {
        let p = grid + 0x40 + 0x30 * i;
        particles.push(vec3(cat, p)?);
        rows.push(be32(cat, p + 0x28)?);
        let (edge_count, edge_at) = (be32(cat, p + 0x10)? as usize, be32(cat, p + 0x14)? as usize);
        for e in 0..edge_count {
            let e = grid + edge_at + 16 * e;
            edges.push(Edge { from: i, to: index(be32(cat, e)?)?, rest: bef32(cat, e + 4)?, kind: be32(cat, e + 8)? });
        }
    }
    let edge_rest = |particle: usize, number: i32| -> Result<f32> {
        if number < 0 {
            return Ok(0.0);
        }
        let p = grid + 0x40 + 0x30 * particle;
        let (edge_count, edge_at) = (be32(cat, p + 0x10)? as usize, be32(cat, p + 0x14)? as usize);
        if number as usize >= edge_count {
            bail!("edge {number} of particle {particle}");
        }
        bef32(cat, grid + edge_at + 16 * number as usize + 4)
    };

    let mut fixed = vec![false; count];
    for k in 0..be32(cat, fix + 0x1C)? as usize {
        let particle = be32(cat, fix + 0x20 + 0x20 * k + 0x14)? as usize;
        *fixed.get_mut(particle).context("fixed particle out of range")? = true;
    }

    let mut writes = Vec::new();
    for k in 0..be32(cat, write + 0x20)? as usize {
        let (particle, target) = (be32(cat, write + 0x60 + 8 * k)?, be32(cat, write + 0x64 + 8 * k)?);
        if particle & 0x8000_0000 == 0 {
            bail!("write {k}: {particle:#x}");
        }
        writes.push(((particle & 0xFFFFFF) as usize, target >> 24, (target & 0xFFFFFF) as usize));
    }

    // Two lists of threads, at +0x18 and +0x20 with their counts after.
    let mut threads = Vec::new();
    for list in [0x18, 0x20] {
        let (first, number) = (be32(cat, thread + list)? as usize, be32(cat, thread + list + 4)? as usize);
        for k in 0..number {
            let entry = thread + first + 0x18 * k;
            let (kind, entries_at, entry_count) = (be32(cat, entry)?, be32(cat, entry + 8)? as usize, be32(cat, entry + 12)? as usize);
            let mut thread_out = Thread { kind, particles: Vec::new(), next_rest: Vec::new(), previous_rest: Vec::new() };
            for q in 0..entry_count {
                let e = thread + entries_at + 12 * q;
                let particle = index(be32(cat, e)?)?;
                thread_out.particles.push(particle);
                thread_out.next_rest.push(edge_rest(particle, be32(cat, e + 4)? as i32)?);
                thread_out.previous_rest.push(edge_rest(particle, be32(cat, e + 8)? as i32)?);
            }
            threads.push(thread_out);
        }
    }

    let mut colliders = Vec::new();
    if colliders_at != 0 {
        let chain = at + colliders_at;
        let mut record = chain;
        loop {
            let (kind, bone, next) = (be32(cat, record)?, be32(cat, record + 4)? as usize, be32(cat, record + 8)? as usize);
            let s = record + 0x30;
            let shape = match kind {
                4 => Shape::Ellipsoid { centre: vec3(cat, s)?, radii: vec3(cat, s + 0x10)? },
                8 => Shape::Box { centre: vec3(cat, s)?, half: vec3(cat, s + 0x30)? },
                1 => Shape::Capsule { start: vec3(cat, s)?, length: bef32(cat, s + 0x10)?, radius: bef32(cat, s + 0x1C)? },
                2 => Shape::Sphere { centre: vec3(cat, s)?, radius: bef32(cat, s + 0x0C)? },
                other => Shape::Other(other),
            };
            colliders.push(Collider { kind, bone, flag: be32(cat, record + 0x14)?, shape });
            if next == 0 || colliders.len() >= 64 {
                break;
            }
            record = chain + next;
        }
    }

    Ok(Cloth { name, property, particles, rows, edges, fixed, writes, threads, colliders })
}

/// The global factor the game applies to stiffness, gravity and the rest (`0x833B84CC`): 0.5
/// in DOAX2 Recomp's 60 fps mode, where the cloth steps every frame.
pub const RECOMP_60_FACTOR: f32 = 0.5;

/// A cloth's particle states, stepped as the game does (see the module's note).
pub struct ClothSim {
    pub cloth: Cloth,
    pub position: Vec<Vec3>,
    previous: Vec<Vec3>,
    velocity: Vec<Vec3>,
    pub normal: Vec<Vec3>,
    started: bool,
}

impl ClothSim {
    pub fn new(cloth: Cloth) -> Self {
        let n = cloth.particles.len();
        Self { position: cloth.particles.clone(), previous: cloth.particles.clone(), velocity: vec![Vec3::ZERO; n], normal: vec![Vec3::Y; n], cloth, started: false }
    }

    /// Puts every particle at its file place on the head, at rest.
    pub fn reset(&mut self, head: Mat4) {
        for (i, p) in self.cloth.particles.iter().enumerate() {
            let at = head.transform_point3(*p);
            self.position[i] = at;
            self.previous[i] = at;
            self.velocity[i] = Vec3::ZERO;
        }
        self.started = true;
    }

    /// One step. `head` takes the file's head space to the space simulated in (y up, metres);
    /// `bone` gives a CAT bone's matrix in that space, for the colliders.
    pub fn step(&mut self, head: Mat4, bone: &dyn Fn(usize) -> Option<Mat4>, factor: f32) {
        if !self.started {
            self.reset(head);
        }
        let c = &self.cloth;
        let n = c.particles.len();
        let damping = c.p(0x40);
        let max_step = c.p(0x44) * factor;
        let gravity = Vec3::new(0.0, c.p(0x34) * factor, 0.0);
        let air = c.p(0x38) * factor;
        let lift = c.p(0x64);
        // Stiffness by kind, stretched and compressed.
        let stiffness = [[0.0, 0.0], [c.p(0x4C), c.p(0x50)], [c.p(0x54), c.p(0x58)], [0.0, c.p(0x5C)]].map(|k| k.map(|v| v * factor));

        // Fixed particles go with the head before the forces.
        for i in 0..n {
            if c.fixed[i] {
                let at = head.transform_point3(c.particles[i]);
                self.position[i] = at;
                self.previous[i] = at;
            }
        }
        // Velocity relative to the frame (taken as still), damped.
        for i in 0..n {
            if !c.fixed[i] {
                self.velocity[i] = (self.position[i] - self.previous[i]) * damping;
            }
            self.previous[i] = self.position[i];
        }
        // Forces: air against the particle's normal, gravity, springs; cut to max_step.
        let mut force: Vec<Vec3> = (0..n)
            .map(|i| {
                if c.fixed[i] {
                    return Vec3::ZERO;
                }
                let relative = self.velocity[i];
                let wind = -relative;
                let speed = wind.length();
                let mut v = relative + gravity;
                if speed > 0.0 {
                    let d = wind / speed;
                    let normal = if d.dot(self.normal[i]) < 0.0 { -self.normal[i] } else { self.normal[i] };
                    let cos = d.dot(normal).clamp(0.0, 1.0);
                    let sin2 = 1.0 - cos * cos;
                    v += wind * (air * cos);
                    if sin2 > 0.0 {
                        v += (normal - d * cos) * (air * lift * speed * cos / sin2.sqrt());
                    }
                }
                v
            })
            .collect();
        for e in &c.edges {
            if c.fixed[e.from] {
                continue;
            }
            let d = self.position[e.to] - self.position[e.from];
            let length = d.length();
            if length <= 0.0 {
                continue;
            }
            let x = length - e.rest;
            let k = stiffness.get(e.kind as usize).map_or(0.0, |k| if x > 0.0 { k[0] } else { k[1] });
            force[e.from] += d / length * (k * x);
        }
        for i in 0..n {
            if c.fixed[i] {
                continue;
            }
            let mut v = force[i];
            if v.length_squared() > max_step * max_step {
                v = v.normalize() * max_step;
            }
            self.velocity[i] = v;
            self.position[i] += v;
        }
        Self::collide(&self.cloth, &mut self.position, bone);
        // Threads: the two-sided cap in four interleaved sweeps, then for type 1 the root-first
        // cap moving the child only.
        let cap = c.p(0x48);
        for thread in &c.threads {
            two_sided(&mut self.position, &c.fixed, thread, cap);
            if thread.kind == 1 {
                one_sided(&mut self.position, &c.fixed, thread, cap);
            }
        }
        Self::collide(&self.cloth, &mut self.position, bone);
    }

    /// Pushes particles out of the colliders. An approximation of the game's collision.
    fn collide(cloth: &Cloth, position: &mut [Vec3], bone: &dyn Fn(usize) -> Option<Mat4>) {
        for collider in &cloth.colliders {
            if collider.flag != 0 {
                continue;
            }
            let Some(matrix) = bone(collider.bone) else { continue };
            let inverse = matrix.inverse();
            for i in 0..position.len() {
                if cloth.fixed[i] {
                    continue;
                }
                let local = inverse.transform_point3(position[i]);
                if let Some(out) = push_out(&collider.shape, local) {
                    position[i] = matrix.transform_point3(out);
                }
            }
        }
    }

    /// Normals from the cloth's own mesh: per particle, the sum of its edges' cross products
    /// with the next edge around it is not kept, so the caller sets them from the head mesh.
    pub fn set_normals(&mut self, normals: impl Iterator<Item = (usize, Vec3)>) {
        for (i, normal) in normals {
            if let Some(out) = self.normal.get_mut(i) {
                *out = normal;
            }
        }
    }
}

/// Where a point inside a shape goes, in the shape's space; None when it is outside.
fn push_out(shape: &Shape, p: Vec3) -> Option<Vec3> {
    match *shape {
        Shape::Ellipsoid { centre, radii } => {
            let q = (p - centre) / radii;
            let length = q.length();
            (length < 1.0 && length > 0.0).then(|| centre + radii * (q / length))
        }
        Shape::Sphere { centre, radius } => {
            let d = p - centre;
            let length = d.length();
            (length < radius && length > 0.0).then(|| centre + d / length * radius)
        }
        Shape::Capsule { start, length, radius } => {
            let along = (p.x - start.x).clamp(0.0, length);
            let axis = start + Vec3::X * along;
            let d = p - axis;
            let distance = d.length();
            (distance < radius && distance > 0.0).then(|| axis + d / distance * radius)
        }
        Shape::Box { centre, half } => {
            let d = p - centre;
            let inside = half - d.abs();
            if inside.min_element() <= 0.0 {
                return None;
            }
            // Out through the nearest face.
            let mut out = p;
            if inside.x <= inside.y && inside.x <= inside.z {
                out.x = centre.x + half.x * d.x.signum();
            } else if inside.y <= inside.z {
                out.y = centre.y + half.y * d.y.signum();
            } else {
                out.z = centre.z + half.z * d.z.signum();
            }
            Some(out)
        }
        Shape::Other(_) => None,
    }
}

/// `0x824988D0`: caps each link at its rest length times `cap`, in four sweeps starting at
/// links 0, 2, 1 and 3 and stepping by four, sharing the correction between the two ends by how
/// stretched their other links are. Type 1, 2 and 5 roots do not move.
fn two_sided(x: &mut [Vec3], locked: &[bool], thread: &Thread, cap: f32) {
    let p = &thread.particles;
    let n = p.len();
    let kind = thread.kind;
    for start in [0usize, 2, 1, 3] {
        let mut s = start;
        while s + 1 < n {
            let (a, b) = (p[s], p[s + 1]);
            let d = x[b] - x[a];
            let limit = thread.next_rest[s] * cap;
            let length2 = d.length_squared();
            if limit * limit < length2 {
                let root = s == 0 && matches!(kind, 1 | 2 | 5);
                let tip = matches!(kind, 2 | 5) && s + 1 == n - 1;
                if !(root && tip) {
                    let share = if root {
                        0.0
                    } else if tip {
                        1.0
                    } else {
                        let mut before = 0.0;
                        let mut after = 0.0;
                        if s != 0 || kind == 3 {
                            let q = if s > 0 { s - 1 } else { n - 1 };
                            let rest = thread.previous_rest[s] * cap;
                            let m2 = (x[p[q]] - x[a]).length_squared();
                            if rest * rest < m2 {
                                before = m2.sqrt() / rest - 1.0;
                            }
                        }
                        if s + 2 != n || kind == 3 {
                            // The game measures the next link from this link's first end.
                            let q = (s + 2) % n;
                            let rest = thread.next_rest[s + 1] * cap;
                            let m2 = (x[p[q]] - x[a]).length_squared();
                            if rest * rest < m2 {
                                after = m2.sqrt() / rest - 1.0;
                            }
                        }
                        let total = before + after;
                        if total == 0.0 {
                            0.5
                        } else {
                            after / total
                        }
                    };
                    let length = length2.sqrt();
                    let excess = length - limit;
                    let unit = d / length;
                    if !locked[a] {
                        x[a] += unit * (excess * share);
                    }
                    if !locked[b] {
                        x[b] -= unit * (excess * (1.0 - share));
                    }
                }
            }
            s += 4;
        }
    }
}

/// `0x82499900` type 1: from the root, each link at most its rest length times `cap`, moving
/// the child.
fn one_sided(x: &mut [Vec3], locked: &[bool], thread: &Thread, cap: f32) {
    let p = &thread.particles;
    for s in 0..p.len().saturating_sub(1) {
        let (a, b) = (p[s], p[s + 1]);
        let d = x[b] - x[a];
        let length = d.length();
        let limit = thread.next_rest[s] * cap;
        if length > limit && !locked[b] {
            x[b] = x[a] + d / length * limit;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::formats::doax::LazyArchive;

    /// Every head with cloth reads, particles match their written vertices' layout, edges
    /// match the particles' distances, and fixed particles are row 0.
    #[test]
    #[ignore]
    fn head_cloths_read() {
        let chr = LazyArchive::open("assets/doax2/chr.afs").unwrap();
        let names: Vec<String> = chr.names().filter(|n| n.ends_with(".cat") && n.len() == 9).map(str::to_string).collect();
        let mut with_cloth = 0;
        for name in names {
            let cat = chr.read(&name).unwrap();
            let cloths = read(&cat).unwrap_or_else(|e| panic!("{name}: {e:#}"));
            if cloths.is_empty() {
                continue;
            }
            with_cloth += 1;
            for cloth in &cloths {
                for e in &cloth.edges {
                    let d = (cloth.particles[e.to] - cloth.particles[e.from]).length();
                    assert!((d - e.rest).abs() < 1e-5, "{name} {}: edge {e:?} at {d}", cloth.name);
                }
                for (i, &fixed) in cloth.fixed.iter().enumerate() {
                    assert!(!fixed || cloth.rows[i] == 0, "{name} {}: fixed particle {i} on row {}", cloth.name, cloth.rows[i]);
                }
                assert_eq!(cloth.writes.len(), cloth.particles.len(), "{name} {}", cloth.name);
                println!(
                    "{name} {}: {} particles, {} edges, {} fixed, {} threads, {} colliders, damping {} cap {}",
                    cloth.name,
                    cloth.particles.len(),
                    cloth.edges.len(),
                    cloth.fixed.iter().filter(|&&f| f).count(),
                    cloth.threads.len(),
                    cloth.colliders.len(),
                    cloth.p(0x40),
                    cloth.p(0x48)
                );
            }
        }
        assert!(with_cloth >= 14);
    }

    /// A hanging cloth stays near its rest length and does not fall through: stepping Lei
    /// Fang's hair 200 times on a still head keeps every thread link within the cap.
    #[test]
    #[ignore]
    fn still_head_settles() {
        let chr = LazyArchive::open("assets/doax2/chr.afs").unwrap();
        let cloths = read(&chr.read("vleAA.cat").unwrap()).unwrap();
        for cloth in cloths {
            let cap = cloth.p(0x48);
            let mut sim = ClothSim::new(cloth);
            for _ in 0..200 {
                sim.step(Mat4::IDENTITY, &|_| None, RECOMP_60_FACTOR);
            }
            for thread in &sim.cloth.threads {
                for s in 0..thread.particles.len() - 1 {
                    let d = (sim.position[thread.particles[s + 1]] - sim.position[thread.particles[s]]).length();
                    assert!(d <= thread.next_rest[s] * cap * 1.001, "{}: link {s} at {d}", sim.cloth.name);
                }
            }
            assert!(sim.position.iter().all(|p| p.is_finite()));
        }
    }
}
