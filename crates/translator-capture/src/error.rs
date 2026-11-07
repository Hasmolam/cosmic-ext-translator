use thiserror::Error;

#[derive(Error, Debug)]
pub enum CaptureError {
    #[error("Ashpd portal error: {0}")]
    Portal(#[from] ashpd::Error),

    #[error("PipeWire error: {0}")]
    PipeWire(String),

    #[error("Failed to acquire valid PipeWire file descriptor")]
    MissingPipeWireFd,

    #[error("No streams returned by screencast portal")]
    NoStreamsAvailable,

    #[error("Selected ROI is out of bounds or empty")]
    OutOfBoundsRoi,

    #[error("Frame buffer too small: expected at least {expected} bytes, got {actual}")]
    BufferTooSmall { expected: usize, actual: usize },

    #[error("Invalid image dimensions for RgbaImage conversion")]
    InvalidImageDimensions,

    #[error("Channel error: {0}")]
    Channel(String),

    #[error("Thread terminated unexpectedly: {0}")]
    ThreadDied(String),
}

impl CaptureError {
    /// The user closed the screen-sharing dialog without choosing a source.
    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        matches!(
            self,
            Self::Portal(ashpd::Error::Response(
                ashpd::desktop::ResponseError::Cancelled
            ))
        )
    }
}

pub type Result<T> = std::result::Result<T, CaptureError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cancelled_portal_dialog_is_detected() {
        let cancelled = CaptureError::Portal(ashpd::Error::Response(
            ashpd::desktop::ResponseError::Cancelled,
        ));
        assert!(cancelled.is_cancelled());
        assert!(!CaptureError::NoStreamsAvailable.is_cancelled());
    }
}
