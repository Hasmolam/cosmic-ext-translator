use translator_core::ocr::{filter_furigana, DetectedTextBox, TextOrientation};
use translator_core::region::Roi;

#[test]
fn test_all_protected_punctuation_characters_in_furigana() {
    let puncts = ["。", "、", "！", "？", "…", "・", ".", ",", "!", "?"];
    for &p in &puncts {
        // Main text box
        let main = DetectedTextBox::new(Roi::new(50, 50, 40, 100), "漢字", 0.95);
        // Small box with punctuation inside ruby lane
        let p_box = DetectedTextBox::new(Roi::new(92, 60, 8, 8), p, 0.90);

        let filtered =
            filter_furigana(vec![main.clone(), p_box.clone()], TextOrientation::Vertical);
        assert_eq!(
            filtered.len(),
            2,
            "Punctuation {p} must be protected from ruby pruning"
        );
        assert_eq!(filtered[1].text, p);
    }
}

#[test]
fn test_isolated_small_text_not_pruned_as_furigana() {
    // If a small box is NOT adjacent to any primary text (e.g. far away page number at corner)
    let main = DetectedTextBox::new(Roi::new(100, 100, 50, 150), "本文", 0.95);
    // Page number far away at X=10, Y=10 (nowhere near main column at X=100)
    let page_num = DetectedTextBox::new(Roi::new(10, 10, 12, 12), "1", 0.90);

    let filtered = filter_furigana(vec![main, page_num.clone()], TextOrientation::Vertical);
    assert_eq!(
        filtered.len(),
        2,
        "Isolated small box must NOT be pruned if it is not adjacent to primary text"
    );
    assert_eq!(filtered[1].text, "1");
}

#[test]
fn test_uniform_small_boxes_no_pruning() {
    // If all boxes in the crop are small (e.g. distant HUD labels), none should be pruned
    let b1 = DetectedTextBox::new(Roi::new(10, 10, 10, 10), "a", 0.9);
    let b2 = DetectedTextBox::new(Roi::new(25, 10, 10, 10), "b", 0.9);
    let b3 = DetectedTextBox::new(Roi::new(40, 10, 10, 10), "c", 0.9);

    let filtered = filter_furigana(vec![b1, b2, b3], TextOrientation::Horizontal);
    assert_eq!(
        filtered.len(),
        3,
        "Uniform small boxes must all be preserved"
    );
}

#[test]
fn test_adversarial_furigana_ruby_positions_vertical() {
    // 5 Main kanji columns: X=30, 100, 200, 300, 400 (W=40, H=120, area=4800)
    // Ensures median_area is firmly anchored to 4800 and median_width=40
    let m1 = DetectedTextBox::new(Roi::new(100, 50, 40, 120), "漢字一", 0.95);
    let m2 = DetectedTextBox::new(Roi::new(30, 50, 40, 120), "漢字二", 0.95);
    let m3 = DetectedTextBox::new(Roi::new(200, 50, 40, 120), "漢字三", 0.95);
    let m4 = DetectedTextBox::new(Roi::new(300, 50, 40, 120), "漢字四", 0.95);
    let m5 = DetectedTextBox::new(Roi::new(400, 50, 40, 120), "漢字五", 0.95);

    // Position 1: Ruby directly to the right of m1 (X=141..153, Y=60..120, W=12, H=60, area=720)
    // dx = 141 - 140 = 1 <= max_gap (0.8 * 40 = 32). Inside ruby lane -> PRUNED
    let ruby_right = DetectedTextBox::new(Roi::new(141, 60, 12, 60), "かんじ", 0.9);

    // Position 2: Small text to the LEFT of all columns (X=10..22, Y=60..120, W=12, H=60, area=720)
    // center_x = 16 < p_center_x of m2 (50). Left of all columns -> PRESERVED
    let small_left = DetectedTextBox::new(Roi::new(10, 60, 12, 60), "左注釈", 0.9);

    // Position 3: Small text far to the right of m1 (X=185..197, Y=60..120, W=12, H=60, area=720)
    // dx = 185 - 140 = 45 > max_gap (32). Outside ruby lane -> PRESERVED
    let small_far_right = DetectedTextBox::new(Roi::new(185, 60, 12, 60), "遠右記号", 0.9);

    // Position 4: Small text directly above column m1 (X=110..122, Y=10..30, W=12, H=20, area=240)
    // vert_overlap = 0. Outside vertical ruby lane -> PRESERVED
    let small_above = DetectedTextBox::new(Roi::new(110, 10, 12, 20), "上話者", 0.9);

    // Position 5: Small text directly below column m1 (X=110..122, Y=180..200, W=12, H=20, area=240)
    // vert_overlap = 0. Outside vertical ruby lane -> PRESERVED
    let small_below = DetectedTextBox::new(Roi::new(110, 180, 12, 20), "下話者", 0.9);

    // Areas: 5x 4800, 3x 720, 2x 240. Total 10 boxes.
    // Sorted areas: [240, 240, 720, 720, 720, 4800, 4800, 4800, 4800, 4800]
    // Median area is areas[5] = 4800. Threshold = 4800 * 0.35 = 1680.
    // All 5 small boxes have area < 1680.
    let boxes = vec![
        m1,
        m2,
        m3,
        m4,
        m5,
        ruby_right,
        small_left,
        small_far_right,
        small_above,
        small_below,
    ];

    let filtered = filter_furigana(boxes, TextOrientation::Vertical);
    let texts: Vec<&str> = filtered.iter().map(|b| b.text.as_str()).collect();

    // Only ruby_right ("かんじ") should be pruned. All other small elements must be preserved.
    assert_eq!(
        texts,
        vec![
            "漢字一",
            "漢字二",
            "漢字三",
            "漢字四",
            "漢字五",
            "左注釈",
            "遠右記号",
            "上話者",
            "下話者"
        ]
    );
}

#[test]
fn test_adversarial_furigana_ruby_positions_horizontal() {
    // 5 Main horizontal lines: Y=100, 180, 260, 340, 420 (W=200, H=40, area=8000)
    let m1 = DetectedTextBox::new(Roi::new(50, 100, 200, 40), "日本語の文章一", 0.95);
    let m2 = DetectedTextBox::new(Roi::new(50, 180, 200, 40), "日本語の文章二", 0.95);
    let m3 = DetectedTextBox::new(Roi::new(50, 260, 200, 40), "日本語の文章三", 0.95);
    let m4 = DetectedTextBox::new(Roi::new(50, 340, 200, 40), "日本語の文章四", 0.95);
    let m5 = DetectedTextBox::new(Roi::new(50, 420, 200, 40), "日本語の文章五", 0.95);

    // Position 1: Ruby directly above m1 (X=60..140, Y=85..97, W=80, H=12, area=960)
    // dy = 100 - 97 = 3 <= max_gap (0.8 * 40 = 32). In ruby lane -> PRUNED
    let ruby_above = DetectedTextBox::new(Roi::new(60, 85, 80, 12), "にほんご", 0.9);

    // Position 2: Small text directly below all lines (X=60..140, Y=480..492, W=80, H=12, area=960)
    // Below line m5 (Y=420..460), not above any line -> PRESERVED
    let small_below = DetectedTextBox::new(Roi::new(60, 480, 80, 12), "下注釈", 0.9);

    // Position 3: Small text far above m1 (X=60..140, Y=20..32, W=80, H=12, area=960)
    // dy = 100 - 32 = 68 > max_gap (32). Too high above -> PRESERVED
    let small_far_above = DetectedTextBox::new(Roi::new(60, 20, 80, 12), "上見出し", 0.9);

    // Position 4: Small text to the left of m1 (X=10..40, Y=105..125, W=30, H=20, area=600)
    // horiz_overlap = 0 -> PRESERVED
    let small_left = DetectedTextBox::new(Roi::new(10, 105, 30, 20), "左印", 0.9);

    // Position 5: Small text to the right of m1 (X=260..290, Y=105..125, W=30, H=20, area=600)
    // horiz_overlap = 0 -> PRESERVED
    let small_right = DetectedTextBox::new(Roi::new(260, 105, 30, 20), "右印", 0.9);

    // Areas: 5x 8000, 3x 960, 2x 600.
    // Median is 8000, threshold = 8000 * 0.35 = 2800.
    // All 5 small boxes have area < 2800.
    let boxes = vec![
        m1,
        m2,
        m3,
        m4,
        m5,
        ruby_above,
        small_below,
        small_far_above,
        small_left,
        small_right,
    ];

    let filtered = filter_furigana(boxes, TextOrientation::Horizontal);
    let texts: Vec<&str> = filtered.iter().map(|b| b.text.as_str()).collect();

    // Only ruby_above ("にほんご") should be pruned. All others must be preserved.
    assert_eq!(
        texts,
        vec![
            "日本語の文章一",
            "日本語の文章二",
            "日本語の文章三",
            "日本語の文章四",
            "日本語の文章五",
            "下注釈",
            "上見出し",
            "左印",
            "右印"
        ]
    );
}

#[test]
fn test_adversarial_furigana_all_punctuation_exhaustive_and_tiny_boxes() {
    let punct_marks = [
        "。", "、", "！", "？", "…", "・", ".", ",", "!", "?", "！？", "？！", "……", "  。  ",
    ];

    for orientation in [TextOrientation::Vertical, TextOrientation::Horizontal] {
        for &p in &punct_marks {
            let main1 = DetectedTextBox::new(Roi::new(100, 100, 50, 100), "主文章", 0.95);
            let main2 = DetectedTextBox::new(Roi::new(20, 100, 50, 100), "副文章", 0.95);

            // Place punctuation directly in the candidate ruby lane with a tiny 1x1 bounding box
            let punct_box = match orientation {
                TextOrientation::Vertical => {
                    // Right flank of main1: X=151, Y=110, W=1, H=1 (area = 1)
                    DetectedTextBox::new(Roi::new(151, 110, 1, 1), p, 0.95)
                }
                TextOrientation::Horizontal => {
                    // Above main1: X=110, Y=98, W=1, H=1 (area = 1)
                    DetectedTextBox::new(Roi::new(110, 98, 1, 1), p, 0.95)
                }
            };

            let boxes = vec![main1.clone(), main2.clone(), punct_box.clone()];
            let filtered = filter_furigana(boxes, orientation);

            assert_eq!(
                filtered.len(),
                3,
                "Punctuation '{p}' with tiny 1x1 box in ruby lane must NEVER be pruned (orientation: {orientation:?})"
            );
            assert_eq!(filtered[2].text, p);
        }
    }
}

#[test]
fn test_adversarial_furigana_non_ruby_isolated_standalone_numbers_and_icons() {
    let main1 = DetectedTextBox::new(Roi::new(100, 100, 60, 120), "キャラクター", 0.95);
    let main2 = DetectedTextBox::new(Roi::new(20, 100, 60, 120), "ステータス", 0.95);

    // Standalone numbers and symbols far away in screen corners or separate UI areas
    let num_lv = DetectedTextBox::new(Roi::new(5, 5, 15, 10), "99", 0.9);
    let icon_star = DetectedTextBox::new(Roi::new(300, 5, 10, 10), "★", 0.9);
    let icon_arrow = DetectedTextBox::new(Roi::new(5, 300, 10, 10), "▶", 0.9);

    let boxes = vec![
        main1,
        main2,
        num_lv.clone(),
        icon_star.clone(),
        icon_arrow.clone(),
    ];

    let filtered = filter_furigana(boxes, TextOrientation::Vertical);
    assert_eq!(
        filtered.len(),
        5,
        "All isolated small non-ruby elements must be preserved"
    );

    let texts: Vec<&str> = filtered.iter().map(|b| b.text.as_str()).collect();
    assert!(texts.contains(&"99"));
    assert!(texts.contains(&"★"));
    assert!(texts.contains(&"▶"));
}

#[test]
fn test_adversarial_furigana_majority_ruby_boxes_heuristic_behavior() {
    // When ruby boxes outnumber primary kanji boxes:
    // e.g. 1 kanji box (4000 area) + 2 small ruby boxes (400 area)
    let kanji = DetectedTextBox::new(Roi::new(100, 100, 40, 100), "魔法", 0.95);
    let ruby1 = DetectedTextBox::new(Roi::new(141, 105, 10, 40), "ま", 0.9);
    let ruby2 = DetectedTextBox::new(Roi::new(141, 150, 10, 40), "ほう", 0.9);

    let boxes = vec![kanji.clone(), ruby1.clone(), ruby2.clone()];
    let filtered = filter_furigana(boxes, TextOrientation::Vertical);

    // Median of [400, 400, 4000] is 400.
    // area_threshold = 400 * 0.35 = 140.
    // Both ruby1 and ruby2 have area 400 >= 140, so they are not categorized as small candidate boxes.
    // This demonstrates the documented heuristic boundary when ruby boxes dominate the crop.
    assert_eq!(filtered.len(), 3);
}

#[test]
fn test_adversarial_furigana_empty_and_zero_area_inputs() {
    // 0 boxes
    assert_eq!(
        filter_furigana(vec![], TextOrientation::Horizontal).len(),
        0
    );

    // 1 box
    let single = DetectedTextBox::new(Roi::new(10, 10, 50, 50), "一", 0.9);
    assert_eq!(
        filter_furigana(vec![single.clone()], TextOrientation::Vertical).len(),
        1
    );

    // Zero-width / zero-height boxes should not panic
    let zero_w = DetectedTextBox::new(Roi::new(10, 10, 0, 50), "zw", 0.9);
    let zero_h = DetectedTextBox::new(Roi::new(10, 10, 50, 0), "zh", 0.9);
    let res = filter_furigana(vec![single, zero_w, zero_h], TextOrientation::Horizontal);
    assert!(!res.is_empty());
}

#[test]
fn test_adversarial_furigana_integer_overflow_panic_near_u32_max_vertical() {
    // Primary box placed near u32::MAX on X axis
    // When calculating p_right = p_rect.x.saturating_add(p_rect.width), execution completes without panic
    let p_box = DetectedTextBox::new(Roi::new(u32::MAX - 8, 100, 10, 100), "主テキスト", 0.95);
    let small_box = DetectedTextBox::new(Roi::new(u32::MAX - 2, 110, 2, 20), "ふりがな", 0.90);

    // Clean execution without panicking; ruby annotation is pruned
    let res = filter_furigana(vec![p_box, small_box], TextOrientation::Vertical);
    assert_eq!(res.len(), 1);
    assert_eq!(res[0].text, "主テキスト");
}

#[test]
fn test_adversarial_furigana_integer_overflow_panic_near_u32_max_horizontal() {
    // Candidate box placed near u32::MAX on Y axis
    // When calculating b_bottom = (b.rect.y.saturating_add(b.rect.height)) as i64, execution completes without panic
    let p_box = DetectedTextBox::new(Roi::new(100, u32::MAX - 2, 1000, 2), "主テキスト", 0.95);
    let small_box = DetectedTextBox::new(Roi::new(100, u32::MAX - 8, 10, 10), "ふりがな", 0.90);

    // Clean execution without panicking
    let res = filter_furigana(vec![p_box, small_box], TextOrientation::Horizontal);
    assert!(!res.is_empty());
}
