use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use anyhow::Result;
use image::DynamicImage;
use ocrs::{ImageSource, OcrEngine, OcrEngineParams, TextItem};
use rten::Model;

pub mod layout;
pub mod furigana;

pub use layout::{
    detect_dominant_orientation, sort_cjk_reading_order, DetectedTextBox, TextOrientation,
};
pub use furigana::{filter_furigana, is_punctuation_char};

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
    pub const DETECTION_URL: &'static str =
        "https://ocrs-models.s3-accelerate.amazonaws.com/text-detection.rten";
    pub const RECOGNITION_URL: &'static str =
        "https://ocrs-models.s3-accelerate.amazonaws.com/text-recognition.rten";

    pub fn default_models_dir() -> std::path::PathBuf {
        let system_dir = std::path::PathBuf::from("/usr/share/cosmic-ext-translator/models");
        if system_dir.join("text-detection.rten").exists()
            && system_dir.join("text-recognition.rten").exists()
        {
            return system_dir;
        }

        if let Ok(data_home) = std::env::var("XDG_DATA_HOME") {
            std::path::PathBuf::from(data_home).join("cosmic-ext-translator/models")
        } else if let Ok(home) = std::env::var("HOME") {
            std::path::PathBuf::from(home).join(".local/share/cosmic-ext-translator/models")
        } else {
            std::env::temp_dir().join("cosmic-ext-translator/models")
        }
    }

    pub fn from_models(detection_model: Model, recognition_model: Model) -> Result<Self> {
        let engine = OcrEngine::new(OcrEngineParams {
            detection_model: Some(detection_model),
            recognition_model: Some(recognition_model),
            ..Default::default()
        })
        .map_err(|e| anyhow::anyhow!("OcrEngine creation error: {e}"))?;

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
            .map(|l| matches!(l.to_lowercase().as_str(), "ja" | "jpn" | "zh" | "chi" | "ko" | "kor"))
            .unwrap_or(false)
    }

    /// Loads the OCR models from the default models directory, downloading them if missing.
    pub async fn load_default_or_download() -> Result<Self> {
        let system_dir = std::path::PathBuf::from("/usr/share/cosmic-ext-translator/models");
        let sys_det = system_dir.join("text-detection.rten");
        let sys_rec = system_dir.join("text-recognition.rten");

        if sys_det.exists() && sys_rec.exists() {
            tracing::info!("Loading OCR models from system directory: {:?}", system_dir);
            let det_model = Model::load_file(&sys_det)?;
            let rec_model = Model::load_file(&sys_rec)?;
            return Self::from_models(det_model, rec_model);
        }

        let models_dir = Self::default_models_dir();
        tokio::fs::create_dir_all(&models_dir).await?;

        let det_path = models_dir.join("text-detection.rten");
        let rec_path = models_dir.join("text-recognition.rten");

        if !det_path.exists() || !rec_path.exists() {
            if let Ok(home) = std::env::var("HOME") {
                let legacy_dir =
                    std::path::PathBuf::from(home).join(".local/share/cosmic-ext-lingua/models");
                let leg_det = legacy_dir.join("text-detection.rten");
                let leg_rec = legacy_dir.join("text-recognition.rten");
                if leg_det.exists() && leg_rec.exists() {
                    let _ = tokio::fs::copy(&leg_det, &det_path).await;
                    let _ = tokio::fs::copy(&leg_rec, &rec_path).await;
                }
            }
        }

        let det_valid = det_path
            .metadata()
            .map(|m| m.len() >= 100_000)
            .unwrap_or(false);
        if !det_valid {
            tracing::info!("Downloading OCR detection model...");
            let resp = reqwest::get(Self::DETECTION_URL)
                .await?
                .error_for_status()?
                .bytes()
                .await?;
            if resp.len() < 100_000 {
                anyhow::bail!(
                    "Downloaded detection model is corrupt or invalid (size: {} bytes)",
                    resp.len()
                );
            }
            tokio::fs::write(&det_path, resp).await?;
        }

        let rec_valid = rec_path
            .metadata()
            .map(|m| m.len() >= 100_000)
            .unwrap_or(false);
        if !rec_valid {
            tracing::info!("Downloading OCR recognition model...");
            let resp = reqwest::get(Self::RECOGNITION_URL)
                .await?
                .error_for_status()?
                .bytes()
                .await?;
            if resp.len() < 100_000 {
                anyhow::bail!(
                    "Downloaded recognition model is corrupt or invalid (size: {} bytes)",
                    resp.len()
                );
            }
            tokio::fs::write(&rec_path, resp).await?;
        }

        let det_model = Model::load_file(&det_path)?;
        let rec_model = Model::load_file(&rec_path)?;

        Self::from_models(det_model, rec_model)
    }
}

/// Tesseract OCR backend for extended language support (CJK: Japanese, Chinese, Korean).
#[derive(Debug, Clone)]
pub struct TesseractBackend {
    pub lang: String,
}

impl TesseractBackend {
    pub fn new(lang: impl Into<String>) -> Self {
        Self {
            lang: lang.into(),
        }
    }

    pub fn is_available() -> bool {
        std::process::Command::new("tesseract")
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }
}

impl OcrBackend for TesseractBackend {
    fn recognize<'a>(&'a self, image: &'a DynamicImage) -> BoxFuture<'a, Result<String>> {
        let lang = self.lang.clone();
        let rgb = image.to_rgb8();
        Box::pin(async move {
            let mut cursor = std::io::Cursor::new(Vec::new());
            rgb.write_to(&mut cursor, image::ImageFormat::Png)
                .map_err(|e| anyhow::anyhow!("Failed to encode image to PNG for Tesseract: {e}"))?;
            let png_bytes = cursor.into_inner();

            let mut child = tokio::process::Command::new("tesseract")
                .arg("stdin")
                .arg("stdout")
                .arg("-l")
                .arg(&lang)
                .stdin(std::process::Stdio::piped())
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::null())
                .spawn()
                .map_err(|e| anyhow::anyhow!("Failed to spawn tesseract: {e}"))?;

            if let Some(mut stdin) = child.stdin.take() {
                use tokio::io::AsyncWriteExt;
                stdin.write_all(&png_bytes).await?;
            }

            let output = child.wait_with_output().await?;
            if !output.status.success() {
                anyhow::bail!("Tesseract exited with non-zero status");
            }

            let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
            Ok(text)
        })
    }
}

/// Automatically selects and instantiates the optimal OCR engine for a given source language.
pub async fn create_ocr_backend_for_lang(src_lang: &str) -> Result<Arc<dyn OcrBackend>> {
    let lower = src_lang.to_lowercase();
    let is_cjk = matches!(lower.as_str(), "ja" | "jpn" | "zh" | "chi" | "ko" | "kor");

    if is_cjk && TesseractBackend::is_available() {
        let tesseract_lang = match lower.as_str() {
            "ja" | "jpn" => "jpn",
            "zh" | "chi" => "chi_sim",
            "ko" | "kor" => "kor",
            other => other,
        };
        tracing::info!("Using Tesseract OCR backend for CJK language: {tesseract_lang}");
        return Ok(Arc::new(TesseractBackend::new(tesseract_lang)));
    }

    match OcrsBackend::load_default_or_download().await {
        Ok(backend) => Ok(Arc::new(backend.with_lang(lower))),
        Err(err) => {
            tracing::warn!("Failed to load OcrsBackend, falling back to MockOcrBackend: {err}");
            Ok(Arc::new(MockOcrBackend::new("Capture Active")))
        }
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
        use crate::capture::roi::Roi;

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

                let dominant_ori = orientation.unwrap_or_else(|| {
                    detect_dominant_orientation(&raw_boxes)
                });

                // 1. Furigana geometric pruning
                let pruned_boxes = filter_furigana(raw_boxes, dominant_ori);

                // 2. Reading order topological sorting
                let sorted_boxes = sort_cjk_reading_order(pruned_boxes, dominant_ori);

                // 3. Space-less (CJK) vs spaced (Latin) token joining
                let is_cjk = is_cjk_config || sorted_boxes.iter().any(|b| is_cjk_text(&b.text));
                let token_strs: Vec<&str> = sorted_boxes.iter().map(|b| b.text.as_str()).collect();
                let joined_text = join_cjk_or_latin_tokens(&token_strs, is_cjk);

                Ok(OcrResult {
                    text: joined_text,
                    boxes: sorted_boxes,
                    orientation: dominant_ori,
                })
            })
            .await
            .map_err(|e| anyhow::anyhow!("Spawn blocking error: {e}"))?
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capture::roi::Roi;
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
        let det_path = models_dir.join("text-detection.rten");
        let rec_path = models_dir.join("text-recognition.rten");
        let det_size = det_path.metadata().map(|m| m.len()).unwrap_or(0);
        let rec_size = rec_path.metadata().map(|m| m.len()).unwrap_or(0);
        if det_size < 100_000 || rec_size < 100_000 {
            // Skip when models have not been pre-downloaded to ensure CI offline resilience
            return;
        }
        let backend = OcrsBackend::load_default_or_download().await.unwrap();
        let dummy = DynamicImage::ImageRgba8(RgbaImage::new(50, 50));
        let res = backend.recognize(&dummy).await.unwrap();
        assert_eq!(res, "");
    }
}
