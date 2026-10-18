use std::sync::Arc;
use image::{DynamicImage, Rgba, RgbaImage};
use translator_core::ocr::{MockOcrBackend, OcrBackend, OcrsBackend};
use translator_core::pipeline::preprocess::{compute_otsu_threshold, preprocess_for_ocr, PreprocessConfig};

/// Computes Character Error Rate (CER) using Levenshtein distance normalized by ground truth length.
fn compute_cer(ground_truth: &str, predicted: &str) -> f64 {
    let gt = ground_truth.trim();
    let pred = predicted.trim();
    if gt.is_empty() {
        return if pred.is_empty() { 0.0 } else { 1.0 };
    }
    let distance = strsim::levenshtein(gt, pred);
    (distance as f64) / (gt.chars().count() as f64)
}

/// Computes Word Error Rate (WER) using word-level Levenshtein distance.
#[allow(clippy::needless_range_loop)]
fn compute_wer(ground_truth: &str, predicted: &str) -> f64 {
    let gt_words: Vec<&str> = ground_truth.split_whitespace().collect();
    let pred_words: Vec<&str> = predicted.split_whitespace().collect();

    if gt_words.is_empty() {
        return if pred_words.is_empty() { 0.0 } else { 1.0 };
    }

    // Levenshtein on word sequences
    let m = gt_words.len();
    let n = pred_words.len();
    let mut dp = vec![vec![0usize; n + 1]; m + 1];

    for i in 0..=m {
        dp[i][0] = i;
    }
    for j in 0..=n {
        dp[0][j] = j;
    }

    for i in 1..=m {
        for j in 1..=n {
            let cost = if gt_words[i - 1].eq_ignore_ascii_case(pred_words[j - 1]) {
                0
            } else {
                1
            };
            dp[i][j] = (dp[i - 1][j] + 1)
                .min(dp[i][j - 1] + 1)
                .min(dp[i - 1][j - 1] + cost);
        }
    }

    (dp[m][n] as f64) / (m as f64)
}

/// Renders basic geometric character shapes onto an RGBA canvas.
fn render_benchmark_banner(text: &str, dark_bg: bool) -> RgbaImage {
    let char_w = 12u32;
    let char_h = 20u32;
    let margin = 16u32;
    let total_w = margin * 2 + (text.len() as u32) * (char_w + 4);
    let total_h = margin * 2 + char_h;

    let bg_color = if dark_bg {
        Rgba([18, 20, 26, 255])
    } else {
        Rgba([240, 242, 245, 255])
    };
    let fg_color = if dark_bg {
        Rgba([250, 250, 255, 255])
    } else {
        Rgba([10, 10, 15, 255])
    };

    let mut img = RgbaImage::from_pixel(total_w, total_h, bg_color);

    for (i, ch) in text.chars().enumerate() {
        let x_start = margin + (i as u32) * (char_w + 4);
        let y_start = margin;

        match ch {
            'A' | 'a' => {
                // Draw 'A' stroke
                for y in 0..char_h {
                    img.put_pixel(x_start, y_start + y, fg_color);
                    img.put_pixel(x_start + char_w - 1, y_start + y, fg_color);
                }
                for x in 0..char_w {
                    img.put_pixel(x_start + x, y_start, fg_color);
                    img.put_pixel(x_start + x, y_start + char_h / 2, fg_color);
                }
            }
            'H' | 'h' => {
                // Draw 'H' stroke
                for y in 0..char_h {
                    img.put_pixel(x_start, y_start + y, fg_color);
                    img.put_pixel(x_start + char_w - 1, y_start + y, fg_color);
                }
                for x in 0..char_w {
                    img.put_pixel(x_start + x, y_start + char_h / 2, fg_color);
                }
            }
            'O' | 'o' => {
                // Draw 'O' stroke
                for y in 0..char_h {
                    img.put_pixel(x_start, y_start + y, fg_color);
                    img.put_pixel(x_start + char_w - 1, y_start + y, fg_color);
                }
                for x in 0..char_w {
                    img.put_pixel(x_start + x, y_start, fg_color);
                    img.put_pixel(x_start + x, y_start + char_h - 1, fg_color);
                }
            }
            _ => {
                // Default box stroke for other characters
                for y in 0..char_h {
                    img.put_pixel(x_start, y_start + y, fg_color);
                }
                for x in 0..char_w {
                    img.put_pixel(x_start + x, y_start + char_h / 2, fg_color);
                }
            }
        }
    }

    img
}

#[test]
fn test_cer_and_wer_metrics() {
    let gt = "The hero entered the ancient dungeon";
    let pred_exact = "The hero entered the ancient dungeon";
    let pred_minor = "The hero entered the anc1ent dungeon"; // 1 char error
    let pred_major = "A knight found a dark cave";

    assert_eq!(compute_cer(gt, pred_exact), 0.0);
    assert_eq!(compute_wer(gt, pred_exact), 0.0);

    let cer_minor = compute_cer(gt, pred_minor);
    assert!(cer_minor > 0.0 && cer_minor < 0.05, "CER for 1 char: {cer_minor}");
    let wer_minor = compute_wer(gt, pred_minor);
    assert!(wer_minor > 0.0 && wer_minor <= 0.2, "WER for 1 word: {wer_minor}");

    let cer_major = compute_cer(gt, pred_major);
    assert!(cer_major > 0.4, "CER for major divergence: {cer_major}");
    let wer_major = compute_wer(gt, pred_major);
    assert!(wer_major > 0.8, "WER for major divergence: {wer_major}");
}

#[test]
fn test_image_preprocessing_polarity_and_scaling() {
    let config = PreprocessConfig {
        scale_factor: 1.5,
        binarize: true,
        auto_invert: true,
    };

    // 1. Test dark background image (white text on dark bg)
    let dark_bg_img = render_benchmark_banner("HAO", true);
    let original_w = dark_bg_img.width();
    let original_h = dark_bg_img.height();

    let processed_dark = preprocess_for_ocr(&dark_bg_img, &config);

    // Scaling check: Dimensions must be scaled by 1.5x
    let expected_w = ((original_w as f32) * 1.5).round() as u32;
    let expected_h = ((original_h as f32) * 1.5).round() as u32;
    assert_eq!(processed_dark.width(), expected_w);
    assert_eq!(processed_dark.height(), expected_h);

    // Polarity inversion check: Inverted result must have white background (255) and black text (0)
    let corner_pixel = processed_dark.get_pixel(2, 2)[0];
    assert_eq!(
        corner_pixel, 255,
        "Background of inverted image must be white (255) for standard OCR input"
    );

    // 2. Test light background image (dark text on light bg)
    let light_bg_img = render_benchmark_banner("HAO", false);
    let processed_light = preprocess_for_ocr(&light_bg_img, &config);
    let corner_pixel_light = processed_light.get_pixel(2, 2)[0];
    assert_eq!(
        corner_pixel_light, 255,
        "Background of light image must remain white (255) after Otsu binarization"
    );
}

#[test]
fn test_otsu_thresholding_separation() {
    let mut gray = image::GrayImage::new(100, 100);
    // 50% dark pixels (value 20), 50% bright pixels (value 220)
    for y in 0..100 {
        for x in 0..100 {
            let v = if x < 50 { 20 } else { 220 };
            gray.put_pixel(x, y, image::Luma([v]));
        }
    }

    let threshold = compute_otsu_threshold(&gray);
    assert!(
        (20..=220).contains(&threshold),
        "Otsu threshold should lie between the two bimodal peaks: {threshold}"
    );
}

#[tokio::test]
async fn test_mock_ocr_and_pipeline_benchmarking() {
    let ground_truth = "Press [A] to interact with NPC";
    let mock = Arc::new(MockOcrBackend::new(ground_truth));

    let banner = render_benchmark_banner("TEST", true);
    let dynamic = DynamicImage::ImageRgba8(banner);

    let recognized = mock.recognize(&dynamic).await.unwrap();
    let cer = compute_cer(ground_truth, &recognized);
    let wer = compute_wer(ground_truth, &recognized);

    assert_eq!(cer, 0.0);
    assert_eq!(wer, 0.0);
}

#[tokio::test]
async fn test_real_ocrs_engine_execution() {
    // If models are available in the system, test actual neural network inference
    let backend_res = OcrsBackend::load_default_or_download().await;
    let Ok(backend) = backend_res else {
        // Skip live inference if environment has no network and no pre-cached models
        return;
    };

    // A clean synthetic white image should yield empty recognized text cleanly
    let blank = DynamicImage::ImageRgba8(RgbaImage::from_pixel(120, 60, Rgba([255, 255, 255, 255])));
    let result = backend.recognize(&blank).await.expect("Inference should not panic on blank input");
    assert!(result.is_empty(), "Blank image should produce empty OCR text");
}
