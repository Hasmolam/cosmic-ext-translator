//! Text direction setting stored in the applet configuration.

use cjk_ocr_layout::TextOrientation;
use serde::{Deserialize, Serialize};

/// User-selected text direction for OCR engines that need it up front (Tesseract).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum OrientationMode {
    /// Try horizontal first; fall back to vertical when the result looks wrong.
    #[default]
    Auto,
    Horizontal,
    Vertical,
}

impl OrientationMode {
    /// All modes, in settings dropdown order.
    pub const ALL: [Self; 3] = [Self::Auto, Self::Horizontal, Self::Vertical];

    /// Code stored in config.
    #[must_use]
    pub fn code(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Horizontal => "horizontal",
            Self::Vertical => "vertical",
        }
    }

    /// Parses a stored code; unknown or empty values fall back to `Auto`.
    #[must_use]
    pub fn from_code(raw: &str) -> Self {
        match raw.trim().to_ascii_lowercase().as_str() {
            "horizontal" | "yokogaki" => Self::Horizontal,
            "vertical" | "tategaki" => Self::Vertical,
            _ => Self::Auto,
        }
    }

    /// Fixed orientation, or `None` for automatic detection.
    #[must_use]
    pub fn forced(self) -> Option<TextOrientation> {
        match self {
            Self::Auto => None,
            Self::Horizontal => Some(TextOrientation::Horizontal),
            Self::Vertical => Some(TextOrientation::Vertical),
        }
    }

    /// Position in [`OrientationMode::ALL`].
    #[must_use]
    pub fn index(self) -> usize {
        match self {
            Self::Auto => 0,
            Self::Horizontal => 1,
            Self::Vertical => 2,
        }
    }
}
