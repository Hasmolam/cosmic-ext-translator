use cosmic::cosmic_config::{self, cosmic_config_derive::CosmicConfigEntry, CosmicConfigEntry};

#[derive(Debug, Default, Clone, CosmicConfigEntry, Eq, PartialEq)]
#[version = 1]
pub struct Config {
    pub enabled: bool,
    pub target_lang: String,
    pub source_lang: String,
    pub restore_token: Option<String>,
}
