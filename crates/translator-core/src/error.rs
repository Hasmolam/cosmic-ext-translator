use thiserror::Error;

pub use translator_capture::{CaptureError, Result};

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

    #[error(
        "no OCR engine available for {language} text. PaddleOCR: {paddle}. Tesseract: {tesseract}"
    )]
    NoEngine {
        language: &'static str,
        paddle: String,
        tesseract: String,
    },

    #[error("OCR initialisation task failed: {0}")]
    Task(String),
}
