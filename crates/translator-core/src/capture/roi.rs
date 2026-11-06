use crate::error::CaptureError;
use image::RgbaImage;
use serde::{Deserialize, Serialize};

/// Defines a rectangular Region of Interest on the captured screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Roi {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

impl Roi {
    #[must_use]
    pub const fn new(x: u32, y: u32, width: u32, height: u32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.width == 0 || self.height == 0
    }

    /// Clamps this ROI within the given frame dimensions.
    /// Returns `None` if the ROI is completely outside the frame or has zero area.
    #[must_use]
    pub fn clamp(&self, frame_width: u32, frame_height: u32) -> Option<Self> {
        if self.x >= frame_width || self.y >= frame_height {
            return None;
        }

        let clamped_width = self.width.min(frame_width - self.x);
        let clamped_height = self.height.min(frame_height - self.y);

        if clamped_width == 0 || clamped_height == 0 {
            None
        } else {
            Some(Self {
                x: self.x,
                y: self.y,
                width: clamped_width,
                height: clamped_height,
            })
        }
    }

    /// Scales the ROI coordinates from Wayland logical surface dimensions to
    /// PipeWire physical buffer dimensions, ensuring pixel-perfect alignment
    /// across HiDPI and fractional scaling displays (%125, %150, %200).
    #[must_use]
    pub fn scale_to_frame(
        &self,
        surface_w: u32,
        surface_h: u32,
        frame_w: u32,
        frame_h: u32,
    ) -> Self {
        if surface_w == 0 || surface_h == 0 || frame_w == 0 || frame_h == 0 {
            return *self;
        }
        let scale_x = frame_w as f64 / surface_w as f64;
        let scale_y = frame_h as f64 / surface_h as f64;

        Self {
            x: ((self.x as f64) * scale_x).round() as u32,
            y: ((self.y as f64) * scale_y).round() as u32,
            width: ((self.width as f64) * scale_x).round() as u32,
            height: ((self.height as f64) * scale_y).round() as u32,
        }
    }

    /// Efficiently crops and copies only the ROI pixels from a raw 32-bit RGBA/BGRA/RGBx frame.
    pub fn crop_to_rgba(
        &self,
        buffer: &[u8],
        frame_width: u32,
        frame_height: u32,
        stride: usize,
        is_bgr: bool,
    ) -> Result<RgbaImage, CaptureError> {
        let clamped = self
            .clamp(frame_width, frame_height)
            .ok_or(CaptureError::OutOfBoundsRoi)?;

        let bytes_per_pixel = 4;
        let roi_width = clamped.width as usize;
        let roi_height = clamped.height as usize;
        let row_bytes = roi_width * bytes_per_pixel;

        let total_required = (clamped.y as usize + roi_height - 1) * stride
            + (clamped.x as usize + roi_width) * bytes_per_pixel;

        if buffer.len() < total_required {
            return Err(CaptureError::BufferTooSmall {
                expected: total_required,
                actual: buffer.len(),
            });
        }

        let mut output_bytes = vec![0u8; roi_width * roi_height * bytes_per_pixel];

        for row in 0..roi_height {
            let src_y = clamped.y as usize + row;
            let src_x_offset = clamped.x as usize * bytes_per_pixel;
            let src_start = src_y * stride + src_x_offset;
            let src_end = src_start + row_bytes;

            let dst_start = row * row_bytes;
            let dst_end = dst_start + row_bytes;

            let src_row = &buffer[src_start..src_end];
            let dst_row = &mut output_bytes[dst_start..dst_end];

            if is_bgr {
                for chunk_idx in 0..roi_width {
                    let s_i = chunk_idx * 4;
                    let d_i = chunk_idx * 4;
                    dst_row[d_i] = src_row[s_i + 2]; // R
                    dst_row[d_i + 1] = src_row[s_i + 1]; // G
                    dst_row[d_i + 2] = src_row[s_i]; // B
                    dst_row[d_i + 3] = 255; // A
                }
            } else {
                dst_row.copy_from_slice(src_row);
                for chunk_idx in 0..roi_width {
                    dst_row[chunk_idx * 4 + 3] = 255;
                }
            }
        }

        RgbaImage::from_raw(clamped.width, clamped.height, output_bytes)
            .ok_or(CaptureError::InvalidImageDimensions)
    }

    /// Masks an inner ignore subregion within the cropped image by painting it with solid black pixels.
    ///
    /// # Arguments
    /// * `image` - Mutable reference to the cropped `RgbaImage` corresponding to `self` (the target ROI).
    /// * `ignore` - The region to ignore/mask, expressed in the same coordinate space as `self` (full-frame coordinates).
    ///
    /// # Coordinate Translation & Intersection
    /// Computes the 2D bounding box intersection between `self` $[x, x + w) \times [y, y + h)$
    /// and `ignore` $[ix, ix + iw) \times [iy, iy + ih)$.
    /// If there is no overlap, this function performs no writes and returns immediately.
    /// If an overlap exists, coordinates are translated to local image coordinates
    /// $(x_{\text{local}} = x_{\text{inter}} - \text{self.x}, y_{\text{local}} = y_{\text{inter}} - \text{self.y})$,
    /// clamped to the image dimensions, and filled with solid black pixels `Rgba([0, 0, 0, 255])`.
    pub fn mask_region(&self, image: &mut RgbaImage, ignore: &Roi) {
        if self.is_empty() || ignore.is_empty() {
            return;
        }

        // Full-frame coordinate bounds of self (target ROI)
        let self_left = self.x;
        let self_right = self.x.saturating_add(self.width);
        let self_top = self.y;
        let self_bottom = self.y.saturating_add(self.height);

        // Full-frame coordinate bounds of ignore ROI
        let ignore_left = ignore.x;
        let ignore_right = ignore.x.saturating_add(ignore.width);
        let ignore_top = ignore.y;
        let ignore_bottom = ignore.y.saturating_add(ignore.height);

        // Intersection bounds in full-frame coordinates
        let inter_left = self_left.max(ignore_left);
        let inter_right = self_right.min(ignore_right);
        let inter_top = self_top.max(ignore_top);
        let inter_bottom = self_bottom.min(ignore_bottom);

        // Non-intersecting check: if empty or disjoint, return immediately
        if inter_left >= inter_right || inter_top >= inter_bottom {
            return;
        }

        // Translate to image-local coordinates
        let local_start_x = inter_left.saturating_sub(self.x);
        let local_end_x = inter_right.saturating_sub(self.x);
        let local_start_y = inter_top.saturating_sub(self.y);
        let local_end_y = inter_bottom.saturating_sub(self.y);

        // Clip against actual image dimensions
        let img_w = image.width();
        let img_h = image.height();

        let x_start = local_start_x.min(img_w);
        let x_end = local_end_x.min(img_w);
        let y_start = local_start_y.min(img_h);
        let y_end = local_end_y.min(img_h);

        if x_start >= x_end || y_start >= y_end {
            return;
        }

        let black = image::Rgba([0, 0, 0, 255]);
        for y in y_start..y_end {
            for x in x_start..x_end {
                image.put_pixel(x, y, black);
            }
        }
    }
}

/// Standard game genre and screen region presets for dialogue capture.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum RoiPreset {
    /// Standard RPG Maker / Retro JRPG dialogue box (Bottom 25% of height, 5% horizontal margins)
    #[default]
    Bottom25,
    /// Visual Novel / Adventure game dialogue box (Bottom 33% of height, 4% horizontal margins)
    Bottom33,
    /// Full screen / entire window capture
    FullScreen,
    /// Custom manually-drawn ROI box
    Custom,
}

impl RoiPreset {
    pub const ALL: [Self; 4] = [
        Self::Bottom25,
        Self::Bottom33,
        Self::FullScreen,
        Self::Custom,
    ];

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Bottom25 => "rpg_bottom_25",
            Self::Bottom33 => "vn_bottom_33",
            Self::FullScreen => "fullscreen",
            Self::Custom => "custom",
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::Bottom25 => "RPG Maker / JRPG (Bottom 25%)",
            Self::Bottom33 => "Visual Novel (Bottom 33%)",
            Self::FullScreen => "Full Screen",
            Self::Custom => "Custom Area",
        }
    }

    pub fn from_str_name(name: &str) -> Self {
        match name {
            "rpg_bottom_25" | "bottom25" => Self::Bottom25,
            "vn_bottom_33" | "bottom33" => Self::Bottom33,
            "fullscreen" => Self::FullScreen,
            _ => Self::Custom,
        }
    }

    /// Computes the pixel ROI dynamically scaled for the given frame dimensions.
    /// Eliminates resolution mismatches (e.g. 1080p -> 1440p -> 4K).
    pub fn compute_for_frame(
        &self,
        frame_w: u32,
        frame_h: u32,
        custom: Option<Roi>,
    ) -> Option<Roi> {
        if frame_w == 0 || frame_h == 0 {
            return None;
        }

        match self {
            Self::Bottom25 => {
                let h = ((frame_h as f32) * 0.25).round() as u32;
                let y = frame_h.saturating_sub(h);
                let margin_x = ((frame_w as f32) * 0.05).round() as u32;
                let w = frame_w.saturating_sub(margin_x * 2);
                Roi::new(margin_x, y, w, h).clamp(frame_w, frame_h)
            }
            Self::Bottom33 => {
                let h = ((frame_h as f32) * 0.33).round() as u32;
                let y = frame_h.saturating_sub(h);
                let margin_x = ((frame_w as f32) * 0.04).round() as u32;
                let w = frame_w.saturating_sub(margin_x * 2);
                Roi::new(margin_x, y, w, h).clamp(frame_w, frame_h)
            }
            Self::FullScreen => Some(Roi::new(0, 0, frame_w, frame_h)),
            Self::Custom => custom.and_then(|r| r.clamp(frame_w, frame_h)),
        }
    }
}

/// Represents the active capture configuration: a preset, an optional custom ROI, and an optional ignore ROI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct CaptureRegion {
    pub preset: RoiPreset,
    pub custom: Option<Roi>,
    pub ignore_roi: Option<Roi>,
}

impl CaptureRegion {
    #[must_use]
    pub const fn new(preset: RoiPreset, custom: Option<Roi>, ignore_roi: Option<Roi>) -> Self {
        Self {
            preset,
            custom,
            ignore_roi,
        }
    }

    #[must_use]
    pub const fn without_ignore(preset: RoiPreset, custom: Option<Roi>) -> Self {
        Self {
            preset,
            custom,
            ignore_roi: None,
        }
    }

    #[must_use]
    pub const fn with_ignore_roi(mut self, ignore_roi: Option<Roi>) -> Self {
        self.ignore_roi = ignore_roi;
        self
    }

    pub fn compute(&self, width: u32, height: u32) -> Option<Roi> {
        self.preset.compute_for_frame(width, height, self.custom)
    }
}

impl From<Option<Roi>> for CaptureRegion {
    fn from(opt: Option<Roi>) -> Self {
        Self {
            preset: if opt.is_some() {
                RoiPreset::Custom
            } else {
                RoiPreset::FullScreen
            },
            custom: opt,
            ignore_roi: None,
        }
    }
}

impl From<Roi> for CaptureRegion {
    fn from(r: Roi) -> Self {
        Self {
            preset: RoiPreset::Custom,
            custom: Some(r),
            ignore_roi: None,
        }
    }
}

impl From<RoiPreset> for CaptureRegion {
    fn from(preset: RoiPreset) -> Self {
        Self {
            preset,
            custom: None,
            ignore_roi: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_roi_clamp() {
        let roi = Roi::new(10, 10, 50, 50);
        let clamped = roi.clamp(100, 100).unwrap();
        assert_eq!(clamped, Roi::new(10, 10, 50, 50));

        let overflowing = Roi::new(80, 80, 50, 50);
        let clamped_over = overflowing.clamp(100, 100).unwrap();
        assert_eq!(clamped_over, Roi::new(80, 80, 20, 20));

        let completely_out = Roi::new(120, 120, 50, 50);
        assert!(completely_out.clamp(100, 100).is_none());
    }

    #[test]
    fn test_crop_to_rgba() {
        let width = 10;
        let height = 10;
        let stride = 10 * 4;
        let mut buffer = vec![0u8; width * height * 4];

        let idx = (3 * width + 2) * 4;
        buffer[idx] = 255;
        buffer[idx + 1] = 0;
        buffer[idx + 2] = 0;
        buffer[idx + 3] = 255;

        let roi = Roi::new(2, 3, 2, 2);
        let cropped = roi
            .crop_to_rgba(&buffer, width as u32, height as u32, stride, false)
            .unwrap();
        assert_eq!(cropped.width(), 2);
        assert_eq!(cropped.height(), 2);

        let pixel = cropped.get_pixel(0, 0);
        assert_eq!(pixel.0, [255, 0, 0, 255]);
    }

    #[test]
    fn test_roi_scale_to_frame_hidpi() {
        let logical_roi = Roi::new(100, 200, 300, 150);
        // Logical surface 1280x720, Physical frame 1920x1080 (1.5x fractional scale)
        let scaled = logical_roi.scale_to_frame(1280, 720, 1920, 1080);
        assert_eq!(scaled.x, 150);
        assert_eq!(scaled.y, 300);
        assert_eq!(scaled.width, 450);
        assert_eq!(scaled.height, 225);

        // 2.0x scale (e.g. 4K 3840x2160 on 1920x1080 surface)
        let scaled_4k = logical_roi.scale_to_frame(1920, 1080, 3840, 2160);
        assert_eq!(scaled_4k.x, 200);
        assert_eq!(scaled_4k.y, 400);
        assert_eq!(scaled_4k.width, 600);
        assert_eq!(scaled_4k.height, 300);
    }

    #[test]
    fn test_roi_preset_dynamic_scaling() {
        // 1080p (1920x1080) Bottom 25%
        let roi_1080p = RoiPreset::Bottom25
            .compute_for_frame(1920, 1080, None)
            .unwrap();
        assert_eq!(roi_1080p.height, 270); // 1080 * 0.25
        assert_eq!(roi_1080p.y, 810); // 1080 - 270
        assert_eq!(roi_1080p.x, 96); // 1920 * 0.05
        assert_eq!(roi_1080p.width, 1728); // 1920 - (96 * 2)

        // 720p (1280x720) Bottom 25%
        let roi_720p = RoiPreset::Bottom25
            .compute_for_frame(1280, 720, None)
            .unwrap();
        assert_eq!(roi_720p.height, 180);
        assert_eq!(roi_720p.y, 540);

        // Visual Novel Bottom 33%
        let roi_vn = RoiPreset::Bottom33
            .compute_for_frame(1920, 1080, None)
            .unwrap();
        assert_eq!(roi_vn.height, 356); // 1080 * 0.33 rounded
        assert_eq!(roi_vn.y, 724);

        // Fullscreen
        let roi_fs = RoiPreset::FullScreen
            .compute_for_frame(1920, 1080, None)
            .unwrap();
        assert_eq!(roi_fs, Roi::new(0, 0, 1920, 1080));
    }

    #[test]
    fn test_mask_region_completely_contained() {
        let target = Roi::new(100, 100, 200, 150);
        let ignore = Roi::new(120, 130, 50, 40);

        let white = image::Rgba([255, 255, 255, 255]);
        let black = image::Rgba([0, 0, 0, 255]);
        let mut image = RgbaImage::from_pixel(200, 150, white);

        target.mask_region(&mut image, &ignore);

        // Expected local bounds:
        // x: 120 - 100 = 20 .. 20 + 50 = 70
        // y: 130 - 100 = 30 .. 30 + 40 = 70
        for y in 0..150 {
            for x in 0..200 {
                let pixel = *image.get_pixel(x, y);
                if (20..70).contains(&x) && (30..70).contains(&y) {
                    assert_eq!(pixel, black, "Masked pixel mismatch at ({x}, {y})");
                } else {
                    assert_eq!(pixel, white, "Unmasked pixel corrupted at ({x}, {y})");
                }
            }
        }
    }

    #[test]
    fn test_mask_region_intersecting_partial_overlap() {
        let target = Roi::new(100, 100, 100, 100);
        // Ignore box extends beyond top-left of target
        let ignore_top_left = Roi::new(50, 80, 80, 60);

        let white = image::Rgba([255, 255, 255, 255]);
        let black = image::Rgba([0, 0, 0, 255]);
        let mut image = RgbaImage::from_pixel(100, 100, white);

        target.mask_region(&mut image, &ignore_top_left);

        // Overlap in frame: [100..130) x [100..140)
        // Local in image:   [0..30)   x [0..40)
        for y in 0..100 {
            for x in 0..100 {
                let pixel = *image.get_pixel(x, y);
                if (0..30).contains(&x) && (0..40).contains(&y) {
                    assert_eq!(pixel, black, "Expected masked pixel at ({x}, {y})");
                } else {
                    assert_eq!(pixel, white, "Expected unmasked pixel at ({x}, {y})");
                }
            }
        }
    }

    #[test]
    fn test_mask_region_non_intersecting() {
        let target = Roi::new(100, 100, 100, 100);
        let white = image::Rgba([255, 255, 255, 255]);

        let non_intersecting_cases = [
            Roi::new(10, 100, 50, 50),  // Left of target (right edge 60 < 100)
            Roi::new(220, 100, 50, 50), // Right of target (left edge 220 >= 200)
            Roi::new(100, 20, 50, 50),  // Above target (bottom edge 70 < 100)
            Roi::new(100, 220, 50, 50), // Below target (top edge 220 >= 200)
            Roi::new(200, 100, 50, 50), // Touching right edge (no area overlap)
        ];

        for (idx, ignore) in non_intersecting_cases.iter().enumerate() {
            let mut image = RgbaImage::from_pixel(100, 100, white);
            target.mask_region(&mut image, ignore);

            // Entire image must remain untouched
            assert!(
                image.pixels().all(|p| *p == white),
                "Non-intersecting case {idx} ({:?}) mutated image pixels",
                ignore
            );
        }
    }

    #[test]
    fn test_mask_region_enclosing() {
        let target = Roi::new(50, 50, 80, 80);
        let ignore = Roi::new(0, 0, 300, 300);

        let white = image::Rgba([255, 255, 255, 255]);
        let black = image::Rgba([0, 0, 0, 255]);
        let mut image = RgbaImage::from_pixel(80, 80, white);

        target.mask_region(&mut image, &ignore);

        // Entire image must be painted black
        assert!(
            image.pixels().all(|p| *p == black),
            "Enclosing ignore ROI did not mask all pixels"
        );
    }

    #[test]
    fn test_mask_region_empty_edge_cases() {
        let target = Roi::new(100, 100, 100, 100);
        let white = image::Rgba([255, 255, 255, 255]);

        let empty_cases = [
            Roi::new(120, 120, 0, 50),
            Roi::new(120, 120, 50, 0),
            Roi::new(120, 120, 0, 0),
        ];

        for ignore in &empty_cases {
            let mut image = RgbaImage::from_pixel(100, 100, white);
            target.mask_region(&mut image, ignore);
            assert!(image.pixels().all(|p| *p == white));
        }

        // Target itself empty
        let empty_target = Roi::new(100, 100, 0, 0);
        let ignore = Roi::new(50, 50, 100, 100);
        let mut image = RgbaImage::from_pixel(10, 10, white);
        empty_target.mask_region(&mut image, &ignore);
        assert!(image.pixels().all(|p| *p == white));
    }
}
