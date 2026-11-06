use image::{imageops::FilterType, GrayImage, RgbaImage};

#[derive(Debug, Clone)]
pub struct PreprocessConfig {
    /// Upscale factor for small game text (e.g., 1.5 or 2.0). 1.0 disables scaling.
    pub scale_factor: f32,
    /// Whether to apply Otsu binarization.
    pub binarize: bool,
    /// Automatically invert to ensure dark text on light background.
    pub auto_invert: bool,
    /// High-pass spatial filter: subtracts Gaussian-blurred background to remove smooth gradients.
    pub blur_subtraction: bool,
    /// Standard deviation sigma for Gaussian blur subtraction filter (default: 2.5).
    pub blur_sigma: f32,
}

impl Default for PreprocessConfig {
    fn default() -> Self {
        Self {
            scale_factor: 1.5,
            binarize: false,
            auto_invert: true,
            blur_subtraction: false,
            blur_sigma: 2.5,
        }
    }
}

/// Applies a high-pass spatial filter by subtracting a Gaussian-blurred low-frequency background.
///
/// For each pixel:
/// $$I_{\text{hp}}(x, y) = \text{clamp}(128 + I(x, y) - I_{\text{blur}}(x, y), 0, 255)$$
///
/// Smooth background gradients, vignetting, and translucent game scenery are flattened
/// to neutral gray (128), while high-frequency font glyph strokes are preserved.
#[must_use]
pub fn subtract_blur(gray: &GrayImage, sigma: f32) -> GrayImage {
    if gray.is_empty() || gray.width() == 0 || gray.height() == 0 {
        return GrayImage::new(gray.width(), gray.height());
    }

    let blurred = image::imageops::blur(gray, sigma);
    let mut output = GrayImage::new(gray.width(), gray.height());

    for (out, (orig, blr)) in output.pixels_mut().zip(gray.pixels().zip(blurred.pixels())) {
        let diff = (orig[0] as i32) - (blr[0] as i32);
        out[0] = (128 + diff).clamp(0, 255) as u8;
    }

    output
}

/// Applies game-optimized image preprocessing to maximize OCR accuracy:
/// 1. Lanczos3 scaling for low-resolution / small font ROIs.
/// 2. Grayscale conversion.
/// 3. Otsu automatic optimal thresholding (binarization).
/// 4. Background polarity check & color inversion (producing black text on white background).
#[must_use]
pub fn preprocess_for_ocr(image: &RgbaImage, config: &PreprocessConfig) -> GrayImage {
    // 1. Fast grayscale conversion
    let gray_base = image::imageops::grayscale(image);

    // 2. High-performance CatmullRom scaling (only for small text ROIs, 5x faster than Lanczos3)
    let should_scale = (config.scale_factor - 1.0).abs() > 0.05 && image.height() < 160;
    let gray_scaled = if should_scale {
        let new_w = ((image.width() as f32) * config.scale_factor).round() as u32;
        let new_h = ((image.height() as f32) * config.scale_factor).round() as u32;
        image::imageops::resize(
            &gray_base,
            new_w.max(1),
            new_h.max(1),
            FilterType::CatmullRom,
        )
    } else {
        gray_base
    };

    // 3. Polarity check: Detect if original background is predominantly dark.
    // Must be performed on the scaled grayscale image BEFORE blur subtraction,
    // because blur subtraction centers the background at neutral gray (128).
    let is_dark_background = if config.auto_invert {
        detect_dark_background(&gray_scaled)
    } else {
        false
    };

    // 4. High-pass blur subtraction filter
    let mut gray = if config.blur_subtraction {
        subtract_blur(&gray_scaled, config.blur_sigma)
    } else {
        gray_scaled
    };

    if !config.binarize {
        return gray;
    }

    // 5. Otsu's thresholding
    let threshold = compute_otsu_threshold(&gray);

    // 6. Apply binarization and inversion to ensure black text on white background
    for pixel in gray.pixels_mut() {
        let is_bright = pixel[0] > threshold;
        if is_dark_background {
            // Bright text on dark background: turn bright text to black (0), dark background to white (255)
            pixel[0] = if is_bright { 0 } else { 255 };
        } else {
            // Dark text on bright background: turn dark text to black (0), bright background to white (255)
            pixel[0] = if is_bright { 255 } else { 0 };
        }
    }

    gray
}

/// Calculates Otsu's optimal binarization threshold maximizing inter-class variance.
#[must_use]
pub fn compute_otsu_threshold(gray: &GrayImage) -> u8 {
    let mut histogram = [0u32; 256];
    for pixel in gray.pixels() {
        histogram[pixel[0] as usize] += 1;
    }

    let total_pixels: u32 = histogram.iter().sum();
    if total_pixels == 0 {
        return 128;
    }

    let mut sum_all: f64 = 0.0;
    for (i, &count) in histogram.iter().enumerate() {
        sum_all += (i as f64) * (count as f64);
    }

    let mut sum_bg: f64 = 0.0;
    let mut weight_bg: u32 = 0;
    let mut max_variance: f64 = 0.0;
    let mut best_threshold: u8 = 128;

    for (t, &count) in histogram.iter().enumerate() {
        weight_bg += count;
        if weight_bg == 0 {
            continue;
        }

        let weight_fg = total_pixels - weight_bg;
        if weight_fg == 0 {
            break;
        }

        sum_bg += (t as f64) * (count as f64);

        let mean_bg = sum_bg / (weight_bg as f64);
        let mean_fg = (sum_all - sum_bg) / (weight_fg as f64);

        let variance = (weight_bg as f64) * (weight_fg as f64) * (mean_bg - mean_fg).powi(2);

        if variance > max_variance {
            max_variance = variance;
            best_threshold = t as u8;
        }
    }

    best_threshold
}

/// Detects whether the border/background pixels of an image are predominantly dark (< 128).
fn detect_dark_background(gray: &GrayImage) -> bool {
    let w = gray.width();
    let h = gray.height();
    if w == 0 || h == 0 {
        return true;
    }

    let mut border_sum: u64 = 0;
    let mut count: u64 = 0;

    // Sample top and bottom borders
    for x in 0..w {
        border_sum += gray.get_pixel(x, 0)[0] as u64;
        border_sum += gray.get_pixel(x, h - 1)[0] as u64;
        count += 2;
    }

    // Sample left and right borders
    for y in 0..h {
        border_sum += gray.get_pixel(0, y)[0] as u64;
        border_sum += gray.get_pixel(w - 1, y)[0] as u64;
        count += 2;
    }

    if count == 0 {
        return true;
    }

    let avg = border_sum / count;
    avg < 128
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Luma, Rgba};

    #[test]
    fn test_otsu_thresholding() {
        // Bimodal distribution: half pixels 30, half pixels 220
        let mut img = GrayImage::new(100, 100);
        for y in 0..100 {
            for x in 0..100 {
                let val = if x < 50 { 30 } else { 220 };
                img.put_pixel(x, y, Luma([val]));
            }
        }

        let threshold = compute_otsu_threshold(&img);
        assert!(
            (30..=220).contains(&threshold),
            "Otsu threshold should be between peaks: {threshold}"
        );
    }

    #[test]
    fn test_preprocess_auto_inversion() {
        // Dark background (10) with bright text box (240) in center
        let mut img = RgbaImage::from_pixel(100, 100, Rgba([10, 10, 10, 255]));
        for y in 40..60 {
            for x in 40..60 {
                img.put_pixel(x, y, Rgba([240, 240, 240, 255]));
            }
        }

        let processed = preprocess_for_ocr(
            &img,
            &PreprocessConfig {
                scale_factor: 1.0,
                binarize: true,
                auto_invert: true,
                ..Default::default()
            },
        );

        // Background should be inverted to white (255)
        assert_eq!(processed.get_pixel(0, 0)[0], 255);
        // Text center should be converted to black (0) for OCR
        assert_eq!(processed.get_pixel(50, 50)[0], 0);
    }

    #[test]
    fn test_high_pass_gradient_attenuation() {
        let w = 200u32;
        let h = 60u32;

        // Linear gradient background: 20 at left -> 220 at right
        let mut grad_img = GrayImage::new(w, h);
        for y in 0..h {
            for x in 0..w {
                let val = (20.0 + (x as f64 / w as f64) * 200.0) as u8;
                grad_img.put_pixel(x, y, image::Luma([val]));
            }
        }

        // Compute baseline variance of raw gradient
        let n = (w * h) as f64;
        let mean_orig: f64 = grad_img.pixels().map(|p| p[0] as f64).sum::<f64>() / n;
        let var_orig: f64 = grad_img
            .pixels()
            .map(|p| (p[0] as f64 - mean_orig).powi(2))
            .sum::<f64>()
            / n;
        assert!(
            var_orig > 3000.0,
            "Raw gradient should have high variance: {var_orig}"
        );

        // Apply high-pass blur subtraction
        let filtered = subtract_blur(&grad_img, 2.5);

        // Compute variance after high-pass filtering
        let mean_filt: f64 = filtered.pixels().map(|p| p[0] as f64).sum::<f64>() / n;
        let var_filt: f64 = filtered
            .pixels()
            .map(|p| (p[0] as f64 - mean_filt).powi(2))
            .sum::<f64>()
            / n;

        // Gradient variance must be attenuated by >99.9% (below 5.0)
        assert!(
            var_filt < 5.0,
            "High-pass filter must flatten smooth gradient variance: {var_filt}"
        );

        // Baseline background level must be neutral gray (~128)
        assert!((126..=130).contains(&(mean_filt.round() as i32)));
    }

    #[test]
    fn test_high_pass_contrast_preservation() {
        let w = 100u32;
        let h = 50u32;

        // 1. Bright text stroke (240) on dark gradient background (20..80)
        let mut img = GrayImage::new(w, h);
        for y in 0..h {
            for x in 0..w {
                let bg = (20.0 + (x as f64 / w as f64) * 60.0) as u8;
                img.put_pixel(x, y, image::Luma([bg]));
            }
        }
        // Draw 2-pixel wide vertical stroke at x = 50
        for y in 10..40 {
            img.put_pixel(50, y, image::Luma([240]));
            img.put_pixel(51, y, image::Luma([240]));
        }

        let filtered = subtract_blur(&img, 2.5);

        let stroke_val = filtered.get_pixel(50, 25)[0];
        let left_bg_val = filtered.get_pixel(15, 25)[0];
        let right_bg_val = filtered.get_pixel(85, 25)[0];

        // Stroke must stand out significantly above the 128 baseline
        assert!(stroke_val >= 200, "Stroke value must be high: {stroke_val}");
        assert!(
            (126..=130).contains(&left_bg_val),
            "Left background should be ~128: {left_bg_val}"
        );
        assert!(
            (126..=130).contains(&right_bg_val),
            "Right background should be ~128: {right_bg_val}"
        );

        // Contrast between stroke and background must exceed 70 gray levels
        let contrast = (stroke_val as i32) - (left_bg_val as i32);
        assert!(
            contrast >= 70,
            "Contrast retention must be >= 70: {contrast}"
        );
    }

    #[test]
    fn test_preprocess_blur_subtraction_otsu_integration() {
        let w = 200u32;
        let h = 60u32;

        // Create an RGBA image with a steep background gradient (30 -> 210)
        // and white text strokes (250) in the middle
        let mut rgba = RgbaImage::new(w, h);
        for y in 0..h {
            for x in 0..w {
                let bg = (30.0 + (x as f64 / w as f64) * 180.0) as u8;
                rgba.put_pixel(x, y, image::Rgba([bg, bg, bg, 255]));
            }
        }
        for y in 20..40 {
            for x in 99..101 {
                rgba.put_pixel(x, y, image::Rgba([250, 250, 250, 255]));
            }
        }

        let config = PreprocessConfig {
            scale_factor: 1.0,
            binarize: true,
            auto_invert: true,
            blur_subtraction: true,
            blur_sigma: 2.5,
        };

        let processed = preprocess_for_ocr(&rgba, &config);

        // Standard OCR target: Background must be white (255) on BOTH left and right sides
        assert_eq!(
            processed.get_pixel(10, 10)[0],
            255,
            "Left background must be white after Otsu"
        );
        assert_eq!(
            processed.get_pixel(180, 10)[0],
            255,
            "Right background must be white after Otsu (no false text blob)"
        );

        // Text box center must be black (0) for OCR
        assert_eq!(
            processed.get_pixel(100, 30)[0],
            0,
            "Text strokes must be inverted to black (0)"
        );
    }
}
