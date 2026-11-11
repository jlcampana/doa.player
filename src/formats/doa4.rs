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

/// A costume in its files' rest pose: the most detailed list, with the first of its groups
/// and the first of its conditions.
pub fn load(tpr: &[u8], cat: &[u8]) -> Result<doax2::Loaded> {
    let lists = draw_lists(cat)?;
    let list = lists.first().context("no draw list")?;
    let first_group = list.iter().filter_map(|d| d.group).min();
    let first_condition = list.iter().filter_map(|d| d.condition).min();
    let placed = list
        .iter()
        .filter(|d| (d.group.is_none() || d.group == first_group) && (d.condition.is_none() || d.condition == first_condition))
        .map(|d| doax2::Placed { slot: d.bone as u32, object: d.object, mark: None })
        .collect();
    doax2::load_list(tpr, placed)
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
