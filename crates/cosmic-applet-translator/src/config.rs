use cosmic::cosmic_config::{self, cosmic_config_derive::CosmicConfigEntry, CosmicConfigEntry};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};
use tracing::{info, warn};
use translator_protocol::Roi;

/// Application ID: desktop entry, AppStream component and `cosmic-config` namespace.
pub const APP_ID: &str = translator_protocol::APPLET_CONFIG_ID;

/// Application ID used up to 0.1.x. Settings stored under it are migrated once.
pub const LEGACY_APP_ID: &str = "com.system76.CosmicExtTranslator";

#[derive(Debug, Clone, CosmicConfigEntry, Eq, PartialEq, Serialize, Deserialize)]
#[version = 1]
pub struct Config {
    pub enabled: bool,
    pub target_lang: String,
    pub source_lang: String,
    /// Text direction for CJK OCR: `auto`, `horizontal` or `vertical`.
    #[serde(default = "default_ocr_orientation")]
    pub ocr_orientation: String,
    /// OCR engine for Japanese and Chinese: `auto`, `paddle` or `tesseract`.
    #[serde(default = "default_ocr_engine")]
    pub ocr_engine: String,
    pub restore_token: Option<String>,
    pub preset: String,
    pub custom_roi: Option<Roi>,
    #[serde(default)]
    pub ignore_roi: Option<Roi>,
    pub engine: String,
    pub ollama_endpoint: String,
    pub ollama_model: String,
    pub deepl_api_key: String,
    pub openai_endpoint: String,
    pub openai_api_key: String,
    pub openai_model: String,
    pub binarize: bool,
    #[serde(default)]
    pub blur_subtraction: bool,
    pub hud_position: String,
    pub hud_persistent: bool,
    pub hud_offset_y: u32,
    pub font_size: u16,
    pub hud_duration_ms: u64,
    pub hud_opacity: u8,
    pub settle_duration_ms: u64,
    pub glossary: HashMap<String, String>,
}

fn default_ocr_orientation() -> String {
    "auto".to_string()
}

fn default_ocr_engine() -> String {
    "auto".to_string()
}

impl Default for Config {
    fn default() -> Self {
        Self {
            enabled: false,
            target_lang: "tr".to_string(),
            source_lang: "auto".to_string(),
            ocr_orientation: default_ocr_orientation(),
            ocr_engine: default_ocr_engine(),
            restore_token: None,
            preset: "rpg_bottom_25".to_string(),
            custom_roi: None,
            ignore_roi: None,
            engine: "google".to_string(),
            ollama_endpoint: "http://localhost:11434".to_string(),
            ollama_model: "qwen2.5:3b".to_string(),
            deepl_api_key: String::new(),
            openai_endpoint: "https://api.openai.com/v1".to_string(),
            openai_api_key: String::new(),
            openai_model: "gpt-4o-mini".to_string(),
            binarize: false,
            blur_subtraction: false,
            hud_position: "bottom".to_string(),
            hud_persistent: true,
            hud_offset_y: 40,
            font_size: 22,
            hud_duration_ms: 0,
            hud_opacity: 92,
            settle_duration_ms: 180,
            glossary: HashMap::new(),
        }
    }
}

/// Copies settings saved under [`LEGACY_APP_ID`] to [`APP_ID`] if the new location has
/// no settings yet. The old directory is left in place so an older build still works.
pub fn migrate_legacy_config() {
    let Some(base) = cosmic_config_base() else {
        return;
    };
    match migrate_config_dir(&base, LEGACY_APP_ID, APP_ID, Config::VERSION) {
        Ok(true) => info!("Migrated settings from {LEGACY_APP_ID} to {APP_ID}"),
        Ok(false) => {}
        Err(err) => warn!("Cannot migrate settings from {LEGACY_APP_ID}: {err}"),
    }
}

/// `$XDG_CONFIG_HOME/cosmic`, falling back to `~/.config/cosmic`.
fn cosmic_config_base() -> Option<PathBuf> {
    let config_home = std::env::var_os("XDG_CONFIG_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
    Some(config_home.join("cosmic"))
}

/// Copies `<base>/<from>/v<version>/*` to `<base>/<to>/v<version>/`.
///
/// Returns `Ok(false)` when there is nothing to migrate or the target already exists.
/// Files are copied into a temporary directory that is renamed into place, so an
/// interrupted migration does not leave a partial settings directory.
pub fn migrate_config_dir(base: &Path, from: &str, to: &str, version: u64) -> io::Result<bool> {
    let version_dir = format!("v{version}");
    let src = base.join(from).join(&version_dir);
    let dst_parent = base.join(to);
    let dst = dst_parent.join(&version_dir);
    if dst.exists() || !src.is_dir() {
        return Ok(false);
    }

    let tmp = dst_parent.join(format!(".{version_dir}.migrating"));
    if tmp.exists() {
        std::fs::remove_dir_all(&tmp)?;
    }
    std::fs::create_dir_all(&tmp)?;
    for entry in std::fs::read_dir(&src)? {
        let entry = entry?;
        if entry.file_type()?.is_file() {
            std::fs::copy(entry.path(), tmp.join(entry.file_name()))?;
        }
    }
    std::fs::rename(&tmp, &dst)?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hud_writes_to_this_config_version() {
        assert_eq!(Config::VERSION, translator_protocol::APPLET_CONFIG_VERSION);
    }

    fn temp_base(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "cosmic-ext-translator-cfg-{tag}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn legacy_config_is_copied_once() {
        let base = temp_base("migrate");
        let old = base.join("old.Id/v1");
        std::fs::create_dir_all(&old).unwrap();
        std::fs::write(old.join("target_lang"), "\"de\"").unwrap();

        assert!(migrate_config_dir(&base, "old.Id", "new.Id", 1).unwrap());
        let migrated = base.join("new.Id/v1/target_lang");
        assert_eq!(std::fs::read_to_string(&migrated).unwrap(), "\"de\"");
        assert!(old.join("target_lang").exists(), "legacy settings are kept");

        // Settings changed after migration are not overwritten by a second run.
        std::fs::write(&migrated, "\"fr\"").unwrap();
        assert!(!migrate_config_dir(&base, "old.Id", "new.Id", 1).unwrap());
        assert_eq!(std::fs::read_to_string(&migrated).unwrap(), "\"fr\"");
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn missing_legacy_config_is_not_an_error() {
        let base = temp_base("missing");
        assert!(!migrate_config_dir(&base, "old.Id", "new.Id", 1).unwrap());
        assert!(!base.join("new.Id").exists());
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn test_default_config() {
        let cfg = Config::default();
        assert_eq!(cfg.preset, "rpg_bottom_25");
        assert_eq!(cfg.font_size, 22);
        assert_eq!(cfg.hud_opacity, 92);
        assert_eq!(cfg.ollama_endpoint, "http://localhost:11434");
        assert_eq!(cfg.ocr_orientation, "auto");
        assert_eq!(cfg.ocr_engine, "auto");
    }
}
