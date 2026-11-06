use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, watch};
use tracing::{debug, error, info, trace, warn};

use crate::capture::pipewire_thread::CapturedFrame;
use crate::ipc::{HudIpcClient, HudMessage, HudStatus};
use crate::ocr::OcrBackend;
use crate::pipeline::debouncer::{FrameDebouncer, TextDiffAction, TextGate};
use crate::pipeline::preprocess::PreprocessConfig;
use crate::pipeline::spawn_recognition_pipeline;
use crate::translation::{
    parse_speaker_tags, FuzzyLruCache, RateLimitManager, TranslationBackend, TranslationContext,
    DEFAULT_CACHE_CAPACITY, DEFAULT_SIMILARITY_THRESHOLD,
};

/// Events emitted by the orchestrator for UI display and status monitoring.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OrchestratorEvent {
    Started,
    TextDetected {
        raw: String,
    },
    TranslationReady {
        original: String,
        translated: String,
        cached: bool,
    },
    Stopped,
    Error(String),
}

/// Configuration for the end-to-end translation orchestrator.
#[derive(Clone)]
pub struct OrchestratorConfig {
    pub src_lang: String,
    pub target_lang: String,
    pub preprocess: PreprocessConfig,
    pub cache_capacity: usize,
    pub similarity_threshold: f64,
    pub forward_to_hud: bool,
    pub glossary: HashMap<String, String>,
    pub hud_duration_ms: u64,
    pub typewriter_settle_duration: Duration,
    pub max_history: usize,
}

impl Default for OrchestratorConfig {
    fn default() -> Self {
        Self {
            src_lang: "en".to_string(),
            target_lang: "tr".to_string(),
            preprocess: PreprocessConfig::default(),
            cache_capacity: DEFAULT_CACHE_CAPACITY,
            similarity_threshold: DEFAULT_SIMILARITY_THRESHOLD,
            forward_to_hud: true,
            glossary: HashMap::new(),
            hud_duration_ms: 0, // 0 = persistent / stay until replaced by next dialogue
            typewriter_settle_duration: Duration::from_millis(250),
            max_history: 4,
        }
    }
}

/// Handle allowing caller to stop or monitor a running orchestrator session.
#[derive(Clone)]
pub struct OrchestratorHandle {
    stop_tx: watch::Sender<bool>,
    events_rx: Arc<tokio::sync::Mutex<mpsc::Receiver<OrchestratorEvent>>>,
}

impl OrchestratorHandle {
    /// Signals the orchestrator worker loop to stop.
    pub fn stop(&self) {
        let _ = self.stop_tx.send(true);
    }

    /// Receives status events from the orchestrator.
    pub async fn next_event(&mut self) -> Option<OrchestratorEvent> {
        self.events_rx.lock().await.recv().await
    }
}

/// Spawns the end-to-end translation loop tying capture, debouncing, OCR, caching,
/// and HUD dispatch together.
pub fn spawn_orchestrator(
    frame_rx: watch::Receiver<Option<CapturedFrame>>,
    debouncer: FrameDebouncer,
    ocr_backend: Arc<dyn OcrBackend>,
    translation_backend: Arc<dyn TranslationBackend>,
    config: OrchestratorConfig,
) -> OrchestratorHandle {
    let (stop_tx, mut stop_rx) = watch::channel(false);
    let (event_tx, event_rx) = mpsc::channel(64);

    let (mut pipeline_handle, mut recognized_rx) =
        spawn_recognition_pipeline(frame_rx, debouncer, config.preprocess.clone(), ocr_backend);

    tokio::spawn(async move {
        let _ = event_tx.send(OrchestratorEvent::Started).await;
        info!("Translation orchestrator started");

        let mut cache = FuzzyLruCache::new(config.cache_capacity, config.similarity_threshold);
        let mut context = TranslationContext::new(config.max_history);
        for (term, repl) in &config.glossary {
            context.set_glossary_term(term.clone(), repl.clone());
        }
        let mut rate_limit_manager = RateLimitManager::default();
        let mut status_shown = false;

        let hud_client = if config.forward_to_hud {
            Some(HudIpcClient::default_client())
        } else {
            None
        };

        let mut text_gate = TextGate::new(config.typewriter_settle_duration, 0.92, 0.60);
        let mut typewriter_deadline: Option<tokio::time::Instant> = None;

        loop {
            tokio::select! {
                _ = stop_rx.changed() => {
                    if *stop_rx.borrow() {
                        debug!("Orchestrator received stop signal");
                        pipeline_handle.stop();
                        break;
                    }
                }

                // Typewriter settle timeout: fires when progressive text expansion pauses
                _ = async {
                    match typewriter_deadline {
                        Some(deadline) => tokio::time::sleep_until(deadline).await,
                        None => std::future::pending().await,
                    }
                }, if typewriter_deadline.is_some() => {
                    typewriter_deadline = None;
                    if let Some(settled_text) = text_gate.flush_pending() {
                        debug!("Orchestrator: typewriter stream settled: \"{settled_text}\"");
                        let mut state = OrchestratorSessionState {
                            cache: &mut cache,
                            context: &mut context,
                            rate_limit_manager: &mut rate_limit_manager,
                            status_shown: &mut status_shown,
                            translation_backend: &translation_backend,
                            config: &config,
                            hud_client: hud_client.as_ref(),
                            event_tx: &event_tx,
                        };
                        dispatch_translation(settled_text, &mut state).await;
                    }
                }

                event_opt = recognized_rx.recv() => {
                    let Some(event) = event_opt else {
                        debug!("Pipeline recognition channel closed");
                        break;
                    };

                    let candidate = event.text.trim();
                    if candidate.is_empty() {
                        continue;
                    }

                    match text_gate.evaluate(candidate) {
                        TextDiffAction::SuppressJitter => {
                            trace!("Orchestrator: candidate \"{candidate}\" suppressed as jitter");
                        }
                        TextDiffAction::TypewriterWait => {
                            debug!("Orchestrator: candidate \"{candidate}\" is expanding; holding emission");
                            typewriter_deadline = Some(tokio::time::Instant::now() + config.typewriter_settle_duration);
                        }
                        TextDiffAction::EmitTranslation => {
                            // Immediate dialogue turn transition: cancel any pending typewriter timer
                            typewriter_deadline = None;
                            let text = text_gate.last_emitted_text().to_string();
                            let mut state = OrchestratorSessionState {
                                cache: &mut cache,
                                context: &mut context,
                                rate_limit_manager: &mut rate_limit_manager,
                                status_shown: &mut status_shown,
                                translation_backend: &translation_backend,
                                config: &config,
                                hud_client: hud_client.as_ref(),
                                event_tx: &event_tx,
                            };
                            dispatch_translation(text, &mut state).await;
                        }
                    }
                }
            }
        }

        let _ = event_tx.send(OrchestratorEvent::Stopped).await;
        info!("Translation orchestrator stopped");
    });

    OrchestratorHandle {
        stop_tx,
        events_rx: Arc::new(tokio::sync::Mutex::new(event_rx)),
    }
}

struct OrchestratorSessionState<'a> {
    cache: &'a mut FuzzyLruCache,
    context: &'a mut TranslationContext,
    rate_limit_manager: &'a mut RateLimitManager,
    /// True while a status badge set by this session is visible on the HUD.
    status_shown: &'a mut bool,
    translation_backend: &'a Arc<dyn TranslationBackend>,
    config: &'a OrchestratorConfig,
    hud_client: Option<&'a HudIpcClient>,
    event_tx: &'a mpsc::Sender<OrchestratorEvent>,
}

async fn dispatch_translation(text: String, state: &mut OrchestratorSessionState<'_>) {
    // If rate-limited, notify HUD and throttle request dispatch
    if let Some(remaining) = state.rate_limit_manager.remaining_pause() {
        let secs = remaining.as_secs().max(1);
        warn!("Orchestrator rate-limited, retrying in {secs}s");
        set_hud_status(
            state,
            Some(HudStatus::RateLimited {
                retry_after_secs: secs,
            }),
        )
        .await;
        return;
    }

    let _ = state
        .event_tx
        .send(OrchestratorEvent::TextDetected { raw: text.clone() })
        .await;

    // Parse speaker tag if present to translate dialogue body cleanly
    let parsed = parse_speaker_tags(&text, state.context.glossary());
    let text_to_translate = if parsed.bracket_style.is_some() && !parsed.dialogue.is_empty() {
        &parsed.dialogue
    } else {
        &text
    };

    // Check fuzzy cache first to prevent redundant remote translation calls
    let (translated, cached) = if let Some(hit) = state.cache.get(text_to_translate) {
        (hit, true)
    } else {
        match state
            .translation_backend
            .translate(
                text_to_translate,
                &state.config.src_lang,
                &state.config.target_lang,
                state.context,
            )
            .await
        {
            Ok(tr) => {
                let tr_clean = tr.trim().to_string();
                state.cache.insert(text_to_translate, &tr_clean);
                state.rate_limit_manager.record_success();
                if *state.status_shown {
                    set_hud_status(state, None).await;
                }
                (tr_clean, false)
            }
            Err(err) => {
                let err_msg = format!("Translation failure: {err}");
                error!("{err_msg}");
                let is_429 = err.to_string().contains("429");
                let status = if is_429 {
                    let pause = state.rate_limit_manager.record_rate_limit();
                    HudStatus::RateLimited {
                        retry_after_secs: pause.as_secs().max(1),
                    }
                } else {
                    HudStatus::TranslationFailed
                };
                set_hud_status(state, Some(status)).await;
                let _ = state.event_tx.send(OrchestratorEvent::Error(err_msg)).await;
                return;
            }
        }
    };

    // Re-wrap translation with speaker tag if present
    let final_translated = match (&parsed.bracket_style, &parsed.resolved_speaker) {
        (Some(style), Some(speaker)) => {
            let wrap = style.wrap(speaker);
            if wrap.ends_with(' ') {
                format!("{wrap}{translated}")
            } else {
                format!("{wrap} {translated}")
            }
        }
        _ => translated.clone(),
    };

    // Record turn in context with resolved speaker attribution on both cache hits and fresh translations
    state.context.add_turn_with_speaker(
        parsed.resolved_speaker.as_deref(),
        &text,
        &final_translated,
    );

    let _ = state
        .event_tx
        .send(OrchestratorEvent::TranslationReady {
            original: text.clone(),
            translated: final_translated.clone(),
            cached,
        })
        .await;

    // Forward to HUD overlay if configured
    if let Some(client) = state.hud_client {
        let msg = HudMessage::ShowSubtitle {
            text: final_translated,
            original: Some(text),
            duration_ms: state.config.hud_duration_ms,
        };
        if let Err(err) = client.send(&msg).await {
            debug!("HUD client send notice (HUD may not be listening): {err}");
        }
    }
}

/// Sends a status badge update to the HUD and records whether one is visible.
async fn set_hud_status(state: &mut OrchestratorSessionState<'_>, status: Option<HudStatus>) {
    *state.status_shown = status.is_some();
    if let Some(client) = state.hud_client {
        if let Err(err) = client.send(&HudMessage::SetHudStatus { status }).await {
            debug!("HUD status send failed (HUD may not be listening): {err}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capture::pipewire_thread::CapturedFrame;
    use crate::ocr::MockOcrBackend;
    use crate::translation::MockTranslationBackend;
    use image::{Rgba, RgbaImage};
    use std::time::Instant;

    use std::time::Duration;

    fn make_test_frame_with_blocks(num_blocks: u32) -> CapturedFrame {
        let mut img = RgbaImage::from_pixel(100, 50, Rgba([0, 0, 0, 255]));
        for i in 0..num_blocks {
            let start_x = (i * 12).min(90);
            for y in 10..40 {
                for x in start_x..(start_x + 8).min(100) {
                    img.put_pixel(x, y, Rgba([255, 255, 255, 255]));
                }
            }
        }
        CapturedFrame {
            image: img,
            timestamp: Instant::now(),
            original_width: 100,
            original_height: 50,
        }
    }

    #[tokio::test]
    async fn test_orchestrator_mock_flow() {
        let (frame_tx, frame_rx) = watch::channel(None);
        let ocr_backend = Arc::new(MockOcrBackend::new("Victory Royale"));
        let translation_backend = Arc::new(MockTranslationBackend::new("TR"));
        let debouncer =
            FrameDebouncer::new(2, Duration::from_millis(40), Duration::from_millis(400));

        let config = OrchestratorConfig {
            forward_to_hud: false, // Don't require running HUD socket in test
            ..Default::default()
        };

        let mut handle = spawn_orchestrator(
            frame_rx,
            debouncer,
            ocr_backend,
            translation_backend,
            config,
        );

        // First event should be Started
        let event = handle.next_event().await.unwrap();
        assert_eq!(event, OrchestratorEvent::Started);

        // Push frame 1 (baseline: 0 blocks)
        frame_tx.send(Some(make_test_frame_with_blocks(0))).unwrap();
        tokio::time::sleep(Duration::from_millis(20)).await;

        // Push frame 2 (animating: 2 blocks)
        frame_tx.send(Some(make_test_frame_with_blocks(2))).unwrap();
        tokio::time::sleep(Duration::from_millis(60)).await;

        // Push frame 3 (settled: 2 blocks)
        frame_tx.send(Some(make_test_frame_with_blocks(2))).unwrap();

        // Wait for translation ready event
        let mut got_translation = false;
        for _ in 0..5 {
            if let Ok(Some(OrchestratorEvent::TranslationReady { translated, .. })) =
                tokio::time::timeout(Duration::from_millis(500), handle.next_event()).await
            {
                assert!(translated.contains("Victory Royale"));
                got_translation = true;
                break;
            }
        }
        assert!(got_translation, "Expected TranslationReady event");

        handle.stop();
    }
}
