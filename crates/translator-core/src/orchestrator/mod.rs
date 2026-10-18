use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, watch};
use tracing::{debug, error, info};

use crate::capture::pipewire_thread::CapturedFrame;
use crate::ipc::{HudIpcClient, HudMessage};
use crate::ocr::OcrBackend;
use crate::pipeline::debouncer::FrameDebouncer;
use crate::pipeline::preprocess::PreprocessConfig;
use crate::pipeline::spawn_recognition_pipeline;
use crate::translation::{
    FuzzyLruCache, TranslationBackend, TranslationContext, DEFAULT_CACHE_CAPACITY,
    DEFAULT_SIMILARITY_THRESHOLD,
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
        }
    }
}

/// Handle allowing caller to stop or monitor a running orchestrator session.
pub struct OrchestratorHandle {
    stop_tx: watch::Sender<bool>,
    events_rx: mpsc::Receiver<OrchestratorEvent>,
}

impl OrchestratorHandle {
    /// Signals the orchestrator worker loop to stop.
    pub fn stop(&self) {
        let _ = self.stop_tx.send(true);
    }

    /// Receives status events from the orchestrator.
    pub async fn next_event(&mut self) -> Option<OrchestratorEvent> {
        self.events_rx.recv().await
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

    let (mut pipeline_handle, mut recognized_rx) = spawn_recognition_pipeline(
        frame_rx,
        debouncer,
        config.preprocess,
        ocr_backend,
    );

    tokio::spawn(async move {
        let _ = event_tx.send(OrchestratorEvent::Started).await;
        info!("Translation orchestrator started");

        let mut cache = FuzzyLruCache::new(config.cache_capacity, config.similarity_threshold);
        let mut context = TranslationContext::default();
        for (term, repl) in &config.glossary {
            context.set_glossary_term(term.clone(), repl.clone());
        }
        let hud_client = if config.forward_to_hud {
            Some(HudIpcClient::default_client())
        } else {
            None
        };

        loop {
            tokio::select! {
                _ = stop_rx.changed() => {
                    if *stop_rx.borrow() {
                        debug!("Orchestrator received stop signal");
                        pipeline_handle.stop();
                        break;
                    }
                }
                event_opt = recognized_rx.recv() => {
                    let Some(event) = event_opt else {
                        debug!("Pipeline recognition channel closed");
                        break;
                    };

                    let recognized = event.text.trim().to_string();
                    if recognized.is_empty() {
                        continue;
                    }

                    let _ = event_tx.send(OrchestratorEvent::TextDetected {
                        raw: recognized.clone(),
                    }).await;

                    // Check fuzzy cache first to prevent redundant remote translation calls
                    let (translated, cached) = if let Some(hit) = cache.get(&recognized) {
                        (hit, true)
                    } else {
                        match translation_backend.translate(
                            &recognized,
                            &config.src_lang,
                            &config.target_lang,
                            &context,
                        ).await {
                            Ok(tr) => {
                                let tr_clean = tr.trim().to_string();
                                cache.insert(&recognized, &tr_clean);
                                context.add_turn(&recognized, &tr_clean);
                                (tr_clean, false)
                            }
                            Err(err) => {
                                let err_msg = format!("Translation failure: {err}");
                                error!("{err_msg}");
                                let _ = event_tx.send(OrchestratorEvent::Error(err_msg)).await;
                                if let Some(ref client) = hud_client {
                                    let err_summary = if err.to_string().contains("429") {
                                        "Hız Sınırı Aşıldı (HTTP 429 Rate Limit)"
                                    } else {
                                        "Çeviri Hatası (Bağlantı/Servis)"
                                    };
                                    let _ = client
                                        .send(&HudMessage::ShowSubtitle {
                                            text: format!("⚠️ {err_summary}"),
                                            original: Some(recognized.clone()),
                                            duration_ms: 3500,
                                        })
                                        .await;
                                }
                                continue;
                            }
                        }
                    };

                    let _ = event_tx.send(OrchestratorEvent::TranslationReady {
                        original: recognized.clone(),
                        translated: translated.clone(),
                        cached,
                    }).await;

                    // Forward to HUD overlay if configured
                    if let Some(ref client) = hud_client {
                        let msg = HudMessage::ShowSubtitle {
                            text: translated,
                            original: Some(recognized),
                            duration_ms: config.hud_duration_ms,
                        };
                        if let Err(err) = client.send(&msg).await {
                            debug!("HUD client send notice (HUD may not be listening): {err}");
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
        events_rx: event_rx,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;
    use crate::capture::pipewire_thread::CapturedFrame;
    use crate::ocr::MockOcrBackend;
    use crate::translation::MockTranslationBackend;
    use image::{Rgba, RgbaImage};

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
        let debouncer = FrameDebouncer::new(
            2,
            Duration::from_millis(40),
            Duration::from_millis(400),
        );

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
