use serde::{Deserialize, Serialize};
use image::RgbaImage;
use crate::error::CaptureError;

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
        Self { x, y, width, height }
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
                    dst_row[d_i] = src_row[s_i + 2];     // R
                    dst_row[d_i + 1] = src_row[s_i + 1]; // G
                    dst_row[d_i + 2] = src_row[s_i];     // B
                    dst_row[d_i + 3] = 255;              // A
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
        let cropped = roi.crop_to_rgba(&buffer, width as u32, height as u32, stride, false).unwrap();
        assert_eq!(cropped.width(), 2);
        assert_eq!(cropped.height(), 2);

        let pixel = cropped.get_pixel(0, 0);
        assert_eq!(pixel.0, [255, 0, 0, 255]);
    }
}
