//! Translation orchestrator: backend selection, (re)start and its event stream.

use std::sync::{Arc, OnceLock};
use std::time::Duration;

use tokio::sync::broadcast;
use translator_core::normalize_source_code;
use translator_core::orchestrator::{spawn_orchestrator, OrchestratorConfig, OrchestratorEvent};
use translator_core::pipeline::debouncer::FrameDebouncer;
use translator_core::pipeline::preprocess::PreprocessConfig;
use translator_core::translation::{
    parse_speaker_tags, DeepLTranslationBackend, GoogleTranslateBackend, OllamaTranslationBackend,
    OpenAiTranslationBackend, TranslationBackend,
};

use super::{AppModel, Message};
use crate::config::Config;

/// Maximum dHash Hamming distance at which two frames count as unchanged.
const DEBOUNCE_HASH_THRESHOLD: u32 = 2;
/// Upper bound on how long animated text may delay OCR.
const DEBOUNCE_MAX_WAIT: Duration = Duration::from_millis(2000);

static EVENT_BUS: OnceLock<broadcast::Sender<OrchestratorEvent>> = OnceLock::new();

/// Carries orchestrator events from the drain task to the iced subscription.
fn event_bus() -> &'static broadcast::Sender<OrchestratorEvent> {
    EVENT_BUS.get_or_init(|| broadcast::channel(256).0)
}

/// Translation backend for the configured engine.
#[must_use]
pub fn translation_backend(config: &Config) -> Arc<dyn TranslationBackend> {
    match config.engine.as_str() {
        "ollama" => Arc::new(OllamaTranslationBackend::new(
            &config.ollama_endpoint,
            &config.ollama_model,
        )),
        "deepl" => Arc::new(DeepLTranslationBackend::new(&config.deepl_api_key)),
        "openai" => Arc::new(OpenAiTranslationBackend::new(
            &config.openai_endpoint,
            &config.openai_api_key,
            &config.openai_model,
        )),
        _ => Arc::new(GoogleTranslateBackend::new()),
    }
}

/// Pre-processing applied to captured frames before OCR.
#[must_use]
pub fn preprocess_config(config: &Config) -> PreprocessConfig {
    PreprocessConfig {
        binarize: config.binarize,
        blur_subtraction: config.blur_subtraction,
        ..Default::default()
    }
}

/// True if switching from `a` to `b` changes what the orchestrator does.
#[must_use]
pub fn translation_settings_differ(a: &Config, b: &Config) -> bool {
    a.engine != b.engine
        || a.target_lang != b.target_lang
        || normalize_source_code(&a.source_lang) != normalize_source_code(&b.source_lang)
        || a.glossary != b.glossary
        || a.hud_duration_ms != b.hud_duration_ms
        || a.binarize != b.binarize
        || a.blur_subtraction != b.blur_subtraction
        || a.settle_duration_ms != b.settle_duration_ms
        || a.ollama_endpoint != b.ollama_endpoint
        || a.ollama_model != b.ollama_model
        || a.deepl_api_key != b.deepl_api_key
        || a.openai_endpoint != b.openai_endpoint
        || a.openai_api_key != b.openai_api_key
        || a.openai_model != b.openai_model
}

impl AppModel {
    /// Starts a new orchestrator with the current config when capture is running and an
    /// OCR backend is loaded. Any previous orchestrator is stopped.
    pub(super) fn restart_orchestrator_if_active(&mut self) {
        if !self.capture_active {
            return;
        }
        let (Some(session), Some(ocr_backend)) = (&self.capture_session, &self.ocr_backend) else {
            return;
        };

        if let Some(old) = self.orchestrator.take() {
            old.stop();
        }

        let orchestrator_config = OrchestratorConfig {
            src_lang: normalize_source_code(&self.config.source_lang),
            target_lang: self.config.target_lang.clone(),
            glossary: self.config.glossary.clone(),
            hud_duration_ms: self.config.hud_duration_ms,
            preprocess: preprocess_config(&self.config),
            ..Default::default()
        };

        let handle = spawn_orchestrator(
            session.frame_receiver.clone(),
            FrameDebouncer::new(
                DEBOUNCE_HASH_THRESHOLD,
                Duration::from_millis(self.config.settle_duration_ms),
                DEBOUNCE_MAX_WAIT,
            ),
            Arc::clone(ocr_backend),
            translation_backend(&self.config),
            orchestrator_config,
        );
        let mut drain_handle = handle.clone();
        let bus = event_bus().clone();
        tokio::spawn(async move {
            while let Some(event) = drain_handle.next_event().await {
                let _ = bus.send(event);
            }
        });
        self.orchestrator = Some(handle);
    }

    pub(super) fn stop_orchestrator(&mut self) {
        if let Some(orchestrator) = self.orchestrator.take() {
            orchestrator.stop();
        }
    }

    pub(super) fn orchestrator_event(&mut self, event: OrchestratorEvent) {
        match event {
            OrchestratorEvent::TextDetected { raw } => {
                self.last_ocr_text = raw;
            }
            OrchestratorEvent::TranslationReady {
                original,
                translated,
                cached: _,
            } => {
                let parsed = parse_speaker_tags(&original, &self.config.glossary);
                let speaker = parsed.resolved_speaker.or(parsed.raw_speaker);
                let source_text = if parsed.dialogue.is_empty() {
                    original.clone()
                } else {
                    parsed.dialogue
                };
                self.last_ocr_text = original;
                self.last_translated_text = translated.clone();
                self.backlog.push(speaker, source_text, translated);
            }
            _ => {}
        }
    }
}

pub(super) fn orchestrator_events_subscription() -> cosmic::iced::Subscription<Message> {
    struct OrchestratorSub;

    cosmic::iced::Subscription::run_with(std::any::TypeId::of::<OrchestratorSub>(), |_| {
        cosmic::iced::stream::channel(
            64,
            |mut output: cosmic::iced::futures::channel::mpsc::Sender<Message>| async move {
                use cosmic::iced::futures::SinkExt;
                let mut rx = event_bus().subscribe();
                while let Ok(event) = rx.recv().await {
                    if output
                        .send(Message::OrchestratorEventReceived(event))
                        .await
                        .is_err()
                    {
                        break;
                    }
                }
            },
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn translation_changes_are_detected() {
        let base = Config::default();
        let target = Config {
            target_lang: "de".into(),
            ..Config::default()
        };
        assert!(translation_settings_differ(&base, &target));

        let source_alias = Config {
            source_lang: "AUTO".into(),
            ..Config::default()
        };
        assert!(!translation_settings_differ(&base, &source_alias));

        let style_only = Config {
            font_size: base.font_size + 2,
            hud_position: "top".into(),
            ..Config::default()
        };
        assert!(!translation_settings_differ(&base, &style_only));

        let token_only = Config {
            restore_token: Some("token".into()),
            ..Config::default()
        };
        assert!(!translation_settings_differ(&base, &token_only));
    }
}
