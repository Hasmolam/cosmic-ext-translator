pub mod debouncer;
pub mod phash;
pub mod preprocess;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;
use image::DynamicImage;
use tokio::sync::{mpsc, watch};
use tracing::{debug, error, info};

use crate::capture::pipewire_thread::CapturedFrame;
use crate::ocr::{OcrBackend, TextOrientation};
pub use debouncer::{DebounceState, FrameDebouncer, SettledFrame, TextDiffAction, TextGate};
use preprocess::{preprocess_for_ocr, PreprocessConfig};

#[derive(Debug, Clone)]
pub struct RecognizedEvent {
    pub text: String,
    pub timestamp: Instant,
    pub hash: u64,
    /// Text direction the OCR backend used or detected for this frame.
    pub orientation: TextOrientation,
}

pub struct PipelineHandle {
    stop_flag: Arc<AtomicBool>,
    task_handle: Option<tokio::task::JoinHandle<()>>,
}

impl PipelineHandle {
    pub fn stop(&mut self) {
        self.stop_flag.store(true, Ordering::SeqCst);
        if let Some(task) = self.task_handle.take() {
            task.abort();
        }
    }
}

impl Drop for PipelineHandle {
    fn drop(&mut self) {
        self.stop();
    }
}

pub fn spawn_recognition_pipeline(
    mut frame_receiver: watch::Receiver<Option<CapturedFrame>>,
    mut debouncer: FrameDebouncer,
    preprocess_config: PreprocessConfig,
    ocr_backend: Arc<dyn OcrBackend>,
) -> (PipelineHandle, mpsc::Receiver<RecognizedEvent>) {
    let (event_sender, event_receiver) = mpsc::channel(32);
    let stop_flag = Arc::new(AtomicBool::new(false));
    let stop_clone = Arc::clone(&stop_flag);

    let task_handle = tokio::spawn(async move {
        info!("Recognition pipeline started");

        while !stop_clone.load(Ordering::Relaxed) {
            let settled = if debouncer.state() == debouncer::DebounceState::Animating {
                // If text is animating, wait for either a new frame OR settle timeout.
                // This prevents zero-damage / 0-FPS game engine freezes (e.g. Ren'Py, RPG Maker)
                // where the text has stopped typing but the engine produces no further frames to trigger watch.
                tokio::select! {
                    res = frame_receiver.changed() => {
                        if res.is_err() {
                            break;
                        }
                        let frame = frame_receiver.borrow_and_update().clone();
                        let Some(captured) = frame else {
                            continue;
                        };
                        debouncer.process_frame(captured)
                    }
                    _ = tokio::time::sleep(debouncer.settle_duration) => {
                        debug!("Pipeline: zero-FPS static screen timeout reached, forcing settle");
                        debouncer.force_settle()
                    }
                }
            } else {
                if frame_receiver.changed().await.is_err() {
                    break;
                }
                let frame = frame_receiver.borrow_and_update().clone();
                let Some(captured) = frame else {
                    continue;
                };
                debouncer.process_frame(captured)
            };

            let Some(settled) = settled else {
                continue;
            };

            debug!("Pipeline: frame settled, running pre-processing and OCR");

            // 2. Game-optimized image pre-processing
            let processed_gray = preprocess_for_ocr(&settled.frame.image, &preprocess_config);
            let dynamic_img = DynamicImage::ImageLuma8(processed_gray);

            // 3. OCR Recognition
            match ocr_backend.recognize_detailed(&dynamic_img, None).await {
                Ok(result) => {
                    let trimmed = result.text.trim();
                    if !trimmed.is_empty() {
                        debug!(
                            "Pipeline OCR recognized ({:?}): \"{trimmed}\"",
                            result.orientation
                        );
                        let event = RecognizedEvent {
                            text: trimmed.to_string(),
                            timestamp: Instant::now(),
                            hash: settled.hash,
                            orientation: result.orientation,
                        };
                        let _ = event_sender.send(event).await;
                    }
                }
                Err(err) => {
                    error!("Pipeline OCR failed: {err}");
                }
            }
        }

        info!("Recognition pipeline stopped");
    });

    (
        PipelineHandle {
            stop_flag,
            task_handle: Some(task_handle),
        },
        event_receiver,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use crate::ocr::MockOcrBackend;
    use image::{Rgba, RgbaImage};

    fn make_frame_with_blocks(num_blocks: u32) -> CapturedFrame {
        let mut img = RgbaImage::from_pixel(100, 50, Rgba([0, 0, 0, 255]));
        for i in 0..num_blocks {
            let start_x = (i * 12).min(90);
            for y in 10..40 {
                for x in start_x..(start_x + 8).min(100) {
                    img.put_pixel(x, y, Rgba([255, 255, 255, 255]));
                }
            }
        }
        CapturedFrame {
            image: img,
            timestamp: Instant::now(),
            original_width: 100,
            original_height: 50,
        }
    }

    #[tokio::test]
    async fn test_end_to_end_pipeline() {
        let (frame_sender, frame_receiver) = watch::channel(None);
        let debouncer = FrameDebouncer::new(
            2,
            Duration::from_millis(50),
            Duration::from_millis(500),
        );
        let ocr_backend = Arc::new(MockOcrBackend::new("Test Dialogue"));

        let (mut handle, mut event_receiver) = spawn_recognition_pipeline(
            frame_receiver,
            debouncer,
            PreprocessConfig::default(),
            ocr_backend,
        );

        // 1. Send baseline frame (0 blocks)
        frame_sender.send(Some(make_frame_with_blocks(0))).unwrap();
        tokio::time::sleep(Duration::from_millis(10)).await;

        // 2. Send dialogue frame (2 blocks -> triggers Animating)
        frame_sender.send(Some(make_frame_with_blocks(2))).unwrap();

        // 3. Wait settle duration (50ms)
        tokio::time::sleep(Duration::from_millis(70)).await;

        // 4. Send same frame (text stopped typing -> triggers Settled -> triggers OCR)
        frame_sender.send(Some(make_frame_with_blocks(2))).unwrap();

        // 5. Check if event was received
        let event = tokio::time::timeout(Duration::from_millis(500), event_receiver.recv()).await;
        assert!(event.is_ok(), "Pipeline should emit recognized event within timeout");
        let recognized = event.unwrap().unwrap();
        assert_eq!(recognized.text, "Test Dialogue");

        handle.stop();
    }
}
