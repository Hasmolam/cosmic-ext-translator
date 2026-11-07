mod crop;
pub mod error;
pub use crop::RoiPixels;
pub mod pipewire_thread;
pub mod screencast;

pub use error::{CaptureError, Result};
use pipewire_thread::{spawn_pipewire_capture_thread, CapturedFrame, PipewireCaptureHandle};
use screencast::{ScreencastManager, ScreencastStreamInfo};
use translator_protocol::{CaptureRegion, Roi};

pub struct CaptureSession {
    pub stream_info: ScreencastStreamInfo,
    pub capture_handle: PipewireCaptureHandle,
    pub frame_receiver: tokio::sync::watch::Receiver<Option<CapturedFrame>>,
}

impl CaptureSession {
    /// Starts a complete capture session:
    /// 1. Prompts for screen/window via XDG Portal (or reuses restore token)
    /// 2. Spawns dedicated PipeWire thread
    /// 3. Returns frame receiver channel and session handle
    pub async fn start(
        restore_token: Option<&str>,
        persist: bool,
        initial_region: impl Into<CaptureRegion>,
    ) -> Result<Self> {
        let region = initial_region.into();
        let stream_info = ScreencastManager::create_stream(restore_token, persist).await?;
        let (capture_handle, frame_receiver) = spawn_pipewire_capture_thread(
            stream_info
                .pipewire_fd
                .try_clone()
                .map_err(|e| CaptureError::PipeWire(e.to_string()))?,
            stream_info.node_id,
            region,
        )?;

        Ok(Self {
            stream_info,
            capture_handle,
            frame_receiver,
        })
    }

    pub fn update_region(&self, region: CaptureRegion) {
        self.capture_handle.update_region(region);
    }

    pub fn update_roi(&self, roi: Option<Roi>) {
        self.capture_handle.update_roi(roi);
    }

    pub fn update_ignore_roi(&self, ignore_roi: Option<Roi>) {
        self.capture_handle.update_ignore_roi(ignore_roi);
    }

    pub fn snapshot_requester(&self) -> pipewire_thread::SnapshotRequester {
        self.capture_handle.snapshot_requester()
    }

    pub async fn get_full_snapshot(&self) -> Option<image::RgbaImage> {
        self.capture_handle.get_full_snapshot().await
    }

    pub fn stop(&mut self) -> Result<()> {
        self.capture_handle.stop()
    }
}

impl std::fmt::Debug for CaptureSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CaptureSession")
            .field("node_id", &self.stream_info.node_id)
            .finish()
    }
}
