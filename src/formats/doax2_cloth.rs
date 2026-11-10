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
    /// The thread record's angle limits (+0x10 a list of their offsets, +0x14 its length).
    pub limits: Vec<AngleLimit>,
}

/// An angle limit on a thread's links (records of 0x40, applied by `0x82499B40` when their
/// type is 1). Link k runs from entry `start + stride * k` to the next by `stride`, for
/// `count` links. Its limit is `first` for the first `first_links` links, then grows by `step`
/// a link (cumulative) up to `max`. A link bent from the one before it (or, for the thread's
/// first link, from its rest direction turned by `bone`) by more than its limit is turned back
/// to the limit, and the child and every later entry move by `factor` times the change.
#[derive(Debug, Clone, Copy)]
pub struct AngleLimit {
    pub kind: u32,
    pub first: f32,
    pub first_links: i32,
    pub step: f32,
    pub max: f32,
    pub count: i32,
    pub start: i32,
    pub stride: i32,
    /// +0x2C, the CAT bone whose frame turns the rest direction (1, the head, on Lei Fang).
    pub bone: i32,
    pub factor: f32,
}

/// A collider on a CAT bone, in that bone's space.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Shape {
    /// Type 4: centre and radii.
    Ellipsoid { centre: Vec3, radii: Vec3 },
    /// Type 8: a box around `centre` whose half axes are `axes` (columns, in the bone's frame):
    /// y is the record's axis (+0x40) times its half height (+0x64), z its front (+0x50) made
    /// square to y and scaled by +0x68, x across by +0x60. Checked against the matrices the
    /// running game builds for each collider (2026-11-10, Kasumi's `AA`).
    Box { centre: Vec3, axes: glam::Mat3 },
    /// Type 1: from `start` along `segment` (the upper arms' run along the bone's x, -x on the
    /// right arm), with `radius`.
    Capsule { start: Vec3, segment: Vec3, radius: f32 },
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
    /// segment rather than the kept contact). Set on four records of Lei Fang's chain (the
    /// head's sphere of 15.5 cm, the chest's box, the upper arms' capsules), whose word at
    /// +0x18 is always the offset in the chain of a later record (or 0, the end): they read as
    /// bounding volumes guarding the records up to there.
    pub flag: u32,
    /// For a flagged record, the index of the collider its +0x18 points to (the end for 0).
    pub skip_to: Option<usize>,
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
    /// Per particle, its triangles' other two corners (`TRIANGLE`, 8 bytes each).
    pub triangles: Vec<Vec<(usize, usize)>>,
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
    let mut triangles = Vec::with_capacity(count);
    for i in 0..count {
        let p = grid + 0x40 + 0x30 * i;
        particles.push(vec3(cat, p)?);
        let (triangle_count, triangle_at) = (be32(cat, p + 0x18)? as usize, be32(cat, p + 0x1C)? as usize);
        triangles.push(
            (0..triangle_count.min(64))
                .map(|t| {
                    let t = grid + triangle_at + 8 * t;
                    Ok((index(be32(cat, t)?)?, index(be32(cat, t + 4)?)?))
                })
                .collect::<Result<Vec<_>>>()?,
        );
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
        // (1, particle as an offset from GRID_DATA, 2, 0) after the position.
        let particle = index(be32(cat, fix + 0x20 + 0x20 * k + 0x14)?)?;
        fixed[particle] = true;
    }

    // A stream from +0x60: 0x80000000 | particle, then the vertices it writes (layout << 24 |
    // byte offset), one or more (two on some of Kasumi's `AB` particles). +0x20 counts the
    // particles, +0x24 the vertices.
    let mut writes = Vec::new();
    let words = be32(cat, write + 0x20)? as usize + be32(cat, write + 0x24)? as usize;
    let mut particle = None;
    for k in 0..words.min(0x10000) {
        let word = be32(cat, write + 0x60 + 4 * k)?;
        if word & 0x8000_0000 != 0 {
            particle = Some((word & 0xFFFFFF) as usize);
        } else {
            let particle = particle.with_context(|| format!("write {k}: {word:#x} before a particle"))?;
            writes.push((particle, word >> 24, (word & 0xFFFFFF) as usize));
        }
    }

    // Two lists of threads, at +0x18 and +0x20 with their counts after.
    let mut threads = Vec::new();
    for list in [0x18, 0x20] {
        let (first, number) = (be32(cat, thread + list)? as usize, be32(cat, thread + list + 4)? as usize);
        for k in 0..number {
            let entry = thread + first + 0x18 * k;
            let (kind, entries_at, entry_count) = (be32(cat, entry)?, be32(cat, entry + 8)? as usize, be32(cat, entry + 12)? as usize);
            let (limits_at, limit_count) = (be32(cat, entry + 0x10)? as usize, be32(cat, entry + 0x14)? as usize & 0xFFFF);
            let limits = (0..limit_count.min(16))
                .map(|q| {
                    let r = thread + be32(cat, thread + limits_at + 4 * q)? as usize;
                    let int = |o: usize| be32(cat, r + o).map(|v| v as i32);
                    Ok(AngleLimit {
                        kind: be32(cat, r)?,
                        first: bef32(cat, r + 0x10)?,
                        first_links: int(0x14)?,
                        step: bef32(cat, r + 0x18)?,
                        max: bef32(cat, r + 0x1C)?,
                        count: int(0x20)?,
                        start: int(0x24)?,
                        stride: int(0x28)?,
                        bone: int(0x2C)?,
                        factor: bef32(cat, r + 0x30)?,
                    })
                })
                .collect::<Result<Vec<_>>>()?;
            let mut thread_out = Thread { kind, particles: Vec::new(), next_rest: Vec::new(), previous_rest: Vec::new(), limits };
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
        let mut offsets = Vec::new();
        let mut skips = Vec::new();
        loop {
            offsets.push(record - chain);
            skips.push(be32(cat, record + 0x18)? as usize);
            let (kind, bone, next) = (be32(cat, record)?, be32(cat, record + 4)? as usize, be32(cat, record + 8)? as usize);
            let s = record + 0x30;
            let shape = match kind {
                4 => Shape::Ellipsoid { centre: vec3(cat, s)?, radii: vec3(cat, s + 0x10)? },
                8 => {
                    let axis = vec3(cat, s + 0x10)?;
                    let up = axis.normalize_or(Vec3::Y);
                    let front = vec3(cat, s + 0x20)?;
                    let depth = (front - up * front.dot(up)).normalize_or(Vec3::Z);
                    let across = up.cross(depth);
                    let half = vec3(cat, s + 0x30)?;
                    Shape::Box { centre: vec3(cat, s)?, axes: glam::Mat3::from_cols(across * half.x, axis * half.y, depth * half.z) }
                }
                1 => Shape::Capsule { start: vec3(cat, s)?, segment: vec3(cat, s + 0x10)?, radius: bef32(cat, s + 0x1C)? },
                2 => Shape::Sphere { centre: vec3(cat, s)?, radius: bef32(cat, s + 0x0C)? },
                other => Shape::Other(other),
            };
            colliders.push(Collider { kind, bone, flag: be32(cat, record + 0x14)?, skip_to: None, shape });
            if next == 0 || colliders.len() >= 64 {
                break;
            }
            record = chain + next;
        }
        for (k, collider) in colliders.iter_mut().enumerate() {
            if collider.flag != 0 {
                let to = skips[k];
                collider.skip_to = Some(if to == 0 { offsets.len() } else { offsets.iter().position(|&o| o == to).unwrap_or(offsets.len()) });
            }
        }
    }

    Ok(Cloth { name, property, particles, rows, edges, triangles, fixed, writes, threads, colliders })
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
    /// For `advance`: the steps run and the head matrix of the last frame.
    steps: u64,
    last_head: Mat4,
    /// Per particle and collider, how deep the particle sat at rest (`level`), set when the
    /// cloth starts on the bones.
    rest_levels: Vec<f32>,
}

impl ClothSim {
    pub fn new(cloth: Cloth) -> Self {
        let n = cloth.particles.len();
        Self {
            position: cloth.particles.clone(),
            previous: cloth.particles.clone(),
            velocity: vec![Vec3::ZERO; n],
            normal: vec![Vec3::Y; n],
            cloth,
            started: false,
            steps: 0,
            last_head: Mat4::IDENTITY,
            rest_levels: Vec::new(),
        }
    }

    /// Steps a second at the file's values: the game steps once a frame at 30 frames a second
    /// with factor 1, and DOAX2 Recomp's 60 fps mode halves the values (`RECOMP_60_FACTOR`).
    pub const STEPS_PER_SECOND: f32 = 30.0;

    /// Brings the cloth to `ticks` steps at factor 1. On frames without a step the particles
    /// only go with the head (the new head matrix times the inverse of the last), as the game
    /// does between steps; a long gap runs at most four steps.
    pub fn advance(&mut self, head: Mat4, bone: &dyn Fn(usize) -> Option<Mat4>, ticks: f32) {
        let target = ticks.max(0.0).floor() as u64;
        // A jump of the head (the first pose of a motion, a cut) starts the cloth at rest there.
        if self.started && (head.w_axis - self.last_head.w_axis).length() > 0.5 {
            self.started = false;
        }
        if !self.started {
            self.reset(head);
            self.update_normals();
            self.steps = target;
            self.last_head = head;
            let frames: Vec<Option<Mat4>> = self.cloth.colliders.iter().map(|c| bone(c.bone).map(|m| m.inverse())).collect();
            self.rest_levels = self
                .position
                .iter()
                .flat_map(|p| {
                    let frames = &frames;
                    self.cloth.colliders.iter().enumerate().map(move |(k, c)| frames[k].map_or(1.0, |inverse| level(&c.shape, inverse.transform_point3(*p))))
                })
                .collect();
        } else if self.steps > target + 1 {
            self.steps = target;
        }
        if self.steps >= target {
            let carry = head * self.last_head.inverse();
            for p in self.position.iter_mut().chain(self.previous.iter_mut()) {
                *p = carry.transform_point3(*p);
            }
            for n in &mut self.normal {
                *n = carry.transform_vector3(*n).normalize_or(Vec3::Y);
            }
        } else {
            for _ in 0..(target - self.steps).min(4) {
                self.step(head, bone, 1.0);
            }
            self.steps = target;
        }
        self.last_head = head;
    }

    /// Normals from the cloth's triangles around each particle, as the air term needs them.
    pub fn update_normals(&mut self) {
        for (i, corners) in self.cloth.triangles.iter().enumerate() {
            let p = self.position[i];
            let sum: Vec3 = corners.iter().map(|&(a, b)| (self.position[a] - p).cross(self.position[b] - p)).sum();
            self.normal[i] = sum.normalize_or(self.normal[i]);
        }
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
        Self::collide(&self.cloth, &mut self.position, &self.previous, &self.rest_levels, bone);
        // Threads: the two-sided cap in four interleaved sweeps, then for type 1 the root-first
        // cap moving the child only.
        let cap = c.p(0x48);
        for thread in &c.threads {
            two_sided(&mut self.position, &c.fixed, thread, cap);
            if thread.kind == 1 {
                one_sided(&mut self.position, &c.fixed, thread, cap);
            }
            if matches!(thread.kind, 1 | 2 | 5) {
                for limit in thread.limits.iter().filter(|l| l.kind == 1) {
                    bend_limit(&mut self.position, &c.fixed, &c.particles, thread, limit, head);
                }
            }
        }
        Self::collide(&self.cloth, &mut self.position, &self.previous, &self.rest_levels, bone);
        self.update_normals();
    }

    /// Stops particles where their move enters a collider: a particle outside a shape before
    /// the step and inside after goes to where the move crosses its surface; one already inside
    /// stays (the running game leaves particles near the roots inside the head's ellipsoid, up
    /// to row 7). Where the game puts the particle after the hit (sliding, the move's rest) is
    /// not read. A flagged record is a bounding volume: a particle outside it skips the records
    /// up to the one it points to, and it is not collided with itself.
    fn collide(cloth: &Cloth, position: &mut [Vec3], before: &[Vec3], rest_levels: &[f32], bone: &dyn Fn(usize) -> Option<Mat4>) {
        let frames: Vec<Option<(Mat4, Mat4)>> = cloth.colliders.iter().map(|c| bone(c.bone).map(|m| (m, m.inverse()))).collect();
        for i in 0..position.len() {
            if cloth.fixed[i] {
                continue;
            }
            let mut k = 0;
            while k < cloth.colliders.len() {
                let collider = &cloth.colliders[k];
                let Some((matrix, inverse)) = frames[k] else {
                    k += 1;
                    continue;
                };
                let local = inverse.transform_point3(position[i]);
                if let Some(skip) = collider.skip_to {
                    k = if inside(&collider.shape, local) { k + 1 } else { skip.max(k + 1) };
                    continue;
                }
                let start = inverse.transform_point3(before[i]);
                let rest = rest_levels.get(i * cloth.colliders.len() + k).copied().unwrap_or(1.0).min(1.0);
                if inside(&collider.shape, local) && inside(&collider.shape, start) {
                    // Already inside: no deeper than at rest (the viewer's rule).
                    if level(&collider.shape, local) < rest {
                        position[i] = matrix.transform_point3(raise(&collider.shape, local, rest));
                    }
                } else if inside(&collider.shape, local) {
                    // Bisect the move for the surface.
                    let (mut outside, mut inner) = (start, local);
                    for _ in 0..12 {
                        let middle = (outside + inner) * 0.5;
                        if inside(&collider.shape, middle) {
                            inner = middle;
                        } else {
                            outside = middle;
                        }
                    }
                    position[i] = matrix.transform_point3(outside);
                }
                k += 1;
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

/// How deep a point (in the collider's bone frame) is: below 1 inside, 0 at the centre or axis.
fn level(shape: &Shape, p: Vec3) -> f32 {
    match *shape {
        Shape::Ellipsoid { centre, radii } => ((p - centre) / radii).length(),
        Shape::Sphere { centre, radius } => (p - centre).length() / radius,
        Shape::Capsule { start, segment, radius } => {
            let t = ((p - start).dot(segment) / segment.length_squared().max(1e-12)).clamp(0.0, 1.0);
            (p - (start + segment * t)).length() / radius
        }
        Shape::Box { centre, axes } => (axes.inverse() * (p - centre)).abs().max_element(),
        Shape::Other(_) => f32::INFINITY,
    }
}

/// The point moved out from the shape's centre (or axis) to depth `target`.
fn raise(shape: &Shape, p: Vec3, target: f32) -> Vec3 {
    let now = level(shape, p);
    if now <= 0.0 || !now.is_finite() {
        return p;
    }
    let scale = target / now;
    match *shape {
        Shape::Ellipsoid { centre, .. } | Shape::Sphere { centre, .. } | Shape::Box { centre, .. } => centre + (p - centre) * scale,
        Shape::Capsule { start, segment, .. } => {
            let t = ((p - start).dot(segment) / segment.length_squared().max(1e-12)).clamp(0.0, 1.0);
            let axis = start + segment * t;
            axis + (p - axis) * scale
        }
        Shape::Other(_) => p,
    }
}

/// Whether a point (in the collider's bone frame) is inside its shape.
fn inside(shape: &Shape, p: Vec3) -> bool {
    match *shape {
        Shape::Ellipsoid { centre, radii } => ((p - centre) / radii).length_squared() < 1.0,
        Shape::Sphere { centre, radius } => (p - centre).length_squared() < radius * radius,
        Shape::Capsule { start, segment, radius } => {
            let t = ((p - start).dot(segment) / segment.length_squared().max(1e-12)).clamp(0.0, 1.0);
            (p - (start + segment * t)).length_squared() < radius * radius
        }
        Shape::Box { centre, axes } => {
            let unit = axes.inverse() * (p - centre);
            unit.abs().max_element() < 1.0
        }
        Shape::Other(_) => false,
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

/// `0x82499B40`: turns each link of `limit` back to its largest angle from the link before.
/// The first link's reference is its rest direction turned by `head` (the game turns it by the
/// limit's bone, the head on the files read).
fn bend_limit(x: &mut [Vec3], locked: &[bool], rest: &[Vec3], thread: &Thread, limit: &AngleLimit, head: Mat4) {
    let p = &thread.particles;
    let n = p.len() as i32;
    let mut angle = limit.first;
    for k in 0..limit.count {
        let i = limit.start + limit.stride * k;
        let child = i + limit.stride;
        if i < 0 || i >= n || child < 0 || child >= n {
            return;
        }
        if k >= limit.first_links {
            angle += limit.step;
        }
        if limit.step > 0.0 && angle > limit.max {
            angle = limit.max;
        }
        if limit.step < 0.0 && angle < limit.max {
            angle = limit.max;
        }
        let (parent, child) = (p[i as usize], p[child as usize]);
        let d = x[child] - x[parent];
        let grand = i - limit.stride;
        let reference = if k == 0 && !(0..n).contains(&grand) {
            head.transform_vector3(rest[child] - rest[parent])
        } else if (0..n).contains(&grand) {
            x[parent] - x[p[grand as usize]]
        } else {
            continue;
        };
        if reference.length_squared() <= 0.0 || d.length_squared() <= 0.0 {
            continue;
        }
        let bent = reference.angle_between(d);
        if angle >= bent {
            continue;
        }
        let axis = reference.cross(d);
        if axis.length_squared() <= 0.0 {
            continue;
        }
        let turned = glam::Quat::from_axis_angle(axis.normalize(), angle) * reference;
        let delta = (turned.normalize() * d.length() - d) * limit.factor;
        let mut j = i + limit.stride;
        while (0..n).contains(&j) {
            let q = p[j as usize];
            if !locked[q] {
                x[q] += delta;
            }
            j += limit.stride;
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
                assert!(cloth.writes.len() >= cloth.particles.len(), "{name} {}", cloth.name);
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
