use image::{Rgba, RgbaImage};
use std::time::{Duration, Instant};
use translator_core::capture::pipewire_thread::CapturedFrame;
use translator_core::pipeline::debouncer::{DebounceState, FrameDebouncer, SettledFrame};

/// Rasterizes test dialogue text into an RGBA frame with high-contrast text pixels.
fn render_dialogue_frame(text: &str, width: u32, height: u32) -> CapturedFrame {
    let mut img = RgbaImage::from_pixel(width, height, Rgba([20, 24, 30, 255]));

    let char_w = 8;
    let char_h = 14;
    let margin_x = 16;
    let margin_y = 16;

    for (i, ch) in text.chars().enumerate() {
        let x_offset = margin_x + (i as u32) * (char_w + 2);
        if x_offset + char_w >= width {
            break;
        }

        let byte = ch as u8;
        for dy in 0..char_h {
            for dx in 0..char_w {
                let bit = (byte.wrapping_add(dy as u8 * 3 + dx as u8)).is_multiple_of(2);
                let color = if bit {
                    Rgba([240, 240, 250, 255])
                } else {
                    Rgba([40, 45, 60, 255])
                };
                let px = x_offset + dx;
                let py = margin_y + dy;
                if px < width && py < height {
                    img.put_pixel(px, py, color);
                }
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

#[test]
fn test_typewriter_stream_single_settle() {
    let settle_ms = 80;
    let max_anim_ms = 3000;
    let mut debouncer = FrameDebouncer::new(
        2,
        Duration::from_millis(settle_ms),
        Duration::from_millis(max_anim_ms),
    );

    let full_text = "The ancient dragon awakened in the fiery depths.";
    let mut settled_frames: Vec<SettledFrame> = Vec::new();

    // 1. Initial idle background
    let baseline = render_dialogue_frame("", 600, 80);
    assert!(debouncer.process_frame(baseline).is_none());
    assert_eq!(debouncer.state(), DebounceState::Idle);

    // 2. Typewriter streaming: 1 character added every 15ms
    let mut current_slice = String::new();
    for ch in full_text.chars() {
        current_slice.push(ch);
        let frame = render_dialogue_frame(&current_slice, 600, 80);

        if let Some(settled) = debouncer.process_frame(frame) {
            settled_frames.push(settled);
        }

        std::thread::sleep(Duration::from_millis(15));
    }

    // While typing was actively happening, debouncer should NOT have settled prematurely
    assert!(
        settled_frames.is_empty(),
        "Debouncer produced a settled frame while text was actively animating"
    );
    assert_eq!(debouncer.state(), DebounceState::Animating);

    // 3. Stability window: typing finished, wait longer than settle_duration
    std::thread::sleep(Duration::from_millis(settle_ms + 25));

    // Next frame with same finalized text must trigger Settled
    let settled_candidate = render_dialogue_frame(&current_slice, 600, 80);
    if let Some(settled) = debouncer.process_frame(settled_candidate) {
        settled_frames.push(settled);
    }

    assert_eq!(
        settled_frames.len(),
        1,
        "Expected exactly 1 settled frame upon text stabilization"
    );
    assert_eq!(debouncer.state(), DebounceState::Idle);

    // 4. Subsequent static frames (simulating player reading the dialogue for 30 frames)
    for _ in 0..30 {
        let static_frame = render_dialogue_frame(&current_slice, 600, 80);
        let extra = debouncer.process_frame(static_frame);
        assert!(
            extra.is_none(),
            "Static unchanging frames must not trigger duplicate OCR events"
        );
    }
}

#[test]
fn test_debouncer_max_animation_timeout() {
    let settle_ms = 200;
    let max_anim_ms = 100;
    let mut debouncer = FrameDebouncer::new(
        2,
        Duration::from_millis(settle_ms),
        Duration::from_millis(max_anim_ms),
    );

    let baseline = render_dialogue_frame("", 300, 60);
    debouncer.process_frame(baseline);

    let mut settled_opt = None;
    let start = Instant::now();

    let mut i = 0;
    while start.elapsed() < Duration::from_millis(max_anim_ms + 60) {
        i += 1;
        let text = format!("Continuous noisy streaming text stream #{i}");
        let frame = render_dialogue_frame(&text, 300, 60);
        if let Some(s) = debouncer.process_frame(frame) {
            settled_opt = Some(s);
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }

    assert!(
        settled_opt.is_some(),
        "Debouncer must forcefully settle when max_animation_duration is exceeded"
    );
}

#[test]
fn test_sequential_dialogue_turns() {
    let settle_ms = 60;
    let mut debouncer = FrameDebouncer::new(
        2,
        Duration::from_millis(settle_ms),
        Duration::from_millis(2000),
    );

    let turns = [
        "First turn: What is your quest?",
        "Second turn: To seek the Holy Grail.",
    ];

    let mut settled_count = 0;

    for turn in turns {
        let mut buf = String::new();
        for ch in turn.chars() {
            buf.push(ch);
            let frame = render_dialogue_frame(&buf, 600, 80);
            if debouncer.process_frame(frame).is_some() {
                settled_count += 1;
            }
            std::thread::sleep(Duration::from_millis(5));
        }

        std::thread::sleep(Duration::from_millis(settle_ms + 25));
        let settled_frame = render_dialogue_frame(&buf, 600, 80);
        if debouncer.process_frame(settled_frame).is_some() {
            settled_count += 1;
        }

        std::thread::sleep(Duration::from_millis(30));
    }

    assert_eq!(
        settled_count, 2,
        "Expected exactly 2 settled frames across 2 dialogue turns"
    );
}

#[test]
fn test_instant_dialogue_skip() {
    let settle_ms = 50;
    let mut debouncer = FrameDebouncer::new(
        2,
        Duration::from_millis(settle_ms),
        Duration::from_millis(2000),
    );

    // Frame 0: Baseline empty box
    let empty = render_dialogue_frame("", 500, 80);
    assert!(debouncer.process_frame(empty).is_none());

    // Frame 1: Full sentence appears instantly without typewriter animation
    let full = render_dialogue_frame("Skip mode: Instant dialogue apparition.", 500, 80);
    assert!(debouncer.process_frame(full.clone()).is_none());
    assert_eq!(debouncer.state(), DebounceState::Animating);

    // Wait for settle duration
    std::thread::sleep(Duration::from_millis(settle_ms + 20));

    // Next identical frame must settle
    let settled = debouncer.process_frame(full);
    assert!(
        settled.is_some(),
        "Instant dialogue change should settle after settle_duration"
    );
    assert_eq!(debouncer.state(), DebounceState::Idle);
}
