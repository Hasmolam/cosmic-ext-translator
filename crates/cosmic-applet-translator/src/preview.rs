//! Live inspection images for the settings dashboard: the cropped capture region and
//! the same region after OCR pre-processing.

use std::hash::{Hash, Hasher};
use std::time::Duration;

use cosmic::iced::core::image::Handle;
use image::RgbaImage;
use tokio::sync::watch;
use tracing::debug;
use translator_core::capture::pipewire_thread::CapturedFrame;
use translator_core::pipeline::preprocess::{preprocess_for_ocr, PreprocessConfig};

/// Minimum interval between preview refreshes. Capture runs at screen refresh rate;
/// the dashboard does not need more than a few updates per second.
const PREVIEW_INTERVAL: Duration = Duration::from_millis(250);

#[derive(Debug, Clone)]
pub struct PreviewImages {
    pub raw: Handle,
    pub preprocessed: Handle,
}

/// Builds both preview images. `None` for an empty frame.
#[must_use]
pub fn build(image: &RgbaImage, config: &PreprocessConfig) -> Option<PreviewImages> {
    let (w, h) = image.dimensions();
    if w == 0 || h == 0 {
        return None;
    }
    let raw = Handle::from_rgba(w, h, image.clone().into_raw());

    let gray = preprocess_for_ocr(image, config);
    let (gw, gh) = gray.dimensions();
    let rgba: Vec<u8> = gray
        .into_raw()
        .into_iter()
        .flat_map(|v| [v, v, v, 255])
        .collect();
    let preprocessed = Handle::from_rgba(gw, gh, rgba);

    Some(PreviewImages { raw, preprocessed })
}

/// Input of the preview subscription. Identified by the capture session and the
/// pre-processing settings, so a new session or changed settings restart the stream.
pub struct PreviewSource {
    pub session_id: u64,
    pub binarize: bool,
    pub blur_subtraction: bool,
    pub frames: watch::Receiver<Option<CapturedFrame>>,
}

impl Hash for PreviewSource {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.session_id.hash(state);
        self.binarize.hash(state);
        self.blur_subtraction.hash(state);
    }
}

/// Emits preview images for new capture frames, at most once per `PREVIEW_INTERVAL`.
pub fn subscription<M: Send + 'static>(
    source: PreviewSource,
    wrap: fn(PreviewImages) -> M,
) -> cosmic::iced::Subscription<M> {
    cosmic::iced::Subscription::run_with((source, wrap), |(source, wrap)| {
        let mut frames = source.frames.clone();
        let wrap = *wrap;
        let config = PreprocessConfig {
            binarize: source.binarize,
            blur_subtraction: source.blur_subtraction,
            ..Default::default()
        };
        cosmic::iced::stream::channel(
            1,
            move |mut output: cosmic::iced::futures::channel::mpsc::Sender<M>| async move {
                use cosmic::iced::futures::SinkExt;
                while frames.changed().await.is_ok() {
                    let Some(image) = frames.borrow_and_update().as_ref().map(|f| f.image.clone())
                    else {
                        continue;
                    };
                    let config = config.clone();
                    let built = tokio::task::spawn_blocking(move || build(&image, &config)).await;
                    match built {
                        Ok(Some(images)) => {
                            if output.send(wrap(images)).await.is_err() {
                                break;
                            }
                        }
                        Ok(None) => {}
                        Err(err) => debug!("Preview build task failed: {err}"),
                    }
                    tokio::time::sleep(PREVIEW_INTERVAL).await;
                }
            },
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_frame_has_no_preview() {
        assert!(build(&RgbaImage::new(0, 0), &PreprocessConfig::default()).is_none());
    }

    #[test]
    fn builds_both_images() {
        let image = RgbaImage::from_pixel(40, 20, image::Rgba([200, 200, 200, 255]));
        assert!(build(&image, &PreprocessConfig::default()).is_some());
    }
}
