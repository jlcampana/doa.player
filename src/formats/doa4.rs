//! Dead or Alive 4 (Xbox 360). Its costumes are an XPR2 model (`.tpr`) and a CAT, as in DOAX2,
//! but one file pair holds the whole character. Nothing here is checked against the game yet:
//! see `docs/doa4-notes.md`.

/// Where the game's archives are read from, unless `DOA4_GAME` names another folder.
pub const DEFAULT_GAME_DIR: &str = "/home/pandi/Ports/Apps/DeadOrAlive4Online-DeadOrAlive4Online/game";

/// The archives that hold costumes, each a `.tpr` followed by its `.cat`.
pub const COSTUME_ARCHIVES: [&str; 3] = ["chrone.afs", "chrtwo.afs", "chrthree.afs"];

pub fn game_dir() -> std::path::PathBuf {
    std::env::var_os("DOA4_GAME").map_or_else(|| DEFAULT_GAME_DIR.into(), Into::into)
}

use anyhow::{bail, Context, Result};

/// The characters by the three letters of their files, with the names they are taken to be.
/// Inferred from the series' file names and the costume counts, not from the game's tables:
/// `hak` (`hakyo` in `anm.afs`) as Kokoro, whose style is bajiquan, and `kor` as Hitomi, as
/// `koryu` is in DOA2U. `rtm` is not identified.
pub const ROSTER: [(&str, &str); 24] = [
    ("kas", "Kasumi"),
    ("aya", "Ayane"),
    ("ryu", "Ryu Hayabusa"),
    ("hyt", "Hayate"),
    ("ein", "Ein"),
    ("hel", "Helena Douglas"),
    ("hak", "Kokoro"),
    ("kor", "Hitomi"),
    ("lei", "Lei Fang"),
    ("tin", "Tina Armstrong"),
    ("lis", "La Mariposa"),
    ("cri", "Christie"),
    ("boy", "Eliot"),
    ("jan", "Jann Lee"),
    ("zac", "Zack"),
    ("bas", "Bass Armstrong"),
    ("bay", "Bayman"),
    ("leo", "Leon"),
    ("bra", "Brad Wong"),
    ("gen", "Gen Fu"),
    ("tng", "Tengu"),
    ("alp", "Alpha-152"),
    ("spa", "Spartan-458"),
    ("rtm", "rtm"),
];

/// A model in the costume archives: its name without `.tpr`, the archive and the entry of its
/// `.tpr` (its `.cat` is the next entry). A DOAX2 swimsuit is two files on disk instead.
#[derive(Debug, Clone, PartialEq)]
pub struct ModelFile {
    pub stem: String,
    pub archive: usize,
    pub entry: usize,
    pub loose: Option<(std::path::PathBuf, std::path::PathBuf)>,
}

/// Where the port keeps the DOAX2 swimsuits it builds for DOA4, unless `DOA4_SUITS` names
/// another folder: outside the install, in the Documents folder of its Windows prefix.
pub const DEFAULT_SUITS_DIR: &str =
    "/home/pandi/.local/share/com.quiverlauncher.app/proton/DeadOrAlive4Online-DeadOrAlive4Online/pfx/drive_c/users/steamuser/Documents/dead_or_alive_4/doax2_suits";

pub fn suits_dir() -> std::path::PathBuf {
    std::env::var_os("DOA4_SUITS").map_or_else(|| DEFAULT_SUITS_DIR.into(), Into::into)
}

/// The game's character numbers of the characters with swimsuits, by their files' letters.
/// From the port's manifest and log (`vtn` suits go to 1, `vks` to 5...); they are DOA2U's
/// numbers for Tina (1), Lei Fang (12) and Hitomi (21).
pub const SUIT_CHARACTERS: [(u32, &str); 9] = [(1, "tin"), (5, "kas"), (7, "hel"), (10, "hak"), (12, "lei"), (13, "aya"), (15, "lis"), (20, "cri"), (21, "kor")];

/// A DOAX2 swimsuit the port built in DOA4's format.
#[derive(Debug, Clone, PartialEq)]
pub struct Suit {
    /// The letters of the character it is for.
    pub code: &'static str,
    /// DOAX2's suit name and colour, as `vtn_cs_e_0`.
    pub stem: String,
    pub tpr: std::path::PathBuf,
    pub cat: std::path::PathBuf,
    /// The head it is worn with: the entry of the head's `.tpr`, counted over the three costume
    /// archives one after another. `None` for Tina, whose suits come with her head (`-` in the
    /// manifest).
    pub head_entry: Option<usize>,
}

/// The swimsuits listed in the folder's `manifest.txt`. A line reads `suit`, the character's
/// number, `own:N` or `append`, the entries of a costume's `.tpr` and `.cat`, those of the
/// head's, DOAX2's suit name, its colour, then each file with its size and SHA-1.
pub fn suits(dir: &std::path::Path) -> Vec<Suit> {
    let Ok(text) = std::fs::read_to_string(dir.join("manifest.txt")) else { return Vec::new() };
    text.lines()
        .filter_map(|line| {
            let words: Vec<&str> = line.split_whitespace().collect();
            let ["suit", character, _, _, _, head, _, _, _, tpr, _, _, cat, ..] = words[..] else { return None };
            let character: u32 = character.parse().ok()?;
            let code = SUIT_CHARACTERS.iter().find(|c| c.0 == character)?.1;
            Some(Suit { code, stem: tpr.strip_suffix(".tpr")?.to_string(), tpr: dir.join(tpr), cat: dir.join(cat), head_entry: head.parse().ok() })
        })
        .collect()
}

/// Every model of the costume archives, from their entry names (`names[archive][entry]`).
/// Names repeat (`leiCA` seven times); each is listed once, its first entry.
pub fn model_files(names: &[Vec<String>]) -> Vec<ModelFile> {
    let mut files: Vec<ModelFile> = Vec::new();
    for (archive, names) in names.iter().enumerate() {
        for (entry, name) in names.iter().enumerate() {
            let Some(stem) = name.strip_suffix(".tpr") else { continue };
            if names.get(entry + 1).is_some_and(|next| next.ends_with(".cat")) && !files.iter().any(|f| f.stem == stem) {
                files.push(ModelFile { stem: stem.to_string(), archive, entry, loose: None });
            }
        }
    }
    files
}

/// Whether a model is a costume (`kas00`) or a head with a hairstyle (`kasAA`), which the
/// costumes of some characters lack: Kasumi's `kas00` draws nothing on bone 1.
pub fn is_costume(stem: &str) -> bool {
    stem.len() == 5 && stem[3..].bytes().all(|b| b.is_ascii_digit())
}

/// The bone of the head in the draw lists.
pub const HEAD_BONE: u32 = 1;

use super::doax2;

/// One draw of a costume's list.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Draw {
    /// 0x90, 0x91, 0x93, 0xA0, 0xA1, 0xA2 or 0xD4. What tells them apart is not known; 0x90 is
    /// the most common and comes first, one object per bone. After 0xA0 and 0xA1 the number
    /// counts up from 0 over a run of objects (Jann Lee's 0xA1 0 to 7, then 0xA0 0), so there
    /// it may not be a bone.
    pub op: u8,
    pub bone: u8,
    pub object: usize,
    /// The argument of the last 0x47 before it, if any: Zack's `zac00` has four such groups,
    /// 0 and 1 with the same draws, 2 and 3 with the same.
    pub group: Option<u8>,
    /// The argument of the last 0x60 before it, while only 0x90 and 0xD4 draws follow: Jann
    /// Lee's `60 20`, two draws, `60 21`, the same two draws; Helena's `hel05` has `60 10` with
    /// one object and `60 11` with another on the same bone. They read as alternatives.
    pub condition: Option<u8>,
}

/// A costume's draw lists, most detailed first. Section kind 2 of the CAT's directory (as in
/// DOAX2: 8-byte entries from the offset in word 1) is a byte stream, not DOAX2's table:
///
/// - 0x90, 0x91, 0x93, 0xA0, 0xA1, 0xA2: bone, object (a byte each).
/// - 0xD4: bone, object and a third byte (0 in `hel05`, the only one).
/// - 0x47 n: starts group n. 0x07 ends the groups.
/// - 0x60 n: comes before one or more 0x90 draws.
/// - 0x04, then 0x02: the next list follows, with a third as many triangles each time.
/// - 0x00: the end.
///
/// Read from the files alone, with the names above guessed from their layout.
pub fn draw_lists(cat: &[u8]) -> Result<Vec<Vec<Draw>>> {
    if cat.get(..4) != Some(&b"CAT\0"[..]) {
        bail!("not a CAT");
    }
    let word = |at: usize| -> Result<usize> { Ok(u32::from_be_bytes(cat.get(at..at + 4).context("CAT too short")?.try_into()?) as usize) };
    let mut entry = word(4)?;
    let mut at = loop {
        let (tag, offset) = (word(entry)?, word(entry + 4)?);
        if tag == 0 {
            bail!("no draw list section");
        }
        if tag == 0x0100_0200 {
            break offset;
        }
        entry += 8;
    };
    let mut lists = vec![Vec::new()];
    let (mut group, mut condition) = (None, None);
    loop {
        let byte = |k: usize| cat.get(at + k).copied().context("draw list runs off the CAT");
        match byte(0)? {
            0 => break,
            op @ (0x90 | 0x91 | 0x93 | 0xA0 | 0xA1 | 0xA2 | 0xD4) => {
                if !matches!(op, 0x90 | 0xD4) {
                    condition = None;
                }
                lists.last_mut().unwrap().push(Draw { op, bone: byte(1)?, object: byte(2)? as usize, group, condition });
                at += if op == 0xD4 { 4 } else { 3 };
            }
            0x47 => {
                condition = None;
                group = Some(byte(1)?);
                at += 2;
            }
            // Helena's DOAX2 swimsuits, built by the port from `hel05`'s list, have `60 60 11`
            // where it has `60 10 90 14 01 60 11`: the first 0x60 is left with nothing.
            0x60 if byte(1)? == 0x60 => at += 1,
            0x60 => {
                condition = Some(byte(1)?);
                at += 2;
            }
            0x07 => {
                group = None;
                condition = None;
                at += 1;
            }
            0x04 | 0x02 => {
                group = None;
                condition = None;
                lists.push(Vec::new());
                at += 1;
            }
            other => bail!("draw list byte {other:#x} at {at:#x}"),
        }
    }
    Ok(lists)
}

/// Whether a draw is one of the 0xA0 to 0xA2 kind: Zack's `zac00` has a coil of rope and a
/// long line among them, which hang far below the floor on their nodes at rest, so something
/// else places them. `load` leaves them out.
pub fn is_loose(draw: &Draw) -> bool {
    matches!(draw.op, 0xA0..=0xA2)
}

/// Whether a costume has its own head: a 0x90 draw on the head's bone. Costumes without one
/// (Kasumi's) only have a 0x91 draw there, the neck.
pub fn has_head(cat: &[u8]) -> Result<bool> {
    Ok(draw_lists(cat)?.first().is_some_and(|list| list.iter().any(|d| d.op == 0x90 && d.bone as u32 == HEAD_BONE)))
}

/// A costume in its files' rest pose: the most detailed list, with the first of its groups
/// and the first of its conditions, without the loose draws.
pub fn load(tpr: &[u8], cat: &[u8]) -> Result<doax2::Loaded> {
    let lists = draw_lists(cat)?;
    let list = lists.first().context("no draw list")?;
    let first_group = list.iter().filter_map(|d| d.group).min();
    let first_condition = list.iter().filter_map(|d| d.condition).min();
    let placed = list
        .iter()
        .filter(|d| !is_loose(d) && (d.group.is_none() || d.group == first_group) && (d.condition.is_none() || d.condition == first_condition))
        .map(|d| doax2::Placed { slot: d.bone as u32, object: d.object, mark: None })
        .collect();
    let mut loaded = doax2::load_list(tpr, placed)?;
    join_mask_textures(&mut loaded);
    Ok(loaded)
}

/// Hair strands come as two textures on stages 0 and 1, both of kind 1: one pure white whose
/// alpha is the strands' shape, the other the hair's colour, in either order (Kokoro's `hakAA`:
/// textures 8 and 9 on one draw, 9 and 8 on another). DOAX2's reader draws stage 0 alone, which
/// left her hair white (the user's report). Here such a pair becomes one texture, the colour of
/// one with both alphas, sampled over the same coordinates: whether stage 1 has coordinates of
/// its own is not read.
fn join_mask_textures(loaded: &mut doax2::Loaded) {
    // A texture that is white all over.
    let white: Vec<bool> = loaded.textures.iter().map(|t| !t.rgba.is_empty() && t.rgba.chunks_exact(4).all(|p| p[0] >= 250 && p[1] >= 250 && p[2] >= 250)).collect();
    let mut joined: std::collections::HashMap<(usize, usize), usize> = std::collections::HashMap::new();
    for index in 0..loaded.pieces.len() {
        let object = loaded.pieces[index].2.object;
        let Some(draws) = loaded.objects.get(object).map(|o| o.draws.clone()) else { continue };
        if draws.len() != loaded.pieces[index].0.materials.len() {
            continue;
        }
        for (k, draw) in draws.iter().enumerate() {
            let stage = |n: u32| draw.stages.iter().find(|s| s.0 == n && s.1 == 1).map(|s| s.2).filter(|&t| t < white.len());
            let (Some(first), Some(second)) = (stage(0), stage(1)) else { continue };
            let (mask, colour) = match (white[first], white[second]) {
                (true, false) => (first, second),
                (false, true) => (second, first),
                _ => continue,
            };
            let texture = *joined.entry((mask, colour)).or_insert_with(|| {
                let (m, c) = (&loaded.textures[mask], &loaded.textures[colour]);
                let (width, height) = (m.width.max(c.width), m.height.max(c.height));
                let mut rgba = Vec::with_capacity((width * height * 4) as usize);
                for y in 0..height {
                    for x in 0..width {
                        let at = |t: &crate::formats::model::ModelTexture| ((y * t.height / height) * t.width + x * t.width / width) as usize * 4;
                        let (mp, cp) = (&m.rgba[at(m)..at(m) + 4], &c.rgba[at(c)..at(c) + 4]);
                        rgba.extend([cp[0], cp[1], cp[2], (mp[3] as u32 * cp[3] as u32 / 255) as u8]);
                    }
                }
                loaded.textures.push(crate::formats::model::ModelTexture { width, height, rgba });
                loaded.textures.len() - 1
            });
            let see_through = loaded.textures[texture].rgba.chunks_exact(4).any(|p| p[3] < 250);
            let material = &mut loaded.pieces[index].0.materials[k];
            // Listed twice, as `doax2::load_list` marks draws blended by their texture's alpha.
            material.textures = if see_through { vec![texture, texture] } else { vec![texture] };
            if let Some(look) = &mut material.look {
                look.alpha_from_texture = see_through;
            }
        }
    }
}

// ─── Motions ──────────────────────────────────────────────────────────────────

/// A character's motion file in `mot.afs`, by the three letters of her models. The files keep
/// the series' internal names; `m_<name>.mot` and `c_<name>.mot` come with each.
pub fn motion_file(code: &str) -> Option<&'static str> {
    Some(match code {
        "kas" => "kasumi",
        "aya" => "ayane",
        "ryu" => "ryu",
        "hyt" => "ninja",
        "ein" => "karate",
        "hel" => "hika",
        "hak" => "hakyo",
        "kor" => "koryu",
        "lei" => "leifan",
        "tin" => "tina",
        "lis" => "lisa",
        "cri" => "gokei",
        "boy" => "boy",
        "jan" => "janlee",
        "zac" => "zack",
        "bas" => "bass",
        "bay" => "sambo",
        "leo" => "bayman",
        "bra" => "suiken",
        "gen" => "genfu",
        "tng" => "tengu",
        "alp" => "alpha",
        "spa" => "spartan",
        _ => return None,
    })
}

pub const TRACK_COUNT: usize = super::doa3::TRACK_COUNT;

/// Offsets of a motion's 44 tracks in its file, as in DOA3.
pub type MotionTracks = [u32; TRACK_COUNT];

fn be16(data: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_be_bytes(data.get(at..at.checked_add(2)?)?.try_into().ok()?))
}

fn be32(data: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_be_bytes(data.get(at..at.checked_add(4)?)?.try_into().ok()?))
}

/// The sizes of DOA3's four kinds of key.
const KEY_SIZES: [usize; 4] = [10, 4, 6, 8];

/// The frames a track covers, or `None` if it does not end inside the file.
fn track_length(data: &[u8], start: u32) -> Option<u32> {
    let (mut at, mut frames) = (start as usize, 0u32);
    for _ in 0..4096 {
        let head = be16(data, at)?;
        if head & 0xFFF == 0 {
            be16(data, at + 2)?;
            return Some(frames);
        }
        frames += ((head >> 2) & 0x3FF) as u32;
        at += KEY_SIZES[(head & 3) as usize];
    }
    None
}

/// The motions of a `.mot` file as (number, track offsets): DOA3's layout with big-endian
/// words. The table's first entry is a placeholder block every unused number points at; a
/// block is 44 track offsets. A block is taken when every track ends inside the file.
pub fn list_motions(data: &[u8]) -> Vec<(usize, MotionTracks)> {
    let Some(placeholder) = be32(data, 0) else { return Vec::new() };
    let Some(first_track) = be32(data, placeholder as usize) else { return Vec::new() };
    let table = (first_track as usize / 4).min(data.len() / 4);
    (0..table)
        .filter_map(|id| {
            let block = be32(data, id * 4)? as usize;
            if block == placeholder as usize {
                return None;
            }
            let mut tracks = [0u32; TRACK_COUNT];
            for (k, track) in tracks.iter_mut().enumerate() {
                *track = be32(data, block + 4 * k)?;
                if (*track as usize) < table * 4 {
                    return None;
                }
                track_length(data, *track)?;
            }
            Some((id, tracks))
        })
        .collect()
}

pub fn motion_length(data: &[u8], tracks: &MotionTracks) -> u32 {
    tracks.iter().filter_map(|&t| track_length(data, t)).max().unwrap_or(0)
}

fn key_value(data: &[u8], at: usize) -> Option<f32> {
    let head = be16(data, at)? as i16 as i32;
    let low = be16(data, at + 2)? as i32;
    Some((((head & !0xFFF) << 4) | low) as f32 * 1e-4)
}

/// DOA3's track evaluation (`doa3::eval_track`) on big-endian keys. The two 24-bit slopes of a
/// kind 0 key are taken as its words swapped one by one, which puts their high bytes at +7 and
/// +6; not checked against the game.
fn eval_track(data: &[u8], start: u32, t: f32) -> Option<f32> {
    let mut at = start as usize;
    let mut segment = 0f32;
    loop {
        let head = be16(data, at)?;
        if head & 0xFFF == 0 {
            return key_value(data, at);
        }
        let dt = ((head >> 2) & 0x3FF) as f32;
        let kind = head & 3;
        let size = KEY_SIZES[kind as usize];
        if t < segment + dt {
            let (v0, v1) = (key_value(data, at)?, key_value(data, at + size)?);
            let x = t - segment;
            let slope = (v1 - v0) / dt;
            let hermite = |m0: f32, m1: f32| {
                let c2 = (3.0 * slope - 2.0 * m0 - m1) / dt;
                let c3 = (m0 + m1 - 2.0 * slope) / (dt * dt);
                ((c3 * x + c2) * x + m0) * x + v0
            };
            let wide = |low: usize, high: usize| -> Option<f32> { Some((((*data.get(high)? as i8 as i32) << 16) | be16(data, low)? as i32) as f32 * 1e-6) };
            return Some(match kind {
                0 => hermite(wide(at + 4, at + 7)?, wide(at + 8, at + 6)?),
                1 => slope * x + v0,
                2 => {
                    let m = be16(data, at + 4)? as i16 as f32 / 32768.0;
                    ((-m * x) + (slope + m * dt)) * x + v0
                }
                _ => hermite(be16(data, at + 4)? as i16 as f32 / 131072.0, be16(data, at + 6)? as i16 as f32 / 131072.0),
            });
        }
        segment += dt;
        at += size;
    }
}

pub fn eval_motion(data: &[u8], tracks: &MotionTracks, frame: f32) -> Result<[f32; TRACK_COUNT]> {
    let mut values = [0f32; TRACK_COUNT];
    for (value, &track) in values.iter_mut().zip(tracks) {
        *value = eval_track(data, track, frame).context("motion track out of bounds")?;
    }
    Ok(values)
}

/// Each bone's parent in DOA3's solve, whose bone numbers DOA4's draw lists share.
pub const BONE_PARENTS: [(usize, usize); 14] = [(0, 2), (1, 0), (8, 0), (4, 8), (5, 4), (14, 0), (10, 14), (11, 10), (6, 2), (7, 6), (3, 7), (12, 2), (13, 12), (9, 13)];

/// The draw lists' eye bones, and the nodes DOA3's solve gives the eyes.
pub const EYE_BONES: [u32; 2] = [17, 21];

/// DOA3's skeleton rows from where the bones are at rest: each bone's offset from its parent,
/// then the eyes' from the head.
pub fn skeleton(rest: &dyn Fn(u32) -> Option<glam::Vec3>) -> Result<[glam::Vec3; 17]> {
    let at = |bone: usize| rest(bone as u32).with_context(|| format!("no piece on bone {bone}"));
    let mut rows = [glam::Vec3::ZERO; 17];
    for (bone, parent) in BONE_PARENTS {
        rows[bone] = at(bone)? - at(parent)?;
    }
    for (row, bone) in [15, 16].into_iter().zip(EYE_BONES) {
        rows[row] = rest(bone).map_or(glam::Vec3::ZERO, |eye| eye - rest(HEAD_BONE).unwrap_or(eye));
    }
    Ok(rows)
}

/// The node of DOA3's solve a draw list bone follows, if it has one.
pub fn solve_node(bone: u32) -> Option<usize> {
    match bone {
        0..=14 => Some(bone as usize),
        17 => Some(super::doa3::NODE_EYES[0]),
        21 => Some(super::doa3::NODE_EYES[1]),
        _ => None,
    }
}

/// The frame DOA3's solve gives a bone when the character stands as in the files, arms out:
/// the pieces are modelled in that stance with every node unturned, while the solve's limb
/// frames run along the limb. Chest, head, pelvis, hands and feet are unturned in both. For a
/// limb the solve's z runs from the joint to its target and a bend moves the middle joint
/// along x (or against it, on the two limbs whose angles it negates); here the elbows are taken
/// to go back and the knees forward, which sets the roll. The forearm and shin are half a turn
/// about y from there. Chosen by how a limb bends, not read from the game.
pub fn rest_frame(bone: usize) -> glam::Mat3 {
    use glam::{Mat3, Vec3};
    let limb = |x: Vec3, z: Vec3| Mat3::from_cols(x, z.cross(x), z);
    let turned = |m: Mat3| m * Mat3::from_rotation_y(std::f32::consts::PI);
    match bone {
        8 => limb(Vec3::NEG_Z, Vec3::X),
        4 => turned(limb(Vec3::NEG_Z, Vec3::X)),
        14 => limb(Vec3::Z, Vec3::NEG_X),
        10 => turned(limb(Vec3::Z, Vec3::NEG_X)),
        12 => limb(Vec3::NEG_Z, Vec3::NEG_Y),
        13 => turned(limb(Vec3::NEG_Z, Vec3::NEG_Y)),
        6 => limb(Vec3::Z, Vec3::NEG_Y),
        7 => turned(limb(Vec3::Z, Vec3::NEG_Y)),
        _ => Mat3::IDENTITY,
    }
}

/// For each blend weight of a piece drawn with 0x91 on `bone`: the solve's node, the bone
/// whose rest place that node has, and the node's rest frame. The vertices carry two to four
/// weights, the first for the bone itself and the second for its parent (in `kas00` the
/// vertices heavy on the first sit down the limb from the joint, those heavy on the second
/// towards the body).
///
/// - A third weight on the elbow pieces is taken to be the forearm's twist node, and on the
///   knee pieces (Zack's and Kokoro's trousers) the shin again, DOA3's solve having no twist
///   there. No vertex is heavy on either; both are guesses.
/// - Bones 32 and 33 are pieces along the forearms: their first weight is heavy near the elbow
///   and the second near the wrist, so they are taken as the forearm and its twist node.
/// - The chest's piece has four weights (half floats). In `kas00` the first two are heavy on
///   its front and its back, in `zac00` on its top and bottom, and the other two are light on
///   both sides: not understood, so it stays rigid on the chest.
pub fn weight_nodes(bone: usize, count: usize) -> Option<Vec<(usize, usize, glam::Mat3)>> {
    let twist = |mid: usize| {
        // The solve turns the forearm about its z until the hand's z lies along x; at rest
        // the hand is unturned.
        let (node, sign) = if mid == 4 { (super::doa3::NODE_TWIST[0], 1.0) } else { (super::doa3::NODE_TWIST[1], -1.0) };
        let hand_z = rest_frame(mid).transpose() * glam::Vec3::Z;
        (node, mid, rest_frame(mid) * glam::Mat3::from_rotation_z((sign * hand_z.y).atan2(sign * hand_z.x)))
    };
    let on = |bone: usize| (bone, bone, rest_frame(bone));
    let nodes = match (bone, count) {
        (32, 2) => vec![on(4), twist(4)],
        (33, 2) => vec![on(10), twist(10)],
        (0, _) => return None,
        _ => {
            let parent = BONE_PARENTS.iter().find(|p| p.0 == bone)?.1;
            let mut nodes = vec![on(bone), on(parent)];
            match (bone, count) {
                (4 | 10, 3) => nodes.push(twist(bone)),
                (7 | 13, 3) => nodes.push(on(bone)),
                _ => {}
            }
            nodes
        }
    };
    (nodes.len() == count).then_some(nodes)
}

// ─── Soft bodies ──────────────────────────────────────────────────────────────

/// The CAT's soft body records: word 2 is the offset of a list of (kind in the top byte,
/// offset from the list) ended by kind 0xFF. The kinds are DOA3's and DOA2U's: 1 chains (hair,
/// ribbons), 2 cloth, 5 springs, 250 vertex copies and so on.
pub fn soft_records(cat: &[u8]) -> Option<(usize, Vec<(u8, usize)>)> {
    let base = be32(cat, 8)? as usize;
    if base == 0 {
        return Some((0, Vec::new()));
    }
    let mut records = Vec::new();
    for k in 0..4096 {
        let kind = *cat.get(base + 8 * k)?;
        if kind == 0xFF {
            return Some((base, records));
        }
        records.push((kind, be32(cat, base + 8 * k + 4)? as usize));
    }
    None
}

/// The chains of a model (kind 1), with each target's vertices numbered as its piece's: the
/// file's words hold a vertex layout in the top byte and a byte offset in that layout below.
/// Read by DOA3's parser, whose record this is field for field (on `kasAA`: 8 or 6 particles
/// on bone 1, segment 0.017, gravity 0.007, pull 0.3, damping 0.8).
pub fn chains(cat: &[u8], objects: &[doax2::Object]) -> Vec<super::doa3_chains::SoftChain> {
    let Some((base, records)) = soft_records(cat) else { return Vec::new() };
    let offsets: Vec<usize> = records.iter().filter(|r| r.0 == 1).map(|r| r.1).collect();
    let mut chains = super::doa3_chains::parse_chain_records_big_endian(cat, base, &offsets);
    for chain in &mut chains {
        for target in &mut chain.targets {
            let object = target.object;
            for point in target.segments.iter_mut().flatten() {
                for vertex in &mut point.vertices {
                    *vertex = piece_vertex(objects, object, *vertex);
                }
            }
        }
    }
    chains
}

/// A soft body's vertex word as its piece's vertex: the layout in the top byte, a byte offset
/// in that layout below. `usize::MAX` when the object has no such layout.
fn piece_vertex(objects: &[doax2::Object], object: usize, word: usize) -> usize {
    let (layout, offset) = (word >> 24, word & 0xFF_FFFF);
    let Some(object) = objects.get(object) else { return usize::MAX };
    let first: usize = object.decls.iter().take(layout).map(|d| d.vertex_count).sum();
    object.decls.get(layout).map_or(usize::MAX, |d| first + offset / d.streams.first().map_or(32, |s| s.1.max(1)))
}

/// The cloth sheets of a model (kind 2): skirts, flaps, sleeves and trouser legs. See
/// `doa3_cloth::parse_cloth_records_doa4` for how the record differs from DOA2U's.
pub fn cloth(cat: &[u8], objects: &[doax2::Object]) -> Vec<super::doa3_cloth::SoftCloth> {
    let Some((base, records)) = soft_records(cat) else { return Vec::new() };
    let offsets: Vec<usize> = records.iter().filter(|r| r.0 == 2).map(|r| r.1).collect();
    let mut sheets = super::doa3_cloth::parse_cloth_records_doa4(cat, base, &offsets);
    for sheet in &mut sheets {
        let object = sheet.object;
        sheet.map_vertices(|word| piece_vertex(objects, object, word));
    }
    sheets
}

/// Kind 250: after the soft bodies run, positions of one object are copied to another, as in
/// DOA2U (`doa2u::VertexCopy`): +0 a condition, +2 the source object, +4 the target, +8 the
/// offset of a list of a source vertex, then target vertices whose low bit says another
/// follows, until -1. Vertices come out numbered as their pieces'.
pub fn vertex_copies(cat: &[u8], objects: &[doax2::Object]) -> Vec<super::doa2u::VertexCopy> {
    let Some((base, records)) = soft_records(cat) else { return Vec::new() };
    let mut copies = Vec::new();
    for &(_, offset) in records.iter().filter(|r| r.0 == 250) {
        let read = || -> Option<super::doa2u::VertexCopy> {
            let at = base + offset;
            let condition = be16(cat, at)?;
            // DOA2U skips 0 and odd values other than 0xFFFF.
            if condition == 0 || (condition != 0xFFFF && condition & 1 != 0) {
                return None;
            }
            let (from, to) = (be16(cat, at + 2)? as usize, be16(cat, at + 4)? as usize);
            if from >= objects.len() || to >= objects.len() {
                return None;
            }
            let mut at = base + be32(cat, at + 8)? as usize;
            let mut links = Vec::new();
            loop {
                let source = be32(cat, at)?;
                if source == u32::MAX {
                    break;
                }
                at += 4;
                loop {
                    let target = be32(cat, at)?;
                    if target == u32::MAX {
                        break;
                    }
                    at += 4;
                    links.push((piece_vertex(objects, from, source as usize), piece_vertex(objects, to, (target & !1) as usize)));
                    if target & 1 == 0 {
                        break;
                    }
                }
            }
            Some(super::doa2u::VertexCopy { from, to, links })
        };
        copies.extend(read());
    }
    copies
}

/// The shape sets of a model: an `MPV` block at the offset in CAT word 3, DOAX2's (see
/// `doax2::hand_poses`). A costume has two sets of 21 entries on its hands and, on some, one of
/// 3 on the chest; a head has one of 5 and one of 9 on the face, which in DOAX2 are the eyes
/// and the mouth with 4 and 8.
pub fn shape_sets(cat: &[u8], objects: &[doax2::Object]) -> Result<Vec<doax2::HandPoses>> {
    let start = be32(cat, 12).context("CAT too short")? as usize;
    if start == 0 {
        return Ok(Vec::new());
    }
    doax2::hand_poses(cat.get(start..).context("shape sets beyond the CAT")?, objects)
}

/// A character's script file in `anm.afs`: `motion_file`'s name with `.bin`, but for Lei Fang,
/// whose motions are `leifan.mot` and scripts `leifang.bin`.
pub fn script_file(code: &str) -> Option<String> {
    let name = motion_file(code)?;
    Some(format!("{}.bin", if name == "leifan" { "leifang" } else { name }))
}

/// A hand's shape set as poses to blend between: pose p of the scripts is taken to be the
/// set's entry p + 1, entry 1 being the hand as modelled (as assumed for DOAX2; not checked).
/// Entries a set lacks stay on the base.
pub fn pose_hand(set: &doax2::HandPoses) -> Option<super::doa3_morphs::PoseHand> {
    let base = set.poses.get(1)?.clone()?;
    let shapes = set.poses[1..].iter().map(|pose| pose.clone().unwrap_or_else(|| base.clone())).collect();
    Some(super::doa3_morphs::PoseHand::new(set.object, set.groups.clone(), shapes, Vec::new(), Vec::new()))
}

/// The springs of a model (kind 5): points on a bone, each on a spring to its place. DOA3's
/// record with big-endian words, 0x34 bytes.
pub fn springs(cat: &[u8], objects: &[doax2::Object]) -> Vec<super::doa3_springs::SoftSprings> {
    let Some((base, records)) = soft_records(cat) else { return Vec::new() };
    let offsets: Vec<usize> = records.iter().filter(|r| r.0 == 5).map(|r| r.1).collect();
    let mut springs = super::doa3_springs::parse_spring_records_big_endian(cat, base, &offsets);
    for spring in &mut springs {
        let object = spring.object;
        spring.map_vertices(|word| piece_vertex(objects, object, word));
    }
    springs
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::formats::doax::LazyArchive;
    /// Prints a costume's objects: layouts, draws and the buffers' sizes. `DOA4_COSTUME` names
    /// it as archive and entry (default `chrone.afs zac00`).
    #[test]
    #[ignore]
    fn costume_objects() {
        let which = std::env::var("DOA4_COSTUME").unwrap_or("chrone.afs zac00".to_string());
        let (archive, stem) = which.split_once(' ').unwrap();
        let afs = LazyArchive::open(game_dir().join(archive)).unwrap();
        let tpr = afs.read(&format!("{stem}.tpr")).unwrap();
        let xpr = doax2::Xpr2::parse(&tpr).unwrap();
        for kind in [&b"VBUF"[..], b"IBUF"] {
            let sizes: Vec<(usize, usize)> = xpr.of_kind(kind.try_into().unwrap()).map(|b| xpr.buffer(b, kind == b"VBUF").unwrap()).collect();
            println!("{}: {sizes:x?}", String::from_utf8_lossy(kind));
        }
        for (o, object) in doax2::objects(&xpr).unwrap().iter().enumerate() {
            println!("object {o}");
            for d in &object.decls {
                println!("  layout: index buffer {} with {} indices, {} vertices, streams {:?}, elements {:?}", d.index_buffer, d.index_count, d.vertex_count, d.streams, d.elements);
            }
            for d in &object.draws {
                println!("  draw: layout {}, indices {} + {}, stages {:x?}", d.decl, d.first_index, d.index_count, d.stages);
            }
        }
    }

    /// Prints a costume's node tree with each node's place at rest and the bone its object is
    /// drawn with. `DOA4_COSTUME` as in `costume_objects`.
    #[test]
    #[ignore]
    fn costume_nodes() {
        let which = std::env::var("DOA4_COSTUME").unwrap_or("chrone.afs zac00".to_string());
        let (archive, stem) = which.split_once(' ').unwrap();
        let afs = LazyArchive::open(game_dir().join(archive)).unwrap();
        let (tpr, cat) = (afs.read(&format!("{stem}.tpr")).unwrap(), afs.read(&format!("{stem}.cat")).unwrap());
        let xpr = doax2::Xpr2::parse(&tpr).unwrap();
        let nodes = doax2::hierarchy(&xpr).unwrap();
        let rest = doax2::rest_pose(&nodes);
        let lists = draw_lists(&cat).unwrap();
        for (k, node) in nodes.iter().enumerate() {
            let draws: Vec<String> = lists[0].iter().filter(|d| d.object == node.object).map(|d| format!("{:x} bone {}", d.op, d.bone)).collect();
            let p = rest[k].w_axis;
            println!("node {k}: parent {:?}, object {}, at ({:.3}, {:.3}, {:.3}), local ({:.3}, {:.3}, {:.3}) {draws:?}", node.parent, node.object, p.x, p.y, p.z, node.local.w_axis.x, node.local.w_axis.y, node.local.w_axis.z);
        }
    }

    /// Every character's motion files list motions whose tracks end inside the file, and every
    /// motion evaluates at its first, middle and last frame. Prints the counts.
    #[test]
    #[ignore]
    fn motions_read() {
        let afs = LazyArchive::open(game_dir().join("mot.afs")).unwrap();
        for (code, _) in ROSTER {
            let Some(name) = motion_file(code) else { continue };
            for prefix in ["", "m_", "c_"] {
                let file = format!("{prefix}{name}.mot");
                let data = afs.read(&file).unwrap_or_else(|e| panic!("{file}: {e:#}"));
                let motions = list_motions(&data);
                let mut longest = 0;
                for (id, tracks) in &motions {
                    let length = motion_length(&data, tracks);
                    longest = longest.max(length);
                    for frame in [0.0, length as f32 / 2.0, length as f32] {
                        let values = eval_motion(&data, tracks, frame).unwrap_or_else(|e| panic!("{file} {id}: {e:#}"));
                        assert!(values.iter().all(|v| v.is_finite()), "{file} {id}");
                    }
                }
                println!("{file}: {} motions, longest {longest} frames", motions.len());
                assert!(prefix != "" || motions.len() > 100, "{file}");
            }
        }
    }

    /// Every DOAX2 swimsuit the port built loads, and its head is named by the manifest or its
    /// own.
    #[test]
    #[ignore]
    fn swimsuits_load() {
        let suits = suits(&suits_dir());
        assert!(!suits.is_empty(), "no manifest in {}", suits_dir().display());
        for suit in &suits {
            let (tpr, cat) = (std::fs::read(&suit.tpr).unwrap(), std::fs::read(&suit.cat).unwrap());
            let loaded = load(&tpr, &cat).unwrap_or_else(|e| panic!("{}: {e:#}", suit.stem));
            let own_head = has_head(&cat).unwrap();
            assert!(own_head || suit.head_entry.is_some(), "{} has no head", suit.stem);
            println!("{} for {}: {} pieces, {} vertices, own head {own_head}, lists {:?}", suit.stem, suit.code, loaded.pieces.len(), loaded.pieces.iter().map(|p| p.0.vertices.len()).sum::<usize>(), draw_lists(&cat).unwrap().iter().map(Vec::len).collect::<Vec<_>>());
        }
        println!("{} swimsuits", suits.len());
    }

    /// Writes a model's textures as `target/captures/doa4_<stem>_<n>.png` and prints their
    /// sizes and how see-through they are. `DOA4_COSTUME` as in `costume_objects`.
    #[test]
    #[ignore]
    fn costume_textures() {
        let which = std::env::var("DOA4_COSTUME").unwrap_or("chrthree.afs hakAA".to_string());
        let (archive, stem) = which.split_once(' ').unwrap();
        let afs = LazyArchive::open(game_dir().join(archive)).unwrap();
        let tpr = afs.read(&format!("{stem}.tpr")).unwrap();
        let xpr = doax2::Xpr2::parse(&tpr).unwrap();
        std::fs::create_dir_all("target/captures").unwrap();
        for (k, texture) in doax2::textures(&xpr).iter().enumerate() {
            let pixels = texture.rgba.chunks_exact(4);
            let count = pixels.len().max(1) as f32;
            let mean = pixels.clone().fold([0f32; 4], |mut sum, p| {
                for c in 0..4 {
                    sum[c] += p[c] as f32 / count;
                }
                sum
            });
            println!("texture {k}: {}x{}, mean colour {:.0?}", texture.width, texture.height, mean);
            let file = std::fs::File::create(format!("target/captures/doa4_{stem}_{k}.png")).unwrap();
            let mut encoder = png::Encoder::new(file, texture.width, texture.height);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            encoder.write_header().unwrap().write_image_data(&texture.rgba).unwrap();
        }
    }

    /// Prints, per piece of a costume's first list, its draw, vertex count, the extra vertex
    /// elements beyond position, normal and texture coordinates, and its blend weights if any.
    #[test]
    #[ignore]
    fn costume_weights() {
        let which = std::env::var("DOA4_COSTUME").unwrap_or("chrone.afs kas00".to_string());
        let (archive, stem) = which.split_once(' ').unwrap();
        let afs = LazyArchive::open(game_dir().join(archive)).unwrap();
        let (tpr, cat) = (afs.read(&format!("{stem}.tpr")).unwrap(), afs.read(&format!("{stem}.cat")).unwrap());
        let loaded = load(&tpr, &cat).unwrap();
        let lists = draw_lists(&cat).unwrap();
        for ((piece, node, placed), weights) in loaded.pieces.iter().zip(&loaded.weights) {
            let op = lists[0].iter().find(|d| d.object == placed.object).map_or(0, |d| d.op);
            let usages: Vec<(u8, u8, u32)> = loaded.objects[placed.object].decls.iter().flat_map(|d| d.elements.iter().filter(|e| !matches!(e.usage, 0 | 3 | 5)).map(|e| (e.usage, e.usage_index, e.kind))).collect();
            let groups = weights.first().map_or(0, Vec::len);
            let mut distinct: Vec<String> = weights.iter().map(|w| format!("{w:.2?}")).collect();
            distinct.sort();
            distinct.dedup();
            println!("{op:x} bone {} object {} node {node}: {} vertices, other elements {usages:x?}, {groups} weights, {} distinct: {:?}", placed.slot, placed.object, piece.vertices.len(), distinct.len(), &distinct[..distinct.len().min(6)]);
            // Where the vertices mostly on each weight sit, from the piece's node.
            for k in 0..groups {
                let heavy: Vec<glam::Vec3> = piece.vertices.iter().zip(weights).filter(|(_, w)| w[k] > 0.8).map(|(v, _)| glam::Vec3::from(v.position)).collect();
                let centre = heavy.iter().copied().sum::<glam::Vec3>() / heavy.len().max(1) as f32;
                println!("    weight {k}: {} vertices over 0.8, centred at ({:.3}, {:.3}, {:.3})", heavy.len(), centre.x, centre.y, centre.z);
                // And where the weight's own mass sits, split by side for weights that are never heavy.
                for (side, sign) in [("+x", 1.0f32), ("-x", -1.0)] {
                    let part = piece.vertices.iter().zip(weights).filter(|(v, _)| v.position[0] * sign > 0.0);
                    let total: f32 = part.clone().map(|(_, w)| w[k]).sum();
                    let mass = part.map(|(v, w)| glam::Vec3::from(v.position) * w[k]).sum::<glam::Vec3>() / total.max(1e-6);
                    println!("      {side}: total {total:.1}, at ({:.3}, {:.3}, {:.3})", mass.x, mass.y, mass.z);
                }
            }
        }
    }

    /// Counts each model's soft body records by kind and how many of its chains DOA3's parser
    /// reads, with the vertices they move.
    #[test]
    #[ignore]
    fn chains_read() {
        let (mut records, mut read, mut vertices, mut bad) = (0, 0, 0, 0);
        let mut spring_totals = (0, 0, 0, 0);
        let mut cloth_totals = (0, 0, 0, 0);
        let mut copy_totals = (0, 0, 0, 0);
        for archive in COSTUME_ARCHIVES {
            let afs = LazyArchive::open(game_dir().join(archive)).unwrap();
            let names: Vec<String> = afs.names().map(str::to_string).collect();
            for (index, name) in names.iter().enumerate() {
                let Some(stem) = name.strip_suffix(".tpr") else { continue };
                let (tpr, cat) = (afs.read_index(index).unwrap(), afs.read_index(index + 1).unwrap());
                let xpr = doax2::Xpr2::parse(&tpr).unwrap();
                let objects = doax2::objects(&xpr).unwrap();
                let Some((_, list)) = soft_records(&cat) else { panic!("{stem}: soft list does not end") };
                let wanted = list.iter().filter(|r| r.0 == 1).count();
                let chains = chains(&cat, &objects);
                let moved: Vec<usize> = chains.iter().flat_map(|c| c.targets.iter()).flat_map(|t| t.segments.iter().flatten().flat_map(|p| p.vertices.iter().copied())).collect();
                let outside = chains
                    .iter()
                    .flat_map(|c| c.targets.iter())
                    .map(|t| {
                        let count = objects.get(t.object).map_or(0, doax2::Object::vertex_count);
                        t.segments.iter().flatten().flat_map(|p| p.vertices.iter()).filter(|&&v| v >= count).count()
                    })
                    .sum::<usize>();
                let wanted_springs = list.iter().filter(|r| r.0 == 5).count();
                let springs = springs(&cat, &objects);
                let spring_outside = springs.iter().map(|s| s.vertices().filter(|&v| v >= objects.get(s.object).map_or(0, doax2::Object::vertex_count)).count()).sum::<usize>();
                spring_totals.0 += wanted_springs;
                spring_totals.1 += springs.len();
                spring_totals.2 += springs.iter().map(|s| s.vertices().count()).sum::<usize>();
                spring_totals.3 += spring_outside;
                if springs.len() < wanted_springs || spring_outside > 0 {
                    println!("{stem}: {} of {wanted_springs} springs read, {spring_outside} vertices outside their object", springs.len());
                }
                let wanted_cloth = list.iter().filter(|r| r.0 == 2).count();
                let sheets = cloth(&cat, &objects);
                let cloth_outside = sheets.iter().map(|s| s.written_vertices().iter().filter(|&&v| v >= objects.get(s.object).map_or(0, doax2::Object::vertex_count)).count()).sum::<usize>();
                cloth_totals.0 += wanted_cloth;
                cloth_totals.1 += sheets.len();
                cloth_totals.2 += sheets.iter().map(|s| s.written_vertices().len()).sum::<usize>();
                cloth_totals.3 += cloth_outside;
                if sheets.len() < wanted_cloth || cloth_outside > 0 {
                    println!("{stem}: {} of {wanted_cloth} sheets read, {cloth_outside} vertices outside their object", sheets.len());
                }
                let copies = vertex_copies(&cat, &objects);
                let first_list: std::collections::HashSet<usize> = draw_lists(&cat).unwrap().first().map(|l| l.iter().map(|d| d.object).collect()).unwrap_or_default();
                copy_totals.0 += list.iter().filter(|r| r.0 == 250).count();
                copy_totals.1 += copies.len();
                copy_totals.2 += copies.iter().filter(|c| first_list.contains(&c.to)).count();
                copy_totals.3 += copies.iter().flat_map(|c| c.links.iter().map(move |l| (c, l))).filter(|(c, l)| l.0 >= objects[c.from].vertex_count() || l.1 >= objects[c.to].vertex_count()).count();
                records += wanted;
                read += chains.len();
                vertices += moved.len();
                bad += outside;
                if wanted > 0 && (chains.len() < wanted || outside > 0) {
                    println!("{stem}: {} of {wanted} chains read, {} vertices, {outside} outside their object", chains.len(), moved.len());
                }
            }
        }
        println!("{read} of {records} chains read, moving {vertices} vertices, {bad} outside their object");
        println!("{} of {} springs read, moving {} vertices, {} outside their object", spring_totals.1, spring_totals.0, spring_totals.2, spring_totals.3);
        println!("{} of {} copies read, {} onto an object of the first list, {} links outside their objects", copy_totals.1, copy_totals.0, copy_totals.2, copy_totals.3);
        println!("{} of {} sheets read, moving {} vertices, {} outside their object", cloth_totals.1, cloth_totals.0, cloth_totals.2, cloth_totals.3);
    }

    /// Every model's shape sets read; prints how many sets there are of each size.
    #[test]
    #[ignore]
    fn shape_sets_read() {
        let mut sizes: std::collections::BTreeMap<(bool, usize), usize> = std::collections::BTreeMap::new();
        let mut far = 0f32;
        let mut worst_by_size: std::collections::BTreeMap<(bool, usize), (f32, String)> = std::collections::BTreeMap::new();
        for archive in COSTUME_ARCHIVES {
            let afs = LazyArchive::open(game_dir().join(archive)).unwrap();
            let names: Vec<String> = afs.names().map(str::to_string).collect();
            for (index, name) in names.iter().enumerate() {
                let Some(stem) = name.strip_suffix(".tpr") else { continue };
                let (tpr, cat) = (afs.read_index(index).unwrap(), afs.read_index(index + 1).unwrap());
                let xpr = doax2::Xpr2::parse(&tpr).unwrap();
                let objects = doax2::objects(&xpr).unwrap();
                let sets = shape_sets(&cat, &objects).unwrap_or_else(|e| panic!("{stem}: {e:#}"));
                for set in &sets {
                    *sizes.entry((is_costume(stem), set.poses.len())).or_default() += 1;
                    let before = far;
                    far = 0.0;
                    // The base shape (entry 1) should be the object's own vertices.
                    let piece = doax2::object_piece(&xpr, &objects[set.object]).unwrap();
                    if let Some(Some(base)) = set.poses.get(1) {
                        for (group, position) in set.groups.iter().zip(base) {
                            for &v in group {
                                far = far.max((glam::Vec3::from(piece.vertices[v].position) - glam::Vec3::from(*position)).length());
                            }
                        }
                    }
                    let worst = worst_by_size.entry((is_costume(stem), set.poses.len())).or_insert((0f32, String::new()));
                    if far > worst.0 {
                        *worst = (far, format!("{stem} object {}", set.object));
                    }
                    far = far.max(before);
                }
            }
        }
        println!("base shape's distance from the vertices, worst by (costume, entries): {worst_by_size:.4?}");
        println!("sets by (costume, entries): {sizes:?}; base shapes at most {far:.5} m from their vertices");
    }

    /// How far DOAX2's readers get on every DOA4 costume: one line per costume with the first
    /// step that fails, or its counts.
    #[test]
    #[ignore]
    fn doax2_readers_on_costumes() {
        let mut failed: std::collections::BTreeMap<String, usize> = std::collections::BTreeMap::new();
        let mut read = 0;
        for archive in COSTUME_ARCHIVES {
            let afs = LazyArchive::open(game_dir().join(archive)).unwrap();
            let names: Vec<String> = afs.names().map(str::to_string).collect();
            for (index, name) in names.iter().enumerate() {
                let Some(stem) = name.strip_suffix(".tpr") else { continue };
                let (tpr, cat) = (afs.read_index(index).unwrap(), afs.read_index(index + 1).unwrap());
                let step = (|| -> std::result::Result<String, String> {
                    let lists = draw_lists(&cat).map_err(|e| format!("draw lists: {e:#}"))?;
                    let loaded = load(&tpr, &cat).map_err(|e| format!("load: {e:#}"))?;
                    let ops: std::collections::BTreeSet<u8> = lists.iter().flatten().map(|d| d.op).collect();
                    let bounds = loaded.pieces.iter().flat_map(|(piece, node, _)| piece.vertices.iter().map(|v| loaded.rest[*node].transform_point3(v.position.into()))).fold(
                        (glam::Vec3::splat(f32::MAX), glam::Vec3::splat(f32::MIN)),
                        |(low, high), p| (low.min(p), high.max(p)),
                    );
                    Ok(format!(
                        "lists of {:?} draws, ops {ops:x?}, {} nodes, {} pieces, {} vertices, {} textures, from {:.2?} to {:.2?}",
                        lists.iter().map(Vec::len).collect::<Vec<_>>(),
                        loaded.nodes.len(),
                        loaded.pieces.len(),
                        loaded.pieces.iter().map(|p| p.0.vertices.len()).sum::<usize>(),
                        loaded.textures.len(),
                        bounds.0.to_array(),
                        bounds.1.to_array()
                    ))
                })();
                match step {
                    Ok(line) => {
                        read += 1;
                        println!("{stem}: {line}");
                    }
                    Err(e) => {
                        println!("{stem}: FAILED {e}");
                        *failed.entry(e.split(':').next().unwrap_or("").to_string()).or_default() += 1;
                    }
                }
            }
        }
        println!("{read} read, failures by step: {failed:?}");
    }
}
