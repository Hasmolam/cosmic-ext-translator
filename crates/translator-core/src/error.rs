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

pub type Result<T> = std::result::Result<T, CaptureError>;

/// Failure to construct an OCR backend for the configured source language.
#[derive(Error, Debug)]
pub enum OcrInitError {
    #[error("cannot create OCR model directory {path}: {source}")]
    ModelDir {
        path: std::path::PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("cannot download OCR model from {url}: {reason}")]
    Download { url: &'static str, reason: String },

    #[error("OCR model from {url} is invalid: {reason}")]
    InvalidModel { url: &'static str, reason: String },

    #[error("cannot write OCR model file {path}: {source}")]
    ModelWrite {
        path: std::path::PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("cannot load OCR model {path}: {reason}")]
    ModelLoad {
        path: std::path::PathBuf,
        reason: String,
    },

    #[error("OCR engine initialisation failed: {0}")]
    Engine(String),

    #[error("{language} text recognition requires Tesseract; install tesseract and its '{pack}' language data")]
    TesseractMissing {
        language: &'static str,
        pack: &'static str,
    },

    #[error("Tesseract is installed but the '{pack}' language data for {language} is missing")]
    TesseractLanguageMissing {
        language: &'static str,
        pack: &'static str,
    },

    #[error("OCR initialisation task failed: {0}")]
    Task(String),
}
