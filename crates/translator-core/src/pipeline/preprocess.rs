use image::{imageops::FilterType, GrayImage, RgbaImage};

#[derive(Debug, Clone)]
pub struct PreprocessConfig {
    /// Upscale factor for small game text (e.g., 1.5 or 2.0). 1.0 disables scaling.
    pub scale_factor: f32,
    /// Whether to apply Otsu binarization.
    pub binarize: bool,
    /// Automatically invert to ensure dark text on light background.
    pub auto_invert: bool,
}

impl Default for PreprocessConfig {
    fn default() -> Self {
        Self {
            scale_factor: 1.5,
            binarize: false,
            auto_invert: true,
        }
    }
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
    let mut gray = if should_scale {
        let new_w = ((image.width() as f32) * config.scale_factor).round() as u32;
        let new_h = ((image.height() as f32) * config.scale_factor).round() as u32;
        image::imageops::resize(&gray_base, new_w.max(1), new_h.max(1), FilterType::CatmullRom)
    } else {
        gray_base
    };

    if !config.binarize {
        return gray;
    }

    // 3. Otsu's thresholding
    let threshold = compute_otsu_threshold(&gray);

    // 4. Polarity check: Detect if background is predominantly dark
    let is_dark_background = if config.auto_invert {
        detect_dark_background(&gray)
    } else {
        false
    };

    // Apply binarization and inversion
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
        assert!((30..=220).contains(&threshold), "Otsu threshold should be between peaks: {threshold}");
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

        let processed = preprocess_for_ocr(&img, &PreprocessConfig {
            scale_factor: 1.0,
            binarize: true,
            auto_invert: true,
        });

        // Background should be inverted to white (255)
        assert_eq!(processed.get_pixel(0, 0)[0], 255);
        // Text center should be converted to black (0) for OCR
        assert_eq!(processed.get_pixel(50, 50)[0], 0);
    }
}
