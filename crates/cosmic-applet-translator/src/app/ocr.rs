//! OCR backend lifecycle for the active capture session.

use std::sync::Arc;

use cosmic::iced::Task;
use tracing::{debug, error, info};
use translator_core::normalize_source_code;
use translator_core::ocr::{create_ocr_backend, OcrBackend, OcrEngineChoice, OrientationMode};
use translator_protocol::{HudMessage, HudStatus};

use super::{AppModel, Message};
use crate::config::Config;

/// Availability of the OCR backend for the current capture session.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum OcrStatus {
    /// No capture session; no backend needed.
    #[default]
    Idle,
    Loading,
    Ready,
    /// Backend construction failed; carries a user-facing reason.
    Unavailable(String),
}

impl AppModel {
    /// Builds a new OCR backend for the configured source language.
    ///
    /// The running orchestrator is stopped first so frames are not read with a backend
    /// for the wrong script while the new one loads.
    pub(super) fn rebuild_ocr_backend(&mut self) -> Task<cosmic::Action<Message>> {
        if let Some(old) = self.orchestrator.take() {
            old.stop();
        }
        self.ocr_backend = None;
        self.ocr_status = OcrStatus::Loading;
        self.ocr_generation = self.ocr_generation.wrapping_add(1);
        let generation = self.ocr_generation;
        let src_lang = self.config.source_lang.clone();
        let orientation = OrientationMode::from_code(&self.config.ocr_orientation);
        let engine = OcrEngineChoice::from_code(&self.config.ocr_engine);

        Task::perform(
            async move {
                let result = create_ocr_backend(&src_lang, orientation, engine)
                    .await
                    .map_err(|err| err.to_string());
                Message::OcrBackendLoaded { generation, result }
            },
            cosmic::Action::from,
        )
    }

    /// Invalidates any in-flight backend build and drops the current backend.
    pub(super) fn reset_ocr_backend(&mut self) {
        self.ocr_generation = self.ocr_generation.wrapping_add(1);
        self.ocr_backend = None;
        self.ocr_status = OcrStatus::Idle;
    }

    pub(super) fn ocr_backend_loaded(
        &mut self,
        generation: u64,
        result: Result<Arc<dyn OcrBackend>, String>,
    ) -> Task<cosmic::Action<Message>> {
        if generation != self.ocr_generation {
            debug!("Discarding superseded OCR backend build {generation}");
            return Task::none();
        }
        let status = match result {
            Ok(backend) => {
                info!("OCR backend ready, starting translation orchestrator");
                self.ocr_backend = Some(backend);
                self.ocr_status = OcrStatus::Ready;
                self.restart_orchestrator_if_active();
                None
            }
            Err(reason) => {
                error!("OCR backend unavailable: {reason}");
                self.ocr_backend = None;
                self.ocr_status = OcrStatus::Unavailable(reason.clone());
                Some(HudStatus::OcrUnavailable { reason })
            }
        };
        self.hud.send_task(HudMessage::SetHudStatus { status })
    }
}

/// Compares the config fields that determine which OCR backend is built.
#[must_use]
pub fn ocr_settings_differ(a: &Config, b: &Config) -> bool {
    normalize_source_code(&a.source_lang) != normalize_source_code(&b.source_lang)
        || OrientationMode::from_code(&a.ocr_orientation)
            != OrientationMode::from_code(&b.ocr_orientation)
        || OcrEngineChoice::from_code(&a.ocr_engine) != OcrEngineChoice::from_code(&b.ocr_engine)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ocr_rebuild_triggers() {
        let base = Config::default();
        let same_lang_alias = Config {
            source_lang: "AUTO".to_string(),
            ..Config::default()
        };
        assert!(!ocr_settings_differ(&base, &same_lang_alias));

        let japanese = Config {
            source_lang: "ja".to_string(),
            ..Config::default()
        };
        assert!(ocr_settings_differ(&base, &japanese));

        let japanese_alias = Config {
            source_lang: "jp".to_string(),
            ..Config::default()
        };
        assert!(!ocr_settings_differ(&japanese, &japanese_alias));

        let vertical = Config {
            ocr_orientation: "vertical".to_string(),
            ..japanese.clone()
        };
        assert!(ocr_settings_differ(&japanese, &vertical));

        let tesseract = Config {
            ocr_engine: "tesseract".to_string(),
            ..japanese.clone()
        };
        assert!(ocr_settings_differ(&japanese, &tesseract));
        let auto_alias = Config {
            ocr_engine: String::new(),
            ..japanese.clone()
        };
        assert!(!ocr_settings_differ(&japanese, &auto_alias));

        let unrelated = Config {
            font_size: 40,
            ..japanese.clone()
        };
        assert!(!ocr_settings_differ(&japanese, &unrelated));
    }
}
