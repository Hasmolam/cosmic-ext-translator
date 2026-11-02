use std::time::{Duration, Instant};
use image::{Rgba, RgbaImage};
use translator_core::capture::roi::Roi;
use translator_core::pipeline::debouncer::{TextDiffAction, TextGate};

// ============================================================================
// 1. TextGate Adversarial Stress Tests
// ============================================================================

#[test]
fn test_adversarial_typewriter_1000_char_stream() {
    let mut gate = TextGate::new(Duration::from_millis(250), 0.92, 0.60);
    let start_time = Instant::now();

    // Generate a 1000-character sentence
    let mut text_chars = Vec::new();
    for i in 0..1000 {
        // Mix ASCII characters to create a realistic long narrative
        let ch = match i % 26 {
            0 => 'A',
            idx => (b'a' + (idx as u8)) as char,
        };
        text_chars.push(ch);
    }
    let full_string: String = text_chars.into_iter().collect();

    // Step 0: Initial character
    let mut current = String::new();
    current.push(full_string.chars().next().unwrap());
    let action0 = gate.evaluate_at(&current, start_time);
    assert_eq!(
        action0,
        TextDiffAction::EmitTranslation,
        "First character should emit translation"
    );
    assert_eq!(gate.last_emitted_text(), current);

    // Steps 1 to 999: Each step appends 1 character
    let chars: Vec<char> = full_string.chars().collect();
    for (i, &ch) in chars.iter().enumerate().skip(1) {
        current.push(ch);
        let now = start_time + Duration::from_millis(i as u64 * 10);
        let action = gate.evaluate_at(&current, now);

        // Verification requirement: 1-character typewriter steps on long sentences
        // (sim > 0.98) are NEVER falsely suppressed as jitter!
        assert_eq!(
            action,
            TextDiffAction::TypewriterWait,
            "Step {i} (len={}): 1-char typewriter increment was falsely handled as {:?} instead of TypewriterWait",
            current.len(),
            action
        );
    }

    assert!(
        gate.has_pending_expansion(),
        "Should have pending expansion at the end of typewriter typing"
    );

    // Timeout settles -> flush pending
    let flushed = gate.flush_pending();
    assert_eq!(
        flushed,
        Some(full_string.clone()),
        "Flushed pending should match full 1000-char string"
    );
    assert_eq!(gate.last_emitted_text(), full_string);
    assert!(!gate.has_pending_expansion());
    assert_eq!(gate.flush_pending(), None);
}

#[test]
fn test_adversarial_typewriter_variable_pauses_and_duplicates() {
    let mut gate = TextGate::new(Duration::from_millis(250), 0.92, 0.60);
    let t0 = Instant::now();

    // Character typing with variable pauses
    assert_eq!(gate.evaluate_at("Hold", t0), TextDiffAction::EmitTranslation);

    // Quick typing (15ms)
    assert_eq!(
        gate.evaluate_at("Hold on", t0 + Duration::from_millis(15)),
        TextDiffAction::TypewriterWait
    );

    // Exact duplicate frame while waiting on typewriter (game render cycle, no new letter yet)
    // Must return SuppressJitter WITHOUT modifying candidate text
    assert_eq!(
        gate.evaluate_at("Hold on", t0 + Duration::from_millis(30)),
        TextDiffAction::SuppressJitter
    );
    assert_eq!(gate.last_candidate_text(), "Hold on");

    // Medium pause (150ms < 250ms settle)
    assert_eq!(
        gate.evaluate_at("Hold on, wait", t0 + Duration::from_millis(180)),
        TextDiffAction::TypewriterWait
    );

    // Another duplicate
    assert_eq!(
        gate.evaluate_at("Hold on, wait", t0 + Duration::from_millis(220)),
        TextDiffAction::SuppressJitter
    );

    // Final punctuation added
    assert_eq!(
        gate.evaluate_at("Hold on, wait!", t0 + Duration::from_millis(350)),
        TextDiffAction::TypewriterWait
    );

    // Settle timeout passed: flushed
    assert_eq!(
        gate.flush_pending(),
        Some("Hold on, wait!".to_string())
    );
}

#[test]
fn test_adversarial_typewriter_japanese_multibyte_stream() {
    let mut gate = TextGate::new(Duration::from_millis(250), 0.92, 0.60);
    let t0 = Instant::now();

    // 1. Incremental progressive typing stream with multi-byte Japanese characters:
    // 「あ」 -> 「ありがとう」 -> 「ありがとうございます」
    // In actual game dialogue streaming, characters append as:
    // 「 -> 「あ -> 「あり -> 「ありが -> 「ありがと -> 「ありがとう -> ... -> 「ありがとうございます」
    let japanese_steps = [
        "「あ",
        "「あり",
        "「ありが",
        "「ありがと",
        "「ありがとう",
        "「ありがとうご",
        "「ありがとうござ",
        "「ありがとうござい",
        "「ありがとうございます",
        "「ありがとうございます」",
    ];

    // Initial character
    assert_eq!(gate.evaluate_at(japanese_steps[0], t0), TextDiffAction::EmitTranslation);

    // All subsequent characters must be TypewriterWait
    for (idx, step) in japanese_steps[1..].iter().enumerate() {
        let t = t0 + Duration::from_millis((idx as u64 + 1) * 30);
        let action = gate.evaluate_at(step, t);
        assert_eq!(
            action,
            TextDiffAction::TypewriterWait,
            "Japanese incremental step {idx} ('{step}') must be TypewriterWait, got {:?}",
            action
        );
    }

    assert_eq!(
        gate.flush_pending(),
        Some("「ありがとうございます」".to_string())
    );

    // 2. Coarse jumps as presented in the prompt:
    // 「あ」 -> 「ありがとう」 -> 「ありがとうございます」
    // Note: 「あ」ends with closing bracket '」', so 「ありがとう」 is NOT a prefix expansion of 「あ」.
    // It should cleanly trigger as distinct dialogue turns or updates without jitter suppression!
    let mut coarse_gate = TextGate::new(Duration::from_millis(250), 0.92, 0.60);
    assert_eq!(
        coarse_gate.evaluate("「あ」"),
        TextDiffAction::EmitTranslation,
        "Initial Japanese text should emit"
    );

    // Jump from 「あ」 to 「ありがとう」:
    // Similarity is 3/7 = 0.42857 < 0.60 (turn_threshold)
    let act2 = coarse_gate.evaluate("「ありがとう」");
    assert_eq!(
        act2,
        TextDiffAction::EmitTranslation,
        "Jump from 「あ」 to 「ありがとう」 must emit translation (new dialogue text), got {:?}",
        act2
    );

    // Jump from 「ありがとう」 to 「ありがとうございます」:
    // Similarity is 7/12 = 0.58333 < 0.60 (turn_threshold)
    let act3 = coarse_gate.evaluate("「ありがとうございます」");
    assert_eq!(
        act3,
        TextDiffAction::EmitTranslation,
        "Jump from 「ありがとう」 to 「ありがとうございます」 must emit translation, got {:?}",
        act3
    );
}

#[test]
fn test_adversarial_1char_step_long_sentence_sim_above_098() {
    let mut gate = TextGate::new(Duration::from_millis(250), 0.92, 0.60);
    let t0 = Instant::now();

    // 70-character base sentence
    let base = "In the beginning the Universe was created. This made many very angry.";
    assert_eq!(gate.evaluate_at(base, t0), TextDiffAction::EmitTranslation);
    assert_eq!(gate.flush_pending(), None);

    // 1-character additions (e.g. extending with 'a'):
    // base len = 69. Adding 'a' makes len 70.
    // Levenshtein similarity = 69 / 70 = 0.9857 > 0.98!
    // Without prefix expansion checked first, this would be falsely suppressed as jitter (0.9857 >= 0.92).
    let ext1 = format!("{base}a");
    let sim1 = strsim::normalized_levenshtein(&ext1, base);
    assert!(sim1 > 0.98, "Similarity must be > 0.98, got {sim1}");

    let action1 = gate.evaluate_at(&ext1, t0 + Duration::from_millis(50));
    assert_eq!(
        action1,
        TextDiffAction::TypewriterWait,
        "1-char increment on long sentence (sim={sim1:.4} > 0.98) must NOT be suppressed as jitter!"
    );

    // Next 1-char addition: 'b'
    let ext2 = format!("{base}ab");
    let action2 = gate.evaluate_at(&ext2, t0 + Duration::from_millis(100));
    assert_eq!(
        action2,
        TextDiffAction::TypewriterWait,
        "2nd 1-char increment on long sentence must NOT be suppressed as jitter!"
    );

    // Flush
    assert_eq!(gate.flush_pending(), Some(ext2));
}

#[test]
fn test_adversarial_random_mutations_vs_jitter_vs_new_turns() {
    let mut gate = TextGate::new(Duration::from_millis(250), 0.92, 0.60);
    let base = "The quick brown fox jumps over the lazy dog near the riverbank.";
    assert_eq!(gate.evaluate(base), TextDiffAction::EmitTranslation);

    // Case 1: 1-character typo/OCR mutation in the middle (sim = 62/63 = 0.984 >= 0.92)
    // NOT a prefix expansion because middle character changed ("quick" -> "qu1ck")
    let mut middle_mutated = base.to_string();
    middle_mutated.replace_range(4..9, "qu1ck");
    let sim_mut = strsim::normalized_levenshtein(&middle_mutated, base);
    assert!(sim_mut >= 0.92, "Sim was {sim_mut}");
    assert_eq!(
        gate.evaluate(&middle_mutated),
        TextDiffAction::SuppressJitter,
        "Single-char OCR jitter in middle of sentence must be suppressed as jitter"
    );

    // Case 2: OCR punctuation drop at the end without prefix expansion (e.g. missing period)
    let no_period = &base[..base.len() - 1];
    let sim_no_period = strsim::normalized_levenshtein(no_period, base);
    assert!(sim_no_period >= 0.92);
    assert_eq!(
        gate.evaluate(no_period),
        TextDiffAction::SuppressJitter,
        "Dropped trailing punctuation must be suppressed as jitter"
    );

    // Case 3: Significant character changes (e.g. half the sentence replaced)
    let major_change = "The quick brown fox jumps into a deep dark cave in the forest.";
    let sim_major = strsim::normalized_levenshtein(major_change, base);
    assert!(sim_major < 0.92, "Sim was {sim_major}");
    assert_eq!(
        gate.evaluate(major_change),
        TextDiffAction::EmitTranslation,
        "Major sentence mutation must be accepted as new translation"
    );

    // Case 4: Completely distinct dialogue turn (sim < 0.60)
    let turn2 = "Farewell traveler, until we meet again at the castle gates!";
    let sim_turn2 = strsim::normalized_levenshtein(turn2, major_change);
    assert!(sim_turn2 < 0.60, "Sim was {sim_turn2}");
    assert_eq!(
        gate.evaluate(turn2),
        TextDiffAction::EmitTranslation,
        "Completely distinct turn must emit immediately"
    );
}

// ============================================================================
// 2. Roi::mask_region Adversarial Stress Tests
// ============================================================================

#[test]
fn test_adversarial_mask_region_extreme_bounds_and_overflow() {
    let white = Rgba([255, 255, 255, 255]);
    let black = Rgba([0, 0, 0, 255]);

    // Test 1: Coordinates near u32::MAX
    {
        let target = Roi::new(u32::MAX - 100, u32::MAX - 100, 50, 50);
        let ignore = Roi::new(u32::MAX - 80, u32::MAX - 80, 50, 50);
        let mut img = RgbaImage::from_pixel(50, 50, white);

        // Must not panic or cause arithmetic overflow
        target.mask_region(&mut img, &ignore);

        // Expected overlap in full frame:
        // inter_left = max(MAX-100, MAX-80) = MAX-80
        // inter_right = min(MAX-50, MAX-30) = MAX-50
        // local_x = (MAX-80) - (MAX-100) = 20 .. (MAX-50) - (MAX-100) = 50
        for y in 0..50 {
            for x in 0..50 {
                let p = *img.get_pixel(x, y);
                if (20..50).contains(&x) && (20..50).contains(&y) {
                    assert_eq!(p, black, "Pixel ({x}, {y}) should be masked black");
                } else {
                    assert_eq!(p, white, "Pixel ({x}, {y}) should be white");
                }
            }
        }
    }

    // Test 2: Extreme values near u32::MAX with addition overflow potential
    {
        // Target is at u32::MAX - 5, width 100.
        // saturating_add clamps target_right to u32::MAX.
        // Ignore is at u32::MAX - 10, width 200 (clamped to u32::MAX).
        // Overlap in u32 space is [u32::MAX - 5, u32::MAX] = 5 pixels wide.
        let target = Roi::new(u32::MAX - 5, u32::MAX - 5, 100, 100);
        let ignore = Roi::new(u32::MAX - 10, u32::MAX - 10, 200, 200);
        let mut img = RgbaImage::from_pixel(20, 20, white);

        // Must execute cleanly without overflow panics
        target.mask_region(&mut img, &ignore);

        // First 5x5 pixels are in bounds of the intersection; remaining are white
        for y in 0..20 {
            for x in 0..20 {
                let p = *img.get_pixel(x, y);
                if x < 5 && y < 5 {
                    assert_eq!(p, black);
                } else {
                    assert_eq!(p, white);
                }
            }
        }

        // Now test where target fully fits within u32::MAX
        let target_fit = Roi::new(u32::MAX - 20, u32::MAX - 20, 20, 20);
        let ignore_fit = Roi::new(u32::MAX - 25, u32::MAX - 25, 30, 30);
        let mut img_fit = RgbaImage::from_pixel(20, 20, white);
        target_fit.mask_region(&mut img_fit, &ignore_fit);
        assert!(
            img_fit.pixels().all(|p| *p == black),
            "Fully enclosed target near u32::MAX should be 100% masked"
        );
    }

    // Test 3: Zero-sized rectangles (width = 0 or height = 0 or both)
    {
        let target = Roi::new(10, 10, 50, 50);
        let zero_cases = [
            Roi::new(10, 10, 0, 50),
            Roi::new(10, 10, 50, 0),
            Roi::new(10, 10, 0, 0),
            Roi::new(0, 0, 0, 0),
            Roi::new(u32::MAX, u32::MAX, 0, 0),
        ];

        for (idx, z_roi) in zero_cases.iter().enumerate() {
            let mut img = RgbaImage::from_pixel(50, 50, white);
            target.mask_region(&mut img, z_roi);
            assert!(
                img.pixels().all(|p| *p == white),
                "Zero-sized ignore case {idx} should perform zero writes"
            );
        }

        // Target itself zero-sized
        let mut img = RgbaImage::from_pixel(50, 50, white);
        Roi::new(10, 10, 0, 50).mask_region(&mut img, &Roi::new(10, 10, 50, 50));
        assert!(img.pixels().all(|p| *p == white));
    }

    // Test 4: Thin 1-pixel strips
    {
        let target = Roi::new(100, 100, 50, 50);

        // 1-pixel vertical strip at x = 110
        let ignore_vert = Roi::new(110, 100, 1, 50);
        let mut img = RgbaImage::from_pixel(50, 50, white);
        target.mask_region(&mut img, &ignore_vert);

        let mut masked_count = 0;
        for y in 0..50 {
            for x in 0..50 {
                if *img.get_pixel(x, y) == black {
                    assert_eq!(x, 10, "Only column x=10 should be black");
                    masked_count += 1;
                }
            }
        }
        assert_eq!(masked_count, 50, "Expected exactly 50 masked pixels for 1x50 strip");

        // 1-pixel horizontal strip at y = 125
        let ignore_horiz = Roi::new(100, 125, 50, 1);
        let mut img2 = RgbaImage::from_pixel(50, 50, white);
        target.mask_region(&mut img2, &ignore_horiz);

        let mut masked_horiz_count = 0;
        for y in 0..50 {
            for x in 0..50 {
                if *img2.get_pixel(x, y) == black {
                    assert_eq!(y, 25, "Only row y=25 should be black");
                    masked_horiz_count += 1;
                }
            }
        }
        assert_eq!(masked_horiz_count, 50, "Expected exactly 50 masked pixels for 50x1 strip");

        // 1-pixel single pixel at (115, 115)
        let ignore_single = Roi::new(115, 115, 1, 1);
        let mut img3 = RgbaImage::from_pixel(50, 50, white);
        target.mask_region(&mut img3, &ignore_single);

        let black_count = img3.pixels().filter(|p| **p == black).count();
        assert_eq!(black_count, 1, "Expected exactly 1 masked pixel");
        assert_eq!(*img3.get_pixel(15, 15), black);
    }

    // Test 5: Overlapping all four corners
    {
        let target = Roi::new(100, 100, 40, 40);

        // Top-Left corner overlap
        let mut img_tl = RgbaImage::from_pixel(40, 40, white);
        target.mask_region(&mut img_tl, &Roi::new(90, 90, 20, 20));
        // Overlap: [100..110) x [100..110) -> local [0..10) x [0..10)
        assert_eq!(img_tl.pixels().filter(|p| **p == black).count(), 100);
        assert_eq!(*img_tl.get_pixel(0, 0), black);
        assert_eq!(*img_tl.get_pixel(9, 9), black);
        assert_eq!(*img_tl.get_pixel(10, 10), white);

        // Top-Right corner overlap
        let mut img_tr = RgbaImage::from_pixel(40, 40, white);
        target.mask_region(&mut img_tr, &Roi::new(130, 90, 20, 20));
        // Overlap: [130..140) x [100..110) -> local [30..40) x [0..10)
        assert_eq!(img_tr.pixels().filter(|p| **p == black).count(), 100);
        assert_eq!(*img_tr.get_pixel(30, 0), black);
        assert_eq!(*img_tr.get_pixel(39, 9), black);
        assert_eq!(*img_tr.get_pixel(29, 0), white);

        // Bottom-Left corner overlap
        let mut img_bl = RgbaImage::from_pixel(40, 40, white);
        target.mask_region(&mut img_bl, &Roi::new(90, 130, 20, 20));
        // Overlap: [100..110) x [130..140) -> local [0..10) x [30..40)
        assert_eq!(img_bl.pixels().filter(|p| **p == black).count(), 100);
        assert_eq!(*img_bl.get_pixel(0, 30), black);
        assert_eq!(*img_bl.get_pixel(9, 39), black);
        assert_eq!(*img_bl.get_pixel(0, 29), white);

        // Bottom-Right corner overlap
        let mut img_br = RgbaImage::from_pixel(40, 40, white);
        target.mask_region(&mut img_br, &Roi::new(130, 130, 20, 20));
        // Overlap: [130..140) x [130..140) -> local [30..40) x [30..40)
        assert_eq!(img_br.pixels().filter(|p| **p == black).count(), 100);
        assert_eq!(*img_br.get_pixel(30, 30), black);
        assert_eq!(*img_br.get_pixel(39, 39), black);
        assert_eq!(*img_br.get_pixel(29, 29), white);
    }

    // Test 6: Ignore region far larger than the canvas
    {
        let target = Roi::new(500, 500, 100, 100);
        let gigantic_ignore = Roi::new(0, 0, u32::MAX, u32::MAX);
        let mut img = RgbaImage::from_pixel(100, 100, white);

        target.mask_region(&mut img, &gigantic_ignore);
        assert!(
            img.pixels().all(|p| *p == black),
            "Ignore larger than canvas should mask entire target ROI without panics"
        );
    }

    // Test 7: Buffer dimensions smaller than target ROI width/height
    {
        let target = Roi::new(0, 0, 1000, 1000);
        let ignore = Roi::new(0, 0, 500, 500);
        // Image buffer is only 50x50
        let mut img = RgbaImage::from_pixel(50, 50, white);

        target.mask_region(&mut img, &ignore);
        // Must clamp to img dimensions (50x50) and not panic out of bounds
        assert!(img.pixels().all(|p| *p == black));
    }
}

// ============================================================================
// 3. Generative Fuzzing & Randomized Stress Tests
// ============================================================================

#[test]
fn test_adversarial_mask_region_random_fuzzing() {
    let white = Rgba([255, 255, 255, 255]);
    let black = Rgba([0, 0, 0, 255]);

    // Pseudo-random linear congruential generator for reproducible deterministic fuzzing
    let mut state: u64 = 0xDEADBEEFCAFEBABE;
    let mut next_u32 = || -> u32 {
        state = state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (state >> 32) as u32
    };

    // Extreme coordinate candidate values to mix with random values
    let extreme_coords = [
        0,
        1,
        2,
        50,
        100,
        u32::MAX / 2,
        u32::MAX - 1000,
        u32::MAX - 50,
        u32::MAX - 1,
        u32::MAX,
    ];

    for iteration in 0..5_000 {
        let pick_coord = |r: u32| -> u32 {
            if r.is_multiple_of(3) {
                extreme_coords[(r as usize / 3) % extreme_coords.len()]
            } else {
                r
            }
        };

        let tx = pick_coord(next_u32());
        let ty = pick_coord(next_u32());
        let tw = pick_coord(next_u32());
        let th = pick_coord(next_u32());

        let ix = pick_coord(next_u32());
        let iy = pick_coord(next_u32());
        let iw = pick_coord(next_u32());
        let ih = pick_coord(next_u32());

        let target = Roi::new(tx, ty, tw, th);
        let ignore = Roi::new(ix, iy, iw, ih);

        // Frame dimensions between 0 and 64
        let img_w = next_u32() % 65;
        let img_h = next_u32() % 65;

        let mut img = RgbaImage::from_pixel(img_w, img_h, white);

        // Verification requirement: ZERO panics, out-of-bounds writes, or arithmetic overflows
        target.mask_region(&mut img, &ignore);

        // Verify that only black or white pixels exist in the buffer (no garbage/memory corruption)
        for p in img.pixels() {
            assert!(
                *p == white || *p == black,
                "Fuzz iteration {iteration}: pixel corrupted to {:?}",
                p
            );
        }
    }
}

#[test]
fn test_adversarial_text_gate_unicode_and_fuzzing() {
    let mut gate = TextGate::new(Duration::from_millis(250), 0.92, 0.60);

    // Exotic Unicode strings: CJK, Arabic RTL, Emojis, combining diacritics, zero-width chars
    let exotic_strings = [
        "",
        "   ",
        "\t\n  \r\n",
        "\u{3000}\u{3000}", // Fullwidth Japanese whitespace
        "こんにちは",
        "こんにちは世界",
        "مرحبا بالعالم",
        "مرحبا بك في اللعبة",
        "🎮 Level 1: Start Quest! 🗡️",
        "🎮 Level 1: Start Quest! 🗡️ Shield equipped.",
        "Z̸a̷l̶g̶o̶ ̴t̷e̷x̸t̷ ̷t̸e̶s̵t̵",
        "Z̸a̷l̶g̶o̶ ̴t̷e̷x̸t̷ ̷t̸e̶s̵t̵ extended",
    ];

    let t0 = Instant::now();
    for (i, s) in exotic_strings.iter().enumerate() {
        let now = t0 + Duration::from_millis(i as u64 * 50);
        // Must never panic on multibyte slicing or unusual codepoints
        let action = gate.evaluate_at(s, now);
        let _ = gate.has_pending_expansion();
        if i.is_multiple_of(3) {
            let _ = gate.flush_pending();
        }
        let _ = format!("{:?}", action);
    }

    // Reset works cleanly
    gate.reset();
    assert_eq!(gate.last_emitted_text(), "");
    assert_eq!(gate.last_candidate_text(), "");
    assert!(!gate.has_pending_expansion());
}

#[test]
fn test_adversarial_typewriter_resume_after_flush() {
    let mut gate = TextGate::new(Duration::from_millis(250), 0.92, 0.60);
    let t0 = Instant::now();

    // Turn 1 starts typing
    assert_eq!(gate.evaluate_at("First sentence part 1", t0), TextDiffAction::EmitTranslation);
    assert_eq!(
        gate.evaluate_at("First sentence part 1 and part 2", t0 + Duration::from_millis(50)),
        TextDiffAction::TypewriterWait
    );

    // Settle timeout expires, flushed to HUD
    let flushed = gate.flush_pending();
    assert_eq!(flushed, Some("First sentence part 1 and part 2".to_string()));
    assert_eq!(gate.last_emitted_text(), "First sentence part 1 and part 2");

    // Game appends more text to the SAME dialogue without clearing:
    // (prefix expansion against last_emitted_text!)
    let extended = "First sentence part 1 and part 2 and bonus part 3!";
    let action_resumed = gate.evaluate_at(extended, t0 + Duration::from_millis(400));
    assert_eq!(
        action_resumed,
        TextDiffAction::TypewriterWait,
        "Extension of settled text must trigger TypewriterWait"
    );

    assert_eq!(gate.flush_pending(), Some(extended.to_string()));
}

