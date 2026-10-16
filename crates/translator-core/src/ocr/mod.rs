use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use anyhow::Result;
use image::DynamicImage;
use ocrs::{ImageSource, OcrEngine, OcrEngineParams};
use rten::Model;

pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

pub trait OcrBackend: Send + Sync {
    fn recognize<'a>(&'a self, image: &'a DynamicImage) -> BoxFuture<'a, Result<String>>;
}

/// A mock OCR engine returning configurable static text (for tests & offline runs).
#[derive(Debug, Clone)]
pub struct MockOcrBackend {
    pub mocked_text: String,
}

impl MockOcrBackend {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            mocked_text: text.into(),
        }
    }
}

impl OcrBackend for MockOcrBackend {
    fn recognize<'a>(&'a self, _image: &'a DynamicImage) -> BoxFuture<'a, Result<String>> {
        let text = self.mocked_text.clone();
        Box::pin(async move { Ok(text) })
    }
}

/// Modern deep-learning OCR backend powered by pure-Rust `ocrs` and `rten`.
pub struct OcrsBackend {
    engine: Arc<OcrEngine>,
}

impl OcrsBackend {
    pub fn from_models(detection_model: Model, recognition_model: Model) -> Result<Self> {
        let engine = OcrEngine::new(OcrEngineParams {
            detection_model: Some(detection_model),
            recognition_model: Some(recognition_model),
            ..Default::default()
        })
        .map_err(|e| anyhow::anyhow!("OcrEngine creation error: {e}"))?;

        Ok(Self {
            engine: Arc::new(engine),
        })
    }
}

impl OcrBackend for OcrsBackend {
    fn recognize<'a>(&'a self, image: &'a DynamicImage) -> BoxFuture<'a, Result<String>> {
        let rgb = image.to_rgb8();
        let (width, height) = rgb.dimensions();
        let raw_bytes = rgb.into_raw();
        let engine = Arc::clone(&self.engine);

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

                let text = line_texts
                    .into_iter()
                    .filter_map(|line| line.map(|l| l.to_string()))
                    .collect::<Vec<_>>()
                    .join(" ");

                Ok(text.trim().to_string())
            })
            .await
            .map_err(|e| anyhow::anyhow!("Spawn blocking error: {e}"))?
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::RgbaImage;

    #[tokio::test]
    async fn test_mock_ocr_backend() {
        let backend = MockOcrBackend::new("Hello World");
        let dummy = DynamicImage::ImageRgba8(RgbaImage::new(10, 10));
        let result = backend.recognize(&dummy).await.unwrap();
        assert_eq!(result, "Hello World");
    }
}
