use image::{DynamicImage, RgbaImage};
use translator_core::ocr::{
    detect_dominant_orientation, is_cjk_text, join_cjk_or_latin_tokens, sort_cjk_reading_order,
    DetectedTextBox, MockOcrBackend, OcrBackend, TextOrientation,
};
use translator_core::region::Roi;

#[test]
fn test_adversarial_layout_vertical_tategaki_6_columns_staggered_jitter() {
    // 6 vertical columns in Japanese tategaki layout:
    // Columns read right-to-left: Col 1 -> Col 2 -> Col 3 -> Col 4 -> Col 5 -> Col 6
    // Within each column, read top-to-bottom: Y ascending
    //
    // Staggered heights: characters have varying heights (18, 20, 24, 28, 30, 36)
    // Horizontal jitter: ±1..2 px offset on X coordinates
    // Different reading lengths: 4, 7, 2, 8, 3, 5 tokens per column

    let mut expected_order = Vec::new();
    let mut scrambled = Vec::new();

    // Column 1 (Rightmost): nominal X = 500, width = 30. 4 tokens.
    let col1 = vec![
        DetectedTextBox::new(Roi::new(499, 10, 30, 24), "C1_T1", 0.95),
        DetectedTextBox::new(Roi::new(501, 38, 30, 30), "C1_T2", 0.95),
        DetectedTextBox::new(Roi::new(500, 72, 30, 18), "C1_T3", 0.95),
        DetectedTextBox::new(Roi::new(498, 94, 30, 36), "C1_T4", 0.95),
    ];

    // Column 2: nominal X = 420, width = 28. 7 tokens.
    let col2 = vec![
        DetectedTextBox::new(Roi::new(420, 12, 28, 20), "C2_T1", 0.95),
        DetectedTextBox::new(Roi::new(421, 36, 28, 28), "C2_T2", 0.95),
        DetectedTextBox::new(Roi::new(419, 68, 28, 22), "C2_T3", 0.95),
        DetectedTextBox::new(Roi::new(420, 94, 28, 30), "C2_T4", 0.95),
        DetectedTextBox::new(Roi::new(422, 128, 28, 18), "C2_T5", 0.95),
        DetectedTextBox::new(Roi::new(419, 150, 28, 24), "C2_T6", 0.95),
        DetectedTextBox::new(Roi::new(420, 178, 28, 26), "C2_T7", 0.95),
    ];

    // Column 3: nominal X = 340, width = 32. 2 tokens.
    let col3 = vec![
        DetectedTextBox::new(Roi::new(339, 15, 32, 35), "C3_T1", 0.95),
        DetectedTextBox::new(Roi::new(341, 55, 32, 35), "C3_T2", 0.95),
    ];

    // Column 4: nominal X = 260, width = 30. 8 tokens.
    let col4 = vec![
        DetectedTextBox::new(Roi::new(260, 5, 30, 25), "C4_T1", 0.95),
        DetectedTextBox::new(Roi::new(259, 34, 30, 25), "C4_T2", 0.95),
        DetectedTextBox::new(Roi::new(261, 63, 30, 30), "C4_T3", 0.95),
        DetectedTextBox::new(Roi::new(260, 97, 30, 20), "C4_T4", 0.95),
        DetectedTextBox::new(Roi::new(258, 121, 30, 28), "C4_T5", 0.95),
        DetectedTextBox::new(Roi::new(261, 153, 30, 32), "C4_T6", 0.95),
        DetectedTextBox::new(Roi::new(260, 189, 30, 22), "C4_T7", 0.95),
        DetectedTextBox::new(Roi::new(259, 215, 30, 25), "C4_T8", 0.95),
    ];

    // Column 5: nominal X = 180, width = 29. 3 tokens.
    let col5 = vec![
        DetectedTextBox::new(Roi::new(181, 20, 29, 30), "C5_T1", 0.95),
        DetectedTextBox::new(Roi::new(179, 54, 29, 30), "C5_T2", 0.95),
        DetectedTextBox::new(Roi::new(180, 88, 29, 30), "C5_T3", 0.95),
    ];

    // Column 6 (Leftmost): nominal X = 100, width = 31. 5 tokens.
    let col6 = vec![
        DetectedTextBox::new(Roi::new(100, 8, 31, 26), "C6_T1", 0.95),
        DetectedTextBox::new(Roi::new(99, 38, 31, 28), "C6_T2", 0.95),
        DetectedTextBox::new(Roi::new(101, 70, 31, 24), "C6_T3", 0.95),
        DetectedTextBox::new(Roi::new(100, 98, 31, 32), "C6_T4", 0.95),
        DetectedTextBox::new(Roi::new(99, 134, 31, 30), "C6_T5", 0.95),
    ];

    for col in [&col1, &col2, &col3, &col4, &col5, &col6] {
        for b in col {
            expected_order.push(b.text.clone());
        }
    }

    scrambled.extend(col4.clone());
    scrambled.extend(col1.clone());
    scrambled.extend(col6.clone());
    scrambled.extend(col2.clone());
    scrambled.extend(col5.clone());
    scrambled.extend(col3.clone());
    scrambled.reverse();

    assert_eq!(scrambled.len(), 29);

    let sorted = sort_cjk_reading_order(scrambled.clone(), TextOrientation::Vertical);
    let actual_order: Vec<String> = sorted.iter().map(|b| b.text.clone()).collect();

    assert_eq!(
        actual_order, expected_order,
        "6-column tategaki sorting failed to maintain strictly right-to-left and top-to-bottom ordering"
    );

    let resorted = sort_cjk_reading_order(sorted.clone(), TextOrientation::Vertical);
    assert_eq!(sorted, resorted, "Sorting must be idempotent");
}

#[test]
fn test_adversarial_layout_boundary_overlaps_vertical_mode() {
    let base = DetectedTextBox::new(Roi::new(100, 10, 100, 20), "Base", 0.9);

    // Case 1: 49% overlap (< 50%) -> Must NOT cluster into the same column
    let box_49 = DetectedTextBox::new(Roi::new(151, 40, 100, 20), "Box_49", 0.9);
    let sorted_49 = sort_cjk_reading_order(
        vec![base.clone(), box_49.clone()],
        TextOrientation::Vertical,
    );
    let texts_49: Vec<&str> = sorted_49.iter().map(|b| b.text.as_str()).collect();
    assert_eq!(
        texts_49,
        vec!["Box_49", "Base"],
        "49% horizontal overlap must NOT cluster into same column"
    );

    // Case 2: 50% overlap (exact threshold >= 50%) -> MUST cluster into the same column
    let box_50 = DetectedTextBox::new(Roi::new(150, 40, 100, 20), "Box_50", 0.9);
    let sorted_50 = sort_cjk_reading_order(
        vec![base.clone(), box_50.clone()],
        TextOrientation::Vertical,
    );
    let texts_50: Vec<&str> = sorted_50.iter().map(|b| b.text.as_str()).collect();
    assert_eq!(
        texts_50,
        vec!["Base", "Box_50"],
        "50% horizontal overlap MUST cluster into same column"
    );

    // Case 3: 51% overlap (> 50%) -> MUST cluster into the same column
    let box_51 = DetectedTextBox::new(Roi::new(149, 40, 100, 20), "Box_51", 0.9);
    let sorted_51 = sort_cjk_reading_order(
        vec![base.clone(), box_51.clone()],
        TextOrientation::Vertical,
    );
    let texts_51: Vec<&str> = sorted_51.iter().map(|b| b.text.as_str()).collect();
    assert_eq!(
        texts_51,
        vec!["Base", "Box_51"],
        "51% horizontal overlap MUST cluster into same column"
    );
}

#[test]
fn test_adversarial_layout_boundary_overlaps_horizontal_mode() {
    let base = DetectedTextBox::new(Roi::new(10, 100, 20, 100), "Base", 0.9);

    // Case 1: 49% overlap (< 50%) -> Must NOT cluster into the same line
    let box_49 = DetectedTextBox::new(Roi::new(40, 151, 20, 100), "Box_49", 0.9);
    let sorted_49 = sort_cjk_reading_order(
        vec![base.clone(), box_49.clone()],
        TextOrientation::Horizontal,
    );
    let texts_49: Vec<&str> = sorted_49.iter().map(|b| b.text.as_str()).collect();
    assert_eq!(
        texts_49,
        vec!["Base", "Box_49"],
        "49% vertical overlap must form distinct lines"
    );

    // Case 2: 50% overlap (exact threshold >= 50%) -> MUST cluster into same line
    let box_50 = DetectedTextBox::new(Roi::new(40, 150, 20, 100), "Box_50", 0.9);
    let sorted_50 = sort_cjk_reading_order(
        vec![base.clone(), box_50.clone()],
        TextOrientation::Horizontal,
    );
    let texts_50: Vec<&str> = sorted_50.iter().map(|b| b.text.as_str()).collect();
    assert_eq!(
        texts_50,
        vec!["Base", "Box_50"],
        "50% vertical overlap MUST cluster into same line"
    );

    // Case 3: 51% overlap (> 50%) -> MUST cluster into same line
    let box_51 = DetectedTextBox::new(Roi::new(40, 149, 20, 100), "Box_51", 0.9);
    let sorted_51 = sort_cjk_reading_order(
        vec![base.clone(), box_51.clone()],
        TextOrientation::Horizontal,
    );
    let texts_51: Vec<&str> = sorted_51.iter().map(|b| b.text.as_str()).collect();
    assert_eq!(
        texts_51,
        vec!["Base", "Box_51"],
        "51% vertical overlap MUST cluster into same line"
    );
}

#[test]
fn test_adversarial_layout_mixed_orientations_voting() {
    let title = DetectedTextBox::new(Roi::new(50, 10, 400, 20), "Title", 0.95);
    let v_col1 = DetectedTextBox::new(Roi::new(350, 50, 30, 200), "V1", 0.95);
    let v_col2 = DetectedTextBox::new(Roi::new(250, 50, 30, 200), "V2", 0.95);
    let v_col3 = DetectedTextBox::new(Roi::new(150, 50, 30, 200), "V3", 0.95);

    let boxes = vec![title, v_col1, v_col2, v_col3];
    assert_eq!(
        detect_dominant_orientation(&boxes),
        TextOrientation::Vertical
    );

    let h1 = DetectedTextBox::new(Roi::new(50, 10, 300, 25), "H1", 0.95);
    let h2 = DetectedTextBox::new(Roi::new(50, 50, 300, 25), "H2", 0.95);
    let h3 = DetectedTextBox::new(Roi::new(50, 90, 300, 25), "H3", 0.95);
    let v_sidebar = DetectedTextBox::new(Roi::new(400, 10, 30, 200), "VSide", 0.95);

    let boxes_h = vec![h1, h2, h3, v_sidebar];
    assert_eq!(
        detect_dominant_orientation(&boxes_h),
        TextOrientation::Horizontal
    );
}

#[test]
fn test_adversarial_layout_mixed_orientation_line_absorption_anomaly() {
    // Challenge Finding:
    // When a tall vertical box (e.g. sidebar or VN character label) is present in
    // horizontal reading mode, `overlap / min(h1, h2) >= 0.5` causes the tall box
    // to absorb multiple horizontal lines into a single line.
    let l1_w1 = DetectedTextBox::new(Roi::new(10, 20, 40, 20), "L1_W1", 0.9);
    let l1_w2 = DetectedTextBox::new(Roi::new(60, 20, 40, 20), "L1_W2", 0.9);
    let l2_w1 = DetectedTextBox::new(Roi::new(10, 60, 40, 20), "L2_W1", 0.9);
    let l2_w2 = DetectedTextBox::new(Roi::new(60, 60, 40, 20), "L2_W2", 0.9);
    let l3_w1 = DetectedTextBox::new(Roi::new(10, 100, 40, 20), "L3_W1", 0.9);
    let l3_w2 = DetectedTextBox::new(Roi::new(60, 100, 40, 20), "L3_W2", 0.9);

    let v1 = DetectedTextBox::new(Roi::new(200, 10, 30, 120), "V1", 0.9);
    let boxes = vec![v1, l1_w1, l1_w2, l2_w1, l2_w2, l3_w1, l3_w2];

    let sorted = sort_cjk_reading_order(boxes, TextOrientation::Horizontal);
    let texts: Vec<&str> = sorted.iter().map(|b| b.text.as_str()).collect();

    // Empirically demonstrates that all 3 lines get merged with V1 and sorted column-major:
    assert_eq!(
        texts,
        vec!["L1_W1", "L2_W1", "L3_W1", "L1_W2", "L2_W2", "L3_W2", "V1"],
        "Demonstrates line absorption flaw due to min_h metric in mixed orientation"
    );
}

#[test]
fn test_adversarial_layout_integer_overflow_panic_near_u32_max() {
    // Verified fix:
    // In `crates/translator-core/src/ocr/layout.rs`:
    // Coordinate additions use `saturating_add` instead of raw `+`.
    // Coordinates near u32::MAX succeed without panic.
    let b1 = DetectedTextBox::new(Roi::new(u32::MAX - 5, 10, 10, 20), "Panic1", 0.9);
    let b2 = DetectedTextBox::new(Roi::new(u32::MAX - 10, 10, 20, 20), "Panic2", 0.9);

    let sorted = sort_cjk_reading_order(vec![b1, b2], TextOrientation::Vertical);
    assert_eq!(
        sorted.len(),
        2,
        "Layout sorting near u32::MAX must succeed without overflow panic"
    );
}

#[test]
fn test_adversarial_layout_vertical_sorting_non_idempotence_flaw() {
    // Verified fix:
    // Full tie-breakers in sorting comparators guarantee strict idempotence.
    let box_a = DetectedTextBox::new(Roi::new(427, 0, 24, 192), "BoxA", 0.97);
    let box_b = DetectedTextBox::new(Roi::new(450, 0, 1, 59), "BoxB", 0.76);
    let box_c = DetectedTextBox::new(Roi::new(260, 539, 181, 26), "BoxC", 0.75);
    let box_d = DetectedTextBox::new(Roi::new(370, 310, 1, 16), "BoxD", 0.50);

    let input = vec![box_b.clone(), box_a.clone(), box_c.clone(), box_d.clone()];
    let pass1 = sort_cjk_reading_order(input, TextOrientation::Vertical);
    let pass2 = sort_cjk_reading_order(pass1.clone(), TextOrientation::Vertical);

    let names1: Vec<&str> = pass1.iter().map(|b| b.text.as_str()).collect();
    let names2: Vec<&str> = pass2.iter().map(|b| b.text.as_str()).collect();

    assert_eq!(names1, names2, "Sorting must be strictly idempotent");
}

#[test]
fn test_adversarial_layout_empty_and_single_box_invariants() {
    assert_eq!(
        sort_cjk_reading_order(vec![], TextOrientation::Horizontal),
        vec![]
    );
    assert_eq!(
        sort_cjk_reading_order(vec![], TextOrientation::Vertical),
        vec![]
    );

    let single = DetectedTextBox::new(Roi::new(50, 50, 100, 25), "only", 0.99);
    assert_eq!(
        sort_cjk_reading_order(vec![single.clone()], TextOrientation::Horizontal),
        vec![single.clone()]
    );
    assert_eq!(
        sort_cjk_reading_order(vec![single.clone()], TextOrientation::Vertical),
        vec![single]
    );
}

#[test]
fn test_adversarial_layout_zero_dimension_boxes() {
    let z1 = DetectedTextBox::new(Roi::new(10, 10, 0, 0), "zero_both", 0.9);
    let z2 = DetectedTextBox::new(Roi::new(10, 20, 0, 50), "zero_w", 0.9);
    let z3 = DetectedTextBox::new(Roi::new(20, 10, 50, 0), "zero_h", 0.9);

    let boxes = vec![z1, z2, z3];
    let sorted_h = sort_cjk_reading_order(boxes.clone(), TextOrientation::Horizontal);
    assert_eq!(sorted_h.len(), 3);

    let sorted_v = sort_cjk_reading_order(boxes, TextOrientation::Vertical);
    assert_eq!(sorted_v.len(), 3);
}

#[test]
fn test_adversarial_layout_identical_and_fully_overlapping_boxes() {
    let b1 = DetectedTextBox::new(Roi::new(100, 100, 50, 50), "same_1", 0.9);
    let b2 = DetectedTextBox::new(Roi::new(100, 100, 50, 50), "same_2", 0.9);

    let sorted_h =
        sort_cjk_reading_order(vec![b1.clone(), b2.clone()], TextOrientation::Horizontal);
    assert_eq!(sorted_h.len(), 2);

    let sorted_v = sort_cjk_reading_order(vec![b1, b2], TextOrientation::Vertical);
    assert_eq!(sorted_v.len(), 2);
}

#[test]
fn test_adversarial_layout_extreme_boundary_coordinates_no_panic() {
    // Stress test with boundary coordinates: x = u32::MAX - 1, w = 100, y = u32::MAX - 5, h = 1000
    let b1 = DetectedTextBox::new(
        Roi::new(u32::MAX - 1, u32::MAX - 5, 100, 1000),
        "Extreme1",
        0.9,
    );
    let b2 = DetectedTextBox::new(
        Roi::new(u32::MAX - 50, u32::MAX - 20, 200, 500),
        "Extreme2",
        0.9,
    );
    let b3 = DetectedTextBox::new(Roi::new(100, 200, 50, 50), "Normal", 0.9);

    // Horizontal mode verification
    let sorted_h = sort_cjk_reading_order(
        vec![b1.clone(), b2.clone(), b3.clone()],
        TextOrientation::Horizontal,
    );
    assert_eq!(sorted_h.len(), 3);
    let resorted_h = sort_cjk_reading_order(sorted_h.clone(), TextOrientation::Horizontal);
    assert_eq!(
        sorted_h, resorted_h,
        "Horizontal extreme coordinate sorting must be idempotent"
    );

    // Vertical mode verification
    let sorted_v = sort_cjk_reading_order(vec![b1, b2, b3], TextOrientation::Vertical);
    assert_eq!(sorted_v.len(), 3);
    let resorted_v = sort_cjk_reading_order(sorted_v.clone(), TextOrientation::Vertical);
    assert_eq!(
        sorted_v, resorted_v,
        "Vertical extreme coordinate sorting must be idempotent"
    );
}

#[test]
fn test_adversarial_layout_saturating_width_idempotence() {
    let box_a = DetectedTextBox::new(Roi::new(u32::MAX - 10, 10, 10, 10), "A", 0.9);
    let box_b = DetectedTextBox::new(Roi::new(u32::MAX - 10, 10, 20, 10), "A", 0.9);
    let box_c = DetectedTextBox::new(Roi::new(u32::MAX - 17, 10, 13, 10), "C", 0.9);

    let input = vec![box_b, box_a, box_c];
    let pass1 = sort_cjk_reading_order(input, TextOrientation::Vertical);
    let pass2 = sort_cjk_reading_order(pass1.clone(), TextOrientation::Vertical);

    assert_eq!(
        pass1, pass2,
        "Vertical sorting must be strictly idempotent even with saturating widths"
    );
}

#[test]
fn test_adversarial_layout_randomized_fuzzing_1000_permutations_idempotence() {
    // Deterministic PRNG (xorshift64)
    struct Rng(u64);
    impl Rng {
        fn next_u64(&mut self) -> u64 {
            let mut x = self.0;
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            self.0 = x;
            x
        }
        fn gen_range(&mut self, low: u32, high: u32) -> u32 {
            if low >= high {
                return low;
            }
            low + (self.next_u64() % ((high - low) as u64)) as u32
        }
    }

    let mut rng = Rng(0xdeadbeef_cafebabe);
    let sample_texts = [
        "A",
        "B",
        "C",
        "D",
        "E",
        "Same",
        "Same",
        "日本語",
        "テスト",
        "",
    ];

    for iteration in 0..1200 {
        let box_count = rng.gen_range(2, 20) as usize;
        let mut boxes = Vec::with_capacity(box_count);

        for _j in 0..box_count {
            // Mix normal coordinates, zero coordinates, and near u32::MAX coordinates
            let coord_mode = rng.gen_range(0, 10);
            let (x, y, w, h) = match coord_mode {
                0 => (
                    rng.gen_range(u32::MAX - 100, u32::MAX),
                    rng.gen_range(0, 500),
                    rng.gen_range(1, 150),
                    rng.gen_range(1, 100),
                ),
                1 => (
                    rng.gen_range(0, 500),
                    rng.gen_range(u32::MAX - 100, u32::MAX),
                    rng.gen_range(1, 100),
                    rng.gen_range(1, 150),
                ),
                2 => (
                    rng.gen_range(0, 300),
                    rng.gen_range(0, 300),
                    0,
                    rng.gen_range(0, 50),
                ),
                3 => (
                    // Same nominal column with jitter
                    rng.gen_range(200, 210),
                    rng.gen_range(0, 400),
                    rng.gen_range(20, 30),
                    rng.gen_range(20, 40),
                ),
                4 => (
                    // Saturating coordinates near u32::MAX
                    rng.gen_range(u32::MAX - 20, u32::MAX),
                    rng.gen_range(10, 20),
                    rng.gen_range(5, 50),
                    rng.gen_range(10, 20),
                ),
                _ => (
                    rng.gen_range(0, 800),
                    rng.gen_range(0, 800),
                    rng.gen_range(10, 80),
                    rng.gen_range(10, 80),
                ),
            };

            let text_idx = (rng.next_u64() % (sample_texts.len() as u64)) as usize;
            boxes.push(DetectedTextBox::new(
                Roi::new(x, y, w, h),
                sample_texts[text_idx],
                0.9,
            ));
        }

        for orientation in [TextOrientation::Horizontal, TextOrientation::Vertical] {
            let pass1 = sort_cjk_reading_order(boxes.clone(), orientation);
            let pass2 = sort_cjk_reading_order(pass1.clone(), orientation);

            assert_eq!(
                pass1, pass2,
                "Idempotence violation at iteration {} for orientation {:?}",
                iteration, orientation
            );
        }
    }
}

#[test]
fn test_aspect_ratio_boundary_and_zero_dimensions() {
    // Zero width should produce aspect ratio 1.0 without dividing by zero
    let zero_w = DetectedTextBox::new(Roi::new(10, 10, 0, 50), "zero_w", 0.9);
    assert_eq!(zero_w.aspect_ratio(), 1.0);

    // Exact threshold boundary tests:
    // H=14, W=10 -> H/W = 1.4 (not strictly > 1.4, treated as neutral)
    let b14 = DetectedTextBox::new(Roi::new(0, 0, 10, 14), "1.4", 0.9);
    assert!((b14.aspect_ratio() - 1.4).abs() < 1e-5);

    // H=15, W=10 -> H/W = 1.5 > 1.4 (strictly vertical candidate)
    let b15 = DetectedTextBox::new(Roi::new(0, 0, 10, 15), "1.5", 0.9);
    assert_eq!(
        detect_dominant_orientation(&[b15]),
        TextOrientation::Vertical
    );

    // H=8, W=10 -> H/W = 0.8 (not strictly < 0.8, treated as neutral)
    let b08 = DetectedTextBox::new(Roi::new(0, 0, 10, 8), "0.8", 0.9);
    assert!((b08.aspect_ratio() - 0.8).abs() < 1e-5);

    // H=7, W=10 -> H/W = 0.7 < 0.8 (strictly horizontal candidate)
    let b07 = DetectedTextBox::new(Roi::new(0, 0, 10, 7), "0.7", 0.9);
    assert_eq!(
        detect_dominant_orientation(&[b07]),
        TextOrientation::Horizontal
    );
}

#[test]
fn test_complex_multi_column_vertical_reading_order() {
    // 3 columns of vertical Japanese text read right-to-left:
    // Column 1 (Right): X=200..230, Y=10, Y=50, Y=90
    // Column 2 (Center): X=120..150, Y=10, Y=50
    // Column 3 (Left): X=40..70, Y=10, Y=50, Y=90, Y=130
    let c1_1 = DetectedTextBox::new(Roi::new(200, 10, 30, 35), "C1_Top", 0.95);
    let c1_2 = DetectedTextBox::new(Roi::new(200, 50, 30, 35), "C1_Mid", 0.95);
    let c1_3 = DetectedTextBox::new(Roi::new(200, 90, 30, 35), "C1_Bot", 0.95);

    let c2_1 = DetectedTextBox::new(Roi::new(120, 10, 30, 35), "C2_Top", 0.95);
    let c2_2 = DetectedTextBox::new(Roi::new(120, 50, 30, 35), "C2_Bot", 0.95);

    let c3_1 = DetectedTextBox::new(Roi::new(40, 10, 30, 35), "C3_1", 0.95);
    let c3_2 = DetectedTextBox::new(Roi::new(40, 50, 30, 35), "C3_2", 0.95);
    let c3_3 = DetectedTextBox::new(Roi::new(40, 90, 30, 35), "C3_3", 0.95);
    let c3_4 = DetectedTextBox::new(Roi::new(40, 130, 30, 35), "C3_4", 0.95);

    // Scramble input boxes
    let scrambled = vec![
        c3_3.clone(),
        c1_2.clone(),
        c2_1.clone(),
        c3_1.clone(),
        c1_1.clone(),
        c3_4.clone(),
        c2_2.clone(),
        c1_3.clone(),
        c3_2.clone(),
    ];

    let sorted = sort_cjk_reading_order(scrambled, TextOrientation::Vertical);
    let texts: Vec<&str> = sorted.iter().map(|b| b.text.as_str()).collect();

    assert_eq!(
        texts,
        vec!["C1_Top", "C1_Mid", "C1_Bot", "C2_Top", "C2_Bot", "C3_1", "C3_2", "C3_3", "C3_4",]
    );
}

#[test]
fn test_complex_multi_line_horizontal_reading_order() {
    // 3 lines of horizontal text:
    // Line 1: Y=20..40, X=10, X=80, X=150
    // Line 2: Y=60..80, X=10, X=100
    // Line 3: Y=100..120, X=10, X=90, X=170
    let l1_1 = DetectedTextBox::new(Roi::new(10, 20, 60, 20), "L1_W1", 0.9);
    let l1_2 = DetectedTextBox::new(Roi::new(80, 20, 60, 20), "L1_W2", 0.9);
    let l1_3 = DetectedTextBox::new(Roi::new(150, 20, 60, 20), "L1_W3", 0.9);

    let l2_1 = DetectedTextBox::new(Roi::new(10, 60, 80, 20), "L2_W1", 0.9);
    let l2_2 = DetectedTextBox::new(Roi::new(100, 60, 80, 20), "L2_W2", 0.9);

    let l3_1 = DetectedTextBox::new(Roi::new(10, 100, 70, 20), "L3_W1", 0.9);
    let l3_2 = DetectedTextBox::new(Roi::new(90, 100, 70, 20), "L3_W2", 0.9);
    let l3_3 = DetectedTextBox::new(Roi::new(170, 100, 70, 20), "L3_W3", 0.9);

    let scrambled = vec![
        l3_2.clone(),
        l1_3.clone(),
        l2_1.clone(),
        l1_1.clone(),
        l3_3.clone(),
        l2_2.clone(),
        l1_2.clone(),
        l3_1.clone(),
    ];

    let sorted = sort_cjk_reading_order(scrambled, TextOrientation::Horizontal);
    let texts: Vec<&str> = sorted.iter().map(|b| b.text.as_str()).collect();

    assert_eq!(
        texts,
        vec!["L1_W1", "L1_W2", "L1_W3", "L2_W1", "L2_W2", "L3_W1", "L3_W2", "L3_W3",]
    );
}

#[test]
fn test_cjk_joining_mixed_tokens() {
    // Mixed Japanese with English loanword
    let mixed = vec!["新機能", "RPG", "モード", "を", "解放", "!"];
    // Since tokens contain CJK, join without spaces
    let joined = join_cjk_or_latin_tokens(&mixed, is_cjk_text("新機能"));
    assert_eq!(joined, "新機能RPGモードを解放!");

    // Pure Latin tokens
    let latin = vec!["Game", "Over", "Try", "Again"];
    let joined_latin = join_cjk_or_latin_tokens(&latin, is_cjk_text("Game"));
    assert_eq!(joined_latin, "Game Over Try Again");
}

#[tokio::test]
async fn test_mock_backend_recognize_detailed_orientation_override() {
    let boxes = vec![
        DetectedTextBox::new(Roi::new(100, 20, 30, 80), "縦1", 0.9),
        DetectedTextBox::new(Roi::new(50, 20, 30, 80), "縦2", 0.9),
    ];
    let mock = MockOcrBackend::new_detailed("縦1縦2", boxes.clone(), TextOrientation::Vertical);

    let dummy = DynamicImage::ImageRgba8(RgbaImage::new(10, 10));

    // Request default detailed result
    let res = mock.recognize_detailed(&dummy, None).await.unwrap();
    assert_eq!(res.orientation, TextOrientation::Vertical);
    assert_eq!(res.boxes.len(), 2);
    assert_eq!(res.text, "縦1縦2");

    // Request with explicit orientation override
    let res_override = mock
        .recognize_detailed(&dummy, Some(TextOrientation::Horizontal))
        .await
        .unwrap();
    assert_eq!(res_override.orientation, TextOrientation::Horizontal);
}

#[test]
fn test_adversarial_cjk_western_token_transition_defect_demonstration() {
    // 1. CJK space-less joining works when purely CJK
    let cjk_tokens = vec!["これ", "は", "本"];
    let cjk_joined = join_cjk_or_latin_tokens(&cjk_tokens, true);
    assert_eq!(cjk_joined, "これは本");

    // 2. Western space-joining works when purely Western
    let latin_tokens = vec!["Hello", "World"];
    let latin_joined = join_cjk_or_latin_tokens(&latin_tokens, false);
    assert_eq!(latin_joined, "Hello World");

    // 3. Mixed CJK + Western transitions:
    // When tokens are ["Level", "10", "へ"]:
    // "Level" and "10" are Western/Latin words requiring a word space ("Level 10").
    // "10" and "へ" transition to CJK ("10へ").
    // Expected typographic output: "Level 10へ".
    let mixed_tokens = vec!["Level", "10", "へ"];
    let is_cjk = mixed_tokens.iter().any(|t| is_cjk_text(t));
    assert!(
        is_cjk,
        "Token 'へ' is CJK text, so is_cjk evaluates to true in OCR pipeline"
    );

    let actual_joined = join_cjk_or_latin_tokens(&mixed_tokens, is_cjk);

    // Verify transition-aware joining:
    // Western words retain spaces while transitioning to CJK without spaces
    assert_eq!(
        actual_joined, "Level 10へ",
        "Western words retain spaces even when is_cjk=true"
    );
}
