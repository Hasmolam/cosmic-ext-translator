use image::{Rgba, RgbaImage};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::watch;
use translator_core::capture::pipewire_thread::CapturedFrame;
use translator_core::capture::roi::{CaptureRegion, Roi, RoiPreset};
use translator_core::ocr::MockOcrBackend;
use translator_core::orchestrator::{spawn_orchestrator, OrchestratorConfig, OrchestratorEvent};
use translator_core::pipeline::debouncer::FrameDebouncer;
use translator_core::translation::MockTranslationBackend;

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
async fn test_roi_cropping_formats_and_strides() {
    let width = 200;
    let height = 100;
    let effective_stride = (width * 4) as usize;

    // 1. Test RGBA format
    let mut rgba_raw = vec![0u8; height as usize * effective_stride];
    for y in 0..height as usize {
        for x in 0..width as usize {
            let offset = y * effective_stride + x * 4;
            rgba_raw[offset] = 200; // R
            rgba_raw[offset + 1] = 100; // G
            rgba_raw[offset + 2] = 50; // B
            rgba_raw[offset + 3] = 255; // A
        }
    }

    let roi = Roi::new(20, 20, 50, 40);
    let cropped = roi
        .crop_to_rgba(&rgba_raw, width, height, effective_stride, false)
        .expect("RGBA cropping should succeed");

    assert_eq!(cropped.width(), 50);
    assert_eq!(cropped.height(), 40);
    assert_eq!(cropped.get_pixel(0, 0), &Rgba([200, 100, 50, 255]));

    // 2. Test BGRx format (swapped B and R channels)
    let mut bgrx_raw = vec![0u8; height as usize * effective_stride];
    for y in 0..height as usize {
        for x in 0..width as usize {
            let offset = y * effective_stride + x * 4;
            bgrx_raw[offset] = 50; // B
            bgrx_raw[offset + 1] = 100; // G
            bgrx_raw[offset + 2] = 200; // R
            bgrx_raw[offset + 3] = 255; // x
        }
    }

    let cropped_bgr = roi
        .crop_to_rgba(&bgrx_raw, width, height, effective_stride, true)
        .expect("BGRx cropping should succeed");

    // After swapping, R should be 200 and B should be 50
    assert_eq!(cropped_bgr.get_pixel(0, 0), &Rgba([200, 100, 50, 255]));

    // 3. Test non-standard padded stride (e.g. GPU buffer row alignment)
    let padded_stride = effective_stride + 128; // 128 bytes padding per line
    let mut padded_raw = vec![0u8; height as usize * padded_stride];
    for y in 0..height as usize {
        for x in 0..width as usize {
            let offset = y * padded_stride + x * 4;
            padded_raw[offset] = 123;
            padded_raw[offset + 1] = 45;
            padded_raw[offset + 2] = 67;
            padded_raw[offset + 3] = 255;
        }
    }

    let cropped_padded = roi
        .crop_to_rgba(&padded_raw, width, height, padded_stride, false)
        .expect("Padded stride cropping should succeed");
    assert_eq!(cropped_padded.get_pixel(10, 10), &Rgba([123, 45, 67, 255]));
}

#[tokio::test]
async fn test_capture_region_presets_computation() {
    let screen_w = 1920;
    let screen_h = 1080;

    // FullScreen preset
    let full = CaptureRegion::new(RoiPreset::FullScreen, None, None);
    let full_roi = full.compute(screen_w, screen_h).unwrap();
    assert_eq!(full_roi.width, 1920);
    assert_eq!(full_roi.height, 1080);
    assert_eq!(full_roi.x, 0);
    assert_eq!(full_roi.y, 0);

    // Bottom 25% preset (typical RPG dialogue box: 5% margin each side)
    let b25 = CaptureRegion::new(RoiPreset::Bottom25, None, None);
    let b25_roi = b25.compute(screen_w, screen_h).unwrap();
    assert_eq!(b25_roi.x, 96);
    assert_eq!(b25_roi.y, 810);
    assert_eq!(b25_roi.width, 1728);
    assert_eq!(b25_roi.height, 270);

    // Custom preset
    let custom_preset =
        CaptureRegion::new(RoiPreset::Custom, Some(Roi::new(50, 60, 400, 200)), None);
    let custom_roi = custom_preset.compute(screen_w, screen_h).unwrap();
    assert_eq!(custom_roi.x, 50);
    assert_eq!(custom_roi.y, 60);
    assert_eq!(custom_roi.width, 400);
    assert_eq!(custom_roi.height, 200);
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
