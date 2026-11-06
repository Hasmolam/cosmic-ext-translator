//! User preference for the OCR engine on scripts that more than one engine can read.

/// OCR engine selection stored in the applet configuration.
///
/// Applies to Japanese and Simplified Chinese, which both PaddleOCR and Tesseract read.
/// Latin text always uses `ocrs`; Korean always uses Tesseract.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OcrEngineChoice {
    /// PaddleOCR, falling back to Tesseract when its models cannot be loaded.
    #[default]
    Auto,
    Paddle,
    Tesseract,
}

impl OcrEngineChoice {
    /// All choices, in settings dropdown order.
    pub const ALL: [Self; 3] = [Self::Auto, Self::Paddle, Self::Tesseract];

    /// Code stored in config.
    #[must_use]
    pub fn code(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Paddle => "paddle",
            Self::Tesseract => "tesseract",
        }
    }

    /// Parses a stored code; unknown or empty values fall back to `Auto`.
    #[must_use]
    pub fn from_code(raw: &str) -> Self {
        match raw.trim().to_ascii_lowercase().as_str() {
            "paddle" | "paddleocr" => Self::Paddle,
            "tesseract" => Self::Tesseract,
            _ => Self::Auto,
        }
    }

    /// Position in [`OcrEngineChoice::ALL`].
    #[must_use]
    pub fn index(self) -> usize {
        Self::ALL.iter().position(|c| *c == self).unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_round_trip() {
        for choice in OcrEngineChoice::ALL {
            assert_eq!(OcrEngineChoice::from_code(choice.code()), choice);
            assert_eq!(OcrEngineChoice::ALL[choice.index()], choice);
        }
        assert_eq!(OcrEngineChoice::from_code(""), OcrEngineChoice::Auto);
        assert_eq!(
            OcrEngineChoice::from_code(" PaddleOCR "),
            OcrEngineChoice::Paddle
        );
    }
}
