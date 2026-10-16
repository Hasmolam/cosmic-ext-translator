use std::collections::HashMap;
use cosmic::cosmic_config::{self, cosmic_config_derive::CosmicConfigEntry, CosmicConfigEntry};
use translator_core::Roi;

#[derive(Debug, Clone, CosmicConfigEntry, Eq, PartialEq)]
#[version = 1]
pub struct Config {
    pub enabled: bool,
    pub target_lang: String,
    pub source_lang: String,
    pub restore_token: Option<String>,
    pub preset: String,
    pub custom_roi: Option<Roi>,
    pub engine: String,
    pub ollama_endpoint: String,
    pub ollama_model: String,
    pub font_size: u16,
    pub hud_duration_ms: u64,
    pub hud_opacity: u8,
    pub settle_duration_ms: u64,
    pub glossary: HashMap<String, String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            enabled: false,
            target_lang: "tr".to_string(),
            source_lang: "auto".to_string(),
            restore_token: None,
            preset: "rpg_bottom_25".to_string(),
            custom_roi: None,
            engine: "google".to_string(),
            ollama_endpoint: "http://localhost:11434".to_string(),
            ollama_model: "qwen2.5:3b".to_string(),
            font_size: 22,
            hud_duration_ms: 6000,
            hud_opacity: 92,
            settle_duration_ms: 250,
            glossary: HashMap::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let cfg = Config::default();
        assert_eq!(cfg.preset, "rpg_bottom_25");
        assert_eq!(cfg.font_size, 22);
        assert_eq!(cfg.hud_opacity, 92);
        assert_eq!(cfg.ollama_endpoint, "http://localhost:11434");
    }
}
