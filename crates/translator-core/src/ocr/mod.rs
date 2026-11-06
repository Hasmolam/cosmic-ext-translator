use anyhow::Result;
use image::DynamicImage;
use ocrs::{ImageSource, OcrEngine, OcrEngineParams, TextItem};
use rten::Model;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use crate::error::OcrInitError;
use crate::language::{normalize_source_code, script_for_code};

pub mod engine;
pub mod furigana;
pub mod layout;
pub mod models;
pub mod normalize;
pub mod paddle;
pub mod tesseract;

pub use engine::OcrEngineChoice;
pub use furigana::{filter_furigana, is_punctuation_char};
pub use layout::{
    detect_dominant_orientation, sort_cjk_reading_order, DetectedTextBox, OrientationMode,
    TextOrientation,
};
pub use normalize::normalize_cjk_text;
pub use paddle::PaddleOcrBackend;
pub use tesseract::{parse_tesseract_lang_list, TesseractBackend};

pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Result of detailed OCR layout and text analysis.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct OcrResult {
    pub text: String,
    pub boxes: Vec<DetectedTextBox>,
    pub orientation: TextOrientation,
}

/// Returns true if a character falls within recognized CJK unicode ranges.
#[must_use]
pub fn is_cjk_char(c: char) -> bool {
    matches!(
        c,
        '\u{3040}'..='\u{309F}' | // Hiragana
        '\u{30A0}'..='\u{30FF}' | // Katakana
        '\u{4E00}'..='\u{9FFF}' | // CJK Unified Ideographs
        '\u{3400}'..='\u{4DBF}' | // CJK Extension A
        '\u{F900}'..='\u{FAFF}' | // CJK Compatibility Ideographs
        '\u{AC00}'..='\u{D7AF}' | // Hangul Syllables
        '\u{1100}'..='\u{11FF}' | // Hangul Jamo
        '\u{FF00}'..='\u{FFEF}' | // Halfwidth and Fullwidth Forms
        '\u{3000}'..='\u{303F}'   // CJK Symbols and Punctuation
    )
}

/// Returns true if any character in the text slice is CJK.
#[must_use]
pub fn is_cjk_text(s: &str) -> bool {
    s.chars().any(is_cjk_char)
}

/// Joins tokens using appropriate typography rules:
/// - Space-less `""` for CJK scripts (Japanese, Chinese, Korean), with spaces preserved between adjacent ASCII alphanumeric tokens.
/// - Space `" "` for Latin / alphabetic scripts.
#[must_use]
pub fn join_cjk_or_latin_tokens<S: AsRef<str>>(tokens: &[S], is_cjk: bool) -> String {
    if tokens.is_empty() {
        return String::new();
    }
    if !is_cjk {
        return tokens
            .iter()
            .map(|t| t.as_ref())
            .collect::<Vec<_>>()
            .join(" ")
            .trim()
            .to_string();
    }

    let mut result = String::new();
    for (i, token) in tokens.iter().enumerate() {
        let t = token.as_ref().trim();
        if t.is_empty() {
            continue;
        }
        if i > 0 && !result.is_empty() {
            let prev_char = result.chars().last().unwrap_or(' ');
            let next_char = t.chars().next().unwrap_or(' ');
            // Insert space if transitioning between two ASCII alphanumeric tokens
            if prev_char.is_ascii_alphanumeric() && next_char.is_ascii_alphanumeric() {
                result.push(' ');
            }
        }
        result.push_str(t);
    }
    result
}

pub trait OcrBackend: std::fmt::Debug + Send + Sync {
    fn recognize<'a>(&'a self, image: &'a DynamicImage) -> BoxFuture<'a, Result<String>>;

    fn recognize_detailed<'a>(
        &'a self,
        image: &'a DynamicImage,
        orientation: Option<TextOrientation>,
    ) -> BoxFuture<'a, Result<OcrResult>> {
        Box::pin(async move {
            let text = self.recognize(image).await?;
            let orientation = orientation.unwrap_or(TextOrientation::Horizontal);
            Ok(OcrResult {
                text,
                boxes: Vec::new(),
                orientation,
            })
        })
    }
}

/// A mock OCR engine returning configurable static text and boxes (for tests & offline runs).
#[derive(Debug, Clone)]
pub struct MockOcrBackend {
    pub mocked_text: String,
    pub mocked_boxes: Vec<DetectedTextBox>,
    pub mocked_orientation: TextOrientation,
}

impl MockOcrBackend {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            mocked_text: text.into(),
            mocked_boxes: Vec::new(),
            mocked_orientation: TextOrientation::Horizontal,
        }
    }

    #[must_use]
    pub fn with_boxes(mut self, boxes: Vec<DetectedTextBox>) -> Self {
        self.mocked_boxes = boxes;
        self
    }

    #[must_use]
    pub fn with_orientation(mut self, orientation: TextOrientation) -> Self {
        self.mocked_orientation = orientation;
        self
    }

    pub fn new_detailed(
        text: impl Into<String>,
        boxes: Vec<DetectedTextBox>,
        orientation: TextOrientation,
    ) -> Self {
        Self {
            mocked_text: text.into(),
            mocked_boxes: boxes,
            mocked_orientation: orientation,
        }
    }
}

impl OcrBackend for MockOcrBackend {
    fn recognize<'a>(&'a self, _image: &'a DynamicImage) -> BoxFuture<'a, Result<String>> {
        let text = self.mocked_text.clone();
        Box::pin(async move { Ok(text) })
    }

    fn recognize_detailed<'a>(
        &'a self,
        _image: &'a DynamicImage,
        orientation: Option<TextOrientation>,
    ) -> BoxFuture<'a, Result<OcrResult>> {
        let text = self.mocked_text.clone();
        let boxes = self.mocked_boxes.clone();
        let orientation = orientation.unwrap_or(self.mocked_orientation);
        Box::pin(async move {
            Ok(OcrResult {
                text,
                boxes,
                orientation,
            })
        })
    }
}

/// Modern deep-learning OCR backend powered by pure-Rust `ocrs` and `rten`.
pub struct OcrsBackend {
    engine: Arc<OcrEngine>,
    lang: Option<String>,
}

impl std::fmt::Debug for OcrsBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OcrsBackend")
            .field("lang", &self.lang)
            .finish()
    }
}

impl OcrsBackend {
    /// Upstream ocrs publishes ONNX models; the former `.rten` URLs return HTTP 403.
    pub const DETECTION_URL: &'static str =
        "https://ocrs-models.s3-accelerate.amazonaws.com/text-detection.onnx";
    pub const RECOGNITION_URL: &'static str =
        "https://ocrs-models.s3-accelerate.amazonaws.com/text-recognition.onnx";

    const SYSTEM_MODELS_DIR: &'static str = models::SYSTEM_MODELS_DIR;

    pub fn default_models_dir() -> std::path::PathBuf {
        let system_dir = std::path::PathBuf::from(Self::SYSTEM_MODELS_DIR);
        if Self::system_model_paths(&system_dir).is_some() {
            return system_dir;
        }
        models::user_models_dir()
    }

    pub fn from_models(
        detection_model: Model,
        recognition_model: Model,
    ) -> std::result::Result<Self, OcrInitError> {
        let engine = OcrEngine::new(OcrEngineParams {
            detection_model: Some(detection_model),
            recognition_model: Some(recognition_model),
            ..Default::default()
        })
        .map_err(|e| OcrInitError::Engine(e.to_string()))?;

        Ok(Self {
            engine: Arc::new(engine),
            lang: None,
        })
    }

    #[must_use]
    pub fn with_lang(mut self, lang: impl Into<String>) -> Self {
        self.lang = Some(lang.into());
        self
    }

    #[must_use]
    pub fn is_cjk_configured(&self) -> bool {
        self.lang
            .as_deref()
            .map(|l| {
                matches!(
                    l.to_lowercase().as_str(),
                    "ja" | "jpn" | "zh" | "chi" | "ko" | "kor"
                )
            })
            .unwrap_or(false)
    }

    pub const DETECTION: models::ModelSource = models::ModelSource {
        file_name: "text-detection.onnx",
        url: Self::DETECTION_URL,
        legacy_names: &["text-detection.rten"],
        sha256: None,
    };
    pub const RECOGNITION: models::ModelSource = models::ModelSource {
        file_name: "text-recognition.onnx",
        url: Self::RECOGNITION_URL,
        legacy_names: &["text-recognition.rten"],
        sha256: None,
    };

    /// Detection and recognition model paths in `dir`, if both are present in any
    /// accepted format.
    pub fn system_model_paths(
        dir: &std::path::Path,
    ) -> Option<(std::path::PathBuf, std::path::PathBuf)> {
        Some((
            models::find_existing(dir, &Self::DETECTION)?,
            models::find_existing(dir, &Self::RECOGNITION)?,
        ))
    }

    /// Loads the OCR models from the system directory if present, otherwise from the user
    /// models directory, downloading missing or corrupt files.
    pub async fn load_default_or_download() -> std::result::Result<Self, OcrInitError> {
        let system_dir = std::path::PathBuf::from(Self::SYSTEM_MODELS_DIR);
        if let Some((sys_det, sys_rec)) = Self::system_model_paths(&system_dir) {
            tracing::info!("Loading OCR models from system directory: {:?}", system_dir);
            let det_model = models::load_readonly(&sys_det).await?;
            let rec_model = models::load_readonly(&sys_rec).await?;
            return Self::from_models(det_model, rec_model);
        }

        let models_dir = Self::default_models_dir();
        let legacy_dir = std::env::var("HOME").ok().map(|home| {
            std::path::PathBuf::from(home).join(".local/share/cosmic-ext-lingua/models")
        });

        let det_model =
            models::ensure_model(&models_dir, Self::DETECTION, legacy_dir.as_deref()).await?;
        let rec_model =
            models::ensure_model(&models_dir, Self::RECOGNITION, legacy_dir.as_deref()).await?;

        Self::from_models(det_model, rec_model)
    }
}

/// Selects and constructs the OCR engine for a source language code with automatic
/// text direction and engine choice. See [`create_ocr_backend`].
pub async fn create_ocr_backend_for_lang(
    src_lang: &str,
) -> std::result::Result<Arc<dyn OcrBackend>, OcrInitError> {
    create_ocr_backend(src_lang, OrientationMode::Auto, OcrEngineChoice::Auto).await
}

/// Selects and constructs the OCR engine for a source language code.
///
/// - Latin-script languages (and `auto`) use the bundled `ocrs` models.
/// - Japanese and Simplified Chinese use PaddleOCR (models downloaded on first use), then
///   Tesseract if PaddleOCR cannot be loaded. `engine` can force either one.
/// - Korean uses Tesseract.
///
/// If no engine can read the script an error is returned instead of silently using a
/// backend that produces garbage.
///
/// `orientation` forces the text direction. For Tesseract, `Vertical` requires the vertical
/// language pack (e.g. `jpn_vert`); `Auto` uses it when installed.
pub async fn create_ocr_backend(
    src_lang: &str,
    orientation: OrientationMode,
    engine: OcrEngineChoice,
) -> std::result::Result<Arc<dyn OcrBackend>, OcrInitError> {
    let script = script_for_code(src_lang);

    if script.tesseract_pack().is_none() {
        let backend = OcrsBackend::load_default_or_download().await?;
        tracing::info!("Using ocrs OCR backend for source language '{src_lang}'");
        return Ok(Arc::new(backend.with_lang(normalize_source_code(src_lang))));
    }

    let paddle_supported = paddle::supports_script(script);
    match engine {
        OcrEngineChoice::Paddle if paddle_supported => {
            create_paddle_backend(script, orientation).await
        }
        OcrEngineChoice::Auto if paddle_supported => {
            let paddle_err = match create_paddle_backend(script, orientation).await {
                Ok(backend) => return Ok(backend),
                Err(err) => err,
            };
            tracing::warn!("PaddleOCR unavailable, trying Tesseract: {paddle_err}");
            create_tesseract_backend(script, orientation)
                .await
                .map_err(|tesseract_err| OcrInitError::NoEngine {
                    language: script.name(),
                    paddle: paddle_err.to_string(),
                    tesseract: tesseract_err.to_string(),
                })
        }
        OcrEngineChoice::Paddle => {
            tracing::warn!(
                "PaddleOCR does not support {}; using Tesseract",
                script.name()
            );
            create_tesseract_backend(script, orientation).await
        }
        OcrEngineChoice::Auto | OcrEngineChoice::Tesseract => {
            create_tesseract_backend(script, orientation).await
        }
    }
}

async fn create_paddle_backend(
    script: crate::language::Script,
    orientation: OrientationMode,
) -> std::result::Result<Arc<dyn OcrBackend>, OcrInitError> {
    let backend = PaddleOcrBackend::load_default_or_download().await?;
    tracing::info!(
        "Using PaddleOCR backend for {} (mode: {})",
        script.name(),
        orientation.code()
    );
    Ok(Arc::new(backend.with_mode(orientation)))
}

async fn create_tesseract_backend(
    script: crate::language::Script,
    orientation: OrientationMode,
) -> std::result::Result<Arc<dyn OcrBackend>, OcrInitError> {
    let language = script.name();
    let Some(pack) = script.tesseract_pack() else {
        return Err(OcrInitError::Engine(format!(
            "Tesseract is not used for {language} text"
        )));
    };
    let Some(installed) = TesseractBackend::installed_languages().await else {
        return Err(OcrInitError::TesseractMissing { language, pack });
    };
    let has = |p: &str| installed.iter().any(|l| l == p);
    if !has(pack) {
        return Err(OcrInitError::TesseractLanguageMissing { language, pack });
    }

    let mut backend = TesseractBackend::new(pack).with_mode(orientation);
    match script.tesseract_vertical_pack() {
        Some(vert) if has(vert) => backend = backend.with_vertical_lang(vert),
        Some(vert) if orientation == OrientationMode::Vertical => {
            return Err(OcrInitError::TesseractLanguageMissing {
                language,
                pack: vert,
            });
        }
        Some(vert) => tracing::warn!(
            "Tesseract '{vert}' data not installed; vertical {language} text will not be recognized"
        ),
        None => {}
    }
    tracing::info!(
        "Using Tesseract OCR backend for {language} ({pack}, vertical: {:?}, mode: {})",
        backend.vertical_lang,
        orientation.code()
    );
    Ok(Arc::new(backend))
}

/// Shared post-processing for line boxes from any backend: furigana pruning, reading-order
/// sorting, CJK-aware joining and text normalisation.
pub(crate) fn assemble_result(
    raw_boxes: Vec<DetectedTextBox>,
    orientation: Option<TextOrientation>,
    force_cjk: bool,
) -> OcrResult {
    let orientation = orientation.unwrap_or_else(|| detect_dominant_orientation(&raw_boxes));
    let pruned = filter_furigana(raw_boxes, orientation);
    let sorted = sort_cjk_reading_order(pruned, orientation);
    let is_cjk = force_cjk || sorted.iter().any(|b| is_cjk_text(&b.text));
    let tokens: Vec<&str> = sorted.iter().map(|b| b.text.as_str()).collect();
    let text = normalize_cjk_text(&join_cjk_or_latin_tokens(&tokens, is_cjk));
    OcrResult {
        text,
        boxes: sorted,
        orientation,
    }
}

impl OcrBackend for OcrsBackend {
    fn recognize<'a>(&'a self, image: &'a DynamicImage) -> BoxFuture<'a, Result<String>> {
        Box::pin(async move {
            let res = self.recognize_detailed(image, None).await?;
            Ok(res.text)
        })
    }

    fn recognize_detailed<'a>(
        &'a self,
        image: &'a DynamicImage,
        orientation: Option<TextOrientation>,
    ) -> BoxFuture<'a, Result<OcrResult>> {
        use crate::region::Roi;

        let rgb = image.to_rgb8();
        let (width, height) = rgb.dimensions();
        let raw_bytes = rgb.into_raw();
        let engine = Arc::clone(&self.engine);
        let is_cjk_config = self.is_cjk_configured();

        Box::pin(async move {
            tokio::task::spawn_blocking(move || {
                let img_source = ImageSource::from_bytes(&raw_bytes, (width, height))
                    .map_err(|e| anyhow::anyhow!("ImageSource creation error: {e}"))?;

                let ocr_input = engine
                    .prepare_input(img_source)
                    .map_err(|e| anyhow::anyhow!("OCR prepare input failed: {e}"))?;

                let word_rects = engine
                    .detect_words(&ocr_input)
                    .map_err(|e| anyhow::anyhow!("OCR detect words failed: {e}"))?;

                let line_rects = engine.find_text_lines(&ocr_input, &word_rects);

                let line_texts = engine
                    .recognize_text(&ocr_input, &line_rects)
                    .map_err(|e| anyhow::anyhow!("OCR recognize text failed: {e}"))?;

                let mut raw_boxes = Vec::new();
                for line in line_texts.into_iter().flatten() {
                    let text = line.to_string();
                    let trimmed = text.trim();
                    if !trimmed.is_empty() {
                        let rect = line.bounding_rect();
                        let roi = Roi {
                            x: rect.left().max(0) as u32,
                            y: rect.top().max(0) as u32,
                            width: rect.width().max(0) as u32,
                            height: rect.height().max(0) as u32,
                        };
                        raw_boxes.push(DetectedTextBox::new(roi, trimmed, 1.0));
                    }
                }

                Ok(assemble_result(raw_boxes, orientation, is_cjk_config))
            })
            .await
            .map_err(|e| anyhow::anyhow!("Spawn blocking error: {e}"))?
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::region::Roi;
    use image::RgbaImage;

    #[tokio::test]
    async fn test_mock_ocr_backend() {
        let backend = MockOcrBackend::new("Hello World");
        let dummy = DynamicImage::ImageRgba8(RgbaImage::new(10, 10));
        let result = backend.recognize(&dummy).await.unwrap();
        assert_eq!(result, "Hello World");
    }

    #[tokio::test]
    async fn test_mock_ocr_detailed_and_builder() {
        let boxes = vec![
            DetectedTextBox::new(Roi::new(0, 0, 100, 30), "Hello", 0.99),
            DetectedTextBox::new(Roi::new(110, 0, 100, 30), "World", 0.98),
        ];
        let backend = MockOcrBackend::new("Hello World")
            .with_boxes(boxes.clone())
            .with_orientation(TextOrientation::Horizontal);

        let dummy = DynamicImage::ImageRgba8(RgbaImage::new(10, 10));
        let detailed = backend.recognize_detailed(&dummy, None).await.unwrap();

        assert_eq!(detailed.text, "Hello World");
        assert_eq!(detailed.boxes.len(), 2);
        assert_eq!(detailed.orientation, TextOrientation::Horizontal);
    }

    #[test]
    fn test_cjk_character_join_without_ascii_spaces() {
        // Japanese tokens: must be joined without spaces ("")
        let ja_tokens = vec!["私", "は", "学生", "です", "。"];
        let ja_joined = join_cjk_or_latin_tokens(&ja_tokens, true);
        assert_eq!(ja_joined, "私は学生です。");

        // Chinese tokens: must be joined without spaces ("")
        let zh_tokens = vec!["你好", "世界"];
        let zh_joined = join_cjk_or_latin_tokens(&zh_tokens, true);
        assert_eq!(zh_joined, "你好世界");

        // Latin / English tokens: must be joined with ASCII space (" ")
        let en_tokens = vec!["The", "hero", "entered", "the", "dungeon."];
        let en_joined = join_cjk_or_latin_tokens(&en_tokens, false);
        assert_eq!(en_joined, "The hero entered the dungeon.");

        // Mixed CJK + Western tokens: ASCII words retain word space, transition to CJK without space
        let mixed = vec!["Level", "10", "へ"];
        let mixed_joined = join_cjk_or_latin_tokens(&mixed, true);
        assert_eq!(mixed_joined, "Level 10へ");
    }

    #[test]
    fn test_cjk_char_detection() {
        assert!(is_cjk_char('あ')); // Hiragana
        assert!(is_cjk_char('ア')); // Katakana
        assert!(is_cjk_char('漢')); // Kanji
        assert!(is_cjk_char('한')); // Hangul
        assert!(is_cjk_char('。')); // CJK punctuation
        assert!(!is_cjk_char('A')); // Latin
        assert!(!is_cjk_char('1')); // Digit
        assert!(!is_cjk_char('?')); // ASCII punctuation

        assert!(is_cjk_text("レベルアップ!"));
        assert!(!is_cjk_text("Level Up!"));
    }

    #[tokio::test]
    async fn test_ocrs_backend_load() {
        let models_dir = OcrsBackend::default_models_dir();
        if OcrsBackend::system_model_paths(&models_dir).is_none() {
            // Skip when models have not been pre-downloaded to ensure CI offline resilience
            return;
        }
        let backend = OcrsBackend::load_default_or_download().await.unwrap();
        let dummy = DynamicImage::ImageRgba8(RgbaImage::new(50, 50));
        let res = backend.recognize(&dummy).await.unwrap();
        assert_eq!(res, "");
    }
}
