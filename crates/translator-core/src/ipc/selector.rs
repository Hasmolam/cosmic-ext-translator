//! Output protocol of `cosmic-translator-overlay --selector`.
//!
//! The selector prints one line to stdout and exits:
//! `ROI_SELECTED:{"x":..,"y":..,"width":..,"height":..,"surface_width":..,"surface_height":..}`
//! on success, or `ROI_CANCELLED` when the user aborts. Other lines (logging) are ignored
//! by the parser.

use serde::{Deserialize, Serialize};

use crate::region::Roi;

/// Line prefix of a successful selection.
pub const SELECTED_PREFIX: &str = "ROI_SELECTED:";
/// Line printed when the selection is cancelled.
pub const CANCELLED_LINE: &str = "ROI_CANCELLED";

/// A region picked in the selector, in logical surface coordinates.
///
/// `surface_width`/`surface_height` are the size of the selector surface; they are needed
/// to scale the region to the physical capture buffer on HiDPI and fractional scaling.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SelectorOutput {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
    #[serde(default)]
    pub surface_width: Option<u32>,
    #[serde(default)]
    pub surface_height: Option<u32>,
}

impl SelectorOutput {
    #[must_use]
    pub const fn new(roi: Roi, surface_width: u32, surface_height: u32) -> Self {
        Self {
            x: roi.x,
            y: roi.y,
            width: roi.width,
            height: roi.height,
            surface_width: Some(surface_width),
            surface_height: Some(surface_height),
        }
    }

    #[must_use]
    pub const fn roi(&self) -> Roi {
        Roi::new(self.x, self.y, self.width, self.height)
    }

    /// Formats the stdout line printed by the selector.
    pub fn to_line(&self) -> serde_json::Result<String> {
        Ok(format!("{SELECTED_PREFIX}{}", serde_json::to_string(self)?))
    }

    /// Parses one line; `None` for unrelated or malformed lines.
    #[must_use]
    pub fn parse_line(line: &str) -> Option<Self> {
        let json = line.trim().strip_prefix(SELECTED_PREFIX)?;
        serde_json::from_str(json).ok()
    }

    /// Returns the first valid selection in the selector's stdout, or `None` if the
    /// selection was cancelled or the output contains no valid selection.
    #[must_use]
    pub fn parse_stdout(stdout: &str) -> Option<Self> {
        stdout.lines().find_map(Self::parse_line)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_roundtrip() {
        let out = SelectorOutput::new(Roi::new(150, 250, 800, 450), 1920, 1080);
        let line = out.to_line().unwrap();
        assert!(line.starts_with(SELECTED_PREFIX));
        assert_eq!(SelectorOutput::parse_line(&line), Some(out));
        assert_eq!(out.roi(), Roi::new(150, 250, 800, 450));
    }
}
