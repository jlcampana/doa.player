//! Voice clips from `voice.afs`, played by the system's audio tools.

use crate::formats::{adx, AfsArchive};
use anyhow::{bail, Context, Result};
use std::process::{Child, Command, Stdio};

/// Clip names start with a character's prefix, by DOA2U's file names. `ln` as Leon and `by` as
/// Bayman are inferred. No prefix for Hitomi was found, and the file has no clips with hers.
const VOICE_PREFIXES: [(&str, &str); 14] = [
    ("Kasumi", "ks_"),
    ("Ayane", "ay_"),
    ("Zack", "zk_"),
    ("Tina Armstrong", "tn_"),
    ("Ryu Hayabusa", "ry_"),
    ("Lei Fang", "le_"),
    ("Jann Lee", "jn_"),
    ("Bass Armstrong", "ba_"),
    ("Bayman", "by_"),
    ("Gen Fu", "gn_"),
    ("Leon", "ln_"),
    ("Tengu", "tg_"),
    ("Ein", "ei_"),
    ("Helena Douglas", "he_"),
];

/// A clip and its name, without the file extension.
pub struct Clip {
    pub entry: usize,
    pub name: String,
}

/// Plays one clip at a time.
#[derive(Default)]
pub struct VoicePlayer {
    archive: Option<AfsArchive>,
    tried: bool,
    playing: Option<Child>,
}

impl VoicePlayer {
    fn archive(&mut self, path: &str) -> Option<&AfsArchive> {
        if !self.tried {
            self.tried = true;
            match AfsArchive::from_file(path) {
                Ok(archive) => self.archive = Some(archive),
                Err(e) => log::warn!("Could not load {path}: {e}"),
            }
        }
        self.archive.as_ref()
    }

    /// The clips of a character, or everything else for no character. Placeholders are out.
    pub fn clips(&mut self, path: &str, character: Option<&str>) -> Vec<Clip> {
        let Some(archive) = self.archive(path) else { return Vec::new() };
        let prefix = character.and_then(|name| VOICE_PREFIXES.iter().find(|(n, _)| *n == name).map(|(_, p)| *p));
        let mut clips: Vec<Clip> = archive
            .entries()
            .iter()
            .enumerate()
            .filter_map(|(entry, file)| {
                let name = file.name.strip_suffix(".adx")?;
                let ours = match (character, prefix) {
                    (Some(_), Some(prefix)) => name.starts_with(prefix),
                    (Some(_), None) => false,
                    (None, _) => !VOICE_PREFIXES.iter().any(|(_, p)| name.starts_with(p)),
                };
                (ours && name != "dummy").then(|| Clip { entry, name: name.to_string() })
            })
            .collect();
        clips.sort_by(|a, b| a.name.cmp(&b.name));
        clips
    }

    pub fn play(&mut self, path: &str, entry: usize) -> Result<()> {
        let archive = self.archive(path).context("voice.afs not loaded")?;
        let audio = adx::decode(archive.get_by_index(entry).context("no such clip")?)?;
        let file = std::env::temp_dir().join(format!("doa_player_voice_{}.wav", std::process::id()));
        std::fs::write(&file, adx::to_wav(&audio))?;
        self.stop();
        for tool in ["pw-play", "paplay", "aplay"] {
            let started = Command::new(tool).arg(&file).stdout(Stdio::null()).stderr(Stdio::null()).spawn();
            if let Ok(child) = started {
                self.playing = Some(child);
                return Ok(());
            }
        }
        bail!("no audio tool found: pw-play, paplay or aplay")
    }

    pub fn stop(&mut self) {
        if let Some(mut child) = self.playing.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

impl Drop for VoicePlayer {
    fn drop(&mut self) {
        self.stop();
        let _ = std::fs::remove_file(std::env::temp_dir().join(format!("doa_player_voice_{}.wav", std::process::id())));
    }
}
