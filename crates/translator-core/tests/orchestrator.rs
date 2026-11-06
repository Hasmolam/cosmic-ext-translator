use image::{Rgba, RgbaImage};
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::watch;
use translator_core::capture::pipewire_thread::CapturedFrame;
use translator_core::ocr::MockOcrBackend;
use translator_core::orchestrator::{spawn_orchestrator, OrchestratorConfig, OrchestratorEvent};
use translator_core::pipeline::debouncer::FrameDebouncer;
use translator_core::translation::{
    BoxFuture, MockTranslationBackend, TranslationBackend, TranslationContext,
};

// -----------------------------------------------------------------------------
// Orchestrator speaker tag preservation and HTTP 429 rate limit backoff
// -----------------------------------------------------------------------------
struct RateLimitFailingBackend {
    call_count: Arc<AtomicUsize>,
    should_fail_429: bool,
}

impl TranslationBackend for RateLimitFailingBackend {
    fn translate<'a>(
        &'a self,
        text: &'a str,
        _src_lang: &'a str,
        _target_lang: &'a str,
        _ctx: &'a TranslationContext,
    ) -> BoxFuture<'a, anyhow::Result<String>> {
        self.call_count.fetch_add(1, Ordering::SeqCst);
        let fail = self.should_fail_429;
        let text = text.to_string();

        Box::pin(async move {
            if fail {
                anyhow::bail!("HTTP 429 Rate Limit Exceeded: quota depleted");
            }
            // Confirm speaker was stripped from dialogue text passed to backend
            assert!(
                !text.contains('【') && !text.contains('】'),
                "TranslationBackend received raw speaker brackets instead of clean dialogue!"
            );
            Ok(format!("Translated: {text}"))
        })
    }
}

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

// -----------------------------------------------------------------------------
// Orchestrator cache decoupling across multiple distinct speakers
// -----------------------------------------------------------------------------
struct CountingMultiSpeakerBackend {
    calls: Arc<AtomicUsize>,
}

impl TranslationBackend for CountingMultiSpeakerBackend {
    fn translate<'a>(
        &'a self,
        text: &'a str,
        _src_lang: &'a str,
        _target_lang: &'a str,
        _ctx: &'a TranslationContext,
    ) -> BoxFuture<'a, anyhow::Result<String>> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let text = text.to_string();
        Box::pin(async move { Ok(format!("TR:{text}")) })
    }
}

#[derive(Debug)]
struct DynamicMockOcrBackend {
    texts: std::sync::Mutex<std::collections::VecDeque<String>>,
}

impl DynamicMockOcrBackend {
    fn new(texts: Vec<&str>) -> Self {
        Self {
            texts: std::sync::Mutex::new(texts.into_iter().map(|s| s.to_string()).collect()),
        }
    }
}

impl translator_core::ocr::OcrBackend for DynamicMockOcrBackend {
    fn recognize<'a>(
        &'a self,
        _image: &'a image::DynamicImage,
    ) -> translator_core::ocr::BoxFuture<'a, anyhow::Result<String>> {
        let mut guard = self.texts.lock().unwrap();
        let text = guard.pop_front().unwrap_or_default();
        Box::pin(async move { Ok(text) })
    }

    fn recognize_detailed<'a>(
        &'a self,
        _image: &'a image::DynamicImage,
        _orientation: Option<translator_core::ocr::TextOrientation>,
    ) -> translator_core::ocr::BoxFuture<'a, anyhow::Result<translator_core::ocr::OcrResult>> {
        let mut guard = self.texts.lock().unwrap();
        let text = guard.pop_front().unwrap_or_default();
        Box::pin(async move {
            Ok(translator_core::ocr::OcrResult {
                text,
                boxes: Vec::new(),
                orientation: translator_core::ocr::TextOrientation::Horizontal,
            })
        })
    }
}

fn make_test_frame(blocks: u32, width: u32, height: u32) -> CapturedFrame {
    let mut img = RgbaImage::from_pixel(width, height, Rgba([10, 10, 10, 255]));
    for i in 0..blocks {
        let start_x = 10 + i * 20;
        for y in 10..40 {
            for x in start_x..(start_x + 15).min(width) {
                img.put_pixel(x, y, Rgba([255, 255, 255, 255]));
            }
        }
    }
    CapturedFrame {
        image: img,
        timestamp: Instant::now(),
        original_width: width,
        original_height: height,
    }
}

#[tokio::test]
async fn test_orchestrator_speaker_tag_preservation_and_turn_recording() {
    let (frame_tx, frame_rx) = watch::channel(None);
    let ocr_backend = Arc::new(MockOcrBackend::new("【アリス】 気をつけてね。"));
    let call_count = Arc::new(AtomicUsize::new(0));
    let translation_backend = Arc::new(RateLimitFailingBackend {
        call_count: Arc::clone(&call_count),
        should_fail_429: false,
    });

    let debouncer = FrameDebouncer::new(2, Duration::from_millis(40), Duration::from_millis(400));

    let mut glossary = HashMap::new();
    glossary.insert("アリス".to_string(), "Alice".to_string());

    let config = OrchestratorConfig {
        forward_to_hud: false,
        glossary,
        typewriter_settle_duration: Duration::from_millis(50),
        ..Default::default()
    };

    let mut handle = spawn_orchestrator(
        frame_rx,
        debouncer,
        ocr_backend,
        translation_backend,
        config,
    );

    let event = handle.next_event().await.unwrap();
    assert_eq!(event, OrchestratorEvent::Started);

    // Push frames to settle debouncer
    frame_tx.send(Some(make_test_frame_with_blocks(0))).unwrap();
    tokio::time::sleep(Duration::from_millis(20)).await;
    frame_tx.send(Some(make_test_frame_with_blocks(2))).unwrap();
    tokio::time::sleep(Duration::from_millis(60)).await;
    frame_tx.send(Some(make_test_frame_with_blocks(2))).unwrap();

    // Expect translation event
    let mut got_translation = false;
    for _ in 0..10 {
        if let Ok(Some(OrchestratorEvent::TranslationReady {
            original,
            translated,
            cached,
        })) = tokio::time::timeout(Duration::from_millis(300), handle.next_event()).await
        {
            assert_eq!(original, "【アリス】 気をつけてね。");
            // Check that output preserved the speaker tag with glossary replacement
            assert!(
                translated.contains("【Alice】"),
                "Output '{translated}' did not preserve re-wrapped speaker tag!"
            );
            assert!(
                translated.contains("Translated: 気をつけてね。"),
                "Output '{translated}' did not translate dialogue properly!"
            );
            assert!(!cached);
            got_translation = true;
            break;
        }
    }
    assert!(
        got_translation,
        "Expected TranslationReady event with speaker preservation"
    );
    assert_eq!(call_count.load(Ordering::SeqCst), 1);

    handle.stop();
}

#[tokio::test]
async fn test_orchestrator_rate_limit_backoff_blocks_further_dispatches() {
    let (frame_tx, frame_rx) = watch::channel(None);
    let ocr_backend = Arc::new(MockOcrBackend::new("Text that triggers 429"));
    let call_count = Arc::new(AtomicUsize::new(0));
    let translation_backend = Arc::new(RateLimitFailingBackend {
        call_count: Arc::clone(&call_count),
        should_fail_429: true,
    });

    let debouncer = FrameDebouncer::new(2, Duration::from_millis(40), Duration::from_millis(400));

    let config = OrchestratorConfig {
        forward_to_hud: false,
        typewriter_settle_duration: Duration::from_millis(50),
        ..Default::default()
    };

    let mut handle = spawn_orchestrator(
        frame_rx,
        debouncer,
        ocr_backend,
        translation_backend,
        config,
    );

    let _ = handle.next_event().await; // Started

    // Trigger frame settle
    frame_tx.send(Some(make_test_frame_with_blocks(0))).unwrap();
    tokio::time::sleep(Duration::from_millis(20)).await;
    frame_tx.send(Some(make_test_frame_with_blocks(3))).unwrap();
    tokio::time::sleep(Duration::from_millis(60)).await;
    frame_tx.send(Some(make_test_frame_with_blocks(3))).unwrap();

    // Wait for Error event
    let mut got_error = false;
    for _ in 0..10 {
        if let Ok(Some(OrchestratorEvent::Error(err))) =
            tokio::time::timeout(Duration::from_millis(300), handle.next_event()).await
        {
            assert!(err.contains("429"));
            got_error = true;
            break;
        }
    }
    assert!(got_error, "Expected OrchestratorEvent::Error with 429");
    assert_eq!(call_count.load(Ordering::SeqCst), 1);

    // Trigger another frame settle while rate-limited (different block count to animate then settle)
    frame_tx.send(Some(make_test_frame_with_blocks(1))).unwrap();
    tokio::time::sleep(Duration::from_millis(60)).await;
    frame_tx.send(Some(make_test_frame_with_blocks(1))).unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Backend must NOT be called again because RateLimitManager throttles it!
    assert_eq!(
        call_count.load(Ordering::SeqCst),
        1,
        "Backend was called while rate limit throttle was active!"
    );

    handle.stop();
}

#[tokio::test]
async fn test_orchestrator_multi_speaker_cache_decoupling() {
    let (frame_tx, frame_rx) = watch::channel(None);
    let calls = Arc::new(AtomicUsize::new(0));
    let ocr_backend = Arc::new(DynamicMockOcrBackend::new(vec![
        "【Alice】 Good morning!",
        "【Bob】 Good morning!",
        "Good morning!",
    ]));
    let translation_backend = Arc::new(CountingMultiSpeakerBackend {
        calls: Arc::clone(&calls),
    });

    let debouncer = FrameDebouncer::new(2, Duration::from_millis(40), Duration::from_millis(400));

    let mut glossary = HashMap::new();
    glossary.insert("Alice".to_string(), "Aliye".to_string());
    glossary.insert("Bob".to_string(), "Bora".to_string());

    let config = OrchestratorConfig {
        forward_to_hud: false,
        glossary,
        typewriter_settle_duration: Duration::from_millis(50),
        ..Default::default()
    };

    let mut handle = spawn_orchestrator(
        frame_rx,
        debouncer,
        ocr_backend,
        translation_backend,
        config,
    );

    let _ = handle.next_event().await; // Started

    // Settle 1st frame with Alice
    frame_tx.send(Some(make_test_frame_with_blocks(0))).unwrap();
    tokio::time::sleep(Duration::from_millis(20)).await;
    frame_tx.send(Some(make_test_frame_with_blocks(2))).unwrap();
    tokio::time::sleep(Duration::from_millis(60)).await;
    frame_tx.send(Some(make_test_frame_with_blocks(2))).unwrap();

    let mut got_alice = false;
    for _ in 0..10 {
        if let Ok(Some(OrchestratorEvent::TranslationReady {
            original,
            translated,
            cached,
        })) = tokio::time::timeout(Duration::from_millis(300), handle.next_event()).await
        {
            assert_eq!(original, "【Alice】 Good morning!");
            assert_eq!(translated, "【Aliye】 TR:Good morning!");
            assert!(!cached);
            got_alice = true;
            break;
        }
    }
    assert!(got_alice, "Expected Alice translation");
    assert_eq!(calls.load(Ordering::SeqCst), 1);

    // Settle 2nd frame with Bob (different blocks -> trigger OCR for Bob)
    frame_tx.send(Some(make_test_frame_with_blocks(4))).unwrap();
    tokio::time::sleep(Duration::from_millis(60)).await;
    frame_tx.send(Some(make_test_frame_with_blocks(4))).unwrap();

    let mut got_bob = false;
    for _ in 0..10 {
        if let Ok(Some(OrchestratorEvent::TranslationReady {
            original,
            translated,
            cached,
        })) = tokio::time::timeout(Duration::from_millis(300), handle.next_event()).await
        {
            assert_eq!(original, "【Bob】 Good morning!");
            assert_eq!(translated, "【Bora】 TR:Good morning!");
            assert!(cached, "Bob translation must be a CACHE HIT!");
            got_bob = true;
            break;
        }
    }
    assert!(got_bob, "Expected Bob translation via cache hit");
    // Call count must STILL be 1 because it was a cache hit!
    assert_eq!(calls.load(Ordering::SeqCst), 1);

    // Settle 3rd frame without speaker (different blocks -> trigger OCR for narrator)
    frame_tx.send(Some(make_test_frame_with_blocks(6))).unwrap();
    tokio::time::sleep(Duration::from_millis(60)).await;
    frame_tx.send(Some(make_test_frame_with_blocks(6))).unwrap();

    let mut got_narrator = false;
    for _ in 0..10 {
        if let Ok(Some(OrchestratorEvent::TranslationReady {
            original,
            translated,
            cached,
        })) = tokio::time::timeout(Duration::from_millis(300), handle.next_event()).await
        {
            assert_eq!(original, "Good morning!");
            assert_eq!(translated, "TR:Good morning!");
            assert!(cached, "Narrator translation must also be a CACHE HIT!");
            got_narrator = true;
            break;
        }
    }
    assert!(got_narrator, "Expected Narrator translation via cache hit");
    // Call count must STILL be 1!
    assert_eq!(calls.load(Ordering::SeqCst), 1);

    handle.stop();
}

#[tokio::test]
async fn test_end_to_end_capture_to_hud_dispatch() {
    // 1. Set up watch channel simulating PipeWire frame stream
    let (frame_tx, frame_rx) = watch::channel(None);

    let debouncer = FrameDebouncer::new(2, Duration::from_millis(50), Duration::from_millis(1000));

    let ocr_backend = Arc::new(MockOcrBackend::new("Enemy spotted ahead!"));
    let translation_backend = Arc::new(MockTranslationBackend::new("TEST_TR"));

    let config = OrchestratorConfig {
        src_lang: "en".to_string(),
        target_lang: "tr".to_string(),
        forward_to_hud: false, // Testing pipeline events directly
        ..Default::default()
    };

    let mut orchestrator = spawn_orchestrator(
        frame_rx,
        debouncer,
        ocr_backend,
        translation_backend,
        config,
    );

    // Verify orchestrator started
    let event1 = orchestrator.next_event().await;
    assert_eq!(event1, Some(OrchestratorEvent::Started));

    // 2. Feed a sequence of frames:
    // Frame 0: Baseline empty frame
    frame_tx.send(Some(make_test_frame(0, 200, 100))).unwrap();
    tokio::time::sleep(Duration::from_millis(20)).await;

    // Frame 1: Text appears (1 block) -> enters Animating
    frame_tx.send(Some(make_test_frame(1, 200, 100))).unwrap();
    tokio::time::sleep(Duration::from_millis(20)).await;

    // Frame 2: Text expands (3 blocks)
    frame_tx.send(Some(make_test_frame(3, 200, 100))).unwrap();
    tokio::time::sleep(Duration::from_millis(70)).await;

    // Frame 3: Stable frame triggering settle
    frame_tx.send(Some(make_test_frame(3, 200, 100))).unwrap();

    // Verify TextDetected event
    let event2 = tokio::time::timeout(Duration::from_secs(3), orchestrator.next_event())
        .await
        .expect("Timeout waiting for TextDetected")
        .expect("Channel closed");

    match event2 {
        OrchestratorEvent::TextDetected { raw } => {
            assert_eq!(raw, "Enemy spotted ahead!");
        }
        other => panic!("Expected TextDetected, got {:?}", other),
    }

    // Verify TranslationReady event
    let event3 = tokio::time::timeout(Duration::from_secs(3), orchestrator.next_event())
        .await
        .expect("Timeout waiting for TranslationReady")
        .expect("Channel closed");

    match event3 {
        OrchestratorEvent::TranslationReady {
            original,
            translated,
            cached,
        } => {
            assert_eq!(original, "Enemy spotted ahead!");
            assert!(translated.contains("Enemy spotted ahead!"));
            assert!(!cached);
        }
        other => panic!("Expected TranslationReady, got {:?}", other),
    }

    // Stop orchestrator cleanly
    orchestrator.stop();
    let event4 = orchestrator.next_event().await;
    assert_eq!(event4, Some(OrchestratorEvent::Stopped));
}
