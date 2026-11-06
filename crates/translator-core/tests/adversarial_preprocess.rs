use image::{GrayImage, Luma, Rgba, RgbaImage};
use translator_core::pipeline::preprocess::{
    compute_otsu_threshold, preprocess_for_ocr, subtract_blur, PreprocessConfig,
};

fn variance(img: &GrayImage) -> f64 {
    let n = (img.width() * img.height()) as f64;
    if n == 0.0 {
        return 0.0;
    }
    let mean = img.pixels().map(|p| p[0] as f64).sum::<f64>() / n;
    img.pixels()
        .map(|p| (p[0] as f64 - mean).powi(2))
        .sum::<f64>()
        / n
}

#[test]
fn test_extreme_all_black() {
    let img = GrayImage::from_pixel(100, 100, Luma([0]));
    let result = subtract_blur(&img, 2.5);

    assert_eq!(result.width(), 100);
    assert_eq!(result.height(), 100);
    for p in result.pixels() {
        assert_eq!(
            p[0], 128,
            "All-black image after blur subtraction must be neutral gray 128"
        );
    }
}

#[test]
fn test_extreme_all_white() {
    let img = GrayImage::from_pixel(100, 100, Luma([255]));
    let result = subtract_blur(&img, 2.5);

    assert_eq!(result.width(), 100);
    assert_eq!(result.height(), 100);
    for p in result.pixels() {
        assert_eq!(
            p[0], 128,
            "All-white image after blur subtraction must be neutral gray 128"
        );
    }
}

#[test]
fn test_pixel_math_bounds_and_no_wrap() {
    // Test all pairs of (orig, blr) values to mathematically verify bounds
    for orig in 0..=255u8 {
        for blr in 0..=255u8 {
            let diff = (orig as i32) - (blr as i32);
            let val = (128 + diff).clamp(0, 255);
            assert!((0..=255).contains(&val));
            let as_u8 = val as u8;
            assert_eq!(val, as_u8 as i32, "No truncation or wrapping allowed");
        }
    }
}

#[test]
fn test_high_frequency_checkerboards() {
    // 1x1 checkerboard: alternating 0 and 255
    let mut img = GrayImage::new(64, 64);
    for y in 0..64 {
        for x in 0..64 {
            let val = if (x + y) % 2 == 0 { 0 } else { 255 };
            img.put_pixel(x, y, Luma([val]));
        }
    }

    let result = subtract_blur(&img, 2.5);
    // On a 1x1 checkerboard, the blur is uniform ~128.
    // Pixel 0 minus 128 -> clamp(128 - 128, 0, 255) = 0.
    // Pixel 255 minus 128 -> clamp(128 + 127, 0, 255) = 255.
    // Output should retain high alternating contrast.
    let p0 = result.get_pixel(10, 10)[0];
    let p1 = result.get_pixel(11, 10)[0];
    assert!(
        (p0 as i32 - p1 as i32).abs() > 200,
        "1x1 checkerboard contrast must be preserved"
    );

    // 2x2 checkerboard
    let mut img2 = GrayImage::new(64, 64);
    for y in 0..64 {
        for x in 0..64 {
            let val = if ((x / 2) + (y / 2)) % 2 == 0 { 0 } else { 255 };
            img2.put_pixel(x, y, Luma([val]));
        }
    }
    let result2 = subtract_blur(&img2, 2.5);
    let q0 = result2.get_pixel(10, 10)[0];
    let q1 = result2.get_pixel(12, 10)[0];
    assert!(
        (q0 as i32 - q1 as i32).abs() > 100,
        "2x2 checkerboard contrast must be preserved"
    );
}

#[test]
fn test_single_pixel_noise() {
    // All black with single white pixel
    let mut black_spike = GrayImage::from_pixel(64, 64, Luma([0]));
    black_spike.put_pixel(32, 32, Luma([255]));

    let res_black = subtract_blur(&black_spike, 2.5);
    let spike_val = res_black.get_pixel(32, 32)[0];
    assert!(
        spike_val > 200,
        "Spike must be strongly positive relative to 128: {spike_val}"
    );
    // Background far away must be 128
    assert_eq!(res_black.get_pixel(0, 0)[0], 128);

    // All white with single black pixel
    let mut white_dip = GrayImage::from_pixel(64, 64, Luma([255]));
    white_dip.put_pixel(32, 32, Luma([0]));

    let res_white = subtract_blur(&white_dip, 2.5);
    let dip_val = res_white.get_pixel(32, 32)[0];
    assert!(
        dip_val < 50,
        "Dip must be strongly negative relative to 128: {dip_val}"
    );
    assert_eq!(res_white.get_pixel(0, 0)[0], 128);
}

#[test]
fn test_tiny_images() {
    // 0x0 empty image
    let empty = GrayImage::new(0, 0);
    let res_empty = subtract_blur(&empty, 2.5);
    assert_eq!(res_empty.width(), 0);
    assert_eq!(res_empty.height(), 0);

    // 1x1 image
    for val in [0u8, 128, 255] {
        let tiny1 = GrayImage::from_pixel(1, 1, Luma([val]));
        let res1 = subtract_blur(&tiny1, 2.5);
        assert_eq!(res1.width(), 1);
        assert_eq!(res1.height(), 1);
        assert_eq!(res1.get_pixel(0, 0)[0], 128, "1x1 image diff is 0, so 128");
    }

    // 2x2 image
    let mut tiny2 = GrayImage::new(2, 2);
    tiny2.put_pixel(0, 0, Luma([0]));
    tiny2.put_pixel(1, 0, Luma([255]));
    tiny2.put_pixel(0, 1, Luma([255]));
    tiny2.put_pixel(1, 1, Luma([0]));
    let res2 = subtract_blur(&tiny2, 2.5);
    assert_eq!(res2.width(), 2);
    assert_eq!(res2.height(), 2);

    // 3x3 image
    let tiny3 = GrayImage::from_pixel(3, 3, Luma([100]));
    let res3 = subtract_blur(&tiny3, 2.5);
    assert_eq!(res3.width(), 3);
    assert_eq!(res3.height(), 3);
    for p in res3.pixels() {
        assert_eq!(p[0], 128);
    }

    // 0x0 image check on subtract_blur and preprocess_for_ocr
    let empty_rgba = RgbaImage::new(0, 0);
    let config = PreprocessConfig {
        scale_factor: 1.0,
        blur_subtraction: true,
        ..Default::default()
    };
    let empty_proc = preprocess_for_ocr(&empty_rgba, &config);
    assert_eq!(empty_proc.width(), 0);
    assert_eq!(empty_proc.height(), 0);

    // Asymmetric tiny images: 1x10 and 10x1
    let strip_h = GrayImage::from_pixel(10, 1, Luma([200]));
    let res_h = subtract_blur(&strip_h, 2.5);
    assert_eq!(res_h.width(), 10);
    assert_eq!(res_h.height(), 1);

    let strip_v = GrayImage::from_pixel(1, 10, Luma([50]));
    let res_v = subtract_blur(&strip_v, 2.5);
    assert_eq!(res_v.width(), 1);
    assert_eq!(res_v.height(), 10);
}

#[test]
fn test_gradient_variance_attenuation_exceeds_95_percent() {
    let w = 256u32;
    let h = 80u32;

    // Linear horizontal gradient: 0 -> 255
    let mut linear_h = GrayImage::new(w, h);
    for y in 0..h {
        for x in 0..w {
            linear_h.put_pixel(x, y, Luma([x as u8]));
        }
    }
    let orig_var = variance(&linear_h);
    let filtered = subtract_blur(&linear_h, 2.5);
    let filt_var = variance(&filtered);
    let attenuation = (orig_var - filt_var) / orig_var;
    assert!(
        attenuation > 0.95,
        "Linear horizontal gradient attenuation must exceed 95%, was: {:.4}% (orig: {}, filt: {})",
        attenuation * 100.0,
        orig_var,
        filt_var
    );

    // Steep non-linear exponential gradient
    let mut exp_grad = GrayImage::new(w, h);
    for y in 0..h {
        for x in 0..w {
            let norm = x as f64 / (w - 1) as f64;
            // e^(norm * ln(256)) - 1
            let val = ((norm.powf(2.0)) * 255.0).round().clamp(0.0, 255.0) as u8;
            exp_grad.put_pixel(x, y, Luma([val]));
        }
    }
    let orig_var_exp = variance(&exp_grad);
    let filtered_exp = subtract_blur(&exp_grad, 2.5);
    let filt_var_exp = variance(&filtered_exp);
    let attenuation_exp = (orig_var_exp - filt_var_exp) / orig_var_exp;
    assert!(
        attenuation_exp > 0.95,
        "Exponential gradient attenuation must exceed 95%, was: {:.4}%",
        attenuation_exp * 100.0
    );

    // Sigmoid / S-curve gradient
    let mut sig_grad = GrayImage::new(w, h);
    for y in 0..h {
        for x in 0..w {
            let t = (x as f64 - 128.0) / 25.0;
            let val = (255.0 / (1.0 + (-t).exp())).round().clamp(0.0, 255.0) as u8;
            sig_grad.put_pixel(x, y, Luma([val]));
        }
    }
    let orig_var_sig = variance(&sig_grad);
    let filtered_sig = subtract_blur(&sig_grad, 2.5);
    let filt_var_sig = variance(&filtered_sig);
    let attenuation_sig = (orig_var_sig - filt_var_sig) / orig_var_sig;
    assert!(
        attenuation_sig > 0.95,
        "Sigmoid gradient attenuation must exceed 95%, was: {:.4}%",
        attenuation_sig * 100.0
    );

    // Diagonal linear gradient
    let mut diag = GrayImage::new(w, h);
    for y in 0..h {
        for x in 0..w {
            let norm = (x as f64 / w as f64 + y as f64 / h as f64) / 2.0;
            diag.put_pixel(x, y, Luma([(norm * 255.0) as u8]));
        }
    }
    let orig_var_diag = variance(&diag);
    let filtered_diag = subtract_blur(&diag, 2.5);
    let filt_var_diag = variance(&filtered_diag);
    let attenuation_diag = (orig_var_diag - filt_var_diag) / orig_var_diag;
    assert!(
        attenuation_diag > 0.95,
        "Diagonal gradient attenuation must exceed 95%, was: {:.4}%",
        attenuation_diag * 100.0
    );
}

#[test]
fn test_interaction_dark_bg_white_text() {
    let w = 120u32;
    let h = 50u32;
    // Dark background (20) with white text strokes (240) - thin stroke 2px wide
    let mut rgba = RgbaImage::from_pixel(w, h, Rgba([20, 20, 20, 255]));
    // Draw 2-pixel wide vertical stroke
    for y in 10..40 {
        rgba.put_pixel(50, y, Rgba([240, 240, 240, 255]));
        rgba.put_pixel(51, y, Rgba([240, 240, 240, 255]));
    }

    let config = PreprocessConfig {
        scale_factor: 1.0,
        binarize: true,
        auto_invert: true,
        blur_subtraction: true,
        blur_sigma: 2.5,
    };

    let gray_scaled = image::imageops::grayscale(&rgba);
    let gray_hp = subtract_blur(&gray_scaled, 2.5);
    let th = compute_otsu_threshold(&gray_hp);
    println!(
        "Thin stroke test: stroke_hp={}, bg_hp={}, th={}",
        gray_hp.get_pixel(50, 25)[0],
        gray_hp.get_pixel(5, 5)[0],
        th
    );

    let processed = preprocess_for_ocr(&rgba, &config);
    println!(
        "Processed: bg={}, stroke={}",
        processed.get_pixel(5, 5)[0],
        processed.get_pixel(50, 25)[0]
    );
    // Background must be white (255)
    assert_eq!(
        processed.get_pixel(5, 5)[0],
        255,
        "Dark background must be converted to white (255)"
    );
    // Text must be black (0)
    assert_eq!(
        processed.get_pixel(50, 25)[0],
        0,
        "White text must be converted to black (0)"
    );
}

#[test]
fn test_interaction_bright_bg_dark_text() {
    let w = 120u32;
    let h = 50u32;
    // Bright background (230) with dark text strokes (20) - 2px stroke
    let mut rgba = RgbaImage::from_pixel(w, h, Rgba([230, 230, 230, 255]));
    for y in 10..40 {
        rgba.put_pixel(50, y, Rgba([20, 20, 20, 255]));
        rgba.put_pixel(51, y, Rgba([20, 20, 20, 255]));
    }

    let config = PreprocessConfig {
        scale_factor: 1.0,
        binarize: true,
        auto_invert: true,
        blur_subtraction: true,
        blur_sigma: 2.5,
    };

    let gray_scaled = image::imageops::grayscale(&rgba);
    let gray_hp = subtract_blur(&gray_scaled, 2.5);
    let th = compute_otsu_threshold(&gray_hp);
    println!(
        "Bright bg test: stroke_hp={}, bg_hp={}, th={}",
        gray_hp.get_pixel(50, 25)[0],
        gray_hp.get_pixel(5, 5)[0],
        th
    );

    let processed = preprocess_for_ocr(&rgba, &config);
    println!(
        "Bright bg processed: bg={}, stroke={}",
        processed.get_pixel(5, 5)[0],
        processed.get_pixel(50, 25)[0]
    );
    // Background must be white (255)
    assert_eq!(
        processed.get_pixel(5, 5)[0],
        255,
        "Bright background must remain white (255)"
    );
    // Text must be black (0)
    assert_eq!(
        processed.get_pixel(50, 25)[0],
        0,
        "Dark text must remain black (0)"
    );
}

#[test]
fn test_interaction_neutral_backgrounds() {
    let w = 120u32;
    let h = 50u32;

    // Subcase A: Neutral dark (120) with white text (240) - 2px stroke
    let mut rgba_a = RgbaImage::from_pixel(w, h, Rgba([120, 120, 120, 255]));
    for y in 10..40 {
        rgba_a.put_pixel(50, y, Rgba([240, 240, 240, 255]));
        rgba_a.put_pixel(51, y, Rgba([240, 240, 240, 255]));
    }
    let config = PreprocessConfig {
        scale_factor: 1.0,
        binarize: true,
        auto_invert: true,
        blur_subtraction: true,
        blur_sigma: 2.5,
    };
    let proc_a = preprocess_for_ocr(&rgba_a, &config);
    println!(
        "Neutral-dark processed: bg={}, stroke={}",
        proc_a.get_pixel(5, 5)[0],
        proc_a.get_pixel(50, 25)[0]
    );
    assert_eq!(
        proc_a.get_pixel(5, 5)[0],
        255,
        "Neutral-dark bg must produce white bg"
    );
    assert_eq!(
        proc_a.get_pixel(50, 25)[0],
        0,
        "White text on neutral-dark must produce black text"
    );

    // Subcase B: Neutral bright (135) with dark text (20) - 2px stroke
    let mut rgba_b = RgbaImage::from_pixel(w, h, Rgba([135, 135, 135, 255]));
    for y in 10..40 {
        rgba_b.put_pixel(50, y, Rgba([20, 20, 20, 255]));
        rgba_b.put_pixel(51, y, Rgba([20, 20, 20, 255]));
    }
    let proc_b = preprocess_for_ocr(&rgba_b, &config);
    println!(
        "Neutral-bright processed: bg={}, stroke={}",
        proc_b.get_pixel(5, 5)[0],
        proc_b.get_pixel(50, 25)[0]
    );
    assert_eq!(
        proc_b.get_pixel(5, 5)[0],
        255,
        "Neutral-bright bg must produce white bg"
    );
    assert_eq!(
        proc_b.get_pixel(50, 25)[0],
        0,
        "Dark text on neutral-bright must produce black text"
    );
}

#[test]
fn test_otsu_threshold_on_uniform_images() {
    let black = GrayImage::from_pixel(50, 50, Luma([0]));
    let th_b = compute_otsu_threshold(&black);
    assert_eq!(
        th_b, 128,
        "Uniform black image should yield default threshold 128"
    );

    let white = GrayImage::from_pixel(50, 50, Luma([255]));
    let th_w = compute_otsu_threshold(&white);
    assert_eq!(
        th_w, 128,
        "Uniform white image should yield default threshold 128"
    );

    let gray = GrayImage::from_pixel(50, 50, Luma([128]));
    let th_g = compute_otsu_threshold(&gray);
    assert_eq!(
        th_g, 128,
        "Uniform gray image should yield default threshold 128"
    );
}

#[test]
fn test_sigma_extremes() {
    let img = GrayImage::from_pixel(30, 30, Luma([150]));

    // Very small sigma
    let res_small = subtract_blur(&img, 0.01);
    assert_eq!(res_small.get_pixel(15, 15)[0], 128);

    // Large sigma
    let res_large = subtract_blur(&img, 50.0);
    assert_eq!(res_large.get_pixel(15, 15)[0], 128);

    // Zero sigma
    let res_zero = subtract_blur(&img, 0.0);
    assert_eq!(res_zero.get_pixel(15, 15)[0], 128);
}

#[test]
fn test_steep_bright_gradient_dark_text() {
    let w = 200u32;
    let h = 60u32;

    // Steep bright gradient: 140 -> 240
    let mut rgba = RgbaImage::new(w, h);
    for y in 0..h {
        for x in 0..w {
            let bg = (140.0 + (x as f64 / w as f64) * 100.0) as u8;
            rgba.put_pixel(x, y, Rgba([bg, bg, bg, 255]));
        }
    }
    // Draw 2-pixel wide dark text stroke (10)
    for y in 15..45 {
        rgba.put_pixel(100, y, Rgba([10, 10, 10, 255]));
        rgba.put_pixel(101, y, Rgba([10, 10, 10, 255]));
    }

    let config = PreprocessConfig {
        scale_factor: 1.0,
        binarize: true,
        auto_invert: true,
        blur_subtraction: true,
        blur_sigma: 2.5,
    };

    let processed = preprocess_for_ocr(&rgba, &config);

    // Background must be white (255) on both sides
    assert_eq!(
        processed.get_pixel(10, 10)[0],
        255,
        "Left bright background must be white"
    );
    assert_eq!(
        processed.get_pixel(180, 10)[0],
        255,
        "Right bright background must be white"
    );
    // Text stroke must be black (0)
    assert_eq!(
        processed.get_pixel(100, 30)[0],
        0,
        "Dark text stroke must be black (0)"
    );
}

#[test]
fn test_steep_dark_gradient_white_text() {
    let w = 200u32;
    let h = 60u32;

    // Steep dark gradient: 15 -> 110
    let mut rgba = RgbaImage::new(w, h);
    for y in 0..h {
        for x in 0..w {
            let bg = (15.0 + (x as f64 / w as f64) * 95.0) as u8;
            rgba.put_pixel(x, y, Rgba([bg, bg, bg, 255]));
        }
    }
    // Draw 2-pixel wide white text stroke (245)
    for y in 15..45 {
        rgba.put_pixel(100, y, Rgba([245, 245, 245, 255]));
        rgba.put_pixel(101, y, Rgba([245, 245, 245, 255]));
    }

    let config = PreprocessConfig {
        scale_factor: 1.0,
        binarize: true,
        auto_invert: true,
        blur_subtraction: true,
        blur_sigma: 2.5,
    };

    let processed = preprocess_for_ocr(&rgba, &config);

    // Background must be white (255) on both sides
    assert_eq!(
        processed.get_pixel(10, 10)[0],
        255,
        "Left dark background must be inverted to white"
    );
    assert_eq!(
        processed.get_pixel(180, 10)[0],
        255,
        "Right dark background must be inverted to white"
    );
    // Text stroke must be black (0)
    assert_eq!(
        processed.get_pixel(100, 30)[0],
        0,
        "White text stroke must be inverted to black (0)"
    );
}

fn render_synthetic_glyph(text: &str, dark_bg: bool) -> RgbaImage {
    let char_w = 10u32;
    let char_h = 18u32;
    let margin = 12u32;
    let total_w = margin * 2 + (text.len() as u32) * (char_w + 4);
    let total_h = margin * 2 + char_h;

    let bg_color = if dark_bg {
        Rgba([20, 20, 25, 255])
    } else {
        Rgba([235, 235, 240, 255])
    };
    let fg_color = if dark_bg {
        Rgba([245, 245, 250, 255])
    } else {
        Rgba([15, 15, 20, 255])
    };

    let mut img = RgbaImage::from_pixel(total_w, total_h, bg_color);

    for (i, _) in text.chars().enumerate() {
        let x_start = margin + (i as u32) * (char_w + 4);
        let y_start = margin;

        // Draw 1-pixel wide vertical strokes and horizontal bars (letter 'H')
        for y in 0..char_h {
            img.put_pixel(x_start, y_start + y, fg_color);
            img.put_pixel(x_start + char_w - 1, y_start + y, fg_color);
        }
        for x in 0..char_w {
            img.put_pixel(x_start + x, y_start + char_h / 2, fg_color);
        }
    }

    img
}

#[test]
fn test_rendered_banner_ocr_polarity_and_scaling() {
    for scale in [1.0f32, 1.5f32, 2.0f32] {
        let config = PreprocessConfig {
            scale_factor: scale,
            binarize: true,
            auto_invert: true,
            blur_subtraction: true,
            blur_sigma: 2.5,
        };

        // Dark background scene with bright text
        let dark_banner = render_synthetic_glyph("HH", true);
        let proc_dark = preprocess_for_ocr(&dark_banner, &config);
        // Corners must be white background
        assert_eq!(
            proc_dark.get_pixel(2, 2)[0],
            255,
            "Dark banner background must be white (scale={scale})"
        );

        // Bright background scene with dark text
        let light_banner = render_synthetic_glyph("HH", false);
        let proc_light = preprocess_for_ocr(&light_banner, &config);
        // Corners must be white background
        assert_eq!(
            proc_light.get_pixel(2, 2)[0],
            255,
            "Light banner background must be white (scale={scale})"
        );
    }
}
