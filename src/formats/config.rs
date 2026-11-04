//! Per-character viewer settings, read from `characters.toml`.

use serde::Deserialize;
use std::collections::HashMap;

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CharacterConfig {
    /// Costume shown when the character is selected.
    pub costume: Option<usize>,
    /// Start of a motion's name in the list, for example "A 0040".
    pub motion: Option<String>,
    /// Turns the chest spring off. It only ever runs for the characters listed
    /// in `doa3::has_chest_spring`, so `true` has no effect on anyone else.
    pub bouncing: Option<bool>,
}

#[derive(Debug, Clone, Default)]
pub struct CharacterConfigs(HashMap<String, CharacterConfig>);

impl CharacterConfigs {
    pub fn parse(text: &str) -> Result<Self, toml::de::Error> {
        toml::from_str(text).map(Self)
    }

    pub fn get(&self, name: &str) -> CharacterConfig {
        self.0.get(name).cloned().unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_settings_and_defaults_missing_characters() {
        let configs = CharacterConfigs::parse(
            "[Tina]\ncostume = 1\nmotion = \"A 0040\"\nbouncing = false\n",
        )
        .unwrap();
        let tina = configs.get("Tina");
        assert_eq!(tina.costume, Some(1));
        assert_eq!(tina.motion.as_deref(), Some("A 0040"));
        assert_eq!(tina.bouncing, Some(false));
        assert_eq!(configs.get("Zack").costume, None);
    }

    #[test]
    fn rejects_unknown_settings() {
        assert!(CharacterConfigs::parse("[Tina]\ncolour = 1\n").is_err());
    }
}
